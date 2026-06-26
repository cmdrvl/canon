#![forbid(unsafe_code)]

#[path = "entity/index_fixture_support.rs"]
mod index_fixture_support;

use canon::{
    RefusalCode,
    entity::{
        cache::EntityCacheLayer,
        index::EntityIndexCacheStatus,
        index_io::{INDEX_ARTIFACT_FILE, read_index_disk_bundle, write_index_disk_bundle},
    },
};
use index_fixture_support::{build_index_fixture, parse_fixture, persist_request};

const SMALL_FIXTURE: &str = include_str!("fixtures/entity/index/small_cmbs_fixture.json");

#[test]
fn entity_cache_reload_round_trips_fixture_without_raw_inputs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fixture = parse_fixture(SMALL_FIXTURE);
    let built = build_index_fixture(&fixture, EntityIndexCacheStatus::Rebuilt);
    let paths = write_index_disk_bundle(temp.path(), persist_request(&built, Some(256_000)))
        .expect("persist index fixture");

    assert_eq!(paths.artifact_path, temp.path().join(INDEX_ARTIFACT_FILE));
    assert!(paths.postings_path.ends_with("index/postings.json"));
    assert!(paths.diagnostics_path.ends_with("index/diagnostics.jsonl"));

    let reloaded = read_index_disk_bundle(
        temp.path(),
        &built.artifact,
        &built.cache_key,
        Some(256_000),
    )
    .expect("reload index fixture");

    assert_eq!(reloaded.artifact, built.artifact);
    assert_eq!(reloaded.cache_key, built.cache_key);
    assert_eq!(reloaded.diagnostics, built.diagnostics);
    assert_eq!(
        serde_json::to_vec(&reloaded.postings).expect("reloaded postings json"),
        serde_json::to_vec(&built.postings).expect("built postings json")
    );
    assert_eq!(
        reloaded.postings.posting_index.surface_ids,
        fixture.expected.surface_ids
    );
}

#[test]
fn entity_cache_reload_hit_and_miss_diagnostics_are_explicit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fixture = parse_fixture(SMALL_FIXTURE);
    let built = build_index_fixture(&fixture, EntityIndexCacheStatus::Hit);
    write_index_disk_bundle(temp.path(), persist_request(&built, Some(256_000)))
        .expect("persist cache hit fixture");

    let hit = read_index_disk_bundle(
        temp.path(),
        &built.artifact,
        &built.cache_key,
        Some(256_000),
    )
    .expect("cache hit reloads");
    assert_eq!(hit.artifact.summary.labels["cache_status"], "hit");
    assert_eq!(hit.diagnostics[0].labels["cache_status"], "hit");
    assert_eq!(hit.cache_key.layer, EntityCacheLayer::NgramPostings);

    let mut stale_key = built.cache_key.clone();
    stale_key.strategy_hash = "blake3:index-fixture-stale-strategy".to_string();
    let refusal = read_index_disk_bundle(temp.path(), &built.artifact, &stale_key, Some(256_000))
        .expect_err("stale cache refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityCacheMismatch);
    assert_eq!(refusal.detail["stage"], "index");
    assert_eq!(refusal.detail["decision"], "miss");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(
        refusal.detail["changed_fields"]
            .as_array()
            .expect("changed fields")
            .iter()
            .any(|field| field == "strategy_hash")
    );
}
