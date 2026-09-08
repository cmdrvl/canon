#![forbid(unsafe_code)]

//! Deterministic null observer over an already-landed footprint plane.
//!
//! The rule re-emits landed footprint outlines with the retained tile pin
//! attached. It is a redundancy baseline: admitted rows must preserve the
//! residual status, count, and backbone.

use super::{
    composition::{
        GeoCompositionArtifact, GeoCompositionProfile, GeoCompositionStatus, GeoEntityLevel,
        GeoEntityRef,
    },
    evidence::{
        GeoEvidenceClaimRole, GeoEvidenceRecordRef, GeoRhoBasis, GeoRhoContract,
        GeoRhoObservationKind,
    },
    geometry::{GeoLinearRingMm, footprint_majority_area_inside_parcel},
    geometry_value::{GeoCanonicalPolygonMm, GeoCanonicalRingMm},
    materialize::{
        CANON_GEO_WAREHOUSE_ROWS_VERSION, GeoWarehouseBuildingParcelRow, GeoWarehouseEvidenceRow,
        GeoWarehouseParcelRow, GeoWarehouseRowsRequest,
    },
    observer::{
        CANON_GEO_OBSERVER_VERSION, GeoImageTilePin, GeoObservationKind, GeoObservationPayload,
        GeoObservationRow, GeoObservationRowsArtifact, GeoObserverContract, GeoObserverError,
        GeoObserverErrorCode, GeoObserverIdentity, admit_observations,
        validate_observation_rows_artifact,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};

pub const CANON_GEO_OBSERVER_CHARACTERIZATION_VERSION: &str =
    "canon_geo_observer_characterization.v0";
pub const GEO_NULL_FOOTPRINT_OBSERVER_ID: &str = "observer.null_footprint";
pub const GEO_NULL_FOOTPRINT_SOURCE_DATASET: &str = "observer.null_footprint";
pub const GEO_NULL_FOOTPRINT_RHO_CONTRACT_ID: &str = "rho.observer.null_footprint.v0";
pub const GEO_NULL_FOOTPRINT_RULE_ID: &str = "null_footprint_outline";
pub const GEO_NULL_FOOTPRINT_RULE_VERSION: &str = "1";
pub const GEO_NULL_FOOTPRINT_METHOD_ID: &str = "observer.null_footprint.prefer_member";
pub const GEO_NULL_FOOTPRINT_METHOD_VERSION: &str = "v0";
pub const GEO_NULL_FOOTPRINT_INVARIANT_ID: &str = "footprint_outline_reemits_landed_plane";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoObserverCharacterizationArtifact {
    pub version: String,
    pub observer_id: String,
    pub population_blake3: String,
    pub per_kind: BTreeMap<String, GeoObserverKindCharacterization>,
    pub method: String,
    pub is_null_baseline: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_redundant_case_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoObserverKindCharacterization {
    pub compared: u64,
    pub exact_agreement: u64,
    pub max_abs_error: u64,
    pub error_band: GeoObserverErrorBand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoObserverErrorBand {
    pub lower_slack: u64,
    pub upper_slack: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNullFootprintPlaneSourcePin {
    pub source_dataset: String,
    pub source_release: String,
    pub source_content_sha256: String,
    pub parser_version: String,
    pub license_terms: String,
    pub attribution_text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_lineage_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNullFootprintPlane {
    pub source_pin: GeoNullFootprintPlaneSourcePin,
    pub frame_id: String,
    pub parcel_rings: BTreeMap<String, GeoCanonicalPolygonMm>,
    pub footprint_rings: BTreeMap<String, GeoCanonicalPolygonMm>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNullFootprintObservationResult {
    pub artifact: GeoObservationRowsArtifact,
    pub emitted_footprint_ids: Vec<String>,
    pub excluded_by_window: Vec<String>,
    pub window_blake3: String,
    pub holes_ignored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNullUnassignedFootprint {
    pub footprint_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNullWarehouseRows {
    pub source_pin: GeoNullFootprintPlaneSourcePin,
    pub request: GeoWarehouseRowsRequest,
    pub unassigned_footprints: Vec<GeoNullUnassignedFootprint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNullRedundancyCase {
    pub case_id: String,
    pub before: GeoCompositionArtifact,
    pub after: GeoCompositionArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNullRedundancyReport {
    pub denominator: u64,
    pub redundant: bool,
    pub non_redundant_case_ids: Vec<String>,
}

pub fn null_footprint_observer_contract(
    population_id: impl Into<String>,
    characterization_blake3: impl Into<String>,
) -> GeoObserverContract {
    GeoObserverContract {
        id: GEO_NULL_FOOTPRINT_OBSERVER_ID.to_string(),
        version: CANON_GEO_OBSERVER_VERSION.to_string(),
        identity: GeoObserverIdentity::RuleBased {
            rule_id: GEO_NULL_FOOTPRINT_RULE_ID.to_string(),
            rule_version: GEO_NULL_FOOTPRINT_RULE_VERSION.to_string(),
        },
        output_kinds: vec![GeoObservationKind::FootprintOutline],
        error_population_id: population_id.into(),
        characterization_blake3: characterization_blake3.into(),
        rho_contract_ids: vec![GEO_NULL_FOOTPRINT_RHO_CONTRACT_ID.to_string()],
    }
}

pub fn null_footprint_rho_contract(
    source_pin: &GeoNullFootprintPlaneSourcePin,
    population_id: &str,
) -> Result<GeoRhoContract, GeoObserverError> {
    validate_source_pin(source_pin)?;
    validate_nonempty("population_id", population_id)?;

    let mut source_lineage_ids = source_pin.source_lineage_ids.clone();
    source_lineage_ids.push(format!(
        "{}:{}",
        source_pin.source_dataset, source_pin.source_release
    ));
    source_lineage_ids.push(format!("{GEO_NULL_FOOTPRINT_OBSERVER_ID}:{population_id}"));
    source_lineage_ids.sort();
    source_lineage_ids.dedup();

    Ok(GeoRhoContract {
        id: GEO_NULL_FOOTPRINT_RHO_CONTRACT_ID.to_string(),
        version: "v0".to_string(),
        source_dataset: GEO_NULL_FOOTPRINT_SOURCE_DATASET.to_string(),
        source_release: source_pin.source_release.clone(),
        source_lineage_ids,
        method_id: GEO_NULL_FOOTPRINT_METHOD_ID.to_string(),
        method_version: GEO_NULL_FOOTPRINT_METHOD_VERSION.to_string(),
        claim_role: GeoEvidenceClaimRole::AttributeObservation,
        basis: GeoRhoBasis::LogicalRelaxation {
            invariant_id: GEO_NULL_FOOTPRINT_INVARIANT_ID.to_string(),
        },
    })
}

pub fn emit_null_footprint_observations(
    contract: &GeoObserverContract,
    rho_contract: &GeoRhoContract,
    tile_pin: &GeoImageTilePin,
    window_blake3: &str,
    window: &GeoCanonicalPolygonMm,
    frame_id: &str,
    footprint_rings: &BTreeMap<String, GeoCanonicalPolygonMm>,
    forbidden_license_ids: &[String],
) -> Result<GeoNullFootprintObservationResult, GeoObserverError> {
    if contract.id != GEO_NULL_FOOTPRINT_OBSERVER_ID {
        return Err(observer_invalid(
            "Geo null observer rows require the null footprint observer contract",
            [("observer_id", contract.id.clone())],
        ));
    }
    if contract.rho_contract_ids.as_slice() != std::slice::from_ref(&rho_contract.id) {
        return Err(observer_invalid(
            "Geo null observer contract must bind exactly its declared rho contract",
            [
                ("observer_id", contract.id.clone()),
                ("rho_contract_id", rho_contract.id.clone()),
            ],
        ));
    }
    validate_blake3("window_blake3", window_blake3)?;
    validate_nonempty("frame_id", frame_id)?;

    let window_ring = predicate_ring(frame_id, window)?;
    let mut rows = Vec::new();
    let mut emitted_footprint_ids = Vec::new();
    let mut excluded_by_window = Vec::new();

    for (footprint_id, footprint) in footprint_rings {
        validate_nonempty("footprint_id", footprint_id)?;
        let footprint_ring = predicate_ring(frame_id, footprint)?;
        if footprint_majority_area_inside_parcel(&footprint_ring, &window_ring)
            .map_err(|error| map_source_error("window_majority", footprint_id, error))?
        {
            let ring_blake3 = canonical_ring_blake3(&footprint.exterior)?;
            let row = GeoObservationRow {
                id: observation_id(footprint_id),
                observer_id: contract.id.clone(),
                tile_pins: vec![tile_pin.clone()],
                window_blake3: window_blake3.to_string(),
                kind: GeoObservationKind::FootprintOutline,
                payload: GeoObservationPayload::FootprintOutline {
                    ring_blake3: ring_blake3.clone(),
                },
                crop_blake3: deterministic_crop_blake3(&tile_pin.blake3, window_blake3),
                label_blake3: ring_blake3,
            };
            rows.push(row);
            emitted_footprint_ids.push(footprint_id.clone());
        } else {
            excluded_by_window.push(footprint_id.clone());
        }
    }

    let artifact = admit_observations(
        contract,
        &rows,
        std::slice::from_ref(rho_contract),
        forbidden_license_ids,
    )?;
    Ok(GeoNullFootprintObservationResult {
        artifact,
        emitted_footprint_ids,
        excluded_by_window,
        window_blake3: window_blake3.to_string(),
        holes_ignored: true,
    })
}

pub fn characterize_null(
    artifact: &GeoObservationRowsArtifact,
    footprint_rings: &BTreeMap<String, GeoCanonicalPolygonMm>,
    population_blake3: &str,
) -> Result<GeoObserverCharacterizationArtifact, GeoObserverError> {
    validate_observation_rows_artifact(artifact)?;
    validate_blake3("population_blake3", population_blake3)?;
    if artifact.contract.id != GEO_NULL_FOOTPRINT_OBSERVER_ID {
        return Err(observer_invalid(
            "Geo null observer characterization requires the null footprint observer",
            [("observer_id", artifact.contract.id.clone())],
        ));
    }

    let compared = validate_observation_rows_match_plane(artifact, footprint_rings)?;

    let artifact = GeoObserverCharacterizationArtifact {
        version: CANON_GEO_OBSERVER_CHARACTERIZATION_VERSION.to_string(),
        observer_id: artifact.contract.id.clone(),
        population_blake3: population_blake3.to_string(),
        per_kind: BTreeMap::from([(
            observation_kind_key(GeoObservationKind::FootprintOutline).to_string(),
            GeoObserverKindCharacterization {
                compared,
                exact_agreement: compared,
                max_abs_error: 0,
                error_band: GeoObserverErrorBand {
                    lower_slack: 0,
                    upper_slack: 0,
                },
            },
        )]),
        method: "exact_ring_digest_agreement_from_landed_footprint_plane".to_string(),
        is_null_baseline: true,
        non_redundant_case_ids: Vec::new(),
    };
    validate_observer_characterization_artifact(&artifact)?;
    Ok(artifact)
}

pub fn null_observer_rows_to_warehouse_rows(
    artifact: &GeoObservationRowsArtifact,
    plane: &GeoNullFootprintPlane,
    max_assignments: u64,
    max_materialized_models: u64,
) -> Result<GeoNullWarehouseRows, GeoObserverError> {
    validate_observation_rows_artifact(artifact)?;
    validate_null_plane(plane)?;
    validate_observation_rows_match_plane(artifact, &plane.footprint_rings)?;
    if artifact.contract.id != GEO_NULL_FOOTPRINT_OBSERVER_ID {
        return Err(observer_invalid(
            "Geo null observer warehouse conversion requires the null footprint observer",
            [("observer_id", artifact.contract.id.clone())],
        ));
    }
    if max_assignments == 0 || max_materialized_models == 0 {
        return Err(observer_invalid(
            "Geo null observer warehouse conversion requires positive budgets",
            [
                ("max_assignments", max_assignments.to_string()),
                (
                    "max_materialized_models",
                    max_materialized_models.to_string(),
                ),
            ],
        ));
    }

    let mut building_parcel_rows = Vec::new();
    let mut unassigned_footprints = Vec::new();
    for (footprint_id, footprint) in &plane.footprint_rings {
        let mut matching_parcels = Vec::new();
        let footprint_ring = predicate_ring(&plane.frame_id, footprint)?;
        for (parcel_id, parcel) in &plane.parcel_rings {
            let parcel_ring = predicate_ring(&plane.frame_id, parcel)?;
            if footprint_majority_area_inside_parcel(&footprint_ring, &parcel_ring)
                .map_err(|error| map_source_error("parcel_majority", footprint_id, error))?
            {
                matching_parcels.push(parcel_id.clone());
            }
        }

        if matching_parcels.is_empty() {
            building_parcel_rows.push(GeoWarehouseBuildingParcelRow {
                building_id: footprint_id.clone(),
                parcel_id: None,
            });
            unassigned_footprints.push(GeoNullUnassignedFootprint {
                footprint_id: footprint_id.clone(),
                reason: "no_majority_parcel".to_string(),
            });
        } else {
            for parcel_id in matching_parcels {
                building_parcel_rows.push(GeoWarehouseBuildingParcelRow {
                    building_id: footprint_id.clone(),
                    parcel_id: Some(parcel_id),
                });
            }
        }
    }
    building_parcel_rows.sort_by(|left, right| {
        (
            left.building_id.as_str(),
            left.parcel_id.as_deref().unwrap_or(""),
        )
            .cmp(&(
                right.building_id.as_str(),
                right.parcel_id.as_deref().unwrap_or(""),
            ))
    });
    unassigned_footprints.sort_by(|left, right| left.footprint_id.cmp(&right.footprint_id));

    let observed_footprint_ids = observed_footprint_ids(artifact)?;
    let mut evidence_rows = Vec::new();
    for footprint_id in observed_footprint_ids {
        let row_id = observation_id(&footprint_id);
        let footprint = plane.footprint_rings.get(&footprint_id).ok_or_else(|| {
            observer_invalid(
                "Geo null observer evidence references an unavailable footprint",
                [("footprint_id", footprint_id.clone())],
            )
        })?;
        let ring_blake3 = canonical_ring_blake3(&footprint.exterior)?;
        evidence_rows.push(GeoWarehouseEvidenceRow {
            observation_id: row_id.clone(),
            contract_id: GEO_NULL_FOOTPRINT_RHO_CONTRACT_ID.to_string(),
            source_record: GeoEvidenceRecordRef {
                source_record_id: format!("{row_id}:ring"),
                source_vintage: plane.source_pin.source_release.clone(),
                record_blake3: ring_blake3,
            },
            valid_time: None,
            observation: GeoRhoObservationKind::PreferMember {
                member: GeoEntityRef::new(GeoEntityLevel::Building, footprint_id),
                cost_if_absent: 0,
            },
        });
    }

    let request = GeoWarehouseRowsRequest {
        version: CANON_GEO_WAREHOUSE_ROWS_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        parcel_rows: plane
            .parcel_rings
            .keys()
            .map(|parcel_id| GeoWarehouseParcelRow {
                parcel_id: parcel_id.clone(),
            })
            .collect(),
        building_parcel_rows,
        contracts: vec![null_footprint_rho_contract(
            &plane.source_pin,
            &artifact.contract.error_population_id,
        )?],
        evidence_rows,
        max_assignments,
        max_materialized_models,
    };
    Ok(GeoNullWarehouseRows {
        source_pin: plane.source_pin.clone(),
        request,
        unassigned_footprints,
    })
}

pub fn assert_redundant(
    cases: &[GeoNullRedundancyCase],
) -> Result<GeoNullRedundancyReport, GeoObserverError> {
    let mut non_redundant_case_ids = Vec::new();
    for case in cases {
        validate_nonempty("case_id", &case.case_id)?;
        if !composition_redundant(&case.before, &case.after) {
            non_redundant_case_ids.push(case.case_id.clone());
        }
    }
    non_redundant_case_ids.sort();
    non_redundant_case_ids.dedup();
    let denominator = u64::try_from(cases.len())
        .map_err(|_| observer_overflow("null observer redundancy denominator"))?;
    if let Some(case_id) = non_redundant_case_ids.first() {
        return Err(observer_error(
            GeoObserverErrorCode::ObserverNotRedundant,
            "Geo null observer changed the residual status, count, or backbone",
            [
                ("case_id", case_id.clone()),
                ("non_redundant_case_ids", non_redundant_case_ids.join(",")),
            ],
        ));
    }
    Ok(GeoNullRedundancyReport {
        denominator,
        redundant: true,
        non_redundant_case_ids,
    })
}

pub fn validate_observer_characterization_artifact(
    artifact: &GeoObserverCharacterizationArtifact,
) -> Result<(), GeoObserverError> {
    if artifact.version != CANON_GEO_OBSERVER_CHARACTERIZATION_VERSION {
        return Err(observer_error(
            GeoObserverErrorCode::UnsupportedVersion,
            "Unsupported Geo observer characterization artifact version",
            [
                ("actual", artifact.version.clone()),
                (
                    "expected",
                    CANON_GEO_OBSERVER_CHARACTERIZATION_VERSION.to_string(),
                ),
            ],
        ));
    }
    validate_nonempty("observer_id", &artifact.observer_id)?;
    validate_blake3("population_blake3", &artifact.population_blake3)?;
    validate_nonempty("method", &artifact.method)?;
    if artifact.per_kind.is_empty() {
        return Err(observer_invalid(
            "Geo observer characterization requires at least one observation kind",
            [("per_kind", "0".to_string())],
        ));
    }
    for (kind, characterization) in &artifact.per_kind {
        validate_nonempty("per_kind.kind", kind)?;
        if characterization.exact_agreement > characterization.compared {
            return Err(observer_invalid(
                "Geo observer characterization exact agreement cannot exceed compared rows",
                [
                    ("kind", kind.clone()),
                    ("compared", characterization.compared.to_string()),
                    (
                        "exact_agreement",
                        characterization.exact_agreement.to_string(),
                    ),
                ],
            ));
        }
        if artifact.is_null_baseline
            && (characterization.max_abs_error != 0
                || characterization.error_band.lower_slack != 0
                || characterization.error_band.upper_slack != 0)
        {
            return Err(observer_error(
                GeoObserverErrorCode::ObserverNullRingMismatch,
                "Geo null observer baseline must have zero error",
                [
                    ("kind", kind.clone()),
                    ("max_abs_error", characterization.max_abs_error.to_string()),
                ],
            ));
        }
    }
    validate_sorted_distinct_ids("non_redundant_case_ids", &artifact.non_redundant_case_ids)
}

pub fn canonical_observer_characterization_bytes(
    artifact: &GeoObserverCharacterizationArtifact,
) -> Result<Vec<u8>, GeoObserverError> {
    validate_observer_characterization_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        observer_invalid(
            "Geo observer characterization artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub fn canonical_ring_blake3(ring: &GeoCanonicalRingMm) -> Result<String, GeoObserverError> {
    serde_json::to_vec(ring)
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
        .map_err(|error| {
            observer_invalid(
                "Geo canonical ring could not be serialized for hashing",
                [("error", error.to_string())],
            )
        })
}

pub fn canonical_polygon_blake3(
    polygon: &GeoCanonicalPolygonMm,
) -> Result<String, GeoObserverError> {
    serde_json::to_vec(polygon)
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
        .map_err(|error| {
            observer_invalid(
                "Geo canonical polygon could not be serialized for hashing",
                [("error", error.to_string())],
            )
        })
}

fn observed_footprint_ids(
    artifact: &GeoObservationRowsArtifact,
) -> Result<Vec<String>, GeoObserverError> {
    let mut ids = Vec::new();
    for row in &artifact.rows {
        ids.push(footprint_id_from_observation_id(&row.id)?.to_string());
    }
    ids.sort();
    reject_duplicates("observed_footprint_ids", ids.iter().map(String::as_str))?;
    Ok(ids)
}

fn validate_observation_rows_match_plane(
    artifact: &GeoObservationRowsArtifact,
    footprint_rings: &BTreeMap<String, GeoCanonicalPolygonMm>,
) -> Result<u64, GeoObserverError> {
    let mut compared = 0_u64;
    for row in &artifact.rows {
        if row.kind != GeoObservationKind::FootprintOutline {
            return Err(observer_invalid(
                "Geo null observer characterization only accepts footprint-outline rows",
                [("observation_id", row.id.clone())],
            ));
        }
        let footprint_id = footprint_id_from_observation_id(&row.id)?;
        let footprint = footprint_rings.get(footprint_id).ok_or_else(|| {
            observer_invalid(
                "Geo null observer row references an unavailable landed footprint",
                [
                    ("observation_id", row.id.clone()),
                    ("footprint_id", footprint_id.to_string()),
                ],
            )
        })?;
        let expected = canonical_ring_blake3(&footprint.exterior)?;
        let GeoObservationPayload::FootprintOutline { ring_blake3 } = &row.payload else {
            return Err(observer_invalid(
                "Geo null observer row carries a non-outline payload",
                [("observation_id", row.id.clone())],
            ));
        };
        if ring_blake3 != &expected || row.label_blake3 != expected {
            return Err(observer_error(
                GeoObserverErrorCode::ObserverNullRingMismatch,
                "Geo null observer outline does not match the landed footprint ring",
                [
                    ("footprint_id", footprint_id.to_string()),
                    ("expected", expected),
                    ("actual", ring_blake3.clone()),
                ],
            ));
        }
        compared = compared
            .checked_add(1)
            .ok_or_else(|| observer_overflow("observer characterization compared rows"))?;
    }
    Ok(compared)
}

fn footprint_id_from_observation_id(row_id: &str) -> Result<&str, GeoObserverError> {
    row_id.strip_prefix("obs:null:").ok_or_else(|| {
        observer_invalid(
            "Geo null observer row ids must use the obs:null:<footprint_id> form",
            [("observation_id", row_id.to_string())],
        )
    })
}

fn observation_id(footprint_id: &str) -> String {
    format!("obs:null:{footprint_id}")
}

fn deterministic_crop_blake3(tile_blake3: &str, window_blake3: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(tile_blake3.as_bytes());
    hasher.update(window_blake3.as_bytes());
    hasher.finalize().to_hex().to_string()
}

fn predicate_ring(
    frame_id: &str,
    polygon: &GeoCanonicalPolygonMm,
) -> Result<GeoLinearRingMm, GeoObserverError> {
    let mut closed = polygon.exterior.vertices.clone();
    let Some(first) = closed.first().copied() else {
        return Err(observer_invalid(
            "Geo null observer polygons require non-empty exterior rings",
            [("frame_id", frame_id.to_string())],
        ));
    };
    closed.push(first);
    GeoLinearRingMm::new(frame_id.to_string(), closed)
        .map_err(|error| map_source_error("predicate_ring", frame_id, error))
}

fn validate_null_plane(plane: &GeoNullFootprintPlane) -> Result<(), GeoObserverError> {
    validate_source_pin(&plane.source_pin)?;
    validate_nonempty("frame_id", &plane.frame_id)?;
    if plane.parcel_rings.is_empty() || plane.footprint_rings.is_empty() {
        return Err(observer_invalid(
            "Geo null observer requires non-empty parcel and footprint planes",
            [
                ("parcel_rings", plane.parcel_rings.len().to_string()),
                ("footprint_rings", plane.footprint_rings.len().to_string()),
            ],
        ));
    }
    for (parcel_id, polygon) in &plane.parcel_rings {
        validate_nonempty("parcel_id", parcel_id)?;
        predicate_ring(&plane.frame_id, polygon)?;
    }
    for (footprint_id, polygon) in &plane.footprint_rings {
        validate_nonempty("footprint_id", footprint_id)?;
        predicate_ring(&plane.frame_id, polygon)?;
    }
    Ok(())
}

fn validate_source_pin(pin: &GeoNullFootprintPlaneSourcePin) -> Result<(), GeoObserverError> {
    validate_nonempty("source_dataset", &pin.source_dataset)?;
    validate_nonempty("source_release", &pin.source_release)?;
    validate_sha256("source_content_sha256", &pin.source_content_sha256)?;
    validate_nonempty("parser_version", &pin.parser_version)?;
    validate_nonempty("license_terms", &pin.license_terms)?;
    validate_nonempty("attribution_text", &pin.attribution_text)?;
    validate_sorted_distinct_ids("source_lineage_ids", &pin.source_lineage_ids)
}

fn composition_redundant(before: &GeoCompositionArtifact, after: &GeoCompositionArtifact) -> bool {
    same_status(before.status, after.status)
        && before.summary.residual_model_count == after.summary.residual_model_count
        && before.summary.residual_model_count_complete
            == after.summary.residual_model_count_complete
        && before.summary.residual_model_count_saturated
            == after.summary.residual_model_count_saturated
        && before.hard_forced == after.hard_forced
}

fn same_status(left: GeoCompositionStatus, right: GeoCompositionStatus) -> bool {
    left == right
}

fn observation_kind_key(kind: GeoObservationKind) -> &'static str {
    match kind {
        GeoObservationKind::StructureCountInWindow => "structure_count_in_window",
        GeoObservationKind::FootprintOutline => "footprint_outline",
        GeoObservationKind::HeightOrFloors => "height_or_floors",
        GeoObservationKind::PresentAtVintage => "present_at_vintage",
        GeoObservationKind::AbsentAtVintage => "absent_at_vintage",
        GeoObservationKind::ChangeEvent => "change_event",
    }
}

fn validate_sorted_distinct_ids(
    field: &'static str,
    ids: &[String],
) -> Result<(), GeoObserverError> {
    let mut previous: Option<&str> = None;
    for id in ids {
        validate_nonempty(field, id)?;
        if previous.is_some_and(|previous| previous >= id.as_str()) {
            return Err(observer_invalid(
                "Geo null observer id lists must be sorted and distinct",
                [("field", field.to_string()), ("id", id.clone())],
            ));
        }
        previous = Some(id.as_str());
    }
    Ok(())
}

fn reject_duplicates<'a>(
    field: &'static str,
    values: impl IntoIterator<Item = &'a str>,
) -> Result<(), GeoObserverError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(observer_invalid(
                "Geo null observer values must be unique",
                [("field", field.to_string()), ("value", value.to_string())],
            ));
        }
    }
    Ok(())
}

fn validate_nonempty(field: &'static str, value: &str) -> Result<(), GeoObserverError> {
    if value.is_empty() || value.trim() != value {
        return Err(observer_invalid(
            "Geo null observer string fields must be non-empty and canonical-trimmed",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn validate_blake3(field: &'static str, value: &str) -> Result<(), GeoObserverError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(observer_invalid(
            "Geo null observer BLAKE3 fields must be lowercase fixed-width hex",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn validate_sha256(field: &'static str, value: &str) -> Result<(), GeoObserverError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(observer_invalid(
            "Geo null observer SHA256 fields must be lowercase fixed-width hex",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn observer_error(
    code: GeoObserverErrorCode,
    message: impl Into<String>,
    detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
) -> GeoObserverError {
    GeoObserverError {
        code,
        message: message.into(),
        detail: detail
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect(),
    }
}

fn observer_invalid(
    message: impl Into<String>,
    detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
) -> GeoObserverError {
    observer_error(GeoObserverErrorCode::InvalidInput, message, detail)
}

fn observer_overflow(context: &'static str) -> GeoObserverError {
    observer_error(
        GeoObserverErrorCode::ArithmeticOverflow,
        "Geo null observer arithmetic overflow",
        [("context", context)],
    )
}

fn map_source_error(field: &'static str, id: &str, error: impl Error) -> GeoObserverError {
    observer_invalid(
        "Geo null observer source geometry failed the exact predicate",
        [
            ("field", field.to_string()),
            ("id", id.to_string()),
            ("error", error.to_string()),
        ],
    )
}
