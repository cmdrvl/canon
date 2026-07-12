#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../src/project/lock.rs"]
mod lock;
#[allow(dead_code)]
#[path = "../src/project/manifest.rs"]
mod manifest;
#[allow(dead_code)]
#[path = "../src/project/plan.rs"]
mod plan;
#[allow(dead_code)]
#[path = "../src/project/receipt.rs"]
mod receipt;
#[allow(dead_code)]
#[path = "../src/project/run.rs"]
mod run;

use lock::{
    ProjectLock, ProjectLockInput, ProjectLockManifestProjection, ProjectLockRefKind,
    ProjectLockResolvedRef, digest_bytes, refresh_project_lock,
};
use manifest::{
    ProjectManifest, ProjectPackageKind, load_project_manifest_toml, project_manifest_digest,
};
use plan::{ProjectPlan, ProjectPlanNode, ProjectPlanRequest, compile_project_plan};
use receipt::project_run_schema_version;
use run::{
    ProjectNodeExecutionContext, ProjectNodeExecutionResult, ProjectNodeExecutor, ProjectRunError,
    ProjectRunErrorCode, ProjectRunFailurePolicy, ProjectRunPolicy,
    canonical_project_run_report_bytes, run_project_plan,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const SCHEMA_JSON: &str = include_str!("../schemas/canon.project.run.v1.schema.json");
const MINIMAL_TOML: &str = include_str!("./fixtures/project/minimal.toml");

#[test]
fn schema_declares_content_validated_resume_contract() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], project_run_schema_version());
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        project_run_schema_version()
    );
    assert_eq!(schema["x-canon-contract"]["content_validated_resume"], true);
    assert_eq!(
        schema["x-canon-contract"]["existence_is_not_completion"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["hash_linked_node_receipts"],
        true
    );
}

#[test]
fn completed_nodes_resume_without_repeating_execution() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = minimal_plan();
    let policy = approving_policy(temp.path());
    let mut executor = DeterministicExecutor::default();

    let first = run_project_plan(&plan, &policy, &mut executor).expect("first run");
    assert_eq!(first.executed_nodes.len(), plan.nodes.len());
    assert_eq!(executor.calls.len(), plan.nodes.len());
    assert_eq!(first.failed_nodes.len(), 0);
    assert_eq!(first.cancelled_nodes.len(), 0);

    let first_tree = tree_bytes(temp.path());
    let mut second_executor = DeterministicExecutor::default();
    let second = run_project_plan(&plan, &policy, &mut second_executor).expect("resume run");
    assert_eq!(second.executed_nodes.len(), 0);
    assert_eq!(second.resumed_nodes.len(), plan.nodes.len());
    assert!(second_executor.calls.is_empty());
    assert_eq!(first_tree, tree_bytes(temp.path()));
    assert_eq!(
        canonical_project_run_report_bytes(&second).expect("report bytes"),
        canonical_project_run_report_bytes(&second).expect("report bytes stable")
    );
}

#[test]
fn interrupted_run_resumes_to_same_bytes_as_uninterrupted_run() {
    let uninterrupted_dir = tempfile::tempdir().expect("uninterrupted");
    let resumed_dir = tempfile::tempdir().expect("resumed");
    let plan = minimal_plan();

    let mut uninterrupted_executor = DeterministicExecutor::default();
    let uninterrupted = run_project_plan(
        &plan,
        &approving_policy(uninterrupted_dir.path()),
        &mut uninterrupted_executor,
    )
    .expect("uninterrupted run");
    assert_eq!(uninterrupted.executed_nodes.len(), plan.nodes.len());

    let mut partial_policy = approving_policy(resumed_dir.path());
    partial_policy
        .cancel_before_nodes
        .insert("block.cluster_default".to_string());
    let mut partial_executor = DeterministicExecutor::default();
    let partial = run_project_plan(&plan, &partial_policy, &mut partial_executor)
        .expect("partial run records cancellation");
    assert!(
        partial
            .cancelled_nodes
            .contains(&"block.cluster_default".to_string())
    );
    assert!(!artifact_path(resumed_dir.path(), &plan, "block.cluster_default").exists());
    let cancelled_receipt = fs::read(receipt_path(resumed_dir.path(), "block.cluster_default"))
        .expect("cancelled receipt exists");
    let cancelled_json: Value =
        serde_json::from_slice(&cancelled_receipt).expect("cancelled receipt json");
    assert_eq!(cancelled_json["outcome"], "cancelled");

    let mut resume_executor = DeterministicExecutor::default();
    let resumed = run_project_plan(
        &plan,
        &approving_policy(resumed_dir.path()),
        &mut resume_executor,
    )
    .expect("resume completes");
    assert_eq!(resumed.failed_nodes.len(), 0);
    assert_eq!(resumed.cancelled_nodes.len(), 0);
    assert!(
        resumed
            .resumed_nodes
            .contains(&"intake.source_alpha".to_string())
    );
    assert_eq!(
        without_cancel_receipts(tree_bytes(resumed_dir.path())),
        tree_bytes(uninterrupted_dir.path())
    );
}

#[test]
fn changed_node_inputs_invalidate_that_node_and_descendants() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut plan = minimal_plan();
    let policy = approving_policy(temp.path());
    let mut executor = DeterministicExecutor::default();
    run_project_plan(&plan, &policy, &mut executor).expect("initial run");

    let intake = plan
        .nodes
        .iter_mut()
        .find(|node| node.node_id == "intake.source_alpha")
        .expect("intake node");
    intake.content_hash_inputs[0].content_hash = digest_bytes(b"changed-source");
    intake.cache.cache_key = digest_bytes(b"changed-intake-cache-key");

    let mut resume_executor = DeterministicExecutor::default();
    let resumed = run_project_plan(&plan, &policy, &mut resume_executor)
        .expect("changed plan produces invalidation report");
    assert!(
        resumed
            .invalidated_nodes
            .contains(&"intake.source_alpha".to_string())
    );
    assert!(
        resumed
            .invalidated_nodes
            .contains(&"normalize.source_alpha".to_string())
    );
    assert!(
        resumed
            .invalidated_nodes
            .contains(&"export.cluster_default.summary".to_string())
    );
    assert!(resume_executor.calls.is_empty());
}

#[test]
fn failed_node_writes_failure_receipt_but_no_complete_artifact() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = minimal_plan();
    let mut executor = DeterministicExecutor::default();
    executor
        .fail_nodes
        .insert("block.cluster_default".to_string());

    let report = run_project_plan(&plan, &approving_policy(temp.path()), &mut executor)
        .expect("failure report");
    assert!(
        report
            .failed_nodes
            .contains(&"block.cluster_default".to_string())
    );
    assert!(!artifact_path(temp.path(), &plan, "block.cluster_default").exists());
    let receipt_bytes = fs::read(receipt_path(temp.path(), "block.cluster_default"))
        .expect("failure receipt exists");
    let receipt_json: Value = serde_json::from_slice(&receipt_bytes).expect("receipt json");
    assert_eq!(receipt_json["outcome"], "failed");
    assert!(receipt_json["outputs"].as_array().is_none_or(Vec::is_empty));
}

#[test]
fn poisoned_receipts_refuse_before_execution() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = minimal_plan();
    let policy = approving_policy(temp.path());
    let mut executor = DeterministicExecutor::default();
    run_project_plan(&plan, &policy, &mut executor).expect("initial run");

    fs::write(
        receipt_path(temp.path(), "intake.source_alpha"),
        b"{not-json",
    )
    .expect("poison receipt");
    let mut second_executor = DeterministicExecutor::default();
    let error = run_project_plan(&plan, &policy, &mut second_executor)
        .expect_err("poisoned receipt refuses");
    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert!(second_executor.calls.is_empty());
}

#[test]
fn mutation_gates_require_explicit_approval() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = minimal_plan();
    let mut executor = DeterministicExecutor::default();
    let policy = ProjectRunPolicy::new(temp.path(), "work");
    let report = run_project_plan(&plan, &policy, &mut executor).expect("run blocks at mutation");
    assert!(
        !report
            .executed_nodes
            .contains(&"promote.cluster_default".to_string())
    );
    assert!(
        report
            .blocked_nodes
            .contains(&"promote.cluster_default".to_string())
    );
    assert!(report.failed_nodes.is_empty());
}

#[test]
fn bounded_parallelism_records_ready_width_for_independent_nodes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = independent_plan();
    let mut policy = approving_policy(temp.path());
    policy.max_parallelism = 1;
    let mut executor = DeterministicExecutor::default();
    let report = run_project_plan(&plan, &policy, &mut executor).expect("independent run");
    assert_eq!(report.max_parallelism, 1);
    assert_eq!(report.max_ready_width, 2);
    assert_eq!(
        report.executed_nodes,
        vec!["alpha".to_string(), "beta".to_string()]
    );
}

#[derive(Default)]
struct DeterministicExecutor {
    calls: Vec<String>,
    fail_nodes: BTreeSet<String>,
}

impl ProjectNodeExecutor for DeterministicExecutor {
    fn execute(
        &mut self,
        node: &ProjectPlanNode,
        context: &ProjectNodeExecutionContext,
    ) -> Result<ProjectNodeExecutionResult, ProjectRunError> {
        self.calls.push(node.node_id.clone());
        if self.fail_nodes.contains(&node.node_id) {
            return Err(ProjectRunError::new(
                ProjectRunErrorCode::ExecutionFailed,
                Some(node.node_id.clone()),
                "injected node failure",
            ));
        }
        let mut outputs = BTreeMap::new();
        for output in &node.outputs {
            outputs.insert(
                output.output_id.clone(),
                format!(
                    "node={}\noutput={}\ncache={}\ndeps={:?}\n",
                    node.node_id,
                    output.output_id,
                    node.cache.cache_key,
                    context.dependency_receipt_hashes
                )
                .into_bytes(),
            );
        }
        let mut result = ProjectNodeExecutionResult::with_outputs(outputs);
        result.duration_millis = node.node_id.len() as u64;
        result
            .resource_observations
            .insert("output_count".to_string(), node.outputs.len() as u64);
        Ok(result)
    }
}

fn minimal_plan() -> ProjectPlan {
    let manifest = minimal_manifest();
    let lock = lock_for_manifest(&manifest);
    let mut request = ProjectPlanRequest::new(
        manifest,
        lock,
        PathBuf::from("tests/fixtures/project/minimal.toml"),
        PathBuf::from("tests/fixtures/project/minimal.lock.json"),
    );
    request.plan_artifact_path = Some(PathBuf::from("work/plan.json"));
    compile_project_plan(request).expect("project plan compiles")
}

fn minimal_manifest() -> ProjectManifest {
    load_project_manifest_toml(MINIMAL_TOML).expect("minimal manifest loads")
}

fn lock_for_manifest(manifest: &ProjectManifest) -> ProjectLock {
    refresh_project_lock(&ProjectLockManifestProjection {
        project_id: manifest.project_id.clone(),
        project_digest: project_manifest_digest(manifest).expect("manifest digest"),
        inputs: manifest
            .sources
            .iter()
            .map(|source| ProjectLockInput {
                input_id: source.source_id.clone(),
                relative_path: source.path.clone(),
                content_digest: digest_bytes(source.path.as_bytes()),
            })
            .collect(),
        resolved_refs: manifest
            .packages
            .iter()
            .map(|package| ProjectLockResolvedRef {
                ref_id: package.alias.clone(),
                kind: match package.kind {
                    ProjectPackageKind::Strategy => ProjectLockRefKind::Strategy,
                    ProjectPackageKind::Registry
                    | ProjectPackageKind::EntityProfile
                    | ProjectPackageKind::SourceMapping
                    | ProjectPackageKind::Extension => ProjectLockRefKind::Package,
                },
                resolved_digest: package.content_hash.clone(),
            })
            .collect(),
    })
    .expect("lock builds")
}

fn approving_policy(root: &Path) -> ProjectRunPolicy {
    let mut policy = ProjectRunPolicy::new(root, "work");
    policy.allow_mutation_gates = true;
    policy.failure_policy = ProjectRunFailurePolicy::CollectIndependentFailures;
    policy
}

fn receipt_path(root: &Path, node_id: &str) -> PathBuf {
    root.join("work")
        .join("receipts")
        .join(format!("{}.json", node_id.replace('.', "_")))
}

fn artifact_path(root: &Path, plan: &ProjectPlan, node_id: &str) -> PathBuf {
    let node = plan
        .nodes
        .iter()
        .find(|node| node.node_id == node_id)
        .expect("node exists");
    root.join(&node.outputs[0].path)
}

fn tree_bytes(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).expect("read dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root)
                        .expect("strip root")
                        .to_string_lossy()
                        .replace('\\', "/"),
                    fs::read(&path).expect("read file"),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn without_cancel_receipts(tree: BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    tree.into_iter()
        .filter(|(path, bytes)| {
            if path != "work/receipts/block_cluster_default.json" {
                return true;
            }
            let value: Value = serde_json::from_slice(bytes).expect("receipt json");
            value["outcome"] != "cancelled"
        })
        .collect()
}

fn independent_plan() -> ProjectPlan {
    let mut plan = minimal_plan();
    let alpha = plan.nodes[0].clone();
    let mut beta = alpha.clone();
    let mut alpha = alpha;
    alpha.node_id = "alpha".to_string();
    alpha.dependencies.clear();
    alpha.outputs[0].output_id = "alpha.output".to_string();
    alpha.outputs[0].path = "work/alpha.json".to_string();
    beta.node_id = "beta".to_string();
    beta.dependencies.clear();
    beta.outputs[0].output_id = "beta.output".to_string();
    beta.outputs[0].path = "work/beta.json".to_string();
    beta.cache.cache_key = digest_bytes(b"beta-cache");
    plan.nodes = vec![alpha, beta];
    plan.graph_hash = digest_bytes(b"independent-plan");
    plan.summary.total_nodes = 2;
    plan.summary.edge_count = 0;
    plan.summary.runnable_nodes = 2;
    plan.summary.blocked_nodes = 0;
    plan
}
