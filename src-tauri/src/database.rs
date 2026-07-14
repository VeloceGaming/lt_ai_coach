use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub database_path: String,
    pub enabled_champions: usize,
    pub players: usize,
    pub athletes_with_stats: usize,
    pub mastery_entries: usize,
    pub teams: usize,
    pub matches: usize,
    pub tournament_matches: usize,
    pub solo_matches: usize,
    pub picks: usize,
    pub bans: usize,
    pub patch_changes: usize,
    pub patch_additions: usize,
    /// The player's team name from the export manifest — identifies which game
    /// (career) this data is from, so the user can confirm the right save.
    pub game_label: Option<String>,
    /// Stable database ID for the team controlled in this save. The live draft
    /// bridge compares this with its blue/red team IDs to identify the user.
    pub player_team_id: Option<i64>,
    /// When the mod wrote the export (unix seconds), so the coach can flag stale
    /// data if the game wasn't running / the save wasn't reloaded.
    pub exported_at_unix: Option<i64>,
}

#[derive(Clone)]
struct MatchRow {
    key: String,
    source: &'static str,
    source_id: i64,
    patch: Option<String>,
    played_at: Option<String>,
    region_id: Option<i64>,
    blue_team_id: Option<i64>,
    red_team_id: Option<i64>,
    blue_win: bool,
    picks: Vec<PickRow>,
    blue_bans: Vec<String>,
    red_bans: Vec<String>,
}

#[derive(Clone)]
struct PickRow {
    side: &'static str,
    slot: usize,
    athlete_id: Option<i64>,
    champion_id: String,
    role: String,
    kills: Option<i64>,
    deaths: Option<i64>,
    assists: Option<i64>,
    damage: Option<i64>,
    tanking: Option<i64>,
    healing: Option<i64>,
    cs: Option<i64>,
    gold: Option<i64>,
    rating: Option<i64>,
}

// ---------------------------------------------------------------------------
// Exporter import (new path): read the lt_ai_coach_exporter mod's output instead
// of running the borrowed probe. The mod reads the live game Database through the
// official SDK and writes clean JSON/TSV, so this is update-proof and probe-free.
// The JSON field names below are the game_core struct field names (the same ones
// the probe debug output used); serde ignores any fields we do not declare.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ExporterTournament {
    id: i64,
    #[serde(default)]
    blue_team_id: Option<i64>,
    #[serde(default)]
    red_team_id: Option<i64>,
    #[serde(default)]
    blue_team_win: bool,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    blue_ban: Vec<String>,
    #[serde(default)]
    red_ban: Vec<String>,
    #[serde(default)]
    blue_team: Vec<ExporterTournamentAthlete>,
    #[serde(default)]
    red_team: Vec<ExporterTournamentAthlete>,
    #[serde(default)]
    blue_performance: ExporterPerformance,
    #[serde(default)]
    red_performance: ExporterPerformance,
}

#[derive(Deserialize)]
struct ExporterTournamentAthlete {
    #[serde(default)]
    athlete_id: Option<i64>,
    #[serde(default)]
    position: String,
    #[serde(default)]
    champion: String,
    #[serde(default)]
    id: i64,
    #[serde(default)]
    statistics: ExporterStatistics,
}

#[derive(Default, Deserialize)]
struct ExporterStatistics {
    #[serde(default)]
    dealing: Option<i64>,
    #[serde(default)]
    assists: Option<i64>,
    #[serde(default)]
    tanking: Option<i64>,
    #[serde(default)]
    healing: Option<i64>,
    #[serde(default)]
    rating: Option<i64>,
    #[serde(default)]
    gold: Option<i64>,
}

#[derive(Default, Deserialize)]
struct ExporterPerformance {
    #[serde(default)]
    gold_line_phase: Vec<i64>,
    #[serde(default)]
    cs_line_phase: Vec<i64>,
    #[serde(default)]
    kills: Vec<i64>,
    #[serde(default)]
    deaths: Vec<i64>,
    #[serde(default)]
    deal: Vec<i64>,
}

#[derive(Deserialize)]
struct ExporterSolo {
    id: i64,
    #[serde(default)]
    region_id: Option<i64>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    blue_team_win: bool,
    #[serde(default)]
    result_time: Option<String>,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    blue_team: Vec<ExporterSoloAthlete>,
    #[serde(default)]
    red_team: Vec<ExporterSoloAthlete>,
}

#[derive(Deserialize)]
struct ExporterSoloAthlete {
    #[serde(default)]
    athlete_id: Option<i64>,
    #[serde(default)]
    champion: String,
    #[serde(default)]
    kill: Option<i64>,
    #[serde(default)]
    death: Option<i64>,
    #[serde(default)]
    assist: Option<i64>,
    #[serde(default)]
    cs: Option<i64>,
    #[serde(default)]
    dealing: Option<i64>,
    #[serde(default)]
    healing: Option<i64>,
    #[serde(default)]
    tanking: Option<i64>,
    #[serde(default)]
    rating: Option<i64>,
    #[serde(default)]
    gold: Option<i64>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExporterAthleteCollection {
    #[serde(default)]
    schema_version: usize,
    #[serde(default)]
    athletes: Vec<ExporterAthlete>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExporterAthlete {
    id: i64,
    name: String,
    #[serde(default)]
    contract: serde_json::Value,
    #[serde(default)]
    stats: Option<ExporterAthleteStats>,
    #[serde(default)]
    recent_champions: Vec<String>,
    #[serde(default)]
    champion_proficiency: BTreeMap<String, ExporterChampionProficiency>,
}

#[derive(Deserialize)]
struct ExporterAthleteStats {
    last_hit: i64,
    skill_avoid: i64,
    skill_hit: i64,
    positioning: i64,
    control_speed: i64,
    concentration: i64,
    mental: i64,
    judgement: i64,
    order: i64,
    roaming: i64,
    aggressive: i64,
    ego: i64,
    top: i64,
    jungle: i64,
    mid: i64,
    bottom: i64,
    support: i64,
}

#[derive(Deserialize)]
struct ExporterChampionProficiency {
    floor: i64,
    value: i64,
}

/// Import from the exporter mod's output directory: champions.tsv / teams.tsv /
/// players.tsv, the match JSON files, and pre_patch_data.json (patch-change
/// detection). This is the sole import path now that the borrowed probe is gone.
pub fn import_exporter_output(
    database_path: PathBuf,
    exporter_dir: &Path,
    game_time: Option<&str>,
) -> Result<ImportSummary, String> {
    // The manifest identifies the game and export time (best-effort).
    let manifest = parse_manifest(&read_required(exporter_dir, "manifest.tsv")?);
    let game_label = manifest
        .get("player_team")
        .filter(|value| !value.is_empty())
        .cloned();
    let player_team_id = manifest
        .get("player_team_id")
        .and_then(|value| value.parse::<i64>().ok());
    let exported_at_unix = manifest
        .get("exported_at_unix")
        .and_then(|value| value.parse::<i64>().ok());

    let enabled_champions: Vec<String> = read_required(exporter_dir, "champions.tsv")?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    let players = parse_id_name_tsv(&read_required(exporter_dir, "players.tsv")?);
    let teams = parse_id_name_tsv(&read_required(exporter_dir, "teams.tsv")?);
    let athlete_export = read_athlete_export(exporter_dir)?;
    let tournament_matches =
        parse_tournament_json(&read_required(exporter_dir, "match_replays.json")?);
    let solo_matches = parse_solo_json(&read_required(exporter_dir, "solo_rank_matches.json")?);
    // Prefer the exporter's date-resolved balance history. Older exporter
    // builds only provide pre_patch_data.json, which is often incomplete.
    let patches = fs::read_to_string(exporter_dir.join("champion_balance_history.json"))
        .ok()
        .and_then(|text| parse_balance_history_json(&text))
        .or_else(|| {
            fs::read_to_string(exporter_dir.join("pre_patch_data.json"))
                .ok()
                .map(|text| parse_patches_json(&text))
        })
        .unwrap_or_default();
    // The current patch is the newest version seen anywhere — patch snapshots or
    // matches — so patch-recency weighting anchors on the latest patch even if a
    // brand-new patch has no pre_patch_data snapshot yet.
    let mut current_patch = patches.current_patch.clone();
    for played in tournament_matches.iter().chain(solo_matches.iter()) {
        if let Some(version) = &played.patch {
            if current_patch
                .as_ref()
                .map_or(true, |current| patch_key(version) > patch_key(current))
            {
                current_patch = Some(version.clone());
            }
        }
    }

    if enabled_champions.is_empty() {
        return Err("No enabled champions were found in the exporter output.".to_string());
    }
    if tournament_matches.is_empty() && solo_matches.is_empty() {
        return Err("No match records were found in the exporter output.".to_string());
    }

    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create database directory: {error}"))?;
    }

    let mut connection = Connection::open(&database_path)
        .map_err(|error| format!("Could not open SQLite database: {error}"))?;
    connection
        .execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            PRAGMA synchronous = NORMAL;
            ",
        )
        .map_err(|error| format!("Could not configure SQLite database: {error}"))?;
    create_schema(&connection)?;

    let transaction = connection
        .transaction()
        .map_err(|error| format!("Could not begin database import: {error}"))?;
    clear_current_data(&transaction)?;

    transaction
        .execute(
            "INSERT INTO import_metadata
             (id, schema_version, save_path, game_time, probe_output_path, player_team_id, imported_at)
             VALUES (1, 8, ?1, ?2, ?3, ?4, datetime('now'))",
            params![
                exporter_dir.to_string_lossy(),
                game_time,
                exporter_dir.to_string_lossy(),
                player_team_id,
            ],
        )
        .map_err(|error| format!("Could not write import metadata: {error}"))?;

    if let Some(current_patch) = &current_patch {
        transaction
            .execute(
                "INSERT INTO save_state (id, current_patch) VALUES (1, ?1)",
                [current_patch],
            )
            .map_err(|error| format!("Could not write current save patch: {error}"))?;
    }
    for (patch, champion_ids) in &patches.changed {
        for champion_id in champion_ids {
            transaction
                .execute(
                    "INSERT INTO patch_changed_champions (patch, champion_id)
                     VALUES (?1, ?2)",
                    params![patch, champion_id],
                )
                .map_err(|error| format!("Could not write patch change: {error}"))?;
        }
    }
    for (patch, champion_id) in &patches.additions {
        transaction
            .execute(
                "INSERT OR IGNORE INTO champion_patch_additions (patch, champion_id)
                 VALUES (?1, ?2)",
                params![patch, champion_id],
            )
            .map_err(|error| format!("Could not write champion addition: {error}"))?;
    }
    for change in &patches.field_changes {
        transaction
            .execute(
                "INSERT OR REPLACE INTO champion_patch_changes
                 (patch, champion_id, asset, target, field, old_value, new_value, impact)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    change.patch,
                    change.champion_id,
                    change.asset,
                    change.target,
                    change.field,
                    change.old_value,
                    change.new_value,
                    change.impact,
                ],
            )
            .map_err(|error| format!("Could not write champion field change: {error}"))?;
    }

    for champion_id in &enabled_champions {
        transaction
            .execute(
                "INSERT INTO enabled_champions (champion_id) VALUES (?1)",
                [champion_id],
            )
            .map_err(|error| format!("Could not write enabled champion: {error}"))?;
    }
    let athlete_team_ids = athlete_export
        .as_ref()
        .map(|export| {
            export
                .athletes
                .iter()
                .filter_map(|athlete| {
                    contract_team_id(&athlete.contract).map(|team_id| (athlete.id, team_id))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    for (id, name) in &players {
        transaction
            .execute(
                "INSERT INTO players (id, name, team_id) VALUES (?1, ?2, ?3)",
                params![id, name, athlete_team_ids.get(id)],
            )
            .map_err(|error| format!("Could not write player: {error}"))?;
    }
    for (id, name) in &teams {
        transaction
            .execute(
                "INSERT INTO teams (id, name) VALUES (?1, ?2)",
                params![id, name],
            )
            .map_err(|error| format!("Could not write team: {error}"))?;
    }

    let (athletes_with_stats, mastery_entries) = if let Some(export) = &athlete_export {
        insert_athlete_data(&transaction, export)?
    } else {
        (0, 0)
    };

    let mut all_matches = tournament_matches.clone();
    all_matches.extend(solo_matches.clone());
    let mut pick_count = 0;
    let mut ban_count = 0;
    for match_row in &all_matches {
        insert_match(&transaction, match_row)?;
        pick_count += match_row.picks.len();
        ban_count += match_row.blue_bans.len() + match_row.red_bans.len();
    }

    transaction
        .commit()
        .map_err(|error| format!("Could not commit database import: {error}"))?;

    Ok(ImportSummary {
        database_path: database_path.to_string_lossy().into_owned(),
        enabled_champions: enabled_champions.len(),
        players: players.len(),
        athletes_with_stats,
        mastery_entries,
        teams: teams.len(),
        matches: all_matches.len(),
        tournament_matches: tournament_matches.len(),
        solo_matches: solo_matches.len(),
        picks: pick_count,
        bans: ban_count,
        patch_changes: patches.changed.values().map(BTreeSet::len).sum(),
        patch_additions: patches.additions.len(),
        game_label,
        player_team_id,
        exported_at_unix,
    })
}

/// One per-patch snapshot from pre_patch_data.json. We only need the champion
/// balance sheet and the enabled list; serde ignores game_setting etc.
#[derive(Deserialize)]
struct ExporterPatchState {
    #[serde(default)]
    champion_info_sheet: BTreeMap<String, serde_json::Value>,
}

#[derive(Default)]
struct PatchSummary {
    current_patch: Option<String>,
    /// patch version -> champions whose balance changed entering that patch.
    changed: BTreeMap<String, BTreeSet<String>>,
    /// (patch version, champion) for champions newly added that patch.
    additions: Vec<(String, String)>,
    field_changes: Vec<crate::patch::PatchFieldChange>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExporterBalanceHistory {
    current_version: String,
    #[serde(default)]
    snapshots: BTreeMap<String, ExporterBalanceSnapshot>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExporterBalanceSnapshot {
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    champion_info_sheet: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    available_champions: Option<BTreeSet<String>>,
}

/// Derive the patch information from pre_patch_data.json: the current patch, the
/// set of champions that changed each patch (by diffing consecutive champion
/// balance sheets), and champions newly added each patch. Mirrors what the probe
/// path produced, without the field-level structured notes (a later refinement).
fn parse_patches_json(text: &str) -> PatchSummary {
    let states: BTreeMap<String, ExporterPatchState> =
        serde_json::from_str(text).unwrap_or_default();
    let mut versions: Vec<String> = states.keys().cloned().collect();
    versions.sort_by(|left, right| patch_key(left).cmp(&patch_key(right)));

    let current_patch = versions.last().cloned();
    let mut changed = BTreeMap::new();
    let mut additions = Vec::new();
    for pair in versions.windows(2) {
        let (previous, current) = (&pair[0], &pair[1]);
        let prev_sheet = flatten_champion_sheet(&states[previous].champion_info_sheet);
        let cur_sheet = flatten_champion_sheet(&states[current].champion_info_sheet);
        let mut changed_ids = BTreeSet::new();
        for (champion_id, cur_value) in &cur_sheet {
            match prev_sheet.get(champion_id) {
                None => additions.push((current.clone(), champion_id.clone())),
                Some(prev_value) if prev_value != cur_value => {
                    changed_ids.insert(champion_id.clone());
                }
                Some(_) => {}
            }
        }
        if !changed_ids.is_empty() {
            changed.insert(current.clone(), changed_ids);
        }
    }
    PatchSummary {
        current_patch,
        changed,
        additions,
        field_changes: Vec::new(),
    }
}

/// Flatten a snapshot's champion sheet into champion_id -> balance value, pulling
/// modded champions out of the nested `mod_champions` array (each entry keyed by
/// its `id`) so they get diffed for buffs/nerfs/additions exactly like base
/// champions. Without this, the whole `mod_champions` group was skipped and mods
/// never showed patch changes.
fn flatten_champion_sheet(
    sheet: &BTreeMap<String, serde_json::Value>,
) -> BTreeMap<String, &serde_json::Value> {
    let mut flat = BTreeMap::new();
    for (key, value) in sheet {
        if key == "mod_champions" {
            if let serde_json::Value::Array(mods) = value {
                for entry in mods {
                    if let Some(id) = entry.get("id").and_then(|id| id.as_str()) {
                        flat.insert(id.to_string(), entry);
                    }
                }
            }
        } else {
            flat.insert(key.clone(), value);
        }
    }
    flat
}

fn parse_balance_history_json(text: &str) -> Option<PatchSummary> {
    let history: ExporterBalanceHistory = serde_json::from_str(text).ok()?;
    if history.snapshots.is_empty() {
        return None;
    }
    let mut versions: Vec<String> = history.snapshots.keys().cloned().collect();
    // A ledger created by an older exporter may contain snapshots from another
    // campaign. The imported save is authoritative: future versions can never
    // belong to its patch history.
    let current_key = patch_key(&history.current_version);
    versions.retain(|version| patch_key(version) <= current_key);
    versions.sort_by(|left, right| patch_key(left).cmp(&patch_key(right)));

    let mut summary = PatchSummary {
        current_patch: Some(history.current_version),
        ..PatchSummary::default()
    };
    let has_roster_snapshots = history
        .snapshots
        .values()
        .any(|snapshot| snapshot.available_champions.is_some());
    let mut last_known_roster: Option<&BTreeSet<String>> = None;
    for version in &versions {
        if let Some(roster) = history.snapshots[version].available_champions.as_ref() {
            if let Some(previous) = last_known_roster {
                summary.additions.extend(
                    roster
                        .difference(previous)
                        .map(|champion_id| (version.clone(), champion_id.clone())),
                );
            }
            last_known_roster = Some(roster);
        }
    }
    for pair in versions.windows(2) {
        let (previous, current) = (&pair[0], &pair[1]);
        // A historical sheet reconstructed by the game has proven capable of
        // returning a later patch's values. Never allow such a diff to affect
        // patch notes or tiering. Exact save/captured/live
        // snapshots remain eligible; source-less legacy fixtures stay valid.
        if history.snapshots[previous].source.as_deref() == Some("historical-untrusted")
            || history.snapshots[current].source.as_deref() == Some("historical-untrusted")
        {
            continue;
        }
        let previous_sheet =
            flatten_champion_sheet(&history.snapshots[previous].champion_info_sheet);
        let current_sheet = flatten_champion_sheet(&history.snapshots[current].champion_info_sheet);
        for (champion_id, current_value) in &current_sheet {
            let Some(previous_value) = previous_sheet.get(champion_id) else {
                if !has_roster_snapshots {
                    summary
                        .additions
                        .push((current.clone(), champion_id.clone()));
                }
                continue;
            };
            if previous_value == current_value {
                continue;
            }
            summary
                .changed
                .entry(current.clone())
                .or_default()
                .insert(champion_id.clone());
            crate::patch::collect_field_changes(
                current,
                champion_id,
                previous_value,
                current_value,
                &mut summary.field_changes,
            );
        }
    }
    Some(summary)
}

/// Parse the `key<TAB>value` manifest into a lookup map.
fn parse_manifest(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('\t')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

/// Parse an `id<TAB>name` TSV (with a header row) into id -> name.
fn parse_id_name_tsv(text: &str) -> BTreeMap<i64, String> {
    text.lines()
        .skip(1) // header
        .filter_map(|line| {
            let (id, name) = line.split_once('\t')?;
            Some((id.trim().parse().ok()?, name.trim().to_string()))
        })
        .collect()
}

/// Map a game position enum value to the coach's role names.
fn role_from_position(position: &str) -> Option<&'static str> {
    match position {
        "Top" => Some("top"),
        "Jungle" => Some("jungle"),
        "Mid" => Some("mid"),
        "Bottom" => Some("bot"),
        "Support" => Some("support"),
        _ => None,
    }
}

/// Build the tournament `MatchRow`s from match_replays.json (a map id -> match).
fn parse_tournament_json(text: &str) -> Vec<MatchRow> {
    let matches = parse_exporter_collection::<ExporterTournament>(text);
    matches
        .into_values()
        .filter_map(|m| {
            let blue_picks = tournament_picks(&m.blue_team, "blue", &m.blue_performance);
            let red_picks = tournament_picks(&m.red_team, "red", &m.red_performance);
            if blue_picks.len() != 5 || red_picks.len() != 5 {
                return None;
            }
            let mut picks = blue_picks;
            picks.extend(red_picks);
            Some(MatchRow {
                key: format!("tournament-{}", m.id),
                source: "tournament",
                source_id: m.id,
                patch: m.version,
                played_at: None,
                region_id: None,
                blue_team_id: m.blue_team_id,
                red_team_id: m.red_team_id,
                blue_win: m.blue_team_win,
                picks,
                blue_bans: m.blue_ban,
                red_bans: m.red_ban,
            })
        })
        .collect()
}

fn tournament_picks(
    team: &[ExporterTournamentAthlete],
    side: &'static str,
    performance: &ExporterPerformance,
) -> Vec<PickRow> {
    team.iter()
        .filter_map(|athlete| {
            let slot = (athlete.id % 5) as usize;
            let role = role_from_position(&athlete.position)?;
            if athlete.champion.is_empty() {
                return None;
            }
            Some(PickRow {
                side,
                slot,
                athlete_id: athlete.athlete_id,
                champion_id: athlete.champion.clone(),
                role: role.to_string(),
                kills: performance.kills.get(slot).copied(),
                deaths: performance.deaths.get(slot).copied(),
                assists: athlete.statistics.assists,
                damage: athlete
                    .statistics
                    .dealing
                    .or_else(|| performance.deal.get(slot).copied()),
                tanking: athlete.statistics.tanking,
                healing: athlete.statistics.healing,
                cs: performance.cs_line_phase.get(slot).copied(),
                // `gold_line_phase` is an early-game team-performance vector.
                // The per-athlete statistics record stores the full match gold
                // earned, which is the number the stats tab wants.
                gold: athlete
                    .statistics
                    .gold
                    .or_else(|| performance.gold_line_phase.get(slot).copied()),
                rating: athlete.statistics.rating,
            })
        })
        .collect()
}

/// Build the solo `MatchRow`s from solo_rank_matches.json (a map id -> match).
fn parse_solo_json(text: &str) -> Vec<MatchRow> {
    const ROLES: [&str; 5] = ["top", "jungle", "mid", "bot", "support"];
    let matches = parse_exporter_collection::<ExporterSolo>(text);
    matches
        .into_values()
        .filter_map(|m| {
            // The `played` flag is absent in some exporter versions (serde defaults it
            // to false). Use valid picks from both teams as the completion signal instead.
            let build = |team: &[ExporterSoloAthlete], side: &'static str| -> Vec<PickRow> {
                team.iter()
                    .enumerate()
                    .filter_map(|(slot, athlete)| {
                        let role = ROLES.get(slot)?;
                        if athlete.champion.is_empty() {
                            return None;
                        }
                        Some(PickRow {
                            side,
                            slot,
                            athlete_id: athlete.athlete_id,
                            champion_id: athlete.champion.clone(),
                            role: (*role).to_string(),
                            kills: athlete.kill,
                            deaths: athlete.death,
                            assists: athlete.assist,
                            damage: athlete.dealing,
                            tanking: athlete.tanking,
                            healing: athlete.healing,
                            cs: athlete.cs,
                            gold: athlete.gold,
                            rating: athlete.rating,
                        })
                    })
                    .collect()
            };
            let blue_picks = build(&m.blue_team, "blue");
            let red_picks = build(&m.red_team, "red");
            if blue_picks.len() != 5 || red_picks.len() != 5 {
                return None;
            }
            let mut picks = blue_picks;
            picks.extend(red_picks);
            Some(MatchRow {
                key: format!("solo-{}", m.id),
                source: "solo",
                source_id: m.id,
                patch: m.version,
                played_at: m.result_time.or(m.date),
                region_id: m.region_id,
                blue_team_id: None,
                red_team_id: None,
                blue_win: m.blue_team_win,
                picks,
                blue_bans: Vec::new(),
                red_bans: Vec::new(),
            })
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ExporterCollection<T> {
    Map(BTreeMap<String, T>),
    List(Vec<T>),
}

fn parse_exporter_collection<T>(text: &str) -> BTreeMap<String, T>
where
    T: for<'de> Deserialize<'de>,
{
    match serde_json::from_str::<ExporterCollection<T>>(text) {
        Ok(ExporterCollection::Map(matches)) => matches,
        Ok(ExporterCollection::List(matches)) => matches
            .into_iter()
            .enumerate()
            .map(|(index, row)| (index.to_string(), row))
            .collect(),
        Err(_) => BTreeMap::new(),
    }
}

fn create_schema(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS import_metadata (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                schema_version INTEGER NOT NULL,
                save_path TEXT NOT NULL,
                game_time TEXT,
                probe_output_path TEXT NOT NULL,
                player_team_id INTEGER,
                imported_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS enabled_champions (
                champion_id TEXT PRIMARY KEY
            );
            CREATE TABLE IF NOT EXISTS patch_changed_champions (
                patch TEXT NOT NULL,
                champion_id TEXT NOT NULL,
                PRIMARY KEY (patch, champion_id)
            );
            -- Derived data: rebuild this table on import so older databases
            -- receive the field-aware primary key without a schema migrator.
            DROP TABLE IF EXISTS champion_patch_changes;
            CREATE TABLE champion_patch_changes (
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
            );
            -- User's manual tier overrides. Deliberately NOT cleared on import
            -- (see clear_current_data) so flags persist across re-imports.
            CREATE TABLE IF NOT EXISTS manual_tiers (
                champion_id TEXT PRIMARY KEY,
                tier TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS players (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                team_id INTEGER
            );
            CREATE TABLE IF NOT EXISTS athlete_stats (
                athlete_id INTEGER PRIMARY KEY,
                last_hit INTEGER NOT NULL,
                skill_avoid INTEGER NOT NULL,
                skill_hit INTEGER NOT NULL,
                positioning INTEGER NOT NULL,
                control_speed INTEGER NOT NULL,
                concentration INTEGER NOT NULL,
                mental INTEGER NOT NULL,
                judgement INTEGER NOT NULL,
                shotcalling INTEGER NOT NULL,
                roaming INTEGER NOT NULL,
                aggressive INTEGER NOT NULL,
                ego INTEGER NOT NULL,
                top INTEGER NOT NULL,
                jungle INTEGER NOT NULL,
                mid INTEGER NOT NULL,
                bottom INTEGER NOT NULL,
                support INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS athlete_mastery (
                athlete_id INTEGER NOT NULL,
                champion_id TEXT NOT NULL,
                floor_raw INTEGER NOT NULL,
                value_raw INTEGER NOT NULL,
                is_recent INTEGER NOT NULL CHECK (is_recent IN (0, 1)),
                PRIMARY KEY (athlete_id, champion_id)
            );
            CREATE INDEX IF NOT EXISTS athlete_mastery_champion_value
                ON athlete_mastery(champion_id, value_raw DESC);
            CREATE TABLE IF NOT EXISTS teams (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS matches (
                match_key TEXT PRIMARY KEY,
                source TEXT NOT NULL CHECK (source IN ('tournament', 'solo')),
                source_id INTEGER NOT NULL,
                patch TEXT,
                played_at TEXT,
                region_id INTEGER,
                blue_team_id INTEGER,
                red_team_id INTEGER,
                blue_win INTEGER NOT NULL CHECK (blue_win IN (0, 1))
            );
            CREATE TABLE IF NOT EXISTS bans (
                match_key TEXT NOT NULL REFERENCES matches(match_key) ON DELETE CASCADE,
                side TEXT NOT NULL CHECK (side IN ('blue', 'red')),
                ban_order INTEGER NOT NULL,
                champion_id TEXT NOT NULL,
                PRIMARY KEY (match_key, side, ban_order)
            );
            CREATE TABLE IF NOT EXISTS picks (
                match_key TEXT NOT NULL REFERENCES matches(match_key) ON DELETE CASCADE,
                side TEXT NOT NULL CHECK (side IN ('blue', 'red')),
                slot INTEGER NOT NULL,
                athlete_id INTEGER,
                champion_id TEXT NOT NULL,
                role TEXT NOT NULL CHECK (role IN ('top', 'jungle', 'mid', 'bot', 'support')),
                kills INTEGER,
                deaths INTEGER,
                assists INTEGER,
                damage INTEGER,
                tanking INTEGER,
                healing INTEGER,
                cs INTEGER,
                gold INTEGER,
                rating INTEGER,
                PRIMARY KEY (match_key, side, slot)
            );
            CREATE INDEX IF NOT EXISTS picks_champion_role
                ON picks(champion_id, role);
            CREATE INDEX IF NOT EXISTS picks_athlete_champion_role
                ON picks(athlete_id, champion_id, role);
            CREATE INDEX IF NOT EXISTS matches_source_patch
                ON matches(source, patch);
            ",
        )
        .map_err(|error| format!("Could not create SQLite schema: {error}"))?;
    ensure_column(connection, "players", "team_id", "INTEGER")?;
    ensure_column(connection, "import_metadata", "player_team_id", "INTEGER")?;
    Ok(())
}

pub fn query_player_team_id(database_path: &Path) -> Result<Option<usize>, String> {
    if !database_path.is_file() {
        return Ok(None);
    }
    let connection = Connection::open(database_path)
        .map_err(|error| format!("Could not open imported database: {error}"))?;
    connection
        .query_row(
            "SELECT player_team_id FROM import_metadata WHERE id = 1",
            [],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()
        .map_err(|error| format!("Could not read imported player team: {error}"))?
        .flatten()
        .map(|id| {
            usize::try_from(id).map_err(|_| "Imported player team ID is invalid.".to_string())
        })
        .transpose()
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| format!("Could not inspect {table} schema: {error}"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| format!("Could not query {table} schema: {error}"))?;
    for existing in columns {
        if existing.map_err(|error| format!("Could not read {table} schema: {error}"))? == column {
            return Ok(());
        }
    }
    connection
        .execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )
        .map(|_| ())
        .map_err(|error| format!("Could not add {table}.{column}: {error}"))
}

fn clear_current_data(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            "
            DELETE FROM bans;
            DELETE FROM picks;
            DELETE FROM matches;
            DELETE FROM enabled_champions;
            DELETE FROM patch_changed_champions;
            DELETE FROM champion_patch_changes;
            DELETE FROM champion_patch_additions;
            DELETE FROM save_state;
            DELETE FROM athlete_mastery;
            DELETE FROM athlete_stats;
            DELETE FROM players;
            DELETE FROM teams;
            DELETE FROM import_metadata;
            ",
        )
        .map_err(|error| format!("Could not clear prior database import: {error}"))
}

fn insert_match(transaction: &Transaction<'_>, row: &MatchRow) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO matches
             (match_key, source, source_id, patch, played_at, region_id,
              blue_team_id, red_team_id, blue_win)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                row.key,
                row.source,
                row.source_id,
                row.patch,
                row.played_at,
                row.region_id,
                row.blue_team_id,
                row.red_team_id,
                row.blue_win as i64
            ],
        )
        .map_err(|error| format!("Could not write match {}: {error}", row.key))?;

    for (side, bans) in [("blue", &row.blue_bans), ("red", &row.red_bans)] {
        for (order, champion_id) in bans.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO bans (match_key, side, ban_order, champion_id)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![row.key, side, order as i64, champion_id],
                )
                .map_err(|error| format!("Could not write ban for {}: {error}", row.key))?;
        }
    }

    for pick in &row.picks {
        transaction
            .execute(
                "INSERT INTO picks
                 (match_key, side, slot, athlete_id, champion_id, role, kills,
                  deaths, assists, damage, tanking, healing, cs, gold, rating)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                         ?13, ?14, ?15)",
                params![
                    row.key,
                    pick.side,
                    pick.slot as i64,
                    pick.athlete_id,
                    pick.champion_id,
                    pick.role,
                    pick.kills,
                    pick.deaths,
                    pick.assists,
                    pick.damage,
                    pick.tanking,
                    pick.healing,
                    pick.cs,
                    pick.gold,
                    pick.rating
                ],
            )
            .map_err(|error| format!("Could not write pick for {}: {error}", row.key))?;
    }
    Ok(())
}

fn patch_key(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or_default())
        .collect()
}

fn read_required(root: &Path, name: &str) -> Result<String, String> {
    fs::read_to_string(root.join(name))
        .map_err(|error| format!("Could not read {name} from exporter output: {error}"))
}

fn read_athlete_export(root: &Path) -> Result<Option<ExporterAthleteCollection>, String> {
    let path = root.join("athlete_mastery.json");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Could not read athlete_mastery.json from exporter output: {error}"
            ))
        }
    };
    let export = serde_json::from_str::<ExporterAthleteCollection>(&text)
        .map_err(|error| format!("Could not parse athlete_mastery.json: {error}"))?;
    if export.schema_version == 0 {
        return Err("athlete_mastery.json is missing a valid schemaVersion.".to_string());
    }
    Ok(Some(export))
}

fn contract_team_id(contract: &serde_json::Value) -> Option<i64> {
    contract
        .get("InContract")
        .or_else(|| contract.get("in_contract"))
        .and_then(|details| details.get("team_id").or_else(|| details.get("teamId")))
        .and_then(serde_json::Value::as_i64)
}

fn insert_athlete_data(
    transaction: &Transaction<'_>,
    export: &ExporterAthleteCollection,
) -> Result<(usize, usize), String> {
    let mut player_statement = transaction
        .prepare(
            "INSERT INTO players (id, name, team_id) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 team_id = excluded.team_id",
        )
        .map_err(|error| format!("Could not prepare athlete identity import: {error}"))?;
    let mut stats_statement = transaction
        .prepare(
            "INSERT INTO athlete_stats
             (athlete_id, last_hit, skill_avoid, skill_hit, positioning,
              control_speed, concentration, mental, judgement, shotcalling,
              roaming, aggressive, ego, top, jungle, mid, bottom, support)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                     ?13, ?14, ?15, ?16, ?17, ?18)",
        )
        .map_err(|error| format!("Could not prepare athlete stat import: {error}"))?;
    let mut mastery_statement = transaction
        .prepare(
            "INSERT INTO athlete_mastery
             (athlete_id, champion_id, floor_raw, value_raw, is_recent)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .map_err(|error| format!("Could not prepare athlete mastery import: {error}"))?;

    let mut athletes_with_stats = 0;
    let mut mastery_entries = 0;
    for athlete in &export.athletes {
        player_statement
            .execute(params![
                athlete.id,
                athlete.name,
                contract_team_id(&athlete.contract)
            ])
            .map_err(|error| format!("Could not write athlete {}: {error}", athlete.id))?;

        if let Some(stats) = &athlete.stats {
            stats_statement
                .execute(params![
                    athlete.id,
                    stats.last_hit,
                    stats.skill_avoid,
                    stats.skill_hit,
                    stats.positioning,
                    stats.control_speed,
                    stats.concentration,
                    stats.mental,
                    stats.judgement,
                    stats.order,
                    stats.roaming,
                    stats.aggressive,
                    stats.ego,
                    stats.top,
                    stats.jungle,
                    stats.mid,
                    stats.bottom,
                    stats.support,
                ])
                .map_err(|error| {
                    format!("Could not write stats for athlete {}: {error}", athlete.id)
                })?;
            athletes_with_stats += 1;
        }

        let recent = athlete
            .recent_champions
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        for (champion_id, proficiency) in &athlete.champion_proficiency {
            mastery_statement
                .execute(params![
                    athlete.id,
                    champion_id,
                    proficiency.floor,
                    proficiency.value,
                    recent.contains(champion_id.as_str()) as i64,
                ])
                .map_err(|error| {
                    format!(
                        "Could not write {champion_id} mastery for athlete {}: {error}",
                        athlete.id
                    )
                })?;
            mastery_entries += 1;
        }
    }
    Ok((athletes_with_stats, mastery_entries))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_history_produces_exact_nested_field_changes() {
        let history = serde_json::json!({
            "currentVersion": "2027.0.1",
            "snapshots": {
                "2027.0.0": {
                    "championInfoSheet": {
                        "fighter": {
                            "stat": { "hp": 1000 },
                            "attack": { "cooltime": 80 },
                            "skill": { "attack_ratio": 70, "duration": 20, "start_timing": 10 },
                            "ult": { "charge_time": 60 }
                        }
                    }
                },
                "2027.0.1": {
                    "championInfoSheet": {
                        "fighter": {
                            "stat": { "hp": 900 },
                            "attack": { "cooltime": 72 },
                            "skill": { "attack_ratio": 75, "duration": 24, "start_timing": 8 },
                            "ult": { "charge_time": 48 }
                        }
                    }
                }
            }
        });
        let summary = parse_balance_history_json(&history.to_string()).unwrap();

        assert_eq!(summary.current_patch.as_deref(), Some("2027.0.1"));
        assert!(summary.changed["2027.0.1"].contains("fighter"));
        assert_eq!(summary.field_changes.len(), 4);
        assert!(!summary
            .field_changes
            .iter()
            .any(|change| matches!(change.field.as_str(), "duration" | "start_timing")));
        let hp = summary
            .field_changes
            .iter()
            .find(|change| change.field == "hp")
            .unwrap();
        assert_eq!(hp.asset, "stat");
        assert_eq!(hp.old_value, 1000.0);
        assert_eq!(hp.new_value, 900.0);
        assert_eq!(hp.impact, -10.0);
        let cooldown = summary
            .field_changes
            .iter()
            .find(|change| change.field == "cooltime")
            .unwrap();
        assert_eq!(cooldown.impact, 10.0);
        let charge_time = summary
            .field_changes
            .iter()
            .find(|change| change.field == "charge_time")
            .unwrap();
        assert_eq!(charge_time.impact, 20.0);
    }

    #[test]
    fn balance_history_marks_new_champions_without_fake_field_diffs() {
        let history = serde_json::json!({
            "currentVersion": "2.0.0",
            "snapshots": {
                "1.0.0": { "championInfoSheet": { "fighter": { "stat": { "hp": 10 } } } },
                "2.0.0": { "championInfoSheet": {
                    "fighter": { "stat": { "hp": 10 } },
                    "newcomer": { "stat": { "hp": 20 } }
                } }
            }
        });
        let summary = parse_balance_history_json(&history.to_string()).unwrap();

        assert_eq!(
            summary.additions,
            vec![("2.0.0".to_string(), "newcomer".to_string())]
        );
        assert!(summary.field_changes.is_empty());
    }

    #[test]
    fn balance_history_uses_authoritative_rosters_across_unknown_intermediate_patch() {
        let history = serde_json::json!({
            "currentVersion": "2026.1.0",
            "snapshots": {
                "2026.0.0": {
                    "availableChampions": ["fighter"],
                    "championInfoSheet": { "fighter": {}, "newcomer": {} }
                },
                "2026.0.1": {
                    "availableChampions": null,
                    "championInfoSheet": { "fighter": {}, "newcomer": {} }
                },
                "2026.1.0": {
                    "availableChampions": ["fighter", "newcomer"],
                    "championInfoSheet": { "fighter": {}, "newcomer": {} }
                }
            }
        });
        let summary = parse_balance_history_json(&history.to_string()).unwrap();

        assert_eq!(
            summary.additions,
            vec![("2026.1.0".to_string(), "newcomer".to_string())]
        );
    }

    #[test]
    fn balance_history_diffs_modded_champions_nested_in_mod_champions() {
        // Modded champions live inside a `mod_champions` array (keyed by `id`),
        // not as top-level keys. Their buffs/nerfs and additions must still register.
        let history = serde_json::json!({
            "currentVersion": "2.0.0",
            "snapshots": {
                "1.0.0": { "championInfoSheet": {
                    "fighter": { "stat": { "hp": 1000 } },
                    "mod_champions": [
                        { "id": "test_mod_jhin", "stat": { "hp": 900 } }
                    ]
                } },
                "2.0.0": { "championInfoSheet": {
                    "fighter": { "stat": { "hp": 1000 } },
                    "mod_champions": [
                        { "id": "test_mod_jhin", "stat": { "hp": 980 } },
                        { "id": "test_mod_gragas", "stat": { "hp": 1100 } }
                    ]
                } }
            }
        });
        let summary = parse_balance_history_json(&history.to_string()).unwrap();

        // The nerfed mod champion is recorded as changed, with a real field diff.
        assert!(summary.changed["2.0.0"].contains("test_mod_jhin"));
        let hp = summary
            .field_changes
            .iter()
            .find(|change| change.champion_id == "test_mod_jhin" && change.field == "hp")
            .expect("modded champion hp change should be captured");
        assert_eq!(hp.old_value, 900.0);
        assert_eq!(hp.new_value, 980.0);
        // The newly added mod champion is recorded as an addition, not a fake diff.
        assert!(summary
            .additions
            .contains(&("2.0.0".to_string(), "test_mod_gragas".to_string())));
        // The `mod_champions` container key itself is never treated as a champion.
        assert!(!summary.changed["2.0.0"].contains("mod_champions"));
    }

    #[test]
    fn balance_history_rejects_untrusted_historical_diffs() {
        let history = serde_json::json!({
            "currentVersion": "2.0.0",
            "snapshots": {
                "1.0.0": {
                    "source": "historical-untrusted",
                    "championInfoSheet": { "fighter": { "stat": { "hp": 10 } } }
                },
                "2.0.0": {
                    "source": "current",
                    "championInfoSheet": { "fighter": { "stat": { "hp": 20 } } }
                }
            }
        });
        let summary = parse_balance_history_json(&history.to_string()).unwrap();

        assert!(summary.changed.is_empty());
        assert!(summary.field_changes.is_empty());
    }

    #[test]
    fn balance_history_rejects_snapshots_newer_than_the_save() {
        let history = serde_json::json!({
            "currentVersion": "2026.0.1",
            "snapshots": {
                "2026.0.0": { "championInfoSheet": { "fighter": { "stat": { "hp": 10 } } } },
                "2026.0.1": { "championInfoSheet": { "fighter": { "stat": { "hp": 11 } } } },
                "2027.0.0": { "championInfoSheet": { "fighter": { "stat": { "hp": 99 } } } }
            }
        });
        let summary = parse_balance_history_json(&history.to_string()).unwrap();

        assert_eq!(summary.current_patch.as_deref(), Some("2026.0.1"));
        assert!(summary.changed.contains_key("2026.0.1"));
        assert!(!summary.changed.contains_key("2027.0.0"));
        assert!(summary
            .field_changes
            .iter()
            .all(|change| change.patch != "2027.0.0"));
    }

    #[test]
    fn solo_rank_matches_parse_from_array_exports() {
        let export = serde_json::json!([
            {
                "id": 42,
                "region_id": 3,
                "version": "2026.1.0",
                "blue_team_win": true,
                "result_time": "2026-06-23 20:00:00",
                "blue_team": [
                    {"athlete_id": 1, "champion": "blue_top", "kill": 1, "death": 2, "assist": 3, "cs": 100, "dealing": 1000, "healing": 10, "tanking": 100, "rating": 70},
                    {"athlete_id": 2, "champion": "blue_jungle"},
                    {"athlete_id": 3, "champion": "blue_mid"},
                    {"athlete_id": 4, "champion": "blue_bot"},
                    {"athlete_id": 5, "champion": "blue_support"}
                ],
                "red_team": [
                    {"athlete_id": 6, "champion": "red_top"},
                    {"athlete_id": 7, "champion": "red_jungle"},
                    {"athlete_id": 8, "champion": "red_mid"},
                    {"athlete_id": 9, "champion": "red_bot"},
                    {"athlete_id": 10, "champion": "red_support"}
                ]
            }
        ]);

        let matches = parse_solo_json(&export.to_string());

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].key, "solo-42");
        assert_eq!(matches[0].source, "solo");
        assert_eq!(matches[0].patch.as_deref(), Some("2026.1.0"));
        assert_eq!(matches[0].picks.len(), 10);
        assert_eq!(matches[0].picks[0].role, "top");
        assert_eq!(matches[0].picks[4].role, "support");
        assert_eq!(matches[0].picks[5].side, "red");
    }

    #[test]
    fn athlete_export_imports_stats_team_and_mastery() {
        let json = serde_json::json!({
            "schemaVersion": 2,
            "athletes": [{
                "id": 7,
                "name": "Test Mid",
                "contract": {"InContract": {"team_id": 42}},
                "stats": {
                    "last_hit": 80, "skill_avoid": 70, "skill_hit": 90,
                    "positioning": 75, "control_speed": 85, "concentration": 65,
                    "mental": 60, "judgement": 88, "order": 55, "roaming": 45,
                    "aggressive": 50, "ego": 40, "top": 10, "jungle": 20,
                    "mid": 95, "bottom": 30, "support": 15
                },
                "recentChampions": ["wind_mage"],
                "championProficiency": {
                    "wind_mage": {"floor": 400, "value": 780}
                }
            }]
        });
        let export: ExporterAthleteCollection = serde_json::from_value(json).unwrap();
        assert_eq!(export.schema_version, 2);

        let mut connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        let transaction = connection.transaction().unwrap();
        let counts = insert_athlete_data(&transaction, &export).unwrap();
        transaction.commit().unwrap();

        assert_eq!(counts, (1, 1));
        assert_eq!(
            connection
                .query_row("SELECT team_id FROM players WHERE id = 7", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            42
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT last_hit, shotcalling, mid FROM athlete_stats WHERE athlete_id = 7",
                    [],
                    |row| Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                )
                .unwrap(),
            (80, 55, 95)
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT value_raw, is_recent FROM athlete_mastery
                     WHERE athlete_id = 7 AND champion_id = 'wind_mage'",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                )
                .unwrap(),
            (780, 1)
        );
    }

    #[test]
    fn schema_one_athlete_export_remains_valid_without_stats() {
        let export: ExporterAthleteCollection = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "athletes": [{
                "id": 9,
                "name": "Legacy",
                "contract": null,
                "recentChampions": [],
                "championProficiency": {
                    "fighter": {"floor": 200, "value": 650}
                }
            }]
        }))
        .unwrap();

        assert!(export.athletes[0].stats.is_none());
        assert_eq!(export.athletes[0].champion_proficiency.len(), 1);
    }

    #[test]
    #[ignore = "audits the current local exporter snapshot"]
    fn audits_live_athlete_export_import() {
        let exporter_dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap()
            .join("com.lttools.lt-ai-coach")
            .join("exporter");
        let database_path = std::env::temp_dir().join(format!(
            "lt-ai-coach-athletes-{}.sqlite3",
            std::process::id()
        ));
        let _ = fs::remove_file(&database_path);

        let summary = import_exporter_output(database_path.clone(), &exporter_dir, None).unwrap();
        assert!(summary.athletes_with_stats > 0);
        assert!(summary.mastery_entries > summary.athletes_with_stats);

        let connection = Connection::open(&database_path).unwrap();
        let stats_count = connection
            .query_row("SELECT COUNT(*) FROM athlete_stats", [], |row| {
                row.get::<_, usize>(0)
            })
            .unwrap();
        let mastery_count = connection
            .query_row("SELECT COUNT(*) FROM athlete_mastery", [], |row| {
                row.get::<_, usize>(0)
            })
            .unwrap();
        assert_eq!(stats_count, summary.athletes_with_stats);
        assert_eq!(mastery_count, summary.mastery_entries);

        let athletes = crate::athletes::query_athletes(&database_path).unwrap();
        assert_eq!(athletes.len(), summary.players);
        let detail = crate::athletes::query_athlete_detail(&database_path, athletes[0].id)
            .unwrap()
            .unwrap();
        assert!(detail.stats.is_some());
        let mastery = detail.masteries.first().unwrap();
        let lookup =
            crate::athletes::query_mastery(&database_path, detail.id, &mastery.champion_id)
                .unwrap()
                .unwrap();
        assert_eq!(lookup.mastery, mastery.mastery);
        assert_eq!(lookup.stat_buff, mastery.stat_buff);
        eprintln!("athletes_with_stats={stats_count} mastery_entries={mastery_count}");

        drop(connection);
        fs::remove_file(database_path).unwrap();
    }
}
