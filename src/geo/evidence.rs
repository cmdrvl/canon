#![forbid(unsafe_code)]

//! Versioned evidence admission for Geo composition.
//!
//! Raw source observations do not become solver constraints directly. Each
//! observation names a rho contract that declares whether its relaxation is
//! logically sound or only empirically high-coverage. Only the former may
//! prune the residual; empirical observations remain diagnostic or soft.

use super::composition::{
    CANON_GEO_COMPOSITION_REQUEST_VERSION, GeoCompositionError, GeoCompositionRequest,
    GeoCompositionUniverse, GeoEntityLevel, GeoEntityRef, GeoHardConstraint, GeoHardConstraintKind,
    GeoIntegerMeasure, GeoIntegerMemberValue, GeoSoftPreference, canonicalize_composition_request,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_EVIDENCE_REQUEST_VERSION: &str = "canon_geo_evidence_request.v0";
pub const CANON_GEO_EVIDENCE_COMPILATION_VERSION: &str = "canon_geo_evidence_compilation.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoRhoSoundness {
    LogicallySound,
    EmpiricalHighCoverage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoEvidenceClaimRole {
    StableIdentityAnchor,
    TemporalOccupancy,
    LifecycleEvent,
    AttributeObservation,
}

/// Why a rho contract may make its declared soundness claim. Logical
/// relaxations name the invariant they preserve. Empirical bands name a
/// population, calibration artifact, and falsification procedure so they
/// cannot masquerade as theorem-backed pruning rules.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeoRhoBasis {
    LogicalRelaxation {
        invariant_id: String,
    },
    EmpiricalCalibration {
        population_id: String,
        calibration_blake3: String,
        falsification_rule_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoRhoContract {
    pub id: String,
    pub version: String,
    pub source_dataset: String,
    pub source_release: String,
    /// Canonical upstream lineage identifiers, sorted and distinct. Shared
    /// identifiers expose common provenance; their absence must never be read
    /// as proof of statistical independence.
    pub source_lineage_ids: Vec<String>,
    pub method_id: String,
    pub method_version: String,
    pub claim_role: GeoEvidenceClaimRole,
    pub basis: GeoRhoBasis,
}

impl GeoRhoContract {
    pub fn soundness(&self) -> GeoRhoSoundness {
        match self.basis {
            GeoRhoBasis::LogicalRelaxation { .. } => GeoRhoSoundness::LogicallySound,
            GeoRhoBasis::EmpiricalCalibration { .. } => GeoRhoSoundness::EmpiricalHighCoverage,
        }
    }
}

/// One immutable input record supporting an observation. `source_vintage` is
/// an exact source-native release/date token; temporal interpretation belongs
/// to an explicit later temporal contract rather than ambient parsing here.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoEvidenceRecordRef {
    pub source_record_id: String,
    pub source_vintage: String,
    pub record_blake3: String,
}

/// Closed interval in whole UTC days since 1970-01-01. Integer days avoid
/// locale/time-zone ambiguity while allowing deliberately wide intervals for
/// coarse source dates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoValidTimeInterval {
    pub start_day: i64,
    pub end_day: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoRhoObservation {
    pub id: String,
    pub contract_id: String,
    pub source_records: Vec<GeoEvidenceRecordRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_time: Option<GeoValidTimeInterval>,
    pub observation: GeoRhoObservationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeoRhoObservationKind {
    /// The selected members at one level equal one of these sets.
    ExactSets {
        level: GeoEntityLevel,
        sets: Vec<Vec<String>>,
    },
    /// At least one declared member participates in the selected composition.
    ExistentialMembership { members: Vec<GeoEntityRef> },
    /// Selected member values sum inside an exact integer band.
    IntegerSumBand {
        level: GeoEntityLevel,
        measure: GeoIntegerMeasure,
        values: Vec<GeoIntegerMemberValue>,
        min: u64,
        max: u64,
    },
    /// Presentation-only preference. It never becomes a hard constraint,
    /// regardless of the contract soundness classification.
    PreferMember {
        member: GeoEntityRef,
        cost_if_absent: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoEvidenceCompilationRequest {
    pub version: String,
    pub universe: GeoCompositionUniverse,
    pub contracts: Vec<GeoRhoContract>,
    pub observations: Vec<GeoRhoObservation>,
    pub max_assignments: u64,
    /// Passed through to the composition kernel's materialization budget.
    #[serde(default = "default_max_materialized_models")]
    pub max_materialized_models: u64,
}

fn default_max_materialized_models() -> u64 {
    super::composition::DEFAULT_MAX_MATERIALIZED_MODELS
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoEvidenceDisposition {
    HardConstraint,
    SoftPreference,
    DiagnosticOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoEvidenceAdmission {
    pub observation_id: String,
    pub contract: GeoRhoContract,
    pub source_records: Vec<GeoEvidenceRecordRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_time: Option<GeoValidTimeInterval>,
    pub observation: GeoRhoObservationKind,
    pub disposition: GeoEvidenceDisposition,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generated_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoEvidenceCompilationArtifact {
    pub version: String,
    pub request_version: String,
    pub composition_request: GeoCompositionRequest,
    pub admissions: Vec<GeoEvidenceAdmission>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoEvidenceErrorCode {
    UnsupportedVersion,
    InvalidInput,
    Composition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoEvidenceError {
    pub code: GeoEvidenceErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoEvidenceError {
    fn new(
        code: GeoEvidenceErrorCode,
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
        Self::new(GeoEvidenceErrorCode::InvalidInput, message, detail)
    }
}

impl From<GeoCompositionError> for GeoEvidenceError {
    fn from(error: GeoCompositionError) -> Self {
        let mut detail = error.detail;
        detail.insert("composition_code".to_string(), format!("{:?}", error.code));
        Self {
            code: GeoEvidenceErrorCode::Composition,
            message: error.message,
            detail,
        }
    }
}

impl fmt::Display for GeoEvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoEvidenceError {}

pub fn compile_evidence(
    request: &GeoEvidenceCompilationRequest,
) -> Result<GeoEvidenceCompilationArtifact, GeoEvidenceError> {
    if request.version != CANON_GEO_EVIDENCE_REQUEST_VERSION {
        return Err(GeoEvidenceError::new(
            GeoEvidenceErrorCode::UnsupportedVersion,
            "Unsupported Geo evidence request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_EVIDENCE_REQUEST_VERSION),
            ],
        ));
    }

    let mut contracts = request.contracts.clone();
    for contract in &contracts {
        validate_contract(contract)?;
    }
    contracts.sort_by(|left, right| left.id.cmp(&right.id));
    reject_duplicate_ids("contracts", contracts.iter().map(|contract| &contract.id))?;
    let contracts_by_id = contracts
        .iter()
        .map(|contract| (contract.id.as_str(), contract))
        .collect::<BTreeMap<_, _>>();

    let mut observations = request.observations.clone();
    for observation in &mut observations {
        validate_identifier("observations[].id", &observation.id)?;
        validate_identifier("observations[].contract_id", &observation.contract_id)?;
        if observation.source_records.is_empty() {
            return Err(GeoEvidenceError::invalid(
                "Geo observations require at least one immutable source record",
                [("observation_id", observation.id.as_str())],
            ));
        }
        for record in &observation.source_records {
            validate_identifier(
                "observations[].source_records[].source_record_id",
                &record.source_record_id,
            )?;
            validate_identifier(
                "observations[].source_records[].source_vintage",
                &record.source_vintage,
            )?;
            validate_blake3(
                "observations[].source_records[].record_blake3",
                &record.record_blake3,
            )?;
        }
        observation.source_records.sort();
        reject_duplicate_ids(
            "observations[].source_records",
            observation
                .source_records
                .iter()
                .map(|record| &record.source_record_id),
        )?;
        if let Some(interval) = observation.valid_time
            && interval.start_day > interval.end_day
        {
            return Err(GeoEvidenceError::invalid(
                "Geo observation valid-time intervals must be ordered",
                [("observation_id", observation.id.as_str())],
            ));
        }
    }
    observations.sort_by(|left, right| left.id.cmp(&right.id));
    reject_duplicate_ids(
        "observations",
        observations.iter().map(|observation| &observation.id),
    )?;

    let mut hard_constraints = Vec::new();
    let mut soft_preferences = Vec::new();
    let mut admissions = Vec::new();

    for observation in observations {
        let contract = contracts_by_id
            .get(observation.contract_id.as_str())
            .ok_or_else(|| {
                GeoEvidenceError::invalid(
                    "Geo observation references an unknown rho contract",
                    [
                        ("observation_id", observation.id.as_str()),
                        ("contract_id", observation.contract_id.as_str()),
                    ],
                )
            })?;
        if matches!(
            contract.claim_role,
            GeoEvidenceClaimRole::TemporalOccupancy | GeoEvidenceClaimRole::LifecycleEvent
        ) && observation.valid_time.is_none()
        {
            return Err(GeoEvidenceError::invalid(
                "Temporal occupancy and lifecycle evidence require explicit valid time",
                [
                    ("observation_id", observation.id.as_str()),
                    ("contract_id", contract.id.as_str()),
                ],
            ));
        }
        let generated_id = format!(
            "rho:{}@{}:{}",
            contract.id, contract.version, observation.id
        );
        let admitted_observation = observation.observation.clone();

        let temporal_claim_role = matches!(
            contract.claim_role,
            GeoEvidenceClaimRole::TemporalOccupancy | GeoEvidenceClaimRole::LifecycleEvent
        );
        let temporally_scoped = temporal_claim_role || observation.valid_time.is_some();
        let (disposition, generated_ids) = match observation.observation {
            kind if temporally_scoped => {
                // The interval is preserved in the admission artifact, but
                // v0 composition has no query-as-of domain. Applying this as
                // either hard or soft evidence would silently project a
                // time-bounded claim into timeless identity.
                let _ = kind;
                (GeoEvidenceDisposition::DiagnosticOnly, Vec::new())
            }
            GeoRhoObservationKind::PreferMember {
                member,
                cost_if_absent,
            } => {
                soft_preferences.push(GeoSoftPreference {
                    id: generated_id.clone(),
                    member,
                    cost_if_absent,
                });
                (GeoEvidenceDisposition::SoftPreference, vec![generated_id])
            }
            kind if contract.soundness() == GeoRhoSoundness::EmpiricalHighCoverage => {
                let _ = kind;
                (GeoEvidenceDisposition::DiagnosticOnly, Vec::new())
            }
            GeoRhoObservationKind::ExactSets { level, sets } => {
                hard_constraints.push(GeoHardConstraint {
                    id: generated_id.clone(),
                    constraint: GeoHardConstraintKind::AllowedSets { level, sets },
                });
                (GeoEvidenceDisposition::HardConstraint, vec![generated_id])
            }
            GeoRhoObservationKind::ExistentialMembership { members } => {
                hard_constraints.push(GeoHardConstraint {
                    id: generated_id.clone(),
                    constraint: GeoHardConstraintKind::AnyOf { members },
                });
                (GeoEvidenceDisposition::HardConstraint, vec![generated_id])
            }
            GeoRhoObservationKind::IntegerSumBand {
                level,
                measure,
                values,
                min,
                max,
            } => {
                hard_constraints.push(GeoHardConstraint {
                    id: generated_id.clone(),
                    constraint: GeoHardConstraintKind::IntegerSumBand {
                        level,
                        measure,
                        values,
                        min,
                        max,
                    },
                });
                (GeoEvidenceDisposition::HardConstraint, vec![generated_id])
            }
        };

        admissions.push(GeoEvidenceAdmission {
            observation_id: observation.id,
            contract: (*contract).clone(),
            source_records: observation.source_records,
            valid_time: observation.valid_time,
            observation: admitted_observation,
            disposition,
            generated_ids,
        });
    }

    let composition_request = canonicalize_composition_request(&GeoCompositionRequest {
        version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        universe: request.universe.clone(),
        hard_constraints,
        soft_preferences,
        max_assignments: request.max_assignments,
        max_materialized_models: request.max_materialized_models,
    })?;

    Ok(GeoEvidenceCompilationArtifact {
        version: CANON_GEO_EVIDENCE_COMPILATION_VERSION.to_string(),
        request_version: request.version.clone(),
        composition_request,
        admissions,
    })
}

pub fn canonical_evidence_compilation_bytes(
    artifact: &GeoEvidenceCompilationArtifact,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(artifact)
}

/// Validate that a deserialized compilation artifact is exactly shaped like
/// compiler output before it is accepted as provenance by `canon geo solve`.
///
/// This establishes canonical form and internal admission/constraint parity;
/// it does not establish that a source record or logical invariant is true.
pub fn validate_evidence_compilation_artifact(
    artifact: &GeoEvidenceCompilationArtifact,
) -> Result<(), GeoEvidenceError> {
    if artifact.version != CANON_GEO_EVIDENCE_COMPILATION_VERSION
        || artifact.request_version != CANON_GEO_EVIDENCE_REQUEST_VERSION
    {
        return Err(GeoEvidenceError::new(
            GeoEvidenceErrorCode::UnsupportedVersion,
            "Unsupported Geo evidence compilation artifact version",
            [
                ("actual_version", artifact.version.as_str()),
                ("actual_request_version", artifact.request_version.as_str()),
            ],
        ));
    }

    let canonical_request = canonicalize_composition_request(&artifact.composition_request)
        .map_err(GeoEvidenceError::from)?;
    if canonical_request != artifact.composition_request {
        return Err(GeoEvidenceError::invalid(
            "Geo evidence compilation contains a non-canonical composition request",
            [("field", "composition_request")],
        ));
    }

    let hard_ids = artifact
        .composition_request
        .hard_constraints
        .iter()
        .map(|constraint| constraint.id.as_str())
        .collect::<BTreeSet<_>>();
    let soft_ids = artifact
        .composition_request
        .soft_preferences
        .iter()
        .map(|preference| preference.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut generated_ids = BTreeSet::new();
    let mut contracts_by_id: BTreeMap<&str, &GeoRhoContract> = BTreeMap::new();
    let mut previous_observation: Option<&str> = None;

    for admission in &artifact.admissions {
        validate_identifier("admissions[].observation_id", &admission.observation_id)?;
        if previous_observation
            .is_some_and(|previous| previous >= admission.observation_id.as_str())
        {
            return Err(GeoEvidenceError::invalid(
                "Geo evidence compilation admissions must be strictly sorted by observation id",
                [("observation_id", admission.observation_id.as_str())],
            ));
        }
        previous_observation = Some(&admission.observation_id);
        validate_contract(&admission.contract)?;
        if let Some(previous) = contracts_by_id.insert(&admission.contract.id, &admission.contract)
            && previous != &admission.contract
        {
            return Err(GeoEvidenceError::invalid(
                "Geo evidence compilation redefines a rho contract id",
                [("contract_id", admission.contract.id.as_str())],
            ));
        }
        validate_source_records(&admission.observation_id, &admission.source_records)?;
        if let Some(interval) = admission.valid_time
            && interval.start_day > interval.end_day
        {
            return Err(GeoEvidenceError::invalid(
                "Geo evidence compilation contains an unordered valid-time interval",
                [("observation_id", admission.observation_id.as_str())],
            ));
        }
        let temporal_role = matches!(
            admission.contract.claim_role,
            GeoEvidenceClaimRole::TemporalOccupancy | GeoEvidenceClaimRole::LifecycleEvent
        );
        if temporal_role && admission.valid_time.is_none() {
            return Err(GeoEvidenceError::invalid(
                "Temporal evidence compilation admissions require explicit valid time",
                [("observation_id", admission.observation_id.as_str())],
            ));
        }
        let temporally_scoped = temporal_role || admission.valid_time.is_some();
        let expected_id = format!(
            "rho:{}@{}:{}",
            admission.contract.id, admission.contract.version, admission.observation_id
        );
        match admission.disposition {
            GeoEvidenceDisposition::DiagnosticOnly => {
                if !admission.generated_ids.is_empty() {
                    return Err(GeoEvidenceError::invalid(
                        "Diagnostic Geo evidence admissions cannot generate solver ids",
                        [("observation_id", admission.observation_id.as_str())],
                    ));
                }
            }
            GeoEvidenceDisposition::HardConstraint => {
                if temporally_scoped
                    || admission.contract.soundness() != GeoRhoSoundness::LogicallySound
                    || admission.generated_ids.as_slice() != std::slice::from_ref(&expected_id)
                    || !hard_ids.contains(expected_id.as_str())
                    || soft_ids.contains(expected_id.as_str())
                {
                    return Err(GeoEvidenceError::invalid(
                        "Geo hard admission does not match its rho contract and generated constraint",
                        [("observation_id", admission.observation_id.as_str())],
                    ));
                }
            }
            GeoEvidenceDisposition::SoftPreference => {
                if temporally_scoped
                    || admission.generated_ids.as_slice() != std::slice::from_ref(&expected_id)
                    || !soft_ids.contains(expected_id.as_str())
                    || hard_ids.contains(expected_id.as_str())
                {
                    return Err(GeoEvidenceError::invalid(
                        "Geo soft admission does not match its generated preference",
                        [("observation_id", admission.observation_id.as_str())],
                    ));
                }
            }
        }
        for generated_id in &admission.generated_ids {
            if !generated_ids.insert(generated_id.as_str()) {
                return Err(GeoEvidenceError::invalid(
                    "Geo evidence compilation reuses a generated solver id",
                    [("generated_id", generated_id.as_str())],
                ));
            }
        }
    }

    if hard_ids
        .iter()
        .chain(soft_ids.iter())
        .any(|id| !generated_ids.contains(id))
        || generated_ids
            .iter()
            .any(|id| !hard_ids.contains(id) && !soft_ids.contains(id))
    {
        return Err(GeoEvidenceError::invalid(
            "Geo evidence compilation solver ids are not in one-to-one admission parity",
            [("field", "composition_request")],
        ));
    }

    let reconstructed = GeoEvidenceCompilationRequest {
        version: artifact.request_version.clone(),
        universe: artifact.composition_request.universe.clone(),
        contracts: contracts_by_id
            .values()
            .map(|contract| (**contract).clone())
            .collect(),
        observations: artifact
            .admissions
            .iter()
            .map(|admission| GeoRhoObservation {
                id: admission.observation_id.clone(),
                contract_id: admission.contract.id.clone(),
                source_records: admission.source_records.clone(),
                valid_time: admission.valid_time,
                observation: admission.observation.clone(),
            })
            .collect(),
        max_assignments: artifact.composition_request.max_assignments,
        max_materialized_models: artifact.composition_request.max_materialized_models,
    };
    let recompiled = compile_evidence(&reconstructed)?;
    if recompiled != *artifact {
        return Err(GeoEvidenceError::invalid(
            "Geo evidence compilation does not replay from its admitted observations",
            [("field", "admissions")],
        ));
    }
    Ok(())
}

fn validate_contract(contract: &GeoRhoContract) -> Result<(), GeoEvidenceError> {
    validate_identifier("contracts[].id", &contract.id)?;
    validate_identifier("contracts[].version", &contract.version)?;
    validate_identifier("contracts[].source_dataset", &contract.source_dataset)?;
    validate_identifier("contracts[].source_release", &contract.source_release)?;
    if contract.source_lineage_ids.is_empty() {
        return Err(GeoEvidenceError::invalid(
            "Geo rho contracts require at least one upstream lineage id",
            [("contract_id", contract.id.as_str())],
        ));
    }
    let mut previous_lineage: Option<&str> = None;
    for lineage_id in &contract.source_lineage_ids {
        validate_identifier("contracts[].source_lineage_ids[]", lineage_id)?;
        if previous_lineage.is_some_and(|previous| previous >= lineage_id.as_str()) {
            return Err(GeoEvidenceError::invalid(
                "Geo rho contract lineage ids must be strictly sorted and distinct",
                [("contract_id", contract.id.as_str())],
            ));
        }
        previous_lineage = Some(lineage_id);
    }
    validate_identifier("contracts[].method_id", &contract.method_id)?;
    validate_identifier("contracts[].method_version", &contract.method_version)?;
    match &contract.basis {
        GeoRhoBasis::LogicalRelaxation { invariant_id } => {
            validate_identifier("contracts[].basis.invariant_id", invariant_id)?;
        }
        GeoRhoBasis::EmpiricalCalibration {
            population_id,
            calibration_blake3,
            falsification_rule_id,
        } => {
            validate_identifier("contracts[].basis.population_id", population_id)?;
            validate_blake3("contracts[].basis.calibration_blake3", calibration_blake3)?;
            validate_identifier(
                "contracts[].basis.falsification_rule_id",
                falsification_rule_id,
            )?;
        }
    }
    Ok(())
}

fn validate_source_records(
    observation_id: &str,
    source_records: &[GeoEvidenceRecordRef],
) -> Result<(), GeoEvidenceError> {
    if source_records.is_empty() {
        return Err(GeoEvidenceError::invalid(
            "Geo evidence compilation admissions require immutable source records",
            [("observation_id", observation_id)],
        ));
    }
    let mut previous_record: Option<&GeoEvidenceRecordRef> = None;
    let mut record_ids = BTreeSet::new();
    for record in source_records {
        validate_identifier(
            "admissions[].source_records[].source_record_id",
            &record.source_record_id,
        )?;
        validate_identifier(
            "admissions[].source_records[].source_vintage",
            &record.source_vintage,
        )?;
        validate_blake3(
            "admissions[].source_records[].record_blake3",
            &record.record_blake3,
        )?;
        if previous_record.is_some_and(|previous| previous >= record) {
            return Err(GeoEvidenceError::invalid(
                "Geo evidence compilation source records must be strictly sorted",
                [("observation_id", observation_id)],
            ));
        }
        previous_record = Some(record);
        if !record_ids.insert(record.source_record_id.as_str()) {
            return Err(GeoEvidenceError::invalid(
                "Geo evidence compilation repeats a source record id",
                [("source_record_id", record.source_record_id.as_str())],
            ));
        }
    }
    Ok(())
}

fn validate_identifier(field: &str, value: &str) -> Result<(), GeoEvidenceError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoEvidenceError::invalid(
            "Geo evidence identifiers must be non-empty and already canonical",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_blake3(field: &str, value: &str) -> Result<(), GeoEvidenceError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoEvidenceError::invalid(
            "Geo evidence BLAKE3 digests must be 64 lowercase hexadecimal characters",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn reject_duplicate_ids<'a>(
    field: &str,
    values: impl IntoIterator<Item = &'a String>,
) -> Result<(), GeoEvidenceError> {
    let mut previous: Option<&str> = None;
    for value in values {
        if previous == Some(value.as_str()) {
            return Err(GeoEvidenceError::invalid(
                "Geo evidence input contains a duplicate identifier",
                [("field", field), ("value", value.as_str())],
            ));
        }
        previous = Some(value);
    }
    Ok(())
}
