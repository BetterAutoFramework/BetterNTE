//! Windows-specific utilities

/// DPI scale information
#[derive(Debug, Clone, Copy)]
pub struct DpiScale {
    pub x: f32,
    pub y: f32,
}

impl DpiScale {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn uniform(scale: f32) -> Self {
        Self::new(scale, scale)
    }

    /// Get reciprocal scale
    pub fn inverse(&self) -> Self {
        Self::new(1.0 / self.x, 1.0 / self.y)
    }
}

impl Default for DpiScale {
    fn default() -> Self {
        Self::uniform(1.0)
    }
}

#[cfg(windows)]
pub mod windows_impl {
    /// Get system DPI scale
    pub fn get_system_dpi() -> super::DpiScale {
        unsafe {
            use windows_sys::Win32::Graphics::Gdi::{GetDC, GetDeviceCaps, ReleaseDC};
            use windows_sys::Win32::Graphics::Gdi::{LOGPIXELSX, LOGPIXELSY};

            let hwnd = std::ptr::null_mut();
            let hdc = GetDC(hwnd);
            if hdc.is_null() {
                return super::DpiScale::default();
            }

            let dpi_x = GetDeviceCaps(hdc, LOGPIXELSX as i32);
            let dpi_y = GetDeviceCaps(hdc, LOGPIXELSY as i32);
            ReleaseDC(hwnd, hdc);

            super::DpiScale::new(dpi_x as f32 / 96.0, dpi_y as f32 / 96.0)
        }
    }

    /// Get DPI scale for a window handle (simplified)
    pub fn get_window_dpi(_hwnd: isize) -> super::DpiScale {
        get_system_dpi()
    }

    /// GetForegroundWindow equivalent
    pub fn get_foreground_window() -> Option<isize> {
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                None
            } else {
                Some(hwnd as isize)
            }
        }
    }
}

#[cfg(not(windows))]
pub mod windows_impl {
    pub fn get_system_dpi() -> super::DpiScale {
        super::DpiScale::default()
    }

    pub fn get_window_dpi(_hwnd: isize) -> super::DpiScale {
        super::DpiScale::default()
    }

    pub fn get_foreground_window() -> Option<isize> {
        None
    }
}

pub use windows_impl::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dpi_scale_new() {
        let dpi = DpiScale::new(1.5, 2.0);
        assert!((dpi.x - 1.5).abs() < 1e-6);
        assert!((dpi.y - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_dpi_scale_uniform() {
        let dpi = DpiScale::uniform(2.0);
        assert!((dpi.x - 2.0).abs() < 1e-6);
        assert!((dpi.y - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_dpi_scale_inverse() {
        let dpi = DpiScale::new(2.0, 4.0);
        let inv = dpi.inverse();
        assert!((inv.x - 0.5).abs() < 1e-6);
        assert!((inv.y - 0.25).abs() < 1e-6);
    }

    #[test]
    fn test_dpi_scale_default() {
        let dpi = DpiScale::default();
        assert!((dpi.x - 1.0).abs() < 1e-6);
        assert!((dpi.y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_get_system_dpi_returns_valid() {
        let dpi = get_system_dpi();
        // On any platform, DPI values should be positive
        assert!(dpi.x > 0.0);
        assert!(dpi.y > 0.0);
    }
}
