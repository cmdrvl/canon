use assert_cmd::Command;
use canon::project::receipt::digest_bytes;
use canon::project::{
    ProjectRunHashRef, ProjectRunNextAction, ProjectRunNodeOutcome, ProjectRunNodeReceipt,
    ProjectRunOutputReceipt, canonical_node_receipt_bytes, finalized_node_receipt,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};
use tempfile::tempdir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

#[test]
fn init_validate_and_describe_emit_deterministic_project_artifacts() {
    let temp = tempdir().expect("tempdir");
    let project_dir = temp.path().join("project");

    let init = canon_command()
        .args([
            "project",
            "init",
            project_dir.to_str().unwrap(),
            "--project-id",
            "project.synthetic.cli",
        ])
        .assert()
        .success();
    assert!(init.get_output().stderr.is_empty());
    let init_json: Value = serde_json::from_slice(&init.get_output().stdout).unwrap();
    assert_eq!(init_json["schema_version"], "canon.project.cli.v1");
    assert_eq!(init_json["command"], "project.init");
    assert_eq!(init_json["project_id"], "project.synthetic.cli");
    assert!(project_dir.join("canon.project.toml").exists());

    let first_validate = canon_command()
        .args(["project", "validate", project_dir.to_str().unwrap()])
        .assert()
        .success();
    let second_validate = canon_command()
        .args(["project", "validate", project_dir.to_str().unwrap()])
        .assert()
        .success();
    assert_eq!(
        first_validate.get_output().stdout,
        second_validate.get_output().stdout
    );
    let validate_json: Value = serde_json::from_slice(&first_validate.get_output().stdout).unwrap();
    assert_eq!(validate_json["valid"], true);
    assert_eq!(validate_json["diagnostics"].as_array().unwrap().len(), 0);
    assert_eq!(
        validate_json["manifest"]["project_id"],
        "project.synthetic.cli"
    );

    let describe = canon_command()
        .args(["project", "describe", project_dir.to_str().unwrap()])
        .assert()
        .success();
    let describe_json: Value = serde_json::from_slice(&describe.get_output().stdout).unwrap();
    assert_eq!(describe_json["command"], "project.describe");
    assert_eq!(describe_json["state_flags"]["valid_manifest"], true);
    assert_eq!(describe_json["state_flags"]["offline_build_only"], true);
    assert_eq!(describe_json["state_flags"]["network_policy"], "deny_all");
    assert_eq!(describe_json["manifest"]["temporal_mode"], "timeless");
    assert_eq!(
        describe_json["capabilities"]["exit_codes"]["1"],
        "project validate found manifest diagnostics"
    );
    assert!(
        describe_json["capabilities"]["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|command| command["command"] == "canon project init <DIR>")
    );
    assert_eq!(
        describe_json["manifest_projection"]["sources"][0]["path"],
        project_dir
            .join("input/minimal.csv")
            .to_string_lossy()
            .replace('\\', "/")
    );
}

#[test]
fn init_refuses_non_empty_target_without_overwriting_files() {
    let temp = tempdir().expect("tempdir");
    let project_dir = temp.path().join("occupied");
    fs::create_dir(&project_dir).unwrap();
    fs::write(project_dir.join("keep.txt"), "before").unwrap();
    let before = tree(&project_dir);
    let keep_bytes = fs::read(project_dir.join("keep.txt")).unwrap();

    let output = canon_command()
        .args(["project", "init", project_dir.to_str().unwrap()])
        .assert()
        .code(2);
    let refusal: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_IO");
    assert!(output.get_output().stderr.is_empty());
    assert_eq!(tree(&project_dir), before);
    assert_eq!(fs::read(project_dir.join("keep.txt")).unwrap(), keep_bytes);
    assert!(!project_dir.join("canon.project.toml").exists());
}

#[test]
fn validate_reports_independent_manifest_requirements() {
    let temp = tempdir().expect("tempdir");
    let project_dir = temp.path().join("invalid");
    fs::create_dir(&project_dir).unwrap();
    fs::write(
        project_dir.join("canon.project.toml"),
        "schema_version = \"canon.project.v1\"\nproject_id = \"project.invalid\"\n",
    )
    .unwrap();

    let output = canon_command()
        .args(["project", "validate", project_dir.to_str().unwrap()])
        .assert()
        .code(1);
    assert!(output.get_output().stderr.is_empty());
    let payload: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(payload["valid"], false);
    let codes = payload["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().unwrap().to_string())
        .collect::<BTreeSet<_>>();
    assert!(codes.contains("missing_review_table"));
    assert!(codes.contains("missing_temporal_table"));
    assert!(codes.contains("missing_packages"));
    assert!(codes.contains("missing_modes"));
    assert!(codes.contains("manifest_artifactcontract"));
}

#[test]
fn validate_summary_and_describe_refusal_are_handoff_oriented() {
    let temp = tempdir().expect("tempdir");
    let project_dir = temp.path().join("invalid");
    fs::create_dir(&project_dir).unwrap();
    fs::write(project_dir.join("canon.project.toml"), "not toml").unwrap();

    let summary = canon_command()
        .args([
            "project",
            "validate",
            project_dir.to_str().unwrap(),
            "--emit",
            "summary",
        ])
        .assert()
        .code(1);
    let stdout = String::from_utf8(summary.get_output().stdout.clone()).unwrap();
    assert!(stdout.starts_with("invalid diagnostics="));
    assert!(stdout.contains("next=\""));
    assert!(summary.get_output().stderr.is_empty());

    let describe = canon_command()
        .args(["project", "describe", project_dir.to_str().unwrap()])
        .assert()
        .code(2);
    let refusal: Value = serde_json::from_slice(&describe.get_output().stdout).unwrap();
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert!(
        refusal["refusal"]["next_command"]
            .as_str()
            .unwrap()
            .contains("canon project validate")
    );
}

#[test]
fn lock_refresh_plan_and_run_reuse_existing_v2_receipt() {
    let temp = tempdir().expect("tempdir");
    let project_dir = temp.path().join("project");
    init_project(&project_dir);
    let source_bytes = b"name\nAlice\n";
    write_source(&project_dir, source_bytes);
    let manifest = project_dir.join("canon.project.toml");
    let lock = project_dir.join("canon.project.lock.json");
    let plan_path = project_dir.join("work/project.plan.json");

    let lock_output = canon_command()
        .args([
            "project",
            "lock",
            "refresh",
            "--manifest",
            manifest.to_str().unwrap(),
            "--out",
            lock.to_str().unwrap(),
        ])
        .assert()
        .success();
    let lock_json: Value = serde_json::from_slice(&lock_output.get_output().stdout).unwrap();
    assert_eq!(lock_json["schema_version"], "canon.project.lock.v1");
    assert_eq!(
        lock_json["inputs"][0]["content_digest"],
        digest_bytes(source_bytes)
    );
    assert_eq!(
        fs::read(&lock).expect("lock bytes"),
        lock_output.get_output().stdout[..lock_output.get_output().stdout.len() - 1]
    );

    let plan_output = canon_command()
        .args([
            "project",
            "plan",
            "--manifest",
            manifest.to_str().unwrap(),
            "--lock",
            lock.to_str().unwrap(),
            "--out",
            plan_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let plan_json: Value = serde_json::from_slice(&plan_output.get_output().stdout).unwrap();
    assert_eq!(plan_json["schema_version"], "canon.project.plan.v1");
    assert!(!json_strings(&plan_json).iter().any(|value| {
        value.contains("canon project execute") || value.contains("canon project review")
    }));

    write_valid_receipt_for_plan_node(&project_dir, &plan_json, "intake.source_alpha");
    let run_output = canon_command()
        .args([
            "project",
            "run",
            "--plan",
            plan_path.to_str().unwrap(),
            "--workspace",
            project_dir.to_str().unwrap(),
            "--node",
            "intake.source_alpha",
        ])
        .assert()
        .success();
    let run_json: Value = serde_json::from_slice(&run_output.get_output().stdout).unwrap();
    assert_eq!(run_json["schema_version"], "canon.project.run.v2");
    assert_eq!(run_json["executed_nodes"], json!([]));
    assert_eq!(
        run_json["resumed_nodes"][0].as_str(),
        Some("intake.source_alpha")
    );
    assert_eq!(run_json["failed_nodes"], json!([]));
    assert_eq!(run_json["cancelled_nodes"], json!([]));
    assert_eq!(run_json["invalidated_nodes"], json!([]));
    assert_eq!(run_json["blocked_nodes"], json!([]));
    assert_eq!(run_json["next_actions"], json!({}));
    assert_eq!(run_json["node_reports"], json!([]));
    assert_eq!(
        run_json["receipt"]["completed_nodes"],
        json!(["intake.source_alpha"])
    );
    assert_eq!(run_json["receipt"]["failed_nodes"], json!([]));
    assert_eq!(run_json["receipt"]["cancelled_nodes"], json!([]));
    assert_eq!(run_json["receipt"]["invalidated_nodes"], json!([]));
    assert_eq!(run_json["receipt"]["blocked_nodes"], json!([]));
}

#[test]
fn lock_refresh_refuses_missing_declared_source_without_synthetic_digest() {
    let temp = tempdir().expect("tempdir");
    let project_dir = temp.path().join("project");
    init_project(&project_dir);
    let manifest = project_dir.join("canon.project.toml");
    let lock = project_dir.join("canon.project.lock.json");

    let output = canon_command()
        .args([
            "project",
            "lock",
            "refresh",
            "--manifest",
            manifest.to_str().unwrap(),
            "--out",
            lock.to_str().unwrap(),
        ])
        .assert()
        .code(2);
    let refusal: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(refusal["refusal"]["code"], "E_IO");
    assert_eq!(
        refusal["refusal"]["detail"]["diagnostic"]["code"],
        "source_read_error"
    );
    assert!(!lock.exists());
}

#[test]
fn plan_refuses_manifest_changed_after_lock_refresh() {
    let temp = tempdir().expect("tempdir");
    let project_dir = temp.path().join("project");
    init_project(&project_dir);
    write_source(&project_dir, b"name\nAlice\n");
    let manifest = project_dir.join("canon.project.toml");
    let lock = project_dir.join("canon.project.lock.json");
    let plan_path = project_dir.join("work/project.plan.json");
    canon_command()
        .args([
            "project",
            "lock",
            "refresh",
            "--manifest",
            manifest.to_str().unwrap(),
            "--out",
            lock.to_str().unwrap(),
        ])
        .assert()
        .success();
    let text = fs::read_to_string(&manifest).expect("manifest");
    fs::write(
        &manifest,
        text.replace("version = \"1.0.0\"", "version = \"1.0.1\""),
    )
    .expect("change manifest");

    let output = canon_command()
        .args([
            "project",
            "plan",
            "--manifest",
            manifest.to_str().unwrap(),
            "--lock",
            lock.to_str().unwrap(),
            "--out",
            plan_path.to_str().unwrap(),
        ])
        .assert()
        .code(2);
    let refusal: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(refusal["refusal"]["detail"]["code"], "LockDrift");
    assert!(
        refusal["refusal"]["next_command"]
            .as_str()
            .unwrap()
            .contains("canon project lock refresh")
    );
    assert!(!plan_path.exists());
}

#[test]
fn run_node_subset_does_not_report_unselected_pending_nodes() {
    let temp = tempdir().expect("tempdir");
    let project_dir = temp.path().join("project");
    init_project(&project_dir);
    write_source(&project_dir, b"name\nAlice\n");
    add_independent_beta_source(&project_dir);
    let manifest = project_dir.join("canon.project.toml");
    let lock = project_dir.join("canon.project.lock.json");
    let plan_path = project_dir.join("work/project.plan.json");
    canon_command()
        .args([
            "project",
            "lock",
            "refresh",
            "--manifest",
            manifest.to_str().unwrap(),
            "--out",
            lock.to_str().unwrap(),
        ])
        .assert()
        .success();
    let plan_output = canon_command()
        .args([
            "project",
            "plan",
            "--manifest",
            manifest.to_str().unwrap(),
            "--lock",
            lock.to_str().unwrap(),
            "--out",
            plan_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let plan_json: Value = serde_json::from_slice(&plan_output.get_output().stdout).unwrap();
    assert!(
        json_strings(&plan_json)
            .iter()
            .any(|value| value == "intake.source_beta")
    );

    write_valid_receipt_for_plan_node(&project_dir, &plan_json, "intake.source_alpha");
    let run_output = canon_command()
        .args([
            "project",
            "run",
            "--plan",
            plan_path.to_str().unwrap(),
            "--workspace",
            project_dir.to_str().unwrap(),
            "--node",
            "intake.source_alpha",
        ])
        .assert()
        .success();
    let run_json: Value = serde_json::from_slice(&run_output.get_output().stdout).unwrap();

    assert_eq!(run_json["executed_nodes"], json!([]));
    assert_eq!(run_json["resumed_nodes"], json!(["intake.source_alpha"]));
    assert_eq!(run_json["blocked_nodes"], json!([]));
    assert_eq!(run_json["next_actions"], json!({}));
    assert_eq!(
        run_json["receipt"]["completed_nodes"],
        json!(["intake.source_alpha"])
    );
    assert!(
        !json_strings(&run_json)
            .iter()
            .any(|value| value.contains("source_beta")),
        "{run_json}"
    );
}

#[test]
fn run_refuses_declared_network_effect_without_policy() {
    let temp = tempdir().expect("tempdir");
    let project_dir = temp.path().join("project");
    init_project(&project_dir);
    write_source(&project_dir, b"name\nAlice\n");
    let manifest = project_dir.join("canon.project.toml");
    let text = fs::read_to_string(&manifest).expect("manifest");
    fs::write(
        &manifest,
        text.replace(
            "offline_build_only = true\nnetwork_policy = \"deny_all\"\ndeclared_hosts = []",
            "offline_build_only = false\nnetwork_policy = \"allow_declared_hosts\"\ndeclared_hosts = [\"example.test\"]",
        ),
    )
    .expect("network manifest");
    let lock = project_dir.join("canon.project.lock.json");
    let plan_path = project_dir.join("work/project.plan.json");
    canon_command()
        .args([
            "project",
            "lock",
            "refresh",
            "--manifest",
            manifest.to_str().unwrap(),
            "--out",
            lock.to_str().unwrap(),
        ])
        .assert()
        .success();
    canon_command()
        .args([
            "project",
            "plan",
            "--manifest",
            manifest.to_str().unwrap(),
            "--lock",
            lock.to_str().unwrap(),
            "--out",
            plan_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    let output = canon_command()
        .args([
            "project",
            "run",
            "--plan",
            plan_path.to_str().unwrap(),
            "--workspace",
            project_dir.to_str().unwrap(),
        ])
        .assert()
        .code(2);
    let refusal: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert!(
        refusal["refusal"]["detail"]["message"]
            .as_str()
            .unwrap()
            .contains("declared node effects exceed project run policy")
    );
    assert!(!project_dir.join("work/materialize/external.json").exists());
}

#[test]
fn run_refuses_pending_node_without_registered_executor_or_output_bytes() {
    let temp = tempdir().expect("tempdir");
    let project_dir = temp.path().join("project");
    init_project(&project_dir);
    write_source(&project_dir, b"name\nAlice\n");
    let manifest = project_dir.join("canon.project.toml");
    let lock = project_dir.join("canon.project.lock.json");
    let plan_path = project_dir.join("work/project.plan.json");
    canon_command()
        .args([
            "project",
            "lock",
            "refresh",
            "--manifest",
            manifest.to_str().unwrap(),
            "--out",
            lock.to_str().unwrap(),
        ])
        .assert()
        .success();
    canon_command()
        .args([
            "project",
            "plan",
            "--manifest",
            manifest.to_str().unwrap(),
            "--lock",
            lock.to_str().unwrap(),
            "--out",
            plan_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    let output = canon_command()
        .args([
            "project",
            "run",
            "--plan",
            plan_path.to_str().unwrap(),
            "--workspace",
            project_dir.to_str().unwrap(),
            "--node",
            "intake.source_alpha",
        ])
        .assert()
        .code(2);
    let refusal: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert!(
        refusal["refusal"]["detail"]["message"]
            .as_str()
            .unwrap()
            .contains("no registered real executor")
    );
    assert!(
        !project_dir
            .join("work/sources/source_alpha/intake.jsonl")
            .exists()
    );
    assert!(
        !project_dir
            .join("work/receipts/intake_source_alpha.json")
            .exists()
    );
}

#[test]
fn describe_and_plan_next_commands_use_executable_project_shapes() {
    let temp = tempdir().expect("tempdir");
    let project_dir = temp.path().join("project");
    init_project(&project_dir);
    write_source(&project_dir, b"name\nAlice\n");
    let manifest = project_dir.join("canon.project.toml");
    let lock = project_dir.join("canon.project.lock.json");
    canon_command()
        .args([
            "project",
            "lock",
            "refresh",
            "--manifest",
            manifest.to_str().unwrap(),
            "--out",
            lock.to_str().unwrap(),
        ])
        .assert()
        .success();

    let describe = canon_command()
        .args(["project", "describe", project_dir.to_str().unwrap()])
        .assert()
        .success();
    let describe_json: Value = serde_json::from_slice(&describe.get_output().stdout).unwrap();
    assert_no_dangling_project_commands(&describe_json);

    let plan = canon_command()
        .args([
            "project",
            "plan",
            "--manifest",
            manifest.to_str().unwrap(),
            "--lock",
            lock.to_str().unwrap(),
        ])
        .assert()
        .success();
    let plan_json: Value = serde_json::from_slice(&plan.get_output().stdout).unwrap();
    assert_no_dangling_project_commands(&plan_json);
}

fn tree(root: &Path) -> BTreeSet<String> {
    let mut entries = BTreeSet::new();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        entries.insert(
            path.strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/"),
        );
    }
    entries
}

fn init_project(project_dir: &Path) {
    canon_command()
        .args([
            "project",
            "init",
            project_dir.to_str().unwrap(),
            "--project-id",
            "project.synthetic.cli",
        ])
        .assert()
        .success();
}

fn write_source(project_dir: &Path, bytes: &[u8]) {
    let input_dir = project_dir.join("input");
    fs::create_dir_all(&input_dir).expect("input dir");
    fs::write(input_dir.join("minimal.csv"), bytes).expect("source bytes");
}

fn add_independent_beta_source(project_dir: &Path) {
    let manifest = project_dir.join("canon.project.toml");
    let text = fs::read_to_string(&manifest).expect("manifest text");
    let source = r#"
[[sources]]
source_id = "source_beta"
path = "input/beta.csv"
format = "csv"
mapping_package = "mapping"
mapping_profile = "pkg.synthetic:contacts"
required = true

"#;
    fs::write(
        &manifest,
        text.replace("\n[[outputs]]", &format!("{source}[[outputs]]")),
    )
    .expect("manifest with beta source");
    fs::write(project_dir.join("input/beta.csv"), b"name\nBob\n").expect("beta source");
}

fn write_valid_receipt_for_plan_node(project_dir: &Path, plan_json: &Value, node_id: &str) {
    let node = plan_json["nodes"]
        .as_array()
        .expect("plan nodes")
        .iter()
        .find(|node| node["node_id"] == node_id)
        .unwrap_or_else(|| panic!("node {node_id} present"));
    let output = &node["outputs"][0];
    let output_path = project_dir.join(output["path"].as_str().expect("output path"));
    let output_bytes = b"previous real executor output\n";
    fs::create_dir_all(output_path.parent().expect("output parent")).expect("output parent");
    fs::write(&output_path, output_bytes).expect("output bytes");

    let receipt = finalized_node_receipt(ProjectRunNodeReceipt {
        schema_version: "canon.project.run.v2".to_string(),
        project_id: plan_json["project_id"].as_str().unwrap().to_string(),
        plan_graph_hash: plan_json["graph_hash"].as_str().unwrap().to_string(),
        node_id: node["node_id"].as_str().unwrap().to_string(),
        node_cache_key: node["cache"]["cache_key"].as_str().unwrap().to_string(),
        content_hash_inputs: node["content_hash_inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|input| ProjectRunHashRef {
                ref_id: input["ref_id"].as_str().unwrap().to_string(),
                content_hash: input["content_hash"].as_str().unwrap().to_string(),
            })
            .collect(),
        dependency_semantic_hashes: Default::default(),
        dependency_receipt_hashes: Default::default(),
        outputs: vec![ProjectRunOutputReceipt {
            output_id: output["output_id"].as_str().unwrap().to_string(),
            path: output["path"].as_str().unwrap().to_string(),
            content_digest: digest_bytes(output_bytes),
            byte_count: output_bytes.len() as u64,
        }],
        outcome: ProjectRunNodeOutcome::Completed,
        deterministic_usage: Default::default(),
        duration_millis: 12,
        resource_observations: Default::default(),
        next_action: ProjectRunNextAction::ReuseReceipt,
        failure_code: None,
        failure_message: None,
        semantic_hash: String::new(),
        telemetry_hash: String::new(),
        receipt_hash: String::new(),
    })
    .expect("receipt finalizes");
    let receipt_bytes = canonical_node_receipt_bytes(&receipt).expect("receipt bytes");
    let receipt_path = project_dir
        .join("work/receipts")
        .join(format!("{}.json", node_id.replace('.', "_")));
    fs::create_dir_all(receipt_path.parent().expect("receipt parent")).expect("receipt parent");
    fs::write(receipt_path, receipt_bytes).expect("receipt write");
}

fn assert_no_dangling_project_commands(value: &Value) {
    for string in json_strings(value) {
        if !string.contains("canon project ") {
            continue;
        }
        assert!(!string.contains("canon project execute"), "{string}");
        assert!(!string.contains("canon project review"), "{string}");
        assert!(!string.contains("canon project audit"), "{string}");
        assert!(!string.contains("canon project status"), "{string}");
        assert!(!string.contains("canon project promote"), "{string}");
        assert!(!string.contains("canon project apply"), "{string}");
        assert!(!string.contains("canon project export"), "{string}");
    }
}

fn json_strings(value: &Value) -> Vec<String> {
    match value {
        Value::String(string) => vec![string.clone()],
        Value::Array(values) => values.iter().flat_map(json_strings).collect(),
        Value::Object(map) => map.values().flat_map(json_strings).collect(),
        _ => Vec::new(),
    }
}
