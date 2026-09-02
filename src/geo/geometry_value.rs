#![forbid(unsafe_code)]

//! Canonical typed geometry at the offline tile-artifact boundary.
//!
//! Source coordinates are fixed-scale decimal strings, never binary floats.
//! A versioned affine frame maps their exact integer representation to local
//! millimetres and records the snap error. Decision-time geometry therefore
//! consumes only canonical integers. This makes replay exact relative to the
//! admitted artifact; it does not make the source survey or projection exact.

use super::{
    control::GeoLicenseClass,
    geometry::{
        GeoLinearRingMm, GeoPointLocation, GeoPointMm, GeoPredicateError, GeoPredicateErrorCode,
        GeoSegmentIntersection, exact_segment_intersection,
    },
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use h3o::{CellIndex, LatLng, Resolution};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    str::FromStr,
};

pub const CANON_GEO_GEOMETRY_REQUEST_VERSION: &str = "canon_geo_geometry_request.v0";
pub const CANON_GEO_GEOMETRY_VALUE_VERSION: &str = "canon_geo_geometry_value.v0";
pub const CANON_GEO_GEOMETRY_TILE_VERSION: &str = "canon_geo_geometry_tile.v0";
pub const CANON_GEO_LOCAL_FRAME_VERSION: &str = "canon_geo_local_frame.v0";
pub const CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION: &str =
    "canon_geo_client_tile_ingest_request.v0";
pub const CANON_GEO_PROVIDER_TILE_BUILD_VERSION: &str = "canon_geo_provider_tile_build.v0";
pub const CANON_GEO_PROVIDER_TILE_CONTRACT_VERSION: &str = "canon_geo_provider_tile_contract.v0";
pub const CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION: &str = "canon_geo_warehouse_geometry_rows.v0";
pub const CANON_GEO_WAREHOUSE_GEOMETRY_VERSION: &str = "canon_geo_warehouse_geometry.v0";

const CANON_GEO_PLANAR_FRAME_METHOD_ID: &str = "canon:planar-source-affine";
const CANON_GEO_PLANAR_FRAME_METHOD_VERSION: &str = "v0";
const CANON_GEO_CLIENT_TILE_TRANSFORM_METHOD_ID: &str = "canon:client-geojson-wgs84-frame-declared";
const CANON_GEO_CLIENT_TILE_TRANSFORM_METHOD_VERSION: &str = "v0";
const ISO_WKB_2D_BASE64_ENCODING: &str = "iso-wkb-2d-base64";

const MAX_SOURCE_DECIMAL_PLACES: u32 = 9;
const MAX_IDENTIFIER_BYTES: usize = 256;
const MAX_PROVIDER_TILE_WORK_CELLS: usize = 10_000;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_tile: Option<GeoProviderTileContract>,
}

/// Provenance mode for a source inside an offline provider tile. Canon-owned
/// source tiles carry verifiable source bytes; client layers carry explicit
/// declarations rather than inferred defaults.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeoProviderTileSourceProvenance {
    CanonFullProvenance {
        source_path: String,
        source_digest: String,
        source_record_count: u64,
    },
    ClientDeclared {
        vendor: String,
        vintage: String,
        source_crs: String,
        coverage_extent: String,
        mutual_exclusivity_declared: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoProviderTileSource {
    pub source_instance_id: String,
    pub release_id: String,
    pub release_digest: String,
    pub license_class: GeoLicenseClass,
    pub license_expression: String,
    pub attribution_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution_text: Option<String>,
    pub provenance: GeoProviderTileSourceProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoProviderTileSubsetKind {
    H3CellSetAndSourceCoverageIntersection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoProviderTileCoverageState {
    Complete,
    Partial,
    Absent,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoProviderTileSourceCoverage {
    pub source_instance_id: String,
    pub h3_cell: String,
    pub coverage_state: GeoProviderTileCoverageState,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoProviderTileSubsetPredicate {
    pub kind: GeoProviderTileSubsetKind,
    pub predicate_id: String,
    pub h3_resolution: u8,
    pub center_cell: String,
    pub halo_k: u32,
    pub work_cells: Vec<String>,
    pub source_coverages: Vec<GeoProviderTileSourceCoverage>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoProviderTileLicensePosture {
    pub posture_id: String,
    pub output_license_expression: String,
    pub redistribution_notice: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attribution_requirements: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub client_restricted_source_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoProviderGeometryFidelity {
    SourceFidelity,
    VendorSimplified,
    DisplaySimplified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoProviderTileRedactionClass {
    Shareable,
    ShareableAttributionRequired,
    LocalOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoProviderTileFieldLocator {
    pub field_path: String,
    pub source_path: String,
    pub record_ordinal: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoProviderTileFeatureProvenance {
    pub source_instance_id: String,
    pub source_path: String,
    pub source_digest: String,
    pub source_record_id: String,
    pub record_ordinal: u64,
    pub field_locators: Vec<GeoProviderTileFieldLocator>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoProviderTileFeatureContract {
    pub feature_id: String,
    pub source_instance_id: String,
    pub source_feature_id: String,
    pub decision_geometry_fidelity: GeoProviderGeometryFidelity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_geometry_fidelity: Option<GeoProviderGeometryFidelity>,
    pub license_class: GeoLicenseClass,
    pub redaction_class: GeoProviderTileRedactionClass,
    pub provenance: GeoProviderTileFeatureProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoClientTileSourceFormat {
    GeoJson,
    NdjsonGeoJson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoClientTileCoverageExtentKind {
    ClientDeclaredH3CellSet,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoClientTileCoverageExtent {
    pub extent_id: String,
    pub kind: GeoClientTileCoverageExtentKind,
    pub h3_cells: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoClientTileVendorIdentifier {
    pub issuer: String,
    pub role: String,
    pub property: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoClientTileIngestRequest {
    pub version: String,
    pub tile_id: String,
    pub source_format: GeoClientTileSourceFormat,
    pub source_path: String,
    pub source_digest: String,
    pub declared_crs: String,
    pub frame: GeoLocalFrameContract,
    pub source_instance_id: String,
    pub release_id: String,
    pub release_digest: String,
    pub vendor: String,
    pub vintage: String,
    pub vendor_identifier: GeoClientTileVendorIdentifier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_record_id_property: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supplemental_h3_cells_property: Option<String>,
    pub license_expression: String,
    pub coverage_extent: GeoClientTileCoverageExtent,
    pub mutual_exclusivity_declared: bool,
    pub h3_resolution: u8,
    pub halo_k: u32,
    pub work_cells: Vec<String>,
    pub max_features: u64,
    pub max_vertices_per_geometry: u64,
    pub max_geometry_bytes_per_tile: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoClientTileMembershipRule {
    RepresentativePointAnchor,
    DeclaredSupplementalCoverage,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoClientTileMembership {
    pub source_instance_id: String,
    pub feature_id: String,
    pub source_feature_id: String,
    pub h3_cell: String,
    pub rule: GeoClientTileMembershipRule,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoClientTileFeatureAlias {
    pub feature_id: String,
    pub alias_namespace: String,
    pub alias_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoClientTileValidationRefusalCount {
    pub reason: GeoGeometryErrorCode,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoClientTileValidationSummary {
    pub source_feature_count: u64,
    pub accepted_feature_count: u64,
    pub refused_feature_count: u64,
    pub outside_work_cell_count: u64,
    pub refusal_counts: Vec<GeoClientTileValidationRefusalCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoClientTileIngestSummary {
    pub validation: GeoClientTileValidationSummary,
    pub membership_row_count: u64,
    pub anchor_membership_count: u64,
    pub supplemental_membership_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoClientTileTransformProvenance {
    pub method_id: String,
    pub method_version: String,
    pub declared_crs: String,
    pub frame_id: String,
    pub h3_library: String,
    pub h3_resolution: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoClientTileIngestReport {
    pub request_version: String,
    pub source_format: GeoClientTileSourceFormat,
    pub source_path: String,
    pub source_digest: String,
    pub declared_crs: String,
    pub transform: GeoClientTileTransformProvenance,
    pub coverage_extent: GeoClientTileCoverageExtent,
    pub mutual_exclusivity_declared: bool,
    pub aliases: Vec<GeoClientTileFeatureAlias>,
    pub memberships: Vec<GeoClientTileMembership>,
    pub summary: GeoClientTileIngestSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoProviderTileDataBookDecision {
    DatabookLikeSelfContainedTileNoNewDependency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoProviderTileContract {
    pub version: String,
    pub tile_id: String,
    pub databook_decision: GeoProviderTileDataBookDecision,
    pub subset: GeoProviderTileSubsetPredicate,
    pub sources: Vec<GeoProviderTileSource>,
    pub license_posture: GeoProviderTileLicensePosture,
    pub features: Vec<GeoProviderTileFeatureContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_ingest: Option<GeoClientTileIngestReport>,
    pub tile_content_blake3: String,
}

/// Offline provider-tile build request. The downloaded tile/local client
/// layer is already present in memory here; this function does not acquire or
/// fetch anything and refuses if the local source declarations cannot explain
/// every emitted decision geometry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoProviderGeometryTileBuildRequest {
    pub version: String,
    pub tile_id: String,
    pub geometry_request: GeoGeometryTileRequest,
    pub subset: GeoProviderTileSubsetPredicate,
    pub sources: Vec<GeoProviderTileSource>,
    pub license_posture: GeoProviderTileLicensePosture,
    pub feature_contracts: Vec<GeoProviderTileFeatureContract>,
    pub allow_vendor_simplified_decision_geometry: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoGeometryErrorCode {
    UnsupportedVersion,
    InvalidInput,
    InvalidFrame,
    InvalidCoordinate,
    InvalidSourceDigest,
    InvalidSourceEncoding,
    InvalidSourceProvenance,
    InvalidTileContract,
    InvalidLicensePosture,
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
        provider_tile: None,
    })
}

pub fn materialize_provider_geometry_tile(
    request: &GeoProviderGeometryTileBuildRequest,
) -> Result<GeoGeometryTileArtifact, GeoGeometryError> {
    if request.version != CANON_GEO_PROVIDER_TILE_BUILD_VERSION {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::UnsupportedVersion,
            "Unsupported Geo provider tile build version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_PROVIDER_TILE_BUILD_VERSION),
            ],
        ));
    }
    validate_provider_contract_identifier("tile_id", &request.tile_id)?;

    let mut geometry_tile = materialize_geometry_tile(&request.geometry_request)?;
    if geometry_tile.frame.tile_id != request.tile_id {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Provider tile id must match the materialized geometry frame tile id",
            [
                ("tile_id", request.tile_id.as_str()),
                ("frame_tile_id", geometry_tile.frame.tile_id.as_str()),
            ],
        ));
    }

    let mut sources = request.sources.clone();
    let source_ids = canonicalize_provider_sources(&mut sources)?;

    let mut subset = request.subset.clone();
    canonicalize_provider_subset(&mut subset, &request.tile_id, &source_ids)?;

    let mut license_posture = request.license_posture.clone();
    canonicalize_provider_license_posture(&mut license_posture, &sources)?;

    let mut feature_contracts = request.feature_contracts.clone();
    canonicalize_provider_feature_contracts(
        &mut feature_contracts,
        &geometry_tile.features,
        &sources,
        request.allow_vendor_simplified_decision_geometry,
    )?;

    let mut provider_tile = GeoProviderTileContract {
        version: CANON_GEO_PROVIDER_TILE_CONTRACT_VERSION.to_string(),
        tile_id: request.tile_id.clone(),
        databook_decision:
            GeoProviderTileDataBookDecision::DatabookLikeSelfContainedTileNoNewDependency,
        subset,
        sources,
        license_posture,
        features: feature_contracts,
        client_ingest: None,
        tile_content_blake3: String::new(),
    };
    provider_tile.tile_content_blake3 =
        provider_geometry_tile_content_blake3(&geometry_tile, &provider_tile)?;
    geometry_tile.provider_tile = Some(provider_tile);
    Ok(geometry_tile)
}

pub fn ingest_client_geometry_tile(
    request: &GeoClientTileIngestRequest,
    source_bytes: &[u8],
) -> Result<GeoGeometryTileArtifact, GeoGeometryError> {
    let (work_cells, coverage_cells) = validate_client_tile_ingest_request(request)?;
    let actual_source_digest = blake3::hash(source_bytes).to_hex().to_string();
    if actual_source_digest != request.source_digest {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidSourceDigest,
            "Client tile ingest source bytes do not match the declared BLAKE3 digest",
            [
                ("source_path", request.source_path.as_str()),
                ("expected", request.source_digest.as_str()),
                ("actual", actual_source_digest.as_str()),
            ],
        ));
    }

    let source_features = parse_client_geojson_feature_values(request.source_format, source_bytes)?;
    let source_feature_count = usize_to_u64(source_features.len(), "client source feature count")?;
    let resolution = Resolution::try_from(request.h3_resolution).map_err(|error| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Client tile ingest declares an unsupported H3 resolution",
            [
                ("h3_resolution", request.h3_resolution.to_string()),
                ("error", error.to_string()),
            ],
        )
    })?;
    let alias_namespace = client_alias_namespace(&request.vendor_identifier)?;
    let mut seen_source_feature_ids = BTreeSet::new();
    let mut refusal_counts = BTreeMap::<GeoGeometryErrorCode, u64>::new();
    let mut outside_work_cell_count = 0_u64;
    let mut geometry_features = Vec::new();
    let mut feature_contracts = Vec::new();
    let mut aliases = Vec::new();
    let mut memberships = Vec::new();
    let mut anchor_membership_count = 0_u64;
    let mut supplemental_membership_count = 0_u64;

    for (record_ordinal, feature) in source_features {
        let parsed = match parse_client_geojson_feature(request, record_ordinal, &feature) {
            Ok(parsed) => parsed,
            Err(error) => {
                increment_refusal_count(&mut refusal_counts, error.code)?;
                continue;
            }
        };
        if !seen_source_feature_ids.insert(parsed.source_feature_id.clone()) {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidSourceProvenance,
                "Client tile ingest repeats a vendor feature identifier",
                [
                    ("alias_namespace", alias_namespace.as_str()),
                    ("source_feature_id", parsed.source_feature_id.as_str()),
                ],
            ));
        }

        let anchor_cell = h3_cell_for_decimal_point(
            &parsed.representative_point,
            request.frame.source_decimal_places,
            resolution,
        )?;
        let mut feature_memberships = vec![GeoClientTileMembership {
            source_instance_id: request.source_instance_id.clone(),
            feature_id: parsed.feature_id.clone(),
            source_feature_id: parsed.source_feature_id.clone(),
            h3_cell: anchor_cell,
            rule: GeoClientTileMembershipRule::RepresentativePointAnchor,
        }];
        match parse_client_supplemental_cells(request, &feature, resolution) {
            Ok(cells) => {
                for cell in cells {
                    feature_memberships.push(GeoClientTileMembership {
                        source_instance_id: request.source_instance_id.clone(),
                        feature_id: parsed.feature_id.clone(),
                        source_feature_id: parsed.source_feature_id.clone(),
                        h3_cell: cell,
                        rule: GeoClientTileMembershipRule::DeclaredSupplementalCoverage,
                    });
                }
            }
            Err(error) => {
                increment_refusal_count(&mut refusal_counts, error.code)?;
                continue;
            }
        }
        feature_memberships.sort();
        feature_memberships.dedup();
        let in_work_cell = feature_memberships
            .iter()
            .any(|membership| work_cells.contains(&membership.h3_cell));
        if !in_work_cell {
            outside_work_cell_count = checked_add_u64(
                outside_work_cell_count,
                1,
                "client tile outside work-cell count",
            )?;
            continue;
        }
        for membership in &feature_memberships {
            if work_cells.contains(&membership.h3_cell)
                && !coverage_cells.contains(&membership.h3_cell)
            {
                return Err(GeoGeometryError::new(
                    GeoGeometryErrorCode::InvalidTileContract,
                    "Client tile feature membership falls outside the declared coverage extent",
                    [
                        ("feature_id", parsed.feature_id.as_str()),
                        ("h3_cell", membership.h3_cell.as_str()),
                        (
                            "coverage_extent",
                            request.coverage_extent.extent_id.as_str(),
                        ),
                    ],
                ));
            }
        }

        let single_request = GeoGeometryTileRequest {
            version: CANON_GEO_GEOMETRY_REQUEST_VERSION.to_string(),
            frame: request.frame.clone(),
            features: vec![parsed.geometry_feature.clone()],
            max_vertices_per_geometry: request.max_vertices_per_geometry,
            max_geometry_bytes_per_tile: request.max_geometry_bytes_per_tile,
        };
        if let Err(error) = materialize_geometry_tile(&single_request) {
            increment_refusal_count(&mut refusal_counts, error.code)?;
            continue;
        }

        anchor_membership_count = checked_add_u64(
            anchor_membership_count,
            1,
            "client tile anchor membership count",
        )?;
        let supplemental_count = feature_memberships
            .iter()
            .filter(|membership| {
                membership.rule == GeoClientTileMembershipRule::DeclaredSupplementalCoverage
                    && work_cells.contains(&membership.h3_cell)
            })
            .count();
        supplemental_membership_count = checked_add_u64(
            supplemental_membership_count,
            usize_to_u64(
                supplemental_count,
                "client tile supplemental membership count",
            )?,
            "client tile supplemental membership count",
        )?;
        memberships.extend(
            feature_memberships
                .into_iter()
                .filter(|membership| work_cells.contains(&membership.h3_cell)),
        );
        aliases.push(GeoClientTileFeatureAlias {
            feature_id: parsed.feature_id.clone(),
            alias_namespace: alias_namespace.clone(),
            alias_value: parsed.source_feature_id.clone(),
        });
        geometry_features.push(parsed.geometry_feature);
        feature_contracts.push(parsed.feature_contract);
    }

    let accepted_feature_count = usize_to_u64(geometry_features.len(), "accepted client features")?;
    if accepted_feature_count > request.max_features {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidInput,
            "Client tile ingest accepted feature count exceeds the declared budget",
            [
                ("observed", accepted_feature_count.to_string()),
                ("configured", request.max_features.to_string()),
            ],
        ));
    }
    memberships.sort();
    memberships.dedup();
    aliases.sort();
    let refused_feature_count = refusal_counts
        .values()
        .copied()
        .try_fold(0_u64, |sum, count| {
            checked_add_u64(sum, count, "client tile refused feature count")
        })?;
    let refusal_counts = refusal_counts
        .into_iter()
        .map(|(reason, count)| GeoClientTileValidationRefusalCount { reason, count })
        .collect::<Vec<_>>();
    let validation = GeoClientTileValidationSummary {
        source_feature_count,
        accepted_feature_count,
        refused_feature_count,
        outside_work_cell_count,
        refusal_counts,
    };
    let report = GeoClientTileIngestReport {
        request_version: request.version.clone(),
        source_format: request.source_format,
        source_path: request.source_path.clone(),
        source_digest: request.source_digest.clone(),
        declared_crs: request.declared_crs.clone(),
        transform: GeoClientTileTransformProvenance {
            method_id: CANON_GEO_CLIENT_TILE_TRANSFORM_METHOD_ID.to_string(),
            method_version: CANON_GEO_CLIENT_TILE_TRANSFORM_METHOD_VERSION.to_string(),
            declared_crs: request.declared_crs.clone(),
            frame_id: request.frame.frame_id.clone(),
            h3_library: "h3o=0.10.0".to_string(),
            h3_resolution: request.h3_resolution,
        },
        coverage_extent: request.coverage_extent.clone(),
        mutual_exclusivity_declared: request.mutual_exclusivity_declared,
        aliases,
        memberships,
        summary: GeoClientTileIngestSummary {
            validation,
            membership_row_count: checked_add_u64(
                anchor_membership_count,
                supplemental_membership_count,
                "client tile membership row count",
            )?,
            anchor_membership_count,
            supplemental_membership_count,
        },
    };

    let source_coverages = work_cells
        .iter()
        .map(|cell| GeoProviderTileSourceCoverage {
            source_instance_id: request.source_instance_id.clone(),
            h3_cell: cell.clone(),
            coverage_state: if coverage_cells.contains(cell) {
                GeoProviderTileCoverageState::Complete
            } else {
                GeoProviderTileCoverageState::Absent
            },
        })
        .collect::<Vec<_>>();
    let build_request = GeoProviderGeometryTileBuildRequest {
        version: CANON_GEO_PROVIDER_TILE_BUILD_VERSION.to_string(),
        tile_id: request.tile_id.clone(),
        geometry_request: GeoGeometryTileRequest {
            version: CANON_GEO_GEOMETRY_REQUEST_VERSION.to_string(),
            frame: request.frame.clone(),
            features: geometry_features,
            max_vertices_per_geometry: request.max_vertices_per_geometry,
            max_geometry_bytes_per_tile: request.max_geometry_bytes_per_tile,
        },
        subset: GeoProviderTileSubsetPredicate {
            kind: GeoProviderTileSubsetKind::H3CellSetAndSourceCoverageIntersection,
            predicate_id: format!(
                "client-ingest:{}:{}",
                request.source_instance_id, request.tile_id
            ),
            h3_resolution: request.h3_resolution,
            center_cell: request.tile_id.clone(),
            halo_k: request.halo_k,
            work_cells: work_cells.iter().cloned().collect(),
            source_coverages,
        },
        sources: vec![GeoProviderTileSource {
            source_instance_id: request.source_instance_id.clone(),
            release_id: request.release_id.clone(),
            release_digest: request.release_digest.clone(),
            license_class: GeoLicenseClass::RestrictedLocalOnly,
            license_expression: request.license_expression.clone(),
            attribution_required: false,
            attribution_text: None,
            provenance: GeoProviderTileSourceProvenance::ClientDeclared {
                vendor: request.vendor.clone(),
                vintage: request.vintage.clone(),
                source_crs: request.declared_crs.clone(),
                coverage_extent: request.coverage_extent.extent_id.clone(),
                mutual_exclusivity_declared: request.mutual_exclusivity_declared,
            },
        }],
        license_posture: GeoProviderTileLicensePosture {
            posture_id: format!("client-ingest-local-only:{}", request.source_instance_id),
            output_license_expression: request.license_expression.clone(),
            redistribution_notice:
                "Contains client-declared geometry; raw tile remains local-only.".to_string(),
            attribution_requirements: Vec::new(),
            client_restricted_source_ids: vec![request.source_instance_id.clone()],
        },
        feature_contracts,
        allow_vendor_simplified_decision_geometry: false,
    };
    let mut geometry_tile = materialize_provider_geometry_tile(&build_request)?;
    let mut provider_tile = geometry_tile.provider_tile.take().ok_or_else(|| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Client tile ingest did not produce a provider tile contract",
            std::iter::empty::<(&str, &str)>(),
        )
    })?;
    provider_tile.client_ingest = Some(report);
    provider_tile.tile_content_blake3 =
        provider_geometry_tile_content_blake3(&geometry_tile, &provider_tile)?;
    geometry_tile.provider_tile = Some(provider_tile);
    Ok(geometry_tile)
}

#[derive(Debug)]
struct ParsedClientTileFeature {
    feature_id: String,
    source_feature_id: String,
    representative_point: GeoSourcePointDecimal,
    geometry_feature: GeoGeometryFeatureInput,
    feature_contract: GeoProviderTileFeatureContract,
}

#[derive(Debug)]
struct ParsedClientGeometry {
    geometry: GeoSourceGeometry,
    representative_point: GeoSourcePointDecimal,
}

fn validate_client_tile_ingest_request(
    request: &GeoClientTileIngestRequest,
) -> Result<(BTreeSet<String>, BTreeSet<String>), GeoGeometryError> {
    if request.version != CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::UnsupportedVersion,
            "Unsupported Geo client tile ingest request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION),
            ],
        ));
    }
    if request.declared_crs != "EPSG:4326" {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::MixedCrs,
            "Client tile ingest v0 requires an explicit EPSG:4326 longitude/latitude CRS",
            [("declared_crs", request.declared_crs.as_str())],
        ));
    }
    validate_frame(&request.frame)?;
    if request.frame.source_crs != request.declared_crs
        || request.frame.source_axis_domain != GeoSourceAxisDomain::GeographicLongitudeLatitude
    {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::MixedCrs,
            "Client tile ingest frame must use the declared WGS84 longitude/latitude CRS",
            [
                ("declared_crs", request.declared_crs.as_str()),
                ("frame_crs", request.frame.source_crs.as_str()),
            ],
        ));
    }
    if request.frame.tile_id != request.tile_id {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Client tile ingest frame tile_id must match the ingest tile_id",
            [
                ("tile_id", request.tile_id.as_str()),
                ("frame_tile_id", request.frame.tile_id.as_str()),
            ],
        ));
    }
    for (field, value) in [
        ("tile_id", request.tile_id.as_str()),
        ("source_path", request.source_path.as_str()),
        ("source_instance_id", request.source_instance_id.as_str()),
        ("release_id", request.release_id.as_str()),
        ("vendor", request.vendor.as_str()),
        ("vintage", request.vintage.as_str()),
        ("license_expression", request.license_expression.as_str()),
        (
            "coverage_extent.extent_id",
            request.coverage_extent.extent_id.as_str(),
        ),
    ] {
        validate_provider_text(field, value)?;
    }
    validate_provider_blake3("source_digest", &request.source_digest)?;
    validate_provider_blake3("release_digest", &request.release_digest)?;
    validate_provider_text(
        "vendor_identifier.issuer",
        &request.vendor_identifier.issuer,
    )?;
    validate_provider_text("vendor_identifier.role", &request.vendor_identifier.role)?;
    validate_provider_text(
        "vendor_identifier.property",
        &request.vendor_identifier.property,
    )?;
    if let Some(property) = &request.source_record_id_property {
        validate_provider_text("source_record_id_property", property)?;
    }
    if let Some(property) = &request.supplemental_h3_cells_property {
        validate_provider_text("supplemental_h3_cells_property", property)?;
    }
    if request.max_features == 0
        || request.max_vertices_per_geometry == 0
        || request.max_geometry_bytes_per_tile == 0
    {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidInput,
            "Client tile ingest budgets must be positive",
            [
                ("max_features", request.max_features.to_string()),
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
    let resolution = Resolution::try_from(request.h3_resolution).map_err(|error| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Client tile ingest declares an unsupported H3 resolution",
            [
                ("h3_resolution", request.h3_resolution.to_string()),
                ("error", error.to_string()),
            ],
        )
    })?;
    parse_client_h3_cell("tile_id", &request.tile_id, resolution)?;
    let work_cells = canonical_client_cell_set("work_cells", &request.work_cells, resolution)?;
    if !work_cells.contains(&request.tile_id) {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Client tile ingest work cells must include the tile center",
            [("tile_id", request.tile_id.as_str())],
        ));
    }
    if request.coverage_extent.h3_cells.is_empty() {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Client tile ingest requires an explicit nonempty coverage extent",
            [(
                "coverage_extent",
                request.coverage_extent.extent_id.as_str(),
            )],
        ));
    }
    let coverage_cells = canonical_client_cell_set(
        "coverage_extent.h3_cells",
        &request.coverage_extent.h3_cells,
        resolution,
    )?;
    Ok((work_cells, coverage_cells))
}

fn canonical_client_cell_set(
    field: &str,
    values: &[String],
    resolution: Resolution,
) -> Result<BTreeSet<String>, GeoGeometryError> {
    if values.is_empty() || values.len() > MAX_PROVIDER_TILE_WORK_CELLS {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Client tile ingest requires a bounded nonempty H3 cell set",
            [
                ("field", field),
                ("observed", values.len().to_string().as_str()),
                ("maximum", MAX_PROVIDER_TILE_WORK_CELLS.to_string().as_str()),
            ],
        ));
    }
    let mut cells = BTreeSet::new();
    for value in values {
        let cell = parse_client_h3_cell(field, value, resolution)?;
        if !cells.insert(cell) {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidTileContract,
                "Client tile ingest H3 cell sets must not repeat cells",
                [("field", field), ("h3_cell", value.as_str())],
            ));
        }
    }
    Ok(cells)
}

fn parse_client_h3_cell(
    field: &str,
    value: &str,
    expected_resolution: Resolution,
) -> Result<String, GeoGeometryError> {
    let cell = parse_provider_h3_cell(field, value, u8::from(expected_resolution))?;
    let canonical = cell.to_string();
    if canonical != value {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Client tile ingest H3 cells must use canonical lowercase encoding",
            [
                ("field", field),
                ("value", value),
                ("canonical", canonical.as_str()),
            ],
        ));
    }
    Ok(canonical)
}

fn parse_client_geojson_feature_values(
    source_format: GeoClientTileSourceFormat,
    source_bytes: &[u8],
) -> Result<Vec<(u64, JsonValue)>, GeoGeometryError> {
    let source = std::str::from_utf8(source_bytes).map_err(|error| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidSourceEncoding,
            "Client tile ingest source is not UTF-8 GeoJSON/NDJSON",
            [("error", error.to_string())],
        )
    })?;
    match source_format {
        GeoClientTileSourceFormat::GeoJson => {
            let value: JsonValue = serde_json::from_str(source).map_err(|error| {
                GeoGeometryError::new(
                    GeoGeometryErrorCode::InvalidSourceEncoding,
                    "Client tile ingest source is not valid GeoJSON",
                    [("error", error.to_string())],
                )
            })?;
            match geojson_type(&value) {
                Some("FeatureCollection") => {
                    let features = value
                        .get("features")
                        .and_then(JsonValue::as_array)
                        .ok_or_else(|| {
                            GeoGeometryError::new(
                                GeoGeometryErrorCode::InvalidSourceEncoding,
                                "GeoJSON FeatureCollection must carry a features array",
                                std::iter::empty::<(&str, &str)>(),
                            )
                        })?;
                    features
                        .iter()
                        .cloned()
                        .enumerate()
                        .map(|(index, feature)| {
                            Ok((usize_to_u64(index, "GeoJSON feature ordinal")?, feature))
                        })
                        .collect()
                }
                Some("Feature") => Ok(vec![(0, value)]),
                Some(kind) => Err(GeoGeometryError::new(
                    GeoGeometryErrorCode::UnsupportedGeometryType,
                    "Client tile ingest GeoJSON input must be a Feature or FeatureCollection",
                    [("type", kind)],
                )),
                None => Err(GeoGeometryError::new(
                    GeoGeometryErrorCode::InvalidSourceEncoding,
                    "Client tile ingest GeoJSON object is missing a type field",
                    std::iter::empty::<(&str, &str)>(),
                )),
            }
        }
        GeoClientTileSourceFormat::NdjsonGeoJson => {
            let mut features = Vec::new();
            for (index, line) in source.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                let value: JsonValue = serde_json::from_str(line).map_err(|error| {
                    GeoGeometryError::new(
                        GeoGeometryErrorCode::InvalidSourceEncoding,
                        "Client tile ingest NDJSON line is not valid GeoJSON",
                        [
                            ("line", index.saturating_add(1).to_string()),
                            ("error", error.to_string()),
                        ],
                    )
                })?;
                if geojson_type(&value) != Some("Feature") {
                    return Err(GeoGeometryError::new(
                        GeoGeometryErrorCode::UnsupportedGeometryType,
                        "Client tile ingest NDJSON supports one GeoJSON Feature per line",
                        [("line", index.saturating_add(1).to_string())],
                    ));
                }
                features.push((usize_to_u64(index, "NDJSON feature ordinal")?, value));
            }
            Ok(features)
        }
    }
}

fn parse_client_geojson_feature(
    request: &GeoClientTileIngestRequest,
    record_ordinal: u64,
    feature: &JsonValue,
) -> Result<ParsedClientTileFeature, GeoGeometryError> {
    if geojson_type(feature) != Some("Feature") {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::UnsupportedGeometryType,
            "Client tile ingest supports only GeoJSON Feature records",
            [("record_ordinal", record_ordinal.to_string())],
        ));
    }
    let properties = feature_properties(feature)?;
    let source_feature_id = feature_property_text(
        properties,
        &request.vendor_identifier.property,
        "vendor_identifier.property",
    )?;
    validate_provider_contract_identifier(
        "properties[vendor_identifier.property]",
        &source_feature_id,
    )?;
    let source_record_id = match &request.source_record_id_property {
        Some(property) => feature_property_text(properties, property, "source_record_id_property")?,
        None => feature
            .get("id")
            .map(json_scalar_text)
            .transpose()?
            .unwrap_or_else(|| source_feature_id.clone()),
    };
    validate_provider_contract_identifier("source_record_id", &source_record_id)?;
    let geometry_value = feature.get("geometry").ok_or_else(|| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::EmptyGeometry,
            "Client tile ingest feature is missing geometry",
            [("source_feature_id", source_feature_id.as_str())],
        )
    })?;
    let parsed_geometry =
        parse_client_geojson_geometry(geometry_value, request.frame.source_decimal_places)?;
    let feature_id = source_feature_id.clone();
    let geometry_feature = GeoGeometryFeatureInput {
        feature_id: feature_id.clone(),
        source_crs: request.declared_crs.clone(),
        geometry: parsed_geometry.geometry,
    };
    let mut field_locators = vec![
        GeoProviderTileFieldLocator {
            field_path: "$.geometry".to_string(),
            source_path: request.source_path.clone(),
            record_ordinal,
        },
        GeoProviderTileFieldLocator {
            field_path: format!("$.properties.{}", request.vendor_identifier.property),
            source_path: request.source_path.clone(),
            record_ordinal,
        },
    ];
    if let Some(property) = &request.source_record_id_property {
        field_locators.push(GeoProviderTileFieldLocator {
            field_path: format!("$.properties.{property}"),
            source_path: request.source_path.clone(),
            record_ordinal,
        });
    }
    if let Some(property) = &request.supplemental_h3_cells_property
        && properties.contains_key(property)
    {
        field_locators.push(GeoProviderTileFieldLocator {
            field_path: format!("$.properties.{property}"),
            source_path: request.source_path.clone(),
            record_ordinal,
        });
    }
    let feature_contract_source_feature_id = source_feature_id.clone();
    Ok(ParsedClientTileFeature {
        feature_id: feature_id.clone(),
        source_feature_id,
        representative_point: parsed_geometry.representative_point,
        geometry_feature,
        feature_contract: GeoProviderTileFeatureContract {
            feature_id,
            source_instance_id: request.source_instance_id.clone(),
            source_feature_id: feature_contract_source_feature_id,
            decision_geometry_fidelity: GeoProviderGeometryFidelity::SourceFidelity,
            display_geometry_fidelity: Some(GeoProviderGeometryFidelity::DisplaySimplified),
            license_class: GeoLicenseClass::RestrictedLocalOnly,
            redaction_class: GeoProviderTileRedactionClass::LocalOnly,
            provenance: GeoProviderTileFeatureProvenance {
                source_instance_id: request.source_instance_id.clone(),
                source_path: request.source_path.clone(),
                source_digest: request.source_digest.clone(),
                source_record_id,
                record_ordinal,
                field_locators,
            },
        },
    })
}

fn parse_client_geojson_geometry(
    geometry: &JsonValue,
    decimal_places: u32,
) -> Result<ParsedClientGeometry, GeoGeometryError> {
    if geometry.is_null() {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::EmptyGeometry,
            "Client tile ingest feature geometry is null",
            std::iter::empty::<(&str, &str)>(),
        ));
    }
    let kind = geojson_type(geometry).ok_or_else(|| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidSourceEncoding,
            "Client tile ingest geometry is missing a GeoJSON type",
            std::iter::empty::<(&str, &str)>(),
        )
    })?;
    let coordinates = geometry.get("coordinates").ok_or_else(|| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::EmptyGeometry,
            "Client tile ingest geometry is missing coordinates",
            [("type", kind)],
        )
    })?;
    match kind {
        "Point" => {
            let coordinate =
                parse_geojson_position(coordinates, decimal_places, "$.geometry.coordinates")?;
            Ok(ParsedClientGeometry {
                geometry: GeoSourceGeometry::Point {
                    coordinate: coordinate.clone(),
                },
                representative_point: coordinate,
            })
        }
        "Polygon" => {
            let rings = coordinates.as_array().ok_or_else(|| {
                GeoGeometryError::new(
                    GeoGeometryErrorCode::InvalidSourceEncoding,
                    "GeoJSON Polygon coordinates must be an array of rings",
                    std::iter::empty::<(&str, &str)>(),
                )
            })?;
            let (exterior, holes, representative_points) =
                parse_geojson_polygon_rings(rings, decimal_places)?;
            Ok(ParsedClientGeometry {
                geometry: GeoSourceGeometry::Polygon { exterior, holes },
                representative_point: representative_point_from_vertices(
                    &representative_points,
                    decimal_places,
                )?,
            })
        }
        "MultiPolygon" => {
            let polygons = coordinates.as_array().ok_or_else(|| {
                GeoGeometryError::new(
                    GeoGeometryErrorCode::InvalidSourceEncoding,
                    "GeoJSON MultiPolygon coordinates must be an array of polygons",
                    std::iter::empty::<(&str, &str)>(),
                )
            })?;
            if polygons.is_empty() {
                return Err(GeoGeometryError::new(
                    GeoGeometryErrorCode::EmptyGeometry,
                    "GeoJSON MultiPolygon must contain at least one polygon",
                    std::iter::empty::<(&str, &str)>(),
                ));
            }
            let mut source_polygons = Vec::with_capacity(polygons.len());
            let mut representative_points = Vec::new();
            for polygon in polygons {
                let rings = polygon.as_array().ok_or_else(|| {
                    GeoGeometryError::new(
                        GeoGeometryErrorCode::InvalidSourceEncoding,
                        "GeoJSON MultiPolygon members must be arrays of rings",
                        std::iter::empty::<(&str, &str)>(),
                    )
                })?;
                let (exterior, holes, mut polygon_representative_points) =
                    parse_geojson_polygon_rings(rings, decimal_places)?;
                representative_points.append(&mut polygon_representative_points);
                source_polygons.push(GeoSourcePolygon { exterior, holes });
            }
            Ok(ParsedClientGeometry {
                geometry: GeoSourceGeometry::MultiPolygon {
                    polygons: source_polygons,
                },
                representative_point: representative_point_from_vertices(
                    &representative_points,
                    decimal_places,
                )?,
            })
        }
        value => Err(GeoGeometryError::new(
            GeoGeometryErrorCode::UnsupportedGeometryType,
            "Client tile ingest supports GeoJSON Point, Polygon, and MultiPolygon geometries",
            [("type", value)],
        )),
    }
}

fn parse_geojson_polygon_rings(
    rings: &[JsonValue],
    decimal_places: u32,
) -> Result<
    (
        Vec<GeoSourcePointDecimal>,
        Vec<Vec<GeoSourcePointDecimal>>,
        Vec<GeoSourcePointDecimal>,
    ),
    GeoGeometryError,
> {
    if rings.is_empty() {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::EmptyGeometry,
            "GeoJSON Polygon must contain an exterior ring",
            std::iter::empty::<(&str, &str)>(),
        ));
    }
    let exterior = parse_geojson_ring(&rings[0], decimal_places, "$.geometry.coordinates[0]")?;
    let mut representative_points = Vec::new();
    append_representative_ring_points(&exterior, &mut representative_points, decimal_places)?;
    let mut holes = Vec::with_capacity(rings.len().saturating_sub(1));
    for (index, ring) in rings.iter().enumerate().skip(1) {
        holes.push(parse_geojson_ring(
            ring,
            decimal_places,
            &format!("$.geometry.coordinates[{index}]"),
        )?);
    }
    Ok((exterior, holes, representative_points))
}

fn parse_geojson_ring(
    value: &JsonValue,
    decimal_places: u32,
    context: &str,
) -> Result<Vec<GeoSourcePointDecimal>, GeoGeometryError> {
    let coordinates = value.as_array().ok_or_else(|| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidSourceEncoding,
            "GeoJSON polygon ring must be an array of positions",
            [("context", context)],
        )
    })?;
    coordinates
        .iter()
        .enumerate()
        .map(|(index, position)| {
            parse_geojson_position(position, decimal_places, &format!("{context}[{index}]"))
        })
        .collect()
}

fn parse_geojson_position(
    value: &JsonValue,
    decimal_places: u32,
    context: &str,
) -> Result<GeoSourcePointDecimal, GeoGeometryError> {
    let coordinates = value.as_array().ok_or_else(|| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidSourceEncoding,
            "GeoJSON position must be a two-number longitude/latitude array",
            [("context", context)],
        )
    })?;
    if coordinates.len() != 2 {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::UnsupportedGeometryType,
            "Client tile ingest supports only 2D GeoJSON coordinates",
            [
                ("context", context.to_string()),
                ("coordinate_count", coordinates.len().to_string()),
            ],
        ));
    }
    let point = GeoSourcePointDecimal {
        x: geojson_number_decimal(&coordinates[0], "longitude", context)?,
        y: geojson_number_decimal(&coordinates[1], "latitude", context)?,
    };
    parse_fixed_decimal("longitude", &point.x, decimal_places)?;
    parse_fixed_decimal("latitude", &point.y, decimal_places)?;
    Ok(point)
}

fn geojson_number_decimal(
    value: &JsonValue,
    axis: &str,
    context: &str,
) -> Result<String, GeoGeometryError> {
    let number = value.as_number().ok_or_else(|| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidCoordinate,
            "GeoJSON coordinates must be JSON numbers",
            [("axis", axis), ("context", context)],
        )
    })?;
    Ok(number.to_string())
}

fn append_representative_ring_points(
    ring: &[GeoSourcePointDecimal],
    representative_points: &mut Vec<GeoSourcePointDecimal>,
    decimal_places: u32,
) -> Result<(), GeoGeometryError> {
    let mut end = ring.len();
    if ring.len() > 1 && source_points_equal_fixed(&ring[0], &ring[ring.len() - 1], decimal_places)?
    {
        end = end.saturating_sub(1);
    }
    representative_points.extend(ring[..end].iter().cloned());
    Ok(())
}

fn source_points_equal_fixed(
    left: &GeoSourcePointDecimal,
    right: &GeoSourcePointDecimal,
    decimal_places: u32,
) -> Result<bool, GeoGeometryError> {
    Ok(parse_fixed_decimal("longitude", &left.x, decimal_places)?
        == parse_fixed_decimal("longitude", &right.x, decimal_places)?
        && parse_fixed_decimal("latitude", &left.y, decimal_places)?
            == parse_fixed_decimal("latitude", &right.y, decimal_places)?)
}

fn representative_point_from_vertices(
    vertices: &[GeoSourcePointDecimal],
    decimal_places: u32,
) -> Result<GeoSourcePointDecimal, GeoGeometryError> {
    if vertices.is_empty() {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::EmptyGeometry,
            "Client tile ingest cannot derive a representative point from empty geometry",
            std::iter::empty::<(&str, &str)>(),
        ));
    }
    let mut x_sum = 0_i128;
    let mut y_sum = 0_i128;
    for point in vertices {
        x_sum = x_sum
            .checked_add(i128::from(parse_fixed_decimal(
                "longitude",
                &point.x,
                decimal_places,
            )?))
            .ok_or_else(|| GeoGeometryError::overflow("representative longitude sum"))?;
        y_sum = y_sum
            .checked_add(i128::from(parse_fixed_decimal(
                "latitude",
                &point.y,
                decimal_places,
            )?))
            .ok_or_else(|| GeoGeometryError::overflow("representative latitude sum"))?;
    }
    let count = usize_to_u64(vertices.len(), "representative vertex count")?;
    let (x, _) = round_rational_ties_even(x_sum, count)?;
    let (y, _) = round_rational_ties_even(y_sum, count)?;
    Ok(GeoSourcePointDecimal {
        x: format_fixed_decimal(x, decimal_places)?,
        y: format_fixed_decimal(y, decimal_places)?,
    })
}

fn h3_cell_for_decimal_point(
    point: &GeoSourcePointDecimal,
    decimal_places: u32,
    resolution: Resolution,
) -> Result<String, GeoGeometryError> {
    let longitude = parse_fixed_decimal("longitude", &point.x, decimal_places)?;
    let latitude = parse_fixed_decimal("latitude", &point.y, decimal_places)?;
    let scale = pow10_i128(decimal_places)?;
    let longitude_degrees = (longitude as f64) / (scale as f64);
    let latitude_degrees = (latitude as f64) / (scale as f64);
    let point = LatLng::new(latitude_degrees, longitude_degrees).map_err(|error| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidCoordinate,
            "Client tile representative point could not enter h3o",
            [
                ("longitude", longitude.to_string()),
                ("latitude", latitude.to_string()),
                ("error", error.to_string()),
            ],
        )
    })?;
    Ok(point.to_cell(resolution).to_string())
}

fn parse_client_supplemental_cells(
    request: &GeoClientTileIngestRequest,
    feature: &JsonValue,
    resolution: Resolution,
) -> Result<Vec<String>, GeoGeometryError> {
    let Some(property) = &request.supplemental_h3_cells_property else {
        return Ok(Vec::new());
    };
    let properties = feature_properties(feature)?;
    let Some(value) = properties.get(property) else {
        return Ok(Vec::new());
    };
    let cells = value.as_array().ok_or_else(|| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Client tile supplemental H3 coverage property must be an array of cells",
            [("property", property.as_str())],
        )
    })?;
    if cells.is_empty() {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Client tile supplemental H3 coverage must not be empty when declared",
            [("property", property.as_str())],
        ));
    }
    let mut parsed = Vec::with_capacity(cells.len());
    for value in cells {
        let Some(cell) = value.as_str() else {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidTileContract,
                "Client tile supplemental H3 coverage cells must be strings",
                [("property", property.as_str())],
            ));
        };
        parsed.push(parse_client_h3_cell(property, cell, resolution)?);
    }
    parsed.sort();
    parsed.dedup();
    Ok(parsed)
}

fn feature_properties(
    feature: &JsonValue,
) -> Result<&serde_json::Map<String, JsonValue>, GeoGeometryError> {
    feature
        .get("properties")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| {
            GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidSourceProvenance,
                "Client tile ingest GeoJSON Feature must carry an object properties map",
                std::iter::empty::<(&str, &str)>(),
            )
        })
}

fn feature_property_text(
    properties: &serde_json::Map<String, JsonValue>,
    property: &str,
    field: &str,
) -> Result<String, GeoGeometryError> {
    let value = properties.get(property).ok_or_else(|| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidSourceProvenance,
            "Client tile ingest feature is missing a required property",
            [("field", field), ("property", property)],
        )
    })?;
    json_scalar_text(value)
}

fn json_scalar_text(value: &JsonValue) -> Result<String, GeoGeometryError> {
    let text = if let Some(value) = value.as_str() {
        value.to_string()
    } else if let Some(value) = value.as_number() {
        value.to_string()
    } else {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidSourceProvenance,
            "Client tile ingest identifiers must be string or number scalars",
            std::iter::empty::<(&str, &str)>(),
        ));
    };
    validate_provider_text("client_identifier", &text)?;
    Ok(text)
}

fn geojson_type(value: &JsonValue) -> Option<&str> {
    value.get("type").and_then(JsonValue::as_str)
}

fn client_alias_namespace(
    identifier: &GeoClientTileVendorIdentifier,
) -> Result<String, GeoGeometryError> {
    validate_provider_text("vendor_identifier.issuer", &identifier.issuer)?;
    validate_provider_text("vendor_identifier.role", &identifier.role)?;
    Ok(format!("{}:{}", identifier.issuer, identifier.role))
}

fn increment_refusal_count(
    counts: &mut BTreeMap<GeoGeometryErrorCode, u64>,
    code: GeoGeometryErrorCode,
) -> Result<(), GeoGeometryError> {
    let entry = counts.entry(code).or_insert(0);
    *entry = checked_add_u64(*entry, 1, "client tile refusal count")?;
    Ok(())
}

fn checked_add_u64(left: u64, right: u64, context: &str) -> Result<u64, GeoGeometryError> {
    left.checked_add(right)
        .ok_or_else(|| GeoGeometryError::overflow(context))
}

fn usize_to_u64(value: usize, context: &str) -> Result<u64, GeoGeometryError> {
    u64::try_from(value).map_err(|_| GeoGeometryError::overflow(context))
}

fn canonicalize_provider_sources(
    sources: &mut [GeoProviderTileSource],
) -> Result<BTreeSet<String>, GeoGeometryError> {
    if sources.is_empty() {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidSourceProvenance,
            "Provider tile build must declare at least one source",
            std::iter::empty::<(&str, &str)>(),
        ));
    }
    sources.sort_by(|left, right| left.source_instance_id.cmp(&right.source_instance_id));

    let mut source_ids = BTreeSet::new();
    for source in sources {
        validate_provider_contract_identifier(
            "sources[].source_instance_id",
            &source.source_instance_id,
        )?;
        validate_provider_contract_identifier("sources[].release_id", &source.release_id)?;
        validate_provider_text("sources[].license_expression", &source.license_expression)?;
        validate_provider_blake3("sources[].release_digest", &source.release_digest)?;
        if !source_ids.insert(source.source_instance_id.clone()) {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidSourceProvenance,
                "Provider tile sources repeat a source_instance_id",
                [("source_instance_id", source.source_instance_id.as_str())],
            ));
        }
        if source.attribution_required {
            let Some(text) = &source.attribution_text else {
                return Err(GeoGeometryError::new(
                    GeoGeometryErrorCode::InvalidLicensePosture,
                    "Attribution-required provider sources must carry attribution text",
                    [("source_instance_id", source.source_instance_id.as_str())],
                ));
            };
            validate_provider_text("sources[].attribution_text", text)?;
        }
        validate_provider_source_provenance(source)?;
    }
    Ok(source_ids)
}

fn validate_provider_source_provenance(
    source: &GeoProviderTileSource,
) -> Result<(), GeoGeometryError> {
    match &source.provenance {
        GeoProviderTileSourceProvenance::CanonFullProvenance {
            source_path,
            source_digest,
            source_record_count,
        } => {
            validate_provider_text("sources[].provenance.source_path", source_path)?;
            validate_provider_blake3("sources[].provenance.source_digest", source_digest)?;
            if *source_record_count == 0 {
                return Err(GeoGeometryError::new(
                    GeoGeometryErrorCode::InvalidSourceProvenance,
                    "Canon-full provider source provenance must declare a positive record count",
                    [("source_instance_id", source.source_instance_id.as_str())],
                ));
            }
        }
        GeoProviderTileSourceProvenance::ClientDeclared {
            vendor,
            vintage,
            source_crs,
            coverage_extent,
            mutual_exclusivity_declared: _,
        } => {
            validate_provider_text("sources[].provenance.vendor", vendor)?;
            validate_provider_text("sources[].provenance.vintage", vintage)?;
            validate_provider_text("sources[].provenance.source_crs", source_crs)?;
            validate_provider_text("sources[].provenance.coverage_extent", coverage_extent)?;
        }
    }
    Ok(())
}

fn canonicalize_provider_subset(
    subset: &mut GeoProviderTileSubsetPredicate,
    tile_id: &str,
    source_ids: &BTreeSet<String>,
) -> Result<(), GeoGeometryError> {
    validate_provider_contract_identifier("subset.predicate_id", &subset.predicate_id)?;
    validate_provider_contract_identifier("subset.center_cell", &subset.center_cell)?;
    if subset.center_cell != tile_id {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Provider tile subset center cell must match the tile id",
            [
                ("tile_id", tile_id),
                ("center_cell", subset.center_cell.as_str()),
            ],
        ));
    }
    let _resolution = Resolution::try_from(subset.h3_resolution).map_err(|error| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Provider tile subset declares an unsupported H3 resolution",
            [
                (
                    "h3_resolution".to_string(),
                    subset.h3_resolution.to_string(),
                ),
                ("error".to_string(), error.to_string()),
            ],
        )
    })?;
    let _center = parse_provider_h3_cell(
        "subset.center_cell",
        &subset.center_cell,
        subset.h3_resolution,
    )?;
    if subset.work_cells.is_empty() || subset.work_cells.len() > MAX_PROVIDER_TILE_WORK_CELLS {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Provider tile subset must declare a bounded nonempty H3 work-cell set",
            [
                (
                    "work_cells".to_string(),
                    subset.work_cells.len().to_string(),
                ),
                (
                    "maximum".to_string(),
                    MAX_PROVIDER_TILE_WORK_CELLS.to_string(),
                ),
            ],
        ));
    }

    subset.work_cells.sort();
    let mut work_cells = BTreeSet::new();
    for cell in &subset.work_cells {
        validate_provider_contract_identifier("subset.work_cells[]", cell)?;
        parse_provider_h3_cell("subset.work_cells[]", cell, subset.h3_resolution)?;
        if !work_cells.insert(cell.clone()) {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidTileContract,
                "Provider tile subset repeats an H3 work cell",
                [("h3_cell", cell.as_str())],
            ));
        }
    }
    if !work_cells.contains(&subset.center_cell) {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Provider tile subset work cells must include the center cell",
            [("center_cell", subset.center_cell.as_str())],
        ));
    }

    subset.source_coverages.sort_by(|left, right| {
        left.source_instance_id
            .cmp(&right.source_instance_id)
            .then_with(|| left.h3_cell.cmp(&right.h3_cell))
    });
    let mut coverage_pairs = BTreeSet::new();
    for coverage in &subset.source_coverages {
        validate_provider_contract_identifier(
            "subset.source_coverages[].source_instance_id",
            &coverage.source_instance_id,
        )?;
        validate_provider_contract_identifier(
            "subset.source_coverages[].h3_cell",
            &coverage.h3_cell,
        )?;
        if !source_ids.contains(&coverage.source_instance_id) {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidTileContract,
                "Provider tile coverage references an undeclared source",
                [("source_instance_id", coverage.source_instance_id.as_str())],
            ));
        }
        if !work_cells.contains(&coverage.h3_cell) {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidTileContract,
                "Provider tile coverage references a cell outside the declared work set",
                [("h3_cell", coverage.h3_cell.as_str())],
            ));
        }
        parse_provider_h3_cell(
            "subset.source_coverages[].h3_cell",
            &coverage.h3_cell,
            subset.h3_resolution,
        )?;
        if !coverage_pairs.insert((
            coverage.source_instance_id.clone(),
            coverage.h3_cell.clone(),
        )) {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidTileContract,
                "Provider tile subset repeats coverage for a source/cell pair",
                [
                    ("source_instance_id", coverage.source_instance_id.as_str()),
                    ("h3_cell", coverage.h3_cell.as_str()),
                ],
            ));
        }
    }

    for source_id in source_ids {
        for cell in &work_cells {
            if !coverage_pairs.contains(&(source_id.clone(), cell.clone())) {
                return Err(GeoGeometryError::new(
                    GeoGeometryErrorCode::InvalidTileContract,
                    "Provider tile subset must declare coverage for every source/cell intersection",
                    [
                        ("source_instance_id", source_id.as_str()),
                        ("h3_cell", cell.as_str()),
                    ],
                ));
            }
        }
    }
    Ok(())
}

fn canonicalize_provider_license_posture(
    posture: &mut GeoProviderTileLicensePosture,
    sources: &[GeoProviderTileSource],
) -> Result<(), GeoGeometryError> {
    validate_provider_contract_identifier("license_posture.posture_id", &posture.posture_id)?;
    validate_provider_text(
        "license_posture.output_license_expression",
        &posture.output_license_expression,
    )?;
    validate_provider_text(
        "license_posture.redistribution_notice",
        &posture.redistribution_notice,
    )?;

    sort_and_reject_duplicate_strings(
        "license_posture.attribution_requirements",
        &mut posture.attribution_requirements,
    )?;
    sort_and_reject_duplicate_strings(
        "license_posture.client_restricted_source_ids",
        &mut posture.client_restricted_source_ids,
    )?;

    let source_ids = sources
        .iter()
        .map(|source| source.source_instance_id.as_str())
        .collect::<BTreeSet<_>>();
    for source_id in &posture.client_restricted_source_ids {
        if !source_ids.contains(source_id.as_str()) {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidLicensePosture,
                "Provider tile license posture references an undeclared restricted source",
                [("source_instance_id", source_id.as_str())],
            ));
        }
    }
    for source in sources {
        if source.attribution_required {
            let attribution_text = source.attribution_text.as_ref().ok_or_else(|| {
                GeoGeometryError::new(
                    GeoGeometryErrorCode::InvalidLicensePosture,
                    "Attribution-required provider source is missing attribution text",
                    [("source_instance_id", source.source_instance_id.as_str())],
                )
            })?;
            if !posture
                .attribution_requirements
                .iter()
                .any(|requirement| requirement == attribution_text)
            {
                return Err(GeoGeometryError::new(
                    GeoGeometryErrorCode::InvalidLicensePosture,
                    "Provider tile license posture must carry every required attribution",
                    [("source_instance_id", source.source_instance_id.as_str())],
                ));
            }
        }
        let client_declared = matches!(
            source.provenance,
            GeoProviderTileSourceProvenance::ClientDeclared { .. }
        );
        if client_declared
            && !posture
                .client_restricted_source_ids
                .iter()
                .any(|restricted| restricted == &source.source_instance_id)
        {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidLicensePosture,
                "Client-declared provider sources must remain inside the restricted-source boundary",
                [("source_instance_id", source.source_instance_id.as_str())],
            ));
        }
    }
    Ok(())
}

fn canonicalize_provider_feature_contracts(
    feature_contracts: &mut [GeoProviderTileFeatureContract],
    geometry_features: &[GeoGeometryFeature],
    sources: &[GeoProviderTileSource],
    allow_vendor_simplified_decision_geometry: bool,
) -> Result<(), GeoGeometryError> {
    if feature_contracts.len() != geometry_features.len() {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Provider tile feature contracts must cover every materialized geometry feature",
            [
                ("contracts".to_string(), feature_contracts.len().to_string()),
                ("geometry".to_string(), geometry_features.len().to_string()),
            ],
        ));
    }
    feature_contracts.sort_by(|left, right| left.feature_id.cmp(&right.feature_id));

    let source_by_id = sources
        .iter()
        .map(|source| (source.source_instance_id.as_str(), source))
        .collect::<BTreeMap<_, _>>();
    let geometry_ids = geometry_features
        .iter()
        .map(|feature| feature.feature_id.as_str())
        .collect::<Vec<_>>();
    let mut contract_ids = BTreeSet::new();
    for (index, contract) in feature_contracts.iter_mut().enumerate() {
        validate_provider_contract_identifier("features[].feature_id", &contract.feature_id)?;
        validate_provider_contract_identifier(
            "features[].source_instance_id",
            &contract.source_instance_id,
        )?;
        validate_provider_contract_identifier(
            "features[].source_feature_id",
            &contract.source_feature_id,
        )?;
        if !contract_ids.insert(contract.feature_id.clone()) {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidTileContract,
                "Provider tile feature contracts repeat a feature id",
                [("feature_id", contract.feature_id.as_str())],
            ));
        }
        let Some(expected_feature_id) = geometry_ids.get(index) else {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidTileContract,
                "Provider tile feature contract index exceeded geometry features",
                [("feature_id", contract.feature_id.as_str())],
            ));
        };
        if contract.feature_id != *expected_feature_id {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidTileContract,
                "Provider tile feature contracts must match the materialized geometry feature set",
                [
                    ("contract_feature_id", contract.feature_id.as_str()),
                    ("geometry_feature_id", *expected_feature_id),
                ],
            ));
        }
        let Some(source) = source_by_id.get(contract.source_instance_id.as_str()) else {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidSourceProvenance,
                "Provider tile feature references an undeclared source",
                [("source_instance_id", contract.source_instance_id.as_str())],
            ));
        };
        validate_provider_feature_fidelity(contract, allow_vendor_simplified_decision_geometry)?;
        validate_provider_feature_license(contract, source)?;
        canonicalize_provider_feature_provenance(contract, source)?;
    }
    Ok(())
}

fn validate_provider_feature_fidelity(
    contract: &GeoProviderTileFeatureContract,
    allow_vendor_simplified_decision_geometry: bool,
) -> Result<(), GeoGeometryError> {
    match contract.decision_geometry_fidelity {
        GeoProviderGeometryFidelity::SourceFidelity => Ok(()),
        GeoProviderGeometryFidelity::VendorSimplified => {
            if allow_vendor_simplified_decision_geometry {
                Ok(())
            } else {
                Err(GeoGeometryError::new(
                    GeoGeometryErrorCode::InvalidTileContract,
                    "Vendor-simplified geometry cannot be used as decision geometry without explicit acknowledgement",
                    [("feature_id", contract.feature_id.as_str())],
                ))
            }
        }
        GeoProviderGeometryFidelity::DisplaySimplified => Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Display-simplified geometry cannot be used as decision geometry",
            [("feature_id", contract.feature_id.as_str())],
        )),
    }
}

fn validate_provider_feature_license(
    contract: &GeoProviderTileFeatureContract,
    source: &GeoProviderTileSource,
) -> Result<(), GeoGeometryError> {
    if source.attribution_required
        && contract.redaction_class == GeoProviderTileRedactionClass::Shareable
    {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidLicensePosture,
            "Attribution-required provider features must be marked shareable_attribution_required or local_only",
            [
                ("feature_id", contract.feature_id.as_str()),
                ("source_instance_id", source.source_instance_id.as_str()),
            ],
        ));
    }
    if contract.license_class == GeoLicenseClass::Unknown
        && contract.redaction_class != GeoProviderTileRedactionClass::LocalOnly
    {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidLicensePosture,
            "Unknown-license provider features must remain local_only",
            [
                ("feature_id", contract.feature_id.as_str()),
                ("source_instance_id", source.source_instance_id.as_str()),
            ],
        ));
    }
    Ok(())
}

fn canonicalize_provider_feature_provenance(
    contract: &mut GeoProviderTileFeatureContract,
    source: &GeoProviderTileSource,
) -> Result<(), GeoGeometryError> {
    let provenance = &mut contract.provenance;
    validate_provider_contract_identifier(
        "features[].provenance.source_instance_id",
        &provenance.source_instance_id,
    )?;
    if provenance.source_instance_id != contract.source_instance_id {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidSourceProvenance,
            "Provider tile feature provenance must agree with the feature source binding",
            [
                ("feature_id", contract.feature_id.as_str()),
                (
                    "provenance_source_instance_id",
                    provenance.source_instance_id.as_str(),
                ),
                ("source_instance_id", contract.source_instance_id.as_str()),
            ],
        ));
    }
    validate_provider_text("features[].provenance.source_path", &provenance.source_path)?;
    validate_provider_blake3(
        "features[].provenance.source_digest",
        &provenance.source_digest,
    )?;
    validate_provider_contract_identifier(
        "features[].provenance.source_record_id",
        &provenance.source_record_id,
    )?;
    if let GeoProviderTileSourceProvenance::CanonFullProvenance {
        source_path,
        source_digest,
        source_record_count: _,
    } = &source.provenance
        && (provenance.source_path != *source_path || provenance.source_digest != *source_digest)
    {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidSourceProvenance,
            "Canon-full feature provenance must point at the declared source artifact",
            [
                ("feature_id", contract.feature_id.as_str()),
                ("source_instance_id", source.source_instance_id.as_str()),
            ],
        ));
    }
    if provenance.field_locators.is_empty() {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidSourceProvenance,
            "Provider tile feature provenance must carry field-level locators",
            [("feature_id", contract.feature_id.as_str())],
        ));
    }
    provenance.field_locators.sort();
    let mut fields = BTreeSet::new();
    for locator in &provenance.field_locators {
        validate_provider_text(
            "features[].provenance.field_locators[].field_path",
            &locator.field_path,
        )?;
        validate_provider_text(
            "features[].provenance.field_locators[].source_path",
            &locator.source_path,
        )?;
        if locator.record_ordinal != provenance.record_ordinal {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidSourceProvenance,
                "Provider tile field locators must bind the same record ordinal as the feature provenance",
                [
                    ("feature_id".to_string(), contract.feature_id.clone()),
                    (
                        "record_ordinal".to_string(),
                        provenance.record_ordinal.to_string(),
                    ),
                    (
                        "locator_record_ordinal".to_string(),
                        locator.record_ordinal.to_string(),
                    ),
                ],
            ));
        }
        if !fields.insert(locator.field_path.clone()) {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidSourceProvenance,
                "Provider tile feature provenance repeats a field locator",
                [
                    ("feature_id", contract.feature_id.as_str()),
                    ("field_path", locator.field_path.as_str()),
                ],
            ));
        }
    }
    Ok(())
}

fn provider_geometry_tile_content_blake3(
    geometry_tile: &GeoGeometryTileArtifact,
    provider_tile: &GeoProviderTileContract,
) -> Result<String, GeoGeometryError> {
    #[derive(Serialize)]
    struct DigestProjection<'a> {
        tile_version: &'a str,
        frame: &'a GeoLocalFrameContract,
        features: &'a [GeoGeometryFeature],
        total_canonical_vertices: u64,
        geometry_bytes: u64,
        max_vertices_per_geometry: u64,
        max_geometry_bytes_per_tile: u64,
        provider_tile_version: &'a str,
        tile_id: &'a str,
        databook_decision: GeoProviderTileDataBookDecision,
        subset: &'a GeoProviderTileSubsetPredicate,
        sources: &'a [GeoProviderTileSource],
        license_posture: &'a GeoProviderTileLicensePosture,
        feature_contracts: &'a [GeoProviderTileFeatureContract],
        client_ingest: &'a Option<GeoClientTileIngestReport>,
    }

    let projection = DigestProjection {
        tile_version: &geometry_tile.version,
        frame: &geometry_tile.frame,
        features: &geometry_tile.features,
        total_canonical_vertices: geometry_tile.total_canonical_vertices,
        geometry_bytes: geometry_tile.geometry_bytes,
        max_vertices_per_geometry: geometry_tile.max_vertices_per_geometry,
        max_geometry_bytes_per_tile: geometry_tile.max_geometry_bytes_per_tile,
        provider_tile_version: &provider_tile.version,
        tile_id: &provider_tile.tile_id,
        databook_decision: provider_tile.databook_decision,
        subset: &provider_tile.subset,
        sources: &provider_tile.sources,
        license_posture: &provider_tile.license_posture,
        feature_contracts: &provider_tile.features,
        client_ingest: &provider_tile.client_ingest,
    };
    let bytes = serde_json::to_vec(&projection).map_err(|error| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::Serialization,
            "Provider tile digest serialization failed",
            [("error", error.to_string())],
        )
    })?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn sort_and_reject_duplicate_strings(
    field: &str,
    values: &mut [String],
) -> Result<(), GeoGeometryError> {
    values.sort();
    let mut seen = BTreeSet::new();
    for value in values.iter() {
        validate_provider_text(field, value)?;
        if !seen.insert(value.clone()) {
            return Err(GeoGeometryError::new(
                GeoGeometryErrorCode::InvalidLicensePosture,
                "Provider tile contract contains a duplicate string value",
                [("field", field), ("value", value.as_str())],
            ));
        }
    }
    Ok(())
}

fn parse_provider_h3_cell(
    field: &str,
    value: &str,
    expected_resolution: u8,
) -> Result<CellIndex, GeoGeometryError> {
    let cell = CellIndex::from_str(value).map_err(|error| {
        GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Provider tile subset declares an invalid H3 cell",
            [
                ("field".to_string(), field.to_string()),
                ("value".to_string(), value.to_string()),
                ("error".to_string(), error.to_string()),
            ],
        )
    })?;
    if u8::from(cell.resolution()) != expected_resolution {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Provider tile subset cell resolution does not match the declared resolution",
            [
                ("field".to_string(), field.to_string()),
                ("value".to_string(), value.to_string()),
                (
                    "actual_resolution".to_string(),
                    u8::from(cell.resolution()).to_string(),
                ),
                (
                    "expected_resolution".to_string(),
                    expected_resolution.to_string(),
                ),
            ],
        ));
    }
    Ok(cell)
}

fn validate_provider_contract_identifier(field: &str, value: &str) -> Result<(), GeoGeometryError> {
    if value.is_empty() || value.trim() != value || value.len() > MAX_IDENTIFIER_BYTES {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Provider tile identifiers must be nonempty, unpadded UTF-8 within the byte limit",
            [
                ("field".to_string(), field.to_string()),
                ("length".to_string(), value.len().to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_provider_text(field: &str, value: &str) -> Result<(), GeoGeometryError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidTileContract,
            "Provider tile text fields must be nonempty and unpadded",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_provider_blake3(field: &str, value: &str) -> Result<(), GeoGeometryError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoGeometryError::new(
            GeoGeometryErrorCode::InvalidSourceDigest,
            "Provider tile digests must be lowercase BLAKE3 hex",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
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
