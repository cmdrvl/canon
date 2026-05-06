use assert_cmd::prelude::*;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn canon_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    command.current_dir(manifest_dir());
    command
}

fn resolve_stdout(args: &[&str], exit_code: i32) -> Vec<u8> {
    canon_command()
        .args(args)
        .assert()
        .code(exit_code)
        .get_output()
        .stdout
        .clone()
}

#[test]
fn minimal_resolve_json_matches_golden_artifact() {
    let stdout = resolve_stdout(
        &[
            "resolve",
            "tests/fixtures/resolve/tapes/reference_loans.csv",
            "tests/fixtures/resolve/tapes/target_loans.csv",
            "--strategy",
            "tests/fixtures/resolve/strategies/minimal.valid.yaml",
            "--registry",
            "tests/fixtures/registries/resolve-servicers",
            "--no-witness",
        ],
        1,
    );
    let actual: Value = serde_json::from_slice(&stdout).unwrap();
    let expected: Value = serde_json::from_str(
        &fs::read_to_string(
            manifest_dir().join("tests/fixtures/resolve/golden/minimal_artifact.json"),
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(actual, expected);
}

#[test]
fn full_resolve_json_is_byte_stable_for_same_inputs() {
    let args = [
        "resolve",
        "tests/fixtures/resolve/tapes/reference_loans.csv",
        "tests/fixtures/resolve/tapes/target_loans.csv",
        "--strategy",
        "tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml",
        "--registry",
        "tests/fixtures/registries/resolve-servicers",
        "--gold",
        "tests/fixtures/resolve/gold/loan_matches.jsonl",
        "--no-witness",
    ];

    let first = resolve_stdout(&args, 1);
    let second = resolve_stdout(&args, 1);

    assert_eq!(first, second);
}

#[test]
fn json_and_summary_modes_agree_on_core_counts() {
    let json_stdout = resolve_stdout(
        &[
            "resolve",
            "tests/fixtures/resolve/tapes/reference_loans.csv",
            "tests/fixtures/resolve/tapes/target_loans.csv",
            "--strategy",
            "tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml",
            "--registry",
            "tests/fixtures/registries/resolve-servicers",
            "--no-witness",
        ],
        1,
    );
    let payload: Value = serde_json::from_slice(&json_stdout).unwrap();
    let summary_stdout = resolve_stdout(
        &[
            "resolve",
            "tests/fixtures/resolve/tapes/reference_loans.csv",
            "tests/fixtures/resolve/tapes/target_loans.csv",
            "--strategy",
            "tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml",
            "--registry",
            "tests/fixtures/registries/resolve-servicers",
            "--emit",
            "summary",
            "--no-witness",
        ],
        1,
    );
    let summary = String::from_utf8(summary_stdout).unwrap();
    let values = parse_summary(&summary);

    assert_eq!(
        values.get("target_records"),
        Some(&payload["summary"]["target_records"].to_string())
    );
    assert_eq!(
        values.get("matched"),
        Some(&payload["summary"]["matched"].to_string())
    );
    assert_eq!(
        values.get("unmatched"),
        Some(&payload["summary"]["unmatched"].to_string())
    );
    assert_eq!(
        values.get("ambiguous"),
        Some(&payload["summary"]["ambiguous"].to_string())
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
