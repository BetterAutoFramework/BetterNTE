import { ChevronDown, ChevronRight,Trash2, X } from "lucide-react";
import { useEffect,useRef, useState } from "react";

import { DEBUG_PANEL_WIDTH_STYLE, DEBUG_SCREENSHOT_THUMB_MAX } from "@/lib/constants/layout";
import { useEngineStore } from "@/lib/store";
import type { DebugCategory,DebugEntry } from "@/lib/types";
import { cn } from "@/lib/utils";

// ============================================================================
// Category config
// ============================================================================

const categoryConfig: Record<DebugCategory, { label: string; color: string; bg: string }> = {
  recognition: { label: "识别", color: "text-blue-400", bg: "bg-blue-500/10" },
  operation: { label: "操作", color: "text-green-400", bg: "bg-green-500/10" },
  wait: { label: "等待", color: "text-yellow-400", bg: "bg-yellow-500/10" },
  utility: { label: "工具", color: "text-purple-400", bg: "bg-purple-500/10" },
  capture: { label: "截图", color: "text-cyan-400", bg: "bg-cyan-500/10" },
  window: { label: "窗口", color: "text-orange-400", bg: "bg-orange-500/10" },
  log: { label: "日志", color: "text-foreground-tertiary", bg: "bg-surface" },
};

// ============================================================================
// Screenshot thumbnail
// ============================================================================

function ScreenshotThumb({
  base64,
  label,
  onClick,
}: {
  base64: string;
  label?: string;
  onClick?: () => void;
}) {
  return (
    <div className="inline-flex flex-col items-start gap-1">
      {label && (
        <span className="text-[10px] text-foreground-tertiary uppercase">{label}</span>
      )}
      <img
        src={`data:image/png;base64,${base64}`}
        alt={label || "screenshot"}
        className="rounded border border-border-subtle cursor-pointer hover:border-primary transition-colors"
        style={{
          maxWidth: DEBUG_SCREENSHOT_THUMB_MAX.w,
          maxHeight: DEBUG_SCREENSHOT_THUMB_MAX.h,
        }}
        onClick={onClick}
      />
    </div>
  );
}

// ============================================================================
// Screenshot modal
// ============================================================================

function ScreenshotModal({
  base64,
  onClose,
}: {
  base64: string;
  onClose: () => void;
}) {
  return (
    <>
      <div className="fixed inset-0 bg-black/60 z-[60]" onClick={onClose} />
      <div className="fixed inset-8 z-[60] flex items-center justify-center pointer-events-none">
        <img
          src={`data:image/png;base64,${base64}`}
          alt="screenshot full"
          className="max-w-full max-h-full rounded-lg shadow-2xl pointer-events-auto"
          onClick={onClose}
        />
      </div>
    </>
  );
}

// ============================================================================
// Debug entry card
// ============================================================================

function DebugEntryCard({ entry }: { entry: DebugEntry }) {
  const [expanded, setExpanded] = useState(false);
  const [modalSrc, setModalSrc] = useState<string | null>(null);
  const cat = categoryConfig[entry.category] ?? categoryConfig.utility;

  const timeStr = entry.timestamp.toLocaleTimeString("zh-CN");

  return (
    <>
      <div
        className={cn(
          "rounded border border-border-subtle p-2 text-xs font-mono",
          entry.success ? "bg-card" : "bg-destructive/5 border-destructive/30"
        )}
      >
        {/* Header row */}
        <div
          className="flex items-center gap-2 cursor-pointer select-none"
          onClick={() => setExpanded(!expanded)}
        >
          {expanded ? (
            <ChevronDown className="w-3 h-3 text-foreground-tertiary shrink-0" />
          ) : (
            <ChevronRight className="w-3 h-3 text-foreground-tertiary shrink-0" />
          )}

          <span className="text-foreground-tertiary shrink-0 w-16">{timeStr}</span>

          <span className={cn("px-1.5 py-0.5 rounded text-[10px] uppercase shrink-0", cat.bg, cat.color)}>
            {cat.label}
          </span>

          <span className="text-foreground font-medium truncate">{entry.method}</span>

          {entry.durationMs > 0 && (
            <span className="text-foreground-tertiary shrink-0 ml-auto">
              {entry.durationMs}ms
            </span>
          )}

          {!entry.success && (
            <span className="text-destructive text-[10px] shrink-0">FAIL</span>
          )}
        </div>

        {/* Expanded details */}
        {expanded && (
          <div className="mt-2 pl-5 space-y-2">
            {/* Args */}
            <div>
              <span className="text-foreground-tertiary text-[10px] uppercase">Args</span>
              <pre className="mt-0.5 text-foreground-secondary whitespace-pre-wrap break-all max-h-32 overflow-y-auto">
                {typeof entry.args === "string"
                  ? entry.args
                  : JSON.stringify(entry.args, null, 2)}
              </pre>
            </div>

            {/* Result */}
            {entry.result && (
              <div>
                <span className="text-foreground-tertiary text-[10px] uppercase">Result</span>
                <pre className="mt-0.5 text-foreground-secondary whitespace-pre-wrap break-all max-h-20 overflow-y-auto">
                  {entry.result}
                </pre>
              </div>
            )}

            {/* Error */}
            {entry.error && (
              <div>
                <span className="text-destructive text-[10px] uppercase">Error</span>
                <pre className="mt-0.5 text-destructive whitespace-pre-wrap break-all">
                  {entry.error}
                </pre>
              </div>
            )}

            {/* Screenshots */}
            {(entry.screenshotBefore || entry.screenshotAfter) && (
              <div className="flex gap-3 flex-wrap">
                {entry.screenshotBefore && (
                  <ScreenshotThumb
                    base64={entry.screenshotBefore}
                    label={entry.screenshotAfter ? "Before" : undefined}
                    onClick={() => setModalSrc(entry.screenshotBefore)}
                  />
                )}
                {entry.screenshotAfter && (
                  <ScreenshotThumb
                    base64={entry.screenshotAfter}
                    label="After"
                    onClick={() => setModalSrc(entry.screenshotAfter)}
                  />
                )}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Full-size screenshot modal */}
      {modalSrc && (
        <ScreenshotModal base64={modalSrc} onClose={() => setModalSrc(null)} />
      )}
    </>
  );
}

// ============================================================================
// DebugPanel
// ============================================================================

export function DebugPanel() {
  const open = useEngineStore((s) => s.debugOpen);
  const entries = useEngineStore((s) => s.debugEntries);
  const toggleDebug = useEngineStore((s) => s.toggleDebug);
  const clearDebug = useEngineStore((s) => s.clearDebug);

  const [categoryFilter, setCategoryFilter] = useState<DebugCategory | "all">("all");
  const [autoScroll, setAutoScroll] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);

  const filtered = entries.filter((e) => {
    if (categoryFilter !== "all" && e.category !== categoryFilter) return false;
    return true;
  });

  // Auto-scroll on new entries
  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [filtered.length, autoScroll]);

  // Scroll to bottom when panel opens
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

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 bg-black/30 z-40 animate-in fade-in duration-200"
        onClick={toggleDebug}
      />

      {/* Panel — slides from right */}
      <div className="fixed top-0 right-0 bottom-0 z-50 animate-in slide-in-from-right duration-300">
        <div
          className="bg-card border-l border-border-subtle shadow-2xl flex flex-col h-full"
          style={{ ...DEBUG_PANEL_WIDTH_STYLE }}
        >
          {/* Header */}
          <div className="flex items-center justify-between px-4 py-2.5 border-b border-border-subtle shrink-0">
            <div className="flex items-center gap-3">
              <span className="text-sm font-semibold text-foreground">调试面板</span>
              <span className="text-xs text-foreground-tertiary font-mono">
                {filtered.length} 条
              </span>
            </div>

            <div className="flex items-center gap-2">
              {/* Category filter */}
              <select
                value={categoryFilter}
                onChange={(e) =>
                  setCategoryFilter(e.target.value as typeof categoryFilter)
                }
                className="bg-surface border border-border rounded-md px-2 py-1 text-xs text-foreground outline-none focus:border-primary"
              >
                <option value="all">全部</option>
                <option value="recognition">识别</option>
                <option value="operation">操作</option>
                <option value="wait">等待</option>
                <option value="utility">工具</option>
                <option value="capture">截图</option>
                <option value="window">窗口</option>
                <option value="log">日志</option>
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

              {/* Clear */}
              <button
                onClick={clearDebug}
                className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary hover:text-foreground"
                title="清空调试记录"
              >
                <Trash2 className="w-3.5 h-3.5" />
              </button>

              {/* Close */}
              <button
                onClick={toggleDebug}
                className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary hover:text-foreground"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
          </div>

          {/* Entry list */}
          <div
            ref={scrollRef}
            className="flex-1 overflow-y-auto p-3 space-y-1.5"
          >
            {filtered.length === 0 ? (
              <div className="flex items-center justify-center h-full text-foreground-tertiary text-sm">
                暂无调试记录
              </div>
            ) : (
              filtered.map((entry) => (
                <DebugEntryCard key={entry.id} entry={entry} />
              ))
            )}
          </div>
        </div>
      </div>
    </>
  );
}
