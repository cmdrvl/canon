//! Synthetic identity histories exercise the real CLI, not real-world identity truth.
use assert_cmd::Command;
use canon::temporal::compile::{
    TemporalCompileRequest, canonical_compile_bytes, compile_exact_lookup_snapshot,
};
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::tempdir;

fn claim(name: &str, id: &str, end: &str) -> Value {
    json!({
        "version":"canon.temporal.alias.v1", "claim_id":"", "claim_key":"", "conflict_key":"",
        "alias_value":name,"alias_kind":"name","entity_id":id,"lookup_visibility":"global","scope":{},
        "valid_time":{"start_at":"2025-01-01T00:00:00Z","start_bound":"inclusive","end_at":end,"end_bound":"inclusive"},
        "recorded_time":{"start_at":"2025-01-01T00:00:00Z","start_bound":"inclusive","end_at":null,"end_bound":"open","transaction_seq":1},
        "source_locator":{"source_system":"synthetic","locator":"fixture.jsonl","fragment":name},
        "materialization_digest":format!("blake3:{}","a".repeat(64)),
        "assertion_status":"accepted","trust_policy_ref":"synthetic-only.v1"
    })
}

fn request() -> Value {
    json!({
        "version":"canon.temporal.compile.v1", "registry_id":"synthetic-history","registry_version":"1.0.0",
        "valid_at":"2026-03-01T00:00:00Z","known_as_of":"2026-03-01T00:00:00Z",
        "policy":{"version":"canon.temporal.conflict_policy.v1","policy_id":"abstain-by-default","clauses":[]},
        "claims":[claim("Current Name","ent:a","2026-12-31T00:00:00Z"),claim("Stale Name","ent:a","2025-12-31T00:00:00Z")]
    })
}

fn cmd(path: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_canon"));
    cmd.args(["temporal", "compile", "--request"]).arg(path);
    cmd
}

#[test]
fn cli_compiles_native_history_deterministically_without_mutating_input() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("request.json");
    let bytes = serde_json::to_vec(&request()).unwrap();
    fs::write(&path, &bytes).unwrap();
    let first = cmd(&path)
        .arg("--no-witness")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let second = cmd(&path)
        .arg("--no-witness")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(first, second);
    let typed: TemporalCompileRequest = serde_json::from_slice(&bytes).unwrap();
    let artifact = compile_exact_lookup_snapshot(typed).unwrap();
    let mut expected = canonical_compile_bytes(&artifact).unwrap();
    expected.push(b'\n');
    assert_eq!(first, expected);
    let output: Value = serde_json::from_slice(&first).unwrap();
    let proofs = output["mapping_proofs"].as_array().unwrap();
    assert_eq!(proofs.len(), 1);
    assert_eq!(proofs[0]["input"], "Current Name");
    assert_eq!(proofs[0]["canonical_id"], "ent:a");
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    let mut older = request();
    older["valid_at"] = json!("2025-06-01T00:00:00Z");
    fs::write(&path, serde_json::to_vec(&older).unwrap()).unwrap();
    let old = cmd(&path)
        .arg("--no-witness")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&old).unwrap()["mapping_proofs"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn conflicts_are_omitted_and_do_not_become_last_writer_wins() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("request.json");
    let mut input = request();
    input["claims"].as_array_mut().unwrap().push(claim(
        "Current Name",
        "ent:b",
        "2026-12-31T00:00:00Z",
    ));
    fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
    let out = cmd(&path)
        .arg("--no-witness")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output: Value = serde_json::from_slice(&out).unwrap();
    assert!(output.get("mapping_proofs").is_none());
    assert!(
        output["omissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["reason"] == "conflict_abstained")
    );
}

#[test]
fn global_visibility_does_not_promote_asserted_claims() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("request.json");
    let mut input = request();
    input["claims"][0]["assertion_status"] = json!("asserted");
    fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
    let out = cmd(&path)
        .arg("--no-witness")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output: Value = serde_json::from_slice(&out).unwrap();
    assert!(
        output["registry_package"]["lookup_entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(output.get("mapping_proofs").is_none());
    assert!(
        output["omissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["reason"] == "assertion_not_accepted")
    );
}

#[test]
fn malformed_invalid_missing_and_oversized_requests_refuse() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("request.json");
    for input in [b"{".to_vec(), serde_json::to_vec(&json!({})).unwrap(), {
        let mut invalid = request();
        invalid["valid_at"] = json!("not-a-date");
        serde_json::to_vec(&invalid).unwrap()
    }] {
        fs::write(&path, input).unwrap();
        let out = cmd(&path)
            .arg("--no-witness")
            .assert()
            .code(2)
            .get_output()
            .stdout
            .clone();
        let refusal: Value = serde_json::from_slice(&out).unwrap();
        assert!(refusal["refusal"]["next_command"].is_string());
    }
    cmd(&path)
        .args(["--max-bytes", "1", "--no-witness"])
        .assert()
        .code(2);
    cmd(&temp.path().join("missing.json"))
        .arg("--no-witness")
        .assert()
        .code(2);
}

#[test]
fn witness_defaults_to_append_optout_and_failures_preserve_outcome() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("request.json");
    let ledger = temp.path().join("witness.jsonl");
    let bytes = serde_json::to_vec(&request()).unwrap();
    fs::write(&path, &bytes).unwrap();
    let expected = cmd(&path)
        .arg("--no-witness")
        .env("EPISTEMIC_WITNESS", &ledger)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(!ledger.exists());
    cmd(&path)
        .env("EPISTEMIC_WITNESS", &ledger)
        .assert()
        .success();
    let records = fs::read_to_string(&ledger).unwrap();
    let record: Value = serde_json::from_str(records.lines().next().unwrap()).unwrap();
    assert_eq!(
        record["inputs"][0]["hash"],
        canon::witness::hash_bytes(&bytes)
    );
    assert_eq!(record["exit_code"], 0);
    let failed = cmd(&path)
        .env("EPISTEMIC_WITNESS", temp.path())
        .assert()
        .success()
        .get_output()
        .clone();
    assert_eq!(failed.stdout, expected);
    assert!(
        String::from_utf8(failed.stderr)
            .unwrap()
            .contains("Warning: witness append skipped:")
    );
    fs::write(&path, b"{").unwrap();
    cmd(&path)
        .env("EPISTEMIC_WITNESS", &ledger)
        .assert()
        .code(2);
    assert_eq!(fs::read_to_string(&ledger).unwrap().lines().count(), 2);
}
