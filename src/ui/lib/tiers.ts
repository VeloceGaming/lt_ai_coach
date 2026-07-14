import type { ChampionRoleStat } from "../types";

export const tierOrder = ["S", "A", "B", "C", "D", "F"] as const;
export type TierName = (typeof tierOrder)[number];

// How much the blended score leans on win rate vs in-game rating, and how far the
// best/worst-rated champion can swing the score in win-rate-equivalent points.
const WIN_RATE_WEIGHT = 0.7;
const RATING_SPAN = 0.1;

// Pool-level context for ranking: the average win rate to regularize toward, plus
// the rating range so an unknown-scale rating can be normalized relative to peers.
export type TierContext = {
  globalWinRate: number;
  minRating: number | null;
  maxRating: number | null;
};

// Builds the ranking context once from the full champion list. Rating min/max come
// from champions that actually have a rating; everything else stays neutral.
export function buildTierContext(rows: ChampionRoleStat[], globalWinRate: number): TierContext {
  const ratings = rows.map((row) => row.avgRating).filter((value): value is number => value != null);
  return {
    globalWinRate: globalWinRate || 0.5,
    minRating: ratings.length ? Math.min(...ratings) : null,
    maxRating: ratings.length ? Math.max(...ratings) : null,
  };
}

// Converts an unknown-scale rating into a win-rate-equivalent value by placing it
// within the pool's rating range and nudging around the average win rate. Returns
// the neutral average when there is no rating or no spread to compare against.
export function ratingToWinRate(rating: number | null, ctx: TierContext): number {
  if (rating == null || ctx.minRating == null || ctx.maxRating == null || ctx.maxRating <= ctx.minRating) {
    return ctx.globalWinRate;
  }
  const normalized = (rating - ctx.minRating) / (ctx.maxRating - ctx.minRating); // 0..1
  return ctx.globalWinRate + (normalized - 0.5) * RATING_SPAN;
}

// The blended performance score (win-rate units): patch-weighted win rate
// (corrected for pilot quality — see pilotWinRateDelta) blended with normalized
// rating, then pulled toward the average by confidence so small, noisy samples
// can't reach the extreme tiers.
export function performanceScore(row: ChampionRoleStat, ctx: TierContext): number {
  const pilotAdjustedWinRate = row.adjustedWinRate + row.pilotWinRateDelta;
  const ratingWinRate = ratingToWinRate(row.avgRating, ctx);
  const blended = WIN_RATE_WEIGHT * pilotAdjustedWinRate + (1 - WIN_RATE_WEIGHT) * ratingWinRate;
  const confidence = Math.max(0, Math.min(1, row.confidence));
  return ctx.globalWinRate + (blended - ctx.globalWinRate) * confidence;
}

export function automaticTier(winRate: number): TierName {
  if (winRate >= 0.55) return "S";
  if (winRate >= 0.52) return "A";
  if (winRate >= 0.49) return "B";
  if (winRate >= 0.46) return "C";
  if (winRate >= 0.43) return "D";
  return "F";
}

export function championTier(row: ChampionRoleStat, overrides: Record<string, string>, ctx: TierContext): TierName {
  const override = overrides[row.championId];
  return tierOrder.includes(override as TierName) ? override as TierName : automaticTier(performanceScore(row, ctx));
}

export function patchDirection(delta: number): "buff" | "nerf" | "unchanged" {
  if (delta > 0.00005) return "buff";
  if (delta < -0.00005) return "nerf";
  return "unchanged";
}

// Prettifies one raw internal field-name segment (the generic fallback used
// when no translation covers it): underscores to spaces, title-cased.
function prettifySegment(segment: string): string {
  return segment.replaceAll("_", " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

// The game's balance data uses hundreds of raw internal field names (many
// undocumented/typo'd), so this can't be a lookup table for every one of
// them. Instead: a curated `field.<token>` translation exists for the common
// stat/skill fields (see strings.ts); anything else falls back to the plain
// English-style prettifier below, exactly as before this function took a
// translator. Passing no `t` reproduces the original untranslated behavior
// verbatim (see tiers.test.ts).
export function readablePatchField(value: string, t?: (key: string) => string): string {
  return value
    .split(".")
    .map((segment) => {
      const key = `field.${segment.toLowerCase()}`;
      const translated = t?.(key);
      return translated && translated !== key ? translated : prettifySegment(segment);
    })
    .join(" · ");
}
