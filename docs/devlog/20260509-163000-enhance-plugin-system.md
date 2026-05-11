---
date: 2026-05-09
type: feature
scope: betternte-core, betternte-script, betternte-engine, betternte-client
---

# Enhance plugin system: enable/disable, config, ctx injection, events

## Summary

Major plugin system redesign transforming plugins from passive computation units into first-class engine extensions.

## Changes

### 1. Plugin Enable/Disable + Config Storage

- Added `PluginState { enabled: bool, config: Value }` to EngineConfig
- `plugins: HashMap<String, PluginState>` persisted in engine config file
- PluginRegistry only loads enabled plugins, tracks all discovered plugins
- New plugins default to disabled
- `enable_plugin()` / `disable_plugin()` methods for runtime toggling

### 2. Plugin Config Schema

- Added `config_schema: Option<Value>` to PluginManifest
- Plugins declare configurable parameters in manifest.json
- Config values stored in EngineConfig.plugins[id].config
- Frontend renders config form dynamically based on schema

### 3. Ctx Injection (JS Plugins)

JS plugins now get a `ctx` object with:
- `ctx.log(msg)`, `ctx.logInfo/Warn/Error(msg)` — logging
- `ctx.getConfig()` — read plugin config
- `ctx.storage.get(key)`, `ctx.storage.set(key, value)`, `ctx.storage.delete(key)` — persistent state
- `ctx.call(method, ...args)` — self-referencing method calls

### 4. Event/Hook System

- Added `hooks: HashMap<String, String>` to PluginManifest
- Plugins declare event→method mappings (e.g. "on_step_end" → "handleStep")
- `PluginRegistry::dispatch_event()` fires hooks on all enabled plugins
- Supported events: on_step_start, on_step_end, on_flow_start, on_flow_end

### 5. Plugin Storage

- Per-plugin file-based persistence at `data/plugins/{id}/storage.json`
- Simple key-value JSON storage
- Auto-saves on write

### 6. Frontend UI

- New "插件" tab in Settings page
- Plugin list with name, version, type, methods
- Enable/disable toggle per plugin
- Dynamic config form when plugin has config_schema

### 7. Tauri Commands

- `list_plugins` — returns all discovered plugins with metadata
- `set_plugin_enabled` — toggle plugin enabled state at runtime

## Files modified (11)

- `crates/betternte-core/src/config/mod.rs`
- `crates/betternte-script/src/plugin.rs`
- `crates/betternte-script/src/engine.rs`
- `crates/betternte-script/src/lib.rs`
- `crates/betternte-engine/src/script_ctx.rs`
- `crates/betternte-engine/src/lib.rs`
- `crates/betternte-engine/src/debug_ctx.rs`
- `crates/betternte-client/src/lib/types.ts`
- `crates/betternte-client/src/lib/stores/helpers.ts`
- `crates/betternte-client/src-tauri/src/commands/settings.rs`
- `crates/betternte-client/src/pages/Settings.tsx`
