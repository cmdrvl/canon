use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::tempdir;

fn fact(
    subject_id: &str,
    subject_type: &str,
    relation: &str,
    object_id: &str,
    object_type: &str,
    start_at: &str,
    end_at: Option<&str>,
) -> Value {
    json!({
        "version": "canon.identity.relation.v1",
        "relation_id": "",
        "edge_key": "",
        "subject": {
            "identity_id": subject_id,
            "entity_type": {"kind": "core", "class": subject_type}
        },
        "relation": {"kind": "core", "class": relation},
        "object": {
            "identity_id": object_id,
            "entity_type": {"kind": "core", "class": object_type}
        },
        "valid_time": {
            "start_at": start_at,
            "start_bound": "inclusive",
            "end_at": end_at,
            "end_bound": if end_at.is_some() { "inclusive" } else { "open" }
        },
        "provenance": {
            "source_system": "test-public-record",
            "locator": "test-fixture/notice#row-1",
            "fragment": "row-1",
            "observed_at": "2026-11-06T12:00:00Z"
        },
        "policy_ref": "relation.test.v1",
        "constraints": {
            "max_objects_per_subject": null,
            "max_subjects_per_object": null,
            "overlap_policy": "allow_overlapping_targets",
            "cycle_policy": "disallow_directed_cycle"
        },
        "review": {"disposition": "related", "reason_code": "reviewed_relation"}
    })
}

fn run_facts(path: &Path, facts: &Value, extra_args: &[&str]) -> std::process::Output {
    run_raw(path, &serde_json::to_vec_pretty(facts).unwrap(), extra_args)
}

fn run_raw(path: &Path, input: &[u8], extra_args: &[&str]) -> std::process::Output {
    fs::write(path, input).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    command
        .args(["temporal", "relation", "finalize", "--facts"])
        .arg(path)
        .args(["--no-witness"])
        .args(extra_args)
        .output()
        .unwrap()
}

fn assert_contract_refusal(output: &std::process::Output) -> Value {
    assert_eq!(output.status.code(), Some(2));
    let refusal: Value = serde_json::from_slice(&output.stdout).expect("JSON refusal on stdout");
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert!(
        refusal["refusal"]["next_command"]
            .as_str()
            .unwrap()
            .contains("canon temporal relation finalize")
    );
    refusal
}

#[test]
fn native_cli_finalizes_dated_successor_hierarchy_and_role_facts_deterministically() {
    let dir = tempdir().unwrap();
    let path_a = dir.path().join("relations-a.json");
    let path_b = dir.path().join("relations-b.json");

    let successor = fact(
        "issuer:former",
        "organization",
        "succession",
        "issuer:successor",
        "organization",
        "2025-07-01T00:00:00Z",
        None,
    );
    let hierarchy = fact(
        "issuer:parent",
        "organization",
        "hierarchy",
        "issuer:child",
        "organization",
        "2025-01-01T00:00:00Z",
        Some("2025-12-31T23:59:59Z"),
    );
    let role = fact(
        "person:investment-officer",
        "person",
        "role",
        "issuer:borrower",
        "organization",
        "2025-03-01T00:00:00Z",
        Some("2025-09-30T23:59:59Z"),
    );
    let mut same_review = fact(
        "issuer:brand",
        "organization",
        "association",
        "issuer:operator",
        "organization",
        "2025-01-01T00:00:00Z",
        None,
    );
    same_review["review"]["disposition"] = json!("same");

    let ordered = json!([successor, hierarchy, role, same_review]);
    let reversed = json!([
        ordered[3].clone(),
        ordered[2].clone(),
        ordered[1].clone(),
        ordered[0].clone()
    ]);
    let first = run_facts(&path_a, &ordered, &[]);
    assert_eq!(
        first.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let second = run_facts(&path_b, &reversed, &[]);
    assert_eq!(
        second.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(first.stdout, second.stdout);

    let finalized: Vec<Value> = serde_json::from_slice(&first.stdout).expect("relation fact array");
    assert_eq!(finalized.len(), 4);
    assert!(
        finalized
            .iter()
            .all(|row| row["version"] == "canon.identity.relation.v1")
    );
    assert!(
        finalized
            .iter()
            .all(|row| row["relation_id"].as_str().unwrap().starts_with("blake3:"))
    );
    assert!(
        finalized
            .iter()
            .all(|row| row["edge_key"].as_str().unwrap().starts_with("blake3:"))
    );
    assert!(
        finalized
            .iter()
            .all(|row| row["identity_implication"]["mode"] == "none")
    );
    let successor = finalized
        .iter()
        .find(|row| row["relation"]["class"] == "succession")
        .unwrap();
    assert_eq!(successor["valid_time"]["start_at"], "2025-07-01T00:00:00Z");
    assert_eq!(
        successor["provenance"]["locator"],
        "test-fixture/notice#row-1"
    );
    let same = finalized
        .iter()
        .find(|row| row["review"]["disposition"] == "same")
        .unwrap();
    assert_eq!(same["identity_implication"]["mode"], "none");
    assert!(
        finalized
            .iter()
            .all(|row| row.get("canonical_id").is_none())
    );
}

#[test]
fn native_cli_deduplicates_identical_relation_facts_and_reports_read_only_summary() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("relations.json");
    let successor = fact(
        "issuer:former",
        "organization",
        "succession",
        "issuer:successor",
        "organization",
        "2025-07-01T00:00:00Z",
        None,
    );
    let output = run_facts(
        &path,
        &json!([successor.clone(), successor]),
        &["--emit", "summary"],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "finalized relations=1 registry_written=false aliases_written=false"
    );
}

#[test]
fn native_cli_refuses_invalid_interval_provenance_cycles_and_cardinality() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("relations.json");

    let mut invalid_interval = fact(
        "issuer:a",
        "organization",
        "succession",
        "issuer:b",
        "organization",
        "2025-01-01T00:00:00Z",
        None,
    );
    invalid_interval["valid_time"]["start_at"] = json!("not-an-rfc3339-timestamp");
    let refusal = assert_contract_refusal(&run_facts(&path, &json!([invalid_interval]), &[]));
    assert!(
        refusal["refusal"]["detail"]["message"]
            .as_str()
            .unwrap()
            .contains("RFC3339")
    );

    let mut invalid_provenance = fact(
        "issuer:a",
        "organization",
        "hierarchy",
        "issuer:b",
        "organization",
        "2025-01-01T00:00:00Z",
        None,
    );
    invalid_provenance["provenance"]["source_system"] = json!(" ");
    let refusal = assert_contract_refusal(&run_facts(&path, &json!([invalid_provenance]), &[]));
    assert!(
        refusal["refusal"]["detail"]["message"]
            .as_str()
            .unwrap()
            .contains("source_system")
    );

    let cycle = json!([
        fact(
            "org:a",
            "organization",
            "hierarchy",
            "org:b",
            "organization",
            "2025-01-01T00:00:00Z",
            None
        ),
        fact(
            "org:b",
            "organization",
            "hierarchy",
            "org:c",
            "organization",
            "2025-01-01T00:00:00Z",
            None
        ),
        fact(
            "org:c",
            "organization",
            "hierarchy",
            "org:a",
            "organization",
            "2025-01-01T00:00:00Z",
            None
        ),
    ]);
    let refusal = assert_contract_refusal(&run_facts(&path, &cycle, &[]));
    assert_eq!(
        refusal["refusal"]["detail"]["relation_error_code"],
        "policy_constraint"
    );

    let mut role_a = fact(
        "person:owner",
        "person",
        "role",
        "org:a",
        "organization",
        "2025-01-01T00:00:00Z",
        None,
    );
    let mut role_b = fact(
        "person:owner",
        "person",
        "role",
        "org:b",
        "organization",
        "2025-01-01T00:00:00Z",
        None,
    );
    for role in [&mut role_a, &mut role_b] {
        role["constraints"]["max_objects_per_subject"] = json!(1);
        role["constraints"]["overlap_policy"] = json!("disallow_overlapping_targets");
    }
    let refusal = assert_contract_refusal(&run_facts(&path, &json!([role_a, role_b]), &[]));
    assert_eq!(
        refusal["refusal"]["detail"]["relation_error_code"],
        "policy_constraint"
    );
}

#[test]
fn native_cli_refuses_relation_input_over_the_declared_byte_limit() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("relations.json");
    let output = run_facts(&path, &json!([]), &["--max-bytes", "1"]);
    assert_eq!(output.status.code(), Some(2));
    let refusal: Value = serde_json::from_slice(&output.stdout).expect("JSON refusal on stdout");
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_PARSE");
    assert_eq!(refusal["refusal"]["detail"]["max_bytes"], 1);
}

#[test]
fn native_cli_refuses_malformed_relation_json_as_parse_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("relations.json");
    let output = run_raw(&path, b"[{\"version\":", &[]);
    assert_eq!(output.status.code(), Some(2));
    let refusal: Value = serde_json::from_slice(&output.stdout).expect("JSON refusal on stdout");
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_PARSE");
    assert!(refusal["refusal"]["detail"]["error"].is_string());
}
