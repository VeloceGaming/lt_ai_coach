import type { ChampionRoleStat } from "../types";
import type { TierName } from "../lib/tiers";
import { ChampionTierTile } from "./ChampionTierTile";
import { useT } from "../stores/useI18nStore";

export function TierRow({ tier, rows, roleFor, selectedId, onSelect }: { tier: TierName; rows: ChampionRoleStat[]; roleFor: (row: ChampionRoleStat) => string; selectedId: string | null; onSelect: (row: ChampionRoleStat) => void }) {
  const t = useT();
  return <section className={`tier-row tier-${tier.toLowerCase()}`} aria-label={`${tier} ${t("tiers.tierAriaSuffix")}`}>
    <div className="tier-rank-label">{tier}</div>
    <div className="tier-grid">{rows.length ? rows.map((row) => <ChampionTierTile key={row.championId} row={row} role={roleFor(row)} selected={selectedId === row.championId} onSelect={() => onSelect(row)} />) : <span className="tier-empty">{t("tiers.noMatchingChampions")}</span>}</div>
  </section>;
}
