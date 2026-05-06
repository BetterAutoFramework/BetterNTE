//! betternte-input/src/adb.rs
//! ADB input engine

use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result as AnyhowResult;

use crate::controller::InputController;
use crate::error::{InputError, Result};
use crate::key::Key;
use crate::key::MouseButton;
use crate::mapper::KeyMapper;
use crate::target::InputTarget;

/// Maximum time we wait for an `adb shell ...` invocation before treating it
/// as hung. Keeps the engine from stalling forever if the device disconnects.
const ADB_COMMAND_TIMEOUT: Duration = Duration::from_millis(3000);

/// ADB input engine.
///
/// Connects to Android emulators via ADB and uses `adb shell input` commands
/// to simulate touch and key input.
#[derive(Debug)]
pub struct AdbInput {
    /// Device serial number (empty means the default device).
    device_serial: String,

    /// Key mapper used by [`InputController::parse_key`].
    key_mapper: KeyMapper,

    /// Last operation latency in milliseconds.
    last_latency: Arc<Mutex<Option<f64>>>,

    /// Whether initialized.
    initialized: bool,
}

impl AdbInput {
    /// Create a new ADB input engine.
    pub fn new(device_serial: String, key_mapper: KeyMapper) -> Self {
        Self {
            device_serial,
            key_mapper,
            last_latency: Arc::new(Mutex::new(None)),
            initialized: false,
        }
    }

    fn update_latency(&self, start: Instant) {
        if let Ok(mut guard) = self.last_latency.lock() {
            *guard = Some(start.elapsed().as_secs_f64() * 1000.0);
        }
    }

    /// Execute an `adb shell <args>` invocation safely.
    ///
    /// The arguments are passed through `Command::args` so the host shell is
    /// bypassed; on the device side `adb shell` re-quotes them, so each
    /// element should be a single argv slot (no embedded spaces unless that's
    /// what you want `input` to receive).
    async fn run_command(&self, args: &[&str]) -> Result<()> {
        let mut cmd = tokio::process::Command::new("adb");
        if !self.device_serial.is_empty() {
            cmd.arg("-s").arg(&self.device_serial);
        }
        cmd.arg("shell").args(args);

        let output = match tokio::time::timeout(ADB_COMMAND_TIMEOUT, cmd.output()).await {
            Ok(res) => res.map_err(|e| InputError::AdbInputFailed(e.to_string()))?,
            Err(_) => {
                return Err(InputError::AdbInputFailed(format!(
                    "adb shell timed out after {} ms",
                    ADB_COMMAND_TIMEOUT.as_millis()
                )));
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(InputError::AdbInputFailed(stderr.to_string()));
        }

        Ok(())
    }

    /// Default ADB serial for a MuMu emulator instance. MuMu exposes ADB on
    /// `127.0.0.1:7555 + 32 * index` for instance N (per official docs).
    fn mumu_default_serial(index: u32) -> String {
        let port = 7555u32.saturating_add(index.saturating_mul(32));
        format!("127.0.0.1:{port}")
    }

    /// Default ADB serial for an LDPlayer emulator instance.
    /// LDPlayer uses `127.0.0.1:5555 + 2 * index`.
    fn ld_default_serial(index: u32) -> String {
        let port = 5555u32.saturating_add(index.saturating_mul(2));
        format!("127.0.0.1:{port}")
    }

    /// Map generic key to Android keycode.
    fn to_android_keycode(&self, key: Key) -> Result<u32> {
        match key {
            Key::A => Ok(29),
            Key::B => Ok(30),
            Key::C => Ok(31),
            Key::D => Ok(32),
            Key::E => Ok(33),
            Key::F => Ok(34),
            Key::G => Ok(35),
            Key::H => Ok(36),
            Key::I => Ok(37),
            Key::J => Ok(38),
            Key::K => Ok(39),
            Key::L => Ok(40),
            Key::M => Ok(41),
            Key::N => Ok(42),
            Key::O => Ok(43),
            Key::P => Ok(44),
            Key::Q => Ok(45),
            Key::R => Ok(46),
            Key::S => Ok(47),
            Key::T => Ok(48),
            Key::U => Ok(49),
            Key::V => Ok(50),
            Key::W => Ok(51),
            Key::X => Ok(52),
            Key::Y => Ok(53),
            Key::Z => Ok(54),
            Key::Space => Ok(62),
            Key::Enter | Key::Return => Ok(66),
            Key::Escape => Ok(111),
            Key::Up => Ok(19),
            Key::Down => Ok(20),
            Key::Left => Ok(21),
            Key::Right => Ok(22),
            Key::Tab => Ok(61),
            Key::Backspace => Ok(67),
            Key::Home => Ok(3),
            Key::Delete => Ok(127),
            Key::End => Ok(123),
            Key::PageUp => Ok(92),
            Key::PageDown => Ok(93),
            Key::Num0 => Ok(7),
            Key::Num1 => Ok(8),
            Key::Num2 => Ok(9),
            Key::Num3 => Ok(10),
            Key::Num4 => Ok(11),
            Key::Num5 => Ok(12),
            Key::Num6 => Ok(13),
            Key::Num7 => Ok(14),
            Key::Num8 => Ok(15),
            Key::Num9 => Ok(16),
            Key::Numpad0 => Ok(144),
            Key::Numpad1 => Ok(145),
            Key::Numpad2 => Ok(146),
            Key::Numpad3 => Ok(147),
            Key::Numpad4 => Ok(148),
            Key::Numpad5 => Ok(149),
            Key::Numpad6 => Ok(150),
            Key::Numpad7 => Ok(151),
            Key::Numpad8 => Ok(152),
            Key::Numpad9 => Ok(153),
            Key::NumpadAdd => Ok(157),
            Key::NumpadSubtract => Ok(156),
            Key::NumpadMultiply => Ok(155),
            Key::NumpadDivide => Ok(158),
            Key::NumpadDecimal => Ok(159),
            Key::CapsLock => Ok(115),
            Key::NumLock => Ok(143),
            Key::ScrollLock => Ok(116),
            Key::PrintScreen => Ok(120),
            Key::Pause => Ok(119),
            Key::F1 => Ok(131),
            Key::F2 => Ok(132),
            Key::F3 => Ok(133),
            Key::F4 => Ok(134),
            Key::F5 => Ok(135),
            Key::F6 => Ok(136),
            Key::F7 => Ok(137),
            Key::F8 => Ok(138),
            Key::F9 => Ok(139),
            Key::F10 => Ok(140),
            Key::F11 => Ok(141),
            Key::F12 => Ok(142),
            _ => Err(InputError::InvalidKey(format!("{:?}", key))),
        }
    }
}

#[async_trait]
impl InputController for AdbInput {
    fn name(&self) -> &str {
        "ADB"
    }

    fn parse_key(&self, name: &str) -> Option<Key> {
        self.key_mapper.map_key(name).ok()
    }

    async fn init(&mut self, target: &InputTarget) -> AnyhowResult<()> {
        match target {
            InputTarget::AdbDevice { serial } => {
                self.device_serial = serial.clone();
                self.initialized = true;
            }
            InputTarget::MumuEmulator { index } => {
                if self.device_serial.is_empty() {
                    self.device_serial = Self::mumu_default_serial(*index);
                }
                self.initialized = true;
            }
            InputTarget::LdEmulator { index } => {
                if self.device_serial.is_empty() {
                    self.device_serial = Self::ld_default_serial(*index);
                }
                self.initialized = true;
            }
            _ => {
                return Err(InputError::SimulationFailed(
                    "ADB engine only supports ADB devices".into(),
                )
                .into())
            }
        }
        Ok(())
    }

    async fn click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        self.run_command(&["input", "tap", &x.to_string(), &y.to_string()])
            .await?;
        self.update_latency(start);
        Ok(())
    }

    async fn double_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        self.click(x, y).await?;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        self.click(x, y).await
    }

    async fn right_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        // Android has no real "right click" — emulate by long-pressing in
        // place. Documented in the trait so callers don't expect a context
        // menu to appear automatically.
        let start = Instant::now();
        let res = self
            .run_command(&[
                "input",
                "swipe",
                &x.to_string(),
                &y.to_string(),
                &x.to_string(),
                &y.to_string(),
                "500",
            ])
            .await;
        self.update_latency(start);
        res.map_err(Into::into)
    }

    async fn mouse_move(&self, _x: i32, _y: i32) -> AnyhowResult<()> {
        // ADB has no notion of an independent cursor — moves only happen as
        // part of a swipe gesture. Surface this as an explicit error so the
        // caller can branch instead of silently no-oping.
        Err(
            InputError::SimulationFailed("mouse_move is not supported on ADB; use swipe()".into())
                .into(),
        )
    }

    async fn mouse_down(&self, _button: MouseButton) -> AnyhowResult<()> {
        Err(
            InputError::SimulationFailed("mouse_down is not supported on ADB; use swipe()".into())
                .into(),
        )
    }

    async fn mouse_up(&self, _button: MouseButton) -> AnyhowResult<()> {
        Err(
            InputError::SimulationFailed("mouse_up is not supported on ADB; use swipe()".into())
                .into(),
        )
    }

    async fn mouse_scroll(&self, delta: i32) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        if delta == 0 {
            return Ok(());
        }
        // Emulate scrolling with a vertical swipe centred at (500, 500).
        // Clamp the destination within a sensible range so large deltas
        // don't produce off-screen swipes.
        let y_offset = (delta * 100).clamp(-400, 400);
        let y_start = 500i32;
        let y_end = (y_start - y_offset).clamp(50, 950);
        let start = Instant::now();
        let res = self
            .run_command(&[
                "input",
                "swipe",
                "500",
                &y_start.to_string(),
                "500",
                &y_end.to_string(),
                "100",
            ])
            .await;
        self.update_latency(start);
        res.map_err(Into::into)
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
        let res = self
            .run_command(&[
                "input",
                "swipe",
                &x1.to_string(),
                &y1.to_string(),
                &x2.to_string(),
                &y2.to_string(),
                &duration_ms.to_string(),
            ])
            .await;
        self.update_latency(start);
        res.map_err(Into::into)
    }

    async fn key_press(&self, key: Key) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        let code = self.to_android_keycode(key)?;
        self.run_command(&["input", "keyevent", &code.to_string()])
            .await?;
        self.update_latency(start);
        Ok(())
    }

    async fn key_release(&self, _key: Key) -> AnyhowResult<()> {
        // `adb input keyevent` is atomic press+release; there is no separate
        // release. Reporting Ok would let callers think they can balance
        // presses with releases on Android, which they can't.
        Err(InputError::SimulationFailed(
            "key_release is not supported on ADB; key_press already releases".into(),
        )
        .into())
    }

    async fn key_tap(&self, key: Key, _duration_ms: Option<u32>) -> AnyhowResult<()> {
        self.key_press(key).await
    }

    async fn type_text(&self, text: &str) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        // `adb shell input text <arg>` interprets `%s` as a literal space and
        // doesn't accept Unicode out of the box. Escape spaces and basic
        // shell metachars so the device-side `sh` doesn't tokenize the text.
        let escaped = encode_adb_input_text(text);
        let start = Instant::now();
        let res = self.run_command(&["input", "text", &escaped]).await;
        self.update_latency(start);
        res.map_err(Into::into)
    }

    async fn key_combo(&self, keys: &[Key]) -> AnyhowResult<()> {
        // ADB has no true combo support; the closest equivalent on a real
        // shell would be `input keycombination`, but it isn't available on
        // every Android version. Press each key sequentially as a best
        // effort and bail on the first failure.
        for &key in keys {
            self.key_press(key).await?;
        }
        Ok(())
    }

    fn supports_background(&self) -> bool {
        true
    }

    fn last_latency_ms(&self) -> Option<f64> {
        self.last_latency.lock().ok().and_then(|g| *g)
    }

    fn mode(&self) -> crate::config::InputMode {
        // ADB always works regardless of window focus
        crate::config::InputMode::Background
    }
}

/// Encode a string for `adb shell input text <arg>`.
///
/// `input text` on Android does not parse the argument as UTF-8 reliably and
/// treats `%s` as a literal space marker. Spaces in the argument would also
/// be tokenized by the device-side shell. We therefore:
///
/// 1. Replace ASCII space with `%s` (the documented placeholder).
/// 2. Backslash-escape characters that the device-side `sh` would otherwise
///    interpret (`'`, `"`, `\\`, `` ` ``, `$`, `(`, `)`, `&`, `;`, `<`, `>`,
///    `|`, `*`, `?`, `[`, `]`, `~`, `#`).
///
/// Non-ASCII characters are passed through untouched; on most modern Android
/// devices `input text` then accepts them via the IME path. For wide
/// compatibility callers should fall back to per-character `key_press` for
/// control characters.
pub(crate) fn encode_adb_input_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 4);
    for ch in text.chars() {
        match ch {
            ' ' => out.push_str("%s"),
            '\'' | '"' | '\\' | '`' | '$' | '(' | ')' | '&' | ';' | '<' | '>' | '|' | '*' | '?'
            | '[' | ']' | '~' | '#' => {
                out.push('\\');
                out.push(ch);
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod adb_unit_tests {
    use super::*;

    #[test]
    fn mumu_default_serial_uses_documented_port_layout() {
        assert_eq!(AdbInput::mumu_default_serial(0), "127.0.0.1:7555");
        assert_eq!(AdbInput::mumu_default_serial(1), "127.0.0.1:7587");
        assert_eq!(AdbInput::mumu_default_serial(2), "127.0.0.1:7619");
    }

    #[test]
    fn ld_default_serial_uses_documented_port_layout() {
        assert_eq!(AdbInput::ld_default_serial(0), "127.0.0.1:5555");
        assert_eq!(AdbInput::ld_default_serial(1), "127.0.0.1:5557");
        assert_eq!(AdbInput::ld_default_serial(3), "127.0.0.1:5561");
    }

    #[test]
    fn encode_adb_input_text_escapes_spaces_and_metachars() {
        assert_eq!(encode_adb_input_text("hello world"), "hello%sworld");
        assert_eq!(encode_adb_input_text("a&b"), "a\\&b");
        assert_eq!(encode_adb_input_text("foo\"bar"), "foo\\\"bar");
        // Spaces are replaced by %s (the documented `input text` placeholder).
        assert_eq!(encode_adb_input_text("$(rm -rf)"), "\\$\\(rm%s-rf\\)");
        assert_eq!(encode_adb_input_text(""), "");
        // Plain ASCII and Unicode pass through unchanged.
        assert_eq!(encode_adb_input_text("Hello"), "Hello");
        assert_eq!(encode_adb_input_text("你好"), "你好");
    }
}
