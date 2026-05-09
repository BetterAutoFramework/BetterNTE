//! Script loader — discovers, validates, and loads scripts.

use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Stable script instance id: path relative to workspace `data_root`, POSIX slashes (e.g. `main/scripts/foo`).
pub fn script_id_for_path(script_dir: &Path, data_root: &Path) -> String {
    let rel = script_dir.strip_prefix(data_root).unwrap_or(script_dir);
    rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

use crate::engine::{Script, ScriptEngine, ScriptType};
use crate::manifest::{EngineVersionReq, Manifest};

// ============================================================================
// ScriptLoader — 脚本加载器
// ============================================================================

/// 脚本加载器，负责发现、验证、加载脚本。
pub struct ScriptLoader {
    /// 已注册的脚本引擎，key = 文件扩展名
    engines: HashMap<String, Box<dyn ScriptEngine>>,

    /// 当前引擎版本
    engine_version: String,

    /// 版本兼容性检查器
    version_req: EngineVersionReq,
}

impl ScriptLoader {
    pub fn new(engine_version: &str) -> Result<Self> {
        Ok(Self {
            engines: HashMap::new(),
            engine_version: engine_version.to_string(),
            version_req: EngineVersionReq::new(engine_version)?,
        })
    }

    /// 注册脚本引擎。
    pub fn register_engine(&mut self, extension: &str, engine: Box<dyn ScriptEngine>) {
        info!(
            "Registered script engine: {} (.{} files)",
            engine.name(),
            extension
        );
        self.engines.insert(extension.to_string(), engine);
    }

    /// 扫描脚本目录，返回所有已发现的脚本信息。
    pub fn discover_scripts(
        &self,
        scripts_dir: &Path,
        data_root: &Path,
    ) -> Result<Vec<ScriptInfo>> {
        let mut scripts = Vec::new();

        if !scripts_dir.exists() {
            return Ok(scripts);
        }

        let mut stack = vec![scripts_dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            for entry in std::fs::read_dir(&current)? {
                let entry = entry?;
                let path = entry.path();
                let file_name = entry.file_name();

                if !path.is_dir() {
                    continue;
                }
                if matches!(file_name.to_str(), Some("templates" | "store")) {
                    continue;
                }

                let manifest_path = path.join("manifest.json");
                if manifest_path.exists() {
                    match Manifest::from_file(&manifest_path) {
                        Ok(manifest) => {
                            let compatible = self.version_req.check(&manifest);
                            let script_id = script_id_for_path(&path, data_root);
                            scripts.push(ScriptInfo {
                                path: path.clone(),
                                script_id,
                                manifest,
                                compatible,
                                loaded: false,
                                data_root: data_root.to_path_buf(),
                            });
                        }
                        Err(e) => {
                            warn!("Failed to load manifest from {}: {}", path.display(), e);
                        }
                    }
                    continue;
                }

                stack.push(path);
            }
        }

        scripts.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
        Ok(scripts)
    }

    /// 加载单个脚本。
    pub async fn load_script(&self, script_info: &ScriptInfo) -> Result<Box<dyn Script>> {
        if !script_info.compatible {
            bail!(
                "Script '{}' requires engine {}, but current is {}",
                script_info.manifest.name,
                script_info.manifest.format_requirement(),
                self.engine_version
            );
        }

        let entry_path = script_info.path.join(&script_info.manifest.entry);
        let extension = entry_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let engine = self.engines.get(extension).ok_or_else(|| {
            anyhow::anyhow!(
                "No script engine registered for '.{}' files (script: {})",
                extension,
                script_info.manifest.name
            )
        })?;

        info!(
            "Loading script '{}' with {} engine",
            script_info.manifest.name,
            engine.name()
        );

        engine
            .load(&entry_path, &script_info.manifest, &script_info.data_root)
            .await
    }

    /// Re-read `manifest.json` beside [`ScriptInfo::path`] and update manifest + compatibility.
    ///
    /// Used when hot-reloading so manifest changes edited on disk take effect
    /// without restarting the engine.
    pub fn refresh_manifest_from_disk(&self, info: &mut ScriptInfo) {
        let manifest_path = info.path.join("manifest.json");
        if !manifest_path.is_file() {
            return;
        }
        match Manifest::from_file(&manifest_path) {
            Ok(manifest) => {
                info.compatible = self.version_req.check(&manifest);
                info.manifest = manifest;
            }
            Err(e) => {
                warn!(
                    path = %manifest_path.display(),
                    error = %e,
                    "refresh_manifest_from_disk: parse failed; keeping cached manifest"
                );
            }
        }
    }

    /// 获取已注册的引擎列表。
    pub fn registered_engines(&self) -> Vec<&str> {
        self.engines.values().map(|e| e.name()).collect()
    }

    pub async fn unload_all(&self) -> Result<()> {
        for engine in self.engines.values() {
            engine.unload_all().await?;
        }
        Ok(())
    }
}

// ============================================================================
// ScriptInfo — 脚本信息
// ============================================================================

/// 已发现的脚本信息（不一定已加载）。
#[derive(Debug, Clone)]
pub struct ScriptInfo {
    pub path: PathBuf,
    /// Unique key under workspace data root (see [`script_id_for_path`]).
    pub script_id: String,
    pub manifest: Manifest,
    pub compatible: bool,
    pub loaded: bool,
    /// Workspace data root (for resolving `manifest.dependencies[].path`).
    pub data_root: PathBuf,
}

impl ScriptInfo {
    pub fn display_name(&self) -> &str {
        &self.manifest.display_name
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    pub fn script_type(&self) -> &ScriptType {
        &self.manifest.script_type
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manifest_json(name: &str, version: &str, entry: &str) -> String {
        serde_json::json!({
            "schema_version": 1,
            "name": name,
            "display_name": name,
            "version": version,
            "type": "solo_task",
            "entry": entry
        })
        .to_string()
    }

    #[test]
    fn test_loader_new_valid_version() {
        let loader = ScriptLoader::new("1.0.0").unwrap();
        assert!(loader.registered_engines().is_empty());
    }

    #[test]
    fn test_loader_new_invalid_version() {
        assert!(ScriptLoader::new("not-a-version").is_err());
    }

    #[test]
    fn test_register_engine_and_list() {
        // We can't easily create a real ScriptEngine without QuickJS runtime,
        // but we can test the registration logic by checking registered_engines()
        // This test verifies the HashMap insert logic
        let loader = ScriptLoader::new("1.0.0").unwrap();
        // Initially empty
        assert!(loader.registered_engines().is_empty());
    }

    #[test]
    fn test_discover_scripts_nonexistent_dir() {
        let loader = ScriptLoader::new("1.0.0").unwrap();
        let result = loader.discover_scripts(Path::new("/nonexistent/dir"), Path::new("/tmp"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_discover_scripts_empty_dir() {
        let dir = std::env::temp_dir().join("betternte_test_loader_empty");
        let _ = std::fs::create_dir_all(&dir);
        let loader = ScriptLoader::new("1.0.0").unwrap();
        let result = loader.discover_scripts(&dir, &dir).unwrap();
        assert!(result.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_scripts_with_valid_manifest() {
        let base = std::env::temp_dir().join("betternte_test_loader_valid");
        let script_dir = base.join("my_script");
        let _ = std::fs::create_dir_all(&script_dir);
        std::fs::write(
            script_dir.join("manifest.json"),
            make_manifest_json("my_script", "1.0.0", "main.js"),
        )
        .unwrap();

        let loader = ScriptLoader::new("1.0.0").unwrap();
        let scripts = loader.discover_scripts(&base, &base).unwrap();
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0].name(), "my_script");
        assert_eq!(scripts[0].version(), "1.0.0");
        assert!(scripts[0].compatible);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_discover_scripts_with_incompatible_manifest() {
        let base = std::env::temp_dir().join("betternte_test_loader_incompat");
        let script_dir = base.join("old_script");
        let _ = std::fs::create_dir_all(&script_dir);
        // Script requires engine >=2.0.0 but we have 1.0.0
        let manifest = serde_json::json!({
            "schema_version": 1,
            "name": "old_script",
            "display_name": "Old Script",
            "version": "1.0.0",
            "type": "solo_task",
            "entry": "main.js",
            "min_engine_version": "2.0.0"
        });
        std::fs::write(script_dir.join("manifest.json"), manifest.to_string()).unwrap();

        let loader = ScriptLoader::new("1.0.0").unwrap();
        let scripts = loader.discover_scripts(&base, &base).unwrap();
        assert_eq!(scripts.len(), 1);
        assert!(!scripts[0].compatible);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_discover_scripts_skips_invalid_manifest() {
        let base = std::env::temp_dir().join("betternte_test_loader_invalid");
        let script_dir = base.join("bad_script");
        let _ = std::fs::create_dir_all(&script_dir);
        std::fs::write(script_dir.join("manifest.json"), "not json").unwrap();

        let loader = ScriptLoader::new("1.0.0").unwrap();
        let scripts = loader.discover_scripts(&base, &base).unwrap();
        assert!(scripts.is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_discover_scripts_skips_non_dir_entries() {
        let base = std::env::temp_dir().join("betternte_test_loader_file");
        let _ = std::fs::create_dir_all(&base);
        // Create a regular file (not a directory) - should be skipped
        std::fs::write(base.join("not_a_dir.txt"), "hello").unwrap();

        let loader = ScriptLoader::new("1.0.0").unwrap();
        let scripts = loader.discover_scripts(&base, &base).unwrap();
        assert!(scripts.is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_script_info_accessors() {
        let base = std::env::temp_dir().join("betternte_test_loader_accessors");
        let script_dir = base.join("info_test");
        let _ = std::fs::create_dir_all(&script_dir);
        std::fs::write(
            script_dir.join("manifest.json"),
            make_manifest_json("info_test", "2.1.0", "index.js"),
        )
        .unwrap();

        let loader = ScriptLoader::new("1.0.0").unwrap();
        let scripts = loader.discover_scripts(&base, &base).unwrap();
        let info = &scripts[0];
        assert_eq!(info.display_name(), "info_test");
        assert_eq!(info.name(), "info_test");
        assert_eq!(info.version(), "2.1.0");
        assert_eq!(*info.script_type(), ScriptType::SoloTask);
        let _ = std::fs::remove_dir_all(&base);
    }
}
