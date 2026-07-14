// Composition read-out for your drafted side: damage-type mix, key coverage
// (frontline / CC / sustain), and gap warnings. Built from game-native rawTags
// (see lib/comp.ts). Advisory only — it never changes recommendations.

import { IconAlertTriangle } from "@tabler/icons-react";
import { analyzeComp } from "../lib/comp";
import { useOverlayStore } from "../stores/useOverlayStore";
import { useT } from "../stores/useI18nStore";

export function CompAnalysis({ picks }: { picks: string[] }) {
  const t = useT();
  const liveTags = useOverlayStore((s) => s.championTags);
  const comp = analyzeComp(picks, liveTags);
  const sustain = (comp.counts["Heal"] ?? 0) + (comp.counts["Shield"] ?? 0);
  const damageTotal = comp.physical + comp.magic;

  return <section className="comp-analysis" aria-label={t("comp.aria")}>
    <div className="comp-analysis-head">
      <span className="comp-analysis-title">{t("comp.title")}</span>
      <span className="comp-analysis-count">{comp.pickCount}/5</span>
    </div>

    {comp.pickCount === 0 ? <p className="comp-analysis-empty">{t("comp.emptyDesc")}</p> : <>
      <div className="comp-damage">
        <div className="comp-damage-bar" role="img" aria-label={`${comp.physical} ${t("comp.physical")}, ${comp.magic} ${t("comp.magic")}`}>
          {damageTotal > 0 ? <>
            <span className="comp-damage-seg ad" style={{ flexGrow: comp.physical }} />
            <span className="comp-damage-seg ap" style={{ flexGrow: comp.magic }} />
          </> : <span className="comp-damage-seg empty" />}
        </div>
        <div className="comp-damage-legend"><span><i className="ad" />{t("comp.physical")} {comp.physical}</span><span><i className="ap" />{t("comp.magic")} {comp.magic}</span></div>
      </div>

      <div className="comp-coverage">
        <CoverageChip label={t("comp.frontline")} value={comp.counts["Tank"] ?? 0} />
        <CoverageChip label={t("comp.cc")} value={comp.counts["CC"] ?? 0} />
        <CoverageChip label={t("comp.sustain")} value={sustain} />
      </div>

      {comp.gaps.length > 0 && <ul className="comp-gaps">{comp.gaps.map((gap) => <li key={gap}><IconAlertTriangle size={13} />{t(gap)}</li>)}</ul>}
    </>}
  </section>;
}

function CoverageChip({ label, value }: { label: string; value: number }) {
  return <span className={`comp-chip${value > 0 ? " present" : " absent"}`}>{label}<b>{value}</b></span>;
}
