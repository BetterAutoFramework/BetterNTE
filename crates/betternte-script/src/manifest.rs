//! Script manifest and version compatibility checking.

use anyhow::{bail, Result};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::engine::ScriptType;

// ============================================================================
// Script dependencies — library roots under data_root
// ============================================================================

/// Declares a library dependency loaded into the same JS context as the host script.
///
/// `path` is relative to the workspace **data root** (same layout as script `dir` in the
/// engine), e.g. `local/scripts/common_api`. Optional engine fields override the library
/// manifest for compatibility checks only (same rules as [`Manifest::check_engine_compatibility`]).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScriptDependency {
    /// Directory relative to data root, POSIX separators, e.g. `local/scripts/my_lib`.
    pub path: String,
    #[serde(default)]
    pub min_engine_version: Option<String>,
    #[serde(default)]
    pub max_engine_version: Option<String>,
    #[serde(default)]
    pub engine_version: Option<String>,
}

impl ScriptDependency {
    pub fn validate_path(&self) -> Result<()> {
        let p = self.path.trim();
        if p.is_empty() {
            bail!("manifest.json: dependency 'path' must not be empty");
        }
        if p.contains("..") {
            bail!("manifest.json: dependency 'path' must not contain '..'");
        }
        Ok(())
    }
}

/// Resolve `dependency.path` against `data_root` and ensure `manifest.json` exists.
pub fn resolve_dependency_root(data_root: &Path, dependency: &ScriptDependency) -> Result<PathBuf> {
    dependency.validate_path()?;
    let tail = dependency.path.trim().trim_start_matches(['/', '\\']);
    let root = data_root.join(tail);
    if !root.join("manifest.json").is_file() {
        bail!(
            "Dependency path '{}' does not contain manifest.json under data root",
            dependency.path
        );
    }
    Ok(root)
}

/// Merge optional engine constraints from the dependency entry onto the library manifest.
pub fn effective_dependency_manifest(dep: &ScriptDependency, lib: &Manifest) -> Manifest {
    let mut m = lib.clone();
    if dep.min_engine_version.is_some() {
        m.min_engine_version = dep.min_engine_version.clone();
    }
    if dep.max_engine_version.is_some() {
        m.max_engine_version = dep.max_engine_version.clone();
    }
    if dep.engine_version.is_some() {
        m.engine_version = dep.engine_version.clone();
    }
    m
}

/// Load a dependency library manifest, ensure it is a library, and check engine compatibility.
pub fn load_and_check_dependency(
    dep: &ScriptDependency,
    lib_root: &Path,
    engine_version: &str,
) -> Result<Manifest> {
    let mf_path = lib_root.join("manifest.json");
    let lib_manifest = Manifest::from_file(&mf_path)?;
    if lib_manifest.script_type != ScriptType::Library {
        bail!(
            "Dependency at '{}' is type {:?}, expected library",
            lib_root.display(),
            lib_manifest.script_type
        );
    }
    let effective = effective_dependency_manifest(dep, &lib_manifest);
    effective
        .check_engine_compatibility(engine_version)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    Ok(lib_manifest)
}

// ============================================================================
// Manifest — 脚本元数据
// ============================================================================

/// 脚本 manifest.json 结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// manifest 格式版本
    pub schema_version: u32,

    /// 唯一标识（英文，snake_case）
    pub name: String,

    /// 显示名称（中文）
    pub display_name: String,

    /// 语义化版本号
    pub version: String,

    /// 作者
    #[serde(default)]
    pub author: String,

    /// 描述
    #[serde(default)]
    pub description: String,

    /// 图标路径
    pub icon: Option<String>,

    /// 脚本类型
    #[serde(rename = "type")]
    pub script_type: ScriptType,

    /// 入口 JS 文件
    pub entry: String,

    /// 配置 Schema 文件路径
    pub settings_ui: Option<String>,

    /// 标签
    #[serde(default)]
    pub tags: Vec<String>,

    /// 分类
    pub category: Option<String>,

    /// 参数 Schema（JSON Schema 格式）
    #[serde(default)]
    pub params_schema: Option<serde_json::Value>,

    // === 版本约束 ===
    #[serde(default)]
    pub min_engine_version: Option<String>,

    #[serde(default)]
    pub max_engine_version: Option<String>,

    #[serde(default)]
    pub engine_version: Option<String>,

    /// Library dependencies (paths relative to data root); see [`ScriptDependency`].
    #[serde(default)]
    pub dependencies: Vec<ScriptDependency>,

    /// Design resolution `[width, height]` for coordinate scaling.
    /// When set, all coordinates passed to the engine are auto-scaled from this resolution
    /// to the actual capture frame size. `None` means no scaling.
    #[serde(default)]
    pub design_resolution: Option<[u32; 2]>,
}

impl Manifest {
    /// 从 JSON 文件加载 manifest。
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let manifest: Manifest = serde_json::from_str(&content)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// 验证 manifest 字段。
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            bail!("Unsupported schema version: {}", self.schema_version);
        }
        if self.name.is_empty() {
            bail!("manifest.json: 'name' is required");
        }
        if self.display_name.is_empty() {
            bail!("manifest.json: 'display_name' is required");
        }
        if self.version.is_empty() {
            bail!("manifest.json: 'version' is required");
        }
        if self.entry.is_empty() {
            bail!("manifest.json: 'entry' is required");
        }

        Version::parse(&self.version)
            .map_err(|e| anyhow::anyhow!("Invalid script version '{}': {}", self.version, e))?;

        for dep in &self.dependencies {
            dep.validate_path()?;
        }

        Ok(())
    }

    /// 检查脚本是否与当前引擎版本兼容。
    pub fn check_engine_compatibility(&self, current_engine_version: &str) -> Result<()> {
        let current = Version::parse(current_engine_version).map_err(|e| {
            anyhow::anyhow!("Invalid engine version '{}': {}", current_engine_version, e)
        })?;

        let req = self.get_version_requirement()?;

        if req.matches(&current) {
            Ok(())
        } else {
            bail!(
                "Script '{}' requires engine {}, but current engine is {}",
                self.name,
                self.format_requirement(),
                current_engine_version
            );
        }
    }

    fn get_version_requirement(&self) -> Result<VersionReq> {
        if let Some(ref engine_ver) = self.engine_version {
            return VersionReq::parse(engine_ver)
                .map_err(|e| anyhow::anyhow!("Invalid engine_version '{}': {}", engine_ver, e));
        }

        let min = self.min_engine_version.as_deref().unwrap_or("0.0.0");
        let max = self.max_engine_version.as_deref();

        let range = match max {
            Some(max) => format!(">={}, <{}", min, max),
            None => format!(">={}", min),
        };

        VersionReq::parse(&range)
            .map_err(|e| anyhow::anyhow!("Invalid version range '{}': {}", range, e))
    }

    pub fn format_requirement(&self) -> String {
        if let Some(ref engine_ver) = self.engine_version {
            return engine_ver.clone();
        }

        let min = self.min_engine_version.as_deref().unwrap_or("0.0.0");
        match &self.max_engine_version {
            Some(max) => format!(">= {}, < {}", min, max),
            None => format!(">= {}", min),
        }
    }
}

// ============================================================================
// EngineVersionReq — 引擎版本约束工具
// ============================================================================

pub struct EngineVersionReq {
    current_version: Version,
}

impl EngineVersionReq {
    pub fn new(current_version: &str) -> Result<Self> {
        Ok(Self {
            current_version: Version::parse(current_version)?,
        })
    }

    pub fn check(&self, manifest: &Manifest) -> bool {
        manifest
            .check_engine_compatibility(&self.current_version.to_string())
            .is_ok()
    }

    pub fn filter_compatible(&self, manifests: Vec<Manifest>) -> Vec<Manifest> {
        manifests.into_iter().filter(|m| self.check(m)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> Manifest {
        Manifest {
            schema_version: 1,
            name: "test_script".into(),
            display_name: "Test Script".into(),
            version: "1.0.0".into(),
            author: String::new(),
            description: String::new(),
            icon: None,
            script_type: ScriptType::SoloTask,
            entry: "main.js".into(),
            settings_ui: None,
            tags: vec![],
            category: None,
            params_schema: None,
            min_engine_version: None,
            max_engine_version: None,
            engine_version: None,
            dependencies: vec![],
            design_resolution: None,
        }
    }

    // ── validate() ──

    #[test]
    fn test_validate_valid_manifest() {
        assert!(valid_manifest().validate().is_ok());
    }

    #[test]
    fn test_validate_wrong_schema_version() {
        let m = Manifest {
            schema_version: 2,
            ..valid_manifest()
        };
        assert!(m.validate().is_err());
    }

    #[test]
    fn test_validate_empty_name() {
        let m = Manifest {
            name: String::new(),
            ..valid_manifest()
        };
        assert!(m.validate().is_err());
    }

    #[test]
    fn test_validate_empty_display_name() {
        let m = Manifest {
            display_name: String::new(),
            ..valid_manifest()
        };
        assert!(m.validate().is_err());
    }

    #[test]
    fn test_validate_empty_version() {
        let m = Manifest {
            version: String::new(),
            ..valid_manifest()
        };
        assert!(m.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_semver() {
        let m = Manifest {
            version: "not-a-version".into(),
            ..valid_manifest()
        };
        assert!(m.validate().is_err());
    }

    #[test]
    fn test_validate_empty_entry() {
        let m = Manifest {
            entry: String::new(),
            ..valid_manifest()
        };
        assert!(m.validate().is_err());
    }

    // ── check_engine_compatibility() ──

    #[test]
    fn test_engine_compatibility_matching() {
        let m = Manifest {
            min_engine_version: Some("1.0.0".into()),
            max_engine_version: Some("2.0.0".into()),
            ..valid_manifest()
        };
        assert!(m.check_engine_compatibility("1.5.0").is_ok());
    }

    #[test]
    fn test_engine_compatibility_too_old() {
        let m = Manifest {
            min_engine_version: Some("2.0.0".into()),
            ..valid_manifest()
        };
        assert!(m.check_engine_compatibility("1.0.0").is_err());
    }

    #[test]
    fn test_engine_compatibility_too_new() {
        let m = Manifest {
            max_engine_version: Some("2.0.0".into()),
            ..valid_manifest()
        };
        assert!(m.check_engine_compatibility("3.0.0").is_err());
    }

    #[test]
    fn test_engine_compatibility_engine_version_override() {
        let m = Manifest {
            engine_version: Some("^1.5.0".into()),
            min_engine_version: Some("0.0.1".into()),
            ..valid_manifest()
        };
        assert!(m.check_engine_compatibility("1.6.0").is_ok());
        assert!(m.check_engine_compatibility("2.0.0").is_err());
    }

    #[test]
    fn test_engine_compatibility_invalid_engine_version() {
        assert!(valid_manifest()
            .check_engine_compatibility("not-a-version")
            .is_err());
    }

    #[test]
    fn test_engine_compatibility_no_constraints() {
        assert!(valid_manifest().check_engine_compatibility("1.0.0").is_ok());
    }

    // ── format_requirement() ──

    #[test]
    fn test_format_requirement_no_constraints() {
        assert_eq!(valid_manifest().format_requirement(), ">= 0.0.0");
    }

    #[test]
    fn test_format_requirement_min_only() {
        let m = Manifest {
            min_engine_version: Some("1.0.0".into()),
            ..valid_manifest()
        };
        assert_eq!(m.format_requirement(), ">= 1.0.0");
    }

    #[test]
    fn test_format_requirement_min_max() {
        let m = Manifest {
            min_engine_version: Some("1.0.0".into()),
            max_engine_version: Some("2.0.0".into()),
            ..valid_manifest()
        };
        assert_eq!(m.format_requirement(), ">= 1.0.0, < 2.0.0");
    }

    #[test]
    fn test_format_requirement_engine_version() {
        let m = Manifest {
            engine_version: Some("^1.5.0".into()),
            ..valid_manifest()
        };
        assert_eq!(m.format_requirement(), "^1.5.0");
    }

    // ── EngineVersionReq ──

    #[test]
    fn test_engine_version_req_new_valid() {
        assert!(EngineVersionReq::new("1.0.0").is_ok());
    }

    #[test]
    fn test_engine_version_req_new_invalid() {
        assert!(EngineVersionReq::new("not-a-version").is_err());
    }

    #[test]
    fn test_engine_version_req_check_compatible() {
        let req = EngineVersionReq::new("1.5.0").unwrap();
        let m = Manifest {
            min_engine_version: Some("1.0.0".into()),
            max_engine_version: Some("2.0.0".into()),
            ..valid_manifest()
        };
        assert!(req.check(&m));
    }

    #[test]
    fn test_engine_version_req_check_incompatible() {
        let req = EngineVersionReq::new("3.0.0").unwrap();
        let m = Manifest {
            min_engine_version: Some("1.0.0".into()),
            max_engine_version: Some("2.0.0".into()),
            ..valid_manifest()
        };
        assert!(!req.check(&m));
    }

    #[test]
    fn test_engine_version_req_filter_compatible() {
        let req = EngineVersionReq::new("1.5.0").unwrap();
        let m1 = Manifest {
            min_engine_version: Some("1.0.0".into()),
            ..valid_manifest()
        };
        let m2 = Manifest {
            min_engine_version: Some("2.0.0".into()),
            ..valid_manifest()
        };
        let m3 = Manifest {
            max_engine_version: Some("1.8.0".into()),
            ..valid_manifest()
        };
        let filtered = req.filter_compatible(vec![m1, m2, m3]);
        assert_eq!(filtered.len(), 2);
    }

    // ── from_file ──

    #[test]
    fn test_from_file_nonexistent() {
        let result = Manifest::from_file(Path::new("/nonexistent/path/manifest.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_from_file_valid_json() {
        let dir = std::env::temp_dir().join("betternte_test_manifest");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("manifest.json");
        let m = valid_manifest();
        let json = serde_json::to_string_pretty(&m).unwrap();
        std::fs::write(&path, json).unwrap();
        let loaded = Manifest::from_file(&path).unwrap();
        assert_eq!(loaded.name, "test_script");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_from_file_invalid_json() {
        let dir = std::env::temp_dir().join("betternte_test_manifest_invalid");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("manifest.json");
        std::fs::write(&path, "not json").unwrap();
        let result = Manifest::from_file(&path);
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
