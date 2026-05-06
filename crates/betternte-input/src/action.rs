//! betternte-input/src/action.rs
//! Input action definitions

use serde::{Deserialize, Serialize};

use crate::key::{Key, MouseButton};

/// Input action enum.
///
/// Represents an atomic input operation, used for macro recording and input queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "params", rename_all = "snake_case")]
pub enum InputAction {
    /// Mouse move
    MouseMove {
        /// Target X coordinate
        x: i32,
        /// Target Y coordinate
        y: i32,
    },
    /// Mouse button press
    MouseDown {
        /// Mouse button
        button: MouseButton,
    },
    /// Mouse button release
    MouseUp {
        /// Mouse button
        button: MouseButton,
    },
    /// Keyboard key press
    KeyDown {
        /// Key
        key: Key,
    },
    /// Keyboard key release
    KeyUp {
        /// Key
        key: Key,
    },
    /// Mouse scroll
    Scroll {
        /// Scroll delta
        delta: i32,
    },
    /// Sleep/wait
    Sleep {
        /// Milliseconds to wait
        ms: u64,
    },
}

/// Recorded input event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputEvent {
    /// Event timestamp (offset from recording start in milliseconds)
    pub offset_ms: u64,

    /// Input action
    pub action: InputAction,
}
