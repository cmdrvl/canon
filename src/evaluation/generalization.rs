#![forbid(unsafe_code)]

//! Entity-disjoint and time-forward discovery benchmark contract.
//!
//! This module validates sealed benchmark inputs and compiles deterministic
//! reports for two generalization families:
//! - entity-disjoint trials, where each canonical entity appears exclusively in
//!   tune or holdout observations;
//! - time-forward trials, where build inputs are strictly before the cutoff and
//!   evaluation inputs are strictly after it.
//!
//! Core logic is intentionally domain-neutral. It tracks opaque observation and
//! entity identifiers, rejects holdout/future leakage, emits quality-gated
//! reports that block release claims for severity-critical false merges, and
//! reports stratified results without interpreting domain facts.

use crate::{
    InputFormat, InputValues, Mapping,
    entity::{
        CANON_ENTITY_BLOCK_BUCKET_VERSION, CANON_ENTITY_BLOCK_VERSION_V1,
        CANON_ENTITY_EDGE_VERSION, CANON_ENTITY_EVIDENCE_VERSION_V1, CANON_ENTITY_INDEX_VERSION_V1,
        CANON_ENTITY_PREPARE_VERSION_V1, CANON_ENTITY_RUN_VERSION_V1,
        CANON_ENTITY_SOLVE_VERSION_V1, EntityArtifactMetadata, EntityArtifactReference,
        EntityArtifactStageV1, EntityStrategyReference,
        block::{
            BlockCandidateGenerationDiagnostics, BlockCandidateRecord,
            CandidateRecallEvaluationRequest, evaluate_candidate_recall,
        },
        block_artifact::{
            BlockCandidateArtifact, ExactBucketAssertion,
            validate_block_candidate_artifact_contract, validate_block_candidate_payload_hashes,
        },
        edge::EdgeEvidenceRecord,
        edge_artifact::{
            EdgeEvidenceArtifact, EdgeEvidenceArtifactRequest,
            build_edge_evidence_artifact_contract, validate_edge_evidence_artifact_contract,
        },
        graph::{SignedEvidenceGraphInput, SurfaceIncumbentId, build_signed_evidence_graph},
        index::{DEFAULT_INDEX_DIAGNOSTICS_PATH, EntityIndexCacheMode, EntityIndexCacheStatus},
        index_io::{
            CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION, EntityIndexCacheReceipt,
            INDEX_CACHE_KEY_FILE, INDEX_CACHE_RECEIPT_FILE,
        },
        prepare::{PreparedExactLookupStatus, PreparedSurfaceRecord},
        run::{
            EntityRunArtifact, RUN_CACHE_EXECUTION_RECEIPT_PATH,
            link::{
                ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION,
                ENTITY_LINK_VERSION as CANON_ENTITY_LINK_VERSION, EntityLinkArtifact,
                EntityLinkObservationSurfaceBinding, EntityLinkRole,
                read_derivation_validated_entity_link_observation_surface_bindings_at_path,
                validate_entity_link_artifact_contract,
            },
        },
        schema::{validate_artifact_v1_core_contract, validate_entity_v1_self_hash},
        solve::{
            SolveArtifact, SolveArtifactRequest, SolveEntityRecord, SolveReconciliationConfig,
            SolveReconciliationState, SolveSurfaceProvenance, build_solve_artifact_contract,
            validate_solve_artifact_contract,
        },
        surface_id::{SurfaceIdMaterial, derive_surface_ids},
        telemetry::{
            CANON_ENTITY_CANDIDATE_RECALL_VERSION, CandidateRecallGoldPair,
            CandidateRecallRankRecord, CandidateRecallStratum, EntityCandidateRecallReport,
        },
    },
    fs_safety::{PathResolution, PlannedAccess, resolve_workspace_path},
    lookup, registry,
};
use chrono::{DateTime, NaiveDate, SecondsFormat};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    error::Error,
    fmt, fs, io,
    path::{Component, Path, PathBuf},
};

pub const CANON_GENERALIZATION_VERSION: &str = "canon.evaluation.generalization.v1";
pub const CANON_GENERALIZATION_EXECUTION_ENVELOPE_VERSION: &str =
    "canon.evaluation.generalization.strict_execution_envelope.v0";
pub const CANON_GENERALIZATION_LEAK_SCAN_SOURCES_VERSION: &str =
    "canon.evaluation.generalization.leak_scan_sources.v0";
pub const CANON_GENERALIZATION_CANDIDATE_RECALL_QUALITY_MANIFEST_VERSION: &str =
    "canon.evaluation.generalization.candidate_recall_quality_manifest.v0";
pub const CANON_GENERALIZATION_SOLVE_POLICY_VERSION: &str =
    "canon.evaluation.generalization.solve_policy.v0";
pub const CANON_GENERALIZATION_CACHE_EXECUTION_VERSION: &str =
    "canon.evaluation.generalization.cache_execution.v0";

pub type GeneralizationResult<T> = Result<T, GeneralizationError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationErrorCode {
    ArtifactContract,
    MissingReference,
    DuplicateRecord,
    EntityDisjointLeak,
    FutureLeakage,
    TemporalReversal,
    CriticalFalseMerge,
    DirectionalLinkContract,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneralizationError {
    pub code: GeneralizationErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TemporalInstant {
    seconds: i64,
    nanos: u32,
}

impl GeneralizationError {
    pub fn new(code: GeneralizationErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for GeneralizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeneralizationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusVisibility {
    PublicFixture,
    PrivateCorpusRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkPartition {
    Tune,
    Holdout,
    Build,
    Evaluation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFamily {
    Reference,
    Target,
    PublicFixture,
    PrivateReference,
    PrivateTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetRole {
    Reference,
    Target,
    SingleSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceAvailability {
    NameOnly,
    SurfaceAndIdentifier,
    RichEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameDifficulty {
    Easy,
    PunctuationCase,
    LegalSuffix,
    Rename,
    Lookalike,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityFrequency {
    Head,
    Tail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationClass {
    SameEntity,
    RelatedDistinct,
    Hierarchy,
    Lookalike,
    RenameContinuity,
    ChangedRelationship,
    NewEntity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DifficultyBand {
    Easy,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryDecision {
    AttachExisting,
    ClusterNew,
    Abstain,
    Reject,
    FalseMerge,
}

impl DiscoveryDecision {
    const fn is_abstention(self) -> bool {
        matches!(self, Self::Abstain)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAction {
    PromoteCluster,
    PromoteLink,
    DeferReview,
    RejectCandidate,
    RecordCannotLink,
    NoAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeakChannel {
    Alias,
    Anchor,
    Threshold,
    Dictionary,
    Patch,
    Cache,
    GeneratedCorpus,
}

impl LeakChannel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Alias => "alias",
            Self::Anchor => "anchor",
            Self::Threshold => "threshold",
            Self::Dictionary => "dictionary",
            Self::Patch => "patch",
            Self::Cache => "cache",
            Self::GeneratedCorpus => "generated_corpus",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtectedSet {
    HoldoutEntity,
    FutureObservation,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeneralizationObservation {
    pub observation_id: String,
    pub canonical_entity_id: String,
    pub dataset_id: String,
    pub dataset_role: DatasetRole,
    pub partition: BenchmarkPartition,
    pub observed_at: String,
    pub surface: String,
    pub evidence_availability: EvidenceAvailability,
    pub source_family: SourceFamily,
    pub name_difficulty: NameDifficulty,
    pub entity_frequency: EntityFrequency,
    pub relation_class: RelationClass,
    pub difficulty_band: DifficultyBand,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EvidenceLaneSummary {
    pub lane_id: String,
    pub available: bool,
    pub support_basis_points: u16,
    pub contradiction_basis_points: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DiscoveryResultRecord {
    pub result_id: String,
    pub observation_ids: Vec<String>,
    pub expected_decision: DiscoveryDecision,
    pub actual_decision: DiscoveryDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_rank: Option<u32>,
    pub evidence_lanes: Vec<EvidenceLaneSummary>,
    pub review_action: ReviewAction,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HardNegativeControl {
    pub control_id: String,
    pub left_observation_id: String,
    pub right_observation_id: String,
    pub relation_class: RelationClass,
    pub severity: Severity,
    pub false_merge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DirectionalCrossSourceLink {
    pub link_id: String,
    pub reference_observation_id: String,
    pub target_observation_id: String,
    pub reference_dataset_id: String,
    pub target_dataset_id: String,
    pub expected_decision: DiscoveryDecision,
    pub actual_decision: DiscoveryDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_rank: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LeakageProbe {
    pub channel: LeakChannel,
    pub protected_set: ProtectedSet,
    pub locator: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityDisjointTrial {
    pub trial_id: String,
    pub observations: Vec<GeneralizationObservation>,
    pub discovery_results: Vec<DiscoveryResultRecord>,
    pub hard_negatives: Vec<HardNegativeControl>,
    pub directional_links: Vec<DirectionalCrossSourceLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leakage_probes: Vec<LeakageProbe>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TimeForwardTrial {
    pub trial_id: String,
    pub cutoff: String,
    pub observations: Vec<GeneralizationObservation>,
    pub build_observation_ids: Vec<String>,
    pub evaluation_observation_ids: Vec<String>,
    pub event_results: Vec<DiscoveryResultRecord>,
    pub hard_negatives: Vec<HardNegativeControl>,
    pub directional_links: Vec<DirectionalCrossSourceLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leakage_probes: Vec<LeakageProbe>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneralizationBenchmark {
    pub version: String,
    pub benchmark_id: String,
    pub corpus_visibility: CorpusVisibility,
    pub corpus_ref: String,
    pub policy_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_disjoint_trials: Vec<EntityDisjointTrial>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub time_forward_trials: Vec<TimeForwardTrial>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeneralizationStratumKey {
    pub evidence_availability: EvidenceAvailability,
    pub source_family: SourceFamily,
    pub name_difficulty: NameDifficulty,
    pub entity_frequency: EntityFrequency,
    pub relation_class: RelationClass,
    pub difficulty_band: DifficultyBand,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeneralizationStratumReport {
    pub key: GeneralizationStratumKey,
    pub result_count: usize,
    pub correct_count: usize,
    pub abstain_count: usize,
    pub false_merge_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityDisjointTrialReport {
    pub trial_id: String,
    pub clean_snapshot_digest: String,
    pub protected_holdout_digest: String,
    pub novel_cluster_result_count: usize,
    pub correct_novel_cluster_count: usize,
    pub related_distinct_hard_negative_count: usize,
    pub critical_false_merge_count: usize,
    pub directional_cross_source_count: usize,
    pub strata: Vec<GeneralizationStratumReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeForwardTrialReport {
    pub trial_id: String,
    pub cutoff: String,
    pub build_snapshot_digest: String,
    pub protected_future_digest: String,
    pub evaluation_result_count: usize,
    pub correct_evaluation_count: usize,
    pub renamed_surface_count: usize,
    pub new_entity_count: usize,
    pub changed_relationship_count: usize,
    pub critical_false_merge_count: usize,
    pub directional_cross_source_count: usize,
    pub strata: Vec<GeneralizationStratumReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneralizationAggregate {
    pub entity_disjoint_trial_count: usize,
    pub time_forward_trial_count: usize,
    pub result_count: usize,
    pub correct_count: usize,
    pub abstain_count: usize,
    pub critical_false_merge_count: usize,
    pub directional_cross_source_count: usize,
    pub head_result_count: usize,
    pub tail_result_count: usize,
    pub easy_result_count: usize,
    pub hard_result_count: usize,
    pub strata: Vec<GeneralizationStratumReport>,
}

pub const CANON_GENERALIZATION_QUALITY_GATE_REPORT_VERSION: &str =
    "canon.evaluation.generalization.quality_gate_report.v0";
pub const CANON_ENTITY_QUALITY_VERSION: &str = "canon.entity.quality.v1";
const QUALITY_GATE_CANDIDATE_RECALL_AT_50_MIN: f64 = 0.995;
const QUALITY_GATE_AUTO_LINK_PRECISION_MIN: f64 = 0.995;
const QUALITY_GATE_AUTO_LINK_RECALL_MIN: f64 = 0.98;
const QUALITY_GATE_CRITICAL_FALSE_MERGES_MAX: f64 = 0.0;
const QUALITY_GATE_ACCOUNTED_CASE_RATE_MIN: f64 = 1.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneralizationReport {
    pub version: String,
    pub benchmark_id: String,
    pub corpus_visibility: CorpusVisibility,
    pub corpus_ref: String,
    pub benchmark_digest: String,
    pub report_digest: String,
    pub entity_disjoint: Vec<EntityDisjointTrialReport>,
    pub time_forward: Vec<TimeForwardTrialReport>,
    pub aggregate: GeneralizationAggregate,
    pub quality: GeneralizationQualityContractReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derivation: Option<GeneralizationDerivationReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationReleaseClaimStatus {
    Eligible,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationQualityGateStatus {
    Pass,
    Fail,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneralizationQualityGateReport {
    pub gate_id: String,
    pub metric_id: String,
    pub status: GeneralizationQualityGateStatus,
    pub observed_value: Option<f64>,
    pub operator: String,
    pub threshold: f64,
    pub waiver_bead_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneralizationQualityContractReport {
    pub version: String,
    pub contract_version: String,
    pub release_claim_status: GeneralizationReleaseClaimStatus,
    pub gates: Vec<GeneralizationQualityGateReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneralizationDerivationReceipt {
    pub source: GeneralizationDerivationSource,
    pub self_attested_outcomes_used: bool,
    pub manifest_hash: String,
    pub benchmark_hash: String,
    pub artifact_hashes: Vec<GeneralizationDerivationArtifactHash>,
    pub leak_source_hashes: Vec<GeneralizationDerivationLeakSourceHash>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationDerivationSource {
    StrictExecutionEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeneralizationDerivationArtifactHash {
    pub artifact_id: String,
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeneralizationDerivationLeakSourceHash {
    pub source_id: String,
    pub phase: GeneralizationLeakSourcePhase,
    pub content_hash: String,
    pub bundle_content_hash: String,
    pub binding_kind: GeneralizationLeakSourceBindingKind,
    pub binding_hash: String,
    pub checked_source_hashes: Vec<String>,
    pub checked_channels: Vec<LeakChannel>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationArtifactKind {
    CandidateRecall,
    Link,
    #[serde(alias = "observation_surface_bindings")]
    LinkObservationSurfaceBindings,
    Run,
    Solve,
    LeakScanSources,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationBenchmarkRef {
    pub path: String,
    pub content_hash: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationArtifactRef {
    pub path: String,
    pub content_hash: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<GeneralizationArtifactKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationExecutionContract {
    pub path_resolver: String,
    pub required_refusals: Vec<GeneralizationRequiredRefusal>,
    pub derive_observations: bool,
    pub derive_candidate_ranks: bool,
    pub derive_evidence_lanes: bool,
    pub derive_hard_negative_outcomes: bool,
    pub recompute_leakage: bool,
    pub self_attested_outcomes_used: bool,
    pub canonical_time_parsing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_artifact_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_artifact_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationRequiredRefusal {
    Traversal,
    Symlink,
    Missing,
    StaleHash,
    VersionMismatch,
    NoncanonicalArtifact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationTrialFamily {
    EntityDisjoint,
    TimeForward,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationCrossBindings {
    pub benchmark_id: String,
    pub run_id: String,
    pub policy_digest: String,
    pub registry_id: String,
    pub registry_version: String,
    pub registry_snapshot_hash: String,
    pub observation_namespace: String,
    pub required_identity_links: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationTypedArtifactRef {
    pub path: String,
    pub content_hash: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationCandidateRecallExecutionRefs {
    pub quality_manifest: GeneralizationTypedArtifactRef,
    pub block_artifact: GeneralizationTypedArtifactRef,
    pub candidates: GeneralizationTypedArtifactRef,
    pub diagnostics: GeneralizationTypedArtifactRef,
    pub exact_bucket_assertions: GeneralizationTypedArtifactRef,
    pub report: GeneralizationTypedArtifactRef,
    pub exact_bucket_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationCacheExecutionMode {
    DisabledBypass,
    EnabledWarmHit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationCacheExecutionRef {
    pub version: String,
    pub mode: GeneralizationCacheExecutionMode,
    pub receipt: GeneralizationTypedArtifactRef,
    pub bundle_receipt: GeneralizationTypedArtifactRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationExecutionBindings {
    pub observation_bindings: Vec<GeneralizationObservationBinding>,
    pub result_bindings: Vec<GeneralizationResultBinding>,
    pub directional_link_bindings: Vec<GeneralizationDirectionalLinkBinding>,
    pub hard_negative_bindings: Vec<GeneralizationHardNegativeBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationObservationBinding {
    pub trial_id: String,
    pub observation_id: String,
    pub surface_id: String,
    pub surface_binding_hash: String,
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<EntityLinkRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_row_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationResultBinding {
    pub trial_id: String,
    pub result_id: String,
    pub observation_ids: Vec<String>,
    pub surface_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_gold_pair_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_pair_observation_ids: Option<Vec<String>>,
    pub solve_disposition: GeneralizationSolveDisposition,
    pub expected_decision: DiscoveryDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeneralizationSolveDisposition {
    Present {
        component_id: String,
        state: SolveReconciliationState,
    },
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationLinkDisposition {
    Matched,
    Ambiguous,
    Unmatched,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationDirectionalLinkBinding {
    pub trial_id: String,
    pub directional_link_id: String,
    pub gold_pair_id: String,
    pub reference_observation_id: String,
    pub target_observation_id: String,
    pub reference_surface_id: String,
    pub target_surface_id: String,
    pub solve_disposition: GeneralizationSolveDisposition,
    pub expected_decision: DiscoveryDecision,
    pub link_disposition: GeneralizationLinkDisposition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationHardNegativeBinding {
    pub trial_id: String,
    pub control_id: String,
    pub left_observation_id: String,
    pub right_observation_id: String,
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub left_solve_disposition: GeneralizationSolveDisposition,
    pub right_solve_disposition: GeneralizationSolveDisposition,
    pub expected_false_merge: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_disposition: Option<GeneralizationLinkDisposition>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationLeakSourcePhase {
    BuildInfluence,
    TuneInfluence,
    PreEvaluationInfluence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationLeakSourceKind {
    RegistryTree,
    RegistryAliasFile,
    RegistryAnchorFile,
    Threshold,
    Dictionary,
    Patch,
    Cache,
    GeneratedCorpus,
    LinkMaterializedRows,
    LinkObservationSurfaceBindings,
    CandidateRecall,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationLeakSourceFormat {
    Json,
    Jsonl,
    Csv,
    Text,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationLeakSourceBindingKind {
    RegistrySnapshot,
    RegistrySidecarSnapshot,
    Profile,
    Strategy,
    Input,
    PatchSet,
    Namekit,
    CandidateRecallQualityManifest,
    CandidateRecallBlockArtifact,
    CandidateRecallCandidates,
    CandidateRecallDiagnostics,
    CandidateRecallExactBucketAssertions,
    CandidateRecallReport,
    ArtifactRef,
    RunArtifact,
    RunStageArtifact,
    RunStageUpstreamArtifact,
    LinkArtifact,
    LinkMaterializedRows,
    LinkObservationSurfaceBindings,
    SolveArtifact,
    SolveUpstreamArtifact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationLeakSourceCoverage {
    CompleteRegistryTree,
    CompleteRelevantSubtree,
    CompleteSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationExecutionEnvelope {
    pub version: String,
    pub benchmark: GeneralizationBenchmarkRef,
    pub execution: GeneralizationExecutionContract,
    pub trials: Vec<GeneralizationTrialExecution>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationTrialExecution {
    pub trial_id: String,
    pub family: GeneralizationTrialFamily,
    pub registry_dir: String,
    pub candidate_recall: GeneralizationCandidateRecallExecutionRefs,
    pub solve_derivation: GeneralizationSolveDerivationRefs,
    pub cache_execution: GeneralizationCacheExecutionRef,
    pub artifacts: Vec<GeneralizationArtifactRef>,
    pub cross_bindings: GeneralizationCrossBindings,
    pub bindings: GeneralizationExecutionBindings,
    pub leak_scan_sources: GeneralizationLeakSourceBundleRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationSolveDerivationRefs {
    pub edge_artifact: GeneralizationTypedArtifactRef,
    pub edge_records: GeneralizationTypedArtifactRef,
    pub prepared_surfaces: GeneralizationTypedArtifactRef,
    pub solve_policy: GeneralizationTypedArtifactRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationLeakSourceBundleRef {
    pub version: String,
    pub phase: GeneralizationLeakSourcePhase,
    pub channels: Vec<LeakChannel>,
    pub path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationLeakSourceBundle {
    pub version: String,
    pub scope: String,
    pub channels: Vec<LeakChannel>,
    pub sources: Vec<GeneralizationStructuredLeakSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationStructuredLeakSource {
    pub source_id: String,
    pub phase: GeneralizationLeakSourcePhase,
    pub channel: LeakChannel,
    pub source_kind: GeneralizationLeakSourceKind,
    pub binding_kind: GeneralizationLeakSourceBindingKind,
    pub binding_hash: String,
    pub coverage: GeneralizationLeakSourceCoverage,
    pub content_hash: String,
    pub content_hash_basis: String,
    pub protected_match_derivation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completeness_manifest: Option<GeneralizationLeakSourceCompletenessManifestRef>,
    pub checked_sources: Vec<GeneralizationCheckedLeakSource>,
    pub records: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationCheckedLeakSource {
    pub path: String,
    pub format: GeneralizationLeakSourceFormat,
    pub content_hash: String,
    pub byte_count: u64,
    pub record_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationLeakSourceCompletenessManifestRef {
    pub path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationLeakSourceCompletenessManifest {
    pub version: String,
    pub coverage: GeneralizationLeakSourceCoverage,
    pub root: String,
    pub entries: Vec<GeneralizationLeakSourceCompletenessEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralizationLeakSourceCompletenessEntry {
    pub path: String,
    pub format: GeneralizationLeakSourceFormat,
    pub content_hash: String,
    pub byte_count: u64,
    pub record_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LoadedGeneralizationArtifact {
    CandidateRecall(EntityCandidateRecallReport),
    Link(EntityLinkArtifact),
    LinkObservationSurfaceBindings(Vec<EntityLinkObservationSurfaceBinding>),
    Run(EntityRunArtifact),
    Solve(SolveArtifact),
    LeakScanSources(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedGeneralizationArtifactRef {
    pub reference: GeneralizationArtifactRef,
    pub artifact: LoadedGeneralizationArtifact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedGeneralizationExecutionEnvelope {
    pub manifest_content_hash: String,
    pub envelope: GeneralizationExecutionEnvelope,
    pub benchmark_content_hash: String,
    pub benchmark: GeneralizationBenchmark,
    pub trials: Vec<LoadedGeneralizationTrialExecution>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedGeneralizationTrialExecution {
    pub execution: GeneralizationTrialExecution,
    pub candidate_recall: LoadedGeneralizationCandidateRecall,
    pub solve_derivation: LoadedGeneralizationSolveDerivation,
    pub cache_execution: LoadedGeneralizationCacheExecution,
    pub artifacts: Vec<LoadedGeneralizationArtifactRef>,
    pub leak_scan_sources: Vec<LoadedGeneralizationLeakSourceRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedGeneralizationCandidateRecall {
    pub references: GeneralizationCandidateRecallExecutionRefs,
    pub quality_manifest_hash: String,
    pub block_artifact_hash: String,
    pub candidate_records_hash: String,
    pub diagnostics_hash: String,
    pub exact_bucket_assertions_hash: String,
    pub report_hash: String,
    pub surface_ids: Vec<String>,
    pub gold_pairs: Vec<CandidateRecallGoldPair>,
    pub block_artifact: BlockCandidateArtifact,
    pub candidate_records: Vec<BlockCandidateRecord>,
    pub diagnostics: BlockCandidateGenerationDiagnostics,
    pub exact_bucket_assertions: Vec<ExactBucketAssertion>,
    pub report: EntityCandidateRecallReport,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedGeneralizationSolveDerivation {
    pub references: GeneralizationSolveDerivationRefs,
    pub edge_artifact_hash: String,
    pub edge_records_hash: String,
    pub prepared_surfaces_hash: String,
    pub solve_policy_hash: String,
    pub edge_artifact: EdgeEvidenceArtifact,
    pub edge_records: Vec<EdgeEvidenceRecord>,
    pub prepared_surfaces: Vec<PreparedSurfaceRecord>,
    pub solve_config: SolveReconciliationConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedGeneralizationCacheExecution {
    pub references: GeneralizationCacheExecutionRef,
    pub receipt_path: String,
    pub receipt_hash: String,
    pub receipt_byte_count: u64,
    pub receipt: EntityIndexCacheReceipt,
    pub bundle_receipt_path: String,
    pub bundle_receipt_hash: String,
    pub bundle_receipt_byte_count: u64,
    pub bundle_receipt: EntityIndexCacheReceipt,
    pub bundle_files: Vec<LoadedGeneralizationCacheReceiptFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedGeneralizationCacheReceiptFile {
    pub role: String,
    pub path: String,
    pub content_hash: String,
    pub byte_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_artifact_content_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedGeneralizationLeakSourceRef {
    pub source_id: String,
    pub phase: GeneralizationLeakSourcePhase,
    pub channel: LeakChannel,
    pub source_kind: GeneralizationLeakSourceKind,
    pub binding_kind: GeneralizationLeakSourceBindingKind,
    pub binding_hash: String,
    pub coverage: GeneralizationLeakSourceCoverage,
    pub content_hash: String,
    pub bundle_content_hash: String,
    #[serde(skip_serializing, skip_deserializing)]
    pub checked_sources: Vec<LoadedGeneralizationCheckedLeakSourceRef>,
    #[serde(skip_serializing, skip_deserializing)]
    pub bytes: Vec<u8>,
    #[serde(skip_serializing, skip_deserializing)]
    pub decoded_strings: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedGeneralizationCheckedLeakSourceRef {
    pub path: String,
    pub format: GeneralizationLeakSourceFormat,
    pub content_hash: String,
    pub byte_count: u64,
    pub record_count: u64,
}

pub fn generalization_schema_version() -> &'static str {
    CANON_GENERALIZATION_VERSION
}

pub fn load_generalization_execution_envelope_manifest(
    manifest_path: impl AsRef<Path>,
) -> GeneralizationResult<LoadedGeneralizationExecutionEnvelope> {
    let manifest_path = manifest_path.as_ref();
    let manifest_file = manifest_path.file_name().ok_or_else(|| {
        error(
            GeneralizationErrorCode::ArtifactContract,
            "manifest path must name a concrete file",
        )
    })?;
    let manifest_file = manifest_file.to_str().ok_or_else(|| {
        error(
            GeneralizationErrorCode::ArtifactContract,
            "manifest path must be valid UTF-8",
        )
    })?;
    let base_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let (_, manifest_bytes) = read_strict_manifest_file(base_dir, "manifest", manifest_file)?;
    let manifest_content_hash = hash_bytes(&manifest_bytes);
    let envelope: GeneralizationExecutionEnvelope =
        serde_json::from_slice(&manifest_bytes).map_err(artifact_error)?;
    load_generalization_execution_envelope(base_dir, envelope, manifest_content_hash)
}

pub fn load_generalization_execution_envelope(
    base_dir: &Path,
    envelope: GeneralizationExecutionEnvelope,
    manifest_content_hash: String,
) -> GeneralizationResult<LoadedGeneralizationExecutionEnvelope> {
    validate_execution_envelope_contract(&envelope)?;
    let (benchmark, benchmark_content_hash) =
        load_generalization_benchmark_ref(base_dir, &envelope.benchmark)?;
    validate_trial_execution_coverage(&benchmark, &envelope.trials)?;

    let max_artifact_bytes = envelope.execution.max_artifact_bytes;
    let trials = envelope
        .trials
        .iter()
        .map(|trial| {
            load_generalization_trial_execution(
                base_dir,
                &envelope.benchmark,
                &benchmark_content_hash,
                &benchmark,
                trial,
                max_artifact_bytes,
            )
        })
        .collect::<GeneralizationResult<Vec<_>>>()?;

    Ok(LoadedGeneralizationExecutionEnvelope {
        manifest_content_hash,
        envelope,
        benchmark_content_hash,
        benchmark,
        trials,
    })
}

fn load_generalization_trial_execution(
    base_dir: &Path,
    benchmark_ref: &GeneralizationBenchmarkRef,
    benchmark_content_hash: &str,
    benchmark: &GeneralizationBenchmark,
    trial: &GeneralizationTrialExecution,
    max_artifact_bytes: Option<u64>,
) -> GeneralizationResult<LoadedGeneralizationTrialExecution> {
    let candidate_recall = load_generalization_candidate_recall_execution(
        base_dir,
        &trial.candidate_recall,
        max_artifact_bytes,
    )?;
    let registry_dir = resolve_strict_manifest_dir(
        base_dir,
        &format!("trials[{}].registry_dir", trial.trial_id),
        &trial.registry_dir,
    )?;
    let solve_derivation =
        load_generalization_solve_derivation(base_dir, trial, max_artifact_bytes)?;
    let artifacts = trial
        .artifacts
        .iter()
        .enumerate()
        .map(|(index, reference)| {
            load_generalization_artifact_ref(
                base_dir,
                &format!("trials[{}].artifacts[{index}].path", trial.trial_id),
                reference,
                max_artifact_bytes,
            )
        })
        .collect::<GeneralizationResult<Vec<_>>>()?;
    let cache_execution =
        load_generalization_cache_execution(base_dir, trial, &artifacts, max_artifact_bytes)?;
    validate_loaded_execution_continuity(
        base_dir,
        benchmark,
        trial,
        &registry_dir,
        &candidate_recall,
        &solve_derivation,
        &artifacts,
    )?;
    let leak_scan_sources = load_generalization_leak_source_bundle_ref(
        base_dir,
        &format!("trials[{}].leak_scan_sources", trial.trial_id),
        &trial.leak_scan_sources,
        max_artifact_bytes,
        benchmark_ref,
        benchmark_content_hash,
        trial,
        &candidate_recall,
        &solve_derivation,
        &cache_execution,
        &artifacts,
    )?;

    Ok(LoadedGeneralizationTrialExecution {
        execution: trial.clone(),
        candidate_recall,
        solve_derivation,
        cache_execution,
        artifacts,
        leak_scan_sources,
    })
}

pub fn compile_strict_generalization_manifest(
    manifest_path: impl AsRef<Path>,
) -> GeneralizationResult<GeneralizationReport> {
    let loaded = load_generalization_execution_envelope_manifest(manifest_path)?;
    compile_loaded_generalization_execution_envelope(loaded)
}

pub fn compile_loaded_generalization_execution_envelope(
    loaded: LoadedGeneralizationExecutionEnvelope,
) -> GeneralizationResult<GeneralizationReport> {
    let contexts = loaded
        .trials
        .iter()
        .map(|trial| {
            Ok((
                trial_execution_key(&trial.execution),
                StrictDerivationContext::new(trial)?,
            ))
        })
        .collect::<GeneralizationResult<BTreeMap<_, _>>>()?;
    let mut benchmark = loaded.benchmark.clone();

    for trial in &mut benchmark.entity_disjoint_trials {
        let key = (
            GeneralizationTrialFamily::EntityDisjoint,
            trial.trial_id.clone(),
        );
        let context = contexts.get(&key).ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!(
                    "missing execution context for entity_disjoint trial {}",
                    trial.trial_id
                ),
            )
        })?;
        context.validate_observation_coverage(&trial.observations)?;
        trial.discovery_results = trial
            .discovery_results
            .iter()
            .map(|result| context.derive_discovery_result(result))
            .collect::<GeneralizationResult<Vec<_>>>()?;
        trial.hard_negatives = trial
            .hard_negatives
            .iter()
            .map(|control| context.derive_hard_negative(control))
            .collect::<GeneralizationResult<Vec<_>>>()?;
        trial.directional_links = trial
            .directional_links
            .iter()
            .map(|link| context.derive_directional_link(link))
            .collect::<GeneralizationResult<Vec<_>>>()?;
        trial.leakage_probes.clear();
    }

    for trial in &mut benchmark.time_forward_trials {
        let key = (
            GeneralizationTrialFamily::TimeForward,
            trial.trial_id.clone(),
        );
        let context = contexts.get(&key).ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!(
                    "missing execution context for time_forward trial {}",
                    trial.trial_id
                ),
            )
        })?;
        context.validate_observation_coverage(&trial.observations)?;
        trial.event_results = trial
            .event_results
            .iter()
            .map(|result| context.derive_discovery_result(result))
            .collect::<GeneralizationResult<Vec<_>>>()?;
        trial.hard_negatives = trial
            .hard_negatives
            .iter()
            .map(|control| context.derive_hard_negative(control))
            .collect::<GeneralizationResult<Vec<_>>>()?;
        trial.directional_links = trial
            .directional_links
            .iter()
            .map(|link| context.derive_directional_link(link))
            .collect::<GeneralizationResult<Vec<_>>>()?;
        trial.leakage_probes.clear();
    }

    recompute_strict_leakage(&loaded, &benchmark)?;
    let receipt = strict_derivation_receipt(&loaded)?;
    let mut report = compile_generalization_benchmark_internal(benchmark)?;
    report.derivation = Some(receipt);
    report.report_digest = generalization_report_digest(&report)?;
    Ok(report)
}

pub fn parse_candidate_recall_artifact(
    bytes: &[u8],
) -> GeneralizationResult<EntityCandidateRecallReport> {
    let artifact: EntityCandidateRecallReport =
        serde_json::from_slice(bytes).map_err(artifact_error)?;
    artifact.validate().map_err(contract_error)?;
    Ok(artifact)
}

pub fn parse_link_artifact(bytes: &[u8]) -> GeneralizationResult<EntityLinkArtifact> {
    let artifact: EntityLinkArtifact = serde_json::from_slice(bytes).map_err(artifact_error)?;
    validate_entity_link_artifact_contract(&artifact).map_err(contract_error)?;
    Ok(artifact)
}

pub fn parse_link_observation_surface_bindings(
    bytes: &[u8],
) -> GeneralizationResult<Vec<EntityLinkObservationSurfaceBinding>> {
    let bindings: Vec<EntityLinkObservationSurfaceBinding> =
        parse_json_or_jsonl(bytes, "link observation/surface bindings")?;
    for binding in &bindings {
        if binding.version != ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                "link observation/surface binding has the wrong version",
            ));
        }
    }
    Ok(bindings)
}

pub fn parse_run_artifact(bytes: &[u8]) -> GeneralizationResult<EntityRunArtifact> {
    let artifact: EntityRunArtifact = serde_json::from_slice(bytes).map_err(artifact_error)?;
    validate_run_artifact_contract(&artifact)?;
    Ok(artifact)
}

pub fn parse_solve_artifact(bytes: &[u8]) -> GeneralizationResult<SolveArtifact> {
    let artifact: SolveArtifact = serde_json::from_slice(bytes).map_err(artifact_error)?;
    validate_solve_artifact_contract(&artifact).map_err(contract_error)?;
    Ok(artifact)
}

fn load_generalization_candidate_recall_execution(
    base_dir: &Path,
    references: &GeneralizationCandidateRecallExecutionRefs,
    max_artifact_bytes: Option<u64>,
) -> GeneralizationResult<LoadedGeneralizationCandidateRecall> {
    validate_candidate_recall_execution_refs(references)?;

    let (_, quality_manifest_bytes) = read_typed_artifact_ref(
        base_dir,
        "candidate_recall.quality_manifest",
        &references.quality_manifest,
        max_artifact_bytes,
    )?;
    let quality_manifest: GeneralizationCandidateRecallQualityManifest =
        serde_json::from_slice(&quality_manifest_bytes).map_err(artifact_error)?;
    validate_candidate_recall_quality_manifest_version(
        &quality_manifest,
        &references.quality_manifest.version,
    )?;
    let (surface_ids, gold_pairs) = candidate_recall_manifest_gold(&quality_manifest)?;

    let (_, block_artifact_bytes) = read_typed_artifact_ref(
        base_dir,
        "candidate_recall.block_artifact",
        &references.block_artifact,
        max_artifact_bytes,
    )?;
    let block_artifact: BlockCandidateArtifact =
        serde_json::from_slice(&block_artifact_bytes).map_err(artifact_error)?;
    validate_block_candidate_artifact_contract(&block_artifact).map_err(contract_error)?;

    let (_, candidate_record_bytes) = read_typed_artifact_ref(
        base_dir,
        "candidate_recall.candidates",
        &references.candidates,
        max_artifact_bytes,
    )?;
    let candidate_records: Vec<BlockCandidateRecord> =
        parse_json_or_jsonl(&candidate_record_bytes, "candidate records")?;
    validate_candidate_record_versions(&candidate_records, &references.candidates.version)?;

    let (_, diagnostics_bytes) = read_typed_artifact_ref(
        base_dir,
        "candidate_recall.diagnostics",
        &references.diagnostics,
        max_artifact_bytes,
    )?;
    let diagnostics: BlockCandidateGenerationDiagnostics =
        serde_json::from_slice(&diagnostics_bytes).map_err(artifact_error)?;

    let (_, exact_bucket_bytes) = read_typed_artifact_ref(
        base_dir,
        "candidate_recall.exact_bucket_assertions",
        &references.exact_bucket_assertions,
        max_artifact_bytes,
    )?;
    let exact_bucket_assertions: Vec<ExactBucketAssertion> =
        parse_json_or_jsonl(&exact_bucket_bytes, "exact bucket assertions")?;
    if references.exact_bucket_count != exact_bucket_assertions.len() as u64 {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "candidate_recall.exact_bucket_count does not match exact bucket assertions",
        ));
    }
    validate_exact_bucket_assertions(
        &exact_bucket_assertions,
        &references.exact_bucket_assertions.version,
    )?;
    validate_block_candidate_payload_hashes(
        &block_artifact,
        &candidate_records,
        &diagnostics,
        &exact_bucket_assertions,
    )
    .map_err(contract_error)?;

    let (_, report_bytes) = read_typed_artifact_ref(
        base_dir,
        "candidate_recall.report",
        &references.report,
        max_artifact_bytes,
    )?;
    let report = parse_candidate_recall_artifact(&report_bytes)?;

    let recomputed = evaluate_candidate_recall(CandidateRecallEvaluationRequest {
        candidate_records: &candidate_records,
        diagnostics: &diagnostics,
        gold_pairs: &gold_pairs,
        surface_ids: &surface_ids,
        exact_bucket_count: references.exact_bucket_count,
    });
    recomputed.validate().map_err(contract_error)?;
    if report != recomputed {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "candidate-recall report does not match recomputed native block inputs",
        ));
    }

    Ok(LoadedGeneralizationCandidateRecall {
        references: references.clone(),
        quality_manifest_hash: hash_bytes(&quality_manifest_bytes),
        block_artifact_hash: block_artifact.artifact_content_hash.clone(),
        candidate_records_hash: hash_bytes(&candidate_record_bytes),
        diagnostics_hash: hash_bytes(&diagnostics_bytes),
        exact_bucket_assertions_hash: hash_bytes(&exact_bucket_bytes),
        report_hash: hash_bytes(&report_bytes),
        surface_ids,
        gold_pairs,
        block_artifact,
        candidate_records,
        diagnostics,
        exact_bucket_assertions,
        report,
    })
}

fn load_generalization_solve_derivation(
    base_dir: &Path,
    trial: &GeneralizationTrialExecution,
    max_artifact_bytes: Option<u64>,
) -> GeneralizationResult<LoadedGeneralizationSolveDerivation> {
    validate_solve_derivation_refs(&trial.solve_derivation)?;

    let (_, edge_artifact_bytes) = read_typed_artifact_ref(
        base_dir,
        "solve_derivation.edge_artifact",
        &trial.solve_derivation.edge_artifact,
        max_artifact_bytes,
    )?;
    let edge_artifact: EdgeEvidenceArtifact =
        serde_json::from_slice(&edge_artifact_bytes).map_err(artifact_error)?;
    validate_edge_evidence_artifact_contract(&edge_artifact).map_err(contract_error)?;

    let (_, edge_record_bytes) = read_typed_artifact_ref(
        base_dir,
        "solve_derivation.edge_records",
        &trial.solve_derivation.edge_records,
        max_artifact_bytes,
    )?;
    let edge_records: Vec<EdgeEvidenceRecord> =
        parse_json_or_jsonl(&edge_record_bytes, "edge records")?;
    validate_edge_record_versions(&edge_records, &trial.solve_derivation.edge_records.version)?;

    let (_, prepared_surface_bytes) = read_typed_artifact_ref(
        base_dir,
        "solve_derivation.prepared_surfaces",
        &trial.solve_derivation.prepared_surfaces,
        max_artifact_bytes,
    )?;
    let prepared_surfaces: Vec<PreparedSurfaceRecord> =
        parse_json_or_jsonl(&prepared_surface_bytes, "prepared surfaces")?;

    let (_, solve_policy_bytes) = read_typed_artifact_ref(
        base_dir,
        "solve_derivation.solve_policy",
        &trial.solve_derivation.solve_policy,
        max_artifact_bytes,
    )?;
    let solve_policy_hash = hash_bytes(&solve_policy_bytes);
    if solve_policy_hash != trial.cross_bindings.policy_digest {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "solve_derivation.solve_policy content hash must equal cross_bindings.policy_digest",
        ));
    }
    let solve_policy: GeneralizationSolvePolicy =
        serde_json::from_slice(&solve_policy_bytes).map_err(artifact_error)?;
    if solve_policy.version != trial.solve_derivation.solve_policy.version {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "solve_derivation.solve_policy version does not match envelope ref",
        ));
    }

    Ok(LoadedGeneralizationSolveDerivation {
        references: trial.solve_derivation.clone(),
        edge_artifact_hash: edge_artifact.artifact_content_hash.clone(),
        edge_records_hash: hash_bytes(&edge_record_bytes),
        prepared_surfaces_hash: hash_bytes(&prepared_surface_bytes),
        solve_policy_hash,
        edge_artifact,
        edge_records,
        prepared_surfaces,
        solve_config: solve_policy.config,
    })
}

fn load_generalization_cache_execution(
    base_dir: &Path,
    trial: &GeneralizationTrialExecution,
    artifacts: &[LoadedGeneralizationArtifactRef],
    max_artifact_bytes: Option<u64>,
) -> GeneralizationResult<LoadedGeneralizationCacheExecution> {
    let field = format!("trials[{}].cache_execution", trial.trial_id);
    validate_cache_execution_ref(&trial.cache_execution, &field)?;
    let (receipt_path, receipt_bytes) = read_typed_artifact_ref(
        base_dir,
        &format!("{field}.receipt"),
        &trial.cache_execution.receipt,
        max_artifact_bytes,
    )?;
    let receipt: EntityIndexCacheReceipt =
        serde_json::from_slice(&receipt_bytes).map_err(artifact_error)?;
    validate_cache_execution_receipt_payload(&trial.cache_execution, &receipt, &field)?;
    let (bundle_receipt_path, bundle_receipt_bytes) = read_typed_artifact_ref(
        base_dir,
        &format!("{field}.bundle_receipt"),
        &trial.cache_execution.bundle_receipt,
        max_artifact_bytes,
    )?;
    let bundle_receipt: EntityIndexCacheReceipt =
        serde_json::from_slice(&bundle_receipt_bytes).map_err(artifact_error)?;
    validate_cache_bundle_receipt_payload(&bundle_receipt, &field)?;
    validate_cache_execution_receipt_matches_bundle(&receipt, &bundle_receipt, &field)?;

    let run_ref = loaded_run_artifact_ref(artifacts)?;
    let run = loaded_run_artifact(artifacts)?;
    let bundle_files = load_and_validate_cache_receipt_bundle_files(
        base_dir,
        &run_ref.reference.path,
        &bundle_receipt,
        max_artifact_bytes,
        &field,
    )?;
    validate_cache_execution_run_binding(CacheExecutionRunBindingContext {
        run_ref_path: &run_ref.reference.path,
        run,
        reference: &trial.cache_execution,
        leak_source_bundle: &trial.leak_scan_sources,
        receipt: &receipt,
        bundle_receipt: &bundle_receipt,
        bundle_files: &bundle_files,
        field: &field,
    })?;

    let receipt_path = receipt_path_to_manifest_relative(base_dir, &receipt_path)
        .unwrap_or_else(|| trial.cache_execution.receipt.path.clone());
    let bundle_receipt_path = receipt_path_to_manifest_relative(base_dir, &bundle_receipt_path)
        .unwrap_or_else(|| trial.cache_execution.bundle_receipt.path.clone());
    Ok(LoadedGeneralizationCacheExecution {
        references: trial.cache_execution.clone(),
        receipt_path,
        receipt_hash: trial.cache_execution.receipt.content_hash.clone(),
        receipt_byte_count: receipt_bytes.len() as u64,
        receipt,
        bundle_receipt_path,
        bundle_receipt_hash: trial.cache_execution.bundle_receipt.content_hash.clone(),
        bundle_receipt_byte_count: bundle_receipt_bytes.len() as u64,
        bundle_receipt,
        bundle_files,
    })
}

fn receipt_path_to_manifest_relative(base_dir: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(base_dir)
        .ok()
        .and_then(|relative| relative.to_str())
        .map(|relative| relative.to_string())
}

fn validate_cache_execution_ref(
    reference: &GeneralizationCacheExecutionRef,
    field: &str,
) -> GeneralizationResult<()> {
    if reference.version != CANON_GENERALIZATION_CACHE_EXECUTION_VERSION {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.version must be {CANON_GENERALIZATION_CACHE_EXECUTION_VERSION}"),
        ));
    }
    validate_typed_artifact_ref(&reference.receipt, &format!("{field}.receipt"))?;
    require_ref_version(
        &reference.receipt,
        CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION,
        &format!("{field}.receipt"),
    )?;
    validate_typed_artifact_ref(
        &reference.bundle_receipt,
        &format!("{field}.bundle_receipt"),
    )?;
    require_ref_version(
        &reference.bundle_receipt,
        CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION,
        &format!("{field}.bundle_receipt"),
    )?;
    if !reference
        .receipt
        .path
        .ends_with(RUN_CACHE_EXECUTION_RECEIPT_PATH)
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.receipt.path must end with {RUN_CACHE_EXECUTION_RECEIPT_PATH}"),
        ));
    }
    if !reference
        .bundle_receipt
        .path
        .ends_with(INDEX_CACHE_RECEIPT_FILE)
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.bundle_receipt.path must end with {INDEX_CACHE_RECEIPT_FILE}"),
        ));
    }
    if reference.receipt.path == reference.bundle_receipt.path
        || reference.receipt.content_hash == reference.bundle_receipt.content_hash
    {
        return Err(error(
            GeneralizationErrorCode::DuplicateRecord,
            format!("{field}.receipt and bundle_receipt must be distinct artifacts"),
        ));
    }
    Ok(())
}

fn validate_cache_execution_receipt_payload(
    reference: &GeneralizationCacheExecutionRef,
    receipt: &EntityIndexCacheReceipt,
    field: &str,
) -> GeneralizationResult<()> {
    if receipt.version != CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "{field}.receipt payload version must be {CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION}"
            ),
        ));
    }
    verify_declared_digest(
        &format!("{field}.receipt.bundle_hash"),
        &receipt.bundle_hash,
    )?;
    let valid = match reference.mode {
        GeneralizationCacheExecutionMode::DisabledBypass => {
            receipt.mode == EntityIndexCacheMode::Disabled
                && receipt.status == EntityIndexCacheStatus::Bypassed
                && !receipt.reusable
        }
        GeneralizationCacheExecutionMode::EnabledWarmHit => {
            receipt.mode == EntityIndexCacheMode::Enabled
                && receipt.status == EntityIndexCacheStatus::Hit
                && receipt.reusable
        }
    };
    if !valid {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "{field}.receipt mode/status/reusable does not satisfy strict {:?}",
                reference.mode
            ),
        ));
    }
    Ok(())
}

fn validate_cache_bundle_receipt_payload(
    receipt: &EntityIndexCacheReceipt,
    field: &str,
) -> GeneralizationResult<()> {
    if receipt.version != CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "{field}.bundle_receipt payload version must be {CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION}"
            ),
        ));
    }
    verify_declared_digest(
        &format!("{field}.bundle_receipt.bundle_hash"),
        &receipt.bundle_hash,
    )?;
    if receipt.mode != EntityIndexCacheMode::Enabled
        || receipt.status != EntityIndexCacheStatus::Rebuilt
        || !receipt.reusable
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "{field}.bundle_receipt must be the immutable enabled rebuilt reusable index bundle receipt"
            ),
        ));
    }
    Ok(())
}

fn validate_cache_execution_receipt_matches_bundle(
    execution: &EntityIndexCacheReceipt,
    bundle: &EntityIndexCacheReceipt,
    field: &str,
) -> GeneralizationResult<()> {
    if execution.bundle_hash != bundle.bundle_hash || execution.files != bundle.files {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.receipt must bind the same immutable index bundle as bundle_receipt"),
        ));
    }
    Ok(())
}

fn required_cache_receipt_files() -> [(&'static str, &'static str); 4] {
    [
        ("artifact", "index/index.json"),
        ("cache_key", INDEX_CACHE_KEY_FILE),
        ("postings", "index/postings.bin"),
        ("diagnostics", DEFAULT_INDEX_DIAGNOSTICS_PATH),
    ]
}

fn load_and_validate_cache_receipt_bundle_files(
    base_dir: &Path,
    run_ref_path: &str,
    receipt: &EntityIndexCacheReceipt,
    max_artifact_bytes: Option<u64>,
    field: &str,
) -> GeneralizationResult<Vec<LoadedGeneralizationCacheReceiptFile>> {
    let expected = required_cache_receipt_files();
    if receipt.files.len() != expected.len() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.receipt.files must bind the complete index cache bundle"),
        ));
    }

    let mut loaded_files = Vec::with_capacity(receipt.files.len());
    let mut bundle_material = Vec::new();
    let mut seen_paths = BTreeSet::new();
    let mut seen_roles = BTreeSet::new();
    for (index, (file, (expected_role, expected_path))) in
        receipt.files.iter().zip(expected.iter()).enumerate()
    {
        let file_field = format!("{field}.receipt.files[{index}]");
        if file.role != *expected_role || file.path != *expected_path {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{file_field} must be {expected_role}:{expected_path}"),
            ));
        }
        if !seen_roles.insert(file.role.as_str()) || !seen_paths.insert(file.path.as_str()) {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("{file_field} duplicates another cache receipt file"),
            ));
        }
        normalize_path_ref(&file.path, &format!("{file_field}.path"))?;
        verify_declared_digest(&format!("{file_field}.content_hash"), &file.content_hash)?;
        if file.byte_count == 0 {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{file_field}.byte_count must be nonzero"),
            ));
        }
        let manifest_path =
            safe_run_stage_checked_path(run_ref_path, &file.path, &format!("{file_field}.path"))?;
        let (_, bytes) =
            read_strict_manifest_file(base_dir, &format!("{file_field}.path"), &manifest_path)?;
        validate_resource_limit(&file_field, bytes.len(), max_artifact_bytes)?;
        verify_declared_content_hash(
            &format!("{file_field}.content_hash"),
            &file.content_hash,
            &bytes,
        )?;
        if file.byte_count != bytes.len() as u64 {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{file_field}.byte_count does not match cache bundle bytes"),
            ));
        }
        let index_artifact_content_hash = if file.role == "artifact" {
            Some(index_artifact_content_hash_from_bytes(
                &bytes,
                &format!("{file_field}.index_artifact"),
            )?)
        } else {
            None
        };
        bundle_material.extend_from_slice(file.role.as_bytes());
        bundle_material.push(0);
        bundle_material.extend_from_slice(file.path.as_bytes());
        bundle_material.push(0);
        bundle_material.extend_from_slice(file.byte_count.to_string().as_bytes());
        bundle_material.push(0);
        bundle_material.extend_from_slice(&bytes);
        bundle_material.push(0);
        loaded_files.push(LoadedGeneralizationCacheReceiptFile {
            role: file.role.clone(),
            path: file.path.clone(),
            content_hash: file.content_hash.clone(),
            byte_count: file.byte_count,
            index_artifact_content_hash,
        });
    }
    let actual_bundle_hash = hash_bytes(&bundle_material);
    if receipt.bundle_hash != actual_bundle_hash {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.receipt.bundle_hash does not match cache bundle bytes"),
        ));
    }
    Ok(loaded_files)
}

fn index_artifact_content_hash_from_bytes(
    bytes: &[u8],
    field: &str,
) -> GeneralizationResult<String> {
    let artifact: Value = serde_json::from_slice(bytes).map_err(|error| {
        GeneralizationError::new(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} is not a valid entity index artifact: {error}"),
        )
    })?;
    let contract = validate_artifact_v1_core_contract(&artifact).map_err(|refusal| {
        error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "{field} failed native index v1 artifact contract: {}",
                refusal.message
            ),
        )
    })?;
    if contract.stage != EntityArtifactStageV1::Index
        || contract.artifact_version != CANON_ENTITY_INDEX_VERSION_V1
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must be a canon_entity_index.v1 artifact"),
        ));
    }
    validate_entity_v1_self_hash(&artifact).map_err(|refusal| {
        error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "{field} failed native index v1 self-hash validation: {}",
                refusal.message
            ),
        )
    })
}

struct CacheExecutionRunBindingContext<'a> {
    run_ref_path: &'a str,
    run: &'a EntityRunArtifact,
    reference: &'a GeneralizationCacheExecutionRef,
    leak_source_bundle: &'a GeneralizationLeakSourceBundleRef,
    receipt: &'a EntityIndexCacheReceipt,
    bundle_receipt: &'a EntityIndexCacheReceipt,
    bundle_files: &'a [LoadedGeneralizationCacheReceiptFile],
    field: &'a str,
}

fn validate_cache_execution_run_binding(
    ctx: CacheExecutionRunBindingContext<'_>,
) -> GeneralizationResult<()> {
    let CacheExecutionRunBindingContext {
        run_ref_path,
        run,
        reference,
        leak_source_bundle,
        receipt,
        bundle_receipt,
        bundle_files,
        field,
    } = ctx;
    let expected_stage = match reference.mode {
        GeneralizationCacheExecutionMode::DisabledBypass => "cache_disabled",
        GeneralizationCacheExecutionMode::EnabledWarmHit => "cache_enabled",
    };
    let mut matching_stages = run
        .stage_artifacts
        .iter()
        .filter(|stage| stage.artifact_content_hash == reference.receipt.content_hash)
        .collect::<Vec<_>>();
    if matching_stages.len() != 1 {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.receipt hash must be bound by exactly one run cache stage"),
        ));
    }
    let cache_stage = matching_stages.remove(0);
    if cache_stage.stage != expected_stage {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.receipt must be bound by run stage {expected_stage}"),
        ));
    }
    if cache_stage.version != reference.receipt.version {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.receipt version does not match the run cache stage"),
        ));
    }
    let stage_path = safe_run_stage_checked_path(
        run_ref_path,
        &cache_stage.path,
        &format!("{field}.stage.path"),
    )?;
    if stage_path != reference.receipt.path || cache_stage.path != RUN_CACHE_EXECUTION_RECEIPT_PATH
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.receipt path does not match native cache stage path"),
        ));
    }
    let bundle_stage_path = safe_run_stage_checked_path(
        run_ref_path,
        INDEX_CACHE_RECEIPT_FILE,
        &format!("{field}.bundle_receipt.path"),
    )?;
    if bundle_stage_path != reference.bundle_receipt.path {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.bundle_receipt path does not match native index bundle path"),
        ));
    }
    require_run_label(
        run,
        "cache_mode",
        receipt.mode.as_str(),
        &format!("{field}.run_labels"),
    )?;
    require_run_label(
        run,
        "cache_status",
        receipt.status.as_str(),
        &format!("{field}.run_labels"),
    )?;
    require_run_label(
        run,
        "cache_receipt_path",
        RUN_CACHE_EXECUTION_RECEIPT_PATH,
        &format!("{field}.run_labels"),
    )?;
    require_run_label(
        run,
        "cache_receipt_hash",
        &reference.receipt.content_hash,
        &format!("{field}.run_labels"),
    )?;
    require_run_label(
        run,
        "cache_bundle_receipt_path",
        INDEX_CACHE_RECEIPT_FILE,
        &format!("{field}.run_labels"),
    )?;
    require_run_label(
        run,
        "cache_bundle_receipt_hash",
        &reference.bundle_receipt.content_hash,
        &format!("{field}.run_labels"),
    )?;

    let index_stage = run
        .stage_artifacts
        .iter()
        .find(|stage| stage.stage == "index" && stage.version == CANON_ENTITY_INDEX_VERSION_V1)
        .ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!("{field} requires a native index run stage"),
            )
        })?;
    if !cache_stage
        .upstream_artifacts
        .contains(&stage_artifact_ref(index_stage))
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.receipt run stage must upstream the native index artifact"),
        ));
    }
    let bundle_ref = EntityArtifactReference {
        version: reference.bundle_receipt.version.clone(),
        content_hash: reference.bundle_receipt.content_hash.clone(),
    };
    let leak_bundle_ref = EntityArtifactReference {
        version: leak_source_bundle.version.clone(),
        content_hash: leak_source_bundle.content_hash.clone(),
    };
    if !cache_stage.upstream_artifacts.contains(&bundle_ref) {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.receipt run stage must upstream the immutable bundle receipt"),
        ));
    }
    if !cache_stage.upstream_artifacts.contains(&leak_bundle_ref) {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.receipt run stage must upstream the trial leak source bundle"),
        ));
    }
    let mut expected_upstreams = vec![
        stage_artifact_ref(index_stage),
        bundle_ref.clone(),
        leak_bundle_ref.clone(),
    ];
    expected_upstreams.sort_by(entity_artifact_ref_cmp);
    if cache_stage.upstream_artifacts != expected_upstreams {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "{field}.receipt run stage must bind exactly index, bundle, and trial leak source upstreams"
            ),
        ));
    }
    verify_declared_digest(
        &format!("{field}.bundle_receipt.bundle_hash"),
        &bundle_receipt.bundle_hash,
    )?;
    let artifact_file = bundle_files
        .iter()
        .find(|file| file.role == "artifact")
        .ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!("{field}.receipt must include the index artifact bundle file"),
            )
        })?;
    let Some(index_artifact_content_hash) = artifact_file.index_artifact_content_hash.as_deref()
    else {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.receipt index artifact file was not parsed"),
        ));
    };
    if index_artifact_content_hash != index_stage.artifact_content_hash {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.receipt parsed index artifact does not match the run index stage"),
        ));
    }
    Ok(())
}

fn require_run_label(
    run: &EntityRunArtifact,
    key: &str,
    expected: &str,
    field: &str,
) -> GeneralizationResult<()> {
    match run.summary.labels.get(key) {
        Some(actual) if actual == expected => Ok(()),
        Some(_) => Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.{key} does not match native cache execution"),
        )),
        None => Err(error(
            GeneralizationErrorCode::MissingReference,
            format!("{field}.{key} is missing from run summary labels"),
        )),
    }
}

pub fn load_generalization_artifact_ref(
    base_dir: &Path,
    field: &str,
    reference: &GeneralizationArtifactRef,
    max_artifact_bytes: Option<u64>,
) -> GeneralizationResult<LoadedGeneralizationArtifactRef> {
    validate_artifact_ref(reference, field)?;
    let (_, bytes) = read_strict_manifest_file(base_dir, field, &reference.path)?;
    validate_resource_limit(field, bytes.len(), max_artifact_bytes)?;
    verify_declared_content_hash(field, &reference.content_hash, &bytes)?;
    let kind = infer_artifact_kind(reference, field)?;
    let artifact = match kind {
        GeneralizationArtifactKind::CandidateRecall => {
            let value: Value = serde_json::from_slice(&bytes).map_err(artifact_error)?;
            validate_json_version(field, &value, &reference.version)?;
            LoadedGeneralizationArtifact::CandidateRecall(parse_candidate_recall_artifact(&bytes)?)
        }
        GeneralizationArtifactKind::Link => {
            let value: Value = serde_json::from_slice(&bytes).map_err(artifact_error)?;
            validate_json_version(field, &value, &reference.version)?;
            LoadedGeneralizationArtifact::Link(parse_link_artifact(&bytes)?)
        }
        GeneralizationArtifactKind::LinkObservationSurfaceBindings => {
            LoadedGeneralizationArtifact::LinkObservationSurfaceBindings(
                parse_link_observation_surface_bindings(&bytes)?,
            )
        }
        GeneralizationArtifactKind::Run => {
            let value: Value = serde_json::from_slice(&bytes).map_err(artifact_error)?;
            validate_json_version(field, &value, &reference.version)?;
            LoadedGeneralizationArtifact::Run(parse_run_artifact(&bytes)?)
        }
        GeneralizationArtifactKind::Solve => {
            let value: Value = serde_json::from_slice(&bytes).map_err(artifact_error)?;
            validate_json_version(field, &value, &reference.version)?;
            LoadedGeneralizationArtifact::Solve(parse_solve_artifact(&bytes)?)
        }
        GeneralizationArtifactKind::LeakScanSources => {
            let value: Value = serde_json::from_slice(&bytes).map_err(artifact_error)?;
            validate_json_version(field, &value, &reference.version)?;
            LoadedGeneralizationArtifact::LeakScanSources(value)
        }
    };
    Ok(LoadedGeneralizationArtifactRef {
        reference: reference.clone(),
        artifact,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn load_generalization_leak_source_bundle_ref(
    base_dir: &Path,
    field: &str,
    reference: &GeneralizationLeakSourceBundleRef,
    max_artifact_bytes: Option<u64>,
    benchmark_ref: &GeneralizationBenchmarkRef,
    benchmark_content_hash: &str,
    trial: &GeneralizationTrialExecution,
    candidate_recall: &LoadedGeneralizationCandidateRecall,
    solve_derivation: &LoadedGeneralizationSolveDerivation,
    cache_execution: &LoadedGeneralizationCacheExecution,
    artifacts: &[LoadedGeneralizationArtifactRef],
) -> GeneralizationResult<Vec<LoadedGeneralizationLeakSourceRef>> {
    validate_leak_source_bundle_ref(reference, field)?;
    let (_, bytes) =
        read_strict_manifest_file(base_dir, &format!("{field}.path"), &reference.path)?;
    validate_resource_limit(field, bytes.len(), max_artifact_bytes)?;
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.path must contain a nonempty pre-evaluation influence source"),
        ));
    }
    verify_declared_content_hash(
        &format!("{field}.content_hash"),
        &reference.content_hash,
        &bytes,
    )?;
    let bundle: GeneralizationLeakSourceBundle =
        serde_json::from_slice(&bytes).map_err(artifact_error)?;
    validate_leak_source_bundle(&bundle, reference, field)?;
    require_run_binds_leak_source_bundle(loaded_run_artifact(artifacts)?, reference, field)?;
    let allowed_bindings = allowed_leak_source_bindings(trial, candidate_recall, artifacts)?;
    let checked_source_guards = checked_source_guards(
        benchmark_ref,
        benchmark_content_hash,
        reference,
        candidate_recall,
        solve_derivation,
        artifacts,
    )?;
    let mut checked_path_provenance = BTreeMap::new();
    let sources = bundle
        .sources
        .into_iter()
        .enumerate()
        .map(|(source_index, source)| {
            let source_field = format!("{field}.sources[{source_index}]");
            validate_leak_source_binding(&source, &allowed_bindings, &source_field)?;
            checked_source_guards.validate_binding(&source.binding_hash, &source_field)?;
            let mut raw_scan_bytes = Vec::new();
            let mut decoded_strings = BTreeSet::new();
            let mut derived_records = Vec::new();
            let mut checked_sources = Vec::new();
            let mut registry_binding_entries = Vec::new();
            for (checked_index, checked_source) in source.checked_sources.iter().enumerate() {
                let checked_field = format!("{source_field}.checked_sources[{checked_index}]");
                checked_source_guards.validate(checked_source, &checked_field)?;
                let (_, source_bytes) = read_strict_manifest_file(
                    base_dir,
                    &format!("{checked_field}.path"),
                    &checked_source.path,
                )?;
                validate_resource_limit(&checked_field, source_bytes.len(), max_artifact_bytes)?;
                verify_declared_content_hash(
                    &format!("{checked_field}.content_hash"),
                    &checked_source.content_hash,
                    &source_bytes,
                )?;
                if checked_source.byte_count != source_bytes.len() as u64 {
                    return Err(error(
                        GeneralizationErrorCode::ArtifactContract,
                        format!("{checked_field}.byte_count does not match source bytes"),
                    ));
                }
                let checked_records = derive_leak_projection_records(
                    checked_source.format,
                    &source_bytes,
                    &checked_field,
                )?;
                if checked_source.record_count != checked_records.len() as u64 {
                    return Err(error(
                        GeneralizationErrorCode::ArtifactContract,
                        format!("{checked_field}.record_count does not match derived records"),
                    ));
                }
                if checked_records.is_empty() {
                    return Err(error(
                        GeneralizationErrorCode::ArtifactContract,
                        format!("{checked_field} must derive at least one scan record"),
                    ));
                }
                decoded_strings.extend(decoded_json_scalar_strings(&Value::Array(
                    checked_records.clone(),
                )));
                if is_registry_leak_source(&source) {
                    registry_binding_entries.push(RegistryLeakBindingEntry {
                        path: checked_source.path.clone(),
                        bytes: source_bytes.clone(),
                    });
                }
                derived_records.extend(checked_records);
                raw_scan_bytes.extend_from_slice(&source_bytes);
                raw_scan_bytes.push(b'\n');
                checked_sources.push(LoadedGeneralizationCheckedLeakSourceRef {
                    path: checked_source.path.clone(),
                    format: checked_source.format,
                    content_hash: checked_source.content_hash.clone(),
                    byte_count: checked_source.byte_count,
                    record_count: checked_source.record_count,
                });
            }
            let completeness_signature = validate_leak_source_completeness_manifest(
                base_dir,
                &source,
                &source_field,
                max_artifact_bytes,
                registry_binding_entries.as_slice(),
            )?;
            validate_checked_path_reuse(
                &mut checked_path_provenance,
                &source,
                &checked_sources,
                completeness_signature.as_ref(),
                &source_field,
            )?;
            validate_derived_leak_source_binding(
                &source,
                &checked_sources,
                registry_binding_entries.as_slice(),
                &allowed_bindings,
                &source_field,
            )?;
            if derived_records != source.records {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!(
                        "leak source {} inline records are not derived from checked source bytes",
                        source.source_id
                    ),
                ));
            }
            let record_bytes = serde_json::to_vec(&source.records).map_err(artifact_error)?;
            raw_scan_bytes.extend_from_slice(&record_bytes);
            let decoded_strings = decoded_strings
                .into_iter()
                .chain(decoded_json_scalar_strings(&Value::Array(
                    source.records.clone(),
                )))
                .collect();
            Ok(LoadedGeneralizationLeakSourceRef {
                source_id: source.source_id,
                phase: source.phase,
                channel: source.channel,
                source_kind: source.source_kind,
                binding_kind: source.binding_kind,
                binding_hash: source.binding_hash,
                coverage: source.coverage,
                content_hash: source.content_hash,
                bundle_content_hash: reference.content_hash.clone(),
                checked_sources,
                bytes: raw_scan_bytes,
                decoded_strings,
            })
        })
        .collect::<GeneralizationResult<Vec<_>>>()?;
    validate_cache_leak_source_matches_execution(&sources, cache_execution, field)?;
    Ok(sources)
}

fn validate_cache_leak_source_matches_execution(
    sources: &[LoadedGeneralizationLeakSourceRef],
    cache_execution: &LoadedGeneralizationCacheExecution,
    field: &str,
) -> GeneralizationResult<()> {
    let cache_sources = sources
        .iter()
        .filter(|source| source.channel == LeakChannel::Cache)
        .collect::<Vec<_>>();
    if cache_sources.len() != 1 {
        return Err(error(
            GeneralizationErrorCode::MissingReference,
            format!("{field} must contain exactly one cache leak source"),
        ));
    }
    let source = cache_sources[0];
    if source.source_kind != GeneralizationLeakSourceKind::Cache
        || source.binding_kind != GeneralizationLeakSourceBindingKind::RunStageArtifact
        || source.coverage != GeneralizationLeakSourceCoverage::CompleteSource
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.cache source must bind the complete cache run-stage artifact"),
        ));
    }
    if source.binding_hash != cache_execution.receipt_hash {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.cache binding_hash must equal cache_execution receipt hash"),
        ));
    }
    if source.checked_sources.len() != 1 {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.cache checked_sources must contain exactly the cache receipt"),
        ));
    }
    let checked = &source.checked_sources[0];
    if checked.path != cache_execution.receipt_path
        || checked.content_hash != cache_execution.receipt_hash
        || checked.byte_count != cache_execution.receipt_byte_count
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "{field}.cache checked source must equal cache_execution receipt path/hash/bytes"
            ),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct RegistryLeakBindingEntry {
    path: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegistryCompletenessProvenanceSignature {
    coverage: GeneralizationLeakSourceCoverage,
    root: String,
    entries: BTreeSet<GeneralizationLeakSourceCompletenessEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckedPathProvenanceSignature {
    binding_kind: GeneralizationLeakSourceBindingKind,
    binding_hash: String,
    coverage: GeneralizationLeakSourceCoverage,
    completeness: RegistryCompletenessProvenanceSignature,
    checked_descriptor: GeneralizationLeakSourceCompletenessEntry,
    derived_bytes_hash: String,
}

#[derive(Debug, Clone)]
struct ObservedCheckedPathProvenance {
    source_id: String,
    channel: LeakChannel,
    signature: Option<CheckedPathProvenanceSignature>,
}

#[derive(Debug)]
struct CheckedSourceGuards {
    prohibited_paths: BTreeSet<String>,
    prohibited_hashes: BTreeSet<String>,
}

impl CheckedSourceGuards {
    fn validate_binding(&self, binding_hash: &str, field: &str) -> GeneralizationResult<()> {
        if self.prohibited_hashes.contains(binding_hash) {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field}.binding_hash must not match sealed or post-evaluation material"),
            ));
        }
        Ok(())
    }

    fn validate(
        &self,
        checked_source: &GeneralizationCheckedLeakSource,
        field: &str,
    ) -> GeneralizationResult<()> {
        if self.prohibited_paths.contains(&checked_source.path) {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field}.path must not point at sealed or post-evaluation material"),
            ));
        }
        if self
            .prohibited_hashes
            .contains(&checked_source.content_hash)
        {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field}.content_hash must not match sealed or post-evaluation material"),
            ));
        }
        Ok(())
    }
}

fn checked_source_guards(
    benchmark_ref: &GeneralizationBenchmarkRef,
    benchmark_content_hash: &str,
    leak_source_ref: &GeneralizationLeakSourceBundleRef,
    candidate_recall: &LoadedGeneralizationCandidateRecall,
    solve_derivation: &LoadedGeneralizationSolveDerivation,
    artifacts: &[LoadedGeneralizationArtifactRef],
) -> GeneralizationResult<CheckedSourceGuards> {
    let mut prohibited_paths = BTreeSet::new();
    let mut prohibited_hashes = BTreeSet::new();
    prohibited_paths.insert(benchmark_ref.path.clone());
    prohibited_hashes.insert(benchmark_content_hash.to_string());
    prohibited_paths.insert(leak_source_ref.path.clone());
    prohibited_hashes.insert(leak_source_ref.content_hash.clone());
    for reference in [
        &candidate_recall.references.quality_manifest,
        &candidate_recall.references.block_artifact,
        &candidate_recall.references.candidates,
        &candidate_recall.references.diagnostics,
        &candidate_recall.references.exact_bucket_assertions,
        &candidate_recall.references.report,
    ] {
        prohibited_paths.insert(reference.path.clone());
        prohibited_hashes.insert(reference.content_hash.clone());
    }
    for reference in [
        &solve_derivation.references.edge_artifact,
        &solve_derivation.references.edge_records,
        &solve_derivation.references.prepared_surfaces,
        &solve_derivation.references.solve_policy,
    ] {
        prohibited_paths.insert(reference.path.clone());
        prohibited_hashes.insert(reference.content_hash.clone());
    }
    for artifact in artifacts {
        prohibited_paths.insert(artifact.reference.path.clone());
        prohibited_hashes.insert(artifact.reference.content_hash.clone());
    }
    let link = loaded_link_artifact(artifacts)?;
    let link_ref = loaded_link_artifact_ref(artifacts)?;
    prohibited_paths.insert(sibling_manifest_path(
        &link_ref.reference.path,
        &link.materialized_rows_path,
    )?);
    prohibited_paths.insert(sibling_manifest_path(
        &link_ref.reference.path,
        &link.observation_surface_bindings_path,
    )?);
    prohibited_hashes.insert(link.materialized_rows_content_hash.clone());
    prohibited_hashes.insert(link.observation_surface_bindings_content_hash.clone());
    prohibited_hashes.insert(
        loaded_run_artifact(artifacts)?
            .artifact_content_hash
            .clone(),
    );
    prohibited_hashes.insert(
        loaded_solve_artifact(artifacts)?
            .artifact_content_hash
            .clone(),
    );
    Ok(CheckedSourceGuards {
        prohibited_paths,
        prohibited_hashes,
    })
}

/// Replace a run's block/edge/solve stage bindings with validated native artifacts.
///
/// The helper preserves prepare/index and non-stage run fields, refreshes only
/// derived stage refs/counts/orchestration artifact references, and reseals the
/// run before returning it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneralizationNativeStageRebindResult {
    pub run: EntityRunArtifact,
    pub solve: SolveArtifact,
}

#[derive(Debug, Clone, Copy)]
pub struct GeneralizationNativeStageRebindRequest<'a> {
    pub run: &'a EntityRunArtifact,
    pub registry_dir: &'a Path,
    pub block: &'a BlockCandidateArtifact,
    pub block_candidate_records: &'a [BlockCandidateRecord],
    pub block_diagnostics: &'a BlockCandidateGenerationDiagnostics,
    pub exact_buckets: &'a [ExactBucketAssertion],
    pub edge: &'a EdgeEvidenceArtifact,
    pub edge_records: &'a [EdgeEvidenceRecord],
    pub prepared_surfaces: &'a [PreparedSurfaceRecord],
    pub solve_config: SolveReconciliationConfig,
}

pub fn rebind_generalization_native_stages(
    request: GeneralizationNativeStageRebindRequest<'_>,
) -> GeneralizationResult<GeneralizationNativeStageRebindResult> {
    let run = request.run;
    let block = request.block;
    let edge = request.edge;
    let registry_dir = request.registry_dir;
    validate_run_artifact_contract(run)?;
    validate_block_candidate_artifact_contract(block).map_err(contract_error)?;
    validate_block_candidate_payload_hashes(
        block,
        request.block_candidate_records,
        request.block_diagnostics,
        request.exact_buckets,
    )
    .map_err(contract_error)?;
    let block_path = validate_run_work_dir_path(
        &run.work_dir.block_artifact_path,
        "run.work_dir.block_artifact_path",
    )?;
    validate_run_work_dir_path(&run.work_dir.surfaces_path, "run.work_dir.surfaces_path")?;
    validate_rebind_work_dir_path(
        &block.candidate_records_path,
        &run.work_dir.candidate_records_path,
        "block.candidate_records_path",
    )?;
    validate_rebind_work_dir_path(
        &block.candidate_diagnostics_path,
        &run.work_dir.candidate_diagnostics_path,
        "block.candidate_diagnostics_path",
    )?;
    let edge_path = validate_run_work_dir_path(
        &run.work_dir.edge_artifact_path,
        "run.work_dir.edge_artifact_path",
    )?;
    validate_rebind_work_dir_path(
        &edge.edge_records_path,
        &run.work_dir.edge_records_path,
        "edge.edge_records_path",
    )?;
    let solve_path = validate_run_work_dir_path(
        &run.work_dir.solve_artifact_path,
        "run.work_dir.solve_artifact_path",
    )?;
    let solve_decision_ledger_path = validate_run_work_dir_path(
        &run.work_dir.decision_ledger_path,
        "run.work_dir.decision_ledger_path",
    )?;

    validate_replacement_metadata_context("block", &block.metadata, &run.metadata)?;
    let expected_block_strategy = derived_stage_strategy(&run.metadata.strategy, "block");
    validate_replacement_stage_strategy(
        "block",
        &block.metadata.strategy,
        &expected_block_strategy,
    )?;
    let expected_edge_strategy = derived_stage_strategy(&run.metadata.strategy, "evidence");
    validate_replacement_stage_strategy(
        "evidence",
        &edge.metadata.strategy,
        &expected_edge_strategy,
    )?;
    let expected_solve_strategy = derived_stage_strategy(&run.metadata.strategy, "solve");
    if run.orchestration.profile_firewall.strategy_hash != expected_solve_strategy.content_hash {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "run profile firewall strategy hash does not match the derived solve stage strategy",
        ));
    }

    let prepare_ref = single_stage_ref(run, "prepare")?;
    let index_ref = single_stage_ref(run, "index")?;
    require_artifact_ref(
        "block.upstream_artifacts",
        &block.upstream_artifacts,
        &prepare_ref,
    )?;
    require_artifact_ref(
        "block.upstream_artifacts",
        &block.upstream_artifacts,
        &index_ref,
    )?;

    let rebuilt_edge = build_edge_evidence_artifact_contract(EdgeEvidenceArtifactRequest {
        block: block.clone(),
        strategy: edge.metadata.strategy.clone(),
        edge_records_path: edge.edge_records_path.clone(),
        edge_records: request.edge_records.to_vec(),
        candidate_records: request.block_candidate_records.to_vec(),
        bucket_assertions: request.exact_buckets.to_vec(),
    })
    .map_err(contract_error)?;
    if &rebuilt_edge != edge {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "edge artifact does not match rebuilt block payload and edge evidence",
        ));
    }
    validate_edge_evidence_artifact_contract(edge).map_err(contract_error)?;
    validate_replacement_metadata_context("edge", &edge.metadata, &run.metadata)?;
    let block_ref = artifact_ref(&block.version, &block.artifact_content_hash);
    require_artifact_ref(
        "edge.upstream_artifacts",
        &edge.upstream_artifacts,
        &block_ref,
    )?;

    let edge_ref = artifact_ref(&edge.version, &edge.artifact_content_hash);
    let (incumbent_ids, solve_provenance) =
        derive_solve_inputs_from_prepared_surfaces(run, registry_dir, request.prepared_surfaces)?;
    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records: internal_edge_records_for_signed_graph(request.edge_records),
        exact_bucket_assertions: request.exact_buckets.to_vec(),
        incumbent_ids,
    })
    .map_err(contract_error)?;
    let mut solve_metadata = edge.metadata.clone();
    solve_metadata.strategy = expected_solve_strategy;
    merge_metadata_upstream_refs(&mut solve_metadata.upstream_artifacts, [edge_ref.clone()]);
    solve_metadata.artifact_content_hash.clear();
    let solve = build_solve_artifact_contract(SolveArtifactRequest {
        metadata: solve_metadata,
        graph,
        config: request.solve_config,
        provenance: solve_provenance,
        decision_ledger_path: solve_decision_ledger_path,
    })
    .map_err(contract_error)?;
    validate_solve_artifact_contract(&solve).map_err(contract_error)?;
    validate_replacement_metadata_context("solve", &solve.metadata, &run.metadata)?;

    let old_refs = BTreeMap::from([
        ("block".to_string(), single_stage_ref(run, "block")?),
        ("evidence".to_string(), single_stage_ref(run, "evidence")?),
        ("solve".to_string(), single_stage_ref(run, "solve")?),
    ]);
    let mut rebound = run.clone();
    replace_native_stage_descriptor(
        &mut rebound.stage_artifacts,
        "block",
        CANON_ENTITY_BLOCK_VERSION_V1,
        block.version.clone(),
        block_path,
        block.artifact_content_hash.clone(),
        block.upstream_artifacts.clone(),
    )?;
    replace_native_stage_descriptor(
        &mut rebound.stage_artifacts,
        "evidence",
        CANON_ENTITY_EVIDENCE_VERSION_V1,
        edge.version.clone(),
        edge_path,
        edge.artifact_content_hash.clone(),
        edge.upstream_artifacts.clone(),
    )?;
    replace_native_stage_descriptor(
        &mut rebound.stage_artifacts,
        "solve",
        CANON_ENTITY_SOLVE_VERSION_V1,
        solve.version.clone(),
        solve_path,
        solve.artifact_content_hash.clone(),
        solve.upstream_artifacts.clone(),
    )?;
    refresh_rebound_run_summary(&mut rebound, block, edge, &solve)?;
    rebound.metadata.upstream_artifacts = rebound
        .stage_artifacts
        .iter()
        .map(stage_artifact_ref)
        .collect();
    rebound
        .metadata
        .upstream_artifacts
        .sort_by(entity_artifact_ref_cmp);
    let new_refs = BTreeMap::from([
        ("block".to_string(), block_ref),
        ("evidence".to_string(), edge_ref),
        (
            "solve".to_string(),
            artifact_ref(&solve.version, &solve.artifact_content_hash),
        ),
    ]);
    refresh_rebound_orchestration_refs(&mut rebound, &old_refs, &new_refs);
    reseal_run_artifact(&mut rebound)?;
    validate_run_artifact_contract(&rebound)?;
    Ok(GeneralizationNativeStageRebindResult {
        run: rebound,
        solve,
    })
}

fn internal_edge_records_for_signed_graph(
    records: &[EdgeEvidenceRecord],
) -> Vec<EdgeEvidenceRecord> {
    let mut records = records.to_vec();
    for record in &mut records {
        record.version = CANON_ENTITY_EDGE_VERSION.to_string();
    }
    records
}

/// Decorate a native entity run with strict generalization provenance receipts.
///
/// This helper is intentionally in-memory: callers must run it before building
/// link artifacts so the link binds the final resealed run hash.
pub fn bind_generalization_run_provenance(
    run: &EntityRunArtifact,
    benchmark_id: &str,
    run_id: &str,
    trial_id: &str,
    family: GeneralizationTrialFamily,
    leak_source_bundle: &GeneralizationLeakSourceBundleRef,
    generated_corpus_receipt: crate::entity::run::EntityRunStageArtifact,
) -> GeneralizationResult<EntityRunArtifact> {
    validate_run_artifact_contract(run)?;
    let benchmark_id = normalize_non_empty(benchmark_id.to_string(), "benchmark_id")?;
    let run_id = normalize_non_empty(run_id.to_string(), "run_id")?;
    let trial_id = normalize_non_empty(trial_id.to_string(), "trial_id")?;
    validate_leak_source_bundle_ref_for_run_binding(leak_source_bundle)?;
    reject_preexisting_generalization_receipts(run)?;

    let leak_bundle_ref = EntityArtifactReference {
        version: leak_source_bundle.version.clone(),
        content_hash: leak_source_bundle.content_hash.clone(),
    };
    let cache_stage_index = validated_native_cache_execution_stage_index(run)?;
    let cache_stage = run.stage_artifacts[cache_stage_index].clone();
    let generated_corpus_receipt = validated_generalization_receipt_stage(
        generated_corpus_receipt,
        "generated_corpus_receipt",
        SafeRunStageClass::GeneratedCorpus,
        &leak_bundle_ref,
        "generated_corpus_receipt",
    )?;
    reject_conflicting_receipt_pair(&cache_stage, &generated_corpus_receipt)?;
    reject_receipt_hash_collisions(run, leak_source_bundle, [&generated_corpus_receipt])?;

    let mut decorated = run.clone();
    bind_generalization_label(&mut decorated.summary.labels, "benchmark_id", &benchmark_id)?;
    bind_generalization_label(&mut decorated.summary.labels, "run_id", &run_id)?;
    bind_generalization_label(&mut decorated.summary.labels, "trial_id", &trial_id)?;
    bind_generalization_label(
        &mut decorated.summary.labels,
        "family",
        trial_family_str(family),
    )?;
    merge_metadata_upstream_refs(
        &mut decorated.stage_artifacts[cache_stage_index].upstream_artifacts,
        [leak_bundle_ref.clone()],
    );
    let cache_stage_ref = stage_artifact_ref(&decorated.stage_artifacts[cache_stage_index]);
    decorated
        .stage_artifacts
        .push(generated_corpus_receipt.clone());
    merge_metadata_upstream_refs(
        &mut decorated.metadata.upstream_artifacts,
        [
            cache_stage_ref,
            stage_artifact_ref(&generated_corpus_receipt),
            leak_bundle_ref,
        ],
    );
    refresh_audit_handoff_refs_to_final_stage_refs(&mut decorated);
    reseal_run_artifact(&mut decorated)?;
    validate_run_artifact_contract(&decorated)?;
    Ok(decorated)
}

fn validate_run_work_dir_path(path: &str, field: &str) -> GeneralizationResult<String> {
    safe_run_stage_checked_path("run.json", path, field)
}

fn validate_rebind_work_dir_path(
    path: &str,
    expected_path: &str,
    field: &str,
) -> GeneralizationResult<()> {
    let path = validate_run_work_dir_path(path, field)?;
    if path != expected_path {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must equal the run work_dir canonical path"),
        ));
    }
    Ok(())
}

fn derived_stage_strategy(base: &EntityStrategyReference, stage: &str) -> EntityStrategyReference {
    EntityStrategyReference {
        id: format!("{}.{}", base.id, stage),
        version: base.version.clone(),
        content_hash: hash_bytes(format!("{}:{stage}", base.content_hash).as_bytes()),
    }
}

fn validate_replacement_stage_strategy(
    stage: &str,
    actual: &EntityStrategyReference,
    expected: &EntityStrategyReference,
) -> GeneralizationResult<()> {
    if actual != expected {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{stage} artifact strategy does not match the derived stage strategy"),
        ));
    }
    Ok(())
}

fn derive_solve_inputs_from_prepared_surfaces(
    run: &EntityRunArtifact,
    registry_dir: &Path,
    surfaces: &[PreparedSurfaceRecord],
) -> GeneralizationResult<(Vec<SurfaceIncumbentId>, Vec<SolveSurfaceProvenance>)> {
    if surfaces.is_empty()
        && run
            .metadata
            .input
            .as_ref()
            .is_some_and(|input| input.row_count > 0)
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "prepared surfaces are required to derive solve incumbents and provenance",
        ));
    }

    let registry_replay = load_prepared_registry_replay(run, registry_dir, surfaces)?;
    validate_prepared_payload_completeness(run, surfaces, &registry_replay)?;
    let mut seen_surface_ids = BTreeSet::new();
    let mut incumbents = Vec::new();
    let mut provenance = Vec::with_capacity(surfaces.len());
    for (index, surface) in surfaces.iter().enumerate() {
        let field = format!("prepared_surfaces[{index}]");
        if ascii_trim(&surface.surface_id).is_empty() {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field}.surface_id must not be empty"),
            ));
        }
        if !seen_surface_ids.insert(surface.surface_id.clone()) {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("{field}.surface_id duplicates another prepared surface"),
            ));
        }
        if surface.profile_id != run.metadata.profile.id {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field}.profile_id does not match the run profile"),
            ));
        }
        validate_prepared_surface_id(surface, &field)?;
        let replay_lookup_inputs = prepared_surface_lookup_inputs(surface);
        validate_prepared_exact_lookup(
            &surface.exact_lookup,
            &run.metadata.registry_snapshot,
            &registry_replay,
            &replay_lookup_inputs,
            &field,
        )?;
        if let Some(canonical_id) = &surface.exact_lookup.canonical_id {
            incumbents.push(SurfaceIncumbentId {
                surface_id: surface.surface_id.clone(),
                canonical_id: canonical_id.clone(),
            });
        }
        provenance.push(SolveSurfaceProvenance {
            surface_id: surface.surface_id.clone(),
            row_count: surface.row_count,
            deal_count: surface.deal_count,
        });
    }
    incumbents.sort_by(|left, right| {
        left.surface_id
            .cmp(&right.surface_id)
            .then_with(|| left.canonical_id.cmp(&right.canonical_id))
    });
    provenance.sort_by(|left, right| left.surface_id.cmp(&right.surface_id));
    Ok((incumbents, provenance))
}

fn validate_prepared_payload_completeness(
    run: &EntityRunArtifact,
    surfaces: &[PreparedSurfaceRecord],
    registry_replay: &PreparedRegistryReplay,
) -> GeneralizationResult<()> {
    let surface_count = u64::try_from(surfaces.len()).map_err(|_| {
        error(
            GeneralizationErrorCode::ArtifactContract,
            "prepared surface count exceeds u64",
        )
    })?;
    let expected_surface_count =
        required_summary_count(&run.summary.counts, "run", "prepared_surfaces")?;
    if surface_count != expected_surface_count {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "prepared surface payload count does not match the native run summary",
        ));
    }

    let row_count = surfaces.iter().try_fold(0u64, |total, surface| {
        total.checked_add(surface.row_count).ok_or_else(|| {
            error(
                GeneralizationErrorCode::ArtifactContract,
                "prepared surface row_count sum overflows u64",
            )
        })
    })?;
    let expected_row_count = required_summary_count(&run.summary.counts, "run", "row_count")?;
    if row_count != expected_row_count {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "prepared surface row_count sum does not match the native run summary",
        ));
    }
    let Some(input) = &run.metadata.input else {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "run metadata input is required for prepared surface completeness validation",
        ));
    };
    if row_count != input.row_count {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "prepared surface row_count sum does not match the native run input row count",
        ));
    }

    let mut resolved_replay_count = 0u64;
    for (index, surface) in surfaces.iter().enumerate() {
        let field = format!("prepared_surfaces[{index}]");
        if replay_prepared_exact_lookup(
            registry_replay,
            &prepared_surface_lookup_inputs(surface),
            &field,
        )?
        .is_some()
        {
            resolved_replay_count = resolved_replay_count.checked_add(1).ok_or_else(|| {
                error(
                    GeneralizationErrorCode::ArtifactContract,
                    "prepared resolved replay count overflows u64",
                )
            })?;
        }
    }
    let expected_resolved_count =
        required_summary_count(&run.summary.counts, "run", "exact_resolved_surfaces")?;
    if resolved_replay_count != expected_resolved_count {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "prepared exact lookup replay resolved count does not match the native run summary",
        ));
    }
    Ok(())
}

struct PreparedRegistryReplay {
    mappings: BTreeMap<String, Mapping>,
}

fn load_prepared_registry_replay(
    run: &EntityRunArtifact,
    registry_dir: &Path,
    surfaces: &[PreparedSurfaceRecord],
) -> GeneralizationResult<PreparedRegistryReplay> {
    if surfaces.is_empty() {
        return Ok(PreparedRegistryReplay {
            mappings: BTreeMap::new(),
        });
    }

    let registry_snapshot = &run.metadata.registry_snapshot;
    if ascii_trim(&registry_snapshot.source).is_empty() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "run registry snapshot source is required for prepared exact lookup replay",
        ));
    }
    let root_hash = hash_registry_json_files_for_replay(registry_dir)?;
    if root_hash != registry_snapshot.lookup_snapshot_hash {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "run registry root hash does not match the registry snapshot",
        ));
    }

    let registry = registry::load_registry(registry_dir).map_err(|source| {
        error(
            GeneralizationErrorCode::ArtifactContract,
            format!("failed to load run registry for prepared exact lookup replay: {source}"),
        )
    })?;
    if registry.meta.id != registry_snapshot.id
        || registry.meta.version != registry_snapshot.version
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "loaded registry id/version does not match the run registry snapshot",
        ));
    }

    let values = surfaces
        .iter()
        .flat_map(prepared_surface_lookup_inputs)
        .map(|input| (input, ()))
        .collect::<HashMap<_, _>>();
    let input_values = InputValues {
        values,
        special: HashMap::new(),
        format: InputFormat::Jsonl,
        delimiter: None,
        source_hash: None,
        source_bytes: None,
    };
    let resolved = lookup::resolve_values(&registry, &input_values).map_err(|source| {
        error(
            GeneralizationErrorCode::ArtifactContract,
            format!("prepared exact lookup replay failed: {source}"),
        )
    })?;

    Ok(PreparedRegistryReplay {
        mappings: resolved
            .mappings
            .into_iter()
            .map(|mapping| (mapping.input.clone(), mapping))
            .collect(),
    })
}

fn hash_registry_json_files_for_replay(registry_dir: &Path) -> GeneralizationResult<String> {
    let metadata = fs::symlink_metadata(registry_dir).map_err(|source| {
        error(
            GeneralizationErrorCode::ArtifactContract,
            format!("failed to inspect run registry root for prepared replay: {source}"),
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "run registry root for prepared replay must not be a symlink",
        ));
    }
    if !metadata.is_dir() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "run registry root for prepared replay must be a directory",
        ));
    }
    let entries = fs::read_dir(registry_dir).map_err(|source| {
        error(
            GeneralizationErrorCode::ArtifactContract,
            format!("failed to read run registry root for prepared replay: {source}"),
        )
    })?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| {
            error(
                GeneralizationErrorCode::ArtifactContract,
                format!("failed to inspect run registry root for prepared replay: {source}"),
            )
        })?;
        let file_type = entry.file_type().map_err(|source| {
            error(
                GeneralizationErrorCode::ArtifactContract,
                format!("failed to inspect run registry source file for prepared replay: {source}"),
            )
        })?;
        if file_type.is_symlink() {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                "run registry root for prepared replay must not contain symlinks",
            ));
        }
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("json") {
            files.push(path);
        }
    }
    files.sort();

    let mut hasher = blake3::Hasher::new();
    for path in files {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                "run registry root contains a non-UTF-8 JSON filename",
            ));
        };
        hasher.update(name.as_bytes());
        hasher.update(&[0]);
        let bytes = fs::read(&path).map_err(|source| {
            error(
                GeneralizationErrorCode::ArtifactContract,
                format!("failed to hash run registry source file for prepared replay: {source}"),
            )
        })?;
        hasher.update(&bytes);
        hasher.update(&[0]);
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

fn validate_prepared_surface_id(
    surface: &PreparedSurfaceRecord,
    field: &str,
) -> GeneralizationResult<()> {
    let material = prepared_surface_id_material(surface, field)?;
    let derived = derive_surface_ids(&[material]).map_err(|source| {
        error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.surface_id could not be recomputed: {source:?}"),
        )
    })?;
    let expected = &derived
        .first()
        .expect("one surface_id material derives one surface_id")
        .surface_id;
    if &surface.surface_id != expected {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.surface_id does not match recomputed prepared surface material"),
        ));
    }
    Ok(())
}

fn prepared_surface_id_material(
    surface: &PreparedSurfaceRecord,
    field: &str,
) -> GeneralizationResult<SurfaceIdMaterial> {
    let view_name = prepared_surface_id_view_name(&surface.profile_id);
    let view = surface.normalized_views.get(view_name).ok_or_else(|| {
        error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.normalized_views is missing the surface_id view {view_name}"),
        )
    })?;
    Ok(SurfaceIdMaterial::new(
        surface.profile_id.clone(),
        view_name.to_string(),
        view.value.clone(),
        surface.raw_variants.clone(),
    ))
}

fn prepared_surface_id_view_name(profile_id: &str) -> &'static str {
    match profile_id {
        "cmbs_tenant_label" => "tenant_core",
        "regab_firm_identity" => "firm_core",
        _ => "core",
    }
}

fn prepared_surface_lookup_inputs(surface: &PreparedSurfaceRecord) -> Vec<String> {
    surface
        .raw_variants
        .iter()
        .filter_map(|value| {
            let trimmed = ascii_trim(value);
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn validate_prepared_exact_lookup(
    exact_lookup: &crate::entity::prepare::PreparedExactLookup,
    registry_snapshot: &crate::entity::EntityRegistrySnapshot,
    registry_replay: &PreparedRegistryReplay,
    replay_lookup_inputs: &[String],
    field: &str,
) -> GeneralizationResult<()> {
    let Some(prepared_snapshot) = exact_lookup.registry_snapshot.as_ref() else {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.exact_lookup.registry_snapshot is required"),
        ));
    };
    if prepared_snapshot.id != registry_snapshot.id
        || prepared_snapshot.version != registry_snapshot.version
        || prepared_snapshot.source != registry_snapshot.source
        || prepared_snapshot.lookup_snapshot_hash != registry_snapshot.lookup_snapshot_hash
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.exact_lookup.registry_snapshot does not match the run registry"),
        ));
    }
    if exact_lookup.lookup_inputs != replay_lookup_inputs {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "{field}.exact_lookup.lookup_inputs do not match prepared raw variant replay inputs"
            ),
        ));
    }
    match exact_lookup.status {
        PreparedExactLookupStatus::Resolved => {
            for (name, value) in [
                ("canonical_id", exact_lookup.canonical_id.as_deref()),
                ("canonical_type", exact_lookup.canonical_type.as_deref()),
                ("rule_id", exact_lookup.rule_id.as_deref()),
                ("matched_input", exact_lookup.matched_input.as_deref()),
            ] {
                if value.is_none_or(|value| ascii_trim(value).is_empty()) {
                    return Err(error(
                        GeneralizationErrorCode::ArtifactContract,
                        format!("{field}.exact_lookup.{name} is required for resolved lookup"),
                    ));
                }
            }
        }
        PreparedExactLookupStatus::Unresolved => {
            if exact_lookup.canonical_id.is_some()
                || exact_lookup.canonical_type.is_some()
                || exact_lookup.rule_id.is_some()
                || exact_lookup.matched_input.is_some()
            {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!(
                        "{field}.exact_lookup unresolved status must not carry resolved fields"
                    ),
                ));
            }
        }
    }
    validate_prepared_exact_lookup_replay(
        exact_lookup,
        registry_replay,
        replay_lookup_inputs,
        field,
    )?;
    Ok(())
}

fn validate_prepared_exact_lookup_replay(
    exact_lookup: &crate::entity::prepare::PreparedExactLookup,
    registry_replay: &PreparedRegistryReplay,
    lookup_inputs: &[String],
    field: &str,
) -> GeneralizationResult<()> {
    let replayed = replay_prepared_exact_lookup(registry_replay, lookup_inputs, field)?;
    match (exact_lookup.status, replayed) {
        (PreparedExactLookupStatus::Resolved, Some((matched_input, mapping))) => {
            if exact_lookup.matched_input.as_deref() != Some(matched_input.as_str())
                || exact_lookup.canonical_id.as_deref() != Some(mapping.canonical_id.as_str())
                || exact_lookup.canonical_type.as_deref() != Some(mapping.canonical_type.as_str())
                || exact_lookup.rule_id.as_deref() != Some(mapping.rule_id.as_str())
            {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!("{field}.exact_lookup does not match native registry replay"),
                ));
            }
        }
        (PreparedExactLookupStatus::Resolved, None) => {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field}.exact_lookup resolved status is not supported by registry replay"),
            ));
        }
        (PreparedExactLookupStatus::Unresolved, Some(_)) => {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field}.exact_lookup unresolved status conflicts with registry replay"),
            ));
        }
        (PreparedExactLookupStatus::Unresolved, None) => {}
    }
    Ok(())
}

fn replay_prepared_exact_lookup(
    registry_replay: &PreparedRegistryReplay,
    lookup_inputs: &[String],
    field: &str,
) -> GeneralizationResult<Option<(String, Mapping)>> {
    let hits = lookup_inputs
        .iter()
        .filter_map(|input| {
            registry_replay
                .mappings
                .get(input)
                .map(|mapping| (input.clone(), mapping))
        })
        .collect::<Vec<_>>();
    let Some((matched_input, first_mapping)) = hits.first() else {
        return Ok(None);
    };
    for (conflicting_input, conflicting_mapping) in hits.iter().skip(1) {
        if conflicting_mapping.canonical_id != first_mapping.canonical_id
            || conflicting_mapping.canonical_type != first_mapping.canonical_type
        {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "{field}.exact_lookup raw variants resolve to conflicting registry entries, including {matched_input} and {conflicting_input}"
                ),
            ));
        }
    }
    Ok(Some((matched_input.clone(), (*first_mapping).clone())))
}

fn validate_replacement_metadata_context(
    stage: &str,
    metadata: &EntityArtifactMetadata,
    run_metadata: &EntityArtifactMetadata,
) -> GeneralizationResult<()> {
    if metadata.profile != run_metadata.profile
        || metadata.registry_snapshot != run_metadata.registry_snapshot
        || metadata.input != run_metadata.input
        || metadata.patch_set != run_metadata.patch_set
        || metadata.namekit != run_metadata.namekit
        || metadata.patch_namespace != run_metadata.patch_namespace
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{stage} artifact metadata context does not match the baseline run"),
        ));
    }
    Ok(())
}

fn single_stage_ref(
    run: &EntityRunArtifact,
    stage_name: &str,
) -> GeneralizationResult<EntityArtifactReference> {
    let mut matches = run
        .stage_artifacts
        .iter()
        .filter(|stage| stage.stage == stage_name);
    let Some(stage) = matches.next() else {
        return Err(error(
            GeneralizationErrorCode::MissingReference,
            format!("run is missing {stage_name} stage"),
        ));
    };
    if matches.next().is_some() {
        return Err(error(
            GeneralizationErrorCode::DuplicateRecord,
            format!("run contains duplicate {stage_name} stages"),
        ));
    }
    Ok(stage_artifact_ref(stage))
}

fn require_artifact_ref(
    field: &str,
    references: &[EntityArtifactReference],
    expected: &EntityArtifactReference,
) -> GeneralizationResult<()> {
    if references.iter().any(|reference| reference == expected) {
        return Ok(());
    }
    Err(error(
        GeneralizationErrorCode::ArtifactContract,
        format!("{field} does not contain the required upstream artifact reference"),
    ))
}

#[allow(clippy::too_many_arguments)]
fn replace_native_stage_descriptor(
    stages: &mut [crate::entity::run::EntityRunStageArtifact],
    stage_name: &str,
    expected_version: &str,
    version: String,
    path: String,
    artifact_content_hash: String,
    upstream_artifacts: Vec<EntityArtifactReference>,
) -> GeneralizationResult<()> {
    if version != expected_version {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{stage_name} replacement artifact has the wrong version"),
        ));
    }
    let mut matching_indexes = stages
        .iter()
        .enumerate()
        .filter_map(|(index, stage)| (stage.stage == stage_name).then_some(index));
    let Some(index) = matching_indexes.next() else {
        return Err(error(
            GeneralizationErrorCode::MissingReference,
            format!("run is missing {stage_name} stage"),
        ));
    };
    if matching_indexes.next().is_some() {
        return Err(error(
            GeneralizationErrorCode::DuplicateRecord,
            format!("run contains duplicate {stage_name} stages"),
        ));
    }
    stages[index] = crate::entity::run::EntityRunStageArtifact {
        stage: stage_name.to_string(),
        version,
        path,
        artifact_content_hash,
        upstream_artifacts,
    };
    Ok(())
}

fn refresh_rebound_run_summary(
    run: &mut EntityRunArtifact,
    block: &BlockCandidateArtifact,
    edge: &EdgeEvidenceArtifact,
    solve: &SolveArtifact,
) -> GeneralizationResult<()> {
    run.summary.counts.insert(
        "exact_bucket_count".to_string(),
        required_summary_count(&block.summary.counts, "block", "exact_bucket_count")?,
    );
    run.summary.counts.insert(
        "candidate_pairs".to_string(),
        required_summary_count(&block.summary.counts, "block", "candidate_pairs")?,
    );
    run.summary.counts.insert(
        "evidence_records".to_string(),
        required_summary_count(&edge.summary.counts, "evidence", "evidence_records")?,
    );
    run.summary.counts.insert(
        "relation_hint_evidence".to_string(),
        required_summary_count(&edge.summary.counts, "evidence", "relation_hint_count")?,
    );
    run.summary.counts.insert(
        "solved_entities".to_string(),
        required_summary_count(&solve.summary.counts, "solve", "entity_count")?,
    );
    run.summary.counts.insert(
        "review_group_count".to_string(),
        required_summary_count(&solve.summary.counts, "solve", "review_group_count")?,
    );
    Ok(())
}

fn required_summary_count(
    counts: &BTreeMap<String, u64>,
    artifact: &str,
    key: &str,
) -> GeneralizationResult<u64> {
    counts.get(key).copied().ok_or_else(|| {
        error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{artifact} artifact summary is missing required count {key}"),
        )
    })
}

fn refresh_rebound_orchestration_refs(
    run: &mut EntityRunArtifact,
    old_refs: &BTreeMap<String, EntityArtifactReference>,
    new_refs: &BTreeMap<String, EntityArtifactReference>,
) {
    let old_to_new = old_refs
        .iter()
        .filter_map(|(stage, old_ref)| new_refs.get(stage).map(|new_ref| (old_ref, new_ref)))
        .collect::<Vec<_>>();
    let final_stage_refs = final_run_stage_refs(run);
    for step in &mut run.orchestration.handoff_steps {
        if step.stage == "audit" {
            step.input_artifacts = final_stage_refs.clone();
            continue;
        }
        for reference in &mut step.input_artifacts {
            if let Some((_, replacement)) =
                old_to_new.iter().find(|(old_ref, _)| *old_ref == reference)
            {
                *reference = (*replacement).clone();
            }
        }
    }
}

fn refresh_audit_handoff_refs_to_final_stage_refs(run: &mut EntityRunArtifact) {
    let final_stage_refs = final_run_stage_refs(run);
    for step in &mut run.orchestration.handoff_steps {
        if step.stage == "audit" {
            step.input_artifacts = final_stage_refs.clone();
        }
    }
}

fn final_run_stage_refs(run: &EntityRunArtifact) -> Vec<EntityArtifactReference> {
    run.stage_artifacts.iter().map(stage_artifact_ref).collect()
}

fn validate_leak_source_bundle_ref_for_run_binding(
    reference: &GeneralizationLeakSourceBundleRef,
) -> GeneralizationResult<()> {
    validate_leak_source_bundle_ref(reference, "leak_source_bundle")
}

fn reject_preexisting_generalization_receipts(run: &EntityRunArtifact) -> GeneralizationResult<()> {
    for stage in &run.stage_artifacts {
        if matches!(
            stage.stage.as_str(),
            "cache_receipt" | "generated_corpus_receipt"
        ) {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "run already contains generalization receipt stage {}",
                    stage.stage
                ),
            ));
        }
    }
    Ok(())
}

fn validated_native_cache_execution_stage_index(
    run: &EntityRunArtifact,
) -> GeneralizationResult<usize> {
    let mut cache_indexes = run
        .stage_artifacts
        .iter()
        .enumerate()
        .filter_map(|(index, stage)| native_cache_execution_mode_for_stage(stage).map(|_| index));
    let Some(cache_stage_index) = cache_indexes.next() else {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "run must contain one native cache_enabled/cache_disabled stage before provenance binding",
        ));
    };
    if cache_indexes.next().is_some() {
        return Err(error(
            GeneralizationErrorCode::DuplicateRecord,
            "run must not contain multiple native cache execution stages",
        ));
    }

    let stage = &run.stage_artifacts[cache_stage_index];
    let cache_mode = native_cache_execution_mode_for_stage(stage).expect("stage index filtered");
    let cache_status = native_cache_execution_status_for_mode(cache_mode, run)?;
    safe_run_stage_checked_path("run.json", &stage.path, "cache_execution_stage.path")?;
    verify_declared_digest(
        "cache_execution_stage.artifact_content_hash",
        &stage.artifact_content_hash,
    )?;
    for (index, upstream) in stage.upstream_artifacts.iter().enumerate() {
        validate_artifact_ref_fields(
            upstream,
            &format!("cache_execution_stage.upstream_artifacts[{index}]"),
        )?;
    }
    let index_stage = run
        .stage_artifacts
        .iter()
        .find(|stage| stage.stage == "index" && stage.version == CANON_ENTITY_INDEX_VERSION_V1)
        .ok_or_else(|| {
            error(
                GeneralizationErrorCode::ArtifactContract,
                "native cache execution stage requires a native index run stage",
            )
        })?;
    let index_stage_ref = stage_artifact_ref(index_stage);
    if !run.metadata.upstream_artifacts.contains(&index_stage_ref) {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "native run metadata must bind the native index artifact",
        ));
    }
    let bundle_hash = run
        .summary
        .labels
        .get("cache_bundle_receipt_hash")
        .ok_or_else(|| {
            error(
                GeneralizationErrorCode::ArtifactContract,
                "native cache execution stage requires cache_bundle_receipt_hash run label",
            )
        })?;
    verify_declared_digest("cache_bundle_receipt_hash", bundle_hash)?;
    let bundle_ref = EntityArtifactReference {
        version: CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION.to_string(),
        content_hash: bundle_hash.clone(),
    };
    if stage.upstream_artifacts != vec![index_stage_ref, bundle_ref] {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "native cache execution stage must upstream exactly the native index artifact and immutable bundle receipt",
        ));
    }
    if !run
        .orchestration
        .stage_order
        .iter()
        .any(|stage_name| stage_name == &stage.stage)
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "native cache execution stage must appear in run stage_order",
        ));
    }
    validate_native_cache_execution_labels(run, stage, cache_mode, cache_status)?;
    Ok(cache_stage_index)
}

fn native_cache_execution_mode_for_stage(
    stage: &crate::entity::run::EntityRunStageArtifact,
) -> Option<EntityIndexCacheMode> {
    if stage.version != CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION
        || stage.path != RUN_CACHE_EXECUTION_RECEIPT_PATH
    {
        return None;
    }
    match stage.stage.as_str() {
        "cache_enabled" => Some(EntityIndexCacheMode::Enabled),
        "cache_disabled" => Some(EntityIndexCacheMode::Disabled),
        _ => None,
    }
}

fn native_cache_execution_status_for_mode(
    mode: EntityIndexCacheMode,
    run: &EntityRunArtifact,
) -> GeneralizationResult<EntityIndexCacheStatus> {
    let status = run
        .summary
        .labels
        .get("cache_status")
        .map(String::as_str)
        .ok_or_else(|| {
            error(
                GeneralizationErrorCode::ArtifactContract,
                "native cache execution stage requires cache_status run label",
            )
        })?;
    let status = match status {
        "hit" => EntityIndexCacheStatus::Hit,
        "rebuilt" => EntityIndexCacheStatus::Rebuilt,
        "bypassed" => EntityIndexCacheStatus::Bypassed,
        _ => {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                "native cache execution stage has incompatible cache_status run label",
            ));
        }
    };
    let allowed = match mode {
        EntityIndexCacheMode::Enabled => {
            matches!(
                status,
                EntityIndexCacheStatus::Hit | EntityIndexCacheStatus::Rebuilt
            )
        }
        EntityIndexCacheMode::Disabled => status == EntityIndexCacheStatus::Bypassed,
    };
    if !allowed {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "native cache execution stage mode and status labels are inconsistent",
        ));
    }
    Ok(status)
}

fn validate_native_cache_execution_labels(
    run: &EntityRunArtifact,
    stage: &crate::entity::run::EntityRunStageArtifact,
    mode: EntityIndexCacheMode,
    status: EntityIndexCacheStatus,
) -> GeneralizationResult<()> {
    require_run_label(
        run,
        "cache_mode",
        mode.as_str(),
        "native cache execution stage",
    )?;
    require_run_label(
        run,
        "cache_status",
        status.as_str(),
        "native cache execution stage",
    )?;
    require_run_label(
        run,
        "cache_receipt_path",
        &stage.path,
        "native cache execution stage",
    )?;
    require_run_label(
        run,
        "cache_receipt_hash",
        &stage.artifact_content_hash,
        "native cache execution stage",
    )?;
    require_run_label(
        run,
        "cache_bundle_receipt_path",
        INDEX_CACHE_RECEIPT_FILE,
        "native cache execution stage",
    )?;
    let bundle_ref = stage
        .upstream_artifacts
        .iter()
        .find(|reference| reference.version == CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION)
        .ok_or_else(|| {
            error(
                GeneralizationErrorCode::ArtifactContract,
                "native cache execution stage must upstream the immutable bundle receipt",
            )
        })?;
    require_run_label(
        run,
        "cache_bundle_receipt_hash",
        &bundle_ref.content_hash,
        "native cache execution stage",
    )?;
    Ok(())
}

fn validated_generalization_receipt_stage(
    mut stage: crate::entity::run::EntityRunStageArtifact,
    expected_stage: &str,
    expected_class: SafeRunStageClass,
    leak_bundle_ref: &EntityArtifactReference,
    field: &str,
) -> GeneralizationResult<crate::entity::run::EntityRunStageArtifact> {
    if stage.stage != expected_stage {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.stage must be exactly {expected_stage}"),
        ));
    }
    if safe_pre_evaluation_run_stage_class(&stage.stage) != Some(expected_class) {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.stage has the wrong receipt class"),
        ));
    }
    if ascii_trim(&stage.version).is_empty() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.version must not be empty"),
        ));
    }
    safe_run_stage_checked_path("run.json", &stage.path, &format!("{field}.path"))?;
    verify_declared_digest(
        &format!("{field}.artifact_content_hash"),
        &stage.artifact_content_hash,
    )?;
    for (index, upstream) in stage.upstream_artifacts.iter().enumerate() {
        validate_artifact_ref_fields(upstream, &format!("{field}.upstream_artifacts[{index}]"))?;
    }
    merge_metadata_upstream_refs(&mut stage.upstream_artifacts, [leak_bundle_ref.clone()]);
    Ok(stage)
}

fn reject_conflicting_receipt_pair(
    cache_receipt: &crate::entity::run::EntityRunStageArtifact,
    generated_corpus_receipt: &crate::entity::run::EntityRunStageArtifact,
) -> GeneralizationResult<()> {
    if cache_receipt.stage == generated_corpus_receipt.stage {
        return Err(error(
            GeneralizationErrorCode::DuplicateRecord,
            "generalization receipt stages must be distinct",
        ));
    }
    if cache_receipt.path == generated_corpus_receipt.path {
        return Err(error(
            GeneralizationErrorCode::DuplicateRecord,
            "generalization receipt paths must be distinct",
        ));
    }
    if cache_receipt.artifact_content_hash == generated_corpus_receipt.artifact_content_hash {
        return Err(error(
            GeneralizationErrorCode::DuplicateRecord,
            "generalization receipt hashes must be distinct",
        ));
    }
    Ok(())
}

fn reject_receipt_hash_collisions<'a>(
    run: &EntityRunArtifact,
    leak_source_bundle: &GeneralizationLeakSourceBundleRef,
    receipts: impl IntoIterator<Item = &'a crate::entity::run::EntityRunStageArtifact>,
) -> GeneralizationResult<()> {
    let stage_paths = run
        .stage_artifacts
        .iter()
        .map(|stage| stage.path.as_str())
        .collect::<BTreeSet<_>>();
    let stage_hashes = run
        .stage_artifacts
        .iter()
        .map(|stage| stage.artifact_content_hash.as_str())
        .collect::<BTreeSet<_>>();
    for receipt in receipts {
        if stage_paths.contains(receipt.path.as_str()) {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "{} receipt path must not equal any preexisting run stage path",
                    receipt.stage
                ),
            ));
        }
        if receipt.artifact_content_hash == leak_source_bundle.content_hash
            || receipt.artifact_content_hash == run.artifact_content_hash
            || stage_hashes.contains(receipt.artifact_content_hash.as_str())
        {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "{} receipt hash must not equal the leak bundle, run, or any preexisting run stage hash",
                    receipt.stage
                ),
            ));
        }
    }
    Ok(())
}

fn bind_generalization_label(
    labels: &mut BTreeMap<String, String>,
    key: &str,
    value: &str,
) -> GeneralizationResult<()> {
    match labels.get(key) {
        Some(existing) if existing != value => Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("run summary label {key} conflicts with requested generalization binding"),
        )),
        Some(_) => Ok(()),
        None => {
            labels.insert(key.to_string(), value.to_string());
            Ok(())
        }
    }
}

fn stage_artifact_ref(
    stage: &crate::entity::run::EntityRunStageArtifact,
) -> EntityArtifactReference {
    artifact_ref(&stage.version, &stage.artifact_content_hash)
}

fn artifact_ref(version: &str, content_hash: &str) -> EntityArtifactReference {
    EntityArtifactReference {
        version: version.to_string(),
        content_hash: content_hash.to_string(),
    }
}

fn validate_artifact_ref_fields(
    reference: &EntityArtifactReference,
    field: &str,
) -> GeneralizationResult<()> {
    if ascii_trim(&reference.version).is_empty() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.version must not be empty"),
        ));
    }
    verify_declared_digest(&format!("{field}.content_hash"), &reference.content_hash)
}

fn merge_metadata_upstream_refs(
    upstreams: &mut Vec<EntityArtifactReference>,
    additions: impl IntoIterator<Item = EntityArtifactReference>,
) {
    upstreams.extend(additions);
    upstreams.sort_by(entity_artifact_ref_cmp);
    upstreams.dedup();
}

fn entity_artifact_ref_cmp(
    left: &EntityArtifactReference,
    right: &EntityArtifactReference,
) -> std::cmp::Ordering {
    left.version
        .cmp(&right.version)
        .then_with(|| left.content_hash.cmp(&right.content_hash))
}

fn reseal_run_artifact(run: &mut EntityRunArtifact) -> GeneralizationResult<()> {
    run.artifact_content_hash.clear();
    run.metadata.artifact_content_hash.clear();
    let content_hash = hash_serialized(run)?;
    run.artifact_content_hash = content_hash.clone();
    run.metadata.artifact_content_hash = content_hash;
    Ok(())
}

fn validate_leak_source_completeness_manifest(
    base_dir: &Path,
    source: &GeneralizationStructuredLeakSource,
    field: &str,
    max_artifact_bytes: Option<u64>,
    registry_binding_entries: &[RegistryLeakBindingEntry],
) -> GeneralizationResult<Option<RegistryCompletenessProvenanceSignature>> {
    if !is_registry_leak_source(source) {
        if source.completeness_manifest.is_some() {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field}.completeness_manifest is only valid for registry sources"),
            ));
        }
        return Ok(None);
    }
    let Some(reference) = &source.completeness_manifest else {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.completeness_manifest is required for registry sources"),
        ));
    };
    normalize_path_ref(
        &reference.path,
        &format!("{field}.completeness_manifest.path"),
    )?;
    verify_declared_digest(
        &format!("{field}.completeness_manifest.content_hash"),
        &reference.content_hash,
    )?;
    let (_, bytes) = read_strict_manifest_file(
        base_dir,
        &format!("{field}.completeness_manifest.path"),
        &reference.path,
    )?;
    validate_resource_limit(
        &format!("{field}.completeness_manifest"),
        bytes.len(),
        max_artifact_bytes,
    )?;
    verify_declared_content_hash(
        &format!("{field}.completeness_manifest.content_hash"),
        &reference.content_hash,
        &bytes,
    )?;
    let manifest: GeneralizationLeakSourceCompletenessManifest =
        serde_json::from_slice(&bytes).map_err(artifact_error)?;
    validate_registry_completeness_manifest(
        base_dir,
        source,
        &manifest,
        registry_binding_entries,
        field,
    )
    .map(Some)
}

fn validate_registry_completeness_manifest(
    base_dir: &Path,
    source: &GeneralizationStructuredLeakSource,
    manifest: &GeneralizationLeakSourceCompletenessManifest,
    registry_binding_entries: &[RegistryLeakBindingEntry],
    field: &str,
) -> GeneralizationResult<RegistryCompletenessProvenanceSignature> {
    if manifest.version != CANON_GENERALIZATION_LEAK_SCAN_SOURCES_VERSION {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.completeness_manifest.version is unsupported"),
        ));
    }
    if manifest.coverage != source.coverage {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.completeness_manifest.coverage does not match source coverage"),
        ));
    }
    normalize_path_ref(
        &manifest.root,
        &format!("{field}.completeness_manifest.root"),
    )?;
    if manifest.entries.is_empty() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.completeness_manifest.entries must not be empty"),
        ));
    }
    let manifest_entries = completeness_entry_set(
        manifest.entries.iter().cloned(),
        &format!("{field}.completeness_manifest.entries"),
    )?;
    let checked_entries = completeness_entry_set(
        source.checked_sources.iter().map(|checked_source| {
            GeneralizationLeakSourceCompletenessEntry {
                path: checked_source.path.clone(),
                format: checked_source.format,
                content_hash: checked_source.content_hash.clone(),
                byte_count: checked_source.byte_count,
                record_count: checked_source.record_count,
            }
        }),
        &format!("{field}.checked_sources"),
    )?;
    if manifest_entries != checked_entries {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.completeness_manifest.entries do not match checked sources"),
        ));
    }
    let enumerated_paths =
        enumerate_registry_snapshot_source_paths(base_dir, &manifest.root, field)?;
    let manifest_paths = manifest
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<BTreeSet<_>>();
    if enumerated_paths != manifest_paths {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.completeness_manifest omits or adds registry source files"),
        ));
    }
    let derived_binding = hash_registry_leak_binding_entries(registry_binding_entries, field)?;
    if derived_binding != source.binding_hash {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.binding_hash does not match the checked registry source tree"),
        ));
    }
    Ok(RegistryCompletenessProvenanceSignature {
        coverage: manifest.coverage,
        root: manifest.root.clone(),
        entries: manifest_entries,
    })
}

fn completeness_entry_set(
    entries: impl IntoIterator<Item = GeneralizationLeakSourceCompletenessEntry>,
    field: &str,
) -> GeneralizationResult<BTreeSet<GeneralizationLeakSourceCompletenessEntry>> {
    let mut seen = BTreeSet::new();
    for entry in entries {
        normalize_path_ref(&entry.path, &format!("{field}.path"))?;
        verify_declared_digest(&format!("{field}.content_hash"), &entry.content_hash)?;
        if entry.byte_count == 0 || entry.record_count == 0 {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field} counts must be nonzero"),
            ));
        }
        if !seen.insert(entry) {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("{field} contains duplicate entries"),
            ));
        }
    }
    Ok(seen)
}

fn validate_checked_path_reuse(
    observed_paths: &mut BTreeMap<String, ObservedCheckedPathProvenance>,
    source: &GeneralizationStructuredLeakSource,
    checked_sources: &[LoadedGeneralizationCheckedLeakSourceRef],
    completeness: Option<&RegistryCompletenessProvenanceSignature>,
    field: &str,
) -> GeneralizationResult<()> {
    for checked_source in checked_sources {
        let signature = if source_checked_path_reuse_eligible(source) {
            Some(checked_path_provenance_signature(
                source,
                checked_source,
                completeness,
                field,
            )?)
        } else {
            None
        };
        if let Some(previous) = observed_paths.get(&checked_source.path) {
            if checked_path_reuse_is_allowed(previous, source, signature.as_ref()) {
                continue;
            }
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!(
                    "{field}.checked_sources path was already used by incompatible leak source {}",
                    previous.source_id
                ),
            ));
        }
        observed_paths.insert(
            checked_source.path.clone(),
            ObservedCheckedPathProvenance {
                source_id: source.source_id.clone(),
                channel: source.channel,
                signature,
            },
        );
    }
    Ok(())
}

fn checked_path_provenance_signature(
    source: &GeneralizationStructuredLeakSource,
    checked_source: &LoadedGeneralizationCheckedLeakSourceRef,
    completeness: Option<&RegistryCompletenessProvenanceSignature>,
    field: &str,
) -> GeneralizationResult<CheckedPathProvenanceSignature> {
    let Some(completeness) = completeness else {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.completeness_manifest is required before reusing checked paths"),
        ));
    };
    Ok(CheckedPathProvenanceSignature {
        binding_kind: source.binding_kind,
        binding_hash: source.binding_hash.clone(),
        coverage: source.coverage,
        completeness: completeness.clone(),
        checked_descriptor: GeneralizationLeakSourceCompletenessEntry {
            path: checked_source.path.clone(),
            format: checked_source.format,
            content_hash: checked_source.content_hash.clone(),
            byte_count: checked_source.byte_count,
            record_count: checked_source.record_count,
        },
        derived_bytes_hash: checked_source.content_hash.clone(),
    })
}

fn checked_path_reuse_is_allowed(
    previous: &ObservedCheckedPathProvenance,
    source: &GeneralizationStructuredLeakSource,
    signature: Option<&CheckedPathProvenanceSignature>,
) -> bool {
    let (Some(previous_signature), Some(signature)) = (&previous.signature, signature) else {
        return false;
    };
    registry_alias_anchor_reuse_eligible(previous.channel)
        && source_checked_path_reuse_eligible(source)
        && previous_signature == signature
}

fn source_checked_path_reuse_eligible(source: &GeneralizationStructuredLeakSource) -> bool {
    registry_alias_anchor_reuse_eligible(source.channel) && is_registry_leak_source(source)
}

fn registry_alias_anchor_reuse_eligible(channel: LeakChannel) -> bool {
    matches!(channel, LeakChannel::Alias | LeakChannel::Anchor)
}

fn enumerate_registry_snapshot_source_paths(
    base_dir: &Path,
    root: &str,
    field: &str,
) -> GeneralizationResult<BTreeSet<String>> {
    let resolution = resolve_workspace_path(base_dir, field, Path::new(root), PlannedAccess::Read)
        .map_err(|error| {
            GeneralizationError::new(GeneralizationErrorCode::ArtifactContract, error.to_string())
        })?;
    if !resolution.exists {
        return Err(error(
            GeneralizationErrorCode::MissingReference,
            format!("{field}.completeness_manifest.root does not exist"),
        ));
    }
    if resolution.leaf_is_symlink {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.completeness_manifest.root must not be a symlink"),
        ));
    }
    let metadata = fs::metadata(&resolution.absolute_path)
        .map_err(|io_error| manifest_io_error(field, io_error))?;
    if !metadata.is_dir() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.completeness_manifest.root must be a directory"),
        ));
    }
    let root_prefix = root.trim_end_matches('/');
    let mut paths = BTreeSet::new();
    for entry in fs::read_dir(&resolution.absolute_path)
        .map_err(|io_error| manifest_io_error(field, io_error))?
    {
        let entry = entry.map_err(|io_error| manifest_io_error(field, io_error))?;
        let file_type = entry
            .file_type()
            .map_err(|io_error| manifest_io_error(field, io_error))?;
        if file_type.is_symlink() {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field}.completeness_manifest.root must not contain symlinks"),
            ));
        }
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field}.completeness_manifest.root contains a non-UTF-8 path"),
            ));
        };
        paths.insert(format!("{root_prefix}/{file_name}"));
    }
    if paths.is_empty() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.completeness_manifest.root must enumerate registry JSON files"),
        ));
    }
    Ok(paths)
}

fn hash_registry_leak_binding_entries(
    entries: &[RegistryLeakBindingEntry],
    field: &str,
) -> GeneralizationResult<String> {
    if entries.is_empty() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.checked_sources must include registry source bytes"),
        ));
    }
    let mut sorted = entries.to_vec();
    sorted.sort_by(|left, right| left.path.cmp(&right.path));
    let mut hasher = blake3::Hasher::new();
    let mut seen_names = BTreeSet::new();
    for entry in sorted {
        let Some(name) = Path::new(&entry.path)
            .file_name()
            .and_then(|name| name.to_str())
        else {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field}.checked_sources contains a non-UTF-8 registry path"),
            ));
        };
        if !seen_names.insert(name.to_string()) {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("{field}.checked_sources contains duplicate registry file names"),
            ));
        }
        hasher.update(name.as_bytes());
        hasher.update(&[0]);
        hasher.update(&entry.bytes);
        hasher.update(&[0]);
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

fn validate_derived_leak_source_binding(
    source: &GeneralizationStructuredLeakSource,
    checked_sources: &[LoadedGeneralizationCheckedLeakSourceRef],
    registry_binding_entries: &[RegistryLeakBindingEntry],
    allowed: &AllowedLeakSourceBindings,
    field: &str,
) -> GeneralizationResult<()> {
    if is_registry_leak_source(source) {
        let derived_binding = hash_registry_leak_binding_entries(registry_binding_entries, field)?;
        if derived_binding != source.binding_hash {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field}.binding_hash does not match derived registry bytes"),
            ));
        }
        return Ok(());
    }
    if let Some(stage_class) = required_safe_run_stage_class(source) {
        validate_safe_run_stage_checked_source(
            source,
            checked_sources,
            allowed,
            field,
            stage_class,
        )?;
        return Ok(());
    }
    let source_content_bound = source.content_hash == source.binding_hash
        || checked_sources
            .iter()
            .any(|checked| checked.content_hash == source.binding_hash);
    if !source_content_bound {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "{field}.binding_hash must match the projection hash or one checked source hash"
            ),
        ));
    }
    Ok(())
}

fn is_registry_leak_source(source: &GeneralizationStructuredLeakSource) -> bool {
    matches!(
        source.source_kind,
        GeneralizationLeakSourceKind::RegistryTree
            | GeneralizationLeakSourceKind::RegistryAliasFile
            | GeneralizationLeakSourceKind::RegistryAnchorFile
    ) || matches!(source.channel, LeakChannel::Alias | LeakChannel::Anchor)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SafeRunStageClass {
    Cache,
    GeneratedCorpus,
}

impl SafeRunStageClass {
    const fn source_label(self) -> &'static str {
        match self {
            Self::Cache => "cache",
            Self::GeneratedCorpus => "generated-corpus",
        }
    }
}

fn required_safe_run_stage_class(
    source: &GeneralizationStructuredLeakSource,
) -> Option<SafeRunStageClass> {
    match (source.channel, source.source_kind, source.binding_kind) {
        (
            LeakChannel::Cache,
            GeneralizationLeakSourceKind::Cache,
            GeneralizationLeakSourceBindingKind::RunStageArtifact,
        ) => Some(SafeRunStageClass::Cache),
        (
            LeakChannel::GeneratedCorpus,
            GeneralizationLeakSourceKind::GeneratedCorpus,
            GeneralizationLeakSourceBindingKind::RunStageArtifact,
        ) => Some(SafeRunStageClass::GeneratedCorpus),
        _ => None,
    }
}

fn safe_pre_evaluation_run_stage_class(stage: &str) -> Option<SafeRunStageClass> {
    match ascii_trim(stage) {
        "cache_enabled" | "cache_disabled" => Some(SafeRunStageClass::Cache),
        "generated_corpus_receipt" => Some(SafeRunStageClass::GeneratedCorpus),
        _ => None,
    }
}

fn validate_safe_run_stage_checked_source(
    source: &GeneralizationStructuredLeakSource,
    checked_sources: &[LoadedGeneralizationCheckedLeakSourceRef],
    allowed: &AllowedLeakSourceBindings,
    field: &str,
    stage_class: SafeRunStageClass,
) -> GeneralizationResult<()> {
    let Some(stage) = allowed.safe_pre_evaluation_run_stage(stage_class, &source.binding_hash)
    else {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "{field}.binding_hash must be a safe {} pre-evaluation run stage",
                stage_class.source_label()
            ),
        ));
    };
    if stage.class != stage_class {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.binding_hash resolved to the wrong safe run stage class"),
        ));
    }
    if checked_sources.len() != 1 {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "{field}.checked_sources must name exactly one {} stage receipt",
                stage_class.source_label()
            ),
        ));
    }
    let checked = &checked_sources[0];
    if checked.path != stage.path || checked.content_hash != stage.content_hash {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "{field}.checked_sources must match the bound {} run stage path and hash",
                stage.stage
            ),
        ));
    }
    Ok(())
}

fn safe_run_stage_checked_path(
    run_ref_path: &str,
    stage_path: &str,
    field: &str,
) -> GeneralizationResult<String> {
    normalize_path_ref(stage_path, field)?;
    let stage_path_ref = Path::new(stage_path);
    if stage_path_ref.is_absolute()
        || stage_path_ref.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::ParentDir
            )
        })
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must be a run-relative safe path"),
        ));
    }
    sibling_manifest_path(run_ref_path, stage_path)
}

#[derive(Debug, Clone)]
struct SafeRunStageBinding {
    class: SafeRunStageClass,
    stage: String,
    path: String,
    content_hash: String,
}

#[derive(Debug, Default)]
struct AllowedLeakSourceBindings {
    by_kind: BTreeMap<GeneralizationLeakSourceBindingKind, BTreeSet<String>>,
    safe_pre_evaluation_run_stages: BTreeMap<(SafeRunStageClass, String), SafeRunStageBinding>,
}

impl AllowedLeakSourceBindings {
    fn insert(&mut self, kind: GeneralizationLeakSourceBindingKind, hash: impl Into<String>) {
        let hash = hash.into();
        if is_blake3_digest(&hash) {
            self.by_kind.entry(kind).or_default().insert(hash);
        }
    }

    fn contains(&self, kind: GeneralizationLeakSourceBindingKind, hash: &str) -> bool {
        self.by_kind
            .get(&kind)
            .is_some_and(|hashes| hashes.contains(hash))
    }

    fn insert_safe_pre_evaluation_run_stage(
        &mut self,
        run_ref_path: &str,
        stage: &crate::entity::run::EntityRunStageArtifact,
    ) -> GeneralizationResult<()> {
        let Some(class) = safe_pre_evaluation_run_stage_class(&stage.stage) else {
            return Ok(());
        };
        let checked_path =
            safe_run_stage_checked_path(run_ref_path, &stage.path, "run.stage_artifacts.path")?;
        verify_declared_digest(
            "run.stage_artifacts.artifact_content_hash",
            &stage.artifact_content_hash,
        )?;
        self.safe_pre_evaluation_run_stages.insert(
            (class, stage.artifact_content_hash.clone()),
            SafeRunStageBinding {
                class,
                stage: stage.stage.clone(),
                path: checked_path,
                content_hash: stage.artifact_content_hash.clone(),
            },
        );
        Ok(())
    }

    fn safe_pre_evaluation_run_stage(
        &self,
        class: SafeRunStageClass,
        hash: &str,
    ) -> Option<&SafeRunStageBinding> {
        self.safe_pre_evaluation_run_stages
            .get(&(class, hash.to_string()))
    }
}

fn allowed_leak_source_bindings(
    trial: &GeneralizationTrialExecution,
    _candidate_recall: &LoadedGeneralizationCandidateRecall,
    artifacts: &[LoadedGeneralizationArtifactRef],
) -> GeneralizationResult<AllowedLeakSourceBindings> {
    let mut allowed = AllowedLeakSourceBindings::default();
    allowed.insert(
        GeneralizationLeakSourceBindingKind::RegistrySnapshot,
        trial.cross_bindings.registry_snapshot_hash.clone(),
    );
    let run_ref = loaded_run_artifact_ref(artifacts)?;
    let run = loaded_run_artifact(artifacts)?;
    insert_metadata_leak_bindings(&mut allowed, &run.metadata);
    for stage in &run.stage_artifacts {
        allowed.insert_safe_pre_evaluation_run_stage(&run_ref.reference.path, stage)?;
    }

    let link = loaded_link_artifact(artifacts)?;
    insert_metadata_leak_bindings(&mut allowed, &link.metadata);

    let solve = loaded_solve_artifact(artifacts)?;
    insert_metadata_leak_bindings(&mut allowed, &solve.metadata);
    Ok(allowed)
}

fn insert_metadata_leak_bindings(
    allowed: &mut AllowedLeakSourceBindings,
    metadata: &crate::entity::EntityArtifactMetadata,
) {
    allowed.insert(
        GeneralizationLeakSourceBindingKind::Strategy,
        metadata.strategy.content_hash.clone(),
    );
    allowed.insert(
        GeneralizationLeakSourceBindingKind::RegistrySnapshot,
        metadata.registry_snapshot.lookup_snapshot_hash.clone(),
    );
    if let Some(sidecar_hash) = &metadata.registry_snapshot.sidecar_snapshot_hash {
        allowed.insert(
            GeneralizationLeakSourceBindingKind::RegistrySidecarSnapshot,
            sidecar_hash.clone(),
        );
    }
    if let Some(profile_hash) = &metadata.profile.content_hash {
        allowed.insert(
            GeneralizationLeakSourceBindingKind::Profile,
            profile_hash.clone(),
        );
    }
    if let Some(input) = &metadata.input {
        allowed.insert(
            GeneralizationLeakSourceBindingKind::Input,
            input.content_hash.clone(),
        );
    }
    if let Some(patch_set) = &metadata.patch_set {
        allowed.insert(
            GeneralizationLeakSourceBindingKind::PatchSet,
            patch_set.content_hash.clone(),
        );
    }
    if let Some(namekit) = &metadata.namekit {
        allowed.insert(
            GeneralizationLeakSourceBindingKind::Namekit,
            namekit.content_hash.clone(),
        );
    }
}

fn require_run_binds_leak_source_bundle(
    run: &EntityRunArtifact,
    reference: &GeneralizationLeakSourceBundleRef,
    field: &str,
) -> GeneralizationResult<()> {
    let is_bundle_ref = |version: &str, content_hash: &str| {
        version == reference.version && content_hash == reference.content_hash
    };
    let bound_by_metadata = run
        .metadata
        .upstream_artifacts
        .iter()
        .any(|upstream| is_bundle_ref(&upstream.version, &upstream.content_hash));
    let bound_by_stage = run.stage_artifacts.iter().any(|stage| {
        is_bundle_ref(&stage.version, &stage.artifact_content_hash)
            || stage
                .upstream_artifacts
                .iter()
                .any(|upstream| is_bundle_ref(&upstream.version, &upstream.content_hash))
    });
    if !bound_by_metadata && !bound_by_stage {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.content_hash is not explicitly bound by the run artifact"),
        ));
    }
    Ok(())
}

fn validate_leak_source_binding(
    source: &GeneralizationStructuredLeakSource,
    allowed: &AllowedLeakSourceBindings,
    field: &str,
) -> GeneralizationResult<()> {
    verify_declared_digest(&format!("{field}.binding_hash"), &source.binding_hash)?;
    validate_leak_source_binding_kind_for_channel(
        source.source_kind,
        source.channel,
        source.binding_kind,
        &format!("{field}.binding_kind"),
    )?;
    if let Some(stage_class) = required_safe_run_stage_class(source) {
        if allowed
            .safe_pre_evaluation_run_stage(stage_class, &source.binding_hash)
            .is_none()
        {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "{field}.binding_hash is not allowed by a safe {} run stage",
                    stage_class.source_label()
                ),
            ));
        }
        return Ok(());
    }
    if !allowed.contains(source.binding_kind, &source.binding_hash) {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.binding_hash is not allowed by the validated trial chain"),
        ));
    }
    Ok(())
}

fn validate_leak_source_binding_kind_for_channel(
    source_kind: GeneralizationLeakSourceKind,
    channel: LeakChannel,
    binding_kind: GeneralizationLeakSourceBindingKind,
    field: &str,
) -> GeneralizationResult<()> {
    let allowed = match channel {
        LeakChannel::Alias | LeakChannel::Anchor => {
            matches!(
                source_kind,
                GeneralizationLeakSourceKind::RegistryTree
                    | GeneralizationLeakSourceKind::RegistryAliasFile
                    | GeneralizationLeakSourceKind::RegistryAnchorFile
            ) && matches!(
                binding_kind,
                GeneralizationLeakSourceBindingKind::RegistrySnapshot
            )
        }
        LeakChannel::Threshold => {
            source_kind == GeneralizationLeakSourceKind::Threshold
                && matches!(
                    binding_kind,
                    GeneralizationLeakSourceBindingKind::Strategy
                        | GeneralizationLeakSourceBindingKind::Profile
                )
        }
        LeakChannel::Dictionary => {
            source_kind == GeneralizationLeakSourceKind::Dictionary
                && matches!(
                    binding_kind,
                    GeneralizationLeakSourceBindingKind::Strategy
                        | GeneralizationLeakSourceBindingKind::Profile
                        | GeneralizationLeakSourceBindingKind::Namekit
                )
        }
        LeakChannel::Patch => {
            source_kind == GeneralizationLeakSourceKind::Patch
                && binding_kind == GeneralizationLeakSourceBindingKind::PatchSet
        }
        LeakChannel::Cache => {
            source_kind == GeneralizationLeakSourceKind::Cache
                && binding_kind == GeneralizationLeakSourceBindingKind::RunStageArtifact
        }
        LeakChannel::GeneratedCorpus => {
            source_kind == GeneralizationLeakSourceKind::GeneratedCorpus
                && binding_kind == GeneralizationLeakSourceBindingKind::RunStageArtifact
        }
    };
    if !allowed {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} is not valid for the leak source channel and kind"),
        ));
    }
    Ok(())
}

fn derive_leak_projection_records(
    format: GeneralizationLeakSourceFormat,
    bytes: &[u8],
    field: &str,
) -> GeneralizationResult<Vec<Value>> {
    match format {
        GeneralizationLeakSourceFormat::Json => derive_json_leak_projection_records(bytes, field),
        GeneralizationLeakSourceFormat::Jsonl => derive_jsonl_leak_projection_records(bytes, field),
        GeneralizationLeakSourceFormat::Csv => derive_csv_leak_projection_records(bytes, field),
        GeneralizationLeakSourceFormat::Text => derive_text_leak_projection_records(bytes, field),
        GeneralizationLeakSourceFormat::Binary => Ok(vec![serde_json::json!({
            "binary_content_hash": hash_bytes(bytes),
            "byte_count": bytes.len() as u64,
        })]),
    }
}

fn derive_json_leak_projection_records(
    bytes: &[u8],
    field: &str,
) -> GeneralizationResult<Vec<Value>> {
    let value: Value = serde_json::from_slice(bytes).map_err(artifact_error)?;
    Ok(match value {
        Value::Array(records) => records,
        Value::Object(mut object) => {
            if let Some(Value::Array(records)) = object.remove("canonical_inline_records") {
                records
            } else if let Some(Value::Array(records)) = object.remove("records") {
                records
            } else {
                vec![Value::Object(object)]
            }
        }
        Value::Null => {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field} must not derive null leak projection"),
            ));
        }
        scalar => vec![scalar],
    })
}

fn derive_jsonl_leak_projection_records(
    bytes: &[u8],
    field: &str,
) -> GeneralizationResult<Vec<Value>> {
    let text = std::str::from_utf8(bytes).map_err(|error| {
        GeneralizationError::new(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} is not valid UTF-8 JSONL: {error}"),
        )
    })?;
    let mut records = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str(line).map_err(|error| {
            GeneralizationError::new(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field} line {} is not valid JSON: {error}", line_index + 1),
            )
        })?;
        records.push(value);
    }
    Ok(records)
}

fn derive_csv_leak_projection_records(
    bytes: &[u8],
    field: &str,
) -> GeneralizationResult<Vec<Value>> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(bytes);
    let mut records = Vec::new();
    for result in reader.records() {
        let record = result.map_err(|error| {
            GeneralizationError::new(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field} CSV record could not be parsed: {error}"),
            )
        })?;
        records.push(Value::Array(
            record
                .iter()
                .map(|field| Value::String(field.to_string()))
                .collect(),
        ));
    }
    Ok(records)
}

fn derive_text_leak_projection_records(
    bytes: &[u8],
    field: &str,
) -> GeneralizationResult<Vec<Value>> {
    let text = std::str::from_utf8(bytes).map_err(|error| {
        GeneralizationError::new(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} is not valid UTF-8 text: {error}"),
        )
    })?;
    Ok(text
        .lines()
        .map(ascii_trim)
        .filter(|line| !line.is_empty())
        .map(|line| Value::String(line.to_string()))
        .collect())
}

fn validate_execution_envelope_contract(
    envelope: &GeneralizationExecutionEnvelope,
) -> GeneralizationResult<()> {
    if envelope.version != CANON_GENERALIZATION_EXECUTION_ENVELOPE_VERSION {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "strict execution envelope has unsupported version {}",
                envelope.version
            ),
        ));
    }
    if envelope.execution.path_resolver != "fs_safety::resolve_workspace_path"
        && envelope.execution.path_resolver != "crate::fs_safety::resolve_workspace_path"
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "strict execution envelope must use crate::fs_safety::resolve_workspace_path",
        ));
    }
    if envelope.execution.self_attested_outcomes_used {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "strict execution envelope must not use self-attested outcomes",
        ));
    }
    if !envelope.execution.canonical_time_parsing {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "strict execution envelope must enable canonical time parsing",
        ));
    }
    validate_execution_guarantees(&envelope.execution)?;
    if envelope.benchmark.role != "gold_only" {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "strict execution envelope benchmark role must be gold_only",
        ));
    }
    verify_declared_digest("benchmark.content_hash", &envelope.benchmark.content_hash)?;
    if envelope.trials.is_empty() {
        return Err(error(
            GeneralizationErrorCode::MissingReference,
            "strict execution envelope must declare trial executions",
        ));
    }
    require_unique_binding_keys(
        envelope
            .trials
            .iter()
            .map(|trial| (trial_family_str(trial.family), trial.trial_id.as_str())),
        "trials",
    )?;
    for (index, trial) in envelope.trials.iter().enumerate() {
        validate_trial_execution_contract(trial, &format!("trials[{index}]"))?;
    }
    if let Some(max_artifact_count) = envelope.execution.max_artifact_count {
        let declared_count = envelope
            .trials
            .iter()
            .map(|trial| trial.artifacts.len() + 12)
            .sum::<usize>();
        if declared_count > max_artifact_count as usize {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                "strict execution envelope exceeds max_artifact_count",
            ));
        }
    }
    Ok(())
}

fn validate_trial_execution_contract(
    trial: &GeneralizationTrialExecution,
    field: &str,
) -> GeneralizationResult<()> {
    normalize_non_empty(trial.trial_id.clone(), &format!("{field}.trial_id"))?;
    normalize_path_ref(&trial.registry_dir, &format!("{field}.registry_dir"))?;
    validate_cross_bindings(&trial.cross_bindings)?;
    validate_candidate_recall_execution_refs(&trial.candidate_recall)?;
    validate_solve_derivation_refs(&trial.solve_derivation)?;
    validate_cache_execution_ref(&trial.cache_execution, &format!("{field}.cache_execution"))?;
    validate_execution_bindings_shallow(&trial.trial_id, &trial.bindings)?;
    if trial.artifacts.is_empty() {
        return Err(error(
            GeneralizationErrorCode::MissingReference,
            format!("{field} must declare execution artifacts"),
        ));
    }
    let mut kinds = BTreeSet::new();
    for (index, reference) in trial.artifacts.iter().enumerate() {
        validate_artifact_ref(reference, &format!("{field}.artifacts[{index}]"))?;
        kinds.insert(infer_artifact_kind(
            reference,
            &format!("{field}.artifacts[{index}]"),
        )?);
    }
    for required in [
        GeneralizationArtifactKind::Link,
        GeneralizationArtifactKind::LinkObservationSurfaceBindings,
        GeneralizationArtifactKind::Run,
        GeneralizationArtifactKind::Solve,
    ] {
        if !kinds.contains(&required) {
            return Err(error(
                GeneralizationErrorCode::MissingReference,
                format!("{field} is missing {required:?} artifact"),
            ));
        }
    }
    validate_leak_source_bundle_ref(
        &trial.leak_scan_sources,
        &format!("{field}.leak_scan_sources"),
    )?;
    Ok(())
}

fn validate_trial_execution_coverage(
    benchmark: &GeneralizationBenchmark,
    trials: &[GeneralizationTrialExecution],
) -> GeneralizationResult<()> {
    let expected = benchmark_trial_keys(benchmark);
    let mut actual = BTreeSet::new();
    for trial in trials {
        let key = trial_execution_key(trial);
        if !actual.insert(key.clone()) {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("duplicate execution chain for {:?} trial {}", key.0, key.1),
            ));
        }
    }
    if actual != expected {
        let missing = expected.difference(&actual).cloned().collect::<Vec<_>>();
        let extra = actual.difference(&expected).cloned().collect::<Vec<_>>();
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "strict execution trials must exactly match benchmark trials; missing={missing:?} extra={extra:?}"
            ),
        ));
    }
    Ok(())
}

fn benchmark_trial_keys(
    benchmark: &GeneralizationBenchmark,
) -> BTreeSet<(GeneralizationTrialFamily, String)> {
    benchmark
        .entity_disjoint_trials
        .iter()
        .map(|trial| {
            (
                GeneralizationTrialFamily::EntityDisjoint,
                trial.trial_id.clone(),
            )
        })
        .chain(benchmark.time_forward_trials.iter().map(|trial| {
            (
                GeneralizationTrialFamily::TimeForward,
                trial.trial_id.clone(),
            )
        }))
        .collect()
}

fn trial_execution_key(
    trial: &GeneralizationTrialExecution,
) -> (GeneralizationTrialFamily, String) {
    (trial.family, trial.trial_id.clone())
}

fn trial_family_str(family: GeneralizationTrialFamily) -> &'static str {
    match family {
        GeneralizationTrialFamily::EntityDisjoint => "entity_disjoint",
        GeneralizationTrialFamily::TimeForward => "time_forward",
    }
}

fn load_generalization_benchmark_ref(
    base_dir: &Path,
    reference: &GeneralizationBenchmarkRef,
) -> GeneralizationResult<(GeneralizationBenchmark, String)> {
    let (_, bytes) = read_strict_manifest_file(base_dir, "benchmark.path", &reference.path)?;
    verify_declared_content_hash("benchmark.content_hash", &reference.content_hash, &bytes)?;
    let benchmark: GeneralizationBenchmark =
        serde_json::from_slice(&bytes).map_err(artifact_error)?;
    let benchmark = finalize_benchmark(benchmark)?;
    if benchmark.version != CANON_GENERALIZATION_VERSION {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "loaded benchmark has the wrong version",
        ));
    }
    Ok((benchmark, hash_bytes(&bytes)))
}

fn validate_execution_guarantees(
    execution: &GeneralizationExecutionContract,
) -> GeneralizationResult<()> {
    let required = BTreeSet::from([
        GeneralizationRequiredRefusal::Traversal,
        GeneralizationRequiredRefusal::Symlink,
        GeneralizationRequiredRefusal::Missing,
        GeneralizationRequiredRefusal::StaleHash,
        GeneralizationRequiredRefusal::VersionMismatch,
        GeneralizationRequiredRefusal::NoncanonicalArtifact,
    ]);
    let declared = execution
        .required_refusals
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if declared != required {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "strict execution envelope must declare all required refusal classes",
        ));
    }
    if !execution.derive_observations
        || !execution.derive_candidate_ranks
        || !execution.derive_evidence_lanes
        || !execution.derive_hard_negative_outcomes
        || !execution.recompute_leakage
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "strict execution envelope must derive observations, ranks, evidence lanes, hard negatives, and leakage",
        ));
    }
    Ok(())
}

fn validate_cross_bindings(bindings: &GeneralizationCrossBindings) -> GeneralizationResult<()> {
    normalize_non_empty(bindings.benchmark_id.clone(), "cross_bindings.benchmark_id")?;
    normalize_non_empty(bindings.run_id.clone(), "cross_bindings.run_id")?;
    normalize_digest(
        bindings.policy_digest.clone(),
        "cross_bindings.policy_digest",
    )?;
    normalize_non_empty(bindings.registry_id.clone(), "cross_bindings.registry_id")?;
    normalize_non_empty(
        bindings.registry_version.clone(),
        "cross_bindings.registry_version",
    )?;
    normalize_digest(
        bindings.registry_snapshot_hash.clone(),
        "cross_bindings.registry_snapshot_hash",
    )?;
    normalize_non_empty(
        bindings.observation_namespace.clone(),
        "cross_bindings.observation_namespace",
    )?;
    let required = BTreeSet::from([
        "trial_id",
        "observation_id",
        "surface_id",
        "surface_binding_hash",
        "result_id",
        "directional_link_id",
        "gold_pair_id",
        "solve_disposition",
        "component_id",
        "run_id",
        "policy_digest",
        "registry_snapshot_hash",
    ]);
    let declared = bindings
        .required_identity_links
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if !required.is_subset(&declared) {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "cross_bindings.required_identity_links is missing required continuity keys",
        ));
    }
    Ok(())
}

fn validate_execution_bindings_shallow(
    trial_id: &str,
    bindings: &GeneralizationExecutionBindings,
) -> GeneralizationResult<()> {
    require_unique_binding_keys(
        bindings
            .observation_bindings
            .iter()
            .map(|binding| (binding.trial_id.as_str(), binding.observation_id.as_str())),
        "observation_bindings",
    )?;
    require_unique_binding_keys(
        bindings
            .result_bindings
            .iter()
            .map(|binding| (binding.trial_id.as_str(), binding.result_id.as_str())),
        "result_bindings",
    )?;
    require_unique_binding_keys(
        bindings.directional_link_bindings.iter().map(|binding| {
            (
                binding.trial_id.as_str(),
                binding.directional_link_id.as_str(),
            )
        }),
        "directional_link_bindings",
    )?;
    require_unique_binding_keys(
        bindings
            .hard_negative_bindings
            .iter()
            .map(|binding| (binding.trial_id.as_str(), binding.control_id.as_str())),
        "hard_negative_bindings",
    )?;

    for binding in &bindings.observation_bindings {
        normalize_non_empty(binding.trial_id.clone(), "observation_bindings.trial_id")?;
        require_binding_trial_id(trial_id, &binding.trial_id, "observation_bindings")?;
        normalize_non_empty(
            binding.observation_id.clone(),
            "observation_bindings.observation_id",
        )?;
        normalize_non_empty(
            binding.surface_id.clone(),
            "observation_bindings.surface_id",
        )?;
        normalize_digest(
            binding.surface_binding_hash.clone(),
            "observation_bindings.surface_binding_hash",
        )?;
        normalize_non_empty(
            binding.profile_id.clone(),
            "observation_bindings.profile_id",
        )?;
        if let Some(source_row_id) = &binding.source_row_id {
            normalize_non_empty(source_row_id.clone(), "observation_bindings.source_row_id")?;
        }
    }
    for binding in &bindings.result_bindings {
        normalize_non_empty(binding.trial_id.clone(), "result_bindings.trial_id")?;
        require_binding_trial_id(trial_id, &binding.trial_id, "result_bindings")?;
        normalize_non_empty(binding.result_id.clone(), "result_bindings.result_id")?;
        if binding.observation_ids.is_empty() || binding.surface_ids.is_empty() {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                "result bindings must include observation_ids and surface_ids",
            ));
        }
        normalize_string_vec(
            binding.observation_ids.clone(),
            "result_bindings.observation_ids",
        )?;
        normalize_string_vec(binding.surface_ids.clone(), "result_bindings.surface_ids")?;
        if let Some(gold_pair_id) = &binding.candidate_gold_pair_id {
            normalize_non_empty(
                gold_pair_id.clone(),
                "result_bindings.candidate_gold_pair_id",
            )?;
        }
        if let Some(pair_observations) = &binding.candidate_pair_observation_ids {
            let normalized = normalize_string_vec(
                pair_observations.clone(),
                "result_bindings.candidate_pair_observation_ids",
            )?;
            if normalized.len() != 2 {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    "candidate_pair_observation_ids must contain exactly two observations",
                ));
            }
        }
        validate_solve_disposition(
            &binding.solve_disposition,
            "result_bindings.solve_disposition",
        )?;
    }
    for binding in &bindings.directional_link_bindings {
        normalize_non_empty(
            binding.trial_id.clone(),
            "directional_link_bindings.trial_id",
        )?;
        require_binding_trial_id(trial_id, &binding.trial_id, "directional_link_bindings")?;
        normalize_non_empty(
            binding.directional_link_id.clone(),
            "directional_link_bindings.directional_link_id",
        )?;
        normalize_non_empty(
            binding.gold_pair_id.clone(),
            "directional_link_bindings.gold_pair_id",
        )?;
        normalize_non_empty(
            binding.reference_observation_id.clone(),
            "directional_link_bindings.reference_observation_id",
        )?;
        normalize_non_empty(
            binding.target_observation_id.clone(),
            "directional_link_bindings.target_observation_id",
        )?;
        normalize_non_empty(
            binding.reference_surface_id.clone(),
            "directional_link_bindings.reference_surface_id",
        )?;
        normalize_non_empty(
            binding.target_surface_id.clone(),
            "directional_link_bindings.target_surface_id",
        )?;
        validate_solve_disposition(
            &binding.solve_disposition,
            "directional_link_bindings.solve_disposition",
        )?;
    }
    for binding in &bindings.hard_negative_bindings {
        normalize_non_empty(binding.trial_id.clone(), "hard_negative_bindings.trial_id")?;
        require_binding_trial_id(trial_id, &binding.trial_id, "hard_negative_bindings")?;
        normalize_non_empty(
            binding.control_id.clone(),
            "hard_negative_bindings.control_id",
        )?;
        normalize_non_empty(
            binding.left_observation_id.clone(),
            "hard_negative_bindings.left_observation_id",
        )?;
        normalize_non_empty(
            binding.right_observation_id.clone(),
            "hard_negative_bindings.right_observation_id",
        )?;
        normalize_non_empty(
            binding.left_surface_id.clone(),
            "hard_negative_bindings.left_surface_id",
        )?;
        normalize_non_empty(
            binding.right_surface_id.clone(),
            "hard_negative_bindings.right_surface_id",
        )?;
        validate_solve_disposition(
            &binding.left_solve_disposition,
            "hard_negative_bindings.left_solve_disposition",
        )?;
        validate_solve_disposition(
            &binding.right_solve_disposition,
            "hard_negative_bindings.right_solve_disposition",
        )?;
    }
    Ok(())
}

fn require_binding_trial_id(expected: &str, actual: &str, field: &str) -> GeneralizationResult<()> {
    if actual != expected {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} contains binding for another trial"),
        ));
    }
    Ok(())
}

fn validate_solve_disposition(
    disposition: &GeneralizationSolveDisposition,
    field: &str,
) -> GeneralizationResult<()> {
    match disposition {
        GeneralizationSolveDisposition::Present {
            component_id,
            state: _,
        } => {
            normalize_non_empty(component_id.clone(), &format!("{field}.component_id"))?;
        }
        GeneralizationSolveDisposition::Absent => {}
    }
    Ok(())
}

fn require_unique_binding_keys<'a>(
    keys: impl Iterator<Item = (&'a str, &'a str)>,
    field: &str,
) -> GeneralizationResult<()> {
    let mut seen = BTreeSet::new();
    for (trial_id, local_id) in keys {
        let key = (trial_id.to_string(), local_id.to_string());
        if !seen.insert(key) {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("{field} contains duplicate binding keys"),
            ));
        }
    }
    Ok(())
}

fn validate_loaded_execution_continuity(
    base_dir: &Path,
    benchmark: &GeneralizationBenchmark,
    trial: &GeneralizationTrialExecution,
    registry_dir: &Path,
    candidate_recall: &LoadedGeneralizationCandidateRecall,
    solve_derivation: &LoadedGeneralizationSolveDerivation,
    artifacts: &[LoadedGeneralizationArtifactRef],
) -> GeneralizationResult<()> {
    if benchmark.benchmark_id != trial.cross_bindings.benchmark_id {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "benchmark_id does not match cross_bindings",
        ));
    }
    if benchmark.policy_digest != trial.cross_bindings.policy_digest {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "policy_digest does not match cross_bindings",
        ));
    }

    let link = loaded_link_artifact(artifacts)?;
    let run_ref = loaded_run_artifact_ref(artifacts)?;
    let run = loaded_run_artifact(artifacts)?;
    let solve = loaded_solve_artifact(artifacts)?;
    let bindings = loaded_observation_surface_bindings(artifacts)?;
    if let Some(loaded_candidate_report) = maybe_loaded_candidate_recall_artifact(artifacts)
        && loaded_candidate_report != &candidate_recall.report
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "standalone candidate-recall artifact does not match recomputed report",
        ));
    }

    if link.shared_run_artifact.version != run.version
        || link.shared_run_artifact.content_hash != run.artifact_content_hash
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "link artifact does not bind the loaded run artifact",
        ));
    }
    if link.shared_solve_artifact.version != solve.version
        || link.shared_solve_artifact.content_hash != solve.artifact_content_hash
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "link artifact does not bind the loaded solve artifact",
        ));
    }
    let run_solve_stage = run.stage_artifacts.iter().any(|stage| {
        stage.stage == "solve"
            && stage.version == solve.version
            && stage.artifact_content_hash == solve.artifact_content_hash
    });
    if !run_solve_stage {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "run artifact solve stage does not bind the loaded solve artifact",
        ));
    }
    validate_solve_derivation_path_continuity(&run_ref.reference.path, run, solve_derivation)?;
    validate_loaded_run_solve_rebuild(
        run,
        registry_dir,
        solve,
        candidate_recall,
        solve_derivation,
    )?;
    if solve.metadata.registry_snapshot.id != trial.cross_bindings.registry_id
        || run.metadata.registry_snapshot.id != trial.cross_bindings.registry_id
        || link.metadata.registry_snapshot.id != trial.cross_bindings.registry_id
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "registry id continuity failed across benchmark/run/link/solve",
        ));
    }
    if solve.metadata.registry_snapshot.version != trial.cross_bindings.registry_version
        || run.metadata.registry_snapshot.version != trial.cross_bindings.registry_version
        || link.metadata.registry_snapshot.version != trial.cross_bindings.registry_version
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "registry version continuity failed across run/link/solve",
        ));
    }
    if solve.metadata.registry_snapshot.lookup_snapshot_hash
        != trial.cross_bindings.registry_snapshot_hash
        || run.metadata.registry_snapshot.lookup_snapshot_hash
            != trial.cross_bindings.registry_snapshot_hash
        || link.metadata.registry_snapshot.lookup_snapshot_hash
            != trial.cross_bindings.registry_snapshot_hash
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "registry snapshot hash continuity failed across run/link/solve",
        ));
    }
    if run
        .summary
        .labels
        .get("benchmark_id")
        .is_none_or(|id| id != &trial.cross_bindings.benchmark_id)
        || run
            .summary
            .labels
            .get("run_id")
            .is_none_or(|id| id != &trial.cross_bindings.run_id)
        || run
            .summary
            .labels
            .get("trial_id")
            .is_none_or(|id| id != &trial.trial_id)
        || run
            .summary
            .labels
            .get("family")
            .is_none_or(|family| family != trial_family_str(trial.family))
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "run summary labels do not bind benchmark_id, run_id, family, and trial_id",
        ));
    }
    let sidecar_ref = artifacts
        .iter()
        .find(|artifact| {
            matches!(
                &artifact.artifact,
                LoadedGeneralizationArtifact::LinkObservationSurfaceBindings(_)
            )
        })
        .ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                "strict execution envelope is missing link observation/surface bindings",
            )
        })?;
    let expected_sidecar_path = sibling_manifest_path(
        &loaded_link_artifact_ref(artifacts)?.reference.path,
        &link.observation_surface_bindings_path,
    )?;
    if sidecar_ref.reference.path != expected_sidecar_path
        || sidecar_ref.reference.content_hash != link.observation_surface_bindings_content_hash
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "link observation/surface bindings ref must match EntityLinkArtifact sidecar path and hash",
        ));
    }
    let (link_artifact_path, _) = read_strict_manifest_file_canonical_path(
        base_dir,
        "artifacts.entity_link.path",
        &loaded_link_artifact_ref(artifacts)?.reference.path,
    )?;
    let derivation_validated_bindings =
        read_derivation_validated_entity_link_observation_surface_bindings_at_path(
            link,
            &link_artifact_path,
            run,
        )
        .map_err(contract_error)?;
    if bindings != derivation_validated_bindings.as_slice() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "explicit link observation/surface binding artifact does not match native derivation",
        ));
    }
    Ok(())
}

fn validate_solve_derivation_path_continuity(
    run_ref_path: &str,
    run: &EntityRunArtifact,
    solve_derivation: &LoadedGeneralizationSolveDerivation,
) -> GeneralizationResult<()> {
    require_solve_derivation_ref_path(
        "solve_derivation.edge_artifact.path",
        &solve_derivation.references.edge_artifact.path,
        &sibling_manifest_path(run_ref_path, &run.work_dir.edge_artifact_path)?,
    )?;
    require_solve_derivation_ref_path(
        "solve_derivation.edge_records.path",
        &solve_derivation.references.edge_records.path,
        &sibling_manifest_path(run_ref_path, &run.work_dir.edge_records_path)?,
    )?;
    require_solve_derivation_ref_path(
        "solve_derivation.prepared_surfaces.path",
        &solve_derivation.references.prepared_surfaces.path,
        &sibling_manifest_path(run_ref_path, &run.work_dir.surfaces_path)?,
    )?;
    require_distinct_solve_derivation_refs(&solve_derivation.references)?;
    Ok(())
}

fn require_solve_derivation_ref_path(
    field: &str,
    actual: &str,
    expected: &str,
) -> GeneralizationResult<()> {
    if actual != expected {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must match the canonical path derived from the loaded run artifact"),
        ));
    }
    Ok(())
}

fn require_distinct_solve_derivation_refs(
    refs: &GeneralizationSolveDerivationRefs,
) -> GeneralizationResult<()> {
    let mut paths = BTreeSet::new();
    let mut hashes = BTreeSet::new();
    for (field, reference) in [
        ("solve_derivation.edge_artifact", &refs.edge_artifact),
        ("solve_derivation.edge_records", &refs.edge_records),
        (
            "solve_derivation.prepared_surfaces",
            &refs.prepared_surfaces,
        ),
        ("solve_derivation.solve_policy", &refs.solve_policy),
    ] {
        if !paths.insert(reference.path.clone()) {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("{field}.path duplicates another solve_derivation ref"),
            ));
        }
        if !hashes.insert(reference.content_hash.clone()) {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("{field}.content_hash duplicates another solve_derivation ref"),
            ));
        }
    }
    Ok(())
}

fn validate_loaded_run_solve_rebuild(
    run: &EntityRunArtifact,
    registry_dir: &Path,
    solve: &SolveArtifact,
    candidate_recall: &LoadedGeneralizationCandidateRecall,
    solve_derivation: &LoadedGeneralizationSolveDerivation,
) -> GeneralizationResult<()> {
    let rebound = rebind_generalization_native_stages(GeneralizationNativeStageRebindRequest {
        run,
        registry_dir,
        block: &candidate_recall.block_artifact,
        block_candidate_records: &candidate_recall.candidate_records,
        block_diagnostics: &candidate_recall.diagnostics,
        exact_buckets: &candidate_recall.exact_bucket_assertions,
        edge: &solve_derivation.edge_artifact,
        edge_records: &solve_derivation.edge_records,
        prepared_surfaces: &solve_derivation.prepared_surfaces,
        solve_config: solve_derivation.solve_config,
    })?;
    if &rebound.solve != solve {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "loaded solve artifact does not match strict rebuild from native derivation inputs",
        ));
    }
    if &rebound.run != run {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "loaded run artifact does not match strict rebuild from native derivation inputs",
        ));
    }
    Ok(())
}

fn loaded_link_artifact_ref(
    artifacts: &[LoadedGeneralizationArtifactRef],
) -> GeneralizationResult<&LoadedGeneralizationArtifactRef> {
    artifacts
        .iter()
        .find(|artifact| matches!(&artifact.artifact, LoadedGeneralizationArtifact::Link(_)))
        .ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                "loaded artifacts are missing entity link artifact",
            )
        })
}

fn loaded_link_artifact(
    artifacts: &[LoadedGeneralizationArtifactRef],
) -> GeneralizationResult<&EntityLinkArtifact> {
    artifacts
        .iter()
        .find_map(|artifact| match &artifact.artifact {
            LoadedGeneralizationArtifact::Link(link) => Some(link),
            _ => None,
        })
        .ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                "loaded artifacts are missing entity link artifact",
            )
        })
}

fn maybe_loaded_candidate_recall_artifact(
    artifacts: &[LoadedGeneralizationArtifactRef],
) -> Option<&EntityCandidateRecallReport> {
    artifacts
        .iter()
        .find_map(|artifact| match &artifact.artifact {
            LoadedGeneralizationArtifact::CandidateRecall(report) => Some(report),
            _ => None,
        })
}

fn sibling_manifest_path(parent_ref_path: &str, child_rel: &str) -> GeneralizationResult<String> {
    let parent = Path::new(parent_ref_path);
    let mut path = parent.parent().map(Path::to_path_buf).unwrap_or_default();
    if parent.file_name() == Some(std::ffi::OsStr::new("run.json"))
        && path.file_name() == Some(std::ffi::OsStr::new("run"))
    {
        path.pop();
    }
    path.push(child_rel);
    let path = path.to_str().ok_or_else(|| {
        error(
            GeneralizationErrorCode::ArtifactContract,
            "artifact sidecar path must be valid UTF-8",
        )
    })?;
    Ok(path.to_string())
}

fn loaded_run_artifact(
    artifacts: &[LoadedGeneralizationArtifactRef],
) -> GeneralizationResult<&EntityRunArtifact> {
    artifacts
        .iter()
        .find_map(|artifact| match &artifact.artifact {
            LoadedGeneralizationArtifact::Run(run) => Some(run),
            _ => None,
        })
        .ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                "loaded artifacts are missing entity run artifact",
            )
        })
}

fn loaded_run_artifact_ref(
    artifacts: &[LoadedGeneralizationArtifactRef],
) -> GeneralizationResult<&LoadedGeneralizationArtifactRef> {
    artifacts
        .iter()
        .find(|artifact| matches!(&artifact.artifact, LoadedGeneralizationArtifact::Run(_)))
        .ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                "loaded artifacts are missing entity run artifact",
            )
        })
}

fn loaded_solve_artifact(
    artifacts: &[LoadedGeneralizationArtifactRef],
) -> GeneralizationResult<&SolveArtifact> {
    artifacts
        .iter()
        .find_map(|artifact| match &artifact.artifact {
            LoadedGeneralizationArtifact::Solve(solve) => Some(solve),
            _ => None,
        })
        .ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                "loaded artifacts are missing entity solve artifact",
            )
        })
}

fn loaded_observation_surface_bindings(
    artifacts: &[LoadedGeneralizationArtifactRef],
) -> GeneralizationResult<&[EntityLinkObservationSurfaceBinding]> {
    artifacts
        .iter()
        .find_map(|artifact| match &artifact.artifact {
            LoadedGeneralizationArtifact::LinkObservationSurfaceBindings(bindings) => {
                Some(bindings.as_slice())
            }
            _ => None,
        })
        .ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                "loaded artifacts are missing link observation/surface bindings",
            )
        })
}

struct StrictDerivationContext<'a> {
    trial_id: &'a str,
    candidate_recall: &'a LoadedGeneralizationCandidateRecall,
    link: &'a EntityLinkArtifact,
    sidecar_by_surface: BTreeMap<String, &'a EntityLinkObservationSurfaceBinding>,
    surface_by_observation: BTreeMap<String, String>,
    decision_ids_by_observation: BTreeMap<String, BTreeSet<String>>,
    component_by_surface: BTreeMap<String, &'a SolveEntityRecord>,
    result_bindings: BTreeMap<String, &'a GeneralizationResultBinding>,
    directional_link_bindings: BTreeMap<String, &'a GeneralizationDirectionalLinkBinding>,
    hard_negative_bindings: BTreeMap<String, &'a GeneralizationHardNegativeBinding>,
}

impl<'a> StrictDerivationContext<'a> {
    fn new(loaded: &'a LoadedGeneralizationTrialExecution) -> GeneralizationResult<Self> {
        let link = loaded_link_artifact(&loaded.artifacts)?;
        let solve = loaded_solve_artifact(&loaded.artifacts)?;
        let sidecar_bindings = loaded_observation_surface_bindings(&loaded.artifacts)?;
        let (sidecar_by_surface, surface_by_observation, decision_ids_by_observation) =
            observation_surface_join(
                sidecar_bindings,
                &loaded.execution.bindings.observation_bindings,
            )?;
        let component_by_surface = solve_component_by_surface(solve)?;
        let result_bindings = result_bindings_by_id(&loaded.execution.bindings.result_bindings)?;
        let directional_link_bindings =
            directional_link_bindings_by_id(&loaded.execution.bindings.directional_link_bindings)?;
        let hard_negative_bindings =
            hard_negative_bindings_by_id(&loaded.execution.bindings.hard_negative_bindings)?;
        validate_candidate_gold_pair_usage(&loaded.candidate_recall, &loaded.execution.bindings)?;
        Ok(Self {
            trial_id: loaded.execution.trial_id.as_str(),
            candidate_recall: &loaded.candidate_recall,
            link,
            sidecar_by_surface,
            surface_by_observation,
            decision_ids_by_observation,
            component_by_surface,
            result_bindings,
            directional_link_bindings,
            hard_negative_bindings,
        })
    }

    fn validate_observation_coverage(
        &self,
        observations: &[GeneralizationObservation],
    ) -> GeneralizationResult<()> {
        for observation in observations {
            self.surface_for_observation(&observation.observation_id)?;
        }
        Ok(())
    }

    fn derive_discovery_result(
        &self,
        result: &DiscoveryResultRecord,
    ) -> GeneralizationResult<DiscoveryResultRecord> {
        let binding = self.result_bindings.get(&result.result_id).ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!(
                    "trial {} is missing result binding {}",
                    self.trial_id, result.result_id
                ),
            )
        })?;
        self.validate_result_binding(result, binding)?;
        let surfaces = self.surfaces_for_observations(&result.observation_ids)?;
        let (component, actual_decision) = self.derive_decision_from_disposition(
            &binding.solve_disposition,
            &result.observation_ids,
            &surfaces,
            None,
            &result.result_id,
        )?;
        if matches!(
            result.expected_decision,
            DiscoveryDecision::AttachExisting | DiscoveryDecision::ClusterNew
        ) && matches!(
            binding.solve_disposition,
            GeneralizationSolveDisposition::Absent
        ) {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "credited result {} requires a Present solve disposition",
                    result.result_id
                ),
            ));
        }
        let candidate_rank = if matches!(
            binding.solve_disposition,
            GeneralizationSolveDisposition::Absent
        ) {
            if binding.candidate_gold_pair_id.is_some() {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!(
                        "Absent result {} must not receive candidate-rank credit",
                        result.result_id
                    ),
                ));
            }
            None
        } else {
            self.candidate_rank_from_binding(
                &result.result_id,
                &binding.candidate_gold_pair_id,
                &binding.candidate_pair_observation_ids,
            )?
        };
        Ok(DiscoveryResultRecord {
            result_id: result.result_id.clone(),
            observation_ids: result.observation_ids.clone(),
            expected_decision: result.expected_decision,
            actual_decision,
            candidate_rank,
            evidence_lanes: evidence_lanes_from_component(component),
            review_action: review_action_for_decision(actual_decision),
        })
    }

    fn derive_hard_negative(
        &self,
        control: &HardNegativeControl,
    ) -> GeneralizationResult<HardNegativeControl> {
        let binding = self
            .hard_negative_bindings
            .get(&control.control_id)
            .ok_or_else(|| {
                error(
                    GeneralizationErrorCode::MissingReference,
                    format!(
                        "trial {} is missing hard-negative binding {}",
                        self.trial_id, control.control_id
                    ),
                )
            })?;
        self.validate_hard_negative_binding(control, binding)?;
        let pair_link_disposition = if let Some(expected) = binding.link_disposition {
            let actual = self.link_decision_for_observations(
                &control.left_observation_id,
                &control.right_observation_id,
            )?;
            if actual != expected.into() {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!(
                        "hard-negative {} link disposition conflicts with native link decisions",
                        control.control_id
                    ),
                ));
            }
            Some(expected)
        } else {
            None
        };
        let left = self.component_for_single_observation_disposition(
            &control.left_observation_id,
            &binding.left_surface_id,
            &binding.left_solve_disposition,
            pair_link_disposition,
            &control.control_id,
        )?;
        let right = self.component_for_single_observation_disposition(
            &control.right_observation_id,
            &binding.right_surface_id,
            &binding.right_solve_disposition,
            pair_link_disposition,
            &control.control_id,
        )?;
        let false_merge = match (left, right) {
            (Some(left), Some(right)) if left.component_id == right.component_id => matches!(
                left.state,
                SolveReconciliationState::ResolvedExisting
                    | SolveReconciliationState::PromotableNew
            ),
            _ => false,
        };
        if binding.expected_false_merge != false_merge {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "hard-negative {} expected_false_merge conflicts with native solve/link evidence",
                    control.control_id
                ),
            ));
        }
        Ok(HardNegativeControl {
            control_id: control.control_id.clone(),
            left_observation_id: control.left_observation_id.clone(),
            right_observation_id: control.right_observation_id.clone(),
            relation_class: control.relation_class,
            severity: control.severity,
            false_merge,
        })
    }

    fn derive_directional_link(
        &self,
        link: &DirectionalCrossSourceLink,
    ) -> GeneralizationResult<DirectionalCrossSourceLink> {
        let binding = self
            .directional_link_bindings
            .get(&link.link_id)
            .ok_or_else(|| {
                error(
                    GeneralizationErrorCode::MissingReference,
                    format!(
                        "trial {} is missing directional link binding {}",
                        self.trial_id, link.link_id
                    ),
                )
            })?;
        self.validate_directional_link_binding(link, binding)?;
        let surfaces = self.surfaces_for_observations(&[
            link.reference_observation_id.clone(),
            link.target_observation_id.clone(),
        ])?;
        let native_link_disposition = self.link_decision_for_observations(
            &link.reference_observation_id,
            &link.target_observation_id,
        )?;
        if native_link_disposition != binding.link_disposition.into() {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "directional link {} typed disposition conflicts with native link decisions",
                    link.link_id
                ),
            ));
        }
        let (_component, actual_decision) = match binding.link_disposition {
            GeneralizationLinkDisposition::Matched => self.derive_decision_from_disposition(
                &binding.solve_disposition,
                &[
                    link.reference_observation_id.clone(),
                    link.target_observation_id.clone(),
                ],
                &surfaces,
                Some(GeneralizationLinkDisposition::Matched),
                &link.link_id,
            )?,
            GeneralizationLinkDisposition::Ambiguous | GeneralizationLinkDisposition::Unmatched => {
                let (component, decision) = self.derive_decision_from_disposition(
                    &binding.solve_disposition,
                    &[
                        link.reference_observation_id.clone(),
                        link.target_observation_id.clone(),
                    ],
                    &surfaces,
                    Some(binding.link_disposition),
                    &link.link_id,
                )?;
                if !decision.is_abstention() {
                    return Err(error(
                        GeneralizationErrorCode::ArtifactContract,
                        format!(
                            "directional link {} is native {:?} but solve disposition derives {:?}",
                            link.link_id, binding.link_disposition, decision
                        ),
                    ));
                }
                (component, DiscoveryDecision::Abstain)
            }
        };
        let candidate_rank = if binding.link_disposition == GeneralizationLinkDisposition::Matched
            && matches!(
                binding.solve_disposition,
                GeneralizationSolveDisposition::Present { .. }
            ) {
            self.candidate_rank_from_binding(
                &link.link_id,
                &Some(binding.gold_pair_id.clone()),
                &Some(vec![
                    binding.reference_observation_id.clone(),
                    binding.target_observation_id.clone(),
                ]),
            )?
        } else {
            None
        };
        Ok(DirectionalCrossSourceLink {
            link_id: link.link_id.clone(),
            reference_observation_id: link.reference_observation_id.clone(),
            target_observation_id: link.target_observation_id.clone(),
            reference_dataset_id: link.reference_dataset_id.clone(),
            target_dataset_id: link.target_dataset_id.clone(),
            expected_decision: link.expected_decision,
            actual_decision,
            candidate_rank,
        })
    }

    fn validate_result_binding(
        &self,
        result: &DiscoveryResultRecord,
        binding: &GeneralizationResultBinding,
    ) -> GeneralizationResult<()> {
        if binding.expected_decision != result.expected_decision
            || binding.observation_ids != result.observation_ids
        {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "result binding {} conflicts with benchmark result",
                    result.result_id
                ),
            ));
        }
        let surfaces = self.surfaces_for_observations(&result.observation_ids)?;
        require_same_string_set(
            &binding.surface_ids,
            &surfaces,
            &format!("result binding {} surface_ids", result.result_id),
        )
    }

    fn validate_directional_link_binding(
        &self,
        link: &DirectionalCrossSourceLink,
        binding: &GeneralizationDirectionalLinkBinding,
    ) -> GeneralizationResult<()> {
        if binding.expected_decision != link.expected_decision
            || binding.reference_observation_id != link.reference_observation_id
            || binding.target_observation_id != link.target_observation_id
        {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "directional link binding {} conflicts with benchmark link",
                    link.link_id
                ),
            ));
        }
        let reference_surface = self.surface_for_observation(&link.reference_observation_id)?;
        let target_surface = self.surface_for_observation(&link.target_observation_id)?;
        if binding.reference_surface_id != reference_surface
            || binding.target_surface_id != target_surface
        {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "directional link binding {} surface IDs conflict with observation bindings",
                    link.link_id
                ),
            ));
        }
        Ok(())
    }

    fn validate_hard_negative_binding(
        &self,
        control: &HardNegativeControl,
        binding: &GeneralizationHardNegativeBinding,
    ) -> GeneralizationResult<()> {
        if binding.left_observation_id != control.left_observation_id
            || binding.right_observation_id != control.right_observation_id
        {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "hard-negative binding {} conflicts with benchmark control",
                    control.control_id
                ),
            ));
        }
        let left_surface = self.surface_for_observation(&control.left_observation_id)?;
        let right_surface = self.surface_for_observation(&control.right_observation_id)?;
        if binding.left_surface_id != left_surface || binding.right_surface_id != right_surface {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "hard-negative binding {} surface IDs conflict with observation bindings",
                    control.control_id
                ),
            ));
        }
        Ok(())
    }

    fn derive_decision_from_disposition(
        &self,
        disposition: &GeneralizationSolveDisposition,
        observation_ids: &[String],
        surfaces: &[String],
        required_link_disposition: Option<GeneralizationLinkDisposition>,
        scored_id: &str,
    ) -> GeneralizationResult<(Option<&'a SolveEntityRecord>, DiscoveryDecision)> {
        match disposition {
            GeneralizationSolveDisposition::Present {
                component_id,
                state,
            } => {
                if required_link_disposition == Some(GeneralizationLinkDisposition::Matched) {
                    let native = self
                        .link_decision_for_observations(&observation_ids[0], &observation_ids[1])?;
                    if native != StrictLinkDecision::Matched {
                        return Err(error(
                            GeneralizationErrorCode::ArtifactContract,
                            format!(
                                "{scored_id} declares Present matched solve without native match"
                            ),
                        ));
                    }
                }
                let component =
                    self.present_component_for_surfaces(component_id, *state, surfaces)?;
                Ok((Some(component), decision_from_solve_component(component)))
            }
            GeneralizationSolveDisposition::Absent => {
                self.validate_absent_disposition(
                    observation_ids,
                    surfaces,
                    required_link_disposition,
                    scored_id,
                )?;
                Ok((None, DiscoveryDecision::Abstain))
            }
        }
    }

    fn component_for_single_observation_disposition(
        &self,
        observation_id: &str,
        surface_id: &str,
        disposition: &GeneralizationSolveDisposition,
        pair_link_disposition: Option<GeneralizationLinkDisposition>,
        scored_id: &str,
    ) -> GeneralizationResult<Option<&'a SolveEntityRecord>> {
        self.surface_for_observation(observation_id)?;
        self.sidecar_by_surface.get(surface_id).ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!("{scored_id} surface {surface_id} is missing from native sidecar"),
            )
        })?;
        match disposition {
            GeneralizationSolveDisposition::Present {
                component_id,
                state,
            } => self
                .present_component_for_surfaces(component_id, *state, &[surface_id.to_string()])
                .map(Some),
            GeneralizationSolveDisposition::Absent => {
                if self.component_by_surface.contains_key(surface_id) {
                    return Err(error(
                        GeneralizationErrorCode::ArtifactContract,
                        format!("{scored_id} declares Absent but solve owns surface {surface_id}"),
                    ));
                }
                if pair_link_disposition.is_none() {
                    self.require_native_absence_for_observation(observation_id, scored_id)?;
                }
                Ok(None)
            }
        }
    }

    fn present_component_for_surfaces(
        &self,
        component_id: &str,
        state: SolveReconciliationState,
        surfaces: &[String],
    ) -> GeneralizationResult<&'a SolveEntityRecord> {
        let mut shared: Option<&'a SolveEntityRecord> = None;
        for surface in surfaces {
            let component = self
                .component_by_surface
                .get(surface)
                .copied()
                .ok_or_else(|| {
                    error(
                        GeneralizationErrorCode::MissingReference,
                        format!("surface {surface} is missing from solve artifact components"),
                    )
                })?;
            if component.component_id != component_id || component.state != state {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!(
                        "surface {surface} solve component conflicts with typed Present disposition"
                    ),
                ));
            }
            if let Some(previous) = shared {
                if previous.component_id != component.component_id {
                    return Err(error(
                        GeneralizationErrorCode::ArtifactContract,
                        "Present solve disposition spans multiple solve components",
                    ));
                }
            } else {
                shared = Some(component);
            }
        }
        shared.ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                "Present solve disposition must reference at least one surface",
            )
        })
    }

    fn validate_absent_disposition(
        &self,
        observation_ids: &[String],
        surfaces: &[String],
        required_link_disposition: Option<GeneralizationLinkDisposition>,
        scored_id: &str,
    ) -> GeneralizationResult<()> {
        for surface in surfaces {
            self.sidecar_by_surface.get(surface).ok_or_else(|| {
                error(
                    GeneralizationErrorCode::MissingReference,
                    format!("{scored_id} surface {surface} is missing from native sidecar"),
                )
            })?;
            if self.component_by_surface.contains_key(surface) {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!("{scored_id} declares Absent but solve owns surface {surface}"),
                ));
            }
        }
        if let Some(required) = required_link_disposition {
            if required == GeneralizationLinkDisposition::Matched {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!("{scored_id} cannot declare Absent for a native matched link"),
                ));
            }
            if observation_ids.len() == 2 {
                let actual =
                    self.link_decision_for_observations(&observation_ids[0], &observation_ids[1])?;
                if actual != required.into() {
                    return Err(error(
                        GeneralizationErrorCode::ArtifactContract,
                        format!(
                            "{scored_id} Absent disposition conflicts with native link decision"
                        ),
                    ));
                }
            }
        } else if observation_ids.len() == 1 {
            self.require_native_absence_for_observation(&observation_ids[0], scored_id)?;
        } else if observation_ids.len() == 2 {
            let actual =
                self.link_decision_for_observations(&observation_ids[0], &observation_ids[1])?;
            if !matches!(
                actual,
                StrictLinkDecision::Ambiguous | StrictLinkDecision::Unmatched
            ) {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!(
                        "{scored_id} Absent disposition requires native unmatched/ambiguous link evidence"
                    ),
                ));
            }
        } else {
            for observation_id in observation_ids {
                self.require_native_absence_for_observation(observation_id, scored_id)?;
            }
        }
        Ok(())
    }

    fn surfaces_for_observations(&self, ids: &[String]) -> GeneralizationResult<Vec<String>> {
        ids.iter()
            .map(|id| self.surface_for_observation(id))
            .collect()
    }

    fn surface_for_observation(&self, id: &str) -> GeneralizationResult<String> {
        self.surface_by_observation.get(id).cloned().ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!("observation {id} is missing from explicit observation bindings"),
            )
        })
    }

    fn candidate_rank_from_binding(
        &self,
        scored_id: &str,
        gold_pair_id: &Option<String>,
        pair_observation_ids: &Option<Vec<String>>,
    ) -> GeneralizationResult<Option<u32>> {
        let Some(gold_pair_id) = gold_pair_id else {
            return Ok(None);
        };
        let pair_observation_ids = pair_observation_ids.as_ref().ok_or_else(|| {
            error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "{scored_id} names candidate gold_pair_id without candidate_pair_observation_ids"
                ),
            )
        })?;
        if pair_observation_ids.len() != 2 {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{scored_id} candidate_pair_observation_ids must contain exactly two IDs"),
            ));
        }
        let pair_surfaces = self.surfaces_for_observations(pair_observation_ids)?;
        let gold_pair = self
            .candidate_recall
            .gold_pairs
            .iter()
            .filter(|pair| pair.gold_pair_id == *gold_pair_id)
            .collect::<Vec<_>>();
        if gold_pair.len() != 1 {
            return Err(error(
                if gold_pair.is_empty() {
                    GeneralizationErrorCode::MissingReference
                } else {
                    GeneralizationErrorCode::DuplicateRecord
                },
                format!(
                    "{scored_id} candidate gold_pair_id {gold_pair_id} is not unique in quality manifest"
                ),
            ));
        }
        let gold_pair = gold_pair[0];
        require_same_string_set(
            &pair_surfaces,
            &[
                gold_pair.left_surface_id.clone(),
                gold_pair.right_surface_id.clone(),
            ],
            &format!("{scored_id} candidate gold pair endpoints"),
        )?;
        if let Some(rank) = candidate_rank_from_true_pair_ranks(
            scored_id,
            gold_pair_id,
            &self.candidate_recall.report.true_pair_ranks,
        )? {
            return Ok(Some(rank));
        }

        let surface_set = pair_surfaces.iter().cloned().collect::<BTreeSet<_>>();
        let misses = self
            .candidate_recall
            .report
            .misses_at_50
            .iter()
            .filter(|miss| miss.gold_pair_id == *gold_pair_id)
            .collect::<Vec<_>>();
        if misses.len() > 1 {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("candidate recall has ambiguous miss records for {gold_pair_id}"),
            ));
        }
        if let Some(miss) = misses.first() {
            if BTreeSet::from([miss.left_surface_id.clone(), miss.right_surface_id.clone()])
                != surface_set
            {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!(
                        "candidate miss {gold_pair_id} endpoints conflict with explicit binding"
                    ),
                ));
            }
            return Ok(None);
        }
        Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("candidate recall record for {gold_pair_id} is missing from recomputed report"),
        ))
    }

    fn link_decision_for_observations(
        &self,
        reference_observation_id: &str,
        target_observation_id: &str,
    ) -> GeneralizationResult<StrictLinkDecision> {
        let reference_ids = self.decision_ids_for_observation(reference_observation_id)?;
        let target_ids = self.decision_ids_for_observation(target_observation_id)?;

        let matched = self
            .link
            .decision_artifact
            .matches
            .iter()
            .filter(|record| {
                reference_ids.contains(&record.reference_id)
                    && target_ids.contains(&record.target_id)
            })
            .count();
        let ambiguous = self
            .link
            .decision_artifact
            .ambiguous
            .iter()
            .filter(|record| target_ids.contains(&record.target_id))
            .count();
        let unmatched = self
            .link
            .decision_artifact
            .unmatched
            .iter()
            .filter(|record| target_ids.contains(&record.target_id))
            .count();
        match (matched, ambiguous, unmatched) {
            (1, 0, 0) => Ok(StrictLinkDecision::Matched),
            (0, 1, 0) => Ok(StrictLinkDecision::Ambiguous),
            (0, 0, 1) => Ok(StrictLinkDecision::Unmatched),
            (0, 0, 0) => Err(error(
                GeneralizationErrorCode::MissingReference,
                format!(
                    "directional target {target_observation_id} is missing from link decisions"
                ),
            )),
            _ => Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "directional target {target_observation_id} has ambiguous link decision records"
                ),
            )),
        }
    }

    fn require_native_absence_for_observation(
        &self,
        observation_id: &str,
        scored_id: &str,
    ) -> GeneralizationResult<()> {
        match self.link_decision_for_observation(observation_id)? {
            StrictLinkDecision::Ambiguous | StrictLinkDecision::Unmatched => Ok(()),
            StrictLinkDecision::Matched => Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "{scored_id} declares Absent but observation {observation_id} is natively matched"
                ),
            )),
        }
    }

    fn link_decision_for_observation(
        &self,
        observation_id: &str,
    ) -> GeneralizationResult<StrictLinkDecision> {
        let ids = self.decision_ids_for_observation(observation_id)?;
        let matched = self
            .link
            .decision_artifact
            .matches
            .iter()
            .filter(|record| ids.contains(&record.reference_id) || ids.contains(&record.target_id))
            .count();
        let ambiguous = self
            .link
            .decision_artifact
            .ambiguous
            .iter()
            .filter(|record| ids.contains(&record.target_id))
            .count();
        let unmatched = self
            .link
            .decision_artifact
            .unmatched
            .iter()
            .filter(|record| ids.contains(&record.target_id))
            .count();
        match (matched, ambiguous, unmatched) {
            (1, 0, 0) => Ok(StrictLinkDecision::Matched),
            (0, 1, 0) => Ok(StrictLinkDecision::Ambiguous),
            (0, 0, 1) => Ok(StrictLinkDecision::Unmatched),
            (0, 0, 0) => Err(error(
                GeneralizationErrorCode::MissingReference,
                format!("observation {observation_id} is missing from link decisions"),
            )),
            _ => Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("observation {observation_id} has ambiguous link decision records"),
            )),
        }
    }

    fn decision_ids_for_observation(
        &self,
        observation_id: &str,
    ) -> GeneralizationResult<&BTreeSet<String>> {
        self.decision_ids_by_observation
            .get(observation_id)
            .ok_or_else(|| {
                error(
                    GeneralizationErrorCode::MissingReference,
                    format!(
                        "observation {observation_id} is missing from link decision id bindings"
                    ),
                )
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StrictLinkDecision {
    Matched,
    Ambiguous,
    Unmatched,
}

impl From<GeneralizationLinkDisposition> for StrictLinkDecision {
    fn from(value: GeneralizationLinkDisposition) -> Self {
        match value {
            GeneralizationLinkDisposition::Matched => Self::Matched,
            GeneralizationLinkDisposition::Ambiguous => Self::Ambiguous,
            GeneralizationLinkDisposition::Unmatched => Self::Unmatched,
        }
    }
}

fn candidate_rank_from_true_pair_ranks(
    scored_id: &str,
    gold_pair_id: &str,
    true_pair_ranks: &[CandidateRecallRankRecord],
) -> GeneralizationResult<Option<u32>> {
    let mut operator_ids = BTreeSet::new();
    let mut best_rank = None::<usize>;
    for rank in true_pair_ranks
        .iter()
        .filter(|rank| rank.gold_pair_id == gold_pair_id)
    {
        if !operator_ids.insert(rank.operator_id.clone()) {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!(
                    "{scored_id} candidate recall has duplicate ranks for {gold_pair_id} from operator {}",
                    rank.operator_id
                ),
            ));
        }
        best_rank = Some(best_rank.map_or(rank.rank, |current| current.min(rank.rank)));
    }
    best_rank
        .map(|rank| {
            u32::try_from(rank).map_err(|_| {
                error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!("candidate recall rank for {gold_pair_id} exceeds u32"),
                )
            })
        })
        .transpose()
}

type ObservationSurfaceJoin<'a> = (
    BTreeMap<String, &'a EntityLinkObservationSurfaceBinding>,
    BTreeMap<String, String>,
    BTreeMap<String, BTreeSet<String>>,
);

fn observation_surface_join<'a>(
    sidecar_bindings: &'a [EntityLinkObservationSurfaceBinding],
    explicit_bindings: &[GeneralizationObservationBinding],
) -> GeneralizationResult<ObservationSurfaceJoin<'a>> {
    let mut sidecar_by_surface = BTreeMap::new();
    for binding in sidecar_bindings {
        if sidecar_by_surface
            .insert(binding.surface_id.clone(), binding)
            .is_some()
        {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!(
                    "surface {} appears multiple times in native sidecar",
                    binding.surface_id
                ),
            ));
        }
    }
    let mut surface_by_observation = BTreeMap::new();
    let mut decision_ids_by_observation = BTreeMap::<String, BTreeSet<String>>::new();
    for binding in explicit_bindings {
        let sidecar = sidecar_by_surface.get(&binding.surface_id).ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!(
                    "observation binding {} references surface {} missing from native sidecar",
                    binding.observation_id, binding.surface_id
                ),
            )
        })?;
        if sidecar.surface_binding_hash != binding.surface_binding_hash
            || sidecar.profile_id != binding.profile_id
            || binding.side.is_some_and(|side| sidecar.side != side)
        {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "observation binding {} conflicts with native sidecar surface binding",
                    binding.observation_id
                ),
            ));
        }
        let source_row_id = binding.source_row_id.as_ref().ok_or_else(|| {
            error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "observation binding {} must name the native source_row_id/link_id explicitly",
                    binding.observation_id
                ),
            )
        })?;
        if sidecar.source_row_id.as_ref() != Some(source_row_id)
            && sidecar.link_id != *source_row_id
        {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "observation binding {} source_row_id does not match native sidecar",
                    binding.observation_id
                ),
            ));
        }
        match surface_by_observation
            .insert(binding.observation_id.clone(), binding.surface_id.clone())
        {
            Some(previous) if previous != binding.surface_id => {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!(
                        "observation binding {} maps to conflicting surfaces",
                        binding.observation_id
                    ),
                ));
            }
            _ => {}
        }
        let ids = decision_ids_by_observation
            .entry(binding.observation_id.clone())
            .or_default();
        ids.insert(sidecar.link_id.clone());
        if let Some(source_row_id) = &sidecar.source_row_id {
            ids.insert(source_row_id.clone());
        }
    }
    Ok((
        sidecar_by_surface,
        surface_by_observation,
        decision_ids_by_observation,
    ))
}

fn result_bindings_by_id(
    bindings: &[GeneralizationResultBinding],
) -> GeneralizationResult<BTreeMap<String, &GeneralizationResultBinding>> {
    let mut by_id = BTreeMap::new();
    for binding in bindings {
        if by_id.insert(binding.result_id.clone(), binding).is_some() {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("duplicate result binding {}", binding.result_id),
            ));
        }
    }
    Ok(by_id)
}

fn directional_link_bindings_by_id(
    bindings: &[GeneralizationDirectionalLinkBinding],
) -> GeneralizationResult<BTreeMap<String, &GeneralizationDirectionalLinkBinding>> {
    let mut by_id = BTreeMap::new();
    for binding in bindings {
        if by_id
            .insert(binding.directional_link_id.clone(), binding)
            .is_some()
        {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!(
                    "duplicate directional link binding {}",
                    binding.directional_link_id
                ),
            ));
        }
    }
    Ok(by_id)
}

fn hard_negative_bindings_by_id(
    bindings: &[GeneralizationHardNegativeBinding],
) -> GeneralizationResult<BTreeMap<String, &GeneralizationHardNegativeBinding>> {
    let mut by_id = BTreeMap::new();
    for binding in bindings {
        if by_id.insert(binding.control_id.clone(), binding).is_some() {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("duplicate hard-negative binding {}", binding.control_id),
            ));
        }
    }
    Ok(by_id)
}

fn validate_candidate_gold_pair_usage(
    candidate_recall: &LoadedGeneralizationCandidateRecall,
    bindings: &GeneralizationExecutionBindings,
) -> GeneralizationResult<()> {
    let expected = candidate_recall
        .gold_pairs
        .iter()
        .map(|pair| pair.gold_pair_id.clone())
        .collect::<BTreeSet<_>>();
    let mut used = BTreeSet::new();
    for binding in &bindings.result_bindings {
        if let Some(gold_pair_id) = &binding.candidate_gold_pair_id {
            used.insert(gold_pair_id.clone());
        }
    }
    for binding in &bindings.directional_link_bindings {
        used.insert(binding.gold_pair_id.clone());
    }
    if used != expected {
        let missing = expected.difference(&used).cloned().collect::<Vec<_>>();
        let extra = used.difference(&expected).cloned().collect::<Vec<_>>();
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "candidate gold-pair bindings must exactly match recomputed quality manifest; missing={missing:?} extra={extra:?}"
            ),
        ));
    }
    Ok(())
}

fn require_same_string_set(
    left: &[String],
    right: &[String],
    field: &str,
) -> GeneralizationResult<()> {
    let left = left.iter().cloned().collect::<BTreeSet<_>>();
    let right = right.iter().cloned().collect::<BTreeSet<_>>();
    if left != right {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} does not match typed/native derivation"),
        ));
    }
    Ok(())
}

fn solve_component_by_surface(
    solve: &SolveArtifact,
) -> GeneralizationResult<BTreeMap<String, &SolveEntityRecord>> {
    let mut by_surface = BTreeMap::new();
    for component in &solve.entities {
        for surface_id in &component.surface_ids {
            if let Some(previous) = by_surface.insert(surface_id.clone(), component) {
                return Err(error(
                    GeneralizationErrorCode::DuplicateRecord,
                    format!(
                        "surface {surface_id} appears in multiple solve components: {} and {}",
                        previous.component_id, component.component_id
                    ),
                ));
            }
        }
    }
    Ok(by_surface)
}

fn decision_from_solve_component(component: &SolveEntityRecord) -> DiscoveryDecision {
    match component.state {
        SolveReconciliationState::ResolvedExisting => DiscoveryDecision::AttachExisting,
        SolveReconciliationState::PromotableNew => DiscoveryDecision::ClusterNew,
        SolveReconciliationState::Escrow => DiscoveryDecision::Abstain,
        SolveReconciliationState::Conflict | SolveReconciliationState::Contradiction => {
            DiscoveryDecision::Reject
        }
    }
}

fn review_action_for_decision(decision: DiscoveryDecision) -> ReviewAction {
    match decision {
        DiscoveryDecision::AttachExisting => ReviewAction::PromoteLink,
        DiscoveryDecision::ClusterNew => ReviewAction::PromoteCluster,
        DiscoveryDecision::Abstain => ReviewAction::DeferReview,
        DiscoveryDecision::Reject => ReviewAction::RejectCandidate,
        DiscoveryDecision::FalseMerge => ReviewAction::RecordCannotLink,
    }
}

fn evidence_lanes_from_component(
    component: Option<&SolveEntityRecord>,
) -> Vec<EvidenceLaneSummary> {
    let Some(component) = component else {
        return vec![EvidenceLaneSummary {
            lane_id: "solve_component".to_string(),
            available: false,
            support_basis_points: 0,
            contradiction_basis_points: 0,
        }];
    };
    let support = component
        .adjusted_support_score_units
        .as_u32()
        .min(u32::from(u16::MAX)) as u16;
    let contradiction = if component.hard_cannot_link_count > 0 {
        10_000
    } else if component.soft_anti_merge_warning_count > 0 {
        5_000
    } else {
        0
    };
    vec![EvidenceLaneSummary {
        lane_id: "solve_component".to_string(),
        available: true,
        support_basis_points: support,
        contradiction_basis_points: contradiction,
    }]
}

fn recompute_strict_leakage(
    loaded: &LoadedGeneralizationExecutionEnvelope,
    benchmark: &GeneralizationBenchmark,
) -> GeneralizationResult<()> {
    validate_leak_sources_are_influence_only(loaded)?;
    let trial_sources = loaded
        .trials
        .iter()
        .map(|trial| {
            (
                trial_execution_key(&trial.execution),
                trial.leak_scan_sources.as_slice(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for trial in &benchmark.entity_disjoint_trials {
        let key = (
            GeneralizationTrialFamily::EntityDisjoint,
            trial.trial_id.clone(),
        );
        let sources = trial_sources.get(&key).ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!(
                    "missing leak-source bundle for entity_disjoint trial {}",
                    trial.trial_id
                ),
            )
        })?;
        let holdout_values =
            protected_values_for_partition(&trial.observations, BenchmarkPartition::Holdout);
        scan_loaded_sources_for_protected_values(
            sources,
            ProtectedSet::HoldoutEntity,
            &holdout_values,
        )?;
    }
    for trial in &benchmark.time_forward_trials {
        let observations_by_id = observations_by_id(&trial.observations)?;
        let build_canonical_ids = trial
            .build_observation_ids
            .iter()
            .filter_map(|id| observations_by_id.get(id))
            .map(|observation| observation.canonical_entity_id.clone())
            .collect::<BTreeSet<_>>();
        let mut future_values = BTreeSet::new();
        for id in &trial.evaluation_observation_ids {
            let observation = observations_by_id.get(id).ok_or_else(|| {
                error(
                    GeneralizationErrorCode::MissingReference,
                    format!(
                        "evaluation observation {id} is missing in {}",
                        trial.trial_id
                    ),
                )
            })?;
            future_values.insert(observation.observation_id.clone());
            future_values.insert(observation.surface.clone());
            if !build_canonical_ids.contains(&observation.canonical_entity_id) {
                future_values.insert(observation.canonical_entity_id.clone());
            }
        }
        let key = (
            GeneralizationTrialFamily::TimeForward,
            trial.trial_id.clone(),
        );
        let sources = trial_sources.get(&key).ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!(
                    "missing leak-source bundle for time_forward trial {}",
                    trial.trial_id
                ),
            )
        })?;
        scan_loaded_sources_for_protected_values(
            sources,
            ProtectedSet::FutureObservation,
            &future_values,
        )?;
    }
    Ok(())
}

fn validate_leak_sources_are_influence_only(
    loaded: &LoadedGeneralizationExecutionEnvelope,
) -> GeneralizationResult<()> {
    let mut bundle_paths = BTreeSet::new();
    let mut bundle_hashes = BTreeSet::new();
    let mut checked_path_owners: BTreeMap<String, (String, bool)> = BTreeMap::new();
    let mut checked_hash_owners: BTreeMap<String, (String, bool)> = BTreeMap::new();
    for trial in &loaded.trials {
        let artifact_paths = trial
            .execution
            .artifacts
            .iter()
            .map(|artifact| artifact.path.as_str())
            .collect::<BTreeSet<_>>();
        let bundle_path = trial.execution.leak_scan_sources.path.as_str();
        if bundle_path == loaded.envelope.benchmark.path || artifact_paths.contains(bundle_path) {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                "leak-source bundle must be a pre-evaluation influence source, not sealed gold or post-evaluation output",
            ));
        }
        if !bundle_paths.insert(bundle_path) {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                "leak-source bundle paths must not be reused across trials",
            ));
        }
        if !bundle_hashes.insert(trial.execution.leak_scan_sources.content_hash.as_str()) {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                "leak-source bundle hashes must not be reused across trials",
            ));
        }
        let trial_key = format!(
            "{}:{}",
            trial_family_str(trial.execution.family),
            trial.execution.trial_id
        );
        for source in &trial.leak_scan_sources {
            if source.phase != GeneralizationLeakSourcePhase::PreEvaluationInfluence {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!(
                        "leak source {} is not pre-evaluation influence",
                        source.source_id
                    ),
                ));
            }
            let shareable = leak_source_allows_cross_trial_reuse(source);
            for checked_source in &source.checked_sources {
                validate_checked_source_cross_trial_isolation(
                    "path",
                    &mut checked_path_owners,
                    checked_source.path.clone(),
                    &trial_key,
                    shareable,
                )?;
                validate_checked_source_cross_trial_isolation(
                    "content_hash",
                    &mut checked_hash_owners,
                    checked_source.content_hash.clone(),
                    &trial_key,
                    shareable,
                )?;
            }
        }
    }
    Ok(())
}

fn leak_source_allows_cross_trial_reuse(source: &LoadedGeneralizationLeakSourceRef) -> bool {
    matches!(
        source.binding_kind,
        GeneralizationLeakSourceBindingKind::RegistrySnapshot
            | GeneralizationLeakSourceBindingKind::RegistrySidecarSnapshot
            | GeneralizationLeakSourceBindingKind::Profile
    ) || matches!(
        source.source_kind,
        GeneralizationLeakSourceKind::RegistryTree
            | GeneralizationLeakSourceKind::RegistryAliasFile
            | GeneralizationLeakSourceKind::RegistryAnchorFile
    )
}

fn validate_checked_source_cross_trial_isolation(
    label: &str,
    owners: &mut BTreeMap<String, (String, bool)>,
    value: String,
    trial_key: &str,
    shareable: bool,
) -> GeneralizationResult<()> {
    if let Some((previous_trial_key, previous_shareable)) = owners.get(&value) {
        if previous_trial_key != trial_key && !(*previous_shareable && shareable) {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("non-static checked leak-source {label} reused across trial chains"),
            ));
        }
        return Ok(());
    }
    owners.insert(value, (trial_key.to_string(), shareable));
    Ok(())
}

fn scan_loaded_sources_for_protected_values(
    sources: &[LoadedGeneralizationLeakSourceRef],
    protected_set: ProtectedSet,
    protected_values: &BTreeSet<String>,
) -> GeneralizationResult<()> {
    let protected_fingerprints = protected_values
        .iter()
        .map(|value| hash_bytes(ascii_trim(value).as_bytes()))
        .collect::<BTreeSet<_>>();
    for source in sources {
        for value in protected_values {
            let protected_value = ascii_trim(value);
            if bytes_contains(&source.bytes, protected_value.as_bytes()) {
                return Err(error(
                    leak_error_code(protected_set),
                    format!(
                        "pre-evaluation influence source {} leaked a protected {:?} value",
                        source.source_id, protected_set
                    ),
                ));
            }
            if decoded_strings_contain(&source.decoded_strings, protected_value) {
                return Err(error(
                    leak_error_code(protected_set),
                    format!(
                        "pre-evaluation influence source {} leaked a protected {:?} decoded scalar",
                        source.source_id, protected_set
                    ),
                ));
            }
        }
        for fingerprint in &protected_fingerprints {
            if bytes_contains(&source.bytes, fingerprint.as_bytes()) {
                return Err(error(
                    leak_error_code(protected_set),
                    format!(
                        "pre-evaluation influence source {} leaked a protected {:?} fingerprint",
                        source.source_id, protected_set
                    ),
                ));
            }
            if decoded_strings_contain(&source.decoded_strings, fingerprint) {
                return Err(error(
                    leak_error_code(protected_set),
                    format!(
                        "pre-evaluation influence source {} leaked a protected {:?} decoded fingerprint",
                        source.source_id, protected_set
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn decoded_strings_contain(decoded_strings: &BTreeSet<String>, needle: &str) -> bool {
    !needle.is_empty()
        && decoded_strings
            .iter()
            .any(|decoded| ascii_trim(decoded).contains(needle))
}

fn leak_error_code(protected_set: ProtectedSet) -> GeneralizationErrorCode {
    match protected_set {
        ProtectedSet::HoldoutEntity => GeneralizationErrorCode::EntityDisjointLeak,
        ProtectedSet::FutureObservation => GeneralizationErrorCode::FutureLeakage,
    }
}

fn bytes_contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn decoded_json_scalar_strings(value: &Value) -> BTreeSet<String> {
    let mut strings = BTreeSet::new();
    collect_json_scalar_strings(value, &mut strings);
    strings
}

fn collect_json_scalar_strings(value: &Value, strings: &mut BTreeSet<String>) {
    match value {
        Value::String(value) => {
            strings.insert(value.clone());
        }
        Value::Number(value) => {
            strings.insert(value.to_string());
        }
        Value::Bool(value) => {
            strings.insert(value.to_string());
        }
        Value::Array(values) => {
            for value in values {
                collect_json_scalar_strings(value, strings);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                strings.insert(key.clone());
                collect_json_scalar_strings(value, strings);
            }
        }
        Value::Null => {}
    }
}

fn strict_derivation_receipt(
    loaded: &LoadedGeneralizationExecutionEnvelope,
) -> GeneralizationResult<GeneralizationDerivationReceipt> {
    let semantic_artifact_hashes = strict_semantic_artifact_hashes(loaded)?;
    let mut artifact_hashes = Vec::new();
    for trial in &loaded.trials {
        let prefix = format!(
            "{}:{}",
            trial_family_str(trial.execution.family),
            trial.execution.trial_id
        );
        push_candidate_recall_receipt_hashes(
            &mut artifact_hashes,
            &prefix,
            &trial.candidate_recall.references,
        );
        push_solve_derivation_receipt_hashes(
            &mut artifact_hashes,
            &prefix,
            &trial.solve_derivation.references,
        );
        push_cache_execution_receipt_hash(&mut artifact_hashes, &prefix, &trial.cache_execution);
        for artifact in &trial.artifacts {
            let artifact_id = strict_artifact_receipt_id(&prefix, artifact);
            let content_hash = semantic_artifact_hashes
                .get(&strict_artifact_semantic_key(trial, artifact))
                .cloned()
                .unwrap_or_else(|| artifact.reference.content_hash.clone());
            artifact_hashes.push(GeneralizationDerivationArtifactHash {
                artifact_id,
                version: artifact.reference.version.clone(),
                content_hash,
            });
        }
    }
    artifact_hashes.sort();
    let mut leak_source_hashes = loaded
        .trials
        .iter()
        .flat_map(|trial| {
            let prefix = format!(
                "{}:{}",
                trial_family_str(trial.execution.family),
                trial.execution.trial_id
            );
            trial.leak_scan_sources.iter().map(move |source| {
                let mut checked_channels = vec![source.channel];
                checked_channels.sort();
                GeneralizationDerivationLeakSourceHash {
                    source_id: format!("{prefix}:{}", source.source_id),
                    phase: source.phase.clone(),
                    content_hash: source.content_hash.clone(),
                    bundle_content_hash: source.bundle_content_hash.clone(),
                    binding_kind: source.binding_kind,
                    binding_hash: source.binding_hash.clone(),
                    checked_source_hashes: source
                        .checked_sources
                        .iter()
                        .map(|checked| checked.content_hash.clone())
                        .collect(),
                    checked_channels,
                }
            })
        })
        .collect::<Vec<_>>();
    leak_source_hashes.sort();
    Ok(GeneralizationDerivationReceipt {
        source: GeneralizationDerivationSource::StrictExecutionEnvelope,
        self_attested_outcomes_used: false,
        manifest_hash: strict_semantic_manifest_hash(loaded, &semantic_artifact_hashes)?,
        benchmark_hash: loaded.benchmark_content_hash.clone(),
        artifact_hashes,
        leak_source_hashes,
    })
}

fn strict_artifact_receipt_id(prefix: &str, artifact: &LoadedGeneralizationArtifactRef) -> String {
    format!(
        "{prefix}:{}",
        artifact
            .reference
            .artifact_id
            .clone()
            .unwrap_or_else(|| artifact.reference.version.clone())
    )
}

fn strict_artifact_semantic_key(
    trial: &LoadedGeneralizationTrialExecution,
    artifact: &LoadedGeneralizationArtifactRef,
) -> (GeneralizationTrialFamily, String, String, String) {
    (
        trial.execution.family,
        trial.execution.trial_id.clone(),
        artifact.reference.path.clone(),
        artifact.reference.content_hash.clone(),
    )
}

fn strict_semantic_artifact_hashes(
    loaded: &LoadedGeneralizationExecutionEnvelope,
) -> GeneralizationResult<BTreeMap<(GeneralizationTrialFamily, String, String, String), String>> {
    let mut hashes = BTreeMap::new();
    for trial in &loaded.trials {
        for artifact in &trial.artifacts {
            hashes.insert(
                strict_artifact_semantic_key(trial, artifact),
                semantic_content_hash_for_loaded_artifact(artifact)?,
            );
        }
    }
    Ok(hashes)
}

fn semantic_content_hash_for_loaded_artifact(
    artifact: &LoadedGeneralizationArtifactRef,
) -> GeneralizationResult<String> {
    match &artifact.artifact {
        LoadedGeneralizationArtifact::Link(link) => semantic_entity_link_content_hash(link),
        _ => Ok(artifact.reference.content_hash.clone()),
    }
}

fn semantic_entity_link_content_hash(link: &EntityLinkArtifact) -> GeneralizationResult<String> {
    const REVIEW_EXPORT_HANDOFF: &str =
        "<canon:evaluation.generalization:path:entity_link.next_commands.review_export>";
    let mut normalized = link.clone();
    normalized.next_commands.review_export = REVIEW_EXPORT_HANDOFF.to_string();
    normalized.artifact_content_hash.clear();
    normalized.metadata.artifact_content_hash.clear();
    hash_serialized(&normalized)
}

fn strict_semantic_manifest_hash(
    loaded: &LoadedGeneralizationExecutionEnvelope,
    semantic_artifact_hashes: &BTreeMap<
        (GeneralizationTrialFamily, String, String, String),
        String,
    >,
) -> GeneralizationResult<String> {
    let mut envelope = loaded.envelope.clone();
    envelope.trials.sort_by(|left, right| {
        trial_execution_key(left)
            .cmp(&trial_execution_key(right))
            .then_with(|| left.trial_id.cmp(&right.trial_id))
    });
    for trial in &mut envelope.trials {
        trial.artifacts.sort_by(|left, right| {
            left.artifact_id
                .cmp(&right.artifact_id)
                .then_with(|| left.version.cmp(&right.version))
                .then_with(|| left.path.cmp(&right.path))
                .then_with(|| left.content_hash.cmp(&right.content_hash))
        });
        for artifact in &mut trial.artifacts {
            let key = (
                trial.family,
                trial.trial_id.clone(),
                artifact.path.clone(),
                artifact.content_hash.clone(),
            );
            if let Some(semantic_hash) = semantic_artifact_hashes.get(&key) {
                artifact.content_hash = semantic_hash.clone();
            }
        }
    }
    serde_json::to_vec(&envelope)
        .map(|bytes| hash_bytes(&bytes))
        .map_err(artifact_error)
}

fn push_candidate_recall_receipt_hashes(
    artifact_hashes: &mut Vec<GeneralizationDerivationArtifactHash>,
    prefix: &str,
    refs: &GeneralizationCandidateRecallExecutionRefs,
) {
    for (artifact_id, reference) in [
        ("candidate_recall.quality_manifest", &refs.quality_manifest),
        ("candidate_recall.block_artifact", &refs.block_artifact),
        ("candidate_recall.candidates", &refs.candidates),
        ("candidate_recall.diagnostics", &refs.diagnostics),
        (
            "candidate_recall.exact_bucket_assertions",
            &refs.exact_bucket_assertions,
        ),
        ("candidate_recall.report", &refs.report),
    ] {
        artifact_hashes.push(GeneralizationDerivationArtifactHash {
            artifact_id: format!("{prefix}:{artifact_id}"),
            version: reference.version.clone(),
            content_hash: reference.content_hash.clone(),
        });
    }
}

fn push_solve_derivation_receipt_hashes(
    artifact_hashes: &mut Vec<GeneralizationDerivationArtifactHash>,
    prefix: &str,
    refs: &GeneralizationSolveDerivationRefs,
) {
    for (artifact_id, reference) in [
        ("solve_derivation.edge_artifact", &refs.edge_artifact),
        ("solve_derivation.edge_records", &refs.edge_records),
        (
            "solve_derivation.prepared_surfaces",
            &refs.prepared_surfaces,
        ),
        ("solve_derivation.solve_policy", &refs.solve_policy),
    ] {
        artifact_hashes.push(GeneralizationDerivationArtifactHash {
            artifact_id: format!("{prefix}:{artifact_id}"),
            version: reference.version.clone(),
            content_hash: reference.content_hash.clone(),
        });
    }
}

fn push_cache_execution_receipt_hash(
    artifact_hashes: &mut Vec<GeneralizationDerivationArtifactHash>,
    prefix: &str,
    cache_execution: &LoadedGeneralizationCacheExecution,
) {
    artifact_hashes.push(GeneralizationDerivationArtifactHash {
        artifact_id: format!("{prefix}:cache_execution.receipt"),
        version: cache_execution.references.receipt.version.clone(),
        content_hash: cache_execution.references.receipt.content_hash.clone(),
    });
    artifact_hashes.push(GeneralizationDerivationArtifactHash {
        artifact_id: format!("{prefix}:cache_execution.bundle_receipt"),
        version: cache_execution.references.bundle_receipt.version.clone(),
        content_hash: cache_execution
            .references
            .bundle_receipt
            .content_hash
            .clone(),
    });
}

fn read_strict_manifest_file(
    base_dir: &Path,
    field: &str,
    rel: &str,
) -> GeneralizationResult<(PathBuf, Vec<u8>)> {
    let (resolution, bytes) = read_strict_manifest_file_resolved(base_dir, field, rel)?;
    Ok((resolution.absolute_path, bytes))
}

fn read_strict_manifest_file_canonical_path(
    base_dir: &Path,
    field: &str,
    rel: &str,
) -> GeneralizationResult<(PathBuf, Vec<u8>)> {
    let (resolution, bytes) = read_strict_manifest_file_resolved(base_dir, field, rel)?;
    Ok((resolution.canonical_path, bytes))
}

fn read_strict_manifest_file_resolved(
    base_dir: &Path,
    field: &str,
    rel: &str,
) -> GeneralizationResult<(PathResolution, Vec<u8>)> {
    let resolution = resolve_workspace_path(base_dir, field, Path::new(rel), PlannedAccess::Read)
        .map_err(|error| {
        GeneralizationError::new(GeneralizationErrorCode::ArtifactContract, error.to_string())
    })?;
    if !resolution.exists {
        return Err(error(
            GeneralizationErrorCode::MissingReference,
            format!("{field} does not exist"),
        ));
    }
    if resolution.leaf_is_symlink {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must not be a symlink"),
        ));
    }
    let metadata = fs::metadata(&resolution.absolute_path)
        .map_err(|io_error| manifest_io_error(field, io_error))?;
    if !metadata.is_file() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must be a file"),
        ));
    }
    let bytes = fs::read(&resolution.absolute_path)
        .map_err(|io_error| manifest_io_error(field, io_error))?;
    Ok((resolution, bytes))
}

fn resolve_strict_manifest_dir(
    base_dir: &Path,
    field: &str,
    rel: &str,
) -> GeneralizationResult<PathBuf> {
    normalize_path_ref(rel, field)?;
    let resolution = resolve_workspace_path(base_dir, field, Path::new(rel), PlannedAccess::Read)
        .map_err(|error| {
        GeneralizationError::new(GeneralizationErrorCode::ArtifactContract, error.to_string())
    })?;
    if !resolution.exists {
        return Err(error(
            GeneralizationErrorCode::MissingReference,
            format!("{field} does not exist"),
        ));
    }
    if resolution.leaf_is_symlink {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must not be a symlink"),
        ));
    }
    let metadata = fs::metadata(&resolution.absolute_path)
        .map_err(|io_error| manifest_io_error(field, io_error))?;
    if !metadata.is_dir() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must be a directory"),
        ));
    }
    Ok(resolution.absolute_path)
}

fn validate_resource_limit(
    field: &str,
    byte_len: usize,
    max_artifact_bytes: Option<u64>,
) -> GeneralizationResult<()> {
    if let Some(max_artifact_bytes) = max_artifact_bytes
        && byte_len as u64 > max_artifact_bytes
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} exceeds max_artifact_bytes"),
        ));
    }
    Ok(())
}

fn parse_json_or_jsonl<T: DeserializeOwned>(
    bytes: &[u8],
    label: &str,
) -> GeneralizationResult<Vec<T>> {
    let first = bytes
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace());
    if first == Some(b'[') {
        return serde_json::from_slice(bytes).map_err(artifact_error);
    }
    let text = std::str::from_utf8(bytes).map_err(|error| {
        GeneralizationError::new(
            GeneralizationErrorCode::ArtifactContract,
            format!("{label} must be valid UTF-8 JSONL: {error}"),
        )
    })?;
    let mut values = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str(line).map_err(|error| {
            GeneralizationError::new(
                GeneralizationErrorCode::ArtifactContract,
                format!("{label} line {} is invalid JSON: {error}", index + 1),
            )
        })?;
        values.push(value);
    }
    if values.is_empty() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{label} must contain at least one record"),
        ));
    }
    Ok(values)
}

fn read_typed_artifact_ref(
    base_dir: &Path,
    field: &str,
    reference: &GeneralizationTypedArtifactRef,
    max_artifact_bytes: Option<u64>,
) -> GeneralizationResult<(PathBuf, Vec<u8>)> {
    validate_typed_artifact_ref(reference, field)?;
    let (path, bytes) =
        read_strict_manifest_file(base_dir, &format!("{field}.path"), &reference.path)?;
    validate_resource_limit(field, bytes.len(), max_artifact_bytes)?;
    verify_declared_content_hash(
        &format!("{field}.content_hash"),
        &reference.content_hash,
        &bytes,
    )?;
    Ok((path, bytes))
}

fn validate_typed_artifact_ref(
    reference: &GeneralizationTypedArtifactRef,
    field: &str,
) -> GeneralizationResult<()> {
    normalize_path_ref(&reference.path, &format!("{field}.path"))?;
    verify_declared_digest(&format!("{field}.content_hash"), &reference.content_hash)?;
    normalize_non_empty(reference.version.clone(), &format!("{field}.version"))?;
    Ok(())
}

fn validate_candidate_recall_execution_refs(
    refs: &GeneralizationCandidateRecallExecutionRefs,
) -> GeneralizationResult<()> {
    validate_typed_artifact_ref(&refs.quality_manifest, "candidate_recall.quality_manifest")?;
    validate_typed_artifact_ref(&refs.block_artifact, "candidate_recall.block_artifact")?;
    validate_typed_artifact_ref(&refs.candidates, "candidate_recall.candidates")?;
    validate_typed_artifact_ref(&refs.diagnostics, "candidate_recall.diagnostics")?;
    validate_typed_artifact_ref(
        &refs.exact_bucket_assertions,
        "candidate_recall.exact_bucket_assertions",
    )?;
    validate_typed_artifact_ref(&refs.report, "candidate_recall.report")?;
    require_ref_version(
        &refs.quality_manifest,
        CANON_GENERALIZATION_CANDIDATE_RECALL_QUALITY_MANIFEST_VERSION,
        "candidate_recall.quality_manifest",
    )?;
    require_ref_version(
        &refs.block_artifact,
        CANON_ENTITY_BLOCK_VERSION_V1,
        "candidate_recall.block_artifact",
    )?;
    require_ref_version(
        &refs.candidates,
        CANON_ENTITY_BLOCK_VERSION_V1,
        "candidate_recall.candidates",
    )?;
    require_ref_version(
        &refs.diagnostics,
        CANON_ENTITY_BLOCK_VERSION_V1,
        "candidate_recall.diagnostics",
    )?;
    require_ref_version(
        &refs.exact_bucket_assertions,
        CANON_ENTITY_BLOCK_BUCKET_VERSION,
        "candidate_recall.exact_bucket_assertions",
    )?;
    require_ref_version(
        &refs.report,
        CANON_ENTITY_CANDIDATE_RECALL_VERSION,
        "candidate_recall.report",
    )?;
    Ok(())
}

fn validate_solve_derivation_refs(
    refs: &GeneralizationSolveDerivationRefs,
) -> GeneralizationResult<()> {
    validate_typed_artifact_ref(&refs.edge_artifact, "solve_derivation.edge_artifact")?;
    validate_typed_artifact_ref(&refs.edge_records, "solve_derivation.edge_records")?;
    validate_typed_artifact_ref(
        &refs.prepared_surfaces,
        "solve_derivation.prepared_surfaces",
    )?;
    validate_typed_artifact_ref(&refs.solve_policy, "solve_derivation.solve_policy")?;
    require_ref_version(
        &refs.edge_artifact,
        CANON_ENTITY_EVIDENCE_VERSION_V1,
        "solve_derivation.edge_artifact",
    )?;
    require_ref_version(
        &refs.edge_records,
        CANON_ENTITY_EVIDENCE_VERSION_V1,
        "solve_derivation.edge_records",
    )?;
    require_ref_version(
        &refs.prepared_surfaces,
        CANON_ENTITY_PREPARE_VERSION_V1,
        "solve_derivation.prepared_surfaces",
    )?;
    require_ref_version(
        &refs.solve_policy,
        CANON_GENERALIZATION_SOLVE_POLICY_VERSION,
        "solve_derivation.solve_policy",
    )?;
    Ok(())
}

fn require_ref_version(
    reference: &GeneralizationTypedArtifactRef,
    expected: &str,
    field: &str,
) -> GeneralizationResult<()> {
    if reference.version != expected {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.version must be {expected}"),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeneralizationSolvePolicy {
    version: String,
    config: SolveReconciliationConfig,
}

#[derive(Debug, Clone, Deserialize)]
struct GeneralizationCandidateRecallQualityManifest {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    observations: Vec<GeneralizationCandidateRecallManifestObservation>,
    quality_harness: GeneralizationCandidateRecallManifestHarness,
}

#[derive(Debug, Clone, Deserialize)]
struct GeneralizationCandidateRecallManifestObservation {
    observation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GeneralizationCandidateRecallManifestHarness {
    #[serde(default)]
    cases: Vec<GeneralizationCandidateRecallManifestCase>,
}

#[derive(Debug, Clone, Deserialize)]
struct GeneralizationCandidateRecallManifestCase {
    case_id: String,
    left_observation_id: String,
    right_observation_id: String,
    stratum: String,
    label_disposition: String,
}

fn validate_candidate_recall_quality_manifest_version(
    manifest: &GeneralizationCandidateRecallQualityManifest,
    expected: &str,
) -> GeneralizationResult<()> {
    if let Some(version) = manifest.version.as_deref()
        && version != expected
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "candidate-recall quality manifest version does not match envelope ref",
        ));
    }
    Ok(())
}

fn candidate_recall_manifest_gold(
    manifest: &GeneralizationCandidateRecallQualityManifest,
) -> GeneralizationResult<(Vec<String>, Vec<CandidateRecallGoldPair>)> {
    let mut surface_ids = manifest
        .observations
        .iter()
        .map(|observation| ascii_trim(&observation.observation_id))
        .filter(|observation_id| !observation_id.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    surface_ids.sort();
    surface_ids.dedup();
    if surface_ids.is_empty() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "candidate-recall quality manifest must include observations",
        ));
    }

    let surface_set = surface_ids.iter().cloned().collect::<BTreeSet<_>>();
    let mut gold_pairs = Vec::new();
    let mut seen = BTreeSet::new();
    for case in &manifest.quality_harness.cases {
        if case.label_disposition != "same_entity" {
            continue;
        }
        let gold_pair_id = normalize_non_empty(case.case_id.clone(), "candidate_recall.case_id")?;
        if !seen.insert(gold_pair_id.clone()) {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("duplicate candidate-recall gold pair {gold_pair_id}"),
            ));
        }
        let left = normalize_non_empty(
            case.left_observation_id.clone(),
            "candidate_recall.left_observation_id",
        )?;
        let right = normalize_non_empty(
            case.right_observation_id.clone(),
            "candidate_recall.right_observation_id",
        )?;
        if left == right || !surface_set.contains(&left) || !surface_set.contains(&right) {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("candidate-recall gold pair {gold_pair_id} has invalid endpoints"),
            ));
        }
        gold_pairs.push(CandidateRecallGoldPair::new(
            &gold_pair_id,
            &left,
            &right,
            candidate_recall_stratum(&case.stratum)?,
        ));
    }
    if gold_pairs.is_empty() {
        return Err(error(
            GeneralizationErrorCode::MissingReference,
            "candidate-recall quality manifest must include at least one same_entity gold pair",
        ));
    }
    gold_pairs.sort_by(|left, right| left.gold_pair_id.cmp(&right.gold_pair_id));
    Ok((surface_ids, gold_pairs))
}

fn candidate_recall_stratum(value: &str) -> GeneralizationResult<CandidateRecallStratum> {
    match value {
        "exact_known" | "exact_known_replay" => Ok(CandidateRecallStratum::ExactKnown),
        "withheld_alias" | "withheld_alias_incumbent" => Ok(CandidateRecallStratum::WithheldAlias),
        "novel_cluster" | "novel_multi_observation" => Ok(CandidateRecallStratum::NovelCluster),
        "directional_link" | "directional_cross_dataset_link" => {
            Ok(CandidateRecallStratum::DirectionalLink)
        }
        _ => Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("unsupported candidate-recall stratum {value}"),
        )),
    }
}

fn validate_candidate_record_versions(
    records: &[BlockCandidateRecord],
    expected: &str,
) -> GeneralizationResult<()> {
    if records.is_empty() {
        return Err(error(
            GeneralizationErrorCode::MissingReference,
            "candidate records must not be empty",
        ));
    }
    for record in records {
        if record.version != expected {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                "candidate record version does not match envelope ref",
            ));
        }
    }
    Ok(())
}

fn validate_edge_record_versions(
    records: &[EdgeEvidenceRecord],
    expected: &str,
) -> GeneralizationResult<()> {
    if records.is_empty() {
        return Err(error(
            GeneralizationErrorCode::MissingReference,
            "edge records must not be empty",
        ));
    }
    for record in records {
        if record.version != expected {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                "edge record version does not match envelope ref",
            ));
        }
    }
    Ok(())
}

fn validate_exact_bucket_assertions(
    assertions: &[ExactBucketAssertion],
    expected: &str,
) -> GeneralizationResult<()> {
    for assertion in assertions {
        if assertion.version != expected {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                "exact bucket assertion version does not match envelope ref",
            ));
        }
        assertion.validate().map_err(contract_error)?;
    }
    Ok(())
}

fn validate_artifact_ref(
    reference: &GeneralizationArtifactRef,
    field: &str,
) -> GeneralizationResult<()> {
    normalize_path_ref(&reference.path, &format!("{field}.path"))?;
    verify_declared_digest(&format!("{field}.content_hash"), &reference.content_hash)?;
    if reference.version.trim().is_empty() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.version must not be empty"),
        ));
    }
    let inferred = infer_artifact_kind(reference, field)?;
    if let Some(kind) = &reference.kind
        && kind != &inferred
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.kind conflicts with version"),
        ));
    }
    Ok(())
}

fn infer_artifact_kind(
    reference: &GeneralizationArtifactRef,
    field: &str,
) -> GeneralizationResult<GeneralizationArtifactKind> {
    let version = reference.version.as_str();
    if version == CANON_ENTITY_CANDIDATE_RECALL_VERSION {
        Ok(GeneralizationArtifactKind::CandidateRecall)
    } else if version == CANON_ENTITY_LINK_VERSION {
        Ok(GeneralizationArtifactKind::Link)
    } else if version == ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION {
        Ok(GeneralizationArtifactKind::LinkObservationSurfaceBindings)
    } else if version == CANON_ENTITY_RUN_VERSION_V1 {
        Ok(GeneralizationArtifactKind::Run)
    } else if version == CANON_ENTITY_SOLVE_VERSION_V1 {
        Ok(GeneralizationArtifactKind::Solve)
    } else if version == CANON_GENERALIZATION_LEAK_SCAN_SOURCES_VERSION {
        Ok(GeneralizationArtifactKind::LeakScanSources)
    } else {
        Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.version is not a recognized public Canon artifact version"),
        ))
    }
}

fn validate_leak_source_bundle_ref(
    reference: &GeneralizationLeakSourceBundleRef,
    field: &str,
) -> GeneralizationResult<()> {
    if reference.version != CANON_GENERALIZATION_LEAK_SCAN_SOURCES_VERSION {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.version is not the generalization leak-source bundle version"),
        ));
    }
    if reference.phase != GeneralizationLeakSourcePhase::PreEvaluationInfluence {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.phase must be pre_evaluation_influence"),
        ));
    }
    normalize_path_ref(&reference.path, &format!("{field}.path"))?;
    verify_declared_digest(&format!("{field}.content_hash"), &reference.content_hash)?;
    validate_leak_channels(&reference.channels, &format!("{field}.channels"))?;
    Ok(())
}

fn validate_leak_source_bundle(
    bundle: &GeneralizationLeakSourceBundle,
    reference: &GeneralizationLeakSourceBundleRef,
    field: &str,
) -> GeneralizationResult<()> {
    if bundle.version != reference.version {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.version does not match loaded leak-source bundle"),
        ));
    }
    if bundle.scope != "pre_evaluation_influence_only" {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.scope must be pre_evaluation_influence_only"),
        ));
    }
    validate_leak_channels(&bundle.channels, &format!("{field}.channels"))?;
    if bundle.channels != reference.channels {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field}.channels do not match loaded leak-source bundle"),
        ));
    }
    if bundle.sources.is_empty() {
        return Err(error(
            GeneralizationErrorCode::MissingReference,
            "leak-source bundle must include concrete pre-evaluation sources",
        ));
    }
    let mut seen_channels = BTreeSet::new();
    let mut seen_sources = BTreeSet::new();
    for source in &bundle.sources {
        normalize_non_empty(source.source_id.clone(), "leak_source.source_id")?;
        if !seen_sources.insert(source.source_id.as_str()) {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("duplicate leak source {}", source.source_id),
            ));
        }
        if !seen_channels.insert(source.channel) {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("duplicate leak source channel {}", source.channel.as_str()),
            ));
        }
        if source.phase != reference.phase
            || source.phase != GeneralizationLeakSourcePhase::PreEvaluationInfluence
        {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "leak source {} must be pre-evaluation influence",
                    source.source_id
                ),
            ));
        }
        if source.content_hash_basis != "canonical_inline_records"
            || source.protected_match_derivation != "derive_from_checked_sources"
        {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "leak source {} must bind canonical inline records derived from checked sources",
                    source.source_id
                ),
            ));
        }
        validate_leak_source_kind_for_channel(
            source.source_kind,
            source.channel,
            "leak_source.source_kind",
        )?;
        validate_leak_source_coverage(source, "leak_source.coverage")?;
        verify_declared_digest("leak_source.binding_hash", &source.binding_hash)?;
        verify_declared_digest("leak_source.content_hash", &source.content_hash)?;
        if source.records.is_empty() {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "leak source {} must include nonempty records",
                    source.source_id
                ),
            ));
        }
        if source.checked_sources.is_empty() {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "leak source {} must include checked source paths",
                    source.source_id
                ),
            ));
        }
        let mut seen_checked_paths = BTreeSet::new();
        for (checked_index, checked_source) in source.checked_sources.iter().enumerate() {
            normalize_path_ref(
                &checked_source.path,
                &format!("leak_source.checked_sources[{checked_index}].path"),
            )?;
            if !seen_checked_paths.insert(checked_source.path.as_str()) {
                return Err(error(
                    GeneralizationErrorCode::DuplicateRecord,
                    format!(
                        "leak source {} contains a duplicate checked source path",
                        source.source_id
                    ),
                ));
            }
            verify_declared_digest(
                &format!("leak_source.checked_sources[{checked_index}].content_hash"),
                &checked_source.content_hash,
            )?;
            if checked_source.byte_count == 0 || checked_source.record_count == 0 {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!(
                        "leak source {} checked source counts must be nonzero",
                        source.source_id
                    ),
                ));
            }
        }
        let actual_hash = hash_serialized(&source.records)?;
        if source.content_hash != actual_hash {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "leak source {} content_hash does not match canonical inline records",
                    source.source_id
                ),
            ));
        }
    }
    let declared_channels = bundle.channels.iter().copied().collect::<BTreeSet<_>>();
    if seen_channels != declared_channels {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "leak-source bundle must include exactly one source for each declared channel",
        ));
    }
    Ok(())
}

fn validate_leak_source_kind_for_channel(
    source_kind: GeneralizationLeakSourceKind,
    channel: LeakChannel,
    field: &str,
) -> GeneralizationResult<()> {
    let allowed = match channel {
        LeakChannel::Alias => matches!(
            source_kind,
            GeneralizationLeakSourceKind::RegistryTree
                | GeneralizationLeakSourceKind::RegistryAliasFile
        ),
        LeakChannel::Anchor => matches!(
            source_kind,
            GeneralizationLeakSourceKind::RegistryTree
                | GeneralizationLeakSourceKind::RegistryAnchorFile
        ),
        LeakChannel::Threshold => source_kind == GeneralizationLeakSourceKind::Threshold,
        LeakChannel::Dictionary => source_kind == GeneralizationLeakSourceKind::Dictionary,
        LeakChannel::Patch => source_kind == GeneralizationLeakSourceKind::Patch,
        LeakChannel::Cache => source_kind == GeneralizationLeakSourceKind::Cache,
        LeakChannel::GeneratedCorpus => {
            source_kind == GeneralizationLeakSourceKind::GeneratedCorpus
        }
    };
    if !allowed {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "{field} is incompatible with leak channel {}",
                channel.as_str()
            ),
        ));
    }
    Ok(())
}

fn validate_leak_source_coverage(
    source: &GeneralizationStructuredLeakSource,
    field: &str,
) -> GeneralizationResult<()> {
    let registry_source = matches!(
        source.source_kind,
        GeneralizationLeakSourceKind::RegistryTree
            | GeneralizationLeakSourceKind::RegistryAliasFile
            | GeneralizationLeakSourceKind::RegistryAnchorFile
    ) || matches!(source.channel, LeakChannel::Alias | LeakChannel::Anchor);
    if registry_source
        && !matches!(
            source.coverage,
            GeneralizationLeakSourceCoverage::CompleteRegistryTree
                | GeneralizationLeakSourceCoverage::CompleteRelevantSubtree
        )
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must be complete for registry and alias sources"),
        ));
    }
    if registry_source && source.completeness_manifest.is_none() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} requires a registry completeness manifest"),
        ));
    }
    if !registry_source && source.coverage != GeneralizationLeakSourceCoverage::CompleteSource {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must declare complete_source for non-registry sources"),
        ));
    }
    if !registry_source && source.completeness_manifest.is_some() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} completeness manifest is only valid for registry sources"),
        ));
    }
    Ok(())
}

fn validate_leak_channels(channels: &[LeakChannel], field: &str) -> GeneralizationResult<()> {
    let required_channels = BTreeSet::from([
        LeakChannel::Alias,
        LeakChannel::Anchor,
        LeakChannel::Threshold,
        LeakChannel::Dictionary,
        LeakChannel::Patch,
        LeakChannel::Cache,
        LeakChannel::GeneratedCorpus,
    ]);
    let declared_channels = channels.iter().copied().collect::<BTreeSet<_>>();
    if declared_channels != required_channels {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must include exactly all leakage channels"),
        ));
    }
    Ok(())
}

fn verify_declared_content_hash(
    field: &str,
    declared: &str,
    bytes: &[u8],
) -> GeneralizationResult<()> {
    verify_declared_digest(field, declared)?;
    let actual = hash_bytes(bytes);
    if declared != actual {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} is stale or does not match artifact bytes"),
        ));
    }
    Ok(())
}

fn verify_declared_digest(field: &str, declared: &str) -> GeneralizationResult<()> {
    if !is_blake3_digest(declared) {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must be a blake3 digest"),
        ));
    }
    Ok(())
}

fn validate_json_version(field: &str, value: &Value, expected: &str) -> GeneralizationResult<()> {
    let Some(actual) = value.get("version").and_then(Value::as_str) else {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} artifact is missing version"),
        ));
    };
    if actual != expected {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} artifact has the wrong version"),
        ));
    }
    Ok(())
}

fn validate_run_artifact_contract(artifact: &EntityRunArtifact) -> GeneralizationResult<()> {
    if artifact.version != CANON_ENTITY_RUN_VERSION_V1 {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "run artifact has the wrong contract version",
        ));
    }
    verify_declared_digest("run.artifact_content_hash", &artifact.artifact_content_hash)?;
    if artifact.metadata.artifact_content_hash != artifact.artifact_content_hash {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "run artifact metadata hash does not match artifact hash",
        ));
    }
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let expected = hash_serialized(&hashable)?;
    if artifact.artifact_content_hash != expected
        || artifact.metadata.artifact_content_hash != expected
    {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "run artifact self hash is stale",
        ));
    }
    let mut has_solve = false;
    for stage in &artifact.stage_artifacts {
        if stage.stage.trim().is_empty()
            || stage.version.trim().is_empty()
            || stage.path.trim().is_empty()
        {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                "run stage artifacts must include stage, version, and path",
            ));
        }
        verify_declared_digest(
            "run.stage_artifacts.artifact_content_hash",
            &stage.artifact_content_hash,
        )?;
        if stage.stage == "solve" && stage.version == CANON_ENTITY_SOLVE_VERSION_V1 {
            has_solve = true;
        }
    }
    if !has_solve {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "run artifact must include the solve stage",
        ));
    }
    Ok(())
}

fn normalize_path_ref(path: &str, field: &str) -> GeneralizationResult<()> {
    if ascii_trim(path).is_empty() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must not be empty"),
        ));
    }
    Ok(())
}

fn manifest_io_error(field: &str, error: io::Error) -> GeneralizationError {
    GeneralizationError::new(
        GeneralizationErrorCode::ArtifactContract,
        format!("{field} could not be read: {}", error.kind()),
    )
}

fn contract_error(error: impl fmt::Debug) -> GeneralizationError {
    GeneralizationError::new(
        GeneralizationErrorCode::ArtifactContract,
        format!("{error:?}"),
    )
}

pub fn compile_generalization_benchmark(
    benchmark: GeneralizationBenchmark,
) -> GeneralizationResult<GeneralizationReport> {
    validate_unit_fixture_self_attested_outcomes(&benchmark)?;
    compile_generalization_benchmark_internal(benchmark)
}

fn compile_generalization_benchmark_internal(
    benchmark: GeneralizationBenchmark,
) -> GeneralizationResult<GeneralizationReport> {
    let benchmark = finalize_benchmark(benchmark)?;
    let benchmark_digest = generalization_benchmark_digest(&benchmark)?;

    let mut entity_reports = Vec::with_capacity(benchmark.entity_disjoint_trials.len());
    for trial in &benchmark.entity_disjoint_trials {
        entity_reports.push(compile_entity_disjoint_trial(trial)?);
    }
    entity_reports.sort_by(|left, right| left.trial_id.cmp(&right.trial_id));

    let mut time_reports = Vec::with_capacity(benchmark.time_forward_trials.len());
    for trial in &benchmark.time_forward_trials {
        time_reports.push(compile_time_forward_trial(trial)?);
    }
    time_reports.sort_by(|left, right| left.trial_id.cmp(&right.trial_id));

    let aggregate = aggregate_reports(&entity_reports, &time_reports);
    let quality = generalization_quality_contract_report(&benchmark);
    let mut report = GeneralizationReport {
        version: CANON_GENERALIZATION_VERSION.to_string(),
        benchmark_id: benchmark.benchmark_id,
        corpus_visibility: benchmark.corpus_visibility,
        corpus_ref: benchmark.corpus_ref,
        benchmark_digest,
        report_digest: String::new(),
        entity_disjoint: entity_reports,
        time_forward: time_reports,
        aggregate,
        quality,
        derivation: None,
    };
    report.report_digest = generalization_report_digest(&report)?;
    Ok(report)
}

fn validate_unit_fixture_self_attested_outcomes(
    benchmark: &GeneralizationBenchmark,
) -> GeneralizationResult<()> {
    for trial in &benchmark.entity_disjoint_trials {
        validate_unit_fixture_results(&trial.discovery_results, &trial.trial_id)?;
        validate_unit_fixture_directional_links(&trial.directional_links, &trial.trial_id)?;
    }
    for trial in &benchmark.time_forward_trials {
        validate_unit_fixture_results(&trial.event_results, &trial.trial_id)?;
        validate_unit_fixture_directional_links(&trial.directional_links, &trial.trial_id)?;
    }
    Ok(())
}

fn validate_unit_fixture_results(
    results: &[DiscoveryResultRecord],
    trial_id: &str,
) -> GeneralizationResult<()> {
    for result in results {
        if result.actual_decision != result.expected_decision
            || result.actual_decision == DiscoveryDecision::FalseMerge
        {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "raw GeneralizationBenchmark is unit-fixture-only; result {} in {trial_id} requires strict artifact derivation",
                    result.result_id
                ),
            ));
        }
    }
    Ok(())
}

fn validate_unit_fixture_directional_links(
    links: &[DirectionalCrossSourceLink],
    trial_id: &str,
) -> GeneralizationResult<()> {
    for link in links {
        if link.actual_decision != link.expected_decision
            || link.actual_decision == DiscoveryDecision::FalseMerge
        {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "raw GeneralizationBenchmark is unit-fixture-only; directional link {} in {trial_id} requires strict artifact derivation",
                    link.link_id
                ),
            ));
        }
    }
    Ok(())
}

pub fn compile_entity_disjoint_trial(
    trial: &EntityDisjointTrial,
) -> GeneralizationResult<EntityDisjointTrialReport> {
    let trial = canonicalize_entity_disjoint_trial(trial.clone())?;
    validate_entity_disjoint_split(&trial)?;
    validate_discovery_result_refs(&trial.discovery_results, &trial.observations)?;
    validate_hard_negatives(&trial.hard_negatives, &trial.observations)?;
    validate_directional_links(&trial.directional_links, &trial.observations)?;

    let holdout_protected =
        protected_values_for_partition(&trial.observations, BenchmarkPartition::Holdout);
    refuse_leakage(
        &trial.leakage_probes,
        ProtectedSet::HoldoutEntity,
        &holdout_protected,
        &trial.trial_id,
    )?;

    let tune_observations = trial
        .observations
        .iter()
        .filter(|observation| observation.partition == BenchmarkPartition::Tune)
        .cloned()
        .collect::<Vec<_>>();
    let holdout_observations = trial
        .observations
        .iter()
        .filter(|observation| observation.partition == BenchmarkPartition::Holdout)
        .cloned()
        .collect::<Vec<_>>();

    let strata = strata_for_results(&trial.discovery_results, &trial.observations)?;
    let novel_results = trial
        .discovery_results
        .iter()
        .filter(|result| result.expected_decision == DiscoveryDecision::ClusterNew)
        .collect::<Vec<_>>();
    let correct_novel_cluster_count = novel_results
        .iter()
        .filter(|result| result.actual_decision == result.expected_decision)
        .count();
    let critical_false_merge_count = critical_false_merges(&trial.hard_negatives);

    Ok(EntityDisjointTrialReport {
        trial_id: trial.trial_id,
        clean_snapshot_digest: hash_serialized(&tune_observations)?,
        protected_holdout_digest: hash_serialized(&holdout_observations)?,
        novel_cluster_result_count: novel_results.len(),
        correct_novel_cluster_count,
        related_distinct_hard_negative_count: trial
            .hard_negatives
            .iter()
            .filter(|control| {
                matches!(
                    control.relation_class,
                    RelationClass::RelatedDistinct
                        | RelationClass::Hierarchy
                        | RelationClass::Lookalike
                )
            })
            .count(),
        critical_false_merge_count,
        directional_cross_source_count: trial.directional_links.len(),
        strata,
    })
}

pub fn compile_time_forward_trial(
    trial: &TimeForwardTrial,
) -> GeneralizationResult<TimeForwardTrialReport> {
    let trial = canonicalize_time_forward_trial(trial.clone())?;
    let observations_by_id = observations_by_id(&trial.observations)?;
    validate_time_forward_cutoff(&trial, &observations_by_id)?;
    validate_discovery_result_refs(&trial.event_results, &trial.observations)?;
    validate_hard_negatives(&trial.hard_negatives, &trial.observations)?;
    validate_directional_links(&trial.directional_links, &trial.observations)?;

    let future_protected = trial
        .evaluation_observation_ids
        .iter()
        .filter_map(|id| observations_by_id.get(id))
        .flat_map(protected_values_for_observation)
        .collect::<BTreeSet<_>>();
    refuse_leakage(
        &trial.leakage_probes,
        ProtectedSet::FutureObservation,
        &future_protected,
        &trial.trial_id,
    )?;

    let build_observations = trial
        .build_observation_ids
        .iter()
        .map(|id| {
            observations_by_id
                .get(id)
                .expect("build refs validated")
                .clone()
        })
        .collect::<Vec<_>>();
    let eval_observations = trial
        .evaluation_observation_ids
        .iter()
        .map(|id| {
            observations_by_id
                .get(id)
                .expect("eval refs validated")
                .clone()
        })
        .collect::<Vec<_>>();

    let strata = strata_for_results(&trial.event_results, &trial.observations)?;
    let correct_evaluation_count = trial
        .event_results
        .iter()
        .filter(|result| result.actual_decision == result.expected_decision)
        .count();
    let critical_false_merge_count = critical_false_merges(&trial.hard_negatives);

    Ok(TimeForwardTrialReport {
        trial_id: trial.trial_id,
        cutoff: trial.cutoff,
        build_snapshot_digest: hash_serialized(&build_observations)?,
        protected_future_digest: hash_serialized(&eval_observations)?,
        evaluation_result_count: trial.event_results.len(),
        correct_evaluation_count,
        renamed_surface_count: count_results_by_relation(
            &trial.event_results,
            &trial.observations,
            RelationClass::RenameContinuity,
        )?,
        new_entity_count: count_results_by_relation(
            &trial.event_results,
            &trial.observations,
            RelationClass::NewEntity,
        )?,
        changed_relationship_count: count_results_by_relation(
            &trial.event_results,
            &trial.observations,
            RelationClass::ChangedRelationship,
        )?,
        critical_false_merge_count,
        directional_cross_source_count: trial.directional_links.len(),
        strata,
    })
}

#[derive(Debug, Default)]
struct GeneralizationQualityCounts {
    candidate_recall_hits: u64,
    candidate_recall_total: u64,
    auto_link_precision_hits: u64,
    auto_link_precision_total: u64,
    auto_link_recall_hits: u64,
    auto_link_recall_total: u64,
    critical_false_merges: u64,
    accounted_cases: u64,
    total_cases: u64,
}

fn generalization_quality_contract_report(
    benchmark: &GeneralizationBenchmark,
) -> GeneralizationQualityContractReport {
    let mut counts = GeneralizationQualityCounts::default();
    for trial in &benchmark.entity_disjoint_trials {
        accumulate_quality_results(&mut counts, &trial.discovery_results);
        accumulate_quality_directional_links(&mut counts, &trial.directional_links);
        accumulate_quality_hard_negatives(&mut counts, &trial.hard_negatives);
    }
    for trial in &benchmark.time_forward_trials {
        accumulate_quality_results(&mut counts, &trial.event_results);
        accumulate_quality_directional_links(&mut counts, &trial.directional_links);
        accumulate_quality_hard_negatives(&mut counts, &trial.hard_negatives);
    }

    let gates = vec![
        quality_rate_gate(
            "candidate_recall_at_50_min",
            "candidate_recall_at_50",
            ">=",
            counts.candidate_recall_hits,
            counts.candidate_recall_total,
            QUALITY_GATE_CANDIDATE_RECALL_AT_50_MIN,
        ),
        quality_rate_gate(
            "auto_link_precision_min",
            "auto_link_precision",
            ">=",
            counts.auto_link_precision_hits,
            counts.auto_link_precision_total,
            QUALITY_GATE_AUTO_LINK_PRECISION_MIN,
        ),
        quality_rate_gate(
            "auto_link_recall_min",
            "auto_link_recall",
            ">=",
            counts.auto_link_recall_hits,
            counts.auto_link_recall_total,
            QUALITY_GATE_AUTO_LINK_RECALL_MIN,
        ),
        quality_count_gate(
            "critical_false_merges_max",
            "hard_negative_false_merges",
            "==",
            counts.critical_false_merges,
            QUALITY_GATE_CRITICAL_FALSE_MERGES_MAX,
        ),
        quality_rate_gate(
            "accounted_case_rate_min",
            "accounted_case_rate",
            "==",
            counts.accounted_cases,
            counts.total_cases,
            QUALITY_GATE_ACCOUNTED_CASE_RATE_MIN,
        ),
    ];
    let release_claim_status = if gates
        .iter()
        .all(|gate| gate.status == GeneralizationQualityGateStatus::Pass)
    {
        GeneralizationReleaseClaimStatus::Eligible
    } else {
        GeneralizationReleaseClaimStatus::Blocked
    };
    GeneralizationQualityContractReport {
        version: CANON_GENERALIZATION_QUALITY_GATE_REPORT_VERSION.to_string(),
        contract_version: CANON_ENTITY_QUALITY_VERSION.to_string(),
        release_claim_status,
        gates,
    }
}

fn accumulate_quality_results(
    counts: &mut GeneralizationQualityCounts,
    results: &[DiscoveryResultRecord],
) {
    for result in results {
        accumulate_labeled_quality_case(
            counts,
            result.expected_decision,
            result.actual_decision,
            result.candidate_rank,
        );
    }
}

fn accumulate_quality_directional_links(
    counts: &mut GeneralizationQualityCounts,
    links: &[DirectionalCrossSourceLink],
) {
    for link in links {
        accumulate_labeled_quality_case(
            counts,
            link.expected_decision,
            link.actual_decision,
            link.candidate_rank,
        );
    }
}

fn accumulate_labeled_quality_case(
    counts: &mut GeneralizationQualityCounts,
    expected_decision: DiscoveryDecision,
    actual_decision: DiscoveryDecision,
    candidate_rank: Option<u32>,
) {
    counts.total_cases += 1;
    counts.accounted_cases += 1;
    if is_quality_must_link(expected_decision) {
        counts.candidate_recall_total += 1;
        if candidate_rank.is_some_and(|rank| rank <= 50) {
            counts.candidate_recall_hits += 1;
        }
        counts.auto_link_recall_total += 1;
        if actual_decision == expected_decision {
            counts.auto_link_recall_hits += 1;
        }
    }
    if is_quality_auto_link(actual_decision) {
        counts.auto_link_precision_total += 1;
        if actual_decision == expected_decision && is_quality_must_link(expected_decision) {
            counts.auto_link_precision_hits += 1;
        }
    } else if actual_decision == DiscoveryDecision::FalseMerge {
        counts.auto_link_precision_total += 1;
    }
}

fn accumulate_quality_hard_negatives(
    counts: &mut GeneralizationQualityCounts,
    controls: &[HardNegativeControl],
) {
    for control in controls {
        counts.total_cases += 1;
        counts.accounted_cases += 1;
        if control.false_merge {
            counts.auto_link_precision_total += 1;
            if control.severity == Severity::Critical {
                counts.critical_false_merges += 1;
            }
        }
    }
}

const fn is_quality_must_link(decision: DiscoveryDecision) -> bool {
    matches!(
        decision,
        DiscoveryDecision::AttachExisting | DiscoveryDecision::ClusterNew
    )
}

const fn is_quality_auto_link(decision: DiscoveryDecision) -> bool {
    is_quality_must_link(decision)
}

fn quality_rate_gate(
    gate_id: &str,
    metric_id: &str,
    operator: &str,
    numerator: u64,
    denominator: u64,
    threshold: f64,
) -> GeneralizationQualityGateReport {
    let observed_value = (denominator > 0).then(|| numerator as f64 / denominator as f64);
    let status = match observed_value {
        None => GeneralizationQualityGateStatus::NotApplicable,
        Some(observed) if quality_gate_passes(observed, operator, threshold) => {
            GeneralizationQualityGateStatus::Pass
        }
        Some(_) => GeneralizationQualityGateStatus::Fail,
    };
    quality_gate_report(
        gate_id,
        metric_id,
        operator,
        observed_value,
        threshold,
        status,
    )
}

fn quality_count_gate(
    gate_id: &str,
    metric_id: &str,
    operator: &str,
    count: u64,
    threshold: f64,
) -> GeneralizationQualityGateReport {
    let observed_value = Some(count as f64);
    let status = if quality_gate_passes(count as f64, operator, threshold) {
        GeneralizationQualityGateStatus::Pass
    } else {
        GeneralizationQualityGateStatus::Fail
    };
    quality_gate_report(
        gate_id,
        metric_id,
        operator,
        observed_value,
        threshold,
        status,
    )
}

fn quality_gate_report(
    gate_id: &str,
    metric_id: &str,
    operator: &str,
    observed_value: Option<f64>,
    threshold: f64,
    status: GeneralizationQualityGateStatus,
) -> GeneralizationQualityGateReport {
    GeneralizationQualityGateReport {
        gate_id: gate_id.to_string(),
        metric_id: metric_id.to_string(),
        status,
        observed_value,
        operator: operator.to_string(),
        threshold,
        waiver_bead_id: None,
    }
}

fn quality_gate_passes(observed: f64, operator: &str, threshold: f64) -> bool {
    match operator {
        ">=" => observed >= threshold,
        "==" => (observed - threshold).abs() < f64::EPSILON,
        "<=" => observed <= threshold,
        _ => false,
    }
}

pub fn canonical_benchmark_bytes(
    benchmark: &GeneralizationBenchmark,
) -> GeneralizationResult<Vec<u8>> {
    let benchmark = finalize_benchmark(benchmark.clone())?;
    serde_json::to_vec(&benchmark).map_err(artifact_error)
}

pub fn canonical_report_bytes(report: &GeneralizationReport) -> GeneralizationResult<Vec<u8>> {
    let mut report = report.clone();
    report
        .entity_disjoint
        .sort_by(|left, right| left.trial_id.cmp(&right.trial_id));
    report
        .time_forward
        .sort_by(|left, right| left.trial_id.cmp(&right.trial_id));
    report.aggregate.strata.sort();
    report
        .quality
        .gates
        .sort_by(|left, right| left.gate_id.cmp(&right.gate_id));
    if let Some(derivation) = &mut report.derivation {
        derivation.artifact_hashes.sort();
        derivation.leak_source_hashes.sort();
        for source in &mut derivation.leak_source_hashes {
            source.checked_channels.sort();
        }
    }
    serde_json::to_vec(&report).map_err(artifact_error)
}

pub fn generalization_benchmark_digest(
    benchmark: &GeneralizationBenchmark,
) -> GeneralizationResult<String> {
    Ok(hash_bytes(&canonical_benchmark_bytes(benchmark)?))
}

pub fn generalization_report_digest(report: &GeneralizationReport) -> GeneralizationResult<String> {
    let mut report = report.clone();
    report.report_digest.clear();
    Ok(hash_bytes(&canonical_report_bytes(&report)?))
}

pub fn finalize_benchmark(
    mut benchmark: GeneralizationBenchmark,
) -> GeneralizationResult<GeneralizationBenchmark> {
    if ascii_trim(&benchmark.version).is_empty() {
        benchmark.version = CANON_GENERALIZATION_VERSION.to_string();
    }
    if benchmark.version != CANON_GENERALIZATION_VERSION {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("unsupported generalization version {}", benchmark.version),
        ));
    }
    benchmark.benchmark_id = normalize_non_empty(benchmark.benchmark_id, "benchmark_id")?;
    benchmark.corpus_ref = normalize_non_empty(benchmark.corpus_ref, "corpus_ref")?;
    benchmark.policy_digest = normalize_digest(benchmark.policy_digest, "policy_digest")?;
    benchmark.entity_disjoint_trials = benchmark
        .entity_disjoint_trials
        .into_iter()
        .map(canonicalize_entity_disjoint_trial)
        .collect::<GeneralizationResult<Vec<_>>>()?;
    benchmark.entity_disjoint_trials.sort();
    benchmark.entity_disjoint_trials = dedup_or_conflict(
        benchmark.entity_disjoint_trials,
        |trial| trial.trial_id.clone(),
        "entity_disjoint_trial",
    )?;
    benchmark.time_forward_trials = benchmark
        .time_forward_trials
        .into_iter()
        .map(canonicalize_time_forward_trial)
        .collect::<GeneralizationResult<Vec<_>>>()?;
    benchmark.time_forward_trials.sort();
    benchmark.time_forward_trials = dedup_or_conflict(
        benchmark.time_forward_trials,
        |trial| trial.trial_id.clone(),
        "time_forward_trial",
    )?;
    if benchmark.entity_disjoint_trials.is_empty() && benchmark.time_forward_trials.is_empty() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "generalization benchmark must include at least one trial",
        ));
    }
    Ok(benchmark)
}

fn canonicalize_entity_disjoint_trial(
    mut trial: EntityDisjointTrial,
) -> GeneralizationResult<EntityDisjointTrial> {
    trial.trial_id = normalize_non_empty(trial.trial_id, "trial_id")?;
    trial.observations = normalize_observations(trial.observations)?;
    for observation in &trial.observations {
        if !matches!(
            observation.partition,
            BenchmarkPartition::Tune | BenchmarkPartition::Holdout
        ) {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "entity-disjoint observation {} must be tune or holdout",
                    observation.observation_id
                ),
            ));
        }
    }
    trial.discovery_results = normalize_discovery_results(trial.discovery_results)?;
    trial.hard_negatives = normalize_hard_negatives(trial.hard_negatives)?;
    trial.directional_links = normalize_directional_links(trial.directional_links)?;
    trial.leakage_probes = normalize_leakage_probes(trial.leakage_probes)?;
    Ok(trial)
}

fn canonicalize_time_forward_trial(
    mut trial: TimeForwardTrial,
) -> GeneralizationResult<TimeForwardTrial> {
    trial.trial_id = normalize_non_empty(trial.trial_id, "trial_id")?;
    trial.cutoff = normalize_non_empty(trial.cutoff, "cutoff")?;
    parse_canonical_temporal_value(&trial.cutoff, "cutoff")?;
    trial.observations = normalize_observations(trial.observations)?;
    for observation in &trial.observations {
        if !matches!(
            observation.partition,
            BenchmarkPartition::Build | BenchmarkPartition::Evaluation
        ) {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "time-forward observation {} must be build or evaluation",
                    observation.observation_id
                ),
            ));
        }
    }
    trial.build_observation_ids =
        normalize_string_vec(trial.build_observation_ids, "build_observation_id")?;
    trial.evaluation_observation_ids = normalize_string_vec(
        trial.evaluation_observation_ids,
        "evaluation_observation_id",
    )?;
    trial.event_results = normalize_discovery_results(trial.event_results)?;
    trial.hard_negatives = normalize_hard_negatives(trial.hard_negatives)?;
    trial.directional_links = normalize_directional_links(trial.directional_links)?;
    trial.leakage_probes = normalize_leakage_probes(trial.leakage_probes)?;
    Ok(trial)
}

fn normalize_observations(
    observations: Vec<GeneralizationObservation>,
) -> GeneralizationResult<Vec<GeneralizationObservation>> {
    let mut observations = observations
        .into_iter()
        .map(|mut observation| {
            observation.observation_id =
                normalize_non_empty(observation.observation_id, "observation_id")?;
            observation.canonical_entity_id =
                normalize_non_empty(observation.canonical_entity_id, "canonical_entity_id")?;
            observation.dataset_id = normalize_non_empty(observation.dataset_id, "dataset_id")?;
            observation.observed_at = normalize_non_empty(observation.observed_at, "observed_at")?;
            parse_canonical_temporal_value(&observation.observed_at, "observed_at")?;
            observation.surface = normalize_non_empty(observation.surface, "surface")?;
            Ok(observation)
        })
        .collect::<GeneralizationResult<Vec<_>>>()?;
    observations.sort();
    dedup_or_conflict(
        observations,
        |observation| observation.observation_id.clone(),
        "observation",
    )
}

fn normalize_discovery_results(
    results: Vec<DiscoveryResultRecord>,
) -> GeneralizationResult<Vec<DiscoveryResultRecord>> {
    let mut results = results
        .into_iter()
        .map(|mut result| {
            result.result_id = normalize_non_empty(result.result_id, "result_id")?;
            result.observation_ids =
                normalize_string_vec(result.observation_ids, "result_observation_id")?;
            if result.observation_ids.is_empty() {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!(
                        "result {} must reference at least one observation",
                        result.result_id
                    ),
                ));
            }
            if result.candidate_rank == Some(0) {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!("result {} candidate_rank is 1-based", result.result_id),
                ));
            }
            result.evidence_lanes = normalize_evidence_lanes(result.evidence_lanes)?;
            Ok(result)
        })
        .collect::<GeneralizationResult<Vec<_>>>()?;
    results.sort();
    dedup_or_conflict(
        results,
        |result| result.result_id.clone(),
        "discovery_result",
    )
}

fn normalize_evidence_lanes(
    lanes: Vec<EvidenceLaneSummary>,
) -> GeneralizationResult<Vec<EvidenceLaneSummary>> {
    let mut lanes = lanes
        .into_iter()
        .map(|mut lane| {
            lane.lane_id = normalize_non_empty(lane.lane_id, "lane_id")?;
            Ok(lane)
        })
        .collect::<GeneralizationResult<Vec<_>>>()?;
    lanes.sort();
    dedup_or_conflict(lanes, |lane| lane.lane_id.clone(), "evidence_lane")
}

fn normalize_hard_negatives(
    controls: Vec<HardNegativeControl>,
) -> GeneralizationResult<Vec<HardNegativeControl>> {
    let mut controls = controls
        .into_iter()
        .map(|mut control| {
            control.control_id = normalize_non_empty(control.control_id, "control_id")?;
            control.left_observation_id =
                normalize_non_empty(control.left_observation_id, "left_observation_id")?;
            control.right_observation_id =
                normalize_non_empty(control.right_observation_id, "right_observation_id")?;
            Ok(control)
        })
        .collect::<GeneralizationResult<Vec<_>>>()?;
    controls.sort();
    dedup_or_conflict(
        controls,
        |control| control.control_id.clone(),
        "hard_negative",
    )
}

fn normalize_directional_links(
    links: Vec<DirectionalCrossSourceLink>,
) -> GeneralizationResult<Vec<DirectionalCrossSourceLink>> {
    let mut links = links
        .into_iter()
        .map(|mut link| {
            link.link_id = normalize_non_empty(link.link_id, "link_id")?;
            link.reference_observation_id =
                normalize_non_empty(link.reference_observation_id, "reference_observation_id")?;
            link.target_observation_id =
                normalize_non_empty(link.target_observation_id, "target_observation_id")?;
            link.reference_dataset_id =
                normalize_non_empty(link.reference_dataset_id, "reference_dataset_id")?;
            link.target_dataset_id =
                normalize_non_empty(link.target_dataset_id, "target_dataset_id")?;
            if link.candidate_rank == Some(0) {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!("link {} candidate_rank is 1-based", link.link_id),
                ));
            }
            Ok(link)
        })
        .collect::<GeneralizationResult<Vec<_>>>()?;
    links.sort();
    dedup_or_conflict(links, |link| link.link_id.clone(), "directional_link")
}

fn normalize_leakage_probes(probes: Vec<LeakageProbe>) -> GeneralizationResult<Vec<LeakageProbe>> {
    let mut probes = probes
        .into_iter()
        .map(|mut probe| {
            probe.locator = normalize_non_empty(probe.locator, "leak_locator")?;
            probe.value = normalize_non_empty(probe.value, "leak_value")?;
            Ok(probe)
        })
        .collect::<GeneralizationResult<Vec<_>>>()?;
    probes.sort();
    probes.dedup();
    Ok(probes)
}

fn validate_entity_disjoint_split(trial: &EntityDisjointTrial) -> GeneralizationResult<()> {
    let mut partitions_by_entity = BTreeMap::<String, BTreeSet<BenchmarkPartition>>::new();
    for observation in &trial.observations {
        partitions_by_entity
            .entry(observation.canonical_entity_id.clone())
            .or_default()
            .insert(observation.partition);
    }
    for (entity_id, partitions) in partitions_by_entity {
        if partitions.contains(&BenchmarkPartition::Tune)
            && partitions.contains(&BenchmarkPartition::Holdout)
        {
            return Err(error(
                GeneralizationErrorCode::EntityDisjointLeak,
                format!("entity {entity_id} appears in both tune and holdout"),
            ));
        }
    }
    Ok(())
}

fn validate_time_forward_cutoff(
    trial: &TimeForwardTrial,
    observations_by_id: &BTreeMap<String, GeneralizationObservation>,
) -> GeneralizationResult<()> {
    if trial.build_observation_ids.is_empty() || trial.evaluation_observation_ids.is_empty() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "time-forward trial {} needs build and evaluation observations",
                trial.trial_id
            ),
        ));
    }

    let cutoff = parse_canonical_temporal_value(&trial.cutoff, "cutoff")?;
    for id in &trial.build_observation_ids {
        let observation = observations_by_id.get(id).ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!("build observation {id} is missing in {}", trial.trial_id),
            )
        })?;
        let observed = parse_canonical_temporal_value(&observation.observed_at, "observed_at")?;
        if observation.partition != BenchmarkPartition::Build || observed >= cutoff {
            return Err(error(
                GeneralizationErrorCode::TemporalReversal,
                format!(
                    "build observation {} is not strictly before cutoff {}",
                    observation.observation_id, trial.cutoff
                ),
            ));
        }
    }
    for id in &trial.evaluation_observation_ids {
        let observation = observations_by_id.get(id).ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!(
                    "evaluation observation {id} is missing in {}",
                    trial.trial_id
                ),
            )
        })?;
        let observed = parse_canonical_temporal_value(&observation.observed_at, "observed_at")?;
        if observation.partition != BenchmarkPartition::Evaluation || observed <= cutoff {
            return Err(error(
                GeneralizationErrorCode::TemporalReversal,
                format!(
                    "evaluation observation {} is not strictly after cutoff {}",
                    observation.observation_id, trial.cutoff
                ),
            ));
        }
    }
    Ok(())
}

fn validate_discovery_result_refs(
    results: &[DiscoveryResultRecord],
    observations: &[GeneralizationObservation],
) -> GeneralizationResult<()> {
    let ids = observations
        .iter()
        .map(|observation| observation.observation_id.as_str())
        .collect::<BTreeSet<_>>();
    for result in results {
        for observation_id in &result.observation_ids {
            if !ids.contains(observation_id.as_str()) {
                return Err(error(
                    GeneralizationErrorCode::MissingReference,
                    format!(
                        "result {} references missing observation {}",
                        result.result_id, observation_id
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_hard_negatives(
    controls: &[HardNegativeControl],
    observations: &[GeneralizationObservation],
) -> GeneralizationResult<()> {
    let ids = observations
        .iter()
        .map(|observation| observation.observation_id.as_str())
        .collect::<BTreeSet<_>>();
    for control in controls {
        if !ids.contains(control.left_observation_id.as_str())
            || !ids.contains(control.right_observation_id.as_str())
        {
            return Err(error(
                GeneralizationErrorCode::MissingReference,
                format!(
                    "hard negative {} references missing observation",
                    control.control_id
                ),
            ));
        }
    }
    Ok(())
}

fn validate_directional_links(
    links: &[DirectionalCrossSourceLink],
    observations: &[GeneralizationObservation],
) -> GeneralizationResult<()> {
    let by_id = observations_by_id(observations)?;
    for link in links {
        let reference = by_id.get(&link.reference_observation_id).ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!(
                    "directional link {} references missing reference observation",
                    link.link_id
                ),
            )
        })?;
        let target = by_id.get(&link.target_observation_id).ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!(
                    "directional link {} references missing target observation",
                    link.link_id
                ),
            )
        })?;
        if link.reference_dataset_id != reference.dataset_id
            || link.target_dataset_id != target.dataset_id
            || link.reference_dataset_id == link.target_dataset_id
            || reference.dataset_role == target.dataset_role
        {
            return Err(error(
                GeneralizationErrorCode::DirectionalLinkContract,
                format!(
                    "directional link {} must connect different reference and target datasets",
                    link.link_id
                ),
            ));
        }
    }
    Ok(())
}

fn refuse_leakage(
    probes: &[LeakageProbe],
    protected_set: ProtectedSet,
    protected_values: &BTreeSet<String>,
    trial_id: &str,
) -> GeneralizationResult<()> {
    let protected_fingerprints = protected_values
        .iter()
        .map(|value| hash_bytes(ascii_trim(value).as_bytes()))
        .collect::<BTreeSet<_>>();
    for probe in probes
        .iter()
        .filter(|probe| probe.protected_set == protected_set)
    {
        let value = ascii_trim(&probe.value);
        if protected_values.contains(value) || protected_fingerprints.contains(value) {
            return Err(error(
                match protected_set {
                    ProtectedSet::HoldoutEntity => GeneralizationErrorCode::EntityDisjointLeak,
                    ProtectedSet::FutureObservation => GeneralizationErrorCode::FutureLeakage,
                },
                format!(
                    "{} leaked protected {:?} value in {} at {}",
                    probe.channel.as_str(),
                    protected_set,
                    trial_id,
                    probe.locator
                ),
            ));
        }
    }
    Ok(())
}

fn strata_for_results(
    results: &[DiscoveryResultRecord],
    observations: &[GeneralizationObservation],
) -> GeneralizationResult<Vec<GeneralizationStratumReport>> {
    let observations_by_id = observations_by_id(observations)?;
    let mut strata = BTreeMap::<GeneralizationStratumKey, GeneralizationStratumReport>::new();
    for result in results {
        for observation_id in &result.observation_ids {
            let observation = observations_by_id.get(observation_id).ok_or_else(|| {
                error(
                    GeneralizationErrorCode::MissingReference,
                    format!(
                        "result {} references missing observation {}",
                        result.result_id, observation_id
                    ),
                )
            })?;
            let key = GeneralizationStratumKey {
                evidence_availability: observation.evidence_availability,
                source_family: observation.source_family,
                name_difficulty: observation.name_difficulty,
                entity_frequency: observation.entity_frequency,
                relation_class: observation.relation_class,
                difficulty_band: observation.difficulty_band,
            };
            let entry = strata
                .entry(key.clone())
                .or_insert_with(|| GeneralizationStratumReport {
                    key,
                    result_count: 0,
                    correct_count: 0,
                    abstain_count: 0,
                    false_merge_count: 0,
                });
            entry.result_count += 1;
            if result.actual_decision == result.expected_decision {
                entry.correct_count += 1;
            }
            if result.actual_decision.is_abstention() {
                entry.abstain_count += 1;
            }
            if result.actual_decision == DiscoveryDecision::FalseMerge {
                entry.false_merge_count += 1;
            }
        }
    }
    Ok(strata.into_values().collect())
}

fn aggregate_reports(
    entity_reports: &[EntityDisjointTrialReport],
    time_reports: &[TimeForwardTrialReport],
) -> GeneralizationAggregate {
    let mut strata = BTreeMap::<GeneralizationStratumKey, GeneralizationStratumReport>::new();
    for report in entity_reports {
        merge_strata(&mut strata, &report.strata);
    }
    for report in time_reports {
        merge_strata(&mut strata, &report.strata);
    }

    let result_count = strata.values().map(|entry| entry.result_count).sum();
    let correct_count = strata.values().map(|entry| entry.correct_count).sum();
    let abstain_count = strata.values().map(|entry| entry.abstain_count).sum();
    let head_result_count = strata
        .values()
        .filter(|entry| entry.key.entity_frequency == EntityFrequency::Head)
        .map(|entry| entry.result_count)
        .sum();
    let tail_result_count = strata
        .values()
        .filter(|entry| entry.key.entity_frequency == EntityFrequency::Tail)
        .map(|entry| entry.result_count)
        .sum();
    let easy_result_count = strata
        .values()
        .filter(|entry| entry.key.difficulty_band == DifficultyBand::Easy)
        .map(|entry| entry.result_count)
        .sum();
    let hard_result_count = strata
        .values()
        .filter(|entry| entry.key.difficulty_band == DifficultyBand::Hard)
        .map(|entry| entry.result_count)
        .sum();

    GeneralizationAggregate {
        entity_disjoint_trial_count: entity_reports.len(),
        time_forward_trial_count: time_reports.len(),
        result_count,
        correct_count,
        abstain_count,
        critical_false_merge_count: entity_reports
            .iter()
            .map(|report| report.critical_false_merge_count)
            .sum::<usize>()
            + time_reports
                .iter()
                .map(|report| report.critical_false_merge_count)
                .sum::<usize>(),
        directional_cross_source_count: entity_reports
            .iter()
            .map(|report| report.directional_cross_source_count)
            .sum::<usize>()
            + time_reports
                .iter()
                .map(|report| report.directional_cross_source_count)
                .sum::<usize>(),
        head_result_count,
        tail_result_count,
        easy_result_count,
        hard_result_count,
        strata: strata.into_values().collect(),
    }
}

fn merge_strata(
    target: &mut BTreeMap<GeneralizationStratumKey, GeneralizationStratumReport>,
    source: &[GeneralizationStratumReport],
) {
    for source_entry in source {
        let entry =
            target
                .entry(source_entry.key.clone())
                .or_insert_with(|| GeneralizationStratumReport {
                    key: source_entry.key.clone(),
                    result_count: 0,
                    correct_count: 0,
                    abstain_count: 0,
                    false_merge_count: 0,
                });
        entry.result_count += source_entry.result_count;
        entry.correct_count += source_entry.correct_count;
        entry.abstain_count += source_entry.abstain_count;
        entry.false_merge_count += source_entry.false_merge_count;
    }
}

fn count_results_by_relation(
    results: &[DiscoveryResultRecord],
    observations: &[GeneralizationObservation],
    relation_class: RelationClass,
) -> GeneralizationResult<usize> {
    let observations_by_id = observations_by_id(observations)?;
    let mut count = 0usize;
    for result in results {
        if result.observation_ids.iter().any(|id| {
            observations_by_id
                .get(id)
                .map(|observation| observation.relation_class == relation_class)
                .unwrap_or(false)
        }) {
            count += 1;
        }
    }
    Ok(count)
}

fn critical_false_merges(controls: &[HardNegativeControl]) -> usize {
    controls
        .iter()
        .filter(|control| control.severity == Severity::Critical && control.false_merge)
        .count()
}

fn observations_by_id(
    observations: &[GeneralizationObservation],
) -> GeneralizationResult<BTreeMap<String, GeneralizationObservation>> {
    let mut by_id = BTreeMap::new();
    for observation in observations {
        if by_id
            .insert(observation.observation_id.clone(), observation.clone())
            .is_some()
        {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("duplicate observation {}", observation.observation_id),
            ));
        }
    }
    Ok(by_id)
}

fn protected_values_for_partition(
    observations: &[GeneralizationObservation],
    partition: BenchmarkPartition,
) -> BTreeSet<String> {
    observations
        .iter()
        .filter(|observation| observation.partition == partition)
        .flat_map(protected_values_for_observation)
        .collect()
}

fn protected_values_for_observation(observation: &GeneralizationObservation) -> Vec<String> {
    vec![
        observation.observation_id.clone(),
        observation.canonical_entity_id.clone(),
        observation.surface.clone(),
    ]
}

fn normalize_string_vec(values: Vec<String>, field: &str) -> GeneralizationResult<Vec<String>> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalize_non_empty(value, field))
        .collect::<GeneralizationResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn normalize_digest(value: String, field: &str) -> GeneralizationResult<String> {
    let value = normalize_non_empty(value, field)?;
    if is_blake3_digest(&value) {
        Ok(value)
    } else {
        Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must be a lowercase blake3 digest"),
        ))
    }
}

fn parse_canonical_temporal_value(
    value: &str,
    field: &str,
) -> GeneralizationResult<TemporalInstant> {
    let value = ascii_trim(value);
    if value.len() == "YYYY-MM-DD".len() && value.as_bytes().get(4) == Some(&b'-') {
        let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
            error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field} must be a canonical YYYY-MM-DD date or RFC3339 timestamp"),
            )
        })?;
        if date.format("%Y-%m-%d").to_string() != value {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field} must be a canonical YYYY-MM-DD date"),
            ));
        }
        let midnight = date.and_hms_opt(0, 0, 0).ok_or_else(|| {
            error(
                GeneralizationErrorCode::ArtifactContract,
                format!("{field} could not be represented as a timestamp"),
            )
        })?;
        return Ok(TemporalInstant {
            seconds: midnight.and_utc().timestamp(),
            nanos: 0,
        });
    }

    let timestamp = DateTime::parse_from_rfc3339(value).map_err(|_| {
        error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must be a canonical YYYY-MM-DD date or RFC3339 timestamp"),
        )
    })?;
    let canonical = timestamp.to_rfc3339_opts(SecondsFormat::AutoSi, true);
    if canonical != value {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must be a canonical RFC3339 timestamp"),
        ));
    }
    Ok(TemporalInstant {
        seconds: timestamp.timestamp(),
        nanos: timestamp.timestamp_subsec_nanos(),
    })
}

fn normalize_non_empty(value: String, field: &str) -> GeneralizationResult<String> {
    let trimmed = ascii_trim(&value).to_string();
    if trimmed.is_empty() {
        Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must not be empty"),
        ))
    } else {
        Ok(trimmed)
    }
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

fn hash_serialized<T: Serialize>(value: &T) -> GeneralizationResult<String> {
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
) -> GeneralizationResult<Vec<T>>
where
    T: Clone + PartialEq,
    K: Ord + fmt::Debug,
{
    let mut deduped = Vec::with_capacity(values.len());
    for value in values {
        if let Some(previous) = deduped.iter().find(|previous| key(previous) == key(&value)) {
            if previous != &value {
                return Err(error(
                    GeneralizationErrorCode::DuplicateRecord,
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

fn artifact_error(error: serde_json::Error) -> GeneralizationError {
    GeneralizationError::new(GeneralizationErrorCode::ArtifactContract, error.to_string())
}

fn error(code: GeneralizationErrorCode, message: impl Into<String>) -> GeneralizationError {
    GeneralizationError::new(code, message)
}

#[cfg(test)]
mod leakage_provenance_tests {
    use super::*;

    fn checked_source(path: &str) -> LoadedGeneralizationCheckedLeakSourceRef {
        LoadedGeneralizationCheckedLeakSourceRef {
            path: path.to_string(),
            format: GeneralizationLeakSourceFormat::Json,
            content_hash: hash_bytes(format!("bytes:{path}").as_bytes()),
            byte_count: 1,
            record_count: 1,
        }
    }

    fn checked_source_with_hash(path: &str, content_hash: &str) -> GeneralizationCheckedLeakSource {
        GeneralizationCheckedLeakSource {
            path: path.to_string(),
            format: GeneralizationLeakSourceFormat::Json,
            content_hash: content_hash.to_string(),
            byte_count: 1,
            record_count: 1,
        }
    }

    fn source(
        source_id: &str,
        channel: LeakChannel,
        source_kind: GeneralizationLeakSourceKind,
        binding_kind: GeneralizationLeakSourceBindingKind,
        binding_hash: &str,
    ) -> GeneralizationStructuredLeakSource {
        GeneralizationStructuredLeakSource {
            source_id: source_id.to_string(),
            phase: GeneralizationLeakSourcePhase::PreEvaluationInfluence,
            channel,
            source_kind,
            binding_kind,
            binding_hash: binding_hash.to_string(),
            coverage: if matches!(channel, LeakChannel::Alias | LeakChannel::Anchor) {
                GeneralizationLeakSourceCoverage::CompleteRegistryTree
            } else {
                GeneralizationLeakSourceCoverage::CompleteSource
            },
            content_hash: hash_bytes(format!("records:{source_id}").as_bytes()),
            content_hash_basis: "canonical_inline_records".to_string(),
            protected_match_derivation: "derive_from_checked_sources".to_string(),
            completeness_manifest: None,
            checked_sources: Vec::new(),
            records: vec![Value::String(source_id.to_string())],
        }
    }

    fn completeness(
        checked: &LoadedGeneralizationCheckedLeakSourceRef,
    ) -> RegistryCompletenessProvenanceSignature {
        RegistryCompletenessProvenanceSignature {
            coverage: GeneralizationLeakSourceCoverage::CompleteRegistryTree,
            root: "registries/test".to_string(),
            entries: BTreeSet::from([GeneralizationLeakSourceCompletenessEntry {
                path: checked.path.clone(),
                format: checked.format,
                content_hash: checked.content_hash.clone(),
                byte_count: checked.byte_count,
                record_count: checked.record_count,
            }]),
        }
    }

    fn run_stage(
        stage: &str,
        path: &str,
        content_hash: &str,
    ) -> crate::entity::run::EntityRunStageArtifact {
        crate::entity::run::EntityRunStageArtifact {
            stage: stage.to_string(),
            version: format!("canon.entity.{stage}.v0"),
            path: path.to_string(),
            artifact_content_hash: content_hash.to_string(),
            upstream_artifacts: Vec::new(),
        }
    }

    fn cache_stage(path: &str, content_hash: &str) -> crate::entity::run::EntityRunStageArtifact {
        run_stage("cache_enabled", path, content_hash)
    }

    fn generated_stage(
        path: &str,
        content_hash: &str,
    ) -> crate::entity::run::EntityRunStageArtifact {
        run_stage("generated_corpus_receipt", path, content_hash)
    }

    fn cache_execution_ref(
        mode: GeneralizationCacheExecutionMode,
        path: &str,
        content_hash: &str,
        bundle_path: &str,
        bundle_hash: &str,
    ) -> GeneralizationCacheExecutionRef {
        GeneralizationCacheExecutionRef {
            version: CANON_GENERALIZATION_CACHE_EXECUTION_VERSION.to_string(),
            mode,
            receipt: GeneralizationTypedArtifactRef {
                path: path.to_string(),
                content_hash: content_hash.to_string(),
                version: CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION.to_string(),
            },
            bundle_receipt: GeneralizationTypedArtifactRef {
                path: bundle_path.to_string(),
                content_hash: bundle_hash.to_string(),
                version: CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION.to_string(),
            },
        }
    }

    fn cache_receipt(
        mode: EntityIndexCacheMode,
        status: EntityIndexCacheStatus,
        reusable: bool,
    ) -> EntityIndexCacheReceipt {
        EntityIndexCacheReceipt {
            version: CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION.to_string(),
            mode,
            status,
            reusable,
            bundle_hash: hash_bytes(b"bundle"),
            files: required_cache_receipt_files()
                .iter()
                .map(
                    |(role, path)| crate::entity::index_io::EntityIndexCacheReceiptFile {
                        role: (*role).to_string(),
                        path: (*path).to_string(),
                        content_hash: hash_bytes(format!("{role}:{path}").as_bytes()),
                        byte_count: 1,
                    },
                )
                .collect(),
        }
    }

    fn cache_bundle_receipt(
        files: &[(&str, &str, &[u8])],
        mode: EntityIndexCacheMode,
        status: EntityIndexCacheStatus,
        reusable: bool,
    ) -> EntityIndexCacheReceipt {
        let mut material = Vec::new();
        let receipt_files = files
            .iter()
            .map(|(role, path, bytes)| {
                material.extend_from_slice(role.as_bytes());
                material.push(0);
                material.extend_from_slice(path.as_bytes());
                material.push(0);
                material.extend_from_slice(bytes.len().to_string().as_bytes());
                material.push(0);
                material.extend_from_slice(bytes);
                material.push(0);
                crate::entity::index_io::EntityIndexCacheReceiptFile {
                    role: (*role).to_string(),
                    path: (*path).to_string(),
                    content_hash: hash_bytes(bytes),
                    byte_count: bytes.len() as u64,
                }
            })
            .collect::<Vec<_>>();
        EntityIndexCacheReceipt {
            version: CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION.to_string(),
            mode,
            status,
            reusable,
            bundle_hash: hash_bytes(&material),
            files: receipt_files,
        }
    }

    fn loaded_checked_source_with_hash(
        path: &str,
        content_hash: &str,
    ) -> LoadedGeneralizationCheckedLeakSourceRef {
        LoadedGeneralizationCheckedLeakSourceRef {
            path: path.to_string(),
            format: GeneralizationLeakSourceFormat::Json,
            content_hash: content_hash.to_string(),
            byte_count: 1,
            record_count: 1,
        }
    }

    fn guard_for_materialized_hash(
        materialized_path: &str,
        materialized_hash: &str,
    ) -> CheckedSourceGuards {
        CheckedSourceGuards {
            prohibited_paths: BTreeSet::from([materialized_path.to_string()]),
            prohibited_hashes: BTreeSet::from([materialized_hash.to_string()]),
        }
    }

    fn write_test_registry() -> PathBuf {
        let temp = tempfile::tempdir().expect("registry tempdir");
        let registry_dir = temp.path().join("registry");
        fs::create_dir_all(&registry_dir).expect("registry dir");
        fs::write(
            registry_dir.join("registry.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "id": "registry",
                "version": "v1",
                "description": "generalization V5 replay test registry",
                "updated": "2026-07-12",
                "entry_count": 1,
                "owner": "test-suite"
            }))
            .expect("registry metadata serializes"),
        )
        .expect("write registry metadata");
        fs::write(
            registry_dir.join("aliases.json"),
            serde_json::to_vec_pretty(&serde_json::json!([
                {
                    "input": "fixture",
                    "canonical_id": "ORG-001",
                    "canonical_type": "firm",
                    "rule_id": "EXACT"
                }
            ]))
            .expect("registry aliases serialize"),
        )
        .expect("write registry aliases");
        std::mem::forget(temp);
        registry_dir
    }

    fn test_registry_snapshot(registry_dir: &Path) -> crate::entity::EntityRegistrySnapshot {
        crate::entity::EntityRegistrySnapshot {
            id: "registry".to_string(),
            version: "v1".to_string(),
            source: registry_dir.display().to_string(),
            lookup_snapshot_hash: hash_registry_json_files_for_replay(registry_dir)
                .expect("registry hash computes"),
            sidecar_snapshot_hash: None,
        }
    }

    fn test_metadata(
        registry_snapshot: crate::entity::EntityRegistrySnapshot,
    ) -> EntityArtifactMetadata {
        EntityArtifactMetadata {
            profile: crate::entity::EntityProfileReference {
                id: "profile".to_string(),
                version: "v1".to_string(),
                entity_type: "firm".to_string(),
                identity_semantics: "test".to_string(),
                canonical_type: "firm".to_string(),
                patch_namespaces: crate::entity::EntityPatchNamespaces {
                    aliases: "profile.aliases".to_string(),
                    distinct: "profile.distinct".to_string(),
                    relations: "profile.relations".to_string(),
                },
                content_hash: Some(hash_bytes(b"profile")),
            },
            strategy: crate::entity::EntityStrategyReference {
                id: "strategy".to_string(),
                version: "v1".to_string(),
                content_hash: hash_bytes(b"strategy"),
            },
            registry_snapshot,
            patch_namespace: "profile.aliases".to_string(),
            input: Some(crate::entity::EntityInputReference {
                row_count: 1,
                content_hash: hash_bytes(b"input"),
            }),
            upstream_artifacts: Vec::new(),
            patch_set: None,
            namekit: None,
            artifact_content_hash: String::new(),
        }
    }

    fn deterministic_summary(counts: &[(&str, u64)]) -> crate::entity::EntityDeterministicSummary {
        crate::entity::EntityDeterministicSummary {
            counts: counts
                .iter()
                .map(|(key, value)| ((*key).to_string(), *value))
                .collect(),
            labels: BTreeMap::from([
                ("profile_id".to_string(), "profile".to_string()),
                ("registry_id".to_string(), "registry".to_string()),
                ("registry_version".to_string(), "v1".to_string()),
            ]),
        }
    }

    fn sealed_index_v1_artifact() -> Value {
        let registry_dir = write_test_registry();
        let mut metadata =
            serde_json::to_value(test_metadata(test_registry_snapshot(&registry_dir)))
                .expect("metadata serializes");
        let contract =
            crate::entity::schema::entity_v1_contract_for_stage(EntityArtifactStageV1::Index)
                .expect("index v1 contract exists");
        metadata["schema"] = serde_json::to_value(
            crate::entity::schema::entity_v1_schema_reference(contract)
                .expect("index v1 schema reference builds"),
        )
        .expect("schema reference serializes");
        metadata["workdir"] = serde_json::to_value(
            crate::entity::schema::entity_v1_workdir_layout(contract, "."),
        )
        .expect("workdir layout serializes");
        metadata["artifact_content_hash"] = Value::String(String::new());
        let mut artifact = serde_json::json!({
            "version": CANON_ENTITY_INDEX_VERSION_V1,
            "artifact_content_hash": "",
            "metadata": metadata,
            "prepare_hash": hash_bytes(b"prepare"),
            "summary": deterministic_summary(&[("index_surfaces", 1)]),
            "postings_path": "index/postings.bin",
            "diagnostics_path": DEFAULT_INDEX_DIAGNOSTICS_PATH,
        });
        crate::entity::schema::finalize_entity_v1_self_hash(&mut artifact)
            .expect("index v1 artifact hashes");
        validate_artifact_v1_core_contract(&artifact).expect("index v1 artifact validates");
        validate_entity_v1_self_hash(&artifact).expect("index v1 self hash validates");
        artifact
    }

    fn native_stage(
        stage: &str,
        version: &str,
        path: &str,
        hash: &str,
        upstream_artifacts: Vec<EntityArtifactReference>,
    ) -> crate::entity::run::EntityRunStageArtifact {
        crate::entity::run::EntityRunStageArtifact {
            stage: stage.to_string(),
            version: version.to_string(),
            path: path.to_string(),
            artifact_content_hash: hash.to_string(),
            upstream_artifacts,
        }
    }

    fn seal_run(mut run: EntityRunArtifact) -> EntityRunArtifact {
        reseal_run_artifact(&mut run).expect("run reseals");
        validate_run_artifact_contract(&run).expect("run validates");
        run
    }

    fn seal_block(mut artifact: BlockCandidateArtifact) -> BlockCandidateArtifact {
        artifact.artifact_content_hash.clear();
        artifact.metadata.artifact_content_hash.clear();
        let content_hash = hash_serialized(&artifact).expect("block hashes");
        artifact.artifact_content_hash = content_hash.clone();
        artifact.metadata.artifact_content_hash = content_hash;
        validate_block_candidate_artifact_contract(&artifact).expect("block validates");
        artifact
    }

    fn seal_edge(mut artifact: EdgeEvidenceArtifact) -> EdgeEvidenceArtifact {
        artifact.artifact_content_hash.clear();
        artifact.metadata.artifact_content_hash.clear();
        let content_hash = hash_serialized(&artifact).expect("edge hashes");
        artifact.artifact_content_hash = content_hash.clone();
        artifact.metadata.artifact_content_hash = content_hash;
        validate_edge_evidence_artifact_contract(&artifact).expect("edge validates");
        artifact
    }

    fn seal_solve(mut artifact: SolveArtifact) -> SolveArtifact {
        artifact.artifact_content_hash.clear();
        artifact.metadata.artifact_content_hash.clear();
        let content_hash = hash_serialized(&artifact).expect("solve hashes");
        artifact.artifact_content_hash = content_hash.clone();
        artifact.metadata.artifact_content_hash = content_hash;
        validate_solve_artifact_contract(&artifact).expect("solve validates");
        artifact
    }

    fn baseline_run() -> EntityRunArtifact {
        let registry_dir = write_test_registry();
        let metadata = test_metadata(test_registry_snapshot(&registry_dir));
        let solve_strategy = derived_stage_strategy(&metadata.strategy, "solve");
        let registry_snapshot_hash = metadata.registry_snapshot.lookup_snapshot_hash.clone();
        let prepare_hash = hash_bytes(b"prepare");
        let index_hash = hash_bytes(b"index");
        let block_hash = hash_bytes(b"old-block");
        let evidence_hash = hash_bytes(b"old-evidence");
        let solve_hash = hash_bytes(b"old-solve");
        let stages = vec![
            native_stage(
                "prepare",
                CANON_ENTITY_PREPARE_VERSION_V1,
                "prepare/prepare.json",
                &prepare_hash,
                Vec::new(),
            ),
            native_stage(
                "index",
                CANON_ENTITY_INDEX_VERSION_V1,
                "index/index.json",
                &index_hash,
                vec![artifact_ref(CANON_ENTITY_PREPARE_VERSION_V1, &prepare_hash)],
            ),
            native_stage(
                "block",
                CANON_ENTITY_BLOCK_VERSION_V1,
                "block/block.json",
                &block_hash,
                vec![
                    artifact_ref(CANON_ENTITY_PREPARE_VERSION_V1, &prepare_hash),
                    artifact_ref(CANON_ENTITY_INDEX_VERSION_V1, &index_hash),
                ],
            ),
            native_stage(
                "evidence",
                CANON_ENTITY_EVIDENCE_VERSION_V1,
                "evidence/evidence.json",
                &evidence_hash,
                vec![artifact_ref(CANON_ENTITY_BLOCK_VERSION_V1, &block_hash)],
            ),
            native_stage(
                "solve",
                CANON_ENTITY_SOLVE_VERSION_V1,
                "solve/solve.json",
                &solve_hash,
                vec![
                    artifact_ref(CANON_ENTITY_BLOCK_VERSION_V1, &block_hash),
                    artifact_ref(CANON_ENTITY_EVIDENCE_VERSION_V1, &evidence_hash),
                ],
            ),
        ];
        let mut run_metadata = metadata;
        run_metadata.upstream_artifacts = stages.iter().map(stage_artifact_ref).collect();
        run_metadata
            .upstream_artifacts
            .sort_by(entity_artifact_ref_cmp);
        seal_run(EntityRunArtifact {
            version: CANON_ENTITY_RUN_VERSION_V1.to_string(),
            artifact_content_hash: String::new(),
            metadata: run_metadata,
            summary: crate::entity::EntityDeterministicSummary {
                counts: BTreeMap::from([
                    ("row_count".to_string(), 1),
                    ("prepared_surfaces".to_string(), 1),
                    ("physical_batch_count".to_string(), 1),
                    ("max_physical_batch_rows".to_string(), 1),
                    ("exact_resolved_surfaces".to_string(), 0),
                    ("index_surfaces".to_string(), 1),
                    ("exact_bucket_count".to_string(), 1),
                    ("candidate_pairs".to_string(), 2),
                    ("evidence_records".to_string(), 3),
                    ("relation_hint_evidence".to_string(), 4),
                    ("solved_entities".to_string(), 5),
                    ("review_group_count".to_string(), 6),
                ]),
                labels: BTreeMap::from([
                    ("profile_id".to_string(), "profile".to_string()),
                    ("registry_id".to_string(), "registry".to_string()),
                    ("registry_version".to_string(), "v1".to_string()),
                ]),
            },
            stage_artifacts: stages.clone(),
            work_dir: crate::entity::run::EntityRunWorkDirLayout {
                prepare_artifact_path: "prepare/prepare.json".to_string(),
                surfaces_path: "prepare/surfaces.jsonl".to_string(),
                index_artifact_path: "index/index.json".to_string(),
                block_artifact_path: "block/block.json".to_string(),
                candidate_records_path: "block/candidates.jsonl".to_string(),
                candidate_diagnostics_path: "block/diagnostics.json".to_string(),
                exact_bucket_assertions_path: "block/exact_buckets.jsonl".to_string(),
                edge_artifact_path: "evidence/evidence.json".to_string(),
                edge_records_path: "evidence/evidence.jsonl".to_string(),
                solve_artifact_path: "solve/solve.json".to_string(),
                decision_ledger_path: "solve/decision_ledger.jsonl".to_string(),
                run_artifact_path: "run/run.json".to_string(),
            },
            next_commands: crate::entity::run::EntityRunNextCommands {
                resume: "resume".to_string(),
                review_export: "review_export".to_string(),
                audit: "audit".to_string(),
                promote: "promote".to_string(),
                apply: "apply".to_string(),
            },
            orchestration: crate::entity::run::EntityRunOrchestration {
                stage_order: vec![
                    "prepare".to_string(),
                    "index".to_string(),
                    "block".to_string(),
                    "evidence".to_string(),
                    "solve".to_string(),
                    "audit".to_string(),
                ],
                profile_firewall: crate::entity::run::EntityRunProfileFirewall {
                    profile_id: "profile".to_string(),
                    profile_version: "v1".to_string(),
                    identity_semantics: "test".to_string(),
                    canonical_type: "firm".to_string(),
                    registry_id: "registry".to_string(),
                    registry_version: "v1".to_string(),
                    registry_snapshot_hash,
                    sidecar_snapshot_hash: None,
                    strategy_hash: solve_strategy.content_hash,
                },
                handoff_steps: vec![
                    crate::entity::run::EntityRunHandoffStep {
                        stage: "audit".to_string(),
                        input_artifacts: stages.iter().map(stage_artifact_ref).collect(),
                        ..crate::entity::run::EntityRunHandoffStep::default()
                    },
                    crate::entity::run::EntityRunHandoffStep {
                        stage: "review_export".to_string(),
                        input_artifacts: vec![artifact_ref(
                            CANON_ENTITY_SOLVE_VERSION_V1,
                            &solve_hash,
                        )],
                        ..crate::entity::run::EntityRunHandoffStep::default()
                    },
                ],
            },
        })
    }

    fn run_with_cache_execution_stage(mut run: EntityRunArtifact) -> EntityRunArtifact {
        let index_ref = run
            .stage_artifacts
            .iter()
            .find(|stage| stage.stage == "index")
            .map(stage_artifact_ref)
            .expect("index stage exists");
        let cache_hash = hash_bytes(b"cache-receipt");
        let bundle_hash = hash_bytes(b"cache-bundle-receipt");
        let mut cache_upstreams = vec![
            index_ref,
            artifact_ref(CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION, &bundle_hash),
        ];
        cache_upstreams.sort_by(entity_artifact_ref_cmp);
        let cache_stage = native_stage(
            "cache_enabled",
            CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION,
            RUN_CACHE_EXECUTION_RECEIPT_PATH,
            &cache_hash,
            cache_upstreams,
        );
        run.stage_artifacts.insert(2, cache_stage);
        if !run
            .orchestration
            .stage_order
            .iter()
            .any(|stage| stage == "cache_enabled")
        {
            run.orchestration
                .stage_order
                .insert(2, "cache_enabled".to_string());
        }
        run.summary
            .labels
            .insert("cache_mode".to_string(), "enabled".to_string());
        run.summary
            .labels
            .insert("cache_status".to_string(), "hit".to_string());
        run.summary.labels.insert(
            "cache_receipt_path".to_string(),
            RUN_CACHE_EXECUTION_RECEIPT_PATH.to_string(),
        );
        run.summary
            .labels
            .insert("cache_receipt_hash".to_string(), cache_hash);
        run.summary.labels.insert(
            "cache_bundle_receipt_path".to_string(),
            INDEX_CACHE_RECEIPT_FILE.to_string(),
        );
        run.summary
            .labels
            .insert("cache_bundle_receipt_hash".to_string(), bundle_hash);
        refresh_audit_handoff_refs_to_final_stage_refs(&mut run);
        seal_run(run)
    }

    fn run_with_prepare_counts(
        mut run: EntityRunArtifact,
        prepared_surfaces: u64,
        row_count: u64,
        exact_resolved_surfaces: u64,
    ) -> EntityRunArtifact {
        run.summary
            .counts
            .insert("prepared_surfaces".to_string(), prepared_surfaces);
        run.summary
            .counts
            .insert("row_count".to_string(), row_count);
        run.summary.counts.insert(
            "exact_resolved_surfaces".to_string(),
            exact_resolved_surfaces,
        );
        if let Some(input) = &mut run.metadata.input {
            input.row_count = row_count;
        }
        seal_run(run)
    }

    struct ReplacementArtifacts {
        registry_dir: PathBuf,
        block: BlockCandidateArtifact,
        edge: EdgeEvidenceArtifact,
        block_candidate_records: Vec<BlockCandidateRecord>,
        block_diagnostics: BlockCandidateGenerationDiagnostics,
        exact_buckets: Vec<ExactBucketAssertion>,
        edge_records: Vec<EdgeEvidenceRecord>,
        prepared_surfaces: Vec<PreparedSurfaceRecord>,
        solve_config: SolveReconciliationConfig,
    }

    fn prepare_registry_snapshot(
        run: &EntityRunArtifact,
    ) -> crate::entity::prepare::PrepareRegistrySnapshot {
        crate::entity::prepare::PrepareRegistrySnapshot {
            id: run.metadata.registry_snapshot.id.clone(),
            version: run.metadata.registry_snapshot.version.clone(),
            source: run.metadata.registry_snapshot.source.clone(),
            lookup_snapshot_hash: run.metadata.registry_snapshot.lookup_snapshot_hash.clone(),
        }
    }

    fn unresolved_exact_lookup(
        run: &EntityRunArtifact,
    ) -> crate::entity::prepare::PreparedExactLookup {
        crate::entity::prepare::PreparedExactLookup {
            status: PreparedExactLookupStatus::Unresolved,
            canonical_id: None,
            canonical_type: None,
            rule_id: None,
            matched_input: None,
            lookup_inputs: vec!["unresolved-fixture".to_string()],
            registry_snapshot: Some(prepare_registry_snapshot(run)),
        }
    }

    fn resolved_exact_lookup(
        run: &EntityRunArtifact,
    ) -> crate::entity::prepare::PreparedExactLookup {
        crate::entity::prepare::PreparedExactLookup {
            status: PreparedExactLookupStatus::Resolved,
            canonical_id: Some("ORG-001".to_string()),
            canonical_type: Some("firm".to_string()),
            rule_id: Some("EXACT".to_string()),
            matched_input: Some("fixture".to_string()),
            lookup_inputs: vec!["fixture".to_string()],
            registry_snapshot: Some(prepare_registry_snapshot(run)),
        }
    }

    fn prepared_surface(
        run: &EntityRunArtifact,
        surface_key: &str,
        exact_lookup: crate::entity::prepare::PreparedExactLookup,
    ) -> PreparedSurfaceRecord {
        let raw = exact_lookup
            .lookup_inputs
            .first()
            .cloned()
            .unwrap_or_else(|| surface_key.to_string());
        let view_name = prepared_surface_id_view_name(&run.metadata.profile.id);
        let normalized_views = BTreeMap::from([(
            view_name.to_string(),
            crate::entity::prepare::PreparedNormalizedView {
                value: raw.clone(),
                reason_codes: vec!["test_fixture".to_string()],
            },
        )]);
        let material = SurfaceIdMaterial::new(
            run.metadata.profile.id.clone(),
            view_name.to_string(),
            raw.clone(),
            [raw.clone()],
        );
        let surface_id = derive_surface_ids(&[material])
            .expect("test surface id derives")
            .pop()
            .expect("one derived test surface id")
            .surface_id;
        PreparedSurfaceRecord {
            surface_id,
            profile_id: run.metadata.profile.id.clone(),
            surface_key: format!("{}:{surface_key}", run.metadata.profile.id),
            primary_surface: raw.clone(),
            normalized_views,
            exact_lookup,
            raw_variants: vec![raw],
            alias_surfaces: Vec::new(),
            mention_surfaces: Vec::new(),
            row_count: 1,
            deal_count: 1,
            provenance_samples: Vec::new(),
        }
    }

    fn block_diagnostics(candidate_count: u64) -> BlockCandidateGenerationDiagnostics {
        BlockCandidateGenerationDiagnostics {
            candidate_record_count: candidate_count,
            candidate_pairs_emitted: candidate_count,
            candidate_pairs_suppressed_by_cap: 0,
            suppressed_candidate_count: 0,
            large_buckets_suppressed: 0,
            candidate_pairs_per_surface_p50: candidate_count,
            candidate_pairs_per_surface_p95: candidate_count,
            candidate_pairs_per_surface_p99: candidate_count,
            max_candidates_for_surface: candidate_count,
            max_candidates_for_operator: candidate_count,
            configured_budget: crate::entity::block::BlockCandidateBudgetConfig::new(8, 64, 128),
            candidate_budget: crate::entity::edge::EdgeCandidateBudgetProof::within_run_budget(
                candidate_count,
                128,
            ),
            candidate_artifact_bytes: 0,
            partial_candidate_artifact_written: false,
            operator_yield: Vec::new(),
            operator_diagnostics: Vec::new(),
        }
    }

    fn replacement_artifacts(run: &EntityRunArtifact) -> ReplacementArtifacts {
        let registry_dir = PathBuf::from(&run.metadata.registry_snapshot.source);
        let prepare_ref = single_stage_ref(run, "prepare").expect("prepare ref");
        let index_ref = single_stage_ref(run, "index").expect("index ref");
        let block_candidate_records = Vec::new();
        let block_diagnostics = block_diagnostics(0);
        let exact_buckets = Vec::new();
        let edge_records = Vec::new();
        let block_strategy = derived_stage_strategy(&run.metadata.strategy, "block");
        let edge_strategy = derived_stage_strategy(&run.metadata.strategy, "evidence");
        let mut block_metadata = run.metadata.clone();
        block_metadata.strategy = block_strategy;
        block_metadata.upstream_artifacts = vec![prepare_ref, index_ref];
        block_metadata.artifact_content_hash.clear();
        let block = seal_block(BlockCandidateArtifact {
            version: CANON_ENTITY_BLOCK_VERSION_V1.to_string(),
            artifact_content_hash: String::new(),
            metadata: block_metadata.clone(),
            summary: deterministic_summary(&[("candidate_pairs", 0), ("exact_bucket_count", 0)]),
            upstream_artifacts: block_metadata.upstream_artifacts.clone(),
            candidate_records_path: run.work_dir.candidate_records_path.clone(),
            candidate_records_hash: hash_bytes(b""),
            candidate_diagnostics_path: run.work_dir.candidate_diagnostics_path.clone(),
            candidate_diagnostics_hash: hash_serialized(&block_diagnostics)
                .expect("diagnostics hash"),
            bucket_assertions_hash: hash_bytes(b""),
        });

        let edge = build_edge_evidence_artifact_contract(EdgeEvidenceArtifactRequest {
            block: block.clone(),
            strategy: edge_strategy,
            edge_records_path: run.work_dir.edge_records_path.clone(),
            edge_records: edge_records.clone(),
            candidate_records: block_candidate_records.clone(),
            bucket_assertions: exact_buckets.clone(),
        })
        .expect("edge artifact builds");

        ReplacementArtifacts {
            registry_dir,
            block,
            edge,
            block_candidate_records,
            block_diagnostics,
            exact_buckets,
            edge_records,
            prepared_surfaces: vec![prepared_surface(
                run,
                "surface.unresolved",
                unresolved_exact_lookup(run),
            )],
            solve_config: SolveReconciliationConfig::escrow_only(
                crate::entity::score::ScoreUnits::ZERO,
            ),
        }
    }

    fn rebuild_fixture_edge(fixture: &mut ReplacementArtifacts, run: &EntityRunArtifact) {
        fixture.edge = build_edge_evidence_artifact_contract(EdgeEvidenceArtifactRequest {
            block: fixture.block.clone(),
            strategy: derived_stage_strategy(&run.metadata.strategy, "evidence"),
            edge_records_path: run.work_dir.edge_records_path.clone(),
            edge_records: fixture.edge_records.clone(),
            candidate_records: fixture.block_candidate_records.clone(),
            bucket_assertions: fixture.exact_buckets.clone(),
        })
        .expect("edge artifact rebuilds");
    }

    fn rebind_request<'a>(
        run: &'a EntityRunArtifact,
        fixture: &'a ReplacementArtifacts,
    ) -> GeneralizationNativeStageRebindRequest<'a> {
        GeneralizationNativeStageRebindRequest {
            run,
            registry_dir: &fixture.registry_dir,
            block: &fixture.block,
            block_candidate_records: &fixture.block_candidate_records,
            block_diagnostics: &fixture.block_diagnostics,
            exact_buckets: &fixture.exact_buckets,
            edge: &fixture.edge,
            edge_records: &fixture.edge_records,
            prepared_surfaces: &fixture.prepared_surfaces,
            solve_config: fixture.solve_config,
        }
    }

    fn typed_ref(path: &str, version: &str, content_hash: &str) -> GeneralizationTypedArtifactRef {
        GeneralizationTypedArtifactRef {
            path: path.to_string(),
            content_hash: content_hash.to_string(),
            version: version.to_string(),
        }
    }

    fn solve_derivation_refs() -> GeneralizationSolveDerivationRefs {
        GeneralizationSolveDerivationRefs {
            edge_artifact: typed_ref(
                "evidence/evidence.json",
                CANON_ENTITY_EVIDENCE_VERSION_V1,
                &hash_bytes(b"edge"),
            ),
            edge_records: typed_ref(
                "evidence/evidence.jsonl",
                CANON_ENTITY_EVIDENCE_VERSION_V1,
                &hash_bytes(b"edges"),
            ),
            prepared_surfaces: typed_ref(
                "prepare/surfaces.jsonl",
                CANON_ENTITY_PREPARE_VERSION_V1,
                &hash_bytes(b"surfaces"),
            ),
            solve_policy: typed_ref(
                "solve/policy.json",
                CANON_GENERALIZATION_SOLVE_POLICY_VERSION,
                &hash_bytes(b"policy"),
            ),
        }
    }

    fn solve_derivation_refs_for_run_ref(
        run: &EntityRunArtifact,
        run_ref_path: &str,
    ) -> GeneralizationSolveDerivationRefs {
        GeneralizationSolveDerivationRefs {
            edge_artifact: typed_ref(
                &sibling_manifest_path(run_ref_path, &run.work_dir.edge_artifact_path)
                    .expect("edge path derives"),
                CANON_ENTITY_EVIDENCE_VERSION_V1,
                &hash_bytes(b"edge"),
            ),
            edge_records: typed_ref(
                &sibling_manifest_path(run_ref_path, &run.work_dir.edge_records_path)
                    .expect("edge records path derives"),
                CANON_ENTITY_EVIDENCE_VERSION_V1,
                &hash_bytes(b"edges"),
            ),
            prepared_surfaces: typed_ref(
                &sibling_manifest_path(run_ref_path, &run.work_dir.surfaces_path)
                    .expect("prepared surfaces path derives"),
                CANON_ENTITY_PREPARE_VERSION_V1,
                &hash_bytes(b"surfaces"),
            ),
            solve_policy: typed_ref(
                "trials/trial/run/solve/policy.json",
                CANON_GENERALIZATION_SOLVE_POLICY_VERSION,
                &hash_bytes(b"policy"),
            ),
        }
    }

    fn loaded_candidate_recall_for_rebuild(
        fixture: &ReplacementArtifacts,
    ) -> LoadedGeneralizationCandidateRecall {
        let report = evaluate_candidate_recall(CandidateRecallEvaluationRequest {
            candidate_records: &fixture.block_candidate_records,
            diagnostics: &fixture.block_diagnostics,
            gold_pairs: &[],
            surface_ids: &[],
            exact_bucket_count: fixture.exact_buckets.len() as u64,
        });
        LoadedGeneralizationCandidateRecall {
            references: GeneralizationCandidateRecallExecutionRefs {
                quality_manifest: typed_ref(
                    "candidate_recall/quality.json",
                    CANON_GENERALIZATION_CANDIDATE_RECALL_QUALITY_MANIFEST_VERSION,
                    &hash_bytes(b"quality"),
                ),
                block_artifact: typed_ref(
                    "block/block.json",
                    CANON_ENTITY_BLOCK_VERSION_V1,
                    &hash_bytes(b"block"),
                ),
                candidates: typed_ref(
                    "block/candidates.jsonl",
                    CANON_ENTITY_BLOCK_VERSION_V1,
                    &hash_bytes(b"candidates"),
                ),
                diagnostics: typed_ref(
                    "block/diagnostics.json",
                    CANON_ENTITY_BLOCK_VERSION_V1,
                    &hash_bytes(b"diagnostics"),
                ),
                exact_bucket_assertions: typed_ref(
                    "block/exact_buckets.jsonl",
                    CANON_ENTITY_BLOCK_BUCKET_VERSION,
                    &hash_bytes(b"exact-buckets"),
                ),
                report: typed_ref(
                    "candidate_recall/report.json",
                    CANON_ENTITY_CANDIDATE_RECALL_VERSION,
                    &hash_bytes(b"report"),
                ),
                exact_bucket_count: fixture.exact_buckets.len() as u64,
            },
            quality_manifest_hash: hash_bytes(b"quality"),
            block_artifact_hash: fixture.block.artifact_content_hash.clone(),
            candidate_records_hash: hash_serialized(&fixture.block_candidate_records)
                .expect("candidate records hash"),
            diagnostics_hash: hash_serialized(&fixture.block_diagnostics)
                .expect("diagnostics hash"),
            exact_bucket_assertions_hash: hash_serialized(&fixture.exact_buckets)
                .expect("exact buckets hash"),
            report_hash: hash_serialized(&report).expect("report hash"),
            surface_ids: Vec::new(),
            gold_pairs: Vec::new(),
            block_artifact: fixture.block.clone(),
            candidate_records: fixture.block_candidate_records.clone(),
            diagnostics: fixture.block_diagnostics.clone(),
            exact_bucket_assertions: fixture.exact_buckets.clone(),
            report,
        }
    }

    fn loaded_solve_derivation_for_rebuild(
        fixture: &ReplacementArtifacts,
    ) -> LoadedGeneralizationSolveDerivation {
        LoadedGeneralizationSolveDerivation {
            references: solve_derivation_refs(),
            edge_artifact_hash: fixture.edge.artifact_content_hash.clone(),
            edge_records_hash: hash_serialized(&fixture.edge_records).expect("edge records hash"),
            prepared_surfaces_hash: hash_serialized(&fixture.prepared_surfaces)
                .expect("prepared surfaces hash"),
            solve_policy_hash: hash_serialized(&fixture.solve_config).expect("solve policy hash"),
            edge_artifact: fixture.edge.clone(),
            edge_records: fixture.edge_records.clone(),
            prepared_surfaces: fixture.prepared_surfaces.clone(),
            solve_config: fixture.solve_config,
        }
    }

    fn leak_bundle_ref() -> GeneralizationLeakSourceBundleRef {
        GeneralizationLeakSourceBundleRef {
            version: CANON_GENERALIZATION_LEAK_SCAN_SOURCES_VERSION.to_string(),
            phase: GeneralizationLeakSourcePhase::PreEvaluationInfluence,
            channels: vec![
                LeakChannel::Alias,
                LeakChannel::Anchor,
                LeakChannel::Threshold,
                LeakChannel::Dictionary,
                LeakChannel::Patch,
                LeakChannel::Cache,
                LeakChannel::GeneratedCorpus,
            ],
            path: "artifacts/leak_scan_sources.json".to_string(),
            content_hash: hash_bytes(b"leak-bundle"),
        }
    }

    fn quality_observation(
        observation_id: &str,
        canonical_entity_id: &str,
        partition: BenchmarkPartition,
    ) -> GeneralizationObservation {
        GeneralizationObservation {
            observation_id: observation_id.to_string(),
            canonical_entity_id: canonical_entity_id.to_string(),
            dataset_id: "dataset.reference".to_string(),
            dataset_role: DatasetRole::SingleSource,
            partition,
            observed_at: "2026-01-01".to_string(),
            surface: format!("surface {observation_id}"),
            evidence_availability: EvidenceAvailability::NameOnly,
            source_family: SourceFamily::PublicFixture,
            name_difficulty: NameDifficulty::Easy,
            entity_frequency: EntityFrequency::Tail,
            relation_class: RelationClass::NewEntity,
            difficulty_band: DifficultyBand::Easy,
        }
    }

    fn quality_benchmark(
        discovery_results: Vec<DiscoveryResultRecord>,
        hard_negatives: Vec<HardNegativeControl>,
    ) -> GeneralizationBenchmark {
        GeneralizationBenchmark {
            version: CANON_GENERALIZATION_VERSION.to_string(),
            benchmark_id: "quality.benchmark".to_string(),
            corpus_visibility: CorpusVisibility::PublicFixture,
            corpus_ref: "public-fixture".to_string(),
            policy_digest: hash_bytes(b"policy"),
            entity_disjoint_trials: vec![EntityDisjointTrial {
                trial_id: "entity.quality".to_string(),
                observations: vec![
                    quality_observation("obs.a", "ORG-A", BenchmarkPartition::Holdout),
                    quality_observation("obs.b", "ORG-A", BenchmarkPartition::Holdout),
                    quality_observation("obs.c", "ORG-C", BenchmarkPartition::Holdout),
                ],
                discovery_results,
                hard_negatives,
                directional_links: Vec::new(),
                leakage_probes: Vec::new(),
            }],
            time_forward_trials: Vec::new(),
        }
    }

    fn quality_gate<'a>(
        report: &'a GeneralizationReport,
        gate_id: &str,
    ) -> &'a GeneralizationQualityGateReport {
        report
            .quality
            .gates
            .iter()
            .find(|gate| gate.gate_id == gate_id)
            .expect("quality gate exists")
    }

    #[test]
    fn low_quality_benchmark_emits_blocking_quality_report() {
        let benchmark = quality_benchmark(
            vec![DiscoveryResultRecord {
                result_id: "result.low".to_string(),
                observation_ids: vec!["obs.a".to_string(), "obs.b".to_string()],
                expected_decision: DiscoveryDecision::ClusterNew,
                actual_decision: DiscoveryDecision::Abstain,
                candidate_rank: None,
                evidence_lanes: Vec::new(),
                review_action: ReviewAction::DeferReview,
            }],
            Vec::new(),
        );

        let report = compile_generalization_benchmark_internal(benchmark)
            .expect("low-quality structurally valid report still emits");

        assert_eq!(
            report.quality.version,
            CANON_GENERALIZATION_QUALITY_GATE_REPORT_VERSION
        );
        assert_eq!(
            report.quality.contract_version,
            CANON_ENTITY_QUALITY_VERSION
        );
        assert_eq!(
            report.quality.release_claim_status,
            GeneralizationReleaseClaimStatus::Blocked
        );
        let candidate_gate = quality_gate(&report, "candidate_recall_at_50_min");
        assert_eq!(candidate_gate.status, GeneralizationQualityGateStatus::Fail);
        assert_eq!(candidate_gate.observed_value, Some(0.0));
        assert_eq!(
            candidate_gate.threshold,
            QUALITY_GATE_CANDIDATE_RECALL_AT_50_MIN
        );
        let recall_gate = quality_gate(&report, "auto_link_recall_min");
        assert_eq!(recall_gate.status, GeneralizationQualityGateStatus::Fail);
        assert_eq!(recall_gate.observed_value, Some(0.0));

        let mut mutated = report.clone();
        let original_digest = mutated.report_digest.clone();
        mutated.quality.gates[0].status = GeneralizationQualityGateStatus::Pass;
        let mutated_digest =
            generalization_report_digest(&mutated).expect("mutated report digest computes");
        assert_ne!(original_digest, mutated_digest);
    }

    #[test]
    fn critical_false_merge_is_quality_gate_failure_not_refusal() {
        let benchmark = quality_benchmark(
            vec![DiscoveryResultRecord {
                result_id: "result.good".to_string(),
                observation_ids: vec!["obs.a".to_string(), "obs.b".to_string()],
                expected_decision: DiscoveryDecision::ClusterNew,
                actual_decision: DiscoveryDecision::ClusterNew,
                candidate_rank: Some(1),
                evidence_lanes: Vec::new(),
                review_action: ReviewAction::PromoteCluster,
            }],
            vec![HardNegativeControl {
                control_id: "hard.critical".to_string(),
                left_observation_id: "obs.a".to_string(),
                right_observation_id: "obs.c".to_string(),
                relation_class: RelationClass::Hierarchy,
                severity: Severity::Critical,
                false_merge: true,
            }],
        );

        let report = compile_generalization_benchmark(benchmark)
            .expect("critical false merge emits a failure report");

        assert_eq!(report.aggregate.critical_false_merge_count, 1);
        assert_eq!(
            report.quality.release_claim_status,
            GeneralizationReleaseClaimStatus::Blocked
        );
        let gate = quality_gate(&report, "critical_false_merges_max");
        assert_eq!(gate.status, GeneralizationQualityGateStatus::Fail);
        assert_eq!(gate.observed_value, Some(1.0));
        assert_eq!(gate.threshold, QUALITY_GATE_CRITICAL_FALSE_MERGES_MAX);
    }

    #[test]
    fn zero_denominator_quality_gates_are_not_applicable() {
        let benchmark = quality_benchmark(Vec::new(), Vec::new());

        let report = compile_generalization_benchmark(benchmark)
            .expect("empty-case structurally valid report emits");

        assert_eq!(
            report.quality.release_claim_status,
            GeneralizationReleaseClaimStatus::Blocked
        );
        for gate_id in [
            "candidate_recall_at_50_min",
            "auto_link_precision_min",
            "auto_link_recall_min",
            "accounted_case_rate_min",
        ] {
            let gate = quality_gate(&report, gate_id);
            assert_eq!(gate.status, GeneralizationQualityGateStatus::NotApplicable);
            assert_eq!(gate.observed_value, None);
        }
        assert_eq!(
            quality_gate(&report, "critical_false_merges_max").status,
            GeneralizationQualityGateStatus::Pass
        );
    }

    #[test]
    fn rebind_native_stages_replaces_chain_and_reseals_deterministically() {
        let run = baseline_run();
        let original_work_dir = run.work_dir.clone();
        let original_next_commands = run.next_commands.clone();
        let original_prepare = run.stage_artifacts[0].clone();
        let original_index = run.stage_artifacts[1].clone();
        let fixture = replacement_artifacts(&run);

        let result = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect("native stages rebind");
        let second = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect("native stages rebind deterministically");
        let rebound = result.run;
        let solve = result.solve;

        assert_eq!(
            rebound.artifact_content_hash,
            second.run.artifact_content_hash
        );
        assert_eq!(
            solve.artifact_content_hash,
            second.solve.artifact_content_hash
        );
        assert_eq!(rebound.work_dir, original_work_dir);
        assert_eq!(rebound.next_commands, original_next_commands);
        assert_eq!(rebound.stage_artifacts[0], original_prepare);
        assert_eq!(rebound.stage_artifacts[1], original_index);
        assert_eq!(rebound.summary.counts.get("candidate_pairs"), Some(&0));
        assert_eq!(rebound.summary.counts.get("exact_bucket_count"), Some(&0));
        assert_eq!(rebound.summary.counts.get("evidence_records"), Some(&0));
        assert_eq!(
            rebound.summary.counts.get("relation_hint_evidence"),
            Some(&0)
        );
        assert_eq!(rebound.summary.counts.get("solved_entities"), Some(&0));
        assert_eq!(rebound.summary.counts.get("review_group_count"), Some(&0));
        let mut expected_upstreams = rebound
            .stage_artifacts
            .iter()
            .map(stage_artifact_ref)
            .collect::<Vec<_>>();
        expected_upstreams.sort_by(entity_artifact_ref_cmp);
        assert_eq!(rebound.metadata.upstream_artifacts, expected_upstreams);
        assert_eq!(
            rebound.orchestration.handoff_steps[0].input_artifacts,
            rebound
                .stage_artifacts
                .iter()
                .map(stage_artifact_ref)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            rebound.orchestration.handoff_steps[1].input_artifacts,
            vec![artifact_ref(&solve.version, &solve.artifact_content_hash)]
        );
    }

    #[test]
    fn strict_loader_rebuild_requires_loaded_run_and_solve_match_derivation() {
        let run = baseline_run();
        let fixture = replacement_artifacts(&run);
        let rebound = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect("native stages rebind");
        let candidate_recall = loaded_candidate_recall_for_rebuild(&fixture);
        let solve_derivation = loaded_solve_derivation_for_rebuild(&fixture);

        validate_loaded_run_solve_rebuild(
            &rebound.run,
            &fixture.registry_dir,
            &rebound.solve,
            &candidate_recall,
            &solve_derivation,
        )
        .expect("loaded run and solve match strict rebuild");

        let mut wrong_solve = rebound.solve.clone();
        wrong_solve.decision_ledger_path = "solve/other_decision_ledger.jsonl".to_string();
        let refusal = validate_loaded_run_solve_rebuild(
            &rebound.run,
            &fixture.registry_dir,
            &wrong_solve,
            &candidate_recall,
            &solve_derivation,
        )
        .expect_err("mutated solve refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let mut wrong_run = rebound.run.clone();
        wrong_run
            .summary
            .labels
            .insert("trial_id".to_string(), "other-trial".to_string());
        let refusal = validate_loaded_run_solve_rebuild(
            &wrong_run,
            &fixture.registry_dir,
            &rebound.solve,
            &candidate_recall,
            &solve_derivation,
        )
        .expect_err("mutated run refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn solve_derivation_refs_are_required_and_versioned() {
        validate_solve_derivation_refs(&solve_derivation_refs())
            .expect("solve derivation refs validate");

        let mut refs = solve_derivation_refs();
        refs.solve_policy.version = "unversioned".to_string();
        let refusal =
            validate_solve_derivation_refs(&refs).expect_err("wrong solve policy version refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let mut refs = solve_derivation_refs();
        refs.edge_records.version = crate::entity::CANON_ENTITY_BLOCK_VERSION.to_string();
        let refusal =
            validate_solve_derivation_refs(&refs).expect_err("wrong edge record version refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn trial_execution_requires_registry_dir_field() {
        let ref_json = |path: &str, version: &str| {
            serde_json::json!({
                "path": path,
                "content_hash": hash_bytes(path.as_bytes()),
                "version": version
            })
        };
        let mut value = serde_json::json!({
            "trial_id": "trial",
            "family": "entity_disjoint",
            "registry_dir": "registry",
            "candidate_recall": {
                "quality_manifest": ref_json("candidate/quality.json", CANON_GENERALIZATION_CANDIDATE_RECALL_QUALITY_MANIFEST_VERSION),
                "block_artifact": ref_json("candidate/block.json", CANON_ENTITY_BLOCK_VERSION_V1),
                "candidates": ref_json("candidate/candidates.jsonl", CANON_ENTITY_BLOCK_VERSION_V1),
                "diagnostics": ref_json("candidate/diagnostics.json", CANON_ENTITY_BLOCK_VERSION_V1),
                "exact_bucket_assertions": ref_json("candidate/exact_buckets.jsonl", CANON_ENTITY_BLOCK_BUCKET_VERSION),
                "report": ref_json("candidate/report.json", CANON_ENTITY_CANDIDATE_RECALL_VERSION),
                "exact_bucket_count": 0
            },
            "solve_derivation": {
                "edge_artifact": ref_json("run/evidence/evidence.json", CANON_ENTITY_EVIDENCE_VERSION_V1),
                "edge_records": ref_json("run/evidence/evidence.jsonl", CANON_ENTITY_EVIDENCE_VERSION_V1),
                "prepared_surfaces": ref_json("run/prepare/surfaces.jsonl", CANON_ENTITY_PREPARE_VERSION_V1),
                "solve_policy": ref_json("run/solve/policy.json", CANON_GENERALIZATION_SOLVE_POLICY_VERSION)
            },
            "cache_execution": {
                "version": CANON_GENERALIZATION_CACHE_EXECUTION_VERSION,
                "mode": "disabled_bypass",
                "receipt": ref_json("run/cache_execution_receipt.json", CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION),
                "bundle_receipt": ref_json("index/cache_receipt.json", CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION)
            },
            "artifacts": [],
            "cross_bindings": {
                "benchmark_id": "benchmark",
                "run_id": "run",
                "policy_digest": hash_bytes(b"policy"),
                "registry_id": "registry",
                "registry_version": "v1",
                "registry_snapshot_hash": hash_bytes(b"registry"),
                "observation_namespace": "observations",
                "required_identity_links": []
            },
            "bindings": {
                "observation_bindings": [],
                "result_bindings": [],
                "directional_link_bindings": [],
                "hard_negative_bindings": []
            },
            "leak_scan_sources": {
                "version": CANON_GENERALIZATION_LEAK_SCAN_SOURCES_VERSION,
                "phase": "pre_evaluation_influence",
                "channels": ["alias"],
                "path": "leak/sources.json",
                "content_hash": hash_bytes(b"leak")
            }
        });
        serde_json::from_value::<GeneralizationTrialExecution>(value.clone())
            .expect("registry_dir-present trial parses");
        value
            .as_object_mut()
            .expect("trial json object")
            .remove("registry_dir");
        serde_json::from_value::<GeneralizationTrialExecution>(value)
            .expect_err("missing registry_dir refuses");
    }

    #[test]
    fn cache_execution_ref_requires_fixed_versions() {
        let hash = hash_bytes(b"cache-receipt");
        let reference = cache_execution_ref(
            GeneralizationCacheExecutionMode::DisabledBypass,
            "trials/t1/run/cache_execution_receipt.json",
            &hash,
            "trials/t1/index/cache_receipt.json",
            &hash_bytes(b"bundle"),
        );
        validate_cache_execution_ref(&reference, "cache_execution")
            .expect("cache execution ref validates");

        let mut wrong_contract = reference.clone();
        wrong_contract.version = "canon.evaluation.generalization.cache_execution.v999".to_string();
        let refusal = validate_cache_execution_ref(&wrong_contract, "cache_execution")
            .expect_err("wrong cache execution version refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let mut wrong_receipt = reference;
        wrong_receipt.receipt.version = "canon_entity_index_cache_receipt.v999".to_string();
        let refusal = validate_cache_execution_ref(&wrong_receipt, "cache_execution")
            .expect_err("wrong native receipt version refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn cache_execution_accepts_only_strict_mode_status_reusable_triples() {
        let disabled = cache_execution_ref(
            GeneralizationCacheExecutionMode::DisabledBypass,
            "trials/t1/run/cache_execution_receipt.json",
            &hash_bytes(b"disabled"),
            "trials/t1/index/cache_receipt.json",
            &hash_bytes(b"bundle"),
        );
        validate_cache_execution_receipt_payload(
            &disabled,
            &cache_receipt(
                EntityIndexCacheMode::Disabled,
                EntityIndexCacheStatus::Bypassed,
                false,
            ),
            "cache_execution",
        )
        .expect("disabled bypass is strict-valid");

        let enabled = cache_execution_ref(
            GeneralizationCacheExecutionMode::EnabledWarmHit,
            "trials/t1/run/cache_execution_receipt.json",
            &hash_bytes(b"enabled"),
            "trials/t1/index/cache_receipt.json",
            &hash_bytes(b"bundle"),
        );
        validate_cache_execution_receipt_payload(
            &enabled,
            &cache_receipt(
                EntityIndexCacheMode::Enabled,
                EntityIndexCacheStatus::Hit,
                true,
            ),
            "cache_execution",
        )
        .expect("enabled warm hit is strict-valid");

        let refusal = validate_cache_execution_receipt_payload(
            &enabled,
            &cache_receipt(
                EntityIndexCacheMode::Enabled,
                EntityIndexCacheStatus::Rebuilt,
                true,
            ),
            "cache_execution",
        )
        .expect_err("enabled rebuilt is not strict warm-hit acceptance");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let refusal = validate_cache_execution_receipt_payload(
            &disabled,
            &cache_receipt(
                EntityIndexCacheMode::Disabled,
                EntityIndexCacheStatus::Hit,
                true,
            ),
            "cache_execution",
        )
        .expect_err("disabled reusable hit refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn cache_receipt_bundle_files_are_loaded_and_hash_checked() {
        let temp = tempfile::tempdir().expect("cache receipt tempdir");
        let trial_root = temp.path().join("trials/t1");
        fs::create_dir_all(trial_root.join("index")).expect("create index dir");
        let index_artifact = sealed_index_v1_artifact();
        let index_artifact_hash =
            validate_entity_v1_self_hash(&index_artifact).expect("index v1 self hash validates");
        let index_artifact_bytes =
            serde_json::to_vec_pretty(&index_artifact).expect("index artifact serializes");
        let files: [(&str, &str, &[u8]); 4] = [
            (
                "artifact",
                "index/index.json",
                index_artifact_bytes.as_slice(),
            ),
            ("cache_key", INDEX_CACHE_KEY_FILE, b"cache-key"),
            ("postings", "index/postings.bin", b"postings"),
            (
                "diagnostics",
                DEFAULT_INDEX_DIAGNOSTICS_PATH,
                b"diagnostics",
            ),
        ];
        for (_role, path, bytes) in files {
            let path = trial_root.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create parent dir");
            }
            fs::write(path, bytes).expect("write cache bundle file");
        }
        let receipt = cache_bundle_receipt(
            &files,
            EntityIndexCacheMode::Enabled,
            EntityIndexCacheStatus::Hit,
            true,
        );

        let loaded = load_and_validate_cache_receipt_bundle_files(
            temp.path(),
            "trials/t1/run.json",
            &receipt,
            None,
            "cache_execution",
        )
        .expect("cache bundle validates");
        assert_eq!(loaded.len(), 4);
        assert_eq!(
            loaded[0].index_artifact_content_hash.as_deref(),
            Some(index_artifact_hash.as_str())
        );
        assert_ne!(loaded[0].content_hash, index_artifact_hash);

        let mut wrong_hash = receipt.clone();
        wrong_hash.files[1].content_hash = hash_bytes(b"wrong-cache-key");
        let refusal = load_and_validate_cache_receipt_bundle_files(
            temp.path(),
            "trials/t1/run.json",
            &wrong_hash,
            None,
            "cache_execution",
        )
        .expect_err("file hash mismatch refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let mut wrong_bundle = receipt;
        wrong_bundle.bundle_hash = hash_bytes(b"wrong-bundle");
        let refusal = load_and_validate_cache_receipt_bundle_files(
            temp.path(),
            "trials/t1/run.json",
            &wrong_bundle,
            None,
            "cache_execution",
        )
        .expect_err("bundle hash mismatch refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn cache_execution_run_binding_requires_mode_specific_stage_and_index_upstream() {
        let mut run = baseline_run();
        let receipt_hash = hash_bytes(b"receipt");
        let bundle_hash = hash_bytes(b"bundle-receipt");
        let leak_bundle = leak_bundle_ref();
        let reference = cache_execution_ref(
            GeneralizationCacheExecutionMode::EnabledWarmHit,
            "trials/t1/run/cache_execution_receipt.json",
            &receipt_hash,
            "trials/t1/index/cache_receipt.json",
            &bundle_hash,
        );
        let receipt = cache_receipt(
            EntityIndexCacheMode::Enabled,
            EntityIndexCacheStatus::Hit,
            true,
        );
        let bundle_receipt = cache_receipt(
            EntityIndexCacheMode::Enabled,
            EntityIndexCacheStatus::Rebuilt,
            true,
        );
        run.summary
            .labels
            .insert("cache_mode".to_string(), "enabled".to_string());
        run.summary
            .labels
            .insert("cache_status".to_string(), "hit".to_string());
        run.summary.labels.insert(
            "cache_receipt_path".to_string(),
            RUN_CACHE_EXECUTION_RECEIPT_PATH.to_string(),
        );
        run.summary
            .labels
            .insert("cache_receipt_hash".to_string(), receipt_hash.clone());
        run.summary.labels.insert(
            "cache_bundle_receipt_path".to_string(),
            INDEX_CACHE_RECEIPT_FILE.to_string(),
        );
        run.summary
            .labels
            .insert("cache_bundle_receipt_hash".to_string(), bundle_hash.clone());
        let index_stage = run
            .stage_artifacts
            .iter()
            .find(|stage| stage.stage == "index")
            .cloned()
            .expect("index stage exists");
        let mut cache_upstreams = vec![
            stage_artifact_ref(&index_stage),
            artifact_ref(CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION, &bundle_hash),
            artifact_ref(&leak_bundle.version, &leak_bundle.content_hash),
        ];
        cache_upstreams.sort_by(entity_artifact_ref_cmp);
        run.stage_artifacts.push(native_stage(
            "cache_enabled",
            CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION,
            RUN_CACHE_EXECUTION_RECEIPT_PATH,
            &receipt_hash,
            cache_upstreams,
        ));
        let bundle_files = vec![LoadedGeneralizationCacheReceiptFile {
            role: "artifact".to_string(),
            path: "index/index.json".to_string(),
            content_hash: hash_bytes(b"raw-index-json-bytes"),
            byte_count: 1,
            index_artifact_content_hash: Some(index_stage.artifact_content_hash.clone()),
        }];

        validate_cache_execution_run_binding(CacheExecutionRunBindingContext {
            run_ref_path: "trials/t1/run.json",
            run: &run,
            reference: &reference,
            leak_source_bundle: &leak_bundle,
            receipt: &receipt,
            bundle_receipt: &bundle_receipt,
            bundle_files: &bundle_files,
            field: "cache_execution",
        })
        .expect("mode-specific cache stage validates");

        let mut wrong_index_artifact_hash = bundle_files.clone();
        wrong_index_artifact_hash[0].index_artifact_content_hash =
            Some(hash_bytes(b"wrong-index-artifact"));
        let refusal = validate_cache_execution_run_binding(CacheExecutionRunBindingContext {
            run_ref_path: "trials/t1/run.json",
            run: &run,
            reference: &reference,
            leak_source_bundle: &leak_bundle,
            receipt: &receipt,
            bundle_receipt: &bundle_receipt,
            bundle_files: &wrong_index_artifact_hash,
            field: "cache_execution",
        })
        .expect_err("parsed index artifact hash mismatch refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let mut generic_stage = run.clone();
        generic_stage
            .stage_artifacts
            .last_mut()
            .expect("cache stage")
            .stage = "cache_receipt".to_string();
        let refusal = validate_cache_execution_run_binding(CacheExecutionRunBindingContext {
            run_ref_path: "trials/t1/run.json",
            run: &generic_stage,
            reference: &reference,
            leak_source_bundle: &leak_bundle,
            receipt: &receipt,
            bundle_receipt: &bundle_receipt,
            bundle_files: &bundle_files,
            field: "cache_execution",
        })
        .expect_err("generic fake cache receipt stage refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let mut missing_upstream = run;
        missing_upstream
            .stage_artifacts
            .last_mut()
            .expect("cache stage")
            .upstream_artifacts
            .clear();
        let refusal = validate_cache_execution_run_binding(CacheExecutionRunBindingContext {
            run_ref_path: "trials/t1/run.json",
            run: &missing_upstream,
            reference: &reference,
            leak_source_bundle: &leak_bundle,
            receipt: &receipt,
            bundle_receipt: &bundle_receipt,
            bundle_files: &bundle_files,
            field: "cache_execution",
        })
        .expect_err("missing index upstream refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn cache_leak_source_must_match_cache_execution_receipt() {
        let receipt_hash = hash_bytes(b"receipt");
        let cache_execution = LoadedGeneralizationCacheExecution {
            references: cache_execution_ref(
                GeneralizationCacheExecutionMode::DisabledBypass,
                "trials/t1/run/cache_execution_receipt.json",
                &receipt_hash,
                "trials/t1/index/cache_receipt.json",
                &hash_bytes(b"bundle-receipt"),
            ),
            receipt_path: "trials/t1/run/cache_execution_receipt.json".to_string(),
            receipt_hash: receipt_hash.clone(),
            receipt_byte_count: 42,
            receipt: cache_receipt(
                EntityIndexCacheMode::Disabled,
                EntityIndexCacheStatus::Bypassed,
                false,
            ),
            bundle_receipt_path: "trials/t1/index/cache_receipt.json".to_string(),
            bundle_receipt_hash: hash_bytes(b"bundle-receipt"),
            bundle_receipt_byte_count: 99,
            bundle_receipt: cache_receipt(
                EntityIndexCacheMode::Enabled,
                EntityIndexCacheStatus::Rebuilt,
                true,
            ),
            bundle_files: Vec::new(),
        };
        let source = LoadedGeneralizationLeakSourceRef {
            source_id: "cache".to_string(),
            phase: GeneralizationLeakSourcePhase::PreEvaluationInfluence,
            channel: LeakChannel::Cache,
            source_kind: GeneralizationLeakSourceKind::Cache,
            binding_kind: GeneralizationLeakSourceBindingKind::RunStageArtifact,
            binding_hash: receipt_hash.clone(),
            coverage: GeneralizationLeakSourceCoverage::CompleteSource,
            content_hash: hash_bytes(b"records"),
            bundle_content_hash: hash_bytes(b"bundle"),
            checked_sources: vec![LoadedGeneralizationCheckedLeakSourceRef {
                path: "trials/t1/run/cache_execution_receipt.json".to_string(),
                format: GeneralizationLeakSourceFormat::Json,
                content_hash: receipt_hash.clone(),
                byte_count: 42,
                record_count: 1,
            }],
            bytes: Vec::new(),
            decoded_strings: BTreeSet::new(),
        };
        validate_cache_leak_source_matches_execution(
            std::slice::from_ref(&source),
            &cache_execution,
            "leak_scan_sources",
        )
        .expect("cache leak source matches execution receipt");

        let mut wrong_path = source;
        wrong_path.checked_sources[0].path = "trials/t1/index/cache_key.json".to_string();
        let refusal = validate_cache_leak_source_matches_execution(
            std::slice::from_ref(&wrong_path),
            &cache_execution,
            "leak_scan_sources",
        )
        .expect_err("cache key checked source cannot satisfy cache receipt proof");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn solve_derivation_paths_bind_to_loaded_run_work_dir_and_refuse_copies() {
        let run = baseline_run();
        let fixture = replacement_artifacts(&run);
        let run_ref_path = "trials/trial/run/run.json";
        let mut derivation = loaded_solve_derivation_for_rebuild(&fixture);
        derivation.references = solve_derivation_refs_for_run_ref(&run, run_ref_path);

        validate_solve_derivation_path_continuity(run_ref_path, &run, &derivation)
            .expect("canonical run-relative solve derivation paths validate");

        let mut copied_path = derivation.clone();
        copied_path.references.edge_records.path = run.work_dir.edge_records_path.clone();
        let refusal = validate_solve_derivation_path_continuity(run_ref_path, &run, &copied_path)
            .expect_err("manifest-relative copy of run-relative edge records path refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let mut duplicate_path = derivation.clone();
        duplicate_path.references.solve_policy.path =
            duplicate_path.references.prepared_surfaces.path.clone();
        let refusal =
            validate_solve_derivation_path_continuity(run_ref_path, &run, &duplicate_path)
                .expect_err("solve policy path collision refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::DuplicateRecord);

        let mut duplicate_hash = derivation;
        duplicate_hash.references.solve_policy.content_hash =
            duplicate_hash.references.edge_records.content_hash.clone();
        let refusal =
            validate_solve_derivation_path_continuity(run_ref_path, &run, &duplicate_hash)
                .expect_err("solve policy hash collision refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::DuplicateRecord);
    }

    #[test]
    fn rebind_native_stages_refuses_stale_run_and_unsafe_path() {
        let mut run = baseline_run();
        let fixture = replacement_artifacts(&run);
        run.summary
            .counts
            .insert("candidate_pairs".to_string(), 999);
        let refusal = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect_err("stale baseline run refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let mut run = baseline_run();
        run.work_dir.block_artifact_path = "../block.json".to_string();
        run = seal_run(run);
        let fixture = replacement_artifacts(&run);
        let refusal = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect_err("unsafe work_dir block path refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn rebind_native_stages_refuses_mismatched_chain() {
        let run = baseline_run();
        let mut fixture = replacement_artifacts(&run);
        fixture.block.candidate_records_hash = hash_bytes(b"wrong-candidates");
        fixture.block = seal_block(fixture.block);

        let refusal = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect_err("block payload hash mismatch refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let mut fixture = replacement_artifacts(&run);
        fixture.edge.candidate_records_hash = hash_bytes(b"wrong-candidates");
        fixture.edge = seal_edge(fixture.edge);
        let refusal = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect_err("edge payload mismatch refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn rebind_native_stages_refuses_wrong_block_strategy() {
        let run = baseline_run();
        let mut fixture = replacement_artifacts(&run);
        fixture.block.metadata.strategy = run.metadata.strategy.clone();
        fixture.block = seal_block(fixture.block);
        rebuild_fixture_edge(&mut fixture, &run);

        let refusal = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect_err("base block strategy refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn rebind_native_stages_derives_incumbent_from_resolved_exact_lookup() {
        let run = run_with_prepare_counts(baseline_run(), 1, 1, 1);
        let mut fixture = replacement_artifacts(&run);
        fixture.prepared_surfaces = vec![prepared_surface(
            &run,
            "surface.incumbent",
            resolved_exact_lookup(&run),
        )];

        let result = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect("resolved exact lookup incumbent rebinds");

        assert_eq!(result.solve.entities.len(), 1);
        assert_eq!(
            result.solve.entities[0].state,
            SolveReconciliationState::ResolvedExisting
        );
        assert_eq!(
            result.solve.entities[0].canonical_id.as_deref(),
            Some("ORG-001")
        );
    }

    #[test]
    fn rebind_native_stages_refuses_unproven_incumbent_in_prepared_surface() {
        let run = baseline_run();
        let mut fixture = replacement_artifacts(&run);
        let mut exact_lookup = unresolved_exact_lookup(&run);
        exact_lookup.canonical_id = Some("ORG-999".to_string());
        fixture.prepared_surfaces = vec![prepared_surface(
            &run,
            "surface.unproven-incumbent",
            exact_lookup,
        )];

        let refusal = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect_err("unresolved prepared surface cannot carry incumbent id");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn rebind_native_stages_replays_exact_lookup_before_incumbent_derivation() {
        let run = run_with_prepare_counts(baseline_run(), 1, 1, 1);
        let mut fixture = replacement_artifacts(&run);
        let mut exact_lookup = resolved_exact_lookup(&run);
        exact_lookup.canonical_id = Some("ORG-999".to_string());
        fixture.prepared_surfaces = vec![prepared_surface(
            &run,
            "surface.replay-mismatch",
            exact_lookup,
        )];

        let refusal = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect_err("forged resolved canonical id refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn rebind_native_stages_refuses_prepared_payload_omission_and_count_mismatch() {
        let run = baseline_run();
        let mut fixture = replacement_artifacts(&run);
        fixture.prepared_surfaces.clear();
        let refusal = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect_err("omitted prepared surface payload refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let run = run_with_prepare_counts(baseline_run(), 2, 1, 0);
        let fixture = replacement_artifacts(&run);
        let refusal = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect_err("prepared surface count mismatch refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let run = run_with_prepare_counts(baseline_run(), 1, 2, 0);
        let fixture = replacement_artifacts(&run);
        let refusal = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect_err("prepared row_count sum mismatch refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn rebind_native_stages_refuses_forged_prepared_surface_id() {
        let run = baseline_run();
        let mut fixture = replacement_artifacts(&run);
        fixture.prepared_surfaces[0].surface_id = "surf:profile:blake3:forged".to_string();

        let refusal = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect_err("forged prepared surface id refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn rebind_native_stages_validates_registry_root_hash_before_replay() {
        let mut run = baseline_run();
        run.metadata.registry_snapshot.lookup_snapshot_hash = hash_bytes(b"wrong-registry-root");
        run.orchestration.profile_firewall.registry_snapshot_hash =
            run.metadata.registry_snapshot.lookup_snapshot_hash.clone();
        run = seal_run(run);
        let fixture = replacement_artifacts(&run);

        let refusal = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect_err("stale registry root hash refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn rebind_native_stages_uses_explicit_registry_dir_not_run_source() {
        let mut run = baseline_run();
        let registry_dir = PathBuf::from(&run.metadata.registry_snapshot.source);
        run.metadata.registry_snapshot.source = "/private/ambient/registry".to_string();
        run = seal_run(run);
        let mut fixture = replacement_artifacts(&run);
        fixture.registry_dir = registry_dir;

        let result = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect("explicit registry_dir drives replay");

        assert_eq!(
            result.run.metadata.registry_snapshot.source,
            "/private/ambient/registry"
        );
    }

    #[test]
    fn strict_registry_dir_resolves_manifest_relative_and_refuses_unsafe_paths() {
        let temp = tempfile::tempdir().expect("manifest tempdir");
        fs::create_dir(temp.path().join("registry")).expect("registry dir");

        let resolved = resolve_strict_manifest_dir(temp.path(), "trial.registry_dir", "registry")
            .expect("manifest-relative registry dir resolves");
        assert!(resolved.ends_with("registry"));

        for unsafe_path in ["../registry", "/tmp/registry"] {
            let refusal =
                resolve_strict_manifest_dir(temp.path(), "trial.registry_dir", unsafe_path)
                    .expect_err("unsafe registry_dir refuses");
            assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
        }
    }

    #[cfg(unix)]
    #[test]
    fn registry_replay_hash_refuses_symlink_root_and_entries() {
        let registry_dir = write_test_registry();
        let temp = tempfile::tempdir().expect("symlink tempdir");
        let linked_root = temp.path().join("registry-link");
        std::os::unix::fs::symlink(&registry_dir, &linked_root).expect("root symlink");
        let refusal = hash_registry_json_files_for_replay(&linked_root)
            .expect_err("registry symlink root refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let registry_dir = write_test_registry();
        std::os::unix::fs::symlink(
            registry_dir.join("aliases.json"),
            registry_dir.join("linked-aliases.json"),
        )
        .expect("entry symlink");
        let refusal = hash_registry_json_files_for_replay(&registry_dir)
            .expect_err("registry symlink entry refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn candidate_rank_uses_min_rank_across_distinct_operators_and_refuses_duplicate_operator() {
        let ranks = vec![
            CandidateRecallRankRecord {
                gold_pair_id: "gold.pair".to_string(),
                stratum: CandidateRecallStratum::NovelCluster,
                operator_id: "posting".to_string(),
                rank: 9,
            },
            CandidateRecallRankRecord {
                gold_pair_id: "gold.pair".to_string(),
                stratum: CandidateRecallStratum::NovelCluster,
                operator_id: "ngram".to_string(),
                rank: 3,
            },
        ];

        let rank = candidate_rank_from_true_pair_ranks("result", "gold.pair", &ranks)
            .expect("distinct operators are accepted");
        assert_eq!(rank, Some(3));

        let duplicate_operator = vec![
            CandidateRecallRankRecord {
                gold_pair_id: "gold.pair".to_string(),
                stratum: CandidateRecallStratum::NovelCluster,
                operator_id: "posting".to_string(),
                rank: 9,
            },
            CandidateRecallRankRecord {
                gold_pair_id: "gold.pair".to_string(),
                stratum: CandidateRecallStratum::NovelCluster,
                operator_id: "posting".to_string(),
                rank: 3,
            },
        ];
        let refusal =
            candidate_rank_from_true_pair_ranks("result", "gold.pair", &duplicate_operator)
                .expect_err("duplicate operator rank refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::DuplicateRecord);
    }

    #[test]
    fn rebind_native_stages_refuses_firewall_strategy_mismatch_and_missing_summary() {
        let mut run = baseline_run();
        let mut fixture = replacement_artifacts(&run);
        run.orchestration.profile_firewall.strategy_hash = hash_bytes(b"other-strategy");
        run = seal_run(run);
        let refusal = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect_err("firewall strategy mismatch refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let run = baseline_run();
        fixture = replacement_artifacts(&run);
        fixture.block.summary.counts.remove("candidate_pairs");
        fixture.block = seal_block(fixture.block);
        rebuild_fixture_edge(&mut fixture, &run);
        let refusal = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect_err("missing required summary count refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn rebind_native_stages_does_not_accept_caller_supplied_solve_state() {
        let run = baseline_run();
        let fixture = replacement_artifacts(&run);
        let block_ref = artifact_ref(&fixture.block.version, &fixture.block.artifact_content_hash);
        let edge_ref = artifact_ref(&fixture.edge.version, &fixture.edge.artifact_content_hash);
        let mut solve_metadata = run.metadata.clone();
        solve_metadata.strategy = derived_stage_strategy(&run.metadata.strategy, "solve");
        solve_metadata.upstream_artifacts = vec![block_ref, edge_ref];
        solve_metadata.artifact_content_hash.clear();
        let malicious = seal_solve(SolveArtifact {
            version: CANON_ENTITY_SOLVE_VERSION_V1.to_string(),
            artifact_content_hash: String::new(),
            metadata: solve_metadata.clone(),
            summary: deterministic_summary(&[("entity_count", 999), ("review_group_count", 999)]),
            upstream_artifacts: solve_metadata.upstream_artifacts.clone(),
            promotable_aliases: Vec::new(),
            entities: Vec::new(),
            review_groups: Vec::new(),
            diagnostics: crate::entity::solve::SolveDiagnosticsReport {
                summary: BTreeMap::new(),
                components: Vec::new(),
                review_group_seeds: Vec::new(),
            },
            decision_ledger_path: run.work_dir.decision_ledger_path.clone(),
        });

        let result = rebind_generalization_native_stages(rebind_request(&run, &fixture))
            .expect("native stages rebind");

        assert_ne!(
            result.solve.artifact_content_hash,
            malicious.artifact_content_hash
        );
        let solve_stage = result
            .run
            .stage_artifacts
            .iter()
            .find(|stage| stage.stage == "solve")
            .expect("solve stage exists");
        assert_eq!(
            solve_stage.artifact_content_hash,
            result.solve.artifact_content_hash
        );
        assert_ne!(
            solve_stage.artifact_content_hash,
            malicious.artifact_content_hash
        );
    }

    #[test]
    fn bind_run_provenance_adds_receipts_bundle_and_preserves_native_fields() {
        let run = run_with_cache_execution_stage(baseline_run());
        let bundle = leak_bundle_ref();
        let generated_hash = hash_bytes(b"generated-receipt");
        let generated = generated_stage("generated/corpus_receipt.json", &generated_hash);

        let decorated = bind_generalization_run_provenance(
            &run,
            "benchmark",
            "run-1",
            "trial-1",
            GeneralizationTrialFamily::TimeForward,
            &bundle,
            generated.clone(),
        )
        .expect("run provenance binds");
        let second = bind_generalization_run_provenance(
            &run,
            "benchmark",
            "run-1",
            "trial-1",
            GeneralizationTrialFamily::TimeForward,
            &bundle,
            generated,
        )
        .expect("run provenance binds deterministically");

        assert_eq!(
            decorated.artifact_content_hash,
            second.artifact_content_hash
        );
        assert_eq!(decorated.work_dir, run.work_dir);
        assert_eq!(decorated.next_commands, run.next_commands);
        assert_eq!(
            decorated.orchestration.stage_order,
            run.orchestration.stage_order
        );
        assert_eq!(
            decorated.orchestration.profile_firewall,
            run.orchestration.profile_firewall
        );
        assert_eq!(
            decorated.orchestration.handoff_steps[1],
            run.orchestration.handoff_steps[1]
        );
        assert_eq!(
            decorated.summary.labels.get("benchmark_id"),
            Some(&"benchmark".to_string())
        );
        assert_eq!(
            decorated.summary.labels.get("family"),
            Some(&"time_forward".to_string())
        );
        let bundle_ref = artifact_ref(&bundle.version, &bundle.content_hash);
        assert!(decorated.metadata.upstream_artifacts.contains(&bundle_ref));
        assert_eq!(
            decorated.stage_artifacts.len(),
            run.stage_artifacts.len() + 1
        );
        for (original, decorated_stage) in
            run.stage_artifacts.iter().zip(&decorated.stage_artifacts)
        {
            if original.stage == "cache_enabled" {
                assert_eq!(decorated_stage.stage, original.stage);
                assert_eq!(decorated_stage.version, original.version);
                assert_eq!(decorated_stage.path, original.path);
                assert_eq!(
                    decorated_stage.artifact_content_hash,
                    original.artifact_content_hash
                );
                assert!(decorated_stage.upstream_artifacts.contains(&bundle_ref));
            } else {
                assert_eq!(decorated_stage, original);
            }
        }
        assert_eq!(
            decorated.orchestration.handoff_steps[0].input_artifacts,
            decorated
                .stage_artifacts
                .iter()
                .map(stage_artifact_ref)
                .collect::<Vec<_>>()
        );
        assert!(
            decorated.stage_artifacts[run.stage_artifacts.len()]
                .upstream_artifacts
                .contains(&bundle_ref)
        );
    }

    #[test]
    fn bind_run_provenance_refuses_stale_run_and_conflicting_labels() {
        let mut run = run_with_cache_execution_stage(baseline_run());
        let bundle = leak_bundle_ref();
        run.summary
            .counts
            .insert("candidate_pairs".to_string(), 999);
        let refusal = bind_generalization_run_provenance(
            &run,
            "benchmark",
            "run-1",
            "trial-1",
            GeneralizationTrialFamily::TimeForward,
            &bundle,
            generated_stage("generated/corpus_receipt.json", &hash_bytes(b"generated")),
        )
        .expect_err("stale run refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let mut run = run_with_cache_execution_stage(baseline_run());
        run.summary
            .labels
            .insert("benchmark_id".to_string(), "other".to_string());
        run = seal_run(run);
        let refusal = bind_generalization_run_provenance(
            &run,
            "benchmark",
            "run-1",
            "trial-1",
            GeneralizationTrialFamily::TimeForward,
            &bundle,
            generated_stage("generated/corpus_receipt.json", &hash_bytes(b"generated")),
        )
        .expect_err("conflicting label refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn bind_run_provenance_refuses_invalid_leak_bundle_ref() {
        let run = run_with_cache_execution_stage(baseline_run());
        let mut bundle = leak_bundle_ref();
        bundle.phase = GeneralizationLeakSourcePhase::BuildInfluence;

        let refusal = bind_generalization_run_provenance(
            &run,
            "benchmark",
            "run-1",
            "trial-1",
            GeneralizationTrialFamily::TimeForward,
            &bundle,
            generated_stage("generated/corpus_receipt.json", &hash_bytes(b"generated")),
        )
        .expect_err("invalid leak bundle phase refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn bind_run_provenance_requires_production_native_cache_stage() {
        let bundle = leak_bundle_ref();
        let generated = generated_stage("generated/corpus_receipt.json", &hash_bytes(b"generated"));
        let refusal = bind_generalization_run_provenance(
            &baseline_run(),
            "benchmark",
            "run-1",
            "trial-1",
            GeneralizationTrialFamily::TimeForward,
            &bundle,
            generated.clone(),
        )
        .expect_err("missing native cache stage refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let mut run = run_with_cache_execution_stage(baseline_run());
        run.summary.labels.insert(
            "cache_receipt_hash".to_string(),
            hash_bytes(b"wrong-cache-receipt"),
        );
        run = seal_run(run);
        let refusal = bind_generalization_run_provenance(
            &run,
            "benchmark",
            "run-1",
            "trial-1",
            GeneralizationTrialFamily::TimeForward,
            &bundle,
            generated.clone(),
        )
        .expect_err("cache receipt label mismatch refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let mut run = run_with_cache_execution_stage(baseline_run());
        let duplicate = run
            .stage_artifacts
            .iter()
            .find(|stage| stage.stage == "cache_enabled")
            .expect("cache stage exists")
            .clone();
        run.stage_artifacts.push(duplicate);
        run = seal_run(run);
        let refusal = bind_generalization_run_provenance(
            &run,
            "benchmark",
            "run-1",
            "trial-1",
            GeneralizationTrialFamily::TimeForward,
            &bundle,
            generated,
        )
        .expect_err("duplicate native cache stages refuse");
        assert_eq!(refusal.code, GeneralizationErrorCode::DuplicateRecord);
    }

    #[test]
    fn bind_run_provenance_refuses_wrong_duplicate_receipts_and_unsafe_path() {
        let run = run_with_cache_execution_stage(baseline_run());
        let bundle = leak_bundle_ref();
        let refusal = bind_generalization_run_provenance(
            &run,
            "benchmark",
            "run-1",
            "trial-1",
            GeneralizationTrialFamily::TimeForward,
            &bundle,
            cache_stage("cache/receipt.json", &hash_bytes(b"cache")),
        )
        .expect_err("wrong generated receipt class refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let cache_hash = run
            .stage_artifacts
            .iter()
            .find(|stage| stage.stage == "cache_enabled")
            .expect("cache stage exists")
            .artifact_content_hash
            .clone();
        let refusal = bind_generalization_run_provenance(
            &run,
            "benchmark",
            "run-1",
            "trial-1",
            GeneralizationTrialFamily::TimeForward,
            &bundle,
            generated_stage("generated/corpus_receipt.json", &cache_hash),
        )
        .expect_err("duplicate receipt hash refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::DuplicateRecord);

        let refusal = bind_generalization_run_provenance(
            &run,
            "benchmark",
            "run-1",
            "trial-1",
            GeneralizationTrialFamily::TimeForward,
            &bundle,
            generated_stage("../generated.json", &hash_bytes(b"generated")),
        )
        .expect_err("unsafe receipt path refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let block_stage = run
            .stage_artifacts
            .iter()
            .find(|stage| stage.stage == "block")
            .expect("block stage exists");
        let refusal = bind_generalization_run_provenance(
            &run,
            "benchmark",
            "run-1",
            "trial-1",
            GeneralizationTrialFamily::TimeForward,
            &bundle,
            generated_stage(&block_stage.path, &hash_bytes(b"generated")),
        )
        .expect_err("receipt path colliding with block stage refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let evidence_stage = run
            .stage_artifacts
            .iter()
            .find(|stage| stage.stage == "evidence")
            .expect("evidence stage exists");
        let refusal = bind_generalization_run_provenance(
            &run,
            "benchmark",
            "run-1",
            "trial-1",
            GeneralizationTrialFamily::TimeForward,
            &bundle,
            generated_stage(
                "generated/corpus_receipt.json",
                &evidence_stage.artifact_content_hash,
            ),
        )
        .expect_err("receipt hash colliding with evidence stage refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn non_registry_first_seen_path_inserts_but_duplicate_refuses() {
        let checked = checked_source("thresholds.json");
        let binding_hash = hash_bytes(b"threshold-binding");
        let threshold = source(
            "threshold",
            LeakChannel::Threshold,
            GeneralizationLeakSourceKind::Threshold,
            GeneralizationLeakSourceBindingKind::Strategy,
            &binding_hash,
        );
        let dictionary = source(
            "dictionary",
            LeakChannel::Dictionary,
            GeneralizationLeakSourceKind::Dictionary,
            GeneralizationLeakSourceBindingKind::Strategy,
            &binding_hash,
        );
        let mut observed = BTreeMap::new();

        validate_checked_path_reuse(
            &mut observed,
            &threshold,
            std::slice::from_ref(&checked),
            None,
            "threshold",
        )
        .expect("first non-registry path inserts without completeness");
        let duplicate = validate_checked_path_reuse(
            &mut observed,
            &dictionary,
            std::slice::from_ref(&checked),
            None,
            "dictionary",
        )
        .expect_err("duplicate non-registry path refuses");

        assert_eq!(duplicate.code, GeneralizationErrorCode::DuplicateRecord);
    }

    #[test]
    fn identical_registry_alias_anchor_paths_can_share() {
        let checked = checked_source("registries/test/aliases.json");
        let binding_hash = hash_bytes(b"registry-binding");
        let alias = source(
            "alias",
            LeakChannel::Alias,
            GeneralizationLeakSourceKind::RegistryTree,
            GeneralizationLeakSourceBindingKind::RegistrySnapshot,
            &binding_hash,
        );
        let anchor = source(
            "anchor",
            LeakChannel::Anchor,
            GeneralizationLeakSourceKind::RegistryTree,
            GeneralizationLeakSourceBindingKind::RegistrySnapshot,
            &binding_hash,
        );
        let completeness = completeness(&checked);
        let mut observed = BTreeMap::new();

        validate_checked_path_reuse(
            &mut observed,
            &alias,
            std::slice::from_ref(&checked),
            Some(&completeness),
            "alias",
        )
        .expect("first registry path inserts");
        validate_checked_path_reuse(
            &mut observed,
            &anchor,
            std::slice::from_ref(&checked),
            Some(&completeness),
            "anchor",
        )
        .expect("identical alias/anchor registry path reuse is allowed");
    }

    #[test]
    fn nested_run_ref_resolves_cache_stage_path_relative_to_work_dir() {
        let resolved = safe_run_stage_checked_path(
            "trials/t1/run/run.json",
            "index/cache_key.json",
            "stage.path",
        )
        .expect("run-relative cache path resolves");

        assert_eq!(resolved, "trials/t1/index/cache_key.json");
    }

    #[test]
    fn unsafe_cache_stage_path_refuses() {
        for unsafe_path in ["../cache_key.json", "/tmp/cache_key.json"] {
            let refusal =
                safe_run_stage_checked_path("trials/t1/run/run.json", unsafe_path, "stage.path")
                    .expect_err("unsafe run stage path refuses");

            assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
        }
    }

    #[test]
    fn per_trial_run_refs_resolve_cache_stage_paths_distinctly() {
        let left = safe_run_stage_checked_path(
            "trials/t1/run/run.json",
            "cache/cache_key.json",
            "stage.path",
        )
        .expect("left trial path resolves");
        let right = safe_run_stage_checked_path(
            "trials/t2/run/run.json",
            "cache/cache_key.json",
            "stage.path",
        )
        .expect("right trial path resolves");

        assert_eq!(left, "trials/t1/cache/cache_key.json");
        assert_eq!(right, "trials/t2/cache/cache_key.json");
        assert_ne!(left, right);
    }

    #[test]
    fn cache_stage_path_and_hash_mismatch_refuse() {
        let binding_hash = hash_bytes(b"cache-stage");
        let stage = cache_stage("cache/cache_key.json", &binding_hash);
        let mut allowed = AllowedLeakSourceBindings::default();
        allowed
            .insert_safe_pre_evaluation_run_stage("trials/t1/run/run.json", &stage)
            .expect("safe stage inserts");
        let cache_source = source(
            "cache",
            LeakChannel::Cache,
            GeneralizationLeakSourceKind::Cache,
            GeneralizationLeakSourceBindingKind::RunStageArtifact,
            &binding_hash,
        );

        let path_mismatch = LoadedGeneralizationCheckedLeakSourceRef {
            path: "trials/t1/run/cache/other_key.json".to_string(),
            format: GeneralizationLeakSourceFormat::Json,
            content_hash: binding_hash.clone(),
            byte_count: 1,
            record_count: 1,
        };
        let refusal = validate_derived_leak_source_binding(
            &cache_source,
            std::slice::from_ref(&path_mismatch),
            &[],
            &allowed,
            "cache",
        )
        .expect_err("cache checked path mismatch refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let hash_mismatch = LoadedGeneralizationCheckedLeakSourceRef {
            path: "trials/t1/run/cache/cache_key.json".to_string(),
            format: GeneralizationLeakSourceFormat::Json,
            content_hash: hash_bytes(b"wrong-cache-stage"),
            byte_count: 1,
            record_count: 1,
        };
        let refusal = validate_derived_leak_source_binding(
            &cache_source,
            std::slice::from_ref(&hash_mismatch),
            &[],
            &allowed,
            "cache",
        )
        .expect_err("cache checked hash mismatch refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn dedicated_generated_corpus_stage_exact_match_passes() {
        let binding_hash = hash_bytes(b"generated-stage");
        let stage = generated_stage("generated/corpus_receipt.json", &binding_hash);
        let mut allowed = AllowedLeakSourceBindings::default();
        allowed
            .insert_safe_pre_evaluation_run_stage("trials/t1/run/run.json", &stage)
            .expect("safe generated stage inserts");
        let generated = source(
            "generated",
            LeakChannel::GeneratedCorpus,
            GeneralizationLeakSourceKind::GeneratedCorpus,
            GeneralizationLeakSourceBindingKind::RunStageArtifact,
            &binding_hash,
        );
        let checked = loaded_checked_source_with_hash(
            "trials/t1/generated/corpus_receipt.json",
            &binding_hash,
        );

        validate_leak_source_binding(&generated, &allowed, "generated")
            .expect("generated source binds the generated-corpus run stage");
        validate_derived_leak_source_binding(
            &generated,
            std::slice::from_ref(&checked),
            &[],
            &allowed,
            "generated",
        )
        .expect("generated checked source matches the bound run stage");
    }

    #[test]
    fn generic_generated_corpus_stage_name_does_not_satisfy_generated_source() {
        let binding_hash = hash_bytes(b"generated-stage");
        let stage = run_stage("generated_corpus", "generated/corpus.json", &binding_hash);
        let mut allowed = AllowedLeakSourceBindings::default();
        allowed
            .insert_safe_pre_evaluation_run_stage("trials/t1/run/run.json", &stage)
            .expect("generic generated stage is ignored rather than inserted");
        let generated = source(
            "generated",
            LeakChannel::GeneratedCorpus,
            GeneralizationLeakSourceKind::GeneratedCorpus,
            GeneralizationLeakSourceBindingKind::RunStageArtifact,
            &binding_hash,
        );

        let refusal = validate_leak_source_binding(&generated, &allowed, "generated")
            .expect_err("generic generated_corpus stage must not authorize generated corpus");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn cache_stage_cannot_satisfy_generated_corpus_source() {
        let binding_hash = hash_bytes(b"cache-stage");
        let stage = cache_stage("cache/cache_key.json", &binding_hash);
        let mut allowed = AllowedLeakSourceBindings::default();
        allowed
            .insert_safe_pre_evaluation_run_stage("trials/t1/run/run.json", &stage)
            .expect("safe cache stage inserts");
        let generated = source(
            "generated",
            LeakChannel::GeneratedCorpus,
            GeneralizationLeakSourceKind::GeneratedCorpus,
            GeneralizationLeakSourceBindingKind::RunStageArtifact,
            &binding_hash,
        );

        let refusal = validate_leak_source_binding(&generated, &allowed, "generated")
            .expect_err("cache stage hash must not authorize generated corpus");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn generated_corpus_stage_cannot_satisfy_cache_source() {
        let binding_hash = hash_bytes(b"generated-stage");
        let stage = generated_stage("generated/corpus_receipt.json", &binding_hash);
        let mut allowed = AllowedLeakSourceBindings::default();
        allowed
            .insert_safe_pre_evaluation_run_stage("trials/t1/run/run.json", &stage)
            .expect("safe generated stage inserts");
        let cache = source(
            "cache",
            LeakChannel::Cache,
            GeneralizationLeakSourceKind::Cache,
            GeneralizationLeakSourceBindingKind::RunStageArtifact,
            &binding_hash,
        );

        let refusal = validate_leak_source_binding(&cache, &allowed, "cache")
            .expect_err("generated-corpus stage hash must not authorize cache");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn generic_cache_receipt_stage_does_not_satisfy_cache_source() {
        let binding_hash = hash_bytes(b"cache-stage");
        let stage = run_stage("cache_receipt", "index/cache_receipt.json", &binding_hash);
        let mut allowed = AllowedLeakSourceBindings::default();
        allowed
            .insert_safe_pre_evaluation_run_stage("trials/t1/run/run.json", &stage)
            .expect("generic cache receipt stage is ignored rather than inserted");
        let cache = source(
            "cache",
            LeakChannel::Cache,
            GeneralizationLeakSourceKind::Cache,
            GeneralizationLeakSourceBindingKind::RunStageArtifact,
            &binding_hash,
        );

        let refusal = validate_leak_source_binding(&cache, &allowed, "cache")
            .expect_err("generic cache_receipt stage must not authorize cache");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn generated_corpus_stage_path_and_hash_mismatch_refuse() {
        let binding_hash = hash_bytes(b"generated-stage");
        let stage = generated_stage("generated/corpus_receipt.json", &binding_hash);
        let mut allowed = AllowedLeakSourceBindings::default();
        allowed
            .insert_safe_pre_evaluation_run_stage("trials/t1/run/run.json", &stage)
            .expect("safe generated stage inserts");
        let generated = source(
            "generated",
            LeakChannel::GeneratedCorpus,
            GeneralizationLeakSourceKind::GeneratedCorpus,
            GeneralizationLeakSourceBindingKind::RunStageArtifact,
            &binding_hash,
        );

        let path_mismatch = loaded_checked_source_with_hash(
            "trials/t1/run/generated/other_receipt.json",
            &binding_hash,
        );
        let refusal = validate_derived_leak_source_binding(
            &generated,
            std::slice::from_ref(&path_mismatch),
            &[],
            &allowed,
            "generated",
        )
        .expect_err("generated checked path mismatch refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let hash_mismatch = loaded_checked_source_with_hash(
            "trials/t1/run/generated/corpus_receipt.json",
            &hash_bytes(b"wrong-generated-stage"),
        );
        let refusal = validate_derived_leak_source_binding(
            &generated,
            std::slice::from_ref(&hash_mismatch),
            &[],
            &allowed,
            "generated",
        )
        .expect_err("generated checked hash mismatch refuses");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }

    #[test]
    fn generated_corpus_input_binding_and_materialized_hash_refuse() {
        let materialized_hash = hash_bytes(b"combined-input");
        let materialized_path = "trials/t1/link/combined_rows.csv";
        let guard = guard_for_materialized_hash(materialized_path, &materialized_hash);
        let generated_input = source(
            "generated",
            LeakChannel::GeneratedCorpus,
            GeneralizationLeakSourceKind::GeneratedCorpus,
            GeneralizationLeakSourceBindingKind::Input,
            &materialized_hash,
        );
        let checked =
            checked_source_with_hash("trials/t1/pre_eval/combined_rows.csv", &materialized_hash);

        let refusal = validate_leak_source_binding_kind_for_channel(
            generated_input.source_kind,
            generated_input.channel,
            generated_input.binding_kind,
            "generated.binding_kind",
        )
        .expect_err("generated corpus must not bind run input");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);

        let refusal = guard
            .validate_binding(&generated_input.binding_hash, "generated")
            .expect_err("materialized input hash remains prohibited");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
        let refusal = guard
            .validate(&checked, "generated.checked_sources[0]")
            .expect_err("checked materialized hash remains prohibited");
        assert_eq!(refusal.code, GeneralizationErrorCode::ArtifactContract);
    }
}
