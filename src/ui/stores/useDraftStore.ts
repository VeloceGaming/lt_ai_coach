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
import type { BridgeState, DraftAction, DraftMode, DraftState, GameRecord, ShadowLists } from "../types";
import { actionTarget, currentDraftAction, emptyDraft, emptyShadow, mergeShadow, removeLatestAction, unavailableReason } from "../lib/draft";
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
  // Shadow (hypothetical) champions staged while the bridge owns the real
  // draft. A separate layer: the real board is never mutated, and a real
  // update silently evicts any shadow it collides with.
  shadow: ShadowLists;
  pushShadowChampion: (championId: string, action: DraftAction, bansPerSide?: number, mode?: DraftMode) => boolean;
  removeShadowChampion: (target: keyof ShadowLists, championId: string) => void;
  clearShadows: () => void;
  // Shadows just evicted by a real bridge update, with the list they lived
  // in, so the board can play a dissolve at the ghost's old spot and flash
  // the real slot that replaced it. `stamp` makes each batch distinct.
  shadowEvictions: { entries: { championId: string; target: keyof ShadowLists }[]; stamp: number } | null;
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

  shadow: emptyShadow,
  shadowEvictions: null,

  pushShadowChampion: (championId, action, bansPerSide = usePreferencesStore.getState().bansPerSide, mode = usePreferencesStore.getState().mode) => {
    // Only board slots can hold shadows; series-history entries stay real.
    const target: keyof ShadowLists | null =
      action === "blue-ban" ? "blueBans" : action === "red-ban" ? "redBans"
        : action === "blue-pick" ? "bluePicks" : action === "red-pick" ? "redPicks" : null;
    if (!target) return false;
    const { draft, shadow } = get();
    // Availability is judged against the board as imagined (real + shadows),
    // so a champion can't be shadowed twice or overflow a full slot row.
    const reason = unavailableReason(championId, action, mode, mergeShadow(draft, shadow, bansPerSide), bansPerSide);
    if (reason) return false;
    set({ shadow: { ...shadow, [target]: [...shadow[target], championId] } });
    return true;
  },

  removeShadowChampion: (target, championId) => set((state) => ({
    shadow: { ...state.shadow, [target]: state.shadow[target].filter((id) => id !== championId) },
  })),

  clearShadows: () => set({ shadow: emptyShadow }),

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
    set({ history: [...history, draft], draft: { ...draft, blueBans: [], redBans: [], bluePicks: [], redPicks: [], actionLog: [] }, roleOverrides: {}, shadow: emptyShadow });
  },

  saveCurrentGame: () => {
    const { draft, currentGame, completedGames } = get();
    const record: GameRecord = { gameNumber: currentGame, bluePicks: [...draft.bluePicks], redPicks: [...draft.redPicks] };
    set({ completedGames: [...completedGames.filter((g) => g.gameNumber !== currentGame), record].sort((a, b) => a.gameNumber - b.gameNumber) });
  },

  moveToGame: (gameNum) => {
    get().saveCurrentGame();
    const { draft, history } = get();
    set({ currentGame: gameNum, history: [...history, draft], draft: { ...draft, blueBans: [], redBans: [], bluePicks: [], redPicks: [], actionLog: [] }, roleOverrides: {}, shadow: emptyShadow });
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
      shadow: emptyShadow,
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
        shadow: emptyShadow,
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
      shadow: emptyShadow,
    };
  }),

  applyBridgeUpdate: (lists) => set((state) => {
    // A real update wins over shadows in three ways:
    //  - collision: the shadowed champion itself landed anywhere on the real
    //    board — evicted (cross-list plays a dissolve, same-list a flash);
    //  - overwrite: a new real entry lands in the slot a shadow was visually
    //    holding — that shadow is consumed and the real slot flashes, rather
    //    than the shadow sliding down a slot;
    //  - overflow: the row filled up — excess shadows leave silently.
    const used = new Set([...lists.blueBans, ...lists.redBans, ...lists.bluePicks, ...lists.redPicks]);
    const targets: (keyof ShadowLists)[] = ["blueBans", "redBans", "bluePicks", "redPicks"];
    const shadow = { ...state.shadow };
    const evicted: Array<{ championId: string; target: keyof ShadowLists }> = [];
    for (const target of targets) {
      const before = state.shadow[target];
      const oldReal = state.draft[target];
      const newReal = lists[target];
      for (const championId of before.filter((id) => used.has(id))) evicted.push({ championId, target });
      let remaining = before.filter((id) => !used.has(id));
      // Each new real entry that was not this list's own shadow consumes the
      // front-most remaining shadow — the one visually holding its slot. The
      // eviction records the real champion so the flash lands on that slot.
      for (const realId of newReal.filter((id) => !oldReal.includes(id))) {
        if (before.includes(realId) || remaining.length === 0) continue;
        remaining = remaining.slice(1);
        evicted.push({ championId: realId, target });
      }
      shadow[target] = remaining.slice(0, Math.max(0, 5 - newReal.length));
    }
    return {
      draft: { ...state.draft, blueBans: lists.blueBans, redBans: lists.redBans, bluePicks: lists.bluePicks, redPicks: lists.redPicks },
      shadow,
      shadowEvictions: evicted.length ? { entries: evicted, stamp: Date.now() } : state.shadowEvictions,
    };
  }),
}));
