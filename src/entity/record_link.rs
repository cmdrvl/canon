#![forbid(unsafe_code)]

//! Shared record-link core for native entity stages.
//!
//! This module is deliberately stage-neutral.  Block/evidence/run/link callers
//! should use these pure builders, then publish their own stage artifacts after
//! the returned sidecars and hashes validate.

use super::{
    evidence_ir::{
        EvidenceAuthorityBasis, EvidenceBooleanMeasurement, EvidenceBundle,
        EvidenceCategoricalMeasurement, EvidenceExtension, EvidenceExtensionValue, EvidenceKind,
        EvidenceMeasurement, EvidenceNumericMeasurement, EvidenceOperatorRef, EvidencePolicyRef,
        EvidenceProvenanceRef, EvidenceRecord, EvidenceScope, EvidenceTarget,
        canonical_bundle_bytes, canonicalize_bundle, canonicalize_record,
    },
    source_mapping::{
        RecordLinkAssignmentRef, RecordLinkComparisonView, RecordLinkInputRecord,
        RecordLinkInputSidecar, canonical_record_link_input_bytes,
        validate_record_link_input_sidecar,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::{Component, Path, PathBuf},
};

pub const RECORD_LINK_CANDIDATE_SET_VERSION: &str = "canon.entity.record_link_candidates.v1";
pub const ASSIGNMENT_ALIGNMENT_VERSION: &str = "canon.entity.assignment_alignment.v1";
pub const RECORD_LINK_EVIDENCE_PATH: &str = "evidence/record_link_evidence.json";
pub const ASSIGNMENT_ALIGNMENT_PATH: &str = "evidence/assignment_alignment.json";
pub const RECORD_LINK_OPERATOR_NAMESPACE: &str = "record_link";
pub const RECORD_LINK_OPERATOR_VERSION: &str = "canon.entity.record_link.operator.v1";
pub const DEFAULT_MAX_CANDIDATE_PAIRS: usize = 25_000;
pub const DEFAULT_MAX_CANDIDATES_PER_RECORD: usize = 25;

pub type RecordLinkResult<T> = Result<T, RecordLinkCoreError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordLinkCoreErrorCode {
    ArtifactContract,
    Io,
    Budget,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkCoreError {
    pub code: RecordLinkCoreErrorCode,
    pub message: String,
    pub stage: String,
    pub reason: String,
    pub writes_performed: bool,
}

impl RecordLinkCoreError {
    fn new(
        code: RecordLinkCoreErrorCode,
        stage: impl Into<String>,
        reason: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            stage: stage.into(),
            reason: reason.into(),
            writes_performed: false,
        }
    }
}

impl fmt::Display for RecordLinkCoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?}: {} [{}:{}]",
            self.code, self.message, self.stage, self.reason
        )
    }
}

impl Error for RecordLinkCoreError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkInputArtifactRef {
    pub path: String,
    pub version: String,
    pub content_hash: String,
    pub source_id: String,
    pub scope_id: String,
    pub profile_id: String,
    pub profile_digest: String,
    pub input_digest: String,
    pub source_mapping_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLinkInputSource {
    pub path: String,
    pub sidecar: RecordLinkInputSidecar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLinkLoadRequest<'a> {
    pub workspace_root: &'a Path,
    pub sidecar_paths: Vec<PathBuf>,
    pub expected_profile_id: Option<String>,
    pub expected_profile_digest: Option<String>,
    pub expected_scope_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLinkInputSet {
    pub refs: Vec<RecordLinkInputArtifactRef>,
    pub inputs: Vec<RecordLinkInputSource>,
    pub scope_id: String,
    pub profile_id: String,
    pub profile_digest: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RecordLinkEndpoint {
    pub source_id: String,
    pub record_id: String,
    pub surface_id: String,
    pub observation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment_ref: Option<RecordLinkAssignmentRef>,
    pub sidecar_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkSurfaceBindingInput {
    pub source_id: String,
    pub surface_id: String,
    pub source_row_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLinkSurfaceIndex {
    endpoint_by_record_key: BTreeMap<(String, String), RecordLinkEndpoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordLinkFeatureKind {
    Numeric,
    Date,
    Categorical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordLinkFeatureValue {
    Numeric {
        units: String,
        scaled_value: i64,
        scale: u32,
    },
    Date {
        value: String,
    },
    Categorical {
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkFeatureComparison {
    pub feature_id: String,
    pub kind: RecordLinkFeatureKind,
    pub left: RecordLinkFeatureValue,
    pub right: RecordLinkFeatureValue,
    pub score_units: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordLinkCandidateAbstentionReason {
    MissingComparison,
    UnconfiguredFeature,
    Mismatch,
    ScaleMismatch,
    Incomparable,
    Tie,
    DuplicateBest,
    CardinalityExceeded,
    MissingAssignment,
    HardCannotLink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkCandidateAbstention {
    pub abstention_id: String,
    pub reason: RecordLinkCandidateAbstentionReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<RecordLinkEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<RecordLinkEndpoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feature_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkCandidate {
    pub candidate_id: String,
    pub left: RecordLinkEndpoint,
    pub right: RecordLinkEndpoint,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub support_features: Vec<RecordLinkFeatureComparison>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflict_features: Vec<RecordLinkFeatureComparison>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_feature_ids: Vec<String>,
    pub score_hint_units: u64,
    pub hard_cannot_link: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkCandidateSet {
    pub version: String,
    pub content_hash: String,
    pub operator_id: String,
    pub input_refs: Vec<RecordLinkInputArtifactRef>,
    pub feature_policy_digest: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub blocking_policy_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocking_policy: Option<RecordLinkBlockingPolicy>,
    #[serde(default, skip_serializing_if = "RecordLinkPairAccounting::is_empty")]
    pub pair_accounting: RecordLinkPairAccounting,
    pub candidates: Vec<RecordLinkCandidate>,
    pub abstentions: Vec<RecordLinkCandidateAbstention>,
    pub summary: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLinkCandidateConfig {
    pub operator_id: String,
    pub max_candidates_per_record: usize,
    pub max_candidate_pairs: usize,
    pub max_pair_comparisons: usize,
    pub require_unique_best_per_record: bool,
    pub feature_policies: BTreeMap<String, RecordLinkFeaturePolicy>,
    pub blocking_policy: Option<RecordLinkBlockingPolicy>,
}

impl Default for RecordLinkCandidateConfig {
    fn default() -> Self {
        Self {
            operator_id: "record_link:exact_comparison:v1".to_string(),
            max_candidates_per_record: DEFAULT_MAX_CANDIDATES_PER_RECORD,
            max_candidate_pairs: DEFAULT_MAX_CANDIDATE_PAIRS,
            max_pair_comparisons: DEFAULT_MAX_CANDIDATE_PAIRS,
            require_unique_best_per_record: true,
            feature_policies: BTreeMap::new(),
            blocking_policy: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkBlockingPolicy {
    pub policy_id: String,
    pub policy_version: String,
    pub keys: Vec<RecordLinkBlockingKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkBlockingKey {
    pub key_id: String,
    pub components: Vec<RecordLinkBlockingComponent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordLinkBlockingComponent {
    Exact {
        feature_id: String,
    },
    FixedDecimalBucket {
        feature_id: String,
        units: String,
        scale: u32,
        bucket_width_scaled_units: u64,
    },
    DateBucket {
        feature_id: String,
        bucket_days: u32,
    },
    CategoricalExact {
        feature_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RecordLinkPairAccounting {
    pub cross_source_pair_count: u64,
    pub admitted_pair_count: u64,
    pub suppressed_pair_count: u64,
    pub scored_pair_count: u64,
    pub blocking_policy_miss_count: u64,
    pub comparison_abstention_count: u64,
    pub ranking_abstention_count: u64,
}

impl RecordLinkPairAccounting {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkFeaturePolicy {
    pub feature_id: String,
    pub kind: RecordLinkFeatureKind,
    pub support: RecordLinkSupportPolicy,
    pub score_units: u64,
    pub hard_conflict_on_mismatch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordLinkSupportPolicy {
    Exact,
    NumericTolerance { tolerance_scaled_units: u64 },
    DateNear { max_days: u32 },
    CategoricalExact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLinkCandidateRequest<'a> {
    pub input_set: &'a RecordLinkInputSet,
    pub surface_index: &'a RecordLinkSurfaceIndex,
    pub config: RecordLinkCandidateConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentCardinality {
    OneToOne,
    ManyToMany,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentAlignmentPolicy {
    pub policy_id: String,
    pub policy_version: String,
    pub cardinality: AssignmentCardinality,
}

impl Default for AssignmentAlignmentPolicy {
    fn default() -> Self {
        Self {
            policy_id: "record_link.assignment_alignment".to_string(),
            policy_version: "1".to_string(),
            cardinality: AssignmentCardinality::OneToOne,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentAlignmentDecisionKind {
    Aligned,
    Abstained,
    CannotLinkVeto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentAlignmentRecord {
    pub alignment_id: String,
    pub left: RecordLinkEndpoint,
    pub right: RecordLinkEndpoint,
    pub decision: AssignmentAlignmentDecisionKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentAlignmentSidecar {
    pub version: String,
    pub artifact_content_hash: String,
    pub scope_id: String,
    pub profile_id: String,
    pub input_refs: Vec<RecordLinkInputArtifactRef>,
    pub feature_policy_digest: String,
    pub candidate_set_hash: String,
    pub record_link_evidence_path: String,
    pub record_link_evidence_hash: String,
    pub policy: AssignmentAlignmentPolicy,
    pub alignments: Vec<AssignmentAlignmentRecord>,
    pub abstentions: Vec<RecordLinkCandidateAbstention>,
    pub summary: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkEdgeHit {
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub evidence_id: String,
    pub lane: String,
    pub hard_cannot_link: bool,
    pub score_units: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLinkEvidenceRequest<'a> {
    pub input_set: &'a RecordLinkInputSet,
    pub candidate_set: &'a RecordLinkCandidateSet,
    pub feature_policies: &'a BTreeMap<String, RecordLinkFeaturePolicy>,
    pub blocking_policy: Option<RecordLinkBlockingPolicy>,
    pub policy: AssignmentAlignmentPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLinkEvidenceOutput {
    pub bundle: EvidenceBundle,
    pub edge_hits_by_surface_pair: BTreeMap<(String, String), Vec<RecordLinkEdgeHit>>,
    pub alignment: AssignmentAlignmentSidecar,
}

pub fn load_record_link_inputs(
    request: RecordLinkLoadRequest<'_>,
) -> RecordLinkResult<RecordLinkInputSet> {
    let mut inputs = Vec::new();
    for path in request.sidecar_paths {
        let resolved = resolve_workspace_path(request.workspace_root, &path)?;
        let bytes = fs::read(resolved.as_path()).map_err(|error| {
            RecordLinkCoreError::new(
                RecordLinkCoreErrorCode::Io,
                "record_link_input",
                "read_failed",
                format!(
                    "failed to read record-link sidecar {}: {error}",
                    resolved.display()
                ),
            )
        })?;
        let sidecar: RecordLinkInputSidecar = serde_json::from_slice(&bytes).map_err(|error| {
            RecordLinkCoreError::new(
                RecordLinkCoreErrorCode::ArtifactContract,
                "record_link_input",
                "parse_failed",
                format!(
                    "failed to parse record-link sidecar {}: {error}",
                    resolved.display()
                ),
            )
        })?;
        inputs.push(RecordLinkInputSource {
            path: path.to_string_lossy().into_owned(),
            sidecar,
        });
    }
    let input_set = build_record_link_input_set(inputs)?;
    if let Some(expected_profile_id) = request.expected_profile_id
        && input_set.profile_id != expected_profile_id
    {
        return Err(contract_error(
            "record_link_input",
            "profile_mismatch",
            format!(
                "record-link inputs profile_id {} did not match expected {}",
                input_set.profile_id, expected_profile_id
            ),
        ));
    }
    if let Some(expected_profile_digest) = request.expected_profile_digest
        && input_set.profile_digest != expected_profile_digest
    {
        return Err(contract_error(
            "record_link_input",
            "profile_digest_mismatch",
            format!(
                "record-link inputs profile_digest {} did not match expected {}",
                input_set.profile_digest, expected_profile_digest
            ),
        ));
    }
    if let Some(expected_scope_id) = request.expected_scope_id
        && input_set.scope_id != expected_scope_id
    {
        return Err(contract_error(
            "record_link_input",
            "scope_mismatch",
            format!(
                "record-link inputs scope_id {} did not match expected {}",
                input_set.scope_id, expected_scope_id
            ),
        ));
    }
    Ok(input_set)
}

pub fn build_record_link_input_set(
    mut inputs: Vec<RecordLinkInputSource>,
) -> RecordLinkResult<RecordLinkInputSet> {
    if inputs.len() < 2 {
        return Err(contract_error(
            "record_link_input",
            "insufficient_sources",
            "record-link candidates require at least two input sidecars",
        ));
    }
    for input in &inputs {
        validate_record_link_input_sidecar(&input.sidecar).map_err(|error| {
            contract_error(
                "record_link_input",
                "invalid_sidecar",
                format!("{}: {error}", input.path),
            )
        })?;
        canonical_record_link_input_bytes(&input.sidecar).map_err(|error| {
            contract_error(
                "record_link_input",
                "invalid_sidecar_hash",
                format!("{}: {error}", input.path),
            )
        })?;
        let content_hash = record_link_input_hash_without_self(&input.sidecar)?;
        if content_hash != input.sidecar.artifact_content_hash {
            return Err(contract_error(
                "record_link_input",
                "stale_sidecar_hash",
                format!(
                    "{} content hash mismatch: expected {}, got {}",
                    input.path, content_hash, input.sidecar.artifact_content_hash
                ),
            ));
        }
    }
    inputs.sort_by(|left, right| {
        left.sidecar
            .source_id
            .cmp(&right.sidecar.source_id)
            .then_with(|| left.path.cmp(&right.path))
    });
    let first_scope = inputs[0].sidecar.scope_id.clone();
    let first_profile = inputs[0].sidecar.profile_id.clone();
    let first_profile_digest = inputs[0].sidecar.profile_digest.clone();
    let mut refs = Vec::with_capacity(inputs.len());
    let mut seen_sources = BTreeSet::new();
    for input in &inputs {
        if input.sidecar.scope_id != first_scope {
            return Err(contract_error(
                "record_link_input",
                "mixed_scope",
                "record-link input sidecars must share one scope_id",
            ));
        }
        if input.sidecar.profile_id != first_profile {
            return Err(contract_error(
                "record_link_input",
                "mixed_profile",
                "record-link input sidecars must share one profile_id",
            ));
        }
        if input.sidecar.profile_digest != first_profile_digest {
            return Err(contract_error(
                "record_link_input",
                "mixed_profile_digest",
                "record-link input sidecars must share one profile_digest",
            ));
        }
        if !seen_sources.insert(input.sidecar.source_id.clone()) {
            return Err(contract_error(
                "record_link_input",
                "duplicate_source_id",
                format!(
                    "record-link input source_id {} appears more than once",
                    input.sidecar.source_id
                ),
            ));
        }
        refs.push(input_ref(input));
    }
    refs.sort_by(|left, right| {
        left.source_id
            .cmp(&right.source_id)
            .then_with(|| left.path.cmp(&right.path))
    });
    let content_hash = hash_json(&json!({
        "version": "canon.entity.record_link_input_set.v1",
        "refs": refs,
    }))?;
    Ok(RecordLinkInputSet {
        refs,
        inputs,
        scope_id: first_scope,
        profile_id: first_profile,
        profile_digest: first_profile_digest,
        content_hash,
    })
}

pub fn validate_record_link_inputs(input_set: &RecordLinkInputSet) -> RecordLinkResult<()> {
    let rebuilt = build_record_link_input_set(input_set.inputs.clone())?;
    if rebuilt.refs != input_set.refs || rebuilt.content_hash != input_set.content_hash {
        return Err(contract_error(
            "record_link_input",
            "input_set_hash_mismatch",
            "record-link input set hash does not match canonical sidecar refs",
        ));
    }
    Ok(())
}

pub fn bind_record_link_surfaces(
    input_set: &RecordLinkInputSet,
    surfaces: &[RecordLinkSurfaceBindingInput],
    stage: &str,
) -> RecordLinkResult<RecordLinkSurfaceIndex> {
    validate_record_link_inputs(input_set)?;
    let mut source_keys = BTreeMap::<String, Vec<String>>::new();
    for surface in surfaces {
        if surface.source_id.trim().is_empty() {
            return Err(contract_error(
                stage,
                "empty_source_id",
                "source_id is required",
            ));
        }
        if surface.surface_id.trim().is_empty() {
            return Err(contract_error(
                stage,
                "empty_surface_id",
                "surface_id is required",
            ));
        }
        for key in &surface.source_row_ids {
            if !key.trim().is_empty() {
                source_keys
                    .entry(scoped_surface_key(&surface.source_id, key))
                    .or_default()
                    .push(surface.surface_id.clone());
            }
        }
    }
    for ids in source_keys.values_mut() {
        ids.sort();
        ids.dedup();
    }

    let mut endpoint_by_record_key = BTreeMap::new();
    for input in &input_set.inputs {
        for record in &input.sidecar.records {
            let keys = record_surface_keys(record);
            let mut matches = BTreeSet::new();
            for key in keys {
                if let Some(surface_ids) =
                    source_keys.get(&scoped_surface_key(&input.sidecar.source_id, &key))
                {
                    matches.extend(surface_ids.iter().cloned());
                }
            }
            let surface_id = match matches.len() {
                1 => matches.into_iter().next().expect("one surface match"),
                0 => {
                    return Err(contract_error(
                        stage,
                        "missing_surface_binding",
                        format!(
                            "record {} does not bind to a prepared surface",
                            record.record_id
                        ),
                    ));
                }
                _ => {
                    return Err(contract_error(
                        stage,
                        "ambiguous_surface_binding",
                        format!(
                            "record {} binds to multiple prepared surfaces",
                            record.record_id
                        ),
                    ));
                }
            };
            let endpoint = RecordLinkEndpoint {
                source_id: input.sidecar.source_id.clone(),
                record_id: record.record_id.clone(),
                surface_id,
                observation_id: record.subject_observation_ref.observation_id.clone(),
                assignment_ref: record.assignment_ref.clone(),
                sidecar_hash: input.sidecar.artifact_content_hash.clone(),
            };
            if endpoint_by_record_key
                .insert(endpoint_key(&endpoint), endpoint)
                .is_some()
            {
                return Err(contract_error(
                    stage,
                    "duplicate_record_id",
                    format!(
                        "record_id {} appears more than once for source {}",
                        record.record_id, input.sidecar.source_id
                    ),
                ));
            }
        }
    }
    Ok(RecordLinkSurfaceIndex {
        endpoint_by_record_key,
    })
}

pub fn generate_record_link_candidates(
    request: RecordLinkCandidateRequest<'_>,
) -> RecordLinkResult<RecordLinkCandidateSet> {
    validate_record_link_inputs(request.input_set)?;
    validate_feature_policies(&request.config.feature_policies)?;
    let feature_policy_digest = feature_policy_digest(&request.config.feature_policies)?;
    let blocking_policy = request.config.blocking_policy.clone();
    validate_record_link_blocking_policy(blocking_policy.as_ref())?;
    let blocking_policy_digest = blocking_policy_digest(blocking_policy.as_ref())?;
    if request.config.max_candidate_pairs == 0
        || request.config.max_candidates_per_record == 0
        || request.config.max_pair_comparisons == 0
    {
        return Err(RecordLinkCoreError::new(
            RecordLinkCoreErrorCode::Budget,
            "block",
            "zero_candidate_budget",
            "record-link candidate budgets must be non-zero",
        ));
    }

    let mut raw_candidates = Vec::new();
    let mut abstentions = Vec::new();
    let mut pair_comparisons = 0usize;
    let records = all_records(request.input_set);
    let pair_plan = candidate_pair_plan(&records, blocking_policy.as_ref())?;
    let mut pair_accounting = pair_plan.accounting;
    for (left_index, right_index) in &pair_plan.admitted_pairs {
        let left = &records[*left_index];
        let right = &records[*right_index];
        pair_comparisons = pair_comparisons.saturating_add(1);
        if pair_comparisons > request.config.max_pair_comparisons {
            return Err(RecordLinkCoreError::new(
                RecordLinkCoreErrorCode::Budget,
                "block",
                "pair_comparison_budget_exceeded",
                format!(
                    "record-link pair comparisons exceeded {}",
                    request.config.max_pair_comparisons
                ),
            ));
        }
        let left_endpoint = endpoint_for_record(
            request.surface_index,
            &left.input.sidecar.source_id,
            left.record,
        )?;
        let right_endpoint = endpoint_for_record(
            request.surface_index,
            &right.input.sidecar.source_id,
            right.record,
        )?;
        let comparison =
            compare_records(left.record, right.record, &request.config.feature_policies)?;
        abstentions.extend(pair_abstentions(
            &left_endpoint,
            &right_endpoint,
            &comparison,
        )?);
        if comparison.support_features.is_empty() && comparison.conflict_features.is_empty() {
            continue;
        }
        let candidate_id = stable_id(
            "record_link_candidate",
            &json!({
                "left": left_endpoint,
                "right": right_endpoint,
                "support": comparison.support_features,
                "conflict": comparison.conflict_features,
            }),
        )?;
        raw_candidates.push(RecordLinkCandidate {
            candidate_id,
            left: left_endpoint,
            right: right_endpoint,
            support_features: comparison.support_features,
            conflict_features: comparison.conflict_features,
            missing_feature_ids: comparison.missing_feature_ids,
            score_hint_units: comparison.score_hint_units,
            hard_cannot_link: comparison.hard_cannot_link,
        });
        if raw_candidates.len() > request.config.max_candidate_pairs {
            return Err(RecordLinkCoreError::new(
                RecordLinkCoreErrorCode::Budget,
                "block",
                "candidate_pair_budget_exceeded",
                format!(
                    "record-link candidate count exceeded {}",
                    request.config.max_candidate_pairs
                ),
            ));
        }
    }
    pair_accounting.scored_pair_count = pair_comparisons as u64;
    raw_candidates.sort_by(candidate_cmp);

    let (candidates, ranking_abstentions) =
        enforce_candidate_bounds(raw_candidates, &request.config)?;
    abstentions.extend(ranking_abstentions);
    abstentions.sort_by(abstention_cmp);
    abstentions.dedup_by(|left, right| left.abstention_id == right.abstention_id);
    pair_accounting.comparison_abstention_count = comparison_abstention_count(&abstentions);
    pair_accounting.ranking_abstention_count = ranking_abstention_count(&abstentions);

    let mut candidate_set = RecordLinkCandidateSet {
        version: RECORD_LINK_CANDIDATE_SET_VERSION.to_string(),
        content_hash: String::new(),
        operator_id: request.config.operator_id,
        input_refs: request.input_set.refs.clone(),
        feature_policy_digest,
        blocking_policy_digest,
        blocking_policy,
        pair_accounting,
        candidates,
        abstentions,
        summary: BTreeMap::new(),
    };
    candidate_set.summary = candidate_set_summary(&candidate_set);
    candidate_set.content_hash = hash_candidate_set_without_self(&candidate_set)?;
    validate_record_link_candidate_set(&candidate_set)?;
    Ok(candidate_set)
}

pub fn validate_record_link_candidate_set(
    candidate_set: &RecordLinkCandidateSet,
) -> RecordLinkResult<()> {
    if candidate_set.version != RECORD_LINK_CANDIDATE_SET_VERSION {
        return Err(contract_error(
            "block",
            "wrong_candidate_set_version",
            format!(
                "expected {RECORD_LINK_CANDIDATE_SET_VERSION}, got {}",
                candidate_set.version
            ),
        ));
    }
    let expected = hash_candidate_set_without_self(candidate_set)?;
    if candidate_set.content_hash != expected {
        return Err(contract_error(
            "block",
            "candidate_set_hash_mismatch",
            format!(
                "record-link candidate set hash mismatch: expected {}, got {}",
                expected, candidate_set.content_hash
            ),
        ));
    }
    if candidate_set.summary != candidate_set_summary(candidate_set) {
        return Err(contract_error(
            "block",
            "candidate_summary_mismatch",
            "record-link candidate set summary does not match candidate payload/accounting",
        ));
    }
    validate_record_link_blocking_policy(candidate_set.blocking_policy.as_ref())?;
    let expected_blocking_digest = blocking_policy_digest(candidate_set.blocking_policy.as_ref())?;
    if candidate_set.blocking_policy_digest != expected_blocking_digest {
        return Err(contract_error(
            "block",
            "candidate_blocking_policy_digest_mismatch",
            format!(
                "record-link candidate set embedded blocking policy digest {} did not match expected {}",
                candidate_set.blocking_policy_digest, expected_blocking_digest
            ),
        ));
    }
    for window in candidate_set.candidates.windows(2) {
        if candidate_cmp(&window[0], &window[1]).is_ge() {
            return Err(contract_error(
                "block",
                "candidate_order",
                "record-link candidates must be in canonical order",
            ));
        }
    }
    Ok(())
}

pub fn validate_record_link_candidate_set_for_inputs(
    input_set: &RecordLinkInputSet,
    candidate_set: &RecordLinkCandidateSet,
    feature_policies: &BTreeMap<String, RecordLinkFeaturePolicy>,
    blocking_policy: Option<&RecordLinkBlockingPolicy>,
) -> RecordLinkResult<()> {
    validate_record_link_inputs(input_set)?;
    validate_record_link_candidate_set(candidate_set)?;
    validate_feature_policies(feature_policies)?;
    let expected_policy_digest = feature_policy_digest(feature_policies)?;
    if candidate_set.feature_policy_digest != expected_policy_digest {
        return Err(contract_error(
            "block",
            "candidate_feature_policy_digest_mismatch",
            format!(
                "record-link candidate set policy digest {} did not match expected {}",
                candidate_set.feature_policy_digest, expected_policy_digest
            ),
        ));
    }
    validate_candidate_blocking_policy(input_set, candidate_set, blocking_policy)?;
    if candidate_set.input_refs != input_set.refs {
        return Err(contract_error(
            "block",
            "candidate_input_ref_mismatch",
            "record-link candidate set does not bind the exact input refs",
        ));
    }
    let record_index = input_record_index(input_set)?;
    for candidate in &candidate_set.candidates {
        let left = validate_candidate_endpoint(&record_index, &candidate.left)?;
        let right = validate_candidate_endpoint(&record_index, &candidate.right)?;
        if candidate.left.source_id == candidate.right.source_id {
            return Err(contract_error(
                "block",
                "candidate_same_source",
                "record-link candidates must span two distinct sources",
            ));
        }
        let expected_comparison =
            compare_feature_maps(&left.features, &right.features, feature_policies)?;
        if candidate.support_features != expected_comparison.support_features {
            return Err(contract_error(
                "block",
                "candidate_support_feature_mismatch",
                format!(
                    "candidate {} support features do not match policy-derived input values",
                    candidate.candidate_id
                ),
            ));
        }
        if candidate.conflict_features != expected_comparison.conflict_features {
            return Err(contract_error(
                "block",
                "candidate_conflict_feature_mismatch",
                format!(
                    "candidate {} conflict features do not match policy-derived input values",
                    candidate.candidate_id
                ),
            ));
        }
        if candidate.missing_feature_ids != expected_comparison.missing_feature_ids {
            return Err(contract_error(
                "block",
                "candidate_missing_feature_mismatch",
                format!(
                    "candidate {} missing features do not match policy-derived input values",
                    candidate.candidate_id
                ),
            ));
        }
        validate_candidate_features(candidate, left, right)?;
        let expected_score = expected_comparison.score_hint_units;
        if candidate.score_hint_units != expected_score {
            return Err(contract_error(
                "block",
                "candidate_score_mismatch",
                format!(
                    "candidate {} score_hint_units {} did not match support score {}",
                    candidate.candidate_id, candidate.score_hint_units, expected_score
                ),
            ));
        }
        let expected_hard_cannot_link = expected_comparison.hard_cannot_link;
        if candidate.hard_cannot_link != expected_hard_cannot_link {
            return Err(contract_error(
                "block",
                "candidate_hard_veto_mismatch",
                format!(
                    "candidate {} hard_cannot_link flag does not match conflict features",
                    candidate.candidate_id
                ),
            ));
        }
        let expected_candidate_id = stable_id(
            "record_link_candidate",
            &json!({
                "left": candidate.left,
                "right": candidate.right,
                "support": candidate.support_features,
                "conflict": candidate.conflict_features,
            }),
        )?;
        if candidate.candidate_id != expected_candidate_id {
            return Err(contract_error(
                "block",
                "candidate_id_mismatch",
                format!(
                    "candidate {} did not match its canonical endpoint/features id {}",
                    candidate.candidate_id, expected_candidate_id
                ),
            ));
        }
    }
    Ok(())
}

pub fn canonical_record_link_candidate_set_bytes(
    candidate_set: &RecordLinkCandidateSet,
) -> RecordLinkResult<Vec<u8>> {
    validate_record_link_candidate_set(candidate_set)?;
    serde_json::to_vec(candidate_set).map_err(|error| {
        contract_error(
            "block",
            "candidate_set_serialize",
            format!("failed to serialize record-link candidate set: {error}"),
        )
    })
}

pub fn build_record_link_evidence(
    request: RecordLinkEvidenceRequest<'_>,
) -> RecordLinkResult<RecordLinkEvidenceOutput> {
    validate_record_link_inputs(request.input_set)?;
    validate_record_link_candidate_set_for_inputs(
        request.input_set,
        request.candidate_set,
        request.feature_policies,
        request.blocking_policy.as_ref(),
    )?;
    let mut records = Vec::new();
    let mut evidence_ids_by_candidate = BTreeMap::<String, Vec<String>>::new();
    let mut edge_hits_by_surface_pair = BTreeMap::<(String, String), Vec<RecordLinkEdgeHit>>::new();

    for candidate in &request.candidate_set.candidates {
        for feature in &candidate.support_features {
            let evidence_policy_hash = evidence_policy_content_hash(
                &feature.feature_id,
                "record_link_feature_support",
                &request.candidate_set.feature_policy_digest,
                request.feature_policies,
            )?;
            let record = record_link_evidence_record(
                candidate,
                feature,
                EvidenceKind::RecordLinkSupport,
                "record_link_feature_support",
                None,
                evidence_policy_hash,
            )?;
            let canonical = canonicalize_record(record).map_err(|error| {
                contract_error("evidence", "invalid_evidence_record", error.to_string())
            })?;
            evidence_ids_by_candidate
                .entry(candidate.candidate_id.clone())
                .or_default()
                .push(canonical.evidence_id.clone());
            edge_hits_by_surface_pair
                .entry(surface_pair(candidate))
                .or_default()
                .push(RecordLinkEdgeHit {
                    left_surface_id: candidate.left.surface_id.clone(),
                    right_surface_id: candidate.right.surface_id.clone(),
                    evidence_id: canonical.evidence_id.clone(),
                    lane: "support".to_string(),
                    hard_cannot_link: false,
                    score_units: feature.score_units,
                });
            records.push(canonical);
        }
        for feature in &candidate.conflict_features {
            let evidence_policy_hash = evidence_policy_content_hash(
                &feature.feature_id,
                "record_link_feature_conflict",
                &request.candidate_set.feature_policy_digest,
                request.feature_policies,
            )?;
            let record = record_link_evidence_record(
                candidate,
                feature,
                EvidenceKind::AntiMergeVeto,
                "record_link_feature_conflict",
                Some(EvidenceAuthorityBasis::AuthoritativeIncompatibility),
                evidence_policy_hash,
            )?;
            let canonical = canonicalize_record(record).map_err(|error| {
                contract_error("evidence", "invalid_evidence_record", error.to_string())
            })?;
            evidence_ids_by_candidate
                .entry(candidate.candidate_id.clone())
                .or_default()
                .push(canonical.evidence_id.clone());
            edge_hits_by_surface_pair
                .entry(surface_pair(candidate))
                .or_default()
                .push(RecordLinkEdgeHit {
                    left_surface_id: candidate.left.surface_id.clone(),
                    right_surface_id: candidate.right.surface_id.clone(),
                    evidence_id: canonical.evidence_id.clone(),
                    lane: "anti_merge".to_string(),
                    hard_cannot_link: true,
                    score_units: u64::MAX,
                });
            records.push(canonical);
        }
    }
    for hits in edge_hits_by_surface_pair.values_mut() {
        hits.sort_by(|left, right| left.evidence_id.cmp(&right.evidence_id));
    }

    let bundle = canonicalize_bundle(records).map_err(|error| {
        contract_error("evidence", "invalid_evidence_bundle", error.to_string())
    })?;
    canonical_bundle_bytes(&bundle).map_err(|error| {
        contract_error(
            "evidence",
            "invalid_evidence_bundle_hash",
            error.to_string(),
        )
    })?;
    let alignment = build_assignment_alignment(
        request.input_set,
        request.candidate_set,
        request.feature_policies,
        request.blocking_policy.as_ref(),
        &bundle,
        &evidence_ids_by_candidate,
        request.policy,
    )?;
    Ok(RecordLinkEvidenceOutput {
        bundle,
        edge_hits_by_surface_pair,
        alignment,
    })
}

pub fn build_assignment_alignment(
    input_set: &RecordLinkInputSet,
    candidate_set: &RecordLinkCandidateSet,
    feature_policies: &BTreeMap<String, RecordLinkFeaturePolicy>,
    blocking_policy: Option<&RecordLinkBlockingPolicy>,
    bundle: &EvidenceBundle,
    evidence_ids_by_candidate: &BTreeMap<String, Vec<String>>,
    policy: AssignmentAlignmentPolicy,
) -> RecordLinkResult<AssignmentAlignmentSidecar> {
    validate_record_link_candidate_set_for_inputs(
        input_set,
        candidate_set,
        feature_policies,
        blocking_policy,
    )?;
    canonical_bundle_bytes(bundle).map_err(|error| {
        contract_error(
            "assignment_alignment",
            "evidence_hash_mismatch",
            error.to_string(),
        )
    })?;
    let mut alignments = Vec::new();
    let mut abstentions = candidate_set.abstentions.clone();
    let mut eligible = Vec::new();
    for candidate in &candidate_set.candidates {
        if candidate.hard_cannot_link {
            alignments.push(alignment_record(
                candidate,
                AssignmentAlignmentDecisionKind::CannotLinkVeto,
                evidence_ids_by_candidate,
                vec!["hard_cannot_link".to_string()],
            )?);
            abstentions.push(candidate_abstention(
                RecordLinkCandidateAbstentionReason::HardCannotLink,
                Some(candidate.left.clone()),
                Some(candidate.right.clone()),
                vec![candidate.candidate_id.clone()],
                candidate
                    .conflict_features
                    .iter()
                    .map(|feature| feature.feature_id.clone())
                    .collect(),
            )?);
            continue;
        }
        if candidate.left.assignment_ref.is_none() || candidate.right.assignment_ref.is_none() {
            abstentions.push(candidate_abstention(
                RecordLinkCandidateAbstentionReason::MissingAssignment,
                Some(candidate.left.clone()),
                Some(candidate.right.clone()),
                vec![candidate.candidate_id.clone()],
                Vec::new(),
            )?);
            continue;
        }
        eligible.push(candidate);
    }

    match policy.cardinality {
        AssignmentCardinality::ManyToMany => {
            for candidate in eligible {
                alignments.push(alignment_record(
                    candidate,
                    AssignmentAlignmentDecisionKind::Aligned,
                    evidence_ids_by_candidate,
                    vec!["assignment_aligned".to_string()],
                )?);
            }
        }
        AssignmentCardinality::OneToOne => {
            let mut left_counts = BTreeMap::<String, usize>::new();
            let mut right_counts = BTreeMap::<String, usize>::new();
            for candidate in &eligible {
                *left_counts
                    .entry(assignment_key(&candidate.left))
                    .or_default() += 1;
                *right_counts
                    .entry(assignment_key(&candidate.right))
                    .or_default() += 1;
            }
            for candidate in eligible {
                if left_counts[&assignment_key(&candidate.left)] > 1
                    || right_counts[&assignment_key(&candidate.right)] > 1
                {
                    abstentions.push(candidate_abstention(
                        RecordLinkCandidateAbstentionReason::DuplicateBest,
                        Some(candidate.left.clone()),
                        Some(candidate.right.clone()),
                        vec![candidate.candidate_id.clone()],
                        candidate
                            .support_features
                            .iter()
                            .map(|feature| feature.feature_id.clone())
                            .collect(),
                    )?);
                } else {
                    alignments.push(alignment_record(
                        candidate,
                        AssignmentAlignmentDecisionKind::Aligned,
                        evidence_ids_by_candidate,
                        vec!["assignment_aligned".to_string()],
                    )?);
                }
            }
        }
    }

    alignments.sort_by(|left, right| left.alignment_id.cmp(&right.alignment_id));
    abstentions.sort_by(abstention_cmp);
    abstentions.dedup_by(|left, right| left.abstention_id == right.abstention_id);
    let mut sidecar = AssignmentAlignmentSidecar {
        version: ASSIGNMENT_ALIGNMENT_VERSION.to_string(),
        artifact_content_hash: String::new(),
        scope_id: input_set.scope_id.clone(),
        profile_id: input_set.profile_id.clone(),
        input_refs: input_set.refs.clone(),
        feature_policy_digest: candidate_set.feature_policy_digest.clone(),
        candidate_set_hash: candidate_set.content_hash.clone(),
        record_link_evidence_path: RECORD_LINK_EVIDENCE_PATH.to_string(),
        record_link_evidence_hash: bundle.content_hash.clone(),
        policy,
        alignments,
        abstentions,
        summary: BTreeMap::new(),
    };
    sidecar.summary = assignment_alignment_summary(&sidecar);
    sidecar.artifact_content_hash = hash_assignment_alignment_without_self(&sidecar)?;
    validate_assignment_alignment_sidecar(&sidecar)?;
    Ok(sidecar)
}

pub fn validate_assignment_alignment_sidecar(
    sidecar: &AssignmentAlignmentSidecar,
) -> RecordLinkResult<()> {
    if sidecar.version != ASSIGNMENT_ALIGNMENT_VERSION {
        return Err(contract_error(
            "assignment_alignment",
            "wrong_version",
            format!(
                "expected {ASSIGNMENT_ALIGNMENT_VERSION}, got {}",
                sidecar.version
            ),
        ));
    }
    if sidecar.record_link_evidence_path != RECORD_LINK_EVIDENCE_PATH {
        return Err(contract_error(
            "assignment_alignment",
            "wrong_evidence_path",
            "assignment alignment must bind the canonical record-link evidence path",
        ));
    }
    require_hash(
        &sidecar.feature_policy_digest,
        "assignment_alignment",
        "feature_policy_digest",
    )?;
    require_hash(
        &sidecar.record_link_evidence_hash,
        "assignment_alignment",
        "record_link_evidence_hash",
    )?;
    require_hash(
        &sidecar.candidate_set_hash,
        "assignment_alignment",
        "candidate_set_hash",
    )?;
    let expected = hash_assignment_alignment_without_self(sidecar)?;
    if sidecar.artifact_content_hash != expected {
        return Err(contract_error(
            "assignment_alignment",
            "alignment_hash_mismatch",
            format!(
                "assignment alignment hash mismatch: expected {}, got {}",
                expected, sidecar.artifact_content_hash
            ),
        ));
    }
    for window in sidecar.alignments.windows(2) {
        if window[0].alignment_id >= window[1].alignment_id {
            return Err(contract_error(
                "assignment_alignment",
                "alignment_order",
                "assignment alignment records must be sorted by alignment_id",
            ));
        }
    }
    Ok(())
}

pub fn canonical_assignment_alignment_bytes(
    sidecar: &AssignmentAlignmentSidecar,
) -> RecordLinkResult<Vec<u8>> {
    validate_assignment_alignment_sidecar(sidecar)?;
    serde_json::to_vec(sidecar).map_err(|error| {
        contract_error(
            "assignment_alignment",
            "alignment_serialize",
            format!("failed to serialize assignment alignment sidecar: {error}"),
        )
    })
}

fn input_ref(input: &RecordLinkInputSource) -> RecordLinkInputArtifactRef {
    RecordLinkInputArtifactRef {
        path: input.path.clone(),
        version: input.sidecar.version.clone(),
        content_hash: input.sidecar.artifact_content_hash.clone(),
        source_id: input.sidecar.source_id.clone(),
        scope_id: input.sidecar.scope_id.clone(),
        profile_id: input.sidecar.profile_id.clone(),
        profile_digest: input.sidecar.profile_digest.clone(),
        input_digest: input.sidecar.input_digest.clone(),
        source_mapping_digest: input.sidecar.source_mapping_digest.clone(),
    }
}

fn record_link_input_hash_without_self(
    sidecar: &RecordLinkInputSidecar,
) -> RecordLinkResult<String> {
    let mut hashable = sidecar.clone();
    hashable.artifact_content_hash.clear();
    hash_json(&hashable)
}

fn resolve_workspace_path(root: &Path, path: &Path) -> RecordLinkResult<PathBuf> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(RecordLinkCoreError::new(
            RecordLinkCoreErrorCode::Path,
            "record_link_input",
            "path_traversal",
            "record-link sidecar paths may not contain parent components",
        ));
    }
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    })
}

fn record_surface_keys(record: &RecordLinkInputRecord) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    keys.insert(record.source_ref.source_object_id.clone());
    keys.insert(record.source_ref.source_locator.locator.clone());
    keys.insert(record.subject_observation_ref.observation_id.clone());
    keys
}

fn scoped_surface_key(source_id: &str, key: &str) -> String {
    format!("{source_id}\u{0}{key}")
}

struct RecordWithInput<'a> {
    input: &'a RecordLinkInputSource,
    record: &'a RecordLinkInputRecord,
}

fn all_records(input_set: &RecordLinkInputSet) -> Vec<RecordWithInput<'_>> {
    input_set
        .inputs
        .iter()
        .flat_map(|input| {
            input
                .sidecar
                .records
                .iter()
                .map(move |record| RecordWithInput { input, record })
        })
        .collect()
}

struct CandidatePairPlan {
    admitted_pairs: Vec<(usize, usize)>,
    accounting: RecordLinkPairAccounting,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct BlockingKeyValue {
    key_id: String,
    component_values: Vec<String>,
}

pub fn validate_record_link_blocking_policy(
    policy: Option<&RecordLinkBlockingPolicy>,
) -> RecordLinkResult<()> {
    let Some(policy) = policy else {
        return Ok(());
    };
    if policy.policy_id.trim().is_empty() {
        return Err(contract_error(
            "block",
            "blocking_policy_empty_id",
            "record-link blocking policy_id is required",
        ));
    }
    if policy.policy_version.trim().is_empty() {
        return Err(contract_error(
            "block",
            "blocking_policy_empty_version",
            "record-link blocking policy_version is required",
        ));
    }
    if policy.keys.is_empty() {
        return Err(contract_error(
            "block",
            "blocking_policy_empty_keys",
            "record-link blocking policy requires at least one key",
        ));
    }
    let mut key_ids = BTreeSet::new();
    for key in &policy.keys {
        if key.key_id.trim().is_empty() {
            return Err(contract_error(
                "block",
                "blocking_key_empty_id",
                "record-link blocking key_id is required",
            ));
        }
        if !key_ids.insert(key.key_id.clone()) {
            return Err(contract_error(
                "block",
                "blocking_key_duplicate_id",
                format!("record-link blocking key_id {} is duplicated", key.key_id),
            ));
        }
        if key.components.is_empty() {
            return Err(contract_error(
                "block",
                "blocking_key_empty_components",
                format!(
                    "record-link blocking key {} requires components",
                    key.key_id
                ),
            ));
        }
        let mut component_ids = BTreeSet::new();
        for component in &key.components {
            let feature_id = blocking_component_feature_id(component);
            if feature_id.trim().is_empty() {
                return Err(contract_error(
                    "block",
                    "blocking_component_empty_feature",
                    format!(
                        "record-link blocking key {} has an empty feature_id",
                        key.key_id
                    ),
                ));
            }
            if !component_ids.insert(feature_id.to_string()) {
                return Err(contract_error(
                    "block",
                    "blocking_component_duplicate_feature",
                    format!(
                        "record-link blocking key {} repeats feature_id {}",
                        key.key_id, feature_id
                    ),
                ));
            }
            match component {
                RecordLinkBlockingComponent::FixedDecimalBucket {
                    units,
                    bucket_width_scaled_units,
                    ..
                } => {
                    if units.trim().is_empty() {
                        return Err(contract_error(
                            "block",
                            "blocking_component_empty_units",
                            format!(
                                "record-link fixed-decimal blocking key {} requires units",
                                key.key_id
                            ),
                        ));
                    }
                    if *bucket_width_scaled_units == 0 {
                        return Err(contract_error(
                            "block",
                            "blocking_component_zero_width",
                            format!(
                                "record-link fixed-decimal blocking key {} requires non-zero bucket_width_scaled_units",
                                key.key_id
                            ),
                        ));
                    }
                }
                RecordLinkBlockingComponent::DateBucket { bucket_days, .. } => {
                    if *bucket_days == 0 {
                        return Err(contract_error(
                            "block",
                            "blocking_component_zero_days",
                            format!(
                                "record-link date blocking key {} requires non-zero bucket_days",
                                key.key_id
                            ),
                        ));
                    }
                }
                RecordLinkBlockingComponent::Exact { .. }
                | RecordLinkBlockingComponent::CategoricalExact { .. } => {}
            }
        }
    }
    Ok(())
}

fn validate_candidate_blocking_policy(
    input_set: &RecordLinkInputSet,
    candidate_set: &RecordLinkCandidateSet,
    blocking_policy: Option<&RecordLinkBlockingPolicy>,
) -> RecordLinkResult<()> {
    validate_record_link_blocking_policy(blocking_policy)?;
    let expected_digest = blocking_policy_digest(blocking_policy)?;
    if candidate_set.blocking_policy_digest != expected_digest {
        return Err(contract_error(
            "block",
            "candidate_blocking_policy_digest_mismatch",
            format!(
                "record-link candidate set blocking policy digest {} did not match expected {}",
                candidate_set.blocking_policy_digest, expected_digest
            ),
        ));
    }
    if candidate_set.blocking_policy.as_ref() != blocking_policy {
        return Err(contract_error(
            "block",
            "candidate_blocking_policy_mismatch",
            "record-link candidate set does not bind the current blocking policy",
        ));
    }
    let records = all_records(input_set);
    let plan = candidate_pair_plan(&records, blocking_policy)?;
    validate_pair_accounting(&candidate_set.pair_accounting, &plan.accounting)?;
    let admitted = pair_endpoint_keys(&records, &plan.admitted_pairs)?;
    for candidate in &candidate_set.candidates {
        let key = canonical_endpoint_pair_key(&candidate.left, &candidate.right);
        if !admitted.contains(&key) {
            return Err(contract_error(
                "block",
                "candidate_blocking_policy_violation",
                format!(
                    "candidate {} was not admitted by the current record-link blocking policy",
                    candidate.candidate_id
                ),
            ));
        }
    }
    if candidate_set.pair_accounting.comparison_abstention_count
        != comparison_abstention_count(&candidate_set.abstentions)
    {
        return Err(contract_error(
            "block",
            "candidate_comparison_accounting_mismatch",
            "record-link comparison abstention accounting does not match abstention records",
        ));
    }
    if candidate_set.pair_accounting.ranking_abstention_count
        != ranking_abstention_count(&candidate_set.abstentions)
    {
        return Err(contract_error(
            "block",
            "candidate_ranking_accounting_mismatch",
            "record-link ranking abstention accounting does not match abstention records",
        ));
    }
    Ok(())
}

fn validate_pair_accounting(
    actual: &RecordLinkPairAccounting,
    expected: &RecordLinkPairAccounting,
) -> RecordLinkResult<()> {
    let mut expected_base = expected.clone();
    expected_base.comparison_abstention_count = actual.comparison_abstention_count;
    expected_base.ranking_abstention_count = actual.ranking_abstention_count;
    if actual != &expected_base {
        return Err(contract_error(
            "block",
            "candidate_pair_accounting_mismatch",
            format!(
                "record-link pair accounting did not match inputs/policy: expected {:?}, got {:?}",
                expected_base, actual
            ),
        ));
    }
    Ok(())
}

fn candidate_pair_plan(
    records: &[RecordWithInput<'_>],
    blocking_policy: Option<&RecordLinkBlockingPolicy>,
) -> RecordLinkResult<CandidatePairPlan> {
    let cross_source_pair_count = cross_source_pair_count(records)?;
    let admitted_pairs = match blocking_policy {
        Some(policy) => blocking_admitted_pairs(records, policy)?,
        None => all_cross_source_pairs(records),
    };
    let admitted_pair_count = admitted_pairs.len() as u64;
    let suppressed_pair_count = cross_source_pair_count
        .checked_sub(admitted_pair_count)
        .ok_or_else(|| {
            contract_error(
                "block",
                "blocking_pair_accounting_underflow",
                "record-link admitted pair count exceeded cross-source pair count",
            )
        })?;
    let blocking_policy_miss_count = if blocking_policy.is_some() {
        suppressed_pair_count
    } else {
        0
    };
    Ok(CandidatePairPlan {
        admitted_pairs,
        accounting: RecordLinkPairAccounting {
            cross_source_pair_count,
            admitted_pair_count,
            suppressed_pair_count,
            scored_pair_count: admitted_pair_count,
            blocking_policy_miss_count,
            comparison_abstention_count: 0,
            ranking_abstention_count: 0,
        },
    })
}

fn cross_source_pair_count(records: &[RecordWithInput<'_>]) -> RecordLinkResult<u64> {
    let mut by_source = BTreeMap::<&str, u64>::new();
    for record in records {
        *by_source
            .entry(&record.input.sidecar.source_id)
            .or_default() += 1;
    }
    let mut total = 0u64;
    let mut seen = 0u64;
    for count in by_source.values() {
        total = total
            .checked_add(seen.checked_mul(*count).ok_or_else(|| {
                contract_error(
                    "block",
                    "pair_accounting_overflow",
                    "record-link pair accounting overflowed",
                )
            })?)
            .ok_or_else(|| {
                contract_error(
                    "block",
                    "pair_accounting_overflow",
                    "record-link pair accounting overflowed",
                )
            })?;
        seen = seen.checked_add(*count).ok_or_else(|| {
            contract_error(
                "block",
                "pair_accounting_overflow",
                "record-link pair accounting overflowed",
            )
        })?;
    }
    Ok(total)
}

fn all_cross_source_pairs(records: &[RecordWithInput<'_>]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for (left_index, left) in records.iter().enumerate() {
        for (right_offset, right) in records.iter().skip(left_index + 1).enumerate() {
            if left.input.sidecar.source_id == right.input.sidecar.source_id {
                continue;
            }
            pairs.push((left_index, left_index + right_offset + 1));
        }
    }
    pairs
}

fn blocking_admitted_pairs(
    records: &[RecordWithInput<'_>],
    policy: &RecordLinkBlockingPolicy,
) -> RecordLinkResult<Vec<(usize, usize)>> {
    validate_record_link_blocking_policy(Some(policy))?;
    let mut postings = BTreeMap::<BlockingKeyValue, BTreeMap<&str, Vec<usize>>>::new();
    for (record_index, record) in records.iter().enumerate() {
        let features = feature_map(record.record)?;
        for key in record_blocking_keys(&features, policy)? {
            postings
                .entry(key)
                .or_default()
                .entry(&record.input.sidecar.source_id)
                .or_default()
                .push(record_index);
        }
    }
    let mut admitted = BTreeSet::new();
    for by_source in postings.values_mut() {
        for indexes in by_source.values_mut() {
            indexes.sort_unstable();
            indexes.dedup();
        }
        let source_ids = by_source.keys().copied().collect::<Vec<_>>();
        for (left_source_index, left_source) in source_ids.iter().enumerate() {
            for right_source in source_ids.iter().skip(left_source_index + 1) {
                for left_index in &by_source[*left_source] {
                    for right_index in &by_source[*right_source] {
                        let pair = if left_index < right_index {
                            (*left_index, *right_index)
                        } else {
                            (*right_index, *left_index)
                        };
                        admitted.insert(pair);
                    }
                }
            }
        }
    }
    Ok(admitted.into_iter().collect())
}

fn record_blocking_keys(
    features: &BTreeMap<String, RecordLinkFeatureValue>,
    policy: &RecordLinkBlockingPolicy,
) -> RecordLinkResult<Vec<BlockingKeyValue>> {
    let mut keys = Vec::new();
    for key in &policy.keys {
        let mut component_values = Vec::with_capacity(key.components.len());
        let mut missing = false;
        for component in &key.components {
            let feature_id = blocking_component_feature_id(component);
            let Some(value) = features.get(feature_id) else {
                missing = true;
                break;
            };
            component_values.push(blocking_component_value(&key.key_id, component, value)?);
        }
        if !missing {
            keys.push(BlockingKeyValue {
                key_id: key.key_id.clone(),
                component_values,
            });
        }
    }
    Ok(keys)
}

fn blocking_component_feature_id(component: &RecordLinkBlockingComponent) -> &str {
    match component {
        RecordLinkBlockingComponent::Exact { feature_id }
        | RecordLinkBlockingComponent::FixedDecimalBucket { feature_id, .. }
        | RecordLinkBlockingComponent::DateBucket { feature_id, .. }
        | RecordLinkBlockingComponent::CategoricalExact { feature_id } => feature_id,
    }
}

fn blocking_component_value(
    key_id: &str,
    component: &RecordLinkBlockingComponent,
    value: &RecordLinkFeatureValue,
) -> RecordLinkResult<String> {
    match (component, value) {
        (RecordLinkBlockingComponent::Exact { .. }, value) => serde_json::to_string(value)
            .map(|value| format!("exact:{value}"))
            .map_err(|error| {
                contract_error(
                    "block",
                    "blocking_component_serialize",
                    format!("failed to serialize record-link blocking value: {error}"),
                )
            }),
        (
            RecordLinkBlockingComponent::FixedDecimalBucket {
                units,
                scale,
                bucket_width_scaled_units,
                ..
            },
            RecordLinkFeatureValue::Numeric {
                units: actual_units,
                scaled_value,
                scale: actual_scale,
            },
        ) => {
            if units != actual_units || scale != actual_scale {
                return Err(contract_error(
                    "block",
                    "blocking_component_numeric_contract_mismatch",
                    format!(
                        "record-link blocking key {} expected numeric units {} scale {}, got units {} scale {}",
                        key_id, units, scale, actual_units, actual_scale
                    ),
                ));
            }
            let width = i64::try_from(*bucket_width_scaled_units).map_err(|_| {
                contract_error(
                    "block",
                    "blocking_component_width_overflow",
                    format!(
                        "record-link blocking key {} bucket_width_scaled_units is too large",
                        key_id
                    ),
                )
            })?;
            Ok(format!(
                "fixed_decimal_bucket:{units}:{scale}:{bucket_width_scaled_units}:{}",
                div_floor(*scaled_value, width)
            ))
        }
        (
            RecordLinkBlockingComponent::DateBucket { bucket_days, .. },
            RecordLinkFeatureValue::Date { value },
        ) => {
            let day = iso_day_number(value).ok_or_else(|| {
                contract_error(
                    "block",
                    "blocking_component_invalid_date",
                    format!(
                        "record-link blocking key {} saw invalid ISO date {}",
                        key_id, value
                    ),
                )
            })?;
            Ok(format!(
                "date_bucket:{bucket_days}:{}",
                div_floor(day, i64::from(*bucket_days))
            ))
        }
        (
            RecordLinkBlockingComponent::CategoricalExact { .. },
            RecordLinkFeatureValue::Categorical { value },
        ) => Ok(format!("categorical_exact:{value}")),
        (component, value) => Err(contract_error(
            "block",
            "blocking_component_kind_mismatch",
            format!(
                "record-link blocking component for feature {} is incompatible with observed {:?}",
                blocking_component_feature_id(component),
                feature_value_kind(value)
            ),
        )),
    }
}

fn div_floor(value: i64, divisor: i64) -> i64 {
    debug_assert!(divisor > 0);
    let quotient = value / divisor;
    let remainder = value % divisor;
    if remainder != 0 && value < 0 {
        quotient - 1
    } else {
        quotient
    }
}

fn blocking_policy_digest(policy: Option<&RecordLinkBlockingPolicy>) -> RecordLinkResult<String> {
    let Some(policy) = policy else {
        return Ok(String::new());
    };
    validate_record_link_blocking_policy(Some(policy))?;
    hash_json(policy)
}

fn pair_endpoint_keys(
    records: &[RecordWithInput<'_>],
    pairs: &[(usize, usize)],
) -> RecordLinkResult<BTreeSet<(String, String, String, String)>> {
    let mut keys = BTreeSet::new();
    for (left_index, right_index) in pairs {
        let Some(left) = records.get(*left_index) else {
            return Err(contract_error(
                "block",
                "candidate_pair_index_out_of_bounds",
                "record-link candidate pair referenced an unknown left index",
            ));
        };
        let Some(right) = records.get(*right_index) else {
            return Err(contract_error(
                "block",
                "candidate_pair_index_out_of_bounds",
                "record-link candidate pair referenced an unknown right index",
            ));
        };
        keys.insert(canonical_record_pair_key(
            &left.input.sidecar.source_id,
            &left.record.record_id,
            &right.input.sidecar.source_id,
            &right.record.record_id,
        ));
    }
    Ok(keys)
}

fn canonical_endpoint_pair_key(
    left: &RecordLinkEndpoint,
    right: &RecordLinkEndpoint,
) -> (String, String, String, String) {
    canonical_record_pair_key(
        &left.source_id,
        &left.record_id,
        &right.source_id,
        &right.record_id,
    )
}

fn canonical_record_pair_key(
    left_source_id: &str,
    left_record_id: &str,
    right_source_id: &str,
    right_record_id: &str,
) -> (String, String, String, String) {
    let left = (left_source_id.to_string(), left_record_id.to_string());
    let right = (right_source_id.to_string(), right_record_id.to_string());
    if left <= right {
        (left.0, left.1, right.0, right.1)
    } else {
        (right.0, right.1, left.0, left.1)
    }
}

fn endpoint_for_record(
    index: &RecordLinkSurfaceIndex,
    source_id: &str,
    record: &RecordLinkInputRecord,
) -> RecordLinkResult<RecordLinkEndpoint> {
    index
        .endpoint_by_record_key
        .get(&(source_id.to_string(), record.record_id.clone()))
        .cloned()
        .ok_or_else(|| {
            contract_error(
                "block",
                "missing_surface_binding",
                format!("record {} has no bound surface", record.record_id),
            )
        })
}

fn endpoint_key(endpoint: &RecordLinkEndpoint) -> (String, String) {
    (endpoint.source_id.clone(), endpoint.record_id.clone())
}

fn endpoint_key_string(endpoint: &RecordLinkEndpoint) -> String {
    format!("{}\u{0}{}", endpoint.source_id, endpoint.record_id)
}

#[derive(Debug)]
struct PairComparison {
    support_features: Vec<RecordLinkFeatureComparison>,
    conflict_features: Vec<RecordLinkFeatureComparison>,
    missing_feature_ids: Vec<String>,
    unconfigured_feature_ids: Vec<String>,
    mismatch_feature_ids: Vec<String>,
    scale_mismatch_feature_ids: Vec<String>,
    incomparable_feature_ids: Vec<String>,
    score_hint_units: u64,
    hard_cannot_link: bool,
}

fn compare_records(
    left: &RecordLinkInputRecord,
    right: &RecordLinkInputRecord,
    feature_policies: &BTreeMap<String, RecordLinkFeaturePolicy>,
) -> RecordLinkResult<PairComparison> {
    let left_features = feature_map(left)?;
    let right_features = feature_map(right)?;
    compare_feature_maps(&left_features, &right_features, feature_policies)
}

fn compare_feature_maps(
    left_features: &BTreeMap<String, RecordLinkFeatureValue>,
    right_features: &BTreeMap<String, RecordLinkFeatureValue>,
    feature_policies: &BTreeMap<String, RecordLinkFeaturePolicy>,
) -> RecordLinkResult<PairComparison> {
    let mut support_features = Vec::new();
    let mut conflict_features = Vec::new();
    let mut missing_feature_ids = Vec::new();
    let mut unconfigured_feature_ids = Vec::new();
    let mut mismatch_feature_ids = Vec::new();
    let mut scale_mismatch_feature_ids = Vec::new();
    let mut incomparable_feature_ids = Vec::new();
    let feature_ids = left_features
        .keys()
        .chain(right_features.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    for feature_id in feature_ids {
        match (
            left_features.get(&feature_id),
            right_features.get(&feature_id),
        ) {
            (Some(left_value), Some(right_value)) => {
                match compare_feature_values(
                    &feature_id,
                    left_value,
                    right_value,
                    feature_policies,
                )? {
                    FeatureDecision::Support(feature) => support_features.push(feature),
                    FeatureDecision::Conflict(feature) => conflict_features.push(feature),
                    FeatureDecision::Unconfigured => unconfigured_feature_ids.push(feature_id),
                    FeatureDecision::Mismatch => mismatch_feature_ids.push(feature_id),
                    FeatureDecision::ScaleMismatch => scale_mismatch_feature_ids.push(feature_id),
                    FeatureDecision::Incomparable => incomparable_feature_ids.push(feature_id),
                }
            }
            _ => missing_feature_ids.push(feature_id),
        }
    }
    support_features.sort_by(|left, right| left.feature_id.cmp(&right.feature_id));
    conflict_features.sort_by(|left, right| left.feature_id.cmp(&right.feature_id));
    missing_feature_ids.sort();
    unconfigured_feature_ids.sort();
    mismatch_feature_ids.sort();
    scale_mismatch_feature_ids.sort();
    incomparable_feature_ids.sort();
    let score_hint_units = checked_support_score(&support_features)?;
    let hard_cannot_link = !conflict_features.is_empty();
    Ok(PairComparison {
        support_features,
        conflict_features,
        missing_feature_ids,
        unconfigured_feature_ids,
        mismatch_feature_ids,
        scale_mismatch_feature_ids,
        incomparable_feature_ids,
        score_hint_units,
        hard_cannot_link,
    })
}

fn feature_map(
    record: &RecordLinkInputRecord,
) -> RecordLinkResult<BTreeMap<String, RecordLinkFeatureValue>> {
    let mut features = BTreeMap::new();
    for view in &record.comparison_views {
        let (feature_id, value) = match view {
            RecordLinkComparisonView::Numeric {
                feature_id,
                units,
                scaled_value,
                scale,
                ..
            } => (
                feature_id.clone(),
                RecordLinkFeatureValue::Numeric {
                    units: units.clone(),
                    scaled_value: *scaled_value,
                    scale: *scale,
                },
            ),
            RecordLinkComparisonView::Date {
                feature_id, value, ..
            } => (
                feature_id.clone(),
                RecordLinkFeatureValue::Date {
                    value: value.clone(),
                },
            ),
            RecordLinkComparisonView::Categorical {
                feature_id, value, ..
            } => (
                feature_id.clone(),
                RecordLinkFeatureValue::Categorical {
                    value: value.clone(),
                },
            ),
        };
        if features.insert(feature_id.clone(), value).is_some() {
            return Err(contract_error(
                "record_link_input",
                "duplicate_feature_id",
                format!(
                    "record {} repeats comparison feature_id {}",
                    record.record_id, feature_id
                ),
            ));
        }
    }
    Ok(features)
}

#[derive(Debug)]
struct RecordInputBinding {
    source_id: String,
    record_id: String,
    observation_id: String,
    assignment_ref: Option<RecordLinkAssignmentRef>,
    sidecar_hash: String,
    features: BTreeMap<String, RecordLinkFeatureValue>,
}

fn input_record_index(
    input_set: &RecordLinkInputSet,
) -> RecordLinkResult<BTreeMap<(String, String), RecordInputBinding>> {
    let mut index = BTreeMap::new();
    for input in &input_set.inputs {
        for record in &input.sidecar.records {
            let key = (input.sidecar.source_id.clone(), record.record_id.clone());
            let binding = RecordInputBinding {
                source_id: input.sidecar.source_id.clone(),
                record_id: record.record_id.clone(),
                observation_id: record.subject_observation_ref.observation_id.clone(),
                assignment_ref: record.assignment_ref.clone(),
                sidecar_hash: input.sidecar.artifact_content_hash.clone(),
                features: feature_map(record)?,
            };
            if index.insert(key, binding).is_some() {
                return Err(contract_error(
                    "record_link_input",
                    "duplicate_record_id",
                    format!(
                        "record_id {} appears more than once for source {}",
                        record.record_id, input.sidecar.source_id
                    ),
                ));
            }
        }
    }
    Ok(index)
}

fn validate_candidate_endpoint<'a>(
    record_index: &'a BTreeMap<(String, String), RecordInputBinding>,
    endpoint: &RecordLinkEndpoint,
) -> RecordLinkResult<&'a RecordInputBinding> {
    if endpoint.surface_id.trim().is_empty() {
        return Err(contract_error(
            "block",
            "candidate_empty_surface",
            "candidate endpoint surface_id must be non-empty",
        ));
    }
    let Some(binding) = record_index.get(&(endpoint.source_id.clone(), endpoint.record_id.clone()))
    else {
        return Err(contract_error(
            "block",
            "candidate_unknown_record",
            format!(
                "candidate endpoint {}:{} does not exist in the bound inputs",
                endpoint.source_id, endpoint.record_id
            ),
        ));
    };
    if endpoint.observation_id != binding.observation_id
        || endpoint.assignment_ref != binding.assignment_ref
    {
        return Err(contract_error(
            "block",
            "candidate_endpoint_binding_mismatch",
            format!(
                "candidate endpoint {}:{} does not match the input record binding",
                endpoint.source_id, endpoint.record_id
            ),
        ));
    }
    if endpoint.sidecar_hash != binding.sidecar_hash {
        return Err(contract_error(
            "block",
            "candidate_endpoint_hash_mismatch",
            format!(
                "candidate endpoint {}:{} sidecar hash does not match the input ref",
                endpoint.source_id, endpoint.record_id
            ),
        ));
    }
    Ok(binding)
}

fn validate_candidate_features(
    candidate: &RecordLinkCandidate,
    left: &RecordInputBinding,
    right: &RecordInputBinding,
) -> RecordLinkResult<()> {
    for feature in candidate
        .support_features
        .iter()
        .chain(candidate.conflict_features.iter())
    {
        if feature.score_units == 0 {
            return Err(contract_error(
                "block",
                "candidate_zero_score",
                format!(
                    "candidate {} feature {} has zero score_units",
                    candidate.candidate_id, feature.feature_id
                ),
            ));
        }
        let Some(left_value) = left.features.get(&feature.feature_id) else {
            return Err(candidate_feature_binding_error(
                candidate,
                feature,
                &left.source_id,
                &left.record_id,
            ));
        };
        let Some(right_value) = right.features.get(&feature.feature_id) else {
            return Err(candidate_feature_binding_error(
                candidate,
                feature,
                &right.source_id,
                &right.record_id,
            ));
        };
        if feature.kind != feature_value_kind(left_value)
            || feature.kind != feature_value_kind(right_value)
            || feature.left != *left_value
            || feature.right != *right_value
        {
            return Err(contract_error(
                "block",
                "candidate_feature_binding_mismatch",
                format!(
                    "candidate {} feature {} does not match bound input record values",
                    candidate.candidate_id, feature.feature_id
                ),
            ));
        }
    }
    Ok(())
}

fn candidate_feature_binding_error(
    candidate: &RecordLinkCandidate,
    feature: &RecordLinkFeatureComparison,
    source_id: &str,
    record_id: &str,
) -> RecordLinkCoreError {
    contract_error(
        "block",
        "candidate_feature_binding_mismatch",
        format!(
            "candidate {} feature {} is not present on bound input record {}:{}",
            candidate.candidate_id, feature.feature_id, source_id, record_id
        ),
    )
}

fn validate_feature_policies(
    feature_policies: &BTreeMap<String, RecordLinkFeaturePolicy>,
) -> RecordLinkResult<()> {
    for (feature_id, policy) in feature_policies {
        if feature_id != &policy.feature_id {
            return Err(contract_error(
                "block",
                "feature_policy_key_mismatch",
                format!(
                    "feature policy map key {} does not match policy feature_id {}",
                    feature_id, policy.feature_id
                ),
            ));
        }
        if policy.score_units == 0 {
            return Err(contract_error(
                "block",
                "feature_policy_zero_score",
                format!("feature policy {feature_id} score_units must be non-zero"),
            ));
        }
        if !support_policy_matches_kind(policy.kind, &policy.support) {
            return Err(contract_error(
                "block",
                "feature_policy_support_mismatch",
                format!(
                    "feature policy {} support {:?} does not match kind {:?}",
                    feature_id, policy.support, policy.kind
                ),
            ));
        }
    }
    Ok(())
}

fn feature_policy_digest(
    feature_policies: &BTreeMap<String, RecordLinkFeaturePolicy>,
) -> RecordLinkResult<String> {
    validate_feature_policies(feature_policies)?;
    hash_json(&json!({
        "version": "canon.entity.record_link_feature_policy_set.v1",
        "feature_policies": feature_policies,
    }))
}

fn evidence_policy_content_hash(
    feature_id: &str,
    reason_code: &str,
    feature_policy_digest: &str,
    feature_policies: &BTreeMap<String, RecordLinkFeaturePolicy>,
) -> RecordLinkResult<String> {
    require_hash(feature_policy_digest, "evidence", "feature_policy_digest")?;
    let Some(feature_policy) = feature_policies.get(feature_id) else {
        return Err(contract_error(
            "evidence",
            "missing_feature_policy",
            format!("evidence feature {feature_id} has no independently supplied policy"),
        ));
    };
    hash_json(&json!({
        "version": "canon.entity.record_link_evidence_policy_ref.v1",
        "feature_policy_digest": feature_policy_digest,
        "feature_id": feature_id,
        "reason_code": reason_code,
        "feature_policy": feature_policy,
    }))
}

fn support_policy_matches_kind(
    kind: RecordLinkFeatureKind,
    support: &RecordLinkSupportPolicy,
) -> bool {
    matches!(
        (kind, support),
        (_, RecordLinkSupportPolicy::Exact)
            | (
                RecordLinkFeatureKind::Numeric,
                RecordLinkSupportPolicy::NumericTolerance { .. },
            )
            | (
                RecordLinkFeatureKind::Date,
                RecordLinkSupportPolicy::DateNear { .. },
            )
            | (
                RecordLinkFeatureKind::Categorical,
                RecordLinkSupportPolicy::CategoricalExact,
            )
    )
}

fn validate_policy_for_observed_values(
    feature_id: &str,
    policy: &RecordLinkFeaturePolicy,
    left: &RecordLinkFeatureValue,
    right: &RecordLinkFeatureValue,
) -> RecordLinkResult<()> {
    if policy.feature_id != feature_id {
        return Err(contract_error(
            "block",
            "feature_policy_key_mismatch",
            format!(
                "feature policy lookup {} returned policy feature_id {}",
                feature_id, policy.feature_id
            ),
        ));
    }
    let left_kind = feature_value_kind(left);
    let right_kind = feature_value_kind(right);
    if left_kind != right_kind {
        return Ok(());
    }
    if policy.kind != left_kind {
        return Err(contract_error(
            "block",
            "feature_policy_kind_mismatch",
            format!(
                "feature policy {} kind {:?} does not match observed kind {:?}",
                feature_id, policy.kind, left_kind
            ),
        ));
    }
    if !support_policy_matches_kind(policy.kind, &policy.support) {
        return Err(contract_error(
            "block",
            "feature_policy_support_mismatch",
            format!(
                "feature policy {} support {:?} does not match kind {:?}",
                feature_id, policy.support, policy.kind
            ),
        ));
    }
    Ok(())
}

fn feature_value_kind(value: &RecordLinkFeatureValue) -> RecordLinkFeatureKind {
    match value {
        RecordLinkFeatureValue::Numeric { .. } => RecordLinkFeatureKind::Numeric,
        RecordLinkFeatureValue::Date { .. } => RecordLinkFeatureKind::Date,
        RecordLinkFeatureValue::Categorical { .. } => RecordLinkFeatureKind::Categorical,
    }
}

fn checked_support_score(features: &[RecordLinkFeatureComparison]) -> RecordLinkResult<u64> {
    features.iter().try_fold(0_u64, |total, feature| {
        total.checked_add(feature.score_units).ok_or_else(|| {
            contract_error(
                "block",
                "candidate_score_overflow",
                format!(
                    "record-link support score overflowed at feature {}",
                    feature.feature_id
                ),
            )
        })
    })
}

enum FeatureDecision {
    Support(RecordLinkFeatureComparison),
    Conflict(RecordLinkFeatureComparison),
    Unconfigured,
    Mismatch,
    ScaleMismatch,
    Incomparable,
}

fn compare_feature_values(
    feature_id: &str,
    left: &RecordLinkFeatureValue,
    right: &RecordLinkFeatureValue,
    feature_policies: &BTreeMap<String, RecordLinkFeaturePolicy>,
) -> RecordLinkResult<FeatureDecision> {
    let Some(policy) = feature_policies.get(feature_id) else {
        return Ok(FeatureDecision::Unconfigured);
    };
    validate_policy_for_observed_values(feature_id, policy, left, right)?;
    let score_units = policy.score_units;
    match (left, right) {
        (
            RecordLinkFeatureValue::Numeric {
                units: left_units,
                scaled_value: left_value,
                scale: left_scale,
            },
            RecordLinkFeatureValue::Numeric {
                units: right_units,
                scaled_value: right_value,
                scale: right_scale,
            },
        ) => {
            if left_units != right_units || left_scale != right_scale {
                Ok(FeatureDecision::ScaleMismatch)
            } else if numeric_values_support(*left_value, *right_value, &policy.support) {
                Ok(FeatureDecision::Support(feature_comparison(
                    feature_id,
                    RecordLinkFeatureKind::Numeric,
                    left.clone(),
                    right.clone(),
                    score_units,
                )))
            } else if policy.hard_conflict_on_mismatch {
                Ok(FeatureDecision::Conflict(feature_comparison(
                    feature_id,
                    RecordLinkFeatureKind::Numeric,
                    left.clone(),
                    right.clone(),
                    u64::MAX,
                )))
            } else {
                Ok(FeatureDecision::Mismatch)
            }
        }
        (
            RecordLinkFeatureValue::Date { value: left_value },
            RecordLinkFeatureValue::Date { value: right_value },
        ) => {
            let comparison = feature_comparison(
                feature_id,
                RecordLinkFeatureKind::Date,
                left.clone(),
                right.clone(),
                score_units,
            );
            if date_values_support(left_value, right_value, &policy.support) {
                Ok(FeatureDecision::Support(comparison))
            } else if policy.hard_conflict_on_mismatch {
                Ok(FeatureDecision::Conflict(feature_comparison(
                    feature_id,
                    RecordLinkFeatureKind::Date,
                    left.clone(),
                    right.clone(),
                    u64::MAX,
                )))
            } else {
                Ok(FeatureDecision::Mismatch)
            }
        }
        (
            RecordLinkFeatureValue::Categorical { value: left_value },
            RecordLinkFeatureValue::Categorical { value: right_value },
        ) => {
            let comparison = feature_comparison(
                feature_id,
                RecordLinkFeatureKind::Categorical,
                left.clone(),
                right.clone(),
                score_units,
            );
            if categorical_values_support(left_value, right_value, &policy.support) {
                Ok(FeatureDecision::Support(comparison))
            } else if policy.hard_conflict_on_mismatch {
                Ok(FeatureDecision::Conflict(feature_comparison(
                    feature_id,
                    RecordLinkFeatureKind::Categorical,
                    left.clone(),
                    right.clone(),
                    u64::MAX,
                )))
            } else {
                Ok(FeatureDecision::Mismatch)
            }
        }
        _ => Ok(FeatureDecision::Incomparable),
    }
}

fn numeric_values_support(left: i64, right: i64, policy: &RecordLinkSupportPolicy) -> bool {
    match policy {
        RecordLinkSupportPolicy::Exact => left == right,
        RecordLinkSupportPolicy::NumericTolerance {
            tolerance_scaled_units,
        } => left.abs_diff(right) <= *tolerance_scaled_units,
        _ => false,
    }
}

fn date_values_support(left: &str, right: &str, policy: &RecordLinkSupportPolicy) -> bool {
    match policy {
        RecordLinkSupportPolicy::Exact => left == right,
        RecordLinkSupportPolicy::DateNear { max_days } => {
            let Some(left_day) = iso_day_number(left) else {
                return false;
            };
            let Some(right_day) = iso_day_number(right) else {
                return false;
            };
            left_day.abs_diff(right_day) <= u64::from(*max_days)
        }
        _ => false,
    }
}

fn categorical_values_support(left: &str, right: &str, policy: &RecordLinkSupportPolicy) -> bool {
    match policy {
        RecordLinkSupportPolicy::Exact | RecordLinkSupportPolicy::CategoricalExact => left == right,
        _ => false,
    }
}

fn iso_day_number(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year = parse_fixed_u32(&value[0..4])?;
    let month = parse_fixed_u32(&value[5..7])?;
    let day = parse_fixed_u32(&value[8..10])?;
    if !(1..=12).contains(&month) {
        return None;
    }
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return None,
    };
    if !(1..=max_day).contains(&day) {
        return None;
    }
    Some(days_from_civil(
        i64::from(year),
        i64::from(month),
        i64::from(day),
    ))
}

fn parse_fixed_u32(value: &str) -> Option<u32> {
    value
        .chars()
        .all(|ch| ch.is_ascii_digit())
        .then(|| value.parse().ok())
        .flatten()
}

fn is_leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && !year.is_multiple_of(100) || year.is_multiple_of(400)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn feature_comparison(
    feature_id: &str,
    kind: RecordLinkFeatureKind,
    left: RecordLinkFeatureValue,
    right: RecordLinkFeatureValue,
    score_units: u64,
) -> RecordLinkFeatureComparison {
    RecordLinkFeatureComparison {
        feature_id: feature_id.to_string(),
        kind,
        left,
        right,
        score_units,
    }
}

fn pair_abstentions(
    left: &RecordLinkEndpoint,
    right: &RecordLinkEndpoint,
    comparison: &PairComparison,
) -> RecordLinkResult<Vec<RecordLinkCandidateAbstention>> {
    let mut abstentions = Vec::new();
    if !comparison.missing_feature_ids.is_empty() {
        abstentions.push(candidate_abstention(
            RecordLinkCandidateAbstentionReason::MissingComparison,
            Some(left.clone()),
            Some(right.clone()),
            Vec::new(),
            comparison.missing_feature_ids.clone(),
        )?);
    }
    if !comparison.unconfigured_feature_ids.is_empty() {
        abstentions.push(candidate_abstention(
            RecordLinkCandidateAbstentionReason::UnconfiguredFeature,
            Some(left.clone()),
            Some(right.clone()),
            Vec::new(),
            comparison.unconfigured_feature_ids.clone(),
        )?);
    }
    if !comparison.mismatch_feature_ids.is_empty() {
        abstentions.push(candidate_abstention(
            RecordLinkCandidateAbstentionReason::Mismatch,
            Some(left.clone()),
            Some(right.clone()),
            Vec::new(),
            comparison.mismatch_feature_ids.clone(),
        )?);
    }
    if !comparison.scale_mismatch_feature_ids.is_empty() {
        abstentions.push(candidate_abstention(
            RecordLinkCandidateAbstentionReason::ScaleMismatch,
            Some(left.clone()),
            Some(right.clone()),
            Vec::new(),
            comparison.scale_mismatch_feature_ids.clone(),
        )?);
    }
    if !comparison.incomparable_feature_ids.is_empty() {
        abstentions.push(candidate_abstention(
            RecordLinkCandidateAbstentionReason::Incomparable,
            Some(left.clone()),
            Some(right.clone()),
            Vec::new(),
            comparison.incomparable_feature_ids.clone(),
        )?);
    }
    Ok(abstentions)
}

fn enforce_candidate_bounds(
    candidates: Vec<RecordLinkCandidate>,
    config: &RecordLinkCandidateConfig,
) -> RecordLinkResult<(Vec<RecordLinkCandidate>, Vec<RecordLinkCandidateAbstention>)> {
    let mut disallowed = BTreeSet::new();
    let mut abstentions = Vec::new();
    let mut by_record = BTreeMap::<String, Vec<&RecordLinkCandidate>>::new();
    for candidate in &candidates {
        by_record
            .entry(endpoint_key_string(&candidate.left))
            .or_default()
            .push(candidate);
        by_record
            .entry(endpoint_key_string(&candidate.right))
            .or_default()
            .push(candidate);
    }
    for per_record in by_record.values_mut() {
        per_record.sort_by(|left, right| {
            right
                .score_hint_units
                .cmp(&left.score_hint_units)
                .then_with(|| left.candidate_id.cmp(&right.candidate_id))
        });
        let rankable = per_record
            .iter()
            .copied()
            .filter(|candidate| !candidate.hard_cannot_link)
            .collect::<Vec<_>>();
        if config.require_unique_best_per_record && rankable.len() > 1 {
            let top_score = rankable[0].score_hint_units;
            let top = rankable
                .iter()
                .filter(|candidate| candidate.score_hint_units == top_score)
                .copied()
                .collect::<Vec<_>>();
            if top.len() > 1 {
                for candidate in top {
                    disallowed.insert(candidate.candidate_id.clone());
                    abstentions.push(candidate_abstention(
                        RecordLinkCandidateAbstentionReason::DuplicateBest,
                        Some(candidate.left.clone()),
                        Some(candidate.right.clone()),
                        vec![candidate.candidate_id.clone()],
                        candidate
                            .support_features
                            .iter()
                            .map(|feature| feature.feature_id.clone())
                            .collect(),
                    )?);
                }
            }
        }
        if rankable.len() > config.max_candidates_per_record {
            let boundary = rankable[config.max_candidates_per_record - 1].score_hint_units;
            let has_boundary_tie = rankable[config.max_candidates_per_record..]
                .iter()
                .any(|candidate| candidate.score_hint_units == boundary);
            for candidate in rankable.iter().skip(config.max_candidates_per_record) {
                disallowed.insert(candidate.candidate_id.clone());
                abstentions.push(candidate_abstention(
                    if has_boundary_tie && candidate.score_hint_units == boundary {
                        RecordLinkCandidateAbstentionReason::Tie
                    } else {
                        RecordLinkCandidateAbstentionReason::CardinalityExceeded
                    },
                    Some(candidate.left.clone()),
                    Some(candidate.right.clone()),
                    vec![candidate.candidate_id.clone()],
                    Vec::new(),
                )?);
            }
            if has_boundary_tie {
                for candidate in rankable
                    .iter()
                    .take(config.max_candidates_per_record)
                    .filter(|candidate| candidate.score_hint_units == boundary)
                {
                    disallowed.insert(candidate.candidate_id.clone());
                    abstentions.push(candidate_abstention(
                        RecordLinkCandidateAbstentionReason::Tie,
                        Some(candidate.left.clone()),
                        Some(candidate.right.clone()),
                        vec![candidate.candidate_id.clone()],
                        Vec::new(),
                    )?);
                }
            }
        }
    }
    let kept = candidates
        .into_iter()
        .filter(|candidate| !disallowed.contains(&candidate.candidate_id))
        .collect::<Vec<_>>();
    Ok((kept, abstentions))
}

fn candidate_abstention(
    reason: RecordLinkCandidateAbstentionReason,
    left: Option<RecordLinkEndpoint>,
    right: Option<RecordLinkEndpoint>,
    mut candidate_ids: Vec<String>,
    mut feature_ids: Vec<String>,
) -> RecordLinkResult<RecordLinkCandidateAbstention> {
    candidate_ids.sort();
    candidate_ids.dedup();
    feature_ids.sort();
    feature_ids.dedup();
    let seed = json!({
        "reason": reason,
        "left": left,
        "right": right,
        "candidate_ids": candidate_ids,
        "feature_ids": feature_ids,
    });
    Ok(RecordLinkCandidateAbstention {
        abstention_id: stable_id("record_link_abstention", &seed)?,
        reason,
        left,
        right,
        candidate_ids,
        feature_ids,
    })
}

fn record_link_evidence_record(
    candidate: &RecordLinkCandidate,
    feature: &RecordLinkFeatureComparison,
    kind: EvidenceKind,
    reason_code: &str,
    authority_basis: Option<EvidenceAuthorityBasis>,
    policy_content_hash: String,
) -> RecordLinkResult<EvidenceRecord> {
    let target = EvidenceTarget::RecordLink {
        left_source: candidate.left.source_id.clone(),
        left_record_id: candidate.left.record_id.clone(),
        right_source: candidate.right.source_id.clone(),
        right_record_id: candidate.right.record_id.clone(),
    };
    let mut provenance = vec![
        EvidenceProvenanceRef {
            source_type: "record_link_input".to_string(),
            source_id: candidate.left.source_id.clone(),
            locator: candidate.left.record_id.clone(),
            content_hash: candidate.left.sidecar_hash.clone(),
            observed_at: None,
        },
        EvidenceProvenanceRef {
            source_type: "record_link_input".to_string(),
            source_id: candidate.right.source_id.clone(),
            locator: candidate.right.record_id.clone(),
            content_hash: candidate.right.sidecar_hash.clone(),
            observed_at: None,
        },
    ];
    provenance.sort_by(|left, right| {
        left.source_id
            .cmp(&right.source_id)
            .then_with(|| left.locator.cmp(&right.locator))
    });
    Ok(EvidenceRecord {
        version: String::new(),
        evidence_id: String::new(),
        kind,
        target,
        operator: EvidenceOperatorRef {
            namespace: RECORD_LINK_OPERATOR_NAMESPACE.to_string(),
            operator_id: format!("record_link:{}", feature.feature_id),
            operator_version: RECORD_LINK_OPERATOR_VERSION.to_string(),
            adapter_id: None,
        },
        reason_code: reason_code.to_string(),
        policy: EvidencePolicyRef {
            policy_id: "record_link.core".to_string(),
            policy_version: "1".to_string(),
            content_hash: policy_content_hash,
        },
        authority_basis,
        scope: Some(EvidenceScope {
            scope_type: "record_link_candidate".to_string(),
            scope_id: candidate.candidate_id.clone(),
            namespace: Some(RECORD_LINK_OPERATOR_NAMESPACE.to_string()),
        }),
        temporal_scope: None,
        provenance,
        measurements: feature_measurements(feature),
        extensions: vec![EvidenceExtension {
            namespace: "record_link".to_string(),
            schema_ref: "canon.entity.record_link_evidence_extension.v1".to_string(),
            payload: BTreeMap::from([
                (
                    "left_surface_id".to_string(),
                    EvidenceExtensionValue::String(candidate.left.surface_id.clone()),
                ),
                (
                    "right_surface_id".to_string(),
                    EvidenceExtensionValue::String(candidate.right.surface_id.clone()),
                ),
                (
                    "candidate_id".to_string(),
                    EvidenceExtensionValue::String(candidate.candidate_id.clone()),
                ),
            ]),
        }],
    })
}

fn feature_measurements(feature: &RecordLinkFeatureComparison) -> Vec<EvidenceMeasurement> {
    match (&feature.kind, &feature.left) {
        (
            RecordLinkFeatureKind::Numeric,
            RecordLinkFeatureValue::Numeric {
                units,
                scaled_value,
                scale,
            },
        ) => vec![EvidenceMeasurement::Numeric(EvidenceNumericMeasurement {
            feature_id: feature.feature_id.clone(),
            units: units.clone(),
            scaled_value: *scaled_value,
            scale: *scale,
        })],
        (RecordLinkFeatureKind::Date, RecordLinkFeatureValue::Date { value })
        | (RecordLinkFeatureKind::Categorical, RecordLinkFeatureValue::Categorical { value }) => {
            vec![EvidenceMeasurement::Categorical(
                EvidenceCategoricalMeasurement {
                    feature_id: feature.feature_id.clone(),
                    value: value.clone(),
                },
            )]
        }
        _ => vec![EvidenceMeasurement::Boolean(EvidenceBooleanMeasurement {
            feature_id: feature.feature_id.clone(),
            value: false,
        })],
    }
}

fn alignment_record(
    candidate: &RecordLinkCandidate,
    decision: AssignmentAlignmentDecisionKind,
    evidence_ids_by_candidate: &BTreeMap<String, Vec<String>>,
    mut reason_codes: Vec<String>,
) -> RecordLinkResult<AssignmentAlignmentRecord> {
    let mut evidence_ids = evidence_ids_by_candidate
        .get(&candidate.candidate_id)
        .cloned()
        .unwrap_or_default();
    evidence_ids.sort();
    reason_codes.sort();
    reason_codes.dedup();
    let seed = json!({
        "candidate_id": candidate.candidate_id,
        "left": candidate.left,
        "right": candidate.right,
        "decision": decision,
        "evidence_ids": evidence_ids,
        "reason_codes": reason_codes,
    });
    Ok(AssignmentAlignmentRecord {
        alignment_id: stable_id("assignment_alignment", &seed)?,
        left: candidate.left.clone(),
        right: candidate.right.clone(),
        decision,
        evidence_ids,
        reason_codes,
    })
}

fn assignment_key(endpoint: &RecordLinkEndpoint) -> String {
    endpoint
        .assignment_ref
        .as_ref()
        .map(|assignment| {
            format!(
                "{}:{}:{}:{}:{}",
                endpoint.source_id,
                assignment.mapping_id,
                assignment.role_id,
                assignment.assignee_type_id,
                assignment.assignment_id
            )
        })
        .unwrap_or_else(|| format!("{}:<missing>", endpoint.record_id))
}

fn surface_pair(candidate: &RecordLinkCandidate) -> (String, String) {
    if candidate.left.surface_id <= candidate.right.surface_id {
        (
            candidate.left.surface_id.clone(),
            candidate.right.surface_id.clone(),
        )
    } else {
        (
            candidate.right.surface_id.clone(),
            candidate.left.surface_id.clone(),
        )
    }
}

fn candidate_cmp(left: &RecordLinkCandidate, right: &RecordLinkCandidate) -> std::cmp::Ordering {
    left.left
        .surface_id
        .cmp(&right.left.surface_id)
        .then_with(|| left.right.surface_id.cmp(&right.right.surface_id))
        .then_with(|| left.candidate_id.cmp(&right.candidate_id))
}

fn abstention_cmp(
    left: &RecordLinkCandidateAbstention,
    right: &RecordLinkCandidateAbstention,
) -> std::cmp::Ordering {
    left.abstention_id.cmp(&right.abstention_id)
}

fn candidate_set_summary(candidate_set: &RecordLinkCandidateSet) -> BTreeMap<String, u64> {
    let mut summary = BTreeMap::from([
        (
            "candidate_count".to_string(),
            candidate_set.candidates.len() as u64,
        ),
        (
            "abstention_count".to_string(),
            candidate_set.abstentions.len() as u64,
        ),
        (
            "hard_cannot_link_count".to_string(),
            candidate_set
                .candidates
                .iter()
                .filter(|candidate| candidate.hard_cannot_link)
                .count() as u64,
        ),
    ]);
    if !candidate_set.pair_accounting.is_empty() {
        summary.insert(
            "cross_source_pair_count".to_string(),
            candidate_set.pair_accounting.cross_source_pair_count,
        );
        summary.insert(
            "admitted_pair_count".to_string(),
            candidate_set.pair_accounting.admitted_pair_count,
        );
        summary.insert(
            "suppressed_pair_count".to_string(),
            candidate_set.pair_accounting.suppressed_pair_count,
        );
        summary.insert(
            "scored_pair_count".to_string(),
            candidate_set.pair_accounting.scored_pair_count,
        );
        summary.insert(
            "blocking_policy_miss_count".to_string(),
            candidate_set.pair_accounting.blocking_policy_miss_count,
        );
        summary.insert(
            "comparison_abstention_count".to_string(),
            candidate_set.pair_accounting.comparison_abstention_count,
        );
        summary.insert(
            "ranking_abstention_count".to_string(),
            candidate_set.pair_accounting.ranking_abstention_count,
        );
    }
    if let Some(policy) = &candidate_set.blocking_policy {
        summary.insert("blocking_policy_count".to_string(), 1);
        summary.insert("blocking_key_count".to_string(), policy.keys.len() as u64);
    }
    summary
}

fn comparison_abstention_count(abstentions: &[RecordLinkCandidateAbstention]) -> u64 {
    abstentions
        .iter()
        .filter(|abstention| {
            matches!(
                abstention.reason,
                RecordLinkCandidateAbstentionReason::MissingComparison
                    | RecordLinkCandidateAbstentionReason::UnconfiguredFeature
                    | RecordLinkCandidateAbstentionReason::Mismatch
                    | RecordLinkCandidateAbstentionReason::ScaleMismatch
                    | RecordLinkCandidateAbstentionReason::Incomparable
            )
        })
        .count() as u64
}

fn ranking_abstention_count(abstentions: &[RecordLinkCandidateAbstention]) -> u64 {
    abstentions
        .iter()
        .filter(|abstention| {
            matches!(
                abstention.reason,
                RecordLinkCandidateAbstentionReason::Tie
                    | RecordLinkCandidateAbstentionReason::DuplicateBest
                    | RecordLinkCandidateAbstentionReason::CardinalityExceeded
            )
        })
        .count() as u64
}

fn assignment_alignment_summary(sidecar: &AssignmentAlignmentSidecar) -> BTreeMap<String, u64> {
    BTreeMap::from([
        (
            "alignment_count".to_string(),
            sidecar.alignments.len() as u64,
        ),
        (
            "abstention_count".to_string(),
            sidecar.abstentions.len() as u64,
        ),
        (
            "cannot_link_veto_count".to_string(),
            sidecar
                .alignments
                .iter()
                .filter(|record| record.decision == AssignmentAlignmentDecisionKind::CannotLinkVeto)
                .count() as u64,
        ),
    ])
}

fn hash_candidate_set_without_self(
    candidate_set: &RecordLinkCandidateSet,
) -> RecordLinkResult<String> {
    let mut hashable = candidate_set.clone();
    hashable.content_hash.clear();
    hash_json(&hashable)
}

fn hash_assignment_alignment_without_self(
    sidecar: &AssignmentAlignmentSidecar,
) -> RecordLinkResult<String> {
    let mut hashable = sidecar.clone();
    hashable.artifact_content_hash.clear();
    hash_json(&hashable)
}

fn hash_json(value: &impl Serialize) -> RecordLinkResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        contract_error(
            "record_link_hash",
            "serialize",
            format!("failed to serialize record-link hash material: {error}"),
        )
    })?;
    Ok(hash_bytes(&bytes))
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn stable_id(kind: &str, value: &impl Serialize) -> RecordLinkResult<String> {
    let hash = hash_json(value)?;
    Ok(format!("{kind}:{hash}"))
}

fn require_hash(value: &str, stage: &str, field: &str) -> RecordLinkResult<()> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(contract_error(
            stage,
            field,
            format!("{field} must use blake3: prefix"),
        ));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(contract_error(
            stage,
            field,
            format!("{field} must be a full BLAKE3 hex digest"),
        ));
    }
    Ok(())
}

fn contract_error(
    stage: impl Into<String>,
    reason: impl Into<String>,
    message: impl Into<String>,
) -> RecordLinkCoreError {
    RecordLinkCoreError::new(
        RecordLinkCoreErrorCode::ArtifactContract,
        stage,
        reason,
        message,
    )
}
