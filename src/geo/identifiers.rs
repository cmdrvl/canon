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
pub const CANON_GEO_REGISTRY_PROPOSAL_VERSION: &str = "canon_geo_registry_proposal.v0";

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoRegistryMintProposal {
    pub version: String,
    pub source_ledger_blake3: String,
    pub entries: Vec<GeoRegistryProposalEntry>,
    pub property_assertions: Vec<GeoPropertyDocumentAssertion>,
    pub summary: GeoRegistryMintProposalSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoRegistryMintProposalSummary {
    pub ledger_rows: u64,
    pub skipped_reach_none_rows: u64,
    pub unique_parcel_aliases: u64,
    pub unique_building_aliases: u64,
    pub property_assertions: u64,
    pub entries: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoLedgerIdentifierArtifact {
    #[serde(default)]
    pub rows: Vec<GeoLedgerIdentifierRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoLedgerIdentifierRow {
    pub accession: String,
    pub deal_id: String,
    pub loan_id: String,
    #[serde(default)]
    pub reach: Option<String>,
    #[serde(default)]
    pub reach_none_reason: Option<String>,
    #[serde(default)]
    pub parcel_set: Option<Vec<String>>,
    #[serde(default)]
    pub building_set: Option<Vec<String>>,
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

pub fn registry_proposal_from_ledger_json(
    ledger_json: &[u8],
) -> Result<GeoRegistryMintProposal, GeoIdentifierError> {
    let artifact =
        serde_json::from_slice::<GeoLedgerIdentifierArtifact>(ledger_json).map_err(|error| {
            GeoIdentifierError::invalid_input(
                "Geo ledger identifier proposal input must be a JSON object with rows",
                [("serde_error", error.to_string())],
            )
        })?;
    registry_proposal_from_ledger_rows(ledger_json, &artifact.rows)
}

pub fn registry_proposal_from_ledger_rows(
    source_ledger_bytes: &[u8],
    rows: &[GeoLedgerIdentifierRow],
) -> Result<GeoRegistryMintProposal, GeoIdentifierError> {
    let mut entries = BTreeMap::<String, GeoRegistryProposalEntry>::new();
    let mut property_assertions = Vec::new();
    let mut document_assertions = BTreeSet::<(String, String)>::new();
    let mut skipped_reach_none_rows = 0_u64;
    let mut parcel_aliases = BTreeSet::<String>::new();
    let mut building_aliases = BTreeSet::<String>::new();

    for row in rows {
        validate_ledger_identifier_row(row)?;

        let reach_none = row
            .reach
            .as_deref()
            .is_some_and(|reach| reach.eq_ignore_ascii_case("none"));
        let parcel_set = row.parcel_set.as_deref().unwrap_or(&[]);
        let building_set = row.building_set.as_deref().unwrap_or(&[]);

        if reach_none {
            if row.reach_none_reason.as_deref().is_none_or(str::is_empty) {
                return Err(GeoIdentifierError::invalid_input(
                    "Geo reach-none ledger rows must carry a reason before proposal",
                    [
                        ("field", "reach_none_reason".to_string()),
                        ("loan_id", row.loan_id.clone()),
                    ],
                ));
            }
            if !parcel_set.is_empty() || !building_set.is_empty() {
                return Err(GeoIdentifierError::invalid_input(
                    "Geo reach-none ledger rows must not fabricate identifier sets",
                    [
                        ("field", "parcel_set_or_building_set".to_string()),
                        ("loan_id", row.loan_id.clone()),
                    ],
                ));
            }
            skipped_reach_none_rows += 1;
            continue;
        }

        if parcel_set.is_empty() && building_set.is_empty() {
            return Err(GeoIdentifierError::invalid_input(
                "Geo ledger rows need at least one stable member unless reach is none",
                [("loan_id", row.loan_id.clone())],
            ));
        }

        let mut property_parcel_ids = Vec::new();
        let mut property_building_ids = Vec::new();
        let parcel_set = sorted_ledger_aliases("parcel_set", GeoEntityLevel::Parcel, parcel_set)?;
        let building_set =
            sorted_ledger_aliases("building_set", GeoEntityLevel::Building, building_set)?;

        for alias in &parcel_set {
            let canonical_id = canonical_id_for_ledger_alias(GeoEntityLevel::Parcel, alias)?;
            insert_proposal_entry(
                &mut entries,
                alias,
                &canonical_id,
                "parcel",
                GEO_DIRECT_ALIAS_RULE_ID,
            )?;
            parcel_aliases.insert(alias.clone());
            property_parcel_ids.push(canonical_id);
        }
        for alias in &building_set {
            let canonical_id = canonical_id_for_ledger_alias(GeoEntityLevel::Building, alias)?;
            insert_proposal_entry(
                &mut entries,
                alias,
                &canonical_id,
                "building",
                GEO_DIRECT_ALIAS_RULE_ID,
            )?;
            building_aliases.insert(alias.clone());
            property_building_ids.push(canonical_id);
        }

        let document_key = (row.accession.clone(), row.loan_id.clone());
        if !document_assertions.insert(document_key) {
            return Err(GeoIdentifierError::invalid_input(
                "Geo registry proposal contains a duplicate property document assertion",
                [
                    ("accession", row.accession.clone()),
                    ("loan_id", row.loan_id.clone()),
                ],
            ));
        }

        property_parcel_ids.sort();
        property_building_ids.sort();
        let property_id = property_id_for_document_assertion(&row.accession, &row.loan_id);
        let document_alias = format!("cmbs:annexa:{}:{}", row.accession, row.loan_id);
        insert_proposal_entry(
            &mut entries,
            &document_alias,
            &property_id,
            "property",
            GEO_PROPERTY_ASSERTION_RULE_ID,
        )?;
        property_assertions.push(GeoPropertyDocumentAssertion {
            property_id,
            document_alias,
            accession: row.accession.clone(),
            loan_id: row.loan_id.clone(),
            parcel_ids: property_parcel_ids,
            building_ids: property_building_ids,
        });
    }

    let entries = entries.into_values().collect::<Vec<_>>();
    let summary = GeoRegistryMintProposalSummary {
        ledger_rows: rows.len() as u64,
        skipped_reach_none_rows,
        unique_parcel_aliases: parcel_aliases.len() as u64,
        unique_building_aliases: building_aliases.len() as u64,
        property_assertions: property_assertions.len() as u64,
        entries: entries.len() as u64,
    };
    Ok(GeoRegistryMintProposal {
        version: CANON_GEO_REGISTRY_PROPOSAL_VERSION.to_string(),
        source_ledger_blake3: format!("blake3:{}", blake3::hash(source_ledger_bytes).to_hex()),
        entries,
        property_assertions,
        summary,
    })
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

fn validate_ledger_identifier_row(row: &GeoLedgerIdentifierRow) -> Result<(), GeoIdentifierError> {
    validate_identifier("accession", &row.accession)?;
    validate_identifier("deal_id", &row.deal_id)?;
    validate_identifier("loan_id", &row.loan_id)?;
    if let Some(reach) = &row.reach {
        validate_identifier("reach", reach)?;
    }
    if let Some(reason) = &row.reach_none_reason {
        validate_identifier("reach_none_reason", reason)?;
    }
    Ok(())
}

fn sorted_ledger_aliases(
    field: &'static str,
    level: GeoEntityLevel,
    aliases: &[String],
) -> Result<BTreeSet<String>, GeoIdentifierError> {
    let mut out = BTreeSet::new();
    for alias in aliases {
        validate_ledger_alias(level, alias)?;
        if !out.insert(alias.clone()) {
            return Err(GeoIdentifierError::invalid_input(
                "Geo ledger identifier set contains a duplicate alias",
                [("field", field.to_string()), ("alias", alias.clone())],
            ));
        }
    }
    Ok(out)
}

fn canonical_id_for_ledger_alias(
    level: GeoEntityLevel,
    alias: &str,
) -> Result<String, GeoIdentifierError> {
    validate_ledger_alias(level, alias)?;
    if alias.starts_with(cluster_prefix(level)?) {
        return Ok(alias.to_string());
    }
    let digest_input = format!("{CANON_GEO_REGISTRY_PROPOSAL_VERSION}:{level:?}:{alias}");
    Ok(format!(
        "{}{}",
        cluster_prefix(level)?,
        blake3::hash(digest_input.as_bytes()).to_hex()
    ))
}

fn property_id_for_document_assertion(accession: &str, loan_id: &str) -> String {
    let digest_input =
        format!("{CANON_GEO_REGISTRY_PROPOSAL_VERSION}:property:{accession}:{loan_id}");
    format!(
        "cmdrvl:property:{}",
        blake3::hash(digest_input.as_bytes()).to_hex()
    )
}

fn insert_proposal_entry(
    entries: &mut BTreeMap<String, GeoRegistryProposalEntry>,
    alias: &str,
    canonical_id: &str,
    canonical_type: &str,
    rule_id: &str,
) -> Result<(), GeoIdentifierError> {
    let entry = GeoRegistryProposalEntry {
        alias: alias.to_string(),
        canonical_id: canonical_id.to_string(),
        canonical_type: canonical_type.to_string(),
        rule_id: rule_id.to_string(),
    };
    match entries.get(alias) {
        Some(existing)
            if existing.canonical_id == entry.canonical_id
                && existing.canonical_type == entry.canonical_type
                && existing.rule_id == entry.rule_id =>
        {
            Ok(())
        }
        Some(existing) => Err(GeoIdentifierError::invalid_input(
            "Geo registry proposal contains an alias with conflicting bindings",
            [
                ("alias", alias.to_string()),
                ("canonical_id_before", existing.canonical_id.clone()),
                ("canonical_id_after", entry.canonical_id),
            ],
        )),
        None => {
            entries.insert(alias.to_string(), entry);
            Ok(())
        }
    }
}

fn validate_ledger_alias(level: GeoEntityLevel, alias: &str) -> Result<(), GeoIdentifierError> {
    validate_identifier("ledger_alias", alias)?;
    if !alias.contains(':') {
        return Err(GeoIdentifierError::invalid_input(
            "Geo ledger aliases must be role-namespaced",
            [("alias", alias.to_string())],
        ));
    }
    if alias.starts_with("cmdrvl:") && !alias.starts_with(cluster_prefix(level)?) {
        return Err(GeoIdentifierError::invalid_input(
            "Geo ledger alias entity level conflicts with its set field",
            [
                ("field", ledger_set_field(level).to_string()),
                ("alias", alias.to_string()),
            ],
        ));
    }
    let wrong_role = match level {
        GeoEntityLevel::Parcel => alias.starts_with("building:") || alias.contains(":building:"),
        GeoEntityLevel::Building => alias.starts_with("parcel:") || alias.contains(":parcel:"),
        GeoEntityLevel::Property | GeoEntityLevel::PoiUnit => false,
    };
    if wrong_role {
        return Err(GeoIdentifierError::invalid_input(
            "Geo ledger alias entity level conflicts with its set field",
            [
                ("field", ledger_set_field(level).to_string()),
                ("alias", alias.to_string()),
            ],
        ));
    }
    if alias.starts_with("cmdrvl:property:") {
        return Err(GeoIdentifierError::invalid_input(
            "Geo ledger parcel/building sets must not contain property ids",
            [
                ("field", ledger_set_field(level).to_string()),
                ("alias", alias.to_string()),
            ],
        ));
    }
    Ok(())
}

fn ledger_set_field(level: GeoEntityLevel) -> &'static str {
    match level {
        GeoEntityLevel::Parcel => "parcel_set",
        GeoEntityLevel::Building => "building_set",
        GeoEntityLevel::Property => "property_set",
        GeoEntityLevel::PoiUnit => "poi_unit_set",
    }
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
