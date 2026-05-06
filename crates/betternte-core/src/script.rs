//! 脚本运行时共享类型。
//!
//! 定义 `LogLevel`、`FindTemplateOpts`、`OcrResult` 等脚本系统使用的公共类型。

use serde::{Deserialize, Serialize};

use crate::image::Region;

/// 日志级别。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// 模板查找选项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindTemplateOpts {
    pub threshold: Option<f64>,
    pub roi: Option<Region>,
}

/// OCR 识别结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    pub text: String,
    pub region: Region,
    pub confidence: f64,
}
