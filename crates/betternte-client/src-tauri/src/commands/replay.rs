//! Replay timeline verification (R3): same logic as **`bnt replay-verify`**, confined to **`replay.artifact_root`**.

use crate::{validate_path_in, AppState};

/// Verify **`replay_expect.json`** + **`timeline.jsonl`** under `session_dir` (standard layout).
#[tauri::command]
pub async fn replay_verify_session(
    session_name: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let trimmed = session_name.trim();
    if trimmed.is_empty() {
        return Err("sessionName is empty".into());
    }
    if trimmed.contains("..") || trimmed.contains('/') || trimmed.contains('\\') {
        return Err(
            "sessionName must be a single directory name under replay.artifact_root (no path separators)"
                .into(),
        );
    }

    let guard = state.read_engine("verifying replay session").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;
    let root = replay_artifact_root(engine)?;

    let session_dir = validate_path_in(&root, trimmed)?;
    betternte_engine::replay_verify::verify_session_directory(&session_dir)
        .map_err(|e| e.to_string())?;

    Ok(format!("Replay verification passed: {:?}", session_dir))
}

/// Arbitrary artifact paths relative to **`replay.artifact_root`** (same semantics as CLI file args).
#[tauri::command]
pub async fn replay_verify_artifacts(
    expect_relative: String,
    timeline_relative: String,
    manifest_relative: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let guard = state.read_engine("verifying replay artifacts").await?;
    let engine = guard.as_ref().ok_or("Engine not initialized")?;
    let root = replay_artifact_root(engine)?;

    let expect_path = validate_path_in(&root, expect_relative.trim())?;
    let timeline_path = validate_path_in(&root, timeline_relative.trim())?;

    let manifest_path = match manifest_relative {
        Some(ref s) if !s.trim().is_empty() => Some(validate_path_in(&root, s.trim())?),
        _ => None,
    };

    betternte_engine::replay_verify::verify_artifact_paths(
        &expect_path,
        &timeline_path,
        manifest_path.as_deref(),
    )
    .map_err(|e| e.to_string())?;

    Ok("Replay verification passed".into())
}

fn replay_artifact_root(engine: &betternte_engine::Engine) -> Result<std::path::PathBuf, String> {
    let raw = engine.config().replay.artifact_root.trim();
    if raw.is_empty() {
        return Err(
            "replay.artifact_root is empty — set Replay artifact directory in Settings first"
                .into(),
        );
    }
    let plugin_id = if engine.config().active_plugin.trim().is_empty() {
        "nte"
    } else {
        engine.config().active_plugin.trim()
    };
    Ok(engine.resolved_config_path(raw).join(plugin_id))
}
