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

fn resolve_data_root_path(cfg: &EngineConfig, base_dir: &Path) -> PathBuf {
    let p = PathBuf::from(cfg.scripts.data_root.clone());
    if p.is_absolute() {
        p
    } else {
        base_dir.join(p)
    }
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

    let config_path_guard = state.config_path.lock().await;
    let config_path = config_path_guard.as_ref().ok_or("Config path not set")?;

    save_config(config_path, &new_config)?;
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
