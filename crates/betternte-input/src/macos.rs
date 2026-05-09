//! betternte-input/src/macos.rs
//! macOS input engine using cliclick + osascript.
//!
//! Uses `cliclick` for mouse operations and `osascript` (AppleScript) for
//! keyboard operations.
//!
//! Install cliclick: `brew install cliclick`
//!
//! For a native Rust implementation in the future, consider using the
//! `coregraphics` crate (CGEvent) directly.

use async_trait::async_trait;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result as AnyhowResult;

use crate::config::InputMode;
use crate::controller::InputController;
use crate::error::{InputError, Result};
use crate::key::Key;
use crate::key::MouseButton;
use crate::mapper::KeyMapper;
use crate::target::InputTarget;

/// macOS input engine.
///
/// Uses `cliclick` for mouse and `osascript` for keyboard simulation.
#[derive(Debug)]
pub struct MacInput {
    /// Window ID for targeted input (future use).
    _window_id: Option<u64>,

    /// Input mode.
    mode: InputMode,

    /// Key mapper.
    key_mapper: KeyMapper,

    /// Last operation latency in milliseconds.
    last_latency: Mutex<Option<f64>>,

    /// Whether initialized.
    initialized: bool,
}

impl MacInput {
    /// Create a new macOS input engine.
    pub fn new(key_mapper: KeyMapper) -> Self {
        Self {
            _window_id: None,
            mode: InputMode::Foreground,
            key_mapper,
            last_latency: Mutex::new(None),
            initialized: false,
        }
    }

    fn update_latency(&self, start: Instant) {
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        if let Ok(mut guard) = self.last_latency.lock() {
            *guard = Some(ms);
        }
    }

    /// Run cliclick with the given arguments.
    fn cliclick(args: &[&str]) -> Result<String> {
        let output = Command::new("cliclick")
            .args(args)
            .output()
            .map_err(|e| InputError::SimulationFailed(format!("cliclick exec failed: {e}")))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(InputError::SimulationFailed(format!(
                "cliclick failed: {stderr}"
            )))
        }
    }

    /// Run osascript with an AppleScript snippet.
    fn osascript(script: &str) -> Result<String> {
        let output = Command::new("osascript")
            .args(&["-e", script])
            .output()
            .map_err(|e| InputError::SimulationFailed(format!("osascript exec failed: {e}")))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(InputError::SimulationFailed(format!(
                "osascript failed: {stderr}"
            )))
        }
    }

    /// Convert our Key enum to cliclick key code.
    fn key_to_cliclick(key: Key) -> &'static str {
        match key {
            // Letters
            Key::A => "a",
            Key::B => "b",
            Key::C => "c",
            Key::D => "d",
            Key::E => "e",
            Key::F => "f",
            Key::G => "g",
            Key::H => "h",
            Key::I => "i",
            Key::J => "j",
            Key::K => "k",
            Key::L => "l",
            Key::M => "m",
            Key::N => "n",
            Key::O => "o",
            Key::P => "p",
            Key::Q => "q",
            Key::R => "r",
            Key::S => "s",
            Key::T => "t",
            Key::U => "u",
            Key::V => "v",
            Key::W => "w",
            Key::X => "x",
            Key::Y => "y",
            Key::Z => "z",
            // Digits
            Key::Num0 => "0",
            Key::Num1 => "1",
            Key::Num2 => "2",
            Key::Num3 => "3",
            Key::Num4 => "4",
            Key::Num5 => "5",
            Key::Num6 => "6",
            Key::Num7 => "7",
            Key::Num8 => "8",
            Key::Num9 => "9",
            // Function keys — cliclick uses "fn" prefix
            Key::F1 => "f1",
            Key::F2 => "f2",
            Key::F3 => "f3",
            Key::F4 => "f4",
            Key::F5 => "f5",
            Key::F6 => "f6",
            Key::F7 => "f7",
            Key::F8 => "f8",
            Key::F9 => "f9",
            Key::F10 => "f10",
            Key::F11 => "f11",
            Key::F12 => "f12",
            // Modifiers
            Key::Shift | Key::LShift | Key::RShift => "shift",
            Key::Control | Key::LControl | Key::RControl => "ctrl",
            Key::Alt | Key::LAlt | Key::RAlt => "alt",
            Key::LWin | Key::RWin => "cmd",
            // Navigation
            Key::Up => "arrow-up",
            Key::Down => "arrow-down",
            Key::Left => "arrow-left",
            Key::Right => "arrow-right",
            Key::Home => "home",
            Key::End => "end",
            Key::PageUp => "page-up",
            Key::PageDown => "page-down",
            // Editing
            Key::Backspace => "delete",
            Key::Tab => "tab",
            Key::Return | Key::Enter => "return",
            Key::Delete => "forward-delete",
            Key::Insert => "help",
            Key::Escape => "escape",
            Key::Space => "space",
            // System
            Key::PrintScreen => "f13", // macOS doesn't have PrintScreen
            Key::CapsLock => "caps-lock",
            // Fallbacks
            _ => "space",
        }
    }

    /// Convert our Key enum to AppleScript key name.
    fn key_to_applescript(key: Key) -> &'static str {
        match key {
            Key::A => "a",
            Key::B => "b",
            Key::C => "c",
            Key::D => "d",
            Key::E => "e",
            Key::F => "f",
            Key::G => "g",
            Key::H => "h",
            Key::I => "i",
            Key::J => "j",
            Key::K => "k",
            Key::L => "l",
            Key::M => "m",
            Key::N => "n",
            Key::O => "o",
            Key::P => "p",
            Key::Q => "q",
            Key::R => "r",
            Key::S => "s",
            Key::T => "t",
            Key::U => "u",
            Key::V => "v",
            Key::W => "w",
            Key::X => "x",
            Key::Y => "y",
            Key::Z => "z",
            Key::Num0 => "0",
            Key::Num1 => "1",
            Key::Num2 => "2",
            Key::Num3 => "3",
            Key::Num4 => "4",
            Key::Num5 => "5",
            Key::Num6 => "6",
            Key::Num7 => "7",
            Key::Num8 => "8",
            Key::Num9 => "9",
            Key::Space => "space",
            Key::Return => "return",
            Key::Tab => "tab",
            Key::Escape => "escape",
            Key::Delete => "delete",
            Key::Backspace => "delete",
            Key::Up => "up arrow",
            Key::Down => "down arrow",
            Key::Left => "left arrow",
            Key::Right => "right arrow",
            _ => "space",
        }
    }
}

#[async_trait]
impl InputController for MacInput {
    fn name(&self) -> &str {
        "macOS"
    }

    fn parse_key(&self, name: &str) -> Option<Key> {
        self.key_mapper.map_key(name).ok()
    }

    async fn init(&mut self, target: &InputTarget) -> AnyhowResult<()> {
        match target {
            InputTarget::NativeWindow { .. } | InputTarget::NativeWindowBackground { .. } => {
                // macOS doesn't use hwnd; we just mark as initialized.
                self.mode = InputMode::Foreground;
                self.initialized = true;
            }
            _ => {
                return Err(InputError::SimulationFailed(
                    "macOS engine only supports native windows (use ADB for emulators)".into(),
                )
                .into())
            }
        }
        Ok(())
    }

    async fn mouse_move(&self, x: i32, y: i32) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        Self::cliclick(&[&format!("m:{x},{y}")])?;
        self.update_latency(start);
        Ok(())
    }

    async fn click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        Self::cliclick(&[&format!("c:{x},{y}")])?;
        self.update_latency(start);
        Ok(())
    }

    async fn double_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        Self::cliclick(&[&format!("dc:{x},{y}")])?;
        self.update_latency(start);
        Ok(())
    }

    async fn right_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        Self::cliclick(&[&format!("rc:{x},{y}")])?;
        self.update_latency(start);
        Ok(())
    }

    async fn mouse_down(&self, button: MouseButton) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        match button {
            MouseButton::Left => Self::cliclick(&["dd:."])?,
            MouseButton::Right => Self::cliclick(&["rd:."])?,
            _ => Self::cliclick(&["dd:."])?,
        };
        self.update_latency(start);
        Ok(())
    }

    async fn mouse_up(&self, button: MouseButton) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        match button {
            MouseButton::Left => Self::cliclick(&["du:."])?,
            MouseButton::Right => Self::cliclick(&["ru:."])?,
            _ => Self::cliclick(&["du:."])?,
        };
        self.update_latency(start);
        Ok(())
    }

    async fn mouse_scroll(&self, delta: i32) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        // cliclick: positive = scroll up, negative = scroll down
        let direction = if delta > 0 { "up" } else { "down" };
        let count = delta.unsigned_abs();
        for _ in 0..count {
            Self::cliclick(&[&format!("kd:cmd;{direction};ku:cmd")])?;
        }
        self.update_latency(start);
        Ok(())
    }

    async fn swipe(
        &self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        duration_ms: u32,
    ) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();

        let steps = ((duration_ms as f64 / 16.0) as u32).clamp(2, 240);
        let step_delay = Duration::from_millis(duration_ms.max(1) as u64 / steps as u64);
        let dx = (x2 - x1) as f64 / steps as f64;
        let dy = (y2 - y1) as f64 / steps as f64;

        // Move to start, press, drag, release.
        Self::cliclick(&[&format!("m:{x1},{y1}")])?;
        Self::cliclick(&["dd:."])?;

        for i in 1..=steps {
            let px = x1 + (dx * i as f64) as i32;
            let py = y1 + (dy * i as f64) as i32;
            Self::cliclick(&[&format!("m:{px},{py}")])?;
            tokio::time::sleep(step_delay).await;
        }

        Self::cliclick(&["du:."])?;
        self.update_latency(start);
        Ok(())
    }

    async fn key_press(&self, key: Key) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        let key_name = Self::key_to_cliclick(key);
        Self::cliclick(&[&format!("kd:{key_name}")])?;
        self.update_latency(start);
        Ok(())
    }

    async fn key_release(&self, key: Key) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        let key_name = Self::key_to_cliclick(key);
        Self::cliclick(&[&format!("ku:{key_name}")])?;
        self.update_latency(start);
        Ok(())
    }

    async fn key_tap(&self, key: Key, duration_ms: Option<u32>) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        let key_name = Self::key_to_cliclick(key);
        Self::cliclick(&[&format!("kp:{key_name}")])?;
        if let Some(ms) = duration_ms {
            tokio::time::sleep(Duration::from_millis(ms as u64)).await;
        }
        self.update_latency(start);
        Ok(())
    }

    async fn type_text(&self, text: &str) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        // osascript keystroke works well for ASCII text.
        let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
        Self::osascript(&format!(
            "tell application \"System Events\" to keystroke \"{escaped}\""
        ))?;
        self.update_latency(start);
        Ok(())
    }

    async fn key_combo(&self, keys: &[Key]) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        // Build AppleScript: e.g. keystroke "a" using {command down, shift down}
        let main_key = keys.last().copied().unwrap_or(Key::Space);
        let main_char = Self::key_to_applescript(main_key);
        let modifiers: Vec<&str> = keys[..keys.len().saturating_sub(1)]
            .iter()
            .map(|k| match k {
                Key::Control | Key::LControl | Key::RControl => "control down",
                Key::Shift | Key::LShift | Key::RShift => "shift down",
                Key::Alt | Key::LAlt | Key::RAlt => "option down",
                Key::LWin | Key::RWin => "command down",
                _ => "",
            })
            .filter(|s| !s.is_empty())
            .collect();

        if modifiers.is_empty() {
            Self::osascript(&format!(
                "tell application \"System Events\" to keystroke \"{main_char}\""
            ))?;
        } else {
            let mod_str = modifiers.join(", ");
            Self::osascript(&format!(
                "tell application \"System Events\" to keystroke \"{main_char}\" using {{{mod_str}}}"
            ))?;
        }
        self.update_latency(start);
        Ok(())
    }

    fn supports_background(&self) -> bool {
        false // macOS doesn't support background input via cliclick
    }

    fn last_latency_ms(&self) -> Option<f64> {
        *self.last_latency.lock().ok()?
    }

    fn mode(&self) -> InputMode {
        self.mode
    }
}

#[cfg(test)]
mod macos_pure_tests {
    use super::*;

    #[test]
    fn key_to_cliclick_letters() {
        assert_eq!(MacInput::key_to_cliclick(Key::A), "a");
        assert_eq!(MacInput::key_to_cliclick(Key::Z), "z");
    }

    #[test]
    fn key_to_cliclick_modifiers() {
        assert_eq!(MacInput::key_to_cliclick(Key::Control), "ctrl");
        assert_eq!(MacInput::key_to_cliclick(Key::Shift), "shift");
        assert_eq!(MacInput::key_to_cliclick(Key::Alt), "alt");
        assert_eq!(MacInput::key_to_cliclick(Key::LWin), "cmd");
    }

    #[test]
    fn key_to_cliclick_navigation() {
        assert_eq!(MacInput::key_to_cliclick(Key::Up), "arrow-up");
        assert_eq!(MacInput::key_to_cliclick(Key::Down), "arrow-down");
        assert_eq!(MacInput::key_to_cliclick(Key::Escape), "escape");
    }

    #[test]
    fn key_to_applescript_letters() {
        assert_eq!(MacInput::key_to_applescript(Key::A), "a");
        assert_eq!(MacInput::key_to_applescript(Key::Return), "return");
    }
}
