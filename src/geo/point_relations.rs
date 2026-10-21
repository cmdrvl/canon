#![forbid(unsafe_code)]

//! Native, representation-relative point/parcel observations. Acquisition and
//! interpretation of the point's relationship to a physical target stay outside
//! Canon. Neither containment nor the number of points grants identity authority.

use super::{
    CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_GEOMETRY_REQUEST_VERSION, GeoCanonicalGeometryMm,
    GeoCompositionProfile, GeoCompositionUniverse, GeoEntityLevel, GeoEntityRef,
    GeoEvidenceCompilationRequest, GeoEvidenceError, GeoEvidenceErrorCode, GeoEvidenceRecordRef,
    GeoFeatureValue, GeoGeometryError, GeoGeometryErrorCode, GeoGeometryFeatureInput,
    GeoGeometryTileRequest, GeoLocalFrameContract, GeoPointLocation, GeoPointMm,
    GeoQuantizationAudit, GeoRhoAdmissionPolicy, GeoRhoBasis, GeoRhoContract, GeoRhoObservation,
    GeoRhoObservationKind, GeoSourceGeometry, GeoSourcePointDecimal, GeoTileSourceBinding,
    GeoTypedGeometry, canonical_geometry_bytes, geometry_value::locate_in_canonical_area,
    materialize_geometry_tile,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CANON_GEO_POINT_PARCEL_REQUEST_VERSION: &str = "canon_geo_point_parcel_request.v0";
pub const CANON_GEO_POINT_PARCEL_RELATIONS_VERSION: &str = "canon_geo_point_parcel_relations.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPointSourceTransform {
    pub method_id: String,
    pub method_version: String,
    pub parameters_blake3: String,
    /// None means unknown source/control accuracy, never zero error. This is
    /// separate from the exact frame projection's measured integer snap error.
    pub source_accuracy_mm: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPointTargetRelation {
    PhysicalComponentOf,
    ProposedComponentOf,
    AssociatedObservationOf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoParcelPointObservation {
    pub point_id: String,
    pub source_record: GeoEvidenceRecordRef,
    pub frame_id: String,
    pub coordinate: GeoSourcePointDecimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPointParcelCandidate {
    pub parcel_id: String,
    pub source_record: GeoEvidenceRecordRef,
    pub geometry: Option<GeoGeometryFeatureInput>,
    /// A retained upstream refusal remains an unknown, not a repaired geometry.
    pub retained_admission_refusal: Option<GeoGeometryError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPointParcelRequest {
    pub version: String,
    pub target_ref: String,
    pub target_relation: GeoPointTargetRelation,
    pub target_source_records: Vec<GeoEvidenceRecordRef>,
    pub source_as_of: String,
    pub source_transform: GeoPointSourceTransform,
    pub family: GeoRhoContract,
    pub inventory_source: GeoTileSourceBinding,
    pub frame: GeoLocalFrameContract,
    pub points: Vec<GeoParcelPointObservation>,
    pub candidates: Vec<GeoPointParcelCandidate>,
    pub acquisition_truncated: bool,
    pub max_relation_pairs: u64,
    pub max_predicates: u64,
    pub max_vertices_per_geometry: u64,
    pub max_geometry_bytes: u64,
    pub max_report_bytes: u64,
    pub max_assignments: u64,
    pub max_materialized_models: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPointParcelOutcome {
    Interior,
    Boundary,
    Exterior,
    UnavailableGeometry,
    RetainedAdmissionRefused,
    GeometryAdmissionRefused,
    PointAdmissionRefused,
    MixedFrame,
    UnsupportedTopology,
    ArithmeticFailure,
    BudgetExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPointParcelGroup {
    pub outcome: GeoPointParcelOutcome,
    pub problem: Option<GeoGeometryError>,
    pub point_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPointParcelLocations {
    pub parcel_id: String,
    pub source_record: GeoEvidenceRecordRef,
    pub geometry_blake3: Option<String>,
    pub groups: Vec<GeoPointParcelGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAdmittedParcelPoint {
    pub point_id: String,
    pub source_record: GeoEvidenceRecordRef,
    pub coordinate: Option<GeoPointMm>,
    pub quantization: Option<GeoQuantizationAudit>,
    pub problem: Option<GeoGeometryError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPointParcelRelations {
    pub version: String,
    pub target_ref: String,
    pub target_relation: GeoPointTargetRelation,
    pub target_source_records: Vec<GeoEvidenceRecordRef>,
    pub source_as_of: String,
    pub source_transform: GeoPointSourceTransform,
    pub frame: GeoLocalFrameContract,
    pub points: Vec<GeoAdmittedParcelPoint>,
    pub candidates: Vec<GeoPointParcelLocations>,
    pub max_relation_pairs: u64,
    pub max_predicates: u64,
    pub max_geometry_bytes: u64,
    pub predicates_evaluated: u64,
    pub bbox_exterior_pairs: u64,
    pub canonical_geometry_bytes: u64,
}

fn invalid(message: &str, field: &str) -> GeoEvidenceError {
    GeoEvidenceError {
        code: GeoEvidenceErrorCode::InvalidInput,
        message: message.to_string(),
        detail: BTreeMap::from([("field".to_string(), field.to_string())]),
    }
}

fn problem(code: GeoGeometryErrorCode, message: &str) -> GeoGeometryError {
    GeoGeometryError {
        code,
        message: message.into(),
        detail: BTreeMap::new(),
        budget: None,
    }
}

fn identifier(value: &str, field: &str) -> Result<(), GeoEvidenceError> {
    if value.trim().is_empty() || value.len() > 256 {
        return Err(invalid(
            "Point relation identifiers must be nonempty and bounded",
            field,
        ));
    }
    Ok(())
}

fn record(value: &GeoEvidenceRecordRef) -> Result<(), GeoEvidenceError> {
    identifier(&value.source_record_id, "source_record_id")?;
    identifier(&value.source_vintage, "source_vintage")?;
    super::evidence::validate_blake3("record_blake3", &value.record_blake3)
}

fn materialize_one(
    input: GeoGeometryFeatureInput,
    request: &GeoPointParcelRequest,
    used: &mut u64,
) -> Result<GeoTypedGeometry, GeoGeometryError> {
    let remaining = request.max_geometry_bytes.saturating_sub(*used);
    if remaining == 0 {
        return Err(problem(
            GeoGeometryErrorCode::TileByteBudgetExceeded,
            "Point relation geometry byte budget exhausted",
        ));
    }
    let tile = materialize_geometry_tile(&GeoGeometryTileRequest {
        version: CANON_GEO_GEOMETRY_REQUEST_VERSION.into(),
        frame: request.frame.clone(),
        features: vec![input],
        max_vertices_per_geometry: request.max_vertices_per_geometry,
        max_geometry_bytes_per_tile: remaining,
    })?;
    *used += tile.geometry_bytes;
    let GeoFeatureValue::Geometry { value } = tile
        .features
        .into_iter()
        .next()
        .expect("one admitted feature")
        .value;
    Ok(value)
}

fn failure_outcome(
    error: &GeoGeometryError,
    default: GeoPointParcelOutcome,
) -> GeoPointParcelOutcome {
    match error.code {
        GeoGeometryErrorCode::MixedCrs | GeoGeometryErrorCode::InvalidFrame => {
            GeoPointParcelOutcome::MixedFrame
        }
        GeoGeometryErrorCode::ArithmeticOverflow => GeoPointParcelOutcome::ArithmeticFailure,
        GeoGeometryErrorCode::VertexBudgetExceeded
        | GeoGeometryErrorCode::TileByteBudgetExceeded => GeoPointParcelOutcome::BudgetExhausted,
        _ => default,
    }
}

/// Preflight validates declarations without admitting feature geometry or
/// performing any containment predicates. Native spatial work belongs in the DAG.
pub fn validate_point_parcel_request(
    request: &GeoPointParcelRequest,
) -> Result<(), GeoEvidenceError> {
    if request.version != CANON_GEO_POINT_PARCEL_REQUEST_VERSION {
        return Err(GeoEvidenceError {
            code: GeoEvidenceErrorCode::UnsupportedVersion,
            message: "Unsupported point/parcel request version".into(),
            detail: BTreeMap::new(),
        });
    }
    identifier(&request.target_ref, "target_ref")?;
    identifier(&request.source_as_of, "source_as_of")?;
    identifier(
        &request.source_transform.method_id,
        "source_transform.method_id",
    )?;
    identifier(
        &request.source_transform.method_version,
        "source_transform.method_version",
    )?;
    super::evidence::validate_blake3(
        "source_transform.parameters_blake3",
        &request.source_transform.parameters_blake3,
    )?;
    if request.acquisition_truncated
        || request.points.is_empty()
        || request.candidates.is_empty()
        || request.max_vertices_per_geometry == 0
        || request.max_geometry_bytes == 0
        || request.max_report_bytes == 0
        || request.target_source_records.is_empty()
    {
        return Err(invalid(
            "Point relations require a complete declared acquisition, target provenance, nonempty points/candidates and geometry/report budgets",
            "point_parcel_request",
        ));
    }
    for source in &request.target_source_records {
        record(source)?;
    }
    let pair_count = (request.points.len() as u64)
        .checked_mul(request.candidates.len() as u64)
        .ok_or_else(|| {
            invalid(
                "Point/parcel pair count overflows its budget counter",
                "max_relation_pairs",
            )
        })?;
    if pair_count > request.max_relation_pairs {
        let mut error = invalid(
            "Declared point/parcel pair budget exceeded before geometry work",
            "max_relation_pairs",
        );
        error
            .detail
            .insert("observed".into(), pair_count.to_string());
        error
            .detail
            .insert("configured".into(), request.max_relation_pairs.to_string());
        return Err(error);
    }
    // Validate the common frame even if every supplied geometry is unavailable.
    materialize_geometry_tile(&GeoGeometryTileRequest {
        version: CANON_GEO_GEOMETRY_REQUEST_VERSION.into(),
        frame: request.frame.clone(),
        features: Vec::new(),
        max_vertices_per_geometry: request.max_vertices_per_geometry,
        max_geometry_bytes_per_tile: request.max_geometry_bytes,
    })
    .map_err(|e| invalid(&e.message, "frame"))?;
    super::tile::validate_source_binding("inventory_source", &request.inventory_source).map_err(
        |error| GeoEvidenceError {
            code: GeoEvidenceErrorCode::InvalidInput,
            message: error.message,
            detail: error.detail,
        },
    )?;
    if request.inventory_source.native_entity_level() != Some(super::GeoControlEntityLevel::Parcel)
    {
        return Err(invalid(
            "Point relations require a parcel inventory source",
            "inventory_source.native_scope",
        ));
    }
    if !matches!(&request.family.basis, GeoRhoBasis::Uncalibrated { admission_policy: GeoRhoAdmissionPolicy::SoftWithWeight { cost_if_absent }, .. } if *cost_if_absent > 0)
        || request.family.claim_role != super::GeoEvidenceClaimRole::AttributeObservation
        || request.family.source_lineage_ids.is_empty()
    {
        return Err(invalid(
            "Native point relations require an uncalibrated soft attribute family with declared lineage",
            "family",
        ));
    }
    let mut family = request.family.clone();
    family.source_lineage_ids.sort();
    family.source_lineage_ids.dedup();
    super::evidence::validate_contract(&family)?;
    let mut point_ids = BTreeSet::new();
    let mut parcel_ids = BTreeSet::new();
    let mut records = request.target_source_records.clone();
    for point in &request.points {
        identifier(&point.point_id, "point_id")?;
        identifier(&point.frame_id, "frame_id")?;
        record(&point.source_record)?;
        records.push(point.source_record.clone());
        if !point_ids.insert(&point.point_id) {
            return Err(invalid("Point IDs must be distinct", "points"));
        }
    }
    for candidate in &request.candidates {
        identifier(&candidate.parcel_id, "parcel_id")?;
        record(&candidate.source_record)?;
        records.push(candidate.source_record.clone());
        if !parcel_ids.insert(&candidate.parcel_id) {
            return Err(invalid("Parcel IDs must be distinct", "candidates"));
        }
        if candidate.geometry.is_some() && candidate.retained_admission_refusal.is_some() {
            return Err(invalid(
                "Candidate geometry cannot coexist with an upstream refusal",
                "candidates",
            ));
        }
        if candidate
            .geometry
            .as_ref()
            .is_some_and(|g| g.feature_id != candidate.parcel_id)
        {
            return Err(invalid(
                "Candidate geometry must bind its declared parcel ID",
                "candidates.geometry.feature_id",
            ));
        }
    }
    records.sort();
    records.dedup();
    super::evidence::validate_source_records("point_parcel_request", &records)?;
    Ok(())
}

/// Every declared point/candidate pair receives exactly one typed outcome.
/// Only admitted canonical bounding boxes can bypass the exact native predicate.
pub fn build_point_parcel_relations(
    request: &GeoPointParcelRequest,
) -> Result<GeoPointParcelRelations, GeoEvidenceError> {
    validate_point_parcel_request(request)?;
    let mut points = request.points.clone();
    points.sort_by(|a, b| a.point_id.cmp(&b.point_id));
    let mut candidates = request.candidates.clone();
    candidates.sort_by(|a, b| a.parcel_id.cmp(&b.parcel_id));
    if points.windows(2).any(|w| w[0].point_id == w[1].point_id)
        || candidates
            .windows(2)
            .any(|w| w[0].parcel_id == w[1].parcel_id)
    {
        return Err(invalid(
            "Point and parcel identifiers must be distinct",
            "points/candidates",
        ));
    }
    let mut used = 0;
    let mut admitted = Vec::with_capacity(points.len());
    for point in points {
        identifier(&point.point_id, "point_id")?;
        record(&point.source_record)?;
        let value = if point.frame_id != request.frame.frame_id {
            Err(problem(
                GeoGeometryErrorCode::MixedCrs,
                "Point frame differs from the declared local frame",
            ))
        } else {
            materialize_one(
                GeoGeometryFeatureInput {
                    feature_id: point.point_id.clone(),
                    source_crs: request.frame.source_crs.clone(),
                    geometry: GeoSourceGeometry::Point {
                        coordinate: point.coordinate,
                    },
                },
                request,
                &mut used,
            )
        };
        let (coordinate, quantization, error) = match value {
            Ok(value) => {
                let GeoCanonicalGeometryMm::Point { coordinate } = value.geometry else {
                    unreachable!("point materialization preserves geometry kind")
                };
                (Some(coordinate), Some(value.quantization), None)
            }
            Err(error) => (None, None, Some(error)),
        };
        admitted.push(GeoAdmittedParcelPoint {
            point_id: point.point_id,
            source_record: point.source_record,
            coordinate,
            quantization,
            problem: error,
        });
    }
    let mut predicates = 0;
    let mut bbox_exterior_pairs = 0;
    let mut locations = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        identifier(&candidate.parcel_id, "parcel_id")?;
        record(&candidate.source_record)?;
        if candidate.geometry.is_some() && candidate.retained_admission_refusal.is_some() {
            return Err(invalid(
                "Candidate geometry cannot coexist with an upstream refusal",
                "candidates",
            ));
        }
        let mut geometry = None;
        let mut geometry_hash = None;
        let mut candidate_failure = None;
        if let Some(error) = candidate.retained_admission_refusal {
            candidate_failure =
                Some((GeoPointParcelOutcome::RetainedAdmissionRefused, Some(error)));
        } else if let Some(input) = candidate.geometry {
            if input.feature_id != candidate.parcel_id {
                return Err(invalid(
                    "Candidate geometry feature ID must equal its parcel ID",
                    "candidates.geometry.feature_id",
                ));
            }
            match materialize_one(input, request, &mut used) {
                Ok(value) if matches!(value.geometry, GeoCanonicalGeometryMm::Point { .. }) => {
                    candidate_failure = Some((GeoPointParcelOutcome::UnsupportedTopology, None))
                }
                Ok(value) => {
                    geometry_hash = Some(
                        blake3::hash(
                            &canonical_geometry_bytes(&value).expect("geometry serializes"),
                        )
                        .to_hex()
                        .to_string(),
                    );
                    geometry = Some(value);
                }
                Err(error) => {
                    candidate_failure = Some((
                        failure_outcome(&error, GeoPointParcelOutcome::GeometryAdmissionRefused),
                        Some(error),
                    ))
                }
            }
        } else {
            candidate_failure = Some((GeoPointParcelOutcome::UnavailableGeometry, None));
        }
        let mut groups: BTreeMap<(GeoPointParcelOutcome, String), GeoPointParcelGroup> =
            BTreeMap::new();
        for point in &admitted {
            let (outcome, error) = if let Some(failure) = &candidate_failure {
                failure.clone()
            } else if let Some(error) = &point.problem {
                (
                    failure_outcome(error, GeoPointParcelOutcome::PointAdmissionRefused),
                    Some(error.clone()),
                )
            } else {
                let value = geometry.as_ref().expect("admitted candidate");
                let coordinate = point.coordinate.expect("admitted point");
                if coordinate.x < value.bbox.min_x
                    || coordinate.x > value.bbox.max_x
                    || coordinate.y < value.bbox.min_y
                    || coordinate.y > value.bbox.max_y
                {
                    bbox_exterior_pairs += 1;
                    (GeoPointParcelOutcome::Exterior, None)
                } else if predicates >= request.max_predicates {
                    (GeoPointParcelOutcome::BudgetExhausted, None)
                } else {
                    predicates += 1;
                    match locate_in_canonical_area(&request.frame, &value.geometry, coordinate) {
                        Ok(GeoPointLocation::Interior) => (GeoPointParcelOutcome::Interior, None),
                        Ok(GeoPointLocation::Boundary) => (GeoPointParcelOutcome::Boundary, None),
                        Ok(GeoPointLocation::Exterior) => (GeoPointParcelOutcome::Exterior, None),
                        Err(error) => (
                            failure_outcome(&error, GeoPointParcelOutcome::ArithmeticFailure),
                            Some(error),
                        ),
                    }
                }
            };
            let key = (
                outcome,
                serde_json::to_string(&error).expect("problem serializes"),
            );
            groups
                .entry(key)
                .or_insert_with(|| GeoPointParcelGroup {
                    outcome,
                    problem: error,
                    point_ids: Vec::new(),
                })
                .point_ids
                .push(point.point_id.clone());
        }
        locations.push(GeoPointParcelLocations {
            parcel_id: candidate.parcel_id,
            source_record: candidate.source_record,
            geometry_blake3: geometry_hash,
            groups: groups.into_values().collect(),
        });
    }
    let mut target_sources = request.target_source_records.clone();
    target_sources.sort();
    target_sources.dedup();
    let report = GeoPointParcelRelations {
        version: CANON_GEO_POINT_PARCEL_RELATIONS_VERSION.into(),
        target_ref: request.target_ref.clone(),
        target_relation: request.target_relation,
        target_source_records: target_sources,
        source_as_of: request.source_as_of.clone(),
        source_transform: request.source_transform.clone(),
        frame: request.frame.clone(),
        points: admitted,
        candidates: locations,
        max_relation_pairs: request.max_relation_pairs,
        max_predicates: request.max_predicates,
        max_geometry_bytes: request.max_geometry_bytes,
        predicates_evaluated: predicates,
        bbox_exterior_pairs,
        canonical_geometry_bytes: used,
    };
    if serde_json::to_vec(&report)
        .expect("relations serialize")
        .len() as u64
        > request.max_report_bytes
    {
        return Err(invalid(
            "Point relation report exceeds its declared byte budget",
            "max_report_bytes",
        ));
    }
    Ok(report)
}

/// Validate accounting against the evidence universe; diagnostics never silently
/// drop missing/refused candidates or turn an unknown pair into an exterior pair.
pub fn validate_point_parcel_relations(
    report: &GeoPointParcelRelations,
    universe: &GeoCompositionUniverse,
) -> Result<(), GeoEvidenceError> {
    if report.version != CANON_GEO_POINT_PARCEL_RELATIONS_VERSION || !universe.buildings.is_empty()
    {
        return Err(invalid(
            "Point relation report requires its supported parcel-only contract",
            "point_parcel_relations",
        ));
    }
    identifier(&report.target_ref, "target_ref")?;
    identifier(&report.source_as_of, "source_as_of")?;
    identifier(
        &report.source_transform.method_id,
        "source_transform.method_id",
    )?;
    identifier(
        &report.source_transform.method_version,
        "source_transform.method_version",
    )?;
    super::evidence::validate_blake3(
        "source_transform.parameters_blake3",
        &report.source_transform.parameters_blake3,
    )?;
    materialize_geometry_tile(&GeoGeometryTileRequest {
        version: CANON_GEO_GEOMETRY_REQUEST_VERSION.into(),
        frame: report.frame.clone(),
        features: Vec::new(),
        max_vertices_per_geometry: 1,
        max_geometry_bytes_per_tile: 1,
    })
    .map_err(|e| invalid(&e.message, "frame"))?;
    if report.target_source_records.is_empty() {
        return Err(invalid(
            "Point relations require retained target relationship provenance",
            "target_source_records",
        ));
    }
    for source in &report.target_source_records {
        record(source)?;
    }
    for point in &report.points {
        identifier(&point.point_id, "point_id")?;
        record(&point.source_record)?;
        if point.coordinate.is_some() != point.quantization.is_some()
            || point.coordinate.is_some() == point.problem.is_some()
        {
            return Err(invalid(
                "Point admission must retain either coordinate/audit or its problem",
                "points",
            ));
        }
        if point.coordinate.is_some_and(|p| {
            p.x.unsigned_abs() > report.frame.max_abs_coordinate_mm as u64
                || p.y.unsigned_abs() > report.frame.max_abs_coordinate_mm as u64
        }) {
            return Err(invalid(
                "Admitted point lies outside the declared coordinate domain",
                "points.coordinate",
            ));
        }
    }
    let ids: Vec<_> = report.points.iter().map(|p| p.point_id.as_str()).collect();
    let admitted_ids: BTreeSet<_> = report
        .points
        .iter()
        .filter(|p| p.coordinate.is_some())
        .map(|p| p.point_id.as_str())
        .collect();
    let pairs = (report.points.len() as u64)
        .checked_mul(report.candidates.len() as u64)
        .ok_or_else(|| invalid("Point relation accounting overflows", "max_relation_pairs"))?;
    if pairs > report.max_relation_pairs
        || report.predicates_evaluated > report.max_predicates
        || report.canonical_geometry_bytes > report.max_geometry_bytes
        || report
            .predicates_evaluated
            .checked_add(report.bbox_exterior_pairs)
            .is_none_or(|count| count > pairs)
    {
        return Err(invalid(
            "Point relation counters exceed the declared budgets or pair universe",
            "point_parcel_relations",
        ));
    }
    if ids.is_empty() || ids.windows(2).any(|w| w[0] >= w[1]) {
        return Err(invalid(
            "Relation point IDs must be sorted and distinct",
            "points",
        ));
    }
    let mut parcels = universe.parcels.clone();
    parcels.sort();
    if report
        .candidates
        .iter()
        .map(|c| &c.parcel_id)
        .ne(parcels.iter())
    {
        return Err(invalid(
            "Point relations must account for every declared parcel exactly once",
            "candidates",
        ));
    }
    for candidate in &report.candidates {
        identifier(&candidate.parcel_id, "parcel_id")?;
        record(&candidate.source_record)?;
        if let Some(hash) = &candidate.geometry_blake3 {
            super::evidence::validate_blake3("geometry_blake3", hash)?;
        }
        let mut seen = BTreeSet::new();
        for group in &candidate.groups {
            if group.outcome == GeoPointParcelOutcome::BudgetExhausted
                && group.problem.is_none()
                && report.predicates_evaluated != report.max_predicates
            {
                return Err(invalid(
                    "Predicate-budget unknowns must bind an exhausted predicate budget",
                    "groups",
                ));
            }
            if group.point_ids.is_empty() || group.point_ids.windows(2).any(|w| w[0] >= w[1]) {
                return Err(invalid(
                    "Relation groups require sorted distinct point IDs",
                    "groups",
                ));
            }
            for id in &group.point_ids {
                if !seen.insert(id.as_str()) {
                    return Err(invalid(
                        "A point/parcel pair cannot have multiple outcomes",
                        "groups",
                    ));
                }
                if matches!(
                    group.outcome,
                    GeoPointParcelOutcome::Interior
                        | GeoPointParcelOutcome::Boundary
                        | GeoPointParcelOutcome::Exterior
                ) && (candidate.geometry_blake3.is_none() || !admitted_ids.contains(id.as_str()))
                {
                    return Err(invalid(
                        "A known geometric location needs an admitted point and geometry",
                        "groups",
                    ));
                }
            }
        }
        if seen.iter().copied().ne(ids.iter().copied()) {
            return Err(invalid(
                "Every point/parcel pair requires a retained outcome",
                "groups",
            ));
        }
    }
    super::evidence::validate_source_records(
        "point_parcel_relations",
        &point_parcel_source_records(report),
    )?;
    Ok(())
}

pub(crate) fn point_parcel_source_records(
    report: &GeoPointParcelRelations,
) -> Vec<GeoEvidenceRecordRef> {
    let mut records = report.target_source_records.clone();
    records.extend(report.points.iter().map(|p| p.source_record.clone()));
    records.extend(report.candidates.iter().map(|c| c.source_record.clone()));
    records.sort();
    records.dedup();
    records
}

/// The report is diagnostic; each supported parcel receives one preference for
/// the whole dependent family, irrespective of the number of interior points.
pub fn materialize_point_parcel_evidence(
    request: &GeoPointParcelRequest,
) -> Result<GeoEvidenceCompilationRequest, GeoEvidenceError> {
    let weight = match request.family.basis {
        GeoRhoBasis::Uncalibrated {
            admission_policy: GeoRhoAdmissionPolicy::SoftWithWeight { cost_if_absent },
            ..
        } if cost_if_absent > 0 => cost_if_absent,
        _ => {
            return Err(invalid(
                "Native point/parcel evidence requires an explicit uncalibrated soft family",
                "family.basis",
            ));
        }
    };
    if request.family.source_lineage_ids.is_empty() {
        return Err(invalid(
            "Point family lineage is required; repeated points are dependent",
            "family.source_lineage_ids",
        ));
    }
    let report = build_point_parcel_relations(request)?;
    let universe = GeoCompositionUniverse {
        parcels: report
            .candidates
            .iter()
            .map(|c| c.parcel_id.clone())
            .collect(),
        buildings: Vec::new(),
    };
    validate_point_parcel_relations(&report, &universe)?;
    let records = point_parcel_source_records(&report);
    let mut observations = Vec::new();
    for candidate in &report.candidates {
        if candidate
            .groups
            .iter()
            .any(|g| g.outcome == GeoPointParcelOutcome::Interior)
        {
            let interior: BTreeSet<_> = candidate
                .groups
                .iter()
                .filter(|g| g.outcome == GeoPointParcelOutcome::Interior)
                .flat_map(|g| g.point_ids.iter())
                .collect();
            let mut support_records = report.target_source_records.clone();
            support_records.push(candidate.source_record.clone());
            support_records.extend(
                report
                    .points
                    .iter()
                    .filter(|p| interior.contains(&p.point_id))
                    .map(|p| p.source_record.clone()),
            );
            support_records.sort();
            support_records.dedup();
            observations.push(GeoRhoObservation {
                id: format!("{}:support:{}", request.family.id, candidate.parcel_id),
                contract_id: request.family.id.clone(),
                source_records: support_records,
                valid_time: None,
                observation: GeoRhoObservationKind::PreferMember {
                    member: GeoEntityRef {
                        level: GeoEntityLevel::Parcel,
                        id: candidate.parcel_id.clone(),
                    },
                    cost_if_absent: weight,
                },
            });
        }
    }
    observations.push(GeoRhoObservation {
        id: format!("{}:point_relations", request.family.id),
        contract_id: request.family.id.clone(),
        source_records: records,
        valid_time: None,
        observation: GeoRhoObservationKind::PointParcelRelations {
            report: Box::new(report),
        },
    });
    observations.sort_by(|a, b| a.id.cmp(&b.id));
    let mut family = request.family.clone();
    family.source_lineage_ids.sort();
    family.source_lineage_ids.dedup();
    let evidence = GeoEvidenceCompilationRequest {
        version: CANON_GEO_EVIDENCE_REQUEST_VERSION.into(),
        profile: GeoCompositionProfile::parcel(),
        universe,
        contracts: vec![family],
        observations,
        max_assignments: request.max_assignments,
        max_materialized_models: request.max_materialized_models,
    };
    super::compile_evidence(&evidence)?;
    Ok(evidence)
}
