import { javascript } from "@codemirror/lang-javascript";
import { EditorState } from "@codemirror/state";
import { oneDark } from "@codemirror/theme-one-dark";
import { keymap } from "@codemirror/view";
import { open } from "@tauri-apps/plugin-dialog";
import { basicSetup, EditorView } from "codemirror";
import {
  Bug,
  ChevronDown,
  ChevronRight,
  Copy,
  File,
  FileCode,
  FolderOpen,
  FolderTree,
  List,
  Monitor,
  PanelLeftClose,
  PanelLeftOpen,
  Play,
  Plus,
  RefreshCw,
  Save,
  Square,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";

import { PathTree, type PathTreeItem } from "@/components/PathTree";
import { PANEL_THRESHOLD_PX } from "@/lib/constants/layout";
import { SCRIPT_EDITOR_AUTOSAVE_MS } from "@/lib/constants/timing";
import { useEngineStore } from "@/lib/store";
import { scriptKey } from "@/lib/stores/helpers";
import { buildScriptTreePath } from "@/lib/treePaths";
import type { ScriptInfo } from "@/lib/types";
import { cn } from "@/lib/utils";

interface OutputLine {
  type: "log" | "result" | "error" | "info";
  text: string;
  timestamp: string;
}

const PERMISSION_LABELS: Record<string, string> = {
  screenshot: "截图",
  click: "点击",
  input: "输入控制",
  keyboard: "键盘输入",
  mouse: "鼠标输入",
  ocr: "OCR 识别",
  template_match: "模板匹配",
  color_detect: "颜色检测",
  window: "窗口访问",
  storage: "本地存储",
  network: "网络请求",
  file: "文件访问",
  notify: "通知",
  call_script: "调用脚本",
  call_library: "调用库",
};

function permissionLabel(permission: string): string {
  return PERMISSION_LABELS[permission] ?? `未定义权限 (${permission})`;
}

const libraryCallSnippet = `// 在任务/触发器脚本中调用公共库
const sum = await ctx.call("common_api", "sum", { a: 1, b: 2 });
ctx.logInfo("sum=" + sum);`;

const libraryExportSnippet = `// 在 library 脚本中导出函数（registerLibrary 由引擎注入）
registerLibrary("sum", async function (args) {
  return Number(args?.a ?? 0) + Number(args?.b ?? 0);
});`;

function now() {
  return new Date().toLocaleTimeString("zh-CN", { hour12: false });
}

// ============================================================================
// New Script Dialog
// ============================================================================

function NewScriptDialog({
  open,
  onClose,
  onCreate,
}: {
  open: boolean;
  onClose: () => void;
  onCreate: (name: string, displayName: string, scriptType: string, description: string) => void;
}) {
  const [name, setName] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [scriptType, setScriptType] = useState("task");
  const [description, setDescription] = useState("");

  if (!open) return null;

  const canCreate = name.trim().length > 0;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
      <div
        className="bg-surface border border-border rounded-lg shadow-lg w-96 p-4"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-sm font-semibold text-foreground">新建脚本</h3>
          <button onClick={onClose} className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary">
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="space-y-3">
          <div>
            <label className="block text-xs text-foreground-secondary mb-1">标识名 (英文)</label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value.replace(/[^a-zA-Z0-9_-]/g, "_"))}
              placeholder="my_script"
              className="w-full px-3 py-1.5 rounded-md bg-surface border border-border text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
            />
          </div>
          <div>
            <label className="block text-xs text-foreground-secondary mb-1">显示名称</label>
            <input
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder="我的脚本"
              className="w-full px-3 py-1.5 rounded-md bg-surface border border-border text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
            />
          </div>
          <div>
            <label className="block text-xs text-foreground-secondary mb-1">类型</label>
            <select
              value={scriptType}
              onChange={(e) => setScriptType(e.target.value)}
              className="w-full px-3 py-1.5 rounded-md bg-surface border border-border text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
            >
              <option value="task">脚本 (task)</option>
              <option value="trigger">触发器 (trigger)</option>
            </select>
          </div>
          <div>
            <label className="block text-xs text-foreground-secondary mb-1">描述</label>
            <input
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="可选描述"
              className="w-full px-3 py-1.5 rounded-md bg-surface border border-border text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
            />
          </div>
        </div>

        <div className="flex justify-end gap-2 mt-4">
          <button
            onClick={onClose}
            className="px-3 py-1.5 rounded-md text-xs text-foreground-secondary hover:text-foreground hover:bg-surface-hover"
          >
            取消
          </button>
          <button
            onClick={() => {
              if (!canCreate) return;
              onCreate(name.trim(), displayName.trim() || name.trim(), scriptType, description.trim());
              setName("");
              setDisplayName("");
              setScriptType("task");
              setDescription("");
              onClose();
            }}
            disabled={!canCreate}
            className="px-3 py-1.5 rounded-md text-xs font-medium bg-primary text-primary-foreground hover:bg-primary-hover disabled:opacity-50"
          >
            创建
          </button>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// Main Component
// ============================================================================

export function ScriptDebugPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const scripts = useEngineStore((s) => s.scripts);
  const triggers = useEngineStore((s) => s.triggers);
  const readScriptSource = useEngineStore((s) => s.readScriptSource);
  const saveScriptSource = useEngineStore((s) => s.saveScriptSource);
  const importScriptAsset = useEngineStore((s) => s.importScriptAsset);
  const createScript = useEngineStore((s) => s.createScript);
  const listScriptFiles = useEngineStore((s) => s.listScriptFiles);
  const runScript = useEngineStore((s) => s.runScript);
  const stopTask = useEngineStore((s) => s.stopTask);
  const testScreenshot = useEngineStore((s) => s.testScreenshot);
  const status = useEngineStore((s) => s.status);

  const [selectedScript, setSelectedScript] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<"list" | "tree">("list");
  const [fileName, setFileName] = useState("main.js");
  const [output, setOutput] = useState<OutputLine[]>([]);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [panelCollapsed, setPanelCollapsed] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [scriptFiles, setScriptFiles] = useState<string[]>([]);
  const [previewSrc, setPreviewSrc] = useState<string | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewCollapsed, setPreviewCollapsed] = useState(false);
  const manualPanelRef = useRef(false);

  const editorRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const autoSaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const handleSaveRef = useRef<() => Promise<void>>(() => Promise.resolve());

  // Merge scripts + triggers
  const allEditable = useMemo(() => [...scripts, ...triggers], [scripts, triggers]);

  const focusFromUrl = searchParams.get("focus");
  useEffect(() => {
    if (!focusFromUrl || allEditable.length === 0) return;
    const found = allEditable.find((s) => scriptKey(s) === focusFromUrl);
    if (!found) return;
    setSelectedScript(scriptKey(found));
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.delete("focus");
        return next;
      },
      { replace: true }
    );
  }, [focusFromUrl, allEditable, setSearchParams]);

  // Find selected script info (has dir field)
  const selectedScriptInfo = useMemo(
    () => allEditable.find((s) => scriptKey(s) === selectedScript) ?? null,
    [allEditable, selectedScript]
  );
  const selectedPermissions = useMemo(
    () =>
      [...(selectedScriptInfo?.permissions ?? [])]
        .sort((a, b) => a.localeCompare(b))
        .map((p) => ({ key: p, label: permissionLabel(p) })),
    [selectedScriptInfo?.permissions]
  );
  const editableTreeItems = useMemo<PathTreeItem<ScriptInfo>[]>(
    () =>
      allEditable.map((item) => {
        const isTrigger = item.type === "trigger";
        const prefix = isTrigger ? "触发器" : "脚本";
        const sourceAndRelative = buildScriptTreePath(item);
        return {
          id: scriptKey(item),
          label: item.display_name || item.name,
          path: `${sourceAndRelative}/${prefix}`,
          data: item,
        };
      }),
    [allEditable]
  );
  const sourceGroups = useMemo(() => {
    const groups = new Map<string, { scripts: ScriptInfo[]; triggers: ScriptInfo[] }>();
    for (const script of scripts) {
      const source = script.source?.trim() || "未分类";
      if (!groups.has(source)) groups.set(source, { scripts: [], triggers: [] });
      groups.get(source)!.scripts.push(script);
    }
    for (const trigger of triggers) {
      const source = trigger.source?.trim() || "未分类";
      if (!groups.has(source)) groups.set(source, { scripts: [], triggers: [] });
      groups.get(source)!.triggers.push(trigger);
    }
    return Array.from(groups.entries())
      .sort(([a], [b]) => a.localeCompare(b, "zh-CN"))
      .map(([source, value]) => ({ source, ...value }));
  }, [scripts, triggers]);

  // Append output line
  const appendOutput = useCallback(
    (type: OutputLine["type"], text: string) => {
      setOutput((prev) => [...prev, { type, text, timestamp: now() }]);
    },
    []
  );

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

  // Load script files when selected script changes
  useEffect(() => {
    if (selectedScriptInfo?.dir) {
      listScriptFiles(selectedScriptInfo.dir)
        .then(setScriptFiles)
        .catch((e) => {
          console.error("[ScriptDebug] listScriptFiles:", e);
          setScriptFiles([]);
        });
      setFileName("main.js");
    } else {
      setScriptFiles([]);
    }
  }, [selectedScriptInfo?.dir, listScriptFiles]);

  // Load source when script or file changes
  useEffect(() => {
    if (!selectedScriptInfo?.dir || !fileName) return;
    const filePath = `${selectedScriptInfo.dir}/${fileName}`;

    readScriptSource(filePath)
      .then((content) => {
        if (viewRef.current) {
          viewRef.current.dispatch({
            changes: {
              from: 0,
              to: viewRef.current.state.doc.length,
              insert: content,
            },
          });
        }
        setDirty(false);
      })
      .catch((e) => {
        appendOutput("error", `加载失败 ${filePath}: ${e}`);
      });
  }, [selectedScriptInfo?.dir, fileName, readScriptSource, appendOutput]);

  // handleSave using ref to avoid stale closure
  const handleSave = useCallback(async () => {
    const info = selectedScriptInfo;
    if (!info?.dir || !viewRef.current) return;
    setSaving(true);
    try {
      const content = viewRef.current.state.doc.toString();
      const filePath = `${info.dir}/${fileName}`;
      await saveScriptSource(filePath, content);
      setDirty(false);
      appendOutput("info", `已保存 ${filePath}`);
    } catch (e) {
      appendOutput("error", `保存失败: ${e}`);
    } finally {
      setSaving(false);
    }
  }, [selectedScriptInfo, fileName, saveScriptSource, appendOutput]);

  // Keep ref up to date
  useEffect(() => {
    handleSaveRef.current = handleSave;
  }, [handleSave]);

  // Initialize CodeMirror
  useEffect(() => {
    if (!editorRef.current) return;
    if (viewRef.current) {
      viewRef.current.destroy();
    }

    const saveKeymap = keymap.of([
      {
        key: "Mod-s",
        run: () => {
          handleSaveRef.current();
          return true;
        },
      },
    ]);

    const state = EditorState.create({
      doc: "// 在左侧选择脚本后开始编辑\n",
      extensions: [
        basicSetup,
        javascript(),
        oneDark,
        saveKeymap,
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            setDirty(true);
            if (autoSaveTimer.current) clearTimeout(autoSaveTimer.current);
            autoSaveTimer.current = setTimeout(() => {
              handleSaveRef.current();
            }, SCRIPT_EDITOR_AUTOSAVE_MS);
          }
        }),
        EditorView.theme({
          "&": { height: "100%" },
          ".cm-scroller": { fontFamily: "monospace" },
        }),
      ],
    });

    const view = new EditorView({
      state,
      parent: editorRef.current,
    });
    viewRef.current = view;

    return () => {
      if (autoSaveTimer.current) {
        clearTimeout(autoSaveTimer.current);
        autoSaveTimer.current = null;
      }
      view.destroy();
      viewRef.current = null;
    };
  }, []);

  const handleRun = async () => {
    if (!selectedScriptInfo) return;
    await handleSave();
    appendOutput("info", `正在运行脚本: ${selectedScriptInfo.name}…`);
    try {
      await runScript(scriptKey(selectedScriptInfo));
      appendOutput("result", "脚本执行完成");
    } catch (e) {
      appendOutput("error", `脚本执行失败: ${e}`);
    }
  };

  const handleStop = async () => {
    await stopTask();
    appendOutput("info", "任务已停止");
  };

  const handleImportAsset = async () => {
    if (!selectedScriptInfo) return;
    try {
      const filePath = await open({
        multiple: false,
        title: "选择要导入的资源文件",
      });
      if (!filePath) return;
      const result = await importScriptAsset(selectedScriptInfo.name, filePath as string);
      appendOutput("info", `已导入资源: ${result}`);
      // Refresh file list
      if (selectedScriptInfo?.dir) {
        listScriptFiles(selectedScriptInfo.dir)
          .then(setScriptFiles)
          .catch((e) => console.error("[ScriptDebug] listScriptFiles after import:", e));
      }
    } catch (e) {
      appendOutput("error", `导入失败: ${e}`);
    }
  };

  const handleRefreshPreview = async () => {
    setPreviewLoading(true);
    try {
      const src = await testScreenshot();
      setPreviewSrc(src);
    } catch (e) {
      appendOutput("error", `截图失败: ${e}`);
    } finally {
      setPreviewLoading(false);
    }
  };

  const copySnippet = async (name: string, content: string) => {
    try {
      await navigator.clipboard.writeText(content);
      appendOutput("info", `已复制片段: ${name}`);
    } catch (e) {
      appendOutput("error", `复制失败: ${e}`);
    }
  };

  const isRunning = status.task !== null;

  const outputTypeColor = {
    log: "text-foreground-secondary",
    result: "text-success",
    error: "text-destructive",
    info: "text-primary",
  };

  return (
    <div className="flex h-full">
      <NewScriptDialog
        open={dialogOpen}
        onClose={() => setDialogOpen(false)}
        onCreate={(name, displayName, scriptType, desc) => createScript(name, displayName, scriptType, desc)}
      />

      {/* Left: Script file list */}
      {panelCollapsed ? (
        <div className="flex flex-col items-center py-3 px-1.5 border-r border-border-subtle bg-surface/30 shrink-0">
          <button
            onClick={() => { manualPanelRef.current = true; setPanelCollapsed(false); }}
            className="p-2 rounded-md hover:bg-surface-hover text-foreground-tertiary hover:text-foreground"
            title="展开文件列表"
          >
            <PanelLeftOpen className="w-4 h-4" />
          </button>
        </div>
      ) : (
      <div className="w-56 border-r border-border-subtle bg-surface/30 flex flex-col">
        <div className="p-3 border-b border-border-subtle">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2 text-sm font-medium text-foreground">
              <FolderOpen className="w-4 h-4" />
              资源管理
            </div>
            <div className="flex items-center gap-1">
              <div className="mr-1 rounded-md border border-border-subtle bg-surface p-0.5 flex">
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
              <button
                onClick={() => setDialogOpen(true)}
                className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary hover:text-foreground"
                title="新建脚本/触发器"
              >
                <Plus className="w-3.5 h-3.5" />
              </button>
              <button
                onClick={() => { manualPanelRef.current = true; setPanelCollapsed(true); }}
                className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary"
                title="收起文件列表"
              >
                <PanelLeftClose className="w-3.5 h-3.5" />
              </button>
            </div>
          </div>
        </div>

        {/* Script/trigger list */}
        <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
          {allEditable.length === 0 ? (
            <div className="text-xs text-foreground-tertiary px-3 py-2">
              暂无脚本
            </div>
          ) : (
            viewMode === "tree" ? (
              <PathTree
                items={editableTreeItems}
                selectedId={selectedScript}
                onSelect={(item) => setSelectedScript(item.id)}
                emptyText="暂无脚本"
                renderLeaf={({ item, isSelected, onSelect }) => {
                  const isTrigger = item.data.type === "trigger";
                  return (
                    <button
                      onClick={onSelect}
                      className={cn(
                        "flex items-center gap-2 w-full px-3 py-2 rounded-md text-sm transition-colors text-left",
                        isSelected
                          ? "bg-primary/15 text-primary"
                          : "text-foreground-secondary hover:text-foreground hover:bg-surface-hover"
                      )}
                    >
                      {isTrigger ? (
                        <Bug className="w-4 h-4 shrink-0" />
                      ) : (
                        <FileCode className="w-4 h-4 shrink-0" />
                      )}
                      <span className="truncate">{item.data.display_name || item.data.name}</span>
                    </button>
                  );
                }}
              />
            ) : (
              <div className="space-y-2">
                {sourceGroups.map(({ source, scripts: srcScripts, triggers: srcTriggers }) => (
                  <div key={source}>
                    <div className="px-2 py-1 text-[10px] font-medium text-foreground-tertiary/70 uppercase tracking-wider">
                      {source} ({srcScripts.length + srcTriggers.length})
                    </div>
                    {srcScripts.length > 0 && (
                      <div className="px-2 py-1 text-[10px] font-medium text-foreground-tertiary/60 uppercase tracking-wider">
                        脚本
                      </div>
                    )}
                    {srcScripts.map((script) => (
                      <button
                        key={scriptKey(script)}
                        onClick={() => setSelectedScript(scriptKey(script))}
                        className={cn(
                          "flex items-center gap-2 w-full px-3 py-2 rounded-md text-sm transition-colors text-left",
                          selectedScript === scriptKey(script)
                            ? "bg-primary/15 text-primary"
                            : "text-foreground-secondary hover:text-foreground hover:bg-surface-hover"
                        )}
                      >
                        <FileCode className="w-4 h-4 shrink-0" />
                        <span className="truncate">{script.display_name || script.name}</span>
                      </button>
                    ))}
                    {srcTriggers.length > 0 && (
                      <div className="px-2 py-1 mt-1 text-[10px] font-medium text-foreground-tertiary/60 uppercase tracking-wider">
                        触发器
                      </div>
                    )}
                    {srcTriggers.map((trigger) => (
                      <button
                        key={scriptKey(trigger)}
                        onClick={() => setSelectedScript(scriptKey(trigger))}
                        className={cn(
                          "flex items-center gap-2 w-full px-3 py-2 rounded-md text-sm transition-colors text-left",
                          selectedScript === scriptKey(trigger)
                            ? "bg-primary/15 text-primary"
                            : "text-foreground-secondary hover:text-foreground hover:bg-surface-hover"
                        )}
                      >
                        <Bug className="w-4 h-4 shrink-0" />
                        <span className="truncate">{trigger.display_name || trigger.name}</span>
                      </button>
                    ))}
                  </div>
                ))}
              </div>
            )
          )}
        </div>

        {/* Resource files when a script is selected */}
        {selectedScriptInfo && (
          <div className="border-t border-border-subtle p-2 space-y-0.5 max-h-48 overflow-y-auto">
            <div className="px-2 py-1 text-[10px] font-medium text-foreground-tertiary/60 uppercase tracking-wider">
              资源文件
            </div>
            {scriptFiles.length === 0 ? (
              <div className="text-xs text-foreground-tertiary px-3 py-1">无文件</div>
            ) : (
              scriptFiles.map((f) => (
                <button
                  key={f}
                  onClick={() => setFileName(f)}
                  className={cn(
                    "flex items-center gap-2 w-full px-3 py-1.5 rounded-md text-xs transition-colors",
                    fileName === f
                      ? "bg-surface text-foreground"
                      : "text-foreground-tertiary hover:text-foreground"
                  )}
                >
                  <File className="w-3 h-3" />
                  <span className="truncate">{f}</span>
                </button>
              ))
            )}
          </div>
        )}
      </div>
      )}

      {/* Center: Editor */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Toolbar */}
        <div className="flex items-center gap-2 px-4 py-2 border-b border-border-subtle bg-surface/30">
          <div className="flex items-center gap-1.5 text-sm text-foreground-secondary">
            <Bug className="w-4 h-4" />
            {selectedScript ? (
              <span className="font-mono text-xs">
                {selectedScript}/{fileName}
                {dirty && <span className="text-warning ml-1">*</span>}
              </span>
            ) : (
              <span>请选择脚本</span>
            )}
          </div>

          <div className="flex-1" />

          <button
            onClick={handleSave}
            disabled={!selectedScript || saving}
            className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-md text-xs font-medium bg-surface border border-border text-foreground hover:bg-surface-hover disabled:opacity-50"
          >
            <Save className="w-3.5 h-3.5" />
            {saving ? "保存中…" : "保存"}
          </button>

          <button
            onClick={handleImportAsset}
            disabled={!selectedScript}
            className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-md text-xs font-medium bg-surface border border-border text-foreground hover:bg-surface-hover disabled:opacity-50"
          >
            <Upload className="w-3.5 h-3.5" />
            导入资源
          </button>

          {isRunning ? (
            <button
              onClick={handleStop}
              className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-md text-xs font-medium bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              <Square className="w-3.5 h-3.5" />
              停止
            </button>
          ) : (
            <button
              onClick={handleRun}
              disabled={!selectedScript}
              className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-md text-xs font-medium bg-primary text-primary-foreground hover:bg-primary-hover disabled:opacity-50"
            >
              <Play className="w-3.5 h-3.5" />
              运行
            </button>
          )}
        </div>

        {/* CodeMirror container */}
        <div ref={editorRef} className="flex-1 overflow-hidden" />
      </div>

      {/* Right: Preview + Output */}
      <div className="w-80 border-l border-border-subtle bg-surface/30 flex flex-col">
        {/* Game Preview */}
        <div className="border-b border-border-subtle">
          <div className="flex items-center justify-between px-3 py-2">
            <button
              onClick={() => setPreviewCollapsed(!previewCollapsed)}
              className="flex items-center gap-1.5 text-sm font-medium text-foreground hover:text-foreground"
            >
              {previewCollapsed ? (
                <ChevronRight className="w-3.5 h-3.5" />
              ) : (
                <ChevronDown className="w-3.5 h-3.5" />
              )}
              <Monitor className="w-4 h-4" />
              游戏预览
            </button>
            <button
              onClick={handleRefreshPreview}
              disabled={previewLoading}
              className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary disabled:opacity-50"
              title="刷新截图"
            >
              <RefreshCw className={cn("w-3.5 h-3.5", previewLoading && "animate-spin")} />
            </button>
          </div>
          {!previewCollapsed && (
            <div className="px-3 pb-3">
              {previewSrc ? (
                <img
                  src={previewSrc}
                  alt="游戏预览"
                  className="w-full rounded border border-border-subtle"
                />
              ) : (
                <div className="flex items-center justify-center h-32 rounded border border-dashed border-border-subtle text-xs text-foreground-tertiary">
                  点击刷新按钮截图预览
                </div>
              )}
            </div>
          )}
        </div>

        {/* Output */}
        <div className="flex items-center justify-between px-3 py-2 border-b border-border-subtle">
          <span className="text-sm font-medium text-foreground">输出</span>
          <button
            onClick={() => setOutput([])}
            className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary"
            title="清空输出"
          >
            <Trash2 className="w-3.5 h-3.5" />
          </button>
        </div>
        <div className="px-3 py-2 border-b border-border-subtle space-y-2">
          <div className="space-y-1.5">
            <div className="text-xs font-medium text-foreground-secondary">权限</div>
            {selectedScriptInfo ? (
              selectedPermissions.length === 0 ? (
                <div className="text-xs text-foreground-tertiary">无</div>
              ) : (
                <div className="flex flex-wrap gap-1.5">
                  {selectedPermissions.map((p) => (
                    <span
                      key={p.key}
                      className="px-2 py-0.5 rounded-md text-[11px] font-mono bg-surface border border-border-subtle text-foreground-secondary"
                    >
                      {p.label}
                    </span>
                  ))}
                </div>
              )
            ) : (
              <div className="text-xs text-foreground-tertiary">请选择脚本或触发器</div>
            )}
          </div>
          <div className="text-xs font-medium text-foreground-secondary">ctx.call 示例片段</div>
          <div className="flex gap-2">
            <button
              onClick={() => copySnippet("调用示例", libraryCallSnippet)}
              className="flex items-center gap-1.5 px-2 py-1 rounded-md bg-surface border border-border text-xs text-foreground hover:bg-surface-hover"
            >
              <Copy className="w-3.5 h-3.5" />
              复制调用示例
            </button>
            <button
              onClick={() => copySnippet("导出示例", libraryExportSnippet)}
              className="flex items-center gap-1.5 px-2 py-1 rounded-md bg-surface border border-border text-xs text-foreground hover:bg-surface-hover"
            >
              <Copy className="w-3.5 h-3.5" />
              复制导出示例
            </button>
          </div>
        </div>
        <div className="flex-1 overflow-y-auto p-3 font-mono text-xs space-y-1">
          {output.length === 0 && (
            <div className="text-foreground-tertiary">
              运行脚本后，输出将显示在此处
            </div>
          )}
          {output.map((line, i) => (
            <div key={i} className={cn("whitespace-pre-wrap break-all", outputTypeColor[line.type])}>
              <span className="text-foreground-tertiary mr-2">{line.timestamp}</span>
              {line.text}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
