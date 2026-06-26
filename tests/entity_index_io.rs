#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_PREPARE_VERSION,
        cache::{EntityCacheKey, EntityCacheLayer},
        contracts::{
            EntityArtifactHeader, EntityArtifactMetadata, EntityInputReference,
            EntityNamekitReference, EntityPatchNamespaces, EntityPatchSetReference,
            EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
        },
        index::ngram_index::{EntityNgramBuildConfig, EntityNgramIndex, EntityNgramSurface},
        index::{
            EntityIndexArtifact, EntityIndexArtifactRequest, EntityIndexCacheStatus,
            build_index_artifact_contract, index_cache_key_from_prepare_header,
            index_summary_counts,
        },
        index_io::{
            EntityIndexDiagnosticRecord, EntityIndexPersistRequest, EntityIndexPostingsBundle,
            INDEX_ARTIFACT_FILE, read_index_disk_bundle, write_index_disk_bundle,
        },
        postings::{EntityPostingBuildConfig, EntityPostingIndex, EntityPostingSurface},
    },
    namekit::ngram::NgramConfig,
};
use serde_json::Value;
use std::collections::BTreeMap;

#[test]
fn entity_index_io_reloads_postings_without_raw_inputs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fixture = sample_fixture();
    let paths = write_index_disk_bundle(
        temp.path(),
        EntityIndexPersistRequest {
            artifact: fixture.artifact.clone(),
            cache_key: fixture.cache_key.clone(),
            postings: fixture.postings.clone(),
            diagnostics: fixture.diagnostics.clone(),
            max_artifact_bytes: Some(128_000),
        },
    )
    .expect("write index artifacts");

    assert!(paths.artifact_path.exists());
    assert!(paths.cache_key_path.exists());
    assert!(paths.postings_path.exists());
    assert!(paths.diagnostics_path.exists());

    let bundle = read_index_disk_bundle(
        temp.path(),
        &fixture.artifact,
        &fixture.cache_key,
        Some(128_000),
    )
    .expect("reload index artifacts");

    assert_eq!(bundle.artifact, fixture.artifact);
    assert_eq!(bundle.cache_key, fixture.cache_key);
    assert_eq!(bundle.postings, fixture.postings);
    assert_eq!(bundle.diagnostics, fixture.diagnostics);
}

#[test]
fn entity_cache_hit_miss_index_io_reports_rebuild_or_refusal() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fixture = sample_fixture();
    write_index_disk_bundle(
        temp.path(),
        EntityIndexPersistRequest {
            artifact: fixture.artifact.clone(),
            cache_key: fixture.cache_key.clone(),
            postings: fixture.postings.clone(),
            diagnostics: fixture.diagnostics.clone(),
            max_artifact_bytes: Some(128_000),
        },
    )
    .expect("write index artifacts");

    let mut changed = fixture.cache_key.clone();
    changed.strategy_hash = "blake3:changed-strategy".to_string();

    let refusal = read_index_disk_bundle(temp.path(), &fixture.artifact, &changed, Some(128_000))
        .expect_err("strict cache mismatch refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityCacheMismatch);
    assert_eq!(refusal.detail["stage"], "index");
    assert_eq!(refusal.detail["decision"], "miss");
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
fn entity_index_io_refuses_tampered_artifact_hash() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fixture = sample_fixture();
    write_index_disk_bundle(
        temp.path(),
        EntityIndexPersistRequest {
            artifact: fixture.artifact.clone(),
            cache_key: fixture.cache_key.clone(),
            postings: fixture.postings.clone(),
            diagnostics: fixture.diagnostics.clone(),
            max_artifact_bytes: Some(128_000),
        },
    )
    .expect("write index artifacts");

    let artifact_path = temp.path().join(INDEX_ARTIFACT_FILE);
    let mut json: Value =
        serde_json::from_slice(&std::fs::read(&artifact_path).expect("artifact bytes"))
            .expect("artifact json");
    json["metadata"]["registry_snapshot"]["lookup_snapshot_hash"] =
        Value::String("blake3:tampered-registry".to_string());
    std::fs::write(
        &artifact_path,
        serde_json::to_vec(&json).expect("tampered artifact json"),
    )
    .expect("write tampered artifact");

    let refusal = read_index_disk_bundle(
        temp.path(),
        &fixture.artifact,
        &fixture.cache_key,
        Some(128_000),
    )
    .expect_err("tampered artifact refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "index");
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
#[allow(non_snake_case)]
fn E_ENTITY_IO_BUDGET_index_io_refuses_before_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fixture = sample_fixture();
    let refusal = write_index_disk_bundle(
        temp.path(),
        EntityIndexPersistRequest {
            artifact: fixture.artifact,
            cache_key: fixture.cache_key,
            postings: fixture.postings,
            diagnostics: fixture.diagnostics,
            max_artifact_bytes: Some(1),
        },
    )
    .expect_err("postings budget refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityIoBudget);
    assert_eq!(refusal.detail["stage"], "index");
    assert_eq!(refusal.detail["limit"], "max_artifact_bytes");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(!temp.path().join(INDEX_ARTIFACT_FILE).exists());
}

struct DiskFixture {
    artifact: EntityIndexArtifact,
    cache_key: EntityCacheKey,
    postings: EntityIndexPostingsBundle,
    diagnostics: Vec<EntityIndexDiagnosticRecord>,
}

fn sample_fixture() -> DiskFixture {
    let prepare = sample_prepare_header();
    let strategy = sample_index_strategy();
    let postings = EntityPostingIndex::build(
        &[
            EntityPostingSurface::new("surf:001")
                .with_exact_view("tenant_core", "sears")
                .with_tokens(["sears"]),
            EntityPostingSurface::new("surf:002")
                .with_exact_view("tenant_core", "sears")
                .with_tokens(["sears", "auto"]),
            EntityPostingSurface::new("surf:003")
                .with_exact_view("tenant_core", "kmart")
                .with_tokens(["kmart"]),
        ],
        EntityPostingBuildConfig {
            common_posting_limit: 1,
        },
    )
    .expect("posting index");
    let ngrams = EntityNgramIndex::build(
        &[
            EntityNgramSurface::new("surf:001", "sears"),
            EntityNgramSurface::new("surf:002", "sears auto"),
            EntityNgramSurface::new("surf:003", "kmart"),
        ],
        EntityNgramBuildConfig {
            ngram: NgramConfig::new(3).expect("width"),
            common_posting_limit: 1,
        },
    )
    .expect("ngram index");
    let posting_diagnostics = postings.diagnostics.clone();
    let ngram_diagnostics = ngrams.diagnostics.clone();
    let postings = EntityIndexPostingsBundle::new(postings, Some(ngrams));
    let artifact = build_index_artifact_contract(EntityIndexArtifactRequest {
        prepare: prepare.clone(),
        strategy: strategy.clone(),
        cache_status: EntityIndexCacheStatus::Rebuilt,
        postings_path: "index/postings.json".to_string(),
        diagnostics_path: "index/diagnostics.jsonl".to_string(),
        counts: index_summary_counts(
            u64::from(posting_diagnostics.surface_count),
            posting_diagnostics.token_count as u64,
            ngram_diagnostics.ngram_count as u64,
            (posting_diagnostics.large_exact_view_bucket_count
                + posting_diagnostics.common_token_count
                + ngram_diagnostics.common_ngram_count) as u64,
        ),
    })
    .expect("index artifact");
    let cache_key =
        index_cache_key_from_prepare_header(EntityCacheLayer::NgramPostings, &prepare, &strategy)
            .expect("cache key");

    DiskFixture {
        artifact,
        cache_key,
        postings,
        diagnostics: sample_diagnostics(),
    }
}

fn sample_diagnostics() -> Vec<EntityIndexDiagnosticRecord> {
    let mut summary = EntityIndexDiagnosticRecord::new("artifact_summary");
    summary
        .counts
        .extend(BTreeMap::from([("surface_count".to_string(), 3)]));
    summary.labels.extend(BTreeMap::from([(
        "cache_status".to_string(),
        "rebuilt".to_string(),
    )]));

    let mut postings = EntityIndexDiagnosticRecord::new("posting_summary");
    postings.counts.extend(BTreeMap::from([
        ("token_terms".to_string(), 3),
        ("ngram_terms".to_string(), 9),
    ]));

    vec![summary, postings]
}

fn sample_prepare_header() -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: CANON_ENTITY_PREPARE_VERSION.to_string(),
        metadata: EntityArtifactMetadata {
            profile: EntityProfileReference {
                id: "cmbs_tenant_label".to_string(),
                version: "0.1.0".to_string(),
                entity_type: "tenant_label".to_string(),
                identity_semantics: "canonical_display_label".to_string(),
                canonical_type: "tenant_label".to_string(),
                patch_namespaces: EntityPatchNamespaces {
                    aliases: "cmbs_tenant_label.aliases".to_string(),
                    distinct: "cmbs_tenant_label.distinct".to_string(),
                    relations: "cmbs_tenant_label.relations".to_string(),
                },
                content_hash: Some("blake3:profile".to_string()),
            },
            strategy: sample_index_strategy(),
            registry_snapshot: EntityRegistrySnapshot {
                id: "cmbs-tenants".to_string(),
                version: "2026.06.25".to_string(),
                source: "registries/cmbs-tenants".to_string(),
                lookup_snapshot_hash: "blake3:registry".to_string(),
                sidecar_snapshot_hash: Some("blake3:sidecars".to_string()),
            },
            patch_namespace: "cmbs_tenant_label.aliases".to_string(),
            input: Some(EntityInputReference {
                row_count: 3,
                content_hash: "blake3:input".to_string(),
            }),
            upstream_artifacts: Vec::new(),
            patch_set: Some(EntityPatchSetReference {
                content_hash: "blake3:patch".to_string(),
                paths: vec!["patches/cmbs-tenants.yaml".to_string()],
            }),
            namekit: Some(EntityNamekitReference {
                version: "namekit.v0".to_string(),
                content_hash: "blake3:namekit".to_string(),
            }),
            artifact_content_hash: "blake3:prepare".to_string(),
        },
        summary: Default::default(),
    }
}

fn sample_index_strategy() -> EntityStrategyReference {
    EntityStrategyReference {
        id: "cmbs_tenant_label.index".to_string(),
        version: "0.1.0".to_string(),
        content_hash: "blake3:strategy".to_string(),
    }
}
