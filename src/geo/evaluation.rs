#![forbid(unsafe_code)]

//! Deterministic evaluation of labeled Geo composition cases.
//!
//! Truth labels are held outside evidence compilation and solving. They are used
//! only for post-solve scoring and validation, so ground truth cannot silently
//! become a constraint or preference.

use super::{
    composition::{
        CANON_GEO_COMPOSITION_VERSION, GeoCompositionArtifact, GeoCompositionBackbone,
        GeoCompositionError, GeoCompositionErrorCode, GeoCompositionModel, GeoCompositionRequest,
        GeoCompositionStatus, GeoEntityLevel, GeoEntityRef, GeoIntegerMemberValue,
        GeoResolvedClaim, GeoResolvedClaimClass, canonical_composition_bytes,
        canonicalize_composition_request, model_satisfies_request, solve_composition,
    },
    control::{
        GeoBudgetAction, GeoClaimClass, GeoControlEntityLevel, GeoEvidenceClass,
        GeoIdentityParticipation, GeoNativeEntityScope, GeoNumericBound, GeoResourceCounter,
        GeoSourceRelease, GeoTelemetrySemanticEffect, GeoValueOrigin,
    },
    evidence::{
        CANON_GEO_EVIDENCE_COMPILATION_VERSION, CANON_GEO_EVIDENCE_REQUEST_VERSION,
        GeoEvidenceCompilationArtifact, GeoEvidenceCompilationRequest, GeoEvidenceDisposition,
        GeoEvidenceError, GeoRhoObservation, GeoRhoObservationKind,
        canonical_evidence_compilation_bytes, compile_evidence,
    },
    executor::{
        GEO_COMPILE_EVIDENCE_COMMAND, GEO_MATERIALIZE_EVIDENCE_COMMAND,
        GEO_MATERIALIZE_HOME_CELLS_COMMAND, GEO_PROPAGATE_OUTPUT_ID, GEO_PROPAGATE_STAGE_COMMAND,
        GEO_REQUEST_BINDING_ID, GEO_ROWS_BINDING_ID, GEO_SOLVE_COMMAND, GEO_TILE_WORK_COMMAND,
    },
    materialize::{
        CANON_GEO_H7_POPULATION_VERSION, CANON_GEO_WAREHOUSE_ROWS_VERSION, GeoH7PopulationArtifact,
        GeoH7PopulationScope, GeoH7ResultMode, GeoMaterializationError,
        GeoWarehouseBuildingParcelRow, GeoWarehouseEvidenceRow, GeoWarehouseParcelRow,
        GeoWarehouseRowsRequest, canonical_h7_population_bytes,
        canonical_materialized_evidence_request_bytes, validate_h7_population_artifact,
    },
    plan::{
        CANON_GEO_PLAN_VERSION, GeoPlan, GeoPlanArtifactRef, GeoPlanBudgetRef, GeoPlanClaimEffect,
        GeoPlanComponentScope, GeoPlanCostEstimateRange, GeoPlanExactSolveScope, GeoPlanGatePlane,
        GeoPlanGateStatus, GeoPlanGrainOutcome, GeoPlanGrainStatus, GeoPlanInventoryRef,
        GeoPlanNodeOverlay, GeoPlanPrecondition, GeoPlanProducedArtifactRef, GeoPlanProfileRef,
        GeoPlanStage, GeoPlanStatus, GeoPlanTransitionSet, geo_plan_semantic_hash,
    },
    propagate::{
        CANON_GEO_PROPAGATION_VERSION, GeoPropagationArtifact, validate_propagation_artifact,
    },
    run::{GeoRunArtifactBinding, GeoRunRequest, GeoRunStatus, run_geo_plan},
    tile::{
        CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION, CANON_GEO_HOME_CELL_ROWS_VERSION,
        CANON_GEO_TILE_WORK_REQUEST_VERSION, CANON_GEO_TILE_WORK_UNIT_VERSION, GeoHomeCellRow,
        GeoHomeCellRowsRequest, GeoTileFeatureRef, GeoTileSourceBinding, GeoTileWorkRequest,
    },
};
use crate::project::{
    ProjectExtensionDagNode, ProjectExtensionDagOutput, ProjectExtensionDagRequest,
    ProjectPlanHashRef, ProjectPlanNodeClass, ProjectPlanNodeKind,
    ProjectPlanOutputMaterialization, ProjectPlanSideEffectKind, ProjectRunFailurePolicy,
    ProjectRunPolicy, compile_extension_project_plan, digest_bytes,
};
use chrono::NaiveDate;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::Path,
};

pub const CANON_GEO_POPULATION_REQUEST_VERSION: &str = "canon_geo_population_request.v0";
pub const CANON_GEO_POPULATION_EVALUATION_VERSION: &str = "canon_geo_population_evaluation.v0";
pub const CANON_GEO_E4_GATE_ASSESSMENT_VERSION: &str = "canon_geo_e4_gate_assessment.v0";
pub const CANON_GEO_E4_RESCORE_COMPARISON_VERSION: &str = "canon_geo_e4_rescore_comparison.v0";
const CANON_GEO_POPULATION_EVIDENCE_STACK_PROOF_VERSION: &str =
    "canon_geo_population_evidence_stack.v0";
pub const CANON_GEO_FROZEN_E4_H7_CANDIDATE_TRUTH_HANDOFF_REQUEST_VERSION: &str =
    "canon_geo_frozen_e4_h7_candidate_truth_handoff_request.v0";
pub const CANON_GEO_FROZEN_E4_H7_CANDIDATE_TRUTH_EVALUATION_VERSION: &str =
    "canon_geo_frozen_e4_h7_candidate_truth_evaluation.v0";
pub const CANON_GEO_FROZEN_E4_H7_GATE_ID: &str =
    "canon_geo_frozen_e4_h7_release_validated_multi_parcel_subject_gate.v0";
pub const CANON_GEO_FROZEN_E4_H7_REQUIRED_SUBJECTS: u64 = 79;
pub const CANON_GEO_FROZEN_E4_H7_RELEASE_26V1: &str = "26v1";
pub const CANON_GEO_FROZEN_E4_H7_RELEASE_26V2: &str = "26v2";
pub const CANON_GEO_DEED_INDEX_ROWS_VERSION: &str = "canon_geo_deed_index_rows.v0";
pub const CANON_GEO_DEED_TRUTH_VERSION: &str = "canon_geo_deed_truth.v0";
pub const CANON_GEO_DEED_ROUND_AMOUNT_LATTICE_CENTS: i64 = 10_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoLabeledCompositionCase {
    pub id: String,
    pub evidence: GeoEvidenceCompilationRequest,
    pub truth_plane: GeoTruthPlane,
    /// Evaluation-only label. It is never passed to `compile_evidence` or
    /// `solve_composition`.
    pub truth: GeoCompositionModel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPopulationEvaluationRequest {
    pub version: String,
    pub cases: Vec<GeoLabeledCompositionCase>,
    pub max_cases: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPopulationCaseTruthReachByGrain {
    pub case_id: String,
    pub truth_reach_by_grain: Vec<GeoTruthReachByGrain>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCandidateTruthEvaluationRequest {
    pub version: String,
    pub population_id: String,
    pub gate: GeoCandidateTruthGate,
    pub logical_subject_bindings: Vec<GeoCandidateTruthLogicalSubjectBinding>,
    pub max_release_rows: usize,
    pub rows: Vec<GeoCandidateTruthHandoffRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCandidateTruthGate {
    pub gate_id: String,
    pub kind: GeoCandidateTruthGateKind,
    pub required_subjects: u64,
    pub required_release_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCandidateTruthGateKind {
    FrozenE4H7ReleaseValidatedMultiParcelSubjects,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCandidateTruthLogicalSubjectBinding {
    pub logical_subject_id: String,
    pub row_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCandidateTruthHandoffRow {
    pub row_id: String,
    pub subject_id: String,
    pub release_id: String,
    pub truth_plane: GeoTruthPlane,
    pub candidate_reach: GeoCandidateReachStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition_request: Option<GeoCompositionRequest>,
    pub truth: GeoCompositionModel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCandidateTruthRowStatus {
    Resolved,
    Ambiguous,
    Conflict,
    AssignmentBudgetExceeded,
    ComponentBudgetFallback,
    /// The upstream candidate handoff had no bounded candidate universe for
    /// this release row, so no solver result is fabricated.
    UpstreamNoCandidateRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCandidateTruthCaseEvaluation {
    pub row_id: String,
    pub logical_subject_id: String,
    pub subject_id: String,
    pub release_id: String,
    pub truth_plane: GeoTruthPlane,
    pub status: GeoCandidateTruthRowStatus,
    pub candidate_reach: GeoCandidateReachStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub composition_request_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solver_digest: Option<String>,
    pub candidate_members: u64,
    pub truth_members: u64,
    pub truth_parcel_members: u64,
    pub truth_building_members: u64,
    pub truth_members_in_universe: u64,
    /// Exactness is only relative to the bounded, canonicalized candidate
    /// request. It is not a candidate-reach or empirical truth claim.
    pub representation_relative_exact: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub residual_model_count: Option<u64>,
    pub residual_count_complete: bool,
    pub residual_count_saturated: bool,
    pub solver_truth_scored: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truth_model_in_residual: Option<bool>,
    pub solver_abstained: bool,
    pub claim_abstained: bool,
    pub false_merge: bool,
    /// Full-reach rows where admitted hard evidence excludes the labeled truth.
    /// This is a rho/admission finding, separate from candidate reach and from
    /// false singleton merges.
    pub rho_falsification: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCandidateTruthPlaneSummary {
    pub truth_plane: GeoTruthPlane,
    pub logical_subjects: u64,
    pub release_validated_logical_subjects: u64,
    pub frozen_e4_h7_genuine_multi_parcel_subjects: u64,
    pub release_rows: u64,
    pub candidate_reach_full_release_rows: u64,
    pub candidate_reach_partial_release_rows: u64,
    pub candidate_reach_none_release_rows: u64,
    pub solver_truth_scored_release_rows: u64,
    pub rho_falsification_release_rows: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCandidateTruthEvaluationSummary {
    pub gate: GeoCandidateTruthGate,
    pub logical_subjects: u64,
    pub release_validated_logical_subjects: u64,
    pub frozen_e4_h7_genuine_multi_parcel_subjects: u64,
    pub release_rows: u64,
    /// Count gate only, fixed to exactly 79 genuine multi-parcel H.7 subjects
    /// across the pinned 26v1/26v2 release pair. It does not claim E4 closure
    /// by itself.
    pub frozen_e4_h7_population_subject_gate_passed: bool,
    pub frozen_e4_h7_population_subject_deficit: u64,
    pub truth_planes: Vec<GeoCandidateTruthPlaneSummary>,
    pub candidate_reach_full_release_rows: u64,
    pub candidate_reach_partial_release_rows: u64,
    pub candidate_reach_none_release_rows: u64,
    pub candidate_recall_failure_release_rows: u64,
    pub solver_artifact_release_rows: u64,
    pub representation_relative_exact_release_rows: u64,
    pub solver_truth_scored_release_rows: u64,
    pub solver_truth_retained_release_rows: u64,
    pub rho_falsification_release_rows: u64,
    pub false_merge_release_rows: u64,
    pub resolved_release_rows: u64,
    pub ambiguous_release_rows: u64,
    pub conflict_release_rows: u64,
    pub assignment_budget_exceeded_release_rows: u64,
    pub component_budget_fallback_release_rows: u64,
    pub upstream_no_candidate_request_release_rows: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCandidateTruthEvaluationArtifact {
    pub version: String,
    pub request_version: String,
    pub population_id: String,
    pub summary: GeoCandidateTruthEvaluationSummary,
    pub rows: Vec<GeoCandidateTruthCaseEvaluation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDeedIndexRowsRequest {
    pub version: String,
    pub source_dataset: String,
    pub source_release: String,
    pub source_pins: Vec<GeoDeedSourcePin>,
    pub rows: Vec<GeoDeedIndexRow>,
    pub max_rows: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDeedSourcePin {
    pub source_table: String,
    pub natural_key: String,
    pub source_release: String,
    pub source_release_field: String,
    pub release_dt: String,
    pub release_dt_field: String,
    pub source_content_sha256: String,
    pub source_content_sha256_field: String,
    pub parser_version: String,
    pub parser_version_field: String,
    pub license_terms: String,
    pub license_terms_field: String,
    pub attribution_text: String,
    pub attribution_text_field: String,
    pub proof_class: GeoDeedTruthProofClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file_field: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoDeedTruthProofClass {
    Fixture,
    RetainedEvidence,
    ObservedWarehouseSnapshot,
    LiveComplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoDeedInstrumentType {
    Mortgage,
    Deed,
    Release,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDeedIndexRow {
    pub instrument_id: String,
    pub instrument_type: GeoDeedInstrumentType,
    pub parcel_ids: Vec<String>,
    pub recording_date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount_cents: Option<i64>,
    pub lender_party_blake3: String,
    pub borrower_party_blake3: String,
    pub source_release: String,
    pub row_blake3: String,
    pub source_pins: Vec<GeoDeedSourcePin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asserted_address: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDeedTruthLoanRef {
    pub loan_id: String,
    pub amount_cents: i64,
    pub origination_date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asserted_address: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoDeedTruthMatchKind {
    Unique,
    NonUniqueDiscarded,
    NoMatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDeedTruthLoanMatch {
    pub loan_id: String,
    pub matched_instrument_ids: Vec<String>,
    pub parcel_ids: Vec<String>,
    pub match_kind: GeoDeedTruthMatchKind,
    pub amount_exact: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_delta_days: Option<u32>,
    pub round_amount: bool,
    pub source_pins: Vec<GeoDeedSourcePin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDeedTruthSummary {
    pub loans: u64,
    pub unique: u64,
    pub non_unique_discarded: u64,
    pub no_match: u64,
    pub round_amount_loans: u64,
    pub round_amount_unique: u64,
    pub non_round_amount_loans: u64,
    pub non_round_amount_unique: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDeedTruthArtifact {
    pub version: String,
    pub deed_index_version: String,
    pub truth_plane: GeoTruthPlane,
    pub proof_class: GeoDeedTruthProofClass,
    pub window_days: u32,
    pub source_pins: Vec<GeoDeedSourcePin>,
    pub summary: GeoDeedTruthSummary,
    pub per_loan: Vec<GeoDeedTruthLoanMatch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPopulationCaseStatus {
    Resolved,
    Ambiguous,
    Conflict,
    AssignmentBudgetExceeded,
    /// The solver emitted a typed `BudgetFallback` for at least one
    /// constraint-connected component; no residual was guessed.
    ComponentBudgetFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoTruthPlane {
    GateV2Historical,
    NonRoundAmountDateLegalBorough,
    RoundExactLenderParty,
    AddressDerivedControl,
    DeedGrainInstrument,
    HumanAdjudication,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCandidateReachStatus {
    Full,
    Partial,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoTruthRepresentationGrain {
    UnitLot,
    BillingLot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoTruthReachByGrain {
    pub grain: GeoTruthRepresentationGrain,
    pub truth_members: u64,
    pub truth_members_in_universe: u64,
    pub candidate_reach: GeoCandidateReachStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPopulationTruthGrainSummary {
    pub grain: GeoTruthRepresentationGrain,
    pub cases: u64,
    pub truth_members: u64,
    pub truth_members_in_universe: u64,
    pub candidate_reach_full_cases: u64,
    pub candidate_reach_partial_cases: u64,
    pub candidate_reach_none_cases: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoEvidenceCoverageStatus {
    NoObservations,
    DiagnosticOnly,
    SoftPreferenceOnly,
    SoftAndDiagnosticOnly,
    HardConstraintPresent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPopulationCaseEvaluation {
    pub case_id: String,
    pub truth_plane: GeoTruthPlane,
    pub status: GeoPopulationCaseStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_claim: Option<GeoResolvedClaim>,
    pub compilation_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solver_digest: Option<String>,
    pub candidate_members: u64,
    pub truth_members: u64,
    pub truth_members_in_universe: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub truth_reach_by_grain: Vec<GeoTruthReachByGrain>,
    pub candidate_reach: GeoCandidateReachStatus,
    pub evidence_coverage: GeoEvidenceCoverageStatus,
    pub evidence_observations: u64,
    /// Total immutable source-record references attached to admitted
    /// observations. This is provenance volume only; it is not an independent
    /// information count, confidence score, or vote tally.
    pub evidence_records: u64,
    pub hard_constraint_observations: u64,
    pub soft_preference_observations: u64,
    pub diagnostic_observations: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub residual_model_count: Option<u64>,
    pub residual_count_complete: bool,
    /// Mirrors the solver's `residual_model_count_saturated`: a saturated
    /// residual is a declared lower bound, never a point estimate. Saturation
    /// of a different summary counter does not taint this claim.
    #[serde(default)]
    pub residual_count_saturated: bool,
    pub full_truth_recall: bool,
    /// Exact formula-membership check against the admitted composition
    /// request. It remains meaningful for component budget fallbacks even
    /// when residual counts and backbone completeness are unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truth_model_in_residual: Option<bool>,
    pub solver_truth_scored: bool,
    pub hard_forced: GeoCompositionBackbone,
    /// Whether `hard_forced` is the solver's complete hard backbone. A budget
    /// handoff must never be read as evidence that no member was forced.
    #[serde(default)]
    pub backbone_complete: bool,
    pub backbone_true_positive_members: u64,
    pub backbone_false_positive_members: u64,
    pub abstained: bool,
    /// True only for a resolved singleton that excludes the labeled truth model.
    /// Ambiguous backbone false positives are reported separately as backbone
    /// accuracy, not silently upgraded to a merge claim.
    pub false_merge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPopulationSummary {
    pub cases: u64,
    /// Cases admitted to the labeled population after request validation. This
    /// is the population denominator; reach failures remain eligible cases.
    pub population_eligible_cases: u64,
    pub truth_planes: Vec<GeoPopulationTruthPlaneSummary>,
    pub resolved_cases: u64,
    pub evidentially_supported_resolved_cases: u64,
    pub structurally_forced_resolved_cases: u64,
    pub resolved_with_reach_not_full_cases: u64,
    pub ambiguous_cases: u64,
    pub conflict_cases: u64,
    pub assignment_budget_exceeded_cases: u64,
    pub component_budget_fallback_cases: u64,
    pub abstention_cases: u64,
    pub false_merge_cases: u64,
    pub full_truth_recall_cases: u64,
    /// Cases for which candidate reach was evaluated. This denominator is
    /// independent of solver feasibility and empirical falsification.
    pub candidate_reach_evaluated_cases: u64,
    pub candidate_reach_full_cases: u64,
    pub candidate_reach_partial_cases: u64,
    pub candidate_reach_none_cases: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub truth_reach_by_grain: Vec<GeoPopulationTruthGrainSummary>,
    /// Cases where at least one labeled truth member was absent from the
    /// candidate universe. These are candidate-generation failures, not
    /// solver false negatives.
    pub candidate_recall_failure_cases: u64,
    pub evidence_no_observation_cases: u64,
    pub evidence_diagnostic_only_cases: u64,
    pub evidence_soft_preference_only_cases: u64,
    pub evidence_soft_and_diagnostic_only_cases: u64,
    pub evidence_hard_constraint_cases: u64,
    /// Cases whose full truth model was representable and for which solver
    /// residual membership was therefore actually scored.
    pub solver_truth_scored_cases: u64,
    /// Cases where the composition solver emitted a typed artifact. This is an
    /// artifact-emission count only; conflicts and budget fallbacks are not
    /// claimed as feasible or exact solves.
    pub solver_artifact_cases: u64,
    /// Denominator for empirical falsification: cases whose truth label was
    /// representable and scored against the admitted solver residual.
    pub empirical_falsification_eligible_cases: u64,
    /// Scored cases where admitted hard evidence excluded the labeled truth
    /// model. This is the population falsification count for the active rho
    /// contracts; it is distinct from a wrong singleton/false merge.
    pub solver_truth_exclusion_cases: u64,
    pub residual_count_complete_cases: u64,
    /// Cases whose residual count is exact, not a saturated lower bound.
    pub residual_count_exact_cases: u64,
    pub residual_count_saturated_cases: u64,
    pub residual_count_unavailable_cases: u64,
    pub backbone_complete_cases: u64,
    pub truth_members: u64,
    pub truth_members_in_universe: u64,
    pub backbone_true_positive_members: u64,
    pub backbone_false_positive_members: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPopulationTruthPlaneSummary {
    pub truth_plane: GeoTruthPlane,
    pub cases: u64,
    pub population_eligible_cases: u64,
    pub resolved_cases: u64,
    pub evidentially_supported_resolved_cases: u64,
    pub structurally_forced_resolved_cases: u64,
    pub resolved_with_reach_not_full_cases: u64,
    pub ambiguous_cases: u64,
    pub conflict_cases: u64,
    pub abstention_cases: u64,
    pub false_merge_cases: u64,
    pub candidate_reach_evaluated_cases: u64,
    pub candidate_reach_full_cases: u64,
    pub candidate_reach_partial_cases: u64,
    pub candidate_reach_none_cases: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub truth_reach_by_grain: Vec<GeoPopulationTruthGrainSummary>,
    pub solver_truth_scored_cases: u64,
    pub solver_artifact_cases: u64,
    pub empirical_falsification_eligible_cases: u64,
    pub solver_truth_exclusion_cases: u64,
    pub residual_count_complete_cases: u64,
    pub residual_count_exact_cases: u64,
    pub residual_count_saturated_cases: u64,
    pub residual_count_unavailable_cases: u64,
    pub component_budget_fallback_cases: u64,
    pub assignment_budget_exceeded_cases: u64,
    pub evidence_no_observation_cases: u64,
    pub evidence_diagnostic_only_cases: u64,
    pub evidence_soft_preference_only_cases: u64,
    pub evidence_soft_and_diagnostic_only_cases: u64,
    pub evidence_hard_constraint_cases: u64,
    pub truth_members: u64,
    pub truth_members_in_universe: u64,
    pub backbone_true_positive_members: u64,
    pub backbone_false_positive_members: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPopulationEvaluationArtifact {
    pub version: String,
    pub request_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truth_binding: Option<GeoPopulationTruthBindingSummary>,
    pub summary: GeoPopulationSummary,
    pub cases: Vec<GeoPopulationCaseEvaluation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPopulationTruthBindingSummary {
    pub truth_plane: GeoTruthPlane,
    pub source_version: String,
    pub source_proof_class: GeoDeedTruthProofClass,
    pub input_loans: u64,
    pub unique_truth_rows: u64,
    pub bound_unique_cases: u64,
    pub unique_not_in_population_cases: u64,
    pub non_unique_discarded: u64,
    pub no_match: u64,
    pub deed_truth_unbound_cases: u64,
    pub round_amount_loans: u64,
    pub round_amount_unique: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoE4GateProofClass {
    FixtureSubset,
    ObservedSnapshot,
    RetainedComplete,
    LiveComplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoE4GateProofDerivation {
    BarePopulationRequest,
    H7PopulationArtifact,
    PopulationEvidenceStackArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoE4GateProofSource {
    pub derivation: GeoE4GateProofDerivation,
    pub proof_class: GeoE4GateProofClass,
    pub source_version: String,
    pub source_blake3: String,
    pub population_request_blake3: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherited_source_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherited_source_blake3: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h7_population_scope: Option<GeoH7PopulationScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h7_result_mode: Option<GeoH7ResultMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h7_materialized_unique_accepted_loans: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h7_solver_population_subjects: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoE4GateStatus {
    Open,
    Passed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoE4GatePlane {
    Proof,
    Coverage,
    CandidateReach,
    Admission,
    SolverExactness,
    Reconciliation,
    TruthQuality,
    Cost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoE4GateBlockerCode {
    ProofClassNotLiveComplete,
    PopulationDenominatorMismatch,
    EvidenceNoObservation,
    CandidateReachIncomplete,
    SolverArtifactMissing,
    ResidualCountInexact,
    RhoFalsification,
    FalseMerge,
    AssignmentBudgetExceeded,
    ComponentBudgetFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoE4GateBlocker {
    pub plane: GeoE4GatePlane,
    pub code: GeoE4GateBlockerCode,
    pub observed: String,
    pub required: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoE4GateCaseFinding {
    pub plane: GeoE4GatePlane,
    pub code: GeoE4GateBlockerCode,
    pub case_id: String,
    pub truth_plane: GeoTruthPlane,
    pub case_status: GeoPopulationCaseStatus,
    pub observed: String,
    pub required: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoE4CoveragePlaneScore {
    pub cases: u64,
    pub population_eligible_cases: u64,
    pub evidence_no_observation_cases: u64,
    pub evidence_diagnostic_only_cases: u64,
    pub evidence_soft_preference_only_cases: u64,
    pub evidence_soft_and_diagnostic_only_cases: u64,
    pub evidence_hard_constraint_cases: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoE4CandidateReachPlaneScore {
    pub evaluated_cases: u64,
    pub full_cases: u64,
    pub partial_cases: u64,
    pub none_cases: u64,
    pub recall_failure_cases: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoE4AdmissionPlaneScore {
    pub hard_constraint_cases: u64,
    pub soft_preference_only_cases: u64,
    pub diagnostic_only_cases: u64,
    pub soft_and_diagnostic_only_cases: u64,
    pub rho_falsification_cases: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoE4SolverExactnessPlaneScore {
    pub solver_artifact_cases: u64,
    pub residual_count_complete_cases: u64,
    pub residual_count_exact_cases: u64,
    pub residual_count_saturated_cases: u64,
    pub residual_count_unavailable_cases: u64,
    pub assignment_budget_exceeded_cases: u64,
    pub component_budget_fallback_cases: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoE4ReconciliationPlaneScore {
    pub resolved_cases: u64,
    pub evidentially_supported_resolved_cases: u64,
    pub structurally_forced_resolved_cases: u64,
    pub resolved_with_reach_not_full_cases: u64,
    pub ambiguous_cases: u64,
    pub conflict_cases: u64,
    pub abstention_cases: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoE4TruthQualityPlaneScore {
    pub full_truth_recall_cases: u64,
    pub solver_truth_scored_cases: u64,
    pub solver_truth_retained_cases: u64,
    pub solver_truth_exclusion_cases: u64,
    #[serde(default)]
    pub exactly_correct_cases: u64,
    pub false_merge_cases: u64,
    pub backbone_complete_cases: u64,
    pub truth_members: u64,
    pub truth_members_in_universe: u64,
    pub backbone_true_positive_members: u64,
    pub backbone_false_positive_members: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoE4CostPlaneScore {
    pub candidate_members: u64,
    pub max_candidate_members: u64,
    pub solver_artifact_cases: u64,
    pub residual_count_complete_cases: u64,
    pub residual_count_saturated_cases: u64,
    pub residual_count_unavailable_cases: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_residual_model_count: Option<u64>,
    pub assignment_budget_exceeded_cases: u64,
    pub component_budget_fallback_cases: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoE4GatePlaneScores {
    pub coverage: GeoE4CoveragePlaneScore,
    pub candidate_reach: GeoE4CandidateReachPlaneScore,
    pub admission: GeoE4AdmissionPlaneScore,
    pub solver_exactness: GeoE4SolverExactnessPlaneScore,
    pub reconciliation: GeoE4ReconciliationPlaneScore,
    pub truth_quality: GeoE4TruthQualityPlaneScore,
    pub cost: GeoE4CostPlaneScore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoE4TruthPlaneGateAssessment {
    pub truth_plane: GeoTruthPlane,
    pub planes: GeoE4GatePlaneScores,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoE4GateAssessment {
    pub version: String,
    pub gate_id: String,
    pub proof_class: GeoE4GateProofClass,
    pub proof_source: GeoE4GateProofSource,
    pub status: GeoE4GateStatus,
    pub release_claim_allowed: bool,
    pub required_subjects: u64,
    pub evaluated_cases: u64,
    pub subject_deficit: u64,
    pub source_evaluation_blake3: String,
    pub blockers: Vec<GeoE4GateBlocker>,
    pub case_findings: Vec<GeoE4GateCaseFinding>,
    pub planes: GeoE4GatePlaneScores,
    pub truth_planes: Vec<GeoE4TruthPlaneGateAssessment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoE4RescoreMetric {
    CandidateReachFull,
    CandidateReachPartial,
    CandidateReachNone,
    Resolved,
    ExactlyCorrect,
    Ambiguous,
    Conflict,
    FalseMerges,
    TruthExclusions,
    ComponentFallbacks,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoE4RescoreSnapshot {
    pub assessment_blake3: String,
    pub proof_class: GeoE4GateProofClass,
    pub status: GeoE4GateStatus,
    pub release_claim_allowed: bool,
    pub evaluated_cases: u64,
    pub subject_deficit: u64,
    pub candidate_reach_full_cases: u64,
    pub candidate_reach_partial_cases: u64,
    pub candidate_reach_none_cases: u64,
    pub resolved_cases: u64,
    pub exactly_correct_cases: u64,
    pub ambiguous_cases: u64,
    pub conflict_cases: u64,
    pub false_merge_cases: u64,
    pub truth_exclusion_cases: u64,
    pub component_fallback_cases: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoE4RescoreComparisonRow {
    pub metric: GeoE4RescoreMetric,
    pub before: u64,
    pub after: u64,
    pub delta: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoE4RescoreInterpretation {
    pub denominator_frozen: bool,
    pub failures_remain_in_denominator: bool,
    pub candidate_universe_change_is_truth_neutral: bool,
    pub ambiguity_may_increase_when_reach_improves: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoE4RescoreComparisonArtifact {
    pub version: String,
    pub gate_id: String,
    pub required_subjects: u64,
    pub before: GeoE4RescoreSnapshot,
    pub after: GeoE4RescoreSnapshot,
    pub table: Vec<GeoE4RescoreComparisonRow>,
    pub interpretation: GeoE4RescoreInterpretation,
}

const E4_RESCORE_METRICS: [GeoE4RescoreMetric; 10] = [
    GeoE4RescoreMetric::CandidateReachFull,
    GeoE4RescoreMetric::CandidateReachPartial,
    GeoE4RescoreMetric::CandidateReachNone,
    GeoE4RescoreMetric::Resolved,
    GeoE4RescoreMetric::ExactlyCorrect,
    GeoE4RescoreMetric::Ambiguous,
    GeoE4RescoreMetric::Conflict,
    GeoE4RescoreMetric::FalseMerges,
    GeoE4RescoreMetric::TruthExclusions,
    GeoE4RescoreMetric::ComponentFallbacks,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoPopulationCaseArtifacts {
    pub case_id: String,
    pub truth_plane: GeoTruthPlane,
    pub evidence: GeoEvidenceCompilationArtifact,
    pub propagation: Option<GeoPropagationArtifact>,
    pub solve: Option<GeoCompositionArtifact>,
    pub compilation_digest: String,
    pub propagation_digest: Option<String>,
    pub solver_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoPopulationEvaluationWithArtifacts {
    pub evaluation: GeoPopulationEvaluationArtifact,
    pub case_artifacts: Vec<GeoPopulationCaseArtifacts>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPopulationErrorCode {
    UnsupportedVersion,
    InvalidInput,
    PopulationBudgetExceeded,
    Evidence,
    Composition,
    ArithmeticOverflow,
    DeedTruthNonUnique,
    DeedTruthAddressJoinDetected,
    DeedTruthPlanePooled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPopulationError {
    pub code: GeoPopulationErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoPopulationError {
    fn new(
        code: GeoPopulationErrorCode,
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            detail: detail
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }

    pub fn invalid_input(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(GeoPopulationErrorCode::InvalidInput, message, detail)
    }

    fn overflow(field: &str) -> Self {
        Self::new(
            GeoPopulationErrorCode::ArithmeticOverflow,
            "Geo population evaluation arithmetic overflowed",
            [("field", field)],
        )
    }
}

impl fmt::Display for GeoPopulationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoPopulationError {}

pub fn evaluate_population(
    request: &GeoPopulationEvaluationRequest,
) -> Result<GeoPopulationEvaluationArtifact, GeoPopulationError> {
    Ok(evaluate_population_with_artifacts(request)?.evaluation)
}

pub fn evaluate_population_with_truth_reach_by_grain(
    request: &GeoPopulationEvaluationRequest,
    truth_reach_by_case: &[GeoPopulationCaseTruthReachByGrain],
) -> Result<GeoPopulationEvaluationArtifact, GeoPopulationError> {
    let truth_reach_by_case = canonical_truth_reach_overlay_map(truth_reach_by_case)?;
    Ok(
        evaluate_population_with_case_executor(request, execute_case_direct, &truth_reach_by_case)?
            .evaluation,
    )
}

pub fn evaluate_population_with_artifacts(
    request: &GeoPopulationEvaluationRequest,
) -> Result<GeoPopulationEvaluationWithArtifacts, GeoPopulationError> {
    let truth_reach_by_case = BTreeMap::new();
    evaluate_population_with_case_executor(request, execute_case_direct, &truth_reach_by_case)
}

pub fn evaluate_population_with_run_artifacts(
    request: &GeoPopulationEvaluationRequest,
    workspace_root: impl AsRef<Path>,
) -> Result<GeoPopulationEvaluationWithArtifacts, GeoPopulationError> {
    let workspace_root = workspace_root.as_ref();
    let truth_reach_by_case = BTreeMap::new();
    evaluate_population_with_case_executor(
        request,
        |case| execute_case_through_geo_run(case, workspace_root),
        &truth_reach_by_case,
    )
}

enum GeoPopulationCaseSolveOutcome {
    Solved(Box<GeoCompositionArtifact>),
    AssignmentBudgetExceeded,
}

struct GeoPopulationCaseExecution {
    evidence: GeoEvidenceCompilationArtifact,
    propagation: Option<GeoPropagationArtifact>,
    solve: GeoPopulationCaseSolveOutcome,
    compilation_digest: String,
    propagation_digest: Option<String>,
    solver_digest: Option<String>,
}

fn evaluate_population_with_case_executor<F>(
    request: &GeoPopulationEvaluationRequest,
    mut execute_case: F,
    truth_reach_by_case: &BTreeMap<String, Vec<GeoTruthReachByGrain>>,
) -> Result<GeoPopulationEvaluationWithArtifacts, GeoPopulationError>
where
    F: FnMut(&GeoLabeledCompositionCase) -> Result<GeoPopulationCaseExecution, GeoPopulationError>,
{
    if request.version != CANON_GEO_POPULATION_REQUEST_VERSION {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo population request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_POPULATION_REQUEST_VERSION),
            ],
        ));
    }
    if request.max_cases == 0 || request.cases.len() > request.max_cases {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::PopulationBudgetExceeded,
            "Geo population exceeds the declared case budget",
            [
                ("cases", request.cases.len().to_string()),
                ("max_cases", request.max_cases.to_string()),
            ],
        ));
    }

    let mut cases = request.cases.clone();
    for case in &mut cases {
        validate_case(case)?;
    }
    cases.sort_by(|left, right| left.id.cmp(&right.id));
    for pair in cases.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo population contains a duplicate case identifier",
                [("case_id", pair[0].id.as_str())],
            ));
        }
    }
    validate_deed_truth_plane_scope(cases.iter().map(|case| case.truth_plane))?;
    validate_truth_reach_overlay_case_ids(truth_reach_by_case, &cases)?;

    let mut evaluations = Vec::with_capacity(cases.len());
    let mut case_artifacts = Vec::with_capacity(cases.len());
    for case in cases {
        // Deliberately compile and solve before reading `case.truth`.
        let case_id = case.id.clone();
        let truth_plane = case.truth_plane;
        let execution = execute_case(&case)?;
        let compilation = execution.evidence;

        let universe = &compilation.composition_request.universe;
        let candidate_members = checked_member_count(
            universe.parcels.len(),
            universe.buildings.len(),
            "candidate_members",
        )?;
        let truth_members = checked_member_count(
            case.truth.parcels.len(),
            case.truth.buildings.len(),
            "truth_members",
        )?;
        let truth_members_in_universe = count_truth_in_universe(&case.truth, universe)?;
        let full_truth_recall = truth_members == truth_members_in_universe;
        let candidate_reach = candidate_reach_status(truth_members, truth_members_in_universe)?;
        let truth_reach_by_grain = truth_reach_by_case
            .get(&case_id)
            .cloned()
            .unwrap_or_default();
        let evidence_metrics = evidence_metrics(&compilation.admissions)?;

        let (evaluation, solve_artifact) = match execution.solve {
            GeoPopulationCaseSolveOutcome::AssignmentBudgetExceeded => (
                GeoPopulationCaseEvaluation {
                    case_id: case.id,
                    truth_plane: case.truth_plane,
                    status: GeoPopulationCaseStatus::AssignmentBudgetExceeded,
                    resolved_claim: None,
                    compilation_digest: execution.compilation_digest.clone(),
                    solver_digest: None,
                    candidate_members,
                    truth_members,
                    truth_members_in_universe,
                    truth_reach_by_grain: truth_reach_by_grain.clone(),
                    candidate_reach,
                    evidence_coverage: evidence_metrics.coverage,
                    evidence_observations: evidence_metrics.observations,
                    evidence_records: evidence_metrics.records,
                    hard_constraint_observations: evidence_metrics.hard_constraints,
                    soft_preference_observations: evidence_metrics.soft_preferences,
                    diagnostic_observations: evidence_metrics.diagnostic_observations,
                    full_truth_recall,
                    residual_model_count: None,
                    residual_count_complete: false,
                    residual_count_saturated: false,
                    truth_model_in_residual: None,
                    solver_truth_scored: false,
                    hard_forced: empty_backbone(),
                    backbone_complete: false,
                    backbone_true_positive_members: 0,
                    backbone_false_positive_members: 0,
                    abstained: true,
                    false_merge: false,
                },
                None,
            ),
            GeoPopulationCaseSolveOutcome::Solved(artifact) => {
                let solver_digest = execution.solver_digest.clone().ok_or_else(|| {
                    GeoPopulationError::new(
                        GeoPopulationErrorCode::Composition,
                        "Geo population case solved without a solver digest",
                        [("case_id", case_id.as_str())],
                    )
                })?;
                let solver_truth_scored = full_truth_recall;
                let (backbone_true, backbone_false) =
                    if solver_truth_scored && artifact.backbone_complete {
                        score_backbone(&artifact.hard_forced, &case.truth)?
                    } else {
                        (0, 0)
                    };
                let truth_model_in_residual = if solver_truth_scored {
                    Some(
                        match model_satisfies_request(&compilation.composition_request, &case.truth)
                        {
                            Ok(satisfied) => satisfied,
                            Err(error) => return Err(map_composition_error(error)),
                        },
                    )
                } else {
                    None
                };
                let status = match artifact.status {
                    GeoCompositionStatus::Resolved => GeoPopulationCaseStatus::Resolved,
                    GeoCompositionStatus::Ambiguous => GeoPopulationCaseStatus::Ambiguous,
                    GeoCompositionStatus::Conflict => GeoPopulationCaseStatus::Conflict,
                    GeoCompositionStatus::BudgetFallback => {
                        GeoPopulationCaseStatus::ComponentBudgetFallback
                    }
                };
                let resolved_claim = resolved_claim_from_artifact(
                    &artifact,
                    compilation.composition_request.hard_constraints.len(),
                );
                let residual_count_complete = artifact.summary.residual_model_count_complete;
                let false_merge = scored_false_merge(status, truth_model_in_residual);
                let evaluation = GeoPopulationCaseEvaluation {
                    case_id: case.id,
                    truth_plane: case.truth_plane,
                    status,
                    resolved_claim,
                    residual_count_saturated: artifact.summary.residual_model_count_saturated,
                    compilation_digest: execution.compilation_digest.clone(),
                    solver_digest: Some(solver_digest.clone()),
                    candidate_members,
                    truth_members,
                    truth_members_in_universe,
                    truth_reach_by_grain: truth_reach_by_grain.clone(),
                    candidate_reach,
                    evidence_coverage: evidence_metrics.coverage,
                    evidence_observations: evidence_metrics.observations,
                    evidence_records: evidence_metrics.records,
                    hard_constraint_observations: evidence_metrics.hard_constraints,
                    soft_preference_observations: evidence_metrics.soft_preferences,
                    diagnostic_observations: evidence_metrics.diagnostic_observations,
                    full_truth_recall,
                    residual_model_count: artifact
                        .summary
                        .residual_model_count_complete
                        .then_some(artifact.summary.residual_model_count),
                    residual_count_complete,
                    truth_model_in_residual,
                    solver_truth_scored,
                    hard_forced: artifact.hard_forced.clone(),
                    backbone_complete: artifact.backbone_complete,
                    backbone_true_positive_members: backbone_true,
                    backbone_false_positive_members: backbone_false,
                    abstained: is_abstention_status(status),
                    false_merge,
                };
                (evaluation, Some(*artifact))
            }
        };
        validate_case_evaluation(&evaluation)?;
        case_artifacts.push(GeoPopulationCaseArtifacts {
            case_id,
            truth_plane,
            evidence: compilation,
            propagation: execution.propagation,
            solve: solve_artifact,
            compilation_digest: execution.compilation_digest,
            propagation_digest: execution.propagation_digest,
            solver_digest: evaluation.solver_digest.clone(),
        });
        evaluations.push(evaluation);
    }

    let summary = summarize(&evaluations)?;
    Ok(GeoPopulationEvaluationWithArtifacts {
        evaluation: GeoPopulationEvaluationArtifact {
            version: CANON_GEO_POPULATION_EVALUATION_VERSION.to_string(),
            request_version: request.version.clone(),
            truth_binding: None,
            summary,
            cases: evaluations,
        },
        case_artifacts,
    })
}

fn execute_case_direct(
    case: &GeoLabeledCompositionCase,
) -> Result<GeoPopulationCaseExecution, GeoPopulationError> {
    let compilation = compile_evidence(&case.evidence).map_err(map_evidence_error)?;
    let compilation_digest = digest_evidence_compilation(&compilation)?;
    match solve_composition(&compilation.composition_request) {
        Err(error) if error.code == GeoCompositionErrorCode::BudgetExceeded => {
            Ok(GeoPopulationCaseExecution {
                evidence: compilation,
                propagation: None,
                solve: GeoPopulationCaseSolveOutcome::AssignmentBudgetExceeded,
                compilation_digest,
                propagation_digest: None,
                solver_digest: None,
            })
        }
        Err(error) => Err(map_composition_error(error)),
        Ok(artifact) => {
            let solver_digest = digest_composition(&artifact)?;
            Ok(GeoPopulationCaseExecution {
                evidence: compilation,
                propagation: None,
                solve: GeoPopulationCaseSolveOutcome::Solved(Box::new(artifact)),
                compilation_digest,
                propagation_digest: None,
                solver_digest: Some(solver_digest),
            })
        }
    }
}

fn execute_case_through_geo_run(
    case: &GeoLabeledCompositionCase,
    workspace_root: &Path,
) -> Result<GeoPopulationCaseExecution, GeoPopulationError> {
    let expected_compilation = compile_evidence(&case.evidence).map_err(map_evidence_error)?;
    let expected_compilation_bytes = canonical_evidence_compilation_bytes(&expected_compilation)
        .map_err(|error| {
            GeoPopulationError::new(
                GeoPopulationErrorCode::Composition,
                "Geo evidence compilation could not be serialized",
                [("case_id", case.id.clone()), ("error", error.to_string())],
            )
        })?;
    let compilation_digest = blake3::hash(&expected_compilation_bytes)
        .to_hex()
        .to_string();
    let case_workspace = workspace_root.join(case_workspace_stem(&case.id));
    std::fs::create_dir_all(&case_workspace).map_err(|error| {
        GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo population evaluate run workspace could not be created",
            [
                ("case_id", case.id.clone()),
                ("workspace", case_workspace.display().to_string()),
                ("error", error.to_string()),
            ],
        )
    })?;
    let level = selected_case_control_level(&case.evidence.profile.selection_level)?;
    let plan = case_geo_run_plan(&case.id, level, &case.evidence)?;
    let bindings = case_geo_run_bindings(&case.id, level, &case.evidence)?;
    let mut policy = ProjectRunPolicy::new(&case_workspace, "work");
    policy.failure_policy = ProjectRunFailurePolicy::FailFast;

    let run = run_geo_plan(GeoRunRequest::new(plan, policy, bindings))
        .map_err(|error| map_geo_run_error(&case.id, error))?;
    // Abstained, Contradicted, and BudgetFallback are solve outcomes that the
    // run reports from the composition artifact's status; the solve artifact
    // exists for each of them and the evaluation scores them like the direct path.
    if !matches!(
        run.status,
        GeoRunStatus::Completed
            | GeoRunStatus::Abstained
            | GeoRunStatus::Contradicted
            | GeoRunStatus::BudgetFallback
    ) {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo population evaluate run path did not complete",
            [
                ("case_id", case.id.clone()),
                ("status", format!("{:?}", run.status)),
            ],
        ));
    }

    let run_prefix = level_run_prefix(level);
    let materialized_bytes = read_run_artifact_bytes(
        &case_workspace,
        &format!("geo/{run_prefix}/materialize_evidence.json"),
        &case.id,
        "materialize_evidence",
    )?;
    if materialized_bytes
        != canonical_materialized_evidence_request_bytes(&case.evidence).map_err(|error| {
            GeoPopulationError::new(
                GeoPopulationErrorCode::Composition,
                "Geo materialized evidence request could not be serialized",
                [("case_id", case.id.clone()), ("error", error.to_string())],
            )
        })?
    {
        return Err(GeoPopulationError::invalid_input(
            "Geo evaluate run path materialized evidence changed the case request",
            [("case_id", case.id.as_str())],
        ));
    }

    let compilation_bytes = read_run_artifact_bytes(
        &case_workspace,
        &format!("geo/{run_prefix}/compile_evidence.json"),
        &case.id,
        "compile_evidence",
    )?;
    if compilation_bytes != expected_compilation_bytes {
        return Err(GeoPopulationError::invalid_input(
            "Geo evaluate run path compiled evidence does not match the direct evidence compiler",
            [("case_id", case.id.as_str())],
        ));
    }
    let compilation = parse_run_artifact::<GeoEvidenceCompilationArtifact>(
        &compilation_bytes,
        &case.id,
        "compile_evidence",
    )?;

    let propagation_bytes = read_run_artifact_bytes(
        &case_workspace,
        &format!("geo/{run_prefix}/propagation.json"),
        &case.id,
        "propagation",
    )?;
    let propagation =
        parse_run_artifact::<GeoPropagationArtifact>(&propagation_bytes, &case.id, "propagation")?;
    validate_propagation_artifact(&propagation).map_err(|error| {
        GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo evaluate run path emitted an invalid propagation artifact",
            [("case_id", case.id.clone()), ("error", error.to_string())],
        )
    })?;
    let propagation_digest = blake3::hash(&propagation_bytes).to_hex().to_string();

    let solve_bytes = read_run_artifact_bytes(
        &case_workspace,
        &format!("geo/{run_prefix}/solve.json"),
        &case.id,
        "solve",
    )?;
    let solve = parse_run_artifact::<GeoCompositionArtifact>(&solve_bytes, &case.id, "solve")?;
    let solver_digest = blake3::hash(&solve_bytes).to_hex().to_string();

    Ok(GeoPopulationCaseExecution {
        evidence: compilation,
        propagation: Some(propagation),
        solve: GeoPopulationCaseSolveOutcome::Solved(Box::new(solve)),
        compilation_digest,
        propagation_digest: Some(propagation_digest),
        solver_digest: Some(solver_digest),
    })
}

fn digest_evidence_compilation(
    artifact: &GeoEvidenceCompilationArtifact,
) -> Result<String, GeoPopulationError> {
    Ok(blake3::hash(
        &canonical_evidence_compilation_bytes(artifact).map_err(|error| {
            GeoPopulationError::new(
                GeoPopulationErrorCode::Composition,
                "Geo evidence compilation could not be serialized",
                [("error", error.to_string())],
            )
        })?,
    )
    .to_hex()
    .to_string())
}

fn digest_composition(artifact: &GeoCompositionArtifact) -> Result<String, GeoPopulationError> {
    Ok(
        blake3::hash(&canonical_composition_bytes(artifact).map_err(|error| {
            GeoPopulationError::new(
                GeoPopulationErrorCode::Composition,
                "Geo composition artifact could not be serialized",
                [("error", error.to_string())],
            )
        })?)
        .to_hex()
        .to_string(),
    )
}

fn case_workspace_stem(case_id: &str) -> String {
    format!("case-{}", blake3::hash(case_id.as_bytes()).to_hex())
}

fn selected_case_control_level(
    selection_level: &GeoEntityLevel,
) -> Result<GeoControlEntityLevel, GeoPopulationError> {
    match selection_level {
        GeoEntityLevel::Parcel => Ok(GeoControlEntityLevel::Parcel),
        GeoEntityLevel::Building => Ok(GeoControlEntityLevel::Building),
        other => Err(GeoPopulationError::invalid_input(
            "Geo evaluate run path supports only parcel or building selected grains",
            [("selection_level", format!("{other:?}"))],
        )),
    }
}

fn level_run_prefix(level: GeoControlEntityLevel) -> &'static str {
    match level {
        GeoControlEntityLevel::Parcel => "parcel",
        GeoControlEntityLevel::Building => "building",
        GeoControlEntityLevel::Address
        | GeoControlEntityLevel::Poi
        | GeoControlEntityLevel::Property
        | GeoControlEntityLevel::Site
        | GeoControlEntityLevel::Unit => "unsupported",
    }
}

fn case_geo_run_bindings(
    case_id: &str,
    level: GeoControlEntityLevel,
    evidence: &GeoEvidenceCompilationRequest,
) -> Result<Vec<GeoRunArtifactBinding>, GeoPopulationError> {
    let prefix = format!("geo.{}", level_run_prefix(level));
    let source = evaluation_section_source(case_id, level, evidence)?;
    let selected_ids = selected_feature_ids(level, evidence);
    if selected_ids.is_empty() {
        return Err(GeoPopulationError::invalid_input(
            "Geo evaluate run path requires a nonempty candidate universe",
            [("case_id", case_id.to_string())],
        ));
    }
    let home_rows = GeoHomeCellRowsRequest {
        version: CANON_GEO_HOME_CELL_ROWS_VERSION.to_string(),
        coordinate_crs: "EPSG:4326".to_string(),
        coordinate_decimal_places: 9,
        h3_resolution: 9,
        stability_radius_fixed: 1_000,
        rows: selected_ids
            .iter()
            .map(|feature_id| GeoHomeCellRow {
                source: source.clone(),
                feature_id: feature_id.clone(),
                source_record_id: format!(
                    "eval-section-row:{}",
                    blake3::hash(format!("{case_id}\0{feature_id}").as_bytes()).to_hex()
                ),
                geometry_sha256: "5ed87d37d872789086452c35f658f5628ba870ca36072c495bb88519592403ed"
                    .to_string(),
                representative_point_method: "declared_candidate_universe_mirror".to_string(),
                longitude: "-73.977264000".to_string(),
                latitude: "40.753429000".to_string(),
                transform_execution_id: Some("canon-geo-evaluate-run-section-mirror".to_string()),
                transform_definition_id: Some(
                    "canon-geo-evaluate-run-section-mirror.v0".to_string(),
                ),
                claimed_home_cell: Some("892a100d62bffff".to_string()),
            })
            .collect(),
        max_rows: selected_ids.len() as u64,
    };
    let features = selected_ids
        .iter()
        .map(|feature_id| GeoTileFeatureRef {
            source: source.clone(),
            feature_id: feature_id.clone(),
            home_cell: "892a100d62bffff".to_string(),
        })
        .collect::<Vec<_>>();
    let tile_request = GeoTileWorkRequest {
        version: CANON_GEO_TILE_WORK_REQUEST_VERSION.to_string(),
        center_cell: "892a100d62bffff".to_string(),
        halo_k: 0,
        features,
        candidate_reach_reference: None,
        max_features: selected_ids.len() as u64,
        max_work_cells: 1,
    };
    let warehouse_rows = warehouse_rows_from_evidence(evidence);
    Ok(vec![
        json_binding(
            &format!("{prefix}.home_cells"),
            GEO_ROWS_BINDING_ID,
            CANON_GEO_HOME_CELL_ROWS_VERSION,
            &home_rows,
        )?,
        json_binding(
            &format!("{prefix}.section"),
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_TILE_WORK_REQUEST_VERSION,
            &tile_request,
        )?,
        json_binding(
            &format!("{prefix}.materialize_evidence"),
            GEO_ROWS_BINDING_ID,
            CANON_GEO_WAREHOUSE_ROWS_VERSION,
            &warehouse_rows,
        )?,
    ])
}

fn json_binding<T: Serialize>(
    node_id: &str,
    binding_id: &str,
    contract_version: &str,
    value: &T,
) -> Result<GeoRunArtifactBinding, GeoPopulationError> {
    GeoRunArtifactBinding::from_json(node_id, binding_id, contract_version, value).map_err(
        |error| {
            GeoPopulationError::new(
                GeoPopulationErrorCode::Composition,
                "Geo evaluate run input binding could not be serialized",
                [
                    ("node_id", node_id.to_string()),
                    ("binding_id", binding_id.to_string()),
                    ("error", error.to_string()),
                ],
            )
        },
    )
}

fn selected_feature_ids(
    level: GeoControlEntityLevel,
    evidence: &GeoEvidenceCompilationRequest,
) -> Vec<String> {
    let mut ids = match level {
        GeoControlEntityLevel::Parcel => evidence.universe.parcels.clone(),
        GeoControlEntityLevel::Building => evidence
            .universe
            .buildings
            .iter()
            .map(|building| building.id.clone())
            .collect(),
        GeoControlEntityLevel::Address
        | GeoControlEntityLevel::Poi
        | GeoControlEntityLevel::Property
        | GeoControlEntityLevel::Site
        | GeoControlEntityLevel::Unit => Vec::new(),
    };
    ids.sort();
    ids.dedup();
    ids
}

fn warehouse_rows_from_evidence(
    evidence: &GeoEvidenceCompilationRequest,
) -> GeoWarehouseRowsRequest {
    let parcel_rows = evidence
        .universe
        .parcels
        .iter()
        .map(|parcel_id| GeoWarehouseParcelRow {
            parcel_id: parcel_id.clone(),
        })
        .collect();
    let mut building_parcel_rows = Vec::new();
    for building in &evidence.universe.buildings {
        if building.parcel_ids.is_empty() {
            building_parcel_rows.push(GeoWarehouseBuildingParcelRow {
                building_id: building.id.clone(),
                parcel_id: None,
            });
        } else {
            for parcel_id in &building.parcel_ids {
                building_parcel_rows.push(GeoWarehouseBuildingParcelRow {
                    building_id: building.id.clone(),
                    parcel_id: Some(parcel_id.clone()),
                });
            }
        }
    }
    let evidence_rows = evidence
        .observations
        .iter()
        .flat_map(|observation| {
            observation
                .source_records
                .iter()
                .map(move |source_record| GeoWarehouseEvidenceRow {
                    observation_id: observation.id.clone(),
                    contract_id: observation.contract_id.clone(),
                    source_record: source_record.clone(),
                    valid_time: observation.valid_time,
                    observation: observation.observation.clone(),
                })
        })
        .collect();
    GeoWarehouseRowsRequest {
        version: CANON_GEO_WAREHOUSE_ROWS_VERSION.to_string(),
        profile: evidence.profile.clone(),
        parcel_rows,
        building_parcel_rows,
        contracts: evidence.contracts.clone(),
        evidence_rows,
        max_assignments: evidence.max_assignments,
        max_materialized_models: evidence.max_materialized_models,
    }
}

fn evaluation_section_source(
    case_id: &str,
    level: GeoControlEntityLevel,
    evidence: &GeoEvidenceCompilationRequest,
) -> Result<GeoTileSourceBinding, GeoPopulationError> {
    let universe_bytes = serde_json::to_vec(&evidence.universe).map_err(|error| {
        GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo evaluate run section source could not serialize the declared universe",
            [
                ("case_id", case_id.to_string()),
                ("error", error.to_string()),
            ],
        )
    })?;
    let release_digest = digest_bytes(&universe_bytes);
    Ok(GeoTileSourceBinding {
        source_instance_id: format!(
            "canon.geo.evaluate.declared_{}_universe",
            level_run_prefix(level)
        ),
        release: GeoSourceRelease {
            release_id: "declared-candidate-universe".to_string(),
            release_digest: release_digest.clone(),
        },
        native_scope: GeoNativeEntityScope::NativeEntity {
            entity_level: level,
            identity_participation: GeoIdentityParticipation::EvidenceOnly,
        },
        inventory_ref: GeoPlanInventoryRef {
            inventory_id: "canon.geo.evaluate.declared_candidate_universe".to_string(),
            semantic_hash: digest_bytes(format!("semantic\0{case_id}").as_bytes()),
            planning_hash: release_digest,
        },
    })
}

fn case_geo_run_plan(
    case_id: &str,
    level: GeoControlEntityLevel,
    evidence: &GeoEvidenceCompilationRequest,
) -> Result<GeoPlan, GeoPopulationError> {
    let prefix = format!("geo.{}", level_run_prefix(level));
    let manifest_digest = digest_bytes(format!("geo-evaluate-manifest\0{case_id}").as_bytes());
    let lock_digest = digest_bytes(format!("geo-evaluate-lock\0{case_id}").as_bytes());
    let bounds = case_deterministic_bounds(evidence);
    let limits = bounds
        .iter()
        .map(|bound| (bound.semantic_id.clone(), bound.value))
        .collect::<BTreeMap<_, _>>();
    let project_nodes = vec![
        extension_node(
            ExtensionNodeSpec {
                node_id: &format!("{prefix}.home_cells"),
                kind: ProjectPlanNodeKind::Normalize,
                command: GEO_MATERIALIZE_HOME_CELLS_COMMAND,
                dependencies: Vec::new(),
                output_id: "home_cells",
                path: &format!("geo/{}/home_cells.json", level_run_prefix(level)),
                content_hash_inputs: vec![ProjectPlanHashRef {
                    ref_id: "geo.evaluate.case".to_string(),
                    content_hash: digest_bytes(case_id.as_bytes()),
                }],
            },
            &limits,
        ),
        extension_node(
            ExtensionNodeSpec {
                node_id: &format!("{prefix}.section"),
                kind: ProjectPlanNodeKind::Block,
                command: GEO_TILE_WORK_COMMAND,
                dependencies: vec![format!("{prefix}.home_cells")],
                output_id: "section",
                path: &format!("geo/{}/section.json", level_run_prefix(level)),
                content_hash_inputs: Vec::new(),
            },
            &limits,
        ),
        extension_node(
            ExtensionNodeSpec {
                node_id: &format!("{prefix}.materialize_evidence"),
                kind: ProjectPlanNodeKind::Evidence,
                command: GEO_MATERIALIZE_EVIDENCE_COMMAND,
                dependencies: vec![format!("{prefix}.section")],
                output_id: "materialize_evidence",
                path: &format!("geo/{}/materialize_evidence.json", level_run_prefix(level)),
                content_hash_inputs: Vec::new(),
            },
            &limits,
        ),
        extension_node(
            ExtensionNodeSpec {
                node_id: &format!("{prefix}.compile_evidence"),
                kind: ProjectPlanNodeKind::Evidence,
                command: GEO_COMPILE_EVIDENCE_COMMAND,
                dependencies: vec![format!("{prefix}.materialize_evidence")],
                output_id: "compile_evidence",
                path: &format!("geo/{}/compile_evidence.json", level_run_prefix(level)),
                content_hash_inputs: Vec::new(),
            },
            &limits,
        ),
        extension_node(
            ExtensionNodeSpec {
                node_id: &format!("{prefix}.propagate"),
                kind: ProjectPlanNodeKind::Solve,
                command: GEO_PROPAGATE_STAGE_COMMAND,
                dependencies: vec![format!("{prefix}.compile_evidence")],
                output_id: GEO_PROPAGATE_OUTPUT_ID,
                path: &format!("geo/{}/propagation.json", level_run_prefix(level)),
                content_hash_inputs: Vec::new(),
            },
            &limits,
        ),
        extension_node(
            ExtensionNodeSpec {
                node_id: &format!("{prefix}.solve"),
                kind: ProjectPlanNodeKind::Solve,
                command: GEO_SOLVE_COMMAND,
                dependencies: vec![
                    format!("{prefix}.compile_evidence"),
                    format!("{prefix}.propagate"),
                    format!("{prefix}.section"),
                ],
                output_id: "solve",
                path: &format!("geo/{}/solve.json", level_run_prefix(level)),
                content_hash_inputs: Vec::new(),
            },
            &limits,
        ),
    ];
    let project_plan =
        compile_extension_project_plan(ProjectExtensionDagRequest::offline_read_only(
            format!("geo-evaluate-{}", blake3::hash(case_id.as_bytes()).to_hex()),
            manifest_digest,
            lock_digest,
            project_nodes,
        ))
        .map_err(|error| {
            GeoPopulationError::new(
                GeoPopulationErrorCode::Composition,
                "Geo evaluate run path could not compile its project DAG",
                [
                    ("case_id", case_id.to_string()),
                    ("project_error", format!("{:?}", error.code)),
                    ("message", error.message),
                ],
            )
        })?;
    let node_ids = [
        "home_cells",
        "section",
        "materialize_evidence",
        "compile_evidence",
        "propagate",
        "solve",
    ]
    .iter()
    .map(|suffix| format!("{prefix}.{suffix}"))
    .collect::<Vec<_>>();
    let overlays = vec![
        overlay_node(
            OverlayNodeSpec {
                project_node_id: &node_ids[0],
                stage: GeoPlanStage::MaterializeHomeCells,
                level,
                expected_output_contract: CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION,
                bounded_section_required: false,
                incidence_factorization_required: false,
                exact_solve_scope: None,
            },
            bounds.clone(),
        ),
        overlay_node(
            OverlayNodeSpec {
                project_node_id: &node_ids[1],
                stage: GeoPlanStage::BuildBoundedSection,
                level,
                expected_output_contract: CANON_GEO_TILE_WORK_UNIT_VERSION,
                bounded_section_required: false,
                incidence_factorization_required: false,
                exact_solve_scope: None,
            },
            bounds.clone(),
        ),
        overlay_node(
            OverlayNodeSpec {
                project_node_id: &node_ids[2],
                stage: GeoPlanStage::MaterializeEvidence,
                level,
                expected_output_contract: CANON_GEO_EVIDENCE_REQUEST_VERSION,
                bounded_section_required: false,
                incidence_factorization_required: false,
                exact_solve_scope: None,
            },
            bounds.clone(),
        ),
        overlay_node(
            OverlayNodeSpec {
                project_node_id: &node_ids[3],
                stage: GeoPlanStage::CompileEvidence,
                level,
                expected_output_contract: CANON_GEO_EVIDENCE_COMPILATION_VERSION,
                bounded_section_required: false,
                incidence_factorization_required: false,
                exact_solve_scope: None,
            },
            bounds.clone(),
        ),
        overlay_node(
            OverlayNodeSpec {
                project_node_id: &node_ids[4],
                stage: GeoPlanStage::PropagateConstraints,
                level,
                expected_output_contract: CANON_GEO_PROPAGATION_VERSION,
                bounded_section_required: false,
                incidence_factorization_required: false,
                exact_solve_scope: None,
            },
            bounds.clone(),
        ),
        overlay_node(
            OverlayNodeSpec {
                project_node_id: &node_ids[5],
                stage: GeoPlanStage::FactorAndSolveExactResidual,
                level,
                expected_output_contract: CANON_GEO_COMPOSITION_VERSION,
                bounded_section_required: true,
                incidence_factorization_required: true,
                exact_solve_scope: Some(GeoPlanExactSolveScope {
                    bounded_section: GeoPlanProducedArtifactRef {
                        producer_node_id: node_ids[1].clone(),
                        output_id: "section".to_string(),
                        output_contract: CANON_GEO_TILE_WORK_UNIT_VERSION.to_string(),
                    },
                    evidence_compilation: GeoPlanProducedArtifactRef {
                        producer_node_id: node_ids[3].clone(),
                        output_id: "compile_evidence".to_string(),
                        output_contract: CANON_GEO_EVIDENCE_COMPILATION_VERSION.to_string(),
                    },
                    component_scope:
                        GeoPlanComponentScope::ActualConnectedComponentsOfCompiledConstraintIncidence,
                    component_key_field: "canon_geo_composition.v0.factorization[].key".to_string(),
                }),
            },
            bounds.clone(),
        ),
    ];
    let profile_hash = digest_bytes(&serde_json::to_vec(&evidence.profile).map_err(|error| {
        GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo evaluate run path could not serialize the composition profile",
            [
                ("case_id", case_id.to_string()),
                ("error", error.to_string()),
            ],
        )
    })?);
    let mut plan = GeoPlan {
        version: CANON_GEO_PLAN_VERSION.to_string(),
        plan_id: String::new(),
        semantic_hash: String::new(),
        status: GeoPlanStatus::Planned,
        question_ref: GeoPlanArtifactRef {
            artifact_id: format!("geo-evaluate-question:{case_id}"),
            semantic_hash: digest_bytes(format!("question\0{case_id}").as_bytes()),
        },
        capabilities_ref: GeoPlanArtifactRef {
            artifact_id: "canon-geo-evaluate-run-internal".to_string(),
            semantic_hash: digest_bytes(b"canon-geo-evaluate-run-internal-capabilities"),
        },
        inventory_ref: GeoPlanInventoryRef {
            inventory_id: "canon.geo.evaluate.declared_candidate_universe".to_string(),
            semantic_hash: digest_bytes(format!("inventory\0{case_id}").as_bytes()),
            planning_hash: digest_bytes(format!("inventory-planning\0{case_id}").as_bytes()),
        },
        profile_ref: GeoPlanProfileRef {
            version: evidence.profile.version.clone(),
            selection_level: evidence.profile.selection_level,
            semantic_hash: profile_hash,
        },
        budget_ref: GeoPlanBudgetRef {
            budget_id: "geo-evaluate-run-internal-budget".to_string(),
            semantic_hash: digest_bytes(format!("budget\0{case_id}").as_bytes()),
            planning_hash: digest_bytes(format!("budget-planning\0{case_id}").as_bytes()),
        },
        project_plan,
        geo_nodes: overlays,
        grain_outcomes: vec![GeoPlanGrainOutcome {
            entity_level: level,
            status: GeoPlanGrainStatus::PlannedRelativeToDeclaredUniverse,
            missing_evidence_classes: Vec::new(),
            project_node_ids: node_ids,
            claim_limitation: "evaluation run consumes the case's declared candidate universe; candidate reach remains the population artifact's independent truth-plane metric".to_string(),
            next_action: "read the emitted evidence, propagation, and solve artifacts".to_string(),
        }],
        external_requests: Vec::new(),
        diagnostics: vec![
            "geo evaluate internal run uses a declared-candidate-universe section mirror; it does not create new source reach evidence".to_string(),
        ],
    };
    let semantic_hash = geo_plan_semantic_hash(&plan).map_err(|error| {
        GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo evaluate run plan could not compute its semantic hash",
            [
                ("case_id", case_id.to_string()),
                ("plan_error", format!("{:?}", error.code)),
                ("message", error.message),
            ],
        )
    })?;
    plan.plan_id = format!(
        "{CANON_GEO_PLAN_VERSION}:{}",
        semantic_hash.trim_start_matches("blake3:")
    );
    plan.semantic_hash = semantic_hash;
    Ok(plan)
}

struct ExtensionNodeSpec<'a> {
    node_id: &'a str,
    kind: ProjectPlanNodeKind,
    command: &'a str,
    dependencies: Vec<String>,
    output_id: &'a str,
    path: &'a str,
    content_hash_inputs: Vec<ProjectPlanHashRef>,
}

fn extension_node(
    spec: ExtensionNodeSpec<'_>,
    limits: &BTreeMap<String, u64>,
) -> ProjectExtensionDagNode {
    ProjectExtensionDagNode {
        node_id: spec.node_id.to_string(),
        kind: spec.kind,
        class: ProjectPlanNodeClass::Computation,
        command: spec.command.to_string(),
        dependencies: spec.dependencies,
        content_hash_inputs: spec.content_hash_inputs,
        outputs: vec![ProjectExtensionDagOutput {
            output_id: spec.output_id.to_string(),
            path: spec.path.to_string(),
            materialization: ProjectPlanOutputMaterialization::PlannedArtifact,
        }],
        limits: limits.clone(),
        cache_eligible: true,
        side_effects: vec![
            crate::project::ProjectPlanSideEffect {
                kind: ProjectPlanSideEffectKind::ReadsInput,
                description: "reads validated local Geo evaluation artifacts".to_string(),
            },
            crate::project::ProjectPlanSideEffect {
                kind: ProjectPlanSideEffectKind::WritesArtifact,
                description: "writes deterministic Geo evaluation run artifacts".to_string(),
            },
        ],
        refusal_conditions: Vec::new(),
    }
}

struct OverlayNodeSpec<'a> {
    project_node_id: &'a str,
    stage: GeoPlanStage,
    level: GeoControlEntityLevel,
    expected_output_contract: &'a str,
    bounded_section_required: bool,
    incidence_factorization_required: bool,
    exact_solve_scope: Option<GeoPlanExactSolveScope>,
}

fn overlay_node(
    spec: OverlayNodeSpec<'_>,
    deterministic_bounds: Vec<GeoNumericBound>,
) -> GeoPlanNodeOverlay {
    GeoPlanNodeOverlay {
        project_node_id: spec.project_node_id.to_string(),
        stage: spec.stage,
        entity_level: Some(spec.level),
        evidence_classes: vec![GeoEvidenceClass::ParcelGeometry],
        claim_classes: vec![GeoClaimClass::CollateralComposition],
        expected_output_contract: spec.expected_output_contract.to_string(),
        preconditions: stage_preconditions(spec.stage),
        claim_effect: GeoPlanClaimEffect::CanChangeRequestedClaim,
        bounded_section_required: spec.bounded_section_required,
        incidence_factorization_required: spec.incidence_factorization_required,
        exact_solve_scope: spec.exact_solve_scope,
        cost_estimate_ranges: deterministic_bounds
            .iter()
            .map(|bound| GeoPlanCostEstimateRange {
                semantic_id: format!("estimate.{}", bound.semantic_id),
                counter: bound.counter,
                lower_bound: 0,
                upper_bound: bound.value,
                unit: bound.unit.clone(),
                basis: "bounded by the evaluation case's declared deterministic counters"
                    .to_string(),
                semantic_effect: GeoTelemetrySemanticEffect::None,
            })
            .collect(),
        deterministic_bounds,
        transitions: GeoPlanTransitionSet {
            success: "validate output and unlock declared dependents".to_string(),
            abstention: "preserve completed artifacts and report the typed residual".to_string(),
            contradiction: "preserve the empty residual and diagnose admitted evidence".to_string(),
            budget_fallback:
                "preserve completed components and report deterministic budget fallback".to_string(),
        },
    }
}

fn stage_preconditions(stage: GeoPlanStage) -> Vec<GeoPlanPrecondition> {
    match stage {
        GeoPlanStage::MaterializeHomeCells => vec![precondition(
            GeoPlanGatePlane::Availability,
            GeoPlanGateStatus::SatisfiedByDeclaredInput,
            "evaluation supplies local typed candidate rows",
        )],
        GeoPlanStage::BuildBoundedSection => vec![precondition(
            GeoPlanGatePlane::Coverage,
            GeoPlanGateStatus::StructurallyCompleteRelativeToInputs,
            "bounded section mirrors the already-declared evaluation candidate universe",
        )],
        GeoPlanStage::MaterializeEvidence | GeoPlanStage::CompileEvidence => vec![precondition(
            GeoPlanGatePlane::Admission,
            GeoPlanGateStatus::PendingArtifact,
            "restricting observations keep their versioned rho admissions",
        )],
        GeoPlanStage::PropagateConstraints => vec![precondition(
            GeoPlanGatePlane::ConstraintEffect,
            GeoPlanGateStatus::PendingArtifact,
            "sound typed propagators prune only values entailed by admitted hard constraints",
        )],
        GeoPlanStage::FactorAndSolveExactResidual => vec![
            precondition(
                GeoPlanGatePlane::Coverage,
                GeoPlanGateStatus::StructurallyCompleteRelativeToInputs,
                "solve consumes the declared bounded section and compiled evidence artifacts",
            ),
            precondition(
                GeoPlanGatePlane::CandidateReach,
                GeoPlanGateStatus::UnverifiedWithClaimLimitation,
                "candidate reach is scored separately from this declared-universe run adapter",
            ),
            precondition(
                GeoPlanGatePlane::SolverCorrectness,
                GeoPlanGateStatus::PendingArtifact,
                "exact backend consumes the propagation artifact as a declared dependency",
            ),
        ],
        GeoPlanStage::ExplainResidual
        | GeoPlanStage::SeparateResidual
        | GeoPlanStage::SelectNextEvidence => vec![precondition(
            GeoPlanGatePlane::SolverCorrectness,
            GeoPlanGateStatus::PendingArtifact,
            "post-solve artifacts are produced only from completed declared-universe residuals",
        )],
    }
}

fn precondition(
    plane: GeoPlanGatePlane,
    status: GeoPlanGateStatus,
    detail: &str,
) -> GeoPlanPrecondition {
    GeoPlanPrecondition {
        plane,
        status,
        detail: detail.to_string(),
    }
}

fn case_deterministic_bounds(evidence: &GeoEvidenceCompilationRequest) -> Vec<GeoNumericBound> {
    let rows = evidence
        .universe
        .parcels
        .len()
        .saturating_add(evidence.universe.buildings.len())
        .saturating_add(
            evidence
                .observations
                .iter()
                .map(|observation| observation.source_records.len())
                .sum::<usize>(),
        )
        .max(1) as u64;
    let candidates = selected_feature_ids(
        selected_case_control_level(&evidence.profile.selection_level)
            .unwrap_or(GeoControlEntityLevel::Parcel),
        evidence,
    )
    .len()
    .max(1) as u64;
    vec![
        numeric_bound("budget.rows", GeoResourceCounter::Rows, rows, "rows"),
        numeric_bound(
            "budget.candidates",
            GeoResourceCounter::Candidates,
            candidates,
            "members",
        ),
        numeric_bound(
            "budget.states",
            GeoResourceCounter::States,
            evidence.max_assignments.max(1),
            "assignments",
        ),
        numeric_bound(
            "budget.models",
            GeoResourceCounter::Models,
            evidence.max_materialized_models.max(1),
            "models",
        ),
    ]
}

fn numeric_bound(
    semantic_id: &str,
    counter: GeoResourceCounter,
    value: u64,
    unit: &str,
) -> GeoNumericBound {
    GeoNumericBound {
        semantic_id: semantic_id.to_string(),
        counter,
        value,
        unit: unit.to_string(),
        origin: GeoValueOrigin::CallerDeclared,
        action: GeoBudgetAction::ReportBudgetFallback,
    }
}

fn read_run_artifact_bytes(
    workspace_root: &Path,
    relative_path: &str,
    case_id: &str,
    artifact_kind: &str,
) -> Result<Vec<u8>, GeoPopulationError> {
    let path = workspace_root.join(relative_path);
    fs::read(&path).map_err(|error| {
        GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo evaluate run path could not read an emitted artifact",
            [
                ("case_id", case_id.to_string()),
                ("artifact_kind", artifact_kind.to_string()),
                ("path", path.display().to_string()),
                ("error", error.to_string()),
            ],
        )
    })
}

fn parse_run_artifact<T: DeserializeOwned>(
    bytes: &[u8],
    case_id: &str,
    artifact_kind: &str,
) -> Result<T, GeoPopulationError> {
    serde_json::from_slice(bytes).map_err(|error| {
        GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo evaluate run path emitted an unreadable JSON artifact",
            [
                ("case_id", case_id.to_string()),
                ("artifact_kind", artifact_kind.to_string()),
                ("error", error.to_string()),
            ],
        )
    })
}

fn map_geo_run_error(case_id: &str, error: super::run::GeoRunError) -> GeoPopulationError {
    let detail = error
        .detail
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("; ");
    GeoPopulationError::new(
        GeoPopulationErrorCode::Composition,
        "Geo population evaluate run path failed",
        [
            ("case_id", case_id.to_string()),
            ("geo_run_code", format!("{:?}", error.code)),
            ("geo_run_message", error.message),
            ("geo_run_detail", detail),
        ],
    )
}

pub fn evaluate_candidate_truth_handoff(
    request: &GeoCandidateTruthEvaluationRequest,
) -> Result<GeoCandidateTruthEvaluationArtifact, GeoPopulationError> {
    if request.version != CANON_GEO_FROZEN_E4_H7_CANDIDATE_TRUTH_HANDOFF_REQUEST_VERSION {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo candidate/truth handoff request version",
            [
                ("actual", request.version.as_str()),
                (
                    "expected",
                    CANON_GEO_FROZEN_E4_H7_CANDIDATE_TRUTH_HANDOFF_REQUEST_VERSION,
                ),
            ],
        ));
    }
    validate_nonempty_canonical("population_id", &request.population_id)?;
    validate_candidate_truth_gate(&request.gate)?;
    if request.max_release_rows == 0 || request.rows.len() > request.max_release_rows {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::PopulationBudgetExceeded,
            "Geo candidate/truth handoff exceeds the declared release-row budget",
            [
                ("release_rows", request.rows.len().to_string()),
                ("max_release_rows", request.max_release_rows.to_string()),
            ],
        ));
    }
    if request.rows.is_empty() {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo candidate/truth handoff must contain at least one release row",
            [("field", "rows")],
        ));
    }

    let mut rows = request.rows.clone();
    for row in &mut rows {
        validate_candidate_truth_handoff_row(row)?;
    }
    validate_candidate_truth_row_release_ids(&request.gate, &rows)?;
    let mut row_ids = BTreeSet::new();
    for row in &rows {
        if !row_ids.insert(row.row_id.clone()) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo candidate/truth handoff contains a duplicate release row identifier",
                [("row_id", row.row_id.as_str())],
            ));
        }
    }
    let row_logical_subjects = validate_candidate_truth_logical_subject_bindings(
        &request.logical_subject_bindings,
        &rows,
    )?;
    rows.sort_by(|left, right| {
        row_logical_subjects
            .get(&left.row_id)
            .expect("validated row binding")
            .cmp(
                row_logical_subjects
                    .get(&right.row_id)
                    .expect("validated row binding"),
            )
            .then_with(|| left.release_id.cmp(&right.release_id))
            .then_with(|| left.row_id.cmp(&right.row_id))
    });
    let mut logical_subject_release_keys = BTreeSet::new();
    let mut logical_subject_truth_planes = BTreeMap::new();
    let mut logical_subject_truth_models = BTreeMap::new();
    for row in &rows {
        let logical_subject_id = row_logical_subjects
            .get(&row.row_id)
            .expect("validated row binding");
        let key = (logical_subject_id.to_string(), row.release_id.clone());
        if !logical_subject_release_keys.insert(key) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo candidate/truth handoff repeats one logical subject/release measurement",
                [
                    ("logical_subject_id", logical_subject_id.as_str()),
                    ("release_id", row.release_id.as_str()),
                ],
            ));
        }
        if let Some(previous) =
            logical_subject_truth_planes.insert(logical_subject_id.to_string(), row.truth_plane)
            && previous != row.truth_plane
        {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo candidate/truth handoff assigns one logical subject to multiple truth planes",
                [
                    ("logical_subject_id", logical_subject_id.to_string()),
                    ("previous_truth_plane", format!("{previous:?}")),
                    ("current_truth_plane", format!("{:?}", row.truth_plane)),
                ],
            ));
        }
        let truth_digest = composition_model_digest(&row.truth)?;
        if let Some(previous) = logical_subject_truth_models
            .insert(logical_subject_id.to_string(), truth_digest.clone())
            && previous != truth_digest
        {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo candidate/truth handoff assigns one logical subject conflicting truth models across releases",
                [
                    ("logical_subject_id", logical_subject_id.to_string()),
                    ("previous_truth_digest", previous),
                    ("current_truth_digest", truth_digest),
                ],
            ));
        }
    }

    let mut evaluations = Vec::with_capacity(rows.len());
    for row in rows {
        let logical_subject_id = row_logical_subjects
            .get(&row.row_id)
            .expect("validated row binding")
            .clone();
        evaluations.push(evaluate_candidate_truth_row(logical_subject_id, row)?);
    }
    let summary = summarize_candidate_truth_evaluations(&request.gate, &evaluations)?;
    Ok(GeoCandidateTruthEvaluationArtifact {
        version: CANON_GEO_FROZEN_E4_H7_CANDIDATE_TRUTH_EVALUATION_VERSION.to_string(),
        request_version: request.version.clone(),
        population_id: request.population_id.clone(),
        summary,
        rows: evaluations,
    })
}

pub fn canonical_population_evaluation_bytes(
    artifact: &GeoPopulationEvaluationArtifact,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(artifact)
}

pub fn canonical_population_request_bytes(
    request: &GeoPopulationEvaluationRequest,
) -> Result<Vec<u8>, GeoPopulationError> {
    let canonical = canonicalize_population_request(request)?;
    serde_json::to_vec(&canonical).map_err(|error| {
        GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo population request could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub fn canonical_candidate_truth_evaluation_bytes(
    artifact: &GeoCandidateTruthEvaluationArtifact,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(artifact)
}

pub fn e4_proof_source_from_population_request(
    request: &GeoPopulationEvaluationRequest,
) -> Result<GeoE4GateProofSource, GeoPopulationError> {
    let population_request_blake3 = digest_population_request(request)?;
    let proof_source = GeoE4GateProofSource {
        derivation: GeoE4GateProofDerivation::BarePopulationRequest,
        proof_class: GeoE4GateProofClass::FixtureSubset,
        source_version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        source_blake3: population_request_blake3.clone(),
        population_request_blake3,
        inherited_source_version: None,
        inherited_source_blake3: None,
        h7_population_scope: None,
        h7_result_mode: None,
        h7_materialized_unique_accepted_loans: None,
        h7_solver_population_subjects: None,
    };
    validate_e4_gate_proof_source(&proof_source)?;
    Ok(proof_source)
}

pub fn e4_proof_source_from_h7_population(
    artifact: &GeoH7PopulationArtifact,
) -> Result<GeoE4GateProofSource, GeoPopulationError> {
    validate_h7_population_artifact(artifact).map_err(map_h7_population_error)?;
    let source_bytes = canonical_h7_population_bytes(artifact).map_err(|error| {
        GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo H.7 population artifact could not be serialized for E4 proof source",
            [("error", error.to_string())],
        )
    })?;
    let proof_source = GeoE4GateProofSource {
        derivation: GeoE4GateProofDerivation::H7PopulationArtifact,
        proof_class: h7_scope_to_e4_proof_class(artifact.summary.population_scope),
        source_version: CANON_GEO_H7_POPULATION_VERSION.to_string(),
        source_blake3: blake3::hash(&source_bytes).to_hex().to_string(),
        population_request_blake3: digest_population_request(&artifact.population)?,
        inherited_source_version: None,
        inherited_source_blake3: None,
        h7_population_scope: Some(artifact.summary.population_scope),
        h7_result_mode: Some(artifact.provenance.result_mode),
        h7_materialized_unique_accepted_loans: Some(
            artifact.summary.materialized_unique_accepted_loans,
        ),
        h7_solver_population_subjects: Some(artifact.summary.solver_population_subjects),
    };
    validate_e4_gate_proof_source(&proof_source)?;
    Ok(proof_source)
}

pub fn e4_proof_source_from_population_stack(
    stack_source_blake3: String,
    population_request_blake3: String,
    base_population_blake3: String,
    base_population_provenance: Option<&GeoE4GateProofSource>,
) -> Result<GeoE4GateProofSource, GeoPopulationError> {
    validate_lowercase_hex64("stack_source_blake3", &stack_source_blake3)?;
    validate_lowercase_hex64("population_request_blake3", &population_request_blake3)?;
    validate_lowercase_hex64("base_population_blake3", &base_population_blake3)?;
    let proof_source = match base_population_provenance {
        Some(base) => {
            validate_e4_gate_proof_source(base)?;
            if base.population_request_blake3 != base_population_blake3 {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo E4 proof source does not match the stack base population",
                    [
                        ("expected", base_population_blake3.as_str()),
                        ("actual", base.population_request_blake3.as_str()),
                    ],
                ));
            }
            GeoE4GateProofSource {
                derivation: GeoE4GateProofDerivation::PopulationEvidenceStackArtifact,
                proof_class: base.proof_class,
                source_version: CANON_GEO_POPULATION_EVIDENCE_STACK_PROOF_VERSION.to_string(),
                source_blake3: stack_source_blake3,
                population_request_blake3,
                inherited_source_version: Some(base.source_version.clone()),
                inherited_source_blake3: Some(base.source_blake3.clone()),
                h7_population_scope: base.h7_population_scope,
                h7_result_mode: base.h7_result_mode,
                h7_materialized_unique_accepted_loans: base.h7_materialized_unique_accepted_loans,
                h7_solver_population_subjects: base.h7_solver_population_subjects,
            }
        }
        None => GeoE4GateProofSource {
            derivation: GeoE4GateProofDerivation::PopulationEvidenceStackArtifact,
            proof_class: GeoE4GateProofClass::FixtureSubset,
            source_version: CANON_GEO_POPULATION_EVIDENCE_STACK_PROOF_VERSION.to_string(),
            source_blake3: stack_source_blake3,
            population_request_blake3,
            inherited_source_version: Some(CANON_GEO_POPULATION_REQUEST_VERSION.to_string()),
            inherited_source_blake3: Some(base_population_blake3),
            h7_population_scope: None,
            h7_result_mode: None,
            h7_materialized_unique_accepted_loans: None,
            h7_solver_population_subjects: None,
        },
    };
    validate_e4_gate_proof_source(&proof_source)?;
    Ok(proof_source)
}

pub fn assess_e4_gate(
    artifact: &GeoPopulationEvaluationArtifact,
    proof_source: &GeoE4GateProofSource,
) -> Result<GeoE4GateAssessment, GeoPopulationError> {
    validate_population_evaluation_artifact(artifact)?;
    validate_e4_gate_proof_source(proof_source)?;
    let proof_class = proof_source.proof_class;
    let source_evaluation_blake3 = blake3::hash(
        &canonical_population_evaluation_bytes(artifact).map_err(|error| {
            GeoPopulationError::new(
                GeoPopulationErrorCode::Composition,
                "Geo E4 gate assessment could not serialize the source evaluation",
                [("error", error.to_string())],
            )
        })?,
    )
    .to_hex()
    .to_string();
    let planes = e4_plane_scores(artifact.cases.iter())?;
    let mut by_truth_plane = BTreeMap::<GeoTruthPlane, Vec<&GeoPopulationCaseEvaluation>>::new();
    for case in &artifact.cases {
        by_truth_plane
            .entry(case.truth_plane)
            .or_default()
            .push(case);
    }
    let mut truth_planes = Vec::with_capacity(by_truth_plane.len());
    for (truth_plane, cases) in by_truth_plane {
        truth_planes.push(GeoE4TruthPlaneGateAssessment {
            truth_plane,
            planes: e4_plane_scores(cases)?,
        });
    }

    let required_subjects = CANON_GEO_FROZEN_E4_H7_REQUIRED_SUBJECTS;
    let evaluated_cases = planes.coverage.cases;
    let subject_deficit = required_subjects.saturating_sub(evaluated_cases);
    let blockers = e4_gate_blockers(proof_class, required_subjects, evaluated_cases, &planes)?;
    let case_findings = e4_gate_case_findings(artifact.cases.iter());
    let status = if blockers.is_empty() {
        GeoE4GateStatus::Passed
    } else {
        GeoE4GateStatus::Open
    };
    let assessment = GeoE4GateAssessment {
        version: CANON_GEO_E4_GATE_ASSESSMENT_VERSION.to_string(),
        gate_id: CANON_GEO_FROZEN_E4_H7_GATE_ID.to_string(),
        proof_class,
        proof_source: proof_source.clone(),
        status,
        release_claim_allowed: status == GeoE4GateStatus::Passed
            && proof_class == GeoE4GateProofClass::LiveComplete,
        required_subjects,
        evaluated_cases,
        subject_deficit,
        source_evaluation_blake3,
        blockers,
        case_findings,
        planes,
        truth_planes,
    };
    validate_e4_gate_assessment(&assessment)?;
    Ok(assessment)
}

pub fn canonical_e4_gate_assessment_bytes(
    assessment: &GeoE4GateAssessment,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(assessment)
}

pub fn compare_e4_gate_assessments(
    before: &GeoE4GateAssessment,
    after: &GeoE4GateAssessment,
) -> Result<GeoE4RescoreComparisonArtifact, GeoPopulationError> {
    validate_e4_gate_assessment(before)?;
    validate_e4_gate_assessment(after)?;
    if before.gate_id != after.gate_id {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 rescore comparison requires matching gate ids",
            [
                ("before_gate_id", before.gate_id.as_str()),
                ("after_gate_id", after.gate_id.as_str()),
            ],
        ));
    }
    if before.required_subjects != after.required_subjects {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 rescore comparison requires matching frozen denominators",
            [
                (
                    "before_required_subjects",
                    before.required_subjects.to_string(),
                ),
                (
                    "after_required_subjects",
                    after.required_subjects.to_string(),
                ),
            ],
        ));
    }
    let before_blake3 = digest_e4_gate_assessment(before)?;
    let after_blake3 = digest_e4_gate_assessment(after)?;
    let mut table = Vec::with_capacity(E4_RESCORE_METRICS.len());
    for metric in E4_RESCORE_METRICS {
        let before_value = e4_rescore_metric_value(before, metric);
        let after_value = e4_rescore_metric_value(after, metric);
        table.push(GeoE4RescoreComparisonRow {
            metric,
            before: before_value,
            after: after_value,
            delta: signed_delta(
                "e4_rescore_comparison.table.delta",
                before_value,
                after_value,
            )?,
        });
    }
    let artifact = GeoE4RescoreComparisonArtifact {
        version: CANON_GEO_E4_RESCORE_COMPARISON_VERSION.to_string(),
        gate_id: before.gate_id.clone(),
        required_subjects: before.required_subjects,
        before: e4_rescore_snapshot(before, before_blake3),
        after: e4_rescore_snapshot(after, after_blake3),
        table,
        interpretation: GeoE4RescoreInterpretation {
            denominator_frozen: true,
            failures_remain_in_denominator: true,
            candidate_universe_change_is_truth_neutral: true,
            ambiguity_may_increase_when_reach_improves: true,
        },
    };
    validate_e4_rescore_comparison_artifact(&artifact)?;
    Ok(artifact)
}

pub fn canonical_e4_rescore_comparison_bytes(
    comparison: &GeoE4RescoreComparisonArtifact,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(comparison)
}

pub fn validate_e4_rescore_comparison_artifact(
    comparison: &GeoE4RescoreComparisonArtifact,
) -> Result<(), GeoPopulationError> {
    if comparison.version != CANON_GEO_E4_RESCORE_COMPARISON_VERSION {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo E4 rescore comparison version",
            [
                ("expected", CANON_GEO_E4_RESCORE_COMPARISON_VERSION),
                ("actual", comparison.version.as_str()),
            ],
        ));
    }
    if comparison.gate_id != CANON_GEO_FROZEN_E4_H7_GATE_ID {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 rescore comparison gate_id is not the frozen E4/H7 gate",
            [
                ("expected", CANON_GEO_FROZEN_E4_H7_GATE_ID),
                ("actual", comparison.gate_id.as_str()),
            ],
        ));
    }
    if comparison.required_subjects != CANON_GEO_FROZEN_E4_H7_REQUIRED_SUBJECTS {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 rescore comparison required_subjects must equal the frozen E4/H7 count",
            [
                (
                    "expected",
                    CANON_GEO_FROZEN_E4_H7_REQUIRED_SUBJECTS.to_string(),
                ),
                ("actual", comparison.required_subjects.to_string()),
            ],
        ));
    }
    validate_e4_rescore_snapshot("before", comparison.required_subjects, &comparison.before)?;
    validate_e4_rescore_snapshot("after", comparison.required_subjects, &comparison.after)?;
    if !comparison.interpretation.denominator_frozen
        || !comparison.interpretation.failures_remain_in_denominator
        || !comparison
            .interpretation
            .candidate_universe_change_is_truth_neutral
        || !comparison
            .interpretation
            .ambiguity_may_increase_when_reach_improves
    {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 rescore comparison interpretation must preserve the frozen-denominator reach semantics",
            [("field", "interpretation")],
        ));
    }
    if comparison.table.len() != E4_RESCORE_METRICS.len() {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 rescore comparison table must contain the predeclared metric set",
            [
                ("expected", E4_RESCORE_METRICS.len().to_string()),
                ("actual", comparison.table.len().to_string()),
            ],
        ));
    }
    for (index, row) in comparison.table.iter().enumerate() {
        let expected_metric = E4_RESCORE_METRICS[index];
        if row.metric != expected_metric {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo E4 rescore comparison table metrics must stay in predeclared order",
                [
                    ("index", index.to_string()),
                    ("expected", format!("{expected_metric:?}")),
                    ("actual", format!("{:?}", row.metric)),
                ],
            ));
        }
        let expected_before = e4_rescore_snapshot_metric_value(&comparison.before, row.metric);
        if row.before != expected_before {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo E4 rescore comparison row before value is inconsistent with the snapshot",
                [
                    ("metric", format!("{:?}", row.metric)),
                    ("expected", expected_before.to_string()),
                    ("actual", row.before.to_string()),
                ],
            ));
        }
        let expected_after = e4_rescore_snapshot_metric_value(&comparison.after, row.metric);
        if row.after != expected_after {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo E4 rescore comparison row after value is inconsistent with the snapshot",
                [
                    ("metric", format!("{:?}", row.metric)),
                    ("expected", expected_after.to_string()),
                    ("actual", row.after.to_string()),
                ],
            ));
        }
        let expected_delta =
            signed_delta("e4_rescore_comparison.table.delta", row.before, row.after)?;
        if row.delta != expected_delta {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo E4 rescore comparison row delta is inconsistent",
                [
                    ("metric", format!("{:?}", row.metric)),
                    ("expected", expected_delta.to_string()),
                    ("actual", row.delta.to_string()),
                ],
            ));
        }
    }
    Ok(())
}

pub fn validate_e4_gate_assessment(
    assessment: &GeoE4GateAssessment,
) -> Result<(), GeoPopulationError> {
    if assessment.version != CANON_GEO_E4_GATE_ASSESSMENT_VERSION {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo E4 gate assessment version",
            [
                ("actual", assessment.version.as_str()),
                ("expected", CANON_GEO_E4_GATE_ASSESSMENT_VERSION),
            ],
        ));
    }
    if assessment.gate_id != CANON_GEO_FROZEN_E4_H7_GATE_ID {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 gate assessment gate_id is not the frozen E4/H7 gate",
            [
                ("actual", assessment.gate_id.as_str()),
                ("expected", CANON_GEO_FROZEN_E4_H7_GATE_ID),
            ],
        ));
    }
    if assessment.required_subjects != CANON_GEO_FROZEN_E4_H7_REQUIRED_SUBJECTS {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 gate assessment required_subjects must equal the frozen E4/H7 count",
            [
                ("actual", assessment.required_subjects.to_string()),
                (
                    "expected",
                    CANON_GEO_FROZEN_E4_H7_REQUIRED_SUBJECTS.to_string(),
                ),
            ],
        ));
    }
    validate_lowercase_hex64(
        "source_evaluation_blake3",
        &assessment.source_evaluation_blake3,
    )?;
    validate_e4_gate_proof_source(&assessment.proof_source)?;
    if assessment.proof_class != assessment.proof_source.proof_class {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 gate assessment proof_class must match its derived proof source",
            [
                (
                    "proof_class",
                    e4_proof_class_name(assessment.proof_class).to_string(),
                ),
                (
                    "proof_source.proof_class",
                    e4_proof_class_name(assessment.proof_source.proof_class).to_string(),
                ),
            ],
        ));
    }
    validate_e4_plane_scores("assessment.planes", &assessment.planes)?;
    if assessment.evaluated_cases != assessment.planes.coverage.cases {
        return Err(summary_invariant_error(
            "e4_gate_assessment",
            "evaluated_cases",
            assessment.planes.coverage.cases,
            assessment.evaluated_cases,
        ));
    }
    let expected_deficit = assessment
        .required_subjects
        .saturating_sub(assessment.evaluated_cases);
    if assessment.subject_deficit != expected_deficit {
        return Err(summary_invariant_error(
            "e4_gate_assessment",
            "subject_deficit",
            expected_deficit,
            assessment.subject_deficit,
        ));
    }
    let expected_status = if assessment.blockers.is_empty() {
        GeoE4GateStatus::Passed
    } else {
        GeoE4GateStatus::Open
    };
    if assessment.status != expected_status {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 gate assessment status is inconsistent with blockers",
            [
                ("status", format!("{:?}", assessment.status)),
                ("blockers", assessment.blockers.len().to_string()),
            ],
        ));
    }
    let expected_release_claim_allowed = assessment.status == GeoE4GateStatus::Passed
        && assessment.proof_source.proof_class == GeoE4GateProofClass::LiveComplete;
    if assessment.release_claim_allowed != expected_release_claim_allowed {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 gate assessment release-claim field is inconsistent",
            [
                (
                    "release_claim_allowed",
                    assessment.release_claim_allowed.to_string(),
                ),
                ("expected", expected_release_claim_allowed.to_string()),
            ],
        ));
    }
    for pair in assessment.blockers.windows(2) {
        if pair[0] >= pair[1] {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo E4 gate assessment blockers must be sorted and unique",
                [
                    ("previous", format!("{:?}", pair[0].code)),
                    ("current", format!("{:?}", pair[1].code)),
                ],
            ));
        }
    }
    let expected_blockers = e4_gate_blockers(
        assessment.proof_class,
        assessment.required_subjects,
        assessment.evaluated_cases,
        &assessment.planes,
    )?;
    if assessment.blockers != expected_blockers {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 gate assessment blockers do not match the scored gate planes",
            [
                (
                    "actual",
                    format!(
                        "{:?}",
                        assessment
                            .blockers
                            .iter()
                            .map(|blocker| blocker.code)
                            .collect::<Vec<_>>()
                    ),
                ),
                (
                    "expected",
                    format!(
                        "{:?}",
                        expected_blockers
                            .iter()
                            .map(|blocker| blocker.code)
                            .collect::<Vec<_>>()
                    ),
                ),
            ],
        ));
    }
    validate_e4_gate_case_findings(assessment)?;
    let mut previous_truth_plane = None;
    let mut truth_plane_cases = 0_u64;
    for plane in &assessment.truth_planes {
        if let Some(previous) = previous_truth_plane
            && previous >= plane.truth_plane
        {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo E4 gate assessment truth planes must be sorted and unique",
                [("truth_plane", format!("{:?}", plane.truth_plane))],
            ));
        }
        previous_truth_plane = Some(plane.truth_plane);
        validate_e4_plane_scores("assessment.truth_planes", &plane.planes)?;
        checked_add(
            &mut truth_plane_cases,
            plane.planes.coverage.cases,
            "truth_plane_cases",
        )?;
    }
    if truth_plane_cases != assessment.evaluated_cases {
        return Err(summary_invariant_error(
            "e4_gate_assessment",
            "truth_plane_cases",
            assessment.evaluated_cases,
            truth_plane_cases,
        ));
    }
    let truth_plane_score_sum =
        e4_sum_plane_scores(assessment.truth_planes.iter().map(|plane| &plane.planes))?;
    if truth_plane_score_sum != assessment.planes {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 gate assessment truth planes do not sum to the global plane scores",
            [("field", "truth_planes")],
        ));
    }
    Ok(())
}

pub fn canonical_deed_index_rows_bytes(
    request: &GeoDeedIndexRowsRequest,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(request)
}

pub fn canonical_deed_truth_bytes(
    artifact: &GeoDeedTruthArtifact,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(artifact)
}

pub fn validate_population_evaluation_artifact(
    artifact: &GeoPopulationEvaluationArtifact,
) -> Result<(), GeoPopulationError> {
    if artifact.version != CANON_GEO_POPULATION_EVALUATION_VERSION {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo population evaluation artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_POPULATION_EVALUATION_VERSION),
            ],
        ));
    }
    if artifact.request_version != CANON_GEO_POPULATION_REQUEST_VERSION {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo population evaluation request version",
            [
                ("actual", artifact.request_version.as_str()),
                ("expected", CANON_GEO_POPULATION_REQUEST_VERSION),
            ],
        ));
    }
    for case in &artifact.cases {
        validate_case_evaluation(case)?;
    }
    validate_deed_truth_plane_scope(artifact.cases.iter().map(|case| case.truth_plane))?;
    if let Some(summary) = &artifact.truth_binding {
        validate_population_truth_binding_summary(summary)?;
        if artifact.summary.cases != summary.bound_unique_cases {
            return Err(summary_invariant_error(
                "truth_binding",
                "bound_unique_cases",
                artifact.summary.cases,
                summary.bound_unique_cases,
            ));
        }
        if artifact
            .cases
            .iter()
            .any(|case| case.truth_plane != summary.truth_plane)
        {
            return Err(GeoPopulationError::invalid_input(
                "Geo deed truth binding summary plane must match every evaluated case",
                [("field", "truth_binding.truth_plane")],
            ));
        }
    }
    validate_summary(&artifact.summary)?;
    let expected_summary = summarize(&artifact.cases)?;
    if artifact.summary != expected_summary {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo population evaluation summary does not match case evaluations",
            [("field", "summary")],
        ));
    }
    Ok(())
}

pub fn validate_candidate_truth_evaluation_artifact(
    artifact: &GeoCandidateTruthEvaluationArtifact,
) -> Result<(), GeoPopulationError> {
    if artifact.version != CANON_GEO_FROZEN_E4_H7_CANDIDATE_TRUTH_EVALUATION_VERSION {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo candidate/truth evaluation artifact version",
            [
                ("actual", artifact.version.as_str()),
                (
                    "expected",
                    CANON_GEO_FROZEN_E4_H7_CANDIDATE_TRUTH_EVALUATION_VERSION,
                ),
            ],
        ));
    }
    if artifact.request_version != CANON_GEO_FROZEN_E4_H7_CANDIDATE_TRUTH_HANDOFF_REQUEST_VERSION {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo candidate/truth handoff request version",
            [
                ("actual", artifact.request_version.as_str()),
                (
                    "expected",
                    CANON_GEO_FROZEN_E4_H7_CANDIDATE_TRUTH_HANDOFF_REQUEST_VERSION,
                ),
            ],
        ));
    }
    validate_nonempty_canonical("population_id", &artifact.population_id)?;
    validate_candidate_truth_gate(&artifact.summary.gate)?;
    let mut row_ids = BTreeSet::new();
    for row in &artifact.rows {
        validate_candidate_truth_case_evaluation(row)?;
        if !row_ids.insert(row.row_id.clone()) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo candidate/truth evaluation artifact contains a duplicate release row identifier",
                [("row_id", row.row_id.as_str())],
            ));
        }
    }
    validate_candidate_truth_evaluation_row_release_ids(&artifact.summary.gate, &artifact.rows)?;
    validate_candidate_truth_summary(&artifact.summary)?;
    let expected_summary =
        summarize_candidate_truth_evaluations(&artifact.summary.gate, &artifact.rows)?;
    if artifact.summary != expected_summary {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo candidate/truth evaluation summary does not match row evaluations",
            [("field", "summary")],
        ));
    }
    Ok(())
}

pub fn validate_deed_index_rows_request(
    request: &GeoDeedIndexRowsRequest,
) -> Result<(), GeoPopulationError> {
    if request.version != CANON_GEO_DEED_INDEX_ROWS_VERSION {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo deed index rows version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_DEED_INDEX_ROWS_VERSION),
            ],
        ));
    }
    validate_nonempty_canonical("source_dataset", &request.source_dataset)?;
    validate_nonempty_canonical("source_release", &request.source_release)?;
    if request.max_rows == 0 || request.rows.len() > request.max_rows {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::PopulationBudgetExceeded,
            "Geo deed index rows exceed the declared row budget",
            [
                ("rows", request.rows.len().to_string()),
                ("max_rows", request.max_rows.to_string()),
            ],
        ));
    }
    validate_sorted_deed_source_pins("source_pins", &request.source_pins, true)?;
    let pinned_releases = request
        .source_pins
        .iter()
        .map(|pin| pin.source_release.as_str())
        .collect::<BTreeSet<_>>();
    let mut previous: Option<&str> = None;
    for row in &request.rows {
        validate_deed_index_row(row, &pinned_releases)?;
        if previous.is_some_and(|previous| previous >= row.instrument_id.as_str()) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo deed index rows must be sorted by distinct instrument_id",
                [
                    ("field", "rows[].instrument_id"),
                    ("instrument_id", row.instrument_id.as_str()),
                ],
            ));
        }
        previous = Some(row.instrument_id.as_str());
    }
    Ok(())
}

pub fn derive_deed_truth_from_index(
    loans: &[GeoDeedTruthLoanRef],
    deed_index: &GeoDeedIndexRowsRequest,
    window_days: u32,
) -> Result<GeoDeedTruthArtifact, GeoPopulationError> {
    validate_deed_index_rows_request(deed_index)?;
    derive_deed_truth_inner(
        loans,
        &deed_index.rows,
        window_days,
        deed_index.source_pins.clone(),
        deed_index.version.clone(),
    )
}

pub fn derive_deed_truth(
    loans: &[GeoDeedTruthLoanRef],
    deeds: &[GeoDeedIndexRow],
    window_days: u32,
) -> Result<GeoDeedTruthArtifact, GeoPopulationError> {
    if deeds.is_empty() {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo deed truth derivation requires source-pinned deed rows",
            [("field", "deeds")],
        ));
    }
    let source_pins = collect_deed_source_pins(deeds)?;
    derive_deed_truth_inner(
        loans,
        deeds,
        window_days,
        source_pins,
        CANON_GEO_DEED_INDEX_ROWS_VERSION.to_string(),
    )
}

pub fn validate_deed_truth_artifact(
    artifact: &GeoDeedTruthArtifact,
) -> Result<(), GeoPopulationError> {
    if artifact.version != CANON_GEO_DEED_TRUTH_VERSION {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo deed truth artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_DEED_TRUTH_VERSION),
            ],
        ));
    }
    if artifact.deed_index_version != CANON_GEO_DEED_INDEX_ROWS_VERSION {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo deed index version on truth artifact",
            [
                ("actual", artifact.deed_index_version.as_str()),
                ("expected", CANON_GEO_DEED_INDEX_ROWS_VERSION),
            ],
        ));
    }
    if artifact.truth_plane != GeoTruthPlane::DeedGrainInstrument {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo deed truth artifact must declare the deed-grain truth plane",
            [("field", "truth_plane")],
        ));
    }
    validate_sorted_deed_source_pins("source_pins", &artifact.source_pins, true)?;
    if artifact.proof_class != deed_truth_proof_class(&artifact.source_pins)? {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo deed truth artifact proof_class must be the minimum source-pin proof class",
            [("field", "proof_class")],
        ));
    }
    let mut previous: Option<&str> = None;
    for row in &artifact.per_loan {
        validate_deed_truth_loan_match(row)?;
        if previous.is_some_and(|previous| previous >= row.loan_id.as_str()) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo deed truth rows must be sorted by distinct loan_id",
                [("loan_id", row.loan_id.as_str())],
            ));
        }
        previous = Some(row.loan_id.as_str());
    }
    let expected_summary = summarize_deed_truth(&artifact.per_loan)?;
    if artifact.summary != expected_summary {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo deed truth summary does not match per-loan rows",
            [("field", "summary")],
        ));
    }
    Ok(())
}

pub fn bind_deed_truth_to_population(
    request: &GeoPopulationEvaluationRequest,
    artifact: &GeoDeedTruthArtifact,
) -> Result<
    (
        GeoPopulationEvaluationRequest,
        GeoPopulationTruthBindingSummary,
    ),
    GeoPopulationError,
> {
    validate_deed_truth_artifact(artifact)?;
    validate_deed_truth_binding_population_envelope(request)?;

    let unique_truth_by_loan = artifact
        .per_loan
        .iter()
        .filter(|row| row.match_kind == GeoDeedTruthMatchKind::Unique)
        .map(|row| (row.loan_id.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    let mut bound_cases = Vec::new();
    for case in &request.cases {
        if let Some(row) = unique_truth_by_loan.get(case.id.as_str()) {
            let mut bound_case = case.clone();
            bound_case.truth_plane = GeoTruthPlane::DeedGrainInstrument;
            bound_case.truth = GeoCompositionModel {
                parcels: row.parcel_ids.clone(),
                buildings: Vec::new(),
            };
            bound_cases.push(bound_case);
        }
    }
    let bound_unique_cases =
        checked_len(bound_cases.len(), "deed_truth_binding.bound_unique_cases")?;
    if bound_unique_cases == 0 {
        return Err(GeoPopulationError::invalid_input(
            "Geo deed truth artifact has no Unique rows matching the population case ids",
            [
                ("field", "truth.per_loan"),
                ("truth_plane", "deed_grain_instrument"),
            ],
        ));
    }
    let unique_not_in_population_cases = checked_deed_truth_binding_difference(
        "deed_truth_binding.unique_not_in_population_cases",
        artifact.summary.unique,
        bound_unique_cases,
    )?;
    let deed_truth_unbound_cases = checked_deed_truth_binding_difference(
        "deed_truth_binding.deed_truth_unbound_cases",
        artifact.summary.loans,
        bound_unique_cases,
    )?;
    let summary = GeoPopulationTruthBindingSummary {
        truth_plane: GeoTruthPlane::DeedGrainInstrument,
        source_version: artifact.version.clone(),
        source_proof_class: artifact.proof_class,
        input_loans: artifact.summary.loans,
        unique_truth_rows: artifact.summary.unique,
        bound_unique_cases,
        unique_not_in_population_cases,
        non_unique_discarded: artifact.summary.non_unique_discarded,
        no_match: artifact.summary.no_match,
        deed_truth_unbound_cases,
        round_amount_loans: artifact.summary.round_amount_loans,
        round_amount_unique: artifact.summary.round_amount_unique,
    };
    validate_population_truth_binding_summary(&summary)?;
    Ok((
        GeoPopulationEvaluationRequest {
            version: request.version.clone(),
            cases: bound_cases,
            max_cases: request.max_cases,
        },
        summary,
    ))
}

fn validate_deed_truth_binding_population_envelope(
    request: &GeoPopulationEvaluationRequest,
) -> Result<(), GeoPopulationError> {
    if request.version != CANON_GEO_POPULATION_REQUEST_VERSION {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo population request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_POPULATION_REQUEST_VERSION),
            ],
        ));
    }
    if request.max_cases == 0 || request.cases.len() > request.max_cases {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::PopulationBudgetExceeded,
            "Geo population exceeds the declared case budget",
            [
                ("cases", request.cases.len().to_string()),
                ("max_cases", request.max_cases.to_string()),
            ],
        ));
    }

    let mut seen = BTreeSet::new();
    for case in &request.cases {
        if case.id.is_empty() || case.id.trim() != case.id {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo population case identifiers must be non-empty and canonical",
                [("case_id", case.id.as_str())],
            ));
        }
        if !seen.insert(case.id.as_str()) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo population contains a duplicate case identifier",
                [("case_id", case.id.as_str())],
            ));
        }
    }
    Ok(())
}

fn validate_population_truth_binding_summary(
    summary: &GeoPopulationTruthBindingSummary,
) -> Result<(), GeoPopulationError> {
    if summary.truth_plane != GeoTruthPlane::DeedGrainInstrument {
        return Err(GeoPopulationError::invalid_input(
            "Geo deed truth binding summary must declare the deed-grain truth plane",
            [("field", "truth_binding.truth_plane")],
        ));
    }
    if summary.source_version != CANON_GEO_DEED_TRUTH_VERSION {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo deed truth binding source version",
            [
                ("actual", summary.source_version.as_str()),
                ("expected", CANON_GEO_DEED_TRUTH_VERSION),
            ],
        ));
    }
    let observed_loans = sum_u64(
        [
            summary.unique_truth_rows,
            summary.non_unique_discarded,
            summary.no_match,
        ],
        "truth_binding.input_loans",
    )?;
    if observed_loans != summary.input_loans {
        return Err(summary_invariant_error(
            "truth_binding",
            "input_loans",
            summary.input_loans,
            observed_loans,
        ));
    }
    if summary.bound_unique_cases > summary.unique_truth_rows {
        return Err(summary_invariant_error(
            "truth_binding",
            "bound_unique_cases",
            summary.unique_truth_rows,
            summary.bound_unique_cases,
        ));
    }
    let expected_unique_not_in_population = checked_deed_truth_binding_difference(
        "truth_binding.unique_not_in_population_cases",
        summary.unique_truth_rows,
        summary.bound_unique_cases,
    )?;
    if summary.unique_not_in_population_cases != expected_unique_not_in_population {
        return Err(summary_invariant_error(
            "truth_binding",
            "unique_not_in_population_cases",
            expected_unique_not_in_population,
            summary.unique_not_in_population_cases,
        ));
    }
    let expected_unbound = checked_deed_truth_binding_difference(
        "truth_binding.deed_truth_unbound_cases",
        summary.input_loans,
        summary.bound_unique_cases,
    )?;
    if summary.deed_truth_unbound_cases != expected_unbound {
        return Err(summary_invariant_error(
            "truth_binding",
            "deed_truth_unbound_cases",
            expected_unbound,
            summary.deed_truth_unbound_cases,
        ));
    }
    if summary.round_amount_unique > summary.round_amount_loans {
        return Err(summary_invariant_error(
            "truth_binding",
            "round_amount_unique",
            summary.round_amount_loans,
            summary.round_amount_unique,
        ));
    }
    Ok(())
}

fn checked_deed_truth_binding_difference(
    field: &'static str,
    total: u64,
    subset: u64,
) -> Result<u64, GeoPopulationError> {
    total
        .checked_sub(subset)
        .ok_or_else(|| summary_invariant_error("truth_binding", field, total, subset))
}

pub fn validate_deed_truth_plane_scope(
    planes: impl IntoIterator<Item = GeoTruthPlane>,
) -> Result<(), GeoPopulationError> {
    let unique = planes.into_iter().collect::<BTreeSet<_>>();
    if unique.contains(&GeoTruthPlane::DeedGrainInstrument) && unique.len() > 1 {
        let truth_planes = unique
            .iter()
            .map(|plane| format!("{plane:?}"))
            .collect::<Vec<_>>()
            .join(",");
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::DeedTruthPlanePooled,
            "Geo deed truth plane cannot be pooled with another truth plane",
            [("truth_planes", truth_planes)],
        ));
    }
    Ok(())
}

fn derive_deed_truth_inner(
    loans: &[GeoDeedTruthLoanRef],
    deeds: &[GeoDeedIndexRow],
    window_days: u32,
    mut source_pins: Vec<GeoDeedSourcePin>,
    deed_index_version: String,
) -> Result<GeoDeedTruthArtifact, GeoPopulationError> {
    if loans.is_empty() {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo deed truth derivation requires at least one loan",
            [("field", "loans")],
        ));
    }
    validate_sorted_deed_source_pins("source_pins", &source_pins, true)?;
    source_pins.sort();
    source_pins.dedup();
    let pinned_releases = source_pins
        .iter()
        .map(|pin| pin.source_release.as_str())
        .collect::<BTreeSet<_>>();

    let mut canonical_deeds = deeds.to_vec();
    canonical_deeds.sort_by(|left, right| left.instrument_id.cmp(&right.instrument_id));
    let mut prior_instrument_id: Option<&str> = None;
    for deed in &canonical_deeds {
        validate_deed_index_row(deed, &pinned_releases)?;
        if prior_instrument_id.is_some_and(|previous| previous == deed.instrument_id.as_str()) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo deed index rows contain a duplicate instrument_id",
                [("instrument_id", deed.instrument_id.as_str())],
            ));
        }
        prior_instrument_id = Some(deed.instrument_id.as_str());
    }

    let mut canonical_loans = loans.to_vec();
    canonical_loans.sort_by(|left, right| left.loan_id.cmp(&right.loan_id));
    let mut prior_loan_id: Option<&str> = None;
    let mut per_loan = Vec::with_capacity(canonical_loans.len());
    for loan in &canonical_loans {
        validate_deed_truth_loan_ref(loan)?;
        if prior_loan_id.is_some_and(|previous| previous == loan.loan_id.as_str()) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo deed truth loan refs contain a duplicate loan_id",
                [("loan_id", loan.loan_id.as_str())],
            ));
        }
        prior_loan_id = Some(loan.loan_id.as_str());
        per_loan.push(match_deed_truth_loan(loan, &canonical_deeds, window_days)?);
    }

    let artifact = GeoDeedTruthArtifact {
        version: CANON_GEO_DEED_TRUTH_VERSION.to_string(),
        deed_index_version,
        truth_plane: GeoTruthPlane::DeedGrainInstrument,
        proof_class: deed_truth_proof_class(&source_pins)?,
        window_days,
        source_pins,
        summary: summarize_deed_truth(&per_loan)?,
        per_loan,
    };
    validate_deed_truth_artifact(&artifact)?;
    Ok(artifact)
}

#[derive(Debug)]
struct GeoDeedTruthCandidate {
    instrument_id: String,
    parcel_ids: Vec<String>,
    date_delta_days: u32,
    source_pins: Vec<GeoDeedSourcePin>,
}

fn match_deed_truth_loan(
    loan: &GeoDeedTruthLoanRef,
    deeds: &[GeoDeedIndexRow],
    window_days: u32,
) -> Result<GeoDeedTruthLoanMatch, GeoPopulationError> {
    let origination_date = parse_deed_date("loans[].origination_date", &loan.origination_date)?;
    let mut amount_date_delta_days = Vec::new();
    let mut candidates = Vec::new();
    for deed in deeds {
        if deed.instrument_type != GeoDeedInstrumentType::Mortgage
            || deed.amount_cents != Some(loan.amount_cents)
        {
            continue;
        }
        let recording_date = parse_deed_date("deeds[].recording_date", &deed.recording_date)?;
        let delta_days = date_delta_days(origination_date, recording_date)?;
        if delta_days > window_days {
            continue;
        }
        amount_date_delta_days.push(delta_days);
        if deed.parcel_ids.is_empty() {
            continue;
        }
        candidates.push(GeoDeedTruthCandidate {
            instrument_id: deed.instrument_id.clone(),
            parcel_ids: deed.parcel_ids.clone(),
            date_delta_days: delta_days,
            source_pins: deed.source_pins.clone(),
        });
    }
    let amount_exact = !amount_date_delta_days.is_empty();
    let date_delta_days = amount_date_delta_days.into_iter().min();
    let round_amount = is_deed_round_amount(loan.amount_cents);
    let row = match candidates.len() {
        0 => GeoDeedTruthLoanMatch {
            loan_id: loan.loan_id.clone(),
            matched_instrument_ids: Vec::new(),
            parcel_ids: Vec::new(),
            match_kind: GeoDeedTruthMatchKind::NoMatch,
            amount_exact,
            date_delta_days,
            round_amount,
            source_pins: Vec::new(),
        },
        1 => {
            let candidate = candidates
                .pop()
                .expect("candidate length checked before pop");
            GeoDeedTruthLoanMatch {
                loan_id: loan.loan_id.clone(),
                matched_instrument_ids: vec![candidate.instrument_id],
                parcel_ids: sorted_unique_strings(candidate.parcel_ids),
                match_kind: GeoDeedTruthMatchKind::Unique,
                amount_exact,
                date_delta_days: Some(candidate.date_delta_days),
                round_amount,
                source_pins: sorted_unique_deed_source_pins(candidate.source_pins),
            }
        }
        _ => {
            let mut matched_instrument_ids = Vec::with_capacity(candidates.len());
            let mut candidate_source_pins = Vec::new();
            for candidate in candidates {
                matched_instrument_ids.push(candidate.instrument_id);
                candidate_source_pins.extend(candidate.source_pins);
            }
            matched_instrument_ids.sort();
            matched_instrument_ids.dedup();
            GeoDeedTruthLoanMatch {
                loan_id: loan.loan_id.clone(),
                matched_instrument_ids,
                parcel_ids: Vec::new(),
                match_kind: GeoDeedTruthMatchKind::NonUniqueDiscarded,
                amount_exact,
                date_delta_days,
                round_amount,
                source_pins: sorted_unique_deed_source_pins(candidate_source_pins),
            }
        }
    };
    validate_deed_truth_loan_match(&row)?;
    Ok(row)
}

fn collect_deed_source_pins(
    deeds: &[GeoDeedIndexRow],
) -> Result<Vec<GeoDeedSourcePin>, GeoPopulationError> {
    let mut source_pins = Vec::new();
    for deed in deeds {
        source_pins.extend(deed.source_pins.clone());
    }
    let source_pins = sorted_unique_deed_source_pins(source_pins);
    validate_sorted_deed_source_pins("source_pins", &source_pins, true)?;
    Ok(source_pins)
}

fn validate_deed_index_row(
    row: &GeoDeedIndexRow,
    pinned_releases: &BTreeSet<&str>,
) -> Result<(), GeoPopulationError> {
    validate_nonempty_canonical("deeds[].instrument_id", &row.instrument_id)?;
    validate_sorted_strings("deeds[].parcel_ids", &row.parcel_ids, false)?;
    parse_deed_date("deeds[].recording_date", &row.recording_date)?;
    if let Some(amount_cents) = row.amount_cents
        && amount_cents <= 0
    {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo deed index amount_cents must be positive when present",
            [
                ("field", "deeds[].amount_cents"),
                ("instrument_id", row.instrument_id.as_str()),
            ],
        ));
    }
    validate_lower_hex_digest("deeds[].lender_party_blake3", &row.lender_party_blake3)?;
    validate_lower_hex_digest("deeds[].borrower_party_blake3", &row.borrower_party_blake3)?;
    validate_nonempty_canonical("deeds[].source_release", &row.source_release)?;
    if !pinned_releases.contains(row.source_release.as_str()) {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo deed index row source_release is not pinned",
            [
                ("instrument_id", row.instrument_id.as_str()),
                ("source_release", row.source_release.as_str()),
            ],
        ));
    }
    validate_lower_hex_digest("deeds[].row_blake3", &row.row_blake3)?;
    validate_sorted_deed_source_pins("deeds[].source_pins", &row.source_pins, true)?;
    for pin in &row.source_pins {
        if !pinned_releases.contains(pin.source_release.as_str()) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo deed index row source pin release is not declared at the request level",
                [
                    ("instrument_id", row.instrument_id.as_str()),
                    ("source_release", pin.source_release.as_str()),
                ],
            ));
        }
    }
    if let Some(address) = &row.asserted_address {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::DeedTruthAddressJoinDetected,
            "Geo deed truth inputs must not carry clear address fields",
            [
                ("field", "deeds[].asserted_address"),
                ("instrument_id", row.instrument_id.as_str()),
                ("value", address.as_str()),
            ],
        ));
    }
    Ok(())
}

fn validate_deed_truth_loan_ref(loan: &GeoDeedTruthLoanRef) -> Result<(), GeoPopulationError> {
    validate_nonempty_canonical("loans[].loan_id", &loan.loan_id)?;
    if loan.amount_cents <= 0 {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo deed truth loan amount_cents must be positive",
            [
                ("field", "loans[].amount_cents"),
                ("loan_id", loan.loan_id.as_str()),
            ],
        ));
    }
    parse_deed_date("loans[].origination_date", &loan.origination_date)?;
    if let Some(address) = &loan.asserted_address {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::DeedTruthAddressJoinDetected,
            "Geo deed truth inputs must not carry clear address fields",
            [
                ("field", "loans[].asserted_address"),
                ("loan_id", loan.loan_id.as_str()),
                ("value", address.as_str()),
            ],
        ));
    }
    Ok(())
}

fn validate_deed_truth_loan_match(row: &GeoDeedTruthLoanMatch) -> Result<(), GeoPopulationError> {
    validate_nonempty_canonical("per_loan[].loan_id", &row.loan_id)?;
    validate_sorted_strings(
        "per_loan[].matched_instrument_ids",
        &row.matched_instrument_ids,
        false,
    )?;
    validate_sorted_strings("per_loan[].parcel_ids", &row.parcel_ids, false)?;
    validate_sorted_deed_source_pins("per_loan[].source_pins", &row.source_pins, false)?;
    match row.match_kind {
        GeoDeedTruthMatchKind::Unique => {
            if row.matched_instrument_ids.len() != 1 || row.parcel_ids.is_empty() {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo deed truth unique rows require one instrument and at least one parcel",
                    [("loan_id", row.loan_id.as_str())],
                ));
            }
            if !row.amount_exact || row.date_delta_days.is_none() || row.source_pins.is_empty() {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo deed truth unique rows must preserve amount/date/source evidence",
                    [("loan_id", row.loan_id.as_str())],
                ));
            }
        }
        GeoDeedTruthMatchKind::NonUniqueDiscarded => {
            if row.matched_instrument_ids.len() < 2 || !row.parcel_ids.is_empty() {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::DeedTruthNonUnique,
                    "Geo deed truth non-unique rows must be discarded from truth",
                    [("loan_id", row.loan_id.as_str())],
                ));
            }
            if !row.amount_exact || row.date_delta_days.is_none() {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo deed truth non-unique rows must preserve amount/date evidence",
                    [("loan_id", row.loan_id.as_str())],
                ));
            }
        }
        GeoDeedTruthMatchKind::NoMatch => {
            if !row.matched_instrument_ids.is_empty() || !row.parcel_ids.is_empty() {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo deed truth no-match rows must not carry truth instruments or parcels",
                    [("loan_id", row.loan_id.as_str())],
                ));
            }
            if !row.amount_exact && row.date_delta_days.is_some() {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo deed truth no-match date_delta_days requires an amount/date candidate",
                    [("loan_id", row.loan_id.as_str())],
                ));
            }
        }
    }
    Ok(())
}

fn summarize_deed_truth(
    per_loan: &[GeoDeedTruthLoanMatch],
) -> Result<GeoDeedTruthSummary, GeoPopulationError> {
    let mut summary = GeoDeedTruthSummary {
        loans: checked_len(per_loan.len(), "deed_truth.loans")?,
        unique: 0,
        non_unique_discarded: 0,
        no_match: 0,
        round_amount_loans: 0,
        round_amount_unique: 0,
        non_round_amount_loans: 0,
        non_round_amount_unique: 0,
    };
    for row in per_loan {
        if row.round_amount {
            checked_inc(
                &mut summary.round_amount_loans,
                "deed_truth.round_amount_loans",
            )?;
        } else {
            checked_inc(
                &mut summary.non_round_amount_loans,
                "deed_truth.non_round_amount_loans",
            )?;
        }
        match row.match_kind {
            GeoDeedTruthMatchKind::Unique => {
                checked_inc(&mut summary.unique, "deed_truth.unique")?;
                if row.round_amount {
                    checked_inc(
                        &mut summary.round_amount_unique,
                        "deed_truth.round_amount_unique",
                    )?;
                } else {
                    checked_inc(
                        &mut summary.non_round_amount_unique,
                        "deed_truth.non_round_amount_unique",
                    )?;
                }
            }
            GeoDeedTruthMatchKind::NonUniqueDiscarded => checked_inc(
                &mut summary.non_unique_discarded,
                "deed_truth.non_unique_discarded",
            )?,
            GeoDeedTruthMatchKind::NoMatch => {
                checked_inc(&mut summary.no_match, "deed_truth.no_match")?;
            }
        }
    }
    validate_deed_truth_summary(&summary)?;
    Ok(summary)
}

fn validate_deed_truth_summary(summary: &GeoDeedTruthSummary) -> Result<(), GeoPopulationError> {
    let outcomes = sum_u64(
        [
            summary.unique,
            summary.non_unique_discarded,
            summary.no_match,
        ],
        "deed_truth.outcomes",
    )?;
    if outcomes != summary.loans {
        return Err(summary_invariant_error(
            "deed_truth",
            "unique_non_unique_no_match",
            summary.loans,
            outcomes,
        ));
    }
    let strata = sum_u64(
        [summary.round_amount_loans, summary.non_round_amount_loans],
        "deed_truth.amount_strata",
    )?;
    if strata != summary.loans {
        return Err(summary_invariant_error(
            "deed_truth",
            "round_amount_strata",
            summary.loans,
            strata,
        ));
    }
    let unique_by_stratum = sum_u64(
        [summary.round_amount_unique, summary.non_round_amount_unique],
        "deed_truth.unique_by_stratum",
    )?;
    if unique_by_stratum != summary.unique {
        return Err(summary_invariant_error(
            "deed_truth",
            "unique_by_stratum",
            summary.unique,
            unique_by_stratum,
        ));
    }
    if summary.round_amount_unique > summary.round_amount_loans {
        return Err(summary_invariant_error(
            "deed_truth",
            "round_amount_unique",
            summary.round_amount_loans,
            summary.round_amount_unique,
        ));
    }
    if summary.non_round_amount_unique > summary.non_round_amount_loans {
        return Err(summary_invariant_error(
            "deed_truth",
            "non_round_amount_unique",
            summary.non_round_amount_loans,
            summary.non_round_amount_unique,
        ));
    }
    Ok(())
}

fn validate_sorted_deed_source_pins(
    field: &str,
    pins: &[GeoDeedSourcePin],
    require_nonempty: bool,
) -> Result<(), GeoPopulationError> {
    if require_nonempty && pins.is_empty() {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo deed truth source pins must be non-empty",
            [("field", field.to_string())],
        ));
    }
    let mut previous: Option<&GeoDeedSourcePin> = None;
    for pin in pins {
        validate_deed_source_pin(field, pin)?;
        if previous.is_some_and(|previous| previous >= pin) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo deed truth source pins must be sorted and distinct",
                [("field", field.to_string())],
            ));
        }
        previous = Some(pin);
    }
    Ok(())
}

fn validate_deed_source_pin(field: &str, pin: &GeoDeedSourcePin) -> Result<(), GeoPopulationError> {
    validate_nonempty_canonical_dynamic(&format!("{field}[].source_table"), &pin.source_table)?;
    validate_nonempty_canonical_dynamic(&format!("{field}[].natural_key"), &pin.natural_key)?;
    validate_nonempty_canonical_dynamic(&format!("{field}[].source_release"), &pin.source_release)?;
    validate_nonempty_canonical_dynamic(
        &format!("{field}[].source_release_field"),
        &pin.source_release_field,
    )?;
    validate_nonempty_canonical_dynamic(&format!("{field}[].release_dt"), &pin.release_dt)?;
    validate_nonempty_canonical_dynamic(
        &format!("{field}[].release_dt_field"),
        &pin.release_dt_field,
    )?;
    parse_deed_date(&format!("{field}[].release_dt"), &pin.release_dt)?;
    validate_lower_hex_digest(
        &format!("{field}[].source_content_sha256"),
        &pin.source_content_sha256,
    )?;
    validate_nonempty_canonical_dynamic(
        &format!("{field}[].source_content_sha256_field"),
        &pin.source_content_sha256_field,
    )?;
    validate_nonempty_canonical_dynamic(&format!("{field}[].parser_version"), &pin.parser_version)?;
    validate_nonempty_canonical_dynamic(
        &format!("{field}[].parser_version_field"),
        &pin.parser_version_field,
    )?;
    validate_nonempty_canonical_dynamic(&format!("{field}[].license_terms"), &pin.license_terms)?;
    validate_nonempty_canonical_dynamic(
        &format!("{field}[].license_terms_field"),
        &pin.license_terms_field,
    )?;
    validate_nonempty_canonical_dynamic(
        &format!("{field}[].attribution_text"),
        &pin.attribution_text,
    )?;
    validate_nonempty_canonical_dynamic(
        &format!("{field}[].attribution_text_field"),
        &pin.attribution_text_field,
    )?;
    match (&pin.source_file, &pin.source_file_field) {
        (Some(source_file), Some(source_file_field)) => {
            validate_nonempty_canonical_dynamic(&format!("{field}[].source_file"), source_file)?;
            validate_nonempty_canonical_dynamic(
                &format!("{field}[].source_file_field"),
                source_file_field,
            )?;
        }
        (None, None) => {}
        _ => {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo deed truth source file pins require value and field name together",
                [("field", format!("{field}[].source_file"))],
            ));
        }
    }
    Ok(())
}

fn sorted_unique_deed_source_pins(mut pins: Vec<GeoDeedSourcePin>) -> Vec<GeoDeedSourcePin> {
    pins.sort();
    pins.dedup();
    pins
}

fn deed_truth_proof_class(
    source_pins: &[GeoDeedSourcePin],
) -> Result<GeoDeedTruthProofClass, GeoPopulationError> {
    source_pins
        .iter()
        .map(|pin| pin.proof_class)
        .min()
        .ok_or_else(|| {
            GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo deed truth artifact requires at least one source pin",
                [("field", "source_pins")],
            )
        })
}

fn sorted_unique_strings(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

fn validate_sorted_strings(
    field: &str,
    values: &[String],
    require_nonempty: bool,
) -> Result<(), GeoPopulationError> {
    if require_nonempty && values.is_empty() {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo deed truth string list must be non-empty",
            [("field", field.to_string())],
        ));
    }
    let mut previous: Option<&str> = None;
    for value in values {
        validate_nonempty_canonical_dynamic(field, value)?;
        if previous.is_some_and(|previous| previous >= value.as_str()) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo deed truth string lists must be sorted and distinct",
                [("field", field.to_string()), ("value", value.clone())],
            ));
        }
        previous = Some(value.as_str());
    }
    Ok(())
}

fn validate_nonempty_canonical_dynamic(field: &str, value: &str) -> Result<(), GeoPopulationError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo deed truth strings must be non-empty and canonical",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn validate_lower_hex_digest(field: &str, value: &str) -> Result<(), GeoPopulationError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo deed truth digests must be lowercase fixed-width hex",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn parse_deed_date(field: &str, value: &str) -> Result<NaiveDate, GeoPopulationError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|error| {
        GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo deed truth dates must use YYYY-MM-DD",
            [
                ("field", field.to_string()),
                ("value", value.to_string()),
                ("error", error.to_string()),
            ],
        )
    })
}

fn date_delta_days(left: NaiveDate, right: NaiveDate) -> Result<u32, GeoPopulationError> {
    u32::try_from((left - right).num_days().unsigned_abs()).map_err(|_| {
        GeoPopulationError::new(
            GeoPopulationErrorCode::ArithmeticOverflow,
            "Geo deed truth date delta overflowed",
            [("field", "date_delta_days")],
        )
    })
}

fn is_deed_round_amount(amount_cents: i64) -> bool {
    amount_cents % CANON_GEO_DEED_ROUND_AMOUNT_LATTICE_CENTS == 0
}

fn validate_case(case: &mut GeoLabeledCompositionCase) -> Result<(), GeoPopulationError> {
    if case.id.is_empty() || case.id.trim() != case.id {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo population case identifiers must be non-empty and canonical",
            [("case_id", case.id.as_str())],
        ));
    }
    case.truth.parcels.sort();
    case.truth.buildings.sort();
    if case.truth.parcels.is_empty() && case.truth.buildings.is_empty() {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo population truth must contain at least one member",
            [("case_id", case.id.as_str())],
        ));
    }
    reject_duplicates("truth.parcels", &case.truth.parcels)?;
    reject_duplicates("truth.buildings", &case.truth.buildings)
}

fn canonical_truth_reach_overlay_map(
    overlays: &[GeoPopulationCaseTruthReachByGrain],
) -> Result<BTreeMap<String, Vec<GeoTruthReachByGrain>>, GeoPopulationError> {
    let mut by_case = BTreeMap::new();
    for overlay in overlays {
        validate_nonempty_canonical("truth_reach.case_id", &overlay.case_id)?;
        let mut reaches = overlay.truth_reach_by_grain.clone();
        reaches.sort_by_key(|reach| reach.grain);
        validate_truth_reach_by_grain("truth_reach_by_grain", &reaches)?;
        if by_case.insert(overlay.case_id.clone(), reaches).is_some() {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo population truth reach overlay repeats a case identifier",
                [("case_id", overlay.case_id.as_str())],
            ));
        }
    }
    Ok(by_case)
}

fn validate_truth_reach_overlay_case_ids(
    overlays: &BTreeMap<String, Vec<GeoTruthReachByGrain>>,
    cases: &[GeoLabeledCompositionCase],
) -> Result<(), GeoPopulationError> {
    let case_ids = cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    for case_id in overlays.keys() {
        if !case_ids.contains(case_id.as_str()) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo population truth reach overlay references an unknown case",
                [("case_id", case_id.as_str())],
            ));
        }
    }
    Ok(())
}

fn validate_truth_reach_by_grain(
    field: &'static str,
    reaches: &[GeoTruthReachByGrain],
) -> Result<(), GeoPopulationError> {
    let mut previous = None;
    for reach in reaches {
        if let Some(previous_grain) = previous
            && previous_grain >= reach.grain
        {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo population truth reach by grain must be sorted and unique",
                [
                    ("field", field.to_string()),
                    ("grain", format!("{:?}", reach.grain)),
                ],
            ));
        }
        previous = Some(reach.grain);
        let expected =
            truth_grain_reach_status(reach.truth_members, reach.truth_members_in_universe)?;
        if reach.candidate_reach != expected {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo population truth reach by grain has inconsistent counts",
                [
                    ("field", field.to_string()),
                    ("grain", format!("{:?}", reach.grain)),
                    (
                        "declared",
                        candidate_reach_name(reach.candidate_reach).to_string(),
                    ),
                    ("expected", candidate_reach_name(expected).to_string()),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_candidate_truth_gate(gate: &GeoCandidateTruthGate) -> Result<(), GeoPopulationError> {
    validate_nonempty_canonical("gate_id", &gate.gate_id)?;
    if gate.gate_id != CANON_GEO_FROZEN_E4_H7_GATE_ID {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo candidate/truth handoff gate_id is not the frozen E4/H7 gate",
            [
                ("actual", gate.gate_id.as_str()),
                ("expected", CANON_GEO_FROZEN_E4_H7_GATE_ID),
            ],
        ));
    }
    if gate.kind != GeoCandidateTruthGateKind::FrozenE4H7ReleaseValidatedMultiParcelSubjects {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo candidate/truth handoff gate kind is not the frozen E4/H7 subject gate",
            [("gate_id", gate.gate_id.as_str())],
        ));
    }
    if gate.required_subjects != CANON_GEO_FROZEN_E4_H7_REQUIRED_SUBJECTS {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo candidate/truth handoff gate required_subjects must equal the frozen E4/H7 count",
            [
                ("actual", gate.required_subjects.to_string()),
                (
                    "expected",
                    CANON_GEO_FROZEN_E4_H7_REQUIRED_SUBJECTS.to_string(),
                ),
            ],
        ));
    }
    let expected_release_ids = frozen_e4_h7_required_release_ids()
        .iter()
        .map(|release_id| (*release_id).to_string())
        .collect::<Vec<_>>();
    for release_id in &gate.required_release_ids {
        validate_nonempty_canonical("gate.required_release_id", release_id)?;
    }
    if gate.required_release_ids != expected_release_ids {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo candidate/truth handoff gate required_release_ids must equal the pinned H7 release pair",
            [
                ("actual", gate.required_release_ids.join(",")),
                ("expected", expected_release_ids.join(",")),
            ],
        ));
    }
    Ok(())
}

fn frozen_e4_h7_required_release_ids() -> [&'static str; 2] {
    [
        CANON_GEO_FROZEN_E4_H7_RELEASE_26V1,
        CANON_GEO_FROZEN_E4_H7_RELEASE_26V2,
    ]
}

fn validate_candidate_truth_row_release_ids(
    gate: &GeoCandidateTruthGate,
    rows: &[GeoCandidateTruthHandoffRow],
) -> Result<(), GeoPopulationError> {
    let required_release_ids = gate
        .required_release_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for row in rows {
        if !required_release_ids.contains(row.release_id.as_str()) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo candidate/truth handoff row release_id is outside the pinned H7 release pair",
                [
                    ("row_id", row.row_id.clone()),
                    ("release_id", row.release_id.clone()),
                    ("expected", gate.required_release_ids.join(",")),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_candidate_truth_evaluation_row_release_ids(
    gate: &GeoCandidateTruthGate,
    rows: &[GeoCandidateTruthCaseEvaluation],
) -> Result<(), GeoPopulationError> {
    let required_release_ids = gate
        .required_release_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for row in rows {
        if !required_release_ids.contains(row.release_id.as_str()) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo candidate/truth evaluation row release_id is outside the pinned H7 release pair",
                [
                    ("row_id", row.row_id.clone()),
                    ("release_id", row.release_id.clone()),
                    ("expected", gate.required_release_ids.join(",")),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_candidate_truth_logical_subject_bindings(
    bindings: &[GeoCandidateTruthLogicalSubjectBinding],
    rows: &[GeoCandidateTruthHandoffRow],
) -> Result<BTreeMap<String, String>, GeoPopulationError> {
    if bindings.is_empty() {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo candidate/truth handoff requires explicit logical subject bindings",
            [("field", "logical_subject_bindings")],
        ));
    }

    let row_ids = rows
        .iter()
        .map(|row| row.row_id.clone())
        .collect::<BTreeSet<_>>();
    let mut logical_subject_ids = BTreeSet::new();
    let mut row_logical_subjects = BTreeMap::new();
    for binding in bindings {
        validate_nonempty_canonical("logical_subject_id", &binding.logical_subject_id)?;
        if binding.row_ids.is_empty() {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo candidate/truth logical subject binding must contain at least one row",
                [("logical_subject_id", binding.logical_subject_id.as_str())],
            ));
        }
        if !logical_subject_ids.insert(binding.logical_subject_id.clone()) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo candidate/truth handoff repeats a logical subject binding",
                [("logical_subject_id", binding.logical_subject_id.as_str())],
            ));
        }
        let mut binding_row_ids = BTreeSet::new();
        for row_id in &binding.row_ids {
            validate_nonempty_canonical("binding.row_id", row_id)?;
            if !row_ids.contains(row_id) {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo candidate/truth logical subject binding references an unknown row",
                    [
                        ("logical_subject_id", binding.logical_subject_id.as_str()),
                        ("row_id", row_id.as_str()),
                    ],
                ));
            }
            if !binding_row_ids.insert(row_id.clone()) {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo candidate/truth logical subject binding repeats a row",
                    [
                        ("logical_subject_id", binding.logical_subject_id.as_str()),
                        ("row_id", row_id.as_str()),
                    ],
                ));
            }
            if let Some(previous) =
                row_logical_subjects.insert(row_id.clone(), binding.logical_subject_id.clone())
            {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo candidate/truth handoff assigns one row to multiple logical subjects",
                    [
                        ("row_id", row_id.clone()),
                        ("previous_logical_subject_id", previous),
                        (
                            "current_logical_subject_id",
                            binding.logical_subject_id.clone(),
                        ),
                    ],
                ));
            }
        }
    }

    if row_logical_subjects.len() != row_ids.len() {
        let missing = row_ids
            .iter()
            .find(|row_id| !row_logical_subjects.contains_key(*row_id))
            .expect("row binding length mismatch implies a missing row");
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo candidate/truth handoff has an unbound release row",
            [("row_id", missing.as_str())],
        ));
    }

    Ok(row_logical_subjects)
}

fn validate_candidate_truth_handoff_row(
    row: &mut GeoCandidateTruthHandoffRow,
) -> Result<(), GeoPopulationError> {
    validate_nonempty_canonical("row_id", &row.row_id)?;
    validate_nonempty_canonical("subject_id", &row.subject_id)?;
    validate_nonempty_canonical("release_id", &row.release_id)?;
    row.truth.parcels.sort();
    row.truth.buildings.sort();
    if row.truth.parcels.is_empty() && row.truth.buildings.is_empty() {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo candidate/truth handoff row truth must contain at least one member",
            [("row_id", row.row_id.as_str())],
        ));
    }
    reject_duplicates("truth.parcels", &row.truth.parcels)?;
    reject_duplicates("truth.buildings", &row.truth.buildings)?;

    if let Some(composition_request) = &mut row.composition_request {
        let canonical_request =
            canonicalize_composition_request(composition_request).map_err(map_composition_error)?;
        let truth_members = checked_member_count(
            row.truth.parcels.len(),
            row.truth.buildings.len(),
            "truth_members",
        )?;
        let truth_members_in_universe =
            count_truth_in_universe(&row.truth, &canonical_request.universe)?;
        let expected_reach = candidate_reach_status(truth_members, truth_members_in_universe)?;
        if row.candidate_reach != expected_reach {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo candidate/truth handoff row declared candidate reach does not match the bounded candidate universe",
                [
                    ("row_id", row.row_id.as_str()),
                    ("declared", candidate_reach_name(row.candidate_reach)),
                    ("computed", candidate_reach_name(expected_reach)),
                ],
            ));
        }
        *composition_request = canonical_request;
    } else if row.candidate_reach != GeoCandidateReachStatus::None {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo candidate/truth handoff row without a composition request must declare no candidate reach",
            [
                ("row_id", row.row_id.as_str()),
                ("declared", candidate_reach_name(row.candidate_reach)),
            ],
        ));
    }
    Ok(())
}

fn evaluate_candidate_truth_row(
    logical_subject_id: String,
    row: GeoCandidateTruthHandoffRow,
) -> Result<GeoCandidateTruthCaseEvaluation, GeoPopulationError> {
    let truth_parcel_members = checked_len(row.truth.parcels.len(), "truth_parcel_members")?;
    let truth_building_members = checked_len(row.truth.buildings.len(), "truth_building_members")?;
    let truth_members = checked_member_count(
        row.truth.parcels.len(),
        row.truth.buildings.len(),
        "truth_members",
    )?;
    let Some(composition_request) = row.composition_request else {
        let evaluation = GeoCandidateTruthCaseEvaluation {
            row_id: row.row_id,
            logical_subject_id,
            subject_id: row.subject_id,
            release_id: row.release_id,
            truth_plane: row.truth_plane,
            status: GeoCandidateTruthRowStatus::UpstreamNoCandidateRequest,
            candidate_reach: GeoCandidateReachStatus::None,
            composition_request_digest: None,
            solver_digest: None,
            candidate_members: 0,
            truth_members,
            truth_parcel_members,
            truth_building_members,
            truth_members_in_universe: 0,
            representation_relative_exact: false,
            residual_model_count: None,
            residual_count_complete: false,
            residual_count_saturated: false,
            solver_truth_scored: false,
            truth_model_in_residual: None,
            solver_abstained: true,
            claim_abstained: true,
            false_merge: false,
            rho_falsification: false,
        };
        validate_candidate_truth_case_evaluation(&evaluation)?;
        return Ok(evaluation);
    };

    let composition_request_digest = Some(composition_request_digest(&composition_request)?);
    let candidate_members = checked_member_count(
        composition_request.universe.parcels.len(),
        composition_request.universe.buildings.len(),
        "candidate_members",
    )?;
    let truth_members_in_universe =
        count_truth_in_universe(&row.truth, &composition_request.universe)?;
    let solved = solve_composition(&composition_request);

    let evaluation = match solved {
        Err(error) if error.code == GeoCompositionErrorCode::BudgetExceeded => {
            GeoCandidateTruthCaseEvaluation {
                row_id: row.row_id,
                logical_subject_id,
                subject_id: row.subject_id,
                release_id: row.release_id,
                truth_plane: row.truth_plane,
                status: GeoCandidateTruthRowStatus::AssignmentBudgetExceeded,
                candidate_reach: row.candidate_reach,
                composition_request_digest,
                solver_digest: None,
                candidate_members,
                truth_members,
                truth_parcel_members,
                truth_building_members,
                truth_members_in_universe,
                representation_relative_exact: false,
                residual_model_count: None,
                residual_count_complete: false,
                residual_count_saturated: false,
                solver_truth_scored: false,
                truth_model_in_residual: None,
                solver_abstained: true,
                claim_abstained: true,
                false_merge: false,
                rho_falsification: false,
            }
        }
        Err(error) => return Err(map_composition_error(error)),
        Ok(artifact) => {
            let solver_digest =
                blake3::hash(&canonical_composition_bytes(&artifact).map_err(|error| {
                    GeoPopulationError::new(
                        GeoPopulationErrorCode::Composition,
                        "Geo composition artifact could not be serialized",
                        [("error", error.to_string())],
                    )
                })?)
                .to_hex()
                .to_string();
            let status = match artifact.status {
                GeoCompositionStatus::Resolved => GeoCandidateTruthRowStatus::Resolved,
                GeoCompositionStatus::Ambiguous => GeoCandidateTruthRowStatus::Ambiguous,
                GeoCompositionStatus::Conflict => GeoCandidateTruthRowStatus::Conflict,
                GeoCompositionStatus::BudgetFallback => {
                    GeoCandidateTruthRowStatus::ComponentBudgetFallback
                }
            };
            let solver_truth_scored = row.candidate_reach == GeoCandidateReachStatus::Full;
            let truth_model_in_residual = if solver_truth_scored {
                Some(
                    match model_satisfies_request(&composition_request, &row.truth) {
                        Ok(satisfied) => satisfied,
                        Err(error) => return Err(map_composition_error(error)),
                    },
                )
            } else {
                None
            };
            let residual_count_complete = artifact.summary.residual_model_count_complete;
            let residual_count_saturated = artifact.summary.residual_model_count_saturated;
            let false_merge = status == GeoCandidateTruthRowStatus::Resolved
                && truth_model_in_residual == Some(false);
            let rho_falsification = truth_model_in_residual == Some(false);
            let solver_abstained = is_candidate_truth_solver_abstention_status(status);
            let claim_abstained = is_candidate_truth_claim_abstention_status(
                status,
                row.candidate_reach,
                truth_model_in_residual,
            );
            GeoCandidateTruthCaseEvaluation {
                row_id: row.row_id,
                logical_subject_id,
                subject_id: row.subject_id,
                release_id: row.release_id,
                truth_plane: row.truth_plane,
                status,
                candidate_reach: row.candidate_reach,
                composition_request_digest,
                solver_digest: Some(solver_digest),
                candidate_members,
                truth_members,
                truth_parcel_members,
                truth_building_members,
                truth_members_in_universe,
                representation_relative_exact: is_representation_relative_exact(
                    status,
                    residual_count_complete,
                    residual_count_saturated,
                ),
                residual_model_count: residual_count_complete
                    .then_some(artifact.summary.residual_model_count),
                residual_count_complete,
                residual_count_saturated,
                solver_truth_scored,
                truth_model_in_residual,
                solver_abstained,
                claim_abstained,
                false_merge,
                rho_falsification,
            }
        }
    };
    validate_candidate_truth_case_evaluation(&evaluation)?;
    Ok(evaluation)
}

fn validate_nonempty_canonical(field: &'static str, value: &str) -> Result<(), GeoPopulationError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo candidate/truth handoff identifiers must be non-empty and canonical",
            [(field, value)],
        ));
    }
    Ok(())
}

fn composition_request_digest(
    request: &GeoCompositionRequest,
) -> Result<String, GeoPopulationError> {
    serde_json::to_vec(request)
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
        .map_err(|error| {
            GeoPopulationError::new(
                GeoPopulationErrorCode::Composition,
                "Geo composition request could not be serialized",
                [("error", error.to_string())],
            )
        })
}

fn composition_model_digest(model: &GeoCompositionModel) -> Result<String, GeoPopulationError> {
    serde_json::to_vec(model)
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
        .map_err(|error| {
            GeoPopulationError::new(
                GeoPopulationErrorCode::Composition,
                "Geo composition truth model could not be serialized",
                [("error", error.to_string())],
            )
        })
}

fn candidate_reach_name(status: GeoCandidateReachStatus) -> &'static str {
    match status {
        GeoCandidateReachStatus::Full => "full",
        GeoCandidateReachStatus::Partial => "partial",
        GeoCandidateReachStatus::None => "none",
    }
}

fn evidence_coverage_name(status: GeoEvidenceCoverageStatus) -> &'static str {
    match status {
        GeoEvidenceCoverageStatus::NoObservations => "no_observations",
        GeoEvidenceCoverageStatus::DiagnosticOnly => "diagnostic_only",
        GeoEvidenceCoverageStatus::SoftPreferenceOnly => "soft_preference_only",
        GeoEvidenceCoverageStatus::SoftAndDiagnosticOnly => "soft_and_diagnostic_only",
        GeoEvidenceCoverageStatus::HardConstraintPresent => "hard_constraint_present",
    }
}

fn reject_duplicates(field: &str, values: &[String]) -> Result<(), GeoPopulationError> {
    for pair in values.windows(2) {
        if pair[0] == pair[1] {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo population truth contains a duplicate member",
                [("field", field), ("member_id", pair[0].as_str())],
            ));
        }
    }
    Ok(())
}

fn count_truth_in_universe(
    truth: &GeoCompositionModel,
    universe: &super::composition::GeoCompositionUniverse,
) -> Result<u64, GeoPopulationError> {
    let parcels = truth
        .parcels
        .iter()
        .filter(|id| universe.parcels.binary_search(id).is_ok())
        .count();
    let buildings = truth
        .buildings
        .iter()
        .filter(|id| {
            universe
                .buildings
                .binary_search_by(|building| building.id.cmp(id))
                .is_ok()
        })
        .count();
    checked_member_count(parcels, buildings, "truth_members_in_universe")
}

fn score_backbone(
    backbone: &GeoCompositionBackbone,
    truth: &GeoCompositionModel,
) -> Result<(u64, u64), GeoPopulationError> {
    let true_parcels = backbone
        .parcels
        .iter()
        .filter(|id| truth.parcels.binary_search(id).is_ok())
        .count();
    let true_buildings = backbone
        .buildings
        .iter()
        .filter(|id| truth.buildings.binary_search(id).is_ok())
        .count();
    let total = checked_member_count(
        backbone.parcels.len(),
        backbone.buildings.len(),
        "backbone_members",
    )?;
    let true_count = checked_member_count(
        true_parcels,
        true_buildings,
        "backbone_true_positive_members",
    )?;
    let false_count = total
        .checked_sub(true_count)
        .ok_or_else(|| GeoPopulationError::overflow("backbone_false_positive_members"))?;
    Ok((true_count, false_count))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GeoEvidenceMetrics {
    coverage: GeoEvidenceCoverageStatus,
    observations: u64,
    records: u64,
    hard_constraints: u64,
    soft_preferences: u64,
    diagnostic_observations: u64,
}

fn evidence_metrics(
    admissions: &[super::evidence::GeoEvidenceAdmission],
) -> Result<GeoEvidenceMetrics, GeoPopulationError> {
    let mut records = 0_u64;
    let mut hard_constraints = 0_u64;
    let mut soft_preferences = 0_u64;
    let mut diagnostic_observations = 0_u64;
    for admission in admissions {
        checked_add(
            &mut records,
            checked_len(admission.source_records.len(), "evidence_records")?,
            "evidence_records",
        )?;
        match admission.disposition {
            GeoEvidenceDisposition::HardConstraint => {
                checked_inc(&mut hard_constraints, "hard_constraint_observations")?;
            }
            GeoEvidenceDisposition::SoftPreference => {
                checked_inc(&mut soft_preferences, "soft_preference_observations")?;
            }
            GeoEvidenceDisposition::DiagnosticOnly => {
                checked_inc(&mut diagnostic_observations, "diagnostic_observations")?;
            }
        }
    }
    let observations = checked_len(admissions.len(), "evidence_observations")?;
    let coverage = match (
        observations,
        hard_constraints > 0,
        soft_preferences > 0,
        diagnostic_observations > 0,
    ) {
        (0, _, _, _) => GeoEvidenceCoverageStatus::NoObservations,
        (_, true, _, _) => GeoEvidenceCoverageStatus::HardConstraintPresent,
        (_, false, true, true) => GeoEvidenceCoverageStatus::SoftAndDiagnosticOnly,
        (_, false, true, false) => GeoEvidenceCoverageStatus::SoftPreferenceOnly,
        (_, false, false, _) => GeoEvidenceCoverageStatus::DiagnosticOnly,
    };
    Ok(GeoEvidenceMetrics {
        coverage,
        observations,
        records,
        hard_constraints,
        soft_preferences,
        diagnostic_observations,
    })
}

fn candidate_reach_status(
    truth_members: u64,
    truth_members_in_universe: u64,
) -> Result<GeoCandidateReachStatus, GeoPopulationError> {
    if truth_members == 0 || truth_members_in_universe > truth_members {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo population truth membership counts are internally inconsistent",
            [
                ("truth_members", truth_members.to_string()),
                (
                    "truth_members_in_universe",
                    truth_members_in_universe.to_string(),
                ),
            ],
        ));
    }
    if truth_members == truth_members_in_universe {
        Ok(GeoCandidateReachStatus::Full)
    } else if truth_members_in_universe == 0 {
        Ok(GeoCandidateReachStatus::None)
    } else {
        Ok(GeoCandidateReachStatus::Partial)
    }
}

fn truth_grain_reach_status(
    truth_members: u64,
    truth_members_in_universe: u64,
) -> Result<GeoCandidateReachStatus, GeoPopulationError> {
    if truth_members_in_universe > truth_members {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo population truth grain counts are internally inconsistent",
            [
                ("truth_members", truth_members.to_string()),
                (
                    "truth_members_in_universe",
                    truth_members_in_universe.to_string(),
                ),
            ],
        ));
    }
    if truth_members == 0 {
        return Ok(GeoCandidateReachStatus::None);
    }
    if truth_members == truth_members_in_universe {
        Ok(GeoCandidateReachStatus::Full)
    } else if truth_members_in_universe == 0 {
        Ok(GeoCandidateReachStatus::None)
    } else {
        Ok(GeoCandidateReachStatus::Partial)
    }
}

fn is_abstention_status(status: GeoPopulationCaseStatus) -> bool {
    matches!(
        status,
        GeoPopulationCaseStatus::Ambiguous
            | GeoPopulationCaseStatus::Conflict
            | GeoPopulationCaseStatus::AssignmentBudgetExceeded
            | GeoPopulationCaseStatus::ComponentBudgetFallback
    )
}

fn resolved_claim_from_artifact(
    artifact: &GeoCompositionArtifact,
    hard_constraint_count: usize,
) -> Option<GeoResolvedClaim> {
    artifact.resolved_claim.clone().or_else(|| {
        if artifact.status != GeoCompositionStatus::Resolved {
            return None;
        }
        let claim_class =
            if hard_constraint_count == 0 && artifact.summary.hard_constraint_evaluations == 0 {
                GeoResolvedClaimClass::StructurallyForced
            } else {
                GeoResolvedClaimClass::EvidentiallySupported
            };
        Some(GeoResolvedClaim {
            claim_class,
            candidate_members: artifact.summary.parcel_candidates
                + artifact.summary.building_candidates,
            parcel_candidates: artifact.summary.parcel_candidates,
            building_candidates: artifact.summary.building_candidates,
            hard_constraint_count,
            hard_constraint_evaluations: artifact.summary.hard_constraint_evaluations,
        })
    })
}

fn is_residual_count_exact_case(case: &GeoPopulationCaseEvaluation) -> bool {
    case.residual_count_complete && !case.residual_count_saturated
}

fn e4_plane_scores<'a>(
    cases: impl IntoIterator<Item = &'a GeoPopulationCaseEvaluation>,
) -> Result<GeoE4GatePlaneScores, GeoPopulationError> {
    let mut scores = GeoE4GatePlaneScores::default();
    for case in cases {
        checked_inc(&mut scores.coverage.cases, "e4.coverage.cases")?;
        checked_inc(
            &mut scores.coverage.population_eligible_cases,
            "e4.coverage.population_eligible_cases",
        )?;
        checked_inc(
            &mut scores.candidate_reach.evaluated_cases,
            "e4.candidate_reach.evaluated_cases",
        )?;
        checked_add(
            &mut scores.cost.candidate_members,
            case.candidate_members,
            "e4.cost.candidate_members",
        )?;
        scores.cost.max_candidate_members = scores
            .cost
            .max_candidate_members
            .max(case.candidate_members);
        if let Some(count) = case.residual_model_count {
            scores.cost.max_residual_model_count = Some(
                scores
                    .cost
                    .max_residual_model_count
                    .map_or(count, |current| current.max(count)),
            );
        }
        match case.evidence_coverage {
            GeoEvidenceCoverageStatus::NoObservations => checked_inc(
                &mut scores.coverage.evidence_no_observation_cases,
                "e4.coverage.evidence_no_observation_cases",
            )?,
            GeoEvidenceCoverageStatus::DiagnosticOnly => {
                checked_inc(
                    &mut scores.coverage.evidence_diagnostic_only_cases,
                    "e4.coverage.evidence_diagnostic_only_cases",
                )?;
                checked_inc(
                    &mut scores.admission.diagnostic_only_cases,
                    "e4.admission.diagnostic_only_cases",
                )?;
            }
            GeoEvidenceCoverageStatus::SoftPreferenceOnly => {
                checked_inc(
                    &mut scores.coverage.evidence_soft_preference_only_cases,
                    "e4.coverage.evidence_soft_preference_only_cases",
                )?;
                checked_inc(
                    &mut scores.admission.soft_preference_only_cases,
                    "e4.admission.soft_preference_only_cases",
                )?;
            }
            GeoEvidenceCoverageStatus::SoftAndDiagnosticOnly => {
                checked_inc(
                    &mut scores.coverage.evidence_soft_and_diagnostic_only_cases,
                    "e4.coverage.evidence_soft_and_diagnostic_only_cases",
                )?;
                checked_inc(
                    &mut scores.admission.soft_and_diagnostic_only_cases,
                    "e4.admission.soft_and_diagnostic_only_cases",
                )?;
            }
            GeoEvidenceCoverageStatus::HardConstraintPresent => {
                checked_inc(
                    &mut scores.coverage.evidence_hard_constraint_cases,
                    "e4.coverage.evidence_hard_constraint_cases",
                )?;
                checked_inc(
                    &mut scores.admission.hard_constraint_cases,
                    "e4.admission.hard_constraint_cases",
                )?;
            }
        }
        match case.candidate_reach {
            GeoCandidateReachStatus::Full => checked_inc(
                &mut scores.candidate_reach.full_cases,
                "e4.candidate_reach.full_cases",
            )?,
            GeoCandidateReachStatus::Partial => {
                checked_inc(
                    &mut scores.candidate_reach.partial_cases,
                    "e4.candidate_reach.partial_cases",
                )?;
                checked_inc(
                    &mut scores.candidate_reach.recall_failure_cases,
                    "e4.candidate_reach.recall_failure_cases",
                )?;
            }
            GeoCandidateReachStatus::None => {
                checked_inc(
                    &mut scores.candidate_reach.none_cases,
                    "e4.candidate_reach.none_cases",
                )?;
                checked_inc(
                    &mut scores.candidate_reach.recall_failure_cases,
                    "e4.candidate_reach.recall_failure_cases",
                )?;
            }
        }
        if case.solver_digest.is_some() {
            checked_inc(
                &mut scores.solver_exactness.solver_artifact_cases,
                "e4.solver_exactness.solver_artifact_cases",
            )?;
            checked_inc(
                &mut scores.cost.solver_artifact_cases,
                "e4.cost.solver_artifact_cases",
            )?;
        }
        match case.status {
            GeoPopulationCaseStatus::Resolved => {
                checked_inc(
                    &mut scores.reconciliation.resolved_cases,
                    "e4.reconciliation.resolved_cases",
                )?;
                match case
                    .resolved_claim
                    .as_ref()
                    .map(|claim| claim.claim_class)
                    .ok_or_else(|| {
                        case_invariant_error(
                            case,
                            "resolved_claim",
                            "Geo E4 gate assessment found a resolved case without a claim class",
                        )
                    })? {
                    GeoResolvedClaimClass::EvidentiallySupported => checked_inc(
                        &mut scores.reconciliation.evidentially_supported_resolved_cases,
                        "e4.reconciliation.evidentially_supported_resolved_cases",
                    )?,
                    GeoResolvedClaimClass::StructurallyForced => checked_inc(
                        &mut scores.reconciliation.structurally_forced_resolved_cases,
                        "e4.reconciliation.structurally_forced_resolved_cases",
                    )?,
                }
                if case.candidate_reach != GeoCandidateReachStatus::Full {
                    checked_inc(
                        &mut scores.reconciliation.resolved_with_reach_not_full_cases,
                        "e4.reconciliation.resolved_with_reach_not_full_cases",
                    )?;
                }
                if case.truth_model_in_residual == Some(true) {
                    checked_inc(
                        &mut scores.truth_quality.exactly_correct_cases,
                        "e4.truth_quality.exactly_correct_cases",
                    )?;
                }
            }
            GeoPopulationCaseStatus::Ambiguous => {
                checked_inc(
                    &mut scores.reconciliation.ambiguous_cases,
                    "e4.reconciliation.ambiguous_cases",
                )?;
                checked_inc(
                    &mut scores.reconciliation.abstention_cases,
                    "e4.reconciliation.abstention_cases",
                )?;
            }
            GeoPopulationCaseStatus::Conflict => {
                checked_inc(
                    &mut scores.reconciliation.conflict_cases,
                    "e4.reconciliation.conflict_cases",
                )?;
                checked_inc(
                    &mut scores.reconciliation.abstention_cases,
                    "e4.reconciliation.abstention_cases",
                )?;
            }
            GeoPopulationCaseStatus::AssignmentBudgetExceeded => {
                checked_inc(
                    &mut scores.solver_exactness.assignment_budget_exceeded_cases,
                    "e4.solver_exactness.assignment_budget_exceeded_cases",
                )?;
                checked_inc(
                    &mut scores.cost.assignment_budget_exceeded_cases,
                    "e4.cost.assignment_budget_exceeded_cases",
                )?;
                checked_inc(
                    &mut scores.reconciliation.abstention_cases,
                    "e4.reconciliation.abstention_cases",
                )?;
            }
            GeoPopulationCaseStatus::ComponentBudgetFallback => {
                checked_inc(
                    &mut scores.solver_exactness.component_budget_fallback_cases,
                    "e4.solver_exactness.component_budget_fallback_cases",
                )?;
                checked_inc(
                    &mut scores.cost.component_budget_fallback_cases,
                    "e4.cost.component_budget_fallback_cases",
                )?;
                checked_inc(
                    &mut scores.reconciliation.abstention_cases,
                    "e4.reconciliation.abstention_cases",
                )?;
            }
        }
        if case.residual_count_complete {
            checked_inc(
                &mut scores.solver_exactness.residual_count_complete_cases,
                "e4.solver_exactness.residual_count_complete_cases",
            )?;
            checked_inc(
                &mut scores.cost.residual_count_complete_cases,
                "e4.cost.residual_count_complete_cases",
            )?;
            if is_residual_count_exact_case(case) {
                checked_inc(
                    &mut scores.solver_exactness.residual_count_exact_cases,
                    "e4.solver_exactness.residual_count_exact_cases",
                )?;
            }
        } else {
            checked_inc(
                &mut scores.solver_exactness.residual_count_unavailable_cases,
                "e4.solver_exactness.residual_count_unavailable_cases",
            )?;
            checked_inc(
                &mut scores.cost.residual_count_unavailable_cases,
                "e4.cost.residual_count_unavailable_cases",
            )?;
        }
        if case.residual_count_saturated {
            checked_inc(
                &mut scores.solver_exactness.residual_count_saturated_cases,
                "e4.solver_exactness.residual_count_saturated_cases",
            )?;
            checked_inc(
                &mut scores.cost.residual_count_saturated_cases,
                "e4.cost.residual_count_saturated_cases",
            )?;
        }
        if case.full_truth_recall {
            checked_inc(
                &mut scores.truth_quality.full_truth_recall_cases,
                "e4.truth_quality.full_truth_recall_cases",
            )?;
        }
        if case.solver_truth_scored {
            checked_inc(
                &mut scores.truth_quality.solver_truth_scored_cases,
                "e4.truth_quality.solver_truth_scored_cases",
            )?;
        }
        match case.truth_model_in_residual {
            Some(true) => checked_inc(
                &mut scores.truth_quality.solver_truth_retained_cases,
                "e4.truth_quality.solver_truth_retained_cases",
            )?,
            Some(false) => {
                checked_inc(
                    &mut scores.truth_quality.solver_truth_exclusion_cases,
                    "e4.truth_quality.solver_truth_exclusion_cases",
                )?;
                checked_inc(
                    &mut scores.admission.rho_falsification_cases,
                    "e4.admission.rho_falsification_cases",
                )?;
            }
            None => {}
        }
        if case.false_merge {
            checked_inc(
                &mut scores.truth_quality.false_merge_cases,
                "e4.truth_quality.false_merge_cases",
            )?;
        }
        if case.backbone_complete {
            checked_inc(
                &mut scores.truth_quality.backbone_complete_cases,
                "e4.truth_quality.backbone_complete_cases",
            )?;
        }
        checked_add(
            &mut scores.truth_quality.truth_members,
            case.truth_members,
            "e4.truth_quality.truth_members",
        )?;
        checked_add(
            &mut scores.truth_quality.truth_members_in_universe,
            case.truth_members_in_universe,
            "e4.truth_quality.truth_members_in_universe",
        )?;
        checked_add(
            &mut scores.truth_quality.backbone_true_positive_members,
            case.backbone_true_positive_members,
            "e4.truth_quality.backbone_true_positive_members",
        )?;
        checked_add(
            &mut scores.truth_quality.backbone_false_positive_members,
            case.backbone_false_positive_members,
            "e4.truth_quality.backbone_false_positive_members",
        )?;
    }
    validate_e4_plane_scores("e4_plane_scores", &scores)?;
    Ok(scores)
}

fn e4_add_count_blocker(
    blockers: &mut Vec<GeoE4GateBlocker>,
    plane: GeoE4GatePlane,
    code: GeoE4GateBlockerCode,
    observed: u64,
    required: u64,
    blocked: bool,
) {
    if blocked {
        blockers.push(GeoE4GateBlocker {
            plane,
            code,
            observed: observed.to_string(),
            required: required.to_string(),
        });
    }
}

fn canonicalize_population_request(
    request: &GeoPopulationEvaluationRequest,
) -> Result<GeoPopulationEvaluationRequest, GeoPopulationError> {
    if request.version != CANON_GEO_POPULATION_REQUEST_VERSION {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo population request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_POPULATION_REQUEST_VERSION),
            ],
        ));
    }
    if request.max_cases == 0 || request.cases.len() > request.max_cases {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::PopulationBudgetExceeded,
            "Geo population exceeds the declared case budget",
            [
                ("cases", request.cases.len().to_string()),
                ("max_cases", request.max_cases.to_string()),
            ],
        ));
    }
    let mut canonical = request.clone();
    for case in &mut canonical.cases {
        validate_case(case)?;
        case.evidence = canonicalize_population_case_evidence(&case.evidence)?;
    }
    canonical
        .cases
        .sort_by(|left, right| left.id.cmp(&right.id));
    for pair in canonical.cases.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo population contains a duplicate case identifier",
                [("case_id", pair[0].id.as_str())],
            ));
        }
    }
    validate_deed_truth_plane_scope(canonical.cases.iter().map(|case| case.truth_plane))?;
    Ok(canonical)
}

fn canonicalize_population_case_evidence(
    request: &GeoEvidenceCompilationRequest,
) -> Result<GeoEvidenceCompilationRequest, GeoPopulationError> {
    let compilation = compile_evidence(request).map_err(map_evidence_error)?;
    let mut canonical = request.clone();
    canonical.profile = compilation.composition_request.profile;
    canonical.universe = compilation.composition_request.universe;
    canonical.max_assignments = compilation.composition_request.max_assignments;
    canonical.max_materialized_models = compilation.composition_request.max_materialized_models;
    canonical
        .contracts
        .sort_by(|left, right| left.id.cmp(&right.id));
    for observation in &mut canonical.observations {
        canonicalize_population_case_observation(observation);
    }
    canonical
        .observations
        .sort_by(|left, right| left.id.cmp(&right.id));
    compile_evidence(&canonical).map_err(map_evidence_error)?;
    Ok(canonical)
}

fn canonicalize_population_case_observation(observation: &mut GeoRhoObservation) {
    observation.source_records.sort();
    match &mut observation.observation {
        GeoRhoObservationKind::ExactSets { sets, .. } => {
            for set in sets.iter_mut() {
                set.sort();
            }
            sets.sort();
        }
        GeoRhoObservationKind::ExistentialMembership { members } => {
            members.sort_by(compare_e4_entity_refs);
        }
        GeoRhoObservationKind::IntegerSumBand { values, .. } => {
            values.sort_by(compare_e4_integer_values);
        }
        GeoRhoObservationKind::PreferMember { .. } => {}
    }
}

fn compare_e4_entity_refs(left: &GeoEntityRef, right: &GeoEntityRef) -> std::cmp::Ordering {
    (left.level, left.id.as_str()).cmp(&(right.level, right.id.as_str()))
}

fn compare_e4_integer_values(
    left: &GeoIntegerMemberValue,
    right: &GeoIntegerMemberValue,
) -> std::cmp::Ordering {
    left.id.cmp(&right.id)
}

fn digest_population_request(
    request: &GeoPopulationEvaluationRequest,
) -> Result<String, GeoPopulationError> {
    Ok(blake3::hash(&canonical_population_request_bytes(request)?)
        .to_hex()
        .to_string())
}

fn h7_scope_to_e4_proof_class(scope: GeoH7PopulationScope) -> GeoE4GateProofClass {
    match scope {
        GeoH7PopulationScope::FixtureSubset => GeoE4GateProofClass::FixtureSubset,
        GeoH7PopulationScope::ObservedSnapshot => GeoE4GateProofClass::ObservedSnapshot,
        GeoH7PopulationScope::RetainedComplete => GeoE4GateProofClass::RetainedComplete,
        GeoH7PopulationScope::LiveComplete => GeoE4GateProofClass::LiveComplete,
    }
}

fn h7_scope_result_mode_is_consistent(
    scope: GeoH7PopulationScope,
    result_mode: GeoH7ResultMode,
) -> bool {
    matches!(
        (scope, result_mode),
        (GeoH7PopulationScope::FixtureSubset, GeoH7ResultMode::Replay)
            | (
                GeoH7PopulationScope::ObservedSnapshot,
                GeoH7ResultMode::Observed
            )
            | (
                GeoH7PopulationScope::RetainedComplete,
                GeoH7ResultMode::Replay
            )
            | (GeoH7PopulationScope::LiveComplete, GeoH7ResultMode::Live)
    )
}

fn map_h7_population_error(error: GeoMaterializationError) -> GeoPopulationError {
    let mut detail = error.detail;
    detail.insert(
        "materialization_code".to_string(),
        format!("{:?}", error.code),
    );
    GeoPopulationError {
        code: GeoPopulationErrorCode::InvalidInput,
        message: error.message,
        detail,
    }
}

pub fn validate_e4_gate_proof_source(
    source: &GeoE4GateProofSource,
) -> Result<(), GeoPopulationError> {
    validate_lowercase_hex64("proof_source.source_blake3", &source.source_blake3)?;
    validate_lowercase_hex64(
        "proof_source.population_request_blake3",
        &source.population_request_blake3,
    )?;
    match (
        source.inherited_source_version.as_ref(),
        source.inherited_source_blake3.as_ref(),
    ) {
        (Some(version), Some(blake3)) => {
            if version.is_empty() || version.trim() != version {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo E4 proof source inherited source version must be canonical",
                    [("field", "proof_source.inherited_source_version")],
                ));
            }
            validate_lowercase_hex64("proof_source.inherited_source_blake3", blake3)?;
        }
        (None, None) => {}
        _ => {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo E4 proof source inherited version and digest must appear together",
                [("field", "proof_source.inherited_source")],
            ));
        }
    }

    let h7_field_count = [
        source.h7_population_scope.is_some(),
        source.h7_result_mode.is_some(),
        source.h7_materialized_unique_accepted_loans.is_some(),
        source.h7_solver_population_subjects.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if h7_field_count != 0 && h7_field_count != 4 {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 proof source H7 provenance fields must be all present or all absent",
            [("field", "proof_source.h7")],
        ));
    }
    if let (Some(scope), Some(result_mode)) = (source.h7_population_scope, source.h7_result_mode) {
        if source.proof_class != h7_scope_to_e4_proof_class(scope) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo E4 proof source proof_class must match its H7 population scope",
                [
                    ("proof_class", e4_proof_class_name(source.proof_class)),
                    ("h7_population_scope", h7_population_scope_name(scope)),
                ],
            ));
        }
        if !h7_scope_result_mode_is_consistent(scope, result_mode) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo E4 proof source H7 scope and result mode are inconsistent",
                [
                    ("h7_population_scope", h7_population_scope_name(scope)),
                    ("h7_result_mode", h7_result_mode_name(result_mode)),
                ],
            ));
        }
    }

    match source.derivation {
        GeoE4GateProofDerivation::BarePopulationRequest => {
            if source.source_version != CANON_GEO_POPULATION_REQUEST_VERSION {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo E4 bare population proof source must cite the population request version",
                    [
                        ("actual", source.source_version.as_str()),
                        ("expected", CANON_GEO_POPULATION_REQUEST_VERSION),
                    ],
                ));
            }
            if source.proof_class != GeoE4GateProofClass::FixtureSubset {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo E4 bare population proof source can only be fixture class",
                    [("proof_class", e4_proof_class_name(source.proof_class))],
                ));
            }
            if source.source_blake3 != source.population_request_blake3 {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo E4 bare population proof source digest must equal the population request digest",
                    [
                        ("source_blake3", source.source_blake3.as_str()),
                        (
                            "population_request_blake3",
                            source.population_request_blake3.as_str(),
                        ),
                    ],
                ));
            }
            if source.inherited_source_version.is_some() || h7_field_count != 0 {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo E4 bare population proof source cannot carry inherited provenance",
                    [("derivation", "bare_population_request")],
                ));
            }
        }
        GeoE4GateProofDerivation::H7PopulationArtifact => {
            if source.source_version != CANON_GEO_H7_POPULATION_VERSION {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo E4 H7 proof source must cite the H7 population artifact version",
                    [
                        ("actual", source.source_version.as_str()),
                        ("expected", CANON_GEO_H7_POPULATION_VERSION),
                    ],
                ));
            }
            if source.inherited_source_version.is_some() {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo E4 H7 proof source cannot carry inherited source provenance",
                    [("field", "proof_source.inherited_source_version")],
                ));
            }
            if h7_field_count != 4 {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo E4 H7 proof source must carry H7 provenance fields",
                    [("field", "proof_source.h7")],
                ));
            }
        }
        GeoE4GateProofDerivation::PopulationEvidenceStackArtifact => {
            if source.source_version != CANON_GEO_POPULATION_EVIDENCE_STACK_PROOF_VERSION {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo E4 stack proof source must cite the population evidence-stack version",
                    [
                        ("actual", source.source_version.as_str()),
                        (
                            "expected",
                            CANON_GEO_POPULATION_EVIDENCE_STACK_PROOF_VERSION,
                        ),
                    ],
                ));
            }
            if source.inherited_source_version.is_none() {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo E4 stack proof source must carry inherited source provenance",
                    [("field", "proof_source.inherited_source_version")],
                ));
            }
            if source.proof_class != GeoE4GateProofClass::FixtureSubset && h7_field_count != 4 {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo E4 non-fixture stack proof must inherit H7 provenance",
                    [("proof_class", e4_proof_class_name(source.proof_class))],
                ));
            }
        }
    }
    Ok(())
}

fn e4_gate_blockers(
    proof_class: GeoE4GateProofClass,
    required_subjects: u64,
    evaluated_cases: u64,
    planes: &GeoE4GatePlaneScores,
) -> Result<Vec<GeoE4GateBlocker>, GeoPopulationError> {
    let mut blockers = Vec::new();
    if proof_class != GeoE4GateProofClass::LiveComplete {
        blockers.push(GeoE4GateBlocker {
            plane: GeoE4GatePlane::Proof,
            code: GeoE4GateBlockerCode::ProofClassNotLiveComplete,
            observed: e4_proof_class_name(proof_class).to_string(),
            required: "live_complete".to_string(),
        });
    }
    e4_add_count_blocker(
        &mut blockers,
        GeoE4GatePlane::Coverage,
        GeoE4GateBlockerCode::PopulationDenominatorMismatch,
        evaluated_cases,
        required_subjects,
        evaluated_cases != required_subjects,
    );
    e4_add_count_blocker(
        &mut blockers,
        GeoE4GatePlane::Coverage,
        GeoE4GateBlockerCode::EvidenceNoObservation,
        planes.coverage.evidence_no_observation_cases,
        0,
        planes.coverage.evidence_no_observation_cases != 0,
    );
    e4_add_count_blocker(
        &mut blockers,
        GeoE4GatePlane::CandidateReach,
        GeoE4GateBlockerCode::CandidateReachIncomplete,
        planes.candidate_reach.full_cases,
        evaluated_cases,
        planes.candidate_reach.full_cases != evaluated_cases,
    );
    e4_add_count_blocker(
        &mut blockers,
        GeoE4GatePlane::SolverExactness,
        GeoE4GateBlockerCode::SolverArtifactMissing,
        planes.solver_exactness.solver_artifact_cases,
        evaluated_cases,
        planes.solver_exactness.solver_artifact_cases != evaluated_cases,
    );
    e4_add_count_blocker(
        &mut blockers,
        GeoE4GatePlane::SolverExactness,
        GeoE4GateBlockerCode::ResidualCountInexact,
        sum_u64(
            [
                planes.solver_exactness.residual_count_saturated_cases,
                planes.solver_exactness.residual_count_unavailable_cases,
            ],
            "e4.solver_exactness.residual_count_inexact_cases",
        )?,
        0,
        planes.solver_exactness.residual_count_saturated_cases != 0
            || planes.solver_exactness.residual_count_unavailable_cases != 0,
    );
    e4_add_count_blocker(
        &mut blockers,
        GeoE4GatePlane::Admission,
        GeoE4GateBlockerCode::RhoFalsification,
        planes.admission.rho_falsification_cases,
        0,
        planes.admission.rho_falsification_cases != 0,
    );
    e4_add_count_blocker(
        &mut blockers,
        GeoE4GatePlane::TruthQuality,
        GeoE4GateBlockerCode::FalseMerge,
        planes.truth_quality.false_merge_cases,
        0,
        planes.truth_quality.false_merge_cases != 0,
    );
    e4_add_count_blocker(
        &mut blockers,
        GeoE4GatePlane::Cost,
        GeoE4GateBlockerCode::AssignmentBudgetExceeded,
        planes.solver_exactness.assignment_budget_exceeded_cases,
        0,
        planes.solver_exactness.assignment_budget_exceeded_cases != 0,
    );
    e4_add_count_blocker(
        &mut blockers,
        GeoE4GatePlane::Cost,
        GeoE4GateBlockerCode::ComponentBudgetFallback,
        planes.solver_exactness.component_budget_fallback_cases,
        0,
        planes.solver_exactness.component_budget_fallback_cases != 0,
    );
    blockers.sort();
    Ok(blockers)
}

fn e4_gate_case_findings<'a>(
    cases: impl IntoIterator<Item = &'a GeoPopulationCaseEvaluation>,
) -> Vec<GeoE4GateCaseFinding> {
    let mut findings = Vec::new();
    for case in cases {
        if case.evidence_coverage == GeoEvidenceCoverageStatus::NoObservations {
            push_e4_case_finding(
                &mut findings,
                case,
                GeoE4GatePlane::Coverage,
                GeoE4GateBlockerCode::EvidenceNoObservation,
                evidence_coverage_name(case.evidence_coverage),
                "hard_soft_or_diagnostic_observation",
            );
        }
        if case.candidate_reach != GeoCandidateReachStatus::Full {
            push_e4_case_finding(
                &mut findings,
                case,
                GeoE4GatePlane::CandidateReach,
                GeoE4GateBlockerCode::CandidateReachIncomplete,
                candidate_reach_name(case.candidate_reach),
                "full",
            );
        }
        if case.solver_digest.is_none() {
            push_e4_case_finding(
                &mut findings,
                case,
                GeoE4GatePlane::SolverExactness,
                GeoE4GateBlockerCode::SolverArtifactMissing,
                "missing",
                "present",
            );
        }
        if case.residual_count_saturated || !case.residual_count_complete {
            let observed = if !case.residual_count_complete {
                "unavailable"
            } else {
                "saturated"
            };
            push_e4_case_finding(
                &mut findings,
                case,
                GeoE4GatePlane::SolverExactness,
                GeoE4GateBlockerCode::ResidualCountInexact,
                observed,
                "exact",
            );
        }
        if case.truth_model_in_residual == Some(false) {
            push_e4_case_finding(
                &mut findings,
                case,
                GeoE4GatePlane::Admission,
                GeoE4GateBlockerCode::RhoFalsification,
                "truth_excluded",
                "truth_retained",
            );
        }
        if case.false_merge {
            push_e4_case_finding(
                &mut findings,
                case,
                GeoE4GatePlane::TruthQuality,
                GeoE4GateBlockerCode::FalseMerge,
                "resolved_truth_excluded",
                "truth_retained_or_abstain",
            );
        }
        if case.status == GeoPopulationCaseStatus::AssignmentBudgetExceeded {
            push_e4_case_finding(
                &mut findings,
                case,
                GeoE4GatePlane::Cost,
                GeoE4GateBlockerCode::AssignmentBudgetExceeded,
                "assignment_budget_exceeded",
                "within_assignment_budget",
            );
        }
        if case.status == GeoPopulationCaseStatus::ComponentBudgetFallback {
            push_e4_case_finding(
                &mut findings,
                case,
                GeoE4GatePlane::Cost,
                GeoE4GateBlockerCode::ComponentBudgetFallback,
                "component_budget_fallback",
                "exact_component_solve",
            );
        }
    }
    findings.sort();
    findings
}

fn push_e4_case_finding(
    findings: &mut Vec<GeoE4GateCaseFinding>,
    case: &GeoPopulationCaseEvaluation,
    plane: GeoE4GatePlane,
    code: GeoE4GateBlockerCode,
    observed: &'static str,
    required: &'static str,
) {
    findings.push(GeoE4GateCaseFinding {
        plane,
        code,
        case_id: case.case_id.clone(),
        truth_plane: case.truth_plane,
        case_status: case.status,
        observed: observed.to_string(),
        required: required.to_string(),
    });
}

fn e4_sum_plane_scores<'a>(
    planes: impl IntoIterator<Item = &'a GeoE4GatePlaneScores>,
) -> Result<GeoE4GatePlaneScores, GeoPopulationError> {
    let mut total = GeoE4GatePlaneScores::default();
    for plane in planes {
        checked_add(
            &mut total.coverage.cases,
            plane.coverage.cases,
            "e4.truth_planes.coverage.cases",
        )?;
        checked_add(
            &mut total.coverage.population_eligible_cases,
            plane.coverage.population_eligible_cases,
            "e4.truth_planes.coverage.population_eligible_cases",
        )?;
        checked_add(
            &mut total.coverage.evidence_no_observation_cases,
            plane.coverage.evidence_no_observation_cases,
            "e4.truth_planes.coverage.evidence_no_observation_cases",
        )?;
        checked_add(
            &mut total.coverage.evidence_diagnostic_only_cases,
            plane.coverage.evidence_diagnostic_only_cases,
            "e4.truth_planes.coverage.evidence_diagnostic_only_cases",
        )?;
        checked_add(
            &mut total.coverage.evidence_soft_preference_only_cases,
            plane.coverage.evidence_soft_preference_only_cases,
            "e4.truth_planes.coverage.evidence_soft_preference_only_cases",
        )?;
        checked_add(
            &mut total.coverage.evidence_soft_and_diagnostic_only_cases,
            plane.coverage.evidence_soft_and_diagnostic_only_cases,
            "e4.truth_planes.coverage.evidence_soft_and_diagnostic_only_cases",
        )?;
        checked_add(
            &mut total.coverage.evidence_hard_constraint_cases,
            plane.coverage.evidence_hard_constraint_cases,
            "e4.truth_planes.coverage.evidence_hard_constraint_cases",
        )?;
        checked_add(
            &mut total.candidate_reach.evaluated_cases,
            plane.candidate_reach.evaluated_cases,
            "e4.truth_planes.candidate_reach.evaluated_cases",
        )?;
        checked_add(
            &mut total.candidate_reach.full_cases,
            plane.candidate_reach.full_cases,
            "e4.truth_planes.candidate_reach.full_cases",
        )?;
        checked_add(
            &mut total.candidate_reach.partial_cases,
            plane.candidate_reach.partial_cases,
            "e4.truth_planes.candidate_reach.partial_cases",
        )?;
        checked_add(
            &mut total.candidate_reach.none_cases,
            plane.candidate_reach.none_cases,
            "e4.truth_planes.candidate_reach.none_cases",
        )?;
        checked_add(
            &mut total.candidate_reach.recall_failure_cases,
            plane.candidate_reach.recall_failure_cases,
            "e4.truth_planes.candidate_reach.recall_failure_cases",
        )?;
        checked_add(
            &mut total.admission.hard_constraint_cases,
            plane.admission.hard_constraint_cases,
            "e4.truth_planes.admission.hard_constraint_cases",
        )?;
        checked_add(
            &mut total.admission.soft_preference_only_cases,
            plane.admission.soft_preference_only_cases,
            "e4.truth_planes.admission.soft_preference_only_cases",
        )?;
        checked_add(
            &mut total.admission.diagnostic_only_cases,
            plane.admission.diagnostic_only_cases,
            "e4.truth_planes.admission.diagnostic_only_cases",
        )?;
        checked_add(
            &mut total.admission.soft_and_diagnostic_only_cases,
            plane.admission.soft_and_diagnostic_only_cases,
            "e4.truth_planes.admission.soft_and_diagnostic_only_cases",
        )?;
        checked_add(
            &mut total.admission.rho_falsification_cases,
            plane.admission.rho_falsification_cases,
            "e4.truth_planes.admission.rho_falsification_cases",
        )?;
        checked_add(
            &mut total.solver_exactness.solver_artifact_cases,
            plane.solver_exactness.solver_artifact_cases,
            "e4.truth_planes.solver_exactness.solver_artifact_cases",
        )?;
        checked_add(
            &mut total.solver_exactness.residual_count_complete_cases,
            plane.solver_exactness.residual_count_complete_cases,
            "e4.truth_planes.solver_exactness.residual_count_complete_cases",
        )?;
        checked_add(
            &mut total.solver_exactness.residual_count_exact_cases,
            plane.solver_exactness.residual_count_exact_cases,
            "e4.truth_planes.solver_exactness.residual_count_exact_cases",
        )?;
        checked_add(
            &mut total.solver_exactness.residual_count_saturated_cases,
            plane.solver_exactness.residual_count_saturated_cases,
            "e4.truth_planes.solver_exactness.residual_count_saturated_cases",
        )?;
        checked_add(
            &mut total.solver_exactness.residual_count_unavailable_cases,
            plane.solver_exactness.residual_count_unavailable_cases,
            "e4.truth_planes.solver_exactness.residual_count_unavailable_cases",
        )?;
        checked_add(
            &mut total.solver_exactness.assignment_budget_exceeded_cases,
            plane.solver_exactness.assignment_budget_exceeded_cases,
            "e4.truth_planes.solver_exactness.assignment_budget_exceeded_cases",
        )?;
        checked_add(
            &mut total.solver_exactness.component_budget_fallback_cases,
            plane.solver_exactness.component_budget_fallback_cases,
            "e4.truth_planes.solver_exactness.component_budget_fallback_cases",
        )?;
        checked_add(
            &mut total.reconciliation.resolved_cases,
            plane.reconciliation.resolved_cases,
            "e4.truth_planes.reconciliation.resolved_cases",
        )?;
        checked_add(
            &mut total.reconciliation.evidentially_supported_resolved_cases,
            plane.reconciliation.evidentially_supported_resolved_cases,
            "e4.truth_planes.reconciliation.evidentially_supported_resolved_cases",
        )?;
        checked_add(
            &mut total.reconciliation.structurally_forced_resolved_cases,
            plane.reconciliation.structurally_forced_resolved_cases,
            "e4.truth_planes.reconciliation.structurally_forced_resolved_cases",
        )?;
        checked_add(
            &mut total.reconciliation.resolved_with_reach_not_full_cases,
            plane.reconciliation.resolved_with_reach_not_full_cases,
            "e4.truth_planes.reconciliation.resolved_with_reach_not_full_cases",
        )?;
        checked_add(
            &mut total.reconciliation.ambiguous_cases,
            plane.reconciliation.ambiguous_cases,
            "e4.truth_planes.reconciliation.ambiguous_cases",
        )?;
        checked_add(
            &mut total.reconciliation.conflict_cases,
            plane.reconciliation.conflict_cases,
            "e4.truth_planes.reconciliation.conflict_cases",
        )?;
        checked_add(
            &mut total.reconciliation.abstention_cases,
            plane.reconciliation.abstention_cases,
            "e4.truth_planes.reconciliation.abstention_cases",
        )?;
        checked_add(
            &mut total.truth_quality.full_truth_recall_cases,
            plane.truth_quality.full_truth_recall_cases,
            "e4.truth_planes.truth_quality.full_truth_recall_cases",
        )?;
        checked_add(
            &mut total.truth_quality.solver_truth_scored_cases,
            plane.truth_quality.solver_truth_scored_cases,
            "e4.truth_planes.truth_quality.solver_truth_scored_cases",
        )?;
        checked_add(
            &mut total.truth_quality.solver_truth_retained_cases,
            plane.truth_quality.solver_truth_retained_cases,
            "e4.truth_planes.truth_quality.solver_truth_retained_cases",
        )?;
        checked_add(
            &mut total.truth_quality.solver_truth_exclusion_cases,
            plane.truth_quality.solver_truth_exclusion_cases,
            "e4.truth_planes.truth_quality.solver_truth_exclusion_cases",
        )?;
        checked_add(
            &mut total.truth_quality.exactly_correct_cases,
            plane.truth_quality.exactly_correct_cases,
            "e4.truth_planes.truth_quality.exactly_correct_cases",
        )?;
        checked_add(
            &mut total.truth_quality.false_merge_cases,
            plane.truth_quality.false_merge_cases,
            "e4.truth_planes.truth_quality.false_merge_cases",
        )?;
        checked_add(
            &mut total.truth_quality.backbone_complete_cases,
            plane.truth_quality.backbone_complete_cases,
            "e4.truth_planes.truth_quality.backbone_complete_cases",
        )?;
        checked_add(
            &mut total.truth_quality.truth_members,
            plane.truth_quality.truth_members,
            "e4.truth_planes.truth_quality.truth_members",
        )?;
        checked_add(
            &mut total.truth_quality.truth_members_in_universe,
            plane.truth_quality.truth_members_in_universe,
            "e4.truth_planes.truth_quality.truth_members_in_universe",
        )?;
        checked_add(
            &mut total.truth_quality.backbone_true_positive_members,
            plane.truth_quality.backbone_true_positive_members,
            "e4.truth_planes.truth_quality.backbone_true_positive_members",
        )?;
        checked_add(
            &mut total.truth_quality.backbone_false_positive_members,
            plane.truth_quality.backbone_false_positive_members,
            "e4.truth_planes.truth_quality.backbone_false_positive_members",
        )?;
        checked_add(
            &mut total.cost.candidate_members,
            plane.cost.candidate_members,
            "e4.truth_planes.cost.candidate_members",
        )?;
        total.cost.max_candidate_members = total
            .cost
            .max_candidate_members
            .max(plane.cost.max_candidate_members);
        checked_add(
            &mut total.cost.solver_artifact_cases,
            plane.cost.solver_artifact_cases,
            "e4.truth_planes.cost.solver_artifact_cases",
        )?;
        checked_add(
            &mut total.cost.residual_count_complete_cases,
            plane.cost.residual_count_complete_cases,
            "e4.truth_planes.cost.residual_count_complete_cases",
        )?;
        checked_add(
            &mut total.cost.residual_count_saturated_cases,
            plane.cost.residual_count_saturated_cases,
            "e4.truth_planes.cost.residual_count_saturated_cases",
        )?;
        checked_add(
            &mut total.cost.residual_count_unavailable_cases,
            plane.cost.residual_count_unavailable_cases,
            "e4.truth_planes.cost.residual_count_unavailable_cases",
        )?;
        total.cost.max_residual_model_count = match (
            total.cost.max_residual_model_count,
            plane.cost.max_residual_model_count,
        ) {
            (Some(left), Some(right)) => Some(left.max(right)),
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (None, None) => None,
        };
        checked_add(
            &mut total.cost.assignment_budget_exceeded_cases,
            plane.cost.assignment_budget_exceeded_cases,
            "e4.truth_planes.cost.assignment_budget_exceeded_cases",
        )?;
        checked_add(
            &mut total.cost.component_budget_fallback_cases,
            plane.cost.component_budget_fallback_cases,
            "e4.truth_planes.cost.component_budget_fallback_cases",
        )?;
    }
    validate_e4_plane_scores("e4.truth_plane_score_sum", &total)?;
    Ok(total)
}

fn digest_e4_gate_assessment(
    assessment: &GeoE4GateAssessment,
) -> Result<String, GeoPopulationError> {
    let bytes = canonical_e4_gate_assessment_bytes(assessment).map_err(|error| {
        GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo E4 rescore comparison could not serialize an assessment",
            [("error", error.to_string())],
        )
    })?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn e4_rescore_snapshot(
    assessment: &GeoE4GateAssessment,
    assessment_blake3: String,
) -> GeoE4RescoreSnapshot {
    GeoE4RescoreSnapshot {
        assessment_blake3,
        proof_class: assessment.proof_class,
        status: assessment.status,
        release_claim_allowed: assessment.release_claim_allowed,
        evaluated_cases: assessment.evaluated_cases,
        subject_deficit: assessment.subject_deficit,
        candidate_reach_full_cases: assessment.planes.candidate_reach.full_cases,
        candidate_reach_partial_cases: assessment.planes.candidate_reach.partial_cases,
        candidate_reach_none_cases: assessment.planes.candidate_reach.none_cases,
        resolved_cases: assessment.planes.reconciliation.resolved_cases,
        exactly_correct_cases: assessment.planes.truth_quality.exactly_correct_cases,
        ambiguous_cases: assessment.planes.reconciliation.ambiguous_cases,
        conflict_cases: assessment.planes.reconciliation.conflict_cases,
        false_merge_cases: assessment.planes.truth_quality.false_merge_cases,
        truth_exclusion_cases: assessment.planes.truth_quality.solver_truth_exclusion_cases,
        component_fallback_cases: assessment
            .planes
            .solver_exactness
            .component_budget_fallback_cases,
    }
}

fn e4_rescore_metric_value(assessment: &GeoE4GateAssessment, metric: GeoE4RescoreMetric) -> u64 {
    match metric {
        GeoE4RescoreMetric::CandidateReachFull => assessment.planes.candidate_reach.full_cases,
        GeoE4RescoreMetric::CandidateReachPartial => {
            assessment.planes.candidate_reach.partial_cases
        }
        GeoE4RescoreMetric::CandidateReachNone => assessment.planes.candidate_reach.none_cases,
        GeoE4RescoreMetric::Resolved => assessment.planes.reconciliation.resolved_cases,
        GeoE4RescoreMetric::ExactlyCorrect => assessment.planes.truth_quality.exactly_correct_cases,
        GeoE4RescoreMetric::Ambiguous => assessment.planes.reconciliation.ambiguous_cases,
        GeoE4RescoreMetric::Conflict => assessment.planes.reconciliation.conflict_cases,
        GeoE4RescoreMetric::FalseMerges => assessment.planes.truth_quality.false_merge_cases,
        GeoE4RescoreMetric::TruthExclusions => {
            assessment.planes.truth_quality.solver_truth_exclusion_cases
        }
        GeoE4RescoreMetric::ComponentFallbacks => {
            assessment
                .planes
                .solver_exactness
                .component_budget_fallback_cases
        }
    }
}

fn e4_rescore_snapshot_metric_value(
    snapshot: &GeoE4RescoreSnapshot,
    metric: GeoE4RescoreMetric,
) -> u64 {
    match metric {
        GeoE4RescoreMetric::CandidateReachFull => snapshot.candidate_reach_full_cases,
        GeoE4RescoreMetric::CandidateReachPartial => snapshot.candidate_reach_partial_cases,
        GeoE4RescoreMetric::CandidateReachNone => snapshot.candidate_reach_none_cases,
        GeoE4RescoreMetric::Resolved => snapshot.resolved_cases,
        GeoE4RescoreMetric::ExactlyCorrect => snapshot.exactly_correct_cases,
        GeoE4RescoreMetric::Ambiguous => snapshot.ambiguous_cases,
        GeoE4RescoreMetric::Conflict => snapshot.conflict_cases,
        GeoE4RescoreMetric::FalseMerges => snapshot.false_merge_cases,
        GeoE4RescoreMetric::TruthExclusions => snapshot.truth_exclusion_cases,
        GeoE4RescoreMetric::ComponentFallbacks => snapshot.component_fallback_cases,
    }
}

fn validate_e4_rescore_snapshot(
    label: &'static str,
    required_subjects: u64,
    snapshot: &GeoE4RescoreSnapshot,
) -> Result<(), GeoPopulationError> {
    validate_lowercase_hex64(
        "e4_rescore_comparison.assessment_blake3",
        &snapshot.assessment_blake3,
    )?;
    let expected_deficit = required_subjects.saturating_sub(snapshot.evaluated_cases);
    if snapshot.subject_deficit != expected_deficit {
        return Err(summary_invariant_error(
            label,
            "subject_deficit",
            expected_deficit,
            snapshot.subject_deficit,
        ));
    }
    let expected_release_claim_allowed = snapshot.status == GeoE4GateStatus::Passed
        && snapshot.proof_class == GeoE4GateProofClass::LiveComplete;
    if snapshot.release_claim_allowed != expected_release_claim_allowed {
        let expected = expected_release_claim_allowed.to_string();
        let actual = snapshot.release_claim_allowed.to_string();
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 rescore comparison snapshot release-claim field is inconsistent",
            [
                ("snapshot", label.to_string()),
                ("expected", expected),
                ("actual", actual),
            ],
        ));
    }
    let reach_total = sum_u64(
        [
            snapshot.candidate_reach_full_cases,
            snapshot.candidate_reach_partial_cases,
            snapshot.candidate_reach_none_cases,
        ],
        "e4_rescore_comparison.snapshot.candidate_reach_cases",
    )?;
    if reach_total != snapshot.evaluated_cases {
        return Err(summary_invariant_error(
            label,
            "candidate_reach_cases",
            snapshot.evaluated_cases,
            reach_total,
        ));
    }
    if snapshot.exactly_correct_cases > snapshot.resolved_cases {
        return Err(summary_invariant_error(
            label,
            "exactly_correct_cases",
            snapshot.resolved_cases,
            snapshot.exactly_correct_cases,
        ));
    }
    if snapshot.false_merge_cases > snapshot.resolved_cases {
        return Err(summary_invariant_error(
            label,
            "false_merge_cases",
            snapshot.resolved_cases,
            snapshot.false_merge_cases,
        ));
    }
    if snapshot.truth_exclusion_cases > snapshot.evaluated_cases {
        return Err(summary_invariant_error(
            label,
            "truth_exclusion_cases",
            snapshot.evaluated_cases,
            snapshot.truth_exclusion_cases,
        ));
    }
    Ok(())
}

fn signed_delta(field: &'static str, before: u64, after: u64) -> Result<i64, GeoPopulationError> {
    let delta = i128::from(after) - i128::from(before);
    i64::try_from(delta).map_err(|_| GeoPopulationError::overflow(field))
}

fn e4_proof_class_name(proof_class: GeoE4GateProofClass) -> &'static str {
    match proof_class {
        GeoE4GateProofClass::FixtureSubset => "fixture_subset",
        GeoE4GateProofClass::ObservedSnapshot => "observed_snapshot",
        GeoE4GateProofClass::RetainedComplete => "retained_complete",
        GeoE4GateProofClass::LiveComplete => "live_complete",
    }
}

fn h7_population_scope_name(scope: GeoH7PopulationScope) -> &'static str {
    match scope {
        GeoH7PopulationScope::FixtureSubset => "fixture_subset",
        GeoH7PopulationScope::ObservedSnapshot => "observed_snapshot",
        GeoH7PopulationScope::RetainedComplete => "retained_complete",
        GeoH7PopulationScope::LiveComplete => "live_complete",
    }
}

fn h7_result_mode_name(mode: GeoH7ResultMode) -> &'static str {
    match mode {
        GeoH7ResultMode::Live => "live",
        GeoH7ResultMode::Observed => "observed",
        GeoH7ResultMode::Replay => "replay",
    }
}

fn validate_lowercase_hex64(field: &'static str, value: &str) -> Result<(), GeoPopulationError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 gate assessment digest fields must be lowercase 64-character hex",
            [(field, value)],
        ));
    }
    Ok(())
}

fn validate_e4_plane_scores(
    scope: &'static str,
    scores: &GeoE4GatePlaneScores,
) -> Result<(), GeoPopulationError> {
    if scores.coverage.population_eligible_cases != scores.coverage.cases {
        return Err(summary_invariant_error(
            scope,
            "coverage.population_eligible_cases",
            scores.coverage.cases,
            scores.coverage.population_eligible_cases,
        ));
    }
    let evidence_total = sum_u64(
        [
            scores.coverage.evidence_no_observation_cases,
            scores.coverage.evidence_diagnostic_only_cases,
            scores.coverage.evidence_soft_preference_only_cases,
            scores.coverage.evidence_soft_and_diagnostic_only_cases,
            scores.coverage.evidence_hard_constraint_cases,
        ],
        "e4.coverage.evidence_cases",
    )?;
    if evidence_total != scores.coverage.cases {
        return Err(summary_invariant_error(
            scope,
            "coverage.evidence_cases",
            scores.coverage.cases,
            evidence_total,
        ));
    }
    let reach_total = sum_u64(
        [
            scores.candidate_reach.full_cases,
            scores.candidate_reach.partial_cases,
            scores.candidate_reach.none_cases,
        ],
        "e4.candidate_reach.cases",
    )?;
    if reach_total != scores.candidate_reach.evaluated_cases {
        return Err(summary_invariant_error(
            scope,
            "candidate_reach.cases",
            scores.candidate_reach.evaluated_cases,
            reach_total,
        ));
    }
    if scores.candidate_reach.evaluated_cases != scores.coverage.cases {
        return Err(summary_invariant_error(
            scope,
            "candidate_reach.evaluated_cases",
            scores.coverage.cases,
            scores.candidate_reach.evaluated_cases,
        ));
    }
    let reach_failures = sum_u64(
        [
            scores.candidate_reach.partial_cases,
            scores.candidate_reach.none_cases,
        ],
        "e4.candidate_reach.recall_failure_cases",
    )?;
    if scores.candidate_reach.recall_failure_cases != reach_failures {
        return Err(summary_invariant_error(
            scope,
            "candidate_reach.recall_failure_cases",
            reach_failures,
            scores.candidate_reach.recall_failure_cases,
        ));
    }
    let admission_total = sum_u64(
        [
            scores.admission.hard_constraint_cases,
            scores.admission.soft_preference_only_cases,
            scores.admission.diagnostic_only_cases,
            scores.admission.soft_and_diagnostic_only_cases,
            scores.coverage.evidence_no_observation_cases,
        ],
        "e4.admission.covered_cases",
    )?;
    if admission_total != scores.coverage.cases {
        return Err(summary_invariant_error(
            scope,
            "admission.covered_cases",
            scores.coverage.cases,
            admission_total,
        ));
    }
    let status_total = sum_u64(
        [
            scores.reconciliation.resolved_cases,
            scores.reconciliation.ambiguous_cases,
            scores.reconciliation.conflict_cases,
            scores.solver_exactness.assignment_budget_exceeded_cases,
            scores.solver_exactness.component_budget_fallback_cases,
        ],
        "e4.reconciliation.status_cases",
    )?;
    if status_total != scores.coverage.cases {
        return Err(summary_invariant_error(
            scope,
            "reconciliation.status_cases",
            scores.coverage.cases,
            status_total,
        ));
    }
    let resolved_classes = sum_u64(
        [
            scores.reconciliation.evidentially_supported_resolved_cases,
            scores.reconciliation.structurally_forced_resolved_cases,
        ],
        "e4.reconciliation.resolved_class_cases",
    )?;
    if resolved_classes != scores.reconciliation.resolved_cases {
        return Err(summary_invariant_error(
            scope,
            "reconciliation.resolved_class_cases",
            scores.reconciliation.resolved_cases,
            resolved_classes,
        ));
    }
    let abstention_cases = sum_u64(
        [
            scores.reconciliation.ambiguous_cases,
            scores.reconciliation.conflict_cases,
            scores.solver_exactness.assignment_budget_exceeded_cases,
            scores.solver_exactness.component_budget_fallback_cases,
        ],
        "e4.reconciliation.abstention_cases",
    )?;
    if abstention_cases != scores.reconciliation.abstention_cases {
        return Err(summary_invariant_error(
            scope,
            "reconciliation.abstention_cases",
            abstention_cases,
            scores.reconciliation.abstention_cases,
        ));
    }
    let residual_complete_or_unavailable = sum_u64(
        [
            scores.solver_exactness.residual_count_complete_cases,
            scores.solver_exactness.residual_count_unavailable_cases,
        ],
        "e4.solver_exactness.residual_cases",
    )?;
    if residual_complete_or_unavailable != scores.coverage.cases {
        return Err(summary_invariant_error(
            scope,
            "solver_exactness.residual_cases",
            scores.coverage.cases,
            residual_complete_or_unavailable,
        ));
    }
    let exact_or_saturated = sum_u64(
        [
            scores.solver_exactness.residual_count_exact_cases,
            scores.solver_exactness.residual_count_saturated_cases,
        ],
        "e4.solver_exactness.exact_or_saturated_cases",
    )?;
    if exact_or_saturated != scores.solver_exactness.residual_count_complete_cases {
        return Err(summary_invariant_error(
            scope,
            "solver_exactness.exact_or_saturated_cases",
            scores.solver_exactness.residual_count_complete_cases,
            exact_or_saturated,
        ));
    }
    if scores.cost.solver_artifact_cases != scores.solver_exactness.solver_artifact_cases
        || scores.cost.residual_count_complete_cases
            != scores.solver_exactness.residual_count_complete_cases
        || scores.cost.residual_count_saturated_cases
            != scores.solver_exactness.residual_count_saturated_cases
        || scores.cost.residual_count_unavailable_cases
            != scores.solver_exactness.residual_count_unavailable_cases
        || scores.cost.assignment_budget_exceeded_cases
            != scores.solver_exactness.assignment_budget_exceeded_cases
        || scores.cost.component_budget_fallback_cases
            != scores.solver_exactness.component_budget_fallback_cases
    {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 cost plane is inconsistent with solver exactness counters",
            [("scope", scope.to_string())],
        ));
    }
    if scores.truth_quality.full_truth_recall_cases != scores.candidate_reach.full_cases {
        return Err(summary_invariant_error(
            scope,
            "truth_quality.full_truth_recall_cases",
            scores.candidate_reach.full_cases,
            scores.truth_quality.full_truth_recall_cases,
        ));
    }
    let truth_scored = sum_u64(
        [
            scores.truth_quality.solver_truth_retained_cases,
            scores.truth_quality.solver_truth_exclusion_cases,
        ],
        "e4.truth_quality.solver_truth_scored_cases",
    )?;
    if truth_scored != scores.truth_quality.solver_truth_scored_cases {
        return Err(summary_invariant_error(
            scope,
            "truth_quality.solver_truth_scored_cases",
            scores.truth_quality.solver_truth_scored_cases,
            truth_scored,
        ));
    }
    if scores.admission.rho_falsification_cases != scores.truth_quality.solver_truth_exclusion_cases
    {
        return Err(summary_invariant_error(
            scope,
            "admission.rho_falsification_cases",
            scores.truth_quality.solver_truth_exclusion_cases,
            scores.admission.rho_falsification_cases,
        ));
    }
    if scores.truth_quality.solver_truth_scored_cases > scores.candidate_reach.full_cases {
        return Err(summary_invariant_error(
            scope,
            "truth_quality.solver_truth_scored_cases",
            scores.candidate_reach.full_cases,
            scores.truth_quality.solver_truth_scored_cases,
        ));
    }
    if scores.truth_quality.exactly_correct_cases > scores.reconciliation.resolved_cases {
        return Err(summary_invariant_error(
            scope,
            "truth_quality.exactly_correct_cases",
            scores.reconciliation.resolved_cases,
            scores.truth_quality.exactly_correct_cases,
        ));
    }
    if scores.truth_quality.exactly_correct_cases > scores.truth_quality.solver_truth_retained_cases
    {
        return Err(summary_invariant_error(
            scope,
            "truth_quality.exactly_correct_cases",
            scores.truth_quality.solver_truth_retained_cases,
            scores.truth_quality.exactly_correct_cases,
        ));
    }
    if scores.truth_quality.false_merge_cases > scores.reconciliation.resolved_cases {
        return Err(summary_invariant_error(
            scope,
            "truth_quality.false_merge_cases",
            scores.reconciliation.resolved_cases,
            scores.truth_quality.false_merge_cases,
        ));
    }
    Ok(())
}

fn validate_e4_gate_case_findings(
    assessment: &GeoE4GateAssessment,
) -> Result<(), GeoPopulationError> {
    for pair in assessment.case_findings.windows(2) {
        if pair[0] >= pair[1] {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo E4 gate assessment case findings must be sorted and unique",
                [
                    ("previous", pair[0].case_id.as_str()),
                    ("current", pair[1].case_id.as_str()),
                ],
            ));
        }
    }

    let mut evidence_no_observation = 0;
    let mut candidate_reach_incomplete = 0;
    let mut solver_artifact_missing = 0;
    let mut residual_count_inexact = 0;
    let mut rho_falsification = 0;
    let mut false_merge = 0;
    let mut assignment_budget_exceeded = 0;
    let mut component_budget_fallback = 0;
    let mut finding_keys = BTreeSet::<(GeoE4GateBlockerCode, String)>::new();
    for finding in &assessment.case_findings {
        validate_nonempty_canonical("case_findings.case_id", &finding.case_id)?;
        validate_nonempty_trimmed_finding_text("case_findings.observed", &finding.observed)?;
        validate_nonempty_trimmed_finding_text("case_findings.required", &finding.required)?;
        if !finding_keys.insert((finding.code, finding.case_id.clone())) {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo E4 gate assessment case findings must be unique per blocker and case",
                [
                    ("case_id", finding.case_id.clone()),
                    ("code", format!("{:?}", finding.code)),
                ],
            ));
        }
        let expected_plane = e4_case_finding_plane(finding.code).ok_or_else(|| {
            GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo E4 gate assessment case finding uses a non-case blocker code",
                [("code", format!("{:?}", finding.code))],
            )
        })?;
        if finding.plane != expected_plane {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo E4 gate assessment case finding plane does not match its blocker code",
                [
                    ("case_id", finding.case_id.clone()),
                    ("code", format!("{:?}", finding.code)),
                ],
            ));
        }
        match finding.code {
            GeoE4GateBlockerCode::EvidenceNoObservation => checked_inc(
                &mut evidence_no_observation,
                "e4.case_findings.evidence_no_observation",
            )?,
            GeoE4GateBlockerCode::CandidateReachIncomplete => checked_inc(
                &mut candidate_reach_incomplete,
                "e4.case_findings.candidate_reach_incomplete",
            )?,
            GeoE4GateBlockerCode::SolverArtifactMissing => checked_inc(
                &mut solver_artifact_missing,
                "e4.case_findings.solver_artifact_missing",
            )?,
            GeoE4GateBlockerCode::ResidualCountInexact => checked_inc(
                &mut residual_count_inexact,
                "e4.case_findings.residual_count_inexact",
            )?,
            GeoE4GateBlockerCode::RhoFalsification => {
                checked_inc(&mut rho_falsification, "e4.case_findings.rho_falsification")?
            }
            GeoE4GateBlockerCode::FalseMerge => {
                checked_inc(&mut false_merge, "e4.case_findings.false_merge")?
            }
            GeoE4GateBlockerCode::AssignmentBudgetExceeded => checked_inc(
                &mut assignment_budget_exceeded,
                "e4.case_findings.assignment_budget_exceeded",
            )?,
            GeoE4GateBlockerCode::ComponentBudgetFallback => checked_inc(
                &mut component_budget_fallback,
                "e4.case_findings.component_budget_fallback",
            )?,
            GeoE4GateBlockerCode::ProofClassNotLiveComplete
            | GeoE4GateBlockerCode::PopulationDenominatorMismatch => {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::InvalidInput,
                    "Geo E4 gate assessment case finding uses a non-case blocker code",
                    [("code", format!("{:?}", finding.code))],
                ));
            }
        }
    }

    let planes = &assessment.planes;
    let solver_artifact_missing_expected = checked_difference(
        "e4.case_findings.solver_artifact_missing",
        planes.coverage.cases,
        planes.solver_exactness.solver_artifact_cases,
    )?;
    let residual_count_inexact_expected = sum_u64(
        [
            planes.solver_exactness.residual_count_saturated_cases,
            planes.solver_exactness.residual_count_unavailable_cases,
        ],
        "e4.case_findings.residual_count_inexact",
    )?;
    validate_e4_case_finding_count(
        GeoE4GateBlockerCode::EvidenceNoObservation,
        planes.coverage.evidence_no_observation_cases,
        evidence_no_observation,
    )?;
    validate_e4_case_finding_count(
        GeoE4GateBlockerCode::CandidateReachIncomplete,
        planes.candidate_reach.recall_failure_cases,
        candidate_reach_incomplete,
    )?;
    validate_e4_case_finding_count(
        GeoE4GateBlockerCode::SolverArtifactMissing,
        solver_artifact_missing_expected,
        solver_artifact_missing,
    )?;
    validate_e4_case_finding_count(
        GeoE4GateBlockerCode::ResidualCountInexact,
        residual_count_inexact_expected,
        residual_count_inexact,
    )?;
    validate_e4_case_finding_count(
        GeoE4GateBlockerCode::RhoFalsification,
        planes.admission.rho_falsification_cases,
        rho_falsification,
    )?;
    validate_e4_case_finding_count(
        GeoE4GateBlockerCode::FalseMerge,
        planes.truth_quality.false_merge_cases,
        false_merge,
    )?;
    validate_e4_case_finding_count(
        GeoE4GateBlockerCode::AssignmentBudgetExceeded,
        planes.solver_exactness.assignment_budget_exceeded_cases,
        assignment_budget_exceeded,
    )?;
    validate_e4_case_finding_count(
        GeoE4GateBlockerCode::ComponentBudgetFallback,
        planes.solver_exactness.component_budget_fallback_cases,
        component_budget_fallback,
    )?;
    Ok(())
}

fn validate_nonempty_trimmed_finding_text(
    field: &'static str,
    value: &str,
) -> Result<(), GeoPopulationError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo E4 gate assessment case finding text must be non-empty and trimmed",
            [(field, value)],
        ));
    }
    Ok(())
}

fn e4_case_finding_plane(code: GeoE4GateBlockerCode) -> Option<GeoE4GatePlane> {
    match code {
        GeoE4GateBlockerCode::EvidenceNoObservation => Some(GeoE4GatePlane::Coverage),
        GeoE4GateBlockerCode::CandidateReachIncomplete => Some(GeoE4GatePlane::CandidateReach),
        GeoE4GateBlockerCode::SolverArtifactMissing
        | GeoE4GateBlockerCode::ResidualCountInexact => Some(GeoE4GatePlane::SolverExactness),
        GeoE4GateBlockerCode::RhoFalsification => Some(GeoE4GatePlane::Admission),
        GeoE4GateBlockerCode::FalseMerge => Some(GeoE4GatePlane::TruthQuality),
        GeoE4GateBlockerCode::AssignmentBudgetExceeded
        | GeoE4GateBlockerCode::ComponentBudgetFallback => Some(GeoE4GatePlane::Cost),
        GeoE4GateBlockerCode::ProofClassNotLiveComplete
        | GeoE4GateBlockerCode::PopulationDenominatorMismatch => None,
    }
}

fn checked_difference(
    field: &'static str,
    total: u64,
    subset: u64,
) -> Result<u64, GeoPopulationError> {
    total
        .checked_sub(subset)
        .ok_or_else(|| summary_invariant_error("e4_gate_assessment", field, total, subset))
}

fn validate_e4_case_finding_count(
    code: GeoE4GateBlockerCode,
    expected: u64,
    actual: u64,
) -> Result<(), GeoPopulationError> {
    if actual != expected {
        return Err(summary_invariant_error(
            "e4_gate_assessment.case_findings",
            e4_case_finding_count_field(code),
            expected,
            actual,
        ));
    }
    Ok(())
}

fn e4_case_finding_count_field(code: GeoE4GateBlockerCode) -> &'static str {
    match code {
        GeoE4GateBlockerCode::EvidenceNoObservation => "evidence_no_observation",
        GeoE4GateBlockerCode::CandidateReachIncomplete => "candidate_reach_incomplete",
        GeoE4GateBlockerCode::SolverArtifactMissing => "solver_artifact_missing",
        GeoE4GateBlockerCode::ResidualCountInexact => "residual_count_inexact",
        GeoE4GateBlockerCode::RhoFalsification => "rho_falsification",
        GeoE4GateBlockerCode::FalseMerge => "false_merge",
        GeoE4GateBlockerCode::AssignmentBudgetExceeded => "assignment_budget_exceeded",
        GeoE4GateBlockerCode::ComponentBudgetFallback => "component_budget_fallback",
        GeoE4GateBlockerCode::ProofClassNotLiveComplete => "proof_class_not_live_complete",
        GeoE4GateBlockerCode::PopulationDenominatorMismatch => "population_denominator_mismatch",
    }
}

fn scored_false_merge(
    status: GeoPopulationCaseStatus,
    truth_model_in_residual: Option<bool>,
) -> bool {
    status == GeoPopulationCaseStatus::Resolved && truth_model_in_residual == Some(false)
}

fn is_candidate_truth_solver_abstention_status(status: GeoCandidateTruthRowStatus) -> bool {
    !matches!(status, GeoCandidateTruthRowStatus::Resolved)
}

fn is_candidate_truth_claim_abstention_status(
    status: GeoCandidateTruthRowStatus,
    candidate_reach: GeoCandidateReachStatus,
    truth_model_in_residual: Option<bool>,
) -> bool {
    candidate_reach != GeoCandidateReachStatus::Full
        || (status != GeoCandidateTruthRowStatus::Resolved
            && truth_model_in_residual != Some(false))
        || truth_model_in_residual.is_none()
}

fn is_representation_relative_exact(
    status: GeoCandidateTruthRowStatus,
    residual_count_complete: bool,
    residual_count_saturated: bool,
) -> bool {
    matches!(
        status,
        GeoCandidateTruthRowStatus::Resolved
            | GeoCandidateTruthRowStatus::Ambiguous
            | GeoCandidateTruthRowStatus::Conflict
    ) && residual_count_complete
        && !residual_count_saturated
}

fn validate_candidate_truth_case_evaluation(
    row: &GeoCandidateTruthCaseEvaluation,
) -> Result<(), GeoPopulationError> {
    validate_nonempty_canonical("row_id", &row.row_id)?;
    validate_nonempty_canonical("logical_subject_id", &row.logical_subject_id)?;
    validate_nonempty_canonical("subject_id", &row.subject_id)?;
    validate_nonempty_canonical("release_id", &row.release_id)?;
    let expected_truth_members = sum_u64(
        [row.truth_parcel_members, row.truth_building_members],
        "truth_members",
    )?;
    if row.truth_members != expected_truth_members {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo candidate/truth evaluation truth member fields are inconsistent",
            [
                ("row_id", row.row_id.clone()),
                ("truth_members", row.truth_members.to_string()),
                ("computed", expected_truth_members.to_string()),
            ],
        ));
    }
    let expected_reach = candidate_reach_status(row.truth_members, row.truth_members_in_universe)?;
    if row.candidate_reach != expected_reach {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo candidate/truth evaluation emitted reach inconsistent with truth counts",
            [
                ("row_id", row.row_id.as_str()),
                ("declared", candidate_reach_name(row.candidate_reach)),
                ("computed", candidate_reach_name(expected_reach)),
            ],
        ));
    }
    if row.candidate_reach != GeoCandidateReachStatus::Full
        && (row.solver_truth_scored
            || row.truth_model_in_residual.is_some()
            || row.false_merge
            || row.rho_falsification)
    {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo candidate/truth evaluation scored unreachable truth",
            [
                ("row_id", row.row_id.as_str()),
                ("candidate_reach", candidate_reach_name(row.candidate_reach)),
            ],
        ));
    }
    if row.solver_truth_scored != row.truth_model_in_residual.is_some() {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo candidate/truth evaluation truth scoring fields are inconsistent",
            [("row_id", row.row_id.as_str())],
        ));
    }
    if row.false_merge
        != (row.status == GeoCandidateTruthRowStatus::Resolved
            && row.truth_model_in_residual == Some(false))
    {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo candidate/truth evaluation false merge field is inconsistent",
            [("row_id", row.row_id.as_str())],
        ));
    }
    if row.rho_falsification != (row.truth_model_in_residual == Some(false)) {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo candidate/truth evaluation rho falsification field is inconsistent",
            [("row_id", row.row_id.as_str())],
        ));
    }
    if row.solver_abstained != is_candidate_truth_solver_abstention_status(row.status) {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo candidate/truth evaluation solver abstention field is inconsistent",
            [("row_id", row.row_id.as_str())],
        ));
    }
    let expected_claim_abstained = is_candidate_truth_claim_abstention_status(
        row.status,
        row.candidate_reach,
        row.truth_model_in_residual,
    );
    if row.claim_abstained != expected_claim_abstained {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo candidate/truth evaluation claim abstention field is inconsistent",
            [("row_id", row.row_id.as_str())],
        ));
    }
    if row.residual_model_count.is_some() != row.residual_count_complete {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo candidate/truth evaluation residual count fields are inconsistent",
            [("row_id", row.row_id.as_str())],
        ));
    }
    if row.residual_count_saturated && !row.residual_count_complete {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo candidate/truth evaluation emitted saturated incomplete residual count",
            [("row_id", row.row_id.as_str())],
        ));
    }
    if row.representation_relative_exact
        != is_representation_relative_exact(
            row.status,
            row.residual_count_complete,
            row.residual_count_saturated,
        )
    {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::Composition,
            "Geo candidate/truth evaluation exactness field is inconsistent",
            [("row_id", row.row_id.as_str())],
        ));
    }
    match row.status {
        GeoCandidateTruthRowStatus::UpstreamNoCandidateRequest => {
            if row.composition_request_digest.is_some()
                || row.solver_digest.is_some()
                || row.candidate_members != 0
                || row.truth_members_in_universe != 0
                || row.candidate_reach != GeoCandidateReachStatus::None
                || row.residual_count_complete
                || row.residual_model_count.is_some()
            {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::Composition,
                    "Geo candidate/truth evaluation fabricated solver state for an upstream no-reach row",
                    [("row_id", row.row_id.as_str())],
                ));
            }
        }
        GeoCandidateTruthRowStatus::AssignmentBudgetExceeded => {
            if row.solver_digest.is_some()
                || row.residual_count_complete
                || row.residual_model_count.is_some()
                || row.solver_truth_scored
            {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::Composition,
                    "Geo candidate/truth evaluation emitted solver-derived truth state for an assignment budget handoff",
                    [("row_id", row.row_id.as_str())],
                ));
            }
        }
        GeoCandidateTruthRowStatus::ComponentBudgetFallback => {
            if row.solver_digest.is_none()
                || row.residual_count_complete
                || row.residual_model_count.is_some()
            {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::Composition,
                    "Geo candidate/truth evaluation emitted complete residual claims for a component budget fallback",
                    [("row_id", row.row_id.as_str())],
                ));
            }
        }
        GeoCandidateTruthRowStatus::Resolved => {
            if row.solver_digest.is_none()
                || row.residual_model_count != Some(1)
                || row.residual_count_saturated
                || row.solver_abstained
            {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::Composition,
                    "Geo candidate/truth evaluation emitted a resolved row without an exact singleton residual",
                    [("row_id", row.row_id.as_str())],
                ));
            }
        }
        GeoCandidateTruthRowStatus::Ambiguous | GeoCandidateTruthRowStatus::Conflict => {
            if row.solver_digest.is_none() {
                return Err(GeoPopulationError::new(
                    GeoPopulationErrorCode::Composition,
                    "Geo candidate/truth evaluation emitted a solved row without a solver digest",
                    [("row_id", row.row_id.as_str())],
                ));
            }
        }
    }
    Ok(())
}

fn validate_case_evaluation(case: &GeoPopulationCaseEvaluation) -> Result<(), GeoPopulationError> {
    let expected_reach =
        candidate_reach_status(case.truth_members, case.truth_members_in_universe)?;
    if case.candidate_reach != expected_reach {
        return Err(case_invariant_error(
            case,
            "candidate_reach",
            "Geo population evaluation emitted a candidate reach bucket inconsistent with truth counts",
        ));
    }
    validate_truth_reach_by_grain("case.truth_reach_by_grain", &case.truth_reach_by_grain)?;
    if case.full_truth_recall != (case.candidate_reach == GeoCandidateReachStatus::Full) {
        return Err(case_invariant_error(
            case,
            "full_truth_recall",
            "Geo population evaluation emitted truth recall inconsistent with candidate reach",
        ));
    }
    if case.residual_count_saturated && !case.residual_count_complete {
        return Err(case_invariant_error(
            case,
            "residual_count_saturated",
            "Geo population evaluation emitted a saturated residual count without a complete residual count",
        ));
    }
    if case.residual_model_count.is_some() != case.residual_count_complete {
        return Err(case_invariant_error(
            case,
            "residual_model_count",
            "Geo population evaluation emitted residual count presence inconsistent with residual completeness",
        ));
    }
    if case.solver_truth_scored != case.truth_model_in_residual.is_some() {
        return Err(case_invariant_error(
            case,
            "solver_truth_scored",
            "Geo population evaluation emitted solver truth scoring inconsistent with residual truth membership",
        ));
    }
    if case.solver_truth_scored && case.candidate_reach != GeoCandidateReachStatus::Full {
        return Err(case_invariant_error(
            case,
            "solver_truth_scored",
            "Geo population evaluation scored solver truth before candidate reach was full",
        ));
    }
    if !case.backbone_complete
        && (case.backbone_true_positive_members != 0 || case.backbone_false_positive_members != 0)
    {
        return Err(case_invariant_error(
            case,
            "backbone_complete",
            "Geo population evaluation emitted backbone accuracy counts for an incomplete backbone",
        ));
    }
    if case.false_merge != scored_false_merge(case.status, case.truth_model_in_residual) {
        return Err(case_invariant_error(
            case,
            "false_merge",
            "Geo population evaluation emitted false merge inconsistent with resolved-singleton semantics",
        ));
    }
    if case.abstained != is_abstention_status(case.status) {
        return Err(case_invariant_error(
            case,
            "abstained",
            "Geo population evaluation emitted abstention inconsistent with case status",
        ));
    }
    if let Some(claim) = &case.resolved_claim {
        if case.status != GeoPopulationCaseStatus::Resolved {
            return Err(case_invariant_error(
                case,
                "resolved_claim",
                "Geo population evaluation emitted a resolved claim for a non-resolved case",
            ));
        }
        if checked_len(claim.candidate_members, "resolved_claim.candidate_members")?
            != case.candidate_members
        {
            return Err(case_invariant_error(
                case,
                "resolved_claim.candidate_members",
                "Geo population evaluation emitted a resolved claim whose candidate count does not match the case",
            ));
        }
        match claim.claim_class {
            GeoResolvedClaimClass::StructurallyForced => {
                if claim.hard_constraint_count != 0 || claim.hard_constraint_evaluations != 0 {
                    return Err(case_invariant_error(
                        case,
                        "resolved_claim.claim_class",
                        "Geo population evaluation emitted a structurally forced claim with admitted hard evidence",
                    ));
                }
            }
            GeoResolvedClaimClass::EvidentiallySupported => {
                if claim.hard_constraint_count == 0 && claim.hard_constraint_evaluations == 0 {
                    return Err(case_invariant_error(
                        case,
                        "resolved_claim.claim_class",
                        "Geo population evaluation emitted an evidence-supported claim without admitted hard evidence",
                    ));
                }
            }
        }
    }
    match case.status {
        GeoPopulationCaseStatus::AssignmentBudgetExceeded => {
            if case.solver_digest.is_some()
                || case.resolved_claim.is_some()
                || case.residual_count_complete
                || case.residual_model_count.is_some()
                || case.truth_model_in_residual.is_some()
                || case.solver_truth_scored
                || case.backbone_complete
                || case.false_merge
            {
                return Err(case_invariant_error(
                    case,
                    "assignment_budget_exceeded",
                    "Geo population evaluation emitted solver-derived fields for an assignment budget handoff",
                ));
            }
        }
        GeoPopulationCaseStatus::ComponentBudgetFallback => {
            if case.solver_digest.is_none()
                || case.resolved_claim.is_some()
                || case.residual_count_complete
                || case.residual_model_count.is_some()
                || case.backbone_complete
            {
                return Err(case_invariant_error(
                    case,
                    "component_budget_fallback",
                    "Geo population evaluation emitted complete residual or backbone claims for a component budget handoff",
                ));
            }
        }
        GeoPopulationCaseStatus::Resolved => {
            if case.solver_digest.is_none()
                || case.resolved_claim.is_none()
                || case.residual_model_count != Some(1)
                || case.residual_count_saturated
                || case.abstained
            {
                return Err(case_invariant_error(
                    case,
                    "resolved",
                    "Geo population evaluation emitted a resolved case without an exact singleton residual",
                ));
            }
        }
        GeoPopulationCaseStatus::Ambiguous | GeoPopulationCaseStatus::Conflict => {
            if case.solver_digest.is_none() || case.resolved_claim.is_some() {
                return Err(case_invariant_error(
                    case,
                    "solver_digest",
                    "Geo population evaluation emitted a solved case without a solver digest",
                ));
            }
        }
    }
    Ok(())
}

fn case_invariant_error(
    case: &GeoPopulationCaseEvaluation,
    field: &str,
    message: &'static str,
) -> GeoPopulationError {
    GeoPopulationError::new(
        GeoPopulationErrorCode::Composition,
        message,
        [("case_id", case.case_id.as_str()), ("field", field)],
    )
}

fn summarize(
    cases: &[GeoPopulationCaseEvaluation],
) -> Result<GeoPopulationSummary, GeoPopulationError> {
    let mut summary = GeoPopulationSummary {
        cases: checked_len(cases.len(), "cases")?,
        population_eligible_cases: 0,
        truth_planes: Vec::new(),
        resolved_cases: 0,
        evidentially_supported_resolved_cases: 0,
        structurally_forced_resolved_cases: 0,
        resolved_with_reach_not_full_cases: 0,
        ambiguous_cases: 0,
        conflict_cases: 0,
        assignment_budget_exceeded_cases: 0,
        component_budget_fallback_cases: 0,
        abstention_cases: 0,
        false_merge_cases: 0,
        full_truth_recall_cases: 0,
        candidate_reach_evaluated_cases: 0,
        candidate_reach_full_cases: 0,
        candidate_reach_partial_cases: 0,
        candidate_reach_none_cases: 0,
        truth_reach_by_grain: Vec::new(),
        candidate_recall_failure_cases: 0,
        evidence_no_observation_cases: 0,
        evidence_diagnostic_only_cases: 0,
        evidence_soft_preference_only_cases: 0,
        evidence_soft_and_diagnostic_only_cases: 0,
        evidence_hard_constraint_cases: 0,
        solver_truth_scored_cases: 0,
        solver_artifact_cases: 0,
        empirical_falsification_eligible_cases: 0,
        solver_truth_exclusion_cases: 0,
        residual_count_complete_cases: 0,
        residual_count_exact_cases: 0,
        residual_count_saturated_cases: 0,
        residual_count_unavailable_cases: 0,
        backbone_complete_cases: 0,
        truth_members: 0,
        truth_members_in_universe: 0,
        backbone_true_positive_members: 0,
        backbone_false_positive_members: 0,
    };
    let mut truth_planes = BTreeMap::<GeoTruthPlane, GeoPopulationTruthPlaneSummary>::new();
    let mut truth_reach_by_grain =
        BTreeMap::<GeoTruthRepresentationGrain, GeoPopulationTruthGrainSummary>::new();
    for case in cases {
        checked_inc(
            &mut summary.population_eligible_cases,
            "population_eligible_cases",
        )?;
        checked_inc(
            &mut summary.candidate_reach_evaluated_cases,
            "candidate_reach_evaluated_cases",
        )?;
        if case.solver_digest.is_some() {
            checked_inc(&mut summary.solver_artifact_cases, "solver_artifact_cases")?;
        }
        match case.status {
            GeoPopulationCaseStatus::Resolved => {
                checked_inc(&mut summary.resolved_cases, "resolved_cases")?;
                match case
                    .resolved_claim
                    .as_ref()
                    .map(|claim| claim.claim_class)
                    .ok_or_else(|| {
                        case_invariant_error(
                            case,
                            "resolved_claim",
                            "Geo population evaluation emitted a resolved case without a claim class",
                        )
                    })? {
                    GeoResolvedClaimClass::EvidentiallySupported => checked_inc(
                        &mut summary.evidentially_supported_resolved_cases,
                        "evidentially_supported_resolved_cases",
                    )?,
                    GeoResolvedClaimClass::StructurallyForced => checked_inc(
                        &mut summary.structurally_forced_resolved_cases,
                        "structurally_forced_resolved_cases",
                    )?,
                }
                if case.candidate_reach != GeoCandidateReachStatus::Full {
                    checked_inc(
                        &mut summary.resolved_with_reach_not_full_cases,
                        "resolved_with_reach_not_full_cases",
                    )?;
                }
            }
            GeoPopulationCaseStatus::Ambiguous => {
                checked_inc(&mut summary.ambiguous_cases, "ambiguous_cases")?;
                checked_inc(&mut summary.abstention_cases, "abstention_cases")?;
            }
            GeoPopulationCaseStatus::Conflict => {
                checked_inc(&mut summary.conflict_cases, "conflict_cases")?;
                checked_inc(&mut summary.abstention_cases, "abstention_cases")?;
            }
            GeoPopulationCaseStatus::AssignmentBudgetExceeded => {
                checked_inc(
                    &mut summary.assignment_budget_exceeded_cases,
                    "assignment_budget_exceeded_cases",
                )?;
                checked_inc(&mut summary.abstention_cases, "abstention_cases")?;
            }
            GeoPopulationCaseStatus::ComponentBudgetFallback => {
                checked_inc(
                    &mut summary.component_budget_fallback_cases,
                    "component_budget_fallback_cases",
                )?;
                checked_inc(&mut summary.abstention_cases, "abstention_cases")?;
            }
        }
        match case.candidate_reach {
            GeoCandidateReachStatus::Full => {
                checked_inc(
                    &mut summary.candidate_reach_full_cases,
                    "candidate_reach_full_cases",
                )?;
            }
            GeoCandidateReachStatus::Partial => {
                checked_inc(
                    &mut summary.candidate_reach_partial_cases,
                    "candidate_reach_partial_cases",
                )?;
            }
            GeoCandidateReachStatus::None => {
                checked_inc(
                    &mut summary.candidate_reach_none_cases,
                    "candidate_reach_none_cases",
                )?;
            }
        }
        match case.evidence_coverage {
            GeoEvidenceCoverageStatus::NoObservations => {
                checked_inc(
                    &mut summary.evidence_no_observation_cases,
                    "evidence_no_observation_cases",
                )?;
            }
            GeoEvidenceCoverageStatus::DiagnosticOnly => {
                checked_inc(
                    &mut summary.evidence_diagnostic_only_cases,
                    "evidence_diagnostic_only_cases",
                )?;
            }
            GeoEvidenceCoverageStatus::SoftPreferenceOnly => {
                checked_inc(
                    &mut summary.evidence_soft_preference_only_cases,
                    "evidence_soft_preference_only_cases",
                )?;
            }
            GeoEvidenceCoverageStatus::SoftAndDiagnosticOnly => {
                checked_inc(
                    &mut summary.evidence_soft_and_diagnostic_only_cases,
                    "evidence_soft_and_diagnostic_only_cases",
                )?;
            }
            GeoEvidenceCoverageStatus::HardConstraintPresent => {
                checked_inc(
                    &mut summary.evidence_hard_constraint_cases,
                    "evidence_hard_constraint_cases",
                )?;
            }
        }
        if case.full_truth_recall {
            checked_inc(
                &mut summary.full_truth_recall_cases,
                "full_truth_recall_cases",
            )?;
        } else {
            checked_inc(
                &mut summary.candidate_recall_failure_cases,
                "candidate_recall_failure_cases",
            )?;
        }
        if case.solver_truth_scored {
            checked_inc(
                &mut summary.solver_truth_scored_cases,
                "solver_truth_scored_cases",
            )?;
            checked_inc(
                &mut summary.empirical_falsification_eligible_cases,
                "empirical_falsification_eligible_cases",
            )?;
        }
        if case.truth_model_in_residual == Some(false) {
            checked_inc(
                &mut summary.solver_truth_exclusion_cases,
                "solver_truth_exclusion_cases",
            )?;
        }
        if case.residual_count_complete {
            checked_inc(
                &mut summary.residual_count_complete_cases,
                "residual_count_complete_cases",
            )?;
            if is_residual_count_exact_case(case) {
                checked_inc(
                    &mut summary.residual_count_exact_cases,
                    "residual_count_exact_cases",
                )?;
            }
        } else {
            checked_inc(
                &mut summary.residual_count_unavailable_cases,
                "residual_count_unavailable_cases",
            )?;
        }
        if case.residual_count_saturated {
            checked_inc(
                &mut summary.residual_count_saturated_cases,
                "residual_count_saturated_cases",
            )?;
        }
        if case.backbone_complete {
            checked_inc(
                &mut summary.backbone_complete_cases,
                "backbone_complete_cases",
            )?;
        }
        if case.false_merge {
            checked_inc(&mut summary.false_merge_cases, "false_merge_cases")?;
        }
        checked_add(
            &mut summary.truth_members,
            case.truth_members,
            "truth_members",
        )?;
        checked_add(
            &mut summary.truth_members_in_universe,
            case.truth_members_in_universe,
            "truth_members_in_universe",
        )?;
        for reach in &case.truth_reach_by_grain {
            truth_reach_by_grain
                .entry(reach.grain)
                .or_insert_with(|| GeoPopulationTruthGrainSummary::new(reach.grain))
                .record(reach)?;
        }
        checked_add(
            &mut summary.backbone_true_positive_members,
            case.backbone_true_positive_members,
            "backbone_true_positive_members",
        )?;
        checked_add(
            &mut summary.backbone_false_positive_members,
            case.backbone_false_positive_members,
            "backbone_false_positive_members",
        )?;
        truth_planes
            .entry(case.truth_plane)
            .or_insert_with(|| GeoPopulationTruthPlaneSummary::new(case.truth_plane))
            .record(case)?;
    }
    summary.truth_reach_by_grain = truth_reach_by_grain.into_values().collect();
    summary.truth_planes = truth_planes.into_values().collect();
    validate_summary(&summary)?;
    Ok(summary)
}

fn summarize_candidate_truth_evaluations(
    gate: &GeoCandidateTruthGate,
    rows: &[GeoCandidateTruthCaseEvaluation],
) -> Result<GeoCandidateTruthEvaluationSummary, GeoPopulationError> {
    validate_candidate_truth_gate(gate)?;
    let mut logical_subject_ids = BTreeSet::new();
    let mut logical_subject_release_keys = BTreeSet::new();
    let mut logical_subject_releases = BTreeMap::<String, BTreeSet<String>>::new();
    let mut multi_parcel_logical_subject_ids = BTreeSet::new();
    let mut logical_subject_truth_planes = BTreeMap::new();
    let mut plane_summaries = BTreeMap::<
        GeoTruthPlane,
        (
            BTreeSet<String>,
            BTreeSet<String>,
            BTreeSet<String>,
            GeoCandidateTruthPlaneSummary,
        ),
    >::new();
    let mut summary = GeoCandidateTruthEvaluationSummary {
        gate: gate.clone(),
        logical_subjects: 0,
        release_validated_logical_subjects: 0,
        frozen_e4_h7_genuine_multi_parcel_subjects: 0,
        release_rows: 0,
        frozen_e4_h7_population_subject_gate_passed: false,
        frozen_e4_h7_population_subject_deficit: 0,
        truth_planes: Vec::new(),
        candidate_reach_full_release_rows: 0,
        candidate_reach_partial_release_rows: 0,
        candidate_reach_none_release_rows: 0,
        candidate_recall_failure_release_rows: 0,
        solver_artifact_release_rows: 0,
        representation_relative_exact_release_rows: 0,
        solver_truth_scored_release_rows: 0,
        solver_truth_retained_release_rows: 0,
        rho_falsification_release_rows: 0,
        false_merge_release_rows: 0,
        resolved_release_rows: 0,
        ambiguous_release_rows: 0,
        conflict_release_rows: 0,
        assignment_budget_exceeded_release_rows: 0,
        component_budget_fallback_release_rows: 0,
        upstream_no_candidate_request_release_rows: 0,
    };

    for row in rows {
        if !logical_subject_release_keys
            .insert((row.logical_subject_id.clone(), row.release_id.clone()))
        {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo candidate/truth evaluation repeats one logical subject/release measurement",
                [
                    ("logical_subject_id", row.logical_subject_id.as_str()),
                    ("release_id", row.release_id.as_str()),
                ],
            ));
        }
        logical_subject_ids.insert(row.logical_subject_id.clone());
        logical_subject_releases
            .entry(row.logical_subject_id.clone())
            .or_default()
            .insert(row.release_id.clone());
        if let Some(previous) =
            logical_subject_truth_planes.insert(row.logical_subject_id.clone(), row.truth_plane)
            && previous != row.truth_plane
        {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo candidate/truth evaluation assigns one logical subject to multiple truth planes",
                [
                    ("logical_subject_id", row.logical_subject_id.clone()),
                    ("previous_truth_plane", format!("{previous:?}")),
                    ("current_truth_plane", format!("{:?}", row.truth_plane)),
                ],
            ));
        }
        if row.truth_parcel_members >= 2 {
            multi_parcel_logical_subject_ids.insert(row.logical_subject_id.clone());
        }
        checked_inc(&mut summary.release_rows, "release_rows")?;
        match row.candidate_reach {
            GeoCandidateReachStatus::Full => {
                checked_inc(
                    &mut summary.candidate_reach_full_release_rows,
                    "candidate_reach_full_release_rows",
                )?;
            }
            GeoCandidateReachStatus::Partial => {
                checked_inc(
                    &mut summary.candidate_reach_partial_release_rows,
                    "candidate_reach_partial_release_rows",
                )?;
                checked_inc(
                    &mut summary.candidate_recall_failure_release_rows,
                    "candidate_recall_failure_release_rows",
                )?;
            }
            GeoCandidateReachStatus::None => {
                checked_inc(
                    &mut summary.candidate_reach_none_release_rows,
                    "candidate_reach_none_release_rows",
                )?;
                checked_inc(
                    &mut summary.candidate_recall_failure_release_rows,
                    "candidate_recall_failure_release_rows",
                )?;
            }
        }
        if row.solver_digest.is_some() {
            checked_inc(
                &mut summary.solver_artifact_release_rows,
                "solver_artifact_release_rows",
            )?;
        }
        if row.representation_relative_exact {
            checked_inc(
                &mut summary.representation_relative_exact_release_rows,
                "representation_relative_exact_release_rows",
            )?;
        }
        if row.solver_truth_scored {
            checked_inc(
                &mut summary.solver_truth_scored_release_rows,
                "solver_truth_scored_release_rows",
            )?;
        }
        if row.truth_model_in_residual == Some(true) {
            checked_inc(
                &mut summary.solver_truth_retained_release_rows,
                "solver_truth_retained_release_rows",
            )?;
        }
        if row.rho_falsification {
            checked_inc(
                &mut summary.rho_falsification_release_rows,
                "rho_falsification_release_rows",
            )?;
        }
        if row.false_merge {
            checked_inc(
                &mut summary.false_merge_release_rows,
                "false_merge_release_rows",
            )?;
        }
        match row.status {
            GeoCandidateTruthRowStatus::Resolved => {
                checked_inc(&mut summary.resolved_release_rows, "resolved_release_rows")?;
            }
            GeoCandidateTruthRowStatus::Ambiguous => {
                checked_inc(
                    &mut summary.ambiguous_release_rows,
                    "ambiguous_release_rows",
                )?;
            }
            GeoCandidateTruthRowStatus::Conflict => {
                checked_inc(&mut summary.conflict_release_rows, "conflict_release_rows")?;
            }
            GeoCandidateTruthRowStatus::AssignmentBudgetExceeded => {
                checked_inc(
                    &mut summary.assignment_budget_exceeded_release_rows,
                    "assignment_budget_exceeded_release_rows",
                )?;
            }
            GeoCandidateTruthRowStatus::ComponentBudgetFallback => {
                checked_inc(
                    &mut summary.component_budget_fallback_release_rows,
                    "component_budget_fallback_release_rows",
                )?;
            }
            GeoCandidateTruthRowStatus::UpstreamNoCandidateRequest => {
                checked_inc(
                    &mut summary.upstream_no_candidate_request_release_rows,
                    "upstream_no_candidate_request_release_rows",
                )?;
            }
        }
        let entry = plane_summaries.entry(row.truth_plane).or_insert_with(|| {
            (
                BTreeSet::new(),
                BTreeSet::new(),
                BTreeSet::new(),
                GeoCandidateTruthPlaneSummary::new(row.truth_plane),
            )
        });
        entry.0.insert(row.logical_subject_id.clone());
        entry.3.record(row)?;
    }

    let required_release_ids = gate
        .required_release_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let release_validated_logical_subject_ids = logical_subject_releases
        .iter()
        .filter(|(_, releases)| {
            releases.len() == required_release_ids.len()
                && required_release_ids
                    .iter()
                    .all(|release_id| releases.contains(release_id))
        })
        .map(|(logical_subject_id, _)| logical_subject_id.clone())
        .collect::<BTreeSet<_>>();
    let frozen_e4_h7_genuine_multi_parcel_subject_ids = release_validated_logical_subject_ids
        .intersection(&multi_parcel_logical_subject_ids)
        .cloned()
        .collect::<BTreeSet<_>>();

    for logical_subject_id in &release_validated_logical_subject_ids {
        let truth_plane = logical_subject_truth_planes
            .get(logical_subject_id)
            .expect("validated logical subject plane");
        plane_summaries
            .get_mut(truth_plane)
            .expect("validated logical subject plane summary")
            .1
            .insert(logical_subject_id.clone());
    }
    for logical_subject_id in &frozen_e4_h7_genuine_multi_parcel_subject_ids {
        let truth_plane = logical_subject_truth_planes
            .get(logical_subject_id)
            .expect("validated logical subject plane");
        plane_summaries
            .get_mut(truth_plane)
            .expect("validated logical subject plane summary")
            .2
            .insert(logical_subject_id.clone());
    }

    summary.logical_subjects = checked_len(logical_subject_ids.len(), "logical_subjects")?;
    summary.release_validated_logical_subjects = checked_len(
        release_validated_logical_subject_ids.len(),
        "release_validated_logical_subjects",
    )?;
    summary.frozen_e4_h7_genuine_multi_parcel_subjects = checked_len(
        frozen_e4_h7_genuine_multi_parcel_subject_ids.len(),
        "frozen_e4_h7_genuine_multi_parcel_subjects",
    )?;
    summary.frozen_e4_h7_population_subject_gate_passed =
        summary.frozen_e4_h7_genuine_multi_parcel_subjects == gate.required_subjects;
    summary.frozen_e4_h7_population_subject_deficit = gate
        .required_subjects
        .saturating_sub(summary.frozen_e4_h7_genuine_multi_parcel_subjects);
    for (
        _,
        (
            logical_subjects,
            release_validated_logical_subjects,
            frozen_e4_h7_genuine_multi_parcel_subjects,
            mut plane_summary,
        ),
    ) in plane_summaries
    {
        plane_summary.logical_subjects =
            checked_len(logical_subjects.len(), "truth_plane.logical_subjects")?;
        plane_summary.release_validated_logical_subjects = checked_len(
            release_validated_logical_subjects.len(),
            "truth_plane.release_validated_logical_subjects",
        )?;
        plane_summary.frozen_e4_h7_genuine_multi_parcel_subjects = checked_len(
            frozen_e4_h7_genuine_multi_parcel_subjects.len(),
            "truth_plane.frozen_e4_h7_genuine_multi_parcel_subjects",
        )?;
        summary.truth_planes.push(plane_summary);
    }
    validate_candidate_truth_summary(&summary)?;
    Ok(summary)
}

fn validate_summary(summary: &GeoPopulationSummary) -> Result<(), GeoPopulationError> {
    validate_summary_denominators(
        "summary",
        summary.cases,
        summary.population_eligible_cases,
        summary.resolved_cases,
        summary.evidentially_supported_resolved_cases,
        summary.structurally_forced_resolved_cases,
        summary.resolved_with_reach_not_full_cases,
        summary.candidate_reach_evaluated_cases,
        summary.candidate_reach_full_cases,
        summary.candidate_reach_partial_cases,
        summary.candidate_reach_none_cases,
        summary.solver_truth_scored_cases,
        summary.empirical_falsification_eligible_cases,
        summary.solver_truth_exclusion_cases,
        summary.residual_count_complete_cases,
        summary.residual_count_saturated_cases,
        summary.residual_count_exact_cases,
    )?;
    validate_truth_plane_sums(summary)?;
    validate_truth_grain_summaries(
        "summary.truth_reach_by_grain",
        &summary.truth_reach_by_grain,
    )?;
    validate_truth_grain_plane_sums(summary)?;
    for plane in &summary.truth_planes {
        validate_summary_denominators(
            "truth_plane",
            plane.cases,
            plane.population_eligible_cases,
            plane.resolved_cases,
            plane.evidentially_supported_resolved_cases,
            plane.structurally_forced_resolved_cases,
            plane.resolved_with_reach_not_full_cases,
            plane.candidate_reach_evaluated_cases,
            plane.candidate_reach_full_cases,
            plane.candidate_reach_partial_cases,
            plane.candidate_reach_none_cases,
            plane.solver_truth_scored_cases,
            plane.empirical_falsification_eligible_cases,
            plane.solver_truth_exclusion_cases,
            plane.residual_count_complete_cases,
            plane.residual_count_saturated_cases,
            plane.residual_count_exact_cases,
        )?;
        validate_truth_grain_summaries(
            "truth_plane.truth_reach_by_grain",
            &plane.truth_reach_by_grain,
        )?;
    }
    Ok(())
}

fn validate_candidate_truth_summary(
    summary: &GeoCandidateTruthEvaluationSummary,
) -> Result<(), GeoPopulationError> {
    validate_candidate_truth_gate(&summary.gate)?;
    let reach_rows = sum_u64(
        [
            summary.candidate_reach_full_release_rows,
            summary.candidate_reach_partial_release_rows,
            summary.candidate_reach_none_release_rows,
        ],
        "candidate_reach_release_rows",
    )?;
    if reach_rows != summary.release_rows {
        return Err(summary_invariant_error(
            "candidate_truth_summary",
            "candidate_reach_release_rows",
            summary.release_rows,
            reach_rows,
        ));
    }
    let reach_failure_rows = sum_u64(
        [
            summary.candidate_reach_partial_release_rows,
            summary.candidate_reach_none_release_rows,
        ],
        "candidate_recall_failure_release_rows",
    )?;
    if reach_failure_rows != summary.candidate_recall_failure_release_rows {
        return Err(summary_invariant_error(
            "candidate_truth_summary",
            "candidate_recall_failure_release_rows",
            reach_failure_rows,
            summary.candidate_recall_failure_release_rows,
        ));
    }
    let scored_truth_rows = sum_u64(
        [
            summary.solver_truth_retained_release_rows,
            summary.rho_falsification_release_rows,
        ],
        "solver_truth_scored_release_rows",
    )?;
    if scored_truth_rows != summary.solver_truth_scored_release_rows {
        return Err(summary_invariant_error(
            "candidate_truth_summary",
            "solver_truth_scored_release_rows",
            summary.solver_truth_scored_release_rows,
            scored_truth_rows,
        ));
    }
    if summary.solver_truth_scored_release_rows > summary.candidate_reach_full_release_rows {
        return Err(summary_invariant_error(
            "candidate_truth_summary",
            "solver_truth_scored_release_rows",
            summary.candidate_reach_full_release_rows,
            summary.solver_truth_scored_release_rows,
        ));
    }
    let status_rows = sum_u64(
        [
            summary.resolved_release_rows,
            summary.ambiguous_release_rows,
            summary.conflict_release_rows,
            summary.assignment_budget_exceeded_release_rows,
            summary.component_budget_fallback_release_rows,
            summary.upstream_no_candidate_request_release_rows,
        ],
        "status_release_rows",
    )?;
    if status_rows != summary.release_rows {
        return Err(summary_invariant_error(
            "candidate_truth_summary",
            "status_release_rows",
            summary.release_rows,
            status_rows,
        ));
    }
    if summary.release_validated_logical_subjects > summary.logical_subjects {
        return Err(summary_invariant_error(
            "candidate_truth_summary",
            "release_validated_logical_subjects",
            summary.logical_subjects,
            summary.release_validated_logical_subjects,
        ));
    }
    if summary.frozen_e4_h7_genuine_multi_parcel_subjects
        > summary.release_validated_logical_subjects
    {
        return Err(summary_invariant_error(
            "candidate_truth_summary",
            "frozen_e4_h7_genuine_multi_parcel_subjects",
            summary.release_validated_logical_subjects,
            summary.frozen_e4_h7_genuine_multi_parcel_subjects,
        ));
    }
    let expected_deficit = summary
        .gate
        .required_subjects
        .saturating_sub(summary.frozen_e4_h7_genuine_multi_parcel_subjects);
    if summary.frozen_e4_h7_population_subject_deficit != expected_deficit {
        return Err(summary_invariant_error(
            "candidate_truth_summary",
            "frozen_e4_h7_population_subject_deficit",
            expected_deficit,
            summary.frozen_e4_h7_population_subject_deficit,
        ));
    }
    let expected_gate =
        summary.frozen_e4_h7_genuine_multi_parcel_subjects == summary.gate.required_subjects;
    if summary.frozen_e4_h7_population_subject_gate_passed != expected_gate {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo candidate/truth frozen E4/H7 subject gate field is internally inconsistent",
            [
                ("logical_subjects", summary.logical_subjects.to_string()),
                (
                    "frozen_e4_h7_genuine_multi_parcel_subjects",
                    summary
                        .frozen_e4_h7_genuine_multi_parcel_subjects
                        .to_string(),
                ),
                (
                    "required_subjects",
                    summary.gate.required_subjects.to_string(),
                ),
            ],
        ));
    }
    for plane in &summary.truth_planes {
        if plane.release_validated_logical_subjects > plane.logical_subjects {
            return Err(summary_invariant_error(
                "candidate_truth_plane",
                "release_validated_logical_subjects",
                plane.logical_subjects,
                plane.release_validated_logical_subjects,
            ));
        }
        if plane.frozen_e4_h7_genuine_multi_parcel_subjects
            > plane.release_validated_logical_subjects
        {
            return Err(summary_invariant_error(
                "candidate_truth_plane",
                "frozen_e4_h7_genuine_multi_parcel_subjects",
                plane.release_validated_logical_subjects,
                plane.frozen_e4_h7_genuine_multi_parcel_subjects,
            ));
        }
    }
    validate_candidate_truth_plane_sum(
        "logical_subjects",
        summary.logical_subjects,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.logical_subjects),
    )?;
    validate_candidate_truth_plane_sum(
        "release_validated_logical_subjects",
        summary.release_validated_logical_subjects,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.release_validated_logical_subjects),
    )?;
    validate_candidate_truth_plane_sum(
        "frozen_e4_h7_genuine_multi_parcel_subjects",
        summary.frozen_e4_h7_genuine_multi_parcel_subjects,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.frozen_e4_h7_genuine_multi_parcel_subjects),
    )?;
    validate_candidate_truth_plane_sum(
        "release_rows",
        summary.release_rows,
        summary.truth_planes.iter().map(|plane| plane.release_rows),
    )?;
    validate_candidate_truth_plane_sum(
        "candidate_reach_full_release_rows",
        summary.candidate_reach_full_release_rows,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.candidate_reach_full_release_rows),
    )?;
    validate_candidate_truth_plane_sum(
        "candidate_reach_partial_release_rows",
        summary.candidate_reach_partial_release_rows,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.candidate_reach_partial_release_rows),
    )?;
    validate_candidate_truth_plane_sum(
        "candidate_reach_none_release_rows",
        summary.candidate_reach_none_release_rows,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.candidate_reach_none_release_rows),
    )?;
    validate_candidate_truth_plane_sum(
        "solver_truth_scored_release_rows",
        summary.solver_truth_scored_release_rows,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.solver_truth_scored_release_rows),
    )?;
    validate_candidate_truth_plane_sum(
        "rho_falsification_release_rows",
        summary.rho_falsification_release_rows,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.rho_falsification_release_rows),
    )
}

fn validate_candidate_truth_plane_sum(
    field: &'static str,
    expected: u64,
    values: impl IntoIterator<Item = u64>,
) -> Result<(), GeoPopulationError> {
    let actual = sum_u64(values, field)?;
    if actual != expected {
        return Err(summary_invariant_error(
            "candidate_truth_planes_sum",
            field,
            expected,
            actual,
        ));
    }
    Ok(())
}

fn validate_truth_grain_summaries(
    scope: &'static str,
    summaries: &[GeoPopulationTruthGrainSummary],
) -> Result<(), GeoPopulationError> {
    let mut previous = None;
    for summary in summaries {
        if let Some(previous_grain) = previous
            && previous_grain >= summary.grain
        {
            return Err(GeoPopulationError::new(
                GeoPopulationErrorCode::InvalidInput,
                "Geo population truth grain summaries must be sorted and unique",
                [
                    ("scope", scope.to_string()),
                    ("grain", format!("{:?}", summary.grain)),
                ],
            ));
        }
        previous = Some(summary.grain);
        if summary.cases == 0 {
            return Err(summary_invariant_error(
                scope,
                "truth_reach_by_grain.cases",
                1,
                summary.cases,
            ));
        }
        if summary.truth_members_in_universe > summary.truth_members {
            return Err(summary_invariant_error(
                scope,
                "truth_reach_by_grain.truth_members_in_universe",
                summary.truth_members,
                summary.truth_members_in_universe,
            ));
        }
        let reach_cases = sum_u64(
            [
                summary.candidate_reach_full_cases,
                summary.candidate_reach_partial_cases,
                summary.candidate_reach_none_cases,
            ],
            "truth_reach_by_grain.candidate_reach_cases",
        )?;
        if reach_cases != summary.cases {
            return Err(summary_invariant_error(
                scope,
                "truth_reach_by_grain.candidate_reach_cases",
                summary.cases,
                reach_cases,
            ));
        }
    }
    Ok(())
}

fn validate_truth_grain_plane_sums(
    summary: &GeoPopulationSummary,
) -> Result<(), GeoPopulationError> {
    let mut expected = Vec::<GeoPopulationTruthGrainSummary>::new();
    for plane in &summary.truth_planes {
        for grain_summary in &plane.truth_reach_by_grain {
            let index = match expected
                .binary_search_by_key(&grain_summary.grain, |summary| summary.grain)
            {
                Ok(index) => index,
                Err(index) => {
                    expected.insert(
                        index,
                        GeoPopulationTruthGrainSummary::new(grain_summary.grain),
                    );
                    index
                }
            };
            expected[index].record_summary(grain_summary)?;
        }
    }
    if summary.truth_reach_by_grain != expected {
        return Err(GeoPopulationError::new(
            GeoPopulationErrorCode::InvalidInput,
            "Geo population truth grain summary does not match truth-plane sums",
            [("field", "truth_reach_by_grain")],
        ));
    }
    Ok(())
}

fn validate_truth_plane_sums(summary: &GeoPopulationSummary) -> Result<(), GeoPopulationError> {
    validate_truth_plane_sum(
        "cases",
        summary.cases,
        summary.truth_planes.iter().map(|plane| plane.cases),
    )?;
    validate_truth_plane_sum(
        "population_eligible_cases",
        summary.population_eligible_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.population_eligible_cases),
    )?;
    validate_truth_plane_sum(
        "resolved_cases",
        summary.resolved_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.resolved_cases),
    )?;
    validate_truth_plane_sum(
        "evidentially_supported_resolved_cases",
        summary.evidentially_supported_resolved_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.evidentially_supported_resolved_cases),
    )?;
    validate_truth_plane_sum(
        "structurally_forced_resolved_cases",
        summary.structurally_forced_resolved_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.structurally_forced_resolved_cases),
    )?;
    validate_truth_plane_sum(
        "resolved_with_reach_not_full_cases",
        summary.resolved_with_reach_not_full_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.resolved_with_reach_not_full_cases),
    )?;
    validate_truth_plane_sum(
        "ambiguous_cases",
        summary.ambiguous_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.ambiguous_cases),
    )?;
    validate_truth_plane_sum(
        "conflict_cases",
        summary.conflict_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.conflict_cases),
    )?;
    validate_truth_plane_sum(
        "abstention_cases",
        summary.abstention_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.abstention_cases),
    )?;
    validate_truth_plane_sum(
        "false_merge_cases",
        summary.false_merge_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.false_merge_cases),
    )?;
    validate_truth_plane_sum(
        "candidate_reach_evaluated_cases",
        summary.candidate_reach_evaluated_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.candidate_reach_evaluated_cases),
    )?;
    validate_truth_plane_sum(
        "candidate_reach_full_cases",
        summary.candidate_reach_full_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.candidate_reach_full_cases),
    )?;
    validate_truth_plane_sum(
        "candidate_reach_partial_cases",
        summary.candidate_reach_partial_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.candidate_reach_partial_cases),
    )?;
    validate_truth_plane_sum(
        "candidate_reach_none_cases",
        summary.candidate_reach_none_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.candidate_reach_none_cases),
    )?;
    validate_truth_plane_sum(
        "solver_truth_scored_cases",
        summary.solver_truth_scored_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.solver_truth_scored_cases),
    )?;
    validate_truth_plane_sum(
        "solver_artifact_cases",
        summary.solver_artifact_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.solver_artifact_cases),
    )?;
    validate_truth_plane_sum(
        "empirical_falsification_eligible_cases",
        summary.empirical_falsification_eligible_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.empirical_falsification_eligible_cases),
    )?;
    validate_truth_plane_sum(
        "solver_truth_exclusion_cases",
        summary.solver_truth_exclusion_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.solver_truth_exclusion_cases),
    )?;
    validate_truth_plane_sum(
        "residual_count_complete_cases",
        summary.residual_count_complete_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.residual_count_complete_cases),
    )?;
    validate_truth_plane_sum(
        "residual_count_exact_cases",
        summary.residual_count_exact_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.residual_count_exact_cases),
    )?;
    validate_truth_plane_sum(
        "residual_count_saturated_cases",
        summary.residual_count_saturated_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.residual_count_saturated_cases),
    )?;
    validate_truth_plane_sum(
        "residual_count_unavailable_cases",
        summary.residual_count_unavailable_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.residual_count_unavailable_cases),
    )?;
    validate_truth_plane_sum(
        "component_budget_fallback_cases",
        summary.component_budget_fallback_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.component_budget_fallback_cases),
    )?;
    validate_truth_plane_sum(
        "assignment_budget_exceeded_cases",
        summary.assignment_budget_exceeded_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.assignment_budget_exceeded_cases),
    )?;
    validate_truth_plane_sum(
        "evidence_no_observation_cases",
        summary.evidence_no_observation_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.evidence_no_observation_cases),
    )?;
    validate_truth_plane_sum(
        "evidence_diagnostic_only_cases",
        summary.evidence_diagnostic_only_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.evidence_diagnostic_only_cases),
    )?;
    validate_truth_plane_sum(
        "evidence_soft_preference_only_cases",
        summary.evidence_soft_preference_only_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.evidence_soft_preference_only_cases),
    )?;
    validate_truth_plane_sum(
        "evidence_soft_and_diagnostic_only_cases",
        summary.evidence_soft_and_diagnostic_only_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.evidence_soft_and_diagnostic_only_cases),
    )?;
    validate_truth_plane_sum(
        "evidence_hard_constraint_cases",
        summary.evidence_hard_constraint_cases,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.evidence_hard_constraint_cases),
    )?;
    validate_truth_plane_sum(
        "truth_members",
        summary.truth_members,
        summary.truth_planes.iter().map(|plane| plane.truth_members),
    )?;
    validate_truth_plane_sum(
        "truth_members_in_universe",
        summary.truth_members_in_universe,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.truth_members_in_universe),
    )?;
    validate_truth_plane_sum(
        "backbone_true_positive_members",
        summary.backbone_true_positive_members,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.backbone_true_positive_members),
    )?;
    validate_truth_plane_sum(
        "backbone_false_positive_members",
        summary.backbone_false_positive_members,
        summary
            .truth_planes
            .iter()
            .map(|plane| plane.backbone_false_positive_members),
    )
}

fn validate_truth_plane_sum(
    field: &'static str,
    expected: u64,
    values: impl IntoIterator<Item = u64>,
) -> Result<(), GeoPopulationError> {
    let actual = sum_u64(values, field)?;
    if actual != expected {
        return Err(summary_invariant_error(
            "truth_planes_sum",
            field,
            expected,
            actual,
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_summary_denominators(
    scope: &'static str,
    cases: u64,
    population_eligible_cases: u64,
    resolved_cases: u64,
    evidentially_supported_resolved_cases: u64,
    structurally_forced_resolved_cases: u64,
    resolved_with_reach_not_full_cases: u64,
    candidate_reach_evaluated_cases: u64,
    candidate_reach_full_cases: u64,
    candidate_reach_partial_cases: u64,
    candidate_reach_none_cases: u64,
    solver_truth_scored_cases: u64,
    empirical_falsification_eligible_cases: u64,
    solver_truth_exclusion_cases: u64,
    residual_count_complete_cases: u64,
    residual_count_saturated_cases: u64,
    residual_count_exact_cases: u64,
) -> Result<(), GeoPopulationError> {
    if population_eligible_cases != cases {
        return Err(summary_invariant_error(
            scope,
            "population_eligible_cases",
            cases,
            population_eligible_cases,
        ));
    }
    let reach_cases = sum_u64(
        [
            candidate_reach_full_cases,
            candidate_reach_partial_cases,
            candidate_reach_none_cases,
        ],
        "candidate_reach_cases",
    )?;
    if candidate_reach_evaluated_cases != reach_cases {
        return Err(summary_invariant_error(
            scope,
            "candidate_reach_evaluated_cases",
            reach_cases,
            candidate_reach_evaluated_cases,
        ));
    }
    if candidate_reach_evaluated_cases != population_eligible_cases {
        return Err(summary_invariant_error(
            scope,
            "candidate_reach_evaluated_cases",
            population_eligible_cases,
            candidate_reach_evaluated_cases,
        ));
    }
    let resolved_class_cases = sum_u64(
        [
            evidentially_supported_resolved_cases,
            structurally_forced_resolved_cases,
        ],
        "resolved_class_cases",
    )?;
    if resolved_cases != resolved_class_cases {
        return Err(summary_invariant_error(
            scope,
            "resolved_class_cases",
            resolved_cases,
            resolved_class_cases,
        ));
    }
    if resolved_with_reach_not_full_cases > resolved_cases {
        return Err(summary_invariant_error(
            scope,
            "resolved_with_reach_not_full_cases",
            resolved_cases,
            resolved_with_reach_not_full_cases,
        ));
    }
    if empirical_falsification_eligible_cases != solver_truth_scored_cases {
        return Err(summary_invariant_error(
            scope,
            "empirical_falsification_eligible_cases",
            solver_truth_scored_cases,
            empirical_falsification_eligible_cases,
        ));
    }
    if solver_truth_exclusion_cases > empirical_falsification_eligible_cases {
        return Err(summary_invariant_error(
            scope,
            "solver_truth_exclusion_cases",
            empirical_falsification_eligible_cases,
            solver_truth_exclusion_cases,
        ));
    }
    let exact_or_saturated_cases = sum_u64(
        [residual_count_exact_cases, residual_count_saturated_cases],
        "residual_count_exact_or_saturated_cases",
    )?;
    if exact_or_saturated_cases != residual_count_complete_cases {
        return Err(summary_invariant_error(
            scope,
            "residual_count_exact_cases",
            residual_count_complete_cases,
            exact_or_saturated_cases,
        ));
    }
    Ok(())
}

fn summary_invariant_error(
    scope: &'static str,
    field: &'static str,
    expected: u64,
    actual: u64,
) -> GeoPopulationError {
    GeoPopulationError::new(
        GeoPopulationErrorCode::InvalidInput,
        "Geo population summary denominators are internally inconsistent",
        [
            ("scope", scope.to_string()),
            ("field", field.to_string()),
            ("expected", expected.to_string()),
            ("actual", actual.to_string()),
        ],
    )
}

impl GeoPopulationTruthPlaneSummary {
    fn new(truth_plane: GeoTruthPlane) -> Self {
        Self {
            truth_plane,
            cases: 0,
            population_eligible_cases: 0,
            resolved_cases: 0,
            evidentially_supported_resolved_cases: 0,
            structurally_forced_resolved_cases: 0,
            resolved_with_reach_not_full_cases: 0,
            ambiguous_cases: 0,
            conflict_cases: 0,
            abstention_cases: 0,
            false_merge_cases: 0,
            candidate_reach_evaluated_cases: 0,
            candidate_reach_full_cases: 0,
            candidate_reach_partial_cases: 0,
            candidate_reach_none_cases: 0,
            truth_reach_by_grain: Vec::new(),
            solver_truth_scored_cases: 0,
            solver_artifact_cases: 0,
            empirical_falsification_eligible_cases: 0,
            solver_truth_exclusion_cases: 0,
            residual_count_complete_cases: 0,
            residual_count_exact_cases: 0,
            residual_count_saturated_cases: 0,
            residual_count_unavailable_cases: 0,
            component_budget_fallback_cases: 0,
            assignment_budget_exceeded_cases: 0,
            evidence_no_observation_cases: 0,
            evidence_diagnostic_only_cases: 0,
            evidence_soft_preference_only_cases: 0,
            evidence_soft_and_diagnostic_only_cases: 0,
            evidence_hard_constraint_cases: 0,
            truth_members: 0,
            truth_members_in_universe: 0,
            backbone_true_positive_members: 0,
            backbone_false_positive_members: 0,
        }
    }

    fn record(&mut self, case: &GeoPopulationCaseEvaluation) -> Result<(), GeoPopulationError> {
        checked_inc(&mut self.cases, "truth_plane.cases")?;
        checked_inc(
            &mut self.population_eligible_cases,
            "truth_plane.population_eligible_cases",
        )?;
        checked_inc(
            &mut self.candidate_reach_evaluated_cases,
            "truth_plane.candidate_reach_evaluated_cases",
        )?;
        if case.solver_digest.is_some() {
            checked_inc(
                &mut self.solver_artifact_cases,
                "truth_plane.solver_artifact_cases",
            )?;
        }
        match case.status {
            GeoPopulationCaseStatus::Resolved => {
                checked_inc(&mut self.resolved_cases, "truth_plane.resolved_cases")?;
                match case
                    .resolved_claim
                    .as_ref()
                    .map(|claim| claim.claim_class)
                    .ok_or_else(|| {
                        case_invariant_error(
                            case,
                            "resolved_claim",
                            "Geo population evaluation emitted a resolved case without a claim class",
                        )
                    })? {
                    GeoResolvedClaimClass::EvidentiallySupported => checked_inc(
                        &mut self.evidentially_supported_resolved_cases,
                        "truth_plane.evidentially_supported_resolved_cases",
                    )?,
                    GeoResolvedClaimClass::StructurallyForced => checked_inc(
                        &mut self.structurally_forced_resolved_cases,
                        "truth_plane.structurally_forced_resolved_cases",
                    )?,
                }
                if case.candidate_reach != GeoCandidateReachStatus::Full {
                    checked_inc(
                        &mut self.resolved_with_reach_not_full_cases,
                        "truth_plane.resolved_with_reach_not_full_cases",
                    )?;
                }
            }
            GeoPopulationCaseStatus::Ambiguous => {
                checked_inc(&mut self.ambiguous_cases, "truth_plane.ambiguous_cases")?;
            }
            GeoPopulationCaseStatus::Conflict => {
                checked_inc(&mut self.conflict_cases, "truth_plane.conflict_cases")?;
            }
            GeoPopulationCaseStatus::AssignmentBudgetExceeded => {
                checked_inc(
                    &mut self.assignment_budget_exceeded_cases,
                    "truth_plane.assignment_budget_exceeded_cases",
                )?;
            }
            GeoPopulationCaseStatus::ComponentBudgetFallback => {
                checked_inc(
                    &mut self.component_budget_fallback_cases,
                    "truth_plane.component_budget_fallback_cases",
                )?;
            }
        }
        if case.abstained {
            checked_inc(&mut self.abstention_cases, "truth_plane.abstention_cases")?;
        }
        if case.false_merge {
            checked_inc(&mut self.false_merge_cases, "truth_plane.false_merge_cases")?;
        }
        match case.candidate_reach {
            GeoCandidateReachStatus::Full => {
                checked_inc(
                    &mut self.candidate_reach_full_cases,
                    "truth_plane.candidate_reach_full_cases",
                )?;
            }
            GeoCandidateReachStatus::Partial => {
                checked_inc(
                    &mut self.candidate_reach_partial_cases,
                    "truth_plane.candidate_reach_partial_cases",
                )?;
            }
            GeoCandidateReachStatus::None => {
                checked_inc(
                    &mut self.candidate_reach_none_cases,
                    "truth_plane.candidate_reach_none_cases",
                )?;
            }
        }
        match case.evidence_coverage {
            GeoEvidenceCoverageStatus::NoObservations => {
                checked_inc(
                    &mut self.evidence_no_observation_cases,
                    "truth_plane.evidence_no_observation_cases",
                )?;
            }
            GeoEvidenceCoverageStatus::DiagnosticOnly => {
                checked_inc(
                    &mut self.evidence_diagnostic_only_cases,
                    "truth_plane.evidence_diagnostic_only_cases",
                )?;
            }
            GeoEvidenceCoverageStatus::SoftPreferenceOnly => {
                checked_inc(
                    &mut self.evidence_soft_preference_only_cases,
                    "truth_plane.evidence_soft_preference_only_cases",
                )?;
            }
            GeoEvidenceCoverageStatus::SoftAndDiagnosticOnly => {
                checked_inc(
                    &mut self.evidence_soft_and_diagnostic_only_cases,
                    "truth_plane.evidence_soft_and_diagnostic_only_cases",
                )?;
            }
            GeoEvidenceCoverageStatus::HardConstraintPresent => {
                checked_inc(
                    &mut self.evidence_hard_constraint_cases,
                    "truth_plane.evidence_hard_constraint_cases",
                )?;
            }
        }
        if case.solver_truth_scored {
            checked_inc(
                &mut self.solver_truth_scored_cases,
                "truth_plane.solver_truth_scored_cases",
            )?;
            checked_inc(
                &mut self.empirical_falsification_eligible_cases,
                "truth_plane.empirical_falsification_eligible_cases",
            )?;
        }
        if case.truth_model_in_residual == Some(false) {
            checked_inc(
                &mut self.solver_truth_exclusion_cases,
                "truth_plane.solver_truth_exclusion_cases",
            )?;
        }
        if case.residual_count_complete {
            checked_inc(
                &mut self.residual_count_complete_cases,
                "truth_plane.residual_count_complete_cases",
            )?;
            if is_residual_count_exact_case(case) {
                checked_inc(
                    &mut self.residual_count_exact_cases,
                    "truth_plane.residual_count_exact_cases",
                )?;
            }
        } else {
            checked_inc(
                &mut self.residual_count_unavailable_cases,
                "truth_plane.residual_count_unavailable_cases",
            )?;
        }
        if case.residual_count_saturated {
            checked_inc(
                &mut self.residual_count_saturated_cases,
                "truth_plane.residual_count_saturated_cases",
            )?;
        }
        checked_add(&mut self.truth_members, case.truth_members, "truth_members")?;
        checked_add(
            &mut self.truth_members_in_universe,
            case.truth_members_in_universe,
            "truth_members_in_universe",
        )?;
        record_truth_grain_reach(&mut self.truth_reach_by_grain, &case.truth_reach_by_grain)?;
        checked_add(
            &mut self.backbone_true_positive_members,
            case.backbone_true_positive_members,
            "backbone_true_positive_members",
        )?;
        checked_add(
            &mut self.backbone_false_positive_members,
            case.backbone_false_positive_members,
            "backbone_false_positive_members",
        )
    }
}

fn record_truth_grain_reach(
    summaries: &mut Vec<GeoPopulationTruthGrainSummary>,
    reaches: &[GeoTruthReachByGrain],
) -> Result<(), GeoPopulationError> {
    for reach in reaches {
        let index = match summaries.binary_search_by_key(&reach.grain, |summary| summary.grain) {
            Ok(index) => index,
            Err(index) => {
                summaries.insert(index, GeoPopulationTruthGrainSummary::new(reach.grain));
                index
            }
        };
        summaries[index].record(reach)?;
    }
    Ok(())
}

impl GeoPopulationTruthGrainSummary {
    fn new(grain: GeoTruthRepresentationGrain) -> Self {
        Self {
            grain,
            cases: 0,
            truth_members: 0,
            truth_members_in_universe: 0,
            candidate_reach_full_cases: 0,
            candidate_reach_partial_cases: 0,
            candidate_reach_none_cases: 0,
        }
    }

    fn record(&mut self, reach: &GeoTruthReachByGrain) -> Result<(), GeoPopulationError> {
        checked_inc(&mut self.cases, "truth_reach_by_grain.cases")?;
        checked_add(
            &mut self.truth_members,
            reach.truth_members,
            "truth_reach_by_grain.truth_members",
        )?;
        checked_add(
            &mut self.truth_members_in_universe,
            reach.truth_members_in_universe,
            "truth_reach_by_grain.truth_members_in_universe",
        )?;
        match reach.candidate_reach {
            GeoCandidateReachStatus::Full => checked_inc(
                &mut self.candidate_reach_full_cases,
                "truth_reach_by_grain.candidate_reach_full_cases",
            )?,
            GeoCandidateReachStatus::Partial => checked_inc(
                &mut self.candidate_reach_partial_cases,
                "truth_reach_by_grain.candidate_reach_partial_cases",
            )?,
            GeoCandidateReachStatus::None => checked_inc(
                &mut self.candidate_reach_none_cases,
                "truth_reach_by_grain.candidate_reach_none_cases",
            )?,
        }
        Ok(())
    }

    fn record_summary(
        &mut self,
        summary: &GeoPopulationTruthGrainSummary,
    ) -> Result<(), GeoPopulationError> {
        checked_add(&mut self.cases, summary.cases, "truth_reach_by_grain.cases")?;
        checked_add(
            &mut self.truth_members,
            summary.truth_members,
            "truth_reach_by_grain.truth_members",
        )?;
        checked_add(
            &mut self.truth_members_in_universe,
            summary.truth_members_in_universe,
            "truth_reach_by_grain.truth_members_in_universe",
        )?;
        checked_add(
            &mut self.candidate_reach_full_cases,
            summary.candidate_reach_full_cases,
            "truth_reach_by_grain.candidate_reach_full_cases",
        )?;
        checked_add(
            &mut self.candidate_reach_partial_cases,
            summary.candidate_reach_partial_cases,
            "truth_reach_by_grain.candidate_reach_partial_cases",
        )?;
        checked_add(
            &mut self.candidate_reach_none_cases,
            summary.candidate_reach_none_cases,
            "truth_reach_by_grain.candidate_reach_none_cases",
        )
    }
}

impl GeoCandidateTruthPlaneSummary {
    fn new(truth_plane: GeoTruthPlane) -> Self {
        Self {
            truth_plane,
            logical_subjects: 0,
            release_validated_logical_subjects: 0,
            frozen_e4_h7_genuine_multi_parcel_subjects: 0,
            release_rows: 0,
            candidate_reach_full_release_rows: 0,
            candidate_reach_partial_release_rows: 0,
            candidate_reach_none_release_rows: 0,
            solver_truth_scored_release_rows: 0,
            rho_falsification_release_rows: 0,
        }
    }

    fn record(&mut self, row: &GeoCandidateTruthCaseEvaluation) -> Result<(), GeoPopulationError> {
        checked_inc(&mut self.release_rows, "truth_plane.release_rows")?;
        match row.candidate_reach {
            GeoCandidateReachStatus::Full => {
                checked_inc(
                    &mut self.candidate_reach_full_release_rows,
                    "truth_plane.candidate_reach_full_release_rows",
                )?;
            }
            GeoCandidateReachStatus::Partial => {
                checked_inc(
                    &mut self.candidate_reach_partial_release_rows,
                    "truth_plane.candidate_reach_partial_release_rows",
                )?;
            }
            GeoCandidateReachStatus::None => {
                checked_inc(
                    &mut self.candidate_reach_none_release_rows,
                    "truth_plane.candidate_reach_none_release_rows",
                )?;
            }
        }
        if row.solver_truth_scored {
            checked_inc(
                &mut self.solver_truth_scored_release_rows,
                "truth_plane.solver_truth_scored_release_rows",
            )?;
        }
        if row.rho_falsification {
            checked_inc(
                &mut self.rho_falsification_release_rows,
                "truth_plane.rho_falsification_release_rows",
            )?;
        }
        Ok(())
    }
}

fn checked_len(value: usize, field: &str) -> Result<u64, GeoPopulationError> {
    u64::try_from(value).map_err(|_| GeoPopulationError::overflow(field))
}

fn checked_member_count(
    parcel_count: usize,
    building_count: usize,
    field: &str,
) -> Result<u64, GeoPopulationError> {
    let mut total = checked_len(parcel_count, field)?;
    checked_add(&mut total, checked_len(building_count, field)?, field)?;
    Ok(total)
}

fn checked_add(target: &mut u64, value: u64, field: &str) -> Result<(), GeoPopulationError> {
    *target = target
        .checked_add(value)
        .ok_or_else(|| GeoPopulationError::overflow(field))?;
    Ok(())
}

fn checked_inc(target: &mut u64, field: &str) -> Result<(), GeoPopulationError> {
    checked_add(target, 1, field)
}

fn sum_u64(values: impl IntoIterator<Item = u64>, field: &str) -> Result<u64, GeoPopulationError> {
    let mut total = 0;
    for value in values {
        checked_add(&mut total, value, field)?;
    }
    Ok(total)
}

fn empty_backbone() -> GeoCompositionBackbone {
    GeoCompositionBackbone {
        parcels: Vec::new(),
        buildings: Vec::new(),
    }
}

fn map_evidence_error(error: GeoEvidenceError) -> GeoPopulationError {
    let mut detail = error.detail;
    detail.insert("evidence_code".to_string(), format!("{:?}", error.code));
    GeoPopulationError {
        code: GeoPopulationErrorCode::Evidence,
        message: error.message,
        detail,
    }
}

fn map_composition_error(error: GeoCompositionError) -> GeoPopulationError {
    let mut detail = error.detail;
    detail.insert("composition_code".to_string(), format!("{:?}", error.code));
    GeoPopulationError {
        code: GeoPopulationErrorCode::Composition,
        message: error.message,
        detail,
    }
}
