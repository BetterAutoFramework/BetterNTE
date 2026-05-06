use thiserror::Error;

#[derive(Error, Debug)]
pub enum OverlayError {
    #[error("Window not created or already destroyed")]
    WindowDestroyed,

    #[error("Game window not found")]
    GameWindowNotFound,

    #[error("Game window is minimized")]
    GameWindowMinimized,

    #[error("Failed to create overlay window")]
    CreateWindowFailed,

    #[error("Failed to commit frame to window")]
    CommitFailed,

    #[error("Out of bounds: x={x}, y={y}, width={width}, height={height}")]
    OutOfBounds {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },

    #[error("Not in frame (call begin_frame first)")]
    NotInFrame,

    #[error("Already in frame (call end_frame first)")]
    AlreadyInFrame,

    #[error("Font load failed: {0}")]
    FontLoadFailed(String),

    #[error("Unsupported character: {0}")]
    UnsupportedCharacter(char),

    #[error("Win32 API error: {0}")]
    Win32Error(String),

    #[error("Overlay not supported on this platform")]
    PlatformNotSupported,
}
