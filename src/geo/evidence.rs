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
    GeoIntegerMemberValue, GeoSoftPreference, canonicalize_composition_request,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fmt};

pub const CANON_GEO_EVIDENCE_REQUEST_VERSION: &str = "canon_geo_evidence_request.v0";
pub const CANON_GEO_EVIDENCE_COMPILATION_VERSION: &str = "canon_geo_evidence_compilation.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoRhoSoundness {
    LogicallySound,
    EmpiricalHighCoverage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoRhoContract {
    pub id: String,
    pub version: String,
    pub soundness: GeoRhoSoundness,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoRhoObservation {
    pub id: String,
    pub contract_id: String,
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
    pub contract_id: String,
    pub contract_version: String,
    pub soundness: GeoRhoSoundness,
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
        validate_identifier("contracts[].id", &contract.id)?;
        validate_identifier("contracts[].version", &contract.version)?;
    }
    contracts.sort_by(|left, right| left.id.cmp(&right.id));
    reject_duplicate_ids("contracts", contracts.iter().map(|contract| &contract.id))?;
    let contracts_by_id = contracts
        .iter()
        .map(|contract| (contract.id.as_str(), contract))
        .collect::<BTreeMap<_, _>>();

    let mut observations = request.observations.clone();
    for observation in &observations {
        validate_identifier("observations[].id", &observation.id)?;
        validate_identifier("observations[].contract_id", &observation.contract_id)?;
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
        let generated_id = format!(
            "rho:{}@{}:{}",
            contract.id, contract.version, observation.id
        );

        let (disposition, generated_ids) = match observation.observation {
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
            kind if contract.soundness == GeoRhoSoundness::EmpiricalHighCoverage => {
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
                values,
                min,
                max,
            } => {
                hard_constraints.push(GeoHardConstraint {
                    id: generated_id.clone(),
                    constraint: GeoHardConstraintKind::IntegerSumBand {
                        level,
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
            contract_id: contract.id.clone(),
            contract_version: contract.version.clone(),
            soundness: contract.soundness,
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

fn validate_identifier(field: &str, value: &str) -> Result<(), GeoEvidenceError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoEvidenceError::invalid(
            "Geo evidence identifiers must be non-empty and already canonical",
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
