# Scripting Guide

> **[English](scripting-guide_EN.md)** | [简体中文](scripting-guide.md)

This document is for developers writing automation scripts for BetterNTE. BetterNTE uses JavaScript (QuickJS engine) as its scripting language, with the `ctx` global object providing access to engine capabilities.

> **Note**: The project is currently in early development, and the API may undergo significant changes. Method signatures, parameters, and return values in this document may change with version updates. Please refer to the actual running engine version.

---

## Table of Contents

- [Quick Start](#quick-start)
- [Script Directory Structure](#script-directory-structure)
- [manifest.json Reference](#manifestjson-reference)
- [Script Lifecycle](#script-lifecycle)
- [ctx API Reference](#ctx-api-reference)
- [Data Types](#data-types)
- [Permission System](#permission-system)
- [User Parameters](#user-parameters)
- [Library Scripts](#library-scripts)
- [Flow Definitions](#flow-definitions)
- [Condition System](#condition-system)
- [Variable System](#variable-system)
- [Best Practices](#best-practices)
- [Example Scripts](#example-scripts)

---

## Quick Start

### Minimal Script

```
my-script/
├── manifest.json
└── main.js
```

**manifest.json:**
```json
{
  "schema_version": 1,
  "name": "my-script",
  "display_name": "My Script",
  "version": "1.0.0",
  "type": "solo_task",
  "entry": "main.js",
  "permissions": ["screenshot", "template_match", "click"]
}
```

**main.js:**
```javascript
async function start() {
  ctx.logInfo("Script started");

  const match = await ctx.findTemplate("button.png", { threshold: 0.9 });
  if (match) {
    await ctx.click(match.x + match.width / 2, match.y + match.height / 2);
  }

  ctx.logInfo("Script finished");
  return "done";
}
```

Place the script directory into `data/main/scripts/`, restart the engine, and you'll see it in the client ready to run.

---

## Script Directory Structure

```
data/main/scripts/
├── my-task/
│   ├── manifest.json       # Script metadata (required)
│   ├── main.js             # Entry file (required)
│   ├── templates/          # Template images directory
│   │   ├── button.png
│   │   └── dialog.png
│   └── store/              # Persistent files (auto-created, no declaration needed)
│       └── config.json
├── my-trigger/
│   ├── manifest.json
│   ├── main.js
│   └── templates/
└── lib/
    ├── manifest.json
    └── main.js
```

**Directory Rules:**
- Images in the `templates/` directory are referenced by filename (without extension), e.g. `ctx.findTemplate("button.png")`
- The `store/` directory is used for script-level file persistence, accessed via `ctx.readStoreFile()` / `ctx.writeStoreFile()`
- The engine automatically skips `templates/` and `store/` directories and will not scan them as subscripts

---

## manifest.json Reference

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `schema_version` | `number` | Must be `1` |
| `name` | `string` | Unique identifier, English snake_case, e.g. `"fishing_assist"` |
| `display_name` | `string` | Display name, can be in any language, e.g. `"Fishing Helper"` |
| `version` | `string` | Semantic version, e.g. `"1.0.0"` |
| `type` | `string` | Script type, see below |
| `entry` | `string` | Entry filename, e.g. `"main.js"` |

### Optional Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `author` | `string` | `""` | Author |
| `description` | `string` | `""` | Description |
| `icon` | `string` | `null` | Icon file path |
| `permissions` | `string[]` | `[]` | Permission declarations, see [Permission System](#permission-system) |
| `tags` | `string[]` | `[]` | Tags for categorization |
| `category` | `string` | `null` | Category |
| `params_schema` | `object` | `null` | JSON Schema defining user-configurable parameters |
| `dependencies` | `object[]` | `[]` | Dependent library scripts |
| `engine_version` | `string` | `null` | Engine version range, e.g. `"^0.0.1"` |
| `min_engine_version` | `string` | `null` | Minimum engine version |
| `max_engine_version` | `string` | `null` | Maximum engine version (exclusive) |

### type Values

| Value | Alias | Description |
|-------|-------|-------------|
| `"solo_task"` | `"task"`, `"flow"` | One-shot task script |
| `"trigger"` | — | Frame trigger, executes every frame |
| `"library"` | — | Reusable library, cannot be run directly |

### Dependency Declaration

```json
{
  "dependencies": [
    { "path": "main/scripts/lib" }
  ]
}
```

`path` is a POSIX path relative to the data root directory, and cannot contain `..`.

### Complete Example

```json
{
  "schema_version": 1,
  "name": "cafe_income",
  "display_name": "Collect Cafe Income",
  "version": "1.0.0",
  "type": "solo_task",
  "entry": "main.js",
  "author": "BetterNTE",
  "description": "Enter cafe, collect, confirm, close; optional restock chain.",
  "tags": ["NTE", "Tycoon", "Cafe"],
  "permissions": ["screenshot", "template_match", "click", "keyboard", "color_detect", "ocr"],
  "dependencies": [{ "path": "main/scripts/lib" }],
  "params_schema": {
    "type": "object",
    "properties": {
      "enable_restock": { "type": "boolean", "title": "Auto Restock", "default": true },
      "max_rounds": { "type": "integer", "title": "Max Polling Rounds", "default": 25, "minimum": 5, "maximum": 100 },
      "enter_key": { "type": "string", "title": "Enter Key", "default": "f5" },
      "exit_key": { "type": "string", "title": "Exit Key", "default": "esc" }
    }
  }
}
```

---

## Script Lifecycle

The engine calls global functions in scripts in the following order (if they exist):

### solo_task Type

```
init()  →  start()  →  stop()  →  destroy()
```

| Phase | Function | Description |
|-------|----------|-------------|
| Load | `init()` | Called once after source is loaded, for initialization |
| Run | `async start()` | Called when user triggers execution, must return a value. Can also use `main()` |
| Stop | `stop()` | Called when user stops or script is cancelled |
| Unload | `destroy()` | Called when script is removed from engine |

### trigger Type

```
init()  →  onEnable(params)  →  onCapture(frame) / onTrigger(ctx) [per frame]  →  onDisable()  →  destroy()
```

| Phase | Function | Description |
|-------|----------|-------------|
| Load | `init()` | Same as above |
| Enable | `onEnable(params)` | Called when trigger is enabled |
| Frame callback | `onCapture({width, height})` | Called each frame, `frame` contains current frame info |
| Frame callback | `onTrigger(ctx)` | Alternative to `onCapture`, receives ctx directly |
| Disable | `onDisable()` | Called when trigger is disabled |
| Unload | `destroy()` | Same as above |

> The engine prioritizes `onTrigger`; if not found, it falls back to `onCapture`.

### library Type

Library scripts have no lifecycle functions. Register callable functions via `registerLibrary()`.

---

## ctx API Reference

All async methods return `Promise` and must use `await`. Sync methods return values directly.

### Screenshot

Requires permission: `screenshot`

```javascript
// Capture full screen
const frame = await ctx.capture(force?);
// Returns: { width, height, data_len }

// Capture specific region
const region = await ctx.captureRegion(x, y, w, h, force?);
// Returns: { width, height }

// Alias
const frame = await ctx.screenshot(force?);
```

When `force` is `true`, forces a frame cache refresh. Default is `false` (reuses the most recent frame).

### Template Matching

Requires permission: `template_match`

```javascript
// Find single template (returns first match or null)
const match = await ctx.findTemplate(name, opts?);
// Returns: { x, y, width, height, confidence } or null

// Find all matches
const matches = await ctx.findTemplates(name, opts?);
// Returns: MatchResult[]

// Batch find (single frame, multiple templates, better performance)
const results = await ctx.findTemplateBatch(entries);
// entries: [{ name, threshold?, roi?, ... }]
// Returns: (MatchResult | null)[]

// Wait for template to appear
const match = await ctx.waitForTemplate(name, timeoutMs, opts?);
// Returns null on timeout

// Wait for template to disappear
const gone = await ctx.waitGone(name, timeoutMs);
// Returns: bool

// Frame-count versions (more precise)
const match = await ctx.waitForTemplateFrames(name, maxFrames, opts?);
const gone = await ctx.waitGoneFrames(name, maxFrames);

// Alias
const match = await ctx.matchTemplate(name, opts?);
```

**opts parameter:**

```javascript
{
  threshold: 0.8,           // Match threshold, 0.0-1.0, default 0.8
  roi: { x, y, width, height }, // Region of interest, limits search area
  orderBy: "score",         // Sort order: "score" | "horizontal" | "vertical" | "area" | "random"
  resultIndex: 0,           // Which result to return (supports negative, -1 = last)
  nmsThreshold: 0.3,        // Non-maximum suppression IoU threshold
  maxResults: 10,           // Max results (findTemplates only)
  greenMask: false,         // Exclude #00FF00 pixels (green screen)
  greenMaskTolerance: 0,    // Green tolerance
  useAlphaMask: false,      // Exclude transparent pixels (PNG transparency)
  alphaMaskThreshold: 8     // Alpha threshold
}
```

### OCR Text Recognition

Requires permission: `ocr`

```javascript
// Recognize text in specific region
const text = await ctx.ocr(x, y, w, h);
// Returns: string

// Recognize all text on screen
const results = await ctx.ocrAll();
// Returns: [{ text, region: { x, y, width, height }, confidence }]
```

### Color Detection

Requires permission: `color_detect`

```javascript
// Get pixel color
const color = await ctx.getColor(x, y);
// Returns: "#RRGGBB"

// Check if pixel matches color
const matched = await ctx.colorMatch(x, y, "#FF0000", tolerance);
// Returns: bool

// Batch color matching
const result = await ctx.colorMatchAll(points, opts?);
// points: [{ x, y, color, tolerance?, rgbaTolerance? }]
// Returns: bool (normal mode) or ColorMatchAllResult (debug mode)

// Count pixels matching a color in a region
const count = await ctx.countColor("#FF0000", { tolerance: 10, roi: { x: 0, y: 0, width: 1920, height: 1080 } });
// tolerance defaults to 0, roi is optional (full screen if not provided)
// Returns: number (matching pixel count)

// Wait for color to appear
const found = await ctx.waitForColor(x, y, "#FF0000", timeoutMs);
const found = await ctx.waitForColorFrames(x, y, "#FF0000", maxFrames);
```

**colorMatchAll opts:**

```javascript
{
  defaultTolerance: 32,     // Default tolerance
  debug: false,             // Debug mode, returns detailed match info
  shiftMax: {               // Allowed offset search range
    maxDx: 5,
    maxDy: 5
  }
}
```

**rgbaTolerance (per-channel tolerance):**

```javascript
{ r: 10, g: 10, b: 10, a: 255 }
```

### Input Simulation

Requires permission: `click`

```javascript
// Mouse
await ctx.click(x, y);                     // Left click
await ctx.doubleClick(x, y);               // Double click
await ctx.rightClick(x, y);                // Right click
await ctx.mouseMove(x, y);                 // Move mouse
await ctx.mouseDown("left");               // Press ("left" | "right" | "middle")
await ctx.mouseUp("left");                 // Release
await ctx.scroll(delta);                   // Scroll (positive=up, negative=down)
await ctx.swipe(x1, y1, x2, y2, durationMs); // Swipe

// Keyboard
await ctx.keyDown("a");                    // Press down
await ctx.keyUp("a");                      // Release
await ctx.keyPress("enter");               // Press and release
await ctx.keyPress("a", 100);              // Hold for 100ms then release
await ctx.keyCombo(["ctrl", "c"]);         // Key combination
await ctx.typeText("hello");               // Type text
```

**Key names:** Use standard key names like `"enter"`, `"esc"`, `"space"`, `"tab"`, `"f1"`-`"f12"`, `"a"`-`"z"`, `"0"`-`"9"`, `"ctrl"`, `"alt"`, `"shift"`, etc.

### Wait

```javascript
// Wait for specified milliseconds
await ctx.sleep(1000);

// Wait for specified frame count
await ctx.sleepFrames(10);
```

`sleep` and `sleepFrames` do not require permissions.

### Window Management

Requires permission: `window`

```javascript
// Find window
const hwnd = await ctx.findWindow("Game Window Title");
// Returns: u64 window handle, null if not found

// Activate window
await ctx.activateWindow(hwnd);

// Get window region
const rect = await ctx.getWindowRect(hwnd);
// Returns: { x, y, width, height }

// Get screen size
const [width, height] = await ctx.getScreenSize();
```

`getScreenSize` requires the `screenshot` permission.

### Inter-Script Calls

```javascript
// Run another script
const result = await ctx.runScript("other-script", { key: "value" });

// Call library function
const result = await ctx.call("lib", "functionName", { arg1: "value" });
```

Requires permission: `call_script` / `call_library`

### Storage (Script-Level KV Store)

No additional permissions needed. Data is stored in `{script_directory}/storage.json`.

```javascript
await ctx.storageSet("count", 42);
const count = await ctx.storageGet("count");    // 42 or null
await ctx.storageDelete("count");
const keys = await ctx.storageKeys();            // ["key1", "key2", ...]
```

Values can be any JSON type (string, number, boolean, object, array, null).

### File Operations (Script-Level)

No additional permissions needed. Paths are relative to `{script_directory}/store/`.

```javascript
await ctx.writeStoreFile("data.json", '{"a":1}');
const content = await ctx.readStoreFile("data.json");
const files = await ctx.listStoreFiles(".");
```

### File Operations (System-Level)

Requires permission: `file`

```javascript
const content = await ctx.readFile("/path/to/file");
await ctx.writeFile("/path/to/file", "content");
const files = await ctx.listFiles("/path/to/dir");
const exists = await ctx.fileExists("/path/to/file");
```

### Network Requests

Requires permission: `network`

```javascript
const response = await ctx.httpGet("https://api.example.com/data");
const response = await ctx.httpPost("https://api.example.com/submit", '{"key":"value"}');
```

### Notifications

Requires permission: `notify`

```javascript
await ctx.notify("Title", "Content");
```

Sends push notifications via configured notification channels (ServerChan, Telegram, Bark, Webhook).

### Logging

No permissions required, synchronous call.

```javascript
ctx.logDebug("Debug info");
ctx.logInfo("Info");
ctx.logWarn("Warning");
ctx.logError("Error");
ctx.log("info", "Custom level");  // level: "debug" | "info" | "warn" | "error"

// console global is also available
console.log("Same as ctx.logInfo");
console.warn("Same as ctx.logWarn");
console.error("Same as ctx.logError");
```

### Status & Progress

No permissions required, synchronous call.

```javascript
// Check if cancelled
if (ctx.isCancelled()) return;

// Report progress (shown in client UI)
ctx.progress(current, total);  // e.g. ctx.progress(3, 10)

// Get current FPS
const fps = ctx.getFps();      // Default 60

// Get current frame number
const frame = ctx.getFrameNumber();
```

---

## Data Types

### MatchResult

Template matching result.

```javascript
{
  x: 100,           // i32 - Match area top-left X
  y: 200,           // i32 - Match area top-left Y
  width: 50,        // u32 - Match area width
  height: 30,       // u32 - Match area height
  confidence: 0.95  // f64 - Match confidence 0.0-1.0
}
```

**Click the center of a match:**
```javascript
const m = await ctx.findTemplate("btn.png");
if (m) {
  await ctx.click(m.x + m.width / 2, m.y + m.height / 2);
}
```

### Region / ROI

Region of interest, used to limit search area.

```javascript
{ x: 0, y: 0, width: 1920, height: 1080 }
```

### OcrResult

OCR recognition result.

```javascript
{
  text: "Start Game",        // string - Recognized text
  region: { x, y, width, height },  // Region
  confidence: 0.88         // f64 - Confidence
}
```

### CaptureFrame

Screenshot frame info (JS side can only get metadata, not pixel data).

```javascript
{ width: 1920, height: 1080, data_len: 8294400 }
```

---

## Permission System

Declare required permissions in the `permissions` array of `manifest.json`. API calls without declared permissions will be rejected.

### Permission List

| Permission | Available APIs |
|------------|---------------|
| `screenshot` | `capture`, `captureRegion`, `getScreenSize` |
| `template_match` | `findTemplate`, `findTemplates`, `findTemplateBatch`, `waitForTemplate`, `waitGone`, `waitForTemplateFrames`, `waitGoneFrames` |
| `ocr` | `ocr`, `ocrAll` |
| `color_detect` | `getColor`, `colorMatch`, `colorMatchAll`, `countColor`, `scanSliderStrip`, `scanStripEdges`, `waitForColor`, `waitForColorFrames` |
| `click` | `click`, `doubleClick`, `rightClick`, `mouseMove`, `mouseDown`, `mouseUp`, `scroll`, `swipe`, `keyDown`, `keyUp`, `keyPress`, `keyCombo`, `typeText` |
| `keyboard` | Same as `click` (alias) |
| `window` | `findWindow`, `activateWindow`, `getWindowRect` |
| `call_script` | `runScript` |
| `call_library` | `call` |
| `notify` | `notify` |
| `storage` | `readStoreFile`, `writeStoreFile`, `listStoreFiles`, `storageGet`, `storageSet`, `storageDelete`, `storageKeys` |
| `file` | `readFile`, `writeFile`, `listFiles`, `fileExists` |
| `network` | `httpGet`, `httpPost` |

### Permission-Free Methods

`sleep`, `sleepFrames`, `isCancelled`, `progress`, `getFps`, `getFrameNumber`, `log*`, `console.*`

### Example

```json
{
  "permissions": ["screenshot", "template_match", "click", "ocr"]
}
```

---

## User Parameters

Define user-configurable parameters via `params_schema`. The client will automatically generate a settings form.

### Defining Parameters

```json
{
  "params_schema": {
    "type": "object",
    "properties": {
      "count": {
        "type": "integer",
        "title": "Loop Count",
        "default": 999,
        "minimum": 1,
        "maximum": 9999
      },
      "enable_feature": {
        "type": "boolean",
        "title": "Enable Feature",
        "default": true
      },
      "mode": {
        "type": "string",
        "title": "Run Mode",
        "enum": ["fast", "normal", "slow"],
        "default": "normal"
      }
    }
  }
}
```

### Reading Parameters

```javascript
function getParams() {
  return typeof globalThis.config === "object" && globalThis.config !== null
    ? globalThis.config : {};
}

async function start() {
  const cfg = getParams();
  const count = cfg.count || 999;
  const enabled = cfg.enable_feature !== false;
  // ...
}
```

Parameters are injected via `globalThis.config` and can also be accessed via `ctx.params`.

---

## Library Scripts

Library scripts (`type: "library"`) encapsulate reusable logic and cannot be run directly.

### Registering Functions

```javascript
// lib/main.js
registerLibrary("roi", function(args) {
  return { x: args.x | 0, y: args.y | 0, width: args.width | 0, height: args.height | 0 };
});

registerLibrary("findFirst", async function(args) {
  const names = args.names || [];
  const threshold = args.threshold != null ? args.threshold : 0.8;
  for (const name of names) {
    const m = await ctx.findTemplate(name, { threshold });
    if (m) return { name, match: m };
  }
  return null;
});
```

### Using Libraries

Declare dependencies in the task script's `manifest.json`:

```json
{
  "dependencies": [{ "path": "main/scripts/lib" }]
}
```

Call in code:

```javascript
const roi = await ctx.call("lib", "roi", { x: 100, y: 200, width: 300, height: 400 });
const found = await ctx.call("lib", "findFirst", { names: ["a.png", "b.png"], threshold: 0.9 });
```

### Library Function Characteristics

- Can be `async` functions
- Can call `ctx.*` methods (library's own manifest must declare corresponding permissions)
- Parameters and return values are JSON format
- Register with `registerLibrary(name, fn)` or `globalThis.registerLibrary`

---

## Flow Definitions

Flow is a directed-graph task orchestration model composed of Steps and Transitions.

### Flow JSON Structure

```json
{
  "id": "my-flow",
  "name": "My Flow",
  "entry": "start",
  "variables": {
    "round": { "value_type": "integer", "default": 0, "persist": true }
  },
  "steps": {
    "start": {
      "kind": { "type": "script", "script": "check-task" },
      "input": { "current_round": "$variables.round" },
      "output": { "$variables.round": "$result.new_round" },
      "transitions": [
        {
          "target": "done",
          "condition": { "type": "variable", "key": "$variables.round", "op": "gte", "value": 10 },
          "priority": 10
        },
        {
          "target": "start",
          "condition": { "type": "always" },
          "priority": 0
        }
      ],
      "timeout_ms": 30000,
      "max_retries": 3,
      "on_error": "error_handler"
    },
    "error_handler": {
      "kind": { "type": "click", "x": 100, "y": 100 },
      "transitions": [
        { "target": "start", "condition": { "type": "always" } }
      ]
    },
    "done": {
      "kind": { "type": "none" },
      "transitions": []
    }
  }
}
```

### Step Types

| type | Fields | Description |
|------|--------|-------------|
| `"script"` | `script` | Execute JS script |
| `"click"` | `x`, `y` | Click coordinates |
| `"swipe"` | `x1`, `y1`, `x2`, `y2`, `duration_ms` | Swipe gesture |
| `"key_press"` | `key` | Key press |
| `"wait"` | `ms` | Wait milliseconds |
| `"flow"` | `flow` | Nested sub-flow |
| `"group"` | `group` | Call task group |
| `"set_variable"` | `key`, `value` | Set variable |
| `"none"` | — | No-op (branch-only node) |

### Transition Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `target` | `string` | — | Target step ID |
| `condition` | `Condition` | — | Transition condition |
| `priority` | `u8` | `0` | Priority, higher = checked first |
| `interrupt` | `bool` | `false` | Whether to check every frame (interrupt current step execution) |

### Input/Output Mapping

`input` injects variables into script parameters, `output` writes script results back to variables:

```json
{
  "input": { "current_hp": "$variables.hp" },
  "output": { "$variables.hp": "$result.new_hp" }
}
```

---

## Condition System

Conditions are used in Transitions and Triggers to determine flow direction.

### Condition Types

| type | Fields | Description |
|------|--------|-------------|
| `"always"` | — | Always true |
| `"template"` | `template`, `threshold?`, `roi?` | Template matching |
| `"ocr"` | `expected`, `roi?` | OCR text detection |
| `"color"` | `x`, `y`, `color`, `tolerance?` | Pixel color match |
| `"variable"` | `key`, `op`, `value` | Variable comparison |
| `"hotkey"` | `key` | Hotkey pressed |
| `"script"` | `script` | Script returns true/false |
| `"and"` | `conditions` | All sub-conditions are true |
| `"or"` | `conditions` | Any sub-condition is true |
| `"not"` | `condition` | Negation |

### Comparison Operators (variable condition)

| op | Description |
|----|-------------|
| `"eq"` | Equal |
| `"ne"` | Not equal |
| `"gt"` | Greater than |
| `"lt"` | Less than |
| `"gte"` | Greater than or equal |
| `"lte"` | Less than or equal |
| `"in"` | Value is in array |
| `"contains"` | String contains substring / array contains element |

### Examples

```json
{"type": "always"}

{"type": "template", "template": "btn.png", "threshold": 0.9}
{"type": "template", "template": "btn.png", "roi": {"x": 0, "y": 0, "width": 100, "height": 50}}

{"type": "ocr", "expected": "Start Game", "roi": {"x": 100, "y": 200, "width": 300, "height": 50}}

{"type": "color", "x": 500, "y": 300, "color": "#FF0000", "tolerance": 20}

{"type": "variable", "key": "$variables.hp", "op": "gt", "value": 0}

{"type": "hotkey", "key": "F12"}

{"type": "and", "conditions": [
  {"type": "template", "template": "ready.png"},
  {"type": "variable", "key": "$variables.flag", "op": "eq", "value": true}
]}

{"type": "not", "condition": {"type": "template", "template": "error.png"}}
```

---

## Variable System

### Variable Definition

```json
{
  "variables": {
    "hp": {
      "value_type": "integer",
      "default": 100,
      "persist": true
    },
    "username": {
      "value_type": "string",
      "default": "",
      "persist": false
    }
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `value_type` | `string` | `"integer"` / `"number"` / `"string"` / `"boolean"` / `"object"` / `"array"` |
| `default` | `any` | Default value |
| `persist` | `bool` | Whether to persist to disk |
| `schema` | `object` | JSON Schema (optional) |

### Variable References

Use the `$` prefix in `input`, `output`, and `Condition`:

| Prefix | Example | Description |
|--------|---------|-------------|
| `$variables.` | `$variables.hp` | Value from variable store |
| `$result.` | `$result.new_hp` | Current step output field |
| `$flow_output.` | `$flow_output.success` | Sub-flow output field |
| `$steps.` | `$steps.detect.result.hp` | Cached output of a specific step |

---

## Best Practices

### 1. Template Image Management

```
my-script/
├── templates/
│   ├── button_start.png
│   ├── button_confirm.png
│   └── dialog_error.png
```

- Use descriptive filenames
- PNG format, same resolution as game screenshots
- For transparent areas, use `useAlphaMask: true`
- For green screen assets, use `greenMask: true`

### 2. Use ROI to Speed Up Matching

```javascript
// Bad: Full screen search
const match = await ctx.findTemplate("small_icon.png");

// Good: Limit search area
const match = await ctx.findTemplate("small_icon.png", {
  roi: { x: 100, y: 200, width: 300, height: 200 }
});
```

### 3. Batch Matching Instead of Loops

```javascript
// Bad: Multiple screenshots
for (const name of ["a.png", "b.png", "c.png"]) {
  const m = await ctx.findTemplate(name);  // Takes a new screenshot each time
}

// Good: Single-frame batch matching
const results = await ctx.findTemplateBatch([
  { name: "a.png", threshold: 0.9 },
  { name: "b.png", threshold: 0.85 },
  { name: "c.png" }
]);
```

### 4. Use Appropriate Wait Methods

```javascript
// Time wait (for fixed delays)
await ctx.sleep(1000);

// Frame wait (for syncing with game frame rate)
await ctx.sleepFrames(10);

// Condition wait (poll until condition met)
const match = await ctx.waitForTemplate("dialog.png", 5000, { threshold: 0.9 });
if (!match) {
  ctx.logWarn("Wait timed out");
  return;
}
```

### 5. Graceful Cancellation

```javascript
async function start() {
  for (let i = 0; i < 100; i++) {
    if (ctx.isCancelled()) {
      ctx.logInfo("User cancelled");
      return;
    }
    // ... work logic
    ctx.progress(i, 100);
    await ctx.sleep(500);
  }
}
```

### 6. Error Handling

```javascript
async function start() {
  try {
    const match = await ctx.findTemplate("required.png");
    if (!match) {
      ctx.logError("Required element not found");
      return;
    }
    await ctx.click(match.x, match.y);
  } catch (e) {
    ctx.logError("Execution error: " + e);
  }
}
```

### 7. Use Storage to Persist State

```javascript
async function start() {
  // Read last count
  let count = (await ctx.storageGet("count")) || 0;

  for (let i = 0; i < 10; i++) {
    count++;
    // Save periodically
    if (count % 5 === 0) {
      await ctx.storageSet("count", count);
    }
  }

  await ctx.storageSet("count", count);
}
```

---

## Example Scripts

### Example 1: Simple Click Task

```json
{
  "schema_version": 1,
  "name": "auto_click",
  "display_name": "Auto Click",
  "version": "1.0.0",
  "type": "solo_task",
  "entry": "main.js",
  "permissions": ["screenshot", "template_match", "click"]
}
```

```javascript
async function start() {
  const match = await ctx.waitForTemplate("target.png", 10000, { threshold: 0.85 });
  if (match) {
    await ctx.click(match.x + match.width / 2, match.y + match.height / 2);
    ctx.logInfo("Click successful");
  } else {
    ctx.logWarn("Target not found");
  }
}
```

### Example 2: Loop Task with Parameters

```json
{
  "schema_version": 1,
  "name": "repeat_task",
  "display_name": "Repeat Task",
  "version": "1.0.0",
  "type": "solo_task",
  "entry": "main.js",
  "permissions": ["screenshot", "template_match", "click"],
  "params_schema": {
    "type": "object",
    "properties": {
      "rounds": { "type": "integer", "title": "Loop Count", "default": 10, "minimum": 1 }
    }
  }
}
```

```javascript
function getParams() {
  return typeof globalThis.config === "object" && globalThis.config !== null
    ? globalThis.config : {};
}

async function start() {
  const rounds = getParams().rounds || 10;

  for (let i = 0; i < rounds; i++) {
    if (ctx.isCancelled()) return;
    ctx.progress(i, rounds);

    const btn = await ctx.waitForTemplate("action_btn.png", 5000);
    if (btn) {
      await ctx.click(btn.x + btn.width / 2, btn.y + btn.height / 2);
      await ctx.sleep(1000);
    } else {
      ctx.logWarn(`Round ${i + 1}: Button not found`);
    }
  }

  ctx.logInfo(`Completed ${rounds} rounds`);
}
```

### Example 3: Trigger Script

```json
{
  "schema_version": 1,
  "name": "auto_confirm",
  "display_name": "Auto Confirm Dialog",
  "version": "1.0.0",
  "type": "trigger",
  "entry": "main.js",
  "permissions": ["screenshot", "template_match", "click"]
}
```

```javascript
function onCapture(frame) {
  // Check for confirmation dialog every frame
  const btn = ctx.findTemplate("confirm_btn.png", { threshold: 0.9 });
  // Note: findTemplate in triggers returns a Promise, use then
  btn.then(m => {
    if (m) {
      ctx.click(m.x + m.width / 2, m.y + m.height / 2);
    }
  });
}

// Or use async version
async function onTrigger(ctx) {
  const btn = await ctx.findTemplate("confirm_btn.png", { threshold: 0.9 });
  if (btn) {
    await ctx.click(btn.x + btn.width / 2, btn.y + btn.height / 2);
  }
}
```

### Example 4: Library Script

```json
{
  "schema_version": 1,
  "name": "utils",
  "display_name": "Utilities",
  "version": "1.0.0",
  "type": "library",
  "entry": "main.js",
  "permissions": ["screenshot", "template_match", "click", "ocr"]
}
```

```javascript
registerLibrary("clickIfFound", async function(args) {
  const match = await ctx.findTemplate(args.template, {
    threshold: args.threshold || 0.8,
    roi: args.roi
  });
  if (match) {
    await ctx.click(match.x + match.width / 2, match.y + match.height / 2);
    return true;
  }
  return false;
});

registerLibrary("waitForAndClick", async function(args) {
  const match = await ctx.waitForTemplate(args.template, args.timeout || 5000, {
    threshold: args.threshold || 0.8
  });
  if (match) {
    await ctx.click(match.x + match.width / 2, match.y + match.height / 2);
    return true;
  }
  return false;
});
```

---

## Appendix: Engine Limits

| Item | Limit |
|------|-------|
| Memory limit | 128 MB |
| Stack size | 4 MB |
| Max execution time | 24 hours |
| Cancel check interval | 50 ms |
