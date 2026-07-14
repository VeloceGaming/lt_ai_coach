// Import/stats drawer — the imported database and everything derived from it:
// backend status, the last import summary, champion statistics, coach state,
// status messages, and manual tier overrides. The async backend calls (startup
// load, import, prepare coach, set tier) live here too; their logic is copied
// verbatim from the old App.tsx so behavior is unchanged.

import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { AppStatus, AthleteChampionLookup, AthleteDetail, AthleteSummary, ChampionRoleStat, DraftCatalog, DraftChampion, ImportSummary, PatchChange, PatchHistoryEntry, RoleStatistics } from "../types";
import { formatAge, titleCase } from "../lib/format";
import { resolveMissingPortraits } from "../lib/portraits";
import { translateNow, useI18nStore } from "./useI18nStore";
import championCatalog from "../../../data/catalog/champions.json";

const previewChampions = championCatalog.champions.map((champion) => ({
  id: champion.id,
  name: titleCase(champion.id.replaceAll("_", " ")),
  portrait: champion.asset ? {
    path: `/${champion.asset.sheet.replace(/^assets[\\/]/, "").replaceAll("\\", "/")}`,
    sheetWidth: champion.asset.sheetWidth,
    sheetHeight: champion.asset.sheetHeight,
    x: champion.asset.frame.x,
    y: champion.asset.frame.y,
    width: champion.asset.frame.w,
    height: champion.asset.frame.h,
  } : null,
  roleFit: champion.roleFit,
}));

const fallbackStatus: AppStatus = { backend: "Browser preview", phase: "Preview data", catalogChampions: previewChampions.length, databaseReady: true };
const fallbackDraftCatalog: DraftCatalog = {
  champions: previewChampions.map(({ id, name, portrait, roleFit }) => ({ id, name, portrait, roleFit })),
};

const fallbackAthletes: AthleteSummary[] = [
  { id: 9001, name: "Han Seojun", teamId: 101, teamName: "Northstar", strongestRole: "mid" },
  { id: 9002, name: "Mira Chen", teamId: 101, teamName: "Northstar", strongestRole: "jungle" },
  { id: 9003, name: "Elias Park", teamId: 102, teamName: "Red Harbor", strongestRole: "bottom" },
  { id: 9004, name: "Noa Laurent", teamId: 102, teamName: "Red Harbor", strongestRole: "support" },
  { id: 9005, name: "Dae Kim", teamId: 103, teamName: "Axiom", strongestRole: "top" },
  { id: 9006, name: "Sora Vale", teamId: null, teamName: null, strongestRole: "mid" },
];

function fallbackAthleteDetail(athleteId: number): AthleteDetail | null {
  const athlete = fallbackAthletes.find((candidate) => candidate.id === athleteId);
  if (!athlete) return null;
  const offset = athleteId % 7;
  const core = {
    lastHit: 78 + offset, skillAvoid: 72 + offset, skillHit: 82 + offset,
    positioning: 76 + offset, controlSpeed: 84 + offset, concentration: 70 + offset,
    mental: 68 + offset, judgement: 80 + offset,
  };
  const masteryValues = [1000, 940, 880, 820, 780, 730, 690, 610, 540, 460, 380, 300, 240, 180, 120, 70];
  const masteryChampionIds = [
    "crossbowman", "nightmare", "sand_mage",
    ...previewChampions.map((champion) => champion.id),
  ];
  return {
    ...athlete,
    stats: {
      core,
      tendencies: { shotcalling: 67 + offset, roaming: 58 + offset, aggressive: 71 - offset, ego: 43 + offset },
      roles: {
        top: athlete.strongestRole === "top" ? 92 : 28 + offset,
        jungle: athlete.strongestRole === "jungle" ? 94 : 31 + offset,
        mid: athlete.strongestRole === "mid" ? 95 : 36 + offset,
        bottom: athlete.strongestRole === "bottom" ? 93 : 30 + offset,
        support: athlete.strongestRole === "support" ? 91 : 34 + offset,
      },
    },
    masteries: masteryValues.map((valueRaw, index) => ({
      championId: masteryChampionIds[(index + offset) % masteryChampionIds.length] ?? "fighter",
      floorRaw: Math.max(0, valueRaw - 420),
      valueRaw,
      mastery: valueRaw / 10,
      statBuff: valueRaw >= 1000 ? 0.20 : valueRaw >= 900 ? 0.15 : valueRaw >= 800 ? 0.10 : valueRaw >= 700 ? 0.05 : 0,
      recent: index === 2 || index === 5,
    })),
  };
}

const previewTierBaselines = [0.565, 0.535, 0.505, 0.475, 0.445, 0.415];
const fallbackStatistics: RoleStatistics = {
  databasePath: "Browser preview",
  totalMatches: 284,
  currentPatch: "14.12",
  globalWinRate: 0.5,
  priorGames: 218,
  reliableGames: 246,
  overallRows: previewChampions.map((champion, index) => {
    const delta = index % 4 === 0 ? 0.024 : index % 5 === 0 ? -0.012 : 0;
    const tierIndex = Math.min(previewTierBaselines.length - 1, Math.floor(index / 12));
    const positionInTier = index % 12;
    const projected = previewTierBaselines[tierIndex] + (5.5 - positionInTier) * 0.0012;
    const role = Object.entries(champion.roleFit).sort(([, a], [, b]) => b - a)[0]?.[0] ?? "mid";
    const patches = ["14.09", "14.10", "14.11", "14.12"];
    const patchTimeline = patches.map((patch, patchIndex) => {
      const trend = (patchIndex - 1.5) * 0.006 + delta * (patchIndex / Math.max(1, patches.length - 1));
      return {
        patch,
        games: 8 + patchIndex * 3 + (index % 4),
        wins: 4 + patchIndex + (index % 3),
        winRate: Math.max(0.35, Math.min(0.68, projected - 0.012 + trend)),
        avgKills: 3.6 + patchIndex * 0.2,
        avgDeaths: 3.4 - patchIndex * 0.08,
        avgAssists: 5.8 + patchIndex * 0.25,
        kda: 3.1 + patchIndex * 0.12,
        avgDamage: 17000 + patchIndex * 650 + index * 12,
        avgTanking: 8500 + patchIndex * 290,
        avgHealing: 900 + patchIndex * 120,
        avgCs: 132 + patchIndex * 5,
        avgGold: 9800 + patchIndex * 420,
        avgRating: 6.7 + patchIndex * 0.12,
      };
    });
    return {
      championId: champion.id, championName: champion.name, role, portrait: champion.portrait,
      games: 52 - index, currentPatchGames: 12 + (index % 9), effectiveGames: 44 - index / 2,
      patchChanged: delta !== 0, patchAdded: false, patchImpact: delta * 1000,
      patchChanges: delta === 0 ? [] : [{ patch: "14.12", championId: champion.id, asset: index % 2 ? "basic_attack" : "skill_q", target: null, field: index % 2 ? "attack_speed_ratio" : "damage", oldValue: index % 2 ? 1 : 210, newValue: index % 2 ? 0.92 : 245, impact: delta > 0 ? 8 : -8 }],
      wins: 25, tournamentGames: 32, soloGames: 20, winRate: projected - 0.006,
      adjustedWinRate: projected - 0.003,
      pilotWinRateDelta: 0, confidence: 0.72, avgKills: 4.2, avgDeaths: 3.1,
      avgAssists: 6.4, kda: 3.42, avgDamage: 18420, avgTanking: 9200,
      avgHealing: 1100, avgCs: 146, avgGold: 10800, avgRating: 7.1,
      patchTimeline,
    };
  }),
  roleRows: [],
};
fallbackStatistics.roleRows = fallbackStatistics.overallRows.map((row) => ({ ...row }));

// Sample multi-patch history for browser preview, so the Patch Notes timeline is
// testable before a real save spans several patches. Uses real preview champion
// ids so names/portraits resolve. Patches span all three sizes (major/medium/minor).
const previewId = (index: number) => previewChampions[index % previewChampions.length]?.id ?? "champion";
const previewChange = (patch: string, index: number, field: string, oldValue: number, newValue: number, impact: number): PatchChange => ({
  patch, championId: previewId(index), asset: index % 2 ? "skill_q" : "basic_attack", target: null, field, oldValue, newValue, impact,
});
const fallbackPatchHistory: PatchHistoryEntry[] = [
  { patch: "2027.0.0", changes: [{ championId: previewId(1), changes: [previewChange("2027.0.0", 1, "damage", 210, 250, 9)] }, { championId: previewId(2), changes: [previewChange("2027.0.0", 2, "cooldown", 9, 11, -7)] }], additions: [previewId(40), previewId(41)] },
  { patch: "2026.2.1", changes: [{ championId: previewId(3), changes: [previewChange("2026.2.1", 3, "attack_speed_ratio", 1, 0.93, -4)] }], additions: [] },
  { patch: "2026.2.0", changes: [{ championId: previewId(4), changes: [previewChange("2026.2.0", 4, "damage", 80, 95, 6)] }, { championId: previewId(5), changes: [previewChange("2026.2.0", 5, "hp", 900, 860, -5)] }], additions: [previewId(42)] },
  { patch: "2026.1.0", changes: [{ championId: previewId(6), changes: [previewChange("2026.1.0", 6, "magic_power", 55, 65, 7)] }], additions: [] },
  { patch: "2026.0.0", changes: [{ championId: previewId(7), changes: [previewChange("2026.0.0", 7, "range", 12, 10, -6)] }], additions: [] },
];

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
    invoke<AppStatus>("get_app_status").then((status) => set({ status })).catch(() => set({ status: fallbackStatus }));
    invoke<DraftCatalog>("get_draft_catalog").then((draftCatalog) => { set({ draftCatalog }); void enrichPortraits(); }).catch(() => { set({ draftCatalog: fallbackDraftCatalog }); void enrichPortraits(); });
    invoke<AthleteSummary[]>("get_athletes").then((athletes) => set({ athletes })).catch(() => set({ athletes: "__TAURI_INTERNALS__" in window ? [] : fallbackAthletes }));
    invoke<PatchHistoryEntry[]>("get_patch_history").then((patchHistory) => set({ patchHistory })).catch(() => set({ patchHistory: fallbackPatchHistory }));
    invoke<Record<string, string>>("get_manual_tiers").then((tiers) => set({ tiers })).catch(() => set({ tiers: {} }));
  },

  // Set or clear a champion's manual tier (empty string clears). The backend
  // invalidates the recommendation cache, so the next get_recommendations call
  // reflects the change automatically.
  setChampionTier: async (championId, tier) => {
    if (!("__TAURI_INTERNALS__" in window)) {
      const next = { ...get().tiers };
      if (tier) next[championId] = tier;
      else delete next[championId];
      set({ tiers: next, message: `Preview tier ${tier || "cleared"} for ${titleCase(championId)}.` });
      return;
    }
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
      if (!("__TAURI_INTERNALS__" in window)) {
        set({ statistics: fallbackStatistics, coachState: "ready", message: "Coach ready with browser preview data." });
        void enrichPortraits();
      } else {
        set({ coachState: "paused", message: e instanceof Error ? e.message : String(e) });
      }
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
    if (!("__TAURI_INTERNALS__" in window)) return fallbackAthleteDetail(athleteId);
    return invoke<AthleteDetail | null>("get_athlete_detail", { athleteId });
  },

  lookupAthleteMastery: async (athleteId, championId) => {
    if (!("__TAURI_INTERNALS__" in window)) {
      const detail = fallbackAthleteDetail(athleteId);
      const mastery = detail?.masteries.find((entry) => entry.championId === championId);
      if (!detail?.stats || !mastery) return null;
      const multiplier = 1 + mastery.statBuff;
      const baseEntries = Object.entries(detail.stats.core);
      const effectiveCore = Object.fromEntries(
        baseEntries.map(([key, value]) => [key, Math.min(100, value * multiplier)]),
      ) as AthleteChampionLookup["effectiveCore"];
      const realizedGain = Object.fromEntries(
        baseEntries.map(([key, value]) => [key, effectiveCore[key as keyof typeof effectiveCore] - value]),
      ) as AthleteChampionLookup["realizedGain"];
      const average = (values: number[]) => values.reduce((sum, value) => sum + value, 0) / values.length;
      const baseCoreAverage = average(baseEntries.map(([, value]) => value));
      const effectiveCoreAverage = average(Object.values(effectiveCore));
      const realizedGainAverage = effectiveCoreAverage - baseCoreAverage;
      return {
        athleteId,
        championId,
        mastery: mastery.mastery,
        statBuff: mastery.statBuff,
        realizedStatBuff: baseCoreAverage > 0 ? realizedGainAverage / baseCoreAverage : 0,
        recent: mastery.recent,
        baseCore: detail.stats.core,
        effectiveCore,
        realizedGain,
        baseCoreAverage,
        effectiveCoreAverage,
        realizedGainAverage,
        cappedStats: Object.values(effectiveCore).filter((value) => value >= 100).length,
      };
    }
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
