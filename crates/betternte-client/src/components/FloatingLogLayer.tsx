import { ChevronDown, ChevronUp, FileText, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { useEngineStore } from "@/lib/store";
import type { LogLevel } from "@/lib/types";
import { cn } from "@/lib/utils";

const COLLAPSED_KEY = "betternte-floating-log-collapsed";
const MAX_LINES = 80;

const levelColors: Record<LogLevel, string> = {
  debug: "text-foreground-tertiary",
  info: "text-foreground-secondary",
  warn: "text-warning",
  error: "text-destructive",
};

/**
 * Compact floating log panel above the status bar so script / engine logs stay visible
 * while using other pages (not only the full log drawer).
 */
export function FloatingLogLayer() {
  const logs = useEngineStore((s) => s.logs);
  const status = useEngineStore((s) => s.status);
  const logDrawerOpen = useEngineStore((s) => s.logDrawerOpen);
  const toggleLogDrawer = useEngineStore((s) => s.toggleLogDrawer);

  const [collapsed, setCollapsed] = useState(() => {
    if (typeof localStorage === "undefined") return false;
    return localStorage.getItem(COLLAPSED_KEY) === "true";
  });
  const [dismissed, setDismissed] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);
  const prevLogCountRef = useRef(0);

  const showChrome = status.state === "running" || logs.length > 0;
  const lines = useMemo(() => logs.slice(-MAX_LINES), [logs]);

  useEffect(() => {
    localStorage.setItem(COLLAPSED_KEY, String(collapsed));
  }, [collapsed]);

  useEffect(() => {
    if (logs.length > prevLogCountRef.current) {
      setDismissed(false);
    }
    prevLogCountRef.current = logs.length;
  }, [logs.length]);

  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [lines.length, autoScroll, collapsed]);

  if (logDrawerOpen) {
    return null;
  }

  if (dismissed || !showChrome) {
    return null;
  }

  if (collapsed) {
    return (
      <div className="fixed bottom-8 right-4 z-[45] flex items-center gap-1">
        <button
          type="button"
          onClick={() => setCollapsed(false)}
          className="flex items-center gap-1.5 rounded-full border border-border-subtle bg-card/95 px-3 py-1.5 text-xs text-foreground shadow-lg backdrop-blur-sm hover:bg-surface-hover"
        >
          <FileText className="w-3.5 h-3.5 text-foreground-tertiary" />
          <span>日志</span>
          {logs.length > 0 && (
            <span className="font-mono text-foreground-tertiary">{logs.length}</span>
          )}
          <ChevronUp className="w-3.5 h-3.5 text-foreground-tertiary" />
        </button>
      </div>
    );
  }

  return (
    <div
      className={cn(
        "fixed bottom-8 right-4 z-[45] flex w-[min(100%-2rem,28rem)] max-h-[min(40vh,320px)] flex-col overflow-hidden",
        "rounded-lg border border-border-subtle bg-card/95 shadow-2xl backdrop-blur-sm"
      )}
    >
      <div className="flex items-center justify-between gap-2 border-b border-border-subtle px-2.5 py-1.5 shrink-0">
        <div className="flex items-center gap-2 min-w-0">
          <span className="text-xs font-semibold text-foreground truncate">日志浮层</span>
          {status.task && (
            <span className="text-[10px] text-foreground-tertiary truncate max-w-40">
              {status.task}
            </span>
          )}
        </div>
        <div className="flex items-center gap-0.5 shrink-0">
          <label className="flex items-center gap-1 px-1 text-[10px] text-foreground-tertiary cursor-pointer select-none">
            <input
              type="checkbox"
              checked={autoScroll}
              onChange={(e) => setAutoScroll(e.target.checked)}
              className="w-3 h-3 rounded border-border accent-primary"
            />
            跟随
          </label>
          <button
            type="button"
            onClick={toggleLogDrawer}
            className="rounded p-1 text-[10px] text-foreground-secondary hover:bg-surface-hover"
            title="打开完整日志侧栏"
          >
            侧栏
          </button>
          <button
            type="button"
            onClick={() => setCollapsed(true)}
            className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary"
            title="收起"
          >
            <ChevronDown className="w-4 h-4" />
          </button>
          <button
            type="button"
            onClick={() => setDismissed(true)}
            className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary"
            title="关闭浮层（任务开始后或产生新日志可再次出现）"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      </div>
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto px-2.5 py-2 font-mono text-[11px] leading-snug"
      >
        {lines.length === 0 ? (
          <div className="text-foreground-tertiary py-4 text-center text-xs">暂无日志</div>
        ) : (
          <div className="space-y-0.5">
            {lines.map((log, i) => (
              <div key={`${log.timestamp}-${i}`} className="flex gap-1.5">
                <span className="text-foreground-tertiary shrink-0 w-14">{log.timestamp}</span>
                <span
                  className={cn(
                    "shrink-0 uppercase w-9 text-[9px] leading-4 text-center",
                    levelColors[log.level as LogLevel] ?? "text-foreground-secondary"
                  )}
                >
                  {log.level}
                </span>
                <span className="text-foreground-secondary break-all min-w-0">{log.message}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
