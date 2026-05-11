# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- FFI plugin support via libloading — native dynamic libraries (.dll/.so/.dylib) can now serve as plugins.
- C ABI contract: `__plugin_info`, `__plugin_call`, and optional `__plugin_free` exports.
- Test FFI plugin (`data/plugins/test-ffi-plugin/`) with `add` and `greet` methods.

## [0.0.1] - 2026-05-06

### Added
- First public release.
- Tauri v2 desktop application with Rust backend + React/TypeScript frontend.
- Screen capture, template matching, OCR (PaddleOCR), and color detection.
- QuickJS-based script engine with `ctx` API for automation scripting.
- Task group system for organizing and running multiple scripts.
- One-click task flow (OneDragonFlow) for batch execution.
- Fishing assist script (fishing_assist_v2) with auto bait, auto sell, and configurable parameters.
- Cafe income script (cafe_income) for automated revenue collection.
- Coffee making script (make_coffee) for automated coffee game.
- Auto-skip trigger (auto_skip) for skipping story dialogs and auto-teleport.
- Notification support (WeChat, WeCom, Telegram, Feishu, DingTalk, Bark, SMTP).
- Overlay UI system for in-game status display.
- Script store system for persistent data.
- Debug panel with script execution tracing.
