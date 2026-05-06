//! Input debug commands for Tauri input test page.

use std::sync::Arc;

use betternte_core::MouseButton;
use betternte_engine::script_ctx::EngineScriptContext;
use betternte_script::{Manifest, ScriptEngine, ScriptType};

use crate::AppState;

async fn bind_and_get_ctx(
    state: tauri::State<'_, AppState>,
    hwnd: Option<u64>,
) -> Result<Arc<EngineScriptContext>, String> {
    let guard = state.read_engine("binding input context").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;

    let target_hwnd = match hwnd {
        Some(v) => v,
        None => {
            engine
                .find_game_window()
                .map_err(|e| format!("Failed to find game window: {}", e))?
                .hwnd
        }
    };

    engine
        .bind_window(target_hwnd)
        .await
        .map_err(|e| format!("Failed to bind window: {}", e))?;

    engine
        .script_ctx_handle()
        .ok_or("Script context not initialized".to_string())
}

fn parse_mouse_button(button: &str) -> Result<MouseButton, String> {
    match button.trim().to_lowercase().as_str() {
        "left" => Ok(MouseButton::Left),
        "right" => Ok(MouseButton::Right),
        "middle" => Ok(MouseButton::Middle),
        "x1" => Ok(MouseButton::X1),
        "x2" => Ok(MouseButton::X2),
        _ => Err(format!("Unknown mouse button: {}", button)),
    }
}

#[tauri::command]
pub async fn input_list_windows(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let guard = state.read_engine("input list windows").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;
    let windows = engine
        .list_windows()
        .map_err(|e| format!("Failed to list windows: {}", e))?;
    Ok(serde_json::to_value(&windows).unwrap_or_default())
}

#[tauri::command]
pub async fn input_bind_window(
    hwnd: u64,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let guard = state.read_engine("input bind window").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;
    engine
        .bind_window(hwnd)
        .await
        .map_err(|e| format!("Failed to bind window: {}", e))?;
    Ok(format!("Bound input target: {}", hwnd))
}

#[tauri::command]
pub async fn input_key_down(
    key: String,
    hwnd: Option<u64>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let ctx = bind_and_get_ctx(state, hwnd).await?;
    ctx.input_key_down(&key)
        .await
        .map_err(|e| format!("key_down failed: {}", e))?;
    Ok(format!("Key down: {}", key))
}

#[tauri::command]
pub async fn input_key_up(
    key: String,
    hwnd: Option<u64>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let ctx = bind_and_get_ctx(state, hwnd).await?;
    ctx.input_key_up(&key)
        .await
        .map_err(|e| format!("key_up failed: {}", e))?;
    Ok(format!("Key up: {}", key))
}

#[tauri::command]
pub async fn input_key_tap(
    key: String,
    duration_ms: Option<u32>,
    hwnd: Option<u64>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let ctx = bind_and_get_ctx(state, hwnd).await?;
    ctx.input_key_tap(&key, duration_ms)
        .await
        .map_err(|e| format!("key_tap failed: {}", e))?;
    Ok(format!("Key tap: {} ({:?} ms)", key, duration_ms))
}

#[tauri::command]
pub async fn input_mouse_move(
    x: i32,
    y: i32,
    hwnd: Option<u64>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let ctx = bind_and_get_ctx(state, hwnd).await?;
    ctx.input_mouse_move(x, y)
        .await
        .map_err(|e| format!("mouse_move failed: {}", e))?;
    Ok(format!("Mouse move: ({}, {})", x, y))
}

#[tauri::command]
pub async fn input_mouse_scroll(
    delta: i32,
    hwnd: Option<u64>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let ctx = bind_and_get_ctx(state, hwnd).await?;
    ctx.input_scroll(delta)
        .await
        .map_err(|e| format!("mouse_scroll failed: {}", e))?;
    Ok(format!("Mouse scroll: {}", delta))
}

#[tauri::command]
pub async fn input_mouse_button(
    button: String,
    pressed: bool,
    hwnd: Option<u64>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let btn = parse_mouse_button(&button)?;
    let ctx = bind_and_get_ctx(state, hwnd).await?;
    if pressed {
        ctx.input_mouse_down(btn)
            .await
            .map_err(|e| format!("mouse_down failed: {}", e))?;
        Ok(format!("Mouse down: {}", button))
    } else {
        ctx.input_mouse_up(btn)
            .await
            .map_err(|e| format!("mouse_up failed: {}", e))?;
        Ok(format!("Mouse up: {}", button))
    }
}

#[tauri::command]
pub async fn input_mouse_click(
    x: i32,
    y: i32,
    button: String,
    double_click: bool,
    hwnd: Option<u64>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let btn = parse_mouse_button(&button)?;
    let ctx = bind_and_get_ctx(state, hwnd).await?;
    ctx.input_mouse_move(x, y)
        .await
        .map_err(|e| format!("mouse_move failed before click: {}", e))?;

    let click_once = || async {
        match btn {
            MouseButton::Left => ctx.input_click(x, y).await,
            MouseButton::Right => ctx.input_right_click(x, y).await,
            MouseButton::Middle | MouseButton::X1 | MouseButton::X2 => {
                ctx.input_mouse_down(btn).await?;
                ctx.input_mouse_up(btn).await
            }
        }
    };

    click_once()
        .await
        .map_err(|e| format!("mouse_click failed: {}", e))?;
    if double_click {
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        click_once()
            .await
            .map_err(|e| format!("mouse_double_click failed: {}", e))?;
    }

    Ok(format!(
        "Mouse {}click: {} at ({}, {})",
        if double_click { "double_" } else { "" },
        button,
        x,
        y
    ))
}

#[tauri::command]
pub async fn input_demo_alt_move(
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    hold_ms: u64,
    hwnd: Option<u64>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let ctx = bind_and_get_ctx(state, hwnd).await?;

    ctx.input_key_down("Alt")
        .await
        .map_err(|e| format!("Alt down failed: {}", e))?;

    let action_res = async {
        ctx.input_mouse_move(start_x, start_y).await?;
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        ctx.input_mouse_move(end_x, end_y).await?;
        tokio::time::sleep(std::time::Duration::from_millis(hold_ms)).await;
        // Click at the final position while Alt is still held.
        ctx.input_click(end_x, end_y).await?;
        Ok::<(), anyhow::Error>(())
    }
    .await;

    let release_res = ctx.input_key_up("Alt").await;

    if let Err(e) = action_res {
        let _ = release_res;
        return Err(format!("Alt+move action failed: {}", e));
    }

    release_res.map_err(|e| format!("Alt release failed: {}", e))?;

    Ok(format!(
        "Demo done: Alt held while moving mouse from ({}, {}) to ({}, {}), then clicked at end",
        start_x, start_y, end_x, end_y
    ))
}

#[tauri::command]
pub async fn input_demo_move_left_click(
    x: i32,
    y: i32,
    hwnd: Option<u64>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let ctx = bind_and_get_ctx(state, hwnd).await?;
    ctx.input_mouse_move(x, y)
        .await
        .map_err(|e| format!("mouse_move failed: {}", e))?;
    ctx.input_click(x, y)
        .await
        .map_err(|e| format!("left click failed: {}", e))?;
    Ok(format!(
        "Demo done: moved mouse to ({}, {}) then left-clicked",
        x, y
    ))
}

#[tauri::command]
pub async fn input_demo_middle_hold_move_click(
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    hold_ms: u64,
    hwnd: Option<u64>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let ctx = bind_and_get_ctx(state, hwnd).await?;

    let action_res = async {
        ctx.input_mouse_move(start_x, start_y).await?;
        ctx.input_mouse_down(MouseButton::Middle).await?;
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        ctx.input_mouse_move(end_x, end_y).await?;
        tokio::time::sleep(std::time::Duration::from_millis(hold_ms)).await;
        ctx.input_mouse_up(MouseButton::Middle).await?;
        ctx.input_click(end_x, end_y).await?;
        Ok::<(), anyhow::Error>(())
    }
    .await;

    action_res.map_err(|e| format!("middle hold + move + click failed: {}", e))?;
    Ok(format!(
        "Demo done: held middle button from ({}, {}) to ({}, {}), then clicked at end",
        start_x, start_y, end_x, end_y
    ))
}

fn wrap_snippet_as_script_source(code: &str) -> String {
    let trimmed = code.trim();
    if trimmed.contains("function start(") || trimmed.contains("async function start(") {
        return trimmed.to_string();
    }
    format!("async function start() {{\n{}\n}}", trimmed)
}

#[tauri::command]
pub async fn input_run_js_snippet(
    code: String,
    hwnd: Option<u64>,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return Err("code is empty".into());
    }

    let ctx = bind_and_get_ctx(state, hwnd).await?;
    let ctx_trait: Arc<dyn betternte_script::ScriptContext> = ctx;

    let source = wrap_snippet_as_script_source(trimmed);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let temp_path = std::env::temp_dir().join(format!("betternte_input_snippet_{}.js", ts));
    std::fs::write(&temp_path, source)
        .map_err(|e| format!("Failed to write temp snippet: {}", e))?;

    let manifest = Manifest {
        schema_version: 1,
        name: format!("input_snippet_{}", ts),
        display_name: "Input Snippet".to_string(),
        version: "0.0.0".to_string(),
        author: "tauri".to_string(),
        description: "temp input snippet".to_string(),
        icon: None,
        script_type: ScriptType::SoloTask,
        entry: "main.js".to_string(),
        settings_ui: None,
        permissions: vec![],
        tags: vec![],
        category: None,
        params_schema: None,
        min_engine_version: None,
        max_engine_version: None,
        engine_version: None,
        dependencies: vec![],
        design_resolution: None,
    };

    let engine =
        betternte_script::quickjs::QuickJsEngine::new("0.0.1").with_max_execution_ms(60_000);
    let mut script = engine
        .load(&temp_path, &manifest, std::env::temp_dir().as_path())
        .await
        .map_err(|e| format!("Failed to load snippet: {}", e))?;

    let run_res = script
        .start(&ctx_trait, &serde_json::json!({}))
        .await
        .map_err(|e| format!("Snippet execution failed: {}", e));
    let _ = std::fs::remove_file(&temp_path);
    run_res?;

    Ok(script
        .last_result()
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}
