#![forbid(unsafe_code)]

//! Offline observer-lane contracts.
//!
//! Network acquisition and model execution stay outside Canon's deterministic
//! runtime. This module only validates retained pins, declared populations, and
//! deterministic selection inputs that later observer artifacts bind by digest.

use super::{
    composition::{
        GeoBuildingCandidate, GeoCompositionUniverse, GeoEntityLevel, GeoEntityRef,
        GeoIntegerMeasure, GeoIntegerMemberValue, GeoIntegerValueOrigin,
    },
    evaluation::GeoTruthPlane,
    evidence::{
        GeoEvidenceRecordRef, GeoRhoContract, GeoRhoObservation, GeoRhoObservationKind,
        GeoValidTimeInterval,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_IMAGE_TILE_PIN_VERSION: &str = "canon_geo_image_tile_pin.v0";
pub const CANON_GEO_OBSERVER_VERSION: &str = "canon_geo_observer.v0";
pub const CANON_GEO_OBSERVER_ADMISSION_REQUEST_VERSION: &str =
    "canon_geo_observer_admission_request.v0";
pub const CANON_GEO_OBSERVATION_ROWS_VERSION: &str = "canon_geo_observation_rows.v0";
pub const CANON_GEO_ERROR_POPULATION_VERSION: &str = "canon_geo_error_population.v0";

const SPLITMIX64_INCREMENT: u64 = 0x9E37_79B9_7F4A_7C15;
const SPLITMIX64_MULTIPLIER_1: u64 = 0xBF58_476D_1CE4_E5B9;
const SPLITMIX64_MULTIPLIER_2: u64 = 0x94D0_49BB_1331_11EB;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoImageTilePin {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_range: Option<(u64, u64)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    pub blake3: String,
    pub vintage: GeoValidTimeInterval,
    pub license_id: String,
    pub license_text_blake3: String,
    pub source_dataset: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoImageTilePinArtifact {
    pub version: String,
    pub source_profile_id: String,
    pub rows: Vec<GeoImageTilePin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeoObserverIdentity {
    RuleBased {
        rule_id: String,
        rule_version: String,
    },
    FrozenWeight {
        model_id: String,
        weight_blake3: String,
        arithmetic_contract: String,
    },
    RecordedHosted {
        model_id: String,
        model_version: String,
        prompt_blake3: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoObservationKind {
    StructureCountInWindow,
    FootprintOutline,
    HeightOrFloors,
    PresentAtVintage,
    AbsentAtVintage,
    ChangeEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoObserverContract {
    pub id: String,
    pub version: String,
    pub identity: GeoObserverIdentity,
    pub output_kinds: Vec<GeoObservationKind>,
    pub error_population_id: String,
    pub characterization_blake3: String,
    pub rho_contract_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeoObservationPayload {
    StructureCountInWindow {
        min: u64,
        max: u64,
    },
    FootprintOutline {
        ring_blake3: String,
    },
    HeightOrFloors {
        min: u64,
        max: u64,
    },
    PresentAtVintage {
        interval: GeoValidTimeInterval,
    },
    AbsentAtVintage {
        interval: GeoValidTimeInterval,
    },
    ChangeEvent {
        before: GeoValidTimeInterval,
        after: GeoValidTimeInterval,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoObservationRow {
    pub id: String,
    pub observer_id: String,
    pub tile_pins: Vec<GeoImageTilePin>,
    pub window_blake3: String,
    pub kind: GeoObservationKind,
    pub payload: GeoObservationPayload,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_count: Option<u64>,
    pub crop_blake3: String,
    pub label_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoObservationRowsArtifact {
    pub version: String,
    pub contract: GeoObserverContract,
    pub rows: Vec<GeoObservationRow>,
    pub rho_observations: Vec<GeoRhoObservation>,
    pub diagnostic_only_ids: Vec<String>,
    pub not_admitted_ids: Vec<String>,
    #[serde(default)]
    pub row_blake3s: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoObserverAdmissionRequest {
    pub version: String,
    pub contract: GeoObserverContract,
    pub rows: Vec<GeoObservationRow>,
    pub rho_contracts: Vec<GeoRhoContract>,
    pub forbidden_license_ids: Vec<String>,
    pub universe: GeoCompositionUniverse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoErrorPopulationSubject {
    pub subject_id: String,
    pub truth_plane: GeoTruthPlane,
    pub window_blake3: String,
    pub parcel_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoErrorPopulationArtifact {
    pub version: String,
    pub population_id: String,
    pub region: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_query_blake3: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_population_blake3: Option<String>,
    pub subjects: Vec<GeoErrorPopulationSubject>,
    pub declared_before_observer_ids: Vec<String>,
    pub stratum_counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoObserverErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
    ObserverNullRingMismatch,
    ObserverNotRedundant,
    ObserverBandNotFromCharacterization,
    ObserverEffectWidened,
    ObserverRuntimeInvoked,
    ObserverMissingProvenance,
    ObserverErrorUncharacterized,
    ObserverLicenseForbidden,
    ImageTileDigestMismatch,
    ObservationRegeneratedAtReplay,
    ObservationTemporalDiagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoObserverError {
    pub code: GeoObserverErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoObserverError {
    fn new(
        code: GeoObserverErrorCode,
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
        Self::new(GeoObserverErrorCode::InvalidInput, message, detail)
    }

    fn invalid_field(
        field: &'static str,
        message: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self::invalid(
            message,
            [("field", field.to_string()), ("value", value.into())],
        )
    }

    fn missing_provenance_field(
        field: &'static str,
        message: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self::new(
            GeoObserverErrorCode::ObserverMissingProvenance,
            message,
            [
                ("field".to_string(), field.to_string()),
                ("value".to_string(), value.into()),
            ],
        )
    }

    fn uncharacterized_field(
        field: &'static str,
        message: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self::new(
            GeoObserverErrorCode::ObserverErrorUncharacterized,
            message,
            [
                ("field".to_string(), field.to_string()),
                ("value".to_string(), value.into()),
            ],
        )
    }

    fn forbidden_license(
        field: &'static str,
        message: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self::new(
            GeoObserverErrorCode::ObserverLicenseForbidden,
            message,
            [
                ("field".to_string(), field.to_string()),
                ("value".to_string(), value.into()),
            ],
        )
    }

    fn digest_mismatch(
        digest_field: &'static str,
        row_id: impl Into<String>,
        expected: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        Self::new(
            GeoObserverErrorCode::ImageTileDigestMismatch,
            "Geo observer replay bytes do not match the stored digest",
            [
                (digest_field.to_string(), expected.into()),
                ("row_id".to_string(), row_id.into()),
                ("actual".to_string(), actual.into()),
            ],
        )
    }

    fn regenerated(row_id: impl Into<String>) -> Self {
        Self::new(
            GeoObserverErrorCode::ObservationRegeneratedAtReplay,
            "Geo observer replay cannot accept a regenerated observation row",
            [("observation_id".to_string(), row_id.into())],
        )
    }
}

impl fmt::Display for GeoObserverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoObserverError {}

pub fn validate_image_tile_pin_artifact(
    artifact: &GeoImageTilePinArtifact,
) -> Result<(), GeoObserverError> {
    if artifact.version != CANON_GEO_IMAGE_TILE_PIN_VERSION {
        return Err(GeoObserverError::new(
            GeoObserverErrorCode::UnsupportedVersion,
            "Unsupported Geo image tile pin artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_IMAGE_TILE_PIN_VERSION),
            ],
        ));
    }
    validate_canonical_string("source_profile_id", &artifact.source_profile_id)?;
    if artifact.rows.is_empty() {
        return Err(GeoObserverError::invalid_field(
            "rows",
            "Geo image tile pin artifacts require at least one row",
            "0",
        ));
    }
    for pin in &artifact.rows {
        validate_image_tile_pin(pin, &[])?;
    }
    Ok(())
}

pub fn canonical_image_tile_pin_bytes(
    artifact: &GeoImageTilePinArtifact,
) -> Result<Vec<u8>, GeoObserverError> {
    validate_image_tile_pin_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoObserverError::invalid(
            "Geo image tile pin artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub fn verify_image_tile_pin_replay(
    artifact: &GeoImageTilePinArtifact,
    bytes_by_blake3: &BTreeMap<String, Vec<u8>>,
) -> Result<(), GeoObserverError> {
    validate_image_tile_pin_artifact(artifact)?;
    for (index, pin) in artifact.rows.iter().enumerate() {
        verify_replay_digest(
            "tile",
            &format!("{}:{index}", pin.source_dataset),
            &pin.blake3,
            bytes_by_blake3,
        )?;
    }
    Ok(())
}

pub fn validate_observer_contract(contract: &GeoObserverContract) -> Result<(), GeoObserverError> {
    if contract.version != CANON_GEO_OBSERVER_VERSION {
        return Err(GeoObserverError::new(
            GeoObserverErrorCode::UnsupportedVersion,
            "Unsupported Geo observer contract version",
            [
                ("actual", contract.version.as_str()),
                ("expected", CANON_GEO_OBSERVER_VERSION),
            ],
        ));
    }
    validate_required_observer_string("id", &contract.id)?;
    validate_observer_identity(&contract.identity)?;
    validate_output_kinds(&contract.output_kinds)?;
    validate_required_observer_string("error_population_id", &contract.error_population_id)
        .map_err(|_| {
            GeoObserverError::uncharacterized_field(
                "error_population_id",
                "Geo observer contracts require a named characterized error population",
                contract.error_population_id.clone(),
            )
        })?;
    validate_blake3("characterization_blake3", &contract.characterization_blake3).map_err(
        |_| {
            GeoObserverError::uncharacterized_field(
                "characterization_blake3",
                "Geo observer contracts require a characterized error digest",
                contract.characterization_blake3.clone(),
            )
        },
    )?;
    validate_rho_contract_ids(&contract.rho_contract_ids)?;
    Ok(())
}

pub fn canonical_observer_bytes(
    contract: &GeoObserverContract,
) -> Result<Vec<u8>, GeoObserverError> {
    validate_observer_contract(contract)?;
    serde_json::to_vec(contract).map_err(|error| {
        GeoObserverError::invalid(
            "Geo observer contract could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub fn validate_observer_admission_request(
    request: &GeoObserverAdmissionRequest,
) -> Result<(), GeoObserverError> {
    canonicalize_observer_admission_request(request).map(|_| ())
}

pub fn canonical_observer_admission_request_bytes(
    request: &GeoObserverAdmissionRequest,
) -> Result<Vec<u8>, GeoObserverError> {
    let canonical = canonicalize_observer_admission_request(request)?;
    serde_json::to_vec(&canonical).map_err(|error| {
        GeoObserverError::invalid(
            "Geo observer admission request could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub fn admit_observer_request(
    request: &GeoObserverAdmissionRequest,
) -> Result<GeoObservationRowsArtifact, GeoObserverError> {
    let canonical = canonicalize_observer_admission_request(request)?;
    admit_observations_with_universe(
        &canonical.contract,
        &canonical.rows,
        &canonical.rho_contracts,
        &canonical.forbidden_license_ids,
        &canonical.universe,
    )
}

pub fn admit_observations(
    contract: &GeoObserverContract,
    rows: &[GeoObservationRow],
    rho: &[GeoRhoContract],
    forbidden_license_ids: &[String],
) -> Result<GeoObservationRowsArtifact, GeoObserverError> {
    admit_observations_with_universe(
        contract,
        rows,
        rho,
        forbidden_license_ids,
        &GeoCompositionUniverse {
            parcels: Vec::new(),
            buildings: Vec::new(),
        },
    )
}

pub fn admit_observations_with_universe(
    contract: &GeoObserverContract,
    rows: &[GeoObservationRow],
    rho: &[GeoRhoContract],
    forbidden_license_ids: &[String],
    universe: &GeoCompositionUniverse,
) -> Result<GeoObservationRowsArtifact, GeoObserverError> {
    validate_observer_contract(contract)?;
    validate_forbidden_license_ids(forbidden_license_ids)?;
    validate_rho_contracts_for_observer(contract, rho)?;

    let mut rows = rows.to_vec();
    rows.sort_by(|left, right| left.id.cmp(&right.id));
    validate_observation_rows(contract, &rows, forbidden_license_ids)?;

    let mut rho_observations = Vec::new();
    let mut diagnostic_only_ids = Vec::new();
    let mut not_admitted_ids = Vec::new();
    let mut row_blake3s = BTreeMap::new();
    for row in &rows {
        row_blake3s.insert(row.id.clone(), observation_row_blake3(row)?);
        if is_temporal_kind(row.kind) {
            diagnostic_only_ids.push(row.id.clone());
        }
        match to_rho_observation(row, contract, universe) {
            Some(observation) => rho_observations.push(observation),
            None => not_admitted_ids.push(row.id.clone()),
        }
    }
    rho_observations.sort_by(|left, right| left.id.cmp(&right.id));
    diagnostic_only_ids.sort();
    not_admitted_ids.sort();

    let artifact = GeoObservationRowsArtifact {
        version: CANON_GEO_OBSERVATION_ROWS_VERSION.to_string(),
        contract: contract.clone(),
        rows,
        rho_observations,
        diagnostic_only_ids,
        not_admitted_ids,
        row_blake3s,
    };
    validate_observation_rows_artifact(&artifact)?;
    Ok(artifact)
}

pub fn to_rho_observation(
    row: &GeoObservationRow,
    contract: &GeoObserverContract,
    universe: &GeoCompositionUniverse,
) -> Option<GeoRhoObservation> {
    let contract_id = contract.rho_contract_ids.first()?.clone();
    let source_records = observation_source_records(row);
    match &row.payload {
        GeoObservationPayload::StructureCountInWindow { min, max } => Some(GeoRhoObservation {
            id: row.id.clone(),
            contract_id,
            source_records,
            valid_time: None,
            observation: GeoRhoObservationKind::IntegerSumBand {
                level: GeoEntityLevel::Building,
                measure: GeoIntegerMeasure {
                    semantic_id: "observer.structure_count_in_window".to_string(),
                    unit: "structure".to_string(),
                    value_origin: GeoIntegerValueOrigin::SourceAsserted,
                },
                values: building_unit_values(&universe.buildings),
                min: *min,
                max: *max,
            },
        }),
        GeoObservationPayload::HeightOrFloors { min, max } => Some(GeoRhoObservation {
            id: row.id.clone(),
            contract_id,
            source_records,
            valid_time: None,
            observation: GeoRhoObservationKind::IntegerSumBand {
                level: GeoEntityLevel::Building,
                measure: GeoIntegerMeasure {
                    semantic_id: "observer.height_or_floors".to_string(),
                    unit: "floor".to_string(),
                    value_origin: GeoIntegerValueOrigin::SourceAsserted,
                },
                values: building_unit_values(&universe.buildings),
                min: *min,
                max: *max,
            },
        }),
        GeoObservationPayload::PresentAtVintage { interval }
        | GeoObservationPayload::AbsentAtVintage { interval } => Some(GeoRhoObservation {
            id: row.id.clone(),
            contract_id,
            source_records,
            valid_time: Some(*interval),
            observation: GeoRhoObservationKind::ExistentialMembership {
                members: building_members(&universe.buildings),
            },
        }),
        GeoObservationPayload::FootprintOutline { .. }
        | GeoObservationPayload::ChangeEvent { .. } => None,
    }
}

pub fn validate_observation_rows_artifact(
    artifact: &GeoObservationRowsArtifact,
) -> Result<(), GeoObserverError> {
    if artifact.version != CANON_GEO_OBSERVATION_ROWS_VERSION {
        return Err(GeoObserverError::new(
            GeoObserverErrorCode::UnsupportedVersion,
            "Unsupported Geo observation rows artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_OBSERVATION_ROWS_VERSION),
            ],
        ));
    }
    validate_observer_contract(&artifact.contract)?;
    validate_observation_rows(&artifact.contract, &artifact.rows, &[])?;
    validate_row_blake3s(&artifact.rows, &artifact.row_blake3s)?;
    validate_id_list(
        "diagnostic_only_ids",
        &artifact.diagnostic_only_ids,
        &artifact.rows,
    )?;
    validate_id_list(
        "not_admitted_ids",
        &artifact.not_admitted_ids,
        &artifact.rows,
    )?;
    validate_rho_observation_rows(artifact)?;
    Ok(())
}

pub fn canonical_observation_rows_bytes(
    artifact: &GeoObservationRowsArtifact,
) -> Result<Vec<u8>, GeoObserverError> {
    validate_observation_rows_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoObserverError::invalid(
            "Geo observation rows artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub fn verify_replay(
    artifact: &GeoObservationRowsArtifact,
    bytes_by_blake3: &BTreeMap<String, Vec<u8>>,
) -> Result<(), GeoObserverError> {
    validate_observation_rows_artifact(artifact)?;
    for row in &artifact.rows {
        for pin in &row.tile_pins {
            verify_replay_digest("tile", &row.id, &pin.blake3, bytes_by_blake3)?;
        }
        verify_replay_digest("crop", &row.id, &row.crop_blake3, bytes_by_blake3)?;
        verify_replay_digest("label", &row.id, &row.label_blake3, bytes_by_blake3)?;
    }
    Ok(())
}

pub fn observation_row_blake3(row: &GeoObservationRow) -> Result<String, GeoObserverError> {
    serde_json::to_vec(row)
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
        .map_err(|error| {
            GeoObserverError::invalid(
                "Geo observation row could not be serialized",
                [("error", error.to_string())],
            )
        })
}

pub fn validate_error_population_artifact(
    artifact: &GeoErrorPopulationArtifact,
) -> Result<(), GeoObserverError> {
    if artifact.version != CANON_GEO_ERROR_POPULATION_VERSION {
        return Err(GeoObserverError::new(
            GeoObserverErrorCode::UnsupportedVersion,
            "Unsupported Geo error-population artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_ERROR_POPULATION_VERSION),
            ],
        ));
    }

    validate_canonical_string("population_id", &artifact.population_id)?;
    validate_canonical_string("region", &artifact.region)?;
    match artifact.selection_seed {
        Some(seed) if seed != 0 => {}
        Some(_) => {
            return Err(GeoObserverError::invalid_field(
                "selection_seed",
                "Geo error-population selection_seed must be present and nonzero",
                "0",
            ));
        }
        None => {
            return Err(GeoObserverError::invalid_field(
                "selection_seed",
                "Geo error-population selection_seed must be present and nonzero",
                "<missing>",
            ));
        }
    }
    validate_required_blake3(
        "selection_query_blake3",
        artifact.selection_query_blake3.as_deref(),
    )?;
    validate_required_blake3(
        "source_population_blake3",
        artifact.source_population_blake3.as_deref(),
    )?;
    validate_declared_before_observer_ids(&artifact.declared_before_observer_ids)?;
    validate_error_population_subjects(&artifact.subjects)?;
    validate_stratum_counts(&artifact.subjects, &artifact.stratum_counts)?;
    Ok(())
}

pub fn canonical_error_population_bytes(
    artifact: &GeoErrorPopulationArtifact,
) -> Result<Vec<u8>, GeoObserverError> {
    validate_error_population_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoObserverError::invalid(
            "Geo error-population artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub fn select_error_population_subjects(
    candidates: &[GeoErrorPopulationSubject],
    seed: u64,
    counts_by_truth_plane: &BTreeMap<GeoTruthPlane, usize>,
) -> Result<Vec<GeoErrorPopulationSubject>, GeoObserverError> {
    if seed == 0 {
        return Err(GeoObserverError::invalid_field(
            "selection_seed",
            "Geo error-population selection_seed must be present and nonzero",
            "0",
        ));
    }
    if counts_by_truth_plane.is_empty() {
        return Err(GeoObserverError::invalid_field(
            "stratum_counts",
            "Geo error-population selection requires at least one stratum",
            "0",
        ));
    }

    let mut grouped = BTreeMap::<GeoTruthPlane, Vec<&GeoErrorPopulationSubject>>::new();
    for subject in candidates {
        grouped
            .entry(subject.truth_plane)
            .or_default()
            .push(subject);
    }

    let mut selected = Vec::new();
    for (truth_plane, requested_count) in counts_by_truth_plane {
        if *requested_count == 0 {
            return Err(GeoObserverError::invalid_field(
                "stratum_counts",
                "Geo error-population stratum counts must be positive",
                "0",
            ));
        }
        let Some(subjects) = grouped.get_mut(truth_plane) else {
            return Err(GeoObserverError::invalid_field(
                "subjects",
                "Geo error-population source candidates are missing a requested stratum",
                truth_plane_key(*truth_plane),
            ));
        };
        subjects.sort_by(|left, right| left.subject_id.cmp(&right.subject_id));
        if subjects.len() < *requested_count {
            return Err(GeoObserverError::invalid(
                "Geo error-population source candidates cannot satisfy the requested stratum count",
                [
                    ("field", "stratum_counts".to_string()),
                    ("truth_plane", truth_plane_key(*truth_plane).to_string()),
                    ("available", subjects.len().to_string()),
                    ("requested", requested_count.to_string()),
                ],
            ));
        }

        let mut ranked = subjects
            .iter()
            .enumerate()
            .map(|(index, subject)| {
                (
                    splitmix64(seed.wrapping_add(index as u64)),
                    subject.subject_id.as_str(),
                    *subject,
                )
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| (left.0, left.1).cmp(&(right.0, right.1)));
        selected.extend(
            ranked
                .into_iter()
                .take(*requested_count)
                .map(|(_, _, subject)| subject.clone()),
        );
    }

    selected.sort_by(|left, right| left.subject_id.cmp(&right.subject_id));
    Ok(selected)
}

fn canonicalize_observer_admission_request(
    request: &GeoObserverAdmissionRequest,
) -> Result<GeoObserverAdmissionRequest, GeoObserverError> {
    if request.version != CANON_GEO_OBSERVER_ADMISSION_REQUEST_VERSION {
        return Err(GeoObserverError::new(
            GeoObserverErrorCode::UnsupportedVersion,
            "Unsupported Geo observer admission request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_OBSERVER_ADMISSION_REQUEST_VERSION),
            ],
        ));
    }
    let mut canonical = request.clone();
    canonical.contract.output_kinds.sort();
    canonical.contract.rho_contract_ids =
        canonicalize_sorted_strings("rho_contract_ids", &canonical.contract.rho_contract_ids)?;
    canonical.forbidden_license_ids =
        canonicalize_sorted_strings("forbidden_license_ids", &request.forbidden_license_ids)?;
    canonical
        .rho_contracts
        .sort_by(|left, right| left.id.cmp(&right.id));
    for rho_contract in &mut canonical.rho_contracts {
        rho_contract.source_lineage_ids = canonicalize_sorted_strings(
            "rho_contracts[].source_lineage_ids",
            &rho_contract.source_lineage_ids,
        )?;
        if rho_contract.source_lineage_ids.is_empty() {
            return Err(GeoObserverError::invalid_field(
                "rho_contracts[].source_lineage_ids",
                "Geo observer admission rho contracts require at least one source lineage id",
                "0",
            ));
        }
    }
    reject_duplicate_sorted_values(
        "rho_contracts",
        canonical
            .rho_contracts
            .iter()
            .map(|contract| contract.id.as_str()),
    )?;
    canonical.universe = canonicalize_observer_universe(&request.universe)?;
    canonical.rows.sort_by(|left, right| left.id.cmp(&right.id));

    validate_observer_contract(&canonical.contract)?;
    validate_forbidden_license_ids(&canonical.forbidden_license_ids)?;
    validate_rho_contracts_for_observer(&canonical.contract, &canonical.rho_contracts)?;
    validate_observation_rows(
        &canonical.contract,
        &canonical.rows,
        &canonical.forbidden_license_ids,
    )?;
    Ok(canonical)
}

fn canonicalize_observer_universe(
    universe: &GeoCompositionUniverse,
) -> Result<GeoCompositionUniverse, GeoObserverError> {
    let parcels = canonicalize_sorted_strings("universe.parcels", &universe.parcels)?;
    let parcel_set = parcels.iter().cloned().collect::<BTreeSet<_>>();
    let mut buildings = universe.buildings.clone();
    for building in &mut buildings {
        validate_canonical_string("universe.buildings[].id", &building.id)?;
        building.parcel_ids =
            canonicalize_sorted_strings("universe.buildings[].parcel_ids", &building.parcel_ids)?;
        for parcel_id in &building.parcel_ids {
            if !parcel_set.contains(parcel_id) {
                return Err(GeoObserverError::invalid(
                    "Geo observer admission universe building parcel ids must reference declared parcels",
                    [
                        (
                            "field".to_string(),
                            "universe.buildings[].parcel_ids".to_string(),
                        ),
                        ("building_id".to_string(), building.id.clone()),
                        ("parcel_id".to_string(), parcel_id.clone()),
                    ],
                ));
            }
        }
    }
    buildings.sort_by(|left, right| left.id.cmp(&right.id));
    reject_duplicate_sorted_values(
        "universe.buildings",
        buildings.iter().map(|building| building.id.as_str()),
    )?;
    Ok(GeoCompositionUniverse { parcels, buildings })
}

fn canonicalize_sorted_strings(
    field: &'static str,
    values: &[String],
) -> Result<Vec<String>, GeoObserverError> {
    let mut sorted = values.to_vec();
    for value in &sorted {
        validate_canonical_string(field, value)?;
    }
    sorted.sort();
    reject_duplicate_sorted_values(field, sorted.iter().map(String::as_str))?;
    Ok(sorted)
}

fn reject_duplicate_sorted_values<'a>(
    field: &'static str,
    values: impl IntoIterator<Item = &'a str>,
) -> Result<(), GeoObserverError> {
    let mut previous: Option<&str> = None;
    for value in values {
        if previous == Some(value) {
            return Err(GeoObserverError::invalid(
                "Geo observer admission request values must be unique after canonical sorting",
                [("field", field), ("value", value)],
            ));
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_image_tile_pin(
    pin: &GeoImageTilePin,
    forbidden_license_ids: &[String],
) -> Result<(), GeoObserverError> {
    validate_required_observer_string("url", &pin.url)?;
    if let Some((start, end)) = pin.byte_range
        && start > end
    {
        return Err(GeoObserverError::invalid(
            "Geo image tile pin byte ranges must be ordered",
            [
                ("field".to_string(), "byte_range".to_string()),
                ("start".to_string(), start.to_string()),
                ("end".to_string(), end.to_string()),
            ],
        ));
    }
    if let Some(etag) = &pin.etag {
        validate_required_observer_string("etag", etag)?;
    }
    validate_blake3("blake3", &pin.blake3).map_err(|_| {
        GeoObserverError::missing_provenance_field(
            "blake3",
            "Geo image tile pins require a tile content digest",
            pin.blake3.clone(),
        )
    })?;
    validate_interval("vintage", pin.vintage)?;
    validate_required_observer_string("license_id", &pin.license_id)?;
    if forbidden_license_ids
        .binary_search_by(|probe| probe.as_str().cmp(pin.license_id.as_str()))
        .is_ok()
    {
        return Err(GeoObserverError::forbidden_license(
            "license_id",
            "Geo image tile pin license is forbidden by the caller policy",
            pin.license_id.clone(),
        ));
    }
    validate_blake3("license_text_blake3", &pin.license_text_blake3).map_err(|_| {
        GeoObserverError::forbidden_license(
            "license_text_blake3",
            "Geo image tile pins require a license text digest",
            pin.license_text_blake3.clone(),
        )
    })?;
    validate_required_observer_string("source_dataset", &pin.source_dataset)?;
    Ok(())
}

fn validate_observer_identity(identity: &GeoObserverIdentity) -> Result<(), GeoObserverError> {
    match identity {
        GeoObserverIdentity::RuleBased {
            rule_id,
            rule_version,
        } => {
            validate_required_observer_string("identity.rule_id", rule_id)?;
            validate_required_observer_string("identity.rule_version", rule_version)?;
        }
        GeoObserverIdentity::FrozenWeight {
            model_id,
            weight_blake3,
            arithmetic_contract,
        } => {
            validate_required_observer_string("identity.model_id", model_id)?;
            validate_blake3("identity.weight_blake3", weight_blake3).map_err(|_| {
                GeoObserverError::missing_provenance_field(
                    "identity.weight_blake3",
                    "Geo frozen-weight observers require a pinned weight digest",
                    weight_blake3.clone(),
                )
            })?;
            validate_required_observer_string("identity.arithmetic_contract", arithmetic_contract)?;
        }
        GeoObserverIdentity::RecordedHosted {
            model_id,
            model_version,
            prompt_blake3,
        } => {
            validate_required_observer_string("identity.model_id", model_id)?;
            validate_required_observer_string("identity.model_version", model_version)?;
            validate_blake3("identity.prompt_blake3", prompt_blake3).map_err(|_| {
                GeoObserverError::missing_provenance_field(
                    "identity.prompt_blake3",
                    "Geo recorded-hosted observers require a pinned prompt digest",
                    prompt_blake3.clone(),
                )
            })?;
        }
    }
    Ok(())
}

fn validate_output_kinds(kinds: &[GeoObservationKind]) -> Result<(), GeoObserverError> {
    if kinds.is_empty() {
        return Err(GeoObserverError::uncharacterized_field(
            "output_kinds",
            "Geo observer contracts require at least one output kind",
            "0",
        ));
    }
    let mut previous = None;
    for kind in kinds {
        if previous.is_some_and(|previous| previous >= *kind) {
            return Err(GeoObserverError::uncharacterized_field(
                "output_kinds",
                "Geo observer output kinds must be sorted and unique",
                format!("{kind:?}"),
            ));
        }
        previous = Some(*kind);
    }
    Ok(())
}

fn validate_rho_contract_ids(ids: &[String]) -> Result<(), GeoObserverError> {
    if ids.is_empty() {
        return Err(GeoObserverError::uncharacterized_field(
            "rho_contract_ids",
            "Geo observer contracts require at least one rho contract id",
            "0",
        ));
    }
    let mut previous: Option<&str> = None;
    for id in ids {
        validate_required_observer_string("rho_contract_ids[]", id)?;
        if previous.is_some_and(|previous| previous >= id.as_str()) {
            return Err(GeoObserverError::uncharacterized_field(
                "rho_contract_ids",
                "Geo observer rho contract ids must be sorted and unique",
                id.clone(),
            ));
        }
        previous = Some(id.as_str());
    }
    Ok(())
}

fn validate_forbidden_license_ids(ids: &[String]) -> Result<(), GeoObserverError> {
    if ids.is_empty() {
        return Err(GeoObserverError::invalid_field(
            "forbidden_license_ids",
            "Geo observer admission requires a non-empty forbidden license policy",
            "0",
        ));
    }
    let mut previous: Option<&str> = None;
    for id in ids {
        validate_canonical_string("forbidden_license_ids[]", id)?;
        if previous.is_some_and(|previous| previous >= id.as_str()) {
            return Err(GeoObserverError::invalid(
                "Geo observer forbidden license ids must be sorted and unique",
                [("field", "forbidden_license_ids")],
            ));
        }
        previous = Some(id.as_str());
    }
    Ok(())
}

fn validate_rho_contracts_for_observer(
    contract: &GeoObserverContract,
    rho: &[GeoRhoContract],
) -> Result<(), GeoObserverError> {
    let mut available = BTreeSet::new();
    for rho_contract in rho {
        validate_required_observer_string("rho[].id", &rho_contract.id)?;
        if !available.insert(rho_contract.id.as_str()) {
            return Err(GeoObserverError::uncharacterized_field(
                "rho[].id",
                "Geo observer admission rho contracts must be unique",
                rho_contract.id.clone(),
            ));
        }
    }
    for id in &contract.rho_contract_ids {
        if !available.contains(id.as_str()) {
            return Err(GeoObserverError::uncharacterized_field(
                "rho_contract_ids",
                "Geo observer contract references an unavailable rho contract",
                id.clone(),
            ));
        }
    }
    Ok(())
}

fn validate_observation_rows(
    contract: &GeoObserverContract,
    rows: &[GeoObservationRow],
    forbidden_license_ids: &[String],
) -> Result<(), GeoObserverError> {
    if rows.is_empty() {
        return Err(GeoObserverError::missing_provenance_field(
            "rows",
            "Geo observation artifacts require at least one row",
            "0",
        ));
    }
    let mut previous: Option<&str> = None;
    for row in rows {
        validate_observation_row(contract, row, forbidden_license_ids)?;
        if previous.is_some_and(|previous| previous >= row.id.as_str()) {
            return Err(GeoObserverError::invalid(
                "Geo observation rows must be sorted and unique",
                [
                    ("field".to_string(), "rows".to_string()),
                    ("id".to_string(), row.id.clone()),
                ],
            ));
        }
        previous = Some(row.id.as_str());
    }
    Ok(())
}

fn validate_observation_row(
    contract: &GeoObserverContract,
    row: &GeoObservationRow,
    forbidden_license_ids: &[String],
) -> Result<(), GeoObserverError> {
    validate_required_observer_string("rows[].id", &row.id)?;
    if row.observer_id != contract.id {
        return Err(GeoObserverError::missing_provenance_field(
            "observer_id",
            "Geo observation rows must bind the observer contract id",
            row.observer_id.clone(),
        ));
    }
    if row.tile_pins.is_empty() {
        return Err(GeoObserverError::missing_provenance_field(
            "tile_pins",
            "Geo observation rows require at least one pinned tile",
            "0",
        ));
    }
    for pin in &row.tile_pins {
        validate_image_tile_pin(pin, forbidden_license_ids)?;
    }
    validate_blake3("window_blake3", &row.window_blake3).map_err(|_| {
        GeoObserverError::missing_provenance_field(
            "window_blake3",
            "Geo observation rows require a pinned window digest",
            row.window_blake3.clone(),
        )
    })?;
    validate_blake3("crop_blake3", &row.crop_blake3).map_err(|_| {
        GeoObserverError::missing_provenance_field(
            "crop_blake3",
            "Geo observation rows require a pinned crop digest",
            row.crop_blake3.clone(),
        )
    })?;
    validate_blake3("label_blake3", &row.label_blake3).map_err(|_| {
        GeoObserverError::missing_provenance_field(
            "label_blake3",
            "Geo observation rows require a pinned label digest",
            row.label_blake3.clone(),
        )
    })?;
    if !contract.output_kinds.contains(&row.kind) {
        return Err(GeoObserverError::uncharacterized_field(
            "kind",
            "Geo observation row kind is not declared by the observer contract",
            format!("{:?}", row.kind),
        ));
    }
    validate_payload_matches_kind(row)?;
    validate_raw_count_matches_kind(row)?;
    Ok(())
}

fn validate_payload_matches_kind(row: &GeoObservationRow) -> Result<(), GeoObserverError> {
    match (&row.kind, &row.payload) {
        (
            GeoObservationKind::StructureCountInWindow,
            GeoObservationPayload::StructureCountInWindow { min, max },
        )
        | (
            GeoObservationKind::HeightOrFloors,
            GeoObservationPayload::HeightOrFloors { min, max },
        ) => {
            if min > max {
                return Err(GeoObserverError::invalid(
                    "Geo observation integer bands must be ordered",
                    [
                        ("field".to_string(), "payload".to_string()),
                        ("observation_id".to_string(), row.id.clone()),
                    ],
                ));
            }
        }
        (
            GeoObservationKind::FootprintOutline,
            GeoObservationPayload::FootprintOutline { ring_blake3 },
        ) => validate_blake3("payload.ring_blake3", ring_blake3).map_err(|_| {
            GeoObserverError::missing_provenance_field(
                "payload.ring_blake3",
                "Geo footprint outline observations require a ring digest",
                ring_blake3.clone(),
            )
        })?,
        (
            GeoObservationKind::PresentAtVintage,
            GeoObservationPayload::PresentAtVintage { interval },
        )
        | (
            GeoObservationKind::AbsentAtVintage,
            GeoObservationPayload::AbsentAtVintage { interval },
        ) => validate_interval("payload.interval", *interval)?,
        (GeoObservationKind::ChangeEvent, GeoObservationPayload::ChangeEvent { before, after }) => {
            validate_interval("payload.before", *before)?;
            validate_interval("payload.after", *after)?;
        }
        _ => {
            return Err(GeoObserverError::uncharacterized_field(
                "payload.kind",
                "Geo observation payload kind must match the declared row kind",
                row.id.clone(),
            ));
        }
    }
    Ok(())
}

fn validate_raw_count_matches_kind(row: &GeoObservationRow) -> Result<(), GeoObserverError> {
    let Some(raw_count) = row.raw_count else {
        return Ok(());
    };
    match (&row.kind, &row.payload) {
        (
            GeoObservationKind::StructureCountInWindow,
            GeoObservationPayload::StructureCountInWindow { min, max },
        ) if raw_count >= *min && raw_count <= *max => Ok(()),
        (
            GeoObservationKind::StructureCountInWindow,
            GeoObservationPayload::StructureCountInWindow { min, max },
        ) => Err(GeoObserverError::invalid(
            "Geo observation raw_count must fall inside the structure-count band",
            [
                ("field".to_string(), "raw_count".to_string()),
                ("observation_id".to_string(), row.id.clone()),
                ("raw_count".to_string(), raw_count.to_string()),
                ("band".to_string(), format!("{min}..{max}")),
            ],
        )),
        _ => Err(GeoObserverError::invalid(
            "Geo observation raw_count is only valid for structure-count rows",
            [
                ("field".to_string(), "raw_count".to_string()),
                ("observation_id".to_string(), row.id.clone()),
            ],
        )),
    }
}

fn validate_row_blake3s(
    rows: &[GeoObservationRow],
    row_blake3s: &BTreeMap<String, String>,
) -> Result<(), GeoObserverError> {
    if row_blake3s.len() != rows.len() {
        return Err(GeoObserverError::regenerated("<row_blake3s>"));
    }
    for row in rows {
        let Some(stored) = row_blake3s.get(&row.id) else {
            return Err(GeoObserverError::regenerated(row.id.clone()));
        };
        validate_blake3("row_blake3s[]", stored)?;
        let actual = observation_row_blake3(row)?;
        if stored != &actual {
            return Err(GeoObserverError::regenerated(row.id.clone()));
        }
    }
    Ok(())
}

fn validate_id_list(
    field: &'static str,
    ids: &[String],
    rows: &[GeoObservationRow],
) -> Result<(), GeoObserverError> {
    let row_ids = rows
        .iter()
        .map(|row| row.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut previous: Option<&str> = None;
    for id in ids {
        validate_required_observer_string(field, id)?;
        if previous.is_some_and(|previous| previous >= id.as_str()) {
            return Err(GeoObserverError::invalid(
                "Geo observation artifact id lists must be sorted and unique",
                [
                    ("field".to_string(), field.to_string()),
                    ("id".to_string(), id.clone()),
                ],
            ));
        }
        if !row_ids.contains(id.as_str()) {
            return Err(GeoObserverError::invalid(
                "Geo observation artifact id lists must reference known rows",
                [
                    ("field".to_string(), field.to_string()),
                    ("id".to_string(), id.clone()),
                ],
            ));
        }
        previous = Some(id.as_str());
    }
    Ok(())
}

fn validate_rho_observation_rows(
    artifact: &GeoObservationRowsArtifact,
) -> Result<(), GeoObserverError> {
    let admitted_ids = artifact
        .rows
        .iter()
        .map(|row| row.id.as_str())
        .filter(|id| {
            artifact
                .not_admitted_ids
                .binary_search_by(|probe| probe.as_str().cmp(id))
                .is_err()
        })
        .collect::<BTreeSet<_>>();
    let allowed_contracts = artifact
        .contract
        .rho_contract_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut previous: Option<&GeoRhoObservation> = None;
    for observation in &artifact.rho_observations {
        if previous.is_some_and(|previous| previous.id.as_str() >= observation.id.as_str()) {
            return Err(GeoObserverError::invalid(
                "Geo observation rho observations must be sorted and unique",
                [("field", "rho_observations")],
            ));
        }
        if !admitted_ids.contains(observation.id.as_str()) {
            return Err(GeoObserverError::invalid(
                "Geo observation rho rows must correspond to admitted observation rows",
                [
                    ("field".to_string(), "rho_observations".to_string()),
                    ("observation_id".to_string(), observation.id.clone()),
                ],
            ));
        }
        if !allowed_contracts.contains(observation.contract_id.as_str()) {
            return Err(GeoObserverError::uncharacterized_field(
                "rho_observations[].contract_id",
                "Geo observation rho rows must use a declared rho contract",
                observation.contract_id.clone(),
            ));
        }
        if observation.source_records.is_empty() {
            return Err(GeoObserverError::missing_provenance_field(
                "rho_observations[].source_records",
                "Geo observation rho rows require source records",
                observation.id.clone(),
            ));
        }
        for record in &observation.source_records {
            validate_required_observer_string(
                "rho_observations[].source_records[].source_record_id",
                &record.source_record_id,
            )?;
            validate_required_observer_string(
                "rho_observations[].source_records[].source_vintage",
                &record.source_vintage,
            )?;
            validate_blake3(
                "rho_observations[].source_records[].record_blake3",
                &record.record_blake3,
            )?;
        }
        previous = Some(observation);
    }
    Ok(())
}

fn observation_source_records(row: &GeoObservationRow) -> Vec<GeoEvidenceRecordRef> {
    let mut records = row
        .tile_pins
        .iter()
        .enumerate()
        .map(|(index, pin)| GeoEvidenceRecordRef {
            source_record_id: format!("{}:tile:{index}", row.id),
            source_vintage: format!("{}..{}", pin.vintage.start_day, pin.vintage.end_day),
            record_blake3: pin.blake3.clone(),
        })
        .collect::<Vec<_>>();
    records.push(GeoEvidenceRecordRef {
        source_record_id: format!("{}:crop", row.id),
        source_vintage: row
            .tile_pins
            .first()
            .map(|pin| format!("{}..{}", pin.vintage.start_day, pin.vintage.end_day))
            .unwrap_or_else(|| "undated".to_string()),
        record_blake3: row.crop_blake3.clone(),
    });
    records.sort();
    records
}

fn building_unit_values(buildings: &[GeoBuildingCandidate]) -> Vec<GeoIntegerMemberValue> {
    let mut values = buildings
        .iter()
        .map(|building| GeoIntegerMemberValue {
            id: building.id.clone(),
            value: 1,
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| left.id.cmp(&right.id));
    values
}

fn building_members(buildings: &[GeoBuildingCandidate]) -> Vec<GeoEntityRef> {
    let mut members = buildings
        .iter()
        .map(|building| GeoEntityRef::new(GeoEntityLevel::Building, building.id.clone()))
        .collect::<Vec<_>>();
    members.sort();
    members
}

fn verify_replay_digest(
    detail_key: &'static str,
    row_id: &str,
    expected: &str,
    bytes_by_blake3: &BTreeMap<String, Vec<u8>>,
) -> Result<(), GeoObserverError> {
    let Some(bytes) = bytes_by_blake3.get(expected) else {
        return Err(GeoObserverError::digest_mismatch(
            detail_key,
            row_id,
            expected,
            "<missing>",
        ));
    };
    let actual = blake3::hash(bytes).to_hex().to_string();
    if actual != expected {
        return Err(GeoObserverError::digest_mismatch(
            detail_key, row_id, expected, actual,
        ));
    }
    Ok(())
}

fn is_temporal_kind(kind: GeoObservationKind) -> bool {
    matches!(
        kind,
        GeoObservationKind::PresentAtVintage | GeoObservationKind::AbsentAtVintage
    )
}

fn validate_required_observer_string(
    field: &'static str,
    value: &str,
) -> Result<(), GeoObserverError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoObserverError::missing_provenance_field(
            field,
            "Geo observer provenance fields must be non-empty and canonical-trimmed",
            value,
        ));
    }
    Ok(())
}

fn validate_interval(
    field: &'static str,
    interval: GeoValidTimeInterval,
) -> Result<(), GeoObserverError> {
    if interval.start_day > interval.end_day {
        return Err(GeoObserverError::invalid(
            "Geo observer valid-time intervals must be ordered",
            [
                ("field".to_string(), field.to_string()),
                ("start_day".to_string(), interval.start_day.to_string()),
                ("end_day".to_string(), interval.end_day.to_string()),
            ],
        ));
    }
    Ok(())
}

pub fn splitmix64(value: u64) -> u64 {
    let mut z = value.wrapping_add(SPLITMIX64_INCREMENT);
    z = (z ^ (z >> 30)).wrapping_mul(SPLITMIX64_MULTIPLIER_1);
    z = (z ^ (z >> 27)).wrapping_mul(SPLITMIX64_MULTIPLIER_2);
    z ^ (z >> 31)
}

pub fn truth_plane_key(truth_plane: GeoTruthPlane) -> &'static str {
    match truth_plane {
        GeoTruthPlane::NonRoundAmountDateLegalBorough => "non_round_amount_date_legal_borough",
        GeoTruthPlane::RoundExactLenderParty => "round_exact_lender_party",
        GeoTruthPlane::GateV2Historical => "gate_v2_historical",
        GeoTruthPlane::AddressDerivedControl => "address_derived_control",
        GeoTruthPlane::DeedGrainInstrument => "deed_grain_instrument",
        GeoTruthPlane::HumanAdjudication => "human_adjudication",
    }
}

fn validate_declared_before_observer_ids(ids: &[String]) -> Result<(), GeoObserverError> {
    if ids.is_empty() {
        return Err(GeoObserverError::invalid_field(
            "declared_before_observer_ids",
            "Geo error-population artifacts must bind at least one future observer id",
            "0",
        ));
    }
    let mut previous: Option<&str> = None;
    for id in ids {
        validate_canonical_string("declared_before_observer_ids[]", id)?;
        if let Some(previous_id) = previous
            && previous_id >= id.as_str()
        {
            return Err(GeoObserverError::invalid(
                "Geo error-population observer ids must be strictly sorted and unique",
                [
                    ("field", "declared_before_observer_ids".to_string()),
                    ("previous", previous_id.to_string()),
                    ("current", id.clone()),
                ],
            ));
        }
        previous = Some(id.as_str());
    }
    Ok(())
}

fn validate_error_population_subjects(
    subjects: &[GeoErrorPopulationSubject],
) -> Result<(), GeoObserverError> {
    if subjects.is_empty() {
        return Err(GeoObserverError::invalid_field(
            "subjects",
            "Geo error-population artifacts must contain at least one subject",
            "0",
        ));
    }
    let mut previous: Option<&str> = None;
    for subject in subjects {
        validate_canonical_string("subjects[].subject_id", &subject.subject_id)?;
        match subject.truth_plane {
            GeoTruthPlane::NonRoundAmountDateLegalBorough
            | GeoTruthPlane::RoundExactLenderParty => {}
            _ => {
                return Err(GeoObserverError::invalid(
                    "Geo error-population subjects must use a controlling H.7 truth plane",
                    [
                        ("field", "subjects[].truth_plane".to_string()),
                        ("subject_id", subject.subject_id.clone()),
                        (
                            "truth_plane",
                            truth_plane_key(subject.truth_plane).to_string(),
                        ),
                    ],
                ));
            }
        }
        validate_blake3("subjects[].window_blake3", &subject.window_blake3)?;
        validate_parcel_ids(subject)?;
        if let Some(previous_id) = previous
            && previous_id >= subject.subject_id.as_str()
        {
            return Err(GeoObserverError::invalid(
                "Geo error-population subjects must be strictly sorted and unique",
                [
                    ("field", "subjects".to_string()),
                    ("previous_subject_id", previous_id.to_string()),
                    ("subject_id", subject.subject_id.clone()),
                ],
            ));
        }
        previous = Some(subject.subject_id.as_str());
    }
    Ok(())
}

fn validate_parcel_ids(subject: &GeoErrorPopulationSubject) -> Result<(), GeoObserverError> {
    if subject.parcel_ids.is_empty() {
        return Err(GeoObserverError::invalid(
            "Geo error-population subjects must carry at least one parcel id",
            [
                ("field", "subjects[].parcel_ids".to_string()),
                ("subject_id", subject.subject_id.clone()),
            ],
        ));
    }
    let mut seen = BTreeSet::new();
    let mut previous: Option<&str> = None;
    for parcel_id in &subject.parcel_ids {
        validate_canonical_string("subjects[].parcel_ids[]", parcel_id)?;
        if !seen.insert(parcel_id.as_str())
            || previous.is_some_and(|previous_id| previous_id >= parcel_id.as_str())
        {
            return Err(GeoObserverError::invalid(
                "Geo error-population subject parcel ids must be strictly sorted and unique",
                [
                    ("field", "subjects[].parcel_ids".to_string()),
                    ("subject_id", subject.subject_id.clone()),
                    ("parcel_id", parcel_id.clone()),
                ],
            ));
        }
        previous = Some(parcel_id.as_str());
    }
    Ok(())
}

fn validate_stratum_counts(
    subjects: &[GeoErrorPopulationSubject],
    stratum_counts: &BTreeMap<String, u64>,
) -> Result<(), GeoObserverError> {
    if stratum_counts.is_empty() {
        return Err(GeoObserverError::invalid_field(
            "stratum_counts",
            "Geo error-population artifacts must declare per-truth-plane counts",
            "0",
        ));
    }
    let mut expected = BTreeMap::<String, u64>::new();
    for subject in subjects {
        let key = truth_plane_key(subject.truth_plane).to_string();
        *expected.entry(key).or_default() += 1;
    }
    for required in [
        GeoTruthPlane::NonRoundAmountDateLegalBorough,
        GeoTruthPlane::RoundExactLenderParty,
    ] {
        if expected
            .get(truth_plane_key(required))
            .copied()
            .unwrap_or(0)
            == 0
        {
            return Err(GeoObserverError::invalid(
                "Geo error-population artifacts must represent both H.7 truth planes",
                [
                    ("field", "stratum_counts".to_string()),
                    ("missing_truth_plane", truth_plane_key(required).to_string()),
                ],
            ));
        }
    }
    if stratum_counts != &expected {
        return Err(GeoObserverError::invalid(
            "Geo error-population stratum_counts must match subjects",
            [("field", "stratum_counts".to_string())],
        ));
    }
    Ok(())
}

fn validate_canonical_string(field: &'static str, value: &str) -> Result<(), GeoObserverError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoObserverError::invalid_field(
            field,
            "Geo observer string fields must be non-empty and canonical-trimmed",
            value,
        ));
    }
    Ok(())
}

fn validate_required_blake3(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), GeoObserverError> {
    match value {
        Some(value) => validate_blake3(field, value),
        None => Err(GeoObserverError::invalid_field(
            field,
            "Geo observer BLAKE3 fields must be present",
            "<missing>",
        )),
    }
}

fn validate_blake3(field: &'static str, value: &str) -> Result<(), GeoObserverError> {
    if value.len() != 64
        || !value.chars().all(|ch| ch.is_ascii_hexdigit())
        || value.chars().any(|ch| ch.is_ascii_uppercase())
    {
        return Err(GeoObserverError::invalid_field(
            field,
            "Geo observer BLAKE3 fields must be lowercase fixed-width hex",
            value,
        ));
    }
    Ok(())
}
