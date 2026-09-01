#![forbid(unsafe_code)]

use super::{
    plan::{
        ProjectExtensionNodePolicy, ProjectPlan, ProjectPlanCacheDecision, ProjectPlanError,
        ProjectPlanHashRef, ProjectPlanNode, ProjectPlanNodeClass,
        ProjectPlanOutputMaterialization, ProjectPlanSideEffectKind, project_plan_node_cache_key,
        validate_extension_node_effects,
    },
    receipt::{
        CANON_PROJECT_RUN_VERSION, ProjectReceiptError, ProjectRunHashRef, ProjectRunNextAction,
        ProjectRunNodeOutcome, ProjectRunNodeReceipt, ProjectRunOutputReceipt, ProjectRunReceipt,
        canonical_node_receipt_bytes, canonical_run_receipt_bytes, converge_node_receipt_in,
        digest_bytes, finalized_node_receipt, finalized_run_receipt, node_receipt_cas_path,
        preserve_node_receipt_cas_in, read_node_receipt, semantic_node_receipt_path,
        semantic_node_result_cache_key,
    },
};
use crate::fs_safety::{PlannedAccess, resolve_workspace_path as resolve_fs_workspace_path};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

pub type ProjectRunResult<T> = Result<T, ProjectRunError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRunErrorCode {
    ArtifactContract,
    WorkspacePolicy,
    ReceiptPoisoning,
    StaleArtifact,
    ExecutionFailed,
    AtomicPublication,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRunError {
    pub code: ProjectRunErrorCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_command: Option<String>,
}

impl ProjectRunError {
    pub fn new(
        code: ProjectRunErrorCode,
        node_id: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            node_id,
            message: message.into(),
            next_command: None,
        }
    }
}

impl fmt::Display for ProjectRunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for ProjectRunError {}

impl From<ProjectReceiptError> for ProjectRunError {
    fn from(error: ProjectReceiptError) -> Self {
        ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            None,
            format!("project run receipt validation failed: {}", error.message),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRunFailurePolicy {
    FailFast,
    CollectIndependentFailures,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRunPolicy {
    pub workspace_root: PathBuf,
    pub work_dir: PathBuf,
    pub max_parallelism: usize,
    pub failure_policy: ProjectRunFailurePolicy,
    pub selected_nodes: BTreeSet<String>,
    pub allow_network: bool,
    pub allow_mutation_gates: bool,
    pub cancel_before_nodes: BTreeSet<String>,
}

impl ProjectRunPolicy {
    pub fn new(workspace_root: impl Into<PathBuf>, work_dir: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            work_dir: work_dir.into(),
            max_parallelism: 1,
            failure_policy: ProjectRunFailurePolicy::FailFast,
            selected_nodes: BTreeSet::new(),
            allow_network: false,
            allow_mutation_gates: false,
            cancel_before_nodes: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectNodeExecutionContext {
    pub node_id: String,
    pub dependency_semantic_hashes: BTreeMap<String, String>,
    /// Content-validated outputs from completed direct dependencies.
    ///
    /// These bytes are operational execution context, not semantic identity.
    /// Their digests are already represented by the dependency receipts and
    /// are rechecked immediately before the executor is invoked. Supplying
    /// them here lets a fresh executor resume after earlier nodes were reused
    /// without trusting ambient files or rebuilding a domain-specific cache.
    pub dependency_outputs: BTreeMap<String, Vec<ProjectDependencyOutput>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDependencyOutput {
    pub output_id: String,
    pub content_digest: String,
    pub byte_count: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectNodeExecutionResult {
    pub outputs: BTreeMap<String, Vec<u8>>,
    pub deterministic_usage: BTreeMap<String, u64>,
    pub duration_millis: u64,
    pub resource_observations: BTreeMap<String, u64>,
}

impl ProjectNodeExecutionResult {
    pub fn with_outputs(outputs: BTreeMap<String, Vec<u8>>) -> Self {
        Self {
            outputs,
            deterministic_usage: BTreeMap::new(),
            duration_millis: 0,
            resource_observations: BTreeMap::new(),
        }
    }
}

pub trait ProjectNodeExecutor {
    fn execute(
        &mut self,
        node: &ProjectPlanNode,
        context: &ProjectNodeExecutionContext,
    ) -> ProjectRunResult<ProjectNodeExecutionResult>;
}

pub const PROJECT_INTERNAL_COPY_FILE_EXECUTOR: &str = "copy-file-v1";
pub const CANON_PROJECT_RUN_MANIFEST_REVISION_VERSION: &str =
    "canon.project.run.manifest_revision.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRunNodeReport {
    pub node_id: String,
    pub outcome: ProjectRunNodeOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRunReport {
    pub schema_version: String,
    pub project_id: String,
    pub plan_graph_hash: String,
    pub run_receipt_hash: String,
    pub max_parallelism: usize,
    pub max_ready_width: usize,
    #[serde(default)]
    pub executed_nodes: Vec<String>,
    #[serde(default)]
    pub resumed_nodes: Vec<String>,
    #[serde(default)]
    pub failed_nodes: Vec<String>,
    #[serde(default)]
    pub cancelled_nodes: Vec<String>,
    #[serde(default)]
    pub invalidated_nodes: Vec<String>,
    #[serde(default)]
    pub blocked_nodes: Vec<String>,
    #[serde(default)]
    pub next_actions: BTreeMap<String, String>,
    pub receipt: ProjectRunReceipt,
    #[serde(default)]
    pub node_reports: Vec<ProjectRunNodeReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRunManifestRevision {
    pub schema_version: String,
    pub project_id: String,
    pub plan_graph_hash: String,
    pub run_receipt_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_revision_hash: Option<String>,
    #[serde(default)]
    pub validated_nodes: Vec<String>,
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
    pub node_receipt_hashes: BTreeMap<String, String>,
    #[serde(default)]
    pub node_semantic_hashes: BTreeMap<String, String>,
    #[serde(default)]
    pub deterministic_usage: BTreeMap<String, u64>,
    pub revision_hash: String,
}

pub fn run_project_plan<E: ProjectNodeExecutor>(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
    executor: &mut E,
) -> ProjectRunResult<ProjectRunReport> {
    validate_plan_shape(plan)?;
    let policy = finalize_policy(policy)?;
    let _manifest_head = read_project_run_manifest_head_for_plan(plan, &policy)?;
    let plan_nodes = plan_node_ids(plan);
    let target_nodes = selected_node_closure(plan, &policy.selected_nodes)?;
    let mut existing = validate_existing_receipts(plan, &policy, &plan_nodes)?;
    if !existing.poisoned_receipts.is_empty() {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            None,
            format!(
                "poisoned project receipts: {}",
                existing.poisoned_receipts.join(", ")
            ),
        ));
    }
    restore_existing_receipts(plan, &policy, &target_nodes, &mut existing)?;

    let mut valid_receipts = existing.valid_receipts;
    let completed_receipts = existing.completed_receipts;
    let prior_receipts = existing.prior_receipts;
    let mut invalidated_nodes = existing.invalidated_nodes;
    invalidated_nodes.extend(descendants(plan, &invalidated_nodes));
    for invalidated in &invalidated_nodes {
        valid_receipts.remove(invalidated);
    }

    let mut report = ProjectRunReport {
        schema_version: CANON_PROJECT_RUN_VERSION.to_string(),
        project_id: plan.project_id.clone(),
        plan_graph_hash: plan.graph_hash.clone(),
        run_receipt_hash: String::new(),
        max_parallelism: policy.max_parallelism,
        max_ready_width: 0,
        executed_nodes: Vec::new(),
        resumed_nodes: valid_receipts
            .keys()
            .filter(|node_id| target_nodes.contains(*node_id))
            .cloned()
            .collect(),
        failed_nodes: Vec::new(),
        cancelled_nodes: Vec::new(),
        invalidated_nodes: invalidated_nodes
            .iter()
            .filter(|node_id| target_nodes.contains(*node_id))
            .cloned()
            .collect(),
        blocked_nodes: Vec::new(),
        next_actions: BTreeMap::new(),
        receipt: empty_run_receipt(plan),
        node_reports: Vec::new(),
    };

    let mut failed_nodes = BTreeSet::new();
    let mut cancelled = false;
    loop {
        let ready = ready_nodes(
            plan,
            &target_nodes,
            &valid_receipts,
            &failed_nodes,
            &invalidated_nodes,
            &policy,
        );
        report.max_ready_width = report.max_ready_width.max(ready.len());
        if ready.is_empty() {
            break;
        }
        for node in ready.into_iter().take(policy.max_parallelism) {
            if policy.cancel_before_nodes.contains(&node.node_id) {
                let receipt = terminal_receipt(
                    plan,
                    node,
                    &valid_receipts,
                    ProjectRunNodeOutcome::Cancelled,
                    ProjectRunNextAction::Resume,
                    "E_PROJECT_CANCELLED",
                    "project run cancelled before node execution",
                )?;
                let receipt = write_receipt(&policy, &receipt, prior_receipts.get(&node.node_id))?;
                report.cancelled_nodes.push(node.node_id.clone());
                report.node_reports.push(node_report(
                    node,
                    ProjectRunNodeOutcome::Cancelled,
                    Some(&receipt.receipt_hash),
                    Some("cancelled before execution"),
                ));
                cancelled = true;
                break;
            }

            match execute_node(
                plan,
                node,
                &policy,
                &valid_receipts,
                &completed_receipts,
                &prior_receipts,
                executor,
            ) {
                Ok(receipt) => {
                    report.executed_nodes.push(node.node_id.clone());
                    report.node_reports.push(node_report(
                        node,
                        ProjectRunNodeOutcome::Completed,
                        Some(&receipt.receipt_hash),
                        None,
                    ));
                    valid_receipts.insert(node.node_id.clone(), receipt);
                }
                Err(error) => {
                    if error.code == ProjectRunErrorCode::ReceiptPoisoning {
                        return Err(error);
                    }
                    let receipt = terminal_receipt(
                        plan,
                        node,
                        &valid_receipts,
                        ProjectRunNodeOutcome::Failed,
                        ProjectRunNextAction::InspectFailure,
                        &format!("{:?}", error.code),
                        &error.message,
                    )?;
                    let receipt =
                        write_receipt(&policy, &receipt, prior_receipts.get(&node.node_id))?;
                    failed_nodes.insert(node.node_id.clone());
                    report.failed_nodes.push(node.node_id.clone());
                    report.node_reports.push(node_report(
                        node,
                        ProjectRunNodeOutcome::Failed,
                        Some(&receipt.receipt_hash),
                        Some(&error.message),
                    ));
                    if policy.failure_policy == ProjectRunFailurePolicy::FailFast {
                        return finish_and_publish_report(
                            plan,
                            &policy,
                            &target_nodes,
                            report,
                            valid_receipts,
                            failed_nodes,
                        );
                    }
                }
            }
        }
        if cancelled {
            break;
        }
    }

    finish_and_publish_report(
        plan,
        &policy,
        &target_nodes,
        report,
        valid_receipts,
        failed_nodes,
    )
}

pub fn run_project_plan_with_registered_executors(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
) -> ProjectRunResult<ProjectRunReport> {
    let mut executor = ProjectRegisteredNodeExecutor::new(&policy.workspace_root);
    ensure_pending_nodes_have_registered_executors(plan, policy, &executor)?;
    run_project_plan(plan, policy, &mut executor)
}

pub fn inspect_project_run_reuse_only(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
) -> ProjectRunResult<ProjectRunReport> {
    validate_plan_shape(plan)?;
    let policy = finalize_policy(policy)?;
    let _manifest_head = read_project_run_manifest_head_for_plan(plan, &policy)?;
    let plan_nodes = plan_node_ids(plan);
    let target_nodes = selected_node_closure(plan, &policy.selected_nodes)?;
    let existing = validate_existing_receipts(plan, &policy, &plan_nodes)?;
    if !existing.poisoned_receipts.is_empty() {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            None,
            format!(
                "poisoned project receipts: {}",
                existing.poisoned_receipts.join(", ")
            ),
        ));
    }

    let mut valid_receipts = existing.valid_receipts;
    let mut invalidated_nodes = existing.invalidated_nodes;
    invalidated_nodes.extend(descendants(plan, &invalidated_nodes));
    for invalidated in &invalidated_nodes {
        valid_receipts.remove(invalidated);
    }
    let reusable_node_reports = plan
        .nodes
        .iter()
        .filter(|node| target_nodes.contains(&node.node_id))
        .filter_map(|node| {
            valid_receipts.get(&node.node_id).map(|receipt| {
                node_report(
                    node,
                    ProjectRunNodeOutcome::Completed,
                    Some(&receipt.receipt_hash),
                    Some("reusable completed receipt validated; read-only inspection did not restore outputs"),
                )
            })
        })
        .collect();

    let report = ProjectRunReport {
        schema_version: CANON_PROJECT_RUN_VERSION.to_string(),
        project_id: plan.project_id.clone(),
        plan_graph_hash: plan.graph_hash.clone(),
        run_receipt_hash: String::new(),
        max_parallelism: policy.max_parallelism,
        max_ready_width: 0,
        executed_nodes: Vec::new(),
        resumed_nodes: Vec::new(),
        failed_nodes: Vec::new(),
        cancelled_nodes: Vec::new(),
        invalidated_nodes: invalidated_nodes
            .iter()
            .filter(|node_id| target_nodes.contains(*node_id))
            .cloned()
            .collect(),
        blocked_nodes: Vec::new(),
        next_actions: BTreeMap::new(),
        receipt: empty_run_receipt(plan),
        node_reports: reusable_node_reports,
    };

    let completed = valid_receipts.keys().cloned().collect::<BTreeSet<_>>();
    let mut unsupported = Vec::new();
    for node in &plan.nodes {
        if !target_nodes.contains(&node.node_id) || completed.contains(&node.node_id) {
            continue;
        }
        ensure_declared_node_effects_allowed(node, &policy)?;
        unsupported.push(node.node_id.clone());
    }
    if !unsupported.is_empty() {
        unsupported.sort();
        return Err(ProjectRunError {
            code: ProjectRunErrorCode::ExecutionFailed,
            node_id: unsupported.first().cloned(),
            message: format!(
                "project run has no registered real executor for pending nodes: {}; this public surface can validate plans and reuse existing completed receipts only",
                unsupported.join(", ")
            ),
            next_command: None,
        });
    }

    finish_report(
        plan,
        &policy,
        &target_nodes,
        report,
        valid_receipts,
        BTreeSet::new(),
    )
}

pub fn canonical_project_run_report_bytes(report: &ProjectRunReport) -> ProjectRunResult<Vec<u8>> {
    serde_json::to_vec(report).map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            None,
            format!("failed to serialize project run report: {error}"),
        )
    })
}

pub fn project_run_manifest_revision_for_report(
    plan: &ProjectPlan,
    report: &ProjectRunReport,
    previous_revision_hash: Option<String>,
) -> ProjectRunResult<ProjectRunManifestRevision> {
    validate_plan_shape(plan)?;
    validate_report_receipt_binding(plan, report)?;
    let mut node_receipt_hashes = BTreeMap::new();
    let mut node_semantic_hashes = BTreeMap::new();
    let mut deterministic_usage = BTreeMap::new();
    for receipt in &report.receipt.node_receipts {
        node_receipt_hashes.insert(receipt.node_id.clone(), receipt.receipt_hash.clone());
        node_semantic_hashes.insert(receipt.node_id.clone(), receipt.semantic_hash.clone());
        for (key, value) in &receipt.deterministic_usage {
            let entry = deterministic_usage.entry(key.clone()).or_insert(0_u64);
            *entry = entry.checked_add(*value).ok_or_else(|| {
                ProjectRunError::new(
                    ProjectRunErrorCode::ArtifactContract,
                    Some(receipt.node_id.clone()),
                    format!("deterministic usage counter {key} overflowed u64"),
                )
            })?;
        }
    }
    for node_report in &report.node_reports {
        let Some(receipt_hash) = &node_report.receipt_hash else {
            continue;
        };
        match node_receipt_hashes.insert(node_report.node_id.clone(), receipt_hash.clone()) {
            Some(existing) if existing != *receipt_hash => {
                return Err(ProjectRunError::new(
                    ProjectRunErrorCode::ArtifactContract,
                    Some(node_report.node_id.clone()),
                    "project run report contains conflicting receipt hashes for the same node",
                ));
            }
            _ => {}
        }
    }
    finalized_project_run_manifest_revision(ProjectRunManifestRevision {
        schema_version: CANON_PROJECT_RUN_MANIFEST_REVISION_VERSION.to_string(),
        project_id: report.project_id.clone(),
        plan_graph_hash: report.plan_graph_hash.clone(),
        run_receipt_hash: report.run_receipt_hash.clone(),
        previous_revision_hash,
        validated_nodes: plan_node_ids(plan).into_iter().collect(),
        completed_nodes: report.receipt.completed_nodes.clone(),
        failed_nodes: report.receipt.failed_nodes.clone(),
        cancelled_nodes: report.receipt.cancelled_nodes.clone(),
        invalidated_nodes: report.receipt.invalidated_nodes.clone(),
        blocked_nodes: report.receipt.blocked_nodes.clone(),
        node_receipt_hashes,
        node_semantic_hashes,
        deterministic_usage,
        revision_hash: String::new(),
    })
}

pub fn canonical_project_run_manifest_revision_bytes(
    revision: &ProjectRunManifestRevision,
) -> ProjectRunResult<Vec<u8>> {
    let canonical = validate_project_run_manifest_revision(revision.clone())?;
    serde_json::to_vec(&canonical).map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            None,
            format!("failed to serialize project run manifest revision: {error}"),
        )
    })
}

pub fn read_project_run_manifest_head(
    policy: &ProjectRunPolicy,
) -> ProjectRunResult<Option<ProjectRunManifestRevision>> {
    let path = project_run_manifest_head_path(policy)?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            None,
            format!(
                "failed to read project run manifest head {}: {error}",
                path.display()
            ),
        )
    })?;
    let revision = parse_project_run_manifest_revision(&bytes)?;
    let (_, immutable_bytes, revision_path) = read_immutable_project_run_manifest_revision(
        policy,
        &revision.revision_hash,
        "project run manifest head revision",
    )?;
    let canonical_bytes = canonical_project_run_manifest_revision_bytes(&revision)?;
    if bytes != canonical_bytes {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            None,
            format!(
                "project run manifest head {} is not canonical",
                path.display()
            ),
        ));
    }
    if immutable_bytes != canonical_bytes {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            None,
            format!(
                "project run manifest head {} does not match immutable revision {}",
                path.display(),
                revision_path.display()
            ),
        ));
    }
    validate_project_run_manifest_history(policy, &revision)?;
    Ok(Some(revision))
}

fn read_project_run_manifest_head_for_plan(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
) -> ProjectRunResult<Option<ProjectRunManifestRevision>> {
    let head = read_project_run_manifest_head(policy)?;
    if let Some(revision) = &head
        && revision.project_id != plan.project_id
    {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            None,
            format!(
                "project run manifest head belongs to project_id={}, expected {}",
                revision.project_id, plan.project_id
            ),
        ));
    }
    Ok(head)
}

fn validate_project_run_manifest_history(
    policy: &ProjectRunPolicy,
    head: &ProjectRunManifestRevision,
) -> ProjectRunResult<()> {
    let mut seen = BTreeSet::new();
    seen.insert(head.revision_hash.clone());
    let mut previous_revision_hash = head.previous_revision_hash.clone();
    while let Some(revision_hash) = previous_revision_hash {
        if !seen.insert(revision_hash.clone()) {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ReceiptPoisoning,
                None,
                format!(
                    "project run manifest revision history contains a cycle at {revision_hash}"
                ),
            ));
        }
        let (previous, _, _) = read_immutable_project_run_manifest_revision(
            policy,
            &revision_hash,
            "previous project run manifest revision",
        )?;
        if previous.project_id != head.project_id {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ReceiptPoisoning,
                None,
                format!(
                    "previous project run manifest revision {revision_hash} belongs to project_id={}, expected {}",
                    previous.project_id, head.project_id
                ),
            ));
        }
        previous_revision_hash = previous.previous_revision_hash;
    }
    Ok(())
}

fn read_immutable_project_run_manifest_revision(
    policy: &ProjectRunPolicy,
    revision_hash: &str,
    context: &str,
) -> ProjectRunResult<(ProjectRunManifestRevision, Vec<u8>, PathBuf)> {
    let revision_path = project_run_manifest_revision_path(policy, revision_hash)?;
    let bytes = fs::read(&revision_path).map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            None,
            format!(
                "{context} {revision_hash} is missing or unreadable at {}: {error}",
                revision_path.display()
            ),
        )
    })?;
    let revision = parse_project_run_manifest_revision(&bytes).map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            error.node_id,
            format!(
                "{context} {revision_hash} at {} is invalid: {}",
                revision_path.display(),
                error.message
            ),
        )
    })?;
    if revision.revision_hash != revision_hash {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            None,
            format!(
                "{context} path {} contains revision_hash {}, expected {revision_hash}",
                revision_path.display(),
                revision.revision_hash
            ),
        ));
    }
    let canonical_bytes = canonical_project_run_manifest_revision_bytes(&revision)?;
    if bytes != canonical_bytes {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            None,
            format!(
                "{context} {revision_hash} at {} is not canonical",
                revision_path.display()
            ),
        ));
    }
    Ok((revision, bytes, revision_path))
}

pub fn project_run_manifest_head_path(policy: &ProjectRunPolicy) -> ProjectRunResult<PathBuf> {
    let policy = finalize_policy(policy)?;
    let relative = run_manifest_head_relative_path(&policy.work_dir)?;
    resolve_fs_workspace_path(
        &policy.workspace_root,
        "project_run.manifest_head",
        &relative,
        PlannedAccess::Write,
    )
    .map(|resolution| resolution.absolute_path)
    .map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::WorkspacePolicy,
            None,
            format!("project run manifest head path failed workspace safety: {error}"),
        )
    })
}

pub fn project_run_manifest_revision_path(
    policy: &ProjectRunPolicy,
    revision_hash: &str,
) -> ProjectRunResult<PathBuf> {
    let policy = finalize_policy(policy)?;
    let relative = run_manifest_revision_relative_path(&policy.work_dir, revision_hash)?;
    resolve_fs_workspace_path(
        &policy.workspace_root,
        "project_run.manifest_revision",
        &relative,
        PlannedAccess::Write,
    )
    .map(|resolution| resolution.absolute_path)
    .map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::WorkspacePolicy,
            None,
            format!("project run manifest revision path failed workspace safety: {error}"),
        )
    })
}

fn finish_and_publish_report(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
    target_nodes: &BTreeSet<String>,
    report: ProjectRunReport,
    valid_receipts: BTreeMap<String, ProjectRunNodeReceipt>,
    failed_nodes: BTreeSet<String>,
) -> ProjectRunResult<ProjectRunReport> {
    let report = finish_report(
        plan,
        policy,
        target_nodes,
        report,
        valid_receipts,
        failed_nodes,
    )?;
    let previous_head = read_project_run_manifest_head_for_plan(plan, policy)?;
    let previous_revision_hash = previous_head
        .as_ref()
        .map(|revision| revision.revision_hash.clone());
    let revision = project_run_manifest_revision_for_report(plan, &report, previous_revision_hash)?;
    publish_project_run_manifest_revision(policy, &revision, previous_head.as_ref())?;
    Ok(report)
}

fn publish_project_run_manifest_revision(
    policy: &ProjectRunPolicy,
    revision: &ProjectRunManifestRevision,
    expected_existing_head: Option<&ProjectRunManifestRevision>,
) -> ProjectRunResult<()> {
    let revision = validate_project_run_manifest_revision(revision.clone())?;
    let revision_bytes = canonical_project_run_manifest_revision_bytes(&revision)?;
    let revision_path = project_run_manifest_revision_path(policy, &revision.revision_hash)?;
    write_project_run_manifest_revision_cas(&revision_path, &revision, &revision_bytes)?;

    let expected_head_bytes = expected_existing_head
        .map(canonical_project_run_manifest_revision_bytes)
        .transpose()?;
    let head_path = project_run_manifest_head_path(policy)?;
    match write_manifest_atomic_replace(
        &head_path,
        &revision_bytes,
        expected_head_bytes.as_deref(),
    )? {
        ManifestSlotWrite::Intended => Ok(()),
        ManifestSlotWrite::Existing => Err(ProjectRunError::new(
            ProjectRunErrorCode::AtomicPublication,
            None,
            format!(
                "refusing to replace project run manifest head {} because it no longer matches the expected previous revision",
                head_path.display()
            ),
        )),
    }
}

fn write_project_run_manifest_revision_cas(
    revision_path: &Path,
    revision: &ProjectRunManifestRevision,
    revision_bytes: &[u8],
) -> ProjectRunResult<()> {
    let expected_file_name = format!("{}.json", digest_leaf_token(&revision.revision_hash)?);
    if revision_path.file_name().and_then(|name| name.to_str()) != Some(expected_file_name.as_str())
    {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            None,
            format!(
                "project run manifest revision path {} does not match revision hash {}",
                revision_path.display(),
                revision.revision_hash
            ),
        ));
    }
    if fs::symlink_metadata(revision_path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::WorkspacePolicy,
            None,
            format!(
                "refusing content-addressed project run manifest revision symlink {}",
                revision_path.display()
            ),
        ));
    }
    if let Some(parent) = revision_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::AtomicPublication,
                None,
                format!(
                    "failed to create project run manifest revision directory {}: {error}",
                    parent.display()
                ),
            )
        })?;
    }
    match write_manifest_atomic_replace(revision_path, revision_bytes, None)? {
        ManifestSlotWrite::Intended => Ok(()),
        ManifestSlotWrite::Existing => Err(ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            None,
            format!(
                "refusing to replace immutable project run manifest revision {} because its bytes differ from {}",
                revision_path.display(),
                revision.revision_hash
            ),
        )),
    }
}

fn parse_project_run_manifest_revision(
    bytes: &[u8],
) -> ProjectRunResult<ProjectRunManifestRevision> {
    let revision: ProjectRunManifestRevision = serde_json::from_slice(bytes).map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            None,
            format!("failed to parse project run manifest revision JSON: {error}"),
        )
    })?;
    validate_project_run_manifest_revision(revision).map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            error.node_id,
            error.message,
        )
    })
}

fn finalized_project_run_manifest_revision(
    mut revision: ProjectRunManifestRevision,
) -> ProjectRunResult<ProjectRunManifestRevision> {
    canonicalize_project_run_manifest_revision(&mut revision);
    revision.revision_hash.clear();
    revision.revision_hash = compute_project_run_manifest_revision_hash(&revision)?;
    validate_project_run_manifest_revision(revision)
}

fn validate_project_run_manifest_revision(
    mut revision: ProjectRunManifestRevision,
) -> ProjectRunResult<ProjectRunManifestRevision> {
    canonicalize_project_run_manifest_revision(&mut revision);
    if revision.schema_version != CANON_PROJECT_RUN_MANIFEST_REVISION_VERSION {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            None,
            format!(
                "project run manifest revision schema_version must equal {CANON_PROJECT_RUN_MANIFEST_REVISION_VERSION}"
            ),
        ));
    }
    if revision.project_id.trim().is_empty() {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            None,
            "project run manifest revision project_id must be non-empty",
        ));
    }
    validate_blake3_digest_field("plan_graph_hash", &revision.plan_graph_hash)?;
    validate_blake3_digest_field("run_receipt_hash", &revision.run_receipt_hash)?;
    validate_blake3_digest_field("revision_hash", &revision.revision_hash)?;
    if let Some(previous) = &revision.previous_revision_hash {
        validate_blake3_digest_field("previous_revision_hash", previous)?;
        if previous == &revision.revision_hash {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                None,
                "project run manifest revision cannot point to itself as previous_revision_hash",
            ));
        }
    }

    validate_node_list("validated_nodes", &revision.validated_nodes)?;
    validate_node_list("completed_nodes", &revision.completed_nodes)?;
    validate_node_list("failed_nodes", &revision.failed_nodes)?;
    validate_node_list("cancelled_nodes", &revision.cancelled_nodes)?;
    validate_node_list("invalidated_nodes", &revision.invalidated_nodes)?;
    validate_node_list("blocked_nodes", &revision.blocked_nodes)?;

    let validated = revision
        .validated_nodes
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    for (field, values) in [
        ("completed_nodes", &revision.completed_nodes),
        ("failed_nodes", &revision.failed_nodes),
        ("cancelled_nodes", &revision.cancelled_nodes),
        ("invalidated_nodes", &revision.invalidated_nodes),
        ("blocked_nodes", &revision.blocked_nodes),
    ] {
        for node_id in values {
            if !validated.contains(node_id) {
                return Err(ProjectRunError::new(
                    ProjectRunErrorCode::ArtifactContract,
                    Some(node_id.clone()),
                    format!("{field} contains node not present in validated_nodes"),
                ));
            }
        }
    }
    validate_disjoint_manifest_status_nodes(&revision)?;
    validate_manifest_digest_map(
        "node_receipt_hashes",
        &revision.node_receipt_hashes,
        &validated,
    )?;
    validate_manifest_digest_map(
        "node_semantic_hashes",
        &revision.node_semantic_hashes,
        &validated,
    )?;
    for node_id in &revision.completed_nodes {
        if !revision.node_receipt_hashes.contains_key(node_id)
            || !revision.node_semantic_hashes.contains_key(node_id)
        {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(node_id.clone()),
                "completed project run manifest node must carry receipt and semantic hashes",
            ));
        }
    }
    for node_id in revision.node_semantic_hashes.keys() {
        if !revision.node_receipt_hashes.contains_key(node_id) {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(node_id.clone()),
                "project run manifest semantic hash cannot appear without a receipt hash",
            ));
        }
    }
    for key in revision.deterministic_usage.keys() {
        if key.trim().is_empty() {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                None,
                "project run manifest deterministic usage keys must be non-empty",
            ));
        }
    }

    let expected = compute_project_run_manifest_revision_hash(&revision)?;
    if revision.revision_hash != expected {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            None,
            format!(
                "project run manifest revision hash mismatch: expected {expected}, got {}",
                revision.revision_hash
            ),
        ));
    }
    Ok(revision)
}

fn validate_report_receipt_binding(
    plan: &ProjectPlan,
    report: &ProjectRunReport,
) -> ProjectRunResult<()> {
    if report.schema_version != CANON_PROJECT_RUN_VERSION
        || report.receipt.schema_version != CANON_PROJECT_RUN_VERSION
    {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            None,
            format!("project run report schema_version must equal {CANON_PROJECT_RUN_VERSION}"),
        ));
    }
    if report.project_id != plan.project_id || report.receipt.project_id != plan.project_id {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            None,
            "project run report project_id must match the project plan",
        ));
    }
    if report.plan_graph_hash != plan.graph_hash
        || report.receipt.plan_graph_hash != plan.graph_hash
    {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            None,
            "project run report plan_graph_hash must match the project plan",
        ));
    }
    canonical_run_receipt_bytes(&report.receipt).map_err(ProjectRunError::from)?;
    if report.run_receipt_hash != report.receipt.receipt_hash {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            None,
            "project run report run_receipt_hash must match its embedded run receipt",
        ));
    }
    let known_nodes = plan_node_ids(plan);
    validate_report_node_set("executed_nodes", &report.executed_nodes, &known_nodes)?;
    validate_report_node_set("resumed_nodes", &report.resumed_nodes, &known_nodes)?;
    validate_report_node_set("failed_nodes", &report.failed_nodes, &known_nodes)?;
    validate_report_node_set("cancelled_nodes", &report.cancelled_nodes, &known_nodes)?;
    validate_report_node_set("invalidated_nodes", &report.invalidated_nodes, &known_nodes)?;
    validate_report_node_set("blocked_nodes", &report.blocked_nodes, &known_nodes)?;
    validate_report_node_set(
        "receipt.completed_nodes",
        &report.receipt.completed_nodes,
        &known_nodes,
    )?;
    if report.receipt.completed_nodes
        != report
            .receipt
            .node_receipts
            .iter()
            .map(|receipt| receipt.node_id.clone())
            .collect::<Vec<_>>()
    {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            None,
            "project run receipt completed_nodes must match embedded completed node receipts",
        ));
    }
    if report.failed_nodes != report.receipt.failed_nodes
        || report.cancelled_nodes != report.receipt.cancelled_nodes
        || report.invalidated_nodes != report.receipt.invalidated_nodes
        || report.blocked_nodes != report.receipt.blocked_nodes
    {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            None,
            "project run report node status lists must match the embedded run receipt",
        ));
    }
    let mut completed_receipts = BTreeMap::new();
    for receipt in &report.receipt.node_receipts {
        canonical_node_receipt_bytes(receipt).map_err(ProjectRunError::from)?;
        if receipt.project_id != plan.project_id
            || receipt.outcome != ProjectRunNodeOutcome::Completed
        {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(receipt.node_id.clone()),
                "project run receipt can embed only completed node receipts for this project",
            ));
        }
        if !known_nodes.contains(&receipt.node_id) {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(receipt.node_id.clone()),
                "project run receipt embeds a node not present in the project plan",
            ));
        }
        completed_receipts.insert(receipt.node_id.clone(), receipt.receipt_hash.clone());
    }
    let mut report_nodes = BTreeSet::new();
    for node_report in &report.node_reports {
        if !report_nodes.insert(node_report.node_id.clone()) {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(node_report.node_id.clone()),
                "project run report contains duplicate node_reports entries",
            ));
        }
        if !known_nodes.contains(&node_report.node_id) {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(node_report.node_id.clone()),
                "project run report references a node not present in the project plan",
            ));
        }
        if let Some(receipt_hash) = &node_report.receipt_hash {
            validate_blake3_digest_field("node_report.receipt_hash", receipt_hash)?;
        }
        match node_report.outcome {
            ProjectRunNodeOutcome::Completed => {
                let expected = completed_receipts
                    .get(&node_report.node_id)
                    .ok_or_else(|| {
                        ProjectRunError::new(
                            ProjectRunErrorCode::ArtifactContract,
                            Some(node_report.node_id.clone()),
                            "completed node_report has no embedded completed node receipt",
                        )
                    })?;
                if node_report.receipt_hash.as_ref() != Some(expected) {
                    return Err(ProjectRunError::new(
                        ProjectRunErrorCode::ArtifactContract,
                        Some(node_report.node_id.clone()),
                        "completed node_report receipt hash must match embedded node receipt",
                    ));
                }
            }
            ProjectRunNodeOutcome::Failed => {
                if !report.failed_nodes.contains(&node_report.node_id) {
                    return Err(ProjectRunError::new(
                        ProjectRunErrorCode::ArtifactContract,
                        Some(node_report.node_id.clone()),
                        "failed node_report must be listed in failed_nodes",
                    ));
                }
            }
            ProjectRunNodeOutcome::Cancelled => {
                if !report.cancelled_nodes.contains(&node_report.node_id) {
                    return Err(ProjectRunError::new(
                        ProjectRunErrorCode::ArtifactContract,
                        Some(node_report.node_id.clone()),
                        "cancelled node_report must be listed in cancelled_nodes",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn canonicalize_project_run_manifest_revision(revision: &mut ProjectRunManifestRevision) {
    revision.validated_nodes.sort();
    revision.completed_nodes.sort();
    revision.failed_nodes.sort();
    revision.cancelled_nodes.sort();
    revision.invalidated_nodes.sort();
    revision.blocked_nodes.sort();
}

fn compute_project_run_manifest_revision_hash(
    revision: &ProjectRunManifestRevision,
) -> ProjectRunResult<String> {
    let mut hashable = revision.clone();
    canonicalize_project_run_manifest_revision(&mut hashable);
    hashable.revision_hash.clear();
    serde_json::to_vec(&hashable)
        .map(|bytes| digest_bytes(&bytes))
        .map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                None,
                format!("failed to hash project run manifest revision: {error}"),
            )
        })
}

fn validate_node_list(field: &str, values: &[String]) -> ProjectRunResult<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.trim().is_empty() {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                None,
                format!("{field} cannot contain an empty node id"),
            ));
        }
        if !seen.insert(value.as_str()) {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(value.clone()),
                format!("{field} cannot contain duplicate node ids"),
            ));
        }
    }
    Ok(())
}

fn validate_report_node_set(
    field: &str,
    values: &[String],
    known_nodes: &BTreeSet<String>,
) -> ProjectRunResult<()> {
    validate_node_list(field, values)?;
    for node_id in values {
        if !known_nodes.contains(node_id) {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(node_id.clone()),
                format!("{field} contains node not present in the project plan"),
            ));
        }
    }
    Ok(())
}

fn validate_disjoint_manifest_status_nodes(
    revision: &ProjectRunManifestRevision,
) -> ProjectRunResult<()> {
    let mut statuses = BTreeMap::<&str, &str>::new();
    for (field, values) in [
        ("completed_nodes", &revision.completed_nodes),
        ("failed_nodes", &revision.failed_nodes),
        ("cancelled_nodes", &revision.cancelled_nodes),
        ("blocked_nodes", &revision.blocked_nodes),
    ] {
        for node_id in values {
            if let Some(existing) = statuses.insert(node_id.as_str(), field) {
                return Err(ProjectRunError::new(
                    ProjectRunErrorCode::ArtifactContract,
                    Some(node_id.clone()),
                    format!("project run manifest node appears in both {existing} and {field}"),
                ));
            }
        }
    }
    Ok(())
}

fn validate_manifest_digest_map(
    field: &str,
    values: &BTreeMap<String, String>,
    validated_nodes: &BTreeSet<String>,
) -> ProjectRunResult<()> {
    for (node_id, digest) in values {
        if node_id.trim().is_empty() {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                None,
                format!("{field} cannot contain an empty node id"),
            ));
        }
        if !validated_nodes.contains(node_id) {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(node_id.clone()),
                format!("{field} contains node not present in validated_nodes"),
            ));
        }
        validate_blake3_digest_field(field, digest)?;
    }
    Ok(())
}

fn validate_blake3_digest_field(field: &str, value: &str) -> ProjectRunResult<()> {
    if is_lowercase_blake3_digest(value) {
        return Ok(());
    }
    Err(ProjectRunError::new(
        ProjectRunErrorCode::ArtifactContract,
        None,
        format!("{field} must be a lowercase blake3 digest"),
    ))
}

fn is_lowercase_blake3_digest(value: &str) -> bool {
    value.strip_prefix("blake3:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

fn digest_leaf_token(value: &str) -> ProjectRunResult<&str> {
    validate_blake3_digest_field("digest path token", value)?;
    Ok(value
        .strip_prefix("blake3:")
        .expect("validated blake3 digest has prefix"))
}

enum ManifestSlotWrite {
    Intended,
    Existing,
}

struct ManifestPublicationLock {
    path: PathBuf,
}

impl Drop for ManifestPublicationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn write_manifest_atomic_replace(
    path: &Path,
    bytes: &[u8],
    expected_existing: Option<&[u8]>,
) -> ProjectRunResult<ManifestSlotWrite> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::AtomicPublication,
                None,
                format!(
                    "failed to create project run manifest directory {}: {error}",
                    parent.display()
                ),
            )
        })?;
    }
    let temp_path = atomic_manifest_temp_path(path, bytes);
    let _slot_lock = acquire_manifest_publication_lock(path, &temp_path, bytes)?;
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
    {
        Ok(mut file) => {
            file.write_all(bytes).map_err(|error| {
                let _ = fs::remove_file(&temp_path);
                manifest_io_error(&temp_path, error)
            })?;
            file.sync_all().map_err(|error| {
                let _ = fs::remove_file(&temp_path);
                manifest_io_error(&temp_path, error)
            })?;
            drop(file);
            finish_manifest_atomic_replace(path, &temp_path, bytes, expected_existing)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            recover_manifest_atomic_temp(path, &temp_path, bytes, expected_existing)
        }
        Err(error) => Err(manifest_io_error(&temp_path, error)),
    }
}

fn acquire_manifest_publication_lock(
    path: &Path,
    temp_path: &Path,
    bytes: &[u8],
) -> ProjectRunResult<ManifestPublicationLock> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("run-manifest");
    let lock_path = path.with_file_name(format!(".{file_name}.publish.lock"));
    loop {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(file) => {
                file.sync_all().map_err(|error| {
                    let _ = fs::remove_file(&lock_path);
                    manifest_io_error(&lock_path, error)
                })?;
                return Ok(ManifestPublicationLock { path: lock_path });
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if recoverable_manifest_publication_lock(path, temp_path, bytes)? {
                    fs::remove_file(&lock_path)
                        .map_err(|error| manifest_io_error(&lock_path, error))?;
                    continue;
                }
                return Err(ProjectRunError::new(
                    ProjectRunErrorCode::AtomicPublication,
                    None,
                    format!(
                        "refusing concurrent publication of project run manifest {} while lock {} is active; retry after the current publisher completes",
                        path.display(),
                        lock_path.display()
                    ),
                ));
            }
            Err(error) => return Err(manifest_io_error(&lock_path, error)),
        }
    }
}

fn recoverable_manifest_publication_lock(
    path: &Path,
    temp_path: &Path,
    bytes: &[u8],
) -> ProjectRunResult<bool> {
    match fs::read(path) {
        Ok(existing) if existing == bytes => return Ok(true),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(manifest_io_error(path, error)),
    }
    match fs::read(temp_path) {
        Ok(existing) => Ok(existing == bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(manifest_io_error(temp_path, error)),
    }
}

fn recover_manifest_atomic_temp(
    path: &Path,
    temp_path: &Path,
    bytes: &[u8],
    expected_existing: Option<&[u8]>,
) -> ProjectRunResult<ManifestSlotWrite> {
    let existing = fs::read(temp_path).map_err(|error| manifest_io_error(temp_path, error))?;
    if existing != bytes {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::AtomicPublication,
            None,
            format!(
                "refusing to reuse atomic project run manifest temp {} because its contents do not match the intended revision bytes",
                temp_path.display()
            ),
        ));
    }
    finish_manifest_atomic_replace(path, temp_path, bytes, expected_existing)
}

fn finish_manifest_atomic_replace(
    path: &Path,
    temp_path: &Path,
    bytes: &[u8],
    expected_existing: Option<&[u8]>,
) -> ProjectRunResult<ManifestSlotWrite> {
    match fs::read(path) {
        Ok(existing) if existing == bytes => {
            let _ = fs::remove_file(temp_path);
            Ok(ManifestSlotWrite::Intended)
        }
        Ok(existing) if expected_existing.is_some_and(|expected| expected == existing) => {
            fs::rename(temp_path, path).map_err(|error| manifest_io_error(temp_path, error))?;
            Ok(ManifestSlotWrite::Intended)
        }
        Ok(_) => {
            let _ = fs::remove_file(temp_path);
            Ok(ManifestSlotWrite::Existing)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::rename(temp_path, path).map_err(|error| manifest_io_error(temp_path, error))?;
            Ok(ManifestSlotWrite::Intended)
        }
        Err(error) => Err(manifest_io_error(path, error)),
    }
}

fn atomic_manifest_temp_path(path: &Path, bytes: &[u8]) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("run-manifest");
    path.with_file_name(format!(
        "{}.{}.tmp",
        file_name,
        digest_bytes(bytes).replace(':', "_")
    ))
}

fn run_manifest_head_relative_path(work_dir: &Path) -> ProjectRunResult<PathBuf> {
    let work_dir = normalize_relative_path(work_dir).map_err(|message| {
        ProjectRunError::new(ProjectRunErrorCode::WorkspacePolicy, None, message)
    })?;
    Ok(work_dir.join("run-manifest").join("head.json"))
}

fn run_manifest_revision_relative_path(
    work_dir: &Path,
    revision_hash: &str,
) -> ProjectRunResult<PathBuf> {
    let work_dir = normalize_relative_path(work_dir).map_err(|message| {
        ProjectRunError::new(ProjectRunErrorCode::WorkspacePolicy, None, message)
    })?;
    Ok(work_dir
        .join("run-manifest")
        .join("revisions")
        .join(format!("{}.json", digest_leaf_token(revision_hash)?)))
}

fn manifest_io_error(path: &Path, error: io::Error) -> ProjectRunError {
    ProjectRunError::new(
        ProjectRunErrorCode::AtomicPublication,
        None,
        format!(
            "failed to write project run manifest {}: {error}",
            path.display()
        ),
    )
}

type ProjectInternalExecutorFn =
    fn(&ProjectInternalNodeExecution<'_>) -> ProjectRunResult<ProjectNodeExecutionResult>;
type ProjectInternalValidatorFn =
    fn(&ProjectPlanNode, &Path, &ProjectInternalCommand) -> ProjectRunResult<()>;

#[derive(Clone, Copy)]
struct ProjectInternalExecutorEntry {
    validate: ProjectInternalValidatorFn,
    execute: ProjectInternalExecutorFn,
}

#[derive(Debug, Clone)]
struct ProjectInternalCommand {
    executor_id: String,
    args: BTreeMap<String, String>,
}

struct ProjectInternalNodeExecution<'a> {
    node: &'a ProjectPlanNode,
    context: &'a ProjectNodeExecutionContext,
    workspace_root: &'a Path,
    command: &'a ProjectInternalCommand,
}

pub struct ProjectRegisteredNodeExecutor {
    workspace_root: PathBuf,
    executors: BTreeMap<String, ProjectInternalExecutorEntry>,
}

impl ProjectRegisteredNodeExecutor {
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        let mut executor = Self {
            workspace_root: workspace_root.into(),
            executors: BTreeMap::new(),
        };
        executor.register(
            PROJECT_INTERNAL_COPY_FILE_EXECUTOR,
            validate_copy_file_executor_contract,
            execute_copy_file_executor,
        );
        executor
    }

    fn register(
        &mut self,
        executor_id: &str,
        validate: ProjectInternalValidatorFn,
        execute: ProjectInternalExecutorFn,
    ) {
        self.executors.insert(
            executor_id.to_string(),
            ProjectInternalExecutorEntry { validate, execute },
        );
    }

    fn declaration_for_node(
        &self,
        node: &ProjectPlanNode,
    ) -> ProjectRunResult<(ProjectInternalCommand, ProjectInternalExecutorEntry)> {
        let command = parse_internal_command(node)?;
        let Some(entry) = self.executors.get(&command.executor_id) else {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ExecutionFailed,
                Some(node.node_id.clone()),
                format!(
                    "project run has no registered real executor named {} for pending node {}",
                    command.executor_id, node.node_id
                ),
            ));
        };
        (entry.validate)(node, &self.workspace_root, &command)?;
        Ok((command, *entry))
    }
}

impl ProjectNodeExecutor for ProjectRegisteredNodeExecutor {
    fn execute(
        &mut self,
        node: &ProjectPlanNode,
        context: &ProjectNodeExecutionContext,
    ) -> ProjectRunResult<ProjectNodeExecutionResult> {
        let (command, entry) = self.declaration_for_node(node)?;
        let execution = ProjectInternalNodeExecution {
            node,
            context,
            workspace_root: &self.workspace_root,
            command: &command,
        };
        (entry.execute)(&execution)
    }
}

fn ensure_pending_nodes_have_registered_executors(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
    executor: &ProjectRegisteredNodeExecutor,
) -> ProjectRunResult<()> {
    validate_plan_shape(plan)?;
    let policy = finalize_policy(policy)?;
    let plan_nodes = plan_node_ids(plan);
    let target_nodes = selected_node_closure(plan, &policy.selected_nodes)?;
    let existing = validate_existing_receipts(plan, &policy, &plan_nodes)?;
    if !existing.poisoned_receipts.is_empty() {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            None,
            format!(
                "poisoned project receipts: {}",
                existing.poisoned_receipts.join(", ")
            ),
        ));
    }

    let mut valid_receipts = existing.valid_receipts;
    let completed_receipts = existing.completed_receipts;
    let mut invalidated_nodes = existing.invalidated_nodes;
    invalidated_nodes.extend(descendants(plan, &invalidated_nodes));
    for invalidated in &invalidated_nodes {
        valid_receipts.remove(invalidated);
    }

    for node in &plan.nodes {
        if !target_nodes.contains(&node.node_id) || valid_receipts.contains_key(&node.node_id) {
            continue;
        }
        ensure_declared_node_effects_allowed(node, &policy)?;
        ensure_outputs_publishable(
            node,
            &policy.workspace_root,
            completed_receipts.get(&node.node_id),
        )?;
        executor.declaration_for_node(node)?;
    }
    Ok(())
}

fn parse_internal_command(node: &ProjectPlanNode) -> ProjectRunResult<ProjectInternalCommand> {
    let tokens = node.command.split_ascii_whitespace().collect::<Vec<_>>();
    if tokens.len() < 4
        || tokens[0] != "canon"
        || tokens[1] != "project"
        || tokens[2] != "internal-node"
    {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ExecutionFailed,
            Some(node.node_id.clone()),
            format!(
                "project run has no registered real executor for pending node {}; command must declare `canon project internal-node <executor-id>`",
                node.node_id
            ),
        ));
    }
    let mut args = BTreeMap::new();
    let mut index = 4;
    while index < tokens.len() {
        let flag = tokens[index];
        let Some(key) = flag.strip_prefix("--") else {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ExecutionFailed,
                Some(node.node_id.clone()),
                format!("internal executor argument {flag} must be a --key token"),
            ));
        };
        if key.is_empty() || index + 1 >= tokens.len() || tokens[index + 1].starts_with("--") {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ExecutionFailed,
                Some(node.node_id.clone()),
                format!("internal executor argument --{key} must have a value"),
            ));
        }
        if args
            .insert(key.to_string(), tokens[index + 1].to_string())
            .is_some()
        {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ExecutionFailed,
                Some(node.node_id.clone()),
                format!("internal executor argument --{key} was declared more than once"),
            ));
        }
        index += 2;
    }
    Ok(ProjectInternalCommand {
        executor_id: tokens[3].to_string(),
        args,
    })
}

fn validate_copy_file_executor_contract(
    node: &ProjectPlanNode,
    workspace_root: &Path,
    command: &ProjectInternalCommand,
) -> ProjectRunResult<()> {
    let input = required_internal_arg(node, command, "input")?;
    let input_digest = required_internal_arg(node, command, "input-digest")?;
    let output_id = required_internal_arg(node, command, "output-id")?;
    let output_digest = required_internal_arg(node, command, "output-digest")?;
    reject_extra_internal_args(
        node,
        command,
        &["input", "input-digest", "output-id", "output-digest"],
    )?;
    validate_blake3_arg(node, "input-digest", input_digest)?;
    validate_blake3_arg(node, "output-digest", output_digest)?;
    resolve_internal_read_path(node, workspace_root, input)?;
    normalize_relative_path(Path::new(input)).map_err(|message| {
        ProjectRunError::new(
            ProjectRunErrorCode::WorkspacePolicy,
            Some(node.node_id.clone()),
            format!("internal executor input path is outside the workspace: {message}"),
        )
    })?;
    if node.class != ProjectPlanNodeClass::Computation {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ExecutionFailed,
            Some(node.node_id.clone()),
            "copy-file-v1 can execute only computation nodes",
        ));
    }
    if !declares_side_effect(node, ProjectPlanSideEffectKind::ReadsInput)
        || !declares_side_effect(node, ProjectPlanSideEffectKind::WritesArtifact)
    {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ExecutionFailed,
            Some(node.node_id.clone()),
            "copy-file-v1 nodes must declare both read-input and write-artifact side effects",
        ));
    }
    if node.side_effects.iter().any(|effect| {
        !matches!(
            effect.kind,
            ProjectPlanSideEffectKind::ReadsInput | ProjectPlanSideEffectKind::WritesArtifact
        )
    }) {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ExecutionFailed,
            Some(node.node_id.clone()),
            "copy-file-v1 is offline/read-only apart from declared artifact publication",
        ));
    }
    if node.outputs.len() != 1
        || node.outputs[0].output_id != output_id
        || node.outputs[0].materialization != ProjectPlanOutputMaterialization::PlannedArtifact
    {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ExecutionFailed,
            Some(node.node_id.clone()),
            "copy-file-v1 must bind exactly one planned-artifact output matching --output-id",
        ));
    }
    if input_digest != output_digest {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ExecutionFailed,
            Some(node.node_id.clone()),
            "copy-file-v1 output digest must match the declared input digest",
        ));
    }
    if !node
        .content_hash_inputs
        .iter()
        .any(|input| input.content_hash == input_digest)
    {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ExecutionFailed,
            Some(node.node_id.clone()),
            "copy-file-v1 input digest must be present in content_hash_inputs",
        ));
    }
    Ok(())
}

fn execute_copy_file_executor(
    execution: &ProjectInternalNodeExecution<'_>,
) -> ProjectRunResult<ProjectNodeExecutionResult> {
    validate_copy_file_executor_contract(
        execution.node,
        execution.workspace_root,
        execution.command,
    )?;
    let input = required_internal_arg(execution.node, execution.command, "input")?;
    let input_digest = required_internal_arg(execution.node, execution.command, "input-digest")?;
    let output_id = required_internal_arg(execution.node, execution.command, "output-id")?;
    let output_digest = required_internal_arg(execution.node, execution.command, "output-digest")?;
    let absolute_input =
        resolve_internal_read_path(execution.node, execution.workspace_root, input)?;
    let bytes = fs::read(&absolute_input).map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::ExecutionFailed,
            Some(execution.node.node_id.clone()),
            format!(
                "copy-file-v1 could not read declared input {}: {error}",
                absolute_input.display()
            ),
        )
    })?;
    let actual_digest = digest_bytes(&bytes);
    if actual_digest != input_digest || actual_digest != output_digest {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ExecutionFailed,
            Some(execution.node.node_id.clone()),
            format!(
                "copy-file-v1 content digest mismatch for {}: expected {}, got {}",
                absolute_input.display(),
                input_digest,
                actual_digest
            ),
        ));
    }
    let mut outputs = BTreeMap::new();
    outputs.insert(output_id.to_string(), bytes);
    let mut result = ProjectNodeExecutionResult::with_outputs(outputs);
    result.deterministic_usage.insert(
        "dependency_semantic_hash_count".to_string(),
        execution.context.dependency_semantic_hashes.len() as u64,
    );
    result.deterministic_usage.insert(
        "input_bytes".to_string(),
        result.outputs[output_id].len() as u64,
    );
    Ok(result)
}

fn resolve_internal_read_path(
    node: &ProjectPlanNode,
    workspace_root: &Path,
    relative_path: &str,
) -> ProjectRunResult<PathBuf> {
    resolve_fs_workspace_path(
        workspace_root,
        "internal_executor.input",
        Path::new(relative_path),
        PlannedAccess::Read,
    )
    .map(|resolution| resolution.absolute_path)
    .map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::WorkspacePolicy,
            Some(node.node_id.clone()),
            format!("internal executor input path failed workspace safety: {error}"),
        )
    })
}

fn required_internal_arg<'a>(
    node: &ProjectPlanNode,
    command: &'a ProjectInternalCommand,
    key: &str,
) -> ProjectRunResult<&'a str> {
    command.args.get(key).map(String::as_str).ok_or_else(|| {
        ProjectRunError::new(
            ProjectRunErrorCode::ExecutionFailed,
            Some(node.node_id.clone()),
            format!("internal executor {} requires --{key}", command.executor_id),
        )
    })
}

fn reject_extra_internal_args(
    node: &ProjectPlanNode,
    command: &ProjectInternalCommand,
    allowed: &[&str],
) -> ProjectRunResult<()> {
    let allowed = allowed.iter().copied().collect::<BTreeSet<_>>();
    let extras = command
        .args
        .keys()
        .filter(|key| !allowed.contains(key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if extras.is_empty() {
        return Ok(());
    }
    Err(ProjectRunError::new(
        ProjectRunErrorCode::ExecutionFailed,
        Some(node.node_id.clone()),
        format!(
            "internal executor {} received unsupported arguments: {}",
            command.executor_id,
            extras.join(", ")
        ),
    ))
}

fn validate_blake3_arg(node: &ProjectPlanNode, key: &str, value: &str) -> ProjectRunResult<()> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(invalid_blake3_arg(node, key));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(invalid_blake3_arg(node, key));
    }
    Ok(())
}

fn invalid_blake3_arg(node: &ProjectPlanNode, key: &str) -> ProjectRunError {
    ProjectRunError::new(
        ProjectRunErrorCode::ExecutionFailed,
        Some(node.node_id.clone()),
        format!("internal executor argument --{key} must be a blake3 digest"),
    )
}

fn declares_side_effect(node: &ProjectPlanNode, kind: ProjectPlanSideEffectKind) -> bool {
    node.side_effects.iter().any(|effect| effect.kind == kind)
}

fn execute_node<E: ProjectNodeExecutor>(
    plan: &ProjectPlan,
    node: &ProjectPlanNode,
    policy: &ProjectRunPolicy,
    valid_receipts: &BTreeMap<String, ProjectRunNodeReceipt>,
    completed_receipts: &BTreeMap<String, ProjectRunNodeReceipt>,
    prior_receipts: &BTreeMap<String, ProjectRunNodeReceipt>,
    executor: &mut E,
) -> ProjectRunResult<ProjectRunNodeReceipt> {
    if node.class == ProjectPlanNodeClass::MutationGate && !policy.allow_mutation_gates {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::Cancelled,
            Some(node.node_id.clone()),
            "mutation gate requires explicit project run approval",
        ));
    }
    ensure_declared_node_effects_allowed(node, policy)?;
    let stale_receipt = completed_receipts.get(&node.node_id);
    ensure_outputs_publishable(node, &policy.workspace_root, stale_receipt)?;
    let dependency_semantic_hashes = dependency_semantic_hashes(node, valid_receipts)?;
    let dependency_receipt_hashes = dependency_receipt_hashes(node, valid_receipts)?;
    let dependency_outputs = dependency_outputs(node, valid_receipts, &policy.workspace_root)?;
    let context = ProjectNodeExecutionContext {
        node_id: node.node_id.clone(),
        dependency_semantic_hashes: dependency_semantic_hashes.clone(),
        dependency_outputs,
    };
    let result = executor.execute(node, &context)?;
    let output_receipts = prepare_outputs(
        node,
        &policy.workspace_root,
        &policy.work_dir,
        &result.outputs,
    )?;
    let receipt = finalized_node_receipt(ProjectRunNodeReceipt {
        schema_version: CANON_PROJECT_RUN_VERSION.to_string(),
        project_id: plan.project_id.clone(),
        plan_graph_hash: plan.graph_hash.clone(),
        node_id: node.node_id.clone(),
        node_cache_key: node.cache.cache_key.clone(),
        content_hash_inputs: receipt_hash_inputs(&node.content_hash_inputs),
        dependency_semantic_hashes,
        dependency_receipt_hashes,
        outputs: output_receipts,
        outcome: ProjectRunNodeOutcome::Completed,
        deterministic_usage: result.deterministic_usage,
        duration_millis: result.duration_millis,
        resource_observations: result.resource_observations,
        next_action: ProjectRunNextAction::ExecuteDependents,
        failure_code: None,
        failure_message: None,
        semantic_hash: String::new(),
        telemetry_hash: String::new(),
        receipt_hash: String::new(),
    })
    .map_err(ProjectRunError::from)?;
    commit_completed_receipt(
        plan,
        policy,
        node,
        &receipt,
        prior_receipts.get(&node.node_id),
    )
}

fn dependency_outputs(
    node: &ProjectPlanNode,
    valid_receipts: &BTreeMap<String, ProjectRunNodeReceipt>,
    workspace_root: &Path,
) -> ProjectRunResult<BTreeMap<String, Vec<ProjectDependencyOutput>>> {
    let mut dependencies = BTreeMap::new();
    for dependency_id in &node.dependencies {
        let receipt = valid_receipts.get(dependency_id).ok_or_else(|| {
            ProjectRunError::new(
                ProjectRunErrorCode::StaleArtifact,
                Some(node.node_id.clone()),
                format!(
                    "dependency {dependency_id} has no validated completed receipt before execution"
                ),
            )
        })?;
        let mut outputs = Vec::with_capacity(receipt.outputs.len());
        for output in &receipt.outputs {
            let path = resolve_fs_workspace_path(
                workspace_root,
                "project_run.dependency_output",
                Path::new(&output.path),
                PlannedAccess::Read,
            )
            .map(|resolution| resolution.absolute_path)
            .map_err(|error| {
                ProjectRunError::new(
                    ProjectRunErrorCode::WorkspacePolicy,
                    Some(node.node_id.clone()),
                    format!(
                        "dependency output {}:{} failed workspace safety: {error}",
                        dependency_id, output.output_id
                    ),
                )
            })?;
            let bytes = fs::read(&path).map_err(|error| {
                ProjectRunError::new(
                    ProjectRunErrorCode::StaleArtifact,
                    Some(node.node_id.clone()),
                    format!(
                        "failed to read dependency output {}:{}: {error}",
                        dependency_id, output.output_id
                    ),
                )
            })?;
            let byte_count = u64::try_from(bytes.len()).map_err(|_| {
                ProjectRunError::new(
                    ProjectRunErrorCode::StaleArtifact,
                    Some(node.node_id.clone()),
                    format!(
                        "dependency output {}:{} byte count exceeds u64",
                        dependency_id, output.output_id
                    ),
                )
            })?;
            let content_digest = digest_bytes(&bytes);
            if byte_count != output.byte_count || content_digest != output.content_digest {
                return Err(ProjectRunError::new(
                    ProjectRunErrorCode::StaleArtifact,
                    Some(node.node_id.clone()),
                    format!(
                        "dependency output {}:{} bytes no longer match its validated receipt",
                        dependency_id, output.output_id
                    ),
                ));
            }
            outputs.push(ProjectDependencyOutput {
                output_id: output.output_id.clone(),
                content_digest,
                byte_count,
                bytes,
            });
        }
        outputs.sort_by(|left, right| left.output_id.cmp(&right.output_id));
        dependencies.insert(dependency_id.clone(), outputs);
    }
    Ok(dependencies)
}

fn finish_report(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
    target_nodes: &BTreeSet<String>,
    mut report: ProjectRunReport,
    valid_receipts: BTreeMap<String, ProjectRunNodeReceipt>,
    failed_nodes: BTreeSet<String>,
) -> ProjectRunResult<ProjectRunReport> {
    let valid_receipts = valid_receipts
        .into_iter()
        .filter(|(node_id, _)| target_nodes.contains(node_id))
        .collect::<BTreeMap<_, _>>();
    let completed = valid_receipts.keys().cloned().collect::<BTreeSet<_>>();
    let mut blocked = BTreeSet::new();
    for node in plan
        .nodes
        .iter()
        .filter(|node| target_nodes.contains(&node.node_id))
    {
        if completed.contains(&node.node_id)
            || report.cancelled_nodes.contains(&node.node_id)
            || report.failed_nodes.contains(&node.node_id)
        {
            continue;
        }
        if node.class == ProjectPlanNodeClass::MutationGate && !policy.allow_mutation_gates {
            blocked.insert(node.node_id.clone());
            continue;
        }
        if node
            .dependencies
            .iter()
            .any(|dependency| failed_nodes.contains(dependency) || !completed.contains(dependency))
        {
            blocked.insert(node.node_id.clone());
        }
    }
    report.blocked_nodes = blocked.iter().cloned().collect();
    report.next_actions = plan
        .nodes
        .iter()
        .filter(|node| target_nodes.contains(&node.node_id))
        .filter(|node| !completed.contains(&node.node_id) && !blocked.contains(&node.node_id))
        .map(|node| (node.node_id.clone(), node.command.clone()))
        .collect();

    let receipt = finalized_run_receipt(ProjectRunReceipt {
        schema_version: CANON_PROJECT_RUN_VERSION.to_string(),
        project_id: plan.project_id.clone(),
        plan_graph_hash: plan.graph_hash.clone(),
        receipt_hash: String::new(),
        completed_nodes: completed.iter().cloned().collect(),
        failed_nodes: report.failed_nodes.clone(),
        cancelled_nodes: report.cancelled_nodes.clone(),
        invalidated_nodes: report.invalidated_nodes.clone(),
        blocked_nodes: report.blocked_nodes.clone(),
        node_receipts: valid_receipts.values().cloned().collect(),
    })
    .map_err(ProjectRunError::from)?;
    canonical_run_receipt_bytes(&receipt).map_err(ProjectRunError::from)?;
    report.run_receipt_hash = receipt.receipt_hash.clone();
    report.receipt = receipt;
    sort_report(&mut report);
    Ok(report)
}

#[derive(Debug, Default)]
struct ExistingReceipts {
    valid_receipts: BTreeMap<String, ProjectRunNodeReceipt>,
    completed_receipts: BTreeMap<String, ProjectRunNodeReceipt>,
    prior_receipts: BTreeMap<String, ProjectRunNodeReceipt>,
    semantic_backfill_nodes: BTreeSet<String>,
    invalidated_nodes: BTreeSet<String>,
    poisoned_receipts: Vec<String>,
}

fn validate_existing_receipts(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
    target_nodes: &BTreeSet<String>,
) -> ProjectRunResult<ExistingReceipts> {
    let mut existing = ExistingReceipts::default();
    for node in plan
        .nodes
        .iter()
        .filter(|node| target_nodes.contains(&node.node_id))
    {
        validate_receipt_storage_paths(policy, &node.node_id)?;
        let path = receipt_path(policy, &node.node_id)?;
        if !path.exists() {
            continue;
        }
        let receipt = match read_node_receipt(&path) {
            Ok(receipt) => receipt,
            Err(error) => {
                existing
                    .poisoned_receipts
                    .push(format!("{} ({})", path.display(), error.message));
                continue;
            }
        };
        if receipt.project_id != plan.project_id || receipt.node_id != node.node_id {
            existing.poisoned_receipts.push(format!(
                "{} (receipt belongs to project_id={} node_id={}, expected project_id={} node_id={})",
                path.display(),
                receipt.project_id,
                receipt.node_id,
                plan.project_id,
                node.node_id
            ));
            continue;
        }
        existing
            .prior_receipts
            .insert(node.node_id.clone(), receipt.clone());
        if receipt.outcome != ProjectRunNodeOutcome::Completed {
            continue;
        }
        existing
            .completed_receipts
            .insert(node.node_id.clone(), receipt.clone());
    }

    // Project plans are serialized in deterministic node-id order, not
    // necessarily dependency order. Reuse validation must therefore walk the
    // DAG topologically: validating a child before its completed parent has
    // been admitted would falsely invalidate the child and every descendant.
    for node in dependency_ordered_nodes(plan)?
        .into_iter()
        .filter(|node| target_nodes.contains(&node.node_id))
    {
        if existing
            .completed_receipts
            .get(&node.node_id)
            .is_some_and(|receipt| {
                receipt.node_cache_key == node.cache.cache_key
                    && !node_receipt_matches_current(plan, node, receipt)
            })
        {
            existing.invalidated_nodes.insert(node.node_id.clone());
            continue;
        }
        let Ok(expected_dependency_semantics) =
            dependency_semantic_hashes(node, &existing.valid_receipts)
        else {
            if existing.completed_receipts.contains_key(&node.node_id) {
                existing.invalidated_nodes.insert(node.node_id.clone());
            }
            continue;
        };
        let result_cache_key =
            semantic_node_result_cache_key(&node.cache.cache_key, &expected_dependency_semantics)
                .map_err(ProjectRunError::from)?;
        let semantic_path = semantic_receipt_path(
            policy,
            &node.node_id,
            &result_cache_key,
            PlannedAccess::Write,
        )?;
        let semantic_receipt = if semantic_path.exists() {
            match read_node_receipt(&semantic_path) {
                Ok(receipt) => Some(receipt),
                Err(error) => {
                    existing.poisoned_receipts.push(format!(
                        "{} ({})",
                        semantic_path.display(),
                        error.message
                    ));
                    continue;
                }
            }
        } else {
            None
        };
        let receipt = semantic_receipt
            .as_ref()
            .or_else(|| existing.completed_receipts.get(&node.node_id));
        let Some(receipt) = receipt else {
            continue;
        };
        if receipt.project_id != plan.project_id
            || receipt.node_id != node.node_id
            || receipt.node_cache_key != node.cache.cache_key
        {
            if semantic_receipt.is_some() {
                existing.poisoned_receipts.push(format!(
                    "{} (semantic receipt belongs to project_id={} node_id={} cache_key={}, expected project_id={} node_id={} cache_key={})",
                    semantic_path.display(),
                    receipt.project_id,
                    receipt.node_id,
                    receipt.node_cache_key,
                    plan.project_id,
                    node.node_id,
                    node.cache.cache_key
                ));
            } else {
                existing.invalidated_nodes.insert(node.node_id.clone());
            }
            continue;
        }
        let dependencies_match =
            dependencies_match_receipts(plan, policy, node, receipt, &existing.valid_receipts)?;
        if !node_receipt_matches_current(plan, node, receipt) || !dependencies_match {
            if semantic_receipt.is_some() {
                existing.poisoned_receipts.push(format!(
                    "{} (semantic receipt does not match the current node path/digest/count or dependency bindings)",
                    semantic_path.display()
                ));
            } else {
                existing.invalidated_nodes.insert(node.node_id.clone());
            }
            continue;
        }

        let canonical_relative = receipt_relative_path(policy, &node.node_id)?;
        let semantic_relative =
            semantic_receipt_relative_path(policy, &node.node_id, &result_cache_key)?;
        validate_receipt_cas_leaf_if_present(
            policy,
            &canonical_relative,
            receipt,
            "canonical project receipt CAS",
        )?;
        validate_receipt_cas_leaf_if_present(
            policy,
            &semantic_relative,
            receipt,
            "semantic project receipt CAS",
        )?;

        if semantic_receipt.is_none() {
            if !outputs_match_receipt(&policy.workspace_root, receipt)? {
                existing.invalidated_nodes.insert(node.node_id.clone());
                continue;
            }
            validate_existing_artifact_cas_if_present(policy, receipt)?;
            existing
                .semantic_backfill_nodes
                .insert(node.node_id.clone());
        } else {
            validate_artifact_cas(policy, receipt)?;
        }
        validate_receipt_output_publication_preconditions(
            policy,
            node,
            receipt,
            existing.prior_receipts.get(&node.node_id),
        )?;
        existing
            .valid_receipts
            .insert(node.node_id.clone(), receipt.clone());
    }
    Ok(existing)
}

fn restore_existing_receipts(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
    target_nodes: &BTreeSet<String>,
    existing: &mut ExistingReceipts,
) -> ProjectRunResult<()> {
    for node in dependency_ordered_nodes(plan)?
        .into_iter()
        .filter(|node| target_nodes.contains(&node.node_id))
    {
        let Some(mut receipt) = existing.valid_receipts.get(&node.node_id).cloned() else {
            continue;
        };
        let result_cache_key = semantic_node_result_cache_key(
            &receipt.node_cache_key,
            &receipt.dependency_semantic_hashes,
        )
        .map_err(ProjectRunError::from)?;
        let semantic_relative =
            semantic_receipt_relative_path(policy, &node.node_id, &result_cache_key)?;
        if existing.semantic_backfill_nodes.contains(&node.node_id) {
            if !backfill_artifact_cas(policy, &receipt)? {
                return Err(ProjectRunError::new(
                    ProjectRunErrorCode::StaleArtifact,
                    Some(node.node_id.clone()),
                    "legacy receipt outputs changed after reuse validation; refusing partial restoration",
                ));
            }
            receipt = converge_workspace_receipt(
                policy,
                &semantic_relative,
                &receipt,
                None,
                "semantic project receipt",
            )?;
        } else {
            validate_artifact_cas(policy, &receipt)?;
        }

        materialize_receipt_outputs(
            plan,
            policy,
            node,
            &receipt,
            existing.prior_receipts.get(&node.node_id),
        )?;
        let canonical_relative = receipt_relative_path(policy, &node.node_id)?;
        receipt = converge_workspace_receipt(
            policy,
            &canonical_relative,
            &receipt,
            existing.prior_receipts.get(&node.node_id),
            "project receipt",
        )?;
        existing
            .valid_receipts
            .insert(node.node_id.clone(), receipt);
    }
    Ok(())
}

fn dependency_ordered_nodes(plan: &ProjectPlan) -> ProjectRunResult<Vec<&ProjectPlanNode>> {
    let mut remaining = plan
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut admitted = BTreeSet::new();
    let mut ordered = Vec::with_capacity(plan.nodes.len());
    while !remaining.is_empty() {
        let ready = remaining
            .iter()
            .filter(|(_, node)| {
                node.dependencies
                    .iter()
                    .all(|dependency| admitted.contains(dependency))
            })
            .map(|(node_id, _)| *node_id)
            .collect::<Vec<_>>();
        if ready.is_empty() {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                None,
                "project plan dependency order could not be resolved during receipt validation",
            ));
        }
        for node_id in ready {
            let node = remaining
                .remove(node_id)
                .expect("ready project node remains present");
            admitted.insert(node_id.to_string());
            ordered.push(node);
        }
    }
    Ok(ordered)
}

fn ready_nodes<'a>(
    plan: &'a ProjectPlan,
    target_nodes: &BTreeSet<String>,
    valid_receipts: &BTreeMap<String, ProjectRunNodeReceipt>,
    failed_nodes: &BTreeSet<String>,
    invalidated_nodes: &BTreeSet<String>,
    policy: &ProjectRunPolicy,
) -> Vec<&'a ProjectPlanNode> {
    plan.nodes
        .iter()
        .filter(|node| target_nodes.contains(&node.node_id))
        .filter(|node| !valid_receipts.contains_key(&node.node_id))
        .filter(|node| !failed_nodes.contains(&node.node_id))
        .filter(|node| {
            invalidated_nodes.contains(&node.node_id)
                || !node.dependencies.iter().any(|dependency| {
                    invalidated_nodes.contains(dependency)
                        && !valid_receipts.contains_key(dependency)
                })
        })
        .filter(|node| {
            if node.cache.decision == ProjectPlanCacheDecision::Hit {
                return false;
            }
            if node.class == ProjectPlanNodeClass::MutationGate && !policy.allow_mutation_gates {
                return false;
            }
            node.dependencies
                .iter()
                .all(|dependency| valid_receipts.contains_key(dependency))
        })
        .collect()
}

fn plan_node_ids(plan: &ProjectPlan) -> BTreeSet<String> {
    plan.nodes.iter().map(|node| node.node_id.clone()).collect()
}

fn selected_node_closure(
    plan: &ProjectPlan,
    selected_nodes: &BTreeSet<String>,
) -> ProjectRunResult<BTreeSet<String>> {
    let known = plan
        .nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut selected = if selected_nodes.is_empty() {
        known.iter().map(|value| (*value).to_string()).collect()
    } else {
        selected_nodes.clone()
    };
    for node_id in selected_nodes {
        if !known.contains(node_id.as_str()) {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(node_id.clone()),
                format!("selected node {node_id} is not present in the plan"),
            ));
        }
    }
    let by_id = plan
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut changed = true;
    while changed {
        changed = false;
        for node_id in selected.clone() {
            if let Some(node) = by_id.get(node_id.as_str()) {
                for dependency in &node.dependencies {
                    if selected.insert(dependency.clone()) {
                        changed = true;
                    }
                }
            }
        }
    }
    Ok(selected)
}

fn descendants(plan: &ProjectPlan, roots: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut changed = true;
    while changed {
        changed = false;
        for node in &plan.nodes {
            if out.contains(&node.node_id) || roots.contains(&node.node_id) {
                continue;
            }
            if node
                .dependencies
                .iter()
                .any(|dependency| roots.contains(dependency) || out.contains(dependency))
            {
                out.insert(node.node_id.clone());
                changed = true;
            }
        }
    }
    out
}

fn prepare_outputs(
    node: &ProjectPlanNode,
    workspace_root: &Path,
    work_dir: &Path,
    outputs: &BTreeMap<String, Vec<u8>>,
) -> ProjectRunResult<Vec<ProjectRunOutputReceipt>> {
    ensure_returned_outputs_match_declared(node, outputs)?;
    let mut receipts = Vec::new();
    for output in &node.outputs {
        let bytes = outputs.get(&output.output_id).ok_or_else(|| {
            ProjectRunError::new(
                ProjectRunErrorCode::ExecutionFailed,
                Some(node.node_id.clone()),
                format!(
                    "executor did not return bytes for output {}",
                    output.output_id
                ),
            )
        })?;
        let receipt = ProjectRunOutputReceipt {
            output_id: output.output_id.clone(),
            path: output.path.clone(),
            content_digest: digest_bytes(bytes),
            byte_count: bytes.len() as u64,
        };
        write_artifact_cas(workspace_root, work_dir, &receipt, bytes, &node.node_id)?;
        receipts.push(receipt);
    }
    Ok(receipts)
}

fn commit_completed_receipt(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
    node: &ProjectPlanNode,
    receipt: &ProjectRunNodeReceipt,
    expected_existing: Option<&ProjectRunNodeReceipt>,
) -> ProjectRunResult<ProjectRunNodeReceipt> {
    let canonical_relative = receipt_relative_path(policy, &receipt.node_id)?;
    let result_cache_key = semantic_node_result_cache_key(
        &receipt.node_cache_key,
        &receipt.dependency_semantic_hashes,
    )
    .map_err(ProjectRunError::from)?;
    let semantic_relative =
        semantic_receipt_relative_path(policy, &receipt.node_id, &result_cache_key)?;
    let winner = converge_workspace_receipt(
        policy,
        &semantic_relative,
        receipt,
        None,
        "semantic project receipt",
    )?;
    preserve_workspace_receipt_cas(policy, &canonical_relative, receipt, "project receipt")?;
    materialize_receipt_outputs(plan, policy, node, &winner, expected_existing)?;
    converge_workspace_receipt(
        policy,
        &canonical_relative,
        &winner,
        expected_existing,
        "project receipt",
    )
}

fn materialize_receipt_outputs(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
    node: &ProjectPlanNode,
    receipt: &ProjectRunNodeReceipt,
    expected_existing: Option<&ProjectRunNodeReceipt>,
) -> ProjectRunResult<()> {
    if !node_receipt_matches_current(plan, node, receipt) {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            Some(node.node_id.clone()),
            "semantic cache receipt does not match the current node contract",
        ));
    }
    for output in &receipt.outputs {
        let bytes = read_artifact_cas(policy, output, &node.node_id)?;
        let stale_output = expected_existing.and_then(|existing| {
            existing.outputs.iter().find(|candidate| {
                candidate.output_id == output.output_id && candidate.path == output.path
            })
        });
        publish_atomic_bytes(
            &policy.workspace_root,
            &policy.work_dir,
            Path::new(&output.path),
            &bytes,
            stale_output,
        )
        .map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::AtomicPublication,
                Some(node.node_id.clone()),
                error,
            )
        })?;
    }
    if outputs_match_receipt(&policy.workspace_root, receipt)? {
        return Ok(());
    }
    Err(ProjectRunError::new(
        ProjectRunErrorCode::StaleArtifact,
        Some(node.node_id.clone()),
        "published project outputs failed receipt digest/count revalidation",
    ))
}

fn validate_receipt_output_publication_preconditions(
    policy: &ProjectRunPolicy,
    node: &ProjectPlanNode,
    receipt: &ProjectRunNodeReceipt,
    expected_existing: Option<&ProjectRunNodeReceipt>,
) -> ProjectRunResult<()> {
    for output in &receipt.outputs {
        let path = resolve_fs_workspace_path(
            &policy.workspace_root,
            "project_run.output",
            Path::new(&output.path),
            PlannedAccess::Write,
        )
        .map(|resolution| resolution.absolute_path)
        .map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::WorkspacePolicy,
                Some(node.node_id.clone()),
                format!("project output path failed workspace safety: {error}"),
            )
        })?;
        if !path.exists() {
            continue;
        }
        let bytes = fs::read(&path).map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::AtomicPublication,
                Some(node.node_id.clone()),
                format!(
                    "failed to read existing artifact {} during restoration preflight: {error}",
                    path.display()
                ),
            )
        })?;
        if digest_bytes(&bytes) == output.content_digest && bytes.len() as u64 == output.byte_count
        {
            continue;
        }
        let stale_output = expected_existing.and_then(|existing| {
            existing.outputs.iter().find(|candidate| {
                candidate.output_id == output.output_id && candidate.path == output.path
            })
        });
        if stale_output.is_some_and(|stale| {
            digest_bytes(&bytes) == stale.content_digest && bytes.len() as u64 == stale.byte_count
        }) {
            continue;
        }
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::AtomicPublication,
            Some(node.node_id.clone()),
            format!(
                "refusing restoration because artifact {} matches neither the intended receipt nor the recoverable prior receipt",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn ensure_returned_outputs_match_declared(
    node: &ProjectPlanNode,
    outputs: &BTreeMap<String, Vec<u8>>,
) -> ProjectRunResult<()> {
    let declared = node
        .outputs
        .iter()
        .map(|output| output.output_id.clone())
        .collect::<BTreeSet<_>>();
    let returned = outputs.keys().cloned().collect::<BTreeSet<_>>();
    if returned == declared {
        return Ok(());
    }
    Err(ProjectRunError::new(
        ProjectRunErrorCode::ExecutionFailed,
        Some(node.node_id.clone()),
        format!(
            "executor outputs must exactly match declared output ids: expected [{}], got [{}]",
            declared.into_iter().collect::<Vec<_>>().join(", "),
            returned.into_iter().collect::<Vec<_>>().join(", ")
        ),
    ))
}

fn ensure_outputs_publishable(
    node: &ProjectPlanNode,
    workspace_root: &Path,
    stale_receipt: Option<&ProjectRunNodeReceipt>,
) -> ProjectRunResult<()> {
    for output in &node.outputs {
        let path = resolve_fs_workspace_path(
            workspace_root,
            "project_run.output",
            Path::new(&output.path),
            PlannedAccess::Write,
        )
        .map(|resolution| resolution.absolute_path)
        .map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::WorkspacePolicy,
                Some(node.node_id.clone()),
                format!("project output path failed workspace safety: {error}"),
            )
        })?;
        if !path.exists() {
            continue;
        }
        let Some(stale_output) = stale_receipt.and_then(|receipt| {
            receipt
                .outputs
                .iter()
                .find(|candidate| candidate.output_id == output.output_id)
        }) else {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::StaleArtifact,
                Some(node.node_id.clone()),
                format!(
                    "existing artifact {} has no valid prior receipt for recovery",
                    path.display()
                ),
            ));
        };
        if stale_output.path != output.path {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::StaleArtifact,
                Some(node.node_id.clone()),
                format!(
                    "existing artifact {} does not match the prior receipt path for {}",
                    path.display(),
                    output.output_id
                ),
            ));
        }
    }
    Ok(())
}

fn write_artifact_cas(
    workspace_root: &Path,
    work_dir: &Path,
    output: &ProjectRunOutputReceipt,
    bytes: &[u8],
    node_id: &str,
) -> ProjectRunResult<()> {
    if digest_bytes(bytes) != output.content_digest || bytes.len() as u64 != output.byte_count {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            Some(node_id.to_string()),
            format!(
                "output {} bytes do not match their artifact CAS binding",
                output.output_id
            ),
        ));
    }
    let cas_path =
        artifact_cas_relative_path(work_dir, &output.content_digest).map_err(|message| {
            ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(node_id.to_string()),
                message,
            )
        })?;
    let absolute_cas_path = resolve_fs_workspace_path(
        workspace_root,
        "project_run.artifact_cas",
        &cas_path,
        PlannedAccess::Write,
    )
    .map(|resolution| resolution.absolute_path)
    .map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::WorkspacePolicy,
            Some(node_id.to_string()),
            format!("artifact CAS path failed workspace safety: {error}"),
        )
    })?;
    if absolute_cas_path.exists() {
        let existing = fs::read(&absolute_cas_path).map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::ReceiptPoisoning,
                Some(node_id.to_string()),
                format!(
                    "failed to read existing content-addressed artifact {}: {error}",
                    absolute_cas_path.display()
                ),
            )
        })?;
        if digest_bytes(&existing) != output.content_digest
            || existing.len() as u64 != output.byte_count
        {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ReceiptPoisoning,
                Some(node_id.to_string()),
                format!(
                    "refusing poisoned content-addressed artifact {} for output {}",
                    absolute_cas_path.display(),
                    output.output_id
                ),
            ));
        }
        return Ok(());
    }
    publish_atomic_bytes(workspace_root, work_dir, &cas_path, bytes, None).map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::AtomicPublication,
            Some(node_id.to_string()),
            format!(
                "content-addressed artifact publication for {} failed: {error}",
                output.output_id
            ),
        )
    })
}

fn validate_artifact_cas(
    policy: &ProjectRunPolicy,
    receipt: &ProjectRunNodeReceipt,
) -> ProjectRunResult<()> {
    for output in &receipt.outputs {
        read_artifact_cas(policy, output, &receipt.node_id)?;
    }
    Ok(())
}

fn validate_existing_artifact_cas_if_present(
    policy: &ProjectRunPolicy,
    receipt: &ProjectRunNodeReceipt,
) -> ProjectRunResult<()> {
    for output in &receipt.outputs {
        let relative = artifact_cas_relative_path(&policy.work_dir, &output.content_digest)
            .map_err(|message| {
                ProjectRunError::new(
                    ProjectRunErrorCode::ReceiptPoisoning,
                    Some(receipt.node_id.clone()),
                    message,
                )
            })?;
        let path = resolve_fs_workspace_path(
            &policy.workspace_root,
            "project_run.artifact_cas",
            &relative,
            PlannedAccess::Read,
        )
        .map(|resolution| resolution.absolute_path)
        .map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::WorkspacePolicy,
                Some(receipt.node_id.clone()),
                format!("artifact CAS path failed workspace safety: {error}"),
            )
        })?;
        if path.exists() {
            read_artifact_cas(policy, output, &receipt.node_id)?;
        }
    }
    Ok(())
}

fn backfill_artifact_cas(
    policy: &ProjectRunPolicy,
    receipt: &ProjectRunNodeReceipt,
) -> ProjectRunResult<bool> {
    let mut outputs = Vec::with_capacity(receipt.outputs.len());
    for output in &receipt.outputs {
        let path = resolve_fs_workspace_path(
            &policy.workspace_root,
            "project_run.receipt_output",
            Path::new(&output.path),
            PlannedAccess::Read,
        )
        .map(|resolution| resolution.absolute_path)
        .map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::WorkspacePolicy,
                Some(receipt.node_id.clone()),
                format!("receipt output path failed workspace safety: {error}"),
            )
        })?;
        let Ok(bytes) = fs::read(path) else {
            return Ok(false);
        };
        if digest_bytes(&bytes) != output.content_digest || bytes.len() as u64 != output.byte_count
        {
            return Ok(false);
        }
        outputs.push((output, bytes));
    }
    for (output, bytes) in outputs {
        write_artifact_cas(
            &policy.workspace_root,
            &policy.work_dir,
            output,
            &bytes,
            &receipt.node_id,
        )?;
    }
    Ok(true)
}

fn read_artifact_cas(
    policy: &ProjectRunPolicy,
    output: &ProjectRunOutputReceipt,
    node_id: &str,
) -> ProjectRunResult<Vec<u8>> {
    let relative = artifact_cas_relative_path(&policy.work_dir, &output.content_digest).map_err(
        |message| {
            ProjectRunError::new(
                ProjectRunErrorCode::ReceiptPoisoning,
                Some(node_id.to_string()),
                message,
            )
        },
    )?;
    let path = resolve_fs_workspace_path(
        &policy.workspace_root,
        "project_run.artifact_cas",
        &relative,
        PlannedAccess::Read,
    )
    .map(|resolution| resolution.absolute_path)
    .map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::WorkspacePolicy,
            Some(node_id.to_string()),
            format!("artifact CAS path failed workspace safety: {error}"),
        )
    })?;
    let bytes = fs::read(&path).map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            Some(node_id.to_string()),
            format!(
                "failed to read content-addressed artifact {} for output {}: {error}",
                path.display(),
                output.output_id
            ),
        )
    })?;
    if digest_bytes(&bytes) != output.content_digest || bytes.len() as u64 != output.byte_count {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            Some(node_id.to_string()),
            format!(
                "content-addressed artifact {} for output {} does not match its receipt digest/count",
                path.display(),
                output.output_id
            ),
        ));
    }
    Ok(bytes)
}

fn artifact_cas_relative_path(work_dir: &Path, content_digest: &str) -> Result<PathBuf, String> {
    let digest_hex = content_digest
        .strip_prefix("blake3:")
        .filter(|hex| {
            hex.len() == 64
                && hex
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
        .ok_or_else(|| {
            "artifact CAS content digest must be a lowercase blake3 digest".to_string()
        })?;
    Ok(normalize_relative_path(work_dir)?
        .join("artifacts")
        .join("cas")
        .join(format!("{digest_hex}.bin")))
}

fn publish_atomic_bytes(
    workspace_root: &Path,
    work_dir: &Path,
    relative_output: &Path,
    bytes: &[u8],
    expected_existing: Option<&ProjectRunOutputReceipt>,
) -> Result<(), String> {
    let relative_output = normalize_relative_path(relative_output)
        .map_err(|message| format!("invalid output path: {message}"))?;
    let final_path = resolve_fs_workspace_path(
        workspace_root,
        "project_run.output",
        &relative_output,
        PlannedAccess::Write,
    )
    .map(|resolution| resolution.absolute_path)
    .map_err(|error| format!("project output path failed workspace safety: {error}"))?;
    let temp_name = format!(
        "{}.{}.tmp",
        path_token(&relative_output),
        digest_bytes(bytes).replace(':', "_")
    );
    let temp_relative = normalize_relative_path(work_dir)?
        .join(".tmp")
        .join(temp_name);
    let temp_path = resolve_fs_workspace_path(
        workspace_root,
        "project_run.output_temp",
        &temp_relative,
        PlannedAccess::Write,
    )
    .map(|resolution| resolution.absolute_path)
    .map_err(|error| format!("project output temp path failed workspace safety: {error}"))?;
    let temp_root = temp_path
        .parent()
        .expect("normalized project output temp path has a parent");
    fs::create_dir_all(temp_root).map_err(|error| {
        format!(
            "failed to create temp root {}: {error}",
            temp_root.display()
        )
    })?;
    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create artifact parent {}: {error}",
                parent.display()
            )
        })?;
    }
    let _publication_lock = acquire_output_publication_lock(&final_path)?;
    prepare_atomic_output_temp(&temp_path, bytes)?;
    finish_atomic_output_publish(&temp_path, &final_path, bytes, expected_existing)
}

struct OutputPublicationLock {
    path: PathBuf,
}

impl Drop for OutputPublicationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_output_publication_lock(final_path: &Path) -> Result<OutputPublicationLock, String> {
    let file_name = final_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact");
    let lock_path = final_path.with_file_name(format!(".{file_name}.publish.lock"));
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(file) => {
            file.sync_all().map_err(|error| {
                let _ = fs::remove_file(&lock_path);
                format!(
                    "failed to sync project artifact publication lock {}: {error}",
                    lock_path.display()
                )
            })?;
            Ok(OutputPublicationLock { path: lock_path })
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(format!(
            "refusing concurrent publication of project artifact {} while lock {} is active; retry after the current publisher completes",
            final_path.display(),
            lock_path.display()
        )),
        Err(error) => Err(format!(
            "failed to create project artifact publication lock {}: {error}",
            lock_path.display()
        )),
    }
}

fn prepare_atomic_output_temp(temp_path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let existing = fs::read(temp_path).map_err(|read_error| {
                format!(
                    "failed to read existing temp artifact {}: {read_error}",
                    temp_path.display()
                )
            })?;
            if existing == bytes {
                return Ok(());
            }
            return Err(format!(
                "refusing to reuse deterministic temp artifact {} because its contents do not match the intended artifact bytes",
                temp_path.display()
            ));
        }
        Err(error) => {
            return Err(format!(
                "failed to create temp artifact {}: {error}",
                temp_path.display()
            ));
        }
    };
    file.write_all(bytes).map_err(|error| {
        let _ = fs::remove_file(temp_path);
        format!(
            "failed to write temp artifact {}: {error}",
            temp_path.display()
        )
    })?;
    file.sync_all().map_err(|error| {
        let _ = fs::remove_file(temp_path);
        format!(
            "failed to sync temp artifact {}: {error}",
            temp_path.display()
        )
    })?;
    Ok(())
}

fn finish_atomic_output_publish(
    temp_path: &Path,
    final_path: &Path,
    bytes: &[u8],
    expected_existing: Option<&ProjectRunOutputReceipt>,
) -> Result<(), String> {
    let intended_digest = digest_bytes(bytes);
    if final_path.exists() {
        let existing = fs::read(final_path).map_err(|error| {
            let _ = fs::remove_file(temp_path);
            format!(
                "failed to read existing artifact {} before replacement: {error}",
                final_path.display()
            )
        })?;
        if digest_bytes(&existing) == intended_digest && existing.len() == bytes.len() {
            let _ = fs::remove_file(temp_path);
            return Ok(());
        }
        let Some(expected) = expected_existing else {
            let _ = fs::remove_file(temp_path);
            return Err(format!(
                "refusing to publish over concurrently created artifact {}",
                final_path.display()
            ));
        };
        if digest_bytes(&existing) != expected.content_digest
            || existing.len() as u64 != expected.byte_count
        {
            let _ = fs::remove_file(temp_path);
            return Err(format!(
                "refusing to replace artifact {} because it no longer matches the recoverable prior receipt",
                final_path.display()
            ));
        }
    }
    fs::rename(temp_path, final_path).map_err(|error| {
        format!(
            "failed to atomically publish {} to {}: {error}",
            temp_path.display(),
            final_path.display()
        )
    })
}

fn outputs_match_receipt(
    workspace_root: &Path,
    receipt: &ProjectRunNodeReceipt,
) -> ProjectRunResult<bool> {
    for output in &receipt.outputs {
        let path = resolve_fs_workspace_path(
            workspace_root,
            "project_run.receipt_output",
            Path::new(&output.path),
            PlannedAccess::Read,
        )
        .map(|resolution| resolution.absolute_path)
        .map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::WorkspacePolicy,
                Some(receipt.node_id.clone()),
                format!("receipt output path failed workspace safety: {error}"),
            )
        })?;
        let Ok(bytes) = fs::read(&path) else {
            return Ok(false);
        };
        if digest_bytes(&bytes) != output.content_digest || bytes.len() as u64 != output.byte_count
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn node_receipt_matches_current(
    plan: &ProjectPlan,
    node: &ProjectPlanNode,
    receipt: &ProjectRunNodeReceipt,
) -> bool {
    receipt.schema_version == CANON_PROJECT_RUN_VERSION
        && receipt.project_id == plan.project_id
        && receipt.node_id == node.node_id
        && receipt.node_cache_key == node.cache.cache_key
        && receipt.outcome == ProjectRunNodeOutcome::Completed
        && receipt.content_hash_inputs == receipt_hash_inputs(&node.content_hash_inputs)
        && receipt_outputs_match_node(node, receipt)
}

fn receipt_outputs_match_node(node: &ProjectPlanNode, receipt: &ProjectRunNodeReceipt) -> bool {
    receipt.outputs.len() == node.outputs.len()
        && node.outputs.iter().all(|planned| {
            receipt
                .outputs
                .iter()
                .any(|actual| actual.output_id == planned.output_id && actual.path == planned.path)
        })
}

fn dependencies_match_receipts(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
    node: &ProjectPlanNode,
    receipt: &ProjectRunNodeReceipt,
    valid_receipts: &BTreeMap<String, ProjectRunNodeReceipt>,
) -> ProjectRunResult<bool> {
    let Ok(expected_semantic) = dependency_semantic_hashes(node, valid_receipts) else {
        return Ok(false);
    };
    if receipt.dependency_semantic_hashes != expected_semantic
        || receipt.dependency_receipt_hashes.len() != node.dependencies.len()
    {
        return Ok(false);
    }
    for dependency_id in &node.dependencies {
        let current = valid_receipts
            .get(dependency_id)
            .expect("dependency semantic hashes require a valid dependency receipt");
        let Some(historical_hash) = receipt.dependency_receipt_hashes.get(dependency_id) else {
            return Ok(false);
        };
        if historical_hash == &current.receipt_hash {
            continue;
        }
        let canonical_relative = receipt_relative_path(policy, dependency_id)?;
        let historical_relative = node_receipt_cas_path(&canonical_relative, historical_hash);
        let historical_path = resolve_fs_workspace_path(
            &policy.workspace_root,
            "project_run.historical_dependency_receipt",
            &historical_relative,
            PlannedAccess::Read,
        )
        .map(|resolution| resolution.absolute_path)
        .map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::WorkspacePolicy,
                Some(node.node_id.clone()),
                format!("historical dependency receipt path failed workspace safety: {error}"),
            )
        })?;
        let historical = read_node_receipt(&historical_path).map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::ReceiptPoisoning,
                Some(node.node_id.clone()),
                format!(
                    "historical dependency receipt {} for {} -> {} could not be validated: {}",
                    historical_path.display(),
                    node.node_id,
                    dependency_id,
                    error.message
                ),
            )
        })?;
        let expected_semantic_hash = receipt
            .dependency_semantic_hashes
            .get(dependency_id)
            .expect("dependency semantic map was validated above");
        if historical.schema_version != CANON_PROJECT_RUN_VERSION
            || historical.project_id != plan.project_id
            || historical.node_id != dependency_id.as_str()
            || historical.outcome != ProjectRunNodeOutcome::Completed
            || historical.receipt_hash != historical_hash.as_str()
            || historical.semantic_hash != expected_semantic_hash.as_str()
        {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ReceiptPoisoning,
                Some(node.node_id.clone()),
                format!(
                    "historical dependency receipt {} does not preserve the recorded project/node/semantic binding for {} -> {}",
                    historical_path.display(),
                    node.node_id,
                    dependency_id
                ),
            ));
        }
    }
    Ok(true)
}

fn dependency_semantic_hashes(
    node: &ProjectPlanNode,
    valid_receipts: &BTreeMap<String, ProjectRunNodeReceipt>,
) -> ProjectRunResult<BTreeMap<String, String>> {
    let mut hashes = BTreeMap::new();
    for dependency in &node.dependencies {
        let receipt = valid_receipts.get(dependency).ok_or_else(|| {
            ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(node.node_id.clone()),
                format!("dependency {dependency} has no completed receipt"),
            )
        })?;
        hashes.insert(dependency.clone(), receipt.semantic_hash.clone());
    }
    Ok(hashes)
}

fn dependency_receipt_hashes(
    node: &ProjectPlanNode,
    valid_receipts: &BTreeMap<String, ProjectRunNodeReceipt>,
) -> ProjectRunResult<BTreeMap<String, String>> {
    let mut hashes = BTreeMap::new();
    for dependency in &node.dependencies {
        let receipt = valid_receipts.get(dependency).ok_or_else(|| {
            ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(node.node_id.clone()),
                format!("dependency {dependency} has no completed receipt"),
            )
        })?;
        hashes.insert(dependency.clone(), receipt.receipt_hash.clone());
    }
    Ok(hashes)
}

fn terminal_receipt(
    plan: &ProjectPlan,
    node: &ProjectPlanNode,
    valid_receipts: &BTreeMap<String, ProjectRunNodeReceipt>,
    outcome: ProjectRunNodeOutcome,
    next_action: ProjectRunNextAction,
    failure_code: &str,
    failure_message: &str,
) -> ProjectRunResult<ProjectRunNodeReceipt> {
    finalized_node_receipt(ProjectRunNodeReceipt {
        schema_version: CANON_PROJECT_RUN_VERSION.to_string(),
        project_id: plan.project_id.clone(),
        plan_graph_hash: plan.graph_hash.clone(),
        node_id: node.node_id.clone(),
        node_cache_key: node.cache.cache_key.clone(),
        content_hash_inputs: receipt_hash_inputs(&node.content_hash_inputs),
        dependency_semantic_hashes: dependency_semantic_hashes(node, valid_receipts)
            .unwrap_or_default(),
        dependency_receipt_hashes: dependency_receipt_hashes(node, valid_receipts)
            .unwrap_or_default(),
        outputs: Vec::new(),
        outcome,
        deterministic_usage: BTreeMap::new(),
        duration_millis: 0,
        resource_observations: BTreeMap::new(),
        next_action,
        failure_code: Some(failure_code.to_string()),
        failure_message: Some(failure_message.to_string()),
        semantic_hash: String::new(),
        telemetry_hash: String::new(),
        receipt_hash: String::new(),
    })
    .map_err(ProjectRunError::from)
}

fn write_receipt(
    policy: &ProjectRunPolicy,
    receipt: &ProjectRunNodeReceipt,
    expected_existing: Option<&ProjectRunNodeReceipt>,
) -> ProjectRunResult<ProjectRunNodeReceipt> {
    let relative = receipt_relative_path(policy, &receipt.node_id)?;
    converge_workspace_receipt(
        policy,
        &relative,
        receipt,
        expected_existing,
        "project receipt",
    )
}

fn converge_workspace_receipt(
    policy: &ProjectRunPolicy,
    relative: &Path,
    receipt: &ProjectRunNodeReceipt,
    expected_existing: Option<&ProjectRunNodeReceipt>,
    logical_field: &str,
) -> ProjectRunResult<ProjectRunNodeReceipt> {
    let path = resolve_receipt_storage_path(
        policy,
        logical_field,
        relative,
        PlannedAccess::Write,
        Some(&receipt.node_id),
    )?;
    converge_node_receipt_in(&path, receipt, expected_existing, |candidate| {
        let cas_relative = node_receipt_cas_path(relative, &candidate.receipt_hash);
        resolve_receipt_storage_path(
            policy,
            "project_run.receipt_cas",
            &cas_relative,
            PlannedAccess::Write,
            Some(&candidate.node_id),
        )
    })
    .map(|publication| publication.receipt)
}

fn preserve_workspace_receipt_cas(
    policy: &ProjectRunPolicy,
    relative: &Path,
    receipt: &ProjectRunNodeReceipt,
    logical_field: &str,
) -> ProjectRunResult<()> {
    let cas_relative = node_receipt_cas_path(relative, &receipt.receipt_hash);
    let cas_path = resolve_receipt_storage_path(
        policy,
        logical_field,
        &cas_relative,
        PlannedAccess::Write,
        Some(&receipt.node_id),
    )?;
    preserve_node_receipt_cas_in(&cas_path, receipt).map_err(ProjectRunError::from)
}

fn validate_receipt_cas_leaf_if_present(
    policy: &ProjectRunPolicy,
    receipt_relative: &Path,
    receipt: &ProjectRunNodeReceipt,
    logical_field: &str,
) -> ProjectRunResult<()> {
    let cas_relative = node_receipt_cas_path(receipt_relative, &receipt.receipt_hash);
    let cas_path = resolve_receipt_storage_path(
        policy,
        logical_field,
        &cas_relative,
        PlannedAccess::Write,
        Some(&receipt.node_id),
    )?;
    if !cas_path.exists() {
        return Ok(());
    }
    let bytes = fs::read(&cas_path).map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            Some(receipt.node_id.clone()),
            format!(
                "failed to read content-addressed project receipt {}: {error}",
                cas_path.display()
            ),
        )
    })?;
    let expected = canonical_node_receipt_bytes(receipt).map_err(ProjectRunError::from)?;
    if bytes != expected {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ReceiptPoisoning,
            Some(receipt.node_id.clone()),
            format!(
                "content-addressed project receipt {} does not match receipt {}",
                cas_path.display(),
                receipt.receipt_hash
            ),
        ));
    }
    Ok(())
}

fn validate_receipt_storage_paths(
    policy: &ProjectRunPolicy,
    node_id: &str,
) -> ProjectRunResult<()> {
    let canonical_relative = receipt_relative_path(policy, node_id)?;
    resolve_receipt_storage_path(
        policy,
        "project receipt path",
        &canonical_relative,
        PlannedAccess::Write,
        Some(node_id),
    )?;
    let canonical_cas = receipt_cas_relative_directory(&canonical_relative)?;
    resolve_receipt_storage_path(
        policy,
        "project receipt CAS path",
        &canonical_cas,
        PlannedAccess::Write,
        Some(node_id),
    )?;

    let receipt_parent = canonical_relative.parent().ok_or_else(|| {
        ProjectRunError::new(
            ProjectRunErrorCode::WorkspacePolicy,
            Some(node_id.to_string()),
            "project receipt path must have a workspace-relative parent",
        )
    })?;
    let semantic_directory = receipt_parent.join("by-cache-key");
    resolve_receipt_storage_path(
        policy,
        "semantic project receipt path",
        &semantic_directory,
        PlannedAccess::Write,
        Some(node_id),
    )?;
    let semantic_cas_directory = semantic_directory.join("cas");
    resolve_receipt_storage_path(
        policy,
        "semantic project receipt CAS path",
        &semantic_cas_directory,
        PlannedAccess::Write,
        Some(node_id),
    )?;
    Ok(())
}

fn receipt_cas_relative_directory(receipt_relative: &Path) -> ProjectRunResult<PathBuf> {
    receipt_relative
        .parent()
        .map(|parent| parent.join("cas"))
        .ok_or_else(|| {
            ProjectRunError::new(
                ProjectRunErrorCode::WorkspacePolicy,
                None,
                "project receipt path must have a workspace-relative parent",
            )
        })
}

fn resolve_receipt_storage_path(
    policy: &ProjectRunPolicy,
    logical_field: &str,
    relative: &Path,
    access: PlannedAccess,
    node_id: Option<&str>,
) -> ProjectRunResult<PathBuf> {
    resolve_fs_workspace_path(&policy.workspace_root, logical_field, relative, access)
        .map(|resolution| resolution.absolute_path)
        .map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::WorkspacePolicy,
                node_id.map(str::to_string),
                format!("{logical_field} failed workspace safety: {error}"),
            )
        })
}

fn receipt_path(policy: &ProjectRunPolicy, node_id: &str) -> ProjectRunResult<PathBuf> {
    let relative = receipt_relative_path(policy, node_id)?;
    resolve_fs_workspace_path(
        &policy.workspace_root,
        "project_run.receipt",
        &relative,
        PlannedAccess::Write,
    )
    .map(|resolution| resolution.absolute_path)
    .map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::WorkspacePolicy,
            Some(node_id.to_string()),
            format!("project receipt path failed workspace safety: {error}"),
        )
    })
}

fn semantic_receipt_path(
    policy: &ProjectRunPolicy,
    node_id: &str,
    result_cache_key: &str,
    access: PlannedAccess,
) -> ProjectRunResult<PathBuf> {
    let relative = semantic_receipt_relative_path(policy, node_id, result_cache_key)?;
    resolve_fs_workspace_path(
        &policy.workspace_root,
        "project_run.semantic_receipt",
        &relative,
        access,
    )
    .map(|resolution| resolution.absolute_path)
    .map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::WorkspacePolicy,
            Some(node_id.to_string()),
            format!("semantic project receipt path failed workspace safety: {error}"),
        )
    })
}

fn semantic_receipt_relative_path(
    policy: &ProjectRunPolicy,
    node_id: &str,
    result_cache_key: &str,
) -> ProjectRunResult<PathBuf> {
    let canonical_relative = receipt_relative_path(policy, node_id)?;
    Ok(semantic_node_receipt_path(
        &canonical_relative,
        result_cache_key,
    ))
}

fn receipt_relative_path(policy: &ProjectRunPolicy, node_id: &str) -> ProjectRunResult<PathBuf> {
    let work_dir = normalize_relative_path(&policy.work_dir).map_err(|message| {
        ProjectRunError::new(ProjectRunErrorCode::WorkspacePolicy, None, message)
    })?;
    Ok(work_dir
        .join("receipts")
        .join(format!("{}.json", node_id_token(node_id))))
}

fn receipt_hash_inputs(inputs: &[ProjectPlanHashRef]) -> Vec<ProjectRunHashRef> {
    let mut refs = inputs
        .iter()
        .map(|input| ProjectRunHashRef {
            ref_id: input.ref_id.clone(),
            content_hash: input.content_hash.clone(),
        })
        .collect::<Vec<_>>();
    refs.sort_by(|left, right| left.ref_id.cmp(&right.ref_id));
    refs
}

fn empty_run_receipt(plan: &ProjectPlan) -> ProjectRunReceipt {
    ProjectRunReceipt {
        schema_version: CANON_PROJECT_RUN_VERSION.to_string(),
        project_id: plan.project_id.clone(),
        plan_graph_hash: plan.graph_hash.clone(),
        receipt_hash: String::new(),
        completed_nodes: Vec::new(),
        failed_nodes: Vec::new(),
        cancelled_nodes: Vec::new(),
        invalidated_nodes: Vec::new(),
        blocked_nodes: Vec::new(),
        node_receipts: Vec::new(),
    }
}

fn node_report(
    node: &ProjectPlanNode,
    outcome: ProjectRunNodeOutcome,
    receipt_hash: Option<&str>,
    reason: Option<&str>,
) -> ProjectRunNodeReport {
    ProjectRunNodeReport {
        node_id: node.node_id.clone(),
        outcome,
        receipt_hash: receipt_hash.map(str::to_string),
        reason: reason.map(str::to_string),
    }
}

fn validate_plan_shape(plan: &ProjectPlan) -> ProjectRunResult<()> {
    if plan.schema_version.trim().is_empty()
        || plan.project_id.trim().is_empty()
        || plan.graph_hash.trim().is_empty()
    {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            None,
            "project plan must declare schema_version, project_id, and graph_hash",
        ));
    }
    let mut ids = BTreeSet::new();
    for node in &plan.nodes {
        if !ids.insert(node.node_id.as_str()) {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(node.node_id.clone()),
                "duplicate project plan node_id",
            ));
        }
    }
    for node in &plan.nodes {
        for dependency in &node.dependencies {
            if !ids.contains(dependency.as_str()) {
                return Err(ProjectRunError::new(
                    ProjectRunErrorCode::ArtifactContract,
                    Some(node.node_id.clone()),
                    format!("dependency {dependency} has no node"),
                ));
            }
        }
        let expected_cache_key = project_plan_node_cache_key(node).map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(node.node_id.clone()),
                format!(
                    "project plan node cache key could not be validated: {}",
                    error.message
                ),
            )
        })?;
        if node.cache.cache_key != expected_cache_key {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ArtifactContract,
                Some(node.node_id.clone()),
                format!(
                    "project plan node cache key must bind command, declared side effects, refusal conditions, inputs, outputs, and limits: expected {expected_cache_key}, got {}",
                    node.cache.cache_key
                ),
            ));
        }
    }
    Ok(())
}

fn ensure_declared_node_effects_allowed(
    node: &ProjectPlanNode,
    policy: &ProjectRunPolicy,
) -> ProjectRunResult<()> {
    let extension_policy = ProjectExtensionNodePolicy {
        allow_network: policy.allow_network,
        allow_registry_mutation: policy.allow_mutation_gates,
    };
    // This validates declared effects before dispatch; it is not a sandbox for executor code.
    validate_extension_node_effects(node, &extension_policy).map_err(|error| {
        ProjectRunError::new(
            ProjectRunErrorCode::WorkspacePolicy,
            Some(node.node_id.clone()),
            format!(
                "declared node effects exceed project run policy: {}",
                plan_error_details(&error)
            ),
        )
    })
}

fn plan_error_details(error: &ProjectPlanError) -> String {
    if error.diagnostics.is_empty() {
        return error.message.clone();
    }
    error
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>()
        .join("; ")
}

fn finalize_policy(policy: &ProjectRunPolicy) -> ProjectRunResult<ProjectRunPolicy> {
    if policy.max_parallelism == 0 {
        return Err(ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            None,
            "project run max_parallelism must be greater than zero",
        ));
    }
    normalize_relative_path(&policy.work_dir).map_err(|message| {
        ProjectRunError::new(ProjectRunErrorCode::WorkspacePolicy, None, message)
    })?;
    Ok(policy.clone())
}

fn normalize_relative_path(path: &Path) -> Result<PathBuf, String> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => normalized.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "path must stay relative to the project workspace: {}",
                    path.display()
                ));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err("path must contain at least one relative segment".to_string());
    }
    Ok(normalized)
}

fn node_id_token(node_id: &str) -> String {
    node_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn path_token(path: &Path) -> String {
    node_id_token(&path.to_string_lossy())
}

fn sort_report(report: &mut ProjectRunReport) {
    report.executed_nodes.sort();
    report.resumed_nodes.sort();
    report.failed_nodes.sort();
    report.cancelled_nodes.sort();
    report.invalidated_nodes.sort();
    report.blocked_nodes.sort();
    report
        .node_reports
        .sort_by(|left, right| left.node_id.cmp(&right.node_id));
}
