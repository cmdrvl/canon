use super::index_fixture_support::{
    BuiltIndexFixture, build_index_fixture, parse_fixture, persist_request,
};
use canon::{
    RefusalCode,
    entity::{
        cache::EntityCacheKey,
        index::EntityIndexCacheStatus,
        index_io::{read_index_disk_bundle, write_index_disk_bundle},
    },
};

const MEDIUM_FIXTURE: &str =
    include_str!("../fixtures/entity/index/en_i002_medium_cache_reload.json");

#[test]
fn entity_cache_reload_medium_fixture_round_trips_without_raw_inputs() {
    let fixture = parse_fixture(MEDIUM_FIXTURE);
    let built = build_index_fixture(&fixture, EntityIndexCacheStatus::Rebuilt);
    let temp = tempfile::tempdir().expect("tempdir");

    let paths = write_index_disk_bundle(temp.path(), persist_request(&built, Some(512_000)))
        .expect("write index bundle");
    let persisted_postings = std::fs::read(&paths.postings_path).expect("postings bytes");
    let persisted_diagnostics =
        std::fs::read_to_string(&paths.diagnostics_path).expect("diagnostics text");

    let reloaded = read_index_disk_bundle(
        temp.path(),
        &built.artifact,
        &built.cache_key,
        Some(512_000),
    )
    .expect("reload index bundle");

    assert_eq!(reloaded.artifact, built.artifact);
    assert_eq!(reloaded.cache_key, built.cache_key);
    assert_eq!(reloaded.postings, built.postings);
    assert_eq!(reloaded.diagnostics, built.diagnostics);
    assert_eq!(
        persisted_postings,
        serde_json::to_vec_pretty(&built.postings).expect("postings serialize")
    );
    assert!(persisted_diagnostics.contains("\"cache_status\":\"rebuilt\""));
    assert_eq!(reloaded.artifact.summary.labels["cache_status"], "rebuilt");
    assert_eq!(
        reloaded.diagnostics[0].labels["fixture_id"],
        fixture.fixture_id
    );
}

#[test]
fn entity_cache_reload_cache_hit_and_miss_diagnostics_are_explicit() {
    let fixture = parse_fixture(MEDIUM_FIXTURE);
    let hit = build_index_fixture(&fixture, EntityIndexCacheStatus::Hit);
    assert_eq!(hit.artifact.summary.labels["cache_status"], "hit");
    assert_eq!(hit.diagnostics[0].labels["cache_status"], "hit");

    let rebuilt = build_index_fixture(&fixture, EntityIndexCacheStatus::Rebuilt);
    let temp = tempfile::tempdir().expect("tempdir");
    write_index_disk_bundle(temp.path(), persist_request(&rebuilt, Some(512_000)))
        .expect("write rebuilt index bundle");

    for (case, changed_field) in [
        ("input", "input_hash"),
        ("profile", "profile_hash"),
        ("strategy", "strategy_hash"),
        ("registry", "registry_snapshot_hash"),
        ("prepare", "upstream_artifact_hash"),
        ("patch", "patch_hash"),
        ("namekit_version", "namekit_version"),
        ("namekit_hash", "namekit_hash"),
    ] {
        let stale_current = changed_current_key(&rebuilt, case);
        let refusal = read_index_disk_bundle(
            temp.path(),
            &rebuilt.artifact,
            &stale_current,
            Some(512_000),
        )
        .unwrap_err_or_else(|| panic!("{case} cache mismatch should refuse"));
        assert_eq!(refusal.code, RefusalCode::EEntityCacheMismatch, "{case}");
        assert_eq!(refusal.detail["stage"], "index", "{case}");
        assert_eq!(refusal.detail["decision"], "miss", "{case}");
        assert_eq!(refusal.detail["writes_performed"], false, "{case}");
        assert!(
            refusal.detail["changed_fields"]
                .as_array()
                .expect("changed fields")
                .iter()
                .any(|field| field == changed_field),
            "{case} should name {changed_field}: {}",
            refusal.detail
        );
    }
}

fn changed_current_key(built: &BuiltIndexFixture, case: &str) -> EntityCacheKey {
    let mut key = built.cache_key.clone();
    match case {
        "input" => key.input_hash = "blake3:changed-input".to_string(),
        "profile" => key.profile_hash = "blake3:changed-profile".to_string(),
        "strategy" => key.strategy_hash = "blake3:changed-strategy".to_string(),
        "registry" => key.registry_snapshot_hash = "blake3:changed-registry".to_string(),
        "prepare" => key.upstream_artifact_hash = Some("blake3:changed-prepare".to_string()),
        "patch" => key.patch_hash = Some("blake3:changed-patch".to_string()),
        "namekit_version" => key.namekit_version = "namekit.v1".to_string(),
        "namekit_hash" => key.namekit_hash = Some("blake3:changed-namekit".to_string()),
        _ => unreachable!("unknown case"),
    }
    key
}

trait ExpectErrOrElse<T, E> {
    fn unwrap_err_or_else<F: FnOnce() -> String>(self, message: F) -> E;
}

impl<T, E> ExpectErrOrElse<T, E> for Result<T, E> {
    fn unwrap_err_or_else<F: FnOnce() -> String>(self, message: F) -> E {
        match self {
            Ok(_) => panic!("{}", message()),
            Err(error) => error,
        }
    }
}
