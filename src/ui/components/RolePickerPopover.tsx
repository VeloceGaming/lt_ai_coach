// A small popover for confirming which role a picked champion plays. Shows the
// five lane glyphs in a row plus a clear option; the engine re-runs its
// recommendations honoring the confirmed role. Positioned near the clicked
// pick (fixed coordinates) and closes on outside click or Escape.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { IconX } from "@tabler/icons-react";
import { RoleGlyph } from "./RoleGlyph";
import { useT } from "../stores/useI18nStore";

const ROLES = ["top", "jungle", "mid", "bot", "support"] as const;

// `current` is the highlighted role (a confirmed override, or the engine's
// inferred role when none is set). `overridden` is true only when the user has
// actually confirmed a role — it gates the Clear action.
export type RolePickerState = { championId: string; championName: string; x: number; y: number; current: string | null; overridden: boolean };

export function RolePickerPopover({ state, onPick, onClear, onClose }: {
  state: RolePickerState;
  onPick: (championId: string, role: string) => void;
  onClear: (championId: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const rootRef = useRef<HTMLDivElement | null>(null);
  // Clamp into the viewport so the popover never spills off the short overlay.
  const [pos, setPos] = useState({ left: state.x, top: state.y });
  useLayoutEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    const left = Math.min(Math.max(6, state.x - width / 2), window.innerWidth - width - 6);
    const top = Math.min(Math.max(6, state.y), window.innerHeight - height - 6);
    setPos({ left, top });
  }, [state.x, state.y]);

  useEffect(() => {
    const onPointer = (event: PointerEvent) => {
      if (event.target instanceof Node && !rootRef.current?.contains(event.target)) onClose();
    };
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  return <div ref={rootRef} className="role-picker" style={{ left: pos.left, top: pos.top }} role="menu" aria-label={`${t("rolePicker.confirmFor")} ${state.championName}`}>
    <div className="role-picker-title">{t("rolePicker.playsAs")}</div>
    <div className="role-picker-roles">
      {ROLES.map((role) => (
        <button
          key={role}
          type="button"
          role="menuitemradio"
          aria-checked={state.current === role}
          className={`role-picker-role${state.current === role ? " active" : ""}`}
          title={t(`role.${role}`)}
          onClick={() => { onPick(state.championId, role); onClose(); }}
        >
          <RoleGlyph role={role} label={t(`role.${role}`)} />
        </button>
      ))}
    </div>
    {state.overridden && (
      <button type="button" className="role-picker-clear" role="menuitem" onClick={() => { onClear(state.championId); onClose(); }}>
        <IconX size={12} stroke={2.4} />{t("rolePicker.clear")}
      </button>
    )}
  </div>;
}
