import type { Recommendation } from "../types";
import { ChampionPortraitView } from "./ChampionPortraitView";
import { RoleGlyph } from "./RoleGlyph";
import { ReasonList } from "./ReasonList";
import { FadeOnChange } from "../motion/FadeOnChange";
import { useT } from "../stores/useI18nStore";

export function FullDraftRecommendations({ rows, loadingLabel, selectedId, onSelect, limit = 4, ranks }: { rows: Recommendation[]; loadingLabel?: string; selectedId: string | null; onSelect: (championId: string) => void; limit?: number; ranks?: Map<string, number> }) {
  const t = useT();
  const visible = rows.slice(0, limit);
  // Cross-fade only when the recommended set changes, not when selection changes.
  const signature = visible.map((row) => row.championId).join("|");
  return <section className="full-recommendations" aria-label={t("draft.recAria")}>
    {visible.length ? <FadeOnChange changeKey={signature} className="full-recommendation-grid">{visible.map((row, index) => {
      const selected = selectedId === row.championId;
      return <article key={row.championId} className={`full-recommendation-card${selected ? " selected" : ""}`} role="button" tabIndex={0} aria-pressed={selected} onClick={() => onSelect(row.championId)} onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); onSelect(row.championId); } }}>
        <div className="recommendation-portrait">
          <ChampionPortraitView portrait={row.portrait} width={92} height={158} scaleMode="champion" fixedCenter />
          <span className="recommendation-rank">#{ranks?.get(row.championId) ?? index + 1}</span>
        </div>
        <div className="recommendation-copy">
          <div className="recommendation-heading"><div><strong>{row.championName}</strong>{row.suggestedRole && <span><RoleGlyph role={row.suggestedRole} />{t(`role.${row.suggestedRole}`)}</span>}</div><span className="fit-score">{row.score.toFixed(0)}</span></div>
          <div className="recommendation-meta"><span>{row.adjustedWinRate.toLocaleString(undefined, { style: "percent", minimumFractionDigits: 1, maximumFractionDigits: 1 })} {t("draft.adjustedWinRate")}</span></div>
          <ReasonList reasons={row.reasons} />
        </div>
      </article>;
    })}</FadeOnChange> : <div className="recommendation-empty">{loadingLabel ?? t("draft.noRecommendations")}</div>}
  </section>;
}
