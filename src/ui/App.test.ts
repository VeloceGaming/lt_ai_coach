import { describe, expect, it } from "vitest";

import { calculateDraftTurn } from "./lib/draft";
import { defaultCustomTuning, parseUserPreferences } from "./lib/preferences";

describe("parseUserPreferences", () => {
  it("restores saved draft and scoring preferences", () => {
    expect(parseUserPreferences(JSON.stringify({
      mode: "fearless-hard",
      bansPerSide: 5,
      weights: {
        performance: 40,
        synergy: 25,
        matchup: 20,
        flexibility: 10,
        draftOrder: 5,
      },
      minimumInteractionGames: 8,
      compactMode: true,
      debugMode: true,
    }))).toEqual({
      mode: "fearless-hard",
      bansPerSide: 5,
      weights: {
        performance: 40,
        synergy: 25,
        matchup: 20,
        flexibility: 10,
        draftOrder: 5,
        draftPresence: 10,
      },
      strategy: "balanced",
      customTuning: defaultCustomTuning,
      minimumInteractionGames: 8,
      compactMode: true,
      debugMode: true,
      autoOverlay: true,
    });
  });

  it("falls back when storage is malformed", () => {
    expect(parseUserPreferences("{invalid")).toMatchObject({
      mode: "normal",
      bansPerSide: 3,
      minimumInteractionGames: 3,
      compactMode: false,
      debugMode: false,
    });
  });

  it("validates each saved field independently", () => {
    expect(parseUserPreferences(JSON.stringify({
      mode: "unknown",
      weights: {
        performance: 200,
        synergy: 35,
      },
      minimumInteractionGames: -1,
    }))).toEqual({
      mode: "normal",
      bansPerSide: 3,
      weights: {
        performance: 50,
        synergy: 35,
        matchup: 15,
        flexibility: 10,
        draftOrder: 5,
        draftPresence: 10,
      },
      strategy: "balanced",
      customTuning: defaultCustomTuning,
      minimumInteractionGames: 3,
      compactMode: false,
      debugMode: false,
      autoOverlay: true,
    });
  });
});

describe("calculateDraftTurn", () => {
  const state = (blueBans: number, redBans: number, bluePicks = 0, redPicks = 0) => ({
    blueBans: Array(blueBans).fill("blue-ban"),
    redBans: Array(redBans).fill("red-ban"),
    bluePicks: Array(bluePicks).fill("blue-pick"),
    redPicks: Array(redPicks).fill("red-pick"),
  });

  it.each([
    [state(0, 0), "ban", "blue", "Blue ban 1"],
    [state(1, 0), "ban", "red", "Red ban 1"],
    [state(1, 1), "ban", "blue", "Blue ban 2"],
    [state(2, 1), "ban", "red", "Red ban 2"],
    [state(2, 2), "ban", "blue", "Blue ban 3"],
    [state(3, 2), "ban", "red", "Red ban 3"],
    [state(3, 3), "pick", "blue", "Blue pick 1"],
    [state(3, 3, 1, 0), "pick", "red", "Red pick 1"],
    [state(3, 3, 1, 1), "pick", "red", "Red pick 2"],
    [state(3, 3, 1, 2), "pick", "blue", "Blue pick 2"],
    [state(3, 3, 2, 2), "pick", "blue", "Blue pick 3"],
    [state(3, 3, 3, 2), "pick", "red", "Red pick 3"],
    [state(3, 3, 3, 3), "pick", "red", "Red pick 4"],
    [state(3, 3, 3, 4), "pick", "blue", "Blue pick 4"],
    [state(3, 3, 4, 4), "pick", "blue", "Blue pick 5"],
    [state(3, 3, 5, 4), "pick", "red", "Red pick 5"],
  ])("calculates the fixed draft sequence", (draft, phase, side, label) => {
    expect(calculateDraftTurn(draft)).toMatchObject({ phase, side, label });
  });

  it("reports a completed draft", () => {
    expect(calculateDraftTurn(state(3, 3, 5, 5))).toEqual({
      phase: "complete",
      side: null,
      label: "Draft complete",
      progress: "16/16 actions",
      ordinal: 0,
      actionNumber: 16,
      totalActions: 16,
    });
  });

  it.each([1, 2, 3, 4, 5])("supports %i bans per side", (bansPerSide) => {
    expect(calculateDraftTurn(state(bansPerSide - 1, bansPerSide - 1), bansPerSide)).toMatchObject({
      phase: "ban",
      side: "blue",
      ordinal: bansPerSide,
      totalActions: bansPerSide * 2,
    });
    expect(calculateDraftTurn(state(bansPerSide, bansPerSide), bansPerSide)).toMatchObject({ phase: "pick", side: "blue" });
    expect(calculateDraftTurn(state(bansPerSide, bansPerSide, 5, 5), bansPerSide)).toMatchObject({
      phase: "complete",
      actionNumber: bansPerSide * 2 + 10,
      totalActions: bansPerSide * 2 + 10,
    });
  });
});
