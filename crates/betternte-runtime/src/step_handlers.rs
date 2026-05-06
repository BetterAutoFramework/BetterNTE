//! Step handler trait and built-in handlers.
//!
//! Each handler executes one `StepKind` variant. The registry dispatches
//! to the matching handler via `step_type()`.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{FlowError, FlowResult};
use crate::executor::{FlowRunner, InputRunner, StepResult};
use crate::types::StepKind;
use crate::variables::VariableStore;

/// Context passed to step handlers during execution.
pub struct StepContext<'a> {
    pub variables: &'a VariableStore,
    pub flow_runner: Option<&'a Arc<dyn FlowRunner>>,
    pub input_runner: Option<&'a Arc<dyn InputRunner>>,
}

/// Trait for pluggable step handlers.
///
/// Each handler is responsible for one `StepKind` variant (identified by `step_type()`).
#[async_trait]
pub trait StepHandler: Send + Sync {
    /// The step variant this handler executes (e.g., "wait", "click", "script").
    fn step_type(&self) -> &str;

    /// Execute the step with the given input and context.
    async fn execute(
        &self,
        kind: &StepKind,
        input: &HashMap<String, Value>,
        ctx: &StepContext<'_>,
    ) -> FlowResult<StepResult>;
}

// ============================================================================
// Built-in handlers
// ============================================================================

pub struct WaitHandler;

#[async_trait]
impl StepHandler for WaitHandler {
    fn step_type(&self) -> &str {
        "wait"
    }

    async fn execute(
        &self,
        kind: &StepKind,
        _input: &HashMap<String, Value>,
        _ctx: &StepContext<'_>,
    ) -> FlowResult<StepResult> {
        if let StepKind::Wait { ms } = kind {
            tokio::time::sleep(std::time::Duration::from_millis(*ms)).await;
            Ok(StepResult {
                success: true,
                output: None,
                error: None,
            })
        } else {
            Err(FlowError::UnsupportedStep("WaitHandler: wrong kind".into()))
        }
    }
}

pub struct NoneHandler;

#[async_trait]
impl StepHandler for NoneHandler {
    fn step_type(&self) -> &str {
        "none"
    }

    async fn execute(
        &self,
        _kind: &StepKind,
        _input: &HashMap<String, Value>,
        _ctx: &StepContext<'_>,
    ) -> FlowResult<StepResult> {
        Ok(StepResult {
            success: true,
            output: None,
            error: None,
        })
    }
}

pub struct SetVariableHandler;

#[async_trait]
impl StepHandler for SetVariableHandler {
    fn step_type(&self) -> &str {
        "set_variable"
    }

    async fn execute(
        &self,
        kind: &StepKind,
        _input: &HashMap<String, Value>,
        ctx: &StepContext<'_>,
    ) -> FlowResult<StepResult> {
        if let StepKind::SetVariable { key, value } = kind {
            ctx.variables.set(key.clone(), value.clone()).await?;
            Ok(StepResult {
                success: true,
                output: Some(serde_json::json!({ "value": value })),
                error: None,
            })
        } else {
            Err(FlowError::UnsupportedStep(
                "SetVariableHandler: wrong kind".into(),
            ))
        }
    }
}

pub struct ClickHandler;

#[async_trait]
impl StepHandler for ClickHandler {
    fn step_type(&self) -> &str {
        "click"
    }

    async fn execute(
        &self,
        kind: &StepKind,
        _input: &HashMap<String, Value>,
        ctx: &StepContext<'_>,
    ) -> FlowResult<StepResult> {
        if let StepKind::Click { x, y } = kind {
            let runner = ctx
                .input_runner
                .ok_or_else(|| FlowError::UnsupportedStep("InputRunner not configured".into()))?;
            runner.click(*x, *y).await?;
            Ok(StepResult {
                success: true,
                output: None,
                error: None,
            })
        } else {
            Err(FlowError::UnsupportedStep(
                "ClickHandler: wrong kind".into(),
            ))
        }
    }
}

pub struct SwipeHandler;

#[async_trait]
impl StepHandler for SwipeHandler {
    fn step_type(&self) -> &str {
        "swipe"
    }

    async fn execute(
        &self,
        kind: &StepKind,
        _input: &HashMap<String, Value>,
        ctx: &StepContext<'_>,
    ) -> FlowResult<StepResult> {
        if let StepKind::Swipe {
            x1,
            y1,
            x2,
            y2,
            duration_ms,
        } = kind
        {
            let runner = ctx
                .input_runner
                .ok_or_else(|| FlowError::UnsupportedStep("InputRunner not configured".into()))?;
            runner.swipe(*x1, *y1, *x2, *y2, *duration_ms).await?;
            Ok(StepResult {
                success: true,
                output: None,
                error: None,
            })
        } else {
            Err(FlowError::UnsupportedStep(
                "SwipeHandler: wrong kind".into(),
            ))
        }
    }
}

pub struct KeyPressHandler;

#[async_trait]
impl StepHandler for KeyPressHandler {
    fn step_type(&self) -> &str {
        "key_press"
    }

    async fn execute(
        &self,
        kind: &StepKind,
        _input: &HashMap<String, Value>,
        ctx: &StepContext<'_>,
    ) -> FlowResult<StepResult> {
        if let StepKind::KeyPress { key } = kind {
            let runner = ctx
                .input_runner
                .ok_or_else(|| FlowError::UnsupportedStep("InputRunner not configured".into()))?;
            runner.key_press(key).await?;
            Ok(StepResult {
                success: true,
                output: None,
                error: None,
            })
        } else {
            Err(FlowError::UnsupportedStep(
                "KeyPressHandler: wrong kind".into(),
            ))
        }
    }
}

pub struct ScriptHandler;

#[async_trait]
impl StepHandler for ScriptHandler {
    fn step_type(&self) -> &str {
        "script"
    }

    async fn execute(
        &self,
        kind: &StepKind,
        input: &HashMap<String, Value>,
        ctx: &StepContext<'_>,
    ) -> FlowResult<StepResult> {
        if let StepKind::Script { script } = kind {
            let runner = ctx
                .flow_runner
                .ok_or_else(|| FlowError::UnsupportedStep("FlowRunner not configured".into()))?;
            runner.run_script(script, input).await
        } else {
            Err(FlowError::UnsupportedStep(
                "ScriptHandler: wrong kind".into(),
            ))
        }
    }
}

pub struct FlowHandler;

#[async_trait]
impl StepHandler for FlowHandler {
    fn step_type(&self) -> &str {
        "flow"
    }

    async fn execute(
        &self,
        kind: &StepKind,
        input: &HashMap<String, Value>,
        ctx: &StepContext<'_>,
    ) -> FlowResult<StepResult> {
        if let StepKind::Flow { flow } = kind {
            let runner = ctx
                .flow_runner
                .ok_or_else(|| FlowError::UnsupportedStep("FlowRunner not configured".into()))?;
            runner.run_flow(flow, input.clone()).await
        } else {
            Err(FlowError::UnsupportedStep("FlowHandler: wrong kind".into()))
        }
    }
}

pub struct GroupHandler;

#[async_trait]
impl StepHandler for GroupHandler {
    fn step_type(&self) -> &str {
        "group"
    }

    async fn execute(
        &self,
        kind: &StepKind,
        input: &HashMap<String, Value>,
        ctx: &StepContext<'_>,
    ) -> FlowResult<StepResult> {
        if let StepKind::Group { group } = kind {
            let runner = ctx
                .flow_runner
                .ok_or_else(|| FlowError::UnsupportedStep("FlowRunner not configured".into()))?;
            runner.run_group(group, input.clone()).await
        } else {
            Err(FlowError::UnsupportedStep(
                "GroupHandler: wrong kind".into(),
            ))
        }
    }
}

// ============================================================================
// StepRegistry — maps step type strings to handlers
// ============================================================================

/// Registry of step handlers, keyed by step type string.
pub struct StepRegistry {
    handlers: HashMap<String, Arc<dyn StepHandler>>,
}

impl StepRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Create a registry with all built-in handlers.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(WaitHandler));
        registry.register(Arc::new(NoneHandler));
        registry.register(Arc::new(SetVariableHandler));
        registry.register(Arc::new(ClickHandler));
        registry.register(Arc::new(SwipeHandler));
        registry.register(Arc::new(KeyPressHandler));
        registry.register(Arc::new(ScriptHandler));
        registry.register(Arc::new(FlowHandler));
        registry.register(Arc::new(GroupHandler));
        registry
    }

    /// Register a step handler.
    pub fn register(&mut self, handler: Arc<dyn StepHandler>) {
        self.handlers
            .insert(handler.step_type().to_string(), handler);
    }

    /// Get the handler for the given step type.
    pub fn get(&self, step_type: &str) -> Option<&dyn StepHandler> {
        self.handlers.get(step_type).map(|h| h.as_ref())
    }

    /// Execute a step using the registered handler.
    pub async fn execute(
        &self,
        kind: &StepKind,
        input: &HashMap<String, Value>,
        ctx: &StepContext<'_>,
    ) -> FlowResult<StepResult> {
        let step_type = kind.type_name();
        match self.handlers.get(step_type) {
            Some(handler) => handler.execute(kind, input, ctx).await,
            None => Err(FlowError::UnsupportedStep(format!(
                "No handler registered for step type: {}",
                step_type
            ))),
        }
    }
}

impl Default for StepRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_variables() -> VariableStore {
        VariableStore::new("test".into(), HashMap::new(), None)
    }

    fn empty_ctx<'a>(vars: &'a VariableStore) -> StepContext<'a> {
        StepContext {
            variables: vars,
            flow_runner: None,
            input_runner: None,
        }
    }

    // ── StepRegistry ──

    #[test]
    fn test_registry_new_empty() {
        let registry = StepRegistry::new();
        assert!(registry.get("wait").is_none());
    }

    #[test]
    fn test_registry_with_defaults() {
        let registry = StepRegistry::with_defaults();
        assert!(registry.get("wait").is_some());
        assert!(registry.get("none").is_some());
        assert!(registry.get("set_variable").is_some());
        assert!(registry.get("click").is_some());
        assert!(registry.get("swipe").is_some());
        assert!(registry.get("key_press").is_some());
        assert!(registry.get("script").is_some());
        assert!(registry.get("flow").is_some());
        assert!(registry.get("group").is_some());
    }

    #[test]
    fn test_registry_default() {
        let registry = StepRegistry::default();
        assert!(registry.get("wait").is_some());
    }

    #[test]
    fn test_registry_get_unknown() {
        let registry = StepRegistry::new();
        assert!(registry.get("unknown").is_none());
    }

    // ── WaitHandler ──

    #[tokio::test]
    async fn test_wait_handler_matching_kind() {
        let handler = WaitHandler;
        let vars = empty_variables();
        let ctx = empty_ctx(&vars);
        let input = HashMap::new();
        let kind = StepKind::Wait { ms: 1 };
        let result = handler.execute(&kind, &input, &ctx).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_wait_handler_wrong_kind() {
        let handler = WaitHandler;
        let vars = empty_variables();
        let ctx = empty_ctx(&vars);
        let input = HashMap::new();
        let kind = StepKind::None;
        let result = handler.execute(&kind, &input, &ctx).await;
        assert!(result.is_err());
    }

    // ── NoneHandler ──

    #[tokio::test]
    async fn test_none_handler_always_succeeds() {
        let handler = NoneHandler;
        let vars = empty_variables();
        let ctx = empty_ctx(&vars);
        let input = HashMap::new();
        let kind = StepKind::None;
        let result = handler.execute(&kind, &input, &ctx).await.unwrap();
        assert!(result.success);
    }

    // ── SetVariableHandler ──

    #[tokio::test]
    async fn test_set_variable_handler() {
        let handler = SetVariableHandler;
        let vars = empty_variables();
        let ctx = empty_ctx(&vars);
        let input = HashMap::new();
        let kind = StepKind::SetVariable {
            key: "x".into(),
            value: serde_json::json!(42),
        };
        let result = handler.execute(&kind, &input, &ctx).await.unwrap();
        assert!(result.success);
        assert_eq!(vars.get("x").await, Some(serde_json::json!(42)));
    }

    #[tokio::test]
    async fn test_set_variable_handler_wrong_kind() {
        let handler = SetVariableHandler;
        let vars = empty_variables();
        let ctx = empty_ctx(&vars);
        let input = HashMap::new();
        let kind = StepKind::None;
        let result = handler.execute(&kind, &input, &ctx).await;
        assert!(result.is_err());
    }

    // ── ClickHandler ──

    #[tokio::test]
    async fn test_click_handler_no_runner() {
        let handler = ClickHandler;
        let vars = empty_variables();
        let ctx = empty_ctx(&vars);
        let input = HashMap::new();
        let kind = StepKind::Click { x: 10, y: 20 };
        let result = handler.execute(&kind, &input, &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_click_handler_wrong_kind() {
        let handler = ClickHandler;
        let vars = empty_variables();
        let ctx = empty_ctx(&vars);
        let input = HashMap::new();
        let kind = StepKind::None;
        let result = handler.execute(&kind, &input, &ctx).await;
        assert!(result.is_err());
    }

    // ── SwipeHandler ──

    #[tokio::test]
    async fn test_swipe_handler_no_runner() {
        let handler = SwipeHandler;
        let vars = empty_variables();
        let ctx = empty_ctx(&vars);
        let input = HashMap::new();
        let kind = StepKind::Swipe {
            x1: 0,
            y1: 0,
            x2: 100,
            y2: 100,
            duration_ms: 300,
        };
        let result = handler.execute(&kind, &input, &ctx).await;
        assert!(result.is_err());
    }

    // ── KeyPressHandler ──

    #[tokio::test]
    async fn test_key_press_handler_no_runner() {
        let handler = KeyPressHandler;
        let vars = empty_variables();
        let ctx = empty_ctx(&vars);
        let input = HashMap::new();
        let kind = StepKind::KeyPress { key: "A".into() };
        let result = handler.execute(&kind, &input, &ctx).await;
        assert!(result.is_err());
    }

    // ── ScriptHandler ──

    #[tokio::test]
    async fn test_script_handler_no_runner() {
        let handler = ScriptHandler;
        let vars = empty_variables();
        let ctx = empty_ctx(&vars);
        let input = HashMap::new();
        let kind = StepKind::Script {
            script: "test.js".into(),
        };
        let result = handler.execute(&kind, &input, &ctx).await;
        assert!(result.is_err());
    }

    // ── FlowHandler ──

    #[tokio::test]
    async fn test_flow_handler_no_runner() {
        let handler = FlowHandler;
        let vars = empty_variables();
        let ctx = empty_ctx(&vars);
        let input = HashMap::new();
        let kind = StepKind::Flow {
            flow: "flow1".into(),
        };
        let result = handler.execute(&kind, &input, &ctx).await;
        assert!(result.is_err());
    }

    // ── GroupHandler ──

    #[tokio::test]
    async fn test_group_handler_no_runner() {
        let handler = GroupHandler;
        let vars = empty_variables();
        let ctx = empty_ctx(&vars);
        let input = HashMap::new();
        let kind = StepKind::Group {
            group: "group1".into(),
        };
        let result = handler.execute(&kind, &input, &ctx).await;
        assert!(result.is_err());
    }

    // ── Registry execute ──

    #[tokio::test]
    async fn test_registry_execute_known_type() {
        let registry = StepRegistry::with_defaults();
        let vars = empty_variables();
        let ctx = empty_ctx(&vars);
        let input = HashMap::new();
        let kind = StepKind::None;
        let result = registry.execute(&kind, &input, &ctx).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_registry_execute_unknown_type() {
        let registry = StepRegistry::new();
        let vars = empty_variables();
        let ctx = empty_ctx(&vars);
        let input = HashMap::new();
        let kind = StepKind::None;
        let result = registry.execute(&kind, &input, &ctx).await;
        assert!(result.is_err());
    }

    // ── Handler step_type ──

    #[test]
    fn test_handler_step_types() {
        assert_eq!(WaitHandler.step_type(), "wait");
        assert_eq!(NoneHandler.step_type(), "none");
        assert_eq!(SetVariableHandler.step_type(), "set_variable");
        assert_eq!(ClickHandler.step_type(), "click");
        assert_eq!(SwipeHandler.step_type(), "swipe");
        assert_eq!(KeyPressHandler.step_type(), "key_press");
        assert_eq!(ScriptHandler.step_type(), "script");
        assert_eq!(FlowHandler.step_type(), "flow");
        assert_eq!(GroupHandler.step_type(), "group");
    }
}
