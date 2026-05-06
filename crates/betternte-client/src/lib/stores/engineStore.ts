import type { StateCreator } from "zustand";

import type {
  CaptureMethodInfo,
  EngineConfig,
  GameWindow,
  StatusResponse,
  Subscription,
} from "../types";
import {
  defaultStatus,
  invokeAction,
  mapConfigToRust,
  mapEngineConfig,
  migrateEngineConfigScriptKeys,
} from "./helpers";
import type { CombinedStore } from "./index";

// ============================================================================
// State + Actions
// ============================================================================

export interface EngineSlice {
  initialized: boolean;
  loading: boolean;
  error: string | null;
  status: StatusResponse;
  config: EngineConfig | null;
  captureMethods: CaptureMethodInfo[];
  subscriptions: Subscription[];
  engineStartedAt: number | null;

  initEngine: () => Promise<void>;
  startEngine: () => Promise<void>;
  stopEngine: () => Promise<void>;
  refreshStatus: () => Promise<void>;
  loadConfig: () => Promise<void>;
  saveConfig: (config: EngineConfig) => Promise<void>;
  refreshCaptureMethods: () => Promise<void>;
  refreshSubscriptions: () => Promise<void>;
  saveSubscription: (sub: Subscription) => Promise<void>;
  deleteSubscription: (directory: string) => Promise<void>;
  listWindows: () => Promise<GameWindow[]>;
  findGameWindow: () => Promise<GameWindow | null>;
  testScreenshot: () => Promise<string | null>;
  stopAll: () => Promise<void>;
}

// ============================================================================
// State creator
// ============================================================================

export const createEngineSlice: StateCreator<CombinedStore, [], [], EngineSlice> = (set, get) => ({
  initialized: false,
  loading: false,
  error: null,
  status: defaultStatus,
  config: null,
  captureMethods: [],
  subscriptions: [],
  engineStartedAt: null,

  initEngine: async () => {
    if (get().initialized || get().loading) return;

    set({ loading: true, error: null });
    try {
      const msg = await invokeAction<string>("init_engine");
      console.info("Engine init:", msg);

      // Register event listener immediately after engine init succeeds,
      // before any other IPC calls that might fail and skip listener setup.
      get().setupEventListener();

      const [statusRaw, configRaw] = await Promise.all([
        invokeAction<Record<string, unknown>>("get_status"),
        invokeAction<Record<string, unknown>>("get_config"),
      ]);

      const status: StatusResponse = {
        state: (statusRaw.state as StatusResponse["state"]) ?? "idle",
        task: (statusRaw.task as string | null) ?? null,
        task_type: (statusRaw.task_type as StatusResponse["task_type"]) ?? undefined,
        progress: null,
        uptime: Number(statusRaw.uptime ?? 0),
        script_count: Number(statusRaw.script_count ?? 0),
        version: statusRaw.version as string | undefined,
        capture_method: statusRaw.capture_method as string | undefined,
        input_mode: statusRaw.input_mode as string | undefined,
      };

      const config = mapEngineConfig(configRaw);

      set({
        initialized: true,
        loading: false,
        status,
        config,
        scriptRunConfigs: config.task_script_params,
      });

      await Promise.all([
        get().refreshCaptureMethods(),
        get().refreshScripts(),
        get().refreshTriggers(),
        get().refreshFlows(),
        get().refreshSubscriptions(),
      ]);

      const cfg0 = get().config;
      if (cfg0) {
        const { config: migrated, changed } = migrateEngineConfigScriptKeys(
          cfg0,
          get().scripts,
          get().triggers,
        );
        if (changed) {
          await get().saveConfig(migrated);
        }
      }
    } catch (e) {
      set({ loading: false, error: String(e) });
      console.error("Engine init failed:", e);
    }
  },

  startEngine: async () => {
    await invokeAction<string>("start_engine", undefined, { silent: true });
    set({ engineStartedAt: Date.now() });
    await Promise.all([get().refreshStatus(), get().refreshScripts()]);
  },

  stopEngine: async () => {
    await invokeAction<string>("stop_engine", undefined, { silent: true });
    set((s) => ({
      status: { ...s.status, state: "idle", task: null, task_type: undefined },
      engineStartedAt: null,
    }));
  },

  refreshStatus: async () => {
    const raw = await invokeAction<Record<string, unknown>>("get_status", undefined, { silent: true });
    if (!raw) return;
    set({
      status: {
        state: (raw.state as StatusResponse["state"]) ?? "idle",
        task: (raw.task as string | null) ?? null,
        task_type: (raw.task_type as StatusResponse["task_type"]) ?? undefined,
        progress: null,
        uptime: Number(raw.uptime ?? 0),
        script_count: Number(raw.script_count ?? 0),
        version: raw.version as string | undefined,
        capture_method: raw.capture_method as string | undefined,
        input_mode: raw.input_mode as string | undefined,
      },
    });
  },

  loadConfig: async () => {
    const raw = await invokeAction<Record<string, unknown>>("get_config", undefined, { silent: true });
    if (!raw) return;
    const mapped = mapEngineConfig(raw);
    set({ config: mapped, scriptRunConfigs: mapped.task_script_params });
  },

  saveConfig: async (config: EngineConfig) => {
    const raw = await invokeAction<Record<string, unknown>>("save_config_cmd", {
      config: mapConfigToRust(config),
    });
    const mapped = mapEngineConfig(raw);
    set({ config: mapped, scriptRunConfigs: mapped.task_script_params });
  },

  refreshCaptureMethods: async () => {
    const methods = await invokeAction<CaptureMethodInfo[]>("get_capture_methods", undefined, { silent: true });
    if (methods) set({ captureMethods: methods });
  },

  refreshSubscriptions: async () => {
    const subs = await invokeAction<Subscription[]>("list_subscriptions", undefined, { silent: true });
    if (subs) set({ subscriptions: subs });
  },

  saveSubscription: async (sub: Subscription) => {
    await invokeAction("save_subscription", { subscription: sub }, {
      showError: true,
      errorTitle: "保存订阅源失败",
    }, get().showError);
    await get().refreshSubscriptions();
    await get().refreshScripts();
    await get().refreshTriggers();
  },

  deleteSubscription: async (directory: string) => {
    await invokeAction("delete_subscription", { directory }, {
      showError: true,
      errorTitle: "删除订阅源失败",
      silent: true,
    }, get().showError);
    await get().refreshSubscriptions();
    await get().refreshScripts();
    await get().refreshTriggers();
  },

  listWindows: async () => {
    const result = await invokeAction<GameWindow[]>("list_windows", undefined, { silent: true }) ?? [];
    return result;
  },

  findGameWindow: async () => {
    const result = await invokeAction<GameWindow>("find_game_window", undefined, { silent: true }) ?? null;
    return result;
  },

  testScreenshot: async () => {
    return await invokeAction<string>("test_screenshot", undefined, {
      showError: true,
      errorTitle: "截图失败",
      silent: true,
    }, get().showError) ?? null;
  },

  stopAll: async () => {
    await invokeAction("stop_all", undefined, { silent: true });
    get().refreshStatus();
  },
});
