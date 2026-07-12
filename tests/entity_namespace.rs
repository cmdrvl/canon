#![forbid(unsafe_code)]

use assert_cmd::prelude::*;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::tempdir;

fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn canon_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    command.current_dir(manifest_dir());
    command
}

fn stdout_for(args: &[&str], exit_code: i32) -> Vec<u8> {
    canon_command()
        .args(args)
        .assert()
        .code(exit_code)
        .get_output()
        .stdout
        .clone()
}

fn json_stdout(args: &[&str], exit_code: i32) -> Value {
    serde_json::from_slice(&stdout_for(args, exit_code)).expect("stdout is valid JSON")
}

#[test]
fn entity_namespace_command_surface_has_no_active_org_alias() {
    canon_command()
        .args(["org", "run", "--help"])
        .assert()
        .code(2);

    let describe_stdout = stdout_for(&["--describe"], 0);
    let describe_text = String::from_utf8(describe_stdout.clone()).expect("describe is UTF-8");
    assert!(!describe_text.contains("canon org"));
    assert!(!describe_text.contains("org run"));
    assert!(!describe_text.contains("canon_org"));

    let describe: Value = serde_json::from_slice(&describe_stdout).expect("describe is JSON");
    let subcommands = describe["subcommands"]
        .as_array()
        .expect("describe subcommands");
    assert!(
        subcommands
            .iter()
            .any(|entry| entry["name"].as_str() == Some("entity run"))
    );
    assert!(
        subcommands.iter().all(|entry| !entry["name"]
            .as_str()
            .is_some_and(|name| name == "org" || name.starts_with("org "))),
        "describe must not expose active org workbench commands"
    );
}

#[test]
fn entity_namespace_core_lookup_golden_outputs_remain_stable_for_csv_and_jsonl() {
    let csv_args = [
        "tests/fixtures/inputs/all_resolved.csv",
        "--registry",
        "tests/fixtures/registries/cusip-isin",
        "--column",
        "cusip",
        "--explicit",
    ];
    let csv_first = stdout_for(&csv_args, 0);
    let csv_second = stdout_for(&csv_args, 0);
    assert_eq!(csv_first, csv_second);

    let expected: Value = serde_json::from_str(
        &fs::read_to_string(manifest_dir().join("tests/fixtures/golden/all_resolved.json"))
            .expect("golden fixture"),
    )
    .expect("golden JSON");
    let csv_actual: Value = serde_json::from_slice(&csv_first).expect("CSV lookup JSON");
    assert_eq!(csv_actual, expected);

    let jsonl_actual = json_stdout(
        &[
            "tests/fixtures/inputs/basic.jsonl",
            "--registry",
            "tests/fixtures/registries/cusip-isin",
            "--column",
            "cusip",
            "--explicit",
        ],
        0,
    );
    assert_eq!(jsonl_actual, expected);
}

#[test]
fn entity_namespace_registry_lint_and_build_still_use_exact_registry_lookup() {
    let lint = json_stdout(
        &[
            "registry",
            "lint",
            "tests/fixtures/registries/cusip-isin",
            "--profile",
            "standard",
        ],
        0,
    );
    assert_eq!(lint["version"], "canon_registry_lint.v0");
    assert_eq!(lint["registry"]["id"], "cusip-isin");
    assert_eq!(lint["summary"]["errors"], 0);

    let temp = tempdir().expect("tempdir");
    let seed = temp.path().join("seed.csv");
    let input = temp.path().join("input.csv");
    let output = temp.path().join("registries/mock-cusip");
    fs::write(&seed, "cusip\nAAPL\nMSFT\n").expect("seed fixture");
    fs::write(&input, "cusip\nAAPL\nMSFT\n").expect("input fixture");

    let build = canon_command()
        .args([
            "registry",
            "build",
            "--source",
            "mock",
            "--seed",
            seed.to_str().unwrap(),
            "--seed-column",
            "cusip",
            "--output",
            output.to_str().unwrap(),
            "--version",
            "2026.06.25",
        ])
        .assert()
        .success();
    let build_json: Value = serde_json::from_slice(&build.get_output().stdout).expect("build JSON");
    assert_eq!(build_json["version"], "canon_registry_build.v0");
    assert_eq!(build_json["registry"]["id"], "mock-cusip");
    assert_eq!(build_json["summary"]["resolved_count"], 2);

    let resolve = canon_command()
        .arg(&input)
        .arg("--registry")
        .arg(&output)
        .args(["--column", "cusip", "--explicit"])
        .assert()
        .success();
    let resolve_json: Value =
        serde_json::from_slice(&resolve.get_output().stdout).expect("lookup JSON");
    assert_eq!(resolve_json["outcome"], "RESOLVED");
    assert_eq!(resolve_json["registry"]["id"], "mock-cusip");
    assert_eq!(resolve_json["summary"]["resolved"], 2);
    assert_eq!(resolve_json["mappings"][0]["canonical_id"], "u8:MOCK::AAPL");
    assert_eq!(resolve_json["mappings"][1]["canonical_id"], "u8:MOCK::MSFT");
}

#[test]
fn entity_namespace_rejects_removed_resolve_and_advertises_entity_link() {
    let rejected = canon_command()
        .args([
            "resolve",
            "tests/fixtures/resolve/tapes/reference_loans.csv",
            "tests/fixtures/resolve/tapes/target_loans.csv",
            "--strategy",
            "tests/fixtures/resolve/strategies/minimal.valid.yaml",
            "--registry",
            "tests/fixtures/registries/resolve-servicers",
            "--no-witness",
        ])
        .assert()
        .code(2);
    let stdout = &rejected.get_output().stdout;
    assert!(
        stdout.is_empty(),
        "removed top-level resolve command must not emit a legacy artifact"
    );
    assert!(
        serde_json::from_slice::<Value>(stdout).is_err(),
        "removed top-level resolve command must not emit structured legacy JSON"
    );

    let describe = json_stdout(&["--describe"], 0);
    let subcommands = describe["subcommands"]
        .as_array()
        .expect("describe subcommands");
    assert!(
        subcommands
            .iter()
            .any(|entry| entry["name"].as_str() == Some("entity link")),
        "describe must expose entity link"
    );
    assert!(
        subcommands.iter().all(|entry| !entry["name"]
            .as_str()
            .is_some_and(|name| name == "resolve" || name.starts_with("resolve "))),
        "describe must not expose a top-level resolve command"
    );
}
