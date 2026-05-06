import { create } from "zustand";

import type { DebugSlice } from "./debugStore";
import { createDebugSlice } from "./debugStore";
import type { EngineSlice } from "./engineStore";
import { createEngineSlice } from "./engineStore";
import type { FlowSlice } from "./flowStore";
import { createFlowSlice } from "./flowStore";
import type { ScriptSlice } from "./scriptStore";
import { createScriptSlice } from "./scriptStore";
import type { UISlice } from "./uiStore";
import { createUISlice } from "./uiStore";

// ============================================================================
// Combined store type
// ============================================================================

export type CombinedStore = EngineSlice & ScriptSlice & FlowSlice & UISlice & DebugSlice;

// ============================================================================
// Store — combines all slices into a single Zustand store
// ============================================================================

export const useEngineStore = create<CombinedStore>()((...args) => ({
  ...createEngineSlice(...args),
  ...createScriptSlice(...args),
  ...createFlowSlice(...args),
  ...createUISlice(...args),
  ...createDebugSlice(...args),
}));

// Re-export types for convenience
export { mapEngineConfig } from "./helpers";
export type { ErrorDialogState } from "./uiStore";
