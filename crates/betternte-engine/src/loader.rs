//! 脚本加载器 — 扫描脚本目录，解析 manifest.json。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 脚本类型（兼容现有脚本格式）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScriptType {
    Task,
    Trigger,
    Library,
}

/// 触发器可见范围。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TriggerVisibility {
    /// 全局可见，在触发器页面展示。
    #[default]
    Global,
    /// 仅工作流内可用，不在触发器页面展示。
    Workflow,
}

impl ScriptType {
    /// 从字符串解析，兼容 "solo_task" → Task 的旧格式。
    pub fn from_str_flexible(s: &str) -> Option<Self> {
        match s {
            "task" | "solo_task" | "flow" => Some(Self::Task),
            "trigger" => Some(Self::Trigger),
            "library" => Some(Self::Library),
            _ => None,
        }
    }
}

impl std::fmt::Display for ScriptType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Task => write!(f, "task"),
            Self::Trigger => write!(f, "trigger"),
            Self::Library => write!(f, "library"),
        }
    }
}

/// 脚本清单（灵活反序列化，兼容多种格式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "RawManifest")]
pub struct ScriptManifest {
    pub schema_version: u32,
    pub name: String,
    pub display_name: String,
    pub version: String,
    #[serde(rename = "type")]
    pub script_type: ScriptType,
    pub entry: String,
    pub author: String,
    pub description: String,
    pub tags: Vec<String>,
    pub params_schema: Option<serde_json::Value>,
    /// 脚本输出的 JSON Schema（用于 Flow 中下游步骤引用）。
    pub output_schema: Option<serde_json::Value>,
    /// 触发器可见范围（仅对 trigger 类型有效）。
    #[serde(default)]
    pub visibility: TriggerVisibility,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub min_engine_version: Option<String>,
}

/// 原始清单（用于灵活反序列化）。
#[derive(Deserialize)]
struct RawManifest {
    #[serde(default)]
    schema_version: u32,
    name: String,
    display_name: String,
    version: String,
    #[serde(rename = "type")]
    script_type: String,
    entry: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    params_schema: Option<serde_json::Value>,
    #[serde(default)]
    output_schema: Option<serde_json::Value>,
    #[serde(default)]
    visibility: TriggerVisibility,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    min_engine_version: Option<String>,
}

impl From<RawManifest> for ScriptManifest {
    fn from(raw: RawManifest) -> Self {
        Self {
            schema_version: raw.schema_version,
            name: raw.name,
            display_name: raw.display_name,
            version: raw.version,
            script_type: ScriptType::from_str_flexible(&raw.script_type)
                .unwrap_or(ScriptType::Task),
            entry: raw.entry,
            author: raw.author,
            description: raw.description,
            tags: raw.tags,
            params_schema: raw.params_schema,
            output_schema: raw.output_schema,
            visibility: raw.visibility,
            category: raw.category,
            min_engine_version: raw.min_engine_version,
        }
    }
}

/// 已加载的脚本条目。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScriptEntry {
    pub path: PathBuf,
    /// 相对于 data_root 的路径，如 "main/scripts/hello_world"。
    pub dir: String,
    pub manifest: ScriptManifest,
    pub compatible: bool,
    pub loaded: bool,
    /// 所属订阅源名称。
    pub source: String,
}

/// 扫描脚本目录，解析每个子目录中的 manifest.json。
///
/// 跳过没有 manifest.json 或解析失败的目录。
/// `source` 标记所属订阅源名称。
/// `data_root` 用于计算相对路径 `dir` 字段。
pub fn load_scripts(dir: &Path, source: &str, data_root: &Path) -> Vec<ScriptEntry> {
    let mut entries = Vec::new();

    tracing::info!(path = %dir.display(), source, "Scanning scripts directory");
    if !dir.exists() {
        return entries;
    }

    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let read_dir = match std::fs::read_dir(&current) {
            Ok(rd) => rd,
            Err(e) => {
                tracing::warn!(path = %current.display(), error = %e, "Failed to read scripts directory");
                continue;
            }
        };

        for entry in read_dir {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            let name = entry.file_name();
            if !path.is_dir() {
                continue;
            }

            let manifest_path = path.join("manifest.json");
            if manifest_path.exists() {
                match load_manifest(&manifest_path) {
                    Ok(manifest) => {
                        tracing::info!(
                            name = %manifest.name,
                            version = %manifest.version,
                            script_type = %manifest.script_type,
                            source,
                            path = %path.display(),
                            "Script loaded"
                        );
                        let dir = path
                            .strip_prefix(data_root)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .replace('\\', "/");
                        entries.push(ScriptEntry {
                            path,
                            dir,
                            manifest,
                            compatible: true,
                            loaded: true,
                            source: source.to_string(),
                        });
                    }
                    Err(e) => {
                        tracing::warn!(
                            dir = %name.to_string_lossy(),
                            error = %e,
                            "Failed to parse manifest.json"
                        );
                    }
                }
                continue;
            }

            stack.push(path);
        }
    }

    tracing::info!(count = entries.len(), source, "Scripts loaded");
    entries
}

fn load_manifest(path: &Path) -> Result<ScriptManifest, anyhow::Error> {
    let content = std::fs::read_to_string(path)?;
    let manifest: ScriptManifest = serde_json::from_str(&content)?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_trigger_manifest() {
        let json = r#"{
            "schema_version": 1,
            "name": "auto_pick",
            "display_name": "自动拾取",
            "version": "1.0.0",
            "type": "trigger",
            "entry": "main.js",
            "author": "Test",
            "tags": ["trigger", "pickup"],
            "permissions": ["screenshot", "click"],
            "params_schema": { "type": "object", "properties": {} }
        }"#;

        let manifest: ScriptManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "auto_pick");
        assert_eq!(manifest.script_type, ScriptType::Trigger);
        assert!(manifest.params_schema.is_some());
    }

    #[test]
    fn test_parse_solo_task_as_task() {
        let json = r#"{
            "schema_version": 1,
            "name": "hello",
            "display_name": "Hello",
            "version": "1.0.0",
            "type": "solo_task",
            "entry": "main.js"
        }"#;

        let manifest: ScriptManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.script_type, ScriptType::Task);
    }
    }
}
