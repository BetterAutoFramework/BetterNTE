//! Script engine trait and core abstractions.
//!
//! Defines the trait-based abstraction for script engines, allowing different
//! runtimes (QuickJS, Lua, JSON pipeline) to be plugged in.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::manifest::Manifest;

// ============================================================================
// ScriptEngine trait — 引擎抽象接口
// ============================================================================

/// 脚本引擎 trait。
#[async_trait]
pub trait ScriptEngine: Send + Sync {
    fn name(&self) -> &str;
    fn supported_types(&self) -> Vec<ScriptType>;
    async fn load(
        &self,
        script_path: &Path,
        manifest: &Manifest,
        data_root: &Path,
    ) -> Result<Box<dyn Script>>;
    async fn unload_all(&self) -> Result<()>;
    fn engine_version(&self) -> &str;
}

// ============================================================================
// Script trait — 单个脚本的抽象
// ============================================================================

/// 已加载的脚本实例。
#[async_trait]
pub trait Script: Send + Sync {
    fn name(&self) -> &str;
    fn script_type(&self) -> ScriptType;

    async fn init(&mut self, _ctx: &Arc<dyn ScriptContext>) -> Result<()> {
        Ok(())
    }
    async fn on_enable(
        &mut self,
        ctx: &Arc<dyn ScriptContext>,
        params: &serde_json::Value,
    ) -> Result<()>;
    async fn start(
        &mut self,
        ctx: &Arc<dyn ScriptContext>,
        config: &serde_json::Value,
    ) -> Result<()>;
    async fn stop(&mut self, ctx: &Arc<dyn ScriptContext>) -> Result<()>;
    async fn on_capture(
        &mut self,
        ctx: &Arc<dyn ScriptContext>,
        frame: &CaptureFrame,
    ) -> Result<()>;
    async fn on_disable(&mut self, ctx: &Arc<dyn ScriptContext>) -> Result<()>;
    async fn destroy(&mut self, _ctx: &Arc<dyn ScriptContext>) -> Result<()> {
        Ok(())
    }
    fn is_cancelled(&self) -> bool;
    fn cancellation_token(&self) -> Option<CancellationToken> {
        None
    }

    /// Get the last return value from start() or on_capture().
    fn last_result(&self) -> Option<&serde_json::Value> {
        None
    }

    /// Call an exported library function.
    ///
    /// Default implementation returns an error for non-library scripts.
    async fn call_function(
        &mut self,
        _ctx: &Arc<dyn ScriptContext>,
        _function: &str,
        _args: &Value,
    ) -> Result<Value> {
        Err(anyhow::anyhow!("Script does not support function calls"))
    }
}

// ============================================================================
// ScriptType — 脚本类型
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ScriptType {
    #[serde(alias = "task", alias = "flow")]
    SoloTask,
    Trigger,
    Library,
}

// ============================================================================
// ScriptContext — 运行时上下文
// ============================================================================

/// 脚本运行时上下文，注入到脚本中供其调用。
#[async_trait]
pub trait ScriptContext: Send + Sync {
    // === 状态 ===
    fn is_cancelled(&self) -> bool;
    fn request_cancel(&self) {}
    fn reset_cancel(&self) {}
    fn get_config(&self) -> &serde_json::Value;
    fn progress(&self, current: u32, total: u32);

    // === Helpers ===
    fn get_fps(&self) -> f64;
    fn get_frame_number(&self) -> u64;
    /// Set the template directory for the current script (no-op by default).
    fn set_template_dir(&self, _dir: PathBuf) {}
    /// Get the current template directory if the context tracks it.
    fn get_template_dir(&self) -> Option<PathBuf> {
        None
    }

    /// Set the design resolution for coordinate scaling (called before script execution).
    fn set_design_resolution(&self, _resolution: Option<(u32, u32)>) {}
    /// Get the current design resolution if set.
    fn get_design_resolution(&self) -> Option<(u32, u32)> { None }
    /// Get current scale factors (scale_x, scale_y). None if no scaling configured.
    fn get_scale_factors(&self) -> Option<(f64, f64)> { None }
    /// Get current actual frame dimensions (width, height).
    fn get_frame_size(&self) -> Option<(u32, u32)> { None }

    /// When false, undeclared manifest permissions log a warning only; when true, abort.
    fn manifest_security_strict(&self) -> bool {
        true
    }

    /// Push manifest permission scope for `ctx.*` bridge checks (paired with [`Self::pop_manifest_permission_scope`]).
    fn push_manifest_permission_scope(&self, _declared: &[String], _strict: bool) {}

    fn pop_manifest_permission_scope(&self) {}

    /// Enforce manifest permission before executing a bridged `ctx` method (Error string aborts the call).
    fn check_manifest_api_permission(&self, _method: &str) -> Result<(), String> {
        Ok(())
    }

    // === 截图 ===
    /// Capture full screen. `force=false` returns cached frame, `true` forces new capture.
    async fn capture(&self, force: bool) -> Result<CaptureFrame>;
    /// Capture a region. `force=false` returns cached frame crop, `true` forces new capture.
    async fn capture_region(&self, region: &Region, force: bool) -> Result<CaptureFrame>;
    /// Save current frame as PNG to the script's store directory. Returns the saved file path.
    async fn save_screenshot(&self, force: bool) -> Result<String>;

    // === 识别 ===
    /// Find template match. Pass `frame` to use an explicit frame, or `None` for cached frame.
    async fn find_template(
        &self,
        name: &str,
        opts: Option<FindTemplateOpts>,
    ) -> Result<Option<MatchResult>>;
    /// Find template matches (multi-result). Pass `frame` to use an explicit frame, or `None` for cached frame.
    async fn find_templates(
        &self,
        name: &str,
        opts: Option<FindTemplateOpts>,
    ) -> Result<Vec<MatchResult>>;

    /// Run several template lookups on **one** cached frame (one decode). Order of results matches `entries`.
    async fn find_template_batch(
        &self,
        entries: &[FindTemplateBatchEntry],
    ) -> Result<Vec<Option<MatchResult>>> {
        let mut out = Vec::with_capacity(entries.len());
        for e in entries {
            out.push(self.find_template(&e.name, Some(e.opts.clone())).await?);
        }
        Ok(out)
    }
    /// OCR on a region. Pass `frame` to use an explicit frame, or `None` for cached frame.
    async fn ocr(&self, region: &Region, text_color: Option<&str>, text_color_tolerance: u8) -> Result<String>;
    /// OCR all text on a frame. Pass `frame` to use an explicit frame, or `None` for cached frame.
    async fn ocr_all(&self) -> Result<Vec<OcrResult>>;
    /// Get pixel color at (x, y). Pass `frame` to use an explicit frame, or `None` for cached frame.
    async fn get_color(&self, x: i32, y: i32) -> Result<String>;
    /// Compare pixel color at (x, y). Pass `frame` to use an explicit frame, or `None` for cached frame.
    async fn color_match(&self, x: i32, y: i32, color: &str, tolerance: u8) -> Result<bool>;
    /// Compare multiple points and return true only when all points match.
    ///
    /// See [`ColorMatchAllOpts`]. When `opts.debug` is true, [`ColorMatchAllResult::points`] lists each point’s
    /// sampled color and match flag. When `opts.shift_max` is set, tries the same integer offset applied to every
    /// point within the inclusive rectangle (after per-axis clamp). `(0, 0)` is always tried first.
    async fn color_match_all(
        &self,
        points: &[ColorMatchPoint],
        opts: &ColorMatchAllOpts,
    ) -> Result<ColorMatchAllResult>;
    /// One-frame scan of a horizontal strip for two target colors (e.g. stamina bar + player marker).
    ///
    /// `opts` JSON (camelCase): `region` `{ x, y, width, height }`, `barColor`, `playerColor`,
    /// optional `barTolerance` / `playerTolerance` (Euclidean u8, default 28),
    /// `stepX` (default 2), `rowOffset` (row within region, default `height/2`),
    /// `minBarRunPx` / `minPlayerRunPx` (defaults 18 / 6).
    async fn scan_slider_strip(&self, opts: &Value) -> Result<Value>;
    /// Horizontal strip: bar left = first `barColor` LTR, bar right = first `barColor` RTL;
    /// player edges similarly. Same `opts` keys as [`scan_slider_strip`] except run-length filters are unused.
    async fn scan_strip_edges(&self, opts: &Value) -> Result<Value>;
    /// Count pixels matching `color` within tolerance. If `opts.roi` is provided, only scan that region.
    ///
    /// `opts` JSON (camelCase): `tolerance` (u8, default 0), `roi` `{ x, y, width, height }`.
    async fn count_color(&self, color: &str, opts: Option<&Value>) -> Result<u32>;

    // === 操作 ===
    async fn click(&self, x: i32, y: i32) -> Result<()>;
    async fn double_click(&self, x: i32, y: i32) -> Result<()>;
    async fn right_click(&self, x: i32, y: i32) -> Result<()>;
    async fn mouse_move(&self, x: i32, y: i32) -> Result<()>;
    async fn mouse_down(&self, button: &str) -> Result<()>;
    async fn mouse_up(&self, button: &str) -> Result<()>;
    async fn scroll(&self, delta: i32) -> Result<()>;
    async fn swipe(&self, x1: i32, y1: i32, x2: i32, y2: i32, duration_ms: u32) -> Result<()>;
    async fn key_down(&self, key: &str) -> Result<()>;
    async fn key_up(&self, key: &str) -> Result<()>;
    async fn key_press(&self, key: &str, duration_ms: Option<u32>) -> Result<()>;
    async fn key_combo(&self, keys: &[String]) -> Result<()>;
    async fn type_text(&self, text: &str) -> Result<()>;

    // === 等待 (time-based) ===
    async fn sleep(&self, ms: u64) -> Result<()>;
    /// Same options as [`Self::find_template`] (`roi`, `threshold`). Pass `None` for full-screen.
    async fn wait_for_template(
        &self,
        name: &str,
        timeout_ms: u64,
        opts: Option<FindTemplateOpts>,
    ) -> Result<Option<MatchResult>>;
    async fn wait_gone(&self, name: &str, timeout_ms: u64) -> Result<bool>;
    async fn wait_for_color(&self, x: i32, y: i32, color: &str, timeout_ms: u64) -> Result<bool>;

    // === 等待 (frame-based) ===
    /// Sleep for N frames at current game FPS.
    async fn sleep_frames(&self, frames: u32) -> Result<()>;
    /// Wait for template match using frame count instead of wall-clock time.
    async fn wait_for_template_frames(
        &self,
        name: &str,
        max_frames: u32,
        opts: Option<FindTemplateOpts>,
    ) -> Result<Option<MatchResult>>;
    /// Wait for template to disappear using frame count.
    async fn wait_gone_frames(&self, name: &str, max_frames: u32) -> Result<bool>;
    /// Wait for color match using frame count.
    async fn wait_for_color_frames(
        &self,
        x: i32,
        y: i32,
        color: &str,
        max_frames: u32,
    ) -> Result<bool>;

    // === 窗口 ===
    async fn find_window(&self, title: &str) -> Result<Option<u64>>;
    async fn activate_window(&self, hwnd: u64) -> Result<()>;
    async fn get_window_rect(&self, hwnd: u64) -> Result<Rect>;
    async fn get_screen_size(&self) -> Result<(u32, u32)>;

    // === 脚本间调用 ===
    async fn run_script(&self, name: &str, params: serde_json::Value) -> Result<serde_json::Value>;
    /// QuickJS: `await ctx.call(library, function, args)`.
    async fn call_library(
        &self,
        library: &str,
        function: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value>;

    // === 工具 ===
    fn log(&self, level: LogLevel, message: &str);
    /// Send notification (requires Notify permission).
    async fn notify(&self, title: &str, body: &str) -> Result<()>;

    // === 文件操作 (manifest-scoped, no permission needed) ===
    /// Read file from `{manifest_dir}/store/{path}`.
    async fn read_store_file(&self, path: &str) -> Result<String>;
    /// Write file to `{manifest_dir}/store/{path}`.
    async fn write_store_file(&self, path: &str, content: &str) -> Result<()>;
    /// List files in `{manifest_dir}/store/{dir}`.
    async fn list_store_files(&self, dir: &str) -> Result<Vec<String>>;

    // === 文件操作 (system-level, requires FileRead/FileWrite permission) ===
    async fn read_file(&self, path: &str) -> Result<String>;
    async fn write_file(&self, path: &str, content: &str) -> Result<()>;
    async fn list_files(&self, dir: &str) -> Result<Vec<String>>;
    async fn file_exists(&self, path: &str) -> Result<bool>;

    // === 网络 ===
    async fn http_get(&self, url: &str) -> Result<String>;
    async fn http_post(&self, url: &str, body: &str) -> Result<String>;

    // === 存储 (manifest-scoped, no permission needed) ===
    async fn storage_get(&self, key: &str) -> Result<Option<serde_json::Value>>;
    async fn storage_set(&self, key: &str, value: serde_json::Value) -> Result<()>;
    async fn storage_delete(&self, key: &str) -> Result<()>;
    async fn storage_keys(&self) -> Result<Vec<String>>;

    // === 插件 (Plugin system) ===
    /// Call a method on a loaded plugin.
    ///
    /// `args_json` is a JSON-encoded `Vec<Value>` of positional arguments.
    /// Returns the JSON-encoded result string.
    async fn plugin_call(
        &self,
        plugin_id: &str,
        method: &str,
        args_json: &str,
    ) -> Result<String>;

    /// List all loaded plugins. Returns JSON-encoded `Vec<PluginInfo>`.
    async fn plugin_list(&self) -> Result<String>;

    /// Get plugin configuration value (from EngineConfig.plugins[id].config).
    fn plugin_config(&self, plugin_id: &str) -> Option<serde_json::Value>;

    /// Check if a plugin is enabled in config.
    fn plugin_enabled(&self, plugin_id: &str) -> bool;
}

/// Image recognition capabilities in script context.
#[async_trait]
pub trait ImageRecognitionContext: ScriptContext {}
impl<T: ScriptContext + ?Sized> ImageRecognitionContext for T {}

/// Input control capabilities in script context.
#[async_trait]
pub trait InputControlContext: ScriptContext {}
impl<T: ScriptContext + ?Sized> InputControlContext for T {}

/// Window operations capabilities in script context.
#[async_trait]
pub trait WindowOpsContext: ScriptContext {}
impl<T: ScriptContext + ?Sized> WindowOpsContext for T {}

/// Network capabilities in script context.
#[async_trait]
pub trait NetworkContext: ScriptContext {}
impl<T: ScriptContext + ?Sized> NetworkContext for T {}

/// Notification capabilities in script context.
#[async_trait]
pub trait NotifyContext: ScriptContext {}
impl<T: ScriptContext + ?Sized> NotifyContext for T {}

/// Storage capabilities in script context.
#[async_trait]
pub trait StorageContext: ScriptContext {}
impl<T: ScriptContext + ?Sized> StorageContext for T {}

/// Inter-script call capabilities in script context.
#[async_trait]
pub trait IpcCallContext: ScriptContext {}
impl<T: ScriptContext + ?Sized> IpcCallContext for T {}

// ============================================================================
// 数据类型
// ============================================================================

#[derive(Clone)]
pub struct CaptureFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResult {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    pub text: String,
    pub region: Region,
    pub confidence: f64,
}

/// Max absolute delta allowed per RGBA channel (screen samples use alpha 255).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RgbaTolerance {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorMatchPoint {
    pub x: i32,
    pub y: i32,
    pub color: String,
    pub tolerance: Option<u8>,
    /// When set, uses per-channel RGBA max deltas instead of [`Self::tolerance`] / default Euclidean distance.
    #[serde(default, rename = "rgbaTolerance", alias = "rgba_tolerance")]
    pub rgba_tolerance: Option<RgbaTolerance>,
}

/// Options for [`ScriptContext::color_match_all`] (QuickJS: second argument object, camelCase keys).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ColorMatchAllOpts {
    #[serde(default = "default_color_match_tolerance")]
    pub default_tolerance: u8,
    /// Default per-channel tolerance for points that omit both `tolerance` and `rgbaTolerance`.
    #[serde(default)]
    pub default_rgba_tolerance: Option<RgbaTolerance>,
    #[serde(default)]
    pub debug: bool,
    #[serde(default)]
    pub shift_max: Option<ColorMatchAllShiftMax>,
}

fn default_color_match_tolerance() -> u8 {
    32
}

impl Default for ColorMatchAllOpts {
    fn default() -> Self {
        Self {
            default_tolerance: 32,
            default_rgba_tolerance: None,
            debug: false,
            shift_max: None,
        }
    }
}

/// Resolves [`betternte_core::ColorTolerance`] for one point (per-point fields override opts defaults).
pub fn color_tolerance_for_match_point(
    point: &ColorMatchPoint,
    opts: &ColorMatchAllOpts,
) -> betternte_core::ColorTolerance {
    use betternte_core::ColorTolerance;
    if let Some(rgba) = point.rgba_tolerance {
        return ColorTolerance::RgbaMaxDelta {
            r: rgba.r,
            g: rgba.g,
            b: rgba.b,
            a: rgba.a,
        };
    }
    if let Some(t) = point.tolerance {
        return ColorTolerance::Euclidean(t);
    }
    if let Some(rgba) = opts.default_rgba_tolerance {
        return ColorTolerance::RgbaMaxDelta {
            r: rgba.r,
            g: rgba.g,
            b: rgba.b,
            a: rgba.a,
        };
    }
    ColorTolerance::Euclidean(opts.default_tolerance)
}

/// `shiftMax: { maxDx, maxDy }` in JSON (snake_case field names also accepted).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorMatchAllShiftMax {
    #[serde(alias = "max_dx")]
    pub max_dx: i32,
    #[serde(alias = "max_dy")]
    pub max_dy: i32,
}

/// Per-point outcome when `color_match_all` is called with `debug: true`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorMatchPointResult {
    /// Coordinates from the input `points` definition.
    pub x: i32,
    pub y: i32,
    /// Pixel coordinates used for sampling (`x + shift.x`, `y + shift.y`).
    pub sample_x: i32,
    pub sample_y: i32,
    pub expected: String,
    pub actual: String,
    /// Euclidean tolerance when that mode applied; `0` when only [`Self::rgba_tolerance`] was used.
    pub tolerance: u8,
    #[serde(default, rename = "rgbaTolerance", alias = "rgba_tolerance")]
    pub rgba_tolerance: Option<RgbaTolerance>,
    pub matched: bool,
}

/// Integer translation applied to every point when `shift_max` search succeeds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ColorMatchShift {
    pub x: i32,
    pub y: i32,
}

/// Result of `color_match_all`; `points` is set only when `debug` was requested.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorMatchAllResult {
    pub all_match: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<ColorMatchPointResult>>,
    /// Translation that made all points match; set only when `all_match` is true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_shift: Option<ColorMatchShift>,
}

/// One entry for [`ScriptContext::find_template_batch`] (JSON: `{ "name": "...", ...findTemplate options }`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FindTemplateBatchEntry {
    /// Template basename or path (same as first argument to [`ScriptContext::find_template`]).
    #[serde(default, alias = "template")]
    pub name: String,
    #[serde(flatten)]
    pub opts: FindTemplateOpts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindTemplateOpts {
    pub threshold: Option<f64>,
    pub roi: Option<Region>,
    pub order_by: Option<FindTemplateOrderBy>,
    pub result_index: Option<i32>,
    pub nms_threshold: Option<f64>,
    pub max_results: Option<usize>,
    /// Chroma key: template pixels matching `#00FF00` within [`Self::green_mask_tolerance`] are excluded from NCC.
    #[serde(default, rename = "greenMask", alias = "green_mask")]
    pub green_mask: bool,
    #[serde(default, rename = "greenMaskTolerance", alias = "green_mask_tolerance")]
    pub green_mask_tolerance: u8,
    /// When `true`, template pixels with alpha ≤ [`Self::alpha_mask_threshold`] are excluded (for PNG templates).
    #[serde(default, rename = "useAlphaMask", alias = "use_alpha_mask")]
    pub use_alpha_mask: bool,
    #[serde(
        default = "default_alpha_mask_threshold",
        rename = "alphaMaskThreshold",
        alias = "alpha_mask_threshold"
    )]
    pub alpha_mask_threshold: u8,
    /// When `true`, convert to grayscale before matching (faster, backward compatible).
    /// When `false`, use full-color BGR matching (better accuracy). Default: `false`.
    #[serde(default = "default_grayscale")]
    pub grayscale: bool,
}

impl Default for FindTemplateOpts {
    fn default() -> Self {
        Self {
            threshold: None,
            roi: None,
            order_by: None,
            result_index: None,
            nms_threshold: None,
            max_results: None,
            green_mask: false,
            green_mask_tolerance: 0,
            use_alpha_mask: false,
            alpha_mask_threshold: default_alpha_mask_threshold(),
            grayscale: false,
        }
    }
}

fn default_alpha_mask_threshold() -> u8 {
    8
}

fn default_grayscale() -> bool {
    true
}

impl FindTemplateOpts {
    pub fn to_template_match_params(&self) -> betternte_core::TemplateMatchParams {
        betternte_core::TemplateMatchParams {
            threshold: self.threshold.map(|t| t as f32).unwrap_or(0.8),
            green_mask: self.green_mask,
            green_mask_tolerance: self.green_mask_tolerance,
            use_alpha_mask: self.use_alpha_mask,
            alpha_mask_threshold: self.alpha_mask_threshold,
            grayscale: self.grayscale,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindTemplateOrderBy {
    Score,
    Horizontal,
    Vertical,
    Area,
    Random,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

// ============================================================================
// CancellationToken
// ============================================================================

#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cancellation_token_new_not_cancelled() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_cancel() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_reset() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
        token.reset();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_default() {
        let token = CancellationToken::default();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_clone_shares_state() {
        let token = CancellationToken::new();
        let cloned = token.clone();
        token.cancel();
        assert!(cloned.is_cancelled());
    }

    #[test]
    fn test_script_type_serde_roundtrip() {
        let types = vec![
            ScriptType::SoloTask,
            ScriptType::Trigger,
            ScriptType::Library,
        ];
        for t in types {
            let json = serde_json::to_string(&t).unwrap();
            let back: ScriptType = serde_json::from_str(&json).unwrap();
            assert_eq!(t, back);
        }
    }

    #[test]
    fn test_script_type_json_values() {
        assert_eq!(
            serde_json::to_string(&ScriptType::SoloTask).unwrap(),
            "\"solo_task\""
        );
        assert_eq!(
            serde_json::to_string(&ScriptType::Trigger).unwrap(),
            "\"trigger\""
        );
        assert_eq!(
            serde_json::to_string(&ScriptType::Library).unwrap(),
            "\"library\""
        );
    }

    #[test]
    fn test_log_level_serde_roundtrip() {
        let levels = vec![
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warn,
            LogLevel::Error,
        ];
        for level in levels {
            let json = serde_json::to_string(&level).unwrap();
            let back: LogLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(level, back);
        }
    }

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn test_match_result_serde_roundtrip() {
        let mr = MatchResult {
            x: 10,
            y: 20,
            width: 100,
            height: 50,
            confidence: 0.95,
        };
        let json = serde_json::to_string(&mr).unwrap();
        let back: MatchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.x, 10);
        assert_eq!(back.y, 20);
        assert_eq!(back.width, 100);
        assert_eq!(back.height, 50);
        assert!((back.confidence - 0.95).abs() < 1e-10);
    }

    #[test]
    fn test_region_serde_roundtrip() {
        let r = Region {
            x: 0,
            y: 0,
            width: 640,
            height: 480,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: Region = serde_json::from_str(&json).unwrap();
        assert_eq!(back.width, 640);
        assert_eq!(back.height, 480);
    }

    #[test]
    fn test_rect_serde_roundtrip() {
        let r = Rect {
            x: 10,
            y: 20,
            width: 30,
            height: 40,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: Rect = serde_json::from_str(&json).unwrap();
        assert_eq!(back.x, 10);
        assert_eq!(back.y, 20);
    }

    #[test]
    fn test_ocr_result_serde_roundtrip() {
        let ocr = OcrResult {
            text: "hello".to_string(),
            region: Region {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            },
            confidence: 0.88,
        };
        let json = serde_json::to_string(&ocr).unwrap();
        let back: OcrResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.text, "hello");
        assert_eq!(back.region.x, 1);
        assert!((back.confidence - 0.88).abs() < 1e-10);
    }

    #[test]
    fn test_find_template_opts_serde_roundtrip() {
        let opts = FindTemplateOpts {
            threshold: Some(0.8),
            roi: Some(Region {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            }),
            order_by: Some(FindTemplateOrderBy::Score),
            result_index: Some(0),
            nms_threshold: Some(0.3),
            max_results: Some(10),
            green_mask: true,
            green_mask_tolerance: 2,
            use_alpha_mask: false,
            alpha_mask_threshold: 8,
        };
        let json = serde_json::to_string(&opts).unwrap();
        let back: FindTemplateOpts = serde_json::from_str(&json).unwrap();
        assert_eq!(back.threshold, Some(0.8));
        assert!(back.roi.is_some());
        assert!(back.green_mask);
        assert_eq!(back.green_mask_tolerance, 2);
    }

    #[test]
    fn test_find_template_opts_none_fields() {
        let opts = FindTemplateOpts {
            threshold: None,
            roi: None,
            order_by: None,
            result_index: None,
            nms_threshold: None,
            max_results: None,
            green_mask: false,
            green_mask_tolerance: 0,
            use_alpha_mask: false,
            alpha_mask_threshold: 8,
        };
        let json = serde_json::to_string(&opts).unwrap();
        let back: FindTemplateOpts = serde_json::from_str(&json).unwrap();
        assert_eq!(back.threshold, None);
        assert_eq!(back.roi, None);
    }

    #[test]
    fn test_capture_frame_clone() {
        let frame = CaptureFrame {
            width: 2,
            height: 2,
            data: vec![0, 0, 0, 255],
        };
        let cloned = frame.clone();
        assert_eq!(cloned.width, 2);
        assert_eq!(cloned.data, vec![0, 0, 0, 255]);
    }
}
