import { Bug,FileText } from "lucide-react";
import { useEffect,useState } from "react";

import { STATUS_BAR_TICK_MS } from "@/lib/constants/timing";
import { useEngineStore } from "@/lib/store";
import { cn } from "@/lib/utils";

const stateConfig: Record<string, { label: string; dot: string; text: string }> = {
  idle: { label: "空闲", dot: "bg-foreground-tertiary", text: "text-foreground-tertiary" },
  running: { label: "运行中", dot: "bg-success", text: "text-success" },
  error: { label: "错误", dot: "bg-destructive", text: "text-destructive" },
};

function formatElapsed(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
}

function ElapsedTime({ startedAt }: { startedAt: number }) {
  const [elapsed, setElapsed] = useState(() => Math.floor((Date.now() - startedAt) / 1000));

  useEffect(() => {
    const timer = setInterval(() => {
      setElapsed(Math.floor((Date.now() - startedAt) / 1000));
    }, STATUS_BAR_TICK_MS);
    return () => clearInterval(timer);
  }, [startedAt]);

  return (
    <span className="text-foreground-secondary font-mono">
      {formatElapsed(elapsed)}
    </span>
  );
}

export function StatusBar() {
  const status = useEngineStore((s) => s.status);
  const engineStartedAt = useEngineStore((s) => s.engineStartedAt);
  const controlStreamHealthy = useEngineStore((s) => s.controlStreamHealthy);
  const controlStreamStaleMs = useEngineStore((s) => s.controlStreamStaleMs);
  const toggleLogDrawer = useEngineStore((s) => s.toggleLogDrawer);
  const toggleDebug = useEngineStore((s) => s.toggleDebug);
  const logCount = useEngineStore((s) => s.logs.length);
  const debugCount = useEngineStore((s) => s.debugEntries.length);
  const state = stateConfig[status.state] ?? stateConfig.idle;
  const controlHealthTitle = controlStreamHealthy
    ? "控制事件流正常"
    : `控制事件流陈旧 ${Math.floor((controlStreamStaleMs ?? 0) / 1000)}s`;

  const [devMode, setDevMode] = useState(() => {
    return localStorage.getItem("betternte-developer-mode") === "true";
  });

  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      setDevMode(Boolean(detail));
    };
    window.addEventListener("developer-mode-changed", handler);
    return () => window.removeEventListener("developer-mode-changed", handler);
  }, []);

  return (
    <div className="flex items-center justify-between px-4 h-7 bg-surface/50 border-t border-border-subtle text-xs text-foreground-tertiary no-select">
      <div className="flex items-center gap-4">
        {/* Status dot + label */}
        <span className="flex items-center gap-1.5">
          <span className={cn("w-2 h-2 rounded-full shrink-0", state.dot)} />
          <span className={state.text}>{state.label}</span>
        </span>

        {/* Running task name */}
        {status.task && (
          <span>
            任务: <span className="text-foreground-secondary">{status.task}</span>
          </span>
        )}

        {/* Capture/input are dev-only details */}
        {devMode && status.capture_method && (
          <span>
            截图: <span className="text-foreground-secondary">{status.capture_method}</span>
          </span>
        )}
        {devMode && status.input_mode && (
          <span>
            输入: <span className="text-foreground-secondary">{status.input_mode}</span>
          </span>
        )}

        {/* Control stream health */}
        <span className="flex items-center gap-1.5" title={controlHealthTitle}>
          <span
            className={cn(
              "w-2 h-2 rounded-full shrink-0",
              controlStreamHealthy ? "bg-success" : "bg-warning"
            )}
          />
          <span className={cn(controlStreamHealthy ? "text-success" : "text-warning")}>
            控制流
          </span>
        </span>
      </div>

      <div className="flex items-center gap-3">
        {devMode && (
          <span>
            脚本: <span className="text-foreground-secondary">{status.script_count}</span>
          </span>
        )}

        {/* Elapsed time since engine start */}
        <span>
          运行时间:{" "}
          {engineStartedAt ? (
            <ElapsedTime startedAt={engineStartedAt} />
          ) : (
            <span className="text-foreground-secondary font-mono">0:00:00</span>
          )}
        </span>

        {/* Debug panel toggle (dev mode only) */}
        {devMode && (
          <button
            onClick={toggleDebug}
            className="flex items-center gap-1 px-1.5 py-0.5 rounded hover:bg-surface-hover text-foreground-tertiary hover:text-foreground transition-colors"
            title="调试面板"
          >
            <Bug className="w-3.5 h-3.5" />
            <span className="font-mono">{debugCount}</span>
          </button>
        )}

        {/* Log drawer — available to all users; floating layer mirrors the same stream */}
        <button
          onClick={toggleLogDrawer}
          className="flex items-center gap-1 px-1.5 py-0.5 rounded hover:bg-surface-hover text-foreground-tertiary hover:text-foreground transition-colors"
          title="查看日志侧栏"
        >
          <FileText className="w-3.5 h-3.5" />
          <span className="font-mono">{logCount}</span>
        </button>
      </div>
    </div>
  );
}
