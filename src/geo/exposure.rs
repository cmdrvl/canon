#![forbid(unsafe_code)]

//! Loan-grain storm exposure over already-built collateral ledger geometry.
//!
//! Exposure is a downstream join over ledger building sets and pinned advisory
//! wind-radius rings. It does not create collateral truth, candidate reach, or
//! rho evidence.

use super::{
    GeoCandidateReachStatus, GeoClaimClass, GeoCollateralLedger, GeoCollateralLedgerProofClass,
    GeoLedgerError, GeoValidTimeInterval,
    geometry::{
        GeoAreaMajorityError, GeoLinearRingMm, GeoPointMm, GeoPredicateError,
        footprint_majority_area_inside_parcel,
    },
    geometry_value::{GeoCanonicalGeometryMm, GeoCanonicalPolygonMm},
    ledger::canonical_collateral_ledger_bytes,
    ledger::validate_collateral_ledger_artifact,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_EVENT_EXPOSURE_VERSION: &str = "canon_geo_event_exposure.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoWindRadiusRing {
    pub knots: u16,
    pub ring: GeoCanonicalPolygonMm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAdvisoryPin {
    pub advisory_id: String,
    pub storm_id: String,
    pub advisory_number: u32,
    pub issued: GeoValidTimeInterval,
    pub source_blake3s: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub frame_id: String,
    pub wind_radii: Vec<GeoWindRadiusRing>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoArchivedAdvisoryRef {
    pub storm_id: String,
    pub advisory_number: u32,
    pub source_blake3s: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAdvisoryArchive {
    #[serde(default)]
    pub source_blake3s: Vec<String>,
    #[serde(default)]
    pub advisories: Vec<GeoArchivedAdvisoryRef>,
}

impl GeoAdvisoryArchive {
    pub fn from_blake3s(source_blake3s: &[String]) -> Self {
        Self {
            source_blake3s: sorted_unique(source_blake3s.to_vec()),
            advisories: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoExposureGeometryInput {
    pub frame_id: String,
    pub buildings: BTreeMap<String, GeoCanonicalGeometryMm>,
}

impl GeoExposureGeometryInput {
    pub fn from_polygons(
        frame_id: impl Into<String>,
        buildings: &BTreeMap<String, GeoCanonicalPolygonMm>,
    ) -> Self {
        Self {
            frame_id: frame_id.into(),
            buildings: buildings
                .iter()
                .map(|(building_id, polygon)| {
                    (
                        building_id.clone(),
                        GeoCanonicalGeometryMm::Polygon {
                            polygon: polygon.clone(),
                        },
                    )
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoExposedBuilding {
    pub accession: String,
    pub loan_id: String,
    pub building_id: String,
    pub knots_band: u16,
    #[serde(default)]
    pub backbone_member: bool,
    #[serde(default = "default_claim_class")]
    pub claim_class: GeoClaimClass,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoUnexposedBuilding {
    pub accession: String,
    pub loan_id: String,
    pub building_id: String,
    #[serde(default)]
    pub backbone_member: bool,
    #[serde(default = "default_claim_class")]
    pub claim_class: GeoClaimClass,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoExposureAccessionSummary {
    pub accession: String,
    pub loans_exposed: u64,
    pub buildings_exposed: u64,
    pub max_knots_band: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoEventExposure {
    pub version: String,
    #[serde(default = "default_proof_class")]
    pub proof_class: GeoCollateralLedgerProofClass,
    pub advisory: GeoAdvisoryPin,
    pub ledger_blake3: String,
    pub exposed: Vec<GeoExposedBuilding>,
    #[serde(default)]
    pub not_exposed: Vec<GeoUnexposedBuilding>,
    pub buildings_without_geometry: Vec<String>,
    #[serde(default)]
    pub holes_ignored: bool,
    #[serde(default)]
    pub per_accession: Vec<GeoExposureAccessionSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoExposureErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
    ExposureAdvisoryStale,
    ExposureGeometryMissing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoExposureError {
    pub code: GeoExposureErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoExposureError {
    fn new(
        code: GeoExposureErrorCode,
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
        Self::new(GeoExposureErrorCode::InvalidInput, message, detail)
    }

    fn invalid_field(field: impl Into<String>, value: impl Into<String>) -> Self {
        Self::invalid(
            "Geo event exposure input contains an invalid field",
            [("field", field.into()), ("value", value.into())],
        )
    }

    fn advisory_stale(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(GeoExposureErrorCode::ExposureAdvisoryStale, message, detail)
    }

    fn geometry_missing(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(
            GeoExposureErrorCode::ExposureGeometryMissing,
            message,
            detail,
        )
    }

    fn overflow(field: impl Into<String>) -> Self {
        Self::new(
            GeoExposureErrorCode::ArithmeticOverflow,
            "Geo event exposure accounting overflowed",
            [("field", field.into())],
        )
    }
}

impl fmt::Display for GeoExposureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeoExposureError {}

pub fn join_exposure(
    ledger: &GeoCollateralLedger,
    advisory: &GeoAdvisoryPin,
    geometry: &BTreeMap<String, GeoCanonicalPolygonMm>,
    archive_blake3s: &[String],
) -> Result<GeoEventExposure, GeoExposureError> {
    let geometry_input = GeoExposureGeometryInput::from_polygons(&advisory.frame_id, geometry);
    let archive = GeoAdvisoryArchive::from_blake3s(archive_blake3s);
    join_exposure_from_geometry_input(ledger, advisory, &geometry_input, &archive)
}

pub fn join_exposure_from_geometry_input(
    ledger: &GeoCollateralLedger,
    advisory: &GeoAdvisoryPin,
    geometry: &GeoExposureGeometryInput,
    archive: &GeoAdvisoryArchive,
) -> Result<GeoEventExposure, GeoExposureError> {
    validate_collateral_ledger_artifact(ledger).map_err(ledger_error)?;
    validate_geometry_input(geometry)?;
    let advisory = canonical_advisory_pin(advisory)?;
    validate_advisory_archive(&advisory, archive)?;
    if advisory.frame_id != geometry.frame_id {
        return Err(GeoExposureError::invalid(
            "Geo event exposure advisory and building geometry must share a frame",
            [
                ("field", "frame_id"),
                ("advisory_frame_id", advisory.frame_id.as_str()),
                ("geometry_frame_id", geometry.frame_id.as_str()),
            ],
        ));
    }

    let ledger_blake3 =
        digest_prefixed(&canonical_collateral_ledger_bytes(ledger).map_err(ledger_error)?);
    let building_refs = ledger_building_refs(ledger)?;
    if building_refs.is_empty() {
        return Err(GeoExposureError::invalid(
            "Geo event exposure requires at least one ledger building",
            [("field", "ledger.rows[].building_set")],
        ));
    }

    let mut exposed = Vec::new();
    let mut not_exposed = Vec::new();
    let mut buildings_without_geometry = BTreeSet::new();
    let mut missing_geometry_count = 0usize;

    for building in &building_refs {
        let Some(polygon) = polygon_for_building(geometry, &building.building_id) else {
            buildings_without_geometry.insert(building.building_id.clone());
            missing_geometry_count = checked_inc_usize(missing_geometry_count, "missing_geometry")?;
            continue;
        };
        let Some(knots_band) = highest_containing_band(&advisory, polygon)? else {
            not_exposed.push(GeoUnexposedBuilding {
                accession: building.accession.clone(),
                loan_id: building.loan_id.clone(),
                building_id: building.building_id.clone(),
                backbone_member: building.backbone_member,
                claim_class: building.claim_class,
            });
            continue;
        };
        exposed.push(GeoExposedBuilding {
            accession: building.accession.clone(),
            loan_id: building.loan_id.clone(),
            building_id: building.building_id.clone(),
            knots_band,
            backbone_member: building.backbone_member,
            claim_class: building.claim_class,
        });
    }

    if missing_geometry_count == building_refs.len() {
        let building_id = building_refs
            .first()
            .map(|building| building.building_id.as_str())
            .unwrap_or("");
        return Err(GeoExposureError::geometry_missing(
            "Geo event exposure cannot run when every ledger building lacks polygon geometry",
            [
                ("field".to_string(), "buildings".to_string()),
                ("building_id".to_string(), building_id.to_string()),
                (
                    "missing_count".to_string(),
                    missing_geometry_count.to_string(),
                ),
                (
                    "building_count".to_string(),
                    building_refs.len().to_string(),
                ),
            ],
        ));
    }

    exposed.sort();
    not_exposed.sort();
    let buildings_without_geometry = buildings_without_geometry.into_iter().collect();
    let per_accession = summarize_per_accession(&exposed)?;

    let artifact = GeoEventExposure {
        version: CANON_GEO_EVENT_EXPOSURE_VERSION.to_string(),
        proof_class: ledger.proof_class,
        advisory,
        ledger_blake3,
        exposed,
        not_exposed,
        buildings_without_geometry,
        holes_ignored: true,
        per_accession,
    };
    validate_event_exposure_artifact(&artifact)?;
    Ok(artifact)
}

pub fn validate_event_exposure_artifact(
    artifact: &GeoEventExposure,
) -> Result<(), GeoExposureError> {
    if artifact.version != CANON_GEO_EVENT_EXPOSURE_VERSION {
        return Err(GeoExposureError::new(
            GeoExposureErrorCode::UnsupportedVersion,
            "Unsupported Geo event exposure artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_EVENT_EXPOSURE_VERSION),
            ],
        ));
    }
    validate_advisory_pin(&artifact.advisory)?;
    validate_prefixed_blake3("ledger_blake3", &artifact.ledger_blake3)?;
    validate_exposed(&artifact.exposed, &artifact.advisory)?;
    validate_unexposed(&artifact.not_exposed)?;
    validate_sorted_unique(
        "buildings_without_geometry",
        &artifact.buildings_without_geometry,
    )?;
    for building_id in &artifact.buildings_without_geometry {
        validate_text("buildings_without_geometry[]", building_id)?;
    }
    validate_disjoint_classifications(artifact)?;
    let expected_summary = summarize_per_accession(&artifact.exposed)?;
    if artifact.per_accession != expected_summary {
        return Err(GeoExposureError::invalid(
            "Geo event exposure per-accession summary does not match exposed rows",
            [("field", "per_accession")],
        ));
    }
    Ok(())
}

pub fn canonical_event_exposure_bytes(
    artifact: &GeoEventExposure,
) -> Result<Vec<u8>, GeoExposureError> {
    validate_event_exposure_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoExposureError::invalid(
            "Geo event exposure artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LedgerBuildingRef {
    accession: String,
    loan_id: String,
    building_id: String,
    backbone_member: bool,
    claim_class: GeoClaimClass,
}

fn ledger_building_refs(
    ledger: &GeoCollateralLedger,
) -> Result<Vec<LedgerBuildingRef>, GeoExposureError> {
    let mut refs = Vec::new();
    for row in &ledger.rows {
        if row.reach == GeoCandidateReachStatus::None {
            continue;
        }
        let mut buildings = BTreeMap::<String, bool>::new();
        for building_id in &row.ambiguous_building_set {
            validate_text("ambiguous_building_set[]", building_id)?;
            buildings.insert(building_id.clone(), false);
        }
        if let Some(building_set) = row.building_set.as_ref() {
            for building_id in building_set {
                validate_text("building_set[]", building_id)?;
                buildings.insert(building_id.clone(), true);
            }
        }
        for (building_id, backbone_member) in buildings {
            refs.push(LedgerBuildingRef {
                accession: row.accession.clone(),
                loan_id: row.loan_id.clone(),
                building_id,
                backbone_member,
                claim_class: row.claim_class,
            });
        }
    }
    refs.sort();
    Ok(refs)
}

fn polygon_for_building<'a>(
    geometry: &'a GeoExposureGeometryInput,
    building_id: &str,
) -> Option<&'a GeoCanonicalPolygonMm> {
    match geometry.buildings.get(building_id) {
        Some(GeoCanonicalGeometryMm::Polygon { polygon }) => Some(polygon),
        Some(GeoCanonicalGeometryMm::Point { .. })
        | Some(GeoCanonicalGeometryMm::MultiPolygon { .. })
        | None => None,
    }
}

fn highest_containing_band(
    advisory: &GeoAdvisoryPin,
    building: &GeoCanonicalPolygonMm,
) -> Result<Option<u16>, GeoExposureError> {
    let building_ring = predicate_ring_from_polygon(&advisory.frame_id, "buildings", "", building)?;
    for wind_radius in advisory.wind_radii.iter().rev() {
        let wind_ring = predicate_ring_from_polygon(
            &advisory.frame_id,
            "wind_radii",
            &wind_radius.knots.to_string(),
            &wind_radius.ring,
        )?;
        if footprint_majority_area_inside_parcel(&building_ring, &wind_ring)
            .map_err(area_majority_error)?
        {
            return Ok(Some(wind_radius.knots));
        }
    }
    Ok(None)
}

fn summarize_per_accession(
    exposed: &[GeoExposedBuilding],
) -> Result<Vec<GeoExposureAccessionSummary>, GeoExposureError> {
    let mut summaries = BTreeMap::<String, (BTreeSet<String>, BTreeSet<String>, u16)>::new();
    for building in exposed {
        let entry = summaries.entry(building.accession.clone()).or_insert((
            BTreeSet::new(),
            BTreeSet::new(),
            0,
        ));
        entry.0.insert(building.loan_id.clone());
        entry.1.insert(building.building_id.clone());
        entry.2 = entry.2.max(building.knots_band);
    }
    summaries
        .into_iter()
        .map(|(accession, (loans, buildings, max_knots_band))| {
            Ok(GeoExposureAccessionSummary {
                accession,
                loans_exposed: usize_to_u64(loans.len(), "per_accession.loans_exposed")?,
                buildings_exposed: usize_to_u64(
                    buildings.len(),
                    "per_accession.buildings_exposed",
                )?,
                max_knots_band,
            })
        })
        .collect()
}

fn canonical_advisory_pin(advisory: &GeoAdvisoryPin) -> Result<GeoAdvisoryPin, GeoExposureError> {
    validate_text("advisory_id", &advisory.advisory_id)?;
    validate_text("storm_id", &advisory.storm_id)?;
    validate_text("frame_id", &advisory.frame_id)?;
    if advisory.advisory_number == 0 {
        return Err(GeoExposureError::invalid_field(
            "advisory_number",
            advisory.advisory_number.to_string(),
        ));
    }
    validate_interval("issued", advisory.issued)?;
    validate_nonempty("source_blake3s", &advisory.source_blake3s)?;
    validate_nonempty("wind_radii", &advisory.wind_radii)?;

    let source_blake3s = sorted_unique(advisory.source_blake3s.clone());
    if source_blake3s.len() != advisory.source_blake3s.len() {
        return Err(GeoExposureError::invalid(
            "Geo event exposure advisory source pins must be unique",
            [("field", "source_blake3s")],
        ));
    }
    for source_blake3 in &source_blake3s {
        validate_prefixed_blake3("source_blake3s[]", source_blake3)?;
    }

    let mut wind_radii = advisory.wind_radii.clone();
    wind_radii.sort_by_key(|radius| radius.knots);
    let mut seen_knots = BTreeSet::new();
    for radius in &wind_radii {
        if radius.knots == 0 {
            return Err(GeoExposureError::invalid_field(
                "wind_radii[].knots",
                radius.knots.to_string(),
            ));
        }
        if !seen_knots.insert(radius.knots) {
            return Err(GeoExposureError::invalid(
                "Geo event exposure advisory wind radius bands must be unique",
                [("field", "wind_radii[].knots")],
            ));
        }
        validate_polygon("wind_radii[].ring", &radius.knots.to_string(), &radius.ring)?;
    }

    Ok(GeoAdvisoryPin {
        advisory_id: advisory.advisory_id.clone(),
        storm_id: advisory.storm_id.clone(),
        advisory_number: advisory.advisory_number,
        issued: advisory.issued,
        source_blake3s,
        frame_id: advisory.frame_id.clone(),
        wind_radii,
    })
}

fn validate_advisory_pin(advisory: &GeoAdvisoryPin) -> Result<(), GeoExposureError> {
    let canonical = canonical_advisory_pin(advisory)?;
    if canonical != *advisory {
        return Err(GeoExposureError::invalid(
            "Geo event exposure advisory pin is not in canonical order",
            [("field", "advisory")],
        ));
    }
    Ok(())
}

fn validate_advisory_archive(
    advisory: &GeoAdvisoryPin,
    archive: &GeoAdvisoryArchive,
) -> Result<(), GeoExposureError> {
    let mut archive_blake3s = BTreeSet::new();
    for source_blake3 in &archive.source_blake3s {
        validate_prefixed_blake3("archive.source_blake3s[]", source_blake3)?;
        archive_blake3s.insert(source_blake3.as_str());
    }
    for archived in &archive.advisories {
        validate_text("archive.advisories[].storm_id", &archived.storm_id)?;
        if archived.advisory_number == 0 {
            return Err(GeoExposureError::invalid_field(
                "archive.advisories[].advisory_number",
                archived.advisory_number.to_string(),
            ));
        }
        for source_blake3 in &archived.source_blake3s {
            validate_prefixed_blake3("archive.advisories[].source_blake3s[]", source_blake3)?;
            archive_blake3s.insert(source_blake3.as_str());
        }
        if archived.storm_id == advisory.storm_id
            && archived.advisory_number > advisory.advisory_number
        {
            return Err(GeoExposureError::advisory_stale(
                "Geo event exposure advisory has been superseded in the archive",
                [
                    ("advisory_id".to_string(), advisory.advisory_id.clone()),
                    ("storm_id".to_string(), advisory.storm_id.clone()),
                    (
                        "current_advisory_number".to_string(),
                        advisory.advisory_number.to_string(),
                    ),
                    (
                        "advisory_number".to_string(),
                        archived.advisory_number.to_string(),
                    ),
                ],
            ));
        }
    }
    for source_blake3 in &advisory.source_blake3s {
        if !archive_blake3s.contains(source_blake3.as_str()) {
            return Err(GeoExposureError::advisory_stale(
                "Geo event exposure advisory source pin is absent from the archive",
                [
                    ("advisory_id", advisory.advisory_id.as_str()),
                    ("storm_id", advisory.storm_id.as_str()),
                    ("source_blake3", source_blake3.as_str()),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_geometry_input(geometry: &GeoExposureGeometryInput) -> Result<(), GeoExposureError> {
    validate_text("frame_id", &geometry.frame_id)?;
    validate_nonempty_map("buildings", &geometry.buildings)?;
    for (building_id, geometry) in &geometry.buildings {
        validate_text("buildings", building_id)?;
        match geometry {
            GeoCanonicalGeometryMm::Polygon { polygon } => {
                validate_polygon("buildings", building_id, polygon)?;
            }
            GeoCanonicalGeometryMm::Point { coordinate } => {
                validate_point("buildings", building_id, *coordinate)?;
            }
            GeoCanonicalGeometryMm::MultiPolygon { polygons } => {
                validate_nonempty("buildings[].polygons", polygons)?;
                for (index, polygon) in polygons.iter().enumerate() {
                    validate_polygon("buildings[].polygons", &index.to_string(), polygon)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_polygon(
    field: &'static str,
    geometry_id: &str,
    polygon: &GeoCanonicalPolygonMm,
) -> Result<(), GeoExposureError> {
    if polygon.exterior.vertices.is_empty() {
        return Err(GeoExposureError::invalid(
            "Geo event exposure polygon exterior must contain vertices",
            [("field", field), ("geometry_id", geometry_id)],
        ));
    }
    for point in &polygon.exterior.vertices {
        validate_point(field, geometry_id, *point)?;
    }
    for hole in &polygon.holes {
        if hole.vertices.is_empty() {
            return Err(GeoExposureError::invalid(
                "Geo event exposure polygon hole must contain vertices when present",
                [("field", field), ("geometry_id", geometry_id)],
            ));
        }
        for point in &hole.vertices {
            validate_point(field, geometry_id, *point)?;
        }
    }
    Ok(())
}

fn predicate_ring_from_polygon(
    frame_id: &str,
    field: &'static str,
    geometry_id: &str,
    polygon: &GeoCanonicalPolygonMm,
) -> Result<GeoLinearRingMm, GeoExposureError> {
    let mut closed = polygon.exterior.vertices.clone();
    if closed.is_empty() {
        return Err(GeoExposureError::invalid(
            "Geo event exposure polygon exterior must contain vertices",
            [("field", field), ("geometry_id", geometry_id)],
        ));
    }
    closed.push(closed[0]);
    GeoLinearRingMm::new(frame_id.to_string(), closed)
        .map_err(|error| predicate_error(field, geometry_id, error))
}

fn validate_exposed(
    exposed: &[GeoExposedBuilding],
    advisory: &GeoAdvisoryPin,
) -> Result<(), GeoExposureError> {
    let bands = advisory
        .wind_radii
        .iter()
        .map(|radius| radius.knots)
        .collect::<BTreeSet<_>>();
    let mut previous: Option<&GeoExposedBuilding> = None;
    let mut seen = BTreeSet::new();
    for building in exposed {
        validate_text("exposed[].accession", &building.accession)?;
        validate_text("exposed[].loan_id", &building.loan_id)?;
        validate_text("exposed[].building_id", &building.building_id)?;
        if !bands.contains(&building.knots_band) {
            return Err(GeoExposureError::invalid(
                "Geo event exposure exposed row references an unknown wind band",
                [
                    ("field".to_string(), "exposed[].knots_band".to_string()),
                    ("building_id".to_string(), building.building_id.clone()),
                    ("knots_band".to_string(), building.knots_band.to_string()),
                ],
            ));
        }
        if building.claim_class != GeoClaimClass::CollateralComposition {
            return Err(GeoExposureError::invalid(
                "Geo event exposure row must carry the ledger collateral-composition claim class",
                [("field", "exposed[].claim_class")],
            ));
        }
        let key = (
            building.accession.as_str(),
            building.loan_id.as_str(),
            building.building_id.as_str(),
        );
        if !seen.insert(key) {
            return Err(GeoExposureError::invalid(
                "Geo event exposure exposed rows must be unique by accession, loan, and building",
                [
                    ("field", "exposed"),
                    ("building_id", building.building_id.as_str()),
                ],
            ));
        }
        if let Some(previous) = previous
            && previous >= building
        {
            return Err(GeoExposureError::invalid(
                "Geo event exposure exposed rows must be strictly sorted and unique",
                [("field", "exposed")],
            ));
        }
        previous = Some(building);
    }
    Ok(())
}

fn validate_unexposed(unexposed: &[GeoUnexposedBuilding]) -> Result<(), GeoExposureError> {
    let mut previous: Option<&GeoUnexposedBuilding> = None;
    let mut seen = BTreeSet::new();
    for building in unexposed {
        validate_text("not_exposed[].accession", &building.accession)?;
        validate_text("not_exposed[].loan_id", &building.loan_id)?;
        validate_text("not_exposed[].building_id", &building.building_id)?;
        if building.claim_class != GeoClaimClass::CollateralComposition {
            return Err(GeoExposureError::invalid(
                "Geo event exposure row must carry the ledger collateral-composition claim class",
                [("field", "not_exposed[].claim_class")],
            ));
        }
        let key = (
            building.accession.as_str(),
            building.loan_id.as_str(),
            building.building_id.as_str(),
        );
        if !seen.insert(key) {
            return Err(GeoExposureError::invalid(
                "Geo event exposure unexposed rows must be unique by accession, loan, and building",
                [
                    ("field", "not_exposed"),
                    ("building_id", building.building_id.as_str()),
                ],
            ));
        }
        if let Some(previous) = previous
            && previous >= building
        {
            return Err(GeoExposureError::invalid(
                "Geo event exposure unexposed rows must be strictly sorted and unique",
                [("field", "not_exposed")],
            ));
        }
        previous = Some(building);
    }
    Ok(())
}

fn validate_disjoint_classifications(artifact: &GeoEventExposure) -> Result<(), GeoExposureError> {
    let mut exposed_keys = BTreeSet::new();
    for building in &artifact.exposed {
        exposed_keys.insert((
            building.accession.as_str(),
            building.loan_id.as_str(),
            building.building_id.as_str(),
        ));
    }
    let missing = artifact
        .buildings_without_geometry
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for building in &artifact.not_exposed {
        let key = (
            building.accession.as_str(),
            building.loan_id.as_str(),
            building.building_id.as_str(),
        );
        if exposed_keys.contains(&key) {
            return Err(GeoExposureError::invalid(
                "Geo event exposure building appears in both exposed and not_exposed",
                [
                    ("field", "not_exposed"),
                    ("building_id", building.building_id.as_str()),
                ],
            ));
        }
    }
    for building in &artifact.exposed {
        if missing.contains(building.building_id.as_str()) {
            return Err(GeoExposureError::invalid(
                "Geo event exposure building appears in both exposed and buildings_without_geometry",
                [
                    ("field", "buildings_without_geometry"),
                    ("building_id", building.building_id.as_str()),
                ],
            ));
        }
    }
    for building in &artifact.not_exposed {
        if missing.contains(building.building_id.as_str()) {
            return Err(GeoExposureError::invalid(
                "Geo event exposure building appears in both not_exposed and buildings_without_geometry",
                [
                    ("field", "buildings_without_geometry"),
                    ("building_id", building.building_id.as_str()),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_interval(
    field: &'static str,
    interval: GeoValidTimeInterval,
) -> Result<(), GeoExposureError> {
    if interval.start_day > interval.end_day {
        return Err(GeoExposureError::invalid(
            "Geo event exposure interval must have start_day <= end_day",
            [
                ("field".to_string(), field.to_string()),
                ("start_day".to_string(), interval.start_day.to_string()),
                ("end_day".to_string(), interval.end_day.to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_point(
    field: &'static str,
    geometry_id: &str,
    point: GeoPointMm,
) -> Result<(), GeoExposureError> {
    point
        .x
        .checked_abs()
        .ok_or_else(|| GeoExposureError::overflow(format!("{field}.{geometry_id}.x")))?;
    point
        .y
        .checked_abs()
        .ok_or_else(|| GeoExposureError::overflow(format!("{field}.{geometry_id}.y")))?;
    Ok(())
}

fn validate_text(field: &str, value: &str) -> Result<(), GeoExposureError> {
    if value.trim().is_empty() {
        return Err(GeoExposureError::invalid_field(field, value));
    }
    Ok(())
}

fn validate_prefixed_blake3(field: &str, value: &str) -> Result<(), GeoExposureError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(GeoExposureError::invalid(
            "Geo event exposure digests must use blake3:<hex>",
            [("field", field), ("value", value)],
        ));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoExposureError::invalid(
            "Geo event exposure digests must use lowercase blake3:<hex>",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_nonempty<T>(field: &'static str, values: &[T]) -> Result<(), GeoExposureError> {
    if values.is_empty() {
        return Err(GeoExposureError::invalid(
            "Geo event exposure vector must be nonempty",
            [("field", field)],
        ));
    }
    Ok(())
}

fn validate_nonempty_map<K, V>(
    field: &'static str,
    values: &BTreeMap<K, V>,
) -> Result<(), GeoExposureError> {
    if values.is_empty() {
        return Err(GeoExposureError::invalid(
            "Geo event exposure map must be nonempty",
            [("field", field)],
        ));
    }
    Ok(())
}

fn validate_sorted_unique(field: &'static str, values: &[String]) -> Result<(), GeoExposureError> {
    for pair in values.windows(2) {
        if pair[0] >= pair[1] {
            return Err(GeoExposureError::invalid(
                "Geo event exposure vectors must be strictly sorted and unique",
                [("field", field), ("value", pair[1].as_str())],
            ));
        }
    }
    Ok(())
}

fn predicate_error(
    field: &'static str,
    geometry_id: &str,
    error: GeoPredicateError,
) -> GeoExposureError {
    let mut detail = BTreeMap::new();
    detail.insert("field".to_string(), field.to_string());
    detail.insert("geometry_id".to_string(), geometry_id.to_string());
    detail.insert("predicate_code".to_string(), format!("{:?}", error.code));
    detail.extend(error.detail);
    GeoExposureError::new(
        GeoExposureErrorCode::InvalidInput,
        "Geo event exposure polygon cannot be converted to a predicate ring",
        detail,
    )
}

fn area_majority_error(error: GeoAreaMajorityError) -> GeoExposureError {
    let mut detail = BTreeMap::new();
    detail.insert("area_code".to_string(), format!("{:?}", error.code));
    detail.extend(error.detail);
    GeoExposureError::new(
        GeoExposureErrorCode::InvalidInput,
        "Geo event exposure exact area predicate failed",
        detail,
    )
}

fn ledger_error(error: GeoLedgerError) -> GeoExposureError {
    let mut detail = BTreeMap::new();
    detail.insert("ledger_code".to_string(), format!("{:?}", error.code));
    detail.extend(error.detail);
    GeoExposureError::new(
        GeoExposureErrorCode::InvalidInput,
        "Geo event exposure received an invalid collateral ledger",
        detail,
    )
}

fn checked_inc_usize(value: usize, field: impl Into<String>) -> Result<usize, GeoExposureError> {
    value
        .checked_add(1)
        .ok_or_else(|| GeoExposureError::overflow(field))
}

fn usize_to_u64(value: usize, field: impl Into<String>) -> Result<u64, GeoExposureError> {
    u64::try_from(value).map_err(|_| GeoExposureError::overflow(field))
}

fn sorted_unique<T: Ord>(values: Vec<T>) -> Vec<T> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn digest_prefixed(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

const fn default_claim_class() -> GeoClaimClass {
    GeoClaimClass::CollateralComposition
}

const fn default_proof_class() -> GeoCollateralLedgerProofClass {
    GeoCollateralLedgerProofClass::Fixture
}
