#![forbid(unsafe_code)]

//! Truth-blind, accretive evidence stacking for bounded Geo populations.
//!
//! Candidate generation and evaluation truth stay outside this seam. An
//! overlay can only add rho contracts and observations to a named case. The
//! exact base population and every before/after evidence request are content
//! bound in the emitted artifact, which can be replay-validated before
//! evaluation.

use super::{
    composition::{GeoCompositionModel, GeoEntityRef, GeoIntegerMemberValue},
    evaluation::{CANON_GEO_POPULATION_REQUEST_VERSION, GeoPopulationEvaluationRequest},
    evidence::{
        GeoEvidenceCompilationRequest, GeoEvidenceDisposition, GeoEvidenceError,
        GeoEvidenceRecordRef, GeoRhoContract, GeoRhoObservation, GeoRhoObservationKind,
        compile_evidence,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION: &str =
    "canon_geo_population_evidence_stack_request.v0";
pub const CANON_GEO_POPULATION_EVIDENCE_STACK_VERSION: &str =
    "canon_geo_population_evidence_stack.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPopulationEvidenceStackRequest {
    pub version: String,
    pub case_overlays: Vec<GeoPopulationCaseEvidenceOverlay>,
    pub max_overlay_cases: usize,
    /// Bounds all observations carried by this overlay, including exact
    /// idempotent replays that are ultimately counted as reused.
    pub max_overlay_observations: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPopulationCaseEvidenceOverlay {
    pub case_id: String,
    /// Optional optimistic-concurrency binding. When present, the overlay is
    /// refused unless the case's canonical pre-stack evidence request matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_base_evidence_blake3: Option<String>,
    #[serde(default)]
    pub contracts: Vec<GeoRhoContract>,
    pub observations: Vec<GeoRhoObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPopulationEvidenceStackCaseSummary {
    pub case_id: String,
    pub base_evidence_blake3: String,
    pub stacked_evidence_blake3: String,
    pub added_contracts: u64,
    pub reused_contracts: u64,
    pub added_observations: u64,
    pub reused_observations: u64,
    /// Provenance volume only. It is neither a confidence score nor an
    /// independent-information count.
    pub added_source_records: u64,
    pub hard_constraint_observations: u64,
    pub soft_preference_observations: u64,
    pub diagnostic_observations: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPopulationEvidenceStackSummary {
    pub population_cases: u64,
    pub overlay_cases: u64,
    pub changed_cases: u64,
    pub added_contracts: u64,
    pub reused_contracts: u64,
    pub added_observations: u64,
    pub reused_observations: u64,
    /// Provenance volume only. Source count is not evidence weight.
    pub added_source_records: u64,
    pub hard_constraint_observations: u64,
    pub soft_preference_observations: u64,
    pub diagnostic_observations: u64,
    pub cases: Vec<GeoPopulationEvidenceStackCaseSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPopulationEvidenceStackArtifact {
    pub version: String,
    pub request_version: String,
    pub base_population_blake3: String,
    pub overlay_blake3: String,
    pub summary: GeoPopulationEvidenceStackSummary,
    /// Retained so the artifact can prove that truth and candidate universes
    /// were not rewritten while evidence was stacked.
    pub base_population: GeoPopulationEvaluationRequest,
    pub request: GeoPopulationEvidenceStackRequest,
    pub population: GeoPopulationEvaluationRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoEvidenceStackErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    Evidence,
    ArithmeticOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoEvidenceStackError {
    pub code: GeoEvidenceStackErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoEvidenceStackError {
    fn new(
        code: GeoEvidenceStackErrorCode,
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
        Self::new(GeoEvidenceStackErrorCode::InvalidInput, message, detail)
    }

    fn overflow(field: &str) -> Self {
        Self::new(
            GeoEvidenceStackErrorCode::ArithmeticOverflow,
            "Geo population evidence-stack accounting overflowed",
            [("field", field)],
        )
    }
}

impl From<GeoEvidenceError> for GeoEvidenceStackError {
    fn from(error: GeoEvidenceError) -> Self {
        let mut detail = error.detail;
        detail.insert("evidence_code".to_string(), format!("{:?}", error.code));
        Self {
            code: GeoEvidenceStackErrorCode::Evidence,
            message: error.message,
            detail,
        }
    }
}

impl fmt::Display for GeoEvidenceStackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoEvidenceStackError {}

pub fn stack_population_evidence(
    base_population: &GeoPopulationEvaluationRequest,
    request: &GeoPopulationEvidenceStackRequest,
) -> Result<GeoPopulationEvidenceStackArtifact, GeoEvidenceStackError> {
    if request.version != CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION {
        return Err(GeoEvidenceStackError::new(
            GeoEvidenceStackErrorCode::UnsupportedVersion,
            "Unsupported Geo population evidence-stack request version",
            [
                ("actual", request.version.as_str()),
                (
                    "expected",
                    CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION,
                ),
            ],
        ));
    }
    if request.case_overlays.is_empty() {
        return Err(GeoEvidenceStackError::invalid(
            "Geo evidence stacking requires at least one case overlay",
            [("field", "case_overlays")],
        ));
    }
    if request.max_overlay_cases == 0 || request.case_overlays.len() > request.max_overlay_cases {
        return Err(GeoEvidenceStackError::new(
            GeoEvidenceStackErrorCode::BudgetExceeded,
            "Geo evidence overlay exceeds its declared case budget",
            [
                ("overlay_cases", request.case_overlays.len().to_string()),
                ("max_overlay_cases", request.max_overlay_cases.to_string()),
            ],
        ));
    }
    if request.max_overlay_observations == 0 {
        return Err(GeoEvidenceStackError::new(
            GeoEvidenceStackErrorCode::BudgetExceeded,
            "Geo evidence overlay requires a positive observation budget",
            [("max_overlay_observations", "0")],
        ));
    }

    let base_population = canonicalize_population(base_population)?;
    let base_population_blake3 = digest_json(&base_population, "base_population")?;
    let request = canonicalize_stack_request(request)?;
    let requested_observations =
        request
            .case_overlays
            .iter()
            .try_fold(0usize, |count, overlay| {
                count
                    .checked_add(overlay.observations.len())
                    .ok_or_else(|| GeoEvidenceStackError::overflow("requested_observations"))
            })?;
    if requested_observations > request.max_overlay_observations {
        return Err(GeoEvidenceStackError::new(
            GeoEvidenceStackErrorCode::BudgetExceeded,
            "Geo evidence overlay exceeds its declared observation budget",
            [
                ("observations", requested_observations.to_string()),
                (
                    "max_overlay_observations",
                    request.max_overlay_observations.to_string(),
                ),
            ],
        ));
    }
    let overlay_blake3 = digest_json(&request, "overlay")?;

    let mut population = base_population.clone();
    let case_indexes = population
        .cases
        .iter()
        .enumerate()
        .map(|(index, case)| (case.id.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let mut case_summaries = Vec::with_capacity(request.case_overlays.len());

    for overlay in &request.case_overlays {
        let case_index = case_indexes.get(&overlay.case_id).copied().ok_or_else(|| {
            GeoEvidenceStackError::invalid(
                "Geo evidence overlay references an unknown population case",
                [("case_id", overlay.case_id.as_str())],
            )
        })?;
        let case = &mut population.cases[case_index];
        let base_evidence_blake3 = digest_json(&case.evidence, "base_evidence")?;
        if let Some(expected) = &overlay.expected_base_evidence_blake3 {
            validate_blake3("expected_base_evidence_blake3", expected)?;
            if expected != &base_evidence_blake3 {
                return Err(GeoEvidenceStackError::invalid(
                    "Geo evidence overlay is stale for its base case evidence",
                    [
                        ("case_id", overlay.case_id.as_str()),
                        ("expected", expected.as_str()),
                        ("actual", base_evidence_blake3.as_str()),
                    ],
                ));
            }
        }

        let merged = merge_case_evidence(&case.evidence, overlay)?;
        case.evidence = merged.evidence;
        let stacked_evidence_blake3 = digest_json(&case.evidence, "stacked_evidence")?;
        case_summaries.push(GeoPopulationEvidenceStackCaseSummary {
            case_id: overlay.case_id.clone(),
            base_evidence_blake3,
            stacked_evidence_blake3,
            added_contracts: to_u64(merged.added_contracts, "added_contracts")?,
            reused_contracts: to_u64(merged.reused_contracts, "reused_contracts")?,
            added_observations: to_u64(merged.added_observations, "added_observations")?,
            reused_observations: to_u64(merged.reused_observations, "reused_observations")?,
            added_source_records: to_u64(merged.added_source_records, "added_source_records")?,
            hard_constraint_observations: to_u64(
                merged.hard_constraint_observations,
                "hard_constraint_observations",
            )?,
            soft_preference_observations: to_u64(
                merged.soft_preference_observations,
                "soft_preference_observations",
            )?,
            diagnostic_observations: to_u64(
                merged.diagnostic_observations,
                "diagnostic_observations",
            )?,
        });
    }

    population = canonicalize_population(&population)?;
    let summary = summarize_stack(&population, &case_summaries)?;
    Ok(GeoPopulationEvidenceStackArtifact {
        version: CANON_GEO_POPULATION_EVIDENCE_STACK_VERSION.to_string(),
        request_version: request.version.clone(),
        base_population_blake3,
        overlay_blake3,
        summary,
        base_population,
        request,
        population,
    })
}

pub fn validate_population_evidence_stack_artifact(
    artifact: &GeoPopulationEvidenceStackArtifact,
) -> Result<(), GeoEvidenceStackError> {
    if artifact.version != CANON_GEO_POPULATION_EVIDENCE_STACK_VERSION
        || artifact.request_version != CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION
    {
        return Err(GeoEvidenceStackError::new(
            GeoEvidenceStackErrorCode::UnsupportedVersion,
            "Unsupported Geo population evidence-stack artifact version",
            [
                ("actual_version", artifact.version.as_str()),
                ("actual_request_version", artifact.request_version.as_str()),
            ],
        ));
    }
    let replay = stack_population_evidence(&artifact.base_population, &artifact.request)?;
    if replay != *artifact {
        return Err(GeoEvidenceStackError::invalid(
            "Geo population evidence-stack artifact does not replay from its bound inputs",
            [("field", "artifact")],
        ));
    }
    Ok(())
}

pub fn canonical_population_evidence_stack_bytes(
    artifact: &GeoPopulationEvidenceStackArtifact,
) -> Result<Vec<u8>, GeoEvidenceStackError> {
    validate_population_evidence_stack_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoEvidenceStackError::invalid(
            "Geo population evidence-stack artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

struct MergedCaseEvidence {
    evidence: GeoEvidenceCompilationRequest,
    added_contracts: usize,
    reused_contracts: usize,
    added_observations: usize,
    reused_observations: usize,
    added_source_records: usize,
    hard_constraint_observations: usize,
    soft_preference_observations: usize,
    diagnostic_observations: usize,
}

fn merge_case_evidence(
    base: &GeoEvidenceCompilationRequest,
    overlay: &GeoPopulationCaseEvidenceOverlay,
) -> Result<MergedCaseEvidence, GeoEvidenceStackError> {
    let mut evidence = canonicalize_evidence(base)?;
    let base_profile = evidence.profile.clone();
    let base_universe = evidence.universe.clone();
    let base_max_assignments = evidence.max_assignments;
    let base_max_materialized_models = evidence.max_materialized_models;
    let mut contracts = evidence
        .contracts
        .iter()
        .map(|contract| (contract.id.clone(), contract.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut added_contracts = 0usize;
    let mut reused_contracts = 0usize;
    for contract in &overlay.contracts {
        match contracts.get(&contract.id) {
            Some(existing) if existing == contract => reused_contracts += 1,
            Some(_) => {
                return Err(GeoEvidenceStackError::invalid(
                    "Geo evidence overlay redefines an existing rho contract",
                    [
                        ("case_id", overlay.case_id.as_str()),
                        ("contract_id", contract.id.as_str()),
                    ],
                ));
            }
            None => {
                contracts.insert(contract.id.clone(), contract.clone());
                added_contracts += 1;
            }
        }
    }

    let mut observations = evidence
        .observations
        .iter()
        .map(|observation| (observation.id.clone(), observation.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut semantic_observations = BTreeMap::new();
    for observation in observations.values() {
        let signature = observation_semantic_digest(observation)?;
        if let Some(existing_id) = semantic_observations.insert(signature, observation.id.clone()) {
            return Err(GeoEvidenceStackError::invalid(
                "Geo base evidence repeats one semantic observation under different ids",
                [
                    ("case_id", overlay.case_id.as_str()),
                    ("first_observation_id", existing_id.as_str()),
                    ("second_observation_id", observation.id.as_str()),
                ],
            ));
        }
    }

    let mut added_observation_ids = BTreeSet::new();
    let mut reused_observations = 0usize;
    let mut added_source_records = 0usize;
    for observation in &overlay.observations {
        if !contracts.contains_key(&observation.contract_id) {
            return Err(GeoEvidenceStackError::invalid(
                "Geo evidence overlay observation references an undeclared rho contract",
                [
                    ("case_id", overlay.case_id.as_str()),
                    ("observation_id", observation.id.as_str()),
                    ("contract_id", observation.contract_id.as_str()),
                ],
            ));
        }
        match observations.get(&observation.id) {
            Some(existing) if existing == observation => {
                reused_observations += 1;
                continue;
            }
            Some(_) => {
                return Err(GeoEvidenceStackError::invalid(
                    "Geo evidence overlay redefines an existing observation id",
                    [
                        ("case_id", overlay.case_id.as_str()),
                        ("observation_id", observation.id.as_str()),
                    ],
                ));
            }
            None => {}
        }
        let signature = observation_semantic_digest(observation)?;
        if let Some(existing_id) = semantic_observations.get(&signature) {
            return Err(GeoEvidenceStackError::invalid(
                "Geo evidence overlay repeats one semantic observation under a different id",
                [
                    ("case_id", overlay.case_id.as_str()),
                    ("existing_observation_id", existing_id.as_str()),
                    ("overlay_observation_id", observation.id.as_str()),
                ],
            ));
        }
        semantic_observations.insert(signature, observation.id.clone());
        added_source_records = added_source_records
            .checked_add(observation.source_records.len())
            .ok_or_else(|| GeoEvidenceStackError::overflow("added_source_records"))?;
        added_observation_ids.insert(observation.id.clone());
        observations.insert(observation.id.clone(), observation.clone());
    }

    for contract in &overlay.contracts {
        if !overlay
            .observations
            .iter()
            .any(|observation| observation.contract_id == contract.id)
            && !evidence
                .observations
                .iter()
                .any(|observation| observation.contract_id == contract.id)
        {
            return Err(GeoEvidenceStackError::invalid(
                "Geo evidence overlay declares an unused rho contract",
                [
                    ("case_id", overlay.case_id.as_str()),
                    ("contract_id", contract.id.as_str()),
                ],
            ));
        }
    }
    evidence.contracts = contracts.into_values().collect();
    evidence.observations = observations.into_values().collect();
    evidence = canonicalize_evidence(&evidence)?;
    if evidence.profile != base_profile
        || evidence.universe != base_universe
        || evidence.max_assignments != base_max_assignments
        || evidence.max_materialized_models != base_max_materialized_models
    {
        return Err(GeoEvidenceStackError::invalid(
            "Geo evidence stacking cannot alter the base solver domain or budgets",
            [("case_id", overlay.case_id.as_str())],
        ));
    }
    let compilation = compile_evidence(&evidence)?;
    let mut hard_constraint_observations = 0usize;
    let mut soft_preference_observations = 0usize;
    let mut diagnostic_observations = 0usize;
    for admission in compilation
        .admissions
        .iter()
        .filter(|admission| added_observation_ids.contains(&admission.observation_id))
    {
        match admission.disposition {
            GeoEvidenceDisposition::HardConstraint => hard_constraint_observations += 1,
            GeoEvidenceDisposition::SoftPreference => soft_preference_observations += 1,
            GeoEvidenceDisposition::DiagnosticOnly => diagnostic_observations += 1,
        }
    }

    Ok(MergedCaseEvidence {
        evidence,
        added_contracts,
        reused_contracts,
        added_observations: added_observation_ids.len(),
        reused_observations,
        added_source_records,
        hard_constraint_observations,
        soft_preference_observations,
        diagnostic_observations,
    })
}

fn canonicalize_stack_request(
    request: &GeoPopulationEvidenceStackRequest,
) -> Result<GeoPopulationEvidenceStackRequest, GeoEvidenceStackError> {
    let mut canonical = request.clone();
    for overlay in &mut canonical.case_overlays {
        validate_identifier("case_id", &overlay.case_id)?;
        if overlay.observations.is_empty() {
            return Err(GeoEvidenceStackError::invalid(
                "Geo evidence case overlays require at least one observation",
                [("case_id", overlay.case_id.as_str())],
            ));
        }
        overlay
            .contracts
            .sort_by(|left, right| left.id.cmp(&right.id));
        reject_duplicate_keys(
            "overlay.contracts",
            overlay.contracts.iter().map(|contract| &contract.id),
        )?;
        for observation in &mut overlay.observations {
            canonicalize_observation(observation);
        }
        overlay
            .observations
            .sort_by(|left, right| left.id.cmp(&right.id));
        reject_duplicate_keys(
            "overlay.observations",
            overlay
                .observations
                .iter()
                .map(|observation| &observation.id),
        )?;
    }
    canonical
        .case_overlays
        .sort_by(|left, right| left.case_id.cmp(&right.case_id));
    reject_duplicate_keys(
        "case_overlays",
        canonical
            .case_overlays
            .iter()
            .map(|overlay| &overlay.case_id),
    )?;
    Ok(canonical)
}

fn canonicalize_population(
    population: &GeoPopulationEvaluationRequest,
) -> Result<GeoPopulationEvaluationRequest, GeoEvidenceStackError> {
    if population.version != CANON_GEO_POPULATION_REQUEST_VERSION {
        return Err(GeoEvidenceStackError::new(
            GeoEvidenceStackErrorCode::UnsupportedVersion,
            "Unsupported Geo population request version for evidence stacking",
            [
                ("actual", population.version.as_str()),
                ("expected", CANON_GEO_POPULATION_REQUEST_VERSION),
            ],
        ));
    }
    if population.max_cases == 0 || population.cases.len() > population.max_cases {
        return Err(GeoEvidenceStackError::new(
            GeoEvidenceStackErrorCode::BudgetExceeded,
            "Geo population exceeds its declared case budget before evidence stacking",
            [
                ("cases", population.cases.len().to_string()),
                ("max_cases", population.max_cases.to_string()),
            ],
        ));
    }
    let mut canonical = population.clone();
    for case in &mut canonical.cases {
        validate_identifier("case_id", &case.id)?;
        canonicalize_truth(&case.id, &mut case.truth)?;
        case.evidence = canonicalize_evidence(&case.evidence)?;
    }
    canonical
        .cases
        .sort_by(|left, right| left.id.cmp(&right.id));
    reject_duplicate_keys(
        "population.cases",
        canonical.cases.iter().map(|case| &case.id),
    )?;
    Ok(canonical)
}

fn canonicalize_truth(
    case_id: &str,
    truth: &mut GeoCompositionModel,
) -> Result<(), GeoEvidenceStackError> {
    truth.parcels.sort();
    truth.buildings.sort();
    if truth.parcels.is_empty() && truth.buildings.is_empty() {
        return Err(GeoEvidenceStackError::invalid(
            "Geo population truth must contain at least one member",
            [("case_id", case_id)],
        ));
    }
    reject_duplicate_keys("truth.parcels", truth.parcels.iter())?;
    reject_duplicate_keys("truth.buildings", truth.buildings.iter())
}

fn canonicalize_evidence(
    request: &GeoEvidenceCompilationRequest,
) -> Result<GeoEvidenceCompilationRequest, GeoEvidenceStackError> {
    let compilation = compile_evidence(request)?;
    let mut canonical = request.clone();
    canonical.profile = compilation.composition_request.profile;
    canonical.universe = compilation.composition_request.universe;
    canonical.max_assignments = compilation.composition_request.max_assignments;
    canonical.max_materialized_models = compilation.composition_request.max_materialized_models;
    canonical
        .contracts
        .sort_by(|left, right| left.id.cmp(&right.id));
    for observation in &mut canonical.observations {
        canonicalize_observation(observation);
    }
    canonical
        .observations
        .sort_by(|left, right| left.id.cmp(&right.id));
    compile_evidence(&canonical)?;
    Ok(canonical)
}

fn canonicalize_observation(observation: &mut GeoRhoObservation) {
    observation.source_records.sort();
    match &mut observation.observation {
        GeoRhoObservationKind::ExactSets { sets, .. } => {
            for set in sets.iter_mut() {
                set.sort();
            }
            sets.sort();
        }
        GeoRhoObservationKind::ExistentialMembership { members } => {
            members.sort_by(compare_entity_refs);
        }
        GeoRhoObservationKind::IntegerSumBand { values, .. } => {
            values.sort_by(compare_integer_values);
        }
        GeoRhoObservationKind::PreferMember { .. } => {}
    }
}

fn compare_entity_refs(left: &GeoEntityRef, right: &GeoEntityRef) -> std::cmp::Ordering {
    (left.level, left.id.as_str()).cmp(&(right.level, right.id.as_str()))
}

fn compare_integer_values(
    left: &GeoIntegerMemberValue,
    right: &GeoIntegerMemberValue,
) -> std::cmp::Ordering {
    left.id.cmp(&right.id)
}

fn observation_semantic_digest(
    observation: &GeoRhoObservation,
) -> Result<String, GeoEvidenceStackError> {
    #[derive(Serialize)]
    struct SemanticObservation<'a> {
        source_records: &'a [GeoEvidenceRecordRef],
        valid_time: &'a Option<super::evidence::GeoValidTimeInterval>,
        observation: &'a GeoRhoObservationKind,
    }
    digest_json(
        &SemanticObservation {
            source_records: &observation.source_records,
            valid_time: &observation.valid_time,
            observation: &observation.observation,
        },
        "semantic_observation",
    )
}

fn summarize_stack(
    population: &GeoPopulationEvaluationRequest,
    cases: &[GeoPopulationEvidenceStackCaseSummary],
) -> Result<GeoPopulationEvidenceStackSummary, GeoEvidenceStackError> {
    let mut summary = GeoPopulationEvidenceStackSummary {
        population_cases: to_u64(population.cases.len(), "population_cases")?,
        overlay_cases: to_u64(cases.len(), "overlay_cases")?,
        changed_cases: 0,
        added_contracts: 0,
        reused_contracts: 0,
        added_observations: 0,
        reused_observations: 0,
        added_source_records: 0,
        hard_constraint_observations: 0,
        soft_preference_observations: 0,
        diagnostic_observations: 0,
        cases: cases.to_vec(),
    };
    for case in cases {
        if case.added_contracts > 0 || case.added_observations > 0 {
            summary.changed_cases = checked_add(summary.changed_cases, 1, "changed_cases")?;
        }
        macro_rules! add_field {
            ($field:ident) => {
                summary.$field = checked_add(summary.$field, case.$field, stringify!($field))?;
            };
        }
        add_field!(added_contracts);
        add_field!(reused_contracts);
        add_field!(added_observations);
        add_field!(reused_observations);
        add_field!(added_source_records);
        add_field!(hard_constraint_observations);
        add_field!(soft_preference_observations);
        add_field!(diagnostic_observations);
    }
    Ok(summary)
}

fn reject_duplicate_keys<'a>(
    field: &str,
    values: impl IntoIterator<Item = &'a String>,
) -> Result<(), GeoEvidenceStackError> {
    let mut previous: Option<&str> = None;
    for value in values {
        if previous == Some(value.as_str()) {
            return Err(GeoEvidenceStackError::invalid(
                "Geo population evidence stacking requires distinct keyed values",
                [("field", field), ("duplicate", value.as_str())],
            ));
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_identifier(field: &str, value: &str) -> Result<(), GeoEvidenceStackError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoEvidenceStackError::invalid(
            "Geo population evidence-stack identifiers must be non-empty and canonical",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_blake3(field: &str, value: &str) -> Result<(), GeoEvidenceStackError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoEvidenceStackError::invalid(
            "Geo population evidence-stack digests must be lowercase 64-character BLAKE3 hex",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn digest_json(value: &impl Serialize, field: &str) -> Result<String, GeoEvidenceStackError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        let error = error.to_string();
        GeoEvidenceStackError::invalid(
            "Geo population evidence-stack value could not be serialized",
            [
                ("field".to_string(), field.to_string()),
                ("error".to_string(), error),
            ],
        )
    })?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn to_u64(value: usize, field: &str) -> Result<u64, GeoEvidenceStackError> {
    u64::try_from(value).map_err(|_| GeoEvidenceStackError::overflow(field))
}

fn checked_add(left: u64, right: u64, field: &str) -> Result<u64, GeoEvidenceStackError> {
    left.checked_add(right)
        .ok_or_else(|| GeoEvidenceStackError::overflow(field))
}
