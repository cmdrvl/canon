//! Offline entity artifact schema snapshot registry.
//!
//! The workbench stages exchange persisted JSON/JSONL artifacts. This module
//! names the schema snapshots used by tests and downstream stages without
//! introducing a runtime schema registry or network dependency.

use super::contracts::{
    CANON_ENTITY_APPLY_VERSION, CANON_ENTITY_AUDIT_VERSION, CANON_ENTITY_BLOCK_BUCKET_VERSION,
    CANON_ENTITY_BLOCK_VERSION, CANON_ENTITY_DECISION_LEDGER_VERSION, CANON_ENTITY_EDGE_VERSION,
    CANON_ENTITY_EXPLAIN_VERSION, CANON_ENTITY_INDEX_VERSION, CANON_ENTITY_PREPARE_VERSION,
    CANON_ENTITY_PROJECTION_VERSION, CANON_ENTITY_PROMOTE_VERSION, CANON_ENTITY_RUN_VERSION,
    CANON_ENTITY_SOLVE_VERSION, EntityArtifactContractDescriptor, EntityArtifactReferenceV1,
    EntityArtifactWorkdirLayoutV1, entity_artifact_v1_contract_for_legacy_version,
    entity_artifact_v1_contract_for_version,
};
use super::error::EntityRefusalKind;
use crate::Refusal;
use serde_json::{Map, Value, json};

pub const CANON_ENTITY_SCHEMA_BUNDLE_VERSION: &str = "canon_entity_schema_bundle.v0";
pub const CANON_ENTITY_SURFACE_ROW_VERSION: &str = "canon_entity_surface_row.v0";
pub const CANON_ENTITY_REVIEW_QUEUE_VERSION: &str = "canon_entity_review_queue.v0";
pub const CANON_ENTITY_REVIEW_IMPORT_VERSION: &str = "canon_entity_review_import.v0";
pub const CANON_ENTITY_PROMOTION_PROOF_VERSION: &str = "canon_entity_promotion_proof.v0";
pub const CANON_ENTITY_PROMOTION_SIDECAR_VERSION: &str = "canon_entity_promotion_sidecar.v0";

pub const ENTITY_SCHEMA_BUNDLE_FIXTURE: &str =
    "tests/fixtures/entity/schemas/entity_artifact_schemas.schema.json";
pub const ENTITY_CONTRACT_GOLDENS_FIXTURE: &str =
    "tests/fixtures/entity/contracts/entity_artifact_goldens.json";
pub const CANON_ENTITY_ARTIFACT_SCHEMA_BUNDLE_VERSION_V1: &str = "canon_entity_artifact_schemas.v1";
pub const ENTITY_ARTIFACT_V1_SCHEMA_FIXTURE: &str =
    "tests/fixtures/entity/contracts/entity_artifact_schemas.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntitySchemaSnapshot {
    pub artifact_version: &'static str,
    pub schema_key: &'static str,
}

pub const ENTITY_SCHEMA_SNAPSHOTS: &[EntitySchemaSnapshot] = &[
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_APPLY_VERSION,
        schema_key: "canon_entity_apply.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_AUDIT_VERSION,
        schema_key: "canon_entity_audit.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_BLOCK_VERSION,
        schema_key: "canon_entity_block.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_BLOCK_BUCKET_VERSION,
        schema_key: "canon_entity_block_bucket.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_DECISION_LEDGER_VERSION,
        schema_key: "canon_entity_decision_ledger.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_EDGE_VERSION,
        schema_key: "canon_entity_edge.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_EXPLAIN_VERSION,
        schema_key: "canon_entity_explain.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_INDEX_VERSION,
        schema_key: "canon_entity_index.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_PREPARE_VERSION,
        schema_key: "canon_entity_prepare.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_PROJECTION_VERSION,
        schema_key: "canon_entity_projection.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_PROMOTE_VERSION,
        schema_key: "canon_entity_promote.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_PROMOTION_PROOF_VERSION,
        schema_key: "canon_entity_promotion_proof.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_PROMOTION_SIDECAR_VERSION,
        schema_key: "canon_entity_promotion_sidecar.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_REVIEW_IMPORT_VERSION,
        schema_key: "canon_entity_review_import.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_REVIEW_QUEUE_VERSION,
        schema_key: "canon_entity_review_queue.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_RUN_VERSION,
        schema_key: "canon_entity_run.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_SOLVE_VERSION,
        schema_key: "canon_entity_solve.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_SURFACE_ROW_VERSION,
        schema_key: "canon_entity_surface_row.v0",
    },
];

pub fn schema_snapshot_for_version(version: &str) -> Option<&'static EntitySchemaSnapshot> {
    ENTITY_SCHEMA_SNAPSHOTS
        .iter()
        .find(|snapshot| snapshot.artifact_version == version)
}

pub fn validate_artifact_core_contract(
    artifact: &Value,
) -> Result<&'static EntitySchemaSnapshot, Refusal> {
    let object = artifact.as_object().ok_or_else(|| {
        artifact_contract_refusal(
            "Entity artifact must be a JSON object",
            json!({ "field": "$" }),
        )
    })?;
    let version = required_string(object, "version", "version")?;
    let snapshot = schema_snapshot_for_version(version).ok_or_else(|| {
        artifact_contract_refusal(
            "Entity artifact version has no schema snapshot",
            json!({ "version": version }),
        )
    })?;

    required_object(object, "summary", "summary")?;
    let metadata = required_object(object, "metadata", "metadata")?;

    if snapshot.artifact_version != CANON_ENTITY_SURFACE_ROW_VERSION {
        required_hash(object, "artifact_content_hash", "artifact_content_hash")?;
    }

    for (field, path) in [
        ("artifact_content_hash", "metadata.artifact_content_hash"),
        ("patch_namespace", "metadata.patch_namespace"),
    ] {
        required_string(metadata, field, path)?;
    }

    required_hash(
        metadata,
        "artifact_content_hash",
        "metadata.artifact_content_hash",
    )?;
    let profile = required_object(metadata, "profile", "metadata.profile")?;
    for (field, path) in [
        ("id", "metadata.profile.id"),
        ("version", "metadata.profile.version"),
        ("entity_type", "metadata.profile.entity_type"),
        ("identity_semantics", "metadata.profile.identity_semantics"),
        ("canonical_type", "metadata.profile.canonical_type"),
        ("content_hash", "metadata.profile.content_hash"),
    ] {
        required_string(profile, field, path)?;
    }

    let namespaces = required_object(
        profile,
        "patch_namespaces",
        "metadata.profile.patch_namespaces",
    )?;
    for (field, path) in [
        ("aliases", "metadata.profile.patch_namespaces.aliases"),
        ("distinct", "metadata.profile.patch_namespaces.distinct"),
        ("relations", "metadata.profile.patch_namespaces.relations"),
    ] {
        required_string(namespaces, field, path)?;
    }

    let strategy = required_object(metadata, "strategy", "metadata.strategy")?;
    required_hash(strategy, "content_hash", "metadata.strategy.content_hash")?;
    let registry = required_object(metadata, "registry_snapshot", "metadata.registry_snapshot")?;
    required_hash(
        registry,
        "lookup_snapshot_hash",
        "metadata.registry_snapshot.lookup_snapshot_hash",
    )?;
    let input = required_object(metadata, "input", "metadata.input")?;
    required_hash(input, "content_hash", "metadata.input.content_hash")?;
    let patch_set = required_object(metadata, "patch_set", "metadata.patch_set")?;
    required_hash(patch_set, "content_hash", "metadata.patch_set.content_hash")?;
    let namekit = required_object(metadata, "namekit", "metadata.namekit")?;
    required_hash(namekit, "content_hash", "metadata.namekit.content_hash")?;

    Ok(snapshot)
}

pub fn validate_artifact_v1_core_contract(
    artifact: &Value,
) -> Result<&'static EntityArtifactContractDescriptor, Refusal> {
    let object = artifact.as_object().ok_or_else(|| {
        artifact_contract_refusal(
            "Entity v1 artifact must be a JSON object",
            json!({ "field": "$" }),
        )
    })?;
    let version = required_string(object, "version", "version")?;
    if let Some(contract) = entity_artifact_v1_contract_for_legacy_version(version) {
        return Err(artifact_contract_refusal(
            "Legacy entity artifact version cannot cross the v1 compatibility firewall",
            json!({
                "actual_version": version,
                "expected_version": contract.artifact_version,
                "stage": contract.stage.as_str(),
                "legacy_versions": contract.legacy_versions
            }),
        ));
    }
    let contract = entity_artifact_v1_contract_for_version(version).ok_or_else(|| {
        artifact_contract_refusal(
            "Entity v1 artifact version is not registered",
            json!({ "version": version }),
        )
    })?;

    let top_level_hash = required_hash(object, "artifact_content_hash", "artifact_content_hash")?;
    reject_forbidden_timestamp_keys(artifact)?;
    required_object(object, "summary", "summary")?;
    let metadata = required_object(object, "metadata", "metadata")?;
    let metadata_hash = required_hash(
        metadata,
        "artifact_content_hash",
        "metadata.artifact_content_hash",
    )?;
    if top_level_hash != metadata_hash {
        return Err(artifact_contract_refusal(
            "Entity v1 artifact content hash must match metadata.artifact_content_hash",
            json!({
                "field": "artifact_content_hash",
                "expected": metadata_hash,
                "actual": top_level_hash
            }),
        ));
    }

    let patch_namespace = required_string(metadata, "patch_namespace", "metadata.patch_namespace")?;
    let profile = required_object(metadata, "profile", "metadata.profile")?;
    let profile_id = required_string(profile, "id", "metadata.profile.id")?;
    for (field, path) in [
        ("version", "metadata.profile.version"),
        ("entity_type", "metadata.profile.entity_type"),
        ("identity_semantics", "metadata.profile.identity_semantics"),
        ("canonical_type", "metadata.profile.canonical_type"),
        ("content_hash", "metadata.profile.content_hash"),
    ] {
        required_string(profile, field, path)?;
    }
    required_hash(profile, "content_hash", "metadata.profile.content_hash")?;

    let namespaces = required_object(
        profile,
        "patch_namespaces",
        "metadata.profile.patch_namespaces",
    )?;
    let aliases = required_string(
        namespaces,
        "aliases",
        "metadata.profile.patch_namespaces.aliases",
    )?;
    let distinct = required_string(
        namespaces,
        "distinct",
        "metadata.profile.patch_namespaces.distinct",
    )?;
    let relations = required_string(
        namespaces,
        "relations",
        "metadata.profile.patch_namespaces.relations",
    )?;
    let profile_prefix = format!("{profile_id}.");
    let namespaces_are_scoped = [aliases, distinct, relations]
        .iter()
        .all(|namespace| namespace.starts_with(&profile_prefix));
    let patch_namespace_in_scope = [aliases, distinct, relations].contains(&patch_namespace);
    if !namespaces_are_scoped || !patch_namespace_in_scope {
        return Err(artifact_contract_refusal(
            "Entity v1 artifact crossed the profile firewall",
            json!({
                "field": "metadata.patch_namespace",
                "profile_id": profile_id,
                "patch_namespace": patch_namespace,
                "aliases": aliases,
                "distinct": distinct,
                "relations": relations
            }),
        ));
    }

    let strategy = required_object(metadata, "strategy", "metadata.strategy")?;
    for (field, path) in [
        ("id", "metadata.strategy.id"),
        ("version", "metadata.strategy.version"),
        ("content_hash", "metadata.strategy.content_hash"),
    ] {
        required_string(strategy, field, path)?;
    }
    required_hash(strategy, "content_hash", "metadata.strategy.content_hash")?;

    let registry = required_object(metadata, "registry_snapshot", "metadata.registry_snapshot")?;
    for (field, path) in [
        ("id", "metadata.registry_snapshot.id"),
        ("version", "metadata.registry_snapshot.version"),
        ("source", "metadata.registry_snapshot.source"),
        (
            "lookup_snapshot_hash",
            "metadata.registry_snapshot.lookup_snapshot_hash",
        ),
    ] {
        required_string(registry, field, path)?;
    }
    required_hash(
        registry,
        "lookup_snapshot_hash",
        "metadata.registry_snapshot.lookup_snapshot_hash",
    )?;

    let input = required_object(metadata, "input", "metadata.input")?;
    required_u64(input, "row_count", "metadata.input.row_count")?;
    required_hash(input, "content_hash", "metadata.input.content_hash")?;

    let schema = required_object(metadata, "schema", "metadata.schema")?;
    let schema_key = required_string(schema, "key", "metadata.schema.key")?;
    required_hash(schema, "content_hash", "metadata.schema.content_hash")?;
    if schema_key != contract.schema_key {
        return Err(artifact_contract_refusal(
            "Entity v1 artifact schema key does not match the registered contract",
            json!({
                "field": "metadata.schema.key",
                "expected": contract.schema_key,
                "actual": schema_key
            }),
        ));
    }

    let workdir = required_object(metadata, "workdir", "metadata.workdir")?;
    let declared_layout = EntityArtifactWorkdirLayoutV1 {
        root_dir: required_string(workdir, "root_dir", "metadata.workdir.root_dir")?.to_string(),
        stage_dir: required_string(workdir, "stage_dir", "metadata.workdir.stage_dir")?.to_string(),
        artifact_relpath: required_string(
            workdir,
            "artifact_relpath",
            "metadata.workdir.artifact_relpath",
        )?
        .to_string(),
        payload_relpath: required_string(
            workdir,
            "payload_relpath",
            "metadata.workdir.payload_relpath",
        )?
        .to_string(),
    };
    if !declared_layout.is_complete()
        || declared_layout.stage_dir != contract.stage_dir
        || declared_layout.artifact_relpath != contract.artifact_relpath
        || declared_layout.payload_relpath != contract.payload_relpath
    {
        return Err(artifact_contract_refusal(
            "Entity v1 artifact work-directory layout does not match the registered contract",
            json!({
                "field": "metadata.workdir",
                "expected": {
                    "stage_dir": contract.stage_dir,
                    "artifact_relpath": contract.artifact_relpath,
                    "payload_relpath": contract.payload_relpath
                },
                "actual": declared_layout
            }),
        ));
    }

    let upstream_artifacts = required_array(
        metadata,
        "upstream_artifacts",
        "metadata.upstream_artifacts",
    )?;
    let upstream_values = upstream_artifacts
        .iter()
        .map(parse_upstream_artifact_v1)
        .collect::<Result<Vec<_>, _>>()?;
    if !upstream_artifacts_are_sorted(&upstream_values) {
        return Err(artifact_contract_refusal(
            "Entity v1 upstream artifact references must stay in deterministic order",
            json!({
                "field": "metadata.upstream_artifacts",
                "actual": render_v1_upstream_refs(&upstream_values)
            }),
        ));
    }

    Ok(contract)
}

pub fn compute_entity_v1_self_hash(artifact: &Value) -> Result<String, Refusal> {
    validate_artifact_v1_core_contract(artifact)?;
    let mut hashable = artifact.clone();
    clear_v1_self_hash_fields(&mut hashable)?;
    Ok(format!(
        "blake3:{}",
        blake3::hash(canonical_json(&hashable).as_bytes()).to_hex()
    ))
}

pub fn validate_entity_v1_self_hash(artifact: &Value) -> Result<String, Refusal> {
    let object = artifact.as_object().ok_or_else(|| {
        artifact_contract_refusal(
            "Entity v1 artifact must be a JSON object",
            json!({ "field": "$" }),
        )
    })?;
    let actual = required_hash(object, "artifact_content_hash", "artifact_content_hash")?;
    let metadata = required_object(object, "metadata", "metadata")?;
    let metadata_actual = required_hash(
        metadata,
        "artifact_content_hash",
        "metadata.artifact_content_hash",
    )?;
    let expected = compute_entity_v1_self_hash(artifact)?;
    if actual != expected || metadata_actual != expected {
        return Err(artifact_contract_refusal(
            "Entity v1 artifact self-hash drifted from canonical content",
            json!({
                "field": "artifact_content_hash",
                "expected": expected,
                "actual": actual,
                "metadata_actual": metadata_actual
            }),
        ));
    }
    Ok(expected)
}

fn required_object<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    path: &str,
) -> Result<&'a Map<String, Value>, Refusal> {
    object
        .get(field)
        .and_then(Value::as_object)
        .ok_or_else(|| missing_field(path))
}

fn required_array<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    path: &str,
) -> Result<&'a Vec<Value>, Refusal> {
    object
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| missing_field(path))
}

fn required_hash<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    path: &str,
) -> Result<&'a str, Refusal> {
    let value = required_string(object, field, path)?;
    if value.starts_with("blake3:") {
        Ok(value)
    } else {
        Err(artifact_contract_refusal(
            "Entity artifact hash field must use blake3: prefix",
            json!({ "field": path, "actual": value }),
        ))
    }
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    path: &str,
) -> Result<&'a str, Refusal> {
    let value = object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| missing_field(path))?;
    if value.trim().is_empty() {
        Err(artifact_contract_refusal(
            "Entity artifact required string field is empty",
            json!({ "field": path }),
        ))
    } else {
        Ok(value)
    }
}

fn required_u64(object: &Map<String, Value>, field: &str, path: &str) -> Result<u64, Refusal> {
    object
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| missing_field(path))
}

fn parse_upstream_artifact_v1(value: &Value) -> Result<EntityArtifactReferenceV1, Refusal> {
    let object = value.as_object().ok_or_else(|| {
        artifact_contract_refusal(
            "Entity v1 upstream artifact reference must be an object",
            json!({ "field": "metadata.upstream_artifacts" }),
        )
    })?;
    let artifact = EntityArtifactReferenceV1 {
        version: required_string(object, "version", "metadata.upstream_artifacts.version")?
            .to_string(),
        schema_key: required_string(
            object,
            "schema_key",
            "metadata.upstream_artifacts.schema_key",
        )?
        .to_string(),
        schema_hash: required_hash(
            object,
            "schema_hash",
            "metadata.upstream_artifacts.schema_hash",
        )?
        .to_string(),
        content_hash: required_hash(
            object,
            "content_hash",
            "metadata.upstream_artifacts.content_hash",
        )?
        .to_string(),
    };
    if artifact.is_complete() {
        Ok(artifact)
    } else {
        Err(artifact_contract_refusal(
            "Entity v1 upstream artifact reference is incomplete",
            json!({ "field": "metadata.upstream_artifacts" }),
        ))
    }
}

fn upstream_artifacts_are_sorted(artifacts: &[EntityArtifactReferenceV1]) -> bool {
    artifacts
        .windows(2)
        .all(|window| artifact_sort_key(&window[0]) <= artifact_sort_key(&window[1]))
}

fn artifact_sort_key(artifact: &EntityArtifactReferenceV1) -> (&str, &str, &str, &str) {
    (
        artifact.version.as_str(),
        artifact.schema_key.as_str(),
        artifact.schema_hash.as_str(),
        artifact.content_hash.as_str(),
    )
}

fn render_v1_upstream_refs(artifacts: &[EntityArtifactReferenceV1]) -> Vec<String> {
    artifacts
        .iter()
        .map(|artifact| {
            format!(
                "{}#{}@{}",
                artifact.version, artifact.schema_key, artifact.content_hash
            )
        })
        .collect()
}

fn reject_forbidden_timestamp_keys(value: &Value) -> Result<(), Refusal> {
    if let Some(path) = find_forbidden_timestamp_key(value, "$".to_string()) {
        Err(artifact_contract_refusal(
            "Entity artifact contract forbids timestamp-dependent keys",
            json!({ "field": path }),
        ))
    } else {
        Ok(())
    }
}

fn find_forbidden_timestamp_key(value: &Value, path: String) -> Option<String> {
    const FORBIDDEN: &[&str] = &["timestamp", "created_at", "updated_at", "wall_clock"];
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let child_path = if path == "$" {
                    key.to_string()
                } else {
                    format!("{path}.{key}")
                };
                if FORBIDDEN.contains(&key.as_str()) {
                    return Some(child_path);
                }
                if let Some(found) = find_forbidden_timestamp_key(child, child_path) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(array) => array.iter().enumerate().find_map(|(index, child)| {
            find_forbidden_timestamp_key(child, format!("{path}[{index}]"))
        }),
        _ => None,
    }
}

fn clear_v1_self_hash_fields(value: &mut Value) -> Result<(), Refusal> {
    let object = value.as_object_mut().ok_or_else(|| {
        artifact_contract_refusal(
            "Entity v1 artifact must be a JSON object",
            json!({ "field": "$" }),
        )
    })?;
    object.insert(
        "artifact_content_hash".to_string(),
        Value::String(String::new()),
    );
    let metadata = object
        .get_mut("metadata")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| missing_field("metadata"))?;
    metadata.insert(
        "artifact_content_hash".to_string(),
        Value::String(String::new()),
    );
    Ok(())
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => serde_json::to_string(text).expect("string serializes"),
        Value::Array(array) => {
            let mut rendered = String::from("[");
            for (index, item) in array.iter().enumerate() {
                if index > 0 {
                    rendered.push(',');
                }
                rendered.push_str(&canonical_json(item));
            }
            rendered.push(']');
            rendered
        }
        Value::Object(object) => {
            let mut keys = object.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            let mut rendered = String::from("{");
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    rendered.push(',');
                }
                rendered.push_str(&serde_json::to_string(key).expect("canonical key serializes"));
                rendered.push(':');
                rendered.push_str(&canonical_json(&object[key]));
            }
            rendered.push('}');
            rendered
        }
    }
}

fn missing_field(path: &str) -> Refusal {
    artifact_contract_refusal(
        "Entity artifact is missing a required schema field",
        json!({ "missing": path }),
    )
}

fn artifact_contract_refusal(message: impl Into<String>, detail: Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(message, detail, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RefusalCode;
    use crate::entity::contracts::{
        CANON_ENTITY_AUDIT_VERSION_V1, CANON_ENTITY_BLOCK_VERSION_V1, CANON_ENTITY_EDGE_VERSION_V1,
        CANON_ENTITY_EXPLAIN_VERSION_V1, CANON_ENTITY_INDEX_VERSION_V1,
        CANON_ENTITY_PREPARE_VERSION_V1, CANON_ENTITY_PROJECT_VERSION_V1,
        CANON_ENTITY_PROMOTE_VERSION_V1, CANON_ENTITY_REVIEW_VERSION_V1,
        CANON_ENTITY_RUN_VERSION_V1, CANON_ENTITY_SOLVE_VERSION_V1, ENTITY_ARTIFACT_V1_CONTRACTS,
        EntityArtifactStageV1,
    };
    use serde_json::{Map, json};

    #[test]
    fn entity_v1_schema_fixture_matches_registered_contract_catalog() {
        let bundle = fixture_bundle();
        assert_eq!(
            bundle["bundle_version"],
            CANON_ENTITY_ARTIFACT_SCHEMA_BUNDLE_VERSION_V1
        );
        assert_eq!(
            ENTITY_ARTIFACT_V1_SCHEMA_FIXTURE,
            "tests/fixtures/entity/contracts/entity_artifact_schemas.json"
        );

        let artifacts = fixture_artifacts(&bundle);
        assert_eq!(artifacts.len(), ENTITY_ARTIFACT_V1_CONTRACTS.len());
        for artifact in artifacts {
            let version = artifact["artifact_version"]
                .as_str()
                .expect("fixture artifact version");
            let contract =
                entity_artifact_v1_contract_for_version(version).expect("registered contract");
            assert_eq!(artifact["stage"], contract.stage.as_str());
            assert_eq!(artifact["command"], contract.command);
            assert_eq!(artifact["schema_key"], contract.schema_key);
            assert_eq!(
                artifact["workdir"]["artifact_relpath"],
                contract.artifact_relpath
            );
            assert_eq!(
                artifact["workdir"]["payload_relpath"],
                contract.payload_relpath
            );
            let legacy_versions = artifact["legacy_versions"]
                .as_array()
                .expect("legacy versions");
            assert_eq!(legacy_versions.len(), contract.legacy_versions.len());
            for (actual, expected) in legacy_versions.iter().zip(contract.legacy_versions.iter()) {
                assert_eq!(actual, *expected);
            }

            let golden = &artifact["golden"];
            let validated = validate_artifact_v1_core_contract(golden).expect("golden validates");
            assert_eq!(validated.artifact_version, version);
            assert_stage_fields_present(artifact, golden);
        }
    }

    #[test]
    fn entity_v1_core_contract_refuses_legacy_versions_and_missing_profile_scope() {
        let mut artifact = sample_artifact(
            CANON_ENTITY_RUN_VERSION_V1,
            "run_manifest_path",
            "run/manifest.json",
        );
        artifact["version"] = Value::String("canon_entity_run.v0".to_string());
        let refusal =
            validate_artifact_v1_core_contract(&artifact).expect_err("legacy run refuses");
        assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
        assert_eq!(refusal.detail["actual_version"], "canon_entity_run.v0");
        assert_eq!(
            refusal.detail["expected_version"],
            CANON_ENTITY_RUN_VERSION_V1
        );

        let mut missing_scope = sample_artifact(
            CANON_ENTITY_REVIEW_VERSION_V1,
            "review_queue_path",
            "review/queue.jsonl",
        );
        remove_path(
            &mut missing_scope,
            &["metadata", "profile", "identity_semantics"],
        );
        let refusal =
            validate_artifact_v1_core_contract(&missing_scope).expect_err("missing scope refuses");
        assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    }

    #[test]
    fn entity_v1_core_contract_refuses_unsorted_upstreams_and_timestamp_keys() {
        let mut artifact = sample_artifact(
            CANON_ENTITY_PROMOTE_VERSION_V1,
            "promotion_manifest_path",
            "promote/sidecar.json",
        );
        artifact["metadata"]["upstream_artifacts"] = json!([
            {
                "version": CANON_ENTITY_RUN_VERSION_V1,
                "schema_key": CANON_ENTITY_RUN_VERSION_V1,
                "schema_hash": "blake3:schema-run",
                "content_hash": "blake3:zz-run"
            },
            {
                "version": CANON_ENTITY_AUDIT_VERSION_V1,
                "schema_key": CANON_ENTITY_AUDIT_VERSION_V1,
                "schema_hash": "blake3:schema-audit",
                "content_hash": "blake3:aa-audit"
            }
        ]);
        let refusal =
            validate_artifact_v1_core_contract(&artifact).expect_err("unsorted upstreams refuse");
        assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
        assert_eq!(refusal.detail["field"], "metadata.upstream_artifacts");

        let mut timestamped = sample_artifact(
            CANON_ENTITY_EXPLAIN_VERSION_V1,
            "explanation_path",
            "explain/evidence.json",
        );
        timestamped["summary"]["created_at"] = Value::String("2026-07-10T12:00:00Z".to_string());
        let refusal =
            validate_artifact_v1_core_contract(&timestamped).expect_err("timestamp refuses");
        assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    }

    #[test]
    fn entity_v1_self_hash_is_canonical_and_detects_drift() {
        let mut artifact = sample_artifact(
            CANON_ENTITY_SOLVE_VERSION_V1,
            "entities_path",
            "solve/entities.jsonl",
        );
        set_self_hash(&mut artifact);

        let first = validate_entity_v1_self_hash(&artifact).expect("self hash validates");
        let second = compute_entity_v1_self_hash(&artifact).expect("self hash computes");
        assert_eq!(first, second);

        let reordered = reorder_keys(&artifact);
        let third = compute_entity_v1_self_hash(&reordered).expect("reordered hash computes");
        assert_eq!(second, third);

        artifact["summary"]["counts"]["review_groups"] = json!(99);
        let refusal = validate_entity_v1_self_hash(&artifact).expect_err("drift refuses");
        assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
        assert_eq!(refusal.detail["field"], "artifact_content_hash");
    }

    fn assert_stage_fields_present(schema_row: &Value, golden: &Value) {
        let stage_fields = schema_row["required_stage_fields"]
            .as_array()
            .expect("required stage fields");
        for field in stage_fields {
            let field = field.as_str().expect("stage field");
            assert!(
                golden.get(field).is_some(),
                "golden missing required stage field {field}"
            );
        }
    }

    fn fixture_bundle() -> Value {
        Value::Object(Map::from_iter([
            (
                "bundle_version".to_string(),
                Value::String(CANON_ENTITY_ARTIFACT_SCHEMA_BUNDLE_VERSION_V1.to_string()),
            ),
            (
                "artifacts".to_string(),
                Value::Array(
                    ENTITY_ARTIFACT_V1_CONTRACTS
                        .iter()
                        .map(fixture_contract_row)
                        .collect(),
                ),
            ),
        ]))
    }

    fn fixture_artifacts(bundle: &Value) -> &Vec<Value> {
        bundle["artifacts"].as_array().expect("fixture artifacts")
    }

    fn fixture_contract_row(
        contract: &crate::entity::contracts::EntityArtifactContractDescriptor,
    ) -> Value {
        let (stage_field, stage_path) = stage_payload_field(contract.stage);
        json!({
            "stage": contract.stage.as_str(),
            "command": contract.command,
            "artifact_version": contract.artifact_version,
            "schema_key": contract.schema_key,
            "legacy_versions": contract.legacy_versions,
            "workdir": {
                "stage_dir": contract.stage_dir,
                "artifact_relpath": contract.artifact_relpath,
                "payload_relpath": contract.payload_relpath
            },
            "required_stage_fields": [stage_field],
            "golden": sample_artifact(contract.artifact_version, stage_field, stage_path)
        })
    }

    fn sample_artifact(version: &str, stage_field: &str, stage_path: &str) -> Value {
        let contract = entity_artifact_v1_contract_for_version(version).expect("v1 contract");
        let mut golden = json!({
            "version": version,
            "artifact_content_hash": "blake3:placeholder",
            "metadata": {
                "profile": {
                    "id": "cmbs_tenant_label",
                    "version": "0.1.0",
                    "entity_type": "tenant_label",
                    "identity_semantics": "canonical_display_label",
                    "canonical_type": "tenant_label",
                    "patch_namespaces": {
                        "aliases": "cmbs_tenant_label.aliases",
                        "distinct": "cmbs_tenant_label.distinct",
                        "relations": "cmbs_tenant_label.relations"
                    },
                    "content_hash": "blake3:profile"
                },
                "strategy": {
                    "id": "cmbs_tenant_label.v1",
                    "version": "0.1.0",
                    "content_hash": "blake3:strategy"
                },
                "registry_snapshot": {
                    "id": "cmbs-tenants",
                    "version": "2026.06.25",
                    "source": "registries/cmbs-tenants",
                    "lookup_snapshot_hash": "blake3:registry"
                },
                "input": {
                    "row_count": 10143,
                    "content_hash": "blake3:input"
                },
                "patch_namespace": "cmbs_tenant_label.aliases",
                "schema": {
                    "key": contract.schema_key,
                    "content_hash": format!("blake3:schema-{}", contract.stage.as_str())
                },
                "workdir": {
                    "root_dir": "target/entity-work/cmbs-sample",
                    "stage_dir": contract.stage_dir,
                    "artifact_relpath": contract.artifact_relpath,
                    "payload_relpath": contract.payload_relpath
                },
                "upstream_artifacts": sorted_upstreams_for_stage(contract.stage),
                "patch_set": {
                    "content_hash": "blake3:patch",
                    "paths": ["patches/cmbs-tenants.yaml"]
                },
                "namekit": {
                    "version": "namekit.v0",
                    "content_hash": "blake3:namekit"
                },
                "artifact_content_hash": "blake3:placeholder"
            },
            "summary": {
                "counts": {
                    "primary_records": 3,
                    "review_groups": 1
                },
                "labels": {
                    "profile": "cmbs_tenant_label",
                    "stage": contract.stage.as_str()
                }
            },
            stage_field: stage_path
        });
        set_self_hash(&mut golden);
        golden
    }

    fn stage_payload_field(stage: EntityArtifactStageV1) -> (&'static str, &'static str) {
        match stage {
            EntityArtifactStageV1::Project => ("project_rows_path", "project/rows.jsonl"),
            EntityArtifactStageV1::Prepare => ("surfaces_path", "prepare/surfaces.jsonl"),
            EntityArtifactStageV1::Index => ("postings_path", "index/postings.bin"),
            EntityArtifactStageV1::Block => ("candidates_path", "block/candidates.jsonl"),
            EntityArtifactStageV1::Edge => ("evidence_path", "edge/evidence.jsonl"),
            EntityArtifactStageV1::Solve => ("entities_path", "solve/entities.jsonl"),
            EntityArtifactStageV1::Run => ("run_manifest_path", "run/manifest.json"),
            EntityArtifactStageV1::Review => ("review_queue_path", "review/queue.jsonl"),
            EntityArtifactStageV1::Audit => ("audit_report_path", "audit/report.json"),
            EntityArtifactStageV1::Promote => ("promotion_manifest_path", "promote/sidecar.json"),
            EntityArtifactStageV1::Apply => ("output_path", "apply/output.csv"),
            EntityArtifactStageV1::Explain => ("explanation_path", "explain/evidence.json"),
        }
    }

    fn sorted_upstreams_for_stage(stage: EntityArtifactStageV1) -> Value {
        match stage {
            EntityArtifactStageV1::Project => json!([]),
            EntityArtifactStageV1::Prepare => json!([upstream(
                CANON_ENTITY_PROJECT_VERSION_V1,
                "schema-project",
                "project"
            )]),
            EntityArtifactStageV1::Index => json!([upstream(
                CANON_ENTITY_PREPARE_VERSION_V1,
                "schema-prepare",
                "prepare"
            )]),
            EntityArtifactStageV1::Block => json!([upstream(
                CANON_ENTITY_INDEX_VERSION_V1,
                "schema-index",
                "index"
            )]),
            EntityArtifactStageV1::Edge => json!([upstream(
                CANON_ENTITY_BLOCK_VERSION_V1,
                "schema-block",
                "block"
            )]),
            EntityArtifactStageV1::Solve => json!([upstream(
                CANON_ENTITY_EDGE_VERSION_V1,
                "schema-edge",
                "edge"
            )]),
            EntityArtifactStageV1::Run => json!([upstream(
                CANON_ENTITY_SOLVE_VERSION_V1,
                "schema-solve",
                "solve"
            )]),
            EntityArtifactStageV1::Review => {
                json!([upstream(CANON_ENTITY_RUN_VERSION_V1, "schema-run", "run")])
            }
            EntityArtifactStageV1::Audit => {
                json!([upstream(CANON_ENTITY_RUN_VERSION_V1, "schema-run", "run")])
            }
            EntityArtifactStageV1::Promote => json!([upstream(
                CANON_ENTITY_AUDIT_VERSION_V1,
                "schema-audit",
                "audit"
            )]),
            EntityArtifactStageV1::Apply => json!([upstream(
                CANON_ENTITY_PROMOTE_VERSION_V1,
                "schema-promote",
                "promote"
            )]),
            EntityArtifactStageV1::Explain => {
                json!([upstream(CANON_ENTITY_RUN_VERSION_V1, "schema-run", "run")])
            }
        }
    }

    fn upstream(version: &str, schema_hash_suffix: &str, content_hash_suffix: &str) -> Value {
        json!({
            "version": version,
            "schema_key": version,
            "schema_hash": format!("blake3:{schema_hash_suffix}"),
            "content_hash": format!("blake3:{content_hash_suffix}")
        })
    }

    fn set_self_hash(artifact: &mut Value) {
        let hash = compute_entity_v1_self_hash(artifact).expect("self hash computes");
        artifact["artifact_content_hash"] = Value::String(hash.clone());
        artifact["metadata"]["artifact_content_hash"] = Value::String(hash);
    }

    fn reorder_keys(value: &Value) -> Value {
        match value {
            Value::Object(object) => {
                let mut keys = object.keys().cloned().collect::<Vec<_>>();
                keys.sort();
                keys.reverse();
                let mut reordered = Map::new();
                for key in keys {
                    reordered.insert(key.clone(), reorder_keys(&object[&key]));
                }
                Value::Object(reordered)
            }
            Value::Array(array) => Value::Array(array.iter().map(reorder_keys).collect()),
            other => other.clone(),
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
}
