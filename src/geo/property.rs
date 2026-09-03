#![forbid(unsafe_code)]

//! Document-scoped property assertions over stable Geo member ids.
//!
//! Property is constituted by a source document. This module mints one
//! `cmdrvl:property:*` proposal per document assertion and publishes the
//! derived relation projection that makes the membership set usable without
//! changing Canon's exact lookup kernel.

use super::{
    composition::{GeoEntityLevel, GeoEntityRef},
    control::GeoControlRelation,
    identifiers::{
        CANON_GEO_REGISTRY_PROPOSAL_VERSION, GeoIdentifierError, GeoLedgerIdentifierRow,
        GeoPropertyDocumentAssertion, GeoPropertySetComparison, GeoPropertySetRelation,
        GeoRegistryMintProposal, compare_property_sets, registry_proposal_from_ledger_rows,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_PROPERTY_ASSERTION_REQUEST_VERSION: &str =
    "canon_geo_property_assertion_request.v0";
pub const CANON_GEO_PROPERTY_ASSERTION_VERSION: &str = "canon_geo_property_assertion.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPropertyAssertionProofClass {
    Fixture,
    RetainedArtifact,
    ObservedWarehouseSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPropertyBlockingStrategy {
    DocumentFirstThenGeography,
    TileLocalOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPropertyRelationGraphProduct {
    PublishedDerivedProjection,
    WorkbenchInternalOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPropertyMembershipStatus {
    AssertedMember,
    AbstainedMembership,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPropertyMembershipAbstentionReason {
    MembershipUnresolved,
    AmbiguousMembership,
    VariousAddressRequiresAnnexAParse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPropertyDocumentMembershipStatus {
    Complete,
    PartialMembershipAbstained,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropertySourceCorpus {
    pub corpus_id: String,
    pub corpus_version: String,
    pub temporal_scope: String,
    pub native_key_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropertySourceRecordRef {
    pub source_record_id: String,
    pub source_vintage: String,
    pub record_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropertyMemberAssertion {
    pub member: GeoEntityRef,
    pub tile_id: String,
    pub status: GeoPropertyMembershipStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abstention_reason: Option<GeoPropertyMembershipAbstentionReason>,
    pub source_record: GeoPropertySourceRecordRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropertyDocumentAssertionRequest {
    pub assertion_id: String,
    pub document_id: String,
    pub accession: String,
    pub deal_id: String,
    pub loan_id: String,
    pub collateral_set_id: String,
    pub source_record: GeoPropertySourceRecordRef,
    pub members: Vec<GeoPropertyMemberAssertion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropertyAssertionRequest {
    pub version: String,
    pub proof_class: GeoPropertyAssertionProofClass,
    pub blocking_strategy: GeoPropertyBlockingStrategy,
    pub relation_graph_product: GeoPropertyRelationGraphProduct,
    pub source_corpus: GeoPropertySourceCorpus,
    pub assertions: Vec<GeoPropertyDocumentAssertionRequest>,
    pub max_assertions: usize,
    pub max_members_per_assertion: usize,
    pub max_pairwise_comparisons: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropertyAssertionSummary {
    pub document_assertions: u64,
    pub property_entities: u64,
    pub collateral_memberships: u64,
    pub membership_edges: u64,
    pub membership_abstentions: u64,
    pub tile_spanning_properties: u64,
    pub pairwise_comparisons: u64,
    pub overlapping_unequal_assertions: u64,
    pub registry_entries: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropertyDocumentKey {
    pub document_id: String,
    pub accession: String,
    pub deal_id: String,
    pub loan_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropertyDocumentEntity {
    pub property_id: String,
    pub document_alias: String,
    pub document_key: GeoPropertyDocumentKey,
    pub collateral_set_id: String,
    pub source_record: GeoPropertySourceRecordRef,
    pub parcel_ids: Vec<String>,
    pub building_ids: Vec<String>,
    pub tile_ids: Vec<String>,
    pub tile_spanning: bool,
    pub membership_status: GeoPropertyDocumentMembershipStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropertyCollateralMembership {
    pub collateral_set_id: String,
    pub property: GeoEntityRef,
    pub document_key: GeoPropertyDocumentKey,
    pub source_record: GeoPropertySourceRecordRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropertyMemberRelation {
    pub relation_id: String,
    pub property: GeoEntityRef,
    pub relation: GeoControlRelation,
    pub member: GeoEntityRef,
    pub collateral_set_id: String,
    pub tile_id: String,
    pub source_record: GeoPropertySourceRecordRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropertyMembershipAbstention {
    pub abstention_id: String,
    pub property_id: String,
    pub collateral_set_id: String,
    pub candidate_member: GeoEntityRef,
    pub tile_id: String,
    pub reason: GeoPropertyMembershipAbstentionReason,
    pub source_record: GeoPropertySourceRecordRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropertyAssertionComparison {
    pub comparison_id: String,
    pub comparison: GeoPropertySetComparison,
    pub overlapping_unequal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropertyAssertionArtifact {
    pub version: String,
    pub request_version: String,
    pub request_blake3: String,
    pub proof_class: GeoPropertyAssertionProofClass,
    pub blocking_strategy: GeoPropertyBlockingStrategy,
    pub relation_graph_product: GeoPropertyRelationGraphProduct,
    pub source_corpus: GeoPropertySourceCorpus,
    pub summary: GeoPropertyAssertionSummary,
    pub registry_proposal: GeoRegistryMintProposal,
    pub property_assertions: Vec<GeoPropertyDocumentEntity>,
    pub collateral_memberships: Vec<GeoPropertyCollateralMembership>,
    pub member_relations: Vec<GeoPropertyMemberRelation>,
    pub membership_abstentions: Vec<GeoPropertyMembershipAbstention>,
    pub assertion_comparisons: Vec<GeoPropertyAssertionComparison>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPropertyAssertionErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
    TileLocalBlockingRefused,
    RelationGraphProjectionRequired,
    IdentifierProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropertyAssertionError {
    pub code: GeoPropertyAssertionErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoPropertyAssertionError {
    fn new(
        code: GeoPropertyAssertionErrorCode,
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

    fn invalid(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(GeoPropertyAssertionErrorCode::InvalidInput, message, detail)
    }

    fn budget(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(
            GeoPropertyAssertionErrorCode::BudgetExceeded,
            message,
            detail,
        )
    }
}

impl fmt::Display for GeoPropertyAssertionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeoPropertyAssertionError {}

pub fn materialize_property_assertions(
    request: &GeoPropertyAssertionRequest,
) -> Result<GeoPropertyAssertionArtifact, GeoPropertyAssertionError> {
    validate_property_assertion_request(request)?;

    let mut assertions = request.assertions.clone();
    assertions.sort_by_key(document_request_key);
    let ledger_rows = ledger_rows_for_assertions(&assertions)?;
    let source_ledger_bytes = canonical_property_ledger_bytes(&ledger_rows)?;
    let registry_proposal = registry_proposal_from_ledger_rows(&source_ledger_bytes, &ledger_rows)
        .map_err(identifier_projection_error)?;
    let registry_assertions = registry_assertions_by_document(&registry_proposal)?;

    let mut property_assertions = Vec::new();
    let mut collateral_memberships = Vec::new();
    let mut member_relations = Vec::new();
    let mut membership_abstentions = Vec::new();

    for assertion in &assertions {
        let key = (assertion.accession.as_str(), assertion.loan_id.as_str());
        let registry_assertion = registry_assertions.get(&key).ok_or_else(|| {
            GeoPropertyAssertionError::new(
                GeoPropertyAssertionErrorCode::IdentifierProjection,
                "Geo property registry proposal omitted a document assertion",
                [
                    ("accession", assertion.accession.as_str()),
                    ("loan_id", assertion.loan_id.as_str()),
                ],
            )
        })?;
        let property_ref =
            GeoEntityRef::new(GeoEntityLevel::Property, &registry_assertion.property_id);
        let document_key = document_key(assertion);
        let asserted_tiles = asserted_member_tile_ids(assertion);
        let has_abstentions = assertion
            .members
            .iter()
            .any(|member| member.status == GeoPropertyMembershipStatus::AbstainedMembership);

        property_assertions.push(GeoPropertyDocumentEntity {
            property_id: registry_assertion.property_id.clone(),
            document_alias: registry_assertion.document_alias.clone(),
            document_key: document_key.clone(),
            collateral_set_id: assertion.collateral_set_id.clone(),
            source_record: assertion.source_record.clone(),
            parcel_ids: registry_assertion.parcel_ids.clone(),
            building_ids: registry_assertion.building_ids.clone(),
            tile_ids: asserted_tiles.clone(),
            tile_spanning: asserted_tiles.len() > 1,
            membership_status: if has_abstentions {
                GeoPropertyDocumentMembershipStatus::PartialMembershipAbstained
            } else {
                GeoPropertyDocumentMembershipStatus::Complete
            },
        });
        collateral_memberships.push(GeoPropertyCollateralMembership {
            collateral_set_id: assertion.collateral_set_id.clone(),
            property: property_ref.clone(),
            document_key: document_key.clone(),
            source_record: assertion.source_record.clone(),
        });

        for member in &assertion.members {
            match member.status {
                GeoPropertyMembershipStatus::AssertedMember => {
                    member_relations.push(GeoPropertyMemberRelation {
                        relation_id: member_relation_id(
                            &registry_assertion.property_id,
                            &assertion.collateral_set_id,
                            &member.member,
                        ),
                        property: property_ref.clone(),
                        relation: GeoControlRelation::Contains,
                        member: member.member.clone(),
                        collateral_set_id: assertion.collateral_set_id.clone(),
                        tile_id: member.tile_id.clone(),
                        source_record: member.source_record.clone(),
                    });
                }
                GeoPropertyMembershipStatus::AbstainedMembership => {
                    membership_abstentions.push(GeoPropertyMembershipAbstention {
                        abstention_id: membership_abstention_id(
                            &registry_assertion.property_id,
                            &assertion.collateral_set_id,
                            &member.member,
                            member
                                .abstention_reason
                                .expect("validated abstention reason"),
                        ),
                        property_id: registry_assertion.property_id.clone(),
                        collateral_set_id: assertion.collateral_set_id.clone(),
                        candidate_member: member.member.clone(),
                        tile_id: member.tile_id.clone(),
                        reason: member
                            .abstention_reason
                            .expect("validated abstention reason"),
                        source_record: member.source_record.clone(),
                    });
                }
            }
        }
    }

    property_assertions.sort_by(|left, right| {
        left.property_id
            .cmp(&right.property_id)
            .then_with(|| left.document_alias.cmp(&right.document_alias))
    });
    collateral_memberships.sort_by(|left, right| {
        left.collateral_set_id
            .cmp(&right.collateral_set_id)
            .then_with(|| left.property.id.cmp(&right.property.id))
    });
    member_relations.sort_by(|left, right| left.relation_id.cmp(&right.relation_id));
    membership_abstentions.sort_by(|left, right| left.abstention_id.cmp(&right.abstention_id));

    let assertion_comparisons = assertion_comparisons(&registry_proposal.property_assertions)?;
    if assertion_comparisons.len() > request.max_pairwise_comparisons {
        return Err(GeoPropertyAssertionError::budget(
            "Geo property assertion comparisons exceed request budget",
            [
                (
                    "max_pairwise_comparisons",
                    request.max_pairwise_comparisons.to_string(),
                ),
                ("actual", assertion_comparisons.len().to_string()),
            ],
        ));
    }

    let summary = GeoPropertyAssertionSummary {
        document_assertions: usize_to_u64(assertions.len(), "assertions.len")?,
        property_entities: usize_to_u64(property_assertions.len(), "property_assertions.len")?,
        collateral_memberships: usize_to_u64(
            collateral_memberships.len(),
            "collateral_memberships.len",
        )?,
        membership_edges: usize_to_u64(member_relations.len(), "member_relations.len")?,
        membership_abstentions: usize_to_u64(
            membership_abstentions.len(),
            "membership_abstentions.len",
        )?,
        tile_spanning_properties: usize_to_u64(
            property_assertions
                .iter()
                .filter(|property| property.tile_spanning)
                .count(),
            "tile_spanning_properties",
        )?,
        pairwise_comparisons: usize_to_u64(
            assertion_comparisons.len(),
            "assertion_comparisons.len",
        )?,
        overlapping_unequal_assertions: usize_to_u64(
            assertion_comparisons
                .iter()
                .filter(|comparison| comparison.overlapping_unequal)
                .count(),
            "overlapping_unequal_assertions",
        )?,
        registry_entries: usize_to_u64(registry_proposal.entries.len(), "registry_entries")?,
    };

    let request_blake3 = format!(
        "blake3:{}",
        blake3::hash(&canonical_property_assertion_request_bytes(request)?).to_hex()
    );
    let artifact = GeoPropertyAssertionArtifact {
        version: CANON_GEO_PROPERTY_ASSERTION_VERSION.to_string(),
        request_version: request.version.clone(),
        request_blake3,
        proof_class: request.proof_class,
        blocking_strategy: request.blocking_strategy,
        relation_graph_product: request.relation_graph_product,
        source_corpus: request.source_corpus.clone(),
        summary,
        registry_proposal,
        property_assertions,
        collateral_memberships,
        member_relations,
        membership_abstentions,
        assertion_comparisons,
    };
    validate_property_assertion_artifact(&artifact)?;
    Ok(artifact)
}

pub fn canonical_property_assertion_request_bytes(
    request: &GeoPropertyAssertionRequest,
) -> Result<Vec<u8>, GeoPropertyAssertionError> {
    validate_property_assertion_request(request)?;
    serde_json::to_vec(request).map_err(|error| {
        GeoPropertyAssertionError::invalid(
            "Geo property assertion request could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub fn canonical_property_assertion_bytes(
    artifact: &GeoPropertyAssertionArtifact,
) -> Result<Vec<u8>, GeoPropertyAssertionError> {
    validate_property_assertion_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoPropertyAssertionError::invalid(
            "Geo property assertion artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub fn validate_property_assertion_artifact(
    artifact: &GeoPropertyAssertionArtifact,
) -> Result<(), GeoPropertyAssertionError> {
    if artifact.version != CANON_GEO_PROPERTY_ASSERTION_VERSION {
        return Err(GeoPropertyAssertionError::new(
            GeoPropertyAssertionErrorCode::UnsupportedVersion,
            "Unsupported Geo property assertion artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_PROPERTY_ASSERTION_VERSION),
            ],
        ));
    }
    if artifact.request_version != CANON_GEO_PROPERTY_ASSERTION_REQUEST_VERSION {
        return Err(GeoPropertyAssertionError::new(
            GeoPropertyAssertionErrorCode::UnsupportedVersion,
            "Unsupported Geo property assertion request version",
            [
                ("actual", artifact.request_version.as_str()),
                ("expected", CANON_GEO_PROPERTY_ASSERTION_REQUEST_VERSION),
            ],
        ));
    }
    validate_blake3_uri("request_blake3", &artifact.request_blake3)?;
    if artifact.blocking_strategy != GeoPropertyBlockingStrategy::DocumentFirstThenGeography {
        return Err(tile_local_blocking_refusal());
    }
    if artifact.relation_graph_product
        != GeoPropertyRelationGraphProduct::PublishedDerivedProjection
    {
        return Err(relation_graph_projection_required());
    }
    validate_source_corpus(&artifact.source_corpus)?;
    validate_registry_proposal(&artifact.registry_proposal)?;
    validate_summary(artifact)?;
    validate_property_entities(&artifact.property_assertions)?;
    validate_collateral_memberships(
        &artifact.collateral_memberships,
        &artifact.property_assertions,
    )?;
    validate_member_relations(&artifact.member_relations, &artifact.property_assertions)?;
    validate_membership_abstentions(
        &artifact.membership_abstentions,
        &artifact.property_assertions,
    )?;
    validate_assertion_comparisons(
        &artifact.assertion_comparisons,
        &artifact.registry_proposal.property_assertions,
    )?;
    Ok(())
}

fn validate_property_assertion_request(
    request: &GeoPropertyAssertionRequest,
) -> Result<(), GeoPropertyAssertionError> {
    if request.version != CANON_GEO_PROPERTY_ASSERTION_REQUEST_VERSION {
        return Err(GeoPropertyAssertionError::new(
            GeoPropertyAssertionErrorCode::UnsupportedVersion,
            "Unsupported Geo property assertion request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_PROPERTY_ASSERTION_REQUEST_VERSION),
            ],
        ));
    }
    if request.blocking_strategy != GeoPropertyBlockingStrategy::DocumentFirstThenGeography {
        return Err(tile_local_blocking_refusal());
    }
    if request.relation_graph_product != GeoPropertyRelationGraphProduct::PublishedDerivedProjection
    {
        return Err(relation_graph_projection_required());
    }
    validate_source_corpus(&request.source_corpus)?;
    if request.assertions.is_empty() {
        return Err(GeoPropertyAssertionError::invalid(
            "Geo property assertion requests must contain at least one document assertion",
            [("field", "assertions")],
        ));
    }
    if request.assertions.len() > request.max_assertions {
        return Err(GeoPropertyAssertionError::budget(
            "Geo property assertion request exceeds assertion budget",
            [
                ("max_assertions", request.max_assertions.to_string()),
                ("actual", request.assertions.len().to_string()),
            ],
        ));
    }
    let pairwise = pairwise_count(request.assertions.len())?;
    if pairwise > request.max_pairwise_comparisons {
        return Err(GeoPropertyAssertionError::budget(
            "Geo property assertion request exceeds pairwise comparison budget",
            [
                (
                    "max_pairwise_comparisons",
                    request.max_pairwise_comparisons.to_string(),
                ),
                ("actual", pairwise.to_string()),
            ],
        ));
    }

    let mut document_keys = BTreeSet::new();
    for assertion in &request.assertions {
        validate_document_assertion_request(assertion)?;
        if assertion.members.len() > request.max_members_per_assertion {
            return Err(GeoPropertyAssertionError::budget(
                "Geo property assertion exceeds member budget",
                [
                    (
                        "max_members_per_assertion",
                        request.max_members_per_assertion.to_string(),
                    ),
                    ("actual", assertion.members.len().to_string()),
                    ("assertion_id", assertion.assertion_id.clone()),
                ],
            ));
        }
        if !document_keys.insert(document_request_key(assertion)) {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo property assertions must be unique per document assertion",
                [
                    ("document_id", assertion.document_id.as_str()),
                    ("accession", assertion.accession.as_str()),
                    ("loan_id", assertion.loan_id.as_str()),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_document_assertion_request(
    assertion: &GeoPropertyDocumentAssertionRequest,
) -> Result<(), GeoPropertyAssertionError> {
    validate_identifier("assertions[].assertion_id", &assertion.assertion_id)?;
    validate_identifier("assertions[].document_id", &assertion.document_id)?;
    validate_identifier("assertions[].accession", &assertion.accession)?;
    validate_identifier("assertions[].deal_id", &assertion.deal_id)?;
    validate_identifier("assertions[].loan_id", &assertion.loan_id)?;
    validate_identifier(
        "assertions[].collateral_set_id",
        &assertion.collateral_set_id,
    )?;
    validate_source_record("assertions[].source_record", &assertion.source_record)?;
    if assertion.members.is_empty() {
        return Err(GeoPropertyAssertionError::invalid(
            "Geo property assertions must carry at least one member or abstained candidate",
            [("assertion_id", assertion.assertion_id.as_str())],
        ));
    }
    let mut member_keys = BTreeSet::new();
    let mut asserted_members = 0_usize;
    for member in &assertion.members {
        validate_member_assertion(member)?;
        let key = (
            member.member.level,
            member.member.id.clone(),
            member.status,
            member.tile_id.clone(),
        );
        if !member_keys.insert(key) {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo property document assertion contains a duplicate member row",
                [
                    ("assertion_id", assertion.assertion_id.as_str()),
                    ("member_id", member.member.id.as_str()),
                ],
            ));
        }
        if member.status == GeoPropertyMembershipStatus::AssertedMember {
            asserted_members += 1;
        }
    }
    if asserted_members == 0 {
        return Err(GeoPropertyAssertionError::invalid(
            "Geo property assertions must not mint a verified membership set with zero asserted members",
            [("assertion_id", assertion.assertion_id.as_str())],
        ));
    }
    Ok(())
}

fn validate_member_assertion(
    member: &GeoPropertyMemberAssertion,
) -> Result<(), GeoPropertyAssertionError> {
    validate_entity_ref("members[].member", &member.member)?;
    validate_identifier("members[].tile_id", &member.tile_id)?;
    validate_source_record("members[].source_record", &member.source_record)?;
    match member.status {
        GeoPropertyMembershipStatus::AssertedMember => {
            if member.abstention_reason.is_some() {
                return Err(GeoPropertyAssertionError::invalid(
                    "Asserted property members must not carry an abstention reason",
                    [("member_id", member.member.id.as_str())],
                ));
            }
        }
        GeoPropertyMembershipStatus::AbstainedMembership => {
            if member.abstention_reason.is_none() {
                return Err(GeoPropertyAssertionError::invalid(
                    "Abstained property members must carry a typed abstention reason",
                    [("member_id", member.member.id.as_str())],
                ));
            }
        }
    }
    Ok(())
}

fn ledger_rows_for_assertions(
    assertions: &[GeoPropertyDocumentAssertionRequest],
) -> Result<Vec<GeoLedgerIdentifierRow>, GeoPropertyAssertionError> {
    let mut rows = Vec::new();
    for assertion in assertions {
        let mut parcel_set = Vec::new();
        let mut building_set = Vec::new();
        for member in assertion
            .members
            .iter()
            .filter(|member| member.status == GeoPropertyMembershipStatus::AssertedMember)
        {
            match member.member.level {
                GeoEntityLevel::Parcel => parcel_set.push(member.member.id.clone()),
                GeoEntityLevel::Building => building_set.push(member.member.id.clone()),
                GeoEntityLevel::Property | GeoEntityLevel::PoiUnit => {
                    return Err(GeoPropertyAssertionError::invalid(
                        "Geo property members must be parcel or building cluster ids",
                        [
                            ("member_id", member.member.id.as_str()),
                            ("member_level", level_name(member.member.level)),
                        ],
                    ));
                }
            }
        }
        rows.push(GeoLedgerIdentifierRow {
            accession: assertion.accession.clone(),
            deal_id: assertion.deal_id.clone(),
            loan_id: assertion.loan_id.clone(),
            reach: Some("full".to_string()),
            reach_none_reason: None,
            parcel_set: Some(parcel_set),
            building_set: Some(building_set),
        });
    }
    Ok(rows)
}

fn canonical_property_ledger_bytes(
    rows: &[GeoLedgerIdentifierRow],
) -> Result<Vec<u8>, GeoPropertyAssertionError> {
    #[derive(Serialize)]
    struct LedgerSeed<'a> {
        version: &'static str,
        rows: &'a [GeoLedgerIdentifierRow],
    }
    serde_json::to_vec(&LedgerSeed {
        version: "canon_geo_collateral_ledger.v0",
        rows,
    })
    .map_err(|error| {
        GeoPropertyAssertionError::invalid(
            "Geo property ledger seed could not be serialized",
            [("error", error.to_string())],
        )
    })
}

fn registry_assertions_by_document(
    proposal: &GeoRegistryMintProposal,
) -> Result<BTreeMap<(&str, &str), &GeoPropertyDocumentAssertion>, GeoPropertyAssertionError> {
    let mut by_key = BTreeMap::new();
    for assertion in &proposal.property_assertions {
        let key = (assertion.accession.as_str(), assertion.loan_id.as_str());
        if by_key.insert(key, assertion).is_some() {
            return Err(GeoPropertyAssertionError::new(
                GeoPropertyAssertionErrorCode::IdentifierProjection,
                "Geo registry proposal returned duplicate property document assertions",
                [
                    ("accession", assertion.accession.as_str()),
                    ("loan_id", assertion.loan_id.as_str()),
                ],
            ));
        }
    }
    Ok(by_key)
}

fn assertion_comparisons(
    assertions: &[GeoPropertyDocumentAssertion],
) -> Result<Vec<GeoPropertyAssertionComparison>, GeoPropertyAssertionError> {
    let mut comparisons = Vec::new();
    for left_index in 0..assertions.len() {
        for right_index in (left_index + 1)..assertions.len() {
            let comparison =
                compare_property_sets(&assertions[left_index], &assertions[right_index])
                    .map_err(identifier_projection_error)?;
            let overlapping_unequal = comparison.relation != GeoPropertySetRelation::SameCollateral
                && (!comparison.shared_parcel_ids.is_empty()
                    || !comparison.shared_building_ids.is_empty());
            comparisons.push(GeoPropertyAssertionComparison {
                comparison_id: property_comparison_id(
                    &comparison.left_property_id,
                    &comparison.right_property_id,
                ),
                comparison,
                overlapping_unequal,
            });
        }
    }
    comparisons.sort_by(|left, right| left.comparison_id.cmp(&right.comparison_id));
    Ok(comparisons)
}

fn asserted_member_tile_ids(assertion: &GeoPropertyDocumentAssertionRequest) -> Vec<String> {
    assertion
        .members
        .iter()
        .filter(|member| member.status == GeoPropertyMembershipStatus::AssertedMember)
        .map(|member| member.tile_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn document_key(assertion: &GeoPropertyDocumentAssertionRequest) -> GeoPropertyDocumentKey {
    GeoPropertyDocumentKey {
        document_id: assertion.document_id.clone(),
        accession: assertion.accession.clone(),
        deal_id: assertion.deal_id.clone(),
        loan_id: assertion.loan_id.clone(),
    }
}

fn document_request_key(
    assertion: &GeoPropertyDocumentAssertionRequest,
) -> (String, String, String) {
    (
        assertion.document_id.clone(),
        assertion.accession.clone(),
        assertion.loan_id.clone(),
    )
}

fn validate_registry_proposal(
    proposal: &GeoRegistryMintProposal,
) -> Result<(), GeoPropertyAssertionError> {
    if proposal.version != CANON_GEO_REGISTRY_PROPOSAL_VERSION {
        return Err(GeoPropertyAssertionError::new(
            GeoPropertyAssertionErrorCode::UnsupportedVersion,
            "Unsupported Geo registry proposal version",
            [
                ("actual", proposal.version.as_str()),
                ("expected", CANON_GEO_REGISTRY_PROPOSAL_VERSION),
            ],
        ));
    }
    validate_blake3_uri(
        "registry_proposal.source_ledger_blake3",
        &proposal.source_ledger_blake3,
    )?;
    if proposal.summary.entries != proposal.entries.len() as u64
        || proposal.summary.property_assertions != proposal.property_assertions.len() as u64
    {
        return Err(GeoPropertyAssertionError::invalid(
            "Geo registry proposal summary must match its sections",
            [("field", "registry_proposal.summary")],
        ));
    }
    Ok(())
}

fn validate_summary(
    artifact: &GeoPropertyAssertionArtifact,
) -> Result<(), GeoPropertyAssertionError> {
    let summary = &artifact.summary;
    if summary.document_assertions != artifact.property_assertions.len() as u64
        || summary.property_entities != artifact.property_assertions.len() as u64
        || summary.collateral_memberships != artifact.collateral_memberships.len() as u64
        || summary.membership_edges != artifact.member_relations.len() as u64
        || summary.membership_abstentions != artifact.membership_abstentions.len() as u64
        || summary.pairwise_comparisons != artifact.assertion_comparisons.len() as u64
        || summary.registry_entries != artifact.registry_proposal.entries.len() as u64
    {
        return Err(GeoPropertyAssertionError::invalid(
            "Geo property assertion summary must match artifact sections",
            [("field", "summary")],
        ));
    }
    let tile_spanning = artifact
        .property_assertions
        .iter()
        .filter(|assertion| assertion.tile_spanning)
        .count() as u64;
    let overlapping_unequal = artifact
        .assertion_comparisons
        .iter()
        .filter(|comparison| comparison.overlapping_unequal)
        .count() as u64;
    if summary.tile_spanning_properties != tile_spanning
        || summary.overlapping_unequal_assertions != overlapping_unequal
    {
        return Err(GeoPropertyAssertionError::invalid(
            "Geo property assertion summary derived counters are stale",
            [("field", "summary")],
        ));
    }
    Ok(())
}

fn validate_property_entities(
    assertions: &[GeoPropertyDocumentEntity],
) -> Result<(), GeoPropertyAssertionError> {
    let mut previous: Option<(&str, &str)> = None;
    for assertion in assertions {
        validate_property_id("property_assertions[].property_id", &assertion.property_id)?;
        validate_identifier(
            "property_assertions[].document_alias",
            &assertion.document_alias,
        )?;
        validate_document_key(&assertion.document_key)?;
        validate_identifier(
            "property_assertions[].collateral_set_id",
            &assertion.collateral_set_id,
        )?;
        validate_source_record(
            "property_assertions[].source_record",
            &assertion.source_record,
        )?;
        validate_stable_ids(
            "property_assertions[].parcel_ids",
            GeoEntityLevel::Parcel,
            &assertion.parcel_ids,
        )?;
        validate_stable_ids(
            "property_assertions[].building_ids",
            GeoEntityLevel::Building,
            &assertion.building_ids,
        )?;
        validate_string_vec("property_assertions[].tile_ids", &assertion.tile_ids)?;
        if assertion.tile_spanning != (assertion.tile_ids.len() > 1) {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo property tile_spanning must derive from distinct asserted member tiles",
                [("property_id", assertion.property_id.as_str())],
            ));
        }
        let key = (
            assertion.property_id.as_str(),
            assertion.document_alias.as_str(),
        );
        if let Some(prior) = previous
            && prior >= key
        {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo property assertions must be strictly sorted",
                [("property_id", assertion.property_id.as_str())],
            ));
        }
        previous = Some(key);
    }
    Ok(())
}

fn validate_collateral_memberships(
    memberships: &[GeoPropertyCollateralMembership],
    assertions: &[GeoPropertyDocumentEntity],
) -> Result<(), GeoPropertyAssertionError> {
    let property_ids = assertions
        .iter()
        .map(|assertion| assertion.property_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut previous: Option<(&str, &str)> = None;
    for membership in memberships {
        validate_identifier(
            "collateral_memberships[].collateral_set_id",
            &membership.collateral_set_id,
        )?;
        validate_entity_ref("collateral_memberships[].property", &membership.property)?;
        if membership.property.level != GeoEntityLevel::Property {
            return Err(GeoPropertyAssertionError::invalid(
                "Collateral membership property ref must be a property entity",
                [("property_id", membership.property.id.as_str())],
            ));
        }
        if !property_ids.contains(membership.property.id.as_str()) {
            return Err(GeoPropertyAssertionError::invalid(
                "Collateral membership references an unknown property assertion",
                [("property_id", membership.property.id.as_str())],
            ));
        }
        validate_document_key(&membership.document_key)?;
        validate_source_record(
            "collateral_memberships[].source_record",
            &membership.source_record,
        )?;
        let key = (
            membership.collateral_set_id.as_str(),
            membership.property.id.as_str(),
        );
        if let Some(prior) = previous
            && prior >= key
        {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo collateral memberships must be strictly sorted",
                [("property_id", membership.property.id.as_str())],
            ));
        }
        previous = Some(key);
    }
    Ok(())
}

fn validate_member_relations(
    relations: &[GeoPropertyMemberRelation],
    assertions: &[GeoPropertyDocumentEntity],
) -> Result<(), GeoPropertyAssertionError> {
    let property_ids = assertions
        .iter()
        .map(|assertion| assertion.property_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut previous: Option<&str> = None;
    for relation in relations {
        validate_identifier("member_relations[].relation_id", &relation.relation_id)?;
        validate_entity_ref("member_relations[].property", &relation.property)?;
        if relation.property.level != GeoEntityLevel::Property
            || !property_ids.contains(relation.property.id.as_str())
        {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo property member relation references an unknown property",
                [("property_id", relation.property.id.as_str())],
            ));
        }
        if relation.relation != GeoControlRelation::Contains {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo property member relations must use the contains relation",
                [("relation", format!("{:?}", relation.relation))],
            ));
        }
        validate_entity_ref("member_relations[].member", &relation.member)?;
        validate_identifier(
            "member_relations[].collateral_set_id",
            &relation.collateral_set_id,
        )?;
        validate_identifier("member_relations[].tile_id", &relation.tile_id)?;
        validate_source_record("member_relations[].source_record", &relation.source_record)?;
        if let Some(prior) = previous
            && prior >= relation.relation_id.as_str()
        {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo property member relations must be strictly sorted",
                [("relation_id", relation.relation_id.as_str())],
            ));
        }
        previous = Some(relation.relation_id.as_str());
    }
    Ok(())
}

fn validate_membership_abstentions(
    abstentions: &[GeoPropertyMembershipAbstention],
    assertions: &[GeoPropertyDocumentEntity],
) -> Result<(), GeoPropertyAssertionError> {
    let property_ids = assertions
        .iter()
        .map(|assertion| assertion.property_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut previous: Option<&str> = None;
    for abstention in abstentions {
        validate_identifier(
            "membership_abstentions[].abstention_id",
            &abstention.abstention_id,
        )?;
        validate_property_id(
            "membership_abstentions[].property_id",
            &abstention.property_id,
        )?;
        if !property_ids.contains(abstention.property_id.as_str()) {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo property membership abstention references an unknown property",
                [("property_id", abstention.property_id.as_str())],
            ));
        }
        validate_identifier(
            "membership_abstentions[].collateral_set_id",
            &abstention.collateral_set_id,
        )?;
        validate_entity_ref(
            "membership_abstentions[].candidate_member",
            &abstention.candidate_member,
        )?;
        validate_identifier("membership_abstentions[].tile_id", &abstention.tile_id)?;
        validate_source_record(
            "membership_abstentions[].source_record",
            &abstention.source_record,
        )?;
        if let Some(prior) = previous
            && prior >= abstention.abstention_id.as_str()
        {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo property membership abstentions must be strictly sorted",
                [("abstention_id", abstention.abstention_id.as_str())],
            ));
        }
        previous = Some(abstention.abstention_id.as_str());
    }
    Ok(())
}

fn validate_assertion_comparisons(
    comparisons: &[GeoPropertyAssertionComparison],
    registry_assertions: &[GeoPropertyDocumentAssertion],
) -> Result<(), GeoPropertyAssertionError> {
    let property_ids = registry_assertions
        .iter()
        .map(|assertion| assertion.property_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut previous: Option<&str> = None;
    for comparison in comparisons {
        validate_identifier(
            "assertion_comparisons[].comparison_id",
            &comparison.comparison_id,
        )?;
        if !property_ids.contains(comparison.comparison.left_property_id.as_str())
            || !property_ids.contains(comparison.comparison.right_property_id.as_str())
        {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo property assertion comparison references an unknown property",
                [("comparison_id", comparison.comparison_id.as_str())],
            ));
        }
        let expected = comparison.comparison.relation != GeoPropertySetRelation::SameCollateral
            && (!comparison.comparison.shared_parcel_ids.is_empty()
                || !comparison.comparison.shared_building_ids.is_empty());
        if comparison.overlapping_unequal != expected {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo property assertion comparison overlapping_unequal is stale",
                [("comparison_id", comparison.comparison_id.as_str())],
            ));
        }
        if let Some(prior) = previous
            && prior >= comparison.comparison_id.as_str()
        {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo property assertion comparisons must be strictly sorted",
                [("comparison_id", comparison.comparison_id.as_str())],
            ));
        }
        previous = Some(comparison.comparison_id.as_str());
    }
    Ok(())
}

fn validate_source_corpus(
    source: &GeoPropertySourceCorpus,
) -> Result<(), GeoPropertyAssertionError> {
    validate_identifier("source_corpus.corpus_id", &source.corpus_id)?;
    validate_identifier("source_corpus.corpus_version", &source.corpus_version)?;
    validate_identifier("source_corpus.temporal_scope", &source.temporal_scope)?;
    validate_string_vec("source_corpus.native_key_fields", &source.native_key_fields)?;
    Ok(())
}

fn validate_document_key(key: &GeoPropertyDocumentKey) -> Result<(), GeoPropertyAssertionError> {
    validate_identifier("document_key.document_id", &key.document_id)?;
    validate_identifier("document_key.accession", &key.accession)?;
    validate_identifier("document_key.deal_id", &key.deal_id)?;
    validate_identifier("document_key.loan_id", &key.loan_id)?;
    Ok(())
}

fn validate_source_record(
    field: &'static str,
    record: &GeoPropertySourceRecordRef,
) -> Result<(), GeoPropertyAssertionError> {
    validate_identifier(
        &format!("{field}.source_record_id"),
        &record.source_record_id,
    )?;
    validate_identifier(&format!("{field}.source_vintage"), &record.source_vintage)?;
    validate_blake3_uri(&format!("{field}.record_blake3"), &record.record_blake3)?;
    Ok(())
}

fn validate_entity_ref(
    field: &'static str,
    entity: &GeoEntityRef,
) -> Result<(), GeoPropertyAssertionError> {
    validate_identifier(&format!("{field}.id"), &entity.id)?;
    match entity.level {
        GeoEntityLevel::Parcel => validate_prefixed_id(&entity.id, "cmdrvl:parcel:", field),
        GeoEntityLevel::Building => validate_prefixed_id(&entity.id, "cmdrvl:building:", field),
        GeoEntityLevel::Property => validate_prefixed_id(&entity.id, "cmdrvl:property:", field),
        GeoEntityLevel::PoiUnit => Err(GeoPropertyAssertionError::invalid(
            "Geo property assertions do not support poi_unit members",
            [("field", field), ("member_id", entity.id.as_str())],
        )),
    }
}

fn validate_stable_ids(
    field: &'static str,
    level: GeoEntityLevel,
    ids: &[String],
) -> Result<(), GeoPropertyAssertionError> {
    let prefix = match level {
        GeoEntityLevel::Parcel => "cmdrvl:parcel:",
        GeoEntityLevel::Building => "cmdrvl:building:",
        GeoEntityLevel::Property => "cmdrvl:property:",
        GeoEntityLevel::PoiUnit => {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo property stable id validation does not support poi_unit",
                [("field", field)],
            ));
        }
    };
    let mut previous: Option<&str> = None;
    for id in ids {
        validate_prefixed_id(id, prefix, field)?;
        if let Some(prior) = previous
            && prior >= id.as_str()
        {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo property member ids must be strictly sorted and unique",
                [("field", field), ("id", id.as_str())],
            ));
        }
        previous = Some(id.as_str());
    }
    Ok(())
}

fn validate_prefixed_id(
    value: &str,
    prefix: &'static str,
    field: &'static str,
) -> Result<(), GeoPropertyAssertionError> {
    validate_identifier(field, value)?;
    if !value.starts_with(prefix) {
        return Err(GeoPropertyAssertionError::invalid(
            "Geo property assertion ids must use the expected cmdrvl entity namespace",
            [
                ("field", field),
                ("expected_prefix", prefix),
                ("value", value),
            ],
        ));
    }
    Ok(())
}

fn validate_property_id(field: &'static str, value: &str) -> Result<(), GeoPropertyAssertionError> {
    validate_prefixed_id(value, "cmdrvl:property:", field)
}

fn validate_string_vec(
    field: &'static str,
    values: &[String],
) -> Result<(), GeoPropertyAssertionError> {
    let mut previous: Option<&str> = None;
    for value in values {
        validate_identifier(field, value)?;
        if let Some(prior) = previous
            && prior >= value.as_str()
        {
            return Err(GeoPropertyAssertionError::invalid(
                "Geo property string vectors must be strictly sorted and unique",
                [("field", field), ("value", value.as_str())],
            ));
        }
        previous = Some(value.as_str());
    }
    Ok(())
}

fn validate_identifier(field: &str, value: &str) -> Result<(), GeoPropertyAssertionError> {
    if value.is_empty() || value.trim_matches(|ch: char| ch.is_ascii_whitespace()) != value {
        return Err(GeoPropertyAssertionError::invalid(
            "Geo property identifiers must be non-empty and ASCII-trimmed",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_blake3_uri(field: &str, value: &str) -> Result<(), GeoPropertyAssertionError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(GeoPropertyAssertionError::invalid(
            "Geo property source digests must be blake3 URIs",
            [("field", field), ("value", value)],
        ));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(GeoPropertyAssertionError::invalid(
            "Geo property source digests must be blake3 URIs",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn pairwise_count(count: usize) -> Result<usize, GeoPropertyAssertionError> {
    count.checked_sub(1).map_or(Ok(0), |prior| {
        count
            .checked_mul(prior)
            .and_then(|product| product.checked_div(2))
            .ok_or_else(arithmetic_overflow)
    })
}

fn usize_to_u64(value: usize, field: &'static str) -> Result<u64, GeoPropertyAssertionError> {
    u64::try_from(value).map_err(|_| {
        GeoPropertyAssertionError::new(
            GeoPropertyAssertionErrorCode::ArithmeticOverflow,
            "Geo property assertion count exceeded u64",
            [
                ("field".to_string(), field.to_string()),
                ("value".to_string(), value.to_string()),
            ],
        )
    })
}

fn arithmetic_overflow() -> GeoPropertyAssertionError {
    GeoPropertyAssertionError::new(
        GeoPropertyAssertionErrorCode::ArithmeticOverflow,
        "Geo property assertion arithmetic overflow",
        std::iter::empty::<(&str, &str)>(),
    )
}

fn member_relation_id(property_id: &str, collateral_set_id: &str, member: &GeoEntityRef) -> String {
    let seed = format!(
        "{CANON_GEO_PROPERTY_ASSERTION_VERSION}\0member\0{property_id}\0{collateral_set_id}\0{:?}\0{}",
        member.level, member.id
    );
    format!(
        "geo-property-member:{}",
        blake3::hash(seed.as_bytes()).to_hex()
    )
}

fn membership_abstention_id(
    property_id: &str,
    collateral_set_id: &str,
    member: &GeoEntityRef,
    reason: GeoPropertyMembershipAbstentionReason,
) -> String {
    let seed = format!(
        "{CANON_GEO_PROPERTY_ASSERTION_VERSION}\0abstention\0{property_id}\0{collateral_set_id}\0{:?}\0{}\0{:?}",
        member.level, member.id, reason
    );
    format!(
        "geo-property-membership-abstention:{}",
        blake3::hash(seed.as_bytes()).to_hex()
    )
}

fn property_comparison_id(left_property_id: &str, right_property_id: &str) -> String {
    let seed = format!(
        "{CANON_GEO_PROPERTY_ASSERTION_VERSION}\0comparison\0{left_property_id}\0{right_property_id}"
    );
    format!(
        "geo-property-comparison:{}",
        blake3::hash(seed.as_bytes()).to_hex()
    )
}

fn identifier_projection_error(error: GeoIdentifierError) -> GeoPropertyAssertionError {
    GeoPropertyAssertionError::new(
        GeoPropertyAssertionErrorCode::IdentifierProjection,
        "Geo property assertion could not project stable identifier proposal",
        [
            ("code", format!("{:?}", error.code)),
            ("message", error.message),
        ],
    )
}

fn tile_local_blocking_refusal() -> GeoPropertyAssertionError {
    GeoPropertyAssertionError::new(
        GeoPropertyAssertionErrorCode::TileLocalBlockingRefused,
        "Geo property assertions are document-scoped and cannot use tile-local blocking as the primary strategy",
        [("required_strategy", "document_first_then_geography")],
    )
}

fn relation_graph_projection_required() -> GeoPropertyAssertionError {
    GeoPropertyAssertionError::new(
        GeoPropertyAssertionErrorCode::RelationGraphProjectionRequired,
        "Geo property membership must be published as a derived relation projection",
        [("required_product", "published_derived_projection")],
    )
}

fn level_name(level: GeoEntityLevel) -> &'static str {
    match level {
        GeoEntityLevel::PoiUnit => "poi_unit",
        GeoEntityLevel::Building => "building",
        GeoEntityLevel::Parcel => "parcel",
        GeoEntityLevel::Property => "property",
    }
}
