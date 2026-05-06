//! Process utilities

/// Check if running with administrator privileges
pub fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        use std::process::Command;
        // Check by attempting to open a system file that requires elevation
        let output = Command::new("net").args(["session"]).output();

        if let Ok(output) = output {
            return output.status.success();
        }

        false
    }
    #[cfg(not(windows))]
    {
        // On non-Windows, check if EUID is 0
        unsafe { libc::geteuid() == 0 }
    }
}

/// Check if debugger is attached
#[cfg(windows)]
pub fn is_debugger_attached() -> bool {
    use std::process::Command;
    let output = Command::new("powershell")
        .args(["-Command", "(Get-Process -Id $PID).DebuggerAttached"])
        .output();

    output
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("True"))
        .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn is_debugger_attached() -> bool {
    std::env::var("RUST_BACKTRACE").is_ok()
}

/// Check if running in debug mode
pub fn is_debug_build() -> bool {
    #[cfg(debug_assertions)]
    {
        true
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

/// Get current process ID
pub fn current_pid() -> u32 {
    std::process::id()
}

/// Get application data directory
pub fn get_app_data_dir() -> Option<std::path::PathBuf> {
    dirs::data_dir()
}

/// Get temporary directory
pub fn get_temp_dir() -> std::path::PathBuf {
    std::env::temp_dir()
}

/// Get home directory
pub fn get_home_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_pid_nonzero() {
        assert!(current_pid() > 0);
    }

    #[test]
    fn test_current_pid_matches_std() {
        assert_eq!(current_pid(), std::process::id());
    }

    #[test]
    fn test_is_debug_build_in_test() {
        // In test builds, debug_assertions is enabled
        assert!(is_debug_build());
    }

    #[test]
    fn test_get_temp_dir_exists() {
        let dir = get_temp_dir();
        assert!(dir.exists());
    }

    #[test]
    fn test_get_home_dir_some() {
        let home = get_home_dir();
        assert!(home.is_some());
    }
}
