#![forbid(unsafe_code)]

//! Workbench-side pre-resolution artifacts for owned property corpora.
//!
//! These artifacts are registry proposals, not runtime lookup behavior. The
//! ordinary Canon path remains exact replay against reviewed registry entries.

use super::{
    CANON_GEO_REGISTRY_PROPOSAL_VERSION, GeoLedgerIdentifierRow, GeoRegistryMintProposal,
    GeoRegistryProposalEntry, registry_proposal_from_ledger_rows,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_PRE_RESOLUTION_VERSION: &str = "canon_geo_pre_resolution.v0";
pub const GEO_PRE_RESOLUTION_CMBS_ADDRESS_RULE_ID: &str =
    "geo_pre_resolution.cmbs_annex_a_address.v1";

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
