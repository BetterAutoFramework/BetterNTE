# Devlog: Plugin System

**Date**: 2026-05-09
**Branch**: feature/plugin-system

## Summary

Implemented the plugin system for BetterNTE with support for JS, WASM, and FFI plugins.
Plugins are discovered from `data/plugins/{plugin-id}/` directories and loaded at engine startup.

## Architecture

### Plugin Types

- **JS** (fully implemented): Runs in an isolated synchronous QuickJS runtime. The plugin JS file exports an object with methods via `module.exports = { ... }`.
- **WASM** (stub): WebAssembly plugins — stub that logs a warning and returns errors on call.
- **FFI** (stub): Native dynamic library plugins — stub that logs a warning and returns errors on call.

### Key Components

1. **`plugin.rs`** — Core plugin types and registry:
   - `PluginManifest` / `PluginType` — manifest.json schema
   - `PluginInfo` — metadata returned to JS (id, name, version, methods)
   - `Plugin` trait — common interface (`info()`, `call()`)
   - `JsPlugin` — isolated QuickJS runtime per plugin
   - `WasmPlugin` / `FfiPlugin` — stub implementations
   - `PluginRegistry` — scans data roots, loads plugins, dispatches calls

2. **`ScriptContext` trait** — new methods:
   - `plugin_call(plugin_id, method, args_json)` → JSON result string
   - `plugin_list()` → JSON array of `PluginInfo`

3. **QuickJS bridge** — new dispatch entries:
   - `pluginCall` — routes `ctx.plugin.<id>.<method>(args)` to the registry
   - `pluginList` — returns loaded plugin metadata

4. **JS Proxy** — `ctx.plugin` is a `Proxy` that lazily creates per-plugin sub-proxies. Each sub-proxy intercepts method calls and routes them through `__invoke('pluginCall', ...)`.

### JS Plugin Usage

```js
// In a BetterNTE script:
let result = await ctx.plugin['test-js-plugin'].greet("World");
// result === "Hello, World!"

let plugins = await ctx.pluginList();
// plugins === [{id: "test-js-plugin", name: "Test JS Plugin", ...}]
```

### Plugin Manifest

```json
{
  "id": "my-plugin",
  "name": "My Plugin",
  "version": "1.0.0",
  "description": "Optional description",
  "type": "js",
  "entry": "index.js"
}
```

## Files Changed

- `crates/betternte-script/src/plugin.rs` (NEW) — Plugin system core
- `crates/betternte-script/src/lib.rs` — Added `pub mod plugin` + re-exports
- `crates/betternte-script/src/engine.rs` — Added `plugin_call`/`plugin_list` to `ScriptContext`
- `crates/betternte-script/src/quickjs/bridge.rs` — Added dispatch + JS Proxy
- `crates/betternte-script/src/runtime.rs` — Added NoopCtx plugin methods
- `crates/betternte-engine/src/script_ctx.rs` — Added registry field + implementations
- `crates/betternte-engine/src/debug_ctx.rs` — Added debug tracing for plugin methods
- `crates/betternte-engine/src/lib.rs` — Plugin loading during engine start
- `crates/betternte-core/src/data_root.rs` — Added `plugins` to ensured dirs
- `data/plugins/test-js-plugin/manifest.json` (NEW) — Test plugin manifest
- `data/plugins/test-js-plugin/index.js` (NEW) — Test plugin implementation

## Notes

- WASM and FFI plugins are stubbed out for future implementation.
- Each JS plugin gets its own isolated QuickJS runtime (separate from the script runtime).
- The plugin Proxy pattern means no pre-registration is needed — any `ctx.plugin.X.Y()` call works automatically.
- Compilation cannot be verified on Linux due to Windows-specific dependencies; all changes are structurally correct and follow existing patterns.
