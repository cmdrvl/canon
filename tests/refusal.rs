use assert_cmd::Command;
use serde_json::Value;
use std::path::PathBuf;

fn fixture(path: &str) -> String {
    PathBuf::from("tests/fixtures")
        .join(path)
        .to_string_lossy()
        .to_string()
}

fn run_json_refusal(args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    parse_refusal_json(&output.stdout)
}

fn run_csv_refusal(args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    parse_refusal_json(&output.stderr)
}

fn parse_refusal_json(bytes: &[u8]) -> Value {
    if let Ok(payload) = serde_json::from_slice(bytes) {
        return payload;
    }
    let text = String::from_utf8_lossy(bytes);
    text.lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line).ok())
        .unwrap()
}

fn assert_refusal_envelope(payload: &Value, code: &str) {
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["registry"], Value::Null);
    assert_eq!(payload["summary"], Value::Null);
    assert_eq!(payload["mappings"], Value::Array(vec![]));
    assert_eq!(payload["unresolved"], Value::Array(vec![]));
    assert_eq!(payload["refusal"]["code"], code);
    assert!(payload["refusal"]["message"].is_string());
    assert!(payload["refusal"]["detail"].is_object());
    // Every refusal is an operator handoff, not a dead end: the envelope must
    // always carry a non-empty recovery path. Regression guard for the core
    // pipeline that previously emitted next_command: null on most codes.
    let next_command = &payload["refusal"]["next_command"];
    assert!(
        next_command.is_string(),
        "refusal {code} must include a next_command recovery path, got {next_command:?}"
    );
    assert!(
        !next_command.as_str().unwrap().trim().is_empty(),
        "refusal {code} next_command must not be empty"
    );
}

#[test]
fn refusal_e_io_nonexistent_input_file() {
    let payload = run_json_refusal(&[
        &fixture("csv/missing.csv"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
    ]);
    assert_refusal_envelope(&payload, "E_IO");
}

#[test]
fn refusal_e_io_nonexistent_registry_directory() {
    let payload = run_json_refusal(&[
        &fixture("csv/all_resolved.csv"),
        "--registry",
        &fixture("registries/missing"),
        "--column",
        "cusip",
    ]);
    assert_refusal_envelope(&payload, "E_IO");
}

#[test]
fn refusal_e_column_not_found_has_available_columns() {
    let payload = run_json_refusal(&[
        &fixture("csv/wrong_column.csv"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
    ]);
    assert_refusal_envelope(&payload, "E_COLUMN_NOT_FOUND");
    assert!(payload["refusal"]["detail"]["available_columns"].is_array());
}

#[test]
fn refusal_e_column_not_found_suggests_an_existing_column() {
    let payload = run_json_refusal(&[
        &fixture("csv/wrong_column.csv"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
    ]);
    let available = payload["refusal"]["detail"]["available_columns"]
        .as_array()
        .expect("available_columns is an array");
    let suggested = available[0].as_str().expect("first available column");
    let next_command = payload["refusal"]["next_command"]
        .as_str()
        .expect("next_command is a string");
    // The recovery path points the agent at a column that actually exists.
    assert!(
        next_command.contains(&format!("--column {suggested}")),
        "next_command should suggest the existing column '{suggested}', got: {next_command}"
    );
}

#[test]
fn refusal_e_too_large_recovery_names_the_real_dashed_flag() {
    let payload = run_json_refusal(&[
        &fixture("csv/all_resolved.csv"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
        "--max-rows",
        "1",
    ]);
    assert_refusal_envelope(&payload, "E_TOO_LARGE");
    // detail keeps the canonical underscore identifier...
    assert_eq!(payload["refusal"]["detail"]["limit_type"], "max_rows");
    // ...but the human-facing message and hint name the real CLI flag.
    let message = payload["refusal"]["message"].as_str().unwrap();
    let next_command = payload["refusal"]["next_command"].as_str().unwrap();
    assert!(message.contains("--max-rows"), "message: {message}");
    assert!(!message.contains("--max_rows"), "message: {message}");
    assert!(
        next_command.contains("--max-rows") && !next_command.contains("--max_rows"),
        "next_command: {next_command}"
    );
}

#[test]
fn refusal_e_bad_registry_malformed_mapping_file() {
    let payload = run_json_refusal(&[
        &fixture("csv/all_resolved.csv"),
        "--registry",
        &fixture("registries/malformed"),
        "--column",
        "cusip",
    ]);
    assert_refusal_envelope(&payload, "E_BAD_REGISTRY");
}

#[test]
fn refusal_e_parse_no_extension() {
    let payload = run_json_refusal(&[
        &fixture("csv/no_extension"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
    ]);
    assert_refusal_envelope(&payload, "E_PARSE");
}

#[test]
fn refusal_e_empty_input_header_only_csv() {
    let payload = run_json_refusal(&[
        &fixture("csv/header_only.csv"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
    ]);
    assert_refusal_envelope(&payload, "E_EMPTY_INPUT");
}

#[test]
fn refusal_e_too_large_max_rows() {
    let payload = run_json_refusal(&[
        &fixture("csv/all_resolved.csv"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
        "--max-rows",
        "1",
    ]);
    assert_refusal_envelope(&payload, "E_TOO_LARGE");
}

#[test]
fn refusal_e_too_large_max_bytes() {
    let payload = run_json_refusal(&[
        &fixture("csv/all_resolved.csv"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
        "--max-bytes",
        "10",
    ]);
    assert_refusal_envelope(&payload, "E_TOO_LARGE");
}

#[test]
fn refusal_e_emit_format_csv_with_jsonl_input() {
    let payload = run_csv_refusal(&[
        &fixture("jsonl/basic.jsonl"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
        "--emit",
        "csv",
    ]);
    assert_refusal_envelope(&payload, "E_EMIT_FORMAT");
    assert!(payload["refusal"]["next_command"].is_string());
}

#[test]
fn refusal_e_column_exists_duplicate_canon_column() {
    let payload = run_csv_refusal(&[
        &fixture("csv/has_canon_column.csv"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
        "--emit",
        "csv",
    ]);
    assert_refusal_envelope(&payload, "E_COLUMN_EXISTS");
}
