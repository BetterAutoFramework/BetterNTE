import { createContext, useContext, useState, useCallback, type ReactNode } from "react";

// ─── Translation dictionaries ────────────────────────────────────────────────

const zh = {
  // Tooltips
  "tooltip.deleteSelected": (n: number) => `删除选中 (${n})`,
  "tooltip.exitSelect": "退出多选",
  "tooltip.multiSelect": "多选",
  "tooltip.clearAll": "清空全部",
  "tooltip.capture": "截图 (F9)",
  "tooltip.stopRecording": "停止录制",
  "tooltip.startRecording": "录制 (F10)",
  "tooltip.pan": "平移 [H]",
  "tooltip.pick": "取色 [I]",
  "tooltip.rect": "矩形 [M]",
  "tooltip.ellipse": "椭圆 [E]",
  "tooltip.roundRect": "圆角矩形 [R]",
  "tooltip.polygon": "多边形 [L]",
  "tooltip.fitView": "适应视图 [0]",
  "tooltip.saveSelection": "保存选区",
  "tooltip.saveFull": "保存整图",
  "tooltip.scrollCapture": "滚动截图",
  "tooltip.panoCapture": "全景截图",
  "tooltip.settings": "设置",
  "tooltip.refreshWindows": "刷新窗口列表",

  // Placeholders
  "placeholder.filterWindows": "输入窗口标题关键字过滤...",
  "placeholder.selectWindow": "选择窗口...",
  "placeholder.notSet": "未设置",
  "placeholder.enterFilename": "输入文件名",

  // Buttons
  "btn.browse": "浏览",
  "btn.cancel": "取消",
  "btn.save": "保存",
  "btn.saveJson": "保存 JSON",
  "btn.resetDefaults": "恢复默认",
  "btn.settings": "设置",
  "btn.pressKey": "按下按键...",

  // Status bar
  "status.pickHint": (n: number) => `已取 ${n} 个点 · Alt+点击取点 · Enter 保存 · Esc 清除`,
  "status.inspectHint": "移动鼠标查看颜色 · 取色工具点击复制",
  "status.copied": (hex: string) => `已复制 ${hex}`,
  "status.cleared": (n: number) => `已清空 ${n} 个文件`,

  // Dialog titles
  "dialog.settings": "设置",
  "dialog.saveSelection": "保存选区",
  "dialog.savePickPoints": "保存取色点",

  // Settings labels
  "setting.window": "窗口",
  "setting.screenshotDir": "截图帧目录",
  "setting.cropDir": "裁剪帧目录",
  "setting.captureInterval": "截图间隔 (ms)",
  "setting.roundedRadius": "圆角矩形默认半径",
  "setting.scrollCapture": "滚动截图",
  "setting.panoCapture": "全景截图",
  "setting.shortcuts": "快捷键",
  "setting.language": "语言",

  // Settings sub-labels
  "setting.direction": "方向",
  "setting.scrollAmount": "滚动量",
  "setting.frames": "帧数",
  "setting.delay": "延迟 (ms)",
  "setting.dragDistance": "拖拽距离",

  // Dialog content
  "dialog.filename": "文件名",
  "dialog.savingTo": (dir: string) => `保存到: ${dir}`,
  "dialog.savingPointsTo": (n: number, dir: string) => `保存 ${n} 个点到: ${dir}`,
  "dialog.noDirSet": "未设置目录",
  "dialog.fileOperations": "文件操作",

  // Error messages
  "error.selectWindow": "请先选择窗口",
  "error.selectRegion": "请先框选区域",
  "error.noSelectionOrDir": "没有选区或未设置保存目录",
  "error.enterFilename": "请输入文件名",
  "error.setCropDir": "请先设置裁剪目录",

  // Sidebar
  "sidebar.screenshots": (n: number) => `截图帧 (${n})`,
  "sidebar.cropDir": (n: number) => `裁剪帧目录 (${n})`,
  "sidebar.noFrames": "暂无截图帧，请按 F9 截图或 F10 录制。",
  "sidebar.selectWindow": "选择窗口后按 F9 或点击截图",
  "sidebar.noCropDir": "未设置裁剪目录，请在设置中配置。",
  "sidebar.noCropFiles": "裁剪目录暂无文件。",

  // Context menu
  "menu.copyFilename": "复制文件名",
  "menu.copyPath": "复制完整路径",
  "menu.copyJsCode": "复制 JS 代码",
  "menu.revealInExplorer": "在资源管理器中显示",
  "menu.delete": "删除",

  // Directions
  "dir.down": "下",
  "dir.up": "上",
  "dir.right": "右",
  "dir.left": "左",

  // Misc
  "misc.noWindow": "未选择窗口",
  "misc.starting": "准备中...",
  "misc.noMatchingWindows": "无匹配窗口",
  "misc.unitPx": (v: number) => `${v}px`,

  // Shortcut labels (keybinding action names)
  "shortcut.tool.pan": "平移",
  "shortcut.tool.pick": "取色",
  "shortcut.tool.rect": "矩形选区",
  "shortcut.tool.ellipse": "椭圆选区",
  "shortcut.tool.roundrect": "圆角矩形选区",
  "shortcut.tool.polygon": "多边形选区",
  "shortcut.view.reset": "重置视图",
  "shortcut.view.zoomIn": "放大",
  "shortcut.view.zoomOut": "缩小",
  "shortcut.selection.cancel": "取消 / 清除",
  "shortcut.selection.confirm": "确认选区",
  "shortcut.capture.screenshot": "截图",
  "shortcut.capture.toggleRecord": "切换录制",
  "shortcut.pick.save": "保存取色点",
};

const en = {
  // Tooltips
  "tooltip.deleteSelected": (n: number) => `Delete selected (${n})`,
  "tooltip.exitSelect": "Exit select",
  "tooltip.multiSelect": "Multi-select",
  "tooltip.clearAll": "Clear all",
  "tooltip.capture": "Capture (F9)",
  "tooltip.stopRecording": "Stop recording",
  "tooltip.startRecording": "Record (F10)",
  "tooltip.pan": "Pan [H]",
  "tooltip.pick": "Pick [I]",
  "tooltip.rect": "Rect [M]",
  "tooltip.ellipse": "Ellipse [E]",
  "tooltip.roundRect": "RoundRect [R]",
  "tooltip.polygon": "Polygon [L]",
  "tooltip.fitView": "Fit to view [0]",
  "tooltip.saveSelection": "Save selection",
  "tooltip.saveFull": "Save full image",
  "tooltip.scrollCapture": "Scroll capture",
  "tooltip.panoCapture": "Panoramic capture",
  "tooltip.settings": "Settings",
  "tooltip.refreshWindows": "Refresh window list",

  // Placeholders
  "placeholder.filterWindows": "Filter by title keyword...",
  "placeholder.selectWindow": "Select a window...",
  "placeholder.notSet": "Not set",
  "placeholder.enterFilename": "Enter filename",

  // Buttons
  "btn.browse": "Browse",
  "btn.cancel": "Cancel",
  "btn.save": "Save",
  "btn.saveJson": "Save JSON",
  "btn.resetDefaults": "Reset to defaults",
  "btn.settings": "Settings",
  "btn.pressKey": "Press a key...",

  // Status bar
  "status.pickHint": (n: number) => `${n} pick(s) · Alt+click to pick · Enter to save · Esc to clear`,
  "status.inspectHint": "Move mouse to inspect · Click to copy (pick tool)",
  "status.copied": (hex: string) => `Copied ${hex}`,
  "status.cleared": (n: number) => `Cleared ${n} files`,

  // Dialog titles
  "dialog.settings": "Settings",
  "dialog.saveSelection": "Save selection",
  "dialog.savePickPoints": "Save pick points",

  // Settings labels
  "setting.window": "Window",
  "setting.screenshotDir": "Screenshot frames directory",
  "setting.cropDir": "Crop frames directory",
  "setting.captureInterval": "Capture interval (ms)",
  "setting.roundedRadius": "Round rect default radius",
  "setting.scrollCapture": "Scroll Capture",
  "setting.panoCapture": "Panoramic Capture",
  "setting.shortcuts": "Keyboard Shortcuts",
  "setting.language": "Language",

  // Settings sub-labels
  "setting.direction": "Direction",
  "setting.scrollAmount": "Scroll amount",
  "setting.frames": "Frames",
  "setting.delay": "Delay (ms)",
  "setting.dragDistance": "Drag distance",

  // Dialog content
  "dialog.filename": "Filename",
  "dialog.savingTo": (dir: string) => `Saving to: ${dir}`,
  "dialog.savingPointsTo": (n: number, dir: string) => `Saving ${n} point(s) to: ${dir}`,
  "dialog.noDirSet": "No directory set",
  "dialog.fileOperations": "File operations",

  // Error messages
  "error.selectWindow": "Please select a window first",
  "error.selectRegion": "Please select a region first",
  "error.noSelectionOrDir": "No selection or save directory not set",
  "error.enterFilename": "Please enter a filename",
  "error.setCropDir": "Please set a crop directory first",

  // Sidebar
  "sidebar.screenshots": (n: number) => `Screenshots (${n})`,
  "sidebar.cropDir": (n: number) => `Crop Directory (${n})`,
  "sidebar.noFrames": "No frames. Capture with F9 or record with F10.",
  "sidebar.selectWindow": "Select a window then press F9 or click Capture",
  "sidebar.noCropDir": "No crop directory set. Configure in Settings.",
  "sidebar.noCropFiles": "No files in crop directory.",

  // Context menu
  "menu.copyFilename": "Copy filename",
  "menu.copyPath": "Copy full path",
  "menu.copyJsCode": "Copy JS Code",
  "menu.revealInExplorer": "Reveal in Explorer",
  "menu.delete": "Delete",

  // Directions
  "dir.down": "Down",
  "dir.up": "Up",
  "dir.right": "Right",
  "dir.left": "Left",

  // Misc
  "misc.noWindow": "No window",
  "misc.starting": "Starting...",
  "misc.noMatchingWindows": "No matching windows",
  "misc.unitPx": (v: number) => `${v}px`,

  // Shortcut labels
  "shortcut.tool.pan": "Pan",
  "shortcut.tool.pick": "Pick color",
  "shortcut.tool.rect": "Rectangle select",
  "shortcut.tool.ellipse": "Ellipse select",
  "shortcut.tool.roundrect": "Round rect select",
  "shortcut.tool.polygon": "Polygon select",
  "shortcut.view.reset": "Reset view",
  "shortcut.view.zoomIn": "Zoom in",
  "shortcut.view.zoomOut": "Zoom out",
  "shortcut.selection.cancel": "Cancel / clear",
  "shortcut.selection.confirm": "Confirm selection",
  "shortcut.capture.screenshot": "Screenshot",
  "shortcut.capture.toggleRecord": "Toggle recording",
  "shortcut.pick.save": "Save pick points",
};

// ─── Types ────────────────────────────────────────────────────────────────────

export type Lang = "zh" | "en";

type Dict = typeof zh;
export type TranslationKey = keyof Dict;

// The value can be a string or a function that returns a string
type TranslationValue = string | ((...args: never[]) => string);
type ResolvedDict = { [K in TranslationKey]: string };

// ─── Context ──────────────────────────────────────────────────────────────────

interface I18nContextValue {
  lang: Lang;
  setLang: (lang: Lang) => void;
  t: (key: TranslationKey, ...args: unknown[]) => string;
}

const I18nContext = createContext<I18nContextValue>({
  lang: "zh",
  setLang: () => {},
  t: (key) => key,
});

// ─── Provider ─────────────────────────────────────────────────────────────────

const DICTS: Record<Lang, Dict> = { zh, en };

export function I18nProvider({
  children,
  initialLang = "zh",
  onLangChange,
}: {
  children: ReactNode;
  initialLang?: Lang;
  onLangChange?: (lang: Lang) => void;
}) {
  const [lang, setLangState] = useState<Lang>(initialLang);

  const setLang = useCallback(
    (newLang: Lang) => {
      setLangState(newLang);
      onLangChange?.(newLang);
    },
    [onLangChange],
  );

  const t = useCallback(
    (key: TranslationKey, ...args: unknown[]): string => {
      const dict = DICTS[lang] ?? DICTS.zh;
      const val = dict[key] ?? DICTS.zh[key] ?? key;
      if (typeof val === "function") {
        return (val as (...a: unknown[]) => string)(...args);
      }
      return val as string;
    },
    [lang],
  );

  return (
    <I18nContext.Provider value={{ lang, setLang, t }}>
      {children}
    </I18nContext.Provider>
  );
}

export function useI18n() {
  return useContext(I18nContext);
}

// ─── Helper: get shortcut label from keybinding action ────────────────────────

export function getShortcutLabelKey(action: string): TranslationKey | null {
  const key = `shortcut.${action}` as TranslationKey;
  if (key in zh) return key;
  return null;
}
