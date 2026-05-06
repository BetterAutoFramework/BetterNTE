//! betternte-capture: Screen capture engines for Windows games.
//!
//! Self-implemented engines, no third-party capture libraries:
//! - **WGC**: Windows Graphics Capture via windows-capture crate (GPU, persistent session)
//! - **DxgiDup**: DXGI Desktop Duplication (GPU, desktop-level capture)
//! - **PrintWindow**: GDI PrintWindow (captures obscured windows via WM_PRINT)
//! - **ScreenDC**: Screen DC + BitBlt (GDI, captures even when obscured)
//! - **BitBlt**: GDI BitBlt (most compatible, cannot capture when obscured)
//!
//! # Example
//!
//! ```ignore
//! use betternte_capture::{WgcCapture, WindowFinderImpl, CaptureTarget};
//! use betternte_capture::ScreenCapture;
//!
//! let finder = WindowFinderImpl::new();
//! let windows = finder.find_by_keyword("Game").unwrap();
//!
//! let mut capture = WgcCapture::new();
//! capture.start(&CaptureTarget::Window { hwnd: windows[0].hwnd }).await?;
//! let frame = capture.capture().await?;
//! capture.stop().await?;
//! ```

#![cfg(windows)]

use std::sync::OnceLock;

pub mod bitblt;
pub mod buffer;
pub mod dxgi_dup;
pub mod error;
pub mod factory;
pub mod print_window;
pub mod screen_dc;
pub mod wgc;
pub mod win32_helper;
pub mod window;

// Re-exports
pub use bitblt::BitBltCapture;
pub use buffer::FrameRingBuffer;
pub use dxgi_dup::DxgiDupCapture;
pub use error::{CaptureError, Result};
pub use factory::{
    available_capture_methods, create_capture_engine, create_capture_engine_with_fps,
    create_capture_engine_with_fps_for_target, create_frame_buffer, resolve_auto_capture_method,
};
pub use print_window::PrintWindowCapture;
pub use screen_dc::ScreenDCCapture;
pub use wgc::WgcCapture;
pub use win32_helper::{
    auto_fix_win11_bitblt, disable_win11_window_optimization, is_windows11_or_greater,
};
pub use window::{WindowFinderImpl, WindowInfo};

// Re-export from betternte-core for convenience
pub use betternte_core::{CaptureFrame, CaptureTarget, ScreenCapture, WindowFinder};

/// Ensure process DPI awareness is set once.
pub fn ensure_process_dpi_aware() -> bool {
    static DPI_AWARE: OnceLock<bool> = OnceLock::new();
    *DPI_AWARE.get_or_init(|| unsafe {
        use windows::Win32::UI::HiDpi::{SetProcessDpiAwareness, PROCESS_PER_MONITOR_DPI_AWARE};
        match SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE) {
            Ok(()) => true,
            Err(e) => {
                // E_ACCESSDENIED means DPI mode was already set by manifest/startup.
                if e.code().0 as u32 == 0x80070005 {
                    true
                } else {
                    tracing::debug!("SetProcessDpiAwareness failed: {:?}", e);
                    false
                }
            }
        }
    })
}

#[cfg(test)]
mod report_validation {
    use std::path::PathBuf;

    fn report_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-reports/capture_smoke.json")
    }

    #[test]
    #[ignore]
    fn validate_capture_smoke_report() {
        let path = report_path();
        assert!(
            path.exists(),
            "Report not found at {:?}. Run the capture_smoke example first.",
            path
        );

        let data = std::fs::read_to_string(&path).expect("Failed to read report");
        let report: serde_json::Value = serde_json::from_str(&data).expect("Invalid JSON");

        // Validate structure
        assert_eq!(report["suite"], "capture_smoke");
        assert!(report["tests"].is_array());
        assert!(report["summary"].is_object());

        let tests = report["tests"].as_array().unwrap();
        assert!(!tests.is_empty(), "Report has no tests");

        // Validate each test entry has required fields
        for test in tests {
            assert!(test["name"].is_string(), "Test missing 'name'");
            assert!(test["passed"].is_boolean(), "Test missing 'passed'");
        }

        // Validate summary
        let summary = &report["summary"];
        assert!(summary["total"].is_number());
        assert!(summary["passed"].is_number());
        assert!(summary["failed"].is_number());

        let total = summary["total"].as_u64().unwrap();
        let passed = summary["passed"].as_u64().unwrap();
        let failed = summary["failed"].as_u64().unwrap();
        assert_eq!(total, passed + failed, "total != passed + failed");
        assert_eq!(total, tests.len() as u64, "summary.total != tests.len()");

        // BitBlt should always be available on Windows
        let bitblt_test = tests.iter().find(|t| t["name"] == "create_bitblt_engine");
        assert!(bitblt_test.is_some(), "Missing create_bitblt_engine test");
        assert!(
            bitblt_test.unwrap()["passed"].as_bool().unwrap_or(false),
            "BitBlt engine creation failed"
        );
    }
}
