//! Plugin system for BetterNTE.
//!
//! Supports three plugin types:
//! - **JS**: Runs in an isolated QuickJS runtime
//! - **WASM**: WebAssembly plugins via wasmtime
//! - **FFI**: Native dynamic library plugins via libloading
//!
//! Plugins are discovered from `data/plugins/{plugin-id}/` directories.
//! Each plugin has a `manifest.json` and an entry file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use libloading::{Library, Symbol};
use serde::{Deserialize, Serialize};

// ━━━ Manifest ━━━

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "type")]
    pub plugin_type: PluginType,
    pub entry: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginType {
    Js,
    Wasm,
    Ffi,
}

// ━━━ Plugin info (returned to JS) ━━━

/// Info about a loaded plugin, returned to scripts via `ctx.pluginList()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(rename = "type")]
    pub plugin_type: String,
    pub methods: Vec<String>,
}

// ━━━ Plugin trait ━━━

/// Trait for all plugin implementations.
///
/// Implementations must be `Send + Sync` because the plugin registry
/// is shared across async tasks.
pub trait Plugin: Send + Sync {
    /// Return plugin metadata including available methods.
    fn info(&self) -> PluginInfo;

    /// Call a method on this plugin with JSON arguments.
    ///
    /// `args` is a `Vec<Value>` where each element is one positional argument.
    /// Returns a JSON `Value` result.
    fn call(&self, method: &str, args: Vec<serde_json::Value>) -> Result<serde_json::Value>;
}

// ━━━ JS Plugin ━━━

/// JS plugin — runs in an isolated synchronous QuickJS runtime.
///
/// The plugin JS file should export an object with methods:
/// ```js
/// module.exports = {
///     greet: function(name) { return "Hello, " + name + "!"; },
///     add: function(a, b) { return a + b; },
/// };
/// ```
pub struct JsPlugin {
    manifest: PluginManifest,
    methods: Vec<String>,
    /// The isolated QuickJS runtime + context, wrapped in a Mutex for thread safety.
    /// Each `call()` locks the mutex, enters the context, invokes the method, and returns.
    inner: Mutex<JsPluginInner>,
}

struct JsPluginInner {
    runtime: rquickjs::Runtime,
    context: rquickjs::Context,
}

impl JsPlugin {
    pub fn new(manifest: PluginManifest, entry_path: &Path) -> Result<Self> {
        use rquickjs::{Context, Runtime};

        let source = std::fs::read_to_string(entry_path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to read plugin entry '{}': {}",
                entry_path.display(),
                e
            )
        })?;

        let rt = Runtime::new()?;
        rt.set_max_stack_size(4 * 1024 * 1024); // 4 MB stack
        rt.set_memory_limit(64 * 1024 * 1024); // 64 MB memory limit

        let ctx = Context::full(&rt)?;

        // Evaluate the plugin source and extract exported methods
        let methods = {
            let guard = ctx.acquire();
            let js_ctx: rquickjs::Ctx<'_> = guard;

            // Set up module.exports and exports globals
            js_ctx.eval::<(), _>(
                r#"
                var module = { exports: {} };
                var exports = module.exports;
                "#,
            )?;

            // Evaluate the plugin source code
            js_ctx.eval::<(), _>(&source)?;

            // Get the exported object
            let exports: rquickjs::Object = js_ctx.eval("module.exports")?;

            // Collect method names
            let mut methods = Vec::new();
            for prop in exports.props::<String, rquickjs::Value>() {
                let (key, val) = prop?;
                if val.is_function() {
                    methods.push(key);
                }
            }
            methods.sort();
            methods
        };

        tracing::info!(
            "JS plugin '{}' loaded with methods: {:?}",
            manifest.name,
            methods
        );

        Ok(Self {
            manifest,
            methods,
            inner: Mutex::new(JsPluginInner {
                runtime: rt,
                context: ctx,
            }),
        })
    }
}

impl Plugin for JsPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: self.manifest.id.clone(),
            name: self.manifest.name.clone(),
            version: self.manifest.version.clone(),
            description: self.manifest.description.clone(),
            plugin_type: "js".to_string(),
            methods: self.methods.clone(),
        }
    }

    fn call(&self, method: &str, args: Vec<serde_json::Value>) -> Result<serde_json::Value> {
        if !self.methods.contains(&method.to_string()) {
            return Err(anyhow::anyhow!(
                "Plugin '{}' has no method '{}'",
                self.manifest.id,
                method
            ));
        }

        let inner = self
            .inner
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        let guard = inner.context.acquire();
        let js_ctx: rquickjs::Ctx<'_> = guard;

        // Serialize args to JSON string, then parse in JS
        let args_json = serde_json::to_string(&args)?;
        let call_code = format!(
            r#"
            (function() {{
                var args = JSON.parse({args_json:?});
                var fn = module.exports[{method:?}];
                if (typeof fn !== "function") {{
                    throw new Error("Method not found: {method}");
                }}
                var result = fn.apply(null, args);
                return JSON.stringify(result !== undefined ? result : null);
            }})()
            "#,
            args_json = args_json,
            method = method,
        );

        let result_str: String = js_ctx.eval(&call_code).map_err(|e| {
            anyhow::anyhow!(
                "Plugin '{}' method '{}' error: {}",
                self.manifest.id,
                method,
                e
            )
        })?;

        // Parse the JSON result string back to Value
        let value: serde_json::Value =
            serde_json::from_str(&result_str).unwrap_or(serde_json::Value::Null);
        Ok(value)
    }
}

// ━━━ WASM Plugin ━━━

/// WASM plugin — loads a WebAssembly module via wasmtime.
///
/// The WASM module must export:
/// - `memory` — standard WASM linear memory
/// - `__alloc(size: i32) -> i32` — allocate bytes in linear memory
/// - `__plugin_info() -> i64` — return plugin info JSON (ptr<<32 | len)
/// - `__plugin_call(name_ptr, name_len, args_ptr, args_len) -> i64` — call a method
///
/// All strings are UTF-8 encoded in WASM linear memory.
/// Args and return values are JSON strings.
pub struct WasmPlugin {
    manifest: PluginManifest,
    engine: wasmtime::Engine,
    module: wasmtime::Module,
    methods: Vec<String>,
}

impl WasmPlugin {
    pub fn new(manifest: PluginManifest, entry_path: &Path) -> Result<Self> {
        let wasm_bytes = std::fs::read(entry_path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to read WASM entry '{}': {}",
                entry_path.display(),
                e
            )
        })?;

        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::new(&engine, &wasm_bytes)?;

        // Instantiate temporarily to read plugin info
        let mut store = wasmtime::Store::new(&engine, ());
        let instance = wasmtime::Linker::new(&engine).instantiate(&mut store, &module)?;

        // Get exported memory
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("WASM module must export 'memory'"))?;

        // Call __plugin_info to discover available methods
        let info_func = instance.get_typed_func::<(), i64>(&mut store, "__plugin_info")?;
        let info_result = info_func.call(&mut store, ())?;

        let ptr = (info_result >> 32) as i32 as usize;
        let len = (info_result & 0xFFFF_FFFF) as i32 as usize;

        let mem_data = memory.data(&store);
        if ptr + len > mem_data.len() {
            return Err(anyhow::anyhow!(
                "__plugin_info returned out-of-bounds pointer: ptr={} len={} mem_size={}",
                ptr,
                len,
                mem_data.len()
            ));
        }
        let info_str = std::str::from_utf8(&mem_data[ptr..ptr + len])?;

        #[derive(Deserialize)]
        struct WasmPluginInfo {
            #[serde(default)]
            methods: Vec<String>,
        }

        let wasm_info: WasmPluginInfo = serde_json::from_str(info_str)?;

        tracing::info!(
            "WASM plugin '{}' loaded with methods: {:?}",
            manifest.name,
            wasm_info.methods
        );

        Ok(Self {
            manifest,
            engine,
            module,
            methods: wasm_info.methods,
        })
    }

    /// Execute a single WASM call in a fresh Store/Instance.
    ///
    /// Creates a new Store per call so WasmPlugin can be Send + Sync
    /// (wasmtime::Store is not Send by default).
    fn call_wasm(&self, method: &str, args: Vec<serde_json::Value>) -> Result<serde_json::Value> {
        let mut store = wasmtime::Store::new(&self.engine, ());
        let instance = wasmtime::Linker::new(&self.engine).instantiate(&mut store, &self.module)?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("WASM module must export 'memory'"))?;

        let alloc = instance.get_typed_func::<i32, i32>(&mut store, "__alloc")?;

        // Write method name into WASM linear memory
        let method_bytes = method.as_bytes();
        let method_ptr = alloc.call(&mut store, method_bytes.len() as i32)? as usize;
        memory.data_mut(&mut store)[method_ptr..method_ptr + method_bytes.len()]
            .copy_from_slice(method_bytes);

        // Serialize and write args JSON into WASM linear memory
        let args_json = serde_json::to_string(&args)?;
        let args_bytes = args_json.as_bytes();
        let args_ptr = alloc.call(&mut store, args_bytes.len() as i32)? as usize;
        memory.data_mut(&mut store)[args_ptr..args_ptr + args_bytes.len()]
            .copy_from_slice(args_bytes);

        // Call __plugin_call
        let call_func =
            instance.get_typed_func::<(i32, i32, i32, i32), i64>(&mut store, "__plugin_call")?;
        let result = call_func.call(
            &mut store,
            (
                method_ptr as i32,
                method_bytes.len() as i32,
                args_ptr as i32,
                args_bytes.len() as i32,
            ),
        )?;

        // Read result from WASM linear memory
        let result_ptr = (result >> 32) as i32 as usize;
        let result_len = (result & 0xFFFF_FFFF) as i32 as usize;

        let mem_data = memory.data(&store);
        if result_ptr + result_len > mem_data.len() {
            return Err(anyhow::anyhow!(
                "__plugin_call returned out-of-bounds pointer: ptr={} len={} mem_size={}",
                result_ptr,
                result_len,
                mem_data.len()
            ));
        }
        let result_str = std::str::from_utf8(&mem_data[result_ptr..result_ptr + result_len])?;

        serde_json::from_str(result_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse WASM result as JSON: {}", e))
    }
}

impl Plugin for WasmPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: self.manifest.id.clone(),
            name: self.manifest.name.clone(),
            version: self.manifest.version.clone(),
            description: self.manifest.description.clone(),
            plugin_type: "wasm".to_string(),
            methods: self.methods.clone(),
        }
    }

    fn call(&self, method: &str, args: Vec<serde_json::Value>) -> Result<serde_json::Value> {
        if !self.methods.contains(&method.to_string()) {
            return Err(anyhow::anyhow!(
                "Method '{}' not found in WASM plugin '{}'. Available: {:?}",
                method,
                self.manifest.id,
                self.methods
            ));
        }
        self.call_wasm(method, args)
    }
}

// ━━━ FFI Plugin ━━━

/// FFI plugin — loads a native dynamic library (.dll / .so / .dylib).
///
/// The library must export:
/// - `__plugin_info() -> *const c_char` — returns plugin info as JSON
/// - `__plugin_call(name: *const c_char, args_json: *const c_char) -> *const c_char` — call a method
/// - `__plugin_free(ptr: *const c_char)` (optional) — free returned strings
///
/// All returned strings are null-terminated C strings owned by the plugin.
/// If `__plugin_free` is not exported, static buffers are assumed.

// Type aliases for the C ABI function pointers
type InfoFn = unsafe extern "C" fn() -> *const std::ffi::c_char;
type CallFn = unsafe extern "C" fn(
    *const std::ffi::c_char,
    *const std::ffi::c_char,
) -> *const std::ffi::c_char;
type FreeFn = unsafe extern "C" fn(*const std::ffi::c_char);

pub struct FfiPlugin {
    manifest: PluginManifest,
    /// Keep the dynamic library loaded for the lifetime of the plugin.
    _lib: Arc<Library>,
    methods: Vec<String>,
    /// Cached function pointers (valid as long as `_lib` is alive).
    info_fn: InfoFn,
    call_fn: CallFn,
    free_fn: Option<FreeFn>,
}

impl FfiPlugin {
    pub fn new(manifest: PluginManifest, entry_path: &Path) -> Result<Self> {
        unsafe {
            let lib = Library::new(entry_path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to load FFI plugin library '{}': {}",
                    entry_path.display(),
                    e
                )
            })?;
            let lib = Arc::new(lib);

            // Get required __plugin_info function
            let info_fn: Symbol<InfoFn> = lib.get(b"__plugin_info").map_err(|e| {
                anyhow::anyhow!("FFI plugin missing '__plugin_info' export: {}", e)
            })?;
            let info_fn = *info_fn;

            // Get required __plugin_call function
            let call_fn: Symbol<CallFn> = lib.get(b"__plugin_call").map_err(|e| {
                anyhow::anyhow!("FFI plugin missing '__plugin_call' export: {}", e)
            })?;
            let call_fn = *call_fn;

            // Get optional __plugin_free function
            let free_fn: Option<FreeFn> = lib.get(b"__plugin_free").ok().map(|s: Symbol<FreeFn>| *s);

            // Call __plugin_info to discover available methods
            let info_ptr = info_fn();
            if info_ptr.is_null() {
                return Err(anyhow::anyhow!("__plugin_info returned null"));
            }
            let info_cstr = std::ffi::CStr::from_ptr(info_ptr);
            let info_str = info_cstr.to_str().map_err(|e| {
                anyhow::anyhow!("__plugin_info returned invalid UTF-8: {}", e)
            })?;

            #[derive(Deserialize)]
            struct FfiPluginInfo {
                #[serde(default)]
                methods: Vec<String>,
            }

            let ffi_info: FfiPluginInfo = serde_json::from_str(info_str)?;

            // Free the info string if a free function is available
            if let Some(free) = free_fn {
                free(info_ptr);
            }

            tracing::info!(
                "FFI plugin '{}' loaded with methods: {:?}",
                manifest.name,
                ffi_info.methods
            );

            Ok(Self {
                manifest,
                _lib: lib,
                methods: ffi_info.methods,
                info_fn,
                call_fn,
                free_fn,
            })
        }
    }
}

impl Plugin for FfiPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: self.manifest.id.clone(),
            name: self.manifest.name.clone(),
            version: self.manifest.version.clone(),
            description: self.manifest.description.clone(),
            plugin_type: "ffi".to_string(),
            methods: self.methods.clone(),
        }
    }

    fn call(&self, method: &str, args: Vec<serde_json::Value>) -> Result<serde_json::Value> {
        if !self.methods.contains(&method.to_string()) {
            return Err(anyhow::anyhow!(
                "Method '{}' not found in FFI plugin '{}'. Available: {:?}",
                method,
                self.manifest.id,
                self.methods
            ));
        }

        unsafe {
            let name_cstr = std::ffi::CString::new(method)?;
            let args_json = serde_json::to_string(&args)?;
            let args_cstr = std::ffi::CString::new(args_json)?;

            let result_ptr = (self.call_fn)(name_cstr.as_ptr(), args_cstr.as_ptr());

            if result_ptr.is_null() {
                return Err(anyhow::anyhow!(
                    "__plugin_call returned null for method '{}'",
                    method
                ));
            }

            let result_cstr = std::ffi::CStr::from_ptr(result_ptr);
            let result_str = result_cstr.to_str().map_err(|e| {
                anyhow::anyhow!(
                    "__plugin_call returned invalid UTF-8 for method '{}': {}",
                    method,
                    e
                )
            })?;

            let result: serde_json::Value = serde_json::from_str(result_str)?;

            // Free the result if a free function is available
            if let Some(free) = self.free_fn {
                free(result_ptr);
            }

            Ok(result)
        }
    }
}

// SAFETY: The Library is kept alive via Arc, and the cached function pointers
// are valid as long as the library is loaded (i.e., for the lifetime of FfiPlugin).
// The C ABI functions are assumed to be safe to call from any thread.
unsafe impl Send for FfiPlugin {}
unsafe impl Sync for FfiPlugin {}

// ━━━ Plugin Registry ━━━

/// Registry that holds all loaded plugins.
///
/// Plugins are loaded from `data/plugins/` directories during engine startup.
/// Each plugin subdirectory must contain a `manifest.json` and the entry file
/// specified in the manifest.
pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn Plugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// Scan data root directories for plugins, loading all that are found.
    ///
    /// Looks for `plugins/{plugin-id}/manifest.json` in each data root.
    /// Later roots can override earlier ones (same plugin id).
    pub fn load_from_dirs(&mut self, data_roots: &[PathBuf]) -> Result<()> {
        for root in data_roots {
            let plugins_dir = root.join("plugins");
            if !plugins_dir.is_dir() {
                continue;
            }

            let entries = match std::fs::read_dir(&plugins_dir) {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("Failed to read plugins dir {:?}: {}", plugins_dir, e);
                    continue;
                }
            };

            for entry in entries {
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!("Failed to read plugins entry: {}", e);
                        continue;
                    }
                };
                if !entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                    continue;
                }

                let manifest_path = entry.path().join("manifest.json");
                if !manifest_path.exists() {
                    continue;
                }

                match self.load_plugin(&entry.path()) {
                    Ok(info) => {
                        tracing::info!(
                            "Loaded plugin: {} v{} ({})",
                            info.name,
                            info.version,
                            info.id
                        );
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load plugin at {:?}: {}", entry.path(), e);
                    }
                }
            }
        }
        Ok(())
    }

    /// Load a single plugin from a directory containing manifest.json.
    fn load_plugin(&mut self, dir: &Path) -> Result<PluginInfo> {
        let manifest_path = dir.join("manifest.json");
        let manifest_str = std::fs::read_to_string(&manifest_path)?;
        let manifest: PluginManifest = serde_json::from_str(&manifest_str)?;
        let entry_path = dir.join(&manifest.entry);

        if !entry_path.exists() {
            return Err(anyhow::anyhow!(
                "Plugin entry file not found: {}",
                entry_path.display()
            ));
        }

        let plugin: Box<dyn Plugin> = match manifest.plugin_type {
            PluginType::Js => Box::new(JsPlugin::new(manifest, &entry_path)?),
            PluginType::Wasm => Box::new(WasmPlugin::new(manifest, &entry_path)?),
            PluginType::Ffi => Box::new(FfiPlugin::new(manifest, &entry_path)?),
        };

        let info = plugin.info();
        let id = info.id.clone();
        self.plugins.insert(id, plugin);
        Ok(info)
    }

    /// Call a method on a loaded plugin.
    pub fn call(
        &self,
        plugin_id: &str,
        method: &str,
        args: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let plugin = self
            .plugins
            .get(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Plugin not found: {}", plugin_id))?;
        plugin.call(method, args)
    }

    /// List all loaded plugins with their info.
    pub fn list(&self) -> Vec<PluginInfo> {
        self.plugins.values().map(|p| p.info()).collect()
    }

    /// Check if a plugin with the given id is loaded.
    pub fn has_plugin(&self, id: &str) -> bool {
        self.plugins.contains_key(id)
    }

    /// Get all loaded plugin ids.
    pub fn plugin_ids(&self) -> Vec<String> {
        self.plugins.keys().cloned().collect()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_plugin_manifest_parse() {
        let json = r#"{
            "id": "my-plugin",
            "name": "My Plugin",
            "version": "1.0.0",
            "description": "A test plugin",
            "type": "js",
            "entry": "index.js"
        }"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.id, "my-plugin");
        assert_eq!(manifest.name, "My Plugin");
        assert_eq!(manifest.version, "1.0.0");
        assert!(matches!(manifest.plugin_type, PluginType::Js));
        assert_eq!(manifest.entry, "index.js");
    }

    #[test]
    fn test_plugin_manifest_parse_wasm() {
        let json = r#"{
            "id": "wasm-plugin",
            "name": "WASM Plugin",
            "version": "0.1.0",
            "type": "wasm",
            "entry": "plugin.wasm"
        }"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert!(matches!(manifest.plugin_type, PluginType::Wasm));
        assert_eq!(manifest.description, ""); // default empty
    }

    #[test]
    fn test_plugin_registry_empty() {
        let registry = PluginRegistry::new();
        assert!(registry.list().is_empty());
        assert!(!registry.has_plugin("anything"));
        assert!(registry.plugin_ids().is_empty());
    }

    #[test]
    fn test_js_plugin_load_and_call() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("test-plugin");
        fs::create_dir_all(&dir).unwrap();

        // Write manifest
        fs::write(
            dir.join("manifest.json"),
            r#"{
                "id": "test-js",
                "name": "Test JS Plugin",
                "version": "1.0.0",
                "type": "js",
                "entry": "index.js"
            }"#,
        )
        .unwrap();

        // Write plugin JS
        fs::write(
            dir.join("index.js"),
            r#"
            module.exports = {
                greet: function(name) {
                    return "Hello, " + name + "!";
                },
                add: function(a, b) {
                    return a + b;
                }
            };
            "#,
        )
        .unwrap();

        let mut registry = PluginRegistry::new();
        registry.load_plugin(&dir).unwrap();

        assert!(registry.has_plugin("test-js"));
        let info = registry.list();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "Test JS Plugin");
        assert!(info[0].methods.contains(&"greet".to_string()));
        assert!(info[0].methods.contains(&"add".to_string()));

        // Call greet
        let result = registry
            .call(
                "test-js",
                "greet",
                vec![serde_json::Value::String("World".into())],
            )
            .unwrap();
        assert_eq!(result, serde_json::Value::String("Hello, World!".into()));

        // Call add
        let result = registry
            .call(
                "test-js",
                "add",
                vec![
                    serde_json::Value::Number(3.into()),
                    serde_json::Value::Number(4.into()),
                ],
            )
            .unwrap();
        assert_eq!(result, serde_json::json!(7));
    }

    #[test]
    fn test_js_plugin_missing_method() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("test-plugin");
        fs::create_dir_all(&dir).unwrap();

        fs::write(
            dir.join("manifest.json"),
            r#"{"id":"test","name":"Test","version":"1","type":"js","entry":"index.js"}"#,
        )
        .unwrap();
        fs::write(
            dir.join("index.js"),
            r#"module.exports = { foo: function() { return 1; } };"#,
        )
        .unwrap();

        let mut registry = PluginRegistry::new();
        registry.load_plugin(&dir).unwrap();

        let err = registry.call("test", "nonexistent", vec![]).unwrap_err();
        assert!(err.to_string().contains("has no method"));
    }

    #[test]
    fn test_plugin_not_found() {
        let registry = PluginRegistry::new();
        let err = registry.call("missing", "method", vec![]).unwrap_err();
        assert!(err.to_string().contains("Plugin not found"));
    }

    #[test]
    fn test_load_from_dirs_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let mut registry = PluginRegistry::new();
        registry
            .load_from_dirs(&[tmp.path().to_path_buf()])
            .unwrap();
        assert!(registry.list().is_empty());
    }

    #[test]
    fn test_load_from_dirs_with_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("plugins").join("test-plugin");
        fs::create_dir_all(&dir).unwrap();

        fs::write(
            dir.join("manifest.json"),
            r#"{"id":"test","name":"Test","version":"1","type":"js","entry":"index.js"}"#,
        )
        .unwrap();
        fs::write(
            dir.join("index.js"),
            r#"module.exports = { hello: function() { return "world"; } };"#,
        )
        .unwrap();

        let mut registry = PluginRegistry::new();
        registry
            .load_from_dirs(&[tmp.path().to_path_buf()])
            .unwrap();

        assert!(registry.has_plugin("test"));
        let result = registry.call("test", "hello", vec![]).unwrap();
        assert_eq!(result, serde_json::json!("world"));
    }
}
