//! 条件评估引擎

use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

use crate::condition_handlers::ConditionHandler;
use crate::error::FlowResult;

use crate::types::{CompareOp, Condition};
use crate::variables::VariableStore;

/// 条件评估器
pub struct ConditionEvaluator<'a> {
    variables: &'a VariableStore,
    handlers: Vec<&'a dyn ConditionHandler>,
}

impl<'a> ConditionEvaluator<'a> {
    /// 创建新的条件评估器（无外部处理器，仅内置条件）
    pub fn new(variables: &'a VariableStore) -> Self {
        Self {
            variables,
            handlers: Vec::new(),
        }
    }

    /// 创建带外部处理器的条件评估器
    pub fn with_handlers(
        variables: &'a VariableStore,
        handlers: &'a [std::sync::Arc<dyn ConditionHandler>],
    ) -> Self {
        Self {
            variables,
            handlers: handlers.iter().map(|h| h.as_ref()).collect(),
        }
    }

    /// 评估条件（使用 Box::pin 处理递归）
    pub fn evaluate<'b>(
        &'b self,
        condition: &'b Condition,
    ) -> Pin<Box<dyn Future<Output = FlowResult<bool>> + Send + 'b>> {
        Box::pin(async move { self.evaluate_inner(condition).await })
    }

    async fn evaluate_inner(&self, condition: &Condition) -> FlowResult<bool> {
        match condition {
            Condition::Always => Ok(true),

            // Dispatch to external handlers
            Condition::Template { .. }
            | Condition::Ocr { .. }
            | Condition::Color { .. }
            | Condition::Hotkey { .. }
            | Condition::Script { .. } => {
                for handler in &self.handlers {
                    let matched = matches!(
                        (handler.condition_type(), condition),
                        ("template", Condition::Template { .. })
                            | ("ocr", Condition::Ocr { .. })
                            | ("color", Condition::Color { .. })
                            | ("hotkey", Condition::Hotkey { .. })
                            | ("script", Condition::Script { .. })
                    );
                    if matched {
                        return handler.evaluate(condition, self.variables).await;
                    }
                }
                // No matching handler — log and return false (same as old stub behavior)
                tracing::debug!("No handler for condition: {:?}", condition);
                Ok(false)
            }

            Condition::Variable { key, op, value } => self.evaluate_variable(key, op, value).await,

            Condition::And(conditions) => {
                for cond in conditions {
                    if !self.evaluate(cond).await? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }

            Condition::Or(conditions) => {
                for cond in conditions {
                    if self.evaluate(cond).await? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }

            Condition::Not(inner) => {
                let result = self.evaluate(inner).await?;
                Ok(!result)
            }
        }
    }

    /// 评估变量条件
    async fn evaluate_variable(
        &self,
        key: &str,
        op: &CompareOp,
        expected: &Value,
    ) -> FlowResult<bool> {
        // 去掉 $variables. 前缀
        let store_key = crate::variables::resolve_variable_ref(key).unwrap_or(key);
        let actual = self.variables.get(store_key).await;

        let actual = match actual {
            Some(v) => v,
            None => {
                tracing::warn!("Variable not found: {}", key);
                return Ok(false);
            }
        };

        let result =
            match op {
                CompareOp::Eq => actual == *expected,
                CompareOp::Ne => actual != *expected,
                CompareOp::Gt => compare_values(&actual, expected)
                    .is_some_and(|o| o == std::cmp::Ordering::Greater),
                CompareOp::Lt => {
                    compare_values(&actual, expected).is_some_and(|o| o == std::cmp::Ordering::Less)
                }
                CompareOp::Gte => {
                    compare_values(&actual, expected).is_some_and(|o| o != std::cmp::Ordering::Less)
                }
                CompareOp::Lte => compare_values(&actual, expected)
                    .is_some_and(|o| o != std::cmp::Ordering::Greater),
                CompareOp::In => {
                    if let Value::Array(arr) = expected {
                        arr.contains(&actual)
                    } else {
                        false
                    }
                }
                CompareOp::Contains => {
                    if let Value::String(s) = &actual {
                        if let Value::String(pattern) = expected {
                            s.contains(pattern.as_str())
                        } else {
                            false
                        }
                    } else if let Value::Array(arr) = &actual {
                        arr.contains(expected)
                    } else {
                        false
                    }
                }
            };

        tracing::debug!(
            "Variable condition: {} {:?} {} = {}",
            key,
            op,
            expected,
            result
        );

        Ok(result)
    }
}

/// 比较两个 JSON 值
///
/// 跨类型比较返回 `None`，表示无法比较（所有比较运算符应返回 false）。
fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let a = a.as_f64().unwrap_or(0.0);
            let b = b.as_f64().unwrap_or(0.0);
            a.partial_cmp(&b)
        }
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
        (Value::Null, Value::Null) => Some(std::cmp::Ordering::Equal),
        // 跨类型比较：无法比较
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_always_condition() {
        let defs = HashMap::new();
        let store = VariableStore::new("test".to_string(), defs, None);
        store.initialize().await.unwrap();

        let evaluator = ConditionEvaluator::new(&store);
        assert!(evaluator.evaluate(&Condition::Always).await.unwrap());
    }

    #[tokio::test]
    async fn test_variable_eq() {
        let mut defs = HashMap::new();
        defs.insert(
            "hp".to_string(),
            crate::types::VariableDef {
                value_type: "integer".to_string(),
                default: Some(serde_json::json!(100)),
                persist: false,
                schema: None,
            },
        );

        let store = VariableStore::new("test".to_string(), defs, None);
        store.initialize().await.unwrap();

        let evaluator = ConditionEvaluator::new(&store);

        let cond = Condition::Variable {
            key: "$variables.hp".to_string(),
            op: CompareOp::Eq,
            value: serde_json::json!(100),
        };
        assert!(evaluator.evaluate(&cond).await.unwrap());

        let cond = Condition::Variable {
            key: "$variables.hp".to_string(),
            op: CompareOp::Eq,
            value: serde_json::json!(200),
        };
        assert!(!evaluator.evaluate(&cond).await.unwrap());
    }

    #[tokio::test]
    async fn test_variable_gt() {
        let mut defs = HashMap::new();
        defs.insert(
            "stamina".to_string(),
            crate::types::VariableDef {
                value_type: "integer".to_string(),
                default: Some(serde_json::json!(50)),
                persist: false,
                schema: None,
            },
        );

        let store = VariableStore::new("test".to_string(), defs, None);
        store.initialize().await.unwrap();

        let evaluator = ConditionEvaluator::new(&store);

        let cond = Condition::Variable {
            key: "$variables.stamina".to_string(),
            op: CompareOp::Gt,
            value: serde_json::json!(0),
        };
        assert!(evaluator.evaluate(&cond).await.unwrap());

        let cond = Condition::Variable {
            key: "$variables.stamina".to_string(),
            op: CompareOp::Gt,
            value: serde_json::json!(100),
        };
        assert!(!evaluator.evaluate(&cond).await.unwrap());
    }

    #[tokio::test]
    async fn test_and_condition() {
        let defs = HashMap::new();
        let store = VariableStore::new("test".to_string(), defs, None);
        store.initialize().await.unwrap();

        let evaluator = ConditionEvaluator::new(&store);

        let cond = Condition::And(vec![
            Box::new(Condition::Always),
            Box::new(Condition::Always),
        ]);
        assert!(evaluator.evaluate(&cond).await.unwrap());

        let cond = Condition::And(vec![
            Box::new(Condition::Always),
            Box::new(Condition::Variable {
                key: "$variables.x".to_string(),
                op: CompareOp::Eq,
                value: serde_json::json!(1),
            }),
        ]);
        assert!(!evaluator.evaluate(&cond).await.unwrap());
    }

    #[tokio::test]
    async fn test_or_condition() {
        let defs = HashMap::new();
        let store = VariableStore::new("test".to_string(), defs, None);
        store.initialize().await.unwrap();

        let evaluator = ConditionEvaluator::new(&store);

        let cond = Condition::Or(vec![
            Box::new(Condition::Always),
            Box::new(Condition::Variable {
                key: "$variables.x".to_string(),
                op: CompareOp::Eq,
                value: serde_json::json!(1),
            }),
        ]);
        assert!(evaluator.evaluate(&cond).await.unwrap());
    }

    #[tokio::test]
    async fn test_not_condition() {
        let defs = HashMap::new();
        let store = VariableStore::new("test".to_string(), defs, None);
        store.initialize().await.unwrap();

        let evaluator = ConditionEvaluator::new(&store);

        let cond = Condition::Not(Box::new(Condition::Always));
        assert!(!evaluator.evaluate(&cond).await.unwrap());
    }

    #[tokio::test]
    async fn test_contains_string() {
        let mut defs = HashMap::new();
        defs.insert(
            "text".to_string(),
            crate::types::VariableDef {
                value_type: "string".to_string(),
                default: Some(serde_json::json!("Hello World")),
                persist: false,
                schema: None,
            },
        );

        let store = VariableStore::new("test".to_string(), defs, None);
        store.initialize().await.unwrap();

        let evaluator = ConditionEvaluator::new(&store);

        let cond = Condition::Variable {
            key: "$variables.text".to_string(),
            op: CompareOp::Contains,
            value: serde_json::json!("World"),
        };
        assert!(evaluator.evaluate(&cond).await.unwrap());
    }

    #[tokio::test]
    async fn test_handler_dispatch() {
        use crate::condition_handlers::TemplateConditionHandler;

        let defs = HashMap::new();
        let store = VariableStore::new("test".to_string(), defs, None);
        store.initialize().await.unwrap();

        let handlers: Vec<std::sync::Arc<dyn ConditionHandler>> =
            vec![std::sync::Arc::new(TemplateConditionHandler)];
        let evaluator = ConditionEvaluator::with_handlers(&store, &handlers);

        let cond = Condition::Template {
            template: "button.png".to_string(),
            threshold: 0.9,
            roi: None,
        };
        // Stub handler returns false
        assert!(!evaluator.evaluate(&cond).await.unwrap());
    }
}
