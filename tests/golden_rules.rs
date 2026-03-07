/// Golden Rules enforcement tests.
///
/// These tests enforce spine protocol invariants from GOLDEN_RULES.md.
/// Each test name references a rule ID (R-001, R-007, etc.) so violations
/// are traceable to the specification.
///
/// Portable: swap CARGO_BIN_EXE and fixture paths for any spine tool.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(path: &str) -> String {
    PathBuf::from("tests/fixtures")
        .join(path)
        .to_string_lossy()
        .to_string()
}

fn fixture_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

// ---------------------------------------------------------------------------
// R-001: Version field required in every JSON output
// ---------------------------------------------------------------------------

#[test]
fn r001_resolved_output_has_version_field() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            &fixture("inputs/all_resolved.csv"),
            "--registry",
            &fixture("registries/cusip-isin"),
            "--column",
            "cusip",
            "--no-witness",
        ])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert!(
        json.get("version").is_some(),
        "R-001 VIOLATION: Missing `version` field in RESOLVED output"
    );
    assert!(
        json["version"].as_str().unwrap().starts_with("canon."),
        "R-001 VIOLATION: version field must start with tool schema prefix"
    );
}

#[test]
fn r001_refusal_output_has_version_field() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "nonexistent.csv",
            "--registry",
            &fixture("registries/cusip-isin"),
            "--column",
            "cusip",
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let json: Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert!(
        json.get("version").is_some(),
        "R-001 VIOLATION: Missing `version` field in REFUSAL output"
    );
}

// ---------------------------------------------------------------------------
// R-007: operator.json refusal codes match source code
// ---------------------------------------------------------------------------

#[test]
fn r007_operator_json_refusal_codes_match_source() {
    let manifest: Value =
        serde_json::from_str(include_str!("../operator.json")).expect("operator.json valid JSON");

    let declared_codes: Vec<String> = manifest["refusals"]
        .as_array()
        .expect("operator.json must have refusals array")
        .iter()
        .map(|entry| entry["code"].as_str().unwrap().to_string())
        .collect();

    // Read the RefusalCode enum from source to get all codes the binary can emit
    let lib_source =
        std::fs::read_to_string(fixture_path("src/lib.rs")).expect("src/lib.rs readable");

    let refusal_source =
        std::fs::read_to_string(fixture_path("src/refusal.rs")).expect("src/refusal.rs readable");

    let combined_source = format!("{}\n{}", lib_source, refusal_source);

    // Every code declared in operator.json must appear somewhere in source
    for code in &declared_codes {
        assert!(
            combined_source.contains(code),
            "R-007 VIOLATION: operator.json declares '{}' but it never appears in source",
            code
        );
    }

    // Every E_ code that appears as a string literal in source should be in operator.json
    let source_codes: Vec<String> = combined_source
        .lines()
        .filter_map(|line| {
            // Match patterns like "E_IO", "E_BAD_REGISTRY", etc.
            let trimmed = line.trim();
            if trimmed.contains("\"E_") {
                // Extract E_ codes from string literals
                let mut codes = vec![];
                let mut remaining = trimmed;
                while let Some(start) = remaining.find("\"E_") {
                    let after = &remaining[start + 1..];
                    if let Some(end) = after.find('"') {
                        let code = &after[..end];
                        if code.starts_with("E_")
                            && code.chars().all(|c| c.is_ascii_uppercase() || c == '_')
                        {
                            codes.push(code.to_string());
                        }
                    }
                    remaining = &remaining[start + 1..];
                }
                if codes.is_empty() { None } else { Some(codes) }
            } else {
                None
            }
        })
        .flatten()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    for code in &source_codes {
        assert!(
            declared_codes.contains(code),
            "R-007 VIOLATION: Source emits '{}' but operator.json does not declare it",
            code
        );
    }
}

#[test]
fn r007_operator_json_exit_codes_match_trinity() {
    let manifest: Value =
        serde_json::from_str(include_str!("../operator.json")).expect("operator.json valid JSON");

    let exit_codes = manifest["exit_codes"]
        .as_object()
        .expect("operator.json must have exit_codes object");

    assert!(
        exit_codes.contains_key("0"),
        "R-002/R-007: operator.json must declare exit code 0"
    );
    assert!(
        exit_codes.contains_key("1"),
        "R-002/R-007: operator.json must declare exit code 1"
    );
    assert!(
        exit_codes.contains_key("2"),
        "R-002/R-007: operator.json must declare exit code 2"
    );

    // No exit codes outside 0/1/2
    for key in exit_codes.keys() {
        assert!(
            ["0", "1", "2"].contains(&key.as_str()),
            "R-002 VIOLATION: operator.json declares exit code {} — only 0, 1, 2 are permitted",
            key
        );
    }
}

// ---------------------------------------------------------------------------
// R-006: Three introspection flags
// ---------------------------------------------------------------------------

#[test]
fn r006_describe_emits_valid_operator_manifest() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--describe")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "R-006: --describe must exit 0"
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("R-006: --describe must emit valid JSON");
    assert!(
        json.get("name").is_some(),
        "R-006: --describe output must include tool name"
    );
    assert!(
        json.get("refusals").is_some() || json.get("refusal_codes").is_some(),
        "R-006: --describe output must include refusal documentation"
    );
}

#[test]
fn r006_schema_emits_valid_json_schema() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--schema")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "R-006: --schema must exit 0");
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("R-006: --schema must emit valid JSON");
    assert!(
        json.get("$schema").is_some() || json.get("type").is_some(),
        "R-006: --schema output must be a JSON Schema document"
    );
    assert!(
        json.get("properties").is_some(),
        "R-006: --schema must describe output properties"
    );
}

// ---------------------------------------------------------------------------
// R-021: CSV mode preserves every input row
// ---------------------------------------------------------------------------

#[test]
fn r021_csv_mode_preserves_row_count() {
    let input = fixture("inputs/partial.csv");
    let input_content = std::fs::read_to_string(&input).unwrap();
    let input_lines: Vec<&str> = input_content.lines().collect();

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            &input,
            "--registry",
            &fixture("registries/cusip-isin"),
            "--column",
            "cusip",
            "--emit",
            "csv",
            "--no-witness",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let output_lines: Vec<&str> = stdout.lines().collect();

    assert_eq!(
        input_lines.len(),
        output_lines.len(),
        "R-021 VIOLATION: CSV input has {} lines but output has {} lines. \
         Row count in must equal row count out.",
        input_lines.len(),
        output_lines.len()
    );
}

#[test]
fn r021_csv_mode_preserves_row_count_with_all_resolved() {
    let input = fixture("inputs/all_resolved.csv");
    let input_content = std::fs::read_to_string(&input).unwrap();
    let input_lines: Vec<&str> = input_content.lines().collect();

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            &input,
            "--registry",
            &fixture("registries/cusip-isin"),
            "--column",
            "cusip",
            "--emit",
            "csv",
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let output_lines: Vec<&str> = stdout.lines().collect();

    assert_eq!(
        input_lines.len(),
        output_lines.len(),
        "R-021 VIOLATION: CSV input has {} lines but output has {} lines",
        input_lines.len(),
        output_lines.len()
    );
}

// ---------------------------------------------------------------------------
// R-008: --no-witness opt-out
// ---------------------------------------------------------------------------

#[test]
fn r008_no_witness_flag_suppresses_ledger_write() {
    let temp = tempfile::tempdir().unwrap();
    let ledger_path = temp.path().join("witness.jsonl");

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .args([
            &fixture("inputs/all_resolved.csv"),
            "--registry",
            &fixture("registries/cusip-isin"),
            "--column",
            "cusip",
            "--no-witness",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        !ledger_path.exists(),
        "R-008 VIOLATION: --no-witness specified but witness file was created"
    );
}

#[test]
fn r008_witness_failure_does_not_change_exit_code() {
    // Point witness to an unwritable location
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", "/dev/null/impossible/witness.jsonl")
        .args([
            &fixture("inputs/all_resolved.csv"),
            "--registry",
            &fixture("registries/cusip-isin"),
            "--column",
            "cusip",
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "R-017 VIOLATION: Witness write failure changed RESOLVED (exit 0) to exit {}",
        output.status.code().unwrap_or(-1)
    );
}
