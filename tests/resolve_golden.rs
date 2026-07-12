use assert_cmd::prelude::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const UNCHANGED_REFERENCE_TAPE: &str =
    "tests/fixtures/resolve/parity/unchanged-link/reference_loans.link.csv";
const UNCHANGED_TARGET_TAPE: &str =
    "tests/fixtures/resolve/parity/unchanged-link/target_loans.link.csv";
const LOAN_MATCH_GOLD: &str = "tests/fixtures/resolve/gold/loan_matches.jsonl";
const UNCHANGED_DECISION_GOLDEN: &str =
    "tests/fixtures/resolve/golden/unchanged_input_decision_projection.json";

fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn canon_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    command.current_dir(manifest_dir());
    command
}

fn entity_link_stdout(
    work_root: &Path,
    strategy: &Path,
    extra_args: &[&str],
    exit_code: i32,
) -> Vec<u8> {
    let reference = Path::new(UNCHANGED_REFERENCE_TAPE);
    let target = Path::new(UNCHANGED_TARGET_TAPE);
    let work_dir = work_root.join("entity-link-work");
    let mut args = vec![
        "entity",
        "link",
        reference.to_str().unwrap(),
        target.to_str().unwrap(),
        "--profile",
        "cmbs_tenant_label",
        "--strategy",
        strategy.to_str().unwrap(),
        "--registry",
        "tests/fixtures/registries/resolve-servicers",
        "--work-dir",
        work_dir.to_str().unwrap(),
        "--no-witness",
    ];
    args.extend_from_slice(extra_args);
    canon_command()
        .args(args)
        .assert()
        .code(exit_code)
        .get_output()
        .stdout
        .clone()
}

#[test]
fn minimal_entity_link_decisions_match_golden_artifact_projection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let stdout = entity_link_stdout(
        temp_dir.path(),
        Path::new("tests/fixtures/resolve/strategies/minimal.valid.yaml"),
        &[],
        1,
    );
    let actual: Value = serde_json::from_slice(&stdout).unwrap();
    let actual_decisions = &actual["decision_artifact"];
    let expected: Value = serde_json::from_str(
        &fs::read_to_string(
            manifest_dir().join("tests/fixtures/resolve/golden/minimal_artifact.json"),
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(actual["version"], "canon_entity_link.v0");
    assert_eq!(
        actual_decisions["version"],
        "canon_entity_link_decisions.v0"
    );
    assert_eq!(actual_decisions["strategy"], expected["strategy"]);
    assert_eq!(actual_decisions["registry"], expected["registry"]);
    assert_eq!(actual_decisions["summary"], expected["summary"]);
    assert_eq!(actual_decisions["matches"], expected["matches"]);
    assert_eq!(actual_decisions["unmatched"], expected["unmatched"]);
    assert_eq!(actual_decisions["ambiguous"], expected["ambiguous"]);
}

#[test]
fn full_entity_link_json_is_byte_stable_for_same_inputs() {
    let temp_dir = tempfile::tempdir().unwrap();
    let extra = ["--gold", LOAN_MATCH_GOLD];

    let first = entity_link_stdout(
        temp_dir.path(),
        Path::new("tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml"),
        &extra,
        1,
    );
    let second = entity_link_stdout(
        temp_dir.path(),
        Path::new("tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml"),
        &extra,
        1,
    );

    assert_eq!(first, second);
}

#[test]
fn full_entity_link_decisions_match_unchanged_input_projection_golden() {
    let temp_dir = tempfile::tempdir().unwrap();
    let stdout = entity_link_stdout(
        temp_dir.path(),
        Path::new("tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml"),
        &["--gold", LOAN_MATCH_GOLD],
        1,
    );
    let actual: Value = serde_json::from_slice(&stdout).unwrap();
    let expected: Value = serde_json::from_str(
        &fs::read_to_string(manifest_dir().join(UNCHANGED_DECISION_GOLDEN)).unwrap(),
    )
    .unwrap();

    let decisions = &actual["decision_artifact"];
    assert_eq!(
        golden_decision_projection(decisions),
        expected["projection"]
    );
    assert!(
        !decisions
            .as_object()
            .expect("decision object")
            .contains_key("write_back")
    );
}

#[test]
fn json_and_summary_modes_agree_on_core_counts() {
    let temp_dir = tempfile::tempdir().unwrap();
    let json_stdout = entity_link_stdout(
        temp_dir.path(),
        Path::new("tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml"),
        &[],
        1,
    );
    let payload: Value = serde_json::from_slice(&json_stdout).unwrap();
    let summary_stdout = entity_link_stdout(
        temp_dir.path(),
        Path::new("tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml"),
        &["--emit", "summary"],
        1,
    );
    let summary = String::from_utf8(summary_stdout).unwrap();
    let values = parse_summary(&summary);

    assert_eq!(
        values.get("target_records"),
        Some(&payload["decision_artifact"]["summary"]["target_records"].to_string())
    );
    assert_eq!(
        values.get("matched"),
        Some(&payload["decision_artifact"]["summary"]["matched"].to_string())
    );
    assert_eq!(
        values.get("unmatched"),
        Some(&payload["decision_artifact"]["summary"]["unmatched"].to_string())
    );
    assert_eq!(
        values.get("ambiguous"),
        Some(&payload["decision_artifact"]["summary"]["ambiguous"].to_string())
    );
    assert_eq!(values.get("match_rate"), Some(&"0.750".to_string()));
}

fn parse_summary(summary: &str) -> BTreeMap<String, String> {
    summary
        .split_whitespace()
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

fn golden_decision_projection(decisions: &Value) -> Value {
    json!({
        "strategy": decisions["strategy"],
        "registry": {
            "id": decisions["registry"]["id"],
            "version": decisions["registry"]["version"]
        },
        "reference": {
            "rows_path": decisions["reference_tape"]["path"],
            "row_count": decisions["reference_tape"]["record_count"]
        },
        "target": {
            "rows_path": decisions["target_tape"]["path"],
            "row_count": decisions["target_tape"]["record_count"]
        },
        "summary": decisions["summary"],
        "matches": compact_matches(decisions),
        "unmatched": compact_unmatched(decisions),
        "ambiguous": compact_ambiguous(decisions),
        "gold_score": decisions["gold_score"],
        "read_only": {
            "write_back_present": decisions
                .as_object()
                .expect("decision object")
                .contains_key("write_back")
        }
    })
}

fn compact_matches(decisions: &Value) -> Value {
    Value::Array(
        decisions["matches"]
            .as_array()
            .expect("matches")
            .iter()
            .map(|record| {
                json!({
                    "target_id": record["target_id"],
                    "reference_id": record["reference_id"],
                    "canonical_id": record["canonical_id"],
                    "score": record["score"]
                })
            })
            .collect(),
    )
}

fn compact_unmatched(decisions: &Value) -> Value {
    Value::Array(
        decisions["unmatched"]
            .as_array()
            .expect("unmatched")
            .iter()
            .map(|record| {
                json!({
                    "target_id": record["target_id"],
                    "reason": record["reason"]
                })
            })
            .collect(),
    )
}

fn compact_ambiguous(decisions: &Value) -> Value {
    Value::Array(
        decisions["ambiguous"]
            .as_array()
            .expect("ambiguous")
            .iter()
            .map(|record| {
                let candidate_reference_ids = record["candidates"]
                    .as_array()
                    .expect("candidate array")
                    .iter()
                    .map(|candidate| candidate["reference_id"].clone())
                    .collect::<Vec<_>>();
                json!({
                    "target_id": record["target_id"],
                    "reason": record["reason"],
                    "candidate_reference_ids": candidate_reference_ids
                })
            })
            .collect(),
    )
}
