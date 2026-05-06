//! 核心错误类型定义

use thiserror::Error;

/// 核心错误类型。
///
/// betternte-core 自身的错误（配置加载、参数验证等）。
/// 子 crate（capture/input/vision）有各自的错误类型。
#[derive(Error, Debug)]
pub enum CoreError {
    #[error("配置文件未找到: {0}")]
    ConfigNotFound(String),

    #[error("配置解析错误: {0}")]
    ConfigParseError(String),

    #[error("配置验证错误: {0}")]
    ConfigValidationError(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML 错误: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("参数无效: {0}")]
    InvalidArgument(String),

    #[error("不支持的操作: {0}")]
    Unsupported(String),

    #[error("窗口未找到: {0}")]
    WindowNotFound(String),

    #[error("操作超时 ({0}ms)")]
    Timeout(u64),

    #[error("{0}")]
    Other(String),
}

/// 便捷 Result 类型
pub type Result<T> = std::result::Result<T, CoreError>;
