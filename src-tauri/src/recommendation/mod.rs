use crate::{
    draft::{DraftCatalog, DraftChampion},
    interactions::InteractionEvidence,
    manual_tiers::ManualTier,
    statistics::{ChampionRoleStat, RatingBaseline, RoleStatistics},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

mod interaction;
mod strength;

use crate::patch::{
    DEFAULT_PATCH_EVIDENCE_GAMES, DEFAULT_PATCH_IMPACT_SCALE, DEFAULT_PATCH_PERFORMANCE_MAX_SHIFT,
};
use interaction::expected_interactions;
use strength::{
    rating_strength, risk_adjusted_win_rate, DEFAULT_WIN_RATE_PRIOR_GAMES, DEFAULT_WIN_RATE_RISK_Z,
};

const ROLES: [&str; 5] = ["top", "jungle", "mid", "bot", "support"];
// Games at which a single ally pairing is trusted ~half-way when it pulls the
// synergy signal toward the best pair; keeps low-sample pairs from dominating.
const SYNERGY_CONFIDENCE_GAMES: f64 = 15.0;
const FIRST_PICK_POOL_DELTA: f64 = 8.0;
const BAN_INTERACTION_CONFIDENCE_GAMES: f64 = 20.0;
const BAN_CONTEXT_SYNERGY_WEIGHT: f64 = 35.0;
const BAN_CONTEXT_MATCHUP_WEIGHT: f64 = 40.0;
const BAN_CONTEXT_DELTA_CLAMP: f64 = 0.15;
const BAN_CONTEXT_REASON_THRESHOLD: f64 = 0.02;
// Max ban-score discount for a champion that is also Blue's own likely first
// pick, scaled by how close it is to Blue's best pick score.
const OWN_CLAIM_DISCOUNT_WEIGHT: f64 = 15.0;
// Stage 4B: survival_factor(pressure) = clamp(BASELINE - SLOPE * pressure, MIN, MAX).
// pressure in [-1, 1] is a champion's red ban score relative to the pool's
// median/spread, so BASELINE is also the neutral value used when the pool's
// scores are tied (spread collapses to ~0).
const RED_PRESSURE_BASELINE: f64 = 0.65;
const RED_PRESSURE_SLOPE: f64 = 0.30;
const SURVIVAL_FACTOR_MIN: f64 = 0.35;
const SURVIVAL_FACTOR_MAX: f64 = 0.95;
// Bound on how much the leave-open portfolio comparison can shift a ban score.
const PORTFOLIO_ADJUSTMENT_CLAMP: f64 = 10.0;
const PORTFOLIO_ADJUSTMENT_REASON_THRESHOLD: f64 = 3.0;
// The "claim tier" for portfolio reasoning is much tighter than the general
// acceptable-pick pool: only the genuinely contested top picks, so survival
// pressure compares like-for-like instead of letting an uncontested mediocre
// pick masquerade as Blue's best claim.
const CLAIM_TIER_SIZE: usize = 3;
const CLAIM_TIER_DELTA: f64 = 4.0;
// A ban candidate the OPPOSING side would also rank near its own best ban is
// "redundant" to ban: the opponent would likely remove it regardless, so this
// side's marginal denial value is reduced.
const REDUNDANT_BAN_DELTA: f64 = 8.0;
const REDUNDANT_BAN_DISCOUNT_WEIGHT: f64 = 12.0;
const DEFAULT_BANS_PER_SIDE: usize = 3;

const ROLE_CREDIBILITY_MIN_PROBABILITY: f64 = 0.15;
const ROLE_CREDIBILITY_MIN_GAMES: usize = 5;
const NEW_CHAMPION_ROLE_MIN_PROBABILITY: f64 = 0.30;
const ROLE_COLLISION_SCORE_WEIGHT: f64 = 12.0;
const ROLE_COLLISION_SCORE_CLAMP: f64 = 14.0;
// A champion whose only viable role is already credibly covered is redundant in
// the lineup. This flat penalty is large enough to drop it out of the 8-slot
// shortlist while still letting it resurface if every alternative is dire.
const ROLE_COLLISION_LOCKED_PENALTY: f64 = 28.0;
// Minimum dominant-role probability before the locked penalty applies, so a
// thin/uncertain champion with a flat role spread is never nuked on a weak guess.
const ROLE_COLLISION_LOCKED_MIN_PROBABILITY: f64 = 0.5;
const FORCED_OFF_ROLE_SCORE_PENALTY: f64 = 8.0;
const ROLE_COLLISION_REASON_THRESHOLD: f64 = 3.0;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationRequest {
    pub mode: String,
    pub side: String,
    pub blue_bans: Vec<String>,
    pub red_bans: Vec<String>,
    pub blue_picks: Vec<String>,
    pub red_picks: Vec<String>,
    #[serde(default = "default_bans_per_side")]
    pub bans_per_side: usize,
    pub history_blue: Vec<String>,
    pub history_red: Vec<String>,
    #[serde(default)]
    pub weights: ScoringWeights,
    #[serde(default)]
    pub tuning: DraftTuning,
    #[serde(default = "default_minimum_interaction_games")]
    pub minimum_interaction_games: usize,
    #[serde(default)]
    pub blue_lineup: Option<DraftLineup>,
    #[serde(default)]
    pub red_lineup: Option<DraftLineup>,
    /// User-confirmed role assignments (champion id -> role name). These override
    /// the engine's inferred role for a picked champion, so coverage, matchups,
    /// and synergy are all evaluated as if that champion plays the chosen role.
    #[serde(default)]
    pub role_overrides: BTreeMap<String, String>,
}

fn default_bans_per_side() -> usize {
    DEFAULT_BANS_PER_SIDE
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftLineup {
    pub top: Option<i64>,
    pub jungle: Option<i64>,
    pub mid: Option<i64>,
    pub bot: Option<i64>,
    pub support: Option<i64>,
}

impl DraftLineup {
    fn athlete_for_role(&self, role: &str) -> Option<i64> {
        match role {
            "top" => self.top,
            "jungle" => self.jungle,
            "mid" => self.mid,
            "bot" | "bottom" => self.bot,
            "support" => self.support,
            _ => None,
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoringWeights {
    pub performance: f64,
    pub synergy: f64,
    pub matchup: f64,
    pub flexibility: f64,
    #[serde(default)]
    pub draft_presence: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            performance: 70.0,
            synergy: 35.0,
            matchup: 20.0,
            flexibility: 5.0,
            draft_presence: 5.0,
        }
    }
}

/// Strategy-level tuning knobs that travel with the recommendation request.
/// Defaults are the Balanced values, which reproduce today's exact behavior.
/// Aggressive raises patch responsiveness and lowers risk aversion (willing to
/// try thin-sample picks). Conservative does the opposite.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftTuning {
    /// Maximum score shift a patch can apply before any real games exist (±units).
    #[serde(default = "default_patch_max_shift")]
    pub patch_max_shift: f64,
    /// Saturation scale for the patch signal — higher means severity matters more.
    #[serde(default = "default_patch_impact_scale")]
    pub patch_impact_scale: f64,
    /// Games at which the patch prior is half-faded by real results.
    #[serde(default = "default_patch_evidence_games")]
    pub patch_evidence_games: f64,
    /// How hard thin win-rate samples are penalised (higher = more conservative).
    #[serde(default = "default_win_rate_risk_z")]
    pub win_rate_risk_z: f64,
    /// Bayesian prior games for win-rate uncertainty (higher = needs more evidence).
    #[serde(default = "default_win_rate_prior_games")]
    pub win_rate_prior_games: f64,
}

fn default_patch_max_shift() -> f64 {
    DEFAULT_PATCH_PERFORMANCE_MAX_SHIFT
}
fn default_patch_impact_scale() -> f64 {
    DEFAULT_PATCH_IMPACT_SCALE
}
fn default_patch_evidence_games() -> f64 {
    DEFAULT_PATCH_EVIDENCE_GAMES
}
fn default_win_rate_risk_z() -> f64 {
    DEFAULT_WIN_RATE_RISK_Z
}
fn default_win_rate_prior_games() -> f64 {
    DEFAULT_WIN_RATE_PRIOR_GAMES
}

impl Default for DraftTuning {
    fn default() -> Self {
        Self {
            patch_max_shift: DEFAULT_PATCH_PERFORMANCE_MAX_SHIFT,
            patch_impact_scale: DEFAULT_PATCH_IMPACT_SCALE,
            patch_evidence_games: DEFAULT_PATCH_EVIDENCE_GAMES,
            win_rate_risk_z: DEFAULT_WIN_RATE_RISK_Z,
            win_rate_prior_games: DEFAULT_WIN_RATE_PRIOR_GAMES,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationShortlist {
    pub pick_recommendations: Vec<Recommendation>,
    pub ban_recommendations: Vec<Recommendation>,
    /// Every scoreable champion, sorted by pick score — the consultation pool
    /// behind champion search and role browsing. Superset of the top-8
    /// `pick_recommendations` shortlist above.
    pub pick_pool: Vec<Recommendation>,
    /// Every ban-scoreable champion, sorted by ban score. Also includes a
    /// protected Blue first pick that the ban shortlist deliberately omits.
    pub ban_pool: Vec<Recommendation>,
    /// Champions with no pick card in this draft, and why (banned, picked,
    /// Fearless-burned, F-tiered, no data). Lets a search explain the absence.
    pub pick_exclusions: Vec<ExcludedChampion>,
    /// Champions with no ban card in this draft. Narrower than the pick list:
    /// F-tiered and (soft) Fearless-burned champions can still be ban-scored.
    pub ban_exclusions: Vec<ExcludedChampion>,
    pub blue_projection: TeamProjection,
    pub red_projection: TeamProjection,
}

/// Why a champion has no score card in the current draft. Side names are
/// absolute (Blue/Red) except the Fearless variants, which are relative to the
/// requesting side because "your team already played this" is what the coach
/// needs to know.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExclusionReason {
    BannedByBlue,
    BannedByRed,
    PickedByBlue,
    PickedByRed,
    /// Fearless: the requesting side already played it earlier in the series.
    FearlessUsedOwn,
    /// Hard Fearless: the opponent already played it earlier in the series.
    FearlessUsedByOpponent,
    /// Manual tier F ("never recommend").
    ManuallyExcluded,
    /// The champion has no recorded games, so there is nothing to score.
    NoData,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcludedChampion {
    pub champion_id: String,
    pub champion_name: String,
    pub portrait: Option<crate::statistics::ChampionPortrait>,
    pub reason: ExclusionReason,
}

fn excluded_champion(champion: &DraftChampion, reason: ExclusionReason) -> ExcludedChampion {
    ExcludedChampion {
        champion_id: champion.id.clone(),
        champion_name: champion.name.clone(),
        portrait: champion.portrait.clone(),
        reason,
    }
}

/// Exclusion reason for a champion already on the board, if any. Checks bans
/// before picks; a champion normally appears in exactly one of the four lists.
fn usage_exclusion_reason(
    champion_id: &str,
    request: &RecommendationRequest,
) -> Option<ExclusionReason> {
    if request.blue_bans.iter().any(|id| id == champion_id) {
        Some(ExclusionReason::BannedByBlue)
    } else if request.red_bans.iter().any(|id| id == champion_id) {
        Some(ExclusionReason::BannedByRed)
    } else if request.blue_picks.iter().any(|id| id == champion_id) {
        Some(ExclusionReason::PickedByBlue)
    } else if request.red_picks.iter().any(|id| id == champion_id) {
        Some(ExclusionReason::PickedByRed)
    } else {
        None
    }
}

/// For a champion blocked by hard Fearless: whose series history burned it.
/// Own history wins when both sides have played it.
fn hard_fearless_exclusion_reason(
    champion_id: &str,
    request: &RecommendationRequest,
) -> ExclusionReason {
    let own_history = if request.side == "red" {
        &request.history_red
    } else {
        &request.history_blue
    };
    if own_history.iter().any(|id| id == champion_id) {
        ExclusionReason::FearlessUsedOwn
    } else {
        ExclusionReason::FearlessUsedByOpponent
    }
}

/// Whether a reason argues *for* the recommended action (positive / good news),
/// warns about a downside (negative), or is just informational (neutral). The UI
/// uses this to color-code each line instead of showing a green check on every
/// reason — so "Weak into Chef" no longer looks like a plus.
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasonTone {
    Positive,
    Negative,
    Neutral,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reason {
    pub text: String,
    pub tone: ReasonTone,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation_key: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub translation_values: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub translation_champion_ids: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub translation_role_ids: BTreeMap<String, String>,
    /// Placeholder name -> translation key, for a placeholder whose value is a
    /// phrase the engine chose rather than a number or a name. Without this the
    /// phrase would travel as a literal and be pasted into a translated sentence
    /// still in English.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub translation_keys: BTreeMap<String, String>,
}

impl Reason {
    fn positive(text: impl Into<String>) -> Self {
        Self::new(text, ReasonTone::Positive)
    }
    fn negative(text: impl Into<String>) -> Self {
        Self::new(text, ReasonTone::Negative)
    }
    fn neutral(text: impl Into<String>) -> Self {
        Self::new(text, ReasonTone::Neutral)
    }
    fn new(text: impl Into<String>, tone: ReasonTone) -> Self {
        Self {
            text: text.into(),
            tone,
            translation_key: None,
            translation_values: BTreeMap::new(),
            translation_champion_ids: BTreeMap::new(),
            translation_role_ids: BTreeMap::new(),
            translation_keys: BTreeMap::new(),
        }
    }
    fn translated<I, K, V>(mut self, key: &str, values: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.translation_key = Some(key.to_string());
        self.translation_values = values
            .into_iter()
            .map(|(name, value)| (name.into(), value.into()))
            .collect();
        self
    }
    fn translated_champions<I, K, V>(mut self, champions: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.translation_champion_ids = champions
            .into_iter()
            .map(|(placeholder, champion_id)| (placeholder.into(), champion_id.into()))
            .collect();
        self
    }
    fn translated_roles<I, K, V>(mut self, roles: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.translation_role_ids = roles
            .into_iter()
            .map(|(placeholder, role_id)| (placeholder.into(), role_id.into()))
            .collect();
        self
    }
    fn translated_phrases<I, K, V>(mut self, phrases: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.translation_keys = phrases
            .into_iter()
            .map(|(placeholder, key)| (placeholder.into(), key.into()))
            .collect();
        self
    }
    /// Tone that follows the sign of a delta: non-negative reads as a plus,
    /// negative as a downside. Used for signed win-rate / matchup lines.
    fn from_delta(text: impl Into<String>, delta: f64) -> Self {
        if delta >= 0.0 {
            Self::positive(text)
        } else {
            Self::negative(text)
        }
    }
}

impl std::fmt::Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

/// Tone for a patch-evidence line: a buff or a fresh addition reads as good news,
/// a nerf as a downside, and mixed/unclear changes as neutral. Mirrors the
/// buff/nerf thresholds in `patch::patch_evidence_reason`.
fn patch_reason_tone(row: &ChampionRoleStat) -> ReasonTone {
    if row.patch_added || row.patch_impact >= 2.0 {
        ReasonTone::Positive
    } else if row.patch_impact <= -2.0 {
        ReasonTone::Negative
    } else {
        ReasonTone::Neutral
    }
}

fn translated_patch_reason(row: &ChampionRoleStat) -> Reason {
    let historical_games = row.games.saturating_sub(row.current_patch_games);
    let context = if row.current_patch_games == 0 {
        "NoCurrent"
    } else if historical_games == 0 {
        "CurrentOnly"
    } else {
        "CurrentAndOlder"
    };
    let (kind, mut values) = if row.patch_added {
        ("Added", BTreeMap::new())
    } else if let Some(strongest) = row.patch_changes.first() {
        let kind = if row.patch_impact >= 2.0 {
            "Buffed"
        } else if row.patch_impact <= -2.0 {
            "Nerfed"
        } else {
            "Mixed"
        };
        let target = strongest
            .target
            .as_deref()
            .map(|target| format!(" ({})", target.replace('_', " ")))
            .unwrap_or_default();
        let values = BTreeMap::from([
            ("changes".to_string(), row.patch_changes.len().to_string()),
            ("signal".to_string(), format!("{:+.1}", row.patch_impact)),
            ("field".to_string(), crate::patch::humanize_patch_field(&strongest.field).to_string()),
            ("target".to_string(), target),
            ("oldValue".to_string(), crate::patch::format_patch_value(strongest.old_value)),
            ("newValue".to_string(), crate::patch::format_patch_value(strongest.new_value)),
        ]);
        (kind, values)
    } else {
        ("Changed", BTreeMap::new())
    };
    values.insert("currentGames".to_string(), row.current_patch_games.to_string());
    values.insert("olderGames".to_string(), historical_games.to_string());
    Reason::new(crate::patch::patch_evidence_reason(row), patch_reason_tone(row)).translated(
        &format!("recommendation.reason.patch{kind}{context}"),
        values,
    )
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub champion_id: String,
    pub champion_name: String,
    pub portrait: Option<crate::statistics::ChampionPortrait>,
    pub score: f64,
    pub suggested_role: Option<String>,
    pub adjusted_win_rate: f64,
    pub role_win_rate: Option<f64>,
    pub games: usize,
    pub confidence: f64,
    pub flexibility: usize,
    pub synergy_score: f64,
    pub matchup_score: f64,
    pub interaction_games: usize,
    pub reasons: Vec<Reason>,
    pub athlete_context: Option<RecommendationAthleteContext>,
    #[serde(skip)]
    #[allow(dead_code)]
    ban_score_components: Option<BanScoreComponents>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationAthleteContext {
    pub athlete_id: i64,
    pub role: String,
    pub mastery: f64,
    pub nominal_stat_buff: f64,
    pub realized_stat_buff: f64,
    pub base_core: crate::athletes::CoreStats,
    pub effective_core: crate::athletes::EffectiveCoreStats,
    pub realized_gain: crate::athletes::EffectiveCoreStats,
    pub base_core_average: f64,
    pub effective_core_average: f64,
    pub realized_gain_average: f64,
    pub capped_stats: usize,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
struct BanScoreComponents {
    base_score: f64,
    own_claim_discount: f64,
    portfolio_adjustment: f64,
    redundant_ban_discount: f64,
    final_score: f64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamProjection {
    pub assignments_considered: usize,
    pub confidence: f64,
    pub champions: Vec<ChampionRoleProjection>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionRoleProjection {
    pub champion_id: String,
    pub champion_name: String,
    pub portrait: Option<crate::statistics::ChampionPortrait>,
    pub roles: Vec<RoleProbability>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleProbability {
    pub role: String,
    pub probability: f64,
    pub assigned: bool,
}

pub fn build_shortlist_with_athletes(
    request: &RecommendationRequest,
    catalog: &DraftCatalog,
    statistics: &RoleStatistics,
    interactions: &InteractionEvidence,
    manual_tiers: &BTreeMap<String, ManualTier>,
    athlete_index: &crate::athletes::AthleteIndex,
) -> RecommendationShortlist {
    build_shortlist_internal(
        request,
        catalog,
        statistics,
        interactions,
        manual_tiers,
        Some(athlete_index),
    )
}

#[cfg(test)]
pub fn build_shortlist(
    request: &RecommendationRequest,
    catalog: &DraftCatalog,
    statistics: &RoleStatistics,
    interactions: &InteractionEvidence,
    manual_tiers: &BTreeMap<String, ManualTier>,
) -> RecommendationShortlist {
    build_shortlist_internal(
        request,
        catalog,
        statistics,
        interactions,
        manual_tiers,
        None,
    )
}

fn build_shortlist_internal(
    request: &RecommendationRequest,
    catalog: &DraftCatalog,
    statistics: &RoleStatistics,
    interactions: &InteractionEvidence,
    manual_tiers: &BTreeMap<String, ManualTier>,
    athlete_index: Option<&crate::athletes::AthleteIndex>,
) -> RecommendationShortlist {
    let request_placeholders = request_only_champions(request, catalog);
    let champions = catalog
        .champions
        .iter()
        .map(|champion| (champion.id.as_str(), champion))
        .chain(
            request_placeholders
                .iter()
                .map(|champion| (champion.id.as_str(), champion)),
        )
        .collect::<BTreeMap<_, _>>();
    let overall = statistics
        .overall_rows
        .iter()
        .map(|row| (row.champion_id.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    let role_rows = statistics.role_rows.iter().fold(
        BTreeMap::<&str, Vec<&ChampionRoleStat>>::new(),
        |mut rows, row| {
            rows.entry(&row.champion_id).or_default().push(row);
            rows
        },
    );
    let rating_baselines = statistics.rating_baselines();
    let used = request
        .blue_bans
        .iter()
        .chain(&request.red_bans)
        .chain(&request.blue_picks)
        .chain(&request.red_picks)
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let forced_roles = forced_role_indices(request);
    let blue_model = model_lineup(&request.blue_picks, &champions, &role_rows, &forced_roles);
    let red_model = model_lineup(&request.red_picks, &champions, &role_rows, &forced_roles);
    let blue_projection = blue_model.projection.clone();
    let red_projection = red_model.projection.clone();
    let own_picks = if request.side == "red" {
        &request.red_picks
    } else {
        &request.blue_picks
    };
    let (own_model, enemy_model, enemy_picks) = if request.side == "red" {
        (&red_model, &blue_model, &request.blue_picks)
    } else {
        (&blue_model, &red_model, &request.red_picks)
    };
    let covered_roles = projected_roles(&own_model.projection);
    let blue_covered_roles = projected_roles(&blue_model.projection);

    let mut picks = Vec::new();
    let mut blue_first_picks = Vec::new();
    let mut pick_exclusions = Vec::new();
    let mut ban_exclusions = Vec::new();
    for (&champion_id, &champion) in &champions {
        if let Some(reason) = usage_exclusion_reason(champion_id, request) {
            pick_exclusions.push(excluded_champion(champion, reason));
            ban_exclusions.push(excluded_champion(champion, reason));
            continue;
        }
        if blocked_by_hard_fearless(champion_id, request) {
            let reason = hard_fearless_exclusion_reason(champion_id, request);
            pick_exclusions.push(excluded_champion(champion, reason));
            ban_exclusions.push(excluded_champion(champion, reason));
            continue;
        }
        let Some(overall_row) = overall.get(champion_id).copied() else {
            pick_exclusions.push(excluded_champion(champion, ExclusionReason::NoData));
            ban_exclusions.push(excluded_champion(champion, ExclusionReason::NoData));
            continue;
        };
        // Manual tier override. An F flag ("never recommend") drops the
        // champion from pick suggestions entirely; other tiers nudge its score.
        // Pick-only exclusion: the ban loops below still score F-tier champions.
        let manual_tier = manual_tiers.get(champion_id).copied();
        if manual_tier.is_some_and(ManualTier::is_excluded) {
            pick_exclusions.push(excluded_champion(champion, ExclusionReason::ManuallyExcluded));
            continue;
        }
        let candidate_roles = role_rows.get(champion_id).cloned().unwrap_or_default();
        let flexibility = credible_flexibility(champion, &candidate_roles);

        let candidate_pick = if pick_is_legal(champion_id, request) {
            let mut projected_picks = own_picks.clone();
            projected_picks.push(champion_id.to_string());
            let candidate_model = model_lineup(&projected_picks, &champions, &role_rows, &forced_roles);
            let projected_role = primary_role_projection(&candidate_model, champion_id);
            let mut recommendation = score_pick(
                champion,
                overall_row,
                &candidate_roles,
                flexibility,
                &covered_roles,
                projected_role,
                &candidate_model,
                enemy_model,
                own_picks,
                enemy_picks,
                &role_rows,
                interactions,
                request,
                &rating_baselines,
                &statistics.draft_presence,
                manual_tier,
            );
            recommendation.athlete_context =
                recommendation_athlete_context(request, champion_id, projected_role, athlete_index);
            Some(recommendation)
        } else {
            // Only reachable in soft Fearless (hard mode was caught above), so
            // the block is always this side's own series history.
            pick_exclusions.push(excluded_champion(champion, ExclusionReason::FearlessUsedOwn));
            None
        };
        if let Some(pick) = candidate_pick {
            picks.push(pick);
        }
        if request.side == "red"
            && request.blue_picks.is_empty()
            && pick_is_legal_for_side(champion_id, request, "blue")
        {
            let projected_picks = vec![champion_id.to_string()];
            let candidate_model = model_lineup(&projected_picks, &champions, &role_rows, &forced_roles);
            let projected_role = primary_role_projection(&candidate_model, champion_id);
            blue_first_picks.push(score_pick(
                champion,
                overall_row,
                &candidate_roles,
                flexibility,
                &blue_covered_roles,
                projected_role,
                &candidate_model,
                &red_model,
                &request.blue_picks,
                &request.red_picks,
                &role_rows,
                interactions,
                request,
                &rating_baselines,
                &statistics.draft_presence,
                manual_tier,
            ));
        }
    }
    let first_pick_candidates = if request.side == "red" {
        &blue_first_picks
    } else {
        &picks
    };
    let pick_scores = first_pick_candidates
        .iter()
        .map(|recommendation| (recommendation.champion_id.as_str(), recommendation.score))
        .collect::<BTreeMap<_, _>>();
    let first_pick = first_pick_context(request, first_pick_candidates);
    // The current side's own pick value per champion, so ban scoring can
    // discount candidates this side actually plans to pick. For Blue this
    // mirrors `pick_scores`; for Red it is Red's own view (while `pick_scores`
    // deliberately holds Blue's first-pick values for denial reasoning).
    let own_pick_scores = picks
        .iter()
        .map(|recommendation| (recommendation.champion_id.as_str(), recommendation.score))
        .collect::<BTreeMap<_, _>>();
    let own_first_pick = first_pick_context(request, &picks);
    let red_bans_remaining = request
        .bans_per_side
        .clamp(1, 5)
        .saturating_sub(request.red_bans.len());
    // Stage 4B: when Blue's acceptable-pick pool has multiple comparable
    // candidates, Blue can still only claim one via its first pick. Model the
    // pool members' Red-side ban/pick values once, then compare "leave open"
    // outcomes per ban candidate against this no-ban baseline.
    let portfolio_pool = if request.side == "blue" {
        build_portfolio_pool(
            &champions,
            &overall,
            &role_rows,
            &rating_baselines,
            interactions,
            request,
            &statistics.draft_presence,
            &pick_scores,
            &first_pick,
            &red_model,
            &blue_model,
            manual_tiers,
            &forced_roles,
        )
    } else {
        Vec::new()
    };
    let baseline_portfolio = portfolio_value(&portfolio_pool.iter().collect::<Vec<_>>());
    // Stage 4B: a ban candidate the OPPOSING side would also rank near its
    // own best ban is largely redundant for this side to spend a ban on —
    // the opponent would likely remove it regardless. Estimate the opposing
    // side's ban score for each candidate by reusing this side's own
    // pick_scores/first_pick context with the side flipped (same
    // no-recursion approximation already used by build_portfolio_pool), then
    // compare each candidate against the opposing side's best.
    let opposing_perspective = RecommendationRequest {
        side: if request.side == "blue" {
            "red".to_string()
        } else {
            "blue".to_string()
        },
        ..request.clone()
    };
    let (opposing_own_model, opposing_own_picks, opposing_enemy_picks) =
        if opposing_perspective.side == "red" {
            (&red_model, &request.red_picks, &request.blue_picks)
        } else {
            (&blue_model, &request.blue_picks, &request.red_picks)
        };
    let mut opposing_ban_scores = BTreeMap::new();
    for (&champion_id, &champion) in &champions {
        if used.contains(champion_id) || blocked_by_hard_fearless(champion_id, request) {
            continue;
        }
        let Some(overall_row) = overall.get(champion_id).copied() else {
            continue;
        };
        let candidate_roles = role_rows.get(champion_id).cloned().unwrap_or_default();
        let ban_flexibility = candidate_roles
            .iter()
            .filter(|row| row.games >= 5 && row.adjusted_win_rate >= 0.48)
            .count();
        let pick_score = pick_scores.get(champion_id).copied();
        let draft_context = ban_draft_context(
            champion_id,
            opposing_own_model,
            opposing_own_picks,
            opposing_enemy_picks,
            &champions,
            &role_rows,
            interactions,
            request.minimum_interaction_games,
            &forced_roles,
        );
        let score = score_ban(
            champion,
            overall_row,
            &candidate_roles,
            ban_flexibility,
            &rating_baselines,
            &overall,
            interactions,
            &draft_context,
            &opposing_perspective,
            pick_score,
            &first_pick,
            // No-recursion approximation: the opposing side's own pick plans
            // are not modeled here, so its own-claim inputs stay neutral.
            None,
            &FirstPickContext::default(),
            &statistics.draft_presence,
            0.0,
            None,
            None,
            None,
            0.0,
            manual_tiers.get(champion_id).copied(),
        )
        .score;
        opposing_ban_scores.insert(champion_id, score);
    }
    let opposing_best_ban_score = opposing_ban_scores.values().copied().fold(0.0, f64::max);
    let mut bans = Vec::new();
    let mut protected_bans = Vec::new();
    for (&champion_id, &champion) in &champions {
        if used.contains(champion_id) || blocked_by_hard_fearless(champion_id, request) {
            continue;
        }
        let Some(overall_row) = overall.get(champion_id).copied() else {
            continue;
        };
        let candidate_roles = role_rows.get(champion_id).cloned().unwrap_or_default();
        let ban_flexibility = candidate_roles
            .iter()
            .filter(|row| row.games >= 5 && row.adjusted_win_rate >= 0.48)
            .count();
        let pick_score = pick_scores.get(champion_id).copied();
        // A protected Blue first pick stays out of the ban shortlist, but the
        // consultation pool still scores it so a search can show its card.
        let protected = request.side == "blue"
            && is_protected_blue_first_pick(pick_score, &first_pick, red_bans_remaining);
        let (portfolio_adjustment, portfolio_claim, portfolio_leftover, leftover_survival) =
            portfolio_adjustment_for(&portfolio_pool, &baseline_portfolio, champion_id);
        let redundant_discount = redundant_ban_discount(
            opposing_ban_scores.get(champion_id).copied().unwrap_or(0.0),
            opposing_best_ban_score,
        );
        let draft_context = ban_draft_context(
            champion_id,
            own_model,
            own_picks,
            enemy_picks,
            &champions,
            &role_rows,
            interactions,
            request.minimum_interaction_games,
            &forced_roles,
        );
        let recommendation = score_ban(
            champion,
            overall_row,
            &candidate_roles,
            ban_flexibility,
            &rating_baselines,
            &overall,
            interactions,
            &draft_context,
            request,
            pick_score,
            &first_pick,
            own_pick_scores.get(champion_id).copied(),
            &own_first_pick,
            &statistics.draft_presence,
            portfolio_adjustment,
            portfolio_claim,
            portfolio_leftover,
            leftover_survival,
            redundant_discount,
            manual_tiers.get(champion_id).copied(),
        );
        if protected {
            protected_bans.push(recommendation);
        } else {
            bans.push(recommendation);
        }
    }
    // The full sorted pools feed champion search / role browsing; the shortlists
    // below stay truncated to the top 8 for the recommendation cards.
    let mut pick_pool = picks.clone();
    sort_rows(&mut pick_pool);
    let mut ban_pool = bans.clone();
    ban_pool.append(&mut protected_bans);
    sort_rows(&mut ban_pool);
    sort_and_limit(&mut picks);
    sort_and_limit(&mut bans);
    RecommendationShortlist {
        pick_recommendations: picks,
        ban_recommendations: bans,
        pick_pool,
        ban_pool,
        pick_exclusions,
        ban_exclusions,
        blue_projection,
        red_projection,
    }
}

fn recommendation_athlete_context(
    request: &RecommendationRequest,
    champion_id: &str,
    projected_role: Option<(&str, f64, bool)>,
    athlete_index: Option<&crate::athletes::AthleteIndex>,
) -> Option<RecommendationAthleteContext> {
    let (role, _, credible) = projected_role?;
    if !credible {
        return None;
    }
    let lineup = if request.side == "red" {
        request.red_lineup.as_ref()?
    } else {
        request.blue_lineup.as_ref()?
    };
    let athlete_id = lineup.athlete_for_role(role)?;
    let lookup = athlete_index?.mastery_for(athlete_id, champion_id)?;
    Some(RecommendationAthleteContext {
        athlete_id,
        role: role.to_string(),
        mastery: lookup.mastery,
        nominal_stat_buff: lookup.stat_buff,
        realized_stat_buff: lookup.realized_stat_buff,
        base_core: lookup.base_core,
        effective_core: lookup.effective_core,
        realized_gain: lookup.realized_gain,
        base_core_average: lookup.base_core_average,
        effective_core_average: lookup.effective_core_average,
        realized_gain_average: lookup.realized_gain_average,
        capped_stats: lookup.capped_stats,
    })
}

fn request_only_champions(
    request: &RecommendationRequest,
    catalog: &DraftCatalog,
) -> Vec<DraftChampion> {
    let known = catalog
        .champions
        .iter()
        .map(|champion| champion.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    request
        .blue_bans
        .iter()
        .chain(&request.red_bans)
        .chain(&request.blue_picks)
        .chain(&request.red_picks)
        .chain(&request.history_blue)
        .chain(&request.history_red)
        .filter_map(|id| {
            if known.contains(id.as_str()) || !seen.insert(id.as_str()) {
                return None;
            }
            Some(DraftChampion {
                id: id.clone(),
                name: crate::statistics::humanize_id(id),
                portrait: None,
                role_fit: BTreeMap::new(),
            })
        })
        .collect()
}

#[derive(Clone)]
struct LineupModel {
    projection: TeamProjection,
    champion_indices: BTreeMap<String, usize>,
    marginals: Vec<[f64; 5]>,
    pair_marginals: Vec<Vec<[[f64; 5]; 5]>>,
    role_claims: BTreeMap<String, [f64; 5]>,
    primary_roles: BTreeMap<String, String>,
}

fn model_lineup(
    picks: &[String],
    champions: &BTreeMap<&str, &DraftChampion>,
    rows: &BTreeMap<&str, Vec<&ChampionRoleStat>>,
    forced_roles: &BTreeMap<String, usize>,
) -> LineupModel {
    if picks.is_empty() {
        return LineupModel {
            projection: TeamProjection {
                assignments_considered: 0,
                confidence: 0.0,
                champions: Vec::new(),
            },
            champion_indices: BTreeMap::new(),
            marginals: Vec::new(),
            pair_marginals: Vec::new(),
            role_claims: BTreeMap::new(),
            primary_roles: BTreeMap::new(),
        };
    }
    let evidence = picks
        .iter()
        .filter_map(|id| {
            let champion = champions.get(id.as_str()).copied()?;
            // A user-confirmed role collapses this champion's role distribution to
            // a certainty, so the assignment search places it exactly there.
            let likelihoods = match forced_roles.get(id.as_str()) {
                Some(&role_index) => {
                    let mut one_hot = [0.0; 5];
                    one_hot[role_index] = 1.0;
                    one_hot
                }
                None => role_likelihoods(champion, rows.get(id.as_str())),
            };
            Some((champion, likelihoods))
        })
        .collect::<Vec<_>>();
    let mut assignments = Vec::new();
    enumerate_assignments(&evidence, 0, &mut Vec::new(), &mut assignments);
    let total_weight = assignments.iter().map(|(_, weight)| weight).sum::<f64>();
    let primary_assignment = assignments
        .iter()
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(assignment, _)| assignment.as_slice());
    let mut marginals = vec![[0.0f64; 5]; evidence.len()];
    let mut pair_marginals = vec![vec![[[0.0f64; 5]; 5]; evidence.len()]; evidence.len()];
    if total_weight > 0.0 {
        for (assignment, weight) in &assignments {
            let probability = weight / total_weight;
            for (champion_index, role_index) in assignment.iter().enumerate() {
                marginals[champion_index][*role_index] += probability;
                for (other_index, other_role_index) in assignment.iter().enumerate() {
                    pair_marginals[champion_index][other_index][*role_index][*other_role_index] +=
                        probability;
                }
            }
        }
    }
    let confidence = marginals
        .iter()
        .map(|probabilities| probabilities.iter().copied().fold(0.0, f64::max))
        .sum::<f64>()
        / marginals.len() as f64;
    let projected = evidence
        .iter()
        .enumerate()
        .map(|(index, (champion, _))| {
            let champion_rows = rows.get(champion.id.as_str());
            let total_role_games = total_role_games(champion_rows);
            let forced_role = forced_roles.get(champion.id.as_str()).copied();
            let mut probabilities = ROLES
                .iter()
                .enumerate()
                .map(|(role_index, role)| {
                    let probability = marginals[index][role_index];
                    let selected = primary_assignment
                        .is_some_and(|assignment| assignment[index] == role_index);
                    RoleProbability {
                        role: (*role).to_string(),
                        probability,
                        // A user-confirmed role is treated as credible outright;
                        // otherwise it must clear the normal evidence bar.
                        assigned: selected
                            && (forced_role == Some(role_index)
                                || role_is_credible(
                                    probability,
                                    role_games(champion_rows, role),
                                    total_role_games,
                                )),
                    }
                })
                .filter(|row| row.probability >= 0.01)
                .collect::<Vec<_>>();
            probabilities.sort_by(|left, right| {
                right
                    .assigned
                    .cmp(&left.assigned)
                    .then_with(|| right.probability.total_cmp(&left.probability))
            });
            ChampionRoleProjection {
                champion_id: champion.id.clone(),
                champion_name: champion.name.clone(),
                portrait: champion.portrait.clone(),
                roles: probabilities,
            }
        })
        .collect();
    let champion_indices = evidence
        .iter()
        .enumerate()
        .map(|(index, (champion, _))| (champion.id.clone(), index))
        .collect();
    let role_claims = evidence
        .iter()
        .map(|(champion, claims)| (champion.id.clone(), *claims))
        .collect();
    let primary_roles = primary_assignment
        .map(|assignment| {
            assignment
                .iter()
                .enumerate()
                .map(|(index, role)| (evidence[index].0.id.clone(), ROLES[*role].to_string()))
                .collect()
        })
        .unwrap_or_default();
    LineupModel {
        projection: TeamProjection {
            assignments_considered: assignments.len(),
            confidence,
            champions: projected,
        },
        champion_indices,
        marginals,
        pair_marginals,
        role_claims,
        primary_roles,
    }
}

/// Map a role name to its index in `ROLES`, if valid. Accepts "bottom" as an
/// alias for "bot" so UI/lineup naming variants both resolve.
fn role_to_index(role: &str) -> Option<usize> {
    let role = if role == "bottom" { "bot" } else { role };
    ROLES.iter().position(|candidate| *candidate == role)
}

/// Build the champion -> forced-role-index map from a request's role overrides,
/// silently dropping any entry whose role name is not recognized.
fn forced_role_indices(request: &RecommendationRequest) -> BTreeMap<String, usize> {
    request
        .role_overrides
        .iter()
        .filter_map(|(champion, role)| Some((champion.clone(), role_to_index(role)?)))
        .collect()
}

fn role_is_credible(probability: f64, games: usize, total_history_games: usize) -> bool {
    probability >= ROLE_CREDIBILITY_MIN_PROBABILITY
        && (games >= ROLE_CREDIBILITY_MIN_GAMES
            || (total_history_games == 0 && probability >= NEW_CHAMPION_ROLE_MIN_PROBABILITY))
}

fn role_games(rows: Option<&Vec<&ChampionRoleStat>>, role: &str) -> usize {
    rows.and_then(|rows| rows.iter().find(|row| row.role == role))
        .map(|row| row.games)
        .unwrap_or_default()
}

fn total_role_games(rows: Option<&Vec<&ChampionRoleStat>>) -> usize {
    rows.map(|rows| rows.iter().map(|row| row.games).sum())
        .unwrap_or_default()
}

fn primary_role_projection<'a>(
    model: &'a LineupModel,
    champion_id: &str,
) -> Option<(&'a str, f64, bool)> {
    let role = model.primary_roles.get(champion_id)?;
    let row = model
        .projection
        .champions
        .iter()
        .find(|champion| champion.champion_id == champion_id)?
        .roles
        .iter()
        .find(|candidate| candidate.role == *role)?;
    Some((role.as_str(), row.probability, row.assigned))
}

fn credible_flexibility(champion: &DraftChampion, roles: &[&ChampionRoleStat]) -> usize {
    let claims = role_likelihoods(champion, Some(&roles.to_vec()));
    let total_games = roles.iter().map(|row| row.games).sum();
    ROLES
        .iter()
        .enumerate()
        .filter(|(index, role)| {
            let row = roles.iter().find(|row| row.role == **role);
            role_is_credible(
                claims[*index],
                row.map(|row| row.games).unwrap_or_default(),
                total_games,
            ) && row
                .map(|row| row.adjusted_win_rate >= 0.48)
                .unwrap_or(total_games == 0)
        })
        .count()
}

/// Outcome of the role-collision check. `penalty` is subtracted from the pick
/// score; `locked_role` is set only when the heavy penalty fired (the champion's
/// sole viable role is already covered), so the caller can show a clear reason.
struct RoleCollision {
    penalty: f64,
    locked_role: Option<String>,
}

fn role_collision_penalty(
    champion: &DraftChampion,
    roles: &[&ChampionRoleStat],
    candidate_model: &LineupModel,
    covered_roles: &BTreeSet<String>,
    own_picks: &[String],
) -> RoleCollision {
    let none = RoleCollision {
        penalty: 0.0,
        locked_role: None,
    };
    let Some(candidate_claims) = candidate_model.role_claims.get(&champion.id) else {
        return none;
    };
    let Some((best_role_index, candidate_probability)) = candidate_claims
        .iter()
        .copied()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(&right.1))
    else {
        return none;
    };
    let existing_pressure = own_picks
        .iter()
        .filter_map(|pick| candidate_model.role_claims.get(pick))
        .map(|claims| claims[best_role_index])
        .sum::<f64>();
    let total_games = roles.iter().map(|row| row.games).sum();
    // A "viable" alternative is a role the champion could actually be moved to:
    // credible role evidence AND a non-losing win rate there (the same bar
    // `credible_flexibility` uses). A few off-role games at a poor win rate do
    // not count, so fake flexibility can't rescue an otherwise single-role
    // champion from the locked penalty below.
    let has_viable_alternative = ROLES.iter().enumerate().any(|(index, role)| {
        if index == best_role_index {
            return false;
        }
        let row = roles.iter().find(|row| row.role == *role);
        role_is_credible(
            candidate_claims[index],
            row.map(|row| row.games).unwrap_or_default(),
            total_games,
        ) && row
            .map(|row| row.adjusted_win_rate >= 0.48)
            .unwrap_or(total_games == 0)
    });
    let best_role = ROLES[best_role_index];
    // Heavy "locked" penalty: the champion's dominant role is already credibly
    // filled by an existing ally and it has no other viable role to slide into,
    // so recommending it would just stack a second champion on a taken role.
    // Drop it out of the shortlist instead of applying the small graded fine.
    // The probability floor keeps a thin/unknown champion (flat role spread)
    // from being locked out on a weak best-role guess.
    if !has_viable_alternative
        && candidate_probability >= ROLE_COLLISION_LOCKED_MIN_PROBABILITY
        && covered_roles.contains(best_role)
    {
        return RoleCollision {
            penalty: ROLE_COLLISION_LOCKED_PENALTY,
            locked_role: Some(best_role.to_string()),
        };
    }
    let flex_relief = if has_viable_alternative { 0.5 } else { 1.0 };
    RoleCollision {
        penalty: (ROLE_COLLISION_SCORE_WEIGHT
            * candidate_probability
            * existing_pressure
            * flex_relief)
            .clamp(0.0, ROLE_COLLISION_SCORE_CLAMP),
        locked_role: None,
    }
}

#[cfg(test)]
fn project_lineup(
    picks: &[String],
    champions: &BTreeMap<&str, &DraftChampion>,
    rows: &BTreeMap<&str, Vec<&ChampionRoleStat>>,
) -> TeamProjection {
    model_lineup(picks, champions, rows, &BTreeMap::new()).projection
}

fn role_likelihoods(champion: &DraftChampion, rows: Option<&Vec<&ChampionRoleStat>>) -> [f64; 5] {
    const STATIC_PRIOR_GAMES: f64 = 8.0;
    let total_games = rows
        .map(|rows| rows.iter().map(|row| row.games).sum::<usize>())
        .unwrap_or_default() as f64;
    let static_total = ROLES
        .iter()
        .map(|role| champion.role_fit.get(*role).copied().unwrap_or(0.0))
        .sum::<f64>();
    let mut result = [0.0; 5];
    for (index, role) in ROLES.iter().enumerate() {
        let row = rows.and_then(|rows| rows.iter().find(|row| row.role == *role));
        let games = row.map(|row| row.games as f64).unwrap_or(0.0);
        let static_probability = if static_total > 0.0 {
            champion.role_fit.get(*role).copied().unwrap_or(0.0) / static_total
        } else {
            0.2
        };
        let frequency =
            (games + STATIC_PRIOR_GAMES * static_probability) / (total_games + STATIC_PRIOR_GAMES);
        let performance = row
            .map(|row| 0.75 + 0.5 * row.adjusted_win_rate)
            .unwrap_or(1.0);
        result[index] = (frequency * performance).max(0.001);
    }
    let total = result.iter().sum::<f64>();
    for value in &mut result {
        *value /= total;
    }
    result
}

fn enumerate_assignments(
    evidence: &[(&DraftChampion, [f64; 5])],
    champion_index: usize,
    current: &mut Vec<usize>,
    assignments: &mut Vec<(Vec<usize>, f64)>,
) {
    if champion_index == evidence.len() {
        let weight = current
            .iter()
            .enumerate()
            .map(|(index, role)| evidence[index].1[*role])
            .product::<f64>();
        assignments.push((current.clone(), weight));
        return;
    }
    for role_index in 0..ROLES.len() {
        if current.contains(&role_index) {
            continue;
        }
        current.push(role_index);
        enumerate_assignments(evidence, champion_index + 1, current, assignments);
        current.pop();
    }
}

fn score_pick(
    champion: &DraftChampion,
    overall: &ChampionRoleStat,
    roles: &[&ChampionRoleStat],
    flexibility: usize,
    covered_roles: &BTreeSet<String>,
    projected_role: Option<(&str, f64, bool)>,
    candidate_model: &LineupModel,
    enemy_model: &LineupModel,
    own_picks: &[String],
    enemy_picks: &[String],
    role_rows: &BTreeMap<&str, Vec<&ChampionRoleStat>>,
    interactions: &InteractionEvidence,
    request: &RecommendationRequest,
    rating_baselines: &BTreeMap<String, RatingBaseline>,
    draft_presence: &BTreeMap<String, f64>,
    manual_tier: Option<ManualTier>,
) -> Recommendation {
    let assigned_role_credible = projected_role
        .map(|(_, _, credible)| credible)
        .unwrap_or(false);
    let best_role = projected_role
        .filter(|(_, _, credible)| *credible)
        .and_then(|(role, _, _)| roles.iter().find(|row| row.role == role).copied())
        .or_else(|| {
            let claims = candidate_model.role_claims.get(&champion.id)?;
            let total_games = roles.iter().map(|row| row.games).sum();
            roles
                .iter()
                .filter(|row| {
                    ROLES
                        .iter()
                        .position(|role| *role == row.role)
                        .is_some_and(|role_index| {
                            role_is_credible(claims[role_index], row.games, total_games)
                        })
                })
                .max_by(|left, right| {
                    pick_role_value(left, covered_roles)
                        .total_cmp(&pick_role_value(right, covered_roles))
                })
                .copied()
        });
    // Risk-adjusted win rates (uncertainty baked in) for role and overall.
    let t = &request.tuning;
    let overall_risk_wr = risk_adjusted_win_rate(
        overall.adjusted_win_rate,
        overall.wins,
        overall.games,
        t.win_rate_risk_z,
        t.win_rate_prior_games,
    );
    let role_value = best_role
        .map(|row| {
            risk_adjusted_win_rate(
                row.adjusted_win_rate,
                row.wins,
                row.games,
                t.win_rate_risk_z,
                t.win_rate_prior_games,
            )
        })
        .unwrap_or(overall_risk_wr * 0.9);
    let coverage_value = best_role
        .filter(|row| assigned_role_credible && !covered_roles.contains(&row.role))
        .map(|_| 1.0)
        .unwrap_or(0.0);
    let rating_value = best_role
        .map(|row| rating_strength(row.avg_rating, row.games, rating_baselines.get(&row.role)))
        .unwrap_or(0.5);
    // Win rate (role + overall, risk-adjusted so low-sample is damped) carries
    // meta/team strength; rating adds the game's role-aware individual read.
    // A manual tier flag (if any) nudges the strength signal as a soft prior.
    let manual_shift = manual_tier
        .map(ManualTier::performance_shift)
        .unwrap_or(0.0);
    let performance_value = (0.55 * role_value
        + 0.25 * overall_risk_wr
        + 0.20 * rating_value
        + crate::patch::patch_performance_shift(
            overall,
            t.patch_max_shift,
            t.patch_impact_scale,
            t.patch_evidence_games,
        )
        + manual_shift)
        .clamp(0.0, 1.0);
    let flexibility_value = if assigned_role_credible {
        (0.65 * (flexibility.min(4) as f64 / 4.0) + 0.35 * coverage_value).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let role_prior = |champion_id: &str, role: &str| {
        role_rows
            .get(champion_id)
            .and_then(|rows| rows.iter().find(|row| row.role == role))
            .map(|row| row.adjusted_win_rate)
            .unwrap_or(0.5)
    };
    let interaction = expected_interactions(
        &champion.id,
        candidate_model,
        enemy_model,
        own_picks,
        enemy_picks,
        interactions,
        request.minimum_interaction_games,
        role_prior,
    );
    // Stage 3 tune: blend the team-average synergy with the single best ally
    // pairing, so one excellent partner isn't averaged into neutrality. The best
    // pair's pull scales with how well-sampled it is, so a high-delta but
    // low-sample pairing barely moves the signal while a deep one counts fully.
    let (best_delta, best_weight) = interaction
        .synergy_pairs
        .first()
        .map(|pair| {
            let confidence = pair.games as f64 / (pair.games as f64 + SYNERGY_CONFIDENCE_GAMES);
            (pair.delta, 0.5 * confidence)
        })
        .unwrap_or((0.0, 0.0));
    let synergy_signal = interaction.synergy_delta * (1.0 - best_weight) + best_delta * best_weight;
    let synergy_value = (0.5 + synergy_signal * 3.0).clamp(0.0, 1.0);
    // Wider multiplier than synergy: matchup data is sparser and clusters near
    // 50%, so a 3x scale left the dimension nearly flat even at high weight.
    let matchup_value = (0.5 + interaction.matchup_delta * 5.0).clamp(0.0, 1.0);
    let blind_pick = enemy_picks.is_empty();
    // How contested this champion is in drafts (pick/ban frequency), as its own
    // tunable dimension so it never inflates the pure win-rate strength.
    let draft_presence_value = draft_presence.get(&champion.id).copied().unwrap_or(0.5);
    let weights = normalized_weights(&request.weights);
    let base_score = 100.0
        * (weights.performance * performance_value
            + weights.synergy * synergy_value
            + weights.matchup * matchup_value
            + weights.flexibility * flexibility_value
            + weights.draft_presence * draft_presence_value);
    let collision = role_collision_penalty(champion, roles, candidate_model, covered_roles, own_picks);
    let collision_penalty = collision.penalty;
    let forced_assignment_penalty = if assigned_role_credible {
        0.0
    } else {
        FORCED_OFF_ROLE_SCORE_PENALTY
    };
    let score = (base_score - collision_penalty - forced_assignment_penalty).max(0.0);
    // Reasons are ordered most-actionable first (the picker shows the top few),
    // and trimmed of redundant/boilerplate lines.
    let mut reasons = Vec::new();
    // Surface a manual tier flag first so it's clear the score was hand-adjusted.
    if let Some(tier) = manual_tier {
        let direction = if tier.performance_shift() >= 0.0 {
            "boosted"
        } else {
            "lowered"
        };
        reasons.push(Reason::from_delta(
            format!("Manually flagged {}-tier ({direction})", tier.code()),
            tier.performance_shift(),
        ).translated(
            if direction == "boosted" { "recommendation.reason.manualTierBoosted" } else { "recommendation.reason.manualTierLowered" },
            [("tier", tier.code().to_string())],
        ));
    }
    // Core role performance (or overall when the champion has no role data yet).
    if let Some(role) = best_role {
        reasons.push(Reason::from_delta(
            format!(
                "{}: {:.1}% adjusted win rate over {} games",
                title_role(&role.role),
                role.adjusted_win_rate * 100.0,
                role.games
            ),
            role.adjusted_win_rate - 0.5,
        ).translated("recommendation.reason.roleWinRate", [
            ("role", title_role(&role.role)),
            ("winRate", format!("{:.1}", role.adjusted_win_rate * 100.0)),
            ("games", role.games.to_string()),
        ]).translated_roles([("role", role.role.clone())]));
    } else {
        reasons.push(Reason::from_delta(
            format!(
                "{:.1}% adjusted win rate over {} games",
                overall.adjusted_win_rate * 100.0,
                overall.games
            ),
            overall.adjusted_win_rate - 0.5,
        ).translated("recommendation.reason.adjustedWinRate", [
            ("winRate", format!("{:.1}", overall.adjusted_win_rate * 100.0)),
            ("games", overall.games.to_string()),
        ]));
    }
    // Concrete synergy evidence: the best-sampled ally pairings, named, with the
    // pair win rate and how many games back it (up to two so it stays readable).
    for pair in interaction
        .synergy_pairs
        .iter()
        .filter(|pair| pair.delta >= 0.02)
        .take(2)
    {
        reasons.push(Reason::positive(format!(
            "Pairs with {}: {:.1}% together over {} games ({:+.1}%)",
            crate::statistics::humanize_id(&pair.other),
            pair.win_rate * 100.0,
            pair.games,
            pair.delta * 100.0
        )).translated("recommendation.reason.pairsWith", [
            ("champion", crate::statistics::humanize_id(&pair.other)),
            ("winRate", format!("{:.1}", pair.win_rate * 100.0)),
            ("games", pair.games.to_string()),
            ("delta", format!("{:+.1}", pair.delta * 100.0)),
        ]).translated_champions([("champion", pair.other.clone())]));
    }
    // Named counter-pick evidence: the strongest lane clashes (either way)
    // against specific enemy picks, so it's clear who the matchup is about.
    let mut named_matchups = interaction.matchup_pairs.iter().collect::<Vec<_>>();
    named_matchups.sort_by(|left, right| right.delta.abs().total_cmp(&left.delta.abs()));
    for matchup in named_matchups
        .into_iter()
        .filter(|m| m.delta.abs() >= 0.02)
        .take(2)
    {
        let verb = if matchup.delta >= 0.0 {
            "Strong into"
        } else {
            "Weak into"
        };
        reasons.push(Reason::from_delta(
            format!(
                "{verb} {} ({}): {:.1}% over {} games ({:+.1}%)",
                crate::statistics::humanize_id(&matchup.enemy),
                title_role(&matchup.role),
                matchup.win_rate * 100.0,
                matchup.games,
                matchup.delta * 100.0
            ),
            matchup.delta,
        ).translated(
            if matchup.delta >= 0.0 { "recommendation.reason.strongInto" } else { "recommendation.reason.weakInto" },
            [
                ("champion", crate::statistics::humanize_id(&matchup.enemy)),
                ("role", title_role(&matchup.role)),
                ("winRate", format!("{:.1}", matchup.win_rate * 100.0)),
                ("games", matchup.games.to_string()),
                ("delta", format!("{:+.1}", matchup.delta * 100.0)),
            ],
        ).translated_champions([("champion", matchup.enemy.clone())])
            .translated_roles([("role", matchup.role.clone())]));
    }
    if let Some(role) = best_role {
        if let (Some(rating), Some(baseline)) = (role.avg_rating, rating_baselines.get(&role.role))
        {
            if role.games >= 10 && (rating - baseline.mean).abs() >= baseline.std {
                let comparison = if rating >= baseline.mean {
                    "above"
                } else {
                    "below"
                };
                reasons.push(Reason::from_delta(
                    format!(
                        "Match rating {comparison} par for {} ({:.0} vs {:.0} avg)",
                        title_role(&role.role),
                        rating,
                        baseline.mean
                    ),
                    rating - baseline.mean,
                ).translated(
                    if rating >= baseline.mean { "recommendation.reason.ratingAbovePar" } else { "recommendation.reason.ratingBelowPar" },
                    [
                        ("role", title_role(&role.role)),
                        ("rating", format!("{rating:.0}")),
                        ("average", format!("{:.0}", baseline.mean)),
                    ],
                ).translated_roles([("role", role.role.clone())]));
            }
        }
    }
    if let Some(role) = best_role {
        if assigned_role_credible && !covered_roles.contains(&role.role) {
            reasons.push(Reason::positive(format!(
                "Adds uncovered {} role",
                title_role(&role.role)
            )).translated("recommendation.reason.addsUncoveredRole", [
                ("role", title_role(&role.role)),
            ]).translated_roles([("role", role.role.clone())]));
        }
    }
    if assigned_role_credible && flexibility >= 2 {
        reasons.push(Reason::positive(format!("Viable in {flexibility} roles"))
            .translated("recommendation.reason.viableRoles", [("count", flexibility.to_string())]));
    }
    if !assigned_role_credible {
        reasons.push(Reason::negative(
            "Role assignment is not evidence-backed; no coverage credit applied",
        ).translated("recommendation.reason.roleAssignmentUnproven", [] as [(&str, String); 0]));
    }
    if let Some(role) = &collision.locked_role {
        reasons.push(Reason::negative(format!(
            "{} is already covered and this champion has no other viable role (-{collision_penalty:.1} score)",
            title_role(role)
        )).translated("recommendation.reason.roleLocked", [
            ("role", title_role(role)),
            ("penalty", format!("{collision_penalty:.1}")),
        ]).translated_roles([("role", role.clone())]));
    } else if collision_penalty >= ROLE_COLLISION_REASON_THRESHOLD {
        reasons.push(Reason::negative(format!(
            "Competes with current picks for the same primary role (-{collision_penalty:.1} score)"
        )).translated("recommendation.reason.roleCollision", [
            ("penalty", format!("{collision_penalty:.1}")),
        ]));
    }
    if draft_presence_value >= 0.8 {
        reasons.push(Reason::neutral(
            "Highly contested in drafts (frequently picked or banned)",
        ).translated("recommendation.reason.highlyContested", [] as [(&str, String); 0]));
    }
    // Only surface patch evidence when the champion was actually changed this
    // patch; the "unchanged, full history" case is constant noise.
    if overall.patch_changed || overall.patch_added {
        reasons.push(translated_patch_reason(overall));
    }
    if interaction.matchup_games == 0 && blind_pick {
        reasons.push(Reason::neutral(
            "Blind-pick phase favors flexible role coverage",
        ).translated("recommendation.reason.blindPickFlexibility", [] as [(&str, String); 0]));
    }
    Recommendation {
        champion_id: champion.id.clone(),
        champion_name: champion.name.clone(),
        portrait: champion.portrait.clone(),
        score,
        suggested_role: if assigned_role_credible {
            best_role.map(|row| row.role.clone())
        } else {
            None
        },
        adjusted_win_rate: overall.adjusted_win_rate,
        role_win_rate: best_role.map(|row| row.adjusted_win_rate),
        games: overall.games,
        confidence: overall.confidence,
        flexibility,
        synergy_score: interaction.synergy_delta,
        matchup_score: interaction.matchup_delta,
        interaction_games: interaction.synergy_games.max(interaction.matchup_games),
        reasons,
        athlete_context: None,
        ban_score_components: None,
    }
}

fn score_ban(
    champion: &DraftChampion,
    overall: &ChampionRoleStat,
    roles: &[&ChampionRoleStat],
    flexibility: usize,
    rating_baselines: &BTreeMap<String, RatingBaseline>,
    overall_rows: &BTreeMap<&str, &ChampionRoleStat>,
    interactions: &InteractionEvidence,
    draft_context: &BanDraftContext,
    request: &RecommendationRequest,
    pick_score: Option<f64>,
    first_pick: &FirstPickContext,
    own_pick_score: Option<f64>,
    own_claim: &FirstPickContext,
    draft_presence: &BTreeMap<String, f64>,
    portfolio_adjustment: f64,
    portfolio_claim: Option<(String, String)>,
    portfolio_leftover: Option<(String, String)>,
    portfolio_leftover_survival: Option<f64>,
    redundant_ban_discount: f64,
    manual_tier: Option<ManualTier>,
) -> Recommendation {
    let best_role = roles
        .iter()
        .filter(|row| row.games >= 5)
        .max_by(|left, right| left.adjusted_win_rate.total_cmp(&right.adjusted_win_rate))
        .copied();
    // Sample depth enters the threat read exactly once, through the same
    // risk-adjusted win rate the pick side uses: a thin sample is pulled toward
    // neutral instead of being rewarded again by a standalone confidence term
    // (which favored heavily-played champions regardless of strength) and a
    // confidence-multiplied peak (which made an unproven 55% role read like a
    // catastrophic ~27% one).
    let t = &request.tuning;
    let overall_risk_wr = risk_adjusted_win_rate(
        overall.adjusted_win_rate,
        overall.wins,
        overall.games,
        t.win_rate_risk_z,
        t.win_rate_prior_games,
    );
    let peak = best_role
        .map(|row| {
            risk_adjusted_win_rate(
                row.adjusted_win_rate,
                row.wins,
                row.games,
                t.win_rate_risk_z,
                t.win_rate_prior_games,
            )
        })
        .unwrap_or(overall_risk_wr);
    let rating_value = best_role
        .map(|row| rating_strength(row.avg_rating, row.games, rating_baselines.get(&row.role)))
        .unwrap_or(0.5);
    let flexibility_bonus = (flexibility.min(4) as f64) * 0.02;
    // A manual tier flag shifts how threatening the champion is judged, the same
    // soft prior used on the pick side: an S champ is a bigger ban target, an
    // F/D champ a smaller one. Same magnitude as performance (±~0.12 at S/D).
    let manual_shift = manual_tier
        .map(ManualTier::performance_shift)
        .unwrap_or(0.0);
    // A champion that performs above par (rating) is a bigger threat to leave open.
    let threat_score = 100.0
        * (0.55 * overall_risk_wr
            + 0.20 * rating_value
            + 0.25 * peak
            + flexibility_bonus
            + crate::patch::patch_performance_shift(
                overall,
                t.patch_max_shift,
                t.patch_impact_scale,
                t.patch_evidence_games,
            )
            + manual_shift);
    let counterability = global_counterability(
        &champion.id,
        overall.adjusted_win_rate,
        overall_rows,
        interactions,
        request.minimum_interaction_games,
    );
    let synergy_hub = global_synergy_hub(
        &champion.id,
        overall.adjusted_win_rate,
        overall_rows,
        interactions,
        request.minimum_interaction_games,
    );
    // Unknown interaction evidence stays neutral. Strong answers reduce the ban
    // score; a reliably poor answer pool raises it.
    let counterability_adjustment = 12.0 * (0.5 - counterability.value);
    let synergy_adjustment = 30.0 * synergy_hub.value;
    let draft_context_adjustment = ban_draft_context_adjustment(draft_context);
    let first_pick_denial = first_pick_denial_adjustment(
        &champion.id,
        pick_score,
        first_pick,
        request,
        draft_presence,
    );
    let own_claim_discount = if request.side == "blue" {
        own_claim_discount(pick_score, first_pick)
    } else {
        red_own_claim_discount(own_pick_score, own_claim, pick_score, first_pick)
    };
    let base_score = threat_score
        + counterability_adjustment
        + synergy_adjustment
        + draft_context_adjustment
        + first_pick_denial;
    let score =
        (base_score - own_claim_discount + portfolio_adjustment - redundant_ban_discount).max(0.0);
    // For a ban, a stronger threat is a stronger reason to ban it, so high
    // win rate reads as "positive" (argues for the recommended action).
    let mut reasons = vec![Reason::from_delta(
        format!(
            "{:.1}% patch-weighted win rate over {} total games",
            overall.adjusted_win_rate * 100.0,
            overall.games
        ),
        overall.adjusted_win_rate - 0.5,
    ).translated("recommendation.reason.patchWeightedWinRate", [
        ("winRate", format!("{:.1}", overall.adjusted_win_rate * 100.0)),
        ("games", overall.games.to_string()),
    ])];
    if let Some(tier) = manual_tier {
        let direction = if tier.performance_shift() >= 0.0 {
            "raised"
        } else {
            "lowered"
        };
        reasons.push(Reason::from_delta(
            format!("Manually flagged {}-tier (ban priority {direction})", tier.code()),
            tier.performance_shift(),
        ).translated(
            if direction == "raised" { "recommendation.reason.manualBanTierRaised" } else { "recommendation.reason.manualBanTierLowered" },
            [("tier", tier.code().to_string())],
        ));
    }
    if overall.patch_changed || overall.patch_added {
        reasons.push(translated_patch_reason(overall));
    }
    if let Some(role) = best_role {
        reasons.push(Reason::positive(format!(
            "Peak threat: {} at {:.1}%",
            title_role(&role.role),
            role.adjusted_win_rate * 100.0
        )).translated("recommendation.reason.peakThreat", [
            ("role", title_role(&role.role)),
            ("winRate", format!("{:.1}", role.adjusted_win_rate * 100.0)),
        ]).translated_roles([("role", role.role.clone())]));
    }
    if flexibility >= 2 {
        reasons.push(Reason::positive(format!("Flexible across {flexibility} roles"))
            .translated("recommendation.reason.flexibleRoles", [("count", flexibility.to_string())]));
    }
    if counterability.games > 0 {
        if counterability.value <= 0.46 {
            reasons.push(Reason::positive(format!(
                "Hard to answer: best reliable counters average {:.1}% over {} games",
                counterability.value * 100.0,
                counterability.games
            )).translated("recommendation.reason.hardToAnswer", [
                ("winRate", format!("{:.1}", counterability.value * 100.0)),
                ("games", counterability.games.to_string()),
            ]));
        }
    }
    if synergy_hub.value >= 0.025 {
        reasons.push(Reason::positive(format!(
            "Enables multiple strong pairings ({:+.1}% synergy lift)",
            synergy_hub.value * 100.0
        )).translated("recommendation.reason.strongPairings", [
            ("delta", format!("{:+.1}", synergy_hub.value * 100.0)),
        ]));
    }
    let opponent_side = if request.side == "red" { "Blue" } else { "Red" };
    let own_side = if request.side == "red" { "Red" } else { "Blue" };
    if draft_context.synergy_games > 0
        && draft_context.synergy_delta.abs() >= BAN_CONTEXT_REASON_THRESHOLD
    {
        reasons.push(Reason::neutral(format!(
            "{opponent_side}'s current picks change its expected synergy by {:+.1}% (up to {} role-pair games)",
            draft_context.synergy_delta * 100.0,
            draft_context.synergy_games
        )).translated("recommendation.reason.currentPicksSynergy", [
            ("side", opponent_side.to_string()),
            ("delta", format!("{:+.1}", draft_context.synergy_delta * 100.0)),
            ("games", draft_context.synergy_games.to_string()),
        ]));
    }
    if draft_context.matchup_games > 0
        && draft_context.matchup_delta.abs() >= BAN_CONTEXT_REASON_THRESHOLD
    {
        reasons.push(Reason::neutral(format!(
            "Expected matchup into {own_side}'s current picks is {:+.1}% (up to {} role-pair games)",
            draft_context.matchup_delta * 100.0,
            draft_context.matchup_games
        )).translated("recommendation.reason.expectedMatchup", [
            ("side", own_side.to_string()),
            ("delta", format!("{:+.1}", draft_context.matchup_delta * 100.0)),
            ("games", draft_context.matchup_games.to_string()),
        ]));
    }
    if first_pick_denial >= 3.0 {
        reasons.push(Reason::positive(
            "High-value Blue first pick; denying it is valuable",
        ).translated("recommendation.reason.denyBlueFirstPick", [] as [(&str, String); 0]));
    }
    if own_claim_discount >= 3.0 {
        reasons.push(Reason::negative(
            "Also one of your own strongest picks; banning it would waste your claim",
        ).translated("recommendation.reason.ownClaimOverlap", [] as [(&str, String); 0]));
    }
    if portfolio_adjustment >= PORTFOLIO_ADJUSTMENT_REASON_THRESHOLD {
        let label = portfolio_leftover_survival.map(survival_label);
        match (portfolio_claim, portfolio_leftover, label) {
            (Some((claim_id, claim)), Some((leftover_id, leftover)), Some((label, label_key))) => reasons.push(Reason::neutral(format!(
                "Blue can only claim one strong open pick; banning this leaves {claim} as Blue's likely survivor while {leftover} remains {label}"
            )).translated("recommendation.reason.blueClaimWithLeftover", [
                ("claim", claim.to_string()),
                ("leftover", leftover.to_string()),
                ("label", label.to_string()),
            ]).translated_champions([
                ("claim", claim_id),
                ("leftover", leftover_id),
            ]).translated_phrases([("label", label_key)])),
            (Some((claim_id, claim)), _, _) => reasons.push(Reason::neutral(format!(
                "Blue can only claim one strong open pick; banning this leaves {claim} as Blue's likely survivor"
            )).translated("recommendation.reason.blueClaim", [("claim", claim.to_string())])
                .translated_champions([("claim", claim_id)])),
            _ => {}
        }
    }
    Recommendation {
        champion_id: champion.id.clone(),
        champion_name: champion.name.clone(),
        portrait: champion.portrait.clone(),
        score,
        suggested_role: best_role.map(|row| row.role.clone()),
        adjusted_win_rate: overall.adjusted_win_rate,
        role_win_rate: best_role.map(|row| row.adjusted_win_rate),
        games: overall.games,
        confidence: overall.confidence,
        flexibility,
        synergy_score: synergy_hub.value,
        matchup_score: counterability.value - 0.5,
        interaction_games: synergy_hub.games.max(counterability.games),
        reasons,
        athlete_context: None,
        ban_score_components: Some(BanScoreComponents {
            base_score,
            own_claim_discount,
            portfolio_adjustment,
            redundant_ban_discount,
            final_score: score,
        }),
    }
}

// Stage 4B: a champion in Blue's acceptable-pick pool, with the values needed
// to reason about "Blue can claim only one of these."
#[derive(Clone)]
struct PortfolioEntry {
    champion_id: String,
    champion_name: String,
    // Blue's own pick score for this champion (first-pick scenario).
    blue_pick_value: f64,
    // Red's ban score for this champion, used as the "pressure" signal for
    // how likely Red is to ban it before Blue's first pick.
    red_ban_score: f64,
    // Red's pick score for this champion, i.e. the value to Red if it
    // survives Blue's claim and Red picks it up instead.
    red_pick_value: f64,
}

// The outcome of "Blue claims its best surviving target, Red takes the best
// of what's left" for a given pool (the full pool, or the pool with one
// candidate removed).
struct PortfolioOutcome {
    value: f64,
    claim: Option<(String, String)>,
    leftover: Option<(String, String)>,
    leftover_survival: Option<f64>,
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let len = sorted.len();
    if len % 2 == 1 {
        sorted[len / 2]
    } else {
        (sorted[len / 2 - 1] + sorted[len / 2]) / 2.0
    }
}

// How likely a champion is to survive Red's remaining bans, expressed as a
// multiplier on Blue's pick value for it. `value` is the champion's own Red
// ban score; `pool_scores` are the Red ban scores across the candidates being
// compared. A champion that Red would clearly rather ban than its peers
// (above the pool median, by the pool's spread) survives less reliably. When
// the pool's scores are effectively tied, there's no relative signal, so the
// factor stays at the neutral baseline.
fn survival_factor(value: f64, pool_scores: &[f64]) -> f64 {
    if pool_scores.len() < 2 {
        return RED_PRESSURE_BASELINE;
    }
    let min = pool_scores.iter().copied().fold(f64::INFINITY, f64::min);
    let max = pool_scores
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let spread = max - min;
    if spread < 1e-6 {
        return RED_PRESSURE_BASELINE;
    }
    let pressure = ((value - median(pool_scores)) / spread).clamp(-1.0, 1.0);
    (RED_PRESSURE_BASELINE - RED_PRESSURE_SLOPE * pressure)
        .clamp(SURVIVAL_FACTOR_MIN, SURVIVAL_FACTOR_MAX)
}

// Qualitative framing of a survival factor for the UI. Never presented as a
// calibrated probability — it's a relative read on Red's ban ranking only.
// Returns (English text, translation key): the text backs the untranslated
// `Reason::text` fallback while the key lets the UI render the phrase in the
// active language. One function returns both so they cannot drift apart.
fn survival_label(factor: f64) -> (&'static str, &'static str) {
    if factor <= 0.45 {
        (
            "a likely Red ban target",
            "recommendation.survival.likelyBanTarget",
        )
    } else if factor < 0.75 {
        (
            "at moderate contest risk from Red",
            "recommendation.survival.moderateRisk",
        )
    } else {
        (
            "likely to survive Red's next ban",
            "recommendation.survival.likelySurvive",
        )
    }
}

// Conditional, mutually-exclusive outcome model for "Blue can claim only one
// of this tier." `tier` is the claim tier under consideration (the full tier,
// or the tier minus one ban candidate).
//
// We only branch on ONE candidate: `target`, the tier member Red is most
// pressured to ban (highest red_ban_score). Red pressure is used to weight
// which branch is more likely, not to independently discount every member:
//   - If Red bans `target` (probability 1 - survival(target)): it's removed
//     entirely, and Blue claims the best of what remains.
//   - If Red doesn't ban `target` (probability survival(target)): nothing is
//     removed, and Blue claims the best-valued member of the full tier —
//     which may or may not be `target` itself.
// In both branches the "claim" is always whichever candidate is actually
// highest-valued in that outcome, never an uncontested mediocre filler.
// Red's leftover is the best red_pick_value among whatever Blue didn't claim
// in that same branch.
fn portfolio_value(entries: &[&PortfolioEntry]) -> PortfolioOutcome {
    match entries.len() {
        0 => PortfolioOutcome {
            value: 0.0,
            claim: None,
            leftover: None,
            leftover_survival: None,
        },
        1 => {
            // No peer to compare against, so there's no claim/leftover split
            // to reason about — Blue either gets this one or nothing.
            let entry = entries[0];
            PortfolioOutcome {
                value: entry.blue_pick_value,
                claim: Some((entry.champion_id.clone(), entry.champion_name.clone())),
                leftover: None,
                leftover_survival: None,
            }
        }
        _ => {
            let red_ban_scores = entries
                .iter()
                .map(|entry| entry.red_ban_score)
                .collect::<Vec<_>>();
            let target_index = entries
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| left.red_ban_score.total_cmp(&right.red_ban_score))
                .map(|(index, _)| index)
                .expect("entries is non-empty");
            let target = entries[target_index];
            let rest = entries
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != target_index)
                .map(|(_, entry)| *entry)
                .collect::<Vec<_>>();
            let best_of_rest = rest
                .iter()
                .copied()
                .max_by(|left, right| left.blue_pick_value.total_cmp(&right.blue_pick_value));

            let p_survive = survival_factor(target.red_ban_score, &red_ban_scores);

            // If Red doesn't ban `target`, Blue claims whichever of `target`
            // and the rest of the tier is actually highest-valued.
            let claim_if_survives = match best_of_rest {
                Some(alternative) if alternative.blue_pick_value > target.blue_pick_value => {
                    alternative
                }
                _ => target,
            };
            // If Red bans `target`, Blue claims the best of what's left (may
            // be None if the tier only had two members).
            let claim_if_banned = best_of_rest;

            let leftover_if_survives = entries
                .iter()
                .copied()
                .filter(|entry| entry.champion_id != claim_if_survives.champion_id)
                .max_by(|left, right| left.red_pick_value.total_cmp(&right.red_pick_value));
            let leftover_if_banned = claim_if_banned.and_then(|claim| {
                rest.iter()
                    .copied()
                    .filter(|entry| entry.champion_id != claim.champion_id)
                    .max_by(|left, right| left.red_pick_value.total_cmp(&right.red_pick_value))
            });

            let claim_value = p_survive * claim_if_survives.blue_pick_value
                + (1.0 - p_survive)
                    * claim_if_banned
                        .map(|entry| entry.blue_pick_value)
                        .unwrap_or(0.0);
            let leftover_value = p_survive
                * leftover_if_survives
                    .map(|entry| entry.red_pick_value)
                    .unwrap_or(0.0)
                + (1.0 - p_survive)
                    * leftover_if_banned
                        .map(|entry| entry.red_pick_value)
                        .unwrap_or(0.0);

            PortfolioOutcome {
                value: claim_value - leftover_value,
                claim: Some((
                    claim_if_survives.champion_id.clone(),
                    claim_if_survives.champion_name.clone(),
                )),
                leftover: leftover_if_survives
                    .map(|entry| (entry.champion_id.clone(), entry.champion_name.clone())),
                leftover_survival: leftover_if_survives
                    .map(|entry| survival_factor(entry.red_ban_score, &red_ban_scores)),
            }
        }
    }
}

// For a given ban candidate, compare the "leave everything open" baseline
// against "this candidate is banned" and return the bounded score adjustment
// plus the resulting claim/leftover for reason text. Champions outside the
// pool (or pools too small to have a claim/leftover split) contribute 0.
fn portfolio_adjustment_for(
    pool: &[PortfolioEntry],
    baseline: &PortfolioOutcome,
    ban_candidate: &str,
) -> (f64, Option<(String, String)>, Option<(String, String)>, Option<f64>) {
    if pool.len() < 2 || !pool.iter().any(|entry| entry.champion_id == ban_candidate) {
        return (0.0, None, None, None);
    }
    let remaining = pool
        .iter()
        .filter(|entry| entry.champion_id != ban_candidate)
        .collect::<Vec<_>>();
    let outcome = portfolio_value(&remaining);
    let adjustment = (outcome.value - baseline.value)
        .clamp(-PORTFOLIO_ADJUSTMENT_CLAMP, PORTFOLIO_ADJUSTMENT_CLAMP);
    (
        adjustment,
        outcome.claim,
        outcome.leftover,
        outcome.leftover_survival,
    )
}

// Build Stage 4B's portfolio pool: the members of Blue's acceptable-pick pool
// (already identified via `first_pick`), each annotated with Red's ban score
// (survival pressure) and Red's pick score (what Red gains if it survives).
// Computed once per request and reused across all ban candidates. Side
// effects are deliberately none: the Red-perspective `score_ban`/`score_pick`
// calls below pass portfolio inputs as neutral (0.0/None) so this cannot
// recurse into portfolio evaluation itself.
fn build_portfolio_pool(
    champions: &BTreeMap<&str, &DraftChampion>,
    overall: &BTreeMap<&str, &ChampionRoleStat>,
    role_rows: &BTreeMap<&str, Vec<&ChampionRoleStat>>,
    rating_baselines: &BTreeMap<String, RatingBaseline>,
    interactions: &InteractionEvidence,
    request: &RecommendationRequest,
    draft_presence: &BTreeMap<String, f64>,
    pick_scores: &BTreeMap<&str, f64>,
    first_pick: &FirstPickContext,
    red_model: &LineupModel,
    blue_model: &LineupModel,
    manual_tiers: &BTreeMap<String, ManualTier>,
    forced_roles: &BTreeMap<String, usize>,
) -> Vec<PortfolioEntry> {
    if first_pick.pool_size < 2 {
        return Vec::new();
    }
    // The claim tier is much tighter than the general acceptable-pick pool:
    // only the top CLAIM_TIER_SIZE picks, and only those genuinely close to
    // Blue's best pick score. This keeps the conditional outcome model
    // comparing like-for-like contenders instead of letting an uncontested
    // mediocre pick into the tier.
    let mut candidates: Vec<(&str, f64)> = pick_scores
        .iter()
        .filter(|(_, &score)| score >= first_pick.best_score - CLAIM_TIER_DELTA)
        .map(|(&champion_id, &score)| (champion_id, score))
        .collect();
    candidates.sort_by(|left, right| right.1.total_cmp(&left.1));
    candidates.truncate(CLAIM_TIER_SIZE);
    if candidates.len() < 2 {
        return Vec::new();
    }

    let red_perspective = RecommendationRequest {
        side: "red".to_string(),
        ..request.clone()
    };
    let red_covered_roles = projected_roles(&red_model.projection);
    let mut pool = Vec::new();
    for (champion_id, blue_pick_value) in candidates {
        let Some(&champion) = champions.get(champion_id) else {
            continue;
        };
        let Some(&overall_row) = overall.get(champion_id) else {
            continue;
        };
        let candidate_roles = role_rows.get(champion_id).cloned().unwrap_or_default();
        let ban_flexibility = candidate_roles
            .iter()
            .filter(|row| row.games >= 5 && row.adjusted_win_rate >= 0.48)
            .count();
        let pick_flexibility = credible_flexibility(champion, &candidate_roles);

        // Red's ban-side view of this champion: full Stage 4A scoring from
        // Red's perspective (including Red's own first-pick-denial reasoning
        // against Blue's pool), but with no own-claim discount (side == red)
        // and no portfolio term (passed as neutral).
        let red_ban_score = score_ban(
            champion,
            overall_row,
            &candidate_roles,
            ban_flexibility,
            rating_baselines,
            overall,
            interactions,
            &ban_draft_context(
                champion_id,
                red_model,
                &request.red_picks,
                &request.blue_picks,
                champions,
                role_rows,
                interactions,
                request.minimum_interaction_games,
                forced_roles,
            ),
            &red_perspective,
            Some(blue_pick_value),
            first_pick,
            // Portfolio evaluation deliberately keeps Red's own-claim inputs
            // neutral (same no-recursion rule as the other portfolio terms).
            None,
            &FirstPickContext::default(),
            draft_presence,
            0.0,
            None,
            None,
            None,
            0.0,
            manual_tiers.get(champion_id).copied(),
        )
        .score;

        // Red's pick-side view: the value to Red if it picks this champion
        // next, given Red's current lineup and Blue's picks so far.
        let mut projected_red_picks = request.red_picks.clone();
        projected_red_picks.push(champion_id.to_string());
        let candidate_model = model_lineup(&projected_red_picks, champions, role_rows, forced_roles);
        let projected_role = primary_role_projection(&candidate_model, champion_id);
        let red_pick_value = score_pick(
            champion,
            overall_row,
            &candidate_roles,
            pick_flexibility,
            &red_covered_roles,
            projected_role,
            &candidate_model,
            blue_model,
            &request.red_picks,
            &request.blue_picks,
            role_rows,
            interactions,
            request,
            rating_baselines,
            draft_presence,
            manual_tiers.get(champion_id).copied(),
        )
        .score;

        pool.push(PortfolioEntry {
            champion_id: champion_id.to_string(),
            champion_name: champion.name.clone(),
            blue_pick_value,
            red_ban_score,
            red_pick_value,
        });
    }
    pool
}

#[derive(Clone, Copy, Default)]
struct FirstPickContext {
    best_score: f64,
    pool_size: usize,
}

#[derive(Default)]
struct BanInteractionSignal {
    value: f64,
    games: usize,
}

#[derive(Default)]
struct BanDraftContext {
    synergy_delta: f64,
    matchup_delta: f64,
    synergy_games: usize,
    matchup_games: usize,
}

fn ban_draft_context_adjustment(context: &BanDraftContext) -> f64 {
    BAN_CONTEXT_SYNERGY_WEIGHT
        * context
            .synergy_delta
            .clamp(-BAN_CONTEXT_DELTA_CLAMP, BAN_CONTEXT_DELTA_CLAMP)
        + BAN_CONTEXT_MATCHUP_WEIGHT
            * context
                .matchup_delta
                .clamp(-BAN_CONTEXT_DELTA_CLAMP, BAN_CONTEXT_DELTA_CLAMP)
}

/// Scores a ban candidate as the opposing side's hypothetical next pick.
/// Positive synergy with their locked picks and positive matchups into this
/// side's locked picks both make leaving the candidate open more dangerous.
fn ban_draft_context(
    candidate: &str,
    banning_model: &LineupModel,
    banning_picks: &[String],
    opposing_picks: &[String],
    champions: &BTreeMap<&str, &DraftChampion>,
    role_rows: &BTreeMap<&str, Vec<&ChampionRoleStat>>,
    evidence: &InteractionEvidence,
    minimum_games: usize,
    forced_roles: &BTreeMap<String, usize>,
) -> BanDraftContext {
    if banning_picks.is_empty() && opposing_picks.is_empty() {
        return BanDraftContext::default();
    }

    let mut projected_opposing_picks = opposing_picks.to_vec();
    projected_opposing_picks.push(candidate.to_string());
    let candidate_model = model_lineup(&projected_opposing_picks, champions, role_rows, forced_roles);
    let role_prior = |champion_id: &str, role: &str| {
        role_rows
            .get(champion_id)
            .and_then(|rows| rows.iter().find(|row| row.role == role))
            .map(|row| row.adjusted_win_rate)
            .unwrap_or(0.5)
    };
    let interactions = expected_interactions(
        candidate,
        &candidate_model,
        banning_model,
        opposing_picks,
        banning_picks,
        evidence,
        minimum_games,
        role_prior,
    );

    BanDraftContext {
        synergy_delta: interactions.synergy_delta,
        matchup_delta: interactions.matchup_delta,
        synergy_games: interactions.synergy_games,
        matchup_games: interactions.matchup_games,
    }
}

fn first_pick_context(
    request: &RecommendationRequest,
    picks: &[Recommendation],
) -> FirstPickContext {
    if !request.blue_picks.is_empty() || picks.is_empty() {
        return FirstPickContext::default();
    }
    let best_score = picks
        .iter()
        .map(|recommendation| recommendation.score)
        .fold(f64::NEG_INFINITY, f64::max);
    let pool = picks
        .iter()
        .filter(|recommendation| recommendation.score >= best_score - FIRST_PICK_POOL_DELTA)
        .collect::<Vec<_>>();
    FirstPickContext {
        best_score,
        pool_size: pool.len(),
    }
}

// Only protect a Blue first-pick target when the acceptable pool is small
// enough that Red's remaining bans could realistically ban it out. A broad
// pool of similarly strong picks doesn't need protection, so those champions
// stay eligible as Blue ban candidates (denial value) instead of being
// blanket-excluded.
fn is_protected_blue_first_pick(
    pick_score: Option<f64>,
    context: &FirstPickContext,
    red_bans_remaining: usize,
) -> bool {
    pick_score.is_some_and(|score| {
        context.pool_size > 0
            && score >= context.best_score - FIRST_PICK_POOL_DELTA
            && context.pool_size <= red_bans_remaining + 1
    })
}

// How close a pick score sits to the top of an acceptable-pick pool, 0..1.
// 1 means "this is (tied for) the pool's best pick", 0 means "outside the
// pool entirely" or "no pool to compare against".
fn claim_closeness(pick_score: Option<f64>, context: &FirstPickContext) -> f64 {
    let Some(pick_score) = pick_score else {
        return 0.0;
    };
    if context.pool_size == 0 {
        return 0.0;
    }
    ((pick_score - (context.best_score - FIRST_PICK_POOL_DELTA)) / FIRST_PICK_POOL_DELTA)
        .clamp(0.0, 1.0)
}

// A ban candidate that is also close to this side's own best pick score is
// probably a champion this side plans to take, so its ban value is discounted —
// banning your own likely pick wastes a ban. The discount fades for
// champions further from the side's best pick score, since those are less
// likely to be its actual claim and retain full denial value.
fn own_claim_discount(pick_score: Option<f64>, context: &FirstPickContext) -> f64 {
    claim_closeness(pick_score, context) * OWN_CLAIM_DISCOUNT_WEIGHT
}

// Red's version of the own-claim discount. Red also wastes a ban by removing
// a champion it wants for itself, but only to the degree Blue wouldn't simply
// claim it first: Blue picks before Red, so a champion Blue also rates highly
// is lost to Red either way — banning it then is denial (already valued by
// `first_pick_denial_adjustment`), not waste.
fn red_own_claim_discount(
    own_pick_score: Option<f64>,
    own_context: &FirstPickContext,
    blue_pick_score: Option<f64>,
    blue_context: &FirstPickContext,
) -> f64 {
    own_claim_discount(own_pick_score, own_context)
        * (1.0 - claim_closeness(blue_pick_score, blue_context))
}

// If the OPPOSING side would also rank this candidate near its own best ban
// (`opposing_ban_score` close to `opposing_best_ban_score`), this side's ban
// is largely redundant — the opponent would likely remove it regardless. The
// discount fades for candidates the opponent has little interest in banning,
// since those are the gaps this side's ban actually closes.
fn redundant_ban_discount(opposing_ban_score: f64, opposing_best_ban_score: f64) -> f64 {
    if opposing_best_ban_score <= 0.0 {
        return 0.0;
    }
    let closeness = ((opposing_ban_score - (opposing_best_ban_score - REDUNDANT_BAN_DELTA))
        / REDUNDANT_BAN_DELTA)
        .clamp(0.0, 1.0);
    closeness * REDUNDANT_BAN_DISCOUNT_WEIGHT
}

fn first_pick_denial_adjustment(
    champion_id: &str,
    pick_score: Option<f64>,
    context: &FirstPickContext,
    request: &RecommendationRequest,
    draft_presence: &BTreeMap<String, f64>,
) -> f64 {
    let Some(pick_score) = pick_score else {
        return 0.0;
    };
    if context.pool_size == 0 || pick_score < context.best_score - FIRST_PICK_POOL_DELTA {
        return 0.0;
    }
    let quality = ((pick_score - (context.best_score - FIRST_PICK_POOL_DELTA))
        / FIRST_PICK_POOL_DELTA)
        .clamp(0.0, 1.0);
    let pressure = draft_presence.get(champion_id).copied().unwrap_or(0.5);
    if request.side == "red" && pick_is_legal_for_side(champion_id, request, "blue") {
        return quality * (4.0 + 7.0 * pressure);
    }
    0.0
}

fn global_counterability(
    threat: &str,
    threat_win_rate: f64,
    overall_rows: &BTreeMap<&str, &ChampionRoleStat>,
    interactions: &InteractionEvidence,
    minimum_games: usize,
) -> BanInteractionSignal {
    let mut answers = overall_rows
        .iter()
        .filter(|(champion_id, _)| **champion_id != threat)
        .filter_map(|(champion_id, row)| {
            let prior = (0.5 + 0.5 * (row.adjusted_win_rate - threat_win_rate)).clamp(0.35, 0.65);
            let estimate = interactions.champion_matchup(champion_id, threat, prior, minimum_games);
            (estimate.games > 0).then(|| {
                let confidence = estimate.games as f64
                    / (estimate.games as f64 + BAN_INTERACTION_CONFIDENCE_GAMES);
                let robust_value = prior + (estimate.win_rate - prior) * confidence;
                (robust_value, estimate.games)
            })
        })
        .collect::<Vec<_>>();
    answers.sort_by(|left, right| right.0.total_cmp(&left.0));
    let top = answers.iter().take(3).collect::<Vec<_>>();
    if top.is_empty() {
        return BanInteractionSignal {
            value: 0.5,
            games: 0,
        };
    }
    BanInteractionSignal {
        value: top.iter().map(|answer| answer.0).sum::<f64>() / top.len() as f64,
        games: top.iter().map(|answer| answer.1).max().unwrap_or(0),
    }
}

fn global_synergy_hub(
    champion: &str,
    champion_win_rate: f64,
    overall_rows: &BTreeMap<&str, &ChampionRoleStat>,
    interactions: &InteractionEvidence,
    minimum_games: usize,
) -> BanInteractionSignal {
    let mut pairings = overall_rows
        .iter()
        .filter(|(ally_id, _)| **ally_id != champion)
        .filter_map(|(ally_id, ally)| {
            let prior = (champion_win_rate + ally.adjusted_win_rate) / 2.0;
            let estimate = interactions.champion_synergy(champion, ally_id, prior, minimum_games);
            (estimate.games > 0).then(|| {
                let confidence = estimate.games as f64
                    / (estimate.games as f64 + BAN_INTERACTION_CONFIDENCE_GAMES);
                (
                    (estimate.win_rate - prior).max(0.0) * confidence,
                    estimate.games,
                )
            })
        })
        .collect::<Vec<_>>();
    pairings.sort_by(|left, right| right.0.total_cmp(&left.0));
    let top = pairings.iter().take(3).collect::<Vec<_>>();
    if top.is_empty() {
        return BanInteractionSignal::default();
    }
    BanInteractionSignal {
        value: top.iter().map(|pairing| pairing.0).sum::<f64>() / top.len() as f64,
        games: top.iter().map(|pairing| pairing.1).max().unwrap_or(0),
    }
}

fn blocked_by_hard_fearless(champion_id: &str, request: &RecommendationRequest) -> bool {
    request.mode == "fearless-hard"
        && request
            .history_blue
            .iter()
            .chain(&request.history_red)
            .any(|id| id == champion_id)
}

fn normalized_weights(weights: &ScoringWeights) -> ScoringWeights {
    let total = weights.performance.max(0.0)
        + weights.synergy.max(0.0)
        + weights.matchup.max(0.0)
        + weights.flexibility.max(0.0)
        + weights.draft_presence.max(0.0);
    if total <= f64::EPSILON {
        return ScoringWeights::default();
    }
    ScoringWeights {
        performance: weights.performance.max(0.0) / total,
        synergy: weights.synergy.max(0.0) / total,
        matchup: weights.matchup.max(0.0) / total,
        flexibility: weights.flexibility.max(0.0) / total,
        draft_presence: weights.draft_presence.max(0.0) / total,
    }
}

fn default_minimum_interaction_games() -> usize {
    5
}

fn projected_roles(projection: &TeamProjection) -> BTreeSet<String> {
    projection
        .champions
        .iter()
        .filter_map(|champion| {
            champion
                .roles
                .iter()
                .find(|row| row.assigned)
                .map(|row| row.role.clone())
        })
        .collect()
}

fn pick_role_value(row: &ChampionRoleStat, covered_roles: &BTreeSet<String>) -> f64 {
    row.adjusted_win_rate * (0.65 + 0.35 * row.confidence)
        + if covered_roles.contains(&row.role) {
            0.0
        } else {
            0.08
        }
}

fn pick_is_legal(champion_id: &str, request: &RecommendationRequest) -> bool {
    pick_is_legal_for_side(champion_id, request, &request.side)
}

fn pick_is_legal_for_side(champion_id: &str, request: &RecommendationRequest, side: &str) -> bool {
    let own_history = if side == "red" {
        &request.history_red
    } else {
        &request.history_blue
    };
    let opponent_history = if side == "red" {
        &request.history_blue
    } else {
        &request.history_red
    };
    request.mode == "normal"
        || (!own_history.iter().any(|id| id == champion_id)
            && (request.mode != "fearless-hard"
                || !opponent_history.iter().any(|id| id == champion_id)))
}

fn sort_rows(rows: &mut [Recommendation]) {
    rows.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.champion_id.cmp(&right.champion_id))
    });
}

fn sort_and_limit(rows: &mut Vec<Recommendation>) {
    sort_rows(rows);
    rows.truncate(8);
}

fn title_role(role: &str) -> String {
    ROLES
        .iter()
        .find(|candidate| **candidate == role)
        .map(|role| {
            let mut chars = role.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .unwrap_or_else(|| role.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn champion_with_role(id: &str, role: &str) -> DraftChampion {
        DraftChampion {
            id: id.to_string(),
            name: id.to_string(),
            portrait: None,
            role_fit: BTreeMap::from([(role.to_string(), 100.0)]),
        }
    }

    fn request_with_lineup(lineup: Option<DraftLineup>) -> RecommendationRequest {
        RecommendationRequest {
            mode: "normal".to_string(),
            side: "blue".to_string(),
            blue_bans: vec![],
            red_bans: vec![],
            blue_picks: vec![],
            red_picks: vec![],
            bans_per_side: DEFAULT_BANS_PER_SIDE,
            history_blue: vec![],
            history_red: vec![],
            weights: ScoringWeights::default(),
            tuning: DraftTuning::default(),
            minimum_interaction_games: 3,
            blue_lineup: lineup,
            red_lineup: None,
            role_overrides: BTreeMap::new(),
        }
    }

    fn stat_row(id: &str, role: &str, games: usize, wins: usize) -> ChampionRoleStat {
        ChampionRoleStat {
            champion_id: id.to_string(),
            champion_name: id.to_string(),
            role: role.to_string(),
            portrait: None,
            games,
            current_patch_games: games,
            effective_games: games as f64,
            patch_changed: false,
            patch_added: false,
            patch_impact: 0.0,
            patch_changes: vec![],
            wins,
            tournament_games: games,
            solo_games: 0,
            win_rate: wins as f64 / games as f64,
            adjusted_win_rate: wins as f64 / games as f64,
            pilot_win_rate_delta: 0.0,
            confidence: 0.8,
            avg_kills: None,
            avg_deaths: None,
            avg_assists: None,
            kda: None,
            avg_damage: None,
            avg_tanking: None,
            avg_healing: None,
            avg_cs: None,
            avg_gold: None,
            avg_rating: None,
            patch_timeline: vec![],
        }
    }

    #[test]
    fn consultation_pool_reports_full_scores_and_exclusion_reasons() {
        let catalog = DraftCatalog {
            champions: vec![
                champion_with_role("alpha", "top"),
                champion_with_role("bravo", "jungle"),
                champion_with_role("charlie", "mid"),
                champion_with_role("delta", "bot"),
                champion_with_role("echo", "support"),
                champion_with_role("foxtrot", "top"),
                // Ghost has no statistics rows at all -> "no data" exclusion.
                champion_with_role("ghost", "top"),
            ],
        };
        let with_stats = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"];
        let statistics = crate::statistics::RoleStatistics {
            database_path: String::new(),
            total_matches: 120,
            current_patch: "1.0".to_string(),
            global_win_rate: 0.5,
            prior_games: 10,
            reliable_games: 20,
            overall_rows: with_stats
                .iter()
                .map(|id| stat_row(id, "overall", 30, 16))
                .collect(),
            role_rows: catalog
                .champions
                .iter()
                .filter(|champion| champion.id != "ghost")
                .map(|champion| {
                    stat_row(&champion.id, champion.role_fit.keys().next().unwrap(), 30, 16)
                })
                .collect(),
            draft_presence: BTreeMap::new(),
        };
        // Soft Fearless draft: alpha banned by Blue, bravo picked by Red,
        // charlie already played by Blue this series, delta manually F-tiered.
        let request = RecommendationRequest {
            mode: "fearless".to_string(),
            blue_bans: vec!["alpha".to_string()],
            red_picks: vec!["bravo".to_string()],
            history_blue: vec!["charlie".to_string()],
            ..request_with_lineup(None)
        };
        let manual_tiers = BTreeMap::from([("delta".to_string(), ManualTier::F)]);
        let shortlist = build_shortlist(
            &request,
            &catalog,
            &statistics,
            &InteractionEvidence::default(),
            &manual_tiers,
        );

        let pick_reason = |id: &str| {
            shortlist
                .pick_exclusions
                .iter()
                .find(|champion| champion.champion_id == id)
                .map(|champion| champion.reason)
        };
        assert_eq!(pick_reason("alpha"), Some(ExclusionReason::BannedByBlue));
        assert_eq!(pick_reason("bravo"), Some(ExclusionReason::PickedByRed));
        assert_eq!(pick_reason("charlie"), Some(ExclusionReason::FearlessUsedOwn));
        assert_eq!(pick_reason("delta"), Some(ExclusionReason::ManuallyExcluded));
        assert_eq!(pick_reason("ghost"), Some(ExclusionReason::NoData));

        // Ban exclusions are narrower: F-tier and soft-Fearless-burned
        // champions can still be ban-scored, so only board/no-data champs land here.
        let ban_excluded = shortlist
            .ban_exclusions
            .iter()
            .map(|champion| champion.champion_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(ban_excluded, BTreeSet::from(["alpha", "bravo", "ghost"]));

        // Every scoreable champion appears in the pick pool, ranked identically
        // to the shortlist while the pool still fits inside the top 8.
        let pool_order = shortlist
            .pick_pool
            .iter()
            .map(|row| row.champion_id.as_str())
            .collect::<Vec<_>>();
        let shortlist_order = shortlist
            .pick_recommendations
            .iter()
            .map(|row| row.champion_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(pool_order, shortlist_order);
        assert_eq!(
            pool_order.iter().copied().collect::<BTreeSet<_>>(),
            BTreeSet::from(["echo", "foxtrot"])
        );

        let ban_pool = shortlist
            .ban_pool
            .iter()
            .map(|row| row.champion_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            ban_pool,
            BTreeSet::from(["charlie", "delta", "echo", "foxtrot"])
        );
        assert!(shortlist
            .pick_pool
            .windows(2)
            .all(|pair| pair[0].score >= pair[1].score));
    }

    fn core_stats(value: i64) -> crate::athletes::CoreStats {
        crate::athletes::CoreStats {
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
    fn credible_role_attaches_capped_athlete_context_without_changing_score_inputs() {
        let request = request_with_lineup(Some(DraftLineup {
            top: Some(42),
            ..DraftLineup::default()
        }));
        let index =
            crate::athletes::AthleteIndex::with_test_entry(42, "candidate", core_stats(95), 1000);

        let context = recommendation_athlete_context(
            &request,
            "candidate",
            Some(("top", 0.9, true)),
            Some(&index),
        )
        .expect("credible assigned athlete with mastery should resolve");

        assert_eq!(context.athlete_id, 42);
        assert_eq!(context.role, "top");
        assert_eq!(context.nominal_stat_buff, 0.20);
        assert!((context.realized_stat_buff - (5.0 / 95.0)).abs() < f64::EPSILON);
        assert_eq!(context.effective_core.last_hit, 100.0);
        assert_eq!(context.realized_gain.last_hit, 5.0);
        assert_eq!(context.capped_stats, 8);
    }

    #[test]
    fn athlete_context_does_not_guess_missing_or_noncredible_assignments() {
        let request = request_with_lineup(Some(DraftLineup {
            top: Some(42),
            ..DraftLineup::default()
        }));
        let index =
            crate::athletes::AthleteIndex::with_test_entry(42, "candidate", core_stats(50), 900);

        assert!(recommendation_athlete_context(
            &request,
            "candidate",
            Some(("top", 0.9, false)),
            Some(&index),
        )
        .is_none());
        assert!(recommendation_athlete_context(
            &request_with_lineup(None),
            "candidate",
            Some(("top", 0.9, true)),
            Some(&index),
        )
        .is_none());
        assert!(recommendation_athlete_context(
            &request,
            "different_champion",
            Some(("top", 0.9, true)),
            Some(&index),
        )
        .is_none());
    }

    #[test]
    fn ban_context_values_synergy_with_opponents_locked_picks() {
        let candidate = champion_with_role("candidate", "top");
        let ally = champion_with_role("ally", "jungle");
        let champions = BTreeMap::from([
            (candidate.id.as_str(), &candidate),
            (ally.id.as_str(), &ally),
        ]);
        let role_rows = BTreeMap::new();
        let banning_model = model_lineup(&[], &champions, &role_rows, &BTreeMap::new());
        let mut evidence = InteractionEvidence::default();
        evidence.insert_role_synergy_sample("candidate", "top", "ally", "jungle", 40, 32);

        let no_picks = ban_draft_context(
            "candidate",
            &banning_model,
            &[],
            &[],
            &champions,
            &role_rows,
            &evidence,
            3,
            &BTreeMap::new(),
        );
        let opposing_picks = vec!["ally".to_string()];
        let with_ally = ban_draft_context(
            "candidate",
            &banning_model,
            &[],
            &opposing_picks,
            &champions,
            &role_rows,
            &evidence,
            3,
            &BTreeMap::new(),
        );

        assert_eq!(ban_draft_context_adjustment(&no_picks), 0.0);
        assert!(with_ally.synergy_delta > 0.05);
        assert_eq!(with_ally.synergy_games, 40);
        assert!(ban_draft_context_adjustment(&with_ally) > 0.0);
    }

    #[test]
    fn ban_context_values_matchups_into_own_locked_picks() {
        let candidate = champion_with_role("candidate", "top");
        let defender = champion_with_role("defender", "top");
        let champions = BTreeMap::from([
            (candidate.id.as_str(), &candidate),
            (defender.id.as_str(), &defender),
        ]);
        let role_rows = BTreeMap::new();
        let banning_picks = vec!["defender".to_string()];
        let banning_model = model_lineup(&banning_picks, &champions, &role_rows, &BTreeMap::new());
        let mut evidence = InteractionEvidence::default();
        evidence.insert_role_matchup_sample("candidate", "top", "defender", "top", 50, 40);

        let context = ban_draft_context(
            "candidate",
            &banning_model,
            &banning_picks,
            &[],
            &champions,
            &role_rows,
            &evidence,
            3,
            &BTreeMap::new(),
        );

        assert!(context.matchup_delta > 0.05);
        assert_eq!(context.matchup_games, 50);
        assert!(ban_draft_context_adjustment(&context) > 0.0);
    }

    #[test]
    fn ban_context_ignores_cross_role_matchups() {
        let candidate = champion_with_role("candidate", "top");
        let defender = champion_with_role("defender", "mid");
        let champions = BTreeMap::from([
            (candidate.id.as_str(), &candidate),
            (defender.id.as_str(), &defender),
        ]);
        let role_rows = BTreeMap::new();
        let banning_picks = vec!["defender".to_string()];
        let banning_model = model_lineup(&banning_picks, &champions, &role_rows, &BTreeMap::new());
        let mut evidence = InteractionEvidence::default();
        evidence.insert_role_matchup_sample("candidate", "top", "defender", "mid", 50, 40);

        let context = ban_draft_context(
            "candidate",
            &banning_model,
            &banning_picks,
            &[],
            &champions,
            &role_rows,
            &evidence,
            3,
            &BTreeMap::new(),
        );

        assert_eq!(context.matchup_delta, 0.0);
        assert_eq!(context.matchup_games, 0);
        assert_eq!(ban_draft_context_adjustment(&context), 0.0);
    }

    #[test]
    fn fearless_legality_matches_game_rules() {
        let request = RecommendationRequest {
            mode: "fearless".to_string(),
            side: "blue".to_string(),
            blue_bans: vec![],
            red_bans: vec![],
            blue_picks: vec![],
            red_picks: vec![],
            bans_per_side: DEFAULT_BANS_PER_SIDE,
            history_blue: vec!["archer".to_string()],
            history_red: vec!["ghost".to_string()],
            weights: ScoringWeights::default(),
            tuning: DraftTuning::default(),
            minimum_interaction_games: 3,
            blue_lineup: None,
            red_lineup: None,
            role_overrides: BTreeMap::new(),
        };
        assert!(!pick_is_legal("archer", &request));
        assert!(pick_is_legal("ghost", &request));
        let hard = RecommendationRequest {
            mode: "fearless-hard".to_string(),
            ..request
        };
        assert!(!pick_is_legal("ghost", &hard));
        assert!(blocked_by_hard_fearless("archer", &hard));
        assert!(blocked_by_hard_fearless("ghost", &hard));
        assert!(!blocked_by_hard_fearless("swordman", &hard));
    }

    #[test]
    fn blue_protects_first_pick_candidates_while_red_values_denial() {
        let request = RecommendationRequest {
            mode: "normal".to_string(),
            side: "blue".to_string(),
            blue_bans: vec![],
            red_bans: vec![],
            blue_picks: vec![],
            red_picks: vec![],
            bans_per_side: DEFAULT_BANS_PER_SIDE,
            history_blue: vec![],
            history_red: vec![],
            weights: ScoringWeights::default(),
            tuning: DraftTuning::default(),
            minimum_interaction_games: 3,
            blue_lineup: None,
            red_lineup: None,
            role_overrides: BTreeMap::new(),
        };
        let broad_pool = FirstPickContext {
            best_score: 80.0,
            pool_size: 5,
        };
        let fragile_pool = FirstPickContext {
            best_score: 80.0,
            pool_size: 2,
        };
        let presence = BTreeMap::from([("target".to_string(), 0.8)]);

        // Red has 3 bans remaining: a broad pool of 5 acceptable picks can't be
        // banned out, so nothing is protected and strong champions remain
        // available as Blue ban candidates.
        assert!(!is_protected_blue_first_pick(Some(80.0), &broad_pool, 3));
        assert!(!is_protected_blue_first_pick(Some(70.0), &broad_pool, 3));
        // A fragile pool of 2 acceptable picks, with 3 Red bans remaining,
        // could be banned out entirely, so the candidates within it are
        // protected from Blue's own ban list.
        assert!(is_protected_blue_first_pick(Some(80.0), &fragile_pool, 3));
        assert!(!is_protected_blue_first_pick(Some(70.0), &fragile_pool, 3));
        let blue =
            first_pick_denial_adjustment("target", Some(80.0), &broad_pool, &request, &presence);
        let red = first_pick_denial_adjustment(
            "target",
            Some(80.0),
            &broad_pool,
            &RecommendationRequest {
                side: "red".to_string(),
                ..request
            },
            &presence,
        );

        assert_eq!(blue, 0.0);
        assert!(red > 0.0);
    }

    fn portfolio_entry(id: &str, blue_pick: f64, red_ban: f64, red_pick: f64) -> PortfolioEntry {
        PortfolioEntry {
            champion_id: id.to_string(),
            champion_name: id.to_string(),
            blue_pick_value: blue_pick,
            red_ban_score: red_ban,
            red_pick_value: red_pick,
        }
    }

    #[test]
    fn red_own_claim_discount_fades_with_blue_interest() {
        let own = FirstPickContext {
            best_score: 100.0,
            pool_size: 3,
        };
        let blue = FirstPickContext {
            best_score: 100.0,
            pool_size: 3,
        };
        // Red's top pick that Blue has no interest in: full discount — banning
        // it would only waste Red's own claim.
        assert_eq!(
            red_own_claim_discount(Some(100.0), &own, None, &blue),
            OWN_CLAIM_DISCOUNT_WEIGHT
        );
        // Blue also rates it as its own best pick: Blue claims it first either
        // way, so banning it is pure denial and the discount vanishes.
        assert_eq!(
            red_own_claim_discount(Some(100.0), &own, Some(100.0), &blue),
            0.0
        );
        // Partial Blue interest scales the discount down proportionally.
        let partial = red_own_claim_discount(
            Some(100.0),
            &own,
            Some(100.0 - FIRST_PICK_POOL_DELTA / 2.0),
            &blue,
        );
        assert!((partial - OWN_CLAIM_DISCOUNT_WEIGHT * 0.5).abs() < 1e-9);
        // A champion Red has no pick interest in is never discounted.
        assert_eq!(red_own_claim_discount(None, &own, None, &blue), 0.0);
    }

    #[test]
    fn survival_factor_neutral_for_single_or_tied_pool() {
        // A pool of one has nothing to compare against, so survival stays neutral.
        assert_eq!(survival_factor(50.0, &[50.0]), RED_PRESSURE_BASELINE);
        // Tied red ban scores collapse the spread to zero; also neutral.
        assert_eq!(
            survival_factor(50.0, &[50.0, 50.0, 50.0]),
            RED_PRESSURE_BASELINE
        );
    }

    #[test]
    fn survival_factor_penalizes_higher_red_pressure() {
        let pool_scores = [40.0, 60.0, 80.0];
        // The champion Red would most want to ban (highest red ban score)
        // survives less reliably than one Red is less interested in.
        let high_pressure = survival_factor(80.0, &pool_scores);
        let low_pressure = survival_factor(40.0, &pool_scores);
        assert!(high_pressure < RED_PRESSURE_BASELINE);
        assert!(low_pressure > RED_PRESSURE_BASELINE);
        assert!(high_pressure < low_pressure);
        assert!((SURVIVAL_FACTOR_MIN..=SURVIVAL_FACTOR_MAX).contains(&high_pressure));
        assert!((SURVIVAL_FACTOR_MIN..=SURVIVAL_FACTOR_MAX).contains(&low_pressure));
    }

    #[test]
    fn portfolio_adjustment_is_zero_with_single_op_candidate() {
        // One OP candidate: nothing to compare it against, so no portfolio
        // term applies to any ban candidate. The fragile-pool gate (which
        // operates earlier, in is_protected_blue_first_pick) is responsible
        // for protecting a lone candidate.
        let pool = vec![portfolio_entry("soldier", 70.0, 86.0, 60.0)];
        let baseline = portfolio_value(&pool.iter().collect::<Vec<_>>());
        let (adjustment, claim, leftover, survival) =
            portfolio_adjustment_for(&pool, &baseline, "soldier");
        assert_eq!(adjustment, 0.0);
        assert_eq!(claim, None);
        assert_eq!(leftover, None);
        assert_eq!(survival, None);
    }

    #[test]
    fn portfolio_adjustment_favors_banning_either_uncontested_op_candidate() {
        // Two OP candidates Blue can't both claim. Soldier is Blue's
        // higher-value pick (70.0 > 68.0), so it's the claim in the
        // "Red doesn't ban its top target" branch regardless of Red
        // pressure — the claim is always whichever candidate is actually
        // highest-valued in its outcome. Whip Master is left to Red.
        let pool = vec![
            portfolio_entry("soldier", 70.0, 86.0, 60.0),
            portfolio_entry("whip_master", 68.0, 70.0, 58.0),
        ];
        let baseline = portfolio_value(&pool.iter().collect::<Vec<_>>());
        assert_eq!(
            baseline.claim.as_ref().map(|(id, _)| id.as_str()),
            Some("soldier")
        );
        assert_eq!(
            baseline.leftover.as_ref().map(|(id, _)| id.as_str()),
            Some("whip_master")
        );

        // Either way, leaving only one OP candidate open removes the "lose
        // the leftover to Red" term entirely, improving the portfolio
        // outcome over the no-ban baseline — banning either is favorable.
        let (ban_soldier, claim_after, leftover_after, _) =
            portfolio_adjustment_for(&pool, &baseline, "soldier");
        assert!(ban_soldier > 0.0);
        assert_eq!(
            claim_after.as_ref().map(|(_, name)| name.as_str()),
            Some("whip_master")
        );
        assert_eq!(leftover_after, None);

        let (ban_whip_master, claim_after, leftover_after, _) =
            portfolio_adjustment_for(&pool, &baseline, "whip_master");
        assert!(ban_whip_master > 0.0);
        assert_eq!(
            claim_after.as_ref().map(|(_, name)| name.as_str()),
            Some("soldier")
        );
        assert_eq!(leftover_after, None);
    }

    #[test]
    fn portfolio_adjustment_clamps_extreme_deltas() {
        let pool = vec![
            portfolio_entry("a", 100.0, 100.0, 0.0),
            portfolio_entry("b", 0.0, 0.0, 100.0),
        ];
        let baseline = portfolio_value(&pool.iter().collect::<Vec<_>>());
        let (adjustment, _, _, _) = portfolio_adjustment_for(&pool, &baseline, "b");
        assert!(adjustment <= PORTFOLIO_ADJUSTMENT_CLAMP);
        assert!(adjustment >= -PORTFOLIO_ADJUSTMENT_CLAMP);
    }

    #[test]
    fn portfolio_pool_is_empty_below_fragile_pool_gate() {
        // pool_size < 2: nothing for the portfolio comparison to do, so the
        // pool stays empty and every candidate gets a zero adjustment.
        let small_pool = FirstPickContext {
            best_score: 70.0,
            pool_size: 1,
        };
        let request = RecommendationRequest {
            mode: "normal".to_string(),
            side: "blue".to_string(),
            blue_bans: vec![],
            red_bans: vec![],
            blue_picks: vec![],
            red_picks: vec![],
            bans_per_side: DEFAULT_BANS_PER_SIDE,
            history_blue: vec![],
            history_red: vec![],
            weights: ScoringWeights::default(),
            tuning: DraftTuning::default(),
            minimum_interaction_games: 3,
            blue_lineup: None,
            red_lineup: None,
            role_overrides: BTreeMap::new(),
        };
        let champions = BTreeMap::new();
        let overall = BTreeMap::new();
        let role_rows = BTreeMap::new();
        let rating_baselines = BTreeMap::new();
        let draft_presence = BTreeMap::new();
        let pick_scores = BTreeMap::from([("soldier", 70.0)]);
        let catalog = DraftCatalog { champions: vec![] };
        let empty_model = model_lineup(&[], &champions, &role_rows, &BTreeMap::new());
        let interactions = InteractionEvidence::default();
        let _ = &catalog;
        let pool = build_portfolio_pool(
            &champions,
            &overall,
            &role_rows,
            &rating_baselines,
            &interactions,
            &request,
            &draft_presence,
            &pick_scores,
            &small_pool,
            &empty_model,
            &empty_model,
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        assert!(pool.is_empty());
    }

    #[test]
    fn survival_label_is_qualitative_not_a_percentage() {
        assert_eq!(
            survival_label(SURVIVAL_FACTOR_MIN),
            (
                "a likely Red ban target",
                "recommendation.survival.likelyBanTarget"
            )
        );
        assert_eq!(
            survival_label(RED_PRESSURE_BASELINE),
            (
                "at moderate contest risk from Red",
                "recommendation.survival.moderateRisk"
            )
        );
        assert_eq!(
            survival_label(SURVIVAL_FACTOR_MAX),
            (
                "likely to survive Red's next ban",
                "recommendation.survival.likelySurvive"
            )
        );
    }

    #[test]
    fn later_pick_changes_flexible_champion_projection() {
        let flex = DraftChampion {
            id: "flex".to_string(),
            name: "Flex".to_string(),
            portrait: None,
            role_fit: BTreeMap::from([("top".to_string(), 80.0), ("jungle".to_string(), 100.0)]),
            ..Default::default()
        };
        let jungler = DraftChampion {
            id: "jungler".to_string(),
            name: "Jungler".to_string(),
            portrait: None,
            role_fit: BTreeMap::from([("jungle".to_string(), 100.0)]),
            ..Default::default()
        };
        let champions = BTreeMap::from([("flex", &flex), ("jungler", &jungler)]);
        let rows = BTreeMap::new();
        let one = project_lineup(&["flex".to_string()], &champions, &rows);
        let two = project_lineup(
            &["flex".to_string(), "jungler".to_string()],
            &champions,
            &rows,
        );
        let first_jungle = one.champions[0]
            .roles
            .iter()
            .find(|row| row.role == "jungle")
            .unwrap()
            .probability;
        let later_jungle = two.champions[0]
            .roles
            .iter()
            .find(|row| row.role == "jungle")
            .map(|row| row.probability)
            .unwrap_or_default();
        assert!(later_jungle < first_jungle);
        assert!(two.champions[0].roles[0].role == "top");
    }

    #[test]
    fn confirmed_role_override_forces_assignment_against_inferred_role() {
        // Flex infers to jungle (its strongest role_fit) with no override.
        let flex = DraftChampion {
            id: "flex".to_string(),
            name: "Flex".to_string(),
            portrait: None,
            role_fit: BTreeMap::from([("top".to_string(), 80.0), ("jungle".to_string(), 100.0)]),
            ..Default::default()
        };
        let champions = BTreeMap::from([("flex", &flex)]);
        let rows = BTreeMap::new();
        let picks = vec!["flex".to_string()];

        let inferred = model_lineup(&picks, &champions, &rows, &BTreeMap::new());
        assert_eq!(
            inferred.primary_roles.get("flex").map(String::as_str),
            Some("jungle")
        );

        // Confirming "top" collapses the distribution: assignment flips to top,
        // is marked assigned (credible by user assertion), and is a certainty.
        let forced = BTreeMap::from([("flex".to_string(), role_to_index("top").unwrap())]);
        let overridden = model_lineup(&picks, &champions, &rows, &forced);
        assert_eq!(
            overridden.primary_roles.get("flex").map(String::as_str),
            Some("top")
        );
        let top_row = overridden.projection.champions[0]
            .roles
            .iter()
            .find(|row| row.role == "top")
            .expect("top role present");
        assert!(top_row.assigned);
        assert!((top_row.probability - 1.0).abs() < 1e-9);
    }

    #[test]
    fn role_to_index_accepts_known_roles_and_bottom_alias() {
        assert_eq!(role_to_index("top"), Some(0));
        assert_eq!(role_to_index("support"), Some(4));
        assert_eq!(role_to_index("bottom"), role_to_index("bot"));
        assert_eq!(role_to_index("midlane"), None);
    }

    #[test]
    fn credibility_requires_probability_and_real_or_dominant_catalog_evidence() {
        assert!(role_is_credible(0.20, 5, 20));
        assert!(!role_is_credible(0.14, 20, 20));
        assert!(!role_is_credible(0.40, 2, 20));
        assert!(role_is_credible(0.30, 0, 0));
        assert!(!role_is_credible(0.20, 0, 0));
    }

    #[test]
    fn two_support_claimants_do_not_force_a_confident_off_role() {
        let first = champion_with_role("first_support", "support");
        let second = champion_with_role("second_support", "support");
        let champions =
            BTreeMap::from([(first.id.as_str(), &first), (second.id.as_str(), &second)]);
        let picks = vec![first.id.clone(), second.id.clone()];
        let model = model_lineup(&picks, &champions, &BTreeMap::new(), &BTreeMap::new());
        let assigned = model
            .projection
            .champions
            .iter()
            .flat_map(|champion| {
                champion
                    .roles
                    .iter()
                    .filter(|role| role.assigned)
                    .map(move |role| (champion.champion_id.as_str(), role.role.as_str()))
            })
            .collect::<Vec<_>>();

        assert_eq!(assigned.len(), 1);
        assert_eq!(assigned[0].1, "support");
        let undetermined = model
            .projection
            .champions
            .iter()
            .find(|champion| !champion.roles.iter().any(|role| role.assigned))
            .unwrap();
        assert!(undetermined
            .roles
            .iter()
            .all(|role| role.role == "support" || !role.assigned));
        let covered = projected_roles(
            &model_lineup(
                std::slice::from_ref(&first.id),
                &champions,
                &BTreeMap::new(),
                &BTreeMap::new(),
            )
            .projection,
        );
        let collision =
            role_collision_penalty(&second, &[], &model, &covered, std::slice::from_ref(&first.id));
        // A second dedicated support onto a covered support role is locked out.
        assert_eq!(collision.locked_role.as_deref(), Some("support"));
        assert!(collision.penalty >= ROLE_COLLISION_LOCKED_PENALTY);
    }

    #[test]
    fn uniform_catalog_fallback_is_role_undetermined() {
        let champion = DraftChampion {
            id: "unknown".to_string(),
            name: "Unknown".to_string(),
            portrait: None,
            role_fit: BTreeMap::new(),
        };
        let champions = BTreeMap::from([(champion.id.as_str(), &champion)]);
        let projection = project_lineup(&[champion.id.clone()], &champions, &BTreeMap::new());

        assert!(!projection.champions[0]
            .roles
            .iter()
            .any(|role| role.assigned));
    }

    #[test]
    fn completed_lineup_surfaces_one_primary_champion_per_role() {
        let champions_owned = (0..5)
            .map(|index| DraftChampion {
                id: format!("champion_{index}"),
                name: format!("Champion {index}"),
                portrait: None,
                role_fit: BTreeMap::from([(ROLES[index].to_string(), 100.0)]),
            })
            .collect::<Vec<_>>();
        let champions = champions_owned
            .iter()
            .map(|champion| (champion.id.as_str(), champion))
            .collect::<BTreeMap<_, _>>();
        let picks = champions_owned
            .iter()
            .map(|champion| champion.id.clone())
            .collect::<Vec<_>>();
        let projection = project_lineup(&picks, &champions, &BTreeMap::new());
        let primary_roles = projection
            .champions
            .iter()
            .map(|champion| {
                let primary = champion.roles.first().unwrap();
                assert!(primary.assigned);
                primary.role.as_str()
            })
            .collect::<BTreeSet<_>>();

        assert_eq!(primary_roles, ROLES.into_iter().collect());
    }

    #[test]
    #[ignore = "prints recommendation reasons (incl. synergy evidence) from the live DB"]
    fn audit_recommendation_reasons() {
        let path = std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap()
            .join("com.lttools.lt-ai-coach")
            .join("lt-ai-coach.sqlite3");
        let catalog = crate::draft::load_draft_catalog(&path).unwrap();
        let statistics = crate::statistics::query_role_statistics(&path).unwrap();
        let interactions = crate::interactions::query_interactions(&path).unwrap();
        let request = RecommendationRequest {
            mode: "normal".to_string(),
            side: "blue".to_string(),
            blue_bans: vec![],
            red_bans: vec![],
            blue_picks: vec!["whip_master".to_string(), "demon".to_string()],
            red_picks: vec![],
            bans_per_side: DEFAULT_BANS_PER_SIDE,
            history_blue: vec![],
            history_red: vec![],
            weights: ScoringWeights::default(),
            tuning: DraftTuning::default(),
            minimum_interaction_games: 3,
            blue_lineup: None,
            red_lineup: None,
            role_overrides: BTreeMap::new(),
        };
        let shortlist = build_shortlist(
            &request,
            &catalog,
            &statistics,
            &interactions,
            &BTreeMap::new(),
        );
        for rec in shortlist.pick_recommendations.iter().take(3) {
            eprintln!("{} — score {:.1}", rec.champion_name, rec.score);
            for reason in &rec.reasons {
                eprintln!("   - {reason}");
            }
        }
        eprintln!("Ban recommendations:");
        for rec in shortlist.ban_recommendations.iter().take(5) {
            eprintln!("{} — score {:.1}", rec.champion_name, rec.score);
            for reason in &rec.reasons {
                eprintln!("   - {reason}");
            }
        }
        let opening = RecommendationRequest {
            blue_picks: vec![],
            ..request.clone()
        };
        let blue_opening = build_shortlist(
            &opening,
            &catalog,
            &statistics,
            &interactions,
            &BTreeMap::new(),
        );
        let red_opening = build_shortlist(
            &RecommendationRequest {
                side: "red".to_string(),
                ..opening
            },
            &catalog,
            &statistics,
            &interactions,
            &BTreeMap::new(),
        );
        let print_breakdown = |side: &str, rec: &Recommendation| {
            let components = rec
                .ban_score_components
                .expect("ban recommendation must retain score components");
            eprintln!(
                "{side} example — {}:\n  base ban score:       {:+.2}\n  own-claim discount:   -{:.2}\n  portfolio adjustment: {:+.2}\n  redundant discount:   -{:.2}\n  final score:           {:.2}",
                rec.champion_name,
                components.base_score,
                components.own_claim_discount,
                components.portfolio_adjustment,
                components.redundant_ban_discount,
                components.final_score
            );
        };
        let blue_top = &blue_opening.ban_recommendations[0];
        let red_top = &red_opening.ban_recommendations[0];
        eprintln!("\nStage 4B structured audit:");
        print_breakdown("Blue", blue_top);
        print_breakdown("Red", red_top);
        let diverged = blue_top.champion_id != red_top.champion_id;
        eprintln!(
            "Blue/Red divergence: {} (Blue={}, Red={})",
            if diverged { "CONFIRMED" } else { "FAILED" },
            blue_top.champion_name,
            red_top.champion_name
        );
        assert!(
            diverged,
            "opening Blue and Red ban recommendations should diverge"
        );
        eprintln!("Opening Blue picks (pool):");
        for rec in blue_opening.pick_recommendations.iter().take(6) {
            eprintln!("{} — score {:.1}", rec.champion_name, rec.score);
        }
        eprintln!("Opening Blue bans (detail):");
        for rec in blue_opening.ban_recommendations.iter().take(6) {
            eprintln!("{} — score {:.1}", rec.champion_name, rec.score);
            for reason in &rec.reasons {
                eprintln!("   - {reason}");
            }
        }
    }

    #[test]
    #[ignore = "audits role projections against the current application database"]
    fn audits_real_flexible_projection() {
        let path = std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap()
            .join("com.lttools.lt-ai-coach")
            .join("lt-ai-coach.sqlite3");
        let catalog = crate::draft::load_draft_catalog(&path).unwrap();
        let statistics = crate::statistics::query_role_statistics(&path).unwrap();
        let request = |picks: Vec<&str>| RecommendationRequest {
            mode: "normal".to_string(),
            side: "blue".to_string(),
            blue_bans: vec![],
            red_bans: vec![],
            blue_picks: picks.into_iter().map(str::to_string).collect(),
            red_picks: vec![],
            bans_per_side: DEFAULT_BANS_PER_SIDE,
            history_blue: vec![],
            history_red: vec![],
            weights: ScoringWeights::default(),
            tuning: DraftTuning::default(),
            minimum_interaction_games: 3,
            blue_lineup: None,
            red_lineup: None,
            role_overrides: BTreeMap::new(),
        };
        let interactions = crate::interactions::query_interactions(&path).unwrap();
        let one = build_shortlist(
            &request(vec!["whip_master"]),
            &catalog,
            &statistics,
            &interactions,
            &BTreeMap::new(),
        );
        let two = build_shortlist(
            &request(vec!["whip_master", "demon"]),
            &catalog,
            &statistics,
            &interactions,
            &BTreeMap::new(),
        );
        let probabilities = |shortlist: &RecommendationShortlist| {
            shortlist.blue_projection.champions[0]
                .roles
                .iter()
                .map(|row| format!("{} {:.0}%", row.role, row.probability * 100.0))
                .collect::<Vec<_>>()
                .join(", ")
        };
        eprintln!("Whip Master alone: {}", probabilities(&one));
        eprintln!("After Demon: {}", probabilities(&two));
        assert_ne!(probabilities(&one), probabilities(&two));
    }

    #[test]
    #[ignore = "profiles recommendation stages against the current application database"]
    fn profiles_live_recommendation_runtime() {
        use std::time::Instant;

        let path = std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap()
            .join("com.lttools.lt-ai-coach")
            .join("lt-ai-coach.sqlite3");

        let started = Instant::now();
        let catalog = crate::draft::load_draft_catalog(&path).unwrap();
        let catalog_elapsed = started.elapsed();

        let started = Instant::now();
        let statistics = crate::statistics::query_role_statistics(&path).unwrap();
        let statistics_elapsed = started.elapsed();

        let started = Instant::now();
        let interactions = crate::interactions::query_interactions(&path).unwrap();
        let interactions_elapsed = started.elapsed();

        let request = RecommendationRequest {
            mode: "normal".to_string(),
            side: "blue".to_string(),
            blue_bans: vec![],
            red_bans: vec![],
            blue_picks: vec!["whip_master".to_string(), "demon".to_string()],
            red_picks: vec!["shield_bearer".to_string(), "archer".to_string()],
            bans_per_side: DEFAULT_BANS_PER_SIDE,
            history_blue: vec![],
            history_red: vec![],
            weights: ScoringWeights::default(),
            tuning: DraftTuning::default(),
            minimum_interaction_games: 3,
            blue_lineup: None,
            red_lineup: None,
            role_overrides: BTreeMap::new(),
        };
        let started = Instant::now();
        let shortlist = build_shortlist(
            &request,
            &catalog,
            &statistics,
            &interactions,
            &BTreeMap::new(),
        );
        let scoring_elapsed = started.elapsed();

        eprintln!(
            "catalog={catalog_elapsed:?} statistics={statistics_elapsed:?} interactions={interactions_elapsed:?} scoring={scoring_elapsed:?} picks={} bans={}",
            shortlist.pick_recommendations.len(),
            shortlist.ban_recommendations.len()
        );
    }
}
