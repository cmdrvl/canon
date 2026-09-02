#![forbid(unsafe_code)]

//! Retry-population fixtures for bounded Geo reacquisition measurements.

use h3o::CellIndex;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fmt, str::FromStr};

pub const CANON_GEO_POINT_POPULATION_VERSION: &str = "canon_geo_point_population.v0";

const NYC_MIN_LON_E7: i64 = -743_000_000;
const NYC_MAX_LON_E7: i64 = -736_500_000;
const NYC_MIN_LAT_E7: i64 = 404_500_000;
const NYC_MAX_LAT_E7: i64 = 410_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPointPopulationReleasePin {
    pub source_dataset: String,
    pub source_release: String,
    pub release_dt: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPointPopulationArtifact {
    pub version: String,
    pub population_id: String,
    pub source_dataset: String,
    pub selection_query_sha256: String,
    pub release_pins: Vec<GeoPointPopulationReleasePin>,
    pub points: Vec<GeoPointPopulationPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPointPopulationPoint {
    pub point_id: String,
    pub subject_id: String,
    pub loan_key: String,
    pub asserted_address_blake3: String,
    pub landed_geocode: GeoPointPopulationGeocode,
    pub home_cell_r9: String,
    pub refuter_fired: bool,
    pub e1_failure_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pip_lot_bbl: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pad_billing_bbl_candidates: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pad_unit_bbl: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_equals_pip: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPointPopulationGeocode {
    pub lon_e7: i64,
    pub lat_e7: i64,
    pub accuracy_type: String,
    pub source_attribution: String,
    pub geocode_asof: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPointPopulationErrorCode {
    UnsupportedVersion,
    InvalidInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPointPopulationError {
    pub code: GeoPointPopulationErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoPointPopulationError {
    fn new(
        code: GeoPointPopulationErrorCode,
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
        Self::new(GeoPointPopulationErrorCode::InvalidInput, message, detail)
    }
}

impl fmt::Display for GeoPointPopulationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoPointPopulationError {}

pub fn validate_point_population_artifact(
    artifact: &GeoPointPopulationArtifact,
) -> Result<(), GeoPointPopulationError> {
    if artifact.version != CANON_GEO_POINT_POPULATION_VERSION {
        return Err(GeoPointPopulationError::new(
            GeoPointPopulationErrorCode::UnsupportedVersion,
            "Unsupported Geo point-population artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_POINT_POPULATION_VERSION),
            ],
        ));
    }
    validate_point_population_string("population_id", &artifact.population_id)?;
    validate_point_population_string("source_dataset", &artifact.source_dataset)?;
    if !artifact.source_dataset.starts_with("fixture.") {
        return Err(point_invalid_field(
            "source_dataset",
            "Geo point-population fixtures must declare a fixture source dataset",
            artifact.source_dataset.as_str(),
        ));
    }
    validate_sha256("selection_query_sha256", &artifact.selection_query_sha256)?;
    validate_release_pins(&artifact.release_pins)?;
    if artifact.points.is_empty() {
        return Err(point_invalid_field(
            "points",
            "Geo point-population artifacts must contain at least one point",
            "0",
        ));
    }

    let mut previous_point_id: Option<&str> = None;
    for point in &artifact.points {
        validate_point(point)?;
        if let Some(previous) = previous_point_id
            && previous >= point.point_id.as_str()
        {
            return Err(GeoPointPopulationError::invalid(
                "Geo point-population points must be strictly sorted by point_id",
                [
                    ("field", "points[].point_id".to_string()),
                    ("previous_point_id", previous.to_string()),
                    ("point_id", point.point_id.clone()),
                ],
            ));
        }
        previous_point_id = Some(point.point_id.as_str());
    }

    Ok(())
}

pub fn canonical_point_population_bytes(
    artifact: &GeoPointPopulationArtifact,
) -> Result<Vec<u8>, GeoPointPopulationError> {
    validate_point_population_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoPointPopulationError::invalid(
            "Geo point-population artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

fn validate_release_pins(
    release_pins: &[GeoPointPopulationReleasePin],
) -> Result<(), GeoPointPopulationError> {
    if release_pins.is_empty() {
        return Err(point_invalid_field(
            "release_pins",
            "Geo point-population artifacts must pin at least one source release",
            "0",
        ));
    }
    let mut previous_key: Option<String> = None;
    for pin in release_pins {
        validate_point_population_string("release_pins[].source_dataset", &pin.source_dataset)?;
        validate_point_population_string("release_pins[].source_release", &pin.source_release)?;
        validate_point_population_string("release_pins[].release_dt", &pin.release_dt)?;
        validate_point_population_string("release_pins[].role", &pin.role)?;
        if let Some(variant) = &pin.variant {
            validate_point_population_string("release_pins[].variant", variant)?;
        }
        let key = format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}",
            pin.source_dataset,
            pin.source_release,
            pin.release_dt,
            pin.variant.as_deref().unwrap_or("")
        );
        if let Some(previous) = &previous_key
            && previous >= &key
        {
            return Err(GeoPointPopulationError::invalid(
                "Geo point-population release pins must be strictly sorted and unique",
                [
                    ("field", "release_pins".to_string()),
                    ("source_dataset", pin.source_dataset.clone()),
                ],
            ));
        }
        previous_key = Some(key);
    }
    Ok(())
}

fn validate_point(point: &GeoPointPopulationPoint) -> Result<(), GeoPointPopulationError> {
    validate_point_population_string("points[].point_id", &point.point_id)?;
    validate_point_population_string("points[].subject_id", &point.subject_id)?;
    validate_point_population_string("points[].loan_key", &point.loan_key)?;
    validate_blake3("points[].asserted_address_blake3", &point.asserted_address_blake3)?;
    validate_point_population_string(
        "points[].landed_geocode.accuracy_type",
        &point.landed_geocode.accuracy_type,
    )?;
    validate_point_population_string(
        "points[].landed_geocode.source_attribution",
        &point.landed_geocode.source_attribution,
    )?;
    validate_point_population_string(
        "points[].landed_geocode.geocode_asof",
        &point.landed_geocode.geocode_asof,
    )?;
    validate_nyc_e7_bbox(point)?;
    validate_home_cell_r9(point)?;

    match point.e1_failure_class.as_str() {
        "gross" => validate_gross_point(point),
        "condo" => validate_condo_point(point),
        _ => Err(GeoPointPopulationError::invalid(
            "Geo point-population E1 failure class is unsupported",
            [
                ("field", "points[].e1_failure_class".to_string()),
                ("point_id", point.point_id.clone()),
                ("value", point.e1_failure_class.clone()),
            ],
        )),
    }
}

fn validate_gross_point(point: &GeoPointPopulationPoint) -> Result<(), GeoPointPopulationError> {
    if point.pip_lot_bbl.is_some()
        || !point.pad_billing_bbl_candidates.is_empty()
        || point.pad_unit_bbl.is_some()
        || point.block.is_some()
        || point.billing_equals_pip.is_some()
    {
        return Err(GeoPointPopulationError::invalid(
            "Gross E1 points must not carry condo crosswalk fields",
            [
                ("field", "points[].condo_fields".to_string()),
                ("point_id", point.point_id.clone()),
            ],
        ));
    }
    Ok(())
}

fn validate_condo_point(point: &GeoPointPopulationPoint) -> Result<(), GeoPointPopulationError> {
    validate_required_option(point, "points[].pip_lot_bbl", point.pip_lot_bbl.as_deref())?;
    validate_required_option(point, "points[].pad_unit_bbl", point.pad_unit_bbl.as_deref())?;
    validate_required_option(point, "points[].block", point.block.as_deref())?;
    if point.billing_equals_pip.is_none() {
        return Err(GeoPointPopulationError::invalid(
            "Condo E1 points must carry billing_equals_pip",
            [
                ("field", "points[].billing_equals_pip".to_string()),
                ("point_id", point.point_id.clone()),
            ],
        ));
    }
    if point.pad_billing_bbl_candidates.is_empty() {
        return Err(GeoPointPopulationError::invalid(
            "Condo E1 points must carry at least one PAD billing BBL candidate",
            [
                (
                    "field",
                    "points[].pad_billing_bbl_candidates".to_string(),
                ),
                ("point_id", point.point_id.clone()),
            ],
        ));
    }
    let mut previous: Option<&str> = None;
    for candidate in &point.pad_billing_bbl_candidates {
        validate_point_population_string("points[].pad_billing_bbl_candidates[]", candidate)?;
        if let Some(previous_candidate) = previous
            && previous_candidate >= candidate.as_str()
        {
            return Err(GeoPointPopulationError::invalid(
                "PAD billing BBL candidates must be strictly sorted and unique",
                [
                    (
                        "field",
                        "points[].pad_billing_bbl_candidates".to_string(),
                    ),
                    ("point_id", point.point_id.clone()),
                    ("value", candidate.clone()),
                ],
            ));
        }
        previous = Some(candidate.as_str());
    }
    Ok(())
}

fn validate_required_option(
    point: &GeoPointPopulationPoint,
    field: &'static str,
    value: Option<&str>,
) -> Result<(), GeoPointPopulationError> {
    match value {
        Some(value) => validate_point_population_string(field, value),
        None => Err(GeoPointPopulationError::invalid(
            "Condo E1 points must carry the PAD condo crosswalk fields",
            [("field", field.to_string()), ("point_id", point.point_id.clone())],
        )),
    }
}

fn validate_nyc_e7_bbox(point: &GeoPointPopulationPoint) -> Result<(), GeoPointPopulationError> {
    let lon = point.landed_geocode.lon_e7;
    let lat = point.landed_geocode.lat_e7;
    if !(NYC_MIN_LON_E7..=NYC_MAX_LON_E7).contains(&lon)
        || !(NYC_MIN_LAT_E7..=NYC_MAX_LAT_E7).contains(&lat)
    {
        return Err(GeoPointPopulationError::invalid(
            "Geo point-population landed geocode is outside the NYC bounding box",
            [
                ("field", "landed_geocode".to_string()),
                ("point_id", point.point_id.clone()),
                ("lon_e7", lon.to_string()),
                ("lat_e7", lat.to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_home_cell_r9(point: &GeoPointPopulationPoint) -> Result<(), GeoPointPopulationError> {
    let cell = CellIndex::from_str(&point.home_cell_r9).map_err(|error| {
        GeoPointPopulationError::invalid(
            "Geo point-population home_cell_r9 is not a valid H3 cell",
            [
                ("field", "home_cell_r9".to_string()),
                ("point_id", point.point_id.clone()),
                ("value", point.home_cell_r9.clone()),
                ("error", error.to_string()),
            ],
        )
    })?;
    if u8::from(cell.resolution()) != 9 {
        return Err(GeoPointPopulationError::invalid(
            "Geo point-population home_cell_r9 must be resolution 9",
            [
                ("field", "home_cell_r9".to_string()),
                ("point_id", point.point_id.clone()),
                ("value", point.home_cell_r9.clone()),
            ],
        ));
    }
    Ok(())
}

fn validate_point_population_string(
    field: &'static str,
    value: &str,
) -> Result<(), GeoPointPopulationError> {
    if value.is_empty() || value.trim() != value {
        return Err(point_invalid_field(
            field,
            "Geo point-population string fields must be non-empty and canonical-trimmed",
            value,
        ));
    }
    Ok(())
}

fn validate_blake3(field: &'static str, value: &str) -> Result<(), GeoPointPopulationError> {
    validate_hex_hash(field, value, 64)
}

fn validate_sha256(field: &'static str, value: &str) -> Result<(), GeoPointPopulationError> {
    validate_hex_hash(field, value, 64)
}

fn validate_hex_hash(
    field: &'static str,
    value: &str,
    length: usize,
) -> Result<(), GeoPointPopulationError> {
    if value.len() != length
        || !value.chars().all(|ch| ch.is_ascii_hexdigit())
        || value.chars().any(|ch| ch.is_ascii_uppercase())
    {
        return Err(point_invalid_field(
            field,
            "Geo point-population hash fields must be lowercase fixed-width hex",
            value,
        ));
    }
    Ok(())
}

fn point_invalid_field(
    field: &'static str,
    message: impl Into<String>,
    value: &str,
) -> GeoPointPopulationError {
    GeoPointPopulationError::invalid(
        message,
        [("field", field.to_string()), ("value", value.to_string())],
    )
}
