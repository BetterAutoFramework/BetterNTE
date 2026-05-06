//! Input configuration definitions.
//!
//! Most input behaviour is currently driven by `betternte-core::AdvancedConfig`
//! (see `input_mode`, `input_rate_limit`, `input_fallback_*` fields) and
//! consumed in `betternte-engine::capture::bind_script_ctx_hwnd`. The
//! standalone [`InputConfig`] below is kept as a convenience for callers that
//! want to instantiate an [`crate::InputQueue`] / controller without the full
//! engine config plumbing.

pub use betternte_core::{ForegroundInputBackend, InputMode};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Input controller configuration.
///
/// Only the fields below are actually consumed by the input layer:
///
/// * [`InputConfig::mode`] — selects foreground/background semantics.
/// * [`InputConfig::rate_limit`] — passed to [`crate::InputQueue::new`] when
///   the caller wires up a queue.
/// * [`InputConfig::key_bindings`] — passed to [`crate::KeyMapper`] so logical
///   key names (e.g. `"attack"`) resolve to a [`betternte_core::Key`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    /// Input mode (foreground/background/auto).
    pub mode: InputMode,

    /// Maximum operations per second for the input queue (`0` = unlimited).
    pub rate_limit: u32,

    /// Logical-name → key-name mapping consumed by [`crate::KeyMapper`].
    pub key_bindings: HashMap<String, String>,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            mode: InputMode::Auto,
            rate_limit: 0,
            key_bindings: HashMap::new(),
        }
    }
}
