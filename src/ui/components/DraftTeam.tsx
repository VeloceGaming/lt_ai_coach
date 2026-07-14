// One team's full draft column: bans, picks, optional Fearless prior picks, and
// the projected-roles readout.

import type { DraftAction, DraftChampion, DraftSide, DraftState, TeamProjection } from "../types";
import { DraftSlots } from "./DraftSlots";
import { LineupProjection } from "./LineupProjection";

export function DraftTeam({ side, isUser, bans, picks, history, projection, champions, activeAction, isFearless, bansPerSide = 3, onRemove, onSlotClick }: { side: DraftSide; isUser: boolean; bans: string[]; picks: string[]; history: string[]; projection: TeamProjection | null; champions: Map<string, DraftChampion>; activeAction: DraftAction | null; isFearless: boolean; bansPerSide?: number; onRemove: (target: keyof DraftState, championId: string) => void; onSlotClick: (action: DraftAction) => void }) {
  const prefix = side === "blue" ? "blue" : "red";
  const bansTarget: keyof DraftState = side === "blue" ? "blueBans" : "redBans";
  const picksTarget: keyof DraftState = side === "blue" ? "bluePicks" : "redPicks";
  const histTarget: keyof DraftState = side === "blue" ? "historyBlue" : "historyRed";
  const banAction: DraftAction = `${prefix}-ban`;
  const pickAction: DraftAction = `${prefix}-pick`;
  const histAction: DraftAction = side === "blue" ? "history-blue" : "history-red";
  return <article className={`draft-team ${side}`}><h4>{side === "blue" ? "Blue" : "Red"} Team{isUser && <span>You</span>}</h4><DraftSlots label="Bans" ids={bans} limit={bansPerSide} champions={champions} slotAction={banAction} activeAction={activeAction} onRemove={(id) => onRemove(bansTarget, id)} onEmptySlotClick={() => onSlotClick(banAction)} /><DraftSlots label="Picks" ids={picks} limit={5} champions={champions} slotAction={pickAction} activeAction={activeAction} onRemove={(id) => onRemove(picksTarget, id)} onEmptySlotClick={() => onSlotClick(pickAction)} />{isFearless && <DraftSlots label="Prior picks" ids={history} champions={champions} slotAction={histAction} activeAction={activeAction} onRemove={(id) => onRemove(histTarget, id)} onEmptySlotClick={() => onSlotClick(histAction)} />}<LineupProjection projection={projection} /></article>;
}
