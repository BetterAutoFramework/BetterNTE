//! Global hotkey registration and handlers.

use std::collections::HashSet;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::AppState;

/// Import ScriptContext trait so we can call save_screenshot on Arc<EngineScriptContext>
use betternte_script::ScriptContext as _;

/// Register global hotkeys from current engine config.
///
/// Hotkeys that fail to register (e.g. already in use) are automatically cleared
/// from the engine config and persisted, so the frontend does not display stale bindings.
///
/// Returns the list of hotkey labels that were cleared due to registration failure.
pub async fn register_hotkeys(
    app: &AppHandle,
    state: &tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let (emergency_shortcut, toggle_overlay_shortcut, triggers) = {
        let guard = state.engine.read().await;
        let engine = guard.as_ref().ok_or("Engine not initialized")?;
        (
            engine.config().hotkeys.emergency_stop.trim().to_string(),
            engine.config().hotkeys.toggle_overlay.trim().to_string(),
            engine.config().hotkey_triggers.clone(),
        )
    };

    let manager = app.global_shortcut();
    manager
        .unregister_all()
        .map_err(|e| format!("Failed to clear old hotkeys: {}", e))?;

    let mut taken = HashSet::<String>::new();
    let mut failed = Vec::<String>::new();

    // ── Emergency stop ──────────────────────────────────────────────────
    if !emergency_shortcut.is_empty() {
        let app_handle = app.clone();
        match manager.on_shortcut(
            emergency_shortcut.as_str(),
            move |_app, _shortcut, event| {
                if event.state != ShortcutState::Pressed {
                    return;
                }

                let app_for_task = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_for_task.state::<AppState>();
                    let guard = state.engine.read().await;
                    let Some(engine) = guard.as_ref() else {
                        tracing::warn!("Emergency stop hotkey fired but engine is not initialized");
                        return;
                    };

                    match engine.stop_all().await {
                        Ok(_) => tracing::info!("Emergency stop triggered by global hotkey"),
                        Err(e) => tracing::error!(error = %e, "Emergency stop hotkey failed"),
                    }
                });
            },
        ) {
            Ok(()) => {
                taken.insert(emergency_shortcut.clone());
                tracing::info!(hotkey = %emergency_shortcut, "Registered emergency stop hotkey");
            }
            Err(e) => {
                tracing::warn!(
                    hotkey = %emergency_shortcut,
                    error = %e,
                    "Failed to register emergency stop hotkey; clearing from config"
                );
                failed.push(format!("紧急停止({})", emergency_shortcut));
            }
        }
    }

    // ── Toggle overlay ──────────────────────────────────────────────────
    if !toggle_overlay_shortcut.is_empty() {
        if taken.insert(toggle_overlay_shortcut.clone()) {
            let app_handle = app.clone();
            match manager.on_shortcut(
                toggle_overlay_shortcut.as_str(),
                move |_app, _shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }
                    let app_for_task = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let state = app_for_task.state::<AppState>();
                        let guard = state.engine.read().await;
                        let Some(engine) = guard.as_ref() else {
                            tracing::warn!(
                                "Toggle overlay hotkey fired but engine is not initialized"
                            );
                            return;
                        };
                        match engine.toggle_overlay() {
                            Ok(visible) => {
                                tracing::info!(visible, "Overlay toggled by global hotkey")
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Toggle overlay hotkey failed")
                            }
                        }
                    });
                },
            ) {
                Ok(()) => {
                    tracing::info!(
                        hotkey = %toggle_overlay_shortcut,
                        "Registered toggle overlay hotkey"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        hotkey = %toggle_overlay_shortcut,
                        error = %e,
                        "Failed to register toggle overlay hotkey; clearing from config"
                    );
                    failed.push(format!("切换叠层({})", toggle_overlay_shortcut));
                }
            }
        } else {
            tracing::warn!(
                hotkey = %toggle_overlay_shortcut,
                "Toggle overlay hotkey conflicts with existing registration"
            );
            failed.push(format!("切换叠层({})", toggle_overlay_shortcut));
        }
    }

    // ── Screenshot ────────────────────────────────────────────────────
    let screenshot_shortcut = {
        let guard = state.engine.read().await;
        let engine = guard.as_ref().ok_or("Engine not initialized")?;
        engine.config().hotkeys.screenshot.trim().to_string()
    };

    if !screenshot_shortcut.is_empty() {
        if taken.insert(screenshot_shortcut.clone()) {
            let app_handle = app.clone();
            match manager.on_shortcut(
                screenshot_shortcut.as_str(),
                move |_app, _shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }
                    let app_for_task = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let state = app_for_task.state::<AppState>();
                        let guard = state.engine.read().await;
                        let Some(engine) = guard.as_ref() else {
                            tracing::warn!("Screenshot hotkey fired but engine is not initialized");
                            return;
                        };

                        // Try script context first (engine must be running with capture active)
                        if let Some(ctx) = engine.script_ctx_handle() {
                            match ctx.save_screenshot(false).await {
                                Ok(path) => {
                                    tracing::info!(path = %path, "Screenshot saved via hotkey");
                                    let _ = app_for_task.emit("screenshot_saved", &path);
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "Screenshot via script context failed, trying fallback");
                                }
                            }
                        } else {
                            // Fallback: use test_screenshot and save manually
                            match engine.test_screenshot().await {
                                Ok(png) => {
                                    let user_profile = std::env::var("USERPROFILE").unwrap_or_default();
                                    let save_dir = std::path::PathBuf::from(&user_profile)
                                        .join("Pictures")
                                        .join("BetterNTE");
                                    let _ = tokio::fs::create_dir_all(&save_dir).await;
                                    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                                    let filename = format!("screenshot_{}.png", ts);
                                    let full = save_dir.join(&filename);
                                    match tokio::fs::write(&full, &png).await {
                                        Ok(_) => {
                                            tracing::info!(path = %full.display(), "Screenshot saved via hotkey (fallback)");
                                            let _ = app_for_task.emit("screenshot_saved", full.to_string_lossy().as_ref());
                                        }
                                        Err(e) => {
                                            tracing::error!(error = %e, "Failed to save screenshot");
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(error = %e, "Screenshot hotkey: both script context and fallback failed");
                                }
                            }
                        }
                    });
                },
            ) {
                Ok(()) => {
                    tracing::info!(
                        hotkey = %screenshot_shortcut,
                        "Registered screenshot hotkey"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        hotkey = %screenshot_shortcut,
                        error = %e,
                        "Failed to register screenshot hotkey; clearing from config"
                    );
                    failed.push(format!("截图({})", screenshot_shortcut));
                }
            }
        } else {
            tracing::warn!(
                hotkey = %screenshot_shortcut,
                "Screenshot hotkey conflicts with existing registration"
            );
            failed.push(format!("截图({})", screenshot_shortcut));
        }
    }

    // ── Script triggers ─────────────────────────────────────────────────
    let mut failed_scripts = Vec::<String>::new();
    for (shortcut, script_name) in triggers.scripts {
        let sc = shortcut.trim().to_string();
        let name = script_name.trim().to_string();
        if sc.is_empty() || name.is_empty() {
            continue;
        }
        if !taken.insert(sc.clone()) {
            tracing::warn!(
                shortcut = %sc,
                script = %name,
                "Shortcut already registered; skipping script trigger"
            );
            failed_scripts.push(sc);
            continue;
        }

        let app_handle = app.clone();
        let name_clone = name.clone();
        match manager.on_shortcut(sc.as_str(), move |_app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }

            let app_for_task = app_handle.clone();
            let name_spawn = name_clone.clone();
            tauri::async_runtime::spawn(async move {
                let state = app_for_task.state::<AppState>();
                let guard = state.engine.read().await;
                let Some(engine) = guard.as_ref() else {
                    tracing::warn!("Script hotkey fired but engine is not initialized");
                    return;
                };

                let Ok(resolved_spawn) = engine.resolve_script_run_key(&name_spawn).await else {
                    tracing::warn!(
                        script = %name_spawn,
                        "Script hotkey: could not resolve script key (ambiguous name or not loaded)"
                    );
                    return;
                };

                if engine.active_solo_script_name().as_deref() == Some(resolved_spawn.as_str()) {
                    match engine.stop_task().await {
                        Ok(_) => tracing::info!(script = %resolved_spawn, "Script stopped via global hotkey"),
                        Err(e) => tracing::warn!(script = %resolved_spawn, error = %e, "Script hotkey stop failed"),
                    }
                    return;
                }

                if let Err(e) = crate::preflight_bind_window(engine) {
                    tracing::warn!(error = %e, "preflight_bind_window failed for script hotkey");
                    return;
                }

                match engine
                    .run_script(&resolved_spawn, serde_json::json!({}))
                    .await
                {
                    Ok(_) => tracing::info!(script = %resolved_spawn, "Script started via global hotkey"),
                    Err(e) => tracing::warn!(script = %resolved_spawn, error = %e, "Script hotkey run failed"),
                }
            });
        }) {
            Ok(()) => {
                tracing::info!(shortcut = %sc, script = %name, "Registered script hotkey trigger");
            }
            Err(e) => {
                tracing::warn!(
                    shortcut = %sc,
                    script = %name,
                    error = %e,
                    "Failed to register script hotkey; clearing from config"
                );
                failed_scripts.push(sc);
            }
        }
    }

    // ── Task group triggers ─────────────────────────────────────────────
    let mut failed_groups = Vec::<String>::new();
    for (shortcut, group_id) in triggers.task_groups {
        let sc = shortcut.trim().to_string();
        let gid = group_id.trim().to_string();
        if sc.is_empty() || gid.is_empty() {
            continue;
        }
        if !taken.insert(sc.clone()) {
            tracing::warn!(
                shortcut = %sc,
                group = %gid,
                "Shortcut already registered; skipping task group trigger"
            );
            failed_groups.push(sc);
            continue;
        }

        let app_handle = app.clone();
        let gid_clone = gid.clone();
        match manager.on_shortcut(sc.as_str(), move |_app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }

            let app_for_task = app_handle.clone();
            let gid_spawn = gid_clone.clone();
            tauri::async_runtime::spawn(async move {
                let state = app_for_task.state::<AppState>();
                let guard = state.engine.read().await;
                let Some(engine) = guard.as_ref() else {
                    tracing::warn!("Task group hotkey fired but engine is not initialized");
                    return;
                };

                if engine.is_flow_running().await {
                    if let Some(running_id) = engine.running_flow_id() {
                        if running_id == gid_spawn {
                            match engine.stop_task_group(&gid_spawn).await {
                                Ok(_) => tracing::info!(group = %gid_spawn, "Task group stopped via global hotkey"),
                                Err(e) => tracing::warn!(group = %gid_spawn, error = %e, "Task group hotkey stop failed"),
                            }
                            return;
                        }
                    }
                }

                drop(guard);
                let mut guard = state.engine.write().await;
                let Some(engine) = guard.as_mut() else {
                    tracing::warn!("Task group hotkey fired but engine is not initialized");
                    return;
                };

                if let Err(e) = crate::preflight_bind_window(engine) {
                    tracing::warn!(error = %e, "preflight_bind_window failed for task group hotkey");
                    return;
                }

                match engine.run_task_group(&gid_spawn, serde_json::json!({})).await {
                    Ok(_) => tracing::info!(group = %gid_spawn, "Task group started via global hotkey"),
                    Err(e) => tracing::warn!(group = %gid_spawn, error = %e, "Task group hotkey run failed"),
                }
            });
        }) {
            Ok(()) => {
                tracing::info!(shortcut = %sc, group = %gid, "Registered task group hotkey trigger");
            }
            Err(e) => {
                tracing::warn!(
                    shortcut = %sc,
                    group = %gid,
                    error = %e,
                    "Failed to register task group hotkey; clearing from config"
                );
                failed_groups.push(sc);
            }
        }
    }

    // ── Clear failed hotkeys from engine config ─────────────────────────
    let has_failures = !failed.is_empty()
        || !failed_scripts.is_empty()
        || !failed_groups.is_empty();

    if has_failures {
        let mut guard = state.engine.write().await;
        if let Some(engine) = guard.as_mut() {
            let cfg = engine.config_mut();

            // Clear failed fixed hotkeys
            for label in &failed {
                if label.starts_with("紧急停止") {
                    cfg.hotkeys.emergency_stop.clear();
                } else if label.starts_with("切换叠层") {
                    cfg.hotkeys.toggle_overlay.clear();
                } else if label.starts_with("截图") {
                    cfg.hotkeys.screenshot.clear();
                }
            }

            // Clear failed script triggers
            for sc in &failed_scripts {
                cfg.hotkey_triggers.scripts.remove(sc);
            }

            // Clear failed task group triggers
            for sc in &failed_groups {
                cfg.hotkey_triggers.task_groups.remove(sc);
            }

            // Persist the cleaned config
            drop(guard);
            if let Err(e) = crate::persist_engine_config_file(state).await {
                tracing::warn!(error = %e, "Failed to persist config after clearing failed hotkeys");
            }
        }

        let mut all_cleared = failed.clone();
        for sc in &failed_scripts {
            all_cleared.push(format!("脚本快捷键({})", sc));
        }
        for sc in &failed_groups {
            all_cleared.push(format!("任务组快捷键({})", sc));
        }
        tracing::info!(cleared = ?all_cleared, "Cleared failed hotkey registrations from config");
    }

    tracing::info!("Global hotkeys registration finished");
    Ok(failed)
}
