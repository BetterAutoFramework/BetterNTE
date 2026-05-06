/** Fast polling for flow/task UI progress (editor sidebar, OneDragon task runner). */
export const UI_POLL_FAST_MS = 500;

/** Status bar elapsed timer tick while a task is running. */
export const STATUS_BAR_TICK_MS = 1000;

/** Interval for detecting stalled engine control-event stream while running. */
export const ENGINE_CONTROL_WATCHDOG_INTERVAL_MS = 3000;

/** If no control event for this long while running, treat stream as stale and resync status. */
export const ENGINE_CONTROL_WATCHDOG_STALE_MS = 8000;

/** Debounce before persisting settings draft after edits. */
export const CONFIG_AUTOSAVE_DEBOUNCE_MS = 500;

/** Debounce after CodeMirror edits before auto-save in script debug page. */
export const SCRIPT_EDITOR_AUTOSAVE_MS = 1000;

/** Cap on in-memory UI log lines appended by the control watchdog. */
export const UI_LOG_SLICE_MAX = 1000;

/** Max entries kept in the script debug panel ring buffer. */
export const DEBUG_ENTRIES_MAX = 500;
