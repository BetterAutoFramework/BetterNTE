//! EngineBuilder — Builder pattern for Engine construction.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use betternte_core::config::SecurityMode;
use betternte_core::vision::TemplateMatcher;
use betternte_core::{EngineConfig, OcrConfig};
use betternte_runtime::{ConditionHandler, InputRunner, StepHandler};

use crate::event::EventBus;
use crate::script_ctx;
use crate::{Engine, EngineState};

fn build_template_matcher(
    realtime_match_cfg: betternte_core::MatchConfig,
) -> Arc<dyn TemplateMatcher> {
    Arc::new(betternte_vision::OpenCvTemplateMatcher::with_config(
        realtime_match_cfg,
    ))
}

/// Builder for constructing an [`Engine`] instance.
///
/// Allows customization of condition handlers, input runners, and step handlers
/// before the engine is fully assembled.
pub struct EngineBuilder {
    config: EngineConfig,
    base_dir: std::path::PathBuf,
    condition_handlers: Vec<Box<dyn ConditionHandler>>,
    input_runner: Option<Arc<dyn InputRunner>>,
    extra_step_handlers: Vec<Box<dyn StepHandler>>,
}

impl EngineBuilder {
    /// Create a new builder with the given config and base directory.
    pub fn new(config: EngineConfig, base_dir: std::path::PathBuf) -> Self {
        Self {
            config,
            base_dir,
            condition_handlers: Vec::new(),
            input_runner: None,
            extra_step_handlers: Vec::new(),
        }
    }

    /// Add custom condition handlers (Strategy pattern).
    pub fn with_condition_handlers(mut self, handlers: Vec<Box<dyn ConditionHandler>>) -> Self {
        self.condition_handlers = handlers;
        self
    }

    /// Set the input runner (ISP pattern).
    pub fn with_input_runner(mut self, runner: Arc<dyn InputRunner>) -> Self {
        self.input_runner = Some(runner);
        self
    }

    /// Add extra step handlers (Command + Registry pattern).
    pub fn with_step_handlers(mut self, handlers: Vec<Box<dyn StepHandler>>) -> Self {
        self.extra_step_handlers = handlers;
        self
    }

    /// Build the Engine instance.
    pub fn build(self) -> Result<Engine> {
        let EngineBuilder {
            config,
            base_dir,
            condition_handlers,
            input_runner,
            extra_step_handlers,
        } = self;

        let custom_condition_handlers: Vec<Arc<dyn ConditionHandler>> =
            condition_handlers.into_iter().map(Arc::from).collect();
        let custom_step_handlers: Vec<Arc<dyn StepHandler>> =
            extra_step_handlers.into_iter().map(Arc::from).collect();
        let custom_input_runner = input_runner.clone();

        let event_bus = EventBus::new(1024);
        info!(base_dir = %base_dir.display(), "Engine created (idle)");

        // Resolve data root with three-directory merge
        let data_root = betternte_core::DataRoot::new(&base_dir);

        // Initialize ScriptRuntime with primary data root as scripts_dir
        let primary_data_root = data_root.primary().clone();

        let mut runtime =
            betternte_script::ScriptRuntime::new(env!("CARGO_PKG_VERSION"), primary_data_root)?;
        runtime.register_engine(
            "js",
            Box::new(betternte_script::QuickJsEngine::new(env!(
                "CARGO_PKG_VERSION"
            ))),
        );

        let engine_storage_dir = data_root
            .primary()
            .join("local")
            .join("scripts")
            .join("_engine");
        let mut ctx = script_ctx::EngineScriptContext::with_manifest_dir(
            serde_json::json!({}),
            engine_storage_dir,
        );
        ctx.set_capture_config(config.capture.clone());
        ctx.set_allow_fallback_capture(true);
        let realtime_match_cfg = betternte_core::MatchConfig {
            // Keep single-scale matching for responsiveness in script runtime.
            multi_scale: vec![1.0],
            ..betternte_core::MatchConfig::default()
        };
        ctx.set_template_matcher(build_template_matcher(realtime_match_cfg));
        ctx.set_color_detector(Arc::new(betternte_vision::ColorDetectorImpl::new()));

        let ocr_model_dir_abs = Engine::resolve_path(&config.advanced.ocr_model_dir, &base_dir);
        let ocr_cfg = OcrConfig {
            model_path: ocr_model_dir_abs.to_string_lossy().to_string(),
            language: "ch".to_string(),
            use_gpu: matches!(
                config.advanced.hardware_acceleration,
                betternte_core::HardwareAcceleration::Cuda
                    | betternte_core::HardwareAcceleration::DirectMl
            ),
            max_side_len: config.advanced.ocr_max_side_len,
            det_threshold: config.advanced.ocr_det_threshold,
            rec_threshold: config.advanced.ocr_rec_threshold,
            batch_size: config.advanced.ocr_batch_size,
            unclip_ratio: config.advanced.ocr_unclip_ratio,
            text_color: config.advanced.ocr_text_color.clone(),
            text_color_tolerance: config.advanced.ocr_text_color_tolerance,
        };
        ctx.set_ocr_config(ocr_cfg);
        let ocr_engine: Arc<tokio::sync::Mutex<dyn betternte_core::OcrEngine>> =
            Arc::new(tokio::sync::Mutex::new(
                betternte_vision::PaddleOcrEngine::new(),
            ));
        ctx.set_ocr_engine(ocr_engine);
        ctx.set_event_bus(event_bus.clone());
        ctx.set_notification_manager(betternte_notify::create_notification_manager(
            &config.notifications,
        ));
        ctx.set_manifest_security_strict(matches!(config.security.mode, SecurityMode::Strict));
        let ctx = Arc::new(ctx);

        // Wire up script_runner so ctx.runScript() works
        let runtime_for_runner = Arc::new(runtime);
        let ctx_for_runner = ctx.clone();
        let rt_clone = runtime_for_runner.clone();
        ctx.set_script_runner(Arc::new(move |name: String, params: serde_json::Value| {
            let rt = rt_clone.clone();
            let c = ctx_for_runner.clone();
            Box::pin(async move {
                rt.start_task(
                    &name,
                    params,
                    &(c as Arc<dyn betternte_script::ScriptContext>),
                )
                .await?;
                Ok(serde_json::json!({"status": "completed", "script": name}))
            })
        }));
        let runtime_for_library = runtime_for_runner.clone();
        let ctx_for_library = ctx.clone();
        let rt_library = runtime_for_library.clone();
        ctx.set_library_runner(Arc::new(
            move |library: String, function: String, args: serde_json::Value| {
                let rt = rt_library.clone();
                let c = ctx_for_library.clone();
                Box::pin(async move {
                    rt.call_library(
                        &library,
                        &function,
                        args,
                        &(c as Arc<dyn betternte_script::ScriptContext>),
                    )
                    .await
                })
            },
        ));

        let mut engine = Engine {
            config,
            event_bus,
            state: EngineState::Idle,
            scripts_store: Vec::new(),
            triggers_store: Vec::new(),
            base_dir,
            data_root,
            runtime: Some(runtime_for_runner),
            script_ctx: Some(ctx),
            capture_stop: None,
            capture_join: None,
            replay_stop: None,
            replay_join: None,
            task_groups: Vec::new(),
            flows_store: Vec::new(),
            flow_progress: Arc::new(RwLock::new(None)),
            flow_executor: Arc::new(RwLock::new(None)),
            custom_condition_handlers: Arc::new(custom_condition_handlers),
            custom_step_handlers: Arc::new(custom_step_handlers),
            custom_input_runner,
            overlay_manager: std::sync::Mutex::new(None),
        };

        engine.ensure_local_subscription();
        let _ = engine.reload_scripts();
        engine.load_flows();
        let _ = engine.load_task_groups();
        Ok(engine)
    }
}
