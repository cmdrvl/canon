use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_INDEX_VERSION, CANON_ENTITY_PREPARE_VERSION, EntityArtifactHeader,
        EntityArtifactMetadata, EntityDeterministicSummary, EntityInputReference,
        EntityNamekitReference, EntityPatchNamespaces, EntityPatchSetReference,
        EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
        cache::EntityCacheLayer,
        index::{
            EntityIndexArtifactRequest, EntityIndexCachePolicy, EntityIndexCacheStatus,
            build_index_artifact_contract, index_cache_key_from_prepare_header,
            index_summary_counts, required_index_hash_fields, validate_index_cache_policy,
        },
        schema::validate_artifact_core_contract,
    },
};
use std::collections::BTreeMap;

#[test]
fn entity_index_contract_artifact_records_prepare_hash_cache_status_and_paths() {
    let prepare = sample_prepare_header();
    let artifact = build_index_artifact_contract(EntityIndexArtifactRequest {
        prepare: prepare.clone(),
        strategy: sample_index_strategy(),
        cache_status: EntityIndexCacheStatus::Rebuilt,
        postings_path: "index/postings.bin".to_string(),
        diagnostics_path: "index/diagnostics.jsonl".to_string(),
        counts: index_summary_counts(3, 2, 7, 1),
    })
    .expect("index artifact contract");

    assert_eq!(artifact.version, CANON_ENTITY_INDEX_VERSION);
    assert_eq!(
        artifact.prepare_hash,
        prepare.metadata.artifact_content_hash
    );
    assert_eq!(
        artifact.metadata.artifact_content_hash,
        artifact.artifact_content_hash
    );
    assert_eq!(artifact.metadata.strategy.id, "cmbs_tenant_label.index.v1");
    assert_eq!(artifact.metadata.upstream_artifacts.len(), 1);
    assert_eq!(
        artifact.metadata.upstream_artifacts[0].content_hash,
        "blake3:prepare"
    );
    assert_eq!(artifact.summary.labels["cache_status"], "rebuilt");
    assert_eq!(artifact.summary.counts["surface_count"], 3);
    assert_eq!(artifact.postings_path, "index/postings.bin");
    assert_eq!(artifact.diagnostics_path, "index/diagnostics.jsonl");

    let json = serde_json::to_value(&artifact).expect("index artifact json");
    let snapshot = validate_artifact_core_contract(&json).expect("core contract validates");
    assert_eq!(snapshot.artifact_version, CANON_ENTITY_INDEX_VERSION);
}

#[test]
fn entity_cache_key_derives_from_prepare_header_and_i21_hashes() {
    let key = index_cache_key_from_prepare_header(
        EntityCacheLayer::TokenPostings,
        &sample_prepare_header(),
        &sample_index_strategy(),
    )
    .expect("cache key");

    assert_eq!(key.input_hash, "blake3:input");
    assert_eq!(key.profile_hash, "blake3:profile");
    assert_eq!(key.strategy_hash, "blake3:index-strategy");
    assert_eq!(key.registry_snapshot_hash, "blake3:registry");
    assert_eq!(
        key.upstream_artifact_hash.as_deref(),
        Some("blake3:prepare")
    );
    assert_eq!(key.patch_hash.as_deref(), Some("blake3:patch"));
    assert_eq!(key.namekit_version, "namekit.v0");
    assert_eq!(key.namekit_hash.as_deref(), Some("blake3:namekit"));

    for field in required_index_hash_fields() {
        assert!(key.required_hash_fields().contains(field));
    }
}

#[test]
fn cache_mismatch_refuses_when_policy_requires_exact_reuse() {
    let current = index_cache_key_from_prepare_header(
        EntityCacheLayer::TokenPostings,
        &sample_prepare_header(),
        &sample_index_strategy(),
    )
    .expect("cache key");
    let mut cached = current.clone();
    cached.registry_snapshot_hash = "blake3:old-registry".to_string();
    cached.namekit_hash = Some("blake3:old-namekit".to_string());

    let miss =
        validate_index_cache_policy(&cached, &current, EntityIndexCachePolicy::RebuildOnMiss)
            .expect("rebuild policy returns miss");
    assert_eq!(miss.changed_fields.len(), 2);
    assert_eq!(
        miss.decision,
        canon::entity::artifact_chain::EntityCacheDecision::Miss
    );

    let refusal =
        validate_index_cache_policy(&cached, &current, EntityIndexCachePolicy::RefuseOnMiss)
            .expect_err("strict policy refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityCacheMismatch);
    assert_eq!(refusal.detail["stage"], "index");
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
fn entity_index_contract_refuses_non_prepare_or_incomplete_prepare_header() {
    let mut wrong_version = sample_prepare_header();
    wrong_version.version = "canon_entity_solve.v0".to_string();
    let refusal = index_cache_key_from_prepare_header(
        EntityCacheLayer::TokenPostings,
        &wrong_version,
        &sample_index_strategy(),
    )
    .expect_err("wrong upstream version refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);

    let mut missing_profile_hash = sample_prepare_header();
    missing_profile_hash.metadata.profile.content_hash = None;
    let refusal = index_cache_key_from_prepare_header(
        EntityCacheLayer::TokenPostings,
        &missing_profile_hash,
        &sample_index_strategy(),
    )
    .expect_err("missing hash refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["field"], "metadata.profile.content_hash");
}

fn sample_index_strategy() -> EntityStrategyReference {
    EntityStrategyReference {
        id: "cmbs_tenant_label.index.v1".to_string(),
        version: "0.1.0".to_string(),
        content_hash: "blake3:index-strategy".to_string(),
    }
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
            strategy: EntityStrategyReference {
                id: "cmbs_tenant_label.prepare".to_string(),
                version: "0.1.0".to_string(),
                content_hash: "blake3:prepare-strategy".to_string(),
            },
            registry_snapshot: EntityRegistrySnapshot {
                id: "cmbs-tenants".to_string(),
                version: "2026.06.25".to_string(),
                source: "registries/cmbs-tenants".to_string(),
                lookup_snapshot_hash: "blake3:registry".to_string(),
                sidecar_snapshot_hash: None,
            },
            patch_namespace: "cmbs_tenant_label.aliases".to_string(),
            input: Some(EntityInputReference {
                row_count: 3,
                content_hash: "blake3:input".to_string(),
            }),
            upstream_artifacts: Vec::new(),
            patch_set: Some(EntityPatchSetReference {
                content_hash: "blake3:patch".to_string(),
                paths: Vec::new(),
            }),
            namekit: Some(EntityNamekitReference {
                version: "namekit.v0".to_string(),
                content_hash: "blake3:namekit".to_string(),
            }),
            artifact_content_hash: "blake3:prepare".to_string(),
        },
        summary: EntityDeterministicSummary {
            counts: BTreeMap::from([
                ("prepared_surfaces".to_string(), 3),
                ("exact_resolved_surfaces".to_string(), 1),
            ]),
            labels: BTreeMap::from([("profile".to_string(), "cmbs_tenant_label".to_string())]),
        },
    }
}
