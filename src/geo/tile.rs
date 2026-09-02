#![forbid(unsafe_code)]

//! Deterministic H3 work-unit ownership and cross-boundary reconciliation.
//!
//! H3 is a blocking and ownership index here, never a geometric truth
//! predicate. Upstream ingest supplies one declared home cell per feature.
//! Canon validates the cell encoding and work-unit consistency, not the
//! coordinate-to-cell truth, then builds a bounded center-plus-halo work unit.
//! A decision emits only from the minimum member home cell. Adjacent work units
//! may observe the same decision, but reconciliation either produces one owned
//! decision or refuses an orphan/non-confluent boundary result.

use super::{
    GeoControlEntityLevel, GeoControlRelation, GeoNativeEntityScope, GeoPlanInventoryRef,
    GeoRegionalInventory, GeoRegionalSourceInstance, GeoSourceRelease,
    canonicalize_regional_inventory, geometry_value::parse_fixed_decimal,
    regional_inventory_planning_hash, regional_inventory_semantic_hash,
};
use h3o::{CellIndex, LatLng, Resolution};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    str::FromStr,
};

pub const CANON_GEO_TILE_WORK_REQUEST_VERSION: &str = "canon_geo_tile_work_request.v1";
pub const CANON_GEO_TILE_WORK_UNIT_VERSION: &str = "canon_geo_tile_work_unit.v1";
pub const CANON_GEO_HOME_CELL_ROWS_VERSION: &str = "canon_geo_home_cell_rows.v1";
pub const CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION: &str = "canon_geo_home_cell_assignment.v1";
pub const CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION: &str =
    "canon_geo_tile_reconciliation_request.v1";
pub const CANON_GEO_TILE_RECONCILIATION_VERSION: &str = "canon_geo_tile_reconciliation.v1";

const MAX_IDENTIFIER_BYTES: usize = 256;
const MAX_FEATURES_PER_WORK_UNIT: u64 = 1_000_000;
const MAX_WORK_CELLS: u64 = 100_000;
const MAX_RECONCILIATION_BATCHES: u64 = 100_000;
const MAX_RECONCILIATION_PROPOSALS: u64 = 1_000_000;
const MAX_MEMBERS_PER_DECISION: u64 = 100_000;
const MAX_HOME_CELL_ROWS: u64 = 1_000_000;
const MAX_COORDINATE_DECIMAL_PLACES: u32 = 9;
const MAX_STABILITY_RADIUS_FIXED: u64 = 10_000_000;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileSourceBinding {
    pub source_instance_id: String,
    pub release: GeoSourceRelease,
    pub native_scope: GeoNativeEntityScope,
    /// Declared regional-inventory reference in the same shape used by a Geo
    /// plan. Reconciliation verifies this reference against supplied inventory
    /// bytes before it grants any inventory-relative stable-identity authority.
    pub inventory_ref: GeoPlanInventoryRef,
}

impl GeoTileSourceBinding {
    pub const fn native_entity_level(&self) -> Option<GeoControlEntityLevel> {
        match &self.native_scope {
            GeoNativeEntityScope::NativeEntity { entity_level, .. } => Some(*entity_level),
            GeoNativeEntityScope::ObservationOnly => None,
        }
    }

    pub const fn may_contribute_stable_alias(&self) -> bool {
        self.native_scope.may_contribute_stable_alias()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileFeatureRef {
    pub source: GeoTileSourceBinding,
    pub feature_id: String,
    /// Canonical H3 cell containing the feature's declared representative
    /// point. Cell computation belongs to ingest; this contract makes the
    /// ownership input explicit and auditable.
    pub home_cell: String,
}

/// One offline representative-point row used to derive an H3 blocking and
/// ownership cell. The geometry digest and optional transform identifiers bind
/// the point to evidence; H3 itself is never admitted as geometric truth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoHomeCellRow {
    pub source: GeoTileSourceBinding,
    pub feature_id: String,
    pub source_record_id: String,
    pub geometry_sha256: String,
    pub representative_point_method: String,
    pub longitude: String,
    pub latitude: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform_execution_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform_definition_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_home_cell: Option<String>,
}

/// Offline rows exported from one or more release-pinned source snapshots.
/// Coordinates are fixed decimal text so request parsing never depends on a
/// JSON-number implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoHomeCellRowsRequest {
    pub version: String,
    pub coordinate_crs: String,
    pub coordinate_decimal_places: u32,
    pub h3_resolution: u8,
    /// Probe radius in exact fixed coordinate units. At nine decimal places,
    /// 1,000 units is one microdegree. All nine corner/axis/centre probes are
    /// retained as a sensitivity set rather than collapsed into a false claim
    /// that the representative point is world-exact.
    pub stability_radius_fixed: u64,
    pub rows: Vec<GeoHomeCellRow>,
    pub max_rows: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoHomeCellParity {
    NotClaimed,
    Match,
    Mismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRepresentativePointFixed {
    pub longitude: i64,
    pub latitude: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoHomeCellFeatureAssignment {
    pub source: GeoTileSourceBinding,
    pub feature_id: String,
    pub source_record_id: String,
    pub geometry_sha256: String,
    pub representative_point_method: String,
    pub representative_point_fixed: GeoRepresentativePointFixed,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform_execution_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform_definition_id: Option<String>,
    pub home_cell: String,
    /// Cells reached by the nine deterministic corner, axis, and center probes
    /// of the declared coordinate envelope. More than one cell is a sampled
    /// blocking-boundary sensitivity finding, not a second identity assertion
    /// or exhaustive coverage of the continuous envelope.
    pub stability_cells: Vec<String>,
    /// Smallest H3 grid radius from `home_cell` containing every retained
    /// stability probe. A work planner can compare this directly with its
    /// controlled halo rather than assuming one ring is always sufficient.
    pub minimum_stability_halo_k: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_home_cell: Option<String>,
    pub parity: GeoHomeCellParity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoHomeCellAssignmentSummary {
    pub total: u64,
    pub boundary_sensitive: u64,
    pub max_minimum_stability_halo_k: u32,
    pub claimed: u64,
    pub matches: u64,
    pub mismatches: u64,
    pub unclaimed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoHomeCellAssignmentArtifact {
    pub version: String,
    pub request_version: String,
    pub coordinate_crs: String,
    pub coordinate_decimal_places: u32,
    pub h3_resolution: u8,
    pub stability_radius_fixed: u64,
    pub features: Vec<GeoHomeCellFeatureAssignment>,
    pub tile_work_features: Vec<GeoTileFeatureRef>,
    pub summary: GeoHomeCellAssignmentSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileWorkRequest {
    pub version: String,
    pub center_cell: String,
    pub halo_k: u32,
    pub features: Vec<GeoTileFeatureRef>,
    pub max_features: u64,
    /// Maximum total cells in the center-plus-halo disk, including the center.
    pub max_work_cells: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoTilePlacement {
    Center,
    Halo,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileFeatureMembership {
    pub source: GeoTileSourceBinding,
    pub feature_id: String,
    pub home_cell: String,
    pub placement: GeoTilePlacement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileWorkUnitArtifact {
    pub version: String,
    pub request_version: String,
    pub center_cell: String,
    pub h3_resolution: u8,
    pub halo_k: u32,
    /// Deterministically sorted center-plus-halo cells, including center_cell.
    pub work_cells: Vec<String>,
    pub features: Vec<GeoTileFeatureMembership>,
    pub center_feature_count: u64,
    pub halo_feature_count: u64,
    pub max_features: u64,
    pub max_work_cells: u64,
    /// Domain-separated BLAKE3 digest of every preceding semantic work-unit
    /// field. This digest excludes itself and binds local proposals to the
    /// precise bounded section they declare; it does not prove an external
    /// solver actually consumed that section.
    pub work_unit_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileDecisionMember {
    pub source: GeoTileSourceBinding,
    pub feature_id: String,
    /// Candidate variable grain claimed by the local decision. This must equal
    /// the source's native entity level. Cross-level relations belong in typed
    /// relation artifacts, never in a retyped identity variable.
    pub candidate_entity_level: GeoControlEntityLevel,
    pub home_cell: String,
}

impl GeoTileDecisionMember {
    pub const fn may_contribute_stable_alias(&self) -> bool {
        self.source.may_contribute_stable_alias()
    }
}

/// Declared meaning of a locally solved decision payload.
///
/// Composition decisions may span entity levels and may consist entirely of
/// evidence-only candidates; they carry no alias-mint authority. Stable
/// identity decisions are same-level equivalence and require a stable-alias
/// participant at that exact level. Relation decisions are explicit
/// cross-level stitching assertions; `same_as` is intentionally not available
/// through that path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeoTileDecisionSemantics {
    Composition,
    StableIdentity {
        entity_level: GeoControlEntityLevel,
    },
    Relation {
        relation: GeoControlRelation,
        from_entity_level: GeoControlEntityLevel,
        to_entity_level: GeoControlEntityLevel,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileDecisionProposal {
    pub semantics: GeoTileDecisionSemantics,
    /// Must equal the canonical digest carried by this proposal's embedded
    /// work unit. This is a deterministic association check, not an execution
    /// receipt from an external solver.
    pub work_unit_blake3: String,
    /// Digest of the declared complete decision payload. Reconciliation does
    /// not interpret or merge payloads, or prove who produced them.
    pub payload_blake3: String,
    pub members: Vec<GeoTileDecisionMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileDecisionBatch {
    /// Exact bounded work unit the caller associates with the proposals below.
    /// Each proposal repeats and is checked against its canonical digest; no
    /// claim is made that an external solver actually consumed it.
    pub work_unit: GeoTileWorkUnitArtifact,
    pub proposals: Vec<GeoTileDecisionProposal>,
}

/// Complete inventory evidence used to validate StableAlias authority.
///
/// The inventory reference has the same shape as the reference embedded in a
/// Geo plan.
/// Reconciliation recomputes both inventory hashes and then matches every
/// stable-identity member to an exact source instance, release, and native
/// scope in the canonical inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileInventoryLineage {
    pub inventory_ref: GeoPlanInventoryRef,
    pub inventory: GeoRegionalInventory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileReconciliationRequest {
    pub version: String,
    pub halo_k: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory_lineage: Option<GeoTileInventoryLineage>,
    pub batches: Vec<GeoTileDecisionBatch>,
    pub max_batches: u64,
    pub max_proposals: u64,
    pub max_members_per_decision: u64,
    pub max_features_per_batch: u64,
    /// Maximum total cells in each center-plus-halo disk, including its center.
    pub max_work_cells_per_batch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileBatchReceipt {
    pub center_cell: String,
    pub work_unit_blake3: String,
    pub proposal_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoReconciledTileDecision {
    pub decision_id: String,
    pub owner_cell: String,
    pub semantics: GeoTileDecisionSemantics,
    /// Inventory-relative authority boundary for a stable-identity decision.
    /// Composition decisions omit this field because they mint no aliases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory_ref: Option<GeoPlanInventoryRef>,
    pub payload_blake3: String,
    pub members: Vec<GeoTileDecisionMember>,
    /// Number of center-cell batches that submitted the same declared
    /// semantics, canonical member set, and payload. This is not proof of
    /// independent solver execution.
    pub proposal_copies: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileRelationshipEndpointRef {
    pub entity_level: GeoControlEntityLevel,
    pub source: GeoTileSourceBinding,
    pub feature_id: String,
    pub home_cell: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileRelationshipEdge {
    pub relationship_id: String,
    pub decision_id: String,
    pub owner_cell: String,
    pub relation: GeoControlRelation,
    pub from: GeoTileRelationshipEndpointRef,
    pub to: GeoTileRelationshipEndpointRef,
    pub payload_blake3: String,
    pub proposal_copies: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoTileRelationshipAnchorSide {
    From,
    To,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileRelationshipGroup {
    pub group_id: String,
    pub relation: GeoControlRelation,
    pub from_entity_level: GeoControlEntityLevel,
    pub to_entity_level: GeoControlEntityLevel,
    pub anchor_side: GeoTileRelationshipAnchorSide,
    pub anchor: GeoTileRelationshipEndpointRef,
    pub related: Vec<GeoTileRelationshipEndpointRef>,
    pub relationship_ids: Vec<String>,
    pub related_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileReconciliationArtifact {
    pub version: String,
    pub request_version: String,
    pub h3_resolution: u8,
    pub halo_k: u32,
    /// Validated regional-inventory reference used by stable-identity
    /// decisions in this artifact, if any. This is inventory-relative
    /// authority, not a claim of external or world truth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory_ref: Option<GeoPlanInventoryRef>,
    pub batches: u64,
    pub input_proposals: u64,
    pub owned_decisions: u64,
    pub discarded_halo_proposals: u64,
    pub batch_receipts: Vec<GeoTileBatchReceipt>,
    pub decisions: Vec<GeoReconciledTileDecision>,
    /// Pairwise, typed relation decisions projected for consumers that need
    /// relationship facts without parsing decision semantics.
    pub relationships: Vec<GeoTileRelationshipEdge>,
    /// Deterministic adjacency projection over `relationships`. Both anchor
    /// sides are emitted so one-to-many and many-to-one structure is explicit
    /// without collapsing the underlying pairwise relation facts.
    pub relationship_groups: Vec<GeoTileRelationshipGroup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoTileErrorCode {
    UnsupportedVersion,
    InvalidInput,
    InvalidCoordinate,
    InvalidSourceDigest,
    IncompatibleSourceBinding,
    InvalidH3Cell,
    ResolutionMismatch,
    HaloBudgetExceeded,
    FeatureBudgetExceeded,
    ReconciliationBudgetExceeded,
    FeatureOutsideHalo,
    DuplicateFeature,
    DuplicateCenter,
    InvalidWorkUnit,
    InvalidDecision,
    InvalidCandidateMember,
    InvalidInventoryLineage,
    NonConfluentDecision,
    MissingOwnerWorkUnit,
    OrphanedDecision,
    ArithmeticOverflow,
    Serialization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoTileError {
    pub code: GeoTileErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoTileError {
    fn new(
        code: GeoTileErrorCode,
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

    fn overflow(field: &str) -> Self {
        Self::new(
            GeoTileErrorCode::ArithmeticOverflow,
            "Geo tile arithmetic exceeded checked integer bounds",
            [("field", field)],
        )
    }
}

impl fmt::Display for GeoTileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoTileError {}

#[derive(Debug)]
struct DecisionAccumulator {
    decision_scope_blake3: String,
    semantics: GeoTileDecisionSemantics,
    payload_blake3: String,
    owner_cell: CellIndex,
    members: Vec<GeoTileDecisionMember>,
    proposal_centers: BTreeSet<CellIndex>,
}

type SourceFeatureKey = (GeoTileSourceBinding, String);
type FeatureHomeCells = BTreeMap<SourceFeatureKey, CellIndex>;
type SourceBindings = BTreeMap<String, GeoTileSourceBinding>;

#[derive(Debug)]
struct ValidatedWorkUnit {
    center: CellIndex,
    feature_home_cells: FeatureHomeCells,
    source_bindings: SourceBindings,
    blake3: String,
}

struct ValidatedInventoryLineage {
    inventory_ref: GeoPlanInventoryRef,
    sources: BTreeMap<String, GeoRegionalSourceInstance>,
}

type RelationshipGroupKey = (
    GeoControlRelation,
    GeoControlEntityLevel,
    GeoControlEntityLevel,
    GeoTileRelationshipAnchorSide,
    GeoTileRelationshipEndpointRef,
);

#[derive(Debug)]
struct RelationshipGroupAccumulator {
    relation: GeoControlRelation,
    from_entity_level: GeoControlEntityLevel,
    to_entity_level: GeoControlEntityLevel,
    anchor_side: GeoTileRelationshipAnchorSide,
    anchor: GeoTileRelationshipEndpointRef,
    related: BTreeSet<GeoTileRelationshipEndpointRef>,
    relationship_ids: BTreeSet<String>,
}

#[derive(Serialize)]
struct GeoTileWorkUnitDigestProjection<'a> {
    version: &'a str,
    request_version: &'a str,
    center_cell: &'a str,
    h3_resolution: u8,
    halo_k: u32,
    work_cells: &'a [String],
    features: &'a [GeoTileFeatureMembership],
    center_feature_count: u64,
    halo_feature_count: u64,
    max_features: u64,
    max_work_cells: u64,
}

#[derive(Serialize)]
struct GeoTileRelationshipIdProjection<'a> {
    decision_id: &'a str,
    relation: GeoControlRelation,
    from: &'a GeoTileRelationshipEndpointRef,
    to: &'a GeoTileRelationshipEndpointRef,
    payload_blake3: &'a str,
}

#[derive(Serialize)]
struct GeoTileRelationshipGroupIdProjection<'a> {
    relation: GeoControlRelation,
    from_entity_level: GeoControlEntityLevel,
    to_entity_level: GeoControlEntityLevel,
    anchor_side: GeoTileRelationshipAnchorSide,
    anchor: &'a GeoTileRelationshipEndpointRef,
    related: &'a [GeoTileRelationshipEndpointRef],
    relationship_ids: &'a [String],
}

/// Derive deterministic H3 home cells from release-bound representative points.
///
/// The primary cell is h3o's cell for the admitted fixed-decimal coordinate.
/// Nine points probe the declared coordinate envelope and their cells are
/// retained explicitly. A mismatch against a claimed warehouse cell is reported rather
/// than refused because parity is an empirical finding; malformed or
/// wrong-resolution claims still refuse.
pub fn materialize_home_cells(
    request: &GeoHomeCellRowsRequest,
) -> Result<GeoHomeCellAssignmentArtifact, GeoTileError> {
    if request.version != CANON_GEO_HOME_CELL_ROWS_VERSION {
        return Err(GeoTileError::new(
            GeoTileErrorCode::UnsupportedVersion,
            "Unsupported Geo home-cell rows version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_HOME_CELL_ROWS_VERSION),
            ],
        ));
    }
    if request.coordinate_crs != "EPSG:4326" {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidInput,
            "H3 representative points must use explicit EPSG:4326 longitude/latitude",
            [("coordinate_crs", request.coordinate_crs.as_str())],
        ));
    }
    if request.coordinate_decimal_places > MAX_COORDINATE_DECIMAL_PLACES {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidInput,
            "Geo home-cell coordinate precision exceeds the deterministic admission limit",
            [
                (
                    "coordinate_decimal_places",
                    request.coordinate_decimal_places.to_string(),
                ),
                ("hard_max", MAX_COORDINATE_DECIMAL_PLACES.to_string()),
            ],
        ));
    }
    if request.stability_radius_fixed > MAX_STABILITY_RADIUS_FIXED {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidInput,
            "Geo home-cell stability radius exceeds the kernel ceiling",
            [
                (
                    "stability_radius_fixed",
                    request.stability_radius_fixed.to_string(),
                ),
                ("hard_max", MAX_STABILITY_RADIUS_FIXED.to_string()),
            ],
        ));
    }
    validate_budget("max_rows", request.max_rows, MAX_HOME_CELL_ROWS)?;
    let row_count = usize_to_u64(request.rows.len(), "home_cell_rows.len")?;
    if row_count == 0 || row_count > request.max_rows {
        return Err(GeoTileError::new(
            GeoTileErrorCode::FeatureBudgetExceeded,
            "Geo home-cell row count is empty or exceeds the declared budget",
            [
                ("observed", row_count.to_string()),
                ("configured", request.max_rows.to_string()),
            ],
        ));
    }
    let resolution = Resolution::try_from(request.h3_resolution).map_err(|error| {
        GeoTileError::new(
            GeoTileErrorCode::ResolutionMismatch,
            "Geo home-cell request uses an unsupported H3 resolution",
            [
                ("h3_resolution", request.h3_resolution.to_string()),
                ("error", error.to_string()),
            ],
        )
    })?;
    let scale = fixed_decimal_scale(request.coordinate_decimal_places)?;
    let stability_radius = i64::try_from(request.stability_radius_fixed)
        .map_err(|_| GeoTileError::overflow("stability_radius_fixed"))?;
    validate_stability_radius(stability_radius, scale)?;
    let mut seen_features = BTreeSet::new();
    let mut seen_records = BTreeSet::new();
    let mut source_signatures = BTreeMap::new();
    let mut features = Vec::with_capacity(request.rows.len());

    for row in &request.rows {
        validate_source_binding("rows[].source", &row.source)?;
        for (field, value) in [
            ("rows[].feature_id", row.feature_id.as_str()),
            ("rows[].source_record_id", row.source_record_id.as_str()),
            (
                "rows[].representative_point_method",
                row.representative_point_method.as_str(),
            ),
        ] {
            validate_identifier(field, value)?;
        }
        validate_sha256("rows[].geometry_sha256", &row.geometry_sha256)?;
        validate_transform_pair(row)?;
        validate_source_signature(&mut source_signatures, row)?;
        let feature_key = source_feature_key(&row.source, &row.feature_id);
        if !seen_features.insert(feature_key) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::DuplicateFeature,
                "Geo home-cell rows repeat a source feature",
                [
                    ("source_instance_id", row.source.source_instance_id.as_str()),
                    ("release_id", row.source.release.release_id.as_str()),
                    ("feature_id", row.feature_id.as_str()),
                ],
            ));
        }
        let record_key = source_feature_key(&row.source, &row.source_record_id);
        if !seen_records.insert(record_key) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::DuplicateFeature,
                "Geo home-cell rows repeat a source record",
                [
                    ("source_instance_id", row.source.source_instance_id.as_str()),
                    ("release_id", row.source.release.release_id.as_str()),
                    ("source_record_id", row.source_record_id.as_str()),
                ],
            ));
        }
        let longitude = parse_home_coordinate(
            "longitude",
            &row.longitude,
            request.coordinate_decimal_places,
        )?;
        let latitude =
            parse_home_coordinate("latitude", &row.latitude, request.coordinate_decimal_places)?;
        validate_longitude_latitude(longitude, latitude, scale)?;
        let home = cell_for_fixed(longitude, latitude, scale, resolution)?;
        let (stability_cells, minimum_stability_halo_k) =
            stability_cells(longitude, latitude, scale, stability_radius, resolution)?;
        let claimed = row
            .claimed_home_cell
            .as_deref()
            .map(|value| parse_claimed_cell(value, resolution))
            .transpose()?;
        let parity = match claimed {
            None => GeoHomeCellParity::NotClaimed,
            Some(cell) if cell == home => GeoHomeCellParity::Match,
            Some(_) => GeoHomeCellParity::Mismatch,
        };
        features.push(GeoHomeCellFeatureAssignment {
            source: row.source.clone(),
            feature_id: row.feature_id.clone(),
            source_record_id: row.source_record_id.clone(),
            geometry_sha256: row.geometry_sha256.clone(),
            representative_point_method: row.representative_point_method.clone(),
            representative_point_fixed: GeoRepresentativePointFixed {
                longitude,
                latitude,
            },
            transform_execution_id: row.transform_execution_id.clone(),
            transform_definition_id: row.transform_definition_id.clone(),
            home_cell: home.to_string(),
            stability_cells,
            minimum_stability_halo_k,
            claimed_home_cell: claimed.map(|cell| cell.to_string()),
            parity,
        });
    }
    features.sort();
    let tile_work_features = features
        .iter()
        .map(|feature| GeoTileFeatureRef {
            source: feature.source.clone(),
            feature_id: feature.feature_id.clone(),
            home_cell: feature.home_cell.clone(),
        })
        .collect::<Vec<_>>();
    let mut summary = GeoHomeCellAssignmentSummary {
        total: row_count,
        boundary_sensitive: 0,
        max_minimum_stability_halo_k: 0,
        claimed: 0,
        matches: 0,
        mismatches: 0,
        unclaimed: 0,
    };
    for feature in &features {
        if feature.stability_cells.len() > 1 {
            summary.boundary_sensitive = checked_add(
                summary.boundary_sensitive,
                1,
                "home_cell_summary.boundary_sensitive",
            )?;
        }
        summary.max_minimum_stability_halo_k = summary
            .max_minimum_stability_halo_k
            .max(feature.minimum_stability_halo_k);
        match feature.parity {
            GeoHomeCellParity::NotClaimed => {
                summary.unclaimed =
                    checked_add(summary.unclaimed, 1, "home_cell_summary.unclaimed")?;
            }
            GeoHomeCellParity::Match => {
                summary.claimed = checked_add(summary.claimed, 1, "home_cell_summary.claimed")?;
                summary.matches = checked_add(summary.matches, 1, "home_cell_summary.matches")?;
            }
            GeoHomeCellParity::Mismatch => {
                summary.claimed = checked_add(summary.claimed, 1, "home_cell_summary.claimed")?;
                summary.mismatches =
                    checked_add(summary.mismatches, 1, "home_cell_summary.mismatches")?;
            }
        }
    }
    Ok(GeoHomeCellAssignmentArtifact {
        version: CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION.to_string(),
        request_version: request.version.clone(),
        coordinate_crs: request.coordinate_crs.clone(),
        coordinate_decimal_places: request.coordinate_decimal_places,
        h3_resolution: request.h3_resolution,
        stability_radius_fixed: request.stability_radius_fixed,
        features,
        tile_work_features,
        summary,
    })
}

/// Build one exact center-plus-halo feature work unit.
///
/// Supplied features outside the declared disk refuse instead of being silently
/// dropped. This makes request-local reach defects visible, but cannot prove
/// that the upstream candidate generator supplied every relevant feature.
pub fn materialize_tile_work_unit(
    request: &GeoTileWorkRequest,
) -> Result<GeoTileWorkUnitArtifact, GeoTileError> {
    if request.version != CANON_GEO_TILE_WORK_REQUEST_VERSION {
        return Err(GeoTileError::new(
            GeoTileErrorCode::UnsupportedVersion,
            "Unsupported Geo tile-work request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_TILE_WORK_REQUEST_VERSION),
            ],
        ));
    }
    validate_budget(
        "max_features",
        request.max_features,
        MAX_FEATURES_PER_WORK_UNIT,
    )?;
    validate_budget("max_work_cells", request.max_work_cells, MAX_WORK_CELLS)?;
    let feature_count = usize_to_u64(request.features.len(), "features.len")?;
    if feature_count > request.max_features {
        return Err(GeoTileError::new(
            GeoTileErrorCode::FeatureBudgetExceeded,
            "Geo tile work unit exceeds the declared feature budget",
            [
                ("observed", feature_count.to_string()),
                ("configured", request.max_features.to_string()),
            ],
        ));
    }

    let center = parse_cell(&request.center_cell, "center_cell")?;
    let disk = bounded_grid_disk(center, request.halo_k, request.max_work_cells)?;
    let center_cell = center.to_string();
    let resolution = center.resolution();
    let mut seen = BTreeSet::new();
    let mut features = Vec::with_capacity(request.features.len());
    let mut center_feature_count = 0_u64;
    let mut halo_feature_count = 0_u64;
    let mut source_bindings = BTreeMap::new();

    for feature in &request.features {
        validate_source_binding("features[].source", &feature.source)?;
        validate_source_binding_consistency(&mut source_bindings, &feature.source)?;
        validate_identifier("feature_id", &feature.feature_id)?;
        let key = source_feature_key(&feature.source, &feature.feature_id);
        if !seen.insert(key) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::DuplicateFeature,
                "Geo tile work unit contains a duplicate source feature",
                [
                    (
                        "source_instance_id",
                        feature.source.source_instance_id.as_str(),
                    ),
                    ("release_id", feature.source.release.release_id.as_str()),
                    ("feature_id", feature.feature_id.as_str()),
                ],
            ));
        }
        let home = parse_cell(&feature.home_cell, "features.home_cell")?;
        require_resolution(center, home, &feature.home_cell)?;
        if !disk.contains(&home) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::FeatureOutsideHalo,
                "Geo feature lies outside the declared center-plus-halo work unit",
                [
                    (
                        "source_instance_id",
                        feature.source.source_instance_id.clone(),
                    ),
                    ("release_id", feature.source.release.release_id.clone()),
                    ("feature_id", feature.feature_id.clone()),
                    ("home_cell", home.to_string()),
                    ("center_cell", center_cell.clone()),
                    ("halo_k", request.halo_k.to_string()),
                ],
            ));
        }
        let placement = if home == center {
            center_feature_count = checked_add(center_feature_count, 1, "center_feature_count")?;
            GeoTilePlacement::Center
        } else {
            halo_feature_count = checked_add(halo_feature_count, 1, "halo_feature_count")?;
            GeoTilePlacement::Halo
        };
        features.push(GeoTileFeatureMembership {
            source: feature.source.clone(),
            feature_id: feature.feature_id.clone(),
            home_cell: home.to_string(),
            placement,
        });
    }
    features.sort();

    let mut artifact = GeoTileWorkUnitArtifact {
        version: CANON_GEO_TILE_WORK_UNIT_VERSION.to_string(),
        request_version: request.version.clone(),
        center_cell,
        h3_resolution: u8::from(resolution),
        halo_k: request.halo_k,
        work_cells: disk.into_iter().map(|cell| cell.to_string()).collect(),
        features,
        center_feature_count,
        halo_feature_count,
        max_features: request.max_features,
        max_work_cells: request.max_work_cells,
        work_unit_blake3: String::new(),
    };
    artifact.work_unit_blake3 = canonical_work_unit_digest(&artifact)?;
    Ok(artifact)
}

/// Reconcile caller-associated tile decisions into one owned result per
/// declared decision semantics plus canonical member set.
///
/// The owner is the numerically smallest H3 home cell among the decision's
/// members. That rule is independent of source name, iteration order, and which
/// neighboring work unit happened to finish first.
pub fn reconcile_tile_decisions(
    request: &GeoTileReconciliationRequest,
) -> Result<GeoTileReconciliationArtifact, GeoTileError> {
    if request.version != CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION {
        return Err(GeoTileError::new(
            GeoTileErrorCode::UnsupportedVersion,
            "Unsupported Geo tile-reconciliation request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION),
            ],
        ));
    }
    validate_budget(
        "max_batches",
        request.max_batches,
        MAX_RECONCILIATION_BATCHES,
    )?;
    validate_budget(
        "max_proposals",
        request.max_proposals,
        MAX_RECONCILIATION_PROPOSALS,
    )?;
    validate_budget(
        "max_members_per_decision",
        request.max_members_per_decision,
        MAX_MEMBERS_PER_DECISION,
    )?;
    validate_budget(
        "max_features_per_batch",
        request.max_features_per_batch,
        MAX_FEATURES_PER_WORK_UNIT,
    )?;
    validate_budget(
        "max_work_cells_per_batch",
        request.max_work_cells_per_batch,
        MAX_WORK_CELLS,
    )?;
    let batch_count = usize_to_u64(request.batches.len(), "batches.len")?;
    if batch_count == 0 || batch_count > request.max_batches {
        return Err(GeoTileError::new(
            GeoTileErrorCode::ReconciliationBudgetExceeded,
            "Geo tile reconciliation batch count is empty or over budget",
            [
                ("observed", batch_count.to_string()),
                ("configured", request.max_batches.to_string()),
            ],
        ));
    }

    let mut batch_centers = BTreeSet::new();
    let mut expected_resolution = None;
    let mut input_proposals = 0_u64;
    let mut decisions: BTreeMap<String, DecisionAccumulator> = BTreeMap::new();
    let mut batch_receipts = Vec::with_capacity(request.batches.len());
    let mut source_bindings = BTreeMap::new();
    let inventory_sources = request
        .inventory_lineage
        .as_ref()
        .map(validate_inventory_lineage)
        .transpose()?;

    for batch in &request.batches {
        let feature_count = usize_to_u64(batch.work_unit.features.len(), "work_unit.features.len")?;
        if feature_count > request.max_features_per_batch {
            return Err(GeoTileError::new(
                GeoTileErrorCode::ReconciliationBudgetExceeded,
                "Geo tile reconciliation work unit exceeds the per-batch feature budget",
                [
                    ("observed", feature_count.to_string()),
                    ("configured", request.max_features_per_batch.to_string()),
                ],
            ));
        }
        let work_cell_count =
            usize_to_u64(batch.work_unit.work_cells.len(), "work_unit.work_cells.len")?;
        if work_cell_count > request.max_work_cells_per_batch {
            return Err(GeoTileError::new(
                GeoTileErrorCode::ReconciliationBudgetExceeded,
                "Geo tile reconciliation work unit exceeds the per-batch cell budget",
                [
                    ("observed", work_cell_count.to_string()),
                    ("configured", request.max_work_cells_per_batch.to_string()),
                ],
            ));
        }
        if batch.work_unit.halo_k != request.halo_k {
            return Err(GeoTileError::new(
                GeoTileErrorCode::InvalidWorkUnit,
                "Geo tile reconciliation work unit uses a different halo radius",
                [
                    ("expected_halo_k", request.halo_k.to_string()),
                    ("actual_halo_k", batch.work_unit.halo_k.to_string()),
                    ("center_cell", batch.work_unit.center_cell.clone()),
                ],
            ));
        }
        let validated_work_unit = validate_work_unit_artifact(&batch.work_unit)?;
        for binding in validated_work_unit.source_bindings.values() {
            validate_source_binding_consistency(&mut source_bindings, binding)?;
        }
        let center = validated_work_unit.center;
        if let Some(resolution) = expected_resolution {
            if center.resolution() != resolution {
                return Err(resolution_error(
                    resolution,
                    center,
                    &batch.work_unit.center_cell,
                ));
            }
        } else {
            expected_resolution = Some(center.resolution());
        }
        if !batch_centers.insert(center) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::DuplicateCenter,
                "Geo tile reconciliation contains duplicate center batches",
                [("center_cell", center.to_string())],
            ));
        }
        let mut batch_decision_scopes = BTreeSet::new();
        batch_receipts.push(GeoTileBatchReceipt {
            center_cell: center.to_string(),
            work_unit_blake3: validated_work_unit.blake3.clone(),
            proposal_count: usize_to_u64(batch.proposals.len(), "batch.proposals.len")?,
        });

        for proposal in &batch.proposals {
            input_proposals = checked_add(input_proposals, 1, "input_proposals")?;
            if input_proposals > request.max_proposals {
                return Err(GeoTileError::new(
                    GeoTileErrorCode::ReconciliationBudgetExceeded,
                    "Geo tile reconciliation exceeds the proposal budget",
                    [
                        ("observed", input_proposals.to_string()),
                        ("configured", request.max_proposals.to_string()),
                    ],
                ));
            }
            validate_decision_blake3("payload_blake3", &proposal.payload_blake3)?;
            validate_decision_blake3("work_unit_blake3", &proposal.work_unit_blake3)?;
            if proposal.work_unit_blake3 != validated_work_unit.blake3 {
                return Err(GeoTileError::new(
                    GeoTileErrorCode::InvalidDecision,
                    "Geo tile proposal is not bound to its embedded canonical work unit",
                    [
                        ("center_cell", center.to_string()),
                        (
                            "expected_work_unit_blake3",
                            validated_work_unit.blake3.clone(),
                        ),
                        ("actual_work_unit_blake3", proposal.work_unit_blake3.clone()),
                    ],
                ));
            }
            let members = normalize_members(
                &proposal.members,
                proposal.semantics,
                center,
                &validated_work_unit.feature_home_cells,
                inventory_sources.as_ref(),
                request.max_members_per_decision,
            )?;
            let decision_scope_bytes = serde_json::to_vec(&(proposal.semantics, &members))
                .map_err(|error| {
                    GeoTileError::new(
                        GeoTileErrorCode::Serialization,
                        "Geo tile decision semantics and members could not be serialized",
                        [("error", error.to_string())],
                    )
                })?;
            let decision_scope_blake3 = blake3::hash(&decision_scope_bytes).to_hex().to_string();
            if !batch_decision_scopes.insert(decision_scope_blake3.clone()) {
                return Err(GeoTileError::new(
                    GeoTileErrorCode::InvalidDecision,
                    "A center batch proposed the same decision semantics and membership more than once",
                    [
                        ("center_cell", center.to_string()),
                        ("decision_scope_blake3", decision_scope_blake3.clone()),
                    ],
                ));
            }
            let owner_cell = members
                .iter()
                .map(|member| {
                    parse_cell(&member.home_cell, "proposals.members.home_cell")
                        .expect("normalized members contain validated cells")
                })
                .min()
                .expect("normalized members are non-empty");

            match decisions.get_mut(&decision_scope_blake3) {
                Some(existing) => {
                    if existing.payload_blake3 != proposal.payload_blake3 {
                        return Err(GeoTileError::new(
                            GeoTileErrorCode::NonConfluentDecision,
                            "Adjacent tile work units produced different payloads for the same decision semantics and members",
                            [
                                ("decision_scope_blake3", decision_scope_blake3.clone()),
                                ("first_payload", existing.payload_blake3.clone()),
                                ("second_payload", proposal.payload_blake3.clone()),
                                ("second_center", center.to_string()),
                            ],
                        ));
                    }
                    existing.proposal_centers.insert(center);
                }
                None => {
                    decisions.insert(
                        decision_scope_blake3.clone(),
                        DecisionAccumulator {
                            decision_scope_blake3,
                            semantics: proposal.semantics,
                            payload_blake3: proposal.payload_blake3.clone(),
                            owner_cell,
                            members,
                            proposal_centers: BTreeSet::from([center]),
                        },
                    );
                }
            }
        }
    }

    let resolution = expected_resolution.expect("non-empty batches establish resolution");
    let mut discarded_halo_proposals = 0_u64;
    let mut reconciled = Vec::with_capacity(decisions.len());
    for (_, decision) in decisions {
        if !batch_centers.contains(&decision.owner_cell) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::MissingOwnerWorkUnit,
                "Geo tile reconciliation is missing the decision owner work unit",
                [
                    ("owner_cell", decision.owner_cell.to_string()),
                    (
                        "decision_scope_blake3",
                        decision.decision_scope_blake3.clone(),
                    ),
                ],
            ));
        }
        if !decision.proposal_centers.contains(&decision.owner_cell) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::OrphanedDecision,
                "A boundary decision was observed only in halo work units",
                [
                    ("owner_cell", decision.owner_cell.to_string()),
                    (
                        "decision_scope_blake3",
                        decision.decision_scope_blake3.clone(),
                    ),
                ],
            ));
        }
        let proposal_copies = usize_to_u64(
            decision.proposal_centers.len(),
            "decision.proposal_centers.len",
        )?;
        discarded_halo_proposals = checked_add(
            discarded_halo_proposals,
            proposal_copies.saturating_sub(1),
            "discarded_halo_proposals",
        )?;
        reconciled.push(GeoReconciledTileDecision {
            decision_id: decision_id(&decision.decision_scope_blake3, &decision.payload_blake3),
            owner_cell: decision.owner_cell.to_string(),
            semantics: decision.semantics,
            inventory_ref: match decision.semantics {
                GeoTileDecisionSemantics::Composition => None,
                GeoTileDecisionSemantics::Relation { .. } => None,
                GeoTileDecisionSemantics::StableIdentity { .. } => inventory_sources
                    .as_ref()
                    .map(|lineage| lineage.inventory_ref.clone()),
            },
            payload_blake3: decision.payload_blake3,
            members: decision.members,
            proposal_copies,
        });
    }
    reconciled.sort_by(|left, right| left.decision_id.cmp(&right.decision_id));
    let relationships = relationship_edges_for_decisions(&reconciled)?;
    let relationship_groups = relationship_groups_for_edges(&relationships)?;
    batch_receipts.sort();
    let owned_decisions = usize_to_u64(reconciled.len(), "decisions.len")?;

    Ok(GeoTileReconciliationArtifact {
        version: CANON_GEO_TILE_RECONCILIATION_VERSION.to_string(),
        request_version: request.version.clone(),
        h3_resolution: u8::from(resolution),
        halo_k: request.halo_k,
        inventory_ref: inventory_sources
            .as_ref()
            .map(|lineage| lineage.inventory_ref.clone()),
        batches: batch_count,
        input_proposals,
        owned_decisions,
        discarded_halo_proposals,
        batch_receipts,
        decisions: reconciled,
        relationships,
        relationship_groups,
    })
}

pub fn canonical_tile_work_unit_bytes(
    artifact: &GeoTileWorkUnitArtifact,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(artifact)
}

fn canonical_work_unit_digest(artifact: &GeoTileWorkUnitArtifact) -> Result<String, GeoTileError> {
    let projection = GeoTileWorkUnitDigestProjection {
        version: &artifact.version,
        request_version: &artifact.request_version,
        center_cell: &artifact.center_cell,
        h3_resolution: artifact.h3_resolution,
        halo_k: artifact.halo_k,
        work_cells: &artifact.work_cells,
        features: &artifact.features,
        center_feature_count: artifact.center_feature_count,
        halo_feature_count: artifact.halo_feature_count,
        max_features: artifact.max_features,
        max_work_cells: artifact.max_work_cells,
    };
    let bytes = serde_json::to_vec(&projection).map_err(|error| {
        GeoTileError::new(
            GeoTileErrorCode::Serialization,
            "Geo tile work-unit digest projection could not be serialized",
            [("error", error.to_string())],
        )
    })?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"canon_geo_tile_work_unit.v1\0");
    hasher.update(&bytes);
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

pub fn canonical_home_cell_assignment_bytes(
    artifact: &GeoHomeCellAssignmentArtifact,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(artifact)
}

pub fn canonical_tile_reconciliation_bytes(
    artifact: &GeoTileReconciliationArtifact,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(artifact)
}

fn relationship_edges_for_decisions(
    decisions: &[GeoReconciledTileDecision],
) -> Result<Vec<GeoTileRelationshipEdge>, GeoTileError> {
    let mut relationships = Vec::new();
    for decision in decisions {
        let GeoTileDecisionSemantics::Relation {
            relation,
            from_entity_level,
            to_entity_level,
        } = decision.semantics
        else {
            continue;
        };
        if relation == GeoControlRelation::SameAs {
            return Err(GeoTileError::new(
                GeoTileErrorCode::InvalidDecision,
                "Geo tile relationship output cannot express same_as; use stable_identity",
                [
                    ("decision_id", decision.decision_id.clone()),
                    ("relation", relation_name(relation)),
                ],
            ));
        }
        let from_member = decision
            .members
            .iter()
            .find(|member| member.candidate_entity_level == from_entity_level)
            .ok_or_else(|| {
                GeoTileError::new(
                    GeoTileErrorCode::InvalidDecision,
                    "Geo tile relation decision is missing its declared from endpoint",
                    [
                        ("decision_id", decision.decision_id.clone()),
                        ("from_entity_level", entity_level_name(from_entity_level)),
                    ],
                )
            })?;
        let to_member = decision
            .members
            .iter()
            .find(|member| member.candidate_entity_level == to_entity_level)
            .ok_or_else(|| {
                GeoTileError::new(
                    GeoTileErrorCode::InvalidDecision,
                    "Geo tile relation decision is missing its declared to endpoint",
                    [
                        ("decision_id", decision.decision_id.clone()),
                        ("to_entity_level", entity_level_name(to_entity_level)),
                    ],
                )
            })?;
        let from = relationship_endpoint(from_entity_level, from_member);
        let to = relationship_endpoint(to_entity_level, to_member);
        relationships.push(GeoTileRelationshipEdge {
            relationship_id: relationship_id(
                &decision.decision_id,
                relation,
                &from,
                &to,
                &decision.payload_blake3,
            )?,
            decision_id: decision.decision_id.clone(),
            owner_cell: decision.owner_cell.clone(),
            relation,
            from,
            to,
            payload_blake3: decision.payload_blake3.clone(),
            proposal_copies: decision.proposal_copies,
        });
    }
    relationships.sort_by(|left, right| left.relationship_id.cmp(&right.relationship_id));
    Ok(relationships)
}

fn relationship_groups_for_edges(
    relationships: &[GeoTileRelationshipEdge],
) -> Result<Vec<GeoTileRelationshipGroup>, GeoTileError> {
    let mut groups = BTreeMap::<RelationshipGroupKey, RelationshipGroupAccumulator>::new();
    for relationship in relationships {
        accumulate_relationship_group(
            &mut groups,
            relationship,
            GeoTileRelationshipAnchorSide::From,
        );
        accumulate_relationship_group(&mut groups, relationship, GeoTileRelationshipAnchorSide::To);
    }

    let mut output = Vec::with_capacity(groups.len());
    for (_, group) in groups {
        let related = group.related.into_iter().collect::<Vec<_>>();
        let relationship_ids = group.relationship_ids.into_iter().collect::<Vec<_>>();
        let related_count = usize_to_u64(related.len(), "relationship_group.related.len")?;
        output.push(GeoTileRelationshipGroup {
            group_id: relationship_group_id(
                group.relation,
                group.from_entity_level,
                group.to_entity_level,
                group.anchor_side,
                &group.anchor,
                &related,
                &relationship_ids,
            )?,
            relation: group.relation,
            from_entity_level: group.from_entity_level,
            to_entity_level: group.to_entity_level,
            anchor_side: group.anchor_side,
            anchor: group.anchor,
            related,
            relationship_ids,
            related_count,
        });
    }
    output.sort_by(|left, right| left.group_id.cmp(&right.group_id));
    Ok(output)
}

fn accumulate_relationship_group(
    groups: &mut BTreeMap<RelationshipGroupKey, RelationshipGroupAccumulator>,
    relationship: &GeoTileRelationshipEdge,
    anchor_side: GeoTileRelationshipAnchorSide,
) {
    let (anchor, related) = match anchor_side {
        GeoTileRelationshipAnchorSide::From => (&relationship.from, &relationship.to),
        GeoTileRelationshipAnchorSide::To => (&relationship.to, &relationship.from),
    };
    let key = (
        relationship.relation,
        relationship.from.entity_level,
        relationship.to.entity_level,
        anchor_side,
        anchor.clone(),
    );
    let entry = groups
        .entry(key)
        .or_insert_with(|| RelationshipGroupAccumulator {
            relation: relationship.relation,
            from_entity_level: relationship.from.entity_level,
            to_entity_level: relationship.to.entity_level,
            anchor_side,
            anchor: anchor.clone(),
            related: BTreeSet::new(),
            relationship_ids: BTreeSet::new(),
        });
    entry.related.insert(related.clone());
    entry
        .relationship_ids
        .insert(relationship.relationship_id.clone());
}

fn relationship_endpoint(
    entity_level: GeoControlEntityLevel,
    member: &GeoTileDecisionMember,
) -> GeoTileRelationshipEndpointRef {
    GeoTileRelationshipEndpointRef {
        entity_level,
        source: member.source.clone(),
        feature_id: member.feature_id.clone(),
        home_cell: member.home_cell.clone(),
    }
}

fn relationship_id(
    decision_id: &str,
    relation: GeoControlRelation,
    from: &GeoTileRelationshipEndpointRef,
    to: &GeoTileRelationshipEndpointRef,
    payload_blake3: &str,
) -> Result<String, GeoTileError> {
    let projection = GeoTileRelationshipIdProjection {
        decision_id,
        relation,
        from,
        to,
        payload_blake3,
    };
    let bytes = serde_json::to_vec(&projection).map_err(|error| {
        GeoTileError::new(
            GeoTileErrorCode::Serialization,
            "Geo tile relationship id projection could not be serialized",
            [("error", error.to_string())],
        )
    })?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"canon_geo_tile_relationship.v1\0");
    hasher.update(&bytes);
    Ok(format!("geo-relationship:{}", hasher.finalize().to_hex()))
}

fn relationship_group_id(
    relation: GeoControlRelation,
    from_entity_level: GeoControlEntityLevel,
    to_entity_level: GeoControlEntityLevel,
    anchor_side: GeoTileRelationshipAnchorSide,
    anchor: &GeoTileRelationshipEndpointRef,
    related: &[GeoTileRelationshipEndpointRef],
    relationship_ids: &[String],
) -> Result<String, GeoTileError> {
    let projection = GeoTileRelationshipGroupIdProjection {
        relation,
        from_entity_level,
        to_entity_level,
        anchor_side,
        anchor,
        related,
        relationship_ids,
    };
    let bytes = serde_json::to_vec(&projection).map_err(|error| {
        GeoTileError::new(
            GeoTileErrorCode::Serialization,
            "Geo tile relationship-group id projection could not be serialized",
            [("error", error.to_string())],
        )
    })?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"canon_geo_tile_relationship_group.v1\0");
    hasher.update(&bytes);
    Ok(format!(
        "geo-relationship-group:{}",
        hasher.finalize().to_hex()
    ))
}

fn normalize_members(
    members: &[GeoTileDecisionMember],
    semantics: GeoTileDecisionSemantics,
    center: CellIndex,
    available_features: &FeatureHomeCells,
    inventory_sources: Option<&ValidatedInventoryLineage>,
    max_members: u64,
) -> Result<Vec<GeoTileDecisionMember>, GeoTileError> {
    let count = usize_to_u64(members.len(), "proposal.members.len")?;
    if count == 0 || count > max_members {
        return Err(GeoTileError::new(
            GeoTileErrorCode::ReconciliationBudgetExceeded,
            "Geo tile decision member count is empty or over budget",
            [
                ("observed", count.to_string()),
                ("configured", max_members.to_string()),
            ],
        ));
    }
    let mut normalized = Vec::with_capacity(members.len());
    let mut seen = BTreeSet::new();
    for member in members {
        validate_source_binding("proposals.members[].source", &member.source)?;
        validate_identifier("feature_id", &member.feature_id)?;
        let key = source_feature_key(&member.source, &member.feature_id);
        if !seen.insert(key) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::InvalidDecision,
                "Geo tile decision contains a duplicate source feature",
                [
                    (
                        "source_instance_id",
                        member.source.source_instance_id.as_str(),
                    ),
                    ("release_id", member.source.release.release_id.as_str()),
                    ("feature_id", member.feature_id.as_str()),
                ],
            ));
        }
        match member.source.native_entity_level() {
            Some(actual) if actual == member.candidate_entity_level => {}
            Some(actual) => {
                return Err(GeoTileError::new(
                    GeoTileErrorCode::InvalidCandidateMember,
                    "Geo tile decision cannot promote a cross-level feature into the selected entity level",
                    [
                        (
                            "source_instance_id",
                            member.source.source_instance_id.clone(),
                        ),
                        ("feature_id", member.feature_id.clone()),
                        (
                            "candidate_entity_level",
                            entity_level_name(member.candidate_entity_level),
                        ),
                        ("native_entity_level", entity_level_name(actual)),
                    ],
                ));
            }
            None => {
                return Err(GeoTileError::new(
                    GeoTileErrorCode::InvalidCandidateMember,
                    "Geo tile decision cannot promote an observation-only feature into an entity candidate",
                    [
                        (
                            "source_instance_id",
                            member.source.source_instance_id.clone(),
                        ),
                        ("feature_id", member.feature_id.clone()),
                        (
                            "candidate_entity_level",
                            entity_level_name(member.candidate_entity_level),
                        ),
                    ],
                ));
            }
        }
        let home = parse_cell(&member.home_cell, "proposals.members.home_cell")?;
        require_resolution(center, home, &member.home_cell)?;
        let expected_home =
            available_features.get(&source_feature_key(&member.source, &member.feature_id));
        if expected_home != Some(&home) {
            return Err(GeoTileError::new(
                GeoTileErrorCode::InvalidDecision,
                "Geo tile decision references a member absent from its producing work unit",
                [
                    ("center_cell", center.to_string()),
                    (
                        "source_instance_id",
                        member.source.source_instance_id.clone(),
                    ),
                    ("release_id", member.source.release.release_id.clone()),
                    ("feature_id", member.feature_id.clone()),
                    ("home_cell", home.to_string()),
                ],
            ));
        }
        normalized.push(GeoTileDecisionMember {
            source: member.source.clone(),
            feature_id: member.feature_id.clone(),
            candidate_entity_level: member.candidate_entity_level,
            home_cell: home.to_string(),
        });
    }
    if let GeoTileDecisionSemantics::StableIdentity { entity_level } = semantics {
        let inventory_sources = inventory_sources.ok_or_else(|| {
            GeoTileError::new(
                GeoTileErrorCode::InvalidInventoryLineage,
                "Geo stable-identity decision requires validated regional-inventory lineage",
                [("stable_identity_level", entity_level_name(entity_level))],
            )
        })?;
        if let Some(member) = normalized
            .iter()
            .find(|member| member.candidate_entity_level != entity_level)
        {
            return Err(GeoTileError::new(
                GeoTileErrorCode::InvalidCandidateMember,
                "Geo stable-identity decision cannot contain a member from another entity level",
                [
                    ("stable_identity_level", entity_level_name(entity_level)),
                    (
                        "member_entity_level",
                        entity_level_name(member.candidate_entity_level),
                    ),
                    (
                        "source_instance_id",
                        member.source.source_instance_id.clone(),
                    ),
                    ("feature_id", member.feature_id.clone()),
                ],
            ));
        }
        for member in &normalized {
            validate_member_inventory_authority(member, inventory_sources)?;
        }
        if !normalized
            .iter()
            .any(GeoTileDecisionMember::may_contribute_stable_alias)
        {
            return Err(GeoTileError::new(
                GeoTileErrorCode::InvalidCandidateMember,
                "Geo stable-identity decision requires a same-level stable-alias participant",
                [
                    ("stable_identity_level", entity_level_name(entity_level)),
                    ("member_count", normalized.len().to_string()),
                ],
            ));
        }
    }
    if let GeoTileDecisionSemantics::Relation {
        relation,
        from_entity_level,
        to_entity_level,
    } = semantics
    {
        validate_relation_decision(relation, from_entity_level, to_entity_level, &normalized)?;
    }
    normalized.sort();
    Ok(normalized)
}

fn validate_relation_decision(
    relation: GeoControlRelation,
    from_entity_level: GeoControlEntityLevel,
    to_entity_level: GeoControlEntityLevel,
    members: &[GeoTileDecisionMember],
) -> Result<(), GeoTileError> {
    if relation == GeoControlRelation::SameAs {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidDecision,
            "Geo tile relation decisions cannot express same_as; use stable_identity",
            [("relation", relation_name(relation))],
        ));
    }
    if from_entity_level == to_entity_level {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidDecision,
            "Geo tile relation decisions must connect distinct entity levels",
            [
                ("from_entity_level", entity_level_name(from_entity_level)),
                ("to_entity_level", entity_level_name(to_entity_level)),
            ],
        ));
    }
    if members.len() != 2 {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidDecision,
            "Geo tile relation decisions require exactly one from member and one to member",
            [
                ("relation", relation_name(relation)),
                ("member_count", members.len().to_string()),
            ],
        ));
    }

    let mut from_count = 0_u8;
    let mut to_count = 0_u8;
    for member in members {
        if member.candidate_entity_level == from_entity_level {
            from_count += 1;
        } else if member.candidate_entity_level == to_entity_level {
            to_count += 1;
        } else {
            return Err(GeoTileError::new(
                GeoTileErrorCode::InvalidCandidateMember,
                "Geo tile relation member is outside the declared cross-level relation",
                [
                    ("relation", relation_name(relation)),
                    (
                        "member_entity_level",
                        entity_level_name(member.candidate_entity_level),
                    ),
                    ("from_entity_level", entity_level_name(from_entity_level)),
                    ("to_entity_level", entity_level_name(to_entity_level)),
                    (
                        "source_instance_id",
                        member.source.source_instance_id.clone(),
                    ),
                    ("feature_id", member.feature_id.clone()),
                ],
            ));
        }
    }
    if from_count != 1 || to_count != 1 {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidDecision,
            "Geo tile relation decisions require one member at each declared entity level",
            [
                ("relation", relation_name(relation)),
                ("from_entity_level", entity_level_name(from_entity_level)),
                ("to_entity_level", entity_level_name(to_entity_level)),
                ("from_member_count", from_count.to_string()),
                ("to_member_count", to_count.to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_work_unit_artifact(
    artifact: &GeoTileWorkUnitArtifact,
) -> Result<ValidatedWorkUnit, GeoTileError> {
    let request = GeoTileWorkRequest {
        version: artifact.request_version.clone(),
        center_cell: artifact.center_cell.clone(),
        halo_k: artifact.halo_k,
        features: artifact
            .features
            .iter()
            .map(|feature| GeoTileFeatureRef {
                source: feature.source.clone(),
                feature_id: feature.feature_id.clone(),
                home_cell: feature.home_cell.clone(),
            })
            .collect(),
        max_features: artifact.max_features,
        max_work_cells: artifact.max_work_cells,
    };
    let expected = materialize_tile_work_unit(&request).map_err(|error| {
        GeoTileError::new(
            GeoTileErrorCode::InvalidWorkUnit,
            "Geo tile reconciliation contains an invalid work-unit artifact",
            [
                ("center_cell", artifact.center_cell.clone()),
                ("cause", geo_tile_error_code_name(error.code)),
                ("cause_message", error.message),
            ],
        )
    })?;
    if &expected != artifact {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidWorkUnit,
            "Geo tile reconciliation work-unit artifact is not canonical",
            [("center_cell", artifact.center_cell.clone())],
        ));
    }
    let mut available_features = BTreeMap::new();
    let mut source_bindings = BTreeMap::new();
    for feature in &artifact.features {
        let home = parse_cell(&feature.home_cell, "work_unit.features.home_cell")?;
        validate_source_binding_consistency(&mut source_bindings, &feature.source)?;
        available_features.insert(
            source_feature_key(&feature.source, &feature.feature_id),
            home,
        );
    }
    let center = parse_cell(&artifact.center_cell, "work_unit.center_cell")?;
    Ok(ValidatedWorkUnit {
        center,
        feature_home_cells: available_features,
        source_bindings,
        blake3: artifact.work_unit_blake3.clone(),
    })
}

fn geo_tile_error_code_name(code: GeoTileErrorCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn parse_cell(value: &str, field: &str) -> Result<CellIndex, GeoTileError> {
    let cell = CellIndex::from_str(value).map_err(|error| {
        GeoTileError::new(
            GeoTileErrorCode::InvalidH3Cell,
            "Geo tile contract contains an invalid H3 cell",
            [
                ("field", field.to_string()),
                ("value", value.to_string()),
                ("error", error.to_string()),
            ],
        )
    })?;
    if cell.to_string() != value {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidH3Cell,
            "Geo tile contract contains a non-canonical H3 cell encoding",
            [
                ("field", field.to_string()),
                ("value", value.to_string()),
                ("canonical", cell.to_string()),
            ],
        ));
    }
    Ok(cell)
}

fn parse_home_coordinate(
    axis: &str,
    value: &str,
    decimal_places: u32,
) -> Result<i64, GeoTileError> {
    parse_fixed_decimal(axis, value, decimal_places).map_err(|error| {
        GeoTileError::new(
            GeoTileErrorCode::InvalidCoordinate,
            "Geo home-cell representative point is not an admissible fixed decimal",
            [
                ("axis", axis.to_string()),
                ("value", value.to_string()),
                ("cause", format!("{:?}", error.code)),
                ("cause_message", error.message),
            ],
        )
    })
}

fn fixed_decimal_scale(decimal_places: u32) -> Result<i64, GeoTileError> {
    let mut scale = 1_i64;
    for _ in 0..decimal_places {
        scale = scale
            .checked_mul(10)
            .ok_or_else(|| GeoTileError::overflow("coordinate_decimal_scale"))?;
    }
    Ok(scale)
}

fn validate_longitude_latitude(
    longitude: i64,
    latitude: i64,
    scale: i64,
) -> Result<(), GeoTileError> {
    let max_longitude = scale
        .checked_mul(180)
        .ok_or_else(|| GeoTileError::overflow("longitude_bound"))?;
    let max_latitude = scale
        .checked_mul(90)
        .ok_or_else(|| GeoTileError::overflow("latitude_bound"))?;
    if longitude < -max_longitude
        || longitude > max_longitude
        || latitude < -max_latitude
        || latitude > max_latitude
    {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidCoordinate,
            "Geo home-cell representative point falls outside longitude/latitude bounds",
            [
                ("longitude_fixed", longitude.to_string()),
                ("latitude_fixed", latitude.to_string()),
                ("coordinate_scale", scale.to_string()),
            ],
        ));
    }
    Ok(())
}

fn cell_for_fixed(
    longitude: i64,
    latitude: i64,
    scale: i64,
    resolution: Resolution,
) -> Result<CellIndex, GeoTileError> {
    let longitude_degrees = (longitude as f64) / (scale as f64);
    let latitude_degrees = (latitude as f64) / (scale as f64);
    let point = LatLng::new(latitude_degrees, longitude_degrees).map_err(|error| {
        GeoTileError::new(
            GeoTileErrorCode::InvalidCoordinate,
            "Geo home-cell representative point could not enter h3o",
            [
                ("longitude_fixed", longitude.to_string()),
                ("latitude_fixed", latitude.to_string()),
                ("coordinate_scale", scale.to_string()),
                ("error", error.to_string()),
            ],
        )
    })?;
    Ok(point.to_cell(resolution))
}

fn stability_cells(
    longitude: i64,
    latitude: i64,
    scale: i64,
    radius: i64,
    resolution: Resolution,
) -> Result<(Vec<String>, u32), GeoTileError> {
    let home = cell_for_fixed(longitude, latitude, scale, resolution)?;
    let mut cells = BTreeSet::new();
    for longitude_offset in [-radius, 0, radius] {
        for latitude_offset in [-radius, 0, radius] {
            let probed_longitude = longitude
                .checked_add(longitude_offset)
                .ok_or_else(|| GeoTileError::overflow("stability_probe.longitude"))?;
            let probed_latitude = latitude
                .checked_add(latitude_offset)
                .ok_or_else(|| GeoTileError::overflow("stability_probe.latitude"))?;
            validate_longitude_latitude(probed_longitude, probed_latitude, scale)?;
            cells.insert(cell_for_fixed(
                probed_longitude,
                probed_latitude,
                scale,
                resolution,
            )?);
        }
    }
    let mut minimum_halo_k = 0_u32;
    for cell in &cells {
        let distance = home.grid_distance(*cell).map_err(|error| {
            GeoTileError::new(
                GeoTileErrorCode::InvalidH3Cell,
                "Geo home-cell stability probe has no deterministic local H3 grid distance",
                [
                    ("home_cell", home.to_string()),
                    ("probe_cell", cell.to_string()),
                    ("error", error.to_string()),
                ],
            )
        })?;
        let distance = u32::try_from(distance)
            .map_err(|_| GeoTileError::overflow("minimum_stability_halo_k"))?;
        minimum_halo_k = minimum_halo_k.max(distance);
    }
    Ok((
        cells.into_iter().map(|cell| cell.to_string()).collect(),
        minimum_halo_k,
    ))
}

fn validate_stability_radius(radius: i64, scale: i64) -> Result<(), GeoTileError> {
    let scaled_radius = u128::try_from(radius)
        .map_err(|_| GeoTileError::overflow("stability_radius_fixed"))?
        .checked_mul(10_000)
        .ok_or_else(|| GeoTileError::overflow("stability_radius_degrees"))?;
    if scaled_radius > u128::try_from(scale).map_err(|_| GeoTileError::overflow("scale"))? {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidInput,
            "Geo home-cell stability radius exceeds 0.0001 coordinate degrees",
            [
                ("stability_radius_fixed", radius.to_string()),
                ("coordinate_scale", scale.to_string()),
                ("maximum_degrees", "0.0001".to_string()),
            ],
        ));
    }
    Ok(())
}

fn parse_claimed_cell(
    value: &str,
    expected_resolution: Resolution,
) -> Result<CellIndex, GeoTileError> {
    let cell = parse_cell(value, "rows[].claimed_home_cell")?;
    if cell.resolution() != expected_resolution {
        return Err(GeoTileError::new(
            GeoTileErrorCode::ResolutionMismatch,
            "Claimed Geo home cell uses a different H3 resolution",
            [
                (
                    "expected_resolution",
                    u8::from(expected_resolution).to_string(),
                ),
                ("actual_resolution", u8::from(cell.resolution()).to_string()),
                ("actual_cell", value.to_string()),
            ],
        ));
    }
    Ok(cell)
}

fn validate_sha256(field: &str, value: &str) -> Result<(), GeoTileError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidSourceDigest,
            "Geo home-cell source geometry digest must be canonical lowercase SHA-256",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_transform_pair(row: &GeoHomeCellRow) -> Result<(), GeoTileError> {
    match (
        row.transform_execution_id.as_deref(),
        row.transform_definition_id.as_deref(),
    ) {
        (None, None) => Ok(()),
        (Some(execution), Some(definition)) => {
            validate_identifier("rows[].transform_execution_id", execution)?;
            validate_identifier("rows[].transform_definition_id", definition)
        }
        _ => Err(GeoTileError::new(
            GeoTileErrorCode::InvalidInput,
            "Geo home-cell transform execution and definition identifiers must be supplied together",
            [
                ("source_instance_id", row.source.source_instance_id.as_str()),
                ("feature_id", row.feature_id.as_str()),
            ],
        )),
    }
}

type SourceSignature = (GeoTileSourceBinding, String, Option<String>, Option<String>);

fn validate_source_signature(
    signatures: &mut BTreeMap<String, SourceSignature>,
    row: &GeoHomeCellRow,
) -> Result<(), GeoTileError> {
    let signature = (
        row.source.clone(),
        row.representative_point_method.clone(),
        row.transform_execution_id.clone(),
        row.transform_definition_id.clone(),
    );
    if let Some(expected) = signatures.get(&row.source.source_instance_id) {
        if expected != &signature {
            return Err(GeoTileError::new(
                GeoTileErrorCode::IncompatibleSourceBinding,
                "One Geo source instance cannot mix releases, native scope, inventory references, point methods, or transform executions",
                [
                    ("source_instance_id", row.source.source_instance_id.as_str()),
                    ("release_id", row.source.release.release_id.as_str()),
                ],
            ));
        }
    } else {
        signatures.insert(row.source.source_instance_id.clone(), signature);
    }
    Ok(())
}

fn validate_source_binding(
    field: &str,
    binding: &GeoTileSourceBinding,
) -> Result<(), GeoTileError> {
    validate_identifier(
        &format!("{field}.source_instance_id"),
        &binding.source_instance_id,
    )?;
    validate_identifier(
        &format!("{field}.release.release_id"),
        &binding.release.release_id,
    )?;
    validate_source_blake3(
        &format!("{field}.release.release_digest"),
        &binding.release.release_digest,
    )?;
    validate_identifier(
        &format!("{field}.inventory_ref.inventory_id"),
        &binding.inventory_ref.inventory_id,
    )?;
    validate_source_blake3(
        &format!("{field}.inventory_ref.semantic_hash"),
        &binding.inventory_ref.semantic_hash,
    )?;
    validate_source_blake3(
        &format!("{field}.inventory_ref.planning_hash"),
        &binding.inventory_ref.planning_hash,
    )
}

fn validate_source_binding_consistency(
    bindings: &mut SourceBindings,
    binding: &GeoTileSourceBinding,
) -> Result<(), GeoTileError> {
    if let Some(expected) = bindings.get(&binding.source_instance_id) {
        if expected != binding {
            return Err(GeoTileError::new(
                GeoTileErrorCode::IncompatibleSourceBinding,
                "One Geo source instance cannot mix release identity, native entity scope, or inventory reference",
                [
                    ("source_instance_id", binding.source_instance_id.as_str()),
                    ("release_id", binding.release.release_id.as_str()),
                ],
            ));
        }
    } else {
        bindings.insert(binding.source_instance_id.clone(), binding.clone());
    }
    Ok(())
}

fn validate_inventory_lineage(
    lineage: &GeoTileInventoryLineage,
) -> Result<ValidatedInventoryLineage, GeoTileError> {
    let canonical = canonicalize_regional_inventory(&lineage.inventory).map_err(|error| {
        GeoTileError::new(
            GeoTileErrorCode::InvalidInventoryLineage,
            "Geo tile inventory lineage contains an invalid regional inventory",
            [
                ("cause", format!("{:?}", error.code)),
                ("cause_message", error.message),
            ],
        )
    })?;
    let semantic_hash = regional_inventory_semantic_hash(&canonical).map_err(|error| {
        GeoTileError::new(
            GeoTileErrorCode::InvalidInventoryLineage,
            "Geo tile inventory semantic hash could not be recomputed",
            [("cause_message", error.message)],
        )
    })?;
    let planning_hash = regional_inventory_planning_hash(&canonical).map_err(|error| {
        GeoTileError::new(
            GeoTileErrorCode::InvalidInventoryLineage,
            "Geo tile inventory planning hash could not be recomputed",
            [("cause_message", error.message)],
        )
    })?;
    for (field, actual, expected) in [
        (
            "inventory_ref.inventory_id",
            lineage.inventory_ref.inventory_id.as_str(),
            canonical.inventory_id.as_str(),
        ),
        (
            "inventory_ref.semantic_hash",
            lineage.inventory_ref.semantic_hash.as_str(),
            semantic_hash.as_str(),
        ),
        (
            "inventory_ref.planning_hash",
            lineage.inventory_ref.planning_hash.as_str(),
            planning_hash.as_str(),
        ),
    ] {
        if actual != expected {
            return Err(GeoTileError::new(
                GeoTileErrorCode::InvalidInventoryLineage,
                "Geo tile inventory lineage does not match its recomputed plan reference",
                [
                    ("field", field.to_string()),
                    ("expected", expected.to_string()),
                    ("actual", actual.to_string()),
                ],
            ));
        }
    }
    let sources = canonical
        .sources
        .into_iter()
        .map(|source| (source.source_instance_id.clone(), source))
        .collect();
    Ok(ValidatedInventoryLineage {
        inventory_ref: lineage.inventory_ref.clone(),
        sources,
    })
}

fn validate_member_inventory_authority(
    member: &GeoTileDecisionMember,
    lineage: &ValidatedInventoryLineage,
) -> Result<(), GeoTileError> {
    if member.source.inventory_ref != lineage.inventory_ref {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidInventoryLineage,
            "Geo stable-identity member is not bound to the validated plan inventory reference",
            [
                (
                    "source_instance_id",
                    member.source.source_instance_id.clone(),
                ),
                (
                    "member_inventory_id",
                    member.source.inventory_ref.inventory_id.clone(),
                ),
                (
                    "validated_inventory_id",
                    lineage.inventory_ref.inventory_id.clone(),
                ),
            ],
        ));
    }
    let Some(source) = lineage.sources.get(&member.source.source_instance_id) else {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidInventoryLineage,
            "Geo stable-identity member source is absent from the validated regional inventory",
            [
                (
                    "source_instance_id",
                    member.source.source_instance_id.clone(),
                ),
                ("inventory_id", lineage.inventory_ref.inventory_id.clone()),
            ],
        ));
    };
    if source.release != member.source.release || source.native_scope != member.source.native_scope
    {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidInventoryLineage,
            "Geo stable-identity member release or native scope disagrees with the validated regional inventory",
            [
                (
                    "source_instance_id",
                    member.source.source_instance_id.clone(),
                ),
                ("release_id", member.source.release.release_id.clone()),
            ],
        ));
    }
    Ok(())
}

fn source_feature_key(source: &GeoTileSourceBinding, feature_id: &str) -> SourceFeatureKey {
    (source.clone(), feature_id.to_string())
}

fn entity_level_name(level: GeoControlEntityLevel) -> String {
    match level {
        GeoControlEntityLevel::Site => "site",
        GeoControlEntityLevel::Property => "property",
        GeoControlEntityLevel::Parcel => "parcel",
        GeoControlEntityLevel::Building => "building",
        GeoControlEntityLevel::Unit => "unit",
        GeoControlEntityLevel::Address => "address",
        GeoControlEntityLevel::Poi => "poi",
    }
    .to_string()
}

fn relation_name(relation: GeoControlRelation) -> String {
    match relation {
        GeoControlRelation::SameAs => "same_as",
        GeoControlRelation::Contains => "contains",
        GeoControlRelation::PartOf => "part_of",
        GeoControlRelation::Within => "within",
        GeoControlRelation::On => "on",
        GeoControlRelation::Fronts => "fronts",
        GeoControlRelation::Intersects => "intersects",
    }
    .to_string()
}

fn require_resolution(
    expected: CellIndex,
    actual: CellIndex,
    actual_text: &str,
) -> Result<(), GeoTileError> {
    if expected.resolution() != actual.resolution() {
        return Err(resolution_error(expected.resolution(), actual, actual_text));
    }
    Ok(())
}

fn resolution_error(
    expected: h3o::Resolution,
    actual: CellIndex,
    actual_text: &str,
) -> GeoTileError {
    GeoTileError::new(
        GeoTileErrorCode::ResolutionMismatch,
        "Geo tile cells use different H3 resolutions",
        [
            ("expected_resolution", u8::from(expected).to_string()),
            (
                "actual_resolution",
                u8::from(actual.resolution()).to_string(),
            ),
            ("actual_cell", actual_text.to_string()),
        ],
    )
}

fn bounded_grid_disk(
    center: CellIndex,
    halo_k: u32,
    max_work_cells: u64,
) -> Result<BTreeSet<CellIndex>, GeoTileError> {
    let k = u64::from(halo_k);
    let theoretical = k
        .checked_add(1)
        .and_then(|next| k.checked_mul(next))
        .and_then(|product| product.checked_mul(3))
        .and_then(|product| product.checked_add(1))
        .ok_or_else(|| GeoTileError::overflow("halo_cell_upper_bound"))?;
    if theoretical > max_work_cells {
        return Err(GeoTileError::new(
            GeoTileErrorCode::HaloBudgetExceeded,
            "Geo tile halo exceeds the declared cell budget before enumeration",
            [
                ("halo_k", halo_k.to_string()),
                ("upper_bound", theoretical.to_string()),
                ("configured", max_work_cells.to_string()),
            ],
        ));
    }
    let disk = center.grid_disk_safe(halo_k).collect::<BTreeSet<_>>();
    let observed = usize_to_u64(disk.len(), "work_cells.len")?;
    if observed > max_work_cells {
        return Err(GeoTileError::new(
            GeoTileErrorCode::HaloBudgetExceeded,
            "Geo tile halo exceeds the declared cell budget",
            [
                ("observed", observed.to_string()),
                ("configured", max_work_cells.to_string()),
            ],
        ));
    }
    Ok(disk)
}

fn validate_identifier(field: &str, value: &str) -> Result<(), GeoTileError> {
    if value.is_empty() || value.len() > MAX_IDENTIFIER_BYTES {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidInput,
            "Geo tile identifier is empty or exceeds its byte budget",
            [
                ("field", field.to_string()),
                ("bytes", value.len().to_string()),
                ("max_bytes", MAX_IDENTIFIER_BYTES.to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_decision_blake3(field: &str, value: &str) -> Result<(), GeoTileError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(invalid_decision_digest(field, value));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid_decision_digest(field, value));
    }
    Ok(())
}

fn validate_source_blake3(field: &str, value: &str) -> Result<(), GeoTileError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(invalid_source_digest(field, value));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid_source_digest(field, value));
    }
    Ok(())
}

fn invalid_source_digest(field: &str, value: &str) -> GeoTileError {
    GeoTileError::new(
        GeoTileErrorCode::InvalidSourceDigest,
        "Geo tile source-binding digest must be canonical lowercase BLAKE3",
        [("field", field), ("value", value)],
    )
}

fn invalid_decision_digest(field: &str, value: &str) -> GeoTileError {
    GeoTileError::new(
        GeoTileErrorCode::InvalidDecision,
        "Geo tile decision digest must be canonical lowercase BLAKE3",
        [("field", field), ("value", value)],
    )
}

fn decision_id(decision_scope_blake3: &str, payload_blake3: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"canon_geo_tile_decision.v1\0");
    hasher.update(decision_scope_blake3.as_bytes());
    hasher.update(b"\0");
    hasher.update(payload_blake3.as_bytes());
    format!("geo-decision:{}", hasher.finalize().to_hex())
}

fn validate_budget(field: &str, value: u64, hard_max: u64) -> Result<(), GeoTileError> {
    if value == 0 || value > hard_max {
        return Err(GeoTileError::new(
            GeoTileErrorCode::InvalidInput,
            "Geo tile budget must be positive and within the kernel ceiling",
            [
                ("field", field.to_string()),
                ("configured", value.to_string()),
                ("hard_max", hard_max.to_string()),
            ],
        ));
    }
    Ok(())
}

fn checked_add(left: u64, right: u64, field: &str) -> Result<u64, GeoTileError> {
    left.checked_add(right)
        .ok_or_else(|| GeoTileError::overflow(field))
}

fn usize_to_u64(value: usize, field: &str) -> Result<u64, GeoTileError> {
    u64::try_from(value).map_err(|_| GeoTileError::overflow(field))
}
