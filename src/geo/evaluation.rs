#![forbid(unsafe_code)]

//! Deterministic evaluation of labeled Geo composition cases.
//!
//! Truth labels are held outside evidence compilation and solving. They are used
//! only for post-solve scoring and validation, so ground truth cannot silently
//! become a constraint or preference.

use super::{
    composition::{
        GeoCompositionArtifact, GeoCompositionBackbone, GeoCompositionError,
        GeoCompositionErrorCode, GeoCompositionModel, GeoCompositionRequest, GeoCompositionStatus,
        GeoResolvedClaim, GeoResolvedClaimClass, canonical_composition_bytes,
        canonicalize_composition_request, model_satisfies_request, solve_composition,
    },
    evidence::{
        GeoEvidenceCompilationArtifact, GeoEvidenceCompilationRequest, GeoEvidenceDisposition,
        GeoEvidenceError, canonical_evidence_compilation_bytes, compile_evidence,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_POPULATION_REQUEST_VERSION: &str = "canon_geo_population_request.v0";
pub const CANON_GEO_POPULATION_EVALUATION_VERSION: &str = "canon_geo_population_evaluation.v0";
pub const CANON_GEO_FROZEN_E4_H7_CANDIDATE_TRUTH_HANDOFF_REQUEST_VERSION: &str =
    "canon_geo_frozen_e4_h7_candidate_truth_handoff_request.v0";
pub const CANON_GEO_FROZEN_E4_H7_CANDIDATE_TRUTH_EVALUATION_VERSION: &str =
    "canon_geo_frozen_e4_h7_candidate_truth_evaluation.v0";
pub const CANON_GEO_FROZEN_E4_H7_GATE_ID: &str =
    "canon_geo_frozen_e4_h7_release_validated_multi_parcel_subject_gate.v0";
pub const CANON_GEO_FROZEN_E4_H7_REQUIRED_SUBJECTS: u64 = 79;
pub const CANON_GEO_FROZEN_E4_H7_RELEASE_26V1: &str = "26v1";
pub const CANON_GEO_FROZEN_E4_H7_RELEASE_26V2: &str = "26v2";

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    pub summary: GeoPopulationSummary,
    pub cases: Vec<GeoPopulationCaseEvaluation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoPopulationCaseArtifacts {
    pub case_id: String,
    pub truth_plane: GeoTruthPlane,
    pub evidence: GeoEvidenceCompilationArtifact,
    pub solve: Option<GeoCompositionArtifact>,
    pub compilation_digest: String,
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

pub fn evaluate_population_with_artifacts(
    request: &GeoPopulationEvaluationRequest,
) -> Result<GeoPopulationEvaluationWithArtifacts, GeoPopulationError> {
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

    let mut evaluations = Vec::with_capacity(cases.len());
    let mut case_artifacts = Vec::with_capacity(cases.len());
    for case in cases {
        // Deliberately compile and solve before reading `case.truth`.
        let case_id = case.id.clone();
        let truth_plane = case.truth_plane;
        let compilation = compile_evidence(&case.evidence).map_err(map_evidence_error)?;
        let compilation_digest = blake3::hash(
            &canonical_evidence_compilation_bytes(&compilation).map_err(|error| {
                GeoPopulationError::new(
                    GeoPopulationErrorCode::Composition,
                    "Geo evidence compilation could not be serialized",
                    [("error", error.to_string())],
                )
            })?,
        )
        .to_hex()
        .to_string();
        let solved = solve_composition(&compilation.composition_request);

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
        let evidence_metrics = evidence_metrics(&compilation.admissions)?;

        let (evaluation, solve_artifact) = match solved {
            Err(error) if error.code == GeoCompositionErrorCode::BudgetExceeded => (
                GeoPopulationCaseEvaluation {
                    case_id: case.id,
                    truth_plane: case.truth_plane,
                    status: GeoPopulationCaseStatus::AssignmentBudgetExceeded,
                    resolved_claim: None,
                    compilation_digest: compilation_digest.clone(),
                    solver_digest: None,
                    candidate_members,
                    truth_members,
                    truth_members_in_universe,
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
                    compilation_digest: compilation_digest.clone(),
                    solver_digest: Some(solver_digest.clone()),
                    candidate_members,
                    truth_members,
                    truth_members_in_universe,
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
                (evaluation, Some(artifact))
            }
        };
        validate_case_evaluation(&evaluation)?;
        case_artifacts.push(GeoPopulationCaseArtifacts {
            case_id,
            truth_plane,
            evidence: compilation,
            solve: solve_artifact,
            compilation_digest,
            solver_digest: evaluation.solver_digest.clone(),
        });
        evaluations.push(evaluation);
    }

    let summary = summarize(&evaluations)?;
    Ok(GeoPopulationEvaluationWithArtifacts {
        evaluation: GeoPopulationEvaluationArtifact {
            version: CANON_GEO_POPULATION_EVALUATION_VERSION.to_string(),
            request_version: request.version.clone(),
            summary,
            cases: evaluations,
        },
        case_artifacts,
    })
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

pub fn canonical_candidate_truth_evaluation_bytes(
    artifact: &GeoCandidateTruthEvaluationArtifact,
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
