#![forbid(unsafe_code)]

//! Workbench-side pre-resolution artifacts for owned property corpora.
//!
//! These artifacts are registry proposals, not runtime lookup behavior. The
//! ordinary Canon path remains exact replay against reviewed registry entries.

use super::{
    CANON_GEO_REGIONAL_INVENTORY_VERSION, CANON_GEO_REGISTRY_PROPOSAL_VERSION, GeoBoundedGeography,
    GeoClaimClass, GeoControlEntityLevel, GeoEgressClass, GeoEvidenceClass,
    GeoIdentityParticipation, GeoLedgerIdentifierRow, GeoLicenseClass, GeoLocalAcquisitionState,
    GeoNativeEntityScope, GeoRegionalInventory, GeoRegionalSourceInstance, GeoRegistryMintProposal,
    GeoRegistryProposalEntry, GeoSourceAvailability, registry_proposal_from_ledger_rows,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_PRE_RESOLUTION_VERSION: &str = "canon_geo_pre_resolution.v0";
pub const CANON_GEO_NAME_REGION_RESOLUTION_VERSION: &str = "canon_geo_name_region_resolution.v0";
pub const GEO_PRE_RESOLUTION_CMBS_ADDRESS_RULE_ID: &str =
    "geo_pre_resolution.cmbs_annex_a_address.v1";
pub const GEO_NAME_REGION_ENTITY_REUSE_PROFILE_ID: &str =
    "geo_name_region.entity_operator_reuse.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPreResolutionRequest {
    pub version: String,
    pub source_corpus: GeoPreResolutionSourceCorpus,
    pub proof_class: GeoPreResolutionProofClass,
    pub build_receipts: Vec<GeoPreResolutionBuildReceipt>,
    pub rows: Vec<GeoPreResolutionSourceRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPreResolutionArtifact {
    pub version: String,
    pub pre_resolution_id: String,
    pub source_corpus: GeoPreResolutionSourceCorpus,
    pub proof_class: GeoPreResolutionProofClass,
    pub build_receipts: Vec<GeoPreResolutionBuildReceipt>,
    pub denominators: GeoPreResolutionDenominators,
    pub registry_proposal: GeoRegistryMintProposal,
    pub stage1_exact_aliases: Vec<GeoPreResolutionExactAlias>,
    pub abstained_rows: Vec<GeoPreResolutionRowDisposition>,
    pub unresolvable_rows: Vec<GeoPreResolutionRowDisposition>,
    pub review_status: GeoPreResolutionReviewStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPreResolutionSourceCorpus {
    pub corpus_id: String,
    pub corpus_kind: GeoPreResolutionCorpusKind,
    pub corpus_version: String,
    pub temporal_scope: String,
    pub native_key_fields: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPreResolutionCorpusKind {
    CmbsAnnexA,
    HudFha,
    FannieFreddie,
    GinniePoolNoAddress,
    ReitScheduleIiiNameOnly,
    NaicDescriptor,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPreResolutionCorpusCapability {
    AddressAssertions,
    NoAddressField,
    NameOnly,
    DescriptorOnly,
    UnsupportedFirstSlice,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPreResolutionProofClass {
    LiveQuery,
    RetainedArtifact,
    Fixture,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNameRegionResolutionRequest {
    pub version: String,
    pub source_corpus: GeoPreResolutionSourceCorpus,
    pub proof_class: GeoPreResolutionProofClass,
    pub build_receipts: Vec<GeoPreResolutionBuildReceipt>,
    pub region: GeoBoundedGeography,
    pub requested_entity_level: GeoControlEntityLevel,
    pub requested_claim_classes: Vec<GeoClaimClass>,
    pub source_pins: Vec<GeoNameRegionSourcePin>,
    pub region_cell_coverage: GeoNameRegionCellCoverage,
    pub subject: GeoNameRegionSubject,
    pub rarity_policy: GeoNameRegionRarityPolicy,
    pub candidates: Vec<GeoNameRegionCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNameRegionSubject {
    pub row_id: String,
    pub source_record_id: String,
    pub source_record_blake3: String,
    pub asserted_name: String,
    pub asserted_region: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asserted_property_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asserted_year_built: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoNameRegionSourceRole {
    RegionCellCoverage,
    NameBearingEntity,
    DescentRelation,
    AttributeConfirmation,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNameRegionSourcePin {
    pub source_instance_id: String,
    pub role: GeoNameRegionSourceRole,
    pub release_id: String,
    pub release_digest: String,
    pub local_artifact_id: String,
    pub local_content_hash: String,
    pub license_class: GeoLicenseClass,
    pub egress_class: GeoEgressClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoNameRegionCoverageBasis {
    AdministrativeBoundaryCells,
    ZipBoundaryCells,
    OperatorDeclaredCells,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNameRegionCellCoverage {
    pub coverage_id: String,
    pub source_instance_id: String,
    pub source_record_id: String,
    pub source_record_blake3: String,
    pub basis: GeoNameRegionCoverageBasis,
    pub h3_cells: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNameRegionRarityPolicy {
    pub policy_id: String,
    pub max_name_matches_for_resolution: u64,
    pub min_name_score_basis_points: u32,
    pub chain_review_min_matches: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoNameRegionNameOperator {
    Namekit,
    TfidfCosine,
    AliasPatchMatch,
    Rapidfuzz,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNameRegionNameEvidence {
    pub operator: GeoNameRegionNameOperator,
    pub operator_id: String,
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_hash: Option<String>,
    pub asserted_surface: String,
    pub candidate_surface: String,
    pub score_basis_points: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoNameRegionAttributeField {
    PropertyType,
    YearBuilt,
    BuildingSize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNameRegionAttributeEvidence {
    pub field: GeoNameRegionAttributeField,
    pub source_instance_id: String,
    pub source_record_id: String,
    pub source_record_blake3: String,
    pub asserted_value: String,
    pub candidate_value: String,
    pub hard_filter_passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNameRegionDescentEdge {
    pub source_instance_id: String,
    pub source_record_id: String,
    pub source_record_blake3: String,
    pub from_level: GeoControlEntityLevel,
    pub from_id: String,
    pub to_level: GeoControlEntityLevel,
    pub to_id: String,
    pub relation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNameRegionCandidate {
    pub candidate_id: String,
    pub entity_level: GeoControlEntityLevel,
    pub source_instance_id: String,
    pub source_record_id: String,
    pub source_record_blake3: String,
    pub display_name: String,
    pub region_cell_ids: Vec<String>,
    pub name_evidence: Vec<GeoNameRegionNameEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attribute_evidence: Vec<GeoNameRegionAttributeEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub descent_edges: Vec<GeoNameRegionDescentEdge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoNameRegionResolutionStatus {
    Resolved,
    Abstained,
    Refused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoNameRegionResolutionReason {
    UniqueNameRegionMatchDescendedToSupportedGrain,
    NoNameMatchInRegion,
    NonDiscriminatingNameInRegion,
    AmbiguousNameMatchesInRegion,
    AttributeDisagreementAtConfirmStep,
    MissingRegionBoundary,
    MissingPinnedNameBearingSource,
    MissingSourceLicensePins,
    NoDescentPathToSupportedGrain,
    ParcelGrainRequestedWithoutParcelInventory,
    UnsupportedRequestedGrain,
    UnsupportedCorpusKind,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNameRegionRarityReport {
    pub policy_id: String,
    pub region_candidate_count: u64,
    pub name_matched_candidate_count: u64,
    pub max_name_matches_for_resolution: u64,
    pub min_name_score_basis_points: u32,
    pub chain_review_min_matches: u64,
    pub discriminating_in_region: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNameRegionCandidateReport {
    pub candidate_id: String,
    pub entity_level: GeoControlEntityLevel,
    pub display_name: String,
    pub source_instance_id: String,
    pub in_region: bool,
    pub name_matched: bool,
    pub best_name_score_basis_points: u32,
    pub region_cell_overlap_count: u64,
    pub hard_attribute_failures: Vec<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNameRegionResolvedSet {
    pub entity_level: GeoControlEntityLevel,
    pub entity_ids: Vec<String>,
    pub source_candidate_ids: Vec<String>,
    pub representation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNameRegionRefusal {
    pub reason: GeoNameRegionResolutionReason,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_entity_level: Option<GeoControlEntityLevel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNameRegionSummary {
    pub input_rows: u64,
    pub resolved_rows: u64,
    pub abstained_rows: u64,
    pub refused_rows: u64,
    pub region_cells: u64,
    pub region_candidates: u64,
    pub name_matched_candidates: u64,
    pub selected_candidates: u64,
    pub resolved_entity_sets: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNameRegionResolutionArtifact {
    pub version: String,
    pub resolution_id: String,
    pub source_corpus: GeoPreResolutionSourceCorpus,
    pub proof_class: GeoPreResolutionProofClass,
    pub build_receipts: Vec<GeoPreResolutionBuildReceipt>,
    pub release_claim_allowed: bool,
    pub region: GeoBoundedGeography,
    pub requested_entity_level: GeoControlEntityLevel,
    pub requested_claim_classes: Vec<GeoClaimClass>,
    pub source_pins: Vec<GeoNameRegionSourcePin>,
    pub region_cell_coverage: GeoNameRegionCellCoverage,
    pub subject: GeoNameRegionSubject,
    pub status: GeoNameRegionResolutionStatus,
    pub reason: GeoNameRegionResolutionReason,
    pub regional_rarity: GeoNameRegionRarityReport,
    pub candidates: Vec<GeoNameRegionCandidateReport>,
    pub selected_sets: Vec<GeoNameRegionResolvedSet>,
    pub refusals: Vec<GeoNameRegionRefusal>,
    pub summary: GeoNameRegionSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPreResolutionBuildReceipt {
    pub receipt_id: String,
    pub query_id: String,
    pub source_artifact_blake3: String,
    pub row_count: u64,
    pub run_status: GeoPreResolutionRunStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPreResolutionRunStatus {
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPreResolutionSourceRow {
    pub row_id: String,
    pub source_record_id: String,
    pub accession: String,
    pub deal_id: String,
    pub loan_id: String,
    pub source_record_blake3: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asserted_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reach: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reach_none_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parcel_set: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub building_set: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPreResolutionDenominators {
    pub total_source_rows: u64,
    pub resolved_rows: u64,
    pub abstained_rows: u64,
    pub unresolvable_rows: u64,
    pub stage1_exact_aliases: u64,
    pub registry_entries: u64,
    pub property_assertions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPreResolutionExactAlias {
    pub alias: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
    pub source_row_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPreResolutionRowDisposition {
    pub row_id: String,
    pub source_record_id: String,
    pub reason: GeoPreResolutionDispositionReason,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPreResolutionDispositionReason {
    ReachNone,
    MissingAddress,
    AmbiguousExactAddress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPreResolutionReviewStatus {
    pub state: GeoPreResolutionReviewState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_receipt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promoted_registry_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPreResolutionReviewState {
    ReviewPending,
    Accepted,
    Rejected,
    Promoted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPreResolutionErrorCode {
    UnsupportedVersion,
    UnsupportedCorpusKind,
    InvalidInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPreResolutionError {
    pub code: GeoPreResolutionErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoPreResolutionError {
    fn new(
        code: GeoPreResolutionErrorCode,
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
        Self::new(GeoPreResolutionErrorCode::InvalidInput, message, detail)
    }
}

impl fmt::Display for GeoPreResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoPreResolutionError {}

pub fn pre_resolution_corpus_capability(
    kind: GeoPreResolutionCorpusKind,
) -> GeoPreResolutionCorpusCapability {
    match kind {
        GeoPreResolutionCorpusKind::CmbsAnnexA => {
            GeoPreResolutionCorpusCapability::AddressAssertions
        }
        GeoPreResolutionCorpusKind::HudFha | GeoPreResolutionCorpusKind::FannieFreddie => {
            GeoPreResolutionCorpusCapability::UnsupportedFirstSlice
        }
        GeoPreResolutionCorpusKind::GinniePoolNoAddress => {
            GeoPreResolutionCorpusCapability::NoAddressField
        }
        GeoPreResolutionCorpusKind::ReitScheduleIiiNameOnly => {
            GeoPreResolutionCorpusCapability::NameOnly
        }
        GeoPreResolutionCorpusKind::NaicDescriptor => {
            GeoPreResolutionCorpusCapability::DescriptorOnly
        }
        GeoPreResolutionCorpusKind::Unknown => GeoPreResolutionCorpusCapability::Unknown,
    }
}

pub fn materialize_pre_resolution(
    request: &GeoPreResolutionRequest,
) -> Result<GeoPreResolutionArtifact, GeoPreResolutionError> {
    validate_pre_resolution_request(request)?;

    let mut rows = request.rows.clone();
    rows.sort_by(|left, right| left.row_id.cmp(&right.row_id));

    let duplicate_addresses = duplicate_exact_addresses(&rows);
    let mut ledger_rows = Vec::new();
    let mut resolved_source_row_ids = BTreeMap::<(String, String), String>::new();
    let mut abstained_rows = Vec::new();
    let mut unresolvable_rows = Vec::new();

    for row in &rows {
        let reach_none = row
            .reach
            .as_deref()
            .is_some_and(|reach| reach.eq_ignore_ascii_case("none"));
        if reach_none {
            if row.reach_none_reason.as_deref().is_none_or(str::is_empty) {
                return Err(GeoPreResolutionError::invalid(
                    "Pre-resolution reach-none rows must carry a reason",
                    [
                        ("field", "rows[].reach_none_reason"),
                        ("row_id", row.row_id.as_str()),
                    ],
                ));
            }
            if !row.parcel_set.is_empty() || !row.building_set.is_empty() {
                return Err(GeoPreResolutionError::invalid(
                    "Pre-resolution reach-none rows must not fabricate identifier sets",
                    [
                        ("field", "rows[].parcel_set_or_building_set"),
                        ("row_id", row.row_id.as_str()),
                    ],
                ));
            }
            unresolvable_rows.push(row_disposition(
                row,
                GeoPreResolutionDispositionReason::ReachNone,
                row.reach_none_reason.as_deref().unwrap_or("reach_none"),
            ));
            continue;
        }

        if row.parcel_set.is_empty() && row.building_set.is_empty() {
            return Err(GeoPreResolutionError::invalid(
                "Pre-resolution rows need at least one stable member unless reach is none",
                [
                    ("field", "rows[].parcel_set_or_building_set"),
                    ("row_id", row.row_id.as_str()),
                ],
            ));
        }

        let Some(address) = row.asserted_address.as_deref() else {
            abstained_rows.push(row_disposition(
                row,
                GeoPreResolutionDispositionReason::MissingAddress,
                "no exact address alias can be promoted",
            ));
            continue;
        };

        if duplicate_addresses.contains(address) {
            abstained_rows.push(row_disposition(
                row,
                GeoPreResolutionDispositionReason::AmbiguousExactAddress,
                "exact address appears on more than one source row in the slice",
            ));
            continue;
        }

        ledger_rows.push(GeoLedgerIdentifierRow {
            accession: row.accession.clone(),
            deal_id: row.deal_id.clone(),
            loan_id: row.loan_id.clone(),
            reach: row.reach.clone(),
            reach_none_reason: row.reach_none_reason.clone(),
            parcel_set: Some(row.parcel_set.clone()),
            building_set: Some(row.building_set.clone()),
        });
        resolved_source_row_ids.insert(
            (row.accession.clone(), row.loan_id.clone()),
            row.row_id.clone(),
        );
    }

    let source_ledger_bytes = canonical_ledger_seed_bytes(&ledger_rows)?;
    let mut registry_proposal =
        registry_proposal_from_ledger_rows(&source_ledger_bytes, &ledger_rows).map_err(
            |error| {
                GeoPreResolutionError::invalid(
                    "Pre-resolution rows could not be converted into a Geo registry proposal",
                    [
                        ("code", format!("{:?}", error.code)),
                        ("message", error.message),
                    ],
                )
            },
        )?;

    sort_registry_proposal(&mut registry_proposal);
    let stage1_exact_aliases =
        stage1_aliases_for_rows(&rows, &resolved_source_row_ids, &registry_proposal)?;
    append_stage1_alias_entries(&mut registry_proposal, &stage1_exact_aliases)?;
    sort_registry_proposal(&mut registry_proposal);

    let denominators = GeoPreResolutionDenominators {
        total_source_rows: rows.len() as u64,
        resolved_rows: stage1_exact_aliases.len() as u64,
        abstained_rows: abstained_rows.len() as u64,
        unresolvable_rows: unresolvable_rows.len() as u64,
        stage1_exact_aliases: stage1_exact_aliases.len() as u64,
        registry_entries: registry_proposal.entries.len() as u64,
        property_assertions: registry_proposal.property_assertions.len() as u64,
    };

    let mut artifact = GeoPreResolutionArtifact {
        version: CANON_GEO_PRE_RESOLUTION_VERSION.to_string(),
        pre_resolution_id: String::new(),
        source_corpus: request.source_corpus.clone(),
        proof_class: request.proof_class,
        build_receipts: request.build_receipts.clone(),
        denominators,
        registry_proposal,
        stage1_exact_aliases,
        abstained_rows,
        unresolvable_rows,
        review_status: GeoPreResolutionReviewStatus {
            state: GeoPreResolutionReviewState::ReviewPending,
            review_receipt_id: None,
            promoted_registry_version: None,
        },
    };
    artifact.pre_resolution_id = pre_resolution_id(&artifact)?;
    validate_pre_resolution_artifact(&artifact)?;
    Ok(artifact)
}

pub fn materialize_name_region_resolution(
    request: &GeoNameRegionResolutionRequest,
    inventory: &GeoRegionalInventory,
) -> Result<GeoNameRegionResolutionArtifact, GeoPreResolutionError> {
    validate_name_region_request_shell(request)?;

    let source_pins = canonical_name_region_source_pins(&request.source_pins)?;
    let region_cell_coverage = canonical_name_region_cell_coverage(&request.region_cell_coverage)?;
    let candidates = canonical_name_region_candidates(&request.candidates)?;
    let requested_claim_classes = canonical_geo_claim_classes(&request.requested_claim_classes)?;
    let candidate_reports = name_region_candidate_reports(
        &candidates,
        &region_cell_coverage,
        request.rarity_policy.min_name_score_basis_points,
        &BTreeSet::new(),
    )?;
    let region_candidates = candidate_reports
        .iter()
        .filter(|candidate| candidate.in_region)
        .count() as u64;
    let name_matched_candidates = candidate_reports
        .iter()
        .filter(|candidate| candidate.name_matched)
        .count() as u64;
    let rarity = GeoNameRegionRarityReport {
        policy_id: request.rarity_policy.policy_id.clone(),
        region_candidate_count: region_candidates,
        name_matched_candidate_count: name_matched_candidates,
        max_name_matches_for_resolution: request.rarity_policy.max_name_matches_for_resolution,
        min_name_score_basis_points: request.rarity_policy.min_name_score_basis_points,
        chain_review_min_matches: request.rarity_policy.chain_review_min_matches,
        discriminating_in_region: name_matched_candidates > 0
            && name_matched_candidates <= request.rarity_policy.max_name_matches_for_resolution
            && name_matched_candidates < request.rarity_policy.chain_review_min_matches,
    };

    let decision = name_region_decision(
        request,
        inventory,
        &source_pins,
        &region_cell_coverage,
        &candidates,
        &candidate_reports,
        &rarity,
    );
    let selected_candidate_ids = decision
        .selected_sets
        .iter()
        .flat_map(|set| set.source_candidate_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    let candidate_reports = name_region_candidate_reports(
        &candidates,
        &region_cell_coverage,
        request.rarity_policy.min_name_score_basis_points,
        &selected_candidate_ids,
    )?;
    let resolved_rows = u64::from(decision.status == GeoNameRegionResolutionStatus::Resolved);
    let abstained_rows = u64::from(decision.status == GeoNameRegionResolutionStatus::Abstained);
    let refused_rows = u64::from(decision.status == GeoNameRegionResolutionStatus::Refused);
    let region_cells = region_cell_coverage.h3_cells.len() as u64;
    let selected_candidates = selected_candidate_ids.len() as u64;
    let resolved_entity_sets = decision.selected_sets.len() as u64;

    let mut artifact = GeoNameRegionResolutionArtifact {
        version: CANON_GEO_NAME_REGION_RESOLUTION_VERSION.to_string(),
        resolution_id: String::new(),
        source_corpus: request.source_corpus.clone(),
        proof_class: request.proof_class,
        build_receipts: request.build_receipts.clone(),
        release_claim_allowed: false,
        region: request.region.clone(),
        requested_entity_level: request.requested_entity_level,
        requested_claim_classes,
        source_pins,
        region_cell_coverage,
        subject: request.subject.clone(),
        status: decision.status,
        reason: decision.reason,
        regional_rarity: rarity,
        candidates: candidate_reports,
        selected_sets: decision.selected_sets,
        refusals: decision.refusals,
        summary: GeoNameRegionSummary {
            input_rows: 1,
            resolved_rows,
            abstained_rows,
            refused_rows,
            region_cells,
            region_candidates,
            name_matched_candidates,
            selected_candidates,
            resolved_entity_sets,
        },
    };
    artifact.resolution_id = name_region_resolution_id(&artifact)?;
    validate_name_region_resolution_artifact(&artifact)?;
    Ok(artifact)
}

pub fn validate_name_region_resolution_artifact(
    artifact: &GeoNameRegionResolutionArtifact,
) -> Result<(), GeoPreResolutionError> {
    if artifact.version != CANON_GEO_NAME_REGION_RESOLUTION_VERSION {
        return Err(GeoPreResolutionError::new(
            GeoPreResolutionErrorCode::UnsupportedVersion,
            "Unsupported Geo name-region resolution artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_NAME_REGION_RESOLUTION_VERSION),
            ],
        ));
    }
    validate_string("resolution_id", &artifact.resolution_id)?;
    validate_source_corpus(&artifact.source_corpus)?;
    validate_build_receipts(&artifact.build_receipts)?;
    validate_bounded_geography("region", &artifact.region)?;
    validate_name_region_subject(&artifact.subject)?;
    validate_name_region_source_pins(&artifact.source_pins)?;
    validate_name_region_cell_coverage(&artifact.region_cell_coverage)?;
    validate_geo_claim_classes(&artifact.requested_claim_classes)?;
    validate_name_region_rarity(&artifact.regional_rarity)?;
    validate_name_region_candidate_reports(&artifact.candidates)?;
    validate_name_region_selected_sets(&artifact.selected_sets)?;
    validate_name_region_refusals(&artifact.refusals)?;
    validate_name_region_summary(&artifact.summary, artifact)?;
    if artifact.release_claim_allowed {
        return Err(GeoPreResolutionError::invalid(
            "Geo name-region v0 is a workbench resolution and cannot emit release claims",
            [("field", "release_claim_allowed")],
        ));
    }
    let expected_id = name_region_resolution_id(artifact)?;
    if artifact.resolution_id != expected_id {
        return Err(GeoPreResolutionError::invalid(
            "Geo name-region resolution id must match canonical artifact content",
            [
                ("field", "resolution_id".to_string()),
                ("expected", expected_id),
                ("actual", artifact.resolution_id.clone()),
            ],
        ));
    }
    Ok(())
}

pub fn canonical_name_region_resolution_bytes(
    artifact: &GeoNameRegionResolutionArtifact,
) -> Result<Vec<u8>, GeoPreResolutionError> {
    validate_name_region_resolution_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoPreResolutionError::invalid(
            "Geo name-region resolution artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

#[derive(Debug, Clone)]
struct NameRegionDecision {
    status: GeoNameRegionResolutionStatus,
    reason: GeoNameRegionResolutionReason,
    selected_sets: Vec<GeoNameRegionResolvedSet>,
    refusals: Vec<GeoNameRegionRefusal>,
}

fn name_region_decision(
    request: &GeoNameRegionResolutionRequest,
    inventory: &GeoRegionalInventory,
    source_pins: &[GeoNameRegionSourcePin],
    region_cell_coverage: &GeoNameRegionCellCoverage,
    candidates: &[GeoNameRegionCandidate],
    candidate_reports: &[GeoNameRegionCandidateReport],
    rarity: &GeoNameRegionRarityReport,
) -> NameRegionDecision {
    if !matches!(
        request.source_corpus.corpus_kind,
        GeoPreResolutionCorpusKind::ReitScheduleIiiNameOnly
            | GeoPreResolutionCorpusKind::NaicDescriptor
    ) {
        return stopped(
            GeoNameRegionResolutionStatus::Refused,
            GeoNameRegionResolutionReason::UnsupportedCorpusKind,
            "name-region resolution accepts name-only Schedule III and NAIC descriptor corpora; Ginnie pool records need the FHA identifier-join path",
            Some(request.requested_entity_level),
        );
    }
    if inventory.version != CANON_GEO_REGIONAL_INVENTORY_VERSION
        || inventory.region.geography_id != request.region.geography_id
        || region_cell_coverage.h3_cells.is_empty()
        || !inventory_has_available_source(inventory, &region_cell_coverage.source_instance_id)
    {
        return stopped(
            GeoNameRegionResolutionStatus::Refused,
            GeoNameRegionResolutionReason::MissingRegionBoundary,
            "region must bind to a pinned local administrative-boundary cell coverage source",
            Some(request.requested_entity_level),
        );
    }
    if let Some(reason) =
        missing_name_region_pin_reason(source_pins, region_cell_coverage, candidates, inventory)
    {
        return stopped(
            GeoNameRegionResolutionStatus::Refused,
            GeoNameRegionResolutionReason::MissingSourceLicensePins,
            reason,
            Some(request.requested_entity_level),
        );
    }
    if !has_pinned_name_bearing_source(inventory, source_pins) {
        return stopped(
            GeoNameRegionResolutionStatus::Refused,
            GeoNameRegionResolutionReason::MissingPinnedNameBearingSource,
            "inventory must expose at least one pinned local native source that can carry entity names in the region",
            Some(request.requested_entity_level),
        );
    }
    if !matches!(
        request.requested_entity_level,
        GeoControlEntityLevel::Parcel | GeoControlEntityLevel::Building
    ) {
        return stopped(
            GeoNameRegionResolutionStatus::Refused,
            GeoNameRegionResolutionReason::UnsupportedRequestedGrain,
            "current composition confirmation supports only building and parcel grains; POI/property/site matches must descend before they can claim a supported answer",
            Some(request.requested_entity_level),
        );
    }
    if request.requested_entity_level == GeoControlEntityLevel::Parcel
        && !inventory_has_native_level(inventory, GeoControlEntityLevel::Parcel)
    {
        return stopped(
            GeoNameRegionResolutionStatus::Refused,
            GeoNameRegionResolutionReason::ParcelGrainRequestedWithoutParcelInventory,
            "parcel-grain name-region resolution requires an explicit parcel inventory source; building-only regions may resolve only at building grain",
            Some(GeoControlEntityLevel::Parcel),
        );
    }
    if rarity.name_matched_candidate_count == 0 {
        return stopped(
            GeoNameRegionResolutionStatus::Abstained,
            GeoNameRegionResolutionReason::NoNameMatchInRegion,
            "no candidate in the bounded region matched the asserted name under the declared entity operators",
            Some(request.requested_entity_level),
        );
    }
    if !rarity.discriminating_in_region {
        return stopped(
            GeoNameRegionResolutionStatus::Abstained,
            GeoNameRegionResolutionReason::NonDiscriminatingNameInRegion,
            "the asserted name is not discriminating in the bounded region and must route to review",
            Some(request.requested_entity_level),
        );
    }

    let eligible_matches = candidate_reports
        .iter()
        .filter(|candidate| candidate.name_matched && candidate.hard_attribute_failures.is_empty())
        .collect::<Vec<_>>();
    if eligible_matches.is_empty() {
        return stopped(
            GeoNameRegionResolutionStatus::Abstained,
            GeoNameRegionResolutionReason::AttributeDisagreementAtConfirmStep,
            "the unique name match fails one or more hard attribute confirmations",
            Some(request.requested_entity_level),
        );
    }
    if eligible_matches.len() > 1 {
        return stopped(
            GeoNameRegionResolutionStatus::Abstained,
            GeoNameRegionResolutionReason::AmbiguousNameMatchesInRegion,
            "more than one candidate remains after name-region blocking and hard attribute confirmation",
            Some(request.requested_entity_level),
        );
    }

    let selected_report = eligible_matches[0];
    let Some(candidate) = candidates
        .iter()
        .find(|candidate| candidate.candidate_id == selected_report.candidate_id)
    else {
        return stopped(
            GeoNameRegionResolutionStatus::Refused,
            GeoNameRegionResolutionReason::NoDescentPathToSupportedGrain,
            "selected name-region candidate is missing from the canonical candidate set",
            Some(request.requested_entity_level),
        );
    };
    let requested_entity_ids = candidate_entity_ids(candidate, request.requested_entity_level);
    if requested_entity_ids.is_empty() {
        return stopped(
            GeoNameRegionResolutionStatus::Refused,
            GeoNameRegionResolutionReason::NoDescentPathToSupportedGrain,
            "selected POI/property candidate has no pinned descent path to the requested supported grain",
            Some(request.requested_entity_level),
        );
    }
    let representation = match candidate.entity_level {
        level if level == request.requested_entity_level => "direct_name_region_candidate",
        GeoControlEntityLevel::Poi | GeoControlEntityLevel::Property => {
            "descended_from_named_entity"
        }
        _ => "descended_to_supported_grain",
    }
    .to_string();
    let mut selected_sets = vec![GeoNameRegionResolvedSet {
        entity_level: request.requested_entity_level,
        entity_ids: requested_entity_ids,
        source_candidate_ids: vec![candidate.candidate_id.clone()],
        representation: representation.clone(),
    }];
    if request.requested_entity_level == GeoControlEntityLevel::Parcel {
        let building_ids = candidate_entity_ids(candidate, GeoControlEntityLevel::Building);
        if !building_ids.is_empty() {
            selected_sets.push(GeoNameRegionResolvedSet {
                entity_level: GeoControlEntityLevel::Building,
                entity_ids: building_ids,
                source_candidate_ids: vec![candidate.candidate_id.clone()],
                representation,
            });
        }
    }
    selected_sets.sort_by_key(|selected_set| selected_set.entity_level);

    NameRegionDecision {
        status: GeoNameRegionResolutionStatus::Resolved,
        reason: GeoNameRegionResolutionReason::UniqueNameRegionMatchDescendedToSupportedGrain,
        selected_sets,
        refusals: Vec::new(),
    }
}

fn stopped(
    status: GeoNameRegionResolutionStatus,
    reason: GeoNameRegionResolutionReason,
    detail: impl Into<String>,
    requested_entity_level: Option<GeoControlEntityLevel>,
) -> NameRegionDecision {
    NameRegionDecision {
        status,
        reason,
        selected_sets: Vec::new(),
        refusals: vec![GeoNameRegionRefusal {
            reason,
            detail: detail.into(),
            requested_entity_level,
        }],
    }
}

fn validate_name_region_request_shell(
    request: &GeoNameRegionResolutionRequest,
) -> Result<(), GeoPreResolutionError> {
    if request.version != CANON_GEO_NAME_REGION_RESOLUTION_VERSION {
        return Err(GeoPreResolutionError::new(
            GeoPreResolutionErrorCode::UnsupportedVersion,
            "Unsupported Geo name-region resolution request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_NAME_REGION_RESOLUTION_VERSION),
            ],
        ));
    }
    validate_source_corpus(&request.source_corpus)?;
    validate_build_receipts(&request.build_receipts)?;
    validate_bounded_geography("region", &request.region)?;
    validate_geo_claim_classes(&request.requested_claim_classes)?;
    validate_name_region_subject(&request.subject)?;
    validate_name_region_rarity_policy(&request.rarity_policy)?;
    Ok(())
}

fn canonical_name_region_source_pins(
    source_pins: &[GeoNameRegionSourcePin],
) -> Result<Vec<GeoNameRegionSourcePin>, GeoPreResolutionError> {
    let mut pins = source_pins.to_vec();
    pins.sort_by(|left, right| {
        left.source_instance_id
            .cmp(&right.source_instance_id)
            .then_with(|| left.role.cmp(&right.role))
            .then_with(|| left.release_id.cmp(&right.release_id))
    });
    validate_name_region_source_pins(&pins)?;
    Ok(pins)
}

fn canonical_name_region_cell_coverage(
    coverage: &GeoNameRegionCellCoverage,
) -> Result<GeoNameRegionCellCoverage, GeoPreResolutionError> {
    let mut coverage = coverage.clone();
    coverage.h3_cells = canonical_string_list(&coverage.h3_cells)?;
    validate_name_region_cell_coverage(&coverage)?;
    Ok(coverage)
}

fn canonical_name_region_candidates(
    candidates: &[GeoNameRegionCandidate],
) -> Result<Vec<GeoNameRegionCandidate>, GeoPreResolutionError> {
    let mut canonical = candidates.to_vec();
    for candidate in &mut canonical {
        candidate.region_cell_ids = canonical_string_list(&candidate.region_cell_ids)?;
        candidate.name_evidence.sort_by(|left, right| {
            left.operator
                .cmp(&right.operator)
                .then_with(|| left.operator_id.cmp(&right.operator_id))
                .then_with(|| left.profile_id.cmp(&right.profile_id))
                .then_with(|| left.candidate_surface.cmp(&right.candidate_surface))
        });
        candidate.attribute_evidence.sort_by(|left, right| {
            left.field
                .cmp(&right.field)
                .then_with(|| left.source_instance_id.cmp(&right.source_instance_id))
                .then_with(|| left.source_record_id.cmp(&right.source_record_id))
                .then_with(|| left.candidate_value.cmp(&right.candidate_value))
        });
        candidate.descent_edges.sort_by(|left, right| {
            left.from_level
                .cmp(&right.from_level)
                .then_with(|| left.from_id.cmp(&right.from_id))
                .then_with(|| left.to_level.cmp(&right.to_level))
                .then_with(|| left.to_id.cmp(&right.to_id))
                .then_with(|| left.source_instance_id.cmp(&right.source_instance_id))
        });
        validate_name_region_candidate(candidate)?;
    }
    canonical.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
    let mut previous: Option<&str> = None;
    for candidate in &canonical {
        if let Some(previous_id) = previous
            && previous_id >= candidate.candidate_id.as_str()
        {
            return Err(GeoPreResolutionError::invalid(
                "Geo name-region candidates must be strictly sorted and unique",
                [
                    ("field", "candidates[].candidate_id".to_string()),
                    ("candidate_id", candidate.candidate_id.clone()),
                ],
            ));
        }
        previous = Some(candidate.candidate_id.as_str());
    }
    Ok(canonical)
}

fn canonical_geo_claim_classes(
    claim_classes: &[GeoClaimClass],
) -> Result<Vec<GeoClaimClass>, GeoPreResolutionError> {
    let mut values = claim_classes.to_vec();
    values.sort();
    values.dedup();
    validate_geo_claim_classes(&values)?;
    Ok(values)
}

fn canonical_string_list(values: &[String]) -> Result<Vec<String>, GeoPreResolutionError> {
    let mut canonical = values.to_vec();
    canonical.sort();
    canonical.dedup();
    validate_string_vec("string_list", &canonical)?;
    Ok(canonical)
}

fn name_region_candidate_reports(
    candidates: &[GeoNameRegionCandidate],
    coverage: &GeoNameRegionCellCoverage,
    min_name_score_basis_points: u32,
    selected_candidate_ids: &BTreeSet<String>,
) -> Result<Vec<GeoNameRegionCandidateReport>, GeoPreResolutionError> {
    let coverage_cells = coverage.h3_cells.iter().collect::<BTreeSet<_>>();
    candidates
        .iter()
        .map(|candidate| {
            let region_cell_overlap_count = candidate
                .region_cell_ids
                .iter()
                .filter(|cell| coverage_cells.contains(cell))
                .count() as u64;
            let in_region = region_cell_overlap_count > 0;
            let best_name_score_basis_points = candidate
                .name_evidence
                .iter()
                .map(|evidence| evidence.score_basis_points)
                .max()
                .unwrap_or(0);
            let name_matched =
                in_region && best_name_score_basis_points >= min_name_score_basis_points;
            let hard_attribute_failures = candidate
                .attribute_evidence
                .iter()
                .filter(|evidence| !evidence.hard_filter_passed)
                .map(|evidence| attribute_field_name(evidence.field).to_string())
                .collect::<Vec<_>>();
            Ok(GeoNameRegionCandidateReport {
                candidate_id: candidate.candidate_id.clone(),
                entity_level: candidate.entity_level,
                display_name: candidate.display_name.clone(),
                source_instance_id: candidate.source_instance_id.clone(),
                in_region,
                name_matched,
                best_name_score_basis_points,
                region_cell_overlap_count,
                hard_attribute_failures,
                selected: selected_candidate_ids.contains(&candidate.candidate_id),
            })
        })
        .collect()
}

fn candidate_entity_ids(
    candidate: &GeoNameRegionCandidate,
    requested_entity_level: GeoControlEntityLevel,
) -> Vec<String> {
    let mut ids = if candidate.entity_level == requested_entity_level {
        vec![candidate.candidate_id.clone()]
    } else {
        candidate
            .descent_edges
            .iter()
            .filter(|edge| {
                edge.from_level == candidate.entity_level
                    && edge.from_id == candidate.candidate_id
                    && edge.to_level == requested_entity_level
            })
            .map(|edge| edge.to_id.clone())
            .collect::<Vec<_>>()
    };
    ids.sort();
    ids.dedup();
    ids
}

fn missing_name_region_pin_reason(
    source_pins: &[GeoNameRegionSourcePin],
    coverage: &GeoNameRegionCellCoverage,
    candidates: &[GeoNameRegionCandidate],
    inventory: &GeoRegionalInventory,
) -> Option<String> {
    let pinned_sources = source_pins
        .iter()
        .map(|pin| pin.source_instance_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut required_sources = BTreeSet::from([coverage.source_instance_id.as_str()]);
    for candidate in candidates {
        required_sources.insert(candidate.source_instance_id.as_str());
        for attribute in &candidate.attribute_evidence {
            required_sources.insert(attribute.source_instance_id.as_str());
        }
        for edge in &candidate.descent_edges {
            required_sources.insert(edge.source_instance_id.as_str());
        }
    }
    if let Some(source_id) = required_sources
        .iter()
        .find(|source_id| !pinned_sources.contains(**source_id))
    {
        return Some(format!(
            "required source {source_id} is not bound by source_pins"
        ));
    }
    if let Some(pin) = source_pins.iter().find(|pin| {
        pin.license_class == GeoLicenseClass::Unknown
            || pin.egress_class == GeoEgressClass::Unknown
            || !inventory_has_available_source(inventory, &pin.source_instance_id)
    }) {
        return Some(format!(
            "source {} lacks an available local inventory row or usable license/egress pin",
            pin.source_instance_id
        ));
    }
    None
}

fn has_pinned_name_bearing_source(
    inventory: &GeoRegionalInventory,
    source_pins: &[GeoNameRegionSourcePin],
) -> bool {
    source_pins.iter().any(|pin| {
        pin.role == GeoNameRegionSourceRole::NameBearingEntity
            && inventory
                .sources
                .iter()
                .find(|source| source.source_instance_id == pin.source_instance_id)
                .is_some_and(source_can_bear_name)
    })
}

fn source_can_bear_name(source: &GeoRegionalSourceInstance) -> bool {
    if !source_is_available(&source.local_state) {
        return false;
    }
    let is_native_entity = matches!(
        &source.native_scope,
        GeoNativeEntityScope::NativeEntity {
            entity_level: GeoControlEntityLevel::Site
                | GeoControlEntityLevel::Property
                | GeoControlEntityLevel::Building
                | GeoControlEntityLevel::Poi,
            identity_participation: GeoIdentityParticipation::StableAlias
                | GeoIdentityParticipation::EvidenceOnly,
        }
    );
    is_native_entity
        && source.evidence_classes.iter().any(|class| {
            matches!(
                class,
                GeoEvidenceClass::AssertedAttribute | GeoEvidenceClass::EntityRelation
            )
        })
}

fn inventory_has_native_level(
    inventory: &GeoRegionalInventory,
    entity_level: GeoControlEntityLevel,
) -> bool {
    inventory.sources.iter().any(|source| {
        source_is_available(&source.local_state)
            && matches!(
                &source.native_scope,
                GeoNativeEntityScope::NativeEntity {
                    entity_level: level,
                    ..
                } if *level == entity_level
            )
    })
}

fn inventory_has_available_source(
    inventory: &GeoRegionalInventory,
    source_instance_id: &str,
) -> bool {
    inventory
        .sources
        .iter()
        .find(|source| source.source_instance_id == source_instance_id)
        .is_some_and(|source| source_is_available(&source.local_state))
}

fn source_is_available(state: &GeoLocalAcquisitionState) -> bool {
    matches!(
        state.state,
        GeoSourceAvailability::Available | GeoSourceAvailability::Partial
    ) && state.local_ref.is_some()
}

fn validate_name_region_subject(
    subject: &GeoNameRegionSubject,
) -> Result<(), GeoPreResolutionError> {
    validate_string("subject.row_id", &subject.row_id)?;
    validate_string("subject.source_record_id", &subject.source_record_id)?;
    validate_blake3_uri(
        "subject.source_record_blake3",
        &subject.source_record_blake3,
    )?;
    validate_string("subject.asserted_name", &subject.asserted_name)?;
    validate_string("subject.asserted_region", &subject.asserted_region)?;
    if let Some(property_type) = &subject.asserted_property_type {
        validate_string("subject.asserted_property_type", property_type)?;
    }
    Ok(())
}

fn validate_bounded_geography(
    field: &'static str,
    region: &GeoBoundedGeography,
) -> Result<(), GeoPreResolutionError> {
    validate_string(field, &region.geography_id)?;
    validate_string(field, &region.geography_kind)?;
    validate_string(field, &region.description)?;
    Ok(())
}

fn validate_geo_claim_classes(
    claim_classes: &[GeoClaimClass],
) -> Result<(), GeoPreResolutionError> {
    if claim_classes.is_empty() {
        return Err(GeoPreResolutionError::invalid(
            "Geo name-region requests must declare at least one claim class",
            [("field", "requested_claim_classes")],
        ));
    }
    Ok(())
}

fn validate_name_region_rarity_policy(
    policy: &GeoNameRegionRarityPolicy,
) -> Result<(), GeoPreResolutionError> {
    validate_string("rarity_policy.policy_id", &policy.policy_id)?;
    if policy.max_name_matches_for_resolution == 0 {
        return Err(GeoPreResolutionError::invalid(
            "Geo name-region rarity policy must allow at least one resolved match",
            [("field", "rarity_policy.max_name_matches_for_resolution")],
        ));
    }
    if policy.min_name_score_basis_points > 10_000 {
        return Err(GeoPreResolutionError::invalid(
            "Geo name-region name scores use integer basis points in 0..=10000",
            [("field", "rarity_policy.min_name_score_basis_points")],
        ));
    }
    if policy.chain_review_min_matches <= policy.max_name_matches_for_resolution {
        return Err(GeoPreResolutionError::invalid(
            "Geo name-region chain review threshold must exceed the resolution match cap",
            [("field", "rarity_policy.chain_review_min_matches")],
        ));
    }
    Ok(())
}

fn validate_name_region_source_pins(
    source_pins: &[GeoNameRegionSourcePin],
) -> Result<(), GeoPreResolutionError> {
    let mut previous: Option<(&str, GeoNameRegionSourceRole)> = None;
    for pin in source_pins {
        validate_string("source_pins[].source_instance_id", &pin.source_instance_id)?;
        validate_string("source_pins[].release_id", &pin.release_id)?;
        validate_blake3_uri("source_pins[].release_digest", &pin.release_digest)?;
        validate_string("source_pins[].local_artifact_id", &pin.local_artifact_id)?;
        validate_blake3_uri("source_pins[].local_content_hash", &pin.local_content_hash)?;
        let key = (pin.source_instance_id.as_str(), pin.role);
        if let Some(previous_key) = previous
            && previous_key >= key
        {
            return Err(GeoPreResolutionError::invalid(
                "Geo name-region source pins must be strictly sorted and unique by source and role",
                [
                    ("field", "source_pins".to_string()),
                    ("source_instance_id", pin.source_instance_id.clone()),
                ],
            ));
        }
        previous = Some(key);
    }
    Ok(())
}

fn validate_name_region_cell_coverage(
    coverage: &GeoNameRegionCellCoverage,
) -> Result<(), GeoPreResolutionError> {
    validate_string("region_cell_coverage.coverage_id", &coverage.coverage_id)?;
    validate_string(
        "region_cell_coverage.source_instance_id",
        &coverage.source_instance_id,
    )?;
    validate_string(
        "region_cell_coverage.source_record_id",
        &coverage.source_record_id,
    )?;
    validate_blake3_uri(
        "region_cell_coverage.source_record_blake3",
        &coverage.source_record_blake3,
    )?;
    validate_string_vec("region_cell_coverage.h3_cells", &coverage.h3_cells)?;
    Ok(())
}

fn validate_name_region_candidate(
    candidate: &GeoNameRegionCandidate,
) -> Result<(), GeoPreResolutionError> {
    validate_string("candidates[].candidate_id", &candidate.candidate_id)?;
    validate_string(
        "candidates[].source_instance_id",
        &candidate.source_instance_id,
    )?;
    validate_string("candidates[].source_record_id", &candidate.source_record_id)?;
    validate_blake3_uri(
        "candidates[].source_record_blake3",
        &candidate.source_record_blake3,
    )?;
    validate_string("candidates[].display_name", &candidate.display_name)?;
    validate_string_vec("candidates[].region_cell_ids", &candidate.region_cell_ids)?;
    if candidate.name_evidence.is_empty() {
        return Err(GeoPreResolutionError::invalid(
            "Geo name-region candidates must carry at least one entity name operator hit",
            [
                ("field", "candidates[].name_evidence".to_string()),
                ("candidate_id", candidate.candidate_id.clone()),
            ],
        ));
    }
    for evidence in &candidate.name_evidence {
        validate_name_region_name_evidence(evidence)?;
    }
    for evidence in &candidate.attribute_evidence {
        validate_name_region_attribute_evidence(evidence)?;
    }
    for edge in &candidate.descent_edges {
        validate_name_region_descent_edge(edge)?;
    }
    Ok(())
}

fn validate_name_region_name_evidence(
    evidence: &GeoNameRegionNameEvidence,
) -> Result<(), GeoPreResolutionError> {
    validate_string("name_evidence[].operator_id", &evidence.operator_id)?;
    validate_string("name_evidence[].profile_id", &evidence.profile_id)?;
    if let Some(profile_hash) = &evidence.profile_hash {
        validate_blake3_uri("name_evidence[].profile_hash", profile_hash)?;
    }
    validate_string(
        "name_evidence[].asserted_surface",
        &evidence.asserted_surface,
    )?;
    validate_string(
        "name_evidence[].candidate_surface",
        &evidence.candidate_surface,
    )?;
    if evidence.score_basis_points > 10_000 {
        return Err(GeoPreResolutionError::invalid(
            "Geo name-region name evidence scores use integer basis points in 0..=10000",
            [("field", "name_evidence[].score_basis_points")],
        ));
    }
    Ok(())
}

fn validate_name_region_attribute_evidence(
    evidence: &GeoNameRegionAttributeEvidence,
) -> Result<(), GeoPreResolutionError> {
    validate_string(
        "attribute_evidence[].source_instance_id",
        &evidence.source_instance_id,
    )?;
    validate_string(
        "attribute_evidence[].source_record_id",
        &evidence.source_record_id,
    )?;
    validate_blake3_uri(
        "attribute_evidence[].source_record_blake3",
        &evidence.source_record_blake3,
    )?;
    validate_string(
        "attribute_evidence[].asserted_value",
        &evidence.asserted_value,
    )?;
    validate_string(
        "attribute_evidence[].candidate_value",
        &evidence.candidate_value,
    )?;
    Ok(())
}

fn validate_name_region_descent_edge(
    edge: &GeoNameRegionDescentEdge,
) -> Result<(), GeoPreResolutionError> {
    validate_string(
        "descent_edges[].source_instance_id",
        &edge.source_instance_id,
    )?;
    validate_string("descent_edges[].source_record_id", &edge.source_record_id)?;
    validate_blake3_uri(
        "descent_edges[].source_record_blake3",
        &edge.source_record_blake3,
    )?;
    validate_string("descent_edges[].from_id", &edge.from_id)?;
    validate_string("descent_edges[].to_id", &edge.to_id)?;
    validate_string("descent_edges[].relation", &edge.relation)?;
    Ok(())
}

fn validate_name_region_rarity(
    rarity: &GeoNameRegionRarityReport,
) -> Result<(), GeoPreResolutionError> {
    validate_string("regional_rarity.policy_id", &rarity.policy_id)?;
    if rarity.min_name_score_basis_points > 10_000 {
        return Err(GeoPreResolutionError::invalid(
            "Geo name-region rarity scores use integer basis points",
            [("field", "regional_rarity.min_name_score_basis_points")],
        ));
    }
    Ok(())
}

fn validate_name_region_candidate_reports(
    candidates: &[GeoNameRegionCandidateReport],
) -> Result<(), GeoPreResolutionError> {
    let mut previous: Option<&str> = None;
    for candidate in candidates {
        validate_string("candidates[].candidate_id", &candidate.candidate_id)?;
        validate_string("candidates[].display_name", &candidate.display_name)?;
        validate_string(
            "candidates[].source_instance_id",
            &candidate.source_instance_id,
        )?;
        validate_string_vec(
            "candidates[].hard_attribute_failures",
            &candidate.hard_attribute_failures,
        )?;
        if candidate.best_name_score_basis_points > 10_000 {
            return Err(GeoPreResolutionError::invalid(
                "Geo name-region candidate report scores use integer basis points",
                [("field", "candidates[].best_name_score_basis_points")],
            ));
        }
        if let Some(previous_id) = previous
            && previous_id >= candidate.candidate_id.as_str()
        {
            return Err(GeoPreResolutionError::invalid(
                "Geo name-region candidate reports must be strictly sorted",
                [
                    ("field", "candidates[].candidate_id".to_string()),
                    ("candidate_id", candidate.candidate_id.clone()),
                ],
            ));
        }
        previous = Some(candidate.candidate_id.as_str());
    }
    Ok(())
}

fn validate_name_region_selected_sets(
    selected_sets: &[GeoNameRegionResolvedSet],
) -> Result<(), GeoPreResolutionError> {
    let mut previous: Option<GeoControlEntityLevel> = None;
    for set in selected_sets {
        validate_string_vec("selected_sets[].entity_ids", &set.entity_ids)?;
        validate_string_vec(
            "selected_sets[].source_candidate_ids",
            &set.source_candidate_ids,
        )?;
        validate_string("selected_sets[].representation", &set.representation)?;
        if let Some(previous_level) = previous
            && previous_level >= set.entity_level
        {
            return Err(GeoPreResolutionError::invalid(
                "Geo name-region selected sets must be strictly sorted by entity level",
                [("field", "selected_sets[].entity_level")],
            ));
        }
        previous = Some(set.entity_level);
    }
    Ok(())
}

fn validate_name_region_refusals(
    refusals: &[GeoNameRegionRefusal],
) -> Result<(), GeoPreResolutionError> {
    for refusal in refusals {
        validate_string("refusals[].detail", &refusal.detail)?;
    }
    Ok(())
}

fn validate_name_region_summary(
    summary: &GeoNameRegionSummary,
    artifact: &GeoNameRegionResolutionArtifact,
) -> Result<(), GeoPreResolutionError> {
    if summary.input_rows != summary.resolved_rows + summary.abstained_rows + summary.refused_rows {
        return Err(GeoPreResolutionError::invalid(
            "Geo name-region summary must classify the input row exactly once",
            [("field", "summary.input_rows")],
        ));
    }
    let expected = match artifact.status {
        GeoNameRegionResolutionStatus::Resolved => (1, 0, 0),
        GeoNameRegionResolutionStatus::Abstained => (0, 1, 0),
        GeoNameRegionResolutionStatus::Refused => (0, 0, 1),
    };
    if (
        summary.resolved_rows,
        summary.abstained_rows,
        summary.refused_rows,
    ) != expected
    {
        return Err(GeoPreResolutionError::invalid(
            "Geo name-region summary row counts must match artifact status",
            [("field", "summary")],
        ));
    }
    if summary.region_cells != artifact.region_cell_coverage.h3_cells.len() as u64
        || summary.region_candidates
            != artifact
                .candidates
                .iter()
                .filter(|candidate| candidate.in_region)
                .count() as u64
        || summary.name_matched_candidates
            != artifact
                .candidates
                .iter()
                .filter(|candidate| candidate.name_matched)
                .count() as u64
        || summary.selected_candidates
            != artifact
                .candidates
                .iter()
                .filter(|candidate| candidate.selected)
                .count() as u64
        || summary.resolved_entity_sets != artifact.selected_sets.len() as u64
    {
        return Err(GeoPreResolutionError::invalid(
            "Geo name-region summary counters must match artifact sections",
            [("field", "summary")],
        ));
    }
    if artifact.status == GeoNameRegionResolutionStatus::Resolved
        && artifact.selected_sets.is_empty()
    {
        return Err(GeoPreResolutionError::invalid(
            "Resolved name-region artifacts must carry at least one selected set",
            [("field", "selected_sets")],
        ));
    }
    if artifact.status != GeoNameRegionResolutionStatus::Resolved
        && !artifact.selected_sets.is_empty()
    {
        return Err(GeoPreResolutionError::invalid(
            "Non-resolved name-region artifacts must not carry selected sets",
            [("field", "selected_sets")],
        ));
    }
    if artifact.status != GeoNameRegionResolutionStatus::Resolved && artifact.refusals.is_empty() {
        return Err(GeoPreResolutionError::invalid(
            "Abstained or refused name-region artifacts must carry a typed stop reason",
            [("field", "refusals")],
        ));
    }
    Ok(())
}

fn name_region_resolution_id(
    artifact: &GeoNameRegionResolutionArtifact,
) -> Result<String, GeoPreResolutionError> {
    #[derive(Serialize)]
    struct ArtifactSeed<'a> {
        version: &'a str,
        source_corpus: &'a GeoPreResolutionSourceCorpus,
        proof_class: GeoPreResolutionProofClass,
        build_receipts: &'a [GeoPreResolutionBuildReceipt],
        release_claim_allowed: bool,
        region: &'a GeoBoundedGeography,
        requested_entity_level: GeoControlEntityLevel,
        requested_claim_classes: &'a [GeoClaimClass],
        source_pins: &'a [GeoNameRegionSourcePin],
        region_cell_coverage: &'a GeoNameRegionCellCoverage,
        subject: &'a GeoNameRegionSubject,
        status: GeoNameRegionResolutionStatus,
        reason: GeoNameRegionResolutionReason,
        regional_rarity: &'a GeoNameRegionRarityReport,
        candidates: &'a [GeoNameRegionCandidateReport],
        selected_sets: &'a [GeoNameRegionResolvedSet],
        refusals: &'a [GeoNameRegionRefusal],
        summary: &'a GeoNameRegionSummary,
    }

    let seed = ArtifactSeed {
        version: &artifact.version,
        source_corpus: &artifact.source_corpus,
        proof_class: artifact.proof_class,
        build_receipts: &artifact.build_receipts,
        release_claim_allowed: artifact.release_claim_allowed,
        region: &artifact.region,
        requested_entity_level: artifact.requested_entity_level,
        requested_claim_classes: &artifact.requested_claim_classes,
        source_pins: &artifact.source_pins,
        region_cell_coverage: &artifact.region_cell_coverage,
        subject: &artifact.subject,
        status: artifact.status,
        reason: artifact.reason,
        regional_rarity: &artifact.regional_rarity,
        candidates: &artifact.candidates,
        selected_sets: &artifact.selected_sets,
        refusals: &artifact.refusals,
        summary: &artifact.summary,
    };
    serde_json::to_vec(&seed)
        .map(|bytes| {
            format!(
                "{CANON_GEO_NAME_REGION_RESOLUTION_VERSION}:{}",
                blake3::hash(&bytes).to_hex()
            )
        })
        .map_err(|error| {
            GeoPreResolutionError::invalid(
                "Geo name-region resolution id seed could not be serialized",
                [("error", error.to_string())],
            )
        })
}

fn attribute_field_name(field: GeoNameRegionAttributeField) -> &'static str {
    match field {
        GeoNameRegionAttributeField::PropertyType => "property_type",
        GeoNameRegionAttributeField::YearBuilt => "year_built",
        GeoNameRegionAttributeField::BuildingSize => "building_size",
    }
}

pub fn validate_pre_resolution_artifact(
    artifact: &GeoPreResolutionArtifact,
) -> Result<(), GeoPreResolutionError> {
    if artifact.version != CANON_GEO_PRE_RESOLUTION_VERSION {
        return Err(GeoPreResolutionError::new(
            GeoPreResolutionErrorCode::UnsupportedVersion,
            "Unsupported Geo pre-resolution artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_PRE_RESOLUTION_VERSION),
            ],
        ));
    }
    validate_string("pre_resolution_id", &artifact.pre_resolution_id)?;
    validate_source_corpus(&artifact.source_corpus)?;
    validate_build_receipts(&artifact.build_receipts)?;
    validate_denominators(&artifact.denominators, artifact)?;
    validate_registry_proposal(&artifact.registry_proposal)?;
    validate_stage1_exact_aliases(&artifact.stage1_exact_aliases)?;
    validate_row_dispositions("abstained_rows", &artifact.abstained_rows)?;
    validate_row_dispositions("unresolvable_rows", &artifact.unresolvable_rows)?;
    validate_review_status(&artifact.review_status)?;
    let expected_id = pre_resolution_id(artifact)?;
    if artifact.pre_resolution_id != expected_id {
        return Err(GeoPreResolutionError::invalid(
            "Geo pre-resolution id must match the canonical artifact content",
            [
                ("field", "pre_resolution_id".to_string()),
                ("expected", expected_id),
                ("actual", artifact.pre_resolution_id.clone()),
            ],
        ));
    }
    Ok(())
}

pub fn canonical_pre_resolution_bytes(
    artifact: &GeoPreResolutionArtifact,
) -> Result<Vec<u8>, GeoPreResolutionError> {
    validate_pre_resolution_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoPreResolutionError::invalid(
            "Geo pre-resolution artifact could not be serialized",
            [("error", error.to_string())],
        )
    })
}

fn validate_pre_resolution_request(
    request: &GeoPreResolutionRequest,
) -> Result<(), GeoPreResolutionError> {
    if request.version != CANON_GEO_PRE_RESOLUTION_VERSION {
        return Err(GeoPreResolutionError::new(
            GeoPreResolutionErrorCode::UnsupportedVersion,
            "Unsupported Geo pre-resolution request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_PRE_RESOLUTION_VERSION),
            ],
        ));
    }
    validate_source_corpus(&request.source_corpus)?;
    if request.source_corpus.corpus_kind != GeoPreResolutionCorpusKind::CmbsAnnexA {
        let capability = pre_resolution_corpus_capability(request.source_corpus.corpus_kind);
        return Err(GeoPreResolutionError::new(
            GeoPreResolutionErrorCode::UnsupportedCorpusKind,
            "Geo pre-resolution v0 is bounded to the CMBS Annex A address slice",
            [
                ("field", "source_corpus.corpus_kind".to_string()),
                (
                    "corpus_kind",
                    format!("{:?}", request.source_corpus.corpus_kind),
                ),
                ("capability", format!("{capability:?}")),
            ],
        ));
    }
    validate_build_receipts(&request.build_receipts)?;
    if request.rows.is_empty() {
        return Err(GeoPreResolutionError::invalid(
            "Geo pre-resolution requests must contain at least one row",
            [("field", "rows")],
        ));
    }
    let expected_rows = request.rows.len() as u64;
    if request
        .build_receipts
        .iter()
        .all(|receipt| receipt.row_count != expected_rows)
    {
        return Err(GeoPreResolutionError::invalid(
            "At least one pre-resolution receipt must declare the source-row denominator",
            [
                ("field", "build_receipts[].row_count".to_string()),
                ("row_count", expected_rows.to_string()),
            ],
        ));
    }
    for row in &request.rows {
        validate_source_row(row)?;
    }
    Ok(())
}

fn validate_source_corpus(
    source: &GeoPreResolutionSourceCorpus,
) -> Result<(), GeoPreResolutionError> {
    validate_string("source_corpus.corpus_id", &source.corpus_id)?;
    validate_string("source_corpus.corpus_version", &source.corpus_version)?;
    validate_string("source_corpus.temporal_scope", &source.temporal_scope)?;
    validate_string_vec("source_corpus.native_key_fields", &source.native_key_fields)?;
    Ok(())
}

fn validate_source_row(row: &GeoPreResolutionSourceRow) -> Result<(), GeoPreResolutionError> {
    validate_string("rows[].row_id", &row.row_id)?;
    validate_string("rows[].source_record_id", &row.source_record_id)?;
    validate_string("rows[].accession", &row.accession)?;
    validate_string("rows[].deal_id", &row.deal_id)?;
    validate_string("rows[].loan_id", &row.loan_id)?;
    validate_blake3_uri("rows[].source_record_blake3", &row.source_record_blake3)?;
    if let Some(address) = &row.asserted_address {
        validate_string("rows[].asserted_address", address)?;
    }
    if let Some(reach) = &row.reach {
        validate_string("rows[].reach", reach)?;
    }
    if let Some(reason) = &row.reach_none_reason {
        validate_string("rows[].reach_none_reason", reason)?;
    }
    validate_string_vec("rows[].parcel_set", &row.parcel_set)?;
    validate_string_vec("rows[].building_set", &row.building_set)?;
    Ok(())
}

fn validate_build_receipts(
    receipts: &[GeoPreResolutionBuildReceipt],
) -> Result<(), GeoPreResolutionError> {
    if receipts.is_empty() {
        return Err(GeoPreResolutionError::invalid(
            "Geo pre-resolution artifacts must carry at least one build/query receipt",
            [("field", "build_receipts")],
        ));
    }
    let mut previous: Option<&str> = None;
    for receipt in receipts {
        validate_string("build_receipts[].receipt_id", &receipt.receipt_id)?;
        validate_string("build_receipts[].query_id", &receipt.query_id)?;
        validate_blake3_uri(
            "build_receipts[].source_artifact_blake3",
            &receipt.source_artifact_blake3,
        )?;
        if receipt.run_status == GeoPreResolutionRunStatus::Cancelled {
            return Err(GeoPreResolutionError::invalid(
                "Cancelled pre-resolution runs are non-evidence and cannot be materialized",
                [
                    ("field", "build_receipts[].run_status".to_string()),
                    ("receipt_id", receipt.receipt_id.clone()),
                ],
            ));
        }
        if let Some(previous_receipt_id) = previous
            && previous_receipt_id >= receipt.receipt_id.as_str()
        {
            return Err(GeoPreResolutionError::invalid(
                "Geo pre-resolution receipts must be strictly sorted and unique",
                [
                    ("field", "build_receipts[].receipt_id".to_string()),
                    ("receipt_id", receipt.receipt_id.clone()),
                ],
            ));
        }
        previous = Some(receipt.receipt_id.as_str());
    }
    Ok(())
}

fn duplicate_exact_addresses(rows: &[GeoPreResolutionSourceRow]) -> BTreeSet<String> {
    let mut counts = BTreeMap::<String, u64>::new();
    for row in rows {
        if row
            .reach
            .as_deref()
            .is_some_and(|reach| reach.eq_ignore_ascii_case("none"))
        {
            continue;
        }
        if let Some(address) = &row.asserted_address {
            *counts.entry(address.clone()).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .filter_map(|(address, count)| (count > 1).then_some(address))
        .collect()
}

fn canonical_ledger_seed_bytes(
    rows: &[GeoLedgerIdentifierRow],
) -> Result<Vec<u8>, GeoPreResolutionError> {
    #[derive(Serialize)]
    struct LedgerSeed<'a> {
        version: &'static str,
        rows: &'a [GeoLedgerIdentifierRow],
    }
    serde_json::to_vec(&LedgerSeed {
        version: "canon_geo_collateral_ledger_seed.v0",
        rows,
    })
    .map_err(|error| {
        GeoPreResolutionError::invalid(
            "Geo pre-resolution ledger seed could not be serialized",
            [("error", error.to_string())],
        )
    })
}

fn stage1_aliases_for_rows(
    rows: &[GeoPreResolutionSourceRow],
    resolved_source_row_ids: &BTreeMap<(String, String), String>,
    registry_proposal: &GeoRegistryMintProposal,
) -> Result<Vec<GeoPreResolutionExactAlias>, GeoPreResolutionError> {
    let assertions = registry_proposal
        .property_assertions
        .iter()
        .map(|assertion| {
            (
                (assertion.accession.clone(), assertion.loan_id.clone()),
                assertion.property_id.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut aliases = Vec::new();
    for row in rows {
        let key = (row.accession.clone(), row.loan_id.clone());
        if !resolved_source_row_ids.contains_key(&key) {
            continue;
        }
        let canonical_id = assertions.get(&key).ok_or_else(|| {
            GeoPreResolutionError::invalid(
                "Geo pre-resolution registry proposal is missing a property assertion",
                [
                    ("accession", row.accession.clone()),
                    ("loan_id", row.loan_id.clone()),
                ],
            )
        })?;
        let address = row.asserted_address.as_ref().ok_or_else(|| {
            GeoPreResolutionError::invalid(
                "Resolved pre-resolution rows must carry an asserted address",
                [("row_id", row.row_id.clone())],
            )
        })?;
        aliases.push(GeoPreResolutionExactAlias {
            alias: address.clone(),
            canonical_id: canonical_id.clone(),
            canonical_type: "property".to_string(),
            rule_id: GEO_PRE_RESOLUTION_CMBS_ADDRESS_RULE_ID.to_string(),
            source_row_ids: vec![row.row_id.clone()],
        });
    }
    aliases.sort_by(|left, right| left.alias.cmp(&right.alias));
    Ok(aliases)
}

fn append_stage1_alias_entries(
    registry_proposal: &mut GeoRegistryMintProposal,
    exact_aliases: &[GeoPreResolutionExactAlias],
) -> Result<(), GeoPreResolutionError> {
    let mut entries = registry_proposal
        .entries
        .iter()
        .map(|entry| (entry.alias.clone(), entry.clone()))
        .collect::<BTreeMap<_, _>>();
    for alias in exact_aliases {
        let entry = GeoRegistryProposalEntry {
            alias: alias.alias.clone(),
            canonical_id: alias.canonical_id.clone(),
            canonical_type: alias.canonical_type.clone(),
            rule_id: alias.rule_id.clone(),
        };
        match entries.get(&entry.alias) {
            Some(existing)
                if existing.canonical_id == entry.canonical_id
                    && existing.canonical_type == entry.canonical_type
                    && existing.rule_id == entry.rule_id =>
            {
                continue;
            }
            Some(existing) => {
                return Err(GeoPreResolutionError::invalid(
                    "Pre-resolution exact alias conflicts with an existing registry proposal entry",
                    [
                        ("alias", entry.alias),
                        ("canonical_id_before", existing.canonical_id.clone()),
                        ("canonical_id_after", entry.canonical_id),
                    ],
                ));
            }
            None => {
                entries.insert(entry.alias.clone(), entry);
            }
        }
    }
    registry_proposal.entries = entries.into_values().collect();
    registry_proposal.summary.entries = registry_proposal.entries.len() as u64;
    Ok(())
}

fn sort_registry_proposal(registry_proposal: &mut GeoRegistryMintProposal) {
    registry_proposal
        .entries
        .sort_by(|left, right| left.alias.cmp(&right.alias));
    registry_proposal
        .property_assertions
        .sort_by(|left, right| {
            left.property_id
                .cmp(&right.property_id)
                .then_with(|| left.document_alias.cmp(&right.document_alias))
        });
}

fn row_disposition(
    row: &GeoPreResolutionSourceRow,
    reason: GeoPreResolutionDispositionReason,
    detail: &str,
) -> GeoPreResolutionRowDisposition {
    GeoPreResolutionRowDisposition {
        row_id: row.row_id.clone(),
        source_record_id: row.source_record_id.clone(),
        reason,
        detail: detail.to_string(),
    }
}

fn pre_resolution_id(artifact: &GeoPreResolutionArtifact) -> Result<String, GeoPreResolutionError> {
    #[derive(Serialize)]
    struct ArtifactSeed<'a> {
        version: &'a str,
        source_corpus: &'a GeoPreResolutionSourceCorpus,
        proof_class: GeoPreResolutionProofClass,
        build_receipts: &'a [GeoPreResolutionBuildReceipt],
        denominators: &'a GeoPreResolutionDenominators,
        registry_proposal: &'a GeoRegistryMintProposal,
        stage1_exact_aliases: &'a [GeoPreResolutionExactAlias],
        abstained_rows: &'a [GeoPreResolutionRowDisposition],
        unresolvable_rows: &'a [GeoPreResolutionRowDisposition],
        review_status: &'a GeoPreResolutionReviewStatus,
    }

    let seed = ArtifactSeed {
        version: &artifact.version,
        source_corpus: &artifact.source_corpus,
        proof_class: artifact.proof_class,
        build_receipts: &artifact.build_receipts,
        denominators: &artifact.denominators,
        registry_proposal: &artifact.registry_proposal,
        stage1_exact_aliases: &artifact.stage1_exact_aliases,
        abstained_rows: &artifact.abstained_rows,
        unresolvable_rows: &artifact.unresolvable_rows,
        review_status: &artifact.review_status,
    };
    serde_json::to_vec(&seed)
        .map(|bytes| {
            format!(
                "{CANON_GEO_PRE_RESOLUTION_VERSION}:{}",
                blake3::hash(&bytes).to_hex()
            )
        })
        .map_err(|error| {
            GeoPreResolutionError::invalid(
                "Geo pre-resolution id seed could not be serialized",
                [("error", error.to_string())],
            )
        })
}

fn validate_denominators(
    denominators: &GeoPreResolutionDenominators,
    artifact: &GeoPreResolutionArtifact,
) -> Result<(), GeoPreResolutionError> {
    if denominators.total_source_rows
        != denominators.resolved_rows + denominators.abstained_rows + denominators.unresolvable_rows
    {
        return Err(GeoPreResolutionError::invalid(
            "Geo pre-resolution row denominators must classify every source row exactly once",
            [("field", "denominators.total_source_rows")],
        ));
    }
    if denominators.stage1_exact_aliases != artifact.stage1_exact_aliases.len() as u64
        || denominators.registry_entries != artifact.registry_proposal.entries.len() as u64
        || denominators.property_assertions
            != artifact.registry_proposal.property_assertions.len() as u64
        || denominators.abstained_rows != artifact.abstained_rows.len() as u64
        || denominators.unresolvable_rows != artifact.unresolvable_rows.len() as u64
    {
        return Err(GeoPreResolutionError::invalid(
            "Geo pre-resolution denominators must match artifact sections",
            [("field", "denominators")],
        ));
    }
    Ok(())
}

fn validate_registry_proposal(
    proposal: &GeoRegistryMintProposal,
) -> Result<(), GeoPreResolutionError> {
    if proposal.version != CANON_GEO_REGISTRY_PROPOSAL_VERSION {
        return Err(GeoPreResolutionError::new(
            GeoPreResolutionErrorCode::UnsupportedVersion,
            "Unsupported embedded Geo registry proposal version",
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
        return Err(GeoPreResolutionError::invalid(
            "Embedded Geo registry proposal summary must match its entries",
            [("field", "registry_proposal.summary")],
        ));
    }
    let mut previous_alias: Option<&str> = None;
    for entry in &proposal.entries {
        validate_string("registry_proposal.entries[].alias", &entry.alias)?;
        validate_string(
            "registry_proposal.entries[].canonical_id",
            &entry.canonical_id,
        )?;
        validate_string(
            "registry_proposal.entries[].canonical_type",
            &entry.canonical_type,
        )?;
        validate_string("registry_proposal.entries[].rule_id", &entry.rule_id)?;
        if let Some(previous) = previous_alias
            && previous >= entry.alias.as_str()
        {
            return Err(GeoPreResolutionError::invalid(
                "Embedded Geo registry proposal entries must be strictly sorted by alias",
                [
                    ("field", "registry_proposal.entries[].alias".to_string()),
                    ("alias", entry.alias.clone()),
                ],
            ));
        }
        previous_alias = Some(entry.alias.as_str());
    }
    let mut previous_property: Option<(&str, &str)> = None;
    for assertion in &proposal.property_assertions {
        validate_string(
            "registry_proposal.property_assertions[].property_id",
            &assertion.property_id,
        )?;
        validate_string(
            "registry_proposal.property_assertions[].document_alias",
            &assertion.document_alias,
        )?;
        validate_string(
            "registry_proposal.property_assertions[].accession",
            &assertion.accession,
        )?;
        validate_string(
            "registry_proposal.property_assertions[].loan_id",
            &assertion.loan_id,
        )?;
        validate_string_vec(
            "registry_proposal.property_assertions[].parcel_ids",
            &assertion.parcel_ids,
        )?;
        validate_string_vec(
            "registry_proposal.property_assertions[].building_ids",
            &assertion.building_ids,
        )?;
        let key = (
            assertion.property_id.as_str(),
            assertion.document_alias.as_str(),
        );
        if let Some(previous) = previous_property
            && previous >= key
        {
            return Err(GeoPreResolutionError::invalid(
                "Embedded Geo registry proposal property assertions must be strictly sorted",
                [
                    ("field", "registry_proposal.property_assertions".to_string()),
                    ("property_id", assertion.property_id.clone()),
                ],
            ));
        }
        previous_property = Some(key);
    }
    Ok(())
}

fn validate_stage1_exact_aliases(
    aliases: &[GeoPreResolutionExactAlias],
) -> Result<(), GeoPreResolutionError> {
    let mut previous: Option<&str> = None;
    for alias in aliases {
        validate_string("stage1_exact_aliases[].alias", &alias.alias)?;
        validate_string("stage1_exact_aliases[].canonical_id", &alias.canonical_id)?;
        validate_string(
            "stage1_exact_aliases[].canonical_type",
            &alias.canonical_type,
        )?;
        validate_string("stage1_exact_aliases[].rule_id", &alias.rule_id)?;
        if alias.rule_id != GEO_PRE_RESOLUTION_CMBS_ADDRESS_RULE_ID {
            return Err(GeoPreResolutionError::invalid(
                "Stage-1 exact pre-resolution aliases must carry the CMBS Annex A rule id",
                [
                    ("field", "stage1_exact_aliases[].rule_id"),
                    ("rule_id", alias.rule_id.as_str()),
                ],
            ));
        }
        validate_string_vec(
            "stage1_exact_aliases[].source_row_ids",
            &alias.source_row_ids,
        )?;
        if alias.canonical_type != "property" {
            return Err(GeoPreResolutionError::invalid(
                "Stage-1 exact address aliases must target property canonical ids",
                [
                    ("field", "stage1_exact_aliases[].canonical_type"),
                    ("canonical_type", alias.canonical_type.as_str()),
                ],
            ));
        }
        if let Some(previous_alias) = previous
            && previous_alias >= alias.alias.as_str()
        {
            return Err(GeoPreResolutionError::invalid(
                "Stage-1 exact aliases must be strictly sorted and unique",
                [
                    ("field", "stage1_exact_aliases[].alias".to_string()),
                    ("alias", alias.alias.clone()),
                ],
            ));
        }
        previous = Some(alias.alias.as_str());
    }
    Ok(())
}

fn validate_row_dispositions(
    field: &'static str,
    rows: &[GeoPreResolutionRowDisposition],
) -> Result<(), GeoPreResolutionError> {
    let mut previous: Option<&str> = None;
    for row in rows {
        validate_string("row_dispositions[].row_id", &row.row_id)?;
        validate_string("row_dispositions[].source_record_id", &row.source_record_id)?;
        validate_string("row_dispositions[].detail", &row.detail)?;
        if let Some(previous_row_id) = previous
            && previous_row_id >= row.row_id.as_str()
        {
            return Err(GeoPreResolutionError::invalid(
                "Geo pre-resolution row dispositions must be strictly sorted and unique",
                [("field", field.to_string()), ("row_id", row.row_id.clone())],
            ));
        }
        previous = Some(row.row_id.as_str());
    }
    Ok(())
}

fn validate_review_status(
    status: &GeoPreResolutionReviewStatus,
) -> Result<(), GeoPreResolutionError> {
    if let Some(receipt_id) = &status.review_receipt_id {
        validate_string("review_status.review_receipt_id", receipt_id)?;
    }
    if let Some(version) = &status.promoted_registry_version {
        validate_string("review_status.promoted_registry_version", version)?;
    }
    if status.state == GeoPreResolutionReviewState::Promoted
        && (status.review_receipt_id.is_none() || status.promoted_registry_version.is_none())
    {
        return Err(GeoPreResolutionError::invalid(
            "Promoted pre-resolution artifacts must cite review receipt and registry version",
            [("field", "review_status")],
        ));
    }
    Ok(())
}

fn validate_string(field: &'static str, value: &str) -> Result<(), GeoPreResolutionError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoPreResolutionError::invalid(
            "Geo pre-resolution string fields must be non-empty and canonical-trimmed",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_string_vec(
    field: &'static str,
    values: &[String],
) -> Result<(), GeoPreResolutionError> {
    let mut previous: Option<&str> = None;
    for value in values {
        validate_string(field, value)?;
        if let Some(previous_value) = previous
            && previous_value >= value.as_str()
        {
            return Err(GeoPreResolutionError::invalid(
                "Geo pre-resolution string lists must be strictly sorted and unique",
                [("field", field.to_string()), ("value", value.clone())],
            ));
        }
        previous = Some(value.as_str());
    }
    Ok(())
}

fn validate_blake3_uri(field: &'static str, value: &str) -> Result<(), GeoPreResolutionError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(GeoPreResolutionError::invalid(
            "Geo pre-resolution digest fields must use blake3:<hex>",
            [("field", field), ("value", value)],
        ));
    };
    if hex.len() != 64
        || !hex.chars().all(|ch| ch.is_ascii_hexdigit())
        || hex.chars().any(|ch| ch.is_ascii_uppercase())
    {
        return Err(GeoPreResolutionError::invalid(
            "Geo pre-resolution blake3 digests must be lowercase fixed-width hex",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}
