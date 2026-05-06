// Barrel re-export — store logic lives in stores/ subdirectory.
// This file exists for backwards compatibility so existing imports work.
export type { ErrorDialogState } from "./stores";
export { mapEngineConfig,useEngineStore } from "./stores";
