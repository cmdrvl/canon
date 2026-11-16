use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

const RULES: &str = r#"version: canon.entity.session.rules.v1
keys:
  authoritative: [cik]
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
"#;

fn canon() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn write_file(temp: &TempDir, name: &str, contents: &str) -> PathBuf {
    let path = temp.path().join(name);
    fs::write(&path, contents).expect("fixture");
    path
}

fn observation(obs_id: &str, lineage: &str, surface: &str) -> String {
    serde_json::to_string(&json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": {"cik": null},
        "source": {"channel": "file", "id": "memory-fixture", "lineage": lineage}
    }))
    .expect("observation JSON")
}

fn same_slot(left: &str, right: &str) -> String {
    serde_json::to_string(&json!({
        "type": "same_slot",
        "left": left,
        "right": right,
        "basis": {
            "rule": "same loan and role",
            "left_value": "loan-7:sponsor",
            "right_value": "loan-7:sponsor",
            "steps": ["join:loan_id", "join:role"]
        },
        "source": {"channel": "agent", "id": "memory-fixture", "lineage": "agent-lineage"},
        "asserted_by": "agent:memory-fixture"
    }))
    .expect("same-slot JSON")
}

fn distinct(left: &str, right: &str) -> String {
    serde_json::to_string(&json!({
        "type": "distinct",
        "left": left,
        "right": right,
        "basis": {"authority": "reviewed"},
        "source": {"channel": "agent", "id": "memory-review", "lineage": "review-lineage"},
        "asserted_by": "human:reviewer"
    }))
    .expect("distinct JSON")
}

fn init(registry: &Path, session: &Path) {
    canon()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "registry memory acceptance",
            "--registry",
            registry.to_str().expect("registry path"),
            "--work-dir",
            session.to_str().expect("session path"),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ])
        .assert()
        .success();
}

fn run_session(session: &Path, observations: &Path, evidence: &Path, rules: &Path) -> Value {
    canon()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().expect("session path"),
            observations.to_str().expect("observations path"),
        ])
        .assert()
        .success();
    canon()
        .args([
            "entity",
            "session",
            "assert",
            session.to_str().expect("session path"),
            evidence.to_str().expect("evidence path"),
        ])
        .assert()
        .success();
    canon()
        .args([
            "entity",
            "session",
            "solve",
            session.to_str().expect("session path"),
            "--rules",
            rules.to_str().expect("rules path"),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .as_slice()
        .pipe(|bytes| serde_json::from_slice(bytes).expect("solve JSON"))
}

trait Pipe: Sized {
    fn pipe<T>(self, function: impl FnOnce(Self) -> T) -> T {
        function(self)
    }
}

impl<T> Pipe for T {}

#[test]
fn registry_memory_persists_known_review_then_vetoes_a_later_merge() {
    let temp = TempDir::new().expect("tempdir");
    let registry = temp.path().join("registry");
    let first_session = temp.path().join("first-session");
    let second_session = temp.path().join("second-session");
    let third_session = temp.path().join("third-session");
    init(&registry, &first_session);

    let first_observations = write_file(
        &temp,
        "first-observations.jsonl",
        &format!(
            "{}\n{}\n",
            observation("alpha-1", "source-a", "Alpha Holdings"),
            observation("beta-1", "source-b", "Beta Holdings")
        ),
    );
    let first_evidence = write_file(
        &temp,
        "first-evidence.jsonl",
        &format!("{}\n", same_slot("alpha-1", "beta-1")),
    );
    let rules = write_file(&temp, "rules.yaml", RULES);
    let first_solve = run_session(&first_session, &first_observations, &first_evidence, &rules);
    assert_eq!(first_solve["summary"]["review_new"], 1);
    assert_eq!(first_solve["summary"]["review_known"], 0);

    let first_commit: Value = canon()
        .args([
            "entity",
            "session",
            "commit",
            first_session.to_str().expect("session path"),
            "--next-version",
            "0.1.0",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .as_slice()
        .pipe(|bytes| serde_json::from_slice(bytes).expect("commit JSON"));
    assert_eq!(first_commit["pending_written"], 1);
    assert!(registry.join("_escrow/pending.jsonl").is_file());

    init(&registry, &second_session);
    let second_observations = write_file(
        &temp,
        "second-observations.jsonl",
        &format!(
            "{}\n{}\n",
            observation("alpha-2", "source-c", "Alpha Holdings"),
            observation("beta-2", "source-d", "Beta Holdings")
        ),
    );
    let second_evidence = write_file(
        &temp,
        "second-evidence.jsonl",
        &format!("{}\n", same_slot("alpha-2", "beta-2")),
    );
    let second_solve = run_session(
        &second_session,
        &second_observations,
        &second_evidence,
        &rules,
    );
    assert_eq!(second_solve["summary"]["review_new"], 0);
    assert_eq!(second_solve["summary"]["review_known"], 1);
    assert!(
        second_solve["review"][0]["known_pending"]
            .as_bool()
            .is_some_and(|value| value)
    );
    assert!(second_solve["review"][0]["escrow_id"].as_str().is_some());

    let veto_session = third_session;
    init(&registry, &veto_session);
    let veto_observations = write_file(
        &temp,
        "veto-observations.jsonl",
        &format!(
            "{}\n{}\n",
            observation("alpha-3", "source-e", "Alpha Holdings"),
            observation("beta-3", "source-f", "Beta Holdings")
        ),
    );
    let veto_evidence = write_file(
        &temp,
        "veto-evidence.jsonl",
        &format!("{}\n", distinct("alpha-3", "beta-3")),
    );
    let veto_solve = run_session(&veto_session, &veto_observations, &veto_evidence, &rules);
    assert_eq!(veto_solve["summary"]["review"], 0);
    let veto_commit: Value = canon()
        .args([
            "entity",
            "session",
            "commit",
            veto_session.to_str().expect("session path"),
            "--next-version",
            "0.2.0",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .as_slice()
        .pipe(|bytes| serde_json::from_slice(bytes).expect("veto commit JSON"));
    assert_eq!(veto_commit["cannot_link_written"], 1);
    assert!(registry.join("_escrow/cannot_link.jsonl").is_file());
    let lint: Value = canon()
        .args([
            "registry",
            "lint",
            registry.to_str().expect("registry path"),
            "--emit",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .as_slice()
        .pipe(|bytes| serde_json::from_slice(bytes).expect("lint JSON"));
    assert_eq!(lint["summary"]["errors"], 0);

    let blocked_session = temp.path().join("blocked-session");
    init(&registry, &blocked_session);
    let blocked_observations = write_file(
        &temp,
        "blocked-observations.jsonl",
        &format!(
            "{}\n{}\n",
            observation("alpha-4", "source-g", "Alpha Holdings"),
            observation("beta-4", "source-h", "Beta Holdings")
        ),
    );
    let blocked_evidence = write_file(
        &temp,
        "blocked-evidence.jsonl",
        &format!("{}\n", same_slot("alpha-4", "beta-4")),
    );
    let blocked_solve = run_session(
        &blocked_session,
        &blocked_observations,
        &blocked_evidence,
        &rules,
    );
    assert_eq!(blocked_solve["summary"]["review"], 0);
    assert!(
        blocked_solve["clusters"]
            .as_array()
            .is_some_and(|clusters| {
                clusters.iter().any(|cluster| {
                    cluster["outcome"] == "contradiction"
                        && cluster["reason"] == "registry_cannot_link"
                })
            })
    );
}
