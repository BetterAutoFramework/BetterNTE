# 脚本开发指南

> **[简体中文](scripting-guide.md)** | [English](scripting-guide_EN.md)

本文档面向为 BetterNTE 编写自动化脚本的开发者。BetterNTE 使用 JavaScript（QuickJS 引擎）作为脚本语言，通过 `ctx` 全局对象调用引擎能力。

> **注意**: 项目目前处于初始开发阶段，API 可能存在大量改动。本文档中的方法签名、参数和返回值可能随版本更新而变化，请以实际运行的引擎版本为准。

---

## 目录

- [快速开始](#快速开始)
- [脚本目录结构](#脚本目录结构)
- [manifest.json 完整参考](#manifestjson-完整参考)
- [脚本生命周期](#脚本生命周期)
- [ctx API 完整参考](#ctx-api-完整参考)
  - [截图](#截图)
  - [模板匹配](#模板匹配)
  - [OCR 文字识别](#ocr-文字识别)
  - [颜色检测](#颜色检测)
  - [输入模拟](#输入模拟)
  - [等待](#等待)
  - [窗口管理](#窗口管理)
  - [分辨率信息](#分辨率信息)
  - [脚本间调用](#脚本间调用)
  - [异步任务（Job）](#异步任务job)
  - [存储](#存储脚本级-kv-存储)
  - [文件操作](#文件操作脚本级)
  - [网络请求](#网络请求)
  - [通知](#通知)
  - [日志](#日志)
  - [状态与进度](#状态与进度)
- [数据类型](#数据类型)
- [权限系统](#权限系统)
- [用户参数](#用户参数)
- [库脚本](#库脚本)
- [Flow 定义](#flow-定义)
- [条件系统](#条件系统)
- [变量系统](#变量系统)
- [最佳实践](#最佳实践)
- [示例脚本](#示例脚本)

---

## 快速开始

### 最小脚本

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
  "display_name": "我的脚本",
  "version": "1.0.0",
  "type": "solo_task",
  "entry": "main.js",
  "permissions": ["screenshot", "template_match", "click"]
}
```

**main.js:**
```javascript
async function start() {
  ctx.logInfo("脚本开始运行");

  const match = await ctx.findTemplate("button.png", { threshold: 0.9 });
  if (match) {
    await ctx.click(match.x + match.width / 2, match.y + match.height / 2);
  }

  ctx.logInfo("脚本执行完毕");
  return "done";
}
```

将脚本目录放入 `data/main/scripts/`，重启引擎即可在客户端中看到并运行。

---

## 脚本目录结构

```
data/main/scripts/
├── my-task/
│   ├── manifest.json       # 脚本元数据（必需）
│   ├── main.js             # 入口文件（必需）
│   ├── templates/          # 模板图片目录
│   │   ├── button.png
│   │   └── dialog.png
│   └── store/              # 持久化文件（自动创建，无需声明）
│       └── config.json
├── my-trigger/
│   ├── manifest.json
│   ├── main.js
│   └── templates/
└── lib/
    ├── manifest.json
    └── main.js
```

**目录规则:**
- `templates/` 目录中的图片通过文件名（不含扩展名）引用，如 `ctx.findTemplate("button.png")`
- `store/` 目录用于脚本级文件持久化，通过 `ctx.readStoreFile()` / `ctx.writeStoreFile()` 访问
- 引擎自动跳过 `templates/` 和 `store/` 目录，不会将它们当作子脚本扫描

---

## manifest.json 完整参考

### 必填字段

| 字段 | 类型 | 说明 |
|------|------|------|
| `schema_version` | `number` | 必须为 `1` |
| `name` | `string` | 唯一标识符，英文 snake_case，如 `"fishing_assist"` |
| `display_name` | `string` | 显示名称，可中文，如 `"钓鱼辅助"` |
| `version` | `string` | 语义化版本，如 `"1.0.0"` |
| `type` | `string` | 脚本类型，见下表 |
| `entry` | `string` | 入口文件名，如 `"main.js"` |

### 可选字段

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `author` | `string` | `""` | 作者 |
| `description` | `string` | `""` | 描述 |
| `icon` | `string` | `null` | 图标文件路径 |
| `permissions` | `string[]` | `[]` | 权限声明，见[权限系统](#权限系统) |
| `tags` | `string[]` | `[]` | 标签，用于分类 |
| `category` | `string` | `null` | 分类 |
| `params_schema` | `object` | `null` | JSON Schema，定义用户可配置参数 |
| `dependencies` | `object[]` | `[]` | 依赖的库脚本 |
| `engine_version` | `string` | `null` | 引擎版本范围，如 `"^0.0.1"` |
| `min_engine_version` | `string` | `null` | 最低引擎版本 |
| `max_engine_version` | `string` | `null` | 最高引擎版本（不含） |
| `design_resolution` | `[number, number]` | `null` | 设计分辨率 `[width, height]`，见[分辨率适配](#分辨率适配) |

### type 取值

| 值 | 别名 | 说明 |
|----|------|------|
| `"solo_task"` | `"task"`, `"flow"` | 任务脚本 |
| `"trigger"` | — | 帧触发器，每帧执行 |
| `"library"` | — | 可复用库，不能直接运行 |

### 依赖声明

```json
{
  "dependencies": [
    { "path": "main/scripts/lib" }
  ]
}
```

`path` 为相对于 data 根目录的 POSIX 路径，不能包含 `..`。

### 完整示例

```json
{
  "schema_version": 1,
  "name": "cafe_income",
  "display_name": "领取一咖舍收益",
  "version": "1.0.0",
  "type": "solo_task",
  "entry": "main.js",
  "author": "BetterNTE",
  "description": "进入一咖舍、领取、确认、关闭；可选补货链。",
  "tags": ["异环", "都市大亨", "一咖舍"],
  "permissions": ["screenshot", "template_match", "click", "keyboard", "color_detect", "ocr"],
  "dependencies": [{ "path": "main/scripts/lib" }],
  "params_schema": {
    "type": "object",
    "properties": {
      "enable_restock": { "type": "boolean", "title": "自动补货", "default": true },
      "max_rounds": { "type": "integer", "title": "最大轮询轮数", "default": 25, "minimum": 5, "maximum": 100 },
      "enter_key": { "type": "string", "title": "进入按键", "default": "f5" },
      "exit_key": { "type": "string", "title": "退出按键", "default": "esc" }
    }
  }
}
```

---

## 脚本生命周期

引擎按以下顺序调用脚本中的全局函数（如果存在）。所有生命周期函数均为可选，只需定义需要的函数即可。

> **当前状态**: `start()`（或 `main()`）是唯一必需的入口函数。其余生命周期钩子（`init`、`stop`、`destroy`、`onEnable`、`onDisable`）引擎均已支持调用，但目前大部分脚本仅使用 `start()`。

### solo_task 类型

```
init()  →  start()  →  stop()  →  destroy()
```

| 阶段 | 函数 | 说明 |
|------|------|------|
| 加载 | `init()` | 源码加载后调用一次，用于初始化 |
| 运行 | `async start()` | 用户触发运行时调用，必须返回一个值。也可用 `main()` 代替 |
| 停止 | `stop()` | 用户停止或脚本取消时调用 |
| 卸载 | `destroy()` | 脚本从引擎中移除时调用 |

### trigger 类型

```
init()  →  onEnable(params)  →  onCapture(frame) / onTrigger(ctx) [每帧]  →  onDisable()  →  destroy()
```

| 阶段 | 函数 | 说明 |
|------|------|------|
| 加载 | `init()` | 同上 |
| 启用 | `onEnable(params)` | 触发器启用时调用 |
| 帧回调 | `onCapture({width, height})` | 每帧调用，`frame` 为当前帧信息 |
| 帧回调 | `onTrigger(ctx)` | 替代 `onCapture`，直接接收 ctx |
| 禁用 | `onDisable()` | 触发器禁用时调用 |
| 卸载 | `destroy()` | 同上 |

> 引擎优先查找 `onTrigger`，未找到时回退到 `onCapture`。

### library 类型

库脚本没有生命周期函数，通过 `registerLibrary()` 注册可调用函数。

---

## ctx API 完整参考

所有异步方法返回 `Promise`，必须使用 `await`。同步方法直接返回值。

### 截图

需要权限: `screenshot`

```javascript
// 截取全屏或目标窗口
const frame = await ctx.capture(force?);
// 返回: { width, height, data_len }

// 截取全屏或目标窗口的指定区域
const region = await ctx.captureRegion(x, y, w, h, force?);
// 返回: { width, height }

// 保存当前帧为 PNG 到脚本 store 目录
const path = await ctx.saveScreenshot(force?);
// 返回: string（保存的文件路径）

// 别名
const path = await ctx.screenshot(force?);
```

`force` 为 `true` 时强制刷新帧缓存，默认 `false`（复用最近一帧）。

### 模板匹配

需要权限: `template_match`

```javascript
// 查找单个模板（返回第一个匹配或 null）
const match = await ctx.findTemplate(name, opts?);
// 返回: { x, y, width, height, confidence } 或 null

// 查找所有匹配
const matches = await ctx.findTemplates(name, opts?);
// 返回: MatchResult[]

// 批量查找（单帧多模板，性能更优）
const results = await ctx.findTemplateBatch(entries);
// entries: [{ name, threshold?, roi?, ... }]
// 返回: (MatchResult | null)[]

// 等待模板出现
const match = await ctx.waitForTemplate(name, timeoutMs, opts?);
// 超时返回 null

// 等待模板消失
const gone = await ctx.waitGone(name, timeoutMs);
// 返回: bool

// 帧计数版本（更精确）
const match = await ctx.waitForTemplateFrames(name, maxFrames, opts?);
const gone = await ctx.waitGoneFrames(name, maxFrames);

// 别名
const match = await ctx.matchTemplate(name, opts?);
```

**opts 参数:**

```javascript
{
  threshold: 0.8,           // 匹配阈值，0.0-1.0，默认 0.8
  roi: { x, y, width, height }, // 感兴趣区域，限制搜索范围
  orderBy: "score",         // 排序方式: "score" | "horizontal" | "vertical" | "area" | "random"
  resultIndex: 0,           // 返回第几个结果（支持负数，-1 = 最后一个）
  nmsThreshold: 0.3,        // 非极大值抑制 IoU 阈值
  maxResults: 10,           // 最大结果数（仅 findTemplates）
  greenMask: false,         // 排除 #00FF00 像素（绿幕抠图）
  greenMaskTolerance: 0,    // 绿色容差
  useAlphaMask: false,      // 排除透明像素（PNG 透明区域）
  alphaMaskThreshold: 8,    // Alpha 阈值
  grayscale: false          // true = 灰度匹配（更快），false = 彩色匹配（更精确），默认 false
}
```

### OCR 文字识别

需要权限: `ocr`

```javascript
// 识别指定区域的文字
const text = await ctx.ocr(x, y, w, h);
// 返回: string

// 识别指定区域，过滤特定颜色的文字
const text = await ctx.ocr(x, y, w, h, { textColor: "#DCDCDC", textColorTolerance: 32 });
// textColor: 目标文字颜色（#RRGGBB），只保留接近该颜色的像素
// textColorTolerance: 颜色容差，0-255，默认 32

// 识别全屏所有文字
const results = await ctx.ocrAll();
// 返回: [{ text, region: { x, y, width, height }, confidence }]
```

### 颜色检测

需要权限: `color_detect`

```javascript
// 获取像素颜色
const color = await ctx.getColor(x, y);
// 返回: "#RRGGBB"

// 判断像素是否匹配颜色
const matched = await ctx.colorMatch(x, y, "#FF0000", tolerance);
// 返回: bool

// 批量颜色匹配
const result = await ctx.colorMatchAll(points, opts?);
// points: [{ x, y, color, tolerance?, rgbaTolerance? }]
// 返回: bool（正常模式）或 ColorMatchAllResult（debug 模式）

// 统计区域内匹配颜色的像素数量
const count = await ctx.countColor("#FF0000", { tolerance: 10, roi: { x: 0, y: 0, width: 1920, height: 1080 } });
// tolerance 默认 0，roi 可选（不传则全屏统计）
// 返回: number（匹配像素数）

// 等待颜色出现
const found = await ctx.waitForColor(x, y, "#FF0000", timeoutMs);
const found = await ctx.waitForColorFrames(x, y, "#FF0000", maxFrames);

// 扫描水平条带（如体力条 + 玩家标记）
const result = await ctx.scanSliderStrip({
  region: { x: 0, y: 0, width: 1920, height: 100 },
  barColor: "#FF0000",
  playerColor: "#00FF00",
  barTolerance: 28,       // 可选，默认 28
  playerTolerance: 28,    // 可选，默认 28
  stepX: 2,               // 可选，默认 2
  rowOffset: 50,          // 可选，默认 height/2
  minBarRunPx: 18,        // 可选，默认 18
  minPlayerRunPx: 6       // 可选，默认 6
});
// 返回: { barLeft, barRight, playerCenter } 或 null

// 扫描条带边缘（左右边界 + 玩家位置）
const edges = await ctx.scanStripEdges({
  region: { x: 0, y: 0, width: 1920, height: 100 },
  barColor: "#FF0000",
  playerColor: "#00FF00",
  barTolerance: 28,
  playerTolerance: 28,
  stepX: 2,
  rowOffset: 50
});
// 返回: { barLeft, barRight, playerLeft, playerRight }
```

**colorMatchAll opts:**

```javascript
{
  defaultTolerance: 32,     // 默认容差
  debug: false,             // 调试模式，返回详细匹配信息
  shiftMax: {               // 允许的偏移搜索范围
    maxDx: 5,
    maxDy: 5
  }
}
```

**rgbaTolerance（逐通道容差）:**

```javascript
{ r: 10, g: 10, b: 10, a: 255 }
```

### 输入模拟

需要权限: `click`

```javascript
// 鼠标
await ctx.click(x, y);                     // 左键点击
await ctx.doubleClick(x, y);               // 双击
await ctx.rightClick(x, y);                // 右键点击
await ctx.mouseMove(x, y);                 // 移动鼠标
await ctx.mouseDown("left");               // 按下 ("left" | "right" | "middle")
await ctx.mouseUp("left");                 // 释放
await ctx.scroll(delta);                   // 滚轮（正=上，负=下）
await ctx.swipe(x1, y1, x2, y2, durationMs); // 滑动

// 键盘
await ctx.keyDown("a");                    // 按下
await ctx.keyUp("a");                      // 释放
await ctx.keyPress("enter");               // 按下并释放
await ctx.keyPress("a", 100);              // 按住 100ms 后释放
await ctx.keyCombo(["ctrl", "c"]);         // 组合键
await ctx.typeText("hello");               // 输入文本
```

**按键名称:** 使用标准按键名，如 `"enter"`, `"esc"`, `"space"`, `"tab"`, `"f1"`-`"f12"`, `"a"`-`"z"`, `"0"`-`"9"`, `"ctrl"`, `"alt"`, `"shift"` 等。

### 等待

```javascript
// 等待指定毫秒
await ctx.sleep(1000);

// 等待指定帧数
await ctx.sleepFrames(10);
```

`sleep` 和 `sleepFrames` 不需要权限。

### 窗口管理

需要权限: `window`

```javascript
// 查找窗口
const hwnd = await ctx.findWindow("游戏窗口标题");
// 返回: u64 窗口句柄，未找到返回 null

// 激活窗口
await ctx.activateWindow(hwnd);

// 获取窗口区域
const rect = await ctx.getWindowRect(hwnd);
// 返回: { x, y, width, height }

// 获取屏幕尺寸
const [width, height] = await ctx.getScreenSize();
```

`getScreenSize` 需要 `screenshot` 权限。

### 分辨率信息

不需要权限。

```javascript
// 获取缩放因子（设计分辨率 → 实际分辨率）
const scale = await ctx.getScaleFactors();
// 返回: { scaleX: 1.333, scaleY: 1.333 } 或 null（未设置 design_resolution 时）

// 获取当前实际帧尺寸
const size = await ctx.getFrameSize();
// 返回: { width: 2560, height: 1440 } 或 null
```

### 分辨率适配

当游戏运行分辨率与模板图片分辨率不一致时，通过 `design_resolution` 让引擎自动缩放帧到设计分辨率，所有视觉操作（模板匹配、OCR、颜色检测）在缩放后的帧上执行，输入操作（点击、滑动）自动反向缩放到实际屏幕坐标。

```json
{
  "design_resolution": [1920, 1080]
}
```

设置后：
- 模板图片按 1920×1080 制作即可，引擎自动适配实际分辨率
- `ctx.click(960, 540)` 会自动映射到实际屏幕中心
- `ctx.findTemplate` 返回的坐标是设计分辨率空间，可直接用于 `ctx.click`
- 不设置时行为与之前完全一致（向后兼容）

### 脚本间调用

```javascript
// 运行另一个脚本
const result = await ctx.runScript("other-script", { key: "value" });

// 调用库函数
const result = await ctx.call("lib", "functionName", { arg1: "value" });
```

需要权限: `call_script` / `call_library`

### 异步任务（Job）

`post*` 系列方法将操作提交为后台异步任务，立即返回 job ID，不阻塞脚本执行。适用于需要并行操作或避免长时间阻塞的场景。

```javascript
// 提交异步操作，返回 job ID（number）
const jobId = await ctx.postCapture(force?);
const jobId = await ctx.postClick(x, y);
const jobId = await ctx.postSwipe(x1, y1, x2, y2, durationMs);
const jobId = await ctx.postKeyPress(key, durationMs?);
const jobId = await ctx.postOcr(x, y, w, h, { textColor?, textColorTolerance? });
const jobId = await ctx.postFindTemplate(name, opts?);
const jobId = await ctx.postWaitForTemplate(name, timeoutMs, opts?);

// 等待 job 完成（阻塞直到完成/失败/取消）
const result = await ctx.jobWait(jobId);

// 查询 job 状态（不阻塞）
const status = ctx.jobStatus(jobId);   // "pending" | "running" | "done" | "failed" | "cancelled"
const result = ctx.jobResult(jobId);   // 完成时的结果值
const error = ctx.jobError(jobId);     // 失败时的错误信息

// 取消 job
const cancelled = ctx.jobCancel(jobId);

// 清理已完成的 job（保留最近 N 个）
ctx.jobGc(10);
```

**示例: 并行 OCR + 模板匹配**

```javascript
const [ocrJob, tplJob] = await Promise.all([
  ctx.postOcr(100, 200, 300, 50),
  ctx.postFindTemplate("button.png", { roi: { x: 100, y: 200, width: 300, height: 50 } })
]);
const text = await ctx.jobWait(ocrJob);
const match = await ctx.jobWait(tplJob);
```

### 存储（脚本级 KV 存储）

不需要额外权限，数据存储在 `{脚本目录}/storage.json`。

```javascript
await ctx.storageSet("count", 42);
const count = await ctx.storageGet("count");    // 42 或 null
await ctx.storageDelete("count");
const keys = await ctx.storageKeys();            // ["key1", "key2", ...]
```

值可以是任意 JSON 类型（string、number、boolean、object、array、null）。

### 文件操作（脚本级）

不需要额外权限，路径相对于 `{脚本目录}/store/`。

```javascript
await ctx.writeStoreFile("data.json", '{"a":1}');
const content = await ctx.readStoreFile("data.json");
const files = await ctx.listStoreFiles(".");
```

### 文件操作（系统级）

需要权限: `file`

```javascript
const content = await ctx.readFile("/path/to/file");
await ctx.writeFile("/path/to/file", "content");
const files = await ctx.listFiles("/path/to/dir");
const exists = await ctx.fileExists("/path/to/file");
```

### 网络请求

需要权限: `network`

```javascript
const response = await ctx.httpGet("https://api.example.com/data");
const response = await ctx.httpPost("https://api.example.com/submit", '{"key":"value"}');
```

### 通知

需要权限: `notify`

```javascript
await ctx.notify("标题", "内容");
```

通过配置的通知通道（ServerChan、Telegram、Bark、Webhook）发送推送。

### 日志

不需要权限，同步调用。

```javascript
ctx.logDebug("调试信息");
ctx.logInfo("普通信息");
ctx.logWarn("警告信息");
ctx.logError("错误信息");
ctx.log("info", "自定义级别");  // level: "debug" | "info" | "warn" | "error"

// console 全局对象也可用
console.log("等同于 ctx.logInfo");
console.warn("等同于 ctx.logWarn");
console.error("等同于 ctx.logError");
```

### 状态与进度

不需要权限，同步调用。

```javascript
// 检查是否被取消
if (ctx.isCancelled()) return;

// 报告进度（显示在客户端 UI）
ctx.progress(current, total);  // 例如 ctx.progress(3, 10)

// 获取当前 FPS
const fps = ctx.getFps();      // 默认 60

// 获取当前帧号
const frame = ctx.getFrameNumber();
```

---

## 数据类型

### MatchResult

模板匹配结果。

```javascript
{
  x: 100,           // i32 - 匹配区域左上角 X
  y: 200,           // i32 - 匹配区域左上角 Y
  width: 50,        // u32 - 匹配区域宽度
  height: 30,       // u32 - 匹配区域高度
  confidence: 0.95  // f64 - 匹配置信度 0.0-1.0
}
```

**点击匹配区域中心:**
```javascript
const m = await ctx.findTemplate("btn.png");
if (m) {
  await ctx.click(m.x + m.width / 2, m.y + m.height / 2);
}
```

### Region / ROI

感兴趣区域，用于限制搜索范围。

```javascript
{ x: 0, y: 0, width: 1920, height: 1080 }
```

### OcrResult

OCR 识别结果。

```javascript
{
  text: "开始游戏",        // string - 识别文字
  region: { x, y, width, height },  // 区域
  confidence: 0.88         // f64 - 置信度
}
```

### CaptureFrame

截图帧信息（JS 侧只能获取元数据，不能访问像素数据）。

```javascript
{ width: 1920, height: 1080, data_len: 8294400 }
```

---

## 权限系统

在 `manifest.json` 的 `permissions` 数组中声明脚本需要的权限。未声明权限的 API 调用将被拒绝。

### 权限列表

| 权限 | 可用 API |
|------|----------|
| `screenshot` | `capture`, `captureRegion`, `saveScreenshot`(`screenshot`), `getScreenSize` |
| `template_match` | `findTemplate`(`matchTemplate`), `findTemplates`, `findTemplateBatch`, `waitForTemplate`, `waitGone`, `waitForTemplateFrames`, `waitGoneFrames`, `postFindTemplate`, `postWaitForTemplate` |
| `ocr` | `ocr`, `ocrAll`, `postOcr` |
| `color_detect` | `getColor`, `colorMatch`, `colorMatchAll`, `countColor`, `scanSliderStrip`, `scanStripEdges`, `waitForColor`, `waitForColorFrames` |
| `click` | `click`, `doubleClick`, `rightClick`, `mouseMove`, `mouseDown`, `mouseUp`, `scroll`, `swipe`, `keyDown`, `keyUp`, `keyPress`, `keyCombo`, `typeText`, `postClick`, `postSwipe`, `postKeyPress` |
| `keyboard` | 同 `click`（别名） |
| `window` | `findWindow`, `activateWindow`, `getWindowRect` |
| `call_script` | `runScript` |
| `call_library` | `call` |
| `notify` | `notify` |
| `storage` | `readStoreFile`, `writeStoreFile`, `listStoreFiles`, `storageGet`, `storageSet`, `storageDelete`, `storageKeys` |
| `file` | `readFile`, `writeFile`, `listFiles`, `fileExists` |
| `network` | `httpGet`, `httpPost` |

### 无需权限的方法

`sleep`, `sleepFrames`, `isCancelled`, `progress`, `getFps`, `getFrameNumber`, `getScaleFactors`, `getFrameSize`, `log*`, `console.*`, `jobStatus`, `jobResult`, `jobError`, `jobCancel`, `jobGc`, `jobWait`

### 示例

```json
{
  "permissions": ["screenshot", "template_match", "click", "ocr"]
}
```

---

## 用户参数

通过 `params_schema` 定义用户可配置的参数，客户端会自动生成设置表单。

### 定义参数

```json
{
  "params_schema": {
    "type": "object",
    "properties": {
      "count": {
        "type": "integer",
        "title": "循环次数",
        "default": 999,
        "minimum": 1,
        "maximum": 9999
      },
      "enable_feature": {
        "type": "boolean",
        "title": "启用功能",
        "default": true
      },
      "mode": {
        "type": "string",
        "title": "运行模式",
        "enum": ["fast", "normal", "slow"],
        "default": "normal"
      }
    }
  }
}
```

### 读取参数

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

参数通过 `globalThis.config` 注入，也可以通过 `ctx.params` 访问。

---

## 库脚本

库脚本（`type: "library"`）用于封装可复用逻辑，不能直接运行。

### 注册函数

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

### 使用库

在任务脚本的 `manifest.json` 中声明依赖：

```json
{
  "dependencies": [{ "path": "main/scripts/lib" }]
}
```

在代码中调用：

```javascript
const roi = await ctx.call("lib", "roi", { x: 100, y: 200, width: 300, height: 400 });
const found = await ctx.call("lib", "findFirst", { names: ["a.png", "b.png"], threshold: 0.9 });
```

### 库函数特性

- 可以是 `async` 函数
- 可以调用 `ctx.*` 方法（需要库自身的 manifest 声明对应权限）
- 参数和返回值为 JSON 格式
- 使用 `registerLibrary(name, fn)` 注册，也可是 `globalThis.registerLibrary`

---

## Flow 定义

Flow 是有向图式的任务编排，由步骤（Step）和转换（Transition）组成。

### Flow JSON 结构

```json
{
  "id": "my-flow",
  "name": "我的流程",
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

### Step 类型

| type | 字段 | 说明 |
|------|------|------|
| `"script"` | `script` | 执行 JS 脚本 |
| `"click"` | `x`, `y` | 点击坐标 |
| `"swipe"` | `x1`, `y1`, `x2`, `y2`, `duration_ms` | 滑动手势 |
| `"key_press"` | `key` | 按键 |
| `"wait"` | `ms` | 等待毫秒 |
| `"flow"` | `flow` | 嵌套子流程 |
| `"group"` | `group` | 调用任务组 |
| `"set_variable"` | `key`, `value` | 设置变量 |
| `"none"` | — | 空操作（纯分支节点） |

### Transition 字段

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `target` | `string` | — | 目标步骤 ID |
| `condition` | `Condition` | — | 转换条件 |
| `priority` | `u8` | `0` | 优先级，越高越先检查 |
| `interrupt` | `bool` | `false` | 是否每帧检查（中断当前步骤执行） |

### 输入/输出映射

`input` 将变量注入脚本参数，`output` 将脚本结果写回变量：

```json
{
  "input": { "current_hp": "$variables.hp" },
  "output": { "$variables.hp": "$result.new_hp" }
}
```

---

## 条件系统

条件用于 Transition 和 Trigger 中，决定流程走向。

### 条件类型

| type | 字段 | 说明 |
|------|------|------|
| `"always"` | — | 始终为 true |
| `"template"` | `template`, `threshold?`, `roi?` | 模板匹配 |
| `"ocr"` | `expected`, `roi?` | OCR 文字检测 |
| `"color"` | `x`, `y`, `color`, `tolerance?` | 像素颜色匹配 |
| `"variable"` | `key`, `op`, `value` | 变量比较 |
| `"hotkey"` | `key` | 热键按下 |
| `"script"` | `script` | 脚本返回 true/false |
| `"and"` | `conditions` | 所有子条件为 true |
| `"or"` | `conditions` | 任一子条件为 true |
| `"not"` | `condition` | 取反 |

### 比较操作符（variable 条件）

| op | 说明 |
|----|------|
| `"eq"` | 等于 |
| `"ne"` | 不等于 |
| `"gt"` | 大于 |
| `"lt"` | 小于 |
| `"gte"` | 大于等于 |
| `"lte"` | 小于等于 |
| `"in"` | 值在数组中 |
| `"contains"` | 字符串包含子串 / 数组包含元素 |

### 示例

```json
{"type": "always"}

{"type": "template", "template": "btn.png", "threshold": 0.9}
{"type": "template", "template": "btn.png", "roi": {"x": 0, "y": 0, "width": 100, "height": 50}}

{"type": "ocr", "expected": "开始游戏", "roi": {"x": 100, "y": 200, "width": 300, "height": 50}}

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

## 变量系统

### 变量定义

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

| 字段 | 类型 | 说明 |
|------|------|------|
| `value_type` | `string` | `"integer"` / `"number"` / `"string"` / `"boolean"` / `"object"` / `"array"` |
| `default` | `any` | 默认值 |
| `persist` | `bool` | 是否持久化到磁盘 |
| `schema` | `object` | JSON Schema（可选） |

### 变量引用

在 `input`、`output`、`Condition` 中使用 `$` 前缀引用：

| 前缀 | 示例 | 说明 |
|------|------|------|
| `$variables.` | `$variables.hp` | 变量存储中的值 |
| `$result.` | `$result.new_hp` | 当前步骤输出字段 |
| `$flow_output.` | `$flow_output.success` | 子流程输出字段 |
| `$steps.` | `$steps.detect.result.hp` | 指定步骤的缓存输出 |

---

## 最佳实践

### 1. 模板图片管理

```
my-script/
├── templates/
│   ├── button_start.png
│   ├── button_confirm.png
│   └── dialog_error.png
```

- 使用描述性文件名
- PNG 格式，保持与游戏截图相同的分辨率
- 对于透明区域，使用 `useAlphaMask: true`
- 对于绿幕素材，使用 `greenMask: true`
- 彩色匹配（默认）精度更高；灰度匹配（`grayscale: true`）速度更快，适合对性能敏感的场景

### 2. 使用 ROI 加速匹配

```javascript
// 不好：全屏搜索
const match = await ctx.findTemplate("small_icon.png");

// 好：限定搜索区域
const match = await ctx.findTemplate("small_icon.png", {
  roi: { x: 100, y: 200, width: 300, height: 200 }
});
```

### 3. 批量匹配代替循环

```javascript
// 不好：多次截图
for (const name of ["a.png", "b.png", "c.png"]) {
  const m = await ctx.findTemplate(name);  // 每次重新截图
}

// 好：单帧批量匹配
const results = await ctx.findTemplateBatch([
  { name: "a.png", threshold: 0.9 },
  { name: "b.png", threshold: 0.85 },
  { name: "c.png" }
]);
```

### 4. 合理使用等待

```javascript
// 时间等待（适合固定延迟）
await ctx.sleep(1000);

// 帧等待（适合与游戏帧率同步）
await ctx.sleepFrames(10);

// 条件等待（轮询直到满足条件）
const match = await ctx.waitForTemplate("dialog.png", 5000, { threshold: 0.9 });
if (!match) {
  ctx.logWarn("等待超时");
  return;
}
```

### 5. 优雅取消

```javascript
async function start() {
  for (let i = 0; i < 100; i++) {
    if (ctx.isCancelled()) {
      ctx.logInfo("用户取消");
      return;
    }
    // ... 工作逻辑
    ctx.progress(i, 100);
    await ctx.sleep(500);
  }
}
```

### 6. 错误处理

```javascript
async function start() {
  try {
    const match = await ctx.findTemplate("required.png");
    if (!match) {
      ctx.logError("找不到必要元素");
      return;
    }
    await ctx.click(match.x, match.y);
  } catch (e) {
    ctx.logError("执行出错: " + e);
  }
}
```

### 7. 使用存储持久化状态

```javascript
async function start() {
  // 读取上次的计数
  let count = (await ctx.storageGet("count")) || 0;

  for (let i = 0; i < 10; i++) {
    count++;
    // 定期保存
    if (count % 5 === 0) {
      await ctx.storageSet("count", count);
    }
  }

  await ctx.storageSet("count", count);
}
```

---

## 示例脚本

### 示例 1: 简单点击任务

```json
{
  "schema_version": 1,
  "name": "auto_click",
  "display_name": "自动点击",
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
    ctx.logInfo("点击成功");
  } else {
    ctx.logWarn("未找到目标");
  }
}
```

### 示例 2: 带参数的循环任务

```json
{
  "schema_version": 1,
  "name": "repeat_task",
  "display_name": "重复任务",
  "version": "1.0.0",
  "type": "solo_task",
  "entry": "main.js",
  "permissions": ["screenshot", "template_match", "click"],
  "params_schema": {
    "type": "object",
    "properties": {
      "rounds": { "type": "integer", "title": "循环次数", "default": 10, "minimum": 1 }
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
      ctx.logWarn(`第 ${i + 1} 轮：未找到按钮`);
    }
  }

  ctx.logInfo(`完成 ${rounds} 轮`);
}
```

### 示例 3: 触发器脚本

```json
{
  "schema_version": 1,
  "name": "auto_confirm",
  "display_name": "自动确认弹窗",
  "version": "1.0.0",
  "type": "trigger",
  "entry": "main.js",
  "permissions": ["screenshot", "template_match", "click"]
}
```

```javascript
function onCapture(frame) {
  // 每帧检查是否有确认弹窗
  const btn = ctx.findTemplate("confirm_btn.png", { threshold: 0.9 });
  // 注意：触发器中 findTemplate 返回的是 Promise，需要用 then
  btn.then(m => {
    if (m) {
      ctx.click(m.x + m.width / 2, m.y + m.height / 2);
    }
  });
}

// 或使用 async 版本
async function onTrigger(ctx) {
  const btn = await ctx.findTemplate("confirm_btn.png", { threshold: 0.9 });
  if (btn) {
    await ctx.click(btn.x + btn.width / 2, btn.y + btn.height / 2);
  }
}
```

### 示例 4: 库脚本

```json
{
  "schema_version": 1,
  "name": "utils",
  "display_name": "工具库",
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

## 附录: 引擎限制

| 项目 | 限制 |
|------|------|
| 内存上限 | 128 MB |
| 栈大小 | 4 MB |
| 最大执行时间 | 24 小时 |
| 取消检查间隔 | 50 ms |
