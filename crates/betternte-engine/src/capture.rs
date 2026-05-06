//! Capture orchestration: capture loop, window binding, screenshot testing.

use tracing::{error, info, warn};
#[cfg(windows)]
use windows::Win32::Foundation::HWND;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::IsIconic;

use super::Engine;

struct RecordingScreenCapture {
    inner: Box<dyn betternte_core::ScreenCapture>,
    session: std::sync::Arc<crate::replay_recorder::ReplaySessionInner>,
    capture_ticks: std::sync::atomic::AtomicU64,
}

impl RecordingScreenCapture {
    fn new(
        inner: Box<dyn betternte_core::ScreenCapture>,
        session: std::sync::Arc<crate::replay_recorder::ReplaySessionInner>,
    ) -> Self {
        Self {
            inner,
            session,
            capture_ticks: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[async_trait::async_trait]
impl betternte_core::ScreenCapture for RecordingScreenCapture {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn is_capturing(&self) -> bool {
        self.inner.is_capturing()
    }

    async fn start(&mut self, target: &betternte_core::CaptureTarget) -> anyhow::Result<()> {
        self.inner.start(target).await
    }

    fn configure(&self, options: betternte_core::CaptureRuntimeOptions) {
        self.inner.configure(options);
    }

    async fn capture(&self) -> anyhow::Result<betternte_core::CaptureFrame> {
        let frame = self.inner.capture().await?;
        let iv = self.session.frame_sample_interval;
        if iv > 0 {
            let ticks = self
                .capture_ticks
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1;
            if ticks % iv as u64 == 0 {
                self.session.record_frame_sample(&frame).await;
            }
        }
        Ok(frame)
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        self.inner.stop().await
    }

    fn last_latency_ms(&self) -> Option<f64> {
        self.inner.last_latency_ms()
    }

    fn fps(&self) -> f64 {
        self.inner.fps()
    }
}

struct ReplayPngScreenCapture {
    frames: Vec<std::path::PathBuf>,
    next_index: std::sync::atomic::AtomicUsize,
    sequence: std::sync::atomic::AtomicU64,
    capturing: std::sync::atomic::AtomicBool,
}

impl ReplayPngScreenCapture {
    fn new(frames: Vec<std::path::PathBuf>) -> anyhow::Result<Self> {
        if frames.is_empty() {
            anyhow::bail!("replay: empty frame list");
        }
        Ok(Self {
            frames,
            next_index: std::sync::atomic::AtomicUsize::new(0),
            sequence: std::sync::atomic::AtomicU64::new(0),
            capturing: std::sync::atomic::AtomicBool::new(false),
        })
    }
}

#[async_trait::async_trait]
impl betternte_core::ScreenCapture for ReplayPngScreenCapture {
    fn name(&self) -> &str {
        "ReplayPng"
    }

    fn is_capturing(&self) -> bool {
        self.capturing.load(std::sync::atomic::Ordering::Relaxed)
    }

    async fn start(&mut self, _target: &betternte_core::CaptureTarget) -> anyhow::Result<()> {
        self.capturing
            .store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn capture(&self) -> anyhow::Result<betternte_core::CaptureFrame> {
        if !self.capturing.load(std::sync::atomic::Ordering::Relaxed) {
            anyhow::bail!("ReplayPng capture called before start");
        }
        let idx = self
            .next_index
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % self.frames.len();
        let path = self.frames[idx].clone();
        let sequence = self
            .sequence
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .wrapping_add(1);
        let path_logged = path.display().to_string();
        match tokio::task::spawn_blocking(move || {
            crate::replay_playback::decode_png_into_core_frame(&path, sequence)
        })
        .await
        {
            Ok(Ok(frame)) => Ok(frame),
            Ok(Err(e)) => Err(anyhow::anyhow!(
                "replay PNG decode failed ({}): {}",
                path_logged,
                e
            )),
            Err(e) => Err(anyhow::anyhow!("replay decode join error: {}", e)),
        }
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        self.capturing
            .store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    fn last_latency_ms(&self) -> Option<f64> {
        None
    }

    fn fps(&self) -> f64 {
        0.0
    }
}

/// Check if a captured frame is mostly black (all-zero or nearly all-zero pixels).
/// Returns true if >99% of pixels are black (each channel < 4).
fn is_black_frame(frame: &betternte_core::CaptureFrame) -> bool {
    if frame.width < 100 || frame.height < 100 {
        return false;
    }
    let bpp = frame.format.bytes_per_pixel() as usize;
    let total_pixels = (frame.width * frame.height) as usize;
    let threshold: u8 = 4;
    let black_count = frame
        .data
        .chunks(bpp)
        .filter(|px| px.iter().all(|&c| c < threshold))
        .count();
    // Consider black if >99% of pixels are black
    black_count * 100 > total_pixels * 99
}

impl Engine {
    #[cfg(windows)]
    pub fn list_windows_static() -> anyhow::Result<Vec<betternte_core::GameWindow>> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            use betternte_capture::{WindowFinder, WindowFinderImpl};
            let finder = WindowFinderImpl::new();
            let _ = tx.send(finder.find_by_keyword(""));
        });
        match rx.recv_timeout(std::time::Duration::from_secs(3)) {
            Ok(inner) => inner.map_err(Into::into),
            Err(_) => Err(anyhow::anyhow!("list_windows timeout after 3s")),
        }
    }

    #[cfg(not(windows))]
    pub fn list_windows_static() -> anyhow::Result<Vec<betternte_core::GameWindow>> {
        Ok(vec![])
    }

    #[cfg(windows)]
    pub fn find_game_window_by_title(title: &str) -> anyhow::Result<betternte_core::GameWindow> {
        let title = title.trim();
        if title.is_empty() {
            anyhow::bail!("game.window_title_keyword is empty; configure exact window title");
        }
        let title_owned = title.to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            use betternte_capture::{WindowFinder, WindowFinderImpl};
            let finder = WindowFinderImpl::new();
            let result = finder.find_by_title(title_owned.trim()).and_then(|windows| {
                windows.into_iter().next().ok_or_else(|| {
                    anyhow::anyhow!("no matching window (exact title={:?})", Some(title_owned.trim()))
                })
            });
            let _ = tx.send(result);
        });
        match rx.recv_timeout(std::time::Duration::from_secs(3)) {
            Ok(inner) => inner.map_err(Into::into),
            Err(_) => Err(anyhow::anyhow!("find_game_window timeout after 3s")),
        }
    }

    #[cfg(not(windows))]
    pub fn find_game_window_by_title(_title: &str) -> anyhow::Result<betternte_core::GameWindow> {
        anyhow::bail!("find_game_window is not supported on this platform")
    }

    fn build_overlay_config(&self, width: u32, height: u32) -> betternte_overlay::OverlayConfig {
        let mode = match self.config.overlay.mode {
            betternte_core::config::OverlayMode::Hidden => betternte_overlay::OverlayMode::Hidden,
            betternte_core::config::OverlayMode::Minimal => betternte_overlay::OverlayMode::Minimal,
            betternte_core::config::OverlayMode::Detailed => {
                betternte_overlay::OverlayMode::Detailed
            }
        };
        let background_color =
            betternte_core::Color::from_hex(&self.config.overlay.background_color);
        betternte_overlay::OverlayConfig {
            enabled: self.config.overlay.enabled,
            opacity: self.config.overlay.opacity,
            width,
            height,
            mode,
            font_size: self.config.overlay.font_size,
            background_color,
            ..betternte_overlay::OverlayConfig::default()
        }
    }

    fn configure_overlay_for_window(
        &self,
        window: &betternte_core::GameWindow,
    ) -> anyhow::Result<()> {
        let mut guard = self
            .overlay_manager
            .lock()
            .map_err(|_| anyhow::anyhow!("overlay manager lock poisoned"))?;

        if !self.config.overlay.enabled
            || matches!(
                self.config.overlay.mode,
                betternte_core::config::OverlayMode::Hidden
            )
        {
            *guard = None;
            return Ok(());
        }

        let width = window.client_rect.width().max(1) as u32;
        let height = window.client_rect.height().max(1) as u32;
        let overlay_config = self.build_overlay_config(width, height);
        let mut manager = betternte_overlay::OverlayManager::new(&overlay_config)
            .map_err(|e| anyhow::anyhow!("failed to create overlay manager: {}", e))?;
        manager
            .bind_to_game(window.hwnd as usize)
            .map_err(|e| anyhow::anyhow!("failed to bind overlay to game window: {}", e))?;
        manager
            .show()
            .map_err(|e| anyhow::anyhow!("failed to show overlay: {}", e))?;
        if let Err(e) = manager.render_fps_text(0.0) {
            warn!(error = %e, "Overlay FPS render failed on init");
        }
        *guard = Some(manager);
        Ok(())
    }

    pub fn toggle_overlay(&self) -> anyhow::Result<bool> {
        let mut guard = self
            .overlay_manager
            .lock()
            .map_err(|_| anyhow::anyhow!("overlay manager lock poisoned"))?;
        let manager = guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("overlay manager not initialized"))?;
        if manager.is_visible() {
            manager
                .hide()
                .map_err(|e| anyhow::anyhow!("failed to hide overlay: {}", e))?;
            Ok(false)
        } else {
            manager
                .show()
                .map_err(|e| anyhow::anyhow!("failed to show overlay: {}", e))?;
            Ok(true)
        }
    }

    pub fn is_overlay_visible(&self) -> bool {
        self.overlay_manager
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|m| m.is_visible()))
            .unwrap_or(false)
    }

    fn resolve_capture_target(
        capture_config: &betternte_core::config::CaptureConfig,
        hwnd: Option<u64>,
    ) -> anyhow::Result<betternte_capture::CaptureTarget> {
        use betternte_core::config::CaptureTargetType;
        match capture_config.target_type {
            CaptureTargetType::Display => Ok(betternte_capture::CaptureTarget::Display {
                index: capture_config.display_index,
            }),
            CaptureTargetType::Window => {
                let hwnd = hwnd.ok_or_else(|| anyhow::anyhow!("No game window found"))?;
                Ok(betternte_capture::CaptureTarget::Window { hwnd })
            }
        }
    }

    /// Create and start a capture engine with fallback on start() failure.
    ///
    /// For Auto mode: tests all whitelist methods (start + warmup + timed capture),
    /// picks the fastest — similar to MaaFramework's speed_test approach.
    /// For specific methods: tries that method first, falls back to speed test on failure.
    async fn create_and_start_capture_engine(
        capture_config: &betternte_core::config::CaptureConfig,
        target: &betternte_capture::CaptureTarget,
    ) -> anyhow::Result<(Box<dyn betternte_capture::ScreenCapture>, betternte_core::config::CaptureMethod)> {
        use betternte_core::config::CaptureMethod;

        let runtime_opts = Self::build_capture_runtime_options(capture_config);
        let whitelist = &capture_config.method_whitelist;

        // For non-Auto: try the specific method first
        if capture_config.method != CaptureMethod::Auto {
            match betternte_capture::factory::create_capture_engine_with_fps_for_target(
                &capture_config.method,
                whitelist,
                capture_config.fps_cap,
                Some(target),
            ) {
                Ok(mut engine) => {
                    let name = engine.name().to_string();
                    engine.configure(runtime_opts.clone());
                    if engine.start(target).await.is_ok() {
                        info!(engine = %name, method = ?capture_config.method, "Capture engine started");
                        return Ok((engine, capture_config.method.clone()));
                    }
                    warn!(engine = %name, "Specific method failed, falling back to speed test");
                }
                Err(e) => {
                    warn!(error = %e, method = ?capture_config.method, "Failed to create engine, falling back to speed test");
                }
            }
        }

        // Speed test: try all whitelist methods, pick the fastest
        Self::speed_test_capture_engine(capture_config, target, &runtime_opts).await
    }

    /// Speed test all capture methods in the whitelist, return the fastest one.
    /// Mirrors MaaFramework's approach: warmup + timed capture for each method.
    async fn speed_test_capture_engine(
        capture_config: &betternte_core::config::CaptureConfig,
        target: &betternte_capture::CaptureTarget,
        runtime_opts: &betternte_core::CaptureRuntimeOptions,
    ) -> anyhow::Result<(Box<dyn betternte_capture::ScreenCapture>, betternte_core::config::CaptureMethod)> {
        use betternte_core::config::CaptureMethod;

        let whitelist = &capture_config.method_whitelist;

        // Build ordered list of methods to test (priority order from factory chain)
        let ordered_methods: Vec<CaptureMethod> = [
            CaptureMethod::BitBlt,
            CaptureMethod::WindowsGraphicsCapture,
            CaptureMethod::DxgiDesktopDuplication,
            CaptureMethod::PrintWindow,
            CaptureMethod::DwmSharedSurface,
        ]
        .iter()
        .filter(|m| whitelist.contains(m))
        .cloned()
        .collect();

        if ordered_methods.is_empty() {
            anyhow::bail!("No capture methods in whitelist");
        }

        let mut best_engine: Option<Box<dyn betternte_capture::ScreenCapture>> = None;
        let mut best_method = CaptureMethod::Auto;
        let mut best_latency = std::time::Duration::from_secs(u64::MAX);

        for method in &ordered_methods {
            let engine = match betternte_capture::factory::create_capture_engine_with_fps_for_target(
                method,
                whitelist,
                capture_config.fps_cap,
                Some(target),
            ) {
                Ok(e) => e,
                Err(e) => {
                    warn!(method = ?method, error = %e, "Speed test: failed to create");
                    continue;
                }
            };

            let _engine_name = engine.name().to_string();
            let mut engine = engine;
            engine.configure(runtime_opts.clone());

            // Start
            if let Err(e) = engine.start(target).await {
                warn!(method = ?method, error = %e, "Speed test: failed to start");
                continue;
            }

            // Warmup (first frame may be slow due to D3D11 init)
            let _ = engine.capture().await;

            // Timed capture
            let t0 = std::time::Instant::now();
            let frame = match engine.capture().await {
                Ok(f) => f,
                Err(e) => {
                    warn!(method = ?method, error = %e, "Speed test: capture failed");
                    let _ = engine.stop().await;
                    continue;
                }
            };
            let latency = t0.elapsed();

            info!(
                method = ?method,
                latency_ms = latency.as_secs_f64() * 1000.0,
                size = format!("{}x{}", frame.width, frame.height),
                "Speed test: OK"
            );

            if latency < best_latency {
                // Stop previous best
                if let Some(mut prev) = best_engine.take() {
                    let _ = prev.stop().await;
                }
                best_latency = latency;
                best_method = method.clone();
                best_engine = Some(engine);
            } else {
                let _ = engine.stop().await;
            }
        }

        match best_engine {
            Some(engine) => {
                info!(
                    method = ?best_method,
                    latency_ms = best_latency.as_secs_f64() * 1000.0,
                    "Speed test: selected best engine"
                );
                Ok((engine, best_method))
            }
            None => anyhow::bail!("All capture methods failed during speed test"),
        }
    }

    fn build_capture_runtime_options(
        capture_config: &betternte_core::config::CaptureConfig,
    ) -> betternte_core::CaptureRuntimeOptions {
        use betternte_core::config::HdrPolicy;
        betternte_core::CaptureRuntimeOptions {
            crop_to_client: true, // 始终裁剪到客户区，确保所有截图引擎输出一致
            hdr_to_sdr: match capture_config.hdr_policy {
                HdrPolicy::Off => false,
                HdrPolicy::Auto | HdrPolicy::Force => true,
            },
            recover_on_resize: capture_config.recover_on_resize,
            recover_on_monitor_switch: capture_config.recover_on_monitor_switch,
        }
    }

    async fn bind_script_ctx_hwnd(
        &self,
        ctx: &super::script_ctx::EngineScriptContext,
        hwnd: u64,
    ) -> anyhow::Result<()> {
        ctx.set_hwnd(hwnd);

        // Create and init input controller for this window.
        use betternte_core::{InputController, InputMode, InputTarget};
        use betternte_input::{
            FailoverConfig, FailoverInputController, InputRecordEvent, KeyMapper,
            QueuedInputController, RecordingInputController, Win32Input,
        };
        use serde_json::Value;
        use std::sync::Arc;
        use std::time::Duration;
        use std::{future::Future, pin::Pin};

        let primary_mode = match self.config.advanced.input_mode {
            InputMode::Auto => InputMode::Foreground,
            other => other,
        };
        let target = match primary_mode {
            InputMode::Foreground => InputTarget::NativeWindow { hwnd },
            InputMode::Background => InputTarget::NativeWindowBackground { hwnd },
            InputMode::Auto => InputTarget::NativeWindow { hwnd },
        };
        let mut primary_ctrl = Win32Input::new_with_backend(
            KeyMapper::new(std::collections::HashMap::new()),
            self.config.advanced.foreground_input_backend,
        );
        primary_ctrl
            .init(&target)
            .await
            .map_err(|e| anyhow::anyhow!("failed to init primary input controller: {}", e))?;

        let fallback_enabled = if self.config.advanced.input_mode == InputMode::Auto {
            true
        } else {
            self.config.advanced.input_fallback_enabled
        };
        let fallback_mode = if self.config.advanced.input_mode == InputMode::Auto {
            InputMode::Background
        } else {
            self.config.advanced.input_fallback_mode
        };

        let fallback_ctrl = if fallback_enabled {
            let fallback_target = match fallback_mode {
                InputMode::Foreground => InputTarget::NativeWindow { hwnd },
                InputMode::Background => InputTarget::NativeWindowBackground { hwnd },
                InputMode::Auto => InputTarget::NativeWindowBackground { hwnd },
            };
            let mut ctrl = Win32Input::new_with_backend(
                KeyMapper::new(std::collections::HashMap::new()),
                self.config.advanced.foreground_input_backend,
            );
            ctrl.init(&fallback_target)
                .await
                .map_err(|e| anyhow::anyhow!("failed to init fallback input controller: {}", e))?;
            Some(Box::new(ctrl) as Box<dyn InputController>)
        } else {
            None
        };

        let failover = FailoverInputController::new(
            Box::new(primary_ctrl),
            fallback_ctrl,
            FailoverConfig {
                enabled: fallback_enabled,
                fail_threshold: self.config.advanced.input_fail_threshold.max(1),
                probe_every: Duration::from_millis(
                    self.config.advanced.input_probe_every_ms.max(1),
                ),
                action_timeout: Duration::from_millis(
                    self.config.advanced.input_action_timeout_ms.max(1),
                ),
                log_switch: self.config.advanced.input_log_switch,
            },
        );

        let backend_label = failover.active_backend_label();
        let input_rate_limit = self.config.advanced.input_rate_limit;
        let queue_enabled = input_rate_limit > 0;
        let queue_min_interval_ms: u64;
        let base_controller: Box<dyn InputController> = if queue_enabled {
            let queued =
                QueuedInputController::from_initialized(Box::new(failover), input_rate_limit);
            queue_min_interval_ms = queued.min_interval().as_millis() as u64;
            Box::new(queued)
        } else {
            queue_min_interval_ms = 0;
            Box::new(failover)
        };
        let recorder_ctx = self
            .script_ctx
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("ScriptContext not initialized"))?;
        let record_sink = Arc::new(move |event: InputRecordEvent| {
            let recorder_ctx = recorder_ctx.clone();
            Box::pin(async move {
                let args = match event.args {
                    Value::Object(map) => map,
                    other => {
                        let mut map = serde_json::Map::new();
                        map.insert("value".into(), other);
                        map
                    }
                };
                recorder_ctx
                    .record_script_input(&event.method, args, event.ok, event.error)
                    .await;
            }) as Pin<Box<dyn Future<Output = ()> + Send>>
        });
        let final_controller: Box<dyn InputController> =
            Box::new(RecordingInputController::new(base_controller, record_sink));

        ctx.set_input_controller(final_controller).await;
        info!(
            hwnd,
            input_mode = %self.config.advanced.input_mode,
            primary_mode = %primary_mode,
            fallback_mode = %fallback_mode,
            fallback_enabled = fallback_enabled,
            input_rate_limit = input_rate_limit,
            queue_enabled = queue_enabled,
            queue_min_interval_ms = queue_min_interval_ms,
            foreground_backend = %self.config.advanced.foreground_input_backend,
            active = %backend_label,
            "Input controller injected"
        );
        Ok(())
    }

    /// 返回实际使用的截图引擎名称。
    ///
    /// 如果配置为 Auto，则从白名单中解析自动选择的结果。
    pub fn resolved_capture_method(&self) -> String {
        use betternte_core::config::CaptureMethod;
        match self.config.capture.method {
            CaptureMethod::Auto => betternte_capture::resolve_auto_capture_method(
                &self.config.capture.method_whitelist,
            )
            .to_string(),
            other => format!("{:?}", other),
        }
    }

    /// 返回所有截图方式及其可用性、白名单状态。
    ///
    /// 每个元素为 `(method_name, is_available, is_in_whitelist)`。
    pub fn available_capture_methods(&self) -> Vec<(String, bool, bool)> {
        let methods = betternte_capture::available_capture_methods();
        let whitelist = &self.config.capture.method_whitelist;
        methods
            .into_iter()
            .map(|info| {
                let name = format!("{}", info.method);
                let in_wl = whitelist.contains(&info.method);
                (name, info.available, in_wl)
            })
            .collect()
    }

    /// 返回当前输入模式的显示名称。
    pub fn resolved_input_mode(&self) -> String {
        self.config.advanced.input_mode.to_string()
    }

    /// 列出系统中所有可见窗口。
    #[cfg(windows)]
    pub fn list_windows(&self) -> anyhow::Result<Vec<betternte_core::GameWindow>> {
        let windows = Self::list_windows_static()?;
        info!(count = windows.len(), "Listed windows");
        Ok(windows)
    }

    /// 非 Windows 平台的桩实现。
    #[cfg(not(windows))]
    pub fn list_windows(&self) -> anyhow::Result<Vec<betternte_core::GameWindow>> {
        Ok(vec![])
    }

    /// Resolve the configured game window using exact title match.
    #[cfg(windows)]
    pub fn find_game_window(&self) -> anyhow::Result<betternte_core::GameWindow> {
        Self::find_game_window_by_title(&self.config.game.window_title_keyword)
    }

    #[cfg(not(windows))]
    pub fn find_game_window(&self) -> anyhow::Result<betternte_core::GameWindow> {
        anyhow::bail!("find_game_window is not supported on this platform")
    }

    /// 将引擎绑定到指定窗口。
    pub async fn bind_window(&self, hwnd: u64) -> anyhow::Result<()> {
        #[cfg(windows)]
        let window_info = {
            use betternte_capture::{WindowFinder, WindowFinderImpl};
            let finder = WindowFinderImpl::new();
            Some(finder.get_window_info(hwnd)?)
        };
        #[cfg(not(windows))]
        let window_info: Option<betternte_core::GameWindow> = None;

        let ctx = self
            .script_ctx
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ScriptContext not initialized"))?;
        self.bind_script_ctx_hwnd(ctx, hwnd).await?;
        if let Some(ref window) = window_info {
            if let Err(e) = self.configure_overlay_for_window(window) {
                warn!(error = %e, "Overlay setup failed on bind_window; continue without overlay");
            }
        }
        info!(hwnd, "bind_window: script context fully bound");
        Ok(())
    }

    /// Get the script context as a trait object for the runtime.
    ///
    /// When `config.advanced.debug_mode` is true, wraps the context with
    /// `DebugScriptContext` that intercepts all calls and publishes trace events.
    pub(super) fn script_context(
        &self,
    ) -> Option<std::sync::Arc<dyn betternte_script::ScriptContext>> {
        self.script_ctx.clone().map(|ctx| {
            if self.config.advanced.debug_mode {
                let debug_ctx = crate::debug_ctx::DebugScriptContext::new(
                    ctx.clone() as std::sync::Arc<dyn betternte_script::ScriptContext>,
                    self.event_bus.clone(),
                );
                std::sync::Arc::new(debug_ctx)
                    as std::sync::Arc<dyn betternte_script::ScriptContext>
            } else {
                ctx as std::sync::Arc<dyn betternte_script::ScriptContext>
            }
        })
    }

    /// Bind the game window to the script context.
    ///
    /// Also creates and injects an input controller targeting the game window.
    ///
    /// Returns the bound window handle on success.
    pub(super) async fn bind_script_ctx_window(
        &self,
        ctx: &super::script_ctx::EngineScriptContext,
    ) -> anyhow::Result<u64> {
        let keyword = self.config.game.window_title_keyword.trim();
        if keyword.is_empty() {
            anyhow::bail!(
                "game.window_title_keyword is empty: engine requires exact target window title before start"
            );
        }

        info!(
            keyword = %keyword,
            "bind_script_ctx_window: searching for game window"
        );
        let window = self
            .find_game_window()
            .map_err(|e| anyhow::anyhow!("failed to find game window: {}", e))?;

        info!(hwnd = window.hwnd, title = %window.title, "bind_script_ctx_window: found window");
        self.bind_script_ctx_hwnd(ctx, window.hwnd).await?;
        if let Err(e) = self.configure_overlay_for_window(&window) {
            warn!(
                error = %e,
                "Overlay setup failed on startup bind; engine keeps running without overlay"
            );
        }
        Ok(window.hwnd)
    }

    /// Shared FPS smoothing, shared-frame update, optional replay PNG sampling, and trigger tick.
    /// Takes `frame.data` for the script tick via [`std::mem::take`] (call only at end of tick).
    async fn tick_capture_frame_after_acquire(
        frame: &mut betternte_core::CaptureFrame,
        source_override: Option<&str>,
        now: std::time::Instant,
        fps_cap: u32,
        last_capture_at: &mut Option<std::time::Instant>,
        smoothed_fps: &mut Option<f64>,
        ctx: &std::sync::Arc<super::script_ctx::EngineScriptContext>,
        runtime: &std::sync::Arc<betternte_script::ScriptRuntime>,
        ctx_trait: &std::sync::Arc<dyn betternte_script::ScriptContext>,
        trigger_tick_log: &'static str,
    ) {
        let mut current_fps = smoothed_fps.unwrap_or(fps_cap as f64);
        if let Some(prev) = *last_capture_at {
            let dt = now.duration_since(prev).as_secs_f64();
            if dt > 0.0 {
                let instant_fps = 1.0 / dt;
                let fps = match *smoothed_fps {
                    Some(prev_smooth) => prev_smooth * 0.8 + instant_fps * 0.2,
                    None => instant_fps,
                };
                *smoothed_fps = Some(fps);
                current_fps = fps;
            }
        }
        *last_capture_at = Some(now);

        if let Some(s) = source_override {
            frame.source = s.to_string();
        }

        ctx.update_shared_frame(frame.clone(), current_fps).await;

        let data = std::mem::take(&mut frame.data);
        let script_frame = betternte_script::CaptureFrame {
            width: frame.width,
            height: frame.height,
            data,
        };
        if let Err(e) = runtime.tick_triggers(ctx_trait, &script_frame).await {
            warn!(error = %e, "{}", trigger_tick_log);
        }
    }

    /// Background capture loop: captures frames and ticks all enabled triggers.
    pub(super) async fn capture_loop(
        fps_cap: u32,
        hwnd: Option<u64>,
        runtime: Option<std::sync::Arc<betternte_script::ScriptRuntime>>,
        ctx: Option<std::sync::Arc<super::script_ctx::EngineScriptContext>>,
        capture_config: betternte_core::config::CaptureConfig,
        mut stop_rx: tokio::sync::oneshot::Receiver<()>,
        replay_session: Option<std::sync::Arc<crate::replay_recorder::ReplaySessionInner>>,
        replay_artifact_frames: Option<Vec<std::path::PathBuf>>,
    ) {
        let runtime = match runtime {
            Some(r) => r,
            None => return,
        };
        let ctx = match ctx {
            Some(c) => c,
            None => return,
        };
        if let Some(hwnd) = hwnd {
            ctx.set_hwnd(hwnd);
        }

        let frame_interval = tokio::time::Duration::from_millis(1000 / fps_cap as u64);
        let ctx_trait: std::sync::Arc<dyn betternte_script::ScriptContext> = ctx.clone();
        let is_replay_artifact = replay_artifact_frames.is_some();
        info!(
            fps_cap,
            replay_artifact = is_replay_artifact,
            target_type = ?capture_config.target_type,
            "Capture loop started"
        );
        let mut current_method: Option<betternte_core::config::CaptureMethod> = None;
        let mut engine: Box<dyn betternte_core::ScreenCapture> = match replay_artifact_frames {
            Some(frames) => {
                info!(
                    count = frames.len(),
                    first = ?frames.first(),
                    "Replay-artifact capture source enabled"
                );
                match ReplayPngScreenCapture::new(frames) {
                    Ok(replay_engine) => Box::new(replay_engine),
                    Err(e) => {
                        error!(error = %e, "Failed to create replay PNG capture engine");
                        return;
                    }
                }
            }
            None => {
                let target = match Self::resolve_capture_target(&capture_config, hwnd) {
                    Ok(t) => t,
                    Err(e) => {
                        warn!(error = %e, "No valid capture target, capture loop not starting");
                        return;
                    }
                };
                match Self::create_and_start_capture_engine(&capture_config, &target).await {
                    Ok((e, m)) => { current_method = Some(m); e },
                    Err(e) => {
                        error!(error = %e, "All capture engines failed for trigger loop");
                        return;
                    }
                }
            }
        };
        if let Some(rep) = replay_session {
            engine = Box::new(RecordingScreenCapture::new(engine, rep));
        }

        // Warmup: first hardware frame may be slow; replay PNG doesn't need warmup.
        if !is_replay_artifact {
            let _ = engine.capture().await;
        }

        let mut last_capture_at: Option<std::time::Instant> = None;
        let mut smoothed_fps: Option<f64> = None;

        loop {
            tokio::select! {
                _ = &mut stop_rx => {
                    info!("Capture loop stopped");
                    break;
                }
                _ = tokio::time::sleep(frame_interval) => {
                    #[cfg(windows)]
                    {
                        if matches!(
                            capture_config.target_type,
                            betternte_core::config::CaptureTargetType::Window
                        ) && matches!(
                            capture_config.minimized_behavior,
                            betternte_core::config::MinimizedBehavior::Pause
                        ) {
                            if let Some(hwnd) = hwnd {
                                if unsafe { IsIconic(HWND(hwnd as *mut _)).as_bool() } {
                                    continue;
                                }
                            }
                        }
                    }
                    match engine.capture().await {
                        Ok(mut frame) => {
                            // Black frame detection: if we get a black frame and the method
                            // is known to produce black frames for hardware-accelerated windows,
                            // try to switch to another method.
                            if !is_replay_artifact && is_black_frame(&frame) {
                                if let Some(ref method) = current_method {
                                    use betternte_core::config::CaptureMethod;
                                    // BitBlt and ScreenDC are known to produce black frames
                                    // for DirectX/Vulkan/OpenGL windows
                                    if matches!(method, CaptureMethod::BitBlt | CaptureMethod::DwmSharedSurface) {
                                        warn!(
                                            method = ?method,
                                            width = frame.width,
                                            height = frame.height,
                                            "Black frame detected, attempting to switch capture method"
                                        );
                                        let target = match Self::resolve_capture_target(&capture_config, hwnd) {
                                            Ok(t) => t,
                                            Err(e) => {
                                                warn!(error = %e, "Failed to resolve capture target for fallback");
                                                continue;
                                            }
                                        };
                                        // Try to create a new engine excluding the current method
                                        let mut fallback_config = capture_config.clone();
                                        fallback_config.method_whitelist.retain(|m| m != method);
                                        if !fallback_config.method_whitelist.is_empty() {
                                            match Self::speed_test_capture_engine(&fallback_config, &target, &Self::build_capture_runtime_options(&capture_config)).await {
                                                Ok((new_engine, new_method)) => {
                                                    info!(old_method = ?method, new_method = ?new_method, "Switched capture method due to black frame");
                                                    let _ = engine.stop().await;
                                                    engine = new_engine;
                                                    current_method = Some(new_method);
                                                    continue;
                                                }
                                                Err(e) => {
                                                    warn!(error = %e, "All fallback capture methods also failed");
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            let now = std::time::Instant::now();
                            Self::tick_capture_frame_after_acquire(
                                &mut frame,
                                if is_replay_artifact { None } else { Some("engine_loop") },
                                now,
                                fps_cap,
                                &mut last_capture_at,
                                &mut smoothed_fps,
                                &ctx,
                                &runtime,
                                &ctx_trait,
                                if is_replay_artifact {
                                    "Trigger tick error (replay-artifact)"
                                } else {
                                    "Trigger tick error"
                                },
                            )
                            .await;
                        }
                        Err(e) => {
                            warn!(error = %e, "Capture failed in trigger loop");
                            // Brief pause before retrying
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        }
                    }
                }
            }
        }

        let _ = engine.stop().await;
    }

    /// 测试截图：根据配置查找窗口、创建截图引擎、截取一帧并返回 PNG 字节。
    pub async fn test_screenshot(&self) -> anyhow::Result<Vec<u8>> {
        use betternte_capture::CaptureTarget;

        let target = match self.config.capture.target_type {
            betternte_core::config::CaptureTargetType::Display => CaptureTarget::Display {
                index: self.config.capture.display_index,
            },
            betternte_core::config::CaptureTargetType::Window => {
                let keyword = self.config.game.window_title_keyword.trim();
                if keyword.is_empty() {
                    anyhow::bail!(
                        "game.window_title_keyword is empty; please configure exact window title in settings first"
                    );
                }
                let window = self.find_game_window()?;
                let hwnd = window.hwnd;
                info!(hwnd = hwnd, title = %window.title, "Test screenshot: found window");
                CaptureTarget::Window { hwnd }
            }
        };

        // Create and start capture engine with fallback on start() failure
        let (mut engine, _used_method) = Self::create_and_start_capture_engine(
            &self.config.capture,
            &target,
        ).await?;

        // Warmup capture (first frame may be slow due to D3D11 init etc.)
        let _ = engine.capture().await?;

        // Actual capture
        let frame = engine.capture().await?;

        // Stop
        engine.stop().await?;

        // Convert BGRA -> RGBA
        let rgba_data: Vec<u8> = frame
            .data
            .chunks(4)
            .flat_map(|px| [px[2], px[1], px[0], px[3]])
            .collect();

        // Encode to PNG
        let img = image::RgbaImage::from_raw(frame.width, frame.height, rgba_data)
            .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer"))?;

        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png)?;

        info!(bytes = buf.get_ref().len(), "Test screenshot captured");
        Ok(buf.into_inner())
    }
}
