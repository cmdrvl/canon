#![forbid(unsafe_code)]

//! Deterministic evaluation of labeled Geo composition cases.
//!
//! Labels are held outside evidence compilation. The compiler and solver run
//! before labels are inspected, so ground truth cannot silently become a
//! constraint or preference.

use super::{
    composition::{
        GeoCompositionBackbone, GeoCompositionError, GeoCompositionErrorCode, GeoCompositionModel,
        GeoCompositionStatus, canonical_composition_bytes, model_satisfies_request,
        solve_composition,
    },
    evidence::{
        GeoEvidenceCompilationRequest, GeoEvidenceError, canonical_evidence_compilation_bytes,
        compile_evidence,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPopulationCaseEvaluation {
    pub case_id: String,
    pub status: GeoPopulationCaseStatus,
    pub compilation_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solver_digest: Option<String>,
    pub candidate_members: u64,
    pub truth_members: u64,
    pub truth_members_in_universe: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub residual_model_count: Option<u64>,
    /// Mirrors the solver's `residual_model_count_saturated`: a saturated
    /// residual is a declared lower bound, never a point estimate. Saturation
    /// of a different summary counter does not taint this claim.
    #[serde(default)]
    pub residual_count_saturated: bool,
    pub full_truth_recall: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truth_model_in_residual: Option<bool>,
    pub hard_forced: GeoCompositionBackbone,
    /// Whether `hard_forced` is the solver's complete hard backbone. A budget
    /// handoff must never be read as evidence that no member was forced.
    #[serde(default)]
    pub backbone_complete: bool,
    pub backbone_true_positive_members: u64,
    pub backbone_false_positive_members: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPopulationSummary {
    pub cases: u64,
    pub resolved_cases: u64,
    pub ambiguous_cases: u64,
    pub conflict_cases: u64,
    pub assignment_budget_exceeded_cases: u64,
    pub component_budget_fallback_cases: u64,
    pub abstention_cases: u64,
    pub false_merge_cases: u64,
    pub full_truth_recall_cases: u64,
    /// Cases where at least one labeled truth member was absent from the
    /// candidate universe. These are candidate-generation failures, not
    /// solver false negatives.
    pub candidate_recall_failure_cases: u64,
    /// Cases whose full truth model was representable and for which solver
    /// residual membership was therefore actually scored.
    pub solver_truth_scored_cases: u64,
    /// Scored cases where admitted hard evidence excluded the labeled truth
    /// model. This is the population falsification count for the active rho
    /// contracts; it is distinct from a wrong singleton/false merge.
    pub solver_truth_exclusion_cases: u64,
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
        let candidate_members = checked_len(
            universe.parcels.len() + universe.buildings.len(),
            "candidate_members",
        )?;
        let truth_members = checked_len(
            case.truth.parcels.len() + case.truth.buildings.len(),
            "truth_members",
        )?;
        let truth_members_in_universe = count_truth_in_universe(&case.truth, universe)?;
        let full_truth_recall = truth_members == truth_members_in_universe;

        let evaluation = match solved {
            Err(error) if error.code == GeoCompositionErrorCode::BudgetExceeded => {
                GeoPopulationCaseEvaluation {
                    case_id: case.id,
                    status: GeoPopulationCaseStatus::AssignmentBudgetExceeded,
                    compilation_digest,
                    solver_digest: None,
                    candidate_members,
                    truth_members,
                    truth_members_in_universe,
                    full_truth_recall,
                    residual_model_count: None,
                    residual_count_saturated: false,
                    truth_model_in_residual: None,
                    hard_forced: empty_backbone(),
                    backbone_complete: false,
                    backbone_true_positive_members: 0,
                    backbone_false_positive_members: 0,
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
                let (backbone_true, backbone_false) = if artifact.backbone_complete {
                    score_backbone(&artifact.hard_forced, &case.truth)?
                } else {
                    (0, 0)
                };
                let truth_model_in_residual = if full_truth_recall {
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
                GeoPopulationCaseEvaluation {
                    case_id: case.id,
                    status: match artifact.status {
                        GeoCompositionStatus::Resolved => GeoPopulationCaseStatus::Resolved,
                        GeoCompositionStatus::Ambiguous => GeoPopulationCaseStatus::Ambiguous,
                        GeoCompositionStatus::Conflict => GeoPopulationCaseStatus::Conflict,
                        GeoCompositionStatus::BudgetFallback => {
                            GeoPopulationCaseStatus::ComponentBudgetFallback
                        }
                    },
                    residual_count_saturated: artifact.summary.residual_model_count_saturated,
                    compilation_digest,
                    solver_digest: Some(solver_digest),
                    candidate_members,
                    truth_members,
                    truth_members_in_universe,
                    full_truth_recall,
                    residual_model_count: artifact
                        .summary
                        .residual_model_count_complete
                        .then_some(artifact.summary.residual_model_count),
                    truth_model_in_residual,
                    hard_forced: artifact.hard_forced,
                    backbone_complete: artifact.backbone_complete,
                    backbone_true_positive_members: backbone_true,
                    backbone_false_positive_members: backbone_false,
                }
            }
        };
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
    checked_len(parcels + buildings, "truth_members_in_universe")
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
    let total = backbone.parcels.len() + backbone.buildings.len();
    let true_count = true_parcels + true_buildings;
    Ok((
        checked_len(true_count, "backbone_true_positive_members")?,
        checked_len(total - true_count, "backbone_false_positive_members")?,
    ))
}

fn summarize(
    cases: &[GeoPopulationCaseEvaluation],
) -> Result<GeoPopulationSummary, GeoPopulationError> {
    let mut summary = GeoPopulationSummary {
        cases: checked_len(cases.len(), "cases")?,
        resolved_cases: 0,
        ambiguous_cases: 0,
        conflict_cases: 0,
        assignment_budget_exceeded_cases: 0,
        component_budget_fallback_cases: 0,
        abstention_cases: 0,
        false_merge_cases: 0,
        full_truth_recall_cases: 0,
        candidate_recall_failure_cases: 0,
        solver_truth_scored_cases: 0,
        solver_truth_exclusion_cases: 0,
        truth_members: 0,
        truth_members_in_universe: 0,
        backbone_true_positive_members: 0,
        backbone_false_positive_members: 0,
    };
    for case in cases {
        match case.status {
            GeoPopulationCaseStatus::Resolved => summary.resolved_cases += 1,
            GeoPopulationCaseStatus::Ambiguous => {
                summary.ambiguous_cases += 1;
                summary.abstention_cases += 1;
            }
            GeoPopulationCaseStatus::Conflict => {
                summary.conflict_cases += 1;
                summary.abstention_cases += 1;
            }
            GeoPopulationCaseStatus::AssignmentBudgetExceeded => {
                summary.assignment_budget_exceeded_cases += 1;
                summary.abstention_cases += 1;
            }
            GeoPopulationCaseStatus::ComponentBudgetFallback => {
                summary.component_budget_fallback_cases += 1;
                summary.abstention_cases += 1;
            }
        }
        if case.full_truth_recall {
            summary.full_truth_recall_cases += 1;
        } else {
            summary.candidate_recall_failure_cases += 1;
        }
        if case.truth_model_in_residual.is_some() {
            summary.solver_truth_scored_cases += 1;
        }
        if case.truth_model_in_residual == Some(false) {
            summary.solver_truth_exclusion_cases += 1;
        }
        let resolved_wrong = case.status == GeoPopulationCaseStatus::Resolved
            && case.truth_model_in_residual == Some(false);
        if resolved_wrong || case.backbone_false_positive_members > 0 {
            summary.false_merge_cases += 1;
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
    }
    Ok(summary)
}

fn checked_len(value: usize, field: &str) -> Result<u64, GeoPopulationError> {
    u64::try_from(value).map_err(|_| GeoPopulationError::overflow(field))
}

fn checked_add(target: &mut u64, value: u64, field: &str) -> Result<(), GeoPopulationError> {
    *target = target
        .checked_add(value)
        .ok_or_else(|| GeoPopulationError::overflow(field))?;
    Ok(())
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
