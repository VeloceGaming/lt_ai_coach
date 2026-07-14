import { beforeEach, describe, expect, it } from "vitest";

import { useDraftStore } from "./useDraftStore";
import { usePreferencesStore } from "./usePreferencesStore";

// Reset both stores to a known clean state before each test. The draft store is
// a module singleton, so without this, state would leak between tests.
function freshDraft() {
  return { blueBans: [], redBans: [], bluePicks: [], redPicks: [], historyBlue: [], historyRed: [], actionLog: [] };
}

beforeEach(() => {
  usePreferencesStore.setState({ mode: "normal" });
  useDraftStore.setState({ liveMatchId: null, draft: freshDraft(), history: [], currentGame: 1, completedGames: [] });
});

describe("useDraftStore: pushChampion", () => {
  it("adds a champion to the named slot and records the action", () => {
    const ok = useDraftStore.getState().pushChampion("knight", "blue-ban");
    expect(ok).toBe(true);
    const { draft } = useDraftStore.getState();
    expect(draft.blueBans).toEqual(["knight"]);
    expect(draft.actionLog).toEqual([{ side: "blue", actionType: "ban", championId: "knight" }]);
  });

  it("rejects a champion already used in the draft and changes nothing", () => {
    useDraftStore.getState().pushChampion("knight", "blue-ban");
    const before = useDraftStore.getState().draft;
    const ok = useDraftStore.getState().pushChampion("knight", "red-pick");
    expect(ok).toBe(false);
    expect(useDraftStore.getState().draft).toBe(before); // unchanged reference
  });

  it("pushes the previous board onto history so undo can revert it", () => {
    useDraftStore.getState().pushChampion("knight", "blue-ban");
    useDraftStore.getState().undo();
    expect(useDraftStore.getState().draft.blueBans).toEqual([]);
    expect(useDraftStore.getState().history).toEqual([]);
  });
});

describe("useDraftStore: removeChampion", () => {
  it("removes a champion and rebuilds the action log", () => {
    useDraftStore.getState().pushChampion("knight", "blue-ban");
    useDraftStore.getState().pushChampion("archer", "blue-ban");
    useDraftStore.getState().removeChampion("blueBans", "knight");
    const { draft } = useDraftStore.getState();
    expect(draft.blueBans).toEqual(["archer"]);
    expect(draft.actionLog).toEqual([{ side: "blue", actionType: "ban", championId: "archer" }]);
  });
});

describe("useDraftStore: resetCurrent", () => {
  it("clears the live board but keeps the action recoverable via history", () => {
    useDraftStore.getState().pushChampion("knight", "blue-ban");
    useDraftStore.getState().resetCurrent();
    expect(useDraftStore.getState().draft.blueBans).toEqual([]);
    useDraftStore.getState().undo();
    expect(useDraftStore.getState().draft.blueBans).toEqual(["knight"]);
  });
});

describe("useDraftStore: Fearless series", () => {
  it("finishGame records the current game's picks and advances to the next", () => {
    useDraftStore.setState({ draft: { ...freshDraft(), bluePicks: ["knight"], redPicks: ["archer"] } });
    useDraftStore.getState().finishGame();
    const { currentGame, completedGames, draft } = useDraftStore.getState();
    expect(currentGame).toBe(2);
    expect(completedGames).toEqual([{ gameNumber: 1, bluePicks: ["knight"], redPicks: ["archer"] }]);
    expect(draft.bluePicks).toEqual([]); // board cleared for the new game
  });

  it("resetSeries clears completed games and returns to game 1 with an empty board", () => {
    useDraftStore.setState({
      currentGame: 3,
      completedGames: [{ gameNumber: 1, bluePicks: ["knight"], redPicks: [] }],
      draft: { ...freshDraft(), bluePicks: ["bard"], historyBlue: ["knight"] },
    });
    useDraftStore.getState().resetSeries();
    const { currentGame, completedGames, draft } = useDraftStore.getState();
    expect(currentGame).toBe(1);
    expect(completedGames).toEqual([]);
    expect(draft.bluePicks).toEqual([]);
    expect(draft.historyBlue).toEqual([]);
  });
});

describe("useDraftStore: live series context", () => {
  it("uses the one-based set number directly for a newly observed match", () => {
    useDraftStore.getState().syncLiveSeries(81, 2, []);
    const state = useDraftStore.getState();
    expect(state.liveMatchId).toBe(81);
    expect(state.currentGame).toBe(2);
    expect(state.completedGames).toEqual([]);
  });

  it("records a completed prior set when the same match advances", () => {
    useDraftStore.getState().syncLiveSeries(81, 1, []);
    useDraftStore.setState({
      draft: {
        ...freshDraft(),
        bluePicks: ["a", "b", "c", "d", "e"],
        redPicks: ["f", "g", "h", "i", "j"],
      },
    });

    useDraftStore.getState().syncLiveSeries(81, 2, [{ gameNumber: 1, bluePicks: ["a", "b", "c", "d", "e"], redPicks: ["f", "g", "h", "i", "j"] }]);
    const state = useDraftStore.getState();
    expect(state.currentGame).toBe(2);
    expect(state.completedGames).toEqual([{ gameNumber: 1, bluePicks: ["a", "b", "c", "d", "e"], redPicks: ["f", "g", "h", "i", "j"] }]);
    expect(state.draft.bluePicks).toEqual([]);
  });

  it("clears prior series state when match identity changes", () => {
    useDraftStore.setState({
      liveMatchId: 81,
      currentGame: 3,
      completedGames: [{ gameNumber: 1, bluePicks: ["a"], redPicks: ["b"] }],
      draft: { ...freshDraft(), bluePicks: ["c"] },
    });

    useDraftStore.getState().syncLiveSeries(82, 1, []);
    const state = useDraftStore.getState();
    expect(state.liveMatchId).toBe(82);
    expect(state.currentGame).toBe(1);
    expect(state.completedGames).toEqual([]);
    expect(state.draft.bluePicks).toEqual([]);
  });
});
