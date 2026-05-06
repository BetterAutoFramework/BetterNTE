import { FolderTree, List, Play, Search, Settings2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { PathTree, type PathTreeItem } from "@/components/PathTree";
import { ScriptManifestMetaSection } from "@/components/ScriptManifestMeta";
import { useEngineStore } from "@/lib/store";
import { scriptKey } from "@/lib/stores/helpers";
import { buildScriptTreePath } from "@/lib/treePaths";
import type { ScriptInfo, ScriptType } from "@/lib/types";
import { cn } from "@/lib/utils";

const typeLabels: Record<ScriptType, string> = {
  task: "任务",
  solo_task: "任务",
  trigger: "触发器",
  library: "公共库",
};

const typeColors: Record<ScriptType, string> = {
  task: "bg-primary/15 text-primary",
  solo_task: "bg-primary/15 text-primary",
  trigger: "bg-success/15 text-success",
  library: "bg-warning/15 text-warning",
};

function ScriptCard({
  script,
  isSelected,
  onSelect,
  onRun,
}: {
  script: ScriptInfo;
  isSelected: boolean;
  onSelect: () => void;
  onRun: () => void;
}) {
  const runnable = script.type !== "library";
  return (
    <div
      onClick={onSelect}
      className={cn(
        "flex items-center justify-between p-3 rounded-lg border cursor-pointer transition-colors",
        isSelected
          ? "border-primary bg-primary/5"
          : "border-border-subtle bg-card hover:bg-card-hover hover:border-border"
      )}
    >
      <div className="flex items-center gap-3 min-w-0">
        <div
          className={cn(
            "w-2 h-2 rounded-full shrink-0",
            script.enabled ? "bg-success" : "bg-foreground-tertiary"
          )}
        />
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium text-foreground truncate">
              {script.display_name}
            </span>
            <span className="text-xs text-foreground-tertiary font-mono">
              v{script.version}
            </span>
            <span
              className={cn(
                "text-[10px] px-1.5 py-0.5 rounded-full font-medium",
                typeColors[script.type]
              )}
            >
              {typeLabels[script.type]}
            </span>
          </div>
          <div className="text-xs text-foreground-tertiary mt-0.5 truncate">
            {script.author} &middot; {script.description}
          </div>
        </div>
      </div>
      <div className="flex items-center gap-2 shrink-0 ml-3">
        <button
          onClick={(e) => {
            e.stopPropagation();
          }}
          className="p-1.5 rounded-md hover:bg-surface-hover text-foreground-tertiary hover:text-foreground"
        >
          <Settings2 className="w-4 h-4" />
        </button>
        <button
          onClick={(e) => {
            e.stopPropagation();
            if (runnable) onRun();
          }}
          disabled={!runnable}
          title={runnable ? "运行脚本" : "公共库不能直接运行"}
          className={cn(
            "p-1.5 rounded-md",
            runnable
              ? "hover:bg-primary/10 text-primary"
              : "text-foreground-tertiary cursor-not-allowed opacity-60"
          )}
        >
          <Play className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}

function ScriptDetail({ script }: { script: ScriptInfo }) {
  return (
    <div className="rounded-lg border border-border-subtle bg-card p-5">
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-base font-semibold text-foreground">{script.display_name}</h3>
        <button
          className={cn(
            "px-3 py-1.5 rounded-md text-xs font-medium transition-colors",
            script.enabled
              ? "bg-success/15 text-success hover:bg-success/25"
              : "bg-foreground-tertiary/15 text-foreground-tertiary hover:bg-foreground-tertiary/25"
          )}
        >
          {script.enabled ? "已启用" : "已禁用"}
        </button>
      </div>

      <ScriptManifestMetaSection script={script} className="mb-4" />

      {script.type === "library" && (
        <div className="rounded-md border border-warning/30 bg-warning/10 px-3 py-2 text-xs text-warning mb-4">
          公共库不可直接运行，请在其他脚本中通过 `ctx.call(...)` 调用。
        </div>
      )}

      {(script.type === "task" || script.type === "solo_task") && (
        <>
          <div className="mt-5 mb-3 text-xs font-medium text-foreground-secondary uppercase tracking-wider">
            配置
          </div>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <span className="text-sm text-foreground-secondary">关卡</span>
              <select className="bg-surface border border-border rounded-md px-3 py-1.5 text-sm text-foreground outline-none focus:border-primary">
                <option>1-7</option>
                <option>CE-5</option>
                <option>CA-5</option>
                <option>SK-5</option>
              </select>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm text-foreground-secondary">次数</span>
              <div className="flex items-center gap-2">
                <button className="w-7 h-7 rounded-md bg-surface border border-border text-foreground hover:bg-surface-hover">
                  -
                </button>
                <span className="text-sm font-mono w-8 text-center">5</span>
                <button className="w-7 h-7 rounded-md bg-surface border border-border text-foreground hover:bg-surface-hover">
                  +
                </button>
              </div>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm text-foreground-secondary">浓缩树脂</span>
              <button className="w-10 h-5 rounded-full bg-primary relative">
                <span className="absolute right-0.5 top-0.5 w-4 h-4 rounded-full bg-white shadow-sm" />
              </button>
            </div>
          </div>
        </>
      )}

      <div className="mt-5 flex gap-3">
        <button className="px-4 py-2 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary-hover">
          保存配置
        </button>
        <button className="px-4 py-2 rounded-md bg-surface border border-border text-foreground text-sm font-medium hover:bg-surface-hover">
          重置默认
        </button>
      </div>
    </div>
  );
}

export function ScriptManager() {
  const scripts = useEngineStore((s) => s.scripts);
  const runScript = useEngineStore((s) => s.runScript);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<"all" | ScriptType>("all");
  const [viewMode, setViewMode] = useState<"list" | "tree">("list");
  const [selected, setSelected] = useState<ScriptInfo | null>(scripts[0] ?? null);

  const filtered = scripts.filter((s) => {
    if (filter !== "all") {
      // "task" filter matches both "task" and "solo_task" (backend maps solo_task -> task)
      if (filter === "task") {
        if (s.type !== "task" && s.type !== "solo_task") return false;
      } else if (s.type !== filter) {
        return false;
      }
    }
    if (search && !s.display_name.includes(search) && !s.name.includes(search)) return false;
    return true;
  });

  const soloTasks = filtered.filter((s) => s.type === "task" || s.type === "solo_task");
  const triggers = filtered.filter((s) => s.type === "trigger");
  const libraries = filtered.filter((s) => s.type === "library");
  const selectedId = selected ? scriptKey(selected) : null;

  const soloTaskItems = useMemo<PathTreeItem<ScriptInfo>[]>(
    () =>
      soloTasks.map((s) => ({
        id: scriptKey(s),
        label: s.display_name,
        path: buildScriptTreePath(s),
        data: s,
      })),
    [soloTasks]
  );
  const triggerItems = useMemo<PathTreeItem<ScriptInfo>[]>(
    () =>
      triggers.map((s) => ({
        id: scriptKey(s),
        label: s.display_name,
        path: buildScriptTreePath(s),
        data: s,
      })),
    [triggers]
  );
  const libraryItems = useMemo<PathTreeItem<ScriptInfo>[]>(
    () =>
      libraries.map((s) => ({
        id: scriptKey(s),
        label: s.display_name,
        path: buildScriptTreePath(s),
        data: s,
      })),
    [libraries]
  );
  const soloTaskSourceGroups = useMemo(() => {
    const groups = new Map<string, ScriptInfo[]>();
    for (const script of soloTasks) {
      const source = script.source?.trim() || "未分类";
      if (!groups.has(source)) groups.set(source, []);
      groups.get(source)!.push(script);
    }
    return Array.from(groups.entries())
      .sort(([a], [b]) => a.localeCompare(b, "zh-CN"))
      .map(([source, items]) => ({ source, items }));
  }, [soloTasks]);
  const triggerSourceGroups = useMemo(() => {
    const groups = new Map<string, ScriptInfo[]>();
    for (const script of triggers) {
      const source = script.source?.trim() || "未分类";
      if (!groups.has(source)) groups.set(source, []);
      groups.get(source)!.push(script);
    }
    return Array.from(groups.entries())
      .sort(([a], [b]) => a.localeCompare(b, "zh-CN"))
      .map(([source, items]) => ({ source, items }));
  }, [triggers]);
  const librarySourceGroups = useMemo(() => {
    const groups = new Map<string, ScriptInfo[]>();
    for (const script of libraries) {
      const source = script.source?.trim() || "未分类";
      if (!groups.has(source)) groups.set(source, []);
      groups.get(source)!.push(script);
    }
    return Array.from(groups.entries())
      .sort(([a], [b]) => a.localeCompare(b, "zh-CN"))
      .map(([source, items]) => ({ source, items }));
  }, [libraries]);

  useEffect(() => {
    if (!selected) return;
    const exists = filtered.some((item) => scriptKey(item) === scriptKey(selected));
    if (!exists) {
      setSelected(filtered[0] ?? null);
    }
  }, [filtered, selected]);

  return (
    <div className="p-6 max-w-5xl">
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-lg font-semibold text-foreground">脚本管理</h1>
        <div className="flex items-center gap-3">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-foreground-tertiary" />
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="搜索脚本..."
              className="pl-9 pr-3 py-2 rounded-md bg-surface border border-border text-sm text-foreground placeholder:text-foreground-tertiary outline-none focus:border-primary w-48"
            />
          </div>
          <select
            value={filter}
            onChange={(e) => setFilter(e.target.value as typeof filter)}
            className="bg-surface border border-border rounded-md px-3 py-2 text-sm text-foreground outline-none focus:border-primary"
          >
            <option value="all">全部</option>
            <option value="task">任务</option>
            <option value="trigger">触发器</option>
            <option value="library">公共库</option>
          </select>
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
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-5 gap-6">
        <div className="lg:col-span-3 space-y-5">
          {soloTasks.length > 0 && (
            <div>
              <div className="flex items-center gap-2 mb-3">
                <h2 className="text-sm font-medium text-foreground-secondary">独立任务</h2>
                <span className="text-xs text-foreground-tertiary">({soloTasks.length})</span>
              </div>
              {viewMode === "tree" ? (
                <PathTree
                  items={soloTaskItems}
                  selectedId={selectedId}
                  onSelect={(item) => setSelected(item.data)}
                  emptyText="暂无独立任务"
                  renderLeaf={({ item, isSelected, onSelect }) => (
                    <ScriptCard
                      key={item.id}
                      script={item.data}
                      isSelected={isSelected}
                      onSelect={onSelect}
                      onRun={() => runScript(scriptKey(item.data))}
                    />
                  )}
                />
              ) : (
                <div className="space-y-2">
                  {soloTaskSourceGroups.map(({ source, items }) => (
                    <div key={source}>
                      <div className="px-2 py-1 text-[10px] font-medium text-foreground-tertiary/70 uppercase tracking-wider">
                        {source} ({items.length})
                      </div>
                      <div className="space-y-2">
                        {items.map((item) => (
                          <ScriptCard
                            key={scriptKey(item)}
                            script={item}
                            isSelected={selectedId === scriptKey(item)}
                            onSelect={() => setSelected(item)}
                            onRun={() => runScript(scriptKey(item))}
                          />
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {triggers.length > 0 && (
            <div>
              <div className="flex items-center gap-2 mb-3">
                <h2 className="text-sm font-medium text-foreground-secondary">触发器</h2>
                <span className="text-xs text-foreground-tertiary">({triggers.length})</span>
              </div>
              {viewMode === "tree" ? (
                <PathTree
                  items={triggerItems}
                  selectedId={selectedId}
                  onSelect={(item) => setSelected(item.data)}
                  emptyText="暂无触发器"
                  renderLeaf={({ item, isSelected, onSelect }) => (
                    <ScriptCard
                      key={item.id}
                      script={item.data}
                      isSelected={isSelected}
                      onSelect={onSelect}
                      onRun={() => runScript(scriptKey(item.data))}
                    />
                  )}
                />
              ) : (
                <div className="space-y-2">
                  {triggerSourceGroups.map(({ source, items }) => (
                    <div key={source}>
                      <div className="px-2 py-1 text-[10px] font-medium text-foreground-tertiary/70 uppercase tracking-wider">
                        {source} ({items.length})
                      </div>
                      <div className="space-y-2">
                        {items.map((item) => (
                          <ScriptCard
                            key={scriptKey(item)}
                            script={item}
                            isSelected={selectedId === scriptKey(item)}
                            onSelect={() => setSelected(item)}
                            onRun={() => runScript(scriptKey(item))}
                          />
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {libraries.length > 0 && (
            <div>
              <div className="flex items-center gap-2 mb-3">
                <h2 className="text-sm font-medium text-foreground-secondary">公共库</h2>
                <span className="text-xs text-foreground-tertiary">({libraries.length})</span>
              </div>
              {viewMode === "tree" ? (
                <PathTree
                  items={libraryItems}
                  selectedId={selectedId}
                  onSelect={(item) => setSelected(item.data)}
                  emptyText="暂无公共库"
                  renderLeaf={({ item, isSelected, onSelect }) => (
                    <ScriptCard
                      key={item.id}
                      script={item.data}
                      isSelected={isSelected}
                      onSelect={onSelect}
                      onRun={() => runScript(scriptKey(item.data))}
                    />
                  )}
                />
              ) : (
                <div className="space-y-2">
                  {librarySourceGroups.map(({ source, items }) => (
                    <div key={source}>
                      <div className="px-2 py-1 text-[10px] font-medium text-foreground-tertiary/70 uppercase tracking-wider">
                        {source} ({items.length})
                      </div>
                      <div className="space-y-2">
                        {items.map((item) => (
                          <ScriptCard
                            key={scriptKey(item)}
                            script={item}
                            isSelected={selectedId === scriptKey(item)}
                            onSelect={() => setSelected(item)}
                            onRun={() => runScript(scriptKey(item))}
                          />
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>

        <div className="lg:col-span-2">
          {selected ? (
            <ScriptDetail script={selected} />
          ) : (
            <div className="flex items-center justify-center h-64 rounded-lg border border-border-subtle bg-card">
              <p className="text-sm text-foreground-tertiary">选择一个脚本查看详情</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
