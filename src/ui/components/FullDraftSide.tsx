import { IconDots } from "@tabler/icons-react";
import type { DraftAction, DraftChampion, DraftLineup, DraftSide, DraftState } from "../types";
import { ChampionPortraitView } from "./ChampionPortraitView";
import { RoleGlyph } from "./RoleGlyph";
import { useT } from "../stores/useI18nStore";

// The five pick slots map to the five roles top→support by position. The live
// lineup from the bridge is keyed by that same role, so each slot's player name
// is an exact lookup — no guessing from the picked champion involved.
const ROLE_ORDER = ["top", "jungle", "mid", "bot", "support"] as const;

type FullDraftSideProps = {
  side: DraftSide;
  part: "bans" | "picks";
  isUser: boolean;
  bans: string[];
  picks: string[];
  champions: Map<string, DraftChampion>;
  activeAction: DraftAction | null;
  onRemove: (target: keyof DraftState, championId: string) => void;
  onSlotClick: (action: DraftAction) => void;
  lineup?: DraftLineup | null;
  athleteNames?: Map<number, string>;
};

export function FullDraftSide({ side, part, isUser, bans, picks, champions, activeAction, onRemove, onSlotClick, lineup, athleteNames }: FullDraftSideProps) {
  const t = useT();
  const sideLabel = t(`draft.side.${side}`);
  const banAction: DraftAction = `${side}-ban`;
  const pickAction: DraftAction = `${side}-pick`;
  const bansTarget: keyof DraftState = side === "blue" ? "blueBans" : "redBans";
  const picksTarget: keyof DraftState = side === "blue" ? "bluePicks" : "redPicks";
  const banSlots = [...bans, ...Array(Math.max(0, 3 - bans.length)).fill(null)] as Array<string | null>;
  const pickSlots = [...picks, ...Array(Math.max(0, 5 - picks.length)).fill(null)] as Array<string | null>;
  const activeBanIndex = activeAction === banAction ? bans.length : -1;
  const activePickIndex = activeAction === pickAction ? picks.length : -1;
  const names = athleteNames ?? new Map<number, string>();

  // The player assigned to a slot's role, if a live lineup is present.
  const athleteForSlot = (index: number): string | null => {
    if (!lineup) return null;
    const id = (lineup as Record<string, number | null | undefined>)[ROLE_ORDER[index]];
    return typeof id === "number" ? names.get(id) ?? null : null;
  };

  // Bans part: a compact row (both teams share one strip below the picks).
  if (part === "bans") {
    return <div className={`full-draft-bans ${side}${isUser ? " is-user" : ""}`} aria-label={`${sideLabel} ${t("draft.aria.bans")}`}>
      <div className="full-side-bans">
        {banSlots.map((id, index) => id
          ? <button type="button" key={`${id}-${index}`} className="full-ban-slot filled" title={`${t("draft.removeTooltip")} ${champions.get(id)?.name ?? id}`} onClick={() => onRemove(bansTarget, id)}><ChampionPortraitView portrait={champions.get(id)?.portrait ?? null} width={34} height={46} /></button>
          : <button type="button" key={`ban-${index}`} className={`full-ban-slot${index === activeBanIndex ? " active" : ""}`} aria-label={`${sideLabel} ${t("draft.aria.ban")} ${index + 1}`} onClick={() => onSlotClick(banAction)}>{index === activeBanIndex ? <IconDots size={17} stroke={2.2} /> : index + 1}</button>)}
      </div>
    </div>;
  }

  // Picks part: the tall column flanking the coach column. Each slot shows the
  // role's player (live) and, once picked, the champion.
  return <section className={`full-draft-side ${side}${isUser ? " is-user" : ""}`} aria-label={`${sideLabel} ${t("draft.aria.side")}`}>
    <div className="full-side-picks" aria-label={`${sideLabel} ${t("draft.aria.picks")}`}>
      {pickSlots.map((id, index) => {
        const champion = id ? champions.get(id) : undefined;
        const role = ROLE_ORDER[index];
        const roleLabel = t(`role.${role}`);
        const athlete = athleteForSlot(index);
        const active = index === activePickIndex;
        return id
          ? <button type="button" key={`${id}-${index}`} className="full-pick-slot filled" title={`${t("draft.removeTooltip")} ${champion?.name ?? id}`} onClick={() => onRemove(picksTarget, id)}>
              <ChampionPortraitView portrait={champion?.portrait ?? null} width={58} height={78} />
              <span>
                <strong>{athlete ?? champion?.name ?? id}</strong>
                <span className="pick-sub">
                  {athlete && <span className="pick-champ">{champion?.name ?? id}</span>}
                  <span className="pick-role"><RoleGlyph role={role} />{roleLabel}</span>
                </span>
              </span>
            </button>
          : <button type="button" key={`pick-${index}`} className={`full-pick-slot empty${active ? " active" : ""}`} onClick={() => onSlotClick(pickAction)}>
              <span className="empty-pick-portrait">{active ? <IconDots size={23} stroke={2.2} /> : <RoleGlyph role={role} />}</span>
              <span>
                <strong>{active ? t("draft.picking") : athlete ?? roleLabel}</strong>
                <span className="pick-sub"><span className="pick-role"><RoleGlyph role={role} />{roleLabel}</span></span>
              </span>
            </button>;
      })}
    </div>
  </section>;
}
