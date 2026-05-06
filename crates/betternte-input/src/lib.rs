//! betternte-input: Input simulation module.
//!
//! Provides input simulation for PC native windows (foreground via SendInput
//! through `enigo`, background via PostMessage) and Android emulators (via
//! `adb shell input`). A failover wrapper transparently switches between
//! primary and fallback backends.
//!
//! # Architecture
//!
//! - [`InputController`]: Core trait for all input engines (from betternte-core)
//! - [`Win32Input`]: Win32 implementation using enigo (foreground) and PostMessage (background)
//! - [`AdbInput`]: ADB implementation for Android emulators
//! - [`InputQueue`]: Serializes input operations with rate limiting
//! - [`InputRecorder`] / [`MacroPlayer`]: Macro recording and playback
//!
//! # Example
//!
//! ```ignore
//! use betternte_input::{Win32Input, InputController, InputTarget, KeyMapper};
//! use std::collections::HashMap;
//!
//! let mapper = KeyMapper::new(HashMap::new());
//! let mut controller = Win32Input::new(mapper);
//!
//! let target = InputTarget::NativeWindow { hwnd: 0x12345 };
//! controller.init(&target).await.unwrap();
//!
//! controller.click(100, 200).await.unwrap();
//! controller.key_tap(Key::A, None).await.unwrap();
//! ```

pub mod action;
pub mod adb;
pub mod config;
pub mod controller;
pub mod error;
pub mod factory;
pub mod failover;
pub mod key;
pub mod mapper;
pub mod queue;
pub mod queued_controller;
pub mod recorder;
pub mod recording_controller;
pub mod target;
pub mod win32;

// Re-exports
pub use action::{InputAction, InputEvent};
pub use adb::AdbInput;
pub use config::{ForegroundInputBackend, InputConfig, InputMode};
pub use controller::InputController;
pub use error::{InputError, Result};
pub use factory::create_input_controller;
pub use failover::{FailoverConfig, FailoverInputController};
pub use key::{Key, MouseButton};
pub use mapper::KeyMapper;
pub use queue::InputQueue;
pub use queued_controller::QueuedInputController;
pub use recorder::{InputRecorder, Macro, MacroPlayer};
pub use recording_controller::{InputRecordEvent, InputRecordSink, RecordingInputController};
pub use target::InputTarget;
pub use win32::Win32Input;

// Tests
#[cfg(test)]
mod tests;
