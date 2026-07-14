import { IconX } from "@tabler/icons-react";
import type { ChampionRoleStat } from "../types";
import { championTier, patchDirection, tierOrder, type TierContext } from "../lib/tiers";
import { formatPatchLabel, formatPatchValue } from "../lib/patchFormat";
import { ChampionPortraitView } from "./ChampionPortraitView";
import { RoleGlyph } from "./RoleGlyph";
import { useT } from "../stores/useI18nStore";

export function ChampionDetailPanel({ row, role, tiers, tierContext, onSetTier, onClose }: { row: ChampionRoleStat; role: string; tiers: Record<string, string>; tierContext: TierContext; onSetTier: (championId: string, tier: string) => void; onClose: () => void }) {
  const t = useT();
  const direction = patchDirection(row.patchImpact);
  const displayedTier = championTier(row, tiers, tierContext);
  return <aside className="champion-detail-panel" aria-label={`${row.championName} ${t("championDetail.detailsAria")}`}>
    <div className="detail-champion-heading">
      <ChampionPortraitView portrait={row.portrait} width={54} height={66} />
      <div><h3>{row.championName}</h3><span><RoleGlyph role={role} />{t(`role.${role}`)} · {displayedTier} {t("tiers.tierAriaSuffix")}</span>{direction !== "unchanged" && <span className={`detail-patch-chip ${direction}`}>{direction === "buff" ? "▲" : "▼"} {direction === "buff" ? t("championDetail.buffed") : t("championDetail.nerfed")}</span>}</div>
      <button type="button" className="icon-button detail-close" onClick={onClose} aria-label={t("championDetail.closeAria")}><IconX size={16} /></button>
    </div>

    <section className="patch-change-section"><span className="detail-label">{t("championDetail.changedThisPatch")}</span>
      {row.patchChanges.length ? <div className="patch-change-list">{row.patchChanges.map((change, index) => <div className="patch-change" key={`${change.asset}-${change.field}-${index}`}><span className={change.impact > 0 ? "buff" : change.impact < 0 ? "nerf" : "unchanged"}>{change.impact > 0 ? "▲" : change.impact < 0 ? "▼" : "–"}</span><span>{formatPatchLabel(change.asset, change.target, change.field, t)}</span><strong>{formatPatchValue(change.oldValue, change.field)}→{formatPatchValue(change.newValue, change.field)}</strong></div>)}</div> : <p className="no-patch-changes">{t("championDetail.noPatchChanges")}</p>}
    </section>

    <section className="tier-override-section"><span className="detail-label">{t("championDetail.tierOverrideLabel")}</span><div className="tier-override-buttons">
      {tierOrder.map((tier) => <button type="button" key={tier} className={tiers[row.championId] === tier ? "active" : ""} onClick={() => onSetTier(row.championId, tiers[row.championId] === tier ? "" : tier)} aria-pressed={tiers[row.championId] === tier}>{tier}</button>)}
    </div><small>{tiers[row.championId] ? t("championDetail.tierOverrideHint") : `${t("championDetail.rankedAutomaticallyAs")} ${displayedTier}.`}</small></section>
  </aside>;
}
