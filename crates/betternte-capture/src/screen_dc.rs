//! Screen DC screenshot engine.
//!
//! Uses GetDC(nullptr) to get the entire screen DC, then BitBlt the window's
//! client area.

use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use async_trait::async_trait;
use betternte_core::{CaptureFrame, PixelFormat};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HGDIOBJ, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowRect, IsIconic, IsWindow};

use crate::error::{CaptureError, Result};
use crate::{CaptureTarget, ScreenCapture};
use anyhow::Result as AnyhowResult;

#[derive(Debug, Clone, Copy)]
struct SafeHwnd(u64);

unsafe impl Send for SafeHwnd {}
unsafe impl Sync for SafeHwnd {}

impl SafeHwnd {
    fn to_hwnd(&self) -> HWND {
        HWND(self.0 as *mut _)
    }

    fn from_hwnd(hwnd: HWND) -> Self {
        Self(hwnd.0 as u64)
    }
}

/// ScreenDC screenshot engine.
pub struct ScreenDCCapture {
    hwnd: Option<SafeHwnd>,
    width: u32,
    height: u32,
    frame_counter: AtomicU64,
    capturing: AtomicBool,
    last_latency: Mutex<Option<f64>>,
    start_time: Mutex<Option<Instant>>,
    options: Mutex<betternte_core::CaptureRuntimeOptions>,
}

impl ScreenDCCapture {
    pub fn new() -> Self {
        Self {
            hwnd: None,
            width: 0,
            height: 0,
            frame_counter: AtomicU64::new(0),
            capturing: AtomicBool::new(false),
            last_latency: Mutex::new(None),
            start_time: Mutex::new(None),
            options: Mutex::new(betternte_core::CaptureRuntimeOptions::default()),
        }
    }

    fn capture_frame_impl(&self) -> Result<CaptureFrame> {
        let hwnd = self
            .hwnd
            .ok_or(CaptureError::InitFailed("No window bound".into()))?;

        unsafe {
            if !IsWindow(Some(hwnd.to_hwnd())).as_bool() {
                return Err(CaptureError::WindowNotFound("Invalid HWND".into()));
            }
            if IsIconic(hwnd.to_hwnd()).as_bool() {
                return Err(CaptureError::WindowMinimized);
            }

            let crop_to_client = self
                .options
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .crop_to_client;

            let (width, height, src_left, src_top) = if crop_to_client {
                let mut client_rect = RECT::default();
                let _ = GetClientRect(hwnd.to_hwnd(), &mut client_rect);
                let width = (client_rect.right - client_rect.left) as u32;
                let height = (client_rect.bottom - client_rect.top) as u32;
                let mut client_top_left = POINT {
                    x: client_rect.left,
                    y: client_rect.top,
                };
                let _ = ClientToScreen(hwnd.to_hwnd(), &mut client_top_left);
                (width, height, client_top_left.x, client_top_left.y)
            } else {
                let mut window_rect = RECT::default();
                let _ = GetWindowRect(hwnd.to_hwnd(), &mut window_rect);
                (
                    (window_rect.right - window_rect.left) as u32,
                    (window_rect.bottom - window_rect.top) as u32,
                    window_rect.left,
                    window_rect.top,
                )
            };

            if width == 0 || height == 0 {
                return Err(CaptureError::CaptureFailed("Invalid window size".into()));
            }

            let screen_dc = GetDC(None);
            if screen_dc.is_invalid() {
                return Err(CaptureError::CaptureFailed("GetDC failed".into()));
            }

            let mem_dc = CreateCompatibleDC(Some(screen_dc));
            if mem_dc.is_invalid() {
                let _ = ReleaseDC(None, screen_dc);
                return Err(CaptureError::CaptureFailed(
                    "CreateCompatibleDC failed".into(),
                ));
            }

            let bitmap = CreateCompatibleBitmap(screen_dc, width as i32, height as i32);
            if bitmap.is_invalid() {
                let _ = DeleteDC(mem_dc);
                let _ = ReleaseDC(None, screen_dc);
                return Err(CaptureError::CaptureFailed(
                    "CreateCompatibleBitmap failed".into(),
                ));
            }

            let old_obj = SelectObject(mem_dc, HGDIOBJ(bitmap.0));
            if old_obj.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                let _ = DeleteDC(mem_dc);
                let _ = ReleaseDC(None, screen_dc);
                return Err(CaptureError::CaptureFailed("SelectObject failed".into()));
            }

            let ok = BitBlt(
                mem_dc,
                0,
                0,
                width as i32,
                height as i32,
                Some(screen_dc),
                src_left,
                src_top,
                SRCCOPY,
            );
            if ok.is_err() {
                let _ = SelectObject(mem_dc, old_obj);
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                let _ = DeleteDC(mem_dc);
                let _ = ReleaseDC(None, screen_dc);
                return Err(CaptureError::CaptureFailed("BitBlt failed".into()));
            }

            // Get pixel data using GetDIBits (reliable format, top-down)
            let bmih = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biPlanes: 1,
                biBitCount: 32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            };
            let mut bmi = BITMAPINFO {
                bmiHeader: bmih,
                ..Default::default()
            };

            let pixel_len = (width * height * 4) as usize;
            let mut pixels: Vec<u8> = Vec::with_capacity(pixel_len);
            pixels.set_len(pixel_len);
            let gdb = GetDIBits(
                mem_dc,
                bitmap,
                0,
                height,
                Some(pixels.as_mut_ptr() as *mut core::ffi::c_void),
                &mut bmi,
                DIB_RGB_COLORS,
            );

            let _ = SelectObject(mem_dc, old_obj);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);

            if gdb <= 0 || gdb as u32 != height {
                return Err(CaptureError::CaptureFailed("GetDIBits failed".into()));
            }

            let frame = CaptureFrame::new(
                width,
                height,
                pixels,
                PixelFormat::Bgra,
                "ScreenDC".to_string(),
            );

            Ok(frame)
        }
    }
}

impl Default for ScreenDCCapture {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScreenCapture for ScreenDCCapture {
    fn name(&self) -> &str {
        "ScreenDC"
    }

    fn is_capturing(&self) -> bool {
        self.capturing.load(Ordering::SeqCst)
    }

    async fn start(&mut self, target: &CaptureTarget) -> AnyhowResult<()> {
        let _ = self.stop().await;
        let hwnd = match target {
            CaptureTarget::Window { hwnd } => {
                if *hwnd == 0 {
                    return Err(CaptureError::UnsupportedTarget(
                        "ScreenDC does not support full screen capture".into(),
                    )
                    .into());
                }
                HWND(*hwnd as *mut _)
            }
            _ => {
                return Err(CaptureError::UnsupportedTarget(
                    "ScreenDC only supports window capture".into(),
                )
                .into())
            }
        };

        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() {
                return Err(CaptureError::WindowNotFound("Invalid HWND".into()).into());
            }

            if IsIconic(hwnd).as_bool() {
                return Err(CaptureError::WindowMinimized.into());
            }

            let mut client_rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut client_rect);
            let width = (client_rect.right - client_rect.left) as u32;
            let height = (client_rect.bottom - client_rect.top) as u32;

            if width == 0 || height == 0 {
                return Err(CaptureError::InitFailed("Window has zero size".into()).into());
            }

            self.hwnd = Some(SafeHwnd::from_hwnd(hwnd));
            self.width = width;
            self.height = height;
            self.capturing.store(true, Ordering::SeqCst);
            *self.start_time.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());

            Ok(())
        }
    }

    async fn capture(&self) -> AnyhowResult<CaptureFrame> {
        if !self.is_capturing() {
            return Err(CaptureError::InitFailed("Capture not started".into()).into());
        }

        let start = Instant::now();
        let frame = self.capture_frame_impl()?;
        let elapsed = start.elapsed();

        *self.last_latency.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(elapsed.as_secs_f64() * 1000.0);
        self.frame_counter.fetch_add(1, Ordering::SeqCst);

        Ok(frame)
    }

    async fn stop(&mut self) -> AnyhowResult<()> {
        self.hwnd = None;
        self.capturing.store(false, Ordering::SeqCst);
        *self.start_time.lock().unwrap_or_else(|e| e.into_inner()) = None;

        Ok(())
    }

    fn last_latency_ms(&self) -> Option<f64> {
        *self.last_latency.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn fps(&self) -> f64 {
        let start = self.start_time.lock().unwrap_or_else(|e| e.into_inner());
        let counter = self.frame_counter.load(Ordering::SeqCst);
        if let Some(start) = *start {
            let elapsed = start.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                return counter as f64 / elapsed;
            }
        }
        0.0
    }

    fn configure(&self, options: betternte_core::CaptureRuntimeOptions) {
        if options.hdr_to_sdr {
            tracing::info!("HDR to SDR policy is not supported by ScreenDC backend, ignoring");
        }
        *self.options.lock().unwrap_or_else(|e| e.into_inner()) = options;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_screendc_invalid_window() {
        let mut capture = ScreenDCCapture::new();
        let result = capture.start(&CaptureTarget::Window { hwnd: 999999 }).await;
        assert!(result.is_err());
    }
}
