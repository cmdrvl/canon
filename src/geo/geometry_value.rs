#![forbid(unsafe_code)]

//! Canonical typed geometry at the offline tile-artifact boundary.
//!
//! Source coordinates are fixed-scale decimal strings, never binary floats.
//! A versioned affine frame maps their exact integer representation to local
//! millimetres and records the snap error. Decision-time geometry therefore
//! consumes only canonical integers. This makes replay exact relative to the
//! admitted artifact; it does not make the source survey or projection exact.

use super::geometry::{
    GeoLinearRingMm, GeoPointLocation, GeoPointMm, GeoPredicateError, GeoPredicateErrorCode,
    GeoSegmentIntersection, exact_segment_intersection,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_GEOMETRY_REQUEST_VERSION: &str = "canon_geo_geometry_request.v0";
pub const CANON_GEO_GEOMETRY_VALUE_VERSION: &str = "canon_geo_geometry_value.v0";
pub const CANON_GEO_GEOMETRY_TILE_VERSION: &str = "canon_geo_geometry_tile.v0";
pub const CANON_GEO_LOCAL_FRAME_VERSION: &str = "canon_geo_local_frame.v0";

const MAX_SOURCE_DECIMAL_PLACES: u32 = 9;
const MAX_IDENTIFIER_BYTES: usize = 256;

/// Source-axis semantics used only for admission checks. Geographic frames
/// reject longitude wrapping rather than guessing how to unwrap a ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoSourceAxisDomain {
    Planar,
    GeographicLongitudeLatitude,
}

/// Exact source coordinate after fixed-decimal parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoSourcePointFixed {
    pub x: i64,
    pub y: i64,
}

/// Exact affine coefficients from fixed source units to local millimetres.
/// Each output axis is a rational with this common positive denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoAffineProjectionMm {
    pub x_from_source_x_numerator: i64,
    pub x_from_source_y_numerator: i64,
    pub y_from_source_x_numerator: i64,
    pub y_from_source_y_numerator: i64,
    pub denominator: u64,
}

/// Provenance and measured/calibrated error envelope for the lossy projection
/// step. The digest addresses the complete parameter table used upstream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoProjectionProvenance {
    pub method_id: String,
    pub method_version: String,
    pub parameters_blake3: String,
    pub max_projection_error_micrometres: u64,
}

/// Versioned per-tile frame. `source_decimal_places` defines the exact integer
/// representation parsed from source decimal strings. The origin and affine
/// coefficients then map those integers to local millimetres without floats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoLocalFrameContract {
    pub version: String,
    pub frame_id: String,
    pub tile_id: String,
    pub source_crs: String,
    pub source_axis_domain: GeoSourceAxisDomain,
    pub source_decimal_places: u32,
    pub source_origin: GeoSourcePointFixed,
    pub affine: GeoAffineProjectionMm,
    pub projection: GeoProjectionProvenance,
    /// Conservative coordinate-domain bound after projection. This protects
    /// the arithmetic proof from accidentally admitting continental values.
    pub max_abs_coordinate_mm: i64,
}

/// Source coordinate bytes. Exponents and binary floats are deliberately not
/// part of the contract: fixed decimal text parses to one exact integer.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoSourcePointDecimal {
    pub x: String,
    pub y: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoSourcePolygon {
    /// Explicitly closed source ring.
    pub exterior: Vec<GeoSourcePointDecimal>,
    /// Explicitly closed source rings.
    #[serde(default)]
    pub holes: Vec<Vec<GeoSourcePointDecimal>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeoSourceGeometry {
    Point {
        coordinate: GeoSourcePointDecimal,
    },
    Polygon {
        exterior: Vec<GeoSourcePointDecimal>,
        #[serde(default)]
        holes: Vec<Vec<GeoSourcePointDecimal>>,
    },
    MultiPolygon {
        polygons: Vec<GeoSourcePolygon>,
    },
}

impl GeoSourceGeometry {
    fn raw_vertex_count(&self) -> Result<u64, GeoGeometryError> {
        let mut count = 0_u64;
        match self {
            Self::Point { .. } => count = 1,
            Self::Polygon { exterior, holes } => {
                count = add_vertex_count(count, exterior.len())?;
                for hole in holes {
                    count = add_vertex_count(count, hole.len())?;
                }
            }
            Self::MultiPolygon { polygons } => {
                for polygon in polygons {
                    count = add_vertex_count(count, polygon.exterior.len())?;
                    for hole in &polygon.holes {
                        count = add_vertex_count(count, hole.len())?;
                    }
                }
            }
        }
        Ok(count)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoBoundingBoxMm {
    pub min_x: i64,
    pub min_y: i64,
    pub max_x: i64,
    pub max_y: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoCanonicalRingMm {
    /// Canonical rings omit the repeated closing vertex. Exterior rings are
    /// counter-clockwise; holes are clockwise; the first vertex is the
    /// lexicographically smallest after orientation normalization.
    pub vertices: Vec<GeoPointMm>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoCanonicalPolygonMm {
    pub exterior: GeoCanonicalRingMm,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<GeoCanonicalRingMm>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeoCanonicalGeometryMm {
    Point {
        coordinate: GeoPointMm,
    },
    Polygon {
        polygon: GeoCanonicalPolygonMm,
    },
    MultiPolygon {
        polygons: Vec<GeoCanonicalPolygonMm>,
    },
}

/// Exact measurement of the affine-to-millimetre snap plus the projection
/// contract's separately declared error envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoQuantizationAudit {
    /// Exact maximum snap error is this numerator divided by
    /// `affine_denominator`, in millimetres.
    pub max_abs_snap_error_numerator_mm: u64,
    pub affine_denominator: u64,
    pub max_abs_snap_error_micrometres_ceiling: u64,
    pub projection_error_envelope_micrometres: u64,
    pub combined_error_envelope_micrometres: u64,
    /// Smallest nonzero axis extent in the canonical bbox, when one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_nonzero_bbox_extent_mm: Option<u64>,
    /// Conservative endpoint-distance error bound relative to that extent.
    /// This is an integer ceiling, not an empirical accuracy claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_distance_error_ppm_upper_bound: Option<u64>,
}

/// Geometry value carried by Geo features/evidence. CRS, frame, coordinate
/// encoding, bbox, and vertex count are explicit rather than ambient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoTypedGeometry {
    pub version: String,
    pub source_crs: String,
    pub local_frame_id: String,
    pub coordinate_unit: String,
    pub coordinate_scale: u32,
    pub vertex_count: u64,
    pub bbox: GeoBoundingBoxMm,
    pub quantization: GeoQuantizationAudit,
    pub geometry: GeoCanonicalGeometryMm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeoFeatureValue {
    Geometry { value: GeoTypedGeometry },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoGeometryFeatureInput {
    pub feature_id: String,
    pub source_crs: String,
    pub geometry: GeoSourceGeometry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoGeometryFeature {
    pub feature_id: String,
    pub value: GeoFeatureValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoGeometryTileRequest {
    pub version: String,
    pub frame: GeoLocalFrameContract,
    pub features: Vec<GeoGeometryFeatureInput>,
    pub max_vertices_per_geometry: u64,
    pub max_geometry_bytes_per_tile: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoGeometryTileArtifact {
    pub version: String,
    pub frame: GeoLocalFrameContract,
    pub features: Vec<GeoGeometryFeature>,
    pub total_canonical_vertices: u64,
    /// Sum of canonical serialized `GeoTypedGeometry` byte lengths. Feature
    /// metadata is excluded and decision geometry is never truncated.
    pub geometry_bytes: u64,
    pub max_vertices_per_geometry: u64,
    pub max_geometry_bytes_per_tile: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoGeometryBudgetLimit {
    MaxVerticesPerGeometry,
    MaxGeometryBytesPerTile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoGeometryBudgetEnforcement {
    RefuseBeforeMaterialization,
    RefuseBeforeOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoGeometryBudgetBreach {
    pub policy_id: String,
    pub limit: GeoGeometryBudgetLimit,
    pub enforcement: GeoGeometryBudgetEnforcement,
    pub observed: u64,
    pub configured: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoGeometryErrorCode {
    UnsupportedVersion,
    InvalidInput,
    InvalidFrame,
    InvalidCoordinate,
    NonFiniteCoordinate,
    SourcePrecisionExceeded,
    MixedCrs,
    AntimeridianCrossing,
    EmptyGeometry,
    UnclosedRing,
    TooFewVertices,
    DuplicateVertex,
    DegenerateRing,
    SelfIntersection,
    HoleOutsideExterior,
    PolygonIntersection,
    VertexBudgetExceeded,
    TileByteBudgetExceeded,
    ArithmeticOverflow,
    Serialization,
}

/// Typed refusal from geometry admission/materialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoGeometryError {
    pub code: GeoGeometryErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<GeoGeometryBudgetBreach>,
}

impl GeoGeometryError {
    fn new(
        code: GeoGeometryErrorCode,
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
            budget: None,
        }
    }

    fn overflow(context: &str) -> Self {
        Self::new(
            GeoGeometryErrorCode::ArithmeticOverflow,
            "Geo geometry materialization exceeded checked integer arithmetic",
            [("context", context)],
        )
    }

    fn budget(
        code: GeoGeometryErrorCode,
        message: impl Into<String>,
        breach: GeoGeometryBudgetBreach,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        let mut error = Self::new(code, message, detail);
        error.budget = Some(breach);
        error
    }
}

impl fmt::Display for GeoGeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoGeometryError {}

#[derive(Debug)]
struct ProjectionAccumulator {
    max_snap_error_numerator: u128,
}

impl ProjectionAccumulator {
    fn new() -> Self {
        Self {
            max_snap_error_numerator: 0,
        }
    }

    fn observe(&mut self, error_numerator: u128) {
        self.max_snap_error_numerator = self.max_snap_error_numerator.max(error_numerator);
    }
}

/// Materialize, normalize, sort, and budget one tile's typed geometry values.
/// Successful output is byte-deterministic under feature, ring-direction,
/// ring-start, hole, and multipolygon order permutations.
pub fn materialize_geometry_tile(
    request: &GeoGeometryTileRequest,
) -> Result<GeoGeometryTileArtifact, GeoGeometryError> {
    if request.version != CANON_GEO_GEOMETRY_REQUEST_VERSION {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::UnsupportedVersion,
            "Unsupported Geo geometry request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_GEOMETRY_REQUEST_VERSION),
            ],
        ));
    }
    validate_frame(&request.frame)?;
    if request.max_vertices_per_geometry == 0 || request.max_geometry_bytes_per_tile == 0 {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidInput,
            "Geo geometry budgets must be nonzero",
            [
                (
                    "max_vertices_per_geometry",
                    request.max_vertices_per_geometry.to_string(),
                ),
                (
                    "max_geometry_bytes_per_tile",
                    request.max_geometry_bytes_per_tile.to_string(),
                ),
            ],
        ));
    }

    let mut inputs = request.features.clone();
    inputs.sort_by(|left, right| left.feature_id.cmp(&right.feature_id));
    reject_duplicate_feature_ids(&inputs)?;

    let mut features = Vec::with_capacity(inputs.len());
    let mut geometry_bytes = 0_u64;
    let mut total_canonical_vertices = 0_u64;
    for input in inputs {
        validate_identifier("feature_id", &input.feature_id)?;
        if input.source_crs != request.frame.source_crs {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::MixedCrs,
                "A geometry feature CRS must match the tile frame source CRS",
                [
                    ("feature_id", input.feature_id.as_str()),
                    ("feature_crs", input.source_crs.as_str()),
                    ("frame_crs", request.frame.source_crs.as_str()),
                ],
            ));
        }
        let raw_vertex_count = input.geometry.raw_vertex_count()?;
        if raw_vertex_count > request.max_vertices_per_geometry {
            return Err(GeoGeometryError::budget(
                GeoGeometryErrorCode::VertexBudgetExceeded,
                "Geometry exceeds the declared per-value vertex budget",
                GeoGeometryBudgetBreach {
                    policy_id: "geometry.max_vertices_per_value".to_string(),
                    limit: GeoGeometryBudgetLimit::MaxVerticesPerGeometry,
                    enforcement: GeoGeometryBudgetEnforcement::RefuseBeforeMaterialization,
                    observed: raw_vertex_count,
                    configured: request.max_vertices_per_geometry,
                },
                [("feature_id", input.feature_id.as_str())],
            ));
        }

        let value = materialize_geometry(&request.frame, &input.source_crs, &input.geometry)?;
        let bytes = canonical_geometry_bytes(&value).map_err(|error| {
            GeoGeometryError::new(
                GeoGeometryErrorCode::Serialization,
                "Canonical geometry serialization failed",
                [
                    ("feature_id", input.feature_id.as_str()),
                    ("error", error.to_string().as_str()),
                ],
            )
        })?;
        let byte_count = u64::try_from(bytes.len())
            .map_err(|_| GeoGeometryError::overflow("canonical geometry byte count"))?;
        geometry_bytes = geometry_bytes
            .checked_add(byte_count)
            .ok_or_else(|| GeoGeometryError::overflow("tile geometry byte sum"))?;
        if geometry_bytes > request.max_geometry_bytes_per_tile {
            return Err(GeoGeometryError::budget(
                GeoGeometryErrorCode::TileByteBudgetExceeded,
                "Tile decision geometry exceeds the declared byte budget",
                GeoGeometryBudgetBreach {
                    policy_id: "geometry.max_bytes_per_tile".to_string(),
                    limit: GeoGeometryBudgetLimit::MaxGeometryBytesPerTile,
                    enforcement: GeoGeometryBudgetEnforcement::RefuseBeforeOutput,
                    observed: geometry_bytes,
                    configured: request.max_geometry_bytes_per_tile,
                },
                [("feature_id", input.feature_id.as_str())],
            ));
        }
        total_canonical_vertices = total_canonical_vertices
            .checked_add(value.vertex_count)
            .ok_or_else(|| GeoGeometryError::overflow("tile canonical vertex sum"))?;
        features.push(GeoGeometryFeature {
            feature_id: input.feature_id,
            value: GeoFeatureValue::Geometry { value },
        });
    }

    Ok(GeoGeometryTileArtifact {
        version: CANON_GEO_GEOMETRY_TILE_VERSION.to_string(),
        frame: request.frame.clone(),
        features,
        total_canonical_vertices,
        geometry_bytes,
        max_vertices_per_geometry: request.max_vertices_per_geometry,
        max_geometry_bytes_per_tile: request.max_geometry_bytes_per_tile,
    })
}

pub fn canonical_geometry_bytes(value: &GeoTypedGeometry) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(value)
}

pub fn canonical_geometry_tile_bytes(
    artifact: &GeoGeometryTileArtifact,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(artifact)
}

fn materialize_geometry(
    frame: &GeoLocalFrameContract,
    source_crs: &str,
    source: &GeoSourceGeometry,
) -> Result<GeoTypedGeometry, GeoGeometryError> {
    let mut accumulator = ProjectionAccumulator::new();
    let geometry = match source {
        GeoSourceGeometry::Point { coordinate } => GeoCanonicalGeometryMm::Point {
            coordinate: parse_and_project_point(frame, coordinate, &mut accumulator)?,
        },
        GeoSourceGeometry::Polygon { exterior, holes } => GeoCanonicalGeometryMm::Polygon {
            polygon: materialize_polygon(frame, exterior, holes, &mut accumulator)?,
        },
        GeoSourceGeometry::MultiPolygon { polygons } => {
            if polygons.is_empty() {
                return Err(GeoGeometryError::new(
                    GeoGeometryErrorCode::EmptyGeometry,
                    "A multipolygon must contain at least one polygon",
                    std::iter::empty::<(&str, &str)>(),
                ));
            }
            let mut canonical = Vec::with_capacity(polygons.len());
            for polygon in polygons {
                canonical.push(materialize_polygon(
                    frame,
                    &polygon.exterior,
                    &polygon.holes,
                    &mut accumulator,
                )?);
            }
            validate_disjoint_polygons(frame, &canonical)?;
            canonical.sort();
            GeoCanonicalGeometryMm::MultiPolygon {
                polygons: canonical,
            }
        }
    };

    let points = geometry_points(&geometry);
    if points.is_empty() {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::EmptyGeometry,
            "Canonical geometry cannot be empty",
            std::iter::empty::<(&str, &str)>(),
        ));
    }
    let bbox = bounding_box(&points);
    let vertex_count = u64::try_from(points.len())
        .map_err(|_| GeoGeometryError::overflow("canonical geometry vertex count"))?;
    let max_snap_error_numerator = u64::try_from(accumulator.max_snap_error_numerator)
        .map_err(|_| GeoGeometryError::overflow("snap error numerator"))?;
    let snap_micrometres = ceil_ratio_u128(
        accumulator
            .max_snap_error_numerator
            .checked_mul(1_000)
            .ok_or_else(|| GeoGeometryError::overflow("snap error micrometre numerator"))?,
        u128::from(frame.affine.denominator),
        "snap error micrometre ceiling",
    )?;
    let snap_micrometres = u64::try_from(snap_micrometres)
        .map_err(|_| GeoGeometryError::overflow("snap error micrometre conversion"))?;
    let combined_error_envelope_micrometres = frame
        .projection
        .max_projection_error_micrometres
        .checked_add(snap_micrometres)
        .ok_or_else(|| GeoGeometryError::overflow("combined geometry error envelope"))?;
    let minimum_nonzero_bbox_extent_mm = minimum_nonzero_bbox_extent(bbox)?;
    let endpoint_distance_error_ppm_upper_bound = minimum_nonzero_bbox_extent_mm
        .map(|extent| endpoint_error_ppm(combined_error_envelope_micrometres, extent))
        .transpose()?;

    Ok(GeoTypedGeometry {
        version: CANON_GEO_GEOMETRY_VALUE_VERSION.to_string(),
        source_crs: source_crs.to_string(),
        local_frame_id: frame.frame_id.clone(),
        coordinate_unit: "millimetre".to_string(),
        coordinate_scale: 1,
        vertex_count,
        bbox,
        quantization: GeoQuantizationAudit {
            max_abs_snap_error_numerator_mm: max_snap_error_numerator,
            affine_denominator: frame.affine.denominator,
            max_abs_snap_error_micrometres_ceiling: snap_micrometres,
            projection_error_envelope_micrometres: frame
                .projection
                .max_projection_error_micrometres,
            combined_error_envelope_micrometres,
            minimum_nonzero_bbox_extent_mm,
            endpoint_distance_error_ppm_upper_bound,
        },
        geometry,
    })
}

fn materialize_polygon(
    frame: &GeoLocalFrameContract,
    exterior: &[GeoSourcePointDecimal],
    holes: &[Vec<GeoSourcePointDecimal>],
    accumulator: &mut ProjectionAccumulator,
) -> Result<GeoCanonicalPolygonMm, GeoGeometryError> {
    let exterior = materialize_ring(frame, exterior, RingRole::Exterior, accumulator)?;
    let mut canonical_holes = Vec::with_capacity(holes.len());
    for hole in holes {
        canonical_holes.push(materialize_ring(frame, hole, RingRole::Hole, accumulator)?);
    }
    validate_holes(frame, &exterior, &canonical_holes)?;
    canonical_holes.sort();
    Ok(GeoCanonicalPolygonMm {
        exterior,
        holes: canonical_holes,
    })
}

#[derive(Debug, Clone, Copy)]
enum RingRole {
    Exterior,
    Hole,
}

fn materialize_ring(
    frame: &GeoLocalFrameContract,
    source: &[GeoSourcePointDecimal],
    role: RingRole,
    accumulator: &mut ProjectionAccumulator,
) -> Result<GeoCanonicalRingMm, GeoGeometryError> {
    if source.is_empty() {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::EmptyGeometry,
            "A polygon ring cannot be empty",
            std::iter::empty::<(&str, &str)>(),
        ));
    }
    let mut fixed = Vec::with_capacity(source.len());
    for point in source {
        fixed.push(parse_source_point(frame, point)?);
    }
    validate_antimeridian(frame, &fixed)?;
    if fixed.first() != fixed.last() {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::UnclosedRing,
            "A source ring must repeat its first coordinate as its final coordinate",
            std::iter::empty::<(&str, &str)>(),
        ));
    }

    fixed.pop();
    collapse_adjacent_duplicates(&mut fixed);
    while fixed.len() > 1 && fixed.last() == fixed.first() {
        fixed.pop();
    }
    if fixed.len() < 3 {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::TooFewVertices,
            "A normalized polygon ring needs at least three vertices",
            [("vertex_count", fixed.len().to_string())],
        ));
    }

    let mut projected = Vec::with_capacity(fixed.len());
    for point in fixed {
        projected.push(project_fixed_point(frame, point, accumulator)?);
    }
    collapse_adjacent_duplicates(&mut projected);
    while projected.len() > 1 && projected.last() == projected.first() {
        projected.pop();
    }
    let mut closed = projected.clone();
    if let Some(first) = projected.first().copied() {
        closed.push(first);
    }
    GeoLinearRingMm::new(frame.frame_id.clone(), closed).map_err(map_predicate_error)?;

    let signed_double_area = signed_double_area(&projected)?;
    let needs_reverse = match role {
        RingRole::Exterior => signed_double_area < 0,
        RingRole::Hole => signed_double_area > 0,
    };
    if needs_reverse {
        projected.reverse();
    }
    rotate_lexicographically(&mut projected);
    Ok(GeoCanonicalRingMm {
        vertices: projected,
    })
}

fn parse_and_project_point(
    frame: &GeoLocalFrameContract,
    source: &GeoSourcePointDecimal,
    accumulator: &mut ProjectionAccumulator,
) -> Result<GeoPointMm, GeoGeometryError> {
    let fixed = parse_source_point(frame, source)?;
    validate_geographic_point(frame, fixed)?;
    project_fixed_point(frame, fixed, accumulator)
}

fn parse_source_point(
    frame: &GeoLocalFrameContract,
    source: &GeoSourcePointDecimal,
) -> Result<GeoSourcePointFixed, GeoGeometryError> {
    let point = GeoSourcePointFixed {
        x: parse_fixed_decimal("x", &source.x, frame.source_decimal_places)?,
        y: parse_fixed_decimal("y", &source.y, frame.source_decimal_places)?,
    };
    validate_geographic_point(frame, point)?;
    Ok(point)
}

fn parse_fixed_decimal(
    axis: &str,
    value: &str,
    decimal_places: u32,
) -> Result<i64, GeoGeometryError> {
    if value.trim() != value || value.is_empty() {
        return Err(invalid_coordinate(
            axis,
            value,
            "unpadded fixed decimal required",
        ));
    }
    let lowercase = value.to_ascii_lowercase();
    if matches!(
        lowercase.as_str(),
        "nan" | "+nan" | "-nan" | "inf" | "+inf" | "-inf" | "infinity" | "+infinity" | "-infinity"
    ) {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::NonFiniteCoordinate,
            "Non-finite source coordinates are not admissible geometry",
            [("axis", axis), ("value", value)],
        ));
    }

    let (negative, unsigned) = match value.as_bytes().first() {
        Some(b'-') => (true, &value[1..]),
        Some(b'+') => (false, &value[1..]),
        _ => (false, value),
    };
    if unsigned.is_empty() || unsigned.contains(['e', 'E']) {
        return Err(invalid_coordinate(
            axis,
            value,
            "exponents are not permitted",
        ));
    }
    let mut parts = unsigned.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid_coordinate(
            axis,
            value,
            "invalid fixed decimal syntax",
        ));
    }

    let permitted = usize::try_from(decimal_places)
        .map_err(|_| GeoGeometryError::overflow("source decimal scale conversion"))?;
    if fraction.len() > permitted && fraction[permitted..].bytes().any(|byte| byte != b'0') {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::SourcePrecisionExceeded,
            "Source coordinate has nonzero precision beyond the frame contract",
            [
                ("axis", axis),
                ("value", value),
                ("decimal_places", decimal_places.to_string().as_str()),
            ],
        ));
    }

    let scale = pow10_i128(decimal_places)?;
    let whole = parse_digits_i128(whole)
        .ok_or_else(|| invalid_coordinate(axis, value, "coordinate magnitude overflow"))?;
    let retained_fraction = &fraction[..fraction.len().min(permitted)];
    let fraction_value = if retained_fraction.is_empty() {
        0
    } else {
        parse_digits_i128(retained_fraction)
            .ok_or_else(|| invalid_coordinate(axis, value, "coordinate fraction overflow"))?
    };
    let padding = decimal_places
        .checked_sub(u32::try_from(retained_fraction.len()).map_err(|_| {
            GeoGeometryError::overflow("source coordinate fraction length conversion")
        })?)
        .ok_or_else(|| GeoGeometryError::overflow("source coordinate padding"))?;
    let padded_fraction = fraction_value
        .checked_mul(pow10_i128(padding)?)
        .ok_or_else(|| GeoGeometryError::overflow("source coordinate fraction scaling"))?;
    let magnitude = whole
        .checked_mul(scale)
        .and_then(|scaled| scaled.checked_add(padded_fraction))
        .ok_or_else(|| GeoGeometryError::overflow("source coordinate scaling"))?;
    let signed = if negative {
        magnitude
            .checked_neg()
            .ok_or_else(|| GeoGeometryError::overflow("source coordinate sign"))?
    } else {
        magnitude
    };
    i64::try_from(signed).map_err(|_| invalid_coordinate(axis, value, "coordinate exceeds i64"))
}

fn invalid_coordinate(axis: &str, value: &str, reason: &str) -> GeoGeometryError {
    GeoGeometryError::new(
        GeoGeometryErrorCode::InvalidCoordinate,
        "Source coordinate is not a canonical fixed decimal",
        [("axis", axis), ("value", value), ("reason", reason)],
    )
}

fn project_fixed_point(
    frame: &GeoLocalFrameContract,
    point: GeoSourcePointFixed,
    accumulator: &mut ProjectionAccumulator,
) -> Result<GeoPointMm, GeoGeometryError> {
    let dx = i128::from(point.x) - i128::from(frame.source_origin.x);
    let dy = i128::from(point.y) - i128::from(frame.source_origin.y);
    let x_numerator = affine_axis_numerator(
        dx,
        dy,
        frame.affine.x_from_source_x_numerator,
        frame.affine.x_from_source_y_numerator,
        "projected x",
    )?;
    let y_numerator = affine_axis_numerator(
        dx,
        dy,
        frame.affine.y_from_source_x_numerator,
        frame.affine.y_from_source_y_numerator,
        "projected y",
    )?;
    let (x, x_error) = round_rational_ties_even(x_numerator, frame.affine.denominator)?;
    let (y, y_error) = round_rational_ties_even(y_numerator, frame.affine.denominator)?;
    accumulator.observe(x_error);
    accumulator.observe(y_error);
    let max_abs_coordinate_mm = frame.max_abs_coordinate_mm.unsigned_abs();
    if x.unsigned_abs() > max_abs_coordinate_mm || y.unsigned_abs() > max_abs_coordinate_mm {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidFrame,
            "Projected coordinate exceeds the frame's declared local bound",
            [
                ("x_mm", x.to_string()),
                ("y_mm", y.to_string()),
                (
                    "max_abs_coordinate_mm",
                    frame.max_abs_coordinate_mm.to_string(),
                ),
            ],
        ));
    }
    Ok(GeoPointMm::new(x, y))
}

fn affine_axis_numerator(
    dx: i128,
    dy: i128,
    x_coefficient: i64,
    y_coefficient: i64,
    context: &str,
) -> Result<i128, GeoGeometryError> {
    let x = dx
        .checked_mul(i128::from(x_coefficient))
        .ok_or_else(|| GeoGeometryError::overflow(context))?;
    let y = dy
        .checked_mul(i128::from(y_coefficient))
        .ok_or_else(|| GeoGeometryError::overflow(context))?;
    x.checked_add(y)
        .ok_or_else(|| GeoGeometryError::overflow(context))
}

fn round_rational_ties_even(
    numerator: i128,
    denominator: u64,
) -> Result<(i64, u128), GeoGeometryError> {
    let denominator = i128::from(denominator);
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let abs_remainder = remainder.unsigned_abs();
    let twice_remainder = abs_remainder
        .checked_mul(2)
        .ok_or_else(|| GeoGeometryError::overflow("rounding remainder comparison"))?;
    let denominator_u128 = denominator.unsigned_abs();
    let increment = twice_remainder > denominator_u128
        || (twice_remainder == denominator_u128 && quotient.rem_euclid(2) != 0);
    let rounded = if increment {
        quotient
            .checked_add(if numerator.is_negative() { -1 } else { 1 })
            .ok_or_else(|| GeoGeometryError::overflow("rounded coordinate"))?
    } else {
        quotient
    };
    let error = numerator
        .checked_sub(
            rounded
                .checked_mul(denominator)
                .ok_or_else(|| GeoGeometryError::overflow("rounded coordinate product"))?,
        )
        .ok_or_else(|| GeoGeometryError::overflow("rounded coordinate error"))?
        .unsigned_abs();
    let rounded = i64::try_from(rounded)
        .map_err(|_| GeoGeometryError::overflow("rounded coordinate conversion"))?;
    Ok((rounded, error))
}

fn validate_frame(frame: &GeoLocalFrameContract) -> Result<(), GeoGeometryError> {
    if frame.version != CANON_GEO_LOCAL_FRAME_VERSION {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::UnsupportedVersion,
            "Unsupported Geo local-frame version",
            [
                ("actual", frame.version.as_str()),
                ("expected", CANON_GEO_LOCAL_FRAME_VERSION),
            ],
        ));
    }
    for (field, value) in [
        ("frame_id", frame.frame_id.as_str()),
        ("tile_id", frame.tile_id.as_str()),
        ("source_crs", frame.source_crs.as_str()),
        ("projection.method_id", frame.projection.method_id.as_str()),
        (
            "projection.method_version",
            frame.projection.method_version.as_str(),
        ),
    ] {
        validate_identifier(field, value)?;
    }
    if frame.source_decimal_places > MAX_SOURCE_DECIMAL_PLACES {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidFrame,
            "Source decimal scale exceeds the deterministic admission limit",
            [
                (
                    "source_decimal_places",
                    frame.source_decimal_places.to_string(),
                ),
                ("maximum", MAX_SOURCE_DECIMAL_PLACES.to_string()),
            ],
        ));
    }
    if frame.affine.denominator == 0 {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidFrame,
            "Affine projection denominator must be positive",
            std::iter::empty::<(&str, &str)>(),
        ));
    }
    let determinant = i128::from(frame.affine.x_from_source_x_numerator)
        .checked_mul(i128::from(frame.affine.y_from_source_y_numerator))
        .and_then(|left| {
            i128::from(frame.affine.x_from_source_y_numerator)
                .checked_mul(i128::from(frame.affine.y_from_source_x_numerator))
                .and_then(|right| left.checked_sub(right))
        })
        .ok_or_else(|| GeoGeometryError::overflow("affine determinant"))?;
    if determinant == 0 || frame.max_abs_coordinate_mm <= 0 {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidFrame,
            "Affine projection must be invertible and have a positive local bound",
            [
                ("determinant", determinant.to_string()),
                (
                    "max_abs_coordinate_mm",
                    frame.max_abs_coordinate_mm.to_string(),
                ),
            ],
        ));
    }
    validate_blake3(
        "projection.parameters_blake3",
        &frame.projection.parameters_blake3,
    )?;
    validate_geographic_point(frame, frame.source_origin)?;
    Ok(())
}

fn validate_identifier(field: &str, value: &str) -> Result<(), GeoGeometryError> {
    if value.is_empty() || value.trim() != value || value.len() > MAX_IDENTIFIER_BYTES {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidFrame,
            "Geo geometry identifiers must be nonempty, unpadded UTF-8 within the byte limit",
            [
                ("field", field),
                ("length", value.len().to_string().as_str()),
            ],
        ));
    }
    Ok(())
}

fn validate_blake3(field: &str, value: &str) -> Result<(), GeoGeometryError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidFrame,
            "Geo projection parameter digest must be lowercase BLAKE3 hex",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_geographic_point(
    frame: &GeoLocalFrameContract,
    point: GeoSourcePointFixed,
) -> Result<(), GeoGeometryError> {
    if frame.source_axis_domain == GeoSourceAxisDomain::Planar {
        return Ok(());
    }
    let scale = pow10_i128(frame.source_decimal_places)?;
    let longitude_limit = 180_i128
        .checked_mul(scale)
        .ok_or_else(|| GeoGeometryError::overflow("longitude domain"))?;
    let latitude_limit = 90_i128
        .checked_mul(scale)
        .ok_or_else(|| GeoGeometryError::overflow("latitude domain"))?;
    if i128::from(point.x).unsigned_abs() > longitude_limit.unsigned_abs()
        || i128::from(point.y).unsigned_abs() > latitude_limit.unsigned_abs()
    {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidCoordinate,
            "Geographic coordinate is outside longitude/latitude bounds",
            [
                ("x_fixed", point.x.to_string()),
                ("y_fixed", point.y.to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_antimeridian(
    frame: &GeoLocalFrameContract,
    ring: &[GeoSourcePointFixed],
) -> Result<(), GeoGeometryError> {
    if frame.source_axis_domain == GeoSourceAxisDomain::Planar || ring.len() < 2 {
        return Ok(());
    }
    let scale = pow10_i128(frame.source_decimal_places)?;
    let half_period = 180_i128
        .checked_mul(scale)
        .ok_or_else(|| GeoGeometryError::overflow("antimeridian half-period"))?;
    for edge in ring.windows(2) {
        let delta = (i128::from(edge[1].x) - i128::from(edge[0].x)).unsigned_abs();
        if delta > half_period.unsigned_abs() {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::AntimeridianCrossing,
                "Geographic rings crossing the antimeridian require a declared split upstream",
                [
                    ("left_x_fixed", edge[0].x.to_string()),
                    ("right_x_fixed", edge[1].x.to_string()),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_holes(
    frame: &GeoLocalFrameContract,
    exterior: &GeoCanonicalRingMm,
    holes: &[GeoCanonicalRingMm],
) -> Result<(), GeoGeometryError> {
    let exterior_predicate = predicate_ring(frame, exterior)?;
    for (index, hole) in holes.iter().enumerate() {
        if rings_intersect(exterior, hole)?
            || exterior_predicate
                .locate_point(&frame.frame_id, hole.vertices[0])
                .map_err(map_predicate_error)?
                != GeoPointLocation::Interior
        {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::HoleOutsideExterior,
                "Polygon holes must be strictly inside and disjoint from the exterior boundary",
                [("hole_index", index.to_string())],
            ));
        }
        for (other_index, other) in holes[..index].iter().enumerate() {
            if rings_intersect(hole, other)?
                || ring_contains_vertex(frame, hole, other)?
                || ring_contains_vertex(frame, other, hole)?
            {
                return Err(GeoGeometryError::new(
                    GeoGeometryErrorCode::PolygonIntersection,
                    "Polygon holes must be pairwise disjoint and non-nested",
                    [
                        ("left_hole_index", other_index.to_string()),
                        ("right_hole_index", index.to_string()),
                    ],
                ));
            }
        }
    }
    Ok(())
}

fn validate_disjoint_polygons(
    frame: &GeoLocalFrameContract,
    polygons: &[GeoCanonicalPolygonMm],
) -> Result<(), GeoGeometryError> {
    for left_index in 0..polygons.len() {
        for right_index in left_index + 1..polygons.len() {
            let left = &polygons[left_index];
            let right = &polygons[right_index];
            if polygon_boundaries_intersect(left, right)?
                || locate_in_polygon(frame, left, right.exterior.vertices[0])?
                    != GeoPointLocation::Exterior
                || locate_in_polygon(frame, right, left.exterior.vertices[0])?
                    != GeoPointLocation::Exterior
            {
                return Err(GeoGeometryError::new(
                    GeoGeometryErrorCode::PolygonIntersection,
                    "Multipolygon members must have disjoint interiors and boundaries",
                    [
                        ("left_polygon_index", left_index.to_string()),
                        ("right_polygon_index", right_index.to_string()),
                    ],
                ));
            }
        }
    }
    Ok(())
}

fn locate_in_polygon(
    frame: &GeoLocalFrameContract,
    polygon: &GeoCanonicalPolygonMm,
    point: GeoPointMm,
) -> Result<GeoPointLocation, GeoGeometryError> {
    let exterior = predicate_ring(frame, &polygon.exterior)?;
    let location = exterior
        .locate_point(&frame.frame_id, point)
        .map_err(map_predicate_error)?;
    if location != GeoPointLocation::Interior {
        return Ok(location);
    }
    for hole in &polygon.holes {
        let location = predicate_ring(frame, hole)?
            .locate_point(&frame.frame_id, point)
            .map_err(map_predicate_error)?;
        match location {
            GeoPointLocation::Boundary => return Ok(GeoPointLocation::Boundary),
            GeoPointLocation::Interior => return Ok(GeoPointLocation::Exterior),
            GeoPointLocation::Exterior => {}
        }
    }
    Ok(GeoPointLocation::Interior)
}

fn polygon_boundaries_intersect(
    left: &GeoCanonicalPolygonMm,
    right: &GeoCanonicalPolygonMm,
) -> Result<bool, GeoGeometryError> {
    let mut left_rings = Vec::with_capacity(left.holes.len() + 1);
    left_rings.push(&left.exterior);
    left_rings.extend(&left.holes);
    let mut right_rings = Vec::with_capacity(right.holes.len() + 1);
    right_rings.push(&right.exterior);
    right_rings.extend(&right.holes);
    for left_ring in left_rings {
        for right_ring in &right_rings {
            if rings_intersect(left_ring, right_ring)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn ring_contains_vertex(
    frame: &GeoLocalFrameContract,
    container: &GeoCanonicalRingMm,
    candidate: &GeoCanonicalRingMm,
) -> Result<bool, GeoGeometryError> {
    Ok(predicate_ring(frame, container)?
        .locate_point(&frame.frame_id, candidate.vertices[0])
        .map_err(map_predicate_error)?
        != GeoPointLocation::Exterior)
}

fn predicate_ring(
    frame: &GeoLocalFrameContract,
    ring: &GeoCanonicalRingMm,
) -> Result<GeoLinearRingMm, GeoGeometryError> {
    let mut closed = ring.vertices.clone();
    closed.push(ring.vertices[0]);
    GeoLinearRingMm::new(frame.frame_id.clone(), closed).map_err(map_predicate_error)
}

fn rings_intersect(
    left: &GeoCanonicalRingMm,
    right: &GeoCanonicalRingMm,
) -> Result<bool, GeoGeometryError> {
    for left_index in 0..left.vertices.len() {
        let left_start = left.vertices[left_index];
        let left_end = left.vertices[(left_index + 1) % left.vertices.len()];
        for right_index in 0..right.vertices.len() {
            let right_start = right.vertices[right_index];
            let right_end = right.vertices[(right_index + 1) % right.vertices.len()];
            if exact_segment_intersection(left_start, left_end, right_start, right_end)
                .map_err(map_predicate_error)?
                != GeoSegmentIntersection::Disjoint
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn map_predicate_error(error: GeoPredicateError) -> GeoGeometryError {
    let code = match error.code {
        GeoPredicateErrorCode::EmptyGeometry => GeoGeometryErrorCode::EmptyGeometry,
        GeoPredicateErrorCode::InvalidFrame | GeoPredicateErrorCode::MixedFrame => {
            GeoGeometryErrorCode::InvalidFrame
        }
        GeoPredicateErrorCode::UnclosedRing => GeoGeometryErrorCode::UnclosedRing,
        GeoPredicateErrorCode::TooFewVertices => GeoGeometryErrorCode::TooFewVertices,
        GeoPredicateErrorCode::DuplicateVertex => GeoGeometryErrorCode::DuplicateVertex,
        GeoPredicateErrorCode::DegenerateRing => GeoGeometryErrorCode::DegenerateRing,
        GeoPredicateErrorCode::SelfIntersection => GeoGeometryErrorCode::SelfIntersection,
        GeoPredicateErrorCode::ArithmeticOverflow => GeoGeometryErrorCode::ArithmeticOverflow,
    };
    GeoGeometryError {
        code,
        message: error.message,
        detail: error.detail,
        budget: None,
    }
}

fn signed_double_area(vertices: &[GeoPointMm]) -> Result<i128, GeoGeometryError> {
    let mut sum = 0_i128;
    for index in 0..vertices.len() {
        let start = vertices[index];
        let end = vertices[(index + 1) % vertices.len()];
        let term = i128::from(start.x)
            .checked_mul(i128::from(end.y))
            .and_then(|forward| {
                i128::from(start.y)
                    .checked_mul(i128::from(end.x))
                    .and_then(|reverse| forward.checked_sub(reverse))
            })
            .ok_or_else(|| GeoGeometryError::overflow("canonical ring shoelace term"))?;
        sum = sum
            .checked_add(term)
            .ok_or_else(|| GeoGeometryError::overflow("canonical ring shoelace sum"))?;
    }
    Ok(sum)
}

fn rotate_lexicographically(vertices: &mut [GeoPointMm]) {
    if let Some((index, _)) = vertices.iter().enumerate().min_by_key(|(_, point)| *point) {
        vertices.rotate_left(index);
    }
}

fn collapse_adjacent_duplicates<T: PartialEq>(values: &mut Vec<T>) {
    values.dedup();
}

fn geometry_points(geometry: &GeoCanonicalGeometryMm) -> Vec<GeoPointMm> {
    let mut points = Vec::new();
    match geometry {
        GeoCanonicalGeometryMm::Point { coordinate } => points.push(*coordinate),
        GeoCanonicalGeometryMm::Polygon { polygon } => append_polygon_points(&mut points, polygon),
        GeoCanonicalGeometryMm::MultiPolygon { polygons } => {
            for polygon in polygons {
                append_polygon_points(&mut points, polygon);
            }
        }
    }
    points
}

fn append_polygon_points(points: &mut Vec<GeoPointMm>, polygon: &GeoCanonicalPolygonMm) {
    points.extend_from_slice(&polygon.exterior.vertices);
    for hole in &polygon.holes {
        points.extend_from_slice(&hole.vertices);
    }
}

fn bounding_box(points: &[GeoPointMm]) -> GeoBoundingBoxMm {
    let first = points[0];
    points.iter().skip(1).fold(
        GeoBoundingBoxMm {
            min_x: first.x,
            min_y: first.y,
            max_x: first.x,
            max_y: first.y,
        },
        |mut bbox, point| {
            bbox.min_x = bbox.min_x.min(point.x);
            bbox.min_y = bbox.min_y.min(point.y);
            bbox.max_x = bbox.max_x.max(point.x);
            bbox.max_y = bbox.max_y.max(point.y);
            bbox
        },
    )
}

fn minimum_nonzero_bbox_extent(bbox: GeoBoundingBoxMm) -> Result<Option<u64>, GeoGeometryError> {
    let width = i128::from(bbox.max_x) - i128::from(bbox.min_x);
    let height = i128::from(bbox.max_y) - i128::from(bbox.min_y);
    let mut extents = [width, height]
        .into_iter()
        .filter(|extent| *extent > 0)
        .collect::<Vec<_>>();
    extents.sort();
    extents
        .first()
        .copied()
        .map(|extent| {
            u64::try_from(extent).map_err(|_| GeoGeometryError::overflow("bbox extent conversion"))
        })
        .transpose()
}

fn endpoint_error_ppm(error_micrometres: u64, extent_mm: u64) -> Result<u64, GeoGeometryError> {
    // Moving both endpoints by the 2-D coordinate envelope changes distance
    // by < 2*sqrt(2)*e; 3e is a simple conservative integer upper bound.
    let numerator = u128::from(error_micrometres)
        .checked_mul(3)
        .and_then(|value| value.checked_mul(1_000_000))
        .ok_or_else(|| GeoGeometryError::overflow("endpoint error ppm numerator"))?;
    let denominator = u128::from(extent_mm)
        .checked_mul(1_000)
        .ok_or_else(|| GeoGeometryError::overflow("endpoint error ppm denominator"))?;
    u64::try_from(ceil_ratio_u128(
        numerator,
        denominator,
        "endpoint error ppm",
    )?)
    .map_err(|_| GeoGeometryError::overflow("endpoint error ppm conversion"))
}

fn ceil_ratio_u128(
    numerator: u128,
    denominator: u128,
    context: &str,
) -> Result<u128, GeoGeometryError> {
    if denominator == 0 {
        return Err(GeoGeometryError::overflow(context));
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    quotient
        .checked_add(u128::from(remainder != 0))
        .ok_or_else(|| GeoGeometryError::overflow(context))
}

fn reject_duplicate_feature_ids(
    features: &[GeoGeometryFeatureInput],
) -> Result<(), GeoGeometryError> {
    let mut seen = BTreeSet::new();
    for feature in features {
        if !seen.insert(feature.feature_id.as_str()) {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidInput,
                "Geometry tile request repeats a feature id",
                [("feature_id", feature.feature_id.as_str())],
            ));
        }
    }
    Ok(())
}

fn add_vertex_count(current: u64, additional: usize) -> Result<u64, GeoGeometryError> {
    current
        .checked_add(
            u64::try_from(additional)
                .map_err(|_| GeoGeometryError::overflow("raw vertex count conversion"))?,
        )
        .ok_or_else(|| GeoGeometryError::overflow("raw vertex count"))
}

fn pow10_i128(exponent: u32) -> Result<i128, GeoGeometryError> {
    10_i128
        .checked_pow(exponent)
        .ok_or_else(|| GeoGeometryError::overflow("decimal scale"))
}

fn parse_digits_i128(value: &str) -> Option<i128> {
    value.bytes().try_fold(0_i128, |accumulator, byte| {
        accumulator
            .checked_mul(10)
            .and_then(|value| value.checked_add(i128::from(byte - b'0')))
    })
}
