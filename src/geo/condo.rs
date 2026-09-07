#![forbid(unsafe_code)]

//! PAD condo unit-to-billing-lot bridge for fixture and offline Geo runs.

use super::{
    composition::{GeoEntityLevel, GeoEntityRef, GeoIdentityRelation, validate_identity_relation},
    geometry::{
        GeoAreaMajorityError, GeoLinearRingMm, GeoPointMm, GeoPredicateError,
        footprint_majority_area_inside_parcel,
    },
    geometry_value::GeoCanonicalPolygonMm,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION: &str = "canon_geo_condo_bridge_request.v0";
pub const CANON_GEO_CONDO_BRIDGE_VERSION: &str = "canon_geo_condo_bridge.v0";
pub const CANON_GEO_LEDGER_BRIDGE_VERSION: &str = "canon_geo_ledger_bridge.v0";
pub const GEO_CONDO_CONFIRMATION_INSUFFICIENT: &str = "condo_confirmation_insufficient";
pub const CANON_GEO_CONDO_BRIDGE_PAD_METHOD: &str = "PAD BBL current release: unit lot -> BILLING_BBL_KEY via exact row or LOW/HIGH range; truth plane re-expressed at billing-lot grain";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCondoBridgeRequest {
    pub version: String,
    pub source_dataset: String,
    pub source_release: String,
    pub source_lineage_ids: Vec<String>,
    pub source_pins: Vec<GeoCondoSourcePin>,
    pub pad_rows: Vec<GeoPadBblRow>,
    pub cases: Vec<GeoCondoBridgeCaseRequest>,
    pub max_pad_rows: usize,
    pub max_cases: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCondoUnitBridgeRequest {
    pub version: String,
    pub unit_bbl: String,
    pub billing_bbl_candidates: Vec<String>,
    pub bin_candidates: Vec<String>,
    pub block: String,
    pub frame_id: String,
    pub source_pins: Vec<GeoCondoSourcePin>,
    pub parcel_rings: BTreeMap<String, GeoCanonicalPolygonMm>,
    pub footprint_rings: BTreeMap<String, GeoCanonicalPolygonMm>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCondoSourcePin {
    pub source_table: String,
    pub natural_key: String,
    pub source_release: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_dt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_row_number: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_content_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_content_sha256_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parser_version: Option<String>,
    pub license_terms: String,
    pub attribution_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCondoConfirmation {
    BlockAndGeometry,
    BlockOnly,
    KeyOnly,
}

pub type GeoLedgerBridgeRelation = (GeoEntityRef, GeoIdentityRelation, GeoEntityRef);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoLedgerBridge {
    pub version: String,
    pub unit_bbl: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_bbl: Option<String>,
    pub bins: Vec<String>,
    pub confirmation: GeoCondoConfirmation,
    pub relations: Vec<GeoLedgerBridgeRelation>,
    pub source_pins: Vec<GeoCondoSourcePin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abstained_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPadBblRow {
    #[serde(rename = "BBL_KEY")]
    pub bbl_key: String,
    #[serde(rename = "LOW_BBL_KEY")]
    pub low_bbl_key: String,
    #[serde(rename = "HIGH_BBL_KEY")]
    pub high_bbl_key: String,
    #[serde(default, rename = "BILLING_BBL_KEY")]
    pub billing_bbl_key: Option<String>,
    #[serde(default, rename = "CONDO_NUMBER")]
    pub condo_number: Option<u64>,
    #[serde(default, rename = "CONDO_FLAG")]
    pub condo_flag: Option<String>,
    #[serde(default, rename = "RELEASE")]
    pub release: Option<String>,
    #[serde(default, rename = "RELEASE_DT")]
    pub release_dt: Option<String>,
    #[serde(default, rename = "SOURCE_ROW_NUMBER")]
    pub source_row_number: Option<u64>,
    #[serde(default, rename = "SOURCE_FILE")]
    pub source_file: Option<String>,
    #[serde(default, rename = "SOURCE_FILENAME")]
    pub source_filename: Option<String>,
    #[serde(default, rename = "SOURCE_ZIP_SHA256")]
    pub source_zip_sha256: Option<String>,
    #[serde(default, rename = "PARSER_VERSION")]
    pub parser_version: Option<String>,
    #[serde(default, rename = "LICENSE_TERMS")]
    pub license_terms: Option<String>,
    #[serde(default, rename = "ATTRIBUTION_TEXT")]
    pub attribution_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCondoBridgeCaseRequest {
    pub case_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loan_key: Option<String>,
    pub truth_parcels: Vec<String>,
    pub universe_parcels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCondoBridgeArtifact {
    pub version: String,
    pub method: String,
    pub source_dataset: String,
    pub source_release: String,
    pub source_lineage_ids: Vec<String>,
    pub source_pins: Vec<GeoCondoSourcePin>,
    pub request_blake3: String,
    pub stats: GeoCondoBridgeStats,
    pub rows: Vec<GeoCondoBridgeCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCondoBridgeStats {
    pub cases: u64,
    pub fully_reached: u64,
    pub partial: u64,
    pub unreached: u64,
    pub truth_unit_lots: u64,
    pub truth_unit_lots_mapped: u64,
    pub truth_unit_lots_unmapped: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCondoBridgeCase {
    pub case_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loan_key: Option<String>,
    pub unit_lots: u64,
    pub lot_mappings: Vec<GeoCondoLotMapping>,
    pub unmapped_lots: Vec<GeoCondoUnmappedLot>,
    pub truth_original_grain: Vec<String>,
    pub universe_original_grain: Vec<String>,
    pub truth_billing_grain: Vec<String>,
    pub universe_billing_grain: Vec<String>,
    pub before: GeoCondoReachCount,
    pub after: GeoCondoReachCount,
    pub kind: GeoCondoBridgeReachKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCondoLotMapping {
    pub unit_lot: String,
    pub status: GeoCondoLotMappingStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_lot: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condo_number: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_billing_lots: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pad_bbl_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_kind: Option<GeoCondoPadMatchKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_pins: Vec<GeoCondoSourcePin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCondoUnmappedLot {
    pub unit_lot: String,
    pub reason: GeoCondoUnmappedReason,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_billing_lots: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pad_bbl_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_pins: Vec<GeoCondoSourcePin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCondoReachCount {
    pub truth_members_in_universe: u64,
    pub truth_members: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCondoBridgeReachKind {
    FullyReached,
    Partial,
    Unreached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCondoLotMappingStatus {
    Mapped,
    Unmapped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCondoPadMatchKind {
    ExactRow,
    Range,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCondoUnmappedReason {
    NoPadBblRow,
    MissingBillingLot,
    AmbiguousBillingLots,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCondoErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
    CondoConfirmationInsufficient,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoCondoError {
    pub code: GeoCondoErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoCondoError {
    fn new(
        code: GeoCondoErrorCode,
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
        Self::new(GeoCondoErrorCode::InvalidInput, message, detail)
    }

    fn overflow(field: &str) -> Self {
        Self::new(
            GeoCondoErrorCode::ArithmeticOverflow,
            "Geo condo bridge arithmetic overflowed",
            [("field", field)],
        )
    }
}

impl fmt::Display for GeoCondoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoCondoError {}

#[derive(Debug, Clone)]
struct PadIndex {
    rows_by_block: BTreeMap<String, Vec<GeoPadBblRow>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LotResolution {
    mapping: GeoCondoLotMapping,
    unmapped: Option<GeoCondoUnmappedLot>,
}

pub fn bridge_condo_unit(
    request: &GeoCondoUnitBridgeRequest,
) -> Result<GeoLedgerBridge, GeoCondoError> {
    let request = canonicalize_condo_unit_bridge_request(request)?;
    let block_candidates = request
        .billing_bbl_candidates
        .iter()
        .filter(|candidate| candidate_matches_declared_block(candidate, &request.block))
        .collect::<Vec<_>>();
    if block_candidates.is_empty() {
        return abstained_ledger_bridge(&request, GeoCondoConfirmation::KeyOnly, "block_mismatch");
    }

    let mut confirmed = Vec::<(String, Vec<String>)>::new();
    for billing_bbl in block_candidates {
        let Some(parcel_polygon) = request.parcel_rings.get(billing_bbl) else {
            continue;
        };
        let parcel_ring = predicate_ring_from_polygon(
            &request.frame_id,
            "parcel_rings",
            billing_bbl,
            parcel_polygon,
        )?;
        let mut bins = Vec::new();
        for bin in &request.bin_candidates {
            let Some(footprint_polygon) = request.footprint_rings.get(bin) else {
                continue;
            };
            let footprint_ring = predicate_ring_from_polygon(
                &request.frame_id,
                "footprint_rings",
                bin,
                footprint_polygon,
            )?;
            if footprint_majority_area_inside_parcel(&footprint_ring, &parcel_ring)
                .map_err(|error| condo_area_error(billing_bbl, bin, error))?
            {
                bins.push(bin.clone());
            }
        }
        if !bins.is_empty() {
            confirmed.push((billing_bbl.clone(), bins));
        }
    }

    match confirmed.as_slice() {
        [(billing_bbl, bins)] => confirmed_ledger_bridge(&request, billing_bbl, bins),
        [] => abstained_ledger_bridge(
            &request,
            GeoCondoConfirmation::BlockOnly,
            "missing_geometry_confirmation",
        ),
        _ => abstained_ledger_bridge(
            &request,
            GeoCondoConfirmation::BlockOnly,
            "ambiguous_billing_bbl",
        ),
    }
}

pub fn validate_condo_bridge_request_artifact(
    request: &GeoCondoUnitBridgeRequest,
) -> Result<(), GeoCondoError> {
    canonicalize_condo_unit_bridge_request(request).map(|_| ())
}

pub fn canonical_condo_unit_bridge_request_bytes(
    request: &GeoCondoUnitBridgeRequest,
) -> Result<Vec<u8>, GeoCondoError> {
    let canonical = canonicalize_condo_unit_bridge_request(request)?;
    serde_json::to_vec(&canonical).map_err(|error| {
        GeoCondoError::invalid(
            "Geo condo unit bridge request could not be serialized",
            [("serde_error", error.to_string())],
        )
    })
}

pub fn canonical_ledger_bridge_bytes(bridge: &GeoLedgerBridge) -> Result<Vec<u8>, GeoCondoError> {
    validate_ledger_bridge_artifact(bridge)?;
    serde_json::to_vec(bridge).map_err(|error| {
        GeoCondoError::invalid(
            "Geo ledger bridge artifact could not be serialized",
            [("serde_error", error.to_string())],
        )
    })
}

pub fn validate_ledger_bridge_artifact(bridge: &GeoLedgerBridge) -> Result<(), GeoCondoError> {
    if bridge.version != CANON_GEO_LEDGER_BRIDGE_VERSION {
        return Err(GeoCondoError::new(
            GeoCondoErrorCode::UnsupportedVersion,
            "Unsupported Geo ledger bridge version",
            [
                ("actual", bridge.version.as_str()),
                ("expected", CANON_GEO_LEDGER_BRIDGE_VERSION),
            ],
        ));
    }
    validate_string("unit_bbl", &bridge.unit_bbl)?;
    let mut source_pins = bridge.source_pins.clone();
    validate_source_pins("source_pins", &mut source_pins, true)?;
    if source_pins != bridge.source_pins {
        return Err(GeoCondoError::invalid(
            "Geo ledger bridge source_pins must be sorted and distinct",
            [("field", "source_pins")],
        ));
    }
    validate_sorted_strings_readonly("bins", &bridge.bins)?;
    for (left, relation, right) in &bridge.relations {
        validate_identity_relation(left, right, *relation).map_err(|error| {
            GeoCondoError::invalid(
                "Geo ledger bridge relation violates the identity relation contract",
                [
                    ("left", format!("{}:{:?}", left.id, left.level)),
                    ("right", format!("{}:{:?}", right.id, right.level)),
                    ("relation", format!("{relation:?}")),
                    ("source_code", format!("{:?}", error.code)),
                    ("source_message", error.message),
                ],
            )
        })?;
    }

    match bridge.confirmation {
        GeoCondoConfirmation::BlockAndGeometry => {
            let Some(billing_bbl) = &bridge.billing_bbl else {
                return Err(GeoCondoError::invalid(
                    "Geo ledger bridge confirmation requires a billing BBL",
                    [("field", "billing_bbl")],
                ));
            };
            validate_string("billing_bbl", billing_bbl)?;
            if bridge.bins.is_empty() {
                return Err(GeoCondoError::invalid(
                    "Geo ledger bridge confirmation requires at least one BIN",
                    [("field", "bins")],
                ));
            }
            if bridge.abstained_reason.is_some() {
                return Err(GeoCondoError::invalid(
                    "Geo ledger bridge confirmation cannot carry an abstention reason",
                    [("field", "abstained_reason")],
                ));
            }
            if bridge.relations.len() != bridge.bins.len() + 1 {
                return Err(GeoCondoError::invalid(
                    "Geo ledger bridge relations must bind the unit and every confirmed BIN",
                    [
                        ("field", "relations".to_string()),
                        ("relations", bridge.relations.len().to_string()),
                        ("bins", bridge.bins.len().to_string()),
                    ],
                ));
            }
            let parcel = GeoEntityRef::new(GeoEntityLevel::Parcel, billing_bbl.clone());
            let mut expected_relations = Vec::with_capacity(bridge.bins.len() + 1);
            expected_relations.push((
                GeoEntityRef::new(GeoEntityLevel::PoiUnit, bridge.unit_bbl.clone()),
                GeoIdentityRelation::PartOf,
                parcel.clone(),
            ));
            for bin in &bridge.bins {
                expected_relations.push((
                    GeoEntityRef::new(GeoEntityLevel::Building, bin.clone()),
                    GeoIdentityRelation::On,
                    parcel.clone(),
                ));
            }
            if bridge.relations != expected_relations {
                return Err(GeoCondoError::invalid(
                    "Geo ledger bridge relations must match the emitted unit, billing BBL, and BINs",
                    [("field", "relations")],
                ));
            }
        }
        GeoCondoConfirmation::BlockOnly | GeoCondoConfirmation::KeyOnly => {
            if bridge.billing_bbl.is_some()
                || !bridge.bins.is_empty()
                || !bridge.relations.is_empty()
            {
                return Err(GeoCondoError::invalid(
                    "Geo ledger bridge abstentions must not emit billing, BIN, or relation sets",
                    [("field", "confirmation")],
                ));
            }
            let Some(reason) = &bridge.abstained_reason else {
                return Err(GeoCondoError::new(
                    GeoCondoErrorCode::CondoConfirmationInsufficient,
                    "Geo ledger bridge abstention must carry a typed reason",
                    [("field", "abstained_reason")],
                ));
            };
            validate_string("abstained_reason", reason)?;
            if !reason.starts_with(GEO_CONDO_CONFIRMATION_INSUFFICIENT) {
                return Err(GeoCondoError::new(
                    GeoCondoErrorCode::CondoConfirmationInsufficient,
                    "Geo ledger bridge abstention reason must preserve the condo confirmation code",
                    [
                        ("field", "abstained_reason"),
                        ("expected", GEO_CONDO_CONFIRMATION_INSUFFICIENT),
                        ("actual", reason.as_str()),
                    ],
                ));
            }
        }
    }
    Ok(())
}

pub fn build_condo_bridge(
    request: &GeoCondoBridgeRequest,
) -> Result<GeoCondoBridgeArtifact, GeoCondoError> {
    let request = canonicalize_condo_bridge_request(request)?;
    if request.pad_rows.len() > request.max_pad_rows {
        return Err(GeoCondoError::new(
            GeoCondoErrorCode::BudgetExceeded,
            "Geo condo bridge request exceeds max_pad_rows",
            [
                ("field", "max_pad_rows".to_string()),
                ("actual", request.pad_rows.len().to_string()),
                ("max", request.max_pad_rows.to_string()),
            ],
        ));
    }
    if request.cases.len() > request.max_cases {
        return Err(GeoCondoError::new(
            GeoCondoErrorCode::BudgetExceeded,
            "Geo condo bridge request exceeds max_cases",
            [
                ("field", "max_cases".to_string()),
                ("actual", request.cases.len().to_string()),
                ("max", request.max_cases.to_string()),
            ],
        ));
    }

    let index = PadIndex::new(&request.pad_rows);
    let mut rows = Vec::new();
    for case in &request.cases {
        let case_row = bridge_case(case, &index, &request.source_pins)?;
        if case_row.unit_lots > 0 {
            rows.push(case_row);
        }
    }
    rows.sort_by(|left, right| left.case_id.cmp(&right.case_id));

    let stats = summarize_rows(&rows)?;
    let artifact = GeoCondoBridgeArtifact {
        version: CANON_GEO_CONDO_BRIDGE_VERSION.to_string(),
        method: CANON_GEO_CONDO_BRIDGE_PAD_METHOD.to_string(),
        source_dataset: request.source_dataset.clone(),
        source_release: request.source_release.clone(),
        source_lineage_ids: request.source_lineage_ids.clone(),
        source_pins: request.source_pins.clone(),
        request_blake3: condo_bridge_request_blake3(&request)?,
        stats,
        rows,
    };
    validate_condo_bridge_artifact(&artifact)?;
    Ok(artifact)
}

pub fn canonicalize_condo_bridge_request(
    request: &GeoCondoBridgeRequest,
) -> Result<GeoCondoBridgeRequest, GeoCondoError> {
    if request.version != CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION {
        return Err(GeoCondoError::new(
            GeoCondoErrorCode::UnsupportedVersion,
            "Unsupported Geo condo bridge request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION),
            ],
        ));
    }
    validate_string("source_dataset", &request.source_dataset)?;
    validate_string("source_release", &request.source_release)?;
    if request.max_pad_rows == 0 {
        return Err(invalid_count("max_pad_rows", request.max_pad_rows));
    }
    if request.max_cases == 0 {
        return Err(invalid_count("max_cases", request.max_cases));
    }

    let mut canonical = request.clone();
    validate_sorted_strings(
        "source_lineage_ids",
        &mut canonical.source_lineage_ids,
        true,
    )?;
    validate_source_pins("source_pins", &mut canonical.source_pins, true)?;
    for row in &canonical.pad_rows {
        validate_pad_row(row)?;
    }
    canonical
        .pad_rows
        .sort_by(|left, right| pad_row_sort_key(left).cmp(&pad_row_sort_key(right)));
    for case in &mut canonical.cases {
        validate_case_request(case)?;
        sort_dedup_bbls("truth_parcels", &mut case.truth_parcels)?;
        sort_dedup_bbls("universe_parcels", &mut case.universe_parcels)?;
    }
    canonical
        .cases
        .sort_by(|left, right| left.case_id.cmp(&right.case_id));
    reject_duplicate_cases(&canonical.cases)?;
    Ok(canonical)
}

pub fn canonical_condo_bridge_request_bytes(
    request: &GeoCondoBridgeRequest,
) -> Result<Vec<u8>, GeoCondoError> {
    let canonical = canonicalize_condo_bridge_request(request)?;
    serde_json::to_vec(&canonical).map_err(|error| {
        GeoCondoError::invalid(
            "Geo condo bridge request could not be serialized",
            [("serde_error", error.to_string())],
        )
    })
}

pub fn canonical_condo_bridge_bytes(
    artifact: &GeoCondoBridgeArtifact,
) -> Result<Vec<u8>, GeoCondoError> {
    validate_condo_bridge_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoCondoError::invalid(
            "Geo condo bridge artifact could not be serialized",
            [("serde_error", error.to_string())],
        )
    })
}

pub fn validate_condo_bridge_artifact(
    artifact: &GeoCondoBridgeArtifact,
) -> Result<(), GeoCondoError> {
    if artifact.version != CANON_GEO_CONDO_BRIDGE_VERSION {
        return Err(GeoCondoError::new(
            GeoCondoErrorCode::UnsupportedVersion,
            "Unsupported Geo condo bridge artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_CONDO_BRIDGE_VERSION),
            ],
        ));
    }
    validate_string("source_dataset", &artifact.source_dataset)?;
    if artifact.method != CANON_GEO_CONDO_BRIDGE_PAD_METHOD {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge artifact declares an unsupported method",
            [
                ("actual", artifact.method.as_str()),
                ("expected", CANON_GEO_CONDO_BRIDGE_PAD_METHOD),
            ],
        ));
    }
    validate_string("source_release", &artifact.source_release)?;
    validate_blake3_ref("request_blake3", &artifact.request_blake3)?;
    let mut lineage = artifact.source_lineage_ids.clone();
    validate_sorted_strings("source_lineage_ids", &mut lineage, true)?;
    if lineage != artifact.source_lineage_ids {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge source_lineage_ids must be sorted and distinct",
            [("field", "source_lineage_ids")],
        ));
    }
    let mut source_pins = artifact.source_pins.clone();
    validate_source_pins("source_pins", &mut source_pins, true)?;
    if source_pins != artifact.source_pins {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge source_pins must be sorted and distinct",
            [("field", "source_pins")],
        ));
    }

    let stats = summarize_rows(&artifact.rows)?;
    if stats != artifact.stats {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge stats do not match rows",
            [("field", "stats")],
        ));
    }
    let mut previous_case_id: Option<&str> = None;
    for row in &artifact.rows {
        validate_case_row(row)?;
        if previous_case_id.is_some_and(|previous| previous >= row.case_id.as_str()) {
            return Err(GeoCondoError::invalid(
                "Geo condo bridge rows must be sorted by case_id and unique",
                [
                    ("field", "rows[].case_id"),
                    ("case_id", row.case_id.as_str()),
                ],
            ));
        }
        previous_case_id = Some(row.case_id.as_str());
    }
    Ok(())
}

fn bridge_case(
    case: &GeoCondoBridgeCaseRequest,
    index: &PadIndex,
    fallback_source_pins: &[GeoCondoSourcePin],
) -> Result<GeoCondoBridgeCase, GeoCondoError> {
    let truth_original = sorted_unique(&case.truth_parcels);
    let universe_original = sorted_unique(&case.universe_parcels);
    let before = reach_count(&truth_original, &universe_original)?;

    let mut lot_mappings = Vec::new();
    let mut unmapped_lots = Vec::new();
    let truth_billing = reexpress_lots(
        &truth_original,
        index,
        fallback_source_pins,
        true,
        &mut lot_mappings,
        &mut unmapped_lots,
    )?;
    let mut ignored_mappings = Vec::new();
    let mut ignored_unmapped = Vec::new();
    let universe_billing = reexpress_lots(
        &universe_original,
        index,
        fallback_source_pins,
        false,
        &mut ignored_mappings,
        &mut ignored_unmapped,
    )?;
    let after = reach_count(&truth_billing, &universe_billing)?;
    let unit_lots = checked_len(lot_mappings.len(), "unit_lots")?;
    let kind = classify_reach(before.truth_members_in_universe, &after);

    Ok(GeoCondoBridgeCase {
        case_id: case.case_id.clone(),
        loan_key: case.loan_key.clone(),
        unit_lots,
        lot_mappings,
        unmapped_lots,
        truth_original_grain: truth_original,
        universe_original_grain: universe_original,
        truth_billing_grain: truth_billing,
        universe_billing_grain: universe_billing,
        before,
        after,
        kind,
    })
}

fn reexpress_lots(
    lots: &[String],
    index: &PadIndex,
    fallback_source_pins: &[GeoCondoSourcePin],
    record_mappings: bool,
    mappings: &mut Vec<GeoCondoLotMapping>,
    unmapped: &mut Vec<GeoCondoUnmappedLot>,
) -> Result<Vec<String>, GeoCondoError> {
    let mut output = BTreeSet::new();
    for lot in lots {
        let resolution = resolve_unit_lot(lot, index, fallback_source_pins)?;
        if resolution.mapping.match_kind.is_some() {
            if record_mappings {
                mappings.push(resolution.mapping.clone());
                if let Some(unmapped_lot) = resolution.unmapped.clone() {
                    unmapped.push(unmapped_lot);
                }
            }
            if let Some(billing_lot) = resolution.mapping.billing_lot {
                output.insert(billing_lot);
            }
        } else {
            output.insert(lot.clone());
        }
    }
    Ok(output.into_iter().collect())
}

fn resolve_unit_lot(
    lot: &str,
    index: &PadIndex,
    fallback_source_pins: &[GeoCondoSourcePin],
) -> Result<LotResolution, GeoCondoError> {
    let block = block_key(lot)?;
    let matches = index
        .rows_by_block
        .get(block)
        .map(|rows| {
            rows.iter()
                .filter(|row| row_matches_lot(row, lot))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut billing_lots = BTreeSet::new();
    let mut pad_bbl_keys = BTreeSet::new();
    let mut condo_numbers = BTreeSet::new();
    let mut source_pins = BTreeSet::new();
    let mut has_exact = false;
    for row in matches {
        pad_bbl_keys.insert(row.bbl_key.clone());
        has_exact |= row.bbl_key == lot;
        if let Some(source_pin) = pad_source_pin_from_row(row) {
            source_pins.insert(source_pin);
        }
        if let Some(billing_lot) = &row.billing_bbl_key {
            billing_lots.insert(billing_lot.clone());
        }
        if let Some(condo_number) = row.condo_number {
            condo_numbers.insert(condo_number);
        }
    }

    let candidate_billing_lots = billing_lots.into_iter().collect::<Vec<_>>();
    let pad_bbl_keys = pad_bbl_keys.into_iter().collect::<Vec<_>>();
    if source_pins.is_empty() && !pad_bbl_keys.is_empty() {
        source_pins.extend(fallback_source_pins.iter().cloned());
    }
    let source_pins = source_pins.into_iter().collect::<Vec<_>>();
    let condo_number = single_value(&condo_numbers);
    let match_kind = (!pad_bbl_keys.is_empty()).then_some(if has_exact {
        GeoCondoPadMatchKind::ExactRow
    } else {
        GeoCondoPadMatchKind::Range
    });

    let (status, billing_lot, reason) = match candidate_billing_lots.as_slice() {
        [] if pad_bbl_keys.is_empty() => (
            GeoCondoLotMappingStatus::Unmapped,
            None,
            Some(GeoCondoUnmappedReason::NoPadBblRow),
        ),
        [] => (
            GeoCondoLotMappingStatus::Unmapped,
            None,
            Some(GeoCondoUnmappedReason::MissingBillingLot),
        ),
        [only] => (GeoCondoLotMappingStatus::Mapped, Some(only.clone()), None),
        _ => (
            GeoCondoLotMappingStatus::Unmapped,
            None,
            Some(GeoCondoUnmappedReason::AmbiguousBillingLots),
        ),
    };
    let mapping = GeoCondoLotMapping {
        unit_lot: lot.to_string(),
        status,
        billing_lot,
        condo_number,
        candidate_billing_lots: candidate_billing_lots.clone(),
        pad_bbl_keys: pad_bbl_keys.clone(),
        match_kind,
        source_pins: source_pins.clone(),
    };
    let unmapped = reason.map(|reason| GeoCondoUnmappedLot {
        unit_lot: lot.to_string(),
        reason,
        candidate_billing_lots,
        pad_bbl_keys,
        source_pins,
    });
    Ok(LotResolution { mapping, unmapped })
}

fn row_matches_lot(row: &GeoPadBblRow, lot: &str) -> bool {
    row.bbl_key == lot || (row.low_bbl_key.as_str() <= lot && lot <= row.high_bbl_key.as_str())
}

impl PadIndex {
    fn new(rows: &[GeoPadBblRow]) -> Self {
        let mut rows_by_block = BTreeMap::<String, Vec<GeoPadBblRow>>::new();
        for row in rows {
            if let Ok(block) = block_key(&row.low_bbl_key) {
                rows_by_block
                    .entry(block.to_string())
                    .or_default()
                    .push(row.clone());
            }
        }
        for rows in rows_by_block.values_mut() {
            rows.sort_by(|left, right| pad_row_sort_key(left).cmp(&pad_row_sort_key(right)));
        }
        Self { rows_by_block }
    }
}

fn summarize_rows(rows: &[GeoCondoBridgeCase]) -> Result<GeoCondoBridgeStats, GeoCondoError> {
    let mut stats = GeoCondoBridgeStats {
        cases: checked_len(rows.len(), "cases")?,
        fully_reached: 0,
        partial: 0,
        unreached: 0,
        truth_unit_lots: 0,
        truth_unit_lots_mapped: 0,
        truth_unit_lots_unmapped: 0,
    };
    for row in rows {
        match row.kind {
            GeoCondoBridgeReachKind::FullyReached => {
                checked_inc(&mut stats.fully_reached, "fully_reached")?
            }
            GeoCondoBridgeReachKind::Partial => checked_inc(&mut stats.partial, "partial")?,
            GeoCondoBridgeReachKind::Unreached => checked_inc(&mut stats.unreached, "unreached")?,
        }
        checked_add(&mut stats.truth_unit_lots, row.unit_lots, "truth_unit_lots")?;
        let unmapped = checked_len(row.unmapped_lots.len(), "truth_unit_lots_unmapped")?;
        checked_add(
            &mut stats.truth_unit_lots_unmapped,
            unmapped,
            "truth_unit_lots_unmapped",
        )?;
        let mapped = row
            .unit_lots
            .checked_sub(unmapped)
            .ok_or_else(|| GeoCondoError::overflow("truth_unit_lots_mapped"))?;
        checked_add(
            &mut stats.truth_unit_lots_mapped,
            mapped,
            "truth_unit_lots_mapped",
        )?;
    }
    Ok(stats)
}

fn classify_reach(before_in_universe: u64, after: &GeoCondoReachCount) -> GeoCondoBridgeReachKind {
    if after.truth_members > 0 && after.truth_members_in_universe == after.truth_members {
        GeoCondoBridgeReachKind::FullyReached
    } else if after.truth_members_in_universe > before_in_universe {
        GeoCondoBridgeReachKind::Partial
    } else {
        GeoCondoBridgeReachKind::Unreached
    }
}

fn reach_count(truth: &[String], universe: &[String]) -> Result<GeoCondoReachCount, GeoCondoError> {
    let universe = universe.iter().collect::<BTreeSet<_>>();
    Ok(GeoCondoReachCount {
        truth_members_in_universe: checked_len(
            truth
                .iter()
                .filter(|truth_lot| universe.contains(truth_lot))
                .count(),
            "truth_members_in_universe",
        )?,
        truth_members: checked_len(truth.len(), "truth_members")?,
    })
}

fn canonicalize_condo_unit_bridge_request(
    request: &GeoCondoUnitBridgeRequest,
) -> Result<GeoCondoUnitBridgeRequest, GeoCondoError> {
    if request.version != CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION {
        return Err(GeoCondoError::new(
            GeoCondoErrorCode::UnsupportedVersion,
            "Unsupported Geo condo bridge request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION),
            ],
        ));
    }
    validate_string("unit_bbl", &request.unit_bbl)?;
    validate_string("block", &request.block)?;
    validate_string("frame_id", &request.frame_id)?;

    let mut canonical = request.clone();
    validate_source_pins("source_pins", &mut canonical.source_pins, true)?;
    sort_dedup_strings(
        "billing_bbl_candidates",
        &mut canonical.billing_bbl_candidates,
    )?;
    sort_dedup_strings("bin_candidates", &mut canonical.bin_candidates)?;
    validate_polygon_map("parcel_rings", &canonical.parcel_rings)?;
    validate_polygon_map("footprint_rings", &canonical.footprint_rings)?;
    Ok(canonical)
}

fn confirmed_ledger_bridge(
    request: &GeoCondoUnitBridgeRequest,
    billing_bbl: &str,
    bins: &[String],
) -> Result<GeoLedgerBridge, GeoCondoError> {
    let unit = GeoEntityRef::new(GeoEntityLevel::PoiUnit, request.unit_bbl.clone());
    let parcel = GeoEntityRef::new(GeoEntityLevel::Parcel, billing_bbl.to_string());
    let mut relations = Vec::with_capacity(bins.len() + 1);
    relations.push((unit, GeoIdentityRelation::PartOf, parcel.clone()));
    for bin in bins {
        relations.push((
            GeoEntityRef::new(GeoEntityLevel::Building, bin.clone()),
            GeoIdentityRelation::On,
            parcel.clone(),
        ));
    }
    let bridge = GeoLedgerBridge {
        version: CANON_GEO_LEDGER_BRIDGE_VERSION.to_string(),
        unit_bbl: request.unit_bbl.clone(),
        billing_bbl: Some(billing_bbl.to_string()),
        bins: bins.to_vec(),
        confirmation: GeoCondoConfirmation::BlockAndGeometry,
        relations,
        source_pins: request.source_pins.clone(),
        abstained_reason: None,
    };
    validate_ledger_bridge_artifact(&bridge)?;
    Ok(bridge)
}

fn abstained_ledger_bridge(
    request: &GeoCondoUnitBridgeRequest,
    confirmation: GeoCondoConfirmation,
    reason: &str,
) -> Result<GeoLedgerBridge, GeoCondoError> {
    let bridge = GeoLedgerBridge {
        version: CANON_GEO_LEDGER_BRIDGE_VERSION.to_string(),
        unit_bbl: request.unit_bbl.clone(),
        billing_bbl: None,
        bins: Vec::new(),
        confirmation,
        relations: Vec::new(),
        source_pins: request.source_pins.clone(),
        abstained_reason: Some(format!("{GEO_CONDO_CONFIRMATION_INSUFFICIENT}:{reason}")),
    };
    validate_ledger_bridge_artifact(&bridge)?;
    Ok(bridge)
}

fn candidate_matches_declared_block(candidate: &str, block: &str) -> bool {
    if candidate == block || candidate.starts_with(block) {
        return true;
    }
    if candidate.len() == 10
        && candidate.bytes().all(|byte| byte.is_ascii_digit())
        && block.bytes().all(|byte| byte.is_ascii_digit())
    {
        let candidate_block = candidate[1..6].trim_start_matches('0');
        let declared_block = block.trim_start_matches('0');
        return !candidate_block.is_empty() && candidate_block == declared_block;
    }
    false
}

fn predicate_ring_from_polygon(
    frame_id: &str,
    field: &'static str,
    geometry_id: &str,
    polygon: &GeoCanonicalPolygonMm,
) -> Result<GeoLinearRingMm, GeoCondoError> {
    if !polygon.holes.is_empty() {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge geometry confirmation consumes exterior rings only",
            [
                ("field", field.to_string()),
                ("geometry_id", geometry_id.to_string()),
                ("holes", polygon.holes.len().to_string()),
            ],
        ));
    }
    let mut closed = polygon.exterior.vertices.clone();
    if closed.is_empty() {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge polygon exterior must contain vertices",
            [("field", field), ("geometry_id", geometry_id)],
        ));
    }
    closed.push(closed[0]);
    GeoLinearRingMm::new(frame_id.to_string(), closed)
        .map_err(|error| condo_predicate_error(field, geometry_id, error))
}

fn validate_polygon_map(
    field: &'static str,
    values: &BTreeMap<String, GeoCanonicalPolygonMm>,
) -> Result<(), GeoCondoError> {
    for (key, polygon) in values {
        validate_string(field, key)?;
        if polygon.exterior.vertices.is_empty() {
            return Err(GeoCondoError::invalid(
                "Geo condo bridge polygon exterior must contain vertices",
                [("field", field), ("geometry_id", key.as_str())],
            ));
        }
        for point in &polygon.exterior.vertices {
            validate_point_mm(field, key, *point)?;
        }
        for hole in &polygon.holes {
            if hole.vertices.is_empty() {
                return Err(GeoCondoError::invalid(
                    "Geo condo bridge polygon holes must contain vertices when present",
                    [("field", field), ("geometry_id", key.as_str())],
                ));
            }
            for point in &hole.vertices {
                validate_point_mm(field, key, *point)?;
            }
        }
    }
    Ok(())
}

fn validate_point_mm(
    field: &'static str,
    geometry_id: &str,
    point: GeoPointMm,
) -> Result<(), GeoCondoError> {
    if point.x == i64::MIN || point.y == i64::MIN {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge geometry points must stay inside the checked arithmetic domain",
            [
                ("field", field.to_string()),
                ("geometry_id", geometry_id.to_string()),
            ],
        ));
    }
    Ok(())
}

fn condo_area_error(billing_bbl: &str, bin: &str, error: GeoAreaMajorityError) -> GeoCondoError {
    let mut detail = error.detail;
    detail.insert("billing_bbl".to_string(), billing_bbl.to_string());
    detail.insert("bin".to_string(), bin.to_string());
    detail.insert("source_code".to_string(), format!("{:?}", error.code));
    detail.insert("source_message".to_string(), error.message);
    GeoCondoError::invalid(
        "Geo condo bridge geometry confirmation failed during area-majority evaluation",
        detail,
    )
}

fn condo_predicate_error(
    field: &'static str,
    geometry_id: &str,
    error: GeoPredicateError,
) -> GeoCondoError {
    let mut detail = error.detail;
    detail.insert("field".to_string(), field.to_string());
    detail.insert("geometry_id".to_string(), geometry_id.to_string());
    detail.insert("source_code".to_string(), format!("{:?}", error.code));
    detail.insert("source_message".to_string(), error.message);
    GeoCondoError::invalid(
        "Geo condo bridge geometry confirmation failed while validating a ring",
        detail,
    )
}

fn validate_pad_row(row: &GeoPadBblRow) -> Result<(), GeoCondoError> {
    validate_bbl("pad_rows[].BBL_KEY", &row.bbl_key)?;
    validate_bbl("pad_rows[].LOW_BBL_KEY", &row.low_bbl_key)?;
    validate_bbl("pad_rows[].HIGH_BBL_KEY", &row.high_bbl_key)?;
    if row.low_bbl_key > row.high_bbl_key {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge PAD row has LOW_BBL_KEY after HIGH_BBL_KEY",
            [
                ("low_bbl_key", row.low_bbl_key.as_str()),
                ("high_bbl_key", row.high_bbl_key.as_str()),
            ],
        ));
    }
    let low_block = block_key(&row.low_bbl_key)?;
    let high_block = block_key(&row.high_bbl_key)?;
    if low_block != high_block {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge PAD range must stay on one block",
            [
                ("low_bbl_key", row.low_bbl_key.as_str()),
                ("high_bbl_key", row.high_bbl_key.as_str()),
            ],
        ));
    }
    if let Some(billing_lot) = &row.billing_bbl_key {
        validate_bbl("pad_rows[].BILLING_BBL_KEY", billing_lot)?;
        if block_key(billing_lot)? != low_block {
            return Err(GeoCondoError::invalid(
                "Geo condo bridge PAD billing lot must be on the same block",
                [
                    ("billing_bbl_key", billing_lot.as_str()),
                    ("low_bbl_key", row.low_bbl_key.as_str()),
                ],
            ));
        }
    }
    if let Some(condo_flag) = &row.condo_flag {
        validate_string("pad_rows[].CONDO_FLAG", condo_flag)?;
    }
    validate_optional_string("pad_rows[].RELEASE", &row.release)?;
    validate_optional_string("pad_rows[].RELEASE_DT", &row.release_dt)?;
    validate_optional_string("pad_rows[].SOURCE_FILE", &row.source_file)?;
    validate_optional_string("pad_rows[].SOURCE_FILENAME", &row.source_filename)?;
    validate_optional_string("pad_rows[].PARSER_VERSION", &row.parser_version)?;
    validate_optional_string("pad_rows[].LICENSE_TERMS", &row.license_terms)?;
    validate_optional_string("pad_rows[].ATTRIBUTION_TEXT", &row.attribution_text)?;
    if let Some(source_row_number) = row.source_row_number
        && source_row_number == 0
    {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge PAD source row numbers are one-based",
            [("field", "pad_rows[].SOURCE_ROW_NUMBER")],
        ));
    }
    if let Some(source_zip_sha256) = &row.source_zip_sha256 {
        validate_sha256("pad_rows[].SOURCE_ZIP_SHA256", source_zip_sha256)?;
    }
    if pad_row_has_source_pin(row) {
        if row.release.is_none()
            || row.release_dt.is_none()
            || row.source_row_number.is_none()
            || row.source_zip_sha256.is_none()
            || row.parser_version.is_none()
        {
            return Err(GeoCondoError::invalid(
                "Geo condo bridge PAD provenance rows require release, source row, source hash, and parser pins",
                [("field", "pad_rows[]")],
            ));
        }
        if row.license_terms.is_none() || row.attribution_text.is_none() {
            return Err(GeoCondoError::invalid(
                "Geo condo bridge PAD provenance rows must carry license and attribution text",
                [("field", "pad_rows[]")],
            ));
        }
    }
    Ok(())
}

fn validate_source_pins(
    field: &str,
    pins: &mut Vec<GeoCondoSourcePin>,
    require_nonempty: bool,
) -> Result<(), GeoCondoError> {
    if require_nonempty && pins.is_empty() {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge source pins must be non-empty",
            [("field", field)],
        ));
    }
    for pin in pins.iter() {
        validate_source_pin(field, pin)?;
    }
    pins.sort();
    pins.dedup();
    Ok(())
}

fn validate_source_pin(field: &str, pin: &GeoCondoSourcePin) -> Result<(), GeoCondoError> {
    validate_string(&format!("{field}[].source_table"), &pin.source_table)?;
    validate_string(&format!("{field}[].natural_key"), &pin.natural_key)?;
    validate_string(&format!("{field}[].source_release"), &pin.source_release)?;
    validate_optional_string(&format!("{field}[].release_dt"), &pin.release_dt)?;
    validate_optional_string(&format!("{field}[].variant"), &pin.variant)?;
    validate_optional_string(&format!("{field}[].source_file"), &pin.source_file)?;
    validate_optional_string(
        &format!("{field}[].source_file_field"),
        &pin.source_file_field,
    )?;
    validate_optional_string(
        &format!("{field}[].source_content_sha256_field"),
        &pin.source_content_sha256_field,
    )?;
    validate_optional_string(&format!("{field}[].parser_version"), &pin.parser_version)?;
    validate_string(&format!("{field}[].license_terms"), &pin.license_terms)?;
    validate_string(
        &format!("{field}[].attribution_text"),
        &pin.attribution_text,
    )?;
    if let Some(source_row_number) = pin.source_row_number
        && source_row_number == 0
    {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge source pins use one-based source row numbers",
            [("field", format!("{field}[].source_row_number"))],
        ));
    }
    match (&pin.source_file, &pin.source_file_field) {
        (Some(_), Some(_)) | (None, None) => {}
        (Some(_), None) | (None, Some(_)) => {
            return Err(GeoCondoError::invalid(
                "Geo condo bridge source file pins require value and field name together",
                [("field", format!("{field}[].source_file"))],
            ));
        }
    }
    match (&pin.source_content_sha256, &pin.source_content_sha256_field) {
        (Some(source_content_sha256), Some(_)) => {
            validate_sha256(
                &format!("{field}[].source_content_sha256"),
                source_content_sha256,
            )?;
        }
        (None, None) => {}
        (Some(_), None) | (None, Some(_)) => {
            return Err(GeoCondoError::invalid(
                "Geo condo bridge source content pins require SHA-256 value and field name together",
                [("field", format!("{field}[].source_content_sha256"))],
            ));
        }
    }
    Ok(())
}

fn pad_row_has_source_pin(row: &GeoPadBblRow) -> bool {
    row.release.is_some()
        || row.release_dt.is_some()
        || row.source_row_number.is_some()
        || row.source_file.is_some()
        || row.source_filename.is_some()
        || row.source_zip_sha256.is_some()
        || row.parser_version.is_some()
        || row.license_terms.is_some()
        || row.attribution_text.is_some()
}

fn pad_source_pin_from_row(row: &GeoPadBblRow) -> Option<GeoCondoSourcePin> {
    let source_release = row.release.clone()?;
    let license_terms = row.license_terms.clone()?;
    let attribution_text = row.attribution_text.clone()?;
    let (source_file, source_file_field) = match (&row.source_file, &row.source_filename) {
        (Some(source_file), _) => (Some(source_file.clone()), Some("SOURCE_FILE".to_string())),
        (None, Some(source_filename)) => (
            Some(source_filename.clone()),
            Some("SOURCE_FILENAME".to_string()),
        ),
        (None, None) => (None, None),
    };
    Some(GeoCondoSourcePin {
        source_table: "EDGAR_DB.SOURCE.NYC_DCP_PAD_BBL_HOT".to_string(),
        natural_key: "release/source_row_number".to_string(),
        source_release,
        release_dt: row.release_dt.clone(),
        variant: None,
        source_row_number: row.source_row_number,
        source_file,
        source_file_field,
        source_content_sha256: row.source_zip_sha256.clone(),
        source_content_sha256_field: row
            .source_zip_sha256
            .as_ref()
            .map(|_| "SOURCE_ZIP_SHA256".to_string()),
        parser_version: row.parser_version.clone(),
        license_terms,
        attribution_text,
    })
}

fn validate_case_request(case: &GeoCondoBridgeCaseRequest) -> Result<(), GeoCondoError> {
    validate_string("cases[].case_id", &case.case_id)?;
    if let Some(loan_key) = &case.loan_key {
        validate_string("cases[].loan_key", loan_key)?;
    }
    for lot in &case.truth_parcels {
        validate_bbl("cases[].truth_parcels", lot)?;
    }
    for lot in &case.universe_parcels {
        validate_bbl("cases[].universe_parcels", lot)?;
    }
    if case.truth_parcels.is_empty() {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge cases must carry at least one truth parcel",
            [
                ("field", "truth_parcels"),
                ("case_id", case.case_id.as_str()),
            ],
        ));
    }
    Ok(())
}

fn validate_case_row(row: &GeoCondoBridgeCase) -> Result<(), GeoCondoError> {
    validate_string("rows[].case_id", &row.case_id)?;
    if let Some(loan_key) = &row.loan_key {
        validate_string("rows[].loan_key", loan_key)?;
    }
    validate_sorted_bbls("truth_original_grain", &row.truth_original_grain)?;
    validate_sorted_bbls("universe_original_grain", &row.universe_original_grain)?;
    validate_sorted_bbls("truth_billing_grain", &row.truth_billing_grain)?;
    validate_sorted_bbls("universe_billing_grain", &row.universe_billing_grain)?;
    if row.before.truth_members != checked_len(row.truth_original_grain.len(), "before")?
        || row.after.truth_members != checked_len(row.truth_billing_grain.len(), "after")?
    {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge reach denominators do not match grain members",
            [("case_id", row.case_id.as_str())],
        ));
    }
    if row.before.truth_members_in_universe > row.before.truth_members
        || row.after.truth_members_in_universe > row.after.truth_members
    {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge reach numerator exceeds denominator",
            [("case_id", row.case_id.as_str())],
        ));
    }
    if row.unit_lots != checked_len(row.lot_mappings.len(), "unit_lots")? {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge unit_lots must equal lot_mappings length",
            [("case_id", row.case_id.as_str())],
        ));
    }
    validate_mappings(&row.lot_mappings)?;
    validate_unmapped(&row.unmapped_lots)?;
    let expected_kind = classify_reach(row.before.truth_members_in_universe, &row.after);
    if row.kind != expected_kind {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge reach kind does not match before/after counts",
            [("case_id", row.case_id.as_str())],
        ));
    }
    Ok(())
}

fn validate_mappings(mappings: &[GeoCondoLotMapping]) -> Result<(), GeoCondoError> {
    let mut previous: Option<&str> = None;
    for mapping in mappings {
        validate_bbl("lot_mappings[].unit_lot", &mapping.unit_lot)?;
        if previous.is_some_and(|previous| previous >= mapping.unit_lot.as_str()) {
            return Err(GeoCondoError::invalid(
                "Geo condo bridge lot mappings must be sorted and unique",
                [("unit_lot", mapping.unit_lot.as_str())],
            ));
        }
        previous = Some(mapping.unit_lot.as_str());
        if let Some(billing_lot) = &mapping.billing_lot {
            validate_bbl("lot_mappings[].billing_lot", billing_lot)?;
        }
        validate_sorted_bbls(
            "lot_mappings[].candidate_billing_lots",
            &mapping.candidate_billing_lots,
        )?;
        validate_sorted_bbls("lot_mappings[].pad_bbl_keys", &mapping.pad_bbl_keys)?;
        let mut source_pins = mapping.source_pins.clone();
        validate_source_pins(
            "lot_mappings[].source_pins",
            &mut source_pins,
            mapping.match_kind.is_some(),
        )?;
        if source_pins != mapping.source_pins {
            return Err(GeoCondoError::invalid(
                "Geo condo bridge mapping source_pins must be sorted and distinct",
                [("unit_lot", mapping.unit_lot.as_str())],
            ));
        }
        match mapping.status {
            GeoCondoLotMappingStatus::Mapped => {
                if mapping.billing_lot.is_none() || mapping.candidate_billing_lots.len() != 1 {
                    return Err(GeoCondoError::invalid(
                        "Geo condo bridge mapped rows require exactly one billing lot",
                        [("unit_lot", mapping.unit_lot.as_str())],
                    ));
                }
            }
            GeoCondoLotMappingStatus::Unmapped => {
                if mapping.billing_lot.is_some() {
                    return Err(GeoCondoError::invalid(
                        "Geo condo bridge unmapped rows cannot carry a billing lot",
                        [("unit_lot", mapping.unit_lot.as_str())],
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_unmapped(unmapped: &[GeoCondoUnmappedLot]) -> Result<(), GeoCondoError> {
    let mut previous: Option<&str> = None;
    for lot in unmapped {
        validate_bbl("unmapped_lots[].unit_lot", &lot.unit_lot)?;
        if previous.is_some_and(|previous| previous >= lot.unit_lot.as_str()) {
            return Err(GeoCondoError::invalid(
                "Geo condo bridge unmapped lots must be sorted and unique",
                [("unit_lot", lot.unit_lot.as_str())],
            ));
        }
        previous = Some(lot.unit_lot.as_str());
        validate_sorted_bbls(
            "unmapped_lots[].candidate_billing_lots",
            &lot.candidate_billing_lots,
        )?;
        validate_sorted_bbls("unmapped_lots[].pad_bbl_keys", &lot.pad_bbl_keys)?;
        let mut source_pins = lot.source_pins.clone();
        validate_source_pins(
            "unmapped_lots[].source_pins",
            &mut source_pins,
            !lot.pad_bbl_keys.is_empty(),
        )?;
        if source_pins != lot.source_pins {
            return Err(GeoCondoError::invalid(
                "Geo condo bridge unmapped source_pins must be sorted and distinct",
                [("unit_lot", lot.unit_lot.as_str())],
            ));
        }
    }
    Ok(())
}

fn condo_bridge_request_blake3(request: &GeoCondoBridgeRequest) -> Result<String, GeoCondoError> {
    let bytes = canonical_condo_bridge_request_bytes(request)?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn pad_row_sort_key(row: &GeoPadBblRow) -> (&str, &str, &str, Option<&str>, Option<u64>) {
    (
        row.low_bbl_key.as_str(),
        row.high_bbl_key.as_str(),
        row.bbl_key.as_str(),
        row.billing_bbl_key.as_deref(),
        row.condo_number,
    )
}

fn reject_duplicate_cases(cases: &[GeoCondoBridgeCaseRequest]) -> Result<(), GeoCondoError> {
    let mut previous: Option<&str> = None;
    for case in cases {
        if previous.is_some_and(|previous| previous == case.case_id.as_str()) {
            return Err(GeoCondoError::invalid(
                "Geo condo bridge case ids must be unique",
                [("case_id", case.case_id.as_str())],
            ));
        }
        previous = Some(case.case_id.as_str());
    }
    Ok(())
}

fn single_value(values: &BTreeSet<u64>) -> Option<u64> {
    let mut iter = values.iter();
    let only = *iter.next()?;
    iter.next().is_none().then_some(only)
}

fn sorted_unique(values: &[String]) -> Vec<String> {
    values
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn sort_dedup_bbls(field: &str, values: &mut Vec<String>) -> Result<(), GeoCondoError> {
    values.sort();
    values.dedup();
    validate_sorted_bbls(field, values)
}

fn validate_sorted_bbls(field: &str, values: &[String]) -> Result<(), GeoCondoError> {
    let mut previous: Option<&str> = None;
    for value in values {
        validate_bbl(field, value)?;
        if previous.is_some_and(|previous| previous >= value.as_str()) {
            return Err(GeoCondoError::invalid(
                "Geo condo bridge BBL lists must be sorted and distinct",
                [("field", field), ("value", value.as_str())],
            ));
        }
        previous = Some(value.as_str());
    }
    Ok(())
}

fn validate_sorted_strings(
    field: &str,
    values: &mut Vec<String>,
    require_nonempty: bool,
) -> Result<(), GeoCondoError> {
    if require_nonempty && values.is_empty() {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge string list must be non-empty",
            [("field", field)],
        ));
    }
    values.sort();
    values.dedup();
    for value in values {
        validate_string(field, value)?;
    }
    Ok(())
}

fn sort_dedup_strings(field: &str, values: &mut Vec<String>) -> Result<(), GeoCondoError> {
    values.sort();
    values.dedup();
    validate_sorted_strings_readonly(field, values)
}

fn validate_sorted_strings_readonly(field: &str, values: &[String]) -> Result<(), GeoCondoError> {
    let mut previous: Option<&str> = None;
    for value in values {
        validate_string(field, value)?;
        if previous.is_some_and(|previous| previous >= value.as_str()) {
            return Err(GeoCondoError::invalid(
                "Geo condo bridge string lists must be sorted and distinct",
                [("field", field), ("value", value.as_str())],
            ));
        }
        previous = Some(value.as_str());
    }
    Ok(())
}

fn block_key(bbl: &str) -> Result<&str, GeoCondoError> {
    validate_bbl("bbl", bbl)?;
    Ok(&bbl[..6])
}

fn validate_bbl(field: &str, value: &str) -> Result<(), GeoCondoError> {
    if value.len() != 10 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge BBL values must be 10 ASCII digits",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_optional_string(field: &str, value: &Option<String>) -> Result<(), GeoCondoError> {
    if let Some(value) = value {
        validate_string(field, value)?;
    }
    Ok(())
}

fn validate_blake3_ref(field: &str, value: &str) -> Result<(), GeoCondoError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge digest must be blake3-prefixed lowercase hex",
            [("field", field), ("value", value)],
        ));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge digest must be blake3-prefixed lowercase hex",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_sha256(field: &str, value: &str) -> Result<(), GeoCondoError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge source content hashes must be lowercase SHA-256 hex",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_string(field: &str, value: &str) -> Result<(), GeoCondoError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoCondoError::invalid(
            "Geo condo bridge strings must be non-empty and already canonical",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn checked_len(value: usize, field: &str) -> Result<u64, GeoCondoError> {
    u64::try_from(value).map_err(|_| GeoCondoError::overflow(field))
}

fn checked_add(target: &mut u64, value: u64, field: &str) -> Result<(), GeoCondoError> {
    *target = target
        .checked_add(value)
        .ok_or_else(|| GeoCondoError::overflow(field))?;
    Ok(())
}

fn checked_inc(target: &mut u64, field: &str) -> Result<(), GeoCondoError> {
    checked_add(target, 1, field)
}

fn invalid_count(field: &'static str, value: usize) -> GeoCondoError {
    GeoCondoError::invalid(
        "Geo condo bridge count limits must be positive",
        [("field", field.to_string()), ("value", value.to_string())],
    )
}
