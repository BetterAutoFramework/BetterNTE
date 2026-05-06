//! Error types for the vision crate.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum VisionError {
    #[error("OCR error: {0}")]
    OcrError(String),

    #[error("Inference error: {0}")]
    InferenceError(String),

    #[error("Image conversion error: {0}")]
    ImageConversionError(String),

    #[error("Template not found: {0}")]
    TemplateNotFound(String),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("Color out of bounds: x={0}, y={1}")]
    ColorOutOfBounds(i32, i32),

    #[error("Invalid image format: {0}")]
    InvalidImageFormat(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Operation timed out after {0}ms")]
    Timeout(u64),

    #[error("Template error: {0}")]
    TemplateError(String),

    #[error("Image processing error: {0}")]
    ImageProcessingError(String),

    #[error("Array shape error: {0}")]
    ArrayShapeError(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Model load failed: {0}")]
    ModelLoadFailed(String),

    #[error("Inference failed: {0}")]
    InferenceFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_color_out_of_bounds() {
        let err = VisionError::ColorOutOfBounds(10, 20);
        let msg = format!("{}", err);
        assert!(msg.contains("out of bounds"));
        assert!(msg.contains("10"));
        assert!(msg.contains("20"));
    }

    #[test]
    fn test_error_invalid_image_format() {
        let err = VisionError::InvalidImageFormat("expected RGBA, got Gray".into());
        let msg = format!("{}", err);
        assert!(msg.contains("Invalid image format"));
    }

    #[test]
    fn test_error_template_not_found() {
        let err = VisionError::TemplateNotFound("button.png".into());
        let msg = format!("{}", err);
        assert!(msg.contains("Template not found"));
        assert!(msg.contains("button.png"));
    }

    #[test]
    fn test_error_model_not_found() {
        let err = VisionError::ModelNotFound("ppocr_det.onnx".into());
        let msg = format!("{}", err);
        assert!(msg.contains("Model not found"));
        assert!(msg.contains("ppocr_det.onnx"));
    }

    #[test]
    fn test_error_timeout() {
        let err = VisionError::Timeout(3000);
        let msg = format!("{}", err);
        assert!(msg.contains("timed out"));
        assert!(msg.contains("3000"));
    }
}
