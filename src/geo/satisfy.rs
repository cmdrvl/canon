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
    GEO_RUN_JSON_MEDIA_TYPE, GEO_SOLVE_COMMAND, GEO_TILE_WORK_COMMAND, GeoAcquisitionDenominator,
    GeoAcquisitionProofClass, GeoAcquisitionReceipt, GeoAcquisitionRequest,
    GeoAcquisitionTerminalState, GeoDigest, GeoDigestAlgorithm, GeoDiscoveryError,
    GeoDiscoveryErrorCode, GeoLocalArtifactDigest, GeoLocalArtifactRef, GeoNativeEntityScope,
    GeoPlan, GeoPlanAcquisitionHandoff, GeoPlanExternalRequest, GeoRegionalInventory,
    GeoRegionalSourceInstance, GeoRunInputBinding, GeoSourceAvailability,
    canonicalize_regional_inventory, geo_acquisition_receipt_satisfies_positive_gate,
    geo_acquisition_request_id, geo_acquisition_request_semantic_hash, geo_run_input_artifact_id,
    validate_geo_acquisition_receipt,
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
    ArtifactReleaseRelationAbsent,
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
    findings: &'a [GeoSatisfactionFinding],
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
    satisfy_geo_acquisition_core(input).map(|core| core.satisfaction)
}

pub fn satisfy_geo_acquisition_for_run(
    input: GeoSatisfactionRunInput<'_>,
) -> Result<GeoAcquisitionRunSatisfaction, GeoSatisfyError> {
    let local_artifact_files = input
        .run_input_files
        .iter()
        .map(|binding| GeoSatisfactionFileBinding {
            binding_id: binding.local_artifact_id.clone(),
            path: binding.path.clone(),
        })
        .collect::<Vec<_>>();
    let core = satisfy_geo_acquisition_core(GeoSatisfactionInput {
        plan: input.plan,
        inventory: input.inventory,
        assignment: input.assignment,
        local_artifact_files,
        result_digest_files: input.result_digest_files,
    })?;
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
    let artifact_release_relation_absent = positive_gate && receipt.releases.len() != 1;
    let status = if positive_gate && !artifact_release_relation_absent {
        GeoSatisfactionStatus::Satisfied
    } else {
        GeoSatisfactionStatus::NotSatisfied
    };
    let mut findings = if artifact_release_relation_absent {
        Vec::new()
    } else {
        findings_for_receipt(request, &receipt, positive_gate)
    };
    let bindings = if status == GeoSatisfactionStatus::Satisfied {
        build_bindings(request, &receipt, &local_artifact_files)?
    } else {
        Vec::new()
    };
    if artifact_release_relation_absent {
        findings.push(finding(
            GeoSatisfactionFindingCode::ArtifactReleaseRelationAbsent,
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
    let updated_inventory = if status == GeoSatisfactionStatus::Satisfied {
        input
            .inventory
            .map(|inventory| updated_inventory(inventory, request, &bindings))
            .transpose()?
    } else {
        None
    };

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

fn build_bindings(
    request: &GeoAcquisitionRequest,
    receipt: &GeoAcquisitionReceipt,
    local_artifacts: &BTreeMap<String, (GeoLocalArtifactDigest, FileFingerprint)>,
) -> Result<Vec<GeoSatisfactionLocalInputBinding>, GeoSatisfyError> {
    let [release] = receipt.releases.as_slice() else {
        return Ok(Vec::new());
    };
    let mut bindings = Vec::new();
    for (artifact, fingerprint) in local_artifacts.values() {
        let result_digest_ids = receipt
            .result_digests
            .iter()
            .filter(|digest| {
                digest.algorithm == GeoDigestAlgorithm::Blake3
                    && fingerprint.digest == format!("blake3:{}", digest.hex_digest)
            })
            .map(|digest| digest.digest_id.clone())
            .collect::<Vec<_>>();
        let binding_payload = (
            &request.request_id,
            &receipt.request_semantic_hash,
            &release.release_id,
            &release.release_digest,
            &artifact.artifact_id,
            &artifact.media_type,
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
            source_instance_id: release.source_instance_id.clone(),
            release_id: release.release_id.clone(),
            release_digest: format!(
                "{}:{}",
                digest_algorithm_name(release.release_digest.algorithm),
                release.release_digest.hex_digest
            ),
            local_artifact_id: artifact.artifact_id.clone(),
            media_type: artifact.media_type.clone(),
            content_hash: fingerprint.digest.clone(),
            byte_count: fingerprint.byte_count,
            result_digest_ids,
        });
    }
    bindings.sort();
    Ok(bindings)
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

fn updated_inventory(
    inventory: &GeoRegionalInventory,
    request: &GeoAcquisitionRequest,
    bindings: &[GeoSatisfactionLocalInputBinding],
) -> Result<GeoRegionalInventory, GeoSatisfyError> {
    let mut updated = inventory.clone();
    for binding in bindings {
        let matching_release = request.releases.iter().any(|release| {
            release.source_instance_id == binding.source_instance_id
                && release.release_id == binding.release_id
                && format!(
                    "{}:{}",
                    digest_algorithm_name(release.release_digest.algorithm),
                    release.release_digest.hex_digest
                ) == binding.release_digest
        });
        if !matching_release {
            return Err(GeoSatisfyError::new(
                GeoSatisfyErrorCode::ReceiptMismatch,
                "Geo local binding does not match any acquisition release pin",
                [("binding_id", binding.binding_id.as_str())],
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
        source.local_state.state = GeoSourceAvailability::Available;
        source.local_state.local_ref = Some(GeoLocalArtifactRef {
            artifact_id: binding.local_artifact_id.clone(),
            content_hash: binding.content_hash.clone(),
            media_type: binding.media_type.clone(),
        });
    }
    canonicalize_regional_inventory(&updated).map_err(control_error)
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
