//! betternte-input/src/linux.rs
//! Linux input engine using xdotool (X11).
//!
//! Uses `xdotool` for both foreground and background input on X11.
//! Requires: `sudo apt install xdotool`
//!
//! For Wayland, xdotool won't work — a future implementation could use
//! `ydotool` (uinput-based) or `wlrctl`.

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

/// Linux input engine.
///
/// Uses `xdotool` for mouse/keyboard simulation on X11.
/// Supports both foreground (active window) and background (specific window ID) modes.
#[derive(Debug)]
pub struct LinuxInput {
    /// Window ID for targeted input (background mode).
    window_id: Option<u64>,

    /// Input mode.
    mode: InputMode,

    /// Key mapper.
    key_mapper: KeyMapper,

    /// Last operation latency in milliseconds.
    last_latency: Mutex<Option<f64>>,

    /// Whether initialized.
    initialized: bool,
}

impl LinuxInput {
    /// Create a new Linux input engine.
    pub fn new(key_mapper: KeyMapper) -> Self {
        Self {
            window_id: None,
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

    /// Convert our Key enum to xdotool key name.
    fn key_to_xdotool(key: Key) -> &'static str {
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
            // Function keys
            Key::F1 => "F1",
            Key::F2 => "F2",
            Key::F3 => "F3",
            Key::F4 => "F4",
            Key::F5 => "F5",
            Key::F6 => "F6",
            Key::F7 => "F7",
            Key::F8 => "F8",
            Key::F9 => "F9",
            Key::F10 => "F10",
            Key::F11 => "F11",
            Key::F12 => "F12",
            // Modifiers
            Key::Shift => "shift",
            Key::LShift => "shift",
            Key::RShift => "shift",
            Key::Control => "ctrl",
            Key::LControl => "ctrl",
            Key::RControl => "ctrl",
            Key::Alt => "alt",
            Key::LAlt => "alt",
            Key::RAlt => "alt",
            // Navigation
            Key::Up => "Up",
            Key::Down => "Down",
            Key::Left => "Left",
            Key::Right => "Right",
            Key::Home => "Home",
            Key::End => "End",
            Key::PageUp => "Prior",
            Key::PageDown => "Next",
            // Editing
            Key::Backspace => "BackSpace",
            Key::Tab => "Tab",
            Key::Return | Key::Enter => "Return",
            Key::Delete => "Delete",
            Key::Insert => "Insert",
            Key::Escape => "Escape",
            Key::Space => "space",
            // System
            Key::PrintScreen => "Print",
            Key::ScrollLock => "Scroll_Lock",
            Key::Pause => "Pause",
            Key::CapsLock => "Caps_Lock",
            Key::NumLock => "Num_Lock",
            Key::LWin => "Super_L",
            Key::RWin => "Super_R",
            Key::Apps => "Menu",
            // OEM keys
            Key::Oem1 => "semicolon",
            Key::OemPlus => "equal",
            Key::OemComma => "comma",
            Key::OemMinus => "minus",
            Key::OemPeriod => "period",
            Key::Oem2 => "slash",
            Key::Oem3 => "grave",
            Key::Oem4 => "bracketleft",
            Key::Oem5 => "backslash",
            Key::Oem6 => "bracketright",
            Key::Oem7 => "apostrophe",
            // Numpad
            Key::Numpad0 => "KP_0",
            Key::Numpad1 => "KP_1",
            Key::Numpad2 => "KP_2",
            Key::Numpad3 => "KP_3",
            Key::Numpad4 => "KP_4",
            Key::Numpad5 => "KP_5",
            Key::Numpad6 => "KP_6",
            Key::Numpad7 => "KP_7",
            Key::Numpad8 => "KP_8",
            Key::Numpad9 => "KP_9",
            Key::NumpadAdd => "KP_Add",
            Key::NumpadSubtract => "KP_Subtract",
            Key::NumpadMultiply => "KP_Multiply",
            Key::NumpadDivide => "KP_Divide",
            Key::NumpadDecimal => "KP_Decimal",
            Key::NumpadEnter => "KP_Enter",
            // Media
            Key::VolumeMute => "XF86AudioMute",
            Key::VolumeDown => "XF86AudioLowerVolume",
            Key::VolumeUp => "XF86AudioRaiseVolume",
            Key::MediaNextTrack => "XF86AudioNext",
            Key::MediaPrevTrack => "XF86AudioPrev",
            Key::MediaStop => "XF86AudioStop",
            Key::MediaPlayPause => "XF86AudioPlay",
            // Browser
            Key::BrowserBack => "XF86Back",
            Key::BrowserForward => "XF86Forward",
            Key::BrowserRefresh => "XF86Refresh",
            Key::BrowserStop => "XF86Stop",
            Key::BrowserSearch => "XF86Search",
            Key::BrowserFavorites => "XF86Favorites",
            Key::BrowserHome => "XF86HomePage",
            // Fallbacks for less common keys
            Key::F13 => "F13",
            Key::F14 => "F14",
            Key::F15 => "F15",
            Key::F16 => "F16",
            Key::F17 => "F17",
            Key::F18 => "F18",
            Key::F19 => "F19",
            Key::F20 => "F20",
            Key::F21 => "F21",
            Key::F22 => "F22",
            Key::F23 => "F23",
            Key::F24 => "F24",
        }
    }

    /// Run xdotool with the given arguments.
    fn xdotool(args: &[&str]) -> Result<String> {
        let output = Command::new("xdotool")
            .args(args)
            .output()
            .map_err(|e| InputError::SimulationFailed(format!("xdotool exec failed: {e}")))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(InputError::SimulationFailed(format!(
                "xdotool failed: {stderr}"
            )))
        }
    }
}

#[async_trait]
impl InputController for LinuxInput {
    fn name(&self) -> &str {
        "Linux"
    }

    fn parse_key(&self, name: &str) -> Option<Key> {
        self.key_mapper.map_key(name).ok()
    }

    async fn init(&mut self, target: &InputTarget) -> AnyhowResult<()> {
        match target {
            InputTarget::NativeWindow { hwnd } => {
                self.window_id = Some(*hwnd);
                self.mode = InputMode::Foreground;
                self.initialized = true;
            }
            InputTarget::NativeWindowBackground { hwnd } => {
                self.window_id = Some(*hwnd);
                self.mode = InputMode::Background;
                self.initialized = true;
            }
            _ => {
                return Err(InputError::SimulationFailed(
                    "Linux engine only supports native windows (use ADB for emulators)".into(),
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

        match self.mode {
            InputMode::Foreground | InputMode::Auto => {
                Self::xdotool(&["mousemove", &x.to_string(), &y.to_string()])?;
            }
            InputMode::Background => {
                let wid = self.window_id.ok_or(InputError::NotInitialized)?;
                Self::xdotool(&[
                    "mousemove",
                    "--window",
                    &wid.to_string(),
                    &x.to_string(),
                    &y.to_string(),
                ])?;
            }
        }

        self.update_latency(start);
        Ok(())
    }

    async fn click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();

        match self.mode {
            InputMode::Foreground | InputMode::Auto => {
                Self::xdotool(&[
                    "mousemove",
                    &x.to_string(),
                    &y.to_string(),
                    "click",
                    "1",
                ])?;
            }
            InputMode::Background => {
                let wid = self.window_id.ok_or(InputError::NotInitialized)?;
                Self::xdotool(&[
                    "mousemove",
                    "--window",
                    &wid.to_string(),
                    &x.to_string(),
                    &y.to_string(),
                    "click",
                    "1",
                ])?;
            }
        }

        self.update_latency(start);
        Ok(())
    }

    async fn double_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();

        match self.mode {
            InputMode::Foreground | InputMode::Auto => {
                Self::xdotool(&[
                    "mousemove",
                    &x.to_string(),
                    &y.to_string(),
                    "click",
                    "--repeat",
                    "2",
                    "1",
                ])?;
            }
            InputMode::Background => {
                let wid = self.window_id.ok_or(InputError::NotInitialized)?;
                Self::xdotool(&[
                    "mousemove",
                    "--window",
                    &wid.to_string(),
                    &x.to_string(),
                    &y.to_string(),
                    "click",
                    "--repeat",
                    "2",
                    "1",
                ])?;
            }
        }

        self.update_latency(start);
        Ok(())
    }

    async fn right_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();

        match self.mode {
            InputMode::Foreground | InputMode::Auto => {
                Self::xdotool(&[
                    "mousemove",
                    &x.to_string(),
                    &y.to_string(),
                    "click",
                    "3",
                ])?;
            }
            InputMode::Background => {
                let wid = self.window_id.ok_or(InputError::NotInitialized)?;
                Self::xdotool(&[
                    "mousemove",
                    "--window",
                    &wid.to_string(),
                    &x.to_string(),
                    &y.to_string(),
                    "click",
                    "3",
                ])?;
            }
        }

        self.update_latency(start);
        Ok(())
    }

    async fn mouse_down(&self, button: MouseButton) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        let btn = match button {
            MouseButton::Left => "1",
            MouseButton::Right => "3",
            MouseButton::Middle => "2",
            MouseButton::X1 => "8",
            MouseButton::X2 => "9",
        };
        Self::xdotool(&["mousedown", btn])?;
        self.update_latency(start);
        Ok(())
    }

    async fn mouse_up(&self, button: MouseButton) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        let btn = match button {
            MouseButton::Left => "1",
            MouseButton::Right => "3",
            MouseButton::Middle => "2",
            MouseButton::X1 => "8",
            MouseButton::X2 => "9",
        };
        Self::xdotool(&["mouseup", btn])?;
        self.update_latency(start);
        Ok(())
    }

    async fn mouse_scroll(&self, delta: i32) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        // xdotool: positive = scroll up, negative = scroll down
        let button = if delta > 0 { "4" } else { "5" };
        let count = delta.unsigned_abs();
        for _ in 0..count {
            Self::xdotool(&["click", button])?;
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

        // Move to start, press, drag to end, release.
        let steps = ((duration_ms as f64 / 16.0) as u32).clamp(2, 240);
        let step_delay = Duration::from_millis(duration_ms.max(1) as u64 / steps as u64);
        let dx = (x2 - x1) as f64 / steps as f64;
        let dy = (y2 - y1) as f64 / steps as f64;

        Self::xdotool(&["mousemove", &x1.to_string(), &y1.to_string()])?;
        Self::xdotool(&["mousedown", "1"])?;

        for i in 1..=steps {
            let px = x1 + (dx * i as f64) as i32;
            let py = y1 + (dy * i as f64) as i32;
            Self::xdotool(&["mousemove", &px.to_string(), &py.to_string()])?;
            tokio::time::sleep(step_delay).await;
        }

        Self::xdotool(&["mouseup", "1"])?;
        self.update_latency(start);
        Ok(())
    }

    async fn key_press(&self, key: Key) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        let key_name = Self::key_to_xdotool(key);
        Self::xdotool(&["keydown", key_name])?;
        self.update_latency(start);
        Ok(())
    }

    async fn key_release(&self, key: Key) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        let key_name = Self::key_to_xdotool(key);
        Self::xdotool(&["keyup", key_name])?;
        self.update_latency(start);
        Ok(())
    }

    async fn key_tap(&self, key: Key, duration_ms: Option<u32>) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        let key_name = Self::key_to_xdotool(key);
        Self::xdotool(&["key", key_name])?;
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
        // xdotool type --clearmodifiers uses XTEST to type each character.
        Self::xdotool(&["type", "--clearmodifiers", text])?;
        self.update_latency(start);
        Ok(())
    }

    async fn key_combo(&self, keys: &[Key]) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        // xdotool key accepts combo like "ctrl+shift+a"
        let combo: String = keys
            .iter()
            .map(|k| Self::key_to_xdotool(*k))
            .collect::<Vec<_>>()
            .join("+");
        Self::xdotool(&["key", &combo])?;
        self.update_latency(start);
        Ok(())
    }

    fn supports_background(&self) -> bool {
        true // xdotool --window works for background input on X11
    }

    fn last_latency_ms(&self) -> Option<f64> {
        *self.last_latency.lock().ok()?
    }

    fn mode(&self) -> InputMode {
        self.mode
    }
}

#[cfg(test)]
mod linux_pure_tests {
    use super::*;

    #[test]
    fn key_to_xdotool_letters() {
        assert_eq!(LinuxInput::key_to_xdotool(Key::A), "a");
        assert_eq!(LinuxInput::key_to_xdotool(Key::Z), "z");
    }

    #[test]
    fn key_to_xdotool_modifiers() {
        assert_eq!(LinuxInput::key_to_xdotool(Key::Control), "ctrl");
        assert_eq!(LinuxInput::key_to_xdotool(Key::Shift), "shift");
        assert_eq!(LinuxInput::key_to_xdotool(Key::Alt), "alt");
    }

    #[test]
    fn key_to_xdotool_navigation() {
        assert_eq!(LinuxInput::key_to_xdotool(Key::Up), "Up");
        assert_eq!(LinuxInput::key_to_xdotool(Key::Down), "Down");
        assert_eq!(LinuxInput::key_to_xdotool(Key::Escape), "Escape");
    }

    #[test]
    fn key_to_xdotool_fkeys() {
        assert_eq!(LinuxInput::key_to_xdotool(Key::F1), "F1");
        assert_eq!(LinuxInput::key_to_xdotool(Key::F12), "F12");
    }
}
