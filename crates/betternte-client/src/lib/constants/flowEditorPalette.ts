import type { StepKindType } from "../types";

/** React Flow node accent colors for workflow editor palette (hex). */
export const FLOW_EDITOR_STEP_COLORS: Record<StepKindType, string> = {
  script: "#3b82f6",
  click: "#22c55e",
  swipe: "#a855f7",
  key_press: "#ec4899",
  wait: "#f59e0b",
  flow: "#6366f1",
  group: "#0ea5e9",
  set_variable: "#14b8a6",
  none: "#6b7280",
};
