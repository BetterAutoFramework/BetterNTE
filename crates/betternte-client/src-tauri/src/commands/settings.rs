//! Settings commands — config / capture methods / subscriptions / windows / logs

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tauri::AppHandle;

use betternte_core::config::NotificationConfig;
use betternte_core::EngineConfig;
use betternte_engine::notify_builder;
use serde::{Deserialize, Serialize};

use crate::{persist_engine_config_file, save_config, AppState};

static WINDOW_QUERY_GATE: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
pub struct GamePluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub manifest_path: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PluginManifestLite {
    id: Option<String>,
    name: Option<String>,
    version: Option<String>,
}

fn resolve_data_root_path(cfg: &EngineConfig, base_dir: &Path) -> PathBuf {
    let p = PathBuf::from(cfg.scripts.data_root.clone());
    if p.is_absolute() {
        p
    } else {
        base_dir.join(p)
    }
}

fn resolve_active_plugin_manifest_path(cfg: &EngineConfig, base_dir: &Path) -> Option<PathBuf> {
    let active = if cfg.active_plugin.trim().is_empty() {
        "nte"
    } else {
        cfg.active_plugin.trim()
    };
    let search_paths = if cfg.plugin_search_paths.is_empty() {
        vec!["plugins".to_string()]
    } else {
        cfg.plugin_search_paths.clone()
    };
    let data_root = resolve_data_root_path(cfg, base_dir);
    for rel in search_paths {
        let root = {
            let p = PathBuf::from(rel);
            if p.is_absolute() {
                p
            } else {
                data_root.join(p)
            }
        };
        let manifest = root.join(active).join("manifest.json");
        if manifest.is_file() {
            return Some(manifest);
        }
    }
    None
}

fn normalized_active_plugin_id(cfg: &EngineConfig) -> String {
    let active = cfg.active_plugin.trim();
    if active.is_empty() {
        "nte".to_string()
    } else {
        active.to_string()
    }
}

fn apply_plugin_manifest_overrides(cfg: &mut EngineConfig, base_dir: &Path) -> Result<(), String> {
    let Some(manifest_path) = resolve_active_plugin_manifest_path(cfg, base_dir) else {
        return Ok(());
    };
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Failed to read plugin manifest: {}", e))?;
    let manifest: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid plugin manifest JSON: {}", e))?;
    let Some(root) = manifest.as_object() else {
        return Err("Plugin manifest root must be an object".into());
    };

    if let Some(game_obj) = root.get("game").and_then(|v| v.as_object()) {
        if let Some(v) = game_obj.get("game_name").and_then(|v| v.as_str()) {
            if !v.trim().is_empty() {
                cfg.game.game_name = v.to_string();
            }
        }
        if let Some(v) = game_obj
            .get("window_title_keyword")
            .and_then(|v| v.as_str())
        {
            if !v.trim().is_empty() {
                cfg.game.window_title_keyword = v.to_string();
            }
        }
        if let Some(v) = game_obj.get("process_name").and_then(|v| v.as_str()) {
            if !v.trim().is_empty() {
                cfg.game.process_name = v.to_string();
            }
        }
        if let Some(v) = game_obj.get("game_language").and_then(|v| v.as_str()) {
            cfg.game.game_language = v.to_string();
        }
        if let Some(v) = game_obj.get("resolution").and_then(|v| v.as_str()) {
            cfg.game.resolution = v.to_string();
        }
        if let Some(v) = game_obj.get("scale").and_then(|v| v.as_f64()) {
            cfg.game.scale = v;
        }
        if let Some(v) = game_obj.get("dpi").and_then(|v| v.as_u64()) {
            cfg.game.dpi = v as u32;
        }
    }
    if cfg.game.window_title_keyword.trim().is_empty() {
        if let Some(v) = root
            .get("window_match")
            .and_then(|wm| wm.get("title_keyword"))
            .and_then(|v| v.as_str())
        {
            cfg.game.window_title_keyword = v.to_string();
        }
    }
    if cfg.game.game_name.trim().is_empty() {
        if let Some(v) = root.get("name").and_then(|v| v.as_str()) {
            cfg.game.game_name = v.to_string();
        }
    }

    if let Some(scripts_obj) = root.get("scripts").and_then(|v| v.as_object()) {
        if let Some(v) = scripts_obj.get("data_root").and_then(|v| v.as_str()) {
            cfg.scripts.data_root = v.to_string();
        }
        if let Some(v) = scripts_obj.get("auto_update").and_then(|v| v.as_bool()) {
            cfg.scripts.auto_update = v;
        }
        if let Some(v) = scripts_obj.get("subscriptions") {
            if let Ok(parsed) =
                serde_json::from_value::<Vec<betternte_core::Subscription>>(v.clone())
            {
                cfg.scripts.subscriptions = parsed;
            }
        }
    }

    if let Some(capture_obj) = root.get("capture") {
        if let Ok(parsed_capture) =
            serde_json::from_value::<betternte_core::config::CaptureConfig>(capture_obj.clone())
        {
            cfg.capture = parsed_capture;
        }
    }
    Ok(())
}

fn sync_config_fields_to_plugin_manifest(
    cfg: &EngineConfig,
    base_dir: &Path,
) -> Result<(), String> {
    let Some(manifest_path) = resolve_active_plugin_manifest_path(cfg, base_dir) else {
        return Ok(());
    };
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Failed to read plugin manifest: {}", e))?;
    let mut manifest: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid plugin manifest JSON: {}", e))?;

    let root = manifest
        .as_object_mut()
        .ok_or("Plugin manifest root must be an object")?;

    let game_obj = root
        .entry("game")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .ok_or("Plugin manifest 'game' must be an object")?;

    game_obj.insert(
        "game_name".into(),
        serde_json::Value::String(cfg.game.game_name.clone()),
    );
    game_obj.insert(
        "window_title_keyword".into(),
        serde_json::Value::String(cfg.game.window_title_keyword.clone()),
    );
    game_obj.insert(
        "process_name".into(),
        serde_json::Value::String(cfg.game.process_name.clone()),
    );
    game_obj.insert(
        "game_language".into(),
        serde_json::Value::String(cfg.game.game_language.clone()),
    );
    game_obj.insert(
        "resolution".into(),
        serde_json::Value::String(cfg.game.resolution.clone()),
    );
    game_obj.insert("scale".into(), serde_json::json!(cfg.game.scale));
    game_obj.insert("dpi".into(), serde_json::json!(cfg.game.dpi));

    let scripts_obj = root
        .entry("scripts")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .ok_or("Plugin manifest 'scripts' must be an object")?;

    scripts_obj.insert(
        "data_root".into(),
        serde_json::Value::String(cfg.scripts.data_root.clone()),
    );
    scripts_obj.insert(
        "auto_update".into(),
        serde_json::Value::Bool(cfg.scripts.auto_update),
    );
    scripts_obj.insert(
        "subscriptions".into(),
        serde_json::to_value(&cfg.scripts.subscriptions)
            .map_err(|e| format!("Failed to serialize subscriptions: {}", e))?,
    );

    root.insert(
        "capture".into(),
        serde_json::to_value(&cfg.capture)
            .map_err(|e| format!("Failed to serialize capture config: {}", e))?,
    );

    let pretty = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Failed to serialize plugin manifest: {}", e))?;
    std::fs::write(&manifest_path, pretty)
        .map_err(|e| format!("Failed to write plugin manifest: {}", e))?;
    Ok(())
}

/// Get the current engine config.
#[tauri::command]
pub async fn get_config(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let guard = state.read_engine("getting config").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    Ok(serde_json::to_value(engine.config()).unwrap_or_default())
}

/// Save updated config to file and apply to engine.
#[tauri::command]
pub async fn save_config_cmd(
    config: serde_json::Value,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let mut new_config: EngineConfig =
        serde_json::from_value(config).map_err(|e| format!("Invalid config: {}", e))?;
    crate::ensure_game_identity_defaults(&mut new_config);

    let current_config = {
        let guard = state.read_engine("reading current config").await?;
        guard.as_ref().map(|engine| engine.config().clone())
    };

    let base_dir = {
        let guard = state.read_engine("reading config base dir").await?;
        let engine = guard.as_ref().ok_or("Engine not initialized")?;
        engine.config_base_dir().to_path_buf()
    };

    if let Some(current) = &current_config {
        if normalized_active_plugin_id(current) != normalized_active_plugin_id(&new_config) {
            apply_plugin_manifest_overrides(&mut new_config, &base_dir)?;
        }
    }
    crate::ensure_game_identity_defaults(&mut new_config);

    let config_path_guard = state.config_path.lock().await;
    let config_path = config_path_guard.as_ref().ok_or("Config path not set")?;

    save_config(config_path, &new_config)?;
    sync_config_fields_to_plugin_manifest(&new_config, &base_dir)?;
    drop(config_path_guard);

    let mut guard = state.write_engine("applying config").await?;
    if let Some(engine) = guard.as_mut() {
        engine
            .set_config(new_config.clone())
            .await
            .map_err(|e| format!("Apply config failed: {}", e))?;
    }
    drop(guard);

    persist_engine_config_file(&state).await?;
    crate::hotkeys::register_hotkeys(&app, &state).await?;

    let applied = {
        let guard = state.read_engine("reading applied config").await?;
        guard.as_ref().map(|e| e.config().clone())
    };
    serde_json::to_value(&applied.ok_or("Engine not initialized")?)
        .map_err(|e| format!("Serialize config failed: {}", e))
}

/// 获取所有截图方式及其可用性、白名单状态。
#[tauri::command]
pub async fn get_capture_methods(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let guard = state.read_engine("getting capture methods").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    let methods = engine.available_capture_methods();
    let arr: Vec<serde_json::Value> = methods
        .into_iter()
        .map(|(name, available, in_wl)| {
            serde_json::json!({
                "value": name,
                "available": available,
                "in_whitelist": in_wl,
            })
        })
        .collect();

    Ok(serde_json::Value::Array(arr))
}

/// List all subscriptions from the current config.
#[tauri::command]
pub async fn list_subscriptions(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let guard = state.read_engine("listing subscriptions").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;
    let subs = &engine.config().scripts.subscriptions;
    serde_json::to_value(subs).map_err(|e| format!("Serialize error: {}", e))
}

/// Add or update a subscription in the config.
#[tauri::command]
pub async fn save_subscription(
    app: tauri::AppHandle,
    subscription: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let sub: betternte_core::Subscription =
        serde_json::from_value(subscription).map_err(|e| format!("Invalid subscription: {}", e))?;

    let mut guard = state.write_engine("saving subscription").await?;
    let engine = guard.as_mut().ok_or("Engine not initialized")?;
    let mut config = engine.config().clone();

    if let Some(existing) = config
        .scripts
        .subscriptions
        .iter_mut()
        .find(|s| s.directory == sub.directory)
    {
        *existing = sub.clone();
    } else {
        config.scripts.subscriptions.push(sub.clone());
    }

    engine
        .set_config(config.clone())
        .await
        .map_err(|e| format!("Apply config failed: {}", e))?;

    drop(guard);
    persist_engine_config_file(&state).await?;
    crate::hotkeys::register_hotkeys(&app, &state).await?;

    Ok(format!("Subscription '{}' saved", sub.name))
}

/// Delete a subscription by directory name.
#[tauri::command]
pub async fn delete_subscription(
    app: tauri::AppHandle,
    directory: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    if directory == "local" {
        return Err("Cannot delete the local subscription".into());
    }
    let mut guard = state.write_engine("deleting subscription").await?;
    let engine = guard.as_mut().ok_or("Engine not initialized")?;
    let mut config = engine.config().clone();

    config
        .scripts
        .subscriptions
        .retain(|s| s.directory != directory);
    engine
        .set_config(config.clone())
        .await
        .map_err(|e| format!("Apply config failed: {}", e))?;

    drop(guard);
    persist_engine_config_file(&state).await?;
    crate::hotkeys::register_hotkeys(&app, &state).await?;

    Ok(format!("Subscription '{}' deleted", directory))
}

/// List all visible windows in the system.
#[tauri::command]
pub async fn list_windows(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let gate = WINDOW_QUERY_GATE.get_or_init(|| tokio::sync::Mutex::new(()));
    let _guard = gate.lock().await;
    {
        let guard = state.read_engine("listing windows").await?;
        if guard.is_none() {
            return Err("Engine not initialized".into());
        }
    }

    let windows = betternte_engine::Engine::list_windows_static()
        .map_err(|e| format!("Failed to list windows: {}", e))?;

    Ok(serde_json::to_value(&windows).unwrap_or_default())
}

/// Find game window using configured title keyword and optional process name filter.
#[tauri::command]
pub async fn find_game_window(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let gate = WINDOW_QUERY_GATE.get_or_init(|| tokio::sync::Mutex::new(()));
    let _guard = gate.lock().await;
    let title = {
        let guard = state.read_engine("finding game window").await?;
        let engine = guard.as_ref().ok_or("Engine not initialized")?;
        engine.config().game.window_title_keyword.clone()
    };

    let window = betternte_engine::Engine::find_game_window_by_title(&title)
        .map_err(|e| format!("{}", e))?;

    Ok(serde_json::to_value(&window).unwrap_or_default())
}

/// Capture a test screenshot and return as base64 data URL.
#[tauri::command]
pub async fn test_screenshot(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let guard = state.read_engine("testing screenshot").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    let png = engine
        .test_screenshot()
        .await
        .map_err(|e| format!("Screenshot failed: {}", e))?;

    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    Ok(format!("data:image/png;base64,{}", b64))
}

/// Export log content to a user-selected file path.
#[tauri::command]
pub async fn export_logs(app: AppHandle, content: String) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;

    let path = app
        .dialog()
        .file()
        .set_title("导出日志")
        .set_file_name("logs.txt")
        .add_filter("Text", &["txt", "log"])
        .blocking_save_file();

    let path = match path {
        Some(p) => p.into_path().map_err(|e| format!("Invalid path: {}", e))?,
        None => return Err("Export cancelled".into()),
    };

    if let Some(parent) = path.parent() {
        if !parent.exists() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }
    }

    tokio::fs::write(&path, &content)
        .await
        .map_err(|e| format!("Failed to write log file: {}", e))?;

    Ok(format!("Logs exported to {}", path.display()))
}

/// List available game plugins from configured plugin search paths.
#[tauri::command]
pub async fn list_game_plugins(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<GamePluginInfo>, String> {
    let guard = state.read_engine("listing game plugins").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;
    let cfg = engine.config().clone();
    let base_dir = engine.config_base_dir().to_path_buf();
    drop(guard);

    let data_root_path = resolve_data_root_path(&cfg, &base_dir);

    let search_paths = if cfg.plugin_search_paths.is_empty() {
        vec!["plugins".to_string()]
    } else {
        cfg.plugin_search_paths.clone()
    };

    let mut entries: Vec<GamePluginInfo> = Vec::new();
    for rel in &search_paths {
        let root = {
            let p = std::path::PathBuf::from(rel);
            if p.is_absolute() {
                p
            } else {
                data_root_path.join(p)
            }
        };
        if !root.is_dir() {
            continue;
        }
        let Ok(dir_iter) = std::fs::read_dir(&root) else {
            continue;
        };
        for item in dir_iter.flatten() {
            let plugin_dir = item.path();
            if !plugin_dir.is_dir() {
                continue;
            }
            let manifest_path = plugin_dir.join("manifest.json");
            if !manifest_path.is_file() {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(&manifest_path) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_str::<PluginManifestLite>(&raw) else {
                continue;
            };
            let fallback_id = plugin_dir
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            entries.push(GamePluginInfo {
                id: manifest.id.unwrap_or_else(|| fallback_id.clone()),
                name: manifest.name.unwrap_or_else(|| fallback_id.clone()),
                version: manifest.version.unwrap_or_else(|| "unknown".to_string()),
                manifest_path: manifest_path.display().to_string(),
            });
        }
    }

    entries.sort_by(|a, b| a.id.cmp(&b.id));
    entries.dedup_by(|a, b| a.id == b.id);
    Ok(entries)
}

/// Build a notification config suitable for a one-off channel test from the settings UI.
fn notification_config_for_channel_test(
    mut base: NotificationConfig,
    ui_channel: &str,
) -> NotificationConfig {
    base.enabled = true;
    match ui_channel {
        "telegram" => base.telegram.enabled = true,
        "discord" => base.discord.enabled = true,
        "serverchan" => base.serverchan.enabled = true,
        "bark" => base.bark.enabled = true,
        _ => {}
    }
    base
}

/// Map UI channel id to the notifier name registered in [`notify_builder::build_notification_manager`].
fn resolve_registered_notifier_name(ui_channel: &str) -> &str {
    match ui_channel {
        "discord" => "webhook",
        other => other,
    }
}

/// Send a test notification on a single channel using the given notification config (e.g. current settings draft).
#[tauri::command]
pub async fn test_notification_channel(
    ui_channel: String,
    notifications: serde_json::Value,
) -> Result<String, String> {
    let base: NotificationConfig = serde_json::from_value(notifications)
        .map_err(|e| format!("Invalid notification config: {}", e))?;
    let cfg = notification_config_for_channel_test(base, &ui_channel);
    let mgr = notify_builder::build_notification_manager(&cfg);
    let name = resolve_registered_notifier_name(&ui_channel);
    mgr.test_channel(name).await.map_err(|e| e.to_string())?;
    Ok("ok".into())
}

/// True when process env `BETTER_NTE_DEBUG` is exactly `1` after ASCII trim (used by the settings UI).
#[tauri::command]
pub fn better_nte_debug_enabled() -> bool {
    std::env::var("BETTER_NTE_DEBUG")
        .map(|v| v.trim() == "1")
        .unwrap_or(false)
}
