import { invoke } from "@tauri-apps/api/core";
import { ArrowDown, Download,Trash2, X } from "lucide-react";
import { useEffect,useRef, useState } from "react";

import { LOG_DRAWER_WIDTH_STYLE } from "@/lib/constants/layout";
import { useEngineStore } from "@/lib/store";
import type { LogLevel } from "@/lib/types";
import { cn } from "@/lib/utils";

const levelColors: Record<LogLevel, string> = {
  debug: "text-foreground-tertiary",
  info: "text-foreground-secondary",
  warn: "text-warning",
  error: "text-destructive",
};

export function LogDrawer() {
  const open = useEngineStore((s) => s.logDrawerOpen);
  const logs = useEngineStore((s) => s.logs);
  const toggleLogDrawer = useEngineStore((s) => s.toggleLogDrawer);
  const clearLogs = useEngineStore((s) => s.clearLogs);

  const [levelFilter, setLevelFilter] = useState<LogLevel | "all">("all");
  const [autoScroll, setAutoScroll] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);

  const filtered = logs.filter((log) => {
    if (levelFilter !== "all" && log.level !== levelFilter) return false;
    return true;
  });

  // Auto-scroll on new logs
  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [filtered.length, autoScroll]);

  // Scroll to bottom when drawer opens
  useEffect(() => {
    if (open && scrollRef.current) {
      requestAnimationFrame(() => {
        if (scrollRef.current) {
          scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
        }
      });
    }
  }, [open]);

  if (!open) return null;

  const handleClear = () => {
    clearLogs();
  };

  const handleExport = async () => {
    const content = filtered
      .map((log) => `[${log.timestamp}] [${log.level.toUpperCase()}] ${log.message}`)
      .join("\n");
    try {
      await invoke("export_logs", { content });
    } catch (e) {
      // User cancelled the dialog is not an error
      if (e !== "Export cancelled") {
        console.error("Failed to export logs:", e);
      }
    }
  };

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 bg-black/30 z-40 animate-in fade-in duration-200"
        onClick={toggleLogDrawer}
      />

      {/* Drawer panel — slides from right */}
      <div className="fixed top-0 right-0 bottom-0 z-50 animate-in slide-in-from-right duration-300">
        <div
          className="bg-card border-l border-border-subtle shadow-2xl flex flex-col h-full"
          style={{ ...LOG_DRAWER_WIDTH_STYLE }}
        >
          {/* Header */}
          <div className="flex items-center justify-between px-4 py-2.5 border-b border-border-subtle shrink-0">
            <div className="flex items-center gap-3">
              <span className="text-sm font-semibold text-foreground">日志</span>
              <span className="text-xs text-foreground-tertiary font-mono">
                {filtered.length} 条
              </span>
            </div>

            <div className="flex items-center gap-2">
              {/* Level filter */}
              <select
                value={levelFilter}
                onChange={(e) =>
                  setLevelFilter(e.target.value as typeof levelFilter)
                }
                className="bg-surface border border-border rounded-md px-2 py-1 text-xs text-foreground outline-none focus:border-primary"
              >
                <option value="all">全部</option>
                <option value="debug">Debug</option>
                <option value="info">Info</option>
                <option value="warn">Warn</option>
                <option value="error">Error</option>
              </select>

              {/* Auto-scroll toggle */}
              <label className="flex items-center gap-1.5 text-xs text-foreground-tertiary cursor-pointer">
                <input
                  type="checkbox"
                  checked={autoScroll}
                  onChange={(e) => setAutoScroll(e.target.checked)}
                  className="w-3 h-3 rounded border-border accent-primary"
                />
                自动滚动
              </label>

              {/* Clear logs */}
              <button
                onClick={handleClear}
                className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary hover:text-foreground"
                title="清空日志"
              >
                <Trash2 className="w-3.5 h-3.5" />
              </button>

              {/* Export logs */}
              <button
                onClick={handleExport}
                className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary hover:text-foreground"
                title="导出日志"
              >
                <Download className="w-3.5 h-3.5" />
              </button>

              {/* Scroll to bottom */}
              <button
                onClick={() => {
                  if (scrollRef.current) {
                    scrollRef.current.scrollTop =
                      scrollRef.current.scrollHeight;
                  }
                }}
                className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary hover:text-foreground"
              >
                <ArrowDown className="w-3.5 h-3.5" />
              </button>

              {/* Close */}
              <button
                onClick={toggleLogDrawer}
                className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary hover:text-foreground"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
          </div>

          {/* Log content */}
          <div
            ref={scrollRef}
            className="flex-1 overflow-y-auto p-3 font-mono text-xs"
          >
            {filtered.length === 0 ? (
              <div className="flex items-center justify-center h-full text-foreground-tertiary">
                暂无日志
              </div>
            ) : (
              <div className="space-y-0.5">
                {filtered.map((log, i) => (
                  <div
                    key={i}
                    className="flex gap-2 py-0.5 px-1 rounded hover:bg-surface-hover/50"
                  >
                    <span className="text-foreground-tertiary shrink-0 w-16">
                      {log.timestamp}
                    </span>
                    <span
                      className={cn(
                        "shrink-0 uppercase w-10 text-center",
                        levelColors[log.level]
                      )}
                    >
                      {log.level}
                    </span>
                    <span className="text-foreground-secondary break-all">
                      {log.message}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </>
  );
}
