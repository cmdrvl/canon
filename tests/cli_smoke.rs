use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn fixture_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

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
fn info_flags_short_circuit_before_invalid_args_are_parsed() {
    let version = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["--version", "--emit", "bogus"])
        .assert()
        .success();
    let version_stdout = String::from_utf8(version.get_output().stdout.clone()).unwrap();
    assert_eq!(
        version_stdout.trim(),
        format!("canon {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(version.get_output().stderr.is_empty());

    let describe = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["--describe", "--column"])
        .assert()
        .success();
    let describe_stdout = String::from_utf8(describe.get_output().stdout.clone()).unwrap();
    let describe_json: Value =
        serde_json::from_str(&describe_stdout).expect("--describe should output valid JSON");
    assert_eq!(describe_json["name"], "canon");
    assert!(describe.get_output().stderr.is_empty());

    let schema = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["--schema", "--max-rows", "nope"])
        .assert()
        .success();
    let schema_stdout = String::from_utf8(schema.get_output().stdout.clone()).unwrap();
    let schema_json: Value =
        serde_json::from_str(&schema_stdout).expect("--schema should output valid JSON");
    assert_eq!(schema_json["$id"], "https://canon.v0/schema.json");
    assert!(schema.get_output().stderr.is_empty());
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
fn test_witness_uses_epistemic_witness_env_path() {
    let ledger_dir = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let ledger_path = ledger_dir.path().join("nested").join("witness.jsonl");

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .current_dir(cwd.path())
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .arg(fixture_path("tests/fixtures/inputs/all_resolved.csv"))
        .arg("--registry")
        .arg(fixture_path("tests/fixtures/registries/cusip-isin"))
        .arg("--column")
        .arg("cusip")
        .assert()
        .success();

    assert!(ledger_path.exists());
    assert!(!cwd.path().join(".canon-witness.jsonl").exists());

    let content = std::fs::read_to_string(&ledger_path).unwrap();
    let record: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(record["tool"], "canon");
    assert!(record["id"].as_str().unwrap().starts_with("blake3:"));
    assert!(
        record["binary_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
    assert_eq!(
        record["inputs"][0]["path"],
        fixture_path("tests/fixtures/inputs/all_resolved.csv")
            .display()
            .to_string()
    );
    assert!(
        record["inputs"][0]["hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
    assert_eq!(record["params"]["registry_id"], "cusip-isin");
    assert_eq!(record["params"]["registry_version"], "1.0.0");
    assert_eq!(record["params"]["emit"], "json");
    assert_eq!(record["outcome"], "RESOLVED");
    assert_eq!(record["exit_code"], 0);
}

#[test]
fn test_witness_defaults_to_home_epistemic_path() {
    let home = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let ledger_path = home.path().join(".epistemic").join("witness.jsonl");

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .current_dir(cwd.path())
        .env_remove("EPISTEMIC_WITNESS")
        .env("HOME", home.path())
        .arg(fixture_path("tests/fixtures/inputs/all_resolved.csv"))
        .arg("--registry")
        .arg(fixture_path("tests/fixtures/registries/cusip-isin"))
        .arg("--column")
        .arg("cusip")
        .assert()
        .success();

    assert!(ledger_path.exists());
    assert!(!cwd.path().join(".canon-witness.jsonl").exists());
}

#[test]
fn test_witness_hash_parity_and_chain_linkage() {
    let ledger_dir = tempdir().unwrap();
    let ledger_path = ledger_dir.path().join("witness.jsonl");
    let input_path = fixture_path("tests/fixtures/inputs/all_resolved.csv");
    let registry_path = fixture_path("tests/fixtures/registries/cusip-isin");

    let json_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .arg(&input_path)
        .arg("--registry")
        .arg(&registry_path)
        .arg("--column")
        .arg("cusip")
        .assert()
        .success();
    let json_stdout = json_output.get_output().stdout.clone();

    let csv_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .arg(&input_path)
        .arg("--registry")
        .arg(&registry_path)
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("csv")
        .assert()
        .success();
    let csv_stdout = csv_output.get_output().stdout.clone();

    let content = std::fs::read_to_string(&ledger_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);

    let first: Value = serde_json::from_str(lines[0]).unwrap();
    let second: Value = serde_json::from_str(lines[1]).unwrap();

    let expected_json_hash = format!("blake3:{}", blake3::hash(&json_stdout).to_hex());
    let expected_csv_hash = format!("blake3:{}", blake3::hash(&csv_stdout).to_hex());

    assert_eq!(first["output_hash"], expected_json_hash);
    assert_eq!(second["output_hash"], expected_csv_hash);
    assert_ne!(second["id"], first["id"]);
    assert_eq!(first["params"]["emit"], "json");
    assert_eq!(second["params"]["emit"], "csv");
}

#[test]
fn test_witness_hashes_stdin_bytes_without_dash_file() {
    let ledger_dir = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let ledger_path = ledger_dir.path().join("witness.jsonl");
    let stdin_data =
        std::fs::read_to_string(fixture_path("tests/fixtures/inputs/basic.jsonl")).unwrap();
    let registry_path = fixture_path("tests/fixtures/registries/cusip-isin");

    assert!(!cwd.path().join("-").exists());

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .current_dir(cwd.path())
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .arg("-")
        .arg("--registry")
        .arg(&registry_path)
        .arg("--column")
        .arg("cusip")
        .write_stdin(stdin_data.clone())
        .assert()
        .success();

    let content = std::fs::read_to_string(&ledger_path).unwrap();
    let record: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    let expected_hash = format!("blake3:{}", blake3::hash(stdin_data.as_bytes()).to_hex());

    assert_eq!(record["inputs"][0]["path"], "-");
    assert_eq!(record["inputs"][0]["hash"], expected_hash);
    assert_eq!(record["inputs"][0]["bytes"], stdin_data.len() as u64);
    assert_eq!(record["outcome"], "RESOLVED");
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
