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
    CANON_ENTITY_SOLVE_VERSION,
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

fn missing_field(path: &str) -> Refusal {
    artifact_contract_refusal(
        "Entity artifact is missing a required schema field",
        json!({ "missing": path }),
    )
}

fn artifact_contract_refusal(message: impl Into<String>, detail: Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(message, detail, None)
}
