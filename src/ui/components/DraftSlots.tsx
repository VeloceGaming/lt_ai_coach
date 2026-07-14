// A labelled row of draft slots (bans, picks, or prior picks) for one team.

// NOTE: unused (superseded by FullDraftSide) — nothing imports this component.
// Kept compiling rather than translated; see docs/ui-modernization-notes.md.
import type { DraftAction, DraftChampion } from "../types";
import { draftActionLabelKey } from "../lib/draft";
import { ChampionPortraitView } from "./ChampionPortraitView";

export function DraftSlots({ label, ids, limit, champions, slotAction, activeAction, onRemove, onEmptySlotClick }: { label: string; ids: string[]; limit?: number; champions: Map<string, DraftChampion>; slotAction: DraftAction; activeAction: DraftAction | null; onRemove: (championId: string) => void; onEmptySlotClick: () => void }) {
  const isActive = activeAction === slotAction;
  const slots = limit ? [...ids, ...Array(Math.max(0, limit - ids.length)).fill(null)] : (ids.length ? ids : [null]);
  return <div className={`draft-slot-group${isActive ? " slot-group-active" : ""}`}><span>{label}</span><div>{slots.map((id, index) => id ? <button type="button" className="draft-slot filled" key={`${id}-${index}`} title="Click to remove" onClick={() => onRemove(id)}><ChampionPortraitView portrait={champions.get(id)?.portrait ?? null} /><small>{champions.get(id)?.name ?? id}</small></button> : <button type="button" className={`draft-slot empty${isActive ? " active-target" : ""}`} key={`empty-${index}`} title={`Set action: ${draftActionLabelKey(slotAction)}`} onClick={onEmptySlotClick} />)}</div></div>;
}
