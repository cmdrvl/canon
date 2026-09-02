#![forbid(unsafe_code)]

use canon::entity::{
    block::{BlockCandidateGenerationDiagnostics, EntityBlockStageRequest},
    block_preflight::{
        BlockPreflightBudgetStatus, EntityBlockPreflightRequest, run_block_preflight,
    },
    run::run_entity_block_stage,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
fn preflight_exact_sample_matches_block_stage_diagnostics_and_reports_skew() {
    let fixture = PreflightFixture::new();
    let report = run_block_preflight(EntityBlockPreflightRequest {
        rows: &fixture.rows,
        profile: "cmbs_tenant_label",
        strategy: &fixture.strategy,
        sample_pct: 100,
        work_dir: None,
    })
    .expect("preflight report");

    let stage_work = fixture.root.join("stage-work");
    let stage_output = run_entity_block_stage(EntityBlockStageRequest {
        rows: &fixture.rows,
        profile: "cmbs_tenant_label",
        strategy: &fixture.strategy,
        registry: &fixture.registry,
        work_dir: &stage_work,
    })
    .expect("block stage");
    let diagnostics: BlockCandidateGenerationDiagnostics =
        read_json(&stage_work.join("block/diagnostics.json"));

    assert_eq!(
        report.totals.observed_candidate_record_count,
        diagnostics.candidate_record_count
    );
    assert_eq!(
        report.totals.observed_candidate_record_count,
        stage_output.candidates.len() as u64
    );
    assert_eq!(
        report.totals.observed_candidate_pairs_emitted,
        diagnostics.candidate_pairs_emitted
    );
    assert_eq!(
        report.totals.observed_max_candidates_for_surface,
        diagnostics.max_candidates_for_surface
    );
    assert_eq!(
        report.totals.observed_max_candidates_for_operator,
        diagnostics.max_candidates_for_operator
    );
    assert_eq!(
        report.totals.estimated_candidate_pairs_emitted, diagnostics.candidate_pairs_emitted,
        "100 percent sampling must be exact, not an estimate"
    );
    assert_eq!(
        report.budget_verdict.status,
        BlockPreflightBudgetStatus::Pass
    );
    assert!(report.operators.iter().any(|operator| {
        operator.operator_id == "ngram_topk:run"
            && operator.observed_cumulative_candidate_count
                >= operator.observed_marginal_candidate_count
    }));

    let john_smith = report
        .top_blocks
        .iter()
        .find(|block| {
            block.operator_id == "exact_view:tenant_core" && block.key_value == "john smith"
        })
        .expect("dominant exact bucket appears");
    assert_eq!(john_smith.observed_row_count, 4);
    assert_eq!(john_smith.estimated_row_count, 4);
}

#[test]
fn preflight_sampling_is_hash_mod_deterministic_under_row_shuffle() {
    let fixture = PreflightFixture::new();
    let original = run_block_preflight(EntityBlockPreflightRequest {
        rows: &fixture.rows,
        profile: "cmbs_tenant_label",
        strategy: &fixture.strategy,
        sample_pct: 50,
        work_dir: None,
    })
    .expect("original preflight");
    let shuffled = run_block_preflight(EntityBlockPreflightRequest {
        rows: &fixture.shuffled_rows,
        profile: "cmbs_tenant_label",
        strategy: &fixture.strategy,
        sample_pct: 50,
        work_dir: None,
    })
    .expect("shuffled preflight");

    assert_eq!(
        original.sample.sampled_row_count,
        shuffled.sample.sampled_row_count
    );
    assert_eq!(
        original.sample.sampled_surface_count,
        shuffled.sample.sampled_surface_count
    );
    assert_eq!(original.totals, shuffled.totals);
    assert_eq!(original.operators, shuffled.operators);
    assert_eq!(original.top_blocks, shuffled.top_blocks);
    assert_eq!(original.budget_verdict, shuffled.budget_verdict);
}

#[test]
fn preflight_budget_verdict_flips_between_pass_tight_and_would_refuse() {
    let fixture = PreflightFixture::new();
    let pass = report_for_strategy(&fixture, &fixture.strategy);
    assert_eq!(pass.budget_verdict.status, BlockPreflightBudgetStatus::Pass);

    let tight_strategy = fixture.write_strategy(
        "tight-strategy.yaml",
        r#"
strategy_id: preflight-tight
strategy_version: 1
block:
  index_budget:
    max_exact_bucket_size: 4
"#,
    );
    let tight = report_for_strategy(&fixture, &tight_strategy);
    assert_eq!(
        tight.budget_verdict.status,
        BlockPreflightBudgetStatus::Tight
    );
    assert!(tight.budget_verdict.checks.iter().any(|check| {
        check.policy_id == "block.max_exact_bucket_size"
            && check.status == BlockPreflightBudgetStatus::Tight
            && check.estimated == 4
            && check.configured == 4
    }));

    let refusal_strategy = fixture.write_strategy(
        "refusal-strategy.yaml",
        r#"
strategy_id: preflight-refusal
strategy_version: 1
block:
  index_budget:
    max_exact_bucket_size: 3
"#,
    );
    let would_refuse = report_for_strategy(&fixture, &refusal_strategy);
    assert_eq!(
        would_refuse.budget_verdict.status,
        BlockPreflightBudgetStatus::WouldRefuse
    );
    assert!(would_refuse.budget_verdict.checks.iter().any(|check| {
        check.policy_id == "block.max_exact_bucket_size"
            && check.status == BlockPreflightBudgetStatus::WouldRefuse
            && check.estimated == 4
            && check.configured == 3
    }));
}

#[test]
fn preflight_is_read_only_without_work_dir_and_writes_one_artifact_with_work_dir() {
    let fixture = PreflightFixture::new();
    let before = recursive_files(&fixture.root);
    let report = run_block_preflight(EntityBlockPreflightRequest {
        rows: &fixture.rows,
        profile: "cmbs_tenant_label",
        strategy: &fixture.strategy,
        sample_pct: 100,
        work_dir: None,
    })
    .expect("stdout-only preflight");
    let after = recursive_files(&fixture.root);
    assert_eq!(before, after);
    assert!(report.artifact_path.is_none());

    let work_dir = fixture.root.join("preflight-work");
    fs::create_dir_all(&work_dir).expect("work dir");
    let report = run_block_preflight(EntityBlockPreflightRequest {
        rows: &fixture.rows,
        profile: "cmbs_tenant_label",
        strategy: &fixture.strategy,
        sample_pct: 100,
        work_dir: Some(&work_dir),
    })
    .expect("artifact preflight");
    assert_eq!(
        recursive_files(&work_dir),
        vec!["block_preflight.json".to_string()]
    );
    let artifact: Value = read_json(&work_dir.join("block_preflight.json"));
    assert_eq!(artifact["version"], "canon_entity_block_preflight.v1");
    assert_eq!(
        artifact["artifact_path"],
        Value::String(report.artifact_path.unwrap())
    );
}

fn report_for_strategy(
    fixture: &PreflightFixture,
    strategy: &Path,
) -> canon::entity::block_preflight::EntityBlockPreflightReport {
    run_block_preflight(EntityBlockPreflightRequest {
        rows: &fixture.rows,
        profile: "cmbs_tenant_label",
        strategy,
        sample_pct: 100,
        work_dir: None,
    })
    .expect("preflight report")
}

struct PreflightFixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    rows: PathBuf,
    shuffled_rows: PathBuf,
    strategy: PathBuf,
    registry: PathBuf,
}

impl PreflightFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().to_path_buf();
        let rows = root.join("rows.csv");
        let shuffled_rows = root.join("rows-shuffled.csv");
        let strategy = root.join("strategy.yaml");
        let registry = root.join("registry");
        fs::write(&rows, rows_csv()).expect("rows");
        fs::write(&shuffled_rows, shuffled_rows_csv()).expect("shuffled rows");
        fs::write(
            &strategy,
            r#"
strategy_id: preflight-default
strategy_version: 1
"#,
        )
        .expect("strategy");
        write_cmbs_registry(&registry);
        Self {
            _temp: temp,
            root,
            rows,
            shuffled_rows,
            strategy,
            registry,
        }
    }

    fn write_strategy(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::write(&path, contents).expect("strategy");
        path
    }
}

fn rows_csv() -> &'static str {
    "source_row_id,deal_id,loan_id,property_id,raw_tenant_name\n\
     r001,D1,L1,P1,John Smith LLC\n\
     r002,D2,L2,P2,John Smith LLC\n\
     r003,D3,L3,P3,John Smith LLC\n\
     r004,D4,L4,P4,John Smith LLC\n\
     r005,D5,L5,P5,Sears Roebuck\n\
     r006,D6,L6,P6,Sears Roebuck Store\n\
     r007,D7,L7,P7,Sears Auto Center\n\
     r008,D8,L8,P8,Kmart\n"
}

fn shuffled_rows_csv() -> &'static str {
    "source_row_id,deal_id,loan_id,property_id,raw_tenant_name\n\
     r006,D6,L6,P6,Sears Roebuck Store\n\
     r002,D2,L2,P2,John Smith LLC\n\
     r008,D8,L8,P8,Kmart\n\
     r004,D4,L4,P4,John Smith LLC\n\
     r001,D1,L1,P1,John Smith LLC\n\
     r007,D7,L7,P7,Sears Auto Center\n\
     r005,D5,L5,P5,Sears Roebuck\n\
     r003,D3,L3,P3,John Smith LLC\n"
}

fn write_cmbs_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"cmbs-tenants","version":"2026.06.25","description":"CMBS preflight test registry","updated":"2026-06-25","entry_count":2}"#,
    )
    .expect("registry metadata");
    fs::write(
        registry.join("aliases.json"),
        r#"[
  {"input":"Sears Roebuck","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
  {"input":"Kmart","canonical_id":"TNT-KMART","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"}
]"#,
    )
    .expect("aliases");
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    let bytes = fs::read(path).expect("read json");
    serde_json::from_slice(&bytes).expect("parse json")
}

fn recursive_files(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files);
    files.sort();
    files
}

fn collect_files(root: &Path, path: &Path, files: &mut Vec<String>) {
    for entry in fs::read_dir(path).expect("read dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else {
            files.push(
                path.strip_prefix(root)
                    .expect("relative path")
                    .display()
                    .to_string(),
            );
        }
    }
}
