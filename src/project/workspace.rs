#![forbid(unsafe_code)]

use crate::fs_safety::{
    AtomicPublicationPlan, FsSafetyError, FsSafetyErrorCode, FsSafetyResult, PathResolution,
    PlannedAccess, ensure_declared_mutation, ensure_not_hard_link_alias,
    ensure_paths_do_not_overlap, ensure_within_owned_root, normalize_relative_path,
    plan_atomic_publication, resolve_workspace_path,
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub use crate::fs_safety::publish_atomic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePolicy {
    pub workspace_root: PathBuf,
    pub owned_output_roots: Vec<PathBuf>,
    pub temp_root: PathBuf,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInput {
    pub logical_field: String,
    pub relative_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceOutput {
    pub logical_field: String,
    pub relative_path: PathBuf,
    pub atomic_publish: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePlan {
    pub workspace_root: PathBuf,
    pub owned_output_roots: Vec<PathBuf>,
    pub temp_root: PathBuf,
    pub inputs: Vec<PathResolution>,
    pub outputs: Vec<PathResolution>,
    pub declared_mutations: Vec<PathBuf>,
    pub atomic_publications: Vec<AtomicPublicationPlan>,
}

pub fn plan_workspace(
    policy: &WorkspacePolicy,
    inputs: &[WorkspaceInput],
    outputs: &[WorkspaceOutput],
) -> FsSafetyResult<WorkspacePlan> {
    let policy = finalize_policy(policy)?;

    if policy.read_only && !outputs.is_empty() {
        return Err(FsSafetyError::new(
            FsSafetyErrorCode::ReadOnlyViolation,
            "workspace_policy.read_only",
            "read-only workspaces must not declare mutating outputs",
            "Drop output declarations or rerun with an explicitly mutating workspace policy.",
        ));
    }

    let temp_root = normalize_relative_path("workspace_policy.temp_root", &policy.temp_root)?;
    let temp_root_absolute = policy.workspace_root.join(&temp_root);
    if !policy
        .owned_output_roots
        .iter()
        .any(|root| temp_root_absolute.starts_with(policy.workspace_root.join(root)))
    {
        return Err(FsSafetyError::new(
            FsSafetyErrorCode::OutputRootPolicy,
            "workspace_policy.temp_root",
            "temp_root must live under a declared owned output root",
            "Choose a temp root inside one of the owned output roots for this workspace.",
        ));
    }

    let resolved_inputs = inputs
        .iter()
        .map(|input| {
            resolve_workspace_path(
                &policy.workspace_root,
                &input.logical_field,
                &input.relative_path,
                PlannedAccess::Read,
            )
        })
        .collect::<FsSafetyResult<Vec<_>>>()?;

    let resolved_outputs = outputs
        .iter()
        .map(|output| {
            let resolution = resolve_workspace_path(
                &policy.workspace_root,
                &output.logical_field,
                &output.relative_path,
                PlannedAccess::Write,
            )?;
            ensure_within_owned_root(
                &policy.workspace_root,
                &resolution,
                &policy.owned_output_roots,
            )?;
            Ok(resolution)
        })
        .collect::<FsSafetyResult<Vec<_>>>()?;

    for input in &resolved_inputs {
        if temp_root_absolute.starts_with(&input.absolute_path)
            || input.absolute_path.starts_with(&temp_root_absolute)
            || input.canonical_path.starts_with(&temp_root_absolute)
        {
            return Err(FsSafetyError::new(
                FsSafetyErrorCode::InputOutputOverlap,
                "workspace_policy.temp_root",
                format!(
                    "temp_root overlaps input declared at logical field {}",
                    input.logical_field
                ),
                "Move the temp root away from input paths so scratch writes cannot alias reads.",
            ));
        }
    }

    let mut declared_mutations = BTreeSet::new();
    let mut atomic_publications = Vec::new();
    for output in outputs {
        let resolution = resolved_outputs
            .iter()
            .find(|candidate| candidate.logical_field == output.logical_field)
            .expect("output resolution available");
        declared_mutations.insert(resolution.absolute_path.clone());
        if output.atomic_publish {
            let publication = plan_atomic_publication(resolution, "canon-workspace")?;
            declared_mutations.insert(publication.temp_path.clone());
            atomic_publications.push(publication);
        }
    }

    let publication_resolutions = atomic_publications
        .iter()
        .map(|publication| {
            resolve_workspace_path(
                &policy.workspace_root,
                &publication.logical_field,
                &relative_from_workspace(&policy.workspace_root, &publication.temp_path),
                PlannedAccess::Write,
            )
        })
        .collect::<FsSafetyResult<Vec<_>>>()?;

    for input in &resolved_inputs {
        for output in &resolved_outputs {
            ensure_paths_do_not_overlap(input, output, "input", "output")?;
            ensure_not_hard_link_alias(input, output)?;
        }
        for publication in &publication_resolutions {
            ensure_paths_do_not_overlap(input, publication, "input", "temp")?;
        }
    }

    for left_index in 0..resolved_outputs.len() {
        for right_index in (left_index + 1)..resolved_outputs.len() {
            ensure_paths_do_not_overlap(
                &resolved_outputs[left_index],
                &resolved_outputs[right_index],
                "output",
                "output",
            )?;
        }
    }

    for output in &resolved_outputs {
        for publication in &publication_resolutions {
            if output.logical_field != publication.logical_field {
                ensure_paths_do_not_overlap(output, publication, "output", "temp")?;
            }
        }
    }

    Ok(WorkspacePlan {
        workspace_root: policy.workspace_root.clone(),
        owned_output_roots: policy.owned_output_roots.clone(),
        temp_root,
        inputs: resolved_inputs,
        outputs: resolved_outputs,
        declared_mutations: declared_mutations.into_iter().collect(),
        atomic_publications,
    })
}

pub fn validate_declared_mutation_target(
    plan: &WorkspacePlan,
    logical_field: &str,
    actual_path: &Path,
) -> FsSafetyResult<()> {
    ensure_declared_mutation(
        &plan.workspace_root,
        logical_field,
        actual_path,
        &plan.declared_mutations,
    )
}

pub fn allocate_temp_path(
    plan: &WorkspacePlan,
    logical_field: &str,
    label: &str,
) -> FsSafetyResult<PathBuf> {
    let label = normalize_relative_path(logical_field, Path::new(label))?;
    let temp_path = plan.workspace_root.join(&plan.temp_root).join(label);
    for input in &plan.inputs {
        if temp_path == input.absolute_path
            || temp_path.starts_with(&input.absolute_path)
            || input.absolute_path.starts_with(&temp_path)
        {
            return Err(FsSafetyError::new(
                FsSafetyErrorCode::InputOutputOverlap,
                logical_field,
                format!(
                    "temp allocation overlaps input declared at logical field {}",
                    input.logical_field
                ),
                "Choose a temp label under the declared temp root that does not alias any input path.",
            ));
        }
    }
    for output in &plan.outputs {
        if temp_path == output.absolute_path {
            return Err(FsSafetyError::new(
                FsSafetyErrorCode::AtomicPublishConflict,
                logical_field,
                "temp allocation collides with a declared output path",
                "Choose a distinct temp label so scratch output cannot shadow a published artifact.",
            ));
        }
    }
    Ok(temp_path)
}

fn finalize_policy(policy: &WorkspacePolicy) -> FsSafetyResult<WorkspacePolicy> {
    if policy.owned_output_roots.is_empty() {
        return Err(FsSafetyError::new(
            FsSafetyErrorCode::ArtifactContract,
            "workspace_policy.owned_output_roots",
            "at least one owned output root is required",
            "Declare the workspace-owned directories that may be mutated.",
        ));
    }

    let owned_output_roots = policy
        .owned_output_roots
        .iter()
        .map(|root| normalize_relative_path("workspace_policy.owned_output_roots", root))
        .collect::<FsSafetyResult<Vec<_>>>()?;

    Ok(WorkspacePolicy {
        workspace_root: policy.workspace_root.clone(),
        owned_output_roots,
        temp_root: normalize_relative_path("workspace_policy.temp_root", &policy.temp_root)?,
        read_only: policy.read_only,
    })
}

fn relative_from_workspace(workspace_root: &Path, absolute_path: &Path) -> PathBuf {
    absolute_path
        .strip_prefix(workspace_root)
        .unwrap_or(absolute_path)
        .to_path_buf()
}
