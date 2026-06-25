use assert_cmd::Command;
use csv::Writer;
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::tempdir;

const TEST_STRATEGY: &str = r#"
strategy_id: bdc_org_graph.v1
strategy_version: 0.1.0
entity_type: organization
description: Synthetic org scaling fixture
id_prefix: IC

observations:
  name_fields: [portfolio_company]
  required_side_fields: [alias_surfaces_json]
  context_fields: [industry]
  anchor_fields:
    lei: lei

normalize:
  views:
    core_name:
      - lowercase
      - strip_legal_suffixes
      - normalize_whitespace

blocking:
  - op: exact_view
    view: core_name
  - op: shared_anchor
    anchor: lei

evidence:
  must_link:
    - op: shared_anchor
      anchor: lei
  support:
    - op: exact_view
      view: core_name
      score: 32
  cannot_link:
    - op: conflicting_anchor
      anchor: lei

solver:
  score_mode: namespace_max_sum
  component_score_mode: core_best_pair_sum
  merge_policy: reciprocal_best
  backbone_score_min: 32
  backbone_requires_positive_name: true
  attach_score_min: 28
  abstain_margin: 6
  max_cluster_diameter: 2
  require_positive_name_evidence: true
  attach_requires_backbone_contact: true
  score_against_backbone_only: true
  attachments_do_not_chain: true

reconcile:
  single_incumbent_overlap: inherit
  multi_incumbent_overlap: abstain_conflict
  allow_incumbent_merge: false
  allow_alias_writeback_for_resolved_existing: true

anchors:
  precedence: [lei]
  trusted_for_must_link: [lei]
  trusted_for_single_doc_promotion: [lei]
  support_only: []
  require_unique_for_attachment: true

promotion:
  write_states: [PROMOTABLE_NEW, RESOLVED_EXISTING]
  require_zero_anchor_conflicts: true
  require_holdout_non_regression: true
  require_perturbation_stability_gte: 0.995
  min_distinct_docs: 2
  allow_single_doc_if_unique_anchor: true
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

fn write_entity_rows(path: &Path) {
    let mut writer = Writer::from_path(path).unwrap();
    writer
        .write_record([
            "source_row_id",
            "doc_id",
            "as_of_date",
            "portfolio_company",
            "alias_surfaces_json",
            "industry",
            "lei",
        ])
        .unwrap();

    for group in 0..4 {
        let company = format!("Group {} Holdings", group + 1);
        let alias = format!(r#"["{}"]"#, company.to_uppercase());
        let lei = format!("549300GROUP{:02}", group + 1);
        for member in 0..4 {
            writer
                .write_record([
                    format!("group-{group}-row-{member}"),
                    format!("doc-group-{group}-{member}"),
                    "2026-03-24".to_string(),
                    company.clone(),
                    alias.clone(),
                    "software".to_string(),
                    lei.clone(),
                ])
                .unwrap();
        }
    }

    for noise in 0..4 {
        writer
            .write_record([
                format!("noise-row-{noise}"),
                format!("noise-doc-{noise}"),
                "2026-03-24".to_string(),
                format!("Noise Entity {noise}"),
                "[]".to_string(),
                "other".to_string(),
                String::new(),
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
    let registry_dir = temp_dir.path().join("org-registry");
    let strategy_path = temp_dir.path().join("strategy.yaml");
    let rows_path = temp_dir.path().join("rows.csv");
    let block_path = temp_dir.path().join("blocks.jsonl");
    let edge_path = temp_dir.path().join("edges.jsonl");

    write_registry_metadata(&registry_dir, "bdc-issuers", "2026.03.01", 0);
    write_strategy(&strategy_path);
    write_entity_rows(&rows_path);

    let block_assert = canon()
        .arg("entity")
        .arg("block")
        .arg(&rows_path)
        .arg("--strategy")
        .arg(&strategy_path)
        .arg("--registry")
        .arg(&registry_dir)
        .assert()
        .success();
    let block_stdout = String::from_utf8(block_assert.get_output().stdout.clone()).unwrap();
    let block_lines = block_stdout.lines().collect::<Vec<_>>();
    assert_eq!(block_lines.len(), 24, "candidate-pair budget changed");
    for line in &block_lines {
        let record: Value = serde_json::from_str(line).unwrap();
        assert_eq!(record["version"], "canon_entity_block.v0");
    }
    fs::write(&block_path, block_stdout).unwrap();

    let edge_assert = canon()
        .arg("entity")
        .arg("edge")
        .arg(&rows_path)
        .arg("--strategy")
        .arg(&strategy_path)
        .arg("--candidates")
        .arg(&block_path)
        .arg("--registry")
        .arg(&registry_dir)
        .assert()
        .success();
    let edge_stdout = String::from_utf8(edge_assert.get_output().stdout.clone()).unwrap();
    let edge_lines = edge_stdout.lines().collect::<Vec<_>>();
    assert_eq!(edge_lines.len(), 24, "edge-count budget changed");
    for line in &edge_lines {
        let record: Value = serde_json::from_str(line).unwrap();
        assert_eq!(record["version"], "canon_entity_edge.v0");
    }
    fs::write(&edge_path, edge_stdout).unwrap();

    let run_assert = canon()
        .arg("entity")
        .arg("run")
        .arg(&rows_path)
        .arg("--strategy")
        .arg(&strategy_path)
        .arg("--registry")
        .arg(&registry_dir)
        .arg("--no-witness")
        .assert()
        .success();
    let run_payload: Value = serde_json::from_slice(&run_assert.get_output().stdout).unwrap();

    assert_eq!(run_payload["version"], "canon_entity_run.v0");
    assert_eq!(run_payload["summary"]["observations"], 20);
    assert_eq!(run_payload["summary"]["resolved_existing"], 0);
    assert_eq!(run_payload["summary"]["promotable_new"], 16);
    assert_eq!(run_payload["summary"]["abstain_low_evidence"], 4);
    assert_eq!(run_payload["summary"]["abstain_conflict"], 0);
    assert_eq!(run_payload["entities"].as_array().unwrap().len(), 4);
    assert_eq!(run_payload["contradictions"].as_array().unwrap().len(), 0);
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
