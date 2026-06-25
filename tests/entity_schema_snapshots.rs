use canon::{
    RefusalCode,
    entity::schema::{
        CANON_ENTITY_SURFACE_ROW_VERSION, ENTITY_CONTRACT_GOLDENS_FIXTURE,
        ENTITY_SCHEMA_BUNDLE_FIXTURE, ENTITY_SCHEMA_SNAPSHOTS, schema_snapshot_for_version,
        validate_artifact_core_contract,
    },
};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

#[test]
fn entity_schema_snapshots_cover_every_persisted_artifact_family() {
    let bundle = schema_bundle();
    let defs = bundle["$defs"].as_object().expect("schema defs object");
    let expected = ENTITY_SCHEMA_SNAPSHOTS
        .iter()
        .map(|snapshot| snapshot.schema_key)
        .collect::<BTreeSet<_>>();
    let actual = defs
        .keys()
        .filter(|key| key.starts_with("canon_entity_"))
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    assert_eq!(actual, expected);
    for snapshot in ENTITY_SCHEMA_SNAPSHOTS {
        let schema = defs
            .get(snapshot.schema_key)
            .unwrap_or_else(|| panic!("missing schema {}", snapshot.schema_key));
        assert_eq!(
            schema["properties"]["version"]["const"],
            snapshot.artifact_version
        );
        assert_eq!(
            schema_snapshot_for_version(snapshot.artifact_version)
                .expect("snapshot lookup")
                .schema_key,
            snapshot.schema_key
        );
    }
}

#[test]
fn entity_schema_snapshots_are_sorted_and_timestamp_free() {
    let bundle = schema_bundle();
    let defs = bundle["$defs"].as_object().expect("schema defs object");
    let keys = defs.keys().cloned().collect::<Vec<_>>();
    let sorted = keys.iter().cloned().collect::<BTreeSet<_>>();
    assert_eq!(keys, sorted.into_iter().collect::<Vec<_>>());

    let serialized = serde_json::to_string(&bundle).expect("schema serializes");
    for forbidden in ["timestamp", "created_at", "updated_at", "wall_clock"] {
        assert!(
            !serialized.contains(forbidden),
            "schema snapshot contains timestamp-dependent field {forbidden}"
        );
    }
}

#[test]
fn entity_contracts_golden_validate_against_schema_snapshots() {
    let bundle = schema_bundle();
    let goldens = contract_goldens();
    let defs = bundle["$defs"].as_object().expect("schema defs object");
    let artifacts = goldens["artifacts"].as_object().expect("golden artifacts");

    for snapshot in ENTITY_SCHEMA_SNAPSHOTS {
        let schema = defs
            .get(snapshot.schema_key)
            .unwrap_or_else(|| panic!("missing schema {}", snapshot.schema_key));
        let artifact = artifacts
            .get(snapshot.schema_key)
            .unwrap_or_else(|| panic!("missing golden {}", snapshot.schema_key));

        let validated = validate_artifact_core_contract(artifact).expect("core contract validates");
        assert_eq!(validated.schema_key, snapshot.schema_key);
        assert_required_fields_present(snapshot.schema_key, schema, artifact);
        assert_no_extra_top_level_fields(snapshot.schema_key, schema, artifact);
    }
}

#[test]
fn artifact_schema_required_fields_refuse_with_entity_artifact_contract() {
    assert_required_field_refusal(&["metadata", "profile", "identity_semantics"]);
    assert_required_field_refusal(&["metadata", "strategy", "content_hash"]);
    assert_required_field_refusal(&["metadata", "registry_snapshot", "lookup_snapshot_hash"]);
    assert_required_field_refusal(&["metadata", "artifact_content_hash"]);

    let mut artifact = golden_artifact("canon_entity_prepare.v0");
    artifact["version"] = Value::String("canon_entity_prepare.v99".to_string());
    let refusal =
        validate_artifact_core_contract(&artifact).expect_err("stale version must refuse");
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
}

#[test]
#[allow(non_snake_case)]
fn E_ENTITY_ARTIFACT_CONTRACT_invalid_hash_prefix_refuses() {
    let mut artifact = golden_artifact("canon_entity_prepare.v0");
    artifact["metadata"]["strategy"]["content_hash"] = Value::String("sha256:strategy".to_string());

    let refusal =
        validate_artifact_core_contract(&artifact).expect_err("invalid hash prefix refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["field"], "metadata.strategy.content_hash");
}

fn assert_required_field_refusal(path: &[&str]) {
    let mut artifact = golden_artifact("canon_entity_prepare.v0");
    remove_path(&mut artifact, path);

    let refusal = validate_artifact_core_contract(&artifact).expect_err("missing field refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
}

fn assert_required_fields_present(schema_key: &str, schema: &Value, artifact: &Value) {
    let required = schema["required"]
        .as_array()
        .unwrap_or_else(|| panic!("{schema_key} required list"));
    let object = artifact
        .as_object()
        .unwrap_or_else(|| panic!("{schema_key} golden object"));
    for field in required {
        let field = field.as_str().expect("required field string");
        assert!(
            object.contains_key(field),
            "{schema_key} golden missing required field {field}"
        );
    }

    assert!(object.contains_key("version"));
    assert!(object.contains_key("metadata"));
    assert!(object.contains_key("summary"));
    if schema_key != CANON_ENTITY_SURFACE_ROW_VERSION {
        assert!(object.contains_key("artifact_content_hash"));
    }
}

fn assert_no_extra_top_level_fields(schema_key: &str, schema: &Value, artifact: &Value) {
    assert_eq!(schema["additionalProperties"], false);
    let allowed = schema["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{schema_key} properties object"))
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let object = artifact
        .as_object()
        .unwrap_or_else(|| panic!("{schema_key} golden object"));
    for field in object.keys() {
        assert!(
            allowed.contains(field),
            "{schema_key} golden has extra field {field}"
        );
    }
}

fn remove_path(value: &mut Value, path: &[&str]) {
    if path.is_empty() {
        return;
    }
    let Some(parent) = descend_to_parent(value, &path[..path.len() - 1]) else {
        return;
    };
    parent.remove(path[path.len() - 1]);
}

fn descend_to_parent<'a>(
    value: &'a mut Value,
    path: &[&str],
) -> Option<&'a mut Map<String, Value>> {
    let mut current = value;
    for segment in path {
        current = current.get_mut(*segment)?;
    }
    current.as_object_mut()
}

fn golden_artifact(schema_key: &str) -> Value {
    contract_goldens()["artifacts"][schema_key].clone()
}

fn schema_bundle() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/entity/schemas/entity_artifact_schemas.schema.json"
    ))
    .expect("schema bundle parses")
}

fn contract_goldens() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/entity/contracts/entity_artifact_goldens.json"
    ))
    .expect("contract goldens parse")
}

#[test]
fn entity_schema_snapshot_fixture_paths_are_stable() {
    assert_eq!(
        ENTITY_SCHEMA_BUNDLE_FIXTURE,
        "tests/fixtures/entity/schemas/entity_artifact_schemas.schema.json"
    );
    assert_eq!(
        ENTITY_CONTRACT_GOLDENS_FIXTURE,
        "tests/fixtures/entity/contracts/entity_artifact_goldens.json"
    );
}
