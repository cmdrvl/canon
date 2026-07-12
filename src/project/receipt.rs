#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fmt, fs, io, path::Path};

pub const CANON_PROJECT_RUN_VERSION: &str = "canon.project.run.v1";

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
    pub dependency_receipt_hashes: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<ProjectRunOutputReceipt>,
    pub outcome: ProjectRunNodeOutcome,
    pub duration_millis: u64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resource_observations: BTreeMap<String, u64>,
    pub next_action: ProjectRunNextAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_message: Option<String>,
    pub receipt_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRunReceipt {
    pub schema_version: String,
    pub project_id: String,
    pub plan_graph_hash: String,
    pub receipt_hash: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed_nodes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed_nodes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cancelled_nodes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidated_nodes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_nodes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_receipts: Vec<ProjectRunNodeReceipt>,
}

pub fn finalized_node_receipt(
    mut receipt: ProjectRunNodeReceipt,
) -> ProjectReceiptResult<ProjectRunNodeReceipt> {
    canonicalize_node_receipt(&mut receipt);
    receipt.receipt_hash.clear();
    receipt.receipt_hash = compute_node_receipt_hash(&receipt)?;
    Ok(receipt)
}

pub fn canonical_node_receipt_bytes(
    receipt: &ProjectRunNodeReceipt,
) -> ProjectReceiptResult<Vec<u8>> {
    let mut canonical = receipt.clone();
    canonicalize_node_receipt(&mut canonical);
    let expected = compute_node_receipt_hash(&canonical)?;
    if canonical.receipt_hash != expected {
        return Err(ProjectReceiptError::new(
            ProjectReceiptErrorCode::HashMismatch,
            format!(
                "node receipt {} hash mismatch: expected {expected}, got {}",
                canonical.node_id, canonical.receipt_hash
            ),
        ));
    }
    serde_json::to_vec(&canonical).map_err(|error| {
        ProjectReceiptError::new(
            ProjectReceiptErrorCode::ArtifactContract,
            format!("failed to serialize node receipt: {error}"),
        )
    })
}

pub fn parse_node_receipt(bytes: &[u8]) -> ProjectReceiptResult<ProjectRunNodeReceipt> {
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
    let bytes = canonical_node_receipt_bytes(receipt)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    }
    fs::write(path, bytes).map_err(|error| io_error(path, error))
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

fn io_error(path: &Path, error: io::Error) -> ProjectReceiptError {
    ProjectReceiptError::new(
        ProjectReceiptErrorCode::Io,
        format!(
            "failed to write project receipt {}: {error}",
            path.display()
        ),
    )
}
