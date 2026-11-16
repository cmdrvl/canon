use assert_cmd::Command;
use canon::{
    entity::{
        evidence_ir::{EvidenceRecord, canonicalize_record},
        publication::{EntityPublicationSnapshot, open_current_stream_generation},
    },
    session::SESSION_EVIDENCE_STREAM_ID,
};
use serde_json::{Value, json};
use std::{fs, path::Path};
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
    serde_json::from_slice(&output.get_output().stdout).expect("JSON stdout envelope")
}

fn init_session(temp: &TempDir) -> std::path::PathBuf {
    let registry = temp.path().join("registry");
    let session = temp.path().join("session");
    let output = canon_command()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "assert evidence",
            "--registry",
            registry.to_str().unwrap(),
            "--work-dir",
            session.to_str().unwrap(),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "4",
            "--emit",
            "json",
        ])
        .assert()
        .success();
    run_and_log("session init", &output);
    session
}

fn assert_file(session: &Path, input: &Path) -> assert_cmd::assert::Assert {
    canon_command()
        .args([
            "entity",
            "session",
            "assert",
            session.to_str().unwrap(),
            input.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
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
        "source": {"channel": "agent", "id": "agent:session-run"},
        "asserted_by": "agent:session-run"
    }))
    .unwrap()
}

fn name_equivalent(left: &str, right: &str) -> String {
    serde_json::to_string(&json!({
        "type": "name_equivalent",
        "left": left,
        "right": right,
        "basis": {
            "rule": "casefold+strip_punct+strip_legal_suffix",
            "left_value": "Acme Holdings, LLC",
            "right_value": "ACME HOLDINGS LLC",
            "steps": ["casefold", "strip_punctuation", "strip_legal_suffix:llc"]
        },
        "source": {
            "channel": "script",
            "id": "goldset:acme",
            "locator": "row:2",
            "content_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        },
        "asserted_by": "script:goldset"
    }))
    .unwrap()
}

fn full_form() -> String {
    serde_json::to_string(&json!({
        "version": "canon.evidence.v1",
        "evidence_id": "",
        "kind": "pair_support",
        "target": {"target_kind": "pair", "left_id": "full-left", "right_id": "full-right"},
        "operator": {
            "namespace": "test",
            "operator_id": "full_record",
            "operator_version": "v1"
        },
        "reason_code": "same_key",
        "policy": {
            "policy_id": "test-policy",
            "policy_version": "v1",
            "content_hash": "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        },
        "provenance": [{
            "source_type": "file",
            "source_id": "full.jsonl",
            "locator": "line:1",
            "content_hash": "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
        }],
        "extensions": [{
            "namespace": "test",
            "schema_ref": "test.full.v1",
            "payload": {"basis": "full-form"}
        }]
    }))
    .unwrap()
}

fn evidence_records(snapshot: &EntityPublicationSnapshot) -> Vec<EvidenceRecord> {
    let mut records = Vec::new();
    for path in snapshot
        .logical_paths()
        .filter(|path| path.starts_with("evidence/") && path.ends_with(".jsonl"))
    {
        let bytes = snapshot
            .read_logical_file(path)
            .expect("evidence shard bytes");
        for line in std::str::from_utf8(bytes)
            .expect("evidence shard UTF-8")
            .lines()
            .filter(|line| !line.trim().is_empty())
        {
            let record: EvidenceRecord = serde_json::from_str(line).expect("evidence record JSON");
            records.push(canonicalize_record(record).expect("canonical evidence record"));
        }
    }
    records
}

#[test]
fn session_assert_accepts_file_stdin_full_form_and_exact_repeats() {
    let temp = TempDir::new().expect("temp root");
    let session = init_session(&temp);
    let first_input = temp.path().join("first.jsonl");
    fs::write(
        &first_input,
        format!(
            "{}\n{}\n",
            same_slot("future-left", "future-right"),
            full_form()
        ),
    )
    .expect("first evidence input");

    let first = assert_file(&session, &first_input).success();
    run_and_log("assert file", &first);
    let first_payload = json_stdout(&first);
    assert_eq!(first_payload["version"], "canon.entity.session.assert.v1");
    assert_eq!(first_payload["accepted"], 2);
    assert_eq!(first_payload["already_present"], 0);
    assert_eq!(first_payload["rejected"].as_array().unwrap().len(), 0);
    let first_generation = first_payload["generation_id"].clone();

    let stdin_line = name_equivalent("future-left", "future-name");
    let second = canon_command()
        .args([
            "entity",
            "session",
            "assert",
            session.to_str().unwrap(),
            "-",
            "--emit",
            "json",
        ])
        .write_stdin(format!("{stdin_line}\n"))
        .assert()
        .success();
    run_and_log("assert stdin", &second);
    let second_payload = json_stdout(&second);
    assert_eq!(second_payload["accepted"], 1);
    assert_ne!(second_payload["generation_id"], first_generation);

    let snapshot = open_current_stream_generation(&session, SESSION_EVIDENCE_STREAM_ID)
        .expect("evidence stream");
    let records = evidence_records(&snapshot);
    assert_eq!(records.len(), 3);
    assert_eq!(snapshot.generation_id, second_payload["generation_id"]);
    assert!(
        records
            .iter()
            .all(|record| record.version == "canon.evidence.v1")
    );
    assert!(
        records
            .iter()
            .any(|record| record.operator.operator_id == "full_record"),
        "full canon.evidence.v1 input must be retained"
    );

    let repeat = assert_file(&session, &first_input).success();
    run_and_log("repeat evidence file", &repeat);
    let repeat_payload = json_stdout(&repeat);
    assert_eq!(repeat_payload["accepted"], 0);
    assert_eq!(repeat_payload["already_present"], 2);
    assert_eq!(
        repeat_payload["generation_id"],
        second_payload["generation_id"]
    );
}

#[test]
fn session_assert_reports_planted_bad_rows_and_keeps_valid_rows() {
    let temp = TempDir::new().expect("temp root");
    let session = init_session(&temp);
    let input = temp.path().join("mixed.jsonl");
    fs::write(
        &input,
        format!(
            "{}\n{{not-valid-json\n{}\n{}\n",
            same_slot("valid-left", "valid-right"),
            serde_json::to_string(&json!({
                "type": "distinct",
                "left": "distinct-left",
                "right": "distinct-right",
                "basis": {"rule": "different"},
                "source": {"channel": "agent", "id": "agent:runner"},
                "asserted_by": "agent:runner"
            }))
            .unwrap(),
            serde_json::to_string(&json!({
                "type": "asserted_alias",
                "left": "alias-left",
                "right": "alias-right",
                "basis": {"rule": "document states alias"},
                "source": {"channel": "agent", "id": "agent:runner"},
                "asserted_by": "agent:runner"
            }))
            .unwrap(),
        ),
    )
    .expect("mixed evidence input");

    let output = assert_file(&session, &input).code(1);
    run_and_log("mixed evidence", &output);
    let payload = json_stdout(&output);
    assert_eq!(payload["accepted"], 1);
    let rejected = payload["rejected"].as_array().unwrap();
    assert_eq!(rejected.len(), 3);
    assert_eq!(rejected[0]["line"], 2);
    assert_eq!(rejected[0]["code"], "invalid_json");
    assert_eq!(rejected[1]["line"], 3);
    assert_eq!(rejected[1]["code"], "missing_field:authority");
    assert_eq!(rejected[2]["line"], 4);
    assert_eq!(rejected[2]["code"], "asserted_alias_citation");
    for item in rejected {
        assert!(item["fix"].as_str().is_some_and(|fix| !fix.is_empty()));
    }

    let snapshot = open_current_stream_generation(&session, SESSION_EVIDENCE_STREAM_ID)
        .expect("evidence stream after partial assertion");
    assert_eq!(evidence_records(&snapshot).len(), 1);
}
