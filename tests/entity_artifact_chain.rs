use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_AUDIT_VERSION, CANON_ENTITY_BLOCK_VERSION, CANON_ENTITY_PREPARE_VERSION,
        EntityArtifactHeader, EntityArtifactMetadata, EntityArtifactReference,
        EntityCacheKeyMaterial, EntityDeterministicSummary, EntityInputReference,
        EntityNamekitReference, EntityPatchNamespaces, EntityPatchSetReference,
        EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
        artifact_chain::{
            EntityArtifactChainExpectation, EntityArtifactChainLink, EntityCacheDecision,
            EntityCacheStage, EntityChainEnforcement, EntityChainStage, EntityHashField,
            EntitySolverAbstention, REGISTRY_HASH_INVALIDATES, STRATEGY_HASH_INVALIDATES,
            audit_gate_refusal, validate_artifact_chain, validate_entity_cache_hit,
        },
    },
};
use std::collections::BTreeMap;

#[test]
fn entity_artifact_chain_validates_matching_prepare_metadata() {
    let link = EntityArtifactChainLink::from_header(&sample_header(CANON_ENTITY_PREPARE_VERSION));
    let expected = EntityArtifactChainExpectation::from_link(EntityChainStage::Index, &link);

    let validation = validate_artifact_chain(&link, &expected).expect("matching chain validates");

    assert_eq!(validation.consumer_stage, EntityChainStage::Index);
    assert_eq!(validation.artifact_content_hash, "blake3:prepare-artifact");
    assert!(
        validation
            .validated_fields
            .contains(&EntityHashField::InputHash)
    );
    assert!(
        validation
            .validated_fields
            .contains(&EntityHashField::RegistrySnapshotHash)
    );
}

#[test]
fn entity_cache_hit_miss_requires_every_i21_hash_for_prepare_and_index() {
    let current = sample_cache_key();
    let hit = validate_entity_cache_hit(EntityCacheStage::Prepare, &current, &current);
    assert_eq!(hit.decision, EntityCacheDecision::Hit);
    assert!(hit.mismatches.is_empty());

    let mut cached = current.clone();
    cached.patch_hash = Some("blake3:old-patch".to_string());
    cached.namekit_hash = Some("blake3:old-namekit".to_string());
    let miss = validate_entity_cache_hit(EntityCacheStage::Index, &cached, &current);

    assert_eq!(miss.decision, EntityCacheDecision::Miss);
    assert!(miss.rebuild_allowed);
    assert_eq!(miss.mismatches.len(), 2);
    assert_eq!(miss.mismatches[0].field, EntityHashField::PatchHash);
    assert_eq!(
        miss.mismatches[0].enforcement,
        EntityChainEnforcement::CacheMiss
    );
    assert_eq!(miss.mismatches[1].field, EntityHashField::NamekitHash);
}

#[test]
#[allow(non_snake_case)]
fn G10_cache_contract_names_hash_invalidation_layers() {
    assert!(STRATEGY_HASH_INVALIDATES.contains(&EntityChainStage::Index));
    assert!(STRATEGY_HASH_INVALIDATES.contains(&EntityChainStage::Block));
    assert!(STRATEGY_HASH_INVALIDATES.contains(&EntityChainStage::Apply));
    assert!(REGISTRY_HASH_INVALIDATES.contains(&EntityChainStage::ReviewImport));
    assert!(REGISTRY_HASH_INVALIDATES.contains(&EntityChainStage::Promote));
}

#[test]
fn stale_registry_snapshot_refusal_reports_expected_actual_and_recovery() {
    let link = EntityArtifactChainLink::from_header(&sample_header(CANON_ENTITY_BLOCK_VERSION));
    let mut expected = EntityArtifactChainExpectation::from_link(EntityChainStage::Edge, &link);
    expected.registry_snapshot_hash = "blake3:new-registry".to_string();

    let refusal =
        validate_artifact_chain(&link, &expected).expect_err("stale registry snapshot must refuse");

    assert_eq!(refusal.code, RefusalCode::EEntityRegistrySnapshot);
    assert_eq!(refusal.detail["stage"], "edge");
    assert_eq!(refusal.detail["field"], "registry_snapshot_hash");
    assert_eq!(refusal.detail["expected"], "blake3:new-registry");
    assert_eq!(refusal.detail["actual"], "blake3:registry");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(
        refusal
            .next_command
            .as_deref()
            .unwrap()
            .contains("canon entity edge")
    );
}

#[test]
fn review_import_hash_refusal_uses_review_import_code_before_ledger_write() {
    let link = EntityArtifactChainLink::from_header(&sample_header(CANON_ENTITY_AUDIT_VERSION));
    let mut expected =
        EntityArtifactChainExpectation::from_link(EntityChainStage::ReviewImport, &link);
    expected.strategy_hash = "blake3:new-strategy".to_string();

    let refusal =
        validate_artifact_chain(&link, &expected).expect_err("stale review import must refuse");

    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["stage"], "review_import");
    assert_eq!(refusal.detail["field"], "strategy_hash");
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
fn promotion_stale_audit_refusal_is_artifact_contract_before_registry_write() {
    let mut header = sample_header(CANON_ENTITY_AUDIT_VERSION);
    header.metadata.upstream_artifacts = vec![EntityArtifactReference {
        version: CANON_ENTITY_BLOCK_VERSION.to_string(),
        content_hash: "blake3:block-old".to_string(),
    }];
    let link = EntityArtifactChainLink::from_header(&header);
    let mut expected = EntityArtifactChainExpectation::from_link(EntityChainStage::Promote, &link);
    expected.upstream_artifacts = vec![EntityArtifactReference {
        version: CANON_ENTITY_BLOCK_VERSION.to_string(),
        content_hash: "blake3:block-current".to_string(),
    }];

    let refusal =
        validate_artifact_chain(&link, &expected).expect_err("stale audit input must refuse");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "promote");
    assert_eq!(refusal.detail["field"], "upstream_artifact_hash");
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
fn stale_artifact_refusals_distinguish_audit_gate_from_chain_refusal() {
    let refusal = audit_gate_refusal("G14", "gold_pair_f1 >= 0.98", "gold_pair_f1 = 0.91");

    assert_eq!(refusal.code, RefusalCode::EEntityAuditGate);
    assert_eq!(refusal.detail["stage"], "audit");
    assert_eq!(refusal.detail["gate_id"], "G14");
    assert_eq!(refusal.detail["expected"], "gold_pair_f1 >= 0.98");
    assert_eq!(refusal.detail["actual"], "gold_pair_f1 = 0.91");
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
fn solver_abstention_is_successful_non_refusal_outcome() {
    let abstention = EntitySolverAbstention::hard_cannot_link("component:sears", 2);

    assert_eq!(abstention.stage, EntityChainStage::Solve);
    assert_eq!(abstention.reason, "hard_cannot_link_present");
    assert_eq!(abstention.hard_cannot_link_count, 2);
    assert!(!abstention.is_refusal());
    assert!(abstention.refusal_code.is_none());
}

fn sample_header(version: &str) -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: version.to_string(),
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
                id: "cmbs_tenant_label.v1".to_string(),
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
                row_count: 10_143,
                content_hash: "blake3:input".to_string(),
            }),
            upstream_artifacts: vec![],
            patch_set: Some(EntityPatchSetReference {
                content_hash: "blake3:patch".to_string(),
                paths: vec!["patches/cmbs-tenants.yaml".to_string()],
            }),
            namekit: Some(EntityNamekitReference {
                version: "namekit.v0".to_string(),
                content_hash: "blake3:namekit".to_string(),
            }),
            artifact_content_hash: "blake3:prepare-artifact".to_string(),
        },
        summary: EntityDeterministicSummary {
            counts: BTreeMap::from([("prepared_surfaces".to_string(), 431)]),
            labels: BTreeMap::from([("cache_status".to_string(), "rebuilt".to_string())]),
        },
    }
}

fn sample_cache_key() -> EntityCacheKeyMaterial {
    EntityCacheKeyMaterial {
        input_hash: "blake3:input".to_string(),
        profile_hash: "blake3:profile".to_string(),
        strategy_hash: "blake3:strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
        patch_hash: Some("blake3:patch".to_string()),
        namekit_version: "namekit.v0".to_string(),
        namekit_hash: Some("blake3:namekit".to_string()),
    }
}
