import {
  Camera,
  Loader2,
  Monitor,
  Play,
  Square,
  X,
  XCircle,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { CardExpander } from "@/components/ui/CardExpander";
import { SettingRow } from "@/components/ui/SettingRow";
import { WindowPickerDialog } from "@/components/WindowPickerDialog";
import { CAPTURE_METHOD_LABELS } from "@/lib/constants/capture";
import { useEngineStore } from "@/lib/store";
import type { GameWindow } from "@/lib/types";

// ============================================================================
// Banner — hero section with gradient fade
// ============================================================================

function Banner() {
  return (
    <div className="relative h-48 rounded-lg overflow-hidden bg-gradient-to-br from-primary/30 via-primary/10 to-transparent">
      {/* Diagonal gradient overlay */}
      <div className="absolute inset-0 bg-gradient-to-tl from-background/80 via-background/20 to-transparent" />

      {/* Content */}
      <div className="absolute bottom-6 left-12">
        <h1 className="text-2xl font-bold text-foreground">BetterNTE - 更好的异环</h1>
        <div className="mt-2 flex items-center gap-4 text-xs text-foreground-secondary">
          <a
            href="https://github.com/BetterAutoFramework/BetterNTE"
            target="_blank"
            rel="noreferrer"
            className="hover:text-primary transition-colors"
          >
            GitHub：BetterAutoFramework/BetterNTE
          </a>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// ScreenshotDialog — shows captured screenshot in a modal
// ============================================================================

function ScreenshotDialog({
  open,
  onClose,
  dataUrl,
  onConfirm,
  captureMethods,
  currentMethod,
  onMethodChange,
  onRetest,
  retesting,
}: {
  open: boolean;
  onClose: () => void;
  dataUrl: string | null;
  onConfirm: () => void;
  captureMethods: { value: string; available: boolean }[];
  currentMethod: string;
  onMethodChange: (method: string) => void;
  onRetest: () => void;
  retesting: boolean;
}) {
  const [mode, setMode] = useState<"confirm" | "adjust">("confirm");

  // Reset to confirm mode when dialog opens with new data
  useEffect(() => {
    if (open) setMode("confirm");
  }, [open]);

  if (!open) return null;

  return (
    <>
      <div
        className="fixed inset-0 bg-black/60 z-50 animate-in fade-in duration-200"
        onClick={onClose}
      />
      <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
        <div className="bg-card border border-border-subtle rounded-xl shadow-2xl max-w-[90vw] max-h-[90vh] flex flex-col animate-in zoom-in-95 duration-200">
          {/* Header */}
          <div className="flex items-center justify-between px-4 py-3 border-b border-border-subtle">
            <h3 className="text-sm font-semibold text-foreground">
              {mode === "confirm" ? "截图确认" : "调整截图模式"}
            </h3>
            <button
              onClick={onClose}
              className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary"
            >
              <X className="w-4 h-4" />
            </button>
          </div>

          {/* Image */}
          <div className="p-4 overflow-auto">
            {dataUrl ? (
              <img
                src={dataUrl}
                alt="Screenshot"
                className="max-w-full rounded-md border border-border-subtle"
              />
            ) : (
              <div className="text-sm text-destructive">截图失败</div>
            )}
          </div>

          {/* Adjust mode: capture method selector */}
          {mode === "adjust" && (
            <div className="px-4 pb-3 flex items-center gap-3">
              <span className="text-sm text-foreground-secondary whitespace-nowrap">截图模式</span>
              <select
                value={currentMethod}
                onChange={(e) => onMethodChange(e.target.value)}
                className="flex-1 bg-surface border border-border rounded-md px-3 py-1.5 text-sm text-foreground outline-none focus:border-primary"
              >
                {captureMethods.map((m) => (
                  <option key={m.value} value={m.value} disabled={!m.available}>
                    {CAPTURE_METHOD_LABELS[m.value] ?? m.value}
                    {!m.available ? " (不可用)" : ""}
                  </option>
                ))}
              </select>
            </div>
          )}

          {/* Footer actions */}
          <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border-subtle">
            {mode === "confirm" ? (
              <>
                <button
                  onClick={() => setMode("adjust")}
                  className="px-4 py-2 rounded-md bg-surface border border-border text-foreground text-sm font-medium hover:bg-surface-hover transition-colors"
                >
                  截图不正确
                </button>
                <button
                  onClick={onConfirm}
                  className="px-4 py-2 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary-hover transition-colors"
                >
                  截图正确，启动引擎
                </button>
              </>
            ) : (
              <>
                <button
                  onClick={onRetest}
                  disabled={retesting}
                  className="flex items-center gap-2 px-4 py-2 rounded-md bg-surface border border-border text-foreground text-sm font-medium hover:bg-surface-hover transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  {retesting ? (
                    <Loader2 className="w-4 h-4 animate-spin" />
                  ) : (
                    <Camera className="w-4 h-4" />
                  )}
                  {retesting ? "截图中..." : "重新截图"}
                </button>
                <button
                  onClick={onConfirm}
                  className="px-4 py-2 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary-hover transition-colors"
                >
                  使用当前截图启动
                </button>
              </>
            )}
          </div>
        </div>
      </div>
    </>
  );
}

// ============================================================================
// TwoStateButton — start/stop toggle button
// ============================================================================

function TwoStateButton({
  running,
  disabled,
  onStart,
  onStop,
}: {
  running: boolean;
  disabled?: boolean;
  onStart: () => void;
  onStop: () => void;
}) {
  if (running) {
    return (
      <button
        onClick={onStop}
        disabled={disabled}
        className="flex items-center gap-2 px-4 py-2 rounded-md bg-destructive text-destructive-foreground text-sm font-medium hover:bg-destructive/90 transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
      >
        <Square className="w-4 h-4" />
        停止
      </button>
    );
  }

  return (
    <button
      onClick={onStart}
      disabled={disabled}
      className="flex items-center gap-2 px-4 py-2 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary-hover transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
    >
      <Play className="w-4 h-4" />
      启动
    </button>
  );
}

// ============================================================================
// EngineLaunchCard — main launch card with expandable settings
// ============================================================================

function EngineLaunchCard() {
  const status = useEngineStore((s) => s.status);
  const config = useEngineStore((s) => s.config);
  const startEngine = useEngineStore((s) => s.startEngine);
  const stopEngine = useEngineStore((s) => s.stopEngine);
  const testScreenshot = useEngineStore((s) => s.testScreenshot);
  const findGameWindow = useEngineStore((s) => s.findGameWindow);
  const saveConfig = useEngineStore((s) => s.saveConfig);
  const showError = useEngineStore((s) => s.showError);
  const captureMethods = useEngineStore((s) => s.captureMethods);
  const running = status.state === "running";

  const [pickerOpen, setPickerOpen] = useState(false);
  const [boundWindow, setBoundWindow] = useState<GameWindow | null>(null);
  const [screenshotOpen, setScreenshotOpen] = useState(false);
  const [screenshotUrl, setScreenshotUrl] = useState<string | null>(null);
  const [screenshotLoading, setScreenshotLoading] = useState(false);
  const [screenshotRetesting, setScreenshotRetesting] = useState(false);
  const [starting, setStarting] = useState(false);

  const handleCaptureMethodChange = (value: string) => {
    if (!config || running) return;
    const updated = { ...config, capture: { ...config.capture, method: value } };
    saveConfig(updated);
  };

  const handleFpsChange = (value: string) => {
    if (!config || running) return;
    const fps = Math.max(1, Math.min(144, Number(value) || 30));
    const updated = { ...config, capture: { ...config.capture, fps_cap: fps } };
    saveConfig(updated);
  };

  const handleStart = async () => {
    if (starting) return;
    setStarting(true);
    try {
      const window = await findGameWindow();
      if (!window) {
        showError(
          "未找到游戏窗口",
          "请确认游戏已启动；并在设置中填写「窗口标题关键字」（精确匹配窗口标题）。"
        );
        return;
      }
      setBoundWindow(window);
      // Auto test screenshot before starting
      setScreenshotLoading(true);
      const url = await testScreenshot();
      setScreenshotLoading(false);
      if (url) {
        setScreenshotUrl(url);
        setScreenshotOpen(true);
      } else {
        // Screenshot failed, start engine directly (fallback)
        await startEngine();
      }
    } finally {
      setStarting(false);
    }
  };

  const handleConfirmScreenshot = useCallback(async () => {
    setScreenshotOpen(false);
    await startEngine();
  }, [startEngine]);

  const handleRetestScreenshot = useCallback(async (method?: string) => {
    if (method && config) {
      const updated = { ...config, capture: { ...config.capture, method } };
      await saveConfig(updated);
    }
    setScreenshotRetesting(true);
    const url = await testScreenshot();
    if (url) setScreenshotUrl(url);
    setScreenshotRetesting(false);
  }, [config, saveConfig, testScreenshot]);

  const handleManualSelect = async (w: GameWindow) => {
    if (starting) return;
    setBoundWindow(w);
    // Always update config with selected window info
    if (config) {
      const updated = {
        ...config,
        game: {
          ...config.game,
          window_title_keyword: w.title,
          process_name: w.process_name,
          game_name: w.process_name.replace(/\.exe$/i, ""),
        },
      };
      await saveConfig(updated);
    }
    if (!running) {
      setStarting(true);
      try {
        await startEngine();
      } finally {
        setStarting(false);
      }
    }
  };

  const handleMethodChangeInDialog = useCallback((method: string) => {
    if (config) {
      const updated = { ...config, capture: { ...config.capture, method } };
      saveConfig(updated);
    }
  }, [config, saveConfig]);

  return (
    <>
      <WindowPickerDialog
        open={pickerOpen}
        onClose={() => setPickerOpen(false)}
        onSelect={handleManualSelect}
        gameName={config?.game.game_name}
        windowKeyword={config?.game.window_title_keyword}
        processNameHint={config?.game.process_name}
      />
      <ScreenshotDialog
        open={screenshotOpen}
        onClose={() => setScreenshotOpen(false)}
        dataUrl={screenshotUrl}
        onConfirm={handleConfirmScreenshot}
        captureMethods={captureMethods}
        currentMethod={config?.capture.method ?? "auto"}
        onMethodChange={handleMethodChangeInDialog}
        onRetest={() => handleRetestScreenshot()}
        retesting={screenshotRetesting}
      />

      <CardExpander
        icon={<Play className="w-4 h-4" />}
        title="BetterNTE 引擎，启动！"
        description={running ? `正在运行: ${status.task ?? "就绪"}` : "展开查看引擎配置"}
        defaultOpen={false}
        headerRight={
          <div onClick={(e) => e.stopPropagation()}>
            <TwoStateButton
              running={running}
              disabled={starting}
              onStart={handleStart}
              onStop={() => stopEngine()}
            />
          </div>
        }
      >
        <SettingRow label="截图模式" description="推荐选择 BitBlt，问题较少">
          <select
            value={config?.capture.method ?? "auto"}
            disabled={running}
            onChange={(e) => handleCaptureMethodChange(e.target.value)}
            className="bg-surface border border-border rounded-md px-3 py-1.5 text-sm text-foreground outline-none focus:border-primary disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {captureMethods.map((m) => (
              <option key={m.value} value={m.value} disabled={!m.available}>
                {CAPTURE_METHOD_LABELS[m.value] ?? m.value}
                {!m.available ? " (不可用)" : ""}
              </option>
            ))}
          </select>
        </SettingRow>

        <SettingRow label="FPS 上限" description="截图最大帧率，默认 30">
          <input
            type="number"
            value={config?.capture.fps_cap ?? 30}
            min={1}
            max={144}
            disabled={running}
            onChange={(e) => handleFpsChange(e.target.value)}
            className="bg-surface border border-border rounded-md px-3 py-1.5 text-sm text-foreground outline-none focus:border-primary w-20 text-center font-mono disabled:opacity-50 disabled:cursor-not-allowed"
          />
        </SettingRow>

        <SettingRow
          label="手动选择窗口"
          description={
            boundWindow
              ? `已绑定: ${boundWindow.title}`
              : "游戏已经启动的情况下，手动选择目标窗口"
          }
        >
          <button
            onClick={() => setPickerOpen(true)}
            className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-surface border border-border text-foreground text-sm font-medium hover:bg-surface-hover transition-colors"
          >
            <Monitor className="w-4 h-4" />
            {boundWindow ? "更换窗口" : "选择窗口"}
          </button>
        </SettingRow>
      </CardExpander>
    </>
  );
}

// ============================================================================
// HomePage — main entry point
// ============================================================================

export function HomePage() {
  const initialized = useEngineStore((s) => s.initialized);
  const loading = useEngineStore((s) => s.loading);
  const error = useEngineStore((s) => s.error);
  const initEngine = useEngineStore((s) => s.initEngine);

  useEffect(() => {
    if (!initialized && !loading) {
      initEngine();
    }
  }, [initialized, loading, initEngine]);

  if (loading && !initialized) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="flex flex-col items-center gap-3">
          <Loader2 className="w-8 h-8 animate-spin text-primary" />
          <p className="text-sm text-foreground-secondary">正在初始化引擎...</p>
        </div>
      </div>
    );
  }

  if (error && !initialized) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="flex flex-col items-center gap-3 text-center">
          <XCircle className="w-8 h-8 text-destructive" />
          <p className="text-sm text-destructive">{error}</p>
          <button
            onClick={() => initEngine()}
            className="px-4 py-2 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary-hover"
          >
            重试
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="p-6 max-w-3xl">
      <div className="space-y-3">
        <Banner />
        <EngineLaunchCard />
      </div>
    </div>
  );
}
