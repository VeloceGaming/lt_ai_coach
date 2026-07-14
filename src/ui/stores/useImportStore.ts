// Import/stats drawer — the imported database and everything derived from it:
// backend status, the last import summary, champion statistics, coach state,
// status messages, and manual tier overrides. The async backend calls (startup
// load, import, prepare coach, set tier) live here too; their logic is copied
// verbatim from the old App.tsx so behavior is unchanged.

import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { AppStatus, AthleteChampionLookup, AthleteDetail, AthleteSummary, ChampionRoleStat, DraftCatalog, DraftChampion, ImportSummary, PatchHistoryEntry, RoleStatistics } from "../types";
import { formatAge } from "../lib/format";
import { resolveMissingPortraits } from "../lib/portraits";
import { translateNow, useI18nStore } from "./useI18nStore";

type CoachState = "paused" | "preparing" | "ready";
type PortraitProbeSummary = { generated: number; skipped: number; failed: number; outputPath: string };

type ImportState = {
  status: AppStatus | null;
  summary: ImportSummary | null;
  statistics: RoleStatistics | null;
  draftCatalog: DraftCatalog | null;
  athletes: AthleteSummary[];
  patchHistory: PatchHistoryEntry[];
  busy: boolean;
  coachState: CoachState;
  message: string | null;
  tiers: Record<string, string>;
  initialize: () => void;
  setChampionTier: (championId: string, tier: string) => Promise<void>;
  prepareCoach: () => Promise<void>;
  runGameImport: () => Promise<boolean>;
  repairMissingPortraits: () => Promise<void>;
  refreshFromGameAndPrepare: () => Promise<void>;
  loadAthleteDetail: (athleteId: number) => Promise<AthleteDetail | null>;
  lookupAthleteMastery: (athleteId: number, championId: string) => Promise<AthleteChampionLookup | null>;
  mergeDraftChampions: (champions: DraftChampion[]) => void;
  setChampionOverride: (championId: string, name: string, portraitPath: string, nameChanged: boolean, portraitPathChanged: boolean) => Promise<void>;
};

export const useImportStore = create<ImportState>((set, get) => {
  // Fill in sprites the bundled catalog is missing (e.g. newly modded champions)
  // by probing the served sprite folders by champion id, then patch the resolved
  // art into the catalog + statistics so every screen picks it up at once.
  const enrichPortraits = async () => {
    const { draftCatalog, statistics } = get();
    const missing = new Set<string>();
    draftCatalog?.champions.forEach((champion) => { if (!champion.portrait) missing.add(champion.id); });
    statistics?.overallRows.forEach((row) => { if (!row.portrait) missing.add(row.championId); });
    if (missing.size === 0) return;
    const resolved = await resolveMissingPortraits([...missing]);
    if (resolved.size === 0) return;
    const patchRows = (rows: ChampionRoleStat[]) => rows.map((row) => (row.portrait || !resolved.has(row.championId) ? row : { ...row, portrait: resolved.get(row.championId)! }));
    set((state) => ({
      draftCatalog: state.draftCatalog
        ? { ...state.draftCatalog, champions: state.draftCatalog.champions.map((champion) => (champion.portrait || !resolved.has(champion.id) ? champion : { ...champion, portrait: resolved.get(champion.id)! })) }
        : state.draftCatalog,
      statistics: state.statistics
        ? { ...state.statistics, overallRows: patchRows(state.statistics.overallRows), roleRows: patchRows(state.statistics.roleRows) }
        : state.statistics,
    }));
  };

  return ({
  status: null,
  summary: null,
  statistics: null,
  draftCatalog: null,
  athletes: [],
  patchHistory: [],
  busy: false,
  coachState: "paused",
  message: null,
  tiers: {},

  initialize: () => {
    invoke<AppStatus>("get_app_status").then((status) => set({ status })).catch(() => set({ status: null }));
    invoke<DraftCatalog>("get_draft_catalog").then((draftCatalog) => { set({ draftCatalog }); void enrichPortraits(); }).catch(() => set({ draftCatalog: null }));
    invoke<AthleteSummary[]>("get_athletes").then((athletes) => set({ athletes })).catch(() => set({ athletes: [] }));
    invoke<PatchHistoryEntry[]>("get_patch_history").then((patchHistory) => set({ patchHistory })).catch(() => set({ patchHistory: [] }));
    invoke<Record<string, string>>("get_manual_tiers").then((tiers) => set({ tiers })).catch(() => set({ tiers: {} }));
  },

  // Set or clear a champion's manual tier (empty string clears). The backend
  // invalidates the recommendation cache, so the next get_recommendations call
  // reflects the change automatically.
  setChampionTier: async (championId, tier) => {
    try {
      await invoke("set_champion_tier", { championId, tier: tier || null });
      const next = { ...get().tiers };
      if (tier) next[championId] = tier;
      else delete next[championId];
      set({ tiers: next });
    } catch (e) {
      set({ message: e instanceof Error ? e.message : String(e) });
    }
  },

  prepareCoach: async () => {
    set({ coachState: "preparing", message: "Preparing recommendation cache..." });
    try {
      const preparedStatistics = await invoke<RoleStatistics>("prepare_recommendation_cache");
      set({ statistics: preparedStatistics, coachState: "ready", message: "Coach ready. Live recommendations are fully cached." });
      void enrichPortraits();
    } catch (e) {
      set({ coachState: "paused", message: e instanceof Error ? e.message : String(e) });
    }
  },

  runGameImport: async () => {
    set({ busy: true, message: "Importing from game..." });
    try {
      const result = await invoke<ImportSummary>("import_from_game_export");
      const draftCatalog = await invoke<DraftCatalog>("get_draft_catalog");
      const athletes = await invoke<AthleteSummary[]>("get_athletes").catch(() => [] as AthleteSummary[]);
      const patchHistory = await invoke<PatchHistoryEntry[]>("get_patch_history").catch(() => [] as PatchHistoryEntry[]);
      set((state) => ({
        summary: result,
        draftCatalog,
        athletes,
        patchHistory,
        statistics: null,
        status: state.status ? { ...state.status, databaseReady: true } : state.status,
        coachState: "paused",
      }));
      const from = result.gameLabel ? ` from ${result.gameLabel}` : "";
      const ageSeconds = result.exportedAtUnix ? Math.max(0, Date.now() / 1000 - result.exportedAtUnix) : null;
      const stale = ageSeconds !== null && ageSeconds > 120
        ? ` ⚠ This export is ${formatAge(ageSeconds)} old — load the save you want in-game and wait a few seconds for a fresh export, then re-import.`
        : "";
      set({
        message:
          `Imported${from}: ${result.teams} teams, ${result.players} players, ` +
          `${result.matches} matches, ${result.picks} picks.${stale}`,
      });
      void enrichPortraits();
      return true;
    } catch (e) {
      set({ message: e instanceof Error ? e.message : String(e) });
      return false;
    } finally {
      set({ busy: false });
    }
  },

  repairMissingPortraits: async () => {
    set({ busy: true, message: translateNow("app.portraitRepair.running") });
    try {
      const languageId = useI18nStore.getState().languageId;
      const result = await invoke<PortraitProbeSummary>("probe_game_portraits", { languageId });
      const draftCatalog = await invoke<DraftCatalog>("get_draft_catalog");
      const repaired = new Map(draftCatalog.champions.filter((champion) => champion.portrait).map((champion) => [champion.id, champion.portrait!]));
      const patchRows = (rows: ChampionRoleStat[]) => rows.map((row) => repaired.has(row.championId) ? { ...row, portrait: repaired.get(row.championId)! } : row);
      const resultKey = result.failed ? "app.portraitRepair.resultWithFailures" : "app.portraitRepair.result";
      const resultMessage = translateNow(resultKey, {
        generated: result.generated,
        cached: result.skipped,
        failed: result.failed,
      });
      set((state) => ({
        draftCatalog,
        statistics: state.statistics ? {
          ...state.statistics,
          overallRows: patchRows(state.statistics.overallRows),
          roleRows: patchRows(state.statistics.roleRows),
        } : state.statistics,
        message: resultMessage,
      }));
      const language = useI18nStore.getState().languageId;
      useI18nStore.getState().setLanguage(language);
    } catch (e) {
      set({ message: e instanceof Error ? e.message : String(e) });
    } finally {
      set({ busy: false });
    }
  },

  refreshFromGameAndPrepare: async () => {
    const { busy, coachState } = get();
    if (busy || coachState === "preparing") return;
    const imported = await get().runGameImport();
    if (imported) await get().prepareCoach();
  },

  loadAthleteDetail: async (athleteId) => {
    return invoke<AthleteDetail | null>("get_athlete_detail", { athleteId });
  },

  lookupAthleteMastery: async (athleteId, championId) => {
    return invoke<AthleteChampionLookup | null>("get_athlete_mastery", { athleteId, championId });
  },

  mergeDraftChampions: (champions) => {
    if (!champions.length) return;
    set((state) => {
      if (!state.draftCatalog) return state;
      let changed = false;
      const byId = new Map(state.draftCatalog.champions.map((champion) => [champion.id, champion]));
      for (const incoming of champions) {
        const existing = byId.get(incoming.id);
        if (!existing) {
          byId.set(incoming.id, incoming);
          changed = true;
          continue;
        }
        const roleFit = existing.roleFit && Object.keys(existing.roleFit).length > 0
          ? existing.roleFit
          : incoming.roleFit;
        const merged = {
          ...existing,
          name: existing.name || incoming.name,
          portrait: existing.portrait ?? incoming.portrait,
          roleFit,
        };
        if (merged !== existing && (merged.portrait !== existing.portrait || merged.roleFit !== existing.roleFit || merged.name !== existing.name)) {
          byId.set(incoming.id, merged);
          changed = true;
        }
      }
      if (!changed) return state;
      return { draftCatalog: { champions: [...byId.values()].sort((left, right) => left.id.localeCompare(right.id)) } };
    });
  },

  setChampionOverride: async (championId, name, portraitPath, nameChanged, portraitPathChanged) => {
    try {
      const champion = await invoke<DraftChampion>("set_champion_override", {
        championId,
        name: name || null,
        portraitPath: portraitPath || null,
        nameChanged,
        portraitPathChanged,
      });
      set((state) => {
        if (!state.draftCatalog) return state;
        const champions = state.draftCatalog.champions.map((current) => current.id === champion.id ? champion : current);
        if (!champions.some((current) => current.id === champion.id)) champions.push(champion);
        return { draftCatalog: { champions: champions.sort((left, right) => left.id.localeCompare(right.id)) } };
      });
    } catch (e) {
      set({ message: e instanceof Error ? e.message : String(e) });
    }
  },
  });
});
