// The compact, always-on-top live-draft overlay — three equal thirds:
// board (left) · selected-pick detail (middle) · the three top picks (right).
// Clicking a pick card fills the middle detail panel.

import { useEffect, useState } from "react";
import type { DraftChampion, DraftMode, DraftSide, DraftState, DraftTurn, RecommendationShortlist } from "../types";
import { ChampionPortraitView } from "./ChampionPortraitView";
import { CompactTeamSlots } from "./CompactTeamSlots";
import { RoleGlyph } from "./RoleGlyph";
import { RolePickerPopover, type RolePickerState } from "./RolePickerPopover";
import { FadeOnChange } from "../motion/FadeOnChange";
import { DURATION } from "../motion/config";
import { ReasonList } from "./ReasonList";
import { translateRecommendationError, translateTurnLabel } from "../lib/draft";
import { useT } from "../stores/useI18nStore";
import { IconRefresh, IconX } from "@tabler/icons-react";

export function CompactDraftBar({ mode, bridgeConnected, turn, draft, recommendations, recommendationError, recommendationsEnabled, champions, userSide, currentGame, roleOverrides, onConfirmRole, onClearRole, onClose, onResetSeries, tiers }: {
  mode: DraftMode;
  bridgeConnected: boolean;
  turn: DraftTurn;
  draft: DraftState;
  recommendations: RecommendationShortlist | null;
  recommendationError: string | null;
  recommendationsEnabled: boolean;
  champions: Map<string, DraftChampion>;
  userSide: DraftSide | null;
  currentGame: number;
  roleOverrides: Record<string, string>;
  onConfirmRole: (championId: string, role: string) => void;
  onClearRole: (championId: string) => void;
  onClose: () => void;
  onResetSeries: () => void;
  tiers: Record<string, string>;
}) {
  const t = useT();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [rolePicker, setRolePicker] = useState<RolePickerState | null>(null);
  const isFearless = mode !== "normal";
  const phase = turn.phase === "complete" ? "pick" : turn.phase;
  const rows = phase === "ban" ? recommendations?.banRecommendations : recommendations?.pickRecommendations;
  const topRows = rows?.slice(0, 3) ?? [];
  // The only time the draft is "complete" while the bridge is still live is the
  // in-game swap stage, where the comp is locked but roles are still being set.
  const isSwap = turn.phase === "complete" && bridgeConnected && userSide !== null;
  const swapSide: DraftSide = userSide ?? "blue";
  const swapProjection = userSide === "blue" ? recommendations?.blueProjection : recommendations?.redProjection;
  const swapRoles = isSwap
    ? new Map((swapProjection?.champions ?? []).map((c) => [c.championId, c.roles[0]?.role] as const))
    : undefined;
  // The engine's inferred role per champion on the user's own side, used to
  // label picks and to pre-highlight the role-confirm popover.
  const myProjection = userSide === "blue" ? recommendations?.blueProjection : userSide === "red" ? recommendations?.redProjection : undefined;
  const myRoles = new Map((myProjection?.champions ?? []).map((c) => [c.championId, c.roles.find((r) => r.assigned)?.role ?? c.roles[0]?.role] as const));
  // Only the user's own picks are role-confirmable, and only during the pick phase.
  const canConfirmRoles = phase === "pick" && userSide !== null;
  const openRolePicker = (championId: string, championName: string, event: { clientX: number; clientY: number }) => {
    setRolePicker({
      championId,
      championName,
      x: event.clientX,
      y: event.clientY,
      current: roleOverrides[championId] ?? myRoles.get(championId) ?? null,
      overridden: championId in roleOverrides,
    });
  };
  const headlineSide = isSwap ? userSide : turn.side;
  const headlineLabel = isSwap ? t("compact.swapStage") : translateTurnLabel(turn, t);
  const modeLabel = mode === "normal" ? t("draft.mode.normal") : `${mode === "fearless" ? t("draft.mode.fearless") : t("compact.fearlessHard")} · G${currentGame}`;

  // Keep the middle detail pinned to a valid pick; default to the top one.
  useEffect(() => {
    if (topRows.some((row) => row.championId === selectedId)) return;
    setSelectedId(topRows[0]?.championId ?? null);
  }, [selectedId, topRows]);

  const selectedRec = topRows.find((row) => row.championId === selectedId) ?? topRows[0] ?? null;

  return <section className={`compact-draft-bar${isSwap ? " swap-mode" : ""}`}>
    <header className="compact-topbar" data-tauri-drag-region>
      <span className={`compact-topbar-dot ${headlineSide ?? ""}`} />
      <strong className="compact-topbar-title">{headlineLabel}</strong>
      {!isSwap && turn.phase !== "complete" && userSide && <span className={`compact-turn-badge ${turn.side}`}>{turn.side === userSide ? t("draft.status.yourTurn") : t("compact.oppTurn")}</span>}
      <span className="compact-mode-label">{modeLabel}</span>
      <span className={`compact-live${bridgeConnected ? " connected" : ""}`}><span aria-hidden="true">●</span>{bridgeConnected ? t("compact.live") : t("compact.waiting")}</span>
      {isFearless && <button type="button" className="compact-newseries" onClick={onResetSeries} title={t("compact.newSeriesTooltip")}><IconRefresh size={13} />{t("draft.newSeries")}</button>}
      <button type="button" className="compact-expand-btn" onClick={onClose} title={t("compact.closeTooltip")} aria-label={t("compact.closeTooltip")}><IconX size={16} /></button>
    </header>

    {isSwap ? (
      <div className="compact-swap">
        <span className="compact-swap-title">{t("compact.assignRolesTitle")}</span>
        <CompactTeamSlots
          side={swapSide}
          label={`${t(`draft.side.${swapSide}`)} ${t("compact.picksWord")}`}
          ids={swapSide === "blue" ? draft.bluePicks : draft.redPicks}
          limit={5}
          champions={champions}
          active={false}
          selected
          roles={swapRoles}
          big
        />
      </div>
    ) : (
      <div className="compact-thirds">
        <div className="compact-board">
          <CompactTeamSlots side="blue" label={`${t("draft.side.blue")} ${phase === "ban" ? t("compact.bansWord") : t("compact.picksWord")}`} ids={phase === "ban" ? draft.blueBans : draft.bluePicks} limit={phase === "ban" ? 3 : 5} champions={champions} active={turn.side === "blue" && turn.phase !== "complete"} selected={userSide === "blue"} roles={userSide === "blue" && phase === "pick" ? myRoles : undefined} overrides={userSide === "blue" ? roleOverrides : undefined} interactive={canConfirmRoles && userSide === "blue"} onSlotActivate={openRolePicker} />
          <CompactTeamSlots side="red" label={`${t("draft.side.red")} ${phase === "ban" ? t("compact.bansWord") : t("compact.picksWord")}`} ids={phase === "ban" ? draft.redBans : draft.redPicks} limit={phase === "ban" ? 3 : 5} champions={champions} active={turn.side === "red" && turn.phase !== "complete"} selected={userSide === "red"} roles={userSide === "red" && phase === "pick" ? myRoles : undefined} overrides={userSide === "red" ? roleOverrides : undefined} interactive={canConfirmRoles && userSide === "red"} onSlotActivate={openRolePicker} />
        </div>

        <div className="compact-detail">
          {turn.phase === "complete" ? <span className="compact-detail-empty">{t("draft.complete")}</span>
            : selectedRec ? <FadeOnChange changeKey={selectedRec.championId} className="compact-detail-body" duration={DURATION.fast}>
              <div className="compact-detail-head">
                <strong>{selectedRec.championName}</strong>
                {selectedRec.suggestedRole && <span className="compact-detail-role"><RoleGlyph role={selectedRec.suggestedRole} /></span>}
                <span className="compact-detail-score">{selectedRec.score.toFixed(0)}</span>
              </div>
              <ReasonList className="compact-detail-reasons" reasons={selectedRec.reasons} />
            </FadeOnChange> : recommendationError ? <span className="compact-detail-empty" title={translateRecommendationError(recommendationError, t)}>{t("compact.recUnavailable")}</span>
              : !recommendationsEnabled ? <span className="compact-detail-empty">{t("compact.coachPaused")}</span>
                : <span className="compact-detail-empty">{t("compact.calculating")}</span>}
        </div>

        <FadeOnChange changeKey={topRows.map((row) => row.championId).join("|")} className="compact-picks" duration={DURATION.fast}>
          {turn.phase === "complete" ? <span className="compact-picks-empty">—</span>
            : topRows.length ? topRows.map((row, index) => <button type="button" key={row.championId} className={`compact-pick-card${selectedRec?.championId === row.championId ? " selected" : ""}`} onClick={() => setSelectedId(row.championId)} title={row.championName}>
              {index === 0 && <span className="compact-pick-rank">1</span>}
              {tiers[row.championId] && <span className="compact-pick-tier" title={`${t("compact.manualTierTooltip")} ${tiers[row.championId]}`}>{tiers[row.championId]}</span>}
              <span className="compact-pick-portrait"><ChampionPortraitView portrait={row.portrait} width={104} height={150} scaleMode="champion" fixedCenter /></span>
              <span className="compact-pick-name">{row.championName}</span>
            </button>) : <span className="compact-picks-empty">{t("compact.calculating")}</span>}
        </FadeOnChange>
      </div>
    )}
    {rolePicker && <RolePickerPopover state={rolePicker} onPick={onConfirmRole} onClear={onClearRole} onClose={() => setRolePicker(null)} />}
  </section>;
}
