//! QuickJS script instance — implements the Script trait.

use anyhow::Result;
use async_trait::async_trait;
use rquickjs::{async_with, promise::PromiseState, AsyncContext, AsyncRuntime, Function};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{error, info, warn};

use super::bridge::{register_ctx_api, CancelTokenSlot, ScriptCtxSlot};
use crate::engine::{CancellationToken, CaptureFrame, LogLevel, Script, ScriptContext, ScriptType};
use crate::manifest::{load_and_check_dependency, resolve_dependency_root, Manifest};

/// QuickJS script instance.
pub struct QuickJsScript {
    manifest: Manifest,
    source: String,
    #[allow(dead_code)]
    rt: AsyncRuntime,
    ctx: AsyncContext,
    cancelled: CancellationToken,
    max_execution_ms: u64,
    last_result: Option<serde_json::Value>,
    /// Whether the script source has been evaluated into the JS context.
    initialized: AtomicBool,
    /// Whether ctx bridge API has been injected into the JS context.
    api_registered: AtomicBool,
    ctx_slot: ScriptCtxSlot,
    cancel_slot: CancelTokenSlot,
    trigger_params: Arc<std::sync::RwLock<serde_json::Value>>,
    data_root: PathBuf,
    engine_version: String,
}

impl QuickJsScript {
    pub fn new(
        manifest: Manifest,
        source: String,
        #[allow(dead_code)] rt: AsyncRuntime,
        ctx: AsyncContext,
        max_execution_ms: u64,
        data_root: PathBuf,
        engine_version: String,
    ) -> Result<Self> {
        Ok(Self {
            manifest,
            source,
            rt,
            ctx,
            cancelled: CancellationToken::new(),
            max_execution_ms,
            last_result: None,
            initialized: AtomicBool::new(false),
            api_registered: AtomicBool::new(false),
            ctx_slot: Arc::new(std::sync::RwLock::new(None)),
            cancel_slot: Arc::new(std::sync::RwLock::new(None)),
            trigger_params: Arc::new(std::sync::RwLock::new(serde_json::json!({}))),
            data_root,
            engine_version,
        })
    }

    /// User-visible script error (log panel) + tracing for developers.
    fn emit_script_failure(&self, ctx: &Arc<dyn ScriptContext>, phase_zh: &str, err: &str) {
        let msg = format!("「{}」{}: {}", self.manifest.display_name, phase_zh, err);
        ctx.log(LogLevel::Error, &msg);
        error!(
            target: "betternte",
            script = %self.manifest.name,
            phase = %phase_zh,
            error = %err,
            "script failure"
        );
    }

    /// Load `manifest.dependencies` into the current JS context: `ctx[libManifestName][export] = fn`.
    fn mount_manifest_dependencies(
        &self,
        js_ctx: &rquickjs::Ctx<'_>,
        ctx: &Arc<dyn ScriptContext>,
    ) -> Result<()> {
        if self.manifest.dependencies.is_empty() {
            return Ok(());
        }

        let strict = ctx.manifest_security_strict();
        for dep in &self.manifest.dependencies {
            let lib_root = resolve_dependency_root(&self.data_root, dep)?;
            let lib_manifest = load_and_check_dependency(dep, &lib_root, &self.engine_version)?;
            let entry_rel = lib_manifest.entry.trim();
            if entry_rel.is_empty() {
                anyhow::bail!("Library '{}' has empty entry", lib_manifest.name);
            }
            let dep_source_path = lib_root.join(entry_rel);
            let dep_source = std::fs::read_to_string(&dep_source_path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to read dependency '{}' entry {}: {}",
                    lib_manifest.name,
                    dep_source_path.display(),
                    e
                )
            })?;

            ctx.push_manifest_permission_scope(&lib_manifest.permissions, strict);

            let reset_exports = "globalThis.__libraryExports = {}; globalThis.exports = {};";
            if let Err(e) = js_ctx.eval::<(), _>(reset_exports) {
                ctx.pop_manifest_permission_scope();
                let detail = format!("{}", e);
                self.emit_script_failure(
                    ctx,
                    &format!("依赖库「{}」初始化失败", lib_manifest.display_name),
                    &detail,
                );
                return Err(anyhow::anyhow!("JS reset exports: {}", detail));
            }

            if let Err(e) = js_ctx.eval::<(), _>(dep_source.as_str()) {
                ctx.pop_manifest_permission_scope();
                let detail = format!("{}", e);
                self.emit_script_failure(
                    ctx,
                    &format!("依赖库「{}」代码错误", lib_manifest.display_name),
                    &detail,
                );
                return Err(anyhow::anyhow!(
                    "Dependency '{}' JS eval error: {}",
                    lib_manifest.name,
                    detail
                ));
            }

            let lib_key = serde_json::to_string(&lib_manifest.name)?;
            let assign_ns = format!(
                r#"(function() {{
                    var ctxObj = globalThis.ctx;
                    if (!ctxObj) throw new Error("ctx missing");
                    var ns = {{}};
                    var ex = globalThis.__libraryExports || {{}};
                    for (var k in ex) {{
                        if (Object.prototype.hasOwnProperty.call(ex, k)) ns[k] = ex[k];
                    }}
                    var ex2 = globalThis.exports || {{}};
                    for (var k2 in ex2) {{
                        if (Object.prototype.hasOwnProperty.call(ex2, k2) && typeof ex2[k2] === "function") {{
                            ns[k2] = ex2[k2];
                        }}
                    }}
                    ctxObj[{lib_key}] = ns;
                    globalThis.__libraryExports = {{}};
                    globalThis.exports = {{}};
                }})();"#,
                lib_key = lib_key
            );
            if let Err(e) = js_ctx.eval::<(), _>(assign_ns.as_str()) {
                ctx.pop_manifest_permission_scope();
                let detail = format!("{}", e);
                self.emit_script_failure(
                    ctx,
                    &format!("依赖库「{}」挂接到 ctx 失败", lib_manifest.display_name),
                    &detail,
                );
                return Err(anyhow::anyhow!(
                    "Failed to attach dependency '{}' to ctx: {}",
                    lib_manifest.name,
                    detail
                ));
            }

            ctx.pop_manifest_permission_scope();
        }

        Ok(())
    }

    fn ensure_api_registered(&self, js_ctx: &rquickjs::Ctx) -> Result<()> {
        if !self.api_registered.load(Ordering::Acquire) {
            register_ctx_api(js_ctx, self.ctx_slot.clone(), self.cancel_slot.clone())?;
            self.api_registered.store(true, Ordering::Release);
        }
        Ok(())
    }

    /// Evaluate the script source once, defining all top-level functions/globals.
    async fn eval_source(&self) -> Result<()> {
        let source = self.source.clone();
        async_with!(self.ctx => |js_ctx| {
            self.ensure_api_registered(&js_ctx)?;
            js_ctx.eval::<(), _>(
                "globalThis.exports = globalThis.exports || {}; \
                 globalThis.__libraryExports = globalThis.__libraryExports || {};",
            )
            .map_err(|e| anyhow::anyhow!("JS prelude eval error: {}", e))?;
            js_ctx.eval::<(), _>(source.as_str())
                .map_err(|e| anyhow::anyhow!("JS eval error: {}", e))?;
            Ok(())
        })
        .await
    }

    /// Execute a closure in the JS context.
    ///
    /// On first call, evaluates the script source so top-level functions/globals are defined.
    /// `enabled` controls the `$enabled` global (true only for on_enable).
    /// `params` is injected as `$params` global.
    async fn with_js<F, R>(
        &self,
        ctx: &Arc<dyn ScriptContext>,
        enabled: bool,
        params: &serde_json::Value,
        f: F,
    ) -> Result<R>
    where
        F: FnOnce(&rquickjs::Ctx) -> Result<R> + Send + 'static,
        R: Send + 'static,
    {
        let params_val = params.clone();
        let needs_init = !self.initialized.load(Ordering::Acquire);
        let source = if needs_init {
            Some(self.source.clone())
        } else {
            None
        };
        if let Ok(mut slot) = self.ctx_slot.write() {
            *slot = Some(ctx.clone());
        }
        if let Ok(mut slot) = self.cancel_slot.write() {
            *slot = Some(self.cancelled.clone());
        }

        let strict = ctx.manifest_security_strict();
        ctx.push_manifest_permission_scope(&self.manifest.permissions, strict);

        let ctx_for_deps = ctx.clone();
        let ctx_emit = ctx.clone();
        let timeout_duration = std::time::Duration::from_millis(self.max_execution_ms);
        let result = tokio::time::timeout(
            timeout_duration,
            async_with!(self.ctx => |js_ctx| {
                self.ensure_api_registered(&js_ctx)?;
                if let Err(e) = js_ctx.eval::<(), _>(
                    "globalThis.exports = globalThis.exports || {}; \
                     globalThis.__libraryExports = globalThis.__libraryExports || {};",
                ) {
                    let detail = format!("{}", e);
                    self.emit_script_failure(&ctx_emit, "脚本预导入失败", &detail);
                    return Err(anyhow::anyhow!("JS prelude eval error: {}", detail));
                }

                // Evaluate script source on first call so top-level functions are defined
                if let Some(src) = source {
                    self.mount_manifest_dependencies(&js_ctx, &ctx_for_deps)?;
                    if let Err(e) = js_ctx.eval::<(), _>(src.as_str()) {
                        let detail = format!("{}", e);
                        self.emit_script_failure(
                            &ctx_emit,
                            "脚本加载失败（语法错误或未闭合括号等）",
                            &detail,
                        );
                        return Err(anyhow::anyhow!("JS eval error: {}", detail));
                    }
                }

                // Inject params into ctx object for new-style scripts
                let params_js = super::value::json_to_js(&js_ctx, &params_val)?;
                if let Ok(ctx_obj) = js_ctx.globals().get::<_, rquickjs::Object>("ctx") {
                    ctx_obj.set("params", params_js.clone())?;
                }

                // Legacy globals for backward compatibility
                js_ctx.globals().set("$enabled", enabled)?;
                js_ctx.globals().set("$params", params_js)?;

                f(&js_ctx)
            }),
        )
        .await;

        ctx.pop_manifest_permission_scope();

        // Mark as initialized only if the outer timeout and inner JS call both succeeded.
        if needs_init && matches!(result, Ok(Ok(_))) {
            self.initialized.store(true, Ordering::Release);
        }

        if let Ok(mut slot) = self.ctx_slot.write() {
            *slot = None;
        }
        if let Ok(mut slot) = self.cancel_slot.write() {
            *slot = None;
        }

        match result {
            Ok(inner) => inner,
            Err(_) => Err(crate::error::ScriptError::Timeout(self.max_execution_ms).into()),
        }
    }

    /// Call an optional JS hook function (no error if it doesn't exist).
    async fn call_optional_hook(&self, ctx: &Arc<dyn ScriptContext>, fn_name: &str) -> Result<()> {
        let fn_name_owned = fn_name.to_string();

        self.with_js(ctx, false, &serde_json::json!({}), move |js_ctx| {
            let func: Function = match js_ctx.globals().get(fn_name_owned.as_str()) {
                Ok(f) => f,
                Err(_) => return Ok(()),
            };
            match func.call::<_, ()>(()) {
                Ok(()) => {
                    // Drain job queue for async hook continuations
                    while js_ctx.execute_pending_job() {}
                    Ok(())
                }
                Err(e) => {
                    warn!("Optional hook '{}' error: {}", fn_name_owned, e);
                    Ok(())
                }
            }
        })
        .await
    }
}

#[async_trait]
impl Script for QuickJsScript {
    fn name(&self) -> &str {
        &self.manifest.name
    }

    fn script_type(&self) -> ScriptType {
        self.manifest.script_type.clone()
    }

    async fn init(&mut self, ctx: &Arc<dyn ScriptContext>) -> Result<()> {
        info!("Initializing script '{}'", self.manifest.name);
        self.eval_source().await?;
        self.call_optional_hook(ctx, "init").await
    }

    async fn on_enable(
        &mut self,
        ctx: &Arc<dyn ScriptContext>,
        params: &serde_json::Value,
    ) -> Result<()> {
        info!("Enabling script '{}'", self.manifest.name);
        if let Ok(mut p) = self.trigger_params.write() {
            *p = params.clone();
        }
        let params_val = params.clone();
        let params_for_closure = params_val.clone();
        self.with_js(ctx, true, &params_val, move |js_ctx| {
            if let Ok(func) = js_ctx.globals().get::<_, Function>("onEnable") {
                let params_js = super::value::json_to_js(js_ctx, &params_for_closure)?;
                func.call::<_, ()>((params_js,))?;
                // Drain job queue for async onEnable continuations
                while js_ctx.execute_pending_job() {}
            }
            Ok(())
        })
        .await
    }

    async fn start(
        &mut self,
        ctx: &Arc<dyn ScriptContext>,
        config: &serde_json::Value,
    ) -> Result<()> {
        info!("Starting script '{}'", self.manifest.name);
        self.cancelled.reset();
        ctx.reset_cancel();

        let config_val = config.clone();
        let config_for_closure = config_val.clone();
        let script_name = self.manifest.name.clone();
        let display_name = self.manifest.display_name.clone();
        let cancel = self.cancelled.clone();
        let runtime_ctx = ctx.clone();

        let result = self
            .with_js(ctx, false, &config_val, move |js_ctx| {
                let config_js = super::value::json_to_js(js_ctx, &config_for_closure)?;
                js_ctx.globals().set("config", config_js)?;

                // Try "start" first, then "main" for compatibility with generated scripts
                let func: Option<Function> = js_ctx
                    .globals()
                    .get::<_, Function>("start")
                    .ok()
                    .or_else(|| js_ctx.globals().get::<_, Function>("main").ok());
                if let Some(func) = func {
                    // Don't pass config as argument — it's already set as global `config`.
                    // Passing it would shadow the global `ctx` if the script uses `start(ctx)`.
                    let ret: rquickjs::Value = match func.call(()) {
                        Ok(v) => v,
                        Err(e) => {
                            let detail = format!("{}", e);
                            let msg =
                                format!("「{}」start/main 入口调用失败: {}", display_name, detail);
                            runtime_ctx.log(LogLevel::Error, &msg);
                            error!(
                                target: "betternte",
                                script = %script_name,
                                error = %detail,
                                "start call failed"
                            );
                            return Err(anyhow::anyhow!("Script start call error: {}", detail));
                        }
                    };
                    // Drain the QuickJS job queue so async function continuations execute.
                    // Each `await ctx.xxx()` call resolves the Promise synchronously via
                    // __invoke, then queues the continuation as a microtask. Without this
                    // loop, the function suspends at the first `await` and never resumes.
                    while js_ctx.execute_pending_job() {
                        // Check cancellation between continuations for force-stop
                        if cancel.is_cancelled() || runtime_ctx.is_cancelled() {
                            info!("Script '{}' cancelled during execution", script_name);
                            break;
                        }
                    }

                    // Check if the return value is a rejected Promise (async function threw).
                    // func.call() returns Ok(promise) even when the async function rejects,
                    // so we must inspect the Promise state explicitly.
                    if ret.is_promise() {
                        if let Some(promise) = ret.clone().into_promise() {
                            match promise.state() {
                                PromiseState::Rejected => {
                                    // result() throws the rejection value; catch it.
                                    let _ = promise.result::<rquickjs::Value>();
                                    let caught = js_ctx.catch();
                                    let detail =
                                        super::value::js_to_json(&caught)
                                            .map(|j| j.to_string())
                                            .unwrap_or_else(|_| format!("{:?}", caught));
                                    let msg = format!(
                                        "「{}」start/main 执行异常: {}",
                                        display_name, detail
                                    );
                                    runtime_ctx.log(LogLevel::Error, &msg);
                                    error!(
                                        target: "betternte",
                                        script = %script_name,
                                        error = %detail,
                                        "async start rejected"
                                    );
                                    return Err(anyhow::anyhow!(
                                        "Script '{}' async error: {}",
                                        script_name,
                                        detail
                                    ));
                                }
                                PromiseState::Resolved => {}
                                PromiseState::Pending => {
                                    // Still pending after draining — async work didn't complete.
                                    warn!(
                                        script = %script_name,
                                        "Script start() returned a pending Promise; async work may not have completed"
                                    );
                                }
                            }
                        }
                    }

                    // Convert JS return value to JSON
                    let json_val =
                        super::value::js_to_json(&ret).unwrap_or(serde_json::Value::Null);
                    return Ok(Some(json_val));
                }
                {
                    let msg = format!(
                        "「{}」未找到入口函数: 请定义 async function start() 或 main()",
                        display_name
                    );
                    runtime_ctx.log(LogLevel::Error, &msg);
                    error!(target: "betternte", script = %script_name, "no start/main");
                    Err(anyhow::anyhow!(
                        "Script '{}' has no 'start' or 'main' function",
                        script_name
                    ))
                }
            })
            .await?;

        self.last_result = result;
        Ok(())
    }

    async fn stop(&mut self, ctx: &Arc<dyn ScriptContext>) -> Result<()> {
        info!("Stopping script '{}'", self.manifest.name);
        // Set cancelled flag — the execute_pending_job() loop in start() checks this
        // and will break out, making start() return and release the JS context.
        self.cancelled.cancel();

        // Try to call the JS stop() hook with a short timeout.
        // If the context is held by start() (which is now checking cancelled),
        // this will timeout and we return immediately — the script will stop
        // once start() finishes its current continuation.
        let stop_result = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            self.with_js(ctx, false, &serde_json::json!({}), move |js_ctx| {
                if let Ok(func) = js_ctx.globals().get::<_, Function>("stop") {
                    func.call::<_, ()>(()).ok();
                    while js_ctx.execute_pending_job() {}
                }
                Ok(())
            }),
        )
        .await;

        match stop_result {
            Ok(inner) => inner,
            Err(_) => {
                // Timeout — context still held by start(). That's fine,
                // start() will exit soon since cancelled flag is set.
                info!(
                    "Script '{}' stop: JS context busy, flag set for graceful exit",
                    self.manifest.name
                );
                Ok(())
            }
        }
    }

    async fn on_capture(
        &mut self,
        ctx: &Arc<dyn ScriptContext>,
        frame: &CaptureFrame,
    ) -> Result<()> {
        if self.cancelled.is_cancelled() {
            return Ok(());
        }

        let _script_name = self.manifest.name.clone();
        let frame_json = serde_json::json!({"width": frame.width, "height": frame.height});
        let trigger_params = self
            .trigger_params
            .read()
            .map(|v| v.clone())
            .unwrap_or_else(|_| serde_json::json!({}));

        // tracing::info!(script = %script_name, "on_capture: calling hook");
        self.with_js(ctx, false, &trigger_params, move |js_ctx| {
            // Prefer onTrigger(ctx) for trigger-style scripts.
            if let Ok(on_trigger) = js_ctx.globals().get::<_, Function>("onTrigger") {
                let ctx_obj: rquickjs::Object = js_ctx.globals().get("ctx")?;
                on_trigger.call::<_, ()>((ctx_obj,))?;
                while js_ctx.execute_pending_job() {}
                return Ok(());
            }

            // Fall back to onCapture(frame)
            let func: Function = match js_ctx.globals().get("onCapture") {
                Ok(f) => f,
                Err(_) => return Ok(()),
            };
            let frame_js = super::value::json_to_js(js_ctx, &frame_json)?;
            func.call::<_, ()>((frame_js,))?;
            // Drain job queue for async onCapture continuations
            while js_ctx.execute_pending_job() {}
            Ok(())
        })
        .await
    }

    async fn on_disable(&mut self, ctx: &Arc<dyn ScriptContext>) -> Result<()> {
        info!("Disabling script '{}'", self.manifest.name);
        self.call_optional_hook(ctx, "onDisable").await
    }

    async fn destroy(&mut self, ctx: &Arc<dyn ScriptContext>) -> Result<()> {
        info!("Destroying script '{}'", self.manifest.name);
        self.call_optional_hook(ctx, "destroy").await
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.is_cancelled()
    }

    fn cancellation_token(&self) -> Option<CancellationToken> {
        Some(self.cancelled.clone())
    }

    fn last_result(&self) -> Option<&serde_json::Value> {
        self.last_result.as_ref()
    }

    async fn call_function(
        &mut self,
        ctx: &Arc<dyn ScriptContext>,
        function: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let function_name = function.to_string();
        let args_json = serde_json::to_string(args).unwrap_or_else(|_| "null".to_string());
        let args_json_for_js = args_json.clone();
        let script_name = self.manifest.name.clone();

        self.with_js(ctx, false, &serde_json::json!({}), move |js_ctx| {
            js_ctx.globals()
                .set("__libCallFnName", function_name.clone())
                .map_err(|e| anyhow::anyhow!("Set __libCallFnName failed: {}", e))?;
            js_ctx
                .globals()
                .set("__libCallArgsJson", args_json_for_js.clone())
                .map_err(|e| anyhow::anyhow!("Set __libCallArgsJson failed: {}", e))?;

            js_ctx.eval::<(), _>(
                r#"
                (function () {
                    var fnName = __libCallFnName;
                    var args = JSON.parse(__libCallArgsJson || "null");
                    var done = false;
                    var resultJson = "null";
                    var errorMsg = null;

                    function resolveFn(name) {
                        if (globalThis.__libraryExports && typeof globalThis.__libraryExports[name] === "function") {
                            return globalThis.__libraryExports[name];
                        }
                        if (globalThis.exports && typeof globalThis.exports[name] === "function") {
                            return globalThis.exports[name];
                        }
                        if (typeof globalThis[name] === "function") {
                            return globalThis[name];
                        }
                        return null;
                    }

                    var fn = resolveFn(fnName);
                    if (typeof fn !== "function") {
                        globalThis.__libCallDone = true;
                        globalThis.__libCallResultJson = "null";
                        globalThis.__libCallError = "__FN_NOT_FOUND__";
                        return;
                    }

                    globalThis.__libCallDone = false;
                    globalThis.__libCallResultJson = "null";
                    globalThis.__libCallError = null;

                    Promise.resolve(fn(args))
                        .then(function (v) {
                            try {
                                var value = (typeof v === "undefined") ? null : v;
                                resultJson = JSON.stringify(value);
                            } catch (e) {
                                errorMsg = "Failed to serialize library result: " + String(e);
                            }
                            done = true;
                        })
                        .catch(function (e) {
                            var msg = (e && e.message) ? e.message : String(e);
                            errorMsg = msg;
                            done = true;
                        })
                        .finally(function () {
                            globalThis.__libCallResultJson = resultJson;
                            globalThis.__libCallError = errorMsg;
                            globalThis.__libCallDone = done;
                        });
                })();
                "#,
            )
            .map_err(|e| anyhow::anyhow!("Library call bootstrap failed: {}", e))?;

            let mut spins = 0usize;
            loop {
                while js_ctx.execute_pending_job() {}
                let done = js_ctx
                    .globals()
                    .get::<_, bool>("__libCallDone")
                    .unwrap_or(false);
                if done {
                    break;
                }
                spins += 1;
                if spins > 10_000 {
                    return Err(anyhow::anyhow!(
                        "{}",
                        crate::error::ScriptError::LibraryExecutionFailed {
                            library: script_name.clone(),
                            function: function_name.clone(),
                            reason: "Library call timed out while waiting for Promise completion"
                                .to_string(),
                        }
                    ));
                }
            }

            if let Ok(error_msg) = js_ctx.globals().get::<_, String>("__libCallError") {
                if error_msg == "__FN_NOT_FOUND__" {
                    return Err(anyhow::anyhow!(
                        "{}",
                        crate::error::ScriptError::LibraryFunctionNotFound {
                            library: script_name.clone(),
                            function: function_name.clone(),
                        }
                    ));
                }
                if !error_msg.is_empty() {
                    return Err(anyhow::anyhow!(
                        "{}",
                        crate::error::ScriptError::LibraryExecutionFailed {
                            library: script_name.clone(),
                            function: function_name.clone(),
                            reason: error_msg,
                        }
                    ));
                }
            }

            let result_json = js_ctx
                .globals()
                .get::<_, String>("__libCallResultJson")
                .unwrap_or_else(|_| "null".to_string());
            let result_val = serde_json::from_str::<serde_json::Value>(&result_json).map_err(|e| {
                anyhow::anyhow!(
                    "{}",
                    crate::error::ScriptError::LibraryExecutionFailed {
                        library: script_name.clone(),
                        function: function_name.clone(),
                        reason: format!("Invalid JSON result: {}", e),
                    }
                )
            })?;
            Ok(result_val)
        })
        .await
    }
}
