use canon::entity::{
    ENTITY_ARTIFACT_VERSIONS, ENTITY_GATE_IDS, ENTITY_INVARIANT_IDS, ENTITY_REFUSAL_CODES,
    schema::ENTITY_SCHEMA_SNAPSHOTS,
};
use serde_json::Value;
use std::collections::BTreeSet;

#[test]
fn entity_contracts_golden_artifact_versions_match_schema_registry() {
    let goldens = contract_goldens();
    assert_eq!(
        goldens["bundle_version"],
        "canon_entity_contract_goldens.v0"
    );

    let artifacts = golden_artifacts(&goldens);
    let golden_keys = artifacts
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let schema_keys = ENTITY_SCHEMA_SNAPSHOTS
        .iter()
        .map(|snapshot| snapshot.schema_key)
        .collect::<BTreeSet<_>>();
    assert_eq!(golden_keys, schema_keys);

    let canonical_versions = ENTITY_ARTIFACT_VERSIONS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    for snapshot in ENTITY_SCHEMA_SNAPSHOTS {
        let artifact = artifacts
            .get(snapshot.schema_key)
            .unwrap_or_else(|| panic!("missing golden {}", snapshot.schema_key));
        assert_eq!(artifact["version"], snapshot.artifact_version);
        if canonical_versions.contains(snapshot.artifact_version) {
            assert!(
                ENTITY_ARTIFACT_VERSIONS.contains(&snapshot.artifact_version),
                "canonical artifact version disappeared: {}",
                snapshot.artifact_version
            );
        }
    }
}

#[test]
fn entity_contracts_golden_metadata_fields_are_complete_and_timestamp_free() {
    let goldens = contract_goldens();
    let serialized = serde_json::to_string(&goldens).expect("goldens serialize");
    for forbidden in ["timestamp", "created_at", "updated_at", "wall_clock"] {
        assert!(
            !serialized.contains(forbidden),
            "contract golden contains nondeterministic field {forbidden}"
        );
    }

    for snapshot in ENTITY_SCHEMA_SNAPSHOTS {
        let artifact = &golden_artifacts(&goldens)[snapshot.schema_key];
        assert_non_empty_path(artifact, &["version"]);
        assert_object_path(artifact, &["summary"]);
        if snapshot.schema_key != "canon_entity_surface_row.v0" {
            assert_hash_path(artifact, &["artifact_content_hash"]);
        }

        for path in [
            &["metadata", "profile", "id"][..],
            &["metadata", "profile", "version"][..],
            &["metadata", "profile", "entity_type"][..],
            &["metadata", "profile", "identity_semantics"][..],
            &["metadata", "profile", "canonical_type"][..],
            &["metadata", "profile", "content_hash"][..],
            &["metadata", "profile", "patch_namespaces", "aliases"][..],
            &["metadata", "profile", "patch_namespaces", "distinct"][..],
            &["metadata", "profile", "patch_namespaces", "relations"][..],
            &["metadata", "strategy", "content_hash"][..],
            &["metadata", "registry_snapshot", "lookup_snapshot_hash"][..],
            &["metadata", "input", "content_hash"][..],
            &["metadata", "patch_set", "content_hash"][..],
            &["metadata", "namekit", "content_hash"][..],
            &["metadata", "patch_namespace"][..],
            &["metadata", "artifact_content_hash"][..],
        ] {
            assert_non_empty_path(artifact, path);
        }

        for path in [
            &["metadata", "profile", "content_hash"][..],
            &["metadata", "strategy", "content_hash"][..],
            &["metadata", "registry_snapshot", "lookup_snapshot_hash"][..],
            &["metadata", "input", "content_hash"][..],
            &["metadata", "patch_set", "content_hash"][..],
            &["metadata", "namekit", "content_hash"][..],
            &["metadata", "artifact_content_hash"][..],
        ] {
            assert_hash_path(artifact, path);
        }
    }
}

#[test]
fn entity_contracts_golden_refusal_taxonomy_is_stable_and_documented() {
    assert_eq!(
        ENTITY_REFUSAL_CODES,
        [
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
        ]
    );

    let plan = include_str!("../docs/PLAN_ENTITY_WORKBENCH.md");
    for code in ENTITY_REFUSAL_CODES {
        assert!(plan.contains(code), "plan does not document {code}");
    }
}

#[test]
fn entity_contracts_golden_invariant_and_gate_ids_are_documented() {
    let plan = include_str!("../docs/PLAN_ENTITY_WORKBENCH.md");

    for id in ENTITY_INVARIANT_IDS {
        assert!(plan.contains(id), "plan does not document invariant {id}");
    }
    for id in ENTITY_GATE_IDS {
        assert!(plan.contains(id), "plan does not document gate {id}");
    }
}

fn contract_goldens() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/entity/contracts/entity_artifact_goldens.json"
    ))
    .expect("contract goldens parse")
}

fn golden_artifacts(goldens: &Value) -> &serde_json::Map<String, Value> {
    goldens["artifacts"]
        .as_object()
        .expect("golden artifacts object")
}

fn assert_object_path(value: &Value, path: &[&str]) {
    let actual = descend(value, path);
    assert!(
        actual.as_object().is_some(),
        "expected object at {}, got {actual:?}",
        path.join(".")
    );
}

fn assert_non_empty_path(value: &Value, path: &[&str]) {
    let actual = descend(value, path);
    assert!(
        actual.as_str().is_some_and(|text| !text.trim().is_empty()),
        "expected non-empty string at {}, got {actual:?}",
        path.join(".")
    );
}

fn assert_hash_path(value: &Value, path: &[&str]) {
    let actual = descend(value, path);
    assert!(
        actual
            .as_str()
            .is_some_and(|text| text.starts_with("blake3:")),
        "expected blake3 hash at {}, got {actual:?}",
        path.join(".")
    );
}

fn descend<'a>(value: &'a Value, path: &[&str]) -> &'a Value {
    let mut current = value;
    for segment in path {
        current = current
            .get(*segment)
            .unwrap_or_else(|| panic!("missing {}", path.join(".")));
    }
    current
}
