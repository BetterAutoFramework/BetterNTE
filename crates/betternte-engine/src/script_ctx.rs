//! EngineScriptContext — bridges ScriptContext trait to capture/input/vision systems.

use anyhow::Result;
use async_trait::async_trait;
use image::{DynamicImage, GenericImageView};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use std::{cmp, sync::OnceLock};
use tracing::{info, warn};

use betternte_capture::ScreenCapture;
use betternte_core::config::CaptureConfig;
use betternte_core::image::CaptureFrame as CoreCaptureFrame;
use betternte_core::image::{Color, Point};
use betternte_core::input::InputController;
use betternte_core::vision::{ColorDetector, OcrEngine, TemplateMatchParams, TemplateMatcher};
use betternte_core::ColorTolerance;
use betternte_core::OcrConfig;
use betternte_runtime::sandbox::PermissionGuard;
use betternte_runtime::types::{Permission, Permissions, ScriptManifest, ScriptType};
use betternte_script::{
    color_tolerance_for_match_point, manifest_permission_key_for_ctx_method, CancellationToken,
    CaptureFrame, ColorMatchAllOpts, ColorMatchAllResult, ColorMatchPoint, ColorMatchPointResult,
    ColorMatchShift, FindTemplateBatchEntry, FindTemplateOpts, FindTemplateOrderBy, LogLevel,
    MatchResult, OcrResult, Rect, Region, RgbaTolerance, ScriptContext,
};
use betternte_vision::TemplateCache;
use betternte_vision::{apply_text_color_filter, parse_color_str};
use opencv::prelude::*;

#[derive(Clone)]
struct ManifestPermScope {
    declared: HashSet<String>,
    strict: bool,
}

#[derive(Clone)]
pub struct SharedFrameSnapshot {
    pub frame: CoreCaptureFrame,
    pub frame_id: u64,
    pub captured_at: chrono::DateTime<chrono::Utc>,
    pub fps: f64,
}

#[derive(Clone)]
struct ScaledFrameCache {
    source_key: u64,
    image: DynamicImage,
}

#[derive(Clone)]
struct MatFrameCache {
    source_key: u64,
    mat: opencv::core::Mat,
}

#[derive(Clone)]
struct OcrBatchCacheSnapshot {
    key: u64,
    design_res: Option<(u32, u32)>,
    results: Vec<OcrResult>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PerfMode {
    Off,
    Steps,
    Verbose,
}

fn perf_mode() -> PerfMode {
    static MODE: OnceLock<PerfMode> = OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("BETTERNTE_PERF_LOG") {
        Ok(v) => match v.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "steps" => PerfMode::Steps,
            "2" | "verbose" | "all" | "full" => PerfMode::Verbose,
            _ => PerfMode::Off,
        },
        Err(_) => PerfMode::Off,
    })
}

fn perf_slow_threshold_ms() -> f64 {
    static TH: OnceLock<f64> = OnceLock::new();
    *TH.get_or_init(|| {
        std::env::var("BETTERNTE_PERF_CTX_SLOW_MS")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|&v| v >= 0.0)
            .unwrap_or(8.0)
    })
}

fn perf_enabled() -> bool {
    perf_mode() != PerfMode::Off
}

/// Engine-level ScriptContext implementation.
///
/// Bridges script calls to the actual capture, input, and vision subsystems.
pub struct EngineScriptContext {
    config: serde_json::Value,
    cancel: CancellationToken,
    hwnd: std::sync::Mutex<Option<u64>>,
    capture_engine: tokio::sync::Mutex<Option<Box<dyn ScreenCapture>>>,
    capture_started_hwnd: std::sync::Mutex<Option<u64>>,
    input_controller: tokio::sync::Mutex<Option<Box<dyn InputController>>>,
    permissions: PermissionGuard,
    capture_config: std::sync::Mutex<CaptureConfig>,
    ocr_config: std::sync::Mutex<OcrConfig>,
    shared_frame: tokio::sync::RwLock<Option<SharedFrameSnapshot>>,
    allow_fallback_capture: std::sync::atomic::AtomicBool,

    // Vision subsystem (injected)
    template_matcher: Option<Arc<dyn TemplateMatcher>>,
    color_detector: Option<Arc<dyn ColorDetector>>,
    ocr_engine: Option<Arc<tokio::sync::Mutex<dyn OcrEngine>>>,

    // Capture frame cache
    frame_cache: tokio::sync::Mutex<Option<CoreCaptureFrame>>,
    scaled_frame_cache: tokio::sync::Mutex<HashMap<(u32, u32), ScaledFrameCache>>,
    mat_frame_cache: tokio::sync::Mutex<HashMap<(u32, u32), MatFrameCache>>,
    ocr_batch_cache: tokio::sync::Mutex<Option<OcrBatchCacheSnapshot>>,
    frame_number: AtomicU64,

    // FPS tracking
    fps: AtomicU32, // stored as fixed-point: fps * 100

    // Notification — always present; disabled / empty by default, swappable at runtime.
    notification_manager: Arc<tokio::sync::RwLock<betternte_notify::NotificationManager>>,

    // Storage (manifest-scoped)
    storage_path: PathBuf,

    // Event bus for direct log publishing
    event_bus: Option<crate::EventBus>,

    /// Active replay **`Record`** timeline sink (cloned into capture loop separately).
    replay_timeline_sink:
        Arc<tokio::sync::Mutex<Option<Arc<crate::replay_recorder::ReplaySessionInner>>>>,

    // Per-script template directory (set before each script execution)
    template_dir: std::sync::Mutex<PathBuf>,
    /// Decoded template images keyed by filename inside [`TemplateCache`] (cleared when `template_dir` changes).
    template_file_cache: Arc<TemplateCache>,

    // Callback for inter-script execution (set by the engine)
    script_runner: std::sync::Mutex<
        Option<
            std::sync::Arc<
                dyn Fn(
                        String,
                        serde_json::Value,
                    ) -> std::pin::Pin<
                        Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send>,
                    > + Send
                    + Sync,
            >,
        >,
    >,
    library_runner: std::sync::Mutex<
        Option<
            std::sync::Arc<
                dyn Fn(
                        String,
                        String,
                        serde_json::Value,
                    ) -> std::pin::Pin<
                        Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send>,
                    > + Send
                    + Sync,
            >,
        >,
    >,

    manifest_perm_stack: std::sync::Mutex<Vec<ManifestPermScope>>,
    manifest_security_strict: AtomicBool,

    // Resolution scaling: design [w, h] from manifest.json. None = no scaling.
    design_resolution: std::sync::Mutex<Option<(u32, u32)>>,
}

impl EngineScriptContext {
    pub fn new(config: serde_json::Value) -> Self {
        Self::with_manifest_dir(config, PathBuf::new())
    }

    /// Create with a manifest directory for storage and store file operations.
    pub fn with_manifest_dir(config: serde_json::Value, manifest_dir: PathBuf) -> Self {
        let manifest = ScriptManifest {
            schema_version: 1,
            uuid: None,
            source: None,
            name: "engine".to_string(),
            display_name: "Engine Context".to_string(),
            version: "1.0.0".to_string(),
            script_type: ScriptType::Task,
            entry: String::new(),
            author: String::new(),
            description: String::new(),
            dependencies: vec![],
            permissions: Permissions {
                required: vec![],
                optional: vec![],
            },
            params_schema: None,
            output_schema: None,
            tags: vec![],
        };
        let permissions = PermissionGuard::new(&manifest, "system");

        Self {
            config,
            cancel: CancellationToken::new(),
            hwnd: std::sync::Mutex::new(None),
            capture_engine: tokio::sync::Mutex::new(None),
            capture_started_hwnd: std::sync::Mutex::new(None),
            input_controller: tokio::sync::Mutex::new(None),
            permissions,
            capture_config: std::sync::Mutex::new(CaptureConfig::default()),
            ocr_config: std::sync::Mutex::new(OcrConfig::default()),
            shared_frame: tokio::sync::RwLock::new(None),
            allow_fallback_capture: std::sync::atomic::AtomicBool::new(true),
            template_matcher: None,
            color_detector: None,
            ocr_engine: None,
            frame_cache: tokio::sync::Mutex::new(None),
            scaled_frame_cache: tokio::sync::Mutex::new(HashMap::new()),
            mat_frame_cache: tokio::sync::Mutex::new(HashMap::new()),
            ocr_batch_cache: tokio::sync::Mutex::new(None),
            frame_number: AtomicU64::new(0),
            fps: AtomicU32::new(0),
            notification_manager: Arc::new(tokio::sync::RwLock::new({
                let mut mgr = betternte_notify::NotificationManager::new();
                mgr.set_enabled(false);
                mgr
            })),
            storage_path: manifest_dir.join("storage.json"),
            event_bus: None,
            replay_timeline_sink: Arc::new(tokio::sync::Mutex::new(None)),
            template_dir: std::sync::Mutex::new(PathBuf::new()),
            template_file_cache: Arc::new(TemplateCache::new(64)),
            script_runner: std::sync::Mutex::new(None),
            library_runner: std::sync::Mutex::new(None),
            manifest_perm_stack: std::sync::Mutex::new(Vec::new()),
            manifest_security_strict: AtomicBool::new(true),
            design_resolution: std::sync::Mutex::new(None),
        }
    }

    pub fn set_hwnd(&self, hwnd: u64) {
        *self.hwnd.lock().unwrap() = Some(hwnd);
    }

    pub fn has_hwnd(&self) -> bool {
        self.hwnd.lock().ok().and_then(|h| *h).is_some()
    }

    pub fn cancel_token(&self) -> &CancellationToken {
        &self.cancel
    }

    /// From engine config: strict = abort on missing manifest permissions; normal = warn only.
    pub fn set_manifest_security_strict(&self, strict: bool) {
        self.manifest_security_strict
            .store(strict, Ordering::Relaxed);
    }

    pub async fn set_input_controller(&self, controller: Box<dyn InputController>) {
        *self.input_controller.lock().await = Some(controller);
    }

    /// Public input helpers for desktop clients (Tauri/CLI debug pages).
    pub async fn input_key_down(&self, key: &str) -> Result<()> {
        <Self as ScriptContext>::key_down(self, key).await
    }

    pub async fn input_key_up(&self, key: &str) -> Result<()> {
        <Self as ScriptContext>::key_up(self, key).await
    }

    pub async fn input_key_tap(&self, key: &str, duration_ms: Option<u32>) -> Result<()> {
        <Self as ScriptContext>::key_press(self, key, duration_ms).await
    }

    pub async fn input_mouse_move(&self, x: i32, y: i32) -> Result<()> {
        <Self as ScriptContext>::mouse_move(self, x, y).await
    }

    pub async fn input_scroll(&self, delta: i32) -> Result<()> {
        <Self as ScriptContext>::scroll(self, delta).await
    }

    pub async fn input_click(&self, x: i32, y: i32) -> Result<()> {
        <Self as ScriptContext>::click(self, x, y).await
    }

    pub async fn input_right_click(&self, x: i32, y: i32) -> Result<()> {
        <Self as ScriptContext>::right_click(self, x, y).await
    }

    /// Low-level mouse button down for manual input testing UI.
    pub async fn input_mouse_down(&self, button: betternte_core::MouseButton) -> Result<()> {
        let guard = self.input_controller.lock().await;
        if let Some(ref ctrl) = *guard {
            ctrl.mouse_down(button).await
        } else {
            Err(anyhow::anyhow!("input controller not ready"))
        }
    }

    /// Low-level mouse button up for manual input testing UI.
    pub async fn input_mouse_up(&self, button: betternte_core::MouseButton) -> Result<()> {
        let guard = self.input_controller.lock().await;
        if let Some(ref ctrl) = *guard {
            ctrl.mouse_up(button).await
        } else {
            Err(anyhow::anyhow!("input controller not ready"))
        }
    }

    pub fn set_template_matcher(&mut self, matcher: Arc<dyn TemplateMatcher>) {
        self.template_matcher = Some(matcher);
    }

    pub fn set_color_detector(&mut self, detector: Arc<dyn ColorDetector>) {
        self.color_detector = Some(detector);
    }

    pub fn set_ocr_engine(&mut self, engine: Arc<tokio::sync::Mutex<dyn OcrEngine>>) {
        self.ocr_engine = Some(engine);
    }

    /// Builder-time notifier injection (before the context is wrapped in `Arc`).
    pub fn set_notification_manager(&mut self, mgr: betternte_notify::NotificationManager) {
        self.notification_manager = Arc::new(tokio::sync::RwLock::new(mgr));
    }

    /// Runtime notifier swap (e.g. from [`crate::Engine::set_config`]).
    pub async fn replace_notification_manager(&self, mgr: betternte_notify::NotificationManager) {
        *self.notification_manager.write().await = mgr;
    }

    pub fn set_event_bus(&mut self, bus: crate::EventBus) {
        self.event_bus = Some(bus);
    }

    pub(crate) async fn set_replay_timeline_sink(
        &self,
        sink: Option<Arc<crate::replay_recorder::ReplaySessionInner>>,
    ) {
        *self.replay_timeline_sink.lock().await = sink;
    }

    pub fn set_capture_config(&self, cfg: CaptureConfig) {
        *self.capture_config.lock().unwrap() = cfg;
    }

    pub fn set_ocr_config(&self, cfg: OcrConfig) {
        *self.ocr_config.lock().unwrap() = cfg;
    }

    pub fn set_allow_fallback_capture(&self, allow: bool) {
        self.allow_fallback_capture.store(allow, Ordering::Relaxed);
    }

    pub async fn update_shared_frame(&self, frame: CoreCaptureFrame, fps: f64) {
        let frame_id = self.frame_number.fetch_add(1, Ordering::Relaxed) + 1;
        let snapshot = SharedFrameSnapshot {
            frame,
            frame_id,
            captured_at: chrono::Utc::now(),
            fps,
        };

        self.set_fps(fps);
        // Single full-frame copy in memory: `get_cached_frame` reads `shared_frame` first.
        // Avoid duplicating ~width*height*4 bytes into `frame_cache` on every tick.
        *self.shared_frame.write().await = Some(snapshot);
    }

    pub fn set_template_dir(&self, dir: PathBuf) {
        *self.template_dir.lock().unwrap() = dir;
        self.template_file_cache.clear();
    }

    /// Get current scale factors by comparing design resolution to latest frame size.
    /// Returns `(scale_x, scale_y)` or `None` if no scaling configured or no frame available.
    fn current_scale_factors(&self) -> Option<(f64, f64)> {
        let design: Option<(u32, u32)> = *self.design_resolution.lock().unwrap();
        let design = design?;
        let frame = self.shared_frame.try_read().ok()?;
        let snap = frame.as_ref()?;
        let sx = snap.frame.width as f64 / design.0 as f64;
        let sy = snap.frame.height as f64 / design.1 as f64;
        if sx == 1.0 && sy == 1.0 {
            return None; // No scaling needed
        }
        Some((sx, sy))
    }

    /// Reverse-scale a point from design resolution to actual screen coordinates.
    fn reverse_scale_point(&self, x: i32, y: i32) -> (i32, i32) {
        if let Some((sx, sy)) = self.current_scale_factors() {
            ((x as f64 * sx).round() as i32, (y as f64 * sy).round() as i32)
        } else {
            (x, y)
        }
    }

    /// Reverse-scale a region from design resolution to actual screen coordinates.
    fn reverse_scale_region(&self, r: &Region) -> Region {
        if let Some((sx, sy)) = self.current_scale_factors() {
            Region {
                x: (r.x as f64 * sx).round() as i32,
                y: (r.y as f64 * sy).round() as i32,
                width: (r.width as f64 * sx).round() as u32,
                height: (r.height as f64 * sy).round() as u32,
            }
        } else {
            r.clone()
        }
    }

    pub fn set_script_runner(
        &self,
        runner: Arc<
            dyn Fn(
                    String,
                    serde_json::Value,
                ) -> std::pin::Pin<
                    Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send>,
                > + Send
                + Sync,
        >,
    ) {
        *self.script_runner.lock().unwrap() = Some(runner);
    }

    pub fn set_library_runner(
        &self,
        runner: Arc<
            dyn Fn(
                    String,
                    String,
                    serde_json::Value,
                ) -> std::pin::Pin<
                    Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send>,
                > + Send
                + Sync,
        >,
    ) {
        *self.library_runner.lock().unwrap() = Some(runner);
    }

    fn parse_mouse_button_name(name: &str) -> Option<betternte_core::MouseButton> {
        match name.trim().to_ascii_lowercase().as_str() {
            "left" => Some(betternte_core::MouseButton::Left),
            "right" => Some(betternte_core::MouseButton::Right),
            "middle" | "mid" => Some(betternte_core::MouseButton::Middle),
            "x1" => Some(betternte_core::MouseButton::X1),
            "x2" => Some(betternte_core::MouseButton::X2),
            _ => None,
        }
    }

    /// Set the current game FPS (for frame-based timing).
    pub fn set_fps(&self, fps: f64) {
        self.fps.store((fps * 100.0) as u32, Ordering::Relaxed);
    }

    // ── Internal helpers ────────────────────────────────────────────────

    fn get_fps_value(&self) -> f64 {
        let raw = self.fps.load(Ordering::Relaxed);
        if raw == 0 {
            60.0
        } else {
            raw as f64 / 100.0
        }
    }

    fn replay_pack_type_text_payload(text: &str) -> Value {
        const PREVIEW_CHARS: usize = 120;
        let count = text.chars().count();
        if count <= PREVIEW_CHARS {
            json!({"text": text})
        } else {
            let preview: String = text.chars().take(PREVIEW_CHARS).collect();
            json!({
                "text_len": text.len(),
                "char_len": count,
                "truncated_preview": preview,
            })
        }
    }

    pub(crate) async fn record_script_input(
        &self,
        method: &str,
        mut args_payload: Map<String, Value>,
        ok: bool,
        error: Option<String>,
    ) {
        let sess = match self.replay_timeline_sink.lock().await.clone() {
            Some(s) => s,
            None => return,
        };
        let frame_ref = self
            .shared_frame
            .read()
            .await
            .as_ref()
            .map(|snap| snap.frame_id);
        args_payload.insert("frame_ref".into(), json!(frame_ref));
        let args = Value::Object(args_payload);
        if let Err(e) = sess.append_script_input(method, args, ok, error).await {
            warn!(
                error = %e,
                method = method,
                "replay: append script_input timeline failed",
            );
        }
    }

    async fn replay_record_script_input_if_recording(
        &self,
        _method: &str,
        _args_payload: Map<String, Value>,
        result: &Result<(), anyhow::Error>,
    ) {
        // Deprecated call path: recording now happens in RecordingInputController.
        let _ = result;
    }

    fn frame_key(frame: &CoreCaptureFrame) -> u64 {
        if frame.sequence != 0 {
            frame.sequence
        } else {
            let ts = frame.timestamp.timestamp_millis().max(0) as u64;
            ts ^ ((frame.width as u64) << 21) ^ ((frame.height as u64) << 9)
        }
    }

    async fn get_cached_core_frame(&self) -> Result<CoreCaptureFrame> {
        {
            let shared = self.shared_frame.read().await;
            if let Some(snapshot) = shared.as_ref() {
                if perf_enabled() {
                    tracing::trace!(
                        target: "betternte_perf",
                        frame_id = snapshot.frame_id,
                        event = "frame_reuse_hit",
                        source = "shared_frame",
                        "frame_reuse_hit"
                    );
                }
                return Ok(snapshot.frame.clone());
            }
        }
        {
            let cache = self.frame_cache.lock().await;
            if let Some(ref frame) = *cache {
                if perf_enabled() {
                    tracing::trace!(
                        target: "betternte_perf",
                        frame_key = Self::frame_key(frame),
                        event = "frame_reuse_hit",
                        source = "frame_cache",
                        "frame_reuse_hit"
                    );
                }
                return Ok(frame.clone());
            }
        }
        if self.allow_fallback_capture.load(Ordering::Relaxed) {
            let frame = self.do_capture_core().await?;
            Ok(frame)
        } else {
            Err(anyhow::anyhow!(
                "No shared frame available and fallback capture is disabled"
            ))
        }
    }

    /// Get the cached frame, capturing a new one if cache is empty.
    async fn get_cached_frame(&self) -> Result<CaptureFrame> {
        let frame = self.get_cached_core_frame().await?;
        Ok(core_frame_to_script(&frame))
    }

    /// Force capture a new frame and update the cache.
    async fn do_capture(&self) -> Result<CaptureFrame> {
        let core = self.do_capture_core().await?;
        Ok(core_frame_to_script(&core))
    }

    async fn do_capture_core(&self) -> Result<CoreCaptureFrame> {
        let hwnd = *self.hwnd.lock().unwrap();
        let hwnd = hwnd.ok_or_else(|| anyhow::anyhow!("No window bound for capture"))?;

        let needs_reinit = {
            let started = self.capture_started_hwnd.lock().unwrap();
            *started != Some(hwnd)
        };

        if needs_reinit {
            let config = self.capture_config.lock().unwrap().clone();
            let target = betternte_capture::CaptureTarget::Window { hwnd };
            let mut engine = betternte_capture::factory::create_capture_engine_with_fps_for_target(
                &config.method,
                &config.method_whitelist,
                config.fps_cap,
                Some(&target),
            )
            .map_err(|e| anyhow::anyhow!("Failed to create capture engine: {}", e))?;

            engine.start(&target).await?;

            // Warmup
            let _ = engine.capture().await?;

            *self.capture_engine.lock().await = Some(engine);
            *self.capture_started_hwnd.lock().unwrap() = Some(hwnd);
        }

        let mut engine_guard = self.capture_engine.lock().await;
        let engine = engine_guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Capture engine not initialized"))?;

        let mut core_frame: CoreCaptureFrame = engine.capture().await?;
        core_frame.source = "engine_fallback".to_string();
        core_frame.sequence = self.frame_number.fetch_add(1, Ordering::Relaxed) + 1;

        // Update cache
        *self.frame_cache.lock().await = Some(core_frame.clone());

        Ok(core_frame)
    }

    /// Convert a core CaptureFrame to DynamicImage for vision operations.
    fn frame_to_dynamic_image(frame: &CoreCaptureFrame) -> Result<DynamicImage> {
        frame
            .to_dynamic_image()
            .map_err(|e| anyhow::anyhow!("Frame conversion error: {}", e))
    }

    /// Convert a core CaptureFrame to an OpenCV Mat, skipping DynamicImage entirely.
    /// Works with any pixel format (BGRA/RGBA → 4ch, BGR/RGB → 3ch, Gray → 1ch).
    fn frame_to_mat(frame: &CoreCaptureFrame) -> Result<opencv::core::Mat> {
        let w = frame.width as i32;
        let h = frame.height as i32;
        let channels = frame.format.bytes_per_pixel() as i32;

        let cv_type = match channels {
            4 => opencv::core::CV_8UC4,
            3 => opencv::core::CV_8UC3,
            1 => opencv::core::CV_8UC1,
            _ => return Err(anyhow::anyhow!("Unsupported pixel format channel count: {}", channels)),
        };

        // Build a 2-D Mat directly from the flat byte slice.
        // Mat::new_rows_cols_with_data creates a borrowed Mat; try_clone makes it owned.
        let data = &*frame.data;
        let mat = match channels {
            4 => {
                // Reinterpret flat &[u8] as &[Vec4b] (4-byte BGRA pixels)
                let pixel_count = (w * h) as usize;
                assert!(data.len() >= pixel_count * 4, "frame data too short");
                let bgra_data: &[opencv::core::Vec4b] = unsafe {
                    std::slice::from_raw_parts(data.as_ptr() as *const opencv::core::Vec4b, pixel_count)
                };
                let borrowed = opencv::core::Mat::new_rows_cols_with_data(h, w, bgra_data)
                    .map_err(|e| anyhow::anyhow!("Mat::new_rows_cols_with_data error: {}", e))?;
                borrowed.try_clone()
                    .map_err(|e| anyhow::anyhow!("Mat::try_clone error: {}", e))?
            }
            3 => {
                let pixel_count = (w * h) as usize;
                assert!(data.len() >= pixel_count * 3, "frame data too short");
                let bgr_data: &[opencv::core::Vec3b] = unsafe {
                    std::slice::from_raw_parts(data.as_ptr() as *const opencv::core::Vec3b, pixel_count)
                };
                let borrowed = opencv::core::Mat::new_rows_cols_with_data(h, w, bgr_data)
                    .map_err(|e| anyhow::anyhow!("Mat::new_rows_cols_with_data error: {}", e))?;
                borrowed.try_clone()
                    .map_err(|e| anyhow::anyhow!("Mat::try_clone error: {}", e))?
            }
            1 => {
                let borrowed = opencv::core::Mat::new_rows_cols_with_data(h, w, data)
                    .map_err(|e| anyhow::anyhow!("Mat::new_rows_cols_with_data error: {}", e))?;
                borrowed.try_clone()
                    .map_err(|e| anyhow::anyhow!("Mat::try_clone error: {}", e))?
            }
            _ => unreachable!(),
        };

        // Verify the Mat has the expected type and dimensions
        debug_assert_eq!(mat.rows(), h);
        debug_assert_eq!(mat.cols(), w);
        debug_assert_eq!(mat.typ() & 7, cv_type & 7); // compare depth bits

        Ok(mat)
    }

    async fn get_decoded_frame_for_vision(&self) -> Result<(CoreCaptureFrame, DynamicImage)> {
        let frame = self.get_cached_core_frame().await?;
        let key = Self::frame_key(&frame);

        // Determine target resolution: design_resolution if set, otherwise native frame size.
        let design: Option<(u32, u32)> = *self.design_resolution.lock().unwrap();
        let (target_w, target_h) = design.unwrap_or((frame.width, frame.height));

        // Check multi-resolution cache.
        {
            let cache = self.scaled_frame_cache.lock().await;
            if let Some(snap) = cache.get(&(target_w, target_h)) {
                if snap.source_key == key {
                    if perf_enabled() {
                        tracing::trace!(
                            target: "betternte_perf",
                            event = "frame_reuse_hit",
                            source = "scaled_frame_cache",
                            frame_key = key,
                            target_w = target_w,
                            target_h = target_h,
                            "frame_reuse_hit"
                        );
                    }
                    return Ok((frame, snap.image.clone()));
                }
            }
        }

        // Cache miss — decode and optionally scale.
        let started = Instant::now();
        let decoded = Self::frame_to_dynamic_image(&frame)?;
        let needs_scale = target_w != frame.width || target_h != frame.height;
        let image = if needs_scale {
            decoded.resize_exact(target_w, target_h, image::imageops::FilterType::Nearest)
        } else {
            decoded
        };
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        if perf_enabled() {
            let mode = perf_mode();
            if mode == PerfMode::Verbose || elapsed_ms >= perf_slow_threshold_ms() {
                tracing::info!(
                    target: "betternte_perf",
                    event = "decode_frame",
                    frame_key = key,
                    target_w = target_w,
                    target_h = target_h,
                    scaled = needs_scale,
                    ms = elapsed_ms,
                    "decode_frame"
                );
            }
        }
        self.scaled_frame_cache.lock().await.insert(
            (target_w, target_h),
            ScaledFrameCache {
                source_key: key,
                image: image.clone(),
            },
        );
        Ok((frame, image))
    }

    async fn get_decoded_mat_for_vision(&self) -> Result<(CoreCaptureFrame, opencv::core::Mat)> {
        let frame = self.get_cached_core_frame().await?;
        let key = Self::frame_key(&frame);

        let design: Option<(u32, u32)> = *self.design_resolution.lock().unwrap();
        let (target_w, target_h) = design.unwrap_or((frame.width, frame.height));

        // Check mat cache
        {
            let cache = self.mat_frame_cache.lock().await;
            if let Some(snap) = cache.get(&(target_w, target_h)) {
                if snap.source_key == key {
                    return Ok((frame, snap.mat.clone()));
                }
            }
        }

        // Cache miss — create Mat from raw bytes (zero-copy where possible)
        let started = Instant::now();
        let mat = Self::frame_to_mat(&frame)?;

        // Resize if capture resolution != design resolution
        let mat = if target_w != frame.width || target_h != frame.height {
            use opencv::core::Size;
            use opencv::imgproc;
            let mut resized = opencv::core::Mat::default();
            let size = Size::new(target_w as i32, target_h as i32);
            imgproc::resize(&mat, &mut resized, size, 0.0, 0.0, imgproc::INTER_NEAREST)
                .map_err(|e| anyhow::anyhow!("cv::resize error: {}", e))?;
            resized
        } else {
            mat
        };

        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        if perf_enabled() {
            let mode = perf_mode();
            if mode == PerfMode::Verbose || elapsed_ms >= perf_slow_threshold_ms() {
                tracing::info!(
                    target: "betternte_perf",
                    event = "decode_mat",
                    frame_key = key,
                    target_w = target_w,
                    target_h = target_h,
                    ms = elapsed_ms,
                    "decode_mat"
                );
            }
        }

        // Store in cache
        self.mat_frame_cache.lock().await.insert(
            (target_w, target_h),
            MatFrameCache { source_key: key, mat: mat.clone() },
        );

        Ok((frame, mat))
    }

    fn ocr_batch_eligible(region: &Region, frame_w: u32, frame_h: u32) -> bool {
        if frame_w == 0 || frame_h == 0 {
            return false;
        }
        let rw = cmp::min(region.width, frame_w) as u64;
        let rh = cmp::min(region.height, frame_h) as u64;
        let region_px = rw.saturating_mul(rh);
        if region_px == 0 {
            return false;
        }
        let frame_px = frame_w as u64 * frame_h as u64;
        let ratio = region_px as f64 / frame_px as f64;
        // Conservative: large regions benefit from full-frame OCR reuse.
        ratio >= 0.20
    }

    /// Validate a store path stays within the store directory.
    fn validate_store_path(&self, path: &str) -> Result<PathBuf> {
        let store_dir = self.current_store_dir();
        let cleaned = path.trim_start_matches('/').trim_start_matches('\\');
        let full = store_dir.join(cleaned);
        // Canonicalize to prevent path traversal
        let canonical = if full.exists() {
            full.canonicalize().unwrap_or(full.clone())
        } else {
            // For new files, check the parent
            if let Some(parent) = full.parent() {
                if parent.exists() {
                    let canon_parent = parent.canonicalize().unwrap_or(parent.to_path_buf());
                    canon_parent.join(full.file_name().unwrap_or_default())
                } else {
                    full
                }
            } else {
                full
            }
        };
        let store_canonical = if store_dir.exists() {
            store_dir.canonicalize().unwrap_or(store_dir.clone())
        } else {
            store_dir.clone()
        };
        if !canonical.starts_with(&store_canonical) {
            return Err(anyhow::anyhow!("Path traversal detected: {}", path));
        }
        Ok(canonical)
    }

    fn current_manifest_dir(&self) -> PathBuf {
        let from_template = self.template_dir.lock().unwrap().clone();
        if !from_template.as_os_str().is_empty() {
            return from_template;
        }
        self.storage_path
            .parent()
            .map_or_else(PathBuf::new, std::path::Path::to_path_buf)
    }

    fn current_storage_path(&self) -> PathBuf {
        self.current_manifest_dir().join("storage.json")
    }

    fn current_store_dir(&self) -> PathBuf {
        self.current_manifest_dir().join("store")
    }

    /// Read the storage.json file.
    async fn read_storage_data(&self) -> HashMap<String, serde_json::Value> {
        let storage_path = self.current_storage_path();
        tokio::fs::read_to_string(&storage_path)
            .await
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Write the storage.json file.
    async fn write_storage_data(&self, data: &HashMap<String, serde_json::Value>) -> Result<()> {
        let storage_path = self.current_storage_path();
        if let Some(parent) = storage_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let json = serde_json::to_string_pretty(data)?;
        tokio::fs::write(&storage_path, json).await?;
        Ok(())
    }

    /// Template match on an already-decoded full-frame image (`frame_w` / `frame_h` for ROI clamp).
    async fn find_templates_on_decoded_frame(
        &self,
        frame_w: u32,
        frame_h: u32,
        frame_mat: &opencv::core::Mat,
        name: &str,
        opts: Option<&FindTemplateOpts>,
    ) -> Result<Vec<MatchResult>> {
        let matcher = match self.template_matcher.as_ref() {
            Some(m) => m,
            None => {
                warn!("find_template: no template matcher injected");
                return Ok(vec![]);
            }
        };

        let match_params: TemplateMatchParams = opts
            .map(|o| o.to_template_match_params())
            .unwrap_or_else(|| TemplateMatchParams::with_threshold(0.8));

        let template_dir = self.template_dir.lock().unwrap().clone();
        let template_file = {
            let p = std::path::Path::new(name);
            if p.extension().is_some() {
                name.to_string()
            } else {
                format!("{}.png", name)
            }
        };
        let template_path = template_dir.join("templates").join(template_file);

        if !template_path.exists() {
            warn!(template = name, path = %template_path.display(), "find_template: template image not found");
            return Ok(vec![]);
        }

        let template_img = self
            .template_file_cache
            .load(&template_path)
            .map_err(|e| anyhow::anyhow!("Failed to load template '{}': {}", name, e))?;

        let (search_mat, roi_offset_x, roi_offset_y): (opencv::core::Mat, i32, i32) =
            if let Some(roi) = opts.and_then(|o| o.roi.as_ref()) {
                let x = roi.x.max(0) as i32;
                let y = roi.y.max(0) as i32;
                let w = (roi.width as i32).min(frame_w as i32 - x);
                let h = (roi.height as i32).min(frame_h as i32 - y);
                if w <= 0 || h <= 0 {
                    warn!(
                        name,
                        roi_x = roi.x,
                        roi_y = roi.y,
                        roi_w = roi.width,
                        roi_h = roi.height,
                        frame_w = frame_w,
                        frame_h = frame_h,
                        "find_template: roi is out of frame bounds"
                    );
                    return Ok(vec![]);
                }
                let rect = opencv::core::Rect::new(x, y, w, h);
                let cropped = frame_mat.roi(rect)
                    .map_err(|e| anyhow::anyhow!("Mat::roi error: {}", e))?
                    .try_clone()?;
                (cropped, x, y)
            } else {
                (frame_mat.try_clone()?, 0, 0)
            };

        let results = match matcher
            .match_template(&search_mat, &template_img, &match_params)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                return Err(e);
            }
        };
        let mut mapped: Vec<MatchResult> = results
            .into_iter()
            .map(|r| MatchResult {
                x: r.position.x + roi_offset_x,
                y: r.position.y + roi_offset_y,
                width: r.width,
                height: r.height,
                confidence: r.score as f64,
            })
            .collect();

        let nms_threshold = opts
            .and_then(|o| o.nms_threshold)
            .unwrap_or(0.3)
            .clamp(0.0, 1.0) as f32;
        mapped = apply_nms(mapped, nms_threshold);

        let order = opts
            .and_then(|o| o.order_by)
            .unwrap_or(FindTemplateOrderBy::Score);
        sort_matches(&mut mapped, order);

        if let Some(max_results) = opts.and_then(|o| o.max_results) {
            mapped.truncate(max_results);
        }
        Ok(mapped)
    }
}

// ── Conversion helpers ──────────────────────────────────────────────────

fn core_frame_to_script(frame: &CoreCaptureFrame) -> CaptureFrame {
    CaptureFrame {
        width: frame.width,
        height: frame.height,
        data: frame.data.clone(),
    }
}

fn parse_hex_color(s: &str) -> Option<Color> {
    let s = s.trim().strip_prefix('#').unwrap_or(s);
    match s.len() {
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            Some(Color { r, g, b, a: 255 })
        }
        8 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            let a = u8::from_str_radix(&s[6..8], 16).ok()?;
            Some(Color { r, g, b, a })
        }
        _ => None,
    }
}

fn color_to_hex(c: &Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// Cluster sorted x samples into runs where consecutive gaps are <= `gap_max`.
fn color_runs_from_x_samples(mut xs: Vec<i32>, gap_max: i32) -> Vec<(i32, i32)> {
    if xs.is_empty() {
        return vec![];
    }
    xs.sort_unstable();
    xs.dedup();
    let mut runs = Vec::new();
    let mut s = xs[0];
    let mut e = xs[0];
    for &x in xs.iter().skip(1) {
        if x - e <= gap_max {
            e = x;
        } else {
            runs.push((s, e));
            s = x;
            e = x;
        }
    }
    runs.push((s, e));
    runs
}

/// First screen-x where `target` matches on row `ry`, scanning left → right every `step_x`.
fn strip_first_color_ltr(
    sub: &opencv::core::Mat,
    rw: u32,
    ry: i32,
    x0: i32,
    step_x: u32,
    detector: &Arc<dyn ColorDetector>,
    target: Color,
    tol: ColorTolerance,
) -> Option<i32> {
    let mut lx: u32 = 0;
    while lx < rw {
        let pt = Point {
            x: lx as i32,
            y: ry,
        };
        if detector.detect_pixel(sub, pt, target, tol) {
            return Some(x0 + lx as i32);
        }
        lx = lx.saturating_add(step_x);
    }
    None
}

/// First screen-x where `target` matches on row `ry`, scanning right → left every `step_x`.
fn strip_first_color_rtl(
    sub: &opencv::core::Mat,
    rw: u32,
    ry: i32,
    x0: i32,
    step_x: u32,
    detector: &Arc<dyn ColorDetector>,
    target: Color,
    tol: ColorTolerance,
) -> Option<i32> {
    if rw == 0 {
        return None;
    }
    let mut lx = (rw - 1) as i32;
    while lx >= 0 {
        let u = lx as u32;
        if u < rw {
            let pt = Point { x: lx, y: ry };
            if detector.detect_pixel(sub, pt, target, tol) {
                return Some(x0 + lx);
            }
        }
        lx -= step_x as i32;
    }
    None
}

fn normalize_result_index(index: i32, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    if index >= 0 {
        let idx = index as usize;
        (idx < len).then_some(idx)
    } else {
        let idx = len as i32 + index;
        (idx >= 0).then_some(idx as usize)
    }
}

fn iou(a: &MatchResult, b: &MatchResult) -> f32 {
    let ax1 = a.x as f32;
    let ay1 = a.y as f32;
    let ax2 = ax1 + a.width as f32;
    let ay2 = ay1 + a.height as f32;
    let bx1 = b.x as f32;
    let by1 = b.y as f32;
    let bx2 = bx1 + b.width as f32;
    let by2 = by1 + b.height as f32;

    let inter_x1 = ax1.max(bx1);
    let inter_y1 = ay1.max(by1);
    let inter_x2 = ax2.min(bx2);
    let inter_y2 = ay2.min(by2);
    let inter_w = (inter_x2 - inter_x1).max(0.0);
    let inter_h = (inter_y2 - inter_y1).max(0.0);
    let inter = inter_w * inter_h;
    if inter <= 0.0 {
        return 0.0;
    }
    let area_a = (ax2 - ax1).max(0.0) * (ay2 - ay1).max(0.0);
    let area_b = (bx2 - bx1).max(0.0) * (by2 - by1).max(0.0);
    let denom = area_a + area_b - inter;
    if denom <= 0.0 {
        0.0
    } else {
        inter / denom
    }
}

fn apply_nms(mut results: Vec<MatchResult>, nms_threshold: f32) -> Vec<MatchResult> {
    if results.is_empty() {
        return results;
    }
    results.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut keep: Vec<MatchResult> = Vec::with_capacity(results.len());
    'outer: for r in results {
        for k in &keep {
            if iou(&r, k) > nms_threshold {
                continue 'outer;
            }
        }
        keep.push(r);
    }
    keep
}

fn sort_matches(results: &mut [MatchResult], order: FindTemplateOrderBy) {
    match order {
        FindTemplateOrderBy::Score => {
            results.sort_by(|a, b| {
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        FindTemplateOrderBy::Horizontal => {
            results.sort_by_key(|m| (m.x, m.y));
        }
        FindTemplateOrderBy::Vertical => {
            results.sort_by_key(|m| (m.y, m.x));
        }
        FindTemplateOrderBy::Area => {
            results.sort_by(|a, b| {
                let aa = (a.width as u64) * (a.height as u64);
                let bb = (b.width as u64) * (b.height as u64);
                bb.cmp(&aa)
            });
        }
        FindTemplateOrderBy::Random => {
            results.sort_by_key(|m| {
                let mut v = (m.x as i64).wrapping_mul(1103515245)
                    ^ (m.y as i64).wrapping_mul(12345)
                    ^ ((m.width as i64) << 8)
                    ^ ((m.height as i64) << 16);
                v ^= v >> 13;
                v ^= v << 7;
                v
            });
        }
    }
}

/// Max absolute shift per axis when scanning `color_match_all` translations (inclusive).
const COLOR_MATCH_SHIFT_CAP: i32 = 128;

fn clamp_color_match_shift(max_dx: i32, max_dy: i32) -> (i32, i32) {
    (
        max_dx.clamp(0, COLOR_MATCH_SHIFT_CAP),
        max_dy.clamp(0, COLOR_MATCH_SHIFT_CAP),
    )
}

/// Evaluate all points under translation `(ox, oy)`. When `collect` is false, the returned vec is empty.
fn color_match_eval_offset(
    detector: &Arc<dyn ColorDetector>,
    mat: &opencv::core::Mat,
    points: &[ColorMatchPoint],
    point_tolerances: &[ColorTolerance],
    ox: i32,
    oy: i32,
    collect: bool,
) -> (bool, Vec<ColorMatchPointResult>) {
    debug_assert_eq!(points.len(), point_tolerances.len());
    let w = mat.cols();
    let h = mat.rows();
    let mut rows = Vec::new();
    let mut all_ok = true;

    for (p, &tol_mode) in points.iter().zip(point_tolerances.iter()) {
        let sx = p.x + ox;
        let sy = p.y + oy;
        let (tol_u8, rgba_tol) = match tol_mode {
            ColorTolerance::Euclidean(n) => (n, None),
            ColorTolerance::RgbaMaxDelta { r, g, b, a } => (0, Some(RgbaTolerance { r, g, b, a })),
        };

        let (actual_hex, row_ok, matched) = if sx < 0 || sy < 0 || sx >= w || sy >= h {
            ("#000000".to_string(), true, false)
        } else {
            let pt = Point { x: sx, y: sy };
            let actual_hex = detector
                .get_pixel_color(mat, pt)
                .map(|c| color_to_hex(&c))
                .unwrap_or_else(|| "#000000".to_string());
            match parse_hex_color(&p.color) {
                Some(target) => {
                    let m = detector.detect_pixel(mat, pt, target, tol_mode);
                    (actual_hex, true, m)
                }
                None => (actual_hex, false, false),
            }
        };

        if !row_ok || !matched {
            all_ok = false;
        }

        if collect {
            rows.push(ColorMatchPointResult {
                x: p.x,
                y: p.y,
                sample_x: sx,
                sample_y: sy,
                expected: p.color.clone(),
                actual: actual_hex,
                tolerance: tol_u8,
                rgba_tolerance: rgba_tol,
                matched: row_ok && matched,
            });
        }
    }

    (all_ok, rows)
}

#[async_trait]
impl ScriptContext for EngineScriptContext {
    // === State ===
    fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    fn request_cancel(&self) {
        self.cancel.cancel();
    }

    fn reset_cancel(&self) {
        self.cancel.reset();
    }

    fn get_config(&self) -> &serde_json::Value {
        &self.config
    }

    fn manifest_security_strict(&self) -> bool {
        self.manifest_security_strict.load(Ordering::Relaxed)
    }

    fn push_manifest_permission_scope(&self, declared: &[String], strict: bool) {
        let declared: HashSet<String> = declared.iter().map(|s| s.to_ascii_lowercase()).collect();
        self.manifest_perm_stack
            .lock()
            .expect("manifest_perm_stack")
            .push(ManifestPermScope { declared, strict });
    }

    fn pop_manifest_permission_scope(&self) {
        let mut g = self
            .manifest_perm_stack
            .lock()
            .expect("manifest_perm_stack");
        let _ = g.pop();
    }

    fn check_manifest_api_permission(&self, method: &str) -> Result<(), String> {
        let Some(key) = manifest_permission_key_for_ctx_method(method) else {
            return Ok(());
        };
        let scope = self
            .manifest_perm_stack
            .lock()
            .expect("manifest_perm_stack")
            .last()
            .cloned();
        let Some(scope) = scope else {
            return Ok(());
        };
        if scope.declared.contains(key) {
            return Ok(());
        }
        let msg = format!(
            "Manifest permission denied: ctx method '{}' requires '{}' (not declared in manifest)",
            method, key
        );
        if scope.strict {
            tracing::error!(target: "betternte", "{}", msg);
            self.log(LogLevel::Error, &msg);
            self.request_cancel();
            Err(msg)
        } else {
            // TODO: 暂时关闭权限 WARN 日志，后续再打开
            // tracing::warn!(target: "betternte", "{}", msg);
            // self.log(LogLevel::Warn, &msg);
            Ok(())
        }
    }

    fn progress(&self, current: u32, total: u32) {
        info!(current, total, "Script progress");
    }

    fn get_fps(&self) -> f64 {
        self.get_fps_value()
    }

    fn get_frame_number(&self) -> u64 {
        self.frame_number.load(Ordering::Relaxed)
    }

    fn set_template_dir(&self, dir: PathBuf) {
        *self.template_dir.lock().unwrap() = dir;
        self.template_file_cache.clear();
    }

    fn get_template_dir(&self) -> Option<PathBuf> {
        Some(self.template_dir.lock().unwrap().clone())
    }

    fn set_design_resolution(&self, resolution: Option<(u32, u32)>) {
        *self.design_resolution.lock().unwrap() = resolution;
    }

    fn get_design_resolution(&self) -> Option<(u32, u32)> {
        *self.design_resolution.lock().unwrap()
    }

    fn get_scale_factors(&self) -> Option<(f64, f64)> {
        self.current_scale_factors()
    }

    fn get_frame_size(&self) -> Option<(u32, u32)> {
        let frame = self.shared_frame.try_read().ok()?;
        let snap = frame.as_ref()?;
        Some((snap.frame.width, snap.frame.height))
    }

    // === Capture ===
    async fn capture(&self, force: bool) -> Result<CaptureFrame> {
        if force {
            self.do_capture().await
        } else {
            self.get_cached_frame().await
        }
    }

    async fn capture_region(&self, region: &Region, force: bool) -> Result<CaptureFrame> {
        let sr = self.reverse_scale_region(region);
        let full = if force {
            self.do_capture().await?
        } else {
            self.get_cached_frame().await?
        };

        let x = sr.x.max(0) as u32;
        let y = sr.y.max(0) as u32;
        let w = sr.width.min(full.width.saturating_sub(x));
        let h = sr.height.min(full.height.saturating_sub(y));

        let mut cropped = Vec::with_capacity((w * h * 4) as usize);
        for row in 0..h {
            let src_start = ((y + row) * full.width + x) as usize * 4;
            let src_end = src_start + w as usize * 4;
            if src_end <= full.data.len() {
                cropped.extend_from_slice(&full.data[src_start..src_end]);
            }
        }

        Ok(CaptureFrame {
            width: w,
            height: h,
            data: Arc::new(cropped),
        })
    }

    async fn save_screenshot(&self, force: bool) -> Result<String> {
        let frame = self.capture(force).await?;
        // BGRA → RGBA
        let mut rgba = (*frame.data).clone();
        for chunk in rgba.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }
        let img = image::RgbaImage::from_raw(frame.width, frame.height, rgba)
            .ok_or_else(|| anyhow::anyhow!("Failed to create image from frame data"))?;
        let dyn_img = DynamicImage::ImageRgba8(img);

        let user_profile = std::env::var("USERPROFILE")
            .map_err(|_| anyhow::anyhow!("Cannot determine user home directory"))?;
        let save_dir = PathBuf::from(user_profile).join("Pictures").join("BetterNTE");
        tokio::fs::create_dir_all(&save_dir).await?;

        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("screenshot_{}.png", ts);
        let full = save_dir.join(&filename);
        dyn_img.save(&full)?;
        Ok(full.to_string_lossy().to_string())
    }

    // === Recognition ===
    async fn find_template(
        &self,
        name: &str,
        opts: Option<FindTemplateOpts>,
    ) -> Result<Option<MatchResult>> {
        let result_index = opts.as_ref().and_then(|o| o.result_index).unwrap_or(0);
        let matches = self.find_templates(name, opts).await?;
        let Some(idx) = normalize_result_index(result_index, matches.len()) else {
            return Ok(None);
        };
        Ok(matches.get(idx).cloned())
    }

    async fn find_templates(
        &self,
        name: &str,
        opts: Option<FindTemplateOpts>,
    ) -> Result<Vec<MatchResult>> {
        let (_frame, frame_mat) = self.get_decoded_mat_for_vision().await?;
        let (fw, fh) = (frame_mat.cols() as u32, frame_mat.rows() as u32);
        self.find_templates_on_decoded_frame(
            fw,
            fh,
            &frame_mat,
            name,
            opts.as_ref(),
        )
        .await
    }

    async fn find_template_batch(
        &self,
        entries: &[FindTemplateBatchEntry],
    ) -> Result<Vec<Option<MatchResult>>> {
        if entries.is_empty() {
            return Ok(vec![]);
        }
        let (_frame, frame_mat) = self.get_decoded_mat_for_vision().await?;
        let (fw, fh) = (frame_mat.cols() as u32, frame_mat.rows() as u32);
        let mut out = Vec::with_capacity(entries.len());
        for e in entries {
            if e.name.is_empty() {
                out.push(None);
                continue;
            }
            let matches = self
                .find_templates_on_decoded_frame(fw, fh, &frame_mat, &e.name, Some(&e.opts))
                .await?;
            let ri = e.opts.result_index.unwrap_or(0);
            let idx = normalize_result_index(ri, matches.len());
            out.push(idx.and_then(|i| matches.get(i).cloned()));
        }
        Ok(out)
    }

    async fn ocr(&self, region: &Region, text_color: Option<&str>, text_color_tolerance: u8) -> Result<String> {
        let (frame, mut dyn_img) = self.get_decoded_frame_for_vision().await?;

        if let Some(color_str) = text_color {
            if let Some(target) = parse_color_str(color_str) {
                dyn_img = apply_text_color_filter(&dyn_img, target, text_color_tolerance);
            }
        }
        let engine_guard = match self.ocr_engine.as_ref() {
            Some(e) => e,
            None => {
                return Err(anyhow::anyhow!("OCR engine not injected (missing builder setup)"));
            }
        };

        let frame_key = Self::frame_key(&frame);
        let (vision_w, vision_h) = dyn_img.dimensions();

        let current_design_res = *self.design_resolution.lock().unwrap();
        {
            let cache = self.ocr_batch_cache.lock().await;
            if let Some(snap) = cache.as_ref() {
                if snap.key == frame_key && snap.design_res == current_design_res {
                    if perf_enabled() {
                        tracing::trace!(
                            target: "betternte_perf",
                            event = "ocr_batch_hit",
                            frame_key = frame_key,
                            "ocr_batch_hit"
                        );
                    }
                    let text = snap
                        .results
                        .iter()
                        .filter(|r| {
                            let rx2 = r.region.x + r.region.width as i32;
                            let ry2 = r.region.y + r.region.height as i32;
                            let qx2 = region.x + region.width as i32;
                            let qy2 = region.y + region.height as i32;
                            r.region.x < qx2 && rx2 > region.x && r.region.y < qy2 && ry2 > region.y
                        })
                        .map(|r| r.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    return Ok(text);
                }
            }
        }

        let mut engine = engine_guard.lock().await;
        if !engine.is_ready() {
            let ocr_cfg = self.ocr_config.lock().unwrap().clone();
            engine.init(&ocr_cfg).await.map_err(|e| {
                anyhow::anyhow!("OCR engine init failed (model_path={}): {}", ocr_cfg.model_path, e)
            })?;
        }

        let eligible = Self::ocr_batch_eligible(region, vision_w, vision_h);
        if eligible {
            let started = Instant::now();
            match engine.recognize(&dyn_img).await {
                Ok(regions) => {
                    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
                    if perf_enabled() {
                        tracing::trace!(
                            target: "betternte_perf",
                            event = "ocr_batch_hit",
                            frame_key = frame_key,
                            ms = elapsed_ms,
                            "ocr_batch_hit"
                        );
                    }
                    let mapped: Vec<OcrResult> = regions
                        .into_iter()
                        .map(|r| OcrResult {
                            text: r.text,
                            region: Region {
                                x: r.bbox.x as i32,
                                y: r.bbox.y as i32,
                                width: r.bbox.width as u32,
                                height: r.bbox.height as u32,
                            },
                            confidence: r.confidence as f64,
                        })
                        .collect();
                    let text = mapped
                        .iter()
                        .filter(|r| {
                            let rx2 = r.region.x + r.region.width as i32;
                            let ry2 = r.region.y + r.region.height as i32;
                            let qx2 = region.x + region.width as i32;
                            let qy2 = region.y + region.height as i32;
                            r.region.x < qx2 && rx2 > region.x && r.region.y < qy2 && ry2 > region.y
                        })
                        .map(|r| r.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    *self.ocr_batch_cache.lock().await = Some(OcrBatchCacheSnapshot {
                        key: frame_key,
                        design_res: current_design_res,
                        results: mapped,
                    });
                    return Ok(text);
                }
                Err(e) => {
                    warn!(error = %e, "ocr: batch recognition failed, degrade to single");
                    if perf_enabled() {
                        tracing::trace!(
                            target: "betternte_perf",
                            event = "degraded_to_single",
                            method = "ocr",
                            frame_key = frame_key,
                            "degraded_to_single"
                        );
                    }
                }
            }
        } else if perf_enabled() {
            tracing::trace!(
                target: "betternte_perf",
                event = "degraded_to_single",
                method = "ocr",
                frame_key = frame_key,
                "degraded_to_single"
            );
        }

        // Non-batch fallback: crop the scaled frame by region and run OCR on it.
        let x = region.x.max(0) as u32;
        let y = region.y.max(0) as u32;
        let w = region.width.min(vision_w.saturating_sub(x));
        let h = region.height.min(vision_h.saturating_sub(y));
        if w == 0 || h == 0 {
            return Ok(String::new());
        }
        let cropped = dyn_img.crop_imm(x, y, w, h);
        match engine.recognize(&cropped).await {
            Ok(regions) => {
                let text = regions
                    .iter()
                    .map(|r| r.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                Ok(text)
            }
            Err(e) => Err(anyhow::anyhow!("OCR recognition failed: {}", e)),
        }
    }

    async fn ocr_all(&self) -> Result<Vec<OcrResult>> {
        let (frame, dyn_img) = self.get_decoded_frame_for_vision().await?;
        let engine_guard = match self.ocr_engine.as_ref() {
            Some(e) => e,
            None => {
                return Err(anyhow::anyhow!("OCR engine not injected (missing builder setup)"));
            }
        };

        let mut engine = engine_guard.lock().await;
        if !engine.is_ready() {
            let ocr_cfg = self.ocr_config.lock().unwrap().clone();
            engine.init(&ocr_cfg).await.map_err(|e| {
                anyhow::anyhow!("OCR engine init failed (model_path={}): {}", ocr_cfg.model_path, e)
            })?;
        }

        match engine.recognize(&dyn_img).await {
            Ok(regions) => {
                let mapped: Vec<OcrResult> = regions
                    .into_iter()
                    .map(|r| OcrResult {
                        text: r.text,
                        region: Region {
                            x: r.bbox.x as i32,
                            y: r.bbox.y as i32,
                            width: r.bbox.width as u32,
                            height: r.bbox.height as u32,
                        },
                        confidence: r.confidence as f64,
                    })
                    .collect();
                let dr = *self.design_resolution.lock().unwrap();
                *self.ocr_batch_cache.lock().await = Some(OcrBatchCacheSnapshot {
                    key: Self::frame_key(&frame),
                    design_res: dr,
                    results: mapped.clone(),
                });
                Ok(mapped)
            }
            Err(e) => {
                Err(anyhow::anyhow!("OCR recognition failed: {}", e))
            }
        }
    }

    async fn get_color(&self, x: i32, y: i32) -> Result<String> {
        let frame = self.get_cached_core_frame().await?;
        let w = frame.width as i32;
        let h = frame.height as i32;
        if x < 0 || y < 0 || x >= w || y >= h {
            return Ok("#000000".into());
        }
        let bpp = frame.format.bytes_per_pixel() as usize;
        let stride = frame.width as usize * bpp;
        let off = y as usize * stride + x as usize * bpp;
        let data = &*frame.data;
        if off + bpp > data.len() {
            return Ok("#000000".into());
        }
        let (r, g, b) = match frame.format {
            betternte_core::image::PixelFormat::Bgra => (data[off + 2], data[off + 1], data[off]),
            betternte_core::image::PixelFormat::Rgba => (data[off], data[off + 1], data[off + 2]),
            betternte_core::image::PixelFormat::Bgr => (data[off + 2], data[off + 1], data[off]),
            betternte_core::image::PixelFormat::Rgb => (data[off], data[off + 1], data[off + 2]),
            betternte_core::image::PixelFormat::Gray => (data[off], data[off], data[off]),
        };
        Ok(format!("#{:02x}{:02x}{:02x}", r, g, b))
    }

    async fn get_colors(&self, points: &[(i32, i32)]) -> Result<Vec<String>> {
        let frame = self.get_cached_core_frame().await?;
        let w = frame.width as i32;
        let h = frame.height as i32;
        let data = &*frame.data;
        let bpp = frame.format.bytes_per_pixel() as usize;
        let stride = frame.width as usize * bpp;

        Ok(points
            .iter()
            .map(|&(x, y)| {
                if x < 0 || y < 0 || x >= w || y >= h {
                    return "#000000".into();
                }
                let off = y as usize * stride + x as usize * bpp;
                if off + bpp > data.len() {
                    return "#000000".into();
                }
                let (r, g, b) = match frame.format {
                    betternte_core::image::PixelFormat::Bgra => (data[off + 2], data[off + 1], data[off]),
                    betternte_core::image::PixelFormat::Rgba => (data[off], data[off + 1], data[off + 2]),
                    betternte_core::image::PixelFormat::Bgr => (data[off + 2], data[off + 1], data[off]),
                    betternte_core::image::PixelFormat::Rgb => (data[off], data[off + 1], data[off + 2]),
                    betternte_core::image::PixelFormat::Gray => (data[off], data[off], data[off]),
                };
                format!("#{:02x}{:02x}{:02x}", r, g, b)
            })
            .collect())
    }

    async fn color_match(&self, x: i32, y: i32, color: &str, tolerance: u8) -> Result<bool> {
        let detector = match self.color_detector.as_ref() {
            Some(d) => d,
            None => {
                warn!("color_match: no color detector injected");
                return Ok(false);
            }
        };

        let target = match parse_hex_color(color) {
            Some(c) => c,
            None => return Ok(false),
        };

        let (_, mat) = self.get_decoded_mat_for_vision().await?;

        Ok(detector.detect_pixel(
            &mat,
            Point { x, y },
            target,
            ColorTolerance::from(tolerance),
        ))
    }

    async fn color_match_all(
        &self,
        points: &[ColorMatchPoint],
        opts: &ColorMatchAllOpts,
    ) -> Result<ColorMatchAllResult> {
        let point_tolerances: Vec<ColorTolerance> = points
            .iter()
            .map(|p| color_tolerance_for_match_point(p, opts))
            .collect();
        let debug = opts.debug;
        let shift_max = opts.shift_max.as_ref().map(|s| (s.max_dx, s.max_dy));

        if points.is_empty() {
            return Ok(ColorMatchAllResult {
                all_match: false,
                points: debug.then_some(vec![]),
                matched_shift: None,
            });
        }
        let detector = match self.color_detector.as_ref() {
            Some(d) => d,
            None => {
                warn!("color_match_all: no color detector injected");
                return Ok(ColorMatchAllResult {
                    all_match: false,
                    points: debug.then_some(vec![]),
                    matched_shift: None,
                });
            }
        };

        let (_, mat) = self.get_decoded_mat_for_vision().await?;

        let mut offsets: Vec<(i32, i32)> = vec![(0, 0)];
        if let Some((mx, my)) = shift_max {
            let (mx, my) = clamp_color_match_shift(mx, my);
            for dy in -my..=my {
                for dx in -mx..=mx {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    offsets.push((dx, dy));
                }
            }
        }

        for (ox, oy) in offsets {
            let collect = debug;
            let (pass, rows) = color_match_eval_offset(
                detector,
                &mat,
                points,
                &point_tolerances,
                ox,
                oy,
                collect,
            );
            if pass {
                return Ok(ColorMatchAllResult {
                    all_match: true,
                    points: if debug { Some(rows) } else { None },
                    matched_shift: Some(ColorMatchShift { x: ox, y: oy }),
                });
            }
        }

        let baseline = if debug {
            color_match_eval_offset(detector, &mat, points, &point_tolerances, 0, 0, true).1
        } else {
            vec![]
        };

        Ok(ColorMatchAllResult {
            all_match: false,
            points: if debug { Some(baseline) } else { None },
            matched_shift: None,
        })
    }

    async fn scan_slider_strip(&self, opts: &Value) -> Result<Value> {
        let region_val = opts
            .get("region")
            .ok_or_else(|| anyhow::anyhow!("scan_slider_strip: missing region"))?;
        let region: Region = serde_json::from_value(region_val.clone())
            .map_err(|e| anyhow::anyhow!("scan_slider_strip: bad region: {e}"))?;
        let bar_color = opts
            .get("barColor")
            .or_else(|| opts.get("bar_color"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("scan_slider_strip: missing barColor"))?;
        let player_color = opts
            .get("playerColor")
            .or_else(|| opts.get("player_color"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("scan_slider_strip: missing playerColor"))?;
        let bar_tol = opts
            .get("barTolerance")
            .and_then(|v| v.as_u64())
            .unwrap_or(28) as u8;
        let player_tol = opts
            .get("playerTolerance")
            .and_then(|v| v.as_u64())
            .unwrap_or(24) as u8;
        let step_x = opts
            .get("stepX")
            .and_then(|v| v.as_u64())
            .unwrap_or(2)
            .max(1) as u32;
        let min_bar = opts
            .get("minBarRunPx")
            .and_then(|v| v.as_i64())
            .unwrap_or(18) as i32;
        let min_player = opts
            .get("minPlayerRunPx")
            .and_then(|v| v.as_i64())
            .unwrap_or(6) as i32;

        let detector = match self.color_detector.as_ref() {
            Some(d) => d,
            None => {
                warn!("scan_slider_strip: no color detector");
                return Ok(json!({
                    "ok": false,
                    "reason": "no_color_detector"
                }));
            }
        };

        let bar_target = match parse_hex_color(bar_color) {
            Some(c) => c,
            None => {
                return Ok(json!({ "ok": false, "reason": "bad_bar_color" }));
            }
        };
        let player_target = match parse_hex_color(player_color) {
            Some(c) => c,
            None => {
                return Ok(json!({ "ok": false, "reason": "bad_player_color" }));
            }
        };

        let (_, mat) = self.get_decoded_mat_for_vision().await?;
        let iw = mat.cols() as u32;
        let ih = mat.rows() as u32;
        let x0 = region.x.max(0).min(iw.saturating_sub(1) as i32);
        let y0 = region.y.max(0).min(ih.saturating_sub(1) as i32);
        let x1 = (region.x + region.width as i32).min(iw as i32).max(x0 + 1);
        let y1 = (region.y + region.height as i32).min(ih as i32).max(y0 + 1);
        let rw = (x1 - x0).max(1) as u32;
        let rh = (y1 - y0).max(1) as u32;
        let rect = opencv::core::Rect::new(x0, y0, rw as i32, rh as i32);
        let sub = mat.roi(rect)
            .map_err(|e| anyhow::anyhow!("Mat::roi error: {}", e))?
            .try_clone()?;
        let sh = rh;
        let ry = if let Some(v) = opts.get("rowOffset").and_then(|v| v.as_i64()) {
            (v as i32).clamp(0, sh.saturating_sub(1) as i32)
        } else {
            (sh / 2).min(sh.saturating_sub(1)) as i32
        };

        let bar_tol_ct = ColorTolerance::from(bar_tol);
        let player_tol_ct = ColorTolerance::from(player_tol);
        let gap_max = (step_x as i32 * 3).max(8);

        let mut bar_xs: Vec<i32> = Vec::new();
        let mut player_xs: Vec<i32> = Vec::new();
        let mut lx: u32 = 0;
        while lx < rw {
            let pt = Point {
                x: lx as i32,
                y: ry,
            };
            if detector.detect_pixel(&sub, pt, bar_target, bar_tol_ct) {
                bar_xs.push(x0 + lx as i32);
            }
            if detector.detect_pixel(&sub, pt, player_target, player_tol_ct) {
                player_xs.push(x0 + lx as i32);
            }
            lx = lx.saturating_add(step_x);
        }

        let bar_run = color_runs_from_x_samples(bar_xs, gap_max)
            .into_iter()
            .filter(|(a, b)| *b - *a >= min_bar)
            .max_by_key(|(a, b)| *b - *a);
        let Some((bl, br)) = bar_run else {
            return Ok(json!({
                "ok": false,
                "reason": "no_bar",
                "row_screen_y": y0 + ry,
                "step_x": step_x,
            }));
        };
        let bar_center = (bl + br) / 2;

        let player_run = color_runs_from_x_samples(player_xs, gap_max)
            .into_iter()
            .filter(|(a, b)| *b - *a >= min_player)
            .max_by_key(|(a, b)| *b - *a);
        let Some((pl, pr)) = player_run else {
            return Ok(json!({
                "ok": false,
                "reason": "no_player",
                "bar_left": bl,
                "bar_right": br,
                "bar_center": bar_center,
                "row_screen_y": y0 + ry,
                "step_x": step_x,
            }));
        };
        let player_center = (pl + pr) / 2;

        Ok(json!({
            "ok": true,
            "bar_left": bl,
            "bar_right": br,
            "bar_center": bar_center,
            "player_left": pl,
            "player_right": pr,
            "player_center": player_center,
            "row_screen_y": y0 + ry,
            "step_x": step_x,
        }))
    }

    async fn scan_strip_edges(&self, opts: &Value) -> Result<Value> {
        let region_val = opts
            .get("region")
            .ok_or_else(|| anyhow::anyhow!("scan_strip_edges: missing region"))?;
        let region: Region = serde_json::from_value(region_val.clone())
            .map_err(|e| anyhow::anyhow!("scan_strip_edges: bad region: {e}"))?;
        let bar_color = opts
            .get("barColor")
            .or_else(|| opts.get("bar_color"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("scan_strip_edges: missing barColor"))?;
        let player_color = opts
            .get("playerColor")
            .or_else(|| opts.get("player_color"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("scan_strip_edges: missing playerColor"))?;
        let bar_tol = opts
            .get("barTolerance")
            .and_then(|v| v.as_u64())
            .unwrap_or(28) as u8;
        let player_tol = opts
            .get("playerTolerance")
            .and_then(|v| v.as_u64())
            .unwrap_or(24) as u8;
        let step_x = opts
            .get("stepX")
            .and_then(|v| v.as_u64())
            .unwrap_or(2)
            .max(1) as u32;

        let detector = match self.color_detector.as_ref() {
            Some(d) => d,
            None => {
                warn!("scan_strip_edges: no color detector");
                return Ok(json!({
                    "ok": false,
                    "reason": "no_color_detector"
                }));
            }
        };

        let bar_target = match parse_hex_color(bar_color) {
            Some(c) => c,
            None => {
                return Ok(json!({ "ok": false, "reason": "bad_bar_color" }));
            }
        };
        let player_target = match parse_hex_color(player_color) {
            Some(c) => c,
            None => {
                return Ok(json!({ "ok": false, "reason": "bad_player_color" }));
            }
        };

        let (_, mat) = self.get_decoded_mat_for_vision().await?;
        let iw = mat.cols() as u32;
        let ih = mat.rows() as u32;
        let x0 = region.x.max(0).min(iw.saturating_sub(1) as i32);
        let y0 = region.y.max(0).min(ih.saturating_sub(1) as i32);
        let x1 = (region.x + region.width as i32)
            .min(iw as i32)
            .max(x0 + 1);
        let y1 = (region.y + region.height as i32)
            .min(ih as i32)
            .max(y0 + 1);
        let rw = (x1 - x0).max(1) as u32;
        let rh = (y1 - y0).max(1) as u32;
        let rect = opencv::core::Rect::new(x0, y0, rw as i32, rh as i32);
        let sub = mat.roi(rect)
            .map_err(|e| anyhow::anyhow!("Mat::roi error: {}", e))?
            .try_clone()?;
        let sh = rh;
        let ry = if let Some(v) = opts.get("rowOffset").and_then(|v| v.as_i64()) {
            (v as i32).clamp(0, sh.saturating_sub(1) as i32)
        } else {
            (sh / 2).min(sh.saturating_sub(1)) as i32
        };

        let bar_tol_ct = ColorTolerance::from(bar_tol);
        let player_tol_ct = ColorTolerance::from(player_tol);

        let bl = strip_first_color_ltr(
            &sub, rw, ry, x0, step_x, detector, bar_target, bar_tol_ct,
        );
        let br = strip_first_color_rtl(
            &sub, rw, ry, x0, step_x, detector, bar_target, bar_tol_ct,
        );
        let Some(bar_left) = bl else {
            return Ok(json!({
                "ok": false,
                "reason": "no_bar",
                "row_screen_y": y0 + ry,
                "step_x": step_x,
                "mode": "edges",
            }));
        };
        let Some(bar_right) = br else {
            return Ok(json!({
                "ok": false,
                "reason": "no_bar",
                "row_screen_y": y0 + ry,
                "step_x": step_x,
                "mode": "edges",
                "bar_left": bar_left,
            }));
        };
        if bar_left >= bar_right {
            return Ok(json!({
                "ok": false,
                "reason": "no_bar",
                "detail": "bar_edges_inverted",
                "bar_left": bar_left,
                "bar_right": bar_right,
                "row_screen_y": y0 + ry,
                "step_x": step_x,
                "mode": "edges",
            }));
        }

        let pl = strip_first_color_ltr(
            &sub,
            rw,
            ry,
            x0,
            step_x,
            detector,
            player_target,
            player_tol_ct,
        );
        let pr = strip_first_color_rtl(
            &sub,
            rw,
            ry,
            x0,
            step_x,
            detector,
            player_target,
            player_tol_ct,
        );

        let Some(player_left) = pl else {
            return Ok(json!({
                "ok": false,
                "reason": "no_player",
                "bar_left": bar_left,
                "bar_right": bar_right,
                "bar_center": (bar_left + bar_right) / 2,
                "row_screen_y": y0 + ry,
                "step_x": step_x,
                "mode": "edges",
            }));
        };
        let Some(player_right) = pr else {
            return Ok(json!({
                "ok": false,
                "reason": "no_player",
                "bar_left": bar_left,
                "bar_right": bar_right,
                "bar_center": (bar_left + bar_right) / 2,
                "player_left": player_left,
                "row_screen_y": y0 + ry,
                "step_x": step_x,
                "mode": "edges",
            }));
        };

        let bar_center = (bar_left + bar_right) / 2;
        let player_center = (player_left + player_right) / 2;

        Ok(json!({
            "ok": true,
            "bar_left": bar_left,
            "bar_right": bar_right,
            "bar_center": bar_center,
            "player_left": player_left,
            "player_right": player_right,
            "player_center": player_center,
            "row_screen_y": y0 + ry,
            "step_x": step_x,
            "mode": "edges",
        }))
    }

    async fn count_color(&self, color: &str, opts: Option<&Value>) -> Result<u32> {
        let target = match parse_hex_color(color) {
            Some(c) => c,
            None => return Ok(0),
        };

        let tolerance = opts
            .and_then(|o| o.get("tolerance"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u8;
        let tol = ColorTolerance::from(tolerance);

        let (_, mat) = self.get_decoded_mat_for_vision().await?;
        let iw = mat.cols() as u32;
        let ih = mat.rows() as u32;

        let (x_start, y_start, x_end, y_end) = if let Some(roi) = opts.and_then(|o| o.get("roi"))
        {
            let rx = roi.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let ry = roi.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let rw = roi.get("width").and_then(|v| v.as_u64()).unwrap_or(iw as u64) as u32;
            let rh = roi.get("height").and_then(|v| v.as_u64()).unwrap_or(ih as u64) as u32;
            (
                rx.max(0).min(iw.saturating_sub(1) as i32) as u32,
                ry.max(0).min(ih.saturating_sub(1) as i32) as u32,
                (rx + rw as i32).min(iw as i32).max(rx) as u32,
                (ry + rh as i32).min(ih as i32).max(ry) as u32,
            )
        } else {
            (0, 0, iw, ih)
        };

        if x_start >= x_end || y_start >= y_end {
            return Ok(0);
        }

        let mut count = 0u32;
        for y in y_start..y_end {
            for x in x_start..x_end {
                if let Ok(pixel) = mat.at_2d::<opencv::core::Vec4b>(y as i32, x as i32) {
                    let c = Color::rgb(pixel[2], pixel[1], pixel[0]); // BGRA → RGB
                    if tol.matches(c, target) {
                        count += 1;
                    }
                }
            }
        }
        Ok(count)
    }

    // === Input — delegates to InputController when available ===
    async fn click(&self, x: i32, y: i32) -> Result<()> {
        let (sx, sy) = self.reverse_scale_point(x, y);
        let guard = self.input_controller.lock().await;
        let result = if let Some(ref ctrl) = *guard {
            ctrl.click(sx, sy).await
        } else {
            info!(sx, sy, "Script click (no controller)");
            Ok(())
        };
        drop(guard);

        let mut m = Map::new();
        m.insert("x".into(), json!(x));
        m.insert("y".into(), json!(y));
        self.replay_record_script_input_if_recording("click", m, &result)
            .await;
        result
    }

    async fn double_click(&self, x: i32, y: i32) -> Result<()> {
        let (sx, sy) = self.reverse_scale_point(x, y);
        let guard = self.input_controller.lock().await;
        let result = if let Some(ref ctrl) = *guard {
            ctrl.double_click(sx, sy).await
        } else {
            info!(sx, sy, "Script double_click (no controller)");
            Ok(())
        };
        drop(guard);

        let mut m = Map::new();
        m.insert("x".into(), json!(x));
        m.insert("y".into(), json!(y));
        self.replay_record_script_input_if_recording("double_click", m, &result)
            .await;
        result
    }

    async fn right_click(&self, x: i32, y: i32) -> Result<()> {
        let (sx, sy) = self.reverse_scale_point(x, y);
        let guard = self.input_controller.lock().await;
        let result = if let Some(ref ctrl) = *guard {
            ctrl.right_click(sx, sy).await
        } else {
            info!(sx, sy, "Script right_click (no controller)");
            Ok(())
        };
        drop(guard);

        let mut m = Map::new();
        m.insert("x".into(), json!(x));
        m.insert("y".into(), json!(y));
        self.replay_record_script_input_if_recording("right_click", m, &result)
            .await;
        result
    }

    async fn mouse_move(&self, x: i32, y: i32) -> Result<()> {
        let (sx, sy) = self.reverse_scale_point(x, y);
        let guard = self.input_controller.lock().await;
        let result = if let Some(ref ctrl) = *guard {
            ctrl.mouse_move(sx, sy).await
        } else {
            info!(sx, sy, "Script mouse_move (no controller)");
            Ok(())
        };
        drop(guard);

        let mut m = Map::new();
        m.insert("x".into(), json!(x));
        m.insert("y".into(), json!(y));
        self.replay_record_script_input_if_recording("mouse_move", m, &result)
            .await;
        result
    }

    async fn mouse_down(&self, button: &str) -> Result<()> {
        let guard = self.input_controller.lock().await;
        let result = if let Some(ref ctrl) = *guard {
            if let Some(btn) = Self::parse_mouse_button_name(button) {
                ctrl.mouse_down(btn).await
            } else {
                Err(anyhow::anyhow!(
                    "unknown mouse button for mouse_down: {button}"
                ))
            }
        } else {
            info!(button, "Script mouse_down (no controller)");
            Ok(())
        };
        drop(guard);

        let mut m = Map::new();
        m.insert("button".into(), json!(button));
        self.replay_record_script_input_if_recording("mouse_down", m, &result)
            .await;
        result
    }

    async fn mouse_up(&self, button: &str) -> Result<()> {
        let guard = self.input_controller.lock().await;
        let result = if let Some(ref ctrl) = *guard {
            if let Some(btn) = Self::parse_mouse_button_name(button) {
                ctrl.mouse_up(btn).await
            } else {
                Err(anyhow::anyhow!(
                    "unknown mouse button for mouse_up: {button}"
                ))
            }
        } else {
            info!(button, "Script mouse_up (no controller)");
            Ok(())
        };
        drop(guard);

        let mut m = Map::new();
        m.insert("button".into(), json!(button));
        self.replay_record_script_input_if_recording("mouse_up", m, &result)
            .await;
        result
    }

    async fn scroll(&self, delta: i32) -> Result<()> {
        let guard = self.input_controller.lock().await;
        let result = if let Some(ref ctrl) = *guard {
            ctrl.mouse_scroll(delta).await
        } else {
            info!(delta, "Script scroll (no controller)");
            Ok(())
        };
        drop(guard);

        let mut m = Map::new();
        m.insert("delta".into(), json!(delta));
        self.replay_record_script_input_if_recording("scroll", m, &result)
            .await;
        result
    }

    async fn swipe(&self, x1: i32, y1: i32, x2: i32, y2: i32, duration_ms: u32) -> Result<()> {
        let (sx1, sy1) = self.reverse_scale_point(x1, y1);
        let (sx2, sy2) = self.reverse_scale_point(x2, y2);
        let guard = self.input_controller.lock().await;
        let result = if let Some(ref ctrl) = *guard {
            ctrl.swipe(sx1, sy1, sx2, sy2, duration_ms).await
        } else {
            info!(sx1, sy1, sx2, sy2, duration_ms, "Script swipe (no controller)");
            Ok(())
        };
        drop(guard);

        let mut m = Map::new();
        m.insert("x1".into(), json!(x1));
        m.insert("y1".into(), json!(y1));
        m.insert("x2".into(), json!(x2));
        m.insert("y2".into(), json!(y2));
        m.insert("duration_ms".into(), json!(duration_ms));
        self.replay_record_script_input_if_recording("swipe", m, &result)
            .await;
        result
    }

    async fn key_down(&self, key: &str) -> Result<()> {
        let guard = self.input_controller.lock().await;
        let result = if let Some(ref ctrl) = *guard {
            // Resolve via the controller so user-defined KeyMapper bindings
            // (e.g. "attack" -> Key::A) take effect.
            if let Some(k) = ctrl.parse_key(key) {
                ctrl.key_press(k).await
            } else {
                Err(anyhow::anyhow!("unknown key for key_down: {key}"))
            }
        } else {
            info!(key, "Script key_down (no controller)");
            Ok(())
        };
        drop(guard);

        let mut m = Map::new();
        m.insert("key".into(), json!(key));
        self.replay_record_script_input_if_recording("key_down", m, &result)
            .await;
        result
    }

    async fn key_up(&self, key: &str) -> Result<()> {
        let guard = self.input_controller.lock().await;
        let result = if let Some(ref ctrl) = *guard {
            if let Some(k) = ctrl.parse_key(key) {
                ctrl.key_release(k).await
            } else {
                Err(anyhow::anyhow!("unknown key for key_up: {key}"))
            }
        } else {
            info!(key, "Script key_up (no controller)");
            Ok(())
        };
        drop(guard);

        let mut m = Map::new();
        m.insert("key".into(), json!(key));
        self.replay_record_script_input_if_recording("key_up", m, &result)
            .await;
        result
    }

    async fn key_press(&self, key: &str, duration_ms: Option<u32>) -> Result<()> {
        let guard = self.input_controller.lock().await;
        let bound_hwnd = *self.hwnd.lock().unwrap();
        #[cfg(windows)]
        let foreground_hwnd = {
            use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
            unsafe { GetForegroundWindow().0 as usize as u64 }
        };
        #[cfg(not(windows))]
        let foreground_hwnd = 0u64;
        let parsed = guard.as_ref().and_then(|c| c.parse_key(key));

        let result = if let Some(ref ctrl) = *guard {
            if let Some(k) = parsed {
                if ctrl.mode() == betternte_core::input::InputMode::Foreground {
                    if let Some(target_hwnd) = bound_hwnd {
                        if target_hwnd != foreground_hwnd {
                            #[cfg(windows)]
                            {
                                use windows::Win32::Foundation::HWND;
                                use windows::Win32::UI::WindowsAndMessaging::{
                                    IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
                                };
                                unsafe {
                                    let hwnd = HWND(target_hwnd as *mut _);
                                    if IsIconic(hwnd).into() {
                                        let _ = ShowWindow(hwnd, SW_RESTORE);
                                    }
                                    let _ = SetForegroundWindow(hwnd);
                                }
                            }
                        }
                    }
                }

                let r = ctrl.key_tap(k, duration_ms).await;
                if let Err(ref e) = r {
                    warn!(key, error = %e, "key_press failed");
                }
                r
            } else {
                warn!(key, "Unknown key for key_press");
                Ok(())
            }
        } else {
            info!(key, "Script key_press (no controller)");
            Ok(())
        };
        drop(guard);

        let mut m = Map::new();
        m.insert("key".into(), json!(key));
        m.insert("duration_ms".into(), json!(duration_ms));
        self.replay_record_script_input_if_recording("key_press", m, &result)
            .await;
        result
    }

    async fn key_combo(&self, keys: &[String]) -> Result<()> {
        let guard = self.input_controller.lock().await;
        let result = if let Some(ref ctrl) = *guard {
            let parsed: Option<Vec<betternte_core::Key>> =
                keys.iter().map(|k| ctrl.parse_key(k)).collect();
            if let Some(v) = parsed {
                ctrl.key_combo(&v).await
            } else {
                Err(anyhow::anyhow!("unknown key in key_combo: {:?}", keys))
            }
        } else {
            info!(?keys, "Script key_combo (no controller)");
            Ok(())
        };
        drop(guard);

        let mut m = Map::new();
        m.insert("keys".into(), json!(keys));
        self.replay_record_script_input_if_recording("key_combo", m, &result)
            .await;
        result
    }

    async fn type_text(&self, text: &str) -> Result<()> {
        let guard = self.input_controller.lock().await;
        let result = if let Some(ref ctrl) = *guard {
            ctrl.type_text(text).await
        } else {
            info!(text, "Script type_text (no controller)");
            Ok(())
        };
        drop(guard);

        let mut m = Map::new();
        m.insert("payload".into(), Self::replay_pack_type_text_payload(text));
        self.replay_record_script_input_if_recording("type_text", m, &result)
            .await;
        result
    }

    async fn sleep(&self, ms: u64) -> Result<()> {
        tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
        Ok(())
    }

    async fn wait_for_template(
        &self,
        name: &str,
        timeout_ms: u64,
        opts: Option<FindTemplateOpts>,
    ) -> Result<Option<MatchResult>> {
        let start = tokio::time::Instant::now();
        let timeout = tokio::time::Duration::from_millis(timeout_ms);

        while start.elapsed() < timeout {
            if let Some(m) = self.find_template(name, opts.clone()).await? {
                return Ok(Some(m));
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }
        Ok(None)
    }

    async fn wait_gone(&self, name: &str, timeout_ms: u64) -> Result<bool> {
        let start = tokio::time::Instant::now();
        let timeout = tokio::time::Duration::from_millis(timeout_ms);

        while start.elapsed() < timeout {
            if self.find_template(name, None).await?.is_none() {
                return Ok(true);
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }
        Ok(false)
    }

    async fn wait_for_color(&self, x: i32, y: i32, color: &str, timeout_ms: u64) -> Result<bool> {
        let start = tokio::time::Instant::now();
        let timeout = tokio::time::Duration::from_millis(timeout_ms);

        while start.elapsed() < timeout {
            if self.color_match(x, y, color, 32).await? {
                return Ok(true);
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }
        Ok(false)
    }

    // === Wait (frame-based) ===
    async fn sleep_frames(&self, frames: u32) -> Result<()> {
        let fps = self.get_fps_value();
        let ms = (frames as f64 * 1000.0 / fps) as u64;
        tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
        Ok(())
    }

    async fn wait_for_template_frames(
        &self,
        name: &str,
        max_frames: u32,
        opts: Option<FindTemplateOpts>,
    ) -> Result<Option<MatchResult>> {
        for _ in 0..max_frames {
            if let Some(m) = self.find_template(name, opts.clone()).await? {
                return Ok(Some(m));
            }
            self.sleep_frames(1).await?;
        }
        Ok(None)
    }

    async fn wait_gone_frames(&self, name: &str, max_frames: u32) -> Result<bool> {
        for _ in 0..max_frames {
            if self.find_template(name, None).await?.is_none() {
                return Ok(true);
            }
            self.sleep_frames(1).await?;
        }
        Ok(false)
    }

    async fn wait_for_color_frames(
        &self,
        x: i32,
        y: i32,
        color: &str,
        max_frames: u32,
    ) -> Result<bool> {
        for _ in 0..max_frames {
            if self.color_match(x, y, color, 32).await? {
                return Ok(true);
            }
            self.sleep_frames(1).await?;
        }
        Ok(false)
    }

    // === Window ===
    async fn find_window(&self, title: &str) -> Result<Option<u64>> {
        use betternte_capture::{WindowFinder, WindowFinderImpl};
        let finder = WindowFinderImpl::new();
        let windows = finder.find_by_keyword(title)?;
        Ok(windows.into_iter().next().map(|w| w.hwnd))
    }

    async fn activate_window(&self, hwnd: u64) -> Result<()> {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
        };

        let hwnd = HWND(hwnd as *mut _);
        unsafe {
            if IsIconic(hwnd).into() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
        }
        Ok(())
    }

    async fn get_window_rect(&self, hwnd: u64) -> Result<Rect> {
        use betternte_capture::{WindowFinder, WindowFinderImpl};
        let finder = WindowFinderImpl::new();
        let info = finder.get_window_info(hwnd)?;
        let r = &info.client_rect;
        Ok(Rect {
            x: r.left,
            y: r.top,
            width: r.width(),
            height: r.height(),
        })
    }

    async fn get_screen_size(&self) -> Result<(u32, u32)> {
        use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
        unsafe {
            let w = GetSystemMetrics(SM_CXSCREEN) as u32;
            let h = GetSystemMetrics(SM_CYSCREEN) as u32;
            Ok((w, h))
        }
    }

    // === Inter-script ===
    async fn run_script(&self, name: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let runner = self.script_runner.lock().unwrap().clone();
        match runner {
            Some(runner) => runner(name.to_string(), params).await,
            None => {
                warn!("run_script: no script runner configured");
                Err(anyhow::anyhow!("Script runner not configured"))
            }
        }
    }

    async fn call_library(
        &self,
        library: &str,
        function: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let runner = self.library_runner.lock().unwrap().clone();
        match runner {
            Some(runner) => runner(library.to_string(), function.to_string(), args).await,
            None => {
                warn!("call_library: no library runner configured");
                Err(anyhow::anyhow!("Library runner not configured"))
            }
        }
    }

    // === Utilities ===
    fn log(&self, level: LogLevel, message: &str) {
        // Publish directly to EventBus for frontend display
        if let Some(ref bus) = self.event_bus {
            let level_str = match level {
                LogLevel::Debug => "debug",
                LogLevel::Info => "info",
                LogLevel::Warn => "warn",
                LogLevel::Error => "error",
            };
            let _ = bus.publish(betternte_core::EngineEvent::LogMessage {
                level: level_str.to_string(),
                module: "script".to_string(),
                message: message.to_string(),
                timestamp: chrono::Utc::now(),
            });
        }
        // Also log to tracing for console output
        match level {
            LogLevel::Debug => tracing::debug!("[script] {}", message),
            LogLevel::Info => tracing::info!("[script] {}", message),
            LogLevel::Warn => tracing::warn!("[script] {}", message),
            LogLevel::Error => tracing::error!("[script] {}", message),
        }
    }

    async fn notify(&self, title: &str, body: &str) -> Result<()> {
        self.permissions
            .check(&Permission::Notify)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let mgr = self.notification_manager.read().await;
        if !mgr.is_enabled() {
            info!(title, "Notifications globally disabled; drop");
            return Ok(());
        }
        if mgr.list_channels().is_empty() {
            info!(title, "No notification channels registered; drop");
            return Ok(());
        }
        let results = mgr.send_all(title, body).await;
        for (i, result) in results.iter().enumerate() {
            if let Err(e) = result {
                warn!(channel = i, error = %e, "Notification send failed");
            }
        }
        Ok(())
    }

    // === File operations (manifest-scoped, no permission needed) ===
    async fn read_store_file(&self, path: &str) -> Result<String> {
        let full = self.validate_store_path(path)?;
        tokio::fs::read_to_string(&full).await.map_err(|e| e.into())
    }

    async fn write_store_file(&self, path: &str, content: &str) -> Result<()> {
        let full = self.validate_store_path(path)?;
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&full, content).await.map_err(|e| e.into())
    }

    async fn list_store_files(&self, dir: &str) -> Result<Vec<String>> {
        let full = self.validate_store_path(dir)?;
        let mut entries = Vec::new();
        let mut dir_entries = tokio::fs::read_dir(&full).await?;
        while let Some(entry) = dir_entries.next_entry().await? {
            if let Some(name) = entry.file_name().to_str() {
                entries.push(name.to_string());
            }
        }
        Ok(entries)
    }

    // === File operations (system-level, permission-gated) ===
    async fn read_file(&self, path: &str) -> Result<String> {
        self.permissions
            .check(&Permission::FileRead {
                paths: vec![path.to_string()],
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        tokio::fs::read_to_string(path).await.map_err(|e| e.into())
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        self.permissions
            .check(&Permission::FileWrite {
                paths: vec![path.to_string()],
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        tokio::fs::write(path, content).await.map_err(|e| e.into())
    }

    async fn list_files(&self, dir: &str) -> Result<Vec<String>> {
        self.permissions
            .check(&Permission::FileRead {
                paths: vec![dir.to_string()],
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let mut entries = Vec::new();
        let mut dir_entries = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = dir_entries.next_entry().await? {
            if let Some(name) = entry.file_name().to_str() {
                entries.push(name.to_string());
            }
        }
        Ok(entries)
    }

    async fn file_exists(&self, path: &str) -> Result<bool> {
        self.permissions
            .check(&Permission::FileRead {
                paths: vec![path.to_string()],
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        Ok(tokio::fs::metadata(path).await.is_ok())
    }

    // === Network ===
    async fn http_get(&self, url: &str) -> Result<String> {
        self.permissions
            .check(&Permission::Network { domains: vec![] })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let client = reqwest::Client::new();
        let resp = client.get(url).send().await?;
        let body = resp.text().await?;
        Ok(body)
    }

    async fn http_post(&self, url: &str, body: &str) -> Result<String> {
        self.permissions
            .check(&Permission::Network { domains: vec![] })
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let client = reqwest::Client::new();
        let resp = client
            .post(url)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await?;
        let resp_body = resp.text().await?;
        Ok(resp_body)
    }

    // === Storage (manifest-scoped, no permission needed) ===
    async fn storage_get(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let data = self.read_storage_data().await;
        Ok(data.get(key).cloned())
    }

    async fn storage_set(&self, key: &str, value: serde_json::Value) -> Result<()> {
        let mut data = self.read_storage_data().await;
        data.insert(key.to_string(), value);
        self.write_storage_data(&data).await
    }

    async fn storage_delete(&self, key: &str) -> Result<()> {
        let mut data = self.read_storage_data().await;
        data.remove(key);
        self.write_storage_data(&data).await
    }

    async fn storage_keys(&self) -> Result<Vec<String>> {
        let data = self.read_storage_data().await;
        Ok(data.keys().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use betternte_core::image::PixelFormat;
    use std::path::PathBuf;

    // ── parse_hex_color ──

    #[test]
    fn test_parse_hex_color_6char() {
        let c = parse_hex_color("#FF0000").unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
        assert_eq!(c.a, 255);
    }

    #[test]
    fn test_parse_hex_color_6char_no_hash() {
        let c = parse_hex_color("00FF00").unwrap();
        assert_eq!(c.r, 0);
        assert_eq!(c.g, 255);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn test_parse_hex_color_8char() {
        let c = parse_hex_color("#FF000080").unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
        assert_eq!(c.a, 128);
    }

    #[test]
    fn test_parse_hex_color_lowercase() {
        let c = parse_hex_color("#ff8040").unwrap();
        assert_eq!(c.r, 0xff);
        assert_eq!(c.g, 0x80);
        assert_eq!(c.b, 0x40);
    }

    #[test]
    fn test_parse_hex_color_with_spaces() {
        let c = parse_hex_color("  #FF0000  ").unwrap();
        assert_eq!(c.r, 255);
    }

    #[test]
    fn test_parse_hex_color_invalid_length() {
        assert!(parse_hex_color("#FFF").is_none());
        assert!(parse_hex_color("#12345").is_none());
        assert!(parse_hex_color("#1234567").is_none());
        assert!(parse_hex_color("#123456789").is_none());
    }

    #[test]
    fn test_parse_hex_color_empty() {
        assert!(parse_hex_color("").is_none());
    }

    #[test]
    fn test_parse_hex_color_invalid_chars() {
        assert!(parse_hex_color("#GGGGGG").is_none());
    }

    // ── color_to_hex ──

    #[test]
    fn test_color_to_hex_red() {
        let c = Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };
        assert_eq!(color_to_hex(&c), "#ff0000");
    }

    #[test]
    fn test_color_to_hex_white() {
        let c = Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };
        assert_eq!(color_to_hex(&c), "#ffffff");
    }

    #[test]
    fn test_color_to_hex_black() {
        let c = Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        };
        assert_eq!(color_to_hex(&c), "#000000");
    }

    #[test]
    fn test_color_to_hex_zero_padded() {
        let c = Color {
            r: 1,
            g: 2,
            b: 3,
            a: 255,
        };
        assert_eq!(color_to_hex(&c), "#010203");
    }

    // ── parse_hex_color + color_to_hex round-trip ──

    #[test]
    fn test_hex_color_roundtrip() {
        let original = "#aabbcc";
        let c = parse_hex_color(original).unwrap();
        let hex = color_to_hex(&c);
        assert_eq!(hex, original);
    }

    // ── script_region_to_core ──

    #[test]
    fn test_script_region_to_core() {
        let r = Region {
            x: 10,
            y: 20,
            width: 100,
            height: 200,
        };
        let core = script_region_to_core(&r);
        assert_eq!(core.x, 10);
        assert_eq!(core.y, 20);
        assert_eq!(core.width, 100);
        assert_eq!(core.height, 200);
    }

    // ── get_fps_value ──

    #[test]
    fn test_get_fps_value_default() {
        use std::sync::atomic::AtomicU32;
        let fps = AtomicU32::new(0);
        let raw = fps.load(Ordering::Relaxed);
        let value = if raw == 0 { 60.0 } else { raw as f64 / 100.0 };
        assert!((value - 60.0).abs() < 1e-6);
    }

    #[test]
    fn test_get_fps_value_set() {
        use std::sync::atomic::AtomicU32;
        let fps = AtomicU32::new(0);
        fps.store((120.0 * 100.0) as u32, Ordering::Relaxed);
        let raw = fps.load(Ordering::Relaxed);
        let value = if raw == 0 { 60.0 } else { raw as f64 / 100.0 };
        assert!((value - 120.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_update_shared_frame_updates_shared_and_counter() {
        let ctx = EngineScriptContext::new(serde_json::json!({}));
        let mut frame = CoreCaptureFrame::new(
            2, 1,
            vec![1, 2, 3, 4, 5, 6, 7, 8],
            PixelFormat::Rgba,
            "test_source".to_string(),
        );
        frame.timestamp = chrono::Utc::now();

        ctx.update_shared_frame(frame, 48.5).await;

        let cached = ctx.get_cached_frame().await.expect("cached frame");
        assert_eq!(cached.width, 2);
        assert_eq!(cached.height, 1);
        assert_eq!(*cached.data, vec![1, 2, 3, 4, 5, 6, 7, 8]);

        let shared = ctx.shared_frame.read().await.clone().expect("shared frame");
        assert_eq!(shared.frame_id, 1);
        assert_eq!(shared.frame.source, "test_source");
        assert!((shared.fps - 48.5).abs() < 1e-6);
        assert_eq!(ctx.frame_number.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_get_cached_frame_respects_fallback_flag_without_shared_frame() {
        let ctx = EngineScriptContext::new(serde_json::json!({}));
        ctx.set_allow_fallback_capture(false);

        let err = match ctx.get_cached_frame().await {
            Ok(_) => panic!("should fail when fallback disabled and no shared frame"),
            Err(e) => e,
        };
        assert!(err
            .to_string()
            .contains("No shared frame available and fallback capture is disabled"));

        let mut frame = CoreCaptureFrame::new(
            1, 1,
            vec![10, 20, 30, 40],
            PixelFormat::Rgba,
            "loop".to_string(),
        );
        frame.timestamp = chrono::Utc::now();
        ctx.update_shared_frame(frame, 30.0).await;

        let cached = ctx
            .get_cached_frame()
            .await
            .expect("shared frame should work");
        assert_eq!(cached.width, 1);
        assert_eq!(cached.height, 1);
        assert_eq!(*cached.data, vec![10, 20, 30, 40]);
    }

    #[test]
    fn test_storage_path_follows_current_template_dir() {
        let ctx = EngineScriptContext::new(serde_json::json!({}));
        ctx.set_template_dir(PathBuf::from("D:/tmp/some_script"));
        assert_eq!(
            ctx.current_storage_path(),
            PathBuf::from("D:/tmp/some_script/storage.json")
        );
    }

    #[test]
    fn test_store_dir_follows_current_template_dir() {
        let ctx = EngineScriptContext::new(serde_json::json!({}));
        ctx.set_template_dir(PathBuf::from("D:/tmp/some_library"));
        assert_eq!(
            ctx.current_store_dir(),
            PathBuf::from("D:/tmp/some_library/store")
        );
    }

    #[test]
    fn test_ocr_batch_eligible_large_region() {
        let region = Region {
            x: 0,
            y: 0,
            width: 1280,
            height: 360,
        };
        assert!(EngineScriptContext::ocr_batch_eligible(&region, 1920, 1080));
    }

    #[test]
    fn test_ocr_batch_eligible_small_region() {
        let region = Region {
            x: 10,
            y: 10,
            width: 120,
            height: 40,
        };
        assert!(!EngineScriptContext::ocr_batch_eligible(
            &region, 1920, 1080
        ));
    }
}
