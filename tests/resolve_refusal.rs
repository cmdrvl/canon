use assert_cmd::prelude::*;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::tempdir;

fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn canon_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    command.current_dir(manifest_dir());
    command
}

fn refusal(args: &[&str]) -> Value {
    let assert = canon_command().args(args).assert().code(2);
    serde_json::from_slice(&assert.get_output().stdout).expect("refusal stdout is json")
}

#[test]
fn malformed_strategy_refuses_with_bad_strategy() {
    let payload = refusal(&[
        "resolve",
        "tests/fixtures/resolve/tapes/reference_loans.csv",
        "tests/fixtures/resolve/tapes/target_loans.csv",
        "--strategy",
        "tests/fixtures/resolve/strategies/malformed_missing_threshold.yaml",
        "--registry",
        "tests/fixtures/registries/resolve-servicers",
        "--no-witness",
    ]);

    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_BAD_STRATEGY");
    assert!(
        payload["refusal"]["detail"]["reason"]
            .as_str()
            .unwrap()
            .contains("match_threshold")
    );
}

#[test]
fn empty_target_tape_refuses_with_empty_tape() {
    let payload = refusal(&[
        "resolve",
        "tests/fixtures/resolve/tapes/reference_loans.csv",
        "tests/fixtures/resolve/tapes/empty_target.csv",
        "--strategy",
        "tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml",
        "--registry",
        "tests/fixtures/registries/resolve-servicers",
        "--no-witness",
    ]);

    assert_eq!(payload["refusal"]["code"], "E_EMPTY_TAPE");
    assert_eq!(payload["refusal"]["detail"]["side"], "target");
}

#[test]
fn too_many_candidates_refusal_reports_target_and_limit() {
    let payload = refusal(&[
        "resolve",
        "tests/fixtures/resolve/tapes/too_many_candidates_reference.csv",
        "tests/fixtures/resolve/tapes/too_many_candidates_target.csv",
        "--strategy",
        "tests/fixtures/resolve/strategies/too_many_candidates.yaml",
        "--registry",
        "tests/fixtures/registries/resolve-servicers",
        "--no-witness",
    ]);

    assert_eq!(payload["refusal"]["code"], "E_TOO_MANY_CANDIDATES");
    assert_eq!(
        payload["refusal"]["detail"]["target_id"],
        "WFCM2019-C50|900"
    );
    assert_eq!(payload["refusal"]["detail"]["candidate_count"], 4);
    assert_eq!(payload["refusal"]["detail"]["max_candidates"], 1);
}

#[test]
fn missing_column_refusal_includes_available_columns() {
    let payload = refusal(&[
        "resolve",
        "tests/fixtures/resolve/tapes/reference_loans.csv",
        "tests/fixtures/resolve/tapes/missing_column_target.csv",
        "--strategy",
        "tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml",
        "--registry",
        "tests/fixtures/registries/resolve-servicers",
        "--no-witness",
    ]);

    assert_eq!(payload["refusal"]["code"], "E_COLUMN_NOT_FOUND");
    assert_eq!(payload["refusal"]["detail"]["side"], "target");
    assert_eq!(payload["refusal"]["detail"]["column"], "balance");
    let available = payload["refusal"]["detail"]["available_columns"]
        .as_array()
        .expect("available_columns");
    assert!(available.iter().any(|column| column == "address"));
    assert!(!available.iter().any(|column| column == "balance"));
}

#[test]
fn max_rows_refusal_uses_existing_too_large_envelope() {
    let payload = refusal(&[
        "resolve",
        "tests/fixtures/resolve/tapes/reference_loans.csv",
        "tests/fixtures/resolve/tapes/target_loans.csv",
        "--strategy",
        "tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml",
        "--registry",
        "tests/fixtures/registries/resolve-servicers",
        "--max-rows",
        "1",
        "--no-witness",
    ]);

    assert_eq!(payload["refusal"]["code"], "E_TOO_LARGE");
    assert_eq!(payload["refusal"]["detail"]["limit_type"], "max_rows");
}

#[test]
fn core_lookup_stays_exact_and_does_not_run_structural_resolution() {
    let temp_dir = tempdir().unwrap();
    let input = temp_dir.path().join("lookup.jsonl");
    fs::write(
        &input,
        "{\"id\":\"WFCM2019-C50|1\"}\n{\"id\":\"Wells Fargo\"}\n",
    )
    .unwrap();

    let assert = canon_command()
        .args([
            input.to_str().unwrap(),
            "--registry",
            "tests/fixtures/registries/resolve-servicers",
            "--column",
            "id",
            "--explicit",
            "--no-witness",
        ])
        .assert()
        .code(1);
    let payload: Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();

    assert_eq!(payload["summary"]["resolved"], 1);
    assert_eq!(payload["summary"]["unresolved"], 1);
    assert_eq!(payload["mappings"][0]["input"], "u8:Wells Fargo");
    assert_eq!(
        payload["mappings"][0]["canonical_id"],
        "u8:SERVICER-WELLS-FARGO"
    );
    assert_eq!(payload["unresolved"][0]["input"], "u8:WFCM2019-C50|1");
}
