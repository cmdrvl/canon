#![forbid(unsafe_code)]

//! E5 source-instance profiles for non-NYC inventory fuel.
//!
//! These helpers stop at the adapter/profile boundary: they construct ordinary
//! `canon_geo_regional_inventory.v1` source instances from pinned local
//! artifacts. Core evidence, composition, and solve code still dispatch only on
//! typed entity levels and evidence classes.

use super::{
    CANON_GEO_REGIONAL_INVENTORY_VERSION, CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION,
    CANON_GEO_WAREHOUSE_ROWS_VERSION, GeoAsOf, GeoBoundedGeography, GeoControlEntityLevel,
    GeoCoveragePredicate, GeoEgressClass, GeoEvidenceClass, GeoGeometryTransformContract,
    GeoIdentityParticipation, GeoLicenseClass, GeoLocalAcquisitionState, GeoLocalArtifactRef,
    GeoNativeEntityScope, GeoNumericMeasure, GeoRegionalInventory, GeoRegionalSourceInstance,
    GeoSourceAvailability, GeoSourceRelease, GeoTemporalScope, GeoValueOrigin,
};
use serde::{Deserialize, Serialize};

pub const E5_FRANKLIN_COUNTY_FIPS: &str = "39049";
pub const E5_FRANKLIN_COUNTY_REGION_ID: &str = "us.oh.franklin_county";
pub const E5_FRANKLIN_AUDITOR_PARCELS_SOURCE_INSTANCE_ID: &str =
    "source.franklin_county_oh.auditor_parcels";
pub const E5_FRANKLIN_AUDITOR_PARCELS_RELEASE_ID: &str = "hub-de09f99cce0bcae7142d6d2e26582fd3-25";
pub const E5_FRANKLIN_AUDITOR_PARCELS_RELEASE_UTC_DAY: &str = "2026-09-01";
pub const E5_FRANKLIN_AUDITOR_PARCELS_ADMITTED_ROWS: u64 = 494_043;
pub const E5_FRANKLIN_AUDITOR_PARCELS_H3_COVERAGE_ROWS: u64 = 550_081;
pub const E5_FRANKLIN_CURRENT_BRIDGE_BUILD_ID: &str = "80d0ea39-a5aa-4c27-a8d7-f662a4507257";
pub const E5_FRANKLIN_CURRENT_SUBJECT_PROPERTIES: u64 = 151;
pub const E5_FRANKLIN_CURRENT_SUBJECT_LOANS: u64 = 202;
pub const E5_FRANKLIN_CURRENT_CENTER_CELLS: u64 = 114;
pub const E5_FRANKLIN_CURRENT_WORK_CELLS: u64 = 585;
pub const E5_FRANKLIN_PARCEL_PIP_REACHED_PROPERTIES: u64 = 147;
pub const E5_FRANKLIN_PARCEL_PIP_UNREACHED_PROPERTIES: u64 = 4;
pub const E5_FRANKLIN_PARCEL_PIP_UNIQUE_PROPERTIES: u64 = 146;
pub const E5_FRANKLIN_PARCEL_PIP_MULTI_PROPERTIES: u64 = 1;
pub const E5_FRANKLIN_PARCEL_BLOCKED_CANDIDATE_PAIRS: u64 = 54_344;

pub const E5_MICROSOFT_GLOBALML_FOOTPRINTS_SOURCE_INSTANCE_ID: &str =
    "source.microsoft_globalml.building_footprints";
pub const E5_MICROSOFT_GLOBALML_FOOTPRINTS_RELEASE_ID: &str = "2026-07-24";
pub const E5_MICROSOFT_GLOBALML_FOOTPRINTS_ROWS: u64 = 141_818_047;
pub const E5_MICROSOFT_GLOBALML_FRANKLIN_THIN_TIER_FEATURES: u64 = 168_778;
pub const E5_MICROSOFT_GLOBALML_FRANKLIN_OCCUPIED_WORK_CELLS: u64 = 581;
pub const E5_MICROSOFT_GLOBALML_FRANKLIN_HOT_GEOMETRY_MISSES: u64 = 0;

const APPLICATION_JSON: &str = "application/json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoE5FuelSourcePin {
    pub release_digest: String,
    pub local_artifact_id: String,
    pub local_content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry_transform_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lineage_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoE5FuelCurveReport {
    pub report_id: String,
    pub region_id: String,
    pub bridge_build_id: String,
    pub subject_properties: u64,
    pub subject_loans: u64,
    pub tiers: Vec<GeoE5FuelTierRow>,
    pub frozen_nyc_e4_delta: GeoE5FuelFrozenDelta,
    pub required_core_dispatch_edits: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoE5FuelTierRow {
    pub tier_id: String,
    pub measurement_id: String,
    pub evidence_class: GeoEvidenceClass,
    pub reach: GeoE5FuelReachRow,
    pub precision: GeoE5FuelPrecisionRow,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoE5FuelReachRow {
    pub kind: GeoE5FuelReachKind,
    pub denominator_name: String,
    pub denominator_value: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reached_properties: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unreached_properties: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occupied_work_cells: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoE5FuelReachKind {
    CandidateReach,
    SourceCoverage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoE5FuelPrecisionRow {
    pub status: GeoE5FuelPrecisionStatus,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoE5FuelPrecisionStatus {
    WaitingForTruthPlane,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoE5FuelFrozenDelta {
    pub resolved: i64,
    pub correct: i64,
    pub reach: i64,
    pub false_merges: i64,
}

pub fn franklin_county_region() -> GeoBoundedGeography {
    GeoBoundedGeography {
        geography_id: E5_FRANKLIN_COUNTY_REGION_ID.to_string(),
        geography_kind: "county_fips".to_string(),
        description: "Franklin County, Ohio".to_string(),
    }
}

pub fn e5_franklin_county_inventory(
    inventory_id: impl Into<String>,
    parcel_pin: GeoE5FuelSourcePin,
    microsoft_footprint_pin: GeoE5FuelSourcePin,
) -> GeoRegionalInventory {
    let region = franklin_county_region();
    GeoRegionalInventory {
        version: CANON_GEO_REGIONAL_INVENTORY_VERSION.to_string(),
        inventory_id: inventory_id.into(),
        region: region.clone(),
        sources: vec![
            franklin_auditor_parcel_source(parcel_pin, region.clone()),
            microsoft_globalml_building_footprint_source(microsoft_footprint_pin, region),
        ],
        discovery_gaps: Vec::new(),
    }
}

pub fn e5_franklin_current_fuel_curve_report() -> GeoE5FuelCurveReport {
    GeoE5FuelCurveReport {
        report_id: "e5.franklin_county.current_fuel_curve".to_string(),
        region_id: E5_FRANKLIN_COUNTY_REGION_ID.to_string(),
        bridge_build_id: E5_FRANKLIN_CURRENT_BRIDGE_BUILD_ID.to_string(),
        subject_properties: E5_FRANKLIN_CURRENT_SUBJECT_PROPERTIES,
        subject_loans: E5_FRANKLIN_CURRENT_SUBJECT_LOANS,
        tiers: vec![
            GeoE5FuelTierRow {
                tier_id: "franklin_parcel_candidate_reach".to_string(),
                measurement_id: "e5_franklin_county_parcel_candidate_reach_v0".to_string(),
                evidence_class: GeoEvidenceClass::ParcelGeometry,
                reach: GeoE5FuelReachRow {
                    kind: GeoE5FuelReachKind::CandidateReach,
                    denominator_name: "eligible_subject_properties".to_string(),
                    denominator_value: E5_FRANKLIN_CURRENT_SUBJECT_PROPERTIES,
                    reached_properties: Some(E5_FRANKLIN_PARCEL_PIP_REACHED_PROPERTIES),
                    unreached_properties: Some(E5_FRANKLIN_PARCEL_PIP_UNREACHED_PROPERTIES),
                    occupied_work_cells: None,
                },
                precision: waiting_for_deed_truth_precision(),
                limitations: vec![
                    "candidate reach uses the source parcel universe only".to_string(),
                    "Snowflake GEOGRAPHY PIP is empirical reference, not Canon exact-local truth"
                        .to_string(),
                ],
            },
            GeoE5FuelTierRow {
                tier_id: "microsoft_globalml_footprint_h3_coverage".to_string(),
                measurement_id: "e5_microsoft_globalml_franklin_h3_coverage_v0".to_string(),
                evidence_class: GeoEvidenceClass::BuildingFootprint,
                reach: GeoE5FuelReachRow {
                    kind: GeoE5FuelReachKind::SourceCoverage,
                    denominator_name: "distinct_coverage_features".to_string(),
                    denominator_value: E5_MICROSOFT_GLOBALML_FRANKLIN_THIN_TIER_FEATURES,
                    reached_properties: None,
                    unreached_properties: None,
                    occupied_work_cells: Some(E5_MICROSOFT_GLOBALML_FRANKLIN_OCCUPIED_WORK_CELLS),
                },
                precision: waiting_for_deed_truth_precision(),
                limitations: vec![
                    "H3 coverage is blocking/source availability, not incidence or precision"
                        .to_string(),
                    "footprints are evidence-only until admitted through rho and scored"
                        .to_string(),
                ],
            },
        ],
        frozen_nyc_e4_delta: GeoE5FuelFrozenDelta {
            resolved: 0,
            correct: 0,
            reach: 0,
            false_merges: 0,
        },
        required_core_dispatch_edits: Vec::new(),
    }
}

pub fn franklin_auditor_parcel_source(
    pin: GeoE5FuelSourcePin,
    region: GeoBoundedGeography,
) -> GeoRegionalSourceInstance {
    GeoRegionalSourceInstance {
        source_instance_id: E5_FRANKLIN_AUDITOR_PARCELS_SOURCE_INSTANCE_ID.to_string(),
        release: GeoSourceRelease {
            release_id: E5_FRANKLIN_AUDITOR_PARCELS_RELEASE_ID.to_string(),
            release_digest: pin.release_digest,
        },
        temporal_scope: release_temporal_scope(
            E5_FRANKLIN_AUDITOR_PARCELS_RELEASE_UTC_DAY,
            "franklin_auditor_parcels.release_dt",
        ),
        lineage_ids: default_if_empty(
            pin.lineage_ids,
            &[
                "EDGAR_DB.SOURCE.FRANKLIN_COUNTY_AUDITOR_PARCELS_HOT",
                "EDGAR_DB.SOURCE.FRANKLIN_COUNTY_AUDITOR_PARCELS_FEATURE_H3_COVERAGE",
                "EDGAR_DB.SOURCE.FRANKLIN_COUNTY_AUDITOR_PARCELS_GEOMETRY_EVIDENCE_EXT",
                "EDGAR_DB.SOURCE.FRANKLIN_COUNTY_AUDITOR_PARCELS_TRANSFORM_RECEIPT_EXT",
            ],
        ),
        native_scope: GeoNativeEntityScope::NativeEntity {
            entity_level: GeoControlEntityLevel::Parcel,
            identity_participation: GeoIdentityParticipation::EvidenceOnly,
        },
        evidence_classes: vec![GeoEvidenceClass::ParcelGeometry],
        coverage: GeoCoveragePredicate {
            coverage_id: "coverage.franklin_county_oh.auditor_parcels".to_string(),
            region,
            predicate: "county_fips=39049 and h3_resolution=8 feature coverage bridge".to_string(),
        },
        local_state: available_local_artifact(pin.local_artifact_id, pin.local_content_hash),
        geometry: pin.geometry_transform_digest.map(|digest| {
            geometry_transform(
                "franklin-auditor-parcels-epsg3735-to-wgs84-v0",
                "EPSG:4326",
                digest,
            )
        }),
        license_class: GeoLicenseClass::PublicAttributionRequired,
        egress_class: GeoEgressClass::DerivedOnly,
        estimates: vec![
            source_release_measure(
                "source.admitted_parcels",
                E5_FRANKLIN_AUDITOR_PARCELS_ADMITTED_ROWS,
                "row",
            ),
            source_release_measure(
                "source.h3_r8_coverage_rows",
                E5_FRANKLIN_AUDITOR_PARCELS_H3_COVERAGE_ROWS,
                "row",
            ),
        ],
    }
}

pub fn microsoft_globalml_building_footprint_source(
    pin: GeoE5FuelSourcePin,
    region: GeoBoundedGeography,
) -> GeoRegionalSourceInstance {
    GeoRegionalSourceInstance {
        source_instance_id: E5_MICROSOFT_GLOBALML_FOOTPRINTS_SOURCE_INSTANCE_ID.to_string(),
        release: GeoSourceRelease {
            release_id: E5_MICROSOFT_GLOBALML_FOOTPRINTS_RELEASE_ID.to_string(),
            release_digest: pin.release_digest,
        },
        temporal_scope: release_temporal_scope(
            E5_MICROSOFT_GLOBALML_FOOTPRINTS_RELEASE_ID,
            "microsoft_globalml_footprints.release_dt",
        ),
        lineage_ids: default_if_empty(
            pin.lineage_ids,
            &[
                "EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_HOT",
                "EDGAR_DB.SOURCE.MICROSOFT_GLOBALML_BUILDING_FOOTPRINTS_H3_COVERAGE_HOT",
            ],
        ),
        native_scope: GeoNativeEntityScope::NativeEntity {
            entity_level: GeoControlEntityLevel::Building,
            identity_participation: GeoIdentityParticipation::EvidenceOnly,
        },
        evidence_classes: vec![GeoEvidenceClass::BuildingFootprint],
        coverage: GeoCoveragePredicate {
            coverage_id: "coverage.microsoft_globalml.building_footprints".to_string(),
            region,
            predicate: "bounded local artifact cut from global H3 feature coverage".to_string(),
        },
        local_state: available_local_artifact(pin.local_artifact_id, pin.local_content_hash),
        geometry: pin.geometry_transform_digest.map(|digest| {
            geometry_transform("microsoft-globalml-wgs84-identity-v0", "EPSG:4326", digest)
        }),
        license_class: GeoLicenseClass::PublicAttributionRequired,
        egress_class: GeoEgressClass::DerivedOnly,
        estimates: vec![
            source_release_measure(
                "source.global_footprint_rows",
                E5_MICROSOFT_GLOBALML_FOOTPRINTS_ROWS,
                "row",
            ),
            source_release_measure(
                "source.franklin_thin_tier_features",
                E5_MICROSOFT_GLOBALML_FRANKLIN_THIN_TIER_FEATURES,
                "feature",
            ),
        ],
    }
}

fn release_temporal_scope(utc_day: &str, semantic_id: &str) -> GeoTemporalScope {
    GeoTemporalScope {
        valid_time: None,
        transaction_time: None,
        release_time: Some(GeoAsOf {
            utc_day: utc_day.to_string(),
            semantic_id: semantic_id.to_string(),
            unit: "utc_day".to_string(),
            origin: GeoValueOrigin::SourceRelease,
        }),
    }
}

fn available_local_artifact(artifact_id: String, content_hash: String) -> GeoLocalAcquisitionState {
    GeoLocalAcquisitionState {
        state: GeoSourceAvailability::Available,
        local_ref: Some(GeoLocalArtifactRef {
            artifact_id,
            contract_version: CANON_GEO_WAREHOUSE_ROWS_VERSION.to_string(),
            content_hash,
            media_type: APPLICATION_JSON.to_string(),
        }),
    }
}

fn geometry_transform(
    transform_id: &str,
    coordinate_reference_system: &str,
    transform_digest: String,
) -> GeoGeometryTransformContract {
    GeoGeometryTransformContract {
        geometry_contract_version: CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION.to_string(),
        coordinate_reference_system: coordinate_reference_system.to_string(),
        transform_id: transform_id.to_string(),
        transform_digest,
        numeric_error_bounds: Vec::new(),
    }
}

fn source_release_measure(semantic_id: &str, value: u64, unit: &str) -> GeoNumericMeasure {
    GeoNumericMeasure {
        semantic_id: semantic_id.to_string(),
        value,
        unit: unit.to_string(),
        origin: GeoValueOrigin::SourceRelease,
    }
}

fn default_if_empty(values: Vec<String>, defaults: &[&str]) -> Vec<String> {
    if values.is_empty() {
        defaults.iter().map(|value| (*value).to_string()).collect()
    } else {
        values
    }
}

fn waiting_for_deed_truth_precision() -> GeoE5FuelPrecisionRow {
    GeoE5FuelPrecisionRow {
        status: GeoE5FuelPrecisionStatus::WaitingForTruthPlane,
        reason:
            "Franklin deed-grain truth source is owned by bd-13ju and is not live-scored in bd-1wmw"
                .to_string(),
    }
}
