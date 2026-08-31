#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    error::Error,
    fmt, fs,
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub const CANON_PROJECT_RUN_VERSION: &str = "canon.project.run.v2";

pub fn project_run_schema_version() -> &'static str {
    CANON_PROJECT_RUN_VERSION
}

pub type ProjectReceiptResult<T> = Result<T, ProjectReceiptError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectReceiptErrorCode {
    ArtifactContract,
    Parse,
    Io,
    HashMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectReceiptError {
    pub code: ProjectReceiptErrorCode,
    pub message: String,
}

impl ProjectReceiptError {
    pub fn new(code: ProjectReceiptErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ProjectReceiptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for ProjectReceiptError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRunNodeOutcome {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRunNextAction {
    ReuseReceipt,
    ExecuteDependents,
    RetryNode,
    InspectFailure,
    Resume,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRunHashRef {
    pub ref_id: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRunOutputReceipt {
    pub output_id: String,
    pub path: String,
    pub content_digest: String,
    pub byte_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRunNodeReceipt {
    pub schema_version: String,
    pub project_id: String,
    pub plan_graph_hash: String,
    pub node_id: String,
    pub node_cache_key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content_hash_inputs: Vec<ProjectRunHashRef>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependency_semantic_hashes: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependency_receipt_hashes: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<ProjectRunOutputReceipt>,
    pub outcome: ProjectRunNodeOutcome,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deterministic_usage: BTreeMap<String, u64>,
    pub duration_millis: u64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resource_observations: BTreeMap<String, u64>,
    pub next_action: ProjectRunNextAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_message: Option<String>,
    pub semantic_hash: String,
    pub telemetry_hash: String,
    pub receipt_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRunReceipt {
    pub schema_version: String,
    pub project_id: String,
    pub plan_graph_hash: String,
    pub receipt_hash: String,
    #[serde(default)]
    pub completed_nodes: Vec<String>,
    #[serde(default)]
    pub failed_nodes: Vec<String>,
    #[serde(default)]
    pub cancelled_nodes: Vec<String>,
    #[serde(default)]
    pub invalidated_nodes: Vec<String>,
    #[serde(default)]
    pub blocked_nodes: Vec<String>,
    #[serde(default)]
    pub node_receipts: Vec<ProjectRunNodeReceipt>,
}

pub fn finalized_node_receipt(
    mut receipt: ProjectRunNodeReceipt,
) -> ProjectReceiptResult<ProjectRunNodeReceipt> {
    canonicalize_node_receipt(&mut receipt);
    receipt.semantic_hash.clear();
    receipt.telemetry_hash.clear();
    receipt.receipt_hash.clear();
    receipt.semantic_hash = compute_node_semantic_hash(&receipt)?;
    receipt.telemetry_hash = compute_node_telemetry_hash(&receipt)?;
    receipt.receipt_hash = compute_node_receipt_hash(&receipt)?;
    Ok(receipt)
}

pub fn canonical_node_receipt_bytes(
    receipt: &ProjectRunNodeReceipt,
) -> ProjectReceiptResult<Vec<u8>> {
    let canonical = validate_node_receipt(receipt.clone())?;
    serde_json::to_vec(&canonical).map_err(|error| {
        ProjectReceiptError::new(
            ProjectReceiptErrorCode::ArtifactContract,
            format!("failed to serialize node receipt: {error}"),
        )
    })
}

pub fn parse_node_receipt(bytes: &[u8]) -> ProjectReceiptResult<ProjectRunNodeReceipt> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|error| {
        ProjectReceiptError::new(
            ProjectReceiptErrorCode::Parse,
            format!("failed to parse project node receipt JSON: {error}"),
        )
    })?;
    if value
        .get("schema_version")
        .and_then(|schema| schema.as_str())
        == Some("canon.project.run.v1")
    {
        return Err(ProjectReceiptError::new(
            ProjectReceiptErrorCode::ArtifactContract,
            "project node receipt uses canon.project.run.v1; v1 receipts are not execution-reusable after the semantic/telemetry split, so refresh the work directory with canon.project.run.v2 receipts before resuming",
        ));
    }
    let receipt: ProjectRunNodeReceipt = serde_json::from_slice(bytes).map_err(|error| {
        ProjectReceiptError::new(
            ProjectReceiptErrorCode::Parse,
            format!("failed to parse project node receipt: {error}"),
        )
    })?;
    validate_node_receipt(receipt)
}

pub fn read_node_receipt(path: &Path) -> ProjectReceiptResult<ProjectRunNodeReceipt> {
    let bytes = fs::read(path).map_err(|error| {
        ProjectReceiptError::new(
            ProjectReceiptErrorCode::Io,
            format!(
                "failed to read project node receipt {}: {error}",
                path.display()
            ),
        )
    })?;
    parse_node_receipt(&bytes)
}

pub fn write_node_receipt(
    path: &Path,
    receipt: &ProjectRunNodeReceipt,
) -> ProjectReceiptResult<()> {
    replace_node_receipt(path, receipt, None)
}

pub fn replace_node_receipt(
    path: &Path,
    receipt: &ProjectRunNodeReceipt,
    expected_existing: Option<&ProjectRunNodeReceipt>,
) -> ProjectReceiptResult<()> {
    let bytes = canonical_node_receipt_bytes(receipt)?;
    let expected_existing_bytes = expected_existing
        .map(canonical_node_receipt_bytes)
        .transpose()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    }
    write_atomic_replace(path, &bytes, expected_existing_bytes.as_deref())
}

pub fn finalized_run_receipt(
    mut receipt: ProjectRunReceipt,
) -> ProjectReceiptResult<ProjectRunReceipt> {
    canonicalize_run_receipt(&mut receipt);
    receipt.receipt_hash.clear();
    receipt.receipt_hash = compute_run_receipt_hash(&receipt)?;
    Ok(receipt)
}

pub fn canonical_run_receipt_bytes(receipt: &ProjectRunReceipt) -> ProjectReceiptResult<Vec<u8>> {
    let mut canonical = receipt.clone();
    canonicalize_run_receipt(&mut canonical);
    let expected = compute_run_receipt_hash(&canonical)?;
    if canonical.receipt_hash != expected {
        return Err(ProjectReceiptError::new(
            ProjectReceiptErrorCode::HashMismatch,
            format!(
                "run receipt hash mismatch: expected {expected}, got {}",
                canonical.receipt_hash
            ),
        ));
    }
    serde_json::to_vec(&canonical).map_err(|error| {
        ProjectReceiptError::new(
            ProjectReceiptErrorCode::ArtifactContract,
            format!("failed to serialize run receipt: {error}"),
        )
    })
}

pub fn validate_node_receipt(
    mut receipt: ProjectRunNodeReceipt,
) -> ProjectReceiptResult<ProjectRunNodeReceipt> {
    if receipt.schema_version != CANON_PROJECT_RUN_VERSION {
        return Err(ProjectReceiptError::new(
            ProjectReceiptErrorCode::ArtifactContract,
            format!("node receipt schema_version must equal {CANON_PROJECT_RUN_VERSION}"),
        ));
    }
    if receipt.project_id.trim().is_empty() || receipt.node_id.trim().is_empty() {
        return Err(ProjectReceiptError::new(
            ProjectReceiptErrorCode::ArtifactContract,
            "node receipt project_id and node_id must be non-empty",
        ));
    }
    canonicalize_node_receipt(&mut receipt);
    let expected_semantic = compute_node_semantic_hash(&receipt)?;
    if receipt.semantic_hash != expected_semantic {
        return Err(ProjectReceiptError::new(
            ProjectReceiptErrorCode::HashMismatch,
            format!(
                "node receipt {} semantic hash mismatch: expected {expected_semantic}, got {}",
                receipt.node_id, receipt.semantic_hash
            ),
        ));
    }
    let expected_telemetry = compute_node_telemetry_hash(&receipt)?;
    if receipt.telemetry_hash != expected_telemetry {
        return Err(ProjectReceiptError::new(
            ProjectReceiptErrorCode::HashMismatch,
            format!(
                "node receipt {} telemetry hash mismatch: expected {expected_telemetry}, got {}",
                receipt.node_id, receipt.telemetry_hash
            ),
        ));
    }
    let expected = compute_node_receipt_hash(&receipt)?;
    if receipt.receipt_hash != expected {
        return Err(ProjectReceiptError::new(
            ProjectReceiptErrorCode::HashMismatch,
            format!(
                "node receipt {} hash mismatch: expected {expected}, got {}",
                receipt.node_id, receipt.receipt_hash
            ),
        ));
    }
    Ok(receipt)
}

pub fn digest_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn compute_node_semantic_hash(receipt: &ProjectRunNodeReceipt) -> ProjectReceiptResult<String> {
    #[derive(Serialize)]
    struct SemanticOutput<'a> {
        output_id: &'a str,
        content_digest: &'a str,
        byte_count: u64,
    }

    #[derive(Serialize)]
    struct SemanticReceipt<'a> {
        schema_version: &'a str,
        project_id: &'a str,
        node_id: &'a str,
        node_cache_key: &'a str,
        content_hash_inputs: &'a [ProjectRunHashRef],
        dependency_semantic_hashes: &'a BTreeMap<String, String>,
        outputs: Vec<SemanticOutput<'a>>,
        outcome: ProjectRunNodeOutcome,
        deterministic_usage: &'a BTreeMap<String, u64>,
        next_action: ProjectRunNextAction,
        failure_code: &'a Option<String>,
        failure_message: &'a Option<String>,
    }

    let mut canonical = receipt.clone();
    canonicalize_node_receipt(&mut canonical);
    let semantic = SemanticReceipt {
        schema_version: &canonical.schema_version,
        project_id: &canonical.project_id,
        node_id: &canonical.node_id,
        node_cache_key: &canonical.node_cache_key,
        content_hash_inputs: &canonical.content_hash_inputs,
        dependency_semantic_hashes: &canonical.dependency_semantic_hashes,
        outputs: canonical
            .outputs
            .iter()
            .map(|output| SemanticOutput {
                output_id: &output.output_id,
                content_digest: &output.content_digest,
                byte_count: output.byte_count,
            })
            .collect(),
        outcome: canonical.outcome,
        deterministic_usage: &canonical.deterministic_usage,
        next_action: canonical.next_action,
        failure_code: &canonical.failure_code,
        failure_message: &canonical.failure_message,
    };
    serde_json::to_vec(&semantic)
        .map(|bytes| digest_bytes(&bytes))
        .map_err(|error| {
            ProjectReceiptError::new(
                ProjectReceiptErrorCode::ArtifactContract,
                format!("failed to hash node semantic receipt: {error}"),
            )
        })
}

fn compute_node_telemetry_hash(receipt: &ProjectRunNodeReceipt) -> ProjectReceiptResult<String> {
    #[derive(Serialize)]
    struct TelemetryOutput<'a> {
        output_id: &'a str,
        path: &'a str,
    }

    #[derive(Serialize)]
    struct TelemetryReceipt<'a> {
        schema_version: &'a str,
        project_id: &'a str,
        plan_graph_hash: &'a str,
        node_id: &'a str,
        duration_millis: u64,
        resource_observations: &'a BTreeMap<String, u64>,
        publication_paths: Vec<TelemetryOutput<'a>>,
    }

    let mut canonical = receipt.clone();
    canonicalize_node_receipt(&mut canonical);
    let telemetry = TelemetryReceipt {
        schema_version: &canonical.schema_version,
        project_id: &canonical.project_id,
        plan_graph_hash: &canonical.plan_graph_hash,
        node_id: &canonical.node_id,
        duration_millis: canonical.duration_millis,
        resource_observations: &canonical.resource_observations,
        publication_paths: canonical
            .outputs
            .iter()
            .map(|output| TelemetryOutput {
                output_id: &output.output_id,
                path: &output.path,
            })
            .collect(),
    };
    serde_json::to_vec(&telemetry)
        .map(|bytes| digest_bytes(&bytes))
        .map_err(|error| {
            ProjectReceiptError::new(
                ProjectReceiptErrorCode::ArtifactContract,
                format!("failed to hash node telemetry receipt: {error}"),
            )
        })
}

fn compute_node_receipt_hash(receipt: &ProjectRunNodeReceipt) -> ProjectReceiptResult<String> {
    let mut hashable = receipt.clone();
    canonicalize_node_receipt(&mut hashable);
    hashable.receipt_hash.clear();
    serde_json::to_vec(&hashable)
        .map(|bytes| digest_bytes(&bytes))
        .map_err(|error| {
            ProjectReceiptError::new(
                ProjectReceiptErrorCode::ArtifactContract,
                format!("failed to hash node receipt: {error}"),
            )
        })
}

fn compute_run_receipt_hash(receipt: &ProjectRunReceipt) -> ProjectReceiptResult<String> {
    let mut hashable = receipt.clone();
    canonicalize_run_receipt(&mut hashable);
    hashable.receipt_hash.clear();
    serde_json::to_vec(&hashable)
        .map(|bytes| digest_bytes(&bytes))
        .map_err(|error| {
            ProjectReceiptError::new(
                ProjectReceiptErrorCode::ArtifactContract,
                format!("failed to hash run receipt: {error}"),
            )
        })
}

fn canonicalize_node_receipt(receipt: &mut ProjectRunNodeReceipt) {
    receipt
        .content_hash_inputs
        .sort_by(|left, right| left.ref_id.cmp(&right.ref_id));
    receipt
        .outputs
        .sort_by(|left, right| left.output_id.cmp(&right.output_id));
}

fn canonicalize_run_receipt(receipt: &mut ProjectRunReceipt) {
    receipt.completed_nodes.sort();
    receipt.failed_nodes.sort();
    receipt.cancelled_nodes.sort();
    receipt.invalidated_nodes.sort();
    receipt.blocked_nodes.sort();
    receipt
        .node_receipts
        .sort_by(|left, right| left.node_id.cmp(&right.node_id));
}

fn write_atomic_replace(
    path: &Path,
    bytes: &[u8],
    expected_existing: Option<&[u8]>,
) -> ProjectReceiptResult<()> {
    let temp_path = atomic_receipt_temp_path(path, bytes);
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
    {
        Ok(mut file) => {
            file.write_all(bytes)
                .map_err(|error| cleanup_io_error(&temp_path, error))?;
            file.sync_all()
                .map_err(|error| cleanup_io_error(&temp_path, error))?;
            drop(file);
            finish_atomic_receipt_replace(path, &temp_path, bytes, expected_existing)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            recover_atomic_receipt_temp(path, &temp_path, bytes, expected_existing)
        }
        Err(error) => Err(io_error(&temp_path, error)),
    }
}

fn recover_atomic_receipt_temp(
    path: &Path,
    temp_path: &Path,
    bytes: &[u8],
    expected_existing: Option<&[u8]>,
) -> ProjectReceiptResult<()> {
    let existing = fs::read(temp_path).map_err(|error| io_error(temp_path, error))?;
    if existing != bytes {
        return Err(ProjectReceiptError::new(
            ProjectReceiptErrorCode::Io,
            format!(
                "refusing to reuse atomic project receipt temp {} because its contents do not match the intended receipt bytes",
                temp_path.display()
            ),
        ));
    }
    finish_atomic_receipt_replace(path, temp_path, bytes, expected_existing)
}

fn finish_atomic_receipt_replace(
    path: &Path,
    temp_path: &Path,
    bytes: &[u8],
    expected_existing: Option<&[u8]>,
) -> ProjectReceiptResult<()> {
    match fs::read(path) {
        Ok(existing) if existing == bytes => {
            let _ = fs::remove_file(temp_path);
            Ok(())
        }
        Ok(existing) if expected_existing.is_some_and(|expected| expected == existing) => {
            fs::rename(temp_path, path).map_err(|error| io_error(temp_path, error))
        }
        Ok(_) => {
            let _ = fs::remove_file(temp_path);
            Err(ProjectReceiptError::new(
                ProjectReceiptErrorCode::Io,
                format!(
                    "refusing to replace existing project receipt {} because its bytes differ from the intended receipt",
                    path.display()
                ),
            ))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::rename(temp_path, path).map_err(|error| io_error(temp_path, error))
        }
        Err(error) => Err(io_error(path, error)),
    }
}

fn atomic_receipt_temp_path(path: &Path, bytes: &[u8]) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("receipt");
    let token = digest_bytes(bytes).replace(':', "_");
    path.with_file_name(format!("{file_name}.{token}.tmp"))
}

fn cleanup_io_error(path: &Path, error: io::Error) -> ProjectReceiptError {
    let _ = fs::remove_file(path);
    io_error(path, error)
}

fn io_error(path: &Path, error: io::Error) -> ProjectReceiptError {
    ProjectReceiptError::new(
        ProjectReceiptErrorCode::Io,
        format!(
            "failed to write project receipt {}: {error}",
            path.display()
        ),
    )
}
