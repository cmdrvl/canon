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

pub const CANON_ENTITY_PROJECT_VERSION_V1: &str = "canon_entity_project.v1";
pub const CANON_ENTITY_PREPARE_VERSION_V1: &str = "canon_entity_prepare.v1";
pub const CANON_ENTITY_INDEX_VERSION_V1: &str = "canon_entity_index.v1";
pub const CANON_ENTITY_BLOCK_VERSION_V1: &str = "canon_entity_block.v1";
pub const CANON_ENTITY_EDGE_VERSION_V1: &str = "canon_entity_edge.v1";
pub const CANON_ENTITY_SOLVE_VERSION_V1: &str = "canon_entity_solve.v1";
pub const CANON_ENTITY_RUN_VERSION_V1: &str = "canon_entity_run.v1";
pub const CANON_ENTITY_REVIEW_VERSION_V1: &str = "canon_entity_review.v1";
pub const CANON_ENTITY_AUDIT_VERSION_V1: &str = "canon_entity_audit.v1";
pub const CANON_ENTITY_PROMOTE_VERSION_V1: &str = "canon_entity_promote.v1";
pub const CANON_ENTITY_APPLY_VERSION_V1: &str = "canon_entity_apply.v1";
pub const CANON_ENTITY_EXPLAIN_VERSION_V1: &str = "canon_entity_explain.v1";

pub const ENTITY_ARTIFACT_V1_VERSIONS: &[&str] = &[
    CANON_ENTITY_PROJECT_VERSION_V1,
    CANON_ENTITY_PREPARE_VERSION_V1,
    CANON_ENTITY_INDEX_VERSION_V1,
    CANON_ENTITY_BLOCK_VERSION_V1,
    CANON_ENTITY_EDGE_VERSION_V1,
    CANON_ENTITY_SOLVE_VERSION_V1,
    CANON_ENTITY_RUN_VERSION_V1,
    CANON_ENTITY_REVIEW_VERSION_V1,
    CANON_ENTITY_AUDIT_VERSION_V1,
    CANON_ENTITY_PROMOTE_VERSION_V1,
    CANON_ENTITY_APPLY_VERSION_V1,
    CANON_ENTITY_EXPLAIN_VERSION_V1,
];

pub const LEGACY_ENTITY_PROJECT_VERSIONS: &[&str] = &[
    CANON_ENTITY_PROJECTION_VERSION,
    "canon_entity_surface_row.v0",
];
pub const LEGACY_ENTITY_PREPARE_VERSIONS: &[&str] = &[CANON_ENTITY_PREPARE_VERSION];
pub const LEGACY_ENTITY_INDEX_VERSIONS: &[&str] = &[CANON_ENTITY_INDEX_VERSION];
pub const LEGACY_ENTITY_BLOCK_VERSIONS: &[&str] = &[
    CANON_ENTITY_BLOCK_VERSION,
    CANON_ENTITY_BLOCK_BUCKET_VERSION,
];
pub const LEGACY_ENTITY_EDGE_VERSIONS: &[&str] = &[CANON_ENTITY_EDGE_VERSION];
pub const LEGACY_ENTITY_SOLVE_VERSIONS: &[&str] = &[CANON_ENTITY_SOLVE_VERSION];
pub const LEGACY_ENTITY_RUN_VERSIONS: &[&str] = &[CANON_ENTITY_RUN_VERSION];
pub const LEGACY_ENTITY_REVIEW_VERSIONS: &[&str] = &[
    "canon_entity_review_export.v0",
    "canon_entity_review_import.v0",
    CANON_ENTITY_DECISION_LEDGER_VERSION,
];
pub const LEGACY_ENTITY_AUDIT_VERSIONS: &[&str] = &[CANON_ENTITY_AUDIT_VERSION];
pub const LEGACY_ENTITY_PROMOTE_VERSIONS: &[&str] = &[
    CANON_ENTITY_PROMOTE_VERSION,
    "canon_entity_promotion_sidecar.v0",
    "canon_entity_promotion_proof.v0",
];
pub const LEGACY_ENTITY_APPLY_VERSIONS: &[&str] = &[CANON_ENTITY_APPLY_VERSION];
pub const LEGACY_ENTITY_EXPLAIN_VERSIONS: &[&str] = &[CANON_ENTITY_EXPLAIN_VERSION];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityArtifactStageV1 {
    Project,
    Prepare,
    Index,
    Block,
    Edge,
    Solve,
    Run,
    Review,
    Audit,
    Promote,
    Apply,
    Explain,
}

impl EntityArtifactStageV1 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Prepare => "prepare",
            Self::Index => "index",
            Self::Block => "block",
            Self::Edge => "edge",
            Self::Solve => "solve",
            Self::Run => "run",
            Self::Review => "review",
            Self::Audit => "audit",
            Self::Promote => "promote",
            Self::Apply => "apply",
            Self::Explain => "explain",
        }
    }

    pub const fn command(self) -> &'static str {
        match self {
            Self::Project => "canon entity project",
            Self::Prepare => "canon entity prepare",
            Self::Index => "canon entity index build",
            Self::Block => "canon entity block",
            Self::Edge => "canon entity edge",
            Self::Solve => "canon entity solve",
            Self::Run => "canon entity run",
            Self::Review => "canon entity review",
            Self::Audit => "canon entity audit",
            Self::Promote => "canon entity promote",
            Self::Apply => "canon entity apply",
            Self::Explain => "canon entity explain",
        }
    }

    pub const fn stage_dir(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Prepare => "prepare",
            Self::Index => "index",
            Self::Block => "block",
            Self::Edge => "edge",
            Self::Solve => "solve",
            Self::Run => "run",
            Self::Review => "review",
            Self::Audit => "audit",
            Self::Promote => "promote",
            Self::Apply => "apply",
            Self::Explain => "explain",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityArtifactPayloadKind {
    Json,
    Jsonl,
    Csv,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EntityArtifactContractDescriptor {
    pub stage: EntityArtifactStageV1,
    pub command: &'static str,
    pub artifact_version: &'static str,
    pub schema_key: &'static str,
    pub stage_dir: &'static str,
    pub artifact_relpath: &'static str,
    pub payload_relpath: &'static str,
    pub payload_kind: EntityArtifactPayloadKind,
    pub legacy_versions: &'static [&'static str],
}

pub const ENTITY_ARTIFACT_V1_CONTRACTS: &[EntityArtifactContractDescriptor] = &[
    EntityArtifactContractDescriptor {
        stage: EntityArtifactStageV1::Project,
        command: EntityArtifactStageV1::Project.command(),
        artifact_version: CANON_ENTITY_PROJECT_VERSION_V1,
        schema_key: CANON_ENTITY_PROJECT_VERSION_V1,
        stage_dir: EntityArtifactStageV1::Project.stage_dir(),
        artifact_relpath: "project/project.json",
        payload_relpath: "project/rows.jsonl",
        payload_kind: EntityArtifactPayloadKind::Jsonl,
        legacy_versions: LEGACY_ENTITY_PROJECT_VERSIONS,
    },
    EntityArtifactContractDescriptor {
        stage: EntityArtifactStageV1::Prepare,
        command: EntityArtifactStageV1::Prepare.command(),
        artifact_version: CANON_ENTITY_PREPARE_VERSION_V1,
        schema_key: CANON_ENTITY_PREPARE_VERSION_V1,
        stage_dir: EntityArtifactStageV1::Prepare.stage_dir(),
        artifact_relpath: "prepare/prepare.json",
        payload_relpath: "prepare/surfaces.jsonl",
        payload_kind: EntityArtifactPayloadKind::Jsonl,
        legacy_versions: LEGACY_ENTITY_PREPARE_VERSIONS,
    },
    EntityArtifactContractDescriptor {
        stage: EntityArtifactStageV1::Index,
        command: EntityArtifactStageV1::Index.command(),
        artifact_version: CANON_ENTITY_INDEX_VERSION_V1,
        schema_key: CANON_ENTITY_INDEX_VERSION_V1,
        stage_dir: EntityArtifactStageV1::Index.stage_dir(),
        artifact_relpath: "index/index.json",
        payload_relpath: "index/postings.bin",
        payload_kind: EntityArtifactPayloadKind::Binary,
        legacy_versions: LEGACY_ENTITY_INDEX_VERSIONS,
    },
    EntityArtifactContractDescriptor {
        stage: EntityArtifactStageV1::Block,
        command: EntityArtifactStageV1::Block.command(),
        artifact_version: CANON_ENTITY_BLOCK_VERSION_V1,
        schema_key: CANON_ENTITY_BLOCK_VERSION_V1,
        stage_dir: EntityArtifactStageV1::Block.stage_dir(),
        artifact_relpath: "block/block.json",
        payload_relpath: "block/candidates.jsonl",
        payload_kind: EntityArtifactPayloadKind::Jsonl,
        legacy_versions: LEGACY_ENTITY_BLOCK_VERSIONS,
    },
    EntityArtifactContractDescriptor {
        stage: EntityArtifactStageV1::Edge,
        command: EntityArtifactStageV1::Edge.command(),
        artifact_version: CANON_ENTITY_EDGE_VERSION_V1,
        schema_key: CANON_ENTITY_EDGE_VERSION_V1,
        stage_dir: EntityArtifactStageV1::Edge.stage_dir(),
        artifact_relpath: "edge/edge.json",
        payload_relpath: "edge/evidence.jsonl",
        payload_kind: EntityArtifactPayloadKind::Jsonl,
        legacy_versions: LEGACY_ENTITY_EDGE_VERSIONS,
    },
    EntityArtifactContractDescriptor {
        stage: EntityArtifactStageV1::Solve,
        command: EntityArtifactStageV1::Solve.command(),
        artifact_version: CANON_ENTITY_SOLVE_VERSION_V1,
        schema_key: CANON_ENTITY_SOLVE_VERSION_V1,
        stage_dir: EntityArtifactStageV1::Solve.stage_dir(),
        artifact_relpath: "solve/solve.json",
        payload_relpath: "solve/entities.jsonl",
        payload_kind: EntityArtifactPayloadKind::Jsonl,
        legacy_versions: LEGACY_ENTITY_SOLVE_VERSIONS,
    },
    EntityArtifactContractDescriptor {
        stage: EntityArtifactStageV1::Run,
        command: EntityArtifactStageV1::Run.command(),
        artifact_version: CANON_ENTITY_RUN_VERSION_V1,
        schema_key: CANON_ENTITY_RUN_VERSION_V1,
        stage_dir: EntityArtifactStageV1::Run.stage_dir(),
        artifact_relpath: "run/run.json",
        payload_relpath: "run/manifest.json",
        payload_kind: EntityArtifactPayloadKind::Json,
        legacy_versions: LEGACY_ENTITY_RUN_VERSIONS,
    },
    EntityArtifactContractDescriptor {
        stage: EntityArtifactStageV1::Review,
        command: EntityArtifactStageV1::Review.command(),
        artifact_version: CANON_ENTITY_REVIEW_VERSION_V1,
        schema_key: CANON_ENTITY_REVIEW_VERSION_V1,
        stage_dir: EntityArtifactStageV1::Review.stage_dir(),
        artifact_relpath: "review/review.json",
        payload_relpath: "review/queue.jsonl",
        payload_kind: EntityArtifactPayloadKind::Jsonl,
        legacy_versions: LEGACY_ENTITY_REVIEW_VERSIONS,
    },
    EntityArtifactContractDescriptor {
        stage: EntityArtifactStageV1::Audit,
        command: EntityArtifactStageV1::Audit.command(),
        artifact_version: CANON_ENTITY_AUDIT_VERSION_V1,
        schema_key: CANON_ENTITY_AUDIT_VERSION_V1,
        stage_dir: EntityArtifactStageV1::Audit.stage_dir(),
        artifact_relpath: "audit/audit.json",
        payload_relpath: "audit/report.json",
        payload_kind: EntityArtifactPayloadKind::Json,
        legacy_versions: LEGACY_ENTITY_AUDIT_VERSIONS,
    },
    EntityArtifactContractDescriptor {
        stage: EntityArtifactStageV1::Promote,
        command: EntityArtifactStageV1::Promote.command(),
        artifact_version: CANON_ENTITY_PROMOTE_VERSION_V1,
        schema_key: CANON_ENTITY_PROMOTE_VERSION_V1,
        stage_dir: EntityArtifactStageV1::Promote.stage_dir(),
        artifact_relpath: "promote/promote.json",
        payload_relpath: "promote/sidecar.json",
        payload_kind: EntityArtifactPayloadKind::Json,
        legacy_versions: LEGACY_ENTITY_PROMOTE_VERSIONS,
    },
    EntityArtifactContractDescriptor {
        stage: EntityArtifactStageV1::Apply,
        command: EntityArtifactStageV1::Apply.command(),
        artifact_version: CANON_ENTITY_APPLY_VERSION_V1,
        schema_key: CANON_ENTITY_APPLY_VERSION_V1,
        stage_dir: EntityArtifactStageV1::Apply.stage_dir(),
        artifact_relpath: "apply/apply.json",
        payload_relpath: "apply/output.csv",
        payload_kind: EntityArtifactPayloadKind::Csv,
        legacy_versions: LEGACY_ENTITY_APPLY_VERSIONS,
    },
    EntityArtifactContractDescriptor {
        stage: EntityArtifactStageV1::Explain,
        command: EntityArtifactStageV1::Explain.command(),
        artifact_version: CANON_ENTITY_EXPLAIN_VERSION_V1,
        schema_key: CANON_ENTITY_EXPLAIN_VERSION_V1,
        stage_dir: EntityArtifactStageV1::Explain.stage_dir(),
        artifact_relpath: "explain/explain.json",
        payload_relpath: "explain/evidence.json",
        payload_kind: EntityArtifactPayloadKind::Json,
        legacy_versions: LEGACY_ENTITY_EXPLAIN_VERSIONS,
    },
];

pub fn entity_artifact_v1_contract_for_version(
    version: &str,
) -> Option<&'static EntityArtifactContractDescriptor> {
    ENTITY_ARTIFACT_V1_CONTRACTS
        .iter()
        .find(|contract| contract.artifact_version == version)
}

pub fn entity_artifact_v1_contract_for_legacy_version(
    version: &str,
) -> Option<&'static EntityArtifactContractDescriptor> {
    ENTITY_ARTIFACT_V1_CONTRACTS
        .iter()
        .find(|contract| contract.legacy_versions.contains(&version))
}

pub fn is_blake3_hash(value: &str) -> bool {
    value.starts_with("blake3:") && value.len() > "blake3:".len()
}

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

pub fn entity_profile_contract_schema_version() -> &'static str {
    concat!("canon.entity.profile", ".v1")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntityContractKind {
    EntityProfile,
    LinkageMap,
    EvidencePolicy,
    ReviewPolicy,
    PromotionPolicy,
    FrozenExecutableStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntityContractDescendant {
    ClusterLineage,
    LinkageLineage,
    ReviewLineage,
    PromotionLineage,
}

impl EntityContractKind {
    pub const fn invalidated_descendants(self) -> &'static [EntityContractDescendant] {
        use EntityContractDescendant::{
            ClusterLineage, LinkageLineage, PromotionLineage, ReviewLineage,
        };

        match self {
            Self::EntityProfile => &[ClusterLineage, ReviewLineage, PromotionLineage],
            Self::LinkageMap => &[LinkageLineage],
            Self::EvidencePolicy => &[
                ClusterLineage,
                LinkageLineage,
                ReviewLineage,
                PromotionLineage,
            ],
            Self::ReviewPolicy => &[ReviewLineage, PromotionLineage],
            Self::PromotionPolicy => &[PromotionLineage],
            Self::FrozenExecutableStrategy => &[
                ClusterLineage,
                LinkageLineage,
                ReviewLineage,
                PromotionLineage,
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityTypedReference {
    pub kind: Option<EntityContractKind>,
    pub id: String,
    pub version: String,
    pub content_hash: String,
}

impl EntityTypedReference {
    pub fn is_complete_as(&self, expected: EntityContractKind) -> bool {
        self.kind == Some(expected)
            && !self.id.trim().is_empty()
            && !self.version.trim().is_empty()
            && is_blake3_hash(&self.content_hash)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityTypedContractErrorCode {
    WrongKind,
    IncompleteReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityTypedContractError {
    pub code: EntityTypedContractErrorCode,
    pub field: String,
    pub expected_kind: EntityContractKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_kind: Option<EntityContractKind>,
}

impl EntityTypedContractError {
    fn wrong_kind(
        field: impl Into<String>,
        expected_kind: EntityContractKind,
        actual_kind: Option<EntityContractKind>,
    ) -> Self {
        Self {
            code: EntityTypedContractErrorCode::WrongKind,
            field: field.into(),
            expected_kind,
            actual_kind,
        }
    }

    fn incomplete(field: impl Into<String>, expected_kind: EntityContractKind) -> Self {
        Self {
            code: EntityTypedContractErrorCode::IncompleteReference,
            field: field.into(),
            expected_kind,
            actual_kind: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityClusterContractSlice {
    pub profile: EntityTypedReference,
    pub evidence_policy: EntityTypedReference,
    pub frozen_executable_strategy: EntityTypedReference,
}

impl EntityClusterContractSlice {
    pub fn validate(&self) -> Result<(), EntityTypedContractError> {
        validate_typed_reference("profile", &self.profile, EntityContractKind::EntityProfile)?;
        validate_typed_reference(
            "evidence_policy",
            &self.evidence_policy,
            EntityContractKind::EvidencePolicy,
        )?;
        validate_typed_reference(
            "frozen_executable_strategy",
            &self.frozen_executable_strategy,
            EntityContractKind::FrozenExecutableStrategy,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityLinkageContractSlice {
    pub linkage_map: EntityTypedReference,
    pub evidence_policy: EntityTypedReference,
    pub frozen_executable_strategy: EntityTypedReference,
}

impl EntityLinkageContractSlice {
    pub fn validate(&self) -> Result<(), EntityTypedContractError> {
        validate_typed_reference(
            "linkage_map",
            &self.linkage_map,
            EntityContractKind::LinkageMap,
        )?;
        validate_typed_reference(
            "evidence_policy",
            &self.evidence_policy,
            EntityContractKind::EvidencePolicy,
        )?;
        validate_typed_reference(
            "frozen_executable_strategy",
            &self.frozen_executable_strategy,
            EntityContractKind::FrozenExecutableStrategy,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityGovernanceContractSlice {
    pub review_policy: EntityTypedReference,
    pub promotion_policy: EntityTypedReference,
}

impl EntityGovernanceContractSlice {
    pub fn validate(&self) -> Result<(), EntityTypedContractError> {
        validate_typed_reference(
            "review_policy",
            &self.review_policy,
            EntityContractKind::ReviewPolicy,
        )?;
        validate_typed_reference(
            "promotion_policy",
            &self.promotion_policy,
            EntityContractKind::PromotionPolicy,
        )?;
        Ok(())
    }
}

fn validate_typed_reference(
    field: &str,
    reference: &EntityTypedReference,
    expected_kind: EntityContractKind,
) -> Result<(), EntityTypedContractError> {
    if reference.kind != Some(expected_kind) {
        return Err(EntityTypedContractError::wrong_kind(
            field,
            expected_kind,
            reference.kind,
        ));
    }
    if !reference.is_complete_as(expected_kind) {
        return Err(EntityTypedContractError::incomplete(field, expected_kind));
    }
    Ok(())
}

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
pub struct EntityArtifactSchemaReferenceV1 {
    pub key: String,
    pub content_hash: String,
}

impl EntityArtifactSchemaReferenceV1 {
    pub fn is_complete(&self) -> bool {
        !self.key.trim().is_empty() && is_blake3_hash(&self.content_hash)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityArtifactWorkdirLayoutV1 {
    pub root_dir: String,
    pub stage_dir: String,
    pub artifact_relpath: String,
    pub payload_relpath: String,
}

impl EntityArtifactWorkdirLayoutV1 {
    pub fn is_complete(&self) -> bool {
        let stage_prefix = format!("{}/", self.stage_dir);
        !self.root_dir.trim().is_empty()
            && !self.stage_dir.trim().is_empty()
            && !self.artifact_relpath.trim().is_empty()
            && !self.payload_relpath.trim().is_empty()
            && !self.artifact_relpath.contains("..")
            && !self.payload_relpath.contains("..")
            && self.artifact_relpath.starts_with(&stage_prefix)
            && self.payload_relpath.starts_with(&stage_prefix)
    }

    pub fn render_identity(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.root_dir, self.stage_dir, self.artifact_relpath, self.payload_relpath
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityArtifactReferenceV1 {
    pub version: String,
    pub schema_key: String,
    pub schema_hash: String,
    pub content_hash: String,
}

impl EntityArtifactReferenceV1 {
    pub fn is_complete(&self) -> bool {
        !self.version.trim().is_empty()
            && !self.schema_key.trim().is_empty()
            && is_blake3_hash(&self.schema_hash)
            && is_blake3_hash(&self.content_hash)
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityArtifactMetadataV1 {
    pub profile: EntityProfileReference,
    pub strategy: EntityStrategyReference,
    pub registry_snapshot: EntityRegistrySnapshot,
    pub input: EntityInputReference,
    pub patch_namespace: String,
    pub schema: EntityArtifactSchemaReferenceV1,
    pub workdir: EntityArtifactWorkdirLayoutV1,
    #[serde(default)]
    pub upstream_artifacts: Vec<EntityArtifactReferenceV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_set: Option<EntityPatchSetReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namekit: Option<EntityNamekitReference>,
    pub artifact_content_hash: String,
}

impl EntityArtifactMetadataV1 {
    pub fn has_profile_firewall(&self) -> bool {
        let namespaces = &self.profile.patch_namespaces;
        namespaces.matches_profile_root(&self.profile.id)
            && (self.patch_namespace == namespaces.aliases
                || self.patch_namespace == namespaces.distinct
                || self.patch_namespace == namespaces.relations)
    }

    pub fn is_complete(&self) -> bool {
        self.profile.is_complete()
            && self
                .profile
                .content_hash
                .as_deref()
                .is_some_and(is_blake3_hash)
            && !self.strategy.id.trim().is_empty()
            && !self.strategy.version.trim().is_empty()
            && is_blake3_hash(&self.strategy.content_hash)
            && !self.registry_snapshot.id.trim().is_empty()
            && !self.registry_snapshot.version.trim().is_empty()
            && !self.registry_snapshot.source.trim().is_empty()
            && is_blake3_hash(&self.registry_snapshot.lookup_snapshot_hash)
            && is_blake3_hash(&self.input.content_hash)
            && !self.patch_namespace.trim().is_empty()
            && self.schema.is_complete()
            && self.workdir.is_complete()
            && self
                .upstream_artifacts
                .iter()
                .all(EntityArtifactReferenceV1::is_complete)
            && self
                .patch_set
                .as_ref()
                .is_none_or(|patch_set| is_blake3_hash(&patch_set.content_hash))
            && self
                .namekit
                .as_ref()
                .is_none_or(|namekit| is_blake3_hash(&namekit.content_hash))
            && is_blake3_hash(&self.artifact_content_hash)
            && self.has_profile_firewall()
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityArtifactHeaderV1 {
    pub version: String,
    pub artifact_content_hash: String,
    pub metadata: EntityArtifactMetadataV1,
    pub summary: EntityDeterministicSummary,
}

impl EntityArtifactHeaderV1 {
    pub fn is_complete(&self) -> bool {
        !self.version.trim().is_empty()
            && is_blake3_hash(&self.artifact_content_hash)
            && self.metadata.is_complete()
            && self.artifact_content_hash == self.metadata.artifact_content_hash
            && !self.summary.counts.is_empty()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::BTreeMap;

    #[test]
    fn entity_v1_contract_catalog_is_stable_and_complete() {
        assert_eq!(ENTITY_ARTIFACT_V1_CONTRACTS.len(), 12);
        assert_eq!(
            ENTITY_ARTIFACT_V1_VERSIONS,
            [
                CANON_ENTITY_PROJECT_VERSION_V1,
                CANON_ENTITY_PREPARE_VERSION_V1,
                CANON_ENTITY_INDEX_VERSION_V1,
                CANON_ENTITY_BLOCK_VERSION_V1,
                CANON_ENTITY_EDGE_VERSION_V1,
                CANON_ENTITY_SOLVE_VERSION_V1,
                CANON_ENTITY_RUN_VERSION_V1,
                CANON_ENTITY_REVIEW_VERSION_V1,
                CANON_ENTITY_AUDIT_VERSION_V1,
                CANON_ENTITY_PROMOTE_VERSION_V1,
                CANON_ENTITY_APPLY_VERSION_V1,
                CANON_ENTITY_EXPLAIN_VERSION_V1,
            ]
        );
        for contract in ENTITY_ARTIFACT_V1_CONTRACTS {
            assert_eq!(contract.schema_key, contract.artifact_version);
            assert!(contract.artifact_relpath.starts_with(contract.stage_dir));
            assert!(contract.payload_relpath.starts_with(contract.stage_dir));
            assert!(!contract.legacy_versions.is_empty());
        }
        assert_eq!(
            entity_artifact_v1_contract_for_legacy_version("canon_entity_run.v0")
                .expect("legacy run contract")
                .artifact_version,
            CANON_ENTITY_RUN_VERSION_V1
        );
    }

    #[test]
    fn entity_v1_header_round_trips_with_profile_firewall_and_workdir_layout() {
        let header = sample_v1_header();
        assert!(header.is_complete());

        let value = serde_json::to_value(&header).expect("header serializes");
        assert_eq!(value["version"], CANON_ENTITY_PREPARE_VERSION_V1);
        assert_eq!(
            value["metadata"]["schema"]["key"],
            CANON_ENTITY_PREPARE_VERSION_V1
        );
        assert_eq!(
            value["metadata"]["workdir"]["artifact_relpath"],
            "prepare/prepare.json"
        );
        assert_eq!(
            value["metadata"]["patch_namespace"],
            "cmbs_tenant_label.aliases"
        );

        let round_tripped: EntityArtifactHeaderV1 =
            serde_json::from_value(value).expect("header deserializes");
        assert_eq!(round_tripped, header);
    }

    #[test]
    fn entity_v1_workdir_and_upstream_refs_require_complete_hash_contracts() {
        let header = sample_v1_header();
        assert!(header.metadata.workdir.is_complete());
        assert!(header.metadata.upstream_artifacts[0].is_complete());

        let rendered = header.metadata.workdir.render_identity();
        assert_eq!(
            rendered,
            "target/entity-work/cmbs-sample|prepare|prepare/prepare.json|prepare/surfaces.jsonl"
        );
    }

    fn sample_v1_header() -> EntityArtifactHeaderV1 {
        EntityArtifactHeaderV1 {
            version: CANON_ENTITY_PREPARE_VERSION_V1.to_string(),
            artifact_content_hash: "blake3:prepare-v1".to_string(),
            metadata: EntityArtifactMetadataV1 {
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
                input: EntityInputReference {
                    row_count: 10_143,
                    content_hash: "blake3:input".to_string(),
                },
                patch_namespace: "cmbs_tenant_label.aliases".to_string(),
                schema: EntityArtifactSchemaReferenceV1 {
                    key: CANON_ENTITY_PREPARE_VERSION_V1.to_string(),
                    content_hash: "blake3:schema-prepare".to_string(),
                },
                workdir: EntityArtifactWorkdirLayoutV1 {
                    root_dir: "target/entity-work/cmbs-sample".to_string(),
                    stage_dir: "prepare".to_string(),
                    artifact_relpath: "prepare/prepare.json".to_string(),
                    payload_relpath: "prepare/surfaces.jsonl".to_string(),
                },
                upstream_artifacts: vec![EntityArtifactReferenceV1 {
                    version: CANON_ENTITY_PROJECT_VERSION_V1.to_string(),
                    schema_key: CANON_ENTITY_PROJECT_VERSION_V1.to_string(),
                    schema_hash: "blake3:schema-project".to_string(),
                    content_hash: "blake3:project-v1".to_string(),
                }],
                patch_set: Some(EntityPatchSetReference {
                    content_hash: "blake3:patch".to_string(),
                    paths: vec!["patches/cmbs-tenants.yaml".to_string()],
                }),
                namekit: Some(EntityNamekitReference {
                    version: "namekit.v0".to_string(),
                    content_hash: "blake3:namekit".to_string(),
                }),
                artifact_content_hash: "blake3:prepare-v1".to_string(),
            },
            summary: EntityDeterministicSummary {
                counts: BTreeMap::from([
                    ("prepared_surfaces".to_string(), 431),
                    ("raw_unique_surfaces".to_string(), 614),
                ]),
                labels: BTreeMap::from([("profile".to_string(), "cmbs_tenant_label".to_string())]),
            },
        }
    }

    #[test]
    fn entity_v1_header_serialized_shape_stays_timestamp_free() {
        let serialized = serde_json::to_string(&sample_v1_header()).expect("header serializes");
        for forbidden in ["timestamp", "created_at", "updated_at", "wall_clock"] {
            assert!(
                !serialized.contains(forbidden),
                "header contains nondeterministic field {forbidden}"
            );
        }
    }

    #[test]
    fn entity_v1_header_json_shape_is_object() {
        let value = serde_json::to_value(sample_v1_header()).expect("header serializes");
        assert!(matches!(value, Value::Object(_)));
    }
}
