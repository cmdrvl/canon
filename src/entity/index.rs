#![forbid(unsafe_code)]

//! Entity index artifact and cache-key contract.
//!
//! The index stage is an accelerator over prepared unique surfaces. This module
//! defines only the persisted artifact shell and cache validation contract; it
//! does not build postings or make identity decisions.

use crate::{
    Refusal,
    entity::{
        CANON_ENTITY_INDEX_VERSION, CANON_ENTITY_PREPARE_VERSION, EntityArtifactMetadata,
        EntityArtifactReference, EntityCacheKeyMaterial, EntityDeterministicSummary,
        EntityStrategyReference,
        artifact_chain::{EntityCacheDecision, EntityHashField},
        cache::{EntityCacheInvalidation, EntityCacheKey, EntityCacheLayer, compare_cache_keys},
        contracts::EntityArtifactHeader,
        error::EntityRefusalKind,
    },
    witness,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityIndexCacheStatus {
    Hit,
    Miss,
    Rebuilt,
}

impl EntityIndexCacheStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Miss => "miss",
            Self::Rebuilt => "rebuilt",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityIndexCachePolicy {
    RebuildOnMiss,
    RefuseOnMiss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityIndexArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub metadata: EntityArtifactMetadata,
    pub prepare_hash: String,
    pub summary: EntityDeterministicSummary,
    pub postings_path: String,
    pub diagnostics_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityIndexArtifactRequest {
    pub prepare: EntityArtifactHeader,
    pub strategy: EntityStrategyReference,
    pub cache_status: EntityIndexCacheStatus,
    pub postings_path: String,
    pub diagnostics_path: String,
    pub counts: BTreeMap<String, u64>,
}

pub fn build_index_artifact_contract(
    request: EntityIndexArtifactRequest,
) -> Result<EntityIndexArtifact, Refusal> {
    validate_prepare_header(&request.prepare)?;
    let prepare_hash = request.prepare.metadata.artifact_content_hash.clone();
    let metadata = index_metadata_from_prepare(&request.prepare, request.strategy)?;
    let mut labels = BTreeMap::from([(
        "cache_status".to_string(),
        request.cache_status.as_str().to_string(),
    )]);
    labels.insert(
        "upstream_version".to_string(),
        request.prepare.version.clone(),
    );

    let mut artifact = EntityIndexArtifact {
        version: CANON_ENTITY_INDEX_VERSION.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        prepare_hash,
        summary: EntityDeterministicSummary {
            counts: request.counts,
            labels,
        },
        postings_path: request.postings_path,
        diagnostics_path: request.diagnostics_path,
    };
    artifact.artifact_content_hash = hash_index_artifact_without_self(&artifact)?;
    artifact.metadata.artifact_content_hash = artifact.artifact_content_hash.clone();
    Ok(artifact)
}

pub fn index_cache_key_from_prepare_header(
    layer: EntityCacheLayer,
    prepare: &EntityArtifactHeader,
    index_strategy: &EntityStrategyReference,
) -> Result<EntityCacheKey, Refusal> {
    validate_prepare_header(prepare)?;
    let metadata = &prepare.metadata;
    let profile_hash = metadata
        .profile
        .content_hash
        .clone()
        .ok_or_else(|| missing_prepare_metadata("metadata.profile.content_hash"))?;
    let input = metadata
        .input
        .as_ref()
        .ok_or_else(|| missing_prepare_metadata("metadata.input"))?;
    let patch_hash = metadata
        .patch_set
        .as_ref()
        .map(|patch| patch.content_hash.clone());
    let namekit = metadata
        .namekit
        .as_ref()
        .ok_or_else(|| missing_prepare_metadata("metadata.namekit"))?;

    Ok(EntityCacheKey::from_i21_material(
        layer,
        EntityCacheKeyMaterial {
            input_hash: input.content_hash.clone(),
            profile_hash,
            strategy_hash: index_strategy.content_hash.clone(),
            registry_snapshot_hash: metadata.registry_snapshot.lookup_snapshot_hash.clone(),
            patch_hash,
            namekit_version: namekit.version.clone(),
            namekit_hash: Some(namekit.content_hash.clone()),
        },
    )
    .with_upstream_artifact_hash(metadata.artifact_content_hash.clone()))
}

pub fn validate_index_cache_policy(
    cached: &EntityCacheKey,
    current: &EntityCacheKey,
    policy: EntityIndexCachePolicy,
) -> Result<EntityCacheInvalidation, Refusal> {
    let invalidation = compare_cache_keys(cached, current);
    if invalidation.decision == EntityCacheDecision::Miss
        && policy == EntityIndexCachePolicy::RefuseOnMiss
    {
        return Err(cache_mismatch_refusal(&invalidation));
    }
    Ok(invalidation)
}

fn index_metadata_from_prepare(
    prepare: &EntityArtifactHeader,
    strategy: EntityStrategyReference,
) -> Result<EntityArtifactMetadata, Refusal> {
    validate_prepare_header(prepare)?;
    let metadata = &prepare.metadata;
    Ok(EntityArtifactMetadata {
        profile: metadata.profile.clone(),
        strategy,
        registry_snapshot: metadata.registry_snapshot.clone(),
        patch_namespace: metadata.patch_namespace.clone(),
        input: metadata.input.clone(),
        upstream_artifacts: vec![EntityArtifactReference {
            version: prepare.version.clone(),
            content_hash: metadata.artifact_content_hash.clone(),
        }],
        patch_set: metadata.patch_set.clone(),
        namekit: metadata.namekit.clone(),
        artifact_content_hash: String::new(),
    })
}

fn validate_prepare_header(prepare: &EntityArtifactHeader) -> Result<(), Refusal> {
    if prepare.version != CANON_ENTITY_PREPARE_VERSION {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity index requires a canon_entity_prepare.v0 artifact",
            json!({
                "expected": CANON_ENTITY_PREPARE_VERSION,
                "actual": prepare.version,
                "stage": "index",
                "writes_performed": false
            }),
            Some("Use a matching prepare artifact or rerun canon entity prepare".to_string()),
        ));
    }
    if prepare.metadata.artifact_content_hash.trim().is_empty() {
        return Err(missing_prepare_metadata("metadata.artifact_content_hash"));
    }
    Ok(())
}

fn missing_prepare_metadata(field: &str) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Entity prepare artifact is missing index cache metadata",
        json!({
            "field": field,
            "stage": "index",
            "writes_performed": false
        }),
        Some("Use a complete prepare artifact or rerun canon entity prepare".to_string()),
    )
}

fn cache_mismatch_refusal(invalidation: &EntityCacheInvalidation) -> Refusal {
    EntityRefusalKind::CacheMismatch.to_refusal(
        "Entity index cache key does not match current artifact inputs",
        json!({
            "stage": "index",
            "layer": invalidation.layer,
            "decision": invalidation.decision,
            "changed_fields": invalidation
                .changed_fields
                .iter()
                .map(|field| field.as_str())
                .collect::<Vec<_>>(),
            "invalidated_layers": invalidation.invalidated_layers,
            "writes_performed": false
        }),
        Some("Rebuild the entity index cache or use the matching prepare artifact".to_string()),
    )
}

fn hash_index_artifact_without_self(artifact: &EntityIndexArtifact) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to hash entity index artifact",
            json!({ "error": error.to_string(), "stage": "index" }),
            None,
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

pub fn index_summary_counts(
    surface_count: u64,
    token_count: u64,
    ngram_count: u64,
    large_bucket_count: u64,
) -> BTreeMap<String, u64> {
    BTreeMap::from([
        ("surface_count".to_string(), surface_count),
        ("token_count".to_string(), token_count),
        ("ngram_count".to_string(), ngram_count),
        ("large_bucket_count".to_string(), large_bucket_count),
    ])
}

pub fn required_index_hash_fields() -> &'static [EntityHashField] {
    &[
        EntityHashField::InputHash,
        EntityHashField::ProfileHash,
        EntityHashField::StrategyHash,
        EntityHashField::RegistrySnapshotHash,
        EntityHashField::UpstreamArtifactHash,
        EntityHashField::PatchHash,
        EntityHashField::NamekitVersion,
        EntityHashField::NamekitHash,
    ]
}
