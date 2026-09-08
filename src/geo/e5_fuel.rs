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

pub const E5_MICROSOFT_GLOBALML_FOOTPRINTS_SOURCE_INSTANCE_ID: &str =
    "source.microsoft_globalml.building_footprints";
pub const E5_MICROSOFT_GLOBALML_FOOTPRINTS_RELEASE_ID: &str = "2026-07-24";
pub const E5_MICROSOFT_GLOBALML_FOOTPRINTS_ROWS: u64 = 141_818_047;
pub const E5_MICROSOFT_GLOBALML_FRANKLIN_THIN_TIER_FEATURES: u64 = 168_778;

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
