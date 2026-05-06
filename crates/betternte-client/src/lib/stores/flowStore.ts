import type { StateCreator } from "zustand";

import type {
  FlowDefinition,
  TaskGroup,
  TaskGroupProgress,
} from "../types";
import { invokeAction } from "./helpers";
import type { CombinedStore } from "./index";

// ============================================================================
// State + Actions
// ============================================================================

export interface FlowSlice {
  flows: FlowDefinition[];
  taskGroups: TaskGroup[];

  refreshFlows: () => Promise<void>;
  saveFlow: (flow: FlowDefinition) => Promise<void>;
  deleteFlow: (flowId: string) => Promise<void>;
  listTaskGroups: () => Promise<void>;
  saveTaskGroup: (group: Omit<TaskGroup, "uuid"> & { uuid?: string }) => Promise<void>;
  deleteTaskGroup: (name: string) => Promise<void>;
  runFlow: (flowId: string, params?: Record<string, unknown>) => Promise<void>;
  stopFlow: (flowId: string) => Promise<void>;
  getFlowProgress: (flowId: string) => Promise<TaskGroupProgress | null>;
  runTaskGroup: (uuid: string, params?: Record<string, unknown>) => Promise<void>;
  stopTaskGroup: (uuid: string) => Promise<void>;
  getTaskGroupProgress: (uuid: string) => Promise<TaskGroupProgress | null>;
}

// ============================================================================
// State creator
// ============================================================================

export const createFlowSlice: StateCreator<CombinedStore, [], [], FlowSlice> = (set, get) => ({
  flows: [],
  taskGroups: [],

  refreshFlows: async () => {
    const flows = await invokeAction<FlowDefinition[]>("list_flows", undefined, { silent: true });
    if (flows) set({ flows });
  },

  saveFlow: async (flow: FlowDefinition) => {
    await invokeAction("save_flow", { flow }, {
      showError: true,
      errorTitle: "保存工作流失败",
    }, get().showError);
    await get().refreshFlows();
  },

  deleteFlow: async (flowId: string) => {
    await invokeAction("delete_flow", { flowId }, {
      showError: true,
      errorTitle: "删除工作流失败",
      silent: true,
    }, get().showError);
    await get().refreshFlows();
  },

  listTaskGroups: async () => {
    const groups = await invokeAction<TaskGroup[]>("list_task_groups", undefined, { silent: true });
    if (groups) set({ taskGroups: groups });
  },

  saveTaskGroup: async (group) => {
    await invokeAction("save_task_group", { group }, {
      showError: true,
      errorTitle: "保存任务组失败",
    }, get().showError);
    await get().listTaskGroups();
  },

  deleteTaskGroup: async (name: string) => {
    await invokeAction("delete_task_group", { name }, {
      showError: true,
      errorTitle: "删除任务组失败",
    }, get().showError);
    await get().listTaskGroups();
  },

  runFlow: async (flowId: string, params?: Record<string, unknown>) => {
    set((s) => ({
      status: { ...s.status, state: "running" as const, task: flowId, task_type: "flow" },
    }));

    try {
      await invokeAction("run_flow", { flowId, params: params ?? {} }, {
        showError: true,
        errorTitle: "工作流启动失败",
      }, get().showError);
    } finally {
      await get().refreshStatus();
    }
  },

  stopFlow: async (flowId: string) => {
    await invokeAction("stop_flow", { flowId }, { silent: true });
    get().refreshStatus();
  },

  getFlowProgress: async (flowId: string) => {
    return await invokeAction<TaskGroupProgress | null>("get_flow_progress", { flowId }, { silent: true }) ?? null;
  },

  runTaskGroup: async (uuid: string, params?: Record<string, unknown>) => {
    await get().runFlow(uuid, params);
  },

  stopTaskGroup: async (uuid: string) => {
    await get().stopFlow(uuid);
  },

  getTaskGroupProgress: async (uuid: string) => {
    return await get().getFlowProgress(uuid);
  },
});
