//! 触发器系统 — 全局/Flow级/Tag级 触发器

use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;

use crate::condition::ConditionEvaluator;
use crate::error::FlowResult;
use crate::types::{Trigger, TriggerDef, TriggerScope};
use crate::variables::VariableStore;

/// 触发器管理器
pub struct TriggerManager {
    /// 所有触发器
    triggers: Vec<Trigger>,
    /// 上次触发时间（用于 cooldown）
    last_fired: RwLock<HashMap<String, Instant>>,
}

impl TriggerManager {
    /// 从 TriggerDef 创建
    pub fn new(def: TriggerDef) -> Self {
        Self {
            triggers: def.triggers,
            last_fired: RwLock::new(HashMap::new()),
        }
    }

    /// 获取所有触发器
    pub fn triggers(&self) -> &[Trigger] {
        &self.triggers
    }

    /// 检查触发器（返回最高优先级命中的触发器）
    pub async fn check(
        &self,
        current_flow_id: Option<&str>,
        current_flow_tags: &[String],
        variables: &VariableStore,
    ) -> FlowResult<Option<&Trigger>> {
        let evaluator = ConditionEvaluator::new(variables);

        // 过滤出当前 scope 生效的触发器
        let mut active: Vec<&Trigger> = self
            .triggers
            .iter()
            .filter(|t| self.is_in_scope(t, current_flow_id, current_flow_tags))
            .collect();

        // 按优先级排序（降序）
        active.sort_by(|a, b| b.priority.cmp(&a.priority));

        // 检查条件
        for trigger in active {
            // 检查 cooldown
            if !self.check_cooldown(trigger).await {
                continue;
            }

            // 评估条件
            if evaluator.evaluate(&trigger.condition).await? {
                // 记录触发时间
                self.mark_fired(&trigger.name).await;
                return Ok(Some(trigger));
            }
        }

        Ok(None)
    }

    /// 检查触发器是否在 scope 内
    fn is_in_scope(
        &self,
        trigger: &Trigger,
        current_flow_id: Option<&str>,
        current_flow_tags: &[String],
    ) -> bool {
        match &trigger.scope {
            TriggerScope::Global => true,
            TriggerScope::Flow { flows } => {
                if let Some(flow_id) = current_flow_id {
                    flows.contains(&flow_id.to_string())
                } else {
                    false
                }
            }
            TriggerScope::Tag { tags } => tags.iter().any(|tag| current_flow_tags.contains(tag)),
        }
    }

    /// 检查 cooldown
    async fn check_cooldown(&self, trigger: &Trigger) -> bool {
        if trigger.cooldown_ms == 0 {
            return true;
        }

        let last_fired = self.last_fired.read().await;
        if let Some(last) = last_fired.get(&trigger.name) {
            last.elapsed().as_millis() >= trigger.cooldown_ms as u128
        } else {
            true
        }
    }

    /// 标记触发器已触发
    async fn mark_fired(&self, name: &str) {
        let mut last_fired = self.last_fired.write().await;
        last_fired.insert(name.to_string(), Instant::now());
    }
}

impl std::str::FromStr for TriggerManager {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let def: TriggerDef = serde_json::from_str(s)?;
        Ok(Self::new(def))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_global_trigger() {
        let json = r#"{
            "triggers": [
                {
                    "name": "stop",
                    "condition": { "type": "always" },
                    "action": { "type": "stop" },
                    "priority": 255,
                    "scope": "global"
                }
            ]
        }"#;

        let manager: TriggerManager = json.parse().unwrap();
        let variables = VariableStore::new("test".to_string(), HashMap::new(), None);
        variables.initialize().await.unwrap();

        let result = manager.check(None, &[], &variables).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "stop");
    }

    #[tokio::test]
    async fn test_flow_scope_trigger() {
        let json = r#"{
            "triggers": [
                {
                    "name": "error",
                    "condition": { "type": "always" },
                    "action": { "type": "interrupt" },
                    "priority": 200,
                    "scope": { "flow": ["dungeon_loop"] }
                }
            ]
        }"#;

        let manager: TriggerManager = json.parse().unwrap();
        let variables = VariableStore::new("test".to_string(), HashMap::new(), None);
        variables.initialize().await.unwrap();

        // 在正确 Flow 中应该触发
        let result = manager
            .check(Some("dungeon_loop"), &[], &variables)
            .await
            .unwrap();
        assert!(result.is_some());

        // 在其他 Flow 中不应该触发
        let result = manager
            .check(Some("other_flow"), &[], &variables)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cooldown() {
        let json = r#"{
            "triggers": [
                {
                    "name": "check",
                    "condition": { "type": "always" },
                    "action": { "type": "interrupt" },
                    "priority": 100,
                    "cooldown_ms": 1000,
                    "scope": "global"
                }
            ]
        }"#;

        let manager: TriggerManager = json.parse().unwrap();
        let variables = VariableStore::new("test".to_string(), HashMap::new(), None);
        variables.initialize().await.unwrap();

        // 第一次应该触发
        let result = manager.check(None, &[], &variables).await.unwrap();
        assert!(result.is_some());

        // 立即再检查，应该被 cooldown 阻止
        let result = manager.check(None, &[], &variables).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        let json = r#"{
            "triggers": [
                {
                    "name": "low",
                    "condition": { "type": "always" },
                    "action": { "type": "interrupt" },
                    "priority": 100,
                    "scope": "global"
                },
                {
                    "name": "high",
                    "condition": { "type": "always" },
                    "action": { "type": "stop" },
                    "priority": 255,
                    "scope": "global"
                }
            ]
        }"#;

        let manager: TriggerManager = json.parse().unwrap();
        let variables = VariableStore::new("test".to_string(), HashMap::new(), None);
        variables.initialize().await.unwrap();

        let result = manager.check(None, &[], &variables).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "high");
    }
}
