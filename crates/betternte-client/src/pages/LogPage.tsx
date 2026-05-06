import { ArrowDown, Search, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { useEngineStore } from "@/lib/store";
import type { LogLevel } from "@/lib/types";
import { cn } from "@/lib/utils";

const levelColors: Record<LogLevel, string> = {
  debug: "text-foreground-tertiary",
  info: "text-foreground-secondary",
  warn: "text-warning",
  error: "text-destructive",
};

export function LogPage() {
  const logs = useEngineStore((s) => s.logs);
  const [search, setSearch] = useState("");
  const [levelFilter, setLevelFilter] = useState<LogLevel | "all">("all");
  const [autoScroll, setAutoScroll] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);

  const filtered = logs.filter((log) => {
    if (levelFilter !== "all" && log.level !== levelFilter) return false;
    if (search && !log.message.includes(search)) return false;
    return true;
  });

  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [filtered.length, autoScroll]);

  return (
    <div className="flex flex-col h-full p-6">
      <div className="flex items-center justify-between mb-5">
        <h1 className="text-lg font-semibold text-foreground">日志</h1>
        <div className="flex items-center gap-3">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-foreground-tertiary" />
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="搜索日志..."
              className="pl-9 pr-3 py-2 rounded-md bg-surface border border-border text-sm text-foreground placeholder:text-foreground-tertiary outline-none focus:border-primary w-48"
            />
          </div>
          <select
            value={levelFilter}
            onChange={(e) => setLevelFilter(e.target.value as typeof levelFilter)}
            className="bg-surface border border-border rounded-md px-3 py-2 text-sm text-foreground outline-none focus:border-primary"
          >
            <option value="all">全部级别</option>
            <option value="debug">Debug</option>
            <option value="info">Info</option>
            <option value="warn">Warn</option>
            <option value="error">Error</option>
          </select>
          <button
            onClick={() => {}}
            className="p-2 rounded-md bg-surface border border-border text-foreground-secondary hover:text-foreground hover:bg-surface-hover"
            title="清除日志"
          >
            <Trash2 className="w-4 h-4" />
          </button>
        </div>
      </div>

      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto rounded-lg border border-border-subtle bg-card p-4 font-mono text-xs"
      >
        {filtered.length === 0 ? (
          <div className="flex items-center justify-center h-full text-foreground-tertiary">
            暂无日志
          </div>
        ) : (
          <div className="space-y-1">
            {filtered.map((log, i) => (
              <div key={i} className="flex gap-2 py-0.5 hover:bg-surface-hover/50 rounded px-1 -mx-1">
                <span className="text-foreground-tertiary shrink-0 w-16">{log.timestamp}</span>
                <span
                  className={cn(
                    "shrink-0 uppercase w-12 text-center",
                    levelColors[log.level]
                  )}
                >
                  {log.level}
                </span>
                <span className="text-foreground-secondary break-all">{log.message}</span>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="flex items-center justify-between mt-3 text-xs text-foreground-tertiary">
        <div className="flex items-center gap-4">
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={autoScroll}
              onChange={(e) => setAutoScroll(e.target.checked)}
              className="w-3.5 h-3.5 rounded border-border accent-primary"
            />
            自动滚动
          </label>
          <span>最大行数: 1000</span>
        </div>
        <div className="flex items-center gap-2">
          <span>共 {filtered.length} 条</span>
          <button
            onClick={() => {
              if (scrollRef.current) {
                scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
              }
            }}
            className="p-1 rounded hover:bg-surface-hover"
          >
            <ArrowDown className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
}
