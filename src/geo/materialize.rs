#![forbid(unsafe_code)]

//! Offline materialization of warehouse-shaped rows into typed Geo evidence.
//!
//! Acquisition remains outside Canon. This module accepts rows that have
//! already been exported with explicit releases and immutable record digests,
//! folds their relational grain into one deterministic evidence request, and
//! validates that request through the real evidence compiler. Row count is
//! provenance only: repeated source rows never increase constraint strength.

use super::{
    composition::{DEFAULT_MAX_MATERIALIZED_MODELS, GeoBuildingCandidate, GeoCompositionUniverse},
    evidence::{
        CANON_GEO_EVIDENCE_REQUEST_VERSION, GeoEvidenceCompilationRequest, GeoEvidenceError,
        GeoEvidenceErrorCode, GeoEvidenceRecordRef, GeoRhoContract, GeoRhoObservation,
        GeoRhoObservationKind, GeoValidTimeInterval, compile_evidence,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_WAREHOUSE_ROWS_VERSION: &str = "canon_geo_warehouse_rows.v0";

/// One row at the parcel-candidate grain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoWarehouseParcelRow {
    pub parcel_id: String,
}

/// One row at the building/possible-parcel incidence grain.
///
/// A null `parcel_id` is an explicit marker that the building was observed but
/// no containment candidate was admitted. It may not be mixed with incidence
/// rows for the same building.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoWarehouseBuildingParcelRow {
    pub building_id: String,
    pub parcel_id: Option<String>,
}

/// One immutable source-record row supporting a typed rho observation.
///
/// Multiple rows may share an `observation_id` only when their contract,
/// valid-time interval, and observation payload are identical. They are then
/// grouped as provenance for one observation rather than counted as separate
/// evidence constraints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoWarehouseEvidenceRow {
    pub observation_id: String,
    pub contract_id: String,
    pub source_record: GeoEvidenceRecordRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_time: Option<GeoValidTimeInterval>,
    pub observation: GeoRhoObservationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoWarehouseRowsRequest {
    pub version: String,
    pub parcel_rows: Vec<GeoWarehouseParcelRow>,
    #[serde(default)]
    pub building_parcel_rows: Vec<GeoWarehouseBuildingParcelRow>,
    pub contracts: Vec<GeoRhoContract>,
    pub evidence_rows: Vec<GeoWarehouseEvidenceRow>,
    pub max_assignments: u64,
    #[serde(default = "default_max_materialized_models")]
    pub max_materialized_models: u64,
}

fn default_max_materialized_models() -> u64 {
    DEFAULT_MAX_MATERIALIZED_MODELS
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoMaterializationErrorCode {
    UnsupportedVersion,
    InvalidInput,
    Evidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoMaterializationError {
    pub code: GeoMaterializationErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoMaterializationError {
    fn new(
        code: GeoMaterializationErrorCode,
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
        Self::new(GeoMaterializationErrorCode::InvalidInput, message, detail)
    }
}

impl From<GeoEvidenceError> for GeoMaterializationError {
    fn from(error: GeoEvidenceError) -> Self {
        let mut detail = error.detail;
        detail.insert(
            "evidence_code".to_string(),
            evidence_error_code_name(error.code).to_string(),
        );
        Self {
            code: GeoMaterializationErrorCode::Evidence,
            message: error.message,
            detail,
        }
    }
}

impl fmt::Display for GeoMaterializationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoMaterializationError {}

#[derive(Debug)]
struct PendingObservation {
    contract_id: String,
    valid_time: Option<GeoValidTimeInterval>,
    observation: GeoRhoObservationKind,
    source_records: BTreeMap<String, GeoEvidenceRecordRef>,
}

/// Fold relational warehouse rows into a deterministic typed evidence request.
///
/// Successful materialization means the emitted request is accepted by the
/// existing evidence/compiler contracts. It does not establish that an
/// upstream row, geometry predicate, rho contract, or source assertion is true.
pub fn materialize_warehouse_rows(
    rows: &GeoWarehouseRowsRequest,
) -> Result<GeoEvidenceCompilationRequest, GeoMaterializationError> {
    if rows.version != CANON_GEO_WAREHOUSE_ROWS_VERSION {
        return Err(GeoMaterializationError::new(
            GeoMaterializationErrorCode::UnsupportedVersion,
            "Unsupported Geo warehouse-row version",
            [
                ("actual", rows.version.as_str()),
                ("expected", CANON_GEO_WAREHOUSE_ROWS_VERSION),
            ],
        ));
    }

    let mut parcels = BTreeSet::new();
    for row in &rows.parcel_rows {
        if !parcels.insert(row.parcel_id.clone()) {
            return Err(GeoMaterializationError::invalid(
                "Geo warehouse parcel rows repeat the declared grain",
                [("parcel_id", row.parcel_id.as_str())],
            ));
        }
    }

    let mut seen_building_rows = BTreeSet::new();
    let mut building_candidates: BTreeMap<String, (bool, BTreeSet<String>)> = BTreeMap::new();
    for row in &rows.building_parcel_rows {
        if !seen_building_rows.insert((row.building_id.clone(), row.parcel_id.clone())) {
            return Err(GeoMaterializationError::invalid(
                "Geo warehouse building/parcel rows repeat the declared grain",
                [
                    ("building_id", row.building_id.as_str()),
                    ("parcel_id", row.parcel_id.as_deref().unwrap_or("null")),
                ],
            ));
        }
        let (has_null_marker, parcel_ids) = building_candidates
            .entry(row.building_id.clone())
            .or_default();
        match &row.parcel_id {
            Some(parcel_id) => {
                parcel_ids.insert(parcel_id.clone());
            }
            None => *has_null_marker = true,
        }
    }

    let mut buildings = Vec::with_capacity(building_candidates.len());
    for (building_id, (has_null_marker, parcel_ids)) in building_candidates {
        if has_null_marker && !parcel_ids.is_empty() {
            return Err(GeoMaterializationError::invalid(
                "A building cannot mix a no-containment marker with parcel incidences",
                [("building_id", building_id.as_str())],
            ));
        }
        buildings.push(GeoBuildingCandidate {
            id: building_id,
            parcel_ids: parcel_ids.into_iter().collect(),
        });
    }

    let mut pending: BTreeMap<String, PendingObservation> = BTreeMap::new();
    for row in &rows.evidence_rows {
        let entry = pending
            .entry(row.observation_id.clone())
            .or_insert_with(|| PendingObservation {
                contract_id: row.contract_id.clone(),
                valid_time: row.valid_time,
                observation: row.observation.clone(),
                source_records: BTreeMap::new(),
            });
        if entry.contract_id != row.contract_id
            || entry.valid_time != row.valid_time
            || entry.observation != row.observation
        {
            return Err(GeoMaterializationError::invalid(
                "Rows for one Geo observation disagree on typed semantics",
                [("observation_id", row.observation_id.as_str())],
            ));
        }
        if entry
            .source_records
            .insert(
                row.source_record.source_record_id.clone(),
                row.source_record.clone(),
            )
            .is_some()
        {
            return Err(GeoMaterializationError::invalid(
                "Geo warehouse evidence rows repeat a source record id within an observation",
                [
                    ("observation_id", row.observation_id.as_str()),
                    (
                        "source_record_id",
                        row.source_record.source_record_id.as_str(),
                    ),
                ],
            ));
        }
    }

    let observations = pending
        .into_iter()
        .map(|(id, observation)| GeoRhoObservation {
            id,
            contract_id: observation.contract_id,
            source_records: observation.source_records.into_values().collect(),
            valid_time: observation.valid_time,
            observation: observation.observation,
        })
        .collect();

    let mut contracts = rows.contracts.clone();
    contracts.sort_by(|left, right| left.id.cmp(&right.id));
    let request = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        universe: GeoCompositionUniverse {
            parcels: parcels.into_iter().collect(),
            buildings,
        },
        contracts,
        observations,
        max_assignments: rows.max_assignments,
        max_materialized_models: rows.max_materialized_models,
    };

    // Validation is deliberately delegated to the production compiler so the
    // row surface cannot become a weaker alternate admission path.
    compile_evidence(&request).map_err(GeoMaterializationError::from)?;
    Ok(request)
}

pub fn canonical_materialized_evidence_request_bytes(
    request: &GeoEvidenceCompilationRequest,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(request)
}

fn evidence_error_code_name(code: GeoEvidenceErrorCode) -> &'static str {
    match code {
        GeoEvidenceErrorCode::UnsupportedVersion => "unsupported_version",
        GeoEvidenceErrorCode::InvalidInput => "invalid_input",
        GeoEvidenceErrorCode::Composition => "composition",
    }
}
