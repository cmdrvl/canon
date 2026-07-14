use assert_cmd::Command;
use canon::entity::{
    EntityArtifactMetadata, EntityArtifactReference, EntityInputReference, EntityPatchNamespaces,
    EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
    edge::{EdgeEvidenceHit, build_edge_evidence_record},
    graph::{SignedEvidenceGraphInput, build_signed_evidence_graph},
    score::{ScoreLane, ScoreUnits},
    solve::{
        SolveArtifactRequest, SolveReconciliationConfig, SolveSurfaceProvenance,
        build_solve_artifact_contract,
    },
};
use predicates::prelude::*;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    thread,
};
use tempfile::tempdir;
use tiny_http::{Header, Response, Server, StatusCode};

mod common;
use common::{fixture_path, write_registry_metadata, write_seed_csv};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn write_mapping_file(temp_dir: &Path, name: &str, entries: serde_json::Value) {
    std::fs::write(
        temp_dir.join(name),
        serde_json::to_string_pretty(&entries).unwrap(),
    )
    .unwrap();
}

#[cfg(unix)]
fn shell_quote(value: impl AsRef<Path>) -> String {
    let rendered = value.as_ref().to_string_lossy();
    format!("'{}'", rendered.replace('\'', "'\\''"))
}

#[cfg(unix)]
fn twinning_bin() -> PathBuf {
    std::env::var_os("TWINNING_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| fixture_path("../twinning/target/debug/twinning"))
}

fn write_strategy_schema(path: &Path, vendor_cardinality: u64) {
    std::fs::write(
        path,
        serde_json::to_string_pretty(&serde_json::json!({
            "columns": [
                {"name": "vendor", "type": "string", "cardinality": vendor_cardinality},
                {"name": "amount", "type": "number", "cardinality": 10}
            ]
        }))
        .unwrap(),
    )
    .unwrap();
}

fn doctor_cmd_in(dir: &Path, witness_path: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    command
        .current_dir(dir)
        .env("EPISTEMIC_WITNESS", witness_path);
    command
}

fn assert_doctor_side_effects_absent(dir: &Path, witness_path: &Path) {
    assert!(!witness_path.exists());
    if let Some(parent) = witness_path.parent() {
        assert!(!parent.exists());
    }
    assert!(!dir.join(".doctor").exists());
    assert!(!dir.join(".canon-witness.jsonl").exists());
    assert!(!dir.join(".cmdrvl").exists());
    assert!(!dir.join("_index.sqlite").exists());
}

fn assert_all_side_effects_false(side_effects: &Value) {
    let object = side_effects
        .as_object()
        .expect("side_effects should be a JSON object");
    assert!(!object.is_empty());
    for (name, value) in object {
        assert_eq!(value, false, "side effect {name} should be false");
    }
}

struct EntityLinkSmokeFixture {
    reference: PathBuf,
    target: PathBuf,
    strategy: PathBuf,
    registry: PathBuf,
    gold: PathBuf,
}

#[derive(Debug, Clone, Copy)]
enum EntityLinkFixtureFormat {
    Csv,
    Tsv,
    Jsonl,
    Ndjson,
}

impl EntityLinkFixtureFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Jsonl => "jsonl",
            Self::Ndjson => "ndjson",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Jsonl => "jsonl",
            Self::Ndjson => "ndjson",
        }
    }
}

fn write_entity_link_smoke_fixture(root: &Path, matched: bool) -> EntityLinkSmokeFixture {
    write_entity_link_smoke_fixture_with_format(root, matched, EntityLinkFixtureFormat::Csv)
}

fn write_entity_link_smoke_fixture_with_format(
    root: &Path,
    matched: bool,
    format: EntityLinkFixtureFormat,
) -> EntityLinkSmokeFixture {
    let reference = root.join(format!("reference.{}", format.extension()));
    let target = root.join(format!("target.{}", format.extension()));
    let strategy = root.join("strategy.yaml");
    let registry = root.join("registry");
    let gold = root.join("gold.jsonl");
    std::fs::create_dir_all(&registry).unwrap();

    write_registry_metadata(&registry, "entity-link-smoke", "0.1.0", 0);
    write_entity_link_rows(
        &reference,
        format,
        EntityLinkRoleFixture::Reference,
        matched,
    );
    write_entity_link_rows(&target, format, EntityLinkRoleFixture::Target, matched);
    std::fs::write(
        &strategy,
        r#"strategy_id: entity-link-smoke.v1
strategy_version: "0.1.0"
entity_type: loan
identity:
  reference:
    id_columns: [loan_id]
  target:
    id_columns: [deal, loan_number]
candidate_filter:
  - field_ref: deal
    field_tgt: deal
    op: exact
assertions:
  - field_ref: address
    field_tgt: address
    op: exact
    weight: 0.60
    required: true
  - field_ref: upb
    field_tgt: balance
    op: tolerance_pct
    tolerance: 0.05
    weight: 0.40
    required: false
match_threshold: 0.75
ambiguity_gap: 0.10
max_candidates: 10
"#,
    )
    .unwrap();
    std::fs::write(
        &gold,
        "{\"target_id\":\"D1|1\",\"expected_reference_id\":\"R-1\"}\n",
    )
    .unwrap();

    EntityLinkSmokeFixture {
        reference,
        target,
        strategy,
        registry,
        gold,
    }
}

fn write_entity_link_neutral_mixed_fixture(root: &Path) -> EntityLinkSmokeFixture {
    let reference = root.join("neutral-reference.csv");
    let target = root.join("neutral-target.csv");
    let strategy = root.join("neutral-strategy.yaml");
    let registry = root.join("neutral-registry");
    let gold = root.join("neutral-gold.jsonl");
    std::fs::create_dir_all(&registry).unwrap();

    write_registry_metadata(&registry, "entity-link-neutral", "0.1.0", 0);
    write_seed_csv(
        &reference,
        "org_id,dataset,field_name,org_name,bucket,source_row_id\nR-MATCH,reference,name,Northstar Analytics,B_MATCH,ref-match\nR-AMB-A,reference,name,Harbor Metrics,B_AMB,ref-amb-a\nR-AMB-B,reference,name,Harbor Metrics,B_AMB,ref-amb-b\n",
    );
    write_seed_csv(
        &target,
        "record_id,dataset,field_name,org_name,bucket,source_row_id\nT-MATCH,target,name,Northstar Analytics,B_MATCH,tgt-match\nT-AMB,target,name,Harbor Metrics,B_AMB,tgt-amb\nT-NONE,target,name,Quartz Signal,B_NONE,tgt-none\n",
    );
    std::fs::write(
        &strategy,
        r#"strategy_id: entity-link-neutral-mixed.v1
strategy_version: "0.1.0"
entity_type: organization
identity:
  reference:
    id_columns: [org_id]
  target:
    id_columns: [record_id]
candidate_filter:
  - field_ref: bucket
    field_tgt: bucket
    op: exact
assertions:
  - field_ref: org_name
    field_tgt: org_name
    op: exact
    weight: 1.0
    required: true
match_threshold: 1.0
ambiguity_gap: 0.10
max_candidates: 10
"#,
    )
    .unwrap();
    std::fs::write(
        &gold,
        "{\"target_id\":\"T-MATCH\",\"expected_reference_id\":\"R-MATCH\"}\n",
    )
    .unwrap();

    EntityLinkSmokeFixture {
        reference,
        target,
        strategy,
        registry,
        gold,
    }
}

#[derive(Debug, Clone, Copy)]
enum EntityLinkRoleFixture {
    Reference,
    Target,
}

fn write_entity_link_rows(
    path: &Path,
    format: EntityLinkFixtureFormat,
    role: EntityLinkRoleFixture,
    matched: bool,
) {
    let target_address = if matched {
        "100 Main St"
    } else {
        "999 Other St"
    };
    match format {
        EntityLinkFixtureFormat::Csv => match role {
            EntityLinkRoleFixture::Reference => write_seed_csv(
                path,
                "loan_id,deal,address,upb,source_row_id,deal_id,property_id,raw_tenant_name\nR-1,D1,100 Main St,100,R-1,D1,1,Reference Name\n",
            ),
            EntityLinkRoleFixture::Target => write_seed_csv(
                path,
                &format!(
                    "deal,loan_number,address,balance,source_row_id,deal_id,property_id,raw_tenant_name,loan_id\nD1,1,{target_address},101,D1|1,D1,1,Target Name,1\n"
                ),
            ),
        },
        EntityLinkFixtureFormat::Tsv => {
            let content = match role {
                EntityLinkRoleFixture::Reference => {
                    "loan_id\tdeal\taddress\tupb\tsource_row_id\tdeal_id\tproperty_id\traw_tenant_name\nR-1\tD1\t100 Main St\t100\tR-1\tD1\t1\tReference Name\n".to_string()
                }
                EntityLinkRoleFixture::Target => format!(
                    "deal\tloan_number\taddress\tbalance\tsource_row_id\tdeal_id\tproperty_id\traw_tenant_name\tloan_id\nD1\t1\t{target_address}\t101\tD1|1\tD1\t1\tTarget Name\t1\n"
                ),
            };
            std::fs::write(path, content).unwrap();
        }
        EntityLinkFixtureFormat::Jsonl | EntityLinkFixtureFormat::Ndjson => {
            let value = match role {
                EntityLinkRoleFixture::Reference => serde_json::json!({
                    "loan_id": "R-1",
                    "deal": "D1",
                    "address": "100 Main St",
                    "upb": 100,
                    "source_row_id": "R-1",
                    "deal_id": "D1",
                    "property_id": "1",
                    "raw_tenant_name": "Reference Name"
                }),
                EntityLinkRoleFixture::Target => serde_json::json!({
                    "deal": "D1",
                    "loan_number": "1",
                    "address": target_address,
                    "balance": 101,
                    "source_row_id": "D1|1",
                    "deal_id": "D1",
                    "property_id": "1",
                    "raw_tenant_name": "Target Name",
                    "loan_id": "1"
                }),
            };
            std::fs::write(
                path,
                format!("{}\n", serde_json::to_string(&value).unwrap()),
            )
            .unwrap();
        }
    }
}

fn write_entity_link_audit_suite(root: &Path) -> PathBuf {
    let suite = root.join("suite");
    std::fs::create_dir_all(&suite).unwrap();
    std::fs::write(
        suite.join("manifest.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "id": "entity_link_smoke_suite",
            "version": "2026.07.11",
            "gates": [
                {
                    "gate_id": "G01",
                    "label": "artifact continuity",
                    "passed": true,
                    "expected": "link_run_artifact_audited",
                    "actual": "link_run_artifact_audited",
                    "evidence": {
                        "contract": "bd-2k28"
                    }
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    suite
}

fn registry_snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut snapshot = BTreeMap::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            snapshot.insert(name, std::fs::read(path).unwrap());
        }
    }
    snapshot
}

fn registry_files(root: &Path) -> BTreeSet<String> {
    registry_snapshot(root).into_keys().collect()
}

fn entity_link_smoke_args<'a>(
    fixture: &'a EntityLinkSmokeFixture,
    work_dir: &'a Path,
) -> Vec<&'a str> {
    entity_link_smoke_args_with_profile(fixture, work_dir, "cmbs_tenant_label")
}

fn entity_link_smoke_args_with_profile<'a>(
    fixture: &'a EntityLinkSmokeFixture,
    work_dir: &'a Path,
    profile: &'a str,
) -> Vec<&'a str> {
    vec![
        "entity",
        "link",
        fixture.reference.to_str().unwrap(),
        fixture.target.to_str().unwrap(),
        "--profile",
        profile,
        "--strategy",
        fixture.strategy.to_str().unwrap(),
        "--registry",
        fixture.registry.to_str().unwrap(),
        "--work-dir",
        work_dir.to_str().unwrap(),
    ]
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

fn canon_args_from_emitted_command_with_suite(command: &str, suite: &Path) -> Vec<String> {
    canon_args_from_emitted_command(&command.replace("<SUITE_DIR>", suite.to_str().unwrap()))
}

fn review_csv_rows(csv: &str) -> Vec<BTreeMap<String, String>> {
    let mut reader = csv::Reader::from_reader(csv.as_bytes());
    let headers = reader.headers().unwrap().clone();
    reader
        .records()
        .map(|record| {
            headers
                .iter()
                .zip(record.unwrap().iter())
                .map(|(header, value)| (header.to_string(), value.to_string()))
                .collect::<BTreeMap<_, _>>()
        })
        .collect()
}

fn review_ids(review: &Value) -> BTreeSet<String> {
    review["review_items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["review_id"].as_str().unwrap().to_string())
        .collect()
}

fn review_priority_reasons(item: &Value) -> Vec<String> {
    item["priority_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|reason| reason.as_str().unwrap().to_string())
        .collect()
}

fn review_json_for_link(link_path: &Path, include: &str) -> Value {
    review_json_for_artifact(link_path, include)
}

fn review_json_for_artifact(artifact_path: &Path, include: &str) -> Value {
    serde_json::from_slice(&review_json_bytes_for_artifact(artifact_path, include)).unwrap()
}

fn review_json_bytes_for_artifact(artifact_path: &Path, include: &str) -> Vec<u8> {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            artifact_path.to_str().unwrap(),
            "--include",
            include,
            "--emit",
            "json",
        ])
        .assert()
        .success();
    output.get_output().stdout.clone()
}

fn review_csv_for_link(link_path: &Path, include: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            link_path.to_str().unwrap(),
            "--include",
            include,
            "--emit",
            "csv",
        ])
        .assert()
        .success();
    String::from_utf8(output.get_output().stdout.clone()).unwrap()
}

fn native_review_json_for_artifact(artifact_path: &Path, include: &str) -> Value {
    serde_json::from_slice(&native_review_json_bytes_for_artifact(
        artifact_path,
        include,
    ))
    .unwrap()
}

fn native_review_json_bytes_for_artifact(artifact_path: &Path, include: &str) -> Vec<u8> {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            artifact_path.to_str().unwrap(),
            "--artifact",
            "native-review",
            "--include",
            include,
            "--emit",
            "json",
        ])
        .assert()
        .success();
    output.get_output().stdout.clone()
}

fn native_review_csv_for_artifact(artifact_path: &Path, include: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            artifact_path.to_str().unwrap(),
            "--artifact",
            "native-review",
            "--include",
            include,
            "--emit",
            "csv",
        ])
        .assert()
        .success();
    String::from_utf8(output.get_output().stdout.clone()).unwrap()
}

fn native_review_html_for_artifact(artifact_path: &Path, include: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            artifact_path.to_str().unwrap(),
            "--artifact",
            "native-review",
            "--include",
            include,
            "--emit",
            "html",
        ])
        .assert()
        .success();
    String::from_utf8(output.get_output().stdout.clone()).unwrap()
}

fn assert_link_review_export_refuses_without_writes(
    artifact_path: &Path,
    link_dir: &Path,
    expected_field: &str,
) {
    let files_before = registry_files(link_dir);
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            artifact_path.to_str().unwrap(),
            "--include",
            "escrow",
            "--emit",
            "json",
        ])
        .assert()
        .code(2);
    let refusal: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(refusal["refusal"]["detail"]["field"], expected_field);
    assert_eq!(refusal["refusal"]["detail"]["writes_performed"], false);
    assert_eq!(registry_files(link_dir), files_before);
}

fn assert_review_csv_id_parity(
    link_path: &Path,
    include: &str,
    review_json: &Value,
) -> Vec<BTreeMap<String, String>> {
    let rows = review_csv_rows(&review_csv_for_link(link_path, include));
    assert_eq!(
        rows.len(),
        review_json["review_items"].as_array().unwrap().len()
    );
    let csv_ids = rows
        .iter()
        .map(|row| row["review_id"].clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(csv_ids, review_ids(review_json));
    rows
}

fn native_review_ids(review: &Value) -> BTreeSet<String> {
    review["review_items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["review_id"].as_str().unwrap().to_string())
        .collect()
}

fn assert_native_review_csv_id_parity(review_json: &Value, review_csv: &str) {
    let rows = review_csv_rows(review_csv);
    assert_eq!(
        rows.len(),
        review_json["review_items"].as_array().unwrap().len()
    );
    let csv_ids = rows
        .iter()
        .map(|row| row["review_id"].clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(csv_ids, native_review_ids(review_json));
    for row in rows {
        assert_eq!(
            row["source_review_artifact_hash"],
            review_json["artifact_content_hash"].as_str().unwrap()
        );
        assert_eq!(
            row["run_content_hash"],
            review_json["binding"]["run_content_hash"].as_str().unwrap()
        );
    }
}

fn assert_native_review_html_offline(html: &str, review_json: &Value) {
    assert!(html.contains("<!doctype html>"));
    assert!(html.contains("Canon Entity Review"));
    assert!(html.contains("id=\"review-data\""));
    assert!(html.contains("data-mode=\"link\""));
    assert!(html.contains(review_json["artifact_content_hash"].as_str().unwrap()));
    assert!(!html.contains("__CANON_NATIVE_REVIEW_JSON__"));
    assert!(!html.contains("http://"));
    assert!(!html.contains("https://"));
    assert!(!html.contains("fetch("));
    assert!(!html.contains("XMLHttpRequest"));
}

fn native_defer_decision(review: &Value, item: &Value) -> Value {
    let mode = item["mode"].as_str().unwrap();
    assert!(matches!(mode, "cluster" | "link"));
    serde_json::json!({
        "review_id": item["review_id"],
        "mode": mode,
        "action": "defer",
        "operator_id": "cli-smoke",
        "reason_code": "bd_14m6_acceptance",
        "source_review_artifact_hash": review["artifact_content_hash"],
        "decision_binding_hash": item["decision_binding_hash"],
        "run_content_hash": review["binding"]["run_content_hash"],
        "policy_content_hash": review["binding"]["policy_content_hash"],
        "registry_snapshot_hash": review["binding"]["registry_snapshot_hash"],
        "mode_context": item["mode_context"].clone()
    })
}

fn write_native_review_decisions(path: &Path, decisions: &[Value]) {
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&serde_json::json!({ "decisions": decisions })).unwrap(),
    )
    .unwrap();
}

fn assert_native_review_import_refuses_without_registry_mutation(
    decisions_path: &Path,
    source_review_path: &Path,
    registry: &Path,
    expected_field: &str,
) {
    let registry_before = registry_snapshot(registry);
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "import",
            decisions_path.to_str().unwrap(),
            "--registry",
            registry.to_str().unwrap(),
            "--next-version",
            "0.1.1",
            "--source-review",
            source_review_path.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .code(2);
    let refusal: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_REVIEW_IMPORT");
    assert_eq!(refusal["refusal"]["detail"]["writes_performed"], false);
    assert_eq!(refusal["refusal"]["detail"]["field"], expected_field);
    assert_eq!(registry_snapshot(registry), registry_before);
}

fn run_neutral_mixed_link_artifacts(
    root: &Path,
) -> (EntityLinkSmokeFixture, PathBuf, PathBuf, PathBuf, Value) {
    let fixture = write_entity_link_neutral_mixed_fixture(root);
    let work_dir = root.join("neutral-link-work");
    let mut args = entity_link_smoke_args_with_profile(&fixture, &work_dir, "regab_firm_identity");
    args.push("--no-witness");
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .assert()
        .code(1);

    let run_path = work_dir.join("run/run.json");
    let solve_path = work_dir.join("solve/solve.json");
    let link_path = work_dir.join("link/link.json");
    let link_artifact: Value = serde_json::from_slice(&std::fs::read(&link_path).unwrap()).unwrap();
    (fixture, run_path, solve_path, link_path, link_artifact)
}

fn run_neutral_mixed_link(root: &Path) -> (PathBuf, Value) {
    let (_, _, _, link_path, link_artifact) = run_neutral_mixed_link_artifacts(root);
    (link_path, link_artifact)
}

fn resealed_typed_link_artifact(artifact: &Value) -> canon::entity::run::link::EntityLinkArtifact {
    let mut hashable: canon::entity::run::link::EntityLinkArtifact =
        serde_json::from_value(artifact.clone()).unwrap();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let hash = canon::witness::hash_bytes(&serde_json::to_vec(&hashable).unwrap());
    hashable.artifact_content_hash = hash.clone();
    hashable.metadata.artifact_content_hash = hash;
    hashable
}

fn write_native_solve_with_escrow(path: &Path) -> Value {
    let edge = build_edge_evidence_record(
        "surf:alpha",
        "surf:alpha_alias",
        vec![EdgeEvidenceHit::new(
            ScoreLane::Support,
            "name",
            "string_similarity",
            "weak_positive_identity_evidence",
            score_units(1_000),
            false,
            "weak positive identity evidence below solve threshold",
        )],
    )
    .expect("edge evidence builds");
    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records: vec![edge],
        exact_bucket_assertions: Vec::new(),
        incumbent_ids: Vec::new(),
    })
    .expect("signed graph builds");
    let solve = build_solve_artifact_contract(SolveArtifactRequest {
        metadata: native_solve_metadata(),
        graph,
        config: SolveReconciliationConfig::delegate_new_ids(score_units(5_000)),
        provenance: vec![
            SolveSurfaceProvenance {
                surface_id: "surf:alpha".to_string(),
                row_count: 3,
                deal_count: 1,
            },
            SolveSurfaceProvenance {
                surface_id: "surf:alpha_alias".to_string(),
                row_count: 2,
                deal_count: 1,
            },
        ],
        decision_ledger_path: "solve/decision-ledger.jsonl".to_string(),
    })
    .expect("solve artifact builds");
    let mut value = serde_json::to_value(&solve).expect("solve artifact serializes");
    let solve_contract = canon::entity::schema::entity_v1_contract_for_stage(
        canon::entity::EntityArtifactStageV1::Solve,
    )
    .expect("solve contract");
    let block_contract =
        canon::entity::entity_artifact_v1_contract_for_version("canon_entity_block.v1")
            .expect("block contract");
    let evidence_contract =
        canon::entity::entity_artifact_v1_contract_for_version("canon_entity_evidence.v1")
            .expect("evidence contract");
    value["metadata"]["schema"] = serde_json::to_value(
        canon::entity::schema::entity_v1_schema_reference(solve_contract).expect("solve schema"),
    )
    .unwrap();
    value["metadata"]["workdir"] = serde_json::to_value(
        canon::entity::schema::entity_v1_workdir_layout(solve_contract, "native-solve-work"),
    )
    .unwrap();
    value["metadata"]["upstream_artifacts"] = serde_json::json!([
        {
            "version": "canon_entity_block.v1",
            "schema_key": "canon_entity_block.v1",
            "schema_hash": canon::entity::schema::entity_v1_schema_content_hash(block_contract)
                .expect("block schema hash"),
            "content_hash": "blake3:block"
        },
        {
            "version": "canon_entity_evidence.v1",
            "schema_key": "canon_entity_evidence.v1",
            "schema_hash": canon::entity::schema::entity_v1_schema_content_hash(evidence_contract)
                .expect("evidence schema hash"),
            "content_hash": "blake3:edge"
        }
    ]);
    canon::entity::schema::finalize_entity_v1_self_hash(&mut value).expect("solve self hash");
    std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    value
}

fn native_solve_metadata() -> EntityArtifactMetadata {
    EntityArtifactMetadata {
        profile: EntityProfileReference {
            id: "neutral_org_identity".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "organization".to_string(),
            identity_semantics: "canonical_organization_identity".to_string(),
            canonical_type: "organization".to_string(),
            patch_namespaces: EntityPatchNamespaces {
                aliases: "neutral_org_identity.aliases".to_string(),
                distinct: "neutral_org_identity.distinct".to_string(),
                relations: "neutral_org_identity.relations".to_string(),
            },
            content_hash: Some("blake3:profile".to_string()),
        },
        strategy: EntityStrategyReference {
            id: "neutral_org_identity.v1".to_string(),
            version: "0.1.0".to_string(),
            content_hash: "blake3:strategy".to_string(),
        },
        registry_snapshot: EntityRegistrySnapshot {
            id: "neutral-orgs".to_string(),
            version: "2026.06.25".to_string(),
            source: "registries/neutral-orgs".to_string(),
            lookup_snapshot_hash: "blake3:registry".to_string(),
            sidecar_snapshot_hash: Some("blake3:sidecars".to_string()),
        },
        patch_namespace: "neutral_org_identity.aliases".to_string(),
        input: Some(EntityInputReference {
            row_count: 5,
            content_hash: "blake3:input".to_string(),
        }),
        upstream_artifacts: vec![
            EntityArtifactReference {
                version: "canon_entity_block.v1".to_string(),
                content_hash: "blake3:block".to_string(),
            },
            EntityArtifactReference {
                version: "canon_entity_evidence.v1".to_string(),
                content_hash: "blake3:edge".to_string(),
            },
        ],
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}

fn score_units(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}

type RecordedOpenFigiRequest = (String, BTreeMap<String, String>);
type OpenFigiServerHandle = thread::JoinHandle<RecordedOpenFigiRequest>;

fn spawn_openfigi_server(response_body: String) -> (String, OpenFigiServerHandle) {
    let server = Server::http("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}/v3/mapping", server.server_addr());
    let handle = thread::spawn(move || {
        let mut request = server.recv().unwrap();
        let headers = request
            .headers()
            .iter()
            .map(|header| {
                (
                    header.field.as_str().to_string().to_ascii_lowercase(),
                    header.value.as_str().to_string(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut body = String::new();
        request.as_reader().read_to_string(&mut body).unwrap();
        let response = Response::from_string(response_body)
            .with_status_code(StatusCode(200))
            .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
        request.respond(response).unwrap();
        (body, headers)
    });

    (base_url, handle)
}

#[test]
fn test_version_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--version")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert_eq!(
        stdout.trim(),
        format!("canon {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn test_bare_invocation_prints_orientation() {
    // Bare `canon` orients the caller toward the canonical command and the
    // machine-readable surfaces instead of emitting a raw clap error. Exit 2
    // (no task performed), guidance on stderr, stdout clean for pipelines.
    let assert = Command::new(env!("CARGO_BIN_EXE_canon"))
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty());
    let output = assert.get_output();
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(stderr.contains("--registry"));
    assert!(stderr.contains("canon doctor --robot-triage"));
    assert!(stderr.contains("canon --describe"));
}

#[test]
fn entity_namespace_cli() {
    let help = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--help")
        .assert()
        .success();
    let help_stdout = String::from_utf8(help.get_output().stdout.clone()).unwrap();
    assert!(help_stdout.contains("entity"));
    assert!(!help_stdout.contains("\n  org"));

    let entity_help = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["entity", "--help"])
        .assert()
        .success();
    let normalize_help = |text: &str| {
        text.lines()
            .map(|line| line.trim_end_matches([' ', '\t']))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let entity_help_stdout = String::from_utf8(entity_help.get_output().stdout.clone()).unwrap();
    assert!(entity_help_stdout.contains("run"));
    assert!(entity_help_stdout.contains("candidate-recall"));
    assert!(entity_help_stdout.contains("alias-withholding"));
    assert!(entity_help_stdout.contains("generalization"));
    assert!(entity_help_stdout.contains("evidence"));
    assert!(entity_help_stdout.contains("link"));
    assert!(!entity_help_stdout.contains("edge"));
    assert!(!entity_help_stdout.contains("org"));
    let expected_entity_help =
        std::fs::read_to_string(fixture_path("tests/fixtures/canon_v1/help/entity_help.txt"))
            .unwrap();
    assert_eq!(
        normalize_help(&entity_help_stdout),
        normalize_help(&expected_entity_help)
    );

    let run_help = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["entity", "run", "--help"])
        .assert()
        .success();
    let run_help_stdout = String::from_utf8(run_help.get_output().stdout.clone()).unwrap();
    assert!(run_help_stdout.contains("--cache-mode <CACHE_MODE>"));
    assert!(run_help_stdout.contains("default: enabled"));
    assert!(run_help_stdout.contains("disabled"));

    let link_help = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["entity", "link", "--help"])
        .assert()
        .success();
    let link_help_stdout = String::from_utf8(link_help.get_output().stdout.clone()).unwrap();
    assert!(link_help_stdout.contains("--cache-mode <CACHE_MODE>"));
    assert!(link_help_stdout.contains("default: enabled"));
    assert!(link_help_stdout.contains("disabled"));
    let expected_link_help = std::fs::read_to_string(fixture_path(
        "tests/fixtures/canon_v1/help/entity_link_help.txt",
    ))
    .unwrap();
    assert_eq!(
        normalize_help(&link_help_stdout),
        normalize_help(&expected_link_help)
    );

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["entity", "edge", "--help"])
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("unrecognized subcommand 'edge'"))
        .stderr(predicate::str::contains("evidence"));

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["org", "run", "--help"])
        .assert()
        .failure();

    let describe = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--describe")
        .assert()
        .success();
    let describe_stdout = String::from_utf8(describe.get_output().stdout.clone()).unwrap();
    let describe_json: Value =
        serde_json::from_str(&describe_stdout).expect("--describe should output valid JSON");
    // Regression guard: the embedded operator.json contract version must track the
    // crate version. A release bump that updates Cargo.toml but forgets operator.json
    // would otherwise ship a stale contract (and fail `doctor health`).
    assert_eq!(
        describe_json["version"].as_str(),
        Some(env!("CARGO_PKG_VERSION")),
        "operator.json version must match the crate version; bump operator.json when bumping Cargo.toml"
    );
    let usage = describe_json["invocation"]["usage"]
        .as_array()
        .expect("describe invocation usage should be an array");
    assert!(usage.iter().any(|entry| {
        entry
            .as_str()
            .is_some_and(|usage| usage.starts_with("canon entity run"))
    }));
    assert!(usage.iter().any(|entry| {
        entry
            .as_str()
            .is_some_and(|usage| usage.starts_with("canon entity evidence"))
    }));
    assert!(usage.iter().any(|entry| {
        entry
            .as_str()
            .is_some_and(|usage| usage.starts_with("canon entity generalization"))
    }));
    assert!(!usage.iter().any(|entry| {
        entry
            .as_str()
            .is_some_and(|usage| usage.starts_with("canon entity edge"))
    }));
    assert!(!usage.iter().any(|entry| {
        entry
            .as_str()
            .is_some_and(|usage| usage.starts_with("canon org"))
    }));

    let subcommands = describe_json["subcommands"]
        .as_array()
        .expect("describe subcommands should be an array");
    assert!(
        subcommands.iter().any(|entry| entry["name"] == "entity run"
            && entry["output_schema"] == "canon_entity_run.v0")
    );
    assert!(
        subcommands
            .iter()
            .any(|entry| entry["name"] == "entity evidence"
                && entry["output_schema"] == "canon_entity_evidence.v1")
    );
    assert!(
        subcommands
            .iter()
            .any(|entry| entry["name"] == "entity generalization"
                && entry["output_schema"] == "canon.evaluation.generalization.v1")
    );
    assert!(
        !subcommands
            .iter()
            .any(|entry| entry["name"] == "entity edge")
    );
    assert!(!subcommands.iter().any(|entry| {
        entry["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("org "))
            || entry["output_schema"]
                .as_str()
                .is_some_and(|schema| schema.starts_with("canon_org_"))
    }));
}

#[test]
fn entity_cutover_dispatch_smoke() {
    let omitted_profile = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "run",
            "rows.csv",
            "--strategy",
            "strategy.yaml",
            "--registry",
            "registry",
            "--emit",
            "json",
        ])
        .assert()
        .code(2);
    let stdout = String::from_utf8(omitted_profile.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_ENTITY_INPUT_CONTRACT");
    assert_eq!(
        payload["refusal"]["detail"]["reason"],
        "legacy_dispatch_removed"
    );
    assert_eq!(
        payload["refusal"]["detail"]["legacy_dispatch_allowed"],
        false
    );

    let link = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "link",
            "reference.csv",
            "target.csv",
            "--profile",
            "entity_profile",
            "--strategy",
            "strategy.yaml",
            "--registry",
            "registry",
            "--work-dir",
            "work/entity-link",
            "--emit",
            "json",
        ])
        .assert()
        .code(2);
    let stdout = String::from_utf8(link.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_ENTITY_INPUT_CONTRACT");
    let detail = &payload["refusal"]["detail"];
    assert_eq!(detail["stage"], "link");
    assert_eq!(detail["role"], "reference");
    assert_eq!(detail["path"], "reference.csv");
    assert_eq!(detail["writes_performed"], false);
}

#[test]
fn entity_link_shorthand_matches_project_dispatch_artifacts() {
    let link_request: canon::entity::runtime::EntityV1ProjectDispatchRequest =
        canon::entity::runtime::EntityV1LinkDispatchRequest {
            reference: PathBuf::from("reference.csv"),
            target: PathBuf::from("target.csv"),
            profile: "entity_profile".to_string(),
            strategy: PathBuf::from("strategy.yaml"),
            registry: PathBuf::from("registry"),
            work_dir: PathBuf::from("work/entity-link"),
            suite: None,
        }
        .into();

    let project_request = canon::entity::runtime::EntityV1ProjectDispatchRequest {
        mode: canon::entity::runtime::EntityV1DispatchMode::TwoSourceLink,
        rows: None,
        reference: Some(PathBuf::from("reference.csv")),
        target: Some(PathBuf::from("target.csv")),
        profile: "entity_profile".to_string(),
        strategy: PathBuf::from("strategy.yaml"),
        registry: PathBuf::from("registry"),
        work_dir: PathBuf::from("work/entity-link"),
        suite: None,
    };

    assert_eq!(link_request, project_request);
    let link_plan = canon::entity::runtime::entity_v1_dispatch_plan(
        canon::entity::EntityArtifactStageV1::Run,
        &link_request,
    );
    let project_plan = canon::entity::runtime::entity_v1_dispatch_plan(
        canon::entity::EntityArtifactStageV1::Run,
        &project_request,
    );
    assert_eq!(link_plan.artifacts, project_plan.artifacts);
    assert_eq!(
        link_plan.requested_stage,
        canon::entity::EntityArtifactStageV1::Run
    );
    assert_eq!(
        link_plan.requested_artifact().unwrap().artifact_path,
        PathBuf::from("work/entity-link/run/run.json")
    );
}

#[test]
fn entity_namespace_internal() {
    assert_eq!(
        canon::entity::runtime::types::CANON_ENTITY_RUN_VERSION,
        "canon_entity_run.v0"
    );
    assert_eq!(
        canon::entity::runtime::types::CANON_ENTITY_SOLVE_VERSION,
        "canon_entity_solve.v0"
    );

    let strategy_type = std::any::type_name::<canon::entity::runtime::types::EntityStrategy>();
    assert!(strategy_type.contains("entity::runtime::types::EntityStrategy"));
    assert!(!strategy_type.contains(concat!("Org", "Strategy")));

    let error_type = std::any::type_name::<canon::entity::runtime::types::EntityError>();
    assert!(error_type.contains("entity::runtime::types::EntityError"));
    assert!(!error_type.contains(concat!("Org", "Error")));
}

fn json_fixture(relative: &str) -> Value {
    let raw = std::fs::read_to_string(fixture_path(relative)).unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn run_exact_lookup_json(input: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg(input)
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("json")
        .arg("--explicit")
        .assert()
        .success();

    String::from_utf8(output.get_output().stdout.clone()).unwrap()
}

#[test]
fn exact_match_entity_namespace_golden_bytes_stay_stable() {
    let expected = json_fixture("tests/fixtures/golden/all_resolved.json");

    let csv_first = run_exact_lookup_json("tests/fixtures/inputs/all_resolved.csv");
    let csv_second = run_exact_lookup_json("tests/fixtures/inputs/all_resolved.csv");
    let csv_value: Value = serde_json::from_str(&csv_first).unwrap();
    assert_eq!(csv_value, expected);
    assert_eq!(csv_second, csv_first);

    let jsonl_first = run_exact_lookup_json("tests/fixtures/inputs/basic.jsonl");
    let jsonl_second = run_exact_lookup_json("tests/fixtures/inputs/basic.jsonl");
    let jsonl_value: Value = serde_json::from_str(&jsonl_first).unwrap();
    assert_eq!(jsonl_value, expected);
    assert_eq!(jsonl_second, jsonl_first);
}

#[test]
fn exact_lookup_regression_after_entity_namespace() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("json")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&stdout).expect("exact lookup emits valid JSON");
    assert_eq!(json["version"], "canon.v0");
    assert_eq!(json["outcome"], "RESOLVED");
    assert_eq!(json["summary"]["total"], 3);
    assert_eq!(json["summary"]["resolved"], 3);
    assert_eq!(json["summary"]["unresolved"], 0);
    assert_eq!(json["mappings"].as_array().unwrap().len(), 3);
}

#[test]
fn test_describe_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--describe")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&stdout).expect("--describe should output valid JSON");

    assert_eq!(json["name"], "canon");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(json["schema_version"], "operator.v0");
    assert!(json["capabilities"].is_object());
    assert!(
        json["subcommands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == "entity link"
                && entry["output_schema"] == "canon_entity_link.v0"
                && entry["status"] == "implemented")
    );
    assert!(
        !json["subcommands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == "resolve")
    );
    assert!(
        json["subcommands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == "doctor"
                && entry["status"] == "implemented"
                && entry["read_only"] == true)
    );
}

#[test]
fn test_doctor_health_json_is_read_only() {
    let temp_dir = tempdir().unwrap();
    let witness_path = temp_dir.path().join("witness").join("canon-witness.jsonl");

    let output = doctor_cmd_in(temp_dir.path(), &witness_path)
        .args(["doctor", "health", "--json"])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["schema"], "canon.doctor.health.v1");
    assert_eq!(payload["contract"], "cmdrvl.read_only_doctor.v1");
    assert_eq!(payload["tool"], "canon");
    assert_eq!(payload["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["read_only"], true);
    assert_eq!(payload["summary"]["checks_failed"], 0);
    assert_eq!(
        payload["config_footprint"]["managed_state_paths"][0],
        "~/.cmdrvl/state/witness/witness.jsonl"
    );
    assert_eq!(
        payload["config_footprint"]["legacy_migration_required"],
        true
    );
    assert_eq!(payload["config_footprint"]["self_contained"], true);
    assert_eq!(
        payload["observed_paths"]["witness_ledger"],
        witness_path.display().to_string()
    );
    assert_eq!(payload["side_effects"]["opens_witness_ledger"], false);
    assert_eq!(payload["side_effects"]["appends_witness_ledger"], false);
    assert_eq!(payload["side_effects"]["creates_witness_directory"], false);
    assert_all_side_effects_false(&payload["side_effects"]);
    assert!(payload["fixers"].as_array().unwrap().is_empty());
    assert_doctor_side_effects_absent(temp_dir.path(), &witness_path);
}

#[test]
fn test_doctor_capabilities_json_has_no_fixers_or_side_effects() {
    let temp_dir = tempdir().unwrap();
    let witness_path = temp_dir.path().join("witness").join("canon-witness.jsonl");

    let output = doctor_cmd_in(temp_dir.path(), &witness_path)
        .args(["doctor", "capabilities", "--json"])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["schema"], "canon.doctor.capabilities.v1");
    assert_eq!(payload["contract"], "cmdrvl.read_only_doctor.v1");
    assert_eq!(payload["read_only"], true);
    assert_eq!(
        payload["config_footprint"]["deprecation_notices"],
        "~/.cmdrvl/notices/deprecated-paths.jsonl"
    );
    assert_all_side_effects_false(&payload["side_effects"]);
    assert!(payload["fixers"].as_array().unwrap().is_empty());
    assert!(
        payload["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|command| command["name"] == "robot-triage"
                && command["usage"] == "canon doctor --robot-triage")
    );
    assert_eq!(payload["composition"]["family"]["name"], "cmdrvl-spine");
    assert_eq!(
        payload["composition"]["role"],
        "canonical identifier normalization before structural checks and reconciliation"
    );
    assert!(
        payload["composition"]["canonical_chain"][0]
            .as_str()
            .is_some_and(|command| command.contains("canon old.csv --registry <REGISTRY>"))
    );
    assert!(
        payload["composition"]["canonical_chain"][2]
            .as_str()
            .is_some_and(|command| command.contains("shape old.canon.csv new.canon.csv"))
    );
    assert_doctor_side_effects_absent(temp_dir.path(), &witness_path);
}

#[test]
fn test_doctor_robot_triage_json_is_machine_readable() {
    let temp_dir = tempdir().unwrap();
    let witness_path = temp_dir.path().join("witness").join("canon-witness.jsonl");

    let output = doctor_cmd_in(temp_dir.path(), &witness_path)
        .args(["doctor", "--robot-triage"])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["schema"], "canon.doctor.triage.v1");
    assert_eq!(payload["contract"], "cmdrvl.read_only_doctor.v1");
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["score"], 100);
    assert_eq!(payload["read_only"], true);
    assert_eq!(
        payload["config_footprint"]["migration_policy"],
        "copy-only legacy witness migration; never delete or move legacy files; never record file contents or secret values"
    );
    assert_all_side_effects_false(&payload["side_effects"]);
    assert_doctor_side_effects_absent(temp_dir.path(), &witness_path);
}

#[test]
fn test_doctor_robot_docs_is_plain_text_and_read_only() {
    let temp_dir = tempdir().unwrap();
    let witness_path = temp_dir.path().join("witness").join("canon-witness.jsonl");

    doctor_cmd_in(temp_dir.path(), &witness_path)
        .args(["doctor", "robot-docs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cmdrvl.read_only_doctor.v1"))
        .stdout(predicate::str::contains("canon doctor health --json"))
        .stdout(predicate::str::contains("composition:"))
        .stdout(predicate::str::contains(
            "shape old.canon.csv new.canon.csv",
        ))
        .stdout(predicate::str::contains("rvl old.canon.csv new.canon.csv"))
        .stdout(predicate::str::contains("no --fix surface"));

    assert_doctor_side_effects_absent(temp_dir.path(), &witness_path);
}

#[test]
fn test_doctor_help_is_available() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["doctor", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("health"))
        .stdout(predicate::str::contains("capabilities"))
        .stdout(predicate::str::contains("robot-docs"))
        .stdout(predicate::str::contains("--robot-triage"));
}

#[test]
fn test_doctor_fix_is_not_available() {
    let temp_dir = tempdir().unwrap();
    let witness_path = temp_dir.path().join("witness").join("canon-witness.jsonl");

    doctor_cmd_in(temp_dir.path(), &witness_path)
        .args(["doctor", "--fix"])
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("--fix"));

    assert_doctor_side_effects_absent(temp_dir.path(), &witness_path);
}

#[test]
fn test_entity_link_cli_success_json() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_entity_link_smoke_fixture(temp_dir.path(), true);
    let work_dir = temp_dir.path().join("entity-link-work");
    let mut args = entity_link_smoke_args(&fixture, &work_dir);
    args.push("--no-witness");

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    let decisions = &payload["decision_artifact"];
    assert_eq!(payload["version"], "canon_entity_link.v0");
    assert_eq!(payload["summary"]["target_records"], 1);
    assert_eq!(payload["summary"]["matched"], 1);
    assert_eq!(payload["summary"]["unmatched"], 0);
    assert_eq!(payload["summary"]["ambiguous"], 0);
    assert_eq!(decisions["version"], "canon_entity_link_decisions.v0");
    assert_eq!(decisions["matches"][0]["reference_id"], "R-1");
    assert_eq!(decisions["matches"][0]["target_id"], "D1|1");
}

#[test]
fn test_entity_cache_mode_disabled_reaches_native_run_and_link() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_entity_link_smoke_fixture(temp_dir.path(), true);
    let run_work_dir = temp_dir.path().join("entity-run-work");

    let run_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "run",
            fixture.reference.to_str().unwrap(),
            "--profile",
            "cmbs_tenant_label",
            "--strategy",
            fixture.strategy.to_str().unwrap(),
            "--registry",
            fixture.registry.to_str().unwrap(),
            "--work-dir",
            run_work_dir.to_str().unwrap(),
            "--cache-mode",
            "disabled",
            "--no-witness",
        ])
        .assert()
        .success();
    let run_payload: Value = serde_json::from_slice(&run_output.get_output().stdout).unwrap();
    assert_eq!(run_payload["summary"]["labels"]["cache_mode"], "disabled");
    assert_eq!(run_payload["summary"]["labels"]["cache_status"], "bypassed");
    assert!(
        run_payload["stage_artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|stage| stage["stage"] == "cache_disabled")
    );

    let link_work_dir = temp_dir.path().join("entity-link-work");
    let mut link_args = entity_link_smoke_args(&fixture, &link_work_dir);
    link_args.extend(["--cache-mode", "disabled", "--no-witness"]);
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(link_args)
        .assert()
        .success();

    let linked_run: Value =
        serde_json::from_slice(&std::fs::read(link_work_dir.join("run/run.json")).unwrap())
            .unwrap();
    let linked_solve: Value =
        serde_json::from_slice(&std::fs::read(link_work_dir.join("solve/solve.json")).unwrap())
            .unwrap();
    let link_artifact: Value =
        serde_json::from_slice(&std::fs::read(link_work_dir.join("link/link.json")).unwrap())
            .unwrap();
    assert_eq!(linked_run["summary"]["labels"]["cache_mode"], "disabled");
    assert_eq!(linked_run["summary"]["labels"]["cache_status"], "bypassed");
    assert!(
        linked_run["stage_artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|stage| stage["stage"] == "cache_disabled")
    );
    assert_eq!(link_artifact["version"], "canon_entity_link.v0");
    assert_eq!(
        link_artifact["shared_solve_artifact"]["version"],
        linked_solve["version"]
    );
    assert_eq!(
        link_artifact["shared_solve_artifact"]["content_hash"],
        linked_solve["artifact_content_hash"]
    );
}

#[test]
fn test_entity_link_cli_suite_writes_stable_audit_artifact() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_entity_link_smoke_fixture(temp_dir.path(), true);
    let suite = write_entity_link_audit_suite(temp_dir.path());
    let work_dir = temp_dir.path().join("entity-link-work");
    let mut args = entity_link_smoke_args(&fixture, &work_dir);
    args.extend(["--suite", suite.to_str().unwrap(), "--no-witness"]);

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    let audit_receipt = &payload["audit_artifact"];
    assert_eq!(
        audit_receipt["path"],
        work_dir.join("audit.json").display().to_string()
    );
    assert_eq!(audit_receipt["version"], "canon_entity_audit.v0");
    assert_eq!(audit_receipt["suite"]["id"], "entity_link_smoke_suite");
    assert_eq!(audit_receipt["status"], "passed");

    let audit_artifact: Value =
        serde_json::from_slice(&std::fs::read(work_dir.join("audit.json")).unwrap()).unwrap();
    assert_eq!(audit_artifact["version"], "canon_entity_audit.v0");
    assert_eq!(audit_artifact["suite_id"], "entity_link_smoke_suite");
    assert_eq!(
        audit_artifact["audited_artifact"]["version"],
        "canon_entity_run.v1"
    );
}

#[test]
fn test_entity_link_emitted_native_handoffs_execute() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_entity_link_smoke_fixture(temp_dir.path(), false);
    let suite = write_entity_link_audit_suite(temp_dir.path());
    let work_dir = temp_dir.path().join("entity-link-work");
    let mut args = entity_link_smoke_args(&fixture, &work_dir);
    args.extend(["--suite", suite.to_str().unwrap(), "--no-witness"]);

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .assert()
        .code(1);

    let run_path = work_dir.join("run/run.json");
    let solve_path = work_dir.join("solve/solve.json");
    let link_path = work_dir.join("link/link.json");
    let run_artifact: Value = serde_json::from_slice(&std::fs::read(&run_path).unwrap()).unwrap();
    let solve_artifact: Value =
        serde_json::from_slice(&std::fs::read(&solve_path).unwrap()).unwrap();
    let link_artifact: Value = serde_json::from_slice(&std::fs::read(&link_path).unwrap()).unwrap();
    assert_eq!(run_artifact["version"], "canon_entity_run.v1");
    assert_eq!(solve_artifact["version"], "canon_entity_solve.v1");
    assert_eq!(link_artifact["version"], "canon_entity_link.v0");
    assert_eq!(
        link_artifact["shared_solve_artifact"]["content_hash"],
        solve_artifact["artifact_content_hash"]
    );
    assert!(
        link_path
            .parent()
            .unwrap()
            .join(link_artifact["materialized_rows_path"].as_str().unwrap())
            .exists()
    );

    let emitted_audit = run_artifact["next_commands"]["audit"]
        .as_str()
        .expect("audit handoff command");
    assert!(emitted_audit.contains(solve_path.to_str().unwrap()));
    let solve_audit_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(canon_args_from_emitted_command_with_suite(
            emitted_audit,
            &suite,
        ))
        .assert()
        .success();
    let solve_audit: Value =
        serde_json::from_slice(&solve_audit_output.get_output().stdout).unwrap();
    assert_eq!(solve_audit["version"], "canon_entity_audit.v1");
    assert_eq!(
        solve_audit["audited_artifact"]["version"],
        "canon_entity_solve.v1"
    );
    assert!(
        solve_audit["metadata"]["upstream_artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|artifact| artifact["version"] == "canon_entity_solve.v1"
                && artifact["content_hash"] == solve_artifact["artifact_content_hash"])
    );

    let run_audit_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "audit",
            run_path.to_str().unwrap(),
            "--suite",
            suite.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let run_audit: Value = serde_json::from_slice(&run_audit_output.get_output().stdout).unwrap();
    assert_eq!(run_audit["version"], "canon_entity_audit.v1");
    assert_eq!(
        run_audit["audited_artifact"]["version"],
        "canon_entity_run.v1"
    );
    assert!(
        run_audit["metadata"]["upstream_artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|artifact| artifact["version"] == "canon_entity_run.v1"
                && artifact["content_hash"] == run_artifact["artifact_content_hash"])
    );

    let emitted_review = run_artifact["next_commands"]["review_export"]
        .as_str()
        .expect("review export handoff command");
    assert!(emitted_review.contains(solve_path.to_str().unwrap()));
    let review_csv_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(canon_args_from_emitted_command(emitted_review))
        .assert()
        .success();
    let review_csv = String::from_utf8(review_csv_output.get_output().stdout.clone()).unwrap();
    let mut reader = csv::Reader::from_reader(review_csv.as_bytes());
    assert!(
        reader
            .headers()
            .unwrap()
            .iter()
            .any(|header| header == "review_id")
    );

    let review_json_command = emitted_review.replace("--emit csv", "--emit json");
    let review_json_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(canon_args_from_emitted_command(&review_json_command))
        .assert()
        .success();
    let review_json: Value =
        serde_json::from_slice(&review_json_output.get_output().stdout).unwrap();
    assert_eq!(review_json["version"], "canon_entity_review.v1");
    assert!(review_json["review_items"].is_array());

    let emitted_link_review = link_artifact["next_commands"]["review_export"]
        .as_str()
        .expect("link review export handoff command");
    assert!(emitted_link_review.contains(link_path.to_str().unwrap()));
    let link_review_csv_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(canon_args_from_emitted_command(emitted_link_review))
        .assert()
        .success();
    let link_review_csv =
        String::from_utf8(link_review_csv_output.get_output().stdout.clone()).unwrap();
    let mut link_reader = csv::Reader::from_reader(link_review_csv.as_bytes());
    let link_csv_ids = link_reader
        .records()
        .map(|record| record.unwrap()[0].to_string())
        .collect::<Vec<_>>();
    assert!(!link_csv_ids.is_empty());

    let link_review_json_command = emitted_link_review.replace("--emit csv", "--emit json");
    let link_review_json_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(canon_args_from_emitted_command(&link_review_json_command))
        .assert()
        .success();
    let link_review_json: Value =
        serde_json::from_slice(&link_review_json_output.get_output().stdout).unwrap();
    assert_eq!(link_review_json["version"], "canon_entity_review_queue.v0");
    assert_eq!(
        link_review_json["source_link_hash"],
        link_artifact["artifact_content_hash"]
    );
    assert_eq!(
        link_review_json["source_solve_hash"],
        solve_artifact["artifact_content_hash"]
    );
    let link_json_ids = link_review_json["review_items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["review_id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(link_csv_ids, link_json_ids);

    let escrow_solve_path = temp_dir.path().join("native-solve-with-escrow.json");
    let escrow_solve = write_native_solve_with_escrow(&escrow_solve_path);
    let escrow_review_json_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            escrow_solve_path.to_str().unwrap(),
            "--include",
            "escrow",
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let escrow_review_json: Value =
        serde_json::from_slice(&escrow_review_json_output.get_output().stdout).unwrap();
    assert_eq!(escrow_review_json["version"], "canon_entity_review.v1");
    assert_eq!(
        escrow_review_json["source_result"]["content_hash"],
        escrow_solve["artifact_content_hash"]
    );
    assert!(escrow_review_json.get("source_link_hash").is_none());
    let escrow_items = escrow_review_json["review_items"].as_array().unwrap();
    assert!(!escrow_items.is_empty());
    assert_eq!(escrow_items[0]["state"], "escrow");

    let escrow_review_csv_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            escrow_solve_path.to_str().unwrap(),
            "--include",
            "escrow",
            "--emit",
            "csv",
        ])
        .assert()
        .success();
    let escrow_review_csv =
        String::from_utf8(escrow_review_csv_output.get_output().stdout.clone()).unwrap();
    let mut escrow_reader = csv::Reader::from_reader(escrow_review_csv.as_bytes());
    assert!(escrow_reader.records().count() >= 1);

    let run_review_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            run_path.to_str().unwrap(),
            "--include",
            "escrow",
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let run_review: Value = serde_json::from_slice(&run_review_output.get_output().stdout).unwrap();
    assert_eq!(run_review["version"], "canon_entity_review.v1");
    assert_eq!(
        run_review["source_result"]["content_hash"],
        run_artifact["artifact_content_hash"]
    );
}

#[test]
fn test_entity_link_neutral_mixed_review_queue_contract() {
    let first_temp = tempdir().unwrap();
    let (link_path, link_artifact) = run_neutral_mixed_link(first_temp.path());

    assert_eq!(link_artifact["version"], "canon_entity_link.v0");
    assert_eq!(link_artifact["summary"]["target_records"], 3);
    assert_eq!(link_artifact["summary"]["matched"], 1);
    assert_eq!(link_artifact["summary"]["ambiguous"], 1);
    assert_eq!(link_artifact["summary"]["unmatched"], 1);
    assert_eq!(
        link_artifact["decision_artifact"]["matches"][0]["target_id"],
        "T-MATCH"
    );
    assert_eq!(
        link_artifact["decision_artifact"]["matches"][0]["reference_id"],
        "R-MATCH"
    );
    assert_eq!(
        link_artifact["decision_artifact"]["ambiguous"][0]["target_id"],
        "T-AMB"
    );
    let ambiguous_refs = link_artifact["decision_artifact"]["ambiguous"][0]["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|candidate| candidate["reference_id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        ambiguous_refs,
        vec!["R-AMB-A".to_string(), "R-AMB-B".to_string()]
    );
    assert_eq!(
        link_artifact["decision_artifact"]["unmatched"][0]["target_id"],
        "T-NONE"
    );
    assert_eq!(
        link_artifact["decision_artifact"]["unmatched"][0]["reason"],
        "no_candidates"
    );

    let escrow_json = review_json_for_link(&link_path, "escrow");
    assert_eq!(escrow_json["version"], "canon_entity_review_queue.v0");
    assert_eq!(
        escrow_json["source_link_hash"],
        link_artifact["artifact_content_hash"]
    );
    assert_eq!(
        escrow_json["source_solve_hash"],
        link_artifact["shared_solve_artifact"]["content_hash"]
    );
    let escrow_items = escrow_json["review_items"].as_array().unwrap();
    assert_eq!(escrow_items.len(), 2);

    let mut escrow_by_id = BTreeMap::new();
    let mut escrow_components = BTreeSet::new();
    for item in escrow_items {
        let review_id = item["review_id"].as_str().unwrap().to_string();
        let component_id = item["component_id"].as_str().unwrap().to_string();
        let state = item["state"].as_str().unwrap().to_string();
        let reasons = review_priority_reasons(item);
        assert_eq!(state, "escrow");
        assert_eq!(
            item["proposed_action"].as_str().unwrap(),
            "review_directional_abstention"
        );
        match component_id.as_str() {
            "T-AMB" => assert_eq!(reasons, vec!["ambiguous".to_string()]),
            "T-NONE" => assert_eq!(reasons, vec!["unmatched".to_string()]),
            other => panic!("unexpected escrow component {other}"),
        }
        escrow_components.insert(component_id.clone());
        escrow_by_id.insert(review_id, (component_id, state, reasons));
    }
    assert_eq!(
        escrow_components,
        BTreeSet::from(["T-AMB".to_string(), "T-NONE".to_string()])
    );

    let escrow_csv_rows = assert_review_csv_id_parity(&link_path, "escrow", &escrow_json);
    for row in escrow_csv_rows {
        let (component_id, state, reasons) = escrow_by_id.get(&row["review_id"]).unwrap();
        let csv_reasons: Vec<String> = serde_json::from_str(&row["priority_reasons_json"]).unwrap();
        assert_eq!(&row["component_id"], component_id);
        assert_eq!(&row["state"], state);
        assert_eq!(&csv_reasons, reasons);
    }

    let resolved_json = review_json_for_link(&link_path, "resolved");
    let resolved_items = resolved_json["review_items"].as_array().unwrap();
    assert_eq!(resolved_items.len(), 1);
    let resolved_item = &resolved_items[0];
    assert_eq!(resolved_item["component_id"], "T-MATCH");
    assert_eq!(resolved_item["state"], "resolved_existing");
    assert_eq!(resolved_item["proposed_action"], "audit_directional_match");
    assert_eq!(
        review_priority_reasons(resolved_item),
        vec!["directional_match".to_string()]
    );
    let resolved_csv_rows = assert_review_csv_id_parity(&link_path, "resolved", &resolved_json);
    assert_eq!(resolved_csv_rows.len(), 1);
    assert_eq!(
        resolved_csv_rows[0]["review_id"],
        resolved_item["review_id"].as_str().unwrap()
    );
    assert_eq!(resolved_csv_rows[0]["component_id"], "T-MATCH");
    assert_eq!(resolved_csv_rows[0]["state"], "resolved_existing");
    let resolved_csv_reasons: Vec<String> =
        serde_json::from_str(&resolved_csv_rows[0]["priority_reasons_json"]).unwrap();
    assert_eq!(resolved_csv_reasons, vec!["directional_match".to_string()]);

    let all_json = review_json_for_link(&link_path, "all");
    let mut expected_all_ids = review_ids(&escrow_json);
    expected_all_ids.extend(review_ids(&resolved_json));
    assert_eq!(all_json["review_items"].as_array().unwrap().len(), 3);
    assert_eq!(review_ids(&all_json), expected_all_ids);
    let all_csv_rows = assert_review_csv_id_parity(&link_path, "all", &all_json);
    assert_eq!(all_csv_rows.len(), 3);

    let second_temp = tempdir().unwrap();
    let (second_link_path, second_link_artifact) = run_neutral_mixed_link(second_temp.path());
    assert_eq!(second_link_artifact["summary"], link_artifact["summary"]);
    let second_all_json = review_json_for_link(&second_link_path, "all");
    assert_eq!(review_ids(&second_all_json), review_ids(&all_json));
}

#[test]
fn test_entity_review_default_native_artifacts_stay_review_queue_contract() {
    let temp_dir = tempdir().unwrap();
    let (_, run_path, solve_path, link_path, link_artifact) =
        run_neutral_mixed_link_artifacts(temp_dir.path());

    for (artifact_path, expected_version) in [
        (&solve_path, "canon_entity_review.v1"),
        (&run_path, "canon_entity_review.v1"),
        (&link_path, "canon_entity_review_queue.v0"),
    ] {
        let first = review_json_bytes_for_artifact(artifact_path, "all");
        let second = review_json_bytes_for_artifact(artifact_path, "all");
        assert_eq!(
            second,
            first,
            "default review export bytes should stay deterministic for {}",
            artifact_path.display()
        );
        let review: Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(review["version"], expected_version);
        assert!(review["review_items"].is_array());
    }

    let link_review = review_json_for_artifact(&link_path, "all");
    assert_eq!(
        link_review["source_link_hash"],
        link_artifact["artifact_content_hash"]
    );
    assert_eq!(
        link_review["source_solve_hash"],
        link_artifact["shared_solve_artifact"]["content_hash"]
    );
}

#[test]
fn test_entity_review_export_public_formats_cover_v1_run_and_native_link() {
    let temp_dir = tempdir().unwrap();
    let (_, run_path, _, link_path, _) = run_neutral_mixed_link_artifacts(temp_dir.path());
    let run_artifact: Value = serde_json::from_slice(&std::fs::read(&run_path).unwrap()).unwrap();

    let run_review = review_json_for_artifact(&run_path, "all");
    assert_eq!(run_review["version"], "canon_entity_review.v1");
    assert_eq!(
        run_review["source_result"]["content_hash"],
        run_artifact["artifact_content_hash"]
    );

    let link_review = native_review_json_for_artifact(&link_path, "escrow");
    assert_eq!(link_review["version"], "canon_entity_native_review.v0");
    assert_native_review_csv_id_parity(
        &link_review,
        &native_review_csv_for_artifact(&link_path, "escrow"),
    );
    assert_native_review_html_offline(
        &native_review_html_for_artifact(&link_path, "escrow"),
        &link_review,
    );

    let link_items = link_review["review_items"].as_array().unwrap();
    assert_eq!(link_items.len(), 2);
    let mut reasons = BTreeSet::new();
    for item in link_items {
        assert_eq!(item["mode"], "link");
        assert_eq!(item["mode_context"]["type"], "link");
        let allowed_actions = item["allowed_actions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|action| action.as_str().unwrap().to_string())
            .collect::<BTreeSet<_>>();
        let item_reasons = item["impact"]["priority_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .map(|reason| reason.as_str().unwrap().to_string())
            .collect::<BTreeSet<_>>();
        reasons.extend(item_reasons.iter().cloned());

        if item_reasons.contains("ambiguous") {
            assert!(
                item["mode_context"]["right_surface_id"]
                    .as_str()
                    .is_some_and(|right| !right.is_empty()),
                "ambiguous directional abstention should retain a right/candidate surface"
            );
            assert!(
                item["candidate_links"]
                    .as_array()
                    .is_some_and(|links| !links.is_empty()),
                "ambiguous directional abstention should retain candidate links"
            );
            assert_eq!(
                allowed_actions,
                BTreeSet::from([
                    "cannot_link".to_string(),
                    "defer".to_string(),
                    "relation".to_string()
                ])
            );
        } else if item_reasons.contains("unmatched") {
            assert!(
                match item["mode_context"].get("right_surface_id") {
                    Some(right) => right.is_null(),
                    None => true,
                },
                "candidate-free unmatched should not invent a right surface"
            );
            assert_eq!(
                item["candidate_links"].as_array().unwrap().len(),
                0,
                "candidate-free unmatched should not invent candidate links"
            );
            assert_eq!(allowed_actions, BTreeSet::from(["defer".to_string()]));
        } else {
            panic!("unexpected link review reasons: {item_reasons:?}");
        }
    }
    assert_eq!(
        reasons,
        BTreeSet::from(["ambiguous".to_string(), "unmatched".to_string()])
    );
}

#[test]
fn test_entity_native_review_import_public_receipt_and_refusals() {
    let temp_dir = tempdir().unwrap();
    let (fixture, _, _, link_path, _) = run_neutral_mixed_link_artifacts(temp_dir.path());
    let source_review_bytes = native_review_json_bytes_for_artifact(&link_path, "escrow");
    let source_review: Value = serde_json::from_slice(&source_review_bytes).unwrap();
    let source_review_path = temp_dir.path().join("native-link-review.json");
    std::fs::write(&source_review_path, &source_review_bytes).unwrap();

    let items = source_review["review_items"].as_array().unwrap();
    assert!(items.len() >= 2);
    let decisions = items
        .iter()
        .map(|item| native_defer_decision(&source_review, item))
        .collect::<Vec<_>>();
    let decisions_path = temp_dir.path().join("native-decisions.json");
    write_native_review_decisions(&decisions_path, &decisions);

    let registry_before = registry_snapshot(&fixture.registry);
    let import_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "import",
            decisions_path.to_str().unwrap(),
            "--registry",
            fixture.registry.to_str().unwrap(),
            "--next-version",
            "0.1.1",
            "--source-review",
            source_review_path.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let receipt: Value = serde_json::from_slice(&import_output.get_output().stdout).unwrap();
    assert_eq!(receipt["version"], "canon_entity_native_review_import.v0");
    assert_eq!(receipt["accepted_decisions"], decisions.len() as u64);
    assert_eq!(
        receipt["source_review_artifact_hash"],
        source_review["artifact_content_hash"]
    );
    assert_eq!(
        receipt["source_review_queue_hash"],
        source_review["binding"]["source_review_queue_hash"]
    );
    assert_eq!(
        receipt["patches"]["defer_patches"]
            .as_array()
            .unwrap()
            .len(),
        decisions.len()
    );
    assert_eq!(registry_snapshot(&fixture.registry), registry_before);

    let mut stale = decisions.clone();
    stale[0]["source_review_artifact_hash"] = Value::String("blake3:stale".to_string());
    let stale_path = temp_dir.path().join("native-decisions-stale.json");
    write_native_review_decisions(&stale_path, &stale);
    assert_native_review_import_refuses_without_registry_mutation(
        &stale_path,
        &source_review_path,
        &fixture.registry,
        "source_review_artifact_hash",
    );

    let mut tampered = decisions.clone();
    tampered[0]["decision_binding_hash"] = Value::String("blake3:tampered".to_string());
    let tampered_path = temp_dir.path().join("native-decisions-tampered.json");
    write_native_review_decisions(&tampered_path, &tampered);
    assert_native_review_import_refuses_without_registry_mutation(
        &tampered_path,
        &source_review_path,
        &fixture.registry,
        "decision_binding_hash",
    );

    let mut duplicate = decisions.clone();
    duplicate.push(decisions[0].clone());
    let duplicate_path = temp_dir.path().join("native-decisions-duplicate.json");
    write_native_review_decisions(&duplicate_path, &duplicate);
    assert_native_review_import_refuses_without_registry_mutation(
        &duplicate_path,
        &source_review_path,
        &fixture.registry,
        "review_id",
    );

    let mut context_swapped = decisions.clone();
    context_swapped[0]["mode_context"] = decisions[1]["mode_context"].clone();
    let context_swapped_path = temp_dir
        .path()
        .join("native-decisions-context-swapped.json");
    write_native_review_decisions(&context_swapped_path, &context_swapped);
    assert_native_review_import_refuses_without_registry_mutation(
        &context_swapped_path,
        &source_review_path,
        &fixture.registry,
        "surface_ids",
    );
}

#[test]
fn test_entity_review_export_run_handoff_rejects_unsafe_or_mismatched_solve_path() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_entity_link_smoke_fixture(temp_dir.path(), true);
    let suite = write_entity_link_audit_suite(temp_dir.path());
    let work_dir = temp_dir.path().join("entity-link-work");
    let mut args = entity_link_smoke_args(&fixture, &work_dir);
    args.extend(["--suite", suite.to_str().unwrap(), "--no-witness"]);
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .assert()
        .success();

    let run_path = work_dir.join("run/run.json");
    let run_artifact: Value = serde_json::from_slice(&std::fs::read(&run_path).unwrap()).unwrap();

    let tampered_run_path = work_dir.join("tampered-run.json");
    let mut tampered_run = run_artifact.clone();
    tampered_run["summary"]["counts"]["tampered"] = Value::from(1);
    std::fs::write(
        &tampered_run_path,
        serde_json::to_vec_pretty(&tampered_run).unwrap(),
    )
    .unwrap();

    let tampered_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            tampered_run_path.to_str().unwrap(),
            "--include",
            "escrow",
            "--emit",
            "json",
        ])
        .assert()
        .code(2);
    let tampered_refusal: Value =
        serde_json::from_slice(&tampered_output.get_output().stdout).unwrap();
    assert_eq!(
        tampered_refusal["refusal"]["code"],
        "E_ENTITY_ARTIFACT_CONTRACT"
    );
    assert_eq!(
        tampered_refusal["refusal"]["detail"]["field"],
        "artifact_content_hash"
    );
    assert_eq!(
        tampered_refusal["refusal"]["detail"]["writes_performed"],
        false
    );
    assert!(
        !tampered_refusal["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("observations")
    );

    let unsafe_run_path = work_dir.join("unsafe-run.json");
    let mut unsafe_run = run_artifact.clone();
    unsafe_run["metadata"]["workdir"]["artifact_relpath"] =
        Value::String("../run/run.json".to_string());
    std::fs::write(
        &unsafe_run_path,
        serde_json::to_vec_pretty(&unsafe_run).unwrap(),
    )
    .unwrap();

    let unsafe_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            unsafe_run_path.to_str().unwrap(),
            "--include",
            "escrow",
            "--emit",
            "json",
        ])
        .assert()
        .code(2);
    let unsafe_refusal: Value = serde_json::from_slice(&unsafe_output.get_output().stdout).unwrap();
    assert_eq!(
        unsafe_refusal["refusal"]["code"],
        "E_ENTITY_ARTIFACT_CONTRACT"
    );
    assert_eq!(
        unsafe_refusal["refusal"]["detail"]["field"],
        "metadata.workdir"
    );
    assert_eq!(
        unsafe_refusal["refusal"]["detail"]["writes_performed"],
        false
    );
    assert!(
        !unsafe_refusal["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("observations")
    );

    let mismatch_run_path = work_dir.join("mismatch-run.json");
    let mut mismatch_run = run_artifact;
    let upstreams = mismatch_run["metadata"]["upstream_artifacts"]
        .as_array_mut()
        .unwrap();
    let solve_upstream = upstreams
        .iter_mut()
        .find(|artifact| artifact["version"] == "canon_entity_solve.v1")
        .expect("solve upstream");
    solve_upstream["content_hash"] = Value::String("mismatch".to_string());
    std::fs::write(
        &mismatch_run_path,
        serde_json::to_vec_pretty(&mismatch_run).unwrap(),
    )
    .unwrap();

    let mismatch_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            mismatch_run_path.to_str().unwrap(),
            "--include",
            "escrow",
            "--emit",
            "json",
        ])
        .assert()
        .code(2);
    let mismatch_refusal: Value =
        serde_json::from_slice(&mismatch_output.get_output().stdout).unwrap();
    assert_eq!(
        mismatch_refusal["refusal"]["code"],
        "E_ENTITY_ARTIFACT_CONTRACT"
    );
    assert_eq!(
        mismatch_refusal["refusal"]["detail"]["field"],
        "metadata.upstream_artifacts.content_hash"
    );
    assert_eq!(
        mismatch_refusal["refusal"]["detail"]["writes_performed"],
        false
    );
    assert!(
        !mismatch_refusal["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("observations")
    );
}

#[test]
fn test_entity_review_export_link_handoff_rejects_malformed_or_tampered_artifacts() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_entity_link_smoke_fixture(temp_dir.path(), true);
    let work_dir = temp_dir.path().join("entity-link-work");
    let mut args = entity_link_smoke_args(&fixture, &work_dir);
    args.push("--no-witness");
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .assert()
        .success();

    let link_path = work_dir.join("link/link.json");
    let link_artifact: Value = serde_json::from_slice(&std::fs::read(&link_path).unwrap()).unwrap();

    let link_dir = work_dir.join("link");
    let malformed_link_path = link_dir.join("malformed-link.json");
    std::fs::write(
        &malformed_link_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": "canon_entity_link.v0"
        }))
        .unwrap(),
    )
    .unwrap();
    let malformed_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            malformed_link_path.to_str().unwrap(),
            "--include",
            "escrow",
            "--emit",
            "json",
        ])
        .assert()
        .code(2);
    let malformed_refusal: Value =
        serde_json::from_slice(&malformed_output.get_output().stdout).unwrap();
    assert_eq!(
        malformed_refusal["refusal"]["code"],
        "E_ENTITY_ARTIFACT_CONTRACT"
    );
    assert_eq!(
        malformed_refusal["refusal"]["detail"]["writes_performed"],
        false
    );

    let unknown_top_level_path = link_dir.join("tampered-link-unknown-top-level.json");
    let mut unknown_top_level = link_artifact.clone();
    unknown_top_level["unexpected_top_level"] = Value::String("discarded".to_string());
    std::fs::write(
        &unknown_top_level_path,
        serde_json::to_vec_pretty(&unknown_top_level).unwrap(),
    )
    .unwrap();
    assert_link_review_export_refuses_without_writes(
        &unknown_top_level_path,
        &link_dir,
        "unexpected_top_level",
    );

    let unknown_decision_path = link_dir.join("tampered-link-unknown-decision.json");
    let mut unknown_decision = link_artifact.clone();
    unknown_decision["decision_artifact"]["unexpected_decision_field"] =
        Value::String("discarded".to_string());
    std::fs::write(
        &unknown_decision_path,
        serde_json::to_vec_pretty(&unknown_decision).unwrap(),
    )
    .unwrap();
    assert_link_review_export_refuses_without_writes(
        &unknown_decision_path,
        &link_dir,
        "decision_artifact.unexpected_decision_field",
    );

    let nested_hash_path = link_dir.join("tampered-link-decision.json");
    let mut nested_hash = link_artifact.clone();
    nested_hash["decision_artifact"]["matches"][0]["score"] = serde_json::json!(0.25);
    std::fs::write(
        &nested_hash_path,
        serde_json::to_vec_pretty(&resealed_typed_link_artifact(&nested_hash)).unwrap(),
    )
    .unwrap();
    let nested_hash_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            nested_hash_path.to_str().unwrap(),
            "--include",
            "escrow",
            "--emit",
            "json",
        ])
        .assert()
        .code(2);
    let nested_hash_refusal: Value =
        serde_json::from_slice(&nested_hash_output.get_output().stdout).unwrap();
    assert_eq!(
        nested_hash_refusal["refusal"]["detail"]["field"],
        "decision_artifact.artifact_content_hash"
    );
    assert_eq!(
        nested_hash_refusal["refusal"]["detail"]["writes_performed"],
        false
    );

    let partition_path = link_dir.join("tampered-link-partition.json");
    let mut partition = link_artifact.clone();
    partition["summary"]["unmatched"] = serde_json::json!(99);
    std::fs::write(
        &partition_path,
        serde_json::to_vec_pretty(&resealed_typed_link_artifact(&partition)).unwrap(),
    )
    .unwrap();
    let partition_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            partition_path.to_str().unwrap(),
            "--include",
            "escrow",
            "--emit",
            "json",
        ])
        .assert()
        .code(2);
    let partition_refusal: Value =
        serde_json::from_slice(&partition_output.get_output().stdout).unwrap();
    assert_eq!(partition_refusal["refusal"]["detail"]["field"], "summary");
    assert_eq!(
        partition_refusal["refusal"]["detail"]["writes_performed"],
        false
    );

    let materialized_path = link_dir.join("tampered-link-materialized.json");
    let mut materialized = link_artifact;
    materialized["materialized_rows_content_hash"] =
        Value::String("blake3:wrong-materialized".to_string());
    std::fs::write(
        &materialized_path,
        serde_json::to_vec_pretty(&resealed_typed_link_artifact(&materialized)).unwrap(),
    )
    .unwrap();
    let materialized_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            materialized_path.to_str().unwrap(),
            "--include",
            "escrow",
            "--emit",
            "json",
        ])
        .assert()
        .code(2);
    let materialized_refusal: Value =
        serde_json::from_slice(&materialized_output.get_output().stdout).unwrap();
    assert_eq!(
        materialized_refusal["refusal"]["detail"]["field"],
        "materialized_rows_content_hash"
    );
    assert_eq!(
        materialized_refusal["refusal"]["detail"]["writes_performed"],
        false
    );
}

#[test]
fn test_entity_link_review_ids_are_path_independent() {
    let left_temp = tempdir().unwrap();
    let right_temp = tempdir().unwrap();

    let left_ids = run_link_review_ids(left_temp.path());
    let right_ids = run_link_review_ids(right_temp.path());

    assert_eq!(left_ids, right_ids);
    assert!(!left_ids.is_empty());
}

fn run_link_review_ids(root: &Path) -> Vec<String> {
    let fixture = write_entity_link_smoke_fixture(root, true);
    let work_dir = root.join("entity-link-work");
    let mut args = entity_link_smoke_args(&fixture, &work_dir);
    args.push("--no-witness");
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .assert()
        .success();

    let link_path = work_dir.join("link/link.json");
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "review",
            "export",
            link_path.to_str().unwrap(),
            "--include",
            "all",
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let review: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    let mut ids = review["review_items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["review_id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

#[test]
fn test_entity_link_cli_invalid_suite_refuses_before_writes() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_entity_link_smoke_fixture(temp_dir.path(), true);
    let suite = temp_dir.path().join("bad-suite");
    std::fs::create_dir_all(&suite).unwrap();
    std::fs::write(suite.join("manifest.json"), "{not json").unwrap();
    let registry_before = registry_snapshot(&fixture.registry);
    let work_dir = temp_dir.path().join("entity-link-work");
    let mut args = entity_link_smoke_args(&fixture, &work_dir);
    args.extend([
        "--suite",
        suite.to_str().unwrap(),
        "--gold",
        fixture.gold.to_str().unwrap(),
        "--write-back",
        "--no-witness",
    ]);

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .assert()
        .code(2);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_ENTITY_AUDIT_GATE");
    assert_eq!(payload["refusal"]["detail"]["writes_performed"], false);
    assert!(!work_dir.exists());
    assert_eq!(registry_snapshot(&fixture.registry), registry_before);
}

#[test]
fn test_entity_link_cli_accepts_advertised_row_formats() {
    let temp_dir = tempdir().unwrap();
    for format in [
        EntityLinkFixtureFormat::Csv,
        EntityLinkFixtureFormat::Tsv,
        EntityLinkFixtureFormat::Jsonl,
        EntityLinkFixtureFormat::Ndjson,
    ] {
        let root = temp_dir.path().join(format.label());
        std::fs::create_dir_all(&root).unwrap();
        let fixture = write_entity_link_smoke_fixture_with_format(&root, true, format);
        let work_dir = root.join("entity-link-work");
        let mut args = entity_link_smoke_args(&fixture, &work_dir);
        args.push("--no-witness");

        let output = Command::new(env!("CARGO_BIN_EXE_canon"))
            .args(args)
            .assert()
            .success();

        let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
        let payload: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(
            payload["summary"]["matched"],
            1,
            "format {} should match",
            format.label()
        );
        assert_eq!(payload["reference"]["row_count"], 1);
        assert_eq!(payload["target"]["row_count"], 1);
        let materialized =
            std::fs::read_to_string(work_dir.join("link/combined_rows.csv")).unwrap();
        assert!(materialized.contains("Reference Name"));
        assert!(materialized.contains("Target Name"));
    }
}

#[test]
fn test_entity_link_cli_budget_refusals_are_public() {
    for (flag, value, expected_code) in [
        ("--max-candidates", "0", "E_TOO_MANY_CANDIDATES"),
        ("--max-rows", "0", "E_TOO_LARGE"),
        ("--max-bytes", "1", "E_TOO_LARGE"),
    ] {
        let temp_dir = tempdir().unwrap();
        let fixture = write_entity_link_smoke_fixture(temp_dir.path(), true);
        let work_dir = temp_dir.path().join("entity-link-work");
        let mut args = entity_link_smoke_args(&fixture, &work_dir);
        args.extend([flag, value, "--no-witness"]);

        let output = Command::new(env!("CARGO_BIN_EXE_canon"))
            .args(args)
            .assert()
            .code(2);

        let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
        let payload: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(
            payload["refusal"]["code"], expected_code,
            "{flag} should refuse with {expected_code}"
        );
        assert!(
            !work_dir.exists(),
            "{flag} should fail before work-dir writes"
        );
    }
}

#[test]
fn test_entity_link_cli_without_writeback_does_not_mutate_inputs_or_registry() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_entity_link_smoke_fixture(temp_dir.path(), true);
    let work_dir = temp_dir.path().join("entity-link-work");
    let reference_before = std::fs::read(&fixture.reference).unwrap();
    let target_before = std::fs::read(&fixture.target).unwrap();
    let registry_before = registry_snapshot(&fixture.registry);
    let registry_files_before = registry_files(&fixture.registry);
    let mut args = entity_link_smoke_args(&fixture, &work_dir);
    args.push("--no-witness");

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .assert()
        .success();

    assert_eq!(std::fs::read(&fixture.reference).unwrap(), reference_before);
    assert_eq!(std::fs::read(&fixture.target).unwrap(), target_before);
    assert_eq!(registry_snapshot(&fixture.registry), registry_before);
    assert_eq!(registry_files(&fixture.registry), registry_files_before);
    assert_eq!(
        registry_files_before,
        BTreeSet::from(["registry.json".to_string()])
    );
}

#[test]
fn test_entity_link_cli_summary_output() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_entity_link_smoke_fixture(temp_dir.path(), true);
    let work_dir = temp_dir.path().join("entity-link-work");
    let mut args = entity_link_smoke_args(&fixture, &work_dir);
    args.extend(["--emit", "summary", "--no-witness"]);

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("canon_entity_link.v0"));
    assert!(stdout.contains("matched=1"));
    assert!(stdout.contains("match_rate=1.000"));
}

#[test]
fn test_entity_link_cli_partial_exit_one() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_entity_link_smoke_fixture(temp_dir.path(), false);
    let work_dir = temp_dir.path().join("entity-link-work");
    let mut args = entity_link_smoke_args(&fixture, &work_dir);
    args.push("--no-witness");

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .assert()
        .code(1);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    let decisions = &payload["decision_artifact"];
    assert_eq!(payload["summary"]["matched"], 0);
    assert_eq!(payload["summary"]["unmatched"], 1);
    assert_eq!(decisions["summary"]["matched"], 0);
    assert_eq!(decisions["summary"]["unmatched"], 1);
    assert_eq!(
        decisions["unmatched"][0]["reason"],
        "required_assertion_failed"
    );
}

#[test]
fn test_entity_link_cli_malformed_strategy_refusal() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_entity_link_smoke_fixture(temp_dir.path(), true);
    std::fs::write(&fixture.strategy, "not: [valid").unwrap();
    let work_dir = temp_dir.path().join("entity-link-work");
    let mut args = entity_link_smoke_args(&fixture, &work_dir);
    args.push("--no-witness");

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .assert()
        .code(2);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_BAD_STRATEGY");
}

#[test]
fn test_entity_link_cli_missing_column_refusal() {
    let temp_dir = tempdir().unwrap();
    let work_dir = temp_dir.path().join("entity-link-work");
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "link",
            fixture_path("tests/fixtures/resolve/tapes/reference_loans.csv")
                .to_str()
                .unwrap(),
            fixture_path("tests/fixtures/resolve/tapes/missing_column_target.csv")
                .to_str()
                .unwrap(),
            "--profile",
            "cmbs_tenant_label",
            "--strategy",
            fixture_path("tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml")
                .to_str()
                .unwrap(),
            "--registry",
            fixture_path("tests/fixtures/registries/resolve-servicers")
                .to_str()
                .unwrap(),
            "--work-dir",
            work_dir.to_str().unwrap(),
            "--no-witness",
        ])
        .assert()
        .code(2);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_COLUMN_NOT_FOUND");
}

#[test]
fn test_entity_link_cli_empty_tape_refusal() {
    let temp_dir = tempdir().unwrap();
    let work_dir = temp_dir.path().join("entity-link-work");
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "link",
            fixture_path("tests/fixtures/resolve/tapes/reference_loans.csv")
                .to_str()
                .unwrap(),
            fixture_path("tests/fixtures/resolve/tapes/empty_target.csv")
                .to_str()
                .unwrap(),
            "--profile",
            "cmbs_tenant_label",
            "--strategy",
            fixture_path("tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml")
                .to_str()
                .unwrap(),
            "--registry",
            fixture_path("tests/fixtures/registries/resolve-servicers")
                .to_str()
                .unwrap(),
            "--work-dir",
            work_dir.to_str().unwrap(),
            "--no-witness",
        ])
        .assert()
        .code(2);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_EMPTY_TAPE");
}

#[test]
fn test_entity_link_cli_no_witness_suppresses_ledger() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_entity_link_smoke_fixture(temp_dir.path(), true);
    let ledger_path = temp_dir.path().join("entity-link-witness.jsonl");
    let work_dir = temp_dir.path().join("entity-link-work");
    let mut args = entity_link_smoke_args(&fixture, &work_dir);
    args.push("--no-witness");

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .args(args)
        .assert()
        .success();

    assert!(!ledger_path.exists());
}

#[test]
fn test_entity_link_cli_witness_append_and_failure_nonfatal() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_entity_link_smoke_fixture(temp_dir.path(), true);
    let ledger_path = temp_dir.path().join("entity-link-witness.jsonl");
    let work_dir = temp_dir.path().join("entity-link-work");
    let args = entity_link_smoke_args(&fixture, &work_dir);

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .args(args)
        .assert()
        .success();

    let content = std::fs::read_to_string(&ledger_path).unwrap();
    let record: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(record["outcome"], "RESOLVED");
    assert_eq!(record["exit_code"], 0);
    assert_eq!(record["params"]["command"], "entity.link");
    assert_eq!(record["params"]["registry_id"], "entity-link-smoke");
    assert_eq!(record["params"]["summary"]["matched"], 1);

    let failure_work_dir = temp_dir.path().join("entity-link-work-failure");
    let failure_args = entity_link_smoke_args(&fixture, &failure_work_dir);
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", temp_dir.path())
        .args(failure_args)
        .assert()
        .success();
}

#[test]
fn test_entity_link_cli_writeback_invocation_shape() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_entity_link_smoke_fixture(temp_dir.path(), true);
    let work_dir = temp_dir.path().join("entity-link-work");
    let mut args = entity_link_smoke_args(&fixture, &work_dir);
    args.extend([
        "--gold",
        fixture.gold.to_str().unwrap(),
        "--write-back",
        "--no-witness",
    ]);

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    let decisions = &payload["decision_artifact"];
    assert_eq!(decisions["gold_score"]["accuracy"], 1.0);
    assert_eq!(decisions["write_back"]["written"], true);
    assert_eq!(decisions["write_back"]["entry_count"], 2);

    let mapping_file = decisions["write_back"]["mapping_file"].as_str().unwrap();
    let mapping_path = fixture.registry.join(mapping_file);
    assert!(mapping_path.exists());
    let mapping_content = std::fs::read_to_string(mapping_path).unwrap();
    assert!(mapping_content.contains("STRUCTURAL_MATCH:entity-link-smoke.v1"));
    assert!(!mapping_content.contains("100 Main St"));
}

#[test]
fn test_schema_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--schema")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&stdout).expect("--schema should output valid JSON");

    assert_eq!(
        json["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(json["$id"], "https://canon.v0/schema.json");
    assert!(json["properties"].is_object());
}

#[test]
fn test_strategy_register_and_resolve_cli() {
    let registry_dir = tempdir().unwrap();
    write_registry_metadata(registry_dir.path(), "strategy-test", "0.1.0", 0);

    let schema_path = registry_dir.path().join("profile.json");
    let compatible_schema_path = registry_dir.path().join("profile-compatible.json");
    let partial_schema_path = registry_dir.path().join("profile-partial.json");
    let skill_path = registry_dir.path().join("SKILL.md");
    let script_path = registry_dir.path().join("script.py");
    let verify_path = registry_dir.path().join("verify.json");
    let assess_path = registry_dir.path().join("assess.json");
    let airlock_path = registry_dir.path().join("airlock.json");

    write_strategy_schema(&schema_path, 3);
    write_strategy_schema(&compatible_schema_path, 99);
    std::fs::write(
        &partial_schema_path,
        serde_json::to_string_pretty(&serde_json::json!({
            "columns": [
                {"name": "vendor", "type": "string", "cardinality": 3},
                {"name": "category", "type": "string", "cardinality": 5}
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(&skill_path, "procurement skill").unwrap();
    std::fs::write(&script_path, "print('total')\n").unwrap();
    std::fs::write(&verify_path, r#"{"status":"PASS"}"#).unwrap();
    std::fs::write(&assess_path, r#"{"decision":"PROCEED"}"#).unwrap();
    std::fs::write(&airlock_path, r#"{"sealed":true}"#).unwrap();

    let register = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "register",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--schema",
            schema_path.to_str().unwrap(),
            "--skill",
            skill_path.to_str().unwrap(),
            "--script",
            script_path.to_str().unwrap(),
            "--script-id",
            "procurement-total.v1",
            "--language",
            "python",
            "--verify",
            verify_path.to_str().unwrap(),
            "--assess",
            assess_path.to_str().unwrap(),
            "--airlock",
            airlock_path.to_str().unwrap(),
            "--next-version",
            "0.2.0",
        ])
        .assert()
        .success();
    let register_stdout = String::from_utf8(register.get_output().stdout.clone()).unwrap();
    let register_json: Value = serde_json::from_str(&register_stdout).unwrap();
    assert_eq!(register_json["version"], "canon_strategy_register.v0");
    assert_eq!(register_json["registry"]["version"], "0.2.0");
    assert_eq!(
        register_json["registered"]["script"]["id"],
        "procurement-total.v1"
    );

    let exact = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "resolve",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--schema",
            schema_path.to_str().unwrap(),
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let exact_stdout = String::from_utf8(exact.get_output().stdout.clone()).unwrap();
    let exact_json: Value = serde_json::from_str(&exact_stdout).unwrap();
    assert_eq!(exact_json["outcome"], "EXACT");
    assert_eq!(exact_json["match"]["script"]["id"], "procurement-total.v1");

    let compatible = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "resolve",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--schema",
            compatible_schema_path.to_str().unwrap(),
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let compatible_stdout = String::from_utf8(compatible.get_output().stdout.clone()).unwrap();
    let compatible_json: Value = serde_json::from_str(&compatible_stdout).unwrap();
    assert_eq!(compatible_json["outcome"], "COMPATIBLE");

    let partial = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "resolve",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--schema",
            partial_schema_path.to_str().unwrap(),
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .code(1);
    let partial_stdout = String::from_utf8(partial.get_output().stdout.clone()).unwrap();
    let partial_json: Value = serde_json::from_str(&partial_stdout).unwrap();
    assert_eq!(partial_json["outcome"], "PARTIAL");
    assert_eq!(
        partial_json["escalation"]["reason"],
        "partial_schema_overlap"
    );
}

#[test]
fn test_strategy_task_operator_lifecycle_cli() {
    let registry_dir = tempdir().unwrap();
    write_registry_metadata(registry_dir.path(), "strategy-task-test", "0.1.0", 0);

    let skill_path = registry_dir.path().join("SKILL.md");
    let script_path = registry_dir.path().join("sql_lineage.py");
    let updated_script_path = registry_dir.path().join("sql_lineage_v2.py");
    let verify_path = registry_dir.path().join("verify.json");
    let assess_path = registry_dir.path().join("assess.json");
    let airlock_path = registry_dir.path().join("airlock.json");
    let witness_path = registry_dir.path().join("witness").join("strategy.jsonl");

    std::fs::write(&skill_path, "sql lineage skill").unwrap();
    std::fs::write(&script_path, "print('lineage v1')\n").unwrap();
    std::fs::write(&updated_script_path, "print('lineage v2')\n").unwrap();
    std::fs::write(&verify_path, r#"{"status":"PASS"}"#).unwrap();
    std::fs::write(&assess_path, r#"{"decision":"PROCEED"}"#).unwrap();
    std::fs::write(&airlock_path, r#"{"sealed":true}"#).unwrap();

    let register = Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &witness_path)
        .args([
            "strategy",
            "register",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
            "--script",
            script_path.to_str().unwrap(),
            "--script-id",
            "sql-lineage.v1",
            "--language",
            "python",
            "--grade",
            "operator-attested",
            "--operator",
            "Zac",
            "--reason",
            "worked on sample rows",
            "--attested-at",
            "2026-06-25T12:00:00Z",
            "--next-version",
            "0.2.0",
        ])
        .assert()
        .success();
    let register_json: Value =
        serde_json::from_slice(&register.get_output().stdout).expect("register JSON");
    assert_eq!(register_json["registered"]["key"]["type"], "task");
    assert_eq!(register_json["registered"]["task"], "sql_lineage");
    assert_eq!(register_json["registered"]["grade"], "operator-attested");
    assert_eq!(register_json["receipt"]["operation"], "register");
    assert!(
        register_json["receipt"]["before_registry_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
    assert!(
        register_json["receipt"]["after_registry_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
    let witness_lines = std::fs::read_to_string(&witness_path).unwrap();
    assert_eq!(witness_lines.lines().count(), 1);
    assert!(witness_lines.contains("strategy_receipt"));

    let resolve = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "resolve",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let resolve_json: Value = serde_json::from_slice(&resolve.get_output().stdout).unwrap();
    assert_eq!(resolve_json["outcome"], "EXACT");
    assert_eq!(resolve_json["match"]["script"]["id"], "sql-lineage.v1");
    assert_eq!(resolve_json["match"]["diagnostics"], Value::Null);

    let list = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "list",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--key-type",
            "task",
            "--status",
            "active",
        ])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    assert_eq!(list_json["version"], "canon_strategy_list.v0");
    assert_eq!(list_json["entries"].as_array().unwrap().len(), 1);

    let explain = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "explain",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let explain_json: Value = serde_json::from_slice(&explain.get_output().stdout).unwrap();
    assert_eq!(
        explain_json["active_resolution"]["script"]["id"],
        "sql-lineage.v1"
    );

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &witness_path)
        .args([
            "strategy",
            "update",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
            "--script",
            updated_script_path.to_str().unwrap(),
            "--script-id",
            "sql-lineage.v2",
            "--language",
            "python",
            "--operator",
            "Zac",
            "--reason",
            "tightened parser",
            "--attested-at",
            "2026-06-25T12:01:00Z",
            "--next-version",
            "0.3.0",
            "--no-witness",
        ])
        .assert()
        .success();
    let witness_lines_after_update = std::fs::read_to_string(&witness_path).unwrap();
    assert_eq!(witness_lines_after_update.lines().count(), 1);

    let updated_resolve = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "resolve",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let updated_resolve_json: Value =
        serde_json::from_slice(&updated_resolve.get_output().stdout).unwrap();
    assert_eq!(
        updated_resolve_json["match"]["script"]["id"],
        "sql-lineage.v2"
    );

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &witness_path)
        .args([
            "strategy",
            "deprecate",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
            "--operator",
            "Zac",
            "--reason",
            "retired active champion",
            "--attested-at",
            "2026-06-25T12:02:00Z",
            "--next-version",
            "0.4.0",
        ])
        .assert()
        .success();

    let deprecated_resolve = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "resolve",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .code(1);
    let deprecated_resolve_json: Value =
        serde_json::from_slice(&deprecated_resolve.get_output().stdout).unwrap();
    assert_eq!(deprecated_resolve_json["outcome"], "UNRESOLVED");

    let deprecated_explain = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "explain",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let deprecated_explain_json: Value =
        serde_json::from_slice(&deprecated_explain.get_output().stdout).unwrap();
    assert_eq!(deprecated_explain_json["active_resolution"], Value::Null);
    assert_eq!(
        deprecated_explain_json["ignored"].as_array().unwrap().len(),
        1
    );

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "register",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
            "--script",
            updated_script_path.to_str().unwrap(),
            "--script-id",
            "sql-lineage.v3",
            "--language",
            "python",
            "--grade",
            "operator-attested",
            "--operator",
            "Zac",
            "--reason",
            "replacement champion",
            "--attested-at",
            "2026-06-25T12:03:00Z",
            "--next-version",
            "0.5.0",
            "--no-witness",
        ])
        .assert()
        .success();

    let promote = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "promote",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
            "--verify",
            verify_path.to_str().unwrap(),
            "--assess",
            assess_path.to_str().unwrap(),
            "--airlock",
            airlock_path.to_str().unwrap(),
            "--next-version",
            "0.6.0",
            "--no-witness",
        ])
        .assert()
        .success();
    let promote_json: Value = serde_json::from_slice(&promote.get_output().stdout).unwrap();
    assert_eq!(promote_json["entry"]["grade"], "proof-attested");
    assert_eq!(promote_json["receipt"]["operation"], "promote");
}

#[test]
fn test_strategy_profile_cli_output_can_resolve_registered_strategy() {
    let registry_dir = tempdir().unwrap();
    write_registry_metadata(registry_dir.path(), "strategy-profile-test", "0.1.0", 0);

    let rows_path = registry_dir.path().join("rows.csv");
    let profile_path = registry_dir.path().join("profile.json");
    let skill_path = registry_dir.path().join("SKILL.md");
    let script_path = registry_dir.path().join("script.py");
    let verify_path = registry_dir.path().join("verify.json");
    let assess_path = registry_dir.path().join("assess.json");
    let airlock_path = registry_dir.path().join("airlock.json");

    std::fs::write(
        &rows_path,
        "vendor,amount,active\nAcme,10,true\nBolt,20,false\nAcme,30,true\n",
    )
    .unwrap();
    std::fs::write(&skill_path, "procurement skill").unwrap();
    std::fs::write(&script_path, "print('profiled total')\n").unwrap();
    std::fs::write(&verify_path, r#"{"status":"PASS"}"#).unwrap();
    std::fs::write(&assess_path, r#"{"decision":"PROCEED"}"#).unwrap();
    std::fs::write(&airlock_path, r#"{"sealed":true}"#).unwrap();

    let profile = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "profile",
            rows_path.to_str().unwrap(),
            "--max-rows",
            "10",
            "--max-bytes",
            "1024",
        ])
        .assert()
        .success();
    let profile_stdout = String::from_utf8(profile.get_output().stdout.clone()).unwrap();
    let profile_json: Value = serde_json::from_str(&profile_stdout).unwrap();
    assert_eq!(profile_json["version"], "canon_strategy_profile.v0");
    assert_eq!(profile_json["summary"]["rows"], 3);
    assert_eq!(profile_json["input"]["format"], "csv");
    assert_eq!(
        profile_json["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|column| column["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["active", "amount", "vendor"]
    );
    std::fs::write(&profile_path, &profile_stdout).unwrap();

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "register",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--schema",
            profile_path.to_str().unwrap(),
            "--skill",
            skill_path.to_str().unwrap(),
            "--script",
            script_path.to_str().unwrap(),
            "--script-id",
            "procurement-profiled-total.v1",
            "--language",
            "python",
            "--verify",
            verify_path.to_str().unwrap(),
            "--assess",
            assess_path.to_str().unwrap(),
            "--airlock",
            airlock_path.to_str().unwrap(),
            "--next-version",
            "0.2.0",
        ])
        .assert()
        .success();

    let resolve = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "resolve",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--schema",
            profile_path.to_str().unwrap(),
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let resolve_stdout = String::from_utf8(resolve.get_output().stdout.clone()).unwrap();
    let resolve_json: Value = serde_json::from_str(&resolve_stdout).unwrap();
    assert_eq!(resolve_json["outcome"], "EXACT");
    assert_eq!(
        resolve_json["query"]["schema_fingerprint"],
        profile_json["schema_fingerprint"]
    );
    assert_eq!(
        resolve_json["match"]["script"]["id"],
        "procurement-profiled-total.v1"
    );
}

#[test]
fn test_strategy_diff_cli_reports_changed_entry() {
    let old_registry_dir = tempdir().unwrap();
    let new_registry_dir = tempdir().unwrap();
    write_registry_metadata(old_registry_dir.path(), "strategy-test", "0.1.0", 0);
    write_registry_metadata(new_registry_dir.path(), "strategy-test", "0.1.0", 0);

    for (registry_dir, script_body) in [
        (old_registry_dir.path(), "print('old')\n"),
        (new_registry_dir.path(), "print('new')\n"),
    ] {
        let schema_path = registry_dir.join("profile.json");
        let skill_path = registry_dir.join("SKILL.md");
        let script_path = registry_dir.join("script.py");
        let verify_path = registry_dir.join("verify.json");
        let assess_path = registry_dir.join("assess.json");
        let airlock_path = registry_dir.join("airlock.json");

        write_strategy_schema(&schema_path, 3);
        std::fs::write(&skill_path, "procurement skill").unwrap();
        std::fs::write(&script_path, script_body).unwrap();
        std::fs::write(&verify_path, r#"{"status":"PASS"}"#).unwrap();
        std::fs::write(&assess_path, r#"{"decision":"PROCEED"}"#).unwrap();
        std::fs::write(&airlock_path, r#"{"sealed":true}"#).unwrap();

        Command::new(env!("CARGO_BIN_EXE_canon"))
            .args([
                "strategy",
                "register",
                "--registry",
                registry_dir.to_str().unwrap(),
                "--schema",
                schema_path.to_str().unwrap(),
                "--skill",
                skill_path.to_str().unwrap(),
                "--script",
                script_path.to_str().unwrap(),
                "--script-id",
                "procurement-total.v1",
                "--language",
                "python",
                "--verify",
                verify_path.to_str().unwrap(),
                "--assess",
                assess_path.to_str().unwrap(),
                "--airlock",
                airlock_path.to_str().unwrap(),
                "--next-version",
                "0.2.0",
            ])
            .assert()
            .success();
    }

    let diff = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "diff",
            "--old",
            old_registry_dir.path().to_str().unwrap(),
            "--new",
            new_registry_dir.path().to_str().unwrap(),
        ])
        .assert()
        .success();
    let diff_stdout = String::from_utf8(diff.get_output().stdout.clone()).unwrap();
    let diff_json: Value = serde_json::from_str(&diff_stdout).unwrap();
    assert_eq!(diff_json["version"], "canon_strategy_diff.v0");
    assert_eq!(diff_json["summary"]["changed"], 1);
    assert_eq!(
        diff_json["changed"][0]["change_types"],
        serde_json::json!(["script_path_change", "script_content_hash_change"])
    );
}

#[cfg(unix)]
#[test]
fn test_strategy_audit_cli_produces_register_compatible_proof() {
    let registry_dir = tempdir().unwrap();
    write_registry_metadata(registry_dir.path(), "strategy-audit-test", "0.1.0", 0);

    let schema_path = registry_dir.path().join("profile.json");
    let skill_path = registry_dir.path().join("SKILL.md");
    let script_path = registry_dir.path().join("script.sh");
    let suite_dir = registry_dir.path().join("suite");
    let audit_path = registry_dir.path().join("audit.json");

    write_strategy_schema(&schema_path, 1);
    std::fs::write(&skill_path, "procurement skill").unwrap();
    std::fs::write(&script_path, "#!/bin/sh\ncat\n").unwrap();
    let mut permissions = std::fs::metadata(&script_path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script_path, permissions).unwrap();

    std::fs::create_dir(&suite_dir).unwrap();
    std::fs::create_dir(suite_dir.join("inputs")).unwrap();
    std::fs::create_dir(suite_dir.join("expected")).unwrap();
    std::fs::write(suite_dir.join("inputs/case1.txt"), "Acme,10\n").unwrap();
    std::fs::write(suite_dir.join("expected/case1.out"), "Acme,10\n").unwrap();
    std::fs::write(
        suite_dir.join("manifest.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "suite_id": "strategy_audit_suite.v1",
            "version": "1.0.0",
            "repeatability_runs": 2,
            "fixtures": [
                {
                    "id": "case1",
                    "input": "inputs/case1.txt",
                    "expected_stdout": "expected/case1.out",
                    "expected_exit_code": 0
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let audit = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "audit",
            "--schema",
            schema_path.to_str().unwrap(),
            "--script",
            script_path.to_str().unwrap(),
            "--suite",
            suite_dir.to_str().unwrap(),
        ])
        .assert();

    if cfg!(target_os = "macos") {
        let audit = audit.success();
        let audit_stdout = String::from_utf8(audit.get_output().stdout.clone()).unwrap();
        let audit_json: Value = serde_json::from_str(&audit_stdout).unwrap();
        assert_eq!(audit_json["version"], "canon_strategy_audit.v0");
        assert_eq!(audit_json["passed"], true);
        assert_eq!(audit_json["decision"], "PROCEED");
        assert_eq!(audit_json["sealed"], true);
        std::fs::write(&audit_path, audit_stdout).unwrap();

        Command::new(env!("CARGO_BIN_EXE_canon"))
            .args([
                "strategy",
                "register",
                "--registry",
                registry_dir.path().to_str().unwrap(),
                "--schema",
                schema_path.to_str().unwrap(),
                "--skill",
                skill_path.to_str().unwrap(),
                "--script",
                script_path.to_str().unwrap(),
                "--script-id",
                "audited-script.v1",
                "--language",
                "sh",
                "--verify",
                audit_path.to_str().unwrap(),
                "--assess",
                audit_path.to_str().unwrap(),
                "--airlock",
                audit_path.to_str().unwrap(),
                "--next-version",
                "0.2.0",
            ])
            .assert()
            .success();
    } else {
        let audit = audit.code(2);
        let audit_stdout = String::from_utf8(audit.get_output().stdout.clone()).unwrap();
        let audit_json: Value = serde_json::from_str(&audit_stdout).unwrap();
        assert_eq!(audit_json["outcome"], "REFUSAL");
        assert_eq!(audit_json["refusal"]["code"], "E_STRATEGY_INPUT_CONTRACT");
    }
}

#[test]
fn info_flags_short_circuit_before_invalid_args_are_parsed() {
    let version = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["--version", "--emit", "bogus"])
        .assert()
        .success();
    let version_stdout = String::from_utf8(version.get_output().stdout.clone()).unwrap();
    assert_eq!(
        version_stdout.trim(),
        format!("canon {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(version.get_output().stderr.is_empty());

    let describe = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["--describe", "--column"])
        .assert()
        .success();
    let describe_stdout = String::from_utf8(describe.get_output().stdout.clone()).unwrap();
    let describe_json: Value =
        serde_json::from_str(&describe_stdout).expect("--describe should output valid JSON");
    assert_eq!(describe_json["name"], "canon");
    assert!(describe.get_output().stderr.is_empty());

    let schema = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["--schema", "--max-rows", "nope"])
        .assert()
        .success();
    let schema_stdout = String::from_utf8(schema.get_output().stdout.clone()).unwrap();
    let schema_json: Value =
        serde_json::from_str(&schema_stdout).expect("--schema should output valid JSON");
    assert_eq!(schema_json["$id"], "https://canon.v0/schema.json");
    assert!(schema.get_output().stderr.is_empty());
}

#[test]
fn test_registry_diff_json_output() {
    let old_dir = tempdir().unwrap();
    write_registry_metadata(old_dir.path(), "openfigi-cusip", "2026.02.28", 3);
    write_mapping_file(
        old_dir.path(),
        "a-primary.json",
        serde_json::json!([
            {
                "input": "AAPL",
                "canonical_id": "BBG000B9XRY4",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            },
            {
                "input": "MSFT",
                "canonical_id": "BBG000BPH459",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            },
            {
                "input": "TSLA",
                "canonical_id": "BBG000N9MNX3",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            }
        ]),
    );

    let new_dir = tempdir().unwrap();
    write_registry_metadata(new_dir.path(), "openfigi-cusip", "2026.03.05", 3);
    write_mapping_file(
        new_dir.path(),
        "a-primary.json",
        serde_json::json!([
            {
                "input": "AAPL",
                "canonical_id": "BBG000B9XRY4",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            },
            {
                "input": "MSFT",
                "canonical_id": "BBG000BPH45Z",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            },
            {
                "input": "NVDA",
                "canonical_id": "BBG000BBJQV0",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            }
        ]),
    );

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "diff",
            "--old",
            old_dir.path().to_str().unwrap(),
            "--new",
            new_dir.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(payload["version"], "canon_registry_diff.v0");
    assert_eq!(payload["old"]["id"], "openfigi-cusip");
    assert_eq!(payload["old"]["version"], "2026.02.28");
    assert_eq!(payload["new"]["version"], "2026.03.05");
    assert_eq!(payload["summary"]["added"], 1);
    assert_eq!(payload["summary"]["removed"], 1);
    assert_eq!(payload["summary"]["changed"], 1);
    assert_eq!(payload["summary"]["unchanged"], 1);
    assert_eq!(payload["added"][0]["input"], "NVDA");
    assert_eq!(payload["removed"][0]["input"], "TSLA");
    assert_eq!(payload["changed"][0]["input"], "MSFT");
    assert_eq!(payload["changed"][0]["change_type"], "canonical_id_change");
}

#[test]
fn test_registry_diff_summary_output() {
    let old_dir = tempdir().unwrap();
    write_registry_metadata(old_dir.path(), "openfigi-cusip", "2026.02.28", 1);
    write_mapping_file(
        old_dir.path(),
        "a-primary.json",
        serde_json::json!([
            {
                "input": "AAPL",
                "canonical_id": "BBG000B9XRY4",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            }
        ]),
    );

    let new_dir = tempdir().unwrap();
    write_registry_metadata(new_dir.path(), "openfigi-cusip", "2026.03.05", 2);
    write_mapping_file(
        new_dir.path(),
        "a-primary.json",
        serde_json::json!([
            {
                "input": "AAPL",
                "canonical_id": "BBG000B9XRY4",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            },
            {
                "input": "NVDA",
                "canonical_id": "BBG000BBJQV0",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            }
        ]),
    );

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "diff",
            "--old",
            old_dir.path().to_str().unwrap(),
            "--new",
            new_dir.path().to_str().unwrap(),
            "--emit",
            "summary",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert_eq!(
        stdout.trim(),
        "openfigi-cusip: 2026.02.28 -> 2026.03.05 | +1 added, -0 removed, ~0 changed, =1 unchanged"
    );
}

#[test]
fn test_registry_diff_mismatched_id_refusal_in_summary_mode() {
    let old_dir = tempdir().unwrap();
    write_registry_metadata(old_dir.path(), "old-registry", "1.0.0", 0);

    let new_dir = tempdir().unwrap();
    write_registry_metadata(new_dir.path(), "new-registry", "1.1.0", 0);

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "diff",
            "--old",
            old_dir.path().to_str().unwrap(),
            "--new",
            new_dir.path().to_str().unwrap(),
            "--emit",
            "summary",
        ])
        .assert()
        .code(2);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();

    assert!(stdout.is_empty());
    assert!(stderr.contains("E_BAD_REGISTRY"));
    assert!(stderr.contains("old-registry"));
    assert!(stderr.contains("new-registry"));
}

#[test]
fn test_registry_audit_json_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "audit",
            "tests/fixtures/inputs/partial.csv",
            "--registry",
            "tests/fixtures/registries/cusip-isin",
            "--column",
            "cusip",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(payload["version"], "canon_registry_audit.v0");
    assert_eq!(payload["seed"]["column"], "cusip");
    assert_eq!(payload["registry"]["id"], "cusip-isin");
    assert_eq!(payload["summary"]["total"], 3);
    assert_eq!(payload["summary"]["resolved"], 2);
    assert_eq!(payload["summary"]["unresolved"], 1);
    assert_eq!(payload["summary"]["distinct_canonical_targets"], 2);
    assert_eq!(payload["summary"]["distinct_rule_ids"], 1);
    assert_eq!(payload["resolved"].as_array().unwrap().len(), 2);
    assert_eq!(payload["unresolved"].as_array().unwrap().len(), 1);
    assert_eq!(payload["canonical_targets"].as_array().unwrap().len(), 2);
    assert_eq!(payload["rule_hits"][0]["rule_id"], "CUSIP_TO_ISIN");
    assert_eq!(payload["rule_hits"][0]["count"], 2);
}

#[test]
fn test_registry_audit_summary_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "audit",
            "tests/fixtures/inputs/partial.csv",
            "--registry",
            "tests/fixtures/registries/cusip-isin",
            "--column",
            "cusip",
            "--emit",
            "summary",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("cusip-isin@1.0.0 audit"));
    assert!(stdout.contains("3 total, 2 resolved, 1 unresolved"));
    assert!(stdout.contains("2 targets, 1 rules"));
}

#[test]
fn test_registry_audit_refusal_in_summary_mode() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "audit",
            "tests/fixtures/inputs/partial.csv",
            "--registry",
            "tests/fixtures/registries/cusip-isin",
            "--column",
            "missing_column",
            "--emit",
            "summary",
        ])
        .assert()
        .code(2);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();

    assert!(stdout.is_empty());
    assert!(stderr.contains("E_COLUMN_NOT_FOUND"));
}

#[test]
fn test_all_resolved_exit_code() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .assert()
        .code(0); // RESOLVED
}

#[test]
fn test_partial_exit_code() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/partial.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .assert()
        .code(1); // PARTIAL
}

#[test]
fn test_all_unresolved_exit_code() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_unresolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .assert()
        .code(1); // UNRESOLVED
}

#[test]
fn test_missing_input_file_refusal() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("nonexistent.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .assert()
        .code(2) // REFUSAL
        .stdout(predicate::str::contains("REFUSAL"));
}

#[test]
fn test_emit_csv_with_jsonl_refusal() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/basic.jsonl")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("csv")
        .assert()
        .code(2) // REFUSAL
        .stderr(predicate::str::contains("E_EMIT_FORMAT"));
}

#[test]
fn test_column_not_found_refusal() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("nonexistent_column")
        .assert()
        .code(2) // REFUSAL
        .stdout(predicate::str::contains("E_COLUMN_NOT_FOUND"));
}

#[test]
fn test_json_mode_success_to_stdout() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("json")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&stdout).expect("JSON mode should output valid JSON");

    assert_eq!(json["version"], "canon.v0");
    assert_eq!(json["outcome"], "RESOLVED");
    assert!(json["registry"].is_object());
    assert!(json["summary"].is_object());
}

#[test]
fn test_csv_mode_success_to_stdout() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("csv")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    // Should be CSV format with canonical column
    assert!(stdout.contains("cusip__canon"));
    // Should not be JSON
    assert!(!stdout.starts_with('{'));
}

#[test]
fn test_csv_mode_refusal_to_stderr() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/wrong_column.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("nonexistent")
        .arg("--emit")
        .arg("csv")
        .assert()
        .code(2);

    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    // Refusal should go to stderr in CSV mode
    assert!(stderr.contains("E_COLUMN_NOT_FOUND"));
    // No CSV output on stdout
    assert!(stdout.is_empty());
}

#[test]
fn test_witness_flag_no_witness() {
    // This test just ensures --no-witness doesn't break execution
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--no-witness")
        .assert()
        .success();
}

#[test]
fn test_witness_uses_epistemic_witness_env_path() {
    let ledger_dir = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let ledger_path = ledger_dir.path().join("nested").join("witness.jsonl");

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .current_dir(cwd.path())
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .arg(fixture_path("tests/fixtures/inputs/all_resolved.csv"))
        .arg("--registry")
        .arg(fixture_path("tests/fixtures/registries/cusip-isin"))
        .arg("--column")
        .arg("cusip")
        .assert()
        .success();

    assert!(ledger_path.exists());
    assert!(!cwd.path().join(".canon-witness.jsonl").exists());

    let content = std::fs::read_to_string(&ledger_path).unwrap();
    let record: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(record["tool"], "canon");
    assert!(record["id"].as_str().unwrap().starts_with("blake3:"));
    assert!(
        record["binary_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
    assert_eq!(
        record["inputs"][0]["path"],
        fixture_path("tests/fixtures/inputs/all_resolved.csv")
            .display()
            .to_string()
    );
    assert!(
        record["inputs"][0]["hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
    assert_eq!(record["params"]["registry_id"], "cusip-isin");
    assert_eq!(record["params"]["registry_version"], "1.0.0");
    assert_eq!(record["params"]["emit"], "json");
    assert_eq!(record["outcome"], "RESOLVED");
    assert_eq!(record["exit_code"], 0);
}

#[test]
fn test_witness_defaults_to_home_cmdrvl_path() {
    let home = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let ledger_path = home
        .path()
        .join(".cmdrvl")
        .join("state")
        .join("witness")
        .join("witness.jsonl");

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .current_dir(cwd.path())
        .env_remove("EPISTEMIC_WITNESS")
        .env("HOME", home.path())
        .arg(fixture_path("tests/fixtures/inputs/all_resolved.csv"))
        .arg("--registry")
        .arg(fixture_path("tests/fixtures/registries/cusip-isin"))
        .arg("--column")
        .arg("cusip")
        .assert()
        .success();

    assert!(ledger_path.exists());
    assert!(
        !home
            .path()
            .join(".epistemic")
            .join("witness.jsonl")
            .exists()
    );
    assert!(!cwd.path().join(".canon-witness.jsonl").exists());
}

#[test]
fn test_witness_migrates_legacy_home_epistemic_path_copy_only() {
    let home = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let legacy_path = home.path().join(".epistemic").join("witness.jsonl");
    std::fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
    std::fs::write(&legacy_path, "{\"tool\":\"legacy-canon\"}\n").unwrap();

    let canonical_path = home
        .path()
        .join(".cmdrvl")
        .join("state")
        .join("witness")
        .join("witness.jsonl");

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .current_dir(cwd.path())
        .env_remove("EPISTEMIC_WITNESS")
        .env("HOME", home.path())
        .arg(fixture_path("tests/fixtures/inputs/all_resolved.csv"))
        .arg("--registry")
        .arg(fixture_path("tests/fixtures/registries/cusip-isin"))
        .arg("--column")
        .arg("cusip")
        .assert()
        .success();

    assert!(legacy_path.exists());
    let content = std::fs::read_to_string(&canonical_path).unwrap();
    assert!(content.contains("\"tool\":\"legacy-canon\""));
    assert!(content.contains("\"tool\":\"canon\""));

    let migration_log =
        std::fs::read_to_string(home.path().join(".cmdrvl/migrations/applied.jsonl")).unwrap();
    assert!(migration_log.contains("\"path_class\":\"witness_ledger\""));
    assert!(migration_log.contains("\"secret_values_recorded\":false"));

    let notices =
        std::fs::read_to_string(home.path().join(".cmdrvl/notices/deprecated-paths.jsonl"))
            .unwrap();
    assert!(notices.contains("\"action\":\"legacy_path_migrated\""));
    assert!(notices.contains("\"secret_values_recorded\":false"));
}

#[test]
fn test_witness_hash_parity_and_chain_linkage() {
    let ledger_dir = tempdir().unwrap();
    let ledger_path = ledger_dir.path().join("witness.jsonl");
    let input_path = fixture_path("tests/fixtures/inputs/all_resolved.csv");
    let registry_path = fixture_path("tests/fixtures/registries/cusip-isin");

    let json_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .arg(&input_path)
        .arg("--registry")
        .arg(&registry_path)
        .arg("--column")
        .arg("cusip")
        .assert()
        .success();
    let json_stdout = json_output.get_output().stdout.clone();

    let csv_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .arg(&input_path)
        .arg("--registry")
        .arg(&registry_path)
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("csv")
        .assert()
        .success();
    let csv_stdout = csv_output.get_output().stdout.clone();

    let content = std::fs::read_to_string(&ledger_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);

    let first: Value = serde_json::from_str(lines[0]).unwrap();
    let second: Value = serde_json::from_str(lines[1]).unwrap();

    let expected_json_hash = format!("blake3:{}", blake3::hash(&json_stdout).to_hex());
    let expected_csv_hash = format!("blake3:{}", blake3::hash(&csv_stdout).to_hex());

    assert_eq!(first["output_hash"], expected_json_hash);
    assert_eq!(second["output_hash"], expected_csv_hash);
    assert_ne!(second["id"], first["id"]);
    assert_eq!(first["params"]["emit"], "json");
    assert_eq!(second["params"]["emit"], "csv");
}

#[test]
fn test_witness_hashes_stdin_bytes_without_dash_file() {
    let ledger_dir = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let ledger_path = ledger_dir.path().join("witness.jsonl");
    let stdin_data =
        std::fs::read_to_string(fixture_path("tests/fixtures/inputs/basic.jsonl")).unwrap();
    let registry_path = fixture_path("tests/fixtures/registries/cusip-isin");

    assert!(!cwd.path().join("-").exists());

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .current_dir(cwd.path())
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .arg("-")
        .arg("--registry")
        .arg(&registry_path)
        .arg("--column")
        .arg("cusip")
        .write_stdin(stdin_data.clone())
        .assert()
        .success();

    let content = std::fs::read_to_string(&ledger_path).unwrap();
    let record: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    let expected_hash = format!("blake3:{}", blake3::hash(stdin_data.as_bytes()).to_hex());

    assert_eq!(record["inputs"][0]["path"], "-");
    assert_eq!(record["inputs"][0]["hash"], expected_hash);
    assert_eq!(record["inputs"][0]["bytes"], stdin_data.len() as u64);
    assert_eq!(record["outcome"], "RESOLVED");
}

#[test]
fn test_map_out_sidecar_in_csv_mode() {
    use tempfile::NamedTempFile;

    let temp_file = NamedTempFile::new().unwrap();
    let map_out_path = temp_file.path().to_str().unwrap();

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("csv")
        .arg("--map-out")
        .arg(map_out_path)
        .assert()
        .success();

    // Check that sidecar JSON was written
    assert!(Path::new(map_out_path).exists());
    let sidecar_content = std::fs::read_to_string(map_out_path).unwrap();
    let json: Value = serde_json::from_str(&sidecar_content).expect("Sidecar should be valid JSON");

    assert_eq!(json["version"], "canon.v0");
    assert_eq!(json["outcome"], "RESOLVED");
}

#[test]
fn test_registry_build_materializes_registry_and_resolves() {
    let temp_dir = tempdir().unwrap();
    let seed_path = temp_dir.path().join("seed.csv");
    let output_dir = temp_dir.path().join("registries/mock-cusip");
    let resolve_path = temp_dir.path().join("resolve.csv");

    write_seed_csv(
        &seed_path,
        "cusip,note\nAAPL,ok\nMSFT,ok\nMISS_UNKNOWN,miss\nFAIL_BROKEN,fail\n,blank\nAAPL,dup\n",
    );
    write_seed_csv(&resolve_path, "cusip\nAAPL\nMSFT\n");

    let build = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "build",
            "--source",
            "mock",
            "--seed",
            seed_path.to_str().unwrap(),
            "--seed-column",
            "cusip",
            "--output",
            output_dir.to_str().unwrap(),
            "--version",
            "2026.03.13",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("provider failure(s)"));

    let build_stdout = String::from_utf8(build.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&build_stdout).unwrap();

    assert_eq!(payload["version"], "canon_registry_build.v0");
    assert_eq!(payload["source"], "mock");
    assert_eq!(payload["registry"]["id"], "mock-cusip");
    assert_eq!(payload["registry"]["version"], "2026.03.13");
    assert_eq!(payload["summary"]["seed_count"], 4);
    assert_eq!(payload["summary"]["queried_count"], 4);
    assert_eq!(payload["summary"]["carried_forward_count"], 0);
    assert_eq!(payload["summary"]["resolved_count"], 2);
    assert_eq!(payload["summary"]["unresolved_count"], 1);
    assert_eq!(payload["summary"]["failure_count"], 1);
    assert_eq!(payload["summary"]["skipped_special_reason_rows"], 1);
    assert_eq!(payload["special_reasons"][0]["reason"], "empty_value");
    assert_eq!(payload["special_reasons"][0]["count"], 1);
    assert_eq!(payload["files"], serde_json::json!(["cusip-to-mock.json"]));

    let registry_json: Value =
        serde_json::from_str(&std::fs::read_to_string(output_dir.join("registry.json")).unwrap())
            .unwrap();
    assert_eq!(registry_json["id"], "mock-cusip");
    assert_eq!(registry_json["version"], "2026.03.13");
    assert_eq!(registry_json["entry_count"], 2);
    assert!(output_dir.join("_build.json").exists());

    let resolve = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg(&resolve_path)
        .arg("--registry")
        .arg(&output_dir)
        .arg("--column")
        .arg("cusip")
        .arg("--explicit")
        .assert()
        .success();

    let resolve_stdout = String::from_utf8(resolve.get_output().stdout.clone()).unwrap();
    let resolve_json: Value = serde_json::from_str(&resolve_stdout).unwrap();
    assert_eq!(resolve_json["outcome"], "RESOLVED");
    assert_eq!(resolve_json["registry"]["id"], "mock-cusip");
    assert_eq!(resolve_json["summary"]["resolved"], 2);
    assert_eq!(resolve_json["mappings"][0]["canonical_id"], "u8:MOCK::AAPL");
    assert_eq!(resolve_json["mappings"][1]["canonical_id"], "u8:MOCK::MSFT");
}

#[test]
fn test_registry_export_dbt_seed_writes_seed_and_scaffolds() {
    let temp_dir = tempdir().unwrap();
    let registry_dir = temp_dir.path().join("registry");
    std::fs::create_dir_all(&registry_dir).unwrap();
    write_registry_metadata(&registry_dir, "funds", "2026.07.07", 3);
    write_mapping_file(
        &registry_dir,
        "fund-aliases.json",
        serde_json::json!([
            {
                "input": "Alpha Fund II",
                "canonical_id": "FUND-0001",
                "canonical_type": "fund",
                "rule_id": "FUND_NAME"
            },
            {
                "input": "ALPHA-II",
                "canonical_id": "FUND-0001",
                "canonical_type": "fund",
                "rule_id": "FUND_TICKER"
            },
            {
                "input": "0001234567",
                "canonical_id": "FUND-0001",
                "canonical_type": "fund",
                "rule_id": "FUND_KEY"
            }
        ]),
    );

    let seed_path = temp_dir.path().join("canon_funds.csv");
    let schema_path = temp_dir.path().join("schema.yml");
    let test_path = temp_dir.path().join("anti_collapse.sql");
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "export",
            "--format",
            "dbt-seed",
            "--registry",
            registry_dir.to_str().unwrap(),
            "--namespace",
            "funds",
            "--canonical-type",
            "fund",
            "--out",
            seed_path.to_str().unwrap(),
            "--schema-out",
            schema_path.to_str().unwrap(),
            "--anti-collapse-test-out",
            test_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["version"], "canon_registry_export.v0");
    assert_eq!(payload["format"], "dbt-seed");
    assert_eq!(payload["summary"]["exported_alias_count"], 3);
    assert_eq!(payload["summary"]["exported_entity_count"], 1);
    assert!(
        payload["content_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );

    let mut reader = csv::Reader::from_path(&seed_path).unwrap();
    let headers = reader.headers().unwrap().clone();
    assert!(headers.iter().any(|name| name == "canonical_iri"));
    let records = reader
        .records()
        .collect::<Result<Vec<_>, csv::Error>>()
        .unwrap();
    assert_eq!(records.len(), 3);
    let first = &records[0];
    assert_eq!(
        first.get(headers.iter().position(|h| h == "namespace").unwrap()),
        Some("funds")
    );
    assert_eq!(
        first.get(headers.iter().position(|h| h == "normalized_key").unwrap()),
        Some("ALPHAFUNDII")
    );
    assert_eq!(
        first.get(headers.iter().position(|h| h == "canonical_iri").unwrap()),
        Some("cmdrvl:FUND-0001")
    );
    assert!(
        std::fs::read_to_string(schema_path)
            .unwrap()
            .contains("seeds:")
    );
    assert!(
        std::fs::read_to_string(test_path)
            .unwrap()
            .contains("count(distinct canonical_id) > 1")
    );
}

#[test]
fn test_registry_export_dbt_seed_requires_namespace() {
    let temp_dir = tempdir().unwrap();
    write_registry_metadata(temp_dir.path(), "funds", "2026.07.07", 0);
    let seed_path = temp_dir.path().join("seed.csv");

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "export",
            "--format",
            "dbt-seed",
            "--registry",
            temp_dir.path().to_str().unwrap(),
            "--out",
            seed_path.to_str().unwrap(),
        ])
        .assert()
        .code(2);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_PARSE");
    assert!(
        payload["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("requires --namespace")
    );
}

#[test]
fn test_registry_export_search_index_writes_generic_sqlite_artifact() {
    let temp_dir = tempdir().unwrap();
    let registry_dir = temp_dir.path().join("registry");
    std::fs::create_dir_all(&registry_dir).unwrap();
    write_registry_metadata(&registry_dir, "funds", "2026.07.07", 3);
    write_mapping_file(
        &registry_dir,
        "fund-aliases.json",
        serde_json::json!([
            {
                "input": "Alpha Fund II",
                "canonical_id": "FUND-0001",
                "canonical_type": "fund",
                "rule_id": "FUND_NAME"
            },
            {
                "input": "ALPHA-II",
                "canonical_id": "FUND-0001",
                "canonical_type": "fund",
                "rule_id": "FUND_TICKER"
            },
            {
                "input": "0001234567",
                "canonical_id": "FUND-0001",
                "canonical_type": "fund",
                "rule_id": "FUND_KEY"
            }
        ]),
    );

    let sqlite_path = temp_dir.path().join("funds-search.sqlite");
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "export",
            "--format",
            "search-index",
            "--registry",
            registry_dir.to_str().unwrap(),
            "--out",
            sqlite_path.to_str().unwrap(),
            "--emit",
            "summary",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("search-index export: 3 aliases, 1 entities"));

    let conn = rusqlite::Connection::open(sqlite_path).unwrap();
    let artifact_version: String = conn
        .query_row(
            "SELECT value FROM metadata WHERE key = 'artifact_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(artifact_version, "canon_registry_search_index.v0");

    let iri: String = conn
        .query_row(
            "SELECT canonical_iri FROM aliases WHERE normalized_key = 'ALPHAFUNDII'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(iri, "cmdrvl:FUND-0001");

    let entity_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entities", [], |row| row.get(0))
        .unwrap();
    assert_eq!(entity_count, 1);

    let exact_score: i64 = conn
        .query_row(
            "SELECT score FROM scoring_tiers WHERE tier = 'exact'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(exact_score, 100);

    let fts_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM aliases_fts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(fts_count, 3);
}

#[test]
fn test_registry_build_incremental_carries_forward_existing_entries() {
    let temp_dir = tempdir().unwrap();
    let initial_seed_path = temp_dir.path().join("seed-initial.csv");
    let incremental_seed_path = temp_dir.path().join("seed-incremental.csv");
    let output_dir = temp_dir.path().join("registries/mock-cusip");

    write_seed_csv(&initial_seed_path, "cusip\nAAPL\nMSFT\n");
    write_seed_csv(&incremental_seed_path, "cusip\nAAPL\nMSFT\nNVDA\n");

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "build",
            "--source",
            "mock",
            "--seed",
            initial_seed_path.to_str().unwrap(),
            "--seed-column",
            "cusip",
            "--output",
            output_dir.to_str().unwrap(),
            "--version",
            "2026.03.13",
        ])
        .assert()
        .success();

    let incremental = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "build",
            "--source",
            "mock",
            "--seed",
            incremental_seed_path.to_str().unwrap(),
            "--seed-column",
            "cusip",
            "--output",
            output_dir.to_str().unwrap(),
            "--version",
            "2026.03.14",
            "--incremental",
        ])
        .assert()
        .success();

    let incremental_stdout = String::from_utf8(incremental.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&incremental_stdout).unwrap();

    assert_eq!(payload["summary"]["seed_count"], 3);
    assert_eq!(payload["summary"]["queried_count"], 1);
    assert_eq!(payload["summary"]["carried_forward_count"], 2);
    assert_eq!(payload["summary"]["resolved_count"], 3);
    assert_eq!(payload["summary"]["unresolved_count"], 0);
    assert_eq!(payload["summary"]["failure_count"], 0);

    let registry_json: Value =
        serde_json::from_str(&std::fs::read_to_string(output_dir.join("registry.json")).unwrap())
            .unwrap();
    assert_eq!(registry_json["version"], "2026.03.14");
    assert_eq!(registry_json["entry_count"], 3);

    let mapping_entries: Value = serde_json::from_str(
        &std::fs::read_to_string(output_dir.join("cusip-to-mock.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(mapping_entries.as_array().unwrap().len(), 3);
    assert_eq!(mapping_entries[0]["input"], "AAPL");
    assert_eq!(mapping_entries[1]["input"], "MSFT");
    assert_eq!(mapping_entries[2]["input"], "NVDA");
}

#[test]
fn test_registry_lint_cli_json_and_summary_output() {
    let registry_dir = tempdir().unwrap();
    write_registry_metadata(registry_dir.path(), "lint-test", "1.0.0", 2);
    write_mapping_file(
        registry_dir.path(),
        "mappings.json",
        serde_json::json!([
            {"input":"A","canonical_id":"C1","canonical_type":"entity","rule_id":"r1"},
            {"input":"A","canonical_id":"C2","canonical_type":"entity","rule_id":"r2"}
        ]),
    );

    let json = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "lint",
            registry_dir.path().to_str().unwrap(),
            "--profile",
            "standard",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(json.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["version"], "canon_registry_lint.v0");
    assert_eq!(payload["profile"], "standard");
    assert_eq!(payload["summary"]["warnings"], 1);
    assert_eq!(payload["findings"][0]["code"], "index_missing");
    assert_eq!(payload["findings"][1]["code"], "shadowed_input");

    let summary = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "lint",
            registry_dir.path().to_str().unwrap(),
            "--profile",
            "standard",
            "--emit",
            "summary",
        ])
        .assert()
        .success();
    let summary_stdout = String::from_utf8(summary.get_output().stdout.clone()).unwrap();
    assert!(summary_stdout.contains("lint-test@1.0.0 lint standard"));
}

#[test]
fn test_registry_build_refuses_non_incremental_overwrite() {
    let temp_dir = tempdir().unwrap();
    let seed_path = temp_dir.path().join("seed.csv");
    let output_dir = temp_dir.path().join("registries/mock-cusip");

    write_seed_csv(&seed_path, "cusip\nAAPL\n");
    std::fs::create_dir_all(&output_dir).unwrap();
    std::fs::write(output_dir.join("existing.txt"), "occupied").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "build",
            "--source",
            "mock",
            "--seed",
            seed_path.to_str().unwrap(),
            "--seed-column",
            "cusip",
            "--output",
            output_dir.to_str().unwrap(),
            "--version",
            "2026.03.13",
        ])
        .assert()
        .code(2);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_IO");
    assert!(
        payload["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("refuse to overwrite in place")
    );
}

#[test]
fn test_registry_build_openfigi_provider_materializes_registry() {
    let response_body = serde_json::json!([
        {
            "data": [{
                "figi": "BBG000B9XRY4",
                "compositeFIGI": "BBG000B9XRY4",
                "ticker": "AAPL",
                "name": "APPLE INC",
                "securityType": "Common Stock"
            }]
        },
        {
            "data": [{
                "figi": "BBG000BPH459",
                "compositeFIGI": "BBG000BPH459",
                "ticker": "MSFT",
                "name": "MICROSOFT CORP",
                "securityType": "Common Stock"
            }]
        }
    ])
    .to_string();
    let (base_url, server_handle) = spawn_openfigi_server(response_body);

    let temp_dir = tempdir().unwrap();
    let seed_path = temp_dir.path().join("seed.csv");
    let output_dir = temp_dir.path().join("registries/openfigi-cusip");
    let resolve_path = temp_dir.path().join("resolve.csv");
    let base_url_arg = format!("base_url={base_url}");

    write_seed_csv(&seed_path, "cusip\n037833100\n594918104\n");
    write_seed_csv(&resolve_path, "cusip\n037833100\n594918104\n");

    let build = Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("OPENFIGI_API_KEY", "env-api-key")
        .args([
            "registry",
            "build",
            "--source",
            "openfigi",
            "--seed",
            seed_path.to_str().unwrap(),
            "--seed-column",
            "cusip",
            "--provider-config",
            &base_url_arg,
            "--output",
            output_dir.to_str().unwrap(),
            "--version",
            "2026.03.14",
        ])
        .assert()
        .success();

    let build_stdout = String::from_utf8(build.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&build_stdout).unwrap();
    assert_eq!(payload["registry"]["id"], "openfigi-cusip");
    assert_eq!(payload["summary"]["resolved_count"], 2);
    assert_eq!(payload["summary"]["api_calls"], 1);
    assert_eq!(payload["summary"]["failure_count"], 0);
    assert_eq!(
        payload["files"],
        serde_json::json!([
            "cusip-to-figi.json",
            "cusip-to-name.json",
            "cusip-to-ticker.json"
        ])
    );

    let build_file: Value =
        serde_json::from_str(&std::fs::read_to_string(output_dir.join("_build.json")).unwrap())
            .unwrap();
    assert_eq!(build_file["provider"]["options"]["base_url"], base_url);
    assert!(build_file["timing"]["elapsed_ms"].as_u64().is_some());

    let (request_body, headers) = server_handle.join().unwrap();
    assert!(request_body.contains("\"idType\":\"ID_CUSIP\""));
    assert!(request_body.contains("\"idValue\":\"037833100\""));
    assert_eq!(
        headers.get("x-openfigi-apikey").map(String::as_str),
        Some("env-api-key")
    );

    let resolve = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg(&resolve_path)
        .arg("--registry")
        .arg(&output_dir)
        .arg("--column")
        .arg("cusip")
        .arg("--explicit")
        .assert()
        .success();

    let resolve_stdout = String::from_utf8(resolve.get_output().stdout.clone()).unwrap();
    let resolve_json: Value = serde_json::from_str(&resolve_stdout).unwrap();
    assert_eq!(resolve_json["outcome"], "RESOLVED");
    assert_eq!(
        resolve_json["mappings"][0]["canonical_id"],
        "u8:BBG000B9XRY4"
    );
    assert_eq!(
        resolve_json["mappings"][1]["canonical_id"],
        "u8:BBG000BPH459"
    );
}

#[test]
fn test_registry_build_openfigi_provider_passes_mapping_filters() {
    let response_body = serde_json::json!([
        {
            "data": [{
                "figi": "BBG000BPH459",
                "compositeFIGI": "BBG000BPH459",
                "shareClassFIGI": "BBG001S5TD05",
                "ticker": "MSFT",
                "name": "MICROSOFT CORP",
                "exchCode": "US",
                "securityType": "Common Stock",
                "securityType2": "Common Stock",
                "marketSector": "Equity"
            }]
        }
    ])
    .to_string();
    let (base_url, server_handle) = spawn_openfigi_server(response_body);

    let temp_dir = tempdir().unwrap();
    let seed_path = temp_dir.path().join("seed.csv");
    let output_dir = temp_dir.path().join("registries/openfigi-isin");
    let base_url_arg = format!("base_url={base_url}");

    write_seed_csv(&seed_path, "isin\nUS5949181045\n");

    let build = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "build",
            "--source",
            "openfigi",
            "--seed",
            seed_path.to_str().unwrap(),
            "--seed-column",
            "isin",
            "--provider-config",
            "id_type=ID_ISIN",
            "--provider-config",
            &base_url_arg,
            "--provider-config",
            "exchCode=US",
            "--provider-config",
            "marketSecDes=Equity",
            "--provider-config",
            "securityType2=Common Stock",
            "--output",
            output_dir.to_str().unwrap(),
            "--version",
            "2026.06.09",
        ])
        .assert()
        .success();

    let build_stdout = String::from_utf8(build.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&build_stdout).unwrap();
    assert_eq!(payload["registry"]["id"], "openfigi-isin");
    assert_eq!(payload["summary"]["resolved_count"], 1);
    assert_eq!(payload["summary"]["failure_count"], 0);

    let figi_entries: Value = serde_json::from_str(
        &std::fs::read_to_string(output_dir.join("isin-to-figi.json")).unwrap(),
    )
    .unwrap();
    let figi_entry = figi_entries
        .as_array()
        .and_then(|entries| entries.first())
        .unwrap();
    assert_eq!(figi_entry["input"], "US5949181045");
    assert_eq!(figi_entry["canonical_id"], "BBG000BPH459");
    assert_eq!(figi_entry["canonical_type"], "composite_figi");

    let build_file: Value =
        serde_json::from_str(&std::fs::read_to_string(output_dir.join("_build.json")).unwrap())
            .unwrap();
    assert_eq!(build_file["provider"]["options"]["id_type"], "ID_ISIN");
    assert_eq!(build_file["provider"]["options"]["exchCode"], "US");
    assert_eq!(build_file["provider"]["options"]["marketSecDes"], "Equity");
    assert_eq!(
        build_file["provider"]["options"]["securityType2"],
        "Common Stock"
    );

    let (request_body, _) = server_handle.join().unwrap();
    let request_json: Value = serde_json::from_str(&request_body).unwrap();
    let request_job = request_json
        .as_array()
        .and_then(|jobs| jobs.first())
        .unwrap();
    assert_eq!(request_job["idType"], "ID_ISIN");
    assert_eq!(request_job["idValue"], "US5949181045");
    assert_eq!(request_job["exchCode"], "US");
    assert_eq!(request_job["marketSecDes"], "Equity");
    assert_eq!(request_job["securityType2"], "Common Stock");
}

#[test]
fn test_registry_build_openfigi_incremental_fetches_only_missing_identifiers() {
    let response_body = serde_json::json!([
        {
            "data": [{
                "figi": "BBG000BPH459",
                "compositeFIGI": "BBG000BPH459",
                "ticker": "MSFT",
                "name": "MICROSOFT CORP",
                "securityType": "Common Stock"
            }]
        }
    ])
    .to_string();
    let (base_url, server_handle) = spawn_openfigi_server(response_body);

    let temp_dir = tempdir().unwrap();
    let seed_path = temp_dir.path().join("seed.csv");
    let output_dir = temp_dir.path().join("registries/openfigi-cusip");
    let base_url_arg = format!("base_url={base_url}");
    write_seed_csv(&seed_path, "cusip\n037833100\n594918104\n");
    std::fs::create_dir_all(&output_dir).unwrap();
    write_registry_metadata(&output_dir, "openfigi-cusip", "2026.06.01", 1);
    write_mapping_file(
        &output_dir,
        "cusip-to-figi.json",
        serde_json::json!([
            {
                "input": "037833100",
                "canonical_id": "BBG000B9XRY4",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI_CUSIP_TO_COMPOSITE_FIGI"
            }
        ]),
    );

    let build = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "build",
            "--source",
            "openfigi",
            "--seed",
            seed_path.to_str().unwrap(),
            "--seed-column",
            "cusip",
            "--provider-config",
            &base_url_arg,
            "--output",
            output_dir.to_str().unwrap(),
            "--version",
            "2026.06.09",
            "--incremental",
        ])
        .assert()
        .success();

    let build_stdout = String::from_utf8(build.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&build_stdout).unwrap();
    assert_eq!(payload["summary"]["seed_count"], 2);
    assert_eq!(payload["summary"]["carried_forward_count"], 1);
    assert_eq!(payload["summary"]["queried_count"], 1);
    assert_eq!(payload["summary"]["resolved_count"], 2);
    assert_eq!(payload["summary"]["api_calls"], 1);

    let build_file: Value =
        serde_json::from_str(&std::fs::read_to_string(output_dir.join("_build.json")).unwrap())
            .unwrap();
    assert_eq!(build_file["summary"]["carried_forward_count"], 1);
    assert_eq!(build_file["summary"]["queried_count"], 1);
    assert_eq!(build_file["summary"]["resolved_count"], 2);

    let (request_body, _) = server_handle.join().unwrap();
    assert!(!request_body.contains("\"idValue\":\"037833100\""));
    assert!(request_body.contains("\"idValue\":\"594918104\""));
}

#[cfg(unix)]
#[test]
fn test_registry_build_openfigi_provider_materializes_registry_with_twinning_stub() {
    let twinning = twinning_bin();
    if !twinning.exists() {
        eprintln!(
            "skipping twinning-backed OpenFIGI smoke; expected twinning binary at {} or set TWINNING_BIN",
            twinning.display()
        );
        return;
    }

    let spec_path =
        fixture_path("../twinning/tests/fixtures/rest/openfigi_v2_v3/response-stub-schema.yaml");
    if !spec_path.exists() {
        eprintln!(
            "skipping twinning-backed OpenFIGI smoke; expected OpenFIGI response-stub fixture at {}",
            spec_path.display()
        );
        return;
    }

    let temp_dir = tempdir().unwrap();
    let seed_path = temp_dir.path().join("seed.csv");
    let output_dir = temp_dir.path().join("registries/openfigi-cusip");
    let report_path = temp_dir.path().join("twinning-rest-report.json");
    let preflight_report_path = temp_dir.path().join("twinning-rest-preflight.json");
    let preflight = std::process::Command::new(&twinning)
        .args([
            "rest",
            "--json",
            "--spec",
            spec_path.to_str().unwrap(),
            "--server-variable",
            "basePath=v3",
            "--auth-mode",
            "shape",
            "--report",
            preflight_report_path.to_str().unwrap(),
            "--run",
            "true",
        ])
        .output()
        .unwrap();
    if !preflight.status.success() {
        eprintln!(
            "skipping twinning-backed OpenFIGI smoke; twinning REST runtime is unavailable: {}",
            String::from_utf8_lossy(&preflight.stdout)
        );
        return;
    }
    write_seed_csv(&seed_path, "cusip\n037833100\n");

    let child_command = format!(
        "{} registry build --source openfigi --seed {} --seed-column cusip --provider-config id_type=ID_CUSIP --provider-config api_key=stub-key --provider-config exchCode=US --provider-config base_url=\"$TWIN_BASE_URL/v3/mapping\" --output {} --version 2026.06.09",
        shell_quote(env!("CARGO_BIN_EXE_canon")),
        shell_quote(&seed_path),
        shell_quote(&output_dir),
    );

    let twin_run = Command::new(&twinning)
        .args([
            "rest",
            "--json",
            "--spec",
            spec_path.to_str().unwrap(),
            "--server-variable",
            "basePath=v3",
            "--auth-mode",
            "shape",
            "--report",
            report_path.to_str().unwrap(),
            "--run",
            &child_command,
        ])
        .assert()
        .success();

    let twin_stdout = String::from_utf8(twin_run.get_output().stdout.clone()).unwrap();
    let twin_payload: Value = serde_json::from_str(&twin_stdout).unwrap();
    assert_eq!(twin_payload["version"], "twinning.rest-run.v0");
    assert_eq!(twin_payload["child"]["exit_code"], 0);
    assert!(
        twin_payload["child"]["command"]
            .as_str()
            .unwrap()
            .contains("registry build --source openfigi")
    );

    let build_file: Value =
        serde_json::from_str(&std::fs::read_to_string(output_dir.join("_build.json")).unwrap())
            .unwrap();
    assert_eq!(build_file["summary"]["queried_count"], 1);
    assert_eq!(build_file["summary"]["resolved_count"], 1);
    assert_eq!(build_file["summary"]["unresolved_count"], 0);
    assert_eq!(build_file["summary"]["failure_count"], 0);
    assert_eq!(build_file["summary"]["api_calls"], 1);
    assert_eq!(
        build_file["provider"]["options"]["api_key"],
        serde_json::json!("[REDACTED]")
    );
    let base_url = build_file["provider"]["options"]["base_url"]
        .as_str()
        .unwrap();
    assert!(base_url.starts_with("http://127.0.0.1:"));
    assert!(!base_url.contains("api.openfigi.com"));

    let figi_entries: Value = serde_json::from_str(
        &std::fs::read_to_string(output_dir.join("cusip-to-figi.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(figi_entries[0]["input"], "037833100");
    assert_eq!(figi_entries[0]["canonical_id"], "BBG000B9XRY4");
    assert_eq!(figi_entries[0]["canonical_type"], "composite_figi");

    let report: Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).unwrap()).unwrap();
    assert_eq!(report["version"], "twinning.rest-report.v0");
    assert_eq!(report["session"]["request_count"], 1);
    assert_eq!(
        report["session"]["response_stubs"]["openfigi_cusip_success_us"],
        1
    );

    let resolve = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg(&seed_path)
        .arg("--registry")
        .arg(&output_dir)
        .arg("--column")
        .arg("cusip")
        .arg("--explicit")
        .assert()
        .success();
    let resolve_stdout = String::from_utf8(resolve.get_output().stdout.clone()).unwrap();
    let resolve_json: Value = serde_json::from_str(&resolve_stdout).unwrap();
    assert_eq!(resolve_json["outcome"], "RESOLVED");
    assert_eq!(
        resolve_json["mappings"][0]["canonical_id"],
        "u8:BBG000B9XRY4"
    );
}

#[cfg(unix)]
#[test]
fn test_non_utf8_input_path_does_not_panic() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let input_path = temp_dir
        .path()
        .join(OsString::from_vec(b"input-\xFF.csv".to_vec()));
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg(&input_path)
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .assert()
        .code(2)
        .stdout(predicate::str::contains("E_IO"));
}
