use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

const VALID_STRATEGY: &str = r#"
strategy_id: bdc_org_graph.v1
strategy_version: "0.1.0"
entity_type: issuer
description: "Resolve BDC portfolio-company identities via constrained evidence graph"
id_prefix: "IC"

observations:
  name_fields: [portfolio_company]
  required_side_fields: [alias_surfaces_json]
  context_fields: [industry, investment_type]
  anchor_fields:
    lei: lei
    figi: figi
    cik: cik

normalize:
  views:
    core_name:
      - lowercase
      - strip_footnotes
      - strip_legal_suffixes
      - normalize_whitespace
    acronym:
      - extract_initials
    rare_tokens:
      - tokenize
      - drop_stopwords

blocking:
  - op: exact_view
    view: core_name
  - op: rare_token_overlap
    left_view: rare_tokens
    right_view: rare_tokens
    min_tokens: 1
    min_idf: 1.0
  - op: shared_anchor
    anchor: lei
  - op: registry_alias_match

evidence:
  must_link:
    - op: shared_anchor
      anchor: lei
  support:
    - op: exact_view
      view: core_name
      score: 32
    - op: acronym_plus_token
      acronym_view: acronym
      token_view: rare_tokens
      score: 10
    - op: categorical_equal
      field: industry
      score: 4
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
  precedence: [lei, cik, figi]
  trusted_for_must_link: [lei]
  trusted_for_single_doc_promotion: [lei]
  support_only: [cik, figi]
  require_unique_for_attachment: true

promotion:
  write_states: [PROMOTABLE_NEW, RESOLVED_EXISTING]
  require_zero_anchor_conflicts: true
  require_holdout_non_regression: true
  require_perturbation_stability_gte: 0.995
  min_distinct_docs: 2
  allow_single_doc_if_unique_anchor: true
"#;

struct OrgFixture {
    _temp_dir: TempDir,
    registry_dir: PathBuf,
    witness_path: PathBuf,
    result_path: PathBuf,
    audit_path: PathBuf,
    result_json: Value,
}

impl OrgFixture {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("temp dir");
        let registry_dir = temp_dir.path().join("registry");
        fs::create_dir_all(&registry_dir).expect("registry dir");
        write_registry_metadata(&registry_dir, "bdc-issuers", "2026.03.01", 0);

        let strategy_path = temp_dir.path().join("strategy.yaml");
        fs::write(&strategy_path, VALID_STRATEGY).expect("strategy");

        let rows_path = temp_dir.path().join("rows.csv");
        write_rows_csv(&rows_path, false);

        let witness_path = temp_dir.path().join("witness.jsonl");
        let result_path = temp_dir.path().join("result.json");
        let audit_path = temp_dir.path().join("audit.json");

        let output = canon_cmd(&witness_path)
            .args([
                "org",
                "run",
                rows_path.to_str().unwrap(),
                "--strategy",
                strategy_path.to_str().unwrap(),
                "--registry",
                registry_dir.to_str().unwrap(),
            ])
            .assert()
            .success();
        let result_stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
        let result_json: Value = serde_json::from_str(&result_stdout).expect("result json");
        fs::write(&result_path, &result_stdout).expect("write result");
        fs::write(
            &audit_path,
            serde_json::to_vec_pretty(&build_matching_audit(
                &result_json,
                result_stdout.as_bytes(),
            ))
            .unwrap(),
        )
        .expect("write audit");

        Self {
            _temp_dir: temp_dir,
            registry_dir,
            witness_path,
            result_path,
            audit_path,
            result_json,
        }
    }
}

fn canon_cmd(witness_path: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    command.env("EPISTEMIC_WITNESS", witness_path);
    command
}

fn write_registry_metadata(path: &Path, id: &str, version: &str, entry_count: usize) {
    let registry_json = json!({
        "id": id,
        "version": version,
        "description": "org test registry",
        "updated": "2026-03-24",
        "entry_count": entry_count,
    });
    fs::write(
        path.join("registry.json"),
        serde_json::to_vec_pretty(&registry_json).unwrap(),
    )
    .unwrap();
}

fn write_rows_csv(path: &Path, malformed_side_fields: bool) {
    let mut writer = csv::Writer::from_path(path).expect("csv writer");
    writer
        .write_record([
            "source_row_id",
            "doc_id",
            "as_of_date",
            "portfolio_company",
            "alias_surfaces_json",
            "mention_surfaces_json",
            "industry",
            "investment_type",
            "lei",
            "figi",
            "cik",
        ])
        .unwrap();
    writer
        .write_record([
            "row-1",
            "doc-a",
            "2026-03-01",
            "Acme Corp.",
            if malformed_side_fields {
                "{\"broken\""
            } else {
                "[\"ACME Corporation\"]"
            },
            "[]",
            "Software",
            "Equity",
            "LEI-1",
            "",
            "",
        ])
        .unwrap();
    writer
        .write_record([
            "row-2",
            "doc-b",
            "2026-03-02",
            "ACME Corporation",
            "[\"Acme Corp.\"]",
            "[]",
            "Software",
            "Equity",
            "LEI-1",
            "",
            "",
        ])
        .unwrap();
    writer.flush().unwrap();
}

fn build_matching_audit(result_json: &Value, result_bytes: &[u8]) -> Value {
    json!({
        "version": "canon_org_audit.v0",
        "result": {
            "version": result_json["version"],
            "content_hash": blake3_string(result_bytes),
            "strategy_content_hash": result_json["strategy"]["content_hash"],
            "lookup_snapshot_hash": result_json["registry"]["lookup_snapshot_hash"],
            "escrow_snapshot_hash": result_json["registry"]["escrow_snapshot_hash"],
        },
        "suite": {
            "id": "bdc_org_eval.v1"
        },
        "summary": {
            "decision": "PROMOTE",
            "hard_gates_passed": true
        },
        "metrics": {
            "gold_pair_f1": 0.99,
            "anchor_consistency": 1.0,
            "anchor_conflicts": 0,
            "holdout_score": 0.99,
            "contradiction_rate": 0.0,
            "perturbation_stability": 1.0,
            "continuity_gain": 0.1,
            "compression_gain": 0.1,
            "registry_churn": 0.0,
            "escrow_reuse_rate": 0.0
        },
        "gate_failures": []
    })
}

fn blake3_string(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

#[test]
fn org_describe_includes_command_family() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--describe")
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).expect("describe json");

    let usage = payload["invocation"]["usage"]
        .as_array()
        .expect("usage array")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    assert!(usage.iter().any(|line| line.contains("canon org run")));
    assert!(usage.iter().any(|line| line.contains("canon org promote")));

    let subcommands = payload["subcommands"]
        .as_array()
        .expect("subcommands array");
    assert!(subcommands
        .iter()
        .any(|entry| entry["name"] == "org run" && entry["output_schema"] == "canon_org_run.v0"));
    assert!(
        subcommands
            .iter()
            .any(|entry| entry["name"] == "org explain"
                && entry["output_schema"] == "canon_org_explain.v0")
    );
}

#[test]
fn org_run_and_promote_happy_path_succeeds() {
    let fixture = OrgFixture::new();

    assert_eq!(fixture.result_json["version"], "canon_org_run.v0");
    assert_eq!(fixture.result_json["summary"]["promotable_new"], 2);
    let witness_body = fs::read_to_string(&fixture.witness_path).expect("witness ledger");
    assert!(witness_body.contains("\"subcommand\":\"org.run\""));

    let output = canon_cmd(&fixture.witness_path)
        .args([
            "org",
            "promote",
            fixture.result_path.to_str().unwrap(),
            "--audit",
            fixture.audit_path.to_str().unwrap(),
            "--registry",
            fixture.registry_dir.to_str().unwrap(),
            "--next-version",
            "2026.03.02",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).expect("promote json");

    assert_eq!(payload["version"], "canon_org_promote.v0");
    assert_eq!(payload["registry"]["version_before"], "2026.03.01");
    assert_eq!(payload["registry"]["version_after"], "2026.03.02");
    assert_eq!(payload["writes"]["new_entity_entries"], 2);
    assert!(fixture.registry_dir.join("org-20260302.json").exists());
}

#[test]
fn org_run_refuses_malformed_side_fields() {
    let temp_dir = TempDir::new().expect("temp dir");
    let registry_dir = temp_dir.path().join("registry");
    fs::create_dir_all(&registry_dir).unwrap();
    write_registry_metadata(&registry_dir, "bdc-issuers", "2026.03.01", 0);

    let strategy_path = temp_dir.path().join("strategy.yaml");
    fs::write(&strategy_path, VALID_STRATEGY).unwrap();
    let rows_path = temp_dir.path().join("rows.csv");
    write_rows_csv(&rows_path, true);

    let output = canon_cmd(&temp_dir.path().join("witness.jsonl"))
        .args([
            "org",
            "run",
            rows_path.to_str().unwrap(),
            "--strategy",
            strategy_path.to_str().unwrap(),
            "--registry",
            registry_dir.to_str().unwrap(),
            "--no-witness",
        ])
        .assert()
        .code(2);
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let refusal: Value = serde_json::from_str(&stdout).expect("refusal json");

    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_ORG_INPUT_CONTRACT");
}

#[test]
fn org_promote_refuses_stale_audit_result_mismatch() {
    let fixture = OrgFixture::new();
    let mut stale_result = fixture.result_json.clone();
    stale_result["summary"]["observations"] = json!(99);
    let stale_result_path = fixture
        .result_path
        .with_file_name("result-stale-audit-mismatch.json");
    fs::write(
        &stale_result_path,
        serde_json::to_vec_pretty(&stale_result).unwrap(),
    )
    .unwrap();

    let output = canon_cmd(&fixture.witness_path)
        .args([
            "org",
            "promote",
            stale_result_path.to_str().unwrap(),
            "--audit",
            fixture.audit_path.to_str().unwrap(),
            "--registry",
            fixture.registry_dir.to_str().unwrap(),
            "--next-version",
            "2026.03.02",
        ])
        .assert()
        .code(2);
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let refusal: Value = serde_json::from_str(&stdout).expect("refusal json");

    assert_eq!(refusal["outcome"], "REFUSAL");
    assert!(
        refusal["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("does not match the result artifact")
    );
}

#[test]
fn org_promote_refuses_stale_registry_and_same_version() {
    let fixture = OrgFixture::new();

    let stale_registry = json!({
        "id": "bdc-issuers",
        "version": "2026.03.99",
        "description": "org test registry",
        "updated": "2026-03-24",
        "entry_count": 0,
    });
    fs::write(
        fixture.registry_dir.join("registry.json"),
        serde_json::to_vec_pretty(&stale_registry).unwrap(),
    )
    .unwrap();

    let stale_output = canon_cmd(&fixture.witness_path)
        .args([
            "org",
            "promote",
            fixture.result_path.to_str().unwrap(),
            "--audit",
            fixture.audit_path.to_str().unwrap(),
            "--registry",
            fixture.registry_dir.to_str().unwrap(),
            "--next-version",
            "2026.03.02",
        ])
        .assert()
        .code(2);
    let stale_stdout = String::from_utf8(stale_output.get_output().stdout.clone()).unwrap();
    let stale_refusal: Value = serde_json::from_str(&stale_stdout).expect("refusal json");
    assert_eq!(stale_refusal["refusal"]["code"], "E_ORG_STALE_REGISTRY");

    write_registry_metadata(&fixture.registry_dir, "bdc-issuers", "2026.03.01", 0);
    let same_version = canon_cmd(&fixture.witness_path)
        .args([
            "org",
            "promote",
            fixture.result_path.to_str().unwrap(),
            "--audit",
            fixture.audit_path.to_str().unwrap(),
            "--registry",
            fixture.registry_dir.to_str().unwrap(),
            "--next-version",
            "2026.03.01",
        ])
        .assert()
        .code(2);
    let same_version_stdout = String::from_utf8(same_version.get_output().stdout.clone()).unwrap();
    let same_version_refusal: Value =
        serde_json::from_str(&same_version_stdout).expect("refusal json");
    assert_eq!(
        same_version_refusal["refusal"]["code"],
        "E_ORG_VERSION_BUMP_REQUIRED"
    );
}
