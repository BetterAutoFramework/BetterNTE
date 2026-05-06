//! Platform check example — outputs a JSON test report.
//!
//! Usage: cargo run -p betternte-helper --example platform_check [-- <output_path>]
//!
//! Default output: target/test-reports/platform_check.json

use std::path::PathBuf;

fn main() {
    let output_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/test-reports/platform_check.json"));

    let mut tests = Vec::new();

    // Test: current_pid
    let pid = betternte_helper::current_pid();
    tests.push(TestResult {
        name: "current_pid".into(),
        result: serde_json::json!(pid),
        passed: pid > 0,
        error: None,
    });

    // Test: is_debug_build
    let debug = betternte_helper::is_debug_build();
    tests.push(TestResult {
        name: "is_debug_build".into(),
        result: serde_json::json!(debug),
        passed: true, // always valid
        error: None,
    });

    // Test: is_elevated
    let elevated = betternte_helper::is_elevated();
    tests.push(TestResult {
        name: "is_elevated".into(),
        result: serde_json::json!(elevated),
        passed: true, // always valid
        error: None,
    });

    // Test: is_debugger_attached
    let debugger = betternte_helper::is_debugger_attached();
    tests.push(TestResult {
        name: "is_debugger_attached".into(),
        result: serde_json::json!(debugger),
        passed: true, // always valid
        error: None,
    });

    // Test: get_system_dpi
    let dpi = betternte_helper::get_system_dpi();
    let dpi_ok = dpi.x > 0.0 && dpi.y > 0.0;
    tests.push(TestResult {
        name: "get_system_dpi".into(),
        result: serde_json::json!({ "x": dpi.x, "y": dpi.y }),
        passed: dpi_ok,
        error: if dpi_ok {
            None
        } else {
            Some("DPI values must be positive".into())
        },
    });

    // Test: get_foreground_window
    let fg = betternte_helper::get_foreground_window();
    tests.push(TestResult {
        name: "get_foreground_window".into(),
        result: serde_json::json!(fg),
        passed: true, // may be None if no window focused
        error: None,
    });

    // Test: DpiScale
    let scale = betternte_helper::DpiScale::new(1.5, 2.0);
    let inv = scale.inverse();
    let scale_ok = (inv.x - 1.0 / 1.5).abs() < 1e-6 && (inv.y - 0.5).abs() < 1e-6;
    tests.push(TestResult {
        name: "dpi_scale_inverse".into(),
        result: serde_json::json!({ "inverse_x": inv.x, "inverse_y": inv.y }),
        passed: scale_ok,
        error: if scale_ok {
            None
        } else {
            Some("DpiScale inverse mismatch".into())
        },
    });

    let total = tests.len();
    let passed = tests.iter().filter(|t| t.passed).count();

    let report = serde_json::json!({
        "suite": "platform_check",
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

    println!(
        "Platform check report written to: {}",
        output_path.display()
    );
    println!("{json}");
}

#[derive(serde::Serialize)]
struct TestResult {
    name: String,
    result: serde_json::Value,
    passed: bool,
    error: Option<String>,
}
