//! Intent inference for legible-but-wrong invocations: flag typos get a
//! did-you-mean naming the exact corrected flag, and a misspelled subcommand
//! that clap would otherwise swallow as the positional input is disambiguated.
//! Real input files are never hijacked.

use assert_cmd::Command;
use serde_json::Value;

fn fixture(path: &str) -> String {
    format!("tests/fixtures/{path}")
}

#[test]
fn flag_typo_suggests_the_exact_flag() {
    let assert = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            &fixture("inputs/partial.csv"),
            "--regisry",
            &fixture("registries/cusip-isin"),
            "--column",
            "cusip",
        ])
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("--regisry"));
    assert!(stderr.contains("did you mean '--registry'"));
}

#[test]
fn unknown_flag_without_near_match_defers_to_clap() {
    // No close known flag => clap's standard error, not a misleading suggestion.
    let assert = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            &fixture("inputs/partial.csv"),
            "--registry",
            &fixture("registries/cusip-isin"),
            "--column",
            "cusip",
            "--zzzzzz",
        ])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("--zzzzzz"));
    assert!(!stderr.contains("did you mean"));
}

#[test]
fn misspelled_subcommand_is_disambiguated() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "regstry",
            "--registry",
            &fixture("registries/cusip-isin"),
            "--column",
            "cusip",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["outcome"], "REFUSAL");
    assert_eq!(
        value["refusal"]["detail"]["suggested_subcommand"],
        "registry"
    );
    assert_eq!(value["refusal"]["next_command"], "canon registry --help");
}

#[test]
fn real_missing_input_file_is_not_hijacked() {
    // A genuine (missing) data file is far from any subcommand: plain E_IO,
    // no spurious subcommand suggestion.
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "positions_2026.csv",
            "--registry",
            &fixture("registries/cusip-isin"),
            "--column",
            "cusip",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["refusal"]["code"], "E_IO");
    assert!(
        value["refusal"]["detail"]
            .get("suggested_subcommand")
            .is_none()
    );
}
