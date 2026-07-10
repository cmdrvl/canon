#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt, fs, io,
    path::{Component, Path, PathBuf},
};

pub type FsSafetyResult<T> = Result<T, FsSafetyError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsSafetyErrorCode {
    ArtifactContract,
    PathTraversal,
    WorkspaceEscape,
    SymlinkEscape,
    HardLinkAlias,
    InputOutputOverlap,
    UndeclaredMutation,
    OutputRootPolicy,
    ReadOnlyViolation,
    AtomicPublishConflict,
    PermissionDenied,
    QuotaExceeded,
    Io,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsSafetyError {
    pub code: FsSafetyErrorCode,
    pub logical_field: String,
    pub message: String,
    pub remediation: String,
}

impl FsSafetyError {
    pub fn new(
        code: FsSafetyErrorCode,
        logical_field: impl Into<String>,
        message: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            code,
            logical_field: logical_field.into(),
            message: message.into(),
            remediation: remediation.into(),
        }
    }
}

impl fmt::Display for FsSafetyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} [{}]: {}",
            self.code, self.logical_field, self.message
        )
    }
}

impl Error for FsSafetyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedAccess {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathResolution {
    pub logical_field: String,
    pub relative_path: PathBuf,
    pub absolute_path: PathBuf,
    pub canonical_path: PathBuf,
    pub parent_canonical_path: PathBuf,
    pub exists: bool,
    pub leaf_is_symlink: bool,
    pub file_identity: Option<FileIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicPublicationPlan {
    pub logical_field: String,
    pub destination: PathBuf,
    pub temp_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIdentity {
    #[cfg(unix)]
    pub device_id: u64,
    #[cfg(unix)]
    pub inode: u64,
    #[cfg(unix)]
    pub hard_link_count: u64,
}

pub fn normalize_relative_path(logical_field: &str, path: &Path) -> FsSafetyResult<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(artifact_contract_error(
            logical_field,
            "path must not be empty",
            "Declare a non-empty workspace-relative path.",
        ));
    }
    if path.is_absolute() {
        return Err(path_traversal_error(
            logical_field,
            "absolute paths are not allowed",
            "Use a workspace-relative path inside the declared workspace.",
        ));
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => normalized.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(path_traversal_error(
                    logical_field,
                    "path traversal segments are not allowed",
                    "Remove '..', drive prefixes, or rooted segments from the declared path.",
                ));
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err(artifact_contract_error(
            logical_field,
            "path normalized to empty",
            "Declare a concrete file or directory path inside the workspace.",
        ));
    }

    Ok(normalized)
}

pub fn canonical_workspace_root(workspace_root: &Path) -> FsSafetyResult<PathBuf> {
    fs::canonicalize(workspace_root).map_err(|error| {
        diagnose_io_error(
            "workspace_root",
            &error,
            "Ensure the workspace root exists and is readable before planning work.",
        )
    })
}

pub fn resolve_workspace_path(
    workspace_root: &Path,
    logical_field: &str,
    relative_path: &Path,
    access: PlannedAccess,
) -> FsSafetyResult<PathResolution> {
    let workspace_canonical = canonical_workspace_root(workspace_root)?;
    let normalized = normalize_relative_path(logical_field, relative_path)?;
    let absolute = workspace_root.join(&normalized);

    let (existing_ancestor, missing_tail) = nearest_existing_ancestor(workspace_root, &absolute)
        .map_err(|error| {
            diagnose_io_error(
                logical_field,
                &error,
                "Ensure the declared path lives under an existing readable workspace directory.",
            )
        })?;
    let ancestor_canonical = fs::canonicalize(&existing_ancestor).map_err(|error| {
        diagnose_io_error(
            logical_field,
            &error,
            "Ensure the declared path resolves under the workspace without unreadable ancestors.",
        )
    })?;
    if !ancestor_canonical.starts_with(&workspace_canonical) {
        return Err(workspace_escape_error(
            logical_field,
            "path resolves outside the workspace root",
            "Choose a path under the declared workspace root or remove the escaping link.",
        ));
    }

    let target_exists = absolute.exists();
    let mut leaf_is_symlink = false;
    let canonical_target = if target_exists {
        let metadata = fs::symlink_metadata(&absolute).map_err(|error| {
            diagnose_io_error(
                logical_field,
                &error,
                "Ensure the declared path is readable before planning work.",
            )
        })?;
        leaf_is_symlink = metadata.file_type().is_symlink();
        let resolved = fs::canonicalize(&absolute).map_err(|error| {
            diagnose_io_error(
                logical_field,
                &error,
                "Ensure the declared path resolves cleanly under the workspace.",
            )
        })?;
        if !resolved.starts_with(&workspace_canonical) {
            return Err(symlink_escape_error(
                logical_field,
                "path resolves outside the workspace via a symlink",
                "Replace the symlink with a workspace-owned path or copy the needed file inside the workspace.",
            ));
        }
        resolved
    } else {
        missing_tail
            .iter()
            .fold(ancestor_canonical.clone(), |path, segment| {
                path.join(segment)
            })
    };

    if matches!(access, PlannedAccess::Write) && leaf_is_symlink {
        return Err(symlink_escape_error(
            logical_field,
            "mutating through a symlink leaf is not allowed",
            "Write to a regular workspace-owned path instead of mutating a symlink target.",
        ));
    }

    let parent_canonical_path = canonical_target
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| workspace_canonical.clone());

    Ok(PathResolution {
        logical_field: logical_field.to_string(),
        relative_path: normalized,
        absolute_path: absolute.clone(),
        canonical_path: canonical_target,
        parent_canonical_path,
        exists: target_exists,
        leaf_is_symlink,
        file_identity: file_identity(&absolute),
    })
}

pub fn ensure_within_owned_root(
    _workspace_root: &Path,
    resolution: &PathResolution,
    owned_roots: &[PathBuf],
) -> FsSafetyResult<()> {
    for owned_root in owned_roots {
        let normalized_root = normalize_relative_path("owned_output_root", owned_root)?;
        if resolution.relative_path.starts_with(&normalized_root) {
            return Ok(());
        }
    }

    Err(output_root_error(
        &resolution.logical_field,
        "output path is outside the declared owned output roots",
        "Choose an output path under one of the owned output roots declared for this workspace.",
    ))
}

pub fn ensure_paths_do_not_overlap(
    left: &PathResolution,
    right: &PathResolution,
    left_role: &str,
    right_role: &str,
) -> FsSafetyResult<()> {
    let lexical_overlap = left.absolute_path == right.absolute_path
        || left.absolute_path.starts_with(&right.absolute_path)
        || right.absolute_path.starts_with(&left.absolute_path);
    let canonical_overlap = left.canonical_path == right.canonical_path
        || left.canonical_path.starts_with(&right.canonical_path)
        || right.canonical_path.starts_with(&left.canonical_path);

    if lexical_overlap || canonical_overlap {
        return Err(overlap_error(
            &right.logical_field,
            format!(
                "{right_role} overlaps {left_role} declared at logical field {}",
                left.logical_field
            ),
            "Choose distinct input, output, and temp paths so reads never alias declared mutations.",
        ));
    }

    Ok(())
}

pub fn ensure_not_hard_link_alias(
    input: &PathResolution,
    output: &PathResolution,
) -> FsSafetyResult<()> {
    match (&input.file_identity, &output.file_identity) {
        (Some(left), Some(right))
            if left == right && input.absolute_path != output.absolute_path =>
        {
            Err(hard_link_error(
                &output.logical_field,
                format!(
                    "output aliases input {} through a hard link",
                    input.logical_field
                ),
                "Write to a fresh output path instead of reusing a hard-linked inode.",
            ))
        }
        _ => Ok(()),
    }
}

pub fn ensure_declared_mutation(
    workspace_root: &Path,
    logical_field: &str,
    actual_path: &Path,
    declared_mutations: &[PathBuf],
) -> FsSafetyResult<()> {
    let absolute = if actual_path.is_absolute() {
        actual_path.to_path_buf()
    } else {
        workspace_root.join(normalize_relative_path(logical_field, actual_path)?)
    };
    if declared_mutations
        .iter()
        .any(|declared| declared == &absolute)
    {
        Ok(())
    } else {
        Err(undeclared_mutation_error(
            logical_field,
            "mutation target was not declared during workspace planning",
            "Add the mutation target to the declared output or temp set before execution.",
        ))
    }
}

pub fn plan_atomic_publication(
    destination: &PathResolution,
    temp_suffix: &str,
) -> FsSafetyResult<AtomicPublicationPlan> {
    let temp_path = atomic_temp_sibling(&destination.absolute_path, temp_suffix);
    if temp_path.exists() {
        return Err(atomic_publish_error(
            &destination.logical_field,
            "atomic publication temp path already exists",
            "Resolve the concurrent output claim or stale temp collision before retrying.",
        ));
    }
    Ok(AtomicPublicationPlan {
        logical_field: destination.logical_field.clone(),
        destination: destination.absolute_path.clone(),
        temp_path,
    })
}

pub fn publish_atomic(plan: &AtomicPublicationPlan, bytes: &[u8]) -> FsSafetyResult<()> {
    if plan.temp_path.exists() {
        return Err(atomic_publish_error(
            &plan.logical_field,
            "atomic publication temp path already exists",
            "Resolve the concurrent output claim or stale temp collision before retrying.",
        ));
    }
    fs::write(&plan.temp_path, bytes).map_err(|error| {
        let _ = fs::remove_file(&plan.temp_path);
        diagnose_io_error(
            &plan.logical_field,
            &error,
            "Check permissions or available space for the destination directory, then retry the publication.",
        )
    })?;
    fs::rename(&plan.temp_path, &plan.destination).map_err(|error| {
        let _ = fs::remove_file(&plan.temp_path);
        diagnose_io_error(
            &plan.logical_field,
            &error,
            "Ensure the destination directory permits atomic rename and is on a writable filesystem.",
        )
    })?;
    Ok(())
}

pub fn atomic_temp_sibling(destination: &Path, temp_suffix: &str) -> PathBuf {
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact");
    destination.with_file_name(format!("{file_name}.{temp_suffix}.tmp"))
}

pub fn diagnose_io_error(
    logical_field: &str,
    error: &io::Error,
    remediation: &str,
) -> FsSafetyError {
    let preview = error.to_string().to_ascii_lowercase();
    if error.kind() == io::ErrorKind::PermissionDenied || preview.contains("permission denied") {
        return FsSafetyError::new(
            FsSafetyErrorCode::PermissionDenied,
            logical_field,
            format!(
                "permission denied while preparing or publishing {}",
                logical_field
            ),
            remediation,
        );
    }
    if preview.contains("quota exceeded")
        || preview.contains("no space left on device")
        || preview.contains("storage full")
    {
        return FsSafetyError::new(
            FsSafetyErrorCode::QuotaExceeded,
            logical_field,
            format!(
                "quota or disk exhaustion prevented writing {}",
                logical_field
            ),
            remediation,
        );
    }
    FsSafetyError::new(
        FsSafetyErrorCode::Io,
        logical_field,
        format!("filesystem operation failed for {}", logical_field),
        remediation,
    )
}

fn nearest_existing_ancestor(
    workspace_root: &Path,
    absolute_path: &Path,
) -> io::Result<(PathBuf, PathBuf)> {
    let mut ancestor = absolute_path.to_path_buf();
    while !ancestor.exists() {
        let Some(parent) = ancestor.parent() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "path has no existing ancestor",
            ));
        };
        ancestor = parent.to_path_buf();
    }

    let missing_tail = absolute_path
        .strip_prefix(&ancestor)
        .unwrap_or_else(|_| Path::new(""))
        .to_path_buf();

    if !ancestor.starts_with(workspace_root) && ancestor != workspace_root {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "path escaped workspace while searching for existing ancestor",
        ));
    }

    Ok((ancestor, missing_tail))
}

#[cfg(unix)]
fn file_identity(path: &Path) -> Option<FileIdentity> {
    use std::os::unix::fs::MetadataExt;

    fs::metadata(path).ok().map(|metadata| FileIdentity {
        device_id: metadata.dev(),
        inode: metadata.ino(),
        hard_link_count: metadata.nlink(),
    })
}

#[cfg(not(unix))]
fn file_identity(_path: &Path) -> Option<FileIdentity> {
    None
}

fn artifact_contract_error(
    logical_field: &str,
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> FsSafetyError {
    FsSafetyError::new(
        FsSafetyErrorCode::ArtifactContract,
        logical_field,
        message,
        remediation,
    )
}

fn path_traversal_error(
    logical_field: &str,
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> FsSafetyError {
    FsSafetyError::new(
        FsSafetyErrorCode::PathTraversal,
        logical_field,
        message,
        remediation,
    )
}

fn workspace_escape_error(
    logical_field: &str,
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> FsSafetyError {
    FsSafetyError::new(
        FsSafetyErrorCode::WorkspaceEscape,
        logical_field,
        message,
        remediation,
    )
}

fn symlink_escape_error(
    logical_field: &str,
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> FsSafetyError {
    FsSafetyError::new(
        FsSafetyErrorCode::SymlinkEscape,
        logical_field,
        message,
        remediation,
    )
}

fn hard_link_error(
    logical_field: &str,
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> FsSafetyError {
    FsSafetyError::new(
        FsSafetyErrorCode::HardLinkAlias,
        logical_field,
        message,
        remediation,
    )
}

fn overlap_error(
    logical_field: &str,
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> FsSafetyError {
    FsSafetyError::new(
        FsSafetyErrorCode::InputOutputOverlap,
        logical_field,
        message,
        remediation,
    )
}

fn undeclared_mutation_error(
    logical_field: &str,
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> FsSafetyError {
    FsSafetyError::new(
        FsSafetyErrorCode::UndeclaredMutation,
        logical_field,
        message,
        remediation,
    )
}

fn output_root_error(
    logical_field: &str,
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> FsSafetyError {
    FsSafetyError::new(
        FsSafetyErrorCode::OutputRootPolicy,
        logical_field,
        message,
        remediation,
    )
}

fn atomic_publish_error(
    logical_field: &str,
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> FsSafetyError {
    FsSafetyError::new(
        FsSafetyErrorCode::AtomicPublishConflict,
        logical_field,
        message,
        remediation,
    )
}
