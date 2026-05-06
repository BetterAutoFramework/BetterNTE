//! 任务组 / 工作流管理 — list / run / stop / save / delete

use anyhow::{bail, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info, warn};

use betternte_runtime::{Flow, FlowExecutor, FlowParser, Group, StepKind, VariableStore};

use super::{Engine, FlowProgress, FlowStatus};

fn flow_json_path(flows_dir: &Path, flow_id: &str) -> PathBuf {
    flows_dir.join(format!("{}.json", flow_id))
}

/// Whether a task-groups JSON file refers to the given id or display name.
fn task_group_json_matches_root(value: &serde_json::Value, key: &str) -> bool {
    match value {
        serde_json::Value::Array(items) => {
            items.iter().any(|v| task_group_json_matches_root(v, key))
        }
        serde_json::Value::Object(obj) => {
            obj.get("uuid").and_then(|v| v.as_str()) == Some(key)
                || obj.get("name").and_then(|v| v.as_str()) == Some(key)
        }
        _ => false,
    }
}

fn collect_json_files_recursive(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let read_dir = match std::fs::read_dir(&dir) {
            Ok(read_dir) => read_dir,
            Err(_) => continue,
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                files.push(path);
            }
        }
    }

    files
}

fn input_map_to_params(input: &std::collections::HashMap<String, String>) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for (k, v) in input {
        let parsed = serde_json::from_str::<serde_json::Value>(v)
            .unwrap_or_else(|_| serde_json::Value::String(v.clone()));
        out.insert(k.clone(), parsed);
    }
    serde_json::Value::Object(out)
}

fn params_to_input_map(
    params: Option<&serde_json::Value>,
) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let Some(value) = params else {
        return out;
    };
    if let Some(obj) = value.as_object() {
        for (k, v) in obj {
            if let Some(s) = v.as_str() {
                out.insert(k.clone(), s.to_string());
            } else {
                out.insert(k.clone(), v.to_string());
            }
        }
    }
    out
}

impl Engine {
    /// Remove hotkey entries that no longer match a loaded flow or task group.
    pub(crate) fn prune_orphan_task_group_hotkeys(&mut self) -> bool {
        let mut valid = HashSet::new();
        for f in &self.flows_store {
            valid.insert(f.id.clone());
            valid.insert(f.name.clone());
        }
        for g in &self.task_groups {
            if !g.uuid.is_empty() {
                valid.insert(g.uuid.clone());
            }
            valid.insert(g.name.clone());
        }

        let before = self.config.hotkey_triggers.task_groups.len();
        self.config
            .hotkey_triggers
            .task_groups
            .retain(|_, id| valid.contains(id.trim()));
        before != self.config.hotkey_triggers.task_groups.len()
    }

    /// 列出所有任务组。
    pub fn list_task_groups(&self) -> Vec<serde_json::Value> {
        self.flows_store
            .iter()
            .map(|flow| {
                let orchestration = flow.orchestration.clone().unwrap_or_default();
                let mut val = serde_json::json!({
                    "uuid": flow.id,
                    "name": flow.name,
                    "description": flow.description,
                    "mode": orchestration.mode.unwrap_or_else(|| "sequential".to_string()),
                    "retry_count": orchestration.retry_count.unwrap_or(0),
                    "nodes": flow.steps.iter().map(|(id, s)| {
                        let script = match &s.kind {
                            StepKind::Script { script } => script.clone(),
                            StepKind::Flow { flow } => flow.clone(),
                            StepKind::Group { group } => group.clone(),
                            _ => id.clone(),
                        };
                        serde_json::json!({
                            "script": script,
                            "alias": id,
                            "timeout_ms": s.timeout_ms,
                            "params": input_map_to_params(&s.input),
                        })
                    }).collect::<Vec<_>>(),
                });
                if let Some(ref v) = orchestration.error_handling {
                    val["error_handling"] = serde_json::json!(v);
                }
                if let Some(ref v) = orchestration.retry {
                    val["retry"] = v.clone();
                }
                if let Some(v) = orchestration.notify_on_failure {
                    val["notify_on_failure"] = serde_json::json!(v);
                }
                if let Some(ref v) = orchestration.schedule {
                    val["schedule"] = v.clone();
                }
                if let Some(ref v) = orchestration.repeat_strategy {
                    val["repeat_strategy"] = serde_json::json!(v);
                }
                if let Some(ref s) = orchestration.source {
                    val["source"] = serde_json::json!(s);
                }
                val
            })
            .collect()
    }

    /// 获取当前运行的任务组进度。
    pub async fn get_task_group_progress(&self) -> Option<serde_json::Value> {
        let progress = self.flow_progress.read().await;
        progress
            .as_ref()
            .map(|p| serde_json::to_value(p).unwrap_or_default())
    }

    /// 执行工作流（统一执行入口）。
    pub async fn run_flow(
        &mut self,
        flow_id: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        if let Some(runtime) = self.runtime.as_ref() {
            if let Some(current) = runtime.active_task_name().await {
                bail!("已有任务 '{}' 正在运行，请先停止后再执行", current);
            }
        }
        if self.is_flow_running().await {
            bail!("已有任务组正在运行，请先停止后再执行");
        }

        // Find the flow by id or name
        let mut flow = self
            .flows_store
            .iter()
            .find(|f| f.id == flow_id || f.name == flow_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("工作流 '{}' 未找到", flow_id))?;

        if let Some(node_params) = params.get("node_params").and_then(|v| v.as_object()) {
            for (alias, step) in flow.steps.iter_mut() {
                if let Some(step_params) = node_params.get(alias).and_then(|v| v.as_object()) {
                    let mut merged = step.input.clone();
                    for (k, v) in step_params {
                        if let Some(s) = v.as_str() {
                            merged.insert(k.clone(), s.to_string());
                        } else {
                            merged.insert(k.clone(), v.to_string());
                        }
                    }
                    step.input = merged;
                }
            }
        }

        info!(id = %flow.id, name = %flow.name, steps = flow.steps.len(), "Running flow");

        let total_steps = flow.steps.len();
        let entry_step = flow.entry.clone();

        // Create FlowRunner
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ScriptRuntime 未初始化"))?
            .clone();
        let ctx = self
            .script_ctx
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ScriptContext 未初始化"))?
            .clone();

        // Bind window before running flow.
        self.bind_script_ctx_window(&ctx).await?;

        let flow_input_runner = self.custom_input_runner.clone().or_else(|| {
            Some(
                Arc::new(super::flow_runner::EngineInputRunner::new(ctx.clone()))
                    as Arc<dyn betternte_runtime::InputRunner>,
            )
        });

        let runner = Arc::new(super::flow_runner::EngineFlowRunner::new(
            runtime,
            ctx.clone(),
            self.flows_store.clone(),
            self.task_groups.clone(),
            flow_input_runner.clone(),
            self.custom_condition_handlers.clone(),
            self.custom_step_handlers.clone(),
        ));
        let variables = Arc::new(VariableStore::new(
            flow.id.clone(),
            flow.variables.clone(),
            None,
        ));
        let step_executor = Arc::new(
            betternte_runtime::DefaultStepExecutor::with_custom_handlers(
                variables.clone(),
                Some(runner),
                flow_input_runner,
                self.custom_step_handlers.as_ref().clone(),
            ),
        );
        let condition_handlers = if self.custom_condition_handlers.is_empty() {
            betternte_runtime::condition_handlers::default_condition_handler_arcs()
        } else {
            self.custom_condition_handlers.as_ref().clone()
        };

        let progress_flow_id = flow.id.clone();
        let executor = Arc::new(FlowExecutor::new_with_custom(
            flow,
            variables,
            step_executor,
            condition_handlers,
        ));

        // Store executor for cancellation
        {
            let mut stored = self.flow_executor.write().await;
            *stored = Some(executor.clone());
        }

        // Initialize progress
        {
            let mut progress = self.flow_progress.write().await;
            *progress = Some(FlowProgress {
                flow_id: progress_flow_id,
                current_node: Some(entry_step),
                completed: 0,
                total: total_steps,
                node_status: std::collections::HashMap::new(),
                status: FlowStatus::Running,
            });
        }

        // Execute flow in background.
        let executor_clone = executor.clone();
        let progress_ref = self.flow_progress.clone();
        let executor_ref = self.flow_executor.clone();

        tokio::spawn(async move {
            let result = executor_clone.run().await;
            let mut progress = progress_ref.write().await;
            match result {
                Ok(()) => {
                    info!("Flow completed");
                    if let Some(ref mut p) = *progress {
                        p.status = FlowStatus::Completed;
                        p.current_node = None;
                        p.completed = p.total;
                    }
                }
                Err(e) => {
                    error!(error = %e, "Flow failed");
                    if let Some(ref mut p) = *progress {
                        p.status = FlowStatus::Failed(e.to_string());
                    }
                }
            }
            // Clear executor and running task
            *executor_ref.write().await = None;
        });

        Ok(serde_json::json!({
            "status": "started",
            "flow": flow_id,
        }))
    }

    /// 执行任务组（兼容别名，内部转调 Flow 运行）。
    pub async fn run_task_group(
        &mut self,
        group_name: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let matched_flow_id = self
            .flows_store
            .iter()
            .find(|f| f.id == group_name || f.name == group_name)
            .map(|f| f.id.clone())
            .or_else(|| {
                self.task_groups
                    .iter()
                    .find(|g| g.name == group_name || g.uuid == group_name)
                    .map(|g| {
                        if g.uuid.is_empty() {
                            g.name.clone()
                        } else {
                            g.uuid.clone()
                        }
                    })
            })
            .unwrap_or_else(|| group_name.to_string());

        self.run_flow(&matched_flow_id, params).await
    }

    /// 停止当前运行的工作流。
    pub async fn stop_flow(&self, _flow_id: &str) -> Result<()> {
        // Stop any running script step first, so long-running script nodes can be interrupted.
        if let (Some(ref runtime), Some(ctx)) = (&self.runtime, self.script_context()) {
            let _ = runtime.stop_task(&ctx).await;
        }

        // Cancel the FlowExecutor so its run loop exits
        {
            let executor_guard = self.flow_executor.read().await;
            if let Some(ref executor) = *executor_guard {
                executor.cancel().await;
                info!("FlowExecutor cancelled");
            }
        }

        let mut progress = self.flow_progress.write().await;
        if let Some(ref mut p) = *progress {
            if p.status == FlowStatus::Running {
                p.status = FlowStatus::Cancelled;
                info!("Task group stop requested");
            }
        } else {
            warn!("No active task group to stop");
        }
        Ok(())
    }

    /// 停止当前运行的任务组（兼容别名，内部转调 Flow 停止）。
    pub async fn stop_task_group(&self, group_name: &str) -> Result<()> {
        self.stop_flow(group_name).await
    }

    /// 获取当前运行的工作流进度。
    pub async fn get_flow_progress(&self) -> Option<serde_json::Value> {
        self.get_task_group_progress().await
    }

    /// 从所有已启用订阅源的 task-groups 目录加载任务组。
    ///
    /// Returns `true` if orphan task-group hotkeys were removed from config.
    pub(super) fn load_task_groups(&mut self) -> bool {
        let data_root = self.data_root();
        self.task_groups = Vec::new();
        let parser = FlowParser::new();
        let subscriptions = self.config.scripts.subscriptions.clone();

        if let Some(plugin_root) = self.active_plugin_root() {
            let dir = plugin_root.join("task-groups");
            let source = format!("插件:{}", self.active_plugin_id());
            if dir.exists() {
                for path in collect_json_files_recursive(&dir) {
                    match std::fs::read_to_string(&path) {
                        Ok(content) => match serde_json::from_str::<Group>(&content) {
                            Ok(mut group) => {
                                if group.source.is_none() {
                                    group.source = Some(source.clone());
                                }
                                info!(name = %group.name, source = %source, "Plugin task group loaded");
                                if let Ok(flow) = parser.parse_group(&group) {
                                    self.upsert_flow_in_memory(flow.clone());
                                    self.persist_legacy_flow_if_needed(&flow);
                                }
                                self.task_groups.push(group);
                            }
                            Err(e) => match serde_json::from_str::<Vec<Group>>(&content) {
                                Ok(groups) => {
                                    info!(count = groups.len(), source = %source, "Plugin task groups loaded (array format)");
                                    for mut group in groups {
                                        if group.source.is_none() {
                                            group.source = Some(source.clone());
                                        }
                                        if let Ok(flow) = parser.parse_group(&group) {
                                            self.upsert_flow_in_memory(flow.clone());
                                            self.persist_legacy_flow_if_needed(&flow);
                                        }
                                        self.task_groups.push(group);
                                    }
                                }
                                Err(_) => {
                                    warn!(file = %path.display(), error = %e, "Failed to parse plugin task group file");
                                }
                            },
                        },
                        Err(e) => {
                            warn!(file = %path.display(), error = %e, "Failed to read plugin task group file");
                        }
                    }
                }
            }
        }

        for sub in &subscriptions {
            if !sub.enabled {
                continue;
            }
            let dir = data_root.join(&sub.directory).join("task-groups");
            if !dir.exists() {
                continue;
            }
            for path in collect_json_files_recursive(&dir) {
                match std::fs::read_to_string(&path) {
                    Ok(content) => match serde_json::from_str::<Group>(&content) {
                        Ok(mut group) => {
                            if group.source.is_none() {
                                group.source = Some(sub.name.clone());
                            }
                            info!(name = %group.name, source = %sub.name, "Task group loaded");
                            if let Ok(flow) = parser.parse_group(&group) {
                                self.upsert_flow_in_memory(flow.clone());
                                self.persist_legacy_flow_if_needed(&flow);
                            }
                            self.task_groups.push(group);
                        }
                        Err(e) => {
                            // 尝试解析为数组格式（兼容旧的 task_groups.json）
                            match serde_json::from_str::<Vec<Group>>(&content) {
                                Ok(groups) => {
                                    info!(count = groups.len(), source = %sub.name, "Task groups loaded (array format)");
                                    for mut group in groups {
                                        if group.source.is_none() {
                                            group.source = Some(sub.name.clone());
                                        }
                                        if let Ok(flow) = parser.parse_group(&group) {
                                            self.upsert_flow_in_memory(flow.clone());
                                            self.persist_legacy_flow_if_needed(&flow);
                                        }
                                        self.task_groups.push(group);
                                    }
                                }
                                Err(_) => {
                                    warn!(file = %path.display(), error = %e, "Failed to parse task group file");
                                }
                            }
                        }
                    },
                    Err(e) => {
                        warn!(file = %path.display(), error = %e, "Failed to read task group file");
                    }
                }
            }
        }

        info!(count = self.task_groups.len(), "Task groups loaded");
        let pruned = self.prune_orphan_task_group_hotkeys();
        if pruned {
            info!("Removed orphan task group hotkey triggers");
        }
        pruned
    }

    /// 从所有已启用订阅源的 flows 目录加载工作流定义。
    pub(super) fn load_flows(&mut self) {
        let data_root = self.data_root();
        self.flows_store = Vec::new();
        let parser = FlowParser::new();
        let subscriptions = self.config.scripts.subscriptions.clone();

        if let Some(plugin_root) = self.active_plugin_root() {
            let dir = plugin_root.join("flows");
            let source = format!("插件:{}", self.active_plugin_id());
            if dir.exists() {
                for path in collect_json_files_recursive(&dir) {
                    match std::fs::read_to_string(&path) {
                        Ok(content) => match parser.parse_or_convert_to_flow(&content) {
                            Ok(flow) => {
                                info!(id = %flow.id, name = %flow.name, source = %source, "Plugin flow loaded");
                                self.upsert_flow_in_memory(flow);
                            }
                            Err(e) => {
                                warn!(file = %path.display(), error = %e, "Failed to parse plugin flow file");
                            }
                        },
                        Err(e) => {
                            warn!(file = %path.display(), error = %e, "Failed to read plugin flow file");
                        }
                    }
                }
            }
        }

        for sub in &subscriptions {
            if !sub.enabled {
                continue;
            }
            let dir = data_root.join(&sub.directory).join("flows");
            if !dir.exists() {
                continue;
            }
            for path in collect_json_files_recursive(&dir) {
                match std::fs::read_to_string(&path) {
                    Ok(content) => match parser.parse_or_convert_to_flow(&content) {
                        Ok(flow) => {
                            info!(id = %flow.id, name = %flow.name, source = %sub.name, "Flow loaded");
                            self.upsert_flow_in_memory(flow);
                        }
                        Err(e) => {
                            warn!(file = %path.display(), error = %e, "Failed to parse flow file");
                        }
                    },
                    Err(e) => {
                        warn!(file = %path.display(), error = %e, "Failed to read flow file");
                    }
                }
            }
        }

        info!(count = self.flows_store.len(), "Flows loaded");
    }

    /// 获取已加载的工作流列表。
    pub fn flows(&self) -> &Vec<betternte_runtime::Flow> {
        &self.flows_store
    }

    fn upsert_flow_in_memory(&mut self, flow: Flow) {
        if let Some(existing) = self.flows_store.iter_mut().find(|f| f.id == flow.id) {
            *existing = flow;
        } else {
            self.flows_store.push(flow);
        }
    }

    fn persist_legacy_flow_if_needed(&self, flow: &Flow) {
        let dir = self.default_flows_dir();
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }

        let path = dir.join(format!("{}.json", flow.id));
        if path.exists() {
            return;
        }

        if let Ok(json) = serde_json::to_string_pretty(flow) {
            let _ = std::fs::write(path, json);
        }
    }

    /// 获取默认存储目录（第一个已启用订阅源的 flows/ 目录）。
    pub(super) fn default_flows_dir(&self) -> std::path::PathBuf {
        self.local_dir("flows")
    }

    /// 保存工作流到默认订阅源的 flows 目录。
    pub fn save_flow(&mut self, flow: &betternte_runtime::Flow) -> Result<()> {
        let dir = self.default_flows_dir();

        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", flow.id));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(flow)?;
        std::fs::write(&path, json)?;

        // 更新内存中的 flows_store
        if let Some(existing) = self.flows_store.iter_mut().find(|f| f.id == flow.id) {
            *existing = flow.clone();
        } else {
            self.flows_store.push(flow.clone());
        }

        info!(id = %flow.id, path = %path.display(), "Flow saved");
        Ok(())
    }

    /// Remove task-group definition JSON files from `task-groups/` trees (plugin + subscriptions).
    ///
    /// [`load_task_groups`] reads these files; removing only `flows/*.json` would resurrect the group on reload.
    fn delete_task_group_definition_files(&self, keys: &[&str]) -> Result<()> {
        let keys: Vec<&str> = keys.iter().copied().filter(|k| !k.is_empty()).collect();
        if keys.is_empty() {
            return Ok(());
        }

        let mut roots: Vec<PathBuf> = Vec::new();
        if let Some(plugin_root) = self.active_plugin_root() {
            roots.push(plugin_root.join("task-groups"));
        }
        let data_root = self.data_root();
        for sub in &self.config.scripts.subscriptions {
            roots.push(data_root.join(&sub.directory).join("task-groups"));
        }

        for root in roots {
            if !root.exists() {
                continue;
            }
            for path in collect_json_files_recursive(&root) {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                    continue;
                };
                let matches = keys.iter().any(|k| task_group_json_matches_root(&value, k));
                if matches {
                    std::fs::remove_file(&path)?;
                    info!(path = %path.display(), "Task group definition file deleted");
                }
            }
        }

        Ok(())
    }

    /// 删除工作流文件。
    ///
    /// Returns `true` if orphan task-group hotkeys were removed from config.
    pub fn delete_flow(&mut self, flow_id: &str) -> Result<bool> {
        let data_root = self.data_root();

        // Same directory as [`save_flow`] — must not depend on subscription `enabled`, otherwise delete misses disk.
        let primary = flow_json_path(&self.default_flows_dir(), flow_id);
        if primary.exists() {
            std::fs::remove_file(&primary)?;
            info!(id = %flow_id, path = %primary.display(), "Flow file deleted (default flows dir)");
        }

        if let Some(plugin_root) = self.active_plugin_root() {
            let path = flow_json_path(&plugin_root.join("flows"), flow_id);
            if path.exists() {
                std::fs::remove_file(&path)?;
                info!(id = %flow_id, path = %path.display(), "Flow file deleted (plugin flows)");
            }
        }

        for sub in &self.config.scripts.subscriptions {
            let path = data_root
                .join(&sub.directory)
                .join("flows")
                .join(format!("{}.json", flow_id));
            if path.exists() && path != primary {
                std::fs::remove_file(&path)?;
                info!(
                    id = %flow_id,
                    path = %path.display(),
                    subscription = %sub.directory,
                    "Flow file deleted"
                );
            }
        }

        self.flows_store.retain(|f| f.id != flow_id);
        let pruned = self.prune_orphan_task_group_hotkeys();
        if pruned {
            info!(%flow_id, "Removed orphan task group hotkey triggers after flow delete");
        }
        Ok(pruned)
    }

    /// 保存任务组（兼容别名）：
    /// 接收前端 TaskGroup JSON，转换为 Flow 后写入 flows/。
    pub fn save_task_group(&mut self, group_json: &serde_json::Value) -> Result<()> {
        let name = group_json
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed");
        let uuid = group_json
            .get("uuid")
            .and_then(|v| v.as_str())
            .unwrap_or(name)
            .to_string();

        let steps: Vec<betternte_runtime::GroupStep> = group_json
            .get("nodes")
            .and_then(|v| v.as_array())
            .map(|nodes| {
                nodes
                    .iter()
                    .map(|node| {
                        let script = node
                            .get("script")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let alias = node
                            .get("alias")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&script)
                            .to_string();
                        let timeout_ms = node.get("timeout_ms").and_then(|v| v.as_u64());
                        let input = params_to_input_map(node.get("params"));
                        betternte_runtime::GroupStep {
                            kind: betternte_runtime::StepKind::Script { script },
                            alias,
                            input,
                            timeout_ms,
                            max_retries: 0,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let description = group_json
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mode = group_json
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("sequential")
            .to_string();
        let retry_count = group_json
            .get("retry_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let group = betternte_runtime::Group {
            uuid,
            name: name.to_string(),
            description,
            mode,
            retry_count,
            steps,
            error_handling: group_json
                .get("error_handling")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            retry: group_json.get("retry").cloned(),
            notify_on_failure: group_json
                .get("notify_on_failure")
                .and_then(|v| v.as_bool()),
            schedule: group_json.get("schedule").cloned(),
            repeat_strategy: group_json
                .get("repeat_strategy")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            source: group_json
                .get("source")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        };

        // 兼容层：保留内存中的 group 视图供旧调用链使用
        if let Some(existing) = self
            .task_groups
            .iter_mut()
            .find(|g| g.name == group.name || g.uuid == group.uuid)
        {
            *existing = group.clone();
        } else {
            self.task_groups.push(group.clone());
        }

        let flow = FlowParser::new()
            .parse_group(&group)
            .map_err(|e| anyhow::anyhow!("解析任务组失败: {}", e))?;
        self.save_flow(&flow)
    }

    /// 删除任务组（兼容别名，内部转调 delete_flow）。
    pub fn delete_task_group(&mut self, name: &str) -> Result<bool> {
        let flow_id = self
            .flows_store
            .iter()
            .find(|f| f.id == name || f.name == name)
            .map(|f| f.id.clone())
            .unwrap_or_else(|| name.to_string());

        self.task_groups
            .retain(|g| g.name != name && g.uuid != name);

        self.delete_task_group_definition_files(&[&name, &flow_id])?;
        self.delete_flow(&flow_id)
    }
}
