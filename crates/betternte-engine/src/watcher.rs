//! File system watcher for hot-reloading scripts, task-groups, and flows.
//!
//! Monitors all data root directories for changes to `.json` and `.js` files,
//! emitting change notifications with debouncing to avoid rapid reloads.

use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::PathBuf;
use tokio::sync::mpsc;

/// File system watcher that monitors data roots for relevant file changes.
pub struct DataWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl DataWatcher {
    /// Create a new watcher that monitors all data roots for changes.
    ///
    /// Returns a `DataWatcher` (which keeps the watcher alive) and a `Receiver`
    /// that yields `()` whenever a relevant file (.json or .js) is created,
    /// modified, or removed.
    pub fn new(data_roots: &[PathBuf]) -> anyhow::Result<(Self, mpsc::Receiver<()>)> {
        let (tx, rx) = mpsc::channel(16);

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                        // Only notify for .json or .js file changes
                        let is_relevant = event.paths.iter().any(|p| {
                            p.extension()
                                .map_or(false, |ext| ext == "json" || ext == "js")
                        });
                        if is_relevant {
                            let _ = tx.try_send(());
                        }
                    }
                    _ => {}
                }
            }
        })?;

        // Watch all data roots recursively
        for root in data_roots {
            if root.exists() {
                tracing::debug!(path = %root.display(), "Watching data root for changes");
                watcher.watch(root, RecursiveMode::Recursive)?;
            }
        }

        Ok((Self { _watcher: watcher }, rx))
    }
}
