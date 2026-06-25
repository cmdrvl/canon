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
        artifact_version: CANON_ENTITY_PROJECTION_VERSION,
        schema_key: "canon_entity_projection.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_PREPARE_VERSION,
        schema_key: "canon_entity_prepare.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_SURFACE_ROW_VERSION,
        schema_key: "canon_entity_surface_row.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_INDEX_VERSION,
        schema_key: "canon_entity_index.v0",
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
        artifact_version: CANON_ENTITY_EDGE_VERSION,
        schema_key: "canon_entity_edge.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_SOLVE_VERSION,
        schema_key: "canon_entity_solve.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_RUN_VERSION,
        schema_key: "canon_entity_run.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_REVIEW_QUEUE_VERSION,
        schema_key: "canon_entity_review_queue.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_REVIEW_IMPORT_VERSION,
        schema_key: "canon_entity_review_import.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_DECISION_LEDGER_VERSION,
        schema_key: "canon_entity_decision_ledger.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_AUDIT_VERSION,
        schema_key: "canon_entity_audit.v0",
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
        artifact_version: CANON_ENTITY_EXPLAIN_VERSION,
        schema_key: "canon_entity_explain.v0",
    },
    EntitySchemaSnapshot {
        artifact_version: CANON_ENTITY_APPLY_VERSION,
        schema_key: "canon_entity_apply.v0",
    },
];

pub fn schema_snapshot_for_version(version: &str) -> Option<&'static EntitySchemaSnapshot> {
    ENTITY_SCHEMA_SNAPSHOTS
        .iter()
        .find(|snapshot| snapshot.artifact_version == version)
}
