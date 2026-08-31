#![forbid(unsafe_code)]

//! Offline materialization of warehouse-shaped rows into typed Geo evidence.
//!
//! Acquisition remains outside Canon. This module accepts rows that have
//! already been exported with explicit releases and immutable record digests,
//! folds their relational grain into one deterministic evidence request, and
//! validates that request through the real evidence compiler. Row count is
//! provenance only: repeated source rows never increase constraint strength.

use super::{
    composition::{
        DEFAULT_MAX_MATERIALIZED_MODELS, GeoBuildingCandidate, GeoCompositionModel,
        GeoCompositionUniverse,
    },
    evaluation::{
        CANON_GEO_POPULATION_REQUEST_VERSION, GeoLabeledCompositionCase,
        GeoPopulationEvaluationRequest, GeoTruthPlane,
    },
    evidence::{
        CANON_GEO_EVIDENCE_REQUEST_VERSION, GeoEvidenceCompilationRequest, GeoEvidenceError,
        GeoEvidenceErrorCode, GeoEvidenceRecordRef, GeoRhoContract, GeoRhoObservation,
        GeoRhoObservationKind, GeoValidTimeInterval, compile_evidence,
    },
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde::{Deserialize, Serialize};
use serde::{Deserializer, de};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_WAREHOUSE_ROWS_VERSION: &str = "canon_geo_warehouse_rows.v0";
pub const CANON_GEO_H7_POPULATION_ROWS_VERSION: &str = "canon_geo_h7_population_rows.v0";
pub const CANON_GEO_H7_POPULATION_VERSION: &str = "canon_geo_h7_population.v0";
pub const CANON_GEO_H7_ACRIS_RELEASE_DT: &str = "2026-08-10";
pub const CANON_GEO_H7_BRIDGE_BUILD_ID: &str = "3aed6660-ce1c-46a9-aeb2-7296c134ce8f";
pub const CANON_GEO_H7_PROPERTY_STATE: &str = "NY";
pub const CANON_GEO_H7_COLLATERAL_SCOPE: &str = "nyc_filed_collateral_slice";
pub const CANON_GEO_H7_PRIMARY_MAPPLUTO_RELEASE: &str = "26v2";
pub const CANON_GEO_H7_AMOUNT_CENTS_QUANTIZATION: &str = "ROUND(value * 100, 0)::NUMBER(38,0)";
/// $100,000 expressed on the integer-cents lattice; $1,000,000 multiples are
/// therefore included in the same round-amount plane.
pub const CANON_GEO_H7_ROUND_AMOUNT_LATTICE_CENTS: u64 = 10_000_000;
pub const CANON_GEO_H7_LENDER_MATCH_TRANSFORM: &str =
    "TRIM(REGEXP_REPLACE(UPPER(name), '[^A-Z0-9 ]', ' '))";
pub const CANON_GEO_H7_NON_ROUND_LEGAL_RESIDUAL_RECEIPT_PURPOSE: &str =
    "live_non_round_acris_candidate_legal_residual";
pub const CANON_GEO_H7_ROUND_LEGAL_RESIDUAL_RECEIPT_PURPOSE: &str =
    "live_round_lender_acris_candidate_legal_residual";
pub const CANON_GEO_H7_MAPPLUTO_GEOMETRY_CONTRACT_VERSION: &str =
    "nyc_dcp_mappluto_geometry_evidence.v3";
pub const CANON_GEO_H7_COMPLETE_NON_ROUND_MULTI_PARCEL_LOANS: u64 = 35;
pub const CANON_GEO_H7_COMPLETE_ROUND_MULTI_PARCEL_LOANS: u64 = 14;
pub const CANON_GEO_H7_COMPLETE_MULTI_PARCEL_LOANS: u64 = 49;
pub const CANON_GEO_H7_COMPLETE_RELEASE_ROWS: u64 = 98;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoH7ResultMode {
    Live,
    Replay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoH7QueryDisposition {
    Cited,
    DiagnosticOnly,
    Discarded,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoH7AssociationPlane {
    SingleProperty,
    MultiProperty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoH7CandidateReachStatus {
    Full,
    Partial,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoH7PopulationScope {
    FixtureSubset,
    RetainedComplete,
    LiveComplete,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoH7MapplutoReleasePin {
    pub release: String,
    pub release_dt: String,
    pub variant: String,
    pub geometry_contract_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoH7FiledCountyMapping {
    pub filed_county: String,
    pub acris_borough: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoH7BoroughEdge {
    pub filed_county: String,
    pub filed_borough: u8,
    pub legal_borough: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoH7SourceHash {
    pub source: String,
    pub hash_kind: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoH7SourceRecordRole {
    BridgeLoan,
    AcrisMaster,
    AcrisLegal,
    AcrisParty,
    MapplutoCandidate,
    GeocodeDiagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct GeoH7SourceEvidenceRecord {
    pub role: GeoH7SourceRecordRole,
    pub parcel_ids: Vec<String>,
    pub source_record: GeoEvidenceRecordRef,
}

impl<'de> Deserialize<'de> for GeoH7SourceEvidenceRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireSourceRecord {
            source_record_id: String,
            source_vintage: String,
            #[serde(default)]
            record_blake3: String,
        }

        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireRecord {
            role: GeoH7SourceRecordRole,
            parcel_ids: Vec<String>,
            source_record: WireSourceRecord,
            #[serde(default)]
            source_record_bytes_base64: Option<String>,
        }

        let wire = WireRecord::deserialize(deserializer)?;
        let record_blake3 = h7_source_record_digest_from_wire(
            &wire.source_record.source_record_id,
            &wire.source_record.record_blake3,
            wire.source_record_bytes_base64.as_deref(),
        )
        .map_err(de::Error::custom)?;
        Ok(Self {
            role: wire.role,
            parcel_ids: wire.parcel_ids,
            source_record: GeoEvidenceRecordRef {
                source_record_id: wire.source_record.source_record_id,
                source_vintage: wire.source_record.source_vintage,
                record_blake3,
            },
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoH7ExternalReceiptKind {
    RevealLineage,
    ArchivedAppendixG7,
    WarehouseQueryHistory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoH7ExternalReceiptRef {
    pub receipt_id: String,
    pub kind: GeoH7ExternalReceiptKind,
    pub purpose: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoH7QueryReceipt {
    pub purpose: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truth_plane: Option<GeoTruthPlane>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_id: Option<String>,
    pub query_text_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_query_text: Option<String>,
    pub query_blake3: String,
    pub result_rows: u64,
    pub row_cap: u64,
    pub disposition: GeoH7QueryDisposition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoH7EmpiricalDiscrepancyStatus {
    Open,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoH7EmpiricalDiscrepancy {
    pub subject: String,
    pub archived_measurement: String,
    pub fresh_measurement: String,
    pub status: GeoH7EmpiricalDiscrepancyStatus,
    pub receipt_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoH7PopulationProvenance {
    pub result_mode: GeoH7ResultMode,
    pub as_of: String,
    pub acris_release_dt: String,
    pub bridge_build_id: String,
    pub collateral_scope: String,
    pub mappluto_releases: Vec<GeoH7MapplutoReleasePin>,
    pub primary_candidate_release: GeoH7MapplutoReleasePin,
    pub amount_cents_quantization: String,
    pub round_amount_lattice_cents: u64,
    pub lender_match_transform: String,
    pub filed_county_mapping: Vec<GeoH7FiledCountyMapping>,
    pub source_hashes: Vec<GeoH7SourceHash>,
    pub query_receipts: Vec<GeoH7QueryReceipt>,
    pub external_receipts: Vec<GeoH7ExternalReceiptRef>,
    pub empirical_discrepancies: Vec<GeoH7EmpiricalDiscrepancy>,
    pub row_cap: u64,
    pub observed_rows: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoH7PlaneDenominator {
    pub truth_plane: GeoTruthPlane,
    pub eligible_loans: u64,
    pub candidate_loans: u64,
    pub legal_confirmed_candidate_loans: u64,
    pub accepted_loans: u64,
    pub ambiguous_loans: u64,
    pub candidate_no_legal_confirmation_loans: u64,
    pub no_candidate_loans: u64,
    pub selected_multi_parcel_loans: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoH7LoanFieldDistinctCounts {
    pub originatorname: u64,
    pub originator_match_text: u64,
    pub originationdate: u64,
    pub originalloanamount: u64,
    pub filed_borough: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoH7PopulationWarehouseRow {
    pub loan_key: String,
    pub document_id: String,
    pub truth_plane: GeoTruthPlane,
    pub association_plane: GeoH7AssociationPlane,
    pub candidate_release: GeoH7MapplutoReleasePin,
    pub property_state: String,
    pub filed_county: String,
    pub filed_borough: u8,
    pub legal_borough: u8,
    pub accepted_borough_edges: Vec<GeoH7BoroughEdge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geocoded_county_fips: Option<String>,
    pub doc_type: String,
    pub originationdate: String,
    pub amount_cents: u64,
    pub is_round_100k_lattice: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originatorname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originator_match_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lender_match_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lender_party_type: Option<String>,
    pub loan_field_distinct_counts: GeoH7LoanFieldDistinctCounts,
    pub truth_parcels: Vec<String>,
    pub candidate_parcels: Vec<String>,
    pub reach_status: GeoH7CandidateReachStatus,
    pub reach_reason: String,
    pub source_records: Vec<GeoH7SourceEvidenceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoH7PopulationRowsRequest {
    pub version: String,
    pub population_scope: GeoH7PopulationScope,
    pub provenance: GeoH7PopulationProvenance,
    pub plane_denominators: Vec<GeoH7PlaneDenominator>,
    pub rows: Vec<GeoH7PopulationWarehouseRow>,
    pub max_cases: usize,
    pub max_assignments: u64,
    #[serde(default = "default_max_materialized_models")]
    pub max_materialized_models: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoH7PopulationCaseArtifact {
    pub subject_id: String,
    pub case_id: String,
    pub loan_key: String,
    pub document_id: String,
    pub truth_plane: GeoTruthPlane,
    pub association_plane: GeoH7AssociationPlane,
    pub candidate_release: GeoH7MapplutoReleasePin,
    pub property_state: String,
    pub filed_county: String,
    pub filed_borough: u8,
    pub legal_borough: u8,
    pub accepted_borough_edges: Vec<GeoH7BoroughEdge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geocoded_county_fips: Option<String>,
    pub doc_type: String,
    pub originationdate: String,
    pub amount_cents: u64,
    pub is_round_100k_lattice: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originatorname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originator_match_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lender_match_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lender_party_type: Option<String>,
    pub truth_parcels: Vec<String>,
    pub candidate_parcels: Vec<String>,
    pub reach_status: GeoH7CandidateReachStatus,
    pub reach_reason: String,
    pub source_records: Vec<GeoH7SourceEvidenceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoH7TruthPlaneSummary {
    pub truth_plane: GeoTruthPlane,
    pub eligible_loans: u64,
    pub candidate_loans: u64,
    pub legal_confirmed_candidate_loans: u64,
    pub accepted_loans: u64,
    pub ambiguous_loans: u64,
    pub candidate_no_legal_confirmation_loans: u64,
    pub no_candidate_loans: u64,
    pub selected_multi_parcel_loans: u64,
    pub materialized_case_rows: u64,
    pub materialized_unique_accepted_loans: u64,
    pub candidate_reach_full_cases: u64,
    pub candidate_reach_partial_cases: u64,
    pub candidate_reach_none_cases: u64,
    pub truth_parcels: u64,
    pub candidate_parcels: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoH7PopulationStratumSummary {
    pub truth_plane: GeoTruthPlane,
    pub association_plane: GeoH7AssociationPlane,
    pub candidate_release: GeoH7MapplutoReleasePin,
    pub materialized_case_rows: u64,
    pub materialized_unique_accepted_loans: u64,
    pub candidate_reach_full_cases: u64,
    pub candidate_reach_partial_cases: u64,
    pub candidate_reach_none_cases: u64,
    pub truth_parcels: u64,
    pub candidate_parcels: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoH7PopulationSummary {
    pub population_scope: GeoH7PopulationScope,
    pub source_rows: u64,
    pub materialized_case_rows: u64,
    pub materialized_unique_accepted_loans: u64,
    pub solver_population_subjects: u64,
    pub truth_planes: Vec<GeoH7TruthPlaneSummary>,
    pub strata: Vec<GeoH7PopulationStratumSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoH7PopulationArtifact {
    pub version: String,
    pub rows_version: String,
    pub provenance: GeoH7PopulationProvenance,
    pub summary: GeoH7PopulationSummary,
    pub cases: Vec<GeoH7PopulationCaseArtifact>,
    pub population: GeoPopulationEvaluationRequest,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct GeoH7AcceptedLoanTruthKey {
    document_id: String,
    truth_plane: GeoTruthPlane,
    association_plane: GeoH7AssociationPlane,
    property_state: String,
    accepted_borough_edges: Vec<GeoH7BoroughEdge>,
    doc_type: String,
    originationdate: String,
    amount_cents: u64,
    is_round_100k_lattice: bool,
    originatorname: Option<String>,
    originator_match_text: Option<String>,
    lender_match_text: Option<String>,
    lender_party_type: Option<String>,
    truth_parcels: Vec<String>,
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

/// Convert release-pinned Appendix H.7 warehouse rows into a typed population
/// artifact with labels held outside solver evidence.
///
/// The row surface is intentionally stricter than plain JSON shape:
/// controlling H.7 has exactly two disjoint truth planes, exact cents are
/// interpreted only on the declared cents quantization, roundness is the
/// $100,000 lattice, and per-loan bridge fields must be singular before a row
/// may be classified. The emitted artifact carries a nested `.population`
/// request for `canon geo evaluate`; its evidence requests contain only
/// candidate universes, so document truth never becomes a solver constraint.
pub fn materialize_h7_population_rows(
    rows: &GeoH7PopulationRowsRequest,
) -> Result<GeoH7PopulationArtifact, GeoMaterializationError> {
    if rows.version != CANON_GEO_H7_POPULATION_ROWS_VERSION {
        return Err(GeoMaterializationError {
            code: GeoMaterializationErrorCode::UnsupportedVersion,
            message: "Unsupported Geo H.7 population-row version".to_string(),
            detail: [
                ("actual".to_string(), rows.version.clone()),
                (
                    "expected".to_string(),
                    CANON_GEO_H7_POPULATION_ROWS_VERSION.to_string(),
                ),
            ]
            .into_iter()
            .collect(),
        });
    }
    if rows.max_cases == 0 {
        return Err(h7_invalid(
            "Geo H.7 population requires a positive solver subject budget",
            [("max_cases", rows.max_cases.to_string())],
        ));
    }
    if rows.max_assignments == 0 {
        return Err(h7_invalid(
            "Geo H.7 population requires a positive solver assignment budget",
            [("max_assignments", rows.max_assignments.to_string())],
        ));
    }
    validate_h7_provenance(rows.population_scope, &rows.provenance, rows.rows.len())?;
    validate_h7_live_legal_residuals(rows.population_scope, &rows.provenance)?;

    let denominators = validate_h7_denominators(&rows.plane_denominators)?;
    if rows.rows.is_empty() {
        return Err(h7_invalid(
            "Geo H.7 population rows must be non-empty",
            [("rows", "0".to_string())],
        ));
    }

    let mut seen_cases = BTreeSet::new();
    let mut accepted_loans: BTreeMap<String, GeoH7AcceptedLoanTruthKey> = BTreeMap::new();
    let mut releases_by_loan: BTreeMap<String, BTreeSet<(String, String, String, String)>> =
        BTreeMap::new();
    let mut cases = Vec::with_capacity(rows.rows.len());

    for row in &rows.rows {
        let materialized =
            materialize_h7_case(row, rows.max_assignments, rows.max_materialized_models)?;
        let truth_key = accepted_truth_key(&materialized);
        if let Some(prior_truth_key) =
            accepted_loans.insert(materialized.loan_key.clone(), truth_key.clone())
            && prior_truth_key != truth_key
        {
            return Err(h7_invalid(
                "Geo H.7 population rows assign one accepted loan to conflicting accepted truth",
                [
                    ("loan_key", materialized.loan_key.clone()),
                    (
                        "prior_truth_plane",
                        h7_plane_name(prior_truth_key.truth_plane).to_string(),
                    ),
                    ("prior_document_id", prior_truth_key.document_id),
                    (
                        "current_truth_plane",
                        h7_plane_name(materialized.truth_plane).to_string(),
                    ),
                    ("current_document_id", materialized.document_id.clone()),
                ],
            ));
        }
        let release_key = mappluto_pin_key(&materialized.candidate_release);
        if !releases_by_loan
            .entry(materialized.loan_key.clone())
            .or_default()
            .insert(release_key.clone())
        {
            return Err(h7_invalid(
                "Geo H.7 population rows repeat one loan/candidate-release measurement",
                [
                    ("loan_key", materialized.loan_key.clone()),
                    (
                        "candidate_release",
                        materialized.candidate_release.release.clone(),
                    ),
                ],
            ));
        }
        if !seen_cases.insert(materialized.case_id.clone()) {
            return Err(h7_invalid(
                "Geo H.7 population rows repeat a stable case identity",
                [
                    ("case_id", materialized.case_id.clone()),
                    ("loan_key", row.loan_key.clone()),
                    ("document_id", row.document_id.clone()),
                ],
            ));
        }
        cases.push(materialized);
    }
    cases.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    validate_h7_release_coverage(&rows.provenance, &accepted_loans, &releases_by_loan)?;
    validate_h7_population_scope(
        rows.population_scope,
        rows.provenance.result_mode,
        len_u64(&rows.provenance.mappluto_releases, "pinned_release_count")?,
        &denominators,
        &cases,
    )?;

    let primary_release_key = mappluto_pin_key(&rows.provenance.primary_candidate_release);
    let primary_release_cases = cases
        .iter()
        .filter(|case| mappluto_pin_key(&case.candidate_release) == primary_release_key)
        .collect::<Vec<_>>();
    if primary_release_cases.len() != accepted_loans.len() {
        return Err(h7_invalid(
            "Geo H.7 primary release must contain exactly one materialized row per accepted loan",
            [
                (
                    "primary_release_rows",
                    primary_release_cases.len().to_string(),
                ),
                ("accepted_loans", accepted_loans.len().to_string()),
            ],
        ));
    }
    // A zero-candidate row is an upstream reach result, not a valid empty
    // universe for exact composition. Preserve it in `cases` and the reach
    // summaries, but do not manufacture a solver case for it.
    let mut population_cases = primary_release_cases
        .into_iter()
        .filter(|case| !case.candidate_parcels.is_empty())
        .map(|case| GeoLabeledCompositionCase {
            id: case.subject_id.clone(),
            evidence: GeoEvidenceCompilationRequest {
                version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
                universe: GeoCompositionUniverse {
                    parcels: case.candidate_parcels.clone(),
                    buildings: Vec::new(),
                },
                contracts: Vec::new(),
                observations: Vec::new(),
                max_assignments: rows.max_assignments,
                max_materialized_models: rows.max_materialized_models,
            },
            truth_plane: case.truth_plane,
            truth: GeoCompositionModel {
                parcels: case.truth_parcels.clone(),
                buildings: Vec::new(),
            },
        })
        .collect::<Vec<_>>();
    population_cases.sort_by(|left, right| left.id.cmp(&right.id));
    if population_cases.len() > rows.max_cases {
        return Err(h7_invalid(
            "Geo H.7 primary-release solver subjects exceed the declared case budget",
            [
                ("solver_subjects", population_cases.len().to_string()),
                ("max_cases", rows.max_cases.to_string()),
            ],
        ));
    }

    let observed_planes = cases
        .iter()
        .map(|case| case.truth_plane)
        .collect::<BTreeSet<_>>();
    for plane in h7_truth_planes() {
        if !observed_planes.contains(&plane) {
            return Err(h7_invalid(
                "Geo H.7 population must keep both controlling truth planes materialized",
                [("missing_truth_plane", h7_plane_name(plane).to_string())],
            ));
        }
    }

    let population = GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: population_cases,
        max_cases: rows.max_cases,
    };
    for case in &population.cases {
        compile_evidence(&case.evidence).map_err(GeoMaterializationError::from)?;
    }

    let summary = summarize_h7_population(
        rows.population_scope,
        &cases,
        &denominators,
        population.cases.len(),
    )?;
    Ok(GeoH7PopulationArtifact {
        version: CANON_GEO_H7_POPULATION_VERSION.to_string(),
        rows_version: rows.version.clone(),
        provenance: canonicalize_h7_provenance(&rows.provenance),
        summary,
        cases,
        population,
    })
}

pub fn canonical_h7_population_bytes(
    artifact: &GeoH7PopulationArtifact,
) -> Result<Vec<u8>, serde_json::Error> {
    let mut canonical = artifact.clone();
    canonical.provenance = canonicalize_h7_provenance(&canonical.provenance);
    canonical
        .cases
        .sort_by(|left, right| left.case_id.cmp(&right.case_id));
    canonical
        .population
        .cases
        .sort_by(|left, right| left.id.cmp(&right.id));
    canonical
        .summary
        .truth_planes
        .sort_by_key(|summary| summary.truth_plane);
    canonical.summary.strata.sort_by(|left, right| {
        (
            left.truth_plane,
            left.association_plane,
            &left.candidate_release,
        )
            .cmp(&(
                right.truth_plane,
                right.association_plane,
                &right.candidate_release,
            ))
    });
    serde_json::to_vec(&canonical)
}

fn evidence_error_code_name(code: GeoEvidenceErrorCode) -> &'static str {
    match code {
        GeoEvidenceErrorCode::UnsupportedVersion => "unsupported_version",
        GeoEvidenceErrorCode::InvalidInput => "invalid_input",
        GeoEvidenceErrorCode::Composition => "composition",
    }
}

fn validate_h7_provenance(
    population_scope: GeoH7PopulationScope,
    provenance: &GeoH7PopulationProvenance,
    input_rows: usize,
) -> Result<(), GeoMaterializationError> {
    validate_h7_string("provenance.as_of", &provenance.as_of)?;
    let input_rows = u64::try_from(input_rows).map_err(|_| h7_overflow("input_rows"))?;
    if provenance.row_cap == 0 {
        return Err(h7_invalid(
            "Geo H.7 provenance requires a positive row cap",
            [("row_cap", "0".to_string())],
        ));
    }
    if input_rows > provenance.row_cap {
        return Err(h7_invalid(
            "Geo H.7 input rows exceed the declared provenance row cap",
            [
                ("input_rows", input_rows.to_string()),
                ("row_cap", provenance.row_cap.to_string()),
            ],
        ));
    }
    if provenance.observed_rows > provenance.row_cap {
        return Err(h7_invalid(
            "Geo H.7 observed rows exceed the declared provenance row cap",
            [
                ("observed_rows", provenance.observed_rows.to_string()),
                ("row_cap", provenance.row_cap.to_string()),
            ],
        ));
    }
    if provenance.acris_release_dt != CANON_GEO_H7_ACRIS_RELEASE_DT {
        return Err(h7_invalid(
            "Geo H.7 population rows drifted from the pinned ACRIS release",
            [
                ("actual", provenance.acris_release_dt.clone()),
                ("expected", CANON_GEO_H7_ACRIS_RELEASE_DT.to_string()),
            ],
        ));
    }
    if provenance.bridge_build_id != CANON_GEO_H7_BRIDGE_BUILD_ID {
        return Err(h7_invalid(
            "Geo H.7 population rows drifted from the pinned loan-property bridge build",
            [
                ("actual", provenance.bridge_build_id.clone()),
                ("expected", CANON_GEO_H7_BRIDGE_BUILD_ID.to_string()),
            ],
        ));
    }
    validate_h7_string("provenance.collateral_scope", &provenance.collateral_scope)?;
    if provenance.collateral_scope != CANON_GEO_H7_COLLATERAL_SCOPE {
        return Err(h7_invalid(
            "Geo H.7 provenance must state that truth is the NYC filed-collateral slice",
            [("collateral_scope", provenance.collateral_scope.clone())],
        ));
    }
    if provenance.amount_cents_quantization != CANON_GEO_H7_AMOUNT_CENTS_QUANTIZATION {
        return Err(h7_invalid(
            "Geo H.7 population rows changed the exact-cents quantization contract",
            [
                ("actual", provenance.amount_cents_quantization.clone()),
                (
                    "expected",
                    CANON_GEO_H7_AMOUNT_CENTS_QUANTIZATION.to_string(),
                ),
            ],
        ));
    }
    if provenance.round_amount_lattice_cents != CANON_GEO_H7_ROUND_AMOUNT_LATTICE_CENTS {
        return Err(h7_invalid(
            "Geo H.7 population rows changed the $100,000 cents lattice",
            [
                ("actual", provenance.round_amount_lattice_cents.to_string()),
                (
                    "expected",
                    CANON_GEO_H7_ROUND_AMOUNT_LATTICE_CENTS.to_string(),
                ),
            ],
        ));
    }
    if provenance.lender_match_transform != CANON_GEO_H7_LENDER_MATCH_TRANSFORM {
        return Err(h7_invalid(
            "Geo H.7 population rows changed the exact lender-name transform",
            [
                ("actual", provenance.lender_match_transform.clone()),
                ("expected", CANON_GEO_H7_LENDER_MATCH_TRANSFORM.to_string()),
            ],
        ));
    }
    validate_h7_mappluto_pins(&provenance.mappluto_releases)?;
    validate_h7_mappluto_pin(
        "primary_candidate_release",
        &provenance.primary_candidate_release,
    )?;
    if provenance.primary_candidate_release.release != CANON_GEO_H7_PRIMARY_MAPPLUTO_RELEASE {
        return Err(h7_invalid(
            "Geo H.7 primary solver population release drifted from the declared control release",
            [
                (
                    "actual",
                    provenance.primary_candidate_release.release.clone(),
                ),
                (
                    "expected",
                    CANON_GEO_H7_PRIMARY_MAPPLUTO_RELEASE.to_string(),
                ),
            ],
        ));
    }
    let release_keys = provenance
        .mappluto_releases
        .iter()
        .map(mappluto_pin_key)
        .collect::<BTreeSet<_>>();
    if !release_keys.contains(&mappluto_pin_key(&provenance.primary_candidate_release)) {
        return Err(h7_invalid(
            "Geo H.7 primary candidate release must be one of the pinned MapPLUTO releases",
            [(
                "primary_candidate_release",
                provenance.primary_candidate_release.release.clone(),
            )],
        ));
    }
    validate_h7_county_mapping(&provenance.filed_county_mapping)?;
    let mut source_hash_identities = BTreeSet::new();
    for hash in &provenance.source_hashes {
        validate_h7_string("source_hash.source", &hash.source)?;
        validate_h7_string("source_hash.hash_kind", &hash.hash_kind)?;
        validate_sha256("source_hash.sha256", &hash.sha256)?;
        if !source_hash_identities.insert((hash.source.clone(), hash.hash_kind.clone())) {
            return Err(h7_invalid(
                "Geo H.7 provenance repeats a source-hash identity",
                [
                    ("source", hash.source.clone()),
                    ("hash_kind", hash.hash_kind.clone()),
                ],
            ));
        }
    }
    let mut receipt_purposes = BTreeSet::new();
    let mut receipt_query_ids = BTreeSet::new();
    for receipt in &provenance.query_receipts {
        validate_h7_string("query_receipt.purpose", &receipt.purpose)?;
        if let Some(truth_plane) = receipt.truth_plane
            && !h7_truth_planes().contains(&truth_plane)
        {
            return Err(h7_invalid(
                "Geo H.7 query receipt truth_plane must be one controlling H.7 plane",
                [
                    ("purpose", receipt.purpose.clone()),
                    ("truth_plane", h7_plane_name(truth_plane).to_string()),
                ],
            ));
        }
        validate_h7_query_receipt_plane_semantics(receipt)?;
        if !receipt_purposes.insert(receipt.purpose.clone()) {
            return Err(h7_invalid(
                "Geo H.7 provenance repeats a query receipt purpose",
                [("purpose", receipt.purpose.clone())],
            ));
        }
        if let Some(query_id) = &receipt.query_id {
            validate_h7_string("query_receipt.query_id", query_id)?;
            if !receipt_query_ids.insert(query_id.clone()) {
                return Err(h7_invalid(
                    "Geo H.7 provenance repeats a query receipt id",
                    [
                        ("query_id", query_id.clone()),
                        ("purpose", receipt.purpose.clone()),
                    ],
                ));
            }
        }
        validate_h7_string("query_receipt.query_text_ref", &receipt.query_text_ref)?;
        validate_blake3_hash("query_receipt.query_blake3", &receipt.query_blake3)?;
        validate_h7_query_receipt_binding(provenance.result_mode, receipt)?;
        if receipt.row_cap == 0 {
            return Err(h7_invalid(
                "Geo H.7 query receipts require positive row caps",
                [("purpose", receipt.purpose.clone())],
            ));
        }
        if receipt.result_rows > receipt.row_cap {
            return Err(h7_invalid(
                "Geo H.7 query receipt result rows exceed their row cap",
                [
                    ("purpose", receipt.purpose.clone()),
                    ("result_rows", receipt.result_rows.to_string()),
                    ("row_cap", receipt.row_cap.to_string()),
                ],
            ));
        }
        let replay_fixture_without_query_id = provenance.result_mode == GeoH7ResultMode::Replay
            && receipt.disposition != GeoH7QueryDisposition::Cited
            && receipt.query_text_ref.starts_with("fixture:");
        let receipt_requires_query_id = provenance.result_mode == GeoH7ResultMode::Live
            || receipt.disposition == GeoH7QueryDisposition::Cited
            || (matches!(
                receipt.disposition,
                GeoH7QueryDisposition::Discarded | GeoH7QueryDisposition::Cancelled
            ) && !replay_fixture_without_query_id);
        if receipt_requires_query_id
            && receipt
                .query_id
                .as_deref()
                .is_none_or(|query_id| query_id.is_empty())
        {
            return Err(h7_invalid(
                "Geo H.7 live, cited, or retained non-fixture query receipts must preserve query ids",
                [("purpose", receipt.purpose.clone())],
            ));
        }
        if receipt.disposition == GeoH7QueryDisposition::Cited {
            if receipt.result_rows == 0 {
                return Err(h7_invalid(
                    "Geo H.7 cited query receipts must have nonzero result rows",
                    [("purpose", receipt.purpose.clone())],
                ));
            }
            if receipt
                .query_id
                .as_deref()
                .is_none_or(|query_id| query_id.is_empty())
            {
                return Err(h7_invalid(
                    "Geo H.7 cited query receipts must preserve query ids",
                    [("purpose", receipt.purpose.clone())],
                ));
            }
            if receipt.result_rows == receipt.row_cap {
                return Err(h7_invalid(
                    "Geo H.7 cited query receipt hit its row cap and may be truncated",
                    [
                        ("purpose", receipt.purpose.clone()),
                        ("row_cap", receipt.row_cap.to_string()),
                    ],
                ));
            }
        }
    }
    let mut external_receipt_ids = BTreeSet::new();
    for receipt in &provenance.external_receipts {
        validate_h7_string("external_receipt.receipt_id", &receipt.receipt_id)?;
        validate_h7_string("external_receipt.purpose", &receipt.purpose)?;
        if !external_receipt_ids.insert(receipt.receipt_id.clone()) {
            return Err(h7_invalid(
                "Geo H.7 provenance repeats an external receipt id",
                [("receipt_id", receipt.receipt_id.clone())],
            ));
        }
    }
    if provenance.result_mode == GeoH7ResultMode::Live {
        if provenance.observed_rows != input_rows {
            return Err(h7_invalid(
                "Geo H.7 live population rows must bind proof to the actual input payload",
                [
                    ("observed_rows", provenance.observed_rows.to_string()),
                    ("input_rows", input_rows.to_string()),
                ],
            ));
        }
        if provenance.observed_rows == 0 {
            return Err(h7_invalid(
                "Geo H.7 live population rows require nonzero fresh result rows",
                [("observed_rows", "0".to_string())],
            ));
        }
        if provenance.source_hashes.is_empty() {
            return Err(h7_invalid(
                "Geo H.7 live population rows require preserved source hashes",
                [("source_hashes", "0".to_string())],
            ));
        }
        let cited_receipts = provenance
            .query_receipts
            .iter()
            .filter(|receipt| receipt.disposition == GeoH7QueryDisposition::Cited)
            .collect::<Vec<_>>();
        if cited_receipts.is_empty() {
            return Err(h7_invalid(
                "Geo H.7 live population rows require at least one cited fresh query receipt",
                [("cited_query_receipts", "0".to_string())],
            ));
        }
        for receipt in cited_receipts {
            if receipt
                .query_id
                .as_deref()
                .is_none_or(|query_id| query_id.is_empty())
            {
                return Err(h7_invalid(
                    "Geo H.7 live cited query receipts must preserve query ids",
                    [("purpose", receipt.purpose.clone())],
                ));
            }
            if receipt.result_rows == receipt.row_cap {
                return Err(h7_invalid(
                    "Geo H.7 live cited query receipt hit its row cap and may be truncated",
                    [
                        ("purpose", receipt.purpose.clone()),
                        ("row_cap", receipt.row_cap.to_string()),
                    ],
                ));
            }
        }
        for receipt in &provenance.query_receipts {
            if receipt
                .query_id
                .as_deref()
                .is_none_or(|query_id| query_id.is_empty())
            {
                return Err(h7_invalid(
                    "Geo H.7 live query receipts must preserve query ids",
                    [("purpose", receipt.purpose.clone())],
                ));
            }
        }
    }
    if population_scope == GeoH7PopulationScope::RetainedComplete {
        if provenance.observed_rows != input_rows {
            return Err(h7_invalid(
                "Geo H.7 retained-complete population rows must bind provenance to the actual input payload",
                [
                    ("observed_rows", provenance.observed_rows.to_string()),
                    ("input_rows", input_rows.to_string()),
                ],
            ));
        }
        if provenance.source_hashes.is_empty() {
            return Err(h7_invalid(
                "Geo H.7 retained-complete population requires preserved source hashes",
                [("source_hashes", "0".to_string())],
            ));
        }
        let has_real_cited_receipt = provenance.query_receipts.iter().any(|receipt| {
            receipt.disposition == GeoH7QueryDisposition::Cited
                && receipt
                    .query_id
                    .as_deref()
                    .is_some_and(|query_id| !query_id.is_empty())
                && !receipt.query_text_ref.starts_with("fixture:")
                && (receipt.normalized_query_text.is_some()
                    || receipt
                        .query_text_ref
                        .contains(&format!("@blake3:{}", receipt.query_blake3)))
        });
        if !has_real_cited_receipt {
            return Err(h7_invalid(
                "Geo H.7 retained-complete population requires a non-fixture SQL-bound cited query receipt",
                [("cited_query_receipts", "0".to_string())],
            ));
        }
    }
    let mut discrepancy_subjects = BTreeSet::new();
    let known_receipt_ids = receipt_query_ids
        .into_iter()
        .chain(external_receipt_ids)
        .collect::<BTreeSet<_>>();
    for discrepancy in &provenance.empirical_discrepancies {
        validate_h7_string("empirical_discrepancy.subject", &discrepancy.subject)?;
        validate_h7_string(
            "empirical_discrepancy.archived_measurement",
            &discrepancy.archived_measurement,
        )?;
        validate_h7_string(
            "empirical_discrepancy.fresh_measurement",
            &discrepancy.fresh_measurement,
        )?;
        if !discrepancy_subjects.insert(discrepancy.subject.clone()) {
            return Err(h7_invalid(
                "Geo H.7 provenance repeats an empirical discrepancy subject",
                [("subject", discrepancy.subject.clone())],
            ));
        }
        let mut receipt_ids = BTreeSet::new();
        if discrepancy.receipt_ids.is_empty() {
            return Err(h7_invalid(
                "Geo H.7 empirical discrepancies require receipt ids",
                [("subject", discrepancy.subject.clone())],
            ));
        }
        for receipt_id in &discrepancy.receipt_ids {
            validate_h7_string("empirical_discrepancy.receipt_id", receipt_id)?;
            if !known_receipt_ids.contains(receipt_id) {
                return Err(h7_invalid(
                    "Geo H.7 empirical discrepancy cites an unregistered receipt id",
                    [
                        ("subject", discrepancy.subject.clone()),
                        ("receipt_id", receipt_id.clone()),
                    ],
                ));
            }
            if !receipt_ids.insert(receipt_id.clone()) {
                return Err(h7_invalid(
                    "Geo H.7 empirical discrepancy repeats a receipt id",
                    [
                        ("subject", discrepancy.subject.clone()),
                        ("receipt_id", receipt_id.clone()),
                    ],
                ));
            }
        }
    }
    Ok(())
}

fn validate_h7_live_legal_residuals(
    population_scope: GeoH7PopulationScope,
    provenance: &GeoH7PopulationProvenance,
) -> Result<(), GeoMaterializationError> {
    if population_scope != GeoH7PopulationScope::LiveComplete
        || provenance.result_mode != GeoH7ResultMode::Live
    {
        return Ok(());
    }
    for plane in h7_truth_planes() {
        let required_purpose = h7_legal_residual_receipt_purpose(plane)?;
        let has_residual_receipt = provenance.query_receipts.iter().any(|receipt| {
            receipt.disposition == GeoH7QueryDisposition::Cited
                && receipt.purpose == required_purpose
                && receipt.truth_plane == Some(plane)
                && receipt.result_rows > 0
        });
        if !has_residual_receipt {
            return Err(h7_invalid(
                "Geo H.7 LiveComplete rows require separate nonzero ACRIS candidate/legal residual receipts for each truth plane",
                [
                    ("truth_plane", h7_plane_name(plane).to_string()),
                    ("required_receipt_purpose", required_purpose.to_string()),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_h7_query_receipt_plane_semantics(
    receipt: &GeoH7QueryReceipt,
) -> Result<(), GeoMaterializationError> {
    let Some(expected_plane) = h7_legal_residual_receipt_plane(&receipt.purpose) else {
        return Ok(());
    };
    if receipt.truth_plane != Some(expected_plane) {
        return Err(h7_invalid(
            "Geo H.7 legal-residual query receipt purpose must match its truth_plane",
            [
                ("purpose", receipt.purpose.clone()),
                (
                    "expected_truth_plane",
                    h7_plane_name(expected_plane).to_string(),
                ),
                (
                    "actual_truth_plane",
                    receipt
                        .truth_plane
                        .map(h7_plane_name)
                        .unwrap_or("<missing>")
                        .to_string(),
                ),
            ],
        ));
    }
    Ok(())
}

fn validate_h7_query_receipt_binding(
    result_mode: GeoH7ResultMode,
    receipt: &GeoH7QueryReceipt,
) -> Result<(), GeoMaterializationError> {
    if let Some(query_text) = &receipt.normalized_query_text {
        if query_text.is_empty() {
            return Err(h7_invalid(
                "Geo H.7 query receipts with embedded normalized SQL must be non-empty",
                [("purpose", receipt.purpose.clone())],
            ));
        }
        let computed = blake3::hash(query_text.as_bytes()).to_hex().to_string();
        if computed != receipt.query_blake3 {
            return Err(h7_invalid(
                "Geo H.7 query receipt hash does not match embedded normalized SQL",
                [
                    ("purpose", receipt.purpose.clone()),
                    ("computed_blake3", computed),
                    ("query_blake3", receipt.query_blake3.clone()),
                ],
            ));
        }
    }

    let purpose_hash = blake3::hash(receipt.purpose.as_bytes())
        .to_hex()
        .to_string();
    let fixture_ref = receipt.query_text_ref.starts_with("fixture:");
    let content_addressed_ref = receipt
        .query_text_ref
        .contains(&format!("@blake3:{}", receipt.query_blake3));
    let has_embedded_verified_text = receipt.normalized_query_text.is_some();
    let fixture_only_replay = result_mode == GeoH7ResultMode::Replay
        && fixture_ref
        && receipt.disposition != GeoH7QueryDisposition::Cited;
    let requires_real_binding = !fixture_only_replay
        && (result_mode == GeoH7ResultMode::Live
            || receipt.disposition != GeoH7QueryDisposition::DiagnosticOnly
            || !fixture_ref);

    if receipt.query_blake3 == purpose_hash && requires_real_binding {
        return Err(h7_invalid(
            "Geo H.7 query receipt hash must bind SQL text, not the receipt purpose",
            [("purpose", receipt.purpose.clone())],
        ));
    }
    if requires_real_binding && fixture_ref {
        return Err(h7_invalid(
            "Geo H.7 live or non-diagnostic query receipts cannot use fixture query-text refs",
            [("purpose", receipt.purpose.clone())],
        ));
    }
    if requires_real_binding && !content_addressed_ref && !has_embedded_verified_text {
        return Err(h7_invalid(
            "Geo H.7 query receipts must carry embedded normalized SQL or a content-addressed query-text ref",
            [
                ("purpose", receipt.purpose.clone()),
                ("query_text_ref", receipt.query_text_ref.clone()),
            ],
        ));
    }
    if !requires_real_binding
        && !fixture_ref
        && !content_addressed_ref
        && !has_embedded_verified_text
    {
        return Err(h7_invalid(
            "Geo H.7 replay diagnostic query receipts must be fixture-scoped or content-addressed",
            [
                ("purpose", receipt.purpose.clone()),
                ("query_text_ref", receipt.query_text_ref.clone()),
            ],
        ));
    }
    Ok(())
}

fn validate_h7_denominators(
    denominators: &[GeoH7PlaneDenominator],
) -> Result<BTreeMap<GeoTruthPlane, GeoH7PlaneDenominator>, GeoMaterializationError> {
    let mut by_plane = BTreeMap::new();
    for denominator in denominators {
        if !h7_truth_planes().contains(&denominator.truth_plane) {
            return Err(h7_invalid(
                "Geo H.7 denominators may only name controlling H.7 truth planes",
                [(
                    "truth_plane",
                    h7_plane_name(denominator.truth_plane).to_string(),
                )],
            ));
        }
        if denominator.eligible_loans == 0 || denominator.accepted_loans == 0 {
            return Err(h7_invalid(
                "Geo H.7 denominator rows must be nonzero before they can be cited",
                [
                    (
                        "truth_plane",
                        h7_plane_name(denominator.truth_plane).to_string(),
                    ),
                    ("eligible_loans", denominator.eligible_loans.to_string()),
                    ("accepted_loans", denominator.accepted_loans.to_string()),
                ],
            ));
        }
        if denominator.selected_multi_parcel_loans == 0
            || denominator.selected_multi_parcel_loans > denominator.accepted_loans
        {
            return Err(h7_invalid(
                "Geo H.7 selected multi-parcel denominator must be nonzero and bounded by accepted loans",
                [
                    (
                        "truth_plane",
                        h7_plane_name(denominator.truth_plane).to_string(),
                    ),
                    (
                        "selected_multi_parcel_loans",
                        denominator.selected_multi_parcel_loans.to_string(),
                    ),
                    ("accepted_loans", denominator.accepted_loans.to_string()),
                ],
            ));
        }
        let legal_confirmed_candidate_loans = denominator
            .accepted_loans
            .checked_add(denominator.ambiguous_loans)
            .ok_or_else(|| h7_overflow("legal_confirmed_candidate_loans"))?;
        if denominator.legal_confirmed_candidate_loans != legal_confirmed_candidate_loans {
            return Err(h7_invalid(
                "Geo H.7 legal-confirmed candidate denominator must equal accepted plus ambiguous loans",
                [
                    (
                        "truth_plane",
                        h7_plane_name(denominator.truth_plane).to_string(),
                    ),
                    (
                        "legal_confirmed_candidate_loans",
                        denominator.legal_confirmed_candidate_loans.to_string(),
                    ),
                    (
                        "accepted_plus_ambiguous",
                        legal_confirmed_candidate_loans.to_string(),
                    ),
                ],
            ));
        }
        let candidate_loans = legal_confirmed_candidate_loans
            .checked_add(denominator.candidate_no_legal_confirmation_loans)
            .ok_or_else(|| h7_overflow("candidate_loans"))?;
        if denominator.candidate_loans != candidate_loans {
            return Err(h7_invalid(
                "Geo H.7 candidate denominator must equal legal-confirmed plus no-legal-confirmation loans",
                [
                    (
                        "truth_plane",
                        h7_plane_name(denominator.truth_plane).to_string(),
                    ),
                    ("candidate_loans", denominator.candidate_loans.to_string()),
                    (
                        "legal_confirmed_plus_no_legal_confirmation",
                        candidate_loans.to_string(),
                    ),
                ],
            ));
        }
        let classified_loans = candidate_loans
            .checked_add(denominator.no_candidate_loans)
            .ok_or_else(|| h7_overflow("eligible_loans"))?;
        if denominator.eligible_loans != classified_loans {
            return Err(h7_invalid(
                "Geo H.7 denominator algebra must reconcile independently per truth plane",
                [
                    (
                        "truth_plane",
                        h7_plane_name(denominator.truth_plane).to_string(),
                    ),
                    ("eligible_loans", denominator.eligible_loans.to_string()),
                    ("candidate_plus_no_candidate", classified_loans.to_string()),
                ],
            ));
        }
        if by_plane
            .insert(denominator.truth_plane, denominator.clone())
            .is_some()
        {
            return Err(h7_invalid(
                "Geo H.7 denominator rows repeat a truth plane",
                [(
                    "truth_plane",
                    h7_plane_name(denominator.truth_plane).to_string(),
                )],
            ));
        }
    }
    for plane in h7_truth_planes() {
        if !by_plane.contains_key(&plane) {
            return Err(h7_invalid(
                "Geo H.7 denominators must state reach independently for both planes",
                [("missing_truth_plane", h7_plane_name(plane).to_string())],
            ));
        }
    }
    Ok(by_plane)
}

fn materialize_h7_case(
    row: &GeoH7PopulationWarehouseRow,
    max_assignments: u64,
    max_materialized_models: u64,
) -> Result<GeoH7PopulationCaseArtifact, GeoMaterializationError> {
    validate_h7_string("loan_key", &row.loan_key)?;
    validate_h7_string("document_id", &row.document_id)?;
    validate_h7_string("property_state", &row.property_state)?;
    if row.property_state != CANON_GEO_H7_PROPERTY_STATE {
        return Err(h7_invalid(
            "Geo H.7 rows require raw filed property_state NY",
            [
                ("loan_key", row.loan_key.clone()),
                ("property_state", row.property_state.clone()),
            ],
        ));
    }
    validate_h7_string("filed_county", &row.filed_county)?;
    if let Some(geocoded_county_fips) = &row.geocoded_county_fips {
        validate_h7_string("geocoded_county_fips", geocoded_county_fips)?;
    }
    validate_h7_string("doc_type", &row.doc_type)?;
    validate_h7_string("originationdate", &row.originationdate)?;
    validate_h7_string("reach_reason", &row.reach_reason)?;
    validate_h7_mappluto_pin("candidate_release", &row.candidate_release)?;
    if !h7_truth_planes().contains(&row.truth_plane) {
        return Err(h7_invalid(
            "Geo H.7 population rows may only use controlling H.7 truth planes",
            [("truth_plane", h7_plane_name(row.truth_plane).to_string())],
        ));
    }
    if filed_county_borough(&row.filed_county) != Some(row.filed_borough) {
        return Err(h7_invalid(
            "Geo H.7 row filed county does not map to the declared ACRIS borough",
            [
                ("filed_county", row.filed_county.clone()),
                ("filed_borough", row.filed_borough.to_string()),
            ],
        ));
    }
    let accepted_borough_edges = canonical_h7_borough_edges(row)?;
    if row.filed_borough != row.legal_borough {
        return Err(h7_invalid(
            "Geo H.7 row conflates filed county and ACRIS legal borough without agreement",
            [
                ("filed_borough", row.filed_borough.to_string()),
                ("legal_borough", row.legal_borough.to_string()),
            ],
        ));
    }
    validate_h7_singular_counts(row)?;
    validate_h7_amount_plane(row)?;
    validate_h7_lender_fields(row)?;

    let truth_parcels = sorted_distinct_nonempty("truth_parcels", &row.truth_parcels)?;
    if truth_parcels.len() < 2 {
        return Err(h7_invalid(
            "Geo H.7 E4 population rows require multi-parcel truth sets",
            [
                ("loan_key", row.loan_key.clone()),
                ("truth_parcels", truth_parcels.len().to_string()),
            ],
        ));
    }
    let candidate_parcels = sorted_distinct("candidate_parcels", &row.candidate_parcels)?;
    let computed_reach = h7_reach_status(&truth_parcels, &candidate_parcels);
    if computed_reach != row.reach_status {
        return Err(h7_invalid(
            "Geo H.7 row declared candidate reach does not match truth/candidate sets",
            [
                ("loan_key", row.loan_key.clone()),
                ("declared", h7_reach_name(row.reach_status).to_string()),
                ("computed", h7_reach_name(computed_reach).to_string()),
            ],
        ));
    }
    let source_records = canonical_h7_source_records(row, &truth_parcels, &candidate_parcels)?;

    let case_id = h7_case_id(
        row.truth_plane,
        &row.loan_key,
        &row.document_id,
        &row.candidate_release,
        &row.property_state,
    );
    let subject_id = h7_subject_id(
        row.truth_plane,
        &row.loan_key,
        &row.document_id,
        &row.property_state,
    );
    let evidence_request = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        universe: GeoCompositionUniverse {
            parcels: candidate_parcels.clone(),
            buildings: Vec::new(),
        },
        contracts: Vec::new(),
        observations: Vec::new(),
        max_assignments,
        max_materialized_models,
    };
    if !candidate_parcels.is_empty() {
        compile_evidence(&evidence_request).map_err(GeoMaterializationError::from)?;
    }

    Ok(GeoH7PopulationCaseArtifact {
        subject_id,
        case_id,
        loan_key: row.loan_key.clone(),
        document_id: row.document_id.clone(),
        truth_plane: row.truth_plane,
        association_plane: row.association_plane,
        candidate_release: row.candidate_release.clone(),
        property_state: row.property_state.clone(),
        filed_county: row.filed_county.clone(),
        filed_borough: row.filed_borough,
        legal_borough: row.legal_borough,
        accepted_borough_edges,
        geocoded_county_fips: canonical_optional_string(&row.geocoded_county_fips),
        doc_type: row.doc_type.clone(),
        originationdate: row.originationdate.clone(),
        amount_cents: row.amount_cents,
        is_round_100k_lattice: row.is_round_100k_lattice,
        originatorname: canonical_optional_string(&row.originatorname),
        originator_match_text: canonical_optional_string(&row.originator_match_text),
        lender_match_text: canonical_optional_string(&row.lender_match_text),
        lender_party_type: canonical_optional_string(&row.lender_party_type),
        truth_parcels,
        candidate_parcels,
        reach_status: row.reach_status,
        reach_reason: row.reach_reason.clone(),
        source_records,
    })
}

fn validate_h7_singular_counts(
    row: &GeoH7PopulationWarehouseRow,
) -> Result<(), GeoMaterializationError> {
    let counts = &row.loan_field_distinct_counts;
    validate_h7_optional_count(
        row,
        "originatorname",
        &row.originatorname,
        counts.originatorname,
    )?;
    validate_h7_optional_count(
        row,
        "originator_match_text",
        &row.originator_match_text,
        counts.originator_match_text,
    )?;
    if counts.originationdate != 1 {
        return Err(h7_invalid(
            "Geo H.7 row has ambiguous or missing origination dates at loan grain",
            [
                ("loan_key", row.loan_key.clone()),
                (
                    "distinct_originationdate",
                    counts.originationdate.to_string(),
                ),
            ],
        ));
    }
    if counts.originalloanamount != 1 {
        return Err(h7_invalid(
            "Geo H.7 row has ambiguous or missing original loan amounts at loan grain",
            [
                ("loan_key", row.loan_key.clone()),
                (
                    "distinct_originalloanamount",
                    counts.originalloanamount.to_string(),
                ),
            ],
        ));
    }
    if row.truth_plane == GeoTruthPlane::RoundExactLenderParty && counts.originatorname != 1 {
        return Err(h7_invalid(
            "Geo H.7 round exact-lender rows require exactly one originator name",
            [
                ("loan_key", row.loan_key.clone()),
                ("distinct_originatorname", counts.originatorname.to_string()),
            ],
        ));
    }
    if row.truth_plane == GeoTruthPlane::RoundExactLenderParty && counts.originator_match_text != 1
    {
        return Err(h7_invalid(
            "Geo H.7 round exact-lender rows require exactly one originator match text",
            [
                ("loan_key", row.loan_key.clone()),
                (
                    "distinct_originator_match_text",
                    counts.originator_match_text.to_string(),
                ),
            ],
        ));
    }
    if counts.filed_borough == 0 {
        return Err(h7_invalid(
            "Geo H.7 row has no mapped filed borough at loan grain",
            [
                ("loan_key", row.loan_key.clone()),
                ("distinct_filed_borough", counts.filed_borough.to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_h7_amount_plane(
    row: &GeoH7PopulationWarehouseRow,
) -> Result<(), GeoMaterializationError> {
    if row.amount_cents == 0 {
        return Err(h7_invalid(
            "Geo H.7 rows require a nonzero exact-cents loan amount",
            [("loan_key", row.loan_key.clone())],
        ));
    }
    let is_round_lattice = row
        .amount_cents
        .is_multiple_of(CANON_GEO_H7_ROUND_AMOUNT_LATTICE_CENTS);
    if is_round_lattice != row.is_round_100k_lattice {
        return Err(h7_invalid(
            "Geo H.7 row declared roundness does not match the $100,000 cents lattice",
            [
                ("loan_key", row.loan_key.clone()),
                ("amount_cents", row.amount_cents.to_string()),
            ],
        ));
    }
    match row.truth_plane {
        GeoTruthPlane::NonRoundAmountDateLegalBorough if is_round_lattice => Err(h7_invalid(
            "Geo H.7 non-round plane cannot contain $100,000-lattice amounts",
            [
                ("loan_key", row.loan_key.clone()),
                ("amount_cents", row.amount_cents.to_string()),
            ],
        )),
        GeoTruthPlane::RoundExactLenderParty if !is_round_lattice => Err(h7_invalid(
            "Geo H.7 round exact-lender plane requires $100,000-lattice amounts",
            [
                ("loan_key", row.loan_key.clone()),
                ("amount_cents", row.amount_cents.to_string()),
            ],
        )),
        _ => Ok(()),
    }
}

fn validate_h7_lender_fields(
    row: &GeoH7PopulationWarehouseRow,
) -> Result<(), GeoMaterializationError> {
    if row.truth_plane != GeoTruthPlane::RoundExactLenderParty {
        return Ok(());
    }
    let originator =
        required_optional_string(row, "originator_match_text", &row.originator_match_text)?;
    let lender = required_optional_string(row, "lender_match_text", &row.lender_match_text)?;
    if originator != lender {
        return Err(h7_invalid(
            "Geo H.7 round exact-lender row lacks exact transformed lender agreement",
            [
                ("loan_key", row.loan_key.clone()),
                ("originator_match_text", originator),
                ("lender_match_text", lender),
            ],
        ));
    }
    let party_type = required_optional_string(row, "lender_party_type", &row.lender_party_type)?;
    let expected_party_type = lender_party_type_for_doc_type(&row.doc_type).ok_or_else(|| {
        h7_invalid(
            "Geo H.7 round exact-lender row has an unsupported mortgage document type",
            [
                ("loan_key", row.loan_key.clone()),
                ("doc_type", row.doc_type.clone()),
            ],
        )
    })?;
    if party_type != expected_party_type {
        return Err(h7_invalid(
            "Geo H.7 round exact-lender row has the wrong ACRIS party role",
            [
                ("loan_key", row.loan_key.clone()),
                ("doc_type", row.doc_type.clone()),
                ("actual_party_type", party_type),
                ("expected_party_type", expected_party_type.to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_h7_mappluto_pin(
    field: &str,
    pin: &GeoH7MapplutoReleasePin,
) -> Result<(), GeoMaterializationError> {
    validate_h7_string(&format!("{field}.release"), &pin.release)?;
    validate_h7_string(&format!("{field}.release_dt"), &pin.release_dt)?;
    validate_h7_string(&format!("{field}.variant"), &pin.variant)?;
    validate_h7_string(
        &format!("{field}.geometry_contract_version"),
        &pin.geometry_contract_version,
    )?;
    if !is_h7_mappluto_pin(pin) {
        return Err(h7_invalid(
            "Geo H.7 candidate sets must name one pinned MapPLUTO release/variant/contract",
            [
                ("field", field.to_string()),
                ("release", pin.release.clone()),
                ("release_dt", pin.release_dt.clone()),
                ("variant", pin.variant.clone()),
                (
                    "geometry_contract_version",
                    pin.geometry_contract_version.clone(),
                ),
            ],
        ));
    }
    Ok(())
}

fn is_h7_mappluto_pin(pin: &GeoH7MapplutoReleasePin) -> bool {
    matches!(
        (
            pin.release.as_str(),
            pin.release_dt.as_str(),
            pin.variant.as_str(),
            pin.geometry_contract_version.as_str(),
        ),
        (
            "26v1",
            "2026-05-01",
            "shoreline_clipped",
            CANON_GEO_H7_MAPPLUTO_GEOMETRY_CONTRACT_VERSION
        ) | (
            "26v2",
            "2026-08-01",
            "shoreline_clipped",
            CANON_GEO_H7_MAPPLUTO_GEOMETRY_CONTRACT_VERSION
        )
    )
}

fn mappluto_pin_key(pin: &GeoH7MapplutoReleasePin) -> (String, String, String, String) {
    (
        pin.release.clone(),
        pin.release_dt.clone(),
        pin.variant.clone(),
        pin.geometry_contract_version.clone(),
    )
}

fn validate_h7_mappluto_pins(
    pins: &[GeoH7MapplutoReleasePin],
) -> Result<(), GeoMaterializationError> {
    let mut seen = BTreeSet::new();
    for pin in pins {
        validate_h7_mappluto_pin("mappluto_release", pin)?;
        if !seen.insert(mappluto_pin_key(pin)) {
            return Err(h7_invalid(
                "Geo H.7 provenance repeats a MapPLUTO release pin",
                [
                    ("release", pin.release.clone()),
                    ("release_dt", pin.release_dt.clone()),
                    ("variant", pin.variant.clone()),
                ],
            ));
        }
    }
    let actual = pins
        .iter()
        .map(|pin| {
            (
                pin.release.as_str(),
                pin.release_dt.as_str(),
                pin.variant.as_str(),
                pin.geometry_contract_version.as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    let expected = [
        (
            "26v1",
            "2026-05-01",
            "shoreline_clipped",
            CANON_GEO_H7_MAPPLUTO_GEOMETRY_CONTRACT_VERSION,
        ),
        (
            "26v2",
            "2026-08-01",
            "shoreline_clipped",
            CANON_GEO_H7_MAPPLUTO_GEOMETRY_CONTRACT_VERSION,
        ),
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(h7_invalid(
            "Geo H.7 population rows drifted from the pinned MapPLUTO release set",
            [
                ("actual_pins", pins.len().to_string()),
                ("expected_pins", expected.len().to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_h7_county_mapping(
    mapping: &[GeoH7FiledCountyMapping],
) -> Result<(), GeoMaterializationError> {
    let mut by_county = BTreeMap::new();
    for row in mapping {
        validate_h7_string("filed_county_mapping.filed_county", &row.filed_county)?;
        if let Some(prior_borough) = by_county.insert(row.filed_county.clone(), row.acris_borough) {
            return Err(h7_invalid(
                "Geo H.7 filed-county mapping repeats a filed county",
                [
                    ("filed_county", row.filed_county.clone()),
                    ("prior_borough", prior_borough.to_string()),
                    ("current_borough", row.acris_borough.to_string()),
                ],
            ));
        }
    }
    let actual = mapping
        .iter()
        .map(|row| (row.filed_county.as_str(), row.acris_borough))
        .collect::<BTreeSet<_>>();
    let expected = [
        ("NEW YORK", 1),
        ("MANHATTAN", 1),
        ("NY061", 1),
        ("BRONX", 2),
        ("KINGS", 3),
        ("BROOKLYN", 3),
        ("QUEENS", 4),
        ("RICHMOND", 5),
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(h7_invalid(
            "Geo H.7 filed-county mapping drifted from the controlling Appendix H.7 map",
            [
                ("actual_rows", mapping.len().to_string()),
                ("expected_rows", expected.len().to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_h7_population_scope(
    scope: GeoH7PopulationScope,
    result_mode: GeoH7ResultMode,
    pinned_release_count: u64,
    denominators: &BTreeMap<GeoTruthPlane, GeoH7PlaneDenominator>,
    cases: &[GeoH7PopulationCaseArtifact],
) -> Result<(), GeoMaterializationError> {
    match (scope, result_mode) {
        (GeoH7PopulationScope::FixtureSubset, GeoH7ResultMode::Replay)
        | (GeoH7PopulationScope::RetainedComplete, GeoH7ResultMode::Replay)
        | (GeoH7PopulationScope::LiveComplete, GeoH7ResultMode::Live) => {}
        _ => {
            return Err(h7_invalid(
                "Geo H.7 population scope must agree with provenance result mode",
                [
                    (
                        "population_scope",
                        h7_population_scope_name(scope).to_string(),
                    ),
                    ("result_mode", h7_result_mode_name(result_mode).to_string()),
                ],
            ));
        }
    }

    let unique_loans_by_plane = unique_h7_loans_by_plane(cases);
    let mut total_selected = 0_u64;
    let mut total_actual_unique = 0_u64;
    for plane in h7_truth_planes() {
        let denominator = denominators.get(&plane).ok_or_else(|| {
            h7_invalid(
                "Geo H.7 completeness scope lacks a denominator for one truth plane",
                [("truth_plane", h7_plane_name(plane).to_string())],
            )
        })?;
        if denominator.selected_multi_parcel_loans == 0 {
            return Err(h7_invalid(
                "Geo H.7 selected multi-parcel denominator must be nonzero for each truth plane",
                [("truth_plane", h7_plane_name(plane).to_string())],
            ));
        }
        if denominator.selected_multi_parcel_loans > denominator.accepted_loans {
            return Err(h7_invalid(
                "Geo H.7 selected multi-parcel loans cannot exceed accepted loans",
                [
                    ("truth_plane", h7_plane_name(plane).to_string()),
                    (
                        "selected_multi_parcel_loans",
                        denominator.selected_multi_parcel_loans.to_string(),
                    ),
                    ("accepted_loans", denominator.accepted_loans.to_string()),
                ],
            ));
        }
        let actual_unique = unique_loans_by_plane
            .get(&plane)
            .map(|loans| len_u64_set(loans, "selected_multi_parcel_loans"))
            .transpose()?
            .unwrap_or(0);
        if actual_unique != denominator.selected_multi_parcel_loans {
            return Err(h7_invalid(
                "Geo H.7 selected multi-parcel denominator must match materialized loan subjects",
                [
                    ("truth_plane", h7_plane_name(plane).to_string()),
                    ("actual_unique_subjects", actual_unique.to_string()),
                    (
                        "selected_multi_parcel_loans",
                        denominator.selected_multi_parcel_loans.to_string(),
                    ),
                ],
            ));
        }
        total_selected = total_selected
            .checked_add(denominator.selected_multi_parcel_loans)
            .ok_or_else(|| h7_overflow("selected_multi_parcel_loans"))?;
        total_actual_unique = total_actual_unique
            .checked_add(actual_unique)
            .ok_or_else(|| h7_overflow("actual_unique_subjects"))?;
    }
    if total_actual_unique != total_selected {
        return Err(h7_invalid(
            "Geo H.7 selected multi-parcel totals must reconcile",
            [
                ("actual_unique_subjects", total_actual_unique.to_string()),
                ("selected_multi_parcel_loans", total_selected.to_string()),
            ],
        ));
    }
    let release_row_count = len_u64(cases, "release_rows")?;
    let expected_release_rows = total_selected
        .checked_mul(pinned_release_count)
        .ok_or_else(|| h7_overflow("expected_release_rows"))?;
    match scope {
        GeoH7PopulationScope::FixtureSubset => {
            if total_selected >= CANON_GEO_H7_COMPLETE_MULTI_PARCEL_LOANS {
                return Err(h7_invalid(
                    "Geo H.7 fixture subsets are non-gate proof and cannot claim complete H.7 scale",
                    [
                        ("selected_multi_parcel_loans", total_selected.to_string()),
                        (
                            "complete_multi_parcel_loans",
                            CANON_GEO_H7_COMPLETE_MULTI_PARCEL_LOANS.to_string(),
                        ),
                    ],
                ));
            }
        }
        GeoH7PopulationScope::RetainedComplete => {
            require_complete_h7_plane_count(
                denominators,
                GeoTruthPlane::NonRoundAmountDateLegalBorough,
                CANON_GEO_H7_COMPLETE_NON_ROUND_MULTI_PARCEL_LOANS,
            )?;
            require_complete_h7_plane_count(
                denominators,
                GeoTruthPlane::RoundExactLenderParty,
                CANON_GEO_H7_COMPLETE_ROUND_MULTI_PARCEL_LOANS,
            )?;
            if total_selected != CANON_GEO_H7_COMPLETE_MULTI_PARCEL_LOANS {
                return Err(h7_invalid(
                    "Geo H.7 RetainedComplete claim must preserve the retained 49-subject H.7 count",
                    [
                        ("selected_multi_parcel_loans", total_selected.to_string()),
                        (
                            "expected",
                            CANON_GEO_H7_COMPLETE_MULTI_PARCEL_LOANS.to_string(),
                        ),
                    ],
                ));
            }
            if release_row_count != CANON_GEO_H7_COMPLETE_RELEASE_ROWS {
                return Err(h7_invalid(
                    "Geo H.7 RetainedComplete claim must preserve exactly 98 release-run rows",
                    [
                        ("release_rows", release_row_count.to_string()),
                        ("expected", CANON_GEO_H7_COMPLETE_RELEASE_ROWS.to_string()),
                    ],
                ));
            }
        }
        GeoH7PopulationScope::LiveComplete => {
            if release_row_count != expected_release_rows {
                return Err(h7_invalid(
                    "Geo H.7 live complete population rows must equal selected subjects times pinned releases",
                    [
                        ("release_rows", release_row_count.to_string()),
                        ("selected_multi_parcel_loans", total_selected.to_string()),
                        ("pinned_release_count", pinned_release_count.to_string()),
                        ("expected_release_rows", expected_release_rows.to_string()),
                    ],
                ));
            }
        }
    }
    Ok(())
}

fn require_complete_h7_plane_count(
    denominators: &BTreeMap<GeoTruthPlane, GeoH7PlaneDenominator>,
    plane: GeoTruthPlane,
    expected: u64,
) -> Result<(), GeoMaterializationError> {
    let actual = denominators
        .get(&plane)
        .map(|denominator| denominator.selected_multi_parcel_loans)
        .unwrap_or(0);
    if actual != expected {
        return Err(h7_invalid(
            "Geo H.7 RetainedComplete selected multi-parcel count drifted",
            [
                ("truth_plane", h7_plane_name(plane).to_string()),
                ("actual", actual.to_string()),
                ("expected", expected.to_string()),
            ],
        ));
    }
    Ok(())
}

fn summarize_h7_population(
    population_scope: GeoH7PopulationScope,
    cases: &[GeoH7PopulationCaseArtifact],
    denominators: &BTreeMap<GeoTruthPlane, GeoH7PlaneDenominator>,
    solver_population_subjects: usize,
) -> Result<GeoH7PopulationSummary, GeoMaterializationError> {
    let mut summaries = BTreeMap::new();
    for (plane, denominator) in denominators {
        summaries.insert(
            *plane,
            GeoH7TruthPlaneSummary {
                truth_plane: *plane,
                eligible_loans: denominator.eligible_loans,
                candidate_loans: denominator.candidate_loans,
                legal_confirmed_candidate_loans: denominator.legal_confirmed_candidate_loans,
                accepted_loans: denominator.accepted_loans,
                ambiguous_loans: denominator.ambiguous_loans,
                candidate_no_legal_confirmation_loans: denominator
                    .candidate_no_legal_confirmation_loans,
                no_candidate_loans: denominator.no_candidate_loans,
                selected_multi_parcel_loans: denominator.selected_multi_parcel_loans,
                materialized_case_rows: 0,
                materialized_unique_accepted_loans: 0,
                candidate_reach_full_cases: 0,
                candidate_reach_partial_cases: 0,
                candidate_reach_none_cases: 0,
                truth_parcels: 0,
                candidate_parcels: 0,
            },
        );
    }
    let mut strata = BTreeMap::new();
    for case in cases {
        let summary = summaries.get_mut(&case.truth_plane).ok_or_else(|| {
            h7_invalid(
                "Geo H.7 case lacks an independent denominator for its truth plane",
                [("truth_plane", h7_plane_name(case.truth_plane).to_string())],
            )
        })?;
        h7_increment(
            &mut summary.materialized_case_rows,
            "summary.materialized_case_rows",
        )?;
        match case.reach_status {
            GeoH7CandidateReachStatus::Full => h7_increment(
                &mut summary.candidate_reach_full_cases,
                "summary.candidate_reach_full_cases",
            )?,
            GeoH7CandidateReachStatus::Partial => h7_increment(
                &mut summary.candidate_reach_partial_cases,
                "summary.candidate_reach_partial_cases",
            )?,
            GeoH7CandidateReachStatus::None => h7_increment(
                &mut summary.candidate_reach_none_cases,
                "summary.candidate_reach_none_cases",
            )?,
        }
        summary.truth_parcels = summary
            .truth_parcels
            .checked_add(len_u64(&case.truth_parcels, "truth_parcels")?)
            .ok_or_else(|| h7_overflow("truth_parcels"))?;
        summary.candidate_parcels = summary
            .candidate_parcels
            .checked_add(len_u64(&case.candidate_parcels, "candidate_parcels")?)
            .ok_or_else(|| h7_overflow("candidate_parcels"))?;

        let stratum_key = (
            case.truth_plane,
            case.association_plane,
            case.candidate_release.clone(),
        );
        let stratum = strata
            .entry(stratum_key)
            .or_insert_with(|| GeoH7PopulationStratumSummary {
                truth_plane: case.truth_plane,
                association_plane: case.association_plane,
                candidate_release: case.candidate_release.clone(),
                materialized_case_rows: 0,
                materialized_unique_accepted_loans: 0,
                candidate_reach_full_cases: 0,
                candidate_reach_partial_cases: 0,
                candidate_reach_none_cases: 0,
                truth_parcels: 0,
                candidate_parcels: 0,
            });
        h7_increment(
            &mut stratum.materialized_case_rows,
            "stratum.materialized_case_rows",
        )?;
        match case.reach_status {
            GeoH7CandidateReachStatus::Full => h7_increment(
                &mut stratum.candidate_reach_full_cases,
                "stratum.candidate_reach_full_cases",
            )?,
            GeoH7CandidateReachStatus::Partial => h7_increment(
                &mut stratum.candidate_reach_partial_cases,
                "stratum.candidate_reach_partial_cases",
            )?,
            GeoH7CandidateReachStatus::None => h7_increment(
                &mut stratum.candidate_reach_none_cases,
                "stratum.candidate_reach_none_cases",
            )?,
        }
        stratum.truth_parcels = stratum
            .truth_parcels
            .checked_add(len_u64(&case.truth_parcels, "truth_parcels")?)
            .ok_or_else(|| h7_overflow("truth_parcels"))?;
        stratum.candidate_parcels = stratum
            .candidate_parcels
            .checked_add(len_u64(&case.candidate_parcels, "candidate_parcels")?)
            .ok_or_else(|| h7_overflow("candidate_parcels"))?;
    }
    let unique_loans_by_plane = unique_h7_loans_by_plane(cases);
    let unique_loans_by_stratum = unique_h7_loans_by_stratum(cases);
    for (plane, unique_loans) in unique_loans_by_plane {
        let summary = summaries.get_mut(&plane).ok_or_else(|| {
            h7_invalid(
                "Geo H.7 case lacks an independent denominator for its truth plane",
                [("truth_plane", h7_plane_name(plane).to_string())],
            )
        })?;
        summary.materialized_unique_accepted_loans = len_u64_set(&unique_loans, "unique_loans")?;
    }
    for (stratum_key, unique_loans) in unique_loans_by_stratum {
        let stratum = strata.get_mut(&stratum_key).ok_or_else(|| {
            h7_invalid(
                "Geo H.7 stratum summary disappeared during unique-loan accounting",
                [("truth_plane", h7_plane_name(stratum_key.0).to_string())],
            )
        })?;
        stratum.materialized_unique_accepted_loans = len_u64_set(&unique_loans, "unique_loans")?;
    }
    for summary in summaries.values() {
        if summary.materialized_unique_accepted_loans > summary.accepted_loans {
            return Err(h7_invalid(
                "Geo H.7 unique materialized accepted loans cannot exceed accepted loans for a truth plane",
                [
                    (
                        "truth_plane",
                        h7_plane_name(summary.truth_plane).to_string(),
                    ),
                    (
                        "materialized_unique_accepted_loans",
                        summary.materialized_unique_accepted_loans.to_string(),
                    ),
                    ("accepted_loans", summary.accepted_loans.to_string()),
                ],
            ));
        }
    }
    Ok(GeoH7PopulationSummary {
        population_scope,
        source_rows: len_u64(cases, "source_rows")?,
        materialized_case_rows: len_u64(cases, "materialized_case_rows")?,
        materialized_unique_accepted_loans: len_u64_set(
            &cases
                .iter()
                .map(|case| case.loan_key.clone())
                .collect::<BTreeSet<_>>(),
            "materialized_unique_accepted_loans",
        )?,
        solver_population_subjects: u64::try_from(solver_population_subjects)
            .map_err(|_| h7_overflow("solver_population_subjects"))?,
        truth_planes: summaries.into_values().collect(),
        strata: strata.into_values().collect(),
    })
}

fn canonicalize_h7_provenance(provenance: &GeoH7PopulationProvenance) -> GeoH7PopulationProvenance {
    let mut canonical = provenance.clone();
    canonical.mappluto_releases.sort_by(|left, right| {
        (
            &left.release,
            &left.release_dt,
            &left.variant,
            &left.geometry_contract_version,
        )
            .cmp(&(
                &right.release,
                &right.release_dt,
                &right.variant,
                &right.geometry_contract_version,
            ))
    });
    canonical
        .filed_county_mapping
        .sort_by(|left, right| left.filed_county.cmp(&right.filed_county));
    canonical.source_hashes.sort_by(|left, right| {
        (&left.source, &left.hash_kind).cmp(&(&right.source, &right.hash_kind))
    });
    canonical
        .query_receipts
        .sort_by(|left, right| left.purpose.cmp(&right.purpose));
    canonical
        .empirical_discrepancies
        .sort_by(|left, right| left.subject.cmp(&right.subject));
    for discrepancy in &mut canonical.empirical_discrepancies {
        discrepancy.receipt_ids.sort();
    }
    canonical
}

fn validate_h7_release_coverage(
    provenance: &GeoH7PopulationProvenance,
    accepted_loans: &BTreeMap<String, GeoH7AcceptedLoanTruthKey>,
    releases_by_loan: &BTreeMap<String, BTreeSet<(String, String, String, String)>>,
) -> Result<(), GeoMaterializationError> {
    let expected_releases = provenance
        .mappluto_releases
        .iter()
        .map(mappluto_pin_key)
        .collect::<BTreeSet<_>>();
    for loan_key in accepted_loans.keys() {
        let actual_releases = releases_by_loan.get(loan_key).ok_or_else(|| {
            h7_invalid(
                "Geo H.7 accepted loan has no candidate-release measurements",
                [("loan_key", loan_key.clone())],
            )
        })?;
        if actual_releases != &expected_releases {
            return Err(h7_invalid(
                "Geo H.7 accepted loans require exactly one row for every pinned candidate release",
                [
                    ("loan_key", loan_key.clone()),
                    ("actual_release_rows", actual_releases.len().to_string()),
                    ("expected_release_rows", expected_releases.len().to_string()),
                ],
            ));
        }
    }
    Ok(())
}

fn accepted_truth_key(case: &GeoH7PopulationCaseArtifact) -> GeoH7AcceptedLoanTruthKey {
    GeoH7AcceptedLoanTruthKey {
        document_id: case.document_id.clone(),
        truth_plane: case.truth_plane,
        association_plane: case.association_plane,
        property_state: case.property_state.clone(),
        accepted_borough_edges: case.accepted_borough_edges.clone(),
        doc_type: case.doc_type.clone(),
        originationdate: case.originationdate.clone(),
        amount_cents: case.amount_cents,
        is_round_100k_lattice: case.is_round_100k_lattice,
        originatorname: case.originatorname.clone(),
        originator_match_text: case.originator_match_text.clone(),
        lender_match_text: case.lender_match_text.clone(),
        lender_party_type: case.lender_party_type.clone(),
        truth_parcels: case.truth_parcels.clone(),
    }
}

fn canonical_h7_borough_edges(
    row: &GeoH7PopulationWarehouseRow,
) -> Result<Vec<GeoH7BoroughEdge>, GeoMaterializationError> {
    if row.accepted_borough_edges.is_empty() {
        return Err(h7_invalid(
            "Geo H.7 rows require accepted filed/legal borough edges",
            [("loan_key", row.loan_key.clone())],
        ));
    }
    let mut edges = row.accepted_borough_edges.clone();
    for edge in &edges {
        validate_h7_string("accepted_borough_edge.filed_county", &edge.filed_county)?;
        if filed_county_borough(&edge.filed_county) != Some(edge.filed_borough) {
            return Err(h7_invalid(
                "Geo H.7 accepted borough edge filed county does not map to its ACRIS borough",
                [
                    ("loan_key", row.loan_key.clone()),
                    ("filed_county", edge.filed_county.clone()),
                    ("filed_borough", edge.filed_borough.to_string()),
                ],
            ));
        }
        if edge.filed_borough != edge.legal_borough {
            return Err(h7_invalid(
                "Geo H.7 accepted borough edge lacks filed/legal borough agreement",
                [
                    ("loan_key", row.loan_key.clone()),
                    ("filed_borough", edge.filed_borough.to_string()),
                    ("legal_borough", edge.legal_borough.to_string()),
                ],
            ));
        }
    }
    edges.sort();
    for pair in edges.windows(2) {
        if pair[0] == pair[1] {
            return Err(h7_invalid(
                "Geo H.7 accepted borough edges repeat one edge",
                [
                    ("loan_key", row.loan_key.clone()),
                    ("filed_county", pair[0].filed_county.clone()),
                    ("filed_borough", pair[0].filed_borough.to_string()),
                ],
            ));
        }
    }
    let edge_boroughs = edges
        .iter()
        .map(|edge| edge.filed_borough)
        .collect::<BTreeSet<_>>();
    let edge_borough_count =
        u64::try_from(edge_boroughs.len()).map_err(|_| h7_overflow("accepted_borough_edges"))?;
    if edge_borough_count != row.loan_field_distinct_counts.filed_borough {
        return Err(h7_invalid(
            "Geo H.7 accepted borough edges do not reconcile to the loan-grain filed-borough count",
            [
                ("loan_key", row.loan_key.clone()),
                ("edge_boroughs", edge_borough_count.to_string()),
                (
                    "distinct_filed_borough",
                    row.loan_field_distinct_counts.filed_borough.to_string(),
                ),
            ],
        ));
    }
    let representative_edge = &edges[0];
    if representative_edge.filed_county != row.filed_county
        || representative_edge.filed_borough != row.filed_borough
        || representative_edge.legal_borough != row.legal_borough
    {
        return Err(h7_invalid(
            "Geo H.7 row-level filed/legal borough must match the canonical accepted-borough representative",
            [
                ("loan_key", row.loan_key.clone()),
                ("filed_county", row.filed_county.clone()),
                ("filed_borough", row.filed_borough.to_string()),
                ("legal_borough", row.legal_borough.to_string()),
                (
                    "representative_filed_county",
                    representative_edge.filed_county.clone(),
                ),
                (
                    "representative_filed_borough",
                    representative_edge.filed_borough.to_string(),
                ),
                (
                    "representative_legal_borough",
                    representative_edge.legal_borough.to_string(),
                ),
            ],
        ));
    }
    Ok(edges)
}

fn unique_h7_loans_by_plane(
    cases: &[GeoH7PopulationCaseArtifact],
) -> BTreeMap<GeoTruthPlane, BTreeSet<String>> {
    let mut unique = BTreeMap::new();
    for case in cases {
        unique
            .entry(case.truth_plane)
            .or_insert_with(BTreeSet::new)
            .insert(case.loan_key.clone());
    }
    unique
}

fn unique_h7_loans_by_stratum(
    cases: &[GeoH7PopulationCaseArtifact],
) -> BTreeMap<
    (
        GeoTruthPlane,
        GeoH7AssociationPlane,
        GeoH7MapplutoReleasePin,
    ),
    BTreeSet<String>,
> {
    let mut unique = BTreeMap::new();
    for case in cases {
        unique
            .entry((
                case.truth_plane,
                case.association_plane,
                case.candidate_release.clone(),
            ))
            .or_insert_with(BTreeSet::new)
            .insert(case.loan_key.clone());
    }
    unique
}

fn h7_truth_planes() -> BTreeSet<GeoTruthPlane> {
    [
        GeoTruthPlane::NonRoundAmountDateLegalBorough,
        GeoTruthPlane::RoundExactLenderParty,
    ]
    .into_iter()
    .collect()
}

fn h7_legal_residual_receipt_plane(purpose: &str) -> Option<GeoTruthPlane> {
    match purpose {
        CANON_GEO_H7_NON_ROUND_LEGAL_RESIDUAL_RECEIPT_PURPOSE => {
            Some(GeoTruthPlane::NonRoundAmountDateLegalBorough)
        }
        CANON_GEO_H7_ROUND_LEGAL_RESIDUAL_RECEIPT_PURPOSE => {
            Some(GeoTruthPlane::RoundExactLenderParty)
        }
        _ => None,
    }
}

fn h7_legal_residual_receipt_purpose(
    plane: GeoTruthPlane,
) -> Result<&'static str, GeoMaterializationError> {
    match plane {
        GeoTruthPlane::NonRoundAmountDateLegalBorough => {
            Ok(CANON_GEO_H7_NON_ROUND_LEGAL_RESIDUAL_RECEIPT_PURPOSE)
        }
        GeoTruthPlane::RoundExactLenderParty => {
            Ok(CANON_GEO_H7_ROUND_LEGAL_RESIDUAL_RECEIPT_PURPOSE)
        }
        _ => Err(h7_invalid(
            "Geo H.7 has no legal-residual receipt purpose for a non-H.7 truth plane",
            [("truth_plane", h7_plane_name(plane).to_string())],
        )),
    }
}

fn h7_reach_status(
    truth_parcels: &[String],
    candidate_parcels: &[String],
) -> GeoH7CandidateReachStatus {
    let candidate_set = candidate_parcels.iter().collect::<BTreeSet<_>>();
    let in_candidate = truth_parcels
        .iter()
        .filter(|parcel| candidate_set.contains(parcel))
        .count();
    if in_candidate == truth_parcels.len() {
        GeoH7CandidateReachStatus::Full
    } else if in_candidate == 0 {
        GeoH7CandidateReachStatus::None
    } else {
        GeoH7CandidateReachStatus::Partial
    }
}

fn h7_case_id(
    plane: GeoTruthPlane,
    loan_key: &str,
    document_id: &str,
    candidate_release: &GeoH7MapplutoReleasePin,
    property_state: &str,
) -> String {
    let digest = blake3::hash(
        format!(
            "{loan_key}\0{document_id}\0{property_state}\0{}\0{}\0{}\0{}",
            candidate_release.release,
            candidate_release.release_dt,
            candidate_release.variant,
            candidate_release.geometry_contract_version
        )
        .as_bytes(),
    )
    .to_hex()
    .to_string();
    format!("h7:{}:{digest}", h7_plane_slug(plane))
}

fn h7_subject_id(
    plane: GeoTruthPlane,
    loan_key: &str,
    document_id: &str,
    property_state: &str,
) -> String {
    let digest = blake3::hash(format!("{loan_key}\0{document_id}\0{property_state}").as_bytes())
        .to_hex()
        .to_string();
    format!("h7-subject:{}:{digest}", h7_plane_slug(plane))
}

fn h7_plane_slug(plane: GeoTruthPlane) -> &'static str {
    match plane {
        GeoTruthPlane::NonRoundAmountDateLegalBorough => "non-round",
        GeoTruthPlane::RoundExactLenderParty => "round-exact-lender",
        _ => "non-h7",
    }
}

fn h7_plane_name(plane: GeoTruthPlane) -> &'static str {
    match plane {
        GeoTruthPlane::GateV2Historical => "gate_v2_historical",
        GeoTruthPlane::NonRoundAmountDateLegalBorough => "non_round_amount_date_legal_borough",
        GeoTruthPlane::RoundExactLenderParty => "round_exact_lender_party",
        GeoTruthPlane::AddressDerivedControl => "address_derived_control",
        GeoTruthPlane::HumanAdjudication => "human_adjudication",
    }
}

fn h7_reach_name(status: GeoH7CandidateReachStatus) -> &'static str {
    match status {
        GeoH7CandidateReachStatus::Full => "full",
        GeoH7CandidateReachStatus::Partial => "partial",
        GeoH7CandidateReachStatus::None => "none",
    }
}

fn h7_population_scope_name(scope: GeoH7PopulationScope) -> &'static str {
    match scope {
        GeoH7PopulationScope::FixtureSubset => "fixture_subset",
        GeoH7PopulationScope::RetainedComplete => "retained_complete",
        GeoH7PopulationScope::LiveComplete => "live_complete",
    }
}

fn h7_result_mode_name(mode: GeoH7ResultMode) -> &'static str {
    match mode {
        GeoH7ResultMode::Live => "live",
        GeoH7ResultMode::Replay => "replay",
    }
}

fn filed_county_borough(county: &str) -> Option<u8> {
    match county.trim().to_ascii_uppercase().as_str() {
        "NEW YORK" | "MANHATTAN" | "NY061" => Some(1),
        "BRONX" => Some(2),
        "KINGS" | "BROOKLYN" => Some(3),
        "QUEENS" => Some(4),
        "RICHMOND" => Some(5),
        _ => None,
    }
}

fn lender_party_type_for_doc_type(doc_type: &str) -> Option<&'static str> {
    match doc_type.trim().to_ascii_uppercase().as_str() {
        "MMTG" => Some("1"),
        "CMTG" | "M&CON" | "MTGE" | "SMTG" | "SPRD" => Some("2"),
        _ => None,
    }
}

fn sorted_distinct_nonempty(
    field: &str,
    values: &[String],
) -> Result<Vec<String>, GeoMaterializationError> {
    let canonical = sorted_distinct(field, values)?;
    if canonical.is_empty() {
        return Err(h7_invalid(
            "Geo H.7 truth fields require non-empty parcel sets",
            [("field", field.to_string())],
        ));
    }
    Ok(canonical)
}

fn sorted_distinct(field: &str, values: &[String]) -> Result<Vec<String>, GeoMaterializationError> {
    let mut canonical = values.to_vec();
    for value in &canonical {
        validate_h7_string(field, value)?;
    }
    canonical.sort();
    for pair in canonical.windows(2) {
        if pair[0] == pair[1] {
            return Err(h7_invalid(
                "Geo H.7 population rows repeat a parcel within one set",
                [("field", field.to_string()), ("parcel_id", pair[0].clone())],
            ));
        }
    }
    Ok(canonical)
}

fn validate_h7_optional_count(
    row: &GeoH7PopulationWarehouseRow,
    field: &str,
    value: &Option<String>,
    distinct_count: u64,
) -> Result<(), GeoMaterializationError> {
    if distinct_count > 1 {
        return Err(h7_invalid(
            "Geo H.7 row has ambiguous optional loan fields at loan grain",
            [
                ("loan_key", row.loan_key.clone()),
                ("field", field.to_string()),
                ("distinct_count", distinct_count.to_string()),
            ],
        ));
    }
    let canonical_value = canonical_optional_string(value);
    match (distinct_count, canonical_value) {
        (0, None) | (1, Some(_)) => Ok(()),
        (0, Some(actual)) => Err(h7_invalid(
            "Geo H.7 row carries a value for a field counted as absent",
            [
                ("loan_key", row.loan_key.clone()),
                ("field", field.to_string()),
                ("value", actual),
            ],
        )),
        (1, None) => Err(h7_invalid(
            "Geo H.7 row omits a value for a field counted as singular",
            [
                ("loan_key", row.loan_key.clone()),
                ("field", field.to_string()),
            ],
        )),
        _ => Err(h7_invalid(
            "Geo H.7 row has an unsupported optional field count",
            [
                ("loan_key", row.loan_key.clone()),
                ("field", field.to_string()),
                ("distinct_count", distinct_count.to_string()),
            ],
        )),
    }
}

fn required_optional_string(
    row: &GeoH7PopulationWarehouseRow,
    field: &str,
    value: &Option<String>,
) -> Result<String, GeoMaterializationError> {
    let Some(value) = canonical_optional_string(value) else {
        return Err(h7_invalid(
            "Geo H.7 round exact-lender row is missing a required lender discriminator",
            [
                ("loan_key", row.loan_key.clone()),
                ("field", field.to_string()),
            ],
        ));
    };
    Ok(value)
}

fn canonical_optional_string(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn canonical_h7_source_records(
    row: &GeoH7PopulationWarehouseRow,
    truth_parcels: &[String],
    candidate_parcels: &[String],
) -> Result<Vec<GeoH7SourceEvidenceRecord>, GeoMaterializationError> {
    if row.source_records.is_empty() {
        return Err(h7_invalid(
            "Geo H.7 rows require typed immutable source records for provenance",
            [("loan_key", row.loan_key.clone())],
        ));
    }

    let mut records = row.source_records.clone();
    let mut role_counts: BTreeMap<GeoH7SourceRecordRole, u64> = BTreeMap::new();
    let mut truth_parcel_support = BTreeSet::new();
    let mut candidate_parcel_support = BTreeSet::new();
    let mut seen_wrappers = BTreeSet::new();
    let mut seen_record_ids = BTreeSet::new();
    for record in &records {
        validate_h7_source_record(row, record)?;
        let canonical_parcels = canonical_h7_source_record_parcels(record)?;
        match record.role {
            GeoH7SourceRecordRole::AcrisLegal => {
                require_single_h7_source_parcel(record.role, &canonical_parcels)?;
                truth_parcel_support.extend(canonical_parcels);
            }
            GeoH7SourceRecordRole::MapplutoCandidate => {
                require_single_h7_source_parcel(record.role, &canonical_parcels)?;
                candidate_parcel_support.extend(canonical_parcels);
            }
            _ => {
                if !canonical_parcels.is_empty() {
                    return Err(h7_invalid(
                        "Geo H.7 source parcel coverage is only allowed on ACRIS legal or MapPLUTO candidate roles",
                        [
                            ("loan_key", row.loan_key.clone()),
                            ("role", h7_source_record_role_name(record.role).to_string()),
                        ],
                    ));
                }
            }
        }
        if !seen_wrappers.insert(record.clone()) {
            return Err(h7_invalid(
                "Geo H.7 rows repeat an exact source evidence wrapper",
                [
                    ("loan_key", row.loan_key.clone()),
                    ("role", h7_source_record_role_name(record.role).to_string()),
                    (
                        "source_record_id",
                        record.source_record.source_record_id.clone(),
                    ),
                ],
            ));
        }
        if !seen_record_ids.insert(record.source_record.source_record_id.clone()) {
            return Err(h7_invalid(
                "Geo H.7 rows repeat an immutable source record id",
                [
                    ("loan_key", row.loan_key.clone()),
                    (
                        "source_record_id",
                        record.source_record.source_record_id.clone(),
                    ),
                ],
            ));
        }
        let count = role_counts.entry(record.role).or_default();
        *count = count
            .checked_add(1)
            .ok_or_else(|| h7_overflow("source_record_role_count"))?;
    }

    let required_roles = required_h7_source_roles(row.truth_plane, !candidate_parcels.is_empty());
    for required_role in required_roles {
        if role_counts.get(&required_role).copied().unwrap_or(0) == 0 {
            return Err(h7_invalid(
                "Geo H.7 rows are missing a required source evidence role",
                [
                    ("loan_key", row.loan_key.clone()),
                    (
                        "required_role",
                        h7_source_record_role_name(required_role).to_string(),
                    ),
                ],
            ));
        }
    }
    if row.truth_plane != GeoTruthPlane::RoundExactLenderParty
        && role_counts.contains_key(&GeoH7SourceRecordRole::AcrisParty)
    {
        return Err(h7_invalid(
            "Geo H.7 ACRIS party source evidence is only admissible in the round exact-lender plane",
            [("loan_key", row.loan_key.clone())],
        ));
    }
    require_h7_parcel_support(
        row,
        GeoH7SourceRecordRole::AcrisLegal,
        truth_parcels,
        &truth_parcel_support,
    )?;
    require_h7_parcel_support(
        row,
        GeoH7SourceRecordRole::MapplutoCandidate,
        candidate_parcels,
        &candidate_parcel_support,
    )?;

    records.sort();
    Ok(records)
}

fn canonical_h7_source_record_parcels(
    record: &GeoH7SourceEvidenceRecord,
) -> Result<Vec<String>, GeoMaterializationError> {
    let mut parcels = record.parcel_ids.clone();
    for parcel_id in &parcels {
        validate_h7_string("source_record.parcel_id", parcel_id)?;
    }
    parcels.sort();
    for pair in parcels.windows(2) {
        if pair[0] == pair[1] {
            return Err(h7_invalid(
                "Geo H.7 source evidence repeats a parcel id within one record",
                [
                    ("role", h7_source_record_role_name(record.role).to_string()),
                    ("parcel_id", pair[0].clone()),
                ],
            ));
        }
    }
    Ok(parcels)
}

fn require_h7_parcel_support(
    row: &GeoH7PopulationWarehouseRow,
    role: GeoH7SourceRecordRole,
    required_parcels: &[String],
    supported_parcels: &BTreeSet<String>,
) -> Result<(), GeoMaterializationError> {
    let required = required_parcels.iter().cloned().collect::<BTreeSet<_>>();
    for parcel_id in required_parcels {
        if !supported_parcels.contains(parcel_id) {
            return Err(h7_invalid(
                "Geo H.7 source evidence parcel union must equal its typed parcel set",
                [
                    ("loan_key", row.loan_key.clone()),
                    ("role", h7_source_record_role_name(role).to_string()),
                    ("mismatch", "missing".to_string()),
                    ("parcel_id", parcel_id.clone()),
                ],
            ));
        }
    }
    for parcel_id in supported_parcels {
        if !required.contains(parcel_id) {
            return Err(h7_invalid(
                "Geo H.7 source evidence parcel union must equal its typed parcel set",
                [
                    ("loan_key", row.loan_key.clone()),
                    ("role", h7_source_record_role_name(role).to_string()),
                    ("mismatch", "extra".to_string()),
                    ("parcel_id", parcel_id.clone()),
                ],
            ));
        }
    }
    Ok(())
}

fn require_single_h7_source_parcel(
    role: GeoH7SourceRecordRole,
    parcels: &[String],
) -> Result<(), GeoMaterializationError> {
    if parcels.len() != 1 {
        return Err(h7_invalid(
            "Geo H.7 ACRIS legal and MapPLUTO source records must name exactly one parcel",
            [
                ("role", h7_source_record_role_name(role).to_string()),
                ("parcel_ids", parcels.len().to_string()),
            ],
        ));
    }
    Ok(())
}

fn h7_source_record_digest_from_wire(
    source_record_id: &str,
    declared_record_blake3: &str,
    source_record_bytes_base64: Option<&str>,
) -> Result<String, String> {
    if !declared_record_blake3.is_empty() && !is_lowercase_blake3_hex(declared_record_blake3) {
        return Err(format!(
            "Geo H.7 source_record.record_blake3 is not canonical lowercase BLAKE3 for {source_record_id}"
        ));
    }

    let Some(source_record_bytes_base64) = source_record_bytes_base64 else {
        if declared_record_blake3.is_empty() {
            return Err(format!(
                "Geo H.7 source evidence requires record_blake3 or source_record_bytes_base64 for {source_record_id}"
            ));
        }
        return Ok(declared_record_blake3.to_string());
    };

    let bytes = BASE64_STANDARD
        .decode(source_record_bytes_base64.as_bytes())
        .map_err(|error| {
            format!(
                "Geo H.7 source_record_bytes_base64 is not canonical base64 for {source_record_id}: {error}"
            )
        })?;
    if BASE64_STANDARD.encode(&bytes) != source_record_bytes_base64 {
        return Err(format!(
            "Geo H.7 source_record_bytes_base64 is not in canonical padded form for {source_record_id}"
        ));
    }
    if bytes.is_empty() {
        return Err(format!(
            "Geo H.7 source_record_bytes_base64 decodes to an empty source record for {source_record_id}"
        ));
    }

    let computed = blake3::hash(&bytes).to_hex().to_string();
    if !declared_record_blake3.is_empty() && declared_record_blake3 != computed {
        return Err(format!(
            "Geo H.7 source_record.record_blake3 does not match source_record_bytes_base64 for {source_record_id}"
        ));
    }
    Ok(computed)
}

fn is_lowercase_blake3_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_h7_source_record(
    row: &GeoH7PopulationWarehouseRow,
    record: &GeoH7SourceEvidenceRecord,
) -> Result<(), GeoMaterializationError> {
    validate_h7_string(
        "source_record.source_record_id",
        &record.source_record.source_record_id,
    )?;
    validate_h7_string(
        "source_record.source_vintage",
        &record.source_record.source_vintage,
    )?;
    validate_blake3_hash(
        "source_record.record_blake3",
        &record.source_record.record_blake3,
    )?;
    let expected_vintage = match record.role {
        GeoH7SourceRecordRole::BridgeLoan => CANON_GEO_H7_BRIDGE_BUILD_ID,
        GeoH7SourceRecordRole::AcrisMaster
        | GeoH7SourceRecordRole::AcrisLegal
        | GeoH7SourceRecordRole::AcrisParty => CANON_GEO_H7_ACRIS_RELEASE_DT,
        GeoH7SourceRecordRole::MapplutoCandidate => row.candidate_release.release_dt.as_str(),
        GeoH7SourceRecordRole::GeocodeDiagnostic => return Ok(()),
    };
    if record.source_record.source_vintage != expected_vintage {
        return Err(h7_invalid(
            "Geo H.7 source evidence vintage does not match its required release",
            [
                ("loan_key", row.loan_key.clone()),
                ("role", h7_source_record_role_name(record.role).to_string()),
                ("actual", record.source_record.source_vintage.clone()),
                ("expected", expected_vintage.to_string()),
            ],
        ));
    }
    Ok(())
}

fn required_h7_source_roles(
    truth_plane: GeoTruthPlane,
    has_candidates: bool,
) -> BTreeSet<GeoH7SourceRecordRole> {
    let mut roles = [
        GeoH7SourceRecordRole::BridgeLoan,
        GeoH7SourceRecordRole::AcrisMaster,
        GeoH7SourceRecordRole::AcrisLegal,
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if has_candidates {
        roles.insert(GeoH7SourceRecordRole::MapplutoCandidate);
    }
    if truth_plane == GeoTruthPlane::RoundExactLenderParty {
        roles.insert(GeoH7SourceRecordRole::AcrisParty);
    }
    roles
}

fn h7_source_record_role_name(role: GeoH7SourceRecordRole) -> &'static str {
    match role {
        GeoH7SourceRecordRole::BridgeLoan => "bridge_loan",
        GeoH7SourceRecordRole::AcrisMaster => "acris_master",
        GeoH7SourceRecordRole::AcrisLegal => "acris_legal",
        GeoH7SourceRecordRole::AcrisParty => "acris_party",
        GeoH7SourceRecordRole::MapplutoCandidate => "mappluto_candidate",
        GeoH7SourceRecordRole::GeocodeDiagnostic => "geocode_diagnostic",
    }
}

fn validate_h7_string(field: &str, value: &str) -> Result<(), GeoMaterializationError> {
    if value.is_empty() || value.trim() != value {
        return Err(h7_invalid(
            "Geo H.7 string fields must be non-empty and canonical-trimmed",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn validate_blake3_hash(field: &str, value: &str) -> Result<(), GeoMaterializationError> {
    validate_hex_hash(field, value, 64)
}

fn validate_sha256(field: &str, value: &str) -> Result<(), GeoMaterializationError> {
    validate_hex_hash(field, value, 64)
}

fn validate_hex_hash(
    field: &str,
    value: &str,
    length: usize,
) -> Result<(), GeoMaterializationError> {
    if value.len() != length || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(h7_invalid(
            "Geo H.7 hash fields must be lowercase fixed-width hex",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    if value.chars().any(|ch| ch.is_ascii_uppercase()) {
        return Err(h7_invalid(
            "Geo H.7 hash fields must be lowercase fixed-width hex",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn len_u64<T>(values: &[T], field: &str) -> Result<u64, GeoMaterializationError> {
    u64::try_from(values.len()).map_err(|_| h7_overflow(field))
}

fn len_u64_set<T>(values: &BTreeSet<T>, field: &str) -> Result<u64, GeoMaterializationError> {
    u64::try_from(values.len()).map_err(|_| h7_overflow(field))
}

fn h7_increment(value: &mut u64, field: &str) -> Result<(), GeoMaterializationError> {
    *value = value.checked_add(1).ok_or_else(|| h7_overflow(field))?;
    Ok(())
}

fn h7_overflow(field: &str) -> GeoMaterializationError {
    GeoMaterializationError {
        code: GeoMaterializationErrorCode::InvalidInput,
        message: "Geo H.7 population arithmetic overflowed".to_string(),
        detail: [("field".to_string(), field.to_string())]
            .into_iter()
            .collect(),
    }
}

fn h7_invalid<const N: usize>(
    message: &'static str,
    detail: [(&'static str, String); N],
) -> GeoMaterializationError {
    GeoMaterializationError {
        code: GeoMaterializationErrorCode::InvalidInput,
        message: message.to_string(),
        detail: detail
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    }
}
