//! betternte-input/src/win32.rs
//! Win32 input engine

use async_trait::async_trait;
use enigo::{Axis, Button, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result as AnyhowResult;

use crate::config::{ForegroundInputBackend, InputMode};
use crate::controller::InputController;
use crate::error::{InputError, Result};
use crate::key::Key;
use crate::key::MouseButton;
use crate::mapper::KeyMapper;
use crate::target::InputTarget;

// === Win32 mouse virtual-key flags (MK_*) packed into wParam for WM_*BUTTON*. ===
const MK_LBUTTON: usize = 0x0001;
const MK_RBUTTON: usize = 0x0002;
const MK_MBUTTON: usize = 0x0010;
const MK_XBUTTON1: usize = 0x0020;
const MK_XBUTTON2: usize = 0x0040;

/// Win32 input engine.
///
/// Uses enigo for foreground input (SendInput) and PostMessage for background input.
/// Supports all PC native window keyboard and mouse operations.
#[derive(Debug)]
pub struct Win32Input {
    /// Enigo instance (foreground mode) - wrapped in Mutex for interior mutability
    enigo: Arc<Mutex<Option<Enigo>>>,

    /// Window handle (background mode uses PostMessage)
    hwnd: Option<u64>,

    /// Input mode
    mode: InputMode,

    /// Key mapper used by [`Self::parse_key_name`].
    key_mapper: KeyMapper,

    /// Last operation latency in milliseconds.
    last_latency: Arc<Mutex<Option<f64>>>,

    /// Whether initialized.
    initialized: bool,

    /// Last known cursor position in client-area coordinates.
    /// Used by background `mouse_down/up` so the WM_*BUTTON* lParam carries
    /// a meaningful (x, y) instead of (0, 0).
    last_pos: Arc<Mutex<(i32, i32)>>,

    /// Token for the currently active swipe.
    ///
    /// Each call to [`InputController::swipe`] obtains a fresh token; calling
    /// [`Self::cancel_swipe`] only cancels swipes whose token is older than
    /// or equal to the value stored here, avoiding races where a "cancel"
    /// emitted slightly before a new swipe accidentally killed the new one.
    swipe_token: Arc<AtomicU64>,

    /// Cancellation watermark — swipes with token <= this value must abort.
    swipe_cancel_at: Arc<AtomicU64>,

    /// Tracks held foreground keys/mouse buttons so [`Drop`] can release them
    /// even if a swipe / hold is interrupted by panic or shutdown.
    held_keys: Arc<Mutex<HashSet<u16>>>,
    held_buttons: Arc<Mutex<HashSet<MouseButton>>>,
}

impl Win32Input {
    /// Create a new Win32 input engine.
    pub fn new(key_mapper: KeyMapper) -> Self {
        Self {
            enigo: Arc::new(Mutex::new(None)),
            hwnd: None,
            mode: InputMode::Foreground,
            key_mapper,
            last_latency: Arc::new(Mutex::new(None)),
            initialized: false,
            last_pos: Arc::new(Mutex::new((0, 0))),
            swipe_token: Arc::new(AtomicU64::new(0)),
            swipe_cancel_at: Arc::new(AtomicU64::new(0)),
            held_keys: Arc::new(Mutex::new(HashSet::new())),
            held_buttons: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Create a new Win32 input engine.
    ///
    /// The `foreground_backend` argument is currently informational — only
    /// the `enigo` backend is wired up. Future alternative backends (e.g.
    /// raw `SendInput` without `enigo`) should branch on this value.
    pub fn new_with_backend(
        key_mapper: KeyMapper,
        foreground_backend: ForegroundInputBackend,
    ) -> Self {
        // Single match arm today; ensures the compiler errors on this site
        // when a new variant is added so it can't silently fall through.
        match foreground_backend {
            ForegroundInputBackend::Enigo => {}
        }
        Self::new(key_mapper)
    }

    /// Mark all in-flight swipes as cancelled.
    pub fn cancel_swipe(&self) {
        let cur = self.swipe_token.load(Ordering::SeqCst);
        self.swipe_cancel_at.store(cur, Ordering::SeqCst);
    }

    fn convert_key(&self, key: Key) -> Result<u16> {
        // Map our Key enum to Windows virtual key codes.
        let vk = match key {
            // Letters
            Key::A => 0x41,
            Key::B => 0x42,
            Key::C => 0x43,
            Key::D => 0x44,
            Key::E => 0x45,
            Key::F => 0x46,
            Key::G => 0x47,
            Key::H => 0x48,
            Key::I => 0x49,
            Key::J => 0x4A,
            Key::K => 0x4B,
            Key::L => 0x4C,
            Key::M => 0x4D,
            Key::N => 0x4E,
            Key::O => 0x4F,
            Key::P => 0x50,
            Key::Q => 0x51,
            Key::R => 0x52,
            Key::S => 0x53,
            Key::T => 0x54,
            Key::U => 0x55,
            Key::V => 0x56,
            Key::W => 0x57,
            Key::X => 0x58,
            Key::Y => 0x59,
            Key::Z => 0x5A,
            // Top-row digits
            Key::Num0 => 0x30,
            Key::Num1 => 0x31,
            Key::Num2 => 0x32,
            Key::Num3 => 0x33,
            Key::Num4 => 0x34,
            Key::Num5 => 0x35,
            Key::Num6 => 0x36,
            Key::Num7 => 0x37,
            Key::Num8 => 0x38,
            Key::Num9 => 0x39,
            // Numpad
            Key::Numpad0 => 0x60,
            Key::Numpad1 => 0x61,
            Key::Numpad2 => 0x62,
            Key::Numpad3 => 0x63,
            Key::Numpad4 => 0x64,
            Key::Numpad5 => 0x65,
            Key::Numpad6 => 0x66,
            Key::Numpad7 => 0x67,
            Key::Numpad8 => 0x68,
            Key::Numpad9 => 0x69,
            Key::NumpadMultiply => 0x6A,
            Key::NumpadAdd => 0x6B,
            Key::NumpadSubtract => 0x6D,
            Key::NumpadDecimal => 0x6E,
            Key::NumpadDivide => 0x6F,
            // Function keys
            Key::F1 => 0x70,
            Key::F2 => 0x71,
            Key::F3 => 0x72,
            Key::F4 => 0x73,
            Key::F5 => 0x74,
            Key::F6 => 0x75,
            Key::F7 => 0x76,
            Key::F8 => 0x77,
            Key::F9 => 0x78,
            Key::F10 => 0x79,
            Key::F11 => 0x7A,
            Key::F12 => 0x7B,
            Key::F13 => 0x7C,
            Key::F14 => 0x7D,
            Key::F15 => 0x7E,
            Key::F16 => 0x7F,
            Key::F17 => 0x80,
            Key::F18 => 0x81,
            Key::F19 => 0x82,
            Key::F20 => 0x83,
            Key::F21 => 0x84,
            Key::F22 => 0x85,
            Key::F23 => 0x86,
            Key::F24 => 0x87,
            // Modifiers
            Key::Shift => 0x10,
            Key::LShift => 0xA0,
            Key::RShift => 0xA1,
            Key::Control => 0x11,
            Key::LControl => 0xA2,
            Key::RControl => 0xA3,
            Key::Alt => 0x12, // VK_MENU
            Key::LAlt => 0xA4,
            Key::RAlt => 0xA5,
            // Editing / whitespace
            Key::Space => 0x20,
            Key::Enter | Key::Return | Key::NumpadEnter => 0x0D,
            Key::Escape => 0x1B,
            Key::Tab => 0x09,
            Key::Backspace => 0x08,
            Key::Delete => 0x2E,
            // Navigation
            Key::Up => 0x26,
            Key::Down => 0x28,
            Key::Left => 0x25,
            Key::Right => 0x27,
            Key::Home => 0x24,
            Key::End => 0x23,
            Key::PageUp => 0x21,
            Key::PageDown => 0x22,
            Key::Insert => 0x2D,
            // Locks / system
            Key::CapsLock => 0x14,
            Key::NumLock => 0x90,
            Key::ScrollLock => 0x91,
            Key::PrintScreen => 0x2C,
            Key::Pause => 0x13,
            Key::LWin => 0x5B,
            Key::RWin => 0x5C,
            Key::Apps => 0x5D,
            // OEM punctuation
            Key::Oem1 => 0xBA,
            Key::OemPlus => 0xBB,
            Key::OemComma => 0xBC,
            Key::OemMinus => 0xBD,
            Key::OemPeriod => 0xBE,
            Key::Oem2 => 0xBF,
            Key::Oem3 => 0xC0,
            Key::Oem4 => 0xDB,
            Key::Oem5 => 0xDC,
            Key::Oem6 => 0xDD,
            Key::Oem7 => 0xDE,
            // Media
            Key::VolumeMute => 0xAD,
            Key::VolumeDown => 0xAE,
            Key::VolumeUp => 0xAF,
            Key::MediaNextTrack => 0xB0,
            Key::MediaPrevTrack => 0xB1,
            Key::MediaStop => 0xB2,
            Key::MediaPlayPause => 0xB3,
            // Browser
            Key::BrowserBack => 0xA6,
            Key::BrowserForward => 0xA7,
            Key::BrowserRefresh => 0xA8,
            Key::BrowserStop => 0xA9,
            Key::BrowserSearch => 0xAA,
            Key::BrowserFavorites => 0xAB,
            Key::BrowserHome => 0xAC,
        };
        Ok(vk)
    }

    fn is_alt_vk(vk: u16) -> bool {
        vk == 0x12 || vk == 0xA4 || vk == 0xA5
    }

    /// Returns `true` if a virtual-key code corresponds to one of the
    /// "extended keys" that require bit 24 of the keyboard message lParam.
    ///
    /// Per MSDN, extended keys include the right ALT/CTRL, INS/DEL/HOME/END,
    /// PAGE UP/DOWN, arrow keys, NUM LOCK, BREAK (Ctrl+PAUSE), PRINT SCREEN,
    /// numpad divide and numpad ENTER.
    fn is_extended_vk(vk: u16, is_numpad_enter: bool) -> bool {
        matches!(
            vk,
            0xA3 | 0xA5
                | 0x2D
                | 0x2E
                | 0x24
                | 0x23
                | 0x21
                | 0x22
                | 0x25
                | 0x26
                | 0x27
                | 0x28
                | 0x90
                | 0x6F
                | 0x2C
        ) || (is_numpad_enter && vk == 0x0D)
    }

    fn make_key_lparam(vk: u16, is_key_up: bool, is_sys: bool, is_extended: bool) -> isize {
        use windows::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, MAPVK_VK_TO_VSC};
        let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) as isize };
        let mut lp: isize = 1 | (scan << 16);
        if is_extended {
            lp |= 1 << 24;
        }
        if is_sys {
            lp |= 1 << 29;
        }
        if is_key_up {
            // Bits 30 (previous-state) and 31 (transition-state) per MSDN.
            lp |= 1 << 30;
            lp |= 1 << 31;
        }
        lp
    }

    fn pack_xy_lparam(x: i32, y: i32) -> isize {
        (((y as u32) << 16) | (x as u32 & 0xFFFF)) as isize
    }

    fn update_latency(&self, start: Instant) {
        if let Ok(mut guard) = self.last_latency.lock() {
            *guard = Some(start.elapsed().as_secs_f64() * 1000.0);
        }
    }

    fn record_pos(&self, x: i32, y: i32) {
        if let Ok(mut guard) = self.last_pos.lock() {
            *guard = (x, y);
        }
    }

    fn current_pos(&self) -> (i32, i32) {
        self.last_pos.lock().map(|g| *g).unwrap_or((0, 0))
    }

    fn to_effective_coords(&self, x: i32, y: i32) -> Result<(i32, i32)> {
        if self.mode == InputMode::Background {
            // Background PostMessage uses client-area coordinates.
            return Ok((x, y));
        }

        let hwnd = self.hwnd.ok_or(InputError::NotInitialized)?;
        use windows::Win32::Foundation::{HWND, POINT};
        use windows::Win32::Graphics::Gdi::ClientToScreen;

        let mut pt = POINT { x, y };
        unsafe {
            if ClientToScreen(HWND(hwnd as *mut _), &mut pt).as_bool() {
                Ok((pt.x, pt.y))
            } else {
                Err(InputError::SimulationFailed(
                    "failed to convert client coordinates to screen coordinates".into(),
                ))
            }
        }
    }

    /// Best-effort: bring the target window to the foreground.
    ///
    /// `HWND` is `*mut c_void` and therefore `!Send`, so we never hold it
    /// across an `.await`. Each step grabs the handle anew inside an
    /// `unsafe` block, performs its sync work, then drops it before the
    /// next yield point.
    async fn ensure_foreground_window_active(&self) -> Result<()> {
        if self.mode == InputMode::Background {
            return Ok(());
        }

        let hwnd_raw = self.hwnd.ok_or(InputError::NotInitialized)?;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, IsIconic, SetForegroundWindow, SetWindowPos, ShowWindow, HWND_TOP,
            SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_RESTORE,
        };

        let already_foreground = unsafe {
            let target = HWND(hwnd_raw as *mut _);
            GetForegroundWindow() == target
        };
        if already_foreground {
            return Ok(());
        }

        unsafe {
            let target = HWND(hwnd_raw as *mut _);
            if IsIconic(target).as_bool() {
                let _ = ShowWindow(target, SW_RESTORE);
            }
            let _ = SetWindowPos(
                target,
                Some(HWND_TOP),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
        }
        tokio::time::sleep(Duration::from_millis(5)).await;

        unsafe {
            let target = HWND(hwnd_raw as *mut _);
            let _ = SetForegroundWindow(target);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;

        let needs_retry = unsafe {
            let target = HWND(hwnd_raw as *mut _);
            GetForegroundWindow() != target
        };
        if needs_retry {
            unsafe {
                let target = HWND(hwnd_raw as *mut _);
                let _ = SetWindowPos(target, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        let activated = unsafe {
            let target = HWND(hwnd_raw as *mut _);
            GetForegroundWindow() == target
        };
        if activated {
            Ok(())
        } else {
            Err(InputError::SimulationFailed(
                "failed to activate target window before foreground input".into(),
            ))
        }
    }

    async fn send_key_foreground(&self, key: Key, press: bool) -> Result<()> {
        let vk = self.convert_key(key)?;
        self.send_key_foreground_sendinput(vk, press)?;
        // Record/forget held key so Drop can clean up.
        if let Ok(mut guard) = self.held_keys.lock() {
            if press {
                guard.insert(vk);
            } else {
                guard.remove(&vk);
            }
        }
        Ok(())
    }

    fn send_key_foreground_sendinput(&self, vk: u16, press: bool) -> Result<()> {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
            KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
            MAPVK_VK_TO_VSC, VIRTUAL_KEY,
        };

        let scan = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC) } as u16;
        let extended = Self::is_extended_vk(vk, false);

        let (wvk, wscan, mut flags_u32) = if scan != 0 {
            (
                VIRTUAL_KEY(0),
                scan,
                KEYEVENTF_SCANCODE.0 | if press { 0 } else { KEYEVENTF_KEYUP.0 },
            )
        } else {
            (
                VIRTUAL_KEY(vk),
                0u16,
                if press { 0 } else { KEYEVENTF_KEYUP.0 },
            )
        };
        if extended {
            flags_u32 |= KEYEVENTF_EXTENDEDKEY.0;
        }
        let dw_flags = KEYBD_EVENT_FLAGS(flags_u32);

        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: wvk,
                    wScan: wscan,
                    dwFlags: dw_flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
        if sent == 0 {
            return Err(InputError::SimulationFailed(
                "SendInput keyboard failed".into(),
            ));
        }
        Ok(())
    }

    async fn send_mouse_move_foreground(&self, x: i32, y: i32) -> Result<()> {
        let mut enigo_guard = self
            .enigo
            .lock()
            .map_err(|_| InputError::SimulationFailed("Lock poisoned".into()))?;
        let enigo = enigo_guard.as_mut().ok_or(InputError::NotInitialized)?;
        enigo.move_mouse(x, y, Coordinate::Abs)?;
        Ok(())
    }

    fn get_enigo_button(button: MouseButton) -> Button {
        match button {
            MouseButton::Left => Button::Left,
            MouseButton::Right => Button::Right,
            MouseButton::Middle => Button::Middle,
            MouseButton::X1 => Button::Back,
            MouseButton::X2 => Button::Forward,
        }
    }

    async fn send_mouse_button_foreground(&self, button: MouseButton, press: bool) -> Result<()> {
        let mut enigo_guard = self
            .enigo
            .lock()
            .map_err(|_| InputError::SimulationFailed("Lock poisoned".into()))?;
        let enigo = enigo_guard.as_mut().ok_or(InputError::NotInitialized)?;
        let btn = Self::get_enigo_button(button);
        let dir = if press {
            Direction::Press
        } else {
            Direction::Release
        };
        enigo.button(btn, dir)?;
        drop(enigo_guard);

        if let Ok(mut guard) = self.held_buttons.lock() {
            if press {
                guard.insert(button);
            } else {
                guard.remove(&button);
            }
        }
        Ok(())
    }

    fn mk_for_button(button: MouseButton) -> usize {
        match button {
            MouseButton::Left => MK_LBUTTON,
            MouseButton::Right => MK_RBUTTON,
            MouseButton::Middle => MK_MBUTTON,
            MouseButton::X1 => MK_XBUTTON1,
            MouseButton::X2 => MK_XBUTTON2,
        }
    }

    fn xbutton_wparam_high(button: MouseButton) -> usize {
        // High word of WM_XBUTTON*'s wParam encodes which X button.
        match button {
            MouseButton::X1 => 1usize << 16,
            MouseButton::X2 => 2usize << 16,
            _ => 0,
        }
    }

    /// Drain all keys recorded as pressed; best-effort release on Drop.
    fn force_release_held(&self) {
        // Only meaningful in foreground mode; in background mode the messages
        // already have explicit up counterparts on each call.
        if self.mode != InputMode::Foreground {
            return;
        }
        let keys: Vec<u16> = self
            .held_keys
            .lock()
            .map(|g| g.iter().copied().collect())
            .unwrap_or_default();
        for vk in keys {
            let _ = self.send_key_foreground_sendinput(vk, false);
        }

        let buttons: Vec<MouseButton> = self
            .held_buttons
            .lock()
            .map(|g| g.iter().copied().collect())
            .unwrap_or_default();
        if !buttons.is_empty() {
            if let Ok(mut guard) = self.enigo.lock() {
                if let Some(enigo) = guard.as_mut() {
                    for btn in buttons {
                        let _ = enigo.button(Self::get_enigo_button(btn), Direction::Release);
                    }
                }
            }
        }
    }

    /// Common helper: cancel-aware sleep that returns false if the swipe was
    /// cancelled in the meantime.
    async fn cancel_aware_sleep(&self, token: u64, duration: Duration) -> bool {
        if duration.is_zero() {
            return self.swipe_cancel_at.load(Ordering::SeqCst) < token;
        }
        tokio::time::sleep(duration).await;
        self.swipe_cancel_at.load(Ordering::SeqCst) < token
    }
}

#[async_trait]
impl InputController for Win32Input {
    fn name(&self) -> &str {
        "Win32"
    }

    fn parse_key(&self, name: &str) -> Option<Key> {
        self.key_mapper.map_key(name).ok()
    }

    async fn init(&mut self, target: &InputTarget) -> AnyhowResult<()> {
        match target {
            InputTarget::NativeWindow { hwnd } => {
                self.hwnd = Some(*hwnd);
                self.mode = InputMode::Foreground;
                let enigo = Enigo::new(&Settings::default())
                    .map_err(|e| InputError::SimulationFailed(e.to_string()))?;
                *self
                    .enigo
                    .lock()
                    .map_err(|_| InputError::SimulationFailed("Lock poisoned".into()))? =
                    Some(enigo);
                self.initialized = true;
            }
            InputTarget::NativeWindowBackground { hwnd } => {
                self.hwnd = Some(*hwnd);
                self.mode = InputMode::Background;
                self.initialized = true;
            }
            _ => {
                return Err(InputError::SimulationFailed(
                    "Win32 engine only supports native windows".into(),
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
        self.record_pos(x, y);

        match self.mode {
            InputMode::Foreground | InputMode::Auto => {
                self.ensure_foreground_window_active().await?;
                let (sx, sy) = self.to_effective_coords(x, y)?;
                let mut enigo_guard = self
                    .enigo
                    .lock()
                    .map_err(|_| InputError::SimulationFailed("Lock poisoned".into()))?;
                let enigo = enigo_guard.as_mut().ok_or(InputError::NotInitialized)?;
                enigo.move_mouse(sx, sy, Coordinate::Abs)?;
                enigo.button(Button::Left, Direction::Click)?;
            }
            InputMode::Background => {
                use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::{
                    PostMessageW, WM_LBUTTONDOWN, WM_LBUTTONUP,
                };

                let hwnd = self.hwnd.ok_or(InputError::NotInitialized)?;
                let lparam = LPARAM(Self::pack_xy_lparam(x, y));
                unsafe {
                    let _ = PostMessageW(
                        Some(HWND(hwnd as *mut _)),
                        WM_LBUTTONDOWN,
                        WPARAM(MK_LBUTTON),
                        lparam,
                    );
                    let _ =
                        PostMessageW(Some(HWND(hwnd as *mut _)), WM_LBUTTONUP, WPARAM(0), lparam);
                }
            }
        }

        self.update_latency(start);
        Ok(())
    }

    async fn double_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        self.click(x, y).await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        self.click(x, y).await
    }

    async fn right_click(&self, x: i32, y: i32) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        self.record_pos(x, y);

        match self.mode {
            InputMode::Foreground | InputMode::Auto => {
                self.ensure_foreground_window_active().await?;
                let (sx, sy) = self.to_effective_coords(x, y)?;
                self.send_mouse_move_foreground(sx, sy).await?;
                self.send_mouse_button_foreground(MouseButton::Right, true)
                    .await?;
                self.send_mouse_button_foreground(MouseButton::Right, false)
                    .await?;
            }
            InputMode::Background => {
                use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::{
                    PostMessageW, WM_RBUTTONDOWN, WM_RBUTTONUP,
                };

                let hwnd = self.hwnd.ok_or(InputError::NotInitialized)?;
                let lparam = LPARAM(Self::pack_xy_lparam(x, y));
                unsafe {
                    let _ = PostMessageW(
                        Some(HWND(hwnd as *mut _)),
                        WM_RBUTTONDOWN,
                        WPARAM(MK_RBUTTON),
                        lparam,
                    );
                    let _ =
                        PostMessageW(Some(HWND(hwnd as *mut _)), WM_RBUTTONUP, WPARAM(0), lparam);
                }
            }
        }

        self.update_latency(start);
        Ok(())
    }

    async fn mouse_move(&self, x: i32, y: i32) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();
        self.record_pos(x, y);

        match self.mode {
            InputMode::Foreground | InputMode::Auto => {
                self.ensure_foreground_window_active().await?;
                let (sx, sy) = self.to_effective_coords(x, y)?;
                self.send_mouse_move_foreground(sx, sy).await?;
            }
            InputMode::Background => {
                use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_MOUSEMOVE};

                let hwnd = self.hwnd.ok_or(InputError::NotInitialized)?;
                let lparam = LPARAM(Self::pack_xy_lparam(x, y));
                unsafe {
                    let _ =
                        PostMessageW(Some(HWND(hwnd as *mut _)), WM_MOUSEMOVE, WPARAM(0), lparam);
                }
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

        match self.mode {
            InputMode::Foreground | InputMode::Auto => {
                self.ensure_foreground_window_active().await?;
                self.send_mouse_button_foreground(button, true).await?;
            }
            InputMode::Background => {
                use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::{
                    PostMessageW, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_XBUTTONDOWN,
                };

                let hwnd = self.hwnd.ok_or(InputError::NotInitialized)?;
                let (cx, cy) = self.current_pos();
                let lparam = LPARAM(Self::pack_xy_lparam(cx, cy));
                let mk = Self::mk_for_button(button);
                unsafe {
                    match button {
                        MouseButton::Left => {
                            let _ = PostMessageW(
                                Some(HWND(hwnd as *mut _)),
                                WM_LBUTTONDOWN,
                                WPARAM(mk),
                                lparam,
                            );
                        }
                        MouseButton::Right => {
                            let _ = PostMessageW(
                                Some(HWND(hwnd as *mut _)),
                                WM_RBUTTONDOWN,
                                WPARAM(mk),
                                lparam,
                            );
                        }
                        MouseButton::Middle => {
                            let _ = PostMessageW(
                                Some(HWND(hwnd as *mut _)),
                                WM_MBUTTONDOWN,
                                WPARAM(mk),
                                lparam,
                            );
                        }
                        MouseButton::X1 | MouseButton::X2 => {
                            let wp = mk | Self::xbutton_wparam_high(button);
                            let _ = PostMessageW(
                                Some(HWND(hwnd as *mut _)),
                                WM_XBUTTONDOWN,
                                WPARAM(wp),
                                lparam,
                            );
                        }
                    }
                }
            }
        }

        self.update_latency(start);
        Ok(())
    }

    async fn mouse_up(&self, button: MouseButton) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();

        match self.mode {
            InputMode::Foreground | InputMode::Auto => {
                self.ensure_foreground_window_active().await?;
                self.send_mouse_button_foreground(button, false).await?;
            }
            InputMode::Background => {
                use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::{
                    PostMessageW, WM_LBUTTONUP, WM_MBUTTONUP, WM_RBUTTONUP, WM_XBUTTONUP,
                };

                let hwnd = self.hwnd.ok_or(InputError::NotInitialized)?;
                let (cx, cy) = self.current_pos();
                let lparam = LPARAM(Self::pack_xy_lparam(cx, cy));
                unsafe {
                    match button {
                        MouseButton::Left => {
                            let _ = PostMessageW(
                                Some(HWND(hwnd as *mut _)),
                                WM_LBUTTONUP,
                                WPARAM(0),
                                lparam,
                            );
                        }
                        MouseButton::Right => {
                            let _ = PostMessageW(
                                Some(HWND(hwnd as *mut _)),
                                WM_RBUTTONUP,
                                WPARAM(0),
                                lparam,
                            );
                        }
                        MouseButton::Middle => {
                            let _ = PostMessageW(
                                Some(HWND(hwnd as *mut _)),
                                WM_MBUTTONUP,
                                WPARAM(0),
                                lparam,
                            );
                        }
                        MouseButton::X1 | MouseButton::X2 => {
                            let wp = Self::xbutton_wparam_high(button);
                            let _ = PostMessageW(
                                Some(HWND(hwnd as *mut _)),
                                WM_XBUTTONUP,
                                WPARAM(wp),
                                lparam,
                            );
                        }
                    }
                }
            }
        }

        self.update_latency(start);
        Ok(())
    }

    async fn mouse_scroll(&self, delta: i32) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();

        match self.mode {
            InputMode::Foreground | InputMode::Auto => {
                self.ensure_foreground_window_active().await?;
                let mut enigo_guard = self
                    .enigo
                    .lock()
                    .map_err(|_| InputError::SimulationFailed("Lock poisoned".into()))?;
                let enigo = enigo_guard.as_mut().ok_or(InputError::NotInitialized)?;
                enigo.scroll(delta, Axis::Vertical)?;
            }
            InputMode::Background => {
                use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_MOUSEWHEEL};

                let hwnd = self.hwnd.ok_or(InputError::NotInitialized)?;
                // Wheel delta is signed and lives in the high word of wParam.
                let raw_delta = (delta * 120) as i16 as u32;
                let wparam = WPARAM(((raw_delta as usize) << 16) & 0xFFFF_0000);
                let (cx, cy) = self.current_pos();
                let lparam = LPARAM(Self::pack_xy_lparam(cx, cy));
                unsafe {
                    let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_MOUSEWHEEL, wparam, lparam);
                }
            }
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

        // Allocate a fresh swipe token; a concurrent cancel after this point
        // raises swipe_cancel_at to >= our token and we abort gracefully.
        let token = self.swipe_token.fetch_add(1, Ordering::SeqCst) + 1;

        // Pick step count and per-step delay so the total time stays close to
        // the requested duration. Floor at 2 frames so straight-line swipes
        // still fire one mid-point event (start, mid, end).
        let target_step_ms: u32 = 16;
        let steps = (duration_ms / target_step_ms).clamp(2, 240);
        let step_delay = Duration::from_millis(duration_ms.max(1) as u64 / steps as u64);
        let dx = (x2 - x1) as f64 / steps as f64;
        let dy = (y2 - y1) as f64 / steps as f64;

        // Press at start, then iterate intermediate points.
        self.mouse_move(x1, y1).await?;
        self.mouse_down(MouseButton::Left).await?;

        for i in 1..=steps {
            if self.swipe_cancel_at.load(Ordering::SeqCst) >= token {
                break;
            }
            let px = x1 + (dx * i as f64) as i32;
            let py = y1 + (dy * i as f64) as i32;
            self.mouse_move(px, py).await?;
            if !self.cancel_aware_sleep(token, step_delay).await {
                break;
            }
        }

        // Always release the button — even when cancelled — to avoid leaving
        // the game in a half-pressed state.
        self.mouse_up(MouseButton::Left).await
    }

    async fn key_press(&self, key: Key) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();

        match self.mode {
            InputMode::Foreground | InputMode::Auto => {
                self.ensure_foreground_window_active().await?;
                self.send_key_foreground(key, true).await?;
            }
            InputMode::Background => {
                use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::{
                    PostMessageW, WM_KEYDOWN, WM_SYSKEYDOWN,
                };

                let hwnd = self.hwnd.ok_or(InputError::NotInitialized)?;
                let vk = self.convert_key(key)?;
                let is_sys = Self::is_alt_vk(vk);
                let is_ext = Self::is_extended_vk(vk, matches!(key, Key::NumpadEnter));
                let msg = if is_sys { WM_SYSKEYDOWN } else { WM_KEYDOWN };
                let lparam = LPARAM(Self::make_key_lparam(vk, false, is_sys, is_ext));
                unsafe {
                    let _ =
                        PostMessageW(Some(HWND(hwnd as *mut _)), msg, WPARAM(vk as usize), lparam);
                }
            }
        }

        self.update_latency(start);
        Ok(())
    }

    async fn key_release(&self, key: Key) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }
        let start = Instant::now();

        match self.mode {
            InputMode::Foreground | InputMode::Auto => {
                self.ensure_foreground_window_active().await?;
                self.send_key_foreground(key, false).await?;
            }
            InputMode::Background => {
                use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::{
                    PostMessageW, WM_KEYUP, WM_SYSKEYUP,
                };

                let hwnd = self.hwnd.ok_or(InputError::NotInitialized)?;
                let vk = self.convert_key(key)?;
                let is_sys = Self::is_alt_vk(vk);
                let is_ext = Self::is_extended_vk(vk, matches!(key, Key::NumpadEnter));
                let msg = if is_sys { WM_SYSKEYUP } else { WM_KEYUP };
                let lparam = LPARAM(Self::make_key_lparam(vk, true, is_sys, is_ext));
                unsafe {
                    let _ =
                        PostMessageW(Some(HWND(hwnd as *mut _)), msg, WPARAM(vk as usize), lparam);
                }
            }
        }

        self.update_latency(start);
        Ok(())
    }

    async fn key_tap(&self, key: Key, duration_ms: Option<u32>) -> AnyhowResult<()> {
        self.key_press(key).await?;
        if let Some(ms) = duration_ms {
            tokio::time::sleep(Duration::from_millis(ms as u64)).await;
        }
        self.key_release(key).await
    }

    async fn type_text(&self, text: &str) -> AnyhowResult<()> {
        if !self.initialized {
            return Err(InputError::NotInitialized.into());
        }

        match self.mode {
            InputMode::Foreground | InputMode::Auto => {
                self.ensure_foreground_window_active().await?;
                let mut enigo_guard = self
                    .enigo
                    .lock()
                    .map_err(|_| InputError::SimulationFailed("Lock poisoned".into()))?;
                let enigo = enigo_guard.as_mut().ok_or(InputError::NotInitialized)?;
                enigo.text(text)?;
            }
            InputMode::Background => {
                use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CHAR};

                let hwnd = self.hwnd.ok_or(InputError::NotInitialized)?;
                // Encode each scalar as UTF-16 and send WM_CHAR per code unit.
                for unit in text.encode_utf16() {
                    unsafe {
                        let _ = PostMessageW(
                            Some(HWND(hwnd as *mut _)),
                            WM_CHAR,
                            WPARAM(unit as usize),
                            LPARAM(1),
                        );
                    }
                }
            }
        }
        Ok(())
    }

    async fn key_combo(&self, keys: &[Key]) -> AnyhowResult<()> {
        for &key in keys {
            self.key_press(key).await?;
        }
        for &key in keys.iter().rev() {
            self.key_release(key).await?;
        }
        Ok(())
    }

    fn supports_background(&self) -> bool {
        true
    }

    fn last_latency_ms(&self) -> Option<f64> {
        self.last_latency.lock().ok().and_then(|g| *g)
    }

    fn mode(&self) -> InputMode {
        self.mode
    }
}

impl Drop for Win32Input {
    fn drop(&mut self) {
        // Best-effort: release any keys/buttons that the controller observed
        // as held so the game window doesn't get stuck after a crash or
        // controller swap.
        self.force_release_held();
    }
}

#[cfg(test)]
mod win32_pure_tests {
    use super::*;

    #[test]
    fn is_extended_vk_covers_arrow_and_navigation_keys() {
        for vk in [0x25, 0x26, 0x27, 0x28, 0x21, 0x22, 0x24, 0x23, 0x2D, 0x2E] {
            assert!(
                Win32Input::is_extended_vk(vk, false),
                "vk 0x{vk:02X} should be flagged as extended"
            );
        }
        // RAlt / RControl / NumLock / numpad divide / PrintScreen
        for vk in [0xA5, 0xA3, 0x90, 0x6F, 0x2C] {
            assert!(
                Win32Input::is_extended_vk(vk, false),
                "vk 0x{vk:02X} should be flagged as extended"
            );
        }
        // Plain VK_RETURN is NOT extended unless it's the numpad enter
        // variant tracked by the caller.
        assert!(!Win32Input::is_extended_vk(0x0D, false));
        assert!(Win32Input::is_extended_vk(0x0D, true));

        // Ordinary letter keys must not be extended.
        for vk in [0x41, 0x42, 0x5A, 0x30, 0x70] {
            assert!(
                !Win32Input::is_extended_vk(vk, false),
                "vk 0x{vk:02X} should not be flagged as extended"
            );
        }
    }

    #[test]
    fn make_key_lparam_sets_extended_bit_24() {
        // Up arrow (0x26) is extended; bit 24 (0x0100_0000) must be set.
        let lp = Win32Input::make_key_lparam(0x26, false, false, true) as u32;
        assert_ne!(lp & 0x0100_0000, 0, "extended bit 24 must be set");

        // Letter A (0x41) is not extended; bit 24 must remain clear.
        let lp_a = Win32Input::make_key_lparam(0x41, false, false, false) as u32;
        assert_eq!(lp_a & 0x0100_0000, 0, "extended bit 24 must be clear");

        // Key-up sets bits 30 and 31.
        let lp_up = Win32Input::make_key_lparam(0x41, true, false, false) as u32;
        assert_ne!(lp_up & 0x4000_0000, 0, "bit 30 must be set on key release");
        assert_ne!(lp_up & 0x8000_0000, 0, "bit 31 must be set on key release");
    }

    #[test]
    fn pack_xy_lparam_packs_y_high_word_x_low_word() {
        let lp = Win32Input::pack_xy_lparam(0x1234, 0x5678) as u32;
        assert_eq!(lp & 0xFFFF, 0x1234);
        assert_eq!((lp >> 16) & 0xFFFF, 0x5678);
    }

    #[test]
    fn mk_for_button_maps_to_msdn_constants() {
        assert_eq!(Win32Input::mk_for_button(MouseButton::Left), MK_LBUTTON);
        assert_eq!(Win32Input::mk_for_button(MouseButton::Right), MK_RBUTTON);
        assert_eq!(Win32Input::mk_for_button(MouseButton::Middle), MK_MBUTTON);
        assert_eq!(Win32Input::mk_for_button(MouseButton::X1), MK_XBUTTON1);
        assert_eq!(Win32Input::mk_for_button(MouseButton::X2), MK_XBUTTON2);
    }

    #[test]
    fn convert_key_covers_extended_keys() {
        let ctrl = Win32Input::new(KeyMapper::new(Default::default()));
        assert_eq!(ctrl.convert_key(Key::F13).unwrap(), 0x7C);
        assert_eq!(ctrl.convert_key(Key::F24).unwrap(), 0x87);
        assert_eq!(ctrl.convert_key(Key::NumpadAdd).unwrap(), 0x6B);
        assert_eq!(ctrl.convert_key(Key::NumpadDivide).unwrap(), 0x6F);
        assert_eq!(ctrl.convert_key(Key::VolumeUp).unwrap(), 0xAF);
        assert_eq!(ctrl.convert_key(Key::MediaPlayPause).unwrap(), 0xB3);
        assert_eq!(ctrl.convert_key(Key::BrowserHome).unwrap(), 0xAC);
        assert_eq!(ctrl.convert_key(Key::LShift).unwrap(), 0xA0);
        assert_eq!(ctrl.convert_key(Key::RControl).unwrap(), 0xA3);
    }

    #[test]
    fn parse_key_routes_through_user_bindings() {
        use std::collections::HashMap;
        let mut bindings = HashMap::new();
        bindings.insert("attack".to_string(), "VK_A".to_string());
        bindings.insert("jump".to_string(), "SPACE".to_string());
        let ctrl = Win32Input::new(KeyMapper::new(bindings));

        // Custom logical name resolves through KeyMapper.
        assert_eq!(ctrl.parse_key("attack"), Some(Key::A));
        assert_eq!(ctrl.parse_key("jump"), Some(Key::Space));
        // Standard names still work.
        assert_eq!(ctrl.parse_key("F12"), Some(Key::F12));
        // Garbage rejected.
        assert!(ctrl.parse_key("definitely_not_a_key").is_none());
    }

    #[test]
    fn cancel_swipe_only_aborts_currently_outstanding_tokens() {
        let ctrl = Win32Input::new(KeyMapper::new(Default::default()));
        assert_eq!(ctrl.swipe_token.load(Ordering::SeqCst), 0);
        // Cancel before any swipe ⇒ cancel watermark stays at 0.
        ctrl.cancel_swipe();
        assert_eq!(ctrl.swipe_cancel_at.load(Ordering::SeqCst), 0);

        // Simulate a fresh swipe (token bumps to 1). A subsequent cancel
        // raises the watermark to 1 and would abort that swipe...
        let token = ctrl.swipe_token.fetch_add(1, Ordering::SeqCst) + 1;
        assert_eq!(token, 1);
        ctrl.cancel_swipe();
        assert_eq!(ctrl.swipe_cancel_at.load(Ordering::SeqCst), 1);
        // ...but a new swipe (token = 2) is unaffected.
        let token2 = ctrl.swipe_token.fetch_add(1, Ordering::SeqCst) + 1;
        assert_eq!(token2, 2);
        assert!(ctrl.swipe_cancel_at.load(Ordering::SeqCst) < token2);
    }
}
