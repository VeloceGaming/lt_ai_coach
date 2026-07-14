// Preferences drawer — the user's persisted settings (draft mode, scoring
// weights, draft strategy + tuning, minimum interaction games, compact toggle,
// auto-overlay).
//
// Behavior matches the old DraftBoard approach exactly: the initial values are
// read from localStorage at startup, and every change is written straight back.
// Persistence lives in the setters here, replacing the save-on-change effect
// that used to live in the component.

import { create } from "zustand";
import type { DraftMode, DraftStrategy, DraftTuning, ScoringWeights, UserPreferences } from "../types";
import { loadUserPreferences, saveUserPreferences } from "../lib/preferences";

type PreferencesState = UserPreferences & {
  setMode: (mode: DraftMode) => void;
  setBansPerSide: (value: number) => void;
  setWeight: (key: keyof ScoringWeights, value: number) => void;
  setStrategy: (strategy: DraftStrategy) => void;
  setCustomTuning: (key: keyof DraftTuning, value: number) => void;
  setMinimumInteractionGames: (value: number) => void;
  setCompactMode: (value: boolean) => void;
  setAutoOverlay: (value: boolean) => void;
  setDebugMode: (value: boolean) => void;
};

// Write the full preferences snapshot to localStorage, mirroring the original
// save effect (which always persisted the whole object on any change).
function persist(state: PreferencesState) {
  saveUserPreferences({
    mode: state.mode,
    bansPerSide: state.bansPerSide,
    weights: state.weights,
    strategy: state.strategy,
    customTuning: state.customTuning,
    minimumInteractionGames: state.minimumInteractionGames,
    compactMode: state.compactMode,
    autoOverlay: state.autoOverlay,
    debugMode: state.debugMode,
  });
}

export const usePreferencesStore = create<PreferencesState>((set, get) => ({
  ...loadUserPreferences(),
  setMode: (mode) => { set({ mode }); persist(get()); },
  setBansPerSide: (value) => { set({ bansPerSide: Math.max(1, Math.min(5, Math.round(value))) }); persist(get()); },
  setWeight: (key, value) => { set({ weights: { ...get().weights, [key]: value } }); persist(get()); },
  setStrategy: (strategy) => { set({ strategy }); persist(get()); },
  setCustomTuning: (key, value) => {
    set({ customTuning: { ...get().customTuning, [key]: value }, strategy: "custom" });
    persist(get());
  },
  setMinimumInteractionGames: (value) => { set({ minimumInteractionGames: value }); persist(get()); },
  setCompactMode: (value) => { set({ compactMode: value }); persist(get()); },
  setAutoOverlay: (value) => { set({ autoOverlay: value }); persist(get()); },
  setDebugMode: (value) => { set({ debugMode: value }); persist(get()); },
}));
