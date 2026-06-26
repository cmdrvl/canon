#![forbid(unsafe_code)]

use canon::entity::run::{
    EntityRunBatchConfig, EntityRunRequest, run_entity_workbench_with_batching,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const RUNBOOK_JSON: &str =
    include_str!("../fixtures/entity/e2e/cmbs_backfill_runbook/runbook.json");
const STRESS_CONTRACT_JSON: &str =
    include_str!("../fixtures/entity/stress/generators/cmbs_500k_contract.json");

#[test]
fn entity_cmbs_backfill_runbook() {
    let runbook = runbook();
    assert_eq!(
        runbook.schema_version,
        "canon.entity.cmbs_backfill_runbook.v0"
    );
    assert_eq!(runbook.fixture_id, "cmbs-3000-deal-backfill-runbook-v0");
    assert_eq!(runbook.profile_id, "cmbs_tenant_label");
    assert_eq!(runbook.identity_semantics, "canonical_display_label");
    assert_eq!(runbook.canonical_type, "tenant_label");

    assert_batch_plan(&runbook);
    assert_stage_commands(&runbook);
    assert_required_log_fields(&runbook);
    assert_small_fixture_links(&runbook);
    assert_duplicate_mint_guard(&runbook);
    assert_stress_hook(&runbook);
}

#[test]
fn cmbs_physical_batch_global_surface_replay() {
    let runbook = runbook();
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    write_cmbs_registry(&registry);

    let deal_by_deal = run_case(
        temp.path(),
        "deal-by-deal",
        &registry,
        runbook.small_fixture.deal_by_deal_probe_rows_per_batch,
    );
    let batched = run_case(
        temp.path(),
        "batched",
        &registry,
        runbook.small_fixture.target_rows_per_physical_batch,
    );

    assert_eq!(
        deal_by_deal.artifact.summary.counts["row_count"],
        runbook.small_fixture.expected_rows
    );
    assert_eq!(
        deal_by_deal.artifact.summary.labels["batching_mode"],
        "physical_batches"
    );
    assert_eq!(
        batched.artifact.summary.labels["batching_mode"],
        "physical_batches"
    );
    assert_eq!(
        deal_by_deal.artifact.summary.counts["physical_batch_count"],
        runbook.small_fixture.expected_rows
    );
    assert!(
        batched.artifact.summary.counts["physical_batch_count"]
            < deal_by_deal.artifact.summary.counts["physical_batch_count"],
        "fixture must exercise distinct physical batch shapes"
    );

    assert_eq!(
        comparable_run_counts(&deal_by_deal.artifact.summary.counts),
        comparable_run_counts(&batched.artifact.summary.counts),
        "physical batches must feed the same global run counts"
    );
    assert_eq!(
        surface_fingerprints(&deal_by_deal.work_dir),
        surface_fingerprints(&batched.work_dir),
        "physical batches must feed one global prepared-surface corpus"
    );
    assert_eq!(
        candidate_fingerprints(&deal_by_deal.work_dir),
        candidate_fingerprints(&batched.work_dir),
        "candidate generation must use one global index view"
    );
    assert_candidate_caps(&runbook, &batched);
    assert_stage_hashes_and_next_commands(&batched);
    assert_cache_status(&runbook, &batched.work_dir);
    assert_same_workdir_rerun_is_byte_identical(&runbook, &registry, &batched.work_dir);
    assert_grouped_review_fixture(&runbook);
    assert_no_duplicate_sears_ids(&runbook, &deal_by_deal.work_dir);
    assert_no_duplicate_sears_ids(&runbook, &batched.work_dir);
}

#[derive(Debug, Deserialize)]
struct BackfillRunbook {
    schema_version: String,
    fixture_id: String,
    profile_id: String,
    identity_semantics: String,
    canonical_type: String,
    production_backfill: ProductionBackfill,
    small_fixture: SmallFixture,
    stage_commands: Vec<StageCommand>,
    required_log_fields: Vec<String>,
    candidate_caps: CandidateCaps,
    cache_contract: CacheContract,
    duplicate_mint_guard: DuplicateMintGuard,
    review_grouping: ReviewGrouping,
    stress_hooks: StressHooks,
}

#[derive(Debug, Deserialize)]
struct ProductionBackfill {
    deal_count: u64,
    tenant_row_count: u64,
    physical_batch_deal_count: u64,
    physical_batch_count: u64,
    batch_unit: String,
    logical_surface_corpus: String,
    logical_index_scope: String,
    registry_memory: String,
}

#[derive(Debug, Deserialize)]
struct SmallFixture {
    observations_path: String,
    strategy_path: String,
    review_queue_path: String,
    expected_summary_path: String,
    promotion_manifest_path: String,
    target_rows_per_physical_batch: u64,
    deal_by_deal_probe_rows_per_batch: u64,
    expected_rows: u64,
    expected_deals: u64,
    expected_raw_unique_names: u64,
    expected_review_group_count: u64,
}

#[derive(Debug, Deserialize)]
struct StageCommand {
    stage: String,
    command: String,
}

#[derive(Debug, Deserialize)]
struct CandidateCaps {
    candidate_pairs_per_unique_surface_p95_max: u64,
    candidate_pairs_per_unique_surface_p99_max: u64,
    exact_bucket_pair_expansion_count: u64,
    review_group_count_max: u64,
}

#[derive(Debug, Deserialize)]
struct CacheContract {
    keyed_layers: Vec<String>,
    unchanged_rerun: String,
    changed_profile_or_strategy: String,
    small_fixture_observed_index_status: String,
}

#[derive(Debug, Deserialize)]
struct DuplicateMintGuard {
    canonical_id: String,
    aliases: Vec<String>,
    forbidden_canonical_ids: Vec<String>,
    failure_mode: String,
}

#[derive(Debug, Deserialize)]
struct ReviewGrouping {
    sears_ambiguity: ReviewExpectation,
    china_king_ambiguity: ReviewExpectation,
}

#[derive(Debug, Deserialize)]
struct ReviewExpectation {
    review_group_id: String,
    reason_code: String,
    row_count: u64,
    deal_count: u64,
    expected_group_count: u64,
}

#[derive(Debug, Deserialize)]
struct StressHooks {
    contract_path: String,
    ci_probe_command: String,
    ignored_command: String,
    generated_static_artifact_policy: String,
}

#[derive(Debug)]
struct RunCase {
    work_dir: PathBuf,
    artifact: canon::entity::run::EntityRunArtifact,
}

fn runbook() -> BackfillRunbook {
    serde_json::from_str(RUNBOOK_JSON).expect("CMBS backfill runbook parses")
}

fn assert_batch_plan(runbook: &BackfillRunbook) {
    let plan = &runbook.production_backfill;
    assert_eq!(plan.deal_count, 3_000);
    assert_eq!(plan.tenant_row_count, 500_000);
    assert_eq!(plan.physical_batch_deal_count, 100);
    assert_eq!(
        plan.physical_batch_count,
        plan.deal_count / plan.physical_batch_deal_count
    );
    assert_eq!(plan.batch_unit, "deal");
    assert_eq!(plan.logical_surface_corpus, "global");
    assert_eq!(plan.logical_index_scope, "global");
    assert_eq!(plan.registry_memory, "global");
}

fn assert_stage_commands(runbook: &BackfillRunbook) {
    let stages = runbook
        .stage_commands
        .iter()
        .map(|command| command.stage.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        stages,
        [
            "prepare",
            "index",
            "block",
            "edge",
            "solve",
            "review_export",
            "audit",
            "review_import",
            "promote",
            "apply",
            "run_wrapper"
        ]
    );
    for command in &runbook.stage_commands {
        assert!(
            command.command.starts_with("canon entity "),
            "{} command is not operator-runnable: {}",
            command.stage,
            command.command
        );
    }
}

fn assert_required_log_fields(runbook: &BackfillRunbook) {
    let required = runbook
        .required_log_fields
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for field in [
        "row_count",
        "raw_unique_names",
        "prepared_surfaces",
        "index_surfaces",
        "candidate_pairs",
        "candidate_pairs_per_surface_p95",
        "candidate_pairs_per_surface_p99",
        "exact_bucket_pair_expansion_count",
        "solved_entities",
        "review_group_count",
        "anti_merge_groups",
        "cache_status",
        "artifact_hashes",
        "next_commands",
    ] {
        assert!(required.contains(field), "runbook omits log field {field}");
    }
    assert_eq!(
        runbook.cache_contract.keyed_layers,
        ["prepare".to_string(), "index".to_string()]
    );
    assert_eq!(
        runbook.cache_contract.unchanged_rerun,
        "reuse_hash_keyed_caches"
    );
    assert_eq!(
        runbook.cache_contract.changed_profile_or_strategy,
        "refuse_stale_artifacts"
    );
}

fn assert_small_fixture_links(runbook: &BackfillRunbook) {
    let rows = csv_rows(&fixture(&runbook.small_fixture.observations_path));
    let summary = json_file(&fixture(&runbook.small_fixture.expected_summary_path));
    assert_eq!(rows.len() as u64, runbook.small_fixture.expected_rows);
    assert_eq!(
        unique_count(&rows, "deal_id"),
        runbook.small_fixture.expected_deals
    );
    assert_eq!(
        unique_count(&rows, "raw_tenant_name"),
        runbook.small_fixture.expected_raw_unique_names
    );
    assert_eq!(
        summary["source"]["deal_count"],
        runbook.small_fixture.expected_deals
    );
    assert_eq!(
        summary["review_groups"]
            .as_array()
            .expect("review groups")
            .len() as u64,
        runbook.small_fixture.expected_review_group_count
    );
}

fn assert_duplicate_mint_guard(runbook: &BackfillRunbook) {
    assert_eq!(runbook.duplicate_mint_guard.canonical_id, "TNT-SEARS");
    assert_eq!(
        runbook.duplicate_mint_guard.failure_mode,
        "deal_by_deal_duplicate_minting"
    );
    let aliases = runbook
        .duplicate_mint_guard
        .aliases
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for alias in ["Sears", "SEARS LLC", "Sears Roebuck & Co."] {
        assert!(
            aliases.contains(alias),
            "missing duplicate guard alias {alias}"
        );
    }

    let promotion = json_file(&fixture(&runbook.small_fixture.promotion_manifest_path));
    for alias in promotion["expected_aliases"]
        .as_array()
        .expect("expected aliases")
    {
        if aliases.contains(alias["input"].as_str().expect("alias input")) {
            assert_eq!(
                alias["canonical_id"],
                runbook.duplicate_mint_guard.canonical_id
            );
        }
    }
}

fn assert_stress_hook(runbook: &BackfillRunbook) {
    assert_eq!(
        runbook.stress_hooks.contract_path,
        "tests/fixtures/entity/stress/generators/cmbs_500k_contract.json"
    );
    assert!(
        runbook
            .stress_hooks
            .ci_probe_command
            .contains("cmbs_500k_fixture_shape")
    );
    assert!(runbook.stress_hooks.ignored_command.contains("--ignored"));
    assert!(
        runbook
            .stress_hooks
            .ignored_command
            .contains("cmbs_500k_stress")
    );

    let stress: Value = serde_json::from_str(STRESS_CONTRACT_JSON).expect("stress contract parses");
    assert_eq!(
        stress["row_count"],
        runbook.production_backfill.tenant_row_count
    );
    assert_eq!(stress["deal_count"], runbook.production_backfill.deal_count);
    assert_eq!(
        stress["topk"]["candidate_cap"],
        runbook
            .candidate_caps
            .candidate_pairs_per_unique_surface_p95_max
    );
    assert_eq!(
        stress["expected"]["candidate_pairs_per_surface_p99_max"],
        runbook
            .candidate_caps
            .candidate_pairs_per_unique_surface_p99_max
    );
    assert_eq!(
        stress["expected"]["exact_bucket_pair_expansion_count"],
        runbook.candidate_caps.exact_bucket_pair_expansion_count
    );
    assert_eq!(
        stress["expected"]["review_group_count_max"],
        runbook.candidate_caps.review_group_count_max
    );
    assert_eq!(
        stress["expected"]["generated_static_artifact_policy"],
        runbook.stress_hooks.generated_static_artifact_policy
    );
}

fn run_case(temp_root: &Path, name: &str, registry: &Path, rows_per_batch: u64) -> RunCase {
    let work_dir = temp_root.join(name);
    let result = run_entity_workbench_with_batching(
        EntityRunRequest {
            rows: &fixture("tests/fixtures/entity/cmbs/small_book/observations.csv"),
            profile: "cmbs_tenant_label",
            strategy: &fixture("tests/fixtures/entity/profiles/cmbs_tenant_label.yaml"),
            registry,
            work_dir: &work_dir,
        },
        EntityRunBatchConfig::new(rows_per_batch),
    )
    .expect("CMBS backfill run succeeds");

    RunCase {
        work_dir,
        artifact: result.artifact,
    }
}

fn comparable_run_counts(counts: &BTreeMap<String, u64>) -> BTreeMap<&'static str, u64> {
    [
        "row_count",
        "prepared_surfaces",
        "exact_resolved_surfaces",
        "index_surfaces",
        "exact_bucket_count",
        "candidate_pairs",
        "edge_records",
        "relation_hint_edges",
        "solved_entities",
        "review_group_count",
    ]
    .into_iter()
    .map(|key| (key, counts.get(key).copied().unwrap_or_default()))
    .collect()
}

fn surface_fingerprints(work_dir: &Path) -> BTreeSet<String> {
    jsonl_values(&work_dir.join("prepare/surfaces.jsonl"))
        .into_iter()
        .map(|surface| {
            format!(
                "{}|{}|{}|{}|{}",
                surface["surface_id"].as_str().unwrap_or_default(),
                surface["surface_key"].as_str().unwrap_or_default(),
                surface["row_count"].as_u64().unwrap_or_default(),
                surface["deal_count"].as_u64().unwrap_or_default(),
                surface["exact_lookup"]["canonical_id"]
                    .as_str()
                    .unwrap_or_default()
            )
        })
        .collect()
}

fn candidate_fingerprints(work_dir: &Path) -> BTreeSet<String> {
    jsonl_values(&work_dir.join("block/candidates.jsonl"))
        .into_iter()
        .map(|candidate| {
            format!(
                "{}|{}|{}",
                candidate["left_surface_id"].as_str().unwrap_or_default(),
                candidate["right_surface_id"].as_str().unwrap_or_default(),
                candidate["operator_id"].as_str().unwrap_or_default()
            )
        })
        .collect()
}

fn assert_candidate_caps(runbook: &BackfillRunbook, case: &RunCase) {
    let counts = &case.artifact.summary.counts;
    let prepared = counts["prepared_surfaces"].max(1);
    assert!(
        counts["candidate_pairs"]
            <= prepared
                * runbook
                    .candidate_caps
                    .candidate_pairs_per_unique_surface_p99_max,
        "candidate pairs exceeded p99 cap"
    );

    let block = json_file(&case.work_dir.join("block/block.json"));
    assert_eq!(
        block["summary"]["counts"]["exact_bucket_pair_expansion_count"],
        runbook.candidate_caps.exact_bucket_pair_expansion_count
    );
    let exact_buckets = jsonl_values(&case.work_dir.join("block/exact_buckets.jsonl"));
    for bucket in exact_buckets {
        assert_eq!(bucket["pair_expansion"], "forbidden");
        assert!(
            bucket["diagnostics"]["suppressed_pair_count"]
                .as_u64()
                .is_some(),
            "exact bucket must log suppressed pair count: {bucket}"
        );
    }
}

fn assert_stage_hashes_and_next_commands(case: &RunCase) {
    for stage in &case.artifact.stage_artifacts {
        assert!(
            stage.artifact_content_hash.starts_with("blake3:"),
            "{} missing artifact hash",
            stage.stage
        );
    }
    for (key, command) in [
        ("resume", case.artifact.next_commands.resume.as_str()),
        (
            "review_export",
            case.artifact.next_commands.review_export.as_str(),
        ),
        ("audit", case.artifact.next_commands.audit.as_str()),
        ("promote", case.artifact.next_commands.promote.as_str()),
        ("apply", case.artifact.next_commands.apply.as_str()),
    ] {
        assert!(command.starts_with("canon entity "), "{key}: {command}");
    }
}

fn assert_cache_status(runbook: &BackfillRunbook, work_dir: &Path) {
    let index = json_file(&work_dir.join("index.json"));
    assert_eq!(
        index["summary"]["labels"]["cache_status"],
        runbook.cache_contract.small_fixture_observed_index_status
    );
}

fn assert_same_workdir_rerun_is_byte_identical(
    runbook: &BackfillRunbook,
    registry: &Path,
    work_dir: &Path,
) {
    let first = fs::read(work_dir.join("run.json")).expect("first run artifact bytes");
    let result = run_entity_workbench_with_batching(
        EntityRunRequest {
            rows: &fixture(&runbook.small_fixture.observations_path),
            profile: &runbook.profile_id,
            strategy: &fixture(&runbook.small_fixture.strategy_path),
            registry,
            work_dir,
        },
        EntityRunBatchConfig::new(runbook.small_fixture.target_rows_per_physical_batch),
    )
    .expect("CMBS backfill rerun succeeds");
    let second = fs::read(work_dir.join("run.json")).expect("second run artifact bytes");
    assert_eq!(
        first, second,
        "same work-dir rerun must replay deterministically"
    );
    assert!(result.artifact.artifact_content_hash.starts_with("blake3:"));
}

fn assert_grouped_review_fixture(runbook: &BackfillRunbook) {
    let rows = csv_rows(&fixture(&runbook.small_fixture.review_queue_path));
    assert_review_expectation(&rows, &runbook.review_grouping.sears_ambiguity);
    assert_review_expectation(&rows, &runbook.review_grouping.china_king_ambiguity);
}

fn assert_review_expectation(rows: &[BTreeMap<String, String>], expected: &ReviewExpectation) {
    let matches = rows
        .iter()
        .filter(|row| row["review_group_id"] == expected.review_group_id)
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len() as u64,
        expected.expected_group_count,
        "{} should be grouped once",
        expected.review_group_id
    );
    let row = matches.first().expect("review group exists");
    assert_eq!(row["reason_code"], expected.reason_code);
    assert_eq!(parse_u64(row, "row_count"), expected.row_count);
    assert_eq!(parse_u64(row, "deal_count"), expected.deal_count);
}

fn assert_no_duplicate_sears_ids(runbook: &BackfillRunbook, work_dir: &Path) {
    let artifact_text = read_tree_text(work_dir);
    assert!(
        artifact_text.contains(&runbook.duplicate_mint_guard.canonical_id),
        "fixture must exercise the Sears canonical ID"
    );
    for forbidden in &runbook.duplicate_mint_guard.forbidden_canonical_ids {
        assert!(
            !artifact_text.contains(forbidden),
            "deal-by-deal duplicate mint appeared in artifacts: {forbidden}"
        );
    }
}

fn read_tree_text(path: &Path) -> String {
    let mut text = String::new();
    for entry in fs::read_dir(path).expect("directory reads") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            text.push_str(&read_tree_text(&path));
        } else if let Ok(contents) = fs::read_to_string(&path) {
            text.push_str(&contents);
        }
    }
    text
}

fn write_cmbs_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"cmbs-tenants","version":"2026.06.26","description":"CMBS backfill runbook registry","updated":"2026-06-26","entry_count":8}"#,
    )
    .expect("registry metadata");
    fs::write(
        registry.join("aliases.json"),
        serde_json::to_string_pretty(&serde_json::json!([
            {"input":"Sears","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"SEARS LLC","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"Sears Roebuck & Co.","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 Hour Fitness","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 HOUR FITNESS USA, INC.","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 HR Fitness","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"238 Sand Island Prop","canonical_id":"TNT-238-SAND-ISLAND-PROPERTY","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"238 SAND ISLAND PROPERTY LLC","canonical_id":"TNT-238-SAND-ISLAND-PROPERTY","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"}
        ]))
        .expect("aliases json"),
    )
    .expect("aliases");
}

fn csv_rows(path: &Path) -> Vec<BTreeMap<String, String>> {
    let mut reader = csv::Reader::from_path(path).expect("csv opens");
    reader
        .deserialize::<BTreeMap<String, String>>()
        .collect::<Result<Vec<_>, _>>()
        .expect("csv rows parse")
}

fn json_file(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}

fn jsonl_values(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("jsonl reads")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("jsonl value parses"))
        .collect()
}

fn unique_count(rows: &[BTreeMap<String, String>], field: &str) -> u64 {
    rows.iter()
        .map(|row| row.get(field).expect("field").as_str())
        .collect::<BTreeSet<_>>()
        .len() as u64
}

fn parse_u64(row: &BTreeMap<String, String>, field: &str) -> u64 {
    row[field]
        .parse()
        .unwrap_or_else(|error| panic!("{field} parses as u64: {error}"))
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
