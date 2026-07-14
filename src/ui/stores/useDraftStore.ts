// Draft drawer — the draft board's domain data and every operation that mutates
// it: the current board, undo history, and the Fearless series
// (current game + completed games). The picker UI (search text, which slot is
// selected, modal open/closed) and recommendation fetching stay in the
// component; this store owns only the draft data itself.
//
// All actions are copied verbatim from the old DraftBoard handlers, so behavior
// is unchanged — they just live here now and read the draft mode from the
// preferences drawer instead of a local variable.

import { create } from "zustand";
import type { BridgeState, DraftAction, DraftMode, DraftState, GameRecord } from "../types";
import { actionTarget, currentDraftAction, emptyDraft, removeLatestAction, unavailableReason } from "../lib/draft";
import { usePreferencesStore } from "./usePreferencesStore";

type DraftStoreState = {
  liveMatchId: number | null;
  draft: DraftState;
  history: DraftState[];
  currentGame: number;
  completedGames: GameRecord[];
  // User-confirmed role assignments for picked champions (championId -> role).
  // Overrides the engine's inferred role. Scoped to the current game: cleared
  // whenever the game/series changes.
  roleOverrides: Record<string, string>;
  setRoleOverride: (championId: string, role: string) => void;
  clearRoleOverride: (championId: string) => void;
  // Adds a champion to the slot named by `action`. Returns false (and changes
  // nothing) when the champion is unavailable, so the caller knows whether to
  // close the picker.
  pushChampion: (championId: string, action: DraftAction, bansPerSide?: number, mode?: DraftMode) => boolean;
  removeChampion: (target: keyof DraftState, championId: string) => void;
  undo: () => void;
  resetCurrent: () => void;
  saveCurrentGame: () => void;
  moveToGame: (gameNum: number) => void;
  finishGame: () => void;
  resetSeries: () => void;
  // Reset series progress to game 1 with no completed games (used when leaving
  // Fearless for Normal mode). Leaves the board untouched, like the original.
  clearSeriesProgress: () => void;
  setSeriesHistory: (blue: string[], red: string[]) => void;
  syncLiveSeries: (matchId: number, setNumber: number, completedGames: GameRecord[]) => void;
  applyBridgeUpdate: (lists: Pick<BridgeState, "blueBans" | "redBans" | "bluePicks" | "redPicks">) => void;
};

export const useDraftStore = create<DraftStoreState>((set, get) => ({
  liveMatchId: null,
  draft: emptyDraft,
  history: [],
  currentGame: 1,
  completedGames: [],
  roleOverrides: {},

  // Confirm a champion's role. A champion can hold only one role, and each role
  // can be claimed by only one champion per side, so any same-side champion
  // previously confirmed to this role has its override dropped.
  setRoleOverride: (championId, role) => set((state) => {
    const { draft } = state;
    const side = draft.bluePicks.includes(championId) ? "blue"
      : draft.redPicks.includes(championId) ? "red" : null;
    const sidePicks = side === "blue" ? draft.bluePicks : side === "red" ? draft.redPicks : [];
    const next: Record<string, string> = {};
    for (const [id, existing] of Object.entries(state.roleOverrides)) {
      if (side && existing === role && id !== championId && sidePicks.includes(id)) continue;
      next[id] = existing;
    }
    next[championId] = role;
    return { roleOverrides: next };
  }),

  clearRoleOverride: (championId) => set((state) => {
    if (!(championId in state.roleOverrides)) return state;
    const next = { ...state.roleOverrides };
    delete next[championId];
    return { roleOverrides: next };
  }),

  pushChampion: (championId, action, bansPerSide = usePreferencesStore.getState().bansPerSide, mode = usePreferencesStore.getState().mode) => {
    const { draft, history } = get();
    const reason = unavailableReason(championId, action, mode, draft, bansPerSide);
    if (reason) return false;
    const next = structuredClone(draft);
    actionTarget(next, action).push(championId);
    const rec = currentDraftAction(action, championId);
    if (rec) next.actionLog.push(rec);
    set({ history: [...history, draft], draft: next });
    return true;
  },

  removeChampion: (target, championId) => {
    const { draft, history, roleOverrides } = get();
    const next = { ...draft, [target]: (draft[target] as string[]).filter((id) => id !== championId) };
    if (target !== "historyBlue" && target !== "historyRed") next.actionLog = removeLatestAction(draft.actionLog, championId);
    // A champion leaving the board drops any confirmed role it held.
    const overrides = championId in roleOverrides ? { ...roleOverrides } : roleOverrides;
    if (overrides !== roleOverrides) delete overrides[championId];
    set({ history: [...history, draft], draft: next, roleOverrides: overrides });
  },

  undo: () => {
    const { history } = get();
    const prev = history.at(-1);
    if (!prev) return;
    set({ draft: prev, history: history.slice(0, -1) });
  },

  resetCurrent: () => {
    const { draft, history } = get();
    set({ history: [...history, draft], draft: { ...draft, blueBans: [], redBans: [], bluePicks: [], redPicks: [], actionLog: [] }, roleOverrides: {} });
  },

  saveCurrentGame: () => {
    const { draft, currentGame, completedGames } = get();
    const record: GameRecord = { gameNumber: currentGame, bluePicks: [...draft.bluePicks], redPicks: [...draft.redPicks] };
    set({ completedGames: [...completedGames.filter((g) => g.gameNumber !== currentGame), record].sort((a, b) => a.gameNumber - b.gameNumber) });
  },

  moveToGame: (gameNum) => {
    get().saveCurrentGame();
    const { draft, history } = get();
    set({ currentGame: gameNum, history: [...history, draft], draft: { ...draft, blueBans: [], redBans: [], bluePicks: [], redPicks: [], actionLog: [] }, roleOverrides: {} });
  },

  finishGame: () => {
    get().saveCurrentGame();
    if (get().currentGame < 5) get().moveToGame(get().currentGame + 1);
  },

  resetSeries: () => {
    const { draft, history } = get();
    set({
      history: [...history, draft],
      completedGames: [],
      currentGame: 1,
      draft: { ...draft, blueBans: [], redBans: [], bluePicks: [], redPicks: [], historyBlue: [], historyRed: [], actionLog: [] },
      roleOverrides: {},
    });
  },

  clearSeriesProgress: () => set({ currentGame: 1, completedGames: [] }),

  setSeriesHistory: (blue, red) => set((state) => ({ draft: { ...state.draft, historyBlue: blue, historyRed: red } })),

  syncLiveSeries: (matchId, setNumber, completedGames) => set((state) => {
    if (setNumber < 1) return state;
    if (state.liveMatchId !== matchId) {
      return {
        liveMatchId: matchId,
        draft: structuredClone(emptyDraft),
        history: [],
        currentGame: setNumber,
        completedGames,
        roleOverrides: {},
      };
    }
    if (state.currentGame === setNumber) {
      return state.completedGames === completedGames ? state : { completedGames };
    }

    return {
      draft: structuredClone(emptyDraft),
      history: [],
      currentGame: setNumber,
      completedGames,
      roleOverrides: {},
    };
  }),

  applyBridgeUpdate: (lists) => set((state) => ({
    draft: { ...state.draft, blueBans: lists.blueBans, redBans: lists.redBans, bluePicks: lists.bluePicks, redPicks: lists.redPicks },
  })),
}));
