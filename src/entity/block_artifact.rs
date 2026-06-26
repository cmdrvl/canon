#![forbid(unsafe_code)]

use crate::{
    Refusal,
    entity::{
        CANON_ENTITY_BLOCK_VERSION, CANON_ENTITY_INDEX_VERSION, CANON_ENTITY_PREPARE_VERSION,
        EntityArtifactHeader, EntityArtifactMetadata, EntityArtifactReference,
        EntityDeterministicSummary, EntityStrategyReference,
        block::{BlockCandidateGenerationDiagnostics, BlockCandidateRecord},
        error::EntityRefusalKind,
    },
    witness,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub const EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN: &str = "forbidden";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCandidateArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub metadata: EntityArtifactMetadata,
    pub summary: EntityDeterministicSummary,
    pub upstream_artifacts: Vec<EntityArtifactReference>,
    pub candidate_records_path: String,
    pub candidate_records_hash: String,
    pub bucket_assertions_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockCandidateArtifactRequest {
    pub index: EntityArtifactHeader,
    pub strategy: EntityStrategyReference,
    pub candidate_records_path: String,
    pub candidate_records: Vec<BlockCandidateRecord>,
    pub bucket_assertions: Vec<ExactBucketAssertion>,
    pub known_surface_ids: Vec<String>,
    pub diagnostics: BlockCandidateGenerationDiagnostics,
}

pub fn build_block_candidate_artifact_contract(
    request: BlockCandidateArtifactRequest,
) -> Result<BlockCandidateArtifact, Refusal> {
    validate_index_header(&request.index)?;
    validate_candidate_records(&request.candidate_records, &request.known_surface_ids)?;
    let prepare_hash = prepare_hash_from_index(&request.index)?;
    validate_bucket_assertions(
        &request.bucket_assertions,
        &request.known_surface_ids,
        prepare_hash,
        &request.index.metadata.artifact_content_hash,
        &request.strategy.content_hash,
        &request
            .index
            .metadata
            .registry_snapshot
            .lookup_snapshot_hash,
        &profile_from_index(&request.index)?,
    )?;
    if request.candidate_records_path.trim().is_empty() {
        return Err(block_artifact_refusal(
            "Block artifact candidate records path is required",
            json!({
                "stage": "block",
                "field": "candidate_records_path",
                "writes_performed": false
            }),
        ));
    }

    let mut upstream_artifacts = request.index.metadata.upstream_artifacts.clone();
    upstream_artifacts.push(EntityArtifactReference {
        version: request.index.version.clone(),
        content_hash: request.index.metadata.artifact_content_hash.clone(),
    });
    let mut metadata = metadata_from_index(&request.index, request.strategy);
    metadata.upstream_artifacts = upstream_artifacts.clone();
    let summary = block_summary(
        &request.candidate_records,
        &request.bucket_assertions,
        &request.diagnostics,
    )?;
    let candidate_records_hash = hash_jsonl_records(&request.candidate_records)?;
    let bucket_assertions_hash = hash_jsonl_records(&request.bucket_assertions)?;

    let mut artifact = BlockCandidateArtifact {
        version: CANON_ENTITY_BLOCK_VERSION.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        summary,
        upstream_artifacts,
        candidate_records_path: request.candidate_records_path,
        candidate_records_hash,
        bucket_assertions_hash,
    };
    artifact.artifact_content_hash = hash_block_artifact_without_self(&artifact)?;
    artifact.metadata.artifact_content_hash = artifact.artifact_content_hash.clone();
    Ok(artifact)
}

pub fn validate_block_candidate_artifact_contract(
    artifact: &BlockCandidateArtifact,
) -> Result<(), Refusal> {
    if artifact.version != CANON_ENTITY_BLOCK_VERSION {
        return Err(block_artifact_refusal(
            "Block artifact version mismatch",
            json!({
                "stage": "block",
                "expected": CANON_ENTITY_BLOCK_VERSION,
                "actual": artifact.version,
                "writes_performed": false
            }),
        ));
    }
    if artifact.candidate_records_path.trim().is_empty() {
        return Err(block_artifact_refusal(
            "Block artifact candidate records path is required",
            json!({
                "stage": "block",
                "field": "candidate_records_path",
                "writes_performed": false
            }),
        ));
    }
    if artifact.candidate_records_hash.trim().is_empty() {
        return Err(block_artifact_refusal(
            "Block artifact candidate records hash is required",
            json!({
                "stage": "block",
                "field": "candidate_records_hash",
                "writes_performed": false
            }),
        ));
    }
    if artifact.bucket_assertions_hash.trim().is_empty() {
        return Err(block_artifact_refusal(
            "Block artifact bucket assertion hash is required",
            json!({
                "stage": "block",
                "field": "bucket_assertions_hash",
                "writes_performed": false
            }),
        ));
    }
    if artifact.upstream_artifacts.is_empty() {
        return Err(block_artifact_refusal(
            "Block artifact must record upstream index artifact",
            json!({
                "stage": "block",
                "field": "upstream_artifacts",
                "writes_performed": false
            }),
        ));
    }
    if !artifact
        .upstream_artifacts
        .iter()
        .any(|reference| reference.version == CANON_ENTITY_PREPARE_VERSION)
    {
        return Err(block_artifact_refusal(
            "Block artifact must record upstream prepare artifact",
            json!({
                "stage": "block",
                "field": "upstream_artifacts",
                "expected": CANON_ENTITY_PREPARE_VERSION,
                "writes_performed": false
            }),
        ));
    }
    if !artifact
        .upstream_artifacts
        .iter()
        .any(|reference| reference.version == CANON_ENTITY_INDEX_VERSION)
    {
        return Err(block_artifact_refusal(
            "Block artifact must record upstream index artifact",
            json!({
                "stage": "block",
                "field": "upstream_artifacts",
                "expected": CANON_ENTITY_INDEX_VERSION,
                "writes_performed": false
            }),
        ));
    }
    if artifact.metadata.artifact_content_hash != artifact.artifact_content_hash {
        return Err(block_artifact_refusal(
            "Block artifact metadata hash does not match artifact hash",
            json!({
                "stage": "block",
                "field": "metadata.artifact_content_hash",
                "expected": artifact.artifact_content_hash,
                "actual": artifact.metadata.artifact_content_hash,
                "writes_performed": false
            }),
        ));
    }
    let expected = hash_block_artifact_without_self(artifact)?;
    if artifact.artifact_content_hash != expected {
        return Err(block_artifact_refusal(
            "Block artifact content hash mismatch",
            json!({
                "stage": "block",
                "field": "artifact_content_hash",
                "expected": expected,
                "actual": artifact.artifact_content_hash,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn validate_index_header(index: &EntityArtifactHeader) -> Result<(), Refusal> {
    if index.version != CANON_ENTITY_INDEX_VERSION {
        return Err(block_artifact_refusal(
            "Block artifact requires a canon_entity_index.v0 upstream artifact",
            json!({
                "stage": "block",
                "expected": CANON_ENTITY_INDEX_VERSION,
                "actual": index.version,
                "writes_performed": false
            }),
        ));
    }
    if index.metadata.artifact_content_hash.trim().is_empty() {
        return Err(block_artifact_refusal(
            "Block artifact upstream index hash is required",
            json!({
                "stage": "block",
                "field": "index.metadata.artifact_content_hash",
                "writes_performed": false
            }),
        ));
    }
    let _ = prepare_hash_from_index(index)?;
    if index
        .metadata
        .registry_snapshot
        .lookup_snapshot_hash
        .trim()
        .is_empty()
    {
        return Err(block_artifact_refusal(
            "Block artifact registry snapshot hash is required",
            json!({
                "stage": "block",
                "field": "index.metadata.registry_snapshot.lookup_snapshot_hash",
                "writes_performed": false
            }),
        ));
    }
    let _ = profile_from_index(index)?;
    Ok(())
}

fn validate_candidate_records(
    candidates: &[BlockCandidateRecord],
    known_surface_ids: &[String],
) -> Result<(), Refusal> {
    let known_surface_ids = known_surface_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut seen_pairs = BTreeSet::new();
    for candidate in candidates {
        if candidate.version != CANON_ENTITY_BLOCK_VERSION {
            return Err(block_artifact_refusal(
                "Candidate record version mismatch",
                json!({
                    "stage": "block",
                    "expected": CANON_ENTITY_BLOCK_VERSION,
                    "actual": candidate.version,
                    "writes_performed": false
                }),
            ));
        }
        if candidate.left_surface_id.trim().is_empty()
            || candidate.right_surface_id.trim().is_empty()
            || candidate.left_surface_id >= candidate.right_surface_id
        {
            return Err(block_artifact_refusal(
                "Candidate record surface IDs must be non-empty canonical pairs",
                json!({
                    "stage": "block",
                    "reason": "malformed_candidate_pair",
                    "left_surface_id": candidate.left_surface_id,
                    "right_surface_id": candidate.right_surface_id,
                    "writes_performed": false
                }),
            ));
        }
        let pair_key = (
            candidate.left_surface_id.as_str(),
            candidate.right_surface_id.as_str(),
        );
        if !seen_pairs.insert(pair_key) {
            return Err(block_artifact_refusal(
                "Candidate artifact contains a duplicate surface pair",
                json!({
                    "stage": "block",
                    "reason": "duplicate_candidate_pair",
                    "left_surface_id": candidate.left_surface_id,
                    "right_surface_id": candidate.right_surface_id,
                    "writes_performed": false
                }),
            ));
        }
        for surface_id in [&candidate.left_surface_id, &candidate.right_surface_id] {
            if !known_surface_ids.contains(surface_id.as_str()) {
                return Err(block_artifact_refusal(
                    "Candidate record references an unknown prepared surface",
                    json!({
                        "stage": "block",
                        "reason": "unknown_surface_id",
                        "surface_id": surface_id,
                        "writes_performed": false
                    }),
                ));
            }
        }
        validate_block_hits(candidate)?;
    }
    for pair in candidates.windows(2) {
        if block_candidate_record_cmp(&pair[0], &pair[1]).is_gt() {
            return Err(block_artifact_refusal(
                "Candidate artifact records are not in deterministic block order",
                json!({
                    "stage": "block",
                    "reason": "unstable_candidate_order",
                    "left_surface_id": pair[1].left_surface_id,
                    "right_surface_id": pair[1].right_surface_id,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(())
}

fn validate_block_hits(candidate: &BlockCandidateRecord) -> Result<(), Refusal> {
    if candidate.block_hits.is_empty() {
        return Err(block_artifact_refusal(
            "Candidate record must carry at least one block hit",
            json!({
                "stage": "block",
                "reason": "missing_block_hits",
                "left_surface_id": candidate.left_surface_id,
                "right_surface_id": candidate.right_surface_id,
                "writes_performed": false
            }),
        ));
    }

    let mut max_score_units = 0;
    let mut seen_operator_ids = BTreeSet::new();
    for hit in &candidate.block_hits {
        if hit.operator_id.trim().is_empty() {
            return Err(block_artifact_refusal(
                "Candidate block hit operator ID is required",
                json!({
                    "stage": "block",
                    "reason": "missing_block_hit_operator_id",
                    "left_surface_id": candidate.left_surface_id,
                    "right_surface_id": candidate.right_surface_id,
                    "writes_performed": false
                }),
            ));
        }
        if !seen_operator_ids.insert(hit.operator_id.as_str()) {
            return Err(block_artifact_refusal(
                "Candidate block hits must be unique by operator",
                json!({
                    "stage": "block",
                    "reason": "duplicate_block_hit_operator",
                    "operator_id": hit.operator_id,
                    "left_surface_id": candidate.left_surface_id,
                    "right_surface_id": candidate.right_surface_id,
                    "writes_performed": false
                }),
            ));
        }
        max_score_units = max_score_units.max(hit.score_units);
    }

    if candidate
        .block_hits
        .windows(2)
        .any(|pair| pair[0].operator_id >= pair[1].operator_id)
    {
        return Err(block_artifact_refusal(
            "Candidate block hits are not in deterministic operator order",
            json!({
                "stage": "block",
                "reason": "unstable_block_hit_order",
                "left_surface_id": candidate.left_surface_id,
                "right_surface_id": candidate.right_surface_id,
                "writes_performed": false
            }),
        ));
    }

    if candidate.candidate_score_hint != max_score_units {
        return Err(block_artifact_refusal(
            "Candidate score hint must equal the highest block hit score",
            json!({
                "stage": "block",
                "reason": "invalid_candidate_score_hint",
                "expected": max_score_units,
                "actual": candidate.candidate_score_hint,
                "left_surface_id": candidate.left_surface_id,
                "right_surface_id": candidate.right_surface_id,
                "writes_performed": false
            }),
        ));
    }

    Ok(())
}

fn validate_bucket_assertions(
    bucket_assertions: &[ExactBucketAssertion],
    known_surface_ids: &[String],
    prepare_hash: &str,
    index_hash: &str,
    strategy_hash: &str,
    registry_snapshot_hash: &str,
    profile: &ExactBucketProfile,
) -> Result<(), Refusal> {
    let known_surface_ids = known_surface_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut seen_buckets = BTreeSet::new();
    for assertion in bucket_assertions {
        assertion.validate().map_err(|error| {
            block_artifact_refusal(
                "Exact bucket assertion failed block artifact validation",
                json!({
                    "stage": "block",
                    "reason": "invalid_exact_bucket_assertion",
                    "bucket_id": assertion.bucket_id,
                    "error": format!("{error:?}"),
                    "writes_performed": false
                }),
            )
        })?;
        let bucket_key = (
            assertion.bucket_id.as_str(),
            assertion.operator_id.as_str(),
            assertion.upstream.index_hash.as_str(),
        );
        if !seen_buckets.insert(bucket_key) {
            return Err(block_artifact_refusal(
                "Block artifact contains a duplicate exact bucket assertion",
                json!({
                    "stage": "block",
                    "reason": "duplicate_exact_bucket_assertion",
                    "bucket_id": assertion.bucket_id,
                    "operator_id": assertion.operator_id,
                    "writes_performed": false
                }),
            ));
        }
        if assertion.profile != *profile {
            return Err(block_artifact_refusal(
                "Exact bucket assertion profile does not match block run",
                json!({
                    "stage": "block",
                    "reason": "profile_mismatch",
                    "bucket_id": assertion.bucket_id,
                    "expected": profile,
                    "actual": &assertion.profile,
                    "writes_performed": false
                }),
            ));
        }
        validate_bucket_surface_refs(assertion, &known_surface_ids)?;
        for (field, expected, actual) in [
            (
                "upstream.prepare_hash",
                prepare_hash,
                assertion.upstream.prepare_hash.as_str(),
            ),
            (
                "upstream.index_hash",
                index_hash,
                assertion.upstream.index_hash.as_str(),
            ),
            (
                "upstream.strategy_hash",
                strategy_hash,
                assertion.upstream.strategy_hash.as_str(),
            ),
            (
                "upstream.registry_snapshot_hash",
                registry_snapshot_hash,
                assertion.upstream.registry_snapshot_hash.as_str(),
            ),
        ] {
            if expected != actual {
                return Err(block_artifact_refusal(
                    "Exact bucket assertion upstream hash does not match block run",
                    json!({
                        "stage": "block",
                        "reason": "stale_exact_bucket_assertion",
                        "bucket_id": assertion.bucket_id,
                        "field": field,
                        "expected": expected,
                        "actual": actual,
                        "writes_performed": false
                    }),
                ));
            }
        }
        if assertion.expanded_pair_count() != 0 {
            return Err(block_artifact_refusal(
                "Exact bucket assertions must not expand pairwise candidates",
                json!({
                    "stage": "block",
                    "reason": "exact_bucket_pair_expansion",
                    "bucket_id": assertion.bucket_id,
                    "expanded_pair_count": assertion.expanded_pair_count(),
                    "writes_performed": false
                }),
            ));
        }
    }
    for pair in bucket_assertions.windows(2) {
        if exact_bucket_artifact_cmp(&pair[0], &pair[1]).is_gt() {
            return Err(block_artifact_refusal(
                "Exact bucket assertions are not in deterministic block order",
                json!({
                    "stage": "block",
                    "reason": "unstable_exact_bucket_order",
                    "bucket_id": pair[1].bucket_id,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(())
}

fn validate_bucket_surface_refs(
    assertion: &ExactBucketAssertion,
    known_surface_ids: &BTreeSet<&str>,
) -> Result<(), Refusal> {
    for surface_id in &assertion.membership.surface_ids {
        if !known_surface_ids.contains(surface_id.as_str()) {
            return Err(block_artifact_refusal(
                "Exact bucket assertion references an unknown prepared surface",
                json!({
                    "stage": "block",
                    "reason": "unknown_surface_id",
                    "bucket_id": assertion.bucket_id,
                    "surface_id": surface_id,
                    "writes_performed": false
                }),
            ));
        }
    }
    for range in &assertion.membership.surface_ranges {
        for surface_id in [&range.start_surface_id, &range.end_surface_id] {
            if !known_surface_ids.contains(surface_id.as_str()) {
                return Err(block_artifact_refusal(
                    "Exact bucket range references an unknown prepared surface",
                    json!({
                        "stage": "block",
                        "reason": "unknown_surface_id",
                        "bucket_id": assertion.bucket_id,
                        "surface_id": surface_id,
                        "writes_performed": false
                    }),
                ));
            }
        }
    }
    Ok(())
}

fn metadata_from_index(
    index: &EntityArtifactHeader,
    strategy: EntityStrategyReference,
) -> EntityArtifactMetadata {
    let metadata = &index.metadata;
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

fn block_summary(
    candidates: &[BlockCandidateRecord],
    bucket_assertions: &[ExactBucketAssertion],
    diagnostics: &BlockCandidateGenerationDiagnostics,
) -> Result<EntityDeterministicSummary, Refusal> {
    let block_hit_count = candidates
        .iter()
        .map(|candidate| candidate.block_hits.len() as u64)
        .sum::<u64>();
    let relation_hint_count = candidates
        .iter()
        .flat_map(|candidate| &candidate.block_hits)
        .filter(|hit| hit.operator_id.starts_with("relation_hint"))
        .count() as u64;
    Ok(EntityDeterministicSummary {
        counts: BTreeMap::from([
            ("candidate_pairs".to_string(), candidates.len() as u64),
            ("candidate_pair_count".to_string(), candidates.len() as u64),
            (
                "candidate_pairs_emitted".to_string(),
                diagnostics.candidate_pairs_emitted,
            ),
            (
                "candidate_pairs_suppressed_by_cap".to_string(),
                diagnostics.candidate_pairs_suppressed_by_cap,
            ),
            (
                "suppressed_candidate_count".to_string(),
                diagnostics.suppressed_candidate_count,
            ),
            ("block_hits".to_string(), block_hit_count),
            ("operator_hit_count".to_string(), block_hit_count),
            ("relation_hint_count".to_string(), relation_hint_count),
            (
                "exact_bucket_count".to_string(),
                bucket_assertions.len() as u64,
            ),
            (
                "exact_bucket_pair_expansion_count".to_string(),
                bucket_assertions
                    .iter()
                    .map(ExactBucketAssertion::expanded_pair_count)
                    .sum(),
            ),
            (
                "large_buckets_suppressed".to_string(),
                diagnostics.large_buckets_suppressed,
            ),
            (
                "candidate_artifact_bytes".to_string(),
                diagnostics.candidate_artifact_bytes,
            ),
            (
                "bucket_assertion_records".to_string(),
                bucket_assertions
                    .iter()
                    .map(ExactBucketAssertion::artifact_membership_record_count)
                    .sum(),
            ),
        ]),
        labels: BTreeMap::from([
            ("blocking".to_string(), "bounded".to_string()),
            (
                "upstream_version".to_string(),
                CANON_ENTITY_INDEX_VERSION.to_string(),
            ),
        ]),
    })
}

fn hash_jsonl_records<T: Serialize>(records: &[T]) -> Result<String, Refusal> {
    let mut bytes = Vec::new();
    for record in records {
        serde_json::to_writer(&mut bytes, record).map_err(|error| {
            block_artifact_refusal(
                "Failed to hash block JSONL artifact",
                json!({
                    "stage": "block",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
        bytes.push(b'\n');
    }
    Ok(witness::hash_bytes(&bytes))
}

fn hash_block_artifact_without_self(artifact: &BlockCandidateArtifact) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        block_artifact_refusal(
            "Failed to hash block artifact",
            json!({
                "stage": "block",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn prepare_hash_from_index(index: &EntityArtifactHeader) -> Result<&str, Refusal> {
    let Some(reference) = index
        .metadata
        .upstream_artifacts
        .iter()
        .find(|reference| reference.version == CANON_ENTITY_PREPARE_VERSION)
    else {
        return Err(block_artifact_refusal(
            "Block artifact requires a prepare artifact hash from the upstream index",
            json!({
                "stage": "block",
                "field": "index.metadata.upstream_artifacts",
                "expected": CANON_ENTITY_PREPARE_VERSION,
                "writes_performed": false
            }),
        ));
    };
    if reference.content_hash.trim().is_empty() {
        return Err(block_artifact_refusal(
            "Block artifact upstream prepare hash is required",
            json!({
                "stage": "block",
                "field": "index.metadata.upstream_artifacts.content_hash",
                "writes_performed": false
            }),
        ));
    }
    Ok(reference.content_hash.as_str())
}

fn profile_from_index(index: &EntityArtifactHeader) -> Result<ExactBucketProfile, Refusal> {
    let Some(content_hash) = index.metadata.profile.content_hash.as_deref() else {
        return Err(block_artifact_refusal(
            "Block artifact profile content hash is required",
            json!({
                "stage": "block",
                "field": "index.metadata.profile.content_hash",
                "writes_performed": false
            }),
        ));
    };
    if content_hash.trim().is_empty() {
        return Err(block_artifact_refusal(
            "Block artifact profile content hash is required",
            json!({
                "stage": "block",
                "field": "index.metadata.profile.content_hash",
                "writes_performed": false
            }),
        ));
    }
    Ok(ExactBucketProfile {
        id: index.metadata.profile.id.clone(),
        version: index.metadata.profile.version.clone(),
        identity_semantics: index.metadata.profile.identity_semantics.clone(),
        content_hash: content_hash.to_string(),
    })
}

fn block_candidate_record_cmp(
    left: &BlockCandidateRecord,
    right: &BlockCandidateRecord,
) -> std::cmp::Ordering {
    right
        .candidate_score_hint
        .cmp(&left.candidate_score_hint)
        .then_with(|| primary_operator_id(left).cmp(primary_operator_id(right)))
        .then_with(|| left.left_surface_id.cmp(&right.left_surface_id))
        .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
}

fn primary_operator_id(candidate: &BlockCandidateRecord) -> &str {
    candidate
        .block_hits
        .first()
        .map(|hit| hit.operator_id.as_str())
        .unwrap_or_default()
}

fn exact_bucket_artifact_cmp(
    left: &ExactBucketAssertion,
    right: &ExactBucketAssertion,
) -> std::cmp::Ordering {
    left.bucket_id
        .cmp(&right.bucket_id)
        .then_with(|| left.operator_id.cmp(&right.operator_id))
}

fn block_artifact_refusal(message: impl Into<String>, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        detail,
        Some("Use matching prepare/index artifacts or rerun canon entity block".to_string()),
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactBucketAssertion {
    pub version: String,
    pub bucket_id: String,
    pub operator_id: String,
    pub profile: ExactBucketProfile,
    pub upstream: ExactBucketUpstream,
    pub membership: ExactBucketMembership,
    pub row_count: u64,
    pub deal_count: u64,
    pub pair_expansion: String,
    pub diagnostics: ExactBucketDiagnostics,
    pub cannot_link_validation: CannotLinkValidationHook,
}

impl ExactBucketAssertion {
    pub fn validate(&self) -> Result<(), ExactBucketContractError> {
        if self.version != crate::entity::CANON_ENTITY_BLOCK_BUCKET_VERSION {
            return Err(ExactBucketContractError::WrongVersion {
                expected: crate::entity::CANON_ENTITY_BLOCK_BUCKET_VERSION,
                actual: self.version.clone(),
            });
        }
        if self.bucket_id.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField("bucket_id"));
        }
        if self.operator_id.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField("operator_id"));
        }
        self.profile.validate()?;
        self.upstream.validate()?;
        self.membership.validate()?;
        if self.row_count < self.membership.member_count() {
            return Err(ExactBucketContractError::RowCountBelowMembership {
                row_count: self.row_count,
                member_count: self.membership.member_count(),
            });
        }
        if self.pair_expansion != EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN {
            return Err(ExactBucketContractError::PairExpansionAllowed {
                actual: self.pair_expansion.clone(),
            });
        }
        self.cannot_link_validation.validate()?;
        Ok(())
    }

    pub fn expanded_pair_count(&self) -> u64 {
        0
    }

    pub fn theoretical_pair_count(&self) -> u64 {
        let member_count = self.membership.member_count();
        member_count.saturating_mul(member_count.saturating_sub(1)) / 2
    }

    pub fn artifact_membership_record_count(&self) -> u64 {
        self.membership.surface_ids.len() as u64 + self.membership.surface_ranges.len() as u64
    }

    pub fn requires_solver_cannot_link_veto(&self) -> bool {
        self.cannot_link_validation.hard_cannot_link_count > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactBucketProfile {
    pub id: String,
    pub version: String,
    pub identity_semantics: String,
    pub content_hash: String,
}

impl ExactBucketProfile {
    fn validate(&self) -> Result<(), ExactBucketContractError> {
        if self.id.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField("profile.id"));
        }
        if self.version.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField("profile.version"));
        }
        if self.identity_semantics.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField(
                "profile.identity_semantics",
            ));
        }
        if self.content_hash.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField(
                "profile.content_hash",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactBucketUpstream {
    pub prepare_hash: String,
    pub index_hash: String,
    pub strategy_hash: String,
    pub registry_snapshot_hash: String,
}

impl ExactBucketUpstream {
    fn validate(&self) -> Result<(), ExactBucketContractError> {
        for (field, value) in [
            ("upstream.prepare_hash", &self.prepare_hash),
            ("upstream.index_hash", &self.index_hash),
            ("upstream.strategy_hash", &self.strategy_hash),
            (
                "upstream.registry_snapshot_hash",
                &self.registry_snapshot_hash,
            ),
        ] {
            if value.trim().is_empty() {
                return Err(ExactBucketContractError::MissingField(field));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactBucketMembership {
    #[serde(default)]
    pub surface_ids: Vec<String>,
    #[serde(default)]
    pub surface_ranges: Vec<SurfaceIdRange>,
}

impl ExactBucketMembership {
    pub fn member_count(&self) -> u64 {
        self.surface_ids.len() as u64
            + self
                .surface_ranges
                .iter()
                .map(|range| range.member_count)
                .sum::<u64>()
    }

    fn validate(&self) -> Result<(), ExactBucketContractError> {
        if self.surface_ids.is_empty() && self.surface_ranges.is_empty() {
            return Err(ExactBucketContractError::EmptyMembership);
        }
        if self
            .surface_ids
            .iter()
            .any(|surface_id| surface_id.trim().is_empty())
        {
            return Err(ExactBucketContractError::EmptySurfaceId);
        }
        if self.surface_ids.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(ExactBucketContractError::UnsortedSurfaceIds);
        }
        for range in &self.surface_ranges {
            range.validate()?;
        }
        if self
            .surface_ranges
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            return Err(ExactBucketContractError::UnsortedSurfaceRanges);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SurfaceIdRange {
    pub start_surface_id: String,
    pub end_surface_id: String,
    pub member_count: u64,
}

impl SurfaceIdRange {
    fn validate(&self) -> Result<(), ExactBucketContractError> {
        if self.start_surface_id.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField(
                "surface_ranges.start_surface_id",
            ));
        }
        if self.end_surface_id.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField(
                "surface_ranges.end_surface_id",
            ));
        }
        if self.start_surface_id > self.end_surface_id {
            return Err(ExactBucketContractError::InvalidSurfaceRange {
                start_surface_id: self.start_surface_id.clone(),
                end_surface_id: self.end_surface_id.clone(),
            });
        }
        if self.member_count == 0 {
            return Err(ExactBucketContractError::EmptySurfaceRange);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExactBucketDiagnostics {
    pub largest_bucket_size: u64,
    pub suppressed_pair_count: u64,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CannotLinkValidationHook {
    pub status: CannotLinkValidationStatus,
    pub checked_fact_count: u64,
    pub hard_cannot_link_count: u64,
    pub action: CannotLinkAction,
}

impl CannotLinkValidationHook {
    fn validate(&self) -> Result<(), ExactBucketContractError> {
        if self.hard_cannot_link_count > 0 && self.action == CannotLinkAction::AllowMerge {
            return Err(ExactBucketContractError::CannotLinkAllowsMerge);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CannotLinkValidationStatus {
    NotChecked,
    CheckedNoConflicts,
    CheckedConflictsPresent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CannotLinkAction {
    AllowMerge,
    RequireSolverVeto,
    RequireReview,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExactBucketContractError {
    WrongVersion {
        expected: &'static str,
        actual: String,
    },
    MissingField(&'static str),
    EmptyMembership,
    EmptySurfaceId,
    UnsortedSurfaceIds,
    UnsortedSurfaceRanges,
    InvalidSurfaceRange {
        start_surface_id: String,
        end_surface_id: String,
    },
    EmptySurfaceRange,
    RowCountBelowMembership {
        row_count: u64,
        member_count: u64,
    },
    PairExpansionAllowed {
        actual: String,
    },
    CannotLinkAllowsMerge,
}
