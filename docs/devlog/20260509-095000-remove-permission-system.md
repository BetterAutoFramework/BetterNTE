---
date: 2026-05-09
type: refactor
scope: betternte-runtime, betternte-script, betternte-engine, betternte-core
---

# Remove entire permission system

## Summary

Removed the complete permission/sandbox system across the workspace. The permission system was an over-engineered layer for a local desktop automation tool — scripts run on the user's own machine with full access anyway.

## Changes

- **Deleted modules**: `sandbox.rs` (PermissionGuard, precheck_flow_permissions), `manifest_permissions.rs` (ctx method → permission key mapping)
- **Removed types**: `Permission` enum, `Permissions` struct, `PermissionGuard`, `PermissionKey`, `ManifestPermScope`, `FlexiblePermissions`, `SecurityMode`, `SecurityConfig`
- **Removed trait methods**: 4 methods from `ScriptContext` (`manifest_security_strict`, `push/pop_manifest_permission_scope`, `check_manifest_api_permission`)
- **Removed error variants**: `PermissionDenied` from both `FlowError` and `ScriptError`
- **Removed config**: `SecurityMode`/`SecurityConfig` from `EngineConfig`
- **Cleaned up**: all re-exports, imports, and permission check calls in script execution paths

## Files modified (21 total)

- `crates/betternte-runtime/src/sandbox.rs` — emptied
- `crates/betternte-runtime/src/types.rs` — removed Permission/Permissions types
- `crates/betternte-runtime/src/lib.rs` — removed re-exports
- `crates/betternte-runtime/src/error.rs` — removed PermissionDenied variant
- `crates/betternte-script/src/manifest_permissions.rs` — emptied
- `crates/betternte-script/src/manifest.rs` — removed permissions field
- `crates/betternte-script/src/lib.rs` — removed re-exports
- `crates/betternte-script/src/engine.rs` — removed 4 trait methods
- `crates/betternte-script/src/error.rs` — removed PermissionDenied variant + test
- `crates/betternte-script/src/quickjs/script.rs` — removed permission scope push/pop
- `crates/betternte-script/src/quickjs/bridge.rs` — removed permission check
- `crates/betternte-script/src/loader.rs` — updated comment
- `crates/betternte-engine/src/script_ctx.rs` — removed all permission fields, methods, checks
- `crates/betternte-engine/src/debug_ctx.rs` — removed delegation methods
- `crates/betternte-engine/src/loader.rs` — removed FlexiblePermissions
- `crates/betternte-engine/src/builder.rs` — removed SecurityMode usage
- `crates/betternte-engine/src/lib.rs` — removed SecurityMode usage
- `crates/betternte-core/src/config/mod.rs` — removed SecurityConfig
- `crates/betternte-core/src/lib.rs` — removed re-exports
- `docs/development.md` — updated
- `docs/development_EN.md` — updated

## Verification

`cargo check` passes for all non-Windows-specific crates.
