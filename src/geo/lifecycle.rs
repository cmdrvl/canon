#![forbid(unsafe_code)]

//! Temporal containment query helpers for Geo mart artifacts.
//!
//! This module is intentionally narrower than the deferred Allen/STP temporal
//! solver in `docs/PLAN_CANON_GEO.md`. It validates and queries reviewed
//! parent/child containment edges at a whole-day `as_of` time; it does not
//! change composition decisions, solve scores, or Canon's exact registry replay
//! path.

use super::GeoEntityLevel;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_TEMPORAL_CONTAINMENT_VERSION: &str = "canon_geo_temporal_containment.v0";
pub const CANON_GEO_AS_OF_RESOLUTION_REQUEST_VERSION: &str =
    "canon_geo_as_of_resolution_request.v0";
pub const CANON_GEO_AS_OF_RESOLUTION_VERSION: &str = "canon_geo_as_of_resolution.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoLifecycleError {
    pub code: GeoLifecycleErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoLifecycleErrorCode {
    UnsupportedVersion,
    InvalidInput,
    ArithmeticOverflow,
}

impl GeoLifecycleError {
    fn new<K, V>(
        code: GeoLifecycleErrorCode,
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (K, V)>,
    ) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            code,
            message: message.into(),
            detail: detail
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }

    fn invalid<K, V>(message: impl Into<String>, detail: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        Self::new(GeoLifecycleErrorCode::InvalidInput, message, detail)
    }
}

impl fmt::Display for GeoLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeoLifecycleError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoAsOfResolutionError {
    pub code: GeoAsOfResolutionErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAsOfResolutionErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
    OutsideAvailableVintage,
}

impl GeoAsOfResolutionError {
    fn new<K, V>(
        code: GeoAsOfResolutionErrorCode,
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (K, V)>,
    ) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            code,
            message: message.into(),
            detail: detail
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }

    fn invalid<K, V>(message: impl Into<String>, detail: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        Self::new(GeoAsOfResolutionErrorCode::InvalidInput, message, detail)
    }
}

impl fmt::Display for GeoAsOfResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeoAsOfResolutionError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAsOfResolutionRequest {
    pub version: String,
    pub as_of_utc_day: String,
    pub tile_layer: GeoAsOfLayerWindow,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_layer: Option<GeoAsOfLayerWindow>,
    pub lookups: Vec<GeoAsOfParcelLookup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parcel_vintages: Vec<GeoMapPlutoParcelVintageRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub change_ledger: Vec<GeoBblChangeLedgerRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAsOfLayerWindow {
    pub layer_id: String,
    pub source_dataset: String,
    pub vintage_id: String,
    pub valid_from_utc_day: String,
    pub valid_to_utc_day: String,
    pub content_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAsOfParcelLookup {
    pub lookup_id: String,
    pub bbl_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoMapPlutoParcelVintageRow {
    #[serde(alias = "BBL_KEY")]
    pub bbl_key: String,
    #[serde(alias = "RELEASE")]
    pub release: String,
    #[serde(alias = "RELEASE_DT")]
    pub release_dt: String,
    #[serde(alias = "VALID_FROM_RELEASE_DT")]
    pub valid_from_release_dt: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "VALID_TO_RELEASE_DT"
    )]
    pub valid_to_release_dt: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "GEOM_WKT_SHA256"
    )]
    pub geometry_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parcel_cluster_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_record_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_record_blake3: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoBblChangeLedgerRow {
    #[serde(alias = "CHANGE_EVENT_ID")]
    pub change_event_id: String,
    #[serde(alias = "EVENT_TYPE")]
    pub event_type: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "CANON_RESOLUTION"
    )]
    pub canon_resolution: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "PREVIOUS_RELEASE"
    )]
    pub previous_release: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "PREVIOUS_RELEASE_DT"
    )]
    pub previous_release_dt: Option<String>,
    #[serde(alias = "CURRENT_RELEASE")]
    pub current_release: String,
    #[serde(alias = "CURRENT_RELEASE_DT")]
    pub current_release_dt: String,
    #[serde(alias = "SUBJECT_BBL_KEY")]
    pub subject_bbl_key: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "RESOLVED_BBL_KEY"
    )]
    pub resolved_bbl_key: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "CANONICAL_BBL_KEY"
    )]
    pub canonical_bbl_key: Option<String>,
    #[serde(default, alias = "PREDECESSOR_CANDIDATE_BBL_KEYS")]
    pub predecessor_candidate_bbl_keys: Vec<String>,
    #[serde(default, alias = "SUCCESSOR_CANDIDATE_BBL_KEYS")]
    pub successor_candidate_bbl_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_record_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_record_blake3: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAsOfParcelResolutionStatus {
    Resolved,
    Abstained,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAsOfParcelResolutionReason {
    ActiveAtAsOf,
    AmbiguousVintageRows,
    ChangedBeforeAsOf,
    NotPresentAsOf,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoBblChangeEventRef {
    pub change_event_id: String,
    pub event_type: String,
    pub current_release: String,
    pub current_release_dt: String,
    pub subject_bbl_key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub predecessor_bbl_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub successor_bbl_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAsOfParcelResolution {
    pub lookup_id: String,
    pub bbl_key: String,
    pub status: GeoAsOfParcelResolutionStatus,
    pub reason: GeoAsOfParcelResolutionReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parcel_cluster_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_release: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_release_dt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_valid_from_release_dt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_valid_to_release_dt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub change_events: Vec<GeoBblChangeEventRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAsOfResolutionArtifact {
    pub version: String,
    pub request_blake3: String,
    pub as_of_utc_day: String,
    pub tile_layer: GeoAsOfLayerWindow,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_layer: Option<GeoAsOfLayerWindow>,
    pub resolutions: Vec<GeoAsOfParcelResolution>,
    pub summary: GeoAsOfResolutionSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAsOfResolutionSummary {
    pub lookups: u64,
    pub resolved: u64,
    pub abstained: u64,
    pub change_events_used: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTemporalContainmentArtifact {
    pub version: String,
    pub mart_id: String,
    pub clusters: Vec<GeoTemporalContainmentCluster>,
    pub edges: Vec<GeoTemporalContainmentEdge>,
    pub summary: GeoTemporalContainmentSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTemporalContainmentCluster {
    pub cluster_id: String,
    pub entity_level: GeoEntityLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoTemporalContainmentRelation {
    PartOf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTemporalContainmentEdge {
    pub edge_id: String,
    pub parent_cluster_id: String,
    pub parent_level: GeoEntityLevel,
    pub child_cluster_id: String,
    pub child_level: GeoEntityLevel,
    pub relation: GeoTemporalContainmentRelation,
    pub valid_interval: GeoTemporalContainmentInterval,
    pub source_receipt: GeoTemporalContainmentSourceReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTemporalContainmentInterval {
    pub start_utc_day: String,
    pub end_utc_day: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTemporalContainmentSourceReceipt {
    pub receipt_id: String,
    pub source_dataset: String,
    pub source_record_id: String,
    pub source_record_blake3: String,
    pub proof_class: String,
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTemporalContainmentSummary {
    pub clusters: u64,
    pub edges: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoContainmentAsOfQuery {
    pub as_of_utc_day: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_cluster_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_cluster_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoContainmentAsOfArtifact {
    pub version: String,
    pub mart_id: String,
    pub as_of_utc_day: String,
    pub edges: Vec<GeoTemporalContainmentEdge>,
    pub summary: GeoContainmentAsOfSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoContainmentAsOfSummary {
    pub edges: u64,
    pub parent_clusters: u64,
    pub child_clusters: u64,
}

pub fn validate_temporal_containment_artifact(
    artifact: &GeoTemporalContainmentArtifact,
) -> Result<(), GeoLifecycleError> {
    validate_temporal_containment_artifact_inner(artifact, true)
}

pub fn canonical_temporal_containment_artifact(
    artifact: &GeoTemporalContainmentArtifact,
) -> Result<GeoTemporalContainmentArtifact, GeoLifecycleError> {
    validate_temporal_containment_artifact_inner(artifact, false)?;
    let mut canonical = artifact.clone();
    canonical
        .clusters
        .sort_by(|left, right| left.cluster_id.cmp(&right.cluster_id));
    canonical.edges.sort_by(edge_sort_order);
    canonical.summary = GeoTemporalContainmentSummary {
        clusters: usize_to_u64(canonical.clusters.len(), "summary.clusters")?,
        edges: usize_to_u64(canonical.edges.len(), "summary.edges")?,
    };
    validate_temporal_containment_artifact(&canonical)?;
    Ok(canonical)
}

pub fn canonical_temporal_containment_bytes(
    artifact: &GeoTemporalContainmentArtifact,
) -> Result<Vec<u8>, GeoLifecycleError> {
    let canonical = canonical_temporal_containment_artifact(artifact)?;
    serde_json::to_vec(&canonical).map_err(|error| {
        GeoLifecycleError::invalid(
            "Geo temporal-containment artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub fn containment_as_of(
    artifact: &GeoTemporalContainmentArtifact,
    query: &GeoContainmentAsOfQuery,
) -> Result<GeoContainmentAsOfArtifact, GeoLifecycleError> {
    let canonical = canonical_temporal_containment_artifact(artifact)?;
    validate_utc_day("as_of_utc_day", &query.as_of_utc_day)?;
    if let Some(parent_cluster_id) = &query.parent_cluster_id {
        validate_cluster_identifier("parent_cluster_id", parent_cluster_id)?;
    }
    if let Some(child_cluster_id) = &query.child_cluster_id {
        validate_cluster_identifier("child_cluster_id", child_cluster_id)?;
    }

    let edges = canonical
        .edges
        .iter()
        .filter(|edge| {
            edge.valid_interval.start_utc_day.as_str() <= query.as_of_utc_day.as_str()
                && query.as_of_utc_day.as_str() <= edge.valid_interval.end_utc_day.as_str()
                && query
                    .parent_cluster_id
                    .as_ref()
                    .is_none_or(|parent| parent == &edge.parent_cluster_id)
                && query
                    .child_cluster_id
                    .as_ref()
                    .is_none_or(|child| child == &edge.child_cluster_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    let summary = containment_summary(&edges)?;
    Ok(GeoContainmentAsOfArtifact {
        version: canonical.version,
        mart_id: canonical.mart_id,
        as_of_utc_day: query.as_of_utc_day.clone(),
        edges,
        summary,
    })
}

pub fn resolve_geo_as_of(
    request: &GeoAsOfResolutionRequest,
) -> Result<GeoAsOfResolutionArtifact, GeoAsOfResolutionError> {
    let canonical_request = canonical_as_of_resolution_request(request)?;
    ensure_as_of_inside_layer(
        "tile_layer",
        &canonical_request.as_of_utc_day,
        &canonical_request.tile_layer,
    )?;
    if let Some(client_layer) = &canonical_request.client_layer {
        ensure_as_of_inside_layer(
            "client_layer",
            &canonical_request.as_of_utc_day,
            client_layer,
        )?;
    }

    let request_blake3 = hash_as_of_request(&canonical_request)?;
    let mut resolutions = Vec::with_capacity(canonical_request.lookups.len());
    for lookup in &canonical_request.lookups {
        resolutions.push(resolve_geo_as_of_lookup(&canonical_request, lookup)?);
    }
    resolutions.sort_by(|left, right| left.lookup_id.cmp(&right.lookup_id));
    let summary = as_of_resolution_summary(&resolutions)?;
    let artifact = GeoAsOfResolutionArtifact {
        version: CANON_GEO_AS_OF_RESOLUTION_VERSION.to_string(),
        request_blake3,
        as_of_utc_day: canonical_request.as_of_utc_day,
        tile_layer: canonical_request.tile_layer,
        client_layer: canonical_request.client_layer,
        resolutions,
        summary,
    };
    validate_as_of_resolution_artifact(&artifact)?;
    Ok(artifact)
}

pub fn canonical_as_of_resolution_request(
    request: &GeoAsOfResolutionRequest,
) -> Result<GeoAsOfResolutionRequest, GeoAsOfResolutionError> {
    validate_as_of_resolution_request(request)?;
    let mut canonical = request.clone();
    canonical
        .lookups
        .sort_by(|left, right| left.lookup_id.cmp(&right.lookup_id));
    canonical.parcel_vintages.sort_by(|left, right| {
        left.bbl_key
            .cmp(&right.bbl_key)
            .then_with(|| left.valid_from_release_dt.cmp(&right.valid_from_release_dt))
            .then_with(|| left.valid_to_release_dt.cmp(&right.valid_to_release_dt))
            .then_with(|| left.release_dt.cmp(&right.release_dt))
            .then_with(|| left.release.cmp(&right.release))
    });
    canonical.change_ledger.sort_by(|left, right| {
        left.change_event_id
            .cmp(&right.change_event_id)
            .then_with(|| left.current_release_dt.cmp(&right.current_release_dt))
    });
    Ok(canonical)
}

pub fn canonical_as_of_resolution_bytes(
    artifact: &GeoAsOfResolutionArtifact,
) -> Result<Vec<u8>, GeoAsOfResolutionError> {
    validate_as_of_resolution_artifact(artifact)?;
    let mut canonical = artifact.clone();
    canonical
        .resolutions
        .sort_by(|left, right| left.lookup_id.cmp(&right.lookup_id));
    for resolution in &mut canonical.resolutions {
        resolution.change_events.sort_by(|left, right| {
            left.change_event_id
                .cmp(&right.change_event_id)
                .then_with(|| left.current_release_dt.cmp(&right.current_release_dt))
        });
    }
    canonical.summary = as_of_resolution_summary(&canonical.resolutions)?;
    serde_json::to_vec(&canonical).map_err(|error| {
        GeoAsOfResolutionError::invalid(
            "Geo as-of resolution artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub fn validate_as_of_resolution_artifact(
    artifact: &GeoAsOfResolutionArtifact,
) -> Result<(), GeoAsOfResolutionError> {
    if artifact.version != CANON_GEO_AS_OF_RESOLUTION_VERSION {
        return Err(GeoAsOfResolutionError::new(
            GeoAsOfResolutionErrorCode::UnsupportedVersion,
            "Unsupported Geo as-of resolution artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_AS_OF_RESOLUTION_VERSION),
            ],
        ));
    }
    validate_as_of_utc_day("as_of_utc_day", &artifact.as_of_utc_day)?;
    validate_as_of_blake3_uri("request_blake3", &artifact.request_blake3)?;
    validate_as_of_layer("tile_layer", &artifact.tile_layer)?;
    if let Some(client_layer) = &artifact.client_layer {
        validate_as_of_layer("client_layer", client_layer)?;
    }
    validate_as_of_resolution_order(&artifact.resolutions)?;
    for resolution in &artifact.resolutions {
        validate_as_of_resolution_row(resolution)?;
    }
    let summary = as_of_resolution_summary(&artifact.resolutions)?;
    if artifact.summary != summary {
        return Err(GeoAsOfResolutionError::invalid(
            "Geo as-of resolution summary does not match resolutions",
            [
                ("field", "summary".to_string()),
                ("actual", format!("{:?}", artifact.summary)),
                ("expected", format!("{summary:?}")),
            ],
        ));
    }
    Ok(())
}

fn validate_as_of_resolution_request(
    request: &GeoAsOfResolutionRequest,
) -> Result<(), GeoAsOfResolutionError> {
    if request.version != CANON_GEO_AS_OF_RESOLUTION_REQUEST_VERSION {
        return Err(GeoAsOfResolutionError::new(
            GeoAsOfResolutionErrorCode::UnsupportedVersion,
            "Unsupported Geo as-of resolution request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_AS_OF_RESOLUTION_REQUEST_VERSION),
            ],
        ));
    }
    validate_as_of_utc_day("as_of_utc_day", &request.as_of_utc_day)?;
    validate_as_of_layer("tile_layer", &request.tile_layer)?;
    if let Some(client_layer) = &request.client_layer {
        validate_as_of_layer("client_layer", client_layer)?;
    }
    if request.lookups.is_empty() {
        return Err(as_of_invalid_field(
            "lookups",
            "Geo as-of resolution requires at least one requested parcel identifier",
            "0",
        ));
    }
    let mut lookup_ids = BTreeSet::new();
    for lookup in &request.lookups {
        validate_as_of_string("lookups[].lookup_id", &lookup.lookup_id)?;
        validate_bbl_key("lookups[].bbl_key", &lookup.bbl_key)?;
        if !lookup_ids.insert(lookup.lookup_id.clone()) {
            return Err(GeoAsOfResolutionError::invalid(
                "Geo as-of resolution lookup ids must be unique",
                [("lookup_id", lookup.lookup_id.clone())],
            ));
        }
    }
    for row in &request.parcel_vintages {
        validate_parcel_vintage_row(row)?;
    }
    for row in &request.change_ledger {
        validate_change_ledger_row(row)?;
    }
    Ok(())
}

fn resolve_geo_as_of_lookup(
    request: &GeoAsOfResolutionRequest,
    lookup: &GeoAsOfParcelLookup,
) -> Result<GeoAsOfParcelResolution, GeoAsOfResolutionError> {
    let active_rows = request
        .parcel_vintages
        .iter()
        .filter(|row| row.bbl_key == lookup.bbl_key)
        .filter(|row| parcel_row_is_usable_as_of(row, &request.as_of_utc_day))
        .collect::<Vec<_>>();

    if active_rows.len() == 1 {
        let row = active_rows[0];
        let cluster_id = row
            .parcel_cluster_id
            .clone()
            .unwrap_or_else(|| format!("cmdrvl:parcel:nyc:bbl:{}", row.bbl_key));
        validate_as_of_cluster_id("parcel_cluster_id", &cluster_id, GeoEntityLevel::Parcel)?;
        return Ok(GeoAsOfParcelResolution {
            lookup_id: lookup.lookup_id.clone(),
            bbl_key: lookup.bbl_key.clone(),
            status: GeoAsOfParcelResolutionStatus::Resolved,
            reason: GeoAsOfParcelResolutionReason::ActiveAtAsOf,
            parcel_cluster_id: Some(cluster_id),
            matched_release: Some(row.release.clone()),
            matched_release_dt: Some(row.release_dt.clone()),
            matched_valid_from_release_dt: Some(row.valid_from_release_dt.clone()),
            matched_valid_to_release_dt: row.valid_to_release_dt.clone(),
            change_events: Vec::new(),
        });
    }

    if active_rows.len() > 1 {
        return Ok(GeoAsOfParcelResolution {
            lookup_id: lookup.lookup_id.clone(),
            bbl_key: lookup.bbl_key.clone(),
            status: GeoAsOfParcelResolutionStatus::Abstained,
            reason: GeoAsOfParcelResolutionReason::AmbiguousVintageRows,
            parcel_cluster_id: None,
            matched_release: None,
            matched_release_dt: None,
            matched_valid_from_release_dt: None,
            matched_valid_to_release_dt: None,
            change_events: Vec::new(),
        });
    }

    let mut events = request
        .change_ledger
        .iter()
        .filter(|event| event.current_release_dt <= request.as_of_utc_day)
        .filter(|event| change_event_mentions_bbl(event, &lookup.bbl_key))
        .map(change_event_ref)
        .collect::<Result<Vec<_>, _>>()?;
    events.sort();
    events.dedup();

    Ok(GeoAsOfParcelResolution {
        lookup_id: lookup.lookup_id.clone(),
        bbl_key: lookup.bbl_key.clone(),
        status: GeoAsOfParcelResolutionStatus::Abstained,
        reason: if events.is_empty() {
            GeoAsOfParcelResolutionReason::NotPresentAsOf
        } else {
            GeoAsOfParcelResolutionReason::ChangedBeforeAsOf
        },
        parcel_cluster_id: None,
        matched_release: None,
        matched_release_dt: None,
        matched_valid_from_release_dt: None,
        matched_valid_to_release_dt: None,
        change_events: events,
    })
}

fn parcel_row_is_usable_as_of(row: &GeoMapPlutoParcelVintageRow, as_of_utc_day: &str) -> bool {
    row.release_dt.as_str() <= as_of_utc_day
        && row.valid_from_release_dt.as_str() <= as_of_utc_day
        && row
            .valid_to_release_dt
            .as_deref()
            .is_none_or(|valid_to| as_of_utc_day <= valid_to)
}

fn change_event_mentions_bbl(row: &GeoBblChangeLedgerRow, bbl_key: &str) -> bool {
    row.subject_bbl_key == bbl_key
        || row.resolved_bbl_key.as_deref() == Some(bbl_key)
        || row.canonical_bbl_key.as_deref() == Some(bbl_key)
        || row
            .predecessor_candidate_bbl_keys
            .iter()
            .any(|candidate| candidate == bbl_key)
        || row
            .successor_candidate_bbl_keys
            .iter()
            .any(|candidate| candidate == bbl_key)
}

fn change_event_ref(
    row: &GeoBblChangeLedgerRow,
) -> Result<GeoBblChangeEventRef, GeoAsOfResolutionError> {
    let mut predecessor_bbl_keys = row.predecessor_candidate_bbl_keys.clone();
    predecessor_bbl_keys.sort();
    predecessor_bbl_keys.dedup();
    let mut successor_bbl_keys = row.successor_candidate_bbl_keys.clone();
    successor_bbl_keys.sort();
    successor_bbl_keys.dedup();
    Ok(GeoBblChangeEventRef {
        change_event_id: row.change_event_id.clone(),
        event_type: row.event_type.clone(),
        current_release: row.current_release.clone(),
        current_release_dt: row.current_release_dt.clone(),
        subject_bbl_key: row.subject_bbl_key.clone(),
        predecessor_bbl_keys,
        successor_bbl_keys,
    })
}

fn as_of_resolution_summary(
    resolutions: &[GeoAsOfParcelResolution],
) -> Result<GeoAsOfResolutionSummary, GeoAsOfResolutionError> {
    let resolved = resolutions
        .iter()
        .filter(|resolution| resolution.status == GeoAsOfParcelResolutionStatus::Resolved)
        .count();
    let abstained = resolutions.len().checked_sub(resolved).ok_or_else(|| {
        GeoAsOfResolutionError::new(
            GeoAsOfResolutionErrorCode::ArithmeticOverflow,
            "Geo as-of resolution summary count underflowed",
            [("field", "summary.abstained")],
        )
    })?;
    let change_events_used = resolutions
        .iter()
        .map(|resolution| resolution.change_events.len())
        .try_fold(0_u64, |sum, len| {
            sum.checked_add(u64::try_from(len).map_err(|_| {
                GeoAsOfResolutionError::new(
                    GeoAsOfResolutionErrorCode::ArithmeticOverflow,
                    "Geo as-of resolution change event count does not fit in u64",
                    [("field", "summary.change_events_used")],
                )
            })?)
            .ok_or_else(|| {
                GeoAsOfResolutionError::new(
                    GeoAsOfResolutionErrorCode::ArithmeticOverflow,
                    "Geo as-of resolution change event count overflowed",
                    [("field", "summary.change_events_used")],
                )
            })
        })?;
    Ok(GeoAsOfResolutionSummary {
        lookups: u64::try_from(resolutions.len()).map_err(|_| {
            GeoAsOfResolutionError::new(
                GeoAsOfResolutionErrorCode::ArithmeticOverflow,
                "Geo as-of resolution lookup count does not fit in u64",
                [("field", "summary.lookups")],
            )
        })?,
        resolved: u64::try_from(resolved).map_err(|_| {
            GeoAsOfResolutionError::new(
                GeoAsOfResolutionErrorCode::ArithmeticOverflow,
                "Geo as-of resolution resolved count does not fit in u64",
                [("field", "summary.resolved")],
            )
        })?,
        abstained: u64::try_from(abstained).map_err(|_| {
            GeoAsOfResolutionError::new(
                GeoAsOfResolutionErrorCode::ArithmeticOverflow,
                "Geo as-of resolution abstained count does not fit in u64",
                [("field", "summary.abstained")],
            )
        })?,
        change_events_used,
    })
}

fn ensure_as_of_inside_layer(
    role: &'static str,
    as_of_utc_day: &str,
    layer: &GeoAsOfLayerWindow,
) -> Result<(), GeoAsOfResolutionError> {
    if as_of_utc_day < layer.valid_from_utc_day.as_str() {
        return Err(GeoAsOfResolutionError::new(
            GeoAsOfResolutionErrorCode::OutsideAvailableVintage,
            "Geo as-of request predates the earliest available layer vintage",
            [
                ("layer_role", role.to_string()),
                ("layer_id", layer.layer_id.clone()),
                ("as_of_utc_day", as_of_utc_day.to_string()),
                (
                    "earliest_available_utc_day",
                    layer.valid_from_utc_day.clone(),
                ),
            ],
        ));
    }
    if as_of_utc_day > layer.valid_to_utc_day.as_str() {
        return Err(GeoAsOfResolutionError::new(
            GeoAsOfResolutionErrorCode::OutsideAvailableVintage,
            "Geo as-of request is after the latest available layer vintage",
            [
                ("layer_role", role.to_string()),
                ("layer_id", layer.layer_id.clone()),
                ("as_of_utc_day", as_of_utc_day.to_string()),
                ("latest_available_utc_day", layer.valid_to_utc_day.clone()),
            ],
        ));
    }
    Ok(())
}

fn hash_as_of_request(
    request: &GeoAsOfResolutionRequest,
) -> Result<String, GeoAsOfResolutionError> {
    serde_json::to_vec(request)
        .map(|bytes| format!("blake3:{}", blake3::hash(&bytes).to_hex()))
        .map_err(|error| {
            GeoAsOfResolutionError::invalid(
                "Geo as-of resolution request could not be serialized for hashing",
                [("error", error.to_string())],
            )
        })
}

fn validate_as_of_layer(
    field: &'static str,
    layer: &GeoAsOfLayerWindow,
) -> Result<(), GeoAsOfResolutionError> {
    validate_as_of_string("layer_id", &layer.layer_id)?;
    validate_as_of_string("source_dataset", &layer.source_dataset)?;
    validate_as_of_string("vintage_id", &layer.vintage_id)?;
    validate_as_of_utc_day("valid_from_utc_day", &layer.valid_from_utc_day)?;
    validate_as_of_utc_day("valid_to_utc_day", &layer.valid_to_utc_day)?;
    if layer.valid_from_utc_day > layer.valid_to_utc_day {
        return Err(GeoAsOfResolutionError::invalid(
            "Geo as-of layer window start must not be after its end",
            [
                ("field", field.to_string()),
                ("valid_from_utc_day", layer.valid_from_utc_day.clone()),
                ("valid_to_utc_day", layer.valid_to_utc_day.clone()),
            ],
        ));
    }
    validate_as_of_blake3_uri("content_digest", &layer.content_digest)
}

fn validate_parcel_vintage_row(
    row: &GeoMapPlutoParcelVintageRow,
) -> Result<(), GeoAsOfResolutionError> {
    validate_bbl_key("parcel_vintages[].bbl_key", &row.bbl_key)?;
    validate_as_of_string("parcel_vintages[].release", &row.release)?;
    validate_as_of_utc_day("parcel_vintages[].release_dt", &row.release_dt)?;
    validate_as_of_utc_day(
        "parcel_vintages[].valid_from_release_dt",
        &row.valid_from_release_dt,
    )?;
    if let Some(valid_to) = &row.valid_to_release_dt {
        validate_as_of_utc_day("parcel_vintages[].valid_to_release_dt", valid_to)?;
        if row.valid_from_release_dt.as_str() > valid_to.as_str() {
            return Err(GeoAsOfResolutionError::invalid(
                "Geo MapPLUTO parcel-vintage validity interval is inverted",
                [
                    ("field", "parcel_vintages[].valid_interval".to_string()),
                    ("bbl_key", row.bbl_key.clone()),
                ],
            ));
        }
    }
    if let Some(cluster_id) = &row.parcel_cluster_id {
        validate_as_of_cluster_id(
            "parcel_vintages[].parcel_cluster_id",
            cluster_id,
            GeoEntityLevel::Parcel,
        )?;
    }
    if let Some(source_record_id) = &row.source_record_id {
        validate_as_of_string("parcel_vintages[].source_record_id", source_record_id)?;
    }
    if let Some(source_record_blake3) = &row.source_record_blake3 {
        validate_as_of_blake3_uri(
            "parcel_vintages[].source_record_blake3",
            source_record_blake3,
        )?;
    }
    if let Some(geometry_digest) = &row.geometry_digest {
        validate_as_of_string("parcel_vintages[].geometry_digest", geometry_digest)?;
    }
    Ok(())
}

fn validate_change_ledger_row(row: &GeoBblChangeLedgerRow) -> Result<(), GeoAsOfResolutionError> {
    validate_as_of_string("change_ledger[].change_event_id", &row.change_event_id)?;
    validate_as_of_string("change_ledger[].event_type", &row.event_type)?;
    if let Some(canon_resolution) = &row.canon_resolution {
        validate_as_of_string("change_ledger[].canon_resolution", canon_resolution)?;
    }
    if let Some(previous_release) = &row.previous_release {
        validate_as_of_string("change_ledger[].previous_release", previous_release)?;
    }
    if let Some(previous_release_dt) = &row.previous_release_dt {
        validate_as_of_utc_day("change_ledger[].previous_release_dt", previous_release_dt)?;
    }
    validate_as_of_string("change_ledger[].current_release", &row.current_release)?;
    validate_as_of_utc_day(
        "change_ledger[].current_release_dt",
        &row.current_release_dt,
    )?;
    validate_bbl_key("change_ledger[].subject_bbl_key", &row.subject_bbl_key)?;
    if let Some(resolved_bbl_key) = &row.resolved_bbl_key {
        validate_bbl_key("change_ledger[].resolved_bbl_key", resolved_bbl_key)?;
    }
    if let Some(canonical_bbl_key) = &row.canonical_bbl_key {
        validate_bbl_key("change_ledger[].canonical_bbl_key", canonical_bbl_key)?;
    }
    validate_bbl_list(
        "change_ledger[].predecessor_candidate_bbl_keys",
        &row.predecessor_candidate_bbl_keys,
    )?;
    validate_bbl_list(
        "change_ledger[].successor_candidate_bbl_keys",
        &row.successor_candidate_bbl_keys,
    )?;
    if let Some(source_record_id) = &row.source_record_id {
        validate_as_of_string("change_ledger[].source_record_id", source_record_id)?;
    }
    if let Some(source_record_blake3) = &row.source_record_blake3 {
        validate_as_of_blake3_uri("change_ledger[].source_record_blake3", source_record_blake3)?;
    }
    Ok(())
}

fn validate_bbl_list(field: &'static str, values: &[String]) -> Result<(), GeoAsOfResolutionError> {
    let mut seen = BTreeSet::new();
    for value in values {
        validate_bbl_key(field, value)?;
        if !seen.insert(value) {
            return Err(GeoAsOfResolutionError::invalid(
                "Geo BBL list values must be unique",
                [("field", field.to_string()), ("bbl_key", value.clone())],
            ));
        }
    }
    Ok(())
}

fn validate_as_of_resolution_order(
    resolutions: &[GeoAsOfParcelResolution],
) -> Result<(), GeoAsOfResolutionError> {
    let mut previous: Option<&str> = None;
    let mut seen = BTreeSet::new();
    for resolution in resolutions {
        if !seen.insert(resolution.lookup_id.as_str()) {
            return Err(GeoAsOfResolutionError::invalid(
                "Geo as-of resolution artifact contains duplicate lookup ids",
                [("lookup_id", resolution.lookup_id.clone())],
            ));
        }
        if let Some(previous_lookup_id) = previous
            && previous_lookup_id >= resolution.lookup_id.as_str()
        {
            return Err(GeoAsOfResolutionError::invalid(
                "Geo as-of resolution rows must be sorted by lookup_id",
                [
                    ("field", "resolutions".to_string()),
                    ("previous_lookup_id", previous_lookup_id.to_string()),
                    ("lookup_id", resolution.lookup_id.clone()),
                ],
            ));
        }
        previous = Some(resolution.lookup_id.as_str());
    }
    Ok(())
}

fn validate_as_of_resolution_row(
    row: &GeoAsOfParcelResolution,
) -> Result<(), GeoAsOfResolutionError> {
    validate_as_of_string("resolutions[].lookup_id", &row.lookup_id)?;
    validate_bbl_key("resolutions[].bbl_key", &row.bbl_key)?;
    if let Some(parcel_cluster_id) = &row.parcel_cluster_id {
        validate_as_of_cluster_id(
            "resolutions[].parcel_cluster_id",
            parcel_cluster_id,
            GeoEntityLevel::Parcel,
        )?;
    }
    for event in &row.change_events {
        validate_as_of_string("change_events[].change_event_id", &event.change_event_id)?;
        validate_as_of_string("change_events[].event_type", &event.event_type)?;
        validate_as_of_string("change_events[].current_release", &event.current_release)?;
        validate_as_of_utc_day(
            "change_events[].current_release_dt",
            &event.current_release_dt,
        )?;
        validate_bbl_key("change_events[].subject_bbl_key", &event.subject_bbl_key)?;
        validate_bbl_list(
            "change_events[].predecessor_bbl_keys",
            &event.predecessor_bbl_keys,
        )?;
        validate_bbl_list(
            "change_events[].successor_bbl_keys",
            &event.successor_bbl_keys,
        )?;
    }
    match row.status {
        GeoAsOfParcelResolutionStatus::Resolved => {
            if row.reason != GeoAsOfParcelResolutionReason::ActiveAtAsOf
                || row.parcel_cluster_id.is_none()
                || row.matched_release.is_none()
                || row.matched_release_dt.is_none()
                || row.matched_valid_from_release_dt.is_none()
                || !row.change_events.is_empty()
            {
                return Err(GeoAsOfResolutionError::invalid(
                    "Geo as-of resolved rows must cite exactly one active source vintage and no change event",
                    [("lookup_id", row.lookup_id.clone())],
                ));
            }
        }
        GeoAsOfParcelResolutionStatus::Abstained => {
            if row.parcel_cluster_id.is_some()
                || row.matched_release.is_some()
                || row.matched_release_dt.is_some()
                || row.matched_valid_from_release_dt.is_some()
                || row.matched_valid_to_release_dt.is_some()
            {
                return Err(GeoAsOfResolutionError::invalid(
                    "Geo as-of abstentions must not fabricate a matched parcel vintage",
                    [("lookup_id", row.lookup_id.clone())],
                ));
            }
        }
    }
    Ok(())
}

fn validate_as_of_cluster_id(
    field: &'static str,
    value: &str,
    level: GeoEntityLevel,
) -> Result<(), GeoAsOfResolutionError> {
    validate_as_of_string(field, value)?;
    let prefix = as_of_cluster_prefix(level)?;
    if !value.starts_with(prefix) {
        return Err(GeoAsOfResolutionError::invalid(
            "Geo as-of cluster id does not match its entity level",
            [
                ("field", field.to_string()),
                ("value", value.to_string()),
                ("expected_prefix", prefix.to_string()),
            ],
        ));
    }
    Ok(())
}

fn as_of_cluster_prefix(level: GeoEntityLevel) -> Result<&'static str, GeoAsOfResolutionError> {
    match level {
        GeoEntityLevel::Parcel => Ok("cmdrvl:parcel:"),
        GeoEntityLevel::Building => Ok("cmdrvl:building:"),
        GeoEntityLevel::Property => Ok("cmdrvl:property:"),
        GeoEntityLevel::PoiUnit => Err(GeoAsOfResolutionError::invalid(
            "Geo as-of resolution does not support poi_unit identifiers",
            [("entity_level", "poi_unit")],
        )),
    }
}

fn validate_bbl_key(field: &'static str, value: &str) -> Result<(), GeoAsOfResolutionError> {
    if value.len() != 10 || !value.chars().all(|digit| digit.is_ascii_digit()) {
        return Err(as_of_invalid_field(
            field,
            "Geo as-of BBL keys must be ten ASCII digits",
            value,
        ));
    }
    Ok(())
}

fn validate_as_of_utc_day(field: &'static str, value: &str) -> Result<(), GeoAsOfResolutionError> {
    validate_utc_day(field, value)
        .map_err(|error| GeoAsOfResolutionError::invalid(error.message, error.detail.into_iter()))
}

fn validate_as_of_blake3_uri(
    field: &'static str,
    value: &str,
) -> Result<(), GeoAsOfResolutionError> {
    validate_blake3_uri(field, value)
        .map_err(|error| GeoAsOfResolutionError::invalid(error.message, error.detail.into_iter()))
}

fn validate_as_of_string(field: &'static str, value: &str) -> Result<(), GeoAsOfResolutionError> {
    validate_string(field, value)
        .map_err(|error| GeoAsOfResolutionError::invalid(error.message, error.detail.into_iter()))
}

fn as_of_invalid_field(
    field: &'static str,
    message: impl Into<String>,
    value: impl Into<String>,
) -> GeoAsOfResolutionError {
    GeoAsOfResolutionError::invalid(
        message,
        [("field", field.to_string()), ("value", value.into())],
    )
}

fn validate_temporal_containment_artifact_inner(
    artifact: &GeoTemporalContainmentArtifact,
    require_canonical_order: bool,
) -> Result<(), GeoLifecycleError> {
    if artifact.version != CANON_GEO_TEMPORAL_CONTAINMENT_VERSION {
        return Err(GeoLifecycleError::new(
            GeoLifecycleErrorCode::UnsupportedVersion,
            "Unsupported Geo temporal-containment artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_TEMPORAL_CONTAINMENT_VERSION),
            ],
        ));
    }
    validate_string("mart_id", &artifact.mart_id)?;
    if artifact.clusters.is_empty() {
        return Err(invalid_field(
            "clusters",
            "Geo temporal-containment artifacts must contain at least one cluster",
            "0",
        ));
    }
    validate_summary(artifact)?;

    let clusters = validate_clusters(&artifact.clusters, require_canonical_order)?;
    validate_edges(&artifact.edges, &clusters, require_canonical_order)
}

fn validate_summary(artifact: &GeoTemporalContainmentArtifact) -> Result<(), GeoLifecycleError> {
    let cluster_count = usize_to_u64(artifact.clusters.len(), "summary.clusters")?;
    if artifact.summary.clusters != cluster_count {
        return Err(GeoLifecycleError::invalid(
            "Geo temporal-containment cluster summary does not match clusters",
            [
                ("field", "summary.clusters".to_string()),
                ("actual", artifact.summary.clusters.to_string()),
                ("expected", cluster_count.to_string()),
            ],
        ));
    }
    let edge_count = usize_to_u64(artifact.edges.len(), "summary.edges")?;
    if artifact.summary.edges != edge_count {
        return Err(GeoLifecycleError::invalid(
            "Geo temporal-containment edge summary does not match edges",
            [
                ("field", "summary.edges".to_string()),
                ("actual", artifact.summary.edges.to_string()),
                ("expected", edge_count.to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_clusters(
    clusters: &[GeoTemporalContainmentCluster],
    require_canonical_order: bool,
) -> Result<BTreeMap<String, GeoEntityLevel>, GeoLifecycleError> {
    let mut out = BTreeMap::new();
    let mut previous_cluster_id: Option<&str> = None;
    for cluster in clusters {
        validate_cluster_id(
            "clusters[].cluster_id",
            &cluster.cluster_id,
            cluster.entity_level,
        )?;
        if out
            .insert(cluster.cluster_id.clone(), cluster.entity_level)
            .is_some()
        {
            return Err(GeoLifecycleError::invalid(
                "Geo temporal-containment clusters must be unique",
                [
                    ("field", "clusters[].cluster_id".to_string()),
                    ("cluster_id", cluster.cluster_id.clone()),
                ],
            ));
        }
        if require_canonical_order {
            if let Some(previous) = previous_cluster_id
                && previous >= cluster.cluster_id.as_str()
            {
                return Err(GeoLifecycleError::invalid(
                    "Geo temporal-containment clusters must be sorted by cluster_id",
                    [
                        ("field", "clusters".to_string()),
                        ("previous_cluster_id", previous.to_string()),
                        ("cluster_id", cluster.cluster_id.clone()),
                    ],
                ));
            }
            previous_cluster_id = Some(cluster.cluster_id.as_str());
        }
    }
    Ok(out)
}

fn validate_edges(
    edges: &[GeoTemporalContainmentEdge],
    clusters: &BTreeMap<String, GeoEntityLevel>,
    require_canonical_order: bool,
) -> Result<(), GeoLifecycleError> {
    if edges.is_empty() {
        return Err(invalid_field(
            "edges",
            "Geo temporal-containment artifacts must contain at least one edge",
            "0",
        ));
    }
    let mut edge_ids = BTreeSet::new();
    let mut semantic_edges = BTreeSet::new();
    let mut previous_key: Option<String> = None;
    for edge in edges {
        validate_edge(edge, clusters)?;
        if !edge_ids.insert(edge.edge_id.clone()) {
            return Err(GeoLifecycleError::invalid(
                "Geo temporal-containment edge ids must be unique",
                [
                    ("field", "edges[].edge_id".to_string()),
                    ("edge_id", edge.edge_id.clone()),
                ],
            ));
        }
        let semantic_key = edge_semantic_key(edge);
        if !semantic_edges.insert(semantic_key) {
            return Err(GeoLifecycleError::invalid(
                "Geo temporal-containment edges must be unique by relation and interval",
                [
                    ("field", "edges".to_string()),
                    ("parent_cluster_id", edge.parent_cluster_id.clone()),
                    ("child_cluster_id", edge.child_cluster_id.clone()),
                ],
            ));
        }
        if require_canonical_order {
            let key = edge_sort_key(edge);
            if let Some(previous) = &previous_key
                && previous >= &key
            {
                return Err(GeoLifecycleError::invalid(
                    "Geo temporal-containment edges must be in canonical order",
                    [
                        ("field", "edges".to_string()),
                        ("previous_key", previous.clone()),
                        ("edge_key", key.clone()),
                    ],
                ));
            }
            previous_key = Some(key);
        }
    }
    Ok(())
}

fn validate_edge(
    edge: &GeoTemporalContainmentEdge,
    clusters: &BTreeMap<String, GeoEntityLevel>,
) -> Result<(), GeoLifecycleError> {
    validate_string("edges[].edge_id", &edge.edge_id)?;
    validate_cluster_id(
        "edges[].parent_cluster_id",
        &edge.parent_cluster_id,
        edge.parent_level,
    )?;
    validate_cluster_id(
        "edges[].child_cluster_id",
        &edge.child_cluster_id,
        edge.child_level,
    )?;
    if edge.parent_cluster_id == edge.child_cluster_id {
        return Err(GeoLifecycleError::invalid(
            "Geo temporal-containment edge cannot contain itself",
            [("cluster_id", edge.parent_cluster_id.clone())],
        ));
    }
    validate_endpoint_level(
        "edges[].parent_cluster_id",
        &edge.parent_cluster_id,
        edge.parent_level,
        clusters,
    )?;
    validate_endpoint_level(
        "edges[].child_cluster_id",
        &edge.child_cluster_id,
        edge.child_level,
        clusters,
    )?;
    match edge.relation {
        GeoTemporalContainmentRelation::PartOf => {
            if edge.parent_level != GeoEntityLevel::Parcel
                || edge.child_level != GeoEntityLevel::Building
            {
                return Err(GeoLifecycleError::invalid(
                    "Geo temporal-containment v0 supports building part_of parcel edges only",
                    [
                        ("field", "edges[].relation".to_string()),
                        ("parent_level", format!("{:?}", edge.parent_level)),
                        ("child_level", format!("{:?}", edge.child_level)),
                    ],
                ));
            }
        }
    }
    validate_interval(&edge.valid_interval)?;
    validate_source_receipt(&edge.source_receipt)
}

fn validate_endpoint_level(
    field: &'static str,
    cluster_id: &str,
    level: GeoEntityLevel,
    clusters: &BTreeMap<String, GeoEntityLevel>,
) -> Result<(), GeoLifecycleError> {
    let Some(actual_level) = clusters.get(cluster_id) else {
        return Err(GeoLifecycleError::invalid(
            "Geo temporal-containment edge references an unknown cluster",
            [
                ("field", field.to_string()),
                ("cluster_id", cluster_id.to_string()),
            ],
        ));
    };
    if *actual_level != level {
        return Err(GeoLifecycleError::invalid(
            "Geo temporal-containment edge endpoint level does not match the cluster table",
            [
                ("field", field.to_string()),
                ("cluster_id", cluster_id.to_string()),
                ("expected_level", format!("{actual_level:?}")),
                ("actual_level", format!("{level:?}")),
            ],
        ));
    }
    Ok(())
}

fn validate_interval(interval: &GeoTemporalContainmentInterval) -> Result<(), GeoLifecycleError> {
    validate_utc_day("valid_interval.start_utc_day", &interval.start_utc_day)?;
    validate_utc_day("valid_interval.end_utc_day", &interval.end_utc_day)?;
    if interval.start_utc_day > interval.end_utc_day {
        return Err(GeoLifecycleError::invalid(
            "Geo temporal-containment interval start must not be after its end",
            [
                ("field", "valid_interval".to_string()),
                ("start_utc_day", interval.start_utc_day.clone()),
                ("end_utc_day", interval.end_utc_day.clone()),
            ],
        ));
    }
    Ok(())
}

fn validate_source_receipt(
    receipt: &GeoTemporalContainmentSourceReceipt,
) -> Result<(), GeoLifecycleError> {
    validate_string("source_receipt.receipt_id", &receipt.receipt_id)?;
    validate_string("source_receipt.source_dataset", &receipt.source_dataset)?;
    validate_string("source_receipt.source_record_id", &receipt.source_record_id)?;
    validate_blake3_uri(
        "source_receipt.source_record_blake3",
        &receipt.source_record_blake3,
    )?;
    validate_string("source_receipt.proof_class", &receipt.proof_class)?;
    validate_string("source_receipt.rule_id", &receipt.rule_id)
}

fn containment_summary(
    edges: &[GeoTemporalContainmentEdge],
) -> Result<GeoContainmentAsOfSummary, GeoLifecycleError> {
    let parent_clusters = edges
        .iter()
        .map(|edge| edge.parent_cluster_id.clone())
        .collect::<BTreeSet<_>>();
    let child_clusters = edges
        .iter()
        .map(|edge| edge.child_cluster_id.clone())
        .collect::<BTreeSet<_>>();
    Ok(GeoContainmentAsOfSummary {
        edges: usize_to_u64(edges.len(), "summary.edges")?,
        parent_clusters: usize_to_u64(parent_clusters.len(), "summary.parent_clusters")?,
        child_clusters: usize_to_u64(child_clusters.len(), "summary.child_clusters")?,
    })
}

fn edge_sort_order(
    left: &GeoTemporalContainmentEdge,
    right: &GeoTemporalContainmentEdge,
) -> std::cmp::Ordering {
    edge_sort_key(left).cmp(&edge_sort_key(right))
}

fn edge_sort_key(edge: &GeoTemporalContainmentEdge) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        edge.parent_cluster_id,
        edge.child_cluster_id,
        edge.valid_interval.start_utc_day,
        edge.valid_interval.end_utc_day,
        edge.edge_id
    )
}

fn edge_semantic_key(edge: &GeoTemporalContainmentEdge) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{:?}\u{1f}{}\u{1f}{}",
        edge.parent_cluster_id,
        edge.child_cluster_id,
        edge.relation,
        edge.valid_interval.start_utc_day,
        edge.valid_interval.end_utc_day
    )
}

fn validate_cluster_id(
    field: &'static str,
    value: &str,
    level: GeoEntityLevel,
) -> Result<(), GeoLifecycleError> {
    validate_cluster_identifier(field, value)?;
    let prefix = cluster_prefix(level)?;
    if !value.starts_with(prefix) {
        return Err(GeoLifecycleError::invalid(
            "Geo temporal-containment cluster id does not match its entity level",
            [
                ("field", field.to_string()),
                ("value", value.to_string()),
                ("expected_prefix", prefix.to_string()),
            ],
        ));
    }
    Ok(())
}

fn validate_cluster_identifier(field: &'static str, value: &str) -> Result<(), GeoLifecycleError> {
    validate_string(field, value)?;
    if !value.starts_with("cmdrvl:") {
        return Err(GeoLifecycleError::invalid(
            "Geo temporal-containment cluster ids must be opaque cmdrvl identifiers",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn cluster_prefix(level: GeoEntityLevel) -> Result<&'static str, GeoLifecycleError> {
    match level {
        GeoEntityLevel::Parcel => Ok("cmdrvl:parcel:"),
        GeoEntityLevel::Building => Ok("cmdrvl:building:"),
        GeoEntityLevel::Property => Ok("cmdrvl:property:"),
        GeoEntityLevel::PoiUnit => Err(GeoLifecycleError::invalid(
            "Geo temporal-containment v0 does not support poi_unit containment",
            [("entity_level", "poi_unit")],
        )),
    }
}

fn validate_utc_day(field: &'static str, value: &str) -> Result<(), GeoLifecycleError> {
    if value.len() != 10 {
        return Err(invalid_field(
            field,
            "Geo temporal-containment dates must be whole UTC days formatted as YYYY-MM-DD",
            value,
        ));
    }
    let bytes = value.as_bytes();
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..10].iter().all(u8::is_ascii_digit)
    {
        return Err(invalid_field(
            field,
            "Geo temporal-containment dates must be whole UTC days formatted as YYYY-MM-DD",
            value,
        ));
    }
    let year = value[0..4].parse::<u16>().map_err(|error| {
        invalid_field(
            field,
            "Geo temporal-containment date year is invalid",
            error.to_string(),
        )
    })?;
    let month = value[5..7].parse::<u8>().map_err(|error| {
        invalid_field(
            field,
            "Geo temporal-containment date month is invalid",
            error.to_string(),
        )
    })?;
    let day = value[8..10].parse::<u8>().map_err(|error| {
        invalid_field(
            field,
            "Geo temporal-containment date day is invalid",
            error.to_string(),
        )
    })?;
    if month == 0 || month > 12 {
        return Err(invalid_field(
            field,
            "Geo temporal-containment date month is out of range",
            value,
        ));
    }
    let max_day = days_in_month(year, month);
    if day == 0 || day > max_day {
        return Err(invalid_field(
            field,
            "Geo temporal-containment date day is out of range",
            value,
        ));
    }
    Ok(())
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn validate_blake3_uri(field: &'static str, value: &str) -> Result<(), GeoLifecycleError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(invalid_field(
            field,
            "Geo temporal-containment source digest must be a blake3 URI",
            value,
        ));
    };
    if hex.len() != 64 || !hex.chars().all(|digit| digit.is_ascii_hexdigit()) {
        return Err(invalid_field(
            field,
            "Geo temporal-containment source digest must be a blake3 URI",
            value,
        ));
    }
    Ok(())
}

fn validate_string(field: &'static str, value: &str) -> Result<(), GeoLifecycleError> {
    if value.is_empty() || value.trim_matches(|ch: char| ch.is_ascii_whitespace()) != value {
        return Err(invalid_field(
            field,
            "Geo temporal-containment strings must be non-empty and ASCII-trimmed",
            value,
        ));
    }
    Ok(())
}

fn invalid_field(
    field: &'static str,
    message: impl Into<String>,
    value: impl Into<String>,
) -> GeoLifecycleError {
    GeoLifecycleError::invalid(
        message,
        [("field", field.to_string()), ("value", value.into())],
    )
}

fn usize_to_u64(value: usize, field: &'static str) -> Result<u64, GeoLifecycleError> {
    u64::try_from(value).map_err(|_| {
        GeoLifecycleError::new(
            GeoLifecycleErrorCode::ArithmeticOverflow,
            "Geo temporal-containment count does not fit in u64",
            [("field", field)],
        )
    })
}
