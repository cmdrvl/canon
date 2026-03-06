use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::path::Path;

#[test]
fn test_version_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--version")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert_eq!(
        stdout.trim(),
        format!("canon {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn test_describe_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--describe")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&stdout).expect("--describe should output valid JSON");

    assert_eq!(json["name"], "canon");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(json["schema_version"], "operator.v0");
    assert!(json["capabilities"].is_object());
}

#[test]
fn test_schema_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--schema")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&stdout).expect("--schema should output valid JSON");

    assert_eq!(
        json["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(json["$id"], "https://canon.v0/schema.json");
    assert!(json["properties"].is_object());
}

#[test]
fn test_all_resolved_exit_code() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .assert()
        .code(0); // RESOLVED
}

#[test]
fn test_partial_exit_code() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/partial.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .assert()
        .code(1); // PARTIAL
}

#[test]
fn test_all_unresolved_exit_code() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_unresolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .assert()
        .code(1); // UNRESOLVED
}

#[test]
fn test_missing_input_file_refusal() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("nonexistent.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .assert()
        .code(2) // REFUSAL
        .stdout(predicate::str::contains("REFUSAL"));
}

#[test]
fn test_emit_csv_with_jsonl_refusal() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/basic.jsonl")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("csv")
        .assert()
        .code(2) // REFUSAL
        .stderr(predicate::str::contains("E_EMIT_FORMAT"));
}

#[test]
fn test_column_not_found_refusal() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("nonexistent_column")
        .assert()
        .code(2) // REFUSAL
        .stdout(predicate::str::contains("E_COLUMN_NOT_FOUND"));
}

#[test]
fn test_json_mode_success_to_stdout() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("json")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&stdout).expect("JSON mode should output valid JSON");

    assert_eq!(json["version"], "canon.v0");
    assert_eq!(json["outcome"], "RESOLVED");
    assert!(json["registry"].is_object());
    assert!(json["summary"].is_object());
}

#[test]
fn test_csv_mode_success_to_stdout() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("csv")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    // Should be CSV format with canonical column
    assert!(stdout.contains("cusip__canon"));
    // Should not be JSON
    assert!(!stdout.starts_with('{'));
}

#[test]
fn test_csv_mode_refusal_to_stderr() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/wrong_column.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("nonexistent")
        .arg("--emit")
        .arg("csv")
        .assert()
        .code(2);

    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    // Refusal should go to stderr in CSV mode
    assert!(stderr.contains("E_COLUMN_NOT_FOUND"));
    // No CSV output on stdout
    assert!(stdout.is_empty());
}

#[test]
fn test_witness_flag_no_witness() {
    // This test just ensures --no-witness doesn't break execution
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--no-witness")
        .assert()
        .success();
}

#[test]
fn test_map_out_sidecar_in_csv_mode() {
    use tempfile::NamedTempFile;

    let temp_file = NamedTempFile::new().unwrap();
    let map_out_path = temp_file.path().to_str().unwrap();

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("csv")
        .arg("--map-out")
        .arg(map_out_path)
        .assert()
        .success();

    // Check that sidecar JSON was written
    assert!(Path::new(map_out_path).exists());
    let sidecar_content = std::fs::read_to_string(map_out_path).unwrap();
    let json: Value = serde_json::from_str(&sidecar_content).expect("Sidecar should be valid JSON");

    assert_eq!(json["version"], "canon.v0");
    assert_eq!(json["outcome"], "RESOLVED");
}

#[cfg(unix)]
#[test]
fn test_non_utf8_input_path_does_not_panic() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let input_path = temp_dir
        .path()
        .join(OsString::from_vec(b"input-\xFF.csv".to_vec()));
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg(&input_path)
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .assert()
        .code(2)
        .stdout(predicate::str::contains("E_IO"));
}
