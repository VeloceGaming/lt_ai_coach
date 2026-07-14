import { IconLayoutGrid, IconSearch } from "@tabler/icons-react";
import { useEffect, useMemo, useState } from "react";
import type { ChampionRoleStat, RoleStatistics } from "../types";
import { buildTierContext, championTier, performanceScore, tierOrder } from "../lib/tiers";
import { ChampionDetailPanel } from "./ChampionDetailPanel";
import { RoleGlyph } from "./RoleGlyph";
import { TierRow } from "./TierRow";
import { StaggerItem, StaggerList } from "../motion/Stagger";
import { useT } from "../stores/useI18nStore";

const roles = ["all", "top", "jungle", "mid", "bot", "support"] as const;
type RoleFilter = (typeof roles)[number];

export function TierListScreen({ statistics, tiers, focusChampionId, onSetTier, onOpenStats }: { statistics: RoleStatistics; tiers: Record<string, string>; focusChampionId?: string | null; onSetTier: (championId: string, tier: string) => void; onOpenStats: (championId: string, role: string) => void }) {
  const t = useT();
  const [role, setRole] = useState<RoleFilter>("all");
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(statistics.overallRows[0]?.championId ?? null);

  // When navigated here for a specific champion (e.g. from Patch notes), clear the
  // role filter so it's visible and select it.
  useEffect(() => {
    if (!focusChampionId) return;
    setRole("all");
    setSelectedId(focusChampionId);
  }, [focusChampionId]);

  const bestRoleByChampion = useMemo(() => {
    const map = new Map<string, ChampionRoleStat>();
    statistics.roleRows.forEach((row) => {
      const current = map.get(row.championId);
      if (!current || row.games > current.games) map.set(row.championId, row);
    });
    return map;
  }, [statistics.roleRows]);

  const visibleRows = useMemo(() => {
    const source = role === "all" ? statistics.overallRows : statistics.roleRows.filter((row) => row.role.toLowerCase() === role);
    const query = search.trim().toLowerCase();
    return source.filter((row) => !query || row.championName.toLowerCase().includes(query) || row.championId.toLowerCase().includes(query));
  }, [role, search, statistics.overallRows, statistics.roleRows]);

  const tierContext = useMemo(() => buildTierContext(statistics.overallRows, statistics.globalWinRate), [statistics.overallRows, statistics.globalWinRate]);
  const grouped = useMemo(() => new Map(tierOrder.map((tier) => [tier, visibleRows.filter((row) => championTier(row, tiers, tierContext) === tier).sort((a, b) => performanceScore(b, tierContext) - performanceScore(a, tierContext))])), [tierContext, tiers, visibleRows]);
  const selectedRow = visibleRows.find((row) => row.championId === selectedId) ?? statistics.overallRows.find((row) => row.championId === selectedId) ?? null;
  const roleFor = (row: ChampionRoleStat) => role === "all" ? bestRoleByChampion.get(row.championId)?.role ?? row.role : role;

  useEffect(() => {
    // null = user explicitly closed; leave it closed rather than snapping to first champion
    if (selectedId === null || visibleRows.some((row) => row.championId === selectedId)) return;
    setSelectedId(visibleRows[0]?.championId ?? null);
  }, [selectedId, visibleRows]);

  return <div className={`tier-list-screen${selectedRow ? " has-detail" : ""}`}>
    <main className="tier-list-main">
      <div className="tier-list-toolbar">
        <div className="tier-title-group"><h2>{t("nav.tiers.label")}</h2><span>{statistics.overallRows.length} {t("tiers.championsWord")} · {t("tiers.patchWord")} {statistics.currentPatch}</span></div>
        <label className="tier-search"><IconSearch size={15} /><input type="search" placeholder={t("tiers.searchPlaceholder")} value={search} onChange={(event) => setSearch(event.target.value)} /></label>
        <div className="tier-role-filters" aria-label={t("tiers.roleFilterAria")}>
          {roles.map((value) => <button type="button" key={value} className={role === value ? "active" : ""} aria-pressed={role === value} title={value === "all" ? t("draft.allRolesTitle") : t(`role.${value}`)} onClick={() => setRole(value)}>{value === "all" ? <IconLayoutGrid size={15} /> : <RoleGlyph role={value} />}</button>)}
        </div>
      </div>
      <StaggerList className="tier-rows">{tierOrder.map((tier) => <StaggerItem key={tier}><TierRow tier={tier} rows={grouped.get(tier) ?? []} roleFor={roleFor} selectedId={selectedId} onSelect={(row) => setSelectedId(row.championId)} /></StaggerItem>)}</StaggerList>
    </main>
    {selectedRow && <ChampionDetailPanel key={selectedRow.championId} row={selectedRow} role={roleFor(selectedRow)} tiers={tiers} tierContext={tierContext} onSetTier={onSetTier} onOpenStats={onOpenStats} onClose={() => setSelectedId(null)} />}
  </div>;
}
