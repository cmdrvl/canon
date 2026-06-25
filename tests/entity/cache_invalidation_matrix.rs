use canon::entity::{
    EntityCacheKeyMaterial,
    artifact_chain::{EntityCacheDecision, EntityHashField},
    cache::{
        ENTITY_CACHE_CONTRACT_VERSION, ENTITY_CACHE_I21_FIELDS, ENTITY_CACHE_LAYERS,
        ENTITY_CACHE_UPSTREAM_BACKED_LAYERS, EntityCacheKey, EntityCacheLayer, compare_cache_keys,
        invalidated_layers_for_changed_fields, invalidated_layers_for_hash_field,
        required_hash_fields_for_layer,
    },
};

#[test]
fn cache_invalidation_matrix_covers_i21_and_upstream_hashes() {
    assert_eq!(ENTITY_CACHE_CONTRACT_VERSION, "canon_entity_cache.v0");

    for layer in ENTITY_CACHE_LAYERS {
        let fields = required_hash_fields_for_layer(*layer);
        for required in ENTITY_CACHE_I21_FIELDS {
            assert!(
                fields.contains(required),
                "{} missing {}",
                layer.as_str(),
                required.as_str()
            );
        }

        if *layer == EntityCacheLayer::NormalizedSurfaces {
            assert!(!fields.contains(&EntityHashField::UpstreamArtifactHash));
        } else {
            assert!(fields.contains(&EntityHashField::UpstreamArtifactHash));
        }
    }
}

#[test]
fn cache_invalidation_matrix_invalidates_exact_layers() {
    assert_eq!(
        invalidated_layers_for_hash_field(EntityHashField::InputHash),
        ENTITY_CACHE_LAYERS
    );
    assert_eq!(
        invalidated_layers_for_hash_field(EntityHashField::StrategyHash),
        ENTITY_CACHE_LAYERS
    );
    assert_eq!(
        invalidated_layers_for_hash_field(EntityHashField::PatchHash),
        ENTITY_CACHE_LAYERS
    );
    assert_eq!(
        invalidated_layers_for_hash_field(EntityHashField::NamekitHash),
        ENTITY_CACHE_LAYERS
    );
    assert_eq!(
        invalidated_layers_for_hash_field(EntityHashField::UpstreamArtifactHash),
        ENTITY_CACHE_UPSTREAM_BACKED_LAYERS
    );
    assert!(
        !invalidated_layers_for_hash_field(EntityHashField::UpstreamArtifactHash)
            .contains(&EntityCacheLayer::NormalizedSurfaces)
    );
}

#[test]
fn cache_hit_miss_hash_parts_are_deterministic() {
    let current = sample_key(EntityCacheLayer::TokenPostings);
    let hit = compare_cache_keys(&current, &current);
    assert_eq!(hit.decision, EntityCacheDecision::Hit);
    assert!(hit.changed_fields.is_empty());
    assert!(hit.invalidated_layers.is_empty());

    let mut cached = current.clone();
    cached.upstream_artifact_hash = Some("blake3:old-prepare".to_string());
    cached.patch_hash = Some("blake3:old-patch".to_string());
    cached.namekit_hash = Some("blake3:old-namekit".to_string());
    let miss = compare_cache_keys(&cached, &current);

    assert_eq!(miss.decision, EntityCacheDecision::Miss);
    assert_eq!(
        miss.changed_fields,
        [
            EntityHashField::UpstreamArtifactHash,
            EntityHashField::PatchHash,
            EntityHashField::NamekitHash,
        ]
    );
    assert_eq!(miss.invalidated_layers, ENTITY_CACHE_LAYERS);

    let json = serde_json::to_string(&miss).expect("cache invalidation serializes");
    assert!(json.contains("upstream_artifact_hash"));
    assert!(json.contains("patch_hash"));
    assert!(json.contains("namekit_hash"));
    assert!(!json.contains(".0"));
}

#[test]
fn upstream_only_change_keeps_normalized_surface_cache_valid() {
    let layers = invalidated_layers_for_changed_fields(&[EntityHashField::UpstreamArtifactHash]);

    assert_eq!(layers, ENTITY_CACHE_UPSTREAM_BACKED_LAYERS);
    assert!(!layers.contains(&EntityCacheLayer::NormalizedSurfaces));
    assert!(layers.contains(&EntityCacheLayer::TokenDictionary));
    assert!(layers.contains(&EntityCacheLayer::CandidateBlocks));
}

fn sample_key(layer: EntityCacheLayer) -> EntityCacheKey {
    EntityCacheKey::from_i21_material(
        layer,
        EntityCacheKeyMaterial {
            input_hash: "blake3:input".to_string(),
            profile_hash: "blake3:profile".to_string(),
            strategy_hash: "blake3:strategy".to_string(),
            registry_snapshot_hash: "blake3:registry".to_string(),
            patch_hash: Some("blake3:patch".to_string()),
            namekit_version: "namekit.v0".to_string(),
            namekit_hash: Some("blake3:namekit".to_string()),
        },
    )
    .with_upstream_artifact_hash("blake3:prepare")
}
