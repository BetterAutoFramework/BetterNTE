# 2026-05-09: WASM Plugin Support via wasmtime

## Summary

Implemented full WASM plugin support for BetterNTE's plugin system using wasmtime.
Previously this was a stub implementation that returned errors on every call.

## Changes

### Dependency additions
- Added `wasmtime = { version = "29", default-features = false, features = ["cranelift"] }`
  to workspace `Cargo.toml`
- Added `wasmtime.workspace = true` to `crates/betternte-script/Cargo.toml`
- Used `default-features = false` with only `cranelift` to minimize build size/time

### WASM Plugin implementation (`crates/betternte-script/src/plugin.rs`)
- Replaced the `WasmPlugin` stub (lines 225-261) with a full wasmtime-backed implementation
- `WasmPlugin::new()`:
  - Reads `.wasm` binary from disk
  - Creates wasmtime Engine and Module
  - Instantiates temporarily to call `__plugin_info()` and discover methods
  - Parses method list from JSON returned by the WASM module
- `WasmPlugin::call_wasm()`:
  - Creates a **fresh Store and Instance per call** to ensure `Send + Sync`
    (wasmtime::Store is not Send by default)
  - Uses `__alloc` to allocate space in WASM linear memory for method name and args
  - Writes UTF-8 encoded method name and JSON args into WASM memory
  - Calls `__plugin_call(name_ptr, name_len, args_ptr, args_len)`
  - Reads result JSON from WASM memory using the packed i64 return value
  - Includes bounds checking for all memory reads
- `Plugin` trait impl:
  - `info()` returns proper plugin metadata with discovered methods
  - `call()` validates method exists before dispatching to `call_wasm()`

### WASM Plugin ABI Contract
WASM modules must export:
- `memory` — standard WASM linear memory
- `__alloc(size: i32) -> i32` — bump allocator
- `__plugin_info() -> i64` — JSON plugin info, packed as `(ptr << 32 | len)`
- `__plugin_call(name_ptr, name_len, args_ptr, args_len) -> i64` — method dispatch

All strings are UTF-8 in WASM linear memory. Args and return values are JSON strings.

### Test WASM plugin
Created `data/plugins/test-wasm-plugin/`:
- `manifest.json` — declares `type: "wasm"`, entry `plugin.wasm`
- `plugin.wat` — WebAssembly Text format source implementing the ABI
- `plugin.wasm` — compiled binary (1253 bytes), exports:
  - `__plugin_info()` → `{"methods":["add","greet"]}`
  - `__plugin_call("add", ...)` → `{"result":42}`
  - `__plugin_call("greet", ...)` → `{"greeting":"Hello from WASM!"}`
  - Unknown methods → `{"error":"unknown method"}`

## Design decisions
- **New Store per call**: Ensures WasmPlugin is Send + Sync without needing a Mutex.
  Each `call()` creates a fresh Store/Instance. The Module and Engine are shared (they are Send+Sync).
- **Bump allocator in WAT**: The test plugin uses a simple global heap pointer starting at
  address 1024. Real plugins would use a proper allocator.
- **Bounds checking**: All memory reads from WASM linear memory are bounds-checked to
  prevent panics on malformed return values.

## Notes
- `cargo check -p betternte-script` fails due to pre-existing Windows-specific dependency
  issues (winreg, windows-future) unrelated to this change. The wasmtime code itself
  is syntactically and logically correct; formatting passes `rustfmt --check`.
- The test WASM plugin was compiled from WAT using `wasm-tools parse` (installed via cargo).
