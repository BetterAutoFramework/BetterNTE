/** Viewport width below which split panels auto-collapse (Task / Trigger / Task groups / Script debug). */
export const PANEL_THRESHOLD_PX = 640;

/** Sidebar auto-collapses below this width (slightly wider than editor split panels). */
export const SIDEBAR_PANEL_THRESHOLD_PX = 768;

/** Debug slide-over panel width constraints (inline styles). */
export const DEBUG_PANEL_WIDTH_STYLE = {
  width: "50vw",
  minWidth: "400px",
  maxWidth: "800px",
} as const;

/** Screenshot thumbnails in debug entries. */
export const DEBUG_SCREENSHOT_THUMB_MAX = { w: 200, h: 120 } as const;

/** Log drawer panel width constraints. */
export const LOG_DRAWER_WIDTH_STYLE = {
  width: "40vw",
  minWidth: "320px",
  maxWidth: "640px",
} as const;
