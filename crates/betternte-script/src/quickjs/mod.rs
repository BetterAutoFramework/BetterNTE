//! QuickJS script engine implementation.

pub mod bridge;
pub mod script;
pub mod value;

use anyhow::Result;
use async_trait::async_trait;
use rquickjs::{AsyncContext, AsyncRuntime};
use std::path::Path;
use tracing::info;

use crate::engine::{ScriptEngine, ScriptType};
use crate::manifest::Manifest;
use script::QuickJsScript;

/// QuickJS 脚本引擎。
pub struct QuickJsEngine {
    engine_version: String,
    max_memory_bytes: usize,
    stack_size_bytes: usize,
    max_execution_ms: u64,
}

impl QuickJsEngine {
    pub fn new(engine_version: &str) -> Self {
        Self {
            engine_version: engine_version.to_string(),
            max_memory_bytes: 128 * 1024 * 1024,
            stack_size_bytes: 4 * 1024 * 1024,
            // Default to a long ceiling for long-running automation tasks.
            max_execution_ms: 24 * 60 * 60 * 1000,
        }
    }

    pub fn with_max_memory(mut self, bytes: usize) -> Self {
        self.max_memory_bytes = bytes;
        self
    }

    pub fn with_stack_size(mut self, bytes: usize) -> Self {
        self.stack_size_bytes = bytes;
        self
    }

    pub fn with_max_execution_ms(mut self, ms: u64) -> Self {
        self.max_execution_ms = ms;
        self
    }

    pub(crate) async fn create_runtime(&self) -> Result<AsyncRuntime> {
        let rt = AsyncRuntime::new()?;
        rt.set_max_stack_size(self.stack_size_bytes).await;
        rt.set_memory_limit(self.max_memory_bytes).await;
        Ok(rt)
    }

    pub(crate) async fn create_context(&self, rt: &AsyncRuntime) -> Result<AsyncContext> {
        let ctx = AsyncContext::full(rt).await?;
        Ok(ctx)
    }
}

#[async_trait]
impl ScriptEngine for QuickJsEngine {
    fn name(&self) -> &str {
        "quickjs"
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
        script_path: &Path,
        manifest: &Manifest,
        data_root: &Path,
    ) -> Result<Box<dyn crate::engine::Script>> {
        info!(
            "Loading script '{}' from {}",
            manifest.name,
            script_path.display()
        );

        let source = std::fs::read_to_string(script_path)?;
        let rt = self.create_runtime().await?;
        let ctx = self.create_context(&rt).await?;

        let script = QuickJsScript::new(
            manifest.clone(),
            source,
            rt,
            ctx,
            self.max_execution_ms,
            data_root.to_path_buf(),
            self.engine_version.clone(),
        )?;

        info!("Script '{}' loaded successfully", manifest.name);
        Ok(Box::new(script))
    }

    async fn unload_all(&self) -> Result<()> {
        Ok(())
    }

    fn engine_version(&self) -> &str {
        &self.engine_version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quickjs_engine_new_defaults() {
        let engine = QuickJsEngine::new("1.0.0");
        assert_eq!(engine.name(), "quickjs");
        assert_eq!(engine.engine_version(), "1.0.0");
        assert_eq!(engine.max_memory_bytes, 128 * 1024 * 1024);
        assert_eq!(engine.stack_size_bytes, 4 * 1024 * 1024);
        assert_eq!(engine.max_execution_ms, 24 * 60 * 60 * 1000);
    }

    #[test]
    fn test_quickjs_engine_with_max_memory() {
        let engine = QuickJsEngine::new("1.0.0").with_max_memory(64 * 1024 * 1024);
        assert_eq!(engine.max_memory_bytes, 64 * 1024 * 1024);
    }

    #[test]
    fn test_quickjs_engine_with_stack_size() {
        let engine = QuickJsEngine::new("1.0.0").with_stack_size(8 * 1024 * 1024);
        assert_eq!(engine.stack_size_bytes, 8 * 1024 * 1024);
    }

    #[test]
    fn test_quickjs_engine_with_max_execution_ms() {
        let engine = QuickJsEngine::new("1.0.0").with_max_execution_ms(5000);
        assert_eq!(engine.max_execution_ms, 5000);
    }

    #[test]
    fn test_quickjs_engine_supported_types() {
        let engine = QuickJsEngine::new("1.0.0");
        let types = engine.supported_types();
        assert_eq!(types.len(), 3);
        assert!(types.contains(&ScriptType::SoloTask));
        assert!(types.contains(&ScriptType::Trigger));
        assert!(types.contains(&ScriptType::Library));
    }

    #[test]
    fn test_quickjs_engine_builder_chain() {
        let engine = QuickJsEngine::new("2.0.0")
            .with_max_memory(256 * 1024 * 1024)
            .with_stack_size(16 * 1024 * 1024)
            .with_max_execution_ms(60000);
        assert_eq!(engine.max_memory_bytes, 256 * 1024 * 1024);
        assert_eq!(engine.stack_size_bytes, 16 * 1024 * 1024);
        assert_eq!(engine.max_execution_ms, 60000);
        assert_eq!(engine.engine_version(), "2.0.0");
    }
}
