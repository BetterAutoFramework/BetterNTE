import { invoke } from "@tauri-apps/api/core";
import { Copy, Crosshair, Keyboard, Loader2, Mouse, RefreshCw } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { useEngineStore } from "@/lib/store";
import type { GameWindow } from "@/lib/types";

interface LogLine {
  ts: string;
  text: string;
  type: "info" | "success" | "error";
}

function nowTime(): string {
  return new Date().toLocaleTimeString("zh-CN", { hour12: false });
}

function Button({
  label,
  onClick,
  disabled,
  variant = "default",
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  variant?: "default" | "primary" | "danger";
}) {
  const base =
    "px-3 py-1.5 rounded-md text-xs font-medium border transition-colors disabled:opacity-50 disabled:cursor-not-allowed";
  const cls =
    variant === "primary"
      ? "bg-primary text-primary-foreground border-primary hover:bg-primary-hover"
      : variant === "danger"
        ? "bg-destructive text-destructive-foreground border-destructive hover:bg-destructive/90"
        : "bg-surface border-border text-foreground hover:bg-surface-hover";
  return (
    <button className={`${base} ${cls}`} onClick={onClick} disabled={disabled}>
      {label}
    </button>
  );
}

export function InputTestPage() {
  const initialized = useEngineStore((s) => s.initialized);
  const loading = useEngineStore((s) => s.loading);
  const initEngine = useEngineStore((s) => s.initEngine);

  const [busy, setBusy] = useState(false);
  const [logs, setLogs] = useState<LogLine[]>([]);

  const [windows, setWindows] = useState<GameWindow[]>([]);
  const [selectedHwnd, setSelectedHwnd] = useState<number | null>(null);

  const [keyName, setKeyName] = useState("A");
  const [keyTapMs, setKeyTapMs] = useState(60);

  const [mouseX, setMouseX] = useState(600);
  const [mouseY, setMouseY] = useState(400);
  const [mouseButton, setMouseButton] = useState("left");
  const [scrollDelta, setScrollDelta] = useState(1);

  const [startX, setStartX] = useState(500);
  const [startY, setStartY] = useState(400);
  const [endX, setEndX] = useState(900);
  const [endY, setEndY] = useState(500);
  const [holdMs, setHoldMs] = useState(220);
  const [quickJsCode, setQuickJsCode] = useState(
    `await ctx.mouseMove(900, 500);
await ctx.click(900, 500);`
  );

  useEffect(() => {
    if (!initialized && !loading) {
      initEngine();
    }
  }, [initialized, loading, initEngine]);

  const selectedWindow = useMemo(
    () => windows.find((w) => w.hwnd === selectedHwnd) ?? null,
    [windows, selectedHwnd]
  );

  const appendLog = (type: LogLine["type"], text: string) => {
    setLogs((prev) => [...prev, { ts: nowTime(), text, type }]);
  };

  const runAction = async (fn: () => Promise<string>) => {
    setBusy(true);
    try {
      const msg = await fn();
      appendLog("success", msg);
    } catch (e) {
      appendLog("error", String(e));
    } finally {
      setBusy(false);
    }
  };

  const loadWindows = async () => {
    await runAction(async () => {
      const list = await invoke<GameWindow[]>("input_list_windows");
      setWindows(list);
      if (list.length > 0 && selectedHwnd === null) {
        setSelectedHwnd(list[0].hwnd);
      }
      return `已刷新窗口列表，共 ${list.length} 个`;
    });
  };

  const bindSelectedWindow = async () => {
    if (!selectedHwnd) {
      appendLog("error", "请先选择一个窗口");
      return;
    }
    await runAction(() => invoke<string>("input_bind_window", { hwnd: selectedHwnd }));
  };

  const invokeHwnd = selectedHwnd ?? undefined;

  const buildAltMoveClickScriptSnippet = () => {
    return `// Alt + 鼠标移动并点击终点
await ctx.keyDown("Alt");
try {
  await ctx.mouseMove(${startX}, ${startY});
  await ctx.sleep(80);
  await ctx.mouseMove(${endX}, ${endY});
  await ctx.sleep(${Math.max(0, holdMs)});
  await ctx.click(${endX}, ${endY});
} finally {
  await ctx.keyUp("Alt");
}`;
  };

  const copyAltDemoSnippet = async () => {
    try {
      await navigator.clipboard.writeText(buildAltMoveClickScriptSnippet());
      appendLog("success", "已复制脚本 API 调用代码");
    } catch (e) {
      appendLog("error", `复制失败: ${String(e)}`);
    }
  };

  const buildMoveLeftClickSnippet = () => {
    return `// 移动鼠标然后点击左键
await ctx.mouseMove(${endX}, ${endY});
await ctx.click(${endX}, ${endY});`;
  };

  const copyMoveLeftClickSnippet = async () => {
    try {
      await navigator.clipboard.writeText(buildMoveLeftClickSnippet());
      appendLog("success", "已复制示例 1 的脚本 API 代码");
    } catch (e) {
      appendLog("error", `复制失败: ${String(e)}`);
    }
  };

  const buildMiddleHoldMoveClickSnippet = () => {
    return `// 按住鼠标中间键，移动，然后点击
await ctx.mouseMove(${startX}, ${startY});
await ctx.mouseDown("middle");
try {
  await ctx.sleep(80);
  await ctx.mouseMove(${endX}, ${endY});
  await ctx.sleep(${Math.max(0, holdMs)});
} finally {
  await ctx.mouseUp("middle");
}
await ctx.click(${endX}, ${endY});`;
  };

  const copyMiddleHoldMoveClickSnippet = async () => {
    try {
      await navigator.clipboard.writeText(buildMiddleHoldMoveClickSnippet());
      appendLog("success", "已复制示例 2 的脚本 API 代码");
    } catch (e) {
      appendLog("error", `复制失败: ${String(e)}`);
    }
  };

  const insertSnippet = (snippet: string) => {
    setQuickJsCode((prev) => {
      const base = prev.trim();
      if (!base) {
        return snippet;
      }
      return `${base}\n\n${snippet}`;
    });
  };

  const runQuickJsCode = async () => {
    const code = quickJsCode.trim();
    if (!code) {
      appendLog("error", "请先输入要执行的 JS 代码");
      return;
    }
    await runAction(async () => {
      const result = await invoke<unknown>("input_run_js_snippet", { code, hwnd: invokeHwnd });
      const text =
        result === null || result === undefined
          ? "null"
          : typeof result === "string"
            ? result
            : JSON.stringify(result);
      return `JS 片段执行完成，返回值: ${text}`;
    });
  };

  return (
    <div className="p-6 max-w-6xl">
      <div className="mb-4">
        <h1 className="text-xl font-semibold text-foreground">输入模块测试</h1>
        <p className="text-sm text-foreground-secondary mt-1">
          用于验证键盘按键、鼠标按键、滚轮、移动、点击，并支持输入 JS 片段快速测试。
        </p>
      </div>

      <div className="grid grid-cols-1 xl:grid-cols-[1fr_320px] gap-4">
        <div className="space-y-4">
          <section className="rounded-lg border border-border-subtle bg-card p-4 space-y-3">
            <div className="flex items-center gap-2 text-sm font-medium">
              <Crosshair className="w-4 h-4" />
              目标窗口
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Button label="刷新窗口列表" onClick={loadWindows} disabled={busy} />
              <Button label="绑定选中窗口" onClick={bindSelectedWindow} disabled={busy} variant="primary" />
            </div>
            <select
              value={selectedHwnd ?? ""}
              onChange={(e) => setSelectedHwnd(Number(e.target.value))}
              className="w-full bg-surface border border-border rounded-md px-3 py-2 text-sm text-foreground"
            >
              <option value="">请选择窗口</option>
              {windows.map((w) => (
                <option key={w.hwnd} value={w.hwnd}>
                  [{w.hwnd}] {w.title} ({w.process_name})
                </option>
              ))}
            </select>
            {selectedWindow && (
              <div className="text-xs text-foreground-secondary">
                当前目标：{selectedWindow.title} / {selectedWindow.process_name}
              </div>
            )}
          </section>

          <section className="rounded-lg border border-border-subtle bg-card p-4 space-y-3">
            <div className="flex items-center gap-2 text-sm font-medium">
              <Keyboard className="w-4 h-4" />
              键盘测试
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <input
                value={keyName}
                onChange={(e) => setKeyName(e.target.value)}
                placeholder="例如 A / Alt / F1"
                className="bg-surface border border-border rounded-md px-3 py-1.5 text-sm w-52"
              />
              <input
                type="number"
                value={keyTapMs}
                min={0}
                onChange={(e) => setKeyTapMs(Number(e.target.value))}
                className="bg-surface border border-border rounded-md px-3 py-1.5 text-sm w-24"
              />
              <span className="text-xs text-foreground-secondary">Tap 时长(ms)</span>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button
                label="按下 KeyDown"
                onClick={() =>
                  runAction(() =>
                    invoke<string>("input_key_down", { key: keyName, hwnd: invokeHwnd })
                  )
                }
                disabled={busy}
              />
              <Button
                label="松开 KeyUp"
                onClick={() =>
                  runAction(() =>
                    invoke<string>("input_key_up", { key: keyName, hwnd: invokeHwnd })
                  )
                }
                disabled={busy}
              />
              <Button
                label="点按 KeyTap"
                onClick={() =>
                  runAction(() =>
                    invoke<string>("input_key_tap", {
                      key: keyName,
                      durationMs: Math.max(0, keyTapMs),
                      hwnd: invokeHwnd,
                    })
                  )
                }
                disabled={busy}
                variant="primary"
              />
            </div>
          </section>

          <section className="rounded-lg border border-border-subtle bg-card p-4 space-y-3">
            <div className="flex items-center gap-2 text-sm font-medium">
              <Mouse className="w-4 h-4" />
              鼠标测试
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-xs text-foreground-secondary">X</span>
              <input
                type="number"
                value={mouseX}
                onChange={(e) => setMouseX(Number(e.target.value))}
                className="bg-surface border border-border rounded-md px-2 py-1 text-sm w-24"
              />
              <span className="text-xs text-foreground-secondary">Y</span>
              <input
                type="number"
                value={mouseY}
                onChange={(e) => setMouseY(Number(e.target.value))}
                className="bg-surface border border-border rounded-md px-2 py-1 text-sm w-24"
              />
              <select
                value={mouseButton}
                onChange={(e) => setMouseButton(e.target.value)}
                className="bg-surface border border-border rounded-md px-3 py-1.5 text-sm"
              >
                <option value="left">left</option>
                <option value="right">right</option>
                <option value="middle">middle</option>
                <option value="x1">x1</option>
                <option value="x2">x2</option>
              </select>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button
                label="移动鼠标"
                onClick={() =>
                  runAction(() =>
                    invoke<string>("input_mouse_move", { x: mouseX, y: mouseY, hwnd: invokeHwnd })
                  )
                }
                disabled={busy}
              />
              <Button
                label="按下鼠标键"
                onClick={() =>
                  runAction(() =>
                    invoke<string>("input_mouse_button", {
                      button: mouseButton,
                      pressed: true,
                      hwnd: invokeHwnd,
                    })
                  )
                }
                disabled={busy}
              />
              <Button
                label="松开鼠标键"
                onClick={() =>
                  runAction(() =>
                    invoke<string>("input_mouse_button", {
                      button: mouseButton,
                      pressed: false,
                      hwnd: invokeHwnd,
                    })
                  )
                }
                disabled={busy}
              />
              <Button
                label="点击"
                onClick={() =>
                  runAction(() =>
                    invoke<string>("input_mouse_click", {
                      x: mouseX,
                      y: mouseY,
                      button: mouseButton,
                      doubleClick: false,
                      hwnd: invokeHwnd,
                    })
                  )
                }
                disabled={busy}
                variant="primary"
              />
              <Button
                label="双击"
                onClick={() =>
                  runAction(() =>
                    invoke<string>("input_mouse_click", {
                      x: mouseX,
                      y: mouseY,
                      button: mouseButton,
                      doubleClick: true,
                      hwnd: invokeHwnd,
                    })
                  )
                }
                disabled={busy}
              />
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <input
                type="number"
                value={scrollDelta}
                onChange={(e) => setScrollDelta(Number(e.target.value))}
                className="bg-surface border border-border rounded-md px-3 py-1.5 text-sm w-28"
              />
              <Button
                label="滚轮滚动"
                onClick={() =>
                  runAction(() =>
                    invoke<string>("input_mouse_scroll", {
                      delta: scrollDelta,
                      hwnd: invokeHwnd,
                    })
                  )
                }
                disabled={busy}
              />
            </div>
          </section>

          <section className="rounded-lg border border-border-subtle bg-card p-4 space-y-3">
            <div className="flex items-center gap-2 text-sm font-medium">
              <RefreshCw className="w-4 h-4" />
              组合示例
            </div>
            <div className="flex flex-wrap items-center gap-2 text-xs text-foreground-secondary">
              起点
              <input
                type="number"
                value={startX}
                onChange={(e) => setStartX(Number(e.target.value))}
                className="bg-surface border border-border rounded-md px-2 py-1 text-sm w-24 text-foreground"
              />
              <input
                type="number"
                value={startY}
                onChange={(e) => setStartY(Number(e.target.value))}
                className="bg-surface border border-border rounded-md px-2 py-1 text-sm w-24 text-foreground"
              />
              终点
              <input
                type="number"
                value={endX}
                onChange={(e) => setEndX(Number(e.target.value))}
                className="bg-surface border border-border rounded-md px-2 py-1 text-sm w-24 text-foreground"
              />
              <input
                type="number"
                value={endY}
                onChange={(e) => setEndY(Number(e.target.value))}
                className="bg-surface border border-border rounded-md px-2 py-1 text-sm w-24 text-foreground"
              />
              保持
              <input
                type="number"
                value={holdMs}
                onChange={(e) => setHoldMs(Number(e.target.value))}
                className="bg-surface border border-border rounded-md px-2 py-1 text-sm w-24 text-foreground"
              />
              ms
            </div>
            <div>
              <div className="flex flex-wrap items-center gap-2 mb-2">
                <Button
                  label="示例 1：移动鼠标后左键点击"
                  onClick={() =>
                    runAction(() =>
                      invoke<string>("input_demo_move_left_click", {
                        x: endX,
                        y: endY,
                        hwnd: invokeHwnd,
                      })
                    )
                  }
                  disabled={busy}
                  variant="primary"
                />
                <button
                  onClick={copyMoveLeftClickSnippet}
                  className="px-3 py-1.5 rounded-md text-xs font-medium border bg-surface border-border text-foreground hover:bg-surface-hover inline-flex items-center gap-1.5"
                  title="复制示例 1 的脚本 API 调用代码"
                >
                  <Copy className="w-3.5 h-3.5" />
                  复制示例 1 代码
                </button>
              </div>

              <div className="flex flex-wrap items-center gap-2 mb-2">
                <Button
                  label="示例 2：按住中键移动后点击"
                  onClick={() =>
                    runAction(() =>
                      invoke<string>("input_demo_middle_hold_move_click", {
                        startX,
                        startY,
                        endX,
                        endY,
                        holdMs: Math.max(0, holdMs),
                        hwnd: invokeHwnd,
                      })
                    )
                  }
                  disabled={busy}
                  variant="primary"
                />
                <button
                  onClick={copyMiddleHoldMoveClickSnippet}
                  className="px-3 py-1.5 rounded-md text-xs font-medium border bg-surface border-border text-foreground hover:bg-surface-hover inline-flex items-center gap-1.5"
                  title="复制示例 2 的脚本 API 调用代码"
                >
                  <Copy className="w-3.5 h-3.5" />
                  复制示例 2 代码
                </button>
              </div>

              <div className="flex flex-wrap items-center gap-2">
                <Button
                  label="示例 3：按住 Alt 移动后点击"
                  onClick={() =>
                    runAction(() =>
                      invoke<string>("input_demo_alt_move", {
                        startX,
                        startY,
                        endX,
                        endY,
                        holdMs: Math.max(0, holdMs),
                        hwnd: invokeHwnd,
                      })
                    )
                  }
                  disabled={busy}
                  variant="primary"
                />
                <button
                  onClick={copyAltDemoSnippet}
                  className="px-3 py-1.5 rounded-md text-xs font-medium border bg-surface border-border text-foreground hover:bg-surface-hover inline-flex items-center gap-1.5"
                  title="复制示例 3 的脚本 API 调用代码"
                >
                  <Copy className="w-3.5 h-3.5" />
                  复制示例 3 代码
                </button>
              </div>
            </div>
          </section>

          <section className="rounded-lg border border-border-subtle bg-card p-4 space-y-3">
            <div className="flex items-center gap-2 text-sm font-medium">
              <Keyboard className="w-4 h-4" />
              快速 JS 测试
            </div>
            <p className="text-xs text-foreground-secondary">
              直接输入脚本片段并执行。支持使用 <code>ctx</code>（如 <code>ctx.click</code>、<code>ctx.keyDown</code>）。
            </p>
            <div className="flex flex-wrap items-center gap-2">
              <Button
                label="插入示例 1 代码"
                onClick={() => insertSnippet(buildMoveLeftClickSnippet())}
                disabled={busy}
              />
              <Button
                label="插入示例 2 代码"
                onClick={() => insertSnippet(buildMiddleHoldMoveClickSnippet())}
                disabled={busy}
              />
              <Button
                label="插入示例 3 代码"
                onClick={() => insertSnippet(buildAltMoveClickScriptSnippet())}
                disabled={busy}
              />
              <Button
                label="清空代码"
                onClick={() => setQuickJsCode("")}
                disabled={busy}
                variant="danger"
              />
            </div>
            <textarea
              value={quickJsCode}
              onChange={(e) => setQuickJsCode(e.target.value)}
              className="w-full min-h-44 bg-surface border border-border rounded-md px-3 py-2 text-sm font-mono text-foreground"
              placeholder={`await ctx.keyDown("Alt");
try {
  await ctx.mouseMove(500, 400);
  await ctx.sleep(120);
  await ctx.mouseMove(900, 500);
  await ctx.click(900, 500);
} finally {
  await ctx.keyUp("Alt");
}`}
            />
            <div className="flex flex-wrap items-center gap-2">
              <Button label="执行 JS 片段" onClick={runQuickJsCode} disabled={busy} variant="primary" />
            </div>
          </section>
        </div>

        <aside className="rounded-lg border border-border-subtle bg-card p-4 h-[calc(100vh-9rem)] flex flex-col min-h-0">
          <div className="flex items-center justify-between mb-2">
            <span className="text-sm font-medium">执行日志</span>
            <button
              onClick={() => setLogs([])}
              className="text-xs text-foreground-tertiary hover:text-foreground"
            >
              清空
            </button>
          </div>
          {busy && (
            <div className="mb-2 flex items-center gap-2 text-xs text-primary">
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
              执行中...
            </div>
          )}
          <div className="flex-1 overflow-y-auto font-mono text-xs space-y-1">
            {logs.length === 0 && (
              <div className="text-foreground-tertiary">还没有日志，先执行一个输入动作。</div>
            )}
            {logs.map((log, idx) => (
              <div
                key={`${log.ts}-${idx}`}
                className={
                  log.type === "error"
                    ? "text-destructive"
                    : log.type === "success"
                      ? "text-success"
                      : "text-foreground-secondary"
                }
              >
                <span className="text-foreground-tertiary mr-2">{log.ts}</span>
                {log.text}
              </div>
            ))}
          </div>
        </aside>
      </div>
    </div>
  );
}
