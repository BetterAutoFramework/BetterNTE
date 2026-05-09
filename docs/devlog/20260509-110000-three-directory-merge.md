---
date: 2026-05-09
type: refactor
scope: betternte-core, betternte-engine, betternte-client
---

# Three-directory merge for data root

## Summary

Replaced configurable `data_root` with a fixed three-directory merge system. Paths are no longer configurable in config file or settings UI.

## Directory resolution

Three data directories, merged with priority (higher overrides lower):

| Priority | Location | Description |
|----------|----------|-------------|
| 3 (highest) | `<exe_dir>/data/` | Executable directory (workspace root in dev mode) |
| 2 (medium) | `~/.betternte/data/` | User home directory |
| 1 (lowest) | `$BETTERNTE_DATA_DIR/data/` | Environment variable |

- Scripts/triggers/flows are scanned from all three roots and merged
- Same relative path in higher-priority root overrides lower-priority
- Write operations (create/delete) always target the highest-priority root

## New module

`crates/betternte-core/src/data_root.rs` — `DataRoot` struct:
- `new(exe_dir)` — resolves all three roots
- `roots()` — all roots in priority order
- `primary()` — highest-priority root (for writes)
- `resolve(relative)` — find first existing path
- `collect_entries(subdir)` — merge entries across all roots with dedup
- `ensure_dirs()` — create primary directory structure

## Files modified (12)

- `crates/betternte-core/src/data_root.rs` (new)
- `crates/betternte-core/src/lib.rs`
- `crates/betternte-core/src/config/mod.rs`
- `crates/betternte-engine/src/lib.rs`
- `crates/betternte-engine/src/builder.rs`
- `crates/betternte-engine/src/scripts.rs`
- `crates/betternte-engine/src/task_groups.rs`
- `crates/betternte-client/src-tauri/src/commands/settings.rs`
- `crates/betternte-client/src/lib/types.ts`
- `crates/betternte-client/src/lib/stores/helpers.ts`
- `crates/betternte-client/src/lib/mock.ts`
- `crates/betternte-client/src/pages/Settings.tsx`
