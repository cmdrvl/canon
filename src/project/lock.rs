#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    path::{Component, Path, PathBuf},
};

pub const CANON_PROJECT_LOCK_VERSION: &str = "canon.project.lock.v1";

const SECRET_PREFIXES: [&str; 5] = ["env:", "keyring:", "op://", "aws-sm://", "gcp-sm://"];

pub fn project_lock_schema_version() -> &'static str {
    CANON_PROJECT_LOCK_VERSION
}

pub fn digest_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

pub type ProjectLockResult<T> = Result<T, ProjectLockError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLockErrorCode {
    ArtifactContract,
    PathPolicy,
    SecretPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLockError {
    pub code: ProjectLockErrorCode,
    pub message: String,
}

impl ProjectLockError {
    pub fn new(code: ProjectLockErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ProjectLockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for ProjectLockError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLockRefKind {
    Package,
    Strategy,
    Policy,
    ToolContract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLockInput {
    pub input_id: String,
    pub relative_path: String,
    pub content_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLockResolvedRef {
    pub ref_id: String,
    pub kind: ProjectLockRefKind,
    pub resolved_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLockManifestProjection {
    pub project_id: String,
    pub project_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<ProjectLockInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolved_refs: Vec<ProjectLockResolvedRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLock {
    pub schema_version: String,
    pub project_id: String,
    pub project_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<ProjectLockInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolved_refs: Vec<ProjectLockResolvedRef>,
    pub lock_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLockDiffKind {
    InputDrift,
    ResolvedDigestDrift,
    ToolContractDrift,
    MissingInput,
    MissingResolvedRef,
    UnexpectedInput,
    UnexpectedResolvedRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLockDiff {
    pub kind: ProjectLockDiffKind,
    pub subject: String,
    pub field: String,
    pub expected: String,
    pub actual: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLockVerificationStatus {
    Fresh,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLockVerification {
    pub status: ProjectLockVerificationStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stale_diffs: Vec<ProjectLockDiff>,
    pub current_lock: ProjectLock,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLockRefreshReceipt {
    pub schema_version: String,
    pub project_id: String,
    pub previous_lock_digest: String,
    pub refreshed_lock_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stale_diffs: Vec<ProjectLockDiff>,
    pub refreshed_lock: ProjectLock,
}

pub fn refresh_project_lock(
    projection: &ProjectLockManifestProjection,
) -> ProjectLockResult<ProjectLock> {
    let projection = validate_projection(projection.clone())?;
    let mut lock = ProjectLock {
        schema_version: CANON_PROJECT_LOCK_VERSION.to_string(),
        project_id: projection.project_id,
        project_digest: projection.project_digest,
        inputs: projection.inputs,
        resolved_refs: projection.resolved_refs,
        lock_digest: String::new(),
    };
    sort_lock(&mut lock);
    lock.lock_digest = compute_lock_digest(&lock)?;
    Ok(lock)
}

pub fn verify_project_lock(
    lock: &ProjectLock,
    projection: &ProjectLockManifestProjection,
) -> ProjectLockResult<ProjectLockVerification> {
    let expected = validate_lock(lock.clone())?;
    let current = refresh_project_lock(projection)?;
    let stale_diffs = diff_locks(&expected, &current);
    let status = if stale_diffs.is_empty() {
        ProjectLockVerificationStatus::Fresh
    } else {
        ProjectLockVerificationStatus::Stale
    };
    Ok(ProjectLockVerification {
        status,
        stale_diffs,
        current_lock: current,
    })
}

pub fn refresh_project_lock_receipt(
    existing_lock: &ProjectLock,
    projection: &ProjectLockManifestProjection,
) -> ProjectLockResult<ProjectLockRefreshReceipt> {
    let existing_lock = validate_lock(existing_lock.clone())?;
    let verification = verify_project_lock(&existing_lock, projection)?;
    Ok(ProjectLockRefreshReceipt {
        schema_version: CANON_PROJECT_LOCK_VERSION.to_string(),
        project_id: verification.current_lock.project_id.clone(),
        previous_lock_digest: existing_lock.lock_digest,
        refreshed_lock_digest: verification.current_lock.lock_digest.clone(),
        stale_diffs: verification.stale_diffs,
        refreshed_lock: verification.current_lock,
    })
}

pub fn canonical_project_lock_bytes(lock: &ProjectLock) -> ProjectLockResult<Vec<u8>> {
    let canonical = validate_lock(lock.clone())?;
    serde_json::to_vec(&canonical).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize canonical project lock: {error}"
        ))
    })
}

pub fn project_lock_digest(lock: &ProjectLock) -> ProjectLockResult<String> {
    let canonical = validate_lock(lock.clone())?;
    Ok(canonical.lock_digest)
}

fn validate_projection(
    mut projection: ProjectLockManifestProjection,
) -> ProjectLockResult<ProjectLockManifestProjection> {
    projection.project_id = sanitized_text(&projection.project_id, "project_id")?;
    projection.project_digest = normalized_digest(&projection.project_digest, "project_digest")?;
    validate_inputs(&mut projection.inputs)?;
    validate_resolved_refs(&mut projection.resolved_refs)?;
    Ok(projection)
}

fn validate_lock(mut lock: ProjectLock) -> ProjectLockResult<ProjectLock> {
    if lock.schema_version != CANON_PROJECT_LOCK_VERSION {
        return Err(artifact_contract_error(format!(
            "schema_version must equal {CANON_PROJECT_LOCK_VERSION}"
        )));
    }
    lock.project_id = sanitized_text(&lock.project_id, "project_id")?;
    lock.project_digest = normalized_digest(&lock.project_digest, "project_digest")?;
    validate_inputs(&mut lock.inputs)?;
    validate_resolved_refs(&mut lock.resolved_refs)?;
    sort_lock(&mut lock);

    let expected_digest = compute_lock_digest(&lock)?;
    if lock.lock_digest != expected_digest {
        return Err(artifact_contract_error(format!(
            "lock_digest must match canonical lock bytes: expected {expected_digest}, got {}",
            lock.lock_digest
        )));
    }

    Ok(lock)
}

fn validate_inputs(inputs: &mut Vec<ProjectLockInput>) -> ProjectLockResult<()> {
    let mut ids = BTreeSet::new();
    for input in &mut *inputs {
        input.input_id = sanitized_text(&input.input_id, "inputs.input_id")?;
        input.relative_path =
            normalized_relative_path(&input.relative_path, "inputs.relative_path")?;
        input.content_digest = normalized_digest(&input.content_digest, "inputs.content_digest")?;
        if !ids.insert(input.input_id.clone()) {
            return Err(artifact_contract_error(format!(
                "duplicate input_id {}",
                input.input_id
            )));
        }
    }
    inputs.sort_by(|left, right| left.input_id.cmp(&right.input_id));
    Ok(())
}

fn validate_resolved_refs(refs: &mut Vec<ProjectLockResolvedRef>) -> ProjectLockResult<()> {
    let mut ids = BTreeMap::new();
    for resolved in &mut *refs {
        resolved.ref_id = sanitized_text(&resolved.ref_id, "resolved_refs.ref_id")?;
        resolved.resolved_digest =
            normalized_digest(&resolved.resolved_digest, "resolved_refs.resolved_digest")?;
        if let Some(previous_kind) = ids.insert(resolved.ref_id.clone(), resolved.kind) {
            return Err(artifact_contract_error(format!(
                "duplicate ref_id {} across {:?} and {:?}",
                resolved.ref_id, previous_kind, resolved.kind
            )));
        }
    }
    refs.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.ref_id.cmp(&right.ref_id))
    });
    Ok(())
}

fn sort_lock(lock: &mut ProjectLock) {
    lock.inputs
        .sort_by(|left, right| left.input_id.cmp(&right.input_id));
    lock.resolved_refs.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.ref_id.cmp(&right.ref_id))
    });
}

fn compute_lock_digest(lock: &ProjectLock) -> ProjectLockResult<String> {
    let mut hashable = lock.clone();
    hashable.lock_digest.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize project lock for hashing: {error}"
        ))
    })?;
    Ok(digest_bytes(&bytes))
}

fn diff_locks(expected: &ProjectLock, actual: &ProjectLock) -> Vec<ProjectLockDiff> {
    let mut diffs = Vec::new();

    if expected.project_digest != actual.project_digest {
        diffs.push(ProjectLockDiff {
            kind: ProjectLockDiffKind::ResolvedDigestDrift,
            subject: "project".to_string(),
            field: "project_digest".to_string(),
            expected: expected.project_digest.clone(),
            actual: actual.project_digest.clone(),
            message:
                "Project digest drifted; review the manifest projection and run an explicit lock refresh"
                    .to_string(),
        });
    }

    let expected_inputs = expected
        .inputs
        .iter()
        .map(|input| (input.input_id.as_str(), input))
        .collect::<BTreeMap<_, _>>();
    let actual_inputs = actual
        .inputs
        .iter()
        .map(|input| (input.input_id.as_str(), input))
        .collect::<BTreeMap<_, _>>();

    for (input_id, expected_input) in &expected_inputs {
        let Some(actual_input) = actual_inputs.get(input_id) else {
            diffs.push(ProjectLockDiff {
                kind: ProjectLockDiffKind::MissingInput,
                subject: (*input_id).to_string(),
                field: "input_id".to_string(),
                expected: (*input_id).to_string(),
                actual: "[missing]".to_string(),
                message: format!(
                    "Input {input_id} is missing; restore it or run an explicit lock refresh"
                ),
            });
            continue;
        };

        if expected_input.relative_path != actual_input.relative_path {
            diffs.push(ProjectLockDiff {
                kind: ProjectLockDiffKind::InputDrift,
                subject: (*input_id).to_string(),
                field: "relative_path".to_string(),
                expected: expected_input.relative_path.clone(),
                actual: actual_input.relative_path.clone(),
                message: format!(
                    "Input {input_id} resolved to a different relative path; review the projection and refresh the lock"
                ),
            });
        }

        if expected_input.content_digest != actual_input.content_digest {
            diffs.push(ProjectLockDiff {
                kind: ProjectLockDiffKind::InputDrift,
                subject: (*input_id).to_string(),
                field: "content_digest".to_string(),
                expected: expected_input.content_digest.clone(),
                actual: actual_input.content_digest.clone(),
                message: format!(
                    "Input bytes changed for {input_id}; review the new bytes and run an explicit lock refresh"
                ),
            });
        }
    }

    for input_id in actual_inputs.keys() {
        if !expected_inputs.contains_key(input_id) {
            diffs.push(ProjectLockDiff {
                kind: ProjectLockDiffKind::UnexpectedInput,
                subject: (*input_id).to_string(),
                field: "input_id".to_string(),
                expected: "[absent]".to_string(),
                actual: (*input_id).to_string(),
                message: format!(
                    "Unexpected input {input_id} appeared in the projection; review it before refreshing the lock"
                ),
            });
        }
    }

    let expected_refs = expected
        .resolved_refs
        .iter()
        .map(|resolved| (resolved.ref_id.as_str(), resolved))
        .collect::<BTreeMap<_, _>>();
    let actual_refs = actual
        .resolved_refs
        .iter()
        .map(|resolved| (resolved.ref_id.as_str(), resolved))
        .collect::<BTreeMap<_, _>>();

    for (ref_id, expected_ref) in &expected_refs {
        let Some(actual_ref) = actual_refs.get(ref_id) else {
            diffs.push(ProjectLockDiff {
                kind: ProjectLockDiffKind::MissingResolvedRef,
                subject: (*ref_id).to_string(),
                field: "ref_id".to_string(),
                expected: (*ref_id).to_string(),
                actual: "[missing]".to_string(),
                message: format!(
                    "Resolved ref {ref_id} is missing; restore it or run an explicit lock refresh"
                ),
            });
            continue;
        };

        if expected_ref.kind != actual_ref.kind {
            diffs.push(ProjectLockDiff {
                kind: ProjectLockDiffKind::ResolvedDigestDrift,
                subject: (*ref_id).to_string(),
                field: "kind".to_string(),
                expected: format!("{:?}", expected_ref.kind),
                actual: format!("{:?}", actual_ref.kind),
                message: format!(
                    "Resolved ref {ref_id} changed kind; review the dependency class before refreshing the lock"
                ),
            });
        }

        if expected_ref.resolved_digest != actual_ref.resolved_digest {
            let kind = if expected_ref.kind == ProjectLockRefKind::ToolContract {
                ProjectLockDiffKind::ToolContractDrift
            } else {
                ProjectLockDiffKind::ResolvedDigestDrift
            };
            let message = if expected_ref.kind == ProjectLockRefKind::ToolContract {
                format!(
                    "Tool contract drifted for {ref_id}; review the executable contract and run an explicit lock refresh"
                )
            } else {
                format!(
                    "Resolved digest drifted for {ref_id}; review the pinned dependency and run an explicit lock refresh"
                )
            };
            diffs.push(ProjectLockDiff {
                kind,
                subject: (*ref_id).to_string(),
                field: "resolved_digest".to_string(),
                expected: expected_ref.resolved_digest.clone(),
                actual: actual_ref.resolved_digest.clone(),
                message,
            });
        }
    }

    for ref_id in actual_refs.keys() {
        if !expected_refs.contains_key(ref_id) {
            diffs.push(ProjectLockDiff {
                kind: ProjectLockDiffKind::UnexpectedResolvedRef,
                subject: (*ref_id).to_string(),
                field: "ref_id".to_string(),
                expected: "[absent]".to_string(),
                actual: (*ref_id).to_string(),
                message: format!(
                    "Unexpected resolved ref {ref_id} appeared in the projection; review it before refreshing the lock"
                ),
            });
        }
    }

    diffs.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.subject.cmp(&right.subject))
            .then_with(|| left.field.cmp(&right.field))
    });
    diffs
}

fn normalized_relative_path(value: &str, field: &str) -> ProjectLockResult<String> {
    let value = sanitized_text(value, field)?;
    let path = Path::new(&value);
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => normalized.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(path_policy_error(format!(
                    "{field} must stay relative and cannot escape the project root: {value}"
                )));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(path_policy_error(format!(
            "{field} must contain at least one relative segment"
        )));
    }
    Ok(normalized.to_string_lossy().replace('\\', "/"))
}

fn sanitized_text(value: &str, field: &str) -> ProjectLockResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must be non-empty"
        )));
    }
    if contains_secret_marker(trimmed) {
        return Err(secret_policy_error(format!(
            "{field} must not embed secret-like handles in hashed lock content"
        )));
    }
    if looks_like_timestamp(trimmed) {
        return Err(artifact_contract_error(format!(
            "{field} must not embed timestamp-like values in hashed lock content"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalized_digest(value: &str, field: &str) -> ProjectLockResult<String> {
    let value = sanitized_text(value, field)?;
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(artifact_contract_error(format!(
            "{field} must start with blake3:"
        )));
    };
    if hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must be a blake3 digest with 64 hex characters"
    )))
}

fn contains_secret_marker(value: &str) -> bool {
    SECRET_PREFIXES.iter().any(|prefix| value.contains(prefix))
}

fn looks_like_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 20 {
        return false;
    }
    bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && matches!(bytes.get(10), Some(b'T') | Some(b' '))
        && bytes.get(13) == Some(&b':')
        && bytes.get(16) == Some(&b':')
        && matches!(bytes.last(), Some(b'Z'))
}

fn artifact_contract_error(message: impl Into<String>) -> ProjectLockError {
    ProjectLockError::new(ProjectLockErrorCode::ArtifactContract, message)
}

fn path_policy_error(message: impl Into<String>) -> ProjectLockError {
    ProjectLockError::new(ProjectLockErrorCode::PathPolicy, message)
}

fn secret_policy_error(message: impl Into<String>) -> ProjectLockError {
    ProjectLockError::new(ProjectLockErrorCode::SecretPolicy, message)
}
