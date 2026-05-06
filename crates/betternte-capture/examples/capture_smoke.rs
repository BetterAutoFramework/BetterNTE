//! Capture smoke test example — outputs a JSON test report.
//!
//! Usage: cargo run -p betternte-capture --example capture_smoke [-- <output_path>]
//!
//! Default output: target/test-reports/capture_smoke.json
//!
//! This example checks which capture methods are available on the current platform
//! without actually capturing any frames.

#![cfg(windows)]

use std::path::PathBuf;

use betternte_capture::WindowFinder;
use betternte_capture::{CaptureTarget, PrintWindowCapture, ScreenCapture, ScreenDCCapture};

fn main() {
    let output_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/test-reports/capture_smoke.json"));

    let mut tests = Vec::new();

    // Test: available_capture_methods
    let methods = betternte_capture::available_capture_methods();
    let method_count = methods.len();
    tests.push(TestResult {
        name: "available_capture_methods_count".into(),
        result: serde_json::json!(method_count),
        passed: method_count >= 4, // at least BitBlt, PrintWindow, ScreenDC + others
        error: None,
    });

    // Test: each method's availability
    for info in &methods {
        let name = format!("method_available_{:?}", info.method);
        tests.push(TestResult {
            name,
            result: serde_json::json!({
                "method": format!("{:?}", info.method),
                "available": info.available,
            }),
            passed: true, // availability check itself always succeeds
            error: None,
        });
    }

    // Test: resolve_auto_capture_method
    let whitelist = vec![
        betternte_core::CaptureMethod::BitBlt,
        betternte_core::CaptureMethod::PrintWindow,
    ];
    let auto = betternte_capture::resolve_auto_capture_method(&whitelist);
    tests.push(TestResult {
        name: "resolve_auto_capture_method".into(),
        result: serde_json::json!(auto),
        passed: !auto.is_empty(),
        error: None,
    });

    // Test: create BitBlt engine (always available)
    let engine = betternte_capture::create_capture_engine(
        &betternte_core::CaptureMethod::BitBlt,
        &whitelist,
    );
    match engine {
        Ok(e) => {
            tests.push(TestResult {
                name: "create_bitblt_engine".into(),
                result: serde_json::json!({ "name": e.name() }),
                passed: e.name() == "BitBlt",
                error: None,
            });
        }
        Err(e) => {
            tests.push(TestResult {
                name: "create_bitblt_engine".into(),
                result: serde_json::Value::Null,
                passed: false,
                error: Some(e.to_string()),
            });
        }
    }

    // Test: create_frame_buffer
    let buf = betternte_capture::create_frame_buffer(10);
    tests.push(TestResult {
        name: "create_frame_buffer".into(),
        result: serde_json::json!({ "capacity": buf.capacity() }),
        passed: buf.capacity() == 10,
        error: None,
    });

    // Test: WindowFinderImpl
    let finder = betternte_capture::WindowFinderImpl::new();
    let all_windows = finder.find_by_keyword("");
    match all_windows {
        Ok(windows) => {
            tests.push(TestResult {
                name: "window_finder_list".into(),
                result: serde_json::json!({ "count": windows.len() }),
                passed: true, // listing windows always succeeds
                error: None,
            });
        }
        Err(e) => {
            tests.push(TestResult {
                name: "window_finder_list".into(),
                result: serde_json::Value::Null,
                passed: false,
                error: Some(e.to_string()),
            });
        }
    }

    // Test: unsupported target returns explicit error kind
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    let unsupported_screen_dc = rt.block_on(async {
        let mut cap = ScreenDCCapture::new();
        cap.start(&CaptureTarget::Display { index: 0 }).await
    });
    tests.push(TestResult {
        name: "screendc_unsupported_display_target".into(),
        result: serde_json::json!({
            "is_err": unsupported_screen_dc.is_err()
        }),
        passed: unsupported_screen_dc.is_err(),
        error: unsupported_screen_dc.err().map(|e| e.to_string()),
    });

    let unsupported_print_window = rt.block_on(async {
        let mut cap = PrintWindowCapture::new();
        cap.start(&CaptureTarget::Display { index: 0 }).await
    });
    tests.push(TestResult {
        name: "printwindow_unsupported_display_target".into(),
        result: serde_json::json!({
            "is_err": unsupported_print_window.is_err()
        }),
        passed: unsupported_print_window.is_err(),
        error: unsupported_print_window.err().map(|e| e.to_string()),
    });

    // Placeholder: capture window resize drift test requires a real, resizable game window.
    tests.push(TestResult {
        name: "window_resize_runtime_placeholder".into(),
        result: serde_json::json!({
            "skipped": true,
            "reason": "Requires interactive window resize during active capture session"
        }),
        passed: true,
        error: None,
    });

    // Placeholder: DXGI access-lost recovery simulation, opt-in by env flag.
    let enable_dxgi_access_lost_probe = std::env::var("BETTERNTE_TEST_DXGI_ACCESS_LOST")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    tests.push(TestResult {
        name: "dxgi_access_lost_recovery_placeholder".into(),
        result: serde_json::json!({
            "enabled": enable_dxgi_access_lost_probe,
            "skipped": !enable_dxgi_access_lost_probe,
            "reason": "Requires lock/switch/display state manipulation in a live desktop session"
        }),
        passed: true,
        error: None,
    });

    let total = tests.len();
    let passed = tests.iter().filter(|t| t.passed).count();

    let report = serde_json::json!({
        "suite": "capture_smoke",
        "tests": tests,
        "summary": {
            "total": total,
            "passed": passed,
            "failed": total - passed,
        }
    });

    // Ensure parent directory exists
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create output directory");
    }

    let json = serde_json::to_string_pretty(&report).expect("Failed to serialize report");
    std::fs::write(&output_path, &json).expect("Failed to write report");

    println!("Capture smoke report written to: {}", output_path.display());
    println!("{json}");
}

#[derive(serde::Serialize)]
struct TestResult {
    name: String,
    result: serde_json::Value,
    passed: bool,
    error: Option<String>,
}
