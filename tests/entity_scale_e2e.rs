#![forbid(unsafe_code)]

use canon::entity::{
    index::EntityIndexCacheStatus,
    run::{NativeScaleProofConfig, NativeScaleStageMetric, prove_native_engine_scale_offline},
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

#[test]
fn native_engine_scale_proof_runs_500k_offline_and_deterministic() {
    let config = NativeScaleProofConfig::offline_500k();
    let proof = prove_native_engine_scale_offline(config.clone()).expect("native scale proof");
    let replay = prove_native_engine_scale_offline(config).expect("native scale replay");

    assert_eq!(proof.artifact_content_hash, replay.artifact_content_hash);
    assert_eq!(
        serde_json::to_value(&proof).expect("proof json"),
        serde_json::to_value(&replay).expect("replay json")
    );

    assert_eq!(proof.intake.observation_count, 500_000);
    assert_eq!(proof.intake.source_count, 5);
    assert_eq!(proof.intake.entity_count, 512);
    assert_eq!(proof.intake.unique_surface_count, 5_120);
    assert!(proof.intake.duplicate_observation_count > 490_000);
    assert!(proof.intake.max_live_surface_records < proof.intake.observation_count);
    assert!(proof.intake.unicode_observation_count > 0);
    assert!(proof.intake.sparse_anchor_observation_count > 0);
    assert!(proof.intake.hard_negative_observation_count > 0);
    assert_eq!(
        proof.intake.multisource_entity_count,
        proof.intake.entity_count
    );

    assert!(!proof.offline.network_required);
    assert!(!proof.offline.python_required);
    assert!(!proof.offline.adapter_required);
    assert!(proof.generator.deterministic);

    assert_eq!(
        proof.cache.cold_cache_status,
        EntityIndexCacheStatus::Rebuilt
    );
    assert_eq!(proof.cache.warm_cache_status, EntityIndexCacheStatus::Hit);
    assert_eq!(
        proof.cache.changed_input_cache_status,
        EntityIndexCacheStatus::Miss
    );
    assert_ne!(
        proof.cache.cold_cache_key_hash,
        proof.cache.changed_input_cache_key_hash
    );
    assert_eq!(proof.cache.changed_fields, ["input_hash"]);
    assert_eq!(proof.cache.invalidated_layers, ["ngram_postings"]);

    assert_eq!(proof.index.surface_count, proof.intake.unique_surface_count);
    assert!(proof.index.token_count > 0);
    assert!(proof.index.ngram_count > 0);
    assert!(proof.index.total_ngram_posting_count > 0);
    assert_eq!(proof.index.exact_bucket_pair_expansion_count, 0);
    assert!(proof.index.suppressed_exact_view_pair_count > 0);
    assert_eq!(proof.index.cache_status, EntityIndexCacheStatus::Rebuilt);
    assert!(proof.index.cache_reusable);

    assert!(proof.block.candidate_record_count > 0);
    assert!(proof.block.candidate_pairs_emitted > 0);
    assert!(proof.block.candidate_budget_validated);
    assert!(!proof.block.partial_candidate_artifact_written);
    assert_eq!(
        proof.budget_refusal.refusal_code,
        "E_ENTITY_CANDIDATE_BUDGET"
    );
    assert_eq!(proof.budget_refusal.stage, "block");
    assert_eq!(proof.budget_refusal.observed, 2);
    assert_eq!(proof.budget_refusal.configured, 1);
    assert!(!proof.budget_refusal.candidate_artifact_written);
    assert!(!proof.budget_refusal.partial_candidate_artifact_written);

    assert!(proof.edge_record_count > 0);
    assert!(proof.solve.graph_surface_node_count > 0);
    assert!(proof.solve.support_edge_count > 0);
    assert!(proof.solve.hard_cannot_link_edge_count > 0);
    assert!(proof.solve.solved_component_count > 0);
    assert_eq!(
        proof.artifact_publication.deterministic_content_hash,
        proof.artifact_content_hash
    );
    assert!(proof.artifact_publication.artifact_bytes > 0);
    assert_eq!(
        proof.artifact_publication.artifact_bytes,
        proof.artifact_publication.disk_write_bytes
    );
    assert_stage_metrics(&proof.stage_metrics);

    let proof_json: Value = serde_json::to_value(&proof).expect("proof json");
    assert_eq!(proof_json["version"], "canon_entity_native_scale_proof.v0");
    assert!(
        proof_json["artifact_content_hash"]
            .as_str()
            .expect("content hash")
            .starts_with("blake3:")
    );
}

#[test]
fn entity_run_cli_completes_500k_generated_tier_offline() {
    let fixture = NativeScaleCliFixture::new(500_000, 128, 2);
    let output = assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "run",
            path_str(&fixture.rows),
            "--profile",
            "cmbs_tenant_label",
            "--strategy",
            path_str(&fixture.strategy),
            "--registry",
            path_str(&fixture.registry),
            "--work-dir",
            path_str(&fixture.work_dir),
            "--no-witness",
            "--emit",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let artifact: Value = serde_json::from_slice(&output).expect("entity run artifact json");
    assert_eq!(artifact["version"], "canon_entity_run.v1");
    assert_eq!(artifact["summary"]["counts"]["row_count"], 500_000);
    assert_eq!(artifact["summary"]["counts"]["prepared_surfaces"], 256);
    assert_eq!(artifact["summary"]["counts"]["index_surfaces"], 256);
    assert!(
        artifact["summary"]["counts"]["candidate_pairs"]
            .as_u64()
            .expect("candidate pairs")
            > 0
    );
    assert!(
        artifact["artifact_content_hash"]
            .as_str()
            .expect("run content hash")
            .starts_with("blake3:")
    );
    assert!(fixture.work_dir.join("run").join("run.json").exists());
    assert!(fixture.work_dir.join("index").join("index.json").exists());
    assert!(fixture.work_dir.join("index").join("postings.bin").exists());
    assert!(
        fixture
            .work_dir
            .join("block")
            .join("candidates.jsonl")
            .exists()
    );
    assert!(
        fixture
            .work_dir
            .join("evidence")
            .join("evidence.json")
            .exists()
    );
    assert!(
        fixture
            .work_dir
            .join("evidence")
            .join("evidence.jsonl")
            .exists()
    );
    assert!(fixture.work_dir.join("solve").join("solve.json").exists());
}

fn assert_stage_metrics(metrics: &[NativeScaleStageMetric]) {
    let stages = metrics
        .iter()
        .map(|metric| metric.stage.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        stages,
        BTreeSet::from([
            "artifact_publication",
            "block",
            "evidence",
            "index",
            "intake",
            "review",
            "solve",
        ])
    );
    for metric in metrics {
        assert_eq!(metric.wall_time_ms, None);
        assert_eq!(metric.peak_rss_bytes, None);
    }
}

struct NativeScaleCliFixture {
    _temp: tempfile::TempDir,
    rows: PathBuf,
    registry: PathBuf,
    strategy: PathBuf,
    work_dir: PathBuf,
}

impl NativeScaleCliFixture {
    fn new(row_count: u64, entity_count: u64, variants_per_entity: u64) -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let rows = temp.path().join("native_scale_rows.csv");
        let registry = temp.path().join("registry");
        let strategy = temp.path().join("strategy.yaml");
        let work_dir = temp.path().join("work");
        write_native_scale_rows(&rows, row_count, entity_count, variants_per_entity);
        write_registry(&registry);
        fs::write(
            &strategy,
            "strategy_id: native_scale_cli.v1\nstrategy_version: 1.0.0\n",
        )
        .expect("strategy");

        Self {
            _temp: temp,
            rows,
            registry,
            strategy,
            work_dir,
        }
    }
}

fn write_native_scale_rows(
    path: &Path,
    row_count: u64,
    entity_count: u64,
    variants_per_entity: u64,
) {
    let file = File::create(path).expect("rows file");
    let mut writer = BufWriter::new(file);
    writeln!(
        writer,
        "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json"
    )
    .expect("header");
    for row_number in 0..row_count {
        let entity_ordinal = row_number % entity_count;
        let variant_ordinal = (row_number / entity_count) % variants_per_entity;
        let source_ordinal = (row_number / (entity_count * variants_per_entity)) % 5;
        writeln!(
            writer,
            "row-{row_number:06},D{source_ordinal:02},L{entity_ordinal:05},P{variant_ordinal:02},Native Entity {entity_ordinal:05} Variant {variant_ordinal:02},[],[]"
        )
        .expect("row");
    }
    writer.flush().expect("flush rows");
}

fn write_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"entity-scale-registry","version":"2026.07.11","description":"entity scale test registry","updated":"2026-07-11","entry_count":0}"#,
    )
    .expect("registry metadata");
    fs::write(registry.join("aliases.json"), "[]\n").expect("aliases");
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("path utf-8")
}
