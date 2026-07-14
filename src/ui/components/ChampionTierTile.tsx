import type { ChampionRoleStat } from "../types";
import { patchDirection } from "../lib/tiers";
import { ChampionPortraitView } from "./ChampionPortraitView";
import { RoleGlyph } from "./RoleGlyph";
import { useT } from "../stores/useI18nStore";

export function ChampionTierTile({ row, role, selected, onSelect }: { row: ChampionRoleStat; role: string; selected: boolean; onSelect: () => void }) {
  const t = useT();
  const direction = patchDirection(row.patchImpact);
  return <button type="button" className={`tier-champion-tile${selected ? " selected" : ""}`} onClick={onSelect} aria-pressed={selected} title={`${row.championName} · ${t(`role.${role}`)}`}>
    <span className="tier-portrait-wrap">
      <ChampionPortraitView portrait={row.portrait} width={64} height={78} scaleMode="champion" />
      <span className="tile-role"><RoleGlyph role={role} /></span>
      {direction !== "unchanged" && <span className={`patch-badge ${direction}`} aria-label={direction === "buff" ? t("championDetail.buffed") : t("championDetail.nerfed")}><span aria-hidden="true">{direction === "buff" ? "▲" : "▼"}</span></span>}
    </span>
    <span className="tier-champion-name">{row.championName}</span>
  </button>;
}
