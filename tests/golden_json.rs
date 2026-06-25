use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

fn fixture(path: &str) -> String {
    PathBuf::from("tests/fixtures")
        .join(path)
        .to_string_lossy()
        .to_string()
}

fn run_json(input: &str, registry: &str, column: &str) -> (i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg(input)
        .args(["--registry", registry, "--column", column, "--explicit"])
        .output()
        .unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8(output.stdout).unwrap(),
    )
}

fn run_json_plain_values(input: &str, registry: &str, column: &str) -> (i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg(input)
        .args([
            "--registry",
            registry,
            "--column",
            column,
            "--explicit",
            "--plain-json-values",
        ])
        .output()
        .unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8(output.stdout).unwrap(),
    )
}

fn canonical_json(raw: &str) -> String {
    serde_json::to_string(&serde_json::from_str::<Value>(raw).unwrap()).unwrap()
}

fn assert_matches_golden(
    input: &str,
    registry: &str,
    column: &str,
    golden_file: &str,
    exit_code: i32,
) {
    let (status, actual) = run_json(input, registry, column);
    assert_eq!(status, exit_code);
    let expected = fs::read_to_string(golden_file).unwrap();
    assert_eq!(canonical_json(&actual), canonical_json(&expected));
}

#[test]
fn all_resolved_matches_golden() {
    assert_matches_golden(
        &fixture("csv/all_resolved.csv"),
        &fixture("registries/cusip-isin"),
        "cusip",
        &fixture("golden/all_resolved.json"),
        0,
    );
}

#[test]
fn partial_matches_golden() {
    assert_matches_golden(
        &fixture("inputs/partial.csv"),
        &fixture("registries/cusip-isin"),
        "cusip",
        &fixture("golden/partial.json"),
        1,
    );
}

#[test]
fn all_unresolved_matches_golden() {
    assert_matches_golden(
        &fixture("inputs/all_unresolved.csv"),
        &fixture("registries/cusip-isin"),
        "cusip",
        &fixture("golden/all_unresolved.json"),
        1,
    );
}

#[test]
fn empty_registry_matches_golden() {
    assert_matches_golden(
        &fixture("inputs/all_resolved.csv"),
        &fixture("registries/empty"),
        "cusip",
        &fixture("golden/empty_registry.json"),
        1,
    );
}

#[test]
fn json_output_is_deterministic_for_same_input() {
    let input = fixture("inputs/partial.csv");
    let registry = fixture("registries/cusip-isin");
    let (first_status, first_output) = run_json(&input, &registry, "cusip");
    let (second_status, second_output) = run_json(&input, &registry, "cusip");

    assert_eq!(first_status, 1);
    assert_eq!(second_status, 1);
    assert_eq!(first_output, second_output);
}

#[test]
fn registry_version_and_identifier_encoding_present() {
    let (status, output) = run_json(
        &fixture("csv/partial.csv"),
        &fixture("registries/cusip-isin"),
        "cusip",
    );
    assert_eq!(status, 1);
    let payload: Value = serde_json::from_str(&output).unwrap();

    assert_eq!(payload["registry"]["version"], "1.0.0");
    for mapping in payload["mappings"].as_array().unwrap() {
        assert!(mapping["input"].as_str().unwrap().starts_with("u8:"));
        assert!(mapping["canonical_id"].as_str().unwrap().starts_with("u8:"));
    }
    for unresolved in payload["unresolved"].as_array().unwrap() {
        if !unresolved["input"].is_null() {
            assert!(unresolved["input"].as_str().unwrap().starts_with("u8:"));
        }
    }
}

#[test]
fn plain_json_values_are_opt_in_and_carry_encoding_metadata() {
    let (status, output) = run_json_plain_values(
        &fixture("csv/partial.csv"),
        &fixture("registries/cusip-isin"),
        "cusip",
    );
    assert_eq!(status, 1);
    let payload: Value = serde_json::from_str(&output).unwrap();

    for mapping in payload["mappings"].as_array().unwrap() {
        assert!(!mapping["input"].as_str().unwrap().starts_with("u8:"));
        assert_eq!(mapping["input_encoding"], "utf8");
        assert!(!mapping["canonical_id"].as_str().unwrap().starts_with("u8:"));
        assert_eq!(mapping["canonical_id_encoding"], "utf8");
    }
    for unresolved in payload["unresolved"].as_array().unwrap() {
        if !unresolved["input"].is_null() {
            assert!(!unresolved["input"].as_str().unwrap().starts_with("u8:"));
            assert_eq!(unresolved["input_encoding"], "utf8");
        }
    }
}

#[test]
fn summary_invariant_holds() {
    let (status, output) = run_json(
        &fixture("inputs/partial.csv"),
        &fixture("registries/cusip-isin"),
        "cusip",
    );
    assert_eq!(status, 1);
    let payload: Value = serde_json::from_str(&output).unwrap();

    let total = payload["summary"]["total"].as_u64().unwrap();
    let resolved = payload["summary"]["resolved"].as_u64().unwrap();
    let unresolved = payload["summary"]["unresolved"].as_u64().unwrap();
    assert_eq!(total, resolved + unresolved);
}

#[test]
fn alias_variants_resolve_to_same_canonical_id() {
    let temp = tempdir().unwrap();
    let input_path = temp.path().join("aliases.csv");
    fs::write(
        &input_path,
        "name\nWells Fargo\nWFB\nJPMorgan\nUNKNOWN_ALIAS\n",
    )
    .unwrap();

    let (status, output) = run_json(
        input_path.to_str().unwrap(),
        &fixture("registries/counterparty"),
        "name",
    );
    assert_eq!(status, 1);
    let payload: Value = serde_json::from_str(&output).unwrap();

    let mut canonical_by_input = std::collections::HashMap::new();
    for mapping in payload["mappings"].as_array().unwrap() {
        let input = mapping["input"]
            .as_str()
            .unwrap()
            .strip_prefix("u8:")
            .unwrap()
            .to_string();
        let canonical = mapping["canonical_id"]
            .as_str()
            .unwrap()
            .strip_prefix("u8:")
            .unwrap()
            .to_string();
        canonical_by_input.insert(input, canonical);
    }

    assert_eq!(canonical_by_input["Wells Fargo"], canonical_by_input["WFB"]);
    assert_ne!(
        canonical_by_input["Wells Fargo"],
        canonical_by_input["JPMorgan"]
    );
}
