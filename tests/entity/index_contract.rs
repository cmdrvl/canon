use canon::{
    entity::{
        artifact_chain::EntityCacheDecision,
        cache::EntityCacheLayer,
        contracts::{
            EntityArtifactHeader, EntityArtifactMetadata, EntityInputReference,
            EntityNamekitReference, EntityPatchNamespaces, EntityPatchSetReference,
            EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
        },
        index::{
            build_index_artifact_contract, index_cache_key_from_prepare_header,
            index_summary_counts, required_index_hash_fields, validate_index_cache_policy,
            EntityIndexArtifactRequest, EntityIndexCachePolicy, EntityIndexCacheStatus,
        },
        schema::validate_artifact_core_contract,
        CANON_ENTITY_INDEX_VERSION, CANON_ENTITY_PREPARE_VERSION,
    },
    RefusalCode,
};
use serde_json::Value;

#[test]
fn entity_index_contract_records_hashes_and_cache_status() {
    let request = sample_request();
    let first = build_index_artifact_contract(request.clone()).expect("index artifact");
    let second = build_index_artifact_contract(request).expect("repeat index artifact");

    assert_eq!(first, second);
    assert_eq!(first.version, CANON_ENTITY_INDEX_VERSION);
    assert_eq!(first.prepare_hash, "blake3:prepare");
    assert_eq!(
        first.metadata.artifact_content_hash,
        first.artifact_content_hash
    );
    assert_eq!(first.metadata.profile.id, "cmbs_tenant_label");
    assert_eq!(first.metadata.profile.version, "0.1.0");
    assert_eq!(
        first.metadata.registry_snapshot.lookup_snapshot_hash,
        "blake3:registry"
    );
    assert_eq!(first.summary.counts["surface_count"], 27_118);
    assert_eq!(first.summary.counts["token_count"], 11_804);
    assert_eq!(first.summary.counts["ngram_count"], 39_210);
    assert_eq!(first.summary.counts["large_bucket_count"], 17);
    assert_eq!(first.summary.labels["cache_status"], "rebuilt");
    assert_eq!(
        first.summary.labels["upstream_version"],
        CANON_ENTITY_PREPARE_VERSION
    );
    assert_eq!(first.postings_path, "entity/index/postings.bin");
    assert_eq!(first.diagnostics_path, "entity/index/diagnostics.json");
    assert!(first.artifact_content_hash.starts_with("blake3:"));

    let json: Value = serde_json::to_value(&first).expect("index artifact json");
    let snapshot = validate_artifact_core_contract(&json).expect("core contract validates");
    assert_eq!(snapshot.artifact_version, CANON_ENTITY_INDEX_VERSION);
}

#[test]
fn entity_cache_key_includes_i21_and_prepare_hash() {
    let prepare = sample_prepare_header();
    let key = index_cache_key_from_prepare_header(
        EntityCacheLayer::TokenPostings,
        &prepare,
        &sample_index_strategy(),
    )
    .expect("index cache key");

    assert_eq!(key.layer, EntityCacheLayer::TokenPostings);
    assert_eq!(key.input_hash, "blake3:input");
    assert_eq!(key.profile_hash, "blake3:profile");
    assert_eq!(key.strategy_hash, "blake3:strategy");
    assert_eq!(key.registry_snapshot_hash, "blake3:registry");
    assert_eq!(
        key.upstream_artifact_hash.as_deref(),
        Some("blake3:prepare")
    );
    assert_eq!(key.patch_hash.as_deref(), Some("blake3:patch"));
    assert_eq!(key.namekit_version, "namekit.v0");
    assert_eq!(key.namekit_hash.as_deref(), Some("blake3:namekit"));
    assert!(required_index_hash_fields()
        .iter()
        .all(|field| key.required_hash_fields().contains(field)));
}

#[test]
fn cache_mismatch_refuses_or_rebuilds_by_policy() {
    let prepare = sample_prepare_header();
    let current = index_cache_key_from_prepare_header(
        EntityCacheLayer::TokenDictionary,
        &prepare,
        &sample_index_strategy(),
    )
    .expect("current key");
    let mut cached = current.clone();
    cached.registry_snapshot_hash = "blake3:old-registry".to_string();
    cached.upstream_artifact_hash = Some("blake3:old-prepare".to_string());

    let rebuild =
        validate_index_cache_policy(&cached, &current, EntityIndexCachePolicy::RebuildOnMiss)
            .expect("rebuild allowed");
    assert_eq!(rebuild.decision, EntityCacheDecision::Miss);
    assert!(rebuild
        .changed_fields
        .contains(&canon::entity::artifact_chain::EntityHashField::RegistrySnapshotHash));
    assert!(rebuild
        .changed_fields
        .contains(&canon::entity::artifact_chain::EntityHashField::UpstreamArtifactHash));

    let refusal =
        validate_index_cache_policy(&cached, &current, EntityIndexCachePolicy::RefuseOnMiss)
            .expect_err("strict cache mismatch refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityCacheMismatch);
    assert_eq!(refusal.detail["stage"], "index");
    assert_eq!(refusal.detail["decision"], "miss");
    assert_eq!(refusal.detail["writes_performed"], false);
}

fn sample_request() -> EntityIndexArtifactRequest {
    EntityIndexArtifactRequest {
        prepare: sample_prepare_header(),
        strategy: sample_index_strategy(),
        cache_status: EntityIndexCacheStatus::Rebuilt,
        postings_path: "entity/index/postings.bin".to_string(),
        diagnostics_path: "entity/index/diagnostics.json".to_string(),
        counts: index_summary_counts(27_118, 11_804, 39_210, 17),
    }
}

fn sample_prepare_header() -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: CANON_ENTITY_PREPARE_VERSION.to_string(),
        metadata: sample_prepare_metadata(),
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

fn sample_prepare_metadata() -> EntityArtifactMetadata {
    EntityArtifactMetadata {
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
            id: "cmbs_tenant_label.index".to_string(),
            version: "0.1.0".to_string(),
            content_hash: "blake3:strategy".to_string(),
        },
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
    }
}
