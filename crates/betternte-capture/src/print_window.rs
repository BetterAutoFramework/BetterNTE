//! PrintWindow screenshot engine.
//!
//! Uses Win32 PrintWindow API to capture windows even when obscured.
//! Based on win-screenshot's implementation with key improvements:
//! - SetProcessDpiAwareness for correct high-DPI dimensions
//! - PW_CLIENTONLY | PW_RENDERFULLCONTENT for DWM-composited content
//! - GetDIBits for reliable pixel format

use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use async_trait::async_trait;
use betternte_core::{CaptureFrame, PixelFormat};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC, HGDIOBJ,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS, PW_CLIENTONLY};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, IsIconic, IsWindow, PW_RENDERFULLCONTENT,
};

use anyhow::Result as AnyhowResult;

use crate::error::{CaptureError, Result};
use crate::{CaptureTarget, ScreenCapture};

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

#[derive(Debug, Clone, Copy)]
struct SafeHdc(u64);

unsafe impl Send for SafeHdc {}
unsafe impl Sync for SafeHdc {}

impl SafeHdc {
    fn to_hdc(&self) -> HDC {
        HDC(self.0 as *mut _)
    }

    fn from_hdc(hdc: HDC) -> Self {
        Self(hdc.0 as u64)
    }
}

/// PrintWindow screenshot engine.
///
/// Uses Win32 PrintWindow API. Can capture windows even when they are
/// partially or fully obscured by other windows (unlike BitBlt).
///
/// Key differences from win-screenshot's implementation:
/// - Uses PW_CLIENTONLY | PW_RENDERFULLCONTENT for correct DWM rendering
/// - Uses GetDIBits for reliable pixel format (top-down BGRA)
/// - Sets DPI awareness for correct high-DPI dimensions
pub struct PrintWindowCapture {
    hwnd: Option<SafeHwnd>,
    hdc_window: Option<SafeHdc>,
    width: u32,
    height: u32,
    frame_counter: AtomicU64,
    capturing: AtomicBool,
    last_latency: Mutex<Option<f64>>,
    start_time: Mutex<Option<Instant>>,
    options: Mutex<betternte_core::CaptureRuntimeOptions>,
}

impl PrintWindowCapture {
    pub fn new() -> Self {
        Self {
            hwnd: None,
            hdc_window: None,
            width: 0,
            height: 0,
            frame_counter: AtomicU64::new(0),
            capturing: AtomicBool::new(false),
            last_latency: Mutex::new(None),
            start_time: Mutex::new(None),
            options: Mutex::new(betternte_core::CaptureRuntimeOptions::default()),
        }
    }

    fn ensure_dpi_aware(&self) {
        let _ = crate::ensure_process_dpi_aware();
    }

    fn get_client_size(&self) -> Result<(u32, u32)> {
        let hwnd = self
            .hwnd
            .ok_or(CaptureError::InitFailed("No window bound".into()))?;
        unsafe {
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd.to_hwnd(), &mut rect);
            let width = (rect.right - rect.left) as u32;
            let height = (rect.bottom - rect.top) as u32;
            if width == 0 || height == 0 {
                return Err(CaptureError::CaptureFailed("Invalid window size".into()));
            }
            Ok((width, height))
        }
    }

    fn get_capture_size(&self) -> Result<(u32, u32)> {
        let crop_to_client = self
            .options
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .crop_to_client;
        if crop_to_client {
            return self.get_client_size();
        }
        let hwnd = self
            .hwnd
            .ok_or(CaptureError::InitFailed("No window bound".into()))?;
        unsafe {
            let mut rect = RECT::default();
            if windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd.to_hwnd(), &mut rect)
                .is_err()
            {
                return Err(CaptureError::CaptureFailed("GetWindowRect failed".into()));
            }
            let width = (rect.right - rect.left) as u32;
            let height = (rect.bottom - rect.top) as u32;
            if width == 0 || height == 0 {
                return Err(CaptureError::CaptureFailed("Invalid window size".into()));
            }
            Ok((width, height))
        }
    }

    fn capture_frame_impl(&self) -> Result<CaptureFrame> {
        let hwnd = self
            .hwnd
            .ok_or(CaptureError::InitFailed("No window bound".into()))?;
        let hdc_window = self
            .hdc_window
            .ok_or(CaptureError::InitFailed("No window DC".into()))?;

        let (width, height) = self.get_capture_size()?;

        unsafe {
            if !IsWindow(Some(hwnd.to_hwnd())).as_bool() {
                return Err(CaptureError::WindowNotFound("Invalid HWND".into()));
            }
            if IsIconic(hwnd.to_hwnd()).as_bool() {
                return Err(CaptureError::WindowMinimized);
            }

            // Create compatible DC
            let hdc_mem = CreateCompatibleDC(Some(hdc_window.to_hdc()));
            if hdc_mem.is_invalid() {
                return Err(CaptureError::CaptureFailed(
                    "CreateCompatibleDC failed".into(),
                ));
            }

            // Create compatible bitmap
            let hbmp = CreateCompatibleBitmap(hdc_window.to_hdc(), width as i32, height as i32);
            if hbmp.is_invalid() {
                let _ = DeleteDC(hdc_mem);
                return Err(CaptureError::CaptureFailed(
                    "CreateCompatibleBitmap failed".into(),
                ));
            }

            // Select bitmap into memory DC
            let old_obj = SelectObject(hdc_mem, HGDIOBJ(hbmp.0));
            if old_obj.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(hbmp.0));
                let _ = DeleteDC(hdc_mem);
                return Err(CaptureError::CaptureFailed("SelectObject failed".into()));
            }

            // PrintWindow: renders the window content into our memory DC
            // PW_CLIENTONLY = only client area
            // PW_RENDERFULLCONTENT = ensure DWM renders full content
            let crop_to_client = self
                .options
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .crop_to_client;
            let flags = if crop_to_client {
                PRINT_WINDOW_FLAGS(PW_CLIENTONLY.0 | PW_RENDERFULLCONTENT)
            } else {
                PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)
            };
            if PrintWindow(hwnd.to_hwnd(), hdc_mem, flags) == false {
                let _ = SelectObject(hdc_mem, old_obj);
                let _ = DeleteObject(HGDIOBJ(hbmp.0));
                let _ = DeleteDC(hdc_mem);
                return Err(CaptureError::CaptureFailed("PrintWindow failed".into()));
            }

            // Get pixel data using GetDIBits (more reliable than GetBitmapBits)
            // biHeight negative = top-down DIB (correct scanline order)
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
            // GetDIBits writes raw bytes directly into this buffer.
            pixels.set_len(pixel_len);
            let gdb = GetDIBits(
                hdc_mem,
                hbmp,
                0,
                height,
                Some(pixels.as_mut_ptr() as *mut core::ffi::c_void),
                &mut bmi,
                DIB_RGB_COLORS,
            );

            // Clean up
            let _ = SelectObject(hdc_mem, old_obj);
            let _ = DeleteObject(HGDIOBJ(hbmp.0));
            let _ = DeleteDC(hdc_mem);

            if gdb <= 0 || gdb as u32 != height {
                return Err(CaptureError::CaptureFailed("GetDIBits failed".into()));
            }

            // GetDIBits returns BGR (Blue at [0]), swap to BGRA is already correct
            // since we requested 32-bit with BI_RGB — it's actually BGRX (B,G,R,0)
            // No swap needed, BGRA format is what we want

            let frame = CaptureFrame::new(
                width,
                height,
                pixels,
                PixelFormat::Bgra,
                "PrintWindow".to_string(),
            );

            Ok(frame)
        }
    }
}

impl Default for PrintWindowCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PrintWindowCapture {
    fn drop(&mut self) {
        unsafe {
            if let Some(hdc_window) = self.hdc_window {
                if let Some(hwnd) = self.hwnd {
                    let _ = ReleaseDC(Some(hwnd.to_hwnd()), hdc_window.to_hdc());
                }
            }
        }
    }
}

#[async_trait]
impl ScreenCapture for PrintWindowCapture {
    fn name(&self) -> &str {
        "PrintWindow"
    }

    fn is_capturing(&self) -> bool {
        self.capturing.load(Ordering::SeqCst)
    }

    async fn start(&mut self, target: &CaptureTarget) -> AnyhowResult<()> {
        let _ = self.stop().await;
        self.ensure_dpi_aware();

        let hwnd = match target {
            CaptureTarget::Window { hwnd } => {
                if *hwnd == 0 {
                    return Err(CaptureError::UnsupportedTarget(
                        "PrintWindow does not support full screen capture".into(),
                    )
                    .into());
                }
                HWND(*hwnd as *mut _)
            }
            _ => {
                return Err(CaptureError::UnsupportedTarget(
                    "PrintWindow only supports window capture".into(),
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

            // Get window DC
            let hdc_window = GetDC(Some(hwnd));
            if hdc_window.is_invalid() {
                return Err(CaptureError::InitFailed("GetDC failed".into()).into());
            }

            // Get client rect for dimensions
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            let width = (rect.right - rect.left) as u32;
            let height = (rect.bottom - rect.top) as u32;

            if width == 0 || height == 0 {
                let _ = ReleaseDC(Some(hwnd), hdc_window);
                return Err(CaptureError::InitFailed("Window has zero size".into()).into());
            }

            self.hwnd = Some(SafeHwnd::from_hwnd(hwnd));
            self.hdc_window = Some(SafeHdc::from_hdc(hdc_window));
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
        unsafe {
            if let Some(hdc_window) = self.hdc_window {
                if let Some(hwnd) = self.hwnd {
                    let _ = ReleaseDC(Some(hwnd.to_hwnd()), hdc_window.to_hdc());
                }
            }
        }

        self.hwnd = None;
        self.hdc_window = None;
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
            tracing::info!("HDR to SDR policy is not supported by PrintWindow backend, ignoring");
        }
        *self.options.lock().unwrap_or_else(|e| e.into_inner()) = options;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_print_window_invalid_window() {
        let mut capture = PrintWindowCapture::new();
        let result = capture.start(&CaptureTarget::Window { hwnd: 999999 }).await;
        assert!(result.is_err());
    }
}
