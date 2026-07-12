#![forbid(unsafe_code)]

//! Edge-stage preflight checks.
//!
//! The scorer must never start from stale or over-budget candidate artifacts.
//! This module keeps that boundary explicit: callers get either a small permit
//! to score or a normal canon refusal envelope before any edge artifact exists.

use crate::{
    Refusal,
    entity::{
        budget::{BudgetLimit, BudgetStage, find_budget_policy},
        contracts::{CANON_ENTITY_BLOCK_VERSION, CANON_ENTITY_EDGE_VERSION},
        error::EntityRefusalKind,
        score::{ScoreBreakdown, ScoreContribution, ScoreLane, ScoreUnits, accumulate_score_units},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;

pub const EDGE_STAGE: &str = "edge";
pub const EDGE_CANDIDATE_ARTIFACT: &str = "candidate_artifact";
pub const EDGE_PARTIAL_ARTIFACT_WRITTEN_ON_REFUSAL: bool = false;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EdgeCandidateArtifactRef {
    pub version: String,
    pub profile_id: String,
    pub profile_version: String,
    pub strategy_hash: String,
    pub registry_snapshot_hash: String,
    pub content_hash: String,
    pub candidate_record_count: u64,
    pub candidate_budget: EdgeCandidateBudgetProof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EdgeCandidateBudgetProof {
    pub validated: bool,
    pub policy_id: String,
    pub observed: u64,
    pub configured: u64,
}

impl EdgeCandidateBudgetProof {
    pub fn within_run_budget(observed: u64, configured: u64) -> Self {
        Self {
            validated: true,
            policy_id: "block.max_candidates_per_run".to_string(),
            observed,
            configured,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EdgeCandidateArtifactExpectation {
    pub profile_id: String,
    pub profile_version: String,
    pub strategy_hash: String,
    pub registry_snapshot_hash: String,
    pub content_hash: String,
    pub max_edge_records: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeScoringPermit {
    pub candidate_record_count: u64,
    pub max_edge_records: u64,
    pub partial_edge_artifact_written: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeEvidenceRecord {
    pub version: String,
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub hits: Vec<EdgeEvidenceHit>,
    pub pair_score_total: ScoreUnits,
    pub score_breakdown: ScoreBreakdown,
    pub has_hard_cannot_link: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeEvidenceHit {
    pub lane: ScoreLane,
    pub namespace: String,
    pub operator_id: String,
    pub reason_code: String,
    pub score_units: ScoreUnits,
    pub hard_cannot_link: bool,
    pub explanation: String,
}

impl EdgeEvidenceHit {
    pub fn new(
        lane: ScoreLane,
        namespace: impl Into<String>,
        operator_id: impl Into<String>,
        reason_code: impl Into<String>,
        score_units: ScoreUnits,
        hard_cannot_link: bool,
        explanation: impl Into<String>,
    ) -> Self {
        Self {
            lane,
            namespace: namespace.into(),
            operator_id: operator_id.into(),
            reason_code: reason_code.into(),
            score_units,
            hard_cannot_link,
            explanation: explanation.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityEvidenceStageRequest<'a> {
    pub rows: &'a Path,
    pub profile: &'a str,
    pub strategy: &'a Path,
    pub candidates: &'a Path,
    pub registry: &'a Path,
    pub work_dir: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityEvidenceStageOutput {
    pub artifact: crate::entity::edge_artifact::EdgeEvidenceArtifact,
    pub records: Vec<EdgeEvidenceRecord>,
    pub candidate_records: Vec<crate::entity::block::BlockCandidateRecord>,
    pub exact_buckets: Vec<crate::entity::block_artifact::ExactBucketAssertion>,
}

pub fn build_edge_evidence_record(
    left_surface_id: impl Into<String>,
    right_surface_id: impl Into<String>,
    hits: Vec<EdgeEvidenceHit>,
) -> Result<EdgeEvidenceRecord, Refusal> {
    let left_surface_id = left_surface_id.into();
    let right_surface_id = right_surface_id.into();
    validate_surface_pair(&left_surface_id, &right_surface_id)?;

    let mut hits = hits;
    for hit in &hits {
        validate_edge_hit(hit)?;
    }
    hits.sort_by(edge_evidence_hit_cmp);

    let score_breakdown = accumulate_score_units(hits.iter().map(|hit| {
        ScoreContribution::new(
            hit.lane,
            format!("{}:{}", hit.namespace, hit.operator_id),
            hit.reason_code.clone(),
            hit.score_units,
        )
    }));
    let pair_score_total = score_breakdown.total_score_units;
    let has_hard_cannot_link = hits
        .iter()
        .any(|hit| hit.lane == ScoreLane::AntiMerge && hit.hard_cannot_link);

    Ok(EdgeEvidenceRecord {
        version: CANON_ENTITY_EDGE_VERSION.to_string(),
        left_surface_id,
        right_surface_id,
        hits,
        pair_score_total,
        score_breakdown,
        has_hard_cannot_link,
    })
}

pub fn validate_edge_candidate_artifact_before_scoring(
    artifact: &EdgeCandidateArtifactRef,
    expected: &EdgeCandidateArtifactExpectation,
) -> Result<EdgeScoringPermit, Refusal> {
    if artifact.version != CANON_ENTITY_BLOCK_VERSION {
        return Err(artifact_contract_refusal(
            "Candidate artifact has the wrong entity contract version",
            json!({
                "stage": EDGE_STAGE,
                "artifact": EDGE_CANDIDATE_ARTIFACT,
                "reason": "wrong_version",
                "expected_version": CANON_ENTITY_BLOCK_VERSION,
                "actual_version": artifact.version,
                "partial_edge_artifact_written": EDGE_PARTIAL_ARTIFACT_WRITTEN_ON_REFUSAL
            }),
        ));
    }

    if let Some(field) = first_missing_artifact_field(artifact) {
        return Err(artifact_contract_refusal(
            "Candidate artifact is missing required edge input metadata",
            json!({
                "stage": EDGE_STAGE,
                "artifact": EDGE_CANDIDATE_ARTIFACT,
                "reason": "missing_field",
                "field": field,
                "partial_edge_artifact_written": EDGE_PARTIAL_ARTIFACT_WRITTEN_ON_REFUSAL
            }),
        ));
    }

    if let Some(refusal) = stale_artifact_refusal(artifact, expected) {
        return Err(refusal);
    }

    if !artifact.candidate_budget.validated {
        return Err(candidate_budget_refusal(
            "Candidate budget proof is missing from the upstream block artifact",
            "candidate_budget_not_validated",
            &artifact.candidate_budget,
        ));
    }

    if artifact.candidate_budget.observed > artifact.candidate_budget.configured {
        return Err(candidate_budget_refusal(
            "Candidate artifact records a block-stage budget breach",
            "candidate_budget_exceeded",
            &artifact.candidate_budget,
        ));
    }

    if artifact.candidate_record_count > expected.max_edge_records {
        let policy = find_budget_policy(BudgetStage::Edge, BudgetLimit::MaxEdgeRecords)
            .expect("edge max_edge_records policy is defined");
        let breach = policy.breach(artifact.candidate_record_count, expected.max_edge_records);
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Edge record budget exceeded before scoring",
            json!({
                "stage": EDGE_STAGE,
                "artifact": EDGE_CANDIDATE_ARTIFACT,
                "reason": "edge_record_budget_exceeded",
                "budget": breach,
                "partial_edge_artifact_written": EDGE_PARTIAL_ARTIFACT_WRITTEN_ON_REFUSAL
            }),
            Some(policy.next_command.to_string()),
        ));
    }

    Ok(EdgeScoringPermit {
        candidate_record_count: artifact.candidate_record_count,
        max_edge_records: expected.max_edge_records,
        partial_edge_artifact_written: false,
    })
}

fn first_missing_artifact_field(artifact: &EdgeCandidateArtifactRef) -> Option<&'static str> {
    [
        ("profile_id", artifact.profile_id.as_str()),
        ("profile_version", artifact.profile_version.as_str()),
        ("strategy_hash", artifact.strategy_hash.as_str()),
        (
            "registry_snapshot_hash",
            artifact.registry_snapshot_hash.as_str(),
        ),
        ("content_hash", artifact.content_hash.as_str()),
    ]
    .into_iter()
    .find_map(|(field, value)| value.trim().is_empty().then_some(field))
}

fn stale_artifact_refusal(
    artifact: &EdgeCandidateArtifactRef,
    expected: &EdgeCandidateArtifactExpectation,
) -> Option<Refusal> {
    for (field, expected_value, actual_value) in [
        (
            "profile_id",
            expected.profile_id.as_str(),
            artifact.profile_id.as_str(),
        ),
        (
            "profile_version",
            expected.profile_version.as_str(),
            artifact.profile_version.as_str(),
        ),
        (
            "strategy_hash",
            expected.strategy_hash.as_str(),
            artifact.strategy_hash.as_str(),
        ),
        (
            "registry_snapshot_hash",
            expected.registry_snapshot_hash.as_str(),
            artifact.registry_snapshot_hash.as_str(),
        ),
        (
            "content_hash",
            expected.content_hash.as_str(),
            artifact.content_hash.as_str(),
        ),
    ] {
        if expected_value != actual_value {
            return Some(artifact_contract_refusal(
                "Candidate artifact does not match the current edge run inputs",
                json!({
                    "stage": EDGE_STAGE,
                    "artifact": EDGE_CANDIDATE_ARTIFACT,
                    "reason": "stale_artifact",
                    "field": field,
                    "expected": expected_value,
                    "actual": actual_value,
                    "partial_edge_artifact_written": EDGE_PARTIAL_ARTIFACT_WRITTEN_ON_REFUSAL
                }),
            ));
        }
    }
    None
}

fn artifact_contract_refusal(message: &'static str, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        detail,
        Some(
            "Use the matching canon_entity_block.v0 candidate artifact or rerun canon entity block"
                .to_string(),
        ),
    )
}

fn validate_surface_pair(left_surface_id: &str, right_surface_id: &str) -> Result<(), Refusal> {
    if left_surface_id.trim().is_empty()
        || right_surface_id.trim().is_empty()
        || left_surface_id >= right_surface_id
    {
        return Err(artifact_contract_refusal(
            "Edge evidence surface IDs must be a deterministic non-empty pair",
            json!({
                "stage": EDGE_STAGE,
                "reason": "invalid_surface_pair",
                "left_surface_id": left_surface_id,
                "right_surface_id": right_surface_id,
                "partial_edge_artifact_written": EDGE_PARTIAL_ARTIFACT_WRITTEN_ON_REFUSAL
            }),
        ));
    }
    Ok(())
}

fn validate_edge_hit(hit: &EdgeEvidenceHit) -> Result<(), Refusal> {
    for (field, value) in [
        ("namespace", hit.namespace.as_str()),
        ("operator_id", hit.operator_id.as_str()),
        ("reason_code", hit.reason_code.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(artifact_contract_refusal(
                "Edge evidence hit is missing required metadata",
                json!({
                    "stage": EDGE_STAGE,
                    "reason": "missing_evidence_hit_field",
                    "field": field,
                    "partial_edge_artifact_written": EDGE_PARTIAL_ARTIFACT_WRITTEN_ON_REFUSAL
                }),
            ));
        }
    }
    Ok(())
}

fn edge_evidence_hit_cmp(left: &EdgeEvidenceHit, right: &EdgeEvidenceHit) -> std::cmp::Ordering {
    left.lane
        .cmp(&right.lane)
        .then_with(|| left.namespace.cmp(&right.namespace))
        .then_with(|| left.operator_id.cmp(&right.operator_id))
        .then_with(|| left.reason_code.cmp(&right.reason_code))
        .then_with(|| right.score_units.cmp(&left.score_units))
        .then_with(|| right.hard_cannot_link.cmp(&left.hard_cannot_link))
        .then_with(|| left.explanation.cmp(&right.explanation))
}

fn candidate_budget_refusal(
    message: &'static str,
    reason: &'static str,
    proof: &EdgeCandidateBudgetProof,
) -> Refusal {
    let policy = find_budget_policy(BudgetStage::Block, BudgetLimit::MaxCandidatesPerRun)
        .expect("block max_candidates_per_run policy is defined");
    let policy_id = if proof.policy_id.trim().is_empty() {
        policy.id
    } else {
        proof.policy_id.as_str()
    };

    EntityRefusalKind::CandidateBudget.to_refusal(
        message,
        json!({
            "stage": EDGE_STAGE,
            "upstream_stage": "block",
            "artifact": EDGE_CANDIDATE_ARTIFACT,
            "reason": reason,
            "policy_id": policy_id,
            "observed": proof.observed,
            "configured": proof.configured,
            "enforcement": "refuse_before_scoring",
            "partial_edge_artifact_written": EDGE_PARTIAL_ARTIFACT_WRITTEN_ON_REFUSAL
        }),
        None,
    )
}
