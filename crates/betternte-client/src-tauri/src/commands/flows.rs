//! Task group and flow commands — list / run / stop / progress / CRUD

use crate::{persist_engine_config_file, preflight_bind_window, AppState};

/// List all loaded task groups.
#[tauri::command]
pub async fn list_task_groups(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let guard = state.read_engine("listing task groups").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    let groups = engine.list_task_groups();
    Ok(serde_json::to_value(&groups).unwrap_or_default())
}

/// Run a task group by name.
#[tauri::command]
pub async fn run_task_group(
    uuid: String,
    params: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let mut guard = state.write_engine("running task group").await?;
    let engine = guard.as_mut().ok_or("Engine not initialized")?;

    if !engine.is_running() {
        engine
            .start()
            .await
            .map_err(|e| format!("Engine auto-start failed: {}", e))?;
    }

    preflight_bind_window(engine)?;

    let result = engine
        .run_task_group(&uuid, params)
        .await
        .map_err(|e| format!("Task group failed: {}", e))?;

    Ok(result)
}

/// Run a flow by id or name.
#[tauri::command]
pub async fn run_flow(
    flow_id: String,
    params: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let mut guard = state.write_engine("running flow").await?;
    let engine = guard.as_mut().ok_or("Engine not initialized")?;

    if !engine.is_running() {
        engine
            .start()
            .await
            .map_err(|e| format!("Engine auto-start failed: {}", e))?;
    }

    preflight_bind_window(engine)?;

    engine
        .run_flow(&flow_id, params)
        .await
        .map_err(|e| format!("Flow failed: {}", e))
}

/// Stop a running task group.
#[tauri::command]
pub async fn stop_task_group(
    uuid: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let guard = state.read_engine("stopping task group").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    engine
        .stop_task_group(&uuid)
        .await
        .map_err(|e| format!("Failed to stop task group: {}", e))?;

    Ok("Task group stopped".into())
}

/// Stop a running flow.
#[tauri::command]
pub async fn stop_flow(
    flow_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let guard = state.read_engine("stopping flow").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    engine
        .stop_flow(&flow_id)
        .await
        .map_err(|e| format!("Failed to stop flow: {}", e))?;

    Ok("Flow stopped".into())
}

/// Get task group progress.
#[tauri::command]
pub async fn get_task_group_progress(
    _uuid: String,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let guard = state.read_engine("getting task group progress").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    Ok(engine
        .get_task_group_progress()
        .await
        .unwrap_or(serde_json::Value::Null))
}

/// Get flow progress.
#[tauri::command]
pub async fn get_flow_progress(
    _flow_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let guard = state.read_engine("getting flow progress").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    Ok(engine
        .get_flow_progress()
        .await
        .unwrap_or(serde_json::Value::Null))
}

/// Save a task group (create or update).
#[tauri::command]
pub async fn save_task_group(
    group: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let mut guard = state.write_engine("saving task group").await?;
    let engine = guard.as_mut().ok_or("Engine not initialized")?;

    engine
        .save_task_group(&group)
        .map_err(|e| format!("Failed to save task group: {}", e))?;

    Ok("Task group saved".into())
}

/// Delete a task group by name.
#[tauri::command]
pub async fn delete_task_group(
    app: tauri::AppHandle,
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let pruned = {
        let mut guard = state.write_engine("deleting task group").await?;
        let engine = guard.as_mut().ok_or("Engine not initialized")?;

        engine
            .delete_task_group(&name)
            .map_err(|e| format!("Failed to delete task group: {}", e))?
    };

    if pruned {
        persist_engine_config_file(&state).await?;
        crate::hotkeys::register_hotkeys(&app, &state).await?;
    }

    Ok("Task group deleted".into())
}

/// List all loaded flows (from data/flows/).
#[tauri::command]
pub async fn list_flows(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let guard = state.read_engine("listing flows").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    let flows = engine.flows();
    Ok(serde_json::to_value(flows).unwrap_or_default())
}

/// Save a flow definition to disk.
#[tauri::command]
pub async fn save_flow(
    flow: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let mut guard = state.write_engine("saving flow").await?;
    let engine = guard.as_mut().ok_or("Engine not initialized")?;

    let flow: betternte_engine::Flow =
        serde_json::from_value(flow).map_err(|e| format!("Invalid flow JSON: {}", e))?;

    engine
        .save_flow(&flow)
        .map_err(|e| format!("Failed to save flow: {}", e))?;

    Ok(format!("Flow '{}' saved", flow.id))
}

/// Delete a flow definition from disk.
#[tauri::command]
pub async fn delete_flow(
    app: tauri::AppHandle,
    flow_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let pruned = {
        let mut guard = state.write_engine("deleting flow").await?;
        let engine = guard.as_mut().ok_or("Engine not initialized")?;

        engine
            .delete_flow(&flow_id)
            .map_err(|e| format!("Failed to delete flow: {}", e))?
    };

    if pruned {
        persist_engine_config_file(&state).await?;
        crate::hotkeys::register_hotkeys(&app, &state).await?;
    }

    Ok(format!("Flow '{}' deleted", flow_id))
}
