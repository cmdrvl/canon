#![forbid(unsafe_code)]

//! Relation-hint evidence helpers for `canon entity edge`.
//!
//! Relation hints describe related-but-not-same context for review and ontology
//! consumers. They deliberately emit `ScoreLane::RelationHint`, so they never
//! add positive merge score.

use crate::entity::{
    edge::EdgeEvidenceHit,
    score::{ScoreLane, ScoreUnits},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationHintRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub relation: &'a str,
    pub left_value: &'a str,
    pub right_value: &'a str,
    pub score_units: ScoreUnits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationPatchHintRequest<'a> {
    pub namespace: &'a str,
    pub operator_id: &'a str,
    pub reason_code: &'a str,
    pub relation: &'a str,
    pub patch_id: &'a str,
    pub source_patch_namespace: &'a str,
    pub target_patch_namespace: &'a str,
    pub score_units: ScoreUnits,
}

pub fn relation_hint_hit(request: RelationHintRequest<'_>) -> Option<EdgeEvidenceHit> {
    let relation = relation_hint_name(request.relation)?;
    let left = request.left_value.trim();
    let right = request.right_value.trim();
    if left.is_empty() || right.is_empty() {
        return None;
    }

    Some(EdgeEvidenceHit::new(
        ScoreLane::RelationHint,
        request.namespace,
        request.operator_id,
        request.reason_code,
        request.score_units,
        false,
        format!(
            "relation hint relation={} left={} right={} handoff=review_and_ontology score_units={}",
            relation,
            left,
            right,
            request.score_units.as_u32()
        ),
    ))
}

pub fn relation_patch_hint_hit(request: RelationPatchHintRequest<'_>) -> Option<EdgeEvidenceHit> {
    let relation = relation_hint_name(request.relation)?;
    let patch_id = request.patch_id.trim();
    let source = request.source_patch_namespace.trim();
    let target = request.target_patch_namespace.trim();
    if patch_id.is_empty() || source.is_empty() || target.is_empty() {
        return None;
    }

    Some(EdgeEvidenceHit::new(
        ScoreLane::RelationHint,
        request.namespace,
        request.operator_id,
        request.reason_code,
        request.score_units,
        false,
        format!(
            "relation patch patch_id={} relation={} source_patch_namespace={} target_patch_namespace={} handoff=ontology score_units={}",
            patch_id,
            relation,
            source,
            target,
            request.score_units.as_u32()
        ),
    ))
}

fn relation_hint_name(relation: &str) -> Option<String> {
    let relation = relation.trim();
    if relation.is_empty() {
        return None;
    }
    let normalized = relation.replace('-', "_").to_ascii_lowercase();
    if normalized == "same_as" || normalized == "sameas" {
        return None;
    }
    Some(normalized)
}
