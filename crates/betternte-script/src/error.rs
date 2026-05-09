//! Script error types.

use thiserror::Error;

/// Script module error type.
#[derive(Error, Debug)]
pub enum ScriptError {
    #[error("Script load failed: {0}")]
    LoadFailed(String),

    #[error("Version incompatible: script requires {required}, engine is {current}")]
    VersionIncompatible { required: String, current: String },

    #[error("Script timeout ({0}ms)")]
    Timeout(u64),

    #[error("Library '{0}' cannot be executed directly")]
    LibraryNotRunnable(String),

    #[error("Library '{0}' not found")]
    LibraryNotFound(String),

    #[error("Library '{library}' has no exported function '{function}'")]
    LibraryFunctionNotFound { library: String, function: String },

    #[error("Library '{library}' function '{function}' execution failed: {reason}")]
    LibraryExecutionFailed {
        library: String,
        function: String,
        reason: String,
    },

    #[error("Script dependency error: {0}")]
    Dependency(String),

    #[error("{0}")]
    Other(String),
}

impl From<anyhow::Error> for ScriptError {
    fn from(err: anyhow::Error) -> Self {
        ScriptError::Other(err.to_string())
    }
}

impl From<serde_json::Error> for ScriptError {
    fn from(err: serde_json::Error) -> Self {
        ScriptError::LoadFailed(err.to_string())
    }
}

/// Result alias for script operations.
pub type ScriptResult<T> = Result<T, ScriptError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_anyhow_error() {
        let err: ScriptError = anyhow::anyhow!("something went wrong").into();
        match err {
            ScriptError::Other(msg) => assert_eq!(msg, "something went wrong"),
            _ => panic!("Expected Other variant"),
        }
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err: ScriptError = json_err.into();
        match err {
            ScriptError::LoadFailed(_) => {}
            _ => panic!("Expected LoadFailed variant"),
        }
    }

    #[test]
    fn test_display_version_incompatible() {
        let err = ScriptError::VersionIncompatible {
            required: ">=1.0.0".into(),
            current: "0.9.0".into(),
        };
        let s = err.to_string();
        assert!(s.contains(">=1.0.0"));
        assert!(s.contains("0.9.0"));
    }

    #[test]
    fn test_display_timeout() {
        let err = ScriptError::Timeout(5000);
        assert_eq!(err.to_string(), "Script timeout (5000ms)");
    }

    #[test]
    fn test_display_library_not_runnable() {
        let err = ScriptError::LibraryNotRunnable("common_api".into());
        assert_eq!(
            err.to_string(),
            "Library 'common_api' cannot be executed directly"
        );
    }

    #[test]
    fn test_display_library_not_found() {
        let err = ScriptError::LibraryNotFound("common_api".into());
        assert_eq!(err.to_string(), "Library 'common_api' not found");
    }

    #[test]
    fn test_display_library_function_not_found() {
        let err = ScriptError::LibraryFunctionNotFound {
            library: "common_api".into(),
            function: "clickNpc".into(),
        };
        assert_eq!(
            err.to_string(),
            "Library 'common_api' has no exported function 'clickNpc'"
        );
    }

    #[test]
    fn test_display_library_execution_failed() {
        let err = ScriptError::LibraryExecutionFailed {
            library: "common_api".into(),
            function: "clickNpc".into(),
            reason: "boom".into(),
        };
        assert_eq!(
            err.to_string(),
            "Library 'common_api' function 'clickNpc' execution failed: boom"
        );
    }

    #[test]
    fn test_display_load_failed() {
        let err = ScriptError::LoadFailed("bad syntax".into());
        assert_eq!(err.to_string(), "Script load failed: bad syntax");
    }

    #[test]
    fn test_display_other() {
        let err = ScriptError::Other("generic".into());
        assert_eq!(err.to_string(), "generic");
    }

    #[test]
    fn test_script_result_ok() {
        let r: ScriptResult<i32> = Ok(42);
        assert_eq!(r.unwrap(), 42);
    }

    #[test]
    fn test_script_result_err() {
        let r: ScriptResult<i32> = Err(ScriptError::LoadFailed("x".into()));
        assert!(r.is_err());
    }
}
