//! Regular expression utilities

use once_cell::sync::Lazy;
use regex::Regex;

/// Regular expression for extracting numbers from strings
pub static EXCLUDE_NUMBER_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[^0-9]+").expect("Invalid regex"));

/// Regular expression for full number strings
pub static FULL_NUMBER_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[0-9]+$").expect("Invalid regex"));

/// Regular expression for Chinese characters
pub static CHINESE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[\u4e00-\u9fa5]").expect("Invalid regex"));

/// Extract all numbers from a string
pub fn extract_numbers(text: &str) -> Vec<u64> {
    let numbers_str = EXCLUDE_NUMBER_REGEX.replace_all(text, " ");
    numbers_str
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect()
}

/// Extract the first number from a string
pub fn extract_first_number(text: &str) -> Option<u64> {
    EXCLUDE_NUMBER_REGEX
        .replace_all(text, " ")
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
}

/// Check if string is a full number
pub fn is_full_number(text: &str) -> bool {
    FULL_NUMBER_REGEX.is_match(text)
}

/// Check if text contains Chinese characters
pub fn contains_chinese(text: &str) -> bool {
    CHINESE_REGEX.is_match(text)
}

/// Extract all Chinese characters from text
pub fn extract_chinese(text: &str) -> String {
    CHINESE_REGEX.find_iter(text).map(|m| m.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_numbers() {
        assert_eq!(extract_numbers("abc123def456"), vec![123, 456]);
        assert_eq!(extract_numbers("no numbers here"), Vec::<u64>::new());
        assert_eq!(extract_numbers("test123abc"), vec![123]);
    }

    #[test]
    fn test_is_full_number() {
        assert!(is_full_number("123"));
        assert!(is_full_number("0"));
        assert!(!is_full_number("123abc"));
        assert!(!is_full_number(""));
    }

    #[test]
    fn test_contains_chinese() {
        assert!(contains_chinese("你好世界"));
        assert!(!contains_chinese("hello world"));
        assert!(contains_chinese("hello你好"));
    }
}
