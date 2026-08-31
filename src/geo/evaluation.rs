#![forbid(unsafe_code)]

//! Deterministic evaluation of labeled Geo composition cases.
//!
//! Truth labels are held outside evidence compilation and solving. They are used
//! only for post-solve scoring and validation, so ground truth cannot silently
//! become a constraint or preference.

use super::{
    composition::{
        GeoCompositionBackbone, GeoCompositionError, GeoCompositionErrorCode, GeoCompositionModel,
        GeoCompositionStatus, canonical_composition_bytes, model_satisfies_request,
        solve_composition,
    },
    evidence::{
        GeoEvidenceCompilationRequest, GeoEvidenceDisposition, GeoEvidenceError,
        canonical_evidence_compilation_bytes, compile_evidence,
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fmt};

pub const CANON_GEO_POPULATION_REQUEST_VERSION: &str = "canon_geo_population_request.v0";
pub const CANON_GEO_POPULATION_EVALUATION_VERSION: &str = "canon_geo_population_evaluation.v0";

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
    for case in cases {
        // Deliberately compile and solve before reading `case.truth`.
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

        let evaluation = match solved {
            Err(error) if error.code == GeoCompositionErrorCode::BudgetExceeded => {
                GeoPopulationCaseEvaluation {
                    case_id: case.id,
                    truth_plane: case.truth_plane,
                    status: GeoPopulationCaseStatus::AssignmentBudgetExceeded,
                    compilation_digest,
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
                let residual_count_complete = artifact.summary.residual_model_count_complete;
                let false_merge = scored_false_merge(status, truth_model_in_residual);
                GeoPopulationCaseEvaluation {
                    case_id: case.id,
                    truth_plane: case.truth_plane,
                    status,
                    residual_count_saturated: artifact.summary.residual_model_count_saturated,
                    compilation_digest,
                    solver_digest: Some(solver_digest),
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
                    hard_forced: artifact.hard_forced,
                    backbone_complete: artifact.backbone_complete,
                    backbone_true_positive_members: backbone_true,
                    backbone_false_positive_members: backbone_false,
                    abstained: is_abstention_status(status),
                    false_merge,
                }
            }
        };
        validate_case_evaluation(&evaluation)?;
        evaluations.push(evaluation);
    }

    let summary = summarize(&evaluations)?;
    Ok(GeoPopulationEvaluationArtifact {
        version: CANON_GEO_POPULATION_EVALUATION_VERSION.to_string(),
        request_version: request.version.clone(),
        summary,
        cases: evaluations,
    })
}

pub fn canonical_population_evaluation_bytes(
    artifact: &GeoPopulationEvaluationArtifact,
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

fn scored_false_merge(
    status: GeoPopulationCaseStatus,
    truth_model_in_residual: Option<bool>,
) -> bool {
    status == GeoPopulationCaseStatus::Resolved && truth_model_in_residual == Some(false)
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
    match case.status {
        GeoPopulationCaseStatus::AssignmentBudgetExceeded => {
            if case.solver_digest.is_some()
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
            if case.solver_digest.is_none() {
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

fn validate_summary(summary: &GeoPopulationSummary) -> Result<(), GeoPopulationError> {
    validate_summary_denominators(
        "summary",
        summary.cases,
        summary.population_eligible_cases,
        summary.candidate_reach_evaluated_cases,
        summary.candidate_reach_full_cases,
        summary.candidate_reach_partial_cases,
        summary.candidate_reach_none_cases,
        summary.solver_truth_scored_cases,
        summary.empirical_falsification_eligible_cases,
        summary.solver_truth_exclusion_cases,
    )?;
    validate_truth_plane_sums(summary)?;
    for plane in &summary.truth_planes {
        validate_summary_denominators(
            "truth_plane",
            plane.cases,
            plane.population_eligible_cases,
            plane.candidate_reach_evaluated_cases,
            plane.candidate_reach_full_cases,
            plane.candidate_reach_partial_cases,
            plane.candidate_reach_none_cases,
            plane.solver_truth_scored_cases,
            plane.empirical_falsification_eligible_cases,
            plane.solver_truth_exclusion_cases,
        )?;
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
    candidate_reach_evaluated_cases: u64,
    candidate_reach_full_cases: u64,
    candidate_reach_partial_cases: u64,
    candidate_reach_none_cases: u64,
    solver_truth_scored_cases: u64,
    empirical_falsification_eligible_cases: u64,
    solver_truth_exclusion_cases: u64,
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
