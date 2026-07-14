// Compact overlay: one team's row of pick/ban slots for the board third —
// 48px-wide portraits with names below, the active slot marked "now", and an
// optional recommended-role label (shown during the swap stage).
//
// When `interactive` is set (the user's own pick row), clicking a filled slot
// activates the role-confirm popover via `onSlotActivate`. A confirmed role is
// shown in place of the inferred one and flagged with a marker.

import type { MouseEvent } from "react";
import { IconDots } from "@tabler/icons-react";
import type { DraftChampion, DraftSide } from "../types";
import { ChampionPortraitView } from "./ChampionPortraitView";
import { useT } from "../stores/useI18nStore";

export function CompactTeamSlots({ side, label, ids, limit, champions, active, selected, roles, overrides, interactive = false, onSlotActivate, big = false }: {
  side: DraftSide;
  label: string;
  ids: string[];
  limit: number;
  champions: Map<string, DraftChampion>;
  active: boolean;
  selected: boolean;
  roles?: Map<string, string | undefined>;
  overrides?: Record<string, string>;
  interactive?: boolean;
  onSlotActivate?: (championId: string, championName: string, coords: { clientX: number; clientY: number }) => void;
  big?: boolean;
}) {
  const t = useT();
  const slots = [...ids.slice(0, limit), ...Array(Math.max(0, limit - ids.length)).fill(null)] as Array<string | null>;
  // The slot currently being decided on the acting side gets the "now" marker.
  const nowIndex = active && ids.length < limit ? ids.length : -1;
  const [pw, ph] = big ? [60, 90] : [54, 81];
  const accessibleLabel = `${label}${selected ? `, ${t("compact.yourTeamSuffix")}` : ""}`;
  const showRoles = roles || overrides;
  return <div className={`compact-board-row ${side}${big ? " big" : ""}${selected ? " player-side" : ""}`} aria-label={accessibleLabel} title={accessibleLabel}>
    <div className="compact-board-slots">
      {slots.map((id, index) => {
        const champ = id ? champions.get(id) : undefined;
        const name = champ?.name ?? id ?? "";
        const isNow = index === nowIndex;
        const override = id ? overrides?.[id] : undefined;
        const shownRole = override ?? (id ? roles?.get(id) : undefined);
        const canActivate = interactive && !!id;
        // Mouse events carry coordinates; keyboard activation anchors the popover
        // to the slot element's center instead.
        const activate = (event: MouseEvent) => { if (id) onSlotActivate?.(id, name, { clientX: event.clientX, clientY: event.clientY }); };
        const activateFromElement = (element: HTMLElement) => {
          if (!id) return;
          const rect = element.getBoundingClientRect();
          onSlotActivate?.(id, name, { clientX: rect.left + rect.width / 2, clientY: rect.bottom });
        };
        return <div
          className={`compact-board-slot${id ? " filled" : ""}${isNow ? " now" : ""}${canActivate ? " interactive" : ""}${override ? " overridden" : ""}`}
          key={`${side}-${index}`}
          title={canActivate ? `${name} ${t("compact.confirmRoleTooltip")}` : id ? name : `${label} ${index + 1}`}
          role={canActivate ? "button" : undefined}
          tabIndex={canActivate ? 0 : undefined}
          onClick={canActivate ? activate : undefined}
          onContextMenu={canActivate ? (event) => { event.preventDefault(); activate(event); } : undefined}
          onKeyDown={canActivate ? (event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); activateFromElement(event.currentTarget); } } : undefined}
        >
          <span className="compact-board-portrait">{id ? <ChampionPortraitView portrait={champ?.portrait ?? null} width={pw} height={ph} /> : isNow ? <IconDots size={18} /> : null}</span>
          <span className="compact-board-name">{id ? name : isNow ? t("compact.now") : " "}</span>
          {id && showRoles && <span className={`compact-slot-role${override ? " confirmed" : ""}`}>{shownRole ? t(`role.${shownRole}`) : "—"}</span>}
        </div>;
      })}
    </div>
  </div>;
}
