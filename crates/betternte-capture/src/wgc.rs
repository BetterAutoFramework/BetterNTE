//! Windows Graphics Capture (WGC) engine.
//!
//! Uses windows-capture 2.0 for hardware-accelerated screenshot capture.

use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;

use async_trait::async_trait;
use betternte_core::{CaptureFrame, PixelFormat};
use tokio::sync::watch;
use windows::Graphics::Capture::GraphicsCaptureSession;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowInfo, IsWindow, WINDOWINFO};
use windows_capture::capture::{CaptureControl, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::settings::{ColorFormat, MinimumUpdateIntervalSettings, Settings};
use windows_capture::window::Window;

use anyhow::Result as AnyhowResult;

use crate::error::CaptureError;
use crate::{CaptureTarget, ScreenCapture};

/// Shared state between WgcCapture and the capture handler
struct WgcCaptureState {
    last_frame: Option<CaptureFrame>,
    frame_counter: u64,
    last_latency_ms: Option<f64>,
    capturing: bool,
    start_time: Option<Instant>,
    width: u32,
    height: u32,
    /// Client rect offset for cropping
    client_left: i32,
    client_top: i32,
    client_width: u32,
    client_height: u32,
    target: Option<CaptureTarget>,
    fps_cap: u32,
    options: betternte_core::CaptureRuntimeOptions,
}

/// Shared crop info accessible by both WgcCapture and the frame handler
#[derive(Clone)]
struct CropInfo {
    left: usize,
    top: usize,
    width: usize,
    height: usize,
}

/// Type alias for crop info shared between capture and handler
type CropInfoStore = Arc<Mutex<CropInfo>>;

/// Combined flags passed to the WGC handler
struct WgcFlags {
    frame_tx: watch::Sender<Option<CaptureFrame>>,
    crop_info: CropInfoStore,
}

/// Frame handler that implements GraphicsCaptureApiHandler.
///
/// This runs on a background thread and sends frames through a channel.
struct WgcFrameHandler {
    frame_tx: watch::Sender<Option<CaptureFrame>>,
    crop_info: CropInfoStore,
}

impl WgcFrameHandler {
    fn new(frame_tx: watch::Sender<Option<CaptureFrame>>, crop_info: CropInfoStore) -> Self {
        Self {
            frame_tx,
            crop_info,
        }
    }
}

impl GraphicsCaptureApiHandler for WgcFrameHandler {
    type Flags = WgcFlags;
    type Error = CaptureError;

    fn new(
        ctx: windows_capture::capture::Context<Self::Flags>,
    ) -> std::result::Result<Self, Self::Error> {
        let flags = ctx.flags;
        let frame_tx = flags.frame_tx;
        let crop_info = flags.crop_info.clone();
        Ok(Self::new(frame_tx, crop_info))
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame<'_>,
        capture_control: InternalCaptureControl,
    ) -> std::result::Result<(), Self::Error> {
        let frame_width = frame.width() as usize;
        let frame_height = frame.height() as usize;

        let crop = self
            .crop_info
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        let frame_buffer = frame.buffer().map_err(|e| {
            CaptureError::CaptureFailed(format!("Failed to get frame buffer: {:?}", e))
        })?;

        let color_format = frame_buffer.color_format();
        let mut pixels = Vec::new();
        let copied = frame_buffer.as_nopadding_buffer(&mut pixels);
        if copied.is_empty() {
            return Err(CaptureError::CaptureFailed(
                "Frame copy failed: empty frame view".into(),
            ));
        }
        if pixels.is_empty() {
            return Err(CaptureError::CaptureFailed(
                "Frame copy failed: empty frame buffer".into(),
            ));
        }

        // Apply cropping if needed (crop to client area)
        let crop_in_bounds = crop.left.saturating_add(crop.width) <= frame_width
            && crop.top.saturating_add(crop.height) <= frame_height;
        let need_crop = crop.width > 0
            && crop.height > 0
            && (crop.width < frame_width || crop.height < frame_height);
        let (width, height, pixels) = if need_crop && crop_in_bounds {
            let row_pitch = frame_width * 4;
            let src_ptr = pixels.as_ptr();

            let mut cropped_pixels = Vec::with_capacity(crop.width * crop.height * 4);
            for y in crop.top..(crop.top + crop.height) {
                let row_ptr = unsafe { src_ptr.add(y * row_pitch + crop.left * 4) };
                cropped_pixels.extend_from_slice(unsafe {
                    std::slice::from_raw_parts(row_ptr, crop.width * 4)
                });
            }
            (crop.width as u32, crop.height as u32, cropped_pixels)
        } else {
            (frame_width as u32, frame_height as u32, pixels)
        };

        let pixel_format = match color_format {
            ColorFormat::Bgra8 => PixelFormat::Bgra,
            _ => PixelFormat::Rgba,
        };

        let capture_frame =
            CaptureFrame::new(width, height, pixels, pixel_format, "WGC".to_string());

        if self.frame_tx.is_closed() {
            let _ = capture_control.stop();
            return Ok(());
        }
        self.frame_tx.send_replace(Some(capture_frame));

        Ok(())
    }
}

/// Windows Graphics Capture engine.
///
/// Uses Windows.Graphics.Capture API for hardware-accelerated capture.
/// Supports capturing windows even when obscured.
pub struct WgcCapture {
    state: Arc<Mutex<WgcCaptureState>>,
    frame_receiver: tokio::sync::Mutex<Option<watch::Receiver<Option<CaptureFrame>>>>,
    crop_info_store: CropInfoStore,
    capture_control: Mutex<Option<CaptureControl<WgcFrameHandler, CaptureError>>>,
}

impl WgcCapture {
    /// Create a new WgcCapture instance
    pub fn new() -> Self {
        Self::new_with_fps(0)
    }

    pub fn new_with_fps(fps_cap: u32) -> Self {
        Self {
            state: Arc::new(Mutex::new(WgcCaptureState {
                last_frame: None,
                frame_counter: 0,
                last_latency_ms: None,
                capturing: false,
                start_time: None,
                width: 0,
                height: 0,
                client_left: 0,
                client_top: 0,
                client_width: 0,
                client_height: 0,
                target: None,
                fps_cap,
                options: betternte_core::CaptureRuntimeOptions::default(),
            })),
            frame_receiver: tokio::sync::Mutex::new(None),
            crop_info_store: Arc::new(Mutex::new(CropInfo {
                left: 0,
                top: 0,
                width: 0,
                height: 0,
            })),
            capture_control: Mutex::new(None),
        }
    }

    /// Check if Windows Graphics Capture is supported
    pub fn is_supported() -> bool {
        static WGC_SUPPORTED: OnceLock<bool> = OnceLock::new();
        *WGC_SUPPORTED.get_or_init(|| GraphicsCaptureSession::IsSupported().unwrap_or(false))
    }

    pub fn set_fps_cap(&self, fps_cap: u32) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.fps_cap = fps_cap;
    }

    fn minimum_update_interval(&self) -> MinimumUpdateIntervalSettings {
        let fps_cap = self.state.lock().unwrap_or_else(|e| e.into_inner()).fps_cap;
        if fps_cap == 0 {
            MinimumUpdateIntervalSettings::Default
        } else {
            let frame_ms = (1000u64 / fps_cap.max(1) as u64).max(1);
            MinimumUpdateIntervalSettings::Custom(Duration::from_millis(frame_ms))
        }
    }

    fn refresh_window_crop(&self, hwnd: u64) -> Result<(), CaptureError> {
        let hwnd = HWND(hwnd as *mut _);
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() {
                return Err(CaptureError::WindowNotFound("Invalid HWND".into()));
            }
        }

        let crop_to_client = {
            self.state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .options
                .crop_to_client
        };

        let client_rect = unsafe {
            let mut rect = RECT::default();
            if GetClientRect(hwnd, &mut rect).is_err() {
                return Err(CaptureError::CaptureFailed("GetClientRect failed".into()));
            }
            rect
        };

        let (frame_left, frame_top, client_screen_left, client_screen_top) = unsafe {
            // Use DWM extended frame bounds to exclude invisible DWM shadows
            let mut frame_rect = RECT::default();
            let hr = DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut frame_rect as *mut _ as *mut _, std::mem::size_of::<RECT>() as u32);
            if hr.is_err() {
                // Fallback to GetWindowInfo if DWM fails
                let mut wi = WINDOWINFO {
                    cbSize: std::mem::size_of::<WINDOWINFO>() as u32,
                    ..Default::default()
                };
                if GetWindowInfo(hwnd, &mut wi).is_err() {
                    return Err(CaptureError::CaptureFailed("GetWindowInfo failed".into()));
                }
                frame_rect = wi.rcWindow;
            }

            // Get client screen position
            let mut client_point = windows::Win32::Foundation::POINT { x: 0, y: 0 };
            if !windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut client_point).as_bool() {
                return Err(CaptureError::CaptureFailed("ClientToScreen failed".into()));
            }

            (
                frame_rect.left,
                frame_rect.top,
                client_point.x,
                client_point.y,
            )
        };

        let client_width = (client_rect.right - client_rect.left) as u32;
        let client_height = (client_rect.bottom - client_rect.top) as u32;
        let crop_left = (client_screen_left - frame_left).max(0) as usize;
        let crop_top = (client_screen_top - frame_top).max(0) as usize;

        {
            let mut crop = self
                .crop_info_store
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if crop_to_client {
                crop.left = crop_left;
                crop.top = crop_top;
                crop.width = client_width as usize;
                crop.height = client_height as usize;
            } else {
                crop.left = 0;
                crop.top = 0;
                crop.width = 0;
                crop.height = 0;
            }
        }

        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.width = client_width;
        state.height = client_height;
        state.client_left = client_screen_left;
        state.client_top = client_screen_top;
        state.client_width = client_width;
        state.client_height = client_height;

        Ok(())
    }
}

impl Default for WgcCapture {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScreenCapture for WgcCapture {
    fn name(&self) -> &str {
        "WindowsGraphicsCapture"
    }

    fn is_capturing(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .capturing
    }

    async fn start(&mut self, target: &CaptureTarget) -> AnyhowResult<()> {
        let _ = self.stop().await;
        if !Self::is_supported() {
            return Err(CaptureError::WgcNotSupported.into());
        }

        let (frame_tx, frame_rx) = watch::channel::<Option<CaptureFrame>>(None);

        match target {
            CaptureTarget::Window { hwnd } => {
                let hwnd_val = *hwnd;
                let hwnd = HWND(hwnd_val as *mut _);

                unsafe {
                    if !IsWindow(Some(hwnd)).as_bool() {
                        return Err(CaptureError::WindowNotFound("Invalid HWND".into()).into());
                    }
                }

                let client_rect = unsafe {
                    let mut rect = RECT::default();
                    if GetClientRect(hwnd, &mut rect).is_err() {
                        return Err(CaptureError::InitFailed("GetClientRect failed".into()).into());
                    }
                    rect
                };

                let (frame_left, frame_top, client_screen_left, client_screen_top) = unsafe {
                    // Use DWM extended frame bounds to exclude invisible DWM shadows
                    let mut frame_rect = RECT::default();
                    let hr = DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut frame_rect as *mut _ as *mut _, std::mem::size_of::<RECT>() as u32);
                    if hr.is_err() {
                        // Fallback to GetWindowInfo if DWM fails
                        let mut wi = WINDOWINFO {
                            cbSize: std::mem::size_of::<WINDOWINFO>() as u32,
                            ..Default::default()
                        };
                        if GetWindowInfo(hwnd, &mut wi).is_err() {
                            return Err(CaptureError::InitFailed("GetWindowInfo failed".into()).into());
                        }
                        frame_rect = wi.rcWindow;
                    }

                    // Get client screen position
                    let mut client_point = windows::Win32::Foundation::POINT { x: 0, y: 0 };
                    if !windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut client_point).as_bool() {
                        return Err(CaptureError::InitFailed("ClientToScreen failed".into()).into());
                    }

                    (
                        frame_rect.left,
                        frame_rect.top,
                        client_point.x,
                        client_point.y,
                    )
                };

                let crop_left = (client_screen_left - frame_left) as usize;
                let crop_top = (client_screen_top - frame_top) as usize;

                let client_width = (client_rect.right - client_rect.left) as u32;
                let client_height = (client_rect.bottom - client_rect.top) as u32;

                {
                    let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    state.width = client_width;
                    state.height = client_height;
                    state.client_left = client_screen_left;
                    state.client_top = client_screen_top;
                    state.client_width = client_width;
                    state.client_height = client_height;
                    state.start_time = Some(Instant::now());
                    state.capturing = true;
                    state.target = Some(CaptureTarget::Window { hwnd: hwnd_val });
                }

                {
                    let mut crop = self
                        .crop_info_store
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    let crop_to_client = self
                        .state
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .options
                        .crop_to_client;
                    if crop_to_client {
                        crop.left = crop_left;
                        crop.top = crop_top;
                        crop.width = client_width as usize;
                        crop.height = client_height as usize;
                    } else {
                        crop.left = 0;
                        crop.top = 0;
                        crop.width = 0;
                        crop.height = 0;
                    }
                }

                let min_interval = self.minimum_update_interval();
                let window = Window::from_raw_hwnd(hwnd.0 as *mut _);

                // Try WithoutBorder first, fall back to WithBorder if unsupported
                let flags = WgcFlags {
                    frame_tx: frame_tx.clone(),
                    crop_info: self.crop_info_store.clone(),
                };
                let settings_no_border = Settings::new(
                    window,
                    windows_capture::settings::CursorCaptureSettings::WithoutCursor,
                    windows_capture::settings::DrawBorderSettings::WithoutBorder,
                    windows_capture::settings::SecondaryWindowSettings::Default,
                    min_interval,
                    windows_capture::settings::DirtyRegionSettings::Default,
                    ColorFormat::Bgra8,
                    flags,
                );

                let capture_control = match WgcFrameHandler::start_free_threaded(settings_no_border) {
                    Ok(ctrl) => ctrl,
                    Err(e) => {
                        let err_str = format!("{:?}", e);
                        if err_str.contains("BorderConfig") {
                            tracing::warn!("WithoutBorder unsupported, falling back to WithBorder");
                            let window2 = Window::from_raw_hwnd(hwnd.0 as *mut _);
                            let flags2 = WgcFlags {
                                frame_tx: frame_tx.clone(),
                                crop_info: self.crop_info_store.clone(),
                            };
                            let settings_with_border = Settings::new(
                                window2,
                                windows_capture::settings::CursorCaptureSettings::WithoutCursor,
                                windows_capture::settings::DrawBorderSettings::WithBorder,
                                windows_capture::settings::SecondaryWindowSettings::Default,
                                min_interval,
                                windows_capture::settings::DirtyRegionSettings::Default,
                                ColorFormat::Bgra8,
                                flags2,
                            );
                            WgcFrameHandler::start_free_threaded(settings_with_border)
                                .map_err(|e2| CaptureError::InitFailed(format!("WGC start failed: {:?}", e2)))?
                        } else {
                            return Err(CaptureError::InitFailed(format!("WGC start failed: {:?}", e)).into());
                        }
                    }
                };

                *self.frame_receiver.lock().await = Some(frame_rx);
                *self
                    .capture_control
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = Some(capture_control);

                Ok(())
            }
            CaptureTarget::Display { index } => {
                let monitor = windows_capture::monitor::Monitor::from_index(*index as usize)
                    .map_err(|_| {
                        CaptureError::InitFailed(format!("Monitor {} not found", index))
                    })?;

                let (width, height) = match (monitor.width(), monitor.height()) {
                    (Ok(w), Ok(h)) => (w as u32, h as u32),
                    _ => (1920, 1080),
                };

                {
                    let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    state.width = width;
                    state.height = height;
                    state.start_time = Some(Instant::now());
                    state.capturing = true;
                    state.target = Some(CaptureTarget::Display { index: *index });
                }

                {
                    let mut crop = self
                        .crop_info_store
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    crop.left = 0;
                    crop.top = 0;
                    crop.width = 0;
                    crop.height = 0;
                }

                let min_interval = self.minimum_update_interval();

                // Try WithoutBorder first, fall back to WithBorder if unsupported
                let flags = WgcFlags {
                    frame_tx: frame_tx.clone(),
                    crop_info: self.crop_info_store.clone(),
                };
                let settings_no_border = Settings::new(
                    monitor,
                    windows_capture::settings::CursorCaptureSettings::WithoutCursor,
                    windows_capture::settings::DrawBorderSettings::WithoutBorder,
                    windows_capture::settings::SecondaryWindowSettings::Default,
                    min_interval,
                    windows_capture::settings::DirtyRegionSettings::Default,
                    ColorFormat::Bgra8,
                    flags,
                );

                let capture_control = match WgcFrameHandler::start_free_threaded(settings_no_border) {
                    Ok(ctrl) => ctrl,
                    Err(e) => {
                        let err_str = format!("{:?}", e);
                        if err_str.contains("BorderConfig") {
                            tracing::warn!("WithoutBorder unsupported, falling back to WithBorder");
                            let monitor2 = windows_capture::monitor::Monitor::from_index(*index as usize)
                                .map_err(|_| CaptureError::InitFailed(format!("Monitor {} not found", index)))?;
                            let flags2 = WgcFlags {
                                frame_tx: frame_tx.clone(),
                                crop_info: self.crop_info_store.clone(),
                            };
                            let settings_with_border = Settings::new(
                                monitor2,
                                windows_capture::settings::CursorCaptureSettings::WithoutCursor,
                                windows_capture::settings::DrawBorderSettings::WithBorder,
                                windows_capture::settings::SecondaryWindowSettings::Default,
                                min_interval,
                                windows_capture::settings::DirtyRegionSettings::Default,
                                ColorFormat::Bgra8,
                                flags2,
                            );
                            WgcFrameHandler::start_free_threaded(settings_with_border)
                                .map_err(|e2| CaptureError::InitFailed(format!("WGC start failed: {:?}", e2)))?
                        } else {
                            return Err(CaptureError::InitFailed(format!("WGC start failed: {:?}", e)).into());
                        }
                    }
                };

                *self.frame_receiver.lock().await = Some(frame_rx);
                *self
                    .capture_control
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = Some(capture_control);

                Ok(())
            }
            _ => Err(CaptureError::UnsupportedTarget(
                "WGC does not support this target type".into(),
            )
            .into()),
        }
    }

    async fn capture(&self) -> AnyhowResult<CaptureFrame> {
        if !self.is_capturing() {
            return Err(CaptureError::InitFailed("Capture not started".into()).into());
        }

        let target = {
            self.state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .target
                .clone()
        };
        if let Some(CaptureTarget::Window { hwnd }) = target {
            self.refresh_window_crop(hwnd)?;
        }

        let mut receiver_guard = self.frame_receiver.lock().await;

        let start = Instant::now();
        let frame = match receiver_guard.as_mut() {
            Some(receiver) => {
                if receiver.borrow().is_none() {
                    receiver
                        .changed()
                        .await
                        .map_err(|_| CaptureError::CaptureFailed("Channel closed".into()))?;
                }
                receiver
                    .borrow_and_update()
                    .clone()
                    .ok_or_else(|| CaptureError::CaptureFailed("No frame available".into()))?
            }
            None => {
                drop(receiver_guard);
                let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(frame) = state.last_frame.clone() {
                    return Ok(frame);
                }
                return Err(CaptureError::CaptureFailed("No frame available".into()).into());
            }
        };

        let elapsed = start.elapsed();

        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.last_latency_ms = Some(elapsed.as_secs_f64() * 1000.0);
            state.frame_counter += 1;
            state.last_frame = Some(frame.clone());
        }

        Ok(frame)
    }

    async fn stop(&mut self) -> AnyhowResult<()> {
        *self.frame_receiver.lock().await = None;
        *self
            .capture_control
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;

        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.capturing = false;
            state.start_time = None;
            state.target = None;
        }

        Ok(())
    }

    fn last_latency_ms(&self) -> Option<f64> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .last_latency_ms
    }

    fn fps(&self) -> f64 {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(start) = state.start_time {
            let elapsed = start.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                return state.frame_counter as f64 / elapsed;
            }
        }
        0.0
    }

    fn configure(&self, options: betternte_core::CaptureRuntimeOptions) {
        if !options.recover_on_resize || !options.recover_on_monitor_switch {
            tracing::info!(
                recover_on_resize = options.recover_on_resize,
                recover_on_monitor_switch = options.recover_on_monitor_switch,
                "WGC currently applies passive recovery; explicit recovery toggles are best-effort"
            );
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.options = options;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_wgc_is_supported() {
        let _ = super::WgcCapture::is_supported();
    }
}
