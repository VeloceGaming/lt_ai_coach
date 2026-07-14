// Pure draft logic: turn order, action bookkeeping, and availability rules.
// No React, no side effects.

import type { DraftAction, DraftActionRecord, DraftMode, DraftSide, DraftState, DraftTurn } from "../types";
import { titleCase } from "./format";

export const emptyDraft: DraftState = { blueBans: [], redBans: [], bluePicks: [], redPicks: [], historyBlue: [], historyRed: [], actionLog: [] };

// The one recommendation-error message DraftBoard sets itself (not a raw
// backend error string) — kept as a constant so the display side can swap in
// a translated string for exactly this message. Raw backend errors pass
// through unchanged so their diagnostic details remain intact.
export const WAITING_FOR_CONTEXT_MESSAGE = "Waiting for live team context.";

// Swaps in the translated string for WAITING_FOR_CONTEXT_MESSAGE; any other
// message (a raw backend error) passes through untranslated.
export function translateRecommendationError(message: string, t: (key: string) => string): string {
  return message === WAITING_FOR_CONTEXT_MESSAGE ? t("draft.waitingForContext") : message;
}

export function calculateDraftTurn(draft: Pick<DraftState, "blueBans" | "redBans" | "bluePicks" | "redPicks">, bansPerSide = 3): DraftTurn {
  const banLimit = Math.max(1, Math.min(5, Math.round(bansPerSide)));
  const blueBans = Math.min(draft.blueBans.length, banLimit);
  const redBans = Math.min(draft.redBans.length, banLimit);
  if (blueBans < banLimit || redBans < banLimit) {
    const side: DraftSide = blueBans <= redBans && blueBans < banLimit ? "blue" : "red";
    const ordinal = (side === "blue" ? blueBans : redBans) + 1;
    const actionNumber = blueBans + redBans + 1;
    const totalActions = banLimit * 2;
    return { phase: "ban", side, ordinal, actionNumber, totalActions, label: `${titleCase(side)} ban ${ordinal}`, progress: `Ban ${actionNumber}/${totalActions}` };
  }

  const bluePicks = Math.min(draft.bluePicks.length, 5);
  const redPicks = Math.min(draft.redPicks.length, 5);
  const pickTurn = (side: DraftSide): DraftTurn => {
    const ordinal = (side === "blue" ? bluePicks : redPicks) + 1;
    const actionNumber = bluePicks + redPicks + 1;
    return { phase: "pick", side, ordinal, actionNumber, totalActions: 10, label: `${titleCase(side)} pick ${ordinal}`, progress: `Pick ${actionNumber}/10` };
  };
  if (bluePicks < 1) return pickTurn("blue");
  if (redPicks < 2) return pickTurn("red");
  if (bluePicks < 3) return pickTurn("blue");
  if (redPicks < 4) return pickTurn("red");
  if (bluePicks < 5) return pickTurn("blue");
  if (redPicks < 5) return pickTurn("red");
  const totalActions = banLimit * 2 + 10;
  return { phase: "complete", side: null, ordinal: 0, actionNumber: totalActions, totalActions, label: "Draft complete", progress: `${totalActions}/${totalActions} actions` };
}

// Returns a translation key (not English text) — call t() on the result.
export function draftActionLabelKey(action: DraftAction) { return { "blue-ban": "draft.action.blueBan", "red-ban": "draft.action.redBan", "blue-pick": "draft.action.bluePick", "red-pick": "draft.action.redPick", "history-blue": "draft.action.historyBlue", "history-red": "draft.action.historyRed" }[action]; }

// Builds the "Blue ban 1" / "Draft complete" style label from a DraftTurn's
// structured fields (not its English label/progress, which exist only for
// internal/test use). Shared by the full board and the compact overlay.
export function translateTurnLabel(turn: DraftTurn, t: (key: string) => string): string {
  if (turn.phase === "complete") return t("draft.complete");
  const sideLabel = turn.side === "blue" ? t("draft.side.blue") : t("draft.side.red");
  const phaseLabel = turn.phase === "ban" ? t("draft.phase.ban") : t("draft.phase.pick");
  return `${sideLabel} ${phaseLabel} ${turn.ordinal}`;
}

export function translateTurnProgress(turn: DraftTurn, t: (key: string) => string): string {
  if (turn.phase === "complete") return `${turn.actionNumber}/${turn.totalActions} ${t("draft.progress.actionsUnit")}`;
  const word = turn.phase === "ban" ? t("draft.progress.banWord") : t("draft.progress.pickWord");
  return `${word} ${turn.actionNumber}/${turn.totalActions}`;
}
export function currentDraftAction(action: DraftAction, championId: string): DraftActionRecord | null { const [side, actionType] = action.split("-"); if ((side !== "blue" && side !== "red") || (actionType !== "ban" && actionType !== "pick")) return null; return { side, actionType, championId } as DraftActionRecord; }
export function removeLatestAction(actionLog: DraftActionRecord[], championId: string) { let index = -1; for (let i = actionLog.length - 1; i >= 0; i--) if (actionLog[i].championId === championId) { index = i; break; } return index < 0 ? actionLog : actionLog.filter((_, i) => i !== index); }
export function actionTarget(draft: DraftState, action: DraftAction): string[] { return { "blue-ban": draft.blueBans, "red-ban": draft.redBans, "blue-pick": draft.bluePicks, "red-pick": draft.redPicks, "history-blue": draft.historyBlue, "history-red": draft.historyRed }[action]; }

// Returns a translation key (not English text) when unavailable, else null.
// Callers that only need the truthiness (e.g. useDraftStore) can ignore the
// key's meaning; callers that display it (DraftBoard) should call t() on it.
export function unavailableReason(championId: string, action: DraftAction, mode: DraftMode, draft: DraftState, bansPerSide = 3) {
  const banLimit = Math.max(1, Math.min(5, Math.round(bansPerSide)));
  const current = [...draft.blueBans, ...draft.redBans, ...draft.bluePicks, ...draft.redPicks];
  if ((action.includes("ban") || action.includes("pick")) && current.includes(championId)) return "draft.unavailable.alreadyUsed";
  if (mode === "fearless-hard" && (draft.historyBlue.includes(championId) || draft.historyRed.includes(championId))) return "draft.unavailable.fearlessHardHistory";
  if (action === "blue-ban" && draft.blueBans.length >= banLimit) return "draft.unavailable.blueBansFull";
  if (action === "red-ban" && draft.redBans.length >= banLimit) return "draft.unavailable.redBansFull";
  if (action === "blue-pick" && draft.bluePicks.length >= 5) return "draft.unavailable.bluePicksFull";
  if (action === "red-pick" && draft.redPicks.length >= 5) return "draft.unavailable.redPicksFull";
  if (action === "history-blue" && draft.historyBlue.includes(championId)) return "draft.unavailable.alreadyBlueHistory";
  if (action === "history-red" && draft.historyRed.includes(championId)) return "draft.unavailable.alreadyRedHistory";
  if (action === "blue-pick" && mode !== "normal") { if (draft.historyBlue.includes(championId)) return "draft.unavailable.blockedByBlueHistory"; if (mode === "fearless-hard" && draft.historyRed.includes(championId)) return "draft.unavailable.blockedByRedHistory"; }
  if (action === "red-pick" && mode !== "normal") { if (draft.historyRed.includes(championId)) return "draft.unavailable.blockedByRedHistory"; if (mode === "fearless-hard" && draft.historyBlue.includes(championId)) return "draft.unavailable.blockedByBlueHistory"; }
  return null;
}
