// Offline satisfaction of Geo acquisition requests with local receipt files.
//
// This module validates explicit `REQUEST_ID=RECEIPT.json` handoffs. It does
// not execute acquisition, read credentials, contact catalogs, or put local
// paths and executor protocol details into semantic satisfaction hashes.

use super::{
    CANON_GEO_ACQUISITION_RECEIPT_VERSION, CANON_GEO_HOME_CELL_ROWS_VERSION,
    CANON_GEO_PLAN_VERSION, CANON_GEO_TILE_WORK_REQUEST_VERSION, CANON_GEO_WAREHOUSE_ROWS_VERSION,
    GEO_COMPILE_EVIDENCE_COMMAND, GEO_MATERIALIZE_EVIDENCE_COMMAND,
    GEO_MATERIALIZE_HOME_CELLS_COMMAND, GEO_REQUEST_BINDING_ID, GEO_ROWS_BINDING_ID,
    GEO_RUN_JSON_MEDIA_TYPE, GEO_SOLVE_COMMAND, GEO_TILE_WORK_COMMAND,
    GeoAcquisitionArtifactReleaseRelation, GeoAcquisitionDenominator, GeoAcquisitionProofClass,
    GeoAcquisitionReceipt, GeoAcquisitionRequest, GeoAcquisitionTerminalState, GeoBoundedGeography,
    GeoBoundedSubset, GeoDigest, GeoDigestAlgorithm, GeoDiscoveryError, GeoDiscoveryErrorCode,
    GeoLocalArtifactDigest, GeoLocalArtifactRef, GeoNativeEntityScope, GeoPlan,
    GeoPlanAcquisitionHandoff, GeoPlanExternalRequest, GeoRegionalInventory,
    GeoRegionalSourceInstance, GeoRunInputBinding, GeoSourceAvailability, GeoSourceRelease,
    GeoSubsetPredicateKind, GeoWarehouseRowsRequest, canonicalize_geo_acquisition_request,
    canonicalize_regional_inventory, geo_acquisition_receipt_satisfies_positive_gate,
    geo_acquisition_request_id, geo_acquisition_request_semantic_hash, geo_run_input_artifact_id,
    materialize_warehouse_rows, regional_inventory_planning_hash, regional_inventory_semantic_hash,
    validate_geo_acquisition_receipt, validate_geo_plan,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

pub const CANON_GEO_ACQUISITION_SATISFACTION_VERSION: &str =
    "canon_geo_acquisition_satisfaction.v0";
pub const CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION: &str =
    "canon_geo_regional_inventory_advancement.v0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoSatisfactionAssignment {
    pub request_id: String,
    pub receipt_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoSatisfactionFileBinding {
    pub binding_id: String,
    pub path: PathBuf,
}

pub type GeoSatisfactionArtifactReleaseRelation = GeoAcquisitionArtifactReleaseRelation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoSatisfactionInput<'a> {
    pub plan: &'a GeoPlan,
    pub inventory: Option<&'a GeoRegionalInventory>,
    pub assignment: GeoSatisfactionAssignment,
    pub local_artifact_files: Vec<GeoSatisfactionFileBinding>,
    pub result_digest_files: Vec<GeoSatisfactionFileBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoSatisfactionRunInputFileBinding {
    pub local_artifact_id: String,
    pub node_id: String,
    pub binding_id: String,
    pub contract_version: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoSatisfactionRunInput<'a> {
    pub plan: &'a GeoPlan,
    pub inventory: Option<&'a GeoRegionalInventory>,
    pub assignment: GeoSatisfactionAssignment,
    pub run_input_files: Vec<GeoSatisfactionRunInputFileBinding>,
    pub result_digest_files: Vec<GeoSatisfactionFileBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoAcquisitionRunSatisfaction {
    pub satisfaction: GeoAcquisitionSatisfaction,
    pub run_input_bindings: Vec<GeoRunInputBinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoSatisfactionStatus {
    Satisfied,
    NotSatisfied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoSatisfactionFindingCode {
    Satisfied,
    ZeroRows,
    Timeout,
    Canceled,
    Partial,
    UnreadableColumns,
    PositiveGateNotMet,
    ArtifactReleaseRelationAmbiguous,
    InventoryAdvancementUnsupportedArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoSatisfactionFinding {
    pub code: GeoSatisfactionFindingCode,
    pub detail: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoSatisfactionFileAudit {
    pub file_id: String,
    pub byte_count: u64,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoSatisfactionLocalInputBinding {
    pub binding_id: String,
    pub request_id: String,
    pub request_semantic_hash: String,
    pub receipt_terminal_state: GeoAcquisitionTerminalState,
    pub proof_class: GeoAcquisitionProofClass,
    pub source_instance_id: String,
    pub release_id: String,
    pub release_digest: String,
    pub local_artifact_id: String,
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_contract_version: Option<String>,
    pub content_hash: String,
    pub byte_count: u64,
    pub result_digest_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoSatisfactionRunInputRef {
    pub node_id: String,
    pub binding_id: String,
    pub artifact_id: String,
    pub contract_version: String,
    pub media_type: String,
    pub content_hash: String,
    pub byte_count: u64,
    pub local_artifact_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoInventoryAdvancementEffect {
    LocalAvailabilityOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoSatisfactionExecutionRef {
    pub proof_class: GeoAcquisitionProofClass,
    pub terminal_state: GeoAcquisitionTerminalState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixture_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_receipt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_query_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_attempt_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRegionalInventorySourceAdvancement {
    pub source_instance_id: String,
    pub release: GeoSourceRelease,
    pub previous_state: GeoSourceAvailability,
    pub advanced_state: GeoSourceAvailability,
    pub local_ref: GeoLocalArtifactRef,
    pub local_artifact_byte_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_artifact_contract_version: Option<String>,
    pub result_digest_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoRegionalInventoryAdvancement {
    pub version: String,
    pub advancement_id: String,
    pub semantic_hash: String,
    pub effect: GeoInventoryAdvancementEffect,
    pub plan_id: String,
    pub plan_semantic_hash: String,
    pub request_id: String,
    pub request_semantic_hash: String,
    pub base_inventory_id: String,
    pub base_inventory_semantic_hash: String,
    pub advanced_inventory_id: String,
    pub advanced_inventory_semantic_hash: String,
    pub bounded_geography: GeoBoundedGeography,
    pub bounded_subset: GeoBoundedSubset,
    pub bounded_subset_hash: String,
    pub receipt_file: GeoSatisfactionFileAudit,
    pub receipt_execution: GeoSatisfactionExecutionRef,
    pub receipt_terminal_state: GeoAcquisitionTerminalState,
    pub proof_class: GeoAcquisitionProofClass,
    pub denominators: Vec<GeoAcquisitionDenominator>,
    pub source_digests: Vec<GeoDigest>,
    pub result_digests: Vec<GeoDigest>,
    pub source_advancements: Vec<GeoRegionalInventorySourceAdvancement>,
    pub advanced_inventory: GeoRegionalInventory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAcquisitionSatisfaction {
    pub version: String,
    pub satisfaction_id: String,
    pub semantic_hash: String,
    pub status: GeoSatisfactionStatus,
    pub request_id: String,
    pub request_semantic_hash: String,
    pub expected_receipt_contract: String,
    pub receipt_file: GeoSatisfactionFileAudit,
    pub local_artifacts: Vec<GeoSatisfactionFileAudit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub result_files: Vec<GeoSatisfactionFileAudit>,
    pub source_digests: Vec<GeoDigest>,
    pub result_digests: Vec<GeoDigest>,
    pub denominators: Vec<GeoAcquisitionDenominator>,
    pub bindings: Vec<GeoSatisfactionLocalInputBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run_input_refs: Vec<GeoSatisfactionRunInputRef>,
    pub receipt_execution: GeoSatisfactionExecutionRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory_advancement: Option<GeoRegionalInventoryAdvancement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_inventory: Option<GeoRegionalInventory>,
    pub findings: Vec<GeoSatisfactionFinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoSatisfyErrorCode {
    InvalidInput,
    UnsupportedVersion,
    RequestNotFound,
    AmbiguousRequest,
    ContractMismatch,
    ReceiptMismatch,
    MissingFileBinding,
    FileRead,
    FileDigestMismatch,
    FileByteCountMismatch,
    Serialization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoSatisfyError {
    pub code: GeoSatisfyErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoSatisfyError {
    fn new(
        code: GeoSatisfyErrorCode,
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
}

impl fmt::Display for GeoSatisfyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeoSatisfyError {}

#[derive(Serialize)]
struct GeoSatisfactionSemanticProjection<'a> {
    version: &'a str,
    status: GeoSatisfactionStatus,
    request_id: &'a str,
    request_semantic_hash: &'a str,
    expected_receipt_contract: &'a str,
    terminal_state: GeoAcquisitionTerminalState,
    proof_class: GeoAcquisitionProofClass,
    releases: Vec<GeoSatisfactionReleaseProjection<'a>>,
    bounded_subset_hash: String,
    fields_hash: String,
    projection_hash: String,
    pagination_terminal_hash: String,
    counts_hash: String,
    denominators: Vec<GeoAcquisitionDenominator>,
    normalized_executed_request_digest: GeoDigest,
    source_digests: Vec<GeoDigest>,
    result_digests: Vec<GeoDigest>,
    local_artifact_digests: Vec<GeoSatisfactionArtifactProjection<'a>>,
    bindings: Vec<GeoSatisfactionBindingProjection<'a>>,
    run_input_refs: Vec<GeoSatisfactionRunInputRef>,
    inventory_advancement_hash: Option<&'a str>,
    findings: &'a [GeoSatisfactionFinding],
}

#[derive(Serialize)]
struct GeoRegionalInventoryAdvancementSemanticProjection<'a> {
    version: &'a str,
    effect: GeoInventoryAdvancementEffect,
    plan_id: &'a str,
    plan_semantic_hash: &'a str,
    request_id: &'a str,
    request_semantic_hash: &'a str,
    base_inventory_id: &'a str,
    base_inventory_semantic_hash: &'a str,
    advanced_inventory_id: &'a str,
    advanced_inventory_semantic_hash: &'a str,
    bounded_geography: &'a GeoBoundedGeography,
    bounded_subset_hash: &'a str,
    receipt_terminal_state: GeoAcquisitionTerminalState,
    proof_class: GeoAcquisitionProofClass,
    denominators: &'a [GeoAcquisitionDenominator],
    source_digests: &'a [GeoDigest],
    result_digests: &'a [GeoDigest],
    source_advancements: &'a [GeoRegionalInventorySourceAdvancement],
}

#[derive(Serialize)]
struct GeoSatisfactionReleaseProjection<'a> {
    release_id: &'a str,
    release_digest: &'a super::GeoDigest,
}

#[derive(Serialize)]
struct GeoSatisfactionArtifactProjection<'a> {
    artifact_id: &'a str,
    media_type: &'a str,
    byte_count: u64,
    digest: &'a super::GeoDigest,
}

#[derive(Serialize)]
struct GeoSatisfactionBindingProjection<'a> {
    binding_id: &'a str,
    request_semantic_hash: &'a str,
    receipt_terminal_state: GeoAcquisitionTerminalState,
    proof_class: GeoAcquisitionProofClass,
    release_id: &'a str,
    release_digest: &'a str,
    local_artifact_id: &'a str,
    media_type: &'a str,
    artifact_contract_version: Option<&'a str>,
    content_hash: &'a str,
    byte_count: u64,
    result_digest_ids: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileFingerprint {
    byte_count: u64,
    digest: String,
    bytes: Vec<u8>,
}

pub fn parse_geo_satisfaction_assignment(
    value: &str,
) -> Result<GeoSatisfactionAssignment, GeoSatisfyError> {
    let (request_id, receipt_path) = value.split_once('=').ok_or_else(|| {
        GeoSatisfyError::new(
            GeoSatisfyErrorCode::InvalidInput,
            "Geo satisfaction input must be REQUEST_ID=RECEIPT.json",
            [("input", value)],
        )
    })?;
    if request_id.is_empty()
        || receipt_path.is_empty()
        || request_id.trim() != request_id
        || receipt_path.trim() != receipt_path
        || receipt_path.contains('=')
    {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::InvalidInput,
            "Geo satisfaction input must contain one trimmed request id and one receipt path",
            [("input", value)],
        ));
    }
    Ok(GeoSatisfactionAssignment {
        request_id: request_id.to_string(),
        receipt_path: PathBuf::from(receipt_path),
    })
}

pub fn satisfy_geo_acquisition(
    input: GeoSatisfactionInput<'_>,
) -> Result<GeoAcquisitionSatisfaction, GeoSatisfyError> {
    satisfy_geo_acquisition_with_relations(input, Vec::new())
}

pub fn satisfy_geo_acquisition_with_relations(
    input: GeoSatisfactionInput<'_>,
    artifact_release_relations: Vec<GeoSatisfactionArtifactReleaseRelation>,
) -> Result<GeoAcquisitionSatisfaction, GeoSatisfyError> {
    satisfy_geo_acquisition_core(input, &artifact_release_relations).map(|core| core.satisfaction)
}

pub fn satisfy_geo_acquisition_for_run(
    input: GeoSatisfactionRunInput<'_>,
) -> Result<GeoAcquisitionRunSatisfaction, GeoSatisfyError> {
    satisfy_geo_acquisition_for_run_with_relations(input, Vec::new())
}

pub fn satisfy_geo_acquisition_for_run_with_relations(
    input: GeoSatisfactionRunInput<'_>,
    artifact_release_relations: Vec<GeoSatisfactionArtifactReleaseRelation>,
) -> Result<GeoAcquisitionRunSatisfaction, GeoSatisfyError> {
    let local_artifact_files = input
        .run_input_files
        .iter()
        .map(|binding| GeoSatisfactionFileBinding {
            binding_id: binding.local_artifact_id.clone(),
            path: binding.path.clone(),
        })
        .collect::<Vec<_>>();
    let core = satisfy_geo_acquisition_core(
        GeoSatisfactionInput {
            plan: input.plan,
            inventory: input.inventory,
            assignment: input.assignment,
            local_artifact_files,
            result_digest_files: input.result_digest_files,
        },
        &artifact_release_relations,
    )?;
    let mut satisfaction = core.satisfaction;
    let mut run_input_bindings = Vec::new();
    if satisfaction.status == GeoSatisfactionStatus::Satisfied {
        let (bindings, refs) =
            build_run_input_bindings(input.plan, &core.local_artifacts, &input.run_input_files)?;
        satisfaction.run_input_refs = refs;
        satisfaction.semantic_hash =
            geo_acquisition_satisfaction_semantic_hash(&satisfaction, &core.receipt)?;
        satisfaction.satisfaction_id = satisfaction_id(&satisfaction.semantic_hash);
        run_input_bindings = bindings;
    }
    Ok(GeoAcquisitionRunSatisfaction {
        satisfaction,
        run_input_bindings,
    })
}

struct GeoSatisfactionCore {
    satisfaction: GeoAcquisitionSatisfaction,
    receipt: GeoAcquisitionReceipt,
    local_artifacts: BTreeMap<String, (GeoLocalArtifactDigest, FileFingerprint)>,
}

fn satisfy_geo_acquisition_core(
    input: GeoSatisfactionInput<'_>,
    artifact_release_relations: &[GeoSatisfactionArtifactReleaseRelation],
) -> Result<GeoSatisfactionCore, GeoSatisfyError> {
    if input.plan.version != CANON_GEO_PLAN_VERSION {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::UnsupportedVersion,
            "unsupported Geo plan version",
            [("version", input.plan.version.as_str())],
        ));
    }

    let (request, handoff) = find_plan_acquisition(input.plan, &input.assignment.request_id)?;
    validate_acquisition_handoff(request, handoff)?;
    let expected_request_id = geo_acquisition_request_id(request).map_err(discovery_error)?;
    if input.assignment.request_id != expected_request_id {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ReceiptMismatch,
            "Geo satisfaction request id does not match the acquisition request semantic id",
            [
                ("expected", expected_request_id),
                ("actual", input.assignment.request_id.clone()),
            ],
        ));
    }
    let request_semantic_hash =
        geo_acquisition_request_semantic_hash(request).map_err(discovery_error)?;

    let receipt_bytes = read_file(&input.assignment.receipt_path, "receipt")?;
    let receipt: GeoAcquisitionReceipt =
        serde_json::from_slice(&receipt_bytes).map_err(|error| {
            GeoSatisfyError::new(
                GeoSatisfyErrorCode::InvalidInput,
                "Geo acquisition receipt JSON could not be parsed",
                [
                    ("path", input.assignment.receipt_path.display().to_string()),
                    ("error", error.to_string()),
                ],
            )
        })?;
    validate_geo_acquisition_receipt(request, &receipt).map_err(discovery_error)?;
    if receipt.request_semantic_hash != request_semantic_hash {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ReceiptMismatch,
            "Geo receipt request semantic hash does not match the plan request",
            [
                ("expected", request_semantic_hash.as_str()),
                ("actual", receipt.request_semantic_hash.as_str()),
            ],
        ));
    }

    validate_receipt_digest_algorithms(&receipt, handoff)?;
    let receipt_file = GeoSatisfactionFileAudit {
        file_id: "receipt".to_string(),
        byte_count: u64::try_from(receipt_bytes.len())
            .map_err(|_| byte_count_overflow("receipt"))?,
        digest: blake3_prefixed(&receipt_bytes),
    };

    let local_artifact_files =
        validate_local_artifact_files(&receipt, &input.local_artifact_files)?;
    let result_files =
        validate_result_files(&receipt, &input.result_digest_files, &local_artifact_files)?;
    validate_receipt_byte_count(&receipt, &local_artifact_files)?;

    let positive_gate = geo_acquisition_receipt_satisfies_positive_gate(request, &receipt);
    let binding_result = if positive_gate || !artifact_release_relations.is_empty() {
        build_bindings_with_relations(
            request,
            &receipt,
            &local_artifact_files,
            artifact_release_relations,
        )?
    } else {
        Some(Vec::new())
    };
    let artifact_release_relation_ambiguous = positive_gate && binding_result.is_none();
    let status = if positive_gate && binding_result.is_some() {
        GeoSatisfactionStatus::Satisfied
    } else {
        GeoSatisfactionStatus::NotSatisfied
    };
    let mut findings = if artifact_release_relation_ambiguous {
        Vec::new()
    } else {
        findings_for_receipt(request, &receipt, positive_gate)
    };
    let bindings = if status == GeoSatisfactionStatus::Satisfied {
        binding_result.unwrap_or_default()
    } else {
        Vec::new()
    };
    if artifact_release_relation_ambiguous {
        findings.push(finding(
            GeoSatisfactionFindingCode::ArtifactReleaseRelationAmbiguous,
            [
                ("release_count", receipt.releases.len().to_string()),
                (
                    "local_artifact_count",
                    receipt.local_artifacts.len().to_string(),
                ),
            ],
        ));
        findings.sort();
        findings.dedup();
    }
    let inventory_advancement = if status == GeoSatisfactionStatus::Satisfied
        && receipt.proof_class == GeoAcquisitionProofClass::Live
    {
        if let Some(inventory) = input.inventory {
            if inventory_advancement_artifact_is_usable(&bindings, &local_artifact_files)? {
                Some(build_inventory_advancement(
                    input.plan,
                    inventory,
                    request,
                    &receipt,
                    &receipt_file,
                    &bindings,
                )?)
            } else {
                let binding = bindings.first();
                findings.push(finding(
                    GeoSatisfactionFindingCode::InventoryAdvancementUnsupportedArtifact,
                    [
                        (
                            "expected_contract",
                            CANON_GEO_WAREHOUSE_ROWS_VERSION.to_string(),
                        ),
                        ("expected_media_type", GEO_RUN_JSON_MEDIA_TYPE.to_string()),
                        (
                            "actual_contract",
                            binding
                                .and_then(|binding| binding.artifact_contract_version.clone())
                                .unwrap_or_else(|| "untyped".to_string()),
                        ),
                        (
                            "actual_media_type",
                            binding
                                .map(|binding| binding.media_type.clone())
                                .unwrap_or_else(|| "none".to_string()),
                        ),
                    ],
                ));
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    findings.sort();
    findings.dedup();
    let updated_inventory = inventory_advancement
        .as_ref()
        .map(|advancement| advancement.advanced_inventory.clone());

    let mut satisfaction = GeoAcquisitionSatisfaction {
        version: CANON_GEO_ACQUISITION_SATISFACTION_VERSION.to_string(),
        satisfaction_id: String::new(),
        semantic_hash: String::new(),
        status,
        request_id: input.assignment.request_id,
        request_semantic_hash,
        expected_receipt_contract: handoff.expected_receipt_contract.clone(),
        receipt_file,
        local_artifacts: file_audits(&local_artifact_files),
        result_files,
        source_digests: sorted_vec(receipt.source_digests.clone()),
        result_digests: sorted_vec(receipt.result_digests.clone()),
        denominators: sorted_vec(receipt.denominators.clone()),
        bindings,
        run_input_refs: Vec::new(),
        receipt_execution: receipt_execution_ref(&receipt),
        inventory_advancement,
        updated_inventory,
        findings,
    };
    satisfaction.semantic_hash =
        geo_acquisition_satisfaction_semantic_hash(&satisfaction, &receipt)?;
    satisfaction.satisfaction_id = satisfaction_id(&satisfaction.semantic_hash);
    Ok(GeoSatisfactionCore {
        satisfaction,
        receipt,
        local_artifacts: local_artifact_files,
    })
}

pub fn geo_acquisition_satisfaction_semantic_hash(
    satisfaction: &GeoAcquisitionSatisfaction,
    receipt: &GeoAcquisitionReceipt,
) -> Result<String, GeoSatisfyError> {
    let fields = sorted_vec(receipt.fields.clone());
    let denominators = sorted_vec(receipt.denominators.clone());
    let source_digests = sorted_vec(receipt.source_digests.clone());
    let result_digests = sorted_vec(receipt.result_digests.clone());
    let mut bindings = satisfaction
        .bindings
        .iter()
        .map(|binding| GeoSatisfactionBindingProjection {
            binding_id: &binding.binding_id,
            request_semantic_hash: &binding.request_semantic_hash,
            receipt_terminal_state: binding.receipt_terminal_state,
            proof_class: binding.proof_class,
            release_id: &binding.release_id,
            release_digest: &binding.release_digest,
            local_artifact_id: &binding.local_artifact_id,
            media_type: &binding.media_type,
            artifact_contract_version: binding.artifact_contract_version.as_deref(),
            content_hash: &binding.content_hash,
            byte_count: binding.byte_count,
            result_digest_ids: &binding.result_digest_ids,
        })
        .collect::<Vec<_>>();
    bindings.sort_by(|left, right| {
        (
            left.binding_id,
            left.release_id,
            left.local_artifact_id,
            left.content_hash,
        )
            .cmp(&(
                right.binding_id,
                right.release_id,
                right.local_artifact_id,
                right.content_hash,
            ))
    });
    let mut releases = receipt
        .releases
        .iter()
        .map(|release| GeoSatisfactionReleaseProjection {
            release_id: &release.release_id,
            release_digest: &release.release_digest,
        })
        .collect::<Vec<_>>();
    releases.sort_by(|left, right| {
        (left.release_id, left.release_digest).cmp(&(right.release_id, right.release_digest))
    });
    let mut local_artifacts = receipt
        .local_artifacts
        .iter()
        .map(|artifact| GeoSatisfactionArtifactProjection {
            artifact_id: &artifact.artifact_id,
            media_type: &artifact.media_type,
            byte_count: artifact.byte_count,
            digest: &artifact.digest,
        })
        .collect::<Vec<_>>();
    local_artifacts.sort_by(|left, right| {
        (
            left.artifact_id,
            left.media_type,
            left.byte_count,
            left.digest,
        )
            .cmp(&(
                right.artifact_id,
                right.media_type,
                right.byte_count,
                right.digest,
            ))
    });
    digest_json(&GeoSatisfactionSemanticProjection {
        version: &satisfaction.version,
        status: satisfaction.status,
        request_id: &satisfaction.request_id,
        request_semantic_hash: &satisfaction.request_semantic_hash,
        expected_receipt_contract: &satisfaction.expected_receipt_contract,
        terminal_state: receipt.terminal_state,
        proof_class: receipt.proof_class,
        releases,
        bounded_subset_hash: digest_json(&receipt.subset)?,
        fields_hash: digest_json(&fields)?,
        projection_hash: digest_json(&receipt.projection)?,
        pagination_terminal_hash: digest_json(&receipt.pagination)?,
        counts_hash: digest_json(&receipt.counts)?,
        denominators,
        normalized_executed_request_digest: receipt.normalized_executed_request_digest.clone(),
        source_digests,
        result_digests,
        local_artifact_digests: local_artifacts,
        bindings,
        run_input_refs: sorted_vec(satisfaction.run_input_refs.clone()),
        inventory_advancement_hash: satisfaction
            .inventory_advancement
            .as_ref()
            .map(|advancement| advancement.semantic_hash.as_str()),
        findings: &satisfaction.findings,
    })
}

fn find_plan_acquisition<'a>(
    plan: &'a GeoPlan,
    request_id: &str,
) -> Result<(&'a GeoAcquisitionRequest, &'a GeoPlanAcquisitionHandoff), GeoSatisfyError> {
    let matches = plan
        .external_requests
        .iter()
        .filter_map(|external| match external {
            GeoPlanExternalRequest::Acquisition { request, handoff }
                if request.request_id == request_id =>
            {
                Some((request, handoff))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [(request, handoff)] => Ok((*request, *handoff)),
        [] => Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::RequestNotFound,
            "Geo plan does not contain a matching acquisition request",
            [("request_id", request_id)],
        )),
        _ => Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::AmbiguousRequest,
            "Geo plan contains more than one acquisition request with the same id",
            [("request_id", request_id)],
        )),
    }
}

fn validate_acquisition_handoff(
    request: &GeoAcquisitionRequest,
    handoff: &GeoPlanAcquisitionHandoff,
) -> Result<(), GeoSatisfyError> {
    if handoff.expected_receipt_contract != CANON_GEO_ACQUISITION_RECEIPT_VERSION {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo acquisition handoff expects a receipt contract that this satisfier cannot validate",
            [
                ("expected", CANON_GEO_ACQUISITION_RECEIPT_VERSION),
                ("actual", handoff.expected_receipt_contract.as_str()),
            ],
        ));
    }
    if handoff.required_result_digest_algorithm != GeoDigestAlgorithm::Blake3 {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo acquisition satisfaction requires BLAKE3 result digest handoffs",
            [(
                "algorithm",
                format!("{:?}", handoff.required_result_digest_algorithm),
            )],
        ));
    }
    let expected = geo_acquisition_request_id(request).map_err(discovery_error)?;
    if request.request_id != expected {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo plan acquisition request id does not match its semantic content",
            [
                ("expected", expected),
                ("actual", request.request_id.clone()),
            ],
        ));
    }
    Ok(())
}

fn validate_receipt_digest_algorithms(
    receipt: &GeoAcquisitionReceipt,
    handoff: &GeoPlanAcquisitionHandoff,
) -> Result<(), GeoSatisfyError> {
    for digest in &receipt.result_digests {
        if digest.algorithm != handoff.required_result_digest_algorithm {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo receipt result digests do not use the handoff-required algorithm",
                [
                    ("digest_id".to_string(), digest.digest_id.clone()),
                    ("algorithm".to_string(), format!("{:?}", digest.algorithm)),
                ],
            ));
        }
    }
    for artifact in &receipt.local_artifacts {
        if artifact.digest.algorithm != GeoDigestAlgorithm::Blake3 {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo local artifact digests must use BLAKE3 for local satisfaction",
                [
                    ("artifact_id".to_string(), artifact.artifact_id.clone()),
                    (
                        "algorithm".to_string(),
                        format!("{:?}", artifact.digest.algorithm),
                    ),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_local_artifact_files(
    receipt: &GeoAcquisitionReceipt,
    bindings: &[GeoSatisfactionFileBinding],
) -> Result<BTreeMap<String, (GeoLocalArtifactDigest, FileFingerprint)>, GeoSatisfyError> {
    let binding_by_id = binding_map("local_artifact_files", bindings)?;
    let mut checked = BTreeMap::new();
    for artifact in &receipt.local_artifacts {
        let path = binding_by_id.get(&artifact.artifact_id).ok_or_else(|| {
            GeoSatisfyError::new(
                GeoSatisfyErrorCode::MissingFileBinding,
                "Geo satisfaction is missing a local artifact file binding",
                [("artifact_id", artifact.artifact_id.as_str())],
            )
        })?;
        let fingerprint = fingerprint_file(path, &artifact.artifact_id)?;
        let expected_digest = format!("blake3:{}", artifact.digest.hex_digest);
        if fingerprint.digest != expected_digest {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::FileDigestMismatch,
                "Geo local artifact file digest does not match the receipt",
                [
                    ("artifact_id", artifact.artifact_id.as_str()),
                    ("expected", expected_digest.as_str()),
                    ("actual", fingerprint.digest.as_str()),
                ],
            ));
        }
        if fingerprint.byte_count != artifact.byte_count {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::FileByteCountMismatch,
                "Geo local artifact file byte count does not match the receipt",
                [
                    ("artifact_id".to_string(), artifact.artifact_id.clone()),
                    ("expected".to_string(), artifact.byte_count.to_string()),
                    ("actual".to_string(), fingerprint.byte_count.to_string()),
                ],
            ));
        }
        checked.insert(
            artifact.artifact_id.clone(),
            (artifact.clone(), fingerprint),
        );
    }
    Ok(checked)
}

fn validate_result_files(
    receipt: &GeoAcquisitionReceipt,
    bindings: &[GeoSatisfactionFileBinding],
    local_artifacts: &BTreeMap<String, (GeoLocalArtifactDigest, FileFingerprint)>,
) -> Result<Vec<GeoSatisfactionFileAudit>, GeoSatisfyError> {
    let binding_by_id = binding_map("result_digest_files", bindings)?;
    let local_digests = local_artifacts
        .values()
        .map(|(_, fingerprint)| fingerprint.digest.as_str())
        .collect::<BTreeSet<_>>();
    let mut audits = Vec::new();
    let mut explicit_result_bytes = 0_u64;
    for digest in &receipt.result_digests {
        let expected = format!("blake3:{}", digest.hex_digest);
        if let Some(path) = binding_by_id.get(&digest.digest_id) {
            let fingerprint = fingerprint_file(path, &digest.digest_id)?;
            if fingerprint.digest != expected {
                return Err(GeoSatisfyError::new(
                    GeoSatisfyErrorCode::FileDigestMismatch,
                    "Geo result file digest does not match the receipt",
                    [
                        ("digest_id", digest.digest_id.as_str()),
                        ("expected", expected.as_str()),
                        ("actual", fingerprint.digest.as_str()),
                    ],
                ));
            }
            explicit_result_bytes = explicit_result_bytes
                .checked_add(fingerprint.byte_count)
                .ok_or_else(|| byte_count_overflow("result_digest_files"))?;
            audits.push(GeoSatisfactionFileAudit {
                file_id: digest.digest_id.clone(),
                byte_count: fingerprint.byte_count,
                digest: fingerprint.digest,
            });
        } else if !local_digests.contains(expected.as_str()) {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::MissingFileBinding,
                "Geo satisfaction cannot prove a result digest from local artifacts or a result file binding",
                [("digest_id", digest.digest_id.as_str())],
            ));
        }
    }
    if !audits.is_empty() && explicit_result_bytes != receipt.counts.bytes {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::FileByteCountMismatch,
            "Geo result file bytes do not match the receipt byte count",
            [
                ("expected", receipt.counts.bytes.to_string()),
                ("actual", explicit_result_bytes.to_string()),
            ],
        ));
    }
    audits.sort();
    Ok(audits)
}

fn validate_receipt_byte_count(
    receipt: &GeoAcquisitionReceipt,
    local_artifacts: &BTreeMap<String, (GeoLocalArtifactDigest, FileFingerprint)>,
) -> Result<(), GeoSatisfyError> {
    let total = local_artifacts
        .values()
        .try_fold(0_u64, |total, (_, fingerprint)| {
            total
                .checked_add(fingerprint.byte_count)
                .ok_or_else(|| byte_count_overflow("local_artifact_files"))
        })?;
    if total != receipt.counts.bytes {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::FileByteCountMismatch,
            "Geo local artifact bytes do not match the receipt byte count",
            [
                ("expected", receipt.counts.bytes.to_string()),
                ("actual", total.to_string()),
            ],
        ));
    }
    Ok(())
}

fn build_run_input_bindings(
    plan: &GeoPlan,
    local_artifacts: &BTreeMap<String, (GeoLocalArtifactDigest, FileFingerprint)>,
    targets: &[GeoSatisfactionRunInputFileBinding],
) -> Result<(Vec<GeoRunInputBinding>, Vec<GeoSatisfactionRunInputRef>), GeoSatisfyError> {
    let mut seen_targets = BTreeSet::new();
    let mut seen_artifacts = BTreeSet::new();
    let mut bindings = Vec::with_capacity(targets.len());
    let mut refs = Vec::with_capacity(targets.len());
    let mut sorted_targets = targets.iter().collect::<Vec<_>>();
    sorted_targets.sort_by(|left, right| {
        (
            left.node_id.as_str(),
            left.binding_id.as_str(),
            left.local_artifact_id.as_str(),
        )
            .cmp(&(
                right.node_id.as_str(),
                right.binding_id.as_str(),
                right.local_artifact_id.as_str(),
            ))
    });

    for target in sorted_targets {
        validate_run_input_target(target)?;
        if !seen_targets.insert((target.node_id.clone(), target.binding_id.clone())) {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::InvalidInput,
                "Geo run input targets must have distinct node and binding ids",
                [
                    ("node_id", target.node_id.as_str()),
                    ("binding_id", target.binding_id.as_str()),
                ],
            ));
        }
        if !seen_artifacts.insert(target.local_artifact_id.clone()) {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::InvalidInput,
                "Geo run input targets must map each receipt local artifact at most once",
                [("local_artifact_id", target.local_artifact_id.as_str())],
            ));
        }
        validate_run_input_target_against_plan(plan, target)?;
        let Some((artifact, fingerprint)) = local_artifacts.get(&target.local_artifact_id) else {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::MissingFileBinding,
                "Geo run input target does not reference a validated receipt local artifact",
                [("local_artifact_id", target.local_artifact_id.as_str())],
            ));
        };
        if artifact.media_type != GEO_RUN_JSON_MEDIA_TYPE {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo run input target requires an application/json local artifact",
                [
                    ("local_artifact_id", target.local_artifact_id.as_str()),
                    ("media_type", artifact.media_type.as_str()),
                ],
            ));
        }
        let actual_contract = json_artifact_version(&fingerprint.bytes, &target.local_artifact_id)?;
        if actual_contract != target.contract_version {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo run input target contract does not match local artifact JSON version",
                [
                    ("local_artifact_id", target.local_artifact_id.as_str()),
                    ("expected", target.contract_version.as_str()),
                    ("actual", actual_contract.as_str()),
                ],
            ));
        }

        let binding = GeoRunInputBinding::from_bytes(
            target.node_id.clone(),
            target.binding_id.clone(),
            target.contract_version.clone(),
            fingerprint.bytes.clone(),
        );
        if binding.content_digest != fingerprint.digest
            || binding.byte_count != fingerprint.byte_count
        {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::FileDigestMismatch,
                "Geo run input binding bytes do not match the validated local artifact",
                [
                    ("local_artifact_id", target.local_artifact_id.as_str()),
                    ("expected_digest", fingerprint.digest.as_str()),
                    ("actual_digest", binding.content_digest.as_str()),
                ],
            ));
        }
        refs.push(GeoSatisfactionRunInputRef {
            node_id: binding.node_id.clone(),
            binding_id: binding.binding_id.clone(),
            artifact_id: binding.artifact_id.clone(),
            contract_version: binding.contract_version.clone(),
            media_type: binding.media_type.clone(),
            content_hash: binding.content_digest.clone(),
            byte_count: binding.byte_count,
            local_artifact_id: target.local_artifact_id.clone(),
        });
        bindings.push(binding);
    }

    bindings.sort_by(|left, right| {
        (left.node_id.as_str(), left.binding_id.as_str())
            .cmp(&(right.node_id.as_str(), right.binding_id.as_str()))
    });
    refs.sort();
    Ok((bindings, refs))
}

fn build_bindings_with_relations(
    request: &GeoAcquisitionRequest,
    receipt: &GeoAcquisitionReceipt,
    local_artifacts: &BTreeMap<String, (GeoLocalArtifactDigest, FileFingerprint)>,
    explicit_relations: &[GeoSatisfactionArtifactReleaseRelation],
) -> Result<Option<Vec<GeoSatisfactionLocalInputBinding>>, GeoSatisfyError> {
    let Some(relations) =
        artifact_release_relations(request, receipt, local_artifacts, explicit_relations)?
    else {
        return Ok(None);
    };
    let mut bindings = Vec::new();
    for relation in relations {
        let Some((artifact, fingerprint)) = local_artifacts.get(&relation.local_artifact_id) else {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::MissingFileBinding,
                "Geo artifact-release relation does not reference a validated local artifact",
                [("local_artifact_id", relation.local_artifact_id.as_str())],
            ));
        };
        let result_digest_ids = receipt
            .result_digests
            .iter()
            .filter(|digest| {
                digest.algorithm == GeoDigestAlgorithm::Blake3
                    && fingerprint.digest == format!("blake3:{}", digest.hex_digest)
            })
            .map(|digest| digest.digest_id.clone())
            .collect::<Vec<_>>();
        let result_digest_ids = sorted_vec(result_digest_ids);
        let artifact_contract_version = local_artifact_contract_version(artifact, fingerprint)?;
        let binding_payload = (
            &request.request_id,
            &receipt.request_semantic_hash,
            &relation.source_instance_id,
            &relation.release_id,
            &relation.release_digest,
            &artifact.artifact_id,
            &artifact.media_type,
            &artifact_contract_version,
            &fingerprint.digest,
            fingerprint.byte_count,
        );
        let binding_id = format!(
            "geo-local-binding:{}",
            digest_json(&binding_payload)?.trim_start_matches("blake3:")
        );
        bindings.push(GeoSatisfactionLocalInputBinding {
            binding_id,
            request_id: request.request_id.clone(),
            request_semantic_hash: receipt.request_semantic_hash.clone(),
            receipt_terminal_state: receipt.terminal_state,
            proof_class: receipt.proof_class,
            source_instance_id: relation.source_instance_id,
            release_id: relation.release_id,
            release_digest: relation.release_digest,
            local_artifact_id: artifact.artifact_id.clone(),
            media_type: artifact.media_type.clone(),
            artifact_contract_version,
            content_hash: fingerprint.digest.clone(),
            byte_count: fingerprint.byte_count,
            result_digest_ids,
        });
    }
    bindings.sort();
    Ok(Some(bindings))
}

fn artifact_release_relations(
    request: &GeoAcquisitionRequest,
    receipt: &GeoAcquisitionReceipt,
    local_artifacts: &BTreeMap<String, (GeoLocalArtifactDigest, FileFingerprint)>,
    explicit_relations: &[GeoSatisfactionArtifactReleaseRelation],
) -> Result<Option<Vec<GeoSatisfactionArtifactReleaseRelation>>, GeoSatisfyError> {
    if !receipt.artifact_release_relations.is_empty() {
        let receipt_relations = validate_artifact_release_relations(
            request,
            local_artifacts,
            &receipt.artifact_release_relations,
        )?;
        if !explicit_relations.is_empty() {
            let supplied =
                validate_artifact_release_relations(request, local_artifacts, explicit_relations)?;
            if supplied != receipt_relations {
                return Err(GeoSatisfyError::new(
                    GeoSatisfyErrorCode::ReceiptMismatch,
                    "Geo caller-supplied artifact-release relations do not match the receipt-native relations",
                    Vec::<(String, String)>::new(),
                ));
            }
        }
        return Ok(Some(receipt_relations));
    }

    let [release] = receipt.releases.as_slice() else {
        return Ok(None);
    };
    let mut artifact_ids = local_artifacts.keys();
    let Some(local_artifact_id) = artifact_ids.next() else {
        return Ok(None);
    };
    if artifact_ids.next().is_some() {
        return Ok(None);
    }
    let attested_relation = GeoSatisfactionArtifactReleaseRelation {
        local_artifact_id: local_artifact_id.clone(),
        source_instance_id: release.source_instance_id.clone(),
        release_id: release.release_id.clone(),
        release_digest: release_digest_string(&release.release_digest),
    };
    let attested_relations = vec![attested_relation];

    if !explicit_relations.is_empty() {
        let supplied =
            validate_artifact_release_relations(request, local_artifacts, explicit_relations)?;
        if supplied != attested_relations {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ReceiptMismatch,
                "Geo artifact-release relation does not match the unambiguous receipt-attested relation",
                Vec::<(String, String)>::new(),
            ));
        }
    }

    validate_artifact_release_relations(request, local_artifacts, &attested_relations).map(Some)
}

fn validate_artifact_release_relations(
    request: &GeoAcquisitionRequest,
    local_artifacts: &BTreeMap<String, (GeoLocalArtifactDigest, FileFingerprint)>,
    relations: &[GeoSatisfactionArtifactReleaseRelation],
) -> Result<Vec<GeoSatisfactionArtifactReleaseRelation>, GeoSatisfyError> {
    if relations.is_empty() {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::InvalidInput,
            "Geo artifact-release relations must not be empty when supplied",
            Vec::<(String, String)>::new(),
        ));
    }

    let artifact_ids = local_artifacts.keys().cloned().collect::<BTreeSet<_>>();
    let release_keys = request
        .releases
        .iter()
        .map(release_key)
        .collect::<BTreeSet<_>>();
    let mut seen_relations = BTreeSet::new();
    let mut release_to_artifact = BTreeMap::<(String, String, String), String>::new();
    let mut artifact_to_release = BTreeMap::<String, (String, String, String)>::new();
    let mut normalized = Vec::with_capacity(relations.len());
    for relation in relations {
        validate_artifact_release_relation(relation)?;
        if !artifact_ids.contains(&relation.local_artifact_id) {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::MissingFileBinding,
                "Geo artifact-release relation references a receipt artifact without validated local bytes",
                [("local_artifact_id", relation.local_artifact_id.as_str())],
            ));
        }
        let key = (
            relation.source_instance_id.clone(),
            relation.release_id.clone(),
            relation.release_digest.clone(),
        );
        if !release_keys.contains(&key) {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ReceiptMismatch,
                "Geo artifact-release relation does not match an acquisition release pin",
                [
                    ("source_instance_id", relation.source_instance_id.as_str()),
                    ("release_id", relation.release_id.as_str()),
                    ("release_digest", relation.release_digest.as_str()),
                ],
            ));
        }
        if !seen_relations.insert((relation.local_artifact_id.clone(), key.clone())) {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::InvalidInput,
                "Geo artifact-release relations must be unique",
                [
                    ("local_artifact_id", relation.local_artifact_id.as_str()),
                    ("release_id", relation.release_id.as_str()),
                ],
            ));
        }
        if let Some(existing_artifact_id) =
            release_to_artifact.insert(key.clone(), relation.local_artifact_id.clone())
            && existing_artifact_id != relation.local_artifact_id
        {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo regional inventory advancement supports exactly one local artifact per source release",
                [
                    ("release_id", relation.release_id.as_str()),
                    ("first_artifact_id", existing_artifact_id.as_str()),
                    ("second_artifact_id", relation.local_artifact_id.as_str()),
                ],
            ));
        }
        if let Some(existing_release) =
            artifact_to_release.insert(relation.local_artifact_id.clone(), key.clone())
            && existing_release != key
        {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo artifact-release relations must not reuse one local artifact across releases",
                [
                    ("local_artifact_id", relation.local_artifact_id.as_str()),
                    ("first_release_id", existing_release.1.as_str()),
                    ("second_release_id", relation.release_id.as_str()),
                ],
            ));
        }
        normalized.push(relation.clone());
    }

    for key in release_keys {
        if !release_to_artifact.contains_key(&key) {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo artifact-release relations must cover every acquisition release pin",
                [
                    ("source_instance_id", key.0.as_str()),
                    ("release_id", key.1.as_str()),
                    ("release_digest", key.2.as_str()),
                ],
            ));
        }
    }
    for artifact_id in artifact_ids {
        if !artifact_to_release.contains_key(&artifact_id) {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo artifact-release relations must cover every local artifact",
                [("local_artifact_id", artifact_id.as_str())],
            ));
        }
    }

    normalized.sort();
    Ok(normalized)
}

fn validate_artifact_release_relation(
    relation: &GeoSatisfactionArtifactReleaseRelation,
) -> Result<(), GeoSatisfyError> {
    for (field, value) in [
        ("local_artifact_id", relation.local_artifact_id.as_str()),
        ("source_instance_id", relation.source_instance_id.as_str()),
        ("release_id", relation.release_id.as_str()),
        ("release_digest", relation.release_digest.as_str()),
    ] {
        if value.trim().is_empty() || value.trim() != value {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::InvalidInput,
                "Geo artifact-release relation fields must be non-empty and trimmed",
                [(field, value)],
            ));
        }
    }
    Ok(())
}

fn release_key(release: &super::GeoReleasePin) -> (String, String, String) {
    (
        release.source_instance_id.clone(),
        release.release_id.clone(),
        release_digest_string(&release.release_digest),
    )
}

fn release_digest_string(digest: &GeoDigest) -> String {
    format!(
        "{}:{}",
        digest_algorithm_name(digest.algorithm),
        digest.hex_digest
    )
}

fn local_artifact_contract_version(
    artifact: &GeoLocalArtifactDigest,
    fingerprint: &FileFingerprint,
) -> Result<Option<String>, GeoSatisfyError> {
    if artifact.media_type == GEO_RUN_JSON_MEDIA_TYPE {
        json_artifact_version(&fingerprint.bytes, &artifact.artifact_id).map(Some)
    } else {
        Ok(None)
    }
}

fn validate_run_input_target(
    target: &GeoSatisfactionRunInputFileBinding,
) -> Result<(), GeoSatisfyError> {
    for (field, value) in [
        ("local_artifact_id", target.local_artifact_id.as_str()),
        ("node_id", target.node_id.as_str()),
        ("binding_id", target.binding_id.as_str()),
        ("contract_version", target.contract_version.as_str()),
    ] {
        if value.trim().is_empty() || value.trim() != value {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::InvalidInput,
                "Geo run input target fields must be non-empty and trimmed",
                [(field, value)],
            ));
        }
    }
    Ok(())
}

fn validate_run_input_target_against_plan(
    plan: &GeoPlan,
    target: &GeoSatisfactionRunInputFileBinding,
) -> Result<(), GeoSatisfyError> {
    let node = plan
        .project_plan
        .nodes
        .iter()
        .find(|node| node.node_id == target.node_id)
        .ok_or_else(|| {
            GeoSatisfyError::new(
                GeoSatisfyErrorCode::RequestNotFound,
                "Geo run input target references a node that is absent from the plan",
                [("node_id", target.node_id.as_str())],
            )
        })?;
    let accepted_contracts = run_input_contracts_for_command(&node.command, &target.binding_id)
        .ok_or_else(|| {
            GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo run input target does not match a supported node input binding",
                [
                    ("node_id", target.node_id.as_str()),
                    ("binding_id", target.binding_id.as_str()),
                    ("command", node.command.as_str()),
                ],
            )
        })?;
    if !accepted_contracts
        .iter()
        .any(|contract| *contract == target.contract_version)
    {
        let expected = accepted_contracts.join("|");
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo run input target contract is not accepted by the planned node",
            [
                ("node_id".to_string(), target.node_id.clone()),
                ("binding_id".to_string(), target.binding_id.clone()),
                ("expected".to_string(), expected),
                ("actual".to_string(), target.contract_version.clone()),
            ],
        ));
    }
    let expected_artifact_id = geo_run_input_artifact_id(&target.node_id, &target.binding_id);
    if expected_artifact_id.trim().is_empty() {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo run input target could not derive a stable run artifact id",
            [
                ("node_id", target.node_id.as_str()),
                ("binding_id", target.binding_id.as_str()),
            ],
        ));
    }
    Ok(())
}

fn run_input_contracts_for_command(
    command: &str,
    binding_id: &str,
) -> Option<&'static [&'static str]> {
    match (command, binding_id) {
        (GEO_MATERIALIZE_HOME_CELLS_COMMAND, GEO_ROWS_BINDING_ID) => {
            Some(&[CANON_GEO_HOME_CELL_ROWS_VERSION])
        }
        (GEO_TILE_WORK_COMMAND, GEO_REQUEST_BINDING_ID) => {
            Some(&[CANON_GEO_TILE_WORK_REQUEST_VERSION])
        }
        (GEO_MATERIALIZE_EVIDENCE_COMMAND, GEO_ROWS_BINDING_ID) => {
            Some(&[CANON_GEO_WAREHOUSE_ROWS_VERSION])
        }
        (GEO_COMPILE_EVIDENCE_COMMAND | GEO_SOLVE_COMMAND, _) => None,
        _ => None,
    }
}

fn json_artifact_version(bytes: &[u8], file_id: &str) -> Result<String, GeoSatisfyError> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|error| {
        GeoSatisfyError::new(
            GeoSatisfyErrorCode::InvalidInput,
            "Geo run input target file is not a JSON artifact",
            [
                ("file_id", file_id.to_string()),
                ("error", error.to_string()),
            ],
        )
    })?;
    value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .filter(|version| !version.trim().is_empty() && version.trim() == *version)
        .map(str::to_string)
        .ok_or_else(|| {
            GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo run input target file lacks a non-empty JSON artifact version",
                [("file_id", file_id)],
            )
        })
}

fn satisfaction_id(semantic_hash: &str) -> String {
    format!(
        "{CANON_GEO_ACQUISITION_SATISFACTION_VERSION}:{}",
        semantic_hash.trim_start_matches("blake3:")
    )
}

fn build_inventory_advancement(
    plan: &GeoPlan,
    inventory: &GeoRegionalInventory,
    request: &GeoAcquisitionRequest,
    receipt: &GeoAcquisitionReceipt,
    receipt_file: &GeoSatisfactionFileAudit,
    bindings: &[GeoSatisfactionLocalInputBinding],
) -> Result<GeoRegionalInventoryAdvancement, GeoSatisfyError> {
    if receipt.proof_class != GeoAcquisitionProofClass::Live {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo inventory advancement requires live acquisition proof",
            [("proof_class", format!("{:?}", receipt.proof_class))],
        ));
    }
    validate_geo_plan(plan).map_err(plan_error)?;
    let base_inventory_semantic_hash = validate_inventory_ref(plan, inventory)?;
    validate_inventory_advancement_subset(inventory, request)?;
    let (advanced_inventory, source_advancements) =
        advanced_inventory(inventory, request, bindings)?;
    let advanced_inventory_semantic_hash =
        regional_inventory_semantic_hash(&advanced_inventory).map_err(control_error)?;
    let bounded_subset = canonicalize_geo_acquisition_request(request).subset;
    let bounded_subset_hash = digest_json(&bounded_subset)?;
    let mut advancement = GeoRegionalInventoryAdvancement {
        version: CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION.to_string(),
        advancement_id: String::new(),
        semantic_hash: String::new(),
        effect: GeoInventoryAdvancementEffect::LocalAvailabilityOnly,
        plan_id: plan.plan_id.clone(),
        plan_semantic_hash: plan.semantic_hash.clone(),
        request_id: request.request_id.clone(),
        request_semantic_hash: receipt.request_semantic_hash.clone(),
        base_inventory_id: inventory.inventory_id.clone(),
        base_inventory_semantic_hash,
        advanced_inventory_id: advanced_inventory.inventory_id.clone(),
        advanced_inventory_semantic_hash,
        bounded_geography: request.bounded_geography.clone(),
        bounded_subset,
        bounded_subset_hash,
        receipt_file: receipt_file.clone(),
        receipt_execution: receipt_execution_ref(receipt),
        receipt_terminal_state: receipt.terminal_state,
        proof_class: receipt.proof_class,
        denominators: sorted_vec(receipt.denominators.clone()),
        source_digests: sorted_vec(receipt.source_digests.clone()),
        result_digests: sorted_vec(receipt.result_digests.clone()),
        source_advancements,
        advanced_inventory,
    };
    advancement.semantic_hash = geo_regional_inventory_advancement_semantic_hash(&advancement)?;
    advancement.advancement_id = inventory_advancement_id(&advancement.semantic_hash);
    Ok(advancement)
}

fn inventory_advancement_artifact_is_usable(
    bindings: &[GeoSatisfactionLocalInputBinding],
    local_artifacts: &BTreeMap<String, (GeoLocalArtifactDigest, FileFingerprint)>,
) -> Result<bool, GeoSatisfyError> {
    if bindings.is_empty() {
        return Ok(false);
    }

    for binding in bindings {
        if binding.media_type != GEO_RUN_JSON_MEDIA_TYPE
            || binding.artifact_contract_version.as_deref()
                != Some(CANON_GEO_WAREHOUSE_ROWS_VERSION)
        {
            return Ok(false);
        }
        let Some((_, fingerprint)) = local_artifacts.get(&binding.local_artifact_id) else {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::MissingFileBinding,
                "Geo inventory advancement lost its validated local artifact binding",
                [("local_artifact_id", binding.local_artifact_id.as_str())],
            ));
        };
        let rows: GeoWarehouseRowsRequest =
            serde_json::from_slice(&fingerprint.bytes).map_err(|error| {
                GeoSatisfyError::new(
                    GeoSatisfyErrorCode::ContractMismatch,
                    "Geo inventory advancement requires a parseable warehouse-row artifact",
                    [
                        ("local_artifact_id", binding.local_artifact_id.clone()),
                        ("error", error.to_string()),
                    ],
                )
            })?;
        materialize_warehouse_rows(&rows).map_err(|error| {
            let mut detail = error.detail;
            detail.insert(
                "local_artifact_id".to_string(),
                binding.local_artifact_id.clone(),
            );
            detail.insert(
                "materialization_error_code".to_string(),
                format!("{:?}", error.code),
            );
            GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                format!(
                    "Geo inventory advancement requires a usable warehouse-row artifact: {}",
                    error.message
                ),
                detail,
            )
        })?;
    }
    Ok(true)
}

fn validate_inventory_ref(
    plan: &GeoPlan,
    inventory: &GeoRegionalInventory,
) -> Result<String, GeoSatisfyError> {
    let semantic_hash = regional_inventory_semantic_hash(inventory).map_err(control_error)?;
    let planning_hash = regional_inventory_planning_hash(inventory).map_err(control_error)?;
    for (field, expected, actual) in [
        (
            "inventory_id",
            plan.inventory_ref.inventory_id.as_str(),
            inventory.inventory_id.as_str(),
        ),
        (
            "semantic_hash",
            plan.inventory_ref.semantic_hash.as_str(),
            semantic_hash.as_str(),
        ),
        (
            "planning_hash",
            plan.inventory_ref.planning_hash.as_str(),
            planning_hash.as_str(),
        ),
    ] {
        if actual != expected {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo inventory advancement requires the supplied inventory to match plan.inventory_ref",
                [("field", field), ("expected", expected), ("actual", actual)],
            ));
        }
    }
    Ok(semantic_hash)
}

fn validate_inventory_advancement_subset(
    inventory: &GeoRegionalInventory,
    request: &GeoAcquisitionRequest,
) -> Result<(), GeoSatisfyError> {
    if request.bounded_geography != inventory.region || request.subset.geography != inventory.region
    {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo inventory advancement requires the acquisition subset geography to equal the planned inventory region",
            [
                ("inventory_region", inventory.region.geography_id.as_str()),
                (
                    "request_region",
                    request.bounded_geography.geography_id.as_str(),
                ),
                (
                    "subset_region",
                    request.subset.geography.geography_id.as_str(),
                ),
            ],
        ));
    }
    let narrowing_predicate = request.subset.predicates.iter().find(|predicate| {
        matches!(
            predicate.kind,
            GeoSubsetPredicateKind::H3Cells
                | GeoSubsetPredicateKind::BoundingBox
                | GeoSubsetPredicateKind::ExplicitIdentifiers
        )
    });
    if !request.subset.h3_cells.is_empty() || narrowing_predicate.is_some() {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo inventory advancement requires a region-complete acquisition subset without narrower spatial filtering",
            [
                ("h3_cell_count", request.subset.h3_cells.len().to_string()),
                (
                    "narrowing_predicate_id",
                    narrowing_predicate
                        .map(|predicate| predicate.predicate_id.as_str())
                        .unwrap_or("none")
                        .to_string(),
                ),
            ],
        ));
    }
    Ok(())
}

pub fn geo_regional_inventory_advancement_semantic_hash(
    advancement: &GeoRegionalInventoryAdvancement,
) -> Result<String, GeoSatisfyError> {
    digest_json(&GeoRegionalInventoryAdvancementSemanticProjection {
        version: &advancement.version,
        effect: advancement.effect,
        plan_id: &advancement.plan_id,
        plan_semantic_hash: &advancement.plan_semantic_hash,
        request_id: &advancement.request_id,
        request_semantic_hash: &advancement.request_semantic_hash,
        base_inventory_id: &advancement.base_inventory_id,
        base_inventory_semantic_hash: &advancement.base_inventory_semantic_hash,
        advanced_inventory_id: &advancement.advanced_inventory_id,
        advanced_inventory_semantic_hash: &advancement.advanced_inventory_semantic_hash,
        bounded_geography: &advancement.bounded_geography,
        bounded_subset_hash: &advancement.bounded_subset_hash,
        receipt_terminal_state: advancement.receipt_terminal_state,
        proof_class: advancement.proof_class,
        denominators: &advancement.denominators,
        source_digests: &advancement.source_digests,
        result_digests: &advancement.result_digests,
        source_advancements: &advancement.source_advancements,
    })
}

pub fn canonical_geo_regional_inventory_advancement_bytes(
    advancement: &GeoRegionalInventoryAdvancement,
) -> Result<Vec<u8>, GeoSatisfyError> {
    validate_geo_regional_inventory_advancement(advancement)?;
    serde_json::to_vec(advancement).map_err(|error| {
        GeoSatisfyError::new(
            GeoSatisfyErrorCode::Serialization,
            "Geo regional inventory advancement could not be serialized",
            [("error", error.to_string())],
        )
    })
}

pub(super) fn validate_geo_regional_inventory_advancement(
    advancement: &GeoRegionalInventoryAdvancement,
) -> Result<(), GeoSatisfyError> {
    if advancement.version != CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::UnsupportedVersion,
            "unsupported Geo regional inventory advancement version",
            [
                ("actual", advancement.version.as_str()),
                ("expected", CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION),
            ],
        ));
    }
    if advancement.effect != GeoInventoryAdvancementEffect::LocalAvailabilityOnly {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement must be local-availability-only",
            [("effect", format!("{:?}", advancement.effect))],
        ));
    }
    if advancement.proof_class != GeoAcquisitionProofClass::Live
        || advancement.receipt_terminal_state != GeoAcquisitionTerminalState::Complete
    {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement requires live COMPLETE acquisition proof",
            [
                ("proof_class", format!("{:?}", advancement.proof_class)),
                (
                    "receipt_terminal_state",
                    format!("{:?}", advancement.receipt_terminal_state),
                ),
            ],
        ));
    }
    validate_advancement_receipt_execution(advancement)?;
    validate_nonempty_trimmed("plan_id", &advancement.plan_id)?;
    validate_blake3_hash("plan_semantic_hash", &advancement.plan_semantic_hash)?;
    validate_identifier_like("request_id", &advancement.request_id)?;
    validate_blake3_hash("request_semantic_hash", &advancement.request_semantic_hash)?;
    validate_nonempty_trimmed("base_inventory_id", &advancement.base_inventory_id)?;
    validate_blake3_hash(
        "base_inventory_semantic_hash",
        &advancement.base_inventory_semantic_hash,
    )?;
    validate_nonempty_trimmed("advanced_inventory_id", &advancement.advanced_inventory_id)?;
    validate_blake3_hash(
        "advanced_inventory_semantic_hash",
        &advancement.advanced_inventory_semantic_hash,
    )?;
    validate_file_audit(&advancement.receipt_file, "receipt_file")?;
    if advancement.bounded_subset.geography != advancement.bounded_geography {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement subset geography must match its bounded geography",
            [
                (
                    "bounded_geography_id",
                    advancement.bounded_geography.geography_id.as_str(),
                ),
                (
                    "subset_geography_id",
                    advancement.bounded_subset.geography.geography_id.as_str(),
                ),
            ],
        ));
    }
    let bounded_subset_hash = digest_json(&advancement.bounded_subset)?;
    validate_equal(
        "bounded_subset_hash",
        &bounded_subset_hash,
        &advancement.bounded_subset_hash,
    )?;
    validate_digest_list("source_digests", &advancement.source_digests, false)?;
    validate_digest_list("result_digests", &advancement.result_digests, false)?;
    if let Some(digest) = advancement
        .result_digests
        .iter()
        .find(|digest| digest.algorithm != GeoDigestAlgorithm::Blake3)
    {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement result digests must use BLAKE3",
            [
                ("digest_id".to_string(), digest.digest_id.clone()),
                ("algorithm".to_string(), format!("{:?}", digest.algorithm)),
            ],
        ));
    }
    validate_denominators(&advancement.denominators)?;
    let mut sorted_source_advancements = advancement.source_advancements.clone();
    sorted_source_advancements.sort();
    sorted_source_advancements.dedup();
    if sorted_source_advancements.is_empty()
        || sorted_source_advancements != advancement.source_advancements
    {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement source_advancements must be sorted, distinct, and non-empty",
            [(
                "source_advancement_count",
                advancement.source_advancements.len().to_string(),
            )],
        ));
    }
    let mut source_keys = BTreeSet::new();
    for source in &advancement.source_advancements {
        validate_source_advancement(source, &advancement.result_digests)?;
        let key = (
            source.source_instance_id.clone(),
            source.release.release_id.clone(),
            source.release.release_digest.clone(),
        );
        if !source_keys.insert(key) {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo regional inventory advancement must name each source release exactly once",
                [("source_instance_id", source.source_instance_id.as_str())],
            ));
        }
    }
    let canonical_inventory =
        canonicalize_regional_inventory(&advancement.advanced_inventory).map_err(control_error)?;
    if canonical_inventory != advancement.advanced_inventory {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement advanced_inventory must be canonical",
            [(
                "advanced_inventory_id",
                advancement.advanced_inventory_id.as_str(),
            )],
        ));
    }
    if canonical_inventory.region != advancement.bounded_geography {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement inventory region must match its bounded geography",
            [
                (
                    "inventory_region_id",
                    canonical_inventory.region.geography_id.as_str(),
                ),
                (
                    "bounded_geography_id",
                    advancement.bounded_geography.geography_id.as_str(),
                ),
            ],
        ));
    }
    for source_advancement in &advancement.source_advancements {
        let matching_sources = canonical_inventory
            .sources
            .iter()
            .filter(|source| {
                source.source_instance_id == source_advancement.source_instance_id
                    && source.release == source_advancement.release
                    && source.coverage.region == advancement.bounded_geography
            })
            .collect::<Vec<_>>();
        if matching_sources.len() != 1
            || matching_sources[0].local_state.state != GeoSourceAvailability::Available
            || matching_sources[0].local_state.local_ref.as_ref()
                != Some(&source_advancement.local_ref)
        {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo regional inventory advancement inventory must carry each advanced local artifact exactly once",
                [
                    (
                        "source_instance_id".to_string(),
                        source_advancement.source_instance_id.clone(),
                    ),
                    ("matches".to_string(), matching_sources.len().to_string()),
                ],
            ));
        }
    }
    validate_equal(
        "advanced_inventory_id",
        &canonical_inventory.inventory_id,
        &advancement.advanced_inventory_id,
    )?;
    let advanced_inventory_semantic_hash =
        regional_inventory_semantic_hash(&canonical_inventory).map_err(control_error)?;
    validate_equal(
        "advanced_inventory_semantic_hash",
        &advanced_inventory_semantic_hash,
        &advancement.advanced_inventory_semantic_hash,
    )?;
    let expected_semantic_hash = geo_regional_inventory_advancement_semantic_hash(advancement)?;
    validate_equal(
        "semantic_hash",
        &expected_semantic_hash,
        &advancement.semantic_hash,
    )?;
    let expected_advancement_id = inventory_advancement_id(&expected_semantic_hash);
    validate_equal(
        "advancement_id",
        &expected_advancement_id,
        &advancement.advancement_id,
    )?;
    Ok(())
}

fn validate_advancement_receipt_execution(
    advancement: &GeoRegionalInventoryAdvancement,
) -> Result<(), GeoSatisfyError> {
    let execution = &advancement.receipt_execution;
    if execution.proof_class != advancement.proof_class
        || execution.terminal_state != advancement.receipt_terminal_state
        || execution.proof_class != GeoAcquisitionProofClass::Live
        || execution.terminal_state != GeoAcquisitionTerminalState::Complete
        || execution.fixture_id.is_some()
        || execution.retained_receipt_id.is_some()
    {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement receipt execution must agree with live COMPLETE proof",
            [
                (
                    "top_level_proof_class".to_string(),
                    format!("{:?}", advancement.proof_class),
                ),
                (
                    "execution_proof_class".to_string(),
                    format!("{:?}", execution.proof_class),
                ),
                (
                    "top_level_terminal_state".to_string(),
                    format!("{:?}", advancement.receipt_terminal_state),
                ),
                (
                    "execution_terminal_state".to_string(),
                    format!("{:?}", execution.terminal_state),
                ),
            ],
        ));
    }
    let executor_request_id = execution.executor_request_id.as_deref().ok_or_else(|| {
        GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement live receipt execution requires an executor request id",
            Vec::<(String, String)>::new(),
        )
    })?;
    validate_nonempty_trimmed("receipt_execution.executor_request_id", executor_request_id)?;
    let executor_query_id = execution.executor_query_id.as_deref().ok_or_else(|| {
        GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement live receipt execution requires an executor query id",
            Vec::<(String, String)>::new(),
        )
    })?;
    validate_nonempty_trimmed("receipt_execution.executor_query_id", executor_query_id)?;
    if let Some(attempt_id) = execution.executor_attempt_id.as_deref() {
        validate_nonempty_trimmed("receipt_execution.executor_attempt_id", attempt_id)?;
    }
    Ok(())
}

fn validate_source_advancement(
    advancement: &GeoRegionalInventorySourceAdvancement,
    result_digests: &[GeoDigest],
) -> Result<(), GeoSatisfyError> {
    validate_nonempty_trimmed(
        "source_advancements[].source_instance_id",
        &advancement.source_instance_id,
    )?;
    validate_nonempty_trimmed(
        "source_advancements[].release.release_id",
        &advancement.release.release_id,
    )?;
    validate_blake3_hash(
        "source_advancements[].release.release_digest",
        &advancement.release.release_digest,
    )?;
    if advancement.advanced_state != GeoSourceAvailability::Available {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement source must end in local availability",
            [
                (
                    "source_instance_id".to_string(),
                    advancement.source_instance_id.clone(),
                ),
                (
                    "advanced_state".to_string(),
                    format!("{:?}", advancement.advanced_state),
                ),
            ],
        ));
    }
    if advancement.local_artifact_byte_count == 0 {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::FileByteCountMismatch,
            "Geo regional inventory advancement local artifact byte count must be positive",
            [(
                "source_instance_id",
                advancement.source_instance_id.as_str(),
            )],
        ));
    }
    validate_nonempty_trimmed(
        "source_advancements[].local_ref.artifact_id",
        &advancement.local_ref.artifact_id,
    )?;
    validate_nonempty_trimmed(
        "source_advancements[].local_ref.contract_version",
        &advancement.local_ref.contract_version,
    )?;
    validate_blake3_hash(
        "source_advancements[].local_ref.content_hash",
        &advancement.local_ref.content_hash,
    )?;
    validate_nonempty_trimmed(
        "source_advancements[].local_ref.media_type",
        &advancement.local_ref.media_type,
    )?;
    validate_equal(
        "source_advancements[].local_artifact_contract_version",
        advancement
            .local_artifact_contract_version
            .as_deref()
            .unwrap_or(""),
        &advancement.local_ref.contract_version,
    )?;
    let mut sorted_result_digest_ids = advancement.result_digest_ids.clone();
    sorted_result_digest_ids.sort();
    sorted_result_digest_ids.dedup();
    if sorted_result_digest_ids.is_empty()
        || sorted_result_digest_ids != advancement.result_digest_ids
    {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement result digest ids must be sorted, distinct, and non-empty",
            [(
                "source_instance_id",
                advancement.source_instance_id.as_str(),
            )],
        ));
    }
    for digest_id in &advancement.result_digest_ids {
        let matches = result_digests
            .iter()
            .filter(|digest| digest.digest_id == *digest_id)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo regional inventory advancement result digest id must resolve exactly once",
                [
                    (
                        "source_instance_id".to_string(),
                        advancement.source_instance_id.clone(),
                    ),
                    ("result_digest_id".to_string(), digest_id.clone()),
                    ("matches".to_string(), matches.len().to_string()),
                ],
            ));
        }
        let digest = release_digest_string(matches[0]);
        if digest != advancement.local_ref.content_hash {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo regional inventory advancement local artifact must match its receipt result digest",
                [
                    (
                        "source_instance_id".to_string(),
                        advancement.source_instance_id.clone(),
                    ),
                    ("result_digest_id".to_string(), digest_id.clone()),
                    ("expected".to_string(), digest),
                    (
                        "actual".to_string(),
                        advancement.local_ref.content_hash.clone(),
                    ),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_file_audit(
    audit: &GeoSatisfactionFileAudit,
    field: &str,
) -> Result<(), GeoSatisfyError> {
    validate_nonempty_trimmed(&format!("{field}.file_id"), &audit.file_id)?;
    validate_blake3_hash(&format!("{field}.digest"), &audit.digest)?;
    if audit.byte_count == 0 {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::FileByteCountMismatch,
            "Geo regional inventory advancement file audit byte count must be positive",
            [(format!("{field}.byte_count"), audit.byte_count.to_string())],
        ));
    }
    Ok(())
}

fn validate_digest_list(
    field: &str,
    digests: &[GeoDigest],
    allow_empty: bool,
) -> Result<(), GeoSatisfyError> {
    if !allow_empty && digests.is_empty() {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement digest lists must be non-empty",
            [("field", field)],
        ));
    }
    let mut sorted = digests.to_vec();
    sorted.sort();
    sorted.dedup();
    if sorted != digests {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement digest lists must be sorted and distinct",
            [("field", field)],
        ));
    }
    for digest in digests {
        validate_nonempty_trimmed(&format!("{field}[].digest_id"), &digest.digest_id)?;
        validate_digest_hex(field, digest.algorithm, &digest.hex_digest)?;
    }
    Ok(())
}

fn validate_denominators(
    denominators: &[GeoAcquisitionDenominator],
) -> Result<(), GeoSatisfyError> {
    if denominators.is_empty() {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement denominators must be non-empty",
            Vec::<(String, String)>::new(),
        ));
    }
    let mut sorted = denominators.to_vec();
    sorted.sort();
    sorted.dedup();
    if sorted != denominators {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement denominators must be sorted and distinct",
            Vec::<(String, String)>::new(),
        ));
    }
    for denominator in denominators {
        validate_nonempty_trimmed("denominators[].denominator_id", &denominator.denominator_id)?;
        validate_nonempty_trimmed("denominators[].unit", &denominator.unit)?;
        validate_nonempty_trimmed("denominators[].description", &denominator.description)?;
    }
    Ok(())
}

fn validate_equal(field: &str, expected: &str, actual: &str) -> Result<(), GeoSatisfyError> {
    if expected == actual {
        Ok(())
    } else {
        Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement field does not match its derived value",
            [
                ("field".to_string(), field.to_string()),
                ("expected".to_string(), expected.to_string()),
                ("actual".to_string(), actual.to_string()),
            ],
        ))
    }
}

fn validate_identifier_like(field: &str, value: &str) -> Result<(), GeoSatisfyError> {
    validate_nonempty_trimmed(field, value)?;
    if value.contains(':') {
        Ok(())
    } else {
        Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::InvalidInput,
            "Geo regional inventory advancement identifier must carry a contract prefix",
            [("field", field), ("value", value)],
        ))
    }
}

fn validate_nonempty_trimmed(field: &str, value: &str) -> Result<(), GeoSatisfyError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::InvalidInput,
            "Geo regional inventory advancement string fields must be non-empty and trimmed",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_blake3_hash(field: &str, value: &str) -> Result<(), GeoSatisfyError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement hash fields must be lowercase BLAKE3",
            [("field", field), ("value", value)],
        ));
    };
    if hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement hash fields must be lowercase BLAKE3",
            [("field", field), ("value", value)],
        ))
    }
}

fn validate_digest_hex(
    field: &str,
    algorithm: GeoDigestAlgorithm,
    hex_digest: &str,
) -> Result<(), GeoSatisfyError> {
    let expected_len = match algorithm {
        GeoDigestAlgorithm::Blake3 | GeoDigestAlgorithm::Sha256 => 64,
        GeoDigestAlgorithm::Sha512 => 128,
    };
    if hex_digest.len() == expected_len
        && hex_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo regional inventory advancement digest hex fields must match their algorithm width",
            [
                ("field", field),
                ("algorithm", digest_algorithm_name(algorithm)),
                ("hex_digest", hex_digest),
            ],
        ))
    }
}

fn inventory_advancement_id(semantic_hash: &str) -> String {
    format!(
        "{CANON_GEO_REGIONAL_INVENTORY_ADVANCEMENT_VERSION}:{}",
        semantic_hash.trim_start_matches("blake3:")
    )
}

fn advanced_inventory(
    inventory: &GeoRegionalInventory,
    request: &GeoAcquisitionRequest,
    bindings: &[GeoSatisfactionLocalInputBinding],
) -> Result<
    (
        GeoRegionalInventory,
        Vec<GeoRegionalInventorySourceAdvancement>,
    ),
    GeoSatisfyError,
> {
    if inventory.region != request.bounded_geography {
        return Err(GeoSatisfyError::new(
            GeoSatisfyErrorCode::ContractMismatch,
            "Geo inventory advancement requires inventory region to match the acquisition bounded geography",
            [
                ("inventory_region", inventory.region.geography_id.as_str()),
                (
                    "request_region",
                    request.bounded_geography.geography_id.as_str(),
                ),
            ],
        ));
    }
    let mut updated = inventory.clone();
    let mut source_advancements = Vec::new();
    let mut seen_sources = BTreeSet::new();
    for binding in bindings {
        let matching_release = request.releases.iter().any(|release| {
            release.source_instance_id == binding.source_instance_id
                && release.release_id == binding.release_id
                && release_digest_string(&release.release_digest) == binding.release_digest
        });
        if !matching_release {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ReceiptMismatch,
                "Geo local binding does not match any acquisition release pin",
                [("binding_id", binding.binding_id.as_str())],
            ));
        }
        if !seen_sources.insert((
            binding.source_instance_id.clone(),
            binding.release_id.clone(),
            binding.release_digest.clone(),
        )) {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo inventory advancement cannot apply duplicate bindings to one source release",
                [
                    ("source_instance_id", binding.source_instance_id.as_str()),
                    ("release_id", binding.release_id.as_str()),
                ],
            ));
        }
        let Some(source) = updated.sources.iter_mut().find(|source| {
            source_matches_binding(source, binding)
                && source.coverage.region == request.bounded_geography
        }) else {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::RequestNotFound,
                "Geo inventory has no source instance matching the satisfied release",
                [
                    ("source_instance_id", binding.source_instance_id.as_str()),
                    ("release_id", binding.release_id.as_str()),
                ],
            ));
        };
        let contract_version = binding.artifact_contract_version.clone().ok_or_else(|| {
            GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo inventory advancement requires a typed local artifact contract version",
                [("local_artifact_id", binding.local_artifact_id.as_str())],
            )
        })?;
        let local_ref = GeoLocalArtifactRef {
            artifact_id: binding.local_artifact_id.clone(),
            contract_version,
            content_hash: binding.content_hash.clone(),
            media_type: binding.media_type.clone(),
        };
        if let Some(existing_ref) = &source.local_state.local_ref
            && existing_ref != &local_ref
        {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ContractMismatch,
                "Geo inventory advancement would overwrite an existing local artifact reference",
                [
                    ("source_instance_id", binding.source_instance_id.as_str()),
                    ("release_id", binding.release_id.as_str()),
                    ("existing_artifact_id", existing_ref.artifact_id.as_str()),
                    ("new_artifact_id", binding.local_artifact_id.as_str()),
                ],
            ));
        }
        let previous_state = source.local_state.state;
        source.local_state.state = GeoSourceAvailability::Available;
        source.local_state.local_ref = Some(local_ref.clone());
        source_advancements.push(GeoRegionalInventorySourceAdvancement {
            source_instance_id: binding.source_instance_id.clone(),
            release: GeoSourceRelease {
                release_id: binding.release_id.clone(),
                release_digest: binding.release_digest.clone(),
            },
            previous_state,
            advanced_state: GeoSourceAvailability::Available,
            local_ref,
            local_artifact_byte_count: binding.byte_count,
            local_artifact_contract_version: binding.artifact_contract_version.clone(),
            result_digest_ids: sorted_vec(binding.result_digest_ids.clone()),
        });
    }
    source_advancements.sort();
    let updated = canonicalize_regional_inventory(&updated).map_err(control_error)?;
    Ok((updated, source_advancements))
}

fn receipt_execution_ref(receipt: &GeoAcquisitionReceipt) -> GeoSatisfactionExecutionRef {
    let executor = receipt.executor.as_ref();
    GeoSatisfactionExecutionRef {
        proof_class: receipt.proof_class,
        terminal_state: receipt.terminal_state,
        fixture_id: receipt.fixture_id.clone(),
        retained_receipt_id: receipt.retained_receipt_id.clone(),
        executor_request_id: executor.map(|trace| trace.executor_request_id.clone()),
        executor_query_id: executor.map(|trace| trace.executor_query_id.clone()),
        executor_attempt_id: executor.and_then(|trace| trace.executor_attempt_id.clone()),
    }
}

fn source_matches_binding(
    source: &GeoRegionalSourceInstance,
    binding: &GeoSatisfactionLocalInputBinding,
) -> bool {
    source.source_instance_id == binding.source_instance_id
        && source.release.release_id == binding.release_id
        && source.release.release_digest == binding.release_digest
        && matches!(
            source.native_scope,
            GeoNativeEntityScope::NativeEntity { .. } | GeoNativeEntityScope::ObservationOnly
        )
}

fn findings_for_receipt(
    request: &GeoAcquisitionRequest,
    receipt: &GeoAcquisitionReceipt,
    positive: bool,
) -> Vec<GeoSatisfactionFinding> {
    let mut findings = Vec::new();
    if positive {
        findings.push(finding(
            GeoSatisfactionFindingCode::Satisfied,
            Vec::<(String, String)>::new(),
        ));
        return findings;
    }
    let code = match receipt.terminal_state {
        GeoAcquisitionTerminalState::Complete => GeoSatisfactionFindingCode::PositiveGateNotMet,
        GeoAcquisitionTerminalState::ZeroRows => GeoSatisfactionFindingCode::ZeroRows,
        GeoAcquisitionTerminalState::Timeout => GeoSatisfactionFindingCode::Timeout,
        GeoAcquisitionTerminalState::Canceled => GeoSatisfactionFindingCode::Canceled,
        GeoAcquisitionTerminalState::Partial => GeoSatisfactionFindingCode::Partial,
        GeoAcquisitionTerminalState::UnreadableColumns => {
            GeoSatisfactionFindingCode::UnreadableColumns
        }
    };
    findings.push(finding(
        code,
        [
            ("terminal_state", format!("{:?}", receipt.terminal_state)),
            ("rows", receipt.counts.rows.to_string()),
            (
                "positive_path_min_rows",
                request.positive_path_min_rows.to_string(),
            ),
        ],
    ));
    if !receipt.unreadable_columns.is_empty() {
        findings.push(finding(
            GeoSatisfactionFindingCode::UnreadableColumns,
            [("columns", receipt.unreadable_columns.join(","))],
        ));
    }
    if receipt.pagination.next_page_token.is_some()
        || receipt.pagination.rows_truncated
        || receipt.pagination.bytes_truncated
    {
        findings.push(finding(
            GeoSatisfactionFindingCode::Partial,
            [
                (
                    "next_page_token",
                    receipt
                        .pagination
                        .next_page_token
                        .clone()
                        .unwrap_or_else(|| "<none>".to_string()),
                ),
                (
                    "rows_truncated",
                    receipt.pagination.rows_truncated.to_string(),
                ),
                (
                    "bytes_truncated",
                    receipt.pagination.bytes_truncated.to_string(),
                ),
            ],
        ));
    }
    findings.sort();
    findings.dedup();
    findings
}

fn finding(
    code: GeoSatisfactionFindingCode,
    detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
) -> GeoSatisfactionFinding {
    GeoSatisfactionFinding {
        code,
        detail: detail
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect(),
    }
}

fn file_audits(
    local_artifacts: &BTreeMap<String, (GeoLocalArtifactDigest, FileFingerprint)>,
) -> Vec<GeoSatisfactionFileAudit> {
    local_artifacts
        .iter()
        .map(|(artifact_id, (_, fingerprint))| GeoSatisfactionFileAudit {
            file_id: artifact_id.clone(),
            byte_count: fingerprint.byte_count,
            digest: fingerprint.digest.clone(),
        })
        .collect()
}

fn binding_map(
    field: &str,
    bindings: &[GeoSatisfactionFileBinding],
) -> Result<BTreeMap<String, PathBuf>, GeoSatisfyError> {
    let mut result = BTreeMap::new();
    for binding in bindings {
        if binding.binding_id.trim().is_empty() || binding.binding_id.trim() != binding.binding_id {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::InvalidInput,
                "Geo satisfaction file binding ids must be non-empty and trimmed",
                [(format!("{field}.binding_id"), binding.binding_id.clone())],
            ));
        }
        if result
            .insert(binding.binding_id.clone(), binding.path.clone())
            .is_some()
        {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::InvalidInput,
                "Geo satisfaction file bindings must have distinct ids",
                [(field.to_string(), binding.binding_id.clone())],
            ));
        }
    }
    Ok(result)
}

fn sorted_vec<T: Ord>(mut values: Vec<T>) -> Vec<T> {
    values.sort();
    values
}

fn fingerprint_file(path: &Path, file_id: &str) -> Result<FileFingerprint, GeoSatisfyError> {
    let bytes = read_file(path, file_id)?;
    Ok(FileFingerprint {
        byte_count: u64::try_from(bytes.len()).map_err(|_| byte_count_overflow(file_id))?,
        digest: blake3_prefixed(&bytes),
        bytes,
    })
}

fn read_file(path: &Path, file_id: &str) -> Result<Vec<u8>, GeoSatisfyError> {
    fs::read(path).map_err(|error| file_read_error(file_id, path, error))
}

fn file_read_error(file_id: &str, path: &Path, error: io::Error) -> GeoSatisfyError {
    GeoSatisfyError::new(
        GeoSatisfyErrorCode::FileRead,
        "Geo satisfaction could not read a declared local file",
        [
            ("file_id", file_id.to_string()),
            ("path", path.display().to_string()),
            ("error", error.to_string()),
        ],
    )
}

fn byte_count_overflow(file_id: &str) -> GeoSatisfyError {
    GeoSatisfyError::new(
        GeoSatisfyErrorCode::FileByteCountMismatch,
        "Geo satisfaction local file byte count overflowed u64",
        [("file_id", file_id)],
    )
}

fn blake3_prefixed(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn digest_json(value: &impl Serialize) -> Result<String, GeoSatisfyError> {
    serde_json::to_vec(value)
        .map(|bytes| blake3_prefixed(&bytes))
        .map_err(|error| {
            GeoSatisfyError::new(
                GeoSatisfyErrorCode::Serialization,
                "Geo satisfaction semantic projection could not be serialized",
                [("error", error.to_string())],
            )
        })
}

fn digest_algorithm_name(algorithm: GeoDigestAlgorithm) -> &'static str {
    match algorithm {
        GeoDigestAlgorithm::Blake3 => "blake3",
        GeoDigestAlgorithm::Sha256 => "sha256",
        GeoDigestAlgorithm::Sha512 => "sha512",
    }
}

fn discovery_error(error: GeoDiscoveryError) -> GeoSatisfyError {
    GeoSatisfyError::new(
        match error.code {
            GeoDiscoveryErrorCode::UnsupportedVersion => GeoSatisfyErrorCode::UnsupportedVersion,
            GeoDiscoveryErrorCode::ReceiptMismatch => GeoSatisfyErrorCode::ReceiptMismatch,
            GeoDiscoveryErrorCode::SemanticIdMismatch => GeoSatisfyErrorCode::ContractMismatch,
            GeoDiscoveryErrorCode::InvalidInput | GeoDiscoveryErrorCode::SecretMaterial => {
                GeoSatisfyErrorCode::InvalidInput
            }
        },
        error.message,
        error.detail,
    )
}

fn plan_error(error: super::GeoPlanError) -> GeoSatisfyError {
    let mut detail = error.detail;
    detail.insert("plan_error_code".to_string(), format!("{:?}", error.code));
    GeoSatisfyError::new(
        match error.code {
            super::GeoPlanErrorCode::UnsupportedVersion => GeoSatisfyErrorCode::UnsupportedVersion,
            _ => GeoSatisfyErrorCode::ContractMismatch,
        },
        "Geo inventory advancement requires a valid Geo plan",
        detail,
    )
}

fn control_error(error: super::GeoControlError) -> GeoSatisfyError {
    GeoSatisfyError::new(
        match error.code {
            super::GeoControlErrorCode::UnsupportedVersion => {
                GeoSatisfyErrorCode::UnsupportedVersion
            }
            _ => GeoSatisfyErrorCode::InvalidInput,
        },
        error.message,
        error.detail,
    )
}
