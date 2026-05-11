# 2026-05-09 Hot-reload watcher & user-root write target

## Summary

Implemented two features for BetterNTE:
1. **Hot-reload for scripts/task-groups/flows** — file system watcher monitors all three data directories and auto-reloads on changes
2. **User-created content targets `~/.betternte/data/`** — write operations for user content now go to the user home directory instead of exe_dir

## Feature 1: Hot-reload watcher

### What changed

- Added `notify = "7"` dependency to `betternte-engine/Cargo.toml`
- Created `crates/betternte-engine/src/watcher.rs` — `DataWatcher` struct that uses `notify` crate to watch data roots recursively for `.json` and `.js` file changes
- Added `mod watcher;` to engine `lib.rs`
- Added `Engine::start_hot_reload()` method that:
  - Creates a `DataWatcher` monitoring all data roots
  - Spawns a tokio background task with 500ms debounce
  - Publishes `EngineEvent::DataChanged` on the EventBus after debounce
- Added `EngineEvent::DataChanged` variant to `betternte-core/src/event.rs`
- In Tauri client `commands/engine.rs`, after `init_engine`:
  - Calls `engine.start_hot_reload()` to start the watcher
  - Spawns a DataChanged event listener that acquires a write lock on the engine and calls `reload_scripts()`, `load_task_groups()`, `load_flows()`

### Design decision

Since `Engine` methods like `reload_scripts()` require `&mut self`, they cannot be called from the spawned watcher task directly (the engine is behind `RwLock`). The watcher publishes a `DataChanged` event, and a separate listener task in the Tauri client acquires the lock and performs the actual reload. This keeps the watcher lightweight and avoids lock contention.

## Feature 2: User-root write target

### What changed

- Added `DataRoot::user_root()` method to `betternte-core/src/data_root.rs` — returns the second root (`~/.betternte/data/`) or falls back to `primary()` if only one root exists
- Changed `Engine::local_dir()` in `lib.rs` to use `user_root()` instead of `primary()` — this affects all write paths: `create_task_group`, `save_flow`, `create_script`, `delete_*`
- Updated `DataRoot::ensure_dirs()` to also create `local/` subdirectories in `user_root()`
- Updated module-level docs in `data_root.rs` to reflect the new write policy

### Why

User-created content (task groups, flows, scripts) should persist across app updates. Writing to exe_dir means packaged app updates could overwrite user data. The `~/.betternte/data/` directory is outside the app installation and persists.

## Files modified

| File | Change |
|------|--------|
| `crates/betternte-engine/Cargo.toml` | Added `notify = "7"` |
| `crates/betternte-engine/src/watcher.rs` | **New** — DataWatcher struct |
| `crates/betternte-engine/src/lib.rs` | Added `mod watcher`, `start_hot_reload()`, changed `local_dir()` to use `user_root()` |
| `crates/betternte-core/src/data_root.rs` | Added `user_root()` method, updated `ensure_dirs()`, updated docs |
| `crates/betternte-core/src/event.rs` | Added `EngineEvent::DataChanged` variant |
| `crates/betternte-client/src-tauri/src/commands/engine.rs` | Start hot-reload watcher + DataChanged listener in `init_engine` |

## Notes

- Cannot fully compile on Linux (Windows-specific deps in `betternte-engine`). `betternte-core` compiles cleanly.
- The `notify` crate uses `inotify` on Linux, `ReadDirectoryChanges` on Windows, `FSEvents`/`kqueue` on macOS.
