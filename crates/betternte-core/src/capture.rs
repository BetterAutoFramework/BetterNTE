//! 截图引擎 trait 和目标类型。
//!
//! 定义 `ScreenCapture` trait 和 `CaptureTarget` enum，
//! 使截图实现与使用方解耦。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::image::CaptureFrame;
use crate::window::{GameWindow, Rect};

/// Runtime capture policy forwarded from engine config to capture backends.
#[derive(Debug, Clone, Copy, Default)]
pub struct CaptureRuntimeOptions {
    pub crop_to_client: bool,
    pub hdr_to_sdr: bool,
    pub recover_on_resize: bool,
    pub recover_on_monitor_switch: bool,
}

/// 截图目标。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "params")]
pub enum CaptureTarget {
    /// PC 原生窗口
    Window { hwnd: u64 },
    /// 显示器（全屏截图）
    Display { index: u32 },
    /// ADB 设备（Android 模拟器）
    AdbDevice { serial: String },
    /// MuMu 模拟器（内存直读）
    MumuEmulator { index: u32 },
    /// 雷电模拟器（内存直读）
    LdEmulator { index: u32 },
}

impl Default for CaptureTarget {
    fn default() -> Self {
        Self::Display { index: 0 }
    }
}

impl CaptureTarget {
    /// 返回 HWND（如果是窗口目标）。
    pub fn as_hwnd(&self) -> Option<u64> {
        match self {
            CaptureTarget::Window { hwnd } => Some(*hwnd),
            _ => None,
        }
    }
}

/// 截图引擎 trait。
///
/// 所有截图引擎（BitBlt、WGC、ADB、MuMu、LD）实现此 trait。
#[async_trait]
pub trait ScreenCapture: Send + Sync {
    /// 引擎名称（如 "BitBlt"、"WindowsGraphicsCapture"）
    fn name(&self) -> &str;

    /// 是否正在截图
    fn is_capturing(&self) -> bool;

    /// 初始化截图引擎，绑定到目标窗口/设备。
    async fn start(&mut self, target: &CaptureTarget) -> anyhow::Result<()>;

    /// Apply runtime capture options before start/capture.
    fn configure(&self, _options: CaptureRuntimeOptions) {}

    /// 截取一帧。必须在 start() 之后调用。
    async fn capture(&self) -> anyhow::Result<CaptureFrame>;

    /// 停止截图并释放资源。
    async fn stop(&mut self) -> anyhow::Result<()>;

    /// 最近一次截图延迟（毫秒），用于性能监控。
    fn last_latency_ms(&self) -> Option<f64>;

    /// 当前帧率。
    fn fps(&self) -> f64;
}

/// 窗口查找器 trait。
pub trait WindowFinder: Send + Sync {
    /// 按精确标题查找窗口
    fn find_by_title(&self, title: &str) -> anyhow::Result<Vec<GameWindow>>;

    /// 按类名查找窗口
    fn find_by_class(&self, class_name: &str) -> anyhow::Result<Vec<GameWindow>>;

    /// 按进程名查找窗口
    fn find_by_process(&self, process_name: &str) -> anyhow::Result<Vec<GameWindow>>;

    /// Fuzzy search (title or class name contains `keyword`). Empty `keyword` lists visible windows.
    fn find_by_keyword(&self, keyword: &str) -> anyhow::Result<Vec<GameWindow>>;

    /// Like [`find_by_keyword`], but when `process_name` is `Some` and non-empty, results must also
    /// match that executable name (case-insensitive, `.exe` optional).
    fn find_by_keyword_and_process(
        &self,
        keyword: &str,
        process_name: Option<&str>,
    ) -> anyhow::Result<Vec<GameWindow>>;

    /// 获取指定窗口的详细信息
    fn get_window_info(&self, hwnd: u64) -> anyhow::Result<GameWindow>;

    /// 获取窗口客户区矩形
    fn get_client_rect(&self, hwnd: u64) -> anyhow::Result<Rect>;

    /// 检查窗口是否最小化
    fn is_minimized(&self, hwnd: u64) -> bool;
}
