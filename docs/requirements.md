# Product Requirements

## Core Workflow

1. Select or automatically locate a `save_*.data` file.
2. Extract the save and load its enabled champions and match history.
3. Select Normal, Fearless, or Fearless Hard and the user's side.
4. Record bans and picks manually through the champion grid.
5. Update local legality, role coverage, and candidate scores without API use.
6. Call the LLM only through an explicit `Ask AI Coach` action.

## Save Data

The normalized data model must support:

- Enabled champions.
- Tournament and solo match lineups.
- Picks, bans, roles, winners, patch, and teams.
- Kills, deaths, assists, damage, tanking, healing, gold, and lane data.
- Player attributes and player/champion/role history.

## Statistics

- Champion performance must be role-specific.
- Synergy and counter statistics must be derived from complete match lineups.
- Lane matchup statistics must compare champions assigned to the same role.
- Player proficiency must distinguish champion and role.
- Every derived value must include sample size and confidence.
- Small samples must use Bayesian smoothing and confidence penalties.

## Draft Rules

- Normal does not retain restrictions between sets.
- Fearless prevents each team from reusing its own previous picks.
- Fearless Hard prevents either team from reusing any previous pick.
- Picks, bans, disabled champions, and Fearless exclusions are unavailable.
- Undo, reset, next set, and complete series reset are required.

## Recommendation

The deterministic engine must:

- Generate legal champion-role candidates.
- Generate feasible five-role lineup assignments.
- Penalize weak-role placement and uncovered roles.
- Consider role performance, player proficiency, synergy, counters,
  flexibility, sample size, and confidence.

The LLM receives aggregated evidence rather than raw replay files. It returns:

- Recommended immediate pick or ban.
- Intended role.
- Expected lineup plan.
- Alternatives.
- Counter-pick and role-assignment risks.
- Evidence and sample sizes supporting the recommendation.

The LLM must not control legality or invent missing statistics.

## Initial Non-Goals

- Reading the live B/P screen.
- Clicking or controlling the game.
- Native game hooks.
- Changing installed mods or their configuration.
- Calling an LLM after every manual draft action.

