//! 任务执行器 trait 和错误类型。
//!
//! 定义 `TaskExecutor` trait 和 `TaskError` enum，
//! 使任务调度逻辑与脚本执行逻辑解耦。

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

/// 任务模块错误类型。
#[derive(Error, Debug)]
pub enum TaskError {
    /// 已有任务在运行，无法启动新任务
    #[error("A task is already running")]
    TaskAlreadyRunning,

    /// 当前没有任务在运行
    #[error("No task is running")]
    NoRunningTask,

    /// 任务组中没有启用的任务
    #[error("No enabled tasks in group: {0}")]
    NoEnabledTasks(String),

    /// 任务组未找到
    #[error("Task group not found: {0}")]
    GroupNotFound(String),

    /// 任务被取消
    #[error("Task cancelled")]
    Cancelled,

    /// 任务执行超时
    #[error("Task timeout after {0}ms")]
    Timeout(u64),

    /// 脚本未找到
    #[error("Script not found: {0}")]
    ScriptNotFound(String),

    /// 脚本执行失败
    #[error("Script execution error: {0}")]
    ExecutorError(String),

    /// 任务组执行错误
    #[error("Task group error: {0}")]
    GroupError(String),

    /// IO 错误
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// JSON 序列化/反序列化错误
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// 任务组加载失败
    #[error("Task group load failed: {0}")]
    GroupLoadFailed(String),

    /// 触发器已存在
    #[error("Trigger already exists: {0}")]
    TriggerAlreadyExists(String),

    /// 触发器未找到
    #[error("Trigger not found: {0}")]
    TriggerNotFound(String),

    /// 触发器条件检查失败
    #[error("Trigger condition check failed: {0}")]
    TriggerConditionFailed(String),

    /// 一条龙流程未找到
    #[error("One dragon flow not found: {0}")]
    FlowNotFound(String),
}

/// 任务执行器 trait。
///
/// 实现此 trait 即可接入 TaskRunner / TaskGroupRunner。
/// 不同平台（Windows/ADB/模拟器）各自实现执行逻辑。
#[async_trait]
pub trait TaskExecutor: Send + Sync {
    /// 执行指定脚本。
    async fn execute(&self, script_name: &str, params: &Value) -> Result<Value, TaskError>;

    /// 停止当前执行。
    async fn stop(&self) -> Result<(), TaskError>;
}
