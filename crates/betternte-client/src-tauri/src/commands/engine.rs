//! Engine lifecycle commands — init / start / stop / status / stop_all

use tauri::{AppHandle, Manager};

use betternte_engine::Engine;

use crate::{
    load_config, persist_engine_config_file, resolve_engine_base_dir, save_config,
    seed_bundled_user_data, spawn_event_bridge, AppState, EventBusLayer, EVENT_BRIDGE_HANDLE,
    RELOAD_HANDLE,
};

/// Initialize the engine (lightweight). Called on app startup.
#[tauri::command]
pub async fn init_engine(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    // Idempotent init: frontend reload should not recreate engine and leak runtime resources.
    {
        let guard = state.read_engine("checking init state").await?;
        if guard.is_some() {
            tracing::info!("init_engine: engine already initialized, skipping recreate");
            return Ok("Engine already initialized".into());
        }
    }

    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get app config dir: {}", e))?;

    let config_path = config_dir.join("config.json");

    let config = load_config(&config_path);

    if !config_path.exists() {
        save_config(&config_path, &config)?;
        tracing::info!("Created default config at {}", config_path.display());
    }

    *state.config_path.lock().await = Some(config_path);

    let base_dir = resolve_engine_base_dir(&app)?;
    seed_bundled_user_data(&app, &base_dir)?;

    let engine =
        Engine::new(config, base_dir).map_err(|e| format!("Engine creation failed: {}", e))?;

    let event_bus = engine.event_bus().clone();

    if let Some(handle) = RELOAD_HANDLE.get() {
        let event_bus_layer = EventBusLayer::new(event_bus.clone());
        if let Err(e) = handle.modify(|layer| *layer = Some(event_bus_layer)) {
            tracing::warn!(error = %e, "Failed to install EventBus tracing layer");
        }
    }

    {
        let handle_cell = EVENT_BRIDGE_HANDLE.get_or_init(|| tokio::sync::Mutex::new(None));
        let mut guard = handle_cell.lock().await;
        if let Some(old_handle) = guard.take() {
            old_handle.abort();
            tracing::info!("Aborted old event bridge");
        }
        let new_handle = spawn_event_bridge(app.clone(), event_bus);
        *guard = Some(new_handle);
    }

    *state.engine.write().await = Some(engine);

    persist_engine_config_file(&state).await?;
    crate::hotkeys::register_hotkeys(&app, &state).await?;

    tracing::info!("Engine created (idle)");
    Ok("Engine created".into())
}

/// Start the engine — full initialization of all components.
#[tauri::command]
pub async fn start_engine(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let mut guard = state.write_engine("starting engine").await?;
    let engine = guard
        .as_mut()
        .ok_or("Engine not initialized, call init_engine first")?;

    engine
        .start()
        .await
        .map_err(|e| format!("Engine start failed: {}", e))?;

    Ok("Engine started".into())
}

/// Stop the engine — releases runtime resources but keeps config.
#[tauri::command]
pub async fn stop_engine(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let mut guard = state.write_engine("stopping engine").await?;
    let engine = guard.as_mut().ok_or("Engine not initialized")?;

    engine
        .stop_all()
        .await
        .map_err(|e| format!("Failed to stop running tasks before engine stop: {}", e))?;

    engine
        .stop()
        .await
        .map_err(|e| format!("Engine stop failed: {}", e))?;

    Ok("Engine stopped".into())
}

/// Get current engine status.
#[tauri::command]
pub async fn get_status(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let guard = state.read_engine("reading status").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    let script_count = engine.scripts().len();
    let is_running = engine.is_running();
    let running_task = engine.running_task_name();
    let is_flow_running = engine.is_flow_running().await;

    let state_str = if is_running { "running" } else { "idle" };
    let task_type = if running_task.is_some() {
        if is_flow_running {
            Some("flow")
        } else {
            Some("script")
        }
    } else {
        None
    };

    Ok(serde_json::json!({
        "state": state_str,
        "task": running_task,
        "task_type": task_type,
        "progress": null,
        "uptime": 0,
        "script_count": script_count,
        "version": engine.version(),
        "capture_method": engine.resolved_capture_method(),
        "input_mode": engine.resolved_input_mode(),
    }))
}

/// Stop all running scripts and task groups.
#[tauri::command]
pub async fn stop_all(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let guard = state.read_engine("stopping all tasks").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    engine
        .stop_all()
        .await
        .map_err(|e| format!("Failed to stop: {}", e))?;

    Ok("All tasks stopped".into())
}
