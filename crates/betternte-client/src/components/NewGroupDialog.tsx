import { ChevronDown, FolderTree, List, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { PathTree, type PathTreeItem } from "@/components/PathTree";
import { HelpHint } from "@/components/ui/HelpHint";
import { Toggle } from "@/components/ui/Toggle";
import { scriptKey } from "@/lib/stores/helpers";
import { buildScriptTreePath } from "@/lib/treePaths";
import type { ScheduleConfig, ScheduleType, ScriptInfo, TaskGroup } from "@/lib/types";
import { cn } from "@/lib/utils";

// ============================================================================
// Tree Script Selector
// ============================================================================

function ScriptTreeSelector({
  scripts,
  selected,
  onChange,
}: {
  scripts: ScriptInfo[];
  selected: string[];
  onChange: (names: string[]) => void;
}) {
  const [viewMode, setViewMode] = useState<"list" | "tree">("list");
  const treeItems = useMemo<PathTreeItem<ScriptInfo>[]>(
    () =>
      scripts.map((script) => ({
        id: scriptKey(script),
        label: script.display_name,
        path: buildScriptTreePath(script),
        data: script,
      })),
    [scripts]
  );
  const sourceGroups = useMemo(() => {
    const groups = new Map<string, ScriptInfo[]>();
    for (const script of scripts) {
      const source = script.source?.trim() || "未分类";
      if (!groups.has(source)) groups.set(source, []);
      groups.get(source)!.push(script);
    }
    return Array.from(groups.entries())
      .sort(([a], [b]) => a.localeCompare(b, "zh-CN"))
      .map(([source, items]) => ({ source, items }));
  }, [scripts]);
  const selectedSet = useMemo(() => new Set(selected), [selected]);

  const toggleScript = (id: string) => {
    const next = new Set(selectedSet);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    onChange(Array.from(next));
  };

  return (
    <div className="border border-border-subtle rounded-lg bg-card max-h-64 overflow-y-auto p-2">
      <div className="mb-2 rounded-md border border-border-subtle bg-surface p-0.5 inline-flex">
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
      {viewMode === "tree" ? (
        <PathTree
          items={treeItems}
          selectedId={null}
          onSelect={(item) => toggleScript(item.id)}
          emptyText="暂无可用脚本"
          renderLeaf={({ item, onSelect }) => {
            const isSelected = selectedSet.has(item.id);
            const script = item.data;
            return (
              <button
                onClick={onSelect}
                className={cn(
                  "w-full text-left flex items-center gap-2 px-2 py-1.5 rounded-md transition-colors",
                  isSelected ? "bg-primary/10" : "hover:bg-surface-hover"
                )}
              >
                <div
                  className={cn(
                    "w-4 h-4 rounded border flex items-center justify-center shrink-0",
                    isSelected
                      ? "bg-primary border-primary text-primary-foreground"
                      : "border-border"
                  )}
                >
                  {isSelected && <div className="w-2 h-2 rounded-sm bg-primary-foreground" />}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="text-sm text-foreground truncate">{script.display_name}</div>
                </div>
                <span className="text-xs text-foreground-tertiary shrink-0">v{script.version}</span>
              </button>
            );
          }}
        />
      ) : (
        <div className="space-y-2">
          {sourceGroups.map(({ source, items }) => (
            <div key={source}>
              <div className="px-2 py-1 text-[10px] font-medium text-foreground-tertiary/70 uppercase tracking-wider">
                {source} ({items.length})
              </div>
              <div className="space-y-1">
                {items.map((script) => {
                  const id = scriptKey(script);
                  const isSelected = selectedSet.has(id);
                  return (
                    <button
                      key={id}
                      onClick={() => toggleScript(id)}
                      className={cn(
                        "w-full text-left flex items-center gap-2 px-2 py-1.5 rounded-md transition-colors",
                        isSelected ? "bg-primary/10" : "hover:bg-surface-hover"
                      )}
                    >
                      <div
                        className={cn(
                          "w-4 h-4 rounded border flex items-center justify-center shrink-0",
                          isSelected
                            ? "bg-primary border-primary text-primary-foreground"
                            : "border-border"
                        )}
                      >
                        {isSelected && <div className="w-2 h-2 rounded-sm bg-primary-foreground" />}
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className="text-sm text-foreground truncate">{script.display_name}</div>
                      </div>
                      <span className="text-xs text-foreground-tertiary shrink-0">v{script.version}</span>
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      )}
      {selected.length > 0 && (
        <div className="text-xs text-foreground-tertiary pt-2 px-2 border-t border-border-subtle">
          已选 {selected.length} 个脚本
        </div>
      )}
    </div>
  );
}

// ============================================================================
// Form Components
// ============================================================================

function FormField({
  label,
  required,
  description,
  children,
}: {
  label: string;
  required?: boolean;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <div className="flex items-center gap-1.5 mb-1.5">
        <label className="flex items-center gap-1 text-sm font-medium text-foreground">
          {label}
          {required && <span className="text-destructive">*</span>}
        </label>
        {description && <HelpHint text={description} />}
      </div>
      {children}
    </div>
  );
}

function Input({
  value,
  onChange,
  placeholder,
  type = "text",
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: string;
}) {
  return (
    <input
      type={type}
      value={value}
      placeholder={placeholder}
      onChange={(e) => onChange(e.target.value)}
      className="w-full bg-surface border border-border rounded-md px-3 py-2 text-sm text-foreground placeholder:text-foreground-tertiary outline-none focus:border-primary"
    />
  );
}

function Select({
  value,
  onChange,
  options,
}: {
  value: string;
  onChange: (v: string) => void;
  options: { label: string; value: string }[];
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="w-full bg-surface border border-border rounded-md px-3 py-2 text-sm text-foreground outline-none focus:border-primary"
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

function NumberInput({
  value,
  onChange,
  min,
  max,
  className,
}: {
  value: number;
  onChange: (v: number) => void;
  min?: number;
  max?: number;
  className?: string;
}) {
  return (
    <input
      type="number"
      value={value}
      min={min}
      max={max}
      onChange={(e) => onChange(Number(e.target.value))}
      className={cn(
        "bg-surface border border-border rounded-md px-3 py-2 text-sm text-foreground outline-none focus:border-primary w-24 text-center font-mono",
        className
      )}
    />
  );
}

// ============================================================================
// Advanced Section
// ============================================================================

function AdvancedSection({
  errorHandling,
  onErrorChange,
  retryEnabled,
  onRetryEnabledChange,
  retryInterval,
  onRetryIntervalChange,
  retryCount,
  onRetryCountChange,
  notifyOnFailure,
  onNotifyChange,
  scheduleType,
  onScheduleTypeChange,
  scheduleHour,
  onScheduleHourChange,
  scheduleMinute,
  onScheduleMinuteChange,
  scheduleDays,
  onScheduleDaysChange,
  repeatStrategy,
  onRepeatStrategyChange,
}: {
  errorHandling: string;
  onErrorChange: (v: string) => void;
  retryEnabled: boolean;
  onRetryEnabledChange: (v: boolean) => void;
  retryInterval: number;
  onRetryIntervalChange: (v: number) => void;
  retryCount: number;
  onRetryCountChange: (v: number) => void;
  notifyOnFailure: boolean;
  onNotifyChange: (v: boolean) => void;
  scheduleType: ScheduleType;
  onScheduleTypeChange: (v: ScheduleType) => void;
  scheduleHour: number;
  onScheduleHourChange: (v: number) => void;
  scheduleMinute: number;
  onScheduleMinuteChange: (v: number) => void;
  scheduleDays: number[];
  onScheduleDaysChange: (v: number[]) => void;
  repeatStrategy: string;
  onRepeatStrategyChange: (v: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const dayLabels = ["日", "一", "二", "三", "四", "五", "六"];

  return (
    <div className="border border-border-subtle rounded-lg overflow-hidden">
      <button
        onClick={() => setOpen(!open)}
        className="w-full flex items-center justify-between px-4 py-3 hover:bg-surface-hover transition-colors"
      >
        <span className="text-sm font-medium text-foreground-secondary">高级配置</span>
        <ChevronDown
          className={cn(
            "w-4 h-4 text-foreground-tertiary transition-transform",
            open && "rotate-180"
          )}
        />
      </button>
      {open && (
        <div className="border-t border-border-subtle px-4 py-4 space-y-4">
          {/* Error handling */}
          <FormField label="错误处理方式" description="脚本执行失败时的处理策略">
            <Select
              value={errorHandling}
              onChange={onErrorChange}
              options={[
                { label: "中断执行", value: "interrupt" },
                { label: "跳过继续", value: "skip" },
              ]}
            />
          </FormField>

          {/* Retry */}
          <div>
            <div className="flex items-center justify-between mb-2">
              <div>
                <div className="text-sm font-medium text-foreground">失败重试</div>
                <div className="text-xs text-foreground-tertiary">脚本失败后自动重试</div>
              </div>
              <Toggle checked={retryEnabled} onChange={onRetryEnabledChange} />
            </div>
            {retryEnabled && (
              <div className="flex items-center gap-4 mt-2 pl-0">
                <div className="flex items-center gap-2">
                  <span className="text-xs text-foreground-tertiary whitespace-nowrap">间隔</span>
                  <NumberInput
                    value={retryInterval}
                    onChange={onRetryIntervalChange}
                    min={0}
                    max={60000}
                    className="w-20"
                  />
                  <span className="text-xs text-foreground-tertiary">ms</span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-foreground-tertiary whitespace-nowrap">次数</span>
                  <NumberInput
                    value={retryCount}
                    onChange={onRetryCountChange}
                    min={1}
                    max={10}
                    className="w-16"
                  />
                </div>
              </div>
            )}
          </div>

          {/* Notify on failure */}
          <div className="flex items-center justify-between">
            <div>
              <div className="text-sm font-medium text-foreground">失败通知</div>
              <div className="text-xs text-foreground-tertiary">任务组执行失败时发送通知</div>
            </div>
            <Toggle checked={notifyOnFailure} onChange={onNotifyChange} />
          </div>

          {/* Schedule */}
          <FormField label="调度方式" description="任务组的执行调度计划">
            <Select
              value={scheduleType}
              onChange={(v) => onScheduleTypeChange(v as ScheduleType)}
              options={[
                { label: "手动执行", value: "once" },
                { label: "每天", value: "daily" },
                { label: "每周", value: "weekly" },
              ]}
            />
          </FormField>

          {(scheduleType === "daily" || scheduleType === "weekly") && (
            <>
              {scheduleType === "weekly" && (
                <FormField label="执行日" description="选择每周执行的日期">
                  <div className="flex gap-1.5">
                    {dayLabels.map((label, i) => {
                      const active = scheduleDays.includes(i);
                      return (
                        <button
                          key={i}
                          onClick={() => {
                            const next = active
                              ? scheduleDays.filter((d) => d !== i)
                              : [...scheduleDays, i].sort();
                            onScheduleDaysChange(next);
                          }}
                          className={cn(
                            "w-8 h-8 rounded-md text-xs font-medium transition-colors",
                            active
                              ? "bg-primary text-primary-foreground"
                              : "bg-surface border border-border text-foreground-tertiary hover:bg-surface-hover"
                          )}
                        >
                          {label}
                        </button>
                      );
                    })}
                  </div>
                </FormField>
              )}
              <div className="flex items-center gap-3">
                <FormField label="执行时间">
                  <div className="flex items-center gap-2">
                    <NumberInput
                      value={scheduleHour}
                      onChange={onScheduleHourChange}
                      min={0}
                      max={23}
                      className="w-16"
                    />
                    <span className="text-foreground-tertiary">:</span>
                    <NumberInput
                      value={scheduleMinute}
                      onChange={onScheduleMinuteChange}
                      min={0}
                      max={59}
                      className="w-16"
                    />
                  </div>
                </FormField>
              </div>
            </>
          )}

          {scheduleType !== "once" && (
            <FormField label="重复调度策略" description="当上一次调度还未完成时的处理方式">
              <Select
                value={repeatStrategy}
                onChange={onRepeatStrategyChange}
                options={[
                  { label: "跳过本次", value: "skip" },
                  { label: "中断上次，执行新的", value: "interrupt" },
                ]}
              />
            </FormField>
          )}
        </div>
      )}
    </div>
  );
}

function uuidParentDirectory(uuid: string): string {
  const n = uuid.replace(/\\/g, "/").trim();
  const parts = n.split("/").filter(Boolean);
  if (parts.length <= 1) return "";
  return parts.slice(0, -1).join("/");
}

// ============================================================================
// NewGroupDialog
// ============================================================================

export function NewGroupDialog({
  open,
  onClose,
  onCreate,
  onSaveEdit,
  scripts,
  editingGroup,
}: {
  open: boolean;
  onClose: () => void;
  onCreate: (group: {
    directory: string;
    name: string;
    description: string;
    mode: "sequential" | "random";
    scripts: string[];
    error_handling: "interrupt" | "skip";
    retry: { enabled: boolean; interval_ms: number; count: number };
    notify_on_failure: boolean;
    schedule: ScheduleConfig | null;
    repeat_strategy: "skip" | "interrupt";
  }) => void;
  /** When set, dialog pre-fills from this task group and saves via `onSaveEdit`. */
  onSaveEdit?: (group: TaskGroup) => void;
  scripts: ScriptInfo[];
  editingGroup?: TaskGroup | null;
}) {
  const isEdit = Boolean(editingGroup);

  const [directory, setDirectory] = useState("");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [mode, setMode] = useState<"sequential" | "random">("sequential");
  const [selectedScriptKeys, setSelectedScriptKeys] = useState<string[]>([]);

  // Advanced
  const [errorHandling, setErrorHandling] = useState<"interrupt" | "skip">("skip");
  const [retryEnabled, setRetryEnabled] = useState(false);
  const [retryInterval, setRetryInterval] = useState(3000);
  const [retryCount, setRetryCount] = useState(3);
  const [notifyOnFailure, setNotifyOnFailure] = useState(false);
  const [scheduleType, setScheduleType] = useState<ScheduleType>("once");
  const [scheduleHour, setScheduleHour] = useState(0);
  const [scheduleMinute, setScheduleMinute] = useState(0);
  const [scheduleDays, setScheduleDays] = useState<number[]>([]);
  const [repeatStrategy, setRepeatStrategy] = useState<"skip" | "interrupt">("skip");

  const resetForm = () => {
    setDirectory("");
    setName("");
    setDescription("");
    setMode("sequential");
    setSelectedScriptKeys([]);
    setErrorHandling("skip");
    setRetryEnabled(false);
    setRetryInterval(3000);
    setRetryCount(3);
    setNotifyOnFailure(false);
    setScheduleType("once");
    setScheduleHour(0);
    setScheduleMinute(0);
    setScheduleDays([]);
    setRepeatStrategy("skip");
  };

  useEffect(() => {
    if (!open || !editingGroup) return;
    const g = editingGroup;
    setDirectory(uuidParentDirectory(g.uuid));
    setName(g.name);
    setDescription(g.description ?? "");
    setMode(g.mode === "random" ? "random" : "sequential");
    const keys: string[] = [];
    for (const n of g.nodes) {
      const byId = scripts.find((s) => scriptKey(s) === n.script);
      if (byId) {
        keys.push(scriptKey(byId));
        continue;
      }
      const byName = scripts.find((s) => s.name === n.script);
      if (byName) keys.push(scriptKey(byName));
    }
    setSelectedScriptKeys(keys);
    setErrorHandling(g.error_handling === "skip" ? "skip" : "interrupt");
    const r = g.retry;
    setRetryEnabled(Boolean(r?.enabled));
    setRetryInterval(r?.interval_ms ?? 3000);
    const rc = r?.count ?? g.retry_count ?? 0;
    setRetryCount(rc > 0 ? rc : 3);
    setNotifyOnFailure(Boolean(g.notify_on_failure));
    const sch = g.schedule;
    if (!sch || sch.type === "once") {
      setScheduleType("once");
      setScheduleHour(sch?.hour ?? 0);
      setScheduleMinute(sch?.minute ?? 0);
      setScheduleDays(sch?.days_of_week ?? []);
    } else {
      setScheduleType(sch.type);
      setScheduleHour(sch.hour ?? 0);
      setScheduleMinute(sch.minute ?? 0);
      setScheduleDays(sch.days_of_week ?? []);
    }
    setRepeatStrategy(g.repeat_strategy === "interrupt" ? "interrupt" : "skip");
  }, [open, editingGroup, scripts]);

  const canCreate = name.trim().length > 0 && selectedScriptKeys.length > 0;

  const buildSchedule = (): ScheduleConfig | null =>
    scheduleType === "once"
      ? null
      : {
          type: scheduleType,
          hour: scheduleHour,
          minute: scheduleMinute,
          days_of_week: scheduleType === "weekly" ? scheduleDays : undefined,
        };

  const handleSubmit = () => {
    if (!canCreate) return;

    if (isEdit && editingGroup) {
      if (!onSaveEdit) return;
      const nodes = selectedScriptKeys.map((key) => {
        const script = scripts.find((s) => scriptKey(s) === key)!;
        const prev =
          editingGroup.nodes.find((n) => n.script === key) ??
          editingGroup.nodes.find((n) => n.script === script.name);
        return {
          script: key,
          alias: prev?.alias ?? script.display_name,
          timeout_ms: prev?.timeout_ms,
          params: prev?.params,
        };
      });
      const next: TaskGroup = {
        uuid: editingGroup.uuid,
        name: name.trim(),
        description: description.trim(),
        mode,
        retry_count: retryCount,
        nodes,
        source: editingGroup.source ?? "local",
        error_handling: errorHandling,
        retry: { enabled: retryEnabled, interval_ms: retryInterval, count: retryCount },
        notify_on_failure: notifyOnFailure,
        schedule: buildSchedule() ?? undefined,
        repeat_strategy: repeatStrategy,
      };
      onSaveEdit(next);
      resetForm();
      onClose();
      return;
    }

    onCreate({
      directory: directory.trim().replaceAll("\\", "/").replace(/^\/+|\/+$/g, ""),
      name: name.trim(),
      description: description.trim(),
      mode,
      scripts: selectedScriptKeys,
      error_handling: errorHandling,
      retry: { enabled: retryEnabled, interval_ms: retryInterval, count: retryCount },
      notify_on_failure: notifyOnFailure,
      schedule: buildSchedule(),
      repeat_strategy: repeatStrategy,
    });
    resetForm();
    onClose();
  };

  if (!open) return null;

  return (
    <>
      <div
        className="fixed inset-0 bg-black/50 z-50 animate-in fade-in duration-200"
        onClick={onClose}
      />
      <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
        <div className="bg-card border border-border-subtle rounded-xl shadow-2xl w-full max-w-xl max-h-[85vh] flex flex-col animate-in zoom-in-95 duration-200">
          {/* Header */}
          <div className="flex items-center justify-between px-5 py-4 border-b border-border-subtle">
            <h3 className="text-base font-semibold text-foreground">
              {isEdit ? "编辑任务组" : "新建任务组"}
            </h3>
            <button
              onClick={onClose}
              className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary"
            >
              <X className="w-4 h-4" />
            </button>
          </div>

          {/* Body */}
          <div className="flex-1 overflow-y-auto px-5 py-4 space-y-5">
            {/* Basic config */}
            {isEdit && editingGroup ? (
              <FormField label="标识（文件 ID）" description="编辑时不修改路径标识，避免破坏已保存文件与快捷键">
                <div className="px-3 py-2 rounded-md bg-surface border border-border text-xs font-mono text-foreground-secondary break-all">
                  {editingGroup.uuid}
                </div>
              </FormField>
            ) : (
              <FormField
                label="目录路径"
                description="可选，例如：daily/farm（用于分组展示，更方便查找）"
              >
                <Input
                  value={directory}
                  onChange={setDirectory}
                  placeholder="留空则存放在根目录"
                />
              </FormField>
            )}

            <FormField label="任务组名称" required>
              <Input
                value={name}
                onChange={setName}
                placeholder="例：每日一条龙"
              />
            </FormField>

            <FormField label="描述" description="可选，简要说明任务组用途">
              <Input
                value={description}
                onChange={setDescription}
                placeholder="例：自动完成每日委托 + 刷秘境 + 领取奖励"
              />
            </FormField>

            <FormField label="执行方式" description="顺序执行会按列表依次运行；随机执行会打乱顺序">
              <Select
                value={mode}
                onChange={(v) => setMode(v as "sequential" | "random")}
                options={[
                  { label: "顺序执行", value: "sequential" },
                  { label: "随机执行", value: "random" },
                ]}
              />
            </FormField>

            <FormField
              label="选择脚本"
              required
              description="按目录选择要执行的脚本，支持选择子目录里的脚本。"
            >
              <ScriptTreeSelector
                scripts={scripts}
                selected={selectedScriptKeys}
                onChange={setSelectedScriptKeys}
              />
            </FormField>

            {/* Advanced section */}
            <AdvancedSection
              errorHandling={errorHandling}
              onErrorChange={(v) => setErrorHandling(v as "interrupt" | "skip")}
              retryEnabled={retryEnabled}
              onRetryEnabledChange={setRetryEnabled}
              retryInterval={retryInterval}
              onRetryIntervalChange={setRetryInterval}
              retryCount={retryCount}
              onRetryCountChange={setRetryCount}
              notifyOnFailure={notifyOnFailure}
              onNotifyChange={setNotifyOnFailure}
              scheduleType={scheduleType}
              onScheduleTypeChange={setScheduleType}
              scheduleHour={scheduleHour}
              onScheduleHourChange={setScheduleHour}
              scheduleMinute={scheduleMinute}
              onScheduleMinuteChange={setScheduleMinute}
              scheduleDays={scheduleDays}
              onScheduleDaysChange={setScheduleDays}
              repeatStrategy={repeatStrategy}
              onRepeatStrategyChange={(v) => setRepeatStrategy(v as "skip" | "interrupt")}
            />
          </div>

          {/* Footer */}
          <div className="flex items-center justify-end gap-3 px-5 py-4 border-t border-border-subtle">
            <button
              onClick={onClose}
              className="px-4 py-2 rounded-md bg-surface border border-border text-foreground text-sm font-medium hover:bg-surface-hover"
            >
              取消
            </button>
            <button
              onClick={handleSubmit}
              disabled={!canCreate}
              className="px-4 py-2 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary-hover disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {isEdit ? "保存" : "创建"}
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
