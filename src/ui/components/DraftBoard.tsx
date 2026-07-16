// The draft board. In the main window it renders full mode (manual draft entry,
// live bridge feed, screenshot detection, recommendations, Fearless tracking).
// In the overlay window (compact prop) it renders the compact live-draft strip.
// The two run as separate windows, each with its own state — see overlayWindow.ts.

import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AthleteSummary, BridgeState, DraftAction, DraftCatalog, DraftChampion, DraftMode, DraftState, LiveRecommendationResponse, Recommendation, RecommendationShortlist, ShadowLists } from "../types";
import { activeTuning } from "../lib/preferences";
import { calculateDraftTurn, draftActionLabelKey, hasShadows, mergeShadow, translateRecommendationError, translateTurnLabel, translateTurnProgress, unavailableReason, WAITING_FOR_CONTEXT_MESSAGE } from "../lib/draft";
import { usePreferencesStore } from "../stores/usePreferencesStore";
import { useOverlayStore } from "../stores/useOverlayStore";
import { useDraftStore } from "../stores/useDraftStore";
import { useImportStore } from "../stores/useImportStore";
import { useT } from "../stores/useI18nStore";
import { previewRecommendations } from "../lib/preview";
import { resolveMissingPortraits } from "../lib/portraits";
import { closeOverlayWindow, showOverlayWindow } from "../lib/overlayWindow";
import { CompactDraftBar } from "./CompactDraftBar";
import { SeriesBar } from "./SeriesBar";
import { ChampionPortraitView } from "./ChampionPortraitView";
import { FullDraftSide } from "./FullDraftSide";
import { FullDraftRecommendations } from "./FullDraftRecommendations";
import { CompAnalysis } from "./CompAnalysis";
import { RoleGlyph } from "./RoleGlyph";
import { RolePickerPopover, type RolePickerState } from "./RolePickerPopover";
import { IconBrain, IconGhost2, IconLayoutGrid, IconMaximize, IconRefresh, IconSearch, IconX } from "@tabler/icons-react";

function fallbackChampionName(id: string) {
  return id.split("_").map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join(" ");
}

export function DraftBoard({ catalog, recommendationsEnabled, tiers, currentPatch, athletes = [], compact = false }: { catalog: DraftCatalog; recommendationsEnabled: boolean; tiers: Record<string, string>; currentPatch?: string; athletes?: AthleteSummary[]; compact?: boolean }) {
  const t = useT();
  const configuredMode = usePreferencesStore((s) => s.mode);
  const configuredBansPerSide = usePreferencesStore((s) => s.bansPerSide);
  const setMode = usePreferencesStore((s) => s.setMode);
  const autoOverlay = usePreferencesStore((s) => s.autoOverlay);
  const setAutoOverlay = usePreferencesStore((s) => s.setAutoOverlay);
  const [action, setAction] = useState<DraftAction>("blue-ban");
  const draft = useDraftStore((s) => s.draft);
  const history = useDraftStore((s) => s.history);
  const [slotSelected, setSlotSelected] = useState(false);
  const [poolSearch, setPoolSearch] = useState("");
  const [recommendationRole, setRecommendationRole] = useState("all");
  const [selectedRecommendationId, setSelectedRecommendationId] = useState<string | null>(null);
  const [rolePicker, setRolePicker] = useState<RolePickerState | null>(null);
  const [recommendations, setRecommendations] = useState<RecommendationShortlist | null>(null);
  const [recommendationError, setRecommendationError] = useState<string | null>(null);
  const weights = usePreferencesStore((s) => s.weights);
  const strategy = usePreferencesStore((s) => s.strategy);
  const customTuning = usePreferencesStore((s) => s.customTuning);
  const tuning = activeTuning(strategy, customTuning);
  const minimumInteractionGames = usePreferencesStore((s) => s.minimumInteractionGames);
  const currentGame = useDraftStore((s) => s.currentGame);
  const completedGames = useDraftStore((s) => s.completedGames);
  const roleOverrides = useDraftStore((s) => s.roleOverrides);
  const setRoleOverride = useDraftStore((s) => s.setRoleOverride);
  const clearRoleOverride = useDraftStore((s) => s.clearRoleOverride);
  const pushChampion = useDraftStore((s) => s.pushChampion);
  const removeChampion = useDraftStore((s) => s.removeChampion);
  const shadow = useDraftStore((s) => s.shadow);
  const pushShadowChampion = useDraftStore((s) => s.pushShadowChampion);
  const removeShadowChampion = useDraftStore((s) => s.removeShadowChampion);
  const clearShadows = useDraftStore((s) => s.clearShadows);
  const shadowEvictions = useDraftStore((s) => s.shadowEvictions);
  // Champion ids whose real slot briefly flashes because it just replaced an
  // evicted shadow, and evicted ghosts still playing their dissolve at their
  // old spot. Both are transient and cleared once the animations finish.
  const [evictionFlash, setEvictionFlash] = useState<Set<string>>(new Set());
  const [dissolving, setDissolving] = useState<Array<{ championId: string; target: keyof ShadowLists }>>([]);
  const undo = useDraftStore((s) => s.undo);
  const resetCurrent = useDraftStore((s) => s.resetCurrent);
  const moveToGame = useDraftStore((s) => s.moveToGame);
  const finishGame = useDraftStore((s) => s.finishGame);
  const resetSeries = useDraftStore((s) => s.resetSeries);
  const clearSeriesProgress = useDraftStore((s) => s.clearSeriesProgress);
  const setSeriesHistory = useDraftStore((s) => s.setSeriesHistory);
  const syncLiveSeries = useDraftStore((s) => s.syncLiveSeries);
  const applyBridgeUpdate = useDraftStore((s) => s.applyBridgeUpdate);
  const mergeDraftChampions = useImportStore((s) => s.mergeDraftChampions);
  const [runtimePortraits, setRuntimePortraits] = useState(new Map<string, DraftChampion["portrait"]>());
  const bridgeConnected = useOverlayStore((s) => s.bridgeConnected);
  const setBridgeConnected = useOverlayStore((s) => s.setBridgeConnected);
  const setChampionTags = useOverlayStore((s) => s.setChampionTags);
  const userSide = useOverlayStore((s) => s.userSide);
  const blueLineup = useOverlayStore((s) => s.blueLineup);
  const redLineup = useOverlayStore((s) => s.redLineup);
  const setBridgeContext = useOverlayStore((s) => s.setBridgeContext);
  const liveRevision = useOverlayStore((s) => s.liveRevision);
  const liveContextRevision = useOverlayStore((s) => s.liveContextRevision);
  const liveBansPerSide = useOverlayStore((s) => s.bansPerSide);
  const liveDraftMode = useOverlayStore((s) => s.draftMode);
  const setLiveRevision = useOverlayStore((s) => s.setLiveRevision);
  const lastBridgeRevision = useRef(-1);
  const lastBridgeContextRevision = useRef(-1);
  const poolSearchRef = useRef<HTMLInputElement>(null);
  const mode = bridgeConnected ? liveDraftMode ?? configuredMode : configuredMode;
  const isFearless = mode !== "normal";
  const bansPerSide = bridgeConnected ? liveBansPerSide ?? configuredBansPerSide : configuredBansPerSide;
  // The board as imagined: real draft plus any shadow (hypothetical) champions.
  // Everything the coach column reasons about — turn, recommendations,
  // consultation, comp analysis, slot rendering — uses this; bridge sync and
  // undo history keep operating on the real draft only.
  const shadowActive = hasShadows(shadow);
  const shadowCount = shadow.blueBans.length + shadow.redBans.length + shadow.bluePicks.length + shadow.redPicks.length;
  const effectiveDraft = useMemo(() => (shadowActive ? mergeShadow(draft, shadow, bansPerSide) : draft), [shadowActive, draft, shadow, bansPerSide]);
  const shadowSets = useMemo(() => ({
    blueBans: new Set(shadow.blueBans),
    redBans: new Set(shadow.redBans),
    bluePicks: new Set(shadow.bluePicks),
    redPicks: new Set(shadow.redPicks),
  }), [shadow]);
  // What the board actually renders: the imagined draft, plus any just-evicted
  // ghost still playing its dissolve at the end of its old list. A ghost whose
  // champion landed in the SAME list solidifies in place (the real slot's
  // flash covers it), so only cross-list evictions linger — which also means
  // a rendered id is never both a real entry and a ghost within one list.
  // Scoring, turn order, and availability all keep using effectiveDraft.
  const board = useMemo(() => {
    const banLimit = Math.max(1, Math.min(5, Math.round(bansPerSide)));
    const caps = { blueBans: banLimit, redBans: banLimit, bluePicks: 5, redPicks: 5 };
    const lists = { blueBans: effectiveDraft.blueBans, redBans: effectiveDraft.redBans, bluePicks: effectiveDraft.bluePicks, redPicks: effectiveDraft.redPicks };
    const shadowIds = { ...shadowSets };
    const dissolvingIds = { blueBans: new Set<string>(), redBans: new Set<string>(), bluePicks: new Set<string>(), redPicks: new Set<string>() };
    for (const { championId, target } of dissolving) {
      if (lists[target].includes(championId) || lists[target].length >= caps[target]) continue;
      lists[target] = [...lists[target], championId];
      shadowIds[target] = new Set(shadowIds[target]).add(championId);
      dissolvingIds[target].add(championId);
    }
    return { lists, shadowIds, dissolvingIds };
  }, [dissolving, effectiveDraft, shadowSets, bansPerSide]);
  const turn = calculateDraftTurn(effectiveDraft, bansPerSide);
  const side = userSide ?? (!("__TAURI_INTERNALS__" in window) ? "blue" : null);
  const recommendationSide = side;
  // boardActiveAction: armed slot when user clicked one manually; otherwise the
  // natural next action from the turn tracker; null when draft is complete.
  const boardActiveAction: DraftAction | null = slotSelected
    ? action
    : turn.side && turn.phase !== "complete" ? `${turn.side}-${turn.phase}` as DraftAction : null;
  // The slot we assign to when applying a champion from search or rec cards.
  const targetAction = boardActiveAction ?? action;

  const draftChampionIds = useMemo(() => {
    return [...new Set([
      ...draft.blueBans,
      ...draft.redBans,
      ...draft.bluePicks,
      ...draft.redPicks,
      ...draft.historyBlue,
      ...draft.historyRed,
      ...shadow.blueBans,
      ...shadow.redBans,
      ...shadow.bluePicks,
      ...shadow.redPicks,
    ])];
  }, [draft, shadow]);
  const catalogChampions = useMemo(() => new Map(catalog.champions.map((c) => [c.id, c])), [catalog.champions]);
  // Athlete id -> name, for showing the live player on each pick slot.
  const athleteNames = useMemo(() => new Map(athletes.map((a) => [a.id, a.name])), [athletes]);
  const champions = useMemo(() => {
    const map = new Map(catalog.champions.map((c) => [c.id, c]));
    for (const id of draftChampionIds) {
      const existing = map.get(id);
      const portrait = runtimePortraits.get(id) ?? null;
      if (existing) {
        if (!existing.portrait && portrait) map.set(id, { ...existing, portrait });
      } else {
        map.set(id, { id, name: fallbackChampionName(id), portrait, roleFit: {} });
      }
    }
    return map;
  }, [catalog.champions, draftChampionIds, runtimePortraits]);

  useEffect(() => {
    const missing = draftChampionIds.filter((id) => !catalogChampions.get(id)?.portrait && !runtimePortraits.has(id));
    if (missing.length === 0) return;
    let active = true;
    resolveMissingPortraits(missing).then((resolved) => {
      if (!active || resolved.size === 0) return;
      setRuntimePortraits((current) => {
        const next = new Map(current);
        resolved.forEach((portrait, id) => next.set(id, portrait));
        return next;
      });
    });
    return () => { active = false; };
  }, [catalogChampions, draftChampionIds, runtimePortraits]);

  // Recommendations come from a separate backend call whose portraits aren't run
  // through the catalog's auto-discovery, so modded champions (e.g. Gragas) come
  // back with no art. Fall back to the enriched catalog portrait by champion id.
  const enrichedRecommendations = useMemo(() => {
    if (!recommendations) return recommendations;
    const fill = (rows: Recommendation[]) => rows.map((row) => (row.portrait ? row : { ...row, portrait: champions.get(row.championId)?.portrait ?? null }));
    return {
      ...recommendations,
      pickRecommendations: fill(recommendations.pickRecommendations),
      banRecommendations: fill(recommendations.banRecommendations),
      pickPool: fill(recommendations.pickPool ?? []),
      banPool: fill(recommendations.banPool ?? []),
    };
  }, [recommendations, champions]);

  // Inline pool: only populated when the user is typing in the search box.
  const filteredPool = useMemo(() => {
    const q = poolSearch.trim().toLowerCase();
    if (!q) return [];
    return catalog.champions.filter((c) => {
      if (!c.name.toLowerCase().includes(q) && !c.id.includes(q)) return false;
      if (recommendationRole !== "all" && c.roleFit) {
        const topRole = Object.entries(c.roleFit).sort(([, a], [, b]) => b - a)[0]?.[0]?.toLowerCase();
        if (topRole !== recommendationRole) return false;
      }
      return true;
    });
  }, [catalog.champions, poolSearch, recommendationRole]);

  // Consultation view: when the user types a search or picks a role filter and
  // the scored pools are available, show real recommendation cards (score,
  // breakdown, reasons) from the full pool for the current phase. Champions
  // with no card (banned/picked/burned) are simply absent from the results.
  // `ranks` carries each card's position in the whole pool so #12 means
  // 12th-best overall.
  const consultation = useMemo(() => {
    if (!enrichedRecommendations) return null;
    const q = poolSearch.trim().toLowerCase();
    if (!q && recommendationRole === "all") return null;
    const banPhase = turn.phase === "ban";
    const pool = banPhase ? enrichedRecommendations.banPool : enrichedRecommendations.pickPool;
    const rows: Recommendation[] = [];
    const ranks = new Map<string, number>();
    pool.forEach((row, index) => {
      if (q && !row.championName.toLowerCase().includes(q) && !row.championId.includes(q)) return;
      if (recommendationRole !== "all" && row.suggestedRole?.toLowerCase() !== recommendationRole) return;
      rows.push(row);
      ranks.set(row.championId, index + 1);
    });
    return { rows, ranks };
  }, [enrichedRecommendations, turn.phase, poolSearch, recommendationRole]);

  const seriesHistory = useMemo(() => {
    const blue: string[] = [], red: string[] = [];
    completedGames.forEach((g) => { if (g.gameNumber < currentGame) { blue.push(...g.bluePicks); red.push(...g.redPicks); } });
    return { blue: [...new Set(blue)], red: [...new Set(red)] };
  }, [completedGames, currentGame]);

  const visibleRecommendations = useMemo(() => {
    const rows = turn.phase === "ban" ? enrichedRecommendations?.banRecommendations : enrichedRecommendations?.pickRecommendations;
    return (rows ?? []).filter((row) => recommendationRole === "all" || row.suggestedRole?.toLowerCase() === recommendationRole);
  }, [recommendationRole, enrichedRecommendations, turn.phase]);

  useEffect(() => {
    if (visibleRecommendations.some((row) => row.championId === selectedRecommendationId)) return;
    setSelectedRecommendationId(visibleRecommendations[0]?.championId ?? null);
  }, [selectedRecommendationId, visibleRecommendations]);

  useEffect(() => {
    if (!isFearless) return;
    setSeriesHistory(seriesHistory.blue, seriesHistory.red);
  }, [isFearless, seriesHistory]);

  // The overlay window gets the full-bleed compact body styling; the main window
  // keeps the normal app layout. (Each window is a separate document.)
  useEffect(() => {
    document.documentElement.classList.toggle("compact-mode", compact);
    document.body.classList.toggle("compact-mode", compact);
    return () => {
      document.documentElement.classList.remove("compact-mode");
      document.body.classList.remove("compact-mode");
    };
  }, [compact]);

  // Clear stale recommendations whenever the coach goes offline (e.g. between
  // save reloads). The fetch effect below will refill them once it's back.
  useEffect(() => {
    if (recommendationsEnabled) return;
    setRecommendations(null);
    setRecommendationError(null);
  }, [recommendationsEnabled]);

  // Live bans and picks from the lt_ai_coach_bridge game mod (read straight
  // from the game's ban/pick UI). Runs regardless of coach readiness: the
  // bridge feeds the board even when recommendations aren't available yet.
  // The main window polls this globally from App.tsx so auto-overlay works from
  // every tab. The compact overlay keeps its own poll because it is a separate
  // window/document.
  useEffect(() => {
    if (!compact) return;
    let active = true;
    const poll = () => {
      invoke<BridgeState>("get_draft_bridge")
        .then((bridge) => {
          if (!active) return;
          const liveDraftConnected = bridge.connected && bridge.phase !== "stadiumEntrance";
          setBridgeConnected(liveDraftConnected);
          if (bridge.contextRevision !== lastBridgeContextRevision.current) {
            lastBridgeContextRevision.current = bridge.contextRevision;
            setBridgeContext(bridge);
            if (bridge.matchId !== null && bridge.setNumber !== null) {
              syncLiveSeries(bridge.matchId, bridge.setNumber, bridge.completedGames);
            }
          }
          if (bridge.champions?.length) mergeDraftChampions(bridge.champions);
          if (!liveDraftConnected || bridge.revision === lastBridgeRevision.current) return;
          lastBridgeRevision.current = bridge.revision;
          setLiveRevision(bridge.revision);
          applyBridgeUpdate({
            blueBans: bridge.blueBans,
            redBans: bridge.redBans,
            bluePicks: bridge.bluePicks,
            redPicks: bridge.redPicks,
          });
        })
        .catch(() => { if (active) setBridgeConnected(false); });
      // Live champion tags from the mod (for comp analysis); cached backend-side.
      invoke<Record<string, string[]>>("get_champion_tags")
        .then((tags) => { if (active && Object.keys(tags).length) setChampionTags(tags); })
        .catch(() => {});
    };
    poll();
    const timer = window.setInterval(poll, 1000);
    return () => { active = false; window.clearInterval(timer); };
  }, [champions, mergeDraftChampions, setBridgeContext, setBridgeConnected, setChampionTags, setLiveRevision, syncLiveSeries]);

  useEffect(() => {
    if (!recommendationSide) {
      setRecommendations(null);
      setRecommendationError(WAITING_FOR_CONTEXT_MESSAGE);
      return;
    }
    let active = true;
    const timer = window.setTimeout(() => {
      const live = bridgeConnected && userSide !== null;
      const request = live
        ? invoke<LiveRecommendationResponse>("get_live_recommendations", { options: { mode, bansPerSide, weights, tuning, minimumInteractionGames, roleOverrides, shadowBlueBans: shadow.blueBans, shadowRedBans: shadow.redBans, shadowBluePicks: shadow.bluePicks, shadowRedPicks: shadow.redPicks } })
            .then((response) => {
              if (!active || response.sourceRevision !== liveRevision || response.sourceContextRevision !== liveContextRevision) return;
              setRecommendations(response.shortlist);
              setRecommendationError(null);
            })
        : invoke<RecommendationShortlist>("get_recommendations", { request: { mode, side: recommendationSide, bansPerSide, weights, tuning, minimumInteractionGames, blueLineup, redLineup, roleOverrides, ...effectiveDraft } })
            .then((shortlist) => { if (active) { setRecommendations(shortlist); setRecommendationError(null); } });
      request
        .catch((e) => {
          if (!active) return;
          if (!("__TAURI_INTERNALS__" in window)) {
            setRecommendations(previewRecommendations(catalog.champions));
            setRecommendationError(null);
          } else {
            setRecommendationError(e instanceof Error ? e.message : String(e));
          }
        });
    }, 120);
    return () => { active = false; window.clearTimeout(timer); };
  }, [effectiveDraft, shadow, mode, bansPerSide, recommendationSide, weights, tuning, minimumInteractionGames, blueLineup, redLineup, roleOverrides, bridgeConnected, userSide, liveRevision, liveContextRevision, recommendationsEnabled, tiers]);

  useEffect(() => { if (mode === "normal") clearSeriesProgress(); }, [mode]);

  useEffect(() => {
    if (!shadowEvictions) return;
    setEvictionFlash(new Set(shadowEvictions.entries.map((entry) => entry.championId)));
    setDissolving(shadowEvictions.entries);
    const flashTimer = window.setTimeout(() => setEvictionFlash(new Set()), 1800);
    const dissolveTimer = window.setTimeout(() => setDissolving([]), 1000);
    return () => { window.clearTimeout(flashTimer); window.clearTimeout(dissolveTimer); };
  }, [shadowEvictions]);

  // Arm a slot manually (user clicked an empty ban or pick slot). Clears the
  // pool search and moves focus to the search box so they can start typing.
  function armSlot(nextAction: DraftAction) {
    setAction(nextAction);
    setSlotSelected(true);
    setPoolSearch("");
    window.requestAnimationFrame(() => poolSearchRef.current?.focus());
  }

  function disarmSlot() {
    setSlotSelected(false);
    setPoolSearch("");
  }

  // While the bridge owns the board, manual entries become shadow (hypothetical)
  // champions instead of mutating the live draft; offline keeps real entry.
  // Series-history slots are never shadowed — they aren't bridge-owned.
  function applyChampion(championId: string) {
    const boardAction = targetAction === "blue-ban" || targetAction === "red-ban" || targetAction === "blue-pick" || targetAction === "red-pick";
    const applied = bridgeConnected && boardAction
      ? pushShadowChampion(championId, targetAction, bansPerSide, mode)
      : pushChampion(championId, targetAction, bansPerSide, mode);
    if (applied) disarmSlot();
  }

  // Removal routes to whichever layer holds the champion: shadows clear from
  // the shadow layer, real entries from the real draft.
  function handleRemove(target: keyof DraftState, championId: string) {
    if ((target === "blueBans" || target === "redBans" || target === "bluePicks" || target === "redPicks") && shadow[target].includes(championId)) {
      removeShadowChampion(target, championId);
      return;
    }
    removeChampion(target, championId);
  }

  // Role-confirm popover for filled pick slots (ported from the compact
  // overlay). Highlights the confirmed override, else the engine's inferred
  // role for that champion from its side's projection.
  const inferredRoles = useMemo(() => {
    const map = new Map<string, string>();
    for (const projection of [enrichedRecommendations?.blueProjection, enrichedRecommendations?.redProjection]) {
      for (const champion of projection?.champions ?? []) {
        const role = champion.roles.find((row) => row.assigned)?.role ?? champion.roles[0]?.role;
        if (role) map.set(champion.championId, role);
      }
    }
    return map;
  }, [enrichedRecommendations]);

  // What each filled pick slot displays: the user's confirmed role wins,
  // else the engine's inferred role for that champion.
  const displayRoles = useMemo(() => {
    const map = new Map(inferredRoles);
    for (const [championId, role] of Object.entries(roleOverrides)) map.set(championId, role);
    return map;
  }, [inferredRoles, roleOverrides]);

  function openRolePicker(championId: string, championName: string, event: { clientX: number; clientY: number }) {
    setRolePicker({
      championId,
      championName,
      x: event.clientX,
      y: event.clientY,
      current: roleOverrides[championId] ?? inferredRoles.get(championId) ?? null,
      overridden: championId in roleOverrides,
    });
  }

  if (compact) {
    return <CompactDraftBar
      mode={mode}
      bridgeConnected={bridgeConnected}
      turn={turn}
      draft={draft}
      recommendations={enrichedRecommendations}
      recommendationError={recommendationError}
      recommendationsEnabled={recommendationsEnabled}
      champions={champions}
      userSide={side}
      currentGame={currentGame}
      bansPerSide={bansPerSide}
      roleOverrides={roleOverrides}
      onConfirmRole={setRoleOverride}
      onClearRole={clearRoleOverride}
      onClose={() => void closeOverlayWindow()}
      onResetSeries={resetSeries}
      tiers={tiers}
    />;
  }

  const searchingPool = poolSearch.trim().length > 0;
  const turnLabel = translateTurnLabel(turn, t);
  const turnProgress = translateTurnProgress(turn, t);

  return (
    <section className={`full-draft-board${shadowActive ? " shadow-mode" : ""}`}>
      {shadowActive && (
        <button type="button" className="shadow-mode-pill" onClick={clearShadows} title={t("draft.clearShadowsTooltip")} aria-label={t("draft.clearShadowsTooltip")}>
          <span className="shadow-pill-icon">
            <IconGhost2 size={30} stroke={1.8} className="pill-ghost" />
            <IconX size={26} stroke={2.2} className="pill-x" />
            <span className="shadow-pill-count">{shadowCount}</span>
          </span>
          <span className="shadow-pill-label">{t("draft.shadowMode")}</span>
        </button>
      )}
      <header className="full-draft-topbar">
        <div className="draft-brand"><IconBrain size={18} stroke={2.2} /><strong>LT AI Coach</strong>{bridgeConnected && <span className="bridge-live" title={t("draft.bridgeLiveTooltip")}>{t("draft.liveBadge")}</span>}</div>
        <div className="draft-top-actions">
          <div className="draft-mode-toggle" aria-label={t("draft.modeAria")}>{(["normal", "fearless", "fearless-hard"] as DraftMode[]).map((value) => <button type="button" key={value} className={mode === value ? "active" : ""} disabled={bridgeConnected && liveDraftMode !== null} onClick={() => setMode(value)}>{value === "normal" ? t("draft.mode.normal") : value === "fearless" ? t("draft.mode.fearless") : t("draft.mode.hard")}</button>)}</div>
          <button type="button" className="draft-toolbar-button draft-reset-button" onClick={mode === "normal" ? resetCurrent : resetSeries}><IconRefresh size={15} />{mode === "normal" ? t("draft.newDraft") : t("draft.newSeries")}</button>
          {currentPatch && <span className="draft-patch-chip">{t("draft.patchChipPrefix")} {currentPatch}</span>}
          <button type="button" className="draft-toolbar-button draft-toolbar-icon" title={t("draft.showOverlayTooltip")} onClick={() => void showOverlayWindow()}><IconMaximize size={15} /></button>
        </div>
      </header>

      <div className="draft-workspace-panel">
        <div className={`draft-turn-bar ${turn.side ?? "complete"}`}>
          <span className="turn-dot" /><strong>{turnLabel}</strong><span className="turn-status">{turn.phase === "complete" ? t("draft.complete") : side ? turn.side === side ? t("draft.status.yourTurn") : t("draft.status.opponentTurn") : t("draft.status.waitingForTeam")}</span>
          <label className="draft-auto-overlay"><input type="checkbox" checked={autoOverlay} onChange={(event) => setAutoOverlay(event.target.checked)} />{t("draft.autoOverlay")}</label>
          <span className="draft-progress">{turnProgress}</span>
          <button type="button" disabled={!history.length} onClick={undo}>{t("draft.undo")}</button>
        </div>

        <div className="full-draft-layout">
          <div className="full-draft-team-column blue">
            <FullDraftSide part="picks" side="blue" isUser={side === "blue"} bansPerSide={bansPerSide} bans={board.lists.blueBans} picks={board.lists.bluePicks} champions={champions} activeAction={boardActiveAction} onRemove={handleRemove} onSlotClick={armSlot} lineup={blueLineup} athleteNames={athleteNames} shadowIds={board.shadowIds.bluePicks} dissolvingIds={board.dissolvingIds.bluePicks} onRoleClick={openRolePicker} roles={displayRoles} overrides={roleOverrides} flashIds={evictionFlash} />
            <FullDraftSide part="bans" side="blue" isUser={side === "blue"} bansPerSide={bansPerSide} bans={board.lists.blueBans} picks={board.lists.bluePicks} champions={champions} activeAction={boardActiveAction} onRemove={handleRemove} onSlotClick={armSlot} shadowIds={board.shadowIds.blueBans} dissolvingIds={board.dissolvingIds.blueBans} flashIds={evictionFlash} />
          </div>
          <main className="draft-coach-column">
          <div className="draft-recommendation-toolbar">
            <label>
              <IconSearch size={16} />
              <input
                ref={poolSearchRef}
                type="search"
                placeholder={slotSelected ? `${t("draft.searchForAction")} ${t(draftActionLabelKey(action))}…` : t("draft.searchPlaceholder")}
                value={poolSearch}
                onChange={(e) => setPoolSearch(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key !== "Enter") return;
                  // Prefer the top scored consultation card; fall back to the
                  // plain catalog grid when the pools aren't available.
                  if (consultation) {
                    if (consultation.rows.length) applyChampion(consultation.rows[0].championId);
                    return;
                  }
                  const first = filteredPool.find((c) => !unavailableReason(c.id, targetAction, mode, effectiveDraft, bansPerSide));
                  if (first) applyChampion(first.id);
                }}
              />
            </label>
            <div className="draft-role-filters" aria-label={t("draft.roleFilterAria")}>
              <button type="button" className={recommendationRole === "all" ? "active" : ""} onClick={() => setRecommendationRole("all")} title={t("draft.allRolesTitle")}><IconLayoutGrid size={15} /></button>
              {["top", "jungle", "mid", "bot", "support"].map((role) => <button type="button" key={role} className={recommendationRole === role ? "active" : ""} onClick={() => setRecommendationRole(role)} title={t(`role.${role}`)}><RoleGlyph role={role} /></button>)}
            </div>
          </div>
          {consultation ? (
            consultation.rows.length === 0 ? (
              <p className="recommendation-empty">
                {searchingPool
                  ? <>{t("draft.noChampionsMatch")} &ldquo;{poolSearch}&rdquo;.</>
                  : t("draft.noRoleCandidates")}
              </p>
            ) : (
              <div className="consultation-results">
                <FullDraftRecommendations
                  rows={consultation.rows}
                  limit={8}
                  ranks={consultation.ranks}
                  selectedId={selectedRecommendationId}
                  onSelect={applyChampion}
                />
              </div>
            )
          ) : searchingPool ? (
            filteredPool.length === 0 ? (
              <p className="recommendation-empty">{t("draft.noChampionsMatch")} &ldquo;{poolSearch}&rdquo;.</p>
            ) : (
              <div className="inline-champion-pool">
                {filteredPool.map((champion) => {
                  const reason = unavailableReason(champion.id, targetAction, mode, effectiveDraft, bansPerSide);
                  return <button type="button" key={champion.id} className="champion-pool-card" disabled={Boolean(reason)} title={reason ? t(reason) : champion.name} onClick={() => applyChampion(champion.id)}><ChampionPortraitView portrait={champion.portrait} /><span>{champion.name}</span></button>;
                })}
              </div>
            )
          ) : (
            <FullDraftRecommendations
              rows={visibleRecommendations}
              selectedId={selectedRecommendationId}
              onSelect={applyChampion}
              loadingLabel={!recommendationsEnabled ? t("draft.prepareToLoadRecs") : recommendationError ? translateRecommendationError(recommendationError, t) : t("draft.calculatingRecs")}
            />
          )}
            <CompAnalysis picks={side === "blue" ? effectiveDraft.bluePicks : side === "red" ? effectiveDraft.redPicks : []} />
          </main>
          <div className="full-draft-team-column red">
            <FullDraftSide part="picks" side="red" isUser={side === "red"} bansPerSide={bansPerSide} bans={board.lists.redBans} picks={board.lists.redPicks} champions={champions} activeAction={boardActiveAction} onRemove={handleRemove} onSlotClick={armSlot} lineup={redLineup} athleteNames={athleteNames} shadowIds={board.shadowIds.redPicks} dissolvingIds={board.dissolvingIds.redPicks} onRoleClick={openRolePicker} roles={displayRoles} overrides={roleOverrides} flashIds={evictionFlash} />
            <FullDraftSide part="bans" side="red" isUser={side === "red"} bansPerSide={bansPerSide} bans={board.lists.redBans} picks={board.lists.redPicks} champions={champions} activeAction={boardActiveAction} onRemove={handleRemove} onSlotClick={armSlot} shadowIds={board.shadowIds.redBans} dissolvingIds={board.dissolvingIds.redBans} flashIds={evictionFlash} />
          </div>
        </div>

        {slotSelected && (
          <div className={`active-target-bar ${action.startsWith("red") ? "red" : "blue"}`}>
            <span>{t("draft.activeSlot")}</span>
            <strong>{t(draftActionLabelKey(action))}</strong>
            <button type="button" className="draft-disarm-btn" onClick={disarmSlot}>{t("draft.cancel")}</button>
          </div>
        )}
        {isFearless && <SeriesBar currentGame={currentGame} completedGames={completedGames} seriesHistory={seriesHistory} onGameClick={moveToGame} onFinishGame={finishGame} />}
      </div>
      {rolePicker && <RolePickerPopover state={rolePicker} onPick={setRoleOverride} onClear={clearRoleOverride} onClose={() => setRolePicker(null)} />}
    </section>
  );
}
