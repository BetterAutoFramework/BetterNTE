import {
  FolderTree,
  List,
  ListTodo,
  Loader2,
  PanelLeftClose,
  PanelLeftOpen,
  Play,
  RefreshCw,
  Square,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { PathTree, type PathTreeItem } from "@/components/PathTree";
import { ScriptManifestMetaSection } from "@/components/ScriptManifestMeta";
import { SchemaForm } from "@/components/SchemaForm";
import { HelpHint } from "@/components/ui/HelpHint";
import { HotkeyInput } from "@/components/ui/HotkeyInput";
import { PANEL_THRESHOLD_PX } from "@/lib/constants/layout";
import { findScriptShortcut, upsertScriptHotkey } from "@/lib/hotkeyTriggers";
import { useEngineStore } from "@/lib/store";
import { scriptKey } from "@/lib/stores/helpers";
import { buildScriptTreePath } from "@/lib/treePaths";
import type { EngineConfig, ScriptInfo } from "@/lib/types";
import { cn } from "@/lib/utils";

// ============================================================================
// TaskPage — dual-panel: left list + right config
// ============================================================================

export function TaskPage() {
  const scripts = useEngineStore((s) => s.scripts);
  const status = useEngineStore((s) => s.status);
  const runScript = useEngineStore((s) => s.runScript);
  const stopTask = useEngineStore((s) => s.stopTask);
  const refreshScripts = useEngineStore((s) => s.refreshScripts);
  const initialized = useEngineStore((s) => s.initialized);
  const initEngine = useEngineStore((s) => s.initEngine);
  const config = useEngineStore((s) => s.config);
  const saveConfig = useEngineStore((s) => s.saveConfig);
  const scriptRunConfigs = useEngineStore((s) => s.scriptRunConfigs);
  const setScriptRunConfig = useEngineStore((s) => s.setScriptRunConfig);

  const [selected, setSelected] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<"list" | "tree">("list");
  const [panelCollapsed, setPanelCollapsed] = useState(false);
  const manualPanelRef = useRef(false);

  // Auto-collapse left panel on narrow window
  useEffect(() => {
    const onResize = () => {
      if (window.innerWidth < PANEL_THRESHOLD_PX) {
        setPanelCollapsed(true);
      } else if (!manualPanelRef.current) {
        setPanelCollapsed(false);
      }
    };
    onResize();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  useEffect(() => {
    if (!initialized) {
      initEngine();
    } else {
      refreshScripts();
    }
  }, [initialized, initEngine, refreshScripts]);

  // Filter task-type scripts
  const tasks = scripts.filter((s) => s.type === "task" || s.type === "solo_task");
  const taskTreeItems = useMemo<PathTreeItem<ScriptInfo>[]>(
    () =>
      tasks.map((task) => ({
        id: scriptKey(task),
        label: task.display_name,
        path: buildScriptTreePath(task),
        data: task,
      })),
    [tasks]
  );
  const taskSourceGroups = useMemo(() => {
    const groups = new Map<string, ScriptInfo[]>();
    for (const task of tasks) {
      const source = task.source?.trim() || "未分类";
      if (!groups.has(source)) groups.set(source, []);
      groups.get(source)!.push(task);
    }
    return Array.from(groups.entries())
      .sort(([a], [b]) => a.localeCompare(b, "zh-CN"))
      .map(([source, items]) => ({ source, items }));
  }, [tasks]);

  // Auto-select first task if none selected
  useEffect(() => {
    if (!selected && tasks.length > 0) {
      setSelected(scriptKey(tasks[0]));
    }
  }, [tasks, selected]);

  // Clear stale selection after delete or reload
  useEffect(() => {
    if (tasks.length === 0) {
      setSelected(null);
      return;
    }
    const exists = tasks.some((t) => scriptKey(t) === selected);
    if (selected && !exists) {
      setSelected(scriptKey(tasks[0]));
    }
  }, [tasks, selected]);

  const runningTask = status.task;
  const selectedTask = tasks.find((t) => scriptKey(t) === selected);

  if (!initialized) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="w-6 h-6 animate-spin text-primary" />
      </div>
    );
  }

  return (
    <div className="flex h-full">
      {/* Left panel — task list */}
      {panelCollapsed ? (
        <div className="flex flex-col items-center py-3 px-1.5 border-r border-border-subtle shrink-0">
          <button
            onClick={() => { manualPanelRef.current = true; setPanelCollapsed(false); }}
            className="p-2 rounded-md hover:bg-surface-hover text-foreground-tertiary hover:text-foreground"
            title="展开列表"
          >
            <PanelLeftOpen className="w-4 h-4" />
          </button>
        </div>
      ) : (
      <div className="w-72 border-r border-border-subtle flex flex-col shrink-0">
        <div className="flex items-center px-4 py-3 border-b border-border-subtle">
          <div className="rounded-md border border-border-subtle bg-surface p-0.5 flex">
              <button
                onClick={() => setViewMode("list")}
                className={cn(
                  "p-1.5 rounded",
                  viewMode === "list"
                    ? "bg-card text-foreground"
                    : "text-foreground-tertiary hover:text-foreground"
                )}
                title="列表视图"
              >
                <List className="w-3.5 h-3.5" />
              </button>
              <button
                onClick={() => setViewMode("tree")}
                className={cn(
                  "p-1.5 rounded",
                  viewMode === "tree"
                    ? "bg-card text-foreground"
                    : "text-foreground-tertiary hover:text-foreground"
                )}
                title="树状视图"
              >
                <FolderTree className="w-3.5 h-3.5" />
              </button>
          </div>
          <div className="ml-auto flex items-center gap-1">
            <button
              onClick={refreshScripts}
              className="p-1.5 rounded-md hover:bg-surface-hover text-foreground-tertiary hover:text-foreground"
            >
              <RefreshCw className="w-3.5 h-3.5" />
            </button>
            <button
              onClick={() => { manualPanelRef.current = true; setPanelCollapsed(true); }}
              className="p-1.5 rounded-md hover:bg-surface-hover text-foreground-tertiary hover:text-foreground"
              title="收起列表"
            >
              <PanelLeftClose className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto p-2 space-y-1">
          {taskTreeItems.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12 text-center px-4">
              <ListTodo className="w-8 h-8 text-foreground-tertiary/30 mb-3" />
              <p className="text-xs text-foreground-tertiary">
                暂无脚本
              </p>
            </div>
          ) : (
            viewMode === "tree" ? (
              <PathTree
                items={taskTreeItems}
                selectedId={selected}
                onSelect={(item) => setSelected(item.id)}
                emptyText="暂无脚本"
                renderLeaf={({ item, isSelected, onSelect }) => {
                  const task = item.data;
                  const isRunning = runningTask === scriptKey(task);
                  return (
                    <button
                      onClick={onSelect}
                      className={cn(
                        "w-full text-left rounded-md px-3 py-2.5 transition-colors",
                        isSelected
                          ? "bg-primary/10 border border-primary/20"
                          : "border border-transparent hover:bg-surface-hover"
                      )}
                    >
                      <div className="flex items-center gap-2.5">
                        <div
                          className={cn(
                            "w-7 h-7 rounded-md flex items-center justify-center shrink-0",
                            isRunning
                              ? "bg-primary/10 text-primary"
                              : "bg-surface text-foreground-tertiary"
                          )}
                        >
                          {isRunning ? (
                            <Loader2 className="w-3.5 h-3.5 animate-spin" />
                          ) : (
                            <ListTodo className="w-3.5 h-3.5" />
                          )}
                        </div>
                        <div className="min-w-0 flex-1">
                          <div className="text-sm text-foreground truncate">
                            {task.display_name}
                          </div>
                          <div className="text-xs text-foreground-tertiary truncate">
                            {task.description || task.name}
                          </div>
                        </div>
                        {isRunning && (
                          <div className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse shrink-0" />
                        )}
                      </div>
                    </button>
                  );
                }}
              />
            ) : (
              <div className="space-y-2">
                {taskSourceGroups.map(({ source, items }) => (
                  <div key={source}>
                    <div className="px-2 py-1 text-[10px] font-medium text-foreground-tertiary/70 uppercase tracking-wider">
                      {source} ({items.length})
                    </div>
                    <div className="space-y-1">
                      {items.map((task) => {
                        const id = scriptKey(task);
                        const isRunning = runningTask === scriptKey(task);
                        const isSelected = selected === id;
                        return (
                          <button
                            key={id}
                            onClick={() => setSelected(id)}
                            className={cn(
                              "w-full text-left rounded-md px-3 py-2.5 transition-colors",
                              isSelected
                                ? "bg-primary/10 border border-primary/20"
                                : "border border-transparent hover:bg-surface-hover"
                            )}
                          >
                            <div className="flex items-center gap-2.5">
                              <div
                                className={cn(
                                  "w-7 h-7 rounded-md flex items-center justify-center shrink-0",
                                  isRunning
                                    ? "bg-primary/10 text-primary"
                                    : "bg-surface text-foreground-tertiary"
                                )}
                              >
                                {isRunning ? (
                                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                                ) : (
                                  <ListTodo className="w-3.5 h-3.5" />
                                )}
                              </div>
                              <div className="min-w-0 flex-1">
                                <div className="text-sm text-foreground truncate">
                                  {task.display_name}
                                </div>
                                <div className="text-xs text-foreground-tertiary truncate">
                                  {task.description || task.name}
                                </div>
                              </div>
                              {isRunning && (
                                <div className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse shrink-0" />
                              )}
                            </div>
                          </button>
                        );
                      })}
                    </div>
                  </div>
                ))}
              </div>
            )
          )}
        </div>
      </div>
      )}

      {/* Right panel — detail */}
      <div className="flex-1 overflow-y-auto">
        {selectedTask ? (
          <TaskDetail
            task={selectedTask}
            running={runningTask === scriptKey(selectedTask)}
            engineConfig={config}
            scriptParams={scriptRunConfigs[scriptKey(selectedTask)] ?? {}}
            onRun={() =>
              runScript(scriptKey(selectedTask), scriptRunConfigs[scriptKey(selectedTask)] ?? {})
            }
            onStop={() => stopTask()}
            onScriptParamsChange={(params) =>
              setScriptRunConfig(scriptKey(selectedTask), params)
            }
            saveEngineConfig={saveConfig}
          />
        ) : (
          <div className="flex items-center justify-center h-full text-foreground-tertiary text-sm">
            选择一个脚本查看详情
          </div>
        )}
      </div>
    </div>
  );
}

// ============================================================================
// Task detail panel
// ============================================================================

function TaskDetail({
  task,
  running,
  engineConfig,
  scriptParams,
  onRun,
  onStop,
  onScriptParamsChange,
  saveEngineConfig,
}: {
  task: ScriptInfo;
  running: boolean;
  engineConfig: EngineConfig | null;
  scriptParams: Record<string, unknown>;
  onRun: () => void;
  onStop: () => void;
  onScriptParamsChange: (params: Record<string, unknown>) => void;
  saveEngineConfig: (config: EngineConfig) => Promise<void>;
}) {
  const schema: Record<string, unknown> = task.params_schema ?? {
    type: "object",
    properties: {},
  };

  return (
    <div className="p-6 max-w-2xl">
      {/* Header */}
      <div className="flex items-center justify-between mb-6 gap-3 flex-wrap">
        <div className="flex items-center gap-3 min-w-0">
          <div
            className={cn(
              "w-10 h-10 rounded-lg flex items-center justify-center shrink-0",
              running
                ? "bg-primary/10 text-primary"
                : "bg-surface text-foreground-tertiary"
            )}
          >
            {running ? (
              <Loader2 className="w-5 h-5 animate-spin" />
            ) : (
              <ListTodo className="w-5 h-5" />
            )}
          </div>
          <div className="min-w-0">
            <h1 className="text-lg font-semibold text-foreground truncate">
              {task.display_name}
            </h1>
            <p className="text-xs text-foreground-tertiary font-mono">v{task.version}</p>
          </div>
        </div>

        <div className="flex items-center gap-2 shrink-0">
          {running ? (
            <button
              type="button"
              onClick={onStop}
              className="flex items-center gap-1.5 px-4 py-2 rounded-md bg-destructive text-destructive-foreground text-sm font-medium hover:bg-destructive/90"
            >
              <Square className="w-3.5 h-3.5" />
              停止
            </button>
          ) : (
            <button
              type="button"
              onClick={onRun}
              className="flex items-center gap-1.5 px-4 py-2 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary-hover"
            >
              <Play className="w-3.5 h-3.5" />
              运行
            </button>
          )}
        </div>
      </div>

      {/* Status */}
      <div className="flex items-center gap-2 mb-6">
        {running ? (
          <div className="px-2.5 py-1 rounded-full text-xs font-medium bg-primary/10 text-primary">
            运行中
          </div>
        ) : null}
      </div>

      {/* Description */}
      <div className="mb-6">
        <h3 className="text-sm font-medium text-foreground mb-3">描述</h3>
        <div className="rounded-lg border border-border-subtle bg-card p-4 text-sm text-foreground-secondary leading-relaxed">
          {task.description?.trim() || "暂无描述"}
        </div>
      </div>

      {/* Config */}
      <div>
        <h3 className="text-sm font-medium text-foreground mb-3">配置</h3>
        <div className="rounded-lg border border-border-subtle bg-card p-4">
          <SchemaForm
            schema={schema}
            values={scriptParams}
            disabled={running}
            onChange={onScriptParamsChange}
            emptyMessage="此脚本没有可配置的选项"
          />
        </div>
      </div>

      <ScriptManifestMetaSection script={task} className="mt-6" showDescription={false} />

      {engineConfig && (
        <div className="mt-6">
          <div className="flex items-center gap-1.5 mb-2">
            <h3 className="text-sm font-medium text-foreground">全局快捷键</h3>
            <HelpHint text="按下可启动该脚本；若该脚本正在单独运行，再按一次可停止。不能与其他全局快捷键重复。" />
          </div>
          <div className="rounded-lg border border-border-subtle bg-card p-4 flex flex-wrap items-center gap-3">
            <HotkeyInput
              value={findScriptShortcut(engineConfig, scriptKey(task))}
              onChange={async (v) => {
                const next = upsertScriptHotkey(engineConfig, scriptKey(task), v);
                await saveEngineConfig(next);
              }}
              disabled={running}
            />
            <button
              type="button"
              disabled={running}
              onClick={async () => {
                const next = upsertScriptHotkey(engineConfig, scriptKey(task), "");
                await saveEngineConfig(next);
              }}
              className="text-xs text-foreground-tertiary hover:text-foreground disabled:opacity-50"
            >
              清除
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
