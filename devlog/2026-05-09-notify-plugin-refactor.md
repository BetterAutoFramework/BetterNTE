# 2026-05-09: Refactor notify system into plugin pattern

## Summary

Extracted the notification trait interface (`Notifier`, `NotificationManager`, `NotifyError`,
`ChannelInfo`) from `betternte-notify` into `betternte-core` so that **any crate** can
implement the plugin contract without depending on HTTP libraries (reqwest).

## Motivation

The notification system was monolithic: the trait, manager, and all concrete implementations
lived in a single crate. This made it impossible for other crates (e.g., JS/WASM/FFI plugins)
to implement `Notifier` without pulling in reqwest and all HTTP dependencies.

## Design

- **`Notifier` trait** → lives in `betternte-core::notify_trait` (the foundation crate)
- **`NotificationManager`** → lives in `betternte-core::notify_trait` (just a `Vec<Box<dyn Notifier>>`)
- **Concrete notifiers** (ServerChan, Telegram, Webhook, Bark) → stay in `betternte-notify`
- **`register_built_in_notifiers()` / `create_notification_manager()`** → convenience functions
  in `betternte-notify` that build a manager from `NotificationConfig`
- Engine uses `betternte_notify::create_notification_manager()` instead of a separate builder module

## Changes

### New file: `crates/betternte-core/src/notify_trait.rs`
- Moved `NotifyError` enum (8 variants)
- Moved `ChannelInfo` struct
- Moved `Notifier` trait (async_trait, 5 methods)
- Moved `NotificationManager` struct (new/register/unregister/send_all/send_to/list_channels/test_channel/set_enabled)
- ~170 lines total

### Updated: `crates/betternte-core/src/lib.rs`
- Added `pub mod notify_trait;`
- Added re-exports: `NotifyError`, `ChannelInfo`, `NotificationManager`, `Notifier`

### Updated: `crates/betternte-notify/src/lib.rs`
- Removed local definitions of `NotifyError`, `ChannelInfo`, `Notifier`, `NotificationManager`
- Now imports and re-exports from `betternte_core::notify_trait`
- Added `register_built_in_notifiers(mgr, cfg)` — registers channels based on config
- Added `create_notification_manager(cfg)` — full manager factory from config
- Removed unused `serde` and `thiserror` dependencies
- Kept all 4 notifier implementations + `WebhookPlatform` enum
- All 23 existing tests pass (including the migration of `notify_builder` tests)

### Updated: `crates/betternte-notify/Cargo.toml`
- Removed `serde` and `thiserror` (no longer needed; types live in betternte-core)

### Deleted: `crates/betternte-engine/src/notify_builder.rs`
- Logic moved to `betternte_notify::create_notification_manager()`

### Updated: `crates/betternte-engine/src/lib.rs`
- Removed `pub mod notify_builder;`
- Updated `set_config()` to use `betternte_notify::create_notification_manager()`

### Updated: `crates/betternte-engine/src/builder.rs`
- Updated import to `betternte_notify::create_notification_manager()`

### Updated: `crates/betternte-engine/src/script_ctx.rs`
- `NotificationManager` type now comes from `betternte_core::NotificationManager`
- All references updated (field type, constructor, setter methods)

## Key design decisions

1. Trait lives in `betternte-core` so any crate can implement it
2. Concrete notifiers stay in `betternte-notify` (needs reqwest for HTTP)
3. Engine no longer has its own notify_builder module
4. Future: user plugins (JS/WASM/FFI) can implement `Notifier` and register themselves
