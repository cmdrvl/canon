use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

mod common;
use common::{canon_bin, write_registry_metadata_full};

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

struct EntityFixture {
    _temp_dir: TempDir,
    registry_dir: PathBuf,
    witness_path: PathBuf,
    run_path: PathBuf,
    solve_path: PathBuf,
    audit_path: PathBuf,
    run_json: Value,
    solve_json: Value,
}

impl EntityFixture {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("temp dir");
        let registry_dir = temp_dir.path().join("registry");
        fs::create_dir_all(&registry_dir).expect("registry dir");
        write_org_registry_metadata(&registry_dir, "bdc-issuers", "2026.03.01", 0);

        let strategy_path = temp_dir.path().join("strategy.yaml");
        fs::write(&strategy_path, VALID_STRATEGY).expect("strategy");

        let rows_path = temp_dir.path().join("rows.csv");
        write_rows_csv(&rows_path, false);

        let witness_path = temp_dir.path().join("witness.jsonl");
        let work_dir = temp_dir.path().join("work");
        let run_path = work_dir.join("run.json");
        let solve_path = work_dir.join("solve").join("solve.json");
        let audit_path = temp_dir.path().join("audit.json");

        let output = canon_cmd(&witness_path)
            .args([
                "entity",
                "run",
                rows_path.to_str().unwrap(),
                "--profile",
                "regab_firm_identity",
                "--strategy",
                strategy_path.to_str().unwrap(),
                "--registry",
                registry_dir.to_str().unwrap(),
                "--work-dir",
                work_dir.to_str().unwrap(),
            ])
            .assert()
            .success();
        let run_stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
        let run_json: Value = serde_json::from_str(&run_stdout).expect("run json");
        let solve_json: Value =
            serde_json::from_slice(&fs::read(&solve_path).expect("solve artifact"))
                .expect("solve json");
        fs::write(
            &audit_path,
            serde_json::to_vec_pretty(&minimal_audit_artifact()).unwrap(),
        )
        .expect("write audit");

        Self {
            _temp_dir: temp_dir,
            registry_dir,
            witness_path,
            run_path,
            solve_path,
            audit_path,
            run_json,
            solve_json,
        }
    }
}

fn canon_cmd(witness_path: &Path) -> Command {
    let mut command = canon_bin();
    command.env("EPISTEMIC_WITNESS", witness_path);
    command
}

fn canon_args_from_emitted_command(command: &str) -> Vec<String> {
    let mut tokens = command
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert_eq!(tokens.first().map(String::as_str), Some("canon"));
    tokens.remove(0);
    tokens
}

fn write_org_registry_metadata(path: &Path, id: &str, version: &str, entry_count: usize) {
    write_registry_metadata_full(
        path,
        id,
        version,
        entry_count,
        "org test registry",
        "2026-03-24",
    );
}

fn write_rows_csv(path: &Path, malformed_side_fields: bool) {
    let mut writer = csv::Writer::from_path(path).expect("csv writer");
    if malformed_side_fields {
        writer
            .write_record(["source_row_id", "field_name", "org_name"])
            .unwrap();
        writer
            .write_record(["row-1", "issuer", "Acme Corp."])
            .unwrap();
        writer.flush().unwrap();
        return;
    }

    writer
        .write_record(["source_row_id", "field_name", "org_name", "dataset"])
        .unwrap();
    writer
        .write_record(["row-1", "issuer", "Acme Corp.", "doc-a"])
        .unwrap();
    writer
        .write_record(["row-2", "issuer", "ACME Corporation", "doc-b"])
        .unwrap();
    writer.flush().unwrap();
}

fn minimal_audit_artifact() -> Value {
    json!({
        "version": "canon_entity_audit.v0",
        "summary": {"decision": "PROMOTE", "hard_gates_passed": true}
    })
}

#[test]
fn entity_describe_includes_command_family() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--describe")
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).expect("describe json");

    assert_eq!(payload["name"], "canon");
    assert!(payload["invocation"]["usage"].as_array().is_some());
    assert!(payload["subcommands"].as_array().is_some());
}

#[test]
fn entity_run_public_contract_succeeds_with_profile_and_work_dir() {
    let fixture = EntityFixture::new();

    assert_eq!(fixture.run_json["version"], "canon_entity_run.v0");
    assert_eq!(
        fixture.run_json["metadata"]["profile"]["id"],
        "regab_firm_identity"
    );
    assert_eq!(fixture.run_json["summary"]["counts"]["row_count"], 2);
    assert_eq!(fixture.run_json["summary"]["labels"]["status"], "completed");
    assert_eq!(fixture.solve_json["version"], "canon_entity_solve.v0");
    assert_eq!(
        fixture.solve_json["metadata"]["profile"]["id"],
        "regab_firm_identity"
    );
    assert!(fixture.run_path.exists());
    assert!(fixture.solve_path.exists());
    let witness_body = fs::read_to_string(&fixture.witness_path).expect("witness ledger");
    assert!(witness_body.contains("\"subcommand\":\"entity.run\""));
}

#[test]
fn entity_promote_refuses_current_artifact_until_lifecycle_cutover() {
    let fixture = EntityFixture::new();

    let output = canon_cmd(&fixture.witness_path)
        .args([
            "entity",
            "promote",
            fixture.solve_path.to_str().unwrap(),
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
    let refusal: Value = serde_json::from_str(&stdout).expect("promote refusal");

    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_PARSE");
    assert_eq!(
        refusal["refusal"]["detail"]["artifact"],
        "entity result artifact"
    );
    assert!(
        refusal["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("missing field `observations`")
    );
}

#[test]
fn entity_review_export_accepts_current_native_solve_artifact() {
    let fixture = EntityFixture::new();

    let emitted_review = fixture.run_json["next_commands"]["review_export"]
        .as_str()
        .expect("review export handoff command");
    assert!(emitted_review.contains(fixture.solve_path.to_str().unwrap()));
    let export_csv = canon_cmd(&fixture.witness_path)
        .args(canon_args_from_emitted_command(emitted_review))
        .assert()
        .success();
    let review_csv = String::from_utf8(export_csv.get_output().stdout.clone()).unwrap();
    let mut reader = csv::Reader::from_reader(review_csv.as_bytes());
    assert!(
        reader
            .headers()
            .unwrap()
            .iter()
            .any(|header| header == "review_id")
    );

    let export_json = canon_cmd(&fixture.witness_path)
        .args(canon_args_from_emitted_command(
            &emitted_review.replace("--emit csv", "--emit json"),
        ))
        .assert()
        .success();
    let review: Value =
        serde_json::from_slice(&export_json.get_output().stdout).expect("review export");

    assert_eq!(review["version"], "canon_entity_review_queue.v0");
    assert_eq!(
        review["source_solve_hash"],
        fixture.solve_json["artifact_content_hash"]
    );
    assert!(review["review_items"].is_array());
}

#[test]
fn entity_run_refuses_malformed_side_fields() {
    let temp_dir = TempDir::new().expect("temp dir");
    let registry_dir = temp_dir.path().join("registry");
    fs::create_dir_all(&registry_dir).unwrap();
    write_org_registry_metadata(&registry_dir, "bdc-issuers", "2026.03.01", 0);

    let strategy_path = temp_dir.path().join("strategy.yaml");
    fs::write(&strategy_path, VALID_STRATEGY).unwrap();
    let rows_path = temp_dir.path().join("rows.csv");
    write_rows_csv(&rows_path, true);

    let output = canon_cmd(&temp_dir.path().join("witness.jsonl"))
        .args([
            "entity",
            "run",
            rows_path.to_str().unwrap(),
            "--profile",
            "regab_firm_identity",
            "--strategy",
            strategy_path.to_str().unwrap(),
            "--registry",
            registry_dir.to_str().unwrap(),
            "--work-dir",
            temp_dir.path().join("work").to_str().unwrap(),
            "--no-witness",
        ])
        .assert()
        .code(2);
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let refusal: Value = serde_json::from_str(&stdout).expect("refusal json");

    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_INPUT_CONTRACT");
    assert_eq!(refusal["refusal"]["detail"]["field"], "dataset");
}

#[test]
fn entity_promote_refuses_current_run_artifact_before_registry_checks() {
    let fixture = EntityFixture::new();

    let output = canon_cmd(&fixture.witness_path)
        .args([
            "entity",
            "promote",
            fixture.run_path.to_str().unwrap(),
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
    assert_eq!(refusal["refusal"]["code"], "E_PARSE");
    assert!(
        refusal["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("missing field `observations`")
    );
}

#[test]
fn entity_run_records_registry_snapshot_and_handoff_commands() {
    let fixture = EntityFixture::new();

    assert_eq!(
        fixture.run_json["metadata"]["registry_snapshot"]["id"],
        "bdc-issuers"
    );
    assert_eq!(
        fixture.run_json["metadata"]["registry_snapshot"]["version"],
        "2026.03.01"
    );
    let promote = fixture.run_json["next_commands"]["promote"]
        .as_str()
        .expect("promote next command");
    assert!(promote.contains("work/solve/solve.json"));
    assert!(promote.contains("--audit"));
}
