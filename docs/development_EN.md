# Development Guide

> **[English](development_EN.md)** | [简体中文](development.md)

This document is for developers who want to contribute to BetterNTE or build on top of its engine.

---

## Table of Contents

- [Prerequisites](#prerequisites)
- [Project Structure](#project-structure)
- [Quick Start](#quick-start)
- [Crate Architecture](#crate-architecture)
- [Frontend Architecture](#frontend-architecture)
- [Tauri IPC Commands](#tauri-ipc-commands)
- [Scripting](#scripting)
- [Flow Engine](#flow-engine)
- [Build & Release](#build--release)
- [Debugging Tips](#debugging-tips)

---

## Prerequisites

| Dependency | Version | Notes |
|------------|---------|-------|
| Rust | edition 2021+ | Install via [rustup](https://rustup.rs) |
| Node.js | 18+ | Frontend build |
| pnpm | 8+ | Package manager (`npm i -g pnpm`) |
| OpenCV | 4.x | Vision recognition; see [opencv-rust install guide](https://github.com/twistedfall/opencv-rust/blob/master/INSTALL.md) |
| ONNX Runtime | — | Auto-downloaded by the `ort` crate; supports CUDA / DirectML acceleration |
| Windows SDK | — | Win32 API calls (screenshots, input simulation, overlay) |

> **Tip**: OpenCV linking configuration is the most common compilation issue. For detailed installation steps and environment variable setup, refer to the [opencv-rust install guide](https://github.com/twistedfall/opencv-rust/blob/master/INSTALL.md). Key environment variables: `OPENCV_LINK_PATHS` (lib directory), `OPENCV_LINK_LIBS` (library names), `OPENCV_INCLUDE_PATHS` (header directory).

### Model Assets

The model files required by the project (OCR, feature matching, etc.) are not distributed with the source code and must be downloaded separately:

1. Go to the [BetterNTE ModelScope files page](https://modelscope.cn/models/WWWSKY/BetterNTE/files) and download `assets.zip`
2. Extract it to the project root directory to get the `assets/` directory

```
BetterNTE/
├── assets/                  # ← Extract here
│   └── models/
│       ├── paddleocr/       # OCR models
│       ├── superpoint/      # Feature point detection models
│       ├── yolo/            # Object detection models
│       ├── mobilenet_v3_small-onnx-float/
│       └── test.png
├── crates/
├── data/
└── ...
```

> **Note**: The `assets/` directory is excluded in `.gitignore` and will not be committed to the repository. Tauri builds will bundle it as bundled resources.

---

## Project Structure

```
BetterNTE/
├── Cargo.toml              # Workspace root config
├── Cargo.lock
├── data/                   # Runtime data (scripts, config templates, etc.)
│   ├── main/               # Built-in scripts
│   └── plugins/            # Plugin manifests
├── crates/
│   ├── betternte-core/     # Base types, trait abstractions, config structures
│   ├── betternte-capture/  # Screenshot engine (WGC / DXGI / PrintWindow / ScreenDC / BitBlt)
│   ├── betternte-vision/   # Vision recognition (template matching, OCR, color detection, contour analysis)
│   ├── betternte-input/    # Input simulation (Win32 foreground/background, ADB emulator)
│   ├── betternte-helper/   # Standalone utility library (encoding, geometry, process, regex)
│   ├── betternte-notify/   # Multi-channel push notifications (ServerChan, Telegram, Bark, Webhook)
│   ├── betternte-overlay/  # Win32 overlay window and drawing API
│   ├── betternte-runtime/  # Flow Engine (Step / Transition / Condition)
│   ├── betternte-script/   # Script engine (QuickJS + async bridge)
│   ├── betternte-engine/   # Engine facade, assembles all subsystems
│   └── betternte-client/   # Tauri desktop client
│       ├── src-tauri/      # Rust backend (Tauri commands, engine integration)
│       ├── src/            # React frontend
│       ├── package.json
│       └── vite.config.ts
└── README.md
```

### Dependency Graph

```
betternte-helper              betternte-core
  (no internal deps)            (no internal deps)
        │                          │
        │         ┌────────────────┼────────────────┐
        │         │                │                │
        │    betternte-       betternte-       betternte-
        │    capture          vision           input
        │         │                │                │
        │         │           betternte-       betternte-
        │         │           notify           overlay
        │         │                │                │
        │         └────────┬───────┘                │
        │                  │                        │
        │           betternte-runtime               │
        │                  │                        │
        │           betternte-script ───────────────┘
        │                  │
        │           betternte-engine
        │                  │
        └──────── betternte-client
```

---

## Quick Start

```bash
# 1. Clone the repository
git clone https://github.com/719733328/BetterNTE.git
cd BetterNTE

# 2. Enter the client directory, install dependencies, and start dev mode
cd crates/betternte-client
pnpm install
pnpm tauri dev
```

> **Note**: `pnpm tauri dev` must be executed from the `crates/betternte-client` directory, not the project root.

On first launch, Tauri will automatically compile the Rust backend. Subsequent frontend changes will trigger HMR hot-reload, and Rust code changes will trigger automatic recompilation.

### Building Rust Backend Only

```bash
cargo build                    # Debug mode
cargo build --release          # Release mode
cargo check                    # Type check only, no binary output
```

### Running Frontend Only

```bash
cd crates/betternte-client
pnpm dev                       # Vite dev server on localhost:1420
```

---

## Crate Architecture

### betternte-core — Foundation Layer

Common dependency for all other crates, defining core types and trait abstractions for the entire project.

**Key Types:**
- `EngineConfig` — Top-level config structure, deserialized from `engine.yaml`, containing 15+ sub-configs (screenshot, hotkeys, overlay, notifications, API, game window, etc.)
- `CaptureFrame` — Generic image container, supports cropping, scaling, format conversion, PNG/JPEG/BMP export
- `GameWindow` — Game window info (handle, title, process, DPI, region)
- `EngineEvent` — Event bus event enum (TaskStarted, TaskStopped, ScriptLoaded, ConfigChanged, Error, LogMessage)

**Key Traits:**
- `ScreenCapture` — Screenshot interface
- `InputController` — Input simulation interface
- `TemplateMatcher` — Template matching interface
- `OcrEngine` — OCR interface
- `ColorDetector` — Color detection interface
- `WindowFinder` — Window finding interface

> Concrete implementations of all traits are distributed across `betternte-capture`, `betternte-input`, `betternte-vision`, etc.

---

### betternte-capture — Screenshot Engine

Provides 5 Windows screenshot backends with no third-party screenshot library dependencies:

| Backend | API | Characteristics |
|---------|-----|-----------------|
| `WgcCapture` | Windows Graphics Capture | GPU accelerated, persistent session |
| `DxgiDupCapture` | DXGI Desktop Duplication | GPU, desktop-level capture |
| `PrintWindowCapture` | GDI PrintWindow | Can capture occluded windows |
| `ScreenDCCapture` | Screen DC + BitBlt | GDI, can capture occluded windows |
| `BitBltCapture` | GDI BitBlt | Best compatibility, cannot capture occluded windows |

**Factory Functions:**
- `create_capture_engine()` — Auto-selects backend based on config
- `resolve_auto_capture_method()` — Probes the best available screenshot method on the current system

---

### betternte-vision — Vision Recognition

Vision pipeline based on OpenCV and ONNX Runtime:

- **Template Matching**: `OpenCvTemplateMatcher` — NCC normalized cross-correlation with template caching
- **OCR**: `PaddleOcrEngine` — PaddleOCR ONNX models (detection + recognition)
- **Color Detection**: `ColorDetectorImpl` — Color range matching
- **Contour Analysis**: `ContourFinder` / `ContourAnalyzer`
- **Morphology**: `Morphology` — Erosion, dilation, etc.
- **Feature Matching**: `SuperPointDetector` / `LightGlueMatcher` — ONNX feature detection and matching
- **Image Preprocessing**: `ImagePreprocessor` — General image processing utilities

---

### betternte-input — Input Simulation

Supports input simulation for both PC native windows and Android emulators:

- `Win32Input` — Win32 implementation (foreground SendInput / background PostMessage)
- `AdbInput` — ADB implementation for Android emulators
- `InputQueue` — Input queue with rate limiting
- `FailoverInputController` — Automatic primary/backup switching
- `InputRecorder` / `MacroPlayer` — Macro recording and playback

---

### betternte-helper — Utility Library

Zero-internal-dependency standalone utility crate providing:
- Directory operations (create, copy, delete, size calculation)
- Encoding (Base64, MD5)
- Geometry (point, rectangle, distance, intersection)
- Process info (PID, debug mode, elevation status)
- String processing (Chinese detection, number extraction)
- Windows-specific (DPI, foreground window)

---

### betternte-notify — Push Notifications

Multi-channel push notification system defining a `Notifier` trait:

| Channel | Implementation | Platform |
|---------|---------------|----------|
| ServerChan | `ServerChanNotifier` | WeChat |
| Telegram | `TelegramNotifier` | Telegram |
| Bark | `BarkNotifier` | iOS |
| Webhook | `WebhookNotifier` | WeCom, DingTalk, Feishu, Discord, Slack |

`NotificationManager` manages all channels, providing `send_all()`, `send_to()`, `test_channel()`.

---

### betternte-overlay — Overlay

Win32 layered transparent window (`WS_EX_TRANSPARENT + WS_EX_LAYERED`), can overlay on the game window to display debug information:

- `OverlayWindow` — Low-level window operations
- `OverlayManager` — High-level manager, binds to game window, syncs position
- `OverlayRenderer` — Frame rendering (begin_frame → draw → end_frame)
- `DrawingApi` — Pixel drawing (rectangles, lines, text, crosshairs, circles, progress bars)

---

### betternte-runtime — Flow Engine

Unified flow execution engine based on the Flow / Step / Transition model:

- `Flow` — Directed graph composed of `Step` and `Transition`
- `Step` — Execution unit, types include script, click, swipe, key_press, wait, flow (nested), group, set_variable
- `Transition` — Connection between steps, with `Condition` conditions
- `Condition` — Condition enum (Always, Template, Ocr, Color, Variable, Hotkey, Script, And/Or/Not)
- `FlowExecutor` — Executor main loop
- `VariableStore` — Two-layer variable system (default values + persistence), supports reference resolution (`$variables.x`, `$result.y`)
- `PermissionGuard` — Manifest-based permission sandbox

---

### betternte-script — Script Engine

JavaScript script runtime based on QuickJS, integrated via the `rquickjs` crate.

**Script Types:**
- `SoloTask` — One-shot task script (calls `start()`)
- `Trigger` — Frame-driven trigger (calls `on_enable()` + per-frame `on_capture()`)
- `Library` — Reusable module (calls `call_function()`)

**QuickJS Async Bridge:**

QuickJS itself is synchronous, but all `ScriptContext` methods are async. The solution:

1. Dedicated background thread `qjs-async-bridge` with its own tokio runtime
2. JS global `__invoke(method, args_json)` synchronous function registered
3. JS wrapper functions wrap `__invoke` into `Promise`, scripts can directly `await ctx.click(100, 200)`
4. `__invoke` sends async closures to the bridge thread via `mpsc::channel`, polls `recv_timeout(50ms)` to check cancellation
5. Nested calls (library calling ctx methods) use `dispatch_ctx_method_nested_blocking`, spawning independent threads + single-threaded tokio runtime to avoid deadlocks

**ScriptContext API (~45 methods):**
- Screenshot: `capture()`, `capture_region()`
- Recognition: `find_template()`, `ocr()`, `get_color()`, `color_match()`
- Input: `click()`, `key_press()`, `swipe()`, `type_text()`
- Wait: `sleep()`, `wait_for_template()`, `wait_for_color()`
- Window: `find_window()`, `activate_window()`
- Storage: `storage_get()`, `storage_set()`
- File: `read_file()`, `write_file()`
- Network: `http_get()`, `http_post()`
- IPC: `run_script()`, `call_library()`
- Notification: `notify()`

---

### betternte-engine — Engine Facade

Top-level facade; the client only interacts with this crate:

```rust
// Lifecycle
EngineBuilder::new(config, base_dir).build()  // -> Engine (Idle)
engine.start()                                 // -> Running
engine.stop()                                  // -> Idle
```

On `start()`:
1. Load scripts from all subscription directories
2. Sync trigger states
3. Bind to target game window
4. Start screenshot loop (spawns tokio task at configured FPS)
5. Start replay recording (if configured)

`EngineBuilder` supports custom `StepHandler`, `ConditionHandler`, `InputRunner` for extensibility.

---

### betternte-client — Tauri Desktop Client

Tauri v2 desktop application. The Rust backend holds `Option<Engine>` via `AppState` (protected by `tokio::sync::RwLock`).

**Modules:**
- `lib.rs` — Plugin registration, system tray, window management, event bridging, log initialization
- `hotkeys.rs` — Global hotkeys (emergency stop, toggle overlay, script/task group triggers)
- `commands/` — 55 Tauri IPC commands across 6 modules

**Tauri Plugins:**
- `tauri-plugin-shell` — Shell command execution
- `tauri-plugin-notification` — System notifications
- `tauri-plugin-dialog` — File dialogs
- `tauri-plugin-clipboard-manager` — Clipboard
- `tauri-plugin-global-shortcut` — Global hotkeys
- `tauri-plugin-single-instance` — Single instance
- `tauri-plugin-updater` — Auto-update (desktop only)

---

## Frontend Architecture

### Tech Stack

| Layer | Technology |
|-------|------------|
| UI Framework | React 19 + TypeScript 6 |
| Build Tool | Vite 8 |
| Routing | react-router-dom 7 |
| State Management | Zustand 5 |
| Styling | Tailwind CSS 4 |
| Icons | lucide-react |
| Code Editor | CodeMirror 6 |
| Flow Editor | @xyflow/react 12 + @dagrejs/dagre |

### Routes

| Path | Page | Description |
|------|------|-------------|
| `/` | HomePage | Launch page |
| `/triggers` | TriggerPage | Trigger management |
| `/scripts` | TaskPage | Script management and execution |
| `/one-dragon` | OneDragonFlow | Task group orchestration |
| `/workflow` | FlowEditorPage | Visual flow editor |
| `/debug` | ScriptDebugPage | Script debug tracing |
| `/settings` | Settings | Engine configuration |
| `/input-test` | InputTestPage | Input debugging (dev mode only) |

### State Management (Zustand Slices)

| Slice | Responsibility |
|-------|---------------|
| `EngineSlice` | Engine lifecycle, config, status, screenshot testing |
| `ScriptSlice` | Script/trigger CRUD, run/stop, source read/write |
| `FlowSlice` | Flow / TaskGroup CRUD, run/stop/progress |
| `UISlice` | Logs, recent tasks, error dialogs, event listening |
| `DebugSlice` | Script call tracing |

### Event Bridging

Rust `EventBus` sends events to the frontend via `app.emit("engine-event", ...)`. The frontend `UISlice.setupEventListener()` uses `listen()` from `@tauri-apps/api/event` to receive and dispatch events to Zustand state.

---

## Tauri IPC Commands

A total of 55 commands, grouped by module:

### Engine (5)
| Command | Description |
|---------|-------------|
| `init_engine` | Initialize engine (idempotent), loads config, injects data, registers hotkeys |
| `start_engine` | Start engine |
| `stop_engine` | Stop engine, release resources |
| `get_status` | Get engine status (idle/running, current task, script count, version) |
| `stop_all` | Emergency stop all running tasks |

### Scripts (13)
| Command | Description |
|---------|-------------|
| `reload_scripts` | Reload scripts from disk |
| `list_scripts` | List all loaded scripts |
| `run_script` | Run script (auto-starts engine) |
| `stop_task` | Stop current task |
| `enable_trigger` / `disable_trigger` | Enable/disable trigger |
| `reload_triggers` / `list_triggers` | Reload/list triggers |
| `create_script` / `delete_script` | Create/delete script |
| `list_script_files` | List files in script directory |
| `read_script_source` / `save_script_source` | Read/write script source (with path traversal protection) |
| `import_script_asset` | Import asset file to script directory |

### Flows (11)
| Command | Description |
|---------|-------------|
| `list_task_groups` / `save_task_group` / `delete_task_group` | Task group CRUD |
| `run_task_group` / `stop_task_group` / `get_task_group_progress` | Task group run control |
| `list_flows` / `save_flow` / `delete_flow` | Flow CRUD |
| `run_flow` / `stop_flow` / `get_flow_progress` | Flow run control |

### Input (12)
| Command | Description |
|---------|-------------|
| `input_list_windows` / `input_bind_window` | Window list and binding |
| `input_key_down` / `input_key_up` / `input_key_tap` | Keyboard simulation |
| `input_mouse_move` / `input_mouse_scroll` / `input_mouse_button` / `input_mouse_click` | Mouse simulation |
| `input_demo_*` | Composite input demos |
| `input_run_js_snippet` | Execute JS code snippet |

### Settings (12)
| Command | Description |
|---------|-------------|
| `get_config` / `save_config_cmd` | Read/write engine config |
| `get_capture_methods` | List available screenshot methods |
| `list_subscriptions` / `save_subscription` / `delete_subscription` | Script subscription management |
| `list_windows` / `find_game_window` | System window enumeration |
| `test_screenshot` | Test screenshot (returns base64) |
| `test_notification_channel` | Test push notification channel |
| `list_game_plugins` | List game plugins |
| `export_logs` | Export logs |
| `better_nte_debug_enabled` | Check debug mode |

### Replay (2)
| Command | Description |
|---------|-------------|
| `replay_verify_session` | Verify replay session |
| `replay_verify_artifacts` | Verify replay artifacts |

---

## Scripting

### Script Directory Structure

```
scripts/
└── my-script/
    ├── manifest.json      # Script metadata
    ├── main.js            # Entry file
    └── assets/            # Asset files (template images, etc.)
```

### manifest.json

```json
{
  "name": "my-script",
  "version": "1.0.0",
  "type": "solo_task",
  "entry": "main.js",
  "permissions": ["capture", "input", "template", "ocr"],
  "dependencies": [],
  "engine_version": ">=0.0.1"
}
```

**type field:**
- `solo_task` — One-shot task
- `trigger` — Frame trigger
- `library` — Reusable library

### ctx API Quick Reference

Scripts call engine capabilities through the global `ctx` object:

```javascript
// Screenshot
const frame = await ctx.capture();
const region = await ctx.capture_region(x, y, w, h);

// Template matching
const result = await ctx.find_template("button.png", { threshold: 0.8 });
const results = await ctx.find_templates(["a.png", "b.png"]);

// OCR
const text = await ctx.ocr({ x: 0, y: 0, w: 200, h: 50 });

// Color
const color = await ctx.get_color(100, 200);
const matched = await ctx.color_match(100, 200, { r: 255, g: 0, b: 0 }, 30);

// Input
await ctx.click(500, 300);
await ctx.key_press("enter");
await ctx.swipe(100, 200, 300, 200, 500);

// Wait
await ctx.sleep(1000);
await ctx.wait_for_template("dialog.png", { timeout: 5000 });

// Storage
await ctx.storage_set("count", "42");
const val = await ctx.storage_get("count");

// Network
const data = await ctx.http_get("https://api.example.com/status");

// IPC
await ctx.run_script("other-script");
const result = await ctx.call_library("utils", "formatDate", [Date.now()]);

// Notification
await ctx.notify("Task Complete", "Script executed successfully");
```

---

## Flow Engine

### Data Model

```
Flow
 ├── entry: StepId           # Entry step
 └── steps: Map<StepId, Step>
      ├── id: StepId
      ├── kind: StepKind     # script / click / swipe / key_press / wait / flow / group / set_variable
      ├── transitions: [Transition]
      │    ├── condition: Condition
      │    ├── target: StepId
      │    ├── priority: int
      │    └── interrupt: bool
      ├── on_error: StepId?  # Error fallback
      ├── retry: int
      └── timeout: Duration
```

### Condition System

The `Condition` enum supports composition:

```
Condition::Always
Condition::Template { image, threshold }
Condition::Ocr { region, pattern }
Condition::Color { x, y, color, tolerance }
Condition::Variable { name, op, value }
Condition::Hotkey { key }
Condition::Script { script, function }
Condition::And(Box<Condition>, Box<Condition>)
Condition::Or(Box<Condition>, Box<Condition>)
Condition::Not(Box<Condition>)
```

### Variable System

`VariableStore` provides two-layer variables:
- **Default values**: Declared in the Flow definition
- **Runtime**: Modified during execution via `set_variable` steps
- **Reference resolution**: `$variables.x`, `$result.y`, `$steps.z.result.w`, `$flow_output.k`

---

## Build & Release

### Development Build

```bash
cargo build                              # Full workspace debug
cargo build -p betternte-engine          # Single crate
cargo check                              # Type check only
```

### Tauri Build

```bash
cd crates/betternte-client
pnpm tauri build                         # Production build (NSIS installer)
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `OPENCV_LINK_PATHS` | OpenCV lib directory path |
| `OPENCV_LINK_LIBS` | OpenCV library names to link |
| `OPENCV_INCLUDE_PATHS` | OpenCV header path (usually auto-detected) |
| `BETTER_NTE_DEBUG` | Set to `1` to enable debug mode |

### Build Artifacts

Tauri build produces an NSIS installer (`.exe`), containing:
- Compiled Rust binary
- Frontend static assets (`dist/`)
- `data/` and `assets/` as bundled resources

---

## Debugging Tips

### Debug Mode

Set the environment variable `BETTER_NTE_DEBUG=1` to enable debug mode:
- Frontend will show the "Input Test" sidebar entry
- Additional debug panels become available

### Logs

- Log files are located in the app data directory, using `tracing` + `tracing-subscriber` with automatic rotation
- Export log files via the "Export Logs" button on the settings page
- Frontend `FloatingLogLayer` and `LogDrawer` display logs in real time

### Overlay Debugging

The overlay can display in real time:
- Template match results (match position, confidence)
- Crosshairs (mouse/touch point)
- Progress bars
- Custom text annotations

Controlled via the `overlay` field in engine configuration.

### Input Test Page

The `/input-test` route (visible only in debug mode) provides:
- Keyboard key testing
- Mouse click/move testing
- Window binding testing

### Script Debugging

The `/debug` route provides script call tracing, recording the parameters and return values of each `ctx` method call.

### JS Code Snippets

Via the `input_run_js_snippet` Tauri command, you can execute arbitrary JS code snippets at engine runtime to quickly test `ctx` APIs.
