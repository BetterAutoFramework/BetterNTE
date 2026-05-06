/**
 * Keyboard shortcut configuration for Window Pixel Studio.
 *
 * Actions are namespaced: "category.action".
 * Keys are DOM KeyboardEvent.key values (lowercase for letters).
 */

export type KeyAction =
  | "tool.pan"
  | "tool.pick"
  | "tool.rect"
  | "tool.ellipse"
  | "tool.roundrect"
  | "tool.polygon"
  | "view.reset"
  | "view.zoomIn"
  | "view.zoomOut"
  | "selection.cancel"
  | "selection.confirm"
  | "capture.screenshot"
  | "capture.toggleRecord"
  | "pick.save";

export const DEFAULT_KEYBINDINGS: Record<KeyAction, string> = {
  "tool.pan": "h",
  "tool.pick": "i",
  "tool.rect": "m",
  "tool.ellipse": "e",
  "tool.roundrect": "r",
  "tool.polygon": "l",
  "view.reset": "0",
  "view.zoomIn": "=",
  "view.zoomOut": "-",
  "selection.cancel": "Escape",
  "selection.confirm": "Enter",
  "capture.screenshot": "F9",
  "capture.toggleRecord": "F10",
  "pick.save": "Enter",
};

/**
 * Build a reverse lookup: key → action.
 * If user has custom bindings, merge them over defaults.
 */
export function buildKeyMap(
  custom?: Record<string, string> | null,
): Map<string, KeyAction> {
  const merged = { ...DEFAULT_KEYBINDINGS, ...custom };
  const map = new Map<string, KeyAction>();
  for (const [action, key] of Object.entries(merged)) {
    map.set(key.toLowerCase(), action as KeyAction);
  }
  return map;
}

/**
 * Format a key for display (capitalize, show special keys).
 */
export function formatKey(key: string): string {
  const special: Record<string, string> = {
    escape: "Esc",
    enter: "Enter",
    " ": "Space",
    arrowup: "↑",
    arrowdown: "↓",
    arrowleft: "←",
    arrowright: "→",
  };
  return special[key.toLowerCase()] ?? key.toUpperCase();
}
