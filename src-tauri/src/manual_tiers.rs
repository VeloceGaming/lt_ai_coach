// Manual tier overrides — the user's expert prior layered on top of the data.
// Each champion can be flagged S/A/C/D/F (unset = no effect at all). A tier
// nudges the champion's performance/strength signal in scoring; F removes it
// from pick recommendations entirely. Stored in its own SQLite table that the
// save-import wipe deliberately leaves alone, so opinions persist across
// re-imports. Last updated 2026-06-15, Maya-independent (Tauri/SQLite app).

use rusqlite::Connection;
use std::{collections::BTreeMap, path::Path};

// One soft-prior step on the 0..1 performance scale. S/D are two steps from
// neutral, A/C one step. Performance is ~50% of the pick score, so a full
// S<->D swing moves a champion by roughly 12 points without hard-overriding
// genuinely strong data.
const TIER_STEP: f64 = 0.06;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManualTier {
    S,
    A,
    C,
    D,
    F,
}

impl ManualTier {
    // Parse the single-letter code as stored / sent from the UI. Anything else
    // (including "" / "none") is treated as "no tier".
    pub fn from_code(code: &str) -> Option<Self> {
        match code.trim().to_ascii_uppercase().as_str() {
            "S" => Some(Self::S),
            "A" => Some(Self::A),
            "C" => Some(Self::C),
            "D" => Some(Self::D),
            "F" => Some(Self::F),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::S => "S",
            Self::A => "A",
            Self::C => "C",
            Self::D => "D",
            Self::F => "F",
        }
    }

    // Additive shift applied to the performance value (0..1). F is excluded
    // from picks entirely, so its shift is irrelevant (kept negative for any
    // path that still scores it).
    pub fn performance_shift(self) -> f64 {
        match self {
            Self::S => 2.0 * TIER_STEP,
            Self::A => TIER_STEP,
            Self::C => -TIER_STEP,
            Self::D => -2.0 * TIER_STEP,
            Self::F => -3.0 * TIER_STEP,
        }
    }

    // F means "never recommend this as a pick".
    pub fn is_excluded(self) -> bool {
        matches!(self, Self::F)
    }
}

// Create the persistence table if it doesn't exist yet. Safe to call before any
// save has been imported (the main schema also declares it, but a tier can be
// set on a fresh database).
fn ensure_table(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS manual_tiers (
                champion_id TEXT PRIMARY KEY,
                tier TEXT NOT NULL
            );",
        )
        .map_err(|error| format!("Could not create manual tiers table: {error}"))
}

// Load every flagged champion. Invalid / unknown tier codes are skipped rather
// than failing the whole load.
pub fn query_manual_tiers(database_path: &Path) -> Result<BTreeMap<String, ManualTier>, String> {
    let connection = Connection::open(database_path)
        .map_err(|error| format!("Could not open tier database: {error}"))?;
    ensure_table(&connection)?;
    let mut statement = connection
        .prepare("SELECT champion_id, tier FROM manual_tiers")
        .map_err(|error| format!("Could not prepare manual tiers query: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| format!("Could not query manual tiers: {error}"))?;
    let mut tiers = BTreeMap::new();
    for row in rows {
        let (champion_id, code) = row.map_err(|error| format!("Could not read tier: {error}"))?;
        if let Some(tier) = ManualTier::from_code(&code) {
            tiers.insert(champion_id, tier);
        }
    }
    Ok(tiers)
}

// Set or clear a champion's tier. `code` of None / "" / an unknown value clears
// the flag; a valid letter upserts it.
pub fn set_manual_tier(
    database_path: &Path,
    champion_id: &str,
    code: Option<&str>,
) -> Result<(), String> {
    let connection = Connection::open(database_path)
        .map_err(|error| format!("Could not open tier database: {error}"))?;
    ensure_table(&connection)?;
    match code.and_then(ManualTier::from_code) {
        Some(tier) => connection
            .execute(
                "INSERT INTO manual_tiers (champion_id, tier) VALUES (?1, ?2)
                 ON CONFLICT(champion_id) DO UPDATE SET tier = excluded.tier",
                rusqlite::params![champion_id, tier.code()],
            )
            .map(|_| ())
            .map_err(|error| format!("Could not save tier: {error}")),
        None => connection
            .execute(
                "DELETE FROM manual_tiers WHERE champion_id = ?1",
                rusqlite::params![champion_id],
            )
            .map(|_| ())
            .map_err(|error| format!("Could not clear tier: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_query_and_clear_round_trip() {
        let dir = std::env::temp_dir().join(format!("tiers-{}.sqlite3", std::process::id()));
        let _ = std::fs::remove_file(&dir);

        set_manual_tier(&dir, "swordsman", Some("D")).unwrap();
        set_manual_tier(&dir, "archer", Some("s")).unwrap(); // case-insensitive
        let tiers = query_manual_tiers(&dir).unwrap();
        assert_eq!(tiers.get("swordsman"), Some(&ManualTier::D));
        assert_eq!(tiers.get("archer"), Some(&ManualTier::S));

        set_manual_tier(&dir, "swordsman", None).unwrap();
        let tiers = query_manual_tiers(&dir).unwrap();
        assert!(!tiers.contains_key("swordsman"));
        assert_eq!(tiers.get("archer"), Some(&ManualTier::S));

        std::fs::remove_file(&dir).unwrap();
    }

    #[test]
    fn tier_shifts_are_ordered_and_excludes_f() {
        assert!(ManualTier::S.performance_shift() > ManualTier::A.performance_shift());
        assert!(ManualTier::A.performance_shift() > 0.0);
        assert!(ManualTier::C.performance_shift() < 0.0);
        assert!(ManualTier::D.performance_shift() < ManualTier::C.performance_shift());
        assert!(ManualTier::F.is_excluded());
        assert!(!ManualTier::D.is_excluded());
    }
}
