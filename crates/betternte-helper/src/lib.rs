//! betternte-helper: Helper utilities
//!
//! Common helper utilities

pub mod directory;
pub mod encoding;
pub mod math;
pub mod process;
pub mod regex;
pub mod string;
pub mod time;
pub mod windows;

// Explicit re-exports to avoid ambiguous glob re-exports
pub use directory::{
    copy_directory, delete_directory, delete_directory_with_retry, directory_size,
    ensure_directory, is_directory_empty,
};
pub use encoding::{
    base64_decode, base64_decode_to_string, base64_encode, base64_encode_string, md5_bytes,
    md5_bytes_raw, md5_string,
};
pub use math::{
    distance, point_in_rect, point_to_line_distance, point_to_segment_distance, rect_intersection,
    rect_union, Point, Rect,
};
pub use process::{current_pid, is_debug_build, is_debugger_attached, is_elevated};
pub use regex::{
    contains_chinese, extract_chinese, extract_first_number, extract_numbers, is_full_number,
    CHINESE_REGEX, EXCLUDE_NUMBER_REGEX, FULL_NUMBER_REGEX,
};
pub use string::{
    contains_chinese as string_contains_chinese, convert_fullwidth_to_halfwidth,
    extract_chinese as string_extract_chinese, extract_positive_int,
    extract_positive_int_with_default, is_blank, is_pure_english, join_non_empty, normalize,
    remove_all_enter, remove_all_space, try_parse_double, try_parse_int,
    try_parse_int_with_default,
};
pub use time::{DefaultServerTimeProvider, ServerTime, ServerTimeProvider, SpeedTimer};
pub use windows::{get_foreground_window, get_system_dpi, get_window_dpi, DpiScale};

#[cfg(test)]
mod report_validation {
    use std::path::PathBuf;

    fn report_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-reports/platform_check.json")
    }

    #[test]
    #[ignore]
    fn validate_platform_check_report() {
        let path = report_path();
        assert!(
            path.exists(),
            "Report not found at {:?}. Run the platform_check example first.",
            path
        );

        let data = std::fs::read_to_string(&path).expect("Failed to read report");
        let report: serde_json::Value = serde_json::from_str(&data).expect("Invalid JSON");

        // Validate structure
        assert_eq!(report["suite"], "platform_check");
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

        // All platform checks should pass on a healthy system
        assert_eq!(
            failed,
            0,
            "Some platform checks failed: {:#?}",
            tests
                .iter()
                .filter(|t| !t["passed"].as_bool().unwrap_or(true))
                .collect::<Vec<_>>()
        );
    }
}
