//! betternte-input/src/error.rs
//! Input module error types

use thiserror::Error;

/// Input module error.
///
/// Covers all possible errors for input simulation, key mapping,
/// input queue, macro recording, etc.
#[derive(Error, Debug)]
pub enum InputError {
    /// Input simulation failed
    #[error("Input simulation failed: {0}")]
    SimulationFailed(String),

    /// Invalid key name
    #[error("Invalid key: {0}")]
    InvalidKey(String),

    /// Window not found
    #[error("Window not found: {0}")]
    WindowNotFound(String),

    /// ADB input failed
    #[error("ADB input failed: {0}")]
    AdbInputFailed(String),

    /// Input rate limited
    #[error("Rate limited")]
    RateLimited,

    /// Input queue full (sender backed off because the channel buffer is exhausted).
    #[error("Input queue full")]
    QueueFull,

    /// The background worker driving the input queue has terminated and cannot
    /// process further submissions.
    #[error("Input worker terminated")]
    WorkerTerminated,

    /// Recording failed
    #[error("Recording failed: {0}")]
    RecordingFailed(String),

    /// Playback failed
    #[error("Playback failed: {0}")]
    PlaybackFailed(String),

    /// Controller not initialized
    #[error("Controller not initialized")]
    NotInitialized,
}

impl From<enigo::InputError> for InputError {
    fn from(e: enigo::InputError) -> Self {
        InputError::SimulationFailed(e.to_string())
    }
}

impl From<enigo::NewConError> for InputError {
    fn from(e: enigo::NewConError) -> Self {
        InputError::SimulationFailed(e.to_string())
    }
}

impl From<std::io::Error> for InputError {
    fn from(e: std::io::Error) -> Self {
        InputError::SimulationFailed(e.to_string())
    }
}

/// Convenient Result type
pub type Result<T> = std::result::Result<T, InputError>;
