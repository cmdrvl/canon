//! Deterministic integer score units shared by entity edge and solve stages.
//!
//! Metric implementations may use floats internally, but this module is the
//! artifact boundary: thresholds, ordering, review output, and solver-visible
//! values use only integer units.

use crate::namekit::SimilarityScore;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

pub const CANON_ENTITY_SCORE_VERSION: &str = "canon_entity_score.v0";
pub const ENTITY_SCORE_SCALE: u32 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScoreUnits(u32);

impl ScoreUnits {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(ENTITY_SCORE_SCALE);

    pub fn from_scaled(value: u32) -> Option<Self> {
        (value <= ENTITY_SCORE_SCALE).then_some(Self(value))
    }

    pub fn from_ratio_parts(numerator: u64, denominator: u64) -> Option<Self> {
        if denominator == 0 {
            return None;
        }

        let numerator = u128::from(numerator);
        let denominator = u128::from(denominator);
        let scaled = (numerator * u128::from(ENTITY_SCORE_SCALE) + (denominator / 2)) / denominator;
        Some(Self::from_clamped_u128(scaled))
    }

    pub fn from_f64_ratio(ratio: f64) -> Self {
        let clamped = if ratio.is_nan() {
            0.0
        } else {
            ratio.clamp(0.0, 1.0)
        };
        Self((clamped * f64::from(ENTITY_SCORE_SCALE) + 0.5).floor() as u32)
    }

    pub const fn saturating_from_units(value: u64) -> Self {
        if value > ENTITY_SCORE_SCALE as u64 {
            Self(ENTITY_SCORE_SCALE)
        } else {
            Self(value as u32)
        }
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }

    fn from_clamped_u128(value: u128) -> Self {
        if value > u128::from(ENTITY_SCORE_SCALE) {
            Self(ENTITY_SCORE_SCALE)
        } else {
            Self(value as u32)
        }
    }
}

impl From<SimilarityScore> for ScoreUnits {
    fn from(score: SimilarityScore) -> Self {
        Self(u32::from(score.as_scaled()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScoreThreshold(ScoreUnits);

impl ScoreThreshold {
    pub const fn new(score_units: ScoreUnits) -> Self {
        Self(score_units)
    }

    pub const fn units(self) -> ScoreUnits {
        self.0
    }

    pub fn accepts(self, score_units: ScoreUnits) -> bool {
        score_units >= self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreLane {
    Support,
    AntiMerge,
    RelationHint,
}

impl ScoreLane {
    pub const fn contributes_to_merge_total(self) -> bool {
        matches!(self, Self::Support)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreContribution {
    pub lane: ScoreLane,
    pub source_id: String,
    pub reason_code: String,
    pub score_units: ScoreUnits,
}

impl ScoreContribution {
    pub fn new(
        lane: ScoreLane,
        source_id: impl Into<String>,
        reason_code: impl Into<String>,
        score_units: ScoreUnits,
    ) -> Self {
        Self {
            lane,
            source_id: source_id.into(),
            reason_code: reason_code.into(),
            score_units,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    pub version: String,
    pub total_score_units: ScoreUnits,
    pub raw_support_score_units: u64,
    pub contributions: Vec<ScoreContribution>,
    pub top_contributors: Vec<ScoreContribution>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ScoreOptimizationHints {
    pub cutoff_units: Option<ScoreUnits>,
    pub score_hint_units: Option<ScoreUnits>,
}

impl ScoreOptimizationHints {
    pub const fn new(
        cutoff_units: Option<ScoreUnits>,
        score_hint_units: Option<ScoreUnits>,
    ) -> Self {
        Self {
            cutoff_units,
            score_hint_units,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoredCandidate {
    pub candidate_id: String,
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub score_units: ScoreUnits,
    pub hard_cannot_link: bool,
}

impl ScoredCandidate {
    pub fn new(
        candidate_id: impl Into<String>,
        left_surface_id: impl Into<String>,
        right_surface_id: impl Into<String>,
        score_units: ScoreUnits,
        hard_cannot_link: bool,
    ) -> Self {
        Self {
            candidate_id: candidate_id.into(),
            left_surface_id: left_surface_id.into(),
            right_surface_id: right_surface_id.into(),
            score_units,
            hard_cannot_link,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateScoreDecision {
    pub candidate_id: String,
    pub score_units: ScoreUnits,
    pub threshold_units: ScoreUnits,
    pub hard_cannot_link: bool,
    pub accepted: bool,
    pub reason: CandidateScoreDecisionReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateScoreDecisionReason {
    Accepted,
    BelowThreshold,
    HardCannotLink,
}

pub fn accumulate_score_units<I>(contributions: I) -> ScoreBreakdown
where
    I: IntoIterator<Item = ScoreContribution>,
{
    let mut contributions = contributions.into_iter().collect::<Vec<_>>();
    contributions.sort_by(score_contribution_accumulation_cmp);

    let raw_support_score_units = contributions
        .iter()
        .filter(|contribution| contribution.lane.contributes_to_merge_total())
        .map(|contribution| u64::from(contribution.score_units.as_u32()))
        .sum::<u64>();
    let total_score_units = ScoreUnits::saturating_from_units(raw_support_score_units);
    let top_contributors = top_score_contributors(&contributions, contributions.len());

    ScoreBreakdown {
        version: CANON_ENTITY_SCORE_VERSION.to_string(),
        total_score_units,
        raw_support_score_units,
        contributions,
        top_contributors,
    }
}

pub fn top_score_contributors(
    contributions: &[ScoreContribution],
    limit: usize,
) -> Vec<ScoreContribution> {
    let mut contributors = contributions.to_vec();
    contributors.sort_by(score_contribution_top_cmp);
    contributors.truncate(limit);
    contributors
}

pub fn evaluate_candidate_score(
    candidate: &ScoredCandidate,
    threshold: ScoreThreshold,
) -> CandidateScoreDecision {
    let reason = if candidate.hard_cannot_link {
        CandidateScoreDecisionReason::HardCannotLink
    } else if threshold.accepts(candidate.score_units) {
        CandidateScoreDecisionReason::Accepted
    } else {
        CandidateScoreDecisionReason::BelowThreshold
    };

    CandidateScoreDecision {
        candidate_id: candidate.candidate_id.clone(),
        score_units: candidate.score_units,
        threshold_units: threshold.units(),
        hard_cannot_link: candidate.hard_cannot_link,
        accepted: reason == CandidateScoreDecisionReason::Accepted,
        reason,
    }
}

pub fn accepted_candidate_ids<I>(candidates: I, threshold: ScoreThreshold) -> Vec<String>
where
    I: IntoIterator<Item = ScoredCandidate>,
{
    accepted_candidate_ids_with_hints(candidates, threshold, ScoreOptimizationHints::default())
}

pub fn accepted_candidate_ids_with_hints<I>(
    candidates: I,
    threshold: ScoreThreshold,
    _hints: ScoreOptimizationHints,
) -> Vec<String>
where
    I: IntoIterator<Item = ScoredCandidate>,
{
    let mut accepted = candidates
        .into_iter()
        .filter(|candidate| evaluate_candidate_score(candidate, threshold).accepted)
        .map(|candidate| candidate.candidate_id)
        .collect::<Vec<_>>();
    accepted.sort_by(|left, right| cmp_bytes(left, right));
    accepted.dedup();
    accepted
}

pub fn sort_scored_candidates(candidates: &mut [ScoredCandidate]) {
    candidates.sort_by(scored_candidate_cmp);
}

fn score_contribution_accumulation_cmp(
    left: &ScoreContribution,
    right: &ScoreContribution,
) -> Ordering {
    left.lane
        .cmp(&right.lane)
        .then_with(|| cmp_bytes(&left.source_id, &right.source_id))
        .then_with(|| cmp_bytes(&left.reason_code, &right.reason_code))
        .then_with(|| right.score_units.cmp(&left.score_units))
}

fn score_contribution_top_cmp(left: &ScoreContribution, right: &ScoreContribution) -> Ordering {
    right
        .score_units
        .cmp(&left.score_units)
        .then_with(|| left.lane.cmp(&right.lane))
        .then_with(|| cmp_bytes(&left.source_id, &right.source_id))
        .then_with(|| cmp_bytes(&left.reason_code, &right.reason_code))
}

fn scored_candidate_cmp(left: &ScoredCandidate, right: &ScoredCandidate) -> Ordering {
    right
        .score_units
        .cmp(&left.score_units)
        .then_with(|| cmp_bytes(&left.candidate_id, &right.candidate_id))
        .then_with(|| cmp_bytes(&left.left_surface_id, &right.left_surface_id))
        .then_with(|| cmp_bytes(&left.right_surface_id, &right.right_surface_id))
}

fn cmp_bytes(left: &str, right: &str) -> Ordering {
    left.as_bytes().cmp(right.as_bytes())
}
