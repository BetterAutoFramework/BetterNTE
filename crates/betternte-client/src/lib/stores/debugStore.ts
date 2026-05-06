import type { StateCreator } from "zustand";

import { DEBUG_ENTRIES_MAX } from "../constants/timing";
import type { DebugEntry } from "../types";
import type { CombinedStore } from "./index";

// ============================================================================
// State + Actions
// ============================================================================

export interface DebugSlice {
  debugOpen: boolean;
  debugEntries: DebugEntry[];

  toggleDebug: () => void;
  clearDebug: () => void;
  addDebugEntry: (entry: DebugEntry) => void;
}

// ============================================================================
// State creator
// ============================================================================

export const createDebugSlice: StateCreator<CombinedStore, [], [], DebugSlice> = (set) => ({
  debugOpen: false,
  debugEntries: [],

  toggleDebug: () => set((s) => ({ debugOpen: !s.debugOpen })),

  clearDebug: () => set({ debugEntries: [] }),

  addDebugEntry: (entry: DebugEntry) =>
    set((s) => ({
      debugEntries: [...s.debugEntries, entry].slice(-DEBUG_ENTRIES_MAX),
    })),
});
