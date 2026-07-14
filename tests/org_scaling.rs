use assert_cmd::Command;
use csv::Writer;
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};
use tempfile::tempdir;

const TEST_STRATEGY: &str = r#"
strategy_id: cmbs_tenant_scaling.v1
strategy_version: 0.1.0
"#;

fn canon() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn write_registry_metadata(dir: &Path, id: &str, version: &str, entry_count: usize) {
    let registry_json = json!({
        "id": id,
        "version": version,
        "description": "test registry",
        "updated": "2026-03-24",
        "entry_count": entry_count,
    });
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("registry.json"),
        serde_json::to_string_pretty(&registry_json).unwrap(),
    )
    .unwrap();
}

fn write_mapping_file(dir: &Path, name: &str, entries: Value) {
    fs::write(
        dir.join(name),
        serde_json::to_string_pretty(&entries).unwrap(),
    )
    .unwrap();
}

fn write_strategy(path: &Path) {
    fs::write(path, TEST_STRATEGY).unwrap();
}

fn count(payload: &Value, name: &str) -> u64 {
    payload["summary"]["counts"][name]
        .as_u64()
        .unwrap_or_else(|| panic!("missing summary count {name}"))
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn write_entity_rows(path: &Path) {
    let mut writer = Writer::from_path(path).unwrap();
    writer
        .write_record([
            "source_row_id",
            "deal_id",
            "loan_id",
            "property_id",
            "raw_tenant_name",
            "alias_surfaces_json",
            "mention_surfaces_json",
        ])
        .unwrap();

    for row_number in 0..20 {
        let entity_ordinal = row_number % 4;
        let variant_ordinal = (row_number / 4) % 4;
        let source_ordinal = (row_number / 16) % 2;
        writer
            .write_record([
                format!("row-{row_number:02}"),
                format!("D{source_ordinal:02}"),
                format!("L{entity_ordinal:02}"),
                format!("P{variant_ordinal:02}"),
                format!("Native Entity {entity_ordinal:02} Variant {variant_ordinal:02}"),
                "[]".to_string(),
                "[]".to_string(),
            ])
            .unwrap();
    }

    writer.flush().unwrap();
}

fn write_lookup_input(path: &Path) {
    fs::write(path, "cusip\n037833100\n").unwrap();
}

#[test]
fn entity_cli_scaling_stays_within_structural_budgets() {
    let temp_dir = tempdir().unwrap();
    let registry_dir = temp_dir.path().join("scale-registry");
    let strategy_path = temp_dir.path().join("strategy.yaml");
    let rows_path = temp_dir.path().join("rows.csv");
    let work_dir = temp_dir.path().join("work");

    write_registry_metadata(&registry_dir, "entity-scale-registry", "2026.03.01", 0);
    write_strategy(&strategy_path);
    write_entity_rows(&rows_path);

    let run_assert = canon()
        .arg("entity")
        .arg("run")
        .arg(&rows_path)
        .arg("--profile")
        .arg("cmbs_tenant_label")
        .arg("--strategy")
        .arg(&strategy_path)
        .arg("--registry")
        .arg(&registry_dir)
        .arg("--work-dir")
        .arg(&work_dir)
        .arg("--no-witness")
        .arg("--emit")
        .arg("json")
        .assert()
        .success();
    let run_payload: Value = serde_json::from_slice(&run_assert.get_output().stdout).unwrap();

    assert_eq!(run_payload["version"], "canon_entity_run.v1");
    assert!(
        run_payload["artifact_content_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );

    assert_eq!(count(&run_payload, "row_count"), 20);
    assert_eq!(count(&run_payload, "prepared_surfaces"), 16);
    assert_eq!(count(&run_payload, "index_surfaces"), 16);
    assert_eq!(count(&run_payload, "exact_resolved_surfaces"), 0);

    let candidate_pairs = count(&run_payload, "candidate_pairs");
    let evidence_records = count(&run_payload, "evidence_records");
    let solved_entities = count(&run_payload, "solved_entities");
    let review_groups = count(&run_payload, "review_group_count");
    assert!(
        candidate_pairs > 0 && candidate_pairs <= 128,
        "candidate pair budget changed: {candidate_pairs}"
    );
    assert!(
        evidence_records > 0 && evidence_records <= candidate_pairs,
        "evidence budget changed: evidence_records={evidence_records} candidate_pairs={candidate_pairs}"
    );
    assert!(
        solved_entities <= 16,
        "solve cardinality exceeded prepared surfaces: {solved_entities}"
    );
    assert!(
        review_groups <= solved_entities,
        "review groups exceeded solved entity count: review_groups={review_groups} solved_entities={solved_entities}"
    );

    let stages = run_payload["stage_artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|stage| stage["stage"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        stages,
        BTreeSet::from([
            "block",
            "cache_enabled",
            "evidence",
            "index",
            "prepare",
            "solve"
        ])
    );

    assert!(work_dir.join("run").join("run.json").exists());
    assert!(work_dir.join("prepare").join("prepare.json").exists());
    assert!(work_dir.join("index").join("index.json").exists());
    assert!(work_dir.join("index").join("postings.bin").exists());
    assert!(work_dir.join("block").join("block.json").exists());
    assert!(work_dir.join("block").join("candidates.jsonl").exists());
    assert!(work_dir.join("evidence").join("evidence.json").exists());
    assert!(work_dir.join("evidence").join("evidence.jsonl").exists());
    assert!(work_dir.join("solve").join("solve.json").exists());

    let block_artifact = read_json(&work_dir.join("block").join("block.json"));
    let evidence_artifact = read_json(&work_dir.join("evidence").join("evidence.json"));
    assert_eq!(block_artifact["version"], "canon_entity_block.v1");
    assert_eq!(evidence_artifact["version"], "canon_entity_evidence.v1");
    assert_eq!(count(&block_artifact, "candidate_pairs"), candidate_pairs);
    assert_eq!(
        count(&evidence_artifact, "evidence_records"),
        evidence_records
    );
}

#[test]
fn lookup_path_non_regression_smoke_still_resolves_exact_matches() {
    let temp_dir = tempdir().unwrap();
    let registry_dir = temp_dir.path().join("lookup-registry");
    let input_path = temp_dir.path().join("input.csv");

    write_registry_metadata(&registry_dir, "cusip-to-isin", "2026.03.01", 1);
    write_mapping_file(
        &registry_dir,
        "primary.json",
        json!([
            {
                "input": "037833100",
                "canonical_id": "US0378331005",
                "canonical_type": "isin",
                "rule_id": "TEST"
            }
        ]),
    );
    write_lookup_input(&input_path);

    let lookup_assert = canon()
        .arg(&input_path)
        .arg("--registry")
        .arg(&registry_dir)
        .arg("--column")
        .arg("cusip")
        .arg("--explicit")
        .assert()
        .success();
    let payload: Value = serde_json::from_slice(&lookup_assert.get_output().stdout).unwrap();

    assert_eq!(payload["summary"]["total"], 1);
    assert_eq!(payload["summary"]["resolved"], 1);
    assert_eq!(payload["summary"]["unresolved"], 0);
    assert_eq!(payload["mappings"][0]["input"], "u8:037833100");
    assert_eq!(payload["mappings"][0]["canonical_id"], "u8:US0378331005");
}
