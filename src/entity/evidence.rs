#![forbid(unsafe_code)]

//! Support-evidence helpers for `canon entity edge`.
//!
//! This module converts already-normalized views and namekit metric output into
//! edge hits. It deliberately emits only support-lane hits; anti-merge and
//! relation-hint evidence live in their own lanes.

use crate::{
    entity::{
        edge::EdgeEvidenceHit,
        score::{ScoreLane, ScoreUnits},
    },
    namekit::{
        SimilarityScore,
        similarity::{SimilarityMetric, SimilarityOptions, SimilarityPath, normalized_similarity},
    },
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactViewSupportRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub view_name: &'a str,
    pub left_value: &'a str,
    pub right_value: &'a str,
    pub score_units: ScoreUnits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenOverlapSupportRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub left_tokens: &'a [&'a str],
    pub right_tokens: &'a [&'a str],
    pub min_shared_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringSimilaritySupportRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub metric: SimilarityMetric,
    pub left_value: &'a str,
    pub right_value: &'a str,
    pub score_cutoff: Option<ScoreUnits>,
    pub score_hint: Option<ScoreUnits>,
}

pub fn exact_view_support_hit(request: ExactViewSupportRequest<'_>) -> Option<EdgeEvidenceHit> {
    let left = request.left_value.trim();
    let right = request.right_value.trim();
    if left.is_empty() || right.is_empty() || left != right {
        return None;
    }

    Some(EdgeEvidenceHit::new(
        ScoreLane::Support,
        request.namespace,
        request.operator_id,
        request.reason_code,
        request.score_units,
        false,
        format!(
            "exact view {} matched with score_units={}",
            request.view_name,
            request.score_units.as_u32()
        ),
    ))
}

pub fn token_overlap_support_hit(
    request: TokenOverlapSupportRequest<'_>,
) -> Option<EdgeEvidenceHit> {
    let left = token_set(request.left_tokens);
    let right = token_set(request.right_tokens);
    if left.is_empty() || right.is_empty() {
        return None;
    }

    let shared_tokens = left.intersection(&right).count();
    if shared_tokens == 0 || shared_tokens < request.min_shared_tokens {
        return None;
    }

    let denominator = left.len().max(right.len()) as u64;
    let score_units =
        ScoreUnits::from_ratio_parts(shared_tokens as u64, denominator).unwrap_or(ScoreUnits::ZERO);

    Some(EdgeEvidenceHit::new(
        ScoreLane::Support,
        request.namespace,
        request.operator_id,
        request.reason_code,
        score_units,
        false,
        format!(
            "token overlap shared_tokens={} score_units={}",
            shared_tokens,
            score_units.as_u32()
        ),
    ))
}

pub fn string_similarity_support_hit(
    request: StringSimilaritySupportRequest<'_>,
) -> Option<EdgeEvidenceHit> {
    let result = normalized_similarity(
        request.metric,
        request.left_value,
        request.right_value,
        SimilarityOptions::new(
            request.score_cutoff.map(score_units_to_namekit),
            request.score_hint.map(score_units_to_namekit),
        ),
    );
    let score_units = result.score.map(ScoreUnits::from)?;

    Some(EdgeEvidenceHit::new(
        ScoreLane::Support,
        request.namespace,
        request.operator_id,
        request.reason_code,
        score_units,
        false,
        format!(
            "string metric {} path={} score_units={}",
            metric_id(request.metric),
            path_id(result.path),
            score_units.as_u32()
        ),
    ))
}

fn token_set<'a>(tokens: &'a [&'a str]) -> BTreeSet<&'a str> {
    tokens
        .iter()
        .map(|token| token.trim())
        .filter(|token| !token.is_empty())
        .collect()
}

fn score_units_to_namekit(score_units: ScoreUnits) -> SimilarityScore {
    SimilarityScore::from_scaled(score_units.as_u32() as u16)
        .expect("entity score units share the namekit score scale")
}

fn metric_id(metric: SimilarityMetric) -> &'static str {
    match metric {
        SimilarityMetric::LevenshteinNormalized => "levenshtein_normalized",
        SimilarityMetric::JaroWinkler => "jaro_winkler",
        SimilarityMetric::DiceSorensen => "dice_sorensen",
        SimilarityMetric::TokenSortRatio => "token_sort_ratio",
        SimilarityMetric::TokenSetRatio => "token_set_ratio",
    }
}

fn path_id(path: SimilarityPath) -> &'static str {
    match path {
        SimilarityPath::AsciiBytes => "ascii_bytes",
        SimilarityPath::UnicodeChars => "unicode_chars",
    }
}
