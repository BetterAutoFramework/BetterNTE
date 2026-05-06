import { open } from "@tauri-apps/plugin-dialog";
import {
  BadgeCheck,
  Bell,
  Bug,
  Camera,
  Clapperboard,
  Code,
  FolderOpen,
  Gamepad2,
  Globe,
  Keyboard,
  Layers,
  Loader2,
  Plus,
  Settings2,
  Shield,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { CardExpander } from "@/components/ui/CardExpander";
import { HelpHint } from "@/components/ui/HelpHint";
import { HotkeyInput } from "@/components/ui/HotkeyInput";
import { SettingRow } from "@/components/ui/SettingRow";
import { Toggle } from "@/components/ui/Toggle";
import { CAPTURE_METHOD_LABELS } from "@/lib/constants/capture";
import { CONFIG_AUTOSAVE_DEBOUNCE_MS } from "@/lib/constants/timing";
import { upsertScriptHotkey, upsertTaskGroupHotkey } from "@/lib/hotkeyTriggers";
import { scriptKey } from "@/lib/stores/helpers";
import { mapEngineConfig, useEngineStore } from "@/lib/store";
import { invokeAction } from "@/lib/stores/helpers";
import type {
  EngineConfig,
  GamePluginInfo,
  HotkeyTriggersConfig,
  Subscription,
} from "@/lib/types";
import { cn } from "@/lib/utils";

type SettingsTab =
  | "general"
  | "capture"
  | "hotkeys"
  | "overlay"
  | "notifications"
  | "scripts"
  | "security"
  | "advanced";

const tabs: { id: SettingsTab; label: string; icon: React.ReactNode }[] = [
  { id: "general", label: "通用", icon: <Gamepad2 className="w-4 h-4" /> },
  { id: "capture", label: "截图", icon: <Camera className="w-4 h-4" /> },
  { id: "hotkeys", label: "热键", icon: <Keyboard className="w-4 h-4" /> },
  { id: "overlay", label: "叠层", icon: <Layers className="w-4 h-4" /> },
  { id: "notifications", label: "通知", icon: <Bell className="w-4 h-4" /> },
  { id: "scripts", label: "脚本", icon: <Code className="w-4 h-4" /> },
  { id: "security", label: "安全", icon: <Shield className="w-4 h-4" /> },
  {
    id: "advanced",
    label: "高级",
    icon: <Settings2 className="w-4 h-4" />,
  },
];

/** General / capture tabs hidden until re-enabled in product (unless `BETTER_NTE_DEBUG=1`). */
const HIDDEN_SETTINGS_TAB_IDS = new Set<SettingsTab>(["general", "capture"]);

function visibleTabsFor(debugEnv: boolean) {
  return tabs.filter(
    (t) => !HIDDEN_SETTINGS_TAB_IDS.has(t.id) || debugEnv
  );
}

/** localStorage key for UI developer mode; default is off when unset. */
const DEV_MODE_KEY = "betternte-developer-mode";

// ============================================================================
// Input Components (controlled)
// ============================================================================

function TextInput({
  value,
  placeholder,
  onChange,
}: {
  value: string;
  placeholder?: string;
  onChange: (v: string) => void;
}) {
  return (
    <input
      type="text"
      value={value}
      placeholder={placeholder}
      onChange={(e) => onChange(e.target.value)}
      className="bg-surface border border-border rounded-md px-3 py-1.5 text-sm text-foreground placeholder:text-foreground-tertiary outline-none focus:border-primary w-48"
    />
  );
}

function NumberInput({
  value,
  min,
  max,
  onChange,
}: {
  value: number;
  min?: number;
  max?: number;
  onChange: (v: number) => void;
}) {
  return (
    <input
      type="number"
      value={value}
      min={min}
      max={max}
      onChange={(e) => onChange(Number(e.target.value))}
      className="bg-surface border border-border rounded-md px-3 py-1.5 text-sm text-foreground outline-none focus:border-primary w-20 text-center font-mono"
    />
  );
}

function SelectInput({
  value,
  options,
  onChange,
}: {
  value: string;
  options: { label: string; value: string }[];
  onChange: (v: string) => void;
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="bg-surface border border-border rounded-md px-3 py-1.5 text-sm text-foreground outline-none focus:border-primary"
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

// ============================================================================
// Helper: update a nested field in config
// ============================================================================

function updateConfig(
  config: EngineConfig,
  section: keyof EngineConfig,
  field: string,
  value: unknown
): EngineConfig {
  return {
    ...config,
    [section]: {
      ...((config[section] as unknown as Record<string, unknown>)),
      [field]: value,
    },
  };
}

type RootConfigField = "active_plugin" | "plugin_search_paths";

function updateRootConfig(
  config: EngineConfig,
  field: RootConfigField,
  value: unknown
): EngineConfig {
  return {
    ...config,
    [field]: value,
  };
}

// ============================================================================
// File/Directory Picker Button
// ============================================================================

function FolderPicker({
  value,
  onChange,
  placeholder,
  directory,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  directory?: boolean;
}) {
  const handlePick = async () => {
    const selected = await open({
      directory: directory ?? false,
      multiple: false,
      defaultPath: value || undefined,
    });
    if (selected) {
      onChange(selected);
    }
  };

  return (
    <div className="flex items-center gap-2">
      <input
        type="text"
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
        className="bg-surface border border-border rounded-md px-3 py-1.5 text-sm text-foreground placeholder:text-foreground-tertiary outline-none focus:border-primary w-48"
      />
      <button
        onClick={handlePick}
        className="p-1.5 rounded-md bg-surface border border-border hover:bg-surface-hover text-foreground-secondary"
      >
        <FolderOpen className="w-4 h-4" />
      </button>
    </div>
  );
}

// ============================================================================
// Settings Sections
// ============================================================================

function GeneralSettings({
  config,
  onRootChange,
}: {
  config: EngineConfig;
  onRootChange: (field: RootConfigField, value: unknown) => void;
}) {
  const [plugins, setPlugins] = useState<GamePluginInfo[]>([]);
  const [pluginsLoading, setPluginsLoading] = useState(false);

  useEffect(() => {
    let alive = true;
    setPluginsLoading(true);
    void invokeAction<GamePluginInfo[]>("list_game_plugins", undefined, { silent: true })
      .then((list) => {
        if (alive && Array.isArray(list)) {
          setPlugins(list);
        }
      })
      .finally(() => {
        if (alive) setPluginsLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [config.plugin_search_paths, config.scripts.data_root]);

  const handleActivePluginChange = (nextPluginId: string) => {
    const currentPluginId = config.active_plugin || "nte";
    if (nextPluginId === currentPluginId) return;
    const ok = window.confirm(
      `切换插件将从「${nextPluginId}」的 manifest 回填并覆盖当前的游戏设置、截图配置、脚本目录与订阅源配置。\n\n是否继续？`
    );
    if (!ok) return;
    onRootChange("active_plugin", nextPluginId);
  };

  return (
    <>
      <CardExpander
        icon={<Globe className="w-4 h-4" />}
        title="插件设置"
        description="选择游戏插件与搜索路径（通用框架预留）"
        defaultOpen={true}
      >
        <SettingRow label="当前插件" description="当前正在使用的游戏插件">
          <SelectInput
            value={config.active_plugin || "nte"}
            onChange={handleActivePluginChange}
            options={[
              ...(plugins.length > 0
                ? plugins.map((p) => ({
                    label: `${p.name} (${p.id})`,
                    value: p.id,
                  }))
                : [{ label: "异环 (默认)", value: "nte" }]),
              ...(!plugins.some((p) => p.id === config.active_plugin) && config.active_plugin
                ? [{ label: `${config.active_plugin} (手动)`, value: config.active_plugin }]
                : []),
            ]}
          />
        </SettingRow>
        <SettingRow
          label="插件搜索路径"
          description="脚本搜索路径。可填写多个，用英文逗号分隔。"
        >
          <TextInput
            value={(config.plugin_search_paths ?? []).join(", ")}
            onChange={(v) =>
              onRootChange(
                "plugin_search_paths",
                v
                  .split(",")
                  .map((s) => s.trim())
                  .filter(Boolean)
              )
            }
            placeholder="例如：plugins, community/plugins"
          />
        </SettingRow>
        <div className="text-xs text-foreground-tertiary">
          {pluginsLoading
            ? "正在扫描插件..."
            : `已发现 ${plugins.length} 个插件（基于当前 data_root 与搜索路径）`}
        </div>
      </CardExpander>

      <CardExpander
        icon={<Gamepad2 className="w-4 h-4" />}
        title="游戏设置"
      description="游戏窗口与进程信息（只读）"
        defaultOpen={true}
      >
        <SettingRow label="游戏名称" description="游戏显示名称">
          <div className="text-sm text-foreground">{config.game.game_name || "-"}</div>
        </SettingRow>
        <SettingRow
          label="窗口标题关键字"
          description="窗口标题精确匹配（仅按标题匹配）"
        >
          <div className="text-sm text-foreground">{config.game.window_title_keyword || "-"}</div>
        </SettingRow>
        <SettingRow
          label="进程名称"
          description="可执行文件名（如 HTGame.exe），当前仅展示，不参与窗口匹配"
        >
          <div className="text-sm text-foreground">{config.game.process_name || "-"}</div>
        </SettingRow>
        <SettingRow label="游戏语言" description="游戏内语言设置">
          <div className="text-sm text-foreground">
            {{
              "zh-cn": "简体中文",
              "zh-tw": "繁體中文",
              en: "English",
              ja: "日本語",
              ko: "한국어",
            }[config.game.game_language] ?? config.game.game_language}
          </div>
        </SettingRow>
        <SettingRow label="游戏分辨率" description="游戏窗口分辨率">
          <div className="text-sm text-foreground">{config.game.resolution || "-"}</div>
        </SettingRow>
        <SettingRow label="缩放比" description="界面缩放比例">
          <div className="text-sm text-foreground">{`${Math.round(config.game.scale * 100)}%`}</div>
        </SettingRow>
        <SettingRow label="DPI" description="显示 DPI 值">
          <div className="text-sm text-foreground">{config.game.dpi}</div>
        </SettingRow>
      </CardExpander>
    </>
  );
}

function CaptureSettings({
  config,
  onChange,
}: {
  config: EngineConfig;
  onChange: (section: keyof EngineConfig, field: string, value: unknown) => void;
}) {
  const captureMethods = useEngineStore((s) => s.captureMethods);

  return (
    <CardExpander
      icon={<Camera className="w-4 h-4" />}
      title="截图设置"
      description="截图引擎和性能配置"
      defaultOpen={true}
    >
      <SettingRow label="截图引擎" description="自动模式会依次尝试可用方案（WGC、DXGI、ScreenDC）。">
        <SelectInput
          value={config.capture.method}
          options={captureMethods.map((m) => ({
            label: (CAPTURE_METHOD_LABELS[m.value] ?? m.value) + (!m.available ? " (不可用)" : ""),
            value: m.value,
          }))}
          onChange={(v) => onChange("capture", "method", v)}
        />
      </SettingRow>
      <SettingRow label="FPS 上限" description="截图最大帧率">
        <NumberInput
          value={config.capture.fps_cap}
          min={1}
          max={144}
          onChange={(v) => onChange("capture", "fps_cap", v)}
        />
      </SettingRow>
      <SettingRow label="捕获目标" description="窗口捕获或显示器捕获">
        <SelectInput
          value={config.capture.target_type}
          options={[
            { label: "窗口", value: "window" },
            { label: "显示器", value: "display" },
          ]}
          onChange={(v) => onChange("capture", "target_type", v)}
        />
      </SettingRow>
      <SettingRow
        label="显示器"
        description={config.capture.target_type === "display" ? "多显示器时选择捕获哪个" : "仅在「捕获目标=显示器」时生效"}
      >
        <NumberInput
          value={config.capture.display_index}
          min={0}
          max={16}
          onChange={(v) => onChange("capture", "display_index", v)}
        />
      </SettingRow>
      <SettingRow label="裁剪模式" description="仅客户端区（去标题栏/阴影）或整窗">
        <SelectInput
          value={config.capture.crop_mode}
          options={[
            { label: "客户端区", value: "client_only" },
            { label: "整窗", value: "window" },
          ]}
          onChange={(v) => onChange("capture", "crop_mode", v)}
        />
      </SettingRow>
      <SettingRow label="HDR 策略" description="自动：支持时自动处理；强制：尽量启用，不支持会自动回退。">
        <SelectInput
          value={config.capture.hdr_policy}
          options={[
            { label: "关闭", value: "off" },
            { label: "自动", value: "auto" },
            { label: "强制", value: "force" },
          ]}
          onChange={(v) => onChange("capture", "hdr_policy", v)}
        />
      </SettingRow>
      <SettingRow label="最小化行为" description="窗口最小化后暂停或继续尝试截图">
        <SelectInput
          value={config.capture.minimized_behavior}
          options={[
            { label: "暂停", value: "pause" },
            { label: "继续尝试", value: "keep_trying" },
            { label: "伪后台(预留)", value: "pseudo_background" },
          ]}
          onChange={(v) => onChange("capture", "minimized_behavior", v)}
        />
      </SettingRow>
      <SettingRow label="窗口变化自动恢复" description="窗口大小变化后自动恢复截图。">
        <Toggle
          checked={config.capture.recover_on_resize}
          onChange={(v) => onChange("capture", "recover_on_resize", v)}
        />
      </SettingRow>
      <SettingRow label="跨显示器自动恢复" description="窗口切到其他显示器时自动恢复截图。">
        <Toggle
          checked={config.capture.recover_on_monitor_switch}
          onChange={(v) => onChange("capture", "recover_on_monitor_switch", v)}
        />
      </SettingRow>
    </CardExpander>
  );
}

function HotkeySettings({
  config,
  onChange,
  onHotkeyTriggersReplace,
}: {
  config: EngineConfig;
  onChange: (section: keyof EngineConfig, field: string, value: unknown) => void;
  onHotkeyTriggersReplace: (triggers: HotkeyTriggersConfig) => void;
}) {
  const scripts = useEngineStore((s) => s.scripts);
  const taskGroups = useEngineStore((s) => s.taskGroups);
  const refreshScripts = useEngineStore((s) => s.refreshScripts);
  const listTaskGroups = useEngineStore((s) => s.listTaskGroups);

  useEffect(() => {
    refreshScripts();
    listTaskGroups();
  }, [refreshScripts, listTaskGroups]);

  const scriptLabel = (key: string) => {
    const s =
      scripts.find((x) => scriptKey(x) === key) ??
      scripts.find((x) => x.name === key);
    return s?.display_name ?? key;
  };
  const taskGroupLabel = (uuid: string) =>
    taskGroups.find((g) => g.uuid === uuid)?.name ?? uuid;

  return (
    <CardExpander
      icon={<Keyboard className="w-4 h-4" />}
      title="热键设置"
      description="全局快捷键配置"
      defaultOpen={true}
    >
      <SettingRow label="启动/停止任务">
        <HotkeyInput
          value={config.hotkeys.toggle_task}
          onChange={(v) => onChange("hotkeys", "toggle_task", v)}
        />
      </SettingRow>
      <SettingRow label="紧急停止">
        <HotkeyInput
          value={config.hotkeys.emergency_stop}
          onChange={(v) => onChange("hotkeys", "emergency_stop", v)}
        />
      </SettingRow>
      <SettingRow label="切换叠层显示">
        <HotkeyInput
          value={config.hotkeys.toggle_overlay}
          onChange={(v) => onChange("hotkeys", "toggle_overlay", v)}
        />
      </SettingRow>
      <SettingRow label="暂停/恢复">
        <HotkeyInput
          value={config.hotkeys.pause_resume}
          onChange={(v) => onChange("hotkeys", "pause_resume", v)}
        />
      </SettingRow>
      <SettingRow label="调试截图">
        <HotkeyInput
          value={config.hotkeys.debug_screenshot}
          onChange={(v) => onChange("hotkeys", "debug_screenshot", v)}
        />
      </SettingRow>

      <div className="border-t border-border-subtle pt-4 mt-4 space-y-3">
        <div>
          <div className="flex items-center gap-1.5 mb-2">
            <div className="text-sm font-medium text-foreground">脚本快捷键</div>
            <HelpHint text="在「脚本」页或此处管理；按下启动、运行中再按停止；删除即取消该脚本的快捷键。" />
          </div>
          {Object.keys(config.hotkey_triggers.scripts).length === 0 ? (
            <p className="text-xs text-foreground-tertiary/80">暂无</p>
          ) : (
            <ul className="space-y-2">
              {Object.entries(config.hotkey_triggers.scripts).map(
                ([shortcut, scriptName]) => (
                  <li
                    key={`script-${shortcut}`}
                    className="flex flex-wrap items-center gap-2 rounded-lg border border-border-subtle bg-card/50 px-3 py-2"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="text-sm text-foreground truncate">
                        {scriptLabel(scriptName)}
                      </div>
                      <div className="text-xs text-foreground-tertiary font-mono truncate">
                        {scriptName}
                      </div>
                    </div>
                    <HotkeyInput
                      value={shortcut}
                      onChange={(v) => {
                        const next = upsertScriptHotkey(config, scriptName, v);
                        onHotkeyTriggersReplace(next.hotkey_triggers);
                      }}
                    />
                    <button
                      type="button"
                      title="删除快捷键"
                      onClick={() => {
                        const scriptsMap = { ...config.hotkey_triggers.scripts };
                        delete scriptsMap[shortcut];
                        onHotkeyTriggersReplace({
                          ...config.hotkey_triggers,
                          scripts: scriptsMap,
                        });
                      }}
                      className="p-1.5 rounded-md hover:bg-destructive/10 text-foreground-tertiary hover:text-destructive"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </li>
                )
              )}
            </ul>
          )}
        </div>

        <div>
          <div className="flex items-center gap-1.5 mb-2">
            <div className="text-sm font-medium text-foreground">任务组快捷键</div>
            <HelpHint text="在「任务组」页或此处管理；按下启动、运行中再按停止；删除即取消该任务组的快捷键。" />
          </div>
          {Object.keys(config.hotkey_triggers.task_groups).length === 0 ? (
            <p className="text-xs text-foreground-tertiary/80">暂无</p>
          ) : (
            <ul className="space-y-2">
              {Object.entries(config.hotkey_triggers.task_groups).map(
                ([shortcut, uuid]) => (
                  <li
                    key={`tg-${shortcut}`}
                    className="flex flex-wrap items-center gap-2 rounded-lg border border-border-subtle bg-card/50 px-3 py-2"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="text-sm text-foreground truncate">
                        {taskGroupLabel(uuid)}
                      </div>
                      <div className="text-xs text-foreground-tertiary font-mono truncate">
                        {uuid}
                      </div>
                    </div>
                    <HotkeyInput
                      value={shortcut}
                      onChange={(v) => {
                        const next = upsertTaskGroupHotkey(config, uuid, v);
                        onHotkeyTriggersReplace(next.hotkey_triggers);
                      }}
                    />
                    <button
                      type="button"
                      title="删除快捷键"
                      onClick={() => {
                        const tg = { ...config.hotkey_triggers.task_groups };
                        delete tg[shortcut];
                        onHotkeyTriggersReplace({
                          ...config.hotkey_triggers,
                          task_groups: tg,
                        });
                      }}
                      className="p-1.5 rounded-md hover:bg-destructive/10 text-foreground-tertiary hover:text-destructive"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </li>
                )
              )}
            </ul>
          )}
        </div>
      </div>

      <div className="flex items-center gap-1.5 px-1 pt-3 mt-2 border-t border-border-subtle text-xs text-foreground-secondary">
        <span>热键提示</span>
        <HelpHint text="修改后自动保存；与「紧急停止」等固定热键勿使用同一组合键。" />
      </div>
    </CardExpander>
  );
}

function OverlaySettings({
  config,
  onChange,
}: {
  config: EngineConfig;
  onChange: (section: keyof EngineConfig, field: string, value: unknown) => void;
}) {
  return (
    <CardExpander
      icon={<Layers className="w-4 h-4" />}
      title="叠层设置"
      description="游戏内叠加层显示配置"
      defaultOpen={true}
      headerRight={
        <Toggle
          checked={config.overlay.enabled}
          onChange={(v) => onChange("overlay", "enabled", v)}
        />
      }
    >
      <SettingRow label="显示模式">
        <SelectInput
          value={config.overlay.mode}
          options={[
            { label: "隐藏", value: "hidden" },
            { label: "精简", value: "minimal" },
            { label: "详细", value: "detailed" },
          ]}
          onChange={(v) => onChange("overlay", "mode", v)}
        />
      </SettingRow>
      <SettingRow label="不透明度">
        <div className="flex items-center gap-3">
          <input
            type="range"
            min={0}
            max={100}
            value={Math.round(config.overlay.opacity * 100)}
            onChange={(e) =>
              onChange("overlay", "opacity", Number(e.target.value) / 100)
            }
            className="w-32 accent-primary"
          />
          <span className="text-sm font-mono text-foreground-secondary w-10">
            {Math.round(config.overlay.opacity * 100)}%
          </span>
        </div>
      </SettingRow>
      <SettingRow label="字体大小">
        <NumberInput
          value={config.overlay.font_size}
          min={10}
          max={24}
          onChange={(v) => onChange("overlay", "font_size", v)}
        />
      </SettingRow>
    </CardExpander>
  );
}

function NotificationSettings({
  config,
  onChange,
}: {
  config: EngineConfig;
  onChange: (section: keyof EngineConfig, field: string, value: unknown) => void;
}) {
  const notif = config.notifications;
  const showError = useEngineStore((s) => s.showError);
  const [testingChannel, setTestingChannel] = useState<
    "telegram" | "discord" | "serverchan" | "bark" | null
  >(null);

  const runChannelTest = async (uiChannel: "telegram" | "discord" | "serverchan" | "bark") => {
    setTestingChannel(uiChannel);
    try {
      await invokeAction<string>(
        "test_notification_channel",
        { uiChannel, notifications: notif as unknown as Record<string, unknown> },
        { showError: true, errorTitle: "通知测试失败" },
        showError
      );
    } catch {
      // Error dialog already shown by invokeAction
    } finally {
      setTestingChannel(null);
    }
  };

  return (
    <CardExpander
      icon={<Bell className="w-4 h-4" />}
      title="通知设置"
      description="任务完成/失败时发送通知"
      defaultOpen={true}
      headerRight={
        <Toggle
          checked={notif.enabled}
          onChange={(v) => onChange("notifications", "enabled", v)}
        />
      }
    >
      <SettingRow label="通知级别">
        <SelectInput
          value={notif.level ?? "warning"}
          options={[
            { label: "Info", value: "info" },
            { label: "Warning", value: "warning" },
            { label: "Error", value: "error" },
          ]}
          onChange={(v) => onChange("notifications", "level", v)}
        />
      </SettingRow>

      {/* Telegram */}
      <div className="pt-2 border-t border-border-subtle">
        <div className="text-xs font-medium text-foreground-secondary mb-2">Telegram</div>
        <SettingRow label="启用">
          <Toggle
            checked={notif.telegram?.enabled ?? false}
            onChange={(v) =>
              onChange("notifications", "telegram", {
                ...(notif.telegram ?? { bot_token: "", chat_id: "" }),
                enabled: v,
              })
            }
          />
        </SettingRow>
        <SettingRow label="Bot Token">
          <TextInput
            value={notif.telegram?.bot_token ?? ""}
            placeholder="123456:ABC-DEF..."
            onChange={(v) =>
              onChange("notifications", "telegram", {
                ...(notif.telegram ?? { enabled: false, chat_id: "" }),
                bot_token: v,
              })
            }
          />
        </SettingRow>
        <SettingRow label="Chat ID">
          <TextInput
            value={notif.telegram?.chat_id ?? ""}
            placeholder="群组或用户 ID"
            onChange={(v) =>
              onChange("notifications", "telegram", {
                ...(notif.telegram ?? { enabled: false, bot_token: "" }),
                chat_id: v,
              })
            }
          />
        </SettingRow>
        <div className="flex justify-end">
          <button
            type="button"
            disabled={testingChannel !== null}
            onClick={() => void runChannelTest("telegram")}
            className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-surface border border-border text-foreground text-sm font-medium hover:bg-surface-hover transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {testingChannel === "telegram" ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : null}
            测试 Telegram
          </button>
        </div>
      </div>

      {/* Discord */}
      <div className="pt-2 border-t border-border-subtle">
        <div className="text-xs font-medium text-foreground-secondary mb-2">Discord</div>
        <SettingRow label="启用">
          <Toggle
            checked={notif.discord?.enabled ?? false}
            onChange={(v) =>
              onChange("notifications", "discord", {
                ...(notif.discord ?? { webhook_url: "" }),
                enabled: v,
              })
            }
          />
        </SettingRow>
        <SettingRow label="Webhook URL">
          <TextInput
            value={notif.discord?.webhook_url ?? ""}
            placeholder="https://discord.com/api/webhooks/..."
            onChange={(v) =>
              onChange("notifications", "discord", {
                ...(notif.discord ?? { enabled: false }),
                webhook_url: v,
              })
            }
          />
        </SettingRow>
        <div className="flex justify-end">
          <button
            type="button"
            disabled={testingChannel !== null}
            onClick={() => void runChannelTest("discord")}
            className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-surface border border-border text-foreground text-sm font-medium hover:bg-surface-hover transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {testingChannel === "discord" ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : null}
            测试 Discord
          </button>
        </div>
      </div>

      {/* ServerChan */}
      <div className="pt-2 border-t border-border-subtle">
        <div className="text-xs font-medium text-foreground-secondary mb-2">Server酱</div>
        <SettingRow label="启用">
          <Toggle
            checked={notif.serverchan?.enabled ?? false}
            onChange={(v) =>
              onChange("notifications", "serverchan", {
                ...(notif.serverchan ?? { send_key: "" }),
                enabled: v,
              })
            }
          />
        </SettingRow>
        <SettingRow label="SendKey">
          <TextInput
            value={notif.serverchan?.send_key ?? ""}
            placeholder="SCT..."
            onChange={(v) =>
              onChange("notifications", "serverchan", {
                ...(notif.serverchan ?? { enabled: false }),
                send_key: v,
              })
            }
          />
        </SettingRow>
        <div className="flex justify-end">
          <button
            type="button"
            disabled={testingChannel !== null}
            onClick={() => void runChannelTest("serverchan")}
            className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-surface border border-border text-foreground text-sm font-medium hover:bg-surface-hover transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {testingChannel === "serverchan" ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : null}
            测试 Server酱
          </button>
        </div>
      </div>

      {/* Bark */}
      <div className="pt-2 border-t border-border-subtle">
        <div className="text-xs font-medium text-foreground-secondary mb-2">Bark (iOS)</div>
        <SettingRow label="启用">
          <Toggle
            checked={notif.bark?.enabled ?? false}
            onChange={(v) =>
              onChange("notifications", "bark", {
                ...(notif.bark ?? {
                  server_url: "https://api.day.app",
                  device_key: "",
                }),
                enabled: v,
              })
            }
          />
        </SettingRow>
        <SettingRow label="服务器 URL">
          <TextInput
            value={notif.bark?.server_url ?? "https://api.day.app"}
            placeholder="https://api.day.app"
            onChange={(v) =>
              onChange("notifications", "bark", {
                ...(notif.bark ?? { enabled: false, device_key: "" }),
                server_url: v,
              })
            }
          />
        </SettingRow>
        <SettingRow label="Device Key">
          <TextInput
            value={notif.bark?.device_key ?? ""}
            placeholder="设备密钥"
            onChange={(v) =>
              onChange("notifications", "bark", {
                ...(notif.bark ?? {
                  enabled: false,
                  server_url: "https://api.day.app",
                }),
                device_key: v,
              })
            }
          />
        </SettingRow>
        <div className="flex justify-end">
          <button
            type="button"
            disabled={testingChannel !== null}
            onClick={() => void runChannelTest("bark")}
            className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-surface border border-border text-foreground text-sm font-medium hover:bg-surface-hover transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {testingChannel === "bark" ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : null}
            测试 Bark
          </button>
        </div>
      </div>
    </CardExpander>
  );
}

function SubscriptionManager({
  subscriptions,
  onChange,
}: {
  subscriptions: Subscription[];
  onChange: (subs: Subscription[]) => void;
}) {
  const [newName, setNewName] = useState("");
  const [newDir, setNewDir] = useState("");

  const addSub = () => {
    if (!newName.trim() || !newDir.trim()) return;
    onChange([
      ...subscriptions,
      { name: newName.trim(), directory: newDir.trim(), enabled: true, auto_update: false },
    ]);
    setNewName("");
    setNewDir("");
  };

  const removeSub = (index: number) => {
    onChange(subscriptions.filter((_, i) => i !== index));
  };

  const toggleSub = (index: number) => {
    onChange(
      subscriptions.map((s, i) => (i === index ? { ...s, enabled: !s.enabled } : s))
    );
  };

  const toggleAutoUpdate = (index: number) => {
    onChange(
      subscriptions.map((s, i) => (i === index ? { ...s, auto_update: !s.auto_update } : s))
    );
  };

  return (
    <div className="space-y-3">
      {subscriptions.map((sub, i) => (
        <div
          key={i}
          className="flex items-center gap-2 p-2 rounded-md bg-surface/50 border border-border-subtle"
        >
          <Toggle checked={sub.enabled} onChange={() => toggleSub(i)} />
          <div className="flex-1 min-w-0">
            <div className="text-sm text-foreground truncate">
              {sub.name}
              <span className="text-xs text-foreground-tertiary ml-1.5">({sub.directory})</span>
            </div>
            <div className="flex items-center gap-2 mt-0.5">
              <label className="flex items-center gap-1 text-xs text-foreground-tertiary cursor-pointer">
                <input
                  type="checkbox"
                  checked={sub.auto_update}
                  onChange={() => toggleAutoUpdate(i)}
                  className="w-3 h-3 rounded border-border accent-primary"
                />
                自动更新
              </label>
            </div>
          </div>
          {sub.directory !== "main" && sub.directory !== "local" && (
            <button
              onClick={() => removeSub(i)}
              className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary hover:text-destructive"
            >
              <Trash2 className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
      ))}

      {/* Add new subscription */}
      <div className="flex items-center gap-2">
        <input
          type="text"
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          placeholder="名称"
          className="bg-surface border border-border rounded-md px-2 py-1 text-xs text-foreground outline-none focus:border-primary w-24"
        />
        <input
          type="text"
          value={newDir}
          onChange={(e) => setNewDir(e.target.value)}
          placeholder="目录名"
          className="flex-1 bg-surface border border-border rounded-md px-2 py-1 text-xs text-foreground outline-none focus:border-primary"
        />
        <button
          onClick={addSub}
          disabled={!newName.trim() || !newDir.trim()}
          className="p-1.5 rounded-md bg-surface border border-border hover:bg-surface-hover text-foreground-secondary disabled:opacity-50"
        >
          <Plus className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}

function ScriptSettings({
  config,
  onChange,
}: {
  config: EngineConfig;
  onChange: (section: keyof EngineConfig, field: string, value: unknown) => void;
}) {
  return (
    <CardExpander
      icon={<Code className="w-4 h-4" />}
      title="订阅管理"
      description="数据目录与订阅源设置"
      defaultOpen={true}
    >
      <SettingRow
        label="数据根目录"
        description="存放订阅与脚本数据。相对路径：开发版相对仓库根；安装版相对本机应用数据目录（首次启动会从安装包复制自带的 data、assets 到该目录）"
      >
        <FolderPicker
          value={config.scripts.data_root}
          onChange={(v) => onChange("scripts", "data_root", v)}
          placeholder="选择数据根目录..."
          directory
        />
      </SettingRow>
      <SettingRow label="自动更新" description="启动时检查订阅源更新">
        <Toggle
          checked={config.scripts.auto_update}
          onChange={(v) => onChange("scripts", "auto_update", v)}
        />
      </SettingRow>

      {/* Subscriptions */}
      <div className="pt-2 border-t border-border-subtle">
        <div className="flex items-center gap-2 mb-3">
          <Globe className="w-4 h-4 text-foreground-secondary" />
          <span className="text-sm font-medium text-foreground">订阅源</span>
        </div>
        <SubscriptionManager
          subscriptions={config.scripts.subscriptions ?? []}
          onChange={(subs) => onChange("scripts", "subscriptions", subs)}
        />
      </div>
    </CardExpander>
  );
}

function SecuritySettings({
  config,
  onChange,
}: {
  config: EngineConfig;
  onChange: (section: keyof EngineConfig, field: string, value: unknown) => void;
}) {
  return (
    <CardExpander
      icon={<Shield className="w-4 h-4" />}
      title="安全"
      description="权限与编辑入口设置"
      defaultOpen={true}
    >
      <SettingRow
        label="安全模式"
        description="严格：未声明权限就阻止运行；普通：仅提醒，不阻止。"
        helpWide
      >
        <SelectInput
          value={config.security.mode}
          options={[
            { label: "严格", value: "strict" },
            { label: "普通", value: "normal" },
          ]}
          onChange={(v) => onChange("security", "mode", v)}
        />
      </SettingRow>
      <div className="text-xs text-foreground-secondary px-1 pb-1 leading-relaxed border-t border-border-subtle pt-3 mt-1">
        脚本与触发器详情页的「编辑」「删除」仅在设置中开启「开发者模式」后显示。
      </div>
    </CardExpander>
  );
}

function ReplaySettings({
  config,
  onChange,
  flushSave,
}: {
  config: EngineConfig;
  onChange: (section: keyof EngineConfig, field: string, value: unknown) => void;
  flushSave: () => Promise<void>;
}) {
  const showError = useEngineStore((s) => s.showError);
  const [verifying, setVerifying] = useState(false);
  const [verifyMsg, setVerifyMsg] = useState<string | null>(null);

  const runVerify = async () => {
    setVerifyMsg(null);
    if (!config.replay.artifact_root.trim()) {
      showError(
        "无法校验",
        "请先填写「回放产物根目录」。录制与回放均使用该路径下的会话子目录。"
      );
      return;
    }
    const session = config.replay.session_name.trim();
    if (!session || session.includes("/") || session.includes("\\")) {
      showError(
        "无法校验",
        "「会话目录名」应为单层文件夹名（与录制会话名一致），不能包含路径分隔符。"
      );
      return;
    }

    setVerifying(true);
    try {
      await flushSave();
      const msg = await invokeAction<string>(
        "replay_verify_session",
        { session_name: session },
        { showError: true, errorTitle: "回放校验失败" },
        showError
      );
      setVerifyMsg(msg);
    } catch {
      // invokeAction 已弹出错误框
    } finally {
      setVerifying(false);
    }
  };

  const rep = config.replay;

  return (
    <>
      <CardExpander
        icon={<Clapperboard className="w-4 h-4" />}
        title="录制 / 回放 (R3)"
        description="时间线：engine_event、frame、script_input（脚本触发的鼠标/键盘等）+ Golden 断言"
        descriptionWide
        defaultOpen={true}
      >
        <SettingRow label="模式" description="录制写工件；回放用已录 PNG 帧驱动截图">
          <SelectInput
            value={rep.mode}
            options={[
              { label: "关闭", value: "normal" },
              { label: "录制 timeline + 可选帧", value: "record" },
              { label: "按工件回放帧", value: "replay" },
            ]}
            onChange={(v) => onChange("replay", "mode", v)}
          />
        </SettingRow>
        <SettingRow
          label="回放产物根目录"
          description="录制/回放会话的父路径（实际按当前插件隔离到 根目录/插件ID/会话名）"
        >
          <FolderPicker
            value={rep.artifact_root}
            onChange={(v) => onChange("replay", "artifact_root", v)}
            placeholder="例如 replay-out 或选择目录..."
            directory
          />
        </SettingRow>
        <SettingRow
          label="会话目录名"
          description="会话文件夹名，工件位于：根目录/会话名/"
        >
          <TextInput
            value={rep.session_name}
            onChange={(v) => onChange("replay", "session_name", v)}
            placeholder="例如 run_001"
          />
        </SettingRow>
        <SettingRow
          label="帧抽样间隔"
          description="录制时每 N 次成功截图写入一帧；0 = 不写帧"
        >
          <NumberInput
            value={rep.frame_sample_interval}
            min={0}
            max={600}
            onChange={(v) => onChange("replay", "frame_sample_interval", v)}
          />
        </SettingRow>
      </CardExpander>

      <CardExpander
        icon={<BadgeCheck className="w-4 h-4" />}
        title="断言校验"
        description="检查当前回放会话是否符合预期。点击“校验会话”前会先保存当前配置。"
        descriptionWide
        defaultOpen={true}
      >
        <div className="flex flex-wrap items-center gap-3">
          <button
            type="button"
            onClick={() => void runVerify()}
            disabled={verifying}
            className={cn(
              "px-3 py-2 rounded-md text-sm font-medium border transition-colors",
              verifying
                ? "opacity-60 border-border text-foreground-tertiary cursor-wait"
                : "border-primary bg-primary/10 text-primary hover:bg-primary/15"
            )}
          >
            {verifying ? "校验中..." : `校验会话「${rep.session_name.trim() || "…"}」`}
          </button>
          {verifyMsg && (
            <span className="text-sm text-green-600 dark:text-green-400 font-medium shrink-0">
              {verifyMsg}
            </span>
          )}
        </div>
      </CardExpander>
    </>
  );
}

type OcrPresetKey = "performance" | "balanced" | "accuracy";

function AdvancedSettings({
  config,
  onChange,
  onReset,
  flushSave,
  debugEnvUnlocked,
}: {
  config: EngineConfig;
  onChange: (section: keyof EngineConfig, field: string, value: unknown) => void;
  onReset: () => void;
  flushSave: () => Promise<void>;
  debugEnvUnlocked: boolean;
}) {
  const [devMode, setDevMode] = useState(
    () => localStorage.getItem(DEV_MODE_KEY) === "true"
  );
  const showDebugTools = devMode || debugEnvUnlocked;
  const toggleDevMode = (v: boolean) => {
    setDevMode(v);
    localStorage.setItem(DEV_MODE_KEY, String(v));
    window.dispatchEvent(new CustomEvent("developer-mode-changed", { detail: v }));
  };

  const applyOcrPreset = (preset: OcrPresetKey) => {
    const profile = config.advanced.ocr_presets[preset];
    onChange("advanced", "ocr_max_side_len", profile.max_side_len);
    onChange("advanced", "ocr_det_threshold", profile.det_threshold);
    onChange("advanced", "ocr_rec_threshold", profile.rec_threshold);
    onChange("advanced", "ocr_batch_size", profile.batch_size);
    onChange("advanced", "ocr_unclip_ratio", profile.unclip_ratio);
  };

  const scorePresetDistance = (preset: OcrPresetKey): number => {
    const current = config.advanced;
    const profile = current.ocr_presets[preset];
    const maxSideDiff = Math.abs(current.ocr_max_side_len - profile.max_side_len) / 4096;
    const detDiff = Math.abs(current.ocr_det_threshold - profile.det_threshold);
    const recDiff = Math.abs(current.ocr_rec_threshold - profile.rec_threshold);
    const batchDiff = Math.abs(current.ocr_batch_size - profile.batch_size) / 256;
    const unclipDiff = Math.abs(current.ocr_unclip_ratio - profile.unclip_ratio) / 10;
    return maxSideDiff + detDiff + recDiff + batchDiff + unclipDiff;
  };

  const activeOcrPreset: OcrPresetKey = (["performance", "balanced", "accuracy"] as const).reduce(
    (best, candidate) =>
      scorePresetDistance(candidate) < scorePresetDistance(best) ? candidate : best,
    "balanced"
  );

  return (
    <>
      <CardExpander
        icon={<Settings2 className="w-4 h-4" />}
        title="高级设置"
        description="OCR 引擎、日志级别等高级选项"
        defaultOpen={true}
      >
        <SettingRow label="OCR 引擎">
          <SelectInput
            value={config.advanced.ocr_engine}
            options={[
              { label: "PaddleOCR", value: "paddle_ocr" },
            ]}
            onChange={(v) => onChange("advanced", "ocr_engine", v)}
          />
        </SettingRow>
        <SettingRow label="硬件加速">
          <SelectInput
            value={config.advanced.hardware_acceleration}
            options={[
              { label: "自动", value: "auto" },
              { label: "CUDA", value: "cuda" },
              { label: "DirectML", value: "direct_ml" },
              { label: "CPU", value: "cpu" },
            ]}
            onChange={(v) => onChange("advanced", "hardware_acceleration", v)}
          />
        </SettingRow>
        <SettingRow label="输入模式" description="自动: 前台优先（先激活目标窗口），失败回退后台">
          <SelectInput
            value={config.advanced.input_mode}
            options={[
              { label: "自动 (前台优先，失败回退后台)", value: "auto" },
              { label: "前台输入 (SendInput)", value: "foreground" },
              { label: "后台输入 (PostMessage)", value: "background" },
            ]}
            onChange={(v) => onChange("advanced", "input_mode", v)}
          />
        </SettingRow>
        <SettingRow label="日志级别">
          <SelectInput
            value={config.advanced.log_level}
            options={[
              { label: "Debug", value: "debug" },
              { label: "Info", value: "info" },
              { label: "Warn", value: "warn" },
              { label: "Error", value: "error" },
            ]}
            onChange={(v) => onChange("advanced", "log_level", v)}
          />
        </SettingRow>
        <SettingRow label="输入限速" description="输入操作最小间隔 (ms)，0 不限">
          <NumberInput
            value={config.advanced.input_rate_limit}
            min={0}
            max={5000}
            onChange={(v) => onChange("advanced", "input_rate_limit", v)}
          />
        </SettingRow>
        <SettingRow label="模板匹配阈值" description="模板匹配置信信度阈值 (0-1)">
          <NumberInput
            value={config.advanced.template_match_threshold}
            min={0}
            max={1}
            onChange={(v) => onChange("advanced", "template_match_threshold", v)}
          />
        </SettingRow>
      </CardExpander>

      {/* Developer mode */}
      <CardExpander
        icon={<Bug className="w-4 h-4" />}
        title="开发者模式"
        description="启用后可访问调试工具和开发者功能；若进程环境变量 BETTER_NTE_DEBUG=1，将显示通用/截图等页面及下方调试项"
        defaultOpen={showDebugTools}
        headerRight={
          <Toggle checked={devMode} onChange={toggleDevMode} />
        }
      >
        <SettingRow label="开发者模式" description="在界面中显示调试信息和开发者工具">
          <Toggle checked={devMode} onChange={toggleDevMode} />
        </SettingRow>

        {(
          <>
            <ReplaySettings config={config} onChange={onChange} flushSave={flushSave} />

            {showDebugTools && (
              <div className="pt-2 border-t border-border-subtle">
                <div className="text-xs font-medium text-foreground-secondary mb-3">
                  调试工具
                </div>
                <SettingRow label="脚本 API 调试跟踪" description="包装 ScriptContext，向调试面板发送识别/输入等调用轨迹与缩略图（需重启引擎后生效）">
                  <Toggle
                    checked={config.advanced.debug_mode}
                    onChange={(v) => onChange("advanced", "debug_mode", v)}
                  />
                </SettingRow>
                <SettingRow label="调试截图目录" description="调试截图的保存路径，留空则不保存">
                  <FolderPicker
                    value={config.advanced.debug_screenshot_dir ?? ""}
                    onChange={(v) => onChange("advanced", "debug_screenshot_dir", v)}
                    placeholder="选择目录..."
                    directory
                  />
                </SettingRow>
              </div>
            )}

            <div className="pt-2 border-t border-border-subtle">
              <div className="text-xs font-medium text-foreground-secondary mb-3">
                性能调试
              </div>
              <SettingRow
                label="OCR 参数预设"
                description="一键应用常用参数组合，便于在速度与识别率之间切换"
              >
                <div className="flex items-center gap-2 flex-wrap">
                  <button
                    type="button"
                    onClick={() => applyOcrPreset("performance")}
                    className={cn(
                      "px-2.5 py-1 rounded-md border text-xs transition-colors",
                      activeOcrPreset === "performance"
                        ? "border-primary bg-primary/10 text-primary"
                        : "border-border text-foreground-secondary hover:text-foreground hover:bg-surface-hover"
                    )}
                  >
                    性能优先
                  </button>
                  <button
                    type="button"
                    onClick={() => applyOcrPreset("balanced")}
                    className={cn(
                      "px-2.5 py-1 rounded-md border text-xs transition-colors",
                      activeOcrPreset === "balanced"
                        ? "border-primary bg-primary/10 text-primary"
                        : "border-border text-foreground-secondary hover:text-foreground hover:bg-surface-hover"
                    )}
                  >
                    平衡
                  </button>
                  <button
                    type="button"
                    onClick={() => applyOcrPreset("accuracy")}
                    className={cn(
                      "px-2.5 py-1 rounded-md border text-xs transition-colors",
                      activeOcrPreset === "accuracy"
                        ? "border-primary bg-primary/10 text-primary"
                        : "border-border text-foreground-secondary hover:text-foreground hover:bg-surface-hover"
                    )}
                  >
                    精度优先
                  </button>
                  <span className="text-xs text-foreground-tertiary ml-1">
                    当前最接近：{
                      activeOcrPreset === "performance"
                        ? "性能优先"
                        : activeOcrPreset === "accuracy"
                          ? "精度优先"
                          : "平衡"
                    }
                  </span>
                </div>
              </SettingRow>
              <SettingRow label="OCR 检测阈值" description="OCR 文字检测置信度阈值">
                <NumberInput
                  value={config.advanced.ocr_det_threshold}
                  min={0}
                  max={1}
                  onChange={(v) => onChange("advanced", "ocr_det_threshold", v)}
                />
              </SettingRow>
              <SettingRow label="OCR 识别阈值" description="OCR 文字识别置信度阈值">
                <NumberInput
                  value={config.advanced.ocr_rec_threshold}
                  min={0}
                  max={1}
                  onChange={(v) => onChange("advanced", "ocr_rec_threshold", v)}
                />
              </SettingRow>
              <SettingRow label="OCR 最大边长" description="检测前缩放长边上限，越大越准但越慢">
                <NumberInput
                  value={config.advanced.ocr_max_side_len}
                  min={32}
                  max={4096}
                  onChange={(v) =>
                    onChange(
                      "advanced",
                      "ocr_max_side_len",
                      Math.max(32, Math.floor(Number.isFinite(v) ? v : 960))
                    )
                  }
                />
              </SettingRow>
              <SettingRow label="OCR 批处理大小" description="单次 OCR 识别推理的文本框批量大小">
                <NumberInput
                  value={config.advanced.ocr_batch_size}
                  min={1}
                  max={256}
                  onChange={(v) =>
                    onChange("advanced", "ocr_batch_size", Math.max(1, Math.floor(v)))
                  }
                />
              </SettingRow>
              <SettingRow label="OCR Unclip 比例" description="文字框扩张比例，越大越容易覆盖完整文本">
                <NumberInput
                  value={config.advanced.ocr_unclip_ratio}
                  min={0.1}
                  max={10}
                  onChange={(v) =>
                    onChange("advanced", "ocr_unclip_ratio", Number.isFinite(v) ? Math.max(v, 0.1) : 2.0)
                  }
                />
              </SettingRow>
            </div>

            <div className="pt-2 border-t border-border-subtle">
              <div className="text-xs font-medium text-foreground-secondary mb-3">
                重置
              </div>
              <SettingRow label="重置所有设置" description="恢复所有配置到默认值">
                <button
                  onClick={onReset}
                  className="px-3 py-1.5 rounded-md bg-destructive/10 border border-destructive/30 text-destructive text-sm font-medium hover:bg-destructive/20 transition-colors"
                >
                  重置
                </button>
              </SettingRow>
            </div>
          </>
        )}
      </CardExpander>
    </>
  );
}

// ============================================================================
// Main Settings Component
// ============================================================================

export function Settings() {
  const [activeTab, setActiveTab] = useState<SettingsTab>("hotkeys");
  const [debugEnvUnlocked, setDebugEnvUnlocked] = useState(false);
  /** Bumps after full reset so AdvancedSettings remounts and re-reads developer mode from storage. */
  const [advancedUiResetKey, setAdvancedUiResetKey] = useState(0);
  const config = useEngineStore((s) => s.config);
  const initialized = useEngineStore((s) => s.initialized);
  const saveConfig = useEngineStore((s) => s.saveConfig);
  const loadConfig = useEngineStore((s) => s.loadConfig);
  const initEngine = useEngineStore((s) => s.initEngine);

  const [draft, setDraft] = useState<EngineConfig | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void invokeAction<boolean>("better_nte_debug_enabled", undefined, {
      silent: true,
    }).then((v) => {
      setDebugEnvUnlocked(Boolean(v));
    });
  }, []);

  // Ensure engine is initialized and config is loaded
  useEffect(() => {
    if (!initialized) {
      initEngine();
    } else if (!config) {
      loadConfig();
    }
  }, [initialized, config, initEngine, loadConfig]);

  const visibleTabs = useMemo(
    () => visibleTabsFor(debugEnvUnlocked),
    [debugEnvUnlocked]
  );

  // Sync draft from store config
  useEffect(() => {
    if (config) {
      setDraft(structuredClone(config));
    }
  }, [config]);

  // Auto-save: debounce saves after last change
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (saveTimerRef.current) {
        clearTimeout(saveTimerRef.current);
        saveTimerRef.current = null;
      }
    },
    []
  );

  const autoSave = useCallback(
    (newConfig: EngineConfig) => {
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
      saveTimerRef.current = setTimeout(async () => {
        setSaving(true);
        try {
          await saveConfig(newConfig);
        } catch {
          // error logged by store
        } finally {
          setSaving(false);
        }
      }, CONFIG_AUTOSAVE_DEBOUNCE_MS);
    },
    [saveConfig]
  );

  const handleChange = useCallback(
    (section: keyof EngineConfig, field: string, value: unknown) => {
      setDraft((prev) => {
        if (!prev) return prev;
        const updated = updateConfig(prev, section, field, value);
        autoSave(updated);
        return updated;
      });
    },
    [autoSave]
  );

  const handleRootChange = useCallback(
    (field: RootConfigField, value: unknown) => {
      setDraft((prev) => {
        if (!prev) return prev;
        const updated = updateRootConfig(prev, field, value);
        autoSave(updated);
        return updated;
      });
    },
    [autoSave]
  );

  const handleHotkeyTriggersReplace = useCallback(
    (triggers: HotkeyTriggersConfig) => {
      setDraft((prev) => {
        if (!prev) return prev;
        const updated = { ...prev, hotkey_triggers: triggers };
        autoSave(updated);
        return updated;
      });
    },
    [autoSave]
  );

  const handleReset = useCallback(async () => {
    if (!config) return;
    const defaultConfig = mapEngineConfig({});
    localStorage.removeItem(DEV_MODE_KEY);
    window.dispatchEvent(
      new CustomEvent("developer-mode-changed", { detail: false })
    );
    setAdvancedUiResetKey((n) => n + 1);
    setDraft(defaultConfig);
    setSaving(true);
    try {
      await saveConfig(defaultConfig);
    } catch {
      // error logged by store
    } finally {
      setSaving(false);
    }
  }, [config, saveConfig]);

  const flushSave = useCallback(async () => {
    if (!draft) return;
    if (saveTimerRef.current) {
      clearTimeout(saveTimerRef.current);
      saveTimerRef.current = null;
    }
    setSaving(true);
    try {
      await saveConfig(draft);
    } finally {
      setSaving(false);
    }
  }, [draft, saveConfig]);

  if (!draft) {
    return (
      <div className="p-6 max-w-3xl">
        <h1 className="text-lg font-semibold text-foreground mb-5">设置</h1>
        <div className="flex items-center gap-2 text-foreground-tertiary text-sm">
          <div className="w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin" />
          加载配置中...
        </div>
      </div>
    );
  }

  const sectionProps = { config: draft, onChange: handleChange };

  const tabContent: Record<SettingsTab, React.ReactNode> = {
    general: <GeneralSettings config={draft} onRootChange={handleRootChange} />,
    capture: <CaptureSettings {...sectionProps} />,
    hotkeys: (
      <HotkeySettings
        {...sectionProps}
        onHotkeyTriggersReplace={handleHotkeyTriggersReplace}
      />
    ),
    overlay: <OverlaySettings {...sectionProps} />,
    notifications: <NotificationSettings {...sectionProps} />,
    scripts: <ScriptSettings {...sectionProps} />,
    security: <SecuritySettings {...sectionProps} />,
    advanced: (
      <AdvancedSettings
        key={advancedUiResetKey}
        {...sectionProps}
        onReset={handleReset}
        flushSave={flushSave}
        debugEnvUnlocked={debugEnvUnlocked}
      />
    ),
  };

  return (
    <div className="p-6 max-w-3xl">
      <div className="flex items-center justify-between mb-5">
        <h1 className="text-lg font-semibold text-foreground">设置</h1>
        {saving && (
          <span className="text-xs text-foreground-tertiary">保存中...</span>
        )}
      </div>

      <div className="flex gap-1 mb-6 bg-surface/50 rounded-lg p-1 border border-border-subtle">
        {visibleTabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={cn(
              "flex items-center gap-2 px-3 py-2 rounded-md text-sm font-medium transition-colors",
              activeTab === tab.id
                ? "bg-card text-foreground shadow-sm"
                : "text-foreground-tertiary hover:text-foreground-secondary"
            )}
          >
            {tab.icon}
            {tab.label}
          </button>
        ))}
      </div>

      <div className="space-y-4">{tabContent[activeTab]}</div>
    </div>
  );
}
