//! Flow Engine 错误类型

use thiserror::Error;

/// Flow Engine 错误
#[derive(Error, Debug)]
pub enum FlowError {
    #[error("Missing entry step: {0}")]
    MissingEntry(String),

    #[error("Empty steps")]
    EmptySteps,

    #[error("Entry not found: {0}")]
    EntryNotFound(String),

    #[error("Step not found: {0}")]
    StepNotFound(String),

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Max depth exceeded: {depth} (max: {max})")]
    MaxDepthExceeded { depth: usize, max: usize },

    #[error("Variable not found: {0}")]
    VariableNotFound(String),

    #[error("Type mismatch for variable {key}: expected {expected}, got {actual}")]
    TypeMismatch {
        key: String,
        expected: String,
        actual: String,
    },

    #[error("Script error: {0}")]
    ScriptError(String),

    #[error("Timeout after {0}ms")]
    Timeout(u64),

    #[error("Cancelled")]
    Cancelled,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Other: {0}")]
    Other(#[from] anyhow::Error),

    #[error("Unsupported step kind: {0}")]
    UnsupportedStep(String),
}

/// Result type alias
pub type FlowResult<T> = Result<T, FlowError>;
