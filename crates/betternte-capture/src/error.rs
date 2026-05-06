//! Error types for betternte-capture.

use thiserror::Error;

/// Capture error types.
#[derive(Error, Debug)]
pub enum CaptureError {
    #[error("Window not found: {0}")]
    WindowNotFound(String),

    #[error("Capture initialization failed: {0}")]
    InitFailed(String),

    #[error("Capture failed: {0}")]
    CaptureFailed(String),

    #[error("Unsupported capture target: {0}")]
    UnsupportedTarget(String),

    #[error("Unsupported capture method: {0}")]
    UnsupportedMethod(String),

    #[error("Frame buffer overflow")]
    FrameBufferOverflow,

    #[error("Window minimized")]
    WindowMinimized,

    #[error("Window closed")]
    WindowClosed,

    #[error("Timeout after {0}ms")]
    Timeout(u64),

    #[error("Image conversion failed: {0}")]
    ImageConversionFailed(String),

    #[error("Region out of bounds: {0}")]
    RegionOutOfBounds(String),

    #[error("Windows Graphics Capture not supported on this system")]
    WgcNotSupported,

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for capture operations.
pub type Result<T> = std::result::Result<T, CaptureError>;
