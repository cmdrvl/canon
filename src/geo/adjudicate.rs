#![forbid(unsafe_code)]

//! Adjudication crop request and receipt contracts.
//!
//! This module validates retained crop labels for the human-adjudication truth
//! plane. It never runs an observer, fetches imagery, or turns a label into a
//! candidate-generation input.

use super::{GeoCanonicalPolygonMm, GeoImageTilePin, GeoTruthPlane, GeoValidTimeInterval};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_ADJUDICATION_REQUEST_VERSION: &str = "canon_geo_adjudication_request.v0";
pub const CANON_GEO_ADJUDICATION_RECEIPT_VERSION: &str = "canon_geo_adjudication_receipt.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAdjudicationRequest {
    pub version: String,
    pub case_id: String,
    pub subject_id: String,
    pub tile_pin: GeoImageTilePin,
    pub window_blake3: String,
    pub candidate_parcel_ids: Vec<String>,
    pub overlay_geometry_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "parcel_ids", rename_all = "snake_case")]
pub enum GeoAdjudicationLabel {
    SelectedParcels(Vec<String>),
    NoneVisible,
    Unresolvable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAdjudicationReceipt {
    pub version: String,
    pub request_blake3: String,
    pub crop_blake3: String,
    pub label: GeoAdjudicationLabel,
    pub adjudicator_id: String,
    pub truth_plane: GeoTruthPlane,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes_blake3: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAdjudicationErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
    ImageTileDigestMismatch,
    AdjudicationLabelOutsideCandidates,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAdjudicationError {
    pub code: GeoAdjudicationErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoAdjudicationError {
    fn new(
        code: GeoAdjudicationErrorCode,
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
        Self::new(GeoAdjudicationErrorCode::InvalidInput, message, detail)
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

    fn digest_mismatch(
        field: &'static str,
        expected: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        Self::new(
            GeoAdjudicationErrorCode::ImageTileDigestMismatch,
            "Geo adjudication receipt digest does not match retained bytes",
            [
                ("field".to_string(), field.to_string()),
                (field.to_string(), expected.into()),
                ("actual".to_string(), actual.into()),
            ],
        )
    }

    fn label_scope(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(
            GeoAdjudicationErrorCode::AdjudicationLabelOutsideCandidates,
            message,
            detail,
        )
    }
}

impl fmt::Display for GeoAdjudicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoAdjudicationError {}

pub fn build_adjudication_requests(
    cases: &[(String, String, Vec<String>)],
    pins: &BTreeMap<String, GeoImageTilePin>,
    overlays: &BTreeMap<String, GeoCanonicalPolygonMm>,
) -> Result<Vec<GeoAdjudicationRequest>, GeoAdjudicationError> {
    if cases.is_empty() {
        return Err(GeoAdjudicationError::invalid_field(
            "cases",
            "Geo adjudication request builds require at least one case",
            "0",
        ));
    }

    let mut requests = Vec::with_capacity(cases.len());
    let mut seen_case_ids = BTreeSet::new();
    for (case_id, subject_id, candidate_parcel_ids) in cases {
        validate_text("case_id", case_id)?;
        validate_text("subject_id", subject_id)?;
        if !seen_case_ids.insert(case_id.as_str()) {
            return Err(GeoAdjudicationError::invalid_field(
                "case_id",
                "Geo adjudication request case ids must be unique",
                case_id.clone(),
            ));
        }
        let Some(tile_pin) = pins.get(subject_id) else {
            return Err(GeoAdjudicationError::invalid_field(
                "subject_id",
                "Geo adjudication request case has no retained tile pin",
                subject_id.clone(),
            ));
        };
        let Some(overlay) = overlays.get(case_id) else {
            return Err(GeoAdjudicationError::invalid_field(
                "case_id",
                "Geo adjudication request case has no overlay geometry",
                case_id.clone(),
            ));
        };
        let overlay_geometry_blake3 = polygon_blake3(overlay)?;
        let mut canonical_candidate_ids = candidate_parcel_ids.clone();
        canonical_candidate_ids.sort();
        canonical_candidate_ids.dedup();

        let request = GeoAdjudicationRequest {
            version: CANON_GEO_ADJUDICATION_REQUEST_VERSION.to_string(),
            case_id: case_id.clone(),
            subject_id: subject_id.clone(),
            tile_pin: tile_pin.clone(),
            window_blake3: overlay_geometry_blake3.clone(),
            candidate_parcel_ids: canonical_candidate_ids,
            overlay_geometry_blake3,
        };
        validate_adjudication_request_artifact(&request)?;
        requests.push(request);
    }

    requests.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    Ok(requests)
}

pub fn validate_adjudication_request_artifact(
    request: &GeoAdjudicationRequest,
) -> Result<(), GeoAdjudicationError> {
    if request.version != CANON_GEO_ADJUDICATION_REQUEST_VERSION {
        return Err(GeoAdjudicationError::new(
            GeoAdjudicationErrorCode::UnsupportedVersion,
            "Unsupported Geo adjudication request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_ADJUDICATION_REQUEST_VERSION),
            ],
        ));
    }
    validate_text("case_id", &request.case_id)?;
    validate_text("subject_id", &request.subject_id)?;
    validate_tile_pin(&request.tile_pin)?;
    validate_blake3("window_blake3", &request.window_blake3)?;
    validate_sorted_unique_strings("candidate_parcel_ids", &request.candidate_parcel_ids)?;
    validate_blake3("overlay_geometry_blake3", &request.overlay_geometry_blake3)?;
    Ok(())
}

pub fn canonical_adjudication_request_bytes(
    request: &GeoAdjudicationRequest,
) -> Result<Vec<u8>, GeoAdjudicationError> {
    validate_adjudication_request_artifact(request)?;
    serde_json::to_vec(request).map_err(|error| {
        GeoAdjudicationError::invalid(
            "Geo adjudication request could not be serialized",
            [("serde_error", error.to_string())],
        )
    })
}

pub fn adjudication_request_blake3(
    request: &GeoAdjudicationRequest,
) -> Result<String, GeoAdjudicationError> {
    let bytes = canonical_adjudication_request_bytes(request)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

pub fn validate_adjudication_receipt_artifact(
    receipt: &GeoAdjudicationReceipt,
) -> Result<(), GeoAdjudicationError> {
    if receipt.version != CANON_GEO_ADJUDICATION_RECEIPT_VERSION {
        return Err(GeoAdjudicationError::new(
            GeoAdjudicationErrorCode::UnsupportedVersion,
            "Unsupported Geo adjudication receipt version",
            [
                ("actual", receipt.version.as_str()),
                ("expected", CANON_GEO_ADJUDICATION_RECEIPT_VERSION),
            ],
        ));
    }
    validate_blake3("request_blake3", &receipt.request_blake3)?;
    validate_blake3("crop_blake3", &receipt.crop_blake3)?;
    validate_label_shape(&receipt.label)?;
    validate_text("adjudicator_id", &receipt.adjudicator_id)?;
    if let Some(notes_blake3) = &receipt.notes_blake3 {
        validate_blake3("notes_blake3", notes_blake3)?;
    }
    Ok(())
}

pub fn canonical_adjudication_receipt_bytes(
    receipt: &GeoAdjudicationReceipt,
) -> Result<Vec<u8>, GeoAdjudicationError> {
    validate_adjudication_receipt_artifact(receipt)?;
    serde_json::to_vec(receipt).map_err(|error| {
        GeoAdjudicationError::invalid(
            "Geo adjudication receipt could not be serialized",
            [("serde_error", error.to_string())],
        )
    })
}

pub fn validate_adjudication_receipt(
    request: &GeoAdjudicationRequest,
    receipt: &GeoAdjudicationReceipt,
    crop_bytes: &[u8],
) -> Result<(), GeoAdjudicationError> {
    validate_adjudication_request_artifact(request)?;
    validate_adjudication_receipt_artifact(receipt)?;

    let expected_request_blake3 = adjudication_request_blake3(request)?;
    if receipt.request_blake3 != expected_request_blake3 {
        return Err(GeoAdjudicationError::digest_mismatch(
            "request_blake3",
            expected_request_blake3,
            receipt.request_blake3.clone(),
        ));
    }

    let expected_crop_blake3 = blake3::hash(crop_bytes).to_hex().to_string();
    if receipt.crop_blake3 != expected_crop_blake3 {
        return Err(GeoAdjudicationError::digest_mismatch(
            "crop_blake3",
            expected_crop_blake3,
            receipt.crop_blake3.clone(),
        ));
    }

    if !receipt.adjudicator_id.starts_with("adjudicator:") {
        return Err(GeoAdjudicationError::label_scope(
            "Geo adjudication receipts require a human adjudicator namespace",
            [("adjudicator_id", receipt.adjudicator_id.as_str())],
        ));
    }

    if receipt.truth_plane != GeoTruthPlane::HumanAdjudication {
        return Err(GeoAdjudicationError::label_scope(
            "Geo adjudication receipts must stay on the human adjudication truth plane",
            [("truth_plane", truth_plane_name(receipt.truth_plane))],
        ));
    }

    if let GeoAdjudicationLabel::SelectedParcels(parcel_ids) = &receipt.label {
        let candidates = request
            .candidate_parcel_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        for parcel_id in parcel_ids {
            if !candidates.contains(parcel_id.as_str()) {
                return Err(GeoAdjudicationError::label_scope(
                    "Geo adjudication labels must select only shown candidate parcels",
                    [("parcel_id", parcel_id.as_str())],
                ));
            }
        }
    }

    Ok(())
}

fn polygon_blake3(polygon: &GeoCanonicalPolygonMm) -> Result<String, GeoAdjudicationError> {
    let bytes = serde_json::to_vec(polygon).map_err(|error| {
        GeoAdjudicationError::invalid(
            "Geo adjudication overlay geometry could not be serialized",
            [("serde_error", error.to_string())],
        )
    })?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn validate_tile_pin(pin: &GeoImageTilePin) -> Result<(), GeoAdjudicationError> {
    validate_text("tile_pin.url", &pin.url)?;
    if let Some((start, end)) = pin.byte_range
        && start > end
    {
        return Err(GeoAdjudicationError::invalid(
            "Geo adjudication tile pin byte ranges must be ordered",
            [
                ("field".to_string(), "tile_pin.byte_range".to_string()),
                ("start".to_string(), start.to_string()),
                ("end".to_string(), end.to_string()),
            ],
        ));
    }
    if let Some(etag) = &pin.etag {
        validate_text("tile_pin.etag", etag)?;
    }
    validate_blake3("tile_pin.blake3", &pin.blake3)?;
    validate_interval("tile_pin.vintage", pin.vintage)?;
    validate_text("tile_pin.license_id", &pin.license_id)?;
    validate_blake3("tile_pin.license_text_blake3", &pin.license_text_blake3)?;
    validate_text("tile_pin.source_dataset", &pin.source_dataset)?;
    Ok(())
}

fn validate_interval(
    field: &'static str,
    interval: GeoValidTimeInterval,
) -> Result<(), GeoAdjudicationError> {
    if interval.start_day > interval.end_day {
        return Err(GeoAdjudicationError::invalid(
            "Geo adjudication valid-time intervals must be ordered",
            [
                ("field".to_string(), field.to_string()),
                ("start_day".to_string(), interval.start_day.to_string()),
                ("end_day".to_string(), interval.end_day.to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_label_shape(label: &GeoAdjudicationLabel) -> Result<(), GeoAdjudicationError> {
    match label {
        GeoAdjudicationLabel::SelectedParcels(parcel_ids) => {
            validate_sorted_unique_strings("label.parcel_ids", parcel_ids)
        }
        GeoAdjudicationLabel::NoneVisible | GeoAdjudicationLabel::Unresolvable => Ok(()),
    }
}

fn validate_sorted_unique_strings(
    field: &'static str,
    values: &[String],
) -> Result<(), GeoAdjudicationError> {
    if values.is_empty() {
        return Err(GeoAdjudicationError::invalid_field(
            field,
            "Geo adjudication values must contain at least one entry",
            "0",
        ));
    }
    let mut previous: Option<&str> = None;
    for value in values {
        validate_text(field, value)?;
        if previous.is_some_and(|previous| previous >= value.as_str()) {
            return Err(GeoAdjudicationError::invalid_field(
                field,
                "Geo adjudication values must be sorted and unique",
                value.clone(),
            ));
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_blake3(field: &'static str, value: &str) -> Result<(), GeoAdjudicationError> {
    if value.len() != 64
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        || value.bytes().any(|byte| byte.is_ascii_uppercase())
    {
        return Err(GeoAdjudicationError::invalid_field(
            field,
            "Geo adjudication digests must be lowercase fixed-width BLAKE3 hex",
            value.to_string(),
        ));
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str) -> Result<(), GeoAdjudicationError> {
    if value.is_empty() || value.trim_matches(|ch: char| ch.is_ascii_whitespace()) != value {
        return Err(GeoAdjudicationError::invalid_field(
            field,
            "Geo adjudication strings must be non-empty and ASCII-trimmed",
            value.to_string(),
        ));
    }
    Ok(())
}

fn truth_plane_name(plane: GeoTruthPlane) -> &'static str {
    match plane {
        GeoTruthPlane::GateV2Historical => "gate_v2_historical",
        GeoTruthPlane::NonRoundAmountDateLegalBorough => "non_round_amount_date_legal_borough",
        GeoTruthPlane::RoundExactLenderParty => "round_exact_lender_party",
        GeoTruthPlane::AddressDerivedControl => "address_derived_control",
        GeoTruthPlane::DeedGrainInstrument => "deed_grain_instrument",
        GeoTruthPlane::HumanAdjudication => "human_adjudication",
    }
}
