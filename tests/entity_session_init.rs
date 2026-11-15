use assert_cmd::Command;
use canon::entity::publication::open_current_stream_generation;
use canon::session::{SESSION_MANIFEST_STREAM_ID, SessionManifest};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn run_and_log(label: &str, output: &assert_cmd::assert::Assert) {
    eprintln!(
        "[{label}] exit={} stdout={} stderr={}",
        output.get_output().status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.get_output().stdout),
        String::from_utf8_lossy(&output.get_output().stderr),
    );
}

fn json_stdout(output: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(output.get_output().stdout.as_slice()).expect("JSON stdout envelope")
}

fn snapshot_files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, current: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(current).expect("read snapshot directory") {
            let entry = entry.expect("read snapshot entry");
            let path = entry.path();
            let relative = path.strip_prefix(root).expect("snapshot relative path");
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    relative.to_path_buf(),
                    fs::read(path).expect("read snapshot file"),
                );
            }
        }
    }

    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

fn init_args(registry: &Path, work_dir: &Path, goal: &str) -> Vec<String> {
    vec![
        "entity".to_string(),
        "session".to_string(),
        "init".to_string(),
        "--goal".to_string(),
        goal.to_string(),
        "--registry".to_string(),
        registry.display().to_string(),
        "--work-dir".to_string(),
        work_dir.display().to_string(),
        "--registry-id".to_string(),
        "organisations".to_string(),
        "--prefix".to_string(),
        "ORG".to_string(),
        "--zero-pad".to_string(),
        "4".to_string(),
    ]
}

#[test]
fn session_init_creates_registry_manifest_and_empty_lookup_is_unresolved() {
    let temp = TempDir::new().expect("temp root");
    let registry = temp.path().join("registries/organisations");
    let work_dir = temp.path().join("sessions/first");
    let args = init_args(&registry, &work_dir, "dedupe BDC borrowers");
    let output = canon_command().args(&args).assert().success();
    run_and_log("session init", &output);

    let payload = json_stdout(&output);
    assert_eq!(payload["version"], "canon.entity.session.init.v1");
    assert_eq!(payload["status"], "initialised");
    assert_eq!(payload["registry_initialized"], true);
    assert_eq!(payload["registries"][0]["id"], "organisations");
    assert_eq!(payload["registries"][0]["kind"], "organisation");
    assert_eq!(
        payload["registries"][0]["path"],
        registry.display().to_string()
    );
    assert!(
        payload["session_id"]
            .as_str()
            .unwrap()
            .starts_with("session:blake3:")
    );

    let lint = canon_command()
        .args([
            "registry",
            "lint",
            registry.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .success();
    run_and_log("registry lint", &lint);
    let lint_payload = json_stdout(&lint);
    assert_eq!(lint_payload["summary"]["errors"], 0);

    let next_id = canon_command()
        .args([
            "registry",
            "next-id",
            "--registry",
            registry.to_str().unwrap(),
        ])
        .assert()
        .success();
    run_and_log("registry next-id", &next_id);
    assert_eq!(
        String::from_utf8(next_id.get_output().stdout.clone()).unwrap(),
        "ORG-0001\n"
    );

    let rows = temp.path().join("rows.csv");
    fs::write(&rows, "name\nUnknown Borrower\n").expect("write rows");
    let lookup = canon_command()
        .args([
            rows.to_str().unwrap(),
            "--registry",
            registry.to_str().unwrap(),
            "--column",
            "name",
            "--no-witness",
        ])
        .assert()
        .code(1);
    run_and_log("empty lookup", &lookup);
    let lookup_payload = json_stdout(&lookup);
    assert_eq!(lookup_payload["outcome"], "UNRESOLVED");
    assert_eq!(lookup_payload["summary"]["total"], 1);

    let snapshot = open_current_stream_generation(&work_dir, SESSION_MANIFEST_STREAM_ID)
        .expect("session manifest stream");
    assert!(
        snapshot
            .read_logical_file("session/session.json")
            .map(|bytes| bytes.len())
            .unwrap_or_default()
            > 0
    );
    let manifest: SessionManifest = serde_json::from_slice(
        snapshot
            .read_logical_file("session/session.json")
            .expect("session manifest file"),
    )
    .expect("session manifest JSON");
    assert_eq!(manifest.goal, "dedupe BDC borrowers");
    assert_eq!(manifest.registries[0].id, "organisations");
    assert_eq!(manifest.registries[0].path, registry.display().to_string());
    assert_eq!(
        snapshot.generation_id,
        payload["generation_id"].as_str().expect("generation id")
    );
}

#[test]
fn registry_init_refuses_existing_registry_and_session_repeat_is_idempotent() {
    let temp = TempDir::new().expect("temp root");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("session");

    let registry_created = canon_command()
        .args([
            "registry",
            "init",
            "--registry",
            registry.to_str().unwrap(),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ])
        .assert()
        .success();
    run_and_log("registry init", &registry_created);
    let before = snapshot_files(&registry);

    let refused = canon_command()
        .args([
            "registry",
            "init",
            "--registry",
            registry.to_str().unwrap(),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
        ])
        .assert()
        .code(2);
    run_and_log("registry init refusal", &refused);
    let refused_payload = json_stdout(&refused);
    assert_eq!(refused_payload["outcome"], "REFUSAL");
    assert_eq!(refused_payload["refusal"]["code"], "E_PARSE");
    assert_eq!(snapshot_files(&registry), before);

    let args = init_args(&registry, &work_dir, "dedupe people");
    let first = canon_command().args(&args).assert().success();
    run_and_log("session init existing registry", &first);
    let first_payload = json_stdout(&first);
    assert_eq!(first_payload["registry_initialized"], false);
    assert_eq!(snapshot_files(&registry), before);

    let second = canon_command().args(&args).assert().success();
    run_and_log("session init already initialised", &second);
    let second_payload = json_stdout(&second);
    assert_eq!(second_payload["status"], "already_initialised");
    assert_eq!(
        second_payload["generation_id"],
        first_payload["generation_id"]
    );
    assert_eq!(snapshot_files(&registry), before);
}

#[test]
fn session_init_rejects_different_goal_without_mutating_registry_or_session() {
    let temp = TempDir::new().expect("temp root");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("session");
    let args = init_args(&registry, &work_dir, "original goal");
    let first = canon_command().args(&args).assert().success();
    run_and_log("session init original goal", &first);
    let registry_before = snapshot_files(&registry);
    let session_before = snapshot_files(&work_dir);

    let refused = canon_command()
        .args(init_args(&registry, &work_dir, "different goal"))
        .assert()
        .code(2);
    run_and_log("session init different goal refusal", &refused);
    let payload = json_stdout(&refused);
    assert_eq!(payload["refusal"]["code"], "E_PARSE");
    assert_eq!(payload["refusal"]["detail"]["field"], "goal");
    assert_eq!(snapshot_files(&registry), registry_before);
    assert_eq!(snapshot_files(&work_dir), session_before);
}

#[test]
fn session_init_repoints_same_registry_id_to_a_new_registry_path() {
    let temp = TempDir::new().expect("temp root");
    let first_registry = temp.path().join("registry-first");
    let second_registry = temp.path().join("registry-second");
    let work_dir = temp.path().join("session");

    let first_args = init_args(&first_registry, &work_dir, "dedupe people");
    let first = canon_command().args(&first_args).assert().success();
    run_and_log("session init first registry", &first);
    let first_payload = json_stdout(&first);

    let second_registry_created = canon_command()
        .args([
            "registry",
            "init",
            "--registry",
            second_registry.to_str().unwrap(),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "4",
        ])
        .assert()
        .success();
    run_and_log("registry init second path", &second_registry_created);

    let second = canon_command()
        .args(init_args(&second_registry, &work_dir, "dedupe people"))
        .assert()
        .success();
    run_and_log("session init registry path update", &second);
    let second_payload = json_stdout(&second);

    assert_eq!(second_payload["status"], "registry_path_updated");
    assert_eq!(second_payload["registry_path_updated"], true);
    assert_eq!(second_payload["session_id"], first_payload["session_id"]);
    assert_ne!(
        second_payload["generation_id"],
        first_payload["generation_id"]
    );
    let snapshot = open_current_stream_generation(&work_dir, SESSION_MANIFEST_STREAM_ID)
        .expect("updated session manifest stream");
    let manifest: SessionManifest = serde_json::from_slice(
        snapshot
            .read_logical_file("session/session.json")
            .expect("updated session manifest"),
    )
    .expect("updated session manifest JSON");
    assert_eq!(
        manifest.registries[0].path,
        second_registry.display().to_string()
    );
}
