use canon::entity::run::{
    EntityRunBatchConfig, EntityRunRequest, run_entity_workbench,
    run_entity_workbench_with_batching,
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

#[test]
fn cmbs_batching_equivalence() {
    let pair = run_default_and_batched(4);

    assert_eq!(
        pair.default.artifact.summary.counts["row_count"],
        pair.batched.artifact.summary.counts["row_count"]
    );
    assert_eq!(
        pair.default.artifact.summary.counts["prepared_surfaces"],
        pair.batched.artifact.summary.counts["prepared_surfaces"]
    );
    assert_eq!(
        pair.default.artifact.summary.counts["candidate_pairs"],
        pair.batched.artifact.summary.counts["candidate_pairs"]
    );
    assert_eq!(
        pair.default.artifact.summary.counts["review_group_count"],
        pair.batched.artifact.summary.counts["review_group_count"]
    );
    assert_eq!(
        pair.default.artifact.summary.labels["profile_id"],
        "cmbs_tenant_label"
    );
    assert_eq!(
        pair.batched.artifact.summary.labels["batching_mode"],
        "physical_batches"
    );
    assert_eq!(
        pair.batched.artifact.summary.counts["physical_batch_count"],
        4
    );
    assert_eq!(
        pair.batched.artifact.summary.counts["max_physical_batch_rows"],
        4
    );

    assert_eq!(
        normalized_surface_fingerprints(&pair.default.work_dir),
        normalized_surface_fingerprints(&pair.batched.work_dir),
        "batching must keep one global prepared-surface view"
    );
    assert_eq!(
        sorted_jsonl_lines(&pair.default.work_dir.join("block/candidates.jsonl")),
        sorted_jsonl_lines(&pair.batched.work_dir.join("block/candidates.jsonl")),
        "batching must not solve per-batch candidate sets"
    );
    assert_eq!(
        exact_bucket_fingerprints(&pair.default.work_dir),
        exact_bucket_fingerprints(&pair.batched.work_dir),
        "exact buckets remain compact global hyperedges"
    );

    let default_solve: Value = read_json(&pair.default.work_dir.join("solve/solve.json"));
    let batched_solve: Value = read_json(&pair.batched.work_dir.join("solve/solve.json"));
    assert_eq!(default_solve["entities"], batched_solve["entities"]);
    assert_eq!(
        default_solve["review_groups"],
        batched_solve["review_groups"]
    );
    assert_eq!(
        default_solve["summary"]["counts"]["review_group_count"],
        batched_solve["summary"]["counts"]["review_group_count"]
    );
    assert_eq!(
        pair.default.artifact.next_commands.apply, pair.batched.artifact.next_commands.apply,
        "exact replay command is over the full input, not per physical batch"
    );
}

#[allow(non_snake_case)]
#[test]
fn MR_BATCH_SIZE_batch_size_does_not_change_global_surface_or_candidate_view() {
    let small_batches = run_default_and_batched(3);
    let larger_batches = run_default_and_batched(7);

    assert_eq!(
        normalized_surface_fingerprints(&small_batches.batched.work_dir),
        normalized_surface_fingerprints(&larger_batches.batched.work_dir)
    );
    assert_eq!(
        sorted_jsonl_lines(
            &small_batches
                .batched
                .work_dir
                .join("block/candidates.jsonl")
        ),
        sorted_jsonl_lines(
            &larger_batches
                .batched
                .work_dir
                .join("block/candidates.jsonl")
        )
    );
    assert_ne!(
        small_batches.batched.artifact.summary.counts["physical_batch_count"],
        larger_batches.batched.artifact.summary.counts["physical_batch_count"],
        "test fixture must exercise different physical chunking"
    );
    assert_eq!(
        small_batches.batched.artifact.summary.counts["candidate_pairs"],
        larger_batches.batched.artifact.summary.counts["candidate_pairs"]
    );
    assert_eq!(
        small_batches.batched.artifact.summary.counts["review_group_count"],
        larger_batches.batched.artifact.summary.counts["review_group_count"]
    );
}

struct RunPair {
    _temp: tempfile::TempDir,
    default: RunCase,
    batched: RunCase,
}

struct RunCase {
    work_dir: PathBuf,
    artifact: canon::entity::run::EntityRunArtifact,
}

fn run_default_and_batched(batch_rows: u64) -> RunPair {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let default_work = temp.path().join("work-default");
    let batched_work = temp.path().join("work-batched");
    write_cmbs_registry(&registry);

    let default = run_entity_workbench(EntityRunRequest {
        rows: &rows_fixture(),
        profile: "cmbs_tenant_label",
        strategy: &strategy_fixture(),
        registry: &registry,
        work_dir: &default_work,
    })
    .expect("default entity run succeeds");

    let batched = run_entity_workbench_with_batching(
        EntityRunRequest {
            rows: &rows_fixture(),
            profile: "cmbs_tenant_label",
            strategy: &strategy_fixture(),
            registry: &registry,
            work_dir: &batched_work,
        },
        EntityRunBatchConfig::new(batch_rows),
    )
    .expect("batched entity run succeeds");

    RunPair {
        _temp: temp,
        default: RunCase {
            work_dir: default_work,
            artifact: default.artifact,
        },
        batched: RunCase {
            work_dir: batched_work,
            artifact: batched.artifact,
        },
    }
}

fn normalized_surface_fingerprints(work_dir: &Path) -> BTreeSet<String> {
    read_jsonl_values(&work_dir.join("prepare/surfaces.jsonl"))
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

fn exact_bucket_fingerprints(work_dir: &Path) -> BTreeSet<String> {
    read_jsonl_values(&work_dir.join("block/exact_buckets.jsonl"))
        .into_iter()
        .map(|bucket| {
            let surface_ids = bucket["membership"]["surface_ids"]
                .as_array()
                .expect("surface ids")
                .iter()
                .map(|value| value.as_str().unwrap_or_default())
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{}|{}|{}|{}|{}|{}",
                bucket["bucket_id"].as_str().unwrap_or_default(),
                surface_ids,
                bucket["row_count"].as_u64().unwrap_or_default(),
                bucket["deal_count"].as_u64().unwrap_or_default(),
                bucket["pair_expansion"].as_str().unwrap_or_default(),
                bucket["diagnostics"]["suppressed_pair_count"]
                    .as_u64()
                    .unwrap_or_default()
            )
        })
        .collect()
}

fn sorted_jsonl_lines(path: &Path) -> Vec<String> {
    let mut lines = fs::read_to_string(path)
        .expect("jsonl file")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    lines.sort();
    lines
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}

fn read_jsonl_values(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("jsonl file")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("jsonl value"))
        .collect()
}

fn write_cmbs_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"cmbs-tenants","version":"2026.06.25","description":"CMBS batching test registry","updated":"2026-06-25","entry_count":8}"#,
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

fn rows_fixture() -> PathBuf {
    fixture("tests/fixtures/entity/cmbs/small_book/observations.csv")
}

fn strategy_fixture() -> PathBuf {
    fixture("tests/fixtures/entity/profiles/cmbs_tenant_label.yaml")
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
