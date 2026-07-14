// The draft board. In the main window it renders full mode (manual draft entry,
// live bridge feed, screenshot detection, recommendations, Fearless tracking).
// In the overlay window (compact prop) it renders the compact live-draft strip.
// The two run as separate windows, each with its own state — see overlayWindow.ts.

import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AthleteSummary, BridgeState, DraftAction, DraftCatalog, DraftChampion, DraftMode, LiveRecommendationResponse, Recommendation, RecommendationShortlist } from "../types";
import { activeTuning } from "../lib/preferences";
import { calculateDraftTurn, draftActionLabelKey, translateRecommendationError, translateTurnLabel, translateTurnProgress, unavailableReason, WAITING_FOR_CONTEXT_MESSAGE } from "../lib/draft";
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
import { IconBrain, IconLayoutGrid, IconMaximize, IconRefresh, IconSearch } from "@tabler/icons-react";

function fallbackChampionName(id: string) {
  return id.split("_").map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join(" ");
}

export function DraftBoard({ catalog, recommendationsEnabled, tiers, currentPatch, athletes = [], compact = false }: { catalog: DraftCatalog; recommendationsEnabled: boolean; tiers: Record<string, string>; currentPatch?: string; athletes?: AthleteSummary[]; compact?: boolean }) {
  const t = useT();
  const mode = usePreferencesStore((s) => s.mode);
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
  const setLiveRevision = useOverlayStore((s) => s.setLiveRevision);
  const lastBridgeRevision = useRef(-1);
  const lastBridgeContextRevision = useRef(-1);
  const poolSearchRef = useRef<HTMLInputElement>(null);
  const isFearless = mode !== "normal";
  const turn = calculateDraftTurn(draft);
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
    ])];
  }, [draft]);
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
    return { ...recommendations, pickRecommendations: fill(recommendations.pickRecommendations), banRecommendations: fill(recommendations.banRecommendations) };
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
        ? invoke<LiveRecommendationResponse>("get_live_recommendations", { options: { mode, weights, tuning, minimumInteractionGames, roleOverrides } })
            .then((response) => {
              if (!active || response.sourceRevision !== liveRevision || response.sourceContextRevision !== liveContextRevision) return;
              setRecommendations(response.shortlist);
              setRecommendationError(null);
            })
        : invoke<RecommendationShortlist>("get_recommendations", { request: { mode, side: recommendationSide, weights, tuning, minimumInteractionGames, blueLineup, redLineup, roleOverrides, ...draft } })
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
  }, [draft, mode, recommendationSide, weights, tuning, minimumInteractionGames, blueLineup, redLineup, roleOverrides, bridgeConnected, userSide, liveRevision, liveContextRevision, recommendationsEnabled, tiers]);

  useEffect(() => { if (mode === "normal") clearSeriesProgress(); }, [mode]);

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

  function applyChampion(championId: string) {
    if (pushChampion(championId, targetAction)) disarmSlot();
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
    <section className="full-draft-board">
      <header className="full-draft-topbar">
        <div className="draft-brand"><IconBrain size={18} stroke={2.2} /><strong>LT AI Coach</strong>{bridgeConnected && <span className="bridge-live" title={t("draft.bridgeLiveTooltip")}>{t("draft.liveBadge")}</span>}</div>
        <div className="draft-top-actions">
          <div className="draft-mode-toggle" aria-label={t("draft.modeAria")}>{(["normal", "fearless", "fearless-hard"] as DraftMode[]).map((value) => <button type="button" key={value} className={mode === value ? "active" : ""} onClick={() => setMode(value)}>{value === "normal" ? t("draft.mode.normal") : value === "fearless" ? t("draft.mode.fearless") : t("draft.mode.hard")}</button>)}</div>
          <button type="button" className="draft-toolbar-button draft-reset-button" onClick={mode === "normal" ? resetCurrent : resetSeries}><IconRefresh size={15} />{mode === "normal" ? t("draft.newDraft") : t("draft.newSeries")}</button>
          {currentPatch && <span className="draft-patch-chip">{t("draft.patchChipPrefix")} {currentPatch}</span>}
          <button type="button" className="draft-toolbar-button draft-toolbar-icon" title={t("draft.showOverlayTooltip")} onClick={() => void showOverlayWindow()}><IconMaximize size={15} /></button>
        </div>
      </header>

      <div className={`draft-turn-bar ${turn.side ?? "complete"}`}>
        <span className="turn-dot" /><strong>{turnLabel}</strong><span className="turn-status">{turn.phase === "complete" ? t("draft.complete") : side ? turn.side === side ? t("draft.status.yourTurn") : t("draft.status.opponentTurn") : t("draft.status.waitingForTeam")}</span>
        <label className="draft-auto-overlay"><input type="checkbox" checked={autoOverlay} onChange={(event) => setAutoOverlay(event.target.checked)} />{t("draft.autoOverlay")}</label>
        <span className="draft-progress">{turnProgress}</span>
        <button type="button" disabled={!history.length} onClick={undo}>{t("draft.undo")}</button>
      </div>

      <div className="full-draft-layout">
        <FullDraftSide part="picks" side="blue" isUser={side === "blue"} bans={draft.blueBans} picks={draft.bluePicks} champions={champions} activeAction={boardActiveAction} onRemove={removeChampion} onSlotClick={armSlot} lineup={blueLineup} athleteNames={athleteNames} />
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
                  const first = filteredPool.find((c) => !unavailableReason(c.id, targetAction, mode, draft));
                  if (first) applyChampion(first.id);
                }}
              />
            </label>
            <div className="draft-role-filters" aria-label={t("draft.roleFilterAria")}>
              <button type="button" className={recommendationRole === "all" ? "active" : ""} onClick={() => setRecommendationRole("all")} title={t("draft.allRolesTitle")}><IconLayoutGrid size={15} /></button>
              {["top", "jungle", "mid", "bot", "support"].map((role) => <button type="button" key={role} className={recommendationRole === role ? "active" : ""} onClick={() => setRecommendationRole(role)} title={t(`role.${role}`)}><RoleGlyph role={role} /></button>)}
            </div>
          </div>
          {searchingPool ? (
            filteredPool.length === 0 ? (
              <p className="recommendation-empty">{t("draft.noChampionsMatch")} &ldquo;{poolSearch}&rdquo;.</p>
            ) : (
              <div className="inline-champion-pool">
                {filteredPool.map((champion) => {
                  const reason = unavailableReason(champion.id, targetAction, mode, draft);
                  return <button type="button" key={champion.id} className="champion-pool-card" disabled={Boolean(reason)} title={reason ? t(reason) : champion.name} onClick={() => applyChampion(champion.id)}><ChampionPortraitView portrait={champion.portrait} /><span>{champion.name}</span></button>;
                })}
              </div>
            )
          ) : (
            <FullDraftRecommendations
              rows={visibleRecommendations}
              selectedId={selectedRecommendationId}
              onSelect={(championId) => {
                if (slotSelected) applyChampion(championId);
                setSelectedRecommendationId(championId);
              }}
              loadingLabel={!recommendationsEnabled ? t("draft.prepareToLoadRecs") : recommendationError ? translateRecommendationError(recommendationError, t) : t("draft.calculatingRecs")}
            />
          )}
          <CompAnalysis picks={side === "blue" ? draft.bluePicks : side === "red" ? draft.redPicks : []} />
        </main>
        <FullDraftSide part="picks" side="red" isUser={side === "red"} bans={draft.redBans} picks={draft.redPicks} champions={champions} activeAction={boardActiveAction} onRemove={removeChampion} onSlotClick={armSlot} lineup={redLineup} athleteNames={athleteNames} />
      </div>

      <div className="draft-bans-strip">
        <FullDraftSide part="bans" side="blue" isUser={side === "blue"} bans={draft.blueBans} picks={draft.bluePicks} champions={champions} activeAction={boardActiveAction} onRemove={removeChampion} onSlotClick={armSlot} />
        <FullDraftSide part="bans" side="red" isUser={side === "red"} bans={draft.redBans} picks={draft.redPicks} champions={champions} activeAction={boardActiveAction} onRemove={removeChampion} onSlotClick={armSlot} />
      </div>

      {slotSelected ? (
        <div className={`active-target-bar ${action.startsWith("red") ? "red" : "blue"}`}>
          <span>{t("draft.activeSlot")}</span>
          <strong>{t(draftActionLabelKey(action))}</strong>
          <button type="button" className="draft-disarm-btn" onClick={disarmSlot}>{t("draft.cancel")}</button>
        </div>
      ) : (
        <p className="draft-slot-prompt">{t("draft.clickSlotPrompt")}</p>
      )}
      {isFearless && <SeriesBar currentGame={currentGame} completedGames={completedGames} seriesHistory={seriesHistory} onGameClick={moveToGame} onFinishGame={finishGame} />}
    </section>
  );
}
