use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

mod stitch;
mod xcap_capture;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowDto {
    pub hwnd: i64,
    pub title: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureDto {
    pub width: u32,
    pub height: u32,
    pub png_base64: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WpsSettings {
    pub save_dir: Option<String>,
    pub crop_dir: Option<String>,
    pub interval_ms: u64,
    pub rounded_rx: u32,
    pub last_hwnd: Option<i64>,
    // UI language ("zh" | "en")
    pub language: Option<String>,
    // Keyboard shortcuts (action → key)
    pub keybindings: Option<HashMap<String, String>>,
    // Scroll capture
    pub scroll_direction: Option<String>,
    pub scroll_amount: Option<i32>,
    pub scroll_frames: Option<u32>,
    pub scroll_delay_ms: Option<u64>,
    // Panoramic capture
    pub pano_direction: Option<String>,
    pub pano_drag_distance: Option<i32>,
    pub pano_frames: Option<u32>,
    pub pano_delay_ms: Option<u64>,
}

impl Default for WpsSettings {
    fn default() -> Self {
        Self {
            save_dir: None,
            crop_dir: None,
            interval_ms: 500,
            rounded_rx: 16,
            last_hwnd: None,
            language: None,
            keybindings: None,
            scroll_direction: Some("down".into()),
            scroll_amount: Some(120),
            scroll_frames: Some(5),
            scroll_delay_ms: Some(500),
            pano_direction: Some("right".into()),
            pano_drag_distance: Some(500),
            pano_frames: Some(5),
            pano_delay_ms: Some(300),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFrameDto {
    pub path: String,
    pub name: String,
    pub modified_ms: u128,
    pub size_bytes: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StitchProgressDto {
    pub current: u32,
    pub total: u32,
    pub phase: String,
}

fn normalize_dir(dir: Option<String>) -> Option<String> {
    dir.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn normalize_settings(mut settings: WpsSettings) -> WpsSettings {
    settings.interval_ms = settings.interval_ms.clamp(50, 60_000);
    settings.rounded_rx = settings.rounded_rx.min(128);
    settings.save_dir = normalize_dir(settings.save_dir);
    settings.crop_dir = normalize_dir(settings.crop_dir);
    settings.scroll_amount = settings.scroll_amount.map(|v| v.clamp(1, 10000));
    settings.scroll_frames = settings.scroll_frames.map(|v| v.clamp(2, 100));
    settings.scroll_delay_ms = settings.scroll_delay_ms.map(|v| v.clamp(50, 10000));
    settings.pano_drag_distance = settings.pano_drag_distance.map(|v| v.clamp(10, 10000));
    settings.pano_frames = settings.pano_frames.map(|v| v.clamp(2, 50));
    settings.pano_delay_ms = settings.pano_delay_ms.map(|v| v.clamp(50, 10000));
    settings
}

fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get app config dir: {}", e))?;
    std::fs::create_dir_all(&config_dir).map_err(|e| format!("Create config dir failed: {}", e))?;
    Ok(config_dir.join("window-pixel-studio.settings.json"))
}

fn load_settings_from(path: &Path) -> Result<WpsSettings, String> {
    if !path.exists() {
        return Ok(WpsSettings::default());
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let settings: WpsSettings = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(normalize_settings(settings))
}

fn list_windows_impl() -> Vec<WindowDto> {
    xcap_capture::list_windows()
}

fn capture_impl(hwnd: i64) -> Result<CaptureDto, String> {
    xcap_capture::capture_window_png(hwnd)
}

#[tauri::command]
fn wps_list_windows() -> Vec<WindowDto> {
    list_windows_impl()
}

/// Legacy full-window capture (includes title bar).
#[tauri::command]
fn wps_capture_window(hwnd: i64) -> Result<CaptureDto, String> {
    capture_impl(hwnd)
}

/// Client-area-only capture (no title bar / borders).
/// Uses PrintWindowCapture with PW_CLIENTONLY under the hood.
#[tauri::command]
async fn wps_capture_client(hwnd: i64) -> Result<CaptureDto, String> {
    xcap_capture::capture_client_area_png(hwnd).await
}

#[tauri::command]
fn wps_save_png(path: String, data: String) -> Result<String, String> {
    let payload = data.trim();
    let b64 = if let Some(idx) = payload.find(',') {
        if payload[..idx].starts_with("data:") {
            &payload[idx + 1..]
        } else {
            payload
        }
    } else {
        payload
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| e.to_string())?;
    let out = PathBuf::from(path);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&out, bytes).map_err(|e| e.to_string())?;
    Ok(out.to_string_lossy().to_string())
}

#[tauri::command]
fn wps_read_frame_base64(path: String) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[tauri::command]
fn wps_read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn wps_list_saved_frames(dir: String) -> Result<Vec<SavedFrameDto>, String> {
    let dir_path = PathBuf::from(dir);
    if !dir_path.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::<SavedFrameDto>::new();
    for entry in std::fs::read_dir(&dir_path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        // Skip hidden files (starting with .)
        if name.starts_with('.') {
            continue;
        }
        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            continue;
        };
        let e = ext.to_ascii_lowercase();
        if !matches!(e.as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp" | "json") {
            continue;
        }
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        let modified_ms = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis())
            .unwrap_or(0);
        out.push(SavedFrameDto {
            path: path.to_string_lossy().to_string(),
            name,
            modified_ms,
            size_bytes: meta.len(),
        });
    }
    out.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
    Ok(out)
}

#[tauri::command]
fn wps_load_settings(app: tauri::AppHandle) -> Result<WpsSettings, String> {
    let path = settings_path(&app)?;
    let settings = load_settings_from(&path)?;
    if !path.exists() {
        let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())?;
    }
    Ok(settings)
}

#[tauri::command]
fn wps_save_settings(app: tauri::AppHandle, settings: WpsSettings) -> Result<WpsSettings, String> {
    let path = settings_path(&app)?;
    let normalized = normalize_settings(settings);
    let text = serde_json::to_string_pretty(&normalized).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())?;
    Ok(normalized)
}

#[tauri::command]
fn wps_save_json(path: String, data: String) -> Result<String, String> {
    let out = PathBuf::from(&path);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&out, data).map_err(|e| e.to_string())?;
    Ok(out.to_string_lossy().to_string())
}

/// Read `.wps_counter` from dir, increment, write back, return the new value.
/// Creates the file (starting at 1) if it doesn't exist.
#[tauri::command]
fn wps_next_counter(dir: String) -> Result<u64, String> {
    let dir_path = PathBuf::from(&dir);
    std::fs::create_dir_all(&dir_path).map_err(|e| e.to_string())?;
    let counter_path = dir_path.join(".wps_counter");
    let current: u64 = if counter_path.exists() {
        let text = std::fs::read_to_string(&counter_path).map_err(|e| e.to_string())?;
        text.trim().parse::<u64>().unwrap_or(0)
    } else {
        0
    };
    let next = current + 1;
    std::fs::write(&counter_path, next.to_string()).map_err(|e| e.to_string())?;
    Ok(next)
}

#[tauri::command]
fn wps_reveal_in_explorer(path: String) -> Result<(), String> {
    std::process::Command::new("explorer")
        .args(["/select,", &path])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn wps_auto_match_window(title_keyword: String) -> Result<WindowDto, String> {
    let windows = xcap_capture::list_windows();
    let keyword_lower = title_keyword.to_lowercase();
    windows
        .into_iter()
        .find(|w| w.title.to_lowercase().contains(&keyword_lower))
        .ok_or_else(|| format!("No window matching '{}' found", title_keyword))
}

// ─── File management commands ─────────────────────────────────────────────────

#[tauri::command]
fn wps_delete_file(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if p.exists() {
        std::fs::remove_file(&p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn wps_delete_files(paths: Vec<String>) -> Result<u32, String> {
    let mut count = 0u32;
    for path in paths {
        let p = PathBuf::from(&path);
        if p.exists() && p.is_file() {
            std::fs::remove_file(&p).map_err(|e| e.to_string())?;
            count += 1;
        }
    }
    Ok(count)
}

#[tauri::command]
fn wps_clear_directory(dir: String) -> Result<u32, String> {
    let dir_path = PathBuf::from(&dir);
    if !dir_path.exists() {
        return Ok(0);
    }
    let mut count = 0u32;
    for entry in std::fs::read_dir(&dir_path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().is_file() {
            std::fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            count += 1;
        }
    }
    Ok(count)
}

// ─── Advanced capture commands ────────────────────────────────────────────────

/// Scroll capture: simulates mouse wheel, captures frames, stitches vertically.
#[tauri::command]
async fn wps_scroll_capture(
    app: tauri::AppHandle,
    hwnd: i64,
    direction: String,
    scroll_amount: i32,
    frame_count: u32,
    delay_ms: u64,
) -> Result<CaptureDto, String> {
    let handle = app.clone();
    stitch::scroll_capture(hwnd, direction, scroll_amount, frame_count, delay_ms, move |msg| {
        let _ = handle.emit("wps-capture-progress", msg);
    })
    .await
}

/// Panoramic capture: simulates middle-mouse drag, captures frames, stitches.
#[tauri::command]
async fn wps_panoramic_capture(
    app: tauri::AppHandle,
    hwnd: i64,
    direction: String,
    drag_distance: i32,
    frame_count: u32,
    delay_ms: u64,
) -> Result<CaptureDto, String> {
    let handle = app.clone();
    stitch::panoramic_capture(hwnd, direction, drag_distance, frame_count, delay_ms, move |msg| {
        let _ = handle.emit("wps-capture-progress", msg);
    })
    .await
}

// ─── App setup ───────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            wps_list_windows,
            wps_capture_window,
            wps_capture_client,
            wps_save_png,
            wps_read_frame_base64,
            wps_read_text_file,
            wps_list_saved_frames,
            wps_load_settings,
            wps_save_settings,
            wps_auto_match_window,
            wps_save_json,
            wps_next_counter,
            wps_reveal_in_explorer,
            wps_delete_file,
            wps_delete_files,
            wps_clear_directory,
            wps_scroll_capture,
            wps_panoramic_capture,
        ])
        .setup(|app| {
            let gs = app.global_shortcut();
            let _ = gs.unregister_all();

            let handle = app.handle().clone();
            if let Err(e) = gs.on_shortcut("F9", move |_app, _shortcut, event| {
                if event.state != ShortcutState::Pressed {
                    return;
                }
                let _ = handle.emit("wps-hotkey-capture", ());
            }) {
                eprintln!("Warning: Failed to register F9 shortcut: {e}");
            }

            let handle2 = app.handle().clone();
            if let Err(e) = gs.on_shortcut("F10", move |_app, _shortcut, event| {
                if event.state != ShortcutState::Pressed {
                    return;
                }
                let _ = handle2.emit("wps-hotkey-toggle-record", ());
            }) {
                eprintln!("Warning: Failed to register F10 shortcut: {e}");
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
