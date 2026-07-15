// User preference loading, parsing, validation, and persistence.

import type { DraftStrategy, DraftTuning, ScoringWeights, UserPreferences } from "../types";

export const defaultScoringWeights: ScoringWeights = {
  performance: 50,
  synergy: 20,
  matchup: 15,
  flexibility: 10,
  draftPresence: 10,
};

// ── Draft strategy presets ──────────────────────────────────────────────────
// Each preset is a named bundle of tuning knobs. Balanced = today's behavior
// (these numbers must stay in sync with the Rust DEFAULT_* constants).

export const STRATEGY_TUNING: Record<Exclude<DraftStrategy, "custom">, DraftTuning> = {
  conservative: {
    // Patches barely move scores; the engine waits for real results.
    patchMaxShift: 0.03,
    patchImpactScale: 15.0,
    patchEvidenceGames: 8.0,
    // High penalty for thin samples; only well-proven champions score well.
    winRateRiskZ: 1.0,
    winRatePriorGames: 30.0,
  },
  balanced: {
    // Mirrors the Rust DEFAULT_* constants exactly — no behavior change.
    patchMaxShift: 0.06,
    patchImpactScale: 25.0,
    patchEvidenceGames: 15.0,
    winRateRiskZ: 0.65,
    winRatePriorGames: 20.0,
  },
  aggressive: {
    // Patches hit hard; a severe nerf drops a champion much further than mild ones.
    patchMaxShift: 0.12,
    patchImpactScale: 50.0,
    patchEvidenceGames: 30.0,
    // Low penalty for thin samples; unproven / newly buffed picks can surface.
    winRateRiskZ: 0.30,
    winRatePriorGames: 10.0,
  },
};

/** Returns the active DraftTuning for a given strategy + stored custom values. */
export function activeTuning(strategy: DraftStrategy, customTuning: DraftTuning): DraftTuning {
  return strategy === "custom" ? customTuning : STRATEGY_TUNING[strategy];
}

export const defaultCustomTuning: DraftTuning = { ...STRATEGY_TUNING.balanced };

export const preferencesStorageKey = "lt-ai-coach.preferences.v1";
export const defaultUserPreferences: UserPreferences = {
  mode: "normal",
  bansPerSide: 3,
  weights: defaultScoringWeights,
  strategy: "balanced",
  customTuning: defaultCustomTuning,
  minimumInteractionGames: 3,
  compactMode: false,
  autoOverlay: true,
  debugMode: false,
};

export function loadUserPreferences(): UserPreferences {
  try {
    return parseUserPreferences(window.localStorage.getItem(preferencesStorageKey));
  } catch {
    return defaultUserPreferences;
  }
}

export function parseUserPreferences(stored: string | null): UserPreferences {
  if (!stored) return defaultUserPreferences;
  try {
    const value = JSON.parse(stored) as Partial<UserPreferences>;
    const strategy = validateStrategy(value.strategy);
    return {
      mode: value.mode === "fearless" || value.mode === "fearless-hard" ? value.mode : "normal",
      bansPerSide: validNumber(value.bansPerSide, 1, 5) && Number.isInteger(value.bansPerSide) ? value.bansPerSide : defaultUserPreferences.bansPerSide,
      weights: validateScoringWeights(value.weights),
      strategy,
      customTuning: validateDraftTuning(value.customTuning),
      minimumInteractionGames: validNumber(value.minimumInteractionGames, 1, 100) ? value.minimumInteractionGames : defaultUserPreferences.minimumInteractionGames,
      compactMode: value.compactMode === true,
      autoOverlay: value.autoOverlay !== false,
      debugMode: value.debugMode === true,
    };
  } catch {
    return defaultUserPreferences;
  }
}

export function saveUserPreferences(preferences: UserPreferences) {
  try {
    window.localStorage.setItem(preferencesStorageKey, JSON.stringify(preferences));
  } catch {
    // Storage can be unavailable in restricted browser previews.
  }
}

function validateStrategy(value: unknown): DraftStrategy {
  if (value === "conservative" || value === "balanced" || value === "aggressive" || value === "custom") return value;
  return "balanced";
}

function validateDraftTuning(value: unknown): DraftTuning {
  if (!value || typeof value !== "object") return defaultCustomTuning;
  const c = value as Partial<Record<keyof DraftTuning, unknown>>;
  return {
    patchMaxShift: validNumber(c.patchMaxShift, 0, 1) ? c.patchMaxShift : defaultCustomTuning.patchMaxShift,
    patchImpactScale: validNumber(c.patchImpactScale, 1, 200) ? c.patchImpactScale : defaultCustomTuning.patchImpactScale,
    patchEvidenceGames: validNumber(c.patchEvidenceGames, 1, 200) ? c.patchEvidenceGames : defaultCustomTuning.patchEvidenceGames,
    winRateRiskZ: validNumber(c.winRateRiskZ, 0, 5) ? c.winRateRiskZ : defaultCustomTuning.winRateRiskZ,
    winRatePriorGames: validNumber(c.winRatePriorGames, 1, 200) ? c.winRatePriorGames : defaultCustomTuning.winRatePriorGames,
  };
}

function validateScoringWeights(value: unknown): ScoringWeights {
  if (!value || typeof value !== "object") return defaultScoringWeights;
  const candidate = value as Partial<Record<keyof ScoringWeights, unknown>>;
  return {
    performance: validNumber(candidate.performance, 0, 100) ? candidate.performance : defaultScoringWeights.performance,
    synergy: validNumber(candidate.synergy, 0, 100) ? candidate.synergy : defaultScoringWeights.synergy,
    matchup: validNumber(candidate.matchup, 0, 100) ? candidate.matchup : defaultScoringWeights.matchup,
    flexibility: validNumber(candidate.flexibility, 0, 100) ? candidate.flexibility : defaultScoringWeights.flexibility,
    draftPresence: validNumber(candidate.draftPresence, 0, 100) ? candidate.draftPresence : defaultScoringWeights.draftPresence,
  };
}

function validNumber(value: unknown, minimum: number, maximum: number): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= minimum && value <= maximum;
}
