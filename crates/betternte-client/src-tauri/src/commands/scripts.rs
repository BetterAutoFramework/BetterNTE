//! Script management commands — list / reload / run / stop / enable / disable / CRUD / file ops

use crate::{persist_engine_config_file, validate_path_in, AppState};
use tauri::Manager;

/// Reload scripts from disk and return the updated list.
#[tauri::command]
pub async fn reload_scripts(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let pruned = {
        let mut guard = state.write_engine("reloading scripts").await?;
        let engine = guard.as_mut().ok_or("Engine not initialized")?;
        engine.reload_scripts()
    };

    let scripts = {
        let guard = state.read_engine("listing scripts after reload").await?;
        guard
            .as_ref()
            .map(|e| serde_json::to_value(e.scripts()).unwrap_or_default())
            .unwrap_or_default()
    };

    tracing::info!(
        pruned,
        count = guard_optional_script_len(&scripts),
        "reload_scripts called"
    );

    if pruned {
        persist_engine_config_file(&state).await?;
        crate::hotkeys::register_hotkeys(&app, &state).await?;
    }

    Ok(scripts)
}

fn guard_optional_script_len(value: &serde_json::Value) -> usize {
    value.as_array().map(|a| a.len()).unwrap_or(0)
}

/// List all loaded scripts.
#[tauri::command]
pub async fn list_scripts(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let guard = state.read_engine("listing scripts").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    let scripts = engine.scripts();
    tracing::info!(count = scripts.len(), "list_scripts called");
    for s in scripts.iter() {
        tracing::info!(name = %s.manifest.name, script_type = %s.manifest.script_type, loaded = s.loaded, "  script entry");
    }
    Ok(serde_json::to_value(scripts).unwrap_or_default())
}

/// Run a script by name.
///
/// If engine is idle, it will be auto-started first.
#[tauri::command]
pub async fn run_script(
    app: tauri::AppHandle,
    name: String,
    params: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    // Hold write lock only for engine start. Do not keep write lock during preflight,
    // otherwise a slow window lookup can block stop_task's read lock.
    {
        let mut guard = state.engine.try_write().map_err(|_| {
            "引擎忙碌中，请先停止当前任务后再重试".to_string()
        })?;
        let engine = guard.as_mut().ok_or("Engine not initialized")?;

        if !engine.is_running() {
            engine
                .start()
                .await
                .map_err(|e| format!("Engine auto-start failed: {}", e))?;
        }
    }
    let app_handle = app.clone();
    let name_spawn = name.clone();
    tauri::async_runtime::spawn(async move {
        let state_handle = app_handle.state::<AppState>();
        let result = {
            let guard = state_handle.read_engine("running script in background").await;
            let Ok(guard) = guard else {
                tracing::warn!(script = %name_spawn, "run_script background task: engine read lock timeout");
                return;
            };
            let Some(engine) = guard.as_ref() else {
                tracing::warn!(script = %name_spawn, "run_script background task: engine not initialized");
                return;
            };
            engine.run_script(&name_spawn, params).await
        };
        match result {
            Ok(_) => tracing::info!(script = %name_spawn, "Script finished"),
            Err(e) => tracing::warn!(script = %name_spawn, error = %e, "Script run failed"),
        }
    });

    Ok(serde_json::json!({"status":"started","script":name}))
}

/// Stop the currently running task.
#[tauri::command]
pub async fn stop_task(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let guard = state.read_engine("stopping task").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    engine
        .stop_task()
        .await
        .map_err(|e| format!("Failed to stop task: {}", e))?;

    Ok("Task stopped".into())
}

/// Enable a trigger script with its configuration params.
#[tauri::command]
pub async fn enable_trigger(
    name: String,
    params: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let guard = state.read_engine("enabling trigger").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    engine
        .enable_trigger(&name, params)
        .await
        .map_err(|e| format!("Failed to enable trigger: {}", e))?;

    Ok(format!("Trigger '{}' enabled", name))
}

/// Disable a trigger script.
#[tauri::command]
pub async fn disable_trigger(
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let guard = state.read_engine("disabling trigger").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    engine
        .disable_trigger(&name)
        .await
        .map_err(|e| format!("Failed to disable trigger: {}", e))?;

    Ok(format!("Trigger '{}' disabled", name))
}

/// Reload triggers from disk and return the updated list.
#[tauri::command]
pub async fn reload_triggers(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let pruned = {
        let mut guard = state.write_engine("reloading triggers").await?;
        let engine = guard.as_mut().ok_or("Engine not initialized")?;
        engine.reload_scripts()
    };

    let triggers = {
        let guard = state.read_engine("listing triggers after reload").await?;
        guard
            .as_ref()
            .map(|e| serde_json::to_value(e.triggers()).unwrap_or_default())
            .unwrap_or_default()
    };

    tracing::info!(
        pruned,
        count = guard_optional_script_len(&triggers),
        "reload_triggers called"
    );

    if pruned {
        persist_engine_config_file(&state).await?;
        crate::hotkeys::register_hotkeys(&app, &state).await?;
    }

    Ok(triggers)
}

/// List all loaded triggers (from data/triggers/).
#[tauri::command]
pub async fn list_triggers(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let guard = state.read_engine("listing triggers").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    let triggers = engine.triggers();
    tracing::info!(count = triggers.len(), "list_triggers called");
    Ok(serde_json::to_value(triggers).unwrap_or_default())
}

/// Create a new script or trigger with manifest.json and main.js.
#[tauri::command]
pub async fn create_script(
    name: String,
    display_name: String,
    script_type: String,
    description: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let mut guard = state.write_engine("creating script").await?;
    let engine = guard.as_mut().ok_or("Engine not initialized")?;

    engine
        .create_script(&name, &display_name, &script_type, &description)
        .await
        .map_err(|e| format!("Failed to create script: {}", e))?;

    Ok(format!("Script '{}' created", name))
}

/// Delete a script or trigger by name.
#[tauri::command]
pub async fn delete_script(
    app: tauri::AppHandle,
    name: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let pruned = {
        let mut guard = state.write_engine("deleting script").await?;
        let engine = guard.as_mut().ok_or("Engine not initialized")?;

        engine
            .delete_script(&name)
            .await
            .map_err(|e| format!("Failed to delete script: {}", e))?
    };

    if pruned {
        persist_engine_config_file(&state).await?;
        crate::hotkeys::register_hotkeys(&app, &state).await?;
    }

    Ok(format!("Script '{}' deleted", name))
}

/// List files in a script directory (relative to data_root).
#[tauri::command]
pub async fn list_script_files(
    script_dir: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    if script_dir.contains("..") {
        return Err("Invalid directory path".into());
    }

    let guard = state.read_engine("listing script files").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    engine
        .list_script_files(&script_dir)
        .map_err(|e| format!("Failed to list files: {}", e))
}

/// Read a source file from a script directory.
#[tauri::command]
pub async fn read_script_source(
    script_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let guard = state.read_engine("reading script source").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    let scripts_dir = engine.scripts_dir();
    let full_path = scripts_dir.join(&script_path);

    let canonical = full_path
        .canonicalize()
        .map_err(|e| format!("File not found: {}", e))?;
    let canonical_scripts = scripts_dir
        .canonicalize()
        .map_err(|e| format!("Scripts dir error: {}", e))?;
    if !canonical.starts_with(&canonical_scripts) {
        return Err("Path traversal detected".into());
    }

    std::fs::read_to_string(&canonical).map_err(|e| format!("Failed to read file: {}", e))
}

/// Save content to a source file in a script directory.
#[tauri::command]
pub async fn save_script_source(
    script_path: String,
    content: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let guard = state.read_engine("saving script source").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    let scripts_dir = engine.scripts_dir();
    let full_path = scripts_dir.join(&script_path);

    if let Some(parent) = full_path.parent() {
        if parent.exists() {
            validate_path_in(&scripts_dir, &script_path)?;
        } else {
            let canonical_scripts = scripts_dir
                .canonicalize()
                .map_err(|e| format!("Scripts dir error: {}", e))?;
            let mut check = parent;
            while !check.exists() {
                check = check.parent().ok_or("Invalid path")?;
            }
            let canonical_check = check
                .canonicalize()
                .map_err(|e| format!("Path error: {}", e))?;
            if !canonical_check.starts_with(&canonical_scripts) {
                return Err("Path traversal detected".into());
            }
        }
    }

    std::fs::write(&full_path, &content).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(format!("Saved {}", script_path))
}

/// Import an asset file into a script's assets directory.
#[tauri::command]
pub async fn import_script_asset(
    script_name: String,
    file_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    if script_name.contains("..") || script_name.contains('/') || script_name.contains('\\') {
        return Err("Invalid script name".into());
    }

    let guard = state.read_engine("importing script asset").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    let scripts_dir = engine.scripts_dir();
    let assets_dir = scripts_dir.join(&script_name).join("assets");

    if !assets_dir.exists() {
        let script_dir = scripts_dir.join(&script_name);
        let canonical_script = script_dir
            .canonicalize()
            .map_err(|e| format!("Script dir not found: {}", e))?;
        let canonical_scripts = scripts_dir
            .canonicalize()
            .map_err(|e| format!("Scripts dir error: {}", e))?;
        if !canonical_script.starts_with(&canonical_scripts) {
            return Err("Path traversal detected".into());
        }
        std::fs::create_dir_all(&assets_dir)
            .map_err(|e| format!("Failed to create assets dir: {}", e))?;
    }

    let source = std::path::Path::new(&file_path);
    if !source.is_file() {
        return Err("Source file not found or is not a regular file".into());
    }

    let file_name = source
        .file_name()
        .ok_or("Invalid file path")?
        .to_string_lossy();
    let dest = assets_dir.join(&*file_name);

    validate_path_in(
        &scripts_dir,
        &format!("{}/assets/{}", script_name, file_name),
    )?;

    std::fs::copy(source, &dest).map_err(|e| format!("Failed to copy file: {}", e))?;

    Ok(format!("{}/assets/{}", script_name, file_name))
}
