use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::NamedTempFile;

fn fixture_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(rel)
}

fn registry_path() -> PathBuf {
    fixture_path("registries/cusip-isin")
}

fn canon_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_canon"))
}

fn run_canon(args: &[&str]) -> Output {
    Command::new(canon_bin())
        .args(args)
        .output()
        .expect("failed to execute canon binary")
}

fn stdout_string(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout should be UTF-8")
}

fn stderr_string(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr should be UTF-8")
}

fn refusal_from_stderr(stderr: &str) -> Value {
    let json_line = stderr
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with('{'))
        .expect("expected refusal JSON on stderr");
    serde_json::from_str(json_line).expect("refusal stderr line should be valid JSON")
}

fn strip_id_prefix(value: &str) -> &str {
    value
        .strip_prefix("u8:")
        .or_else(|| value.strip_prefix("hex:"))
        .unwrap_or(value)
}

#[test]
fn emit_csv_partial_appends_canon_column_and_preserves_rows() {
    let input = fixture_path("inputs/partial.csv");
    let registry = registry_path();
    let output = run_canon(&[
        input.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
        "--column",
        "cusip",
        "--emit",
        "csv",
    ]);

    assert_eq!(output.status.code(), Some(1));

    let stdout = stdout_string(&output);
    assert_eq!(
        stdout,
        "cusip,amount,cusip__canon\n\
037833100,100,US0378331005\n\
594918104,200,US5949181045\n\
UNKNOWN99,300,\n"
    );
    assert!(!stdout.contains("u8:"));
}

#[test]
fn emit_csv_supports_custom_column_and_preserves_delimiter() {
    let registry = registry_path();
    let resolved_input = fixture_path("inputs/all_resolved.csv");
    let resolved_output = run_canon(&[
        resolved_input.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
        "--column",
        "cusip",
        "--emit",
        "csv",
        "--canon-column",
        "canonical_id",
    ]);
    assert_eq!(resolved_output.status.code(), Some(0));
    let resolved_stdout = stdout_string(&resolved_output);
    assert!(resolved_stdout.starts_with("cusip,amount,canonical_id\n"));

    let tab_input = fixture_path("inputs/tab_delimited.tsv");
    let tab_output = run_canon(&[
        tab_input.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
        "--column",
        "cusip",
        "--emit",
        "csv",
    ]);
    assert_eq!(tab_output.status.code(), Some(1));
    let tab_stdout = stdout_string(&tab_output);
    let first_line = tab_stdout.lines().next().unwrap();
    assert_eq!(first_line, "cusip\tamount\tcusip__canon");
}

#[test]
fn emit_csv_preserves_blank_rows() {
    let input = fixture_path("inputs/blank_rows.csv");
    let registry = registry_path();
    let output = run_canon(&[
        input.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
        "--column",
        "cusip",
        "--emit",
        "csv",
    ]);

    assert_eq!(output.status.code(), Some(0));

    let stdout = stdout_string(&output);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 6);
    assert_eq!(lines[0], "cusip,amount,cusip__canon");
    assert_eq!(lines[1], "037833100,100,US0378331005");
    assert_eq!(lines[2], ",,");
    assert_eq!(lines[3], "594918104,200,US5949181045");
    assert_eq!(lines[4], "  ,  ,");
    assert_eq!(lines[5], "17275R102,300,US17275R1023");
}

#[test]
fn emit_csv_map_out_matches_emit_json_output() {
    let input = fixture_path("inputs/partial.csv");
    let registry = registry_path();
    let map_file = NamedTempFile::new().unwrap();

    let csv_output = run_canon(&[
        input.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
        "--column",
        "cusip",
        "--emit",
        "csv",
        "--map-out",
        map_file.path().to_str().unwrap(),
    ]);
    assert_eq!(csv_output.status.code(), Some(1));
    assert!(stdout_string(&csv_output).contains("UNKNOWN99,300,"));

    let map_json: Value =
        serde_json::from_str(&fs::read_to_string(map_file.path()).unwrap()).unwrap();

    let json_output = run_canon(&[
        input.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
        "--column",
        "cusip",
        "--emit",
        "json",
    ]);
    assert_eq!(json_output.status.code(), Some(1));
    let json_stdout: Value = serde_json::from_str(&stdout_string(&json_output)).unwrap();

    assert_eq!(map_json, json_stdout);
}

#[test]
fn emit_csv_with_jsonl_input_refuses_to_stderr() {
    let input = fixture_path("inputs/basic.jsonl");
    let registry = registry_path();
    let output = run_canon(&[
        input.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
        "--column",
        "cusip",
        "--emit",
        "csv",
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());

    let refusal = refusal_from_stderr(&stderr_string(&output));
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_EMIT_FORMAT");
}

#[test]
fn emit_csv_with_existing_canon_column_refuses() {
    let input = fixture_path("inputs/has_canon_column.csv");
    let registry = registry_path();
    let output = run_canon(&[
        input.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
        "--column",
        "cusip",
        "--emit",
        "csv",
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());

    let refusal = refusal_from_stderr(&stderr_string(&output));
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_COLUMN_EXISTS");
}

#[test]
fn csv_canonical_values_match_json_mappings_without_prefix() {
    let input = fixture_path("inputs/partial.csv");
    let registry = registry_path();
    let map_file = NamedTempFile::new().unwrap();

    let csv_output = run_canon(&[
        input.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
        "--column",
        "cusip",
        "--emit",
        "csv",
        "--map-out",
        map_file.path().to_str().unwrap(),
    ]);
    assert_eq!(csv_output.status.code(), Some(1));

    let csv_stdout = stdout_string(&csv_output);
    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(csv_stdout.as_bytes());
    let mut csv_map = HashMap::new();
    for record in csv_reader.records() {
        let record = record.unwrap();
        let input_value = record.get(0).unwrap().to_string();
        let canon_value = record.get(2).unwrap().to_string();
        if !canon_value.is_empty() {
            csv_map.insert(input_value, canon_value);
        }
    }

    let map_json: Value =
        serde_json::from_str(&fs::read_to_string(map_file.path()).unwrap()).unwrap();
    let mappings = map_json["mappings"].as_array().unwrap();
    for mapping in mappings {
        let input_value = strip_id_prefix(mapping["input"].as_str().unwrap()).to_string();
        let canonical_id = strip_id_prefix(mapping["canonical_id"].as_str().unwrap()).to_string();
        assert_eq!(csv_map.get(&input_value), Some(&canonical_id));
    }
}
