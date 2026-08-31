#![forbid(unsafe_code)]

//! Protocol-neutral Geo discovery and acquisition handoff contracts.
//!
//! Canon emits and validates these artifacts offline. External executors may
//! satisfy them through catalog services, warehouses, object stores, HTTP, or
//! local exports, but those protocols, credentials, and network effects never
//! enter Canon's deterministic request identity.

use crate::geo::{GeoBoundedGeography, GeoControlEntityLevel, GeoEvidenceClass};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_DISCOVERY_REQUEST_VERSION: &str = "canon_geo_discovery_request.v0";
pub const CANON_GEO_ACQUISITION_REQUEST_VERSION: &str = "canon_geo_acquisition_request.v0";
pub const CANON_GEO_ACQUISITION_RECEIPT_VERSION: &str = "canon_geo_acquisition_receipt.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoDiscoveryStep {
    CatalogSearch,
    ListReleases,
    DescribeSchema,
    ColumnReadabilityProbe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoFieldRole {
    Identifier,
    Geometry,
    Attribute,
    Temporal,
    Ordering,
    Denominator,
    Digest,
    Provenance,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRequestedField {
    pub field_id: String,
    pub role: GeoFieldRole,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoReleasePin {
    pub source_instance_id: String,
    pub release_id: String,
    pub release_digest: GeoDigest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoReleaseSelectionMode {
    LatestNotAfterAsOf,
    ExactReleaseIds,
    AllOverlappingAsOf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDiscoveryReleaseSelectionPolicy {
    /// Whole UTC day in `YYYY-MM-DD` form.
    pub as_of_utc_day: String,
    pub mode: GeoReleaseSelectionMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_release_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoSubsetPredicateKind {
    H3Cells,
    BoundingBox,
    AdministrativeBoundary,
    ExplicitIdentifiers,
    TemporalPartition,
    ReleasePartition,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoSubsetPredicate {
    pub predicate_id: String,
    pub kind: GeoSubsetPredicateKind,
    pub expression: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoBoundedSubset {
    pub subset_id: String,
    pub geography: GeoBoundedGeography,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub h3_cells: Vec<String>,
    pub predicates: Vec<GeoSubsetPredicate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoOrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoNullOrdering {
    First,
    Last,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoOrderingTerm {
    pub position: u32,
    pub field_id: String,
    pub direction: GeoOrderDirection,
    pub nulls: GeoNullOrdering,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPaginationRequest {
    pub page_size_rows: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRowByteCeilings {
    pub max_rows: u64,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoProjectionOperation {
    pub coordinate_reference_system: String,
    pub operation_id: String,
    pub operation_version: String,
    pub operation_digest: GeoDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoColumnReadabilityProbe {
    pub probe_id: String,
    pub fields: Vec<String>,
    pub subset: GeoBoundedSubset,
    pub ceilings: GeoRowByteCeilings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDiscoveryRequest {
    pub version: String,
    pub request_id: String,
    pub bounded_geography: GeoBoundedGeography,
    pub subset: GeoBoundedSubset,
    pub requested_entity_levels: Vec<GeoControlEntityLevel>,
    pub requested_evidence_classes: Vec<GeoEvidenceClass>,
    pub release_selection: GeoDiscoveryReleaseSelectionPolicy,
    pub releases: Vec<GeoReleasePin>,
    pub fields: Vec<GeoRequestedField>,
    pub required_steps: Vec<GeoDiscoveryStep>,
    pub column_readability_probe: GeoColumnReadabilityProbe,
    pub ceilings: GeoRowByteCeilings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAcquisitionRequest {
    pub version: String,
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_request_id: Option<String>,
    pub bounded_geography: GeoBoundedGeography,
    pub subset: GeoBoundedSubset,
    pub releases: Vec<GeoReleasePin>,
    pub fields: Vec<GeoRequestedField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection: Option<GeoProjectionOperation>,
    pub ordering: Vec<GeoOrderingTerm>,
    pub pagination: GeoPaginationRequest,
    pub ceilings: GeoRowByteCeilings,
    pub positive_path_min_rows: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoExecutorKind {
    Catalog,
    QueryEngine,
    ObjectStore,
    HttpService,
    LocalFile,
    ManualExport,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoExecutorTrace {
    pub executor_kind: GeoExecutorKind,
    pub executor_id: String,
    pub executor_version: String,
    pub tool_id: String,
    pub tool_version: String,
    pub executor_request_id: String,
    pub executor_query_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_attempt_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAcquisitionProofClass {
    Fixture,
    Retained,
    Live,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GeoAcquisitionTerminalState {
    Complete,
    ZeroRows,
    Timeout,
    Canceled,
    Partial,
    UnreadableColumns,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoDigestAlgorithm {
    Blake3,
    Sha256,
    Sha512,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoDigest {
    pub digest_id: String,
    pub algorithm: GeoDigestAlgorithm,
    pub hex_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoDenominatorSource {
    RequestedSubset,
    ExecutorReported,
    ResultArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAcquisitionDenominator {
    pub denominator_id: String,
    pub source: GeoDenominatorSource,
    pub count: u64,
    pub unit: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPaginationReceipt {
    pub requested_page: GeoPaginationRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub rows_truncated: bool,
    pub bytes_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAcquisitionCounts {
    pub rows: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAcquisitionResumability {
    pub resumable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_request_id: Option<String>,
    pub retry_guidance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoLocalArtifactDigest {
    pub artifact_id: String,
    pub media_type: String,
    pub byte_count: u64,
    pub digest: GeoDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAcquisitionReceipt {
    pub version: String,
    pub request_id: String,
    pub request_semantic_hash: String,
    pub terminal_state: GeoAcquisitionTerminalState,
    pub proof_class: GeoAcquisitionProofClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor: Option<GeoExecutorTrace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixture_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_receipt_id: Option<String>,
    pub bounded_geography: GeoBoundedGeography,
    pub subset: GeoBoundedSubset,
    pub releases: Vec<GeoReleasePin>,
    pub fields: Vec<GeoRequestedField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection: Option<GeoProjectionOperation>,
    pub normalized_executed_request_digest: GeoDigest,
    pub pagination: GeoPaginationReceipt,
    pub counts: GeoAcquisitionCounts,
    pub denominators: Vec<GeoAcquisitionDenominator>,
    pub source_digests: Vec<GeoDigest>,
    pub result_digests: Vec<GeoDigest>,
    pub local_artifacts: Vec<GeoLocalArtifactDigest>,
    pub unreadable_columns: Vec<String>,
    pub resumability: GeoAcquisitionResumability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoDiscoveryErrorCode {
    UnsupportedVersion,
    InvalidInput,
    SecretMaterial,
    SemanticIdMismatch,
    ReceiptMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoDiscoveryError {
    pub code: GeoDiscoveryErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoDiscoveryError {
    fn new(
        code: GeoDiscoveryErrorCode,
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
        Self::new(GeoDiscoveryErrorCode::InvalidInput, message, detail)
    }

    fn secret(path: impl Into<String>, value: impl Into<String>) -> Self {
        Self::new(
            GeoDiscoveryErrorCode::SecretMaterial,
            "Geo discovery/acquisition artifacts must not contain credentials or protocol endpoints",
            [
                ("path", path.into()),
                ("value", redact_secretish_value(&value.into())),
            ],
        )
    }
}

impl fmt::Display for GeoDiscoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeoDiscoveryError {}

pub fn canonicalize_geo_discovery_request(request: &GeoDiscoveryRequest) -> GeoDiscoveryRequest {
    let mut canonical = request.clone();
    canonical.request_id.clear();
    canonical.subset = canonicalize_subset(&canonical.subset);
    canonical.column_readability_probe.subset =
        canonicalize_subset(&canonical.column_readability_probe.subset);
    canonical.requested_entity_levels = sorted_distinct(canonical.requested_entity_levels);
    canonical.requested_evidence_classes = sorted_distinct(canonical.requested_evidence_classes);
    canonical.release_selection.candidate_release_ids =
        sorted_distinct(canonical.release_selection.candidate_release_ids);
    canonical.releases = sorted_distinct(canonical.releases);
    canonical.fields = sorted_distinct(canonical.fields);
    canonical.required_steps = sorted_distinct(canonical.required_steps);
    canonical.column_readability_probe.fields =
        sorted_distinct(canonical.column_readability_probe.fields);
    canonical
}

pub fn canonicalize_geo_acquisition_request(
    request: &GeoAcquisitionRequest,
) -> GeoAcquisitionRequest {
    let mut canonical = request.clone();
    canonical.request_id.clear();
    canonical.subset = canonicalize_subset(&canonical.subset);
    canonical.releases = sorted_distinct(canonical.releases);
    canonical.fields = sorted_distinct(canonical.fields);
    canonical.ordering.sort();
    canonical
}

pub fn canonical_geo_discovery_request_bytes(
    request: &GeoDiscoveryRequest,
) -> Result<Vec<u8>, GeoDiscoveryError> {
    serde_json::to_vec(&canonicalize_geo_discovery_request(request)).map_err(json_error)
}

pub fn canonical_geo_acquisition_request_bytes(
    request: &GeoAcquisitionRequest,
) -> Result<Vec<u8>, GeoDiscoveryError> {
    serde_json::to_vec(&canonicalize_geo_acquisition_request(request)).map_err(json_error)
}

pub fn geo_discovery_request_semantic_hash(
    request: &GeoDiscoveryRequest,
) -> Result<String, GeoDiscoveryError> {
    let bytes = canonical_geo_discovery_request_bytes(request)?;
    Ok(blake3_hash(&bytes))
}

pub fn geo_acquisition_request_semantic_hash(
    request: &GeoAcquisitionRequest,
) -> Result<String, GeoDiscoveryError> {
    let bytes = canonical_geo_acquisition_request_bytes(request)?;
    Ok(blake3_hash(&bytes))
}

pub fn geo_discovery_request_id(
    request: &GeoDiscoveryRequest,
) -> Result<String, GeoDiscoveryError> {
    let hash = geo_discovery_request_semantic_hash(request)?;
    Ok(format!(
        "{}:{}",
        CANON_GEO_DISCOVERY_REQUEST_VERSION,
        hash.trim_start_matches("blake3:")
    ))
}

pub fn geo_acquisition_request_id(
    request: &GeoAcquisitionRequest,
) -> Result<String, GeoDiscoveryError> {
    let hash = geo_acquisition_request_semantic_hash(request)?;
    Ok(format!(
        "{}:{}",
        CANON_GEO_ACQUISITION_REQUEST_VERSION,
        hash.trim_start_matches("blake3:")
    ))
}

pub fn validate_geo_discovery_request(
    request: &GeoDiscoveryRequest,
) -> Result<(), GeoDiscoveryError> {
    reject_secret_material(request)?;
    if request.version != CANON_GEO_DISCOVERY_REQUEST_VERSION {
        return Err(GeoDiscoveryError::new(
            GeoDiscoveryErrorCode::UnsupportedVersion,
            "unsupported Geo discovery request version",
            [("version", request.version.clone())],
        ));
    }
    validate_geography("bounded_geography", &request.bounded_geography)?;
    validate_subset("subset", &request.subset)?;
    validate_matching_geography(
        "bounded_geography",
        &request.bounded_geography,
        "subset.geography",
        &request.subset.geography,
    )?;
    if canonicalize_subset(&request.subset)
        != canonicalize_subset(&request.column_readability_probe.subset)
    {
        return Err(GeoDiscoveryError::invalid(
            "Geo discovery column readability must probe the same bounded subset",
            [
                ("request_subset_id", request.subset.subset_id.clone()),
                (
                    "probe_subset_id",
                    request.column_readability_probe.subset.subset_id.clone(),
                ),
            ],
        ));
    }
    validate_nonempty_distinct("requested_entity_levels", &request.requested_entity_levels)?;
    validate_nonempty_distinct(
        "requested_evidence_classes",
        &request.requested_evidence_classes,
    )?;
    validate_release_selection_policy(&request.release_selection)?;
    validate_optional_releases(&request.releases)?;
    validate_fields(&request.fields)?;
    validate_nonempty_distinct("required_steps", &request.required_steps)?;
    if !request
        .required_steps
        .contains(&GeoDiscoveryStep::ColumnReadabilityProbe)
    {
        return Err(GeoDiscoveryError::invalid(
            "metadata/list/describe discovery is not column readability",
            [("required_steps", "missing column_readability_probe")],
        ));
    }
    validate_column_readability_probe(&request.column_readability_probe, &request.fields)?;
    validate_ceilings("ceilings", &request.ceilings)?;
    let expected = geo_discovery_request_id(request)?;
    if request.request_id != expected {
        return Err(GeoDiscoveryError::new(
            GeoDiscoveryErrorCode::SemanticIdMismatch,
            "Geo discovery request_id must be the stable semantic request id",
            [
                ("expected", expected),
                ("actual", request.request_id.clone()),
            ],
        ));
    }
    Ok(())
}

pub fn validate_geo_acquisition_request(
    request: &GeoAcquisitionRequest,
) -> Result<(), GeoDiscoveryError> {
    reject_secret_material(request)?;
    if request.version != CANON_GEO_ACQUISITION_REQUEST_VERSION {
        return Err(GeoDiscoveryError::new(
            GeoDiscoveryErrorCode::UnsupportedVersion,
            "unsupported Geo acquisition request version",
            [("version", request.version.clone())],
        ));
    }
    validate_geography("bounded_geography", &request.bounded_geography)?;
    validate_subset("subset", &request.subset)?;
    validate_matching_geography(
        "bounded_geography",
        &request.bounded_geography,
        "subset.geography",
        &request.subset.geography,
    )?;
    if let Some(discovery_request_id) = &request.discovery_request_id {
        validate_discovery_request_id_format("discovery_request_id", discovery_request_id)?;
    }
    validate_releases(&request.releases)?;
    validate_fields(&request.fields)?;
    validate_projection_contract(&request.fields, request.projection.as_ref())?;
    validate_ordering(&request.ordering, &request.fields)?;
    validate_pagination_request(&request.pagination)?;
    validate_ceilings("ceilings", &request.ceilings)?;
    if request.pagination.page_size_rows > request.ceilings.max_rows {
        return Err(GeoDiscoveryError::invalid(
            "Geo acquisition page size must not exceed the row ceiling",
            [
                (
                    "page_size_rows",
                    request.pagination.page_size_rows.to_string(),
                ),
                ("max_rows", request.ceilings.max_rows.to_string()),
            ],
        ));
    }
    if request.positive_path_min_rows > request.ceilings.max_rows {
        return Err(GeoDiscoveryError::invalid(
            "Geo acquisition positive path minimum rows must not exceed the row ceiling",
            [
                (
                    "positive_path_min_rows",
                    request.positive_path_min_rows.to_string(),
                ),
                ("max_rows", request.ceilings.max_rows.to_string()),
            ],
        ));
    }
    let expected = geo_acquisition_request_id(request)?;
    if request.request_id != expected {
        return Err(GeoDiscoveryError::new(
            GeoDiscoveryErrorCode::SemanticIdMismatch,
            "Geo acquisition request_id must be the stable semantic request id",
            [
                ("expected", expected),
                ("actual", request.request_id.clone()),
            ],
        ));
    }
    Ok(())
}

pub fn validate_geo_acquisition_receipt(
    request: &GeoAcquisitionRequest,
    receipt: &GeoAcquisitionReceipt,
) -> Result<(), GeoDiscoveryError> {
    validate_geo_acquisition_request(request)?;
    reject_secret_material(receipt)?;
    if receipt.version != CANON_GEO_ACQUISITION_RECEIPT_VERSION {
        return Err(GeoDiscoveryError::new(
            GeoDiscoveryErrorCode::UnsupportedVersion,
            "unsupported Geo acquisition receipt version",
            [("version", receipt.version.clone())],
        ));
    }
    let expected_request_id = geo_acquisition_request_id(request)?;
    let expected_request_hash = geo_acquisition_request_semantic_hash(request)?;
    if receipt.request_id != expected_request_id {
        return Err(receipt_mismatch(
            "receipt request_id does not match acquisition request",
            "request_id",
            &expected_request_id,
            &receipt.request_id,
        ));
    }
    if receipt.request_semantic_hash != expected_request_hash {
        return Err(receipt_mismatch(
            "receipt request_semantic_hash does not match acquisition request",
            "request_semantic_hash",
            &expected_request_hash,
            &receipt.request_semantic_hash,
        ));
    }
    if canonicalize_subset(&receipt.subset) != canonicalize_subset(&request.subset) {
        return Err(receipt_mismatch(
            "receipt bounded subset does not match acquisition request",
            "subset_id",
            &request.subset.subset_id,
            &receipt.subset.subset_id,
        ));
    }
    if receipt.bounded_geography != request.bounded_geography {
        return Err(receipt_mismatch(
            "receipt bounded geography does not match acquisition request",
            "geography_id",
            &request.bounded_geography.geography_id,
            &receipt.bounded_geography.geography_id,
        ));
    }
    if sorted_distinct(receipt.releases.clone()) != sorted_distinct(request.releases.clone()) {
        return Err(GeoDiscoveryError::new(
            GeoDiscoveryErrorCode::ReceiptMismatch,
            "receipt releases do not match acquisition request",
            BTreeMap::<String, String>::new(),
        ));
    }
    if sorted_distinct(receipt.fields.clone()) != sorted_distinct(request.fields.clone()) {
        return Err(GeoDiscoveryError::new(
            GeoDiscoveryErrorCode::ReceiptMismatch,
            "receipt fields do not match acquisition request",
            BTreeMap::<String, String>::new(),
        ));
    }
    if receipt.projection != request.projection {
        return Err(GeoDiscoveryError::new(
            GeoDiscoveryErrorCode::ReceiptMismatch,
            "receipt projection contract does not match acquisition request",
            BTreeMap::<String, String>::new(),
        ));
    }
    validate_proof_class(receipt)?;
    validate_digest(
        "normalized_executed_request_digest",
        &receipt.normalized_executed_request_digest,
    )?;
    validate_pagination_receipt(&request.pagination, &receipt.pagination)?;
    validate_counts_against_request(request, receipt)?;
    validate_denominators(&receipt.denominators)?;
    validate_digests("source_digests", &receipt.source_digests)?;
    validate_digests("result_digests", &receipt.result_digests)?;
    validate_local_artifacts(&receipt.local_artifacts)?;
    validate_resumability(receipt)?;
    validate_terminal_state(request, receipt)?;
    Ok(())
}

pub fn geo_acquisition_receipt_satisfies_positive_gate(
    request: &GeoAcquisitionRequest,
    receipt: &GeoAcquisitionReceipt,
) -> bool {
    receipt.terminal_state == GeoAcquisitionTerminalState::Complete
        && receipt.counts.rows >= request.positive_path_min_rows
        && !receipt.pagination.rows_truncated
        && !receipt.pagination.bytes_truncated
        && receipt.pagination.next_page_token.is_none()
        && receipt.unreadable_columns.is_empty()
}

fn canonicalize_subset(subset: &GeoBoundedSubset) -> GeoBoundedSubset {
    let mut canonical = subset.clone();
    canonical.h3_cells = sorted_distinct(canonical.h3_cells);
    canonical.predicates = sorted_distinct(canonical.predicates);
    canonical
}

fn sorted_distinct<T: Ord>(values: Vec<T>) -> Vec<T> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn validate_geography(
    field: &str,
    geography: &GeoBoundedGeography,
) -> Result<(), GeoDiscoveryError> {
    validate_nonempty_trimmed(&format!("{field}.geography_id"), &geography.geography_id)?;
    validate_nonempty_trimmed(
        &format!("{field}.geography_kind"),
        &geography.geography_kind,
    )?;
    validate_nonempty_trimmed(&format!("{field}.description"), &geography.description)?;
    Ok(())
}

fn validate_subset(field: &str, subset: &GeoBoundedSubset) -> Result<(), GeoDiscoveryError> {
    validate_nonempty_trimmed(&format!("{field}.subset_id"), &subset.subset_id)?;
    validate_geography(&format!("{field}.geography"), &subset.geography)?;
    if subset.h3_cells.is_empty() && subset.predicates.is_empty() {
        return Err(GeoDiscoveryError::invalid(
            "Geo acquisition/discovery subset must be bounded",
            [("subset_id", subset.subset_id.clone())],
        ));
    }
    validate_distinct("h3_cells", &subset.h3_cells)?;
    for cell in &subset.h3_cells {
        validate_nonempty_trimmed("h3_cells[]", cell)?;
    }
    validate_distinct("predicates", &subset.predicates)?;
    for predicate in &subset.predicates {
        validate_nonempty_trimmed("predicate_id", &predicate.predicate_id)?;
        validate_nonempty_trimmed("predicate.expression", &predicate.expression)?;
        if looks_unbounded(&predicate.expression) {
            return Err(GeoDiscoveryError::invalid(
                "Geo subset predicate is unbounded",
                [
                    ("predicate_id", predicate.predicate_id.clone()),
                    ("expression", predicate.expression.clone()),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_releases(releases: &[GeoReleasePin]) -> Result<(), GeoDiscoveryError> {
    if releases.is_empty() {
        return Err(GeoDiscoveryError::invalid(
            "Geo discovery/acquisition collection must be non-empty",
            [("releases", "0".to_string())],
        ));
    }
    validate_optional_releases(releases)
}

fn validate_optional_releases(releases: &[GeoReleasePin]) -> Result<(), GeoDiscoveryError> {
    let mut release_keys = BTreeSet::new();
    for release in releases {
        validate_nonempty_trimmed("release.source_instance_id", &release.source_instance_id)?;
        validate_nonempty_trimmed("release.release_id", &release.release_id)?;
        let release_key = (
            release.source_instance_id.as_str(),
            release.release_id.as_str(),
        );
        if !release_keys.insert(release_key) {
            return Err(GeoDiscoveryError::invalid(
                "Geo release pins must have unique source_instance_id/release_id keys",
                [
                    ("source_instance_id", release.source_instance_id.clone()),
                    ("release_id", release.release_id.clone()),
                ],
            ));
        }
        validate_digest("release.release_digest", &release.release_digest)?;
    }
    Ok(())
}

fn validate_release_selection_policy(
    policy: &GeoDiscoveryReleaseSelectionPolicy,
) -> Result<(), GeoDiscoveryError> {
    validate_utc_day("release_selection.as_of_utc_day", &policy.as_of_utc_day)?;
    validate_distinct(
        "release_selection.candidate_release_ids",
        &policy.candidate_release_ids,
    )?;
    for release_id in &policy.candidate_release_ids {
        validate_nonempty_trimmed("release_selection.candidate_release_ids[]", release_id)?;
    }
    if policy.mode == GeoReleaseSelectionMode::ExactReleaseIds
        && policy.candidate_release_ids.is_empty()
    {
        return Err(GeoDiscoveryError::invalid(
            "exact discovery release selection requires candidate release ids",
            [("release_selection.mode", "exact_release_ids".to_string())],
        ));
    }
    Ok(())
}

fn validate_fields(fields: &[GeoRequestedField]) -> Result<(), GeoDiscoveryError> {
    validate_nonempty_distinct("fields", fields)?;
    let mut field_ids = BTreeSet::new();
    for field in fields {
        validate_nonempty_trimmed("field.field_id", &field.field_id)?;
        if !field_ids.insert(field.field_id.clone()) {
            return Err(GeoDiscoveryError::invalid(
                "Geo field ids must be unique",
                [("field_id", field.field_id.clone())],
            ));
        }
    }
    Ok(())
}

fn validate_ordering(
    ordering: &[GeoOrderingTerm],
    fields: &[GeoRequestedField],
) -> Result<(), GeoDiscoveryError> {
    validate_nonempty_distinct("ordering", ordering)?;
    let field_ids = fields
        .iter()
        .map(|field| field.field_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut positions = BTreeSet::new();
    for term in ordering {
        if !positions.insert(term.position) {
            return Err(GeoDiscoveryError::invalid(
                "Geo acquisition ordering positions must be unique",
                [("position", term.position.to_string())],
            ));
        }
        validate_nonempty_trimmed("ordering.field_id", &term.field_id)?;
        if !field_ids.contains(term.field_id.as_str()) {
            return Err(GeoDiscoveryError::invalid(
                "Geo acquisition ordering must reference an acquired field",
                [("field_id", term.field_id.clone())],
            ));
        }
    }
    Ok(())
}

fn validate_projection(projection: &GeoProjectionOperation) -> Result<(), GeoDiscoveryError> {
    validate_nonempty_trimmed(
        "projection.coordinate_reference_system",
        &projection.coordinate_reference_system,
    )?;
    validate_nonempty_trimmed("projection.operation_id", &projection.operation_id)?;
    validate_nonempty_trimmed(
        "projection.operation_version",
        &projection.operation_version,
    )?;
    validate_digest("projection.operation_digest", &projection.operation_digest)?;
    Ok(())
}

fn validate_projection_contract(
    fields: &[GeoRequestedField],
    projection: Option<&GeoProjectionOperation>,
) -> Result<(), GeoDiscoveryError> {
    let has_geometry_field = fields
        .iter()
        .any(|field| field.role == GeoFieldRole::Geometry);
    match (has_geometry_field, projection) {
        (true, Some(projection)) => validate_projection(projection),
        (true, None) => Err(GeoDiscoveryError::invalid(
            "Geo acquisition requests with geometry fields require a projection contract",
            [("projection", "missing")],
        )),
        (false, Some(_)) => Err(GeoDiscoveryError::invalid(
            "Geo acquisition requests without geometry fields must omit projection",
            [("projection", "unexpected")],
        )),
        (false, None) => Ok(()),
    }
}

fn validate_column_readability_probe(
    probe: &GeoColumnReadabilityProbe,
    request_fields: &[GeoRequestedField],
) -> Result<(), GeoDiscoveryError> {
    validate_nonempty_trimmed("column_readability_probe.probe_id", &probe.probe_id)?;
    validate_subset("column_readability_probe.subset", &probe.subset)?;
    validate_ceilings("column_readability_probe.ceilings", &probe.ceilings)?;
    validate_nonempty_distinct("column_readability_probe.fields", &probe.fields)?;
    let request_field_ids = request_fields
        .iter()
        .map(|field| field.field_id.as_str())
        .collect::<BTreeSet<_>>();
    for field in &probe.fields {
        validate_nonempty_trimmed("column_readability_probe.fields[]", field)?;
        if !request_field_ids.contains(field.as_str()) {
            return Err(GeoDiscoveryError::invalid(
                "Geo column readability probe must reference requested fields",
                [("field_id", field.clone())],
            ));
        }
    }
    for field in request_fields {
        if !probe.fields.contains(&field.field_id) {
            return Err(GeoDiscoveryError::invalid(
                "Geo column readability probe must cover every requested field",
                [("field_id", field.field_id.clone())],
            ));
        }
    }
    Ok(())
}

fn validate_pagination_request(pagination: &GeoPaginationRequest) -> Result<(), GeoDiscoveryError> {
    if pagination.page_size_rows == 0 {
        return Err(GeoDiscoveryError::invalid(
            "Geo acquisition pagination page size must be positive",
            [("page_size_rows", "0".to_string())],
        ));
    }
    if let Some(token) = &pagination.page_token {
        validate_nonempty_trimmed("pagination.page_token", token)?;
    }
    Ok(())
}

fn validate_pagination_receipt(
    requested: &GeoPaginationRequest,
    receipt: &GeoPaginationReceipt,
) -> Result<(), GeoDiscoveryError> {
    if &receipt.requested_page != requested {
        return Err(GeoDiscoveryError::new(
            GeoDiscoveryErrorCode::ReceiptMismatch,
            "receipt pagination request does not match acquisition request",
            BTreeMap::<String, String>::new(),
        ));
    }
    if let Some(token) = &receipt.next_page_token {
        validate_nonempty_trimmed("pagination.next_page_token", token)?;
    }
    Ok(())
}

fn validate_ceilings(field: &str, ceilings: &GeoRowByteCeilings) -> Result<(), GeoDiscoveryError> {
    if ceilings.max_rows == 0 {
        return Err(GeoDiscoveryError::invalid(
            "Geo row ceiling must be positive",
            [(format!("{field}.max_rows"), "0".to_string())],
        ));
    }
    if ceilings.max_bytes == 0 {
        return Err(GeoDiscoveryError::invalid(
            "Geo byte ceiling must be positive",
            [(format!("{field}.max_bytes"), "0".to_string())],
        ));
    }
    Ok(())
}

fn validate_counts_against_request(
    request: &GeoAcquisitionRequest,
    receipt: &GeoAcquisitionReceipt,
) -> Result<(), GeoDiscoveryError> {
    if receipt.counts.rows > request.ceilings.max_rows {
        return Err(GeoDiscoveryError::invalid(
            "Geo acquisition receipt exceeds requested row ceiling",
            [
                ("rows", receipt.counts.rows.to_string()),
                ("max_rows", request.ceilings.max_rows.to_string()),
            ],
        ));
    }
    if receipt.counts.rows > request.pagination.page_size_rows {
        return Err(GeoDiscoveryError::invalid(
            "Geo acquisition receipt exceeds requested page size",
            [
                ("rows", receipt.counts.rows.to_string()),
                (
                    "page_size_rows",
                    request.pagination.page_size_rows.to_string(),
                ),
            ],
        ));
    }
    if receipt.counts.bytes > request.ceilings.max_bytes {
        return Err(GeoDiscoveryError::invalid(
            "Geo acquisition receipt exceeds requested byte ceiling",
            [
                ("bytes", receipt.counts.bytes.to_string()),
                ("max_bytes", request.ceilings.max_bytes.to_string()),
            ],
        ));
    }
    let total_local_artifact_bytes =
        receipt
            .local_artifacts
            .iter()
            .try_fold(0_u64, |total, artifact| {
                total.checked_add(artifact.byte_count).ok_or_else(|| {
                    GeoDiscoveryError::invalid(
                        "Geo acquisition local artifact byte count overflowed",
                        [("artifact_id", artifact.artifact_id.clone())],
                    )
                })
            })?;
    if total_local_artifact_bytes > request.ceilings.max_bytes {
        return Err(GeoDiscoveryError::invalid(
            "Geo acquisition local artifacts exceed requested byte ceiling",
            [
                (
                    "local_artifact_bytes",
                    total_local_artifact_bytes.to_string(),
                ),
                ("max_bytes", request.ceilings.max_bytes.to_string()),
            ],
        ));
    }
    for artifact in &receipt.local_artifacts {
        if artifact.byte_count > request.ceilings.max_bytes {
            return Err(GeoDiscoveryError::invalid(
                "Geo acquisition local artifact exceeds requested byte ceiling",
                [
                    ("artifact_id", artifact.artifact_id.clone()),
                    ("byte_count", artifact.byte_count.to_string()),
                    ("max_bytes", request.ceilings.max_bytes.to_string()),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_resumability(receipt: &GeoAcquisitionReceipt) -> Result<(), GeoDiscoveryError> {
    validate_nonempty_trimmed(
        "resumability.retry_guidance",
        &receipt.resumability.retry_guidance,
    )?;
    let has_resume_token = match &receipt.resumability.resume_token {
        Some(token) => {
            validate_nonempty_trimmed("resumability.resume_token", token)?;
            true
        }
        None => false,
    };
    let has_resume_request_id = match &receipt.resumability.resume_request_id {
        Some(request_id) => {
            validate_acquisition_request_id_format("resumability.resume_request_id", request_id)?;
            true
        }
        None => false,
    };
    if !receipt.resumability.resumable && (has_resume_token || has_resume_request_id) {
        return Err(GeoDiscoveryError::invalid(
            "non-resumable acquisition receipts must not carry resume handles",
            [("resumable", "false".to_string())],
        ));
    }
    if matches!(
        receipt.terminal_state,
        GeoAcquisitionTerminalState::Complete | GeoAcquisitionTerminalState::ZeroRows
    ) && receipt.resumability.resumable
    {
        return Err(GeoDiscoveryError::invalid(
            "terminal acquisition receipts must not claim resumability",
            [("terminal_state", format!("{:?}", receipt.terminal_state))],
        ));
    }
    Ok(())
}

fn validate_proof_class(receipt: &GeoAcquisitionReceipt) -> Result<(), GeoDiscoveryError> {
    match receipt.proof_class {
        GeoAcquisitionProofClass::Fixture => {
            if receipt.fixture_id.is_none()
                || receipt.retained_receipt_id.is_some()
                || receipt.executor.is_some()
            {
                return Err(GeoDiscoveryError::invalid(
                    "fixture acquisition receipts must not carry retained or live executor proof",
                    [("proof_class", "fixture")],
                ));
            }
        }
        GeoAcquisitionProofClass::Retained => {
            if receipt.fixture_id.is_some()
                || receipt.retained_receipt_id.is_none()
                || receipt.executor.is_none()
            {
                return Err(GeoDiscoveryError::invalid(
                    "retained acquisition receipts require retained identity and executor ids only",
                    [("proof_class", "retained")],
                ));
            }
        }
        GeoAcquisitionProofClass::Live => {
            if receipt.fixture_id.is_some()
                || receipt.retained_receipt_id.is_some()
                || receipt.executor.is_none()
            {
                return Err(GeoDiscoveryError::invalid(
                    "live acquisition receipts require live executor ids only",
                    [("proof_class", "live")],
                ));
            }
        }
    }
    if let Some(executor) = &receipt.executor {
        validate_nonempty_trimmed("executor.executor_id", &executor.executor_id)?;
        validate_nonempty_trimmed("executor.executor_version", &executor.executor_version)?;
        validate_nonempty_trimmed("executor.tool_id", &executor.tool_id)?;
        validate_nonempty_trimmed("executor.tool_version", &executor.tool_version)?;
        validate_nonempty_trimmed(
            "executor.executor_request_id",
            &executor.executor_request_id,
        )?;
        validate_nonempty_trimmed("executor.executor_query_id", &executor.executor_query_id)?;
        if let Some(attempt_id) = &executor.executor_attempt_id {
            validate_nonempty_trimmed("executor.executor_attempt_id", attempt_id)?;
        }
    }
    if let Some(fixture_id) = &receipt.fixture_id {
        validate_nonempty_trimmed("fixture_id", fixture_id)?;
    }
    if let Some(retained_receipt_id) = &receipt.retained_receipt_id {
        validate_nonempty_trimmed("retained_receipt_id", retained_receipt_id)?;
    }
    Ok(())
}

fn validate_terminal_state(
    request: &GeoAcquisitionRequest,
    receipt: &GeoAcquisitionReceipt,
) -> Result<(), GeoDiscoveryError> {
    if let Some(detail) = &receipt.terminal_detail {
        validate_nonempty_trimmed("terminal_detail", detail)?;
    }
    let truncated = receipt.pagination.rows_truncated
        || receipt.pagination.bytes_truncated
        || receipt.pagination.next_page_token.is_some();
    match receipt.terminal_state {
        GeoAcquisitionTerminalState::Complete => {
            if receipt.counts.rows == 0 {
                return Err(GeoDiscoveryError::invalid(
                    "Geo acquisition COMPLETE requires positive rows; use ZERO_ROWS for an empty result",
                    [("rows", "0".to_string())],
                ));
            }
            if truncated {
                return Err(GeoDiscoveryError::invalid(
                    "Geo acquisition pagination truncation cannot claim COMPLETE",
                    [("terminal_state", "COMPLETE".to_string())],
                ));
            }
            if !receipt.unreadable_columns.is_empty() {
                return Err(GeoDiscoveryError::invalid(
                    "Geo acquisition COMPLETE cannot include unreadable columns",
                    [(
                        "unreadable_columns",
                        receipt.unreadable_columns.len().to_string(),
                    )],
                ));
            }
        }
        GeoAcquisitionTerminalState::ZeroRows => {
            if receipt.counts.rows != 0 {
                return Err(GeoDiscoveryError::invalid(
                    "Geo acquisition ZERO_ROWS requires zero rows",
                    [("rows", receipt.counts.rows.to_string())],
                ));
            }
            if truncated {
                return Err(GeoDiscoveryError::invalid(
                    "Geo acquisition ZERO_ROWS cannot be paginated or truncated",
                    [("terminal_state", "ZERO_ROWS".to_string())],
                ));
            }
            if !receipt.unreadable_columns.is_empty() {
                return Err(GeoDiscoveryError::invalid(
                    "Geo acquisition ZERO_ROWS is not an unreadable-column failure",
                    [(
                        "unreadable_columns",
                        receipt.unreadable_columns.len().to_string(),
                    )],
                ));
            }
        }
        GeoAcquisitionTerminalState::Partial => {
            require_executor_ids(receipt)?;
            require_terminal_detail(receipt)?;
            if receipt.counts.rows == 0 {
                return Err(GeoDiscoveryError::invalid(
                    "Geo acquisition PARTIAL requires at least one materialized row",
                    [("rows", "0".to_string())],
                ));
            }
            require_resume_or_retry_guidance(receipt)?;
        }
        GeoAcquisitionTerminalState::Timeout | GeoAcquisitionTerminalState::Canceled => {
            require_executor_ids(receipt)?;
            require_terminal_detail(receipt)?;
            require_resume_or_retry_guidance(receipt)?;
        }
        GeoAcquisitionTerminalState::UnreadableColumns => {
            require_executor_ids(receipt)?;
            require_terminal_detail(receipt)?;
            validate_nonempty_distinct("unreadable_columns", &receipt.unreadable_columns)?;
            if receipt.counts.rows != 0 {
                return Err(GeoDiscoveryError::invalid(
                    "Geo acquisition UNREADABLE_COLUMNS must not claim materialized rows",
                    [("rows", receipt.counts.rows.to_string())],
                ));
            }
            if truncated {
                return Err(GeoDiscoveryError::invalid(
                    "Geo acquisition UNREADABLE_COLUMNS cannot be paginated or truncated",
                    [("terminal_state", "UNREADABLE_COLUMNS".to_string())],
                ));
            }
            require_resume_or_retry_guidance(receipt)?;
            let requested = request
                .fields
                .iter()
                .map(|field| field.field_id.as_str())
                .collect::<BTreeSet<_>>();
            for column in &receipt.unreadable_columns {
                validate_nonempty_trimmed("unreadable_columns[]", column)?;
                if !requested.contains(column.as_str()) {
                    return Err(GeoDiscoveryError::invalid(
                        "unreadable columns must be requested acquisition fields",
                        [("field_id", column.clone())],
                    ));
                }
            }
        }
    }
    if receipt.terminal_state != GeoAcquisitionTerminalState::UnreadableColumns
        && !receipt.unreadable_columns.is_empty()
    {
        return Err(GeoDiscoveryError::invalid(
            "only UNREADABLE_COLUMNS may include unreadable_columns",
            [(
                "unreadable_columns",
                receipt.unreadable_columns.len().to_string(),
            )],
        ));
    }
    Ok(())
}

fn validate_denominators(
    denominators: &[GeoAcquisitionDenominator],
) -> Result<(), GeoDiscoveryError> {
    validate_nonempty_distinct("denominators", denominators)?;
    let mut ids = BTreeSet::new();
    for denominator in denominators {
        validate_nonempty_trimmed("denominator.denominator_id", &denominator.denominator_id)?;
        if !ids.insert(denominator.denominator_id.clone()) {
            return Err(GeoDiscoveryError::invalid(
                "Geo acquisition denominators must have unique ids",
                [("denominator_id", denominator.denominator_id.clone())],
            ));
        }
        validate_nonempty_trimmed("denominator.unit", &denominator.unit)?;
        validate_nonempty_trimmed("denominator.description", &denominator.description)?;
    }
    Ok(())
}

fn validate_digests(field: &str, digests: &[GeoDigest]) -> Result<(), GeoDiscoveryError> {
    validate_nonempty_distinct(field, digests)?;
    let mut ids = BTreeSet::new();
    for digest in digests {
        validate_nonempty_trimmed(&format!("{field}.digest_id"), &digest.digest_id)?;
        if !ids.insert(digest.digest_id.clone()) {
            return Err(GeoDiscoveryError::invalid(
                "Geo acquisition digests must have unique ids",
                [("digest_id", digest.digest_id.clone())],
            ));
        }
        validate_digest(field, digest)?;
    }
    Ok(())
}

fn validate_local_artifacts(artifacts: &[GeoLocalArtifactDigest]) -> Result<(), GeoDiscoveryError> {
    validate_nonempty_distinct("local_artifacts", artifacts)?;
    let mut ids = BTreeSet::new();
    for artifact in artifacts {
        validate_nonempty_trimmed("local_artifact.artifact_id", &artifact.artifact_id)?;
        if !ids.insert(artifact.artifact_id.clone()) {
            return Err(GeoDiscoveryError::invalid(
                "Geo acquisition local artifacts must have unique ids",
                [("artifact_id", artifact.artifact_id.clone())],
            ));
        }
        validate_nonempty_trimmed("local_artifact.media_type", &artifact.media_type)?;
        validate_digest("local_artifact.digest", &artifact.digest)?;
    }
    Ok(())
}

fn validate_matching_geography(
    left_field: &str,
    left: &GeoBoundedGeography,
    right_field: &str,
    right: &GeoBoundedGeography,
) -> Result<(), GeoDiscoveryError> {
    if left != right {
        return Err(GeoDiscoveryError::invalid(
            "Geo request bounded_geography must equal subset.geography",
            [
                (left_field.to_string(), left.geography_id.clone()),
                (right_field.to_string(), right.geography_id.clone()),
            ],
        ));
    }
    Ok(())
}

fn validate_digest(field: &str, digest: &GeoDigest) -> Result<(), GeoDiscoveryError> {
    validate_nonempty_trimmed(&format!("{field}.digest_id"), &digest.digest_id)?;
    let width = match digest.algorithm {
        GeoDigestAlgorithm::Blake3 | GeoDigestAlgorithm::Sha256 => 64,
        GeoDigestAlgorithm::Sha512 => 128,
    };
    if digest.hex_digest.len() != width
        || !digest
            .hex_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoDiscoveryError::invalid(
            "Geo acquisition digest fields must be lowercase fixed-width hex",
            [(field.to_string(), digest.hex_digest.clone())],
        ));
    }
    Ok(())
}

fn validate_discovery_request_id_format(field: &str, value: &str) -> Result<(), GeoDiscoveryError> {
    validate_request_id_format(field, value, CANON_GEO_DISCOVERY_REQUEST_VERSION)
}

fn validate_acquisition_request_id_format(
    field: &str,
    value: &str,
) -> Result<(), GeoDiscoveryError> {
    validate_request_id_format(field, value, CANON_GEO_ACQUISITION_REQUEST_VERSION)
}

fn validate_request_id_format(
    field: &str,
    value: &str,
    expected_version: &str,
) -> Result<(), GeoDiscoveryError> {
    validate_nonempty_trimmed(field, value)?;
    let expected_prefix = format!("{expected_version}:");
    let Some(hex_digest) = value.strip_prefix(&expected_prefix) else {
        return Err(GeoDiscoveryError::invalid(
            "Geo discovery/acquisition request ids must use the expected version prefix",
            [
                ("field", field.to_string()),
                ("expected_prefix", expected_prefix),
            ],
        ));
    };
    if hex_digest.len() != 64
        || !hex_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoDiscoveryError::invalid(
            "Geo discovery/acquisition request ids must end with lowercase 64-byte hex",
            [(field.to_string(), value.to_string())],
        ));
    }
    Ok(())
}

fn require_executor_ids(receipt: &GeoAcquisitionReceipt) -> Result<(), GeoDiscoveryError> {
    if receipt.executor.is_none() {
        return Err(GeoDiscoveryError::invalid(
            "Geo acquisition terminal state requires retained executor request/query ids",
            [("terminal_state", format!("{:?}", receipt.terminal_state))],
        ));
    }
    Ok(())
}

fn require_terminal_detail(receipt: &GeoAcquisitionReceipt) -> Result<(), GeoDiscoveryError> {
    match receipt.terminal_detail.as_deref() {
        Some(detail) if !detail.trim().is_empty() => Ok(()),
        _ => Err(GeoDiscoveryError::invalid(
            "Geo acquisition non-complete terminal state requires a terminal detail",
            [("terminal_state", format!("{:?}", receipt.terminal_state))],
        )),
    }
}

fn require_resume_or_retry_guidance(
    receipt: &GeoAcquisitionReceipt,
) -> Result<(), GeoDiscoveryError> {
    validate_nonempty_trimmed(
        "resumability.retry_guidance",
        &receipt.resumability.retry_guidance,
    )?;
    let has_resume = receipt
        .resumability
        .resume_token
        .as_deref()
        .is_some_and(|token| !token.trim().is_empty())
        || receipt
            .resumability
            .resume_request_id
            .as_deref()
            .is_some_and(|request_id| !request_id.trim().is_empty())
        || receipt
            .pagination
            .next_page_token
            .as_deref()
            .is_some_and(|token| !token.trim().is_empty());
    if !receipt.resumability.resumable || has_resume {
        Ok(())
    } else {
        Err(GeoDiscoveryError::invalid(
            "Geo acquisition resumable terminal states must retain a resume token, resume request id, or next page token",
            [("terminal_state", format!("{:?}", receipt.terminal_state))],
        ))
    }
}

fn validate_nonempty_distinct<T: Ord>(field: &str, values: &[T]) -> Result<(), GeoDiscoveryError> {
    if values.is_empty() {
        return Err(GeoDiscoveryError::invalid(
            "Geo discovery/acquisition collection must be non-empty",
            [(field.to_string(), "0".to_string())],
        ));
    }
    validate_distinct(field, values)
}

fn validate_distinct<T: Ord>(field: &str, values: &[T]) -> Result<(), GeoDiscoveryError> {
    if values.iter().collect::<BTreeSet<_>>().len() != values.len() {
        return Err(GeoDiscoveryError::invalid(
            "Geo discovery/acquisition collection must be distinct",
            [(field.to_string(), values.len().to_string())],
        ));
    }
    Ok(())
}

fn validate_nonempty_trimmed(field: &str, value: &str) -> Result<(), GeoDiscoveryError> {
    if value.is_empty() || value != value.trim() {
        return Err(GeoDiscoveryError::invalid(
            "Geo discovery/acquisition string fields must be non-empty and canonical-trimmed",
            [(field.to_string(), value.to_string())],
        ));
    }
    Ok(())
}

fn validate_utc_day(field: &str, value: &str) -> Result<(), GeoDiscoveryError> {
    validate_nonempty_trimmed(field, value)?;
    let bytes = value.as_bytes();
    let valid = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(GeoDiscoveryError::invalid(
            "Geo discovery/acquisition UTC day fields must be YYYY-MM-DD",
            [(field.to_string(), value.to_string())],
        ))
    }
}

fn looks_unbounded(expression: &str) -> bool {
    let lowered = expression.trim().to_ascii_lowercase();
    matches!(lowered.as_str(), "*" | "all" | "true" | "1=1")
        || lowered.contains("where true")
        || lowered.contains("where 1=1")
}

fn reject_secret_material<T: Serialize>(value: &T) -> Result<(), GeoDiscoveryError> {
    let json = serde_json::to_value(value).map_err(json_error)?;
    reject_secret_material_in_value("$", &json)
}

fn reject_secret_material_in_value(
    path: &str,
    value: &serde_json::Value,
) -> Result<(), GeoDiscoveryError> {
    match value {
        serde_json::Value::String(text) => {
            let lowered = text.to_ascii_lowercase();
            let has_secret = [
                "authorization:",
                "bearer ",
                "password=",
                "password:",
                "api_key",
                "apikey",
                "x-api-key",
                "secret=",
                "secret:",
                "aws_secret_access_key",
                "private_key",
                "credential=",
                "token=",
            ]
            .iter()
            .any(|marker| lowered.contains(marker));
            let has_protocol_endpoint =
                ["http://", "https://", "s3://", "snowflake://", "reveal://"]
                    .iter()
                    .any(|marker| lowered.contains(marker));
            if has_secret || has_protocol_endpoint {
                return Err(GeoDiscoveryError::secret(path, text));
            }
        }
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                reject_secret_material_in_value(&format!("{path}[{index}]"), item)?;
            }
        }
        serde_json::Value::Object(object) => {
            for (key, child) in object {
                reject_secret_material_in_value(&format!("{path}.{key}"), child)?;
            }
        }
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) | serde_json::Value::Null => {}
    }
    Ok(())
}

fn redact_secretish_value(value: &str) -> String {
    if value.len() <= 8 {
        "<redacted>".to_string()
    } else {
        let prefix = value.chars().take(4).collect::<String>();
        format!("{prefix}<redacted>")
    }
}

fn blake3_hash(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn json_error(error: serde_json::Error) -> GeoDiscoveryError {
    GeoDiscoveryError::new(
        GeoDiscoveryErrorCode::InvalidInput,
        "Geo discovery/acquisition JSON serialization failed",
        [("error", error.to_string())],
    )
}

fn receipt_mismatch(
    message: impl Into<String>,
    field: &str,
    expected: &str,
    actual: &str,
) -> GeoDiscoveryError {
    GeoDiscoveryError::new(
        GeoDiscoveryErrorCode::ReceiptMismatch,
        message,
        [
            ("field", field.to_string()),
            ("expected", expected.to_string()),
            ("actual", actual.to_string()),
        ],
    )
}
