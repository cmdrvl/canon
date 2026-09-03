#![forbid(unsafe_code)]

//! Explanation artifacts for empty and residual Geo composition model sets.
//!
//! This module keeps the composition kernel as the exact backend. It narrows
//! requests only by deleting declared hard constraints or by appending declared
//! prospective outcomes, then calls the existing solver for each check.

use super::{
    GeoCompositionError, GeoCompositionRequest, GeoCompositionStatus,
    GeoEvidenceCompilationArtifact, GeoEvidenceRecordRef, GeoHardConstraint, GeoHardConstraintKind,
    canonical_evidence_compilation_bytes, canonicalize_composition_request, solve_composition,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_EXPLANATION_VERSION: &str = "canon_geo_explanation.v0";
pub const CANON_GEO_SEPARATION_REQUEST_VERSION: &str = "canon_geo_separation_request.v0";
pub const CANON_GEO_SEPARATION_VERSION: &str = "canon_geo_separation.v0";

const DEFAULT_MAX_CORE_SOLVES: u64 = 64;
const DEFAULT_MAX_CORES: u64 = 8;
const DEFAULT_MAX_HITTING_SETS: u64 = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoReliabilityOrder {
    pub contract_ids_most_reliable_first: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoExplanationBudget {
    pub max_core_solves: u64,
    pub max_cores: u64,
    pub max_hitting_sets: u64,
}

impl Default for GeoExplanationBudget {
    fn default() -> Self {
        Self {
            max_core_solves: DEFAULT_MAX_CORE_SOLVES,
            max_cores: DEFAULT_MAX_CORES,
            max_hitting_sets: DEFAULT_MAX_HITTING_SETS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoExplanationSubjectRef {
    pub accession: String,
    pub loan_id: String,
    pub subject_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCoreDeletionCheck {
    pub constraint_id: String,
    pub status_after_deletion: GeoCompositionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoMinimalCore {
    pub constraint_ids: Vec<String>,
    pub observation_ids: Vec<String>,
    pub source_record_ids: Vec<String>,
    pub rho_contract_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_record_refs: Vec<GeoEvidenceRecordRef>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub admitted_values: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deletion_checks: Vec<GeoCoreDeletionCheck>,
    pub minimal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCorrectionSet {
    pub observation_ids: Vec<String>,
    pub source_record_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rho_contract_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_record_refs: Vec<GeoEvidenceRecordRef>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub admitted_values: BTreeMap<String, String>,
    pub minimal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoExplanationArtifact {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_ref: Option<GeoExplanationSubjectRef>,
    pub request_blake3: String,
    pub evidence_blake3: String,
    pub cores: Vec<GeoMinimalCore>,
    pub cores_complete: bool,
    pub correction_sets: Vec<GeoCorrectionSet>,
    pub explanation_complete: bool,
    pub counters: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoProspectiveOutcome {
    pub outcome_id: String,
    pub induced: Vec<GeoHardConstraintKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoProspectiveObservation {
    pub id: String,
    pub contract_id: String,
    pub cost_units: u64,
    pub outcomes: Vec<GeoProspectiveOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoSeparationRequest {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_ref: Option<GeoExplanationSubjectRef>,
    pub request: GeoCompositionRequest,
    pub prospective: Vec<GeoProspectiveObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoOutcomeSeparation {
    pub outcome_id: String,
    pub residual_model_count: u64,
    pub count_exact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoObservationSeparation {
    pub observation_id: String,
    pub per_outcome: Vec<GeoOutcomeSeparation>,
    pub worst_case_remaining: u64,
    pub redundant: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoSeparationArtifact {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_ref: Option<GeoExplanationSubjectRef>,
    pub request_blake3: String,
    pub baseline_model_count: u64,
    pub per_observation: Vec<GeoObservationSeparation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoExplanationErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
    CoreNotMinimal,
    CoreEnumerationCeiling,
    ExplanationNotConflict,
    SeparationResidualInexact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoExplanationError {
    pub code: GeoExplanationErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoExplanationError {
    fn new(
        code: GeoExplanationErrorCode,
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

    fn invalid(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(GeoExplanationErrorCode::InvalidInput, message, detail)
    }

    fn overflow(context: &str) -> Self {
        Self::new(
            GeoExplanationErrorCode::ArithmeticOverflow,
            "Geo explanation arithmetic overflowed",
            [("context", context)],
        )
    }
}

impl fmt::Display for GeoExplanationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoExplanationError {}

impl From<GeoCompositionError> for GeoExplanationError {
    fn from(error: GeoCompositionError) -> Self {
        let mut detail = error.detail;
        detail.insert("composition_code".to_string(), format!("{:?}", error.code));
        Self {
            code: GeoExplanationErrorCode::InvalidInput,
            message: error.message,
            detail,
        }
    }
}

pub fn reliability_order_from_evidence(
    evidence: &GeoEvidenceCompilationArtifact,
) -> GeoReliabilityOrder {
    let mut ids = evidence
        .admissions
        .iter()
        .filter(|admission| !admission.generated_ids.is_empty())
        .map(|admission| admission.contract.id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    ids.sort();
    GeoReliabilityOrder {
        contract_ids_most_reliable_first: ids,
    }
}

pub fn minimal_core(
    request: &GeoCompositionRequest,
    evidence: &GeoEvidenceCompilationArtifact,
    order: &GeoReliabilityOrder,
    budget: &GeoExplanationBudget,
) -> Result<GeoExplanationArtifact, GeoExplanationError> {
    validate_budget(budget)?;
    let request = canonical_request_matching_evidence(request, evidence)?;
    let evidence_index = EvidenceIndex::from_evidence(evidence)?;
    let order_rank = validate_reliability_order(order, &evidence_index)?;
    let mut counter = SolveCounter::new(budget.max_core_solves);
    let baseline = solve_composition(&request)?;
    counter.record()?;
    if baseline.status != GeoCompositionStatus::Conflict {
        return Err(not_conflict_error(baseline.status));
    }

    let all_constraint_ids = request
        .hard_constraints
        .iter()
        .map(|constraint| constraint.id.clone())
        .collect::<Vec<_>>();
    if all_constraint_ids.is_empty() {
        return Err(GeoExplanationError::invalid(
            "Geo explanation conflicts require at least one hard constraint",
            [("field", "hard_constraints")],
        ));
    }
    let (core_ids, deletion_checks) = minimize_conflict_subset(
        &request,
        &all_constraint_ids,
        &evidence_index,
        &order_rank,
        &mut counter,
    )?;
    let core = core_from_ids(&request, &core_ids, &evidence_index, deletion_checks, true)?;
    let mut counters = BTreeMap::new();
    counters.insert("core_solves".to_string(), counter.count);
    counters.insert("cores_enumerated".to_string(), 1);
    counters.insert("hitting_sets".to_string(), 0);
    counters.insert(
        "hard_constraint_count".to_string(),
        request.hard_constraints.len() as u64,
    );
    counters.insert(
        "source_record_count".to_string(),
        evidence_index.source_record_count()?,
    );

    let artifact = GeoExplanationArtifact {
        version: CANON_GEO_EXPLANATION_VERSION.to_string(),
        subject_ref: None,
        request_blake3: request_blake3(&request)?,
        evidence_blake3: evidence_blake3(evidence)?,
        cores: vec![core],
        cores_complete: false,
        correction_sets: Vec::new(),
        explanation_complete: false,
        counters,
    };
    validate_explanation_artifact(&artifact)?;
    Ok(artifact)
}

pub fn correction_sets(
    artifact: &mut GeoExplanationArtifact,
    request: &GeoCompositionRequest,
    evidence: &GeoEvidenceCompilationArtifact,
    budget: &GeoExplanationBudget,
) -> Result<(), GeoExplanationError> {
    validate_budget(budget)?;
    validate_explanation_artifact(artifact)?;
    let request = canonical_request_matching_evidence(request, evidence)?;
    if artifact.request_blake3 != request_blake3(&request)?
        || artifact.evidence_blake3 != evidence_blake3(evidence)?
    {
        return Err(GeoExplanationError::invalid(
            "Geo explanation artifact does not match the supplied request and evidence",
            [("field", "request_blake3")],
        ));
    }

    let evidence_index = EvidenceIndex::from_evidence(evidence)?;
    let order = reliability_order_from_evidence(evidence);
    let order_rank = validate_reliability_order(&order, &evidence_index)?;
    let mut counter = SolveCounter::new(budget.max_core_solves);
    counter.count = artifact.counters.get("core_solves").copied().unwrap_or(0);
    let mut ceiling = artifact.cores.len() as u64 >= budget.max_cores;

    if !ceiling {
        ceiling = !enumerate_minimal_cores(
            artifact,
            &request,
            &evidence_index,
            &order_rank,
            &mut counter,
            budget,
        )?;
    }

    if ceiling {
        mark_explanation_incomplete(artifact);
        artifact
            .counters
            .insert("core_enumeration_ceiling".to_string(), 1);
        artifact
            .counters
            .insert("core_solves".to_string(), counter.count);
        validate_explanation_artifact(artifact)?;
        return Ok(());
    }

    artifact.cores_complete = true;
    let hitting_sets = enumerate_hitting_sets(&artifact.cores, &evidence_index, budget)?;
    artifact.correction_sets = hitting_sets;
    artifact.explanation_complete = true;
    artifact
        .counters
        .insert("core_solves".to_string(), counter.count);
    artifact
        .counters
        .insert("cores_enumerated".to_string(), artifact.cores.len() as u64);
    artifact.counters.insert(
        "hitting_sets".to_string(),
        artifact.correction_sets.len() as u64,
    );
    if artifact.correction_sets.len() as u64 >= budget.max_hitting_sets
        && hitting_set_search_space(&artifact.cores)? as u64 > budget.max_hitting_sets
    {
        mark_explanation_incomplete(artifact);
        artifact
            .counters
            .insert("core_enumeration_ceiling".to_string(), 1);
    }
    validate_explanation_artifact(artifact)
}

pub fn separate(
    request: &GeoSeparationRequest,
    budget: &GeoExplanationBudget,
) -> Result<GeoSeparationArtifact, GeoExplanationError> {
    validate_budget(budget)?;
    validate_separation_request(request)?;
    let canonical = canonicalize_composition_request(&request.request)?;
    let baseline = solve_composition(&canonical)?;
    let baseline_exact = exact_count_available(&baseline);
    let mut per_observation = Vec::new();
    for observation in &request.prospective {
        let mut per_outcome = Vec::new();
        let mut worst_case_remaining = 0_u64;
        for outcome in &observation.outcomes {
            let outcome_request = request_with_outcome(&canonical, &observation.id, outcome)?;
            let solved = solve_composition(&outcome_request)?;
            worst_case_remaining =
                std::cmp::max(worst_case_remaining, solved.summary.residual_model_count);
            let count_exact = baseline_exact && exact_count_available(&solved);
            per_outcome.push(GeoOutcomeSeparation {
                outcome_id: outcome.outcome_id.clone(),
                residual_model_count: solved.summary.residual_model_count,
                count_exact,
            });
        }
        per_outcome.sort_by(|left, right| left.outcome_id.cmp(&right.outcome_id));
        let all_exact = per_outcome.iter().all(|outcome| outcome.count_exact);
        let redundant = all_exact
            && per_outcome.iter().all(|outcome| {
                outcome.residual_model_count == baseline.summary.residual_model_count
            });
        per_observation.push(GeoObservationSeparation {
            observation_id: observation.id.clone(),
            per_outcome,
            worst_case_remaining,
            redundant,
        });
    }
    per_observation.sort_by(|left, right| left.observation_id.cmp(&right.observation_id));
    let artifact = GeoSeparationArtifact {
        version: CANON_GEO_SEPARATION_VERSION.to_string(),
        subject_ref: request.subject_ref.clone(),
        request_blake3: request_blake3(&canonical)?,
        baseline_model_count: baseline.summary.residual_model_count,
        per_observation,
    };
    validate_separation_artifact(&artifact)?;
    Ok(artifact)
}

pub fn verify_minimal_core_members(
    request: &GeoCompositionRequest,
    evidence: &GeoEvidenceCompilationArtifact,
    order: &GeoReliabilityOrder,
    constraint_ids: &[String],
) -> Result<Vec<GeoCoreDeletionCheck>, GeoExplanationError> {
    let request = canonical_request_matching_evidence(request, evidence)?;
    let evidence_index = EvidenceIndex::from_evidence(evidence)?;
    let order_rank = validate_reliability_order(order, &evidence_index)?;
    let mut counter = SolveCounter::new(u64::MAX);
    verify_deletion_minimality(
        &request,
        constraint_ids,
        &evidence_index,
        &order_rank,
        &mut counter,
    )
}

pub fn validate_explanation_artifact(
    artifact: &GeoExplanationArtifact,
) -> Result<(), GeoExplanationError> {
    if artifact.version != CANON_GEO_EXPLANATION_VERSION {
        return Err(GeoExplanationError::new(
            GeoExplanationErrorCode::UnsupportedVersion,
            "Unsupported Geo explanation artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_EXPLANATION_VERSION),
            ],
        ));
    }
    validate_blake3_ref("request_blake3", &artifact.request_blake3)?;
    validate_blake3_ref("evidence_blake3", &artifact.evidence_blake3)?;
    if let Some(subject) = &artifact.subject_ref {
        validate_subject_ref(subject)?;
    }
    if artifact.cores.is_empty() {
        return Err(GeoExplanationError::invalid(
            "Geo explanation artifacts require at least one core",
            [("field", "cores")],
        ));
    }
    let mut previous_core: Option<&Vec<String>> = None;
    for core in &artifact.cores {
        validate_core(core)?;
        if let Some(previous) = previous_core
            && previous >= &core.constraint_ids
        {
            return Err(GeoExplanationError::invalid(
                "Geo explanation cores must be strictly sorted by constraint ids",
                [("field", "cores")],
            ));
        }
        previous_core = Some(&core.constraint_ids);
    }
    for correction in &artifact.correction_sets {
        validate_correction_set(correction)?;
        if !artifact
            .cores
            .iter()
            .all(|core| intersects_ids(&correction.observation_ids, &core.observation_ids))
        {
            return Err(GeoExplanationError::invalid(
                "Geo correction set must hit every enumerated core",
                [("field", "correction_sets[].observation_ids")],
            ));
        }
        if correction.minimal
            && correction.observation_ids.iter().any(|removed| {
                let reduced = correction
                    .observation_ids
                    .iter()
                    .filter(|candidate| *candidate != removed)
                    .cloned()
                    .collect::<Vec<_>>();
                artifact
                    .cores
                    .iter()
                    .all(|core| intersects_ids(&reduced, &core.observation_ids))
            })
        {
            return Err(GeoExplanationError::invalid(
                "Geo correction set minimality requires every member to hit at least one core alone",
                [("field", "correction_sets[].minimal")],
            ));
        }
    }
    if artifact.explanation_complete && !artifact.cores_complete {
        return Err(GeoExplanationError::invalid(
            "Geo explanation cannot be complete when core enumeration is incomplete",
            [("field", "explanation_complete")],
        ));
    }
    if artifact.counters.get("core_enumeration_ceiling") == Some(&1)
        && artifact.cores.iter().any(|core| {
            core.minimal
                || core
                    .deletion_checks
                    .iter()
                    .any(|check| check.status_after_deletion == GeoCompositionStatus::Conflict)
        })
    {
        return Err(GeoExplanationError::new(
            GeoExplanationErrorCode::CoreEnumerationCeiling,
            "Geo explanation ceiling artifacts cannot claim minimality",
            [("field", "cores[].minimal")],
        ));
    }
    validate_counters(&artifact.counters)
}

pub fn canonical_explanation_bytes(
    artifact: &GeoExplanationArtifact,
) -> Result<Vec<u8>, GeoExplanationError> {
    validate_explanation_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoExplanationError::invalid(
            "Geo explanation artifact could not be serialized",
            [("serde_error", error.to_string())],
        )
    })
}

pub fn validate_separation_request(
    request: &GeoSeparationRequest,
) -> Result<(), GeoExplanationError> {
    if request.version != CANON_GEO_SEPARATION_REQUEST_VERSION {
        return Err(GeoExplanationError::new(
            GeoExplanationErrorCode::UnsupportedVersion,
            "Unsupported Geo separation request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_SEPARATION_REQUEST_VERSION),
            ],
        ));
    }
    if let Some(subject) = &request.subject_ref {
        validate_subject_ref(subject)?;
    }
    canonicalize_composition_request(&request.request)?;
    if request.prospective.is_empty() {
        return Err(GeoExplanationError::invalid(
            "Geo separation requests require at least one prospective observation",
            [("field", "prospective")],
        ));
    }
    let mut previous_observation: Option<&str> = None;
    for observation in &request.prospective {
        validate_identifier("prospective[].id", &observation.id)?;
        validate_identifier("prospective[].contract_id", &observation.contract_id)?;
        if previous_observation.is_some_and(|previous| previous >= observation.id.as_str()) {
            return Err(GeoExplanationError::invalid(
                "Geo separation prospective observations must be strictly sorted by id",
                [("observation_id", observation.id.as_str())],
            ));
        }
        previous_observation = Some(&observation.id);
        if observation.outcomes.is_empty() {
            return Err(GeoExplanationError::invalid(
                "Geo separation prospective observations require an exhaustive outcome domain",
                [("observation_id", observation.id.as_str())],
            ));
        }
        let mut previous_outcome: Option<&str> = None;
        for outcome in &observation.outcomes {
            validate_identifier("prospective[].outcomes[].outcome_id", &outcome.outcome_id)?;
            if previous_outcome.is_some_and(|previous| previous >= outcome.outcome_id.as_str()) {
                return Err(GeoExplanationError::invalid(
                    "Geo separation outcomes must be strictly sorted by id",
                    [("outcome_id", outcome.outcome_id.as_str())],
                ));
            }
            previous_outcome = Some(&outcome.outcome_id);
            if outcome.induced.is_empty() {
                return Err(GeoExplanationError::invalid(
                    "Geo separation outcomes must induce at least one hard constraint",
                    [("outcome_id", outcome.outcome_id.as_str())],
                ));
            }
        }
    }
    Ok(())
}

pub fn canonical_separation_request_bytes(
    request: &GeoSeparationRequest,
) -> Result<Vec<u8>, GeoExplanationError> {
    validate_separation_request(request)?;
    serde_json::to_vec(request).map_err(|error| {
        GeoExplanationError::invalid(
            "Geo separation request could not be serialized",
            [("serde_error", error.to_string())],
        )
    })
}

pub fn validate_separation_artifact(
    artifact: &GeoSeparationArtifact,
) -> Result<(), GeoExplanationError> {
    if artifact.version != CANON_GEO_SEPARATION_VERSION {
        return Err(GeoExplanationError::new(
            GeoExplanationErrorCode::UnsupportedVersion,
            "Unsupported Geo separation artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_SEPARATION_VERSION),
            ],
        ));
    }
    if let Some(subject) = &artifact.subject_ref {
        validate_subject_ref(subject)?;
    }
    validate_blake3_ref("request_blake3", &artifact.request_blake3)?;
    if artifact.per_observation.is_empty() {
        return Err(GeoExplanationError::invalid(
            "Geo separation artifacts require at least one observation row",
            [("field", "per_observation")],
        ));
    }
    let mut previous_observation: Option<&str> = None;
    for observation in &artifact.per_observation {
        validate_identifier(
            "per_observation[].observation_id",
            &observation.observation_id,
        )?;
        if previous_observation
            .is_some_and(|previous| previous >= observation.observation_id.as_str())
        {
            return Err(GeoExplanationError::invalid(
                "Geo separation observations must be strictly sorted",
                [("observation_id", observation.observation_id.as_str())],
            ));
        }
        previous_observation = Some(&observation.observation_id);
        if observation.per_outcome.is_empty() {
            return Err(GeoExplanationError::invalid(
                "Geo separation observations require at least one outcome",
                [("observation_id", observation.observation_id.as_str())],
            ));
        }
        if !observation
            .per_outcome
            .iter()
            .all(|outcome| outcome.count_exact)
            && observation.redundant
        {
            return Err(GeoExplanationError::new(
                GeoExplanationErrorCode::SeparationResidualInexact,
                "Geo separation cannot claim redundancy from inexact counts",
                [("observation_id", observation.observation_id.as_str())],
            ));
        }
        let mut previous_outcome: Option<&str> = None;
        for outcome in &observation.per_outcome {
            validate_identifier(
                "per_observation[].per_outcome[].outcome_id",
                &outcome.outcome_id,
            )?;
            if previous_outcome.is_some_and(|previous| previous >= outcome.outcome_id.as_str()) {
                return Err(GeoExplanationError::invalid(
                    "Geo separation outcome rows must be strictly sorted",
                    [("outcome_id", outcome.outcome_id.as_str())],
                ));
            }
            previous_outcome = Some(&outcome.outcome_id);
        }
    }
    Ok(())
}

pub fn canonical_separation_bytes(
    artifact: &GeoSeparationArtifact,
) -> Result<Vec<u8>, GeoExplanationError> {
    validate_separation_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoExplanationError::invalid(
            "Geo separation artifact could not be serialized",
            [("serde_error", error.to_string())],
        )
    })
}

fn enumerate_minimal_cores(
    artifact: &mut GeoExplanationArtifact,
    request: &GeoCompositionRequest,
    evidence_index: &EvidenceIndex,
    order_rank: &BTreeMap<String, usize>,
    counter: &mut SolveCounter,
    budget: &GeoExplanationBudget,
) -> Result<bool, GeoExplanationError> {
    let constraint_ids = request
        .hard_constraints
        .iter()
        .map(|constraint| constraint.id.clone())
        .collect::<Vec<_>>();
    if constraint_ids.len() >= usize::BITS as usize {
        return Ok(false);
    }
    let total_masks = 1_usize
        .checked_shl(constraint_ids.len() as u32)
        .ok_or_else(|| GeoExplanationError::overflow("core subset masks"))?;
    let mut seen = artifact
        .cores
        .iter()
        .map(|core| core.constraint_ids.iter().cloned().collect::<BTreeSet<_>>())
        .collect::<BTreeSet<_>>();

    for mask in 1..total_masks {
        if artifact.cores.len() as u64 >= budget.max_cores {
            return Ok(false);
        }
        if counter.count >= budget.max_core_solves {
            return Ok(false);
        }
        let subset = constraint_ids
            .iter()
            .enumerate()
            .filter_map(|(index, id)| ((mask >> index) & 1 == 1).then_some(id.clone()))
            .collect::<Vec<_>>();
        let status = solve_subset_status(request, &subset, counter)?;
        if status != GeoCompositionStatus::Conflict {
            continue;
        }
        let (core_ids, deletion_checks) =
            minimize_conflict_subset(request, &subset, evidence_index, order_rank, counter)?;
        let signature = core_ids.iter().cloned().collect::<BTreeSet<_>>();
        if seen.insert(signature) {
            let core = core_from_ids(request, &core_ids, evidence_index, deletion_checks, true)?;
            artifact.cores.push(core);
            artifact
                .cores
                .sort_by(|left, right| left.constraint_ids.cmp(&right.constraint_ids));
        }
    }
    Ok(true)
}

fn enumerate_hitting_sets(
    cores: &[GeoMinimalCore],
    evidence_index: &EvidenceIndex,
    budget: &GeoExplanationBudget,
) -> Result<Vec<GeoCorrectionSet>, GeoExplanationError> {
    let universe = cores
        .iter()
        .flat_map(|core| core.observation_ids.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if universe.len() >= usize::BITS as usize {
        return Ok(Vec::new());
    }
    let total_masks = 1_usize
        .checked_shl(universe.len() as u32)
        .ok_or_else(|| GeoExplanationError::overflow("hitting set masks"))?;
    let core_sets = cores
        .iter()
        .map(|core| {
            core.observation_ids
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();
    let mut sets = Vec::new();
    for mask in 1..total_masks {
        if sets.len() as u64 >= budget.max_hitting_sets {
            break;
        }
        let candidate = universe
            .iter()
            .enumerate()
            .filter_map(|(index, id)| ((mask >> index) & 1 == 1).then_some(id.clone()))
            .collect::<BTreeSet<_>>();
        if !hits_all(&candidate, &core_sets) || !is_minimal_hitting_set(&candidate, &core_sets) {
            continue;
        }
        sets.push(correction_set_from_observations(
            &candidate,
            evidence_index,
            true,
        )?);
    }
    sets.sort_by(|left, right| {
        left.observation_ids
            .len()
            .cmp(&right.observation_ids.len())
            .then_with(|| left.observation_ids.cmp(&right.observation_ids))
    });
    Ok(sets)
}

fn minimize_conflict_subset(
    request: &GeoCompositionRequest,
    initial_ids: &[String],
    evidence_index: &EvidenceIndex,
    order_rank: &BTreeMap<String, usize>,
    counter: &mut SolveCounter,
) -> Result<(Vec<String>, Vec<GeoCoreDeletionCheck>), GeoExplanationError> {
    let mut active = initial_ids.iter().cloned().collect::<BTreeSet<_>>();
    for id in deletion_order(initial_ids, evidence_index, order_rank)? {
        if !active.contains(&id) {
            continue;
        }
        let trial = active
            .iter()
            .filter(|candidate| *candidate != &id)
            .cloned()
            .collect::<Vec<_>>();
        if trial.is_empty() {
            continue;
        }
        if solve_subset_status(request, &trial, counter)? == GeoCompositionStatus::Conflict {
            active.remove(&id);
        }
    }
    let core_ids = request_ordered_ids(request, &active);
    let deletion_checks =
        verify_deletion_minimality(request, &core_ids, evidence_index, order_rank, counter)?;
    Ok((core_ids, deletion_checks))
}

fn verify_deletion_minimality(
    request: &GeoCompositionRequest,
    constraint_ids: &[String],
    evidence_index: &EvidenceIndex,
    order_rank: &BTreeMap<String, usize>,
    counter: &mut SolveCounter,
) -> Result<Vec<GeoCoreDeletionCheck>, GeoExplanationError> {
    if constraint_ids.is_empty() {
        return Err(GeoExplanationError::invalid(
            "Geo minimal core verification requires at least one constraint",
            [("field", "constraint_ids")],
        ));
    }
    let baseline = solve_subset_status(request, constraint_ids, counter)?;
    if baseline != GeoCompositionStatus::Conflict {
        return Err(GeoExplanationError::invalid(
            "Geo minimal core candidate is not itself a conflict",
            [("status", status_name(baseline))],
        ));
    }
    let mut checks = Vec::new();
    for id in deletion_order(constraint_ids, evidence_index, order_rank)? {
        let trial = constraint_ids
            .iter()
            .filter(|candidate| *candidate != &id)
            .cloned()
            .collect::<Vec<_>>();
        let status = if trial.is_empty() {
            GeoCompositionStatus::Resolved
        } else {
            solve_subset_status(request, &trial, counter)?
        };
        if status == GeoCompositionStatus::Conflict {
            return Err(GeoExplanationError::new(
                GeoExplanationErrorCode::CoreNotMinimal,
                "Geo minimal core member deletion still conflicts",
                [
                    ("constraint_id", id),
                    ("status", status_name(status).to_string()),
                ],
            ));
        }
        checks.push(GeoCoreDeletionCheck {
            constraint_id: id,
            status_after_deletion: status,
        });
    }
    checks.sort_by(|left, right| left.constraint_id.cmp(&right.constraint_id));
    Ok(checks)
}

fn solve_subset_status(
    request: &GeoCompositionRequest,
    constraint_ids: &[String],
    counter: &mut SolveCounter,
) -> Result<GeoCompositionStatus, GeoExplanationError> {
    counter.record()?;
    let id_set = constraint_ids.iter().collect::<BTreeSet<_>>();
    let mut subset = request.clone();
    subset
        .hard_constraints
        .retain(|constraint| id_set.contains(&constraint.id));
    Ok(solve_composition(&subset)?.status)
}

fn request_with_outcome(
    request: &GeoCompositionRequest,
    observation_id: &str,
    outcome: &GeoProspectiveOutcome,
) -> Result<GeoCompositionRequest, GeoExplanationError> {
    let mut next = request.clone();
    for (index, induced) in outcome.induced.iter().enumerate() {
        next.hard_constraints.push(GeoHardConstraint {
            id: format!(
                "prospective:{observation_id}:{}:{index}",
                outcome.outcome_id
            ),
            constraint: induced.clone(),
        });
    }
    Ok(canonicalize_composition_request(&next)?)
}

fn exact_count_available(artifact: &super::GeoCompositionArtifact) -> bool {
    artifact.status != GeoCompositionStatus::BudgetFallback
        && artifact.summary.residual_model_count_complete
        && !artifact.summary.residual_model_count_saturated
}

fn core_from_ids(
    request: &GeoCompositionRequest,
    ids: &[String],
    evidence_index: &EvidenceIndex,
    deletion_checks: Vec<GeoCoreDeletionCheck>,
    minimal: bool,
) -> Result<GeoMinimalCore, GeoExplanationError> {
    let active = ids.iter().cloned().collect::<BTreeSet<_>>();
    let constraint_ids = request_ordered_ids(request, &active);
    for id in &constraint_ids {
        if id.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(GeoExplanationError::invalid(
                "Geo explanation cores must name stable constraint ids, not numeric indices",
                [("constraint_id", id.as_str())],
            ));
        }
    }
    let mut observation_ids = BTreeSet::new();
    let mut source_record_ids = BTreeSet::new();
    let mut rho_contract_ids = BTreeSet::new();
    let mut source_record_refs = BTreeSet::new();
    let mut admitted_values = BTreeMap::new();
    for id in &constraint_ids {
        let entry = evidence_index.by_constraint.get(id).ok_or_else(|| {
            GeoExplanationError::invalid(
                "Geo explanation core constraint is not mapped to an evidence admission",
                [("constraint_id", id.as_str())],
            )
        })?;
        observation_ids.insert(entry.observation_id.clone());
        rho_contract_ids.insert(entry.contract_id.clone());
        for source_id in &entry.source_record_ids {
            source_record_ids.insert(source_id.clone());
        }
        for source_ref in &entry.source_record_refs {
            source_record_refs.insert(source_ref.clone());
        }
        admitted_values.insert(entry.observation_id.clone(), entry.admitted_value.clone());
    }
    Ok(GeoMinimalCore {
        constraint_ids,
        observation_ids: observation_ids.into_iter().collect(),
        source_record_ids: source_record_ids.into_iter().collect(),
        rho_contract_ids: rho_contract_ids.into_iter().collect(),
        source_record_refs: source_record_refs.into_iter().collect(),
        admitted_values,
        deletion_checks,
        minimal,
    })
}

fn correction_set_from_observations(
    observation_ids: &BTreeSet<String>,
    evidence_index: &EvidenceIndex,
    minimal: bool,
) -> Result<GeoCorrectionSet, GeoExplanationError> {
    let mut source_record_ids = BTreeSet::new();
    let mut rho_contract_ids = BTreeSet::new();
    let mut source_record_refs = BTreeSet::new();
    let mut admitted_values = BTreeMap::new();
    for observation_id in observation_ids {
        let entry = evidence_index
            .by_observation
            .get(observation_id)
            .ok_or_else(|| {
                GeoExplanationError::invalid(
                    "Geo correction set observation is not mapped to evidence",
                    [("observation_id", observation_id.as_str())],
                )
            })?;
        rho_contract_ids.insert(entry.contract_id.clone());
        for source_id in &entry.source_record_ids {
            source_record_ids.insert(source_id.clone());
        }
        for source_ref in &entry.source_record_refs {
            source_record_refs.insert(source_ref.clone());
        }
        admitted_values.insert(entry.observation_id.clone(), entry.admitted_value.clone());
    }
    Ok(GeoCorrectionSet {
        observation_ids: observation_ids.iter().cloned().collect(),
        source_record_ids: source_record_ids.into_iter().collect(),
        rho_contract_ids: rho_contract_ids.into_iter().collect(),
        source_record_refs: source_record_refs.into_iter().collect(),
        admitted_values,
        minimal,
    })
}

fn request_ordered_ids(request: &GeoCompositionRequest, ids: &BTreeSet<String>) -> Vec<String> {
    request
        .hard_constraints
        .iter()
        .filter_map(|constraint| {
            ids.contains(&constraint.id)
                .then_some(constraint.id.clone())
        })
        .collect()
}

fn deletion_order(
    ids: &[String],
    evidence_index: &EvidenceIndex,
    order_rank: &BTreeMap<String, usize>,
) -> Result<Vec<String>, GeoExplanationError> {
    let mut ordered = ids.to_vec();
    ordered.sort_by(|left, right| {
        let left_rank = evidence_index
            .by_constraint
            .get(left)
            .and_then(|entry| order_rank.get(&entry.contract_id))
            .copied()
            .unwrap_or(usize::MAX);
        let right_rank = evidence_index
            .by_constraint
            .get(right)
            .and_then(|entry| order_rank.get(&entry.contract_id))
            .copied()
            .unwrap_or(usize::MAX);
        right_rank.cmp(&left_rank).then_with(|| left.cmp(right))
    });
    for id in &ordered {
        if !evidence_index.by_constraint.contains_key(id) {
            return Err(GeoExplanationError::invalid(
                "Geo explanation constraint is not mapped to an evidence admission",
                [("constraint_id", id.as_str())],
            ));
        }
    }
    Ok(ordered)
}

fn hits_all(candidate: &BTreeSet<String>, cores: &[BTreeSet<String>]) -> bool {
    cores.iter().all(|core| {
        core.iter()
            .any(|observation_id| candidate.contains(observation_id))
    })
}

fn intersects_ids(left: &[String], right: &[String]) -> bool {
    left.iter().any(|value| right.contains(value))
}

fn is_minimal_hitting_set(candidate: &BTreeSet<String>, cores: &[BTreeSet<String>]) -> bool {
    candidate.iter().all(|removed| {
        let reduced = candidate
            .iter()
            .filter(|id| *id != removed)
            .cloned()
            .collect::<BTreeSet<_>>();
        !hits_all(&reduced, cores)
    })
}

fn hitting_set_search_space(cores: &[GeoMinimalCore]) -> Result<usize, GeoExplanationError> {
    let universe_size = cores
        .iter()
        .flat_map(|core| core.observation_ids.iter())
        .collect::<BTreeSet<_>>()
        .len();
    1_usize
        .checked_shl(universe_size as u32)
        .ok_or_else(|| GeoExplanationError::overflow("hitting set search space"))
}

fn mark_explanation_incomplete(artifact: &mut GeoExplanationArtifact) {
    artifact.cores_complete = false;
    artifact.explanation_complete = false;
    for core in &mut artifact.cores {
        core.minimal = false;
        core.deletion_checks.clear();
    }
    for correction in &mut artifact.correction_sets {
        correction.minimal = false;
    }
}

fn canonical_request_matching_evidence(
    request: &GeoCompositionRequest,
    evidence: &GeoEvidenceCompilationArtifact,
) -> Result<GeoCompositionRequest, GeoExplanationError> {
    let request = canonicalize_composition_request(request)?;
    let evidence_request = canonicalize_composition_request(&evidence.composition_request)?;
    if request != evidence_request {
        return Err(GeoExplanationError::invalid(
            "Geo explanation request must match the evidence compilation request",
            [("field", "evidence.composition_request")],
        ));
    }
    Ok(request)
}

fn validate_reliability_order(
    order: &GeoReliabilityOrder,
    evidence_index: &EvidenceIndex,
) -> Result<BTreeMap<String, usize>, GeoExplanationError> {
    let mut actual = BTreeSet::new();
    let mut ranks = BTreeMap::new();
    for (index, contract_id) in order.contract_ids_most_reliable_first.iter().enumerate() {
        validate_identifier("contract_ids_most_reliable_first[]", contract_id)?;
        if !actual.insert(contract_id.clone()) {
            return Err(GeoExplanationError::invalid(
                "Geo reliability order repeats a contract id",
                [("contract_id", contract_id.as_str())],
            ));
        }
        ranks.insert(contract_id.clone(), index);
    }
    if actual != evidence_index.contract_ids {
        let expected = evidence_index
            .contract_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        let actual = actual.into_iter().collect::<Vec<_>>().join(",");
        return Err(GeoExplanationError::invalid(
            "Geo reliability order must name every admitted hard-evidence contract exactly once",
            [("expected", expected), ("actual", actual)],
        ));
    }
    Ok(ranks)
}

fn validate_budget(budget: &GeoExplanationBudget) -> Result<(), GeoExplanationError> {
    if budget.max_core_solves == 0 {
        return Err(GeoExplanationError::invalid(
            "Geo explanation max_core_solves must be positive",
            [("field", "max_core_solves")],
        ));
    }
    if budget.max_cores == 0 {
        return Err(GeoExplanationError::invalid(
            "Geo explanation max_cores must be positive",
            [("field", "max_cores")],
        ));
    }
    if budget.max_hitting_sets == 0 {
        return Err(GeoExplanationError::invalid(
            "Geo explanation max_hitting_sets must be positive",
            [("field", "max_hitting_sets")],
        ));
    }
    Ok(())
}

fn validate_core(core: &GeoMinimalCore) -> Result<(), GeoExplanationError> {
    validate_sorted_nonempty_ids("cores[].constraint_ids", &core.constraint_ids, true)?;
    validate_sorted_nonempty_ids("cores[].observation_ids", &core.observation_ids, false)?;
    validate_sorted_nonempty_ids("cores[].source_record_ids", &core.source_record_ids, false)?;
    validate_sorted_nonempty_ids("cores[].rho_contract_ids", &core.rho_contract_ids, false)?;
    validate_source_record_refs(&core.source_record_refs)?;
    for key in core.admitted_values.keys() {
        validate_identifier("cores[].admitted_values.key", key)?;
    }
    for check in &core.deletion_checks {
        validate_identifier(
            "cores[].deletion_checks[].constraint_id",
            &check.constraint_id,
        )?;
    }
    if core.minimal && core.deletion_checks.len() != core.constraint_ids.len() {
        return Err(GeoExplanationError::new(
            GeoExplanationErrorCode::CoreNotMinimal,
            "Geo minimal core must carry one deletion check per member",
            [("field", "deletion_checks")],
        ));
    }
    if core.minimal
        && core
            .deletion_checks
            .iter()
            .any(|check| check.status_after_deletion == GeoCompositionStatus::Conflict)
    {
        return Err(GeoExplanationError::new(
            GeoExplanationErrorCode::CoreNotMinimal,
            "Geo minimal core deletion checks include a remaining conflict",
            [("field", "deletion_checks")],
        ));
    }
    Ok(())
}

fn validate_correction_set(correction: &GeoCorrectionSet) -> Result<(), GeoExplanationError> {
    validate_sorted_nonempty_ids(
        "correction_sets[].observation_ids",
        &correction.observation_ids,
        false,
    )?;
    validate_sorted_nonempty_ids(
        "correction_sets[].source_record_ids",
        &correction.source_record_ids,
        false,
    )?;
    validate_sorted_ids(
        "correction_sets[].rho_contract_ids",
        &correction.rho_contract_ids,
        false,
    )?;
    validate_source_record_refs(&correction.source_record_refs)?;
    for key in correction.admitted_values.keys() {
        validate_identifier("correction_sets[].admitted_values.key", key)?;
    }
    Ok(())
}

fn validate_source_record_refs(refs: &[GeoEvidenceRecordRef]) -> Result<(), GeoExplanationError> {
    let mut previous: Option<&GeoEvidenceRecordRef> = None;
    for reference in refs {
        validate_identifier(
            "source_record_refs[].source_record_id",
            &reference.source_record_id,
        )?;
        validate_identifier(
            "source_record_refs[].source_vintage",
            &reference.source_vintage,
        )?;
        validate_hex_digest(
            "source_record_refs[].record_blake3",
            &reference.record_blake3,
        )?;
        if previous.is_some_and(|previous| previous >= reference) {
            return Err(GeoExplanationError::invalid(
                "Geo explanation source record refs must be strictly sorted",
                [("field", "source_record_refs")],
            ));
        }
        previous = Some(reference);
    }
    Ok(())
}

fn validate_subject_ref(subject: &GeoExplanationSubjectRef) -> Result<(), GeoExplanationError> {
    validate_identifier("subject_ref.accession", &subject.accession)?;
    validate_identifier("subject_ref.loan_id", &subject.loan_id)?;
    validate_identifier("subject_ref.subject_id", &subject.subject_id)
}

fn validate_counters(counters: &BTreeMap<String, u64>) -> Result<(), GeoExplanationError> {
    for required in ["core_solves", "cores_enumerated", "hitting_sets"] {
        if !counters.contains_key(required) {
            return Err(GeoExplanationError::invalid(
                "Geo explanation counters are missing a required key",
                [("counter", required)],
            ));
        }
    }
    for key in counters.keys() {
        validate_identifier("counters.key", key)?;
    }
    Ok(())
}

fn validate_sorted_nonempty_ids(
    field: &str,
    values: &[String],
    reject_numeric: bool,
) -> Result<(), GeoExplanationError> {
    if values.is_empty() {
        return Err(GeoExplanationError::invalid(
            "Geo explanation id lists must be non-empty",
            [("field", field)],
        ));
    }
    validate_sorted_ids(field, values, reject_numeric)
}

fn validate_sorted_ids(
    field: &str,
    values: &[String],
    reject_numeric: bool,
) -> Result<(), GeoExplanationError> {
    let mut previous: Option<&str> = None;
    for value in values {
        validate_identifier(field, value)?;
        if reject_numeric && value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(GeoExplanationError::invalid(
                "Geo explanation ids must not be numeric constraint indices",
                [("field", field), ("value", value.as_str())],
            ));
        }
        if previous.is_some_and(|previous| previous >= value.as_str()) {
            return Err(GeoExplanationError::invalid(
                "Geo explanation ids must be strictly sorted and distinct",
                [("field", field), ("value", value.as_str())],
            ));
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_blake3_ref(field: &str, value: &str) -> Result<(), GeoExplanationError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(GeoExplanationError::invalid(
            "Geo explanation digest must be blake3-prefixed lowercase hex",
            [("field", field), ("value", value)],
        ));
    };
    validate_hex_digest(field, hex)
}

fn validate_hex_digest(field: &str, value: &str) -> Result<(), GeoExplanationError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoExplanationError::invalid(
            "Geo explanation digests must be 64 lowercase hexadecimal characters",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_identifier(field: &str, value: &str) -> Result<(), GeoExplanationError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoExplanationError::invalid(
            "Geo explanation identifiers must be non-empty and already canonical",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn request_blake3(request: &GeoCompositionRequest) -> Result<String, GeoExplanationError> {
    let canonical = canonicalize_composition_request(request)?;
    let request_bytes = serde_json::to_vec(&canonical).map_err(|error| {
        GeoExplanationError::invalid(
            "Geo explanation request could not be serialized",
            [("serde_error", error.to_string())],
        )
    })?;
    Ok(format!("blake3:{}", blake3::hash(&request_bytes).to_hex()))
}

fn evidence_blake3(
    evidence: &GeoEvidenceCompilationArtifact,
) -> Result<String, GeoExplanationError> {
    let bytes = canonical_evidence_compilation_bytes(evidence).map_err(|error| {
        GeoExplanationError::invalid(
            "Geo explanation evidence could not be serialized",
            [("serde_error", error.to_string())],
        )
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn not_conflict_error(status: GeoCompositionStatus) -> GeoExplanationError {
    GeoExplanationError::new(
        GeoExplanationErrorCode::ExplanationNotConflict,
        "Geo explanation minimal cores require a conflict composition",
        [("status", status_name(status))],
    )
}

fn status_name(status: GeoCompositionStatus) -> &'static str {
    match status {
        GeoCompositionStatus::Resolved => "resolved",
        GeoCompositionStatus::Ambiguous => "ambiguous",
        GeoCompositionStatus::Conflict => "conflict",
        GeoCompositionStatus::BudgetFallback => "budget_fallback",
    }
}

#[derive(Debug, Clone)]
struct EvidenceCoreEntry {
    observation_id: String,
    contract_id: String,
    source_record_ids: Vec<String>,
    source_record_refs: Vec<GeoEvidenceRecordRef>,
    admitted_value: String,
}

#[derive(Debug, Clone)]
struct EvidenceIndex {
    by_constraint: BTreeMap<String, EvidenceCoreEntry>,
    by_observation: BTreeMap<String, EvidenceCoreEntry>,
    contract_ids: BTreeSet<String>,
}

impl EvidenceIndex {
    fn from_evidence(
        evidence: &GeoEvidenceCompilationArtifact,
    ) -> Result<Self, GeoExplanationError> {
        let mut by_constraint = BTreeMap::new();
        let mut by_observation = BTreeMap::new();
        let mut contract_ids = BTreeSet::new();
        for admission in &evidence.admissions {
            if admission.generated_ids.is_empty() {
                continue;
            }
            validate_identifier("admissions[].observation_id", &admission.observation_id)?;
            validate_identifier("admissions[].contract.id", &admission.contract.id)?;
            contract_ids.insert(admission.contract.id.clone());
            let mut source_record_refs = admission.source_records.clone();
            source_record_refs.sort();
            validate_source_record_refs(&source_record_refs)?;
            let source_record_ids = source_record_refs
                .iter()
                .map(|record| record.source_record_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let admitted_value =
                serde_json::to_string(&admission.observation).map_err(|error| {
                    GeoExplanationError::invalid(
                        "Geo explanation admitted value could not be serialized",
                        [("serde_error", error.to_string())],
                    )
                })?;
            let entry = EvidenceCoreEntry {
                observation_id: admission.observation_id.clone(),
                contract_id: admission.contract.id.clone(),
                source_record_ids,
                source_record_refs,
                admitted_value,
            };
            if by_observation
                .insert(admission.observation_id.clone(), entry.clone())
                .is_some()
            {
                return Err(GeoExplanationError::invalid(
                    "Geo explanation evidence repeats an observation id",
                    [("observation_id", admission.observation_id.as_str())],
                ));
            }
            for generated_id in &admission.generated_ids {
                validate_identifier("admissions[].generated_ids[]", generated_id)?;
                if by_constraint
                    .insert(generated_id.clone(), entry.clone())
                    .is_some()
                {
                    return Err(GeoExplanationError::invalid(
                        "Geo explanation evidence repeats a generated solver id",
                        [("generated_id", generated_id.as_str())],
                    ));
                }
            }
        }
        if by_constraint.is_empty() {
            return Err(GeoExplanationError::invalid(
                "Geo explanation evidence must map at least one hard constraint",
                [("field", "admissions[].generated_ids")],
            ));
        }
        Ok(Self {
            by_constraint,
            by_observation,
            contract_ids,
        })
    }

    fn source_record_count(&self) -> Result<u64, GeoExplanationError> {
        let count = self
            .by_observation
            .values()
            .flat_map(|entry| entry.source_record_ids.iter())
            .collect::<BTreeSet<_>>()
            .len();
        u64::try_from(count).map_err(|_| GeoExplanationError::overflow("source record count"))
    }
}

#[derive(Debug, Clone)]
struct SolveCounter {
    max: u64,
    count: u64,
}

impl SolveCounter {
    fn new(max: u64) -> Self {
        Self { max, count: 0 }
    }

    fn record(&mut self) -> Result<(), GeoExplanationError> {
        if self.count >= self.max {
            return Err(GeoExplanationError::new(
                GeoExplanationErrorCode::BudgetExceeded,
                "Geo explanation exceeded the declared re-solve budget",
                [
                    ("field", "max_core_solves".to_string()),
                    ("configured", self.max.to_string()),
                ],
            ));
        }
        self.count = self
            .count
            .checked_add(1)
            .ok_or_else(|| GeoExplanationError::overflow("core_solves"))?;
        Ok(())
    }
}
