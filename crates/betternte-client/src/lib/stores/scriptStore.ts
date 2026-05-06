import type { StateCreator } from "zustand";

import type { ScriptInfo } from "../types";
import type { RustScriptInfo } from "./helpers";
import { invokeAction,mapScriptInfo } from "./helpers";
import type { CombinedStore } from "./index";

/** Serialize `saveConfig` calls so rapid toggles do not apply stale `task_script_params`. */
let persistTaskScriptParamsChain: Promise<void> = Promise.resolve();

// ============================================================================
// State + Actions
// ============================================================================

export interface ScriptSlice {
  scripts: ScriptInfo[];
  triggers: ScriptInfo[];
  scriptRunConfigs: Record<string, Record<string, unknown>>;

  refreshScripts: () => Promise<void>;
  refreshTriggers: () => Promise<void>;
  runScript: (name: string, params?: Record<string, unknown>) => Promise<void>;
  stopTask: () => Promise<void>;
  enableTrigger: (name: string, params?: Record<string, unknown>) => Promise<void>;
  disableTrigger: (name: string) => Promise<void>;
  readScriptSource: (scriptPath: string) => Promise<string>;
  saveScriptSource: (scriptPath: string, content: string) => Promise<void>;
  importScriptAsset: (scriptName: string, filePath: string) => Promise<string>;
  createScript: (name: string, displayName: string, scriptType: string, description: string) => Promise<void>;
  deleteScript: (name: string) => Promise<void>;
  listScriptFiles: (scriptDir: string) => Promise<string[]>;
  setScriptRunConfig: (name: string, params: Record<string, unknown>) => void;
  getScriptRunConfig: (name: string) => Record<string, unknown>;
}

// ============================================================================
// State creator
// ============================================================================

export const createScriptSlice: StateCreator<CombinedStore, [], [], ScriptSlice> = (set, get) => ({
  scripts: [],
  triggers: [],
  scriptRunConfigs: {},

  refreshScripts: async () => {
    const raw = await invokeAction<RustScriptInfo[]>("list_scripts", undefined, { silent: true });
    if (raw) set({ scripts: raw.map(mapScriptInfo) });
  },

  refreshTriggers: async () => {
    const raw = await invokeAction<RustScriptInfo[]>("list_triggers", undefined, { silent: true });
    if (raw) set({ triggers: raw.map(mapScriptInfo) });
  },

  runScript: async (name: string, params?: Record<string, unknown>) => {
    const effectiveParams = params ?? get().scriptRunConfigs[name] ?? {};
    set((s) => ({
      status: { ...s.status, state: "running" as const, task: name },
    }));

    if (!localStorage.getItem("betternte-hint-stop-shown")) {
      const config = get().config;
      const key = config?.hotkeys.emergency_stop || "Ctrl+P";
      localStorage.setItem("betternte-hint-stop-shown", "1");
      get().showError(
        "提示",
        `脚本运行中，可随时按下 ${key} 紧急停止。`,
      );
    }

    try {
      await invokeAction("run_script", { name, params: effectiveParams }, {
        showError: true,
        errorTitle: "脚本执行失败",
      }, get().showError);
    } finally {
      await get().refreshStatus();
    }
  },

  stopTask: async () => {
    await invokeAction("stop_task", undefined, { silent: true });
    get().refreshStatus();
  },

  enableTrigger: async (name: string, params?: Record<string, unknown>) => {
    await invokeAction("enable_trigger", { name, params: params ?? {} }, {
      showError: true,
      errorTitle: "启用触发器失败",
    }, get().showError);
  },

  disableTrigger: async (name: string) => {
    await invokeAction("disable_trigger", { name }, { silent: true });
  },

  readScriptSource: async (scriptPath: string) => {
    return await invokeAction<string>("read_script_source", { scriptPath });
  },

  saveScriptSource: async (scriptPath: string, content: string) => {
    await invokeAction("save_script_source", { scriptPath, content });
  },

  importScriptAsset: async (scriptName: string, filePath: string) => {
    return await invokeAction<string>("import_script_asset", { scriptName, filePath });
  },

  createScript: async (name: string, displayName: string, scriptType: string, description: string) => {
    await invokeAction("create_script", { name, displayName, scriptType, description });
    await get().refreshScripts();
    await get().refreshTriggers();
  },

  deleteScript: async (name: string) => {
    await invokeAction(
      "delete_script",
      { name },
      { showError: true, errorTitle: "删除失败" },
      get().showError
    );
    await get().refreshScripts();
    await get().refreshTriggers();
  },

  listScriptFiles: async (scriptDir: string): Promise<string[]> => {
    return await invokeAction<string[]>("list_script_files", { scriptDir });
  },

  setScriptRunConfig: (name: string, params: Record<string, unknown>) => {
    set((s) => ({
      scriptRunConfigs: { ...s.scriptRunConfigs, [name]: params },
    }));

    persistTaskScriptParamsChain = persistTaskScriptParamsChain
      .then(async () => {
        const latestCfg = get().config;
        if (!latestCfg) return;
        try {
          await get().saveConfig({
            ...latestCfg,
            task_script_params: { ...get().scriptRunConfigs },
          });
        } catch (e) {
          await get().loadConfig().catch(() => {});
          get().showError("保存脚本配置失败", String(e));
        }
      })
      .catch(() => {});
  },

  getScriptRunConfig: (name: string) => get().scriptRunConfigs[name] ?? {},
});
