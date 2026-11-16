#![forbid(unsafe_code)]

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path};
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

fn path(value: &Path) -> &str {
    value.to_str().expect("fixture path is UTF-8")
}

fn init_session(temp: &TempDir) -> std::path::PathBuf {
    let registry = temp.path().join("registry");
    let session = temp.path().join("session");
    canon()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "tier-c review round trip",
            "--registry",
            path(&registry),
            "--work-dir",
            path(&session),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "4",
        ])
        .assert()
        .success();
    session
}

fn observation(obs_id: &str, lineage: &str, surface: &str) -> String {
    serde_json::to_string(&json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": {"cik": null, "lei": null},
        "source": {"channel": "file", "id": "review-observations", "lineage": lineage}
    }))
    .expect("observation serializes")
}

fn same_slot(left: &str, right: &str, lineage: &str) -> String {
    serde_json::to_string(&json!({
        "type": "same_slot",
        "left": left,
        "right": right,
        "basis": {"rule": "same loan and role", "loan": lineage},
        "source": {"channel": "agent", "id": "review-agent"},
        "asserted_by": "agent:review-fixture"
    }))
    .expect("evidence serializes")
}

fn write_jsonl(path: &Path, rows: &[String]) {
    fs::write(path, format!("{}\n", rows.join("\n"))).expect("JSONL writes");
}

#[test]
fn tier_c_review_export_group_import_returns_human_evidence_and_rejects_stale() {
    let temp = tempfile::tempdir().expect("tempdir");
    let session = init_session(&temp);
    let observations_path = temp.path().join("observations.jsonl");
    write_jsonl(
        &observations_path,
        &[
            observation("obs-a", "lineage-a", "Alpha Borrower"),
            observation("obs-b", "lineage-b", "Alpha Holdings"),
            observation("obs-c", "lineage-c", "Beta Borrower"),
            observation("obs-d", "lineage-d", "Beta Holdings"),
            observation("obs-e", "lineage-e", "Gamma Borrower"),
            observation("obs-f", "lineage-f", "Gamma Holdings"),
        ],
    );
    canon()
        .args([
            "entity",
            "session",
            "ingest",
            path(&session),
            path(&observations_path),
        ])
        .assert()
        .success();

    let evidence_path = temp.path().join("evidence.jsonl");
    write_jsonl(
        &evidence_path,
        &[
            same_slot("obs-a", "obs-b", "loan-a"),
            same_slot("obs-c", "obs-d", "loan-b"),
            same_slot("obs-e", "obs-f", "loan-c"),
        ],
    );
    canon()
        .args([
            "entity",
            "session",
            "assert",
            path(&session),
            path(&evidence_path),
        ])
        .assert()
        .success();

    let rules_path = temp.path().join("rules.yaml");
    fs::write(&rules_path, RULES).expect("rules write");
    canon()
        .args([
            "entity",
            "session",
            "solve",
            path(&session),
            "--rules",
            path(&rules_path),
        ])
        .assert()
        .success();

    let solve_path = session.join("solve/solve.json");
    let queue_output = canon()
        .args([
            "entity",
            "review",
            "export",
            path(&solve_path),
            "--include",
            "all",
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let queue: Value =
        serde_json::from_slice(&queue_output.get_output().stdout).expect("review queue JSON");
    assert_eq!(queue["version"], "canon_entity_review_export.v0");
    let items = queue["items"].as_array().expect("review items");
    assert_eq!(items.len(), 3);
    let signature_id = items[0]["evidence_signature_id"]
        .as_str()
        .expect("signature id")
        .to_string();
    assert!(
        items
            .iter()
            .all(|item| { item["evidence_signature_id"].as_str() == Some(signature_id.as_str()) })
    );
    let solve_hash = queue["result"]["content_hash"]
        .as_str()
        .expect("solve hash")
        .to_string();

    let grouped_output = canon()
        .args([
            "entity",
            "review",
            "export",
            path(&solve_path),
            "--artifact",
            "native-review",
            "--group-by",
            "signature",
            "--include",
            "all",
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let grouped: Value =
        serde_json::from_slice(&grouped_output.get_output().stdout).expect("grouped review JSON");
    assert_eq!(grouped["version"], "canon_entity_native_review.v0");
    assert_eq!(grouped["review_groups"].as_array().unwrap().len(), 1);
    assert_eq!(grouped["review_groups"][0]["member_count"], 3);
    let native_signature_id = grouped["review_groups"][0]["signature_id"]
        .as_str()
        .expect("native signature id")
        .to_string();

    let explicit_review_id = items[0]["review_id"]
        .as_str()
        .expect("review id")
        .to_string();
    let decisions_path = temp.path().join("decisions.json");
    fs::write(
        &decisions_path,
        serde_json::to_vec_pretty(&json!({
            "source_solve_hash": solve_hash,
            "decisions": [
                {"signature_id": native_signature_id, "decision": "same", "note": "group reviewed"},
                {"review_id": explicit_review_id, "decision": "different", "note": "planted veto"}
            ]
        }))
        .expect("decisions serialize"),
    )
    .expect("decisions write");
    let import_output = canon()
        .args([
            "entity",
            "review",
            "import",
            path(&decisions_path),
            "--session",
            path(&session),
            "--reviewer",
            "operator-1",
        ])
        .assert()
        .success();
    let receipt: Value =
        serde_json::from_slice(&import_output.get_output().stdout).expect("session import receipt");
    assert_eq!(receipt["accepted_decisions"], 3);
    assert_eq!(receipt["asserted_alias"], 2);
    assert_eq!(receipt["distinct"], 1);

    let replay = canon()
        .args([
            "entity",
            "review",
            "import",
            path(&decisions_path),
            "--session",
            path(&session),
            "--reviewer",
            "operator-1",
        ])
        .assert()
        .success();
    let replay_receipt: Value =
        serde_json::from_slice(&replay.get_output().stdout).expect("replay receipt");
    assert_eq!(replay_receipt["accepted_decisions"], 0);
    assert_eq!(replay_receipt["already_present"], 3);

    canon()
        .args([
            "entity",
            "session",
            "solve",
            path(&session),
            "--rules",
            path(&rules_path),
        ])
        .assert()
        .success();
    let solve_again: Value =
        serde_json::from_slice(&fs::read(session.join("solve/solve.json")).expect("updated solve"))
            .expect("updated solve JSON");
    assert_eq!(solve_again["summary"]["accepted_tier_a"], 2);
    assert_eq!(solve_again["summary"]["contradiction"], 1);

    let stale_path = temp.path().join("stale.json");
    let mut stale = queue;
    stale["result"]["content_hash"] = Value::String("blake3:stale".to_string());
    fs::write(
        &stale_path,
        serde_json::to_vec(&stale).expect("stale review serializes"),
    )
    .expect("stale review write");
    canon()
        .args([
            "entity",
            "review",
            "import",
            path(&stale_path),
            "--session",
            path(&session),
            "--reviewer",
            "operator-1",
        ])
        .assert()
        .code(2);
}
