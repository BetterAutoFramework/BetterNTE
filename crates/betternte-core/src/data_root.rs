//! Two-directory merge system for the data directory.
//!
//! Priority (high to low):
//!   1. `<base_dir>/data/` — workspace root in dev mode; `AppData\Local` when installed
//!   2. `<install_dir>/data/` — read-only fallback (install directory, e.g. `C:\Program Files\BetterNTE`)
//!
//! Higher-priority roots override lower-priority ones when scanning for scripts,
//! task groups, and flows. User-created content is written to the primary root.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Resolved data root with three-layer merge support.
#[derive(Debug, Clone)]
pub struct DataRoot {
    /// All roots in priority order (highest first).
    roots: Vec<PathBuf>,
}

impl DataRoot {
    /// Create a new DataRoot with the given base directory as the primary root.
    ///
    /// `base_dir` is the workspace root in dev mode, or `app_local_data_dir` when installed.
    /// Additional roots (e.g. install directory) can be added via [`add_root`].
    pub fn new(base_dir: &Path) -> Self {
        let mut roots = Vec::new();

        // Primary (highest priority): base_dir/data
        roots.push(base_dir.join("data"));

        // Optional lowest-priority root from environment variable
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

    /// Add an additional data root at the lowest priority.
    /// Used to include the install directory (e.g. `C:\Program Files\BetterNTE\data`)
    /// which is separate from the runtime base dir (`AppData\Local\BetterNTE`).
    pub fn add_root(&mut self, root: PathBuf) {
        if !self.roots.contains(&root) {
            self.roots.push(root);
        }
    }

    /// Get the highest-priority root (for write operations).
    /// New files should always be written to the highest-priority root.
    pub fn primary(&self) -> &PathBuf {
        &self.roots[0]
    }

    /// Get the user-writable data root (same as `primary()`).
    ///
    /// User-created content is always written to the primary root.
    pub fn user_root(&self) -> &PathBuf {
        self.primary()
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

    /// Ensure the primary data root subdirectories exist.
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
        Ok(())
    }
}
