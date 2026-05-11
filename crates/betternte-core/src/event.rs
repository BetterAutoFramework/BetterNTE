//! 引擎事件类型

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 引擎事件。
///
/// 用于 WebSocket 推送或引擎内部模块间通信。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum EngineEvent {
    TaskStarted {
        task_name: String,
        task_type: String,
        timestamp: DateTime<Utc>,
    },

    TaskStopped {
        task_name: String,
        reason: TaskStopReason,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },

    TaskProgress {
        task_name: String,
        current: u32,
        total: u32,
        message: String,
    },

    ScriptLoaded {
        script_name: String,
        version: String,
        path: String,
    },

    ScriptUnloaded {
        script_name: String,
    },

    CaptureStatusChanged {
        engine_name: String,
        is_capturing: bool,
        fps: f64,
    },

    Error {
        module: String,
        message: String,
        severity: ErrorSeverity,
        recoverable: bool,
    },

    ConfigChanged {
        key: String,
        old_value: Option<String>,
        new_value: String,
    },

    LogMessage {
        level: String,
        module: String,
        message: String,
        timestamp: DateTime<Utc>,
    },

    ScriptCallTrace {
        id: String,
        category: String,
        method: String,
        args: serde_json::Value,
        result: Option<String>,
        success: bool,
        error: Option<String>,
        screenshot_before: Option<String>,
        screenshot_after: Option<String>,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },

    /// Emitted by the file system watcher when data files change on disk.
    /// The client should call `reload_scripts()`, `load_task_groups()`, and
    /// `load_flows()` in response.
    DataChanged,
}

/// 任务停止原因。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStopReason {
    Completed,
    UserCancelled,
    EmergencyStop,
    Error(String),
    Timeout,
}

/// 错误严重级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    Warning,
    Error,
    Fatal,
}
