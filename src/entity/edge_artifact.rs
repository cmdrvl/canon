#![forbid(unsafe_code)]

//! `canon_entity_edge.v0` artifact contract.

use crate::{
    Refusal,
    entity::{
        CANON_ENTITY_BLOCK_VERSION, CANON_ENTITY_EDGE_VERSION, EntityArtifactMetadata,
        EntityArtifactReference, EntityDeterministicSummary, EntityStrategyReference,
        block::BlockCandidateRecord,
        block_artifact::{
            BlockCandidateArtifact, ExactBucketAssertion,
            validate_block_candidate_artifact_contract,
        },
        edge::{EdgeEvidenceRecord, build_edge_evidence_record},
        error::EntityRefusalKind,
        score::ScoreLane,
    },
    witness,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeEvidenceArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub metadata: EntityArtifactMetadata,
    pub summary: EntityDeterministicSummary,
    pub upstream_artifacts: Vec<EntityArtifactReference>,
    pub edge_records_path: String,
    pub edge_records_hash: String,
    pub candidate_records_hash: String,
    pub bucket_assertions_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeEvidenceArtifactRequest {
    pub block: BlockCandidateArtifact,
    pub strategy: EntityStrategyReference,
    pub edge_records_path: String,
    pub edge_records: Vec<EdgeEvidenceRecord>,
    pub candidate_records: Vec<BlockCandidateRecord>,
    pub bucket_assertions: Vec<ExactBucketAssertion>,
}

pub fn build_edge_evidence_artifact_contract(
    request: EdgeEvidenceArtifactRequest,
) -> Result<EdgeEvidenceArtifact, Refusal> {
    validate_block_candidate_artifact_contract(&request.block)?;
    if request.edge_records_path.trim().is_empty() {
        return Err(edge_artifact_refusal(
            "Edge artifact records path is required",
            json!({
                "stage": "edge",
                "field": "edge_records_path",
                "writes_performed": false
            }),
        ));
    }

    let candidate_records_hash = hash_jsonl_records(&request.candidate_records)?;
    if candidate_records_hash != request.block.candidate_records_hash {
        return Err(edge_artifact_refusal(
            "Edge artifact candidate records do not match upstream block artifact",
            json!({
                "stage": "edge",
                "reason": "stale_candidate_records",
                "expected": request.block.candidate_records_hash,
                "actual": candidate_records_hash,
                "writes_performed": false
            }),
        ));
    }

    let bucket_assertions_hash = hash_jsonl_records(&request.bucket_assertions)?;
    if bucket_assertions_hash != request.block.bucket_assertions_hash {
        return Err(edge_artifact_refusal(
            "Edge artifact bucket assertions do not match upstream block artifact",
            json!({
                "stage": "edge",
                "reason": "stale_bucket_assertions",
                "expected": request.block.bucket_assertions_hash,
                "actual": bucket_assertions_hash,
                "writes_performed": false
            }),
        ));
    }
    validate_bucket_assertions(&request.bucket_assertions)?;
    validate_edge_records(&request.edge_records, &request.candidate_records)?;

    let edge_records_hash = hash_jsonl_records(&request.edge_records)?;
    let mut upstream_artifacts = request.block.metadata.upstream_artifacts.clone();
    upstream_artifacts.push(EntityArtifactReference {
        version: request.block.version.clone(),
        content_hash: request.block.artifact_content_hash.clone(),
    });
    let mut metadata = metadata_from_block(&request.block, request.strategy);
    metadata.upstream_artifacts = upstream_artifacts.clone();
    let summary = edge_summary(
        &request.edge_records,
        &request.candidate_records,
        &request.bucket_assertions,
    );

    let mut artifact = EdgeEvidenceArtifact {
        version: CANON_ENTITY_EDGE_VERSION.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        summary,
        upstream_artifacts,
        edge_records_path: request.edge_records_path,
        edge_records_hash,
        candidate_records_hash,
        bucket_assertions_hash,
    };
    artifact.artifact_content_hash = hash_edge_artifact_without_self(&artifact)?;
    artifact.metadata.artifact_content_hash = artifact.artifact_content_hash.clone();
    Ok(artifact)
}

pub fn validate_edge_evidence_artifact_contract(
    artifact: &EdgeEvidenceArtifact,
) -> Result<(), Refusal> {
    if artifact.version != CANON_ENTITY_EDGE_VERSION {
        return Err(edge_artifact_refusal(
            "Edge artifact version mismatch",
            json!({
                "stage": "edge",
                "expected": CANON_ENTITY_EDGE_VERSION,
                "actual": artifact.version,
                "writes_performed": false
            }),
        ));
    }
    for (field, value) in [
        ("edge_records_path", artifact.edge_records_path.as_str()),
        ("edge_records_hash", artifact.edge_records_hash.as_str()),
        (
            "candidate_records_hash",
            artifact.candidate_records_hash.as_str(),
        ),
        (
            "bucket_assertions_hash",
            artifact.bucket_assertions_hash.as_str(),
        ),
    ] {
        if value.trim().is_empty() {
            return Err(edge_artifact_refusal(
                "Edge artifact is missing a required field",
                json!({
                    "stage": "edge",
                    "field": field,
                    "writes_performed": false
                }),
            ));
        }
    }
    if !artifact
        .upstream_artifacts
        .iter()
        .any(|reference| reference.version == CANON_ENTITY_BLOCK_VERSION)
    {
        return Err(edge_artifact_refusal(
            "Edge artifact must record upstream block artifact",
            json!({
                "stage": "edge",
                "field": "upstream_artifacts",
                "expected": CANON_ENTITY_BLOCK_VERSION,
                "writes_performed": false
            }),
        ));
    }
    if artifact.metadata.artifact_content_hash != artifact.artifact_content_hash {
        return Err(edge_artifact_refusal(
            "Edge artifact metadata hash does not match artifact hash",
            json!({
                "stage": "edge",
                "field": "metadata.artifact_content_hash",
                "expected": artifact.artifact_content_hash,
                "actual": artifact.metadata.artifact_content_hash,
                "writes_performed": false
            }),
        ));
    }
    let expected = hash_edge_artifact_without_self(artifact)?;
    if artifact.artifact_content_hash != expected {
        return Err(edge_artifact_refusal(
            "Edge artifact content hash mismatch",
            json!({
                "stage": "edge",
                "field": "artifact_content_hash",
                "expected": expected,
                "actual": artifact.artifact_content_hash,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn validate_edge_records(
    edge_records: &[EdgeEvidenceRecord],
    candidate_records: &[BlockCandidateRecord],
) -> Result<(), Refusal> {
    let candidate_pairs = candidate_records
        .iter()
        .map(|candidate| {
            (
                candidate.left_surface_id.as_str(),
                candidate.right_surface_id.as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    let mut seen_pairs = BTreeSet::new();
    for record in edge_records {
        if record.version != CANON_ENTITY_EDGE_VERSION {
            return Err(edge_artifact_refusal(
                "Edge evidence record version mismatch",
                json!({
                    "stage": "edge",
                    "expected": CANON_ENTITY_EDGE_VERSION,
                    "actual": record.version,
                    "writes_performed": false
                }),
            ));
        }
        let pair_key = (
            record.left_surface_id.as_str(),
            record.right_surface_id.as_str(),
        );
        if !candidate_pairs.contains(&pair_key) {
            return Err(edge_artifact_refusal(
                "Edge evidence record references a pair absent from the block candidates",
                json!({
                    "stage": "edge",
                    "reason": "unknown_candidate_pair",
                    "left_surface_id": record.left_surface_id,
                    "right_surface_id": record.right_surface_id,
                    "writes_performed": false
                }),
            ));
        }
        if !seen_pairs.insert(pair_key) {
            return Err(edge_artifact_refusal(
                "Edge artifact contains duplicate evidence for a candidate pair",
                json!({
                    "stage": "edge",
                    "reason": "duplicate_edge_pair",
                    "left_surface_id": record.left_surface_id,
                    "right_surface_id": record.right_surface_id,
                    "writes_performed": false
                }),
            ));
        }
        let expected = build_edge_evidence_record(
            record.left_surface_id.clone(),
            record.right_surface_id.clone(),
            record.hits.clone(),
        )?;
        if &expected != record {
            return Err(edge_artifact_refusal(
                "Edge evidence record does not match canonical score or hit ordering",
                json!({
                    "stage": "edge",
                    "reason": "noncanonical_edge_record",
                    "left_surface_id": record.left_surface_id,
                    "right_surface_id": record.right_surface_id,
                    "writes_performed": false
                }),
            ));
        }
    }
    for pair in edge_records.windows(2) {
        if edge_record_cmp(&pair[0], &pair[1]).is_gt() {
            return Err(edge_artifact_refusal(
                "Edge evidence records are not in deterministic order",
                json!({
                    "stage": "edge",
                    "reason": "unstable_edge_order",
                    "left_surface_id": pair[1].left_surface_id,
                    "right_surface_id": pair[1].right_surface_id,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(())
}

fn validate_bucket_assertions(bucket_assertions: &[ExactBucketAssertion]) -> Result<(), Refusal> {
    for assertion in bucket_assertions {
        assertion.validate().map_err(|error| {
            edge_artifact_refusal(
                "Exact bucket assertion failed edge artifact validation",
                json!({
                    "stage": "edge",
                    "reason": "invalid_exact_bucket_assertion",
                    "bucket_id": assertion.bucket_id,
                    "error": format!("{error:?}"),
                    "writes_performed": false
                }),
            )
        })?;
        if assertion.expanded_pair_count() != 0 {
            return Err(edge_artifact_refusal(
                "Exact bucket assertions must remain compact through edge scoring",
                json!({
                    "stage": "edge",
                    "reason": "exact_bucket_pair_expansion",
                    "bucket_id": assertion.bucket_id,
                    "expanded_pair_count": assertion.expanded_pair_count(),
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(())
}

fn metadata_from_block(
    block: &BlockCandidateArtifact,
    strategy: EntityStrategyReference,
) -> EntityArtifactMetadata {
    let metadata = &block.metadata;
    EntityArtifactMetadata {
        profile: metadata.profile.clone(),
        strategy,
        registry_snapshot: metadata.registry_snapshot.clone(),
        patch_namespace: metadata.patch_namespace.clone(),
        input: metadata.input.clone(),
        upstream_artifacts: Vec::new(),
        patch_set: metadata.patch_set.clone(),
        namekit: metadata.namekit.clone(),
        artifact_content_hash: String::new(),
    }
}

fn edge_summary(
    edge_records: &[EdgeEvidenceRecord],
    candidate_records: &[BlockCandidateRecord],
    bucket_assertions: &[ExactBucketAssertion],
) -> EntityDeterministicSummary {
    let support_hit_count = lane_hit_count(edge_records, ScoreLane::Support);
    let cannot_link_hit_count = lane_hit_count(edge_records, ScoreLane::AntiMerge);
    let relation_hint_count = lane_hit_count(edge_records, ScoreLane::RelationHint);
    EntityDeterministicSummary {
        counts: BTreeMap::from([
            ("edge_records".to_string(), edge_records.len() as u64),
            ("edge_record_count".to_string(), edge_records.len() as u64),
            (
                "candidate_record_count".to_string(),
                candidate_records.len() as u64,
            ),
            (
                "exact_bucket_count".to_string(),
                bucket_assertions.len() as u64,
            ),
            (
                "edge_hit_count".to_string(),
                support_hit_count + cannot_link_hit_count + relation_hint_count,
            ),
            ("support_hit_count".to_string(), support_hit_count),
            ("cannot_link_hit_count".to_string(), cannot_link_hit_count),
            ("relation_hint_count".to_string(), relation_hint_count),
            (
                "hard_cannot_link_count".to_string(),
                edge_records
                    .iter()
                    .filter(|record| record.has_hard_cannot_link)
                    .count() as u64,
            ),
        ]),
        labels: BTreeMap::from([
            ("edge_scoring".to_string(), "bounded".to_string()),
            (
                "upstream_version".to_string(),
                CANON_ENTITY_BLOCK_VERSION.to_string(),
            ),
        ]),
    }
}

fn lane_hit_count(edge_records: &[EdgeEvidenceRecord], lane: ScoreLane) -> u64 {
    edge_records
        .iter()
        .flat_map(|record| &record.hits)
        .filter(|hit| hit.lane == lane)
        .count() as u64
}

fn hash_jsonl_records<T: Serialize>(records: &[T]) -> Result<String, Refusal> {
    let mut bytes = Vec::new();
    for record in records {
        serde_json::to_writer(&mut bytes, record).map_err(|error| {
            edge_artifact_refusal(
                "Failed to hash edge JSONL artifact",
                json!({
                    "stage": "edge",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
        bytes.push(b'\n');
    }
    Ok(witness::hash_bytes(&bytes))
}

fn hash_edge_artifact_without_self(artifact: &EdgeEvidenceArtifact) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        edge_artifact_refusal(
            "Failed to hash edge artifact",
            json!({
                "stage": "edge",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn edge_record_cmp(left: &EdgeEvidenceRecord, right: &EdgeEvidenceRecord) -> std::cmp::Ordering {
    left.left_surface_id
        .cmp(&right.left_surface_id)
        .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
}

fn edge_artifact_refusal(message: impl Into<String>, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        detail,
        Some("Use matching block candidates or rerun canon entity edge".to_string()),
    )
}
