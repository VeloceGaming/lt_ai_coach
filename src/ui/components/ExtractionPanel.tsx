// Summary of the most recent game import (counts + freshness).

import type { ImportSummary } from "../types";
import { formatAge } from "../lib/format";
import { useT } from "../stores/useI18nStore";

export function ExtractionPanel({ summary }: { summary: ImportSummary }) {
  const t = useT();
  const ageSeconds = summary.exportedAtUnix ? Math.max(0, Date.now() / 1000 - summary.exportedAtUnix) : null;
  const freshness = ageSeconds === null ? "" : `${t("extraction.exportedPrefix")} ${formatAge(ageSeconds)} ${t("extraction.agoSuffix")}`;
  return <section className="extraction-panel"><div className="panel-heading"><div><p className="section-label">{t("extraction.eyebrow")}</p><h3>{summary.gameLabel ?? t("extraction.liveGameFallback")}</h3></div><span>{freshness}</span></div><div className="metric-grid"><Metric label={t("extraction.metric.champions")} value={summary.enabledChampions} /><Metric label={t("extraction.metric.teams")} value={summary.teams} /><Metric label={t("extraction.metric.players")} value={summary.players} /><Metric label={t("extraction.metric.matches")} value={summary.matches} /><Metric label={t("extraction.metric.tournament")} value={summary.tournamentMatches} /><Metric label={t("extraction.metric.solo")} value={summary.soloMatches} /><Metric label={t("extraction.metric.picks")} value={summary.picks} /><Metric label={t("extraction.metric.bans")} value={summary.bans} /><Metric label={t("extraction.metric.patchChanges")} value={summary.patchChanges} /><Metric label={t("extraction.metric.newChampions")} value={summary.patchAdditions} /></div><p className="output-path" title={summary.databasePath}>{t("extraction.dbPrefix")} {summary.databasePath}</p></section>;
}

function Metric({ label, value }: { label: string; value: number | null }) {
  const t = useT();
  return <div><span>{label}</span><strong>{value ?? t("stats.notAvailable")}</strong></div>;
}
