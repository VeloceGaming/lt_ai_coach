// Overlay drawer — shared live-overlay state. For now this is just the bridge
// connection flag: whether the in-game mod is currently feeding a live draft.
// It's read by the draft board's auto-overlay / auto-finish effects and the
// "live draft" indicators, and will be read by the Stage D compact overlay tree.
//
// The window-manipulation behavior itself (always-on-top, minimize, resize)
// stays as React effects in the components, since it reacts to component
// lifecycle; only the shared state lives here.

import { create } from "zustand";
import type { BridgeState, DraftLineup, DraftSide, GameRecord } from "../types";

type OverlayState = {
  bridgeConnected: boolean;
  setBridgeConnected: (connected: boolean) => void;
  userSide: DraftSide | null;
  blueLineup: DraftLineup | null;
  redLineup: DraftLineup | null;
  completedGames: GameRecord[];
  liveRevision: number;
  liveContextRevision: number;
  setLiveRevision: (revision: number) => void;
  setBridgeContext: (bridge: Pick<BridgeState, "contextRevision" | "userSide" | "blueStarters" | "redStarters" | "completedGames">) => void;
  // Live champion -> tags from the bridge mod (empty until a tags packet arrives).
  championTags: Record<string, string[]>;
  setChampionTags: (tags: Record<string, string[]>) => void;
};

export const useOverlayStore = create<OverlayState>((set) => ({
  bridgeConnected: false,
  setBridgeConnected: (connected) => set({ bridgeConnected: connected }),
  userSide: null,
  blueLineup: null,
  redLineup: null,
  completedGames: [],
  liveRevision: 0,
  liveContextRevision: 0,
  setLiveRevision: (revision) => set((state) => state.liveRevision === revision ? state : { liveRevision: revision }),
  setBridgeContext: (bridge) => {
    const lineup = (starters: number[]): DraftLineup | null => starters.length === 5
      ? { top: starters[0], jungle: starters[1], mid: starters[2], bot: starters[3], support: starters[4] }
      : null;
    set({
      userSide: bridge.userSide,
      blueLineup: lineup(bridge.blueStarters),
      redLineup: lineup(bridge.redStarters),
      completedGames: bridge.completedGames,
      liveContextRevision: bridge.contextRevision,
    });
  },
  championTags: {},
  setChampionTags: (tags) => set({ championTags: tags }),
}));
