use canon::resolve::{
    ResolveRequest, TapeLoadOptions, load_strategy, load_tapes, select_candidates,
};
use std::fs;
use tempfile::tempdir;

const ROW_COUNT: usize = 5_000;

#[test]
fn resolves_five_thousand_by_five_thousand_with_bounded_candidates() {
    let temp_dir = tempdir().unwrap();
    let reference = temp_dir.path().join("reference.csv");
    let target = temp_dir.path().join("target.csv");
    let strategy = temp_dir.path().join("strategy.yaml");
    let registry = temp_dir.path().join("registry");
    fs::create_dir_all(&registry).unwrap();

    fs::write(
        registry.join("registry.json"),
        r#"{
  "id": "resolve-scale",
  "version": "0.1.0",
  "description": "empty resolve scale test registry",
  "updated": "2026-05-06",
  "entry_count": 0
}
"#,
    )
    .unwrap();
    fs::write(&reference, reference_csv(ROW_COUNT)).unwrap();
    fs::write(&target, target_csv(ROW_COUNT)).unwrap();
    fs::write(
        &strategy,
        r#"strategy_id: scale-test.v1
strategy_version: "0.1.0"
entity_type: loan
identity:
  reference:
    id_columns: [loan_id]
  target:
    id_columns: [deal, loan_number]
candidate_filter:
  - field_ref: bucket
    field_tgt: bucket
    op: exact
assertions:
  - field_ref: address
    field_tgt: address
    op: exact
    weight: 1.0
    required: true
match_threshold: 1.0
ambiguity_gap: 0.10
max_candidates: 1
"#,
    )
    .unwrap();

    let parsed_strategy = load_strategy(&strategy).unwrap();
    let tapes = load_tapes(
        &reference,
        &target,
        &parsed_strategy,
        TapeLoadOptions {
            max_rows: None,
            max_bytes: None,
        },
    )
    .unwrap();
    let selection = select_candidates(&tapes, &parsed_strategy, None, None).unwrap();
    assert_eq!(selection.targets.len(), ROW_COUNT);
    assert_eq!(selection.total_candidate_pairs(), ROW_COUNT);

    let artifact = canon::resolve::run(ResolveRequest {
        reference_tape: reference,
        target_tape: target,
        strategy,
        registry,
        gold: None,
        write_back: false,
        max_candidates: Some(1),
        max_rows: None,
        max_bytes: None,
        no_witness: true,
    })
    .unwrap();

    assert_eq!(artifact.summary.target_records, ROW_COUNT);
    assert_eq!(artifact.summary.matched, ROW_COUNT);
    assert_eq!(artifact.summary.unmatched, 0);
    assert_eq!(artifact.summary.ambiguous, 0);
    assert!(
        artifact
            .matches
            .iter()
            .all(|record| record.runner_up.is_none())
    );
}

fn reference_csv(row_count: usize) -> String {
    let mut content = String::from("loan_id,bucket,address\n");
    for index in 0..row_count {
        content.push_str(&format!("R{index:05},B{index:05},Address {index:05}\n"));
    }
    content
}

fn target_csv(row_count: usize) -> String {
    let mut content = String::from("deal,loan_number,bucket,address\n");
    for index in 0..row_count {
        content.push_str(&format!("D,T{index:05},B{index:05},Address {index:05}\n"));
    }
    content
}
