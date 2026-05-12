//! Script runtime — manages script lifecycle and execution.

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::engine::{CaptureFrame, LogLevel, Script, ScriptContext, ScriptType};
use crate::loader::{ScriptInfo, ScriptLoader};
use crate::manifest::Manifest;

/// 脚本运行时，管理所有已加载脚本的生命周期。
pub struct ScriptRuntime {
    loader: ScriptLoader,
    scripts: RwLock<HashMap<String, LoadedScript>>,
    default_scripts_dir: PathBuf,
    active_task: RwLock<Option<String>>,
    active_cancel: RwLock<Option<crate::engine::CancellationToken>>,
}

pub struct LoadedScript {
    pub info: ScriptInfo,
    pub script: Box<dyn Script>,
    pub enabled: bool,
    pub last_enable_params: Option<serde_json::Value>,
}

/// Resolve a runtime script key: [`ScriptInfo::script_id`] or unique `manifest.name`.
///
/// When multiple directories share the same manifest name, callers must pass `script_id`.
fn resolve_runtime_script_key(
    scripts: &HashMap<String, LoadedScript>,
    key: &str,
) -> Result<String, String> {
    if scripts.contains_key(key) {
        return Ok(key.to_string());
    }
    let matches: Vec<String> = scripts
        .values()
        .filter(|s| s.info.manifest.name == key)
        .map(|s| s.info.script_id.clone())
        .collect();
    match matches.len() {
        0 => Err(format!("script '{}' not found", key)),
        1 => Ok(matches[0].clone()),
        _ => Err(format!(
            "ambiguous script name '{}': use script path id (e.g. '{}' or '{}')",
            key, matches[0], matches[1]
        )),
    }
}

impl ScriptRuntime {
    pub fn new(engine_version: &str, scripts_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            loader: ScriptLoader::new(engine_version)?,
            scripts: RwLock::new(HashMap::new()),
            default_scripts_dir: scripts_dir,
            active_task: RwLock::new(None),
            active_cancel: RwLock::new(None),
        })
    }

    pub fn register_engine(
        &mut self,
        extension: &str,
        engine: Box<dyn crate::engine::ScriptEngine>,
    ) {
        self.loader.register_engine(extension, engine);
    }

    /// Access the scripts map (for trigger enumeration in spawn_trigger_consumers).
    pub fn scripts(&self) -> &RwLock<HashMap<String, LoadedScript>> {
        &self.scripts
    }

    pub async fn load_all(&self) -> Result<Vec<ScriptInfo>> {
        self.load_from_dirs(std::slice::from_ref(&self.default_scripts_dir))
            .await
    }

    /// 从指定的多个目录加载脚本（支持多订阅源）。
    pub async fn load_from_dirs(&self, dirs: &[PathBuf]) -> Result<Vec<ScriptInfo>> {
        let mut all_infos = Vec::new();
        let data_root = self.default_scripts_dir.clone();
        for dir in dirs {
            match self.loader.discover_scripts(dir, &data_root) {
                Ok(infos) => all_infos.extend(infos),
                Err(e) => {
                    warn!(path = %dir.display(), error = %e, "Failed to discover scripts");
                }
            }
        }

        let mut scripts = self.scripts.write().await;

        for info in &all_infos {
            if !info.compatible {
                warn!(
                    "Skipping incompatible script '{}' (requires engine {})",
                    info.manifest.name,
                    info.manifest.format_requirement()
                );
                continue;
            }

            match self.loader.load_script(info).await {
                Ok(script) => {
                    let same_name: Vec<&str> = scripts
                        .values()
                        .filter(|s| s.info.manifest.name == info.manifest.name)
                        .map(|s| s.info.script_id.as_str())
                        .collect();
                    if !same_name.is_empty() {
                        warn!(
                            name = %info.manifest.name,
                            existing = ?same_name,
                            added = %info.script_id,
                            "Multiple script directories share the same manifest name"
                        );
                    }
                    let mut loaded_info = info.clone();
                    loaded_info.loaded = true;
                    scripts.insert(
                        info.script_id.clone(),
                        LoadedScript {
                            info: loaded_info,
                            script,
                            enabled: false,
                            last_enable_params: None,
                        },
                    );
                }
                Err(e) => {
                    error!("Failed to load script '{}': {}", info.manifest.name, e);
                }
            }
        }

        Ok(all_infos)
    }

    /// Resolve `script_id` or unique manifest `name` to the canonical [`ScriptInfo::script_id`].
    pub async fn resolve_script_run_key(&self, key: &str) -> Result<String, anyhow::Error> {
        let scripts = self.scripts.read().await;
        resolve_runtime_script_key(&scripts, key).map_err(anyhow::Error::msg)
    }

    pub async fn list_scripts(&self) -> Vec<ScriptInfo> {
        let scripts = self.scripts.read().await;
        scripts.values().map(|s| s.info.clone()).collect()
    }

    /// Reload a single script from disk, replacing the in-memory instance.
    ///
    /// This re-reads the source file and creates a fresh QuickJsScript with a new
    /// runtime/context, so edits are picked up without restarting the engine.
    /// Preserves the `enabled` state of the previous instance.
    pub async fn reload_single_script(
        &self,
        name: &str,
        ctx: Option<&Arc<dyn ScriptContext>>,
    ) -> Result<()> {
        let key = {
            let scripts = self.scripts.read().await;
            match resolve_runtime_script_key(&scripts, name) {
                Ok(k) => k,
                Err(e) => {
                    if e.contains("ambiguous") {
                        return Err(anyhow::anyhow!(e));
                    }
                    return Ok(());
                }
            }
        };

        // Phase 1: get the ScriptInfo under a read lock, then release
        let (mut info, was_enabled, last_enable_params, script_path) = {
            let scripts = self.scripts.read().await;
            match scripts.get(&key) {
                Some(loaded) => (
                    loaded.info.clone(),
                    loaded.enabled,
                    loaded.last_enable_params.clone(),
                    loaded.info.path.clone(),
                ),
                None => return Ok(()), // not found — caller will handle the error
            }
        };

        self.loader.refresh_manifest_from_disk(&mut info);
        let design_res = info.manifest.design_resolution.map(|r| (r[0], r[1]));

        // Phase 2: load fresh instance from disk (no lock held)
        let script = self.loader.load_script(&info).await?;

        // Phase 3: swap the instance under a write lock, preserving enabled state
        let mut scripts = self.scripts.write().await;
        if let Some(existing) = scripts.get_mut(&key) {
            existing.script = script;
            existing.enabled = was_enabled;
            existing.last_enable_params = last_enable_params.clone();
            existing.info = info;
            info!("Reloaded script from disk: {}", key);
        }
        // else: script was removed between locks — nothing to do
        drop(scripts);

        if was_enabled {
            if let (Some(ctx), Some(params)) = (ctx, last_enable_params) {
                ctx.set_template_dir(script_path);
                ctx.set_design_resolution(design_res);
                let mut scripts = self.scripts.write().await;
                if let Some(existing) = scripts.get_mut(&key) {
                    existing.script.on_enable(ctx, &params).await?;
                    info!("Re-enabled script '{}' after reload", key);
                }
            }
        }

        Ok(())
    }

    pub async fn get_manifest(&self, name: &str) -> Option<Manifest> {
        let scripts = self.scripts.read().await;
        let key = resolve_runtime_script_key(&scripts, name).ok()?;
        scripts.get(&key).map(|s| s.info.manifest.clone())
    }

    pub async fn enable_script(
        &self,
        name: &str,
        ctx: &Arc<dyn ScriptContext>,
        params: &serde_json::Value,
    ) -> Result<()> {
        let key = {
            let scripts = self.scripts.read().await;
            resolve_runtime_script_key(&scripts, name)
                .map_err(|e| crate::error::ScriptError::LoadFailed(e))?
        };
        let mut scripts = self.scripts.write().await;
        let loaded = scripts.get_mut(&key).ok_or_else(|| {
            crate::error::ScriptError::LoadFailed(format!("script '{}' not found", name))
        })?;

        if loaded.enabled {
            return Ok(());
        }

        // Set template directory for this script
        ctx.set_template_dir(loaded.info.path.clone());
        ctx.set_design_resolution(loaded.info.manifest.design_resolution.map(|r| (r[0], r[1])));
        {
            let mut active_cancel = self.active_cancel.write().await;
            *active_cancel = loaded.script.cancellation_token();
        }

        loaded.script.on_enable(ctx, params).await?;
        loaded.enabled = true;
        loaded.last_enable_params = Some(params.clone());
        info!("Enabled script: {}", key);
        Ok(())
    }

    pub async fn disable_script(&self, name: &str, ctx: &Arc<dyn ScriptContext>) -> Result<()> {
        let key = {
            let scripts = self.scripts.read().await;
            resolve_runtime_script_key(&scripts, name)
                .map_err(|e| crate::error::ScriptError::LoadFailed(e))?
        };
        let mut scripts = self.scripts.write().await;
        let loaded = scripts.get_mut(&key).ok_or_else(|| {
            crate::error::ScriptError::LoadFailed(format!("script '{}' not found", name))
        })?;

        if !loaded.enabled {
            return Ok(());
        }

        loaded.script.on_disable(ctx).await?;
        loaded.enabled = false;
        info!("Disabled script: {}", key);
        Ok(())
    }

    pub async fn start_task(
        &self,
        name: &str,
        config: serde_json::Value,
        ctx: &Arc<dyn ScriptContext>,
    ) -> Result<()> {
        let key = {
            let scripts = self.scripts.read().await;
            resolve_runtime_script_key(&scripts, name).map_err(|e| anyhow::anyhow!(e))?
        };

        // Atomic check-and-set: use a single write lock to prevent TOCTOU race
        {
            let mut active = self.active_task.write().await;
            if active.is_some() {
                return Err(anyhow::anyhow!("A task is already running"));
            }
            *active = Some(key.clone());
        }

        // Take the script out so we don't hold scripts lock during async execution.
        let removed_entry = {
            let mut scripts = self.scripts.write().await;
            scripts.remove(&key)
        };
        let mut entry = match removed_entry {
            Some(entry) => entry,
            None => {
                let mut active = self.active_task.write().await;
                *active = None;
                return Err(crate::error::ScriptError::LoadFailed(format!(
                    "script '{}' not found",
                    key
                ))
                .into());
            }
        };

        let script_name = key.clone();
        let script_type = entry.info.manifest.script_type.clone();

        if script_type == ScriptType::Library {
            {
                let mut scripts = self.scripts.write().await;
                scripts.insert(key, entry);
            }
            {
                let mut active = self.active_task.write().await;
                *active = None;
            }
            return Err(crate::error::ScriptError::LibraryNotRunnable(name.to_string()).into());
        }

        // Set template directory for this script
        ctx.set_template_dir(entry.info.path.clone());
        ctx.set_design_resolution(entry.info.manifest.design_resolution.map(|r| (r[0], r[1])));
        {
            let mut active_cancel = self.active_cancel.write().await;
            *active_cancel = entry.script.cancellation_token();
        }

        let result = if script_type == ScriptType::Trigger {
            // Trigger scripts: enable and do a single on_capture tick
            entry.enabled = true;
            entry.script.on_enable(ctx, &config).await?;
            let frame = CaptureFrame {
                width: 0,
                height: 0,
                data: Arc::new(vec![]),
            };
            entry.script.on_capture(ctx, &frame).await
        } else {
            // Task scripts: call start()
            entry.script.start(ctx, &config).await
        };

        match &result {
            Ok(_) => info!("Task '{}' completed", script_name),
            Err(e) => {
                let detail = format!("{:#}", e);
                ctx.log(LogLevel::Error, &format!("脚本执行失败: {}", detail));
                error!(target: "betternte", script = %script_name, error = %detail, "script run failed");
            }
        }
        {
            let mut active_cancel = self.active_cancel.write().await;
            *active_cancel = None;
        }
        {
            let mut scripts = self.scripts.write().await;
            scripts.insert(script_name, entry);
        }
        {
            let mut active = self.active_task.write().await;
            *active = None;
        }
        result
    }

    pub async fn call_library(
        &self,
        library: &str,
        function: &str,
        args: serde_json::Value,
        ctx: &Arc<dyn ScriptContext>,
    ) -> Result<serde_json::Value> {
        let lib_key = {
            let scripts = self.scripts.read().await;
            resolve_runtime_script_key(&scripts, library).map_err(|e| anyhow::anyhow!(e))?
        };
        let removed_entry = {
            let mut scripts = self.scripts.write().await;
            scripts.remove(&lib_key)
        };
        let mut entry = match removed_entry {
            Some(entry) => entry,
            None => {
                return Err(crate::error::ScriptError::LibraryNotFound(library.to_string()).into())
            }
        };

        if entry.info.manifest.script_type != ScriptType::Library {
            let mut scripts = self.scripts.write().await;
            scripts.insert(lib_key, entry);
            return Err(crate::error::ScriptError::LibraryNotRunnable(library.to_string()).into());
        }

        let previous_template_dir = ctx.get_template_dir();
        let previous_design_res = ctx.get_design_resolution();
        ctx.set_template_dir(entry.info.path.clone());
        ctx.set_design_resolution(entry.info.manifest.design_resolution.map(|r| (r[0], r[1])));

        let result = entry.script.call_function(ctx, function, &args).await;

        if let Some(dir) = previous_template_dir {
            ctx.set_template_dir(dir);
        }
        ctx.set_design_resolution(previous_design_res);
        match &result {
            Ok(_) => info!("Library call '{}.{}' completed", lib_key, function),
            Err(e) => error!("Library call '{}.{}' failed: {}", lib_key, function, e),
        }

        {
            let mut scripts = self.scripts.write().await;
            scripts.insert(lib_key, entry);
        }

        result
    }

    pub async fn stop_task(&self, ctx: &Arc<dyn ScriptContext>) -> Result<()> {
        ctx.request_cancel();
        if let Some(token) = self.active_cancel.read().await.clone() {
            token.cancel();
        }
        let active_name = self.active_task.read().await.clone();
        let stop_result = if let Some(name) = active_name {
            let mut entry = {
                let mut scripts = self.scripts.write().await;
                scripts.remove(&name)
            };
            if let Some(ref mut loaded) = entry {
                let result = loaded.script.stop(ctx).await;
                if result.is_ok() {
                    info!("Stopped task: {}", name);
                }
                let mut scripts = self.scripts.write().await;
                if let Some(e) = entry {
                    scripts.insert(name.clone(), e);
                }
                result
            } else {
                Ok(())
            }
        } else {
            Ok(())
        };
        let mut active = self.active_task.write().await;
        *active = None;
        let mut active_cancel = self.active_cancel.write().await;
        *active_cancel = None;
        drop(active);
        drop(active_cancel);
        stop_result
    }

    pub async fn tick_triggers(
        &self,
        ctx: &Arc<dyn ScriptContext>,
        frame: &CaptureFrame,
    ) -> Result<()> {
        // Collect trigger names first.
        let trigger_names: Vec<String> = {
            let scripts = self.scripts.read().await;
            scripts
                .iter()
                .filter(|(_, s)| s.enabled && s.info.manifest.script_type == ScriptType::Trigger)
                .map(|(name, _)| name.clone())
                .collect()
        };

        // Run all enabled triggers concurrently for this frame.
        let mut join_set = tokio::task::JoinSet::new();
        for name in trigger_names {
            // Take the script out of the map so we don't hold the map lock during on_capture.
            let entry = {
                let mut scripts = self.scripts.write().await;
                match scripts.remove(&name) {
                    Some(entry) => entry,
                    None => continue, // removed between collection and execution
                }
            };
            let ctx_cloned = ctx.clone();
            let frame_cloned = frame.clone();
            join_set.spawn(async move {
                let mut entry = entry;
                let result = entry.script.on_capture(&ctx_cloned, &frame_cloned).await;
                (name, entry, result)
            });
        }

        while let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok((name, mut entry, result)) => {
                    let mut scripts = self.scripts.write().await;
                    if let Err(e) = result {
                        error!("Trigger '{}' error: {}", name, e);
                        entry.enabled = false;
                        warn!("Auto-disabled trigger '{}' due to error", name);
                    }
                    scripts.insert(name, entry);
                }
                Err(e) => {
                    error!("Trigger task join error: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Tick a single named trigger (for decoupled frame pool architecture).
    ///
    /// Each trigger runs in its own tokio task and calls this method independently.
    /// The script entry is temporarily removed from the map during execution to
    /// avoid holding the lock across the async `on_capture` call.
    pub async fn tick_single_trigger(
        &self,
        ctx: &Arc<dyn ScriptContext>,
        frame: &CaptureFrame,
        name: &str,
    ) -> Result<()> {
        let entry = {
            let mut scripts = self.scripts.write().await;
            match scripts.remove(name) {
                Some(entry) => entry,
                None => return Ok(()), // trigger was removed
            }
        };

        let mut entry = entry;
        let result = entry.script.on_capture(ctx, frame).await;

        let mut scripts = self.scripts.write().await;
        if let Err(e) = result {
            error!("Trigger '{}' error: {}", name, e);
            entry.enabled = false;
            warn!("Auto-disabled trigger '{}' due to error", name);
        }
        scripts.insert(name.to_string(), entry);

        Ok(())
    }

    pub async fn is_task_running(&self) -> bool {
        let active = self.active_task.read().await;
        active.is_some()
    }

    pub async fn active_task_name(&self) -> Option<String> {
        let active = self.active_task.read().await;
        active.clone()
    }

    pub fn active_task_name_sync(&self) -> Option<String> {
        self.active_task.try_read().ok().and_then(|g| g.clone())
    }

    pub async fn shutdown(&self, ctx: &Arc<dyn ScriptContext>) -> Result<()> {
        let names: Vec<String> = {
            let scripts = self.scripts.read().await;
            scripts.keys().cloned().collect()
        };
        for name in names {
            let mut entry = {
                let mut scripts = self.scripts.write().await;
                match scripts.remove(&name) {
                    Some(e) => e,
                    None => continue,
                }
            };
            if entry.enabled {
                let _ = entry.script.on_disable(ctx).await;
            }
            let _ = entry.script.destroy(ctx).await;
        }
        self.loader.unload_all().await?;
        crate::quickjs::bridge::shutdown_bridge();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{
        CancellationToken, ColorMatchPoint, FindTemplateOpts, LogLevel, MatchResult, OcrResult,
        Rect, Region, ScriptEngine,
    };
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::{Arc, Mutex, Weak};

    #[derive(Clone)]
    struct NoopCtx {
        config: serde_json::Value,
        template_dir: Arc<Mutex<PathBuf>>,
    }

    impl NoopCtx {
        fn new() -> Self {
            Self {
                config: json!({}),
                template_dir: Arc::new(Mutex::new(PathBuf::new())),
            }
        }
    }

    #[async_trait]
    impl ScriptContext for NoopCtx {
        fn is_cancelled(&self) -> bool {
            false
        }
        fn get_config(&self) -> &serde_json::Value {
            &self.config
        }
        fn progress(&self, _current: u32, _total: u32) {}
        fn get_fps(&self) -> f64 {
            60.0
        }
        fn get_frame_number(&self) -> u64 {
            0
        }
        fn set_template_dir(&self, dir: PathBuf) {
            *self.template_dir.lock().expect("lock template_dir") = dir;
        }
        fn get_template_dir(&self) -> Option<PathBuf> {
            Some(self.template_dir.lock().expect("lock template_dir").clone())
        }
        async fn capture(&self, _force: bool) -> Result<CaptureFrame> {
            Ok(CaptureFrame {
                width: 1,
                height: 1,
                data: Arc::new(vec![0, 0, 0, 0]),
            })
        }
        async fn capture_region(&self, _region: &Region, _force: bool) -> Result<CaptureFrame> {
            self.capture(false).await
        }
        async fn save_screenshot(&self, _force: bool) -> Result<String> {
            Ok(String::new())
        }
        async fn find_template(
            &self,
            _name: &str,
            _opts: Option<FindTemplateOpts>,
        ) -> Result<Option<MatchResult>> {
            Ok(None)
        }
        async fn find_templates(
            &self,
            _name: &str,
            _opts: Option<FindTemplateOpts>,
        ) -> Result<Vec<MatchResult>> {
            Ok(vec![])
        }
        async fn ocr(&self, _region: &Region) -> Result<String> {
            Ok(String::new())
        }
        async fn ocr_all(&self) -> Result<Vec<OcrResult>> {
            Ok(vec![])
        }
        async fn get_color(&self, _x: i32, _y: i32) -> Result<String> {
            Ok("#000000".into())
        }
        async fn color_match(
            &self,
            _x: i32,
            _y: i32,
            _color: &str,
            _tolerance: u8,
        ) -> Result<bool> {
            Ok(false)
        }
        async fn color_match_all(
            &self,
            _points: &[ColorMatchPoint],
            opts: &crate::engine::ColorMatchAllOpts,
        ) -> Result<crate::engine::ColorMatchAllResult> {
            Ok(crate::engine::ColorMatchAllResult {
                all_match: false,
                points: opts.debug.then_some(vec![]),
                matched_shift: None,
            })
        }
        async fn scan_slider_strip(&self, _opts: &serde_json::Value) -> Result<serde_json::Value> {
            Ok(serde_json::json!({
                "ok": false,
                "reason": "noop_context"
            }))
        }
        async fn scan_strip_edges(&self, _opts: &serde_json::Value) -> Result<serde_json::Value> {
            Ok(serde_json::json!({
                "ok": false,
                "reason": "noop_context"
            }))
        }
        async fn click(&self, _x: i32, _y: i32) -> Result<()> {
            Ok(())
        }
        async fn double_click(&self, _x: i32, _y: i32) -> Result<()> {
            Ok(())
        }
        async fn right_click(&self, _x: i32, _y: i32) -> Result<()> {
            Ok(())
        }
        async fn mouse_move(&self, _x: i32, _y: i32) -> Result<()> {
            Ok(())
        }
        async fn mouse_down(&self, _button: &str) -> Result<()> {
            Ok(())
        }
        async fn mouse_up(&self, _button: &str) -> Result<()> {
            Ok(())
        }
        async fn scroll(&self, _delta: i32) -> Result<()> {
            Ok(())
        }
        async fn swipe(
            &self,
            _x1: i32,
            _y1: i32,
            _x2: i32,
            _y2: i32,
            _duration_ms: u32,
        ) -> Result<()> {
            Ok(())
        }
        async fn key_down(&self, _key: &str) -> Result<()> {
            Ok(())
        }
        async fn key_up(&self, _key: &str) -> Result<()> {
            Ok(())
        }
        async fn key_press(&self, _key: &str, _duration_ms: Option<u32>) -> Result<()> {
            Ok(())
        }
        async fn key_combo(&self, _keys: &[String]) -> Result<()> {
            Ok(())
        }
        async fn type_text(&self, _text: &str) -> Result<()> {
            Ok(())
        }
        async fn sleep(&self, _ms: u64) -> Result<()> {
            Ok(())
        }
        async fn wait_for_template(
            &self,
            _name: &str,
            _timeout_ms: u64,
            _opts: Option<FindTemplateOpts>,
        ) -> Result<Option<MatchResult>> {
            Ok(None)
        }
        async fn wait_gone(&self, _name: &str, _timeout_ms: u64) -> Result<bool> {
            Ok(true)
        }
        async fn wait_for_color(
            &self,
            _x: i32,
            _y: i32,
            _color: &str,
            _timeout_ms: u64,
        ) -> Result<bool> {
            Ok(false)
        }
        async fn sleep_frames(&self, _frames: u32) -> Result<()> {
            Ok(())
        }
        async fn wait_for_template_frames(
            &self,
            _name: &str,
            _max_frames: u32,
            _opts: Option<FindTemplateOpts>,
        ) -> Result<Option<MatchResult>> {
            Ok(None)
        }
        async fn wait_gone_frames(&self, _name: &str, _max_frames: u32) -> Result<bool> {
            Ok(true)
        }
        async fn wait_for_color_frames(
            &self,
            _x: i32,
            _y: i32,
            _color: &str,
            _max_frames: u32,
        ) -> Result<bool> {
            Ok(false)
        }
        async fn find_window(&self, _title: &str) -> Result<Option<u64>> {
            Ok(None)
        }
        async fn activate_window(&self, _hwnd: u64) -> Result<()> {
            Ok(())
        }
        async fn get_window_rect(&self, _hwnd: u64) -> Result<Rect> {
            Ok(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            })
        }
        async fn get_screen_size(&self) -> Result<(u32, u32)> {
            Ok((1920, 1080))
        }
        async fn run_script(
            &self,
            _name: &str,
            _params: serde_json::Value,
        ) -> Result<serde_json::Value> {
            Ok(json!({"status":"noop"}))
        }
        async fn call_library(
            &self,
            _library: &str,
            _function: &str,
            _args: serde_json::Value,
        ) -> Result<serde_json::Value> {
            Ok(serde_json::Value::Null)
        }
        fn log(&self, _level: LogLevel, _message: &str) {}
        async fn notify(&self, _title: &str, _body: &str) -> Result<()> {
            Ok(())
        }
        async fn read_store_file(&self, _path: &str) -> Result<String> {
            Ok(String::new())
        }
        async fn write_store_file(&self, _path: &str, _content: &str) -> Result<()> {
            Ok(())
        }
        async fn list_store_files(&self, _dir: &str) -> Result<Vec<String>> {
            Ok(vec![])
        }
        async fn read_file(&self, _path: &str) -> Result<String> {
            Ok(String::new())
        }
        async fn write_file(&self, _path: &str, _content: &str) -> Result<()> {
            Ok(())
        }
        async fn list_files(&self, _dir: &str) -> Result<Vec<String>> {
            Ok(vec![])
        }
        async fn file_exists(&self, _path: &str) -> Result<bool> {
            Ok(false)
        }
        async fn http_get(&self, _url: &str) -> Result<String> {
            Ok(String::new())
        }
        async fn http_post(&self, _url: &str, _body: &str) -> Result<String> {
            Ok(String::new())
        }
        async fn storage_get(&self, _key: &str) -> Result<Option<serde_json::Value>> {
            Ok(None)
        }
        async fn storage_set(&self, _key: &str, _value: serde_json::Value) -> Result<()> {
            Ok(())
        }
        async fn storage_delete(&self, _key: &str) -> Result<()> {
            Ok(())
        }
        async fn storage_keys(&self) -> Result<Vec<String>> {
            Ok(vec![])
        }

        async fn plugin_call(&self, _plugin_id: &str, _method: &str, _args_json: &str) -> Result<String> {
            Err(anyhow::anyhow!("Plugins not supported in noop context"))
        }

        async fn plugin_list(&self) -> Result<String> {
            Ok("[]".to_string())
        }
    }

    #[derive(Clone)]
    enum MockKind {
        Library,
        SoloTaskCaller,
        TriggerCaller,
    }

    struct MockScript {
        name: String,
        kind: MockKind,
        runtime_ref: Arc<Mutex<Option<Weak<ScriptRuntime>>>>,
        observed: Arc<Mutex<HashMap<String, serde_json::Value>>>,
        cancel: CancellationToken,
    }

    impl MockScript {
        fn runtime(&self) -> Result<Arc<ScriptRuntime>> {
            let weak = self
                .runtime_ref
                .lock()
                .expect("lock runtime_ref")
                .as_ref()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("runtime weak ref not set"))?;
            weak.upgrade()
                .ok_or_else(|| anyhow::anyhow!("runtime already dropped"))
        }
    }

    #[async_trait]
    impl Script for MockScript {
        fn name(&self) -> &str {
            &self.name
        }

        fn script_type(&self) -> ScriptType {
            match self.kind {
                MockKind::Library => ScriptType::Library,
                MockKind::SoloTaskCaller => ScriptType::SoloTask,
                MockKind::TriggerCaller => ScriptType::Trigger,
            }
        }

        async fn on_enable(
            &mut self,
            _ctx: &Arc<dyn ScriptContext>,
            _params: &serde_json::Value,
        ) -> Result<()> {
            Ok(())
        }

        async fn start(
            &mut self,
            ctx: &Arc<dyn ScriptContext>,
            _config: &serde_json::Value,
        ) -> Result<()> {
            if matches!(self.kind, MockKind::SoloTaskCaller) {
                let rt = self.runtime()?;
                let result = rt
                    .call_library("common_lib", "sum", json!({"a": 1, "b": 2}), ctx)
                    .await?;
                self.observed
                    .lock()
                    .expect("lock observed")
                    .insert(self.name.clone(), result);
            }
            Ok(())
        }

        async fn stop(&mut self, _ctx: &Arc<dyn ScriptContext>) -> Result<()> {
            Ok(())
        }

        async fn on_capture(
            &mut self,
            ctx: &Arc<dyn ScriptContext>,
            _frame: &CaptureFrame,
        ) -> Result<()> {
            if matches!(self.kind, MockKind::TriggerCaller) {
                let rt = self.runtime()?;
                let result = rt
                    .call_library("common_lib", "sum", json!({"a": 2, "b": 3}), ctx)
                    .await?;
                self.observed
                    .lock()
                    .expect("lock observed")
                    .insert(self.name.clone(), result);
            }
            Ok(())
        }

        async fn on_disable(&mut self, _ctx: &Arc<dyn ScriptContext>) -> Result<()> {
            Ok(())
        }

        fn is_cancelled(&self) -> bool {
            self.cancel.is_cancelled()
        }

        fn cancellation_token(&self) -> Option<CancellationToken> {
            Some(self.cancel.clone())
        }

        async fn call_function(
            &mut self,
            _ctx: &Arc<dyn ScriptContext>,
            function: &str,
            args: &serde_json::Value,
        ) -> Result<serde_json::Value> {
            if !matches!(self.kind, MockKind::Library) {
                return Err(anyhow::anyhow!("non-library script cannot be called"));
            }
            if function != "sum" {
                return Err(crate::error::ScriptError::LibraryFunctionNotFound {
                    library: self.name.clone(),
                    function: function.to_string(),
                }
                .into());
            }
            let a = args.get("a").and_then(|v| v.as_i64()).unwrap_or(0);
            let b = args.get("b").and_then(|v| v.as_i64()).unwrap_or(0);
            Ok(json!(a + b))
        }
    }

    struct MockEngine {
        runtime_ref: Arc<Mutex<Option<Weak<ScriptRuntime>>>>,
        observed: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    }

    #[async_trait]
    impl ScriptEngine for MockEngine {
        fn name(&self) -> &str {
            "mock"
        }

        fn supported_types(&self) -> Vec<ScriptType> {
            vec![
                ScriptType::SoloTask,
                ScriptType::Trigger,
                ScriptType::Library,
            ]
        }

        async fn load(
            &self,
            _script_path: &Path,
            manifest: &Manifest,
            _data_root: &std::path::Path,
        ) -> Result<Box<dyn Script>> {
            let kind = match manifest.script_type {
                ScriptType::Library => MockKind::Library,
                ScriptType::Trigger => MockKind::TriggerCaller,
                _ => MockKind::SoloTaskCaller,
            };
            Ok(Box::new(MockScript {
                name: manifest.name.clone(),
                kind,
                runtime_ref: self.runtime_ref.clone(),
                observed: self.observed.clone(),
                cancel: CancellationToken::new(),
            }))
        }

        async fn unload_all(&self) -> Result<()> {
            Ok(())
        }

        fn engine_version(&self) -> &str {
            "1.0.0"
        }
    }

    fn write_mock_script(base: &Path, name: &str, script_type: &str) -> Result<()> {
        let dir = base.join(name);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::json!({
                "schema_version": 1,
                "name": name,
                "display_name": name,
                "version": "1.0.0",
                "type": script_type,
                "entry": "main.mock"
            })
            .to_string(),
        )?;
        std::fs::write(dir.join("main.mock"), "// mock script")?;
        Ok(())
    }

    #[tokio::test]
    async fn test_solo_task_calls_library_via_runtime() -> Result<()> {
        let root = std::env::temp_dir().join("betternte_runtime_library_solo");
        let _ = std::fs::remove_dir_all(&root);
        let scripts_dir = root.join("scripts");
        std::fs::create_dir_all(&scripts_dir)?;
        write_mock_script(&scripts_dir, "common_lib", "library")?;
        write_mock_script(&scripts_dir, "task_caller", "solo_task")?;

        let runtime_ref: Arc<Mutex<Option<Weak<ScriptRuntime>>>> = Arc::new(Mutex::new(None));
        let observed = Arc::new(Mutex::new(HashMap::new()));
        let mut rt = ScriptRuntime::new("1.0.0", scripts_dir.clone())?;
        rt.register_engine(
            "mock",
            Box::new(MockEngine {
                runtime_ref: runtime_ref.clone(),
                observed: observed.clone(),
            }),
        );
        let runtime = Arc::new(rt);
        *runtime_ref.lock().expect("lock runtime_ref") = Some(Arc::downgrade(&runtime));

        let _ = runtime
            .load_from_dirs(std::slice::from_ref(&scripts_dir))
            .await?;
        let ctx: Arc<dyn ScriptContext> = Arc::new(NoopCtx::new());
        runtime.start_task("task_caller", json!({}), &ctx).await?;

        let val = observed
            .lock()
            .expect("lock observed")
            .get("task_caller")
            .cloned();
        assert_eq!(val, Some(json!(3)));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[tokio::test]
    async fn test_trigger_calls_library_via_runtime() -> Result<()> {
        let root = std::env::temp_dir().join("betternte_runtime_library_trigger");
        let _ = std::fs::remove_dir_all(&root);
        let scripts_dir = root.join("scripts");
        std::fs::create_dir_all(&scripts_dir)?;
        write_mock_script(&scripts_dir, "common_lib", "library")?;
        write_mock_script(&scripts_dir, "trigger_caller", "trigger")?;

        let runtime_ref: Arc<Mutex<Option<Weak<ScriptRuntime>>>> = Arc::new(Mutex::new(None));
        let observed = Arc::new(Mutex::new(HashMap::new()));
        let mut rt = ScriptRuntime::new("1.0.0", scripts_dir.clone())?;
        rt.register_engine(
            "mock",
            Box::new(MockEngine {
                runtime_ref: runtime_ref.clone(),
                observed: observed.clone(),
            }),
        );
        let runtime = Arc::new(rt);
        *runtime_ref.lock().expect("lock runtime_ref") = Some(Arc::downgrade(&runtime));

        let _ = runtime
            .load_from_dirs(std::slice::from_ref(&scripts_dir))
            .await?;
        let ctx: Arc<dyn ScriptContext> = Arc::new(NoopCtx::new());
        runtime
            .start_task("trigger_caller", json!({}), &ctx)
            .await?;

        let val = observed
            .lock()
            .expect("lock observed")
            .get("trigger_caller")
            .cloned();
        assert_eq!(val, Some(json!(5)));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[tokio::test]
    async fn test_call_library_restores_caller_template_dir() -> Result<()> {
        let root = std::env::temp_dir().join("betternte_runtime_library_restore_template_dir");
        let _ = std::fs::remove_dir_all(&root);
        let scripts_dir = root.join("scripts");
        std::fs::create_dir_all(&scripts_dir)?;
        write_mock_script(&scripts_dir, "common_lib", "library")?;

        let runtime_ref: Arc<Mutex<Option<Weak<ScriptRuntime>>>> = Arc::new(Mutex::new(None));
        let observed = Arc::new(Mutex::new(HashMap::new()));
        let mut rt = ScriptRuntime::new("1.0.0", scripts_dir.clone())?;
        rt.register_engine(
            "mock",
            Box::new(MockEngine {
                runtime_ref: runtime_ref.clone(),
                observed,
            }),
        );
        let runtime = Arc::new(rt);
        *runtime_ref.lock().expect("lock runtime_ref") = Some(Arc::downgrade(&runtime));

        let _ = runtime
            .load_from_dirs(std::slice::from_ref(&scripts_dir))
            .await?;
        let ctx_impl = Arc::new(NoopCtx::new());
        let caller_dir = scripts_dir.join("task_caller");
        ctx_impl.set_template_dir(caller_dir.clone());
        let ctx: Arc<dyn ScriptContext> = ctx_impl.clone();

        let _ = runtime
            .call_library("common_lib", "sum", json!({"a": 1, "b": 2}), &ctx)
            .await?;

        assert_eq!(ctx_impl.get_template_dir(), Some(caller_dir));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }
}
