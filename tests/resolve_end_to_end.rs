use assert_cmd::prelude::*;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
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

fn run_json(args: &[&str], exit_code: i32) -> Value {
    let assert = canon_command().args(args).assert().code(exit_code);
    serde_json::from_slice(&assert.get_output().stdout).expect("resolve stdout is json")
}

fn full_corpus_args() -> [&'static str; 12] {
    [
        "resolve",
        "tests/fixtures/resolve/tapes/reference_loans.csv",
        "tests/fixtures/resolve/tapes/target_loans.csv",
        "--strategy",
        "tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml",
        "--registry",
        "tests/fixtures/registries/resolve-servicers",
        "--gold",
        "tests/fixtures/resolve/gold/loan_matches.jsonl",
        "--no-witness",
        "--emit",
        "json",
    ]
}

#[test]
fn full_fixture_corpus_resolves_expected_records() {
    let payload = run_json(&full_corpus_args(), 1);

    assert_eq!(payload["version"], "canon_resolve.v0");
    assert_eq!(payload["summary"]["target_records"], 12);
    assert_eq!(payload["summary"]["matched"], 9);
    assert_eq!(payload["summary"]["unmatched"], 2);
    assert_eq!(payload["summary"]["ambiguous"], 1);
    assert_eq!(payload["summary"]["match_rate"], 0.75);
    assert_eq!(payload["gold_score"]["accuracy"], 1.0);
    assert!(
        payload["gold_score"]["regressions"]
            .as_array()
            .expect("gold regressions array")
            .is_empty()
    );

    let actual_pairs = payload["matches"]
        .as_array()
        .expect("matches array")
        .iter()
        .map(|record| {
            (
                record["target_id"].as_str().unwrap().to_string(),
                record["reference_id"].as_str().unwrap().to_string(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let expected_pairs = BTreeMap::from([
        ("WFCM2019-C50|1".to_string(), "223232".to_string()),
        ("WFCM2019-C50|2".to_string(), "223233".to_string()),
        ("WFCM2019-C50|3".to_string(), "223234".to_string()),
        ("WFCM2019-C50|4".to_string(), "223235".to_string()),
        ("WFCM2019-C50|5".to_string(), "223236".to_string()),
        ("WFCM2019-C50|6".to_string(), "223237".to_string()),
        ("WFCM2019-C50|7".to_string(), "223238".to_string()),
        ("WFCM2019-C50|8".to_string(), "223239".to_string()),
        ("WFCM2019-C50|9".to_string(), "223240".to_string()),
    ]);
    assert_eq!(
        actual_pairs, expected_pairs,
        "matched target/reference pairs"
    );

    let unmatched = payload["unmatched"]
        .as_array()
        .expect("unmatched array")
        .iter()
        .map(|record| record["target_id"].as_str().unwrap().to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        unmatched,
        BTreeSet::from([
            "WFCM2019-C50|404".to_string(),
            "WFCM2019-C50|NULLSERV".to_string(),
        ])
    );

    let ambiguous = payload["ambiguous"].as_array().expect("ambiguous array");
    assert_eq!(ambiguous.len(), 1);
    assert_eq!(ambiguous[0]["target_id"], "WFCM2019-C50|AMB");
    let ambiguous_candidates = ambiguous[0]["candidates"]
        .as_array()
        .expect("ambiguous candidates")
        .iter()
        .map(|candidate| candidate["reference_id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(ambiguous_candidates, vec!["223240", "223241"]);
}

#[test]
fn conflict_warnings_are_reported_for_one_to_many_matches() {
    let temp_dir = tempdir().unwrap();
    let reference = temp_dir.path().join("reference.csv");
    let target = temp_dir.path().join("target.csv");
    let strategy = temp_dir.path().join("strategy.yaml");
    let registry = temp_dir.path().join("registry");
    fs::create_dir_all(&registry).unwrap();

    fs::write(
        registry.join("registry.json"),
        r#"{
  "id": "resolve-conflict",
  "version": "0.1.0",
  "description": "empty resolve conflict test registry",
  "updated": "2026-05-06",
  "entry_count": 0
}
"#,
    )
    .unwrap();
    fs::write(&reference, "loan_id,address\nR-1,100 Main St\n").unwrap();
    fs::write(
        &target,
        "deal,loan_number,address\nD,1,100 Main St\nD,2,100 Main St\n",
    )
    .unwrap();
    fs::write(
        &strategy,
        r#"strategy_id: conflict-test.v1
strategy_version: "0.1.0"
entity_type: loan
identity:
  reference:
    id_columns: [loan_id]
  target:
    id_columns: [deal, loan_number]
assertions:
  - field_ref: address
    field_tgt: address
    op: exact
    weight: 1.0
    required: true
match_threshold: 1.0
ambiguity_gap: 0.10
"#,
    )
    .unwrap();

    let payload = run_json(
        &[
            "resolve",
            reference.to_str().unwrap(),
            target.to_str().unwrap(),
            "--strategy",
            strategy.to_str().unwrap(),
            "--registry",
            registry.to_str().unwrap(),
            "--no-witness",
        ],
        0,
    );

    assert_eq!(payload["summary"]["matched"], 2);
    let warnings = payload["conflict_warnings"]
        .as_array()
        .expect("conflict warnings");
    assert_eq!(warnings.len(), 1);
    let warning = warnings[0].as_str().unwrap();
    assert!(warning.contains("one_to_many_conflict"), "{warning}");
    assert!(warning.contains("R-1"), "{warning}");
    assert!(warning.contains("D|1"), "{warning}");
    assert!(warning.contains("D|2"), "{warning}");
}

#[test]
fn writeback_feedback_loop_makes_structural_matches_exactly_lookupable() {
    let temp_dir = tempdir().unwrap();
    let registry = temp_dir.path().join("registry");
    copy_json_registry_fixture("tests/fixtures/registries/resolve-servicers", &registry);

    let payload = run_json(
        &[
            "resolve",
            "tests/fixtures/resolve/tapes/reference_loans.csv",
            "tests/fixtures/resolve/tapes/target_loans.csv",
            "--strategy",
            "tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml",
            "--registry",
            registry.to_str().unwrap(),
            "--gold",
            "tests/fixtures/resolve/gold/loan_matches.jsonl",
            "--write-back",
            "--no-witness",
        ],
        1,
    );

    assert_eq!(payload["write_back"]["written"], true);
    assert_eq!(payload["write_back"]["entry_count"], 18);
    let mapping_file = payload["write_back"]["mapping_file"]
        .as_str()
        .expect("mapping file");
    assert!(registry.join(mapping_file).exists());

    let lookup_input = temp_dir.path().join("lookup.jsonl");
    fs::write(
        &lookup_input,
        "{\"id\":\"WFCM2019-C50|1\"}\n{\"id\":\"223232\"}\n",
    )
    .unwrap();
    let assert = canon_command()
        .args([
            lookup_input.to_str().unwrap(),
            "--registry",
            registry.to_str().unwrap(),
            "--column",
            "id",
            "--explicit",
            "--no-witness",
        ])
        .assert()
        .success();
    let lookup: Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();
    let mappings = lookup["mappings"].as_array().expect("lookup mappings");
    assert_eq!(lookup["summary"]["resolved"], 2);
    assert!(mappings.iter().any(|mapping| {
        mapping["input"] == "u8:WFCM2019-C50|1" && mapping["canonical_id"] == "u8:223232"
    }));
    assert!(
        mappings.iter().any(
            |mapping| mapping["input"] == "u8:223232" && mapping["canonical_id"] == "u8:223232"
        )
    );
}

fn copy_json_registry_fixture(relative: &str, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    let source = manifest_dir().join(relative);
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("json") {
            fs::copy(path, destination.join(entry.file_name())).unwrap();
        }
    }
}
