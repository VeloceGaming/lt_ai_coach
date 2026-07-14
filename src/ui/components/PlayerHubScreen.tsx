import { useEffect, useMemo, useRef, useState } from "react";
import {
  IconBriefcase,
  IconBuilding,
  IconCheck,
  IconChevronDown,
  IconHistory,
  IconSearch,
  IconUser,
  IconUserOff,
  IconUsers,
} from "@tabler/icons-react";
import type {
  AthleteChampionLookup,
  AthleteCoreStats,
  AthleteDetail,
  AthleteMastery,
  AthleteRoleRatings,
  AthleteSummary,
  AthleteTendencyStats,
  DraftCatalog,
} from "../types";
import { titleCase } from "../lib/format";
import { resolveMissingPortraits } from "../lib/portraits";
import { ChampionPortraitView } from "./ChampionPortraitView";
import { RoleGlyph } from "./RoleGlyph";
import { usePlayerHubUiStore, type ContractFilter } from "../stores/usePlayerHubUiStore";
import { PlayerHubMarkingMenu, type PlayerMarkingMenuState } from "./PlayerHubMarkingMenu";
import { useT } from "../stores/useI18nStore";

// Translation keys (not English text) — call t() on each.
const coreLabels: Array<[keyof AthleteCoreStats, string]> = [
  ["lastHit", "playerHub.core.lastHit"],
  ["skillAvoid", "playerHub.core.skillAvoid"],
  ["skillHit", "playerHub.core.skillHit"],
  ["positioning", "playerHub.core.positioning"],
  ["controlSpeed", "playerHub.core.controlSpeed"],
  ["concentration", "playerHub.core.concentration"],
  ["mental", "playerHub.core.mental"],
  ["judgement", "playerHub.core.judgement"],
];

const tendencyLabels: Array<[keyof AthleteTendencyStats, string]> = [
  ["shotcalling", "playerHub.tendency.shotcalling"],
  ["roaming", "playerHub.tendency.roaming"],
  ["aggressive", "playerHub.tendency.aggressive"],
  ["ego", "playerHub.tendency.ego"],
];

// Display label is derived from the existing role.* keys (see RoleStatGroup),
// not stored here — glyphRole normalizes "bottom" -> "bot" for the lookup.
const roleKeys: Array<keyof AthleteRoleRatings> = ["top", "jungle", "mid", "bottom", "support"];

export function PlayerHubScreen({
  athletes,
  playerTeamId,
  catalog,
  loadDetail,
  lookupMastery,
}: {
  athletes: AthleteSummary[];
  playerTeamId: number | null;
  catalog: DraftCatalog;
  loadDetail: (athleteId: number) => Promise<AthleteDetail | null>;
  lookupMastery: (athleteId: number, championId: string) => Promise<AthleteChampionLookup | null>;
}) {
  const t = useT();
  const query = usePlayerHubUiStore((state) => state.query);
  const team = usePlayerHubUiStore((state) => state.team);
  const contract = usePlayerHubUiStore((state) => state.contract);
  const selectedId = usePlayerHubUiStore((state) => state.selectedId);
  const setQuery = usePlayerHubUiStore((state) => state.setQuery);
  const setTeam = usePlayerHubUiStore((state) => state.setTeam);
  const setContract = usePlayerHubUiStore((state) => state.setContract);
  const setSelectedId = usePlayerHubUiStore((state) => state.setSelectedId);
  const clearDirectoryFilters = usePlayerHubUiStore((state) => state.clearDirectoryFilters);
  const initializeDefaultTeam = usePlayerHubUiStore((state) => state.initializeDefaultTeam);
  const [detail, setDetail] = useState<AthleteDetail | null>(null);
  const [detailBusy, setDetailBusy] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [markingMenu, setMarkingMenu] = useState<PlayerMarkingMenuState | null>(null);

  const teams = useMemo(
    () => [...new Set(athletes.map((athlete) => athlete.teamName).filter((name): name is string => Boolean(name)))].sort(),
    [athletes],
  );
  const playerTeamName = useMemo(
    () => athletes.find((athlete) => athlete.teamId === playerTeamId)?.teamName ?? null,
    [athletes, playerTeamId],
  );
  useEffect(() => {
    if (playerTeamId !== null && !playerTeamName) return;
    initializeDefaultTeam(playerTeamName);
  }, [initializeDefaultTeam, playerTeamId, playerTeamName]);
  const visibleAthletes = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return athletes.filter((athlete) => {
      if (team !== "all" && athlete.teamName !== team) return false;
      if (contract === "contracted" && athlete.teamId === null) return false;
      if (contract === "free" && athlete.teamId !== null) return false;
      if (!needle) return true;
      return athlete.name.toLowerCase().includes(needle)
        || (athlete.teamName ?? "free agent").toLowerCase().includes(needle)
        || (athlete.strongestRole ?? "").toLowerCase().includes(needle);
    });
  }, [athletes, contract, query, team]);

  useEffect(() => {
    if (selectedId !== null && visibleAthletes.some((athlete) => athlete.id === selectedId)) return;
    setSelectedId(visibleAthletes[0]?.id ?? null);
  }, [selectedId, setSelectedId, visibleAthletes]);

  useEffect(() => {
    if (selectedId === null) {
      setDetail(null);
      return;
    }
    let active = true;
    setDetailBusy(true);
    setDetailError(null);
    loadDetail(selectedId)
      .then((next) => { if (active) setDetail(next); })
      .catch((error) => {
        if (!active) return;
        setDetail(null);
        setDetailError(error instanceof Error ? error.message : String(error));
      })
      .finally(() => { if (active) setDetailBusy(false); });
    return () => { active = false; };
  }, [loadDetail, selectedId]);

  if (athletes.length === 0) {
    return <section className="screen-panel player-hub-panel"><div className="screen-empty"><IconUsers size={28} /><strong>{t("playerHub.noProfilesTitle")}</strong><p>{t("playerHub.noProfilesDesc")}</p></div></section>;
  }

  return <section className="screen-panel player-hub-panel">
    <div className="player-hub-layout">
      <aside className="player-directory">
        <header className="player-directory-heading">
          <div><span className="player-section-label">{t("playerHub.rosterDatabase")}</span><h2>{t("nav.players.label")}</h2></div>
          <strong>{visibleAthletes.length}</strong>
        </header>

        <label className="tier-search player-search">
          <IconSearch size={15} />
          <input type="search" placeholder={t("playerHub.searchPlayersPlaceholder")} value={query} onChange={(event) => setQuery(event.target.value)} />
        </label>

        <div className="player-filter-row">
          <TeamFilterMenu teams={teams} playerTeam={playerTeamName} value={team} onChange={setTeam} />
          <div className="player-contract-filter" aria-label={t("playerHub.contractFilterAria")}>
            <button type="button" className={contract === "all" ? "active" : ""} title={t("playerHub.allPlayersTitle")} aria-pressed={contract === "all"} onClick={() => setContract("all")}><IconUsers size={14} /></button>
            <button type="button" className={contract === "contracted" ? "active" : ""} title={t("playerHub.contractedPlayersTitle")} aria-pressed={contract === "contracted"} onClick={() => setContract("contracted")}><IconBriefcase size={14} /></button>
            <button type="button" className={contract === "free" ? "active" : ""} title={t("playerHub.freeAgentsTitle")} aria-pressed={contract === "free"} onClick={() => setContract("free")}><IconUserOff size={14} /></button>
          </div>
        </div>

        <div className="player-list">
          {visibleAthletes.map((athlete, index) => <button type="button" key={athlete.id} className={`player-row${selectedId === athlete.id ? " active" : ""}${athlete.teamId === null ? " free-agent" : ""}`} aria-pressed={selectedId === athlete.id} onClick={() => setSelectedId(athlete.id)} onPointerDown={(event) => { if (event.button !== 2) return; event.preventDefault(); setMarkingMenu({ x: event.clientX, y: event.clientY, athlete }); }} onContextMenu={(event) => event.preventDefault()} onKeyDown={(event) => {
            if (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "Home" || event.key === "End") {
              event.preventDefault();
              const nextIndex = event.key === "Home" ? 0 : event.key === "End" ? visibleAthletes.length - 1 : Math.max(0, Math.min(visibleAthletes.length - 1, index + (event.key === "ArrowDown" ? 1 : -1)));
              const next = event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>(".player-row")[nextIndex];
              next?.focus();
              if (visibleAthletes[nextIndex]) setSelectedId(visibleAthletes[nextIndex].id);
            } else if (event.key === "F10" && event.shiftKey) {
              event.preventDefault();
              const rect = event.currentTarget.getBoundingClientRect();
              setMarkingMenu({ x: rect.left + rect.width / 2, y: rect.top + rect.height / 2, athlete, keyboard: true });
            }
          }}>
            <span className="player-avatar"><IconUser size={16} /></span>
            <span className="player-row-main">
              <strong>{athlete.name}</strong>
              <small>{athlete.teamName ?? t("playerHub.freeAgent")}</small>
            </span>
            {athlete.strongestRole && <span className="player-role" title={`${t("playerHub.strongestRolePrefix")} ${t(`role.${glyphRole(athlete.strongestRole)}`)}`}><RoleGlyph role={glyphRole(athlete.strongestRole)} /></span>}
          </button>)}
          {visibleAthletes.length === 0 && <div className="player-list-empty">{t("playerHub.noMatchingPlayers")}</div>}
        </div>
      </aside>

      <main className={`player-detail-workspace${detailBusy ? " is-refreshing" : ""}`} aria-busy={detailBusy}>
        {detailBusy && !detail && <div className="player-detail-state"><span className="loading-spinner" /><strong>{t("playerHub.loadingProfile")}</strong></div>}
        {!detailBusy && detailError && <div className="player-detail-state error"><strong>{t("playerHub.couldNotLoadAthlete")}</strong><p>{detailError}</p></div>}
        {!detailBusy && !detailError && !detail && <div className="player-detail-state"><strong>{t("playerHub.selectAthlete")}</strong></div>}
        {detail && <PlayerDetail detail={detail} catalog={catalog} lookupMastery={lookupMastery} />}
      </main>
    </div>
    {markingMenu && <PlayerHubMarkingMenu state={markingMenu} onClose={() => setMarkingMenu(null)} onFilterTeam={() => { if (markingMenu.athlete.teamName) setTeam(markingMenu.athlete.teamName); }} onFilterRole={() => { if (markingMenu.athlete.strongestRole) setQuery(roleName(markingMenu.athlete.strongestRole)); }} onClearFilters={clearDirectoryFilters} />}
  </section>;
}

function PlayerDetail({
  detail,
  catalog,
  lookupMastery,
}: {
  detail: AthleteDetail;
  catalog: DraftCatalog;
  lookupMastery: (athleteId: number, championId: string) => Promise<AthleteChampionLookup | null>;
}) {
  const t = useT();
  const masteryQuery = usePlayerHubUiStore((state) => state.masteryQuery);
  const selectedChampion = usePlayerHubUiStore((state) => state.selectedChampion);
  const setMasteryQuery = usePlayerHubUiStore((state) => state.setMasteryQuery);
  const setSelectedChampion = usePlayerHubUiStore((state) => state.setSelectedChampion);
  const [lookup, setLookup] = useState<AthleteChampionLookup | null>(null);
  const [resolvedPortraits, setResolvedPortraits] = useState(new Map<string, DraftCatalog["champions"][number]["portrait"]>());
  const catalogChampions = useMemo(() => new Map(catalog.champions.map((champion) => [champion.id, champion])), [catalog.champions]);
  const champions = useMemo(() => {
    const records = new Map(catalogChampions);
    for (const mastery of detail.masteries) {
      const existing = records.get(mastery.championId);
      const resolved = resolvedPortraits.get(mastery.championId) ?? null;
      if (!existing) {
        records.set(mastery.championId, { id: mastery.championId, name: championNameFromId(mastery.championId), portrait: resolved, roleFit: {} });
      } else if (!existing.portrait && resolved) {
        records.set(mastery.championId, { ...existing, portrait: resolved });
      }
    }
    return records;
  }, [catalogChampions, detail.masteries, resolvedPortraits]);
  const masteryGroups = useMemo(() => {
    const needle = masteryQuery.trim().toLowerCase();
    const visible = detail.masteries.filter((mastery) => {
      if (mastery.mastery < 60) return false;
      if (!needle) return true;
      const champion = champions.get(mastery.championId);
      return mastery.championId.toLowerCase().includes(needle)
        || (champion?.name ?? "").toLowerCase().includes(needle);
    });
    return ([
      ["100", visible.filter((mastery) => mastery.mastery >= 100)],
      ["90+", visible.filter((mastery) => mastery.mastery >= 90 && mastery.mastery < 100)],
      ["80+", visible.filter((mastery) => mastery.mastery >= 80 && mastery.mastery < 90)],
      ["70+", visible.filter((mastery) => mastery.mastery >= 70 && mastery.mastery < 80)],
      ["60+", visible.filter((mastery) => mastery.mastery >= 60 && mastery.mastery < 70)],
    ] as const).filter(([, masteries]) => masteries.length > 0);
  }, [champions, detail.masteries, masteryQuery]);

  useEffect(() => {
    if (!detail.masteries.some((mastery) => mastery.championId === selectedChampion)) {
      setSelectedChampion(detail.masteries.find((mastery) => mastery.mastery >= 60)?.championId ?? null);
    }
  }, [detail.id, detail.masteries, selectedChampion, setSelectedChampion]);

  useEffect(() => {
    let active = true;
    const missing = detail.masteries
      .map((mastery) => mastery.championId)
      .filter((championId) => !catalogChampions.get(championId)?.portrait);
    resolveMissingPortraits(missing).then((found) => {
      if (active && found.size > 0) setResolvedPortraits(found);
    });
    return () => { active = false; };
  }, [catalogChampions, detail.id, detail.masteries]);

  useEffect(() => {
    if (!selectedChampion) {
      setLookup(null);
      return;
    }
    let active = true;
    lookupMastery(detail.id, selectedChampion)
      .then((next) => { if (active) setLookup(next); })
      .catch(() => { if (active) setLookup(null); });
    return () => { active = false; };
  }, [detail.id, lookupMastery, selectedChampion]);

  const selectedMastery = detail.masteries.find((mastery) => mastery.championId === selectedChampion) ?? null;
  const selectedChampionRecord = selectedChampion ? champions.get(selectedChampion) : undefined;
  const coreNote = selectedMastery && selectedChampionRecord
    ? `${selectedChampionRecord.name} · ${selectedMastery.statBuff > 0 ? `+${Math.round(selectedMastery.statBuff * 100)}%` : t("playerHub.noBuff")}`
    : t("playerHub.masteryAffectedFallback");

  return <div className="player-detail">
    <div className="player-profile-column">
    <header className="player-profile-header">
      <span className="player-profile-avatar"><IconUser size={24} /></span>
      <div>
        <span className="player-section-label">{t("playerHub.athleteProfileLabel")}</span>
        <div className="player-profile-identity" key={detail.id}>
          <h2>{detail.name}</h2>
          <p><IconBuilding size={13} />{detail.teamName ?? t("playerHub.freeAgent")}{detail.strongestRole && <><span>·</span><RoleGlyph role={glyphRole(detail.strongestRole)} />{t(`role.${glyphRole(detail.strongestRole)}`)}</>}</p>
        </div>
      </div>
      <div className="player-profile-count"><span>{t("playerHub.championRecordsLabel")}</span><strong>{detail.masteries.length}</strong></div>
    </header>

    {!detail.stats && <div className="player-data-warning">{t("playerHub.baseStatsUnavailable")}</div>}

    {detail.stats && <div className="player-stat-groups">
      <StatGroup title={t("playerHub.coreMechanicsTitle")} note={coreNote} values={detail.stats.core} effectiveValues={lookup?.effectiveCore} labels={coreLabels} accent />
      <StatGroup title={t("playerHub.tendenciesTitle")} note={t("playerHub.unaffected")} values={detail.stats.tendencies} labels={tendencyLabels} />
      <RoleStatGroup values={detail.stats.roles} />
    </div>}
    </div>

    <section className="player-mastery-section">
      <div className="player-mastery-heading">
        <div><span className="player-section-label">{t("playerHub.championMasteryLabel")}</span><h3>{t("playerHub.championPoolTitle")}</h3></div>
        <label className="tier-search mastery-search"><IconSearch size={14} /><input type="search" placeholder={t("playerHub.searchChampionPlaceholder")} value={masteryQuery} onChange={(event) => setMasteryQuery(event.target.value)} /></label>
      </div>

      <div className="player-mastery-layout">
        <div className="mastery-groups" key={detail.id}>
          {masteryGroups.map(([label, masteries]) => <section className={`mastery-band mastery-band-${label.replace("+", "")}`} key={label}>
            <header><strong>{label}</strong><span>{masteries.length}</span></header>
            <div className="mastery-grid">{masteries.map((mastery) => <MasteryTile key={mastery.championId} mastery={mastery} champion={champions.get(mastery.championId)} selected={selectedChampion === mastery.championId} onSelect={() => setSelectedChampion(mastery.championId)} />)}</div>
          </section>)}
          {masteryGroups.length === 0 && <div className="mastery-empty">{t("playerHub.noMasteryChampions")}</div>}
        </div>
      </div>
    </section>
  </div>;
}

function StatGroup<T extends Record<string, number>>({
  title,
  note,
  values,
  effectiveValues,
  labels,
  accent = false,
}: {
  title: string;
  note: string;
  values: T;
  effectiveValues?: Partial<Record<keyof T, number>>;
  labels: Array<[keyof T, string]>;
  accent?: boolean;
}) {
  const t = useT();
  return <section className={`player-stat-group${accent ? " accent" : ""}`}>
    <header><h3>{title}</h3><span>{note}</span></header>
    <div>{labels.map(([key, label]) => {
      const base = values[key];
      const effective = effectiveValues?.[key];
      const boosted = effective !== undefined && effective > base;
      return <div className={`player-stat-row${boosted ? " boosted" : ""}`} key={String(key)}><span>{t(label)}</span><i><b style={{ width: `${Math.min(100, effective ?? base)}%` }} /></i><strong title={boosted ? `${t("playerHub.baseValuePrefix")} ${base}` : undefined}>{boosted && <small>{base}</small>}{formatEffective(effective ?? base)}</strong></div>;
    })}</div>
  </section>;
}

function RoleStatGroup({ values }: { values: AthleteRoleRatings }) {
  const t = useT();
  return <section className="player-stat-group role-ratings">
    <header><h3>{t("playerHub.roleRatingsTitle")}</h3><span>{t("playerHub.unaffected")}</span></header>
    <div>{roleKeys.map((key) => <div className="player-stat-row" key={key}><span><RoleGlyph role={glyphRole(key)} />{t(`role.${glyphRole(key)}`)}</span><i><b style={{ width: `${Math.min(100, values[key])}%` }} /></i><strong>{values[key]}</strong></div>)}</div>
  </section>;
}

function MasteryTile({
  mastery,
  champion,
  selected,
  onSelect,
}: {
  mastery: AthleteMastery;
  champion: DraftCatalog["champions"][number] | undefined;
  selected: boolean;
  onSelect: () => void;
}) {
  const t = useT();
  return <button type="button" className={`mastery-tile tier-${masteryTier(mastery.valueRaw)}${selected ? " active" : ""}`} onClick={onSelect}>
    <span className="mastery-portrait">
      <ChampionPortraitView portrait={champion?.portrait ?? null} width={160} height={108} fixedCenter />
      {mastery.recent && <span className="mastery-recent" title={t("playerHub.recentChampionTitle")}><IconHistory size={10} /></span>}
    </span>
    <span className="mastery-tile-main"><strong>{champion?.name ?? championNameFromId(mastery.championId)}</strong><small>{mastery.statBuff > 0 ? `+${Math.round(mastery.statBuff * 100)}% ${t("playerHub.statsWord")}` : t("playerHub.noStatBuff")}</small></span>
    <span className="mastery-value">{formatMastery(mastery.mastery)}</span>
  </button>;
}

function masteryTier(valueRaw: number) {
  if (valueRaw >= 1000) return "100";
  if (valueRaw >= 900) return "90";
  if (valueRaw >= 800) return "80";
  if (valueRaw >= 700) return "70";
  return "base";
}

function formatMastery(value: number) {
  return Number.isInteger(value) ? value.toFixed(0) : value.toFixed(1);
}

function formatEffective(value: number) {
  return Number.isInteger(value) ? value.toFixed(0) : value.toFixed(1);
}

function glyphRole(role: string) {
  return role === "bottom" ? "bot" : role;
}

function roleName(role: string) {
  return role === "bottom" ? "Bot" : titleCase(role);
}

function championNameFromId(championId: string) {
  return titleCase(championId.replaceAll("_", " "));
}

function TeamFilterMenu({ teams, playerTeam, value, onChange }: { teams: string[]; playerTeam: string | null; value: string; onChange: (value: string) => void }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (!open) return;
    function close(event: PointerEvent) {
      if (event.target instanceof Node && !rootRef.current?.contains(event.target)) setOpen(false);
    }
    function closeByKey(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", closeByKey);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", closeByKey);
    };
  }, [open]);

  const orderedTeams = playerTeam ? [playerTeam, ...teams.filter((team) => team !== playerTeam)] : teams;
  const options = [{ value: "all", label: t("playerHub.allTeams"), home: false }, ...orderedTeams.map((team) => ({ value: team, label: team, home: team === playerTeam }))];
  return <div className={`player-team-filter${open ? " open" : ""}`} ref={rootRef}>
    <button type="button" className="player-team-trigger" aria-haspopup="listbox" aria-expanded={open} onClick={() => setOpen((current) => !current)}>
      <IconBuilding size={14} /><span>{value === "all" ? t("playerHub.allTeams") : value}</span><IconChevronDown size={13} />
    </button>
    {open && <div className="player-team-menu" role="listbox" aria-label={t("playerHub.filterByTeamAria")}>
      {options.map((option) => <button type="button" className={option.home ? "player-team-home" : ""} role="option" aria-selected={value === option.value} key={option.value} onClick={() => { onChange(option.value); setOpen(false); }}><span>{option.label}{option.home ? ` · ${t("playerHub.yourTeamSuffix")}` : ""}</span>{value === option.value && <IconCheck size={13} />}</button>)}
    </div>}
  </div>;
}
