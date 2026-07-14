//! Patch domain: everything about turning per-version champion balance sheets
//! into a usable signal.
//!
//! Three concerns live together here because they are one pipeline:
//!   1. Diffing two balance sheets into field-level changes (the write path that
//!      `database.rs` stores), via [`collect_field_changes`].
//!   2. The read-back record [`PatchChange`] that `statistics.rs` loads per row.
//!   3. The scoring math that weights fields by importance, via
//!      [`weighted_patch_impact`].
//!
//! Last updated: 2026-06-20.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::statistics::ChampionRoleStat;

/// A single field-level champion balance change in a patch, as read back from
/// the `champion_patch_changes` table by `statistics.rs`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchChange {
    pub patch: String,
    pub champion_id: String,
    pub asset: String,
    pub target: Option<String>,
    pub field: String,
    pub old_value: f64,
    pub new_value: f64,
    /// Signed percentage impact where positive means a buff and negative means
    /// a nerf. Cooldowns and attack intervals intentionally invert direction.
    pub impact: f64,
}

/// A field-level change produced while diffing two balance sheets, before it is
/// written to the database. The write-path twin of [`PatchChange`].
pub(crate) struct PatchFieldChange {
    pub patch: String,
    pub champion_id: String,
    pub asset: String,
    pub target: String,
    pub field: String,
    pub old_value: f64,
    pub new_value: f64,
    pub impact: f64,
}

/// Diff one champion's previous vs. current balance sheet into field-level
/// changes. Numeric fields are flattened by dotted path, then compared; the
/// animation/timing fields `start_timing` and `duration` are excluded because a
/// change there is not a power change. Appends to `output`.
pub(crate) fn collect_field_changes(
    patch: &str,
    champion_id: &str,
    previous: &serde_json::Value,
    current: &serde_json::Value,
    output: &mut Vec<PatchFieldChange>,
) {
    let mut previous_numbers = BTreeMap::new();
    let mut current_numbers = BTreeMap::new();
    flatten_numeric_fields(previous, "", &mut previous_numbers);
    flatten_numeric_fields(current, "", &mut current_numbers);
    for (path, new_value) in current_numbers {
        let Some(old_value) = previous_numbers.get(&path).copied() else {
            continue;
        };
        if (new_value - old_value).abs() < f64::EPSILON {
            continue;
        }
        let segments: Vec<&str> = path.split('.').collect();
        let Some(field) = segments.last() else {
            continue;
        };
        if matches!(*field, "start_timing" | "duration") {
            continue;
        }
        let asset = segments.first().copied().unwrap_or_default().to_string();
        let target = if segments.len() > 2 {
            segments[1..segments.len() - 1].join(".")
        } else {
            String::new()
        };
        output.push(PatchFieldChange {
            patch: patch.to_string(),
            champion_id: champion_id.to_string(),
            asset,
            target,
            field: (*field).to_string(),
            old_value,
            new_value,
            impact: semantic_patch_impact(&path, old_value, new_value),
        });
    }
}

/// Flatten every numeric leaf in a JSON value into `path -> number`, joining
/// nested object keys with dots. Non-numeric leaves (strings, bools) are ignored.
fn flatten_numeric_fields(
    value: &serde_json::Value,
    prefix: &str,
    output: &mut BTreeMap<String, f64>,
) {
    match value {
        serde_json::Value::Object(fields) => {
            for (name, child) in fields {
                let path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}.{name}")
                };
                flatten_numeric_fields(child, &path, output);
            }
        }
        serde_json::Value::Number(number) => {
            if let Some(number) = number.as_f64() {
                output.insert(prefix.to_string(), number);
            }
        }
        _ => {}
    }
}

/// Signed percentage impact of a single field change. Higher is a buff for most
/// stats, but cooldowns, attack intervals, and charge times invert (lower is the
/// buff), so their direction is flipped.
fn semantic_patch_impact(path: &str, old_value: f64, new_value: f64) -> f64 {
    let denominator = old_value.abs().max(1.0);
    let relative_change = (new_value - old_value) / denominator * 100.0;
    let field = path.rsplit('.').next().unwrap_or_default();
    if field.contains("cooltime") || field.contains("interval") || field.contains("charge_time") {
        -relative_change
    } else {
        relative_change
    }
}

/// How much a single balance field defines champion power, used to weight that
/// field's contribution to the overall patch signal. A 10% cut to attack should
/// move the needle far more than a 10% cut to projectile range. Keyed on the
/// leaf field name so `stat.attack` and `growth.attack` both count fully. The
/// raw direction/percentage is unchanged; this only scales how much it matters.
/// Unknown fields get a moderate default so a newly added stat is neither
/// ignored nor allowed to dominate.
fn patch_field_weight(field: &str) -> f64 {
    match field {
        // Core power: base stats, per-level growth, and cooldowns/charge times.
        "attack" | "hp" | "hp_regen" | "defence" | "magic_resistance" | "magic_power"
        | "attack_ratio" | "crit_chance" | "cooltime" | "charge_time" => 1.0,
        // Skill effect magnitudes: real but secondary to raw stats.
        "slow_ratio" | "stun" | "shield_amount" | "shield_hp_ratio" | "explosion_range"
        | "slow_duration" | "stun_duration" | "shield_duration" | "buff_duration" | "stack" => 0.6,
        // Reach and movement: rarely the headline of a patch.
        "range" | "attack_range" | "stun_range" | "speed" | "move_speed" => 0.3,
        _ => 0.5,
    }
}

/// Combine a champion's field-level changes into one signed patch signal: each
/// field's percentage impact is clamped (so one extreme value can't dominate),
/// scaled by how much that field matters, then summed. Positive = net buff.
pub(crate) fn weighted_patch_impact(changes: &[PatchChange]) -> f64 {
    changes
        .iter()
        .map(|change| patch_field_weight(&change.field) * change.impact.clamp(-25.0, 25.0))
        .sum()
}

// How the patch signal feeds the recommendation score. These are the Balanced
// (default) values; the strategy system can override them per request.
pub const DEFAULT_PATCH_IMPACT_SCALE: f64 = 25.0;
pub const DEFAULT_PATCH_EVIDENCE_GAMES: f64 = 15.0;
pub const DEFAULT_PATCH_PERFORMANCE_MAX_SHIFT: f64 = 0.06;

/// A short, plain-language note describing how the patch touched a champion,
/// shown among the recommendation reasons. (Temporary surface: per the planned
/// frontend overhaul this moves to a dedicated patch-detail view.)
pub(crate) fn patch_evidence_reason(row: &ChampionRoleStat) -> String {
    let historical_games = row.games.saturating_sub(row.current_patch_games);
    let game_context = if row.current_patch_games == 0 {
        format!("no current-patch games; {historical_games} older games discounted")
    } else if historical_games == 0 {
        format!("{} current-patch games", row.current_patch_games)
    } else {
        format!(
            "{} current-patch games; {historical_games} older games discounted",
            row.current_patch_games
        )
    };
    if row.patch_added {
        return format!("Added this patch; {game_context}");
    }
    if !row.patch_changes.is_empty() {
        let direction = if row.patch_impact >= 2.0 {
            "Buffed"
        } else if row.patch_impact <= -2.0 {
            "Nerfed"
        } else {
            "Mixed changes"
        };
        let strongest = &row.patch_changes[0];
        let target = strongest
            .target
            .as_deref()
            .map(|target| format!(" ({})", target.replace('_', " ")))
            .unwrap_or_default();
        return format!(
            "{direction} this patch: {} tracked changes ({:+.1} signal); largest {}{target} {} -> {}; {game_context}",
            row.patch_changes.len(),
            row.patch_impact,
            humanize_patch_field(&strongest.field),
            format_patch_value(strongest.old_value),
            format_patch_value(strongest.new_value)
        );
    }
    if !row.patch_changed {
        "Unchanged this patch; full history retained".to_string()
    } else {
        format!("Changed this patch; {game_context}")
    }
}

/// The patch's nudge to a champion's recommendation score: a directional,
/// game-faded shift derived from its weighted signal. This is scoring, not
/// display. The three tuning params control how loud and how long the signal is.
pub(crate) fn patch_performance_shift(
    row: &ChampionRoleStat,
    max_shift: f64,
    impact_scale: f64,
    evidence_games: f64,
) -> f64 {
    patch_performance_shift_values(
        row.patch_impact,
        row.current_patch_games,
        !row.patch_changes.is_empty(),
        max_shift,
        impact_scale,
        evidence_games,
    )
}

pub(crate) fn patch_performance_shift_values(
    patch_impact: f64,
    current_patch_games: usize,
    has_changes: bool,
    max_shift: f64,
    impact_scale: f64,
    evidence_games: f64,
) -> f64 {
    if !has_changes {
        return 0.0;
    }
    let directional_signal = (patch_impact / impact_scale).tanh();
    let evidence_gap = evidence_games / (evidence_games + current_patch_games as f64);
    max_shift * directional_signal * evidence_gap
}

fn humanize_patch_field(field: &str) -> &str {
    match field {
        "magicPower" => "magic power",
        "magicResistance" => "magic resistance",
        "moveSpeed" => "move speed",
        other => other,
    }
}

fn format_patch_value(value: f64) -> String {
    if value.fract().abs() < 0.005 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch_change(field: &str, impact: f64) -> PatchChange {
        PatchChange {
            patch: "2026.0.1".to_string(),
            champion_id: "test".to_string(),
            asset: "stat".to_string(),
            target: None,
            field: field.to_string(),
            old_value: 0.0,
            new_value: 0.0,
            impact,
        }
    }

    #[test]
    fn weighting_favors_core_stats_over_reach() {
        // Equal-magnitude nerfs: an attack cut must outweigh a range cut.
        let attack = weighted_patch_impact(&[patch_change("attack", -10.0)]);
        let range = weighted_patch_impact(&[patch_change("range", -10.0)]);
        assert!(
            attack < range,
            "attack nerf should be stronger than range nerf"
        );
        assert_eq!(attack, -10.0); // core stat, full weight
        assert!((range - -3.0).abs() < 1e-9); // reach, 0.3 weight

        // Direction is preserved and a buff stays positive.
        assert!(weighted_patch_impact(&[patch_change("hp", 12.0)]) > 0.0);

        // Per-field clamp still caps an extreme value before weighting.
        assert_eq!(
            weighted_patch_impact(&[patch_change("attack", -80.0)]),
            -25.0
        );

        // Unknown fields contribute at the moderate default, not zero.
        assert!((weighted_patch_impact(&[patch_change("mystery_stat", 10.0)]) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn patch_note_prior_is_directional_bounded_and_fades_with_games() {
        let (ms, is, eg) = (
            DEFAULT_PATCH_PERFORMANCE_MAX_SHIFT,
            DEFAULT_PATCH_IMPACT_SCALE,
            DEFAULT_PATCH_EVIDENCE_GAMES,
        );
        let early_buff = patch_performance_shift_values(50.0, 0, true, ms, is, eg);
        let sampled_buff = patch_performance_shift_values(50.0, 60, true, ms, is, eg);
        let early_nerf = patch_performance_shift_values(-50.0, 0, true, ms, is, eg);

        assert!(early_buff > 0.0);
        assert!(early_buff <= DEFAULT_PATCH_PERFORMANCE_MAX_SHIFT);
        assert!(sampled_buff > 0.0 && sampled_buff < early_buff);
        assert!(early_nerf < 0.0);
        assert_eq!(
            patch_performance_shift_values(50.0, 0, false, ms, is, eg),
            0.0
        );
    }

    #[test]
    fn charge_time_reduction_reads_as_buff_and_timing_is_excluded() {
        let previous = serde_json::json!({
            "ult": { "charge_time": 60, "duration": 20, "start_timing": 10 },
        });
        let current = serde_json::json!({
            "ult": { "charge_time": 48, "duration": 24, "start_timing": 8 },
        });
        let mut changes = Vec::new();
        collect_field_changes("2026.0.1", "test", &previous, &current, &mut changes);

        // Only charge_time survives; duration and start_timing are excluded.
        assert_eq!(changes.len(), 1);
        let change = &changes[0];
        assert_eq!(change.field, "charge_time");
        assert_eq!(change.impact, 20.0); // 60 -> 48 is a 20% buff after inversion
    }
}
