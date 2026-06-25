#![forbid(unsafe_code)]

//! Entity artifact chain-of-custody contracts.
//!
//! This module is data-only preflight logic for persisted workbench artifacts.
//! It names the hash fields that make an artifact reusable, the downstream
//! stages invalidated by each field, and the refusal kind used when a command
//! must stop instead of silently consuming stale evidence.

use crate::{
    Refusal,
    entity::{
        contracts::{
            EntityArtifactHeader, EntityArtifactReference, EntityCacheKeyMaterial,
            EntityProfileReference,
        },
        error::EntityRefusalKind,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const ENTITY_ARTIFACT_CHAIN_CONTRACT_VERSION: &str = "canon_entity_artifact_chain.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityChainStage {
    Projection,
    Prepare,
    Index,
    Block,
    Edge,
    Solve,
    ReviewExport,
    ReviewImport,
    DecisionLedger,
    Audit,
    Promote,
    Apply,
}

impl EntityChainStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Projection => "projection",
            Self::Prepare => "prepare",
            Self::Index => "index",
            Self::Block => "block",
            Self::Edge => "edge",
            Self::Solve => "solve",
            Self::ReviewExport => "review_export",
            Self::ReviewImport => "review_import",
            Self::DecisionLedger => "decision_ledger",
            Self::Audit => "audit",
            Self::Promote => "promote",
            Self::Apply => "apply",
        }
    }

    pub const fn command_name(self) -> &'static str {
        match self {
            Self::Projection => "project",
            Self::Prepare => "prepare",
            Self::Index => "index build",
            Self::Block => "block",
            Self::Edge => "edge",
            Self::Solve => "solve",
            Self::ReviewExport => "review export",
            Self::ReviewImport => "review import",
            Self::DecisionLedger => "review import",
            Self::Audit => "audit",
            Self::Promote => "promote",
            Self::Apply => "apply",
        }
    }
}

pub const INPUT_HASH_INVALIDATES: &[EntityChainStage] = &[
    EntityChainStage::Prepare,
    EntityChainStage::Index,
    EntityChainStage::Block,
    EntityChainStage::Edge,
    EntityChainStage::Solve,
    EntityChainStage::ReviewExport,
    EntityChainStage::ReviewImport,
    EntityChainStage::DecisionLedger,
    EntityChainStage::Audit,
    EntityChainStage::Promote,
    EntityChainStage::Apply,
];

pub const PROFILE_HASH_INVALIDATES: &[EntityChainStage] = &[
    EntityChainStage::Prepare,
    EntityChainStage::Index,
    EntityChainStage::Block,
    EntityChainStage::Edge,
    EntityChainStage::Solve,
    EntityChainStage::ReviewExport,
    EntityChainStage::ReviewImport,
    EntityChainStage::DecisionLedger,
    EntityChainStage::Audit,
    EntityChainStage::Promote,
    EntityChainStage::Apply,
];

pub const STRATEGY_HASH_INVALIDATES: &[EntityChainStage] = &[
    EntityChainStage::Prepare,
    EntityChainStage::Index,
    EntityChainStage::Block,
    EntityChainStage::Edge,
    EntityChainStage::Solve,
    EntityChainStage::ReviewExport,
    EntityChainStage::ReviewImport,
    EntityChainStage::DecisionLedger,
    EntityChainStage::Audit,
    EntityChainStage::Promote,
    EntityChainStage::Apply,
];

pub const REGISTRY_HASH_INVALIDATES: &[EntityChainStage] = &[
    EntityChainStage::Prepare,
    EntityChainStage::Index,
    EntityChainStage::Block,
    EntityChainStage::Edge,
    EntityChainStage::Solve,
    EntityChainStage::ReviewExport,
    EntityChainStage::ReviewImport,
    EntityChainStage::DecisionLedger,
    EntityChainStage::Audit,
    EntityChainStage::Promote,
    EntityChainStage::Apply,
];

pub const PATCH_HASH_INVALIDATES: &[EntityChainStage] = &[
    EntityChainStage::Prepare,
    EntityChainStage::Index,
    EntityChainStage::Block,
    EntityChainStage::Edge,
    EntityChainStage::Solve,
    EntityChainStage::ReviewExport,
    EntityChainStage::ReviewImport,
    EntityChainStage::DecisionLedger,
    EntityChainStage::Audit,
    EntityChainStage::Promote,
    EntityChainStage::Apply,
];

pub const NAMEKIT_HASH_INVALIDATES: &[EntityChainStage] = &[
    EntityChainStage::Prepare,
    EntityChainStage::Index,
    EntityChainStage::Block,
    EntityChainStage::Edge,
    EntityChainStage::Solve,
    EntityChainStage::ReviewExport,
    EntityChainStage::ReviewImport,
    EntityChainStage::DecisionLedger,
    EntityChainStage::Audit,
    EntityChainStage::Promote,
    EntityChainStage::Apply,
];

pub const UPSTREAM_ARTIFACT_HASH_INVALIDATES: &[EntityChainStage] = &[
    EntityChainStage::Index,
    EntityChainStage::Block,
    EntityChainStage::Edge,
    EntityChainStage::Solve,
    EntityChainStage::ReviewExport,
    EntityChainStage::ReviewImport,
    EntityChainStage::DecisionLedger,
    EntityChainStage::Audit,
    EntityChainStage::Promote,
    EntityChainStage::Apply,
];

pub const ARTIFACT_CONTENT_HASH_INVALIDATES: &[EntityChainStage] = &[
    EntityChainStage::Block,
    EntityChainStage::Edge,
    EntityChainStage::Solve,
    EntityChainStage::ReviewExport,
    EntityChainStage::ReviewImport,
    EntityChainStage::DecisionLedger,
    EntityChainStage::Audit,
    EntityChainStage::Promote,
    EntityChainStage::Apply,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityHashField {
    ArtifactVersion,
    ProfileId,
    ProfileVersion,
    ProfileHash,
    StrategyHash,
    RegistrySnapshotHash,
    InputHash,
    UpstreamArtifactHash,
    PatchHash,
    NamekitVersion,
    NamekitHash,
    ArtifactContentHash,
}

impl EntityHashField {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ArtifactVersion => "artifact_version",
            Self::ProfileId => "profile_id",
            Self::ProfileVersion => "profile_version",
            Self::ProfileHash => "profile_hash",
            Self::StrategyHash => "strategy_hash",
            Self::RegistrySnapshotHash => "registry_snapshot_hash",
            Self::InputHash => "input_hash",
            Self::UpstreamArtifactHash => "upstream_artifact_hash",
            Self::PatchHash => "patch_hash",
            Self::NamekitVersion => "namekit_version",
            Self::NamekitHash => "namekit_hash",
            Self::ArtifactContentHash => "artifact_content_hash",
        }
    }

    pub const fn invalidates(self) -> &'static [EntityChainStage] {
        match self {
            Self::ArtifactVersion => ARTIFACT_CONTENT_HASH_INVALIDATES,
            Self::ProfileId | Self::ProfileVersion | Self::ProfileHash => PROFILE_HASH_INVALIDATES,
            Self::StrategyHash => STRATEGY_HASH_INVALIDATES,
            Self::RegistrySnapshotHash => REGISTRY_HASH_INVALIDATES,
            Self::InputHash => INPUT_HASH_INVALIDATES,
            Self::UpstreamArtifactHash => UPSTREAM_ARTIFACT_HASH_INVALIDATES,
            Self::PatchHash => PATCH_HASH_INVALIDATES,
            Self::NamekitVersion | Self::NamekitHash => NAMEKIT_HASH_INVALIDATES,
            Self::ArtifactContentHash => ARTIFACT_CONTENT_HASH_INVALIDATES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityChainEnforcement {
    CacheMiss,
    ArtifactContractRefusal,
    RegistrySnapshotRefusal,
    ReviewImportRefusal,
    AuditGateRefusal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityChainMismatch {
    pub field: EntityHashField,
    pub expected: String,
    pub actual: String,
    pub invalidates: Vec<EntityChainStage>,
    pub enforcement: EntityChainEnforcement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityArtifactChainLink {
    pub version: String,
    pub profile_id: String,
    pub profile_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_hash: Option<String>,
    pub strategy_hash: String,
    pub registry_snapshot_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_hash: Option<String>,
    #[serde(default)]
    pub upstream_artifacts: Vec<EntityArtifactReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namekit_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namekit_hash: Option<String>,
    pub artifact_content_hash: String,
}

impl EntityArtifactChainLink {
    pub fn from_header(header: &EntityArtifactHeader) -> Self {
        let metadata = &header.metadata;
        Self {
            version: header.version.clone(),
            profile_id: metadata.profile.id.clone(),
            profile_version: metadata.profile.version.clone(),
            profile_hash: metadata.profile.content_hash.clone(),
            strategy_hash: metadata.strategy.content_hash.clone(),
            registry_snapshot_hash: metadata.registry_snapshot.lookup_snapshot_hash.clone(),
            input_hash: metadata
                .input
                .as_ref()
                .map(|input| input.content_hash.clone()),
            upstream_artifacts: metadata.upstream_artifacts.clone(),
            patch_hash: metadata
                .patch_set
                .as_ref()
                .map(|patch_set| patch_set.content_hash.clone()),
            namekit_version: metadata
                .namekit
                .as_ref()
                .map(|namekit| namekit.version.clone()),
            namekit_hash: metadata
                .namekit
                .as_ref()
                .map(|namekit| namekit.content_hash.clone()),
            artifact_content_hash: metadata.artifact_content_hash.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityArtifactChainExpectation {
    pub consumer_stage: EntityChainStage,
    pub expected_version: String,
    pub profile_id: String,
    pub profile_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_hash: Option<String>,
    pub strategy_hash: String,
    pub registry_snapshot_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_hash: Option<String>,
    #[serde(default)]
    pub upstream_artifacts: Vec<EntityArtifactReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namekit_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namekit_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_content_hash: Option<String>,
}

impl EntityArtifactChainExpectation {
    pub fn from_link(consumer_stage: EntityChainStage, link: &EntityArtifactChainLink) -> Self {
        Self {
            consumer_stage,
            expected_version: link.version.clone(),
            profile_id: link.profile_id.clone(),
            profile_version: link.profile_version.clone(),
            profile_hash: link.profile_hash.clone(),
            strategy_hash: link.strategy_hash.clone(),
            registry_snapshot_hash: link.registry_snapshot_hash.clone(),
            input_hash: link.input_hash.clone(),
            upstream_artifacts: link.upstream_artifacts.clone(),
            patch_hash: link.patch_hash.clone(),
            namekit_version: link.namekit_version.clone(),
            namekit_hash: link.namekit_hash.clone(),
            artifact_content_hash: Some(link.artifact_content_hash.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityArtifactChainValidation {
    pub consumer_stage: EntityChainStage,
    pub artifact_content_hash: String,
    pub validated_fields: Vec<EntityHashField>,
}

pub fn validate_artifact_chain(
    artifact: &EntityArtifactChainLink,
    expected: &EntityArtifactChainExpectation,
) -> Result<EntityArtifactChainValidation, Refusal> {
    let mismatches = collect_artifact_chain_mismatches(artifact, expected);
    if let Some(mismatch) = mismatches.first() {
        return Err(refusal_for_chain_mismatch(
            expected.consumer_stage,
            mismatch.clone(),
        ));
    }

    Ok(EntityArtifactChainValidation {
        consumer_stage: expected.consumer_stage,
        artifact_content_hash: artifact.artifact_content_hash.clone(),
        validated_fields: vec![
            EntityHashField::ArtifactVersion,
            EntityHashField::ProfileId,
            EntityHashField::ProfileVersion,
            EntityHashField::ProfileHash,
            EntityHashField::StrategyHash,
            EntityHashField::RegistrySnapshotHash,
            EntityHashField::InputHash,
            EntityHashField::UpstreamArtifactHash,
            EntityHashField::PatchHash,
            EntityHashField::NamekitVersion,
            EntityHashField::NamekitHash,
            EntityHashField::ArtifactContentHash,
        ],
    })
}

pub fn collect_artifact_chain_mismatches(
    artifact: &EntityArtifactChainLink,
    expected: &EntityArtifactChainExpectation,
) -> Vec<EntityChainMismatch> {
    let mut mismatches = Vec::new();

    compare_required(
        &mut mismatches,
        expected.consumer_stage,
        EntityHashField::ArtifactVersion,
        &expected.expected_version,
        &artifact.version,
    );
    compare_required(
        &mut mismatches,
        expected.consumer_stage,
        EntityHashField::ProfileId,
        &expected.profile_id,
        &artifact.profile_id,
    );
    compare_required(
        &mut mismatches,
        expected.consumer_stage,
        EntityHashField::ProfileVersion,
        &expected.profile_version,
        &artifact.profile_version,
    );
    compare_optional(
        &mut mismatches,
        expected.consumer_stage,
        EntityHashField::ProfileHash,
        &expected.profile_hash,
        &artifact.profile_hash,
    );
    compare_required(
        &mut mismatches,
        expected.consumer_stage,
        EntityHashField::StrategyHash,
        &expected.strategy_hash,
        &artifact.strategy_hash,
    );
    compare_required(
        &mut mismatches,
        expected.consumer_stage,
        EntityHashField::RegistrySnapshotHash,
        &expected.registry_snapshot_hash,
        &artifact.registry_snapshot_hash,
    );
    compare_optional(
        &mut mismatches,
        expected.consumer_stage,
        EntityHashField::InputHash,
        &expected.input_hash,
        &artifact.input_hash,
    );
    if !expected.upstream_artifacts.is_empty()
        && expected.upstream_artifacts != artifact.upstream_artifacts
    {
        mismatches.push(mismatch(
            expected.consumer_stage,
            EntityHashField::UpstreamArtifactHash,
            render_artifact_refs(&expected.upstream_artifacts),
            render_artifact_refs(&artifact.upstream_artifacts),
        ));
    }
    compare_optional(
        &mut mismatches,
        expected.consumer_stage,
        EntityHashField::PatchHash,
        &expected.patch_hash,
        &artifact.patch_hash,
    );
    compare_optional(
        &mut mismatches,
        expected.consumer_stage,
        EntityHashField::NamekitVersion,
        &expected.namekit_version,
        &artifact.namekit_version,
    );
    compare_optional(
        &mut mismatches,
        expected.consumer_stage,
        EntityHashField::NamekitHash,
        &expected.namekit_hash,
        &artifact.namekit_hash,
    );
    compare_optional_value(
        &mut mismatches,
        expected.consumer_stage,
        EntityHashField::ArtifactContentHash,
        expected.artifact_content_hash.as_deref(),
        Some(artifact.artifact_content_hash.as_str()),
    );

    mismatches
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityCacheStage {
    Prepare,
    Index,
}

impl EntityCacheStage {
    pub const fn as_chain_stage(self) -> EntityChainStage {
        match self {
            Self::Prepare => EntityChainStage::Prepare,
            Self::Index => EntityChainStage::Index,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityCacheDecision {
    Hit,
    Miss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityCacheValidation {
    pub stage: EntityCacheStage,
    pub decision: EntityCacheDecision,
    pub mismatches: Vec<EntityChainMismatch>,
    pub rebuild_allowed: bool,
}

pub fn validate_entity_cache_hit(
    stage: EntityCacheStage,
    cached: &EntityCacheKeyMaterial,
    current: &EntityCacheKeyMaterial,
) -> EntityCacheValidation {
    let consumer_stage = stage.as_chain_stage();
    let mut mismatches = Vec::new();

    compare_required(
        &mut mismatches,
        consumer_stage,
        EntityHashField::InputHash,
        &current.input_hash,
        &cached.input_hash,
    );
    compare_required(
        &mut mismatches,
        consumer_stage,
        EntityHashField::ProfileHash,
        &current.profile_hash,
        &cached.profile_hash,
    );
    compare_required(
        &mut mismatches,
        consumer_stage,
        EntityHashField::StrategyHash,
        &current.strategy_hash,
        &cached.strategy_hash,
    );
    compare_required(
        &mut mismatches,
        consumer_stage,
        EntityHashField::RegistrySnapshotHash,
        &current.registry_snapshot_hash,
        &cached.registry_snapshot_hash,
    );
    compare_optional(
        &mut mismatches,
        consumer_stage,
        EntityHashField::PatchHash,
        &current.patch_hash,
        &cached.patch_hash,
    );
    compare_required(
        &mut mismatches,
        consumer_stage,
        EntityHashField::NamekitVersion,
        &current.namekit_version,
        &cached.namekit_version,
    );
    compare_optional(
        &mut mismatches,
        consumer_stage,
        EntityHashField::NamekitHash,
        &current.namekit_hash,
        &cached.namekit_hash,
    );

    for mismatch in &mut mismatches {
        mismatch.enforcement = EntityChainEnforcement::CacheMiss;
    }

    EntityCacheValidation {
        stage,
        decision: if mismatches.is_empty() {
            EntityCacheDecision::Hit
        } else {
            EntityCacheDecision::Miss
        },
        mismatches,
        rebuild_allowed: true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitySolverAbstention {
    pub stage: EntityChainStage,
    pub component_id: String,
    pub reason: String,
    pub hard_cannot_link_count: u64,
    pub refusal_code: Option<String>,
}

impl EntitySolverAbstention {
    pub fn hard_cannot_link(component_id: impl Into<String>, hard_cannot_link_count: u64) -> Self {
        Self {
            stage: EntityChainStage::Solve,
            component_id: component_id.into(),
            reason: "hard_cannot_link_present".to_string(),
            hard_cannot_link_count,
            refusal_code: None,
        }
    }

    pub const fn is_refusal(&self) -> bool {
        false
    }
}

pub fn audit_gate_refusal(
    gate_id: impl Into<String>,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> Refusal {
    EntityRefusalKind::AuditGate.to_refusal(
        "Entity audit gate failed",
        json!({
            "stage": EntityChainStage::Audit.as_str(),
            "gate_id": gate_id.into(),
            "expected": expected.into(),
            "actual": actual.into(),
            "enforcement": EntityChainEnforcement::AuditGateRefusal,
            "writes_performed": false
        }),
        Some(
            "Fix the audited artifact or review threshold waiver, then rerun canon entity audit"
                .to_string(),
        ),
    )
}

pub fn required_profile_hash(profile: &EntityProfileReference) -> Option<String> {
    profile.content_hash.clone()
}

fn compare_required(
    mismatches: &mut Vec<EntityChainMismatch>,
    consumer_stage: EntityChainStage,
    field: EntityHashField,
    expected: &str,
    actual: &str,
) {
    compare_optional_value(
        mismatches,
        consumer_stage,
        field,
        Some(expected),
        Some(actual),
    );
}

fn compare_optional(
    mismatches: &mut Vec<EntityChainMismatch>,
    consumer_stage: EntityChainStage,
    field: EntityHashField,
    expected: &Option<String>,
    actual: &Option<String>,
) {
    compare_optional_value(
        mismatches,
        consumer_stage,
        field,
        expected.as_deref(),
        actual.as_deref(),
    );
}

fn compare_optional_value(
    mismatches: &mut Vec<EntityChainMismatch>,
    consumer_stage: EntityChainStage,
    field: EntityHashField,
    expected: Option<&str>,
    actual: Option<&str>,
) {
    let Some(expected) = expected else {
        return;
    };
    let actual = actual.unwrap_or("<missing>");
    if expected != actual {
        mismatches.push(mismatch(
            consumer_stage,
            field,
            expected.to_string(),
            actual.to_string(),
        ));
    }
}

fn mismatch(
    consumer_stage: EntityChainStage,
    field: EntityHashField,
    expected: String,
    actual: String,
) -> EntityChainMismatch {
    EntityChainMismatch {
        field,
        expected,
        actual,
        invalidates: field.invalidates().to_vec(),
        enforcement: enforcement_for_mismatch(consumer_stage, field),
    }
}

fn enforcement_for_mismatch(
    consumer_stage: EntityChainStage,
    field: EntityHashField,
) -> EntityChainEnforcement {
    if field == EntityHashField::RegistrySnapshotHash {
        EntityChainEnforcement::RegistrySnapshotRefusal
    } else if consumer_stage == EntityChainStage::ReviewImport {
        EntityChainEnforcement::ReviewImportRefusal
    } else {
        EntityChainEnforcement::ArtifactContractRefusal
    }
}

fn refusal_for_chain_mismatch(
    consumer_stage: EntityChainStage,
    mismatch: EntityChainMismatch,
) -> Refusal {
    let kind = match mismatch.enforcement {
        EntityChainEnforcement::CacheMiss => EntityRefusalKind::CacheMismatch,
        EntityChainEnforcement::ArtifactContractRefusal => EntityRefusalKind::ArtifactContract,
        EntityChainEnforcement::RegistrySnapshotRefusal => EntityRefusalKind::RegistrySnapshot,
        EntityChainEnforcement::ReviewImportRefusal => EntityRefusalKind::ReviewImport,
        EntityChainEnforcement::AuditGateRefusal => EntityRefusalKind::AuditGate,
    };

    kind.to_refusal(
        "Entity artifact chain validation failed",
        json!({
            "contract_version": ENTITY_ARTIFACT_CHAIN_CONTRACT_VERSION,
            "stage": consumer_stage.as_str(),
            "reason": "stale_or_mismatched_artifact",
            "field": mismatch.field.as_str(),
            "expected": mismatch.expected,
            "actual": mismatch.actual,
            "invalidates": mismatch.invalidates,
            "enforcement": mismatch.enforcement,
            "writes_performed": false
        }),
        Some(format!(
            "Use matching upstream artifacts or rerun canon entity {}",
            consumer_stage.command_name()
        )),
    )
}

fn render_artifact_refs(artifacts: &[EntityArtifactReference]) -> String {
    artifacts
        .iter()
        .map(|artifact| format!("{}@{}", artifact.version, artifact.content_hash))
        .collect::<Vec<_>>()
        .join(",")
}
