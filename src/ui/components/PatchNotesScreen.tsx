// Patch Notes screen: browse every stored patch via a left-side timeline (grouped
// by major version, each badged Major/Medium/Minor), and see that patch's changes
// grouped into Buffs, Nerfs, and New. Champion names/portraits come from the
// enriched catalog.

import { useEffect, useMemo, useRef, useState } from "react";
import { IconSearch } from "@tabler/icons-react";
import type { ChampionPortrait, PatchChange, PatchHistoryEntry } from "../types";
import { titleCase } from "../lib/format";
import { formatPatchLabel, formatPatchValue } from "../lib/patchFormat";
import { ChampionPortraitView } from "./ChampionPortraitView";
import { StaggerItem, StaggerList } from "../motion/Stagger";
import { useT } from "../stores/useI18nStore";

type ChampionInfo = { name: string; portrait: ChampionPortrait | null };
type Row = { championId: string; name: string; portrait: ChampionPortrait | null; changes: PatchChange[]; isNew: boolean; direction: "buff" | "nerf"; magnitude: number };

// Compare a patch to the next-older one to label how big a step it was.
function patchSize(patch: string, older: string | undefined): "Major" | "Medium" | "Minor" | null {
  if (!older) return null;
  const a = patch.split(".").map((part) => Number(part) || 0);
  const b = older.split(".").map((part) => Number(part) || 0);
  if (a[0] !== b[0]) return "Major";
  if (a[1] !== b[1]) return "Medium";
  if (a[2] !== b[2]) return "Minor";
  return null;
}

export function PatchNotesScreen({ patchHistory, currentPatch, championLookup, focusChampionId, onOpenChampion }: {
  patchHistory: PatchHistoryEntry[];
  currentPatch?: string;
  championLookup: Map<string, ChampionInfo>;
  focusChampionId: string | null;
  onOpenChampion: (championId: string) => void;
}) {
  const t = useT();
  const [search, setSearch] = useState("");
  const [selectedPatch, setSelectedPatch] = useState<string | null>(null);
  const focusedRef = useRef<HTMLButtonElement>(null);

  // Default the selection to the current patch (or the newest), and when arriving
  // for a specific champion, jump to the newest patch that touched it.
  useEffect(() => {
    if (!patchHistory.length) return;
    if (focusChampionId) {
      const hit = patchHistory.find((entry) => entry.additions.includes(focusChampionId) || entry.changes.some((c) => c.championId === focusChampionId));
      if (hit) { setSelectedPatch(hit.patch); return; }
    }
    setSelectedPatch(() => {
      // A fresh import can advance the save while this screen still has the old
      // patch selected. Follow the newly reported current patch when it changes;
      // ordinary manual history browsing does not rerun this effect.
      if (currentPatch && patchHistory.some((e) => e.patch === currentPatch)) return currentPatch;
      return patchHistory[0].patch;
    });
  }, [patchHistory, currentPatch, focusChampionId]);

  const entry = patchHistory.find((candidate) => candidate.patch === selectedPatch) ?? null;
  const isCurrent = entry?.patch === currentPatch;

  useEffect(() => {
    if (focusChampionId && entry) focusedRef.current?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [focusChampionId, entry]);

  const { buffs, nerfs, additions, total } = useMemo(() => {
    if (!entry) return { buffs: [] as Row[], nerfs: [] as Row[], additions: [] as Row[], total: 0 };
    const query = search.trim().toLowerCase();
    const info = (id: string): ChampionInfo => championLookup.get(id) ?? { name: titleCase(id.replaceAll("_", " ")), portrait: null };
    const matches = (id: string, name: string) => !query || name.toLowerCase().includes(query) || id.toLowerCase().includes(query);

    const changeRows: Row[] = entry.changes.flatMap((champion) => {
      const meta = info(champion.championId);
      if (!matches(champion.championId, meta.name)) return [];
      const netImpact = champion.changes.reduce((sum, change) => sum + change.impact, 0);
      const direction: "buff" | "nerf" = netImpact >= 0 ? "buff" : "nerf";
      const magnitude = champion.changes.reduce((sum, change) => sum + Math.abs(change.impact), 0);
      return [{ championId: champion.championId, name: meta.name, portrait: meta.portrait, changes: champion.changes, isNew: false, direction, magnitude }];
    });
    const additionRows: Row[] = entry.additions.flatMap((id) => {
      const meta = info(id);
      if (!matches(id, meta.name)) return [];
      return [{ championId: id, name: meta.name, portrait: meta.portrait, changes: [], isNew: true, direction: "buff" as const, magnitude: 0 }];
    });

    const byMagnitude = (a: Row, b: Row) => b.magnitude - a.magnitude;
    return {
      buffs: changeRows.filter((row) => row.direction === "buff").sort(byMagnitude),
      nerfs: changeRows.filter((row) => row.direction === "nerf").sort(byMagnitude),
      additions: additionRows.sort((a, b) => a.name.localeCompare(b.name)),
      total: changeRows.length + additionRows.length,
    };
  }, [entry, search, championLookup]);

  if (!patchHistory.length) {
    return <div className="patch-notes-screen empty"><div className="screen-empty"><strong>{t("patchNotes.emptyTitle")}</strong><p>{t("patchNotes.emptyDesc")}</p></div></div>;
  }

  const renderRow = (row: Row) => {
    const focused = row.championId === focusChampionId;
    return <button type="button" key={row.championId} ref={focused ? focusedRef : undefined} className={`patch-champ${focused ? " focused" : ""}`} onClick={() => onOpenChampion(row.championId)} title={t("patchNotes.openChampionOnTierList", { champion: row.name })}>
      <ChampionPortraitView portrait={row.portrait} width={58} height={72} />
      <div className="patch-champ-main">
        <div className="patch-champ-head">
          <strong>{row.name}</strong>
          {row.isNew && <span className="patch-delta-chip new">{t("patchNotes.newChip")}</span>}
        </div>
        {row.changes.length
          ? <ul className="patch-change-list">{row.changes.map((change, index) => <li className="patch-change" key={`${change.asset}-${change.field}-${index}`}><span className={change.impact > 0 ? "buff" : change.impact < 0 ? "nerf" : "unchanged"}>{change.impact > 0 ? "▲" : change.impact < 0 ? "▼" : "–"}</span><span>{formatPatchLabel(change.asset, change.target, change.field, t)}</span><strong>{formatPatchValue(change.oldValue, change.field)}→{formatPatchValue(change.newValue, change.field)}</strong></li>)}</ul>
          : <p className="patch-champ-note">{t("patchNotes.newChampionNote")}</p>}
      </div>
    </button>;
  };

  const group = (title: string, rows: Row[]) => rows.length > 0 && <section className="patch-group" key={title}>
    <div className="patch-group-head"><h3>{title}</h3><span>{rows.length}</span></div>
    <StaggerList key={entry?.patch} className="patch-group-list">{rows.map((row) => <StaggerItem key={row.championId}>{renderRow(row)}</StaggerItem>)}</StaggerList>
  </section>;

  return <div className="patch-notes-screen">
    <aside className="patch-timeline" aria-label={t("patchNotes.historyAria")}>
      {patchHistory.map((candidate, index) => {
        const size = patchSize(candidate.patch, patchHistory[index + 1]?.patch);
        const major = candidate.patch.split(".")[0];
        const showMajor = index === 0 || patchHistory[index - 1].patch.split(".")[0] !== major;
        return <div key={candidate.patch}>
          {showMajor && <div className="patch-timeline-major">{major}</div>}
          <button type="button" className={`patch-timeline-item${candidate.patch === selectedPatch ? " active" : ""}`} onClick={() => setSelectedPatch(candidate.patch)}>
            <span className="patch-timeline-version">{candidate.patch}{candidate.patch === currentPatch && <em>{t("patchNotes.currentWord")}</em>}</span>
            {size && <span className={`patch-size-badge ${size.toLowerCase()}`}>{t(`patchNotes.size.${size.toLowerCase()}`)}</span>}
          </button>
        </div>;
      })}
    </aside>

    <div className="patch-notes-main">
      <div className="patch-notes-toolbar">
        <div className="tier-title-group"><h2>{t("nav.patch.label")}</h2><span>{entry ? `${t("patchNotes.patchWord")} ${entry.patch}${isCurrent ? ` · ${t("patchNotes.currentWord")}` : ""}` : ""}</span></div>
        <label className="tier-search"><IconSearch size={15} /><input type="search" placeholder={t("tiers.searchPlaceholder")} value={search} onChange={(event) => setSearch(event.target.value)} /></label>
      </div>
      {total === 0
        ? <div className="screen-empty"><strong>{t("patchNotes.noChangesTitle")}</strong><p>{t("patchNotes.noChangesDescPrefix")} {entry?.patch}{search ? ` ${t("patchNotes.matchingSearchSuffix")}` : ""}.</p></div>
        : <div className="patch-groups">
            {group(t("patchNotes.buffsTitle"), buffs)}
            {group(t("patchNotes.nerfsTitle"), nerfs)}
            {group(t("patchNotes.newThisPatchTitle"), additions)}
          </div>}
    </div>
  </div>;
}
