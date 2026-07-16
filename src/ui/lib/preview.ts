// Placeholder recommendations used only in the browser preview (no Tauri
// backend), so the draft board still renders something useful during design.

import type { DraftChampion, Recommendation, RecommendationShortlist, TeamProjection } from "../types";

export function previewRecommendations(champions: DraftChampion[]): RecommendationShortlist {
  const rows = champions.slice(0, 8).map((champion, index): Recommendation => ({
    championId: champion.id,
    championName: champion.name,
    portrait: champion.portrait,
    score: 91 - index * 4.5,
    suggestedRole: ["mid", "bot", "support"][index % 3],
    adjustedWinRate: 0.574 - index * 0.009,
    roleWinRate: null,
    games: 42 - index * 3,
    confidence: 0.8,
    flexibility: 0.5,
    synergyScore: 0.5,
    matchupScore: 0.5,
    interactionGames: 12,
    reasons: [
      { text: index % 2 === 0 ? "Strong fit for the open role" : "Reliable performance on this patch", tone: "positive" },
      { text: index % 3 === 0 ? "Adds useful draft flexibility" : "Favorable matchup profile", tone: "positive" },
      { text: "Weak into the enemy support pick", tone: "negative" },
      { text: "Backed by recent match evidence", tone: "neutral" },
    ],
    athleteContext: null,
  }));
  const projection: TeamProjection = { assignmentsConsidered: 0, confidence: 0, champions: [] };
  return {
    banRecommendations: rows.slice(0, 4),
    pickRecommendations: rows.slice(4, 8),
    banPool: rows.slice(0, 4),
    pickPool: rows.slice(4, 8),
    pickExclusions: [],
    banExclusions: [],
    blueProjection: projection,
    redProjection: projection,
  };
}
