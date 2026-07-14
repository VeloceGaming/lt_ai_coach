// Small formatting helpers shared across the UI.

import type { ScoringWeights } from "../types";

export function formatAge(seconds: number) {
  if (seconds < 90) return `${Math.round(seconds)}s`;
  if (seconds < 5400) return `${Math.round(seconds / 60)} min`;
  return `${Math.round(seconds / 3600)} h`;
}

export function scoringWeightLabel(key: keyof ScoringWeights) { return { performance: "Performance", synergy: "Ally synergy", matchup: "Enemy matchup", flexibility: "Flexibility", draftOrder: "Draft stage", draftPresence: "Draft presence" }[key]; }

export function formatPercent(value: number) { return `${(value * 100).toFixed(1)}%`; }
export function formatNumber(value: number | null, digits: number) { return value === null ? "N/A" : value.toFixed(digits); }
export function titleCase(value: string) { return value ? `${value[0].toUpperCase()}${value.slice(1)}` : ""; }
