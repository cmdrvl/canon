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
pub const CANON_GEO_TILE_IDENTIFIER_STABILITY_REQUEST_VERSION: &str =
    "canon_geo_tile_identifier_stability_request.v0";
pub const CANON_GEO_TILE_IDENTIFIER_STABILITY_VERSION: &str =
    "canon_geo_tile_identifier_stability.v0";

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub successor_cluster_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub survivor_cluster_id: Option<String>,
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
#[serde(deny_unknown_fields)]
pub struct GeoTileIdentifierStabilityRequest {
    pub version: String,
    pub before: GeoTileIdentifierVintage,
    pub after: GeoTileIdentifierVintage,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub client_aliases: Vec<GeoClientTileAliasBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoClientTileAliasBinding {
    pub client_alias: String,
    pub cluster_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoTileIdentifierContractDisposition {
    May,
    Never,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileIdentifierContractRule {
    pub rule_id: String,
    pub disposition: GeoTileIdentifierContractDisposition,
    pub statement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileIdentifierStabilityContract {
    pub rules: Vec<GeoTileIdentifierContractRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoClientTileAliasImpactStatus {
    Active,
    Tombstoned,
    MergedToSurvivor,
    UnknownCluster,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoClientTileAliasImpact {
    pub client_alias: String,
    pub cluster_id: String,
    pub status: GeoClientTileAliasImpactStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replacement_cluster_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tombstone_reason: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileIdentifierStabilityArtifact {
    pub version: String,
    pub request_blake3: String,
    pub tile_id: String,
    pub before_vintage_id: String,
    pub after_vintage_id: String,
    pub contract: GeoTileIdentifierStabilityContract,
    pub diff: GeoTileIdentifierDiff,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub client_alias_impacts: Vec<GeoClientTileAliasImpact>,
    pub summary: GeoTileIdentifierStabilitySummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTileIdentifierStabilitySummary {
    pub retained_clusters: u64,
    pub added_clusters: u64,
    pub tombstoned_clusters: u64,
    pub merged_prior_ids: u64,
    pub client_aliases: u64,
    pub stale_client_aliases: u64,
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

pub fn check_tile_identifier_stability(
    request: &GeoTileIdentifierStabilityRequest,
) -> Result<GeoTileIdentifierStabilityArtifact, GeoIdentifierError> {
    let request = canonical_tile_identifier_stability_request(request)?;
    reject_retired_id_reuse(&request.before, &request.after)?;
    let diff = diff_tile_identifier_vintages(&request.before, &request.after)?;
    validate_split_tombstone_successors(&request.after)?;
    let client_alias_impacts = client_alias_impacts(&request)?;
    let summary = tile_identifier_stability_summary(&diff, &client_alias_impacts)?;
    let artifact = GeoTileIdentifierStabilityArtifact {
        version: CANON_GEO_TILE_IDENTIFIER_STABILITY_VERSION.to_string(),
        request_blake3: hash_stability_request(&request)?,
        tile_id: request.after.tile_id,
        before_vintage_id: request.before.vintage_id,
        after_vintage_id: request.after.vintage_id,
        contract: default_tile_identifier_stability_contract(),
        diff,
        client_alias_impacts,
        summary,
    };
    validate_tile_identifier_stability_artifact(&artifact)?;
    Ok(artifact)
}

pub fn canonical_tile_identifier_stability_request(
    request: &GeoTileIdentifierStabilityRequest,
) -> Result<GeoTileIdentifierStabilityRequest, GeoIdentifierError> {
    if request.version != CANON_GEO_TILE_IDENTIFIER_STABILITY_REQUEST_VERSION {
        return Err(GeoIdentifierError {
            code: GeoIdentifierErrorCode::UnsupportedVersion,
            message: "Unsupported Geo tile identifier stability request version".to_string(),
            detail: BTreeMap::from([
                ("actual".to_string(), request.version.clone()),
                (
                    "expected".to_string(),
                    CANON_GEO_TILE_IDENTIFIER_STABILITY_REQUEST_VERSION.to_string(),
                ),
            ]),
        });
    }
    let mut canonical = request.clone();
    canonical.before = canonical_tile_identifier_vintage(&canonical.before)?;
    canonical.after = canonical_tile_identifier_vintage(&canonical.after)?;
    canonical.client_aliases.sort_by(|left, right| {
        left.client_alias
            .cmp(&right.client_alias)
            .then_with(|| left.cluster_id.cmp(&right.cluster_id))
    });
    let mut aliases = BTreeSet::new();
    for alias in &canonical.client_aliases {
        validate_identifier("client_aliases[].client_alias", &alias.client_alias)?;
        validate_cluster_id("client_aliases[].cluster_id", &alias.cluster_id)?;
        if !aliases.insert(alias.client_alias.clone()) {
            return Err(GeoIdentifierError::invalid_input(
                "Geo tile identifier stability request contains a duplicate client alias",
                [("client_alias", alias.client_alias.clone())],
            ));
        }
    }
    Ok(canonical)
}

pub fn canonical_tile_identifier_stability_bytes(
    artifact: &GeoTileIdentifierStabilityArtifact,
) -> Result<Vec<u8>, GeoIdentifierError> {
    validate_tile_identifier_stability_artifact(artifact)?;
    let mut canonical = artifact.clone();
    canonical.diff.retained_cluster_ids.sort();
    canonical.diff.added_cluster_ids.sort();
    canonical.diff.tombstoned_cluster_ids.sort();
    canonical.diff.merged_prior_ids.sort();
    canonical
        .client_alias_impacts
        .sort_by(|left, right| left.client_alias.cmp(&right.client_alias));
    canonical.summary =
        tile_identifier_stability_summary(&canonical.diff, &canonical.client_alias_impacts)?;
    serde_json::to_vec(&canonical).map_err(|error| {
        GeoIdentifierError::invalid_input(
            "Geo tile identifier stability artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub fn validate_tile_identifier_stability_artifact(
    artifact: &GeoTileIdentifierStabilityArtifact,
) -> Result<(), GeoIdentifierError> {
    if artifact.version != CANON_GEO_TILE_IDENTIFIER_STABILITY_VERSION {
        return Err(GeoIdentifierError {
            code: GeoIdentifierErrorCode::UnsupportedVersion,
            message: "Unsupported Geo tile identifier stability artifact version".to_string(),
            detail: BTreeMap::from([
                ("actual".to_string(), artifact.version.clone()),
                (
                    "expected".to_string(),
                    CANON_GEO_TILE_IDENTIFIER_STABILITY_VERSION.to_string(),
                ),
            ]),
        });
    }
    validate_identifier("tile_id", &artifact.tile_id)?;
    validate_identifier("before_vintage_id", &artifact.before_vintage_id)?;
    validate_identifier("after_vintage_id", &artifact.after_vintage_id)?;
    validate_blake3("request_blake3", &artifact.request_blake3)?;
    validate_stability_contract(&artifact.contract)?;
    validate_diff("diff", &artifact.diff)?;
    validate_client_alias_impacts(&artifact.client_alias_impacts)?;
    let expected =
        tile_identifier_stability_summary(&artifact.diff, &artifact.client_alias_impacts)?;
    if artifact.summary != expected {
        return Err(GeoIdentifierError::invalid_input(
            "Geo tile identifier stability summary does not match the diff and client impacts",
            [
                ("field", "summary".to_string()),
                ("actual", format!("{:?}", artifact.summary)),
                ("expected", format!("{expected:?}")),
            ],
        ));
    }
    Ok(())
}

pub fn default_tile_identifier_stability_contract() -> GeoTileIdentifierStabilityContract {
    GeoTileIdentifierStabilityContract {
        rules: vec![
            contract_rule(
                "add_new_clusters",
                GeoTileIdentifierContractDisposition::May,
                "A tile refresh may add new clusters.",
            ),
            contract_rule(
                "retire_with_tombstone",
                GeoTileIdentifierContractDisposition::May,
                "A tile refresh may retire a cluster only with a tombstone.",
            ),
            contract_rule(
                "split_with_tombstone_successors",
                GeoTileIdentifierContractDisposition::May,
                "A tile refresh may split a cluster only by retiring the prior id with a tombstone naming successor clusters.",
            ),
            contract_rule(
                "merge_retaining_prior_ids",
                GeoTileIdentifierContractDisposition::May,
                "A tile refresh may merge clusters by retaining every prior id as an alias of the survivor.",
            ),
            contract_rule(
                "reassign_existing_id",
                GeoTileIdentifierContractDisposition::Never,
                "A tile refresh must never reassign an existing minted id to different geometry.",
            ),
            contract_rule(
                "reuse_retired_id",
                GeoTileIdentifierContractDisposition::Never,
                "A tile refresh must never reuse a retired id.",
            ),
        ],
    }
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

fn canonical_tile_identifier_vintage(
    vintage: &GeoTileIdentifierVintage,
) -> Result<GeoTileIdentifierVintage, GeoIdentifierError> {
    validate_identifier("tile_id", &vintage.tile_id)?;
    validate_identifier("vintage_id", &vintage.vintage_id)?;
    let mut canonical = vintage.clone();
    for cluster in &mut canonical.clusters {
        validate_cluster(cluster)?;
        cluster.aliases.sort();
        cluster.aliases.dedup();
    }
    canonical
        .clusters
        .sort_by(|left, right| left.cluster_id.cmp(&right.cluster_id));
    for tombstone in &mut canonical.tombstones {
        validate_tombstone(tombstone)?;
        tombstone.successor_cluster_ids.sort();
        tombstone.successor_cluster_ids.dedup();
    }
    canonical
        .tombstones
        .sort_by(|left, right| left.cluster_id.cmp(&right.cluster_id));
    Ok(canonical)
}

fn reject_retired_id_reuse(
    before: &GeoTileIdentifierVintage,
    after: &GeoTileIdentifierVintage,
) -> Result<(), GeoIdentifierError> {
    let after_clusters = cluster_map("after.clusters", &after.clusters)?;
    for tombstone in &before.tombstones {
        validate_tombstone(tombstone)?;
        if after_clusters.contains_key(&tombstone.cluster_id) {
            return Err(GeoIdentifierError::invalid_input(
                "Geo tile refresh reused a retired minted id",
                [
                    ("cluster_id", tombstone.cluster_id.clone()),
                    ("before_vintage_id", before.vintage_id.clone()),
                    ("after_vintage_id", after.vintage_id.clone()),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_split_tombstone_successors(
    after: &GeoTileIdentifierVintage,
) -> Result<(), GeoIdentifierError> {
    let after_clusters = cluster_map("after.clusters", &after.clusters)?;
    for tombstone in &after.tombstones {
        validate_tombstone(tombstone)?;
        if after_clusters.contains_key(&tombstone.cluster_id) {
            return Err(GeoIdentifierError::invalid_input(
                "Geo tile refresh cannot both retain and tombstone a minted id",
                [("cluster_id", tombstone.cluster_id.clone())],
            ));
        }
        if tombstone.reason.contains("split") && tombstone.successor_cluster_ids.is_empty() {
            return Err(GeoIdentifierError::invalid_input(
                "Geo split tombstone must name successor clusters",
                [("cluster_id", tombstone.cluster_id.clone())],
            ));
        }
        for successor in &tombstone.successor_cluster_ids {
            if !after_clusters.contains_key(successor) {
                return Err(GeoIdentifierError::invalid_input(
                    "Geo tombstone successor is absent from the refreshed tile vintage",
                    [
                        ("cluster_id", tombstone.cluster_id.clone()),
                        ("successor_cluster_id", successor.clone()),
                    ],
                ));
            }
        }
        if let Some(survivor) = &tombstone.survivor_cluster_id
            && !after_clusters.contains_key(survivor)
        {
            return Err(GeoIdentifierError::invalid_input(
                "Geo tombstone survivor is absent from the refreshed tile vintage",
                [
                    ("cluster_id", tombstone.cluster_id.clone()),
                    ("survivor_cluster_id", survivor.clone()),
                ],
            ));
        }
    }
    Ok(())
}

fn client_alias_impacts(
    request: &GeoTileIdentifierStabilityRequest,
) -> Result<Vec<GeoClientTileAliasImpact>, GeoIdentifierError> {
    let after_clusters = cluster_map("after.clusters", &request.after.clusters)?;
    let after_tombstones = request
        .after
        .tombstones
        .iter()
        .map(|tombstone| (tombstone.cluster_id.clone(), tombstone))
        .collect::<BTreeMap<_, _>>();
    let alias_survivors = retained_alias_survivors(&request.after.clusters)?;
    let mut impacts = Vec::with_capacity(request.client_aliases.len());
    for alias in &request.client_aliases {
        let impact = if after_clusters.contains_key(&alias.cluster_id) {
            GeoClientTileAliasImpact {
                client_alias: alias.client_alias.clone(),
                cluster_id: alias.cluster_id.clone(),
                status: GeoClientTileAliasImpactStatus::Active,
                replacement_cluster_ids: Vec::new(),
                tombstone_reason: None,
                message: "client alias still points at an active retained cluster".to_string(),
            }
        } else if let Some(tombstone) = after_tombstones.get(&alias.cluster_id) {
            let mut replacements = tombstone.successor_cluster_ids.clone();
            if let Some(survivor) = &tombstone.survivor_cluster_id {
                replacements.push(survivor.clone());
            }
            replacements.sort();
            replacements.dedup();
            GeoClientTileAliasImpact {
                client_alias: alias.client_alias.clone(),
                cluster_id: alias.cluster_id.clone(),
                status: GeoClientTileAliasImpactStatus::Tombstoned,
                replacement_cluster_ids: replacements,
                tombstone_reason: Some(tombstone.reason.clone()),
                message: "client alias points at a tombstoned tile cluster".to_string(),
            }
        } else if let Some(survivor) = alias_survivors.get(&alias.cluster_id) {
            GeoClientTileAliasImpact {
                client_alias: alias.client_alias.clone(),
                cluster_id: alias.cluster_id.clone(),
                status: GeoClientTileAliasImpactStatus::MergedToSurvivor,
                replacement_cluster_ids: vec![survivor.clone()],
                tombstone_reason: None,
                message: "client alias points at a prior id retained as a survivor alias"
                    .to_string(),
            }
        } else {
            GeoClientTileAliasImpact {
                client_alias: alias.client_alias.clone(),
                cluster_id: alias.cluster_id.clone(),
                status: GeoClientTileAliasImpactStatus::UnknownCluster,
                replacement_cluster_ids: Vec::new(),
                tombstone_reason: None,
                message: "client alias points at a cluster absent from the refreshed tile contract"
                    .to_string(),
            }
        };
        impacts.push(impact);
    }
    impacts.sort_by(|left, right| left.client_alias.cmp(&right.client_alias));
    Ok(impacts)
}

fn retained_alias_survivors(
    clusters: &[GeoIdentifierCluster],
) -> Result<BTreeMap<String, String>, GeoIdentifierError> {
    let mut survivors = BTreeMap::new();
    for cluster in clusters {
        validate_cluster(cluster)?;
        for alias in cluster
            .aliases
            .iter()
            .filter(|alias| alias.starts_with("cmdrvl:"))
        {
            if let Some(previous) = survivors.insert(alias.clone(), cluster.cluster_id.clone()) {
                return Err(GeoIdentifierError::invalid_input(
                    "Geo prior minted id is retained as an alias by multiple survivor clusters",
                    [
                        ("prior_cluster_id", alias.clone()),
                        ("survivor_before", previous),
                        ("survivor_after", cluster.cluster_id.clone()),
                    ],
                ));
            }
        }
    }
    Ok(survivors)
}

fn tile_identifier_stability_summary(
    diff: &GeoTileIdentifierDiff,
    client_alias_impacts: &[GeoClientTileAliasImpact],
) -> Result<GeoTileIdentifierStabilitySummary, GeoIdentifierError> {
    Ok(GeoTileIdentifierStabilitySummary {
        retained_clusters: usize_to_u64(diff.retained_cluster_ids.len(), "retained_clusters")?,
        added_clusters: usize_to_u64(diff.added_cluster_ids.len(), "added_clusters")?,
        tombstoned_clusters: usize_to_u64(
            diff.tombstoned_cluster_ids.len(),
            "tombstoned_clusters",
        )?,
        merged_prior_ids: usize_to_u64(diff.merged_prior_ids.len(), "merged_prior_ids")?,
        client_aliases: usize_to_u64(client_alias_impacts.len(), "client_aliases")?,
        stale_client_aliases: usize_to_u64(
            client_alias_impacts
                .iter()
                .filter(|impact| impact.status != GeoClientTileAliasImpactStatus::Active)
                .count(),
            "stale_client_aliases",
        )?,
    })
}

fn validate_stability_contract(
    contract: &GeoTileIdentifierStabilityContract,
) -> Result<(), GeoIdentifierError> {
    let mut rules = BTreeSet::new();
    for rule in &contract.rules {
        validate_identifier("contract.rules[].rule_id", &rule.rule_id)?;
        validate_identifier("contract.rules[].statement", &rule.statement)?;
        if !rules.insert(rule.rule_id.clone()) {
            return Err(GeoIdentifierError::invalid_input(
                "Geo tile identifier stability contract contains a duplicate rule",
                [("rule_id", rule.rule_id.clone())],
            ));
        }
    }
    for required in [
        "add_new_clusters",
        "retire_with_tombstone",
        "split_with_tombstone_successors",
        "merge_retaining_prior_ids",
        "reassign_existing_id",
        "reuse_retired_id",
    ] {
        if !rules.contains(required) {
            return Err(GeoIdentifierError::invalid_input(
                "Geo tile identifier stability contract is missing a required rule",
                [("rule_id", required.to_string())],
            ));
        }
    }
    Ok(())
}

fn validate_diff(
    field: &'static str,
    diff: &GeoTileIdentifierDiff,
) -> Result<(), GeoIdentifierError> {
    validate_cluster_id_list(field, "retained_cluster_ids", &diff.retained_cluster_ids)?;
    validate_cluster_id_list(field, "added_cluster_ids", &diff.added_cluster_ids)?;
    validate_cluster_id_list(
        field,
        "tombstoned_cluster_ids",
        &diff.tombstoned_cluster_ids,
    )?;
    validate_cluster_id_list(field, "merged_prior_ids", &diff.merged_prior_ids)
}

fn validate_cluster_id_list(
    parent: &'static str,
    field: &'static str,
    ids: &[String],
) -> Result<(), GeoIdentifierError> {
    let mut seen = BTreeSet::new();
    for id in ids {
        validate_cluster_id(field, id)?;
        if !seen.insert(id) {
            return Err(GeoIdentifierError::invalid_input(
                "Geo tile identifier stability id lists must not contain duplicates",
                [
                    ("field", format!("{parent}.{field}")),
                    ("cluster_id", id.clone()),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_client_alias_impacts(
    impacts: &[GeoClientTileAliasImpact],
) -> Result<(), GeoIdentifierError> {
    let mut aliases = BTreeSet::new();
    let mut previous: Option<&str> = None;
    for impact in impacts {
        validate_identifier("client_alias_impacts[].client_alias", &impact.client_alias)?;
        validate_cluster_id("client_alias_impacts[].cluster_id", &impact.cluster_id)?;
        validate_identifier("client_alias_impacts[].message", &impact.message)?;
        for replacement in &impact.replacement_cluster_ids {
            validate_cluster_id(
                "client_alias_impacts[].replacement_cluster_ids",
                replacement,
            )?;
        }
        if let Some(reason) = &impact.tombstone_reason {
            validate_identifier("client_alias_impacts[].tombstone_reason", reason)?;
        }
        if !aliases.insert(impact.client_alias.as_str()) {
            return Err(GeoIdentifierError::invalid_input(
                "Geo tile identifier stability client impacts contain a duplicate alias",
                [("client_alias", impact.client_alias.clone())],
            ));
        }
        if let Some(previous_alias) = previous
            && previous_alias >= impact.client_alias.as_str()
        {
            return Err(GeoIdentifierError::invalid_input(
                "Geo tile identifier stability client impacts must be sorted by client_alias",
                [
                    ("previous_client_alias", previous_alias.to_string()),
                    ("client_alias", impact.client_alias.clone()),
                ],
            ));
        }
        previous = Some(impact.client_alias.as_str());
    }
    Ok(())
}

fn contract_rule(
    rule_id: &str,
    disposition: GeoTileIdentifierContractDisposition,
    statement: &str,
) -> GeoTileIdentifierContractRule {
    GeoTileIdentifierContractRule {
        rule_id: rule_id.to_string(),
        disposition,
        statement: statement.to_string(),
    }
}

fn hash_stability_request(
    request: &GeoTileIdentifierStabilityRequest,
) -> Result<String, GeoIdentifierError> {
    serde_json::to_vec(request)
        .map(|bytes| format!("blake3:{}", blake3::hash(&bytes).to_hex()))
        .map_err(|error| {
            GeoIdentifierError::invalid_input(
                "Geo tile identifier stability request could not be serialized for hashing",
                [("error", error.to_string())],
            )
        })
}

fn validate_tombstone(tombstone: &GeoIdentifierTombstone) -> Result<(), GeoIdentifierError> {
    validate_cluster_id("tombstones[].cluster_id", &tombstone.cluster_id)?;
    validate_blake3("tombstones[].geometry_blake3", &tombstone.geometry_blake3)?;
    validate_identifier("tombstones[].reason", &tombstone.reason)?;
    for successor in &tombstone.successor_cluster_ids {
        validate_cluster_id("tombstones[].successor_cluster_ids", successor)?;
    }
    if let Some(survivor) = &tombstone.survivor_cluster_id {
        validate_cluster_id("tombstones[].survivor_cluster_id", survivor)?;
    }
    Ok(())
}

fn usize_to_u64(value: usize, field: &'static str) -> Result<u64, GeoIdentifierError> {
    u64::try_from(value).map_err(|_| {
        GeoIdentifierError::invalid_input(
            "Geo identifier count does not fit in u64",
            [("field", field.to_string())],
        )
    })
}

fn tombstone_map<'a>(
    before_clusters: &BTreeMap<String, &'a GeoIdentifierCluster>,
    tombstones: &'a [GeoIdentifierTombstone],
) -> Result<BTreeMap<String, &'a GeoIdentifierTombstone>, GeoIdentifierError> {
    let mut by_id = BTreeMap::new();
    for tombstone in tombstones {
        validate_tombstone(tombstone)?;
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
