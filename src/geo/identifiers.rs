#![forbid(unsafe_code)]

//! Geo identifier scheme helpers.
//!
//! The registry remains flat exact replay. This module only prepares
//! workbench-side alias bindings, tile-vintage stability checks, and
//! property-set comparisons before reviewed promotion.

use super::GeoEntityLevel;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const GEO_BBL_NORMALIZATION_RULE_ID: &str = "bbl_normalization.v1";
pub const GEO_DIRECT_ALIAS_RULE_ID: &str = "geo_direct_alias.v1";
pub const GEO_PROPERTY_ASSERTION_RULE_ID: &str = "geo_property_document_assertion.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoIdentifierError {
    pub code: GeoIdentifierErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoIdentifierErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
}

impl GeoIdentifierError {
    pub fn invalid_input(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (&'static str, String)>,
    ) -> Self {
        Self {
            code: GeoIdentifierErrorCode::InvalidInput,
            message: message.into(),
            detail: detail
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
        }
    }
}

impl fmt::Display for GeoIdentifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeoIdentifierError {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoBblNormalization {
    pub rule_id: String,
    pub input: String,
    pub normalized: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoRegistryProposalEntry {
    pub alias: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoIdentifierCluster {
    pub cluster_id: String,
    pub entity_level: GeoEntityLevel,
    pub geometry_blake3: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoIdentifierTombstone {
    pub cluster_id: String,
    pub geometry_blake3: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoTileIdentifierVintage {
    pub tile_id: String,
    pub vintage_id: String,
    pub clusters: Vec<GeoIdentifierCluster>,
    #[serde(default)]
    pub tombstones: Vec<GeoIdentifierTombstone>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoTileIdentifierDiff {
    pub retained_cluster_ids: Vec<String>,
    pub added_cluster_ids: Vec<String>,
    pub tombstoned_cluster_ids: Vec<String>,
    pub merged_prior_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPropertyDocumentAssertion {
    pub property_id: String,
    pub document_alias: String,
    pub accession: String,
    pub loan_id: String,
    pub parcel_ids: Vec<String>,
    pub building_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPropertySetRelation {
    SameCollateral,
    LeftSuperset,
    LeftSubset,
    Intersects,
    Disjoint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPropertySetComparison {
    pub left_property_id: String,
    pub right_property_id: String,
    pub relation: GeoPropertySetRelation,
    pub shared_parcel_ids: Vec<String>,
    pub shared_building_ids: Vec<String>,
    pub left_only_parcel_ids: Vec<String>,
    pub left_only_building_ids: Vec<String>,
    pub right_only_parcel_ids: Vec<String>,
    pub right_only_building_ids: Vec<String>,
}

pub fn normalize_nyc_bbl(raw: &str) -> Result<GeoBblNormalization, GeoIdentifierError> {
    let input = raw.trim_matches(|ch: char| ch.is_ascii_whitespace());
    let normalized = if let Some((prefix, suffix)) = input.split_once('.') {
        if !suffix.is_empty() && suffix.chars().all(|digit| digit == '0') {
            prefix
        } else {
            return Err(GeoIdentifierError::invalid_input(
                "BBL decimal suffix is not the declared zero warehouse projection suffix",
                [
                    ("input", input.to_string()),
                    ("rule_id", GEO_BBL_NORMALIZATION_RULE_ID.to_string()),
                ],
            ));
        }
    } else {
        input
    };
    if normalized.len() != 10 || !normalized.chars().all(|digit| digit.is_ascii_digit()) {
        return Err(GeoIdentifierError::invalid_input(
            "BBL must normalize to ten ASCII digits",
            [
                ("input", input.to_string()),
                ("rule_id", GEO_BBL_NORMALIZATION_RULE_ID.to_string()),
            ],
        ));
    }
    Ok(GeoBblNormalization {
        rule_id: GEO_BBL_NORMALIZATION_RULE_ID.to_string(),
        input: input.to_string(),
        normalized: normalized.to_string(),
    })
}

pub fn registry_entries_for_clusters(
    clusters: &[GeoIdentifierCluster],
) -> Result<Vec<GeoRegistryProposalEntry>, GeoIdentifierError> {
    let mut entries = BTreeMap::<String, GeoRegistryProposalEntry>::new();
    for cluster in clusters {
        validate_cluster(cluster)?;
        for alias in &cluster.aliases {
            validate_identifier("aliases[]", alias)?;
            let entry = GeoRegistryProposalEntry {
                alias: alias.clone(),
                canonical_id: cluster.cluster_id.clone(),
                canonical_type: canonical_type(cluster.entity_level).to_string(),
                rule_id: GEO_DIRECT_ALIAS_RULE_ID.to_string(),
            };
            if entries.insert(alias.clone(), entry).is_some() {
                return Err(GeoIdentifierError::invalid_input(
                    "Geo registry proposal contains a duplicate exact alias",
                    [("alias", alias.clone())],
                ));
            }
        }
    }
    Ok(entries.into_values().collect())
}

pub fn diff_tile_identifier_vintages(
    before: &GeoTileIdentifierVintage,
    after: &GeoTileIdentifierVintage,
) -> Result<GeoTileIdentifierDiff, GeoIdentifierError> {
    validate_identifier("before.tile_id", &before.tile_id)?;
    validate_identifier("after.tile_id", &after.tile_id)?;
    if before.tile_id != after.tile_id {
        return Err(GeoIdentifierError::invalid_input(
            "Tile identifier vintages must describe the same tile",
            [
                ("before_tile_id", before.tile_id.clone()),
                ("after_tile_id", after.tile_id.clone()),
            ],
        ));
    }

    let before_clusters = cluster_map("before.clusters", &before.clusters)?;
    let after_clusters = cluster_map("after.clusters", &after.clusters)?;
    let tombstones = tombstone_map(&before_clusters, &after.tombstones)?;
    let retained_aliases = retained_prior_aliases(&after.clusters);

    let mut retained_cluster_ids = Vec::new();
    let mut added_cluster_ids = Vec::new();
    let mut tombstoned_cluster_ids = Vec::new();
    let mut merged_prior_ids = Vec::new();

    for (cluster_id, before_cluster) in &before_clusters {
        if let Some(after_cluster) = after_clusters.get(cluster_id) {
            if before_cluster.geometry_blake3 != after_cluster.geometry_blake3 {
                return Err(GeoIdentifierError::invalid_input(
                    "Geo tile refresh reassigned an existing minted id to different geometry",
                    [
                        ("cluster_id", cluster_id.clone()),
                        (
                            "geometry_blake3_before",
                            before_cluster.geometry_blake3.clone(),
                        ),
                        (
                            "geometry_blake3_after",
                            after_cluster.geometry_blake3.clone(),
                        ),
                    ],
                ));
            }
            retained_cluster_ids.push(cluster_id.clone());
        } else if tombstones.contains_key(cluster_id) {
            tombstoned_cluster_ids.push(cluster_id.clone());
        } else if retained_aliases.contains(cluster_id) {
            merged_prior_ids.push(cluster_id.clone());
        } else {
            return Err(GeoIdentifierError::invalid_input(
                "Geo tile refresh dropped a prior minted id without a tombstone or retained alias",
                [
                    ("cluster_id", cluster_id.clone()),
                    ("field", "tombstones_or_aliases".to_string()),
                ],
            ));
        }
    }

    for cluster_id in after_clusters.keys() {
        if !before_clusters.contains_key(cluster_id) {
            added_cluster_ids.push(cluster_id.clone());
        }
    }

    Ok(GeoTileIdentifierDiff {
        retained_cluster_ids,
        added_cluster_ids,
        tombstoned_cluster_ids,
        merged_prior_ids,
    })
}

pub fn compare_property_sets(
    left: &GeoPropertyDocumentAssertion,
    right: &GeoPropertyDocumentAssertion,
) -> Result<GeoPropertySetComparison, GeoIdentifierError> {
    validate_property_assertion("left", left)?;
    validate_property_assertion("right", right)?;

    let left_parcels = sorted_unique("left.parcel_ids", &left.parcel_ids)?;
    let left_buildings = sorted_unique("left.building_ids", &left.building_ids)?;
    let right_parcels = sorted_unique("right.parcel_ids", &right.parcel_ids)?;
    let right_buildings = sorted_unique("right.building_ids", &right.building_ids)?;
    let left_members = typed_members(&left_parcels, &left_buildings);
    let right_members = typed_members(&right_parcels, &right_buildings);
    let intersection = left_members
        .intersection(&right_members)
        .cloned()
        .collect::<BTreeSet<_>>();

    let relation = if left_members == right_members {
        GeoPropertySetRelation::SameCollateral
    } else if right_members.is_subset(&left_members) {
        GeoPropertySetRelation::LeftSuperset
    } else if left_members.is_subset(&right_members) {
        GeoPropertySetRelation::LeftSubset
    } else if !intersection.is_empty() {
        GeoPropertySetRelation::Intersects
    } else {
        GeoPropertySetRelation::Disjoint
    };

    Ok(GeoPropertySetComparison {
        left_property_id: left.property_id.clone(),
        right_property_id: right.property_id.clone(),
        relation,
        shared_parcel_ids: intersection_ids(&left_parcels, &right_parcels),
        shared_building_ids: intersection_ids(&left_buildings, &right_buildings),
        left_only_parcel_ids: difference_ids(&left_parcels, &right_parcels),
        left_only_building_ids: difference_ids(&left_buildings, &right_buildings),
        right_only_parcel_ids: difference_ids(&right_parcels, &left_parcels),
        right_only_building_ids: difference_ids(&right_buildings, &left_buildings),
    })
}

fn cluster_map<'a>(
    field: &'static str,
    clusters: &'a [GeoIdentifierCluster],
) -> Result<BTreeMap<String, &'a GeoIdentifierCluster>, GeoIdentifierError> {
    let mut by_id = BTreeMap::new();
    for cluster in clusters {
        validate_cluster(cluster)?;
        if by_id.insert(cluster.cluster_id.clone(), cluster).is_some() {
            return Err(GeoIdentifierError::invalid_input(
                "Geo tile vintage contains a duplicate cluster id",
                [
                    ("field", field.to_string()),
                    ("cluster_id", cluster.cluster_id.clone()),
                ],
            ));
        }
    }
    Ok(by_id)
}

fn tombstone_map<'a>(
    before_clusters: &BTreeMap<String, &'a GeoIdentifierCluster>,
    tombstones: &'a [GeoIdentifierTombstone],
) -> Result<BTreeMap<String, &'a GeoIdentifierTombstone>, GeoIdentifierError> {
    let mut by_id = BTreeMap::new();
    for tombstone in tombstones {
        validate_cluster_id("tombstones[].cluster_id", &tombstone.cluster_id)?;
        validate_blake3("tombstones[].geometry_blake3", &tombstone.geometry_blake3)?;
        validate_identifier("tombstones[].reason", &tombstone.reason)?;
        if !before_clusters.contains_key(&tombstone.cluster_id) {
            return Err(GeoIdentifierError::invalid_input(
                "Geo tile tombstone names a cluster absent from the prior vintage",
                [("cluster_id", tombstone.cluster_id.clone())],
            ));
        }
        if by_id
            .insert(tombstone.cluster_id.clone(), tombstone)
            .is_some()
        {
            return Err(GeoIdentifierError::invalid_input(
                "Geo tile vintage contains a duplicate tombstone",
                [("cluster_id", tombstone.cluster_id.clone())],
            ));
        }
    }
    Ok(by_id)
}

fn retained_prior_aliases(clusters: &[GeoIdentifierCluster]) -> BTreeSet<String> {
    clusters
        .iter()
        .flat_map(|cluster| cluster.aliases.iter())
        .filter(|alias| alias.starts_with("cmdrvl:"))
        .cloned()
        .collect()
}

fn validate_cluster(cluster: &GeoIdentifierCluster) -> Result<(), GeoIdentifierError> {
    validate_cluster_id("cluster_id", &cluster.cluster_id)?;
    validate_blake3("geometry_blake3", &cluster.geometry_blake3)?;
    let expected_prefix = cluster_prefix(cluster.entity_level)?;
    if !cluster.cluster_id.starts_with(expected_prefix) {
        return Err(GeoIdentifierError::invalid_input(
            "Geo minted id prefix does not match its entity level",
            [
                ("cluster_id", cluster.cluster_id.clone()),
                ("expected_prefix", expected_prefix.to_string()),
            ],
        ));
    }
    for alias in &cluster.aliases {
        validate_identifier("aliases[]", alias)?;
    }
    Ok(())
}

fn validate_property_assertion(
    side: &'static str,
    assertion: &GeoPropertyDocumentAssertion,
) -> Result<(), GeoIdentifierError> {
    validate_property_id(&format!("{side}.property_id"), &assertion.property_id)?;
    validate_identifier("document_alias", &assertion.document_alias)?;
    validate_identifier("accession", &assertion.accession)?;
    validate_identifier("loan_id", &assertion.loan_id)?;
    if !assertion.document_alias.starts_with("cmbs:") {
        return Err(GeoIdentifierError::invalid_input(
            "Geo property document assertion alias must be role-namespaced",
            [("document_alias", assertion.document_alias.clone())],
        ));
    }
    let parcels = sorted_unique("parcel_ids", &assertion.parcel_ids)?;
    let buildings = sorted_unique("building_ids", &assertion.building_ids)?;
    if parcels.is_empty() && buildings.is_empty() {
        return Err(GeoIdentifierError::invalid_input(
            "Geo property document assertion must contain at least one stable member",
            [("property_id", assertion.property_id.clone())],
        ));
    }
    for parcel_id in &parcels {
        if !parcel_id.starts_with("cmdrvl:parcel:") {
            return Err(GeoIdentifierError::invalid_input(
                "Geo property parcel members must be stable parcel cluster ids",
                [("parcel_id", parcel_id.clone())],
            ));
        }
    }
    for building_id in &buildings {
        if !building_id.starts_with("cmdrvl:building:") {
            return Err(GeoIdentifierError::invalid_input(
                "Geo property building members must be stable building cluster ids",
                [("building_id", building_id.clone())],
            ));
        }
    }
    Ok(())
}

fn typed_members(
    parcels: &BTreeSet<String>,
    buildings: &BTreeSet<String>,
) -> BTreeSet<(GeoEntityLevel, String)> {
    parcels
        .iter()
        .cloned()
        .map(|id| (GeoEntityLevel::Parcel, id))
        .chain(
            buildings
                .iter()
                .cloned()
                .map(|id| (GeoEntityLevel::Building, id)),
        )
        .collect()
}

fn intersection_ids(left: &BTreeSet<String>, right: &BTreeSet<String>) -> Vec<String> {
    left.intersection(right).cloned().collect()
}

fn difference_ids(left: &BTreeSet<String>, right: &BTreeSet<String>) -> Vec<String> {
    left.difference(right).cloned().collect()
}

fn sorted_unique(
    field: &'static str,
    values: &[String],
) -> Result<BTreeSet<String>, GeoIdentifierError> {
    let mut out = BTreeSet::new();
    for value in values {
        validate_identifier(field, value)?;
        if !out.insert(value.clone()) {
            return Err(GeoIdentifierError::invalid_input(
                "Geo identifier set contains a duplicate member",
                [("field", field.to_string()), ("value", value.clone())],
            ));
        }
    }
    Ok(out)
}

fn validate_cluster_id(field: &'static str, value: &str) -> Result<(), GeoIdentifierError> {
    validate_identifier(field, value)?;
    if !value.starts_with("cmdrvl:") {
        return Err(GeoIdentifierError::invalid_input(
            "Geo minted ids must be opaque cmdrvl identifiers",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn validate_property_id(field: &str, value: &str) -> Result<(), GeoIdentifierError> {
    validate_identifier("property_id", value)?;
    if !value.starts_with("cmdrvl:property:") {
        return Err(GeoIdentifierError::invalid_input(
            "Geo property id must use the property cluster namespace",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn validate_blake3(field: &'static str, value: &str) -> Result<(), GeoIdentifierError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(GeoIdentifierError::invalid_input(
            "Geo geometry digest must be a blake3 hex digest",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    };
    if hex.len() != 64 || !hex.chars().all(|digit| digit.is_ascii_hexdigit()) {
        return Err(GeoIdentifierError::invalid_input(
            "Geo geometry digest must be a blake3 hex digest",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn validate_identifier(field: &str, value: &str) -> Result<(), GeoIdentifierError> {
    if value.trim_matches(|ch: char| ch.is_ascii_whitespace()) != value || value.is_empty() {
        return Err(GeoIdentifierError::invalid_input(
            "Geo identifier must be non-empty and ASCII-trimmed",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn cluster_prefix(level: GeoEntityLevel) -> Result<&'static str, GeoIdentifierError> {
    match level {
        GeoEntityLevel::Parcel => Ok("cmdrvl:parcel:"),
        GeoEntityLevel::Building => Ok("cmdrvl:building:"),
        GeoEntityLevel::Property => Ok("cmdrvl:property:"),
        GeoEntityLevel::PoiUnit => Err(GeoIdentifierError::invalid_input(
            "Geo identifier minting is not defined for poi_unit in this scheme",
            [("entity_level", "poi_unit".to_string())],
        )),
    }
}

fn canonical_type(level: GeoEntityLevel) -> &'static str {
    match level {
        GeoEntityLevel::Parcel => "parcel",
        GeoEntityLevel::Building => "building",
        GeoEntityLevel::Property => "property",
        GeoEntityLevel::PoiUnit => "poi_unit",
    }
}
