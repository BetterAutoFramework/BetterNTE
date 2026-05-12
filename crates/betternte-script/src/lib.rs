//! betternte-script: Script engine abstraction with multiple runtime support.
//!
//! Provides a trait-based abstraction for script engines, allowing different
//! runtimes (QuickJS, Lua, JSON pipeline) to be plugged in.
//!
//! # QuickJS Async Bridging
//!
//! ScriptContext async methods are bridged to JS Promises via `Async()` wrapper.
//! Scripts can use `await ctx.click(100, 200)` natively.

pub mod engine;
pub mod error;
pub mod loader;
pub mod manifest;
pub mod manifest_permissions;
pub mod plugin;
pub mod quickjs;
pub mod runtime;

// Re-exports
pub use engine::{
    color_tolerance_for_match_point, CancellationToken, CaptureFrame, ColorMatchAllOpts,
    ColorMatchAllResult, ColorMatchAllShiftMax, ColorMatchPoint, ColorMatchPointResult,
    ColorMatchShift, FindTemplateBatchEntry, FindTemplateOpts, FindTemplateOrderBy,
    ImageRecognitionContext, InputControlContext, IpcCallContext, LogLevel, MatchResult,
    NetworkContext, NotifyContext, OcrResult, Rect, Region, RgbaTolerance, Script, ScriptContext,
    ScriptEngine, ScriptType, StorageContext, WindowOpsContext,
};
pub use error::{ScriptError, ScriptResult};
pub use loader::{ScriptInfo, ScriptLoader};
pub use manifest::{EngineVersionReq, Manifest, ScriptDependency};
pub use manifest_permissions::manifest_permission_key_for_ctx_method;
pub use plugin::{PluginInfo, PluginRegistry, PluginStorage};
pub use quickjs::QuickJsEngine;
pub use runtime::{LoadedScript, ScriptRuntime};
