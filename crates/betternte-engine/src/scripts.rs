//! 脚本管理 — reload / list / run / create / delete

use anyhow::{bail, Result};
use betternte_script::{ScriptError, ScriptType};
use std::collections::HashSet;
use tracing::{info, warn};

use super::Engine;

impl Engine {
    /// Remove hotkey entries whose script name no longer exists under scripts/ or triggers/.
    pub(crate) fn prune_orphan_script_hotkeys(&mut self) -> bool {
        let mut valid: HashSet<String> = self
            .scripts_store
            .iter()
            .map(|e| e.manifest.name.clone())
            .collect();
        valid.extend(self.triggers_store.iter().map(|e| e.manifest.name.clone()));

        let before = self.config.hotkey_triggers.scripts.len();
        self.config
            .hotkey_triggers
            .scripts
            .retain(|_, name| valid.contains(name.trim()));
        before != self.config.hotkey_triggers.scripts.len()
    }

    /// Reload all scripts and triggers from all enabled subscriptions across all data roots.
    ///
    /// Returns `true` if orphan script hotkeys were removed from config.
    pub fn reload_scripts(&mut self) -> bool {
        // If subscriptions is empty, use defaults
        if self.config.scripts.subscriptions.is_empty() {
            warn!("subscriptions is empty, falling back to default (官方源)");
            self.config.scripts.subscriptions =
                betternte_core::config::ScriptConfig::default().subscriptions;
        }

        info!(
            subscriptions = self.config.scripts.subscriptions.len(),
            data_roots = self.data_root.roots().len(),
            "reload_scripts: starting scan"
        );

        self.scripts_store = Vec::new();
        self.triggers_store = Vec::new();

        for sub in &self.config.scripts.subscriptions {
            if !sub.enabled {
                info!(name = %sub.name, dir = %sub.directory, "Skipping disabled subscription");
                continue;
            }
            let source = &sub.name;

            // Scan scripts/ and triggers/ across all data roots
            for suffix in &["scripts", "triggers"] {
                let sub_path = format!("{}/{}", sub.directory, suffix);
                let mut seen_dirs = std::collections::HashSet::new();
                for root in self.data_root.roots().iter().rev() {
                    let dir = root.join(&sub_path);
                    if !dir.is_dir() {
                        continue;
                    }
                    // Canonicalize to deduplicate equivalent paths on Windows
                    let canonical = std::fs::canonicalize(&dir).unwrap_or(dir.clone());
                    if !seen_dirs.insert(canonical) {
                        continue;
                    }
                    info!(
                        path = %dir.display(),
                        "Scanning {} directory", suffix
                    );
                    let store = if *suffix == "scripts" {
                        &mut self.scripts_store
                    } else {
                        &mut self.triggers_store
                    };
                    store.extend(super::loader::load_scripts(
                        &dir,
                        source,
                        root,
                    ));
                }
            }
        }

        // Deduplicate scripts/triggers by ID (higher-priority source wins)
        for store in [&mut self.scripts_store, &mut self.triggers_store] {
            let mut seen_ids = std::collections::HashSet::new();
            store.retain(|e| seen_ids.insert(e.manifest.name.clone()));
        }

        // 发布 ScriptLoaded 事件
        for entry in self.scripts_store.iter().chain(self.triggers_store.iter()) {
            if entry.loaded {
                let _ = self
                    .event_bus
                    .publish(betternte_core::EngineEvent::ScriptLoaded {
                        script_name: entry.manifest.name.clone(),
                        version: entry.manifest.version.clone(),
                        path: entry.path.to_string_lossy().to_string(),
                    });
            }
        }

        info!(
            scripts = self.scripts_store.len(),
            triggers = self.triggers_store.len(),
            "Scripts and triggers reloaded"
        );

        let pruned = self.prune_orphan_script_hotkeys();
        if pruned {
            info!("Removed orphan script hotkey triggers");
        }
        pruned
    }

    /// 获取已加载的脚本列表。
    pub fn scripts(&self) -> &Vec<super::loader::ScriptEntry> {
        &self.scripts_store
    }

    /// 获取已加载的触发器列表。
    pub fn triggers(&self) -> &Vec<super::loader::ScriptEntry> {
        &self.triggers_store
    }

    /// 执行脚本。
    ///
    /// 如果脚本未在运行时中找到，会尝试重新加载脚本目录（支持调试场景下的热重载）。
    pub async fn run_script(
        &self,
        name: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ScriptRuntime not initialized"))?;
        if let Some(current) = runtime.active_task_name().await {
            bail!("已有任务 '{}' 正在运行，请先停止后再执行", current);
        }
        let ctx = self
            .script_context()
            .ok_or_else(|| anyhow::anyhow!("ScriptContext not initialized"))?;

        let resolved = runtime.resolve_script_run_key(name).await?;

        if let Some(manifest) = runtime.get_manifest(&resolved).await {
            if manifest.script_type == ScriptType::Library {
                return Err(ScriptError::LibraryNotRunnable(name.to_string()).into());
            }
        }
        // Bind target window and input controller only when context is not yet bound.
        if let Some(ref concrete_ctx) = self.script_ctx {
            if !concrete_ctx.has_hwnd() {
                self.bind_script_ctx_window(concrete_ctx).await?;
            }
        }

        // Emit TaskStarted
        let start_time = chrono::Utc::now();
        let _ = self
            .event_bus
            .publish(betternte_core::EngineEvent::TaskStarted {
                task_name: resolved.clone(),
                task_type: "solo_task".to_string(),
                timestamp: start_time,
            });

        // Reload script from disk so user edits are always picked up
        if let Err(e) = runtime
            .reload_single_script(&resolved, Some(&ctx))
            .await
        {
            info!(name = %resolved, error = %e, "Script reload failed, will try loading from dirs");
            let script_dirs = self.all_script_dirs();
            let _ = runtime.load_from_dirs(&script_dirs).await;
        }

        // Start task — if not found, reload scripts from all subscriptions and retry
        let result = async {
            let is_not_found = |err: &anyhow::Error| {
                err.downcast_ref::<betternte_script::ScriptError>()
                    .and_then(|se| match se {
                        betternte_script::ScriptError::LoadFailed(msg) => Some(msg),
                        _ => None,
                    })
                    .map(|msg| msg.contains("not found"))
                    .unwrap_or(false)
            };
            if let Err(e) = runtime
                .start_task(&resolved, params.clone(), &ctx)
                .await
            {
                if is_not_found(&e) {
                    info!(name = %resolved, "Script not in runtime, reloading scripts...");
                    let script_dirs = self.all_script_dirs();
                    let _ = runtime.load_from_dirs(&script_dirs).await;
                    runtime.start_task(&resolved, params, &ctx).await?;
                } else {
                    return Err(e);
                }
            }
            Ok(())
        }
        .await;

        // Emit TaskStopped
        let duration_ms = (chrono::Utc::now() - start_time).num_milliseconds().max(0) as u64;
        let reason = if result.is_ok() {
            betternte_core::event::TaskStopReason::Completed
        } else {
            betternte_core::event::TaskStopReason::Error(result.as_ref().unwrap_err().to_string())
        };
        let _ = self
            .event_bus
            .publish(betternte_core::EngineEvent::TaskStopped {
                task_name: resolved.clone(),
                reason,
                duration_ms,
                timestamp: chrono::Utc::now(),
            });

        result?;
        Ok(serde_json::json!({"status": "completed", "script": name}))
    }

    /// 停止当前任务。
    pub async fn stop_task(&self) -> Result<()> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ScriptRuntime not initialized"))?;
        let ctx = self
            .script_context()
            .ok_or_else(|| anyhow::anyhow!("ScriptContext not initialized"))?;

        runtime.stop_task(&ctx).await?;
        Ok(())
    }

    /// 启用触发器（当用户在 UI 中开启触发器时调用）。
    ///
    /// 如果脚本未在运行时中找到，会尝试重新加载脚本目录（支持调试场景下的热重载）。
    pub async fn enable_trigger(&self, name: &str, params: serde_json::Value) -> Result<()> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ScriptRuntime not initialized"))?;
        let ctx = self
            .script_context()
            .ok_or_else(|| anyhow::anyhow!("ScriptContext not initialized"))?;

        // Bind target window and input controller before trigger enable.
        if let Some(ref concrete_ctx) = self.script_ctx {
            self.bind_script_ctx_window(concrete_ctx).await?;
        }

        // Reload trigger from disk so user edits are always picked up
        if let Err(e) = runtime.reload_single_script(name, Some(&ctx)).await {
            info!(name, error = %e, "Trigger reload failed, will try loading from dirs");
            let script_dirs = self.all_script_dirs();
            let _ = runtime.load_from_dirs(&script_dirs).await;
        }

        // Try to enable — if not found, reload scripts from all subscriptions and retry
        if let Err(e) = runtime.enable_script(name, &ctx, &params).await {
            let is_not_found = e
                .downcast_ref::<betternte_script::ScriptError>()
                .and_then(|se| match se {
                    betternte_script::ScriptError::LoadFailed(msg) => Some(msg),
                    _ => None,
                })
                .map(|msg| msg.contains("not found"))
                .unwrap_or(false);
            if is_not_found {
                info!(name, "Trigger not in runtime, reloading scripts...");
                let script_dirs = self.all_script_dirs();
                let _ = runtime.load_from_dirs(&script_dirs).await;
                runtime.enable_script(name, &ctx, &params).await?;
            } else {
                return Err(e);
            }
        }

        info!(name, "Trigger enabled");
        Ok(())
    }

    /// 禁用触发器（当用户在 UI 中关闭触发器时调用）。
    pub async fn disable_trigger(&self, name: &str) -> Result<()> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ScriptRuntime not initialized"))?;
        let ctx = self
            .script_context()
            .ok_or_else(|| anyhow::anyhow!("ScriptContext not initialized"))?;

        runtime.disable_script(name, &ctx).await?;

        info!(name, "Trigger disabled");
        Ok(())
    }

    /// 从配置中同步所有触发器的启用/禁用状态。
    pub(super) async fn sync_trigger_states(&self) {
        let runtime = match self.runtime.as_ref() {
            Some(r) => r,
            None => return,
        };
        let ctx = match self.script_context() {
            Some(c) => c,
            None => return,
        };

        for (name, state) in &self.config.triggers {
            if state.enabled {
                if let Err(e) = runtime.enable_script(name, &ctx, &state.params).await {
                    warn!(name, error = %e, "Failed to auto-enable trigger");
                }
            }
        }
    }

    /// 获取默认脚本/触发器存储目录（第一个已启用订阅源的 scripts/ 或 triggers/ 目录）。
    pub(super) fn default_script_dir(&self, script_type: &str) -> std::path::PathBuf {
        let sub_dir = match script_type {
            "trigger" => "triggers",
            _ => "scripts",
        };
        self.local_dir(sub_dir)
    }

    /// 创建新脚本/触发器，生成 manifest.json 和 main.js。
    pub async fn create_script(
        &mut self,
        name: &str,
        display_name: &str,
        script_type: &str,
        description: &str,
    ) -> Result<()> {
        let dir = self.default_script_dir(script_type);
        let script_dir = dir.join(name);
        std::fs::create_dir_all(&script_dir)?;

        // ScriptRuntime expects "solo_task" not "task"
        let manifest_type = match script_type {
            "trigger" => "trigger",
            _ => "solo_task",
        };
        let manifest = serde_json::json!({
            "schema_version": 1,
            "name": name,
            "display_name": display_name,
            "version": "1.0.0",
            "type": manifest_type,
            "entry": "main.js",
            "author": "",
            "description": description,
        });
        std::fs::write(
            script_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;

        let main_js = match script_type {
            "trigger" => format!(
                r#"// {display_name} 触发器

async function onEnable(ctx) {{
  ctx.log("info", "{display_name} 已启用");
}}

async function onCapture(ctx) {{
  // TODO: Implement frame detection logic
}}

async function onDisable(ctx) {{
  ctx.log("info", "{display_name} 已禁用");
}}
"#
            ),
            _ => format!(
                r#"// {display_name}
async function main(ctx) {{
  ctx.log("info", "{display_name} 开始执行");

  // TODO: Implement automation logic

  return {{ success: true }};
}}
"#
            ),
        };
        std::fs::write(script_dir.join("main.js"), main_js)?;

        info!(name = %name, r#type = %script_type, path = %script_dir.display(), "Script created");
        let _ = self.reload_scripts();
        self.reload_runtime_scripts().await;
        Ok(())
    }

    /// Delete a script/trigger directory and all its files.
    ///
    /// Returns `true` if orphan script hotkeys were removed from config.
    pub async fn delete_script(&mut self, name: &str) -> Result<bool> {
        for sub in &self.config.scripts.subscriptions {
            if !sub.enabled {
                continue;
            }
            for sub_dir in &["scripts", "triggers"] {
                let sub_path = format!("{}/{}", sub.directory, sub_dir);
                let entries = self.data_root.collect_entries(&sub_path);
                for (_relative, absolute) in entries {
                    let path = absolute.join(name);
                    if path.exists() {
                        std::fs::remove_dir_all(&path)?;
                        info!(name = %name, path = %path.display(), "Script directory deleted");
                        break;
                    }
                }
            }
        }
        let pruned = self.reload_scripts();
        self.reload_runtime_scripts().await;
        Ok(pruned)
    }

    /// 重新加载所有脚本到 ScriptRuntime（热重载）。
    pub(super) async fn reload_runtime_scripts(&self) {
        if let Some(ref runtime) = self.runtime {
            let script_dirs = self.all_script_dirs();
            match runtime.load_from_dirs(&script_dirs).await {
                Ok(infos) => {
                    info!(count = infos.len(), "Runtime scripts reloaded");
                }
                Err(e) => {
                    warn!(error = %e, "Failed to reload runtime scripts");
                }
            }
        }
    }

    /// List all files in a script directory (not subdirectories).
    pub fn list_script_files(&self, script_dir: &str) -> Result<Vec<String>> {
        let full_path = self.data_root.resolve(std::path::Path::new(script_dir));
        // Security: validate the resolved path is within one of the data roots
        let canonical = full_path
            .canonicalize()
            .map_err(|e| anyhow::anyhow!("Directory not found: {}", e))?;
        let is_within_root = self
            .data_root
            .roots()
            .iter()
            .filter_map(|r| r.canonicalize().ok())
            .any(|cr| canonical.starts_with(&cr));
        if !is_within_root {
            bail!("Path traversal detected");
        }
        let entries = std::fs::read_dir(&canonical)?;
        let files: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        Ok(files)
    }
}
