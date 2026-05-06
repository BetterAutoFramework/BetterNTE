import {
  Filter,
  FolderTree,
  List,
  Loader2,
  PanelLeftClose,
  PanelLeftOpen,
  RefreshCw,
  Zap,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { PathTree, type PathTreeItem } from "@/components/PathTree";
import { ScriptManifestMetaSection } from "@/components/ScriptManifestMeta";
import { SchemaForm } from "@/components/SchemaForm";
import { PANEL_THRESHOLD_PX } from "@/lib/constants/layout";
import { useEngineStore } from "@/lib/store";
import { scriptKey } from "@/lib/stores/helpers";
import { buildScriptTreePath } from "@/lib/treePaths";
import type { ScriptInfo, TriggerState } from "@/lib/types";
import { cn } from "@/lib/utils";

// ============================================================================
// TriggerPage — dual-panel: left list + right config
// ============================================================================

export function TriggerPage() {
  const triggers = useEngineStore((s) => s.triggers);
  const config = useEngineStore((s) => s.config);
  const saveConfig = useEngineStore((s) => s.saveConfig);
  const refreshTriggers = useEngineStore((s) => s.refreshTriggers);
  const initialized = useEngineStore((s) => s.initialized);
  const initEngine = useEngineStore((s) => s.initEngine);
  const enableTrigger = useEngineStore((s) => s.enableTrigger);
  const disableTrigger = useEngineStore((s) => s.disableTrigger);

  const [selected, setSelected] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<"list" | "tree">("list");
  const [showEnabledOnly, setShowEnabledOnly] = useState(false);
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
      refreshTriggers();
    }
  }, [initialized, initEngine, refreshTriggers]);

  const globalTriggers = triggers.filter(
    (t) => t.visibility === "global" || !t.visibility
  );

  const triggerConfigs = config?.triggers ?? {};
  const filteredTriggers = useMemo(
    () =>
      showEnabledOnly
        ? globalTriggers.filter((trigger) => triggerConfigs[scriptKey(trigger)]?.enabled)
        : globalTriggers,
    [globalTriggers, showEnabledOnly, triggerConfigs]
  );

  // Auto-select first trigger if none selected
  useEffect(() => {
    if (!selected && filteredTriggers.length > 0) {
      setSelected(scriptKey(filteredTriggers[0]));
    }
  }, [filteredTriggers, selected]);

  useEffect(() => {
    if (filteredTriggers.length === 0) {
      setSelected(null);
      return;
    }
    const exists = filteredTriggers.some((t) => scriptKey(t) === selected);
    if (selected && !exists) {
      setSelected(scriptKey(filteredTriggers[0]));
    }
  }, [filteredTriggers, selected]);

  const selectedTrigger = filteredTriggers.find((t) => scriptKey(t) === selected);
  const triggerTreeItems = useMemo<PathTreeItem<ScriptInfo>[]>(
    () =>
      filteredTriggers.map((trigger) => ({
        id: scriptKey(trigger),
        label: trigger.display_name,
        path: buildScriptTreePath(trigger),
        data: trigger,
      })),
    [filteredTriggers]
  );
  const triggerSourceGroups = useMemo(() => {
    const groups = new Map<string, ScriptInfo[]>();
    for (const trigger of filteredTriggers) {
      const source = trigger.source?.trim() || "未分类";
      if (!groups.has(source)) groups.set(source, []);
      groups.get(source)!.push(trigger);
    }
    return Array.from(groups.entries())
      .sort(([a], [b]) => a.localeCompare(b, "zh-CN"))
      .map(([source, items]) => ({ source, items }));
  }, [filteredTriggers]);

  const getTriggerState = (name: string): TriggerState => {
    return triggerConfigs[name] ?? { enabled: false, params: {} };
  };

  const setTriggerState = async (
    name: string,
    update: { enabled?: boolean; params?: Record<string, unknown> }
  ) => {
    if (!config) return;
    const current = getTriggerState(name);
    const newEnabled = update.enabled ?? current.enabled;
    const newParams = update.params ?? current.params;
    const newState: TriggerState = { enabled: newEnabled, params: newParams };
    const updatedConfig = {
      ...config,
      triggers: { ...config.triggers, [name]: newState },
    };
    try {
      await saveConfig(updatedConfig);
      if (newEnabled) {
        await enableTrigger(name, newParams);
      } else {
        await disableTrigger(name);
      }
    } catch {
      // Revert config to previous state on failure
      const revertedConfig = {
        ...config,
        triggers: { ...config.triggers, [name]: current },
      };
      await saveConfig(revertedConfig);
    }
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
      {/* Left panel — trigger list */}
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
              onClick={() => setShowEnabledOnly((v) => !v)}
              className={cn(
                "p-1.5 rounded-md border transition-colors",
                showEnabledOnly
                  ? "bg-primary/15 border-primary/30 text-primary"
                  : "bg-surface border-border-subtle text-foreground-tertiary hover:text-foreground"
              )}
              title={showEnabledOnly ? "显示全部触发器" : "仅显示已激活触发器"}
              aria-label={showEnabledOnly ? "显示全部触发器" : "仅显示已激活触发器"}
            >
              <Filter className="w-3.5 h-3.5" />
            </button>
            <button
              onClick={refreshTriggers}
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
          {triggerTreeItems.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12 text-center px-4">
              <Zap className="w-8 h-8 text-foreground-tertiary/30 mb-3" />
              <p className="text-xs text-foreground-tertiary">
                暂无触发器
              </p>
            </div>
          ) : (
            viewMode === "tree" ? (
              <PathTree
                items={triggerTreeItems}
                selectedId={selected}
                onSelect={(item) => setSelected(item.id)}
                emptyText="暂无触发器"
                renderLeaf={({ item, isSelected, onSelect }) => {
                  const trigger = item.data;
                  const state = getTriggerState(scriptKey(trigger));
                  return (
                    <button
                      onClick={onSelect}
                      className={cn(
                        "w-full text-left rounded-md px-3 py-2.5 transition-colors border",
                        state.enabled
                          ? "border-primary/35 bg-primary/10 shadow-[inset_0_0_0_1px_rgba(59,130,246,0.18)]"
                          : "border-transparent",
                        isSelected
                          ? "ring-1 ring-primary/40"
                          : "hover:bg-surface-hover"
                      )}
                    >
                      <div className="flex items-center gap-2.5">
                        <div
                          className={cn(
                            "w-7 h-7 rounded-md flex items-center justify-center shrink-0",
                            state.enabled
                              ? "bg-primary/10 text-primary"
                              : "bg-surface text-foreground-tertiary"
                          )}
                        >
                          <Zap className="w-3.5 h-3.5" />
                        </div>
                        <div className="min-w-0 flex-1">
                          <div className="text-sm text-foreground truncate">
                            {trigger.display_name}
                          </div>
                          <div className="text-xs text-foreground-tertiary truncate">
                            {trigger.description || trigger.name}
                          </div>
                        </div>
                        {state.enabled ? (
                          <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-primary/15 text-primary shrink-0">
                            已激活
                          </span>
                        ) : null}
                      </div>
                    </button>
                  );
                }}
              />
            ) : (
              <div className="space-y-2">
                {triggerSourceGroups.map(({ source, items }) => (
                  <div key={source}>
                    <div className="px-2 py-1 text-[10px] font-medium text-foreground-tertiary/70 uppercase tracking-wider">
                      {source} ({items.length})
                    </div>
                    <div className="space-y-1">
                      {items.map((trigger) => {
                        const id = scriptKey(trigger);
                        const state = getTriggerState(scriptKey(trigger));
                        const isSelected = selected === id;
                        return (
                          <button
                            key={id}
                            onClick={() => setSelected(id)}
                            className={cn(
                              "w-full text-left rounded-md px-3 py-2.5 transition-colors border",
                              state.enabled
                                ? "border-primary/35 bg-primary/10 shadow-[inset_0_0_0_1px_rgba(59,130,246,0.18)]"
                                : "border-transparent",
                              isSelected
                                ? "ring-1 ring-primary/40"
                                : "hover:bg-surface-hover"
                            )}
                          >
                            <div className="flex items-center gap-2.5">
                              <div
                                className={cn(
                                  "w-7 h-7 rounded-md flex items-center justify-center shrink-0",
                                  state.enabled
                                    ? "bg-primary/10 text-primary"
                                    : "bg-surface text-foreground-tertiary"
                                )}
                              >
                                <Zap className="w-3.5 h-3.5" />
                              </div>
                              <div className="min-w-0 flex-1">
                                <div className="text-sm text-foreground truncate">
                                  {trigger.display_name}
                                </div>
                                <div className="text-xs text-foreground-tertiary truncate">
                                  {trigger.description || trigger.name}
                                </div>
                              </div>
                              {state.enabled ? (
                                <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-primary/15 text-primary shrink-0">
                                  已激活
                                </span>
                              ) : null}
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

      {/* Right panel — config */}
      <div className="flex-1 overflow-y-auto">
        {selectedTrigger ? (
          <TriggerDetail
            trigger={selectedTrigger}
            state={getTriggerState(scriptKey(selectedTrigger))}
            onToggle={(enabled) =>
              setTriggerState(scriptKey(selectedTrigger), { enabled })
            }
            onConfigChange={(params) =>
              setTriggerState(scriptKey(selectedTrigger), { params })
            }
          />
        ) : (
          <div className="flex items-center justify-center h-full text-foreground-tertiary text-sm">
            选择一个触发器查看配置
          </div>
        )}
      </div>
    </div>
  );
}

// ============================================================================
// Trigger detail panel
// ============================================================================

function TriggerDetail({
  trigger,
  state,
  onToggle,
  onConfigChange,
}: {
  trigger: ScriptInfo;
  state: TriggerState;
  onToggle: (enabled: boolean) => void;
  onConfigChange: (params: Record<string, unknown>) => void;
}) {
  const schema: Record<string, unknown> = trigger.params_schema ?? {
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
              state.enabled
                ? "bg-primary/10 text-primary"
                : "bg-surface text-foreground-tertiary"
            )}
          >
            <Zap className="w-5 h-5" />
          </div>
          <div className="min-w-0">
            <h1 className="text-lg font-semibold text-foreground truncate">
              {trigger.display_name}
            </h1>
            <p className="text-xs text-foreground-tertiary font-mono">v{trigger.version}</p>
          </div>
        </div>

        <div className="flex items-center gap-3 shrink-0">
          {/* Enable toggle */}
          <button
            type="button"
            onClick={() => onToggle(!state.enabled)}
            className={cn(
              "w-11 h-6 rounded-full relative transition-colors shrink-0",
              state.enabled ? "bg-primary" : "bg-foreground-tertiary/30"
            )}
          >
            <span
              className={cn(
                "absolute top-0.5 w-5 h-5 rounded-full bg-white shadow-sm transition-all",
                state.enabled ? "left-[22px]" : "left-0.5"
              )}
            />
          </button>
        </div>
      </div>

      {/* Status */}
      <div className="flex items-center gap-2 mb-6">
        <div
          className={cn(
            "px-2.5 py-1 rounded-full text-xs font-medium",
            state.enabled
              ? "bg-primary/10 text-primary"
              : "bg-surface text-foreground-tertiary"
          )}
        >
          {state.enabled ? "运行中" : "已停止"}
        </div>
        {trigger.tags && trigger.tags.length > 0 && (
          <div className="flex gap-1.5">
            {trigger.tags.map((tag: string) => (
              <span
                key={tag}
                className="px-2 py-0.5 rounded text-xs bg-surface text-foreground-tertiary"
              >
                {tag}
              </span>
            ))}
          </div>
        )}
      </div>

      {/* Description */}
      <div className="mb-6">
        <h3 className="text-sm font-medium text-foreground mb-3">描述</h3>
        <div className="rounded-lg border border-border-subtle bg-card p-4 text-sm text-foreground-secondary leading-relaxed">
          {trigger.description?.trim() || "暂无描述"}
        </div>
      </div>

      {/* Config */}
      <div>
        <h3 className="text-sm font-medium text-foreground mb-3">配置</h3>
        <div className="rounded-lg border border-border-subtle bg-card p-4">
          <SchemaForm
            schema={schema}
            values={state.params}
            disabled={state.enabled}
            onChange={onConfigChange}
            emptyMessage="此触发器没有可配置的选项"
          />
        </div>
      </div>

      <ScriptManifestMetaSection script={trigger} className="mt-6" showDescription={false} />
    </div>
  );
}
