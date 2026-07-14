// Champion intelligence screen: OP.GG-style index + detail panel.

import { useEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent, type RefObject } from "react";
import { IconChartBar, IconLayoutGrid, IconSearch, IconX } from "@tabler/icons-react";
import type { ChampionRoleStat, DraftCatalog, DraftChampion, RoleStatistics } from "../types";
import { championTags } from "../lib/comp";
import { formatNumber, formatPercent } from "../lib/format";
import { formatPatchLabel, formatPatchValue } from "../lib/patchFormat";
import { useOverlayStore } from "../stores/useOverlayStore";
import { useT } from "../stores/useI18nStore";
import { ChampionPortraitView } from "./ChampionPortraitView";
import { RoleGlyph } from "./RoleGlyph";

const roles = ["all", "top", "jungle", "mid", "bot", "support"] as const;
type RoleFilter = (typeof roles)[number];
const detailRoles = ["top", "jungle", "mid", "bot", "support"] as const;
type DetailRole = (typeof detailRoles)[number];
const MIN_ROLE_SAMPLE_GAMES = 5;
type MetricKey = "winRate" | "damage" | "damagePerGold" | "cs" | "gold" | "tanking" | "healing" | "kda" | "rating";

// Translation keys (not English text) — call t() on each `.key` via stats.metric.*.
const METRICS: { key: MetricKey }[] = [
  { key: "winRate" }, { key: "damage" }, { key: "damagePerGold" }, { key: "cs" }, { key: "gold" },
  { key: "tanking" }, { key: "healing" }, { key: "kda" }, { key: "rating" },
];

type ChampionAtlasEntry = { champion: DraftChampion; row: ChampionRoleStat | null; roleRow: ChampionRoleStat | null };
type PortraitFlight = { championId: string; fromX: number; fromY: number; portrait: DraftChampion["portrait"] };
type DetailRoleOption = { role: DetailRole; row: ChampionRoleStat | null; enabled: boolean };

export function StatisticsPanel({ statistics, draftCatalog, tiers, focusChampion, onFocusChampionHandled, onSetTier, onSetChampionOverride }: { statistics: RoleStatistics; draftCatalog: DraftCatalog; tiers: Record<string, string>; focusChampion?: { championId: string; role: string } | null; onFocusChampionHandled?: () => void; onSetTier: (championId: string, tier: string) => void; onSetChampionOverride: (championId: string, name: string, portraitPath: string, nameChanged: boolean, portraitPathChanged: boolean) => Promise<void> }) {
  const t = useT();
  const [role, setRole] = useState<RoleFilter>("all");
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detailRole, setDetailRole] = useState<DetailRole | null>(null);
  const [portraitFlight, setPortraitFlight] = useState<PortraitFlight | null>(null);
  const [flightLanded, setFlightLanded] = useState(false);
  const atlasLayoutRef = useRef<HTMLDivElement | null>(null);
  const detailRef = useRef<HTMLElement | null>(null);
  const liveTags = useOverlayStore((state) => state.championTags);
  const overallByChampion = useMemo(() => new Map(statistics.overallRows.map((row) => [row.championId, row])), [statistics.overallRows]);

  const bestRoleByChampion = useMemo(() => {
    const map = new Map<string, ChampionRoleStat>();
    for (const row of statistics.roleRows) {
      const current = map.get(row.championId);
      if (!current || row.games > current.games) map.set(row.championId, row);
    }
    return map;
  }, [statistics.roleRows]);

  const atlasEntries = useMemo(() => {
    return draftCatalog.champions.map((champion) => {
      const row = overallByChampion.get(champion.id) ?? null;
      const bestRole = bestRoleByChampion.get(champion.id) ?? null;
      const roleRow = role === "all"
        ? bestRole
        : statistics.roleRows.find((candidate) => candidate.championId === champion.id && candidate.role === role) ?? null;
      return { champion, row, roleRow } satisfies ChampionAtlasEntry;
    });
  }, [bestRoleByChampion, draftCatalog.champions, overallByChampion, role, statistics.roleRows]);

  const visibleEntries = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return atlasEntries.filter((entry) => {
      if (role !== "all" && !entry.roleRow) return false;
      if (!needle) return true;
      const tags = championTags(entry.champion.id, liveTags).join(" ").toLowerCase();
      const statName = entry.row?.championName ?? "";
      return entry.champion.name.toLowerCase().includes(needle)
        || statName.toLowerCase().includes(needle)
        || entry.champion.id.toLowerCase().includes(needle)
        || tags.includes(needle);
    });
  }, [atlasEntries, liveTags, query, role]);

  const selectedEntry = selectedId
    ? visibleEntries.find((entry) => entry.champion.id === selectedId)
      ?? atlasEntries.find((entry) => entry.champion.id === selectedId)
      ?? null
    : null;
  const selectedRoleOptions = useMemo<DetailRoleOption[]>(() => {
    if (!selectedEntry) return [];
    return detailRoles.map((candidateRole) => {
      const row = statistics.roleRows.find((candidate) => candidate.championId === selectedEntry.champion.id && candidate.role === candidateRole) ?? null;
      return { role: candidateRole, row, enabled: Boolean(row && row.games >= MIN_ROLE_SAMPLE_GAMES) };
    });
  }, [selectedEntry, statistics.roleRows]);
  const selectedRole = detailRole ?? "all";
  const detailRow = selectedRole === "all"
    ? selectedEntry?.row ?? null
    : selectedRoleOptions.find((option) => option.role === detailRole)?.row ?? null;
  const selectedTags = selectedEntry ? championTags(selectedEntry.champion.id, liveTags) : [];

  const roleAverage = useMemo(() => {
    const rows = selectedRole === "all" ? statistics.overallRows : statistics.roleRows.filter((row) => row.role === selectedRole);
    return averageStats(rows);
  }, [selectedRole, statistics.overallRows, statistics.roleRows]);

  useEffect(() => {
    if (!selectedId) return;
    function onPointerDown(event: PointerEvent) {
      const target = event.target;
      if (!(target instanceof Element)) return;
      if (detailRef.current?.contains(target)) return;
      if (target.closest("button")) return;
      closeIntel();
    }
    document.addEventListener("pointerdown", onPointerDown, true);
    return () => document.removeEventListener("pointerdown", onPointerDown, true);
  }, [selectedId]);

  useEffect(() => {
    if (!focusChampion) return;
    focusChampionDetail(focusChampion.championId, focusChampion.role);
    onFocusChampionHandled?.();
  }, [focusChampion]);

  if (atlasEntries.length === 0) {
    return <section className="statistics-panel stats-intel"><div className="screen-empty"><strong>{t("stats.noStatsYet")}</strong></div></section>;
  }

  function selectChampion(event: MouseEvent<HTMLButtonElement>, entry: ChampionAtlasEntry) {
    if (selectedId) return;
    const layoutRect = atlasLayoutRef.current?.getBoundingClientRect();
    const cardRect = event.currentTarget.getBoundingClientRect();
    setFlightLanded(false);
    setPortraitFlight({
      championId: entry.champion.id,
      fromX: layoutRect ? cardRect.left - layoutRect.left + (cardRect.width - 64) / 2 : 0,
      fromY: layoutRect ? cardRect.top - layoutRect.top + 7 : 0,
      portrait: entry.champion.portrait ?? entry.row?.portrait ?? null,
    });
    const defaultRole = detailRoles
      .map((candidateRole) => statistics.roleRows.find((row) => row.championId === entry.champion.id && row.role === candidateRole) ?? null)
      .filter((row): row is ChampionRoleStat => Boolean(row && row.games >= MIN_ROLE_SAMPLE_GAMES))
      .sort((left, right) => right.games - left.games)[0]?.role as DetailRole | undefined;
    setDetailRole(defaultRole ?? null);
    setSelectedId(entry.champion.id);
  }

  function focusChampionDetail(championId: string, role: string) {
    const entry = atlasEntries.find((candidate) => candidate.champion.id === championId);
    if (!entry) return;
    const defaultRole = detailRoles.includes(role as DetailRole)
      ? role as DetailRole
      : detailRoles
      .map((candidateRole) => statistics.roleRows.find((row) => row.championId === entry.champion.id && row.role === candidateRole) ?? null)
      .filter((row): row is ChampionRoleStat => Boolean(row && row.games >= MIN_ROLE_SAMPLE_GAMES))
      .sort((left, right) => right.games - left.games)[0]?.role as DetailRole | undefined;
    setPortraitFlight(null);
    setFlightLanded(false);
    setDetailRole(role === "all" ? null : defaultRole ?? null);
    setSelectedId(entry.champion.id);
  }

  function closeIntel() {
    setSelectedId(null);
    setDetailRole(null);
    setPortraitFlight(null);
    setFlightLanded(false);
  }

  const flightStyle = portraitFlight
    ? {
        "--portrait-from-x": `${portraitFlight.fromX}px`,
        "--portrait-from-y": `${portraitFlight.fromY}px`,
      } as CSSProperties
    : undefined;

  return <section className="statistics-panel stats-intel">
    <div className="panel-heading stats-intel-heading">
      <div><p className="section-label">{t("stats.eyebrow")}</p><h3>{t("stats.title")}</h3></div>
      <span>{t("stats.patchPrefix")} {statistics.currentPatch} · {statistics.totalMatches} {t("stats.matchesWord")} · {visibleEntries.length} {t("stats.shownWord")}</span>
    </div>

    <div ref={atlasLayoutRef} className={`stats-atlas-layout${selectedEntry ? " has-selection" : ""}`}>
      <main className="stats-atlas">
        <div className="stats-index-tools">
          <label className="tier-search stats-search"><IconSearch size={15} /><input type="search" placeholder={t("stats.searchPlaceholder")} value={query} onChange={(event) => setQuery(event.target.value)} /></label>
          <div className="tier-role-filters stats-role-filters" aria-label={t("stats.roleFilterAria")}>
            {roles.map((value) => <button type="button" key={value} className={role === value ? "active" : ""} aria-pressed={role === value} title={value === "all" ? t("draft.allRolesTitle") : t(`role.${value}`)} onClick={() => setRole(value)}>{value === "all" ? <IconLayoutGrid size={15} /> : <RoleGlyph role={value} />}</button>)}
          </div>
        </div>
        <div className="stats-champion-grid">
          {visibleEntries.map((entry) => {
            const row = entry.roleRow ?? entry.row;
            const selected = selectedEntry?.champion.id === entry.champion.id;
            return <button type="button" key={entry.champion.id} className={`stats-champion-card${selected ? " active" : ""}${row ? "" : " low-data"}${selectedEntry && !selected ? " faded" : ""}`} onClick={(event) => selectChampion(event, entry)}>
              <ChampionPortraitView portrait={entry.champion.portrait ?? row?.portrait ?? null} width={64} height={78} />
              <span className="stats-champion-main"><strong>{entry.champion.name}</strong></span>
            </button>;
          })}
        </div>
      </main>

      {selectedEntry && <button type="button" className="stats-overlay-dismiss" aria-label={t("stats.closeIntelAria")} onClick={closeIntel} />}
      {selectedEntry && <ChampionIntelDetail detailRef={detailRef} champion={selectedEntry.champion} row={detailRow} role={selectedRole} roleOptions={selectedRoleOptions} onRoleChange={setDetailRole} tags={selectedTags} patch={statistics.currentPatch} tiers={tiers} onSetTier={onSetTier} onSetChampionOverride={onSetChampionOverride} roleAverage={roleAverage} onClose={closeIntel} />}
      {portraitFlight && selectedEntry && !flightLanded && <div key={`${portraitFlight.championId}-${portraitFlight.fromX}-${portraitFlight.fromY}`} className="stats-selected-portrait-flight" style={flightStyle} onAnimationEnd={() => setFlightLanded(true)}>
        <ChampionPortraitView portrait={portraitFlight.portrait} width={64} height={78} />
      </div>}
    </div>
  </section>;
}

function ChampionIntelDetail({ detailRef, champion, row, role, roleOptions, onRoleChange, tags, patch, tiers, onSetTier, onSetChampionOverride, roleAverage, onClose }: { detailRef: RefObject<HTMLElement | null>; champion: DraftChampion; row: ChampionRoleStat | null; role: string; roleOptions: DetailRoleOption[]; onRoleChange: (role: DetailRole | null) => void; tags: string[]; patch: string; tiers: Record<string, string>; onSetTier: (championId: string, tier: string) => void; onSetChampionOverride: (championId: string, name: string, portraitPath: string, nameChanged: boolean, portraitPathChanged: boolean) => Promise<void>; roleAverage: StatAverages; onClose: () => void }) {
  const t = useT();
  const [metric, setMetric] = useState<MetricKey>("winRate");
  const [editOpen, setEditOpen] = useState(false);
  const portrait = champion.portrait ?? row?.portrait ?? null;
  const championName = champion.name || row?.championName || champion.id;
  const damagePerGold = row ? ratioOrNull(row.avgDamage, row.avgGold) : null;
  const suggestedRole = [...roleOptions]
    .filter((option) => option.enabled && option.row)
    .sort((left, right) => right.row!.games - left.row!.games)[0];

  return <aside ref={detailRef} className="stats-detail" aria-label={`${championName} ${t("stats.champIntelAria")}`}>
    <div className="stats-detail-header">
      <div className="stats-identity-portrait">
        <button type="button" onClick={() => setEditOpen(true)} title={t("stats.editIdentity")}>
          <ChampionPortraitView portrait={portrait} width={64} height={78} />
        </button>
        {editOpen && <ChampionOverridePopover champion={champion} portrait={portrait} onClose={() => setEditOpen(false)} onSave={onSetChampionOverride} />}
      </div>
      <div>
        <div className="stats-title-line"><h3>{championName}</h3><span>{role !== "all" && <RoleGlyph role={role} />}{role === "all" ? t("draft.allRolesTitle") : t(`role.${role}`)} · {row ? tiers[champion.id] || t("stats.autoTier") : t("stats.lowSample")}</span></div>
        <div className="stats-tag-row">{tags.length ? tags.map((tag) => <span key={tag}>{tag}</span>) : <span>{t("stats.noRawTags")}</span>}<span>{champion.id}</span></div>
        <div className="stats-detail-role-switcher" aria-label={t("stats.detailRoleSwitcherAria")}>
          {roleOptions.map((option) => {
            const sampleLabel = option.row ? t("stats.roleGames", { games: option.row.games }) : t("stats.noRoleData");
            return <button type="button" key={option.role} className={role === option.role ? "active" : ""} disabled={!option.enabled} aria-pressed={role === option.role} title={`${t(`role.${option.role}`)} · ${sampleLabel}`} onClick={() => onRoleChange(role === option.role ? null : option.role)}><RoleGlyph role={option.role} label={t(`role.${option.role}`)} /></button>;
          })}
        </div>
        {suggestedRole && <div className="stats-suggested-role"><span>{t("stats.suggestedRole")}:</span><RoleGlyph role={suggestedRole.role} /><span>{t(`role.${suggestedRole.role}`)}</span></div>}
      </div>
      <select value={tiers[champion.id] ?? ""} onChange={(event) => onSetTier(champion.id, event.target.value)} title={t("stats.manualTierTooltip")} disabled={!row}>
        {["", "S", "A", "C", "D", "F"].map((tier) => <option key={tier} value={tier}>{tier || t("stats.autoOption")}</option>)}
      </select>
      <button type="button" className="stats-detail-close" onClick={onClose} aria-label={t("stats.closeIntelAria")}><IconX size={16} /></button>
    </div>

    {!row ? <div className="stats-low-data">
      <strong>{t("stats.intelLockedTitle")}</strong>
      <p>{t("stats.intelLockedDesc")}</p>
    </div> : <>
    <div className="stats-summary-grid">
      <StatCard label={t("stats.adjustedWR")} value={formatPercent(row.adjustedWinRate)} sub={`${row.games} ${t("stats.gamesWord")} · ${formatPercent(row.confidence)} ${t("stats.confWord")}`} />
      <StatCard label={t("stats.rawWR")} value={formatPercent(row.winRate)} sub={`${row.wins}/${row.games} ${t("stats.winsWord")}`} />
      <StatCard label={t("stats.source")} value={`T${row.tournamentGames}/S${row.soloGames}`} sub={t("stats.tournamentSoloQueue")} />
      <StatCard label={t("stats.patchPrefix")} value={row.currentPatchGames.toString()} sub={`${t("stats.currentPatchPrefix")} ${patch}`} />
      <StatCard label={t("stats.metric.damage")} value={formatNumber(row.avgDamage, 0)} sub={compareToAverage(row.avgDamage, roleAverage.damage, t)} />
      <StatCard label={t("stats.metric.damagePerGold")} value={formatNumber(damagePerGold, 2)} sub={compareToAverage(damagePerGold, roleAverage.damagePerGold, t)} />
      <StatCard label={t("stats.metric.cs")} value={formatNumber(row.avgCs, 0)} sub={compareToAverage(row.avgCs, roleAverage.cs, t)} />
      <StatCard label={t("stats.metric.gold")} value={formatNumber(row.avgGold, 0)} sub={compareToAverage(row.avgGold, roleAverage.gold, t)} />
      <StatCard label={t("stats.metric.tanking")} value={formatNumber(row.avgTanking, 0)} sub={compareToAverage(row.avgTanking, roleAverage.tanking, t)} />
      <StatCard label={t("stats.metric.healing")} value={formatNumber(row.avgHealing, 0)} sub={compareToAverage(row.avgHealing, roleAverage.healing, t)} />
      <StatCard label={`${t("stats.metric.kda")} / ${t("stats.metric.rating")}`} value={`${formatNumber(row.kda, 2)} / ${formatNumber(row.avgRating, 1)}`} sub={compareToAverage(row.avgRating, roleAverage.rating, t)} />
    </div>

    <section className="stats-chart-panel">
      <div className="stats-chart-head"><span className="detail-label">{t("stats.interactiveMetricView")}</span><div>{METRICS.map((item) => <button type="button" key={item.key} className={metric === item.key ? "active" : ""} onClick={() => setMetric(item.key)}>{t(`stats.metric.${item.key}`)}</button>)}</div></div>
      <MetricChart metric={metric} row={row} currentPatch={patch} />
    </section>

    <section className="stats-breakdown-grid">
      <PerformanceBar label={t("stats.metric.damage")} value={row.avgDamage} average={roleAverage.damage} />
      <PerformanceBar label={t("stats.metric.damagePerGold")} value={damagePerGold} average={roleAverage.damagePerGold} digits={2} />
      <PerformanceBar label={t("stats.metric.cs")} value={row.avgCs} average={roleAverage.cs} />
      <PerformanceBar label={t("stats.metric.gold")} value={row.avgGold} average={roleAverage.gold} />
      <PerformanceBar label={t("stats.metric.tanking")} value={row.avgTanking} average={roleAverage.tanking} />
      <PerformanceBar label={t("stats.metric.healing")} value={row.avgHealing} average={roleAverage.healing} />
      <PerformanceBar label={t("stats.metric.kda")} value={row.kda} average={roleAverage.kda} digits={2} />
      <PerformanceBar label={t("stats.metric.rating")} value={row.avgRating} average={roleAverage.rating} digits={2} />
    </section>

    <section className="patch-change-section stats-patch-section"><span className="detail-label">{t("stats.patchStatChanges")}</span>
      {row.patchChanges.length ? <div className="patch-change-list">{row.patchChanges.map((change, index) => <div className="patch-change" key={`${change.asset}-${change.field}-${index}`}><span className={change.impact > 0 ? "buff" : change.impact < 0 ? "nerf" : "unchanged"}>{change.impact > 0 ? "▲" : change.impact < 0 ? "▼" : "–"}</span><span>{formatPatchLabel(change.asset, change.target, change.field, t)}</span><strong>{formatPatchValue(change.oldValue, change.field)}→{formatPatchValue(change.newValue, change.field)}</strong></div>)}</div> : <p className="no-patch-changes">{t("championDetail.noPatchChanges")}</p>}
    </section>
    </>}
  </aside>;
}

function ChampionOverridePopover({ champion, portrait, onClose, onSave }: { champion: DraftChampion; portrait: DraftChampion["portrait"]; onClose: () => void; onSave: (championId: string, name: string, portraitPath: string, nameChanged: boolean, portraitPathChanged: boolean) => Promise<void> }) {
  const t = useT();
  const [name, setName] = useState(champion.name);
  const [portraitPath, setPortraitPath] = useState((portrait?.path ?? "").replace(/^\//, ""));
  const [saving, setSaving] = useState(false);
  async function save(nextName = name, nextPortraitPath = portraitPath) {
    setSaving(true);
    try {
      await onSave(
        champion.id,
        nextName.trim(),
        nextPortraitPath.trim(),
        nextName.trim() !== champion.name,
        nextPortraitPath.trim() !== (portrait?.path ?? "").replace(/^\//, ""),
      );
      onClose();
    } finally {
      setSaving(false);
    }
  }
  return <div className="champion-override-popover" role="dialog" aria-label={t("stats.editIdentity")}>
    <button type="button" className="override-close" onClick={onClose} aria-label={t("common.close")}><IconX size={14} /></button>
    <label><span>{t("stats.displayNameLabel")}</span><input value={name} onChange={(event) => setName(event.target.value)} /></label>
    <label><span>{t("stats.portraitPathLabel")}</span><input value={portraitPath} onChange={(event) => setPortraitPath(event.target.value)} placeholder="mod-champions/example.png" /></label>
    <div className="override-actions">
      <button type="button" onClick={() => void save("", "")} disabled={saving}>{t("common.clear")}</button>
      <button type="button" onClick={onClose} disabled={saving}>{t("common.cancel")}</button>
      <button type="button" className="primary-button" onClick={() => void save()} disabled={saving}>{t("common.save")}</button>
    </div>
  </div>;
}

function StatCard({ label, value, sub }: { label: string; value: string; sub: string }) {
  return <div className="stats-card"><span>{label}</span><strong>{value}</strong><small>{sub}</small></div>;
}

function MetricChart({ metric, row, currentPatch }: { metric: MetricKey; row: ChampionRoleStat; currentPatch: string }) {
  const t = useT();
  const [hovered, setHovered] = useState<number | null>(null);
  const points = patchMetricPoints(metric, row, currentPatch, t);
  const historicalValues = points.map((point) => point.value).filter((value): value is number => value !== null);
  const values = historicalValues;
  const min = values.length ? Math.min(...values) : 0;
  const max = values.length ? Math.max(...values) : 1;
  const spread = Math.max(max - min, metric === "winRate" ? 0.05 : max * 0.12, 1e-6);
  const yMin = metric === "winRate" ? Math.max(0, min - spread * 0.35) : Math.max(0, min - spread * 0.45);
  const yMax = metric === "winRate" ? Math.min(1, max + spread * 0.35) : max + spread * 0.45;
  const chartW = 520;
  const chartH = 190;
  const pad = { left: 38, right: 18, top: 66, bottom: 34 };
  const xFor = (index: number) => pad.left + (index / Math.max(1, points.length - 1)) * (chartW - pad.left - pad.right);
  const yFor = (value: number) => pad.top + (1 - ((value - yMin) / Math.max(1e-6, yMax - yMin))) * (chartH - pad.top - pad.bottom);
  const plotted = points.map((point, index) => point.value === null ? null : { ...point, index, x: xFor(index), y: yFor(point.value) }).filter((point): point is NonNullable<typeof point> => point !== null);
  const line = plotted.map((point) => `${point.x},${point.y}`).join(" ");
  const activePoint = hovered === null ? null : plotted.find((point) => point.index === hovered) ?? null;
  const tooltipX = activePoint ? Math.min(chartW - 150, Math.max(pad.left + 8, activePoint.x - 66)) : 0;
  const tooltipY = 8;
  return <div className="stats-metric-chart line" aria-label={`${t(`stats.metric.${metric}`)} ${t("stats.lineChartSuffix")}`}>
    <svg viewBox={`0 0 ${chartW} ${chartH}`} role="img">
      <line className="chart-grid" x1={pad.left} x2={chartW - pad.right} y1={pad.top} y2={pad.top} />
      <line className="chart-grid" x1={pad.left} x2={chartW - pad.right} y1={chartH - pad.bottom} y2={chartH - pad.bottom} />
      <text className="chart-axis" x={8} y={pad.top + 4}>{metric === "winRate" ? formatPercent(yMax) : formatCompact(yMax)}</text>
      <text className="chart-axis" x={8} y={chartH - pad.bottom + 4}>{metric === "winRate" ? formatPercent(yMin) : formatCompact(yMin)}</text>
      <polyline key={row.championId} className="chart-line stats-line" points={line} pathLength={1} />
      {points.map((point, index) => {
        const x = xFor(index);
        const y = point.value === null ? null : yFor(point.value);
        return <g key={`${point.label}-${index}`}>
          <line className={point.value === null ? "chart-marker chart-marker-empty" : "chart-marker"} x1={x} x2={x} y1={pad.top} y2={chartH - pad.bottom} />
          {y === null ? <circle className="chart-point chart-point-empty" cx={x} cy={chartH - pad.bottom} r={4.5} aria-label={`${point.label}: ${t("stats.noGamesYetSuffix")}`} /> : <>
            <circle
              className="chart-hit-target"
              cx={x}
              cy={y}
              r={15}
              tabIndex={0}
              role="img"
              aria-label={`${point.label}: ${point.display}`}
              onMouseEnter={() => setHovered(index)}
              onFocus={() => setHovered(index)}
              onMouseLeave={() => setHovered(null)}
              onBlur={() => setHovered(null)}
            />
            <circle
              className="chart-point"
              cx={x}
              cy={y}
              r={5.5}
              aria-hidden="true"
            />
          </>}
          <text className="chart-label" x={x} y={chartH - 10}>{point.label}</text>
        </g>;
      })}
      {activePoint && <g className="chart-tooltip" transform={`translate(${tooltipX} ${tooltipY})`}>
        <rect width="132" height="50" rx="4" />
        <text x="9" y="15">{activePoint.label}</text>
        <text x="9" y="30">{activePoint.display}</text>
        <text x="9" y="45">{activePoint.games} {t("stats.gamesWord")}</text>
      </g>}
    </svg>
    <p>{points.length > 1 ? t("stats.chartDescMultiple") : t("stats.chartDescSingle")}</p>
  </div>;
}

function PerformanceBar({ label, value, average, digits }: { label: string; value: number | null; average: number | null; digits?: number }) {
  const t = useT();
  const ratio = value !== null && average && average > 0 ? Math.min(1.8, value / average) : 0;
  return <div className="performance-bar"><div><span>{label}</span><strong>{formatNumber(value, digits ?? 0)}</strong></div><div><i style={{ width: `${Math.max(4, Math.min(100, ratio * 50))}%` }} /></div><small>{compareToAverage(value, average, t)}</small></div>;
}

type StatAverages = { damage: number | null; damagePerGold: number | null; cs: number | null; gold: number | null; tanking: number | null; healing: number | null; kda: number | null; rating: number | null };

function averageStats(rows: ChampionRoleStat[]): StatAverages {
  return {
    damage: weightedAverage(rows, (row) => row.avgDamage),
    damagePerGold: weightedAverage(rows, (row) => ratioOrNull(row.avgDamage, row.avgGold)),
    cs: weightedAverage(rows, (row) => row.avgCs),
    gold: weightedAverage(rows, (row) => row.avgGold),
    tanking: weightedAverage(rows, (row) => row.avgTanking),
    healing: weightedAverage(rows, (row) => row.avgHealing),
    kda: weightedAverage(rows, (row) => row.kda),
    rating: weightedAverage(rows, (row) => row.avgRating),
  };
}

function weightedAverage(rows: ChampionRoleStat[], pick: (row: ChampionRoleStat) => number | null) {
  let total = 0;
  let games = 0;
  for (const row of rows) {
    const value = pick(row);
    if (value === null || row.games === 0) continue;
    total += value * row.games;
    games += row.games;
  }
  return games ? total / games : null;
}

function compareToAverage(value: number | null, average: number | null, t: (key: string) => string) {
  if (value === null || average === null || average === 0) return t("stats.noRoleAverage");
  const delta = ((value / average) - 1) * 100;
  return `${delta >= 0 ? "+" : ""}${delta.toFixed(0)}% ${t("stats.vsRoleAvgSuffix")}`;
}

function ratioOrNull(numerator: number | null, denominator: number | null) {
  if (numerator === null || denominator === null || denominator === 0) return null;
  return numerator / denominator;
}

function patchMetricPoints(metric: MetricKey, row: ChampionRoleStat, currentPatch: string, t: (key: string) => string) {
  const timeline = row.patchTimeline.length
    ? row.patchTimeline
    : [{
        patch: t("stats.allPatchesFallback"),
        games: row.games,
        wins: row.wins,
        winRate: row.winRate,
        avgKills: row.avgKills,
        avgDeaths: row.avgDeaths,
        avgAssists: row.avgAssists,
        kda: row.kda,
        avgDamage: row.avgDamage,
        avgTanking: row.avgTanking,
        avgHealing: row.avgHealing,
        avgCs: row.avgCs,
        avgGold: row.avgGold,
        avgRating: row.avgRating,
      }];
  const points = timeline.map((point) => {
    const map = {
      winRate: { value: point.winRate, digits: 1 },
      damage: { value: point.avgDamage, digits: 0 },
      damagePerGold: { value: ratioOrNull(point.avgDamage, point.avgGold), digits: 2 },
      cs: { value: point.avgCs, digits: 0 },
      gold: { value: point.avgGold, digits: 0 },
      tanking: { value: point.avgTanking, digits: 0 },
      healing: { value: point.avgHealing, digits: 0 },
      kda: { value: point.kda, digits: 2 },
      rating: { value: point.avgRating, digits: 1 },
    }[metric];
    const display = map.value === null ? t("stats.notAvailable") : metric === "winRate" ? formatPercent(map.value) : formatNumber(map.value, map.digits);
    return { label: point.patch, value: map.value, display, games: point.games };
  });
  if (currentPatch && points.length > 0 && points.every((point) => point.label !== currentPatch)) {
    points.push({ label: currentPatch, value: null, display: t("stats.notAvailable"), games: 0 });
  }
  return points;
}

function formatCompact(value: number) {
  if (value >= 1000) return `${(value / 1000).toFixed(1)}k`;
  if (value >= 100) return value.toFixed(0);
  return value.toFixed(1);
}
