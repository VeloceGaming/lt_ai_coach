use crate::statistics::{current_patch, patch_recency_weight_case};
use rusqlite::Connection;
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, Debug, Default)]
pub struct InteractionSample {
    // Raw counts of games/wins actually played. Used for the minimum-sample
    // gate and for the game counts shown in recommendation reasons.
    pub games: usize,
    pub wins: usize,
    // Patch-recency-weighted equivalents: each game is weighted by how many
    // patches old it is (see patch_recency_weight_case), so a pairing's recent
    // performance leads. The win rate is computed from these; confidence keeps
    // using the raw `games` (see `smooth`) so uncertainty isn't double-counted.
    pub effective_games: f64,
    pub weighted_wins: f64,
}

// SQL fragment: 1 when this side (`a`) won the match, else 0. `a`/`m` are the
// table aliases used by every interaction query below.
const WIN_EXPRESSION: &str =
    "(CASE WHEN (a.side = 'blue' AND m.blue_win = 1) OR (a.side = 'red' AND m.blue_win = 0) \
       THEN 1 ELSE 0 END)";

#[derive(Default)]
pub struct InteractionEvidence {
    synergy_roles: BTreeMap<(String, String, String, String), InteractionSample>,
    synergy_champions: BTreeMap<(String, String), InteractionSample>,
    matchup_roles: BTreeMap<(String, String, String, String), InteractionSample>,
    matchup_champions: BTreeMap<(String, String), InteractionSample>,
}

#[derive(Clone, Copy, Debug)]
pub struct InteractionEstimate {
    pub win_rate: f64,
    pub games: usize,
}

impl InteractionEvidence {
    pub fn champion_synergy(
        &self,
        champion: &str,
        ally: &str,
        prior: f64,
        minimum_games: usize,
    ) -> InteractionEstimate {
        smooth(
            self.synergy_champions.get(&ordered_pair(champion, ally)),
            prior,
            16.0,
            minimum_games,
        )
    }

    pub fn champion_matchup(
        &self,
        champion: &str,
        enemy: &str,
        prior: f64,
        minimum_games: usize,
    ) -> InteractionEstimate {
        smooth(
            self.matchup_champions
                .get(&(champion.to_string(), enemy.to_string())),
            prior,
            16.0,
            minimum_games,
        )
    }

    pub fn synergy(
        &self,
        champion: &str,
        role: &str,
        ally: &str,
        ally_role: &str,
        prior: f64,
        minimum_games: usize,
    ) -> InteractionEstimate {
        let champion_key = ordered_pair(champion, ally);
        let champion_estimate =
            self.champion_synergy(&champion_key.0, &champion_key.1, prior, minimum_games);
        let role_key = ordered_role_pair(champion, role, ally, ally_role);
        smooth(
            self.synergy_roles.get(&role_key),
            champion_estimate.win_rate,
            10.0,
            minimum_games,
        )
    }

    pub fn matchup(
        &self,
        champion: &str,
        role: &str,
        enemy: &str,
        enemy_role: &str,
        prior: f64,
        minimum_games: usize,
    ) -> InteractionEstimate {
        if role != enemy_role {
            return InteractionEstimate {
                win_rate: prior,
                games: 0,
            };
        }
        let champion_estimate = self.champion_matchup(champion, enemy, prior, minimum_games);
        smooth(
            self.matchup_roles.get(&(
                champion.to_string(),
                role.to_string(),
                enemy.to_string(),
                enemy_role.to_string(),
            )),
            champion_estimate.win_rate,
            10.0,
            minimum_games,
        )
    }

    #[cfg(test)]
    pub(crate) fn insert_role_synergy_sample(
        &mut self,
        champion: &str,
        role: &str,
        ally: &str,
        ally_role: &str,
        games: usize,
        wins: usize,
    ) {
        self.synergy_roles.insert(
            ordered_role_pair(champion, role, ally, ally_role),
            InteractionSample {
                games,
                wins,
                effective_games: games as f64,
                weighted_wins: wins as f64,
            },
        );
    }

    #[cfg(test)]
    pub(crate) fn insert_role_matchup_sample(
        &mut self,
        champion: &str,
        role: &str,
        enemy: &str,
        enemy_role: &str,
        games: usize,
        wins: usize,
    ) {
        self.matchup_roles.insert(
            (
                champion.to_string(),
                role.to_string(),
                enemy.to_string(),
                enemy_role.to_string(),
            ),
            InteractionSample {
                games,
                wins,
                effective_games: games as f64,
                weighted_wins: wins as f64,
            },
        );
    }
}

pub fn query_interactions(database_path: &Path) -> Result<InteractionEvidence, String> {
    let connection = Connection::open(database_path)
        .map_err(|error| format!("Could not open interaction database: {error}"))?;
    // Build the patch-recency weight once and reuse it across all four queries.
    // If the patch can't be resolved (no save_state / no patched matches) the
    // helper falls back to "1.0" — i.e. unweighted, the pre-patch behaviour.
    let current_patch = current_patch(&connection).unwrap_or_default();
    let weight = patch_recency_weight_case(&connection, &current_patch, "m.patch")?;
    Ok(InteractionEvidence {
        synergy_roles: query_synergy_roles(&connection, &weight)?,
        synergy_champions: query_synergy_champions(&connection, &weight)?,
        matchup_roles: query_matchup_roles(&connection, &weight)?,
        matchup_champions: query_matchup_champions(&connection, &weight)?,
    })
}

fn query_synergy_roles(
    connection: &Connection,
    weight: &str,
) -> Result<BTreeMap<(String, String, String, String), InteractionSample>, String> {
    let win = WIN_EXPRESSION;
    let mut statement = connection
        .prepare(&format!(
            "SELECT a.champion_id, a.role, b.champion_id, b.role,
                    COUNT(*),
                    SUM({win}),
                    SUM({weight}),
                    SUM({win} * {weight})
             FROM picks a
             JOIN picks b ON b.match_key = a.match_key
                         AND b.side = a.side
                         AND b.slot > a.slot
             JOIN matches m ON m.match_key = a.match_key
             GROUP BY a.champion_id, a.role, b.champion_id, b.role",
        ))
        .map_err(|error| format!("Could not prepare role synergy query: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            let champion: String = row.get(0)?;
            let role: String = row.get(1)?;
            let ally: String = row.get(2)?;
            let ally_role: String = row.get(3)?;
            Ok((
                ordered_role_pair(&champion, &role, &ally, &ally_role),
                InteractionSample {
                    games: row.get(4)?,
                    wins: row.get(5)?,
                    effective_games: row.get(6)?,
                    weighted_wins: row.get(7)?,
                },
            ))
        })
        .map_err(|error| format!("Could not query role synergies: {error}"))?;
    collect_rows(rows, "role synergies")
}

fn query_synergy_champions(
    connection: &Connection,
    weight: &str,
) -> Result<BTreeMap<(String, String), InteractionSample>, String> {
    let win = WIN_EXPRESSION;
    let mut statement = connection
        .prepare(&format!(
            "SELECT a.champion_id, b.champion_id,
                    COUNT(*),
                    SUM({win}),
                    SUM({weight}),
                    SUM({win} * {weight})
             FROM picks a
             JOIN picks b ON b.match_key = a.match_key
                         AND b.side = a.side
                         AND b.slot > a.slot
             JOIN matches m ON m.match_key = a.match_key
             GROUP BY a.champion_id, b.champion_id",
        ))
        .map_err(|error| format!("Could not prepare champion synergy query: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            let champion: String = row.get(0)?;
            let ally: String = row.get(1)?;
            Ok((
                ordered_pair(&champion, &ally),
                InteractionSample {
                    games: row.get(2)?,
                    wins: row.get(3)?,
                    effective_games: row.get(4)?,
                    weighted_wins: row.get(5)?,
                },
            ))
        })
        .map_err(|error| format!("Could not query champion synergies: {error}"))?;
    collect_rows(rows, "champion synergies")
}

fn query_matchup_roles(
    connection: &Connection,
    weight: &str,
) -> Result<BTreeMap<(String, String, String, String), InteractionSample>, String> {
    let win = WIN_EXPRESSION;
    let mut statement = connection
        .prepare(&format!(
            "SELECT a.champion_id, a.role, b.champion_id, b.role,
                    COUNT(*),
                    SUM({win}),
                    SUM({weight}),
                    SUM({win} * {weight})
             FROM picks a
             JOIN picks b ON b.match_key = a.match_key
                         AND b.side <> a.side
                         AND b.role = a.role
             JOIN matches m ON m.match_key = a.match_key
             GROUP BY a.champion_id, a.role, b.champion_id, b.role",
        ))
        .map_err(|error| format!("Could not prepare role matchup query: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                (row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?),
                InteractionSample {
                    games: row.get(4)?,
                    wins: row.get(5)?,
                    effective_games: row.get(6)?,
                    weighted_wins: row.get(7)?,
                },
            ))
        })
        .map_err(|error| format!("Could not query role matchups: {error}"))?;
    collect_rows(rows, "role matchups")
}

fn query_matchup_champions(
    connection: &Connection,
    weight: &str,
) -> Result<BTreeMap<(String, String), InteractionSample>, String> {
    let win = WIN_EXPRESSION;
    let mut statement = connection
        .prepare(&format!(
            "SELECT a.champion_id, b.champion_id,
                    COUNT(*),
                    SUM({win}),
                    SUM({weight}),
                    SUM({win} * {weight})
             FROM picks a
             JOIN picks b ON b.match_key = a.match_key
                         AND b.side <> a.side
                         AND b.role = a.role
             JOIN matches m ON m.match_key = a.match_key
             GROUP BY a.champion_id, b.champion_id",
        ))
        .map_err(|error| format!("Could not prepare champion matchup query: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                (row.get(0)?, row.get(1)?),
                InteractionSample {
                    games: row.get(2)?,
                    wins: row.get(3)?,
                    effective_games: row.get(4)?,
                    weighted_wins: row.get(5)?,
                },
            ))
        })
        .map_err(|error| format!("Could not query champion matchups: {error}"))?;
    collect_rows(rows, "champion matchups")
}

fn collect_rows<K: Ord, I>(rows: I, label: &str) -> Result<BTreeMap<K, InteractionSample>, String>
where
    I: Iterator<Item = rusqlite::Result<(K, InteractionSample)>>,
{
    let mut result = BTreeMap::<K, InteractionSample>::new();
    for row in rows {
        let (key, sample) = row.map_err(|error| format!("Could not read {label}: {error}"))?;
        let total = result.entry(key).or_default();
        total.games += sample.games;
        total.wins += sample.wins;
        total.effective_games += sample.effective_games;
        total.weighted_wins += sample.weighted_wins;
    }
    Ok(result)
}

fn smooth(
    sample: Option<&InteractionSample>,
    prior: f64,
    prior_games: f64,
    minimum_games: usize,
) -> InteractionEstimate {
    let Some(sample) = sample.filter(|sample| sample.games >= minimum_games) else {
        return InteractionEstimate {
            win_rate: prior,
            games: 0,
        };
    };
    // Win rate from the patch-weighted totals so stale-patch games for a
    // changed champion barely move it; `games` stays raw for display and for
    // downstream confidence (avoids double-counting uncertainty).
    InteractionEstimate {
        win_rate: (sample.weighted_wins + prior * prior_games)
            / (sample.effective_games + prior_games),
        games: sample.games,
    }
}

fn ordered_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn ordered_role_pair(
    champion: &str,
    role: &str,
    ally: &str,
    ally_role: &str,
) -> (String, String, String, String) {
    if (champion, role) <= (ally, ally_role) {
        (
            champion.to_string(),
            role.to_string(),
            ally.to_string(),
            ally_role.to_string(),
        )
    } else {
        (
            ally.to_string(),
            ally_role.to_string(),
            champion.to_string(),
            role.to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A sample with no patch discount applied (effective == raw), for tests
    // that exercise `smooth` directly rather than the patch-weighted queries.
    fn unweighted_sample(games: usize, wins: usize) -> InteractionSample {
        InteractionSample {
            games,
            wins,
            effective_games: games as f64,
            weighted_wins: wins as f64,
        }
    }

    #[test]
    fn sparse_samples_fall_back_to_the_prior() {
        let sample = unweighted_sample(2, 2);
        let estimate = smooth(Some(&sample), 0.48, 10.0, 3);
        assert_eq!(estimate.games, 0);
        assert_eq!(estimate.win_rate, 0.48);
    }

    #[test]
    fn smoothing_limits_small_sample_extremes() {
        let sample = unweighted_sample(4, 4);
        let estimate = smooth(Some(&sample), 0.50, 10.0, 3);
        assert!(estimate.win_rate > 0.50);
        assert!(estimate.win_rate < 0.70);
    }

    #[test]
    fn role_matchups_reject_cross_role_evidence() {
        let mut evidence = InteractionEvidence::default();
        evidence.insert_role_matchup_sample("alpha", "top", "beta", "mid", 20, 14);

        let estimate = evidence.matchup("alpha", "top", "beta", "mid", 0.48, 3);

        assert_eq!(estimate.games, 0);
        assert_eq!(estimate.win_rate, 0.48);
    }

    #[test]
    fn matchup_queries_only_aggregate_lane_opponents() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE matches (
                    match_key TEXT PRIMARY KEY,
                    blue_win INTEGER NOT NULL,
                    patch TEXT
                );
                CREATE TABLE picks (
                    match_key TEXT NOT NULL,
                    side TEXT NOT NULL,
                    slot INTEGER NOT NULL,
                    champion_id TEXT NOT NULL,
                    role TEXT NOT NULL
                );
                CREATE TABLE patch_changed_champions (
                    patch TEXT NOT NULL,
                    champion_id TEXT NOT NULL,
                    PRIMARY KEY (patch, champion_id)
                );
                INSERT INTO matches VALUES ('same-role', 1, '2026.0.0'), ('cross-role', 0, '2026.0.0');
                INSERT INTO picks VALUES
                    ('same-role', 'blue', 0, 'alpha', 'top'),
                    ('same-role', 'red', 0, 'beta', 'top'),
                    ('cross-role', 'blue', 0, 'alpha', 'top'),
                    ('cross-role', 'red', 0, 'beta', 'mid');",
            )
            .unwrap();

        let champion_matchups = query_matchup_champions(&connection, "1.0").unwrap();
        let role_matchups = query_matchup_roles(&connection, "1.0").unwrap();
        let champion_sample = champion_matchups
            .get(&("alpha".to_string(), "beta".to_string()))
            .unwrap();

        assert_eq!(champion_sample.games, 1);
        assert_eq!(champion_sample.wins, 1);
        assert!(role_matchups.contains_key(&(
            "alpha".to_string(),
            "top".to_string(),
            "beta".to_string(),
            "top".to_string(),
        )));
        assert!(!role_matchups.contains_key(&(
            "alpha".to_string(),
            "top".to_string(),
            "beta".to_string(),
            "mid".to_string(),
        )));
    }

    #[test]
    fn champion_level_queries_preserve_direction_and_confidence() {
        let evidence = InteractionEvidence {
            synergy_champions: BTreeMap::from([(
                ordered_pair("alpha", "beta"),
                unweighted_sample(20, 14),
            )]),
            matchup_champions: BTreeMap::from([(
                ("alpha".to_string(), "beta".to_string()),
                unweighted_sample(20, 14),
            )]),
            ..Default::default()
        };

        let synergy = evidence.champion_synergy("beta", "alpha", 0.50, 3);
        let matchup = evidence.champion_matchup("alpha", "beta", 0.50, 3);
        let reverse = evidence.champion_matchup("beta", "alpha", 0.50, 3);

        assert_eq!(synergy.games, 20);
        assert_eq!(matchup.games, 20);
        assert!(synergy.win_rate > 0.50);
        assert!(matchup.win_rate > 0.50);
        assert_eq!(reverse.games, 0);
        assert_eq!(reverse.win_rate, 0.50);
    }

    #[test]
    fn older_patch_synergy_games_are_recency_weighted() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE matches (
                    match_key TEXT PRIMARY KEY,
                    blue_win INTEGER NOT NULL,
                    patch TEXT
                );
                CREATE TABLE picks (
                    match_key TEXT NOT NULL,
                    side TEXT NOT NULL,
                    slot INTEGER NOT NULL,
                    champion_id TEXT NOT NULL,
                    role TEXT NOT NULL
                );
                -- Two games one patch back (2025.0.0) and one on the current
                -- patch (2026.0.0). Recency weighting applies to every pair.
                INSERT INTO matches VALUES
                    ('old-1', 1, '2025.0.0'),
                    ('old-2', 1, '2025.0.0'),
                    ('new-1', 1, '2026.0.0');
                INSERT INTO picks VALUES
                    ('old-1', 'blue', 0, 'alpha', 'top'),
                    ('old-1', 'blue', 1, 'beta', 'mid'),
                    ('old-2', 'blue', 0, 'alpha', 'top'),
                    ('old-2', 'blue', 1, 'beta', 'mid'),
                    ('new-1', 'blue', 0, 'alpha', 'top'),
                    ('new-1', 'blue', 1, 'beta', 'mid');",
            )
            .unwrap();

        let weight = patch_recency_weight_case(&connection, "2026.0.0", "m.patch").unwrap();
        let synergies = query_synergy_champions(&connection, &weight).unwrap();

        // alpha+beta: raw 3 games; the two 2025 games are one patch back so each
        // weighs 0.6, the current-patch game weighs 1.0 -> 0.6*2 + 1.0 = 2.2.
        let pair = synergies.get(&ordered_pair("alpha", "beta")).unwrap();
        assert_eq!(pair.games, 3);
        assert!((pair.effective_games - 2.2).abs() < 1e-9);
        // All three games were wins, weighted the same way.
        assert!((pair.weighted_wins - 2.2).abs() < 1e-9);
    }
}
