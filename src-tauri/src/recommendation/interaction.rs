//! Expected synergy and matchup for a candidate against the current draft.
//!
//! Averages each interaction over the plausible role assignments of both teams
//! (using the lineup model's role marginals), and keeps the best-sampled pairing
//! per ally/enemy so pick and ban scoring can cite concrete "pairs with X" /
//! "strong into Y" evidence. The scoring lives in the parent module; this file
//! only computes the evidence it consumes.

use std::collections::BTreeMap;

use crate::interactions::InteractionEvidence;

use super::{LineupModel, ROLES};

pub(super) struct ExpectedInteractions {
    pub(super) synergy_delta: f64,
    pub(super) matchup_delta: f64,
    pub(super) synergy_games: usize,
    pub(super) matchup_games: usize,
    /// Per-ally synergy evidence (best role pairing for each ally), sorted by
    /// strongest positive effect first, for concrete "pairs with X" reasons.
    pub(super) synergy_pairs: Vec<PairEvidence>,
    /// Per-enemy matchup evidence (best-sampled lane clash against each enemy
    /// pick), sorted by most favourable first, for named "strong/weak into X"
    /// counter-pick reasons.
    pub(super) matchup_pairs: Vec<MatchupEvidence>,
}

pub(super) struct PairEvidence {
    pub(super) other: String,
    pub(super) win_rate: f64,
    pub(super) games: usize,
    pub(super) delta: f64,
}

pub(super) struct MatchupEvidence {
    pub(super) enemy: String,
    pub(super) role: String,
    pub(super) win_rate: f64,
    pub(super) games: usize,
    pub(super) delta: f64,
}

pub(super) fn expected_interactions(
    candidate: &str,
    own_model: &LineupModel,
    enemy_model: &LineupModel,
    own_picks: &[String],
    enemy_picks: &[String],
    evidence: &InteractionEvidence,
    minimum_games: usize,
    role_prior: impl Fn(&str, &str) -> f64,
) -> ExpectedInteractions {
    let mut synergy_delta = 0.0;
    let mut matchup_delta = 0.0;
    let mut synergy_games = 0;
    let mut matchup_games = 0;
    // Best-sampled role pairing per ally, for concrete synergy evidence.
    let mut best_by_ally: BTreeMap<String, PairEvidence> = BTreeMap::new();
    // Best (most favourable, sampled) lane clash per enemy, for named reasons.
    let mut best_by_enemy: BTreeMap<String, MatchupEvidence> = BTreeMap::new();
    let Some(&candidate_index) = own_model.champion_indices.get(candidate) else {
        return ExpectedInteractions {
            synergy_delta,
            matchup_delta,
            synergy_games,
            matchup_games,
            synergy_pairs: Vec::new(),
            matchup_pairs: Vec::new(),
        };
    };
    for ally in own_picks {
        let Some(&ally_index) = own_model.champion_indices.get(ally) else {
            continue;
        };
        for (candidate_role_index, candidate_role) in ROLES.iter().enumerate() {
            for (ally_role_index, ally_role) in ROLES.iter().enumerate() {
                let probability = own_model.pair_marginals[candidate_index][ally_index]
                    [candidate_role_index][ally_role_index];
                if probability <= f64::EPSILON {
                    continue;
                }
                let prior =
                    (role_prior(candidate, candidate_role) + role_prior(ally, ally_role)) / 2.0;
                let estimate = evidence.synergy(
                    candidate,
                    candidate_role,
                    ally,
                    ally_role,
                    prior,
                    minimum_games,
                );
                synergy_delta += probability * (estimate.win_rate - prior);
                synergy_games = synergy_games.max(estimate.games);
                if estimate.games > 0 {
                    let delta = estimate.win_rate - prior;
                    let keep = best_by_ally
                        .get(ally)
                        .is_none_or(|current| delta > current.delta);
                    if keep {
                        best_by_ally.insert(
                            ally.clone(),
                            PairEvidence {
                                other: ally.clone(),
                                win_rate: estimate.win_rate,
                                games: estimate.games,
                                delta,
                            },
                        );
                    }
                }
            }
        }
    }
    for enemy in enemy_picks {
        let Some(&enemy_index) = enemy_model.champion_indices.get(enemy) else {
            continue;
        };
        for (role_index, role) in ROLES.iter().enumerate() {
            let joint_probability = own_model.marginals[candidate_index][role_index]
                * enemy_model.marginals[enemy_index][role_index];
            if joint_probability <= f64::EPSILON {
                continue;
            }
            let prior = (0.5 + 0.5 * (role_prior(candidate, role) - role_prior(enemy, role)))
                .clamp(0.35, 0.65);
            let estimate = evidence.matchup(candidate, role, enemy, role, prior, minimum_games);
            matchup_delta += joint_probability * (estimate.win_rate - prior);
            matchup_games = matchup_games.max(estimate.games);
            if estimate.games > 0 {
                let delta = estimate.win_rate - prior;
                // Keep the most favourable sampled lane clash per enemy.
                let keep = best_by_enemy
                    .get(enemy)
                    .is_none_or(|current| delta > current.delta);
                if keep {
                    best_by_enemy.insert(
                        enemy.clone(),
                        MatchupEvidence {
                            enemy: enemy.clone(),
                            role: (*role).to_string(),
                            win_rate: estimate.win_rate,
                            games: estimate.games,
                            delta,
                        },
                    );
                }
            }
        }
    }
    let ally_count = own_picks.len().max(1) as f64;
    let mut synergy_pairs: Vec<PairEvidence> = best_by_ally.into_values().collect();
    synergy_pairs.sort_by(|left, right| right.delta.total_cmp(&left.delta));
    let mut matchup_pairs: Vec<MatchupEvidence> = best_by_enemy.into_values().collect();
    matchup_pairs.sort_by(|left, right| right.delta.total_cmp(&left.delta));
    ExpectedInteractions {
        synergy_delta: synergy_delta / ally_count,
        matchup_delta,
        synergy_games,
        matchup_games,
        synergy_pairs,
        matchup_pairs,
    }
}
