//! Flow 执行器 — Step 执行 + Transition 跳转

use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use crate::condition::ConditionEvaluator;
use crate::condition_handlers::{default_condition_handler_arcs, ConditionHandler};
use crate::error::{FlowError, FlowResult};
use crate::perf_log;
use crate::step_handlers::{StepContext, StepRegistry};
use crate::types::{Flow, Step, StepKind, Transition, Trigger, TriggerAction};
use crate::variables::VariableStore;

/// Flow 执行状态
#[derive(Debug, Clone, PartialEq)]
pub enum FlowStatus {
    /// 未开始
    Idle,
    /// 运行中
    Running,
    /// 已暂停
    Paused,
    /// 已完成
    Completed,
    /// 出错
    Error(String),
}

/// Step 执行结果
#[derive(Debug, Clone)]
pub struct StepResult {
    /// 是否成功
    pub success: bool,
    /// 输出数据
    pub output: Option<Value>,
    /// 错误信息
    pub error: Option<String>,
}

/// Flow 执行器
pub struct FlowExecutor {
    /// 当前 Flow
    flow: Arc<Flow>,
    /// 变量存储
    variables: Arc<VariableStore>,
    /// 当前 Step ID
    current_step: RwLock<Option<String>>,
    /// 执行状态
    status: RwLock<FlowStatus>,
    /// 取消标志
    cancelled: Arc<AtomicBool>,
    /// 执行历史
    history: RwLock<Vec<ExecutionEvent>>,
    /// 每个 Step 的输出缓存（用于 $steps.xxx.result.yyy 引用）
    step_outputs: RwLock<HashMap<String, Value>>,
    /// Step 执行器
    step_executor: Arc<dyn StepExecutor>,
    /// 条件处理器（策略模式）
    condition_handlers: Arc<Vec<Arc<dyn ConditionHandler>>>,
}

/// 执行事件
#[derive(Debug, Clone)]
pub struct ExecutionEvent {
    pub timestamp: Instant,
    pub event_type: EventType,
    pub step_id: Option<String>,
    pub details: String,
}

/// 事件类型
#[derive(Debug, Clone)]
pub enum EventType {
    FlowStart,
    FlowEnd,
    StepStart,
    StepEnd,
    Transition,
    Trigger,
    Error,
}

#[derive(Default)]
struct StepExecTimings {
    resolve_input_ms: f64,
    step_executor_ms: f64,
    attempts: u32,
}

/// Step 执行器 trait
#[async_trait::async_trait]
pub trait StepExecutor: Send + Sync {
    /// 执行 Step
    async fn execute(
        &self,
        kind: &StepKind,
        input: &HashMap<String, Value>,
    ) -> FlowResult<StepResult>;
}

impl FlowExecutor {
    /// 创建新的执行器
    pub fn new(
        flow: Flow,
        variables: Arc<VariableStore>,
        step_executor: Arc<dyn StepExecutor>,
    ) -> Self {
        Self {
            flow: Arc::new(flow),
            variables,
            current_step: RwLock::new(None),
            status: RwLock::new(FlowStatus::Idle),
            cancelled: Arc::new(AtomicBool::new(false)),
            history: RwLock::new(Vec::new()),
            step_outputs: RwLock::new(HashMap::new()),
            step_executor,
            condition_handlers: Arc::new(default_condition_handler_arcs()),
        }
    }

    /// 使用 DefaultStepExecutor 创建执行器（便捷构造器）
    pub fn new_with_defaults(
        flow: Flow,
        variables: Arc<VariableStore>,
        flow_runner: Option<Arc<dyn FlowRunner>>,
    ) -> Self {
        let executor = Arc::new(DefaultStepExecutor::new(variables.clone(), flow_runner));
        Self::new_with_custom(flow, variables, executor, default_condition_handler_arcs())
    }

    /// 使用自定义执行器与条件处理器创建执行器。
    pub fn new_with_custom(
        flow: Flow,
        variables: Arc<VariableStore>,
        step_executor: Arc<dyn StepExecutor>,
        condition_handlers: Vec<Arc<dyn ConditionHandler>>,
    ) -> Self {
        Self {
            flow: Arc::new(flow),
            variables,
            current_step: RwLock::new(None),
            status: RwLock::new(FlowStatus::Idle),
            cancelled: Arc::new(AtomicBool::new(false)),
            history: RwLock::new(Vec::new()),
            step_outputs: RwLock::new(HashMap::new()),
            step_executor,
            condition_handlers: Arc::new(condition_handlers),
        }
    }

    /// 执行 Flow
    pub async fn run(&self) -> FlowResult<()> {
        // 初始化变量
        self.variables.initialize().await?;

        // 设置入口
        *self.current_step.write().await = Some(self.flow.entry.clone());
        *self.status.write().await = FlowStatus::Running;

        self.log_event(EventType::FlowStart, None, "Flow started".to_string())
            .await;

        let flow_id = self.flow.id.clone();
        // 主循环
        loop {
            // 检查取消
            if self.cancelled.load(Ordering::SeqCst) {
                *self.status.write().await = FlowStatus::Error("Cancelled".to_string());
                self.log_event(EventType::Error, None, "Flow cancelled".to_string())
                    .await;
                return Err(FlowError::Cancelled);
            }

            // 获取当前 Step
            let current = {
                let guard = self.current_step.read().await;
                guard.clone()
            };

            let step_id = match current {
                Some(id) => id,
                None => {
                    // 没有更多 Step，Flow 完成
                    *self.status.write().await = FlowStatus::Completed;
                    self.log_event(EventType::FlowEnd, None, "Flow completed".to_string())
                        .await;

                    // 持久化变量
                    self.variables.persist().await?;
                    return Ok(());
                }
            };

            // 获取 Step 定义
            let step = self
                .flow
                .steps
                .get(&step_id)
                .ok_or_else(|| FlowError::StepNotFound(step_id.clone()))?;

            // 检查 interrupt Transitions（每帧检查，优先于正常执行）
            let interrupt_check_start = Instant::now();
            let interrupt_target = check_interrupt_transitions(
                &self.flow,
                &step_id,
                &self.variables,
                &self.condition_handlers,
            )
            .await?;
            let interrupt_check_ms = interrupt_check_start.elapsed().as_secs_f64() * 1000.0;
            if let Some(interrupt_target) = interrupt_target {
                perf_log::log_flow_interrupt(
                    &flow_id,
                    &step_id,
                    interrupt_check_ms,
                    &interrupt_target,
                );
                self.log_event(
                    EventType::Transition,
                    Some(step_id.clone()),
                    format!("Interrupt transition to: {}", interrupt_target),
                )
                .await;
                *self.current_step.write().await = Some(interrupt_target);
                continue;
            }

            // 执行 Step
            self.log_event(
                EventType::StepStart,
                Some(step_id.clone()),
                "Step started".to_string(),
            )
            .await;

            let mut step_timings = StepExecTimings::default();
            let result = self
                .execute_step_with_retry(&step_id, step, &mut step_timings)
                .await;

            match result {
                Ok(step_result) => {
                    // 缓存 Step 输出（用于 $steps.xxx.result.yyy 引用）
                    if let Some(ref output) = step_result.output {
                        self.step_outputs
                            .write()
                            .await
                            .insert(step_id.clone(), output.clone());
                    }

                    // 写入输出变量
                    let apply_start = Instant::now();
                    self.apply_output(&step.output, &step_result).await?;
                    let apply_output_ms = apply_start.elapsed().as_secs_f64() * 1000.0;

                    self.log_event(
                        EventType::StepEnd,
                        Some(step_id.clone()),
                        format!("Step completed: success={}", step_result.success),
                    )
                    .await;

                    // 查找下一个 Step（含 interrupt 检查）
                    let find_start = Instant::now();
                    let next = self.find_next_step(&step, &step_id).await?;
                    let find_next_step_ms = find_start.elapsed().as_secs_f64() * 1000.0;

                    perf_log::log_flow_step_success(
                        &flow_id,
                        &step_id,
                        interrupt_check_ms,
                        step_timings.attempts,
                        step_timings.resolve_input_ms,
                        step_timings.step_executor_ms,
                        apply_output_ms,
                        find_next_step_ms,
                    );

                    if let Some(next_id) = next {
                        self.log_event(
                            EventType::Transition,
                            Some(step_id.clone()),
                            format!("Transition to: {}", next_id),
                        )
                        .await;

                        *self.current_step.write().await = Some(next_id);
                    } else {
                        // 没有更多转换，Flow 完成
                        *self.current_step.write().await = None;
                    }
                }
                Err(e) => {
                    perf_log::log_flow_step_error(
                        &flow_id,
                        &step_id,
                        interrupt_check_ms,
                        step_timings.attempts,
                        step_timings.resolve_input_ms,
                        step_timings.step_executor_ms,
                    );
                    // 错误处理
                    self.log_event(
                        EventType::Error,
                        Some(step_id.clone()),
                        format!("Step failed: {}", e),
                    )
                    .await;

                    // 检查 on_error
                    if let Some(on_error) = &step.on_error {
                        self.log_event(
                            EventType::Transition,
                            Some(step_id.clone()),
                            format!("Error transition to: {}", on_error),
                        )
                        .await;

                        *self.current_step.write().await = Some(on_error.clone());
                    } else {
                        // 没有错误处理，Flow 失败
                        *self.status.write().await = FlowStatus::Error(e.to_string());
                        return Err(e);
                    }
                }
            }
        }
    }

    /// 执行 Step（带重试）
    async fn execute_step_with_retry(
        &self,
        step_id: &str,
        step: &Step,
        timings: &mut StepExecTimings,
    ) -> FlowResult<StepResult> {
        let max_retries = step.max_retries;
        let mut last_error = None;

        for attempt in 0..=max_retries {
            timings.attempts = attempt + 1;
            if attempt > 0 {
                tracing::info!("Retrying step '{}' (attempt {})", step_id, attempt);
            }

            // 解析输入
            let t_resolve = Instant::now();
            let input = self.resolve_input(&step.input).await;
            timings.resolve_input_ms = t_resolve.elapsed().as_secs_f64() * 1000.0;

            // 执行
            let timeout_ms = step.timeout_ms.unwrap_or(30000); // 默认 30 秒超时
            let t_exec = Instant::now();
            let result = tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms),
                self.step_executor.execute(&step.kind, &input),
            )
            .await;
            timings.step_executor_ms = t_exec.elapsed().as_secs_f64() * 1000.0;

            match result {
                Ok(Ok(step_result)) => {
                    if step_result.success {
                        return Ok(step_result);
                    } else {
                        last_error = step_result.error.clone();
                        tracing::warn!(
                            "Step '{}' failed (attempt {}): {:?}",
                            step_id,
                            attempt,
                            step_result.error
                        );
                    }
                }
                Ok(Err(e)) => {
                    last_error = Some(e.to_string());
                    tracing::warn!("Step '{}' error (attempt {}): {}", step_id, attempt, e);
                }
                Err(_) => {
                    last_error = Some(format!("Timeout after {}ms", timeout_ms));
                    tracing::warn!("Step '{}' timeout (attempt {})", step_id, attempt);
                }
            }
        }

        Err(FlowError::Other(anyhow::anyhow!(
            "Step '{}' failed after {} retries: {:?}",
            step_id,
            max_retries,
            last_error
        )))
    }

    /// 查找下一个 Step
    async fn find_next_step(&self, step: &Step, _current_id: &str) -> FlowResult<Option<String>> {
        let handlers = &*self.condition_handlers;
        let evaluator = ConditionEvaluator::with_handlers(&self.variables, handlers);

        // 按优先级排序
        let mut transitions = step.transitions.clone();
        transitions.sort_by(|a, b| b.priority.cmp(&a.priority));

        for trans in &transitions {
            if evaluator.evaluate(&trans.condition).await? {
                return Ok(Some(trans.target.clone()));
            }
        }

        Ok(None)
    }

    /// 应用输出映射
    async fn apply_output(
        &self,
        output_mapping: &HashMap<String, String>,
        result: &StepResult,
    ) -> FlowResult<()> {
        for (var_ref, value_ref) in output_mapping {
            // 解析变量引用 ($variables.xxx)
            if let Some(var_name) = crate::variables::resolve_variable_ref(var_ref) {
                // 解析值引用
                let value = self.resolve_value_ref(value_ref, result).await;
                self.variables.set(var_name.to_string(), value).await?;
            }
            // 解析子 Flow 输出引用 ($flow_output.xxx)
            else if let Some(flow_output_key) = crate::variables::resolve_flow_output_ref(var_ref)
            {
                let value = self.resolve_value_ref(value_ref, result).await;
                self.variables
                    .set(flow_output_key.to_string(), value)
                    .await?;
            }
        }

        Ok(())
    }

    /// 解析值引用
    async fn resolve_value_ref(&self, value_ref: &str, result: &StepResult) -> Value {
        // $result.xxx → 从 Step 输出中读取
        if let Some(result_field) = crate::variables::resolve_output_ref(value_ref) {
            let val = result
                .output
                .as_ref()
                .and_then(|o| o.get(result_field))
                .cloned()
                .unwrap_or(Value::Null);
            return val;
        }

        // $flow_output.xxx → 从子 Flow 输出中读取（同 $result）
        if let Some(flow_output_field) = crate::variables::resolve_flow_output_ref(value_ref) {
            return result
                .output
                .as_ref()
                .and_then(|o| o.get(flow_output_field))
                .cloned()
                .unwrap_or(Value::Null);
        }

        // $steps.xxx.result.yyy → 从 Step 输出缓存中读取
        if let Some((step_id, field)) = crate::variables::resolve_step_output_ref(value_ref) {
            let outputs = self.step_outputs.read().await;
            if let Some(step_output) = outputs.get(step_id) {
                return step_output.get(field).cloned().unwrap_or(Value::Null);
            }
            tracing::warn!("Step output not found for step '{}'", step_id);
            return Value::Null;
        }

        // $variables.xxx → 从变量存储中读取
        if let Some(var_name) = crate::variables::resolve_variable_ref(value_ref) {
            return self.variables.get(var_name).await.unwrap_or(Value::Null);
        }

        // 尝试解析为 JSON 字面量
        serde_json::from_str(value_ref).unwrap_or(Value::String(value_ref.to_string()))
    }

    /// 解析输入映射
    async fn resolve_input(
        &self,
        input_mapping: &HashMap<String, String>,
    ) -> HashMap<String, Value> {
        let mut result = HashMap::new();

        for (key, value_ref) in input_mapping {
            let value = if let Some(var_name) = crate::variables::resolve_variable_ref(value_ref) {
                self.variables.get(var_name).await.unwrap_or(Value::Null)
            } else {
                Value::String(value_ref.clone())
            };

            result.insert(key.clone(), value);
        }

        result
    }

    /// 取消 Flow
    pub async fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        *self.status.write().await = FlowStatus::Error("Cancelled".to_string());
    }

    /// 获取当前状态
    pub async fn status(&self) -> FlowStatus {
        self.status.read().await.clone()
    }

    /// 获取当前 Step
    pub async fn current_step(&self) -> Option<String> {
        self.current_step.read().await.clone()
    }

    /// 获取执行历史
    pub async fn history(&self) -> Vec<ExecutionEvent> {
        self.history.read().await.clone()
    }

    /// 记录事件
    async fn log_event(&self, event_type: EventType, step_id: Option<String>, details: String) {
        let event = ExecutionEvent {
            timestamp: Instant::now(),
            event_type,
            step_id,
            details,
        };

        tracing::debug!("{:?}", event);
        self.history.write().await.push(event);
    }

    /// 处理触发器
    pub async fn handle_trigger(&self, trigger: &Trigger) -> FlowResult<()> {
        match &trigger.action {
            TriggerAction::JumpTo { step } => {
                if self.flow.steps.contains_key(step) {
                    *self.current_step.write().await = Some(step.clone());
                    self.log_event(
                        EventType::Trigger,
                        Some(step.clone()),
                        format!("Trigger '{}' jumped to step '{}'", trigger.name, step),
                    )
                    .await;
                } else {
                    return Err(FlowError::StepNotFound(step.clone()));
                }
            }
            TriggerAction::Interrupt => {
                self.cancel().await;
            }
            TriggerAction::Stop => {
                self.cancel().await;
            }
            TriggerAction::StartFlow { flow: _ } => {
                // 由上层处理
            }
        }

        Ok(())
    }
}

// ============================================================================
// FlowRunner — 嵌套 Flow/Group/Script 执行委托
// ============================================================================

/// Flow 运行器 — 用于嵌套 Flow/Group/Script 执行
///
/// 引擎层实现此 trait，将 Script/Flow/Group 委托给具体的执行引擎。
#[async_trait::async_trait]
pub trait FlowRunner: Send + Sync {
    /// 执行子 Flow，返回结果
    async fn run_flow(
        &self,
        flow_id: &str,
        input: HashMap<String, Value>,
    ) -> FlowResult<StepResult>;
    /// 执行 Group（线性 Flow），返回结果
    async fn run_group(
        &self,
        group_id: &str,
        input: HashMap<String, Value>,
    ) -> FlowResult<StepResult>;
    /// 执行脚本
    async fn run_script(
        &self,
        script: &str,
        input: &HashMap<String, Value>,
    ) -> FlowResult<StepResult>;
}

// ============================================================================
// InputRunner — 输入操作委托（ISP 拆分）
// ============================================================================

/// 输入运行器 — 用于 Click/Swipe/KeyPress 操作
///
/// 从 FlowRunner 中拆分出来，遵循接口隔离原则。
/// 引擎层实现此 trait，将输入操作委托给 InputController。
#[async_trait::async_trait]
pub trait InputRunner: Send + Sync {
    /// 执行点击
    async fn click(&self, x: i32, y: i32) -> FlowResult<()>;
    /// 执行滑动
    async fn swipe(&self, x1: i32, y1: i32, x2: i32, y2: i32, duration_ms: u32) -> FlowResult<()>;
    /// 执行按键
    async fn key_press(&self, key: &str) -> FlowResult<()>;
}

// ============================================================================
// DefaultStepExecutor — 默认 Step 执行器
// ============================================================================

/// 默认 Step 执行器
///
/// Uses StepRegistry (Command + Registry pattern) for step dispatch.
pub struct DefaultStepExecutor {
    variables: Arc<VariableStore>,
    flow_runner: Option<Arc<dyn FlowRunner>>,
    input_runner: Option<Arc<dyn InputRunner>>,
    registry: StepRegistry,
}

impl DefaultStepExecutor {
    /// 创建新的默认执行器
    pub fn new(variables: Arc<VariableStore>, flow_runner: Option<Arc<dyn FlowRunner>>) -> Self {
        Self {
            variables,
            flow_runner,
            input_runner: None,
            registry: StepRegistry::with_defaults(),
        }
    }

    /// 创建带输入运行器的默认执行器
    pub fn with_input_runner(
        variables: Arc<VariableStore>,
        flow_runner: Option<Arc<dyn FlowRunner>>,
        input_runner: Option<Arc<dyn InputRunner>>,
    ) -> Self {
        Self::with_custom_handlers(variables, flow_runner, input_runner, Vec::new())
    }

    /// 创建带输入运行器和额外 step handlers 的默认执行器
    pub fn with_custom_handlers(
        variables: Arc<VariableStore>,
        flow_runner: Option<Arc<dyn FlowRunner>>,
        input_runner: Option<Arc<dyn InputRunner>>,
        extra_step_handlers: Vec<Arc<dyn crate::step_handlers::StepHandler>>,
    ) -> Self {
        let mut registry = StepRegistry::with_defaults();
        for handler in extra_step_handlers {
            registry.register(handler);
        }
        Self {
            variables,
            flow_runner,
            input_runner,
            registry,
        }
    }
}

#[async_trait::async_trait]
impl StepExecutor for DefaultStepExecutor {
    async fn execute(
        &self,
        kind: &StepKind,
        input: &HashMap<String, Value>,
    ) -> FlowResult<StepResult> {
        let ctx = StepContext {
            variables: &self.variables,
            flow_runner: self.flow_runner.as_ref(),
            input_runner: self.input_runner.as_ref(),
        };
        self.registry.execute(kind, input, &ctx).await
    }
}

/// 检查 interrupt Transitions
pub async fn check_interrupt_transitions(
    flow: &Flow,
    current_step_id: &str,
    variables: &VariableStore,
    condition_handlers: &[Arc<dyn ConditionHandler>],
) -> FlowResult<Option<String>> {
    let step = match flow.steps.get(current_step_id) {
        Some(s) => s,
        None => return Ok(None),
    };

    let evaluator = ConditionEvaluator::with_handlers(variables, condition_handlers);

    // 收集所有 interrupt=true 的 Transition
    let mut interrupt_transitions: Vec<&Transition> =
        step.transitions.iter().filter(|t| t.interrupt).collect();

    // 按优先级排序
    interrupt_transitions.sort_by(|a, b| b.priority.cmp(&a.priority));

    // 检查条件
    for trans in &interrupt_transitions {
        if evaluator.evaluate(&trans.condition).await? {
            return Ok(Some(trans.target.clone()));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::condition_handlers::ConditionHandler;
    use crate::step_handlers::StepHandler;
    use crate::types::StepKind;

    struct MockStepExecutor;

    #[async_trait::async_trait]
    impl StepExecutor for MockStepExecutor {
        async fn execute(
            &self,
            kind: &StepKind,
            _input: &HashMap<String, Value>,
        ) -> FlowResult<StepResult> {
            match kind {
                StepKind::Wait { ms } => {
                    tokio::time::sleep(std::time::Duration::from_millis(*ms)).await;
                    Ok(StepResult {
                        success: true,
                        output: None,
                        error: None,
                    })
                }
                StepKind::None => Ok(StepResult {
                    success: true,
                    output: None,
                    error: None,
                }),
                StepKind::SetVariable { value, .. } => Ok(StepResult {
                    success: true,
                    output: Some(serde_json::json!({ "value": value })),
                    error: None,
                }),
                _ => Ok(StepResult {
                    success: true,
                    output: None,
                    error: None,
                }),
            }
        }
    }

    #[tokio::test]
    async fn test_simple_flow() {
        let json = r#"{
            "id": "test",
            "name": "Test",
            "entry": "start",
            "steps": {
                "start": {
                    "kind": { "type": "wait", "ms": 10 },
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

        let flow = crate::parser::FlowParser::new().parse_str(json).unwrap();
        let variables = Arc::new(VariableStore::new("test".to_string(), HashMap::new(), None));
        let executor = FlowExecutor::new(flow, variables, Arc::new(MockStepExecutor));

        executor.run().await.unwrap();
        assert_eq!(executor.status().await, FlowStatus::Completed);
    }

    #[tokio::test]
    async fn test_flow_with_variable() {
        let json = r#"{
            "id": "test",
            "name": "Test",
            "entry": "set_var",
            "steps": {
                "set_var": {
                    "kind": { "type": "set_variable", "key": "hp", "value": 100 },
                    "output": { "$variables.hp": "$result.value" },
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

        let flow = crate::parser::FlowParser::new().parse_str(json).unwrap();
        let variables = Arc::new(VariableStore::new("test".to_string(), HashMap::new(), None));
        let executor = FlowExecutor::new(flow, variables.clone(), Arc::new(MockStepExecutor));

        executor.run().await.unwrap();
        assert_eq!(executor.status().await, FlowStatus::Completed);
        assert_eq!(variables.get("hp").await, Some(serde_json::json!(100)));
    }

    // ========================================================================
    // DefaultStepExecutor 测试
    // ========================================================================

    #[tokio::test]
    async fn test_default_executor_wait() {
        let vars = Arc::new(VariableStore::new("test".to_string(), HashMap::new(), None));
        vars.initialize().await.unwrap();
        let executor = DefaultStepExecutor::new(vars, None);

        let result = executor
            .execute(&StepKind::Wait { ms: 10 }, &HashMap::new())
            .await
            .unwrap();

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_default_executor_none() {
        let vars = Arc::new(VariableStore::new("test".to_string(), HashMap::new(), None));
        vars.initialize().await.unwrap();
        let executor = DefaultStepExecutor::new(vars, None);

        let result = executor
            .execute(&StepKind::None, &HashMap::new())
            .await
            .unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_default_executor_set_variable() {
        let vars = Arc::new(VariableStore::new("test".to_string(), HashMap::new(), None));
        vars.initialize().await.unwrap();
        let executor = DefaultStepExecutor::new(vars.clone(), None);

        let result = executor
            .execute(
                &StepKind::SetVariable {
                    key: "hp".to_string(),
                    value: serde_json::json!(100),
                },
                &HashMap::new(),
            )
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(vars.get("hp").await, Some(serde_json::json!(100)));
    }

    #[tokio::test]
    async fn test_default_executor_unsupported_click() {
        let vars = Arc::new(VariableStore::new("test".to_string(), HashMap::new(), None));
        vars.initialize().await.unwrap();
        let executor = DefaultStepExecutor::new(vars, None);

        let result = executor
            .execute(&StepKind::Click { x: 100, y: 200 }, &HashMap::new())
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FlowError::UnsupportedStep(_)));
    }

    #[tokio::test]
    async fn test_default_executor_unsupported_script() {
        let vars = Arc::new(VariableStore::new("test".to_string(), HashMap::new(), None));
        vars.initialize().await.unwrap();
        let executor = DefaultStepExecutor::new(vars, None);

        let result = executor
            .execute(
                &StepKind::Script {
                    script: "test.js".to_string(),
                },
                &HashMap::new(),
            )
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FlowError::UnsupportedStep(_)));
    }

    // ========================================================================
    // FlowRunner Mock 测试
    // ========================================================================

    struct MockFlowRunner;

    #[async_trait::async_trait]
    impl FlowRunner for MockFlowRunner {
        async fn run_flow(
            &self,
            _flow_id: &str,
            _input: HashMap<String, Value>,
        ) -> FlowResult<StepResult> {
            Ok(StepResult {
                success: true,
                output: Some(serde_json::json!({"done": true})),
                error: None,
            })
        }
        async fn run_group(
            &self,
            _group_id: &str,
            _input: HashMap<String, Value>,
        ) -> FlowResult<StepResult> {
            Ok(StepResult {
                success: true,
                output: None,
                error: None,
            })
        }
        async fn run_script(
            &self,
            _script: &str,
            _input: &HashMap<String, Value>,
        ) -> FlowResult<StepResult> {
            Ok(StepResult {
                success: true,
                output: Some(serde_json::json!({"result": "ok"})),
                error: None,
            })
        }
    }

    struct MockInputRunner;

    #[async_trait::async_trait]
    impl InputRunner for MockInputRunner {
        async fn click(&self, _x: i32, _y: i32) -> FlowResult<()> {
            Ok(())
        }
        async fn swipe(
            &self,
            _x1: i32,
            _y1: i32,
            _x2: i32,
            _y2: i32,
            _duration_ms: u32,
        ) -> FlowResult<()> {
            Ok(())
        }
        async fn key_press(&self, _key: &str) -> FlowResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_default_executor_with_runner_click() {
        let vars = Arc::new(VariableStore::new("test".to_string(), HashMap::new(), None));
        vars.initialize().await.unwrap();
        let executor = DefaultStepExecutor::with_input_runner(
            vars,
            Some(Arc::new(MockFlowRunner)),
            Some(Arc::new(MockInputRunner)),
        );

        let result = executor
            .execute(&StepKind::Click { x: 100, y: 200 }, &HashMap::new())
            .await
            .unwrap();

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_default_executor_with_runner_script() {
        let vars = Arc::new(VariableStore::new("test".to_string(), HashMap::new(), None));
        vars.initialize().await.unwrap();
        let executor = DefaultStepExecutor::new(vars, Some(Arc::new(MockFlowRunner)));

        let result = executor
            .execute(
                &StepKind::Script {
                    script: "test.js".to_string(),
                },
                &HashMap::new(),
            )
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.output, Some(serde_json::json!({"result": "ok"})));
    }

    #[tokio::test]
    async fn test_default_executor_with_runner_flow() {
        let vars = Arc::new(VariableStore::new("test".to_string(), HashMap::new(), None));
        vars.initialize().await.unwrap();
        let executor = DefaultStepExecutor::new(vars, Some(Arc::new(MockFlowRunner)));

        let result = executor
            .execute(
                &StepKind::Flow {
                    flow: "sub_flow".to_string(),
                },
                &HashMap::new(),
            )
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.output, Some(serde_json::json!({"done": true})));
    }

    // ========================================================================
    // new_with_defaults 测试
    // ========================================================================

    #[tokio::test]
    async fn test_flow_executor_new_with_defaults() {
        let json = r#"{
            "id": "test",
            "name": "Test",
            "entry": "start",
            "steps": {
                "start": {
                    "kind": { "type": "wait", "ms": 10 },
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

        let flow = crate::parser::FlowParser::new().parse_str(json).unwrap();
        let variables = Arc::new(VariableStore::new("test".to_string(), HashMap::new(), None));
        let executor = FlowExecutor::new_with_defaults(flow, variables, None);

        executor.run().await.unwrap();
        assert_eq!(executor.status().await, FlowStatus::Completed);
    }

    // ========================================================================
    // $steps.xxx.result.yyy 变量解析测试
    // ========================================================================

    #[test]
    fn test_resolve_step_output_ref() {
        let result = crate::variables::resolve_step_output_ref("$steps.detect.result.hp");
        assert_eq!(result, Some(("detect", "hp")));

        let result = crate::variables::resolve_step_output_ref("$steps.attack.result.damage");
        assert_eq!(result, Some(("attack", "damage")));

        let result = crate::variables::resolve_step_output_ref("$variables.hp");
        assert_eq!(result, None);
    }

    // ========================================================================
    // 条件分支 Flow 测试
    // ========================================================================

    #[tokio::test]
    async fn test_flow_with_condition_branch() {
        let json = r#"{
            "id": "test",
            "name": "Test",
            "entry": "check",
            "variables": {
                "hp": { "value_type": "integer", "default": 50 }
            },
            "steps": {
                "check": {
                    "kind": { "type": "none" },
                    "transitions": [
                        {
                            "target": "heal",
                            "condition": {
                                "type": "variable",
                                "key": "$variables.hp",
                                "op": "lt",
                                "value": 100
                            }
                        },
                        { "target": "end", "condition": { "type": "always" } }
                    ]
                },
                "heal": {
                    "kind": { "type": "set_variable", "key": "hp", "value": 100 },
                    "output": { "$variables.hp": "$result.value" },
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

        let flow = crate::parser::FlowParser::new().parse_str(json).unwrap();
        let variables = Arc::new(VariableStore::new(
            "test".to_string(),
            flow.variables.clone(),
            None,
        ));
        let executor = FlowExecutor::new_with_defaults(flow, variables.clone(), None);

        executor.run().await.unwrap();
        assert_eq!(executor.status().await, FlowStatus::Completed);
        assert_eq!(variables.get("hp").await, Some(serde_json::json!(100)));
    }

    // ========================================================================
    // 错误处理 + 重试测试
    // ========================================================================

    struct FailingThenSuccessExecutor {
        attempts: std::sync::atomic::AtomicU32,
    }

    #[async_trait::async_trait]
    impl StepExecutor for FailingThenSuccessExecutor {
        async fn execute(
            &self,
            _kind: &StepKind,
            _input: &HashMap<String, Value>,
        ) -> FlowResult<StepResult> {
            let count = self.attempts.fetch_add(1, Ordering::SeqCst);
            if count < 2 {
                Ok(StepResult {
                    success: false,
                    output: None,
                    error: Some("fail".into()),
                })
            } else {
                Ok(StepResult {
                    success: true,
                    output: None,
                    error: None,
                })
            }
        }
    }

    #[tokio::test]
    async fn test_flow_retry_succeeds() {
        let json = r#"{
            "id": "test",
            "name": "Test",
            "entry": "retry_step",
            "steps": {
                "retry_step": {
                    "kind": { "type": "none" },
                    "transitions": [
                        { "target": "end", "condition": { "type": "always" } }
                    ],
                    "max_retries": 3
                },
                "end": {
                    "kind": { "type": "none" },
                    "transitions": []
                }
            }
        }"#;

        let flow = crate::parser::FlowParser::new().parse_str(json).unwrap();
        let variables = Arc::new(VariableStore::new("test".to_string(), HashMap::new(), None));
        let executor = FlowExecutor::new(
            flow,
            variables,
            Arc::new(FailingThenSuccessExecutor {
                attempts: std::sync::atomic::AtomicU32::new(0),
            }),
        );

        executor.run().await.unwrap();
        assert_eq!(executor.status().await, FlowStatus::Completed);
    }

    // ========================================================================
    // on_error 测试
    // ========================================================================

    struct AlwaysFailExecutor;

    #[async_trait::async_trait]
    impl StepExecutor for AlwaysFailExecutor {
        async fn execute(
            &self,
            kind: &StepKind,
            _input: &HashMap<String, Value>,
        ) -> FlowResult<StepResult> {
            // None 类型的 Step 成功，其他失败
            match kind {
                StepKind::None => Ok(StepResult {
                    success: true,
                    output: None,
                    error: None,
                }),
                _ => Ok(StepResult {
                    success: false,
                    output: None,
                    error: Some("always fail".into()),
                }),
            }
        }
    }

    #[tokio::test]
    async fn test_flow_on_error_transition() {
        let json = r#"{
            "id": "test",
            "name": "Test",
            "entry": "main",
            "steps": {
                "main": {
                    "kind": { "type": "none" },
                    "transitions": [
                        { "target": "end", "condition": { "type": "always" } }
                    ],
                    "max_retries": 0,
                    "on_error": "error_handler"
                },
                "error_handler": {
                    "kind": { "type": "none" },
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

        let flow = crate::parser::FlowParser::new().parse_str(json).unwrap();
        let variables = Arc::new(VariableStore::new("test".to_string(), HashMap::new(), None));
        let executor = FlowExecutor::new(flow, variables, Arc::new(AlwaysFailExecutor));

        executor.run().await.unwrap();
        assert_eq!(executor.status().await, FlowStatus::Completed);
    }

    struct AlwaysTrueTemplateConditionHandler;

    #[async_trait::async_trait]
    impl ConditionHandler for AlwaysTrueTemplateConditionHandler {
        fn condition_type(&self) -> &str {
            "template"
        }

        async fn evaluate(
            &self,
            condition: &crate::types::Condition,
            _variables: &VariableStore,
        ) -> FlowResult<bool> {
            match condition {
                crate::types::Condition::Template { .. } => Ok(true),
                _ => Ok(false),
            }
        }
    }

    #[tokio::test]
    async fn test_flow_executor_new_with_custom_condition_handler() {
        let json = r#"{
            "id": "test_custom_condition",
            "name": "Test Custom Condition",
            "entry": "start",
            "steps": {
                "start": {
                    "kind": { "type": "set_variable", "key": "marker", "value": 1 },
                    "transitions": [
                        {
                            "target": "end",
                            "condition": {
                                "type": "template",
                                "template": "btn.png",
                                "threshold": 0.9
                            }
                        }
                    ]
                },
                "end": {
                    "kind": { "type": "set_variable", "key": "marker", "value": 2 },
                    "transitions": []
                }
            }
        }"#;

        let flow = crate::parser::FlowParser::new().parse_str(json).unwrap();
        let variables = Arc::new(VariableStore::new("test".to_string(), HashMap::new(), None));
        let step_executor = Arc::new(DefaultStepExecutor::new(variables.clone(), None));
        let executor = FlowExecutor::new_with_custom(
            flow,
            variables.clone(),
            step_executor,
            vec![Arc::new(AlwaysTrueTemplateConditionHandler)],
        );

        executor.run().await.unwrap();
        assert_eq!(executor.status().await, FlowStatus::Completed);
        assert_eq!(variables.get("marker").await, Some(serde_json::json!(2)));
    }

    struct CustomNoneStepHandler;

    #[async_trait::async_trait]
    impl StepHandler for CustomNoneStepHandler {
        fn step_type(&self) -> &str {
            "none"
        }

        async fn execute(
            &self,
            kind: &StepKind,
            _input: &HashMap<String, Value>,
            _ctx: &crate::step_handlers::StepContext<'_>,
        ) -> FlowResult<StepResult> {
            if matches!(kind, StepKind::None) {
                Ok(StepResult {
                    success: true,
                    output: Some(serde_json::json!({ "custom": true })),
                    error: None,
                })
            } else {
                Err(FlowError::UnsupportedStep(
                    "CustomNoneStepHandler: wrong kind".into(),
                ))
            }
        }
    }

    #[tokio::test]
    async fn test_default_step_executor_with_custom_handlers_overrides_builtin() {
        let json = r#"{
            "id": "test_custom_step_handler",
            "name": "Test Custom Step Handler",
            "entry": "start",
            "steps": {
                "start": {
                    "kind": { "type": "none" },
                    "output": { "$variables.flag": "$result.custom" },
                    "transitions": []
                }
            }
        }"#;

        let flow = crate::parser::FlowParser::new().parse_str(json).unwrap();
        let variables = Arc::new(VariableStore::new("test".to_string(), HashMap::new(), None));
        let step_executor = Arc::new(DefaultStepExecutor::with_custom_handlers(
            variables.clone(),
            None,
            None,
            vec![Arc::new(CustomNoneStepHandler)],
        ));
        let executor = FlowExecutor::new_with_custom(
            flow,
            variables.clone(),
            step_executor,
            crate::condition_handlers::default_condition_handler_arcs(),
        );

        executor.run().await.unwrap();
        assert_eq!(executor.status().await, FlowStatus::Completed);
        assert_eq!(variables.get("flag").await, Some(serde_json::json!(true)));
    }
}
