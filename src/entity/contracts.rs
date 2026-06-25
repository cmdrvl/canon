//! Shared `canon entity` contract surface.
//!
//! This module is intentionally data-only. Downstream workbench stages import
//! these constants and metadata structs so persisted artifacts agree on
//! profile semantics, registry snapshots, hashes, and stable contract IDs
//! before stage-specific implementation begins.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CANON_ENTITY_PROJECTION_VERSION: &str = "canon_entity_projection.v0";
pub const CANON_ENTITY_PREPARE_VERSION: &str = "canon_entity_prepare.v0";
pub const CANON_ENTITY_INDEX_VERSION: &str = "canon_entity_index.v0";
pub const CANON_ENTITY_BLOCK_VERSION: &str = "canon_entity_block.v0";
pub const CANON_ENTITY_BLOCK_BUCKET_VERSION: &str = "canon_entity_block_bucket.v0";
pub const CANON_ENTITY_EDGE_VERSION: &str = "canon_entity_edge.v0";
pub const CANON_ENTITY_SOLVE_VERSION: &str = "canon_entity_solve.v0";
pub const CANON_ENTITY_RUN_VERSION: &str = "canon_entity_run.v0";
pub const CANON_ENTITY_DECISION_LEDGER_VERSION: &str = "canon_entity_decision_ledger.v0";
pub const CANON_ENTITY_AUDIT_VERSION: &str = "canon_entity_audit.v0";
pub const CANON_ENTITY_PROMOTE_VERSION: &str = "canon_entity_promote.v0";
pub const CANON_ENTITY_EXPLAIN_VERSION: &str = "canon_entity_explain.v0";
pub const CANON_ENTITY_APPLY_VERSION: &str = "canon_entity_apply.v0";

pub const ENTITY_ARTIFACT_VERSIONS: &[&str] = &[
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
];

pub const ENTITY_INVARIANT_IDS: &[&str] = &[
    "I01", "I02", "I03", "I04", "I05", "I06", "I07", "I08", "I09", "I10", "I11", "I12", "I13",
    "I14", "I15", "I16", "I17", "I18", "I19", "I20", "I21", "I22", "I23", "I24", "I25",
];

pub const ENTITY_GATE_IDS: &[&str] = &[
    "G01", "G02", "G03", "G04", "G05", "G06", "G07", "G08", "G09", "G10", "G11", "G12", "G13",
    "G14", "G15",
];

pub const ENTITY_REFUSAL_CODES: &[&str] = &[
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
];

/// Profile identity metadata required by invariant I10.
///
/// Profiles define entity semantics; this prevents a tenant display-label run
/// from being reused as legal-entity or firm-identity evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityProfileReference {
    pub id: String,
    pub version: String,
    pub entity_type: String,
    pub identity_semantics: String,
    pub canonical_type: String,
    #[serde(default)]
    pub patch_namespaces: EntityPatchNamespaces,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

impl EntityProfileReference {
    pub fn is_complete(&self) -> bool {
        !self.id.is_empty()
            && !self.version.is_empty()
            && !self.entity_type.is_empty()
            && !self.identity_semantics.is_empty()
            && !self.canonical_type.is_empty()
            && self.patch_namespaces.is_complete()
    }
}

/// Profile-scoped patch namespaces carried by every persisted artifact.
///
/// The workbench keeps aliases, distinct facts, and relation hints separate,
/// but all three namespaces must share the same profile root so cross-profile
/// patches cannot be consumed as same-profile merge evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityPatchNamespaces {
    #[serde(default)]
    pub aliases: String,
    #[serde(default)]
    pub distinct: String,
    #[serde(default)]
    pub relations: String,
}

impl EntityPatchNamespaces {
    pub fn is_complete(&self) -> bool {
        !self.aliases.trim().is_empty()
            && !self.distinct.trim().is_empty()
            && !self.relations.trim().is_empty()
    }

    pub fn matches_profile_root(&self, profile_id: &str) -> bool {
        if profile_id.trim().is_empty() {
            return false;
        }
        let expected_prefix = format!("{profile_id}.");
        self.aliases.starts_with(&expected_prefix)
            && self.distinct.starts_with(&expected_prefix)
            && self.relations.starts_with(&expected_prefix)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityStrategyReference {
    pub id: String,
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityRegistrySnapshot {
    pub id: String,
    pub version: String,
    pub source: String,
    pub lookup_snapshot_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidecar_snapshot_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityInputReference {
    pub row_count: u64,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityArtifactReference {
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityPatchSetReference {
    pub content_hash: String,
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityNamekitReference {
    pub version: String,
    pub content_hash: String,
}

/// Mandatory metadata for persisted entity artifacts.
///
/// Invariant I03 requires deterministic local runs. I04 requires every
/// workbench artifact to record profile, strategy, registry, input, patch
/// namespace, and artifact hashes. Optional patch/namekit hashes are included
/// because I21 makes them part of cache-hit identity when those inputs exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityArtifactMetadata {
    pub profile: EntityProfileReference,
    pub strategy: EntityStrategyReference,
    pub registry_snapshot: EntityRegistrySnapshot,
    pub patch_namespace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<EntityInputReference>,
    #[serde(default)]
    pub upstream_artifacts: Vec<EntityArtifactReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_set: Option<EntityPatchSetReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namekit: Option<EntityNamekitReference>,
    pub artifact_content_hash: String,
}

/// Deterministic summary container for cross-stage count and label fields.
///
/// BTreeMap ordering is part of the contract: summaries can be serialized
/// byte-stably after callers also use deterministic JSON formatting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityDeterministicSummary {
    #[serde(default)]
    pub counts: BTreeMap<String, u64>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityArtifactHeader {
    pub version: String,
    pub metadata: EntityArtifactMetadata,
    pub summary: EntityDeterministicSummary,
}

/// Cache identity material named by invariant I21.
///
/// A cache hit is valid only when all populated hashes match the current run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityCacheKeyMaterial {
    pub input_hash: String,
    pub profile_hash: String,
    pub strategy_hash: String,
    pub registry_snapshot_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_hash: Option<String>,
    pub namekit_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namekit_hash: Option<String>,
}
