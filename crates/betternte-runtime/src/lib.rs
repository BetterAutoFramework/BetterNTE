//! betternte-runtime: Flow Engine
//!
//! 统一的 Flow + Step + Transition 模型，替代原有的 TaskGroup / Pipeline / StateMachine。
//!
//! # 核心概念
//!
//! - [`Flow`] — 流程容器
//! - [`Step`] — 执行单元
//! - [`Transition`] — 步骤间转换
//! - [`Condition`] — 转换条件
//! - [`Trigger`] — 触发器

pub mod condition;
pub mod condition_handlers;
pub mod error;
pub mod executor;
pub mod parser;
mod perf_log;
pub mod pipeline_tools;
pub mod sandbox;
pub mod step_handlers;
pub mod trigger;
pub mod types;
pub mod variables;

// Re-exports
pub use condition::ConditionEvaluator;
pub use condition_handlers::ConditionHandler;
pub use error::{FlowError, FlowResult};
pub use executor::{
    DefaultStepExecutor, FlowExecutor, FlowRunner, FlowStatus, InputRunner, StepExecutor,
    StepResult,
};
pub use parser::FlowParser;
pub use pipeline_tools::{
    check_pipeline_file, dump_pipeline_file, PipelineCheckReport, PipelineDump, PipelineSourceKind,
};
pub use sandbox::PermissionGuard;
pub use step_handlers::{StepContext, StepHandler, StepRegistry};
pub use trigger::TriggerManager;
pub use types::*;
pub use variables::VariableStore;
