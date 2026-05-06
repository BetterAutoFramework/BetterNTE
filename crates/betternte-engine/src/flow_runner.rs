//! EngineFlowRunner + EngineInputRunner — implements FlowRunner + InputRunner for the engine layer.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tracing::info;

use betternte_runtime::{
    FlowError, FlowExecutor, FlowParser, FlowResult, FlowRunner, Group, InputRunner, StepResult,
    VariableStore,
};
use betternte_script::ScriptContext;

use crate::script_ctx;

/// Engine-level FlowRunner implementation.
///
/// Delegates script execution to ScriptRuntime and flow/group execution to nested executors.
pub struct EngineFlowRunner {
    runtime: Arc<betternte_script::ScriptRuntime>,
    ctx: Arc<script_ctx::EngineScriptContext>,
    flows: Vec<betternte_runtime::Flow>,
    task_groups: Vec<Group>,
    input_runner: Option<Arc<dyn InputRunner>>,
    condition_handlers: Arc<Vec<Arc<dyn betternte_runtime::ConditionHandler>>>,
    step_handlers: Arc<Vec<Arc<dyn betternte_runtime::StepHandler>>>,
}

impl EngineFlowRunner {
    pub fn new(
        runtime: Arc<betternte_script::ScriptRuntime>,
        ctx: Arc<script_ctx::EngineScriptContext>,
        flows: Vec<betternte_runtime::Flow>,
        task_groups: Vec<Group>,
        input_runner: Option<Arc<dyn InputRunner>>,
        condition_handlers: Arc<Vec<Arc<dyn betternte_runtime::ConditionHandler>>>,
        step_handlers: Arc<Vec<Arc<dyn betternte_runtime::StepHandler>>>,
    ) -> Self {
        Self {
            runtime,
            ctx,
            flows,
            task_groups,
            input_runner,
            condition_handlers,
            step_handlers,
        }
    }

    /// Create a nested FlowExecutor and run it to completion.
    async fn execute_flow(
        &self,
        flow: betternte_runtime::Flow,
        _input: HashMap<String, Value>,
    ) -> FlowResult<StepResult> {
        let variables = Arc::new(VariableStore::new(
            flow.id.clone(),
            flow.variables.clone(),
            None,
        ));

        // Create nested runners for deeper nesting
        let nested_flow_runner: Arc<dyn FlowRunner> = Arc::new(EngineFlowRunner::new(
            self.runtime.clone(),
            self.ctx.clone(),
            self.flows.clone(),
            self.task_groups.clone(),
            self.input_runner.clone(),
            self.condition_handlers.clone(),
            self.step_handlers.clone(),
        ));

        let step_executor = Arc::new(
            betternte_runtime::DefaultStepExecutor::with_custom_handlers(
                variables.clone(),
                Some(nested_flow_runner),
                self.input_runner.clone(),
                self.step_handlers.as_ref().clone(),
            ),
        );
        let condition_handlers = if self.condition_handlers.is_empty() {
            betternte_runtime::condition_handlers::default_condition_handler_arcs()
        } else {
            self.condition_handlers.as_ref().clone()
        };

        let executor =
            FlowExecutor::new_with_custom(flow, variables, step_executor, condition_handlers);
        executor.run().await?;

        Ok(StepResult {
            success: true,
            output: None,
            error: None,
        })
    }
}

#[async_trait]
impl FlowRunner for EngineFlowRunner {
    async fn run_script(
        &self,
        script: &str,
        input: &HashMap<String, Value>,
    ) -> FlowResult<StepResult> {
        let mut params_map = serde_json::Map::new();
        for (k, v) in input {
            let parsed = match v {
                Value::String(s) => {
                    serde_json::from_str::<Value>(s).unwrap_or_else(|_| Value::String(s.clone()))
                }
                _ => v.clone(),
            };
            params_map.insert(k.clone(), parsed);
        }
        let params = Value::Object(params_map);
        let ctx: Arc<dyn betternte_script::ScriptContext> = self.ctx.clone();

        info!(script, "FlowRunner: executing script");

        // Enable the script first, then start it
        self.runtime
            .enable_script(script, &ctx, &serde_json::json!({}))
            .await
            .map_err(|e| FlowError::ScriptError(format!("enable_script failed: {}", e)))?;

        self.runtime
            .start_task(script, params, &ctx)
            .await
            .map_err(|e| FlowError::ScriptError(format!("start_task failed: {}", e)))?;

        Ok(StepResult {
            success: true,
            output: None,
            error: None,
        })
    }

    async fn run_flow(
        &self,
        flow_id: &str,
        input: HashMap<String, Value>,
    ) -> FlowResult<StepResult> {
        let flow = self
            .flows
            .iter()
            .find(|f| f.id == flow_id)
            .cloned()
            .ok_or_else(|| FlowError::Other(anyhow::anyhow!("Flow '{}' not found", flow_id)))?;

        info!(flow_id, "FlowRunner: executing nested flow");
        self.execute_flow(flow, input).await
    }

    async fn run_group(
        &self,
        group_id: &str,
        input: HashMap<String, Value>,
    ) -> FlowResult<StepResult> {
        if let Some(flow) = self
            .flows
            .iter()
            .find(|f| f.id == group_id || f.name == group_id)
            .cloned()
        {
            info!(group_id, "FlowRunner: group alias resolved to flow");
            return self.execute_flow(flow, input).await;
        }

        let group = self
            .task_groups
            .iter()
            .find(|g| g.name == group_id || g.uuid == group_id)
            .cloned()
            .ok_or_else(|| {
                FlowError::Other(anyhow::anyhow!("Task group '{}' not found", group_id))
            })?;

        info!(group_id, "FlowRunner: executing nested task group");

        let parser = FlowParser::new();
        let flow = parser
            .parse_group(&group)
            .map_err(|e| FlowError::Other(anyhow::anyhow!("Failed to parse task group: {}", e)))?;

        self.execute_flow(flow, input).await
    }
}

// ============================================================================
// EngineInputRunner — implements InputRunner
// ============================================================================

/// Engine-level InputRunner implementation.
///
/// Delegates input operations to ScriptContext (which wraps InputController).
pub struct EngineInputRunner {
    ctx: Arc<script_ctx::EngineScriptContext>,
}

impl EngineInputRunner {
    pub fn new(ctx: Arc<script_ctx::EngineScriptContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl InputRunner for EngineInputRunner {
    async fn click(&self, x: i32, y: i32) -> FlowResult<()> {
        self.ctx.click(x, y).await.map_err(flow_err)
    }

    async fn swipe(&self, x1: i32, y1: i32, x2: i32, y2: i32, duration_ms: u32) -> FlowResult<()> {
        self.ctx
            .swipe(x1, y1, x2, y2, duration_ms)
            .await
            .map_err(flow_err)
    }

    async fn key_press(&self, key: &str) -> FlowResult<()> {
        self.ctx.key_press(key, None).await.map_err(flow_err)
    }
}

fn flow_err(e: anyhow::Error) -> FlowError {
    FlowError::ScriptError(e.to_string())
}
