//! betternte-input/src/mapper.rs
//! Key mapper

use std::collections::HashMap;

use crate::error::{InputError, Result};
use crate::key::Key;

/// Key mapper.
///
/// Maps string key names to specific `Key` enum values.
/// Supports user-defined mappings (e.g., "attack" -> Key::A).
#[derive(Debug, Clone)]
pub struct KeyMapper {
    /// User-defined key mappings
    bindings: HashMap<String, String>,
}

impl KeyMapper {
    /// Create a new key mapper.
    ///
    /// # Arguments
    /// * `bindings` - User-defined mapping table
    pub fn new(bindings: HashMap<String, String>) -> Self {
        Self { bindings }
    }

    /// Map key name to Key enum.
    ///
    /// Lookup order:
    /// 1. User-defined mapping (e.g., "attack" -> "VK_LBUTTON" -> Key::A)
    /// 2. Direct parsing of standard key name (e.g., "A" -> Key::A)
    pub fn map_key(&self, key: &str) -> Result<Key> {
        // Check user-defined mapping first
        if let Some(mapped) = self.bindings.get(key) {
            if let Some(k) = Key::try_parse(mapped) {
                return Ok(k);
            }
        }
        // Direct parsing
        Key::try_parse(key).ok_or_else(|| InputError::InvalidKey(key.to_string()))
    }

    /// Update mapping table
    pub fn update_bindings(&mut self, bindings: HashMap<String, String>) {
        self.bindings = bindings;
    }

    /// Get current mapping table
    pub fn bindings(&self) -> &HashMap<String, String> {
        &self.bindings
    }
}
