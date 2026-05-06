//! String utilities

use regex::Regex;

use once_cell::sync::Lazy;

/// Regular expression for Chinese characters
static CHINESE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[\u4e00-\u9fa5]").expect("Invalid regex"));

/// Remove all whitespace characters
pub fn remove_all_space(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Remove all newline characters
pub fn remove_all_enter(s: &str) -> String {
    s.chars().filter(|&c| c != '\n' && c != '\r').collect()
}

/// Check if string contains Chinese characters
pub fn contains_chinese(s: &str) -> bool {
    CHINESE_REGEX.is_match(s)
}

/// Extract all Chinese characters
pub fn extract_chinese(s: &str) -> String {
    CHINESE_REGEX.find_iter(s).map(|m| m.as_str()).collect()
}

/// Check if string is pure ASCII letters
pub fn is_pure_english(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphabetic())
}

/// Convert full-width alphanumeric characters to half-width
pub fn convert_fullwidth_to_halfwidth(s: &str) -> String {
    s.chars()
        .map(|c| {
            let code = c as u32;
            // Full-width numbers: ０(0xFF10) - ９(0xFF19)
            if (0xFF10..=0xFF19).contains(&code) {
                let offset = code - 0xFF10;
                char::from_u32('0' as u32 + offset).unwrap_or(c)
            // Full-width uppercase letters: Ａ(0xFF21) - Ｚ(0xFF3A)
            } else if (0xFF21..=0xFF3A).contains(&code) {
                let offset = code - 0xFF21;
                char::from_u32('A' as u32 + offset).unwrap_or(c)
            // Full-width lowercase letters: ａ(0xFF41) - ｚ(0xFF5A)
            } else if (0xFF41..=0xFF5A).contains(&code) {
                let offset = code - 0xFF41;
                char::from_u32('a' as u32 + offset).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

/// Try to parse string as f64, returns 0.0 on failure
pub fn try_parse_double(s: &str) -> f64 {
    s.parse().unwrap_or(0.0)
}

/// Try to parse string as i32, returns 0 on failure
pub fn try_parse_int(s: &str) -> i32 {
    s.parse().unwrap_or(0)
}

/// Try to parse string as i32 with default value
pub fn try_parse_int_with_default(s: &str, default: i32) -> i32 {
    s.parse().unwrap_or(default)
}

/// Extract positive integer from string
pub fn extract_positive_int(s: &str) -> Option<i32> {
    let re = Regex::new(r"[0-9]+").ok()?;
    re.find(s).and_then(|m| m.as_str().parse().ok())
}

/// Extract first positive integer from string with default
pub fn extract_positive_int_with_default(s: &str, default: i32) -> i32 {
    extract_positive_int(s).unwrap_or(default)
}

/// Trim and convert to lowercase
pub fn normalize(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Check if string is empty or whitespace only
pub fn is_blank(s: &str) -> bool {
    s.trim().is_empty()
}

/// Join strings with separator, ignoring empty ones
pub fn join_non_empty(parts: &[&str], sep: &str) -> String {
    let filtered: Vec<&str> = parts.iter().filter(|s| !s.is_empty()).cloned().collect();
    filtered.join(sep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_space() {
        assert_eq!(remove_all_space("a b c"), "abc");
        assert_eq!(remove_all_space("a\tb\nc"), "abc");
    }

    #[test]
    fn test_fullwidth_to_halfwidth() {
        assert_eq!(convert_fullwidth_to_halfwidth("１２３"), "123");
        assert_eq!(convert_fullwidth_to_halfwidth("ａ"), "a");
    }

    #[test]
    fn test_is_pure_english() {
        assert!(is_pure_english("hello"));
        assert!(!is_pure_english("hello123"));
        assert!(!is_pure_english("你好"));
        assert!(!is_pure_english(""));
    }

    #[test]
    fn test_remove_all_enter() {
        assert_eq!(remove_all_enter("a\nb\r\nc"), "abc");
        assert_eq!(remove_all_enter("no newlines"), "no newlines");
        assert_eq!(remove_all_enter(""), "");
    }

    #[test]
    fn test_contains_chinese() {
        assert!(!contains_chinese("hello"));
        assert!(contains_chinese("你好"));
        assert!(contains_chinese("hello你好"));
        assert!(!contains_chinese("123"));
        assert!(!contains_chinese(""));
    }

    #[test]
    fn test_extract_chinese() {
        assert_eq!(extract_chinese("abc你好def世界"), "你好世界");
        assert_eq!(extract_chinese("no chinese"), "");
        assert_eq!(extract_chinese("你好"), "你好");
        assert_eq!(extract_chinese(""), "");
    }

    #[test]
    fn test_try_parse_double() {
        let three_point_one_four = 157_f64 / 50_f64;
        assert!((try_parse_double("3.14") - three_point_one_four).abs() < 1e-10);
        assert!((try_parse_double("abc") - 0.0).abs() < 1e-10);
        assert!((try_parse_double("-1.5") - (-1.5)).abs() < 1e-10);
    }

    #[test]
    fn test_try_parse_int() {
        assert_eq!(try_parse_int("42"), 42);
        assert_eq!(try_parse_int("abc"), 0);
        assert_eq!(try_parse_int("-5"), -5);
    }

    #[test]
    fn test_try_parse_int_with_default() {
        assert_eq!(try_parse_int_with_default("42", 0), 42);
        assert_eq!(try_parse_int_with_default("abc", 99), 99);
    }

    #[test]
    fn test_extract_positive_int() {
        assert_eq!(extract_positive_int("abc123def"), Some(123));
        assert_eq!(extract_positive_int("no numbers"), None);
        assert_eq!(extract_positive_int("x42y7z"), Some(42));
        assert_eq!(extract_positive_int("0"), Some(0));
    }

    #[test]
    fn test_extract_positive_int_with_default() {
        assert_eq!(extract_positive_int_with_default("abc123", 0), 123);
        assert_eq!(extract_positive_int_with_default("no numbers", 99), 99);
    }

    #[test]
    fn test_normalize() {
        assert_eq!(normalize("  Hello World  "), "hello world");
        assert_eq!(normalize("ABC"), "abc");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn test_is_blank() {
        assert!(is_blank(""));
        assert!(is_blank("  \t\n"));
        assert!(!is_blank("a"));
        assert!(!is_blank(" a "));
    }

    #[test]
    fn test_join_non_empty() {
        assert_eq!(join_non_empty(&["", "a", "", "b", ""], ","), "a,b");
        assert_eq!(join_non_empty(&["a", "b"], "-"), "a-b");
        assert_eq!(join_non_empty(&["", ""], ","), "");
        assert_eq!(join_non_empty(&[], ","), "");
    }

    #[test]
    fn test_fullwidth_uppercase() {
        assert_eq!(convert_fullwidth_to_halfwidth("ＡＢＣ"), "ABC");
    }

    #[test]
    fn test_fullwidth_numbers() {
        assert_eq!(
            convert_fullwidth_to_halfwidth("０１２３４５６７８９"),
            "0123456789"
        );
    }
}
