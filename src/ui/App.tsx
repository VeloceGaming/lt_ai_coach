import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DraftBoard } from "./components/DraftBoard";
import { ExtractionPanel } from "./components/ExtractionPanel";
import { FullAppShell } from "./components/FullAppShell";
import type { FullScreen } from "./components/NavigationRail";
import { TierListScreen } from "./components/TierListScreen";
import { StatisticsPanel } from "./components/StatisticsPanel";
import { PatchNotesScreen } from "./components/PatchNotesScreen";
import { PlayerHubScreen } from "./components/PlayerHubScreen";
import { SettingsScreen } from "./components/SettingsScreen";
import { useImportStore } from "./stores/useImportStore";
import { useDraftStore } from "./stores/useDraftStore";
import { useOverlayStore } from "./stores/useOverlayStore";
import { usePreferencesStore } from "./stores/usePreferencesStore";
import { useAppShortcuts } from "./hooks/useAppShortcuts";
import { currentWindowLabel, hideOverlayWindow, showOverlayWindow } from "./lib/overlayWindow";
import { useChampionName, useT } from "./stores/useI18nStore";
import type { BridgePhase, BridgeState } from "./types";

// Which window this document is running in. The "overlay" window renders only the
// compact live-draft strip; the "main" window renders the full app.
const isOverlayWindow = currentWindowLabel() === "overlay";

// Same labels as the navigation rail (commands/appCommands.ts) — reusing the
// keys instead of duplicating text keeps the title bar and rail in sync.
const screenNameKeys: Record<FullScreen, string> = {
  import: "nav.import.label",
  players: "nav.players.label",
  tiers: "nav.tiers.label",
  stats: "nav.stats.label",
  draft: "nav.draft.label",
  patch: "nav.patch.label",
  settings: "nav.settings.label",
};

export function App() {
  const t = useT();
  const championName = useChampionName();
  const status = useImportStore((s) => s.status);
  const summary = useImportStore((s) => s.summary);
  const rawStatistics = useImportStore((s) => s.statistics);
  const rawDraftCatalog = useImportStore((s) => s.draftCatalog);
  const athletes = useImportStore((s) => s.athletes);
  const busy = useImportStore((s) => s.busy);
  const coachState = useImportStore((s) => s.coachState);
  const message = useImportStore((s) => s.message);
  const tiers = useImportStore((s) => s.tiers);
  const patchHistory = useImportStore((s) => s.patchHistory);
  const initialize = useImportStore((s) => s.initialize);
  const setChampionTier = useImportStore((s) => s.setChampionTier);
  const setChampionOverride = useImportStore((s) => s.setChampionOverride);
  const prepareCoach = useImportStore((s) => s.prepareCoach);
  const runGameImport = useImportStore((s) => s.runGameImport);
  const repairMissingPortraits = useImportStore((s) => s.repairMissingPortraits);
  const refreshFromGameAndPrepare = useImportStore((s) => s.refreshFromGameAndPrepare);
  const loadAthleteDetail = useImportStore((s) => s.loadAthleteDetail);
  const lookupAthleteMastery = useImportStore((s) => s.lookupAthleteMastery);
  const mergeDraftChampions = useImportStore((s) => s.mergeDraftChampions);
  const autoOverlay = usePreferencesStore((s) => s.autoOverlay);
  const bridgeConnected = useOverlayStore((s) => s.bridgeConnected);
  const setBridgeConnected = useOverlayStore((s) => s.setBridgeConnected);
  const setBridgeContext = useOverlayStore((s) => s.setBridgeContext);
  const setLiveRevision = useOverlayStore((s) => s.setLiveRevision);
  const setChampionTags = useOverlayStore((s) => s.setChampionTags);
  const applyBridgeUpdate = useDraftStore((s) => s.applyBridgeUpdate);
  const syncLiveSeries = useDraftStore((s) => s.syncLiveSeries);
  const [screen, setScreen] = useState<FullScreen>("import");
  useAppShortcuts(setScreen);
  const lastDraftLive = useRef<boolean | null>(null);
  const lastBridgePhase = useRef<BridgePhase>("");
  const lastBridgePhaseRevision = useRef(-1);
  const lastBridgeRevision = useRef(-1);
  const lastBridgeContextRevision = useRef(-1);
  // Champion to focus when jumping between the Tier list and Patch notes screens.
  const [focusChampion, setFocusChampion] = useState<string | null>(null);
  // Champion names, translated once here, for every screen downstream (they
  // all receive draftCatalog/statistics as props from this component). See
  // useChampionName's doc comment for why this doesn't touch en/t().
  const draftCatalog = rawDraftCatalog ? { ...rawDraftCatalog, champions: rawDraftCatalog.champions.map((champion) => ({ ...champion, name: championName(champion.id, champion.name) })) } : rawDraftCatalog;
  const statistics = rawStatistics ? {
    ...rawStatistics,
    overallRows: rawStatistics.overallRows.map((row) => ({ ...row, championName: championName(row.championId, row.championName) })),
    roleRows: rawStatistics.roleRows.map((row) => ({ ...row, championName: championName(row.championId, row.championName) })),
  } : rawStatistics;
  const championLookup = new Map((draftCatalog?.champions ?? []).map((champion) => [champion.id, { name: champion.name, portrait: champion.portrait }]));
  const openChampionInTiers = (championId: string) => { setFocusChampion(championId); setScreen("tiers"); };

  useEffect(() => { initialize(); }, [initialize]);

  // The live-draft bridge watcher belongs to the main app shell, not the Draft
  // tab. That lets auto-overlay wake up even while the full window is showing
  // stats, patch notes, settings, etc.
  useEffect(() => {
    if (isOverlayWindow) return;
    let active = true;
    const poll = () => {
      invoke<BridgeState>("get_draft_bridge")
        .then((bridge) => {
          if (!active) return;
          if (bridge.phaseRevision !== lastBridgePhaseRevision.current) {
            const previous = lastBridgePhase.current;
            lastBridgePhase.current = bridge.phase;
            lastBridgePhaseRevision.current = bridge.phaseRevision;
            // Refresh game data and rebuild the recommendation cache now, while
            // there's a natural window before the draft begins — so the coach is
            // ready by the time picks/bans start. This is the only place that
            // re-imports; opening the overlay no longer disrupts a live draft.
            if (previous !== "stadiumEntrance" && bridge.phase === "stadiumEntrance") {
              void refreshFromGameAndPrepare();
            }
          }
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
          if (!liveDraftConnected) return;
          // Only push to the draft store when the bans/picks actually changed.
          // Without this guard, applyBridgeUpdate builds a fresh draft object every
          // poll (1s), which re-runs the recommendation fetch and cancels any
          // in-flight request — leaving the board stuck on "Calculating…" whenever
          // a fetch takes longer than the poll interval. (The compact overlay's
          // own poll already guards this way.)
          if (bridge.revision === lastBridgeRevision.current) return;
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
      invoke<Record<string, string[]>>("get_champion_tags")
        .then((tags) => { if (active && Object.keys(tags).length) setChampionTags(tags); })
        .catch(() => {});
    };
    poll();
    const timer = window.setInterval(poll, 1000);
    return () => { active = false; window.clearInterval(timer); };
  }, [applyBridgeUpdate, mergeDraftChampions, refreshFromGameAndPrepare, setBridgeConnected, setBridgeContext, setChampionTags, setLiveRevision, syncLiveSeries]);

  // Auto-show/hide the compact overlay from the global bridge state. This only
  // toggles window visibility; the import + precache happens on the stadium-entrance
  // transition above, not here — so opening the overlay never disrupts a draft.
  useEffect(() => {
    if (isOverlayWindow) return;
    const was = lastDraftLive.current;
    lastDraftLive.current = bridgeConnected;
    if (!autoOverlay) return;
    if (bridgeConnected && was !== true) void showOverlayWindow();
    if (!bridgeConnected && was === true) void hideOverlayWindow();
  }, [autoOverlay, bridgeConnected]);

  // Overlay window: render only the compact live-draft strip.
  if (isOverlayWindow) {
    return draftCatalog
      ? <DraftBoard catalog={draftCatalog} recommendationsEnabled={coachState === "ready"} tiers={tiers} currentPatch={statistics?.currentPatch} compact />
      : <section className="overlay-loading">{t("app.overlayLoading")}</section>;
  }

  const automaticState = bridgeConnected ? "live" : busy ? "working" : status?.databaseReady ? "ready" : "waiting";
  const header = <header className="workspace-header" data-automatic-state={automaticState}>
    <div><span className="workspace-brand">LT AI Coach</span><strong>{t(screenNameKeys[screen])}</strong><span className="phase-badge" data-state={automaticState}>{status?.phase ?? t("app.connecting")}</span></div>
    <div className="workspace-actions">
      <button type="button" className="secondary-button" disabled={busy} onClick={repairMissingPortraits}>{t("app.repairPortraits")}</button>
      <button type="button" className="secondary-button" disabled={busy} onClick={() => runGameImport()}>{busy ? t("app.importingButton") : t("app.importFromGame")}</button>
      <button type="button" className={coachState === "ready" ? "coach-ready-button" : "primary-button"} disabled={busy || coachState !== "paused" || !status?.databaseReady} onClick={prepareCoach}>{coachState === "preparing" ? t("app.coachPreparing") : coachState === "ready" ? t("app.coachReady") : t("app.prepareCoach")}</button>
    </div>
  </header>;

  return <FullAppShell screen={screen} screenTitle={t(screenNameKeys[screen])} onScreenChange={setScreen} header={screen === "tiers" || screen === "draft" ? undefined : header}>
    {screen === "import" && message && <p className={summary ? "notice success" : "notice"} role="status">{message}</p>}

    {screen === "import" && <section className="screen-panel import-screen">
      <div className="screen-heading"><div><span className="eyebrow">{t("app.import.eyebrow")}</span><h2>{t("app.import.title")}</h2></div><span>{status?.databaseReady ? t("app.import.dbReady") : t("app.import.waiting")}</span></div>
      {!summary && <div className="screen-empty"><strong>{t("app.import.emptyTitle")}</strong><p>{t("app.import.emptyDesc")}</p></div>}
      {summary && <ExtractionPanel summary={summary} />}
    </section>}

    {screen === "players" && (draftCatalog
      ? <PlayerHubScreen athletes={athletes} playerTeamId={summary?.playerTeamId ?? null} catalog={draftCatalog} loadDetail={loadAthleteDetail} lookupMastery={lookupAthleteMastery} />
      : <section className="screen-panel screen-empty"><strong>{t("app.players.emptyTitle")}</strong><p>{t("app.players.emptyDesc")}</p></section>)}

    {screen === "tiers" && (statistics ? <TierListScreen statistics={statistics} tiers={tiers} focusChampionId={focusChampion} onSetTier={setChampionTier} /> : <section className="screen-panel screen-empty"><strong>{t("app.tiers.emptyTitle")}</strong><p>{t("app.tiers.emptyDesc")}</p><button type="button" className="primary-button" disabled={busy || coachState !== "paused" || !status?.databaseReady} onClick={prepareCoach}>{t("app.prepareCoach")}</button></section>)}

    {screen === "stats" && (statistics && draftCatalog ? <section className="screen-panel stats-screen"><StatisticsPanel statistics={statistics} draftCatalog={draftCatalog} tiers={tiers} onSetTier={setChampionTier} onSetChampionOverride={setChampionOverride} /></section> : <section className="screen-panel screen-empty"><strong>{t("app.stats.emptyTitle")}</strong><p>{t("app.stats.emptyDesc")}</p><button type="button" className="primary-button" disabled={busy || coachState !== "paused" || !status?.databaseReady} onClick={prepareCoach}>{t("app.prepareCoach")}</button></section>)}

    {screen === "draft" && (draftCatalog ? <DraftBoard catalog={draftCatalog} recommendationsEnabled={coachState === "ready"} tiers={tiers} currentPatch={statistics?.currentPatch} athletes={athletes} /> : <section className="screen-panel screen-empty"><strong>{t("app.draft.emptyTitle")}</strong><p>{t("app.draft.emptyDesc")}</p></section>)}

    {screen === "patch" && (patchHistory.length ? <section className="screen-panel patch-notes-panel"><PatchNotesScreen patchHistory={patchHistory} currentPatch={statistics?.currentPatch} championLookup={championLookup} focusChampionId={focusChampion} onOpenChampion={openChampionInTiers} /></section> : <section className="screen-panel screen-empty"><strong>{t("app.patch.emptyTitle")}</strong><p>{t("app.patch.emptyDesc")}</p><button type="button" className="secondary-button" disabled={busy} onClick={() => runGameImport()}>{busy ? t("app.importingButton") : t("app.importFromGame")}</button></section>)}
    {screen === "settings" && <section className="screen-panel settings-panel"><SettingsScreen /></section>}
  </FullAppShell>;
}
