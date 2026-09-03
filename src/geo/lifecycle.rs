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
