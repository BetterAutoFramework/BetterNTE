//! 输入控制器 trait 和相关类型。
//!
//! 定义 `InputController` trait、`Key`、`MouseButton`、`InputTarget`、`InputMode`，
//! 使输入实现与使用方解耦。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 输入模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    /// 自动模式（前台优先，失败时回退后台）
    Auto,
    /// 前台输入（SendInput，需要窗口激活）
    Foreground,
    /// 后台输入（PostMessage，不需要窗口激活）
    Background,
}

impl std::fmt::Display for InputMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "自动模式"),
            Self::Foreground => write!(f, "前台输入"),
            Self::Background => write!(f, "后台输入"),
        }
    }
}

/// 前台输入后端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForegroundInputBackend {
    /// Use `enigo` for system-level foreground input.
    #[serde(alias = "win_auto_utils")]
    Enigo,
}

impl std::fmt::Display for ForegroundInputBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Enigo => write!(f, "enigo"),
        }
    }
}

/// 输入目标。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "params")]
pub enum InputTarget {
    /// PC 原生窗口（前台输入）
    NativeWindow { hwnd: u64 },
    /// PC 原生窗口（后台输入，PostMessage）
    NativeWindowBackground { hwnd: u64 },
    /// ADB 设备（Android 模拟器）
    AdbDevice { serial: String },
    /// MuMu 模拟器
    MumuEmulator { index: u32 },
    /// 雷电模拟器
    LdEmulator { index: u32 },
}

/// 鼠标按钮。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

impl Default for MouseButton {
    fn default() -> Self {
        Self::Left
    }
}

/// Failed to parse a [`Key`] name string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseKeyError;

impl std::fmt::Display for ParseKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown keyboard key")
    }
}

impl std::error::Error for ParseKeyError {}

/// 键盘按键枚举。
///
/// 完整的 Windows 虚拟键码映射，用于输入控制器和按键映射。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    // === 字母键 ===
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    // === 数字键 ===
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,

    // === 功能键 ===
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,

    // === 修饰键 ===
    Shift,
    Control,
    Alt,
    LShift,
    RShift,
    LControl,
    RControl,
    LAlt,
    RAlt,

    // === 导航键 ===
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,

    // === 编辑键 ===
    Backspace,
    Tab,
    Enter,
    Return,
    Delete,
    Insert,
    Escape,
    Space,

    // === OEM 标点键 ===
    Oem1,      // ;:
    OemPlus,   // =+
    OemComma,  // ,<
    OemMinus,  // -_
    OemPeriod, // .>
    Oem2,      // /?
    Oem3,      // `~
    Oem4,      // [{
    Oem5,      // \|
    Oem6,      // ]}
    Oem7,      // '"

    // === 小键盘 ===
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadDecimal,
    NumpadEnter,

    // === 系统键 ===
    PrintScreen,
    ScrollLock,
    Pause,
    CapsLock,
    NumLock,
    LWin,
    RWin,
    Apps,

    // === 媒体键 ===
    VolumeMute,
    VolumeDown,
    VolumeUp,
    MediaNextTrack,
    MediaPrevTrack,
    MediaStop,
    MediaPlayPause,

    // === 浏览器键 ===
    BrowserBack,
    BrowserForward,
    BrowserRefresh,
    BrowserStop,
    BrowserSearch,
    BrowserFavorites,
    BrowserHome,
}

impl Key {
    fn parse_key(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            // 字母键
            "VK_A" | "A" => Some(Key::A),
            "VK_B" | "B" => Some(Key::B),
            "VK_C" | "C" => Some(Key::C),
            "VK_D" | "D" => Some(Key::D),
            "VK_E" | "E" => Some(Key::E),
            "VK_F" | "F" => Some(Key::F),
            "VK_G" | "G" => Some(Key::G),
            "VK_H" | "H" => Some(Key::H),
            "VK_I" | "I" => Some(Key::I),
            "VK_J" | "J" => Some(Key::J),
            "VK_K" | "K" => Some(Key::K),
            "VK_L" | "L" => Some(Key::L),
            "VK_M" | "M" => Some(Key::M),
            "VK_N" | "N" => Some(Key::N),
            "VK_O" | "O" => Some(Key::O),
            "VK_P" | "P" => Some(Key::P),
            "VK_Q" | "Q" => Some(Key::Q),
            "VK_R" | "R" => Some(Key::R),
            "VK_S" | "S" => Some(Key::S),
            "VK_T" | "T" => Some(Key::T),
            "VK_U" | "U" => Some(Key::U),
            "VK_V" | "V" => Some(Key::V),
            "VK_W" | "W" => Some(Key::W),
            "VK_X" | "X" => Some(Key::X),
            "VK_Y" | "Y" => Some(Key::Y),
            "VK_Z" | "Z" => Some(Key::Z),

            // 数字键
            "VK_0" | "0" => Some(Key::Num0),
            "VK_1" | "1" => Some(Key::Num1),
            "VK_2" | "2" => Some(Key::Num2),
            "VK_3" | "3" => Some(Key::Num3),
            "VK_4" | "4" => Some(Key::Num4),
            "VK_5" | "5" => Some(Key::Num5),
            "VK_6" | "6" => Some(Key::Num6),
            "VK_7" | "7" => Some(Key::Num7),
            "VK_8" | "8" => Some(Key::Num8),
            "VK_9" | "9" => Some(Key::Num9),

            // 功能键
            "VK_F1" | "F1" => Some(Key::F1),
            "VK_F2" | "F2" => Some(Key::F2),
            "VK_F3" | "F3" => Some(Key::F3),
            "VK_F4" | "F4" => Some(Key::F4),
            "VK_F5" | "F5" => Some(Key::F5),
            "VK_F6" | "F6" => Some(Key::F6),
            "VK_F7" | "F7" => Some(Key::F7),
            "VK_F8" | "F8" => Some(Key::F8),
            "VK_F9" | "F9" => Some(Key::F9),
            "VK_F10" | "F10" => Some(Key::F10),
            "VK_F11" | "F11" => Some(Key::F11),
            "VK_F12" | "F12" => Some(Key::F12),

            // 空格
            "VK_SPACE" | "SPACE" => Some(Key::Space),

            // 回车
            "VK_RETURN" | "ENTER" | "RETURN" => Some(Key::Return),

            // Escape
            "VK_ESCAPE" | "ESC" | "ESCAPE" => Some(Key::Escape),

            // 修饰键
            "VK_SHIFT" | "SHIFT" => Some(Key::Shift),
            "VK_CONTROL" | "CTRL" | "CONTROL" => Some(Key::Control),
            "VK_ALT" | "ALT" => Some(Key::Alt),

            // Tab
            "VK_TAB" | "TAB" => Some(Key::Tab),

            // Backspace
            "VK_BACK" | "BACKSPACE" => Some(Key::Backspace),

            // Delete
            "VK_DELETE" | "DEL" | "DELETE" => Some(Key::Delete),

            // 导航键
            "VK_UP" | "UP" => Some(Key::Up),
            "VK_DOWN" | "DOWN" => Some(Key::Down),
            "VK_LEFT" | "LEFT" => Some(Key::Left),
            "VK_RIGHT" | "RIGHT" => Some(Key::Right),
            "VK_HOME" | "HOME" => Some(Key::Home),
            "VK_END" | "END" => Some(Key::End),
            "VK_PRIOR" | "PAGEUP" | "PAGE_UP" => Some(Key::PageUp),
            "VK_NEXT" | "PAGEDOWN" | "PAGE_DOWN" => Some(Key::PageDown),

            // Insert
            "VK_INSERT" | "INSERT" => Some(Key::Insert),

            // CapsLock
            "VK_CAPITAL" | "CAPSLOCK" | "CAPS" => Some(Key::CapsLock),

            // NumLock
            "VK_NUMLOCK" | "NUMLOCK" => Some(Key::NumLock),

            // ScrollLock
            "VK_SCROLL" | "SCROLLLOCK" | "SCROLL" => Some(Key::ScrollLock),

            // PrintScreen
            "VK_SNAPSHOT" | "PRINTSCREEN" | "PRINT" => Some(Key::PrintScreen),

            // Pause
            "VK_PAUSE" | "PAUSE" => Some(Key::Pause),

            // LWin / RWin
            "VK_LWIN" | "LWIN" => Some(Key::LWin),
            "VK_RWIN" | "RWIN" => Some(Key::RWin),

            // Apps
            "VK_APPS" | "APPS" => Some(Key::Apps),

            // 小键盘
            "VK_NUMPAD0" | "NUMPAD0" => Some(Key::Numpad0),
            "VK_NUMPAD1" | "NUMPAD1" => Some(Key::Numpad1),
            "VK_NUMPAD2" | "NUMPAD2" => Some(Key::Numpad2),
            "VK_NUMPAD3" | "NUMPAD3" => Some(Key::Numpad3),
            "VK_NUMPAD4" | "NUMPAD4" => Some(Key::Numpad4),
            "VK_NUMPAD5" | "NUMPAD5" => Some(Key::Numpad5),
            "VK_NUMPAD6" | "NUMPAD6" => Some(Key::Numpad6),
            "VK_NUMPAD7" | "NUMPAD7" => Some(Key::Numpad7),
            "VK_NUMPAD8" | "NUMPAD8" => Some(Key::Numpad8),
            "VK_NUMPAD9" | "NUMPAD9" => Some(Key::Numpad9),
            "VK_ADD" | "NUMPADADD" => Some(Key::NumpadAdd),
            "VK_SUBTRACT" | "NUMPADSUBTRACT" => Some(Key::NumpadSubtract),
            "VK_MULTIPLY" | "NUMPADMULTIPLY" => Some(Key::NumpadMultiply),
            "VK_DIVIDE" | "NUMPADDIVIDE" => Some(Key::NumpadDivide),
            "VK_DECIMAL" | "NUMPADDECIMAL" => Some(Key::NumpadDecimal),
            "VK_NUMPADENTER" | "NUMPADENTER" => Some(Key::NumpadEnter),

            // OEM 键
            "VK_OEM_1" | "OEM1" | ";" => Some(Key::Oem1),
            "VK_OEM_PLUS" | "OEMPLUS" | "+" => Some(Key::OemPlus),
            "VK_OEM_COMMA" | "OEMCOMMA" | "," => Some(Key::OemComma),
            "VK_OEM_MINUS" | "OEMMINUS" | "-" => Some(Key::OemMinus),
            "VK_OEM_PERIOD" | "OEMPERIOD" | "." => Some(Key::OemPeriod),
            "VK_OEM_2" | "OEM2" | "/" => Some(Key::Oem2),
            "VK_OEM_3" | "OEM3" | "`" => Some(Key::Oem3),
            "VK_OEM_4" | "OEM4" | "[" => Some(Key::Oem4),
            "VK_OEM_5" | "OEM5" | "\\" => Some(Key::Oem5),
            "VK_OEM_6" | "OEM6" | "]" => Some(Key::Oem6),
            "VK_OEM_7" | "OEM7" | "'" => Some(Key::Oem7),

            _ => None,
        }
    }

    /// Parse a key name (same rules as [`FromStr`] for [`Key`]).
    #[must_use]
    pub fn try_parse(s: &str) -> Option<Self> {
        Self::parse_key(s)
    }
}

impl std::str::FromStr for Key {
    type Err = ParseKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_key(s).ok_or(ParseKeyError)
    }
}

/// 输入控制器 trait。
///
/// 所有输入引擎（Win32、ADB）实现此 trait。
/// 提供统一的鼠标、键盘、触控输入接口。
#[async_trait]
pub trait InputController: Send + Sync {
    /// 引擎名称（如 "Win32"、"ADB"）
    fn name(&self) -> &str;

    /// 将字符串按键名解析为 [`Key`]。
    ///
    /// 默认实现等价于 [`Key::try_parse`]，子类可覆写以应用自定义 `KeyMapper`
    /// 绑定（例如 `attack -> A`），从而让脚本/UI 中的逻辑按键名生效。
    fn parse_key(&self, name: &str) -> Option<Key> {
        Key::try_parse(name)
    }

    /// 初始化输入控制器。
    async fn init(&mut self, target: &InputTarget) -> anyhow::Result<()>;

    // === 鼠标操作 ===

    /// 移动鼠标到指定位置。
    async fn mouse_move(&self, x: i32, y: i32) -> anyhow::Result<()>;

    /// 左键单击。
    async fn click(&self, x: i32, y: i32) -> anyhow::Result<()>;

    /// 双击。
    async fn double_click(&self, x: i32, y: i32) -> anyhow::Result<()>;

    /// 右键单击。
    async fn right_click(&self, x: i32, y: i32) -> anyhow::Result<()>;

    /// 按下鼠标按钮。
    async fn mouse_down(&self, button: MouseButton) -> anyhow::Result<()>;

    /// 释放鼠标按钮。
    async fn mouse_up(&self, button: MouseButton) -> anyhow::Result<()>;

    /// 鼠标滚轮。
    async fn mouse_scroll(&self, delta: i32) -> anyhow::Result<()>;

    /// 滑动操作。
    async fn swipe(
        &self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        duration_ms: u32,
    ) -> anyhow::Result<()>;

    // === 键盘操作 ===

    /// 按下按键（不释放）。
    async fn key_press(&self, key: Key) -> anyhow::Result<()>;

    /// 释放按键。
    async fn key_release(&self, key: Key) -> anyhow::Result<()>;

    /// 敲击按键（按下 + 释放）。
    async fn key_tap(&self, key: Key, duration_ms: Option<u32>) -> anyhow::Result<()>;

    /// 输入文本。
    async fn type_text(&self, text: &str) -> anyhow::Result<()>;

    /// 组合键（如 Ctrl+C）。
    async fn key_combo(&self, keys: &[Key]) -> anyhow::Result<()>;

    // === 状态 ===

    /// 是否支持后台输入。
    fn supports_background(&self) -> bool;

    /// 最近一次操作延迟（毫秒）。
    fn last_latency_ms(&self) -> Option<f64>;

    /// 获取输入模式。
    fn mode(&self) -> InputMode;
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Key::try_parse — letters ──

    #[test]
    fn test_key_letters_uppercase() {
        assert_eq!(Key::try_parse("A"), Some(Key::A));
        assert_eq!(Key::try_parse("Z"), Some(Key::Z));
    }

    #[test]
    fn test_key_letters_lowercase() {
        assert_eq!(Key::try_parse("a"), Some(Key::A));
        assert_eq!(Key::try_parse("z"), Some(Key::Z));
    }

    #[test]
    fn test_key_letters_vk_prefix() {
        assert_eq!(Key::try_parse("VK_A"), Some(Key::A));
        assert_eq!(Key::try_parse("vk_a"), Some(Key::A));
        assert_eq!(Key::try_parse("VK_Z"), Some(Key::Z));
    }

    // ── Key::try_parse — digits ──

    #[test]
    fn test_key_digits() {
        assert_eq!(Key::try_parse("0"), Some(Key::Num0));
        assert_eq!(Key::try_parse("9"), Some(Key::Num9));
        assert_eq!(Key::try_parse("VK_5"), Some(Key::Num5));
    }

    // ── Key::try_parse — F-keys ──

    #[test]
    fn test_key_fkeys() {
        assert_eq!(Key::try_parse("F1"), Some(Key::F1));
        assert_eq!(Key::try_parse("F12"), Some(Key::F12));
        assert_eq!(Key::try_parse("VK_F1"), Some(Key::F1));
        assert_eq!(Key::try_parse("VK_F12"), Some(Key::F12));
    }

    // ── Key::try_parse — modifiers ──

    #[test]
    fn test_key_modifiers() {
        assert_eq!(Key::try_parse("SHIFT"), Some(Key::Shift));
        assert_eq!(Key::try_parse("VK_SHIFT"), Some(Key::Shift));
        assert_eq!(Key::try_parse("CTRL"), Some(Key::Control));
        assert_eq!(Key::try_parse("CONTROL"), Some(Key::Control));
        assert_eq!(Key::try_parse("VK_CONTROL"), Some(Key::Control));
        assert_eq!(Key::try_parse("ALT"), Some(Key::Alt));
        assert_eq!(Key::try_parse("VK_ALT"), Some(Key::Alt));
    }

    // ── Key::try_parse — aliases ──

    #[test]
    fn test_key_enter_aliases() {
        assert_eq!(Key::try_parse("ENTER"), Some(Key::Return));
        assert_eq!(Key::try_parse("RETURN"), Some(Key::Return));
        assert_eq!(Key::try_parse("VK_RETURN"), Some(Key::Return));
    }

    #[test]
    fn test_key_escape_aliases() {
        assert_eq!(Key::try_parse("ESC"), Some(Key::Escape));
        assert_eq!(Key::try_parse("ESCAPE"), Some(Key::Escape));
        assert_eq!(Key::try_parse("VK_ESCAPE"), Some(Key::Escape));
    }

    #[test]
    fn test_key_delete_aliases() {
        assert_eq!(Key::try_parse("DEL"), Some(Key::Delete));
        assert_eq!(Key::try_parse("DELETE"), Some(Key::Delete));
        assert_eq!(Key::try_parse("VK_DELETE"), Some(Key::Delete));
    }

    #[test]
    fn test_key_page_aliases() {
        assert_eq!(Key::try_parse("PAGEUP"), Some(Key::PageUp));
        assert_eq!(Key::try_parse("PAGE_UP"), Some(Key::PageUp));
        assert_eq!(Key::try_parse("VK_PRIOR"), Some(Key::PageUp));
        assert_eq!(Key::try_parse("PAGEDOWN"), Some(Key::PageDown));
        assert_eq!(Key::try_parse("PAGE_DOWN"), Some(Key::PageDown));
        assert_eq!(Key::try_parse("VK_NEXT"), Some(Key::PageDown));
    }

    // ── Key::try_parse — navigation ──

    #[test]
    fn test_key_navigation() {
        assert_eq!(Key::try_parse("UP"), Some(Key::Up));
        assert_eq!(Key::try_parse("DOWN"), Some(Key::Down));
        assert_eq!(Key::try_parse("LEFT"), Some(Key::Left));
        assert_eq!(Key::try_parse("RIGHT"), Some(Key::Right));
        assert_eq!(Key::try_parse("HOME"), Some(Key::Home));
        assert_eq!(Key::try_parse("END"), Some(Key::End));
    }

    // ── Key::try_parse — special keys ──

    #[test]
    fn test_key_special() {
        assert_eq!(Key::try_parse("SPACE"), Some(Key::Space));
        assert_eq!(Key::try_parse("VK_SPACE"), Some(Key::Space));
        assert_eq!(Key::try_parse("TAB"), Some(Key::Tab));
        assert_eq!(Key::try_parse("BACKSPACE"), Some(Key::Backspace));
        assert_eq!(Key::try_parse("VK_BACK"), Some(Key::Backspace));
        assert_eq!(Key::try_parse("INSERT"), Some(Key::Insert));
        assert_eq!(Key::try_parse("CAPSLOCK"), Some(Key::CapsLock));
        assert_eq!(Key::try_parse("NUMLOCK"), Some(Key::NumLock));
        assert_eq!(Key::try_parse("PRINTSCREEN"), Some(Key::PrintScreen));
        assert_eq!(Key::try_parse("PAUSE"), Some(Key::Pause));
    }

    // ── Key::try_parse — numpad ──

    #[test]
    fn test_key_numpad() {
        assert_eq!(Key::try_parse("NUMPAD0"), Some(Key::Numpad0));
        assert_eq!(Key::try_parse("NUMPAD9"), Some(Key::Numpad9));
        assert_eq!(Key::try_parse("NUMPADADD"), Some(Key::NumpadAdd));
        assert_eq!(Key::try_parse("NUMPADSUBTRACT"), Some(Key::NumpadSubtract));
        assert_eq!(Key::try_parse("NUMPADMULTIPLY"), Some(Key::NumpadMultiply));
        assert_eq!(Key::try_parse("NUMPADDIVIDE"), Some(Key::NumpadDivide));
        assert_eq!(Key::try_parse("NUMPADDECIMAL"), Some(Key::NumpadDecimal));
        assert_eq!(Key::try_parse("NUMPADENTER"), Some(Key::NumpadEnter));
    }

    // ── Key::try_parse — OEM keys ──

    #[test]
    fn test_key_oem_symbols() {
        assert_eq!(Key::try_parse(";"), Some(Key::Oem1));
        assert_eq!(Key::try_parse("+"), Some(Key::OemPlus));
        assert_eq!(Key::try_parse(","), Some(Key::OemComma));
        assert_eq!(Key::try_parse("-"), Some(Key::OemMinus));
        assert_eq!(Key::try_parse("."), Some(Key::OemPeriod));
        assert_eq!(Key::try_parse("/"), Some(Key::Oem2));
        assert_eq!(Key::try_parse("`"), Some(Key::Oem3));
        assert_eq!(Key::try_parse("["), Some(Key::Oem4));
        assert_eq!(Key::try_parse("\\"), Some(Key::Oem5));
        assert_eq!(Key::try_parse("]"), Some(Key::Oem6));
        assert_eq!(Key::try_parse("'"), Some(Key::Oem7));
    }

    #[test]
    fn test_key_oem_vk_prefix() {
        assert_eq!(Key::try_parse("VK_OEM_1"), Some(Key::Oem1));
        assert_eq!(Key::try_parse("VK_OEM_PLUS"), Some(Key::OemPlus));
        assert_eq!(Key::try_parse("VK_OEM_MINUS"), Some(Key::OemMinus));
    }

    // ── Key::try_parse — window keys ──

    #[test]
    fn test_key_win_keys() {
        assert_eq!(Key::try_parse("LWIN"), Some(Key::LWin));
        assert_eq!(Key::try_parse("RWIN"), Some(Key::RWin));
        assert_eq!(Key::try_parse("APPS"), Some(Key::Apps));
    }

    // ── Key::try_parse — edge cases ──

    #[test]
    fn test_key_invalid_returns_none() {
        assert_eq!(Key::try_parse(""), None);
        assert_eq!(Key::try_parse("BANANA"), None);
        assert_eq!(Key::try_parse("VK_INVALID"), None);
    }

    // ── InputMode ──

    #[test]
    fn test_input_mode_display() {
        assert_eq!(InputMode::Auto.to_string(), "自动模式");
        assert_eq!(InputMode::Foreground.to_string(), "前台输入");
        assert_eq!(InputMode::Background.to_string(), "后台输入");
    }

    #[test]
    fn test_input_mode_serde_roundtrip() {
        let modes = vec![
            InputMode::Auto,
            InputMode::Foreground,
            InputMode::Background,
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let back: InputMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, back);
        }
    }

    // ── MouseButton ──

    #[test]
    fn test_mouse_button_default() {
        assert_eq!(MouseButton::default(), MouseButton::Left);
    }

    #[test]
    fn test_mouse_button_serde_roundtrip() {
        let buttons = vec![MouseButton::Left, MouseButton::Right, MouseButton::Middle];
        for btn in buttons {
            let json = serde_json::to_string(&btn).unwrap();
            let back: MouseButton = serde_json::from_str(&json).unwrap();
            assert_eq!(btn, back);
        }
    }
}
