use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

const PRIOR_GAMES: f64 = 20.0;
const RELIABLE_GAMES: f64 = 40.0;
// Universal patch-recency decay: every game is weighted by how many patches
// old it is, for ALL champions (not just patch-noted ones), so the recommender
// tracks meta shifts and not only direct buffs/nerfs. The current patch counts
// fully (1.0); each patch back is multiplied by RECENCY_DECAY; weights are
// floored at RECENCY_WEIGHT_FLOOR so deep history fades without vanishing.
// 0.6 ≈ current patch counts ~1.7x the previous, ~2.8x two back. Shared with
// the synergy / matchup queries (interactions.rs) via patch_recency_weight_case.
const RECENCY_DECAY: f64 = 0.6;
const RECENCY_WEIGHT_FLOOR: f64 = 0.05;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleStatistics {
    pub database_path: String,
    pub total_matches: usize,
    pub current_patch: String,
    pub global_win_rate: f64,
    pub prior_games: usize,
    pub reliable_games: usize,
    pub overall_rows: Vec<ChampionRoleStat>,
    pub role_rows: Vec<ChampionRoleStat>,
    /// Per-champion draft presence (0..1 percentile of how contested it is in
    /// drafts via picks + bans). Backend-only scoring input; not sent to the UI.
    #[serde(skip)]
    pub draft_presence: BTreeMap<String, f64>,
}

/// Per-role mean/spread of champion average ratings. The game's per-match
/// `rating` sits on a consistent ~47–92 scale across tournament and solo, but
/// role means differ (support runs lower than mid), so a champion's strength
/// must be judged against its own role's par, not an absolute number.
#[derive(Clone, Copy, Debug)]
pub struct RatingBaseline {
    pub mean: f64,
    pub std: f64,
}

impl RoleStatistics {
    /// Computes a rating baseline per role from champions with enough games, so
    /// a one-off outlier can't distort the par. The std is floored to avoid
    /// over-amplifying tiny spreads.
    pub fn rating_baselines(&self) -> BTreeMap<String, RatingBaseline> {
        let mut by_role: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
        for row in &self.role_rows {
            if row.games >= 10 {
                if let Some(rating) = row.avg_rating {
                    by_role.entry(row.role.as_str()).or_default().push(rating);
                }
            }
        }
        by_role
            .into_iter()
            .map(|(role, ratings)| {
                let count = ratings.len() as f64;
                let mean = ratings.iter().sum::<f64>() / count;
                let variance = if ratings.len() > 1 {
                    ratings
                        .iter()
                        .map(|value| (value - mean).powi(2))
                        .sum::<f64>()
                        / count
                } else {
                    0.0
                };
                (
                    role.to_string(),
                    RatingBaseline {
                        mean,
                        std: variance.sqrt().max(3.0),
                    },
                )
            })
            .collect()
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionRoleStat {
    pub champion_id: String,
    pub champion_name: String,
    pub role: String,
    pub portrait: Option<ChampionPortrait>,
    pub games: usize,
    pub current_patch_games: usize,
    pub effective_games: f64,
    pub patch_changed: bool,
    pub patch_added: bool,
    pub patch_impact: f64,
    pub patch_changes: Vec<crate::patch::PatchChange>,
    pub wins: usize,
    pub tournament_games: usize,
    pub solo_games: usize,
    pub win_rate: f64,
    pub adjusted_win_rate: f64,
    /// Confidence-gated win-rate delta explained by pilot quality rather than
    /// the champion itself (see `pilot.rs`): positive means the champion wins
    /// more than its pilots' independent skill predicts, negative means less.
    /// Display-only — feeds the tier list's automatic ranking, not scoring.
    pub pilot_win_rate_delta: f64,
    pub confidence: f64,
    pub avg_kills: Option<f64>,
    pub avg_deaths: Option<f64>,
    pub avg_assists: Option<f64>,
    pub kda: Option<f64>,
    pub avg_damage: Option<f64>,
    pub avg_tanking: Option<f64>,
    pub avg_healing: Option<f64>,
    pub avg_cs: Option<f64>,
    pub avg_gold: Option<f64>,
    pub avg_rating: Option<f64>,
    pub patch_timeline: Vec<ChampionPatchMetricPoint>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionPatchMetricPoint {
    pub patch: String,
    pub games: usize,
    pub wins: usize,
    pub win_rate: f64,
    pub avg_kills: Option<f64>,
    pub avg_deaths: Option<f64>,
    pub avg_assists: Option<f64>,
    pub kda: Option<f64>,
    pub avg_damage: Option<f64>,
    pub avg_tanking: Option<f64>,
    pub avg_healing: Option<f64>,
    pub avg_cs: Option<f64>,
    pub avg_gold: Option<f64>,
    pub avg_rating: Option<f64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionPortrait {
    pub path: String,
    pub sheet_width: usize,
    pub sheet_height: usize,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub face_offset_x: i32,
    pub face_offset_y: i32,
    pub center_offset_x: i32,
    pub center_offset_y: i32,
}

pub fn query_role_statistics(
    database_path: &Path,
    catalog_json: &str,
) -> Result<RoleStatistics, String> {
    if !database_path.is_file() {
        return Err("No imported database is available. Load a save first.".to_string());
    }

    let connection = Connection::open(database_path)
        .map_err(|error| format!("Could not open statistics database: {error}"))?;
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS patch_changed_champions (
                patch TEXT NOT NULL,
                champion_id TEXT NOT NULL,
                PRIMARY KEY (patch, champion_id)
            );
            CREATE TABLE IF NOT EXISTS champion_patch_changes (
                patch TEXT NOT NULL,
                champion_id TEXT NOT NULL,
                asset TEXT NOT NULL,
                target TEXT NOT NULL DEFAULT '',
                field TEXT NOT NULL,
                old_value REAL NOT NULL,
                new_value REAL NOT NULL,
                impact REAL NOT NULL,
                PRIMARY KEY (patch, champion_id, asset, target, field)
            );
            CREATE TABLE IF NOT EXISTS champion_patch_additions (
                patch TEXT NOT NULL,
                champion_id TEXT NOT NULL,
                PRIMARY KEY (patch, champion_id)
            );
            CREATE TABLE IF NOT EXISTS save_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                current_patch TEXT NOT NULL
            );",
        )
        .map_err(|error| format!("Could not prepare patch-change storage: {error}"))?;
    let total_matches: usize = connection
        .query_row("SELECT COUNT(*) FROM matches", [], |row| row.get(0))
        .map_err(|error| format!("Could not count imported matches: {error}"))?;
    if total_matches == 0 {
        return Err("The imported database contains no matches.".to_string());
    }

    let global_win_rate: f64 = connection
        .query_row(
            "SELECT AVG(CASE
                WHEN (p.side = 'blue' AND m.blue_win = 1)
                  OR (p.side = 'red' AND m.blue_win = 0)
                THEN 1.0 ELSE 0.0 END)
             FROM picks p
             JOIN matches m ON m.match_key = p.match_key",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("Could not calculate the global win rate: {error}"))?;
    let current_patch = current_patch(&connection)?;
    let patch_changes = current_patch_changes(&connection, &current_patch)?;

    let portraits = champion_portraits(catalog_json, database_path.parent())?;
    let pilot_model = crate::pilot::PilotModel::build(&connection, &current_patch)?;
    let pilot_deltas_all = pilot_model.win_rate_deltas(&connection, &current_patch, true)?;
    let pilot_deltas_role = pilot_model.win_rate_deltas(&connection, &current_patch, false)?;
    let overall_rows = query_rows(
        &connection,
        &portraits,
        global_win_rate,
        &current_patch,
        &patch_changes,
        "all",
        "GROUP BY p.champion_id",
        &pilot_deltas_all,
    )?;
    let role_rows = query_rows(
        &connection,
        &portraits,
        global_win_rate,
        &current_patch,
        &patch_changes,
        "role",
        "GROUP BY p.champion_id, p.role",
        &pilot_deltas_role,
    )?;

    let ban_counts = champion_ban_counts(&connection);
    let draft_presence = draft_presence_map(&overall_rows, &ban_counts);

    Ok(RoleStatistics {
        database_path: database_path.to_string_lossy().into_owned(),
        total_matches,
        current_patch,
        global_win_rate,
        prior_games: PRIOR_GAMES as usize,
        reliable_games: RELIABLE_GAMES as usize,
        overall_rows,
        role_rows,
        draft_presence,
    })
}

/// Ban counts per champion (tournament drafts only; solo has no bans). Resilient
/// to a missing `bans` table so synthetic test databases still work.
fn champion_ban_counts(connection: &Connection) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    if let Ok(mut statement) =
        connection.prepare("SELECT champion_id, COUNT(*) FROM bans GROUP BY champion_id")
    {
        if let Ok(rows) = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        }) {
            for row in rows.flatten() {
                counts.insert(row.0, row.1);
            }
        }
    }
    counts
}

/// Draft presence as a 0..1 percentile of tournament-weighted pick+ban activity,
/// so a frequently picked/banned champion ranks high regardless of win rate.
fn draft_presence_map(
    overall_rows: &[ChampionRoleStat],
    ban_counts: &BTreeMap<String, usize>,
) -> BTreeMap<String, f64> {
    let raws: Vec<(String, f64)> = overall_rows
        .iter()
        .map(|row| {
            let bans = ban_counts.get(&row.champion_id).copied().unwrap_or(0) as f64;
            // Bans are tournament-only, so they ride the tournament weight.
            let raw = 0.75 * (row.tournament_games as f64 + bans) + 0.25 * row.solo_games as f64;
            (row.champion_id.clone(), raw)
        })
        .collect();
    let mut sorted: Vec<f64> = raws.iter().map(|(_, raw)| *raw).collect();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let count = sorted.len();
    raws.into_iter()
        .map(|(id, raw)| {
            let below = sorted.iter().filter(|&&value| value < raw).count();
            let percentile = if count > 1 {
                below as f64 / (count - 1) as f64
            } else {
                0.5
            };
            (id, percentile.clamp(0.0, 1.0))
        })
        .collect()
}

fn query_rows(
    connection: &Connection,
    portraits: &BTreeMap<String, ChampionPortrait>,
    global_win_rate: f64,
    current_patch: &str,
    patch_changes: &BTreeMap<String, Vec<crate::patch::PatchChange>>,
    role_mode: &str,
    group_by: &str,
    pilot_deltas: &BTreeMap<(String, String), f64>,
) -> Result<Vec<ChampionRoleStat>, String> {
    let role_expression = if role_mode == "all" {
        "'all'"
    } else {
        "p.role"
    };
    let weight = patch_recency_weight_case(connection, current_patch, "m.patch")?;
    let sql = format!(
        "SELECT
                p.champion_id,
                {role_expression},
                COUNT(*) AS games,
                SUM(CASE WHEN m.patch = ?1 THEN 1 ELSE 0 END) AS current_patch_games,
                SUM({weight}) AS effective_games,
                SUM(CASE
                    WHEN (p.side = 'blue' AND m.blue_win = 1)
                      OR (p.side = 'red' AND m.blue_win = 0)
                    THEN {weight} ELSE 0.0 END) AS weighted_wins,
                MAX(CASE WHEN changed.champion_id IS NOT NULL THEN 1 ELSE 0 END) AS patch_changed,
                MAX(CASE WHEN added.champion_id IS NOT NULL THEN 1 ELSE 0 END) AS patch_added,
                SUM(CASE
                    WHEN (p.side = 'blue' AND m.blue_win = 1)
                      OR (p.side = 'red' AND m.blue_win = 0)
                    THEN 1 ELSE 0 END) AS wins,
                SUM(CASE WHEN m.source = 'tournament' THEN 1 ELSE 0 END),
                SUM(CASE WHEN m.source = 'solo' THEN 1 ELSE 0 END),
                AVG(p.kills),
                AVG(p.deaths),
                AVG(p.assists),
                AVG(p.damage),
                AVG(p.tanking),
                AVG(p.healing),
                AVG(p.cs),
                AVG(p.gold),
                AVG(p.rating)
             FROM picks p
             JOIN matches m ON m.match_key = p.match_key
             LEFT JOIN patch_changed_champions changed
               ON changed.patch = ?1 AND changed.champion_id = p.champion_id
             LEFT JOIN champion_patch_additions added
               ON added.patch = ?1 AND added.champion_id = p.champion_id
             {group_by}"
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("Could not prepare role statistics: {error}"))?;

    let mapped = statement
        .query_map([current_patch], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, usize>(2)?,
                row.get::<_, usize>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, usize>(6)?,
                row.get::<_, usize>(7)?,
                row.get::<_, usize>(8)?,
                row.get::<_, usize>(9)?,
                row.get::<_, usize>(10)?,
                row.get::<_, Option<f64>>(11)?,
                row.get::<_, Option<f64>>(12)?,
                row.get::<_, Option<f64>>(13)?,
                row.get::<_, Option<f64>>(14)?,
                row.get::<_, Option<f64>>(15)?,
                row.get::<_, Option<f64>>(16)?,
                row.get::<_, Option<f64>>(17)?,
                row.get::<_, Option<f64>>(18)?,
                row.get::<_, Option<f64>>(19)?,
            ))
        })
        .map_err(|error| format!("Could not query role statistics: {error}"))?;

    let mut rows = Vec::new();
    for result in mapped {
        let (
            champion_id,
            role,
            games,
            current_patch_games,
            effective_games,
            weighted_wins,
            patch_changed,
            patch_added,
            wins,
            tournament_games,
            solo_games,
            avg_kills,
            avg_deaths,
            avg_assists,
            avg_damage,
            avg_tanking,
            avg_healing,
            avg_cs,
            avg_gold,
            avg_rating,
        ) = result.map_err(|error| format!("Could not read role statistics: {error}"))?;
        let win_rate = wins as f64 / games as f64;
        let adjusted_win_rate =
            (weighted_wins + PRIOR_GAMES * global_win_rate) / (effective_games + PRIOR_GAMES);
        let kda = match (avg_kills, avg_deaths, avg_assists) {
            (Some(kills), Some(deaths), Some(assists)) => Some((kills + assists) / deaths.max(1.0)),
            _ => None,
        };
        let champion_patch_changes = patch_changes.get(&champion_id).cloned().unwrap_or_default();
        let patch_impact = crate::patch::weighted_patch_impact(&champion_patch_changes);
        let patch_timeline = query_patch_timeline(connection, &champion_id, &role, role_mode)?;
        let pilot_win_rate_delta = pilot_deltas
            .get(&(champion_id.clone(), role.clone()))
            .copied()
            .unwrap_or(0.0);

        rows.push(ChampionRoleStat {
            champion_name: humanize_id(&champion_id),
            portrait: portraits.get(&champion_id).cloned(),
            champion_id,
            role,
            games,
            current_patch_games,
            effective_games,
            patch_changed: patch_changed != 0,
            patch_added: patch_added != 0,
            patch_impact,
            patch_changes: champion_patch_changes,
            wins,
            tournament_games,
            solo_games,
            win_rate,
            adjusted_win_rate,
            pilot_win_rate_delta,
            confidence: (effective_games / RELIABLE_GAMES).min(1.0),
            avg_kills,
            avg_deaths,
            avg_assists,
            kda,
            avg_damage,
            avg_tanking,
            avg_healing,
            avg_cs,
            avg_gold,
            avg_rating,
            patch_timeline,
        });
    }

    rows.sort_by(|left, right| {
        right
            .adjusted_win_rate
            .total_cmp(&left.adjusted_win_rate)
            .then_with(|| right.games.cmp(&left.games))
            .then_with(|| left.champion_id.cmp(&right.champion_id))
    });

    Ok(rows)
}

fn query_patch_timeline(
    connection: &Connection,
    champion_id: &str,
    role: &str,
    role_mode: &str,
) -> Result<Vec<ChampionPatchMetricPoint>, String> {
    let role_filter = if role_mode == "all" {
        ""
    } else {
        "AND p.role = ?2"
    };
    let sql = format!(
        "SELECT
                m.patch,
                COUNT(*) AS games,
                SUM(CASE
                    WHEN (p.side = 'blue' AND m.blue_win = 1)
                      OR (p.side = 'red' AND m.blue_win = 0)
                    THEN 1 ELSE 0 END) AS wins,
                AVG(p.kills),
                AVG(p.deaths),
                AVG(p.assists),
                AVG(p.damage),
                AVG(p.tanking),
                AVG(p.healing),
                AVG(p.cs),
                AVG(p.gold),
                AVG(p.rating)
             FROM picks p
             JOIN matches m ON m.match_key = p.match_key
             WHERE p.champion_id = ?1
               AND m.patch IS NOT NULL
               {role_filter}
             GROUP BY m.patch"
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("Could not prepare champion patch timeline: {error}"))?;

    let mapped = if role_mode == "all" {
        statement.query_map(params![champion_id], patch_timeline_point)
    } else {
        statement.query_map(params![champion_id, role], patch_timeline_point)
    }
    .map_err(|error| format!("Could not query champion patch timeline: {error}"))?;

    let mut points = Vec::new();
    for row in mapped {
        points
            .push(row.map_err(|error| format!("Could not read champion patch timeline: {error}"))?);
    }
    points.sort_by(|left, right| {
        patch_key(&left.patch)
            .cmp(&patch_key(&right.patch))
            .then_with(|| left.patch.cmp(&right.patch))
    });
    Ok(points)
}

fn patch_timeline_point(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChampionPatchMetricPoint> {
    let patch = row.get::<_, String>(0)?;
    let games = row.get::<_, usize>(1)?;
    let wins = row.get::<_, usize>(2)?;
    let avg_kills = row.get::<_, Option<f64>>(3)?;
    let avg_deaths = row.get::<_, Option<f64>>(4)?;
    let avg_assists = row.get::<_, Option<f64>>(5)?;
    let avg_damage = row.get::<_, Option<f64>>(6)?;
    let avg_tanking = row.get::<_, Option<f64>>(7)?;
    let avg_healing = row.get::<_, Option<f64>>(8)?;
    let avg_cs = row.get::<_, Option<f64>>(9)?;
    let avg_gold = row.get::<_, Option<f64>>(10)?;
    let avg_rating = row.get::<_, Option<f64>>(11)?;
    let kda = match (avg_kills, avg_deaths, avg_assists) {
        (Some(kills), Some(deaths), Some(assists)) => Some((kills + assists) / deaths.max(1.0)),
        _ => None,
    };

    Ok(ChampionPatchMetricPoint {
        patch,
        games,
        wins,
        win_rate: wins as f64 / games.max(1) as f64,
        avg_kills,
        avg_deaths,
        avg_assists,
        kda,
        avg_damage,
        avg_tanking,
        avg_healing,
        avg_cs,
        avg_gold,
        avg_rating,
    })
}

fn current_patch_changes(
    connection: &Connection,
    current_patch: &str,
) -> Result<BTreeMap<String, Vec<crate::patch::PatchChange>>, String> {
    let mut statement = connection
        .prepare(
            "SELECT champion_id, asset, target, field, old_value, new_value, impact
             FROM champion_patch_changes
             WHERE patch = ?1
             ORDER BY champion_id, ABS(impact) DESC, asset, target",
        )
        .map_err(|error| format!("Could not prepare structured patch changes: {error}"))?;
    let rows = statement
        .query_map([current_patch], |row| {
            let target = row.get::<_, String>(2)?;
            Ok(crate::patch::PatchChange {
                patch: current_patch.to_string(),
                champion_id: row.get(0)?,
                asset: row.get(1)?,
                target: (!target.is_empty()).then_some(target),
                field: row.get(3)?,
                old_value: row.get(4)?,
                new_value: row.get(5)?,
                impact: row.get(6)?,
            })
        })
        .map_err(|error| format!("Could not query structured patch changes: {error}"))?;
    let mut changes = BTreeMap::<String, Vec<crate::patch::PatchChange>>::new();
    for row in rows {
        let change =
            row.map_err(|error| format!("Could not read structured patch change: {error}"))?;
        changes
            .entry(change.champion_id.clone())
            .or_default()
            .push(change);
    }
    Ok(changes)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchChampionChanges {
    pub champion_id: String,
    pub changes: Vec<crate::patch::PatchChange>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchHistoryEntry {
    pub patch: String,
    pub changes: Vec<PatchChampionChanges>,
    pub additions: Vec<String>,
}

/// Every patch with tracked balance changes or champion additions, newest first.
/// Read-only over the patch tables — lets the Patch Notes screen browse history,
/// not just the current patch. Win-rate deltas are NOT included here (those are a
/// live current-patch figure); historical patches are shown by their raw changes.
pub fn query_patch_history(database_path: &Path) -> Result<Vec<PatchHistoryEntry>, String> {
    if !database_path.is_file() {
        return Ok(Vec::new());
    }
    let connection = Connection::open(database_path)
        .map_err(|error| format!("Could not open database for patch history: {error}"))?;
    let mut patches = std::collections::BTreeSet::<String>::new();
    for sql in [
        "SELECT DISTINCT patch FROM champion_patch_changes",
        "SELECT DISTINCT patch FROM champion_patch_additions",
        "SELECT DISTINCT patch FROM patch_changed_champions",
    ] {
        let mut statement = connection
            .prepare(sql)
            .map_err(|error| format!("Could not prepare patch history query: {error}"))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| format!("Could not query patch list: {error}"))?;
        for row in rows {
            patches.insert(row.map_err(|error| format!("Could not read patch: {error}"))?);
        }
    }
    let mut sorted: Vec<String> = patches.into_iter().collect();
    sorted.sort_by(|left, right| patch_key(right).cmp(&patch_key(left))); // newest first
    let mut history = Vec::with_capacity(sorted.len());
    for patch in sorted {
        let changes = current_patch_changes(&connection, &patch)?
            .into_iter()
            .map(|(champion_id, changes)| PatchChampionChanges {
                champion_id,
                changes,
            })
            .collect();
        let mut statement = connection
            .prepare(
                "SELECT champion_id FROM champion_patch_additions
                 WHERE patch = ?1 ORDER BY champion_id",
            )
            .map_err(|error| format!("Could not prepare patch additions query: {error}"))?;
        let additions = statement
            .query_map([&patch], |row| row.get::<_, String>(0))
            .map_err(|error| format!("Could not query patch additions: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Could not read patch additions: {error}"))?;
        history.push(PatchHistoryEntry {
            patch,
            changes,
            additions,
        });
    }
    Ok(history)
}

pub(crate) fn current_patch(connection: &Connection) -> Result<String, String> {
    let save_patch = connection
        .query_row(
            "SELECT current_patch FROM save_state WHERE id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("Could not query the save's current patch: {error}"))?;
    if let Some(save_patch) = save_patch.filter(|patch| !patch.is_empty()) {
        return Ok(save_patch);
    }

    let mut statement = connection
        .prepare("SELECT DISTINCT patch FROM matches WHERE patch IS NOT NULL")
        .map_err(|error| format!("Could not prepare patch query: {error}"))?;
    let versions = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("Could not query patches: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not read patches: {error}"))?;
    versions
        .into_iter()
        .max_by_key(|version| patch_key(version))
        .ok_or_else(|| "The imported matches do not contain patch versions.".to_string())
}

fn patch_key(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or_default())
        .collect()
}

/// Build a SQL fragment mapping `patch_column` to a recency weight in (0, 1].
/// The newest patch at or before `current_patch` counts fully (1.0); each patch
/// further back is multiplied by RECENCY_DECAY, floored at RECENCY_WEIGHT_FLOOR
/// so ancient games fade without vanishing. Unknown / NULL patches fall to the
/// floor. Shared by the win-rate query and the synergy / matchup queries so
/// every signal tracks the meta the same way. Falls back to "1.0" (no
/// weighting) when no patched matches exist.
pub(crate) fn patch_recency_weight_case(
    connection: &Connection,
    current_patch: &str,
    patch_column: &str,
) -> Result<String, String> {
    let mut statement = connection
        .prepare("SELECT DISTINCT patch FROM matches WHERE patch IS NOT NULL")
        .map_err(|error| format!("Could not prepare patch list: {error}"))?;
    let patches = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("Could not query patches: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not read patches: {error}"))?;
    if patches.is_empty() {
        return Ok("1.0".to_string());
    }
    let current_key = patch_key(current_patch);
    let mut arms = String::new();
    for patch in &patches {
        let key = patch_key(patch);
        // Steps back = distinct game-patches strictly newer than this one but
        // no newer than the current patch. 0 for the current/newest patch.
        let distance = patches
            .iter()
            .filter(|other| {
                let other_key = patch_key(other);
                other_key > key && other_key <= current_key
            })
            .count();
        let weight = RECENCY_DECAY
            .powi(distance as i32)
            .max(RECENCY_WEIGHT_FLOOR);
        let escaped = patch.replace('\'', "''");
        arms.push_str(&format!(" WHEN '{escaped}' THEN {weight}"));
    }
    Ok(format!(
        "(CASE {patch_column}{arms} ELSE {RECENCY_WEIGHT_FLOOR} END)"
    ))
}

pub(crate) fn champion_portraits(
    catalog_json: &str,
    runtime_root: Option<&Path>,
) -> Result<BTreeMap<String, ChampionPortrait>, String> {
    crate::champion_registry::resolved_catalog_portraits(catalog_json, runtime_root)
}

pub(crate) fn humanize_id(id: &str) -> String {
    id.split('_')
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn aggregates_and_smooths_role_statistics() {
        let path = std::env::temp_dir().join(format!(
            "lt-ai-coach-statistics-{}.sqlite3",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE matches (
                    match_key TEXT PRIMARY KEY,
                    source TEXT NOT NULL,
                    patch TEXT NOT NULL,
                    blue_win INTEGER NOT NULL
                 );
                 CREATE TABLE patch_changed_champions (
                    patch TEXT NOT NULL,
                    champion_id TEXT NOT NULL
                 );
                 CREATE TABLE champion_patch_changes (
                    patch TEXT NOT NULL,
                    champion_id TEXT NOT NULL,
                    asset TEXT NOT NULL,
                    target TEXT NOT NULL,
                    field TEXT NOT NULL,
                    old_value REAL NOT NULL,
                    new_value REAL NOT NULL,
                    impact REAL NOT NULL
                 );
                 CREATE TABLE champion_patch_additions (
                    patch TEXT NOT NULL,
                    champion_id TEXT NOT NULL
                 );
                 CREATE TABLE picks (
                    match_key TEXT NOT NULL,
                    side TEXT NOT NULL,
                    champion_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    kills INTEGER,
                    deaths INTEGER,
                    assists INTEGER,
                    damage INTEGER,
                    tanking INTEGER,
                    healing INTEGER,
                    cs INTEGER,
                    gold INTEGER,
                    rating INTEGER
                 );",
            )
            .unwrap();
        for (key, patch, blue_win) in [("one", "2026.0.0", 1), ("two", "2025.9.0", 0)] {
            connection
                .execute(
                    "INSERT INTO matches VALUES (?1, 'tournament', ?2, ?3)",
                    params![key, patch, blue_win],
                )
                .unwrap();
        }
        for (key, side, champion) in [
            ("one", "blue", "swordsman"),
            ("one", "red", "archer"),
            ("two", "blue", "swordsman"),
            ("two", "red", "archer"),
        ] {
            connection
                .execute(
                    "INSERT INTO picks VALUES
                     (?1, ?2, ?3, 'jungle', 2, 1, 3, 100, 20, 0, 30, 50, 70)",
                    params![key, side, champion],
                )
                .unwrap();
        }
        connection
            .execute(
                "INSERT INTO patch_changed_champions VALUES ('2026.0.0', 'swordsman')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO champion_patch_changes VALUES
                 ('2026.0.0', 'swordsman', 'stat.attack', '', 'attack', 100, 110, 10)",
                [],
            )
            .unwrap();
        drop(connection);

        let result = query_role_statistics(
            &path,
            r#"{"champions":[{"id":"swordsman","name":"Swordsman"},{"id":"archer","name":"Archer"}]}"#,
        )
        .unwrap();

        assert_eq!(result.total_matches, 2);
        assert_eq!(result.current_patch, "2026.0.0");
        assert_eq!(result.overall_rows.len(), 2);
        assert_eq!(result.role_rows.len(), 2);
        let swordsman = result
            .overall_rows
            .iter()
            .find(|row| row.champion_id == "swordsman")
            .unwrap();
        assert_eq!(swordsman.games, 2);
        assert_eq!(swordsman.current_patch_games, 1);
        // Recency decay: current-patch game weighs 1.0, the 2025.9.0 game is one
        // patch back so weighs RECENCY_DECAY (0.6) -> 1.6 effective games.
        assert_eq!(swordsman.effective_games, 1.6);
        assert_eq!(swordsman.win_rate, 0.5);
        assert!(swordsman.patch_changed);
        assert_eq!(swordsman.patch_impact, 10.0);
        assert_eq!(swordsman.patch_changes.len(), 1);
        assert_eq!(swordsman.patch_changes[0].old_value, 100.0);
        assert_eq!(swordsman.kda, Some(5.0));
        assert_eq!(swordsman.role, "all");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn current_patch_prefers_save_state_over_latest_completed_match() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE matches (patch TEXT);
                 INSERT INTO matches VALUES ('2026.1.0');
                 CREATE TABLE save_state (
                    id INTEGER PRIMARY KEY,
                    current_patch TEXT NOT NULL
                 );
                 INSERT INTO save_state VALUES (1, '2026.1.1');",
            )
            .unwrap();

        assert_eq!(current_patch(&connection).unwrap(), "2026.1.1");
    }

    #[test]
    fn patch_recency_weight_decays_by_distance() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE matches (match_key TEXT, patch TEXT);
                 INSERT INTO matches VALUES
                    ('a', '2026.2.0'), ('b', '2026.1.0'), ('c', '2026.0.0');",
            )
            .unwrap();

        let case = patch_recency_weight_case(&connection, "2026.2.0", "patch").unwrap();
        let weight_for = |patch: &str| -> f64 {
            connection
                .query_row(
                    &format!("SELECT {case} FROM matches WHERE patch = ?1"),
                    [patch],
                    |row| row.get::<_, f64>(0),
                )
                .unwrap()
        };

        // Current patch full weight; each step back multiplied by the decay.
        assert!((weight_for("2026.2.0") - 1.0).abs() < 1e-9);
        assert!((weight_for("2026.1.0") - RECENCY_DECAY).abs() < 1e-9);
        assert!((weight_for("2026.0.0") - RECENCY_DECAY.powi(2)).abs() < 1e-9);
    }

    #[test]
    #[ignore = "prints the live rating/stat distributions to design v2 normalization"]
    fn audits_rating_and_stat_scales() {
        let path = std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap()
            .join("com.lttools.lt-ai-coach")
            .join("lt-ai-coach.sqlite3");
        let connection = Connection::open(&path).expect("open live db");

        eprintln!("== rating by source x role (n, min, avg, max) ==");
        let mut statement = connection
            .prepare(
                "SELECT m.source, p.role, COUNT(p.rating), MIN(p.rating),
                        ROUND(AVG(p.rating), 1), MAX(p.rating)
                 FROM picks p JOIN matches m ON m.match_key = p.match_key
                 WHERE p.rating IS NOT NULL
                 GROUP BY m.source, p.role
                 ORDER BY m.source, p.role",
            )
            .unwrap();
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<f64>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                ))
            })
            .unwrap();
        for row in rows {
            let (source, role, n, min, avg, max) = row.unwrap();
            eprintln!(
                "{source:<11} {role:<8} n={n:<6} min={:<6?} avg={:<7?} max={:?}",
                min, avg, max
            );
        }

        eprintln!("== damage / tanking / healing by role (avg, max) ==");
        let mut statement = connection
            .prepare(
                "SELECT role,
                        ROUND(AVG(damage),0), MAX(damage),
                        ROUND(AVG(tanking),0), MAX(tanking),
                        ROUND(AVG(healing),0), MAX(healing)
                 FROM picks GROUP BY role ORDER BY role",
            )
            .unwrap();
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<f64>>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                ))
            })
            .unwrap();
        for row in rows {
            let (role, dmg_avg, dmg_max, tank_avg, tank_max, heal_avg, heal_max) = row.unwrap();
            eprintln!(
                "{role:<8} dmg(avg={:?} max={:?})  tank(avg={:?} max={:?})  heal(avg={:?} max={:?})",
                dmg_avg, dmg_max, tank_avg, tank_max, heal_avg, heal_max
            );
        }
    }

    #[test]
    #[ignore = "audits the current application database"]
    fn audits_current_application_database() {
        let path = std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap()
            .join("com.lttools.lt-ai-coach")
            .join("lt-ai-coach.sqlite3");
        let result =
            query_role_statistics(&path, crate::CHAMPION_CATALOG).expect("database should query");

        assert_eq!(
            result.role_rows.iter().map(|row| row.games).sum::<usize>(),
            result.total_matches * 10
        );
        assert_eq!(
            result
                .overall_rows
                .iter()
                .map(|row| row.games)
                .sum::<usize>(),
            result.total_matches * 10
        );
        assert!(result.role_rows.iter().all(|row| row.wins <= row.games));
        assert!(result.role_rows.iter().all(|row| {
            row.tournament_games + row.solo_games == row.games
                && (0.0..=1.0).contains(&row.win_rate)
                && (0.0..=1.0).contains(&row.adjusted_win_rate)
        }));
        let changed = result
            .overall_rows
            .iter()
            .filter(|row| !row.patch_changes.is_empty())
            .collect::<Vec<_>>();
        assert!(!changed.is_empty());
        eprintln!(
            "current patch {} has {} champions with {} exact changes",
            result.current_patch,
            changed.len(),
            changed
                .iter()
                .map(|row| row.patch_changes.len())
                .sum::<usize>()
        );
        for row in &changed {
            eprintln!(
                "{}: {:+.2} semantic impact across {} changes",
                row.champion_id,
                row.patch_impact,
                row.patch_changes.len()
            );
        }

        let mut by_sample = result.role_rows.iter().collect::<Vec<_>>();
        by_sample.sort_by(|left, right| right.games.cmp(&left.games));
        for row in by_sample.into_iter().take(10) {
            eprintln!(
                "{} ({}) {}: {} games, {:.1}% raw, {:.1}% adjusted, T{} / S{}",
                row.champion_name,
                row.champion_id,
                row.role,
                row.games,
                row.win_rate * 100.0,
                row.adjusted_win_rate * 100.0,
                row.tournament_games,
                row.solo_games
            );
        }
    }
}
