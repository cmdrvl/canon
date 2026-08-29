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
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_GEOMETRY_REQUEST_VERSION: &str = "canon_geo_geometry_request.v0";
pub const CANON_GEO_GEOMETRY_VALUE_VERSION: &str = "canon_geo_geometry_value.v0";
pub const CANON_GEO_GEOMETRY_TILE_VERSION: &str = "canon_geo_geometry_tile.v0";
pub const CANON_GEO_LOCAL_FRAME_VERSION: &str = "canon_geo_local_frame.v0";
pub const CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION: &str = "canon_geo_warehouse_geometry_rows.v0";
pub const CANON_GEO_WAREHOUSE_GEOMETRY_VERSION: &str = "canon_geo_warehouse_geometry.v0";

const CANON_GEO_PLANAR_FRAME_METHOD_ID: &str = "canon:planar-source-affine";
const CANON_GEO_PLANAR_FRAME_METHOD_VERSION: &str = "v0";
const ISO_WKB_2D_BASE64_ENCODING: &str = "iso-wkb-2d-base64";

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

/// Exact conversion from one source coordinate unit to millimetres. For
/// EPSG:2263 US survey feet this is 1_200_000 / 3_937 mm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoExactSourceUnitMm {
    pub unit_id: String,
    pub numerator: u64,
    pub denominator: u64,
}

/// One release-pinned row from a warehouse source-geometry plane. The base64
/// bytes are decoded and their SHA-256 is recomputed before any coordinate is
/// admitted. Transform ids link the source and WGS84 sibling planes; the
/// source-plane local frame does not reapply that WGS84 operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoWarehouseGeometryRow {
    pub feature_id: String,
    pub source_record_id: String,
    pub source_dataset: String,
    pub source_release: String,
    pub source_release_date: String,
    pub source_geometry_contract_version: String,
    pub source_archive_sha256: String,
    pub source_crs: String,
    pub source_srid: u32,
    pub source_geom_wkb_base64: String,
    pub source_geom_wkb_sha256: String,
    pub transform_execution_id: String,
    pub transform_definition_id: String,
}

/// Offline warehouse-row materialization request. `source_decimal_places`
/// declares the first Canon quantization boundary. The row-level receipt
/// measures WKB-f64 -> fixed-decimal loss separately from local-mm snapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoWarehouseGeometryRowsRequest {
    pub version: String,
    pub tile_id: String,
    pub frame_id: String,
    pub source_crs: String,
    pub source_srid: u32,
    pub source_decimal_places: u32,
    /// Stable, versioned frame anchor. It must be reused when later evidence
    /// accretes into the same tile; deriving it from the current row set would
    /// rewrite every prior local coordinate.
    pub source_origin: GeoSourcePointDecimal,
    pub source_unit_to_millimetres: GeoExactSourceUnitMm,
    pub rows: Vec<GeoWarehouseGeometryRow>,
    pub max_abs_coordinate_mm: i64,
    pub max_vertices_per_geometry: u64,
    pub max_geometry_bytes_per_tile: u64,
}

/// Exact fixed-decimal quantization error. The magnitude is
/// `numerator / denominator` of one fixed-decimal source unit; one such unit
/// is `10^-source_decimal_places` source units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoSourceQuantizationFraction {
    pub numerator: u64,
    pub denominator: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoWarehouseGeometryRowReceipt {
    pub feature_id: String,
    pub source_record_id: String,
    pub source_geom_wkb_sha256: String,
    pub decoded_vertex_count: u64,
    pub max_abs_source_quantization_error_fixed: GeoSourceQuantizationFraction,
    pub max_abs_source_quantization_error_micrometres_ceiling: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoWarehouseGeometrySourceReceipt {
    pub source_encoding: String,
    pub source_dataset: String,
    pub source_release: String,
    pub source_release_date: String,
    pub source_geometry_contract_version: String,
    pub source_archive_sha256: String,
    pub source_crs: String,
    pub source_srid: u32,
    pub source_decimal_places: u32,
    pub source_bounds_fixed: GeoSourceBoundsFixed,
    pub source_unit_to_millimetres: GeoExactSourceUnitMm,
    pub transform_execution_id: String,
    pub transform_definition_id: String,
    pub max_abs_source_quantization_error_fixed: GeoSourceQuantizationFraction,
    pub max_abs_source_quantization_error_micrometres_ceiling: u64,
    pub rows: Vec<GeoWarehouseGeometryRowReceipt>,
}

/// Source-plane geometry artifact. Error quantities stay on separate planes:
/// `source_receipt` reports WKB-to-decimal admission loss, while each typed
/// geometry reports affine-to-millimetre snapping. The local affine itself is
/// an exact translation and unit conversion and therefore declares zero
/// projection error; WGS84 transform disagreement is not folded into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoWarehouseGeometryArtifact {
    pub version: String,
    pub source_receipt: GeoWarehouseGeometrySourceReceipt,
    pub geometry_tile: GeoGeometryTileArtifact,
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
    InvalidSourceDigest,
    InvalidSourceEncoding,
    MalformedWkb,
    UnsupportedGeometryType,
    MixedSourceExecution,
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

pub fn canonical_warehouse_geometry_bytes(
    artifact: &GeoWarehouseGeometryArtifact,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(artifact)
}

#[derive(Debug)]
struct DecodedWarehouseGeometry {
    row: GeoWarehouseGeometryRow,
    geometry: GeoSourceGeometry,
    vertex_count: u64,
    max_error: GeoSourceQuantizationFraction,
    bounds: GeoSourceBoundsFixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoSourceBoundsFixed {
    pub min_x: i64,
    pub min_y: i64,
    pub max_x: i64,
    pub max_y: i64,
}

impl GeoSourceBoundsFixed {
    fn include(&mut self, other: Self) {
        self.min_x = self.min_x.min(other.min_x);
        self.min_y = self.min_y.min(other.min_y);
        self.max_x = self.max_x.max(other.max_x);
        self.max_y = self.max_y.max(other.max_y);
    }
}

/// Decode release-pinned ISO WKB rows, verify their source hashes, construct
/// one exact source-plane affine frame, then delegate topology normalization
/// and tile budgeting to the canonical geometry materializer.
pub fn materialize_warehouse_geometry(
    request: &GeoWarehouseGeometryRowsRequest,
) -> Result<GeoWarehouseGeometryArtifact, GeoGeometryError> {
    validate_warehouse_geometry_request(request)?;

    let mut rows = request.rows.clone();
    rows.sort_by(|left, right| {
        left.feature_id
            .cmp(&right.feature_id)
            .then_with(|| left.source_record_id.cmp(&right.source_record_id))
    });
    reject_duplicate_warehouse_rows(&rows)?;
    let first = rows.first().cloned().ok_or_else(|| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidInput,
            "Warehouse geometry request must contain at least one row",
            std::iter::empty::<(&str, &str)>(),
        )
    })?;
    validate_homogeneous_source_rows(request, &first, &rows)?;

    let mut decoded = Vec::with_capacity(rows.len());
    let mut global_bounds: Option<GeoSourceBoundsFixed> = None;
    let mut global_error = GeoSourceQuantizationFraction {
        numerator: 0,
        denominator: 1,
    };
    for row in rows {
        let value = decode_warehouse_geometry_row(
            row,
            request.source_decimal_places,
            request.max_vertices_per_geometry,
        )?;
        if let Some(bounds) = &mut global_bounds {
            bounds.include(value.bounds);
        } else {
            global_bounds = Some(value.bounds);
        }
        global_error = max_quantization_fraction(global_error, value.max_error)?;
        decoded.push(value);
    }
    let bounds = global_bounds.ok_or_else(|| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::EmptyGeometry,
            "Decoded warehouse geometry did not contain coordinates",
            std::iter::empty::<(&str, &str)>(),
        )
    })?;

    let affine = exact_source_unit_affine(
        &request.source_unit_to_millimetres,
        request.source_decimal_places,
    )?;
    let source_origin = GeoSourcePointFixed {
        x: parse_fixed_decimal(
            "source_origin.x",
            &request.source_origin.x,
            request.source_decimal_places,
        )?,
        y: parse_fixed_decimal(
            "source_origin.y",
            &request.source_origin.y,
            request.source_decimal_places,
        )?,
    };
    let parameters = serde_json::to_vec(&(
        (
            CANON_GEO_PLANAR_FRAME_METHOD_ID,
            CANON_GEO_PLANAR_FRAME_METHOD_VERSION,
        ),
        (
            &request.tile_id,
            &request.frame_id,
            &request.source_crs,
            request.source_srid,
            request.source_decimal_places,
            &request.source_unit_to_millimetres,
        ),
        source_origin,
    ))
    .map_err(|error| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::Serialization,
            "Local-frame parameter serialization failed",
            [("error", error.to_string())],
        )
    })?;
    let frame = GeoLocalFrameContract {
        version: CANON_GEO_LOCAL_FRAME_VERSION.to_string(),
        frame_id: request.frame_id.clone(),
        tile_id: request.tile_id.clone(),
        source_crs: request.source_crs.clone(),
        source_axis_domain: GeoSourceAxisDomain::Planar,
        source_decimal_places: request.source_decimal_places,
        source_origin,
        affine,
        projection: GeoProjectionProvenance {
            method_id: CANON_GEO_PLANAR_FRAME_METHOD_ID.to_string(),
            method_version: CANON_GEO_PLANAR_FRAME_METHOD_VERSION.to_string(),
            parameters_blake3: blake3::hash(&parameters).to_hex().to_string(),
            max_projection_error_micrometres: 0,
        },
        max_abs_coordinate_mm: request.max_abs_coordinate_mm,
    };
    let geometry_request = GeoGeometryTileRequest {
        version: CANON_GEO_GEOMETRY_REQUEST_VERSION.to_string(),
        frame,
        features: decoded
            .iter()
            .map(|value| GeoGeometryFeatureInput {
                feature_id: value.row.feature_id.clone(),
                source_crs: value.row.source_crs.clone(),
                geometry: value.geometry.clone(),
            })
            .collect(),
        max_vertices_per_geometry: request.max_vertices_per_geometry,
        max_geometry_bytes_per_tile: request.max_geometry_bytes_per_tile,
    };
    let geometry_tile = materialize_geometry_tile(&geometry_request)?;

    let mut row_receipts = Vec::with_capacity(decoded.len());
    for value in decoded {
        row_receipts.push(GeoWarehouseGeometryRowReceipt {
            feature_id: value.row.feature_id,
            source_record_id: value.row.source_record_id,
            source_geom_wkb_sha256: value.row.source_geom_wkb_sha256,
            decoded_vertex_count: value.vertex_count,
            max_abs_source_quantization_error_fixed: value.max_error,
            max_abs_source_quantization_error_micrometres_ceiling:
                source_error_micrometres_ceiling(
                    value.max_error,
                    request.source_decimal_places,
                    &request.source_unit_to_millimetres,
                )?,
        });
    }
    let source_receipt = GeoWarehouseGeometrySourceReceipt {
        source_encoding: ISO_WKB_2D_BASE64_ENCODING.to_string(),
        source_dataset: first.source_dataset.clone(),
        source_release: first.source_release.clone(),
        source_release_date: first.source_release_date.clone(),
        source_geometry_contract_version: first.source_geometry_contract_version.clone(),
        source_archive_sha256: first.source_archive_sha256.clone(),
        source_crs: request.source_crs.clone(),
        source_srid: request.source_srid,
        source_decimal_places: request.source_decimal_places,
        source_bounds_fixed: bounds,
        source_unit_to_millimetres: request.source_unit_to_millimetres.clone(),
        transform_execution_id: first.transform_execution_id.clone(),
        transform_definition_id: first.transform_definition_id.clone(),
        max_abs_source_quantization_error_fixed: global_error,
        max_abs_source_quantization_error_micrometres_ceiling: source_error_micrometres_ceiling(
            global_error,
            request.source_decimal_places,
            &request.source_unit_to_millimetres,
        )?,
        rows: row_receipts,
    };
    Ok(GeoWarehouseGeometryArtifact {
        version: CANON_GEO_WAREHOUSE_GEOMETRY_VERSION.to_string(),
        source_receipt,
        geometry_tile,
    })
}

fn validate_warehouse_geometry_request(
    request: &GeoWarehouseGeometryRowsRequest,
) -> Result<(), GeoGeometryError> {
    if request.version != CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::UnsupportedVersion,
            "Unsupported warehouse geometry rows version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION),
            ],
        ));
    }
    for (field, value) in [
        ("tile_id", request.tile_id.as_str()),
        ("frame_id", request.frame_id.as_str()),
        ("source_crs", request.source_crs.as_str()),
        (
            "source_unit_to_millimetres.unit_id",
            request.source_unit_to_millimetres.unit_id.as_str(),
        ),
    ] {
        validate_identifier(field, value)?;
    }
    if request.source_srid == 0
        || request.source_decimal_places > MAX_SOURCE_DECIMAL_PLACES
        || request.source_unit_to_millimetres.numerator == 0
        || request.source_unit_to_millimetres.denominator == 0
        || request.max_abs_coordinate_mm <= 0
        || request.max_vertices_per_geometry == 0
        || request.max_geometry_bytes_per_tile == 0
    {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidInput,
            "Warehouse geometry frame, unit conversion, and budgets must be positive and supported",
            [
                ("source_srid", request.source_srid.to_string()),
                (
                    "source_decimal_places",
                    request.source_decimal_places.to_string(),
                ),
                (
                    "unit_numerator",
                    request.source_unit_to_millimetres.numerator.to_string(),
                ),
                (
                    "unit_denominator",
                    request.source_unit_to_millimetres.denominator.to_string(),
                ),
            ],
        ));
    }
    Ok(())
}

fn reject_duplicate_warehouse_rows(
    rows: &[GeoWarehouseGeometryRow],
) -> Result<(), GeoGeometryError> {
    let mut feature_ids = BTreeSet::new();
    let mut record_ids = BTreeSet::new();
    for row in rows {
        validate_identifier("rows[].feature_id", &row.feature_id)?;
        validate_identifier("rows[].source_record_id", &row.source_record_id)?;
        if !feature_ids.insert(row.feature_id.as_str()) {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidInput,
                "Warehouse geometry rows repeat a feature id",
                [("feature_id", row.feature_id.as_str())],
            ));
        }
        if !record_ids.insert(row.source_record_id.as_str()) {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidInput,
                "Warehouse geometry rows repeat a source record id",
                [("source_record_id", row.source_record_id.as_str())],
            ));
        }
    }
    Ok(())
}

fn validate_homogeneous_source_rows(
    request: &GeoWarehouseGeometryRowsRequest,
    first: &GeoWarehouseGeometryRow,
    rows: &[GeoWarehouseGeometryRow],
) -> Result<(), GeoGeometryError> {
    validate_sha256("rows[].source_archive_sha256", &first.source_archive_sha256)?;
    for (field, value) in [
        ("rows[].source_dataset", first.source_dataset.as_str()),
        ("rows[].source_release", first.source_release.as_str()),
        (
            "rows[].source_release_date",
            first.source_release_date.as_str(),
        ),
        (
            "rows[].source_geometry_contract_version",
            first.source_geometry_contract_version.as_str(),
        ),
        (
            "rows[].transform_execution_id",
            first.transform_execution_id.as_str(),
        ),
        (
            "rows[].transform_definition_id",
            first.transform_definition_id.as_str(),
        ),
    ] {
        validate_identifier(field, value)?;
    }
    for row in rows {
        if row.source_crs != request.source_crs || row.source_srid != request.source_srid {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::MixedCrs,
                "Warehouse geometry rows must match the request source CRS and SRID",
                [
                    ("feature_id", row.feature_id.as_str()),
                    ("row_source_crs", row.source_crs.as_str()),
                    ("request_source_crs", request.source_crs.as_str()),
                    ("row_source_srid", row.source_srid.to_string().as_str()),
                    (
                        "request_source_srid",
                        request.source_srid.to_string().as_str(),
                    ),
                ],
            ));
        }
        let homogeneous = row.source_dataset == first.source_dataset
            && row.source_release == first.source_release
            && row.source_release_date == first.source_release_date
            && row.source_geometry_contract_version == first.source_geometry_contract_version
            && row.source_archive_sha256 == first.source_archive_sha256
            && row.transform_execution_id == first.transform_execution_id
            && row.transform_definition_id == first.transform_definition_id;
        if !homogeneous {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::MixedSourceExecution,
                "One local geometry tile cannot mix releases, archives, contracts, or transform executions",
                [("feature_id", row.feature_id.as_str())],
            ));
        }
    }
    Ok(())
}

fn exact_source_unit_affine(
    unit: &GeoExactSourceUnitMm,
    source_decimal_places: u32,
) -> Result<GeoAffineProjectionMm, GeoGeometryError> {
    let scale = u128::try_from(pow10_i128(source_decimal_places)?)
        .map_err(|_| GeoGeometryError::overflow("source decimal scale conversion"))?;
    let numerator = u128::from(unit.numerator);
    let denominator = u128::from(unit.denominator)
        .checked_mul(scale)
        .ok_or_else(|| GeoGeometryError::overflow("source unit affine denominator"))?;
    let divisor = gcd_u128(numerator, denominator);
    let numerator = i64::try_from(numerator / divisor)
        .map_err(|_| GeoGeometryError::overflow("source unit affine numerator"))?;
    let denominator = u64::try_from(denominator / divisor)
        .map_err(|_| GeoGeometryError::overflow("source unit affine denominator conversion"))?;
    Ok(GeoAffineProjectionMm {
        x_from_source_x_numerator: numerator,
        x_from_source_y_numerator: 0,
        y_from_source_x_numerator: 0,
        y_from_source_y_numerator: numerator,
        denominator,
    })
}

fn source_error_micrometres_ceiling(
    error: GeoSourceQuantizationFraction,
    source_decimal_places: u32,
    unit: &GeoExactSourceUnitMm,
) -> Result<u64, GeoGeometryError> {
    let scale = u128::try_from(pow10_i128(source_decimal_places)?)
        .map_err(|_| GeoGeometryError::overflow("source error decimal scale"))?;
    let numerator = u128::from(error.numerator)
        .checked_mul(u128::from(unit.numerator))
        .and_then(|value| value.checked_mul(1_000))
        .ok_or_else(|| GeoGeometryError::overflow("source error micrometre numerator"))?;
    let denominator = u128::from(error.denominator)
        .checked_mul(scale)
        .and_then(|value| value.checked_mul(u128::from(unit.denominator)))
        .ok_or_else(|| GeoGeometryError::overflow("source error micrometre denominator"))?;
    u64::try_from(ceil_ratio_u128(
        numerator,
        denominator,
        "source error micrometre ceiling",
    )?)
    .map_err(|_| GeoGeometryError::overflow("source error micrometre conversion"))
}

fn max_quantization_fraction(
    left: GeoSourceQuantizationFraction,
    right: GeoSourceQuantizationFraction,
) -> Result<GeoSourceQuantizationFraction, GeoGeometryError> {
    let left_cross = u128::from(left.numerator)
        .checked_mul(u128::from(right.denominator))
        .ok_or_else(|| GeoGeometryError::overflow("source quantization comparison"))?;
    let right_cross = u128::from(right.numerator)
        .checked_mul(u128::from(left.denominator))
        .ok_or_else(|| GeoGeometryError::overflow("source quantization comparison"))?;
    Ok(if right_cross > left_cross {
        right
    } else {
        left
    })
}

fn gcd_u128(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
}

fn decode_warehouse_geometry_row(
    row: GeoWarehouseGeometryRow,
    source_decimal_places: u32,
    max_vertices: u64,
) -> Result<DecodedWarehouseGeometry, GeoGeometryError> {
    validate_sha256("rows[].source_geom_wkb_sha256", &row.source_geom_wkb_sha256)?;
    let bytes = BASE64_STANDARD
        .decode(row.source_geom_wkb_base64.as_bytes())
        .map_err(|error| {
            GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidSourceEncoding,
                "Warehouse source geometry is not canonical base64",
                [
                    ("feature_id", row.feature_id.as_str()),
                    ("error", error.to_string().as_str()),
                ],
            )
        })?;
    if BASE64_STANDARD.encode(&bytes) != row.source_geom_wkb_base64 {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidSourceEncoding,
            "Warehouse source geometry base64 is not in canonical padded form",
            [("feature_id", row.feature_id.as_str())],
        ));
    }
    let actual_sha256 = sha256_hex(&bytes);
    if actual_sha256 != row.source_geom_wkb_sha256 {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidSourceDigest,
            "Warehouse source geometry bytes do not match their declared SHA-256",
            [
                ("feature_id", row.feature_id.as_str()),
                ("expected", row.source_geom_wkb_sha256.as_str()),
                ("actual", actual_sha256.as_str()),
            ],
        ));
    }

    let mut cursor = WkbCursor::new(&bytes);
    let mut accumulator = WkbDecodeAccumulator::new(source_decimal_places, max_vertices);
    let geometry = decode_wkb_geometry(&mut cursor, &mut accumulator, true)?;
    if cursor.remaining() != 0 {
        return Err(malformed_wkb(
            "ISO WKB contains trailing bytes after the top-level geometry",
            cursor.position(),
        ));
    }
    let bounds = accumulator.bounds.ok_or_else(|| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::EmptyGeometry,
            "Decoded warehouse WKB did not contain coordinates",
            [("feature_id", row.feature_id.as_str())],
        )
    })?;
    Ok(DecodedWarehouseGeometry {
        row,
        geometry,
        vertex_count: accumulator.vertex_count,
        max_error: accumulator.max_error,
        bounds,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn validate_sha256(field: &str, value: &str) -> Result<(), GeoGeometryError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidSourceDigest,
            "Geo source digests must be lowercase SHA-256 hex",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum WkbEndian {
    Big,
    Little,
}

struct WkbCursor<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> WkbCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn position(&self) -> usize {
        self.position
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.position)
    }

    fn read_exact<const N: usize>(&mut self) -> Result<[u8; N], GeoGeometryError> {
        let end = self
            .position
            .checked_add(N)
            .ok_or_else(|| GeoGeometryError::overflow("WKB cursor"))?;
        let slice = self.bytes.get(self.position..end).ok_or_else(|| {
            malformed_wkb("ISO WKB ended before the declared geometry", self.position)
        })?;
        self.position = end;
        slice
            .try_into()
            .map_err(|_| malformed_wkb("ISO WKB field had an invalid byte width", self.position))
    }

    fn read_endian(&mut self) -> Result<WkbEndian, GeoGeometryError> {
        match self.read_exact::<1>()?[0] {
            0 => Ok(WkbEndian::Big),
            1 => Ok(WkbEndian::Little),
            value => Err(GeoGeometryError::new(
                GeoGeometryErrorCode::MalformedWkb,
                "ISO WKB byte-order marker must be zero or one",
                [
                    ("position", self.position.saturating_sub(1).to_string()),
                    ("marker", value.to_string()),
                ],
            )),
        }
    }

    fn read_u32(&mut self, endian: WkbEndian) -> Result<u32, GeoGeometryError> {
        let bytes = self.read_exact::<4>()?;
        Ok(match endian {
            WkbEndian::Big => u32::from_be_bytes(bytes),
            WkbEndian::Little => u32::from_le_bytes(bytes),
        })
    }

    fn read_f64_bits(&mut self, endian: WkbEndian) -> Result<u64, GeoGeometryError> {
        let bytes = self.read_exact::<8>()?;
        Ok(match endian {
            WkbEndian::Big => u64::from_be_bytes(bytes),
            WkbEndian::Little => u64::from_le_bytes(bytes),
        })
    }
}

struct WkbDecodeAccumulator {
    source_decimal_places: u32,
    max_vertices: u64,
    vertex_count: u64,
    max_error: GeoSourceQuantizationFraction,
    bounds: Option<GeoSourceBoundsFixed>,
}

impl WkbDecodeAccumulator {
    fn new(source_decimal_places: u32, max_vertices: u64) -> Self {
        Self {
            source_decimal_places,
            max_vertices,
            vertex_count: 0,
            max_error: GeoSourceQuantizationFraction {
                numerator: 0,
                denominator: 1,
            },
            bounds: None,
        }
    }

    fn reserve_vertices(&mut self, additional: u32) -> Result<usize, GeoGeometryError> {
        let additional_u64 = u64::from(additional);
        let observed = self
            .vertex_count
            .checked_add(additional_u64)
            .ok_or_else(|| GeoGeometryError::overflow("decoded WKB vertex count"))?;
        if observed > self.max_vertices {
            return Err(GeoGeometryError::budget(
                GeoGeometryErrorCode::VertexBudgetExceeded,
                "Decoded WKB exceeds the declared per-value vertex budget",
                GeoGeometryBudgetBreach {
                    policy_id: "geometry.max_vertices_per_value".to_string(),
                    limit: GeoGeometryBudgetLimit::MaxVerticesPerGeometry,
                    enforcement: GeoGeometryBudgetEnforcement::RefuseBeforeMaterialization,
                    observed,
                    configured: self.max_vertices,
                },
                std::iter::empty::<(&str, &str)>(),
            ));
        }
        self.vertex_count = observed;
        usize::try_from(additional)
            .map_err(|_| GeoGeometryError::overflow("decoded WKB allocation size"))
    }

    fn container_capacity(
        &self,
        count: u32,
        context: &'static str,
    ) -> Result<usize, GeoGeometryError> {
        let count = u64::from(count);
        if count > self.max_vertices {
            return Err(GeoGeometryError::budget(
                GeoGeometryErrorCode::VertexBudgetExceeded,
                "Decoded WKB container count cannot fit within the declared vertex budget",
                GeoGeometryBudgetBreach {
                    policy_id: "geometry.max_vertices_per_value".to_string(),
                    limit: GeoGeometryBudgetLimit::MaxVerticesPerGeometry,
                    enforcement: GeoGeometryBudgetEnforcement::RefuseBeforeMaterialization,
                    observed: count,
                    configured: self.max_vertices,
                },
                [("context", context)],
            ));
        }
        usize::try_from(count)
            .map_err(|_| GeoGeometryError::overflow("decoded WKB container allocation size"))
    }

    fn coordinate(
        &mut self,
        x_bits: u64,
        y_bits: u64,
    ) -> Result<GeoSourcePointDecimal, GeoGeometryError> {
        let (x, x_fixed, x_error) = quantize_f64_to_decimal(x_bits, self.source_decimal_places)?;
        let (y, y_fixed, y_error) = quantize_f64_to_decimal(y_bits, self.source_decimal_places)?;
        self.max_error = max_quantization_fraction(self.max_error, x_error)?;
        self.max_error = max_quantization_fraction(self.max_error, y_error)?;
        let point_bounds = GeoSourceBoundsFixed {
            min_x: x_fixed,
            min_y: y_fixed,
            max_x: x_fixed,
            max_y: y_fixed,
        };
        if let Some(bounds) = &mut self.bounds {
            bounds.include(point_bounds);
        } else {
            self.bounds = Some(point_bounds);
        }
        Ok(GeoSourcePointDecimal { x, y })
    }
}

fn decode_wkb_geometry(
    cursor: &mut WkbCursor<'_>,
    accumulator: &mut WkbDecodeAccumulator,
    top_level: bool,
) -> Result<GeoSourceGeometry, GeoGeometryError> {
    let endian = cursor.read_endian()?;
    let geometry_type = cursor.read_u32(endian)?;
    match geometry_type {
        1 if top_level => {
            accumulator.reserve_vertices(1)?;
            let coordinate = accumulator
                .coordinate(cursor.read_f64_bits(endian)?, cursor.read_f64_bits(endian)?)?;
            Ok(GeoSourceGeometry::Point { coordinate })
        }
        3 => {
            let polygon = decode_wkb_polygon_body(cursor, endian, accumulator)?;
            Ok(GeoSourceGeometry::Polygon {
                exterior: polygon.exterior,
                holes: polygon.holes,
            })
        }
        6 if top_level => {
            let polygon_count = cursor.read_u32(endian)?;
            if polygon_count == 0 {
                return Err(GeoGeometryError::new(
                    GeoGeometryErrorCode::EmptyGeometry,
                    "ISO WKB multipolygon must contain at least one polygon",
                    std::iter::empty::<(&str, &str)>(),
                ));
            }
            let capacity = accumulator.container_capacity(polygon_count, "multipolygon members")?;
            let mut polygons = Vec::with_capacity(capacity);
            for _ in 0..polygon_count {
                let member = decode_wkb_geometry(cursor, accumulator, false)?;
                match member {
                    GeoSourceGeometry::Polygon { exterior, holes } => {
                        polygons.push(GeoSourcePolygon { exterior, holes });
                    }
                    _ => {
                        return Err(malformed_wkb(
                            "ISO WKB multipolygon member was not a polygon",
                            cursor.position(),
                        ));
                    }
                }
            }
            Ok(GeoSourceGeometry::MultiPolygon { polygons })
        }
        value => Err(GeoGeometryError::new(
            GeoGeometryErrorCode::UnsupportedGeometryType,
            "Warehouse WKB must be 2D ISO Point, Polygon, or MultiPolygon without EWKB flags",
            [
                ("geometry_type", value.to_string()),
                ("top_level", top_level.to_string()),
            ],
        )),
    }
}

fn decode_wkb_polygon_body(
    cursor: &mut WkbCursor<'_>,
    endian: WkbEndian,
    accumulator: &mut WkbDecodeAccumulator,
) -> Result<GeoSourcePolygon, GeoGeometryError> {
    let ring_count = cursor.read_u32(endian)?;
    if ring_count == 0 {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::EmptyGeometry,
            "ISO WKB polygon must contain an exterior ring",
            std::iter::empty::<(&str, &str)>(),
        ));
    }
    let capacity = accumulator.container_capacity(ring_count, "polygon rings")?;
    let mut rings = Vec::with_capacity(capacity);
    for _ in 0..ring_count {
        let coordinate_count = cursor.read_u32(endian)?;
        let capacity = accumulator.reserve_vertices(coordinate_count)?;
        let mut ring = Vec::with_capacity(capacity);
        for _ in 0..coordinate_count {
            ring.push(
                accumulator
                    .coordinate(cursor.read_f64_bits(endian)?, cursor.read_f64_bits(endian)?)?,
            );
        }
        rings.push(ring);
    }
    let mut rings = rings.into_iter();
    let exterior = rings.next().ok_or_else(|| {
        malformed_wkb(
            "ISO WKB polygon did not contain an exterior ring",
            cursor.position(),
        )
    })?;
    Ok(GeoSourcePolygon {
        exterior,
        holes: rings.collect(),
    })
}

fn quantize_f64_to_decimal(
    bits: u64,
    decimal_places: u32,
) -> Result<(String, i64, GeoSourceQuantizationFraction), GeoGeometryError> {
    let negative = bits >> 63 != 0;
    let biased_exponent = ((bits >> 52) & 0x7ff) as u16;
    let fraction = bits & ((1_u64 << 52) - 1);
    if biased_exponent == 0x7ff {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::NonFiniteCoordinate,
            "Non-finite WKB coordinates are not admissible geometry",
            [("f64_bits", format!("{bits:016x}"))],
        ));
    }
    if biased_exponent == 0 && fraction == 0 {
        return Ok((
            format_fixed_decimal(0, decimal_places)?,
            0,
            GeoSourceQuantizationFraction {
                numerator: 0,
                denominator: 1,
            },
        ));
    }
    if biased_exponent == 0 {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::SourcePrecisionExceeded,
            "Subnormal WKB coordinates fall outside the exact fixed-decimal admission budget",
            [("f64_bits", format!("{bits:016x}"))],
        ));
    }

    let mantissa = u128::from((1_u64 << 52) | fraction);
    let binary_exponent = i32::from(biased_exponent) - 1023 - 52;
    let decimal_scale = u128::try_from(pow10_i128(decimal_places)?)
        .map_err(|_| GeoGeometryError::overflow("WKB decimal scale"))?;
    let unsigned_scaled_numerator = mantissa
        .checked_mul(decimal_scale)
        .ok_or_else(|| GeoGeometryError::overflow("WKB fixed-decimal numerator"))?;
    let (unsigned_scaled_numerator, denominator) = if binary_exponent >= 0 {
        let shift = u32::try_from(binary_exponent)
            .map_err(|_| GeoGeometryError::overflow("WKB positive binary exponent"))?;
        (
            unsigned_scaled_numerator
                .checked_shl(shift)
                .ok_or_else(|| GeoGeometryError::overflow("WKB coordinate magnitude"))?,
            1_u64,
        )
    } else {
        let shift = binary_exponent.unsigned_abs();
        if shift >= 64 {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::SourcePrecisionExceeded,
                "WKB coordinate needs a binary denominator outside the exact admission budget",
                [("binary_denominator_power", shift.to_string())],
            ));
        }
        (unsigned_scaled_numerator, 1_u64 << shift)
    };
    let signed_numerator = i128::try_from(unsigned_scaled_numerator)
        .map_err(|_| GeoGeometryError::overflow("signed WKB coordinate numerator"))?;
    let signed_numerator = if negative {
        signed_numerator
            .checked_neg()
            .ok_or_else(|| GeoGeometryError::overflow("negative WKB coordinate"))?
    } else {
        signed_numerator
    };
    let (fixed, error_numerator) = round_rational_ties_even(signed_numerator, denominator)?;
    let divisor = gcd_u128(error_numerator, u128::from(denominator));
    let error = GeoSourceQuantizationFraction {
        numerator: u64::try_from(error_numerator / divisor)
            .map_err(|_| GeoGeometryError::overflow("WKB quantization numerator"))?,
        denominator: u64::try_from(u128::from(denominator) / divisor)
            .map_err(|_| GeoGeometryError::overflow("WKB quantization denominator"))?,
    };
    Ok((format_fixed_decimal(fixed, decimal_places)?, fixed, error))
}

fn format_fixed_decimal(value: i64, decimal_places: u32) -> Result<String, GeoGeometryError> {
    if decimal_places == 0 {
        return Ok(value.to_string());
    }
    let scale = u64::try_from(pow10_i128(decimal_places)?)
        .map_err(|_| GeoGeometryError::overflow("fixed-decimal formatting scale"))?;
    let magnitude = value.unsigned_abs();
    let whole = magnitude / scale;
    let fraction = magnitude % scale;
    let width = usize::try_from(decimal_places)
        .map_err(|_| GeoGeometryError::overflow("fixed-decimal formatting width"))?;
    Ok(format!(
        "{}{whole}.{fraction:0width$}",
        if value.is_negative() { "-" } else { "" }
    ))
}

fn malformed_wkb(message: &str, position: usize) -> GeoGeometryError {
    GeoGeometryError::new(
        GeoGeometryErrorCode::MalformedWkb,
        message,
        [("position", position.to_string())],
    )
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

pub(super) fn parse_fixed_decimal(
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
