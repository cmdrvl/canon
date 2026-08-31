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
    finalized_node_receipt, parse_node_receipt, project_run_schema_version, read_node_receipt,
    write_node_receipt,
};
use run::{
    PROJECT_INTERNAL_COPY_FILE_EXECUTOR, ProjectNodeExecutionContext, ProjectNodeExecutionResult,
    ProjectNodeExecutor, ProjectRunError, ProjectRunErrorCode, ProjectRunFailurePolicy,
    ProjectRunPolicy, canonical_project_run_report_bytes, run_project_plan,
    run_project_plan_with_registered_executors,
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
fn output_publishes_before_v2_receipt_for_registered_executor_recovery() {
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
    .expect_err("receipt publication fails after artifact publication");

    assert_eq!(error.code, ProjectRunErrorCode::ReceiptPoisoning);
    assert_eq!(
        fs::read(artifact_path(temp.path(), &plan, "alpha")).expect("artifact published"),
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
