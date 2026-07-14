import { beforeEach, describe, expect, it } from "vitest";
import { usePlayerHubUiStore } from "./usePlayerHubUiStore";

describe("usePlayerHubUiStore", () => {
  beforeEach(() => usePlayerHubUiStore.setState({
    query: "", team: "all", contract: "all", selectedId: null,
    masteryQuery: "", defaultTeamInitialized: false, selectedChampion: null,
  }));

  it("keeps workspace selections independently of component mounting", () => {
    const state = usePlayerHubUiStore.getState();
    state.setSelectedId(13);
    state.setSelectedChampion("swordman");
    expect(usePlayerHubUiStore.getState()).toMatchObject({ selectedId: 13, selectedChampion: "swordman" });
  });

  it("initializes the imported player team only once per session", () => {
    usePlayerHubUiStore.getState().initializeDefaultTeam("Northstar");
    expect(usePlayerHubUiStore.getState()).toMatchObject({ team: "Northstar", defaultTeamInitialized: true });
    usePlayerHubUiStore.getState().setTeam("Axiom");
    usePlayerHubUiStore.getState().initializeDefaultTeam("Northstar");
    expect(usePlayerHubUiStore.getState().team).toBe("Axiom");
  });

  it("clears directory filters without clearing inspection state", () => {
    usePlayerHubUiStore.setState({ query: "top", team: "Blue Otter", contract: "contracted", selectedId: 13 });
    usePlayerHubUiStore.getState().clearDirectoryFilters();
    expect(usePlayerHubUiStore.getState()).toMatchObject({ query: "", team: "all", contract: "all", selectedId: 13 });
  });
});
