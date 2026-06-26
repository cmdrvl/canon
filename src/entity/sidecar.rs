#![forbid(unsafe_code)]

//! Promotion sidecars for entity knowledge that is not an exact alias.

use crate::{
    Refusal,
    entity::{
        contracts::{
            CANON_ENTITY_AUDIT_VERSION, CANON_ENTITY_DECISION_LEDGER_VERSION,
            EntityArtifactMetadata, EntityArtifactReference, EntityDeterministicSummary,
        },
        error::EntityRefusalKind,
        patches::{CannotLinkSidecarRecord, RelationReviewPatch, ReviewPatchBundle},
        schema::{CANON_ENTITY_PROMOTION_PROOF_VERSION, CANON_ENTITY_PROMOTION_SIDECAR_VERSION},
    },
    witness,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityPromotionSidecarRequest {
    pub metadata: EntityArtifactMetadata,
    pub source_audit_hash: String,
    pub source_decision_ledger_hash: String,
    pub patch_bundle: ReviewPatchBundle,
    pub escrow_entities: Vec<EntityEscrowSidecarRecord>,
    pub contradiction_entities: Vec<EntityContradictionSidecarRecord>,
    pub promoted_alias_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPromotionSidecarArtifacts {
    pub sidecar: EntityPromotionSidecarArtifact,
    pub proof: EntityPromotionProofArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPromotionSidecarArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub metadata: EntityArtifactMetadata,
    pub summary: EntityDeterministicSummary,
    pub source_audit_hash: String,
    pub source_decision_ledger_hash: String,
    pub escrow_entities: Vec<EntityEscrowSidecarRecord>,
    pub contradiction_entities: Vec<EntityContradictionSidecarRecord>,
    pub cannot_link_facts: Vec<CannotLinkSidecarRecord>,
    pub relation_hints: Vec<RelationReviewPatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPromotionProofArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub metadata: EntityArtifactMetadata,
    pub summary: EntityDeterministicSummary,
    pub source_audit_hash: String,
    pub sidecar_snapshot_hash: String,
    pub registry_snapshot_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityEscrowSidecarRecord {
    pub escrow_id: String,
    pub profile_id: String,
    pub identity_semantics: String,
    pub surface_ids: Vec<String>,
    pub reason: String,
    pub source_decision_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityContradictionSidecarRecord {
    pub contradiction_id: String,
    pub profile_id: String,
    pub identity_semantics: String,
    pub surface_ids: Vec<String>,
    pub reason: String,
    pub source_decision_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPromotionSidecarWriteReceipt {
    pub sidecar_path: PathBuf,
    pub proof_path: PathBuf,
    pub sidecar_snapshot_hash: String,
    pub proof_hash: String,
    pub bytes_written: u64,
}

pub fn build_promotion_sidecar_artifacts(
    request: EntityPromotionSidecarRequest,
) -> Result<EntityPromotionSidecarArtifacts, Refusal> {
    validate_sidecar_request(&request)?;

    let mut cannot_link_facts = request.patch_bundle.cannot_link_sidecars;
    cannot_link_facts.sort_by(|left, right| left.sidecar_id.cmp(&right.sidecar_id));
    let mut relation_hints = request.patch_bundle.relation_patches;
    relation_hints.sort_by(|left, right| left.patch_id.cmp(&right.patch_id));
    let mut escrow_entities = request.escrow_entities;
    for escrow in &mut escrow_entities {
        escrow.surface_ids.sort();
        escrow.surface_ids.dedup();
    }
    escrow_entities.sort();
    let mut contradiction_entities = request.contradiction_entities;
    for contradiction in &mut contradiction_entities {
        contradiction.surface_ids.sort();
        contradiction.surface_ids.dedup();
    }
    contradiction_entities.sort();

    let mut metadata = request.metadata;
    metadata.artifact_content_hash.clear();
    metadata.upstream_artifacts = vec![
        EntityArtifactReference {
            version: CANON_ENTITY_AUDIT_VERSION.to_string(),
            content_hash: request.source_audit_hash.clone(),
        },
        EntityArtifactReference {
            version: CANON_ENTITY_DECISION_LEDGER_VERSION.to_string(),
            content_hash: request.source_decision_ledger_hash.clone(),
        },
    ];

    let mut sidecar = EntityPromotionSidecarArtifact {
        version: CANON_ENTITY_PROMOTION_SIDECAR_VERSION.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        summary: sidecar_summary(
            escrow_entities.len(),
            contradiction_entities.len(),
            cannot_link_facts.len(),
            relation_hints.len(),
        ),
        source_audit_hash: request.source_audit_hash.clone(),
        source_decision_ledger_hash: request.source_decision_ledger_hash,
        escrow_entities,
        contradiction_entities,
        cannot_link_facts,
        relation_hints,
    };
    sidecar.artifact_content_hash = hash_sidecar_without_self(&sidecar)?;
    sidecar.metadata.artifact_content_hash = sidecar.artifact_content_hash.clone();

    let mut proof_metadata = sidecar.metadata.clone();
    proof_metadata.artifact_content_hash.clear();
    proof_metadata.upstream_artifacts = vec![
        EntityArtifactReference {
            version: CANON_ENTITY_AUDIT_VERSION.to_string(),
            content_hash: request.source_audit_hash.clone(),
        },
        EntityArtifactReference {
            version: CANON_ENTITY_PROMOTION_SIDECAR_VERSION.to_string(),
            content_hash: sidecar.artifact_content_hash.clone(),
        },
    ];
    let mut proof = EntityPromotionProofArtifact {
        version: CANON_ENTITY_PROMOTION_PROOF_VERSION.to_string(),
        artifact_content_hash: String::new(),
        metadata: proof_metadata,
        summary: proof_summary(request.promoted_alias_count, &sidecar),
        source_audit_hash: request.source_audit_hash,
        sidecar_snapshot_hash: sidecar.artifact_content_hash.clone(),
        registry_snapshot_hash: sidecar
            .metadata
            .registry_snapshot
            .lookup_snapshot_hash
            .clone(),
    };
    proof.artifact_content_hash = hash_proof_without_self(&proof)?;
    proof.metadata.artifact_content_hash = proof.artifact_content_hash.clone();

    Ok(EntityPromotionSidecarArtifacts { sidecar, proof })
}

pub fn write_promotion_sidecar_artifacts(
    output_dir: &Path,
    artifacts: &EntityPromotionSidecarArtifacts,
) -> Result<EntityPromotionSidecarWriteReceipt, Refusal> {
    fs::create_dir_all(output_dir).map_err(|error| sidecar_io_refusal(output_dir, error))?;
    let sidecar_path = output_dir.join("promotion-sidecars.json");
    let proof_path = output_dir.join("promotion-proof.json");
    let sidecar_bytes = to_pretty_bytes(&artifacts.sidecar)?;
    let proof_bytes = to_pretty_bytes(&artifacts.proof)?;
    write_atomic(&sidecar_path, &sidecar_bytes)
        .map_err(|error| sidecar_io_refusal(&sidecar_path, error))?;
    if let Err(error) = write_atomic(&proof_path, &proof_bytes) {
        let _ = fs::remove_file(&sidecar_path);
        return Err(sidecar_io_refusal(&proof_path, error));
    }
    Ok(EntityPromotionSidecarWriteReceipt {
        sidecar_path,
        proof_path,
        sidecar_snapshot_hash: artifacts.sidecar.artifact_content_hash.clone(),
        proof_hash: artifacts.proof.artifact_content_hash.clone(),
        bytes_written: (sidecar_bytes.len() + proof_bytes.len()) as u64,
    })
}

fn validate_sidecar_request(request: &EntityPromotionSidecarRequest) -> Result<(), Refusal> {
    if !request.metadata.profile.is_complete() {
        return Err(sidecar_refusal(
            "Promotion sidecars require complete profile metadata",
            json!({
                "stage": "promote",
                "field": "metadata.profile",
                "writes_performed": false
            }),
        ));
    }
    let profile_id = request.metadata.profile.id.as_str();
    let identity_semantics = request.metadata.profile.identity_semantics.as_str();
    for cannot_link in &request.patch_bundle.cannot_link_sidecars {
        validate_record_scope(
            "cannot_link",
            &cannot_link.profile_id,
            &cannot_link.identity_semantics,
            profile_id,
            identity_semantics,
        )?;
    }
    for relation in &request.patch_bundle.relation_patches {
        validate_record_scope(
            "relation",
            &relation.profile_id,
            &relation.identity_semantics,
            profile_id,
            identity_semantics,
        )?;
    }
    for escrow in &request.escrow_entities {
        validate_record_scope(
            "escrow",
            &escrow.profile_id,
            &escrow.identity_semantics,
            profile_id,
            identity_semantics,
        )?;
    }
    for contradiction in &request.contradiction_entities {
        validate_record_scope(
            "contradiction",
            &contradiction.profile_id,
            &contradiction.identity_semantics,
            profile_id,
            identity_semantics,
        )?;
    }
    if request
        .metadata
        .registry_snapshot
        .lookup_snapshot_hash
        .trim()
        .is_empty()
        || request.source_audit_hash.trim().is_empty()
        || request.source_decision_ledger_hash.trim().is_empty()
    {
        return Err(sidecar_refusal(
            "Promotion sidecars require registry, audit, and ledger hashes",
            json!({
                "stage": "promote",
                "field": "hashes",
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn validate_record_scope(
    record_kind: &str,
    actual_profile_id: &str,
    actual_identity_semantics: &str,
    expected_profile_id: &str,
    expected_identity_semantics: &str,
) -> Result<(), Refusal> {
    if actual_profile_id == expected_profile_id
        && actual_identity_semantics == expected_identity_semantics
    {
        Ok(())
    } else {
        Err(sidecar_refusal(
            "Promotion sidecar record crossed the profile firewall",
            json!({
                "stage": "promote",
                "field": "profile_scope",
                "record_kind": record_kind,
                "expected_profile_id": expected_profile_id,
                "actual_profile_id": actual_profile_id,
                "expected_identity_semantics": expected_identity_semantics,
                "actual_identity_semantics": actual_identity_semantics,
                "writes_performed": false
            }),
        ))
    }
}

fn sidecar_summary(
    escrow_count: usize,
    contradiction_count: usize,
    cannot_link_count: usize,
    relation_hint_count: usize,
) -> EntityDeterministicSummary {
    EntityDeterministicSummary {
        counts: BTreeMap::from([
            ("escrow_count".to_string(), escrow_count as u64),
            (
                "contradiction_count".to_string(),
                contradiction_count as u64,
            ),
            ("cannot_link_count".to_string(), cannot_link_count as u64),
            (
                "relation_hint_count".to_string(),
                relation_hint_count as u64,
            ),
        ]),
        labels: BTreeMap::from([("scope".to_string(), "profile_registry_snapshot".to_string())]),
    }
}

fn proof_summary(
    promoted_alias_count: u64,
    sidecar: &EntityPromotionSidecarArtifact,
) -> EntityDeterministicSummary {
    let sidecar_record_count = sidecar.summary.counts.values().sum::<u64>();
    EntityDeterministicSummary {
        counts: BTreeMap::from([
            ("promoted_alias_count".to_string(), promoted_alias_count),
            ("sidecar_record_count".to_string(), sidecar_record_count),
        ]),
        labels: BTreeMap::from([("status".to_string(), "promoted".to_string())]),
    }
}

fn hash_sidecar_without_self(artifact: &EntityPromotionSidecarArtifact) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        sidecar_refusal(
            "Failed to hash promotion sidecar artifact",
            json!({
                "stage": "promote",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn hash_proof_without_self(artifact: &EntityPromotionProofArtifact) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        sidecar_refusal(
            "Failed to hash promotion proof artifact",
            json!({
                "stage": "promote",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn to_pretty_bytes<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, Refusal> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        sidecar_refusal(
            "Failed to serialize promotion sidecar artifact",
            json!({
                "stage": "promote",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temp_path = path.with_extension("tmp");
    if let Err(error) = fs::write(&temp_path, bytes) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    Ok(())
}

fn sidecar_refusal(message: &'static str, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        detail,
        Some("canon entity promote <RESULT.json> --audit <AUDIT.json>".to_string()),
    )
}

fn sidecar_io_refusal(path: &Path, error: std::io::Error) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        format!(
            "Entity promotion could not write sidecar {}",
            path.display()
        ),
        json!({
            "stage": "promote",
            "path": path.display().to_string(),
            "error": error.to_string(),
            "writes_performed": false
        }),
        Some("Check output permissions and rerun canon entity promote".to_string()),
    )
}
