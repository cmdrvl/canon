#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../src/fs_safety.rs"]
mod fs_safety;
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
use plan::{
    ProjectPlan, ProjectPlanErrorCode, ProjectPlanHashRef, ProjectPlanNode, ProjectPlanNodeClass,
    ProjectPlanRefusalCondition, ProjectPlanRequest, ProjectPlanSideEffect,
    ProjectPlanSideEffectKind, compile_project_plan, project_plan_node_cache_key,
};
use receipt::{
    ProjectReceiptErrorCode, ProjectRunNodeOutcome, canonical_node_receipt_bytes,
    finalized_node_receipt, node_receipt_cas_path, parse_node_receipt, project_run_schema_version,
    read_node_receipt, replace_node_receipt, semantic_node_receipt_path,
    semantic_node_result_cache_key, write_node_receipt,
};
use run::{
    CANON_PROJECT_RUN_MANIFEST_REVISION_VERSION, PROJECT_INTERNAL_COPY_FILE_EXECUTOR,
    ProjectNodeExecutionContext, ProjectNodeExecutionResult, ProjectNodeExecutor, ProjectRunError,
    ProjectRunErrorCode, ProjectRunFailurePolicy, ProjectRunPolicy,
    canonical_project_run_manifest_revision_bytes, canonical_project_run_report_bytes,
    inspect_project_run_reuse_only, project_run_manifest_head_path,
    project_run_manifest_revision_for_report, project_run_manifest_revision_path,
    read_project_run_manifest_head, run_project_plan, run_project_plan_with_registered_executors,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const SCHEMA_JSON: &str = include_str!("../schemas/canon.project.run.v2.schema.json");
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
    assert_eq!(
        schema["x-canon-contract"]["semantic_dependency_identity"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["telemetry_integrity_separated"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["deterministic_usage_hash_bound"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["runtime_telemetry_not_dependency_identity"],
        true
    );
    let node_receipt = &schema["$defs"]["node_receipt"];
    let required = node_receipt["required"]
        .as_array()
        .expect("node receipt required array");
    for field in ["semantic_hash", "telemetry_hash", "receipt_hash"] {
        assert!(
            required.iter().any(|value| value.as_str() == Some(field)),
            "node receipt schema must require {field}"
        );
    }
    let properties = &node_receipt["properties"];
    for field in [
        "dependency_semantic_hashes",
        "dependency_receipt_hashes",
        "deterministic_usage",
        "semantic_hash",
        "telemetry_hash",
    ] {
        assert!(
            properties[field].is_object(),
            "node receipt schema must declare {field}"
        );
    }
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
    assert_eq!(
        without_run_manifest(first_tree),
        without_run_manifest(tree_bytes(temp.path()))
    );
    assert_eq!(
        canonical_project_run_report_bytes(&second).expect("report bytes"),
        canonical_project_run_report_bytes(&second).expect("report bytes stable")
    );
}

#[test]
fn project_run_publishes_immutable_manifest_revision_lineage() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan_a = chain_plan();
    let policy = approving_policy(temp.path());
    let mut executor_a = DeterministicExecutor::default();

    let report_a = run_project_plan(&plan_a, &policy, &mut executor_a).expect("revision A runs");
    let head_a = read_project_run_manifest_head(&policy)
        .expect("manifest head reads")
        .expect("manifest head exists");
    let head_a_bytes =
        canonical_project_run_manifest_revision_bytes(&head_a).expect("revision A bytes");
    let head_a_path = project_run_manifest_head_path(&policy).expect("head path");
    let revision_a_path =
        project_run_manifest_revision_path(&policy, &head_a.revision_hash).expect("A path");

    assert_eq!(
        head_a.schema_version,
        CANON_PROJECT_RUN_MANIFEST_REVISION_VERSION
    );
    assert_eq!(head_a.previous_revision_hash, None);
    assert_eq!(head_a.project_id, plan_a.project_id);
    assert_eq!(head_a.plan_graph_hash, plan_a.graph_hash);
    assert_eq!(head_a.run_receipt_hash, report_a.run_receipt_hash);
    assert_eq!(
        head_a.validated_nodes,
        vec!["alpha".to_string(), "beta".to_string()]
    );
    assert_eq!(
        head_a.completed_nodes,
        vec!["alpha".to_string(), "beta".to_string()]
    );
    assert_eq!(fs::read(&head_a_path).expect("head bytes"), head_a_bytes);
    assert_eq!(
        fs::read(&revision_a_path).expect("immutable A bytes"),
        head_a_bytes
    );
    for receipt in &report_a.receipt.node_receipts {
        assert_eq!(
            head_a.node_receipt_hashes.get(&receipt.node_id),
            Some(&receipt.receipt_hash)
        );
        assert_eq!(
            head_a.node_semantic_hashes.get(&receipt.node_id),
            Some(&receipt.semantic_hash)
        );
    }

    let mut plan_b = plan_a.clone();
    change_alpha_identity(&mut plan_b);
    let mut executor_b = DeterministicExecutor::default();
    let report_b = run_project_plan(&plan_b, &policy, &mut executor_b).expect("revision B runs");
    let head_b = read_project_run_manifest_head(&policy)
        .expect("manifest head B reads")
        .expect("manifest head B exists");

    assert_ne!(head_b.revision_hash, head_a.revision_hash);
    assert_eq!(
        head_b.previous_revision_hash,
        Some(head_a.revision_hash.clone())
    );
    assert_eq!(head_b.plan_graph_hash, plan_b.graph_hash);
    assert_eq!(head_b.run_receipt_hash, report_b.run_receipt_hash);
    assert_eq!(
        fs::read(&revision_a_path).expect("immutable A still present"),
        head_a_bytes
    );
    assert_eq!(
        fs::read(
            project_run_manifest_revision_path(&policy, &head_b.revision_hash).expect("B path")
        )
        .expect("immutable B bytes"),
        canonical_project_run_manifest_revision_bytes(&head_b).expect("revision B bytes")
    );
}

#[test]
fn selected_run_validates_poisoned_unselected_receipt_before_execution() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = independent_plan();
    let policy = approving_policy(temp.path());
    let mut executor = DeterministicExecutor::default();
    run_project_plan(&plan, &policy, &mut executor).expect("initial run");
    fs::write(receipt_path(temp.path(), "beta"), b"{poisoned-beta-receipt")
        .expect("poison unselected beta receipt");
    let alpha_before = fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("alpha before");

    let mut selected_policy = policy.clone();
    selected_policy.selected_nodes.insert("alpha".to_string());
    let mut selected_executor = DeterministicExecutor::default();
    let error = run_project_plan(&plan, &selected_policy, &mut selected_executor)
        .expect_err("full-plan receipt validation refuses poisoned beta");

    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert!(error.message.contains("poisoned project receipts"));
    assert!(selected_executor.calls.is_empty());
    assert_eq!(
        fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("alpha after"),
        alpha_before
    );
}

#[test]
fn manifest_matching_temp_and_lock_are_recovered_on_retry() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_node_plan();
    let policy = approving_policy(temp.path());
    let mut executor = DeterministicExecutor::default();
    run_project_plan(&plan, &policy, &mut executor).expect("initial run");
    let previous_head = read_project_run_manifest_head(&policy)
        .expect("previous head reads")
        .expect("previous head exists");
    let inspected =
        inspect_project_run_reuse_only(&plan, &policy).expect("reuse-only report builds");
    let expected_revision = project_run_manifest_revision_for_report(
        &plan,
        &inspected,
        Some(previous_head.revision_hash.clone()),
    )
    .expect("expected retry revision");
    let expected_bytes = canonical_project_run_manifest_revision_bytes(&expected_revision)
        .expect("expected revision bytes");
    let head_path = project_run_manifest_head_path(&policy).expect("head path");
    let temp_path = manifest_temp_path_for_test(&head_path, &expected_bytes);
    fs::write(&temp_path, &expected_bytes).expect("stale matching manifest temp");
    let lock_path = publication_lock_path_for_test(&head_path);
    fs::write(&lock_path, b"stale matching manifest publication").expect("stale manifest lock");

    let mut retry_executor = DeterministicExecutor::default();
    run_project_plan(&plan, &policy, &mut retry_executor).expect("retry recovers manifest temp");

    assert!(retry_executor.calls.is_empty());
    assert_eq!(fs::read(&head_path).expect("manifest head"), expected_bytes);
    assert_eq!(
        read_project_run_manifest_head(&policy)
            .expect("head reads after recovery")
            .expect("head exists after recovery")
            .revision_hash,
        expected_revision.revision_hash
    );
    assert!(!temp_path.exists());
    assert!(!lock_path.exists());
}

#[test]
fn manifest_mismatched_temp_refuses_without_replacing_head() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_node_plan();
    let policy = approving_policy(temp.path());
    let mut executor = DeterministicExecutor::default();
    run_project_plan(&plan, &policy, &mut executor).expect("initial run");
    let previous_head = read_project_run_manifest_head(&policy)
        .expect("previous head reads")
        .expect("previous head exists");
    let previous_bytes =
        canonical_project_run_manifest_revision_bytes(&previous_head).expect("previous bytes");
    let inspected =
        inspect_project_run_reuse_only(&plan, &policy).expect("reuse-only report builds");
    let expected_revision = project_run_manifest_revision_for_report(
        &plan,
        &inspected,
        Some(previous_head.revision_hash.clone()),
    )
    .expect("expected retry revision");
    let expected_bytes = canonical_project_run_manifest_revision_bytes(&expected_revision)
        .expect("expected revision bytes");
    let head_path = project_run_manifest_head_path(&policy).expect("head path");
    let temp_path = manifest_temp_path_for_test(&head_path, &expected_bytes);
    fs::write(&temp_path, b"wrong-manifest-revision").expect("stale mismatched temp");

    let mut retry_executor = DeterministicExecutor::default();
    let error = run_project_plan(&plan, &policy, &mut retry_executor)
        .expect_err("mismatched manifest temp refuses");

    assert_eq!(error.code, ProjectRunErrorCode::AtomicPublication);
    assert!(
        error
            .message
            .contains("refusing to reuse atomic project run manifest temp")
    );
    assert!(retry_executor.calls.is_empty());
    assert_eq!(
        fs::read(&head_path).expect("head unchanged"),
        previous_bytes
    );
    assert_eq!(
        fs::read(&temp_path).expect("mismatched temp preserved"),
        b"wrong-manifest-revision"
    );
}

#[test]
fn manifest_head_rejects_unknown_fields() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_node_plan();
    let policy = approving_policy(temp.path());
    let mut executor = DeterministicExecutor::default();
    run_project_plan(&plan, &policy, &mut executor).expect("initial run");
    let head_path = project_run_manifest_head_path(&policy).expect("head path");
    let mut value: Value =
        serde_json::from_slice(&fs::read(&head_path).expect("head bytes")).expect("head json");
    value["unexpected"] = Value::from(true);
    fs::write(
        &head_path,
        serde_json::to_vec(&value).expect("bad head bytes"),
    )
    .expect("write bad head");

    let error =
        read_project_run_manifest_head(&policy).expect_err("unknown manifest field refuses");

    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert!(error.message.contains("unknown field"));
}

#[test]
fn immutable_revision_a_is_restored_after_b_without_execution() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan_a = chain_plan();
    let policy = approving_policy(temp.path());
    let mut executor_a = DeterministicExecutor::default();
    run_project_plan(&plan_a, &policy, &mut executor_a).expect("revision A runs");
    let a_bytes = ["alpha", "beta"]
        .into_iter()
        .map(|node_id| {
            (
                node_id,
                fs::read(artifact_path(temp.path(), &plan_a, node_id)).expect("A output"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let a_receipt_hashes = ["alpha", "beta"]
        .into_iter()
        .map(|node_id| {
            (
                node_id,
                read_node_receipt(&receipt_path(temp.path(), node_id))
                    .expect("A receipt")
                    .receipt_hash,
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut plan_b = plan_a.clone();
    change_alpha_identity(&mut plan_b);
    let mut executor_b = DeterministicExecutor::default();
    run_project_plan(&plan_b, &policy, &mut executor_b).expect("revision B runs");
    assert_eq!(
        executor_b.calls,
        vec!["alpha".to_string(), "beta".to_string()]
    );
    assert_ne!(
        a_bytes["beta"],
        fs::read(artifact_path(temp.path(), &plan_b, "beta")).expect("B output")
    );

    let mut resume_a_executor = DeterministicExecutor::default();
    let resumed = run_project_plan(&plan_a, &policy, &mut resume_a_executor)
        .expect("revision A restores from immutable artifacts");

    assert!(resume_a_executor.calls.is_empty());
    assert!(resumed.executed_nodes.is_empty());
    assert_eq!(
        resumed.resumed_nodes,
        vec!["alpha".to_string(), "beta".to_string()]
    );
    assert!(resumed.invalidated_nodes.is_empty());
    for node_id in ["alpha", "beta"] {
        assert_eq!(
            fs::read(artifact_path(temp.path(), &plan_a, node_id)).expect("restored A output"),
            a_bytes[node_id]
        );
        assert_eq!(
            read_node_receipt(&receipt_path(temp.path(), node_id))
                .expect("restored A receipt")
                .receipt_hash,
            a_receipt_hashes[node_id]
        );
    }
}

#[test]
fn poisoned_artifact_cas_refuses_revision_restore_without_execution() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_node_plan();
    let policy = approving_policy(temp.path());
    let mut executor = DeterministicExecutor::default();
    run_project_plan(&plan, &policy, &mut executor).expect("initial run");
    let receipt = read_node_receipt(&receipt_path(temp.path(), "alpha")).expect("receipt");
    let cas_path = artifact_cas_path_for_test(temp.path(), &receipt.outputs[0]);
    fs::write(&cas_path, b"poisoned-cas-bytes").expect("poison artifact CAS");

    let mut resume_executor = DeterministicExecutor::default();
    let error = run_project_plan(&plan, &policy, &mut resume_executor)
        .expect_err("poisoned artifact CAS refuses");

    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert!(
        error
            .message
            .contains("does not match its receipt digest/count")
    );
    assert!(resume_executor.calls.is_empty());
}

#[test]
fn later_poison_refuses_before_any_earlier_revision_is_restored() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan_a = independent_plan();
    let policy = approving_policy(temp.path());
    let mut executor_a = DeterministicExecutor::default();
    run_project_plan(&plan_a, &policy, &mut executor_a).expect("revision A runs");
    let beta_a = read_node_receipt(&receipt_path(temp.path(), "beta")).expect("beta A receipt");
    let beta_semantic = semantic_receipt_path_for_test(&receipt_path(temp.path(), "beta"), &beta_a);

    let mut plan_b = plan_a.clone();
    change_alpha_identity(&mut plan_b);
    let mut executor_b = DeterministicExecutor::default();
    run_project_plan(&plan_b, &policy, &mut executor_b).expect("revision B runs");
    fs::write(&beta_semantic, b"{poisoned-semantic-receipt")
        .expect("poison later semantic receipt");
    let before = tree_bytes(temp.path());

    let mut resume_executor = DeterministicExecutor::default();
    let error = run_project_plan(&plan_a, &policy, &mut resume_executor)
        .expect_err("later poison refuses before restoring alpha A");

    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert!(resume_executor.calls.is_empty());
    assert_eq!(tree_bytes(temp.path()), before);
}

#[test]
fn reuse_only_inspection_does_not_restore_mutable_revision_heads() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan_a = chain_plan();
    let policy = approving_policy(temp.path());
    let mut executor_a = DeterministicExecutor::default();
    run_project_plan(&plan_a, &policy, &mut executor_a).expect("revision A runs");

    let mut plan_b = plan_a.clone();
    change_alpha_identity(&mut plan_b);
    let mut executor_b = DeterministicExecutor::default();
    run_project_plan(&plan_b, &policy, &mut executor_b).expect("revision B runs");
    let before = tree_bytes(temp.path());

    let inspected = inspect_project_run_reuse_only(&plan_a, &policy)
        .expect("inspection validates reusable A without restoring it");

    assert!(inspected.resumed_nodes.is_empty());
    assert_eq!(inspected.node_reports.len(), 2);
    assert!(inspected.node_reports.iter().all(|report| {
        report
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("read-only inspection did not restore outputs"))
    }));
    assert_eq!(
        inspected.receipt.completed_nodes,
        vec!["alpha".to_string(), "beta".to_string()]
    );
    assert_eq!(tree_bytes(temp.path()), before);
}

#[test]
fn poisoned_canonical_receipt_cas_refuses_before_mutable_head_restoration() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan_a = single_node_plan();
    let policy = approving_policy(temp.path());
    let mut executor_a = DeterministicExecutor::default();
    run_project_plan(&plan_a, &policy, &mut executor_a).expect("revision A runs");
    let alpha_a = read_node_receipt(&receipt_path(temp.path(), "alpha")).expect("alpha A receipt");
    let alpha_a_cas =
        node_receipt_cas_path(&receipt_path(temp.path(), "alpha"), &alpha_a.receipt_hash);

    let mut plan_b = plan_a.clone();
    change_alpha_identity(&mut plan_b);
    let mut executor_b = DeterministicExecutor::default();
    run_project_plan(&plan_b, &policy, &mut executor_b).expect("revision B runs");
    fs::write(&alpha_a_cas, b"poisoned-canonical-receipt-cas")
        .expect("poison canonical receipt CAS");
    let before = tree_bytes(temp.path());

    let mut resume_executor = DeterministicExecutor::default();
    let error = run_project_plan(&plan_a, &policy, &mut resume_executor)
        .expect_err("canonical receipt CAS poison refuses before restoring A");

    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert!(error.message.contains("content-addressed project receipt"));
    assert!(resume_executor.calls.is_empty());
    assert_eq!(tree_bytes(temp.path()), before);
}

#[test]
fn later_unreceipted_head_refuses_before_any_earlier_revision_is_restored() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan_a = independent_plan();
    let policy = approving_policy(temp.path());
    let mut executor_a = DeterministicExecutor::default();
    run_project_plan(&plan_a, &policy, &mut executor_a).expect("revision A runs");

    let mut plan_b = plan_a.clone();
    change_alpha_identity(&mut plan_b);
    let mut executor_b = DeterministicExecutor::default();
    run_project_plan(&plan_b, &policy, &mut executor_b).expect("revision B runs");
    fs::write(
        artifact_path(temp.path(), &plan_b, "beta"),
        b"unreceipted-beta-head",
    )
    .expect("plant unreceipted later head");
    let before = tree_bytes(temp.path());

    let mut resume_executor = DeterministicExecutor::default();
    let error = run_project_plan(&plan_a, &policy, &mut resume_executor)
        .expect_err("later unreceipted head refuses before restoring alpha");

    assert_eq!(error.code, ProjectRunErrorCode::AtomicPublication);
    assert!(error.message.contains("matches neither"));
    assert!(resume_executor.calls.is_empty());
    assert_eq!(tree_bytes(temp.path()), before);
}

#[test]
fn unexpected_mutable_head_refuses_atomic_revision_restore() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan_a = single_node_plan();
    let policy = approving_policy(temp.path());
    let mut executor_a = DeterministicExecutor::default();
    run_project_plan(&plan_a, &policy, &mut executor_a).expect("revision A runs");
    let mut plan_b = plan_a.clone();
    change_alpha_identity(&mut plan_b);
    let mut executor_b = DeterministicExecutor::default();
    run_project_plan(&plan_b, &policy, &mut executor_b).expect("revision B runs");
    let output_path = artifact_path(temp.path(), &plan_a, "alpha");
    fs::write(&output_path, b"unreceipted-concurrent-head").expect("replace mutable head");

    let mut resume_executor = DeterministicExecutor::default();
    let error = run_project_plan(&plan_a, &policy, &mut resume_executor)
        .expect_err("unexpected mutable head refuses restoration");

    assert_eq!(error.code, ProjectRunErrorCode::AtomicPublication);
    assert!(error.message.contains("recoverable prior receipt"));
    assert_eq!(
        fs::read(output_path).expect("unexpected head preserved"),
        b"unreceipted-concurrent-head"
    );
    assert!(resume_executor.calls.is_empty());
}

#[test]
fn semantic_cache_output_path_binding_is_revalidated_before_restore() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_node_plan();
    let policy = approving_policy(temp.path());
    let mut executor = DeterministicExecutor::default();
    run_project_plan(&plan, &policy, &mut executor).expect("initial run");
    let canonical = receipt_path(temp.path(), "alpha");
    let canonical_receipt = read_node_receipt(&canonical).expect("canonical receipt");
    let semantic = semantic_receipt_path_for_test(&canonical, &canonical_receipt);
    let mut poisoned = read_node_receipt(&semantic).expect("semantic receipt");
    poisoned.outputs[0].path = "work/not-the-declared-output.json".to_string();
    let poisoned = finalized_node_receipt(poisoned).expect("path-poisoned receipt finalizes");
    fs::write(
        &semantic,
        canonical_node_receipt_bytes(&poisoned).expect("poisoned receipt bytes"),
    )
    .expect("poison semantic cache path binding");

    let mut resume_executor = DeterministicExecutor::default();
    let error = run_project_plan(&plan, &policy, &mut resume_executor)
        .expect_err("path-poisoned semantic cache refuses");

    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert!(
        error
            .message
            .contains("path/digest/count or dependency bindings")
    );
    assert!(resume_executor.calls.is_empty());
}

#[test]
fn semantic_cache_dependency_receipt_binding_is_revalidated_before_restore() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = chain_plan();
    let policy = approving_policy(temp.path());
    let mut executor = DeterministicExecutor::default();
    run_project_plan(&plan, &policy, &mut executor).expect("initial chain run");
    let canonical = receipt_path(temp.path(), "beta");
    let canonical_receipt = read_node_receipt(&canonical).expect("canonical beta receipt");
    let semantic = semantic_receipt_path_for_test(&canonical, &canonical_receipt);
    let mut poisoned = read_node_receipt(&semantic).expect("beta semantic receipt");
    poisoned.dependency_receipt_hashes.insert(
        "alpha".to_string(),
        digest_bytes(b"not-the-restored-alpha-receipt"),
    );
    let poisoned = finalized_node_receipt(poisoned).expect("dependency-poisoned receipt finalizes");
    fs::write(
        &semantic,
        canonical_node_receipt_bytes(&poisoned).expect("poisoned receipt bytes"),
    )
    .expect("poison dependency binding");

    let mut resume_executor = DeterministicExecutor::default();
    let error = run_project_plan(&plan, &policy, &mut resume_executor)
        .expect_err("dependency-poisoned semantic cache refuses");

    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert!(error.message.contains("historical dependency receipt"));
    assert!(resume_executor.calls.is_empty());
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
        without_run_manifest(without_cancel_receipts(tree_bytes(resumed_dir.path()))),
        without_run_manifest(tree_bytes(uninterrupted_dir.path()))
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
    refresh_node_cache_key(intake);

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
    assert_eq!(resume_executor.calls.len(), plan.nodes.len());
    assert_eq!(resumed.executed_nodes.len(), plan.nodes.len());
    assert!(resumed.blocked_nodes.is_empty());
    assert!(resumed.next_actions.is_empty());
}

#[test]
fn invalidation_reexecutes_changed_nodes_while_reusing_unchanged_nodes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut plan = independent_plan();
    let policy = approving_policy(temp.path());
    let mut executor = DeterministicExecutor::default();
    run_project_plan(&plan, &policy, &mut executor).expect("initial independent run");

    let alpha = plan
        .nodes
        .iter_mut()
        .find(|node| node.node_id == "alpha")
        .expect("alpha node");
    alpha.content_hash_inputs[0].content_hash = digest_bytes(b"changed-alpha");
    refresh_node_cache_key(alpha);

    let mut resume_executor = DeterministicExecutor::default();
    let resumed = run_project_plan(&plan, &policy, &mut resume_executor)
        .expect("independent invalidation recovers");

    assert_eq!(resume_executor.calls, vec!["alpha".to_string()]);
    assert_eq!(resumed.executed_nodes, vec!["alpha".to_string()]);
    assert_eq!(resumed.resumed_nodes, vec!["beta".to_string()]);
    assert_eq!(resumed.invalidated_nodes, vec!["alpha".to_string()]);
    assert!(resumed.next_actions.is_empty());
}

#[test]
fn telemetry_variation_preserves_semantic_hashes_and_downstream_artifacts() {
    let plan = chain_plan();
    let first_dir = tempfile::tempdir().expect("first");
    let second_dir = tempfile::tempdir().expect("second");

    let mut first_executor = TelemetryExecutor::new(10, 100, 7);
    let first = run_project_plan(
        &plan,
        &approving_policy(first_dir.path()),
        &mut first_executor,
    )
    .expect("first telemetry run");
    assert_eq!(first.failed_nodes.len(), 0);

    let mut second_executor = TelemetryExecutor::new(99_999, 42_424, 7);
    let second = run_project_plan(
        &plan,
        &approving_policy(second_dir.path()),
        &mut second_executor,
    )
    .expect("second telemetry run");
    assert_eq!(second.failed_nodes.len(), 0);

    for node_id in ["alpha", "beta"] {
        let first_receipt =
            read_node_receipt(&receipt_path(first_dir.path(), node_id)).expect("first receipt");
        let second_receipt =
            read_node_receipt(&receipt_path(second_dir.path(), node_id)).expect("second receipt");
        assert_eq!(first_receipt.semantic_hash, second_receipt.semantic_hash);
        assert_ne!(first_receipt.telemetry_hash, second_receipt.telemetry_hash);
        assert_ne!(first_receipt.receipt_hash, second_receipt.receipt_hash);
    }

    assert_eq!(
        fs::read(artifact_path(first_dir.path(), &plan, "beta")).expect("first beta output"),
        fs::read(artifact_path(second_dir.path(), &plan, "beta")).expect("second beta output")
    );
}

#[test]
fn child_reuse_accepts_parent_telemetry_change_with_verified_historical_lineage() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = chain_plan();
    let policy = approving_policy(temp.path());
    let mut executor = TelemetryExecutor::new(10, 100, 7);
    run_project_plan(&plan, &policy, &mut executor).expect("initial telemetry run");
    let beta_before = read_node_receipt(&receipt_path(temp.path(), "beta")).expect("beta receipt");
    let (alpha_before, alpha_after) = install_parent_telemetry_variant(temp.path(), &plan);

    let mut resume_executor = DeterministicExecutor::default();
    let resumed = run_project_plan(&plan, &policy, &mut resume_executor)
        .expect("verified historical dependency lineage permits child reuse");

    assert!(resume_executor.calls.is_empty());
    assert_eq!(
        resumed.resumed_nodes,
        vec!["alpha".to_string(), "beta".to_string()]
    );
    assert_eq!(alpha_before.semantic_hash, alpha_after.semantic_hash);
    assert_ne!(alpha_before.receipt_hash, alpha_after.receipt_hash);
    assert_eq!(
        read_node_receipt(&receipt_path(temp.path(), "alpha"))
            .expect("current alpha receipt")
            .receipt_hash,
        alpha_after.receipt_hash
    );
    assert_eq!(
        read_node_receipt(&receipt_path(temp.path(), "beta"))
            .expect("reused beta receipt")
            .dependency_receipt_hashes["alpha"],
        alpha_before.receipt_hash
    );
    assert_eq!(
        beta_before.dependency_receipt_hashes["alpha"],
        alpha_before.receipt_hash
    );
}

#[test]
fn child_reuse_refuses_parent_telemetry_change_when_historical_lineage_is_poisoned() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = chain_plan();
    let policy = approving_policy(temp.path());
    let mut executor = TelemetryExecutor::new(10, 100, 7);
    run_project_plan(&plan, &policy, &mut executor).expect("initial telemetry run");
    let (alpha_before, _) = install_parent_telemetry_variant(temp.path(), &plan);
    let historical_path = node_receipt_cas_path(
        &receipt_path(temp.path(), "alpha"),
        &alpha_before.receipt_hash,
    );
    fs::write(&historical_path, b"poisoned historical receipt")
        .expect("poison historical dependency receipt CAS");

    let mut resume_executor = DeterministicExecutor::default();
    let error = run_project_plan(&plan, &policy, &mut resume_executor)
        .expect_err("poisoned historical dependency lineage refuses child reuse");

    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert!(error.message.contains("historical dependency receipt"));
    assert!(resume_executor.calls.is_empty());
}

#[test]
fn failure_prose_is_integrity_protected_but_not_semantic_identity() {
    let mut first = standalone_receipt(Path::new("unused"), "failed-node");
    first.outcome = ProjectRunNodeOutcome::Failed;
    first.next_action = receipt::ProjectRunNextAction::InspectFailure;
    first.failure_code = Some("E_INPUT".to_string());
    first.failure_message = Some("failed to read /first/workspace/input.json".to_string());
    let first = finalized_node_receipt(first).expect("first failure receipt");

    let mut second = first.clone();
    second.failure_message = Some("failed to read /second/workspace/input.json".to_string());
    let second = finalized_node_receipt(second).expect("second failure receipt");

    assert_eq!(first.semantic_hash, second.semantic_hash);
    assert_ne!(first.telemetry_hash, second.telemetry_hash);
    assert_ne!(first.receipt_hash, second.receipt_hash);
}

#[test]
fn deterministic_usage_counters_are_semantic_dependency_inputs() {
    let plan = chain_plan();
    let first_dir = tempfile::tempdir().expect("first");
    let second_dir = tempfile::tempdir().expect("second");

    let mut first_executor = TelemetryExecutor::new(10, 100, 7);
    run_project_plan(
        &plan,
        &approving_policy(first_dir.path()),
        &mut first_executor,
    )
    .expect("first run");

    let mut second_executor = TelemetryExecutor::new(10, 100, 8);
    run_project_plan(
        &plan,
        &approving_policy(second_dir.path()),
        &mut second_executor,
    )
    .expect("second run");

    let first_alpha =
        read_node_receipt(&receipt_path(first_dir.path(), "alpha")).expect("first alpha receipt");
    let second_alpha =
        read_node_receipt(&receipt_path(second_dir.path(), "alpha")).expect("second alpha receipt");
    assert_ne!(first_alpha.semantic_hash, second_alpha.semantic_hash);

    let first_beta =
        read_node_receipt(&receipt_path(first_dir.path(), "beta")).expect("first beta receipt");
    let second_beta =
        read_node_receipt(&receipt_path(second_dir.path(), "beta")).expect("second beta receipt");
    assert_ne!(
        first_beta.dependency_semantic_hashes,
        second_beta.dependency_semantic_hashes
    );
    assert_ne!(first_beta.semantic_hash, second_beta.semantic_hash);
}

#[test]
fn telemetry_tampering_is_detected_without_downstream_semantic_coupling() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = chain_plan();
    let mut executor = TelemetryExecutor::new(10, 100, 7);
    run_project_plan(&plan, &approving_policy(temp.path()), &mut executor).expect("run");

    let alpha_path = receipt_path(temp.path(), "alpha");
    let original = read_node_receipt(&alpha_path).expect("original receipt");
    let mut tampered: Value =
        serde_json::from_slice(&fs::read(&alpha_path).expect("receipt bytes"))
            .expect("receipt json");
    tampered["duration_millis"] = Value::from(999_999_u64);
    fs::write(
        &alpha_path,
        serde_json::to_vec(&tampered).expect("tampered bytes"),
    )
    .expect("write tampered receipt");

    let error = read_node_receipt(&alpha_path).expect_err("tampered telemetry refuses");
    assert_eq!(error.code, ProjectReceiptErrorCode::HashMismatch);
    assert!(error.message.contains("telemetry hash mismatch"));
    assert_eq!(
        original.semantic_hash,
        tampered["semantic_hash"].as_str().unwrap()
    );
}

#[test]
fn run_report_serializes_schema_declared_hash_surfaces() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = chain_plan();
    let mut executor = TelemetryExecutor::new(10, 100, 7);
    let report = run_project_plan(&plan, &approving_policy(temp.path()), &mut executor)
        .expect("telemetry run");
    let report_json: Value =
        serde_json::from_slice(&canonical_project_run_report_bytes(&report).expect("report bytes"))
            .expect("report json");
    let receipts = report_json["receipt"]["node_receipts"]
        .as_array()
        .expect("node receipts");
    let alpha_json = receipts
        .iter()
        .find(|receipt| receipt["node_id"] == "alpha")
        .expect("alpha receipt");
    let beta_json = receipts
        .iter()
        .find(|receipt| receipt["node_id"] == "beta")
        .expect("beta receipt");
    let alpha = read_node_receipt(&receipt_path(temp.path(), "alpha")).expect("alpha receipt");

    assert_eq!(alpha_json["deterministic_usage"]["rows_examined"], 7);
    assert!(alpha_json["semantic_hash"].as_str().is_some());
    assert!(alpha_json["telemetry_hash"].as_str().is_some());
    assert_eq!(
        beta_json["dependency_semantic_hashes"]["alpha"]
            .as_str()
            .expect("beta dependency semantic hash"),
        alpha.semantic_hash
    );
    assert!(
        beta_json["dependency_receipt_hashes"]["alpha"]
            .as_str()
            .expect("beta dependency receipt hash")
            .starts_with("blake3:")
    );
}

#[test]
fn executor_context_carries_dependency_semantics_and_validated_output_bytes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = chain_plan();
    let mut executor = ContextRecordingExecutor::default();
    run_project_plan(&plan, &approving_policy(temp.path()), &mut executor)
        .expect("context recording run");

    let alpha = read_node_receipt(&receipt_path(temp.path(), "alpha")).expect("alpha receipt");
    let beta = read_node_receipt(&receipt_path(temp.path(), "beta")).expect("beta receipt");

    assert!(
        executor
            .dependency_contexts
            .get("alpha")
            .expect("alpha context")
            .is_empty()
    );
    assert_eq!(
        executor
            .dependency_contexts
            .get("beta")
            .and_then(|deps| deps.get("alpha")),
        Some(&alpha.semantic_hash)
    );
    assert_eq!(
        beta.dependency_semantic_hashes.get("alpha"),
        Some(&alpha.semantic_hash)
    );
    assert_eq!(
        beta.dependency_receipt_hashes.get("alpha"),
        Some(&alpha.receipt_hash)
    );
    let alpha_outputs = executor
        .dependency_outputs
        .get("beta")
        .and_then(|dependencies| dependencies.get("alpha"))
        .expect("beta receives alpha outputs");
    assert_eq!(alpha_outputs.len(), 1);
    assert_eq!(alpha_outputs[0].output_id, alpha.outputs[0].output_id);
    assert_eq!(
        alpha_outputs[0].content_digest,
        alpha.outputs[0].content_digest
    );
    assert_eq!(
        alpha_outputs[0].bytes,
        fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("alpha artifact")
    );
}

#[test]
fn plan_graph_hash_change_alone_reuses_project_scoped_node_receipts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut plan = independent_plan();
    let policy = approving_policy(temp.path());
    let mut executor = DeterministicExecutor::default();
    run_project_plan(&plan, &policy, &mut executor).expect("initial run");

    plan.graph_hash = digest_bytes(b"same-nodes-new-plan-graph");
    let mut resume_executor = DeterministicExecutor::default();
    let resumed = run_project_plan(&plan, &policy, &mut resume_executor)
        .expect("plan graph only change resumes");

    assert!(resume_executor.calls.is_empty());
    assert!(resumed.executed_nodes.is_empty());
    assert_eq!(
        resumed.resumed_nodes,
        vec!["alpha".to_string(), "beta".to_string()]
    );
    assert!(resumed.invalidated_nodes.is_empty());
}

#[test]
fn command_effect_and_refusal_contract_changes_invalidate_node_local_reuse() {
    type NodeMutation = fn(&mut ProjectPlanNode);
    let cases: [(&str, NodeMutation); 3] = [
        ("command", mutate_command_contract),
        ("side_effect", mutate_side_effect_contract),
        ("refusal", mutate_refusal_contract),
    ];

    for (label, mutate) in cases {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut plan = independent_plan();
        let policy = approving_policy(temp.path());
        let mut executor = DeterministicExecutor::default();
        run_project_plan(&plan, &policy, &mut executor).expect("initial independent run");

        let alpha = plan
            .nodes
            .iter_mut()
            .find(|node| node.node_id == "alpha")
            .expect("alpha node");
        mutate(alpha);
        refresh_node_cache_key(alpha);
        plan.graph_hash = digest_bytes(format!("node-local-{label}-change").as_bytes());

        let mut resume_executor = DeterministicExecutor::default();
        let resumed = run_project_plan(&plan, &policy, &mut resume_executor)
            .unwrap_or_else(|error| panic!("{label} change should produce a run report: {error}"));

        assert_eq!(resume_executor.calls, vec!["alpha".to_string()], "{label}");
        assert_eq!(resumed.executed_nodes, vec!["alpha".to_string()], "{label}");
        assert_eq!(resumed.resumed_nodes, vec!["beta".to_string()], "{label}");
        assert_eq!(
            resumed.invalidated_nodes,
            vec!["alpha".to_string()],
            "{label}"
        );
        assert!(resumed.failed_nodes.is_empty(), "{label}");
    }
}

#[test]
fn stale_cache_key_after_semantic_contract_change_refuses_before_execution() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut plan = single_node_plan();
    let policy = approving_policy(temp.path());
    let mut executor = FixedOutputExecutor::new(b"initial".to_vec());
    run_project_plan(&plan, &policy, &mut executor).expect("initial run");

    plan.nodes[0].command =
        "canon project execute --plan work/plan.json --node alpha-v2".to_string();
    plan.graph_hash = digest_bytes(b"stale-cache-key-contract-change");
    let mut resume_executor = FixedOutputExecutor::new(b"should-not-run".to_vec());
    let error = run_project_plan(&plan, &policy, &mut resume_executor)
        .expect_err("stale cache key refuses");

    assert!(resume_executor.calls.is_empty());
    assert_eq!(error.code, ProjectRunErrorCode::ArtifactContract);
    assert!(
        error
            .message
            .contains("node cache key must bind command, declared side effects")
    );
}

#[test]
fn registered_internal_copy_file_executor_runs_pending_node_and_reuses_receipt() {
    let temp = tempfile::tempdir().expect("tempdir");
    let input_bytes = b"id,name\n1,Alice\n";
    write_workspace_file(temp.path(), "input/source.csv", input_bytes);
    let mut plan = single_node_plan();
    configure_internal_copy_node(&mut plan, "alpha", "input/source.csv", input_bytes);
    let policy = ProjectRunPolicy::new(temp.path(), "work");

    let first = run_project_plan_with_registered_executors(&plan, &policy)
        .expect("registered executor run");

    assert_eq!(first.executed_nodes, vec!["alpha".to_string()]);
    assert_eq!(first.resumed_nodes, Vec::<String>::new());
    assert!(first.failed_nodes.is_empty());
    assert_eq!(
        fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("published artifact"),
        input_bytes
    );
    let receipt = read_node_receipt(&receipt_path(temp.path(), "alpha")).expect("node receipt");
    assert_eq!(receipt.outputs[0].content_digest, digest_bytes(input_bytes));
    assert_eq!(receipt.outputs[0].byte_count, input_bytes.len() as u64);

    let second = run_project_plan_with_registered_executors(&plan, &policy)
        .expect("registered executor resume");

    assert!(second.executed_nodes.is_empty());
    assert_eq!(second.resumed_nodes, vec!["alpha".to_string()]);
}

#[test]
fn completed_receipt_with_foreign_output_id_is_refused_not_resumed() {
    let temp = tempfile::tempdir().expect("tempdir");
    let input_bytes = b"id,name\n1,Alice\n";
    write_workspace_file(temp.path(), "input/source.csv", input_bytes);
    let mut plan = single_node_plan();
    configure_internal_copy_node(&mut plan, "alpha", "input/source.csv", input_bytes);
    let policy = ProjectRunPolicy::new(temp.path(), "work");

    run_project_plan_with_registered_executors(&plan, &policy).expect("initial execution");
    let path = receipt_path(temp.path(), "alpha");
    let mut receipt = read_node_receipt(&path).expect("initial receipt");
    receipt.outputs[0].output_id = "foreign-output".to_string();
    let receipt = finalized_node_receipt(receipt).expect("foreign output receipt finalizes");
    fs::write(
        &path,
        canonical_node_receipt_bytes(&receipt).expect("foreign output receipt bytes"),
    )
    .expect("write foreign output receipt");

    let error = run_project_plan_with_registered_executors(&plan, &policy)
        .expect_err("foreign output receipt leaves an unrecoverable stale artifact");

    assert_eq!(error.code, ProjectRunErrorCode::StaleArtifact);
    assert!(error.message.contains("has no valid prior receipt"));
    assert_eq!(
        fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("artifact preserved"),
        input_bytes
    );
    let preserved = read_node_receipt(&path).expect("foreign receipt remains integrity-valid");
    assert_eq!(preserved.outputs[0].output_id, "foreign-output");
}

#[test]
fn unknown_registered_executor_refuses_before_publication_or_receipt() {
    let temp = tempfile::tempdir().expect("tempdir");
    let input_bytes = b"id,name\n1,Alice\n";
    write_workspace_file(temp.path(), "input/source.csv", input_bytes);
    let mut plan = single_node_plan();
    configure_internal_copy_node(&mut plan, "alpha", "input/source.csv", input_bytes);
    plan.nodes[0].command = plan.nodes[0]
        .command
        .replace(PROJECT_INTERNAL_COPY_FILE_EXECUTOR, "missing-executor-v1");
    refresh_node_cache_key(&mut plan.nodes[0]);
    plan.graph_hash = digest_bytes(b"unknown-registered-executor");

    let error = run_project_plan_with_registered_executors(
        &plan,
        &ProjectRunPolicy::new(temp.path(), "work"),
    )
    .expect_err("unknown executor refuses");

    assert_eq!(error.code, ProjectRunErrorCode::ExecutionFailed);
    assert!(error.message.contains("no registered real executor"));
    assert!(!artifact_path(temp.path(), &plan, "alpha").exists());
    assert!(!receipt_path(temp.path(), "alpha").exists());
}

#[test]
fn undeclared_internal_copy_input_refuses_before_publication_or_receipt() {
    let temp = tempfile::tempdir().expect("tempdir");
    let input_bytes = b"id,name\n1,Alice\n";
    write_workspace_file(temp.path(), "input/source.csv", input_bytes);
    let mut plan = single_node_plan();
    configure_internal_copy_node(&mut plan, "alpha", "input/source.csv", input_bytes);
    plan.nodes[0]
        .side_effects
        .retain(|effect| effect.kind != ProjectPlanSideEffectKind::ReadsInput);
    refresh_node_cache_key(&mut plan.nodes[0]);
    plan.graph_hash = digest_bytes(b"undeclared-copy-input");

    let error = run_project_plan_with_registered_executors(
        &plan,
        &ProjectRunPolicy::new(temp.path(), "work"),
    )
    .expect_err("undeclared input refuses");

    assert_eq!(error.code, ProjectRunErrorCode::ExecutionFailed);
    assert!(error.message.contains("read-input"));
    assert!(!artifact_path(temp.path(), &plan, "alpha").exists());
    assert!(!receipt_path(temp.path(), "alpha").exists());
}

#[test]
#[cfg(unix)]
fn registered_internal_copy_file_executor_refuses_symlink_escape_input() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let outside = tempfile::tempdir().expect("outside tempdir");
    let outside_bytes = b"id,name\n1,outside\n";
    fs::write(outside.path().join("outside.csv"), outside_bytes).expect("outside bytes");
    fs::create_dir_all(temp.path().join("input")).expect("input dir");
    symlink(
        outside.path().join("outside.csv"),
        temp.path().join("input/escape.csv"),
    )
    .expect("escape symlink");
    let mut plan = single_node_plan();
    configure_internal_copy_node(&mut plan, "alpha", "input/escape.csv", outside_bytes);

    let error = run_project_plan_with_registered_executors(
        &plan,
        &ProjectRunPolicy::new(temp.path(), "work"),
    )
    .expect_err("symlink escape refuses before execution");

    assert_eq!(error.code, ProjectRunErrorCode::WorkspacePolicy);
    assert!(error.message.contains("workspace safety"));
    assert!(error.message.contains("outside the workspace"));
    assert!(!artifact_path(temp.path(), &plan, "alpha").exists());
    assert!(!receipt_path(temp.path(), "alpha").exists());
}

#[test]
#[cfg(unix)]
fn registered_internal_copy_file_executor_refuses_symlink_escape_output_parent() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let outside = tempfile::tempdir().expect("outside tempdir");
    let input_bytes = b"id,name\n1,Alice\n";
    write_workspace_file(temp.path(), "input/source.csv", input_bytes);
    symlink(outside.path(), temp.path().join("published")).expect("output parent symlink");
    let mut plan = single_node_plan();
    configure_internal_copy_node(&mut plan, "alpha", "input/source.csv", input_bytes);
    plan.nodes[0].outputs[0].path = "published/alpha.json".to_string();
    refresh_node_cache_key(&mut plan.nodes[0]);
    plan.graph_hash = digest_bytes(b"symlink-output-parent");

    let error = run_project_plan_with_registered_executors(
        &plan,
        &ProjectRunPolicy::new(temp.path(), "work"),
    )
    .expect_err("symlink output parent refuses before execution");

    assert_eq!(error.code, ProjectRunErrorCode::WorkspacePolicy);
    assert!(
        error
            .message
            .contains("output path failed workspace safety")
    );
    assert!(!outside.path().join("alpha.json").exists());
    assert!(!receipt_path(temp.path(), "alpha").exists());
}

#[test]
#[cfg(unix)]
fn project_run_refuses_symlink_escape_work_directory() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let outside = tempfile::tempdir().expect("outside tempdir");
    let input_bytes = b"id,name\n1,Alice\n";
    write_workspace_file(temp.path(), "input/source.csv", input_bytes);
    symlink(outside.path(), temp.path().join("work")).expect("work directory symlink");
    let mut plan = single_node_plan();
    configure_internal_copy_node(&mut plan, "alpha", "input/source.csv", input_bytes);

    let error = run_project_plan_with_registered_executors(
        &plan,
        &ProjectRunPolicy::new(temp.path(), "work"),
    )
    .expect_err("symlink work directory refuses before execution");

    assert_eq!(error.code, ProjectRunErrorCode::WorkspacePolicy);
    assert!(
        error
            .message
            .contains("receipt path failed workspace safety")
    );
    assert!(!outside.path().join("receipts/alpha.json").exists());
    assert!(!outside.path().join("alpha.json").exists());
}

#[test]
#[cfg(unix)]
fn project_run_refuses_semantic_receipt_symlink_escape() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let outside = tempfile::tempdir().expect("outside tempdir");
    fs::create_dir_all(temp.path().join("work/receipts")).expect("receipt directory");
    symlink(
        outside.path(),
        temp.path().join("work/receipts/by-cache-key"),
    )
    .expect("semantic receipt directory symlink");
    let plan = single_node_plan();
    let mut executor = DeterministicExecutor::default();

    let error = run_project_plan(&plan, &approving_policy(temp.path()), &mut executor)
        .expect_err("semantic receipt symlink escape refuses before execution");

    assert_eq!(error.code, ProjectRunErrorCode::WorkspacePolicy);
    assert!(error.message.contains("semantic project receipt path"));
    assert!(error.message.contains("outside the workspace"));
    assert!(executor.calls.is_empty());
    assert!(
        fs::read_dir(outside.path())
            .expect("outside directory")
            .next()
            .is_none(),
        "semantic receipt resolution must not write through an escaping symlink"
    );
    assert!(!artifact_path(temp.path(), &plan, "alpha").exists());
}

#[test]
#[cfg(unix)]
fn project_run_refuses_canonical_receipt_cas_symlink_escape() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let outside = tempfile::tempdir().expect("outside tempdir");
    fs::create_dir_all(temp.path().join("work/receipts")).expect("receipt directory");
    symlink(outside.path(), temp.path().join("work/receipts/cas"))
        .expect("canonical receipt CAS symlink");
    let plan = single_node_plan();
    let mut executor = DeterministicExecutor::default();

    let error = run_project_plan(&plan, &approving_policy(temp.path()), &mut executor)
        .expect_err("canonical receipt CAS symlink escape refuses before execution");

    assert_eq!(error.code, ProjectRunErrorCode::WorkspacePolicy);
    assert!(error.message.contains("project receipt CAS path"));
    assert!(error.message.contains("outside the workspace"));
    assert!(executor.calls.is_empty());
    assert!(
        fs::read_dir(outside.path())
            .expect("outside directory")
            .next()
            .is_none(),
        "canonical receipt CAS resolution must not write through an escaping symlink"
    );
}

#[test]
#[cfg(unix)]
fn project_run_refuses_semantic_receipt_cas_symlink_escape() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let outside = tempfile::tempdir().expect("outside tempdir");
    fs::create_dir_all(temp.path().join("work/receipts/by-cache-key"))
        .expect("semantic receipt directory");
    symlink(
        outside.path(),
        temp.path().join("work/receipts/by-cache-key/cas"),
    )
    .expect("semantic receipt CAS symlink");
    let plan = single_node_plan();
    let mut executor = DeterministicExecutor::default();

    let error = run_project_plan(&plan, &approving_policy(temp.path()), &mut executor)
        .expect_err("semantic receipt CAS symlink escape refuses before execution");

    assert_eq!(error.code, ProjectRunErrorCode::WorkspacePolicy);
    assert!(error.message.contains("semantic project receipt CAS path"));
    assert!(error.message.contains("outside the workspace"));
    assert!(executor.calls.is_empty());
    assert!(
        fs::read_dir(outside.path())
            .expect("outside directory")
            .next()
            .is_none(),
        "semantic receipt CAS resolution must not write through an escaping symlink"
    );
}

#[test]
#[cfg(unix)]
fn project_run_refuses_canonical_receipt_cas_leaf_symlink_escape() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let outside = tempfile::tempdir().expect("outside tempdir");
    let plan = single_node_plan();
    let policy = approving_policy(temp.path());
    let mut executor = DeterministicExecutor::default();
    run_project_plan(&plan, &policy, &mut executor).expect("initial run");
    let receipt = read_node_receipt(&receipt_path(temp.path(), "alpha")).expect("alpha receipt");
    let cas_path =
        node_receipt_cas_path(&receipt_path(temp.path(), "alpha"), &receipt.receipt_hash);
    fs::rename(&cas_path, cas_path.with_extension("saved")).expect("preserve original CAS leaf");
    let outside_path = outside.path().join("receipt.json");
    fs::write(&outside_path, b"outside-receipt-bytes").expect("outside receipt");
    symlink(&outside_path, &cas_path).expect("canonical CAS leaf symlink");
    let head_before = fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("alpha head");

    let mut resume_executor = DeterministicExecutor::default();
    let error = run_project_plan(&plan, &policy, &mut resume_executor)
        .expect_err("canonical receipt CAS leaf symlink refuses");

    assert_eq!(error.code, ProjectRunErrorCode::WorkspacePolicy);
    assert!(error.message.contains("canonical project receipt CAS"));
    assert!(resume_executor.calls.is_empty());
    assert_eq!(
        fs::read(&outside_path).expect("outside receipt unchanged"),
        b"outside-receipt-bytes"
    );
    assert_eq!(
        fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("alpha head unchanged"),
        head_before
    );
}

#[test]
#[cfg(unix)]
fn project_run_refuses_semantic_receipt_cas_leaf_symlink_escape() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let outside = tempfile::tempdir().expect("outside tempdir");
    let plan = single_node_plan();
    let policy = approving_policy(temp.path());
    let mut executor = DeterministicExecutor::default();
    run_project_plan(&plan, &policy, &mut executor).expect("initial run");
    let canonical = receipt_path(temp.path(), "alpha");
    let receipt = read_node_receipt(&canonical).expect("alpha receipt");
    let semantic = semantic_receipt_path_for_test(&canonical, &receipt);
    let cas_path = node_receipt_cas_path(&semantic, &receipt.receipt_hash);
    fs::rename(&cas_path, cas_path.with_extension("saved")).expect("preserve semantic CAS leaf");
    let outside_path = outside.path().join("receipt.json");
    fs::write(&outside_path, b"outside-semantic-receipt").expect("outside receipt");
    symlink(&outside_path, &cas_path).expect("semantic CAS leaf symlink");
    let head_before = fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("alpha head");

    let mut resume_executor = DeterministicExecutor::default();
    let error = run_project_plan(&plan, &policy, &mut resume_executor)
        .expect_err("semantic receipt CAS leaf symlink refuses");

    assert_eq!(error.code, ProjectRunErrorCode::WorkspacePolicy);
    assert!(error.message.contains("semantic project receipt CAS"));
    assert!(resume_executor.calls.is_empty());
    assert_eq!(
        fs::read(&outside_path).expect("outside receipt unchanged"),
        b"outside-semantic-receipt"
    );
    assert_eq!(
        fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("alpha head unchanged"),
        head_before
    );
}

#[test]
fn registered_executor_refuses_uppercase_blake3_arguments() {
    let temp = tempfile::tempdir().expect("tempdir");
    let input_bytes = b"id,name\n1,Alice\n";
    write_workspace_file(temp.path(), "input/source.csv", input_bytes);
    let mut plan = single_node_plan();
    configure_internal_copy_node(&mut plan, "alpha", "input/source.csv", input_bytes);
    let uppercase_digest = digest_bytes(input_bytes).to_ascii_uppercase();
    plan.nodes[0].command = internal_copy_command(
        "input/source.csv",
        &uppercase_digest,
        &plan.nodes[0].outputs[0].output_id,
        &uppercase_digest,
    );
    refresh_node_cache_key(&mut plan.nodes[0]);
    plan.graph_hash = digest_bytes(b"uppercase-command-digest");

    let error = run_project_plan_with_registered_executors(
        &plan,
        &ProjectRunPolicy::new(temp.path(), "work"),
    )
    .expect_err("uppercase digest refuses before execution");

    assert_eq!(error.code, ProjectRunErrorCode::ExecutionFailed);
    assert!(error.message.contains("must be a blake3 digest"));
    assert!(!artifact_path(temp.path(), &plan, "alpha").exists());
    assert!(!receipt_path(temp.path(), "alpha").exists());
}

#[test]
fn registered_executor_content_digest_mismatch_writes_failure_receipt_without_artifact() {
    let temp = tempfile::tempdir().expect("tempdir");
    let input_bytes = b"id,name\n1,Alice\n";
    write_workspace_file(temp.path(), "input/source.csv", input_bytes);
    let mut plan = single_node_plan();
    configure_internal_copy_node(&mut plan, "alpha", "input/source.csv", input_bytes);
    plan.nodes[0].command = internal_copy_command(
        "input/source.csv",
        &digest_bytes(b"declared-wrong"),
        &plan.nodes[0].outputs[0].output_id,
        &digest_bytes(b"declared-wrong"),
    );
    plan.nodes[0].content_hash_inputs = vec![ProjectPlanHashRef {
        ref_id: "input.source.csv".to_string(),
        content_hash: digest_bytes(b"declared-wrong"),
    }];
    refresh_node_cache_key(&mut plan.nodes[0]);
    plan.graph_hash = digest_bytes(b"copy-digest-mismatch");
    let mut policy = ProjectRunPolicy::new(temp.path(), "work");
    policy.failure_policy = ProjectRunFailurePolicy::CollectIndependentFailures;

    let report =
        run_project_plan_with_registered_executors(&plan, &policy).expect("failure report");

    assert_eq!(report.failed_nodes, vec!["alpha".to_string()]);
    assert!(!artifact_path(temp.path(), &plan, "alpha").exists());
    let receipt =
        read_node_receipt(&receipt_path(temp.path(), "alpha")).expect("failure receipt validates");
    assert_eq!(receipt.outcome, ProjectRunNodeOutcome::Failed);
    assert!(receipt.outputs.is_empty());
}

#[test]
fn receipt_failure_preserves_artifact_cas_without_publishing_mutable_head() {
    let temp = tempfile::tempdir().expect("tempdir");
    let input_bytes = b"id,name\n1,Alice\n";
    write_workspace_file(temp.path(), "input/source.csv", input_bytes);
    let mut plan = single_node_plan();
    configure_internal_copy_node(&mut plan, "alpha", "input/source.csv", input_bytes);
    fs::create_dir_all(temp.path().join("work")).expect("work dir");
    fs::write(
        temp.path().join("work/receipts"),
        b"receipt parent collision",
    )
    .expect("receipt parent collision");

    let error = run_project_plan_with_registered_executors(
        &plan,
        &ProjectRunPolicy::new(temp.path(), "work"),
    )
    .expect_err("semantic receipt publication fails after artifact CAS preservation");

    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert!(
        !artifact_path(temp.path(), &plan, "alpha").exists(),
        "mutable head is not published before semantic receipt convergence"
    );
    let output_receipt = receipt::ProjectRunOutputReceipt {
        output_id: plan.nodes[0].outputs[0].output_id.clone(),
        path: plan.nodes[0].outputs[0].path.clone(),
        content_digest: digest_bytes(input_bytes),
        byte_count: input_bytes.len() as u64,
    };
    assert_eq!(
        fs::read(artifact_cas_path_for_test(temp.path(), &output_receipt))
            .expect("artifact CAS preserved"),
        input_bytes
    );
    assert!(!receipt_path(temp.path(), "alpha").exists());
}

#[test]
fn registered_executor_target_scope_reuses_only_selected_closure() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_workspace_file(temp.path(), "input/alpha.csv", b"id\nalpha\n");
    write_workspace_file(temp.path(), "input/beta.csv", b"id\nbeta\n");
    let mut plan = independent_plan();
    configure_internal_copy_node(&mut plan, "alpha", "input/alpha.csv", b"id\nalpha\n");
    configure_internal_copy_node(&mut plan, "beta", "input/beta.csv", b"id\nbeta\n");
    let mut policy = ProjectRunPolicy::new(temp.path(), "work");
    policy.selected_nodes.insert("alpha".to_string());

    let first =
        run_project_plan_with_registered_executors(&plan, &policy).expect("selected alpha run");

    assert_eq!(first.executed_nodes, vec!["alpha".to_string()]);
    assert!(first.resumed_nodes.is_empty());
    assert_eq!(first.receipt.completed_nodes, vec!["alpha".to_string()]);
    assert!(artifact_path(temp.path(), &plan, "alpha").exists());
    assert!(!artifact_path(temp.path(), &plan, "beta").exists());
    assert!(!receipt_path(temp.path(), "beta").exists());

    let second =
        run_project_plan_with_registered_executors(&plan, &policy).expect("selected alpha reuse");

    assert!(second.executed_nodes.is_empty());
    assert_eq!(second.resumed_nodes, vec!["alpha".to_string()]);
    assert_eq!(second.receipt.completed_nodes, vec!["alpha".to_string()]);
    assert!(!artifact_path(temp.path(), &plan, "beta").exists());
}

#[test]
fn selected_reuse_does_not_restore_populated_unselected_cache() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan_a = independent_plan();
    let policy = approving_policy(temp.path());
    let mut executor_a = DeterministicExecutor::default();
    run_project_plan(&plan_a, &policy, &mut executor_a).expect("revision A runs");

    let mut plan_b = plan_a.clone();
    change_alpha_identity(&mut plan_b);
    let mut executor_b = DeterministicExecutor::default();
    run_project_plan(&plan_b, &policy, &mut executor_b).expect("revision B runs");
    let alpha_b_bytes =
        fs::read(artifact_path(temp.path(), &plan_b, "alpha")).expect("revision B alpha output");
    let alpha_b_receipt =
        read_node_receipt(&receipt_path(temp.path(), "alpha")).expect("revision B alpha receipt");

    let mut selected_policy = policy.clone();
    selected_policy.selected_nodes.insert("beta".to_string());
    let mut selected_executor = DeterministicExecutor::default();
    let selected = run_project_plan(&plan_a, &selected_policy, &mut selected_executor)
        .expect("selected beta reuses without touching alpha");

    assert!(selected_executor.calls.is_empty());
    assert_eq!(selected.resumed_nodes, vec!["beta".to_string()]);
    assert_eq!(
        fs::read(artifact_path(temp.path(), &plan_b, "alpha")).expect("alpha head remains B"),
        alpha_b_bytes
    );
    assert_eq!(
        read_node_receipt(&receipt_path(temp.path(), "alpha"))
            .expect("alpha receipt remains B")
            .receipt_hash,
        alpha_b_receipt.receipt_hash
    );
}

#[test]
fn registered_executor_preflight_does_not_restore_before_pending_executor_refusal() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan_a = independent_plan();
    let policy = approving_policy(temp.path());
    let mut executor_a = DeterministicExecutor::default();
    run_project_plan(&plan_a, &policy, &mut executor_a).expect("revision A runs");

    let mut plan_b = plan_a.clone();
    change_alpha_identity(&mut plan_b);
    let mut executor_b = DeterministicExecutor::default();
    run_project_plan(&plan_b, &policy, &mut executor_b).expect("revision B runs");
    let alpha_b_bytes =
        fs::read(artifact_path(temp.path(), &plan_b, "alpha")).expect("revision B alpha output");

    let mut plan_with_pending_beta = plan_a.clone();
    let beta = plan_with_pending_beta
        .nodes
        .iter_mut()
        .find(|node| node.node_id == "beta")
        .expect("beta node");
    beta.content_hash_inputs[0].content_hash = digest_bytes(b"changed-beta-input");
    refresh_node_cache_key(beta);
    plan_with_pending_beta.graph_hash = digest_bytes(b"pending-beta-preflight");

    let error = run_project_plan_with_registered_executors(
        &plan_with_pending_beta,
        &ProjectRunPolicy::new(temp.path(), "work"),
    )
    .expect_err("unregistered beta refuses during read-only preflight");

    assert_eq!(error.code, ProjectRunErrorCode::ExecutionFailed);
    assert_eq!(
        fs::read(artifact_path(temp.path(), &plan_b, "alpha")).expect("alpha head remains B"),
        alpha_b_bytes
    );
}

#[test]
fn cross_project_receipts_do_not_recover_existing_outputs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_node_plan();
    let policy = approving_policy(temp.path());
    let original_bytes = b"project-owned-output".to_vec();
    let mut executor = FixedOutputExecutor::new(original_bytes.clone());
    run_project_plan(&plan, &policy, &mut executor).expect("initial run");

    let path = receipt_path(temp.path(), "alpha");
    let mut foreign_receipt = read_node_receipt(&path).expect("current receipt");
    foreign_receipt.project_id = "other-project".to_string();
    let foreign_receipt = finalized_node_receipt(foreign_receipt).expect("foreign receipt");
    let foreign_bytes = canonical_node_receipt_bytes(&foreign_receipt).expect("foreign bytes");
    fs::write(&path, &foreign_bytes).expect("write foreign receipt");

    let mut resume_executor = FixedOutputExecutor::new(b"replacement-output".to_vec());
    let error = run_project_plan(&plan, &policy, &mut resume_executor)
        .expect_err("cross-project final receipt refuses");

    assert!(resume_executor.calls.is_empty());
    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert_eq!(
        fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("artifact"),
        original_bytes
    );
    assert_eq!(fs::read(path).expect("receipt bytes"), foreign_bytes);
}

#[test]
fn foreign_receipt_in_canonical_slot_refuses_before_executor_or_artifact() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_node_plan();
    let policy = approving_policy(temp.path());
    let mut foreign_receipt = standalone_receipt(temp.path(), "alpha");
    foreign_receipt.project_id = "foreign-project".to_string();
    let foreign_receipt = finalized_node_receipt(foreign_receipt).expect("foreign finalizes");
    let foreign_bytes = canonical_node_receipt_bytes(&foreign_receipt).expect("foreign bytes");
    let slot = receipt_path(temp.path(), "alpha");
    fs::create_dir_all(slot.parent().expect("slot parent")).expect("slot parent");
    fs::write(&slot, &foreign_bytes).expect("foreign slot");

    let mut executor = FixedOutputExecutor::new(b"must-not-publish".to_vec());
    let error = run_project_plan(&plan, &policy, &mut executor).expect_err("foreign poison");

    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert!(error.message.contains("receipt belongs to project_id"));
    assert!(executor.calls.is_empty());
    assert!(!artifact_path(temp.path(), &plan, "alpha").exists());
    assert_eq!(fs::read(slot).expect("slot preserved"), foreign_bytes);
}

#[test]
fn old_v1_receipts_are_refused_actionably() {
    let error = parse_node_receipt(br#"{"schema_version":"canon.project.run.v1"}"#)
        .expect_err("v1 receipt refused before v2 parse");

    assert_eq!(error.code, ProjectReceiptErrorCode::ArtifactContract);
    assert!(error.message.contains("canon.project.run.v1"));
    assert!(error.message.contains("canon.project.run.v2"));
    assert!(error.message.contains("not execution-reusable"));
}

#[test]
fn stale_matching_output_temp_is_published_on_retry() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_node_plan();
    let intended = b"publish-from-temp".to_vec();
    let temp_path = output_temp_path_for_test(temp.path(), "work", "work/alpha.json", &intended);
    fs::create_dir_all(temp_path.parent().expect("temp parent")).expect("temp parent dir");
    fs::write(&temp_path, &intended).expect("stale matching temp");

    let mut executor = FixedOutputExecutor::new(intended.clone());
    let report = run_project_plan(&plan, &approving_policy(temp.path()), &mut executor)
        .expect("matching temp recovers");

    assert_eq!(report.executed_nodes, vec!["alpha".to_string()]);
    assert!(report.failed_nodes.is_empty());
    assert_eq!(
        fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("artifact"),
        intended
    );
    assert!(!temp_path.exists());
}

#[test]
fn stale_mismatched_output_temp_refuses_without_artifact() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_node_plan();
    let intended = b"intended-output".to_vec();
    let temp_path = output_temp_path_for_test(temp.path(), "work", "work/alpha.json", &intended);
    fs::create_dir_all(temp_path.parent().expect("temp parent")).expect("temp parent dir");
    fs::write(&temp_path, b"wrong-output").expect("stale mismatched temp");

    let mut executor = FixedOutputExecutor::new(intended);
    let report = run_project_plan(&plan, &approving_policy(temp.path()), &mut executor)
        .expect("mismatched temp produces failure report");

    assert_eq!(report.failed_nodes, vec!["alpha".to_string()]);
    assert!(!artifact_path(temp.path(), &plan, "alpha").exists());
    assert_eq!(
        fs::read(&temp_path).expect("temp preserved"),
        b"wrong-output"
    );
    assert!(
        report.node_reports[0]
            .reason
            .as_deref()
            .expect("failure reason")
            .contains("refusing to reuse deterministic temp artifact")
    );
}

#[test]
fn already_published_intended_output_is_recovered_to_receipt() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut plan = single_node_plan();
    let policy = approving_policy(temp.path());
    let mut first_executor = FixedOutputExecutor::new(b"old-output".to_vec());
    run_project_plan(&plan, &policy, &mut first_executor).expect("initial run");

    change_alpha_identity(&mut plan);
    let intended = b"new-output-after-interruption".to_vec();
    fs::write(artifact_path(temp.path(), &plan, "alpha"), &intended)
        .expect("already published intended artifact");
    let mut resume_executor = FixedOutputExecutor::new(intended.clone());
    let report = run_project_plan(&plan, &policy, &mut resume_executor)
        .expect("already published artifact recovers");

    assert_eq!(resume_executor.calls, vec!["alpha".to_string()]);
    assert_eq!(report.executed_nodes, vec!["alpha".to_string()]);
    assert!(report.failed_nodes.is_empty());
    let receipt = read_node_receipt(&receipt_path(temp.path(), "alpha")).expect("receipt");
    assert_eq!(receipt.outputs[0].content_digest, digest_bytes(&intended));
    assert_eq!(
        fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("artifact"),
        intended
    );
}

#[test]
fn mismatched_existing_output_is_not_overwritten_during_recovery() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut plan = single_node_plan();
    let policy = approving_policy(temp.path());
    let mut first_executor = FixedOutputExecutor::new(b"old-output".to_vec());
    run_project_plan(&plan, &policy, &mut first_executor).expect("initial run");

    change_alpha_identity(&mut plan);
    let artifact = artifact_path(temp.path(), &plan, "alpha");
    fs::write(&artifact, b"foreign-output").expect("foreign overwrite");
    let intended = b"new-output".to_vec();
    let temp_path = output_temp_path_for_test(temp.path(), "work", "work/alpha.json", &intended);
    let mut resume_executor = FixedOutputExecutor::new(intended);
    let report = run_project_plan(&plan, &policy, &mut resume_executor)
        .expect("mismatched existing artifact produces failure report");

    assert_eq!(resume_executor.calls, vec!["alpha".to_string()]);
    assert_eq!(report.failed_nodes, vec!["alpha".to_string()]);
    assert_eq!(fs::read(&artifact).expect("artifact"), b"foreign-output");
    assert!(!temp_path.exists());
    assert!(
        report.node_reports[0]
            .reason
            .as_deref()
            .expect("failure reason")
            .contains("no longer matches the recoverable prior receipt")
    );
}

#[test]
fn stale_matching_receipt_temp_is_published_on_retry() {
    let temp = tempfile::tempdir().expect("tempdir");
    let receipt = standalone_receipt(temp.path(), "manual");
    let path = temp
        .path()
        .join("work")
        .join("receipts")
        .join("manual.json");
    let bytes = canonical_node_receipt_bytes(&receipt).expect("receipt bytes");
    let temp_path = receipt_temp_path_for_test(&path, &bytes);
    fs::create_dir_all(temp_path.parent().expect("temp parent")).expect("temp parent dir");
    fs::write(&temp_path, &bytes).expect("stale receipt temp");

    write_node_receipt(&path, &receipt).expect("matching receipt temp recovers");

    assert_eq!(fs::read(&path).expect("receipt"), bytes);
    assert!(!temp_path.exists());
}

#[test]
fn stale_mismatched_receipt_temp_refuses_without_publish() {
    let temp = tempfile::tempdir().expect("tempdir");
    let receipt = standalone_receipt(temp.path(), "manual");
    let path = temp
        .path()
        .join("work")
        .join("receipts")
        .join("manual.json");
    let bytes = canonical_node_receipt_bytes(&receipt).expect("receipt bytes");
    let temp_path = receipt_temp_path_for_test(&path, &bytes);
    fs::create_dir_all(temp_path.parent().expect("temp parent")).expect("temp parent dir");
    fs::write(&temp_path, b"wrong-receipt").expect("stale receipt temp");

    let error = write_node_receipt(&path, &receipt).expect_err("mismatched receipt temp refuses");

    assert_eq!(error.code, ProjectReceiptErrorCode::Io);
    assert!(!path.exists());
    assert_eq!(
        fs::read(&temp_path).expect("temp preserved"),
        b"wrong-receipt"
    );
}

#[test]
fn mismatched_existing_final_receipt_is_not_overwritten() {
    let temp = tempfile::tempdir().expect("tempdir");
    let receipt = standalone_receipt(temp.path(), "manual");
    let mut conflicting = receipt.clone();
    conflicting.duration_millis = 42;
    let conflicting = finalized_node_receipt(conflicting).expect("conflicting receipt finalizes");
    let path = temp
        .path()
        .join("work")
        .join("receipts")
        .join("manual.json");
    let intended_bytes = canonical_node_receipt_bytes(&receipt).expect("receipt bytes");
    let conflicting_bytes =
        canonical_node_receipt_bytes(&conflicting).expect("conflicting receipt bytes");
    fs::create_dir_all(path.parent().expect("receipt parent")).expect("receipt parent dir");
    fs::write(&path, &conflicting_bytes).expect("conflicting final receipt");

    let error = write_node_receipt(&path, &receipt).expect_err("final conflict refuses");

    assert_eq!(error.code, ProjectReceiptErrorCode::Io);
    assert!(
        error
            .message
            .contains("refusing to replace existing project receipt")
    );
    assert_eq!(fs::read(&path).expect("final receipt"), conflicting_bytes);
    assert!(!receipt_temp_path_for_test(&path, &intended_bytes).exists());
}

#[test]
fn active_publication_lock_refuses_without_exposing_or_overwriting_canonical_receipt() {
    let temp = tempfile::tempdir().expect("tempdir");
    let receipt = standalone_receipt(temp.path(), "manual");
    let path = temp
        .path()
        .join("work")
        .join("receipts")
        .join("manual.json");
    let lock_path = path.with_file_name(".manual.json.publish.lock");
    fs::create_dir_all(path.parent().expect("receipt parent")).expect("receipt parent dir");
    fs::write(&lock_path, b"").expect("active publication lock");

    let error = write_node_receipt(&path, &receipt).expect_err("active writer must win");

    assert_eq!(error.code, ProjectReceiptErrorCode::Io);
    assert!(error.message.contains("concurrent publication"));
    assert!(
        !path.exists(),
        "no partial canonical receipt may be exposed"
    );
    assert!(
        node_receipt_cas_path(&path, &receipt.receipt_hash).exists(),
        "the intended immutable receipt remains recoverable even when the canonical slot is busy"
    );
}

#[test]
fn semantically_equivalent_concurrent_completed_receipt_dedupes_without_overwrite() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_node_plan();
    let mut executor =
        PrewritingReceiptExecutor::semantic_duplicate(temp.path(), &plan, b"same-output".to_vec());

    let report = run_project_plan(&plan, &approving_policy(temp.path()), &mut executor)
        .expect("semantic duplicate receipt converges");

    let slot = receipt_path(temp.path(), "alpha");
    let disk = read_node_receipt(&slot).expect("canonical receipt");
    let prewritten_hash = executor
        .prewritten_receipt_hash
        .as_deref()
        .expect("prewritten hash");
    let intended_hash = executor
        .intended_receipt_hash
        .as_deref()
        .expect("intended hash");
    assert_eq!(report.executed_nodes, vec!["alpha".to_string()]);
    assert!(report.failed_nodes.is_empty());
    assert_eq!(disk.receipt_hash, prewritten_hash);
    assert_ne!(disk.receipt_hash, intended_hash);
    assert_eq!(
        disk.semantic_hash,
        executor
            .intended_semantic_hash
            .as_deref()
            .expect("intended semantic")
    );
    assert_eq!(
        report.receipt.node_receipts[0].receipt_hash,
        disk.receipt_hash
    );
    assert_eq!(
        fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("artifact"),
        b"same-output"
    );
    assert!(node_receipt_cas_path(&slot, prewritten_hash).exists());
    assert!(node_receipt_cas_path(&slot, intended_hash).exists());
}

#[test]
fn semantically_conflicting_concurrent_receipt_refuses_without_losing_receipts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_node_plan();
    let mut executor =
        PrewritingReceiptExecutor::semantic_conflict(temp.path(), &plan, b"same-output".to_vec());

    let error = run_project_plan(&plan, &approving_policy(temp.path()), &mut executor)
        .expect_err("semantic conflict refuses");

    let slot = receipt_path(temp.path(), "alpha");
    let disk = read_node_receipt(&slot).expect("canonical receipt");
    let prewritten_hash = executor
        .prewritten_receipt_hash
        .as_deref()
        .expect("prewritten hash");
    let intended_hash = executor
        .intended_receipt_hash
        .as_deref()
        .expect("intended hash");
    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert!(error.message.contains("different semantic result"));
    assert_eq!(disk.receipt_hash, prewritten_hash);
    assert_ne!(
        disk.semantic_hash,
        executor
            .intended_semantic_hash
            .as_deref()
            .expect("intended semantic")
    );
    assert_eq!(
        fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("artifact"),
        b"same-output"
    );
    assert!(node_receipt_cas_path(&slot, prewritten_hash).exists());
    assert!(node_receipt_cas_path(&slot, intended_hash).exists());
}

#[test]
fn divergent_output_for_same_semantic_cache_key_refuses_before_head_publication() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_node_plan();
    let mut executor = PrewritingSemanticCacheExecutor::new(
        temp.path(),
        &plan,
        b"intended-output".to_vec(),
        b"divergent-output".to_vec(),
    );

    let error = run_project_plan(&plan, &approving_policy(temp.path()), &mut executor)
        .expect_err("same semantic cache key with divergent output refuses");

    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert!(error.message.contains("different semantic result"));
    assert!(executor.called);
    assert!(
        !artifact_path(temp.path(), &plan, "alpha").exists(),
        "a divergent semantic result must be refused before mutable head publication"
    );
}

#[test]
fn semantic_duplicate_with_different_output_path_refuses_without_false_binding_reuse() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_node_plan();
    let mut executor = PrewritingReceiptExecutor::output_path_binding_mismatch(
        temp.path(),
        &plan,
        b"same-output".to_vec(),
    );

    let error = run_project_plan(&plan, &approving_policy(temp.path()), &mut executor)
        .expect_err("output path binding mismatch refuses");

    let slot = receipt_path(temp.path(), "alpha");
    let disk = read_node_receipt(&slot).expect("canonical receipt");
    let prewritten_hash = executor
        .prewritten_receipt_hash
        .as_deref()
        .expect("prewritten hash");
    let intended_hash = executor
        .intended_receipt_hash
        .as_deref()
        .expect("intended hash");
    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert!(error.message.contains("operational binding"));
    assert_eq!(disk.receipt_hash, prewritten_hash);
    assert_ne!(disk.receipt_hash, intended_hash);
    assert_eq!(
        disk.semantic_hash,
        executor
            .intended_semantic_hash
            .as_deref()
            .expect("intended semantic")
    );
    assert_ne!(disk.outputs[0].path, plan.nodes[0].outputs[0].path);
    assert_eq!(
        fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("intended artifact"),
        b"same-output"
    );
    assert!(node_receipt_cas_path(&slot, prewritten_hash).exists());
    assert!(node_receipt_cas_path(&slot, intended_hash).exists());
}

#[test]
fn semantic_duplicate_with_different_dependency_receipt_hash_refuses_without_false_binding_reuse() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = chain_plan();
    let mut executor = PrewritingReceiptExecutor::dependency_receipt_binding_mismatch(
        temp.path(),
        &plan,
        "beta",
        b"same-output".to_vec(),
    );

    let error = run_project_plan(&plan, &approving_policy(temp.path()), &mut executor)
        .expect_err("dependency receipt binding mismatch refuses");

    let alpha = read_node_receipt(&receipt_path(temp.path(), "alpha")).expect("alpha receipt");
    let beta_slot = receipt_path(temp.path(), "beta");
    let beta = read_node_receipt(&beta_slot).expect("canonical beta receipt");
    let prewritten_hash = executor
        .prewritten_receipt_hash
        .as_deref()
        .expect("prewritten hash");
    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert!(error.message.contains("operational binding"));
    assert_eq!(beta.receipt_hash, prewritten_hash);
    assert_eq!(
        beta.semantic_hash,
        executor
            .intended_semantic_hash
            .as_deref()
            .expect("intended semantic")
    );
    assert_ne!(
        beta.dependency_receipt_hashes.get("alpha"),
        Some(&alpha.receipt_hash)
    );
    assert!(node_receipt_cas_path(&beta_slot, prewritten_hash).exists());
    assert!(
        cas_contains_receipt_matching(&beta_slot, |receipt| {
            receipt.node_id == "beta"
                && receipt.semantic_hash == beta.semantic_hash
                && receipt.dependency_receipt_hashes.get("alpha") == Some(&alpha.receipt_hash)
        }),
        "the intended beta receipt with the executed dependency receipt binding remains in CAS"
    );
}

#[test]
fn concurrently_created_conflicting_output_bytes_refuse_without_overwrite() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_node_plan();
    let mut executor = ConcurrentArtifactWriterExecutor::new(
        temp.path(),
        b"intended-output".to_vec(),
        b"winner-output".to_vec(),
    );

    let report = run_project_plan(&plan, &approving_policy(temp.path()), &mut executor)
        .expect("artifact race records failed node");

    let artifact = artifact_path(temp.path(), &plan, "alpha");
    assert_eq!(report.failed_nodes, vec!["alpha".to_string()]);
    assert!(report.executed_nodes.is_empty());
    assert_eq!(fs::read(&artifact).expect("artifact"), b"winner-output");
    let receipt = read_node_receipt(&receipt_path(temp.path(), "alpha")).expect("failure receipt");
    assert_eq!(receipt.outcome, ProjectRunNodeOutcome::Failed);
    assert!(receipt.outputs.is_empty());
    assert!(
        report.node_reports[0]
            .reason
            .as_deref()
            .expect("failure reason")
            .contains("concurrently created artifact")
    );
}

#[test]
fn active_output_publication_lock_refuses_without_exposing_artifact_or_completed_receipt() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_node_plan();
    let artifact = artifact_path(temp.path(), &plan, "alpha");
    fs::create_dir_all(artifact.parent().expect("artifact parent")).expect("artifact parent");
    let artifact_name = artifact
        .file_name()
        .and_then(|name| name.to_str())
        .expect("artifact file name");
    let lock_path = artifact.with_file_name(format!(".{artifact_name}.publish.lock"));
    fs::write(&lock_path, b"active publisher").expect("active output publication lock");
    let mut executor = FixedOutputExecutor::new(b"intended-output".to_vec());

    let report = run_project_plan(&plan, &approving_policy(temp.path()), &mut executor)
        .expect("active artifact publisher records failed node");

    assert_eq!(executor.calls, vec!["alpha".to_string()]);
    assert_eq!(report.failed_nodes, vec!["alpha".to_string()]);
    assert!(report.executed_nodes.is_empty());
    assert!(
        !artifact.exists(),
        "the active publisher's slot is not overwritten"
    );
    let receipt = read_node_receipt(&receipt_path(temp.path(), "alpha")).expect("failure receipt");
    assert_eq!(receipt.outcome, ProjectRunNodeOutcome::Failed);
    assert!(receipt.outputs.is_empty());
    assert!(
        report.node_reports[0]
            .reason
            .as_deref()
            .expect("failure reason")
            .contains("concurrent publication")
    );
}

#[test]
fn declared_network_effect_refuses_before_executor_invocation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut plan = single_node_plan();
    let node = &mut plan.nodes[0];
    node.side_effects.push(ProjectPlanSideEffect {
        kind: ProjectPlanSideEffectKind::MayUseNetwork,
        description: "declared extension acquisition".to_string(),
    });
    refresh_node_cache_key(node);
    plan.graph_hash = digest_bytes(b"declared-network-effect");
    let policy = approving_policy(temp.path());
    let mut executor = FixedOutputExecutor::new(b"should-not-run".to_vec());

    let report = run_project_plan(&plan, &policy, &mut executor)
        .expect("declared network effect produces failure report");

    assert!(executor.calls.is_empty());
    assert_eq!(report.failed_nodes, vec!["alpha".to_string()]);
    assert!(
        report.node_reports[0]
            .reason
            .as_deref()
            .expect("failure reason")
            .contains("declared node effects exceed project run policy")
    );
    assert!(
        report.node_reports[0]
            .reason
            .as_deref()
            .expect("failure reason")
            .contains("network access")
    );
}

#[test]
fn declared_mutation_effect_refuses_before_executor_invocation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut plan = single_node_plan();
    let node = &mut plan.nodes[0];
    node.side_effects.push(ProjectPlanSideEffect {
        kind: ProjectPlanSideEffectKind::MutatesRegistry,
        description: "declared extension registry mutation".to_string(),
    });
    refresh_node_cache_key(node);
    plan.graph_hash = digest_bytes(b"declared-mutation-effect");
    let mut policy = ProjectRunPolicy::new(temp.path(), "work");
    policy.failure_policy = ProjectRunFailurePolicy::CollectIndependentFailures;
    let mut executor = FixedOutputExecutor::new(b"should-not-run".to_vec());

    let report = run_project_plan(&plan, &policy, &mut executor)
        .expect("declared mutation effect produces failure report");

    assert!(executor.calls.is_empty());
    assert_eq!(report.failed_nodes, vec!["alpha".to_string()]);
    assert!(
        report.node_reports[0]
            .reason
            .as_deref()
            .expect("failure reason")
            .contains("registry mutation")
    );
}

#[test]
fn declared_external_materialization_class_refuses_before_executor_invocation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut plan = single_node_plan();
    let node = &mut plan.nodes[0];
    node.class = ProjectPlanNodeClass::ExternalMaterialization;
    refresh_node_cache_key(node);
    plan.graph_hash = digest_bytes(b"declared-external-materialization-class");
    let policy = approving_policy(temp.path());
    let mut executor = FixedOutputExecutor::new(b"should-not-run".to_vec());

    let report = run_project_plan(&plan, &policy, &mut executor)
        .expect("declared external materialization class produces failure report");

    assert!(executor.calls.is_empty());
    assert_eq!(report.failed_nodes, vec!["alpha".to_string()]);
    assert!(
        report.node_reports[0]
            .reason
            .as_deref()
            .expect("failure reason")
            .contains("external materialization nodes require declared network permission")
    );
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
                    context.dependency_semantic_hashes
                )
                .into_bytes(),
            );
        }
        let mut result = ProjectNodeExecutionResult::with_outputs(outputs);
        result
            .deterministic_usage
            .insert("output_count".to_string(), node.outputs.len() as u64);
        result.duration_millis = node.node_id.len() as u64;
        result
            .resource_observations
            .insert("output_count".to_string(), node.outputs.len() as u64);
        Ok(result)
    }
}

struct TelemetryExecutor {
    duration_millis: u64,
    observed_rows: u64,
    deterministic_rows: u64,
}

impl TelemetryExecutor {
    fn new(duration_millis: u64, observed_rows: u64, deterministic_rows: u64) -> Self {
        Self {
            duration_millis,
            observed_rows,
            deterministic_rows,
        }
    }
}

impl ProjectNodeExecutor for TelemetryExecutor {
    fn execute(
        &mut self,
        node: &ProjectPlanNode,
        context: &ProjectNodeExecutionContext,
    ) -> Result<ProjectNodeExecutionResult, ProjectRunError> {
        let mut outputs = BTreeMap::new();
        for output in &node.outputs {
            outputs.insert(
                output.output_id.clone(),
                format!(
                    "node={}\noutput={}\ncache={}\ndeps={:?}\n",
                    node.node_id,
                    output.output_id,
                    node.cache.cache_key,
                    context.dependency_semantic_hashes
                )
                .into_bytes(),
            );
        }
        let mut result = ProjectNodeExecutionResult::with_outputs(outputs);
        result.duration_millis = self.duration_millis;
        result
            .resource_observations
            .insert("observed_rows".to_string(), self.observed_rows);
        result
            .deterministic_usage
            .insert("rows_examined".to_string(), self.deterministic_rows);
        Ok(result)
    }
}

#[derive(Default)]
struct ContextRecordingExecutor {
    dependency_contexts: BTreeMap<String, BTreeMap<String, String>>,
    dependency_outputs: BTreeMap<String, BTreeMap<String, Vec<run::ProjectDependencyOutput>>>,
}

impl ProjectNodeExecutor for ContextRecordingExecutor {
    fn execute(
        &mut self,
        node: &ProjectPlanNode,
        context: &ProjectNodeExecutionContext,
    ) -> Result<ProjectNodeExecutionResult, ProjectRunError> {
        self.dependency_contexts.insert(
            context.node_id.clone(),
            context.dependency_semantic_hashes.clone(),
        );
        self.dependency_outputs
            .insert(context.node_id.clone(), context.dependency_outputs.clone());
        let mut outputs = BTreeMap::new();
        for output in &node.outputs {
            outputs.insert(
                output.output_id.clone(),
                format!(
                    "node={}\noutput={}\ndeps={:?}\n",
                    node.node_id, output.output_id, context.dependency_semantic_hashes
                )
                .into_bytes(),
            );
        }
        Ok(ProjectNodeExecutionResult::with_outputs(outputs))
    }
}

struct FixedOutputExecutor {
    bytes: Vec<u8>,
    calls: Vec<String>,
}

impl FixedOutputExecutor {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            calls: Vec::new(),
        }
    }
}

impl ProjectNodeExecutor for FixedOutputExecutor {
    fn execute(
        &mut self,
        node: &ProjectPlanNode,
        _context: &ProjectNodeExecutionContext,
    ) -> Result<ProjectNodeExecutionResult, ProjectRunError> {
        self.calls.push(node.node_id.clone());
        let mut outputs = BTreeMap::new();
        for output in &node.outputs {
            outputs.insert(output.output_id.clone(), self.bytes.clone());
        }
        let mut result = ProjectNodeExecutionResult::with_outputs(outputs);
        result
            .deterministic_usage
            .insert("fixed_bytes".to_string(), self.bytes.len() as u64);
        Ok(result)
    }
}

#[derive(Clone, Copy)]
enum PrewriteReceiptMode {
    SemanticDuplicate,
    SemanticConflict,
    OutputPathBindingMismatch,
    DependencyReceiptBindingMismatch,
}

struct PrewritingReceiptExecutor {
    workspace_root: PathBuf,
    project_id: String,
    plan_graph_hash: String,
    bytes: Vec<u8>,
    mode: PrewriteReceiptMode,
    target_node_id: Option<String>,
    prewritten_receipt_hash: Option<String>,
    intended_receipt_hash: Option<String>,
    intended_semantic_hash: Option<String>,
}

impl PrewritingReceiptExecutor {
    fn semantic_duplicate(root: &Path, plan: &ProjectPlan, bytes: Vec<u8>) -> Self {
        Self::new(root, plan, bytes, PrewriteReceiptMode::SemanticDuplicate)
    }

    fn semantic_conflict(root: &Path, plan: &ProjectPlan, bytes: Vec<u8>) -> Self {
        Self::new(root, plan, bytes, PrewriteReceiptMode::SemanticConflict)
    }

    fn output_path_binding_mismatch(root: &Path, plan: &ProjectPlan, bytes: Vec<u8>) -> Self {
        Self::new(
            root,
            plan,
            bytes,
            PrewriteReceiptMode::OutputPathBindingMismatch,
        )
    }

    fn dependency_receipt_binding_mismatch(
        root: &Path,
        plan: &ProjectPlan,
        target_node_id: &str,
        bytes: Vec<u8>,
    ) -> Self {
        let mut executor = Self::new(
            root,
            plan,
            bytes,
            PrewriteReceiptMode::DependencyReceiptBindingMismatch,
        );
        executor.target_node_id = Some(target_node_id.to_string());
        executor
    }

    fn new(root: &Path, plan: &ProjectPlan, bytes: Vec<u8>, mode: PrewriteReceiptMode) -> Self {
        Self {
            workspace_root: root.to_path_buf(),
            project_id: plan.project_id.clone(),
            plan_graph_hash: plan.graph_hash.clone(),
            bytes,
            mode,
            target_node_id: None,
            prewritten_receipt_hash: None,
            intended_receipt_hash: None,
            intended_semantic_hash: None,
        }
    }
}

impl ProjectNodeExecutor for PrewritingReceiptExecutor {
    fn execute(
        &mut self,
        node: &ProjectPlanNode,
        context: &ProjectNodeExecutionContext,
    ) -> Result<ProjectNodeExecutionResult, ProjectRunError> {
        if let Some(target_node_id) = &self.target_node_id
            && target_node_id != &node.node_id
        {
            return Ok(fixed_output_result(node, &self.bytes));
        }

        let mut intended_usage = BTreeMap::new();
        intended_usage.insert("fixed_bytes".to_string(), self.bytes.len() as u64);
        let mut prewritten_usage = intended_usage.clone();
        if matches!(self.mode, PrewriteReceiptMode::SemanticConflict) {
            prewritten_usage.insert("fixed_bytes".to_string(), self.bytes.len() as u64 + 1);
        }

        let mut prewritten = completed_test_receipt(
            &self.project_id,
            &self.plan_graph_hash,
            node,
            context,
            &self.bytes,
            prewritten_usage,
            999,
        );
        let intended = completed_test_receipt(
            &self.project_id,
            &self.plan_graph_hash,
            node,
            context,
            &self.bytes,
            intended_usage.clone(),
            1,
        );
        self.prewritten_receipt_hash = Some(prewritten.receipt_hash.clone());
        if !matches!(
            self.mode,
            PrewriteReceiptMode::DependencyReceiptBindingMismatch
        ) {
            self.intended_receipt_hash = Some(intended.receipt_hash.clone());
        }
        self.intended_semantic_hash = Some(intended.semantic_hash.clone());

        match self.mode {
            PrewriteReceiptMode::SemanticDuplicate | PrewriteReceiptMode::SemanticConflict => {}
            PrewriteReceiptMode::OutputPathBindingMismatch => {
                for output in &mut prewritten.outputs {
                    output.path =
                        format!("work/prewritten-{}-{}.json", node.node_id, output.output_id);
                }
                prewritten =
                    finalized_node_receipt(prewritten).expect("path-mismatched receipt finalizes");
                self.prewritten_receipt_hash = Some(prewritten.receipt_hash.clone());
            }
            PrewriteReceiptMode::DependencyReceiptBindingMismatch => {
                let dependency_id = node
                    .dependencies
                    .first()
                    .expect("dependency mismatch target has a dependency");
                prewritten.dependency_receipt_hashes.insert(
                    dependency_id.clone(),
                    digest_bytes(format!("competing-{dependency_id}-receipt").as_bytes()),
                );
                prewritten = finalized_node_receipt(prewritten)
                    .expect("dependency-mismatched receipt finalizes");
                self.prewritten_receipt_hash = Some(prewritten.receipt_hash.clone());
            }
        }

        let slot = receipt_path(&self.workspace_root, &node.node_id);
        fs::create_dir_all(slot.parent().expect("receipt parent")).expect("receipt parent");
        fs::write(
            &slot,
            canonical_node_receipt_bytes(&prewritten).expect("prewritten bytes"),
        )
        .expect("prewrite concurrent receipt");

        let mut result = fixed_output_result(node, &self.bytes);
        result.duration_millis = 1;
        Ok(result)
    }
}

struct PrewritingSemanticCacheExecutor {
    workspace_root: PathBuf,
    project_id: String,
    plan_graph_hash: String,
    intended_bytes: Vec<u8>,
    divergent_bytes: Vec<u8>,
    called: bool,
}

impl PrewritingSemanticCacheExecutor {
    fn new(
        root: &Path,
        plan: &ProjectPlan,
        intended_bytes: Vec<u8>,
        divergent_bytes: Vec<u8>,
    ) -> Self {
        Self {
            workspace_root: root.to_path_buf(),
            project_id: plan.project_id.clone(),
            plan_graph_hash: plan.graph_hash.clone(),
            intended_bytes,
            divergent_bytes,
            called: false,
        }
    }
}

impl ProjectNodeExecutor for PrewritingSemanticCacheExecutor {
    fn execute(
        &mut self,
        node: &ProjectPlanNode,
        context: &ProjectNodeExecutionContext,
    ) -> Result<ProjectNodeExecutionResult, ProjectRunError> {
        self.called = true;
        let mut usage = BTreeMap::new();
        usage.insert("fixed_bytes".to_string(), self.divergent_bytes.len() as u64);
        let divergent = completed_test_receipt(
            &self.project_id,
            &self.plan_graph_hash,
            node,
            context,
            &self.divergent_bytes,
            usage,
            1,
        );
        let canonical = receipt_path(&self.workspace_root, &node.node_id);
        let result_cache_key = semantic_node_result_cache_key(
            &node.cache.cache_key,
            &context.dependency_semantic_hashes,
        )
        .expect("semantic result cache key");
        let semantic = semantic_node_receipt_path(&canonical, &result_cache_key);
        write_node_receipt(&semantic, &divergent).expect("prewrite divergent semantic receipt");
        Ok(fixed_output_result(node, &self.intended_bytes))
    }
}

fn fixed_output_result(node: &ProjectPlanNode, bytes: &[u8]) -> ProjectNodeExecutionResult {
    let mut outputs = BTreeMap::new();
    for output in &node.outputs {
        outputs.insert(output.output_id.clone(), bytes.to_vec());
    }
    let mut result = ProjectNodeExecutionResult::with_outputs(outputs);
    result
        .deterministic_usage
        .insert("fixed_bytes".to_string(), bytes.len() as u64);
    result
}

struct ConcurrentArtifactWriterExecutor {
    workspace_root: PathBuf,
    intended_bytes: Vec<u8>,
    concurrent_bytes: Vec<u8>,
}

impl ConcurrentArtifactWriterExecutor {
    fn new(root: &Path, intended_bytes: Vec<u8>, concurrent_bytes: Vec<u8>) -> Self {
        Self {
            workspace_root: root.to_path_buf(),
            intended_bytes,
            concurrent_bytes,
        }
    }
}

impl ProjectNodeExecutor for ConcurrentArtifactWriterExecutor {
    fn execute(
        &mut self,
        node: &ProjectPlanNode,
        _context: &ProjectNodeExecutionContext,
    ) -> Result<ProjectNodeExecutionResult, ProjectRunError> {
        let output = node.outputs.first().expect("single test output");
        let artifact = self.workspace_root.join(&output.path);
        fs::create_dir_all(artifact.parent().expect("artifact parent")).expect("artifact parent");
        fs::write(&artifact, &self.concurrent_bytes).expect("concurrent artifact");

        let mut outputs = BTreeMap::new();
        outputs.insert(output.output_id.clone(), self.intended_bytes.clone());
        Ok(ProjectNodeExecutionResult::with_outputs(outputs))
    }
}

fn completed_test_receipt(
    project_id: &str,
    plan_graph_hash: &str,
    node: &ProjectPlanNode,
    context: &ProjectNodeExecutionContext,
    bytes: &[u8],
    deterministic_usage: BTreeMap<String, u64>,
    duration_millis: u64,
) -> receipt::ProjectRunNodeReceipt {
    let mut content_hash_inputs = node
        .content_hash_inputs
        .iter()
        .map(|input| receipt::ProjectRunHashRef {
            ref_id: input.ref_id.clone(),
            content_hash: input.content_hash.clone(),
        })
        .collect::<Vec<_>>();
    content_hash_inputs.sort_by(|left, right| left.ref_id.cmp(&right.ref_id));
    let outputs = node
        .outputs
        .iter()
        .map(|output| receipt::ProjectRunOutputReceipt {
            output_id: output.output_id.clone(),
            path: output.path.clone(),
            content_digest: digest_bytes(bytes),
            byte_count: bytes.len() as u64,
        })
        .collect();
    finalized_node_receipt(receipt::ProjectRunNodeReceipt {
        schema_version: project_run_schema_version().to_string(),
        project_id: project_id.to_string(),
        plan_graph_hash: plan_graph_hash.to_string(),
        node_id: node.node_id.clone(),
        node_cache_key: node.cache.cache_key.clone(),
        content_hash_inputs,
        dependency_semantic_hashes: context.dependency_semantic_hashes.clone(),
        dependency_receipt_hashes: BTreeMap::new(),
        outputs,
        outcome: ProjectRunNodeOutcome::Completed,
        deterministic_usage,
        duration_millis,
        resource_observations: BTreeMap::new(),
        next_action: receipt::ProjectRunNextAction::ExecuteDependents,
        failure_code: None,
        failure_message: None,
        semantic_hash: String::new(),
        telemetry_hash: String::new(),
        receipt_hash: String::new(),
    })
    .expect("completed test receipt finalizes")
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

fn artifact_cas_path_for_test(root: &Path, output: &receipt::ProjectRunOutputReceipt) -> PathBuf {
    let digest_hex = output
        .content_digest
        .strip_prefix("blake3:")
        .expect("blake3 output digest");
    root.join("work")
        .join("artifacts")
        .join("cas")
        .join(format!("{digest_hex}.bin"))
}

fn semantic_receipt_path_for_test(
    canonical_path: &Path,
    receipt: &receipt::ProjectRunNodeReceipt,
) -> PathBuf {
    let result_cache_key = semantic_node_result_cache_key(
        &receipt.node_cache_key,
        &receipt.dependency_semantic_hashes,
    )
    .expect("semantic node-result cache key");
    semantic_node_receipt_path(canonical_path, &result_cache_key)
}

fn install_parent_telemetry_variant(
    root: &Path,
    plan: &ProjectPlan,
) -> (
    receipt::ProjectRunNodeReceipt,
    receipt::ProjectRunNodeReceipt,
) {
    let canonical = receipt_path(root, "alpha");
    let before = read_node_receipt(&canonical).expect("alpha receipt before telemetry variant");
    let semantic = semantic_receipt_path_for_test(&canonical, &before);
    let mut after = before.clone();
    after.duration_millis += 1_000;
    after
        .resource_observations
        .insert("telemetry_variant".to_string(), 1);
    after.plan_graph_hash = plan.graph_hash.clone();
    let after = finalized_node_receipt(after).expect("alpha telemetry variant finalizes");
    replace_node_receipt(&canonical, &after, Some(&before))
        .expect("replace canonical alpha telemetry receipt");
    replace_node_receipt(&semantic, &after, Some(&before))
        .expect("replace semantic alpha telemetry receipt");
    (before, after)
}

fn cas_contains_receipt_matching<F>(slot: &Path, predicate: F) -> bool
where
    F: Fn(&receipt::ProjectRunNodeReceipt) -> bool,
{
    let cas_dir = slot.parent().expect("receipt parent").join("cas");
    fs::read_dir(cas_dir)
        .expect("receipt CAS directory")
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read(entry.path()).ok())
        .filter_map(|bytes| parse_node_receipt(&bytes).ok())
        .any(|receipt| predicate(&receipt))
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
            if !path.starts_with("work/receipts/") || !path.ends_with(".json") {
                return true;
            }
            let value: Value = serde_json::from_slice(bytes).expect("receipt json");
            value["outcome"] != "cancelled"
        })
        .collect()
}

fn without_run_manifest(tree: BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    tree.into_iter()
        .filter(|(path, _)| !path.starts_with("work/run-manifest/"))
        .collect()
}

fn independent_plan() -> ProjectPlan {
    let mut plan = minimal_plan();
    let alpha = plan.nodes[0].clone();
    let mut beta = alpha.clone();
    let mut alpha = alpha;
    alpha.node_id = "alpha".to_string();
    alpha.command = test_node_command(&alpha.node_id);
    alpha.dependencies.clear();
    alpha.outputs[0].output_id = "alpha.output".to_string();
    alpha.outputs[0].path = "work/alpha.json".to_string();
    beta.node_id = "beta".to_string();
    beta.command = test_node_command(&beta.node_id);
    beta.dependencies.clear();
    beta.outputs[0].output_id = "beta.output".to_string();
    beta.outputs[0].path = "work/beta.json".to_string();
    plan.nodes = vec![alpha, beta];
    refresh_all_node_cache_keys(&mut plan);
    plan.graph_hash = digest_bytes(b"independent-plan");
    plan.summary.total_nodes = 2;
    plan.summary.edge_count = 0;
    plan.summary.runnable_nodes = 2;
    plan.summary.blocked_nodes = 0;
    plan
}

fn chain_plan() -> ProjectPlan {
    let mut plan = minimal_plan();
    let mut alpha = plan.nodes[0].clone();
    let mut beta = plan.nodes[1].clone();
    alpha.node_id = "alpha".to_string();
    alpha.command = test_node_command(&alpha.node_id);
    alpha.dependencies.clear();
    alpha.outputs[0].output_id = "alpha.output".to_string();
    alpha.outputs[0].path = "work/alpha.json".to_string();
    beta.node_id = "beta".to_string();
    beta.command = test_node_command(&beta.node_id);
    beta.dependencies = vec!["alpha".to_string()];
    beta.outputs[0].output_id = "beta.output".to_string();
    beta.outputs[0].path = "work/beta.json".to_string();
    plan.nodes = vec![alpha, beta];
    refresh_all_node_cache_keys(&mut plan);
    plan.graph_hash = digest_bytes(b"chain-plan");
    plan.summary.total_nodes = 2;
    plan.summary.edge_count = 1;
    plan.summary.runnable_nodes = 1;
    plan.summary.blocked_nodes = 1;
    plan
}

fn single_node_plan() -> ProjectPlan {
    let mut plan = independent_plan();
    plan.nodes.truncate(1);
    plan.graph_hash = digest_bytes(b"single-node-plan");
    plan.summary.total_nodes = 1;
    plan.summary.edge_count = 0;
    plan.summary.runnable_nodes = 1;
    plan.summary.blocked_nodes = 0;
    plan
}

fn change_alpha_identity(plan: &mut ProjectPlan) {
    let alpha = plan
        .nodes
        .iter_mut()
        .find(|node| node.node_id == "alpha")
        .expect("alpha node");
    alpha.content_hash_inputs[0].content_hash = digest_bytes(b"changed-alpha-input");
    refresh_node_cache_key(alpha);
    plan.graph_hash = digest_bytes(b"changed-single-node-plan");
}

fn configure_internal_copy_node(
    plan: &mut ProjectPlan,
    node_id: &str,
    input_path: &str,
    input_bytes: &[u8],
) {
    let digest = digest_bytes(input_bytes);
    let node = plan
        .nodes
        .iter_mut()
        .find(|node| node.node_id == node_id)
        .unwrap_or_else(|| panic!("node {node_id} exists"));
    node.command = internal_copy_command(input_path, &digest, &node.outputs[0].output_id, &digest);
    node.content_hash_inputs = vec![ProjectPlanHashRef {
        ref_id: format!("input.{input_path}"),
        content_hash: digest,
    }];
    node.side_effects = vec![
        ProjectPlanSideEffect {
            kind: ProjectPlanSideEffectKind::ReadsInput,
            description: "reads declared workspace input".to_string(),
        },
        ProjectPlanSideEffect {
            kind: ProjectPlanSideEffectKind::WritesArtifact,
            description: "writes declared project artifact".to_string(),
        },
    ];
    refresh_node_cache_key(node);
    plan.graph_hash = digest_bytes(format!("internal-copy-{node_id}").as_bytes());
}

fn internal_copy_command(
    input_path: &str,
    input_digest: &str,
    output_id: &str,
    output_digest: &str,
) -> String {
    format!(
        "canon project internal-node {PROJECT_INTERNAL_COPY_FILE_EXECUTOR} --input {input_path} --input-digest {input_digest} --output-id {output_id} --output-digest {output_digest}"
    )
}

fn write_workspace_file(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("workspace file parent"))
        .expect("workspace file parent");
    fs::write(path, bytes).expect("workspace file bytes");
}

fn mutate_command_contract(node: &mut ProjectPlanNode) {
    node.command = format!("{} --semantic-contract v2", node.command);
}

fn mutate_side_effect_contract(node: &mut ProjectPlanNode) {
    node.side_effects.push(ProjectPlanSideEffect {
        kind: ProjectPlanSideEffectKind::ReadsInput,
        description: "additional declared semantic input read".to_string(),
    });
}

fn mutate_refusal_contract(node: &mut ProjectPlanNode) {
    node.refusal_conditions.push(ProjectPlanRefusalCondition {
        code: ProjectPlanErrorCode::ManifestPolicy,
        message: "additional declared semantic precondition".to_string(),
        next_command: Some("canon project validate --manifest <MANIFEST>".to_string()),
    });
}

fn refresh_all_node_cache_keys(plan: &mut ProjectPlan) {
    for node in &mut plan.nodes {
        refresh_node_cache_key(node);
    }
}

fn refresh_node_cache_key(node: &mut ProjectPlanNode) {
    node.cache.cache_key = project_plan_node_cache_key(node).expect("node cache key recomputes");
}

fn test_node_command(node_id: &str) -> String {
    format!("canon project execute --plan work/plan.json --node {node_id}")
}

fn standalone_receipt(_root: &Path, node_id: &str) -> receipt::ProjectRunNodeReceipt {
    finalized_node_receipt(receipt::ProjectRunNodeReceipt {
        schema_version: project_run_schema_version().to_string(),
        project_id: "receipt-temp-test".to_string(),
        plan_graph_hash: digest_bytes(b"receipt-temp-test-plan"),
        node_id: node_id.to_string(),
        node_cache_key: digest_bytes(b"receipt-temp-test-cache"),
        content_hash_inputs: Vec::new(),
        dependency_semantic_hashes: BTreeMap::new(),
        dependency_receipt_hashes: BTreeMap::new(),
        outputs: Vec::new(),
        outcome: receipt::ProjectRunNodeOutcome::Completed,
        deterministic_usage: BTreeMap::new(),
        duration_millis: 0,
        resource_observations: BTreeMap::new(),
        next_action: receipt::ProjectRunNextAction::ExecuteDependents,
        failure_code: None,
        failure_message: None,
        semantic_hash: String::new(),
        telemetry_hash: String::new(),
        receipt_hash: String::new(),
    })
    .expect("standalone receipt finalizes")
}

fn output_temp_path_for_test(
    root: &Path,
    work_dir: &str,
    relative_output: &str,
    bytes: &[u8],
) -> PathBuf {
    root.join(work_dir).join(".tmp").join(format!(
        "{}.{}.tmp",
        path_token_for_test(relative_output),
        digest_bytes(bytes).replace(':', "_")
    ))
}

fn receipt_temp_path_for_test(path: &Path, bytes: &[u8]) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("receipt");
    path.with_file_name(format!(
        "{}.{}.tmp",
        file_name,
        digest_bytes(bytes).replace(':', "_")
    ))
}

fn manifest_temp_path_for_test(path: &Path, bytes: &[u8]) -> PathBuf {
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

fn publication_lock_path_for_test(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact");
    path.with_file_name(format!(".{file_name}.publish.lock"))
}

fn path_token_for_test(path: &str) -> String {
    path.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}
