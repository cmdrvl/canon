#![forbid(unsafe_code)]

//! Cannot-link evidence helpers for `canon entity edge`.
//!
//! These helpers convert profile/namekit anti-overmerge signals into hard
//! anti-merge edge hits. They never emit support-lane evidence.

use crate::entity::{
    edge::EdgeEvidenceHit,
    score::{ScoreLane, ScoreUnits},
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedTokenConflictRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub left_tokens: &'a [&'a str],
    pub right_tokens: &'a [&'a str],
    pub score_units: ScoreUnits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedDistinctPhraseRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub left_value: &'a str,
    pub right_value: &'a str,
    pub phrases: &'a [&'a str],
    pub score_units: ScoreUnits,
}

pub fn protected_token_conflict_hit(
    request: ProtectedTokenConflictRequest<'_>,
) -> Option<EdgeEvidenceHit> {
    let left = token_set(request.left_tokens);
    let right = token_set(request.right_tokens);
    if left.is_empty() && right.is_empty() {
        return None;
    }
    if left == right {
        return None;
    }

    let left_only = sorted_difference(&left, &right);
    let right_only = sorted_difference(&right, &left);

    Some(EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        request.namespace,
        request.operator_id,
        request.reason_code,
        request.score_units,
        true,
        format!(
            "protected token conflict left_only={} right_only={} score_units={}",
            list_or_none(&left_only),
            list_or_none(&right_only),
            request.score_units.as_u32()
        ),
    ))
}

pub fn related_distinct_phrase_hit(
    request: RelatedDistinctPhraseRequest<'_>,
) -> Option<EdgeEvidenceHit> {
    let left = request.left_value.to_ascii_lowercase();
    let right = request.right_value.to_ascii_lowercase();
    let mut matched = request
        .phrases
        .iter()
        .map(|phrase| phrase.trim())
        .filter(|phrase| !phrase.is_empty())
        .filter(|phrase| left.contains(phrase) ^ right.contains(phrase))
        .collect::<Vec<_>>();
    matched.sort_unstable();
    matched.dedup();

    if matched.is_empty() {
        return None;
    }

    Some(EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        request.namespace,
        request.operator_id,
        request.reason_code,
        request.score_units,
        true,
        format!(
            "related distinct phrase matched={} score_units={}",
            matched.join("|"),
            request.score_units.as_u32()
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

fn sorted_difference(left: &BTreeSet<&str>, right: &BTreeSet<&str>) -> Vec<String> {
    left.difference(right)
        .map(|token| (*token).to_string())
        .collect()
}

fn list_or_none(tokens: &[String]) -> String {
    if tokens.is_empty() {
        "none".to_string()
    } else {
        tokens.join("|")
    }
}
