//! CMBS tenant-label profile helpers.
//!
//! The allocator in this module is deliberately pure: it derives a candidate
//! `TNT-*` ID and validates replay/collision metadata, but it does not write a
//! registry, mapping file, or output row.

use crate::entity::{
    EntityPatchNamespaces, EntityProfileReference,
    error::{EntityRefusalKind, entity_refusal},
};
use crate::{Refusal, RefusalCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

pub const CMBS_TENANT_PROFILE_ID: &str = "cmbs_tenant_label";
pub const CMBS_TENANT_PROFILE_VERSION: &str = "0.1.0";
pub const CMBS_TENANT_ENTITY_TYPE: &str = "tenant_label";
pub const CMBS_TENANT_IDENTITY_SEMANTICS: &str = "canonical_display_label";
pub const CMBS_TENANT_CANONICAL_TYPE: &str = "tenant_label";
pub const CMBS_TENANT_ID_PREFIX: &str = "TNT";
pub const CMBS_TENANT_ID_ALLOCATOR_VERSION: &str = "canon_entity_cmbs_tenant_id_allocator.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CmbsTenantIdAllocationRequest {
    pub profile_id: String,
    pub profile_version: String,
    pub canonical_type: String,
    pub identity_semantics: String,
    pub reviewed_display_label: String,
    pub normalized_display_label: String,
    pub registry_snapshot_hash: String,
    pub alias_patch_hash: String,
    pub review_decision_id: String,
}

impl CmbsTenantIdAllocationRequest {
    pub fn new(
        reviewed_display_label: impl Into<String>,
        normalized_display_label: impl Into<String>,
        registry_snapshot_hash: impl Into<String>,
        alias_patch_hash: impl Into<String>,
        review_decision_id: impl Into<String>,
    ) -> Self {
        Self {
            profile_id: CMBS_TENANT_PROFILE_ID.to_string(),
            profile_version: CMBS_TENANT_PROFILE_VERSION.to_string(),
            canonical_type: CMBS_TENANT_CANONICAL_TYPE.to_string(),
            identity_semantics: CMBS_TENANT_IDENTITY_SEMANTICS.to_string(),
            reviewed_display_label: reviewed_display_label.into(),
            normalized_display_label: normalized_display_label.into(),
            registry_snapshot_hash: registry_snapshot_hash.into(),
            alias_patch_hash: alias_patch_hash.into(),
            review_decision_id: review_decision_id.into(),
        }
    }

    pub fn replay_key(&self) -> CmbsTenantIdReplayKey {
        CmbsTenantIdReplayKey {
            profile_id: self.profile_id.clone(),
            canonical_type: self.canonical_type.clone(),
            identity_semantics: self.identity_semantics.clone(),
            normalized_display_label: self.normalized_display_label.clone(),
            registry_snapshot_hash: self.registry_snapshot_hash.clone(),
            alias_patch_hash: self.alias_patch_hash.clone(),
            review_decision_id: self.review_decision_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CmbsTenantIdReplayKey {
    pub profile_id: String,
    pub canonical_type: String,
    pub identity_semantics: String,
    pub normalized_display_label: String,
    pub registry_snapshot_hash: String,
    pub alias_patch_hash: String,
    pub review_decision_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CmbsTenantReservedId {
    pub canonical_id: String,
    pub replay_key: CmbsTenantIdReplayKey,
}

impl CmbsTenantReservedId {
    pub fn new(canonical_id: impl Into<String>, replay_key: CmbsTenantIdReplayKey) -> Self {
        Self {
            canonical_id: canonical_id.into(),
            replay_key,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CmbsTenantIdAllocation {
    pub version: String,
    pub profile: EntityProfileReference,
    pub canonical_id: String,
    pub replay_key: CmbsTenantIdReplayKey,
    pub candidate_source: String,
    pub candidate_normalization: String,
    pub collision_policy: String,
    pub side_effects: CmbsTenantIdAllocationSideEffects,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CmbsTenantIdAllocationSideEffects {
    pub registry_writes: u8,
    pub output_rows_written: u8,
}

#[derive(Debug, Clone, Default)]
pub struct CmbsTenantIdAllocator {
    reserved_by_id: BTreeMap<String, CmbsTenantIdReplayKey>,
}

impl CmbsTenantIdAllocator {
    pub fn new(reserved: impl IntoIterator<Item = CmbsTenantReservedId>) -> Self {
        Self {
            reserved_by_id: reserved
                .into_iter()
                .map(|reserved| (reserved.canonical_id, reserved.replay_key))
                .collect(),
        }
    }

    pub fn allocate(
        &self,
        request: &CmbsTenantIdAllocationRequest,
    ) -> Result<CmbsTenantIdAllocation, Refusal> {
        validate_request(request)?;

        let canonical_id = candidate_tnt_id(&request.reviewed_display_label)?;
        let replay_key = request.replay_key();

        if let Some(existing_key) = self.reserved_by_id.get(&canonical_id) {
            if existing_key == &replay_key {
                return Ok(allocation(canonical_id, replay_key));
            }
            return Err(collision_refusal(&canonical_id, &replay_key, existing_key));
        }

        Ok(allocation(canonical_id, replay_key))
    }
}

pub fn cmbs_tenant_profile_reference() -> EntityProfileReference {
    EntityProfileReference {
        id: CMBS_TENANT_PROFILE_ID.to_string(),
        version: CMBS_TENANT_PROFILE_VERSION.to_string(),
        entity_type: CMBS_TENANT_ENTITY_TYPE.to_string(),
        identity_semantics: CMBS_TENANT_IDENTITY_SEMANTICS.to_string(),
        canonical_type: CMBS_TENANT_CANONICAL_TYPE.to_string(),
        patch_namespaces: EntityPatchNamespaces {
            aliases: "cmbs_tenant_label.aliases".to_string(),
            distinct: "cmbs_tenant_label.distinct".to_string(),
            relations: "cmbs_tenant_label.relations".to_string(),
        },
        content_hash: None,
    }
}

pub fn candidate_tnt_id(reviewed_display_label: &str) -> Result<String, Refusal> {
    let slug = uppercase_ascii_slug(reviewed_display_label);
    if slug.is_empty() {
        return Err(entity_refusal(
            EntityRefusalKind::InputContract,
            "CMBS tenant ID allocation requires a non-empty reviewed display label",
            json!({
                "field": "reviewed_display_label",
                "normalization": "uppercase_ascii_slug"
            }),
        ));
    }
    Ok(format!("{CMBS_TENANT_ID_PREFIX}-{slug}"))
}

pub fn is_valid_tnt_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("TNT-") else {
        return false;
    };
    !rest.is_empty()
        && rest.split('-').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        })
}

fn allocation(canonical_id: String, replay_key: CmbsTenantIdReplayKey) -> CmbsTenantIdAllocation {
    CmbsTenantIdAllocation {
        version: CMBS_TENANT_ID_ALLOCATOR_VERSION.to_string(),
        profile: cmbs_tenant_profile_reference(),
        canonical_id,
        replay_key,
        candidate_source: "reviewed_display_label".to_string(),
        candidate_normalization: "uppercase_ascii_slug".to_string(),
        collision_policy: "refuse_without_suffix_or_silent_remint".to_string(),
        side_effects: CmbsTenantIdAllocationSideEffects {
            registry_writes: 0,
            output_rows_written: 0,
        },
    }
}

fn validate_request(request: &CmbsTenantIdAllocationRequest) -> Result<(), Refusal> {
    let mut missing = Vec::new();
    for (field, value) in [
        ("profile_id", request.profile_id.as_str()),
        ("profile_version", request.profile_version.as_str()),
        ("canonical_type", request.canonical_type.as_str()),
        ("identity_semantics", request.identity_semantics.as_str()),
        (
            "reviewed_display_label",
            request.reviewed_display_label.as_str(),
        ),
        (
            "normalized_display_label",
            request.normalized_display_label.as_str(),
        ),
        (
            "registry_snapshot_hash",
            request.registry_snapshot_hash.as_str(),
        ),
        ("alias_patch_hash", request.alias_patch_hash.as_str()),
        ("review_decision_id", request.review_decision_id.as_str()),
    ] {
        if value.trim().is_empty() {
            missing.push(field);
        }
    }
    if !missing.is_empty() {
        return Err(entity_refusal(
            EntityRefusalKind::InputContract,
            "CMBS tenant ID allocation request is missing required replay fields",
            json!({ "missing": missing }),
        ));
    }

    let profile = cmbs_tenant_profile_reference();
    let mut mismatches = Vec::new();
    for (field, actual, expected) in [
        (
            "profile_id",
            request.profile_id.as_str(),
            profile.id.as_str(),
        ),
        (
            "profile_version",
            request.profile_version.as_str(),
            profile.version.as_str(),
        ),
        (
            "canonical_type",
            request.canonical_type.as_str(),
            profile.canonical_type.as_str(),
        ),
        (
            "identity_semantics",
            request.identity_semantics.as_str(),
            profile.identity_semantics.as_str(),
        ),
    ] {
        if actual != expected {
            mismatches.push(json!({
                "field": field,
                "actual": actual,
                "expected": expected
            }));
        }
    }
    if !mismatches.is_empty() {
        return Err(entity_refusal(
            EntityRefusalKind::Profile,
            "CMBS tenant ID allocator cannot be reused across profile semantics",
            json!({ "mismatches": mismatches }),
        ));
    }

    Ok(())
}

fn collision_refusal(
    canonical_id: &str,
    requested_key: &CmbsTenantIdReplayKey,
    existing_key: &CmbsTenantIdReplayKey,
) -> Refusal {
    if existing_key.profile_id != CMBS_TENANT_PROFILE_ID {
        return refusal_with_next_command(
            RefusalCode::EEntityProfile,
            "CMBS tenant ID allocator refuses cross-profile canonical ID reuse",
            json!({
                "canonical_id": canonical_id,
                "existing_profile_id": existing_key.profile_id,
                "requested_profile_id": requested_key.profile_id,
                "relation_policy": "relation_hint_only"
            }),
        );
    }

    if differs_only_registry_snapshot(existing_key, requested_key) {
        return refusal_with_next_command(
            RefusalCode::EEntityRegistrySnapshot,
            "CMBS tenant ID allocation is stale for the current registry snapshot",
            json!({
                "canonical_id": canonical_id,
                "existing_registry_snapshot_hash": existing_key.registry_snapshot_hash,
                "requested_registry_snapshot_hash": requested_key.registry_snapshot_hash
            }),
        );
    }

    refusal_with_next_command(
        RefusalCode::EEntityPatchConflict,
        "CMBS tenant ID allocation collided with an existing reviewed tenant label",
        json!({
            "canonical_id": canonical_id,
            "collision_policy": "refuse_without_suffix_or_silent_remint",
            "existing_replay_key": existing_key,
            "requested_replay_key": requested_key
        }),
    )
}

fn refusal_with_next_command(
    code: RefusalCode,
    message: impl Into<String>,
    detail: serde_json::Value,
) -> Refusal {
    let next_command = match code {
        RefusalCode::EEntityProfile => {
            "Record a relation hint instead of reusing the TNT id across profiles"
        }
        RefusalCode::EEntityRegistrySnapshot => {
            "Re-run from prepare or use the matching registry snapshot before promotion"
        }
        RefusalCode::EEntityPatchConflict => {
            "Resolve the patch conflict before running tenant ID promotion again"
        }
        _ => code.default_next_command(),
    };
    Refusal {
        code,
        message: message.into(),
        detail,
        next_command: Some(next_command.to_string()),
    }
}

fn differs_only_registry_snapshot(
    left: &CmbsTenantIdReplayKey,
    right: &CmbsTenantIdReplayKey,
) -> bool {
    left.profile_id == right.profile_id
        && left.canonical_type == right.canonical_type
        && left.identity_semantics == right.identity_semantics
        && left.normalized_display_label == right.normalized_display_label
        && left.alias_patch_hash == right.alias_patch_hash
        && left.review_decision_id == right.review_decision_id
        && left.registry_snapshot_hash != right.registry_snapshot_hash
}

fn uppercase_ascii_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = true;

    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() {
            slug.push(byte.to_ascii_uppercase() as char);
            last_was_separator = false;
        } else if !last_was_separator {
            slug.push('-');
            last_was_separator = true;
        }
    }

    if last_was_separator {
        slug.pop();
    }
    slug
}
