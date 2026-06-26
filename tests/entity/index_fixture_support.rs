#![allow(dead_code)]

use canon::{
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
        },
        postings::{EntityPostingBuildConfig, EntityPostingIndex, EntityPostingSurface},
    },
    namekit::ngram::NgramConfig,
};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
pub struct IndexFixture {
    pub fixture_id: String,
    pub schema_version: String,
    pub profile: String,
    pub common_posting_limit: usize,
    pub ngram_width: usize,
    pub hashes: FixtureHashes,
    pub surfaces: Vec<IndexSurfaceFixture>,
    pub expected: ExpectedIndexFixture,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FixtureHashes {
    pub input: String,
    pub profile: String,
    pub strategy: String,
    pub registry_snapshot: String,
    pub patch: String,
    pub namekit_version: String,
    pub namekit: String,
    pub prepare: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndexSurfaceFixture {
    pub surface_id: String,
    pub exact_views: BTreeMap<String, String>,
    pub tokens: Vec<String>,
    pub ngram_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExpectedIndexFixture {
    pub surface_ids: Vec<String>,
    pub token_term_ids: BTreeMap<String, u32>,
    pub token_document_frequency: BTreeMap<String, u32>,
    pub token_idf_descending: Vec<String>,
    pub token_postings: BTreeMap<String, Vec<u32>>,
    pub token_offsets: Vec<usize>,
    pub exact_view_postings: BTreeMap<String, Vec<u32>>,
    pub exact_offsets: Vec<usize>,
    pub ngram_term_ids: BTreeMap<String, u32>,
    pub ngram_postings: BTreeMap<String, Vec<u32>>,
    pub ngram_offsets: Option<Vec<usize>>,
    pub common_exact_view_diagnostics: Vec<ExpectedCommonPosting>,
    pub common_token_diagnostics: Vec<ExpectedCommonPosting>,
    pub common_ngram_diagnostics: Vec<ExpectedCommonPosting>,
    pub counts: ExpectedCounts,
    pub artifact: ExpectedArtifact,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ExpectedCommonPosting {
    pub key: String,
    pub posting_count: usize,
    pub configured_limit: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExpectedCounts {
    pub surface_count: u32,
    pub exact_view_count: usize,
    pub token_count: usize,
    pub ngram_count: usize,
    pub large_exact_view_bucket_count: usize,
    pub common_token_count: usize,
    pub common_ngram_count: usize,
    pub largest_exact_view_bucket_size: usize,
    pub largest_token_posting_size: usize,
    pub largest_ngram_posting_size: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExpectedArtifact {
    pub prepare_hash: String,
    pub strategy_hash: String,
    pub profile_hash: String,
    pub registry_snapshot_hash: String,
    pub input_hash: String,
    pub patch_hash: String,
    pub namekit_version: String,
    pub namekit_hash: String,
    pub cache_status: String,
}

#[derive(Debug, Clone)]
pub struct BuiltIndexFixture {
    pub artifact: EntityIndexArtifact,
    pub cache_key: EntityCacheKey,
    pub postings: EntityIndexPostingsBundle,
    pub diagnostics: Vec<EntityIndexDiagnosticRecord>,
}

pub fn parse_fixture(text: &str) -> IndexFixture {
    serde_json::from_str(text).expect("index fixture parses")
}

pub fn build_index_fixture(
    fixture: &IndexFixture,
    cache_status: EntityIndexCacheStatus,
) -> BuiltIndexFixture {
    let posting_index = build_posting_index(fixture);
    let ngram_index = build_ngram_index(fixture);
    let posting_diagnostics = posting_index.diagnostics.clone();
    let ngram_diagnostics = ngram_index.diagnostics.clone();
    let postings = EntityIndexPostingsBundle::new(posting_index, Some(ngram_index));
    postings.validate_reload().expect("postings reload");

    let prepare = prepare_header(fixture);
    let strategy = index_strategy(fixture);
    let artifact = build_index_artifact_contract(EntityIndexArtifactRequest {
        prepare: prepare.clone(),
        strategy: strategy.clone(),
        cache_status,
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
    .expect("index artifact builds");
    let cache_key =
        index_cache_key_from_prepare_header(EntityCacheLayer::NgramPostings, &prepare, &strategy)
            .expect("index cache key");

    BuiltIndexFixture {
        artifact,
        cache_key,
        postings,
        diagnostics: diagnostics_for(fixture, cache_status),
    }
}

pub fn persist_request(
    built: &BuiltIndexFixture,
    max_artifact_bytes: Option<u64>,
) -> EntityIndexPersistRequest {
    EntityIndexPersistRequest {
        artifact: built.artifact.clone(),
        cache_key: built.cache_key.clone(),
        postings: built.postings.clone(),
        diagnostics: built.diagnostics.clone(),
        max_artifact_bytes,
    }
}

pub fn build_posting_index(fixture: &IndexFixture) -> EntityPostingIndex {
    EntityPostingIndex::build(
        &posting_surfaces(&fixture.surfaces),
        EntityPostingBuildConfig {
            common_posting_limit: fixture.common_posting_limit,
        },
    )
    .expect("posting index builds")
}

pub fn build_ngram_index(fixture: &IndexFixture) -> EntityNgramIndex {
    EntityNgramIndex::build(
        &ngram_surfaces(&fixture.surfaces),
        EntityNgramBuildConfig {
            ngram: NgramConfig::new(fixture.ngram_width).expect("ngram width"),
            common_posting_limit: fixture.common_posting_limit,
        },
    )
    .expect("ngram index builds")
}

fn posting_surfaces(surfaces: &[IndexSurfaceFixture]) -> Vec<EntityPostingSurface> {
    surfaces
        .iter()
        .map(|surface| {
            let mut posting = EntityPostingSurface::new(&surface.surface_id);
            for (view_name, value) in &surface.exact_views {
                posting = posting.with_exact_view(view_name, value);
            }
            posting.with_tokens(surface.tokens.iter().cloned())
        })
        .collect()
}

fn ngram_surfaces(surfaces: &[IndexSurfaceFixture]) -> Vec<EntityNgramSurface> {
    surfaces
        .iter()
        .map(|surface| EntityNgramSurface::new(&surface.surface_id, &surface.ngram_key))
        .collect()
}

fn diagnostics_for(
    fixture: &IndexFixture,
    cache_status: EntityIndexCacheStatus,
) -> Vec<EntityIndexDiagnosticRecord> {
    let mut summary = EntityIndexDiagnosticRecord::new("artifact_summary");
    summary.counts.extend(BTreeMap::from([
        (
            "surface_count".to_string(),
            u64::from(fixture.expected.counts.surface_count),
        ),
        (
            "token_count".to_string(),
            fixture.expected.counts.token_count as u64,
        ),
        (
            "ngram_count".to_string(),
            fixture.expected.counts.ngram_count as u64,
        ),
    ]));
    summary.labels.extend(BTreeMap::from([
        ("fixture_id".to_string(), fixture.fixture_id.clone()),
        (
            "cache_status".to_string(),
            cache_status.as_str().to_string(),
        ),
    ]));

    let mut caps = EntityIndexDiagnosticRecord::new("posting_caps");
    caps.counts.extend(BTreeMap::from([
        (
            "large_exact_view_bucket_count".to_string(),
            fixture.expected.counts.large_exact_view_bucket_count as u64,
        ),
        (
            "common_token_count".to_string(),
            fixture.expected.counts.common_token_count as u64,
        ),
        (
            "common_ngram_count".to_string(),
            fixture.expected.counts.common_ngram_count as u64,
        ),
    ]));

    vec![summary, caps]
}

fn prepare_header(fixture: &IndexFixture) -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: CANON_ENTITY_PREPARE_VERSION.to_string(),
        metadata: EntityArtifactMetadata {
            profile: EntityProfileReference {
                id: fixture.profile.clone(),
                version: "0.1.0".to_string(),
                entity_type: "tenant_label".to_string(),
                identity_semantics: "canonical_display_label".to_string(),
                canonical_type: "tenant_label".to_string(),
                patch_namespaces: EntityPatchNamespaces {
                    aliases: format!("{}.aliases", fixture.profile),
                    distinct: format!("{}.distinct", fixture.profile),
                    relations: format!("{}.relations", fixture.profile),
                },
                content_hash: Some(fixture.hashes.profile.clone()),
            },
            strategy: EntityStrategyReference {
                id: format!("{}.prepare", fixture.profile),
                version: "0.1.0".to_string(),
                content_hash: format!("blake3:prepare-strategy-{}", fixture.fixture_id),
            },
            registry_snapshot: EntityRegistrySnapshot {
                id: "cmbs-tenants".to_string(),
                version: "2026.06.25".to_string(),
                source: "registries/cmbs-tenants".to_string(),
                lookup_snapshot_hash: fixture.hashes.registry_snapshot.clone(),
                sidecar_snapshot_hash: Some(format!("blake3:sidecars-{}", fixture.fixture_id)),
            },
            patch_namespace: format!("{}.aliases", fixture.profile),
            input: Some(EntityInputReference {
                row_count: fixture.surfaces.len() as u64,
                content_hash: fixture.hashes.input.clone(),
            }),
            upstream_artifacts: Vec::new(),
            patch_set: Some(EntityPatchSetReference {
                content_hash: fixture.hashes.patch.clone(),
                paths: vec![format!("patches/{}.yaml", fixture.profile)],
            }),
            namekit: Some(EntityNamekitReference {
                version: fixture.hashes.namekit_version.clone(),
                content_hash: fixture.hashes.namekit.clone(),
            }),
            artifact_content_hash: fixture.hashes.prepare.clone(),
        },
        summary: Default::default(),
    }
}

fn index_strategy(fixture: &IndexFixture) -> EntityStrategyReference {
    EntityStrategyReference {
        id: format!("{}.index", fixture.profile),
        version: "0.1.0".to_string(),
        content_hash: fixture.hashes.strategy.clone(),
    }
}
