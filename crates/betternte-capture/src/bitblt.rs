//! BitBlt screenshot engine.
//!
//! Uses GDI BitBlt for maximum compatibility.
//! Based on win-screenshot's implementation with improvements:
//! - SetProcessDpiAwareness for correct high-DPI dimensions
//! - GetDIBits for reliable pixel format (top-down BGRA)

use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use async_trait::async_trait;
use betternte_core::{CaptureFrame, PixelFormat};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC, HGDIOBJ,
    SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetDesktopWindow, IsIconic, IsWindow,
};

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

/// BitBlt screenshot engine.
///
/// Uses Windows GDI BitBlt for screenshots. Most compatible but cannot
/// capture when window is obscured.
pub struct BitBltCapture {
    hwnd: Option<SafeHwnd>,
    hdc_window: Option<SafeHdc>,
    width: u32,
    height: u32,
    frame_counter: AtomicU64,
    capturing: AtomicBool,
    last_latency: Mutex<Option<f64>>,
    start_time: Mutex<Option<Instant>>,
}

impl BitBltCapture {
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

    fn capture_frame_impl(&self) -> Result<CaptureFrame> {
        let hwnd = self
            .hwnd
            .ok_or(CaptureError::InitFailed("No window bound".into()))?;
        let hdc_window = self
            .hdc_window
            .ok_or(CaptureError::InitFailed("No window DC".into()))?;

        let (width, height) = self.get_client_size()?;

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

            // BitBlt from window DC to memory DC
            let ok = BitBlt(
                hdc_mem,
                0,
                0,
                width as i32,
                height as i32,
                Some(hdc_window.to_hdc()),
                0,
                0,
                SRCCOPY,
            );
            if ok.is_err() {
                let _ = SelectObject(hdc_mem, old_obj);
                let _ = DeleteObject(HGDIOBJ(hbmp.0));
                let _ = DeleteDC(hdc_mem);
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

            let frame = CaptureFrame::new(
                width,
                height,
                pixels,
                PixelFormat::Bgra,
                "BitBlt".to_string(),
            );

            Ok(frame)
        }
    }
}

impl Default for BitBltCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for BitBltCapture {
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
impl ScreenCapture for BitBltCapture {
    fn name(&self) -> &str {
        "BitBlt"
    }

    fn is_capturing(&self) -> bool {
        self.capturing.load(Ordering::SeqCst)
    }

    async fn start(&mut self, target: &CaptureTarget) -> AnyhowResult<()> {
        let _ = self.stop().await;
        self.ensure_dpi_aware();

        let (hwnd, is_desktop) = match target {
            CaptureTarget::Window { hwnd } => {
                if *hwnd == 0 {
                    (unsafe { GetDesktopWindow() }, true)
                } else {
                    (HWND(*hwnd as *mut _), false)
                }
            }
            _ => {
                return Err(CaptureError::UnsupportedTarget(
                    "BitBlt only supports window capture".into(),
                )
                .into())
            }
        };

        if !is_desktop {
            if let Err(e) = crate::win32_helper::auto_fix_win11_bitblt() {
                tracing::warn!("Failed to apply Win11 BitBlt fix: {}", e);
            }
        }

        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() {
                return Err(CaptureError::WindowNotFound("Invalid HWND".into()).into());
            }

            if !is_desktop && IsIconic(hwnd).as_bool() {
                return Err(CaptureError::WindowMinimized.into());
            }

            let hdc_window = GetDC(Some(hwnd));
            if hdc_window.is_invalid() {
                return Err(CaptureError::InitFailed("GetDC failed".into()).into());
            }

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bitblt_invalid_window() {
        let mut capture = BitBltCapture::new();
        let result = capture.start(&CaptureTarget::Window { hwnd: 999999 }).await;
        assert!(result.is_err());
    }
}
