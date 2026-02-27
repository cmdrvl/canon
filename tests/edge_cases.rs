use serde_json::Value;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use tempfile::tempdir;

fn fixture(path: &str) -> String {
    PathBuf::from("tests/fixtures")
        .join(path)
        .to_string_lossy()
        .to_string()
}

fn run_canon(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .output()
        .unwrap()
}

fn run_canon_with_stdin(args: &[&str], stdin_data: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin_data.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn parse_stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}

fn unresolved_reason_count(payload: &Value, reason: &str) -> usize {
    payload["unresolved"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["reason"] == reason)
        .count()
}

fn unresolved_contains(payload: &Value, input: Option<&str>, reason: &str) -> bool {
    payload["unresolved"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| {
            let reason_matches = entry["reason"] == reason;
            let input_matches = match input {
                Some(value) => entry["input"] == value,
                None => entry["input"].is_null(),
            };
            reason_matches && input_matches
        })
}

#[test]
fn jsonl_missing_null_and_non_scalar_reasons_use_null_input() {
    let missing_output = run_canon(&[
        &fixture("inputs/missing_fields.jsonl"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
    ]);
    assert_eq!(missing_output.status.code(), Some(1));
    let missing_payload = parse_stdout_json(&missing_output);
    assert!(unresolved_contains(&missing_payload, None, "missing_field"));

    let null_output = run_canon(&[
        &fixture("inputs/null_fields.jsonl"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
    ]);
    assert_eq!(null_output.status.code(), Some(1));
    let null_payload = parse_stdout_json(&null_output);
    assert!(unresolved_contains(&null_payload, None, "null_value"));

    let non_scalar_output = run_canon(&[
        &fixture("inputs/non_scalar.jsonl"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
    ]);
    assert_eq!(non_scalar_output.status.code(), Some(1));
    let non_scalar_payload = parse_stdout_json(&non_scalar_output);
    assert!(unresolved_contains(
        &non_scalar_payload,
        None,
        "non_scalar_value"
    ));
    assert_eq!(
        unresolved_reason_count(&non_scalar_payload, "non_scalar_value"),
        1
    );
}

#[test]
fn jsonl_mixed_types_are_coerced_and_special_reasons_dedup() {
    let mixed_output = run_canon(&[
        &fixture("inputs/mixed_types.jsonl"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
    ]);
    assert_eq!(mixed_output.status.code(), Some(1));
    let mixed_payload = parse_stdout_json(&mixed_output);
    assert!(unresolved_contains(
        &mixed_payload,
        Some("u8:42"),
        "no matching rule"
    ));
    assert!(unresolved_contains(
        &mixed_payload,
        Some("u8:true"),
        "no matching rule"
    ));
    assert!(unresolved_contains(&mixed_payload, None, "empty_value"));

    let temp = tempdir().unwrap();
    let path = temp.path().join("dedup.jsonl");
    fs::write(
        &path,
        "{\"amount\":1}\n{\"amount\":2}\n{\"cusip\":null}\n{\"cusip\":null}\n{\"cusip\":{}}\n{\"cusip\":[]}\n{\"cusip\":\"\"}\n{\"cusip\":\"\"}\n",
    )
    .unwrap();

    let dedup_output = run_canon(&[
        path.to_str().unwrap(),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
    ]);
    assert_eq!(dedup_output.status.code(), Some(1));
    let dedup_payload = parse_stdout_json(&dedup_output);
    assert_eq!(unresolved_reason_count(&dedup_payload, "missing_field"), 1);
    assert_eq!(unresolved_reason_count(&dedup_payload, "null_value"), 1);
    assert_eq!(
        unresolved_reason_count(&dedup_payload, "non_scalar_value"),
        1
    );
    assert_eq!(unresolved_reason_count(&dedup_payload, "empty_value"), 1);
}

#[test]
fn jsonl_stdin_mode_reads_from_dash_input() {
    let stdin_data = fs::read_to_string(fixture("inputs/basic.jsonl")).unwrap();
    let output = run_canon_with_stdin(
        &[
            "-",
            "--registry",
            &fixture("registries/cusip-isin"),
            "--column",
            "cusip",
        ],
        &stdin_data,
    );
    assert_eq!(output.status.code(), Some(0));
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["outcome"], "RESOLVED");
    assert_eq!(payload["summary"]["resolved"], 3);
}

#[test]
fn csv_blank_rows_and_empty_values_are_classified_correctly() {
    let blank_output = run_canon(&[
        &fixture("inputs/blank_rows.csv"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
    ]);
    assert_eq!(blank_output.status.code(), Some(0));
    let blank_payload = parse_stdout_json(&blank_output);
    assert_eq!(blank_payload["summary"]["total"], 3);
    assert_eq!(blank_payload["summary"]["resolved"], 3);
    assert_eq!(blank_payload["summary"]["unresolved"], 0);

    let empty_output = run_canon(&[
        &fixture("inputs/empty_values.csv"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
    ]);
    assert_eq!(empty_output.status.code(), Some(1));
    let empty_payload = parse_stdout_json(&empty_output);
    assert_eq!(empty_payload["summary"]["total"], 3);
    assert_eq!(empty_payload["summary"]["resolved"], 2);
    assert_eq!(empty_payload["summary"]["unresolved"], 1);
    assert!(unresolved_contains(&empty_payload, None, "empty_value"));
}

#[test]
fn csv_duplicate_rows_resolve_once_per_unique_value() {
    let temp = tempdir().unwrap();
    let input_path = temp.path().join("dupes.csv");
    generate_large_csv(&input_path, 100, 5);

    let output = run_canon(&[
        input_path.to_str().unwrap(),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["summary"]["total"], 5);
    assert_eq!(payload["mappings"].as_array().unwrap().len(), 3);
    assert_eq!(payload["unresolved"].as_array().unwrap().len(), 2);
}

#[test]
fn stress_large_input_and_limits() {
    let temp = tempdir().unwrap();
    let input_path = temp.path().join("large.csv");
    generate_large_csv(&input_path, 100_000, 5);

    let large_output = run_canon(&[
        input_path.to_str().unwrap(),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
    ]);
    assert_eq!(large_output.status.code(), Some(1));
    let large_payload = parse_stdout_json(&large_output);
    assert_eq!(large_payload["summary"]["total"], 5);

    let max_rows_output = run_canon(&[
        input_path.to_str().unwrap(),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
        "--max-rows",
        "10",
    ]);
    assert_eq!(max_rows_output.status.code(), Some(2));
    let max_rows_payload = parse_stdout_json(&max_rows_output);
    assert_eq!(max_rows_payload["refusal"]["code"], "E_TOO_LARGE");

    let max_bytes_output = run_canon(&[
        input_path.to_str().unwrap(),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
        "--max-bytes",
        "100",
    ]);
    assert_eq!(max_bytes_output.status.code(), Some(2));
    let max_bytes_payload = parse_stdout_json(&max_bytes_output);
    assert_eq!(max_bytes_payload["refusal"]["code"], "E_TOO_LARGE");
}

fn generate_large_csv(path: &Path, rows: usize, unique_values: usize) {
    let values = [
        "037833100",
        "594918104",
        "17275R102",
        "UNKNOWN99",
        "UNKNOWN98",
    ];
    let mut file = File::create(path).unwrap();
    writeln!(file, "cusip,amount").unwrap();
    for row in 0..rows {
        let value = values[row % unique_values];
        writeln!(file, "{},{}", value, row).unwrap();
    }
}
