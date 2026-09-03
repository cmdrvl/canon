#![forbid(unsafe_code)]

//! Calibrated assessment-roll and building-footprint evidence producers.
//!
//! This module is an offline run-stage producer. It emits ordinary
//! `canon_geo_evidence_request.v0` observations and relies on the shared
//! evidence compiler for rho admission and hard-constraint validation.

use super::{
    CANON_GEO_EVIDENCE_REQUEST_VERSION, DEFAULT_MAX_MATERIALIZED_MODELS, GeoCompositionProfile,
    GeoCompositionUniverse, GeoEntityLevel, GeoEvidenceClaimRole, GeoEvidenceCompilationRequest,
    GeoEvidenceError, GeoEvidenceRecordRef, GeoIntegerMeasure, GeoIntegerMemberValue,
    GeoIntegerValueOrigin, GeoRhoBasis, GeoRhoContract, GeoRhoObservation, GeoRhoObservationKind,
    compile_evidence,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_FOOTPRINT_ROLL_EVIDENCE_REQUEST_VERSION: &str =
    "canon_geo_footprint_roll_evidence_request.v0";

pub const GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID: &str =
    "rho.size.assessment_roll_gross_sqft_band";
pub const GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_CONTRACT_ID: &str =
    "rho.footprint.building_count_floor";

pub const GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_OBSERVATION_PREFIX: &str =
    "obs.size.assessment_roll_gross_sqft_band";
pub const GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_OBSERVATION_PREFIX: &str =
    "obs.footprint.building_count_floor";

pub const GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CALIBRATION_BLAKE3: &str =
    "f5c84419b86fa59cb21e8b551a2077f0d69f8a7bf757074f5d0cdbd887719d61";
pub const GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_CALIBRATION_BLAKE3: &str =
    "0663ab66c8614c567c811ede97361fce03551184a92f19f17f56dddbb2624ba3";

pub const GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_POPULATION_ID: &str =
    "h7-d1-residuals-2026-09-03-roll";
pub const GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_POPULATION_ID: &str = "h7-d1-residuals-2026-09-03";

pub const GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_MAX: u64 = u64::MAX;

const ASSESSMENT_ROLL_CONTRACT_SOURCE_DATASET: &str =
    "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION_FY2026P3_x_PROPERTY_PERIOD_FACT";
const ASSESSMENT_ROLL_CONTRACT_SOURCE_RELEASE: &str = "FY2026P3_ppf-latest";
const ASSESSMENT_ROLL_ROW_SOURCE_DATASET: &str =
    "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION";
const ASSESSMENT_ROLL_ROW_SOURCE_VINTAGE: &str = "FY2026P3";

const FOOTPRINT_CONTRACT_SOURCE_DATASET: &str =
    "EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT_x_LOAN_ISSUANCE_PROPERTY";
const FOOTPRINT_CONTRACT_SOURCE_RELEASE: &str = "footprints-latest_lip-current";
const FOOTPRINT_ROW_SOURCE_DATASET: &str = "EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT";
const FOOTPRINT_ROW_SOURCE_VINTAGE: &str = "latest";

const ASSESSMENT_ROLL_LINEAGE: [&str; 2] = [
    "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.WRGL_NYC_OPENDATA_PROPERTY_VALUATION_AND_ASSESSMENT_DATA_TAX_CLASSES_1_2_3_4__STRUCTURED:FY2026P3",
    "EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT:latest_reporting_period",
];
const FOOTPRINT_LINEAGE: [&str; 2] = [
    "EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:current",
    "EDGAR_DB.SOURCE.NYC_BUILDING_FOOTPRINTS_HOT:latest",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoFootprintRollEvidenceRequest {
    pub version: String,
    #[serde(default)]
    pub profile: GeoCompositionProfile,
    pub case_id: String,
    pub universe: GeoCompositionUniverse,
    pub loan: GeoFootprintRollLoanFields,
    #[serde(default)]
    pub source_config: GeoFootprintRollSourceConfig,
    #[serde(default)]
    pub calibration: GeoFootprintRollCalibration,
    #[serde(default)]
    pub assessment_roll_rows: Vec<GeoAssessmentRollGrossSqftRow>,
    #[serde(default)]
    pub footprint_rows: Vec<GeoBuildingFootprintRow>,
    pub max_assignments: u64,
    #[serde(default = "default_max_materialized_models")]
    pub max_materialized_models: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoFootprintRollLoanFields {
    pub loan_key: String,
    pub filed_size: Option<u64>,
    pub size_measure: String,
    pub loan_county_property_count: Option<u64>,
    pub size_source_record_id: String,
    pub size_source_vintage: String,
    pub county_property_count_source_record_id: String,
    pub county_property_count_source_vintage: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoFootprintRollSourceConfig {
    pub assessment_roll_contract_source_dataset: String,
    pub assessment_roll_contract_source_release: String,
    pub assessment_roll_row_source_dataset: String,
    pub assessment_roll_row_source_vintage: String,
    pub footprint_contract_source_dataset: String,
    pub footprint_contract_source_release: String,
    pub footprint_row_source_dataset: String,
    pub footprint_row_source_vintage: String,
}

impl Default for GeoFootprintRollSourceConfig {
    fn default() -> Self {
        Self {
            assessment_roll_contract_source_dataset: ASSESSMENT_ROLL_CONTRACT_SOURCE_DATASET
                .to_string(),
            assessment_roll_contract_source_release: ASSESSMENT_ROLL_CONTRACT_SOURCE_RELEASE
                .to_string(),
            assessment_roll_row_source_dataset: ASSESSMENT_ROLL_ROW_SOURCE_DATASET.to_string(),
            assessment_roll_row_source_vintage: ASSESSMENT_ROLL_ROW_SOURCE_VINTAGE.to_string(),
            footprint_contract_source_dataset: FOOTPRINT_CONTRACT_SOURCE_DATASET.to_string(),
            footprint_contract_source_release: FOOTPRINT_CONTRACT_SOURCE_RELEASE.to_string(),
            footprint_row_source_dataset: FOOTPRINT_ROW_SOURCE_DATASET.to_string(),
            footprint_row_source_vintage: FOOTPRINT_ROW_SOURCE_VINTAGE.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoFootprintRollCalibration {
    pub assessment_roll_gross_sqft_band: GeoAssessmentRollGrossSqftBandCalibration,
    pub footprint_building_count_floor: GeoFootprintBuildingCountFloorCalibration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAssessmentRollGrossSqftBandCalibration {
    pub population_id: String,
    pub calibration_blake3: String,
    pub falsification_rule_id: String,
    pub lower_numerator: u64,
    pub lower_denominator: u64,
    pub upper_numerator: u64,
    pub upper_denominator: u64,
    pub upper_inclusive_padding: u64,
}

impl Default for GeoAssessmentRollGrossSqftBandCalibration {
    fn default() -> Self {
        Self {
            population_id: GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_POPULATION_ID.to_string(),
            calibration_blake3: GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CALIBRATION_BLAKE3.to_string(),
            falsification_rule_id: "truth-gross-sum-outside-band".to_string(),
            lower_numerator: 7,
            lower_denominator: 10,
            upper_numerator: 16,
            upper_denominator: 10,
            upper_inclusive_padding: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoFootprintBuildingCountFloorCalibration {
    pub population_id: String,
    pub calibration_blake3: String,
    pub falsification_rule_id: String,
}

impl Default for GeoFootprintBuildingCountFloorCalibration {
    fn default() -> Self {
        Self {
            population_id: GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_POPULATION_ID.to_string(),
            calibration_blake3: GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_CALIBRATION_BLAKE3.to_string(),
            falsification_rule_id: "truth-buildings-below-floor".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAssessmentRollGrossSqftRow {
    #[serde(rename = "BBL")]
    pub bbl: String,
    #[serde(rename = "GROSS_SQFT")]
    pub gross_sqft: Option<u64>,
    #[serde(rename = "UNITS")]
    pub units: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoBuildingFootprintRow {
    #[serde(rename = "MAPPLUTO_BBL")]
    pub mappluto_bbl: String,
    #[serde(rename = "BIN")]
    pub bin: String,
    #[serde(rename = "ACTIVE")]
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoFootprintRollEvidenceErrorCode {
    UnsupportedVersion,
    InvalidInput,
    ArithmeticOverflow,
    Serialization,
    Evidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoFootprintRollEvidenceError {
    pub code: GeoFootprintRollEvidenceErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoFootprintRollEvidenceError {
    fn new(
        code: GeoFootprintRollEvidenceErrorCode,
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
        Self::new(
            GeoFootprintRollEvidenceErrorCode::InvalidInput,
            message,
            detail,
        )
    }

    fn overflow(context: &str) -> Self {
        Self::new(
            GeoFootprintRollEvidenceErrorCode::ArithmeticOverflow,
            "Geo footprint/roll evidence arithmetic overflowed",
            [("context", context)],
        )
    }

    fn serialization(error: serde_json::Error) -> Self {
        Self::new(
            GeoFootprintRollEvidenceErrorCode::Serialization,
            "Geo footprint/roll evidence serialization failed",
            [("error", error.to_string())],
        )
    }
}

impl fmt::Display for GeoFootprintRollEvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoFootprintRollEvidenceError {}

impl From<GeoEvidenceError> for GeoFootprintRollEvidenceError {
    fn from(error: GeoEvidenceError) -> Self {
        let mut detail = error.detail;
        detail.insert("evidence_code".to_string(), format!("{:?}", error.code));
        Self {
            code: GeoFootprintRollEvidenceErrorCode::Evidence,
            message: error.message,
            detail,
        }
    }
}

pub fn materialize_footprint_roll_evidence(
    request: &GeoFootprintRollEvidenceRequest,
) -> Result<GeoEvidenceCompilationRequest, GeoFootprintRollEvidenceError> {
    let request = canonicalize_footprint_roll_evidence_request(request)?;
    let roll_rows = assessment_rows_by_bbl(&request.assessment_roll_rows)?;
    let footprint_rows = active_footprint_rows_by_bbl(&request.footprint_rows)?;

    let mut contracts = Vec::new();
    let mut observations = Vec::new();

    if let Some(observation) = roll_gross_sqft_band_observation(&request, &roll_rows)? {
        contracts.push(assessment_roll_gross_sqft_band_contract(
            &request.source_config,
            &request.calibration.assessment_roll_gross_sqft_band,
        ));
        observations.push(observation);
    }

    if let Some(observation) =
        footprint_building_count_floor_observation(&request, &footprint_rows)?
    {
        contracts.push(footprint_building_count_floor_contract(
            &request.source_config,
            &request.calibration.footprint_building_count_floor,
        ));
        observations.push(observation);
    }

    contracts.sort_by(|left, right| left.id.cmp(&right.id));
    observations.sort_by(|left, right| left.id.cmp(&right.id));
    for observation in &mut observations {
        observation.source_records.sort();
    }

    let evidence = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: request.profile,
        universe: request.universe,
        contracts,
        observations,
        max_assignments: request.max_assignments,
        max_materialized_models: request.max_materialized_models,
    };

    compile_evidence(&evidence).map_err(GeoFootprintRollEvidenceError::from)?;
    Ok(evidence)
}

pub fn validate_footprint_roll_evidence_request(
    request: &GeoFootprintRollEvidenceRequest,
) -> Result<(), GeoFootprintRollEvidenceError> {
    materialize_footprint_roll_evidence(request).map(|_| ())
}

pub fn canonicalize_footprint_roll_evidence_request(
    request: &GeoFootprintRollEvidenceRequest,
) -> Result<GeoFootprintRollEvidenceRequest, GeoFootprintRollEvidenceError> {
    if request.version != CANON_GEO_FOOTPRINT_ROLL_EVIDENCE_REQUEST_VERSION {
        return Err(GeoFootprintRollEvidenceError::new(
            GeoFootprintRollEvidenceErrorCode::UnsupportedVersion,
            "Unsupported Geo footprint/roll evidence request version",
            [
                ("actual", request.version.as_str()),
                (
                    "expected",
                    CANON_GEO_FOOTPRINT_ROLL_EVIDENCE_REQUEST_VERSION,
                ),
            ],
        ));
    }
    if request.profile.selection_level != GeoEntityLevel::Parcel {
        return Err(GeoFootprintRollEvidenceError::invalid(
            "Geo footprint/roll evidence requires a parcel composition profile",
            [(
                "selection_level",
                format!("{:?}", request.profile.selection_level),
            )],
        ));
    }
    validate_identifier("case_id", &request.case_id)?;
    validate_loan_fields(&request.loan)?;
    validate_source_config(&request.source_config)?;
    validate_calibration(&request.calibration)?;

    let mut canonical = request.clone();
    validate_and_sort_ids("universe.parcels", &mut canonical.universe.parcels)?;
    let parcel_set = canonical
        .universe
        .parcels
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for building in &mut canonical.universe.buildings {
        validate_identifier("universe.buildings[].id", &building.id)?;
        validate_and_sort_ids("universe.buildings[].parcel_ids", &mut building.parcel_ids)?;
        for parcel_id in &building.parcel_ids {
            if !parcel_set.contains(parcel_id.as_str()) {
                return Err(GeoFootprintRollEvidenceError::invalid(
                    "Geo footprint/roll building incidence references an unknown parcel",
                    [
                        ("building_id", building.id.as_str()),
                        ("parcel_id", parcel_id.as_str()),
                    ],
                ));
            }
        }
    }
    canonical
        .universe
        .buildings
        .sort_by(|left, right| left.id.cmp(&right.id));
    let mut previous_building = None;
    for building in &canonical.universe.buildings {
        if previous_building == Some(building.id.as_str()) {
            return Err(GeoFootprintRollEvidenceError::invalid(
                "Geo footprint/roll universe repeats a building id",
                [("building_id", building.id.as_str())],
            ));
        }
        previous_building = Some(building.id.as_str());
    }
    canonical
        .assessment_roll_rows
        .sort_by(|left, right| left.bbl.cmp(&right.bbl));
    canonical.footprint_rows.sort_by(|left, right| {
        left.mappluto_bbl
            .cmp(&right.mappluto_bbl)
            .then_with(|| left.bin.cmp(&right.bin))
            .then_with(|| left.active.cmp(&right.active))
    });
    assessment_rows_by_bbl(&canonical.assessment_roll_rows)?;
    active_footprint_rows_by_bbl(&canonical.footprint_rows)?;
    Ok(canonical)
}

pub fn canonical_footprint_roll_evidence_request_bytes(
    request: &GeoFootprintRollEvidenceRequest,
) -> Result<Vec<u8>, GeoFootprintRollEvidenceError> {
    let canonical = canonicalize_footprint_roll_evidence_request(request)?;
    serde_json::to_vec(&canonical).map_err(GeoFootprintRollEvidenceError::serialization)
}

pub fn calibration_receipt_blake3(value: &Value) -> Result<String, GeoFootprintRollEvidenceError> {
    let bytes = canonical_json_value_bytes(value)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn roll_gross_sqft_band_observation(
    request: &GeoFootprintRollEvidenceRequest,
    rows_by_bbl: &BTreeMap<String, &GeoAssessmentRollGrossSqftRow>,
) -> Result<Option<GeoRhoObservation>, GeoFootprintRollEvidenceError> {
    let Some(filed_size) = request.loan.filed_size else {
        return Ok(None);
    };
    if request.loan.size_measure != "SQFT" || filed_size < 500 {
        return Ok(None);
    }

    let mut values = Vec::with_capacity(request.universe.parcels.len());
    let mut source_records = vec![loan_size_source_record(&request.loan)?];
    for parcel_id in &request.universe.parcels {
        let Some(row) = rows_by_bbl.get(parcel_id.as_str()) else {
            return Ok(None);
        };
        let Some(gross_sqft) = row.gross_sqft else {
            return Ok(None);
        };
        values.push(GeoIntegerMemberValue {
            id: parcel_id.clone(),
            value: gross_sqft,
        });
        source_records.push(assessment_roll_source_record(&request.source_config, row)?);
    }

    values.sort();
    let calibration = &request.calibration.assessment_roll_gross_sqft_band;
    let min = floor_ratio(
        filed_size,
        calibration.lower_numerator,
        calibration.lower_denominator,
        "assessment roll gross sqft lower band",
    )?;
    let max = floor_ratio(
        filed_size,
        calibration.upper_numerator,
        calibration.upper_denominator,
        "assessment roll gross sqft upper band",
    )?
    .checked_add(calibration.upper_inclusive_padding)
    .ok_or_else(|| GeoFootprintRollEvidenceError::overflow("assessment roll upper padding"))?;

    Ok(Some(GeoRhoObservation {
        id: format!(
            "{GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_OBSERVATION_PREFIX}:{}",
            request.case_id
        ),
        contract_id: GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID.to_string(),
        source_records,
        valid_time: None,
        observation: GeoRhoObservationKind::IntegerSumBand {
            level: GeoEntityLevel::Parcel,
            measure: GeoIntegerMeasure {
                semantic_id: "assessment_roll.gross_sqft".to_string(),
                unit: "sqft".to_string(),
                value_origin: GeoIntegerValueOrigin::SourceAsserted,
            },
            values,
            min,
            max,
        },
    }))
}

fn footprint_building_count_floor_observation(
    request: &GeoFootprintRollEvidenceRequest,
    rows_by_bbl: &BTreeMap<String, Vec<&GeoBuildingFootprintRow>>,
) -> Result<Option<GeoRhoObservation>, GeoFootprintRollEvidenceError> {
    let Some(count) = request.loan.loan_county_property_count else {
        return Ok(None);
    };
    let Some(min) = count.checked_sub(1).filter(|floor| *floor > 0) else {
        return Ok(None);
    };

    let mut values = Vec::with_capacity(request.universe.parcels.len());
    let mut source_records = vec![loan_county_property_count_source_record(&request.loan)?];
    for parcel_id in &request.universe.parcels {
        let Some(rows) = rows_by_bbl
            .get(parcel_id.as_str())
            .filter(|rows| !rows.is_empty())
        else {
            return Ok(None);
        };
        values.push(GeoIntegerMemberValue {
            id: parcel_id.clone(),
            value: rows.len() as u64,
        });
        for row in rows {
            source_records.push(footprint_source_record(&request.source_config, row)?);
        }
    }

    values.sort();
    Ok(Some(GeoRhoObservation {
        id: format!(
            "{GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_OBSERVATION_PREFIX}:{}",
            request.case_id
        ),
        contract_id: GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_CONTRACT_ID.to_string(),
        source_records,
        valid_time: None,
        observation: GeoRhoObservationKind::IntegerSumBand {
            level: GeoEntityLevel::Parcel,
            measure: GeoIntegerMeasure {
                semantic_id: "footprints.active_bin_count".to_string(),
                unit: "buildings".to_string(),
                value_origin: GeoIntegerValueOrigin::SourceAsserted,
            },
            values,
            min,
            max: GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_MAX,
        },
    }))
}

fn assessment_roll_gross_sqft_band_contract(
    source_config: &GeoFootprintRollSourceConfig,
    calibration: &GeoAssessmentRollGrossSqftBandCalibration,
) -> GeoRhoContract {
    GeoRhoContract {
        id: GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID.to_string(),
        version: "1.0.0".to_string(),
        source_dataset: source_config
            .assessment_roll_contract_source_dataset
            .clone(),
        source_release: source_config
            .assessment_roll_contract_source_release
            .clone(),
        source_lineage_ids: ASSESSMENT_ROLL_LINEAGE
            .iter()
            .map(|id| (*id).to_string())
            .collect(),
        method_id: "asserted-sqft-roll-gross-sum-band".to_string(),
        method_version: format!(
            "1.0.0_band_{}_over_{}_to_{}_over_{}_upper_plus_{}",
            calibration.lower_numerator,
            calibration.lower_denominator,
            calibration.upper_numerator,
            calibration.upper_denominator,
            calibration.upper_inclusive_padding
        ),
        claim_role: GeoEvidenceClaimRole::AttributeObservation,
        basis: GeoRhoBasis::EmpiricalCalibration {
            population_id: calibration.population_id.clone(),
            calibration_blake3: calibration.calibration_blake3.clone(),
            falsification_rule_id: calibration.falsification_rule_id.clone(),
            admissible_hard_band: true,
        },
    }
}

fn footprint_building_count_floor_contract(
    source_config: &GeoFootprintRollSourceConfig,
    calibration: &GeoFootprintBuildingCountFloorCalibration,
) -> GeoRhoContract {
    GeoRhoContract {
        id: GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_CONTRACT_ID.to_string(),
        version: "1.0.0".to_string(),
        source_dataset: source_config.footprint_contract_source_dataset.clone(),
        source_release: source_config.footprint_contract_source_release.clone(),
        source_lineage_ids: FOOTPRINT_LINEAGE
            .iter()
            .map(|id| (*id).to_string())
            .collect(),
        method_id: "active-bin-count-floor".to_string(),
        method_version: "1.0.0_count_minus_1_unbounded_max".to_string(),
        claim_role: GeoEvidenceClaimRole::AttributeObservation,
        basis: GeoRhoBasis::EmpiricalCalibration {
            population_id: calibration.population_id.clone(),
            calibration_blake3: calibration.calibration_blake3.clone(),
            falsification_rule_id: calibration.falsification_rule_id.clone(),
            admissible_hard_band: true,
        },
    }
}

fn assessment_rows_by_bbl(
    rows: &[GeoAssessmentRollGrossSqftRow],
) -> Result<BTreeMap<String, &GeoAssessmentRollGrossSqftRow>, GeoFootprintRollEvidenceError> {
    let mut by_bbl = BTreeMap::new();
    for row in rows {
        validate_identifier("assessment_roll_rows[].BBL", &row.bbl)?;
        if by_bbl.insert(row.bbl.clone(), row).is_some() {
            return Err(GeoFootprintRollEvidenceError::invalid(
                "Geo assessment-roll rows repeat a BBL",
                [("BBL", row.bbl.as_str())],
            ));
        }
    }
    Ok(by_bbl)
}

fn active_footprint_rows_by_bbl(
    rows: &[GeoBuildingFootprintRow],
) -> Result<BTreeMap<String, Vec<&GeoBuildingFootprintRow>>, GeoFootprintRollEvidenceError> {
    let mut seen_bins = BTreeSet::new();
    let mut by_bbl: BTreeMap<String, Vec<&GeoBuildingFootprintRow>> = BTreeMap::new();
    for row in rows {
        validate_identifier("footprint_rows[].MAPPLUTO_BBL", &row.mappluto_bbl)?;
        validate_identifier("footprint_rows[].BIN", &row.bin)?;
        let key = (row.mappluto_bbl.clone(), row.bin.clone());
        if !seen_bins.insert(key) {
            return Err(GeoFootprintRollEvidenceError::invalid(
                "Geo building-footprint rows repeat a MAPPLUTO_BBL/BIN pair",
                [
                    ("MAPPLUTO_BBL", row.mappluto_bbl.as_str()),
                    ("BIN", row.bin.as_str()),
                ],
            ));
        }
        if row.active {
            by_bbl
                .entry(row.mappluto_bbl.clone())
                .or_default()
                .push(row);
        }
    }
    Ok(by_bbl)
}

fn loan_size_source_record(
    loan: &GeoFootprintRollLoanFields,
) -> Result<GeoEvidenceRecordRef, GeoFootprintRollEvidenceError> {
    let Some(value) = loan.filed_size else {
        return Err(GeoFootprintRollEvidenceError::invalid(
            "Geo roll sqft source record requires filed_size",
            [("loan_key", loan.loan_key.as_str())],
        ));
    };
    source_record(
        &loan.size_source_record_id,
        &loan.size_source_vintage,
        &LoanSizeSourcePayload {
            loan_key: &loan.loan_key,
            field: "SIZE",
            value,
            size_measure: &loan.size_measure,
        },
    )
}

fn loan_county_property_count_source_record(
    loan: &GeoFootprintRollLoanFields,
) -> Result<GeoEvidenceRecordRef, GeoFootprintRollEvidenceError> {
    let Some(value) = loan.loan_county_property_count else {
        return Err(GeoFootprintRollEvidenceError::invalid(
            "Geo footprint source record requires loan_county_property_count",
            [("loan_key", loan.loan_key.as_str())],
        ));
    };
    source_record(
        &loan.county_property_count_source_record_id,
        &loan.county_property_count_source_vintage,
        &LoanCountSourcePayload {
            loan_key: &loan.loan_key,
            field: "LOAN_COUNTY_PROPERTY_COUNT",
            value,
        },
    )
}

fn assessment_roll_source_record(
    source_config: &GeoFootprintRollSourceConfig,
    row: &GeoAssessmentRollGrossSqftRow,
) -> Result<GeoEvidenceRecordRef, GeoFootprintRollEvidenceError> {
    source_record(
        &format!(
            "{}:{}:gsf:{}",
            source_config.assessment_roll_row_source_dataset,
            source_config.assessment_roll_row_source_vintage,
            row.bbl
        ),
        &source_config.assessment_roll_row_source_vintage,
        row,
    )
}

fn footprint_source_record(
    source_config: &GeoFootprintRollSourceConfig,
    row: &GeoBuildingFootprintRow,
) -> Result<GeoEvidenceRecordRef, GeoFootprintRollEvidenceError> {
    source_record(
        &format!(
            "{}:{}:{}:{}",
            source_config.footprint_row_source_dataset,
            source_config.footprint_row_source_vintage,
            row.mappluto_bbl,
            row.bin
        ),
        &source_config.footprint_row_source_vintage,
        row,
    )
}

fn source_record<T: Serialize>(
    source_record_id: &str,
    source_vintage: &str,
    payload: &T,
) -> Result<GeoEvidenceRecordRef, GeoFootprintRollEvidenceError> {
    validate_identifier("source_record_id", source_record_id)?;
    validate_identifier("source_vintage", source_vintage)?;
    let value =
        serde_json::to_value(payload).map_err(GeoFootprintRollEvidenceError::serialization)?;
    let bytes = canonical_json_value_bytes(&value)?;
    Ok(GeoEvidenceRecordRef {
        source_record_id: source_record_id.to_string(),
        source_vintage: source_vintage.to_string(),
        record_blake3: blake3::hash(&bytes).to_hex().to_string(),
    })
}

#[derive(Serialize)]
struct LoanSizeSourcePayload<'a> {
    loan_key: &'a str,
    field: &'static str,
    value: u64,
    size_measure: &'a str,
}

#[derive(Serialize)]
struct LoanCountSourcePayload<'a> {
    loan_key: &'a str,
    field: &'static str,
    value: u64,
}

fn validate_loan_fields(
    loan: &GeoFootprintRollLoanFields,
) -> Result<(), GeoFootprintRollEvidenceError> {
    validate_identifier("loan.loan_key", &loan.loan_key)?;
    validate_identifier("loan.size_measure", &loan.size_measure)?;
    validate_identifier("loan.size_source_record_id", &loan.size_source_record_id)?;
    validate_identifier("loan.size_source_vintage", &loan.size_source_vintage)?;
    validate_identifier(
        "loan.county_property_count_source_record_id",
        &loan.county_property_count_source_record_id,
    )?;
    validate_identifier(
        "loan.county_property_count_source_vintage",
        &loan.county_property_count_source_vintage,
    )
}

fn validate_source_config(
    source_config: &GeoFootprintRollSourceConfig,
) -> Result<(), GeoFootprintRollEvidenceError> {
    validate_identifier(
        "source_config.assessment_roll_contract_source_dataset",
        &source_config.assessment_roll_contract_source_dataset,
    )?;
    validate_identifier(
        "source_config.assessment_roll_contract_source_release",
        &source_config.assessment_roll_contract_source_release,
    )?;
    validate_identifier(
        "source_config.assessment_roll_row_source_dataset",
        &source_config.assessment_roll_row_source_dataset,
    )?;
    validate_identifier(
        "source_config.assessment_roll_row_source_vintage",
        &source_config.assessment_roll_row_source_vintage,
    )?;
    validate_identifier(
        "source_config.footprint_contract_source_dataset",
        &source_config.footprint_contract_source_dataset,
    )?;
    validate_identifier(
        "source_config.footprint_contract_source_release",
        &source_config.footprint_contract_source_release,
    )?;
    validate_identifier(
        "source_config.footprint_row_source_dataset",
        &source_config.footprint_row_source_dataset,
    )?;
    validate_identifier(
        "source_config.footprint_row_source_vintage",
        &source_config.footprint_row_source_vintage,
    )
}

fn validate_calibration(
    calibration: &GeoFootprintRollCalibration,
) -> Result<(), GeoFootprintRollEvidenceError> {
    let roll = &calibration.assessment_roll_gross_sqft_band;
    validate_identifier("calibration.roll.population_id", &roll.population_id)?;
    validate_blake3(
        "calibration.roll.calibration_blake3",
        &roll.calibration_blake3,
    )?;
    validate_identifier(
        "calibration.roll.falsification_rule_id",
        &roll.falsification_rule_id,
    )?;
    if roll.lower_denominator == 0 || roll.upper_denominator == 0 {
        return Err(GeoFootprintRollEvidenceError::invalid(
            "Geo roll sqft calibration denominators must be positive",
            [("field", "calibration.assessment_roll_gross_sqft_band")],
        ));
    }
    if u128::from(roll.lower_numerator) * u128::from(roll.upper_denominator)
        > u128::from(roll.upper_numerator) * u128::from(roll.lower_denominator)
    {
        return Err(GeoFootprintRollEvidenceError::invalid(
            "Geo roll sqft calibration lower band exceeds upper band",
            [("field", "calibration.assessment_roll_gross_sqft_band")],
        ));
    }

    let footprint = &calibration.footprint_building_count_floor;
    validate_identifier(
        "calibration.footprint.population_id",
        &footprint.population_id,
    )?;
    validate_blake3(
        "calibration.footprint.calibration_blake3",
        &footprint.calibration_blake3,
    )?;
    validate_identifier(
        "calibration.footprint.falsification_rule_id",
        &footprint.falsification_rule_id,
    )
}

fn validate_and_sort_ids(
    field: &str,
    values: &mut [String],
) -> Result<(), GeoFootprintRollEvidenceError> {
    for value in values.iter() {
        validate_identifier(field, value)?;
    }
    values.sort();
    let mut previous = None;
    for value in values.iter() {
        if previous == Some(value.as_str()) {
            return Err(GeoFootprintRollEvidenceError::invalid(
                "Geo footprint/roll evidence input contains a duplicate id",
                [("field", field), ("value", value.as_str())],
            ));
        }
        previous = Some(value.as_str());
    }
    Ok(())
}

fn validate_identifier(field: &str, value: &str) -> Result<(), GeoFootprintRollEvidenceError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoFootprintRollEvidenceError::invalid(
            "Geo footprint/roll evidence identifiers must be non-empty and already canonical",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_blake3(field: &str, value: &str) -> Result<(), GeoFootprintRollEvidenceError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoFootprintRollEvidenceError::invalid(
            "Geo footprint/roll evidence BLAKE3 digests must be 64 lowercase hex characters",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn floor_ratio(
    value: u64,
    numerator: u64,
    denominator: u64,
    context: &'static str,
) -> Result<u64, GeoFootprintRollEvidenceError> {
    if denominator == 0 {
        return Err(GeoFootprintRollEvidenceError::invalid(
            "Geo footprint/roll ratio denominator must be positive",
            [("context", context)],
        ));
    }
    value
        .checked_mul(numerator)
        .ok_or_else(|| GeoFootprintRollEvidenceError::overflow(context))
        .map(|product| product / denominator)
}

fn canonical_json_value_bytes(value: &Value) -> Result<Vec<u8>, GeoFootprintRollEvidenceError> {
    canonical_json_value_string(value)
        .map(String::into_bytes)
        .map_err(GeoFootprintRollEvidenceError::serialization)
}

fn canonical_json_value_string(value: &Value) -> Result<String, serde_json::Error> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            serde_json::to_string(value)
        }
        Value::Array(values) => {
            let mut out = String::from("[");
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&canonical_json_value_string(value)?);
            }
            out.push(']');
            Ok(out)
        }
        Value::Object(values) => {
            let mut out = String::from("{");
            let mut entries = values.iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(right.0));
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key)?);
                out.push(':');
                out.push_str(&canonical_json_value_string(value)?);
            }
            out.push('}');
            Ok(out)
        }
    }
}

fn default_max_materialized_models() -> u64 {
    DEFAULT_MAX_MATERIALIZED_MODELS
}
