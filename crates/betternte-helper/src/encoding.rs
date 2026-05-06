//! Encoding utilities: Base64 and MD5

use base64::{engine::general_purpose, Engine as _};
use md5::{Digest as Md5Digest, Md5};

/// Decode Base64 string to UTF-8 string
pub fn base64_decode_to_string(input: &str) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = base64_decode(input)?;
    Ok(String::from_utf8(bytes)?)
}

/// Decode Base64 string to bytes
pub fn base64_decode(input: &str) -> Result<Vec<u8>, base64::DecodeError> {
    // Handle URL-safe Base64
    let input = input.replace('-', "+").replace('_', "/");
    general_purpose::STANDARD.decode(input)
}

/// Encode bytes to Base64 string
pub fn base64_encode(data: &[u8]) -> String {
    general_purpose::STANDARD.encode(data)
}

/// Encode string to Base64
pub fn base64_encode_string(input: &str) -> String {
    base64_encode(input.as_bytes())
}

/// Calculate MD5 hash of a string
pub fn md5_string(input: &str) -> String {
    md5_bytes(input.as_bytes())
}

/// Calculate MD5 hash of bytes
pub fn md5_bytes(input: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(input);
    let result = hasher.finalize();
    hex_encode(&result)
}

/// Calculate MD5 hash (returns raw bytes)
pub fn md5_bytes_raw(input: &[u8]) -> [u8; 16] {
    let mut hasher = Md5::new();
    hasher.update(input);
    let result = hasher.finalize();
    result.into()
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode_decode() {
        let original = "Hello, World!";
        let encoded = base64_encode_string(original);
        assert_eq!(encoded, "SGVsbG8sIFdvcmxkIQ==");
        let decoded = base64_decode_to_string(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_md5() {
        let hash = md5_string("hello");
        assert_eq!(hash, "5d41402abc4b2a76b9719d911017c592");
    }
}
