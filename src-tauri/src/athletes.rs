use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AthleteSummary {
    pub id: i64,
    pub name: String,
    pub team_id: Option<i64>,
    pub team_name: Option<String>,
    pub strongest_role: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AthleteDetail {
    pub id: i64,
    pub name: String,
    pub team_id: Option<i64>,
    pub team_name: Option<String>,
    pub strongest_role: Option<String>,
    pub stats: Option<AthleteStats>,
    pub masteries: Vec<AthleteMastery>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AthleteStats {
    pub core: CoreStats,
    pub tendencies: TendencyStats,
    pub roles: RoleRatings,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreStats {
    pub last_hit: i64,
    pub skill_avoid: i64,
    pub skill_hit: i64,
    pub positioning: i64,
    pub control_speed: i64,
    pub concentration: i64,
    pub mental: i64,
    pub judgement: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveCoreStats {
    pub last_hit: f64,
    pub skill_avoid: f64,
    pub skill_hit: f64,
    pub positioning: f64,
    pub control_speed: f64,
    pub concentration: f64,
    pub mental: f64,
    pub judgement: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TendencyStats {
    pub shotcalling: i64,
    pub roaming: i64,
    pub aggressive: i64,
    pub ego: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleRatings {
    pub top: i64,
    pub jungle: i64,
    pub mid: i64,
    pub bottom: i64,
    pub support: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AthleteMastery {
    pub champion_id: String,
    pub floor_raw: i64,
    pub value_raw: i64,
    pub mastery: f64,
    pub stat_buff: f64,
    pub recent: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AthleteChampionLookup {
    pub athlete_id: i64,
    pub champion_id: String,
    pub mastery: f64,
    pub stat_buff: f64,
    pub realized_stat_buff: f64,
    pub recent: bool,
    pub base_core: CoreStats,
    pub effective_core: EffectiveCoreStats,
    pub realized_gain: EffectiveCoreStats,
    pub base_core_average: f64,
    pub effective_core_average: f64,
    pub realized_gain_average: f64,
    pub capped_stats: usize,
}

#[derive(Clone, Default)]
pub struct AthleteIndex {
    stats: BTreeMap<i64, CoreStats>,
    masteries: BTreeMap<(i64, String), AthleteMastery>,
}

impl AthleteIndex {
    pub fn mastery_for(&self, athlete_id: i64, champion_id: &str) -> Option<AthleteChampionLookup> {
        let base_core = self.stats.get(&athlete_id)?.clone();
        let mastery = self.masteries.get(&(athlete_id, champion_id.to_string()))?;
        Some(build_lookup(
            athlete_id,
            champion_id,
            base_core,
            mastery.stat_buff,
            mastery.mastery,
            mastery.recent,
        ))
    }

    #[cfg(test)]
    pub(crate) fn with_test_entry(
        athlete_id: i64,
        champion_id: &str,
        base_core: CoreStats,
        value_raw: i64,
    ) -> Self {
        let mastery = AthleteMastery {
            champion_id: champion_id.to_string(),
            floor_raw: 0,
            value_raw,
            mastery: value_raw as f64 / 10.0,
            stat_buff: mastery_buff(value_raw),
            recent: false,
        };
        Self {
            stats: BTreeMap::from([(athlete_id, base_core)]),
            masteries: BTreeMap::from([((athlete_id, champion_id.to_string()), mastery)]),
        }
    }
}

pub fn mastery_buff(value_raw: i64) -> f64 {
    match value_raw {
        1000.. => 0.20,
        900..=999 => 0.15,
        800..=899 => 0.10,
        700..=799 => 0.05,
        _ => 0.0,
    }
}

pub fn query_athletes(database_path: &Path) -> Result<Vec<AthleteSummary>, String> {
    let connection = open_database(database_path)?;
    let mut statement = connection
        .prepare(
            "SELECT p.id, p.name, p.team_id, t.name,
                    s.top, s.jungle, s.mid, s.bottom, s.support
             FROM players p
             LEFT JOIN teams t ON t.id = p.team_id
             LEFT JOIN athlete_stats s ON s.athlete_id = p.id
             ORDER BY p.name COLLATE NOCASE, p.id",
        )
        .map_err(|error| format!("Could not prepare athlete list: {error}"))?;
    let rows = statement
        .query_map([], summary_from_row)
        .map_err(|error| format!("Could not query athletes: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not read athlete list: {error}"))
}

pub fn query_athlete_detail(
    database_path: &Path,
    athlete_id: i64,
) -> Result<Option<AthleteDetail>, String> {
    let connection = open_database(database_path)?;
    let summary = connection
        .query_row(
            "SELECT p.id, p.name, p.team_id, t.name,
                    s.top, s.jungle, s.mid, s.bottom, s.support
             FROM players p
             LEFT JOIN teams t ON t.id = p.team_id
             LEFT JOIN athlete_stats s ON s.athlete_id = p.id
             WHERE p.id = ?1",
            [athlete_id],
            summary_from_row,
        )
        .optional()
        .map_err(|error| format!("Could not query athlete {athlete_id}: {error}"))?;
    let Some(summary) = summary else {
        return Ok(None);
    };

    let stats = connection
        .query_row(
            "SELECT last_hit, skill_avoid, skill_hit, positioning, control_speed,
                    concentration, mental, judgement, shotcalling, roaming,
                    aggressive, ego, top, jungle, mid, bottom, support
             FROM athlete_stats WHERE athlete_id = ?1",
            [athlete_id],
            stats_from_row,
        )
        .optional()
        .map_err(|error| format!("Could not query stats for athlete {athlete_id}: {error}"))?;
    let masteries = query_masteries(&connection, athlete_id)?;

    Ok(Some(AthleteDetail {
        id: summary.id,
        name: summary.name,
        team_id: summary.team_id,
        team_name: summary.team_name,
        strongest_role: summary.strongest_role,
        stats,
        masteries,
    }))
}

pub fn query_mastery(
    database_path: &Path,
    athlete_id: i64,
    champion_id: &str,
) -> Result<Option<AthleteChampionLookup>, String> {
    let index = load_athlete_index(database_path)?;
    Ok(index.mastery_for(athlete_id, champion_id))
}

pub fn load_athlete_index(database_path: &Path) -> Result<AthleteIndex, String> {
    let connection = open_database(database_path)?;
    let mut index = AthleteIndex::default();

    let mut stats_statement = connection
        .prepare(
            "SELECT athlete_id, last_hit, skill_avoid, skill_hit, positioning,
                    control_speed, concentration, mental, judgement
             FROM athlete_stats",
        )
        .map_err(|error| format!("Could not prepare athlete stat index: {error}"))?;
    let stats_rows = stats_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                CoreStats {
                    last_hit: row.get(1)?,
                    skill_avoid: row.get(2)?,
                    skill_hit: row.get(3)?,
                    positioning: row.get(4)?,
                    control_speed: row.get(5)?,
                    concentration: row.get(6)?,
                    mental: row.get(7)?,
                    judgement: row.get(8)?,
                },
            ))
        })
        .map_err(|error| format!("Could not query athlete stat index: {error}"))?;
    for row in stats_rows {
        let (athlete_id, stats) =
            row.map_err(|error| format!("Could not read athlete stat index: {error}"))?;
        index.stats.insert(athlete_id, stats);
    }

    let mut mastery_statement = connection
        .prepare(
            "SELECT athlete_id, champion_id, floor_raw, value_raw, is_recent
             FROM athlete_mastery",
        )
        .map_err(|error| format!("Could not prepare athlete mastery index: {error}"))?;
    let mastery_rows = mastery_statement
        .query_map([], |row| {
            let athlete_id = row.get::<_, i64>(0)?;
            let mastery = mastery_from_row(row, 1)?;
            Ok((athlete_id, mastery))
        })
        .map_err(|error| format!("Could not query athlete mastery index: {error}"))?;
    for row in mastery_rows {
        let (athlete_id, mastery) =
            row.map_err(|error| format!("Could not read athlete mastery index: {error}"))?;
        index
            .masteries
            .insert((athlete_id, mastery.champion_id.clone()), mastery);
    }
    Ok(index)
}

fn open_database(database_path: &Path) -> Result<Connection, String> {
    if !database_path.is_file() {
        return Err("No imported database is available. Load a save first.".to_string());
    }
    Connection::open(database_path)
        .map_err(|error| format!("Could not open athlete database: {error}"))
}

fn query_masteries(
    connection: &Connection,
    athlete_id: i64,
) -> Result<Vec<AthleteMastery>, String> {
    let mut statement = connection
        .prepare(
            "SELECT champion_id, floor_raw, value_raw, is_recent
             FROM athlete_mastery
             WHERE athlete_id = ?1
             ORDER BY value_raw DESC, champion_id",
        )
        .map_err(|error| format!("Could not prepare athlete mastery list: {error}"))?;
    let rows = statement
        .query_map([athlete_id], |row| mastery_from_row(row, 0))
        .map_err(|error| format!("Could not query athlete masteries: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not read athlete masteries: {error}"))
}

fn summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AthleteSummary> {
    let role_values = [
        ("top", row.get::<_, Option<i64>>(4)?),
        ("jungle", row.get::<_, Option<i64>>(5)?),
        ("mid", row.get::<_, Option<i64>>(6)?),
        ("bottom", row.get::<_, Option<i64>>(7)?),
        ("support", row.get::<_, Option<i64>>(8)?),
    ];
    let strongest_role = role_values
        .iter()
        .filter_map(|(role, value)| value.map(|value| (*role, value)))
        .max_by_key(|(_, value)| *value)
        .map(|(role, _)| role.to_string());
    Ok(AthleteSummary {
        id: row.get(0)?,
        name: row.get(1)?,
        team_id: row.get(2)?,
        team_name: row.get(3)?,
        strongest_role,
    })
}

fn stats_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AthleteStats> {
    Ok(AthleteStats {
        core: CoreStats {
            last_hit: row.get(0)?,
            skill_avoid: row.get(1)?,
            skill_hit: row.get(2)?,
            positioning: row.get(3)?,
            control_speed: row.get(4)?,
            concentration: row.get(5)?,
            mental: row.get(6)?,
            judgement: row.get(7)?,
        },
        tendencies: TendencyStats {
            shotcalling: row.get(8)?,
            roaming: row.get(9)?,
            aggressive: row.get(10)?,
            ego: row.get(11)?,
        },
        roles: RoleRatings {
            top: row.get(12)?,
            jungle: row.get(13)?,
            mid: row.get(14)?,
            bottom: row.get(15)?,
            support: row.get(16)?,
        },
    })
}

fn mastery_from_row(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<AthleteMastery> {
    let value_raw = row.get::<_, i64>(offset + 2)?;
    Ok(AthleteMastery {
        champion_id: row.get(offset)?,
        floor_raw: row.get(offset + 1)?,
        value_raw,
        mastery: value_raw as f64 / 10.0,
        stat_buff: mastery_buff(value_raw),
        recent: row.get::<_, i64>(offset + 3)? != 0,
    })
}

fn build_lookup(
    athlete_id: i64,
    champion_id: &str,
    base_core: CoreStats,
    stat_buff: f64,
    mastery: f64,
    recent: bool,
) -> AthleteChampionLookup {
    let multiplier = 1.0 + stat_buff;
    let effective_core = EffectiveCoreStats {
        last_hit: capped_stat(base_core.last_hit, multiplier),
        skill_avoid: capped_stat(base_core.skill_avoid, multiplier),
        skill_hit: capped_stat(base_core.skill_hit, multiplier),
        positioning: capped_stat(base_core.positioning, multiplier),
        control_speed: capped_stat(base_core.control_speed, multiplier),
        concentration: capped_stat(base_core.concentration, multiplier),
        mental: capped_stat(base_core.mental, multiplier),
        judgement: capped_stat(base_core.judgement, multiplier),
    };
    let realized_gain = EffectiveCoreStats {
        last_hit: effective_core.last_hit - base_core.last_hit as f64,
        skill_avoid: effective_core.skill_avoid - base_core.skill_avoid as f64,
        skill_hit: effective_core.skill_hit - base_core.skill_hit as f64,
        positioning: effective_core.positioning - base_core.positioning as f64,
        control_speed: effective_core.control_speed - base_core.control_speed as f64,
        concentration: effective_core.concentration - base_core.concentration as f64,
        mental: effective_core.mental - base_core.mental as f64,
        judgement: effective_core.judgement - base_core.judgement as f64,
    };
    let base_core_average = core_average(&base_core);
    let effective_core_average = effective_average(&effective_core);
    let realized_gain_average = effective_average(&realized_gain);
    let realized_stat_buff = if base_core_average > 0.0 {
        realized_gain_average / base_core_average
    } else {
        0.0
    };
    let capped_stats = effective_values(&effective_core)
        .iter()
        .filter(|value| **value >= 100.0)
        .count();
    AthleteChampionLookup {
        athlete_id,
        champion_id: champion_id.to_string(),
        mastery,
        stat_buff,
        realized_stat_buff,
        recent,
        base_core,
        effective_core,
        realized_gain,
        base_core_average,
        effective_core_average,
        realized_gain_average,
        capped_stats,
    }
}

fn capped_stat(base: i64, multiplier: f64) -> f64 {
    (base as f64 * multiplier).min(100.0)
}

fn core_average(stats: &CoreStats) -> f64 {
    [
        stats.last_hit,
        stats.skill_avoid,
        stats.skill_hit,
        stats.positioning,
        stats.control_speed,
        stats.concentration,
        stats.mental,
        stats.judgement,
    ]
    .iter()
    .sum::<i64>() as f64
        / 8.0
}

fn effective_values(stats: &EffectiveCoreStats) -> [f64; 8] {
    [
        stats.last_hit,
        stats.skill_avoid,
        stats.skill_hit,
        stats.positioning,
        stats.control_speed,
        stats.concentration,
        stats.mental,
        stats.judgement,
    ]
}

fn effective_average(stats: &EffectiveCoreStats) -> f64 {
    effective_values(stats).iter().sum::<f64>() / 8.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core(value: i64) -> CoreStats {
        CoreStats {
            last_hit: value,
            skill_avoid: value,
            skill_hit: value,
            positioning: value,
            control_speed: value,
            concentration: value,
            mental: value,
            judgement: value,
        }
    }

    #[test]
    fn mastery_buff_respects_every_threshold_boundary() {
        assert_eq!(mastery_buff(699), 0.0);
        assert_eq!(mastery_buff(700), 0.05);
        assert_eq!(mastery_buff(799), 0.05);
        assert_eq!(mastery_buff(800), 0.10);
        assert_eq!(mastery_buff(899), 0.10);
        assert_eq!(mastery_buff(900), 0.15);
        assert_eq!(mastery_buff(999), 0.15);
        assert_eq!(mastery_buff(1000), 0.20);
    }

    #[test]
    fn lookup_scales_only_the_core_profile() {
        let lookup = build_lookup(3, "fighter", core(10), 0.20, 100.0, true);
        assert_eq!(lookup.base_core.last_hit, 10);
        assert_eq!(lookup.effective_core.last_hit, 12.0);
        assert_eq!(lookup.effective_core.judgement, 12.0);
        assert_eq!(lookup.realized_gain.last_hit, 2.0);
        assert_eq!(lookup.base_core_average, 10.0);
        assert_eq!(lookup.effective_core_average, 12.0);
        assert_eq!(lookup.realized_gain_average, 2.0);
        assert_eq!(lookup.realized_stat_buff, 0.20);
        assert_eq!(lookup.capped_stats, 0);
        assert!(lookup.recent);
    }

    #[test]
    fn lookup_caps_effective_stats_and_reports_the_realized_buff() {
        let lookup = build_lookup(3, "fighter", core(95), 0.20, 100.0, false);
        assert_eq!(lookup.effective_core.last_hit, 100.0);
        assert_eq!(lookup.realized_gain.last_hit, 5.0);
        assert_eq!(lookup.base_core_average, 95.0);
        assert_eq!(lookup.effective_core_average, 100.0);
        assert_eq!(lookup.realized_gain_average, 5.0);
        assert!((lookup.realized_stat_buff - (5.0 / 95.0)).abs() < f64::EPSILON);
        assert_eq!(lookup.capped_stats, 8);
    }
}
