import { listen } from "@tauri-apps/api/event";
import type { StateCreator } from "zustand";

import {
  ENGINE_CONTROL_WATCHDOG_INTERVAL_MS,
  ENGINE_CONTROL_WATCHDOG_STALE_MS,
  UI_LOG_SLICE_MAX,
} from "../constants/timing";
import type {
  DebugEntry,
  EngineEvent,
  LogEntry,
  TaskResult,
} from "../types";
import type { CombinedStore } from "./index";

// ============================================================================
// Types
// ============================================================================

export interface ErrorDialogState {
  title: string;
  message: string;
  detail?: string;
}

// ============================================================================
// State + Actions
// ============================================================================

export interface UISlice {
  logs: LogEntry[];
  recentTasks: TaskResult[];
  logDrawerOpen: boolean;
  errorDialog: ErrorDialogState | null;
  controlStreamHealthy: boolean;
  controlStreamStaleMs: number | null;
  lastControlEventAt: number | null;

  toggleLogDrawer: () => void;
  showError: (title: string, message: string, detail?: string) => void;
  closeError: () => void;
  clearLogs: () => void;

  _unlisten: (() => void) | null;
  setupEventListener: () => Promise<void>;
}

// ============================================================================
// State creator
// ============================================================================

export const createUISlice: StateCreator<CombinedStore, [], [], UISlice> = (set, get) => ({
  logs: [],
  recentTasks: [],
  logDrawerOpen: false,
  errorDialog: null,
  controlStreamHealthy: true,
  controlStreamStaleMs: null,
  lastControlEventAt: null,
  _unlisten: null,

  toggleLogDrawer: () => set((s) => ({ logDrawerOpen: !s.logDrawerOpen })),

  showError: (title: string, message: string, detail?: string) => {
    set({ errorDialog: { title, message, detail } });
  },

  closeError: () => {
    set({ errorDialog: null });
  },

  clearLogs: () => {
    set({ logs: [] });
  },

  setupEventListener: async () => {
    const prev = get()._unlisten;
    if (prev) prev();

    let statusResyncTimer: ReturnType<typeof setTimeout> | null = null;
    let controlWatchdogTimer: ReturnType<typeof setInterval> | null = null;
    let lastControlEventAt = Date.now();
    let watchdogWarned = false;
    const markControlHeartbeat = () => {
      lastControlEventAt = Date.now();
      watchdogWarned = false;
      set({
        controlStreamHealthy: true,
        controlStreamStaleMs: 0,
        lastControlEventAt,
      });
    };
    /** Long-running scripts rarely emit control-class events; treat engine activity as stream-alive while running. */
    const bumpControlStreamActivity = () => {
      if (get().status.state !== "running") return;
      lastControlEventAt = Date.now();
      watchdogWarned = false;
    };
    const scheduleStatusResync = () => {
      if (statusResyncTimer) clearTimeout(statusResyncTimer);
      statusResyncTimer = setTimeout(() => {
        void get().refreshStatus();
      }, 120);
    };
    const isControlEvent = (event: EngineEvent): boolean => {
      return (
        event.type === "TaskStarted" ||
        event.type === "TaskStopped" ||
        event.type === "CaptureStatusChanged" ||
        event.type === "ConfigChanged" ||
        event.type === "Error"
      );
    };

    const handleEvent = (data: EngineEvent) => {
      switch (data.type) {
        case "TaskStarted": {
          markControlHeartbeat();
          const d = data.data;
          set((s) => ({
            status: { ...s.status, state: "running", task: d.task_name },
            logs: [
              ...s.logs,
              {
                level: "info",
                message: `任务开始: ${d.task_name}`,
                timestamp: new Date(d.timestamp).toLocaleTimeString("zh-CN"),
              },
            ],
          }));
          scheduleStatusResync();
          break;
        }
        case "TaskStopped": {
          markControlHeartbeat();
          const d = data.data;
          set((s) => ({
            status: { ...s.status, task: null },
            recentTasks: [
              {
                name: d.task_name,
                success: d.reason === "completed",
                duration_ms: d.duration_ms,
                started_at: "",
                completed_at: new Date(d.timestamp).toISOString(),
              },
              ...s.recentTasks,
            ].slice(0, 20),
            logs: [
              ...s.logs,
              {
                level: d.reason === "completed" ? "info" : "warn",
                message: `任务停止: ${d.task_name} (${d.reason})`,
                timestamp: new Date(d.timestamp).toLocaleTimeString("zh-CN"),
              },
            ],
          }));
          scheduleStatusResync();
          break;
        }
        case "TaskProgress": {
          bumpControlStreamActivity();
          const d = data.data;
          set((s) => ({
            status: {
              ...s.status,
              progress: { current: d.current, total: d.total, message: d.message },
            },
          }));
          break;
        }
        case "LogMessage": {
          bumpControlStreamActivity();
          const d = data.data;
          set((s) => ({
            logs: [
              ...s.logs,
              {
                level: d.level as LogEntry["level"],
                message: d.message,
                timestamp: new Date(d.timestamp).toLocaleTimeString("zh-CN"),
              },
            ].slice(-UI_LOG_SLICE_MAX),
          }));
          break;
        }
        case "Error": {
          markControlHeartbeat();
          const d = data.data;
          set((s) => ({
            logs: [
              ...s.logs,
              {
                level: "error",
                message: `[${d.module}] ${d.message}`,
                timestamp: new Date().toLocaleTimeString("zh-CN"),
              },
            ],
          }));
          scheduleStatusResync();
          break;
        }
        case "CaptureStatusChanged": {
          markControlHeartbeat();
          scheduleStatusResync();
          break;
        }
        case "ConfigChanged": {
          markControlHeartbeat();
          void get().loadConfig();
          scheduleStatusResync();
          break;
        }
        case "ScriptLoaded": {
          get().refreshScripts();
          break;
        }
        case "ScriptCallTrace": {
          bumpControlStreamActivity();
          const d = data.data;
          get().addDebugEntry({
            id: d.id,
            category: d.category as DebugEntry["category"],
            method: d.method,
            args: d.args,
            result: d.result,
            success: d.success,
            error: d.error,
            screenshotBefore: d.screenshot_before,
            screenshotAfter: d.screenshot_after,
            durationMs: d.duration_ms,
            timestamp: new Date(d.timestamp),
          });
          break;
        }
      }
    };

    const unlistenData = await listen<EngineEvent>("engine-event", (event) => {
      const data = event.payload;
      if (isControlEvent(data)) {
        // Control-class events are handled by dedicated engine-control-event.
        return;
      }
      handleEvent(data);
    });

    const unlistenControl = await listen<EngineEvent>("engine-control-event", (event) => {
      handleEvent(event.payload);
    });

    controlWatchdogTimer = setInterval(() => {
      const now = Date.now();
      const state = get().status.state;
      if (state !== "running") {
        watchdogWarned = false;
        set({
          controlStreamHealthy: true,
          controlStreamStaleMs: null,
        });
        return;
      }

      const staleMs = now - lastControlEventAt;
      if (staleMs > ENGINE_CONTROL_WATCHDOG_STALE_MS) {
        set({
          controlStreamHealthy: false,
          controlStreamStaleMs: staleMs,
          lastControlEventAt,
        });
        void get().refreshStatus();
        if (!watchdogWarned) {
          set((s) => ({
            logs: [
              ...s.logs,
              {
                level: "warn" as LogEntry["level"],
                message: `控制事件流超过 ${Math.floor(staleMs / 1000)}s 未更新，已触发状态重拉`,
                timestamp: new Date().toLocaleTimeString("zh-CN"),
              },
            ].slice(-UI_LOG_SLICE_MAX),
          }));
          watchdogWarned = true;
        }
      } else {
        set({
          controlStreamHealthy: true,
          controlStreamStaleMs: staleMs,
          lastControlEventAt,
        });
      }
    }, ENGINE_CONTROL_WATCHDOG_INTERVAL_MS);

    set({
      _unlisten: () => {
        unlistenData();
        unlistenControl();
        if (statusResyncTimer) {
          clearTimeout(statusResyncTimer);
          statusResyncTimer = null;
        }
        if (controlWatchdogTimer) {
          clearInterval(controlWatchdogTimer);
          controlWatchdogTimer = null;
        }
      },
    });
  },
});
