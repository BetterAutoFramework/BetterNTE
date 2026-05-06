//! ONNX Session management
//!
//! Wraps ort::Session creation and configuration.

use crate::error::VisionError;
use ort::session::Session;
use std::path::Path;

pub struct SessionBuilder {
    use_cuda: bool,
    use_directml: bool,
}

impl SessionBuilder {
    pub fn new() -> Self {
        Self {
            use_cuda: false,
            use_directml: false,
        }
    }

    pub fn with_cuda(mut self, enable: bool) -> Self {
        self.use_cuda = enable;
        self
    }

    pub fn with_directml(mut self, enable: bool) -> Self {
        self.use_directml = enable;
        self
    }

    pub fn build_from_file(&self, model_path: &Path) -> Result<Session, VisionError> {
        if !model_path.exists() {
            return Err(VisionError::ModelNotFound(model_path.display().to_string()));
        }

        let builder = Session::builder()
            .map_err(|e| VisionError::InferenceError(format!("Session: {}", e)))?;

        // Build execution provider list in priority order
        let mut eps: Vec<ort::ep::ExecutionProviderDispatch> = Vec::new();

        if self.use_directml {
            eps.push(ort::execution_providers::DirectML::default().build());
        }
        if self.use_cuda {
            eps.push(ort::execution_providers::CUDA::default().build());
        }

        let mut builder = if !eps.is_empty() {
            builder
                .with_execution_providers(eps)
                .map_err(|e| VisionError::InferenceError(format!("EP config: {}", e)))?
        } else {
            builder
        };

        let session = builder
            .commit_from_file(model_path)
            .map_err(|e| VisionError::InferenceError(format!("Load: {}", e)))?;

        Ok(session)
    }
}

impl Default for SessionBuilder {
    fn default() -> Self {
        Self::new()
    }
}
