#![forbid(unsafe_code)]

use super::{
    plan::{
        ProjectExtensionNodePolicy, ProjectPlan, ProjectPlanCacheDecision, ProjectPlanError,
        ProjectPlanHashRef, ProjectPlanNode, ProjectPlanNodeClass, project_plan_node_cache_key,
        validate_extension_node_effects,
    },
    receipt::{
        CANON_PROJECT_RUN_VERSION, ProjectReceiptError, ProjectRunHashRef, ProjectRunNextAction,
        ProjectRunNodeOutcome, ProjectRunNodeReceipt, ProjectRunOutputReceipt, ProjectRunReceipt,
        canonical_run_receipt_bytes, digest_bytes, finalized_node_receipt, finalized_run_receipt,
        read_node_receipt, replace_node_receipt,
    },
};
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

pub fn run_project_plan<E: ProjectNodeExecutor>(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
    executor: &mut E,
) -> ProjectRunResult<ProjectRunReport> {
    validate_plan_shape(plan)?;
    let policy = finalize_policy(policy)?;
    let target_nodes = selected_node_closure(plan, &policy.selected_nodes)?;
    let existing = load_existing_receipts(plan, &policy)?;
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
                write_receipt(&policy, &receipt, prior_receipts.get(&node.node_id))?;
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
                executor,
            ) {
                Ok(receipt) => {
                    write_receipt(&policy, &receipt, prior_receipts.get(&node.node_id))?;
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
                    let receipt = terminal_receipt(
                        plan,
                        node,
                        &valid_receipts,
                        ProjectRunNodeOutcome::Failed,
                        ProjectRunNextAction::InspectFailure,
                        &format!("{:?}", error.code),
                        &error.message,
                    )?;
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
                        return finish_report(
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

    finish_report(
        plan,
        &policy,
        &target_nodes,
        report,
        valid_receipts,
        failed_nodes,
    )
}

pub fn inspect_project_run_reuse_only(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
) -> ProjectRunResult<ProjectRunReport> {
    validate_plan_shape(plan)?;
    let policy = finalize_policy(policy)?;
    let target_nodes = selected_node_closure(plan, &policy.selected_nodes)?;
    let existing = load_existing_receipts(plan, &policy)?;
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

    let report = ProjectRunReport {
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

fn execute_node<E: ProjectNodeExecutor>(
    plan: &ProjectPlan,
    node: &ProjectPlanNode,
    policy: &ProjectRunPolicy,
    valid_receipts: &BTreeMap<String, ProjectRunNodeReceipt>,
    completed_receipts: &BTreeMap<String, ProjectRunNodeReceipt>,
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
    let context = ProjectNodeExecutionContext {
        node_id: node.node_id.clone(),
        dependency_semantic_hashes: dependency_semantic_hashes.clone(),
    };
    let result = executor.execute(node, &context)?;
    let output_receipts = publish_outputs(
        node,
        &policy.workspace_root,
        &policy.work_dir,
        stale_receipt,
        result.outputs,
    )?;
    finalized_node_receipt(ProjectRunNodeReceipt {
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
    .map_err(ProjectRunError::from)
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
    invalidated_nodes: BTreeSet<String>,
    poisoned_receipts: Vec<String>,
}

fn load_existing_receipts(
    plan: &ProjectPlan,
    policy: &ProjectRunPolicy,
) -> ProjectRunResult<ExistingReceipts> {
    let mut existing = ExistingReceipts::default();
    for node in &plan.nodes {
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
        if node_receipt_matches_current(plan, node, &receipt)
            && outputs_match_receipt(&policy.workspace_root, &receipt)?
            && dependencies_match_receipts(node, &receipt, &existing.valid_receipts)
        {
            existing
                .valid_receipts
                .insert(node.node_id.clone(), receipt);
        } else {
            existing.invalidated_nodes.insert(node.node_id.clone());
        }
    }
    Ok(existing)
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

fn publish_outputs(
    node: &ProjectPlanNode,
    workspace_root: &Path,
    work_dir: &Path,
    stale_receipt: Option<&ProjectRunNodeReceipt>,
    outputs: BTreeMap<String, Vec<u8>>,
) -> ProjectRunResult<Vec<ProjectRunOutputReceipt>> {
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
        let stale_output = stale_receipt.and_then(|receipt| {
            receipt.outputs.iter().find(|candidate| {
                candidate.output_id == output.output_id && candidate.path == output.path
            })
        });
        publish_atomic_bytes(
            workspace_root,
            work_dir,
            Path::new(&output.path),
            bytes,
            stale_output,
        )
        .map_err(|error| {
            ProjectRunError::new(
                ProjectRunErrorCode::AtomicPublication,
                Some(node.node_id.clone()),
                error,
            )
        })?;
        receipts.push(ProjectRunOutputReceipt {
            output_id: output.output_id.clone(),
            path: output.path.clone(),
            content_digest: digest_bytes(bytes),
            byte_count: bytes.len() as u64,
        });
    }
    Ok(receipts)
}

fn ensure_outputs_publishable(
    node: &ProjectPlanNode,
    workspace_root: &Path,
    stale_receipt: Option<&ProjectRunNodeReceipt>,
) -> ProjectRunResult<()> {
    for output in &node.outputs {
        let relative = normalize_relative_path(Path::new(&output.path)).map_err(|message| {
            ProjectRunError::new(
                ProjectRunErrorCode::WorkspacePolicy,
                Some(node.node_id.clone()),
                message,
            )
        })?;
        let path = workspace_root.join(relative);
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

fn publish_atomic_bytes(
    workspace_root: &Path,
    work_dir: &Path,
    relative_output: &Path,
    bytes: &[u8],
    expected_existing: Option<&ProjectRunOutputReceipt>,
) -> Result<(), String> {
    let relative_output = normalize_relative_path(relative_output)
        .map_err(|message| format!("invalid output path: {message}"))?;
    let final_path = workspace_root.join(&relative_output);
    let temp_root = workspace_root
        .join(normalize_relative_path(work_dir)?)
        .join(".tmp");
    fs::create_dir_all(&temp_root).map_err(|error| {
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
    let temp_name = format!(
        "{}.{}.tmp",
        path_token(&relative_output),
        digest_bytes(bytes).replace(':', "_")
    );
    let temp_path = temp_root.join(temp_name);
    prepare_atomic_output_temp(&temp_path, bytes)?;
    finish_atomic_output_publish(&temp_path, &final_path, bytes, expected_existing)
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
        let relative = normalize_relative_path(Path::new(&output.path)).map_err(|message| {
            ProjectRunError::new(
                ProjectRunErrorCode::WorkspacePolicy,
                Some(receipt.node_id.clone()),
                message,
            )
        })?;
        let path = workspace_root.join(relative);
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
}

fn dependencies_match_receipts(
    node: &ProjectPlanNode,
    receipt: &ProjectRunNodeReceipt,
    valid_receipts: &BTreeMap<String, ProjectRunNodeReceipt>,
) -> bool {
    let Ok(expected) = dependency_semantic_hashes(node, valid_receipts) else {
        return false;
    };
    receipt.dependency_semantic_hashes == expected
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
) -> ProjectRunResult<()> {
    let path = receipt_path(policy, &receipt.node_id)?;
    replace_node_receipt(&path, receipt, expected_existing).map_err(ProjectRunError::from)
}

fn receipt_path(policy: &ProjectRunPolicy, node_id: &str) -> ProjectRunResult<PathBuf> {
    let work_dir = normalize_relative_path(&policy.work_dir).map_err(|message| {
        ProjectRunError::new(ProjectRunErrorCode::WorkspacePolicy, None, message)
    })?;
    Ok(policy
        .workspace_root
        .join(work_dir)
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
