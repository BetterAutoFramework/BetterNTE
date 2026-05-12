//! CLI tool for recording keyboard and mouse actions to JSON macro files.
//!
//! Uses Win32 low-level hooks (WH_KEYBOARD_LL / WH_MOUSE_LL) to capture
//! global input events and writes them in the same JSON format used by
//! `betternte-input::recorder::Macro`.
//!
//! Usage:
//!   key-recorder -d ./recordings              # dir mode, F10 toggle, Esc quit
//!   key-recorder -o recording.json            # single file, F10 toggle, Esc quit
//!   key-recorder -d ./rec -n "my-macro"       # custom macro name prefix

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use clap::Parser;
use serde::Serialize;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// ── CLI ──────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "key-recorder", about = "Record keyboard and mouse actions to JSON")]
struct Cli {
    /// Output directory (auto-generates 001.json, 002.json, ...)
    #[arg(short, long)]
    dir: Option<PathBuf>,

    /// Output JSON file path (single-file mode, conflicts with --dir)
    #[arg(short, long, conflicts_with = "dir")]
    output: Option<String>,

    /// Macro name (or prefix in dir mode)
    #[arg(short, long, default_value = "recorded")]
    name: String,
}

// ── Data types (mirror betternte-input) ──────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum MouseButtonSerde {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "params", rename_all = "snake_case")]
enum InputAction {
    MouseMove { x: i32, y: i32 },
    MouseDown { button: MouseButtonSerde },
    MouseUp { button: MouseButtonSerde },
    KeyDown { key: String },
    KeyUp { key: String },
    Scroll { delta: i32 },
}

#[derive(Debug, Clone, Serialize)]
struct InputEvent {
    offset_ms: u64,
    action: InputAction,
}

#[derive(Debug, Clone, Serialize)]
struct Macro {
    name: String,
    events: Vec<InputEvent>,
    total_duration_ms: u64,
    loop_count: u32,
}

// ── Global state ─────────────────────────────────────────────────────────────

static START_TIME: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
static EVENTS: OnceLock<Mutex<Vec<InputEvent>>> = OnceLock::new();
static RECORDING: AtomicBool = AtomicBool::new(false);
static QUIT: AtomicBool = AtomicBool::new(false);
static FILE_COUNTER: AtomicU32 = AtomicU32::new(1);

/// Where to save: None = single-file mode, Some(dir) = directory mode.
static OUTPUT_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
/// Single-file output path (used when OUTPUT_DIR is None).
static OUTPUT_FILE: OnceLock<Option<String>> = OnceLock::new();
/// Macro name prefix.
static MACRO_NAME: OnceLock<String> = OnceLock::new();

fn elapsed_ms() -> u64 {
    START_TIME
        .get()
        .and_then(|m| m.lock().ok())
        .and_then(|guard| *guard)
        .map(|t| t.elapsed().as_millis() as u64)
        .unwrap_or(0)
}

fn reset_timer() {
    if let Some(m) = START_TIME.get() {
        if let Ok(mut guard) = m.lock() {
            *guard = Some(Instant::now());
        }
    }
}

fn clear_events() {
    if let Some(events) = EVENTS.get() {
        if let Ok(mut guard) = events.lock() {
            guard.clear();
        }
    }
}

fn clone_events() -> Vec<InputEvent> {
    EVENTS
        .get()
        .and_then(|m| m.lock().ok())
        .map(|g| g.clone())
        .unwrap_or_default()
}

fn push_event(action: InputAction) {
    if !RECORDING.load(Ordering::Relaxed) {
        return;
    }
    if let Some(events) = EVENTS.get() {
        if let Ok(mut guard) = events.lock() {
            guard.push(InputEvent {
                offset_ms: elapsed_ms(),
                action,
            });
        }
    }
}

// ── Save logic ───────────────────────────────────────────────────────────────

fn save_recording() {
    let events = clone_events();
    let total_ms = events.last().map(|e| e.offset_ms).unwrap_or(0);
    let name = MACRO_NAME
        .get()
        .cloned()
        .unwrap_or_else(|| "recorded".to_string());

    let mac = Macro {
        name: name.clone(),
        events,
        total_duration_ms: total_ms,
        loop_count: 1,
    };

    let json = serde_json::to_string_pretty(&mac).expect("Failed to serialize macro");

    let output_path = if let Some(Some(dir)) = OUTPUT_DIR.get() {
        // Directory mode: auto-increment filename
        let idx = FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let filename = format!("{:03}.json", idx);
        dir.join(filename)
    } else if let Some(Some(file)) = OUTPUT_FILE.get() {
        PathBuf::from(file)
    } else {
        PathBuf::from("recording.json")
    };

    // Ensure parent directory exists
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    match std::fs::write(&output_path, &json) {
        Ok(_) => {
            println!(
                "[SAVED] {} events ({} ms) -> {}",
                mac.events.len(),
                mac.total_duration_ms,
                output_path.display()
            );
        }
        Err(e) => {
            eprintln!("[ERROR] Failed to write {}: {}", output_path.display(), e);
        }
    }
}

// ── Recording control ────────────────────────────────────────────────────────

fn start_recording() {
    if RECORDING.load(Ordering::Relaxed) {
        return; // already recording
    }
    clear_events();
    reset_timer();
    RECORDING.store(true, Ordering::Relaxed);
    println!("[REC] Recording started... (F10 to stop, Esc to quit)");
}

fn stop_recording() {
    if !RECORDING.load(Ordering::Relaxed) {
        return; // not recording
    }
    RECORDING.store(false, Ordering::Relaxed);
    save_recording();
}

fn toggle_recording() {
    if RECORDING.load(Ordering::Relaxed) {
        stop_recording();
    } else {
        start_recording();
    }
}

// ── Virtual key → name ───────────────────────────────────────────────────────

fn vk_to_name(vk: VIRTUAL_KEY) -> Option<String> {
    let code = vk.0;
    let name = match code {
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Return",
        0x10 => "Shift",
        0x11 => "Control",
        0x12 => "Alt",
        0x13 => "Pause",
        0x14 => "CapsLock",
        0x1B => "Escape",
        0x20 => "Space",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x2C => "PrintScreen",
        0x2D => "Insert",
        0x2E => "Delete",
        0x5B => "LWin",
        0x5C => "RWin",
        0x5D => "Apps",
        0x60 => "Numpad0",
        0x61 => "Numpad1",
        0x62 => "Numpad2",
        0x63 => "Numpad3",
        0x64 => "Numpad4",
        0x65 => "Numpad5",
        0x66 => "Numpad6",
        0x67 => "Numpad7",
        0x68 => "Numpad8",
        0x69 => "Numpad9",
        0x6A => "NumpadMultiply",
        0x6B => "NumpadAdd",
        0x6C => "NumpadSeparator",
        0x6D => "NumpadSubtract",
        0x6E => "NumpadDecimal",
        0x6F => "NumpadDivide",
        0x70 => "F1",
        0x71 => "F2",
        0x72 => "F3",
        0x73 => "F4",
        0x74 => "F5",
        0x75 => "F6",
        0x76 => "F7",
        0x77 => "F8",
        0x78 => "F10",
        0x79 => "F10",
        0x7A => "F11",
        0x7B => "F12",
        0x90 => "NumLock",
        0x91 => "ScrollLock",
        0xA0 => "LShift",
        0xA1 => "RShift",
        0xA2 => "LControl",
        0xA3 => "RControl",
        0xA4 => "LAlt",
        0xA5 => "RAlt",
        0xBA => "Oem1",      // ;:
        0xBB => "OemPlus",   // =+
        0xBC => "OemComma",  // ,<
        0xBD => "OemMinus",  // -_
        0xBE => "OemPeriod", // .>
        0xBF => "Oem2",      // /?
        0xC0 => "Oem3",      // `~
        0xDB => "Oem4",      // [{
        0xDC => "Oem5",      // \|
        0xDD => "Oem6",      // ]}
        0xDE => "Oem7",      // '"
        _ => {
            if (0x30..=0x39).contains(&code) {
                return Some(format!("Num{}", code - 0x30));
            }
            if (0x41..=0x5A).contains(&code) {
                return Some(format!("{}", (b'A' + (code - 0x41) as u8) as char));
            }
            return None;
        }
    };
    Some(name.to_string())
}

// ── Hook callbacks ───────────────────────────────────────────────────────────

unsafe extern "system" fn keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 {
        let kb = unsafe { *(l_param.0 as *const KBDLLHOOKSTRUCT) };
        let vk = VIRTUAL_KEY(kb.vkCode as u16);

        match w_param.0 as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if let Some(name) = vk_to_name(vk) {
                    // F10 toggles recording
                    if name == "F10" {
                        toggle_recording();
                        return unsafe { CallNextHookEx(None, n_code, w_param, l_param) };
                    }
                    // Escape quits (stop recording first if active)
                    if name == "Escape" {
                        stop_recording();
                        QUIT.store(true, Ordering::Relaxed);
                        unsafe { PostQuitMessage(0) };
                        return LRESULT(1);
                    }
                    push_event(InputAction::KeyDown { key: name });
                }
            }
            WM_KEYUP | WM_SYSKEYUP => {
                if let Some(name) = vk_to_name(vk) {
                    // Don't record F10 key-up as an input action
                    if name == "F10" {
                        return unsafe { CallNextHookEx(None, n_code, w_param, l_param) };
                    }
                    push_event(InputAction::KeyUp { key: name });
                }
            }
            _ => {}
        }
    }
    unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
}

unsafe extern "system" fn mouse_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 {
        let ms = unsafe { *(l_param.0 as *const MSLLHOOKSTRUCT) };
        let pt = ms.pt;

        match w_param.0 as u32 {
            WM_MOUSEMOVE => {
                push_event(InputAction::MouseMove {
                    x: pt.x,
                    y: pt.y,
                });
            }
            WM_LBUTTONDOWN => push_event(InputAction::MouseDown {
                button: MouseButtonSerde::Left,
            }),
            WM_LBUTTONUP => push_event(InputAction::MouseUp {
                button: MouseButtonSerde::Left,
            }),
            WM_RBUTTONDOWN => push_event(InputAction::MouseDown {
                button: MouseButtonSerde::Right,
            }),
            WM_RBUTTONUP => push_event(InputAction::MouseUp {
                button: MouseButtonSerde::Right,
            }),
            WM_MBUTTONDOWN => push_event(InputAction::MouseDown {
                button: MouseButtonSerde::Middle,
            }),
            WM_MBUTTONUP => push_event(InputAction::MouseUp {
                button: MouseButtonSerde::Middle,
            }),
            WM_XBUTTONDOWN => {
                let xbutton = ((ms.mouseData >> 16) & 0xFFFF) as u16;
                let btn = if xbutton == XBUTTON1 {
                    MouseButtonSerde::X1
                } else {
                    MouseButtonSerde::X2
                };
                push_event(InputAction::MouseDown { button: btn });
            }
            WM_XBUTTONUP => {
                let xbutton = ((ms.mouseData >> 16) & 0xFFFF) as u16;
                let btn = if xbutton == XBUTTON1 {
                    MouseButtonSerde::X1
                } else {
                    MouseButtonSerde::X2
                };
                push_event(InputAction::MouseUp { button: btn });
            }
            WM_MOUSEWHEEL => {
                let delta = ((ms.mouseData >> 16) & 0xFFFF) as i16 as i32;
                let normalized = if delta > 0 { 1 } else { -1 };
                push_event(InputAction::Scroll { delta: normalized });
            }
            _ => {}
        }
    }
    unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    // Validate arguments
    if cli.dir.is_none() && cli.output.is_none() {
        eprintln!("Error: specify either -d <dir> or -o <file>");
        eprintln!("Run with --help for usage information.");
        std::process::exit(1);
    }

    // Initialize global state
    let _ = START_TIME.set(Mutex::new(None));
    let _ = EVENTS.set(Mutex::new(Vec::new()));
    let _ = MACRO_NAME.set(cli.name.clone());

    if let Some(dir) = cli.dir {
        // Directory mode
        let _ = std::fs::create_dir_all(&dir);
        let _ = OUTPUT_DIR.set(Some(dir.clone()));
        println!("=== Key Recorder (dir mode) ===");
        println!("Directory: {}", dir.display());
        println!("Name prefix: {}", cli.name);
    } else if let Some(ref file) = cli.output {
        // Single-file mode
        let _ = OUTPUT_FILE.set(Some(file.clone()));
        println!("=== Key Recorder (file mode) ===");
        println!("Output: {}", file);
    }

    println!("F10    = start / stop recording");
    println!("Esc   = quit\n");

    // Install hooks
    let kbd_hook = unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0)
            .expect("Failed to install keyboard hook")
    };
    let mouse_hook = unsafe {
        SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0)
            .expect("Failed to install mouse hook")
    };

    // Message pump (blocks until PostQuitMessage)
    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.into() {
        // no-op, just pump
    }

    // Cleanup hooks
    unsafe {
        let _ = UnhookWindowsHookEx(kbd_hook);
        let _ = UnhookWindowsHookEx(mouse_hook);
    }

    println!("\nDone.");
}
