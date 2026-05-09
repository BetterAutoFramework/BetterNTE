import { invoke } from "@tauri-apps/api/core";

import type {
  EngineConfig,
  ScriptInfo,
  StatusResponse,
  TriggerState,
} from "../types";

// ============================================================================
// invokeAction — unified invoke middleware
// ============================================================================

export class InvokeActionError extends Error {
  public readonly command: string;
  public override readonly cause?: unknown;

  constructor(
    message: string,
    command: string,
    cause?: unknown,
  ) {
    super(message);
    this.command = command;
    this.cause = cause;
    this.name = "InvokeActionError";
  }
}

export interface InvokeActionOptions {
  /** Show error dialog to user via showError */
  showError?: boolean;
  /** Title for the error dialog (defaults to command name) */
  errorTitle?: string;
  /** If true, swallow the error after logging (no re-throw) */
  silent?: boolean;
}

/**
 * Unified Tauri invoke wrapper with consistent error handling.
 *
 * @example
 * // Silent swallow (like refreshStatus)
 * await invokeAction("get_status", {}, { silent: true });
 *
 * // Show dialog + re-throw (like saveSubscription)
 * await invokeAction("save_subscription", { subscription: sub }, { showError: true, errorTitle: "保存订阅源失败" });
 *
 * // Default: log + re-throw (like saveConfig)
 * await invokeAction("save_config_cmd", { config });
 */
export async function invokeAction<T>(
  command: string,
  args?: Record<string, unknown>,
  options?: InvokeActionOptions,
  showError?: (title: string, message: string) => void,
): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    const msg = String(e);
    console.error(`[${command}] ${msg}`, e);

    if (options?.showError && showError) {
      showError(options.errorTitle ?? command, msg);
    }

    if (options?.silent) {
      // Swallow — caller gets undefined for non-generic, or can catch themselves
      return undefined as T;
    }

    throw new InvokeActionError(msg, command, e);
  }
}

// ============================================================================
// Types matching Rust output
// ============================================================================

export interface RustManifest {
  schema_version: number;
  name: string;
  display_name: string;
  version: string;
  author: string;
  description: string;
  type: string;
  entry: string;
  tags: string[];
  permissions: string[];
  min_engine_version?: string;
  category?: string;
  icon?: string;
  params_schema?: Record<string, unknown>;
  visibility?: string;
}

export interface RustScriptInfo {
  path: string;
  dir: string;
  manifest: RustManifest;
  compatible: boolean;
  loaded: boolean;
  source: string;
}

// ============================================================================
// Helpers
// ============================================================================

export const defaultStatus: StatusResponse = {
  state: "idle",
  task: null,
  progress: null,
  uptime: 0,
  script_count: 0,
};

/** Defaults for `game` when fields are absent in merged config (aligned with `GameConfig` in core). */
const DEFAULT_GAME_NAME = "异环";
const DEFAULT_WINDOW_TITLE_KEYWORD = "异环";
const DEFAULT_PROCESS_NAME = "HTGame.exe";

/** Treat null/undefined/blank as missing so defaults apply (serde `game: {}` yields ""). */
function strGameField(v: unknown, defaultVal: string): string {
  const s = v == null ? "" : String(v).trim();
  return s === "" ? defaultVal : s;
}

/** Unique key for a script — dir is unique across sources, name is fallback. */
export function scriptKey(s: ScriptInfo): string {
  return s.dir ?? s.name;
}

/** Resolve stored task params for a task-group node (`script` = path id or legacy manifest name). */
export function scriptRunConfigLookup(
  configs: Record<string, Record<string, unknown>>,
  nodeScript: string,
  scripts: ScriptInfo[],
): Record<string, unknown> {
  const direct = configs[nodeScript];
  if (direct != null && typeof direct === "object" && Object.keys(direct).length > 0) {
    return direct;
  }

  const info =
    scripts.find((s) => scriptKey(s) === nodeScript) ??
    scripts.find((s) => s.name === nodeScript);
  if (!info) {
    return direct ?? {};
  }

  const byId = configs[scriptKey(info)];
  if (byId != null && typeof byId === "object" && Object.keys(byId).length > 0) {
    return byId;
  }

  const byName = configs[info.name];
  if (byName != null && typeof byName === "object" && Object.keys(byName).length > 0) {
    return byName;
  }

  return direct ?? {};
}

/**
 * Migrate legacy manifest-name keys to `script_id` (`scriptKey`) for triggers,
 * task_script_params, and script hotkey map values when unambiguous.
 */
export function migrateEngineConfigScriptKeys(
  config: EngineConfig,
  taskScripts: ScriptInfo[],
  triggerScripts: ScriptInfo[],
): { config: EngineConfig; changed: boolean } {
  let changed = false;

  const soloTasks = taskScripts.filter(
    (s) => s.type === "task" || s.type === "solo_task",
  );

  // --- triggers ---
  const triggersNext: Record<string, TriggerState> = { ...config.triggers };
  for (const [key, state] of Object.entries(config.triggers)) {
    if (triggerScripts.some((t) => scriptKey(t) === key)) {
      continue;
    }
    const matches = triggerScripts.filter((t) => t.name === key);
    if (matches.length !== 1) {
      continue;
    }
    const newKey = scriptKey(matches[0]);
    if (newKey === key || triggersNext[newKey] !== undefined) {
      continue;
    }
    delete triggersNext[key];
    triggersNext[newKey] = state;
    changed = true;
  }

  // --- task_script_params ---
  const paramsNext: Record<string, Record<string, unknown>> = {
    ...config.task_script_params,
  };
  for (const [key, val] of Object.entries(config.task_script_params)) {
    if (soloTasks.some((s) => scriptKey(s) === key)) {
      continue;
    }
    const matches = soloTasks.filter((s) => s.name === key);
    if (matches.length !== 1) {
      continue;
    }
    const newKey = scriptKey(matches[0]);
    if (newKey === key || paramsNext[newKey] !== undefined) {
      continue;
    }
    delete paramsNext[key];
    paramsNext[newKey] = val;
    changed = true;
  }

  // --- hotkey_triggers.scripts (shortcut → script id or legacy name) ---
  const scriptsMap = { ...config.hotkey_triggers.scripts };
  for (const [shortcut, target] of Object.entries(config.hotkey_triggers.scripts)) {
    const t = String(target).trim();
    if (soloTasks.some((s) => scriptKey(s) === t)) {
      continue;
    }
    const matches = soloTasks.filter((s) => s.name === t);
    if (matches.length !== 1) {
      continue;
    }
    const newTarget = scriptKey(matches[0]);
    if (newTarget === t) {
      continue;
    }
    scriptsMap[shortcut] = newTarget;
    changed = true;
  }

  if (!changed) {
    return { config, changed: false };
  }

  return {
    config: {
      ...config,
      triggers: triggersNext,
      task_script_params: paramsNext,
      hotkey_triggers: {
        ...config.hotkey_triggers,
        scripts: scriptsMap,
      },
    },
    changed: true,
  };
}

/** Normalize `engine.json` / `engine.yaml` `task_script_params` for the client store. */
function normalizeTaskScriptParams(raw: unknown): Record<string, Record<string, unknown>> {
  if (raw == null || typeof raw !== "object" || Array.isArray(raw)) {
    return {};
  }
  const out: Record<string, Record<string, unknown>> = {};
  for (const [key, val] of Object.entries(raw as Record<string, unknown>)) {
    if (val != null && typeof val === "object" && !Array.isArray(val)) {
      out[key] = val as Record<string, unknown>;
    }
  }
  return out;
}

export function mapScriptInfo(rust: RustScriptInfo): ScriptInfo {
  const perms = rust.manifest.permissions;
  return {
    name: rust.manifest.name,
    display_name: rust.manifest.display_name,
    version: rust.manifest.version,
    type: rust.manifest.type as ScriptInfo["type"],
    author: rust.manifest.author,
    description: rust.manifest.description,
    permissions: Array.isArray(perms) ? [...perms] : [],
    enabled: rust.loaded && rust.compatible,
    icon: rust.manifest.icon || undefined,
    tags: rust.manifest.tags || undefined,
    category: rust.manifest.category?.trim() || undefined,
    min_engine_version: rust.manifest.min_engine_version?.trim() || undefined,
    entry: rust.manifest.entry?.trim() || undefined,
    params_schema: rust.manifest.params_schema as Record<string, unknown> | undefined,
    visibility: (rust.manifest.visibility as "global" | "workflow") || undefined,
    source: rust.source || undefined,
    dir: rust.dir || undefined,
  };
}

export function mapEngineConfig(raw: Record<string, unknown>): EngineConfig {
  const game = (raw.game as Record<string, unknown>) ?? {};
  const api = (raw.api as Record<string, unknown>) ?? {};
  const capture = (raw.capture as Record<string, unknown>) ?? {};
  const hotkeys = (raw.hotkeys as Record<string, unknown>) ?? {};
  const hotkeyTriggers = (raw.hotkey_triggers as Record<string, unknown>) ?? {};
  const keyBindings = (raw.key_bindings as Record<string, unknown>) ?? {};
  const overlay = (raw.overlay as Record<string, unknown>) ?? {};
  const scripts = (raw.scripts as Record<string, unknown>) ?? {};
  const notifications = (raw.notifications as Record<string, unknown>) ?? {};
  const advanced = (raw.advanced as Record<string, unknown>) ?? {};
  const ocrPresets = (advanced.ocr_presets as Record<string, Record<string, unknown>>) ?? {};
  const perfPreset = (ocrPresets.performance as Record<string, unknown>) ?? {};
  const balancedPreset = (ocrPresets.balanced as Record<string, unknown>) ?? {};
  const accuracyPreset = (ocrPresets.accuracy as Record<string, unknown>) ?? {};
  const triggers = (raw.triggers as Record<string, Record<string, unknown>>) ?? {};
  const taskScriptParams = normalizeTaskScriptParams(raw.task_script_params);
  const replay = (raw.replay as Record<string, unknown>) ?? {};
  const security = (raw.security as Record<string, unknown>) ?? {};

  return {
    capture: {
      method: String(capture.method ?? "auto"),
      method_whitelist: Array.isArray(capture.method_whitelist)
        ? (capture.method_whitelist as string[])
        : ["bitblt", "print_window", "dwm_shared_surface", "windows_graphics_capture", "dxgi_desktop_duplication"],
      target_type: String(capture.target_type ?? "window") === "display" ? "display" : "window",
      fps_cap: Number(capture.fps_cap ?? 30),
      display_index: Number(capture.display_index ?? 0),
      crop_mode: (() => {
        if (capture.crop_mode !== undefined) {
          return String(capture.crop_mode) === "window" ? "window" : "client_only";
        }
        return (capture.crop_shadow ?? true) ? "client_only" : "window";
      })(),
      hdr_policy: (() => {
        if (capture.hdr_policy === undefined) {
          return (capture.hdr_to_sdr ?? true) ? "auto" : "off";
        }
        const policy = String(capture.hdr_policy);
        if (policy === "off" || policy === "force") return policy;
        return "auto";
      })(),
      minimized_behavior: (() => {
        const behavior = String(capture.minimized_behavior ?? "keep_trying");
        if (behavior === "pause" || behavior === "pseudo_background") return behavior;
        return "keep_trying";
      })(),
      recover_on_resize: Boolean(capture.recover_on_resize ?? true),
      recover_on_monitor_switch: Boolean(capture.recover_on_monitor_switch ?? true),
      crop_shadow: Boolean(capture.crop_shadow ?? true),
      hdr_to_sdr: Boolean(capture.hdr_to_sdr ?? true),
    },
    hotkeys: {
      toggle_task: String(hotkeys.toggle_task ?? "Ctrl+L"),
      emergency_stop: String(hotkeys.emergency_stop ?? "Ctrl+P"),
      toggle_overlay: String(hotkeys.toggle_overlay ?? "Ctrl+O"),
      pause_resume: String(hotkeys.toggle_pause ?? "Ctrl+I"),
      debug_screenshot: String(hotkeys.screenshot ?? "Ctrl+U"),
    },
    hotkey_triggers: {
      scripts: (hotkeyTriggers.scripts as Record<string, string>) ?? {},
      task_groups: (hotkeyTriggers.task_groups as Record<string, string>) ?? {},
    },
    key_bindings: {
      bindings: (keyBindings.bindings as Record<string, string>) ?? {},
    },
    overlay: {
      enabled: Boolean(overlay.enabled ?? false),
      mode: String(overlay.mode ?? "minimal"),
      opacity: Number(overlay.opacity ?? 0.8),
      font_size: Number(overlay.font_size ?? 14),
    },
    scripts: {
      auto_update: Boolean(scripts.auto_update ?? false),
      subscriptions: Array.isArray(scripts.subscriptions)
        ? (scripts.subscriptions as Array<Record<string, unknown>>).map((s) => ({
            name: String(s.name ?? ""),
            directory: String(s.directory ?? ""),
            enabled: Boolean(s.enabled ?? true),
            auto_update: Boolean(s.auto_update ?? false),
            url: s.url ? String(s.url) : undefined,
          }))
        : [],
    },
    triggers: Object.fromEntries(
      Object.entries(triggers).map(([name, state]) => [
        name,
        {
          enabled: Boolean(state.enabled ?? false),
          params: (state.params as Record<string, unknown>) ?? {},
        },
      ])
    ),
    task_script_params: taskScriptParams,
    notifications: {
      enabled: Boolean(notifications.enabled ?? false),
      level: String(notifications.level ?? "warning"),
      telegram: notifications.telegram
        ? {
            enabled: Boolean((notifications.telegram as Record<string, unknown>).enabled ?? false),
            bot_token: String((notifications.telegram as Record<string, unknown>).bot_token ?? ""),
            chat_id: String((notifications.telegram as Record<string, unknown>).chat_id ?? ""),
          }
        : undefined,
      discord: notifications.discord
        ? {
            enabled: Boolean((notifications.discord as Record<string, unknown>).enabled ?? false),
            webhook_url: String((notifications.discord as Record<string, unknown>).webhook_url ?? ""),
          }
        : undefined,
      serverchan: notifications.serverchan
        ? {
            enabled: Boolean((notifications.serverchan as Record<string, unknown>).enabled ?? false),
            send_key: String((notifications.serverchan as Record<string, unknown>).send_key ?? ""),
          }
        : undefined,
      bark: notifications.bark
        ? {
            enabled: Boolean((notifications.bark as Record<string, unknown>).enabled ?? false),
            server_url: String((notifications.bark as Record<string, unknown>).server_url ?? "https://api.day.app"),
            device_key: String((notifications.bark as Record<string, unknown>).device_key ?? ""),
          }
        : undefined,
    },
    api: {
      host: String(api.host ?? "127.0.0.1"),
      port: Number(api.port ?? 23330),
      auth_token: String(api.auth_token ?? ""),
    },
    game: {
      game_name: strGameField(game.game_name, DEFAULT_GAME_NAME),
      window_title_keyword: strGameField(
        game.window_title_keyword,
        DEFAULT_WINDOW_TITLE_KEYWORD
      ),
      process_name: strGameField(game.process_name, DEFAULT_PROCESS_NAME),
      game_language: String(game.game_language ?? "zh-cn"),
      resolution: String(game.resolution ?? "1920x1080"),
      scale: Number(game.scale ?? 1),
      dpi: Number(game.dpi ?? 96),
    },
    advanced: {
      ocr_engine: String(advanced.ocr_engine ?? "paddle_ocr"),
      ocr_model_dir: String(advanced.ocr_model_dir ?? "assets/models/paddleocr"),
      ocr_max_side_len: Number(advanced.ocr_max_side_len ?? 960),
      ocr_det_threshold: Number(advanced.ocr_det_threshold ?? 0.3),
      ocr_rec_threshold: Number(advanced.ocr_rec_threshold ?? 0.5),
      ocr_batch_size: Number(advanced.ocr_batch_size ?? 8),
      ocr_unclip_ratio: Number(advanced.ocr_unclip_ratio ?? 2.0),
      ocr_presets: {
        performance: {
          max_side_len: Number(perfPreset.max_side_len ?? 640),
          det_threshold: Number(perfPreset.det_threshold ?? 0.25),
          rec_threshold: Number(perfPreset.rec_threshold ?? 0.45),
          batch_size: Number(perfPreset.batch_size ?? 16),
          unclip_ratio: Number(perfPreset.unclip_ratio ?? 1.6),
        },
        balanced: {
          max_side_len: Number(balancedPreset.max_side_len ?? 960),
          det_threshold: Number(balancedPreset.det_threshold ?? 0.3),
          rec_threshold: Number(balancedPreset.rec_threshold ?? 0.5),
          batch_size: Number(balancedPreset.batch_size ?? 8),
          unclip_ratio: Number(balancedPreset.unclip_ratio ?? 2.0),
        },
        accuracy: {
          max_side_len: Number(accuracyPreset.max_side_len ?? 1280),
          det_threshold: Number(accuracyPreset.det_threshold ?? 0.35),
          rec_threshold: Number(accuracyPreset.rec_threshold ?? 0.55),
          batch_size: Number(accuracyPreset.batch_size ?? 4),
          unclip_ratio: Number(accuracyPreset.unclip_ratio ?? 2.4),
        },
      },
      template_match_threshold: Number(advanced.template_match_threshold ?? 0.8),
      hardware_acceleration: String(advanced.hardware_acceleration ?? "auto"),
      input_mode: String(advanced.input_mode ?? "auto"),
      foreground_input_backend: String(advanced.foreground_input_backend ?? "enigo"),
      input_rate_limit: Number(advanced.input_rate_limit ?? 0),
      log_level: String(advanced.log_level ?? "info"),
      log_file: String(advanced.log_file ?? "logs/betternte.log"),
      log_max_size: Number(advanced.log_max_size ?? 50),
      log_max_files: Number(advanced.log_max_files ?? 5),
      task_groups_file: String(advanced.task_groups_file ?? "task_groups.json"),
      debug_screenshot_dir: String(advanced.debug_screenshot_dir ?? ""),
      debug_mode: Boolean(advanced.debug_mode ?? false),
    },
    replay: {
      mode: normalizeReplayMode(replay.mode),
      artifact_root: String(replay.artifact_root ?? ""),
      session_name: String(replay.session_name ?? ""),
      frame_sample_interval: Number(replay.frame_sample_interval ?? 0),
    },
    security: {
      mode: String(security.mode ?? "normal") === "strict" ? "strict" : "normal",
    },
    plugins: Object.fromEntries(
      Object.entries((raw.plugins as Record<string, unknown>) ?? {}).map(([id, state]) => {
        const s = (state as Record<string, unknown>) ?? {};
        return [
          id,
          {
            enabled: Boolean(s.enabled ?? false),
            config: (s.config as Record<string, unknown>) ?? {},
          },
        ];
      })
    ),
  };
}

function normalizeReplayMode(v: unknown): "normal" | "record" | "replay" {
  const s = String(v ?? "normal");
  if (s === "record" || s === "replay") return s;
  return "normal";
}

export function mapConfigToRust(config: EngineConfig): Record<string, unknown> {
  const g = config.game as EngineConfig["game"] & {
    launch_args?: string;
    auto_launch?: boolean;
    launch_delay?: number;
  };
  return {
    capture: {
      method: config.capture.method,
      method_whitelist: config.capture.method_whitelist,
      target_type: config.capture.target_type,
      fps_cap: config.capture.fps_cap,
      display_index: config.capture.display_index,
      crop_mode: config.capture.crop_mode,
      hdr_policy: config.capture.hdr_policy,
      minimized_behavior: config.capture.minimized_behavior,
      recover_on_resize: config.capture.recover_on_resize,
      recover_on_monitor_switch: config.capture.recover_on_monitor_switch,
      crop_shadow: config.capture.crop_shadow,
      hdr_to_sdr: config.capture.hdr_to_sdr,
    },
    hotkeys: {
      toggle_task: config.hotkeys.toggle_task,
      emergency_stop: config.hotkeys.emergency_stop,
      toggle_overlay: config.hotkeys.toggle_overlay,
      toggle_pause: config.hotkeys.pause_resume,
      screenshot: config.hotkeys.debug_screenshot,
    },
    hotkey_triggers: config.hotkey_triggers,
    key_bindings: config.key_bindings,
    overlay: config.overlay,
    scripts: config.scripts,
    triggers: config.triggers,
    task_script_params: config.task_script_params,
    notifications: config.notifications,
    api: config.api,
    game: {
      game_name: strGameField(g.game_name, DEFAULT_GAME_NAME),
      window_title_keyword: strGameField(
        g.window_title_keyword,
        DEFAULT_WINDOW_TITLE_KEYWORD
      ),
      process_name: strGameField(g.process_name, DEFAULT_PROCESS_NAME),
      game_language: g.game_language,
      resolution: g.resolution,
      scale: g.scale,
      dpi: g.dpi,
      launch_args: g.launch_args ?? "",
      auto_launch: g.auto_launch ?? false,
      launch_delay: typeof g.launch_delay === "number" ? g.launch_delay : 30,
    },
    advanced: config.advanced,
    replay: {
      mode: config.replay.mode,
      artifact_root: config.replay.artifact_root,
      session_name: config.replay.session_name,
      frame_sample_interval: config.replay.frame_sample_interval,
    },
    security: config.security,
    plugins: config.plugins,
  };
}
