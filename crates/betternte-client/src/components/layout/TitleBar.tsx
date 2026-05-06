import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Moon, Octagon, Square, Sun, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { useEngineStore } from "@/lib/store";

function useTheme() {
  const [theme, setTheme] = useState<"dark" | "light">(() => {
    const saved = localStorage.getItem("betternte-theme");
    return saved === "dark" ? "dark" : "light";
  });

  useEffect(() => {
    const root = document.documentElement;
    if (theme === "light") {
      root.classList.add("light");
    } else {
      root.classList.remove("light");
    }
    localStorage.setItem("betternte-theme", theme);
  }, [theme]);

  const toggle = () => setTheme((t) => (t === "dark" ? "light" : "dark"));

  return { theme, toggle };
}

export function TitleBar() {
  const { theme, toggle } = useTheme();
  const version = useEngineStore((s) => s.status.version ?? "0.0.1");
  const running = useEngineStore((s) => s.status.task);
  const stopAll = useEngineStore((s) => s.stopAll);
  const appWindow = useMemo(() => getCurrentWindow(), []);

  const handleMinimize = () => appWindow.minimize();
  const handleMaximize = async () => {
    const isMaximized = await appWindow.isMaximized();
    if (isMaximized) {
      appWindow.unmaximize();
    } else {
      appWindow.maximize();
    }
  };
  const handleClose = () => appWindow.close();

  return (
    <div
      data-tauri-drag-region
      className="flex h-9 items-center justify-between bg-background/80 backdrop-blur-md border-b border-border-subtle no-select"
    >
      {/* Left: logo + name */}
      <div className="flex items-center gap-2 pl-3" data-tauri-drag-region>
        <div className="w-4 h-4 rounded bg-primary flex items-center justify-center">
          <span className="text-[8px] font-bold text-primary-foreground">B</span>
        </div>
        <span className="text-xs font-medium text-foreground-secondary" data-tauri-drag-region>
          BetterNTE
        </span>
        <span className="text-[10px] text-foreground-tertiary font-mono" data-tauri-drag-region>
          v{version}
        </span>
      </div>

      {/* Right: stop button + theme toggle + window controls */}
      <div className="flex items-center">
        {running && (
          <button
            onClick={stopAll}
            className="flex items-center justify-center w-9 h-9 hover:bg-destructive/20 text-destructive transition-colors animate-pulse"
            title={`停止: ${running}`}
          >
            <Octagon className="w-4 h-4" />
          </button>
        )}
        <button
          onClick={toggle}
          className="flex items-center justify-center w-9 h-9 hover:bg-surface-hover text-foreground-secondary hover:text-foreground transition-colors"
          title={theme === "dark" ? "切换亮色主题" : "切换暗色主题"}
        >
          {theme === "dark" ? <Sun className="w-4 h-4" /> : <Moon className="w-4 h-4" />}
        </button>
        <button
          onClick={handleMinimize}
          className="flex items-center justify-center w-11 h-9 hover:bg-surface-hover text-foreground-secondary hover:text-foreground transition-colors"
          title="最小化"
        >
          <Minus className="w-4 h-4" />
        </button>
        <button
          onClick={handleMaximize}
          className="flex items-center justify-center w-11 h-9 hover:bg-surface-hover text-foreground-secondary hover:text-foreground transition-colors"
          title="最大化"
        >
          <Square className="w-3 h-3" />
        </button>
        <button
          onClick={handleClose}
          className="flex items-center justify-center w-11 h-9 hover:bg-destructive hover:text-primary-foreground text-foreground-secondary transition-colors"
          title="关闭"
        >
          <X className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}
