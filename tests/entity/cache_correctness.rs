use canon::entity::{
    EntityCacheKeyMaterial,
    artifact_chain::{EntityCacheDecision, EntityHashField},
    block::{
        BlockCandidateBudgetConfig, BlockCandidateBudgetObservation,
        validate_block_candidate_budget_before_artifact_emission,
    },
    cache::{
        ENTITY_CACHE_LAYERS, ENTITY_CACHE_UPSTREAM_BACKED_LAYERS, EntityCacheKey, EntityCacheLayer,
        compare_cache_keys,
    },
};

#[test]
fn entity_cache_correctness_unchanged_rerun_reuses_normalization_and_postings() {
    let previous_normalized = cache_key(EntityCacheLayer::NormalizedSurfaces);
    let previous_postings = cache_key(EntityCacheLayer::NgramPostings);

    let normalized = compare_cache_keys(&previous_normalized, &previous_normalized);
    let postings = compare_cache_keys(&previous_postings, &previous_postings);

    assert_eq!(normalized.decision, EntityCacheDecision::Hit);
    assert_eq!(postings.decision, EntityCacheDecision::Hit);
    assert!(normalized.changed_fields.is_empty());
    assert!(postings.changed_fields.is_empty());
    assert!(normalized.invalidated_layers.is_empty());
    assert!(postings.invalidated_layers.is_empty());
    assert!(
        cache_plan_allows_downstream_finalization(&[normalized, postings]),
        "unchanged rerun can reuse normalization/postings and finalize downstream artifacts"
    );
}

#[allow(non_snake_case)]
#[test]
fn G10_patch_change_invalidates_cache_layers_and_blocks_downstream_finalization() {
    let cached_normalized = cache_key(EntityCacheLayer::NormalizedSurfaces);
    let cached_postings = cache_key(EntityCacheLayer::NgramPostings);
    let mut current_normalized = cached_normalized.clone();
    let mut current_postings = cached_postings.clone();
    current_normalized.patch_hash = Some("blake3:patch-v2".to_string());
    current_postings.patch_hash = Some("blake3:patch-v2".to_string());

    let normalized = compare_cache_keys(&cached_normalized, &current_normalized);
    let postings = compare_cache_keys(&cached_postings, &current_postings);

    assert_eq!(normalized.decision, EntityCacheDecision::Miss);
    assert_eq!(postings.decision, EntityCacheDecision::Miss);
    assert_eq!(normalized.changed_fields, [EntityHashField::PatchHash]);
    assert_eq!(postings.changed_fields, [EntityHashField::PatchHash]);
    assert_eq!(normalized.invalidated_layers, ENTITY_CACHE_LAYERS);
    assert_eq!(postings.invalidated_layers, ENTITY_CACHE_LAYERS);
    assert!(
        !cache_plan_allows_downstream_finalization(&[normalized, postings]),
        "stale cache refusal must prevent downstream artifact finalization"
    );
}

#[test]
fn cache_hash_dimension_matrix_invalidates_precisely_one_dimension_at_a_time() {
    for (case, changed_field, expected_layers) in [
        (
            "input",
            EntityHashField::InputHash,
            ENTITY_CACHE_LAYERS.to_vec(),
        ),
        (
            "profile",
            EntityHashField::ProfileHash,
            ENTITY_CACHE_LAYERS.to_vec(),
        ),
        (
            "strategy",
            EntityHashField::StrategyHash,
            ENTITY_CACHE_LAYERS.to_vec(),
        ),
        (
            "registry",
            EntityHashField::RegistrySnapshotHash,
            ENTITY_CACHE_LAYERS.to_vec(),
        ),
        (
            "patch",
            EntityHashField::PatchHash,
            ENTITY_CACHE_LAYERS.to_vec(),
        ),
        (
            "namekit_version",
            EntityHashField::NamekitVersion,
            ENTITY_CACHE_LAYERS.to_vec(),
        ),
        (
            "namekit_hash",
            EntityHashField::NamekitHash,
            ENTITY_CACHE_LAYERS.to_vec(),
        ),
        (
            "upstream",
            EntityHashField::UpstreamArtifactHash,
            ENTITY_CACHE_UPSTREAM_BACKED_LAYERS.to_vec(),
        ),
    ] {
        let cached = cache_key(EntityCacheLayer::NgramPostings);
        let current = changed_key(&cached, case);
        let invalidation = compare_cache_keys(&cached, &current);

        assert_eq!(invalidation.decision, EntityCacheDecision::Miss, "{case}");
        assert_eq!(invalidation.changed_fields, [changed_field], "{case}");
        assert_eq!(invalidation.invalidated_layers, expected_layers, "{case}");
    }
}

#[test]
fn candidate_pair_metrics_are_reported_and_gateable() {
    let observations = candidate_budget_observations();
    let emitted: u64 = observations
        .iter()
        .map(|observation| observation.emitted_candidate_count)
        .sum();
    let diagnostics = validate_block_candidate_budget_before_artifact_emission(
        &BlockCandidateBudgetConfig::new(25, emitted, emitted),
        &observations,
    )
    .expect("candidate metrics stay inside configured caps");

    assert_eq!(diagnostics.candidate_pairs_emitted, emitted);
    assert_eq!(diagnostics.candidate_pairs_per_surface_p50, 8);
    assert_eq!(diagnostics.candidate_pairs_per_surface_p95, 16);
    assert_eq!(diagnostics.candidate_pairs_per_surface_p99, 25);
    assert_eq!(diagnostics.max_candidates_for_surface, 25);
    assert_eq!(diagnostics.suppressed_candidate_count, 150);
    assert!(diagnostics.candidate_budget.validated);
    assert!(!diagnostics.partial_candidate_artifact_written);
}

fn cache_plan_allows_downstream_finalization(
    invalidations: &[canon::entity::cache::EntityCacheInvalidation],
) -> bool {
    invalidations
        .iter()
        .all(|invalidation| invalidation.decision == EntityCacheDecision::Hit)
}

fn cache_key(layer: EntityCacheLayer) -> EntityCacheKey {
    EntityCacheKey::from_i21_material(
        layer,
        EntityCacheKeyMaterial {
            input_hash: "blake3:input-v1".to_string(),
            profile_hash: "blake3:profile-v1".to_string(),
            strategy_hash: "blake3:strategy-v1".to_string(),
            registry_snapshot_hash: "blake3:registry-v1".to_string(),
            patch_hash: Some("blake3:patch-v1".to_string()),
            namekit_version: "namekit.v0".to_string(),
            namekit_hash: Some("blake3:namekit-v1".to_string()),
        },
    )
    .with_upstream_artifact_hash("blake3:prepare-v1")
}

fn changed_key(cached: &EntityCacheKey, case: &str) -> EntityCacheKey {
    let mut current = cached.clone();
    match case {
        "input" => current.input_hash = "blake3:input-v2".to_string(),
        "profile" => current.profile_hash = "blake3:profile-v2".to_string(),
        "strategy" => current.strategy_hash = "blake3:strategy-v2".to_string(),
        "registry" => current.registry_snapshot_hash = "blake3:registry-v2".to_string(),
        "patch" => current.patch_hash = Some("blake3:patch-v2".to_string()),
        "namekit_version" => current.namekit_version = "namekit.v1".to_string(),
        "namekit_hash" => current.namekit_hash = Some("blake3:namekit-v2".to_string()),
        "upstream" => current.upstream_artifact_hash = Some("blake3:prepare-v2".to_string()),
        _ => unreachable!("unknown cache dimension"),
    }
    current
}

fn candidate_budget_observations() -> Vec<BlockCandidateBudgetObservation> {
    (0..100)
        .map(|index| {
            let emitted = if index < 50 {
                8
            } else if index < 95 {
                16
            } else {
                25
            };
            BlockCandidateBudgetObservation::new(
                format!("surf:cache:{index:03}"),
                "ngram_topk:cache_correctness",
                emitted,
                1 + u64::from(index % 2 == 0),
            )
        })
        .collect()
}
