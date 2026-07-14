// Formats a single tracked balance change into a readable label + value, shared by
// the champion detail panel and the Patch Notes screen.

import { readablePatchField } from "./tiers";

// Balance values are stored in the game's raw internal units; convert the few
// fields whose display unit differs (cooldown/charge in 1/60s, move speed ×16.67,
// range ×1000) so patch notes read like the in-game numbers.
function toDisplayValue(value: number, field?: string): number {
  const name = (field ?? "").toLowerCase();
  if (name.includes("cooltime") || name.includes("charge_time") || name.includes("interval")) return value / 60;
  if (name.includes("move_speed") || name === "speed") return value * 0.06;
  if (name.includes("range")) return value / 1000;
  return value;
}

export function formatPatchValue(value: number, field?: string): string {
  const display = toDisplayValue(value, field);
  if (Number.isInteger(display)) return display.toFixed(0);
  return display.toFixed(2).replace(/\.?0+$/, "");
}

// Per-level growth stats (asset === "growth") get one composite phrase instead
// of two segments joined with a middot — matches the game's own patch-note
// wording ("每級物理攻擊成長" vs a mechanical "Growth · Attack").
const GROWTH_FIELD_KEYS: Record<string, string> = {
  attack: "patchKey.growth_attack",
  hp: "patchKey.growth_hp",
  defence: "patchKey.growth_defence",
  magic_power: "patchKey.growth_magic_power",
  magic_resistance: "patchKey.growth_magic_resistance",
  move_speed: "patchKey.growth_move_speed",
  range: "patchKey.growth_range",
};

export function formatPatchLabel(asset: string, target: string | null, field: string, t?: (key: string) => string): string {
  const skill = asset.match(/^skill_([a-z])$/i);
  if (skill) return `${skill[1].toUpperCase()} ${readablePatchField(field, t).toLowerCase()}`;
  // Attack speed is stored as stat.attack.interval or stat.attack.cooltime in the
  // game's data, which would produce "Attack · Cooltime". Normalise all attack-speed
  // variants to the in-game label used everywhere else.
  const attackSpeedLabel = t ? `${t("field.attack")} · ${t("field.attack_speed")}` : "Attack · Speed";
  if (target === "attack" && (field === "interval" || field === "cooltime")) return attackSpeedLabel;
  if (field === "attack_speed") return attackSpeedLabel;
  if (field.startsWith("attack_speed.")) return `${attackSpeedLabel} · ${readablePatchField(field.slice("attack_speed.".length), t)}`;
  if (asset === "growth" && GROWTH_FIELD_KEYS[field]) return t?.(GROWTH_FIELD_KEYS[field]) ?? readablePatchField(`growth.${field}`);
  // The game names its primary skill container `skill`; Monk uses the more
  // descriptive `heal_skill`. Both are Skill1 in the UI so every champion's
  // first and second skills use the same labels.
  const displayAsset = asset === "skill" || asset === "heal_skill" ? "skill1" : asset;
  // `stat` is only the game's container for base attributes. Showing it adds
  // no meaning ("Stat · HP"), so expose the actual attribute label directly.
  return readablePatchField([displayAsset === "stat" ? null : displayAsset, target, field].filter(Boolean).join("."), t);
}
