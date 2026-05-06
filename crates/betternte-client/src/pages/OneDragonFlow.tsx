import {
  CheckCircle2,
  Circle,
  FolderTree,
  List,
  Loader2,
  Monitor,
  PanelLeftClose,
  PanelLeftOpen,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  Square,
  Trash2,
  Turtle,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { ConfirmDialog } from "@/components/ConfirmDialog";
import { NewGroupDialog } from "@/components/NewGroupDialog";
import { PathTree, type PathTreeItem } from "@/components/PathTree";
import { TaskGroupPermissionsUnionSection } from "@/components/ScriptManifestMeta";
import { HelpHint } from "@/components/ui/HelpHint";
import { HotkeyInput } from "@/components/ui/HotkeyInput";
import { PANEL_THRESHOLD_PX } from "@/lib/constants/layout";
import { UI_POLL_FAST_MS } from "@/lib/constants/timing";
import { findTaskGroupShortcut, upsertTaskGroupHotkey } from "@/lib/hotkeyTriggers";
import { useEngineStore } from "@/lib/store";
import { scriptKey, scriptRunConfigLookup } from "@/lib/stores/helpers";
import type {
  EngineConfig,
  GameWindow,
  ScriptInfo,
  TaskGroup,
  TaskGroupNode,
  TaskGroupProgress,
} from "@/lib/types";
import { cn } from "@/lib/utils";

function normalizeTaskGroupId(value: string): string {
  return value.replaceAll("\\", "/").replace(/^\/+|\/+$/g, "");
}

function getTaskGroupTreePath(group: TaskGroup): string {
  const normalized = normalizeTaskGroupId(group.uuid || group.name);
  const parts = normalized.split("/").filter(Boolean);
  if (parts.length <= 1) return "";
  return parts.slice(0, -1).join("/");
}

/** 数据源：优先引擎返回的 `source`（订阅 / 插件名），否则用路径首段。 */
function getTaskGroupSource(group: TaskGroup): string {
  const normalizeSourceLabel = (raw: string): string => {
    const source = raw.trim();
    if (!source) return "未分类";
    if (source === "main") return "官方源";
    if (source === "local") return "本地源";
    if (source === "legacy_task_group") return "兼容任务组";
    return source;
  };

  if (group.source?.trim()) return normalizeSourceLabel(group.source);
  const normalized = normalizeTaskGroupId(group.uuid || group.name);
  const parts = normalized.split("/").filter(Boolean);
  if (parts.length <= 1) return "未分类";
  return normalizeSourceLabel(parts[0] || "");
}

function TaskStatusIcon({ nodeAlias, progress }: { nodeAlias: string; progress: TaskGroupProgress | null }) {
  if (!progress) return <Circle className="w-4 h-4 text-foreground-tertiary" />;
  const status = progress.node_status[nodeAlias];
  if (!status) return <Circle className="w-4 h-4 text-foreground-tertiary" />;
  switch (status.status) {
    case "completed":
      return <CheckCircle2 className="w-4 h-4 text-success" />;
    case "running":
      return <Play className="w-4 h-4 text-primary animate-pulse" />;
    case "failed":
      return <Circle className="w-4 h-4 text-destructive" />;
    default:
      return <Circle className="w-4 h-4 text-foreground-tertiary" />;
  }
}

function GroupListItem({
  group,
  isSelected,
  onSelect,
  isRunning,
}: {
  group: TaskGroup;
  isSelected: boolean;
  onSelect: () => void;
  isRunning?: boolean;
}) {
  return (
    <button
      type="button"
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
            isRunning ? "bg-primary/10 text-primary" : "bg-surface text-foreground-tertiary"
          )}
        >
          {isRunning ? (
            <Loader2 className="w-3.5 h-3.5 animate-spin" />
          ) : (
            <Turtle className="w-3.5 h-3.5" />
          )}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-sm text-foreground truncate">{group.name}</span>
            <span className="text-[10px] shrink-0 px-1.5 py-0.5 rounded bg-surface text-foreground-tertiary">
              {group.mode === "sequential" ? "串行" : "随机"}
            </span>
          </div>
          <div className="text-xs text-foreground-tertiary truncate">
            {getTaskGroupSource(group)} · {group.nodes.length} 个节点
          </div>
        </div>
        {isRunning && (
          <div className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse shrink-0" />
        )}
      </div>
    </button>
  );
}

function TaskItemRow({
  node,
  progress,
}: {
  node: TaskGroupNode;
  progress: TaskGroupProgress | null;
}) {
  const status = progress?.node_status[node.alias]?.status;
  const isRunning = status === "running";
  return (
    <div
      className={cn(
        "flex items-center gap-3 py-2 px-3 rounded-md transition-colors",
        isRunning
          ? "bg-primary/10 border border-primary/20"
          : "border border-transparent hover:bg-surface-hover"
      )}
    >
      <TaskStatusIcon nodeAlias={node.alias} progress={progress} />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className={cn("text-sm", isRunning ? "text-primary font-medium" : "text-foreground")}>
            {node.alias}
          </span>
          <span className="text-xs text-foreground-tertiary font-mono">
            {node.script}
          </span>
        </div>
      </div>
      {node.timeout_ms && (
        <span className="text-xs text-foreground-tertiary shrink-0">超时 {node.timeout_ms}ms</span>
      )}
    </div>
  );
}

function WindowInfoCard({ window }: { window: GameWindow | null }) {
  if (!window) return null;
  return (
    <div className="rounded-lg border border-border-subtle bg-card p-4 flex items-center gap-3">
      <Monitor className="w-5 h-5 text-primary shrink-0" />
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium text-foreground truncate">{window.title}</div>
        <div className="text-xs text-foreground-tertiary">
          {window.process_name} · HWND {window.hwnd}
        </div>
      </div>
      <div className="text-xs text-success px-2 py-0.5 rounded-full bg-success/10">
        已匹配
      </div>
    </div>
  );
}

function GroupDetail({
  group,
  progress,
  onStart,
  onStop,
  running,
  matchedWindow,
  engineConfig,
  saveEngineConfig,
  scripts,
  onEdit,
  onRequestDelete,
}: {
  group: TaskGroup;
  progress: TaskGroupProgress | null;
  onStart: () => void;
  onStop: () => void;
  running: boolean;
  matchedWindow: GameWindow | null;
  engineConfig: EngineConfig | null;
  saveEngineConfig: (config: EngineConfig) => Promise<void>;
  scripts: ScriptInfo[];
  onEdit: () => void;
  onRequestDelete: () => void;
}) {
  const scheduleText = group.schedule
    ? group.schedule.type === "weekly"
      ? `每周 ${group.schedule.days_of_week?.join(",") ?? "-"} ${String(group.schedule.hour ?? 0).padStart(2, "0")}:${String(group.schedule.minute ?? 0).padStart(2, "0")}`
      : `每日 ${String(group.schedule.hour ?? 0).padStart(2, "0")}:${String(group.schedule.minute ?? 0).padStart(2, "0")}`
    : "一次性/手动";

  return (
    <div className="p-6 max-w-2xl space-y-4">
      <div className="rounded-lg border border-border-subtle bg-card p-5">
        <div className="flex items-center justify-between mb-1 gap-2 flex-wrap">
          <h3 className="text-base font-semibold text-foreground min-w-0 truncate">{group.name}</h3>
          <div className="flex items-center gap-2 shrink-0">
            <button
              type="button"
              onClick={onEdit}
              disabled={running}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-surface border border-border text-foreground text-sm font-medium hover:bg-surface-hover disabled:opacity-50"
            >
              <Pencil className="w-3.5 h-3.5" />
              编辑
            </button>
            <button
              type="button"
              onClick={onRequestDelete}
              disabled={running}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-surface border border-border text-destructive text-sm font-medium hover:bg-destructive/10 disabled:opacity-50"
            >
              <Trash2 className="w-3.5 h-3.5" />
              删除
            </button>
            {running ? (
              <button
                type="button"
                onClick={onStop}
                className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-destructive text-destructive-foreground text-sm font-medium hover:bg-destructive/90"
              >
                <Square className="w-3.5 h-3.5" />
                停止
              </button>
            ) : (
              <button
                type="button"
                onClick={onStart}
                className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary-hover"
              >
                <Play className="w-3.5 h-3.5" />
                启动
              </button>
            )}
          </div>
        </div>
        <div className="space-y-1 mb-4">
          {group.nodes.map((node) => (
            <TaskItemRow key={node.alias} node={node} progress={progress} />
          ))}
        </div>
      </div>

      <div className="rounded-lg border border-border-subtle bg-card p-4">
        <div className="text-sm font-medium text-foreground mb-3">高级配置</div>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <ConfigKV label="错误处理" value={group.error_handling === "skip" ? "跳过" : "中断"} />
          <ConfigKV
            label="重试"
            value={
              group.retry?.enabled
                ? `开启（间隔 ${group.retry.interval_ms}ms，次数 ${group.retry.count}）`
                : "关闭"
            }
          />
          <ConfigKV label="失败通知" value={group.notify_on_failure ? "开启" : "关闭"} />
          <ConfigKV
            label="重复调度策略"
            value={group.repeat_strategy === "interrupt" ? "中断上次并启动新任务" : "跳过本次"}
          />
          <ConfigKV label="调度" value={scheduleText} />
          <ConfigKV label="模式" value={group.mode === "sequential" ? "顺序执行" : "随机执行"} />
        </div>
      </div>

      <TaskGroupPermissionsUnionSection nodes={group.nodes} scripts={scripts} />

      {engineConfig && (
        <div className="rounded-lg border border-border-subtle bg-card p-4">
          <div className="flex items-center gap-1.5 mb-2">
            <div className="text-sm font-medium text-foreground">全局快捷键</div>
            <HelpHint text="按下可启动该任务组；若该任务组正在运行，再按一次可停止。" />
          </div>
          <div className="flex flex-wrap items-center gap-3 rounded-md border border-border-subtle bg-surface/50 p-3">
            <HotkeyInput
              value={findTaskGroupShortcut(engineConfig, group.uuid)}
              onChange={async (v) => {
                const next = upsertTaskGroupHotkey(engineConfig, group.uuid, v);
                await saveEngineConfig(next);
              }}
              disabled={running}
            />
            <button
              type="button"
              disabled={running}
              onClick={async () => {
                const next = upsertTaskGroupHotkey(engineConfig, group.uuid, "");
                await saveEngineConfig(next);
              }}
              className="text-xs text-foreground-tertiary hover:text-foreground disabled:opacity-50"
            >
              清除
            </button>
          </div>
        </div>
      )}

      {/* 匹配到的窗口信息 */}
      <WindowInfoCard window={matchedWindow} />
    </div>
  );
}

function ConfigKV({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border border-border-subtle bg-surface/40 px-3 py-2">
      <div className="text-xs text-foreground-tertiary">{label}</div>
      <div className="text-sm text-foreground mt-0.5">{value}</div>
    </div>
  );
}

export function OneDragonFlow() {
  const taskGroups = useEngineStore((s) => s.taskGroups);
  const listTaskGroups = useEngineStore((s) => s.listTaskGroups);
  const saveTaskGroup = useEngineStore((s) => s.saveTaskGroup);
  const deleteTaskGroup = useEngineStore((s) => s.deleteTaskGroup);
  const runTaskGroup = useEngineStore((s) => s.runTaskGroup);
  const stopTaskGroup = useEngineStore((s) => s.stopTaskGroup);
  const getTaskGroupProgress = useEngineStore((s) => s.getTaskGroupProgress);
  const findGameWindow = useEngineStore((s) => s.findGameWindow);
  const scripts = useEngineStore((s) => s.scripts);
  const initialized = useEngineStore((s) => s.initialized);
  const initEngine = useEngineStore((s) => s.initEngine);
  const refreshScripts = useEngineStore((s) => s.refreshScripts);
  const config = useEngineStore((s) => s.config);
  const saveConfig = useEngineStore((s) => s.saveConfig);
  const scriptRunConfigs = useEngineStore((s) => s.scriptRunConfigs);
  const [selectedGroupId, setSelectedGroupId] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<"list" | "tree">("list");
  const [runningGroup, setRunningGroup] = useState<string | null>(null);
  const [progress, setProgress] = useState<TaskGroupProgress | null>(null);
  const [matchedWindow, setMatchedWindow] = useState<GameWindow | null>(null);
  const [, setLoading] = useState(false);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [editingGroup, setEditingGroup] = useState<TaskGroup | null>(null);
  const [groupToDelete, setGroupToDelete] = useState<TaskGroup | null>(null);
  const [panelCollapsed, setPanelCollapsed] = useState(false);
  const manualPanelRef = useRef(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

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
      listTaskGroups();
      refreshScripts();
    }
  }, [initialized, initEngine, listTaskGroups, refreshScripts]);

  useEffect(() => {
    if (taskGroups.length > 0 && !selectedGroupId) {
      setSelectedGroupId(taskGroups[0].uuid);
      return;
    }
    if (selectedGroupId && !taskGroups.some((group) => group.uuid === selectedGroupId)) {
      setSelectedGroupId(taskGroups[0]?.uuid ?? null);
    }
  }, [taskGroups, selectedGroupId]);

  const selectedGroup = taskGroups.find((group) => group.uuid === selectedGroupId) ?? null;
  const taskGroupTreeItems: PathTreeItem<TaskGroup>[] = taskGroups.map((group) => ({
    id: group.uuid,
    label: group.name,
    path: getTaskGroupTreePath(group),
    data: group,
  }));
  const taskGroupSourceGroups = taskGroups.reduce((acc, group) => {
    const source = getTaskGroupSource(group);
    const bucket = acc.get(source) ?? [];
    bucket.push(group);
    acc.set(source, bucket);
    return acc;
  }, new Map<string, TaskGroup[]>());
  const sortedSourceGroups = Array.from(taskGroupSourceGroups.entries())
    .sort(([a], [b]) => a.localeCompare(b, "zh-CN"))
    .map(([source, groups]) => ({ source, groups }));

  // Poll progress while a task group is running
  const startPolling = useCallback((uuid: string) => {
    if (pollRef.current) clearInterval(pollRef.current);
    pollRef.current = setInterval(async () => {
      const p = await getTaskGroupProgress(uuid);
      setProgress(p);
      // If no progress returned, the group finished
      if (!p) {
        if (pollRef.current) clearInterval(pollRef.current);
        pollRef.current = null;
        setRunningGroup(null);
        setLoading(false);
        setProgress(null);
      }
    }, UI_POLL_FAST_MS);
  }, [getTaskGroupProgress]);

  const stopPolling = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  // Cleanup polling on unmount
  useEffect(() => {
    return () => stopPolling();
  }, [stopPolling]);

  const handleStart = async () => {
    if (!selectedGroup) return;
    setLoading(true);
    setRunningGroup(selectedGroup.uuid);
    setProgress(null);
    setMatchedWindow(null);

    // 先查找游戏窗口
    const window = await findGameWindow();
    if (window) {
      setMatchedWindow(window);
    }

    startPolling(selectedGroup.uuid);
    try {
      const nodeParams = Object.fromEntries(
        selectedGroup.nodes.map((node) => {
          const base = scriptRunConfigLookup(scriptRunConfigs, node.script, scripts);
          const override = node.params ?? {};
          return [node.alias, { ...base, ...override }];
        })
      );
      await runTaskGroup(selectedGroup.uuid, { node_params: nodeParams });
    } catch (e) {
      // Error is already shown by store's runTaskGroup
      console.error("Task group failed:", e);
      stopPolling();
      setLoading(false);
      setRunningGroup(null);
      setProgress(null);
    }
  };

  const handleStop = async () => {
    if (!runningGroup) return;
    await stopTaskGroup(runningGroup);
    stopPolling();
    setRunningGroup(null);
    setProgress(null);
  };

  // Only task-type scripts for selection (not triggers)
  const taskScripts = scripts.filter(
    (s) => s.type === "task" || s.type === "solo_task"
  );

  const handleCreateGroup = (data: {
    directory: string;
    name: string;
    description: string;
    mode: "sequential" | "random";
    scripts: string[];
    error_handling: "interrupt" | "skip";
    retry: { enabled: boolean; interval_ms: number; count: number };
    notify_on_failure: boolean;
    schedule: import("@/lib/types").ScheduleConfig | null;
    repeat_strategy: "skip" | "interrupt";
  }) => {
    const cleanDir = data.directory.replaceAll("\\", "/").replace(/^\/+|\/+$/g, "");
    const uuid = cleanDir ? `${cleanDir}/${data.name}` : data.name;
    const group: TaskGroup = {
      uuid,
      name: data.name,
      description: data.description,
      mode: data.mode,
      retry_count: data.retry.count,
      source: "local",
      nodes: data.scripts.map((scriptRef) => {
        const script =
          scripts.find((s) => scriptKey(s) === scriptRef) ??
          scripts.find((s) => s.name === scriptRef);
        const id = script ? scriptKey(script) : scriptRef;
        return {
          script: id,
          alias: script?.display_name ?? scriptRef,
        };
      }),
      error_handling: data.error_handling,
      retry: data.retry,
      notify_on_failure: data.notify_on_failure,
      schedule: data.schedule ?? undefined,
      repeat_strategy: data.repeat_strategy,
    };
    saveTaskGroup(group);
  };

  const handleRefresh = () => {
    listTaskGroups();
    refreshScripts();
  };

  if (!initialized) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="w-6 h-6 animate-spin text-primary" />
      </div>
    );
  }

  return (
    <div className="flex h-full">
      <ConfirmDialog
        open={groupToDelete != null}
        title="删除任务组"
        message={
          groupToDelete
            ? `确定要删除任务组「${groupToDelete.name}」吗？将删除已保存的本地文件且不可恢复。`
            : ""
        }
        detail={groupToDelete?.uuid}
        destructive
        confirmLabel="删除"
        onCancel={() => setGroupToDelete(null)}
        onConfirm={async () => {
          if (!groupToDelete) return;
          const id = groupToDelete.uuid;
          setGroupToDelete(null);
          await deleteTaskGroup(id);
        }}
      />

      <NewGroupDialog
        open={createDialogOpen || editingGroup != null}
        onClose={() => {
          setCreateDialogOpen(false);
          setEditingGroup(null);
        }}
        onCreate={handleCreateGroup}
        onSaveEdit={(g) => {
          void saveTaskGroup(g);
          setEditingGroup(null);
        }}
        editingGroup={editingGroup}
        scripts={taskScripts}
      />

      {panelCollapsed ? (
        <div className="flex flex-col items-center py-3 px-1.5 border-r border-border-subtle shrink-0">
          <button
            type="button"
            onClick={() => {
              manualPanelRef.current = true;
              setPanelCollapsed(false);
            }}
            className="p-2 rounded-md hover:bg-surface-hover text-foreground-tertiary hover:text-foreground"
            title="展开列表"
          >
            <PanelLeftOpen className="w-4 h-4" />
          </button>
        </div>
      ) : (
        <div className="w-72 border-r border-border-subtle flex flex-col shrink-0">
          <div className="flex items-center px-4 py-3 border-b border-border-subtle">
            <div className="rounded-md border border-border-subtle bg-surface p-0.5 flex shrink-0">
                <button
                  type="button"
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
                  type="button"
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
            <div className="ml-auto flex items-center gap-1 min-w-0">
              <button
                type="button"
                onClick={handleRefresh}
                className="p-1.5 rounded-md hover:bg-surface-hover text-foreground-tertiary hover:text-foreground shrink-0"
                title="刷新"
              >
                <RefreshCw className="w-3.5 h-3.5" />
              </button>
              <button
                type="button"
                onClick={() => setCreateDialogOpen(true)}
                className="p-1.5 rounded-md hover:bg-surface-hover text-foreground-tertiary hover:text-foreground shrink-0"
                title="新建任务组"
              >
                <Plus className="w-3.5 h-3.5" />
              </button>
              <button
                type="button"
                onClick={() => {
                  manualPanelRef.current = true;
                  setPanelCollapsed(true);
                }}
                className="p-1.5 rounded-md hover:bg-surface-hover text-foreground-tertiary hover:text-foreground shrink-0"
                title="收起列表"
              >
                <PanelLeftClose className="w-3.5 h-3.5" />
              </button>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-2 space-y-1">
            {taskGroups.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-12 text-center px-4">
                <Turtle className="w-8 h-8 text-foreground-tertiary/30 mb-3" />
                <p className="text-xs text-foreground-tertiary">暂无任务组</p>
                <button
                  type="button"
                  onClick={() => setCreateDialogOpen(true)}
                  className="mt-3 text-xs text-primary hover:underline"
                >
                  新建任务组
                </button>
              </div>
            ) : viewMode === "tree" ? (
              <PathTree
                items={taskGroupTreeItems}
                selectedId={selectedGroupId}
                onSelect={(item) => setSelectedGroupId(item.id)}
                emptyText="暂无任务组"
                renderLeaf={({ item, isSelected, onSelect }) => (
                  <GroupListItem
                    group={item.data}
                    isSelected={isSelected}
                    onSelect={onSelect}
                    isRunning={runningGroup === item.data.uuid}
                  />
                )}
              />
            ) : (
              <div className="space-y-2">
                {sortedSourceGroups.map(({ source, groups }) => (
                  <div key={source}>
                    <div className="px-2 py-1 text-[10px] font-medium text-foreground-tertiary/70 uppercase tracking-wider">
                      {source} ({groups.length})
                    </div>
                    <div className="space-y-1">
                      {groups.map((group) => (
                        <GroupListItem
                          key={group.uuid}
                          group={group}
                          isSelected={selectedGroupId === group.uuid}
                          onSelect={() => setSelectedGroupId(group.uuid)}
                          isRunning={runningGroup === group.uuid}
                        />
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}

      <div className="flex-1 overflow-y-auto min-w-0">
        {selectedGroup ? (
          <GroupDetail
            group={selectedGroup}
            progress={runningGroup === selectedGroup.uuid ? progress : null}
            onStart={handleStart}
            onStop={handleStop}
            running={runningGroup === selectedGroup.uuid}
            matchedWindow={matchedWindow}
            engineConfig={config}
            saveEngineConfig={saveConfig}
            scripts={scripts}
            onEdit={() => setEditingGroup(selectedGroup)}
            onRequestDelete={() => setGroupToDelete(selectedGroup)}
          />
        ) : (
          <div className="flex items-center justify-center h-full min-h-[200px] text-foreground-tertiary text-sm px-6 text-center">
            {taskGroups.length === 0
              ? "暂无任务组，请使用左侧「+」新建"
              : "选择一个任务组查看详情"}
          </div>
        )}
      </div>
    </div>
  );
}
