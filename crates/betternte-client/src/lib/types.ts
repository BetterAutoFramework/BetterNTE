// Engine state
export type EngineState = "idle" | "running" | "error";

export interface StatusResponse {
  state: EngineState;
  task: string | null;
  task_type?: "script" | "flow";
  progress: ProgressInfo | null;
  uptime: number;
  script_count: number;
  version?: string;
  capture_method?: string;
  input_mode?: string;
}

export interface ProgressInfo {
  current: number;
  total: number;
  message?: string;
}

export interface VersionResponse {
  version: string;
  engine: string;
}

// Script types
export type ScriptType = "task" | "solo_task" | "trigger" | "library";
export type TriggerVisibility = "global" | "workflow";

export interface ScriptInfo {
  name: string;
  display_name: string;
  version: string;
  type: ScriptType;
  author: string;
  description: string;
  enabled: boolean;
  /** Declared API permissions from manifest (e.g. screenshot, input). */
  permissions?: string[];
  icon?: string;
  tags?: string[];
  /** manifest `category` */
  category?: string;
  /** manifest `min_engine_version` */
  min_engine_version?: string;
  /** manifest `entry` (e.g. main.js) */
  entry?: string;
  params_schema?: Record<string, unknown>;
  visibility?: TriggerVisibility;
  /** 所属订阅源名称 */
  source?: string;
  /** 相对于 data_root 的目录路径，如 "main/scripts/hello_world" */
  dir?: string;
}

export interface ScriptDetail extends ScriptInfo {
  tags: string[];
}

export interface ScriptConfigSchema {
  config_schema: Record<string, unknown>;
  ui_schema?: Record<string, unknown>;
}

// Task types
export type TaskStatusType = "idle" | "pending" | "running" | "completed" | "failed" | "cancelled";

export interface TaskStatusResponse {
  running: boolean;
  script: string | null;
  elapsed_ms?: number;
}

export interface TaskResult {
  name: string;
  success: boolean;
  duration_ms: number;
  error?: string;
  started_at: string;
  completed_at: string;
}

// Task group types
export interface TaskGroup {
  uuid: string;
  name: string;
  /** Subscription / plugin label from engine (same layer as script `source`). */
  source?: string;
  description?: string;
  mode: "sequential" | "random";
  retry_count: number;
  nodes: TaskGroupNode[];
  // Advanced
  error_handling?: "interrupt" | "skip";
  retry?: { enabled: boolean; interval_ms: number; count: number };
  notify_on_failure?: boolean;
  schedule?: ScheduleConfig;
  repeat_strategy?: "skip" | "interrupt";
}

export type ScheduleType = "once" | "daily" | "weekly";

export interface ScheduleConfig {
  type: ScheduleType;
  hour?: number;
  minute?: number;
  days_of_week?: number[]; // 0=Sun, 1=Mon, ..., 6=Sat
}

export interface TaskGroupNode {
  /** Script instance id (path under data root, e.g. `main/scripts/foo`); legacy groups may still use manifest `name`. */
  script: string;
  alias: string;
  timeout_ms?: number;
  params?: Record<string, unknown>;
}

export interface TaskGroupProgress {
  current_node: string | null;
  completed: number;
  total: number;
  node_status: Record<string, NodeStatus>;
}

export type NodeStatus =
  | { status: "pending" }
  | { status: "running" }
  | { status: "completed"; data: ScriptResult }
  | { status: "failed"; error: string };

export interface ScriptResult {
  success: boolean;
  data?: unknown;
  message?: string;
}


export type ReplayMode = "normal" | "record" | "replay";

export type SecurityMode = "strict" | "normal";

export interface SecurityConfig {
  mode: SecurityMode;
}

export interface ReplayConfig {
  mode: ReplayMode;
  artifact_root: string;
  session_name: string;
  /** 每 N 次成功截图写一帧 PNG + timeline；0 禁用 */
  frame_sample_interval: number;
}

// Config types
export interface EngineConfig {
  capture: CaptureConfig;
  hotkeys: HotkeyConfig;
  hotkey_triggers: HotkeyTriggersConfig;
  key_bindings: KeyBindingsConfig;
  overlay: OverlayConfig;
  scripts: ScriptConfig;
  triggers: Record<string, TriggerState>;
  /** Default / last-used task script params (`params_schema`), keyed by script path id (`dir`) or legacy manifest name. */
  task_script_params: Record<string, Record<string, unknown>>;
  notifications: NotificationConfig;
  api: ApiConfig;
  game: GameConfig;
  advanced: AdvancedConfig;
  replay: ReplayConfig;
  security: SecurityConfig;
  active_plugin: string;
  plugin_search_paths: string[];
}

export interface GamePluginInfo {
  id: string;
  name: string;
  version: string;
  manifest_path: string;
}

export interface TriggerState {
  enabled: boolean;
  params: Record<string, unknown>;
}

export interface CaptureConfig {
  method: string;
  method_whitelist: string[];
  target_type: "window" | "display";
  fps_cap: number;
  display_index: number;
  crop_mode: "client_only" | "window";
  hdr_policy: "off" | "auto" | "force";
  minimized_behavior: "pause" | "keep_trying" | "pseudo_background";
  recover_on_resize: boolean;
  recover_on_monitor_switch: boolean;
  crop_shadow: boolean;
  hdr_to_sdr: boolean;
}

export interface CaptureMethodInfo {
  value: string;
  available: boolean;
  in_whitelist: boolean;
}

export interface KeyBindingsConfig {
  bindings: Record<string, string>;
}

export interface OcrTuningProfile {
  max_side_len: number;
  det_threshold: number;
  rec_threshold: number;
  batch_size: number;
  unclip_ratio: number;
}

export interface OcrPresetsConfig {
  performance: OcrTuningProfile;
  balanced: OcrTuningProfile;
  accuracy: OcrTuningProfile;
}

export interface AdvancedConfig {
  ocr_engine: string;
  ocr_model_dir: string;
  ocr_max_side_len: number;
  ocr_det_threshold: number;
  ocr_rec_threshold: number;
  ocr_batch_size: number;
  ocr_unclip_ratio: number;
  ocr_presets: OcrPresetsConfig;
  template_match_threshold: number;
  hardware_acceleration: string;
  input_mode: string;
  foreground_input_backend: string;
  input_rate_limit: number;
  log_level: string;
  log_file: string;
  log_max_size: number;
  log_max_files: number;
  task_groups_file: string;
  debug_screenshot_dir: string;
  debug_mode: boolean;
}

export interface HotkeyConfig {
  toggle_task: string;
  emergency_stop: string;
  toggle_overlay: string;
  pause_resume: string;
  debug_screenshot: string;
}

/** Global shortcut → script name or task group uuid; press again while that target runs to stop (saved in engine config). */
export interface HotkeyTriggersConfig {
  scripts: Record<string, string>;
  task_groups: Record<string, string>;
}

export interface OverlayConfig {
  enabled: boolean;
  mode: string;
  opacity: number;
  font_size: number;
}

export interface Subscription {
  name: string;
  directory: string;
  enabled: boolean;
  auto_update: boolean;
  url?: string;
}

export interface ScriptConfig {
  data_root: string;
  auto_update: boolean;
  subscriptions: Subscription[];
}

export interface NotificationConfig {
  enabled: boolean;
  level?: string;
  telegram?: { enabled: boolean; bot_token: string; chat_id: string };
  discord?: { enabled: boolean; webhook_url: string };
  serverchan?: { enabled: boolean; send_key: string };
  bark?: { enabled: boolean; server_url: string; device_key: string };
}

export interface ApiConfig {
  host: string;
  port: number;
  auth_token?: string;
}

export interface GameConfig {
  game_name: string;
  window_title_keyword: string;
  process_name: string;
  game_language: string;
  resolution: string;
  scale: number;
  dpi: number;
}

// Log types
export type LogLevel = "debug" | "info" | "warn" | "error";

export interface LogEntry {
  level: LogLevel;
  message: string;
  timestamp: string;
}

// Store script (for ScriptStore page)
export interface StoreScript {
  name: string;
  display_name: string;
  version: string;
  author: string;
  description: string;
  category: string;
  rating: number;
  downloads: number;
  installed: boolean;
  update_available: boolean;
}

// Engine events (from Rust EventBus via Tauri app.emit)
export type EngineEvent =
  | { type: "TaskStarted"; data: { task_name: string; task_type: string; timestamp: string } }
  | { type: "TaskStopped"; data: { task_name: string; reason: string; duration_ms: number; timestamp: string } }
  | { type: "TaskProgress"; data: { task_name: string; current: number; total: number; message: string } }
  | { type: "ScriptLoaded"; data: { script_name: string; version: string; path: string } }
  | { type: "ScriptUnloaded"; data: { script_name: string } }
  | { type: "CaptureStatusChanged"; data: { engine_name: string; is_capturing: boolean; fps: number } }
  | { type: "Error"; data: { module: string; message: string; severity: string; recoverable: boolean } }
  | { type: "ConfigChanged"; data: { key: string; old_value: string | null; new_value: string } }
  | { type: "LogMessage"; data: { level: string; module: string; message: string; timestamp: string } }
  | { type: "ScriptCallTrace"; data: ScriptCallTraceData };

// Debug trace types
export type DebugCategory = "recognition" | "operation" | "wait" | "utility" | "capture" | "window" | "log";

export interface ScriptCallTraceData {
  id: string;
  category: string;
  method: string;
  args: unknown;
  result: string | null;
  success: boolean;
  error: string | null;
  screenshot_before: string | null;
  screenshot_after: string | null;
  duration_ms: number;
  timestamp: string;
}

export interface DebugEntry {
  id: string;
  category: DebugCategory;
  method: string;
  args: unknown;
  result: string | null;
  success: boolean;
  error: string | null;
  screenshotBefore: string | null;
  screenshotAfter: string | null;
  durationMs: number;
  timestamp: Date;
}

// ============================================================================
// Flow / Workflow types (matching betternte-runtime)
// ============================================================================

export type StepKindType =
  | "script"
  | "click"
  | "swipe"
  | "key_press"
  | "wait"
  | "flow"
  | "group"
  | "set_variable"
  | "none";

export interface StepKindScript { type: "script"; script: string }
export interface StepKindClick { type: "click"; x: number; y: number }
export interface StepKindSwipe { type: "swipe"; x1: number; y1: number; x2: number; y2: number; duration_ms: number }
export interface StepKindKeyPress { type: "key_press"; key: string }
export interface StepKindWait { type: "wait"; ms: number }
export interface StepKindFlow { type: "flow"; flow: string }
export interface StepKindGroup { type: "group"; group: string }
export interface StepKindSetVariable { type: "set_variable"; key: string; value: unknown }
export interface StepKindNone { type: "none" }

export type StepKind =
  | StepKindScript
  | StepKindClick
  | StepKindSwipe
  | StepKindKeyPress
  | StepKindWait
  | StepKindFlow
  | StepKindGroup
  | StepKindSetVariable
  | StepKindNone;

export interface RegionDef {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type CompareOp = "eq" | "ne" | "gt" | "lt" | "gte" | "lte" | "in" | "contains";

export type Condition =
  | { type: "always" }
  | { type: "template"; template: string; threshold: number; roi?: RegionDef }
  | { type: "ocr"; expected: string; roi?: RegionDef }
  | { type: "color"; x: number; y: number; color: string; tolerance: number }
  | { type: "variable"; key: string; op: CompareOp; value: unknown }
  | { type: "hotkey"; key: string }
  | { type: "script"; script: string }
  | { type: "and"; conditions: Condition[] }
  | { type: "or"; conditions: Condition[] }
  | { type: "not"; condition: Condition };

export interface Transition {
  target: string;
  condition: Condition;
  priority: number;
  interrupt: boolean;
}

export interface FlowStep {
  kind: StepKind;
  input: Record<string, string>;
  output: Record<string, string>;
  transitions: Transition[];
  timeout_ms?: number;
  max_retries: number;
  on_error?: string;
}

export interface VariableDef {
  value_type: string;
  default?: unknown;
  persist: boolean;
  schema?: unknown;
}

export interface FlowDefinition {
  id: string;
  name: string;
  description: string;
  version: string;
  entry: string;
  steps: Record<string, FlowStep>;
  variables: Record<string, VariableDef>;
  tags: string[];
  output_schema?: unknown;
  orchestration?: FlowOrchestration;
}

export interface FlowOrchestration {
  mode?: "sequential" | "random" | string;
  retry_count?: number;
  error_handling?: "interrupt" | "skip" | string;
  retry?: { enabled: boolean; interval_ms: number; count: number } | Record<string, unknown>;
  notify_on_failure?: boolean;
  schedule?: ScheduleConfig | Record<string, unknown>;
  repeat_strategy?: "skip" | "interrupt" | string;
  source?: string;
}

// Window types
export interface WindowRect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface GameWindow {
  hwnd: number;
  title: string;
  class_name: string;
  pid: number;
  process_name: string;
  rect: WindowRect;
  client_rect: WindowRect;
  is_minimized: boolean;
  dpi_scale: number;
}
