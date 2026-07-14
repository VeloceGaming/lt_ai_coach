//! Champion strength signals for scoring: turning raw win rate and the game's
//! per-match rating into stable, uncertainty-aware numbers. Self-contained math
//! used by pick and ban scoring.

use crate::statistics::RatingBaseline;

const RATING_PRIOR_GAMES: f64 = 10.0;
// Balanced (default) risk-aversion values; strategy system can override per request.
pub const DEFAULT_WIN_RATE_PRIOR_GAMES: f64 = 20.0;
pub const DEFAULT_WIN_RATE_RISK_Z: f64 = 0.65;

fn sigmoid(value: f64) -> f64 {
    1.0 / (1.0 + (-value).exp())
}

/// Risk-adjusted win rate: subtract a fraction of the Beta posterior's standard
/// deviation so an uncertain (low-sample) win rate ranks below a deep one — e.g.
/// 55% over 8 games sits under 55% over 200. `risk_z` sets how strongly thin
/// samples are penalised; `prior_games` controls how quickly the posterior
/// tightens. Both are strategy-tunable: Aggressive lowers risk_z (trusts thin
/// samples), Conservative raises it (demands deep history).
pub(super) fn risk_adjusted_win_rate(
    win_rate: f64,
    wins: usize,
    games: usize,
    risk_z: f64,
    prior_games: f64,
) -> f64 {
    let games = games as f64;
    if games <= 0.0 {
        return win_rate;
    }
    let wins = (wins as f64).clamp(0.0, games);
    let alpha = wins + prior_games * 0.5;
    let beta = (games - wins) + prior_games * 0.5;
    let total = alpha + beta;
    let variance = (alpha * beta) / (total * total * (total + 1.0));
    (win_rate - risk_z * variance.sqrt()).clamp(0.0, 1.0)
}

/// Role-relative strength from the game's per-match `rating`, in [0,1] with 0.5
/// at par. Win rate alone is team-confounded; rating is the game's own,
/// role-aware read on how the champion actually performed. Smoothed toward 0.5
/// by sample size, and neutral when rating or a baseline is missing.
pub(super) fn rating_strength(
    rating: Option<f64>,
    games: usize,
    baseline: Option<&RatingBaseline>,
) -> f64 {
    let (Some(rating), Some(baseline)) = (rating, baseline) else {
        return 0.5;
    };
    let raw = sigmoid((rating - baseline.mean) / baseline.std);
    let confidence = games as f64 / (games as f64 + RATING_PRIOR_GAMES);
    0.5 + (raw - 0.5) * confidence
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_adjustment_penalizes_uncertain_win_rates() {
        let (rz, pg) = (DEFAULT_WIN_RATE_RISK_Z, DEFAULT_WIN_RATE_PRIOR_GAMES);
        // Same headline win rate, but a deep sample keeps more of it than a thin one.
        let deep = risk_adjusted_win_rate(0.60, 120, 200, rz, pg);
        let thin = risk_adjusted_win_rate(0.60, 6, 10, rz, pg);
        assert!(deep > thin);
        assert!(deep < 0.60); // still discounted a little
        assert!(thin < deep);
        // No games -> nothing to adjust.
        assert_eq!(risk_adjusted_win_rate(0.60, 0, 0, rz, pg), 0.60);
    }

    #[test]
    fn rating_strength_is_role_relative_and_sample_smoothed() {
        let baseline = RatingBaseline {
            mean: 72.0,
            std: 6.0,
        };
        // Missing rating or baseline stays neutral.
        assert_eq!(rating_strength(None, 50, Some(&baseline)), 0.5);
        assert_eq!(rating_strength(Some(80.0), 50, None), 0.5);
        // Above par with a solid sample scores above neutral; below par, under.
        assert!(rating_strength(Some(80.0), 100, Some(&baseline)) > 0.55);
        assert!(rating_strength(Some(64.0), 100, Some(&baseline)) < 0.45);
        // The same rating with few games is pulled back toward neutral.
        let many = rating_strength(Some(80.0), 100, Some(&baseline));
        let few = rating_strength(Some(80.0), 3, Some(&baseline));
        assert!(few < many);
        assert!(few > 0.5);
    }
}
