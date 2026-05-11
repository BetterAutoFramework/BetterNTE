//! betternte-engine: BetterNTE 游戏自动化引擎库 crate。
//!
//! 提供 `Engine` struct 作为统一门面，客户端（Tauri/CLI）通过它使用全部功能。
//!
//! # 生命周期
//!
//! ```rust,no_run
//! use betternte_engine::Engine;
//! use betternte_core::EngineConfig;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let config = EngineConfig::default();
//! let base_dir = std::path::PathBuf::from(".");
//! let mut engine = Engine::new(config, base_dir)?;
//! engine.start().await?;
//! engine.stop().await?;
//! # Ok(())
//! # }
//! ```

pub mod builder;
pub mod capture;
pub mod debug_ctx;
pub mod event;
pub mod flow_runner;
pub mod loader;

mod replay_playback;
mod replay_recorder;
pub mod replay_verify;
pub mod script_ctx;
pub mod scripts;
pub mod task_groups;
mod watcher;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use betternte_core::EngineConfig;
use betternte_runtime::{FlowExecutor, Group};

pub use betternte_notify::create_notification_manager;
pub use betternte_runtime::Flow;
pub use builder::EngineBuilder;
pub use event::EventBus;

/// 引擎运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineState {
    /// 已创建，未启动（仅持有配置）。
    Idle,
    /// 运行中（截图/OCR/输入/脚本全部就绪）。
    Running,
}

/// BetterNTE 引擎门面。
///
/// 客户端的唯一入口。生命周期：`new()` → `start()` → `stop()`。
///
/// - `new()` 轻量创建，仅存储配置和事件总线
/// - `start()` 完整初始化所有组件
/// - `stop()` 释放运行时资源，引擎可再次 `start()`
pub struct Engine {
    pub(crate) config: EngineConfig,
    pub(crate) event_bus: EventBus,
    pub(crate) state: EngineState,
    pub(crate) scripts_store: Vec<loader::ScriptEntry>,
    pub(crate) triggers_store: Vec<loader::ScriptEntry>,
    pub(crate) base_dir: std::path::PathBuf,
    pub(crate) data_root: betternte_core::DataRoot,
    pub(crate) runtime: Option<Arc<betternte_script::ScriptRuntime>>,
    pub(crate) script_ctx: Option<Arc<script_ctx::EngineScriptContext>>,
    pub(crate) capture_stop: Option<tokio::sync::oneshot::Sender<()>>,
    pub(crate) capture_join: Option<tokio::task::JoinHandle<()>>,
    pub(crate) replay_stop: Option<tokio::sync::watch::Sender<bool>>,
    pub(crate) replay_join: Option<tokio::task::JoinHandle<std::result::Result<(), anyhow::Error>>>,
    // Task group / Flow integration
    pub(crate) task_groups: Vec<Group>,
    pub(crate) flows_store: Vec<betternte_runtime::Flow>,
    pub(crate) flow_progress: Arc<RwLock<Option<FlowProgress>>>,
    pub(crate) flow_executor: Arc<RwLock<Option<Arc<FlowExecutor>>>>,
    pub(crate) custom_condition_handlers: Arc<Vec<Arc<dyn betternte_runtime::ConditionHandler>>>,
    pub(crate) custom_step_handlers: Arc<Vec<Arc<dyn betternte_runtime::StepHandler>>>,
    pub(crate) custom_input_runner: Option<Arc<dyn betternte_runtime::InputRunner>>,
    pub(crate) overlay_manager: std::sync::Mutex<Option<betternte_overlay::OverlayManager>>,
}

/// 任务组/工作流执行状态。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowStatus {
    Running,
    Completed,
    Cancelled,
    #[serde(serialize_with = "serialize_flow_status_failed")]
    Failed(String),
}

fn serialize_flow_status_failed<S: serde::Serializer>(msg: &str, s: S) -> Result<S::Ok, S::Error> {
    use serde::Serialize;
    serde_json::json!({"failed": msg}).serialize(s)
}

/// 运行中的任务组进度快照。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FlowProgress {
    /// Stable flow id (task group UUID or legacy name); used to match global hotkeys.
    #[serde(default)]
    pub flow_id: String,
    pub current_node: Option<String>,
    pub completed: usize,
    pub total: usize,
    pub node_status: std::collections::HashMap<String, serde_json::Value>,
    pub status: FlowStatus,
}

const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

impl Engine {
    /// 创建引擎实例（轻量）。
    ///
    /// `base_dir` 用于解析配置中的相对路径（如 scripts.directory）。
    /// 通常是 Tauri 应用的 CWD 或项目根目录。
    ///
    /// For advanced customization, use [`builder::EngineBuilder`].
    pub fn new(config: EngineConfig, base_dir: std::path::PathBuf) -> Result<Self> {
        builder::EngineBuilder::new(config, base_dir).build()
    }

    /// 启动引擎，完整初始化所有组件。
    pub async fn start(&mut self) -> Result<()> {
        if self.state == EngineState::Running {
            debug!("Engine already running, ignoring start()");
            return Ok(());
        }

        info!("Engine starting...");

        // Load scripts into ScriptRuntime from all enabled subscriptions
        if let Some(ref runtime) = self.runtime {
            let script_dirs = self.all_script_dirs();
            match runtime.load_from_dirs(&script_dirs).await {
                Ok(infos) => {
                    info!(count = infos.len(), "Scripts loaded into runtime");
                }
                Err(e) => {
                    tracing::warn!("Failed to load scripts into runtime: {}", e);
                }
            }
        }

        // Load plugins from data roots
        if let Some(ref ctx) = self.script_ctx {
            let data_roots = self.data_root.roots().to_vec();
            ctx.load_plugins(&data_roots, &self.config.plugins).await;
        }

        // Auto-enable triggers from config
        self.sync_trigger_states().await;

        // Engine must always bind a target window before entering running state.
        let concrete_ctx = self
            .script_ctx
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ScriptContext not initialized"))?
            .clone();
        let hwnd = self.bind_script_ctx_window(&concrete_ctx).await?;

        let replay_artifact_frames = match self.config.replay.mode {
            betternte_core::ReplayMode::Replay => {
                concrete_ctx.set_allow_fallback_capture(false);
                Some(replay_playback::discover_replay_frames(
                    &self.base_dir,
                    &self.config.replay,
                )?)
            }
            betternte_core::ReplayMode::Normal | betternte_core::ReplayMode::Record => {
                concrete_ctx.set_allow_fallback_capture(true);
                None
            }
        };

        // Start capture loop for trigger ticking
        let fps_cap = self.config.capture.fps_cap.max(1);
        let hwnd = Some(hwnd);
        let runtime = self.runtime.clone();
        let ctx = self.script_ctx.clone();
        let capture_config = self.config.capture.clone();

        self.replay_stop = None;
        self.replay_join = None;

        let replay_session = match replay_recorder::try_start_replay_recording(
            self.event_bus.clone(),
            &self.base_dir,
            &self.config,
            ENGINE_VERSION,
        ) {
            Ok(Some(rec)) => {
                self.replay_stop = Some(rec.stop_tx);
                self.replay_join = Some(rec.join);
                rec.session
            }
            Ok(None) => None,
            Err(e) => {
                tracing::warn!("Replay record session not started: {:#}", e);
                None
            }
        };

        if let Some(ctx) = self.script_ctx.as_ref() {
            ctx.set_replay_timeline_sink(replay_session.clone()).await;
        }

        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        self.capture_stop = Some(stop_tx);
        let join = tokio::spawn(async move {
            Self::capture_loop(
                fps_cap,
                hwnd,
                runtime,
                ctx,
                capture_config,
                stop_rx,
                replay_session,
                replay_artifact_frames,
            )
            .await;
        });
        self.capture_join = Some(join);

        self.state = EngineState::Running;
        info!("Engine started (fps_cap={})", fps_cap);
        Ok(())
    }

    /// 停止引擎，释放运行时资源。
    pub async fn stop(&mut self) -> Result<()> {
        if self.state == EngineState::Idle {
            debug!("Engine already idle, ignoring stop()");
            return Ok(());
        }

        // Stop capture loop
        if let Some(stop_tx) = self.capture_stop.take() {
            let _ = stop_tx.send(());
        }
        if let Some(join) = self.capture_join.take() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(3), join).await;
        }

        // Stop any running task
        if let (Some(ref runtime), Some(ctx)) = (&self.runtime, self.script_context()) {
            let _ = runtime.stop_task(&ctx).await;
            let _ = runtime.shutdown(&ctx).await;
        }

        if let Some(tx) = self.replay_stop.take() {
            let _ = tx.send(true);
        }
        if let Some(join) = self.replay_join.take() {
            match join.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::warn!("replay recorder exited with error: {:#}", e),
                Err(e) => tracing::warn!("replay recorder join error: {}", e),
            }
        }

        if let Some(ctx) = self.script_ctx.as_ref() {
            ctx.set_replay_timeline_sink(None).await;
        }

        self.state = EngineState::Idle;
        if let Ok(mut guard) = self.overlay_manager.lock() {
            *guard = None;
        }
        info!("Engine stopped");
        Ok(())
    }

    /// 当前引擎状态。
    pub fn state(&self) -> EngineState {
        self.state
    }

    /// 引擎是否正在运行。
    pub fn is_running(&self) -> bool {
        self.state == EngineState::Running
    }

    /// 获取当前正在执行的脚本或任务组名称。
    pub fn running_task_name(&self) -> Option<String> {
        if let Some(name) = self.active_solo_script_name() {
            return Some(name);
        }
        if let Ok(progress) = self.flow_progress.try_read() {
            if let Some(p) = progress.as_ref() {
                if p.status == FlowStatus::Running {
                    return p.current_node.clone().or_else(|| Some("flow".to_string()));
                }
            }
        }
        None
    }

    /// Active solo script task name, if any (not used when only a flow orchestrator is idle between steps).
    pub fn active_solo_script_name(&self) -> Option<String> {
        self.runtime
            .as_ref()
            .and_then(|r| r.active_task_name_sync())
    }

    /// Resolve manifest `name` or [`ScriptInfo::script_id`] to the canonical runtime key (`script_id`).
    pub async fn resolve_script_run_key(&self, key: &str) -> Result<String, anyhow::Error> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ScriptRuntime not initialized"))?;
        runtime.resolve_script_run_key(key).await
    }

    /// When a task-group flow is running, its stable id (same as [`Flow::id`] / group UUID).
    pub fn running_flow_id(&self) -> Option<String> {
        let Ok(guard) = self.flow_progress.try_read() else {
            return None;
        };
        let Some(p) = guard.as_ref() else {
            return None;
        };
        if p.status != FlowStatus::Running || p.flow_id.is_empty() {
            return None;
        }
        Some(p.flow_id.clone())
    }

    /// 当前是否有 flow 正在运行。
    pub async fn is_flow_running(&self) -> bool {
        let progress = self.flow_progress.read().await;
        progress
            .as_ref()
            .map(|p| p.status == FlowStatus::Running)
            .unwrap_or(false)
    }

    /// 全局停止：停止当前正在执行的脚本和任务组。
    pub async fn stop_all(&self) -> Result<()> {
        // Stop running script
        if let Some(ref runtime) = self.runtime {
            if let Ok(ctx) = self
                .script_context()
                .ok_or_else(|| anyhow::anyhow!("no ctx"))
            {
                let _ = runtime.stop_task(&ctx).await;
            }
        }

        // Stop running task group
        {
            let mut progress = self.flow_progress.write().await;
            if let Some(ref mut p) = *progress {
                if p.status == FlowStatus::Running {
                    p.status = FlowStatus::Cancelled;
                }
            }
        }

        // Cancel the FlowExecutor
        {
            let executor_guard = self.flow_executor.read().await;
            if let Some(ref executor) = *executor_guard {
                executor.cancel().await;
            }
        }

        info!("All tasks stopped");
        Ok(())
    }

    /// 获取事件总线引用（客户端可以订阅事件）。
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    /// 获取配置引用。
    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// 获取可变配置引用。
    pub fn config_mut(&mut self) -> &mut EngineConfig {
        &mut self.config
    }

    /// Expose concrete script context for host-side debug tooling.
    pub fn script_ctx_handle(&self) -> Option<Arc<script_ctx::EngineScriptContext>> {
        self.script_ctx.clone()
    }

    /// Get the data root with three-directory merge support.
    pub fn data_root(&self) -> &betternte_core::DataRoot {
        &self.data_root
    }

    /// Root used to resolve relative paths in config (repo root in dev, per-user data when packaged).
    pub fn config_base_dir(&self) -> &std::path::Path {
        self.base_dir.as_path()
    }

    /// Get the primary scripts directory (highest-priority data root).
    pub fn scripts_dir(&self) -> std::path::PathBuf {
        self.data_root.primary().clone()
    }

    /// 按引擎工作区解析配置里的路径片段（绝对路径保持不变，否则相对 `workspace`）。
    ///
    /// 用于客户端把 `replay.artifact_root` 等写成相对仓库根的路径。
    pub fn resolved_config_path(&self, dir_str: &str) -> std::path::PathBuf {
        Self::resolve_path(dir_str.trim(), &self.base_dir)
    }

    /// 引擎版本号。
    pub fn version(&self) -> &str {
        ENGINE_VERSION
    }

    /// 解析配置中的相对/绝对路径。
    pub(crate) fn resolve_path(dir_str: &str, base_dir: &std::path::Path) -> std::path::PathBuf {
        if std::path::Path::new(dir_str).is_absolute() {
            std::path::PathBuf::from(dir_str)
        } else {
            base_dir.join(dir_str)
        }
    }

    /// Ensure "local" subscription exists and create directory structure.
    fn ensure_local_subscription(&mut self) {
        let has_local = self
            .config
            .scripts
            .subscriptions
            .iter()
            .any(|s| s.directory == "local");
        if !has_local {
            info!("Adding default 'local' subscription (本地源)");
            self.config
                .scripts
                .subscriptions
                .push(betternte_core::config::Subscription {
                    name: "本地源".into(),
                    directory: "local".into(),
                    enabled: true,
                    auto_update: false,
                    url: None,
                });
        }

        // Create directory structure in the primary data root
        if let Err(e) = self.data_root.ensure_dirs() {
            tracing::warn!(error = %e, "Failed to ensure local data directories");
        }
    }

    /// Start a background task that watches all data roots for file changes.
    ///
    /// When a `.json` or `.js` file is created, modified, or removed, the engine
    /// reloads scripts, task-groups, and flows after a 500ms debounce window.
    /// Returns a `JoinHandle` that runs for the lifetime of the engine.
    pub fn start_hot_reload(&self) -> tokio::task::JoinHandle<()> {
        let data_roots = self.data_root.roots().to_vec();
        // Since Engine is behind RwLock in the Tauri client, we cannot call &self
        // reload methods from the spawned watcher task. Instead, we publish a
        // DataChanged event on the EventBus; the client-side listener handles reload.
        let event_bus = self.event_bus.clone();

        tokio::spawn(async move {
            let (_watcher, mut rx) = match watcher::DataWatcher::new(&data_roots) {
                Ok(pair) => pair,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to start data watcher");
                    return;
                }
            };

            tracing::info!("Hot-reload watcher started for {} data roots", data_roots.len());

            loop {
                // Wait for any file change signal
                if rx.recv().await.is_none() {
                    tracing::info!("Data watcher channel closed, stopping hot-reload");
                    break;
                }

                // Debounce: drain rapid-fire events within 500ms
                loop {
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(500),
                        rx.recv(),
                    )
                    .await
                    {
                        Ok(Some(())) => {
                            // More changes arrived during debounce window, keep draining
                        }
                        Ok(None) => {
                            // Channel closed
                            return;
                        }
                        Err(_) => {
                            // 500ms elapsed with no more changes — time to reload
                            break;
                        }
                    }
                }

                tracing::info!("Data directory change detected, triggering reload");
                let _ = event_bus.publish(betternte_core::EngineEvent::DataChanged);
            }
        })
    }

    /// Get the local source directory (for writing user-created content).
    ///
    /// Writes target the primary data root (`<base_dir>/data/local/`).
    pub(crate) fn local_dir(&self, sub_dir: &str) -> std::path::PathBuf {
        self.data_root.primary().join("local").join(sub_dir)
    }

    /// Get all enabled subscription scripts/ and triggers/ directories across all data roots.
    pub(crate) fn all_script_dirs(&self) -> Vec<std::path::PathBuf> {
        let mut dirs = Vec::new();
        for sub in &self.config.scripts.subscriptions {
            if !sub.enabled {
                continue;
            }
            for suffix in &["scripts", "triggers"] {
                let sub_path = format!("{}/{}", sub.directory, suffix);
                let mut seen = std::collections::HashSet::new();
                for root in self.data_root.roots().iter().rev() {
                    let dir = root.join(&sub_path);
                    if !dir.is_dir() {
                        continue;
                    }
                    let canonical = std::fs::canonicalize(&dir).unwrap_or(dir.clone());
                    if seen.insert(canonical) {
                        dirs.push(dir);
                    }
                }
            }
        }
        dirs
    }

    /// 更新配置。
    ///
    /// - `scripts.data_root/subscriptions`：热重载脚本与 Flow 索引。
    /// - `notifications`：重建通知管理器并热替换。
    /// - 截图/输入/replay/debug/game 窗口相关字段变更：若引擎正在运行，自动执行 `stop -> start` 以生效。
    pub async fn set_config(&mut self, config: EngineConfig) -> Result<()> {
        let old = self.config.clone();
        let was_running = self.is_running();
        let subs_changed = old.scripts.subscriptions != config.scripts.subscriptions;
        let notify_changed = !config_notifications_equal(&old.notifications, &config.notifications);
        let runtime_restart_required = config_runtime_restart_required(&old, &config);

        if was_running && runtime_restart_required {
            info!("Config change requires runtime restart; stopping engine");
            self.stop().await?;
        }

        info!("Engine config updated");
        self.config = config;
        if let Some(ctx) = self.script_ctx.as_ref() {
            ctx.set_manifest_security_strict(matches!(
                self.config.security.mode,
                betternte_core::config::SecurityMode::Strict
            ));
        }
        self.ensure_local_subscription();
        if subs_changed {
            let _ = self.reload_scripts();
            self.load_flows();
            let _ = self.load_task_groups();
        }
        if notify_changed {
            if let Some(ctx) = self.script_ctx.as_ref() {
                let mgr = betternte_notify::create_notification_manager(&self.config.notifications);
                ctx.replace_notification_manager(mgr).await;
                info!("notification manager rebuilt after config change");
            }
        }

        if was_running && runtime_restart_required {
            info!("Restarting engine after config update");
            self.start().await?;
        }

        Ok(())
    }
}

fn config_notifications_equal(
    a: &betternte_core::config::NotificationConfig,
    b: &betternte_core::config::NotificationConfig,
) -> bool {
    // Cheap structural equality via serde_json, avoids adding PartialEq upstream.
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

fn config_runtime_restart_required(old: &EngineConfig, new: &EngineConfig) -> bool {
    !config_capture_equal(&old.capture, &new.capture)
        || !config_advanced_equal(&old.advanced, &new.advanced)
        || !config_replay_equal(&old.replay, &new.replay)
        || !config_game_equal(&old.game, &new.game)
        || !config_overlay_equal(&old.overlay, &new.overlay)
}

fn config_capture_equal(
    a: &betternte_core::config::CaptureConfig,
    b: &betternte_core::config::CaptureConfig,
) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

fn config_advanced_equal(
    a: &betternte_core::config::AdvancedConfig,
    b: &betternte_core::config::AdvancedConfig,
) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

fn config_replay_equal(a: &betternte_core::ReplayConfig, b: &betternte_core::ReplayConfig) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

fn config_game_equal(
    a: &betternte_core::config::GameConfig,
    b: &betternte_core::config::GameConfig,
) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

fn config_overlay_equal(
    a: &betternte_core::config::OverlayConfig,
    b: &betternte_core::config::OverlayConfig,
) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_version() {
        assert!(!ENGINE_VERSION.is_empty());
    }
}
