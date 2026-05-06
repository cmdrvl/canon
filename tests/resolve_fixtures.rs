use serde_json::Value;
use std::{fs, path::Path};

fn fixture(path: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/resolve")
        .join(path)
        .display()
        .to_string()
}

#[test]
fn resolve_csv_fixtures_are_parseable_and_cover_expected_counts() {
    let cases = [
        ("tapes/reference_loans.csv", 10usize),
        ("tapes/target_loans.csv", 12usize),
        ("tapes/missing_column_target.csv", 1usize),
        ("tapes/empty_target.csv", 0usize),
        ("tapes/too_many_candidates_reference.csv", 4usize),
        ("tapes/too_many_candidates_target.csv", 1usize),
    ];

    for (relative, expected_rows) in cases {
        let path = fixture(relative);
        let mut reader = csv::Reader::from_path(&path).expect("csv fixture opens");
        let headers = reader.headers().expect("csv headers").clone();
        assert!(!headers.is_empty(), "{relative} has headers");
        let row_count = reader
            .records()
            .collect::<Result<Vec<_>, _>>()
            .expect("csv records")
            .len();
        assert_eq!(row_count, expected_rows, "{relative} row count");
    }
}

#[test]
fn resolve_jsonl_and_gold_fixtures_are_parseable() {
    let cases = [
        ("tapes/reference_loans.jsonl", 10usize),
        ("tapes/target_loans.jsonl", 12usize),
        ("gold/loan_matches.jsonl", 9usize),
        ("gold/loan_matches_with_regression.jsonl", 9usize),
    ];

    for (relative, expected_lines) in cases {
        let content = fs::read_to_string(fixture(relative)).expect("jsonl fixture");
        let mut count = 0usize;
        for line in content.lines().filter(|line| !line.trim().is_empty()) {
            serde_json::from_str::<Value>(line).expect("jsonl line parses");
            count += 1;
        }
        assert_eq!(count, expected_lines, "{relative} line count");
    }
}

#[test]
fn resolve_yaml_strategy_fixtures_are_parseable() {
    let cases = [
        "strategies/cmbs_loans.valid.yaml",
        "strategies/minimal.valid.yaml",
        "strategies/malformed_missing_threshold.yaml",
        "strategies/too_many_candidates.yaml",
    ];

    for relative in cases {
        let content = fs::read_to_string(fixture(relative)).expect("strategy fixture");
        serde_yaml::from_str::<serde_yaml::Value>(&content).expect("yaml parses");
    }
}

#[test]
fn resolve_valid_strategy_fixtures_match_scaffold_contract() {
    let content = fs::read_to_string(fixture("strategies/cmbs_loans.valid.yaml"))
        .expect("valid strategy fixture");
    let strategy: canon::resolve::ResolveStrategy =
        serde_yaml::from_str(&content).expect("valid strategy contract");

    assert_eq!(strategy.id, "cmbs-loan-match.v1");
    assert_eq!(strategy.identity.reference.id_columns, vec!["loan_id"]);
    assert_eq!(
        strategy.identity.target.id_columns,
        vec!["deal", "loan_number"]
    );
    assert_eq!(strategy.candidate_filter.len(), 3);
    assert_eq!(strategy.assertions.len(), 7);
    assert_eq!(strategy.max_candidates, Some(10));

    let malformed = fs::read_to_string(fixture("strategies/malformed_missing_threshold.yaml"))
        .expect("malformed strategy fixture");
    assert!(serde_yaml::from_str::<canon::resolve::ResolveStrategy>(&malformed).is_err());
}

#[test]
fn resolve_servicer_registry_fixture_is_parseable() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/registries/resolve-servicers");
    serde_json::from_str::<Value>(
        &fs::read_to_string(root.join("registry.json")).expect("registry metadata"),
    )
    .expect("registry metadata json");
    let aliases = serde_json::from_str::<Value>(
        &fs::read_to_string(root.join("servicer-aliases.json")).expect("alias mapping"),
    )
    .expect("alias mapping json");
    assert_eq!(aliases.as_array().unwrap().len(), 8);
}
