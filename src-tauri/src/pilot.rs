//! Pilot-quality de-confounding: a champion's raw win rate can't tell whether
//! the champion is strong or its pilots are. This module estimates each
//! athlete's own baseline strength (independent of champion) from their
//! win rate and in-game rating across everything they've played, then checks
//! whether a champion wins more or less than its pilots' skill alone would
//! predict. The result is a small, confidence-gated delta (display-only, feeds
//! the tier list's automatic ranking) — it does not touch recommendation or
//! ban scoring.
//!
//! Last updated: 2026-07-13.

use rusqlite::Connection;
use std::collections::BTreeMap;

use crate::statistics::{patch_recency_weight_case, RatingBaseline};

// How much an athlete's own sample must grow before their estimated strength
// is trusted; below this it stays pulled toward neutral (0.5).
const PILOT_PRIOR_GAMES: f64 = 30.0;
// Equal blend of win-rate and rating evidence for pilot skill (user-confirmed):
// win rate alone is itself team-confounded, rating alone ignores who won.
const PILOT_WIN_RATE_WEIGHT: f64 = 0.5;
// Smoothing for the rating half of the blend, matching the champion-level
// rating_strength smoothing in recommendation/strength.rs.
const PILOT_RATING_PRIOR_GAMES: f64 = 10.0;
// How much weighted evidence a champion needs before its pilot-residual is
// trusted at face value; below this the delta shrinks toward zero.
const ADJUSTMENT_PRIOR_WEIGHT: f64 = 20.0;
// The largest win-rate nudge this correction can apply, win or lose.
const ADJUSTMENT_MAX_SWING: f64 = 0.08;

fn sigmoid(value: f64) -> f64 {
    1.0 / (1.0 + (-value).exp())
}

/// Same shrink-toward-neutral logic as recommendation::strength::risk_adjusted_win_rate,
/// duplicated here to keep this module decoupled from the recommendation engine.
fn shrink_toward_half(win_rate: f64, wins: usize, games: usize, prior_games: f64) -> f64 {
    let games = games as f64;
    if games <= 0.0 {
        return win_rate;
    }
    let wins = (wins as f64).clamp(0.0, games);
    let alpha = wins + prior_games * 0.5;
    let beta = (games - wins) + prior_games * 0.5;
    (alpha / (alpha + beta)).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug)]
struct AthleteStrength {
    strength: f64,
    confidence: f64,
}

/// Some schemas (older imports, minimal test fixtures) don't have `athlete_id`
/// on `picks`; pilot correction degrades to a no-op rather than erroring, the
/// same way `champion_ban_counts` tolerates a missing `bans` table.
fn has_athlete_id_column(connection: &Connection) -> bool {
    connection
        .prepare("SELECT athlete_id FROM picks LIMIT 0")
        .is_ok()
}

/// Per-role mean/std of raw pick ratings (not per-champion averaged), used only
/// to z-score an athlete's rating against the pool they actually played in.
fn pick_rating_baselines(connection: &Connection) -> Result<BTreeMap<String, RatingBaseline>, String> {
    let mut statement = connection
        .prepare("SELECT role, rating FROM picks WHERE rating IS NOT NULL")
        .map_err(|error| format!("Could not prepare pick ratings: {error}"))?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)))
        .map_err(|error| format!("Could not query pick ratings: {error}"))?;

    let mut by_role: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for result in rows {
        let (role, rating) = result.map_err(|error| format!("Could not read pick rating: {error}"))?;
        by_role.entry(role).or_default().push(rating);
    }
    Ok(by_role
        .into_iter()
        .map(|(role, ratings)| {
            let count = ratings.len() as f64;
            let mean = ratings.iter().sum::<f64>() / count;
            let variance = if ratings.len() > 1 {
                ratings.iter().map(|value| (value - mean).powi(2)).sum::<f64>() / count
            } else {
                0.0
            };
            (
                role,
                RatingBaseline {
                    mean,
                    std: variance.sqrt().max(3.0),
                },
            )
        })
        .collect())
}

/// Per-athlete baseline strength, estimated across every champion/role they've
/// played — the "how good is this pilot" number the residual pass compares
/// each game's actual result against.
pub struct PilotModel {
    athlete_strengths: BTreeMap<i64, AthleteStrength>,
}

impl PilotModel {
    pub fn build(connection: &Connection, current_patch: &str) -> Result<Self, String> {
        if !has_athlete_id_column(connection) {
            return Ok(Self {
                athlete_strengths: BTreeMap::new(),
            });
        }
        let rating_baselines = pick_rating_baselines(connection)?;
        let weight = patch_recency_weight_case(connection, current_patch, "m.patch")?;
        let sql = format!(
            "SELECT p.athlete_id, p.role,
                    COUNT(*) AS games,
                    SUM(CASE
                        WHEN (p.side = 'blue' AND m.blue_win = 1)
                          OR (p.side = 'red' AND m.blue_win = 0)
                        THEN 1 ELSE 0 END) AS wins,
                    SUM({weight}) AS effective_games,
                    AVG(p.rating) AS avg_rating
             FROM picks p
             JOIN matches m ON m.match_key = p.match_key
             WHERE p.athlete_id IS NOT NULL
             GROUP BY p.athlete_id, p.role"
        );
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| format!("Could not prepare pilot role stats: {error}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, usize>(2)?,
                    row.get::<_, usize>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, Option<f64>>(5)?,
                ))
            })
            .map_err(|error| format!("Could not query pilot role stats: {error}"))?;

        // Combine an athlete's per-role rows into one overall strength, weighted
        // by how much patch-recency-adjusted evidence each role contributes.
        let mut accum: BTreeMap<i64, (f64, f64, f64)> = BTreeMap::new(); // (weighted_strength, weight, total_games)
        for result in rows {
            let (athlete_id, role, games, wins, effective_games, avg_rating) =
                result.map_err(|error| format!("Could not read pilot role stats: {error}"))?;
            let win_rate = wins as f64 / games.max(1) as f64;
            let win_rate_component = shrink_toward_half(win_rate, wins, games, PILOT_PRIOR_GAMES);
            let rating_component = match (avg_rating, rating_baselines.get(&role)) {
                (Some(rating), Some(baseline)) => {
                    let raw = sigmoid((rating - baseline.mean) / baseline.std);
                    let confidence = games as f64 / (games as f64 + PILOT_RATING_PRIOR_GAMES);
                    0.5 + (raw - 0.5) * confidence
                }
                _ => 0.5,
            };
            let role_strength =
                PILOT_WIN_RATE_WEIGHT * win_rate_component + (1.0 - PILOT_WIN_RATE_WEIGHT) * rating_component;
            let entry = accum.entry(athlete_id).or_insert((0.0, 0.0, 0.0));
            entry.0 += effective_games * role_strength;
            entry.1 += effective_games;
            entry.2 += games as f64;
        }

        let athlete_strengths = accum
            .into_iter()
            .filter(|(_, (_, weight, _))| *weight > 0.0)
            .map(|(athlete_id, (weighted_strength, weight, total_games))| {
                (
                    athlete_id,
                    AthleteStrength {
                        strength: (weighted_strength / weight).clamp(0.0, 1.0),
                        confidence: total_games / (total_games + PILOT_PRIOR_GAMES),
                    },
                )
            })
            .collect();

        Ok(Self { athlete_strengths })
    }

    /// Confidence-gated win-rate delta per champion (or champion+role) explained
    /// by pilot quality rather than the champion itself: positive means the
    /// champion wins more than its pilots' skill predicts, negative means less.
    /// Keys match `query_rows`' grouping: `(champion_id, "all")` when
    /// `bucket_all` is set, else `(champion_id, role)`.
    pub fn win_rate_deltas(
        &self,
        connection: &Connection,
        current_patch: &str,
        bucket_all: bool,
    ) -> Result<BTreeMap<(String, String), f64>, String> {
        if self.athlete_strengths.is_empty() {
            return Ok(BTreeMap::new());
        }
        let weight = patch_recency_weight_case(connection, current_patch, "m.patch")?;
        let sql = format!(
            "SELECT p.champion_id, p.role, p.athlete_id,
                    (CASE
                        WHEN (p.side = 'blue' AND m.blue_win = 1)
                          OR (p.side = 'red' AND m.blue_win = 0)
                        THEN 1 ELSE 0 END) AS won,
                    {weight} AS w
             FROM picks p
             JOIN matches m ON m.match_key = p.match_key
             WHERE p.athlete_id IS NOT NULL"
        );
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| format!("Could not prepare pilot residual query: {error}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, f64>(4)?,
                ))
            })
            .map_err(|error| format!("Could not query pilot residuals: {error}"))?;

        let mut accum: BTreeMap<(String, String), (f64, f64)> = BTreeMap::new(); // (weighted_residual, weight)
        for result in rows {
            let (champion_id, role, athlete_id, won, patch_weight) =
                result.map_err(|error| format!("Could not read pilot residual row: {error}"))?;
            let Some(athlete) = self.athlete_strengths.get(&athlete_id) else {
                continue;
            };
            let row_weight = patch_weight * athlete.confidence;
            if row_weight <= 0.0 {
                continue;
            }
            let residual = won as f64 - athlete.strength;
            let key = (champion_id, if bucket_all { "all".to_string() } else { role });
            let entry = accum.entry(key).or_insert((0.0, 0.0));
            entry.0 += row_weight * residual;
            entry.1 += row_weight;
        }

        Ok(accum
            .into_iter()
            .map(|(key, (weighted_residual, weight))| {
                let raw_delta = weighted_residual / weight;
                let confidence = weight / (weight + ADJUSTMENT_PRIOR_WEIGHT);
                (key, (raw_delta * confidence).clamp(-ADJUSTMENT_MAX_SWING, ADJUSTMENT_MAX_SWING))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn seed_database(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE matches (
                    match_key TEXT PRIMARY KEY,
                    patch TEXT,
                    blue_win INTEGER NOT NULL
                );
                CREATE TABLE picks (
                    match_key TEXT NOT NULL,
                    side TEXT NOT NULL,
                    slot INTEGER NOT NULL,
                    athlete_id INTEGER,
                    champion_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    rating REAL,
                    PRIMARY KEY (match_key, side, slot)
                );",
            )
            .unwrap();
    }

    // Two champions have an identical 80% raw win rate over the same number of
    // games. "carry_magnet" is piloted solely by an athlete who *also* has a
    // deep, independently-strong record on an unrelated champion — real
    // evidence the pilot, not the champion, deserves credit. "solo_maining"
    // is piloted solely by an athlete with no games anywhere else, so there is
    // no independent evidence to separate pilot skill from champion strength.
    // The correction should credit "carry_magnet" less than "solo_maining",
    // and both should land below the naive, uncorrected excess (0.8 - 0.5).
    #[test]
    fn proven_pilot_discounts_more_than_unproven_pilot() {
        let connection = Connection::open_in_memory().unwrap();
        seed_database(&connection);
        let mut match_index = 0;
        let mut insert_match = |connection: &Connection, blue_win: i64| {
            let match_key = format!("m{match_index}");
            match_index += 1;
            connection
                .execute(
                    "INSERT INTO matches (match_key, patch, blue_win) VALUES (?1, '1.0', ?2)",
                    params![match_key, blue_win],
                )
                .unwrap();
            match_key
        };
        let insert_pick = |connection: &Connection,
                            match_key: &str,
                            side: &str,
                            athlete_id: i64,
                            champion_id: &str,
                            rating: f64| {
            connection
                .execute(
                    "INSERT INTO picks (match_key, side, slot, athlete_id, champion_id, role, rating)
                     VALUES (?1, ?2, 0, ?3, ?4, 'mid', ?5)",
                    params![match_key, side, athlete_id, champion_id, rating],
                )
                .unwrap();
        };

        // carry_magnet: athlete 1, 16 wins / 20 games (80%).
        for index in 0..20 {
            let win = if index < 16 { 1 } else { 0 };
            let match_key = insert_match(&connection, win);
            insert_pick(&connection, &match_key, "blue", 1, "carry_magnet", 70.0);
            insert_pick(&connection, &match_key, "red", 2, "opponent_a", 60.0);
        }
        // Athlete 1's independent history: 24/30 (80%) on an unrelated champion.
        for index in 0..30 {
            let win = if index < 24 { 1 } else { 0 };
            let match_key = insert_match(&connection, win);
            insert_pick(&connection, &match_key, "blue", 1, "other_champion", 70.0);
            insert_pick(&connection, &match_key, "red", 3, "filler_champion", 60.0);
        }
        // solo_maining: athlete 5, also 16/20 (80%), no other recorded games.
        for index in 0..20 {
            let win = if index < 16 { 1 } else { 0 };
            let match_key = insert_match(&connection, win);
            insert_pick(&connection, &match_key, "blue", 5, "solo_maining", 70.0);
            insert_pick(&connection, &match_key, "red", 2, "opponent_a", 60.0);
        }

        let model = PilotModel::build(&connection, "1.0").unwrap();
        let deltas = model.win_rate_deltas(&connection, "1.0", true).unwrap();

        let carry_magnet_delta = deltas
            .get(&("carry_magnet".to_string(), "all".to_string()))
            .copied()
            .unwrap_or(0.0);
        let solo_maining_delta = deltas
            .get(&("solo_maining".to_string(), "all".to_string()))
            .copied()
            .unwrap_or(0.0);

        assert!(
            carry_magnet_delta < solo_maining_delta,
            "proven pilot should be credited less: carry_magnet={carry_magnet_delta}, solo_maining={solo_maining_delta}"
        );
        // Both stay well under the naive, uncorrected excess (0.8 - 0.5 = 0.3).
        assert!(carry_magnet_delta < 0.3 && solo_maining_delta < 0.3);
        assert!(carry_magnet_delta > 0.0 && solo_maining_delta > 0.0);
    }

    #[test]
    fn champion_with_no_athlete_data_gets_zero_delta() {
        let connection = Connection::open_in_memory().unwrap();
        seed_database(&connection);
        connection
            .execute(
                "INSERT INTO matches (match_key, patch, blue_win) VALUES ('m1', '1.0', 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO picks (match_key, side, slot, athlete_id, champion_id, role, rating)
                 VALUES ('m1', 'blue', 0, NULL, 'mystery', 'mid', NULL)",
                [],
            )
            .unwrap();

        let model = PilotModel::build(&connection, "1.0").unwrap();
        let deltas = model.win_rate_deltas(&connection, "1.0", true).unwrap();
        assert!(deltas.get(&("mystery".to_string(), "all".to_string())).is_none());
    }
}
