//! 变量系统 — 双层结构 (Flow 级共享 + Step 级输出)

use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::RwLock;

use crate::error::FlowResult;
use crate::types::VariableDef;

/// 变量存储
#[derive(Debug)]
pub struct VariableStore {
    /// 共享变量
    variables: RwLock<HashMap<String, Value>>,
    /// 变量声明
    definitions: HashMap<String, VariableDef>,
    /// 持久化目录
    persist_dir: Option<PathBuf>,
    /// 流程 ID (用于持久化路径)
    flow_id: String,
}

impl VariableStore {
    /// 创建新的变量存储
    pub fn new(
        flow_id: String,
        definitions: HashMap<String, VariableDef>,
        persist_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            variables: RwLock::new(HashMap::new()),
            definitions,
            persist_dir,
            flow_id,
        }
    }

    /// 初始化变量（加载默认值 + 持久化数据）
    pub async fn initialize(&self) -> FlowResult<()> {
        let mut vars = self.variables.write().await;

        // 1. 加载默认值
        for (key, def) in &self.definitions {
            if let Some(default) = &def.default {
                vars.insert(key.clone(), default.clone());
            }
        }

        // 2. 加载持久化数据
        if let Some(dir) = &self.persist_dir {
            let persist_file = dir.join(format!("{}.json", self.flow_id));
            if persist_file.exists() {
                let content = tokio::fs::read_to_string(&persist_file).await?;
                let persisted: HashMap<String, Value> = serde_json::from_str(&content)?;

                for (key, value) in persisted {
                    // 只加载声明了 persist: true 的变量
                    if let Some(def) = self.definitions.get(&key) {
                        if def.persist {
                            vars.insert(key, value);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// 读取变量
    pub async fn get(&self, key: &str) -> Option<Value> {
        let vars = self.variables.read().await;
        vars.get(key).cloned()
    }

    /// 写入变量
    pub async fn set(&self, key: String, value: Value) -> FlowResult<()> {
        // 类型校验
        if let Some(def) = self.definitions.get(&key) {
            self.validate_type(&key, &value, def)?;
        }

        let mut vars = self.variables.write().await;
        vars.insert(key, value);
        Ok(())
    }

    /// 批量写入变量
    pub async fn set_batch(&self, entries: HashMap<String, Value>) -> FlowResult<()> {
        for (key, value) in entries {
            self.set(key, value).await?;
        }
        Ok(())
    }

    /// 持久化变量
    pub async fn persist(&self) -> FlowResult<()> {
        if let Some(dir) = &self.persist_dir {
            // 确保目录存在
            tokio::fs::create_dir_all(dir).await?;

            // 只持久化声明了 persist: true 的变量
            let vars = self.variables.read().await;
            let mut to_persist = HashMap::new();

            for (key, def) in &self.definitions {
                if def.persist {
                    if let Some(value) = vars.get(key) {
                        to_persist.insert(key.clone(), value.clone());
                    }
                }
            }

            // 写入文件
            let persist_file = dir.join(format!("{}.json", self.flow_id));
            let content = serde_json::to_string_pretty(&to_persist)?;
            tokio::fs::write(&persist_file, content).await?;
        }

        Ok(())
    }

    /// 获取所有变量的快照
    pub async fn snapshot(&self) -> HashMap<String, Value> {
        let vars = self.variables.read().await;
        vars.clone()
    }

    /// 清空变量
    pub async fn clear(&self) {
        let mut vars = self.variables.write().await;
        vars.clear();
    }

    /// 校验变量类型
    fn validate_type(&self, key: &str, value: &Value, def: &VariableDef) -> FlowResult<()> {
        let actual_type = match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    "integer"
                } else {
                    "number"
                }
            }
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        };

        let expected = &def.value_type;
        let valid = match expected.as_str() {
            "integer" => actual_type == "integer" || actual_type == "number",
            "number" => actual_type == "integer" || actual_type == "number",
            "boolean" => actual_type == "boolean",
            "string" => actual_type == "string",
            "object" => actual_type == "object",
            "array" => actual_type == "array",
            _ => true, // 未知类型不校验
        };

        if valid {
            Ok(())
        } else {
            tracing::warn!(
                "Type mismatch for variable '{}': expected {}, got {}",
                key,
                expected,
                actual_type
            );
            Ok(()) // 只打印 warn，不阻断
        }
    }
}

/// 解析变量引用 ($variables.xxx)
pub fn resolve_variable_ref(reference: &str) -> Option<&str> {
    if reference.starts_with("$variables.") {
        Some(&reference[11..]) // 跳过 "$variables."
    } else {
        None
    }
}

/// 解析步骤输出引用 ($result.xxx)
pub fn resolve_output_ref(reference: &str) -> Option<&str> {
    if reference.starts_with("$result.") {
        Some(&reference[8..]) // 跳过 "$result."
    } else {
        None
    }
}

/// 解析子流程输出引用 ($flow_output.xxx)
pub fn resolve_flow_output_ref(reference: &str) -> Option<&str> {
    if reference.starts_with("$flow_output.") {
        Some(&reference[13..]) // 跳过 "$flow_output."
    } else {
        None
    }
}

/// 解析步骤输出引用 ($steps.xxx.result.yyy) → (step_id, field_name)
pub fn resolve_step_output_ref(reference: &str) -> Option<(&str, &str)> {
    // 格式: $steps.<step_id>.result.<field>
    let rest = reference.strip_prefix("$steps.")?;
    let dot_pos = rest.find('.')?;
    let step_id = &rest[..dot_pos];
    let after_dot = &rest[dot_pos + 1..];
    let field = after_dot.strip_prefix("result.")?;
    Some((step_id, field))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_variable_store_basic() {
        let defs = HashMap::new();
        let store = VariableStore::new("test".to_string(), defs, None);

        store.initialize().await.unwrap();

        store
            .set("hp".to_string(), serde_json::json!(100))
            .await
            .unwrap();
        assert_eq!(store.get("hp").await, Some(serde_json::json!(100)));
    }

    #[tokio::test]
    async fn test_variable_store_default() {
        let mut defs = HashMap::new();
        defs.insert(
            "hp".to_string(),
            VariableDef {
                value_type: "integer".to_string(),
                default: Some(serde_json::json!(100)),
                persist: false,
                schema: None,
            },
        );

        let store = VariableStore::new("test".to_string(), defs, None);
        store.initialize().await.unwrap();

        assert_eq!(store.get("hp").await, Some(serde_json::json!(100)));
    }

    #[tokio::test]
    async fn test_variable_ref_parsing() {
        assert_eq!(resolve_variable_ref("$variables.hp"), Some("hp"));
        assert_eq!(resolve_variable_ref("$variables.stamina"), Some("stamina"));
        assert_eq!(resolve_variable_ref("$result.hp"), None);
    }

    #[tokio::test]
    async fn test_output_ref_parsing() {
        assert_eq!(resolve_output_ref("$result.current"), Some("current"));
        assert_eq!(resolve_output_ref("$variables.hp"), None);
    }

    #[tokio::test]
    async fn test_type_validation_warn_only() {
        let mut defs = HashMap::new();
        defs.insert(
            "hp".to_string(),
            VariableDef {
                value_type: "integer".to_string(),
                default: None,
                persist: false,
                schema: None,
            },
        );

        let store = VariableStore::new("test".to_string(), defs, None);

        // 类型不匹配只打印 warn，不报错
        let result = store
            .set("hp".to_string(), serde_json::json!("not a number"))
            .await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_step_output_ref_parsing() {
        assert_eq!(
            resolve_step_output_ref("$steps.detect.result.hp"),
            Some(("detect", "hp"))
        );
        assert_eq!(
            resolve_step_output_ref("$steps.attack.result.damage"),
            Some(("attack", "damage"))
        );
        assert_eq!(
            resolve_step_output_ref("$steps.step1.result.value"),
            Some(("step1", "value"))
        );
        assert_eq!(resolve_step_output_ref("$variables.hp"), None);
        assert_eq!(resolve_step_output_ref("$result.hp"), None);
        assert_eq!(resolve_step_output_ref("$steps.noresult"), None);
    }

    #[test]
    fn test_flow_output_ref_parsing() {
        assert_eq!(
            resolve_flow_output_ref("$flow_output.success"),
            Some("success")
        );
        assert_eq!(
            resolve_flow_output_ref("$flow_output.result"),
            Some("result")
        );
        assert_eq!(resolve_flow_output_ref("$variables.hp"), None);
    }
}
