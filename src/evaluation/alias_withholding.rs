#![forbid(unsafe_code)]

//! Counterfactual alias-withholding benchmark contract.
//!
//! This module builds clean base registry snapshots by withholding one reviewed
//! alias from an incumbent entity, checks that exact lookup cannot see the
//! withheld surface, rejects side-channel leaks, and records entity-engine
//! attach/abstain/reject outcomes without giving credit for relation-only
//! guessing.

use crate::{
    entity::{
        apply::ApplyRunArtifact,
        audit::{EntityAuditArtifact, EntityAuditGateStatus},
        block::{
            BlockCandidateGenerationDiagnostics, BlockCandidateRecord,
            CandidateRecallEvaluationRequest, evaluate_candidate_recall,
        },
        block_artifact::{
            BlockCandidateArtifact, ExactBucketAssertion,
            validate_block_candidate_artifact_contract, validate_block_candidate_payload_hashes,
        },
        review::{
            LinkReviewQueueRequest, ReviewExportInclude, ReviewQueueArtifact, ReviewQueueRequest,
            build_link_review_queue_artifact, build_review_queue_artifact,
        },
        review_export::{
            NativeReviewDecisionAction as NativeReviewExportAction, NativeReviewExportRequest,
            build_native_review_artifact,
        },
        review_import::{CANON_ENTITY_NATIVE_REVIEW_IMPORT_VERSION, NativeReviewImportReceipt},
        run::{
            EntityRunArtifact,
            link::{
                EntityLinkArtifact, EntityLinkObservationSurfaceBinding, EntityLinkRole,
                read_derivation_validated_entity_link_observation_surface_bindings_at_path,
                validate_entity_link_artifact_at_path, validate_entity_link_artifact_raw_shape,
            },
        },
        schema::{validate_artifact_v1_core_contract, validate_entity_v1_self_hash},
        solve::{SolveArtifact, SolveReconciliationState, validate_solve_artifact_contract},
        telemetry::{CandidateRecallGoldPair, CandidateRecallStratum, EntityCandidateRecallReport},
    },
    fs_safety::{PlannedAccess, resolve_workspace_path},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

pub const CANON_ALIAS_WITHHOLDING_VERSION: &str = "canon.evaluation.alias_withholding.v1";
pub const CANON_ALIAS_WITHHOLDING_NATIVE_EVIDENCE_VERSION: &str =
    "canon.evaluation.alias_withholding.native_engine_evidence.v0";
pub const CANON_ALIAS_WITHHOLDING_EXECUTION_MANIFEST_VERSION: &str =
    "canon.evaluation.alias_withholding.execution_manifest.v0";
pub const CANON_ALIAS_WITHHOLDING_ASSIGNMENT_FIREWALL_VERSION: &str =
    "canon.evaluation.alias_withholding.assignment_firewall.v0";
pub const CANON_ALIAS_WITHHOLDING_LEAKAGE_SCAN_VERSION: &str =
    "canon.evaluation.alias_withholding.leakage_scan.v0";
pub const CANON_ENTITY_CANDIDATE_RECALL_VERSION: &str = "canon_entity_candidate_recall.v0";
pub const CANON_ENTITY_LINK_VERSION: &str = "canon_entity_link.v0";
pub const CANON_ENTITY_LINK_DECISIONS_VERSION: &str = "canon_entity_link_decisions.v0";
pub const CANON_ENTITY_RUN_VERSION: &str = "canon_entity_run.v0";
pub const CANON_ENTITY_SOLVE_VERSION: &str = "canon_entity_solve.v0";
pub const CANON_ENTITY_REVIEW_QUEUE_VERSION: &str = "canon_entity_review_queue.v0";
pub const CANON_ENTITY_AUDIT_VERSION: &str = "canon_entity_audit.v0";
pub const CANON_ENTITY_PROMOTE_VERSION: &str = "canon_entity_promote.v0";
pub const CANON_ENTITY_APPLY_VERSION: &str = "canon_entity_apply.v0";
pub const CANON_ENTITY_PROMOTE_VERSION_V1: &str = "canon_entity_promote.v1";
pub const CANON_REGISTRY_ADD_ENTRY_PROMOTION_VERSION: &str = "canon_registry_add_entry.v0";

pub type AliasWithholdingResult<T> = Result<T, AliasWithholdingError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AliasWithholdingErrorCode {
    ArtifactContract,
    MissingReference,
    DuplicateRecord,
    IneligibleAlias,
    ExactLookupLeak,
    SideChannelLeak,
    ReplayMismatch,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasWithholdingError {
    pub code: AliasWithholdingErrorCode,
    pub message: String,
}

impl AliasWithholdingError {
    pub fn new(code: AliasWithholdingErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for AliasWithholdingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for AliasWithholdingError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AliasClass {
    PunctuationCase,
    Abbreviation,
    LegalSuffix,
    WordOrder,
    OcrNoise,
    Dba,
    ReviewedRename,
    EvidenceRich,
    NameOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationPolicy {
    SameEntityAllowed,
    RelatedDistinct,
    PredecessorSuccessor,
    ParentSubsidiary,
    DivisionLabel,
    ExternalEvidenceRequired,
}

impl RelationPolicy {
    pub const fn identity_credit_allowed(self) -> bool {
        matches!(self, Self::SameEntityAllowed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityEngineDecision {
    Attach,
    Abstain,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAction {
    PromoteAlias,
    DeferReview,
    RejectCandidate,
    RecordCannotLink,
    NoAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeakChannel {
    MappingFile,
    SearchIndex,
    Cache,
    NormalizationPatch,
    GeneratedCorpus,
    DisplayNameCopy,
}

impl LeakChannel {
    pub const fn all() -> [Self; 6] {
        [
            Self::MappingFile,
            Self::SearchIndex,
            Self::Cache,
            Self::NormalizationPatch,
            Self::GeneratedCorpus,
            Self::DisplayNameCopy,
        ]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MappingFile => "mapping_file",
            Self::SearchIndex => "search_index",
            Self::Cache => "cache",
            Self::NormalizationPatch => "normalization_patch",
            Self::GeneratedCorpus => "generated_corpus",
            Self::DisplayNameCopy => "display_name_copy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExactMappingKind {
    Alias,
    TrustedIdentifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrialOutcome {
    CorrectAttachment,
    CorrectAbstention,
    CorrectReject,
    UnsupportedGuess,
    CandidateMiss,
    ReplayMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegistryIdentity {
    pub registry_id: String,
    pub registry_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AliasRecord {
    pub alias_id: String,
    pub value: String,
    pub alias_class: AliasClass,
    pub reviewed: bool,
    pub eligible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TrustedIdentifier {
    pub identifier_id: String,
    pub namespace_id: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PermissibleContext {
    pub context_id: String,
    pub context_kind: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct IncumbentEntitySnapshot {
    pub canonical_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<AliasRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_identifiers: Vec<TrustedIdentifier>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissible_context: Vec<PermissibleContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WithheldAlias {
    pub alias_id: String,
    pub observation_id: String,
    pub surface: String,
    pub alias_class: AliasClass,
    pub relation_policy: RelationPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EvidenceLaneReport {
    pub lane_id: String,
    pub support_basis_points: u16,
    pub contradiction_basis_points: u16,
    pub public_evidence_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PromotionReplay {
    pub approved: bool,
    pub promoted_registry_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact_replay_canonical_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CandidateEvaluation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_rank: Option<u32>,
    pub decision: EntityEngineDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_canonical_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_lanes: Vec<EvidenceLaneReport>,
    pub abstention_action: ReviewAction,
    pub review_action: ReviewAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promotion_replay: Option<PromotionReplay>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeakageCheckStatus {
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeLinkDecision {
    Matched,
    Ambiguous,
    Unmatched,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeCandidateRecallDisposition {
    EvaluatedPair,
    PreparedSurfaceCollapse,
    RelationPolicyControl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeSolveState {
    ResolvedExisting,
    PromotableNew,
    Escrow,
    Contradiction,
    Conflict,
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeAuditStatus {
    Passed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativePromotionStatus {
    Approved,
    Rejected,
    NotRun,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LeakageReceipt {
    pub channel: LeakChannel,
    pub status: LeakageCheckStatus,
    pub checked_artifact_hash: String,
    pub checked_count: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checked_source_hashes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain_binding_hashes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NativeCandidateRankEvidence {
    pub gold_pair_id: String,
    pub operator_id: String,
    pub rank: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NativeCandidateMissEvidence {
    pub gold_pair_id: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_rank: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeCandidateRecallEvidence {
    pub report_version: String,
    pub report_hash: String,
    pub block_artifact_hash: String,
    pub disposition: NativeCandidateRecallDisposition,
    pub gold_pair_id: String,
    pub left_observation_id: String,
    pub right_observation_id: String,
    pub cutoffs: Vec<u32>,
    pub total_gold_pairs: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub true_pair_ranks: Vec<NativeCandidateRankEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub misses_at_50: Vec<NativeCandidateMissEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NativeStageArtifactReference {
    pub stage: String,
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeLinkEvidence {
    pub artifact_version: String,
    pub artifact_content_hash: String,
    pub decision_artifact_version: String,
    pub decision_artifact_hash: String,
    pub shared_run_hash: String,
    pub shared_solve_hash: String,
    pub materialized_rows_content_hash: String,
    pub observation_surface_bindings_content_hash: String,
    pub target_observation_id: String,
    pub target_surface_id: String,
    pub asserted_reference_observation_id: String,
    pub asserted_reference_surface_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_observation_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_surface_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_reference_id: Option<String>,
    pub decision: NativeLinkDecision,
    pub target_count: u64,
    pub matched_count: u64,
    pub ambiguous_count: u64,
    pub unmatched_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeRunEvidence {
    pub artifact_version: String,
    pub artifact_content_hash: String,
    pub solve_artifact_hash: String,
    pub stage_artifacts: Vec<NativeStageArtifactReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeSolveEvidence {
    pub artifact_version: String,
    pub artifact_content_hash: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upstream_artifact_hashes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,
    pub target_observation_id: String,
    pub target_surface_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub component_surface_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    pub state: NativeSolveState,
    pub review_group_count: u64,
    pub alias_fact_count: u64,
    pub assignment_fact_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewEvidence {
    pub artifact_version: String,
    pub artifact_content_hash: String,
    pub source_link_hash: String,
    pub source_solve_hash: String,
    pub review_id: String,
    pub state: NativeSolveState,
    pub proposed_action: String,
    pub review_item_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeAuditEvidence {
    pub artifact_version: String,
    pub artifact_content_hash: String,
    pub audited_artifact_hash: String,
    pub status: NativeAuditStatus,
    pub gate_count: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_gate_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativePromotionEvidence {
    pub artifact_version: String,
    pub artifact_content_hash: String,
    pub audit_artifact_hash: String,
    pub sandbox_registry_digest_before: String,
    pub sandbox_registry_digest_after: String,
    pub promoted_alias_count: u64,
    pub status: NativePromotionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeExactReplayEvidence {
    pub apply_artifact_version: String,
    pub apply_artifact_hash: String,
    pub output_content_hash: String,
    pub registry_digest: String,
    pub input_fingerprint: String,
    pub exact_replay_canonical_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeAssignmentFirewallEvidence {
    pub artifact_content_hash: String,
    pub checked_source_count: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checked_source_hashes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain_binding_hashes: Vec<String>,
    pub issuer_identity_alias_count: u64,
    pub assignment_fact_count: u64,
    pub assignment_derived_alias_count: u64,
    pub identity_key_count: u64,
    pub external_crosswalk_identity_key_count: u64,
    pub assignment_facts_used_as_aliases: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignment_fact_hashes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasWithholdingNativeEvidence {
    pub version: String,
    pub artifact_content_hash: String,
    pub trial_id: String,
    pub observation_id: String,
    pub clean_base_registry_digest: String,
    pub clean_registry_tree_hash: String,
    pub exact_absence_proof: ExactAbsenceProof,
    pub candidate_recall_manifest_hash: String,
    pub candidate_records_hash: String,
    pub candidate_diagnostics_hash: String,
    pub leakage: Vec<LeakageReceipt>,
    pub candidate_recall: NativeCandidateRecallEvidence,
    pub link: NativeLinkEvidence,
    pub run: NativeRunEvidence,
    pub solve: NativeSolveEvidence,
    pub review: NativeReviewEvidence,
    pub audit: NativeAuditEvidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promotion: Option<NativePromotionEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact_replay: Option<NativeExactReplayEvidence>,
    pub assignment_firewall: NativeAssignmentFirewallEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeEngineEvidenceReceipt {
    pub evidence_hash: String,
    pub clean_registry_tree_hash: String,
    pub candidate_recall_report_hash: String,
    pub candidate_recall_manifest_hash: String,
    pub candidate_records_hash: String,
    pub candidate_diagnostics_hash: String,
    pub candidate_recall_disposition: NativeCandidateRecallDisposition,
    pub link_artifact_hash: String,
    pub link_materialized_rows_hash: String,
    pub link_observation_surface_bindings_hash: String,
    pub run_artifact_hash: String,
    pub solve_artifact_hash: String,
    pub review_queue_hash: String,
    pub audit_artifact_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promotion_artifact_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply_artifact_hash: Option<String>,
    pub leak_channels_checked: Vec<LeakChannel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leakage_scan_hashes: Vec<String>,
    pub assignment_firewall_artifact_hash: String,
    pub assignment_checked_source_count: u64,
    pub assignment_fact_count: u64,
    pub issuer_identity_alias_count: u64,
    pub assignment_derived_alias_count: u64,
    pub identity_key_count: u64,
    pub external_crosswalk_identity_key_count: u64,
    pub assignment_facts_used_as_aliases: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativePromotionRoute {
    PromoteV1,
    RegistryAddEntry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasWithholdingExecutionManifest {
    pub version: String,
    pub trial_id: String,
    pub observation_id: String,
    pub assertions: AliasWithholdingExecutionAssertions,
    pub candidate_recall: CandidateRecallExecutionPaths,
    pub link_artifact_path: String,
    pub run_artifact_path: String,
    pub solve_artifact_path: String,
    pub review_queue_artifact_path: String,
    pub audit_artifact_path: String,
    pub clean_registry_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promotion: Option<PromotionExecutionPaths>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact_replay: Option<ExactReplayExecutionPaths>,
    pub assignment_firewall_path: String,
    pub leakage: Vec<LeakageExecutionPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasWithholdingExecutionEnvelope {
    pub version: String,
    pub benchmark: AliasWithholdingBenchmark,
    pub manifests: Vec<AliasWithholdingExecutionManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasWithholdingExecutionEnvelopeSummary {
    pub version: String,
    pub benchmark_id: String,
    pub trial_count: usize,
    pub manifest_count: usize,
    pub native_manifest_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasWithholdingExecutionAssertions {
    pub gold_pair_id: String,
    pub reference_observation_id: String,
    pub target_observation_id: String,
    pub incumbent_canonical_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateRecallExecutionPaths {
    pub quality_manifest_path: String,
    pub block_artifact_path: String,
    pub candidates_path: String,
    pub diagnostics_path: String,
    pub exact_bucket_assertions_path: String,
    pub report_path: String,
    pub exact_bucket_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotionExecutionPaths {
    pub route: NativePromotionRoute,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promotion_artifact_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_import_receipt_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_queue_artifact_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_id: Option<String>,
    pub promoted_registry_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactReplayExecutionPaths {
    pub input_path: String,
    pub lookup_column: String,
    pub apply_artifact_path: String,
    pub output_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeakageExecutionPath {
    pub channel: LeakChannel,
    pub artifact_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckedSourceArtifact {
    path: String,
    content_hash: String,
    binding_hash: String,
    byte_count: u64,
    record_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeakageScanArtifact {
    version: String,
    artifact_content_hash: String,
    trial_id: String,
    channel: LeakChannel,
    checked_sources: Vec<CheckedSourceArtifact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AssignmentFirewallSourceKind {
    AssignmentFacts,
    IssuerIdentityAliases,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssignmentFirewallSourceArtifact {
    kind: AssignmentFirewallSourceKind,
    source: CheckedSourceArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssignmentFirewallArtifact {
    version: String,
    artifact_content_hash: String,
    trial_id: String,
    assignment_facts_used_as_aliases: bool,
    assignment_fact_hashes: Vec<String>,
    issuer_identity_alias_count: u64,
    assignment_fact_count: u64,
    assignment_derived_alias_count: u64,
    identity_key_count: u64,
    external_crosswalk_identity_key_count: u64,
    checked_sources: Vec<AssignmentFirewallSourceArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LeakageProbe {
    pub channel: LeakChannel,
    pub locator: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AliasWithholdingTrialSpec {
    pub trial_id: String,
    pub entity: IncumbentEntitySnapshot,
    pub withheld_alias: WithheldAlias,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retained_alias_ids: Vec<String>,
    pub evaluation: CandidateEvaluation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leakage_probes: Vec<LeakageProbe>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasWithholdingBenchmark {
    pub version: String,
    pub benchmark_id: String,
    pub registry: RegistryIdentity,
    pub policy_digest: String,
    pub trials: Vec<AliasWithholdingTrialSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ExactMapping {
    pub input_value: String,
    pub canonical_id: String,
    pub mapping_kind: ExactMappingKind,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseRegistrySnapshot {
    pub snapshot_id: String,
    pub content_digest: String,
    pub registry: RegistryIdentity,
    pub withheld_surface_fingerprint: String,
    pub incumbent_entity_digest: String,
    pub exact_mappings: Vec<ExactMapping>,
    pub permissible_context_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ExactAbsenceProof {
    pub base_registry_digest: String,
    pub lookup_value_fingerprint: String,
    pub checked_mapping_count: usize,
    pub lookup_found: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasWithholdingTrialReport {
    pub trial_id: String,
    pub observation_id: String,
    pub canonical_id: String,
    pub alias_class: AliasClass,
    pub relation_policy: RelationPolicy,
    pub clean_base_registry_digest: String,
    pub exact_absence_proof: ExactAbsenceProof,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_rank: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_recall_disposition: Option<NativeCandidateRecallDisposition>,
    pub decision: EntityEngineDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_canonical_id: Option<String>,
    pub evidence_lanes: Vec<EvidenceLaneReport>,
    pub abstention_action: ReviewAction,
    pub review_action: ReviewAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promotion_replay: Option<PromotionReplay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_engine_evidence: Option<NativeEngineEvidenceReceipt>,
    pub outcome: TrialOutcome,
    pub credited_attachment: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AliasWithholdingStratumSummary {
    pub alias_class: AliasClass,
    pub relation_policy: RelationPolicy,
    pub trial_count: usize,
    pub credited_attachment_count: usize,
    pub abstain_count: usize,
    pub reject_count: usize,
    pub unsupported_guess_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasWithholdingAggregate {
    pub trial_count: usize,
    pub clean_base_snapshot_count: usize,
    pub credited_attachment_count: usize,
    pub abstain_count: usize,
    pub reject_count: usize,
    pub unsupported_guess_count: usize,
    pub strata: Vec<AliasWithholdingStratumSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasWithholdingReport {
    pub version: String,
    pub benchmark_id: String,
    pub registry: RegistryIdentity,
    pub benchmark_digest: String,
    pub report_digest: String,
    pub trials: Vec<AliasWithholdingTrialReport>,
    pub aggregate: AliasWithholdingAggregate,
}

pub fn alias_withholding_schema_version() -> &'static str {
    CANON_ALIAS_WITHHOLDING_VERSION
}

pub fn compile_alias_withholding_benchmark(
    benchmark: AliasWithholdingBenchmark,
) -> AliasWithholdingResult<AliasWithholdingReport> {
    let benchmark = finalize_benchmark(benchmark)?;
    let benchmark_digest = alias_withholding_benchmark_digest(&benchmark)?;

    let mut reports = Vec::with_capacity(benchmark.trials.len());
    for trial in &benchmark.trials {
        reports.push(compile_trial(&benchmark.registry, trial)?);
    }
    reports.sort_by(|left, right| left.trial_id.cmp(&right.trial_id));

    let aggregate = aggregate_reports(&reports);
    let mut report = AliasWithholdingReport {
        version: CANON_ALIAS_WITHHOLDING_VERSION.to_string(),
        benchmark_id: benchmark.benchmark_id,
        registry: benchmark.registry,
        benchmark_digest,
        report_digest: String::new(),
        trials: reports,
        aggregate,
    };
    report.report_digest = alias_withholding_report_digest(&report)?;
    Ok(report)
}

pub fn compile_alias_withholding_benchmark_from_execution_manifest(
    benchmark: AliasWithholdingBenchmark,
    base_dir: &Path,
    manifests: Vec<AliasWithholdingExecutionManifest>,
) -> AliasWithholdingResult<AliasWithholdingReport> {
    let benchmark = finalize_benchmark(benchmark)?;
    let benchmark_digest = alias_withholding_benchmark_digest(&benchmark)?;
    let mut manifest_by_trial = BTreeMap::new();
    for manifest in manifests {
        let manifest = canonicalize_execution_manifest(manifest)?;
        let trial_id = manifest.trial_id.clone();
        if manifest_by_trial
            .insert(trial_id.clone(), manifest)
            .is_some()
        {
            return Err(error(
                AliasWithholdingErrorCode::DuplicateRecord,
                format!("duplicate native execution manifest for trial {trial_id}"),
            ));
        }
    }

    let mut reports = Vec::with_capacity(benchmark.trials.len());
    for trial in &benchmark.trials {
        let manifest = manifest_by_trial.remove(&trial.trial_id).ok_or_else(|| {
            error(
                AliasWithholdingErrorCode::MissingReference,
                format!(
                    "native execution manifest is missing for alias-withholding trial {}",
                    trial.trial_id
                ),
            )
        })?;
        reports.push(compile_trial_from_execution_manifest(
            &benchmark.registry,
            &benchmark.policy_digest,
            trial,
            base_dir,
            &manifest,
        )?);
    }
    if let Some(extra) = manifest_by_trial.keys().next() {
        return Err(error(
            AliasWithholdingErrorCode::MissingReference,
            format!("native execution manifest references unknown trial {extra}"),
        ));
    }
    reports.sort_by(|left, right| left.trial_id.cmp(&right.trial_id));

    let aggregate = aggregate_reports(&reports);
    let mut report = AliasWithholdingReport {
        version: CANON_ALIAS_WITHHOLDING_VERSION.to_string(),
        benchmark_id: benchmark.benchmark_id,
        registry: benchmark.registry,
        benchmark_digest,
        report_digest: String::new(),
        trials: reports,
        aggregate,
    };
    report.report_digest = alias_withholding_report_digest(&report)?;
    Ok(report)
}

pub fn compile_alias_withholding_execution_envelope(
    envelope: AliasWithholdingExecutionEnvelope,
    base_dir: &Path,
) -> AliasWithholdingResult<AliasWithholdingReport> {
    if envelope.version != CANON_ALIAS_WITHHOLDING_EXECUTION_MANIFEST_VERSION {
        return Err(native_contract_error(
            "alias-withholding execution envelope has the wrong version",
        ));
    }
    compile_alias_withholding_benchmark_from_execution_manifest(
        envelope.benchmark,
        base_dir,
        envelope.manifests,
    )
}

pub fn summarize_alias_withholding_execution_envelope(
    envelope: &AliasWithholdingExecutionEnvelope,
) -> AliasWithholdingResult<AliasWithholdingExecutionEnvelopeSummary> {
    Ok(AliasWithholdingExecutionEnvelopeSummary {
        version: CANON_ALIAS_WITHHOLDING_EXECUTION_MANIFEST_VERSION.to_string(),
        benchmark_id: normalize_non_empty(envelope.benchmark.benchmark_id.clone(), "benchmark_id")?,
        trial_count: envelope.benchmark.trials.len(),
        manifest_count: envelope.manifests.len(),
        native_manifest_count: envelope
            .manifests
            .iter()
            .filter(|manifest| {
                manifest.version == CANON_ALIAS_WITHHOLDING_EXECUTION_MANIFEST_VERSION
            })
            .count(),
    })
}

pub fn render_alias_withholding_execution_envelope_summary(
    summary: &AliasWithholdingExecutionEnvelopeSummary,
) -> String {
    format!(
        "{} trials={} manifests={} native_manifests={}",
        summary.benchmark_id,
        summary.trial_count,
        summary.manifest_count,
        summary.native_manifest_count
    )
}

pub fn compile_trial_from_execution_manifest(
    registry: &RegistryIdentity,
    policy_digest: &str,
    trial: &AliasWithholdingTrialSpec,
    base_dir: &Path,
    manifest: &AliasWithholdingExecutionManifest,
) -> AliasWithholdingResult<AliasWithholdingTrialReport> {
    let trial = canonicalize_trial(trial.clone())?;
    let withheld_alias = eligible_withheld_alias(&trial)?;
    let clean_base = build_clean_base_registry_snapshot(registry, &trial)?;
    let absence = prove_exact_absence(&clean_base, &withheld_alias.value)?;
    if absence.lookup_found {
        return Err(error(
            AliasWithholdingErrorCode::ExactLookupLeak,
            format!(
                "withheld alias leaked into exact lookup for {}",
                trial.trial_id
            ),
        ));
    }
    refuse_side_channel_leaks(&trial, &withheld_alias.value)?;

    let evidence = load_native_engine_evidence(
        &trial,
        &clean_base,
        &absence,
        policy_digest,
        base_dir,
        manifest,
    )?;
    let evidence = validate_native_engine_evidence(&trial, &clean_base, &absence, &evidence)?;
    let derived = derive_native_candidate_evaluation(&trial, &evidence)?;
    let outcome = outcome_for_native_evaluation(&trial, &withheld_alias, &derived, &evidence)?;
    let credited_attachment = outcome == TrialOutcome::CorrectAttachment
        && trial
            .withheld_alias
            .relation_policy
            .identity_credit_allowed();
    let receipt = native_engine_evidence_receipt(&evidence);
    let clean_base_registry_digest = evidence.clean_base_registry_digest.clone();
    let exact_absence_proof = evidence.exact_absence_proof.clone();

    Ok(AliasWithholdingTrialReport {
        trial_id: trial.trial_id,
        observation_id: trial.withheld_alias.observation_id,
        canonical_id: trial.entity.canonical_id,
        alias_class: trial.withheld_alias.alias_class,
        relation_policy: trial.withheld_alias.relation_policy,
        clean_base_registry_digest,
        exact_absence_proof,
        candidate_rank: derived.candidate_rank,
        candidate_recall_disposition: Some(evidence.candidate_recall.disposition),
        decision: derived.decision,
        candidate_canonical_id: derived.candidate_canonical_id,
        evidence_lanes: derived.evidence_lanes,
        abstention_action: derived.abstention_action,
        review_action: derived.review_action,
        promotion_replay: derived.promotion_replay,
        native_engine_evidence: Some(receipt),
        outcome,
        credited_attachment,
    })
}

pub fn compile_trial(
    registry: &RegistryIdentity,
    trial: &AliasWithholdingTrialSpec,
) -> AliasWithholdingResult<AliasWithholdingTrialReport> {
    let trial = canonicalize_trial(trial.clone())?;
    let withheld_alias = eligible_withheld_alias(&trial)?;
    let clean_base = build_clean_base_registry_snapshot(registry, &trial)?;
    let absence = prove_exact_absence(&clean_base, &withheld_alias.value)?;
    if absence.lookup_found {
        return Err(error(
            AliasWithholdingErrorCode::ExactLookupLeak,
            format!(
                "withheld alias leaked into exact lookup for {}",
                trial.trial_id
            ),
        ));
    }
    refuse_side_channel_leaks(&trial, &withheld_alias.value)?;

    let outcome = outcome_for_trial(&trial, &withheld_alias)?;
    let credited_attachment = outcome == TrialOutcome::CorrectAttachment
        && trial
            .withheld_alias
            .relation_policy
            .identity_credit_allowed();

    Ok(AliasWithholdingTrialReport {
        trial_id: trial.trial_id,
        observation_id: trial.withheld_alias.observation_id,
        canonical_id: trial.entity.canonical_id,
        alias_class: trial.withheld_alias.alias_class,
        relation_policy: trial.withheld_alias.relation_policy,
        clean_base_registry_digest: clean_base.content_digest.clone(),
        exact_absence_proof: absence,
        candidate_rank: trial.evaluation.candidate_rank,
        candidate_recall_disposition: None,
        decision: trial.evaluation.decision,
        candidate_canonical_id: trial.evaluation.candidate_canonical_id,
        evidence_lanes: trial.evaluation.evidence_lanes,
        abstention_action: trial.evaluation.abstention_action,
        review_action: trial.evaluation.review_action,
        promotion_replay: trial.evaluation.promotion_replay,
        native_engine_evidence: None,
        outcome,
        credited_attachment,
    })
}

pub fn build_clean_base_registry_snapshot(
    registry: &RegistryIdentity,
    trial: &AliasWithholdingTrialSpec,
) -> AliasWithholdingResult<BaseRegistrySnapshot> {
    let trial = canonicalize_trial(trial.clone())?;
    let withheld_alias = eligible_withheld_alias(&trial)?;
    refuse_source_copy_leaks(&trial, &withheld_alias.value)?;

    if trial
        .retained_alias_ids
        .iter()
        .any(|alias_id| alias_id == &trial.withheld_alias.alias_id)
    {
        return Err(error(
            AliasWithholdingErrorCode::ExactLookupLeak,
            format!(
                "retained aliases for {} include withheld alias {}",
                trial.trial_id, trial.withheld_alias.alias_id
            ),
        ));
    }

    let retained_alias_ids = trial
        .retained_alias_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let aliases_by_id = trial
        .entity
        .aliases
        .iter()
        .map(|alias| (alias.alias_id.clone(), alias.clone()))
        .collect::<BTreeMap<_, _>>();
    for alias_id in &retained_alias_ids {
        if !aliases_by_id.contains_key(alias_id) {
            return Err(error(
                AliasWithholdingErrorCode::MissingReference,
                format!(
                    "retained alias {alias_id} is not present in {}",
                    trial.trial_id
                ),
            ));
        }
    }

    let mut exact_mappings = Vec::new();
    for alias_id in retained_alias_ids {
        let alias = aliases_by_id
            .get(&alias_id)
            .expect("retained alias existence checked");
        exact_mappings.push(ExactMapping {
            input_value: alias.value.clone(),
            canonical_id: trial.entity.canonical_id.clone(),
            mapping_kind: ExactMappingKind::Alias,
            source_ref: alias.alias_id.clone(),
        });
    }
    for identifier in &trial.entity.trusted_identifiers {
        exact_mappings.push(ExactMapping {
            input_value: identifier.value.clone(),
            canonical_id: trial.entity.canonical_id.clone(),
            mapping_kind: ExactMappingKind::TrustedIdentifier,
            source_ref: identifier.identifier_id.clone(),
        });
    }
    exact_mappings.sort();
    exact_mappings.dedup();

    let permissible_context_bytes =
        serde_json::to_vec(&trial.entity.permissible_context).map_err(artifact_error)?;
    let mut snapshot = BaseRegistrySnapshot {
        snapshot_id: format!("snapshot:{}", trial.trial_id),
        content_digest: String::new(),
        registry: registry.clone(),
        withheld_surface_fingerprint: hash_bytes(withheld_alias.value.as_bytes()),
        incumbent_entity_digest: hash_serialized(&trial.entity)?,
        exact_mappings,
        permissible_context_digest: hash_bytes(&permissible_context_bytes),
    };
    snapshot.content_digest = base_registry_snapshot_digest(&snapshot)?;
    Ok(snapshot)
}

pub fn prove_exact_absence(
    snapshot: &BaseRegistrySnapshot,
    withheld_surface: &str,
) -> AliasWithholdingResult<ExactAbsenceProof> {
    let found = exact_lookup(snapshot, withheld_surface).is_some();
    Ok(ExactAbsenceProof {
        base_registry_digest: snapshot.content_digest.clone(),
        lookup_value_fingerprint: hash_bytes(ascii_trim(withheld_surface).as_bytes()),
        checked_mapping_count: snapshot.exact_mappings.len(),
        lookup_found: found,
    })
}

pub fn exact_lookup<'a>(
    snapshot: &'a BaseRegistrySnapshot,
    input: &str,
) -> Option<&'a ExactMapping> {
    let trimmed = ascii_trim(input);
    snapshot
        .exact_mappings
        .iter()
        .find(|mapping| ascii_trim(&mapping.input_value) == trimmed)
}

fn load_native_engine_evidence(
    trial: &AliasWithholdingTrialSpec,
    clean_base_model: &BaseRegistrySnapshot,
    _synthetic_absence: &ExactAbsenceProof,
    policy_digest: &str,
    base_dir: &Path,
    manifest: &AliasWithholdingExecutionManifest,
) -> AliasWithholdingResult<AliasWithholdingNativeEvidence> {
    let manifest = canonicalize_execution_manifest(manifest.clone())?;
    validate_manifest_trial_binding(trial, &manifest)?;

    let clean_registry_dir =
        manifest_directory(base_dir, "clean_registry_dir", &manifest.clean_registry_dir)?;
    let clean_scan = scan_registry_tree(&clean_registry_dir, &trial.withheld_alias.surface)?;
    if clean_scan.mapping.is_some() {
        return Err(error(
            AliasWithholdingErrorCode::ExactLookupLeak,
            "withheld alias is present in the real clean registry",
        ));
    }
    validate_clean_registry_scan(clean_base_model, &clean_scan)?;
    let exact_absence_proof = ExactAbsenceProof {
        base_registry_digest: clean_scan.tree_hash.clone(),
        lookup_value_fingerprint: hash_bytes(ascii_trim(&trial.withheld_alias.surface).as_bytes()),
        checked_mapping_count: clean_scan.checked_mapping_count,
        lookup_found: false,
    };

    let link = load_link_evidence(base_dir, trial, &manifest)?;
    let recall = load_candidate_recall_evidence(base_dir, trial, &manifest, &link)?;
    let run = load_run_evidence(base_dir, &manifest, &link, &recall)?;
    let solve = load_solve_evidence(base_dir, trial, &manifest, &link, &run)?;
    let review = load_review_evidence(base_dir, &manifest, &link, &solve)?;
    let audit = load_audit_evidence(base_dir, &manifest, &link, &run, &solve, &review)?;
    let chain_bindings = NativeChainBindings::from_validated_chain(
        &clean_scan,
        &recall,
        &link,
        &run,
        &solve,
        &review,
    )?;
    let assignment_firewall =
        load_assignment_firewall(base_dir, trial, &manifest, &chain_bindings)?;
    let leakage = load_leakage_receipts(base_dir, trial, &manifest, &chain_bindings, &clean_scan)?;
    let (promotion, exact_replay) =
        load_promotion_and_replay_evidence(NativePromotionReplayContext {
            base_dir,
            trial,
            manifest: &manifest,
            link: &link,
            run: &run,
            audit: &audit,
            clean_scan: &clean_scan,
            policy_digest,
        })?;

    let mut evidence = AliasWithholdingNativeEvidence {
        version: CANON_ALIAS_WITHHOLDING_NATIVE_EVIDENCE_VERSION.to_string(),
        artifact_content_hash: String::new(),
        trial_id: trial.trial_id.clone(),
        observation_id: trial.withheld_alias.observation_id.clone(),
        clean_base_registry_digest: clean_scan.tree_hash.clone(),
        clean_registry_tree_hash: clean_scan.tree_hash,
        exact_absence_proof,
        candidate_recall_manifest_hash: recall.manifest_hash,
        candidate_records_hash: recall.candidate_records_hash,
        candidate_diagnostics_hash: recall.candidate_diagnostics_hash,
        leakage,
        candidate_recall: recall.evidence,
        link,
        run,
        solve,
        review,
        audit,
        promotion,
        exact_replay,
        assignment_firewall,
    };
    evidence.artifact_content_hash = native_engine_evidence_digest(&evidence)?;
    Ok(evidence)
}

#[derive(Debug, Clone)]
struct CandidateRecallLoad {
    evidence: NativeCandidateRecallEvidence,
    manifest_hash: String,
    candidate_records_hash: String,
    candidate_diagnostics_hash: String,
    exact_bucket_assertions_hash: String,
    block_artifact_hash: String,
}

#[derive(Debug, Clone)]
struct NativeChainBindings {
    leakage: BTreeMap<LeakChannel, BTreeSet<String>>,
    assignment_facts: BTreeSet<String>,
    issuer_identity_aliases: BTreeSet<String>,
}

impl NativeChainBindings {
    fn from_validated_chain(
        clean_scan: &RegistryTreeScan,
        recall: &CandidateRecallLoad,
        link: &NativeLinkEvidence,
        run: &NativeRunEvidence,
        solve: &NativeSolveEvidence,
        review: &NativeReviewEvidence,
    ) -> AliasWithholdingResult<Self> {
        let prepare_hash = required_run_stage_hash(run, "prepare")?;
        let index_hash = required_run_stage_hash(run, "index")?;
        let mut leakage = BTreeMap::new();
        leakage.insert(
            LeakChannel::MappingFile,
            BTreeSet::from([clean_scan.tree_hash.clone()]),
        );
        leakage.insert(
            LeakChannel::SearchIndex,
            BTreeSet::from([
                index_hash.clone(),
                recall.block_artifact_hash.clone(),
                recall.candidate_records_hash.clone(),
                recall.candidate_diagnostics_hash.clone(),
                recall.exact_bucket_assertions_hash.clone(),
            ]),
        );
        leakage.insert(
            LeakChannel::Cache,
            BTreeSet::from([index_hash, run.artifact_content_hash.clone()]),
        );
        leakage.insert(
            LeakChannel::NormalizationPatch,
            BTreeSet::from([prepare_hash.clone(), recall.manifest_hash.clone()]),
        );
        leakage.insert(
            LeakChannel::GeneratedCorpus,
            BTreeSet::from([
                prepare_hash.clone(),
                recall.candidate_records_hash.clone(),
                link.materialized_rows_content_hash.clone(),
                link.observation_surface_bindings_content_hash.clone(),
            ]),
        );
        leakage.insert(
            LeakChannel::DisplayNameCopy,
            BTreeSet::from([
                clean_scan.tree_hash.clone(),
                link.materialized_rows_content_hash.clone(),
                link.observation_surface_bindings_content_hash.clone(),
                review.artifact_content_hash.clone(),
            ]),
        );
        let assignment_facts = BTreeSet::from([
            prepare_hash,
            link.materialized_rows_content_hash.clone(),
            link.observation_surface_bindings_content_hash.clone(),
            run.artifact_content_hash.clone(),
            solve.artifact_content_hash.clone(),
        ]);
        let issuer_identity_aliases = BTreeSet::from([
            clean_scan.tree_hash.clone(),
            solve.artifact_content_hash.clone(),
            review.artifact_content_hash.clone(),
        ]);
        for hash in leakage
            .values()
            .flat_map(|hashes| hashes.iter())
            .chain(assignment_facts.iter())
            .chain(issuer_identity_aliases.iter())
        {
            require_digest(hash, "native_chain_binding")?;
        }
        Ok(Self {
            leakage,
            assignment_facts,
            issuer_identity_aliases,
        })
    }

    fn leakage_hashes(&self, channel: LeakChannel) -> AliasWithholdingResult<&BTreeSet<String>> {
        self.leakage.get(&channel).ok_or_else(|| {
            native_contract_error(format!(
                "{} has no validated native-chain binding set",
                channel.as_str()
            ))
        })
    }

    fn assignment_hashes(&self, kind: AssignmentFirewallSourceKind) -> &BTreeSet<String> {
        match kind {
            AssignmentFirewallSourceKind::AssignmentFacts => &self.assignment_facts,
            AssignmentFirewallSourceKind::IssuerIdentityAliases => &self.issuer_identity_aliases,
        }
    }
}

fn required_run_stage_hash(
    run: &NativeRunEvidence,
    required_stage: &str,
) -> AliasWithholdingResult<String> {
    let hashes = run
        .stage_artifacts
        .iter()
        .filter(|stage| stage.stage == required_stage)
        .map(|stage| stage.content_hash.clone())
        .collect::<Vec<_>>();
    if hashes.len() != 1 {
        return Err(native_contract_error(format!(
            "run artifact must contain exactly one {required_stage} stage"
        )));
    }
    require_digest(&hashes[0], "run.stage_artifact_hash")?;
    Ok(hashes[0].clone())
}

fn load_candidate_recall_evidence(
    base_dir: &Path,
    trial: &AliasWithholdingTrialSpec,
    manifest: &AliasWithholdingExecutionManifest,
    link: &NativeLinkEvidence,
) -> AliasWithholdingResult<CandidateRecallLoad> {
    let (quality_manifest, quality_manifest_bytes) = read_manifest_json::<CandidateRecallManifest>(
        base_dir,
        "candidate_recall.quality_manifest_path",
        &manifest.candidate_recall.quality_manifest_path,
    )?;
    let (block_artifact, _) = read_manifest_json::<BlockCandidateArtifact>(
        base_dir,
        "candidate_recall.block_artifact_path",
        &manifest.candidate_recall.block_artifact_path,
    )?;
    validate_block_candidate_artifact_contract(&block_artifact)
        .map_err(|refusal| refusal_contract_error("block artifact", refusal))?;

    let (candidate_records, candidate_records_bytes) =
        read_manifest_json_or_jsonl::<BlockCandidateRecord>(
            base_dir,
            "candidate_recall.candidates_path",
            &manifest.candidate_recall.candidates_path,
        )?;
    let (diagnostics, diagnostics_bytes) = read_manifest_json::<BlockCandidateGenerationDiagnostics>(
        base_dir,
        "candidate_recall.diagnostics_path",
        &manifest.candidate_recall.diagnostics_path,
    )?;
    let (exact_buckets, exact_bucket_bytes) = read_manifest_json_or_jsonl::<ExactBucketAssertion>(
        base_dir,
        "candidate_recall.exact_bucket_assertions_path",
        &manifest.candidate_recall.exact_bucket_assertions_path,
    )?;
    if manifest.candidate_recall.exact_bucket_count != exact_buckets.len() as u64 {
        return Err(native_contract_error(
            "candidate-recall exact_bucket_count does not match exact bucket assertions",
        ));
    }
    validate_block_candidate_payload_hashes(
        &block_artifact,
        &candidate_records,
        &diagnostics,
        &exact_buckets,
    )
    .map_err(|refusal| refusal_contract_error("block payloads", refusal))?;

    let disposition = if !trial
        .withheld_alias
        .relation_policy
        .identity_credit_allowed()
    {
        NativeCandidateRecallDisposition::RelationPolicyControl
    } else if link.target_surface_id == link.asserted_reference_surface_id {
        NativeCandidateRecallDisposition::PreparedSurfaceCollapse
    } else {
        NativeCandidateRecallDisposition::EvaluatedPair
    };
    validate_candidate_recall_manifest_case(
        &quality_manifest,
        &manifest.assertions,
        &link.asserted_reference_surface_id,
        &link.target_surface_id,
        disposition,
    )?;
    let (surface_ids, gold_pairs) = candidate_recall_manifest_gold(&quality_manifest)?;
    let recomputed = evaluate_candidate_recall(CandidateRecallEvaluationRequest {
        candidate_records: &candidate_records,
        diagnostics: &diagnostics,
        gold_pairs: &gold_pairs,
        surface_ids: &surface_ids,
        exact_bucket_count: manifest.candidate_recall.exact_bucket_count,
    });
    recomputed
        .validate()
        .map_err(|error| native_contract_error(error.to_string()))?;
    let (loaded_report, loaded_report_bytes) = read_manifest_json::<EntityCandidateRecallReport>(
        base_dir,
        "candidate_recall.report_path",
        &manifest.candidate_recall.report_path,
    )?;
    loaded_report
        .validate()
        .map_err(|error| native_contract_error(error.to_string()))?;
    if loaded_report != recomputed {
        return Err(native_contract_error(
            "candidate-recall report does not match recomputed candidate inputs",
        ));
    }

    let assertions = &manifest.assertions;
    let true_pair_ranks = loaded_report
        .true_pair_ranks
        .iter()
        .filter(|rank| rank.gold_pair_id == assertions.gold_pair_id)
        .map(|rank| NativeCandidateRankEvidence {
            gold_pair_id: rank.gold_pair_id.clone(),
            operator_id: rank.operator_id.clone(),
            rank: rank.rank as u32,
        })
        .collect::<Vec<_>>();
    let misses_at_50 = loaded_report
        .misses_at_50
        .iter()
        .filter(|miss| miss.gold_pair_id == assertions.gold_pair_id)
        .map(|miss| NativeCandidateMissEvidence {
            gold_pair_id: miss.gold_pair_id.clone(),
            reason: format!("{:?}", miss.reason).to_ascii_lowercase(),
            best_rank: miss.best_rank.map(|rank| rank as u32),
        })
        .collect::<Vec<_>>();

    Ok(CandidateRecallLoad {
        evidence: NativeCandidateRecallEvidence {
            report_version: loaded_report.version,
            report_hash: hash_bytes(&loaded_report_bytes),
            block_artifact_hash: block_artifact.artifact_content_hash.clone(),
            disposition,
            gold_pair_id: assertions.gold_pair_id.clone(),
            left_observation_id: link.asserted_reference_surface_id.clone(),
            right_observation_id: link.target_surface_id.clone(),
            cutoffs: loaded_report
                .cutoffs
                .iter()
                .map(|cutoff| *cutoff as u32)
                .collect(),
            total_gold_pairs: loaded_report.total_gold_pairs,
            true_pair_ranks,
            misses_at_50,
        },
        manifest_hash: hash_bytes(&quality_manifest_bytes),
        candidate_records_hash: hash_bytes(&candidate_records_bytes),
        candidate_diagnostics_hash: hash_bytes(&diagnostics_bytes),
        exact_bucket_assertions_hash: hash_bytes(&exact_bucket_bytes),
        block_artifact_hash: block_artifact.artifact_content_hash,
    })
}

fn load_link_evidence(
    base_dir: &Path,
    _trial: &AliasWithholdingTrialSpec,
    manifest: &AliasWithholdingExecutionManifest,
) -> AliasWithholdingResult<NativeLinkEvidence> {
    let link_path =
        manifest_file_path(base_dir, "link_artifact_path", &manifest.link_artifact_path)?;
    let link_bytes = fs::read(&link_path).map_err(|error| io_error(&link_path, error))?;
    let raw: Value = serde_json::from_slice(&link_bytes).map_err(artifact_error)?;
    validate_entity_link_artifact_raw_shape(&raw)
        .map_err(|refusal| refusal_contract_error("link raw shape", refusal))?;
    let link: EntityLinkArtifact = serde_json::from_value(raw).map_err(artifact_error)?;
    validate_entity_link_artifact_at_path(&link, &link_path)
        .map_err(|refusal| refusal_contract_error("link artifact", refusal))?;
    let (link_run, _) = read_manifest_json::<EntityRunArtifact>(
        base_dir,
        "run_artifact_path",
        &manifest.run_artifact_path,
    )?;
    let bindings = read_derivation_validated_entity_link_observation_surface_bindings_at_path(
        &link, &link_path, &link_run,
    )
    .map_err(|refusal| refusal_contract_error("link observation/surface derivation", refusal))?;

    let target_id = manifest.assertions.target_observation_id.as_str();
    let matches = link
        .decision_artifact
        .matches
        .iter()
        .filter(|record| record.target_id == target_id)
        .collect::<Vec<_>>();
    let ambiguous = link
        .decision_artifact
        .ambiguous
        .iter()
        .filter(|record| record.target_id == target_id)
        .collect::<Vec<_>>();
    let unmatched = link
        .decision_artifact
        .unmatched
        .iter()
        .filter(|record| record.target_id == target_id)
        .collect::<Vec<_>>();
    let bucket_count = matches.len() + ambiguous.len() + unmatched.len();
    if bucket_count != 1 {
        return Err(native_contract_error(
            "link artifact must classify the withheld target exactly once",
        ));
    }

    let (decision, reference_observation_ids, matched_reference_id) =
        if let Some(record) = matches.first() {
            (
                NativeLinkDecision::Matched,
                vec![record.reference_id.clone()],
                Some(record.reference_id.clone()),
            )
        } else if let Some(record) = ambiguous.first() {
            (
                NativeLinkDecision::Ambiguous,
                record
                    .candidates
                    .iter()
                    .map(|candidate| candidate.reference_id.clone())
                    .collect::<Vec<_>>(),
                None,
            )
        } else {
            let reference_ids = unmatched[0]
                .best_candidate
                .as_ref()
                .map(|candidate| vec![candidate.reference_id.clone()])
                .unwrap_or_default();
            (NativeLinkDecision::Unmatched, reference_ids, None)
        };
    if decision == NativeLinkDecision::Matched
        && reference_observation_ids != vec![manifest.assertions.reference_observation_id.clone()]
    {
        return Err(native_contract_error(
            "matched link reference does not match the execution manifest assertion",
        ));
    }
    if manifest.assertions.target_observation_id == manifest.assertions.reference_observation_id {
        return Err(native_contract_error(
            "alias-withholding requires distinct target and reference link observations",
        ));
    }

    let target_binding = required_link_surface_binding(
        &bindings,
        EntityLinkRole::Target,
        &manifest.assertions.target_observation_id,
    )?;
    let asserted_reference_binding = required_link_surface_binding(
        &bindings,
        EntityLinkRole::Reference,
        &manifest.assertions.reference_observation_id,
    )?;
    if target_binding.profile_id != asserted_reference_binding.profile_id {
        return Err(native_contract_error(
            "link target and asserted reference surface bindings use different profiles",
        ));
    }
    let reference_surface_ids = reference_observation_ids
        .iter()
        .map(|reference_id| {
            required_link_surface_binding(&bindings, EntityLinkRole::Reference, reference_id)
                .map(|binding| binding.surface_id.clone())
        })
        .collect::<AliasWithholdingResult<Vec<_>>>()?;

    Ok(NativeLinkEvidence {
        artifact_version: link.version,
        artifact_content_hash: link.artifact_content_hash,
        decision_artifact_version: link.decision_artifact.version,
        decision_artifact_hash: link.decision_artifact.artifact_content_hash,
        shared_run_hash: link.shared_run_artifact.content_hash,
        shared_solve_hash: link.shared_solve_artifact.content_hash,
        materialized_rows_content_hash: link.materialized_rows_content_hash,
        observation_surface_bindings_content_hash: link.observation_surface_bindings_content_hash,
        target_observation_id: target_id.to_string(),
        target_surface_id: target_binding.surface_id.clone(),
        asserted_reference_observation_id: manifest.assertions.reference_observation_id.clone(),
        asserted_reference_surface_id: asserted_reference_binding.surface_id.clone(),
        reference_observation_ids,
        reference_surface_ids,
        matched_reference_id,
        decision,
        target_count: link.decision_artifact.summary.target_records as u64,
        matched_count: link.decision_artifact.matches.len() as u64,
        ambiguous_count: link.decision_artifact.ambiguous.len() as u64,
        unmatched_count: link.decision_artifact.unmatched.len() as u64,
    })
}

fn required_link_surface_binding<'a>(
    bindings: &'a [EntityLinkObservationSurfaceBinding],
    side: EntityLinkRole,
    link_id: &str,
) -> AliasWithholdingResult<&'a EntityLinkObservationSurfaceBinding> {
    let matches = bindings
        .iter()
        .filter(|binding| binding.side == side && binding.link_id == link_id)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [binding] => Ok(*binding),
        [] => Err(native_contract_error(format!(
            "link observation/surface bindings lack the asserted {side:?} link id"
        ))),
        _ => Err(native_contract_error(format!(
            "link observation/surface bindings duplicate the asserted {side:?} link id"
        ))),
    }
}

fn load_run_evidence(
    base_dir: &Path,
    manifest: &AliasWithholdingExecutionManifest,
    link: &NativeLinkEvidence,
    recall: &CandidateRecallLoad,
) -> AliasWithholdingResult<NativeRunEvidence> {
    let (run, _) = read_manifest_json::<EntityRunArtifact>(
        base_dir,
        "run_artifact_path",
        &manifest.run_artifact_path,
    )?;
    validate_run_artifact_self_hash(&run)?;
    if run.artifact_content_hash != link.shared_run_hash {
        return Err(native_contract_error(
            "run artifact hash does not match link shared run hash",
        ));
    }
    let stage_names = run
        .stage_artifacts
        .iter()
        .map(|stage| stage.stage.as_str())
        .collect::<BTreeSet<_>>();
    if stage_names.len() != run.stage_artifacts.len()
        || run.stage_artifacts.iter().any(|stage| {
            stage.stage.trim().is_empty() || !is_blake3_digest(&stage.artifact_content_hash)
        })
    {
        return Err(native_contract_error(
            "run artifact stages must be unique and content-hash bound",
        ));
    }
    if !run.stage_artifacts.iter().any(|stage| {
        stage.stage == "block" && stage.artifact_content_hash == recall.block_artifact_hash
    }) {
        return Err(native_contract_error(
            "run artifact does not bind the validated block candidate artifact",
        ));
    }
    if !run.stage_artifacts.iter().any(|stage| {
        stage.stage == "solve" && stage.artifact_content_hash == link.shared_solve_hash
    }) {
        return Err(native_contract_error(
            "run artifact does not bind the validated solve artifact",
        ));
    }
    Ok(NativeRunEvidence {
        artifact_version: run.version,
        artifact_content_hash: run.artifact_content_hash,
        solve_artifact_hash: link.shared_solve_hash.clone(),
        stage_artifacts: run
            .stage_artifacts
            .into_iter()
            .map(|stage| NativeStageArtifactReference {
                stage: stage.stage,
                version: stage.version,
                content_hash: stage.artifact_content_hash,
            })
            .collect(),
    })
}

fn load_solve_evidence(
    base_dir: &Path,
    trial: &AliasWithholdingTrialSpec,
    manifest: &AliasWithholdingExecutionManifest,
    link: &NativeLinkEvidence,
    run: &NativeRunEvidence,
) -> AliasWithholdingResult<NativeSolveEvidence> {
    let (solve, _) = read_manifest_json::<SolveArtifact>(
        base_dir,
        "solve_artifact_path",
        &manifest.solve_artifact_path,
    )?;
    validate_solve_artifact_contract(&solve)
        .map_err(|refusal| refusal_contract_error("solve artifact", refusal))?;
    if solve.artifact_content_hash != link.shared_solve_hash
        || solve.artifact_content_hash != run.solve_artifact_hash
    {
        return Err(native_contract_error(
            "solve artifact hash does not match run/link references",
        ));
    }
    let mut upstream_artifact_hashes = solve
        .metadata
        .upstream_artifacts
        .iter()
        .map(|artifact| artifact.content_hash.clone())
        .collect::<Vec<_>>();
    upstream_artifact_hashes.sort();
    upstream_artifact_hashes.dedup();
    if upstream_artifact_hashes.is_empty()
        || upstream_artifact_hashes
            .iter()
            .any(|hash| !is_blake3_digest(hash))
    {
        return Err(native_contract_error(
            "solve artifact must bind hashed upstream artifacts",
        ));
    }
    let reference_entities = solve
        .entities
        .iter()
        .filter(|entity| {
            entity
                .surface_ids
                .contains(&link.asserted_reference_surface_id)
        })
        .collect::<Vec<_>>();
    let [reference_entity] = reference_entities.as_slice() else {
        return Err(native_contract_error(
            "audited solve must contain the asserted incumbent reference exactly once",
        ));
    };
    if reference_entity.canonical_id.as_deref() != Some(trial.entity.canonical_id.as_str())
        || reference_entity.state != SolveReconciliationState::ResolvedExisting
    {
        return Err(native_contract_error(
            "asserted reference surface does not resolve to the trial incumbent",
        ));
    }
    let entity = solve.entities.iter().find(|entity| {
        entity
            .surface_ids
            .iter()
            .any(|surface_id| surface_id == &link.target_surface_id)
    });
    let Some(entity) = entity else {
        if link.decision == NativeLinkDecision::Matched {
            return Err(native_contract_error(
                "matched link target is absent from the audited solve",
            ));
        }
        return Ok(NativeSolveEvidence {
            artifact_version: solve.version,
            artifact_content_hash: solve.artifact_content_hash,
            upstream_artifact_hashes,
            component_id: None,
            target_observation_id: manifest.assertions.target_observation_id.clone(),
            target_surface_id: link.target_surface_id.clone(),
            component_surface_ids: Vec::new(),
            canonical_id: None,
            state: NativeSolveState::Absent,
            review_group_count: solve.review_groups.len() as u64,
            alias_fact_count: 0,
            assignment_fact_count: 0,
        });
    };
    let contains_reference = entity.surface_ids.iter().any(|surface_id| {
        surface_id == &link.asserted_reference_surface_id
            || link.reference_surface_ids.contains(surface_id)
    });
    let resolves_to_incumbent =
        entity.canonical_id.as_deref() == Some(trial.entity.canonical_id.as_str());
    match link.decision {
        NativeLinkDecision::Matched => {
            if !contains_reference
                || !resolves_to_incumbent
                || entity.state != SolveReconciliationState::ResolvedExisting
            {
                return Err(native_contract_error(
                    "matched link is inconsistent with the audited solve target component",
                ));
            }
        }
        NativeLinkDecision::Ambiguous
        | NativeLinkDecision::Unmatched
        | NativeLinkDecision::Rejected => {
            if contains_reference || resolves_to_incumbent {
                return Err(native_contract_error(
                    "non-attach link is inconsistent with the audited solve target component",
                ));
            }
        }
    }
    Ok(NativeSolveEvidence {
        artifact_version: solve.version,
        artifact_content_hash: solve.artifact_content_hash,
        upstream_artifact_hashes,
        component_id: Some(entity.component_id.clone()),
        target_observation_id: manifest.assertions.target_observation_id.clone(),
        target_surface_id: link.target_surface_id.clone(),
        component_surface_ids: entity.surface_ids.clone(),
        canonical_id: entity.canonical_id.clone(),
        state: native_solve_state(entity.state),
        review_group_count: solve.review_groups.len() as u64,
        alias_fact_count: 0,
        assignment_fact_count: 0,
    })
}

fn load_review_evidence(
    base_dir: &Path,
    manifest: &AliasWithholdingExecutionManifest,
    link: &NativeLinkEvidence,
    _solve: &NativeSolveEvidence,
) -> AliasWithholdingResult<NativeReviewEvidence> {
    let (link_artifact, _) = read_manifest_json::<EntityLinkArtifact>(
        base_dir,
        "link_artifact_path",
        &manifest.link_artifact_path,
    )?;
    let expected = build_link_review_queue_artifact(LinkReviewQueueRequest {
        link_artifact,
        include: ReviewExportInclude::All,
    })
    .map_err(|refusal| refusal_contract_error("review rebuild", refusal))?;
    let (loaded, _) = read_manifest_json::<ReviewQueueArtifact>(
        base_dir,
        "review_queue_artifact_path",
        &manifest.review_queue_artifact_path,
    )?;
    if loaded != expected {
        return Err(native_contract_error(
            "review queue artifact does not match rebuilt all-review queue",
        ));
    }
    let review_item = if let Some(review_id) = &manifest.assertions.review_id {
        loaded
            .review_items
            .iter()
            .find(|item| &item.review_id == review_id)
            .ok_or_else(|| {
                native_contract_error("review queue does not contain the asserted review item")
            })?
    } else {
        loaded
            .review_items
            .iter()
            .find(|item| {
                item.surface_ids
                    .iter()
                    .any(|surface_id| surface_id == &manifest.assertions.target_observation_id)
            })
            .ok_or_else(|| native_contract_error("review queue does not contain the trial item"))?
    };
    if !review_item
        .surface_ids
        .iter()
        .any(|surface_id| surface_id == &manifest.assertions.target_observation_id)
    {
        return Err(native_contract_error(
            "review queue item does not bind the withheld link target",
        ));
    }
    match link.decision {
        NativeLinkDecision::Matched => {
            if review_item.state != SolveReconciliationState::ResolvedExisting
                || review_item.proposed_action != "audit_directional_match"
            {
                return Err(native_contract_error(
                    "matched link review item does not preserve the resolved decision",
                ));
            }
        }
        NativeLinkDecision::Ambiguous
        | NativeLinkDecision::Unmatched
        | NativeLinkDecision::Rejected => {
            if review_item.state != SolveReconciliationState::Escrow
                || review_item.proposed_action != "review_directional_abstention"
            {
                return Err(native_contract_error(
                    "non-attach link review item is not an escrowed abstention",
                ));
            }
        }
    }
    Ok(NativeReviewEvidence {
        artifact_version: loaded.version,
        artifact_content_hash: loaded.artifact_content_hash,
        source_link_hash: loaded
            .source_link_hash
            .clone()
            .ok_or_else(|| native_contract_error("link review queue must bind source_link_hash"))?,
        source_solve_hash: loaded.source_solve_hash,
        review_id: review_item.review_id.clone(),
        state: native_solve_state(review_item.state),
        proposed_action: review_item.proposed_action.clone(),
        review_item_count: loaded.review_items.len() as u64,
    })
}

fn load_audit_evidence(
    base_dir: &Path,
    manifest: &AliasWithholdingExecutionManifest,
    link: &NativeLinkEvidence,
    run: &NativeRunEvidence,
    solve: &NativeSolveEvidence,
    review: &NativeReviewEvidence,
) -> AliasWithholdingResult<NativeAuditEvidence> {
    let (audit, _) = read_manifest_json::<EntityAuditArtifact>(
        base_dir,
        "audit_artifact_path",
        &manifest.audit_artifact_path,
    )?;
    validate_audit_artifact_self_hash(&audit)?;
    if audit
        .gates
        .iter()
        .any(|gate| gate.status != EntityAuditGateStatus::Passed)
    {
        return Err(native_contract_error(
            "audit artifact contains a failing gate",
        ));
    }
    let audited = audit.audited_artifact.content_hash.clone();
    let certified = audit
        .certified_artifacts
        .iter()
        .map(|artifact| artifact.content_hash.as_str())
        .collect::<BTreeSet<_>>();
    let run_stage_hashes = run
        .stage_artifacts
        .iter()
        .map(|stage| stage.content_hash.as_str())
        .collect::<BTreeSet<_>>();
    let solve_upstreams_are_run_stages = solve
        .upstream_artifact_hashes
        .iter()
        .all(|hash| run_stage_hashes.contains(hash.as_str()));
    let solve_audit_set_certified = certified.contains(solve.artifact_content_hash.as_str())
        && solve
            .upstream_artifact_hashes
            .iter()
            .all(|hash| certified.contains(hash.as_str()));
    let chain_bound = audited == solve.artifact_content_hash
        && link.shared_solve_hash == solve.artifact_content_hash
        && review.source_solve_hash == solve.artifact_content_hash
        && solve_upstreams_are_run_stages
        && solve_audit_set_certified;
    if !chain_bound {
        return Err(native_contract_error(
            "audit artifact does not certify the real solve audit and validated run continuity",
        ));
    }
    Ok(NativeAuditEvidence {
        artifact_version: audit.version,
        artifact_content_hash: audit.artifact_content_hash,
        audited_artifact_hash: audited,
        status: NativeAuditStatus::Passed,
        gate_count: audit.gates.len() as u64,
        required_gate_ids: audit.gates.into_iter().map(|gate| gate.gate_id).collect(),
    })
}

struct NativePromotionReplayContext<'a> {
    base_dir: &'a Path,
    trial: &'a AliasWithholdingTrialSpec,
    manifest: &'a AliasWithholdingExecutionManifest,
    link: &'a NativeLinkEvidence,
    run: &'a NativeRunEvidence,
    audit: &'a NativeAuditEvidence,
    clean_scan: &'a RegistryTreeScan,
    policy_digest: &'a str,
}

fn load_promotion_and_replay_evidence(
    context: NativePromotionReplayContext<'_>,
) -> AliasWithholdingResult<(
    Option<NativePromotionEvidence>,
    Option<NativeExactReplayEvidence>,
)> {
    let NativePromotionReplayContext {
        base_dir,
        trial,
        manifest,
        link,
        run,
        audit,
        clean_scan,
        policy_digest,
    } = context;
    if !trial
        .withheld_alias
        .relation_policy
        .identity_credit_allowed()
    {
        if manifest.promotion.is_some() || manifest.exact_replay.is_some() {
            return Err(native_contract_error(
                "relation-policy controls must not provide promotion or replay evidence",
            ));
        }
        return Ok((None, None));
    }
    match link.decision {
        NativeLinkDecision::Matched => {
            let promotion_manifest = manifest.promotion.as_ref().ok_or_else(|| {
                native_contract_error("matched native path requires promotion evidence")
            })?;
            let replay_manifest = manifest.exact_replay.as_ref().ok_or_else(|| {
                native_contract_error("matched native path requires exact replay evidence")
            })?;
            let promoted_registry_dir = manifest_directory(
                base_dir,
                "promotion.promoted_registry_dir",
                &promotion_manifest.promoted_registry_dir,
            )?;
            let promoted_scan =
                scan_registry_tree(&promoted_registry_dir, &trial.withheld_alias.surface)?;
            let promoted_mapping = promoted_scan.mapping.as_ref().ok_or_else(|| {
                native_contract_error("promoted registry does not map the withheld alias")
            })?;
            if promoted_mapping.canonical_id != trial.entity.canonical_id {
                return Err(native_contract_error(
                    "promoted registry maps the withheld alias to a different canonical id",
                ));
            }
            if clean_scan.tree_hash == promoted_scan.tree_hash {
                return Err(native_contract_error(
                    "promotion did not change the registry tree digest",
                ));
            }
            let promotion_diff = validate_registry_promotion_diff(
                clean_scan,
                &promoted_scan,
                &trial.withheld_alias.surface,
                &trial.entity.canonical_id,
            )?;

            validate_review_import_alias_patch(
                base_dir,
                promotion_manifest,
                trial,
                manifest,
                link,
                run,
                policy_digest,
            )?;
            let (promotion_hash, promoted_alias_count, artifact_version) = load_promotion_artifact(
                base_dir,
                promotion_manifest,
                audit,
                trial,
                clean_scan,
                &promoted_scan,
            )?;
            if promotion_manifest.route == NativePromotionRoute::RegistryAddEntry
                && promoted_alias_count != promotion_diff.added_count
            {
                return Err(native_contract_error(
                    "add-entry receipt count does not corroborate the registry diff",
                ));
            }
            let replay =
                load_exact_replay_evidence(base_dir, replay_manifest, trial, &promoted_scan)?;
            let promotion = NativePromotionEvidence {
                artifact_version,
                artifact_content_hash: promotion_hash,
                audit_artifact_hash: audit.artifact_content_hash.clone(),
                sandbox_registry_digest_before: clean_scan.tree_hash.clone(),
                sandbox_registry_digest_after: promoted_scan.tree_hash,
                promoted_alias_count: promotion_diff.added_count,
                status: NativePromotionStatus::Approved,
            };
            Ok((Some(promotion), Some(replay)))
        }
        NativeLinkDecision::Ambiguous
        | NativeLinkDecision::Unmatched
        | NativeLinkDecision::Rejected => {
            if manifest.promotion.is_some() || manifest.exact_replay.is_some() {
                return Err(native_contract_error(
                    "non-attach native path must not provide promotion or replay evidence",
                ));
            }
            Ok((None, None))
        }
    }
}

fn load_promotion_artifact(
    base_dir: &Path,
    promotion: &PromotionExecutionPaths,
    audit: &NativeAuditEvidence,
    trial: &AliasWithholdingTrialSpec,
    clean_scan: &RegistryTreeScan,
    promoted_scan: &RegistryTreeScan,
) -> AliasWithholdingResult<(String, u64, String)> {
    match promotion.route {
        NativePromotionRoute::PromoteV1 => {
            let path = promotion.promotion_artifact_path.as_ref().ok_or_else(|| {
                native_contract_error("promote-v1 route requires promotion artifact path")
            })?;
            let (artifact, bytes) =
                read_manifest_json::<Value>(base_dir, "promotion.promotion_artifact_path", path)?;
            let contract = validate_artifact_v1_core_contract(&artifact)
                .map_err(|refusal| refusal_contract_error("promote-v1 artifact", refusal))?;
            if contract.artifact_version != CANON_ENTITY_PROMOTE_VERSION_V1 {
                return Err(native_contract_error(
                    "promotion artifact is not canon_entity_promote.v1",
                ));
            }
            validate_entity_v1_self_hash(&artifact)
                .map_err(|refusal| refusal_contract_error("promote-v1 self hash", refusal))?;
            let audit_hash =
                value_string(&artifact, &["audit", "content_hash"]).ok_or_else(|| {
                    native_contract_error("promote-v1 artifact does not bind audit hash")
                })?;
            if audit_hash != audit.artifact_content_hash {
                return Err(native_contract_error(
                    "promote-v1 artifact audit hash does not match validated audit",
                ));
            }
            let alias_count = value_u64(&artifact, &["summary", "counts", "promoted_aliases"])
                .unwrap_or_else(|| value_array_len(&artifact, &["aliases"]).unwrap_or(0));
            Ok((
                hash_bytes(&bytes),
                alias_count,
                CANON_ENTITY_PROMOTE_VERSION_V1.to_string(),
            ))
        }
        NativePromotionRoute::RegistryAddEntry => {
            let path = promotion.promotion_artifact_path.as_ref().ok_or_else(|| {
                native_contract_error("registry add-entry route requires command receipt artifact")
            })?;
            let (receipt, bytes) =
                read_manifest_json::<Value>(base_dir, "promotion.promotion_artifact_path", path)?;
            let version = value_string(&receipt, &["version"])
                .ok_or_else(|| native_contract_error("add-entry receipt lacks version"))?;
            if version != CANON_REGISTRY_ADD_ENTRY_PROMOTION_VERSION {
                return Err(native_contract_error(
                    "registry add-entry receipt has the wrong version",
                ));
            }
            let alias_input = value_string(&receipt, &["alias_entry", "input"])
                .ok_or_else(|| native_contract_error("add-entry receipt lacks alias input"))?;
            let canonical_id = value_string(&receipt, &["alias_entry", "canonical_id"])
                .ok_or_else(|| native_contract_error("add-entry receipt lacks canonical id"))?;
            if ascii_trim(alias_input) != ascii_trim(&trial.withheld_alias.surface)
                || canonical_id != trial.entity.canonical_id
            {
                return Err(native_contract_error(
                    "add-entry receipt does not bind the withheld alias to the incumbent",
                ));
            }
            let before =
                value_u64(&receipt, &["registry", "entry_count_before"]).ok_or_else(|| {
                    native_contract_error("add-entry receipt lacks entry_count_before")
                })?;
            let after =
                value_u64(&receipt, &["registry", "entry_count_after"]).ok_or_else(|| {
                    native_contract_error("add-entry receipt lacks entry_count_after")
                })?;
            if after != before + 1 {
                return Err(native_contract_error(
                    "add-entry receipt entry counts do not show exactly one added entry",
                ));
            }
            if before != clean_scan.checked_mapping_count as u64
                || after != promoted_scan.checked_mapping_count as u64
            {
                return Err(native_contract_error(
                    "add-entry receipt counts do not match the actual clean and promoted registries",
                ));
            }
            let registry_id = value_string(&receipt, &["registry", "id"])
                .ok_or_else(|| native_contract_error("add-entry receipt lacks registry id"))?;
            if registry_id != clean_scan.registry.registry_id
                || registry_id != promoted_scan.registry.registry_id
            {
                return Err(native_contract_error(
                    "add-entry receipt registry id does not match the actual registries",
                ));
            }
            let version_before = value_string(&receipt, &["registry", "version_before"])
                .ok_or_else(|| native_contract_error("add-entry receipt lacks version_before"))?;
            let version_after = value_string(&receipt, &["registry", "version_after"])
                .ok_or_else(|| native_contract_error("add-entry receipt lacks version_after"))?;
            if version_before == version_after {
                return Err(native_contract_error(
                    "add-entry receipt registry version did not change",
                ));
            }
            if version_before != clean_scan.registry.registry_version
                || version_after != promoted_scan.registry.registry_version
            {
                return Err(native_contract_error(
                    "add-entry receipt versions do not match the actual clean and promoted registries",
                ));
            }
            if value_u64(&receipt, &["lint", "errors"]) != Some(0) {
                return Err(native_contract_error(
                    "add-entry receipt must record a zero-error registry lint",
                ));
            }
            let touched = value_string_array_required(&receipt, &["touched_files"])?;
            let registry_touched = touched.iter().any(|path| path == "registry.json");
            let alias_touched = touched.iter().any(|path| {
                path.ends_with(".json") && path != "registry.json" && path != "_build.json"
            });
            if !registry_touched || !alias_touched {
                return Err(native_contract_error(
                    "add-entry receipt touched_files must include registry.json and an alias file",
                ));
            }
            Ok((hash_bytes(&bytes), 1, version.to_string()))
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RegistryPromotionDiff {
    added_count: u64,
}

fn validate_registry_promotion_diff(
    clean_scan: &RegistryTreeScan,
    promoted_scan: &RegistryTreeScan,
    withheld_surface: &str,
    incumbent_canonical_id: &str,
) -> AliasWithholdingResult<RegistryPromotionDiff> {
    if clean_scan.registry.registry_id != promoted_scan.registry.registry_id {
        return Err(native_contract_error(
            "promoted registry id does not match the clean registry id",
        ));
    }
    if clean_scan.registry.registry_version == promoted_scan.registry.registry_version {
        return Err(native_contract_error(
            "promoted registry version did not change",
        ));
    }
    let removed = clean_scan
        .exact_mappings
        .difference(&promoted_scan.exact_mappings)
        .collect::<Vec<_>>();
    if !removed.is_empty() {
        return Err(native_contract_error(
            "promotion removed mappings from the clean registry",
        ));
    }
    let added = promoted_scan
        .exact_mappings
        .difference(&clean_scan.exact_mappings)
        .collect::<Vec<_>>();
    if added.len() != 1 {
        return Err(native_contract_error(
            "promotion must add exactly one exact mapping",
        ));
    }
    let added = added[0];
    if ascii_trim(&added.input_value) != ascii_trim(withheld_surface)
        || added.canonical_id != incumbent_canonical_id
    {
        return Err(native_contract_error(
            "promotion diff does not add the withheld alias to the incumbent",
        ));
    }
    Ok(RegistryPromotionDiff { added_count: 1 })
}

fn validate_review_import_alias_patch(
    base_dir: &Path,
    promotion: &PromotionExecutionPaths,
    trial: &AliasWithholdingTrialSpec,
    manifest: &AliasWithholdingExecutionManifest,
    link: &NativeLinkEvidence,
    run: &NativeRunEvidence,
    policy_digest: &str,
) -> AliasWithholdingResult<()> {
    let path = promotion
        .review_import_receipt_path
        .as_ref()
        .ok_or_else(|| native_contract_error("matched promotion requires review import receipt"))?;
    let (receipt, _) =
        read_manifest_json::<Value>(base_dir, "promotion.review_import_receipt_path", path)?;
    let receipt: NativeReviewImportReceipt =
        serde_json::from_value(receipt).map_err(artifact_error)?;
    if receipt.version != CANON_ENTITY_NATIVE_REVIEW_IMPORT_VERSION {
        return Err(native_contract_error(
            "review import receipt has the wrong native version",
        ));
    }
    let review_queue_path = promotion
        .review_queue_artifact_path
        .as_ref()
        .ok_or_else(|| {
            native_contract_error("matched promotion requires solve review queue artifact")
        })?;
    let promotion_review_id = promotion
        .review_id
        .as_deref()
        .ok_or_else(|| native_contract_error("matched promotion requires promotion review_id"))?;
    let (solve_artifact, _) = read_manifest_json::<SolveArtifact>(
        base_dir,
        "solve_artifact_path",
        &manifest.solve_artifact_path,
    )?;
    let expected_review_queue = build_review_queue_artifact(ReviewQueueRequest {
        solve_artifact,
        include: ReviewExportInclude::All,
        provenance_samples: Vec::new(),
        relation_hints: Vec::new(),
    })
    .map_err(|refusal| refusal_contract_error("promotion review rebuild", refusal))?;
    let (review_queue, _) = read_manifest_json::<ReviewQueueArtifact>(
        base_dir,
        "promotion.review_queue_artifact_path",
        review_queue_path,
    )?;
    if review_queue != expected_review_queue {
        return Err(native_contract_error(
            "promotion review queue does not match rebuilt solve review queue",
        ));
    }
    let native_review = build_native_review_artifact(NativeReviewExportRequest {
        review_queue,
        run_content_hash: run.artifact_content_hash.clone(),
        policy_content_hash: policy_digest.to_string(),
    })
    .map_err(|refusal| refusal_contract_error("native review export", refusal))?;
    if receipt.source_review_artifact_hash != native_review.artifact_content_hash
        || receipt.source_review_queue_hash != native_review.binding.source_review_queue_hash
        || receipt.run_content_hash != native_review.binding.run_content_hash
        || receipt.run_content_hash != run.artifact_content_hash
        || receipt.policy_content_hash != native_review.binding.policy_content_hash
        || receipt.registry_snapshot_hash != native_review.binding.registry_snapshot_hash
        || receipt.profile_id != native_review.binding.profile_id
        || receipt.profile_version != native_review.binding.profile_version
        || receipt.entity_type != native_review.binding.entity_type
        || receipt.identity_semantics != native_review.binding.identity_semantics
        || receipt.strategy_hash != native_review.binding.strategy_hash
    {
        return Err(native_contract_error(
            "review import receipt does not bind the rebuilt native review artifact",
        ));
    }
    let native_review_item = native_review
        .review_items
        .iter()
        .find(|item| item.review_id == promotion_review_id)
        .ok_or_else(|| native_contract_error("rebuilt native review lacks promotion review_id"))?;
    if native_review_item.recommended_action != NativeReviewExportAction::Alias
        || !native_review_item
            .allowed_actions
            .contains(&NativeReviewExportAction::Alias)
    {
        return Err(native_contract_error(
            "reviewed native decision is not promotion-compatible alias evidence",
        ));
    }
    let matching_alias_patches = receipt
        .patches
        .alias_patches
        .iter()
        .filter(|patch| patch.review_id == promotion_review_id)
        .collect::<Vec<_>>();
    if matching_alias_patches.len() != 1 {
        return Err(native_contract_error(
            "review import receipt must contain exactly one alias patch for this review_id",
        ));
    }
    if receipt
        .patches
        .cannot_link_patches
        .iter()
        .any(|patch| patch.review_id == promotion_review_id)
        || receipt
            .patches
            .relation_patches
            .iter()
            .any(|patch| patch.review_id == promotion_review_id)
        || receipt
            .patches
            .assignment_patches
            .iter()
            .any(|patch| patch.review_id == promotion_review_id)
        || receipt
            .patches
            .defer_patches
            .iter()
            .any(|patch| patch.review_id == promotion_review_id)
    {
        return Err(native_contract_error(
            "review import receipt has non-alias decisions for the promoted review_id",
        ));
    }
    let alias_patch = matching_alias_patches[0];
    if alias_patch.canonical_hint != trial.entity.canonical_id
        || alias_patch.decision_binding_hash != native_review_item.decision_binding_hash
        || alias_patch.profile_id != native_review.binding.profile_id
        || alias_patch.identity_semantics != native_review.binding.identity_semantics
        || !alias_patch
            .surface_ids
            .iter()
            .any(|surface_id| surface_id == &link.target_surface_id)
        || !alias_patch
            .surface_ids
            .iter()
            .any(|surface_id| surface_id == &link.asserted_reference_surface_id)
        || alias_patch.operator_id.trim().is_empty()
        || alias_patch.reason_code.trim().is_empty()
    {
        return Err(native_contract_error(
            "review import alias patch does not bind the trial target and incumbent",
        ));
    }
    require_digest(
        &alias_patch.decision_binding_hash,
        "review_import.alias_patch.decision_binding_hash",
    )?;
    let patch_count = receipt.patches.alias_patches.len()
        + receipt.patches.cannot_link_patches.len()
        + receipt.patches.relation_patches.len()
        + receipt.patches.assignment_patches.len()
        + receipt.patches.defer_patches.len();
    if receipt.accepted_decisions != patch_count as u64 || receipt.accepted_decisions == 0 {
        return Err(native_contract_error(
            "review import receipt accepted_decisions does not match its native patches",
        ));
    }
    Ok(())
}

fn load_exact_replay_evidence(
    base_dir: &Path,
    replay: &ExactReplayExecutionPaths,
    trial: &AliasWithholdingTrialSpec,
    promoted_scan: &RegistryTreeScan,
) -> AliasWithholdingResult<NativeExactReplayEvidence> {
    let (_, input_bytes) =
        read_manifest_file(base_dir, "exact_replay.input_path", &replay.input_path)?;
    let (apply, apply_bytes) = read_manifest_json::<ApplyRunArtifact>(
        base_dir,
        "exact_replay.apply_artifact_path",
        &replay.apply_artifact_path,
    )?;
    validate_apply_artifact_self_hash(&apply)?;
    if apply.registry.id != promoted_scan.registry.registry_id
        || apply.registry.version != promoted_scan.registry.registry_version
        || apply.registry_snapshot_hash.as_deref() != Some(promoted_scan.tree_hash.as_str())
    {
        return Err(native_contract_error(
            "exact replay apply artifact does not bind the promoted registry snapshot",
        ));
    }
    require_digest(
        apply.registry_snapshot_hash.as_deref().unwrap_or_default(),
        "exact_replay.apply.registry_snapshot_hash",
    )?;
    require_digest(
        &apply.output_content_hash,
        "exact_replay.apply.output_content_hash",
    )?;
    if hash_bytes(&input_bytes) != apply.streaming.input.content_hash {
        return Err(native_contract_error(
            "exact replay input bytes do not match apply artifact input hash",
        ));
    }
    validate_exact_replay_input(
        &input_bytes,
        &replay.lookup_column,
        &trial.withheld_alias.surface,
    )?;
    if apply.summary.get("rows").copied() != Some(1)
        || apply.summary.get("resolved").copied() != Some(1)
        || apply.summary.get("unresolved").copied() != Some(0)
    {
        return Err(native_contract_error(
            "exact replay apply artifact must record one fully resolved trial row",
        ));
    }
    let (output_path, output_bytes) =
        read_manifest_file(base_dir, "exact_replay.output_path", &replay.output_path)?;
    if !apply.output_path.is_empty()
        && Path::new(&apply.output_path)
            .file_name()
            .is_some_and(|name| output_path.file_name() != Some(name))
    {
        return Err(native_contract_error(
            "apply artifact output path does not match exact replay output",
        ));
    }
    let output_content_hash = hash_bytes(&output_bytes);
    if apply.output_content_hash != output_content_hash {
        return Err(native_contract_error(
            "exact replay output bytes do not match the apply artifact output hash",
        ));
    }
    if !output_maps_withheld_alias(
        &output_bytes,
        &replay.lookup_column,
        &trial.withheld_alias.surface,
        &trial.entity.canonical_id,
    )? {
        return Err(error(
            AliasWithholdingErrorCode::ReplayMismatch,
            "exact replay output does not map the withheld alias to the incumbent canonical id",
        ));
    }
    Ok(NativeExactReplayEvidence {
        apply_artifact_version: apply.version,
        apply_artifact_hash: hash_bytes(&apply_bytes),
        output_content_hash,
        registry_digest: promoted_scan.tree_hash.clone(),
        input_fingerprint: apply.streaming.input.content_hash,
        exact_replay_canonical_id: trial.entity.canonical_id.clone(),
    })
}

fn load_assignment_firewall(
    base_dir: &Path,
    trial: &AliasWithholdingTrialSpec,
    manifest: &AliasWithholdingExecutionManifest,
    chain_bindings: &NativeChainBindings,
) -> AliasWithholdingResult<NativeAssignmentFirewallEvidence> {
    let (value, bytes) = read_manifest_json::<Value>(
        base_dir,
        "assignment_firewall_path",
        &manifest.assignment_firewall_path,
    )?;
    validate_value_contract_version(
        &value,
        CANON_ALIAS_WITHHOLDING_ASSIGNMENT_FIREWALL_VERSION,
        "assignment firewall artifact",
    )?;
    validate_required_value_self_hash(&value, &bytes, "assignment firewall artifact")?;
    let artifact: AssignmentFirewallArtifact =
        serde_json::from_value(value).map_err(artifact_error)?;
    if artifact.trial_id != trial.trial_id {
        return Err(native_contract_error(
            "assignment firewall artifact does not bind the trial",
        ));
    }
    if artifact.checked_sources.is_empty() {
        return Err(native_contract_error(
            "assignment firewall artifact must bind concrete checked source paths",
        ));
    }
    let fingerprint = hash_bytes(ascii_trim(&trial.withheld_alias.surface).as_bytes());
    refuse_checked_bytes_leak(
        &bytes,
        &trial.withheld_alias.surface,
        &fingerprint,
        AliasWithholdingErrorCode::ArtifactContract,
        "assignment firewall artifact",
    )?;
    let mut paths = BTreeSet::new();
    let mut checked_source_hashes = Vec::new();
    let mut chain_binding_hashes = Vec::new();
    let mut assignment_fact_hashes = Vec::new();
    let mut identity_alias_hashes = Vec::new();
    for checked in &artifact.checked_sources {
        let loaded = load_checked_source(
            base_dir,
            "assignment_firewall.checked_sources.path",
            &checked.source,
            chain_bindings.assignment_hashes(checked.kind),
            &trial.withheld_alias.surface,
            &fingerprint,
            AliasWithholdingErrorCode::ArtifactContract,
            "assignment firewall",
        )?;
        if !paths.insert(loaded.path.clone()) {
            return Err(native_contract_error(
                "assignment firewall repeats a checked source path",
            ));
        }
        checked_source_hashes.push(loaded.content_hash);
        chain_binding_hashes.push(loaded.binding_hash);
        match checked.kind {
            AssignmentFirewallSourceKind::AssignmentFacts => {
                assignment_fact_hashes.extend(loaded.record_hashes)
            }
            AssignmentFirewallSourceKind::IssuerIdentityAliases => {
                identity_alias_hashes.extend(loaded.record_hashes)
            }
        }
    }
    assignment_fact_hashes.sort();
    identity_alias_hashes.sort();
    if assignment_fact_hashes.is_empty() || identity_alias_hashes.is_empty() {
        return Err(native_contract_error(
            "assignment firewall must check nonempty assignment and issuer-identity sources",
        ));
    }
    if assignment_fact_hashes
        .windows(2)
        .any(|pair| pair[0] == pair[1])
        || identity_alias_hashes
            .windows(2)
            .any(|pair| pair[0] == pair[1])
    {
        return Err(native_contract_error(
            "assignment firewall checked sources contain duplicate records",
        ));
    }
    if assignment_fact_hashes
        .iter()
        .any(|hash| identity_alias_hashes.binary_search(hash).is_ok())
    {
        return Err(native_contract_error(
            "assignment facts overlap issuer identity alias records",
        ));
    }
    let mut declared_assignment_hashes = artifact.assignment_fact_hashes.clone();
    declared_assignment_hashes.sort();
    if declared_assignment_hashes != assignment_fact_hashes
        || artifact.assignment_fact_count != assignment_fact_hashes.len() as u64
        || artifact.issuer_identity_alias_count != identity_alias_hashes.len() as u64
    {
        return Err(native_contract_error(
            "assignment firewall counts and fact hashes do not match checked source bytes",
        ));
    }
    if artifact.assignment_facts_used_as_aliases
        || artifact.assignment_derived_alias_count != 0
        || artifact.identity_key_count != 0
        || artifact.external_crosswalk_identity_key_count != 0
    {
        return Err(native_contract_error(
            "assignment firewall artifact allows assignment-derived identity leakage",
        ));
    }
    checked_source_hashes.sort();
    chain_binding_hashes.sort();
    chain_binding_hashes.dedup();
    Ok(NativeAssignmentFirewallEvidence {
        artifact_content_hash: artifact.artifact_content_hash,
        checked_source_count: paths.len() as u64,
        checked_source_hashes,
        chain_binding_hashes,
        issuer_identity_alias_count: artifact.issuer_identity_alias_count,
        assignment_fact_count: artifact.assignment_fact_count,
        assignment_derived_alias_count: artifact.assignment_derived_alias_count,
        identity_key_count: artifact.identity_key_count,
        external_crosswalk_identity_key_count: artifact.external_crosswalk_identity_key_count,
        assignment_facts_used_as_aliases: artifact.assignment_facts_used_as_aliases,
        assignment_fact_hashes,
    })
}

fn load_leakage_receipts(
    base_dir: &Path,
    trial: &AliasWithholdingTrialSpec,
    manifest: &AliasWithholdingExecutionManifest,
    chain_bindings: &NativeChainBindings,
    clean_scan: &RegistryTreeScan,
) -> AliasWithholdingResult<Vec<LeakageReceipt>> {
    let mut receipts = Vec::with_capacity(manifest.leakage.len());
    for leakage in &manifest.leakage {
        let (value, bytes) =
            read_manifest_json::<Value>(base_dir, "leakage.artifact_path", &leakage.artifact_path)?;
        validate_value_contract_version(
            &value,
            CANON_ALIAS_WITHHOLDING_LEAKAGE_SCAN_VERSION,
            "leakage scan artifact",
        )?;
        validate_required_value_self_hash(&value, &bytes, "leakage scan artifact")?;
        let artifact: LeakageScanArtifact =
            serde_json::from_value(value).map_err(artifact_error)?;
        if artifact.trial_id != trial.trial_id || artifact.channel != leakage.channel {
            return Err(error(
                AliasWithholdingErrorCode::SideChannelLeak,
                format!(
                    "{} leakage artifact does not bind its trial and channel",
                    leakage.channel.as_str()
                ),
            ));
        }
        if artifact.checked_sources.is_empty() {
            return Err(error(
                AliasWithholdingErrorCode::SideChannelLeak,
                format!(
                    "{} leakage artifact did not bind concrete checked source paths",
                    leakage.channel.as_str()
                ),
            ));
        }
        let fingerprint = hash_bytes(ascii_trim(&trial.withheld_alias.surface).as_bytes());
        refuse_checked_bytes_leak(
            &bytes,
            &trial.withheld_alias.surface,
            &fingerprint,
            AliasWithholdingErrorCode::SideChannelLeak,
            leakage.channel.as_str(),
        )?;
        let allowed = chain_bindings.leakage_hashes(leakage.channel)?;
        let mut paths = BTreeSet::new();
        let mut checked_source_hashes = Vec::new();
        let mut chain_binding_hashes = Vec::new();
        let mut checked_count = 0u64;
        let mut mapping_sources = BTreeMap::new();
        for source in &artifact.checked_sources {
            let loaded = load_checked_source(
                base_dir,
                "leakage.checked_sources.path",
                source,
                allowed,
                &trial.withheld_alias.surface,
                &fingerprint,
                AliasWithholdingErrorCode::SideChannelLeak,
                leakage.channel.as_str(),
            )?;
            if !paths.insert(loaded.path.clone()) {
                return Err(error(
                    AliasWithholdingErrorCode::SideChannelLeak,
                    format!(
                        "{} leakage scan repeats a checked source path",
                        leakage.channel.as_str()
                    ),
                ));
            }
            checked_count = checked_count
                .checked_add(loaded.record_hashes.len() as u64)
                .ok_or_else(|| native_contract_error("leakage checked record count overflow"))?;
            mapping_sources.insert(loaded.path, loaded.content_hash.clone());
            checked_source_hashes.push(loaded.content_hash);
            chain_binding_hashes.push(loaded.binding_hash);
        }
        if leakage.channel == LeakChannel::MappingFile && mapping_sources != clean_scan.file_hashes
        {
            return Err(error(
                AliasWithholdingErrorCode::SideChannelLeak,
                "mapping_file leakage scan must enumerate the complete clean registry tree",
            ));
        }
        checked_source_hashes.sort();
        chain_binding_hashes.sort();
        chain_binding_hashes.dedup();
        receipts.push(LeakageReceipt {
            channel: leakage.channel,
            status: LeakageCheckStatus::Clear,
            checked_artifact_hash: artifact.artifact_content_hash,
            checked_count,
            checked_source_hashes,
            chain_binding_hashes,
        });
    }
    receipts.sort();
    Ok(receipts)
}

#[derive(Debug, Clone)]
struct LoadedCheckedSource {
    path: PathBuf,
    content_hash: String,
    binding_hash: String,
    record_hashes: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
fn load_checked_source(
    base_dir: &Path,
    field: &str,
    source: &CheckedSourceArtifact,
    allowed_bindings: &BTreeSet<String>,
    withheld_surface: &str,
    fingerprint: &str,
    error_code: AliasWithholdingErrorCode,
    label: &str,
) -> AliasWithholdingResult<LoadedCheckedSource> {
    require_digest(&source.content_hash, "checked_source.content_hash")?;
    require_digest(&source.binding_hash, "checked_source.binding_hash")?;
    if !allowed_bindings.contains(&source.binding_hash) {
        return Err(error(
            error_code,
            format!("{label} checked source is not bound to the validated native chain"),
        ));
    }
    let (path, bytes) = read_manifest_file(base_dir, field, &source.path)?;
    if bytes.is_empty() || source.byte_count == 0 || source.byte_count != bytes.len() as u64 {
        return Err(error(
            error_code,
            format!("{label} checked source byte count does not match nonempty source bytes"),
        ));
    }
    let actual_hash = hash_bytes(&bytes);
    if source.content_hash != actual_hash {
        return Err(error(
            error_code,
            format!("{label} checked source content hash is stale"),
        ));
    }
    refuse_checked_bytes_leak(&bytes, withheld_surface, fingerprint, error_code, label)?;
    let record_hashes = source_record_hashes(&bytes)?;
    if record_hashes.is_empty() || source.record_count != record_hashes.len() as u64 {
        return Err(error(
            error_code,
            format!("{label} checked source record count does not match source bytes"),
        ));
    }
    Ok(LoadedCheckedSource {
        path,
        content_hash: actual_hash,
        binding_hash: source.binding_hash.clone(),
        record_hashes,
    })
}

fn refuse_checked_bytes_leak(
    bytes: &[u8],
    withheld_surface: &str,
    fingerprint: &str,
    error_code: AliasWithholdingErrorCode,
    label: &str,
) -> AliasWithholdingResult<()> {
    if bytes_contain(bytes, ascii_trim(withheld_surface).as_bytes())
        || bytes_contain(bytes, fingerprint.as_bytes())
    {
        return Err(error(
            error_code,
            format!("{label} leaked withheld alias bytes or fingerprint"),
        ));
    }
    Ok(())
}

fn source_record_hashes(bytes: &[u8]) -> AliasWithholdingResult<Vec<String>> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Vec::new());
    }
    if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
        let records = match &value {
            Value::Array(records) => records.clone(),
            Value::Object(_) => ["records", "assignment_facts", "aliases"]
                .iter()
                .find_map(|key| value.get(*key).and_then(Value::as_array).cloned())
                .unwrap_or_else(|| vec![value]),
            Value::Null => Vec::new(),
            _ => vec![value],
        };
        return records
            .iter()
            .map(hash_serialized)
            .collect::<AliasWithholdingResult<Vec<_>>>();
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(text
            .lines()
            .map(ascii_trim)
            .filter(|line| !line.is_empty())
            .map(|line| hash_bytes(line.as_bytes()))
            .collect());
    }
    Ok(vec![hash_bytes(bytes)])
}

#[derive(Debug, Clone)]
struct RegistryTreeScan {
    tree_hash: String,
    registry: RegistryIdentity,
    checked_mapping_count: usize,
    exact_mappings: BTreeSet<RegistryExactMapping>,
    file_hashes: BTreeMap<PathBuf, String>,
    mapping: Option<RegistryMapping>,
}

#[derive(Debug, Clone)]
struct RegistryMapping {
    canonical_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RegistryExactMapping {
    input_value: String,
    canonical_id: String,
}

#[derive(Debug, Deserialize)]
struct RegistryMetadataRecord {
    id: String,
    version: String,
}

#[derive(Debug, Deserialize)]
struct RegistryMappingRecord {
    input: String,
    canonical_id: String,
}

fn validate_clean_registry_scan(
    clean_base_model: &BaseRegistrySnapshot,
    clean_scan: &RegistryTreeScan,
) -> AliasWithholdingResult<()> {
    if clean_scan.registry != clean_base_model.registry {
        return Err(native_contract_error(
            "real clean registry id/version does not match the benchmark clean-base model",
        ));
    }
    let expected = clean_base_model
        .exact_mappings
        .iter()
        .map(|mapping| RegistryExactMapping {
            input_value: ascii_trim(&mapping.input_value).to_string(),
            canonical_id: mapping.canonical_id.clone(),
        })
        .collect::<BTreeSet<_>>();
    if clean_scan.exact_mappings != expected
        || clean_scan.checked_mapping_count != clean_scan.exact_mappings.len()
    {
        return Err(native_contract_error(
            "real clean registry exact mapping set does not match the benchmark clean-base model",
        ));
    }
    Ok(())
}

fn scan_registry_tree(
    registry_dir: &Path,
    withheld_surface: &str,
) -> AliasWithholdingResult<RegistryTreeScan> {
    let mut files = Vec::new();
    files.push(registry_dir.join("registry.json"));
    for entry in fs::read_dir(registry_dir).map_err(|error| io_error(registry_dir, error))? {
        let entry = entry.map_err(|error| io_error(registry_dir, error))?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "json")
            && path
                .file_name()
                .is_some_and(|name| name != "registry.json" && name != "_build.json")
        {
            files.push(path);
        }
    }
    files.sort();

    let mut hasher = blake3::Hasher::new();
    let mut checked_mapping_count = 0usize;
    let mut registry = None;
    let mut exact_mappings = BTreeSet::new();
    let mut file_hashes = BTreeMap::new();
    let mut mapping = None;
    for path in files {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| native_contract_error("registry path is not UTF-8"))?;
        let bytes = fs::read(&path).map_err(|error| io_error(&path, error))?;
        file_hashes.insert(path.clone(), hash_bytes(&bytes));
        hasher.update(name.as_bytes());
        hasher.update(&[0]);
        hasher.update(&bytes);
        hasher.update(&[0xff]);
        if name == "registry.json" {
            let metadata =
                serde_json::from_slice::<RegistryMetadataRecord>(&bytes).map_err(artifact_error)?;
            registry = Some(RegistryIdentity {
                registry_id: normalize_non_empty(metadata.id, "registry.id")?,
                registry_version: normalize_non_empty(metadata.version, "registry.version")?,
            });
            continue;
        }
        let records =
            serde_json::from_slice::<Vec<RegistryMappingRecord>>(&bytes).map_err(artifact_error)?;
        for record in records {
            checked_mapping_count += 1;
            exact_mappings.insert(RegistryExactMapping {
                input_value: ascii_trim(&record.input).to_string(),
                canonical_id: record.canonical_id.clone(),
            });
            if ascii_trim(&record.input) == ascii_trim(withheld_surface) {
                if mapping.is_some() {
                    return Err(native_contract_error(
                        "registry contains duplicate exact mappings for withheld alias",
                    ));
                }
                mapping = Some(RegistryMapping {
                    canonical_id: record.canonical_id,
                });
            }
        }
    }
    Ok(RegistryTreeScan {
        tree_hash: format!("blake3:{}", hasher.finalize().to_hex()),
        registry: registry
            .ok_or_else(|| native_contract_error("registry.json metadata was not scanned"))?,
        checked_mapping_count,
        exact_mappings,
        file_hashes,
        mapping,
    })
}

fn manifest_file_path(base_dir: &Path, field: &str, rel: &str) -> AliasWithholdingResult<PathBuf> {
    let resolution = resolve_workspace_path(base_dir, field, Path::new(rel), PlannedAccess::Read)
        .map_err(|error| native_contract_error(error.to_string()))?;
    if !resolution.exists {
        return Err(native_contract_error(format!("{field} does not exist")));
    }
    if resolution.leaf_is_symlink {
        return Err(native_contract_error(format!(
            "{field} must not be a symlink"
        )));
    }
    let metadata = fs::metadata(&resolution.absolute_path)
        .map_err(|error| io_error(&resolution.absolute_path, error))?;
    if !metadata.is_file() {
        return Err(native_contract_error(format!("{field} must be a file")));
    }
    Ok(resolution.absolute_path)
}

fn manifest_directory(base_dir: &Path, field: &str, rel: &str) -> AliasWithholdingResult<PathBuf> {
    let resolution = resolve_workspace_path(base_dir, field, Path::new(rel), PlannedAccess::Read)
        .map_err(|error| native_contract_error(error.to_string()))?;
    if !resolution.exists {
        return Err(native_contract_error(format!("{field} does not exist")));
    }
    if resolution.leaf_is_symlink {
        return Err(native_contract_error(format!(
            "{field} must not be a symlink"
        )));
    }
    let metadata = fs::metadata(&resolution.absolute_path)
        .map_err(|error| io_error(&resolution.absolute_path, error))?;
    if !metadata.is_dir() {
        return Err(native_contract_error(format!(
            "{field} must be a directory"
        )));
    }
    Ok(resolution.absolute_path)
}

fn read_manifest_file(
    base_dir: &Path,
    field: &str,
    rel: &str,
) -> AliasWithholdingResult<(PathBuf, Vec<u8>)> {
    let path = manifest_file_path(base_dir, field, rel)?;
    let bytes = fs::read(&path).map_err(|error| io_error(&path, error))?;
    Ok((path, bytes))
}

fn read_manifest_json<T: DeserializeOwned>(
    base_dir: &Path,
    field: &str,
    rel: &str,
) -> AliasWithholdingResult<(T, Vec<u8>)> {
    let (_, bytes) = read_manifest_file(base_dir, field, rel)?;
    let value = serde_json::from_slice(&bytes).map_err(artifact_error)?;
    Ok((value, bytes))
}

fn read_manifest_json_or_jsonl<T: DeserializeOwned>(
    base_dir: &Path,
    field: &str,
    rel: &str,
) -> AliasWithholdingResult<(Vec<T>, Vec<u8>)> {
    let (_, bytes) = read_manifest_file(base_dir, field, rel)?;
    let first = bytes
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace());
    let values = if first == Some(b'[') {
        serde_json::from_slice(&bytes).map_err(artifact_error)?
    } else {
        let text = std::str::from_utf8(&bytes).map_err(|utf8_error| {
            error(
                AliasWithholdingErrorCode::ArtifactContract,
                format!("{field} is not UTF-8 JSONL: {utf8_error}"),
            )
        })?;
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).map_err(artifact_error))
            .collect::<AliasWithholdingResult<Vec<_>>>()?
    };
    Ok((values, bytes))
}

fn validate_native_engine_evidence(
    trial: &AliasWithholdingTrialSpec,
    _clean_base: &BaseRegistrySnapshot,
    _synthetic_absence: &ExactAbsenceProof,
    evidence: &AliasWithholdingNativeEvidence,
) -> AliasWithholdingResult<AliasWithholdingNativeEvidence> {
    let evidence = canonicalize_native_engine_evidence(evidence.clone())?;
    validate_native_engine_evidence_self_hash(&evidence)?;
    if evidence.trial_id != trial.trial_id {
        return Err(native_contract_error(
            "native engine evidence trial_id does not match the trial",
        ));
    }
    if evidence.observation_id != trial.withheld_alias.observation_id {
        return Err(native_contract_error(
            "native engine evidence observation_id does not match the withheld observation",
        ));
    }
    if evidence.clean_base_registry_digest != evidence.clean_registry_tree_hash
        || evidence.exact_absence_proof.base_registry_digest != evidence.clean_registry_tree_hash
    {
        return Err(native_contract_error(
            "native engine evidence clean-base digest does not bind the real registry tree",
        ));
    }
    if evidence.exact_absence_proof.lookup_found {
        return Err(native_contract_error(
            "native engine evidence exact-absence proof found the withheld alias",
        ));
    }
    let expected_fingerprint = hash_bytes(ascii_trim(&trial.withheld_alias.surface).as_bytes());
    if evidence.exact_absence_proof.lookup_value_fingerprint != expected_fingerprint {
        return Err(native_contract_error(
            "native engine evidence exact-absence fingerprint does not match the withheld alias",
        ));
    }
    validate_leakage_receipts(&evidence.leakage)?;
    validate_link_evidence(trial, &evidence.link)?;
    validate_candidate_recall_evidence(trial, &evidence.link, &evidence.candidate_recall)?;
    validate_run_evidence(&evidence.link, &evidence.run)?;
    validate_solve_evidence(trial, &evidence.link, &evidence.run, &evidence.solve)?;
    validate_review_evidence(&evidence.link, &evidence.solve, &evidence.review)?;
    validate_audit_evidence(
        &evidence.link,
        &evidence.run,
        &evidence.solve,
        &evidence.review,
        &evidence.audit,
    )?;
    validate_assignment_firewall(&evidence.assignment_firewall)?;
    validate_promotion_and_replay(trial, &evidence)?;
    Ok(evidence)
}

fn validate_native_engine_evidence_self_hash(
    evidence: &AliasWithholdingNativeEvidence,
) -> AliasWithholdingResult<()> {
    if evidence.version != CANON_ALIAS_WITHHOLDING_NATIVE_EVIDENCE_VERSION {
        return Err(native_contract_error(
            "native engine evidence has the wrong contract version",
        ));
    }
    let expected = native_engine_evidence_digest(evidence)?;
    if evidence.artifact_content_hash != expected {
        return Err(native_contract_error(
            "native engine evidence content hash is stale",
        ));
    }
    Ok(())
}

fn validate_leakage_receipts(receipts: &[LeakageReceipt]) -> AliasWithholdingResult<()> {
    let expected = LeakChannel::all().into_iter().collect::<BTreeSet<_>>();
    let actual = receipts
        .iter()
        .map(|receipt| receipt.channel)
        .collect::<BTreeSet<_>>();
    if actual != expected || receipts.len() != expected.len() {
        return Err(error(
            AliasWithholdingErrorCode::SideChannelLeak,
            "native engine evidence must include one clear receipt for each leakage channel",
        ));
    }
    for receipt in receipts {
        if receipt.status != LeakageCheckStatus::Clear {
            return Err(error(
                AliasWithholdingErrorCode::SideChannelLeak,
                format!("{} leakage receipt did not pass", receipt.channel.as_str()),
            ));
        }
        require_digest(
            &receipt.checked_artifact_hash,
            "leakage.checked_artifact_hash",
        )?;
        if receipt.checked_count == 0
            || receipt.checked_source_hashes.is_empty()
            || receipt.chain_binding_hashes.is_empty()
        {
            return Err(error(
                AliasWithholdingErrorCode::SideChannelLeak,
                format!(
                    "{} leakage receipt checked no bound source records",
                    receipt.channel.as_str()
                ),
            ));
        }
        for digest in &receipt.checked_source_hashes {
            require_digest(digest, "leakage.checked_source_hashes")?;
        }
        for digest in &receipt.chain_binding_hashes {
            require_digest(digest, "leakage.chain_binding_hashes")?;
        }
    }
    Ok(())
}

fn validate_candidate_recall_evidence(
    trial: &AliasWithholdingTrialSpec,
    link: &NativeLinkEvidence,
    recall: &NativeCandidateRecallEvidence,
) -> AliasWithholdingResult<()> {
    if recall.report_version != CANON_ENTITY_CANDIDATE_RECALL_VERSION {
        return Err(native_contract_error(
            "candidate-recall evidence has the wrong report version",
        ));
    }
    require_digest(&recall.report_hash, "candidate_recall.report_hash")?;
    if recall.gold_pair_id.trim().is_empty() {
        return Err(native_contract_error(
            "candidate-recall evidence must bind a non-empty gold pair",
        ));
    }
    if recall.cutoffs != [1, 5, 10, 25, 50] {
        return Err(native_contract_error(
            "candidate-recall evidence must use canonical cutoffs 1,5,10,25,50",
        ));
    }
    if recall.left_observation_id != link.asserted_reference_surface_id
        || recall.right_observation_id != link.target_surface_id
    {
        return Err(native_contract_error(
            "candidate-recall gold pair does not match the hash-bound link surfaces",
        ));
    }
    for rank in &recall.true_pair_ranks {
        if rank.rank == 0 || rank.rank > 50 || rank.operator_id.trim().is_empty() {
            return Err(native_contract_error(
                "candidate-recall true pair ranks must be 1..=50 with an operator id",
            ));
        }
    }
    for miss in &recall.misses_at_50 {
        if miss.gold_pair_id.trim().is_empty() || miss.reason.trim().is_empty() {
            return Err(native_contract_error(
                "candidate-recall misses must bind a gold pair and reason",
            ));
        }
    }
    match recall.disposition {
        NativeCandidateRecallDisposition::EvaluatedPair => {
            if recall.left_observation_id == recall.right_observation_id {
                return Err(native_contract_error(
                    "evaluated candidate-recall pair collapses to one prepared surface",
                ));
            }
            if recall.total_gold_pairs == 0
                || (recall.true_pair_ranks.is_empty()
                    && !recall
                        .misses_at_50
                        .iter()
                        .any(|miss| miss.gold_pair_id == recall.gold_pair_id))
            {
                return Err(native_contract_error(
                    "candidate-recall evidence has neither a rank nor miss forensic for the trial pair",
                ));
            }
        }
        NativeCandidateRecallDisposition::PreparedSurfaceCollapse => {
            if recall.left_observation_id != recall.right_observation_id
                || link.target_observation_id == link.asserted_reference_observation_id
                || link.decision != NativeLinkDecision::Matched
                || !recall.true_pair_ranks.is_empty()
                || !recall.misses_at_50.is_empty()
            {
                return Err(native_contract_error(
                    "prepared-surface collapse must bind distinct link observations to one surface without rank or miss credit",
                ));
            }
        }
        NativeCandidateRecallDisposition::RelationPolicyControl => {
            if trial
                .withheld_alias
                .relation_policy
                .identity_credit_allowed()
                || !recall.true_pair_ranks.is_empty()
                || !recall.misses_at_50.is_empty()
            {
                return Err(native_contract_error(
                    "relation-policy controls must be non-identity trials without candidate-recall rank or miss credit",
                ));
            }
        }
    }
    Ok(())
}

fn validate_link_evidence(
    trial: &AliasWithholdingTrialSpec,
    link: &NativeLinkEvidence,
) -> AliasWithholdingResult<()> {
    if link.artifact_version != CANON_ENTITY_LINK_VERSION
        || link.decision_artifact_version != CANON_ENTITY_LINK_DECISIONS_VERSION
    {
        return Err(native_contract_error(
            "link evidence has the wrong artifact version",
        ));
    }
    require_digest(&link.artifact_content_hash, "link.artifact_content_hash")?;
    require_digest(&link.decision_artifact_hash, "link.decision_artifact_hash")?;
    require_digest(&link.shared_run_hash, "link.shared_run_hash")?;
    require_digest(&link.shared_solve_hash, "link.shared_solve_hash")?;
    require_digest(
        &link.materialized_rows_content_hash,
        "link.materialized_rows_content_hash",
    )?;
    require_digest(
        &link.observation_surface_bindings_content_hash,
        "link.observation_surface_bindings_content_hash",
    )?;
    if link.target_observation_id != trial.withheld_alias.observation_id {
        return Err(native_contract_error(
            "link evidence target does not match the withheld observation",
        ));
    }
    if link.target_surface_id.trim().is_empty()
        || link.asserted_reference_observation_id.trim().is_empty()
        || link.asserted_reference_surface_id.trim().is_empty()
        || link.target_observation_id == link.asserted_reference_observation_id
    {
        return Err(native_contract_error(
            "link evidence must bind distinct target/reference observations to prepared surfaces",
        ));
    }
    if link.target_count != link.matched_count + link.ambiguous_count + link.unmatched_count {
        return Err(native_contract_error(
            "link evidence decision counts do not partition target count",
        ));
    }
    match link.decision {
        NativeLinkDecision::Matched => {
            if link.reference_observation_ids.len() != 1
                || link.reference_surface_ids.len() != 1
                || link.reference_observation_ids[0] != link.asserted_reference_observation_id
                || link.matched_reference_id.as_deref()
                    != Some(link.reference_observation_ids[0].as_str())
            {
                return Err(native_contract_error(
                    "matched link evidence must bind one hash-mapped asserted reference",
                ));
            }
        }
        NativeLinkDecision::Ambiguous => {
            if link.reference_observation_ids.len() < 2
                || link.reference_surface_ids.len() != link.reference_observation_ids.len()
                || link.matched_reference_id.is_some()
            {
                return Err(native_contract_error(
                    "ambiguous link evidence must bind candidate references",
                ));
            }
        }
        NativeLinkDecision::Unmatched => {
            if link.reference_surface_ids.len() != link.reference_observation_ids.len()
                || link.matched_reference_id.is_some()
            {
                return Err(native_contract_error(
                    "unmatched link evidence has inconsistent mapped references",
                ));
            }
        }
        NativeLinkDecision::Rejected => {
            if link.reference_surface_ids.len() != link.reference_observation_ids.len()
                || link.matched_reference_id.is_some()
            {
                return Err(native_contract_error(
                    "rejected link evidence must not carry an accepted reference",
                ));
            }
        }
    }
    Ok(())
}

fn validate_run_evidence(
    link: &NativeLinkEvidence,
    run: &NativeRunEvidence,
) -> AliasWithholdingResult<()> {
    if run.artifact_version != CANON_ENTITY_RUN_VERSION {
        return Err(native_contract_error(
            "run evidence has the wrong artifact version",
        ));
    }
    require_digest(&run.artifact_content_hash, "run.artifact_content_hash")?;
    require_digest(&run.solve_artifact_hash, "run.solve_artifact_hash")?;
    if run.artifact_content_hash != link.shared_run_hash
        || run.solve_artifact_hash != link.shared_solve_hash
    {
        return Err(native_contract_error(
            "run evidence does not match the link shared run/solve hashes",
        ));
    }
    let solve_stage = run.stage_artifacts.iter().any(|stage| {
        stage.stage == "solve"
            && stage.version == CANON_ENTITY_SOLVE_VERSION
            && stage.content_hash == run.solve_artifact_hash
    });
    if !solve_stage {
        return Err(native_contract_error(
            "run evidence must include the hash-bound solve stage",
        ));
    }
    for stage in &run.stage_artifacts {
        if stage.stage.trim().is_empty() || stage.version.trim().is_empty() {
            return Err(native_contract_error(
                "run stage artifact references must include stage and version",
            ));
        }
        require_digest(&stage.content_hash, "run.stage_artifacts.content_hash")?;
    }
    Ok(())
}

fn validate_solve_evidence(
    trial: &AliasWithholdingTrialSpec,
    link: &NativeLinkEvidence,
    run: &NativeRunEvidence,
    solve: &NativeSolveEvidence,
) -> AliasWithholdingResult<()> {
    if solve.artifact_version != CANON_ENTITY_SOLVE_VERSION {
        return Err(native_contract_error(
            "solve evidence has the wrong artifact version",
        ));
    }
    require_digest(&solve.artifact_content_hash, "solve.artifact_content_hash")?;
    if solve.artifact_content_hash != link.shared_solve_hash
        || solve.artifact_content_hash != run.solve_artifact_hash
    {
        return Err(native_contract_error(
            "solve evidence does not match the run/link solve hash",
        ));
    }
    let run_stage_hashes = run
        .stage_artifacts
        .iter()
        .map(|stage| stage.content_hash.as_str())
        .collect::<BTreeSet<_>>();
    if solve.upstream_artifact_hashes.is_empty()
        || solve
            .upstream_artifact_hashes
            .iter()
            .any(|hash| !is_blake3_digest(hash) || !run_stage_hashes.contains(hash.as_str()))
    {
        return Err(native_contract_error(
            "solve evidence upstream hashes do not match validated run stages",
        ));
    }
    if solve.target_observation_id != trial.withheld_alias.observation_id {
        return Err(native_contract_error(
            "solve evidence does not bind the trial target component",
        ));
    }
    if solve.target_surface_id != link.target_surface_id {
        return Err(native_contract_error(
            "solve evidence target surface does not match the hash-bound link target",
        ));
    }
    if solve.state == NativeSolveState::Absent {
        if solve.component_id.is_some()
            || !solve.component_surface_ids.is_empty()
            || solve.canonical_id.is_some()
            || link.decision == NativeLinkDecision::Matched
        {
            return Err(native_contract_error(
                "absent solve evidence is inconsistent with the link decision",
            ));
        }
        return Ok(());
    }
    if solve.component_id.as_deref().is_none_or(str::is_empty)
        || !solve
            .component_surface_ids
            .contains(&solve.target_surface_id)
    {
        return Err(native_contract_error(
            "solve evidence does not bind the trial target component",
        ));
    }
    let contains_incumbent_reference = solve
        .component_surface_ids
        .contains(&link.asserted_reference_surface_id)
        || link.reference_surface_ids.iter().any(|reference| {
            solve
                .component_surface_ids
                .iter()
                .any(|surface_id| surface_id == reference)
        });
    let resolves_to_incumbent =
        solve.canonical_id.as_deref() == Some(trial.entity.canonical_id.as_str());
    match link.decision {
        NativeLinkDecision::Matched => {
            if !contains_incumbent_reference
                || !resolves_to_incumbent
                || solve.state != NativeSolveState::ResolvedExisting
            {
                return Err(native_contract_error(
                    "matched link does not agree with the validated solve target component",
                ));
            }
        }
        NativeLinkDecision::Ambiguous
        | NativeLinkDecision::Unmatched
        | NativeLinkDecision::Rejected => {
            if contains_incumbent_reference || resolves_to_incumbent {
                return Err(native_contract_error(
                    "non-attach link does not agree with the validated solve target component",
                ));
            }
        }
    }
    Ok(())
}

fn validate_review_evidence(
    link: &NativeLinkEvidence,
    solve: &NativeSolveEvidence,
    review: &NativeReviewEvidence,
) -> AliasWithholdingResult<()> {
    if review.artifact_version != CANON_ENTITY_REVIEW_QUEUE_VERSION {
        return Err(native_contract_error(
            "review evidence has the wrong artifact version",
        ));
    }
    require_digest(
        &review.artifact_content_hash,
        "review.artifact_content_hash",
    )?;
    if review.source_link_hash != link.artifact_content_hash
        || review.source_solve_hash != solve.artifact_content_hash
    {
        return Err(native_contract_error(
            "review evidence does not match the validated link/solve sources",
        ));
    }
    if review.review_id.trim().is_empty()
        || review.proposed_action.trim().is_empty()
        || review.review_item_count == 0
    {
        return Err(native_contract_error(
            "review evidence must bind a review item and proposed action",
        ));
    }
    Ok(())
}

fn validate_audit_evidence(
    link: &NativeLinkEvidence,
    run: &NativeRunEvidence,
    solve: &NativeSolveEvidence,
    review: &NativeReviewEvidence,
    audit: &NativeAuditEvidence,
) -> AliasWithholdingResult<()> {
    if audit.artifact_version != CANON_ENTITY_AUDIT_VERSION {
        return Err(native_contract_error(
            "audit evidence has the wrong artifact version",
        ));
    }
    require_digest(&audit.artifact_content_hash, "audit.artifact_content_hash")?;
    require_digest(&audit.audited_artifact_hash, "audit.audited_artifact_hash")?;
    if audit.status != NativeAuditStatus::Passed || audit.gate_count == 0 {
        return Err(native_contract_error(
            "audit evidence must be a passing audit with at least one gate",
        ));
    }
    if audit.audited_artifact_hash != solve.artifact_content_hash
        || link.shared_solve_hash != solve.artifact_content_hash
        || review.source_solve_hash != solve.artifact_content_hash
    {
        return Err(native_contract_error(
            "audit evidence must audit the validated solve artifact",
        ));
    }
    let stage_hashes = run
        .stage_artifacts
        .iter()
        .map(|stage| stage.content_hash.as_str())
        .collect::<BTreeSet<_>>();
    if !stage_hashes.contains(solve.artifact_content_hash.as_str()) {
        return Err(native_contract_error(
            "run evidence must include the audited solve as a stage artifact",
        ));
    }
    for gate in &audit.required_gate_ids {
        if gate.trim().is_empty() {
            return Err(native_contract_error(
                "audit required gate ids must be non-empty",
            ));
        }
    }
    Ok(())
}

fn validate_assignment_firewall(
    firewall: &NativeAssignmentFirewallEvidence,
) -> AliasWithholdingResult<()> {
    require_digest(
        &firewall.artifact_content_hash,
        "assignment_firewall.artifact_content_hash",
    )?;
    if firewall.checked_source_count < 2
        || firewall.checked_source_hashes.len() as u64 != firewall.checked_source_count
        || firewall.chain_binding_hashes.is_empty()
        || firewall.assignment_fact_count == 0
        || firewall.issuer_identity_alias_count == 0
        || firewall.assignment_fact_hashes.is_empty()
    {
        return Err(native_contract_error(
            "assignment firewall must bind concrete assignment and issuer-identity sources",
        ));
    }
    if firewall.assignment_facts_used_as_aliases {
        return Err(native_contract_error(
            "assignment facts cannot count as identity aliases",
        ));
    }
    if firewall.assignment_fact_count != firewall.assignment_fact_hashes.len() as u64 {
        return Err(native_contract_error(
            "assignment firewall fact count must equal assignment fact hashes length",
        ));
    }
    if firewall.assignment_derived_alias_count != 0
        || firewall.identity_key_count != 0
        || firewall.external_crosswalk_identity_key_count != 0
    {
        return Err(native_contract_error(
            "assignment firewall includes assignment-derived identity material",
        ));
    }
    let unique = firewall
        .assignment_fact_hashes
        .iter()
        .collect::<BTreeSet<_>>();
    if unique.len() != firewall.assignment_fact_hashes.len() {
        return Err(native_contract_error(
            "assignment firewall assignment fact hashes must be unique",
        ));
    }
    for digest in &firewall.assignment_fact_hashes {
        require_digest(digest, "assignment_firewall.assignment_fact_hashes")?;
    }
    for digest in &firewall.checked_source_hashes {
        require_digest(digest, "assignment_firewall.checked_source_hashes")?;
    }
    for digest in &firewall.chain_binding_hashes {
        require_digest(digest, "assignment_firewall.chain_binding_hashes")?;
    }
    Ok(())
}

fn validate_promotion_and_replay(
    trial: &AliasWithholdingTrialSpec,
    evidence: &AliasWithholdingNativeEvidence,
) -> AliasWithholdingResult<()> {
    if !trial
        .withheld_alias
        .relation_policy
        .identity_credit_allowed()
    {
        if evidence.promotion.is_some() || evidence.exact_replay.is_some() {
            return Err(native_contract_error(
                "relation-policy controls must not include promotion or exact replay",
            ));
        }
        return Ok(());
    }
    match evidence.link.decision {
        NativeLinkDecision::Matched => {
            let promotion = evidence.promotion.as_ref().ok_or_else(|| {
                native_contract_error("matched native evidence requires sandbox promotion evidence")
            })?;
            let replay = evidence.exact_replay.as_ref().ok_or_else(|| {
                native_contract_error("matched native evidence requires exact replay evidence")
            })?;
            validate_promotion_evidence(&evidence.audit, promotion)?;
            validate_exact_replay_evidence(trial, promotion, replay)?;
        }
        NativeLinkDecision::Ambiguous
        | NativeLinkDecision::Unmatched
        | NativeLinkDecision::Rejected => {
            if evidence
                .promotion
                .as_ref()
                .is_some_and(|promotion| promotion.status == NativePromotionStatus::Approved)
            {
                return Err(native_contract_error(
                    "non-attach native evidence must not include approved promotion",
                ));
            }
            if evidence.exact_replay.is_some() {
                return Err(native_contract_error(
                    "non-attach native evidence must not include exact replay",
                ));
            }
        }
    }
    Ok(())
}

fn validate_promotion_evidence(
    audit: &NativeAuditEvidence,
    promotion: &NativePromotionEvidence,
) -> AliasWithholdingResult<()> {
    if !matches!(
        promotion.artifact_version.as_str(),
        CANON_ENTITY_PROMOTE_VERSION_V1 | CANON_REGISTRY_ADD_ENTRY_PROMOTION_VERSION
    ) {
        return Err(native_contract_error(
            "promotion evidence has the wrong artifact version",
        ));
    }
    require_digest(
        &promotion.artifact_content_hash,
        "promotion.artifact_content_hash",
    )?;
    require_digest(
        &promotion.sandbox_registry_digest_before,
        "promotion.sandbox_registry_digest_before",
    )?;
    require_digest(
        &promotion.sandbox_registry_digest_after,
        "promotion.sandbox_registry_digest_after",
    )?;
    if promotion.audit_artifact_hash != audit.artifact_content_hash
        || promotion.status != NativePromotionStatus::Approved
        || promotion.promoted_alias_count == 0
        || promotion.sandbox_registry_digest_before == promotion.sandbox_registry_digest_after
    {
        return Err(native_contract_error(
            "promotion evidence must be approved, audited, sandboxed, and registry-changing",
        ));
    }
    Ok(())
}

fn validate_exact_replay_evidence(
    trial: &AliasWithholdingTrialSpec,
    promotion: &NativePromotionEvidence,
    replay: &NativeExactReplayEvidence,
) -> AliasWithholdingResult<()> {
    if replay.apply_artifact_version != CANON_ENTITY_APPLY_VERSION {
        return Err(native_contract_error(
            "exact replay evidence has the wrong apply artifact version",
        ));
    }
    require_digest(
        &replay.apply_artifact_hash,
        "exact_replay.apply_artifact_hash",
    )?;
    require_digest(
        &replay.output_content_hash,
        "exact_replay.output_content_hash",
    )?;
    require_digest(&replay.registry_digest, "exact_replay.registry_digest")?;
    require_digest(&replay.input_fingerprint, "exact_replay.input_fingerprint")?;
    if replay.registry_digest != promotion.sandbox_registry_digest_after {
        return Err(native_contract_error(
            "exact replay registry digest does not match sandbox promotion output",
        ));
    }
    if replay.exact_replay_canonical_id.trim().is_empty() {
        return Err(native_contract_error(
            "exact replay canonical id must be present",
        ));
    }
    if replay.exact_replay_canonical_id != trial.entity.canonical_id {
        return Err(error(
            AliasWithholdingErrorCode::ReplayMismatch,
            "exact replay canonical id does not match the incumbent entity",
        ));
    }
    Ok(())
}

fn derive_native_candidate_evaluation(
    trial: &AliasWithholdingTrialSpec,
    evidence: &AliasWithholdingNativeEvidence,
) -> AliasWithholdingResult<CandidateEvaluation> {
    let candidate_rank = native_candidate_rank(&evidence.candidate_recall);
    let (decision, candidate_canonical_id, abstention_action, review_action, promotion_replay) =
        match evidence.link.decision {
            NativeLinkDecision::Matched => {
                if candidate_rank.is_none()
                    && evidence.candidate_recall.disposition
                        == NativeCandidateRecallDisposition::EvaluatedPair
                {
                    return Err(native_contract_error(
                        "matched native link evidence has no candidate-recall rank",
                    ));
                }
                if evidence.candidate_recall.disposition
                    == NativeCandidateRecallDisposition::RelationPolicyControl
                {
                    (
                        EntityEngineDecision::Attach,
                        evidence.solve.canonical_id.clone(),
                        ReviewAction::RecordCannotLink,
                        ReviewAction::RejectCandidate,
                        None,
                    )
                } else {
                    (
                        EntityEngineDecision::Attach,
                        evidence.solve.canonical_id.clone(),
                        ReviewAction::NoAction,
                        ReviewAction::PromoteAlias,
                        evidence.promotion.as_ref().and_then(|promotion| {
                            evidence
                                .exact_replay
                                .as_ref()
                                .map(|replay| PromotionReplay {
                                    approved: promotion.status == NativePromotionStatus::Approved,
                                    promoted_registry_digest: promotion
                                        .sandbox_registry_digest_after
                                        .clone(),
                                    exact_replay_canonical_id: Some(
                                        replay.exact_replay_canonical_id.clone(),
                                    ),
                                })
                        }),
                    )
                }
            }
            NativeLinkDecision::Ambiguous | NativeLinkDecision::Unmatched => (
                EntityEngineDecision::Abstain,
                None,
                ReviewAction::DeferReview,
                ReviewAction::DeferReview,
                None,
            ),
            NativeLinkDecision::Rejected => (
                EntityEngineDecision::Reject,
                None,
                ReviewAction::RecordCannotLink,
                ReviewAction::RejectCandidate,
                None,
            ),
        };
    let evidence_lanes = native_evidence_lanes(trial, evidence, decision);
    Ok(CandidateEvaluation {
        candidate_rank,
        decision,
        candidate_canonical_id,
        evidence_lanes,
        abstention_action,
        review_action,
        promotion_replay,
    })
}

fn native_evidence_lanes(
    trial: &AliasWithholdingTrialSpec,
    evidence: &AliasWithholdingNativeEvidence,
    decision: EntityEngineDecision,
) -> Vec<EvidenceLaneReport> {
    let relation_policy_control = evidence.candidate_recall.disposition
        == NativeCandidateRecallDisposition::RelationPolicyControl;
    let support = if decision == EntityEngineDecision::Attach && !relation_policy_control {
        10_000
    } else {
        0
    };
    let contradiction = if decision == EntityEngineDecision::Reject
        || (decision == EntityEngineDecision::Attach && relation_policy_control)
    {
        10_000
    } else {
        0
    };
    let candidate_recall_support = if evidence.candidate_recall.disposition
        == NativeCandidateRecallDisposition::EvaluatedPair
    {
        support
    } else {
        0
    };
    let mut lanes = vec![
        EvidenceLaneReport {
            lane_id: "candidate_recall".to_string(),
            support_basis_points: candidate_recall_support,
            contradiction_basis_points: 0,
            public_evidence_ref: evidence.candidate_recall.report_hash.clone(),
        },
        EvidenceLaneReport {
            lane_id: "link_decision".to_string(),
            support_basis_points: support,
            contradiction_basis_points: contradiction,
            public_evidence_ref: evidence.link.artifact_content_hash.clone(),
        },
        EvidenceLaneReport {
            lane_id: "review_audit".to_string(),
            support_basis_points: support,
            contradiction_basis_points: contradiction,
            public_evidence_ref: evidence.audit.artifact_content_hash.clone(),
        },
    ];
    if evidence.candidate_recall.disposition
        == NativeCandidateRecallDisposition::PreparedSurfaceCollapse
    {
        lanes.push(EvidenceLaneReport {
            lane_id: "prepared_surface_collapse".to_string(),
            support_basis_points: support,
            contradiction_basis_points: 0,
            public_evidence_ref: evidence
                .link
                .observation_surface_bindings_content_hash
                .clone(),
        });
    }
    if relation_policy_control {
        lanes.push(EvidenceLaneReport {
            lane_id: "relation_policy_control".to_string(),
            support_basis_points: 0,
            contradiction_basis_points: contradiction,
            public_evidence_ref: evidence.link.artifact_content_hash.clone(),
        });
    }
    if let Some(replay) = &evidence.exact_replay {
        lanes.push(EvidenceLaneReport {
            lane_id: format!("exact_replay:{}", trial.withheld_alias.observation_id),
            support_basis_points: support,
            contradiction_basis_points: 0,
            public_evidence_ref: replay.apply_artifact_hash.clone(),
        });
    }
    lanes.sort();
    lanes
}

fn native_engine_evidence_receipt(
    evidence: &AliasWithholdingNativeEvidence,
) -> NativeEngineEvidenceReceipt {
    NativeEngineEvidenceReceipt {
        evidence_hash: evidence.artifact_content_hash.clone(),
        clean_registry_tree_hash: evidence.clean_registry_tree_hash.clone(),
        candidate_recall_report_hash: evidence.candidate_recall.report_hash.clone(),
        candidate_recall_manifest_hash: evidence.candidate_recall_manifest_hash.clone(),
        candidate_records_hash: evidence.candidate_records_hash.clone(),
        candidate_diagnostics_hash: evidence.candidate_diagnostics_hash.clone(),
        candidate_recall_disposition: evidence.candidate_recall.disposition,
        link_artifact_hash: evidence.link.artifact_content_hash.clone(),
        link_materialized_rows_hash: evidence.link.materialized_rows_content_hash.clone(),
        link_observation_surface_bindings_hash: evidence
            .link
            .observation_surface_bindings_content_hash
            .clone(),
        run_artifact_hash: evidence.run.artifact_content_hash.clone(),
        solve_artifact_hash: evidence.solve.artifact_content_hash.clone(),
        review_queue_hash: evidence.review.artifact_content_hash.clone(),
        audit_artifact_hash: evidence.audit.artifact_content_hash.clone(),
        promotion_artifact_hash: evidence
            .promotion
            .as_ref()
            .map(|promotion| promotion.artifact_content_hash.clone()),
        apply_artifact_hash: evidence
            .exact_replay
            .as_ref()
            .map(|replay| replay.apply_artifact_hash.clone()),
        leak_channels_checked: evidence
            .leakage
            .iter()
            .map(|receipt| receipt.channel)
            .collect(),
        leakage_scan_hashes: evidence
            .leakage
            .iter()
            .map(|receipt| receipt.checked_artifact_hash.clone())
            .collect(),
        assignment_firewall_artifact_hash: evidence
            .assignment_firewall
            .artifact_content_hash
            .clone(),
        assignment_checked_source_count: evidence.assignment_firewall.checked_source_count,
        assignment_fact_count: evidence.assignment_firewall.assignment_fact_count,
        issuer_identity_alias_count: evidence.assignment_firewall.issuer_identity_alias_count,
        assignment_derived_alias_count: evidence.assignment_firewall.assignment_derived_alias_count,
        identity_key_count: evidence.assignment_firewall.identity_key_count,
        external_crosswalk_identity_key_count: evidence
            .assignment_firewall
            .external_crosswalk_identity_key_count,
        assignment_facts_used_as_aliases: evidence
            .assignment_firewall
            .assignment_facts_used_as_aliases,
    }
}

fn canonicalize_execution_manifest(
    mut manifest: AliasWithholdingExecutionManifest,
) -> AliasWithholdingResult<AliasWithholdingExecutionManifest> {
    if ascii_trim(&manifest.version).is_empty() {
        manifest.version = CANON_ALIAS_WITHHOLDING_EXECUTION_MANIFEST_VERSION.to_string();
    }
    if manifest.version != CANON_ALIAS_WITHHOLDING_EXECUTION_MANIFEST_VERSION {
        return Err(native_contract_error(
            "alias-withholding execution manifest has the wrong version",
        ));
    }
    manifest.trial_id = normalize_non_empty(manifest.trial_id, "manifest.trial_id")?;
    manifest.observation_id =
        normalize_non_empty(manifest.observation_id, "manifest.observation_id")?;
    manifest.assertions.gold_pair_id =
        normalize_non_empty(manifest.assertions.gold_pair_id, "assertions.gold_pair_id")?;
    manifest.assertions.reference_observation_id = normalize_non_empty(
        manifest.assertions.reference_observation_id,
        "assertions.reference_observation_id",
    )?;
    manifest.assertions.target_observation_id = normalize_non_empty(
        manifest.assertions.target_observation_id,
        "assertions.target_observation_id",
    )?;
    manifest.assertions.incumbent_canonical_id = normalize_non_empty(
        manifest.assertions.incumbent_canonical_id,
        "assertions.incumbent_canonical_id",
    )?;
    manifest.assertions.review_id = manifest
        .assertions
        .review_id
        .map(|value| normalize_non_empty(value, "assertions.review_id"))
        .transpose()?;
    manifest.clean_registry_dir =
        normalize_non_empty(manifest.clean_registry_dir, "clean_registry_dir")?;
    manifest.link_artifact_path =
        normalize_non_empty(manifest.link_artifact_path, "link_artifact_path")?;
    manifest.run_artifact_path =
        normalize_non_empty(manifest.run_artifact_path, "run_artifact_path")?;
    manifest.solve_artifact_path =
        normalize_non_empty(manifest.solve_artifact_path, "solve_artifact_path")?;
    manifest.review_queue_artifact_path = normalize_non_empty(
        manifest.review_queue_artifact_path,
        "review_queue_artifact_path",
    )?;
    manifest.audit_artifact_path =
        normalize_non_empty(manifest.audit_artifact_path, "audit_artifact_path")?;
    manifest.assignment_firewall_path = normalize_non_empty(
        manifest.assignment_firewall_path,
        "assignment_firewall_path",
    )?;
    manifest.candidate_recall.quality_manifest_path = normalize_non_empty(
        manifest.candidate_recall.quality_manifest_path,
        "candidate_recall.quality_manifest_path",
    )?;
    manifest.candidate_recall.block_artifact_path = normalize_non_empty(
        manifest.candidate_recall.block_artifact_path,
        "candidate_recall.block_artifact_path",
    )?;
    manifest.candidate_recall.candidates_path = normalize_non_empty(
        manifest.candidate_recall.candidates_path,
        "candidate_recall.candidates_path",
    )?;
    manifest.candidate_recall.diagnostics_path = normalize_non_empty(
        manifest.candidate_recall.diagnostics_path,
        "candidate_recall.diagnostics_path",
    )?;
    manifest.candidate_recall.exact_bucket_assertions_path = normalize_non_empty(
        manifest.candidate_recall.exact_bucket_assertions_path,
        "candidate_recall.exact_bucket_assertions_path",
    )?;
    manifest.candidate_recall.report_path = normalize_non_empty(
        manifest.candidate_recall.report_path,
        "candidate_recall.report_path",
    )?;
    if let Some(promotion) = &mut manifest.promotion {
        promotion.promoted_registry_dir = normalize_non_empty(
            promotion.promoted_registry_dir.clone(),
            "promotion.promoted_registry_dir",
        )?;
        promotion.promotion_artifact_path = promotion
            .promotion_artifact_path
            .take()
            .map(|value| normalize_non_empty(value, "promotion.promotion_artifact_path"))
            .transpose()?;
        promotion.review_import_receipt_path = promotion
            .review_import_receipt_path
            .take()
            .map(|value| normalize_non_empty(value, "promotion.review_import_receipt_path"))
            .transpose()?;
    }
    if let Some(replay) = &mut manifest.exact_replay {
        replay.input_path =
            normalize_non_empty(replay.input_path.clone(), "exact_replay.input_path")?;
        replay.lookup_column =
            normalize_non_empty(replay.lookup_column.clone(), "exact_replay.lookup_column")?;
        replay.apply_artifact_path = normalize_non_empty(
            replay.apply_artifact_path.clone(),
            "exact_replay.apply_artifact_path",
        )?;
        replay.output_path =
            normalize_non_empty(replay.output_path.clone(), "exact_replay.output_path")?;
    }
    for leakage in &mut manifest.leakage {
        leakage.artifact_path =
            normalize_non_empty(leakage.artifact_path.clone(), "leakage.artifact_path")?;
    }
    manifest.leakage.sort_by_key(|leakage| leakage.channel);
    Ok(manifest)
}

fn canonicalize_native_engine_evidence(
    mut evidence: AliasWithholdingNativeEvidence,
) -> AliasWithholdingResult<AliasWithholdingNativeEvidence> {
    for receipt in &mut evidence.leakage {
        receipt.checked_source_hashes.sort();
        receipt.chain_binding_hashes.sort();
        receipt.chain_binding_hashes.dedup();
    }
    evidence.leakage.sort();
    evidence.candidate_recall.true_pair_ranks.sort();
    evidence.candidate_recall.misses_at_50.sort();
    evidence.link.reference_observation_ids.sort();
    evidence.link.reference_observation_ids.dedup();
    evidence.link.reference_surface_ids.sort();
    evidence.run.stage_artifacts.sort();
    evidence.solve.upstream_artifact_hashes.sort();
    evidence.solve.upstream_artifact_hashes.dedup();
    evidence.solve.component_surface_ids.sort();
    evidence.solve.component_surface_ids.dedup();
    evidence.audit.required_gate_ids.sort();
    evidence.assignment_firewall.checked_source_hashes.sort();
    evidence.assignment_firewall.chain_binding_hashes.sort();
    evidence.assignment_firewall.chain_binding_hashes.dedup();
    evidence.assignment_firewall.assignment_fact_hashes.sort();
    evidence.assignment_firewall.assignment_fact_hashes.dedup();
    Ok(evidence)
}

fn native_engine_evidence_digest(
    evidence: &AliasWithholdingNativeEvidence,
) -> AliasWithholdingResult<String> {
    let mut evidence = canonicalize_native_engine_evidence(evidence.clone())?;
    evidence.artifact_content_hash.clear();
    hash_serialized(&evidence)
}

fn validate_manifest_trial_binding(
    trial: &AliasWithholdingTrialSpec,
    manifest: &AliasWithholdingExecutionManifest,
) -> AliasWithholdingResult<()> {
    if manifest.trial_id != trial.trial_id
        || manifest.observation_id != trial.withheld_alias.observation_id
        || manifest.assertions.target_observation_id != trial.withheld_alias.observation_id
        || manifest.assertions.incumbent_canonical_id != trial.entity.canonical_id
    {
        return Err(native_contract_error(
            "execution manifest assertions do not bind the trial",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
struct CandidateRecallManifest {
    #[serde(default)]
    observations: Vec<CandidateRecallManifestObservation>,
    quality_harness: CandidateRecallManifestHarness,
}

#[derive(Debug, Clone, Deserialize)]
struct CandidateRecallManifestObservation {
    observation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CandidateRecallManifestHarness {
    #[serde(default)]
    cases: Vec<CandidateRecallManifestCase>,
}

#[derive(Debug, Clone, Deserialize)]
struct CandidateRecallManifestCase {
    case_id: String,
    left_observation_id: String,
    right_observation_id: String,
    stratum: String,
    label_disposition: String,
}

fn candidate_recall_manifest_gold(
    manifest: &CandidateRecallManifest,
) -> AliasWithholdingResult<(Vec<String>, Vec<CandidateRecallGoldPair>)> {
    let mut surface_ids = manifest
        .observations
        .iter()
        .map(|observation| ascii_trim(&observation.observation_id))
        .filter(|observation_id| !observation_id.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    surface_ids.sort();
    surface_ids.dedup();

    let mut gold_pairs = Vec::new();
    for case in &manifest.quality_harness.cases {
        if case.label_disposition != "same_entity" {
            continue;
        }
        gold_pairs.push(CandidateRecallGoldPair::new(
            &case.case_id,
            &case.left_observation_id,
            &case.right_observation_id,
            candidate_recall_stratum(&case.stratum)?,
        ));
    }
    gold_pairs.sort_by(|left, right| left.gold_pair_id.cmp(&right.gold_pair_id));
    Ok((surface_ids, gold_pairs))
}

fn validate_candidate_recall_manifest_case(
    manifest: &CandidateRecallManifest,
    assertions: &AliasWithholdingExecutionAssertions,
    expected_left_surface_id: &str,
    expected_right_surface_id: &str,
    disposition: NativeCandidateRecallDisposition,
) -> AliasWithholdingResult<()> {
    let case = manifest
        .quality_harness
        .cases
        .iter()
        .find(|case| case.case_id == assertions.gold_pair_id)
        .ok_or_else(|| {
            native_contract_error("candidate-recall manifest lacks asserted gold pair case")
        })?;
    let expected_label = match disposition {
        NativeCandidateRecallDisposition::EvaluatedPair => "same_entity",
        NativeCandidateRecallDisposition::PreparedSurfaceCollapse => "prepared_surface_collapse",
        NativeCandidateRecallDisposition::RelationPolicyControl => "relation_policy_control",
    };
    if case.label_disposition != expected_label
        || case.left_observation_id != expected_left_surface_id
        || case.right_observation_id != expected_right_surface_id
    {
        return Err(native_contract_error(
            "candidate-recall manifest case does not match execution assertions",
        ));
    }
    Ok(())
}

fn candidate_recall_stratum(stratum: &str) -> AliasWithholdingResult<CandidateRecallStratum> {
    match stratum {
        "exact_known" | "exact_known_replay" => Ok(CandidateRecallStratum::ExactKnown),
        "withheld_alias" | "withheld_alias_incumbent" => Ok(CandidateRecallStratum::WithheldAlias),
        "novel_cluster" | "novel_multi_observation" => Ok(CandidateRecallStratum::NovelCluster),
        "directional_link" | "directional_cross_dataset_link" => {
            Ok(CandidateRecallStratum::DirectionalLink)
        }
        _ => Err(native_contract_error(format!(
            "unsupported candidate recall stratum {stratum}"
        ))),
    }
}

fn native_solve_state(state: SolveReconciliationState) -> NativeSolveState {
    match state {
        SolveReconciliationState::ResolvedExisting => NativeSolveState::ResolvedExisting,
        SolveReconciliationState::PromotableNew => NativeSolveState::PromotableNew,
        SolveReconciliationState::Escrow => NativeSolveState::Escrow,
        SolveReconciliationState::Contradiction => NativeSolveState::Contradiction,
        SolveReconciliationState::Conflict => NativeSolveState::Conflict,
    }
}

fn outcome_for_evaluation(
    trial: &AliasWithholdingTrialSpec,
    withheld_alias: &AliasRecord,
    evaluation: &CandidateEvaluation,
) -> AliasWithholdingResult<TrialOutcome> {
    let mut trial = trial.clone();
    trial.evaluation = evaluation.clone();
    outcome_for_trial(&trial, withheld_alias)
}

fn outcome_for_native_evaluation(
    trial: &AliasWithholdingTrialSpec,
    withheld_alias: &AliasRecord,
    evaluation: &CandidateEvaluation,
    evidence: &AliasWithholdingNativeEvidence,
) -> AliasWithholdingResult<TrialOutcome> {
    if evidence.candidate_recall.disposition
        != NativeCandidateRecallDisposition::PreparedSurfaceCollapse
    {
        return outcome_for_evaluation(trial, withheld_alias, evaluation);
    }
    if evaluation.decision != EntityEngineDecision::Attach {
        return outcome_for_evaluation(trial, withheld_alias, evaluation);
    }
    if !trial
        .withheld_alias
        .relation_policy
        .identity_credit_allowed()
    {
        return Ok(TrialOutcome::UnsupportedGuess);
    }
    if evaluation.candidate_rank.is_some()
        || evaluation.candidate_canonical_id.as_deref() != Some(trial.entity.canonical_id.as_str())
    {
        return Ok(TrialOutcome::CandidateMiss);
    }
    match &evaluation.promotion_replay {
        Some(replay) if replay.approved => {
            if replay.exact_replay_canonical_id.as_deref()
                == Some(trial.entity.canonical_id.as_str())
            {
                Ok(TrialOutcome::CorrectAttachment)
            } else {
                Ok(TrialOutcome::ReplayMismatch)
            }
        }
        _ => Err(error(
            AliasWithholdingErrorCode::ReplayMismatch,
            format!(
                "approved promotion replay is required for collapsed withheld alias {}",
                withheld_alias.alias_id
            ),
        )),
    }
}

fn native_candidate_rank(recall: &NativeCandidateRecallEvidence) -> Option<u32> {
    recall
        .true_pair_ranks
        .iter()
        .filter(|rank| rank.gold_pair_id == recall.gold_pair_id)
        .map(|rank| rank.rank)
        .min()
}

fn require_digest(value: &str, field: &str) -> AliasWithholdingResult<()> {
    if is_blake3_digest(value) {
        Ok(())
    } else {
        Err(native_contract_error(format!(
            "{field} must be a lowercase blake3 digest"
        )))
    }
}

fn validate_run_artifact_self_hash(run: &EntityRunArtifact) -> AliasWithholdingResult<()> {
    let mut hashable = run.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let expected = hash_serialized(&hashable)?;
    if run.artifact_content_hash != expected || run.metadata.artifact_content_hash != expected {
        return Err(native_contract_error("run artifact self hash is stale"));
    }
    Ok(())
}

fn validate_audit_artifact_self_hash(audit: &EntityAuditArtifact) -> AliasWithholdingResult<()> {
    let mut hashable = audit.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let expected = hash_serialized(&hashable)?;
    if audit.artifact_content_hash != expected || audit.metadata.artifact_content_hash != expected {
        return Err(native_contract_error("audit artifact self hash is stale"));
    }
    Ok(())
}

fn validate_apply_artifact_self_hash(apply: &ApplyRunArtifact) -> AliasWithholdingResult<()> {
    let mut hashable = apply.clone();
    hashable.artifact_content_hash.clear();
    let expected = hash_serialized(&hashable)?;
    if apply.artifact_content_hash != expected {
        return Err(native_contract_error("apply artifact self hash is stale"));
    }
    Ok(())
}

fn bytes_contain(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn value_string<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    value_at(value, path).and_then(Value::as_str)
}

fn value_u64(value: &Value, path: &[&str]) -> Option<u64> {
    value_at(value, path).and_then(Value::as_u64)
}

fn value_array<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Vec<Value>> {
    value_at(value, path).and_then(Value::as_array)
}

fn value_array_len(value: &Value, path: &[&str]) -> Option<u64> {
    value_array(value, path).map(|array| array.len() as u64)
}

fn value_string_array_required(
    value: &Value,
    path: &[&str],
) -> AliasWithholdingResult<Vec<String>> {
    value_array(value, path)
        .ok_or_else(|| native_contract_error(format!("missing string array {}", path.join("."))))?
        .iter()
        .map(|item| {
            item.as_str().map(str::to_string).ok_or_else(|| {
                native_contract_error(format!("{} must contain strings", path.join(".")))
            })
        })
        .collect()
}

fn validate_value_contract_version(
    value: &Value,
    expected: &str,
    label: &str,
) -> AliasWithholdingResult<()> {
    let version = value_string(value, &["version"])
        .ok_or_else(|| native_contract_error(format!("{label} must declare contract version")))?;
    if version != expected {
        return Err(native_contract_error(format!(
            "{label} has the wrong contract version"
        )));
    }
    Ok(())
}

fn validate_required_value_self_hash(
    value: &Value,
    bytes: &[u8],
    label: &str,
) -> AliasWithholdingResult<()> {
    let declared = value_string(value, &["artifact_content_hash"])
        .or_else(|| value_string(value, &["receipt_content_hash"]))
        .or_else(|| value_string(value, &["content_hash"]))
        .ok_or_else(|| native_contract_error(format!("{label} must declare self hash")))?;
    validate_declared_value_self_hash(value, bytes, label, declared)
}

fn validate_declared_value_self_hash(
    value: &Value,
    bytes: &[u8],
    label: &str,
    declared: &str,
) -> AliasWithholdingResult<()> {
    require_digest(declared, label)?;
    let mut hashable = value.clone();
    clear_value_hash(&mut hashable, &["artifact_content_hash"]);
    clear_value_hash(&mut hashable, &["receipt_content_hash"]);
    clear_value_hash(&mut hashable, &["content_hash"]);
    clear_value_hash(&mut hashable, &["metadata", "artifact_content_hash"]);
    let expected = hash_serialized(&hashable)?;
    if declared != expected && declared != hash_bytes(bytes) {
        return Err(native_contract_error(format!("{label} self hash is stale")));
    }
    Ok(())
}

fn clear_value_hash(value: &mut Value, path: &[&str]) {
    if path.is_empty() {
        return;
    }
    let mut current = value;
    for key in &path[..path.len() - 1] {
        let Some(next) = current.get_mut(*key) else {
            return;
        };
        current = next;
    }
    if let Some(object) = current.as_object_mut()
        && let Some(hash) = object.get_mut(path[path.len() - 1])
    {
        *hash = Value::String(String::new());
    }
}

fn output_maps_withheld_alias(
    bytes: &[u8],
    lookup_column: &str,
    withheld_surface: &str,
    canonical_id: &str,
) -> AliasWithholdingResult<bool> {
    let mut reader = csv::Reader::from_reader(bytes);
    let headers = reader
        .headers()
        .map_err(|error| native_contract_error(error.to_string()))?
        .clone();
    let lookup_index = headers
        .iter()
        .position(|header| header == lookup_column)
        .ok_or_else(|| native_contract_error("exact replay output lacks lookup column"))?;
    let canonical_index = headers
        .iter()
        .position(|header| {
            matches!(
                header,
                "canonical_id" | "canon_canonical_id" | "_canonical_id" | "_org_canonical_id"
            )
        })
        .ok_or_else(|| native_contract_error("exact replay output lacks canonical id column"))?;
    let records = reader
        .records()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| native_contract_error(error.to_string()))?;
    if records.len() != 1 {
        return Err(native_contract_error(
            "exact replay output must contain exactly one data row",
        ));
    }
    let record = &records[0];
    Ok(
        record.get(lookup_index).map(ascii_trim) == Some(ascii_trim(withheld_surface))
            && record.get(canonical_index) == Some(canonical_id),
    )
}

fn validate_exact_replay_input(
    bytes: &[u8],
    lookup_column: &str,
    withheld_surface: &str,
) -> AliasWithholdingResult<()> {
    let mut reader = csv::Reader::from_reader(bytes);
    let headers = reader
        .headers()
        .map_err(|error| native_contract_error(error.to_string()))?
        .clone();
    let lookup_index = headers
        .iter()
        .position(|header| header == lookup_column)
        .ok_or_else(|| native_contract_error("exact replay input lacks lookup column"))?;
    let records = reader
        .records()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| native_contract_error(error.to_string()))?;
    if records.len() != 1 {
        return Err(native_contract_error(
            "exact replay input must contain exactly one data row",
        ));
    }
    if records[0].get(lookup_index).map(ascii_trim) != Some(ascii_trim(withheld_surface)) {
        return Err(native_contract_error(
            "exact replay input does not contain the withheld alias",
        ));
    }
    Ok(())
}

fn refusal_contract_error(label: &str, refusal: crate::Refusal) -> AliasWithholdingError {
    native_contract_error(format!("{label}: {}", refusal.message))
}

fn native_contract_error(message: impl Into<String>) -> AliasWithholdingError {
    error(AliasWithholdingErrorCode::ArtifactContract, message)
}

fn io_error(path: &Path, error: std::io::Error) -> AliasWithholdingError {
    AliasWithholdingError::new(
        AliasWithholdingErrorCode::ArtifactContract,
        format!("failed to read {}: {error}", path.display()),
    )
}

pub fn canonical_benchmark_bytes(
    benchmark: &AliasWithholdingBenchmark,
) -> AliasWithholdingResult<Vec<u8>> {
    let benchmark = finalize_benchmark(benchmark.clone())?;
    serde_json::to_vec(&benchmark).map_err(artifact_error)
}

pub fn canonical_report_bytes(report: &AliasWithholdingReport) -> AliasWithholdingResult<Vec<u8>> {
    let mut report = report.clone();
    report
        .trials
        .sort_by(|left, right| left.trial_id.cmp(&right.trial_id));
    report.aggregate.strata.sort();
    serde_json::to_vec(&report).map_err(artifact_error)
}

pub fn alias_withholding_benchmark_digest(
    benchmark: &AliasWithholdingBenchmark,
) -> AliasWithholdingResult<String> {
    Ok(hash_bytes(&canonical_benchmark_bytes(benchmark)?))
}

pub fn alias_withholding_report_digest(
    report: &AliasWithholdingReport,
) -> AliasWithholdingResult<String> {
    let mut report = report.clone();
    report.report_digest.clear();
    Ok(hash_bytes(&canonical_report_bytes(&report)?))
}

pub fn base_registry_snapshot_digest(
    snapshot: &BaseRegistrySnapshot,
) -> AliasWithholdingResult<String> {
    let mut snapshot = snapshot.clone();
    snapshot.content_digest.clear();
    serde_json::to_vec(&snapshot)
        .map(|bytes| hash_bytes(&bytes))
        .map_err(artifact_error)
}

pub fn finalize_benchmark(
    mut benchmark: AliasWithholdingBenchmark,
) -> AliasWithholdingResult<AliasWithholdingBenchmark> {
    if ascii_trim(&benchmark.version).is_empty() {
        benchmark.version = CANON_ALIAS_WITHHOLDING_VERSION.to_string();
    }
    if benchmark.version != CANON_ALIAS_WITHHOLDING_VERSION {
        return Err(error(
            AliasWithholdingErrorCode::ArtifactContract,
            format!(
                "unsupported alias-withholding version {}",
                benchmark.version
            ),
        ));
    }
    benchmark.benchmark_id = normalize_non_empty(benchmark.benchmark_id, "benchmark_id")?;
    benchmark.registry.registry_id =
        normalize_non_empty(benchmark.registry.registry_id, "registry_id")?;
    benchmark.registry.registry_version =
        normalize_non_empty(benchmark.registry.registry_version, "registry_version")?;
    benchmark.policy_digest = normalize_digest(benchmark.policy_digest, "policy_digest")?;
    benchmark.trials = benchmark
        .trials
        .into_iter()
        .map(canonicalize_trial)
        .collect::<AliasWithholdingResult<Vec<_>>>()?;
    benchmark.trials.sort();
    benchmark.trials =
        dedup_or_conflict(benchmark.trials, |trial| trial.trial_id.clone(), "trial")?;
    if benchmark.trials.is_empty() {
        return Err(error(
            AliasWithholdingErrorCode::ArtifactContract,
            "alias-withholding benchmark must contain at least one trial",
        ));
    }
    Ok(benchmark)
}

fn canonicalize_trial(
    mut trial: AliasWithholdingTrialSpec,
) -> AliasWithholdingResult<AliasWithholdingTrialSpec> {
    trial.trial_id = normalize_non_empty(trial.trial_id, "trial_id")?;
    trial.entity.canonical_id = normalize_non_empty(trial.entity.canonical_id, "canonical_id")?;
    trial.entity.display_name = normalize_non_empty(trial.entity.display_name, "display_name")?;
    trial.entity.aliases = trial
        .entity
        .aliases
        .into_iter()
        .map(normalize_alias_record)
        .collect::<AliasWithholdingResult<Vec<_>>>()?;
    trial.entity.aliases.sort();
    trial.entity.aliases = dedup_or_conflict(
        trial.entity.aliases,
        |alias| alias.alias_id.clone(),
        "alias",
    )?;

    trial.entity.trusted_identifiers = trial
        .entity
        .trusted_identifiers
        .into_iter()
        .map(normalize_trusted_identifier)
        .collect::<AliasWithholdingResult<Vec<_>>>()?;
    trial.entity.trusted_identifiers.sort();
    trial.entity.trusted_identifiers = dedup_or_conflict(
        trial.entity.trusted_identifiers,
        |identifier| identifier.identifier_id.clone(),
        "trusted_identifier",
    )?;

    trial.entity.permissible_context = trial
        .entity
        .permissible_context
        .into_iter()
        .map(normalize_context)
        .collect::<AliasWithholdingResult<Vec<_>>>()?;
    trial.entity.permissible_context.sort();
    trial.entity.permissible_context = dedup_or_conflict(
        trial.entity.permissible_context,
        |context| context.context_id.clone(),
        "permissible_context",
    )?;

    trial.withheld_alias.alias_id =
        normalize_non_empty(trial.withheld_alias.alias_id, "withheld_alias_id")?;
    trial.withheld_alias.observation_id =
        normalize_non_empty(trial.withheld_alias.observation_id, "observation_id")?;
    trial.withheld_alias.surface =
        normalize_non_empty(trial.withheld_alias.surface, "withheld_surface")?;
    trial.retained_alias_ids = normalize_string_vec(trial.retained_alias_ids, "retained_alias_id")?;
    trial.evaluation = normalize_candidate_evaluation(trial.evaluation)?;
    trial.leakage_probes = trial
        .leakage_probes
        .into_iter()
        .map(normalize_leakage_probe)
        .collect::<AliasWithholdingResult<Vec<_>>>()?;
    trial.leakage_probes.sort();
    trial.leakage_probes.dedup();
    Ok(trial)
}

fn normalize_alias_record(mut alias: AliasRecord) -> AliasWithholdingResult<AliasRecord> {
    alias.alias_id = normalize_non_empty(alias.alias_id, "alias_id")?;
    alias.value = normalize_non_empty(alias.value, "alias_value")?;
    Ok(alias)
}

fn normalize_trusted_identifier(
    mut identifier: TrustedIdentifier,
) -> AliasWithholdingResult<TrustedIdentifier> {
    identifier.identifier_id = normalize_non_empty(identifier.identifier_id, "identifier_id")?;
    identifier.namespace_id = normalize_non_empty(identifier.namespace_id, "namespace_id")?;
    identifier.value = normalize_non_empty(identifier.value, "identifier_value")?;
    Ok(identifier)
}

fn normalize_context(
    mut context: PermissibleContext,
) -> AliasWithholdingResult<PermissibleContext> {
    context.context_id = normalize_non_empty(context.context_id, "context_id")?;
    context.context_kind = normalize_non_empty(context.context_kind, "context_kind")?;
    context.value = normalize_non_empty(context.value, "context_value")?;
    Ok(context)
}

fn normalize_candidate_evaluation(
    mut evaluation: CandidateEvaluation,
) -> AliasWithholdingResult<CandidateEvaluation> {
    if evaluation.candidate_rank == Some(0) {
        return Err(error(
            AliasWithholdingErrorCode::ArtifactContract,
            "candidate_rank is 1-based when present",
        ));
    }
    evaluation.candidate_canonical_id = evaluation
        .candidate_canonical_id
        .map(|value| normalize_non_empty(value, "candidate_canonical_id"))
        .transpose()?;
    evaluation.evidence_lanes = evaluation
        .evidence_lanes
        .into_iter()
        .map(normalize_evidence_lane)
        .collect::<AliasWithholdingResult<Vec<_>>>()?;
    evaluation.evidence_lanes.sort();
    evaluation.evidence_lanes = dedup_or_conflict(
        evaluation.evidence_lanes,
        |lane| lane.lane_id.clone(),
        "evidence_lane",
    )?;
    evaluation.promotion_replay = evaluation
        .promotion_replay
        .map(normalize_promotion_replay)
        .transpose()?;
    Ok(evaluation)
}

fn normalize_evidence_lane(
    mut lane: EvidenceLaneReport,
) -> AliasWithholdingResult<EvidenceLaneReport> {
    lane.lane_id = normalize_non_empty(lane.lane_id, "lane_id")?;
    lane.public_evidence_ref =
        normalize_non_empty(lane.public_evidence_ref, "public_evidence_ref")?;
    Ok(lane)
}

fn normalize_promotion_replay(
    mut replay: PromotionReplay,
) -> AliasWithholdingResult<PromotionReplay> {
    replay.promoted_registry_digest =
        normalize_digest(replay.promoted_registry_digest, "promoted_registry_digest")?;
    replay.exact_replay_canonical_id = replay
        .exact_replay_canonical_id
        .map(|value| normalize_non_empty(value, "exact_replay_canonical_id"))
        .transpose()?;
    Ok(replay)
}

fn normalize_leakage_probe(mut probe: LeakageProbe) -> AliasWithholdingResult<LeakageProbe> {
    probe.locator = normalize_non_empty(probe.locator, "leak_locator")?;
    probe.value = normalize_non_empty(probe.value, "leak_value")?;
    Ok(probe)
}

fn eligible_withheld_alias(
    trial: &AliasWithholdingTrialSpec,
) -> AliasWithholdingResult<AliasRecord> {
    let alias = trial
        .entity
        .aliases
        .iter()
        .find(|alias| alias.alias_id == trial.withheld_alias.alias_id)
        .ok_or_else(|| {
            error(
                AliasWithholdingErrorCode::MissingReference,
                format!(
                    "withheld alias {} is missing from {}",
                    trial.withheld_alias.alias_id, trial.trial_id
                ),
            )
        })?;
    if !alias.reviewed || !alias.eligible {
        return Err(error(
            AliasWithholdingErrorCode::IneligibleAlias,
            format!(
                "withheld alias {} is not reviewed and eligible",
                trial.withheld_alias.alias_id
            ),
        ));
    }
    if alias.alias_class != trial.withheld_alias.alias_class {
        return Err(error(
            AliasWithholdingErrorCode::ArtifactContract,
            format!(
                "withheld alias class mismatch for {}",
                trial.withheld_alias.alias_id
            ),
        ));
    }
    if ascii_trim(&alias.value) != ascii_trim(&trial.withheld_alias.surface) {
        return Err(error(
            AliasWithholdingErrorCode::ArtifactContract,
            format!(
                "withheld surface for {} must equal the reviewed alias value",
                trial.withheld_alias.alias_id
            ),
        ));
    }
    Ok(alias.clone())
}

fn refuse_source_copy_leaks(
    trial: &AliasWithholdingTrialSpec,
    withheld_surface: &str,
) -> AliasWithholdingResult<()> {
    let fingerprint = hash_bytes(ascii_trim(withheld_surface).as_bytes());
    if leaked_value(&trial.entity.display_name, withheld_surface, &fingerprint) {
        return Err(leak_error(
            LeakChannel::DisplayNameCopy,
            "entity.display_name",
            &trial.trial_id,
        ));
    }
    for alias in &trial.entity.aliases {
        if alias.alias_id != trial.withheld_alias.alias_id
            && leaked_value(&alias.value, withheld_surface, &fingerprint)
        {
            return Err(leak_error(
                LeakChannel::MappingFile,
                &format!("alias:{}", alias.alias_id),
                &trial.trial_id,
            ));
        }
    }
    for identifier in &trial.entity.trusted_identifiers {
        if leaked_value(&identifier.value, withheld_surface, &fingerprint) {
            return Err(leak_error(
                LeakChannel::MappingFile,
                &format!("identifier:{}", identifier.identifier_id),
                &trial.trial_id,
            ));
        }
    }
    for context in &trial.entity.permissible_context {
        if leaked_value(&context.value, withheld_surface, &fingerprint) {
            return Err(leak_error(
                LeakChannel::GeneratedCorpus,
                &format!("context:{}", context.context_id),
                &trial.trial_id,
            ));
        }
    }
    Ok(())
}

fn refuse_side_channel_leaks(
    trial: &AliasWithholdingTrialSpec,
    withheld_surface: &str,
) -> AliasWithholdingResult<()> {
    let fingerprint = hash_bytes(ascii_trim(withheld_surface).as_bytes());
    for probe in &trial.leakage_probes {
        if leaked_value(&probe.value, withheld_surface, &fingerprint) {
            return Err(leak_error(probe.channel, &probe.locator, &trial.trial_id));
        }
    }
    Ok(())
}

fn outcome_for_trial(
    trial: &AliasWithholdingTrialSpec,
    withheld_alias: &AliasRecord,
) -> AliasWithholdingResult<TrialOutcome> {
    if trial.evaluation.decision == EntityEngineDecision::Attach
        && !trial
            .withheld_alias
            .relation_policy
            .identity_credit_allowed()
    {
        return Ok(TrialOutcome::UnsupportedGuess);
    }

    match trial.evaluation.decision {
        EntityEngineDecision::Attach => {
            if trial.evaluation.candidate_rank.is_none() {
                return Ok(TrialOutcome::CandidateMiss);
            }
            if trial.evaluation.candidate_canonical_id.as_deref()
                != Some(trial.entity.canonical_id.as_str())
            {
                return Ok(TrialOutcome::CandidateMiss);
            }
            match &trial.evaluation.promotion_replay {
                Some(replay) if replay.approved => {
                    if replay.exact_replay_canonical_id.as_deref()
                        == Some(trial.entity.canonical_id.as_str())
                    {
                        Ok(TrialOutcome::CorrectAttachment)
                    } else {
                        Ok(TrialOutcome::ReplayMismatch)
                    }
                }
                _ => Err(error(
                    AliasWithholdingErrorCode::ReplayMismatch,
                    format!(
                        "approved promotion replay is required for attached withheld alias {}",
                        withheld_alias.alias_id
                    ),
                )),
            }
        }
        EntityEngineDecision::Abstain => Ok(TrialOutcome::CorrectAbstention),
        EntityEngineDecision::Reject => Ok(TrialOutcome::CorrectReject),
    }
}

fn aggregate_reports(reports: &[AliasWithholdingTrialReport]) -> AliasWithholdingAggregate {
    let mut strata =
        BTreeMap::<(AliasClass, RelationPolicy), AliasWithholdingStratumSummary>::new();
    for report in reports {
        let entry = strata
            .entry((report.alias_class, report.relation_policy))
            .or_insert_with(|| AliasWithholdingStratumSummary {
                alias_class: report.alias_class,
                relation_policy: report.relation_policy,
                trial_count: 0,
                credited_attachment_count: 0,
                abstain_count: 0,
                reject_count: 0,
                unsupported_guess_count: 0,
            });
        entry.trial_count += 1;
        if report.credited_attachment {
            entry.credited_attachment_count += 1;
        }
        match report.decision {
            EntityEngineDecision::Attach => {}
            EntityEngineDecision::Abstain => entry.abstain_count += 1,
            EntityEngineDecision::Reject => entry.reject_count += 1,
        }
        if report.outcome == TrialOutcome::UnsupportedGuess {
            entry.unsupported_guess_count += 1;
        }
    }

    let credited_attachment_count = reports
        .iter()
        .filter(|report| report.credited_attachment)
        .count();
    let abstain_count = reports
        .iter()
        .filter(|report| report.decision == EntityEngineDecision::Abstain)
        .count();
    let reject_count = reports
        .iter()
        .filter(|report| report.decision == EntityEngineDecision::Reject)
        .count();
    let unsupported_guess_count = reports
        .iter()
        .filter(|report| report.outcome == TrialOutcome::UnsupportedGuess)
        .count();

    AliasWithholdingAggregate {
        trial_count: reports.len(),
        clean_base_snapshot_count: reports.len(),
        credited_attachment_count,
        abstain_count,
        reject_count,
        unsupported_guess_count,
        strata: strata.into_values().collect(),
    }
}

fn normalize_string_vec(values: Vec<String>, field: &str) -> AliasWithholdingResult<Vec<String>> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalize_non_empty(value, field))
        .collect::<AliasWithholdingResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn normalize_digest(value: String, field: &str) -> AliasWithholdingResult<String> {
    let value = normalize_non_empty(value, field)?;
    if is_blake3_digest(&value) {
        Ok(value)
    } else {
        Err(error(
            AliasWithholdingErrorCode::ArtifactContract,
            format!("{field} must be a lowercase blake3 digest"),
        ))
    }
}

fn normalize_non_empty(value: String, field: &str) -> AliasWithholdingResult<String> {
    let trimmed = ascii_trim(&value).to_string();
    if trimmed.is_empty() {
        Err(error(
            AliasWithholdingErrorCode::ArtifactContract,
            format!("{field} must not be empty"),
        ))
    } else {
        Ok(trimmed)
    }
}

fn leaked_value(value: &str, withheld_surface: &str, fingerprint: &str) -> bool {
    ascii_trim(value) == ascii_trim(withheld_surface) || ascii_trim(value) == fingerprint
}

fn is_blake3_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn hash_serialized<T: Serialize>(value: &T) -> AliasWithholdingResult<String> {
    serde_json::to_vec(value)
        .map(|bytes| hash_bytes(&bytes))
        .map_err(artifact_error)
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn ascii_trim(value: &str) -> &str {
    value.trim_matches(|character: char| character.is_ascii_whitespace())
}

fn dedup_or_conflict<T, K>(
    values: Vec<T>,
    key: impl Fn(&T) -> K,
    label: &str,
) -> AliasWithholdingResult<Vec<T>>
where
    T: Clone + PartialEq,
    K: Ord + fmt::Debug,
{
    let mut deduped = Vec::with_capacity(values.len());
    for value in values {
        if let Some(previous) = deduped.iter().find(|previous| key(previous) == key(&value)) {
            if previous != &value {
                return Err(error(
                    AliasWithholdingErrorCode::DuplicateRecord,
                    format!(
                        "duplicate {label} {:?} has conflicting content",
                        key(&value)
                    ),
                ));
            }
            continue;
        }
        deduped.push(value);
    }
    Ok(deduped)
}

fn leak_error(channel: LeakChannel, locator: &str, trial_id: &str) -> AliasWithholdingError {
    error(
        AliasWithholdingErrorCode::SideChannelLeak,
        format!(
            "{} leaked withheld alias in {} at {}",
            channel.as_str(),
            trial_id,
            locator
        ),
    )
}

fn artifact_error(error: serde_json::Error) -> AliasWithholdingError {
    AliasWithholdingError::new(
        AliasWithholdingErrorCode::ArtifactContract,
        error.to_string(),
    )
}

fn error(code: AliasWithholdingErrorCode, message: impl Into<String>) -> AliasWithholdingError {
    AliasWithholdingError::new(code, message)
}
