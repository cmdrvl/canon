//! The `redacted` discovery breadcrumb on canon.v0 mapping artifacts.
//!
//! Values are masked by default (zero-retention posture); the envelope's
//! `redacted` field lets an agent detect the masking and learn that `--explicit`
//! reveals the values, without reading --help.

use assert_cmd::Command;
use serde_json::Value;

fn fixture(path: &str) -> String {
    format!("tests/fixtures/{path}")
}

fn run(args: &[&str]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .output()
        .unwrap();
    let code = output.status.code().unwrap();
    let value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    (code, value)
}

#[test]
fn default_output_is_redacted_and_flags_it() {
    let (code, value) = run(&[
        &fixture("inputs/partial.csv"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
        "--no-witness",
    ]);
    assert_eq!(code, 1); // PARTIAL
    assert_eq!(value["redacted"], true);
    // The breadcrumb is honest: values really are masked.
    assert_eq!(value["mappings"][0]["input"], "[REDACTED]");
    assert_eq!(value["mappings"][0]["canonical_id"], "[REDACTED]");
}

#[test]
fn explicit_reveals_values_and_clears_the_flag() {
    let (code, value) = run(&[
        &fixture("inputs/partial.csv"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
        "--explicit",
        "--no-witness",
    ]);
    assert_eq!(code, 1);
    assert_eq!(value["redacted"], false);
    assert!(
        value["mappings"][0]["input"]
            .as_str()
            .unwrap()
            .starts_with("u8:")
    );
}

#[test]
fn explicit_plain_json_values_emit_utf8_without_prefix_and_metadata() {
    let (code, value) = run(&[
        &fixture("inputs/partial.csv"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "cusip",
        "--explicit",
        "--plain-json-values",
        "--no-witness",
    ]);
    assert_eq!(code, 1);
    assert_eq!(value["redacted"], false);
    assert_eq!(value["mappings"][0]["input"], "037833100");
    assert_eq!(value["mappings"][0]["input_encoding"], "utf8");
    assert_eq!(value["mappings"][0]["canonical_id"], "US0378331005");
    assert_eq!(value["mappings"][0]["canonical_id_encoding"], "utf8");
}

#[test]
fn refusal_outputs_omit_the_redacted_flag() {
    // Refusals carry no values, so the breadcrumb does not apply.
    let (code, value) = run(&[
        &fixture("inputs/partial.csv"),
        "--registry",
        &fixture("registries/cusip-isin"),
        "--column",
        "does_not_exist",
        "--no-witness",
    ]);
    assert_eq!(code, 2);
    assert_eq!(value["outcome"], "REFUSAL");
    assert!(value.get("redacted").is_none());
}
