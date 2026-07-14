import { create } from "zustand";

export type ContractFilter = "all" | "contracted" | "free";
type PlayerHubUiState = {
  query: string;
  team: string;
  contract: ContractFilter;
  selectedId: number | null;
  masteryQuery: string;
  defaultTeamInitialized: boolean;
  selectedChampion: string | null;
  setQuery: (query: string) => void;
  setTeam: (team: string) => void;
  setContract: (contract: ContractFilter) => void;
  setSelectedId: (selectedId: number | null) => void;
  setMasteryQuery: (masteryQuery: string) => void;
  initializeDefaultTeam: (team: string | null) => void;
  setSelectedChampion: (selectedChampion: string | null) => void;
  clearDirectoryFilters: () => void;
};

export const usePlayerHubUiStore = create<PlayerHubUiState>((set) => ({
  query: "",
  team: "all",
  contract: "all",
  selectedId: null,
  masteryQuery: "",
  defaultTeamInitialized: false,
  selectedChampion: null,
  setQuery: (query) => set({ query }),
  setTeam: (team) => set({ team }),
  setContract: (contract) => set({ contract }),
  setSelectedId: (selectedId) => set({ selectedId }),
  setMasteryQuery: (masteryQuery) => set({ masteryQuery }),
  initializeDefaultTeam: (team) => set((state) => state.defaultTeamInitialized
    ? state
    : { team: team ?? "all", contract: "all", defaultTeamInitialized: true }),
  setSelectedChampion: (selectedChampion) => set({ selectedChampion }),
  clearDirectoryFilters: () => set({ query: "", team: "all", contract: "all" }),
}));
