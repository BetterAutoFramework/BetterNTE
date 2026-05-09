//! Three-directory merge system for the data directory.
//!
//! Priority (high to low):
//!   1. `<exe_dir>/data/` — executable location (or workspace root in dev mode)
//!   2. `~/.betternte/data/` — user home directory
//!   3. `$BETTERNTE_DATA_DIR/data/` — environment variable
//!
//! Higher-priority roots override lower-priority ones when scanning for scripts,
//! task groups, and flows. User-created content is written to `user_root()` (~/.betternte/data/)
//! so it persists across app updates.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Resolved data root with three-layer merge support.
#[derive(Debug, Clone)]
pub struct DataRoot {
    /// All roots in priority order (highest first).
    roots: Vec<PathBuf>,
}

impl DataRoot {
    /// Create a new DataRoot, resolving all three directories.
    ///
    /// `exe_dir` is the directory containing the executable (or workspace root in dev mode).
    pub fn new(exe_dir: &Path) -> Self {
        let mut roots = Vec::new();

        // Priority 3 (highest): exe_dir/data
        roots.push(exe_dir.join("data"));

        // Priority 2 (medium): ~/.betternte/data
        if let Some(home) = dirs::home_dir() {
            roots.push(home.join(".betternte").join("data"));
        }

        // Priority 1 (lowest): $BETTERNTE_DATA_DIR/data
        if let Ok(env_dir) = std::env::var("BETTERNTE_DATA_DIR") {
            roots.push(PathBuf::from(env_dir).join("data"));
        }

        Self { roots }
    }

    /// Get all roots in priority order (highest first).
    /// Used for scanning directories where we want to merge results.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Get the highest-priority root (for write operations).
    /// New files should always be written to the highest-priority root.
    pub fn primary(&self) -> &PathBuf {
        &self.roots[0]
    }

    /// Get the user home data root (`~/.betternte/data/`).
    ///
    /// Used for user-created content that should persist across app updates.
    /// Falls back to `primary()` if only one root exists (no home dir resolved).
    pub fn user_root(&self) -> &PathBuf {
        // user_root is always the second root (priority 2)
        // If only one root exists (no home dir), fall back to primary
        self.roots.get(1).unwrap_or(&self.roots[0])
    }

    /// Resolve a relative path against the merged data root.
    /// Returns the first existing path from highest to lowest priority.
    /// If none exist, returns the path in the highest-priority root.
    pub fn resolve(&self, relative: &Path) -> PathBuf {
        for root in &self.roots {
            let candidate = root.join(relative);
            if candidate.exists() {
                return candidate;
            }
        }
        // Default to highest priority root
        self.roots[0].join(relative)
    }

    /// Collect files/dirs from all roots, with deduplication by relative path.
    /// Higher-priority entries override lower-priority ones.
    /// Returns `(absolute_path, relative_path)` pairs.
    pub fn collect_entries(&self, subdir: &str) -> Vec<(PathBuf, PathBuf)> {
        let mut seen: HashMap<PathBuf, PathBuf> = HashMap::new();

        // Iterate in reverse priority order so highest priority overwrites
        for root in self.roots.iter().rev() {
            let dir = root.join(subdir);
            if dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let relative = PathBuf::from(subdir).join(entry.file_name());
                        seen.insert(relative, path);
                    }
                }
            }
        }

        // Return sorted by relative path for deterministic order
        let mut result: Vec<_> = seen.into_iter().collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    /// Ensure the primary data root and user_root subdirectories exist.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        let primary = self.primary();
        for subdir in &[
            "main",
            "local",
            "local/scripts",
            "local/triggers",
            "local/task-groups",
            "local/flows",
            "plugins",
        ] {
            std::fs::create_dir_all(primary.join(subdir))?;
        }

        // Also ensure user_root has local/ directories for user-created content
        let user = self.user_root();
        if user != primary {
            for subdir in &[
                "local",
                "local/scripts",
                "local/triggers",
                "local/task-groups",
                "local/flows",
            ] {
                std::fs::create_dir_all(user.join(subdir))?;
            }
        }

        Ok(())
    }
}
