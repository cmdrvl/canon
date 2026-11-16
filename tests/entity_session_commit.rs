use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use tempfile::TempDir;

const RULES: &str = r#"version: canon.entity.session.rules.v1
keys:
  authoritative: [cik, lei]
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

fn observation(
    obs_id: &str,
    lineage: &str,
    surface: &str,
    cik: Option<&str>,
    lei: Option<&str>,
) -> String {
    serde_json::to_string(&json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": {"cik": cik, "lei": lei},
        "source": {"channel": "file", "id": "commit-fixture", "lineage": lineage}
    }))
    .expect("observation JSON")
}

fn write_fixture(temp: &TempDir, name: &str, lines: &[String]) -> PathBuf {
    let path = temp.path().join(name);
    fs::write(&path, format!("{}\n", lines.join("\n"))).expect("fixture");
    path
}

fn stdout_json(assertion: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&assertion.get_output().stdout).expect("JSON stdout")
}

fn init(temp: &TempDir) -> (PathBuf, PathBuf) {
    let registry = temp.path().join("registry");
    let session = temp.path().join("session");
    canon()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "commit acceptance",
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
    (registry, session)
}

fn evidence(left: &str, right: &str) -> String {
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
        "source": {"channel": "agent", "id": "commit-fixture", "lineage": "agent-lineage"},
        "asserted_by": "agent:commit-fixture"
    }))
    .expect("evidence JSON")
}

#[test]
fn session_commit_mints_exact_clusters_and_plain_lookup_resolves_variants() {
    let temp = TempDir::new().expect("tempdir");
    let (registry, session) = init(&temp);
    let observations = write_fixture(
        &temp,
        "observations.jsonl",
        &[
            observation("r1", "fund-a", "Medline Borrower, LP", Some("111"), None),
            observation(
                "r2",
                "fund-b",
                "Mozart Borrower LP (dba Medline)",
                Some("111"),
                None,
            ),
            observation(
                "r3",
                "fund-a",
                "Acme Holdings Inc",
                None,
                Some("CANONFIXT0000000AA49"),
            ),
            observation(
                "r4",
                "fund-b",
                "Acme Hldgs",
                None,
                Some("CANONFIXT0000000AA49"),
            ),
            observation("r5", "fund-a", "Zeta Corp", Some("222"), None),
            observation("r6", "fund-b", "Zeta Corporation", Some("222"), None),
            observation("review-a", "fund-a", "Review Borrower", None, None),
            observation("review-b", "fund-b", "Review Parent", None, None),
            observation("apex-a", "fund-a", "Apex LLC", Some("333"), None),
            observation("apex-b", "fund-b", "Apex LLC", Some("444"), None),
        ],
    );
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

    let assertions = write_fixture(&temp, "evidence.jsonl", &[evidence("review-a", "review-b")]);
    canon()
        .args([
            "entity",
            "session",
            "assert",
            session.to_str().expect("session path"),
            assertions.to_str().expect("evidence path"),
        ])
        .assert()
        .success();
    let rules = write_fixture(&temp, "rules.yaml", &[RULES.trim_end().to_string()]);
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
        .success();

    let commit = canon()
        .args([
            "entity",
            "session",
            "commit",
            session.to_str().expect("session path"),
            "--next-version",
            "0.1.0",
        ])
        .assert()
        .success();
    let commit_json = stdout_json(&commit);
    assert_eq!(commit_json["outcome"], "committed");
    assert_eq!(commit_json["registry"]["version_after"], "0.1.0");
    assert_eq!(commit_json["minted"].as_array().expect("minted").len(), 3);
    for minted in commit_json["minted"].as_array().expect("minted") {
        assert!(minted["outcome_ref"].as_str().is_some());
        assert!(minted["closure_hash"].as_str().is_some());
    }
    assert!(commit_json["skipped"].as_array().is_some_and(|rows| {
        rows.iter().any(|row| row["reason"] == "standalone")
            || rows.iter().any(|row| row["reason"] == "review")
            || rows.iter().any(|row| row["reason"] == "contradiction")
    }));

    let aliases: Value = serde_json::from_slice(
        &fs::read(registry.join("aliases.json")).expect("aliases registry file"),
    )
    .expect("aliases JSON");
    for input in [
        "Medline Borrower, LP",
        "Mozart Borrower LP (dba Medline)",
        "111",
        "Acme Holdings Inc",
        "Acme Hldgs",
        "CANONFIXT0000000AA49",
        "Zeta Corp",
        "Zeta Corporation",
        "222",
    ] {
        assert!(
            aliases
                .as_array()
                .expect("aliases array")
                .iter()
                .any(|entry| entry["input"] == input),
            "missing committed alias {input}"
        );
    }

    let lookup_input = write_fixture(
        &temp,
        "lookup.jsonl",
        &[
            serde_json::to_string(&json!({"name": "Medline Borrower, LP"})).unwrap(),
            serde_json::to_string(&json!({"name": "Mozart Borrower LP (dba Medline)"})).unwrap(),
            serde_json::to_string(&json!({"name": "111"})).unwrap(),
        ],
    );
    let lookup = canon()
        .args([
            lookup_input.to_str().expect("lookup path"),
            "--registry",
            registry.to_str().expect("registry path"),
            "--column",
            "name",
        ])
        .assert()
        .success();
    let lookup_json = stdout_json(&lookup);
    assert_eq!(lookup_json["summary"]["resolved"], 3);
    assert_eq!(
        lookup_json["mappings"][0]["canonical_id"],
        lookup_json["mappings"][1]["canonical_id"]
    );
    assert_eq!(
        lookup_json["mappings"][0]["canonical_id"],
        lookup_json["mappings"][2]["canonical_id"]
    );

    let negative = write_fixture(
        &temp,
        "negative.jsonl",
        &[
            serde_json::to_string(&json!({"name": "Apex LLC"})).unwrap(),
            serde_json::to_string(&json!({"name": "Review Borrower"})).unwrap(),
        ],
    );
    let unresolved = canon()
        .args([
            negative.to_str().expect("negative path"),
            "--registry",
            registry.to_str().expect("registry path"),
            "--column",
            "name",
        ])
        .assert()
        .code(1);
    let unresolved_json = stdout_json(&unresolved);
    assert_eq!(unresolved_json["summary"]["unresolved"], 2);

    let registry_before_repeat = fs::read(registry.join("registry.json")).expect("registry");
    let aliases_before_repeat = fs::read(registry.join("aliases.json")).expect("aliases");
    let repeat = canon()
        .args([
            "entity",
            "session",
            "commit",
            session.to_str().expect("session path"),
            "--next-version",
            "0.1.0",
        ])
        .assert()
        .success();
    assert_eq!(stdout_json(&repeat)["outcome"], "already_committed");
    assert_eq!(
        fs::read(registry.join("registry.json")).expect("registry"),
        registry_before_repeat
    );
    assert_eq!(
        fs::read(registry.join("aliases.json")).expect("aliases"),
        aliases_before_repeat
    );

    let extra = write_fixture(
        &temp,
        "extra.jsonl",
        &[observation(
            "extra",
            "fund-c",
            "New Surface",
            Some("999"),
            None,
        )],
    );
    canon()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().expect("session path"),
            extra.to_str().expect("extra path"),
        ])
        .assert()
        .success();
    let stale = canon()
        .args([
            "entity",
            "session",
            "commit",
            session.to_str().expect("session path"),
            "--next-version",
            "0.1.0",
        ])
        .assert()
        .code(2);
    let stale_output = String::from_utf8_lossy(&stale.get_output().stdout);
    assert!(stale_output.contains("stale_solve"), "{stale_output}");
}
