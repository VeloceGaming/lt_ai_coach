// Shared type definitions for the LT AI Coach UI.
// Pure type declarations only — no runtime values live here.

export type AppStatus = {
  backend: string;
  phase: string;
  catalogChampions: number;
  databaseReady: boolean;
};

export type ImportSummary = {
  databasePath: string;
  enabledChampions: number;
  players: number;
  athletesWithStats: number;
  masteryEntries: number;
  teams: number;
  matches: number;
  tournamentMatches: number;
  soloMatches: number;
  picks: number;
  bans: number;
  patchChanges: number;
  patchAdditions: number;
  gameLabel: string | null;
  playerTeamId: number | null;
  exportedAtUnix: number | null;
};

export type AthleteSummary = {
  id: number;
  name: string;
  teamId: number | null;
  teamName: string | null;
  strongestRole: string | null;
};

export type AthleteCoreStats = {
  lastHit: number;
  skillAvoid: number;
  skillHit: number;
  positioning: number;
  controlSpeed: number;
  concentration: number;
  mental: number;
  judgement: number;
};

export type AthleteEffectiveCoreStats = {
  lastHit: number;
  skillAvoid: number;
  skillHit: number;
  positioning: number;
  controlSpeed: number;
  concentration: number;
  mental: number;
  judgement: number;
};

export type AthleteTendencyStats = {
  shotcalling: number;
  roaming: number;
  aggressive: number;
  ego: number;
};

export type AthleteRoleRatings = {
  top: number;
  jungle: number;
  mid: number;
  bottom: number;
  support: number;
};

export type AthleteStats = {
  core: AthleteCoreStats;
  tendencies: AthleteTendencyStats;
  roles: AthleteRoleRatings;
};

export type AthleteMastery = {
  championId: string;
  floorRaw: number;
  valueRaw: number;
  mastery: number;
  statBuff: number;
  recent: boolean;
};

export type AthleteDetail = AthleteSummary & {
  stats: AthleteStats | null;
  masteries: AthleteMastery[];
};

export type AthleteChampionLookup = {
  athleteId: number;
  championId: string;
  mastery: number;
  statBuff: number;
  realizedStatBuff: number;
  recent: boolean;
  baseCore: AthleteCoreStats;
  effectiveCore: AthleteEffectiveCoreStats;
  realizedGain: AthleteEffectiveCoreStats;
  baseCoreAverage: number;
  effectiveCoreAverage: number;
  realizedGainAverage: number;
  cappedStats: number;
};

export type ChampionRoleStat = {
  championId: string;
  championName: string;
  role: string;
  portrait: ChampionPortrait | null;
  games: number;
  currentPatchGames: number;
  effectiveGames: number;
  patchChanged: boolean;
  patchAdded: boolean;
  patchImpact: number;
  patchChanges: PatchChange[];
  wins: number;
  tournamentGames: number;
  soloGames: number;
  winRate: number;
  adjustedWinRate: number;
  /** Confidence-gated win-rate delta explained by pilot quality rather than the
   * champion itself: positive means it wins more than its pilots' independent
   * skill predicts, negative means less. Display-only (automatic tier ranking). */
  pilotWinRateDelta: number;
  confidence: number;
  avgKills: number | null;
  avgDeaths: number | null;
  avgAssists: number | null;
  kda: number | null;
  avgDamage: number | null;
  avgTanking: number | null;
  avgHealing: number | null;
  avgCs: number | null;
  avgGold: number | null;
  avgRating: number | null;
  patchTimeline: ChampionPatchMetricPoint[];
};

export type ChampionPatchMetricPoint = {
  patch: string;
  games: number;
  wins: number;
  winRate: number;
  avgKills: number | null;
  avgDeaths: number | null;
  avgAssists: number | null;
  kda: number | null;
  avgDamage: number | null;
  avgTanking: number | null;
  avgHealing: number | null;
  avgCs: number | null;
  avgGold: number | null;
  avgRating: number | null;
};

export type PatchChange = {
  patch: string;
  championId: string;
  asset: string;
  target: string | null;
  field: string;
  oldValue: number;
  newValue: number;
  impact: number;
};

export type PatchChampionChanges = { championId: string; changes: PatchChange[] };
export type PatchHistoryEntry = { patch: string; changes: PatchChampionChanges[]; additions: string[] };

export type ChampionPortrait = {
  path: string;
  sheetWidth: number;
  sheetHeight: number;
  x: number;
  y: number;
  width: number;
  height: number;
  faceOffsetX?: number;
  faceOffsetY?: number;
  centerOffsetX?: number;
  centerOffsetY?: number;
};

export type RoleStatistics = {
  databasePath: string;
  totalMatches: number;
  currentPatch: string;
  globalWinRate: number;
  priorGames: number;
  reliableGames: number;
  overallRows: ChampionRoleStat[];
  roleRows: ChampionRoleStat[];
};

export type DraftChampion = {
  id: string;
  name: string;
  portrait: ChampionPortrait | null;
  roleFit?: Record<string, number>;
};

export type DraftCatalog = {
  champions: DraftChampion[];
};

/** Whether a reason argues for the pick (positive), warns of a downside
 * (negative), or is purely informational (neutral). Drives its color/icon. */
export type ReasonTone = "positive" | "negative" | "neutral";

export type Reason = {
  text: string;
  tone: ReasonTone;
  /** Stable i18n key and named placeholders supplied by the recommendation
   * engine. Older cached/preview reasons may omit these and fall back to text. */
  translationKey?: string;
  translationValues?: Record<string, string>;
  /** Placeholder name -> champion id. The renderer resolves these through the
   * active language while retaining translationValues as display fallbacks. */
  translationChampionIds?: Record<string, string>;
  /** Placeholder name -> raw role id, resolved through role.* translations. */
  translationRoleIds?: Record<string, string>;
  /** Placeholder name -> translation key, for placeholders whose value is a
   * phrase chosen by the engine rather than a number or a name. Resolved
   * through the active language so the phrase isn't pasted in as English. */
  translationKeys?: Record<string, string>;
};

export type Recommendation = {
  championId: string;
  championName: string;
  portrait: ChampionPortrait | null;
  score: number;
  suggestedRole: string | null;
  adjustedWinRate: number;
  roleWinRate: number | null;
  games: number;
  confidence: number;
  flexibility: number;
  synergyScore: number;
  matchupScore: number;
  interactionGames: number;
  reasons: Reason[];
  athleteContext: RecommendationAthleteContext | null;
};

export type DraftLineup = {
  top?: number | null;
  jungle?: number | null;
  mid?: number | null;
  bot?: number | null;
  support?: number | null;
};

export type RecommendationAthleteContext = {
  athleteId: number;
  role: string;
  mastery: number;
  nominalStatBuff: number;
  realizedStatBuff: number;
  baseCore: AthleteCoreStats;
  effectiveCore: AthleteEffectiveCoreStats;
  realizedGain: AthleteEffectiveCoreStats;
  baseCoreAverage: number;
  effectiveCoreAverage: number;
  realizedGainAverage: number;
  cappedStats: number;
};

export type ScoringWeights = {
  performance: number;
  synergy: number;
  matchup: number;
  flexibility: number;
  draftPresence: number;
};

export type RecommendationShortlist = {
  pickRecommendations: Recommendation[];
  banRecommendations: Recommendation[];
  blueProjection: TeamProjection;
  redProjection: TeamProjection;
};

export type LiveRecommendationResponse = {
  sourceRevision: number;
  sourceContextRevision: number;
  shortlist: RecommendationShortlist;
};

export type TeamProjection = {
  assignmentsConsidered: number;
  confidence: number;
  champions: ChampionRoleProjection[];
};

export type ChampionRoleProjection = {
  championId: string;
  championName: string;
  portrait: ChampionPortrait | null;
  roles: Array<{
    role: string;
    probability: number;
    assigned: boolean;
  }>;
};

export type DraftActionRecord = {
  side: DraftSide;
  actionType: "ban" | "pick";
  championId: string;
};

export type DraftMode = "normal" | "fearless" | "fearless-hard";
export type DraftSide = "blue" | "red";
export type DraftAction = "blue-ban" | "red-ban" | "blue-pick" | "red-pick" | "history-blue" | "history-red";
export type DraftState = { blueBans: string[]; redBans: string[]; bluePicks: string[]; redPicks: string[]; historyBlue: string[]; historyRed: string[]; actionLog: DraftActionRecord[]; };
export type GameRecord = { gameNumber: number; bluePicks: string[]; redPicks: string[] };
export type BridgePhase = "unknown" | "stadiumEntrance" | "draft" | "";
export type BridgeState = {
  connected: boolean;
  revision: number;
  phase: BridgePhase;
  phaseRevision: number;
  blueBans: string[];
  redBans: string[];
  bluePicks: string[];
  redPicks: string[];
  bansPerSide: number | null;
  draftMode: DraftMode | null;
  contextRevision: number;
  matchId: number | null;
  setNumber: number | null;
  blueTeamId: number | null;
  redTeamId: number | null;
  blueStarters: number[];
  redStarters: number[];
  userSide: DraftSide | null;
  completedGames: GameRecord[];
  champions?: DraftChampion[];
};
// label/progress are English strings kept for internal/test use; UI code should
// build a translated display string from phase/side/ordinal/actionNumber/totalActions instead.
export type DraftTurn = { phase: "ban" | "pick" | "complete"; side: DraftSide | null; label: string; progress: string; ordinal: number; actionNumber: number; totalActions: number };

/** How aggressively the engine reacts to patches and unproven champions. */
export type DraftStrategy = "conservative" | "balanced" | "aggressive" | "custom";

/** Engine-tuning knobs that travel with every recommendation request. */
export type DraftTuning = {
  /** Max score shift a patch can apply before real games exist (0–1 fraction). */
  patchMaxShift: number;
  /** Saturation scale for patch signal — higher gives more dynamic range. */
  patchImpactScale: number;
  /** Games at which the patch prior is half-faded by real results. */
  patchEvidenceGames: number;
  /** Uncertainty penalty on thin win-rate samples — higher = more conservative. */
  winRateRiskZ: number;
  /** Bayesian prior games for win-rate uncertainty — higher = needs more data. */
  winRatePriorGames: number;
};

export type UserPreferences = { mode: DraftMode; bansPerSide: number; weights: ScoringWeights; strategy: DraftStrategy; customTuning: DraftTuning; minimumInteractionGames: number; compactMode: boolean; autoOverlay: boolean; debugMode: boolean };
