import { useEffect, useRef, useState } from "react";
import { IconBuilding, IconFilter, IconUserSearch } from "@tabler/icons-react";
import type { AthleteSummary } from "../types";
import { resolveMarkingDirection, type MarkingDirection } from "../lib/markingMenu";
import { useT } from "../stores/useI18nStore";

export type PlayerMarkingMenuState = { x: number; y: number; athlete: AthleteSummary; keyboard?: boolean };

export function PlayerHubMarkingMenu({ state, onClose, onFilterTeam, onFilterRole, onClearFilters }: {
  state: PlayerMarkingMenuState;
  onClose: () => void;
  onFilterTeam: () => void;
  onFilterRole: () => void;
  onClearFilters: () => void;
}) {
  const t = useT();
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [armed, setArmed] = useState<MarkingDirection | null>(null);
  const actions: Partial<Record<MarkingDirection, () => void>> = {
    east: state.athlete.teamName ? onFilterTeam : undefined,
    south: onClearFilters,
    west: state.athlete.strongestRole ? onFilterRole : undefined,
  };
  useEffect(() => {
    const closeByPointer = (event: PointerEvent) => {
      if (state.keyboard && event.target instanceof Node && !rootRef.current?.contains(event.target)) onClose();
    };
    const closeByKey = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    const armByPointer = (event: PointerEvent) => {
      if (state.keyboard) return;
      setArmed(resolveMarkingDirection(event.clientX - state.x, event.clientY - state.y));
    };
    const finishByPointer = (event: PointerEvent) => {
      if (state.keyboard || event.button !== 2) return;
      const direction = resolveMarkingDirection(event.clientX - state.x, event.clientY - state.y);
      actions[direction ?? "north"]?.();
      onClose();
    };
    document.addEventListener("pointerdown", closeByPointer);
    document.addEventListener("keydown", closeByKey);
    document.addEventListener("pointermove", armByPointer);
    document.addEventListener("pointerup", finishByPointer);
    return () => {
      document.removeEventListener("pointerdown", closeByPointer);
      document.removeEventListener("keydown", closeByKey);
      document.removeEventListener("pointermove", armByPointer);
      document.removeEventListener("pointerup", finishByPointer);
    };
  }, [actions, onClose, state.keyboard, state.x, state.y]);

  const run = (action: () => void) => () => { action(); onClose(); };
  return <div ref={rootRef} className="player-marking-menu" style={{ left: state.x, top: state.y }} role="menu" aria-label={`${t("playerHub.commandsForPrefix")} ${state.athlete.name}`} data-armed={armed ?? "cancel"}>
    <button type="button" className={`marking-east${armed === "east" ? " armed" : ""}`} role="menuitem" disabled={!state.athlete.teamName} onClick={run(onFilterTeam)}><IconBuilding size={18} /><span>{t("playerHub.teamWord")}</span></button>
    <button type="button" className={`marking-west${armed === "west" ? " armed" : ""}`} role="menuitem" disabled={!state.athlete.strongestRole} onClick={run(onFilterRole)}><IconUserSearch size={18} /><span>{t("playerHub.roleWord")}</span></button>
    <button type="button" className={`marking-south${armed === "south" ? " armed" : ""}`} role="menuitem" onClick={run(onClearFilters)}><IconFilter size={18} /><span>{t("common.clear")}</span></button>
    <div className="marking-center" aria-hidden="true"><strong>{state.athlete.name}</strong><span>{armed ? t("playerHub.release") : t("common.cancel")}</span></div>
  </div>;
}
