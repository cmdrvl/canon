#![forbid(unsafe_code)]

//! Shared cache-key and invalidation contracts for entity stages.
//!
//! Stage owners decide how to persist and reload their own caches. This module
//! only names the hash material and cache layers required by I21/G10 so a stale
//! warm-cache hit cannot silently reuse evidence from a different run.

use crate::entity::{
    artifact_chain::{EntityCacheDecision, EntityHashField},
    contracts::EntityCacheKeyMaterial,
};
use serde::{Deserialize, Serialize};

pub const ENTITY_CACHE_CONTRACT_VERSION: &str = "canon_entity_cache.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityCacheLayer {
    NormalizedSurfaces,
    TokenDictionary,
    IdfTable,
    ExactViewPostings,
    TokenPostings,
    NgramPostings,
    CandidateBlocks,
    EdgeEvidence,
}

impl EntityCacheLayer {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NormalizedSurfaces => "normalized_surfaces",
            Self::TokenDictionary => "token_dictionary",
            Self::IdfTable => "idf_table",
            Self::ExactViewPostings => "exact_view_postings",
            Self::TokenPostings => "token_postings",
            Self::NgramPostings => "ngram_postings",
            Self::CandidateBlocks => "candidate_blocks",
            Self::EdgeEvidence => "edge_evidence",
        }
    }
}

pub const ENTITY_CACHE_LAYERS: &[EntityCacheLayer] = &[
    EntityCacheLayer::NormalizedSurfaces,
    EntityCacheLayer::TokenDictionary,
    EntityCacheLayer::IdfTable,
    EntityCacheLayer::ExactViewPostings,
    EntityCacheLayer::TokenPostings,
    EntityCacheLayer::NgramPostings,
    EntityCacheLayer::CandidateBlocks,
    EntityCacheLayer::EdgeEvidence,
];

pub const ENTITY_CACHE_UPSTREAM_BACKED_LAYERS: &[EntityCacheLayer] = &[
    EntityCacheLayer::TokenDictionary,
    EntityCacheLayer::IdfTable,
    EntityCacheLayer::ExactViewPostings,
    EntityCacheLayer::TokenPostings,
    EntityCacheLayer::NgramPostings,
    EntityCacheLayer::CandidateBlocks,
    EntityCacheLayer::EdgeEvidence,
];

pub const ENTITY_CACHE_I21_FIELDS: &[EntityHashField] = &[
    EntityHashField::InputHash,
    EntityHashField::ProfileHash,
    EntityHashField::StrategyHash,
    EntityHashField::RegistrySnapshotHash,
    EntityHashField::PatchHash,
    EntityHashField::NamekitVersion,
    EntityHashField::NamekitHash,
];

pub const ENTITY_CACHE_NORMALIZED_SURFACE_FIELDS: &[EntityHashField] = &[
    EntityHashField::InputHash,
    EntityHashField::ProfileHash,
    EntityHashField::StrategyHash,
    EntityHashField::RegistrySnapshotHash,
    EntityHashField::PatchHash,
    EntityHashField::NamekitVersion,
    EntityHashField::NamekitHash,
];

pub const ENTITY_CACHE_UPSTREAM_BACKED_FIELDS: &[EntityHashField] = &[
    EntityHashField::InputHash,
    EntityHashField::ProfileHash,
    EntityHashField::StrategyHash,
    EntityHashField::RegistrySnapshotHash,
    EntityHashField::UpstreamArtifactHash,
    EntityHashField::PatchHash,
    EntityHashField::NamekitVersion,
    EntityHashField::NamekitHash,
];

const CACHE_COMPARE_ORDER: &[EntityHashField] = &[
    EntityHashField::ArtifactVersion,
    EntityHashField::InputHash,
    EntityHashField::ProfileHash,
    EntityHashField::StrategyHash,
    EntityHashField::RegistrySnapshotHash,
    EntityHashField::UpstreamArtifactHash,
    EntityHashField::PatchHash,
    EntityHashField::NamekitVersion,
    EntityHashField::NamekitHash,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityCacheKey {
    pub contract_version: String,
    pub layer: EntityCacheLayer,
    pub input_hash: String,
    pub profile_hash: String,
    pub strategy_hash: String,
    pub registry_snapshot_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_artifact_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_hash: Option<String>,
    pub namekit_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namekit_hash: Option<String>,
}

impl EntityCacheKey {
    pub fn from_i21_material(layer: EntityCacheLayer, material: EntityCacheKeyMaterial) -> Self {
        Self {
            contract_version: ENTITY_CACHE_CONTRACT_VERSION.to_string(),
            layer,
            input_hash: material.input_hash,
            profile_hash: material.profile_hash,
            strategy_hash: material.strategy_hash,
            registry_snapshot_hash: material.registry_snapshot_hash,
            upstream_artifact_hash: None,
            patch_hash: material.patch_hash,
            namekit_version: material.namekit_version,
            namekit_hash: material.namekit_hash,
        }
    }

    pub fn with_upstream_artifact_hash(mut self, hash: impl Into<String>) -> Self {
        self.upstream_artifact_hash = Some(hash.into());
        self
    }

    pub fn required_hash_fields(&self) -> &'static [EntityHashField] {
        required_hash_fields_for_layer(self.layer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityCacheInvalidation {
    pub layer: EntityCacheLayer,
    pub decision: EntityCacheDecision,
    pub changed_fields: Vec<EntityHashField>,
    pub invalidated_layers: Vec<EntityCacheLayer>,
}

pub const fn required_hash_fields_for_layer(layer: EntityCacheLayer) -> &'static [EntityHashField] {
    match layer {
        EntityCacheLayer::NormalizedSurfaces => ENTITY_CACHE_NORMALIZED_SURFACE_FIELDS,
        EntityCacheLayer::TokenDictionary
        | EntityCacheLayer::IdfTable
        | EntityCacheLayer::ExactViewPostings
        | EntityCacheLayer::TokenPostings
        | EntityCacheLayer::NgramPostings
        | EntityCacheLayer::CandidateBlocks
        | EntityCacheLayer::EdgeEvidence => ENTITY_CACHE_UPSTREAM_BACKED_FIELDS,
    }
}

pub const fn invalidated_layers_for_hash_field(
    field: EntityHashField,
) -> &'static [EntityCacheLayer] {
    match field {
        EntityHashField::ArtifactVersion
        | EntityHashField::ProfileId
        | EntityHashField::ProfileVersion
        | EntityHashField::ProfileHash
        | EntityHashField::StrategyHash
        | EntityHashField::RegistrySnapshotHash
        | EntityHashField::InputHash
        | EntityHashField::PatchHash
        | EntityHashField::NamekitVersion
        | EntityHashField::NamekitHash => ENTITY_CACHE_LAYERS,
        EntityHashField::UpstreamArtifactHash | EntityHashField::ArtifactContentHash => {
            ENTITY_CACHE_UPSTREAM_BACKED_LAYERS
        }
    }
}

pub fn compare_cache_keys(
    cached: &EntityCacheKey,
    current: &EntityCacheKey,
) -> EntityCacheInvalidation {
    let mut changed_fields = Vec::new();

    for field in CACHE_COMPARE_ORDER {
        if cache_key_field_changed(*field, cached, current) {
            changed_fields.push(*field);
        }
    }

    let invalidated_layers = invalidated_layers_for_changed_fields(&changed_fields);
    EntityCacheInvalidation {
        layer: current.layer,
        decision: if changed_fields.is_empty() {
            EntityCacheDecision::Hit
        } else {
            EntityCacheDecision::Miss
        },
        changed_fields,
        invalidated_layers,
    }
}

pub fn invalidated_layers_for_changed_fields(
    changed_fields: &[EntityHashField],
) -> Vec<EntityCacheLayer> {
    let mut layers = Vec::new();
    for layer in ENTITY_CACHE_LAYERS {
        if changed_fields
            .iter()
            .any(|field| invalidated_layers_for_hash_field(*field).contains(layer))
        {
            layers.push(*layer);
        }
    }
    layers
}

fn cache_key_field_changed(
    field: EntityHashField,
    cached: &EntityCacheKey,
    current: &EntityCacheKey,
) -> bool {
    match field {
        EntityHashField::ArtifactVersion => {
            cached.contract_version != current.contract_version || cached.layer != current.layer
        }
        EntityHashField::InputHash => cached.input_hash != current.input_hash,
        EntityHashField::ProfileHash => cached.profile_hash != current.profile_hash,
        EntityHashField::StrategyHash => cached.strategy_hash != current.strategy_hash,
        EntityHashField::RegistrySnapshotHash => {
            cached.registry_snapshot_hash != current.registry_snapshot_hash
        }
        EntityHashField::UpstreamArtifactHash => {
            cached.upstream_artifact_hash != current.upstream_artifact_hash
        }
        EntityHashField::PatchHash => cached.patch_hash != current.patch_hash,
        EntityHashField::NamekitVersion => cached.namekit_version != current.namekit_version,
        EntityHashField::NamekitHash => cached.namekit_hash != current.namekit_hash,
        EntityHashField::ProfileId
        | EntityHashField::ProfileVersion
        | EntityHashField::ArtifactContentHash => false,
    }
}
