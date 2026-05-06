import { Loader2,Search, X } from "lucide-react";
import { useCallback,useEffect, useState } from "react";

import { useEngineStore } from "@/lib/store";
import type { GameWindow } from "@/lib/types";
import { cn } from "@/lib/utils";

export interface WindowPickerDialogProps {
  open: boolean;
  onClose: () => void;
  onSelect: (window: GameWindow) => void;
  gameName?: string;
  windowKeyword?: string;
  /** Prefer sorting windows whose process matches this hint (e.g. HTGame.exe). */
  processNameHint?: string;
  /** Header title; default "选择窗口" */
  title?: string;
}

export function WindowPickerDialog({
  open,
  onClose,
  onSelect,
  gameName,
  windowKeyword,
  processNameHint,
  title = "选择窗口",
}: WindowPickerDialogProps) {
  const [windows, setWindows] = useState<GameWindow[]>([]);
  const [loading, setLoading] = useState(false);
  const [search, setSearch] = useState("");
  const listWindows = useEngineStore((s) => s.listWindows);

  const fetchWindows = useCallback(async () => {
    setLoading(true);
    const result = await listWindows();
    setWindows(result);
    setLoading(false);
  }, [listWindows]);

  useEffect(() => {
    if (open) {
      setSearch("");
      fetchWindows();
    }
  }, [open, fetchWindows]);

  if (!open) return null;

  const keyword = windowKeyword?.toLowerCase() ?? "";

  const normalizeExeBase = (name: string) =>
    name.replace(/\.exe$/i, "").trim().toLowerCase();

  const hintProc = processNameHint ? normalizeExeBase(processNameHint) : "";

  const filtered = windows.filter(
    (w) =>
      w.title.toLowerCase().includes(search.toLowerCase()) ||
      w.process_name.toLowerCase().includes(search.toLowerCase())
  );

  const sorted = keyword || hintProc
    ? [...filtered].sort((a, b) => {
        const score = (w: GameWindow) => {
          let s = 0;
          if (keyword && w.title.toLowerCase().includes(keyword)) s -= 2;
          if (hintProc && normalizeExeBase(w.process_name) === hintProc) s -= 3;
          return s;
        };
        return score(a) - score(b);
      })
    : filtered;

  return (
    <>
      <div
        className="fixed inset-0 bg-black/50 z-50 animate-in fade-in duration-200"
        onClick={onClose}
      />
      <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
        <div className="bg-card border border-border-subtle rounded-xl shadow-2xl w-full max-w-lg max-h-[70vh] flex flex-col animate-in zoom-in-95 duration-200">
          <div className="flex items-center justify-between px-4 py-3 border-b border-border-subtle">
            <h3 className="text-sm font-semibold text-foreground">{title}</h3>
            <button
              type="button"
              onClick={onClose}
              className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary"
            >
              <X className="w-4 h-4" />
            </button>
          </div>

          {(gameName || windowKeyword || processNameHint) && (
            <div className="px-4 py-2 border-b border-border-subtle bg-surface/30">
              <div className="flex items-center gap-4 text-xs flex-wrap">
                {gameName && (
                  <span className="text-foreground-tertiary">
                    游戏: <span className="text-foreground-secondary font-medium">{gameName}</span>
                  </span>
                )}
                {windowKeyword && (
                  <span className="text-foreground-tertiary">
                    窗口关键字:{" "}
                    <span className="text-primary font-medium">{windowKeyword}</span>
                  </span>
                )}
                {processNameHint && (
                  <span className="text-foreground-tertiary">
                    进程:{" "}
                    <span className="text-primary font-medium">{processNameHint}</span>
                  </span>
                )}
              </div>
            </div>
          )}

          <div className="px-4 py-2 border-b border-border-subtle">
            <div className="flex items-center gap-2 bg-surface border border-border rounded-md px-3 py-1.5">
              <Search className="w-4 h-4 text-foreground-tertiary" />
              <input
                type="text"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="搜索窗口标题或进程名..."
                className="flex-1 bg-transparent text-sm text-foreground outline-none placeholder:text-foreground-tertiary"
              />
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-2">
            {loading ? (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="w-5 h-5 animate-spin text-primary" />
              </div>
            ) : sorted.length === 0 ? (
              <div className="text-center py-8 text-sm text-foreground-tertiary">
                {search ? "没有匹配的窗口" : "没有找到窗口"}
              </div>
            ) : (
              <div className="space-y-1">
                {sorted.map((w) => {
                  const isMatch = keyword && w.title.toLowerCase().includes(keyword);
                  return (
                    <button
                      type="button"
                      key={w.hwnd}
                      onClick={() => {
                        onSelect(w);
                        onClose();
                      }}
                      className={cn(
                        "w-full text-left px-3 py-2 rounded-md transition-colors group",
                        isMatch
                          ? "bg-primary/10 border border-primary/30 hover:bg-primary/15"
                          : "hover:bg-surface-hover"
                      )}
                    >
                      <div className="flex items-center gap-2">
                        <span
                          className={cn(
                            "text-sm font-medium truncate",
                            isMatch
                              ? "text-primary"
                              : "text-foreground group-hover:text-primary"
                          )}
                        >
                          {w.title || "(无标题)"}
                        </span>
                        {isMatch && (
                          <span className="shrink-0 text-[10px] px-1.5 py-0.5 rounded bg-primary/20 text-primary font-medium">
                            匹配
                          </span>
                        )}
                      </div>
                      <div className="text-xs text-foreground-tertiary mt-0.5">
                        {w.process_name} · PID {w.pid} · {w.class_name}
                      </div>
                    </button>
                  );
                })}
              </div>
            )}
          </div>

          <div className="px-4 py-2 border-t border-border-subtle text-xs text-foreground-tertiary">
            共 {sorted.length} 个窗口
            {keyword && (
              <span className="ml-2">
                · 匹配关键字: <span className="text-primary">{windowKeyword}</span>
              </span>
            )}
          </div>
        </div>
      </div>
    </>
  );
}
