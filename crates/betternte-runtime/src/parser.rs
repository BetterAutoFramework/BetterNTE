//! Flow 解析器 — JSON → 结构体 + 验证

use indexmap::IndexMap;
use std::collections::HashMap;
use std::path::Path;

use crate::error::{FlowError, FlowResult};
use crate::types::{Flow, FlowOrchestration, Group, Step, Transition};

/// Flow 解析器
pub struct FlowParser {
    /// 最大嵌套深度
    max_depth: usize,
}

impl FlowParser {
    /// 创建新的解析器
    pub fn new() -> Self {
        Self { max_depth: 5 }
    }

    /// 设置最大嵌套深度
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// 从 JSON 字符串解析 Flow
    pub fn parse_str(&self, json: &str) -> FlowResult<Flow> {
        let flow: Flow = serde_json::from_str(json)?;
        self.validate(&flow)?;
        Ok(flow)
    }

    /// 兼容解析入口：优先按 Flow 解析，失败后按 Group 解析并转换。
    pub fn parse_or_convert_to_flow(&self, json: &str) -> FlowResult<Flow> {
        match self.parse_str(json) {
            Ok(flow) => Ok(flow),
            Err(flow_err) => {
                let group: Group = serde_json::from_str(json).map_err(|group_err| {
                    crate::error::FlowError::Other(anyhow::anyhow!(
                        "Failed to parse as Flow ({}) or Group ({})",
                        flow_err,
                        group_err
                    ))
                })?;
                self.parse_group(&group)
            }
        }
    }

    /// 从文件解析 Flow
    pub async fn parse_file(&self, path: &Path) -> FlowResult<Flow> {
        let content = tokio::fs::read_to_string(path).await?;
        self.parse_str(&content)
    }

    /// 解析 Group 为 Flow（语法糖展开）
    pub fn parse_group(&self, group: &Group) -> FlowResult<Flow> {
        let mut steps = IndexMap::new();

        for (i, step) in group.steps.iter().enumerate() {
            let id = step.alias.clone();
            let next_target = group.steps.get(i + 1).map(|s| s.alias.clone());

            let transitions = match next_target {
                Some(target) => vec![Transition {
                    target,
                    condition: crate::types::Condition::Always,
                    priority: 0,
                    interrupt: false,
                }],
                None => vec![],
            };

            let flow_step = Step {
                kind: step.kind.clone(),
                input: step.input.clone(),
                output: HashMap::new(),
                transitions,
                timeout_ms: step.timeout_ms,
                max_retries: step.max_retries,
                on_error: None,
            };

            steps.insert(id, flow_step);
        }

        let entry = group
            .steps
            .first()
            .map(|s| s.alias.clone())
            .unwrap_or_default();

        let flow_id = if group.uuid.is_empty() {
            group.name.clone()
        } else {
            group.uuid.clone()
        };

        Ok(Flow {
            id: flow_id,
            name: group.name.clone(),
            description: group.description.clone(),
            version: "1.0.0".to_string(),
            entry,
            steps,
            variables: HashMap::new(),
            tags: vec![],
            output_schema: None,
            orchestration: Some(FlowOrchestration {
                mode: Some(group.mode.clone()),
                retry_count: Some(group.retry_count),
                error_handling: group.error_handling.clone(),
                retry: group.retry.clone(),
                notify_on_failure: group.notify_on_failure,
                schedule: group.schedule.clone(),
                repeat_strategy: group.repeat_strategy.clone(),
                source: group
                    .source
                    .clone()
                    .or_else(|| Some("legacy_task_group".to_string())),
            }),
        })
    }

    /// 验证 Flow
    pub fn validate(&self, flow: &Flow) -> FlowResult<()> {
        // 1. 检查 steps 非空
        if flow.steps.is_empty() {
            return Err(FlowError::EmptySteps);
        }

        // 2. 检查 entry 存在
        if !flow.steps.contains_key(&flow.entry) {
            return Err(FlowError::EntryNotFound(flow.entry.clone()));
        }

        // 3. 检查所有 transition 目标存在
        for (step_id, step) in &flow.steps {
            for trans in &step.transitions {
                if !flow.steps.contains_key(&trans.target) {
                    return Err(FlowError::StepNotFound(format!(
                        "Step '{}' references non-existent target '{}'",
                        step_id, trans.target
                    )));
                }
            }
            if let Some(on_error) = &step.on_error {
                if !flow.steps.contains_key(on_error) {
                    return Err(FlowError::StepNotFound(format!(
                        "Step '{}' references non-existent on_error '{}'",
                        step_id, on_error
                    )));
                }
            }
        }

        // 4. 检测循环引用 (DFS)
        self.detect_cycles(flow)?;

        Ok(())
    }

    /// 检测循环引用
    fn detect_cycles(&self, flow: &Flow) -> FlowResult<()> {
        let mut visited = HashMap::new(); // 0: 未访问, 1: 访问中, 2: 已完成

        for step_id in flow.steps.keys() {
            if !visited.contains_key(step_id) {
                self.dfs(flow, step_id, &mut visited, &mut Vec::new())?;
            }
        }

        Ok(())
    }

    /// DFS 遍历检测环
    fn dfs(
        &self,
        flow: &Flow,
        step_id: &str,
        visited: &mut HashMap<String, u8>,
        path: &mut Vec<String>,
    ) -> FlowResult<()> {
        visited.insert(step_id.to_string(), 1); // 标记为访问中
        path.push(step_id.to_string());

        if let Some(step) = flow.steps.get(step_id) {
            for trans in &step.transitions {
                let target = &trans.target;
                match visited.get(target) {
                    Some(1) => {
                        // 发现环
                        path.push(target.clone());
                        return Err(FlowError::CircularDependency(path.join(" -> ")));
                    }
                    Some(2) => {
                        // 已完成，跳过
                    }
                    _ => {
                        self.dfs(flow, target, visited, path)?;
                    }
                }
            }
        }

        path.pop();
        visited.insert(step_id.to_string(), 2); // 标记为已完成
        Ok(())
    }
}

impl Default for FlowParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_flow() {
        let json = r#"{
            "id": "test",
            "name": "Test",
            "entry": "start",
            "steps": {
                "start": {
                    "kind": { "type": "wait", "ms": 100 },
                    "transitions": [
                        { "target": "end", "condition": { "type": "always" } }
                    ]
                },
                "end": {
                    "kind": { "type": "none" },
                    "transitions": []
                }
            }
        }"#;

        let parser = FlowParser::new();
        let flow = parser.parse_str(json).unwrap();
        assert_eq!(flow.steps.len(), 2);
    }

    #[test]
    fn test_parse_empty_steps() {
        let json = r#"{
            "id": "test",
            "name": "Test",
            "entry": "start",
            "steps": {}
        }"#;

        let parser = FlowParser::new();
        let result = parser.parse_str(json);
        assert!(matches!(result, Err(FlowError::EmptySteps)));
    }

    #[test]
    fn test_parse_missing_entry() {
        let json = r#"{
            "id": "test",
            "name": "Test",
            "entry": "nonexistent",
            "steps": {
                "start": {
                    "kind": { "type": "none" },
                    "transitions": []
                }
            }
        }"#;

        let parser = FlowParser::new();
        let result = parser.parse_str(json);
        assert!(matches!(result, Err(FlowError::EntryNotFound(_))));
    }

    #[test]
    fn test_parse_invalid_target() {
        let json = r#"{
            "id": "test",
            "name": "Test",
            "entry": "start",
            "steps": {
                "start": {
                    "kind": { "type": "none" },
                    "transitions": [
                        { "target": "nonexistent", "condition": { "type": "always" } }
                    ]
                }
            }
        }"#;

        let parser = FlowParser::new();
        let result = parser.parse_str(json);
        assert!(matches!(result, Err(FlowError::StepNotFound(_))));
    }

    #[test]
    fn test_detect_cycle() {
        let json = r#"{
            "id": "test",
            "name": "Test",
            "entry": "a",
            "steps": {
                "a": {
                    "kind": { "type": "none" },
                    "transitions": [
                        { "target": "b", "condition": { "type": "always" } }
                    ]
                },
                "b": {
                    "kind": { "type": "none" },
                    "transitions": [
                        { "target": "a", "condition": { "type": "always" } }
                    ]
                }
            }
        }"#;

        let parser = FlowParser::new();
        let result = parser.parse_str(json);
        assert!(matches!(result, Err(FlowError::CircularDependency(_))));
    }

    #[test]
    fn test_parse_group() {
        let json = r#"{
            "name": "test",
            "steps": [
                { "type": "script", "script": "a", "alias": "step1" },
                { "type": "wait", "ms": 100, "alias": "step2" },
                { "type": "none", "alias": "step3" }
            ]
        }"#;

        let group: Group = serde_json::from_str(json).unwrap();
        let parser = FlowParser::new();
        let flow = parser.parse_group(&group).unwrap();

        assert_eq!(flow.steps.len(), 3);
        assert_eq!(flow.entry, "step1");

        // 检查 transition 链
        let step1 = flow.steps.get("step1").unwrap();
        assert_eq!(step1.transitions.len(), 1);
        assert_eq!(step1.transitions[0].target, "step2");

        let step2 = flow.steps.get("step2").unwrap();
        assert_eq!(step2.transitions.len(), 1);
        assert_eq!(step2.transitions[0].target, "step3");

        let step3 = flow.steps.get("step3").unwrap();
        assert!(step3.transitions.is_empty());
        let orchestration = flow
            .orchestration
            .as_ref()
            .expect("group should carry orchestration");
        assert_eq!(orchestration.mode.as_deref(), Some("sequential"));
        assert_eq!(orchestration.source.as_deref(), Some("legacy_task_group"));
    }

    #[test]
    fn test_parse_or_convert_to_flow_from_group() {
        let json = r#"{
            "uuid": "legacy-group",
            "name": "legacy-group",
            "mode": "random",
            "retry_count": 2,
            "steps": [
                { "type": "script", "script": "a", "alias": "s1" },
                { "type": "none", "alias": "s2" }
            ]
        }"#;

        let parser = FlowParser::new();
        let flow = parser.parse_or_convert_to_flow(json).unwrap();
        assert_eq!(flow.id, "legacy-group");
        assert_eq!(flow.entry, "s1");
        assert!(flow.steps.contains_key("s2"));
        let orchestration = flow.orchestration.as_ref().expect("missing orchestration");
        assert_eq!(orchestration.mode.as_deref(), Some("random"));
        assert_eq!(orchestration.retry_count, Some(2));
    }
}
