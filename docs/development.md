# 开发者指南

> **[简体中文](development.md)** | [English](development_EN.md)

本文档面向希望参与 BetterNTE 开发或基于其引擎进行二次开发的开发者。

---

## 目录

- [环境要求](#环境要求)
- [项目结构](#项目结构)
- [快速开始](#快速开始)
- [Crate 架构详解](#crate-架构详解)
- [前端架构](#前端架构)
- [Tauri IPC 命令](#tauri-ipc-命令)
- [脚本开发](#脚本开发)
- [Flow Engine](#flow-engine)
- [构建与发布](#构建与发布)
- [调试技巧](#调试技巧)

---

## 环境要求

| 依赖 | 版本要求 | 说明 |
|------|---------|------|
| Rust | edition 2021+ | 通过 [rustup](https://rustup.rs) 安装 |
| Node.js | 22+ | 前端构建 |
| pnpm | 10+ | 包管理器（`npm i -g pnpm`） |
| OpenCV | 4.x | 视觉识别，安装与配置见 [opencv-rust 安装指南](https://github.com/twistedfall/opencv-rust/blob/master/INSTALL.md) |
| ONNX Runtime | — | `ort` crate 自动下载，支持 CUDA / DirectML 加速 |
| Windows SDK | — | Win32 API 调用（截图、输入模拟、叠加层） |

> **提示**: OpenCV 的链接配置是最常见的编译问题。详细安装步骤与环境变量配置请参考 [opencv-rust 安装指南](https://github.com/twistedfall/opencv-rust/blob/master/INSTALL.md)。核心环境变量：`OPENCV_LINK_PATHS`（lib 目录）、`OPENCV_LINK_LIBS`（库名）、`OPENCV_INCLUDE_PATHS`（头文件目录）。

### 模型资源文件

项目运行所需的模型文件（OCR、特征匹配等）不随源码分发，需单独下载：

1. 前往 [BetterNTE ModelScope 文件页](https://modelscope.cn/models/WWWSKY/BetterNTE/files) 下载 `assets.zip`
2. 解压到项目根目录，得到 `assets/` 目录

```
BetterNTE/
├── assets/                  # ← 解压到这里
│   └── models/
│       ├── paddleocr/       # OCR 模型
│       ├── superpoint/      # 特征点检测模型
│       ├── yolo/            # 目标检测模型
│       ├── mobilenet_v3_small-onnx-float/
│       └── test.png
├── crates/
├── data/
└── ...
```

> **注意**: `assets/` 目录已在 `.gitignore` 中排除，不会被提交到仓库。Tauri 构建时会将其作为 bundled resources 打包。

---

## 项目结构

```
BetterNTE/
├── Cargo.toml              # Workspace 根配置
├── Cargo.lock
├── data/                   # 运行时数据（脚本、配置模板等）
│   ├── main/               # 内置脚本
│   └── plugins/            # 插件 manifest
├── crates/
│   ├── betternte-core/     # 基础类型、trait 抽象、配置结构
│   ├── betternte-capture/  # 截图引擎（WGC / DXGI / PrintWindow / ScreenDC / BitBlt）
│   ├── betternte-vision/   # 视觉识别（模板匹配、OCR、颜色检测、轮廓分析）
│   ├── betternte-input/    # 输入模拟（Win32 前后台、ADB 模拟器）
│   ├── betternte-helper/   # 独立工具库（编码、几何、进程、正则）
│   ├── betternte-notify/   # 多通道推送（ServerChan、Telegram、Bark、Webhook）
│   ├── betternte-overlay/  # Win32 叠加层窗口与绘制 API
│   ├── betternte-runtime/  # Flow Engine（Step / Transition / Condition）
│   ├── betternte-script/   # 脚本引擎（QuickJS + async bridge）
│   ├── betternte-engine/   # 引擎门面，组装所有子系统
│   └── betternte-client/   # Tauri 桌面端
│       ├── src-tauri/      # Rust 后端（Tauri commands、引擎接入）
│       ├── src/            # React 前端
│       ├── package.json
│       └── vite.config.ts
└── README.md
```

### 依赖关系

```
betternte-helper              betternte-core
  （无内部依赖）                 （无内部依赖）
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

## 快速开始

```bash
# 1. 克隆仓库
git clone https://github.com/719733328/BetterNTE.git
cd BetterNTE

# 2. 进入客户端目录、安装依赖、启动开发模式
cd crates/betternte-client
pnpm install
pnpm tauri dev
```

> **注意**: `pnpm tauri dev` 必须在 `crates/betternte-client` 目录下执行，不是项目根目录。

首次启动时，Tauri 会自动编译 Rust 后端。后续修改前端代码会触发 HMR 热更新，修改 Rust 代码会自动重新编译。

### 仅编译 Rust 后端

```bash
cargo build                    # debug 模式
cargo build --release          # release 模式
cargo check                    # 仅检查，不生成二进制
```

### 仅运行前端

```bash
cd crates/betternte-client
pnpm dev                       # Vite dev server on localhost:1420
```

---

## Crate 架构详解

### betternte-core — 基础层

所有其他 crate 的公共依赖，定义了整个项目的核心类型和 trait 抽象。

**关键类型:**
- `EngineConfig` — 顶层配置结构，从 `engine.yaml` 反序列化，包含 15+ 子配置（截图、热键、叠加层、通知、API、游戏窗口等）
- `CaptureFrame` — 通用图像容器，支持裁剪、缩放、格式转换、PNG/JPEG/BMP 导出
- `GameWindow` — 游戏窗口信息（句柄、标题、进程、DPI、区域）
- `EngineEvent` — 事件总线事件枚举（TaskStarted、TaskStopped、ScriptLoaded、ConfigChanged、Error、LogMessage）

**关键 trait:**
- `ScreenCapture` — 截图接口
- `InputController` — 输入模拟接口
- `TemplateMatcher` — 模板匹配接口
- `OcrEngine` — OCR 接口
- `ColorDetector` — 颜色检测接口
- `WindowFinder` — 窗口查找接口

> 所有 trait 的具体实现分布在 `betternte-capture`、`betternte-input`、`betternte-vision` 等 crate 中。

---

### betternte-capture — 截图引擎

提供 5 种 Windows 截图后端，无第三方截图库依赖：

| 后端 | API | 特点 |
|------|-----|------|
| `WgcCapture` | Windows Graphics Capture | GPU 加速，持久会话 |
| `DxgiDupCapture` | DXGI Desktop Duplication | GPU，桌面级捕获 |
| `PrintWindowCapture` | GDI PrintWindow | 可捕获被遮挡窗口 |
| `ScreenDCCapture` | Screen DC + BitBlt | GDI，可捕获被遮挡窗口 |
| `BitBltCapture` | GDI BitBlt | 兼容性最好，不能捕获被遮挡窗口 |

**工厂函数:**
- `create_capture_engine()` — 根据配置自动选择后端
- `resolve_auto_capture_method()` — 探测当前系统可用的最佳截图方式

---

### betternte-vision — 视觉识别

基于 OpenCV 和 ONNX Runtime 的视觉管线：

- **模板匹配**: `OpenCvTemplateMatcher` — NCC 归一化互相关，支持模板缓存
- **OCR**: `PaddleOcrEngine` — PaddleOCR ONNX 模型（检测 + 识别）
- **颜色检测**: `ColorDetectorImpl` — 颜色范围匹配
- **轮廓分析**: `ContourFinder` / `ContourAnalyzer`
- **形态学**: `Morphology` — 腐蚀、膨胀等操作
- **特征匹配**: `SuperPointDetector` / `LightGlueMatcher` — ONNX 特征检测与匹配
- **图像预处理**: `ImagePreprocessor` — 通用图像处理工具

---

### betternte-input — 输入模拟

支持 PC 原生窗口和 Android 模拟器的输入模拟：

- `Win32Input` — Win32 实现（前台 SendInput / 后台 PostMessage）
- `AdbInput` — ADB 实现，用于 Android 模拟器
- `InputQueue` — 输入队列，带速率限制
- `FailoverInputController` — 主备自动切换
- `InputRecorder` / `MacroPlayer` — 宏录制与回放

---

### betternte-helper — 工具库

零内部依赖的独立工具 crate，提供：
- 目录操作（创建、复制、删除、大小计算）
- 编码（Base64、MD5）
- 几何计算（点、矩形、距离、交集）
- 进程信息（PID、是否调试、是否提权）
- 字符串处理（中文检测、数字提取）
- Windows 特定（DPI、前台窗口）

---

### betternte-notify — 推送通知

多通道推送系统，定义 `Notifier` trait：

| 通道 | 实现 | 平台 |
|------|------|------|
| ServerChan | `ServerChanNotifier` | 微信 |
| Telegram | `TelegramNotifier` | Telegram |
| Bark | `BarkNotifier` | iOS |
| Webhook | `WebhookNotifier` | 企业微信、钉钉、飞书、Discord、Slack |

`NotificationManager` 统一管理所有通道，提供 `send_all()`、`send_to()`、`test_channel()`。

---

### betternte-overlay — 叠加层

Win32 分层透明窗口（`WS_EX_TRANSPARENT + WS_EX_LAYERED`），可覆盖在游戏窗口上显示调试信息：

- `OverlayWindow` — 底层窗口操作
- `OverlayManager` — 高层管理器，绑定游戏窗口、同步位置
- `OverlayRenderer` — 帧渲染（begin_frame → draw → end_frame）
- `DrawingApi` — 像素绘制（矩形、线段、文字、十字准星、圆形、进度条）

---

### betternte-runtime — Flow Engine

统一流程执行引擎，基于 Flow / Step / Transition 模型：

- `Flow` — 有向图，由 `Step` 和 `Transition` 组成
- `Step` — 执行单元，类型包括 script、click、swipe、key_press、wait、flow（嵌套）、group、set_variable
- `Transition` — 步骤间连接，附带 `Condition` 条件
- `Condition` — 条件枚举（Always、Template、Ocr、Color、Variable、Hotkey、Script、And/Or/Not）
- `FlowExecutor` — 执行器主循环
- `VariableStore` — 双层变量系统（默认值 + 持久化），支持引用解析（`$variables.x`、`$result.y`）
- `PermissionGuard` — 基于 manifest 的权限沙箱（已移除）

---

### betternte-script — 脚本引擎

基于 QuickJS 的 JavaScript 脚本运行时，通过 `rquickjs` crate 集成。

**脚本类型:**
- `SoloTask` — 一次性任务脚本（调用 `start()`）
- `Trigger` — 帧驱动触发器（调用 `on_enable()` + 每帧 `on_capture()`）
- `Library` — 可复用模块（调用 `call_function()`）

**QuickJS Async Bridge:**

QuickJS 本身是同步的，但 `ScriptContext` 的所有方法都是 async 的。解决方案：

1. 独立后台线程 `qjs-async-bridge` 拥有自己的 tokio 运行时
2. JS 全局注册 `__invoke(method, args_json)` 同步函数
3. JS 包装函数将 `__invoke` 包装为 `Promise`，脚本可直接 `await ctx.click(100, 200)`
4. `__invoke` 通过 `mpsc::channel` 将 async 闭包发送到桥接线程，轮询 `recv_timeout(50ms)` 检查取消
5. 嵌套调用（library 调用 ctx 方法）使用 `dispatch_ctx_method_nested_blocking`，启动独立线程 + 单线程 tokio 运行时避免死锁

**ScriptContext API（约 45 个方法）:**
- 截图: `capture()`, `capture_region()`
- 识别: `find_template()`, `ocr()`, `get_color()`, `color_match()`
- 输入: `click()`, `key_press()`, `swipe()`, `type_text()`
- 等待: `sleep()`, `wait_for_template()`, `wait_for_color()`
- 窗口: `find_window()`, `activate_window()`
- 存储: `storage_get()`, `storage_set()`
- 文件: `read_file()`, `write_file()`
- 网络: `http_get()`, `http_post()`
- IPC: `run_script()`, `call_library()`
- 通知: `notify()`

---

### betternte-engine — 引擎门面

顶层 Facade，客户端只与此 crate 交互：

```rust
// 生命周期
EngineBuilder::new(config, base_dir).build()  // -> Engine (Idle)
engine.start()                                 // -> Running
engine.stop()                                  // -> Idle
```

`start()` 时执行：
1. 从所有订阅目录加载脚本
2. 同步触发器状态
3. 绑定目标游戏窗口
4. 启动截图循环（按配置 FPS 生成 tokio task）
5. 启动回放录制（如已配置）

`EngineBuilder` 支持自定义 `StepHandler`、`ConditionHandler`、`InputRunner`，便于扩展。

---

### betternte-client — Tauri 桌面端

Tauri v2 桌面应用，Rust 后端通过 `AppState` 持有 `Option<Engine>`（`tokio::sync::RwLock` 保护）。

**模块:**
- `lib.rs` — 插件注册、系统托盘、窗口管理、事件桥接、日志初始化
- `hotkeys.rs` — 全局热键（紧急停止、切换叠加层、脚本/任务组触发）
- `commands/` — 55 个 Tauri IPC 命令，分 6 个模块

**Tauri 插件:**
- `tauri-plugin-shell` — Shell 命令执行
- `tauri-plugin-notification` — 系统通知
- `tauri-plugin-dialog` — 文件对话框
- `tauri-plugin-clipboard-manager` — 剪贴板
- `tauri-plugin-global-shortcut` — 全局快捷键
- `tauri-plugin-single-instance` — 单实例
- `tauri-plugin-updater` — 自动更新（仅桌面端）

---

## 前端架构

### 技术栈

| 层 | 技术 |
|----|------|
| UI 框架 | React 19 + TypeScript 6 |
| 构建工具 | Vite 8 |
| 路由 | react-router-dom 7 |
| 状态管理 | Zustand 5 |
| 样式 | Tailwind CSS 4 |
| 图标 | lucide-react |
| 代码编辑器 | CodeMirror 6 |
| 流程图编辑 | @xyflow/react 12 + @dagrejs/dagre |

### 路由

| 路径 | 页面 | 说明 |
|------|------|------|
| `/` | HomePage | 启动页 |
| `/triggers` | TriggerPage | 触发器管理 |
| `/scripts` | TaskPage | 脚本管理与运行 |
| `/one-dragon` | OneDragonFlow | 任务组编排 |
| `/workflow` | FlowEditorPage | 可视化流程编辑器 |
| `/debug` | ScriptDebugPage | 脚本调试追踪 |
| `/settings` | Settings | 引擎配置 |
| `/input-test` | InputTestPage | 输入调试（仅开发模式） |

### 状态管理（Zustand 切片）

| 切片 | 职责 |
|------|------|
| `EngineSlice` | 引擎生命周期、配置、状态、截图测试 |
| `ScriptSlice` | 脚本/触发器 CRUD、运行/停止、源码读写 |
| `FlowSlice` | Flow / TaskGroup CRUD、运行/停止/进度 |
| `UISlice` | 日志、最近任务、错误弹窗、事件监听 |
| `DebugSlice` | 脚本调用追踪 |

### 事件桥接

Rust `EventBus` 通过 `app.emit("engine-event", ...)` 发送到前端，前端 `UISlice.setupEventListener()` 使用 `@tauri-apps/api/event` 的 `listen()` 接收并分发到 Zustand 状态。

---

## Tauri IPC 命令

共 55 个命令，按模块分组：

### Engine（5 个）
| 命令 | 说明 |
|------|------|
| `init_engine` | 初始化引擎（幂等），加载配置、注入数据、注册热键 |
| `start_engine` | 启动引擎 |
| `stop_engine` | 停止引擎，释放资源 |
| `get_status` | 获取引擎状态（idle/running、当前任务、脚本数、版本） |
| `stop_all` | 紧急停止所有运行中的任务 |

### Scripts（13 个）
| 命令 | 说明 |
|------|------|
| `reload_scripts` | 从磁盘重新加载脚本 |
| `list_scripts` | 列出所有已加载脚本 |
| `run_script` | 运行脚本（自动启动引擎） |
| `stop_task` | 停止当前任务 |
| `enable_trigger` / `disable_trigger` | 启用/禁用触发器 |
| `reload_triggers` / `list_triggers` | 重新加载/列出触发器 |
| `create_script` / `delete_script` | 创建/删除脚本 |
| `list_script_files` | 列出脚本目录中的文件 |
| `read_script_source` / `save_script_source` | 读写脚本源码（带路径遍历防护） |
| `import_script_asset` | 导入资源文件到脚本目录 |

### Flows（11 个）
| 命令 | 说明 |
|------|------|
| `list_task_groups` / `save_task_group` / `delete_task_group` | 任务组 CRUD |
| `run_task_group` / `stop_task_group` / `get_task_group_progress` | 任务组运行控制 |
| `list_flows` / `save_flow` / `delete_flow` | Flow CRUD |
| `run_flow` / `stop_flow` / `get_flow_progress` | Flow 运行控制 |

### Input（12 个）
| 命令 | 说明 |
|------|------|
| `input_list_windows` / `input_bind_window` | 窗口列表与绑定 |
| `input_key_down` / `input_key_up` / `input_key_tap` | 键盘模拟 |
| `input_mouse_move` / `input_mouse_scroll` / `input_mouse_button` / `input_mouse_click` | 鼠标模拟 |
| `input_demo_*` | 复合输入演示 |
| `input_run_js_snippet` | 执行 JS 代码片段 |

### Settings（12 个）
| 命令 | 说明 |
|------|------|
| `get_config` / `save_config_cmd` | 读写引擎配置 |
| `get_capture_methods` | 列出可用截图方式 |
| `list_subscriptions` / `save_subscription` / `delete_subscription` | 脚本订阅管理 |
| `list_windows` / `find_game_window` | 系统窗口枚举 |
| `test_screenshot` | 测试截图（返回 base64） |
| `test_notification_channel` | 测试推送通道 |
| `list_game_plugins` | 列出游戏插件 |
| `export_logs` | 导出日志 |
| `better_nte_debug_enabled` | 检查调试模式 |

### Replay（2 个）
| 命令 | 说明 |
|------|------|
| `replay_verify_session` | 验证回放会话 |
| `replay_verify_artifacts` | 验证回放产物 |

---

## 脚本开发

### 脚本目录结构

```
scripts/
└── my-script/
    ├── manifest.json      # 脚本元数据
    ├── main.js            # 入口文件
    └── assets/            # 资源文件（模板图片等）
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

**type 字段:**
- `solo_task` — 一次性任务
- `trigger` — 帧触发器
- `library` — 可复用库

### ctx API 速查

脚本中通过全局 `ctx` 对象调用引擎能力：

```javascript
// 截图
const frame = await ctx.capture();
const region = await ctx.capture_region(x, y, w, h);

// 模板匹配
const result = await ctx.find_template("button.png", { threshold: 0.8 });
const results = await ctx.find_templates(["a.png", "b.png"]);

// OCR
const text = await ctx.ocr({ x: 0, y: 0, w: 200, h: 50 });

// 颜色
const color = await ctx.get_color(100, 200);
const matched = await ctx.color_match(100, 200, { r: 255, g: 0, b: 0 }, 30);

// 输入
await ctx.click(500, 300);
await ctx.key_press("enter");
await ctx.swipe(100, 200, 300, 200, 500);

// 等待
await ctx.sleep(1000);
await ctx.wait_for_template("dialog.png", { timeout: 5000 });

// 存储
await ctx.storage_set("count", "42");
const val = await ctx.storage_get("count");

// 网络
const data = await ctx.http_get("https://api.example.com/status");

// IPC
await ctx.run_script("other-script");
const result = await ctx.call_library("utils", "formatDate", [Date.now()]);

// 通知
await ctx.notify("任务完成", "脚本已成功执行");
```

---

## Flow Engine

### 数据模型

```
Flow
 ├── entry: StepId           # 入口步骤
 └── steps: Map<StepId, Step>
      ├── id: StepId
      ├── kind: StepKind     # script / click / swipe / key_press / wait / flow / group / set_variable
      ├── transitions: [Transition]
      │    ├── condition: Condition
      │    ├── target: StepId
      │    ├── priority: int
      │    └── interrupt: bool
      ├── on_error: StepId?  # 错误回退
      ├── retry: int
      └── timeout: Duration
```

### 条件系统

`Condition` 枚举支持组合：

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

### 变量系统

`VariableStore` 提供双层变量：
- **默认值**: 在 Flow 定义中声明
- **运行时**: 执行过程中通过 `set_variable` 步骤修改
- **引用解析**: `$variables.x`、`$result.y`、`$steps.z.result.w`、`$flow_output.k`

---

## 构建与发布

### 开发构建

```bash
cargo build                              # 全 workspace debug
cargo build -p betternte-engine          # 单个 crate
cargo check                              # 仅类型检查
```

### Tauri 构建

```bash
cd crates/betternte-client
pnpm tauri build                         # 生产构建（NSIS 安装包）
```

### 环境变量

| 变量 | 说明 |
|------|------|
| `OPENCV_LINK_PATHS` | OpenCV lib 目录路径 |
| `OPENCV_LINK_LIBS` | 需要链接的 OpenCV 库名 |
| `OPENCV_INCLUDE_PATHS` | OpenCV 头文件路径（通常自动检测） |
| `BETTER_NTE_DEBUG` | 设为 `1` 启用调试模式 |

### 打包产物

Tauri 构建生成 NSIS 安装包（`.exe`），包含：
- 编译后的 Rust 二进制
- 前端静态资源（`dist/`）
- `data/` 和 `assets/` 作为 bundled resources

---

## 调试技巧

### 调试模式

设置环境变量 `BETTER_NTE_DEBUG=1` 启用调试模式：
- 前端会显示 "Input Test" 侧边栏入口
- 额外的调试面板可用

### 日志

- 日志文件位于应用数据目录，使用 `tracing` + `tracing-subscriber`，支持自动轮转
- 通过设置页的"导出日志"按钮可导出日志文件
- 前端 `FloatingLogLayer` 和 `LogDrawer` 实时显示日志

### Overlay 调试

叠加层可以实时显示：
- 模板匹配结果（匹配位置、置信度）
- 十字准星（鼠标/触摸点）
- 进度条
- 自定义文字标注

通过引擎配置中的 `overlay` 字段控制开关。

### 输入测试页

`/input-test` 路由（仅调试模式可见）提供：
- 键盘按键测试
- 鼠标点击/移动测试
- 窗口绑定测试

### 脚本调试

`/debug` 路由提供脚本调用追踪，记录每次 `ctx` 方法调用的参数和返回值。

### JS 代码片段

通过 `input_run_js_snippet` Tauri 命令，可以在引擎运行时执行任意 JS 代码片段，快速测试 `ctx` API。
