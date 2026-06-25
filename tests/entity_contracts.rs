use canon::entity::{
    CANON_ENTITY_APPLY_VERSION, CANON_ENTITY_AUDIT_VERSION, CANON_ENTITY_BLOCK_BUCKET_VERSION,
    CANON_ENTITY_BLOCK_VERSION, CANON_ENTITY_DECISION_LEDGER_VERSION, CANON_ENTITY_EDGE_VERSION,
    CANON_ENTITY_EXPLAIN_VERSION, CANON_ENTITY_INDEX_VERSION, CANON_ENTITY_PREPARE_VERSION,
    CANON_ENTITY_PROJECTION_VERSION, CANON_ENTITY_PROMOTE_VERSION, CANON_ENTITY_RUN_VERSION,
    CANON_ENTITY_SOLVE_VERSION, ENTITY_ARTIFACT_VERSIONS, ENTITY_GATE_IDS, ENTITY_INVARIANT_IDS,
    ENTITY_REFUSAL_CODES, EntityArtifactHeader, EntityArtifactMetadata, EntityCacheKeyMaterial,
    EntityDeterministicSummary, EntityInputReference, EntityNamekitReference,
    EntityPatchSetReference, EntityProfileReference, EntityRegistrySnapshot,
    EntityStrategyReference,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn entity_contracts_artifact_version_constants_are_stable() {
    assert_eq!(
        ENTITY_ARTIFACT_VERSIONS,
        [
            CANON_ENTITY_PROJECTION_VERSION,
            CANON_ENTITY_PREPARE_VERSION,
            CANON_ENTITY_INDEX_VERSION,
            CANON_ENTITY_BLOCK_VERSION,
            CANON_ENTITY_BLOCK_BUCKET_VERSION,
            CANON_ENTITY_EDGE_VERSION,
            CANON_ENTITY_SOLVE_VERSION,
            CANON_ENTITY_RUN_VERSION,
            CANON_ENTITY_DECISION_LEDGER_VERSION,
            CANON_ENTITY_AUDIT_VERSION,
            CANON_ENTITY_PROMOTE_VERSION,
            CANON_ENTITY_EXPLAIN_VERSION,
            CANON_ENTITY_APPLY_VERSION,
        ]
    );

    assert_eq!(CANON_ENTITY_PREPARE_VERSION, "canon_entity_prepare.v0");
    assert_eq!(
        CANON_ENTITY_BLOCK_BUCKET_VERSION,
        "canon_entity_block_bucket.v0"
    );
    assert_eq!(
        CANON_ENTITY_DECISION_LEDGER_VERSION,
        "canon_entity_decision_ledger.v0"
    );
}

#[test]
fn entity_contracts_cover_plan_invariants_gates_and_refusal_codes() {
    let invariants = ENTITY_INVARIANT_IDS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    for number in 1..=25 {
        let id = format!("I{number:02}");
        assert!(invariants.contains(id.as_str()), "missing {id}");
    }

    let gates = ENTITY_GATE_IDS.iter().copied().collect::<BTreeSet<_>>();
    for number in 1..=15 {
        let id = format!("G{number:02}");
        assert!(gates.contains(id.as_str()), "missing {id}");
    }

    let refusal_codes = ENTITY_REFUSAL_CODES
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    for required in [
        "E_ENTITY_PROFILE",
        "E_ENTITY_STRATEGY",
        "E_ENTITY_INPUT_CONTRACT",
        "E_ENTITY_SURFACE_ID_COLLISION",
        "E_ENTITY_PATCH_CONFLICT",
        "E_ENTITY_REGISTRY_SNAPSHOT",
        "E_ENTITY_CACHE_MISMATCH",
        "E_ENTITY_INDEX_LIMIT",
        "E_ENTITY_CANDIDATE_BUDGET",
        "E_ENTITY_ARTIFACT_CONTRACT",
        "E_ENTITY_CANNOT_LINK_OVERRIDE",
        "E_ENTITY_REVIEW_IMPORT",
        "E_ENTITY_AUDIT_GATE",
        "E_ENTITY_APPLY_UNRESOLVED",
        "E_ENTITY_IO_BUDGET",
    ] {
        assert!(refusal_codes.contains(required), "missing {required}");
    }
}

#[test]
fn entity_profile_reference_requires_identity_semantics_fields() {
    let incomplete = EntityProfileReference {
        id: "cmbs_tenant_label".to_string(),
        version: "0.1.0".to_string(),
        ..EntityProfileReference::default()
    };
    assert!(!incomplete.is_complete());

    let complete = EntityProfileReference {
        id: "cmbs_tenant_label".to_string(),
        version: "0.1.0".to_string(),
        entity_type: "tenant_label".to_string(),
        identity_semantics: "canonical_display_label".to_string(),
        canonical_type: "tenant_label".to_string(),
        content_hash: Some("blake3:profile".to_string()),
    };
    assert!(complete.is_complete());
}

#[test]
fn entity_artifact_metadata_serializes_required_hash_fields() {
    let metadata = sample_metadata();
    let value = serde_json::to_value(&metadata).expect("metadata serializes");

    assert_eq!(value["profile"]["id"], "cmbs_tenant_label");
    assert_eq!(
        value["profile"]["identity_semantics"],
        "canonical_display_label"
    );
    assert_eq!(value["strategy"]["content_hash"], "blake3:strategy");
    assert_eq!(
        value["registry_snapshot"]["lookup_snapshot_hash"],
        "blake3:registry"
    );
    assert_eq!(value["input"]["content_hash"], "blake3:input");
    assert_eq!(value["patch_set"]["content_hash"], "blake3:patch");
    assert_eq!(value["namekit"]["content_hash"], "blake3:namekit");
    assert_eq!(value["artifact_content_hash"], "blake3:artifact");
}

#[test]
fn entity_artifact_header_round_trips_with_deterministic_summary() {
    let mut counts = BTreeMap::new();
    counts.insert("prepared_surfaces".to_string(), 3);
    counts.insert("raw_unique_surfaces".to_string(), 5);

    let header = EntityArtifactHeader {
        version: CANON_ENTITY_PREPARE_VERSION.to_string(),
        metadata: sample_metadata(),
        summary: EntityDeterministicSummary {
            counts,
            labels: BTreeMap::from([("cache_status".to_string(), "rebuilt".to_string())]),
        },
    };

    let json = serde_json::to_string(&header).expect("header serializes");
    assert!(
        json.find("prepared_surfaces").unwrap() < json.find("raw_unique_surfaces").unwrap(),
        "BTreeMap keeps summary keys deterministic"
    );

    let round_tripped: EntityArtifactHeader =
        serde_json::from_str(&json).expect("header deserializes");
    assert_eq!(round_tripped, header);
}

#[test]
fn entity_cache_key_material_names_i21_hash_inputs() {
    let key = EntityCacheKeyMaterial {
        input_hash: "blake3:input".to_string(),
        profile_hash: "blake3:profile".to_string(),
        strategy_hash: "blake3:strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
        patch_hash: Some("blake3:patch".to_string()),
        namekit_version: "namekit.v0".to_string(),
        namekit_hash: Some("blake3:namekit".to_string()),
    };

    let value = serde_json::to_value(key).expect("cache key serializes");
    assert_eq!(
        value,
        json!({
            "input_hash": "blake3:input",
            "profile_hash": "blake3:profile",
            "strategy_hash": "blake3:strategy",
            "registry_snapshot_hash": "blake3:registry",
            "patch_hash": "blake3:patch",
            "namekit_version": "namekit.v0",
            "namekit_hash": "blake3:namekit"
        })
    );
}

fn sample_metadata() -> EntityArtifactMetadata {
    EntityArtifactMetadata {
        profile: EntityProfileReference {
            id: "cmbs_tenant_label".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "tenant_label".to_string(),
            identity_semantics: "canonical_display_label".to_string(),
            canonical_type: "tenant_label".to_string(),
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
        input: Some(EntityInputReference {
            row_count: 500_000,
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
        artifact_content_hash: "blake3:artifact".to_string(),
    }
}
