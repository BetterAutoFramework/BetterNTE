---
date: 2026-05-09
type: refactor
scope: betternte-core, betternte-engine, betternte-client
---

# Remove plugin system

## Summary

Removed the "game plugin" system entirely. Plugin management configuration (game settings, capture config, window match) now comes directly from the client's config file instead of a separate plugin manifest.

## What was removed

- `active_plugin` and `plugin_search_paths` from `EngineConfig`
- Plugin loading from `data/plugins/{id}/` directory
- Plugin manifest parsing and config override/sync logic
- `GamePluginInfo`, `PluginManifestLite` types
- `list_game_plugins` Tauri command
- Plugin selector UI in Settings page
- Plugin-namespaced replay paths

## What was kept

- Subscription system (`data/main/`, `data/local/`) — untouched
- All game/capture/window config fields in `EngineConfig` — untouched (they were already there)
- Replay recording/playback — just simplified path (removed plugin_id prefix)

## Files modified (13)

- `crates/betternte-core/src/config/mod.rs`
- `crates/betternte-engine/src/lib.rs`
- `crates/betternte-engine/src/scripts.rs`
- `crates/betternte-engine/src/task_groups.rs`
- `crates/betternte-engine/src/replay_recorder.rs`
- `crates/betternte-engine/src/replay_playback.rs`
- `crates/betternte-client/src-tauri/src/commands/settings.rs`
- `crates/betternte-client/src-tauri/src/commands/replay.rs`
- `crates/betternte-client/src-tauri/src/lib.rs`
- `crates/betternte-client/src/lib/types.ts`
- `crates/betternte-client/src/lib/stores/helpers.ts`
- `crates/betternte-client/src/lib/mock.ts`
- `crates/betternte-client/src/pages/Settings.tsx`

## Net change

-608 lines, +3 lines
