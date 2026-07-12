use assert_cmd::Command;
use serde_json::Value;
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
