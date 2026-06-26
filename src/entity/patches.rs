#![forbid(unsafe_code)]

//! Derive profile-scoped patch and sidecar records from reviewed ledger replay.

use crate::{
    Refusal,
    entity::{
        error::EntityRefusalKind,
        ledger_replay::{DecisionLedgerReplayReport, LedgerDerivedPatch, LedgerDerivedPatchKind},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReviewPatchBundle {
    pub alias_patches: Vec<AliasReviewPatch>,
    pub distinct_patches: Vec<DistinctReviewPatch>,
    pub relation_patches: Vec<RelationReviewPatch>,
    pub cannot_link_sidecars: Vec<CannotLinkSidecarRecord>,
    pub override_records: Vec<OverrideReviewRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasReviewPatch {
    pub patch_id: String,
    pub profile_id: String,
    pub identity_semantics: String,
    pub namespace: String,
    pub canonical_hint: String,
    pub inputs: Vec<String>,
    pub review_decision_id: String,
    pub source_event_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistinctReviewPatch {
    pub patch_id: String,
    pub profile_id: String,
    pub identity_semantics: String,
    pub namespace: String,
    pub left: String,
    pub right: String,
    pub reason: String,
    pub review_decision_id: String,
    pub source_event_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationReviewPatch {
    pub patch_id: String,
    pub profile_id: String,
    pub identity_semantics: String,
    pub namespace: String,
    pub left: String,
    pub right: String,
    pub relation: String,
    pub review_decision_id: String,
    pub source_event_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CannotLinkSidecarRecord {
    pub sidecar_id: String,
    pub profile_id: String,
    pub identity_semantics: String,
    pub left: String,
    pub right: String,
    pub hard_cannot_link: bool,
    pub reason: String,
    pub review_decision_id: String,
    pub source_event_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverrideReviewRecord {
    pub override_id: String,
    pub profile_id: String,
    pub identity_semantics: String,
    pub surface_ids: Vec<String>,
    pub reason: String,
    pub review_decision_id: String,
    pub source_event_hash: String,
}

pub fn derive_review_patches(
    report: &DecisionLedgerReplayReport,
) -> Result<ReviewPatchBundle, Refusal> {
    let mut bundle = ReviewPatchBundle::default();
    for patch in &report.derived_patches {
        match patch.kind {
            LedgerDerivedPatchKind::Alias => {
                bundle.alias_patches.push(alias_patch(patch)?);
            }
            LedgerDerivedPatchKind::Distinct => {
                bundle.distinct_patches.push(distinct_patch(patch)?);
            }
            LedgerDerivedPatchKind::CannotLink => {
                bundle
                    .cannot_link_sidecars
                    .push(cannot_link_sidecar(patch)?);
            }
            LedgerDerivedPatchKind::Relation => {
                bundle.relation_patches.push(relation_patch(patch)?);
            }
            LedgerDerivedPatchKind::OverrideProof => {
                bundle.override_records.push(override_record(patch)?);
            }
            LedgerDerivedPatchKind::Promotion | LedgerDerivedPatchKind::Revert => {}
        }
    }
    sort_bundle(&mut bundle);
    validate_patch_contradictions(&bundle)?;
    Ok(bundle)
}

fn alias_patch(patch: &LedgerDerivedPatch) -> Result<AliasReviewPatch, Refusal> {
    require_surfaces(patch, 1)?;
    Ok(AliasReviewPatch {
        patch_id: patch_id("alias", patch),
        profile_id: patch.profile_id.clone(),
        identity_semantics: patch.identity_semantics.clone(),
        namespace: format!("{}.aliases", patch.profile_id),
        canonical_hint: patch
            .entity_ids
            .first()
            .cloned()
            .unwrap_or_else(|| patch.decision_id.clone()),
        inputs: patch.surface_ids.clone(),
        review_decision_id: patch.decision_id.clone(),
        source_event_hash: patch.event_hash.clone(),
    })
}

fn distinct_patch(patch: &LedgerDerivedPatch) -> Result<DistinctReviewPatch, Refusal> {
    let (left, right) = surface_pair(patch)?;
    Ok(DistinctReviewPatch {
        patch_id: patch_id("distinct", patch),
        profile_id: patch.profile_id.clone(),
        identity_semantics: patch.identity_semantics.clone(),
        namespace: format!("{}.distinct", patch.profile_id),
        left,
        right,
        reason: patch.reason_code.clone(),
        review_decision_id: patch.decision_id.clone(),
        source_event_hash: patch.event_hash.clone(),
    })
}

fn relation_patch(patch: &LedgerDerivedPatch) -> Result<RelationReviewPatch, Refusal> {
    let (left, right) = surface_pair(patch)?;
    Ok(RelationReviewPatch {
        patch_id: patch_id("relation", patch),
        profile_id: patch.profile_id.clone(),
        identity_semantics: patch.identity_semantics.clone(),
        namespace: format!("{}.relations", patch.profile_id),
        left,
        right,
        relation: patch.reason_code.clone(),
        review_decision_id: patch.decision_id.clone(),
        source_event_hash: patch.event_hash.clone(),
    })
}

fn cannot_link_sidecar(patch: &LedgerDerivedPatch) -> Result<CannotLinkSidecarRecord, Refusal> {
    let (left, right) = surface_pair(patch)?;
    Ok(CannotLinkSidecarRecord {
        sidecar_id: patch_id("cannot_link", patch),
        profile_id: patch.profile_id.clone(),
        identity_semantics: patch.identity_semantics.clone(),
        left,
        right,
        hard_cannot_link: true,
        reason: patch.reason_code.clone(),
        review_decision_id: patch.decision_id.clone(),
        source_event_hash: patch.event_hash.clone(),
    })
}

fn override_record(patch: &LedgerDerivedPatch) -> Result<OverrideReviewRecord, Refusal> {
    require_surfaces(patch, 1)?;
    Ok(OverrideReviewRecord {
        override_id: patch_id("override", patch),
        profile_id: patch.profile_id.clone(),
        identity_semantics: patch.identity_semantics.clone(),
        surface_ids: patch.surface_ids.clone(),
        reason: patch.reason_code.clone(),
        review_decision_id: patch.decision_id.clone(),
        source_event_hash: patch.event_hash.clone(),
    })
}

fn validate_patch_contradictions(bundle: &ReviewPatchBundle) -> Result<(), Refusal> {
    let mut alias_pairs = BTreeMap::<PatchPairKey, String>::new();
    for alias in &bundle.alias_patches {
        for pair in all_surface_pairs(&alias.inputs) {
            alias_pairs.insert(
                PatchPairKey::new(
                    &alias.profile_id,
                    &alias.identity_semantics,
                    &pair.0,
                    &pair.1,
                ),
                alias.review_decision_id.clone(),
            );
        }
    }

    for distinct in &bundle.distinct_patches {
        let key = PatchPairKey::new(
            &distinct.profile_id,
            &distinct.identity_semantics,
            &distinct.left,
            &distinct.right,
        );
        if let Some(alias_decision_id) = alias_pairs.get(&key) {
            return Err(patch_conflict_refusal(
                "alias_distinct_conflict",
                alias_decision_id,
                &distinct.review_decision_id,
                &key,
            ));
        }
    }
    for cannot_link in &bundle.cannot_link_sidecars {
        let key = PatchPairKey::new(
            &cannot_link.profile_id,
            &cannot_link.identity_semantics,
            &cannot_link.left,
            &cannot_link.right,
        );
        if let Some(alias_decision_id) = alias_pairs.get(&key) {
            return Err(patch_conflict_refusal(
                "alias_cannot_link_conflict",
                alias_decision_id,
                &cannot_link.review_decision_id,
                &key,
            ));
        }
    }
    Ok(())
}

fn require_surfaces(patch: &LedgerDerivedPatch, minimum: usize) -> Result<(), Refusal> {
    if patch.surface_ids.len() >= minimum {
        Ok(())
    } else {
        Err(patch_refusal(
            EntityRefusalKind::PatchConflict,
            "Review patch derivation requires surface references",
            json!({
                "stage": "review_patch_derivation",
                "field": "surface_ids",
                "decision_id": patch.decision_id
            }),
        ))
    }
}

fn surface_pair(patch: &LedgerDerivedPatch) -> Result<(String, String), Refusal> {
    require_surfaces(patch, 2)?;
    let mut surface_ids = patch.surface_ids.clone();
    surface_ids.sort();
    surface_ids.dedup();
    Ok((surface_ids[0].clone(), surface_ids[1].clone()))
}

fn all_surface_pairs(surface_ids: &[String]) -> Vec<(String, String)> {
    let mut ids = surface_ids.to_vec();
    ids.sort();
    ids.dedup();
    let mut pairs = Vec::new();
    for left_index in 0..ids.len() {
        for right_index in (left_index + 1)..ids.len() {
            pairs.push((ids[left_index].clone(), ids[right_index].clone()));
        }
    }
    pairs
}

fn patch_id(prefix: &str, patch: &LedgerDerivedPatch) -> String {
    format!(
        "{}:{}:{}",
        prefix,
        patch.profile_id,
        patch
            .decision_id
            .strip_prefix("decision:")
            .unwrap_or(&patch.decision_id)
    )
}

fn sort_bundle(bundle: &mut ReviewPatchBundle) {
    bundle
        .alias_patches
        .sort_by(|left, right| left.patch_id.cmp(&right.patch_id));
    bundle
        .distinct_patches
        .sort_by(|left, right| left.patch_id.cmp(&right.patch_id));
    bundle
        .relation_patches
        .sort_by(|left, right| left.patch_id.cmp(&right.patch_id));
    bundle
        .cannot_link_sidecars
        .sort_by(|left, right| left.sidecar_id.cmp(&right.sidecar_id));
    bundle
        .override_records
        .sort_by(|left, right| left.override_id.cmp(&right.override_id));
}

fn patch_conflict_refusal(
    reason: &'static str,
    alias_decision_id: &str,
    negative_decision_id: &str,
    key: &PatchPairKey,
) -> Refusal {
    patch_refusal(
        EntityRefusalKind::PatchConflict,
        "Review-derived patches contain contradictory same-profile knowledge",
        json!({
            "stage": "review_patch_derivation",
            "reason": reason,
            "profile_id": key.profile_id,
            "identity_semantics": key.identity_semantics,
            "left": key.left,
            "right": key.right,
            "alias_review_decision_id": alias_decision_id,
            "negative_review_decision_id": negative_decision_id
        }),
    )
}

fn patch_refusal(
    kind: EntityRefusalKind,
    message: &'static str,
    detail: serde_json::Value,
) -> Refusal {
    kind.to_refusal(
        message,
        detail,
        Some(
            "Review the conflicting alias/distinct/relation decisions before promotion".to_string(),
        ),
    )
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PatchPairKey {
    profile_id: String,
    identity_semantics: String,
    left: String,
    right: String,
}

impl PatchPairKey {
    fn new(profile_id: &str, identity_semantics: &str, left: &str, right: &str) -> Self {
        let (left, right) = if left <= right {
            (left.to_string(), right.to_string())
        } else {
            (right.to_string(), left.to_string())
        };
        Self {
            profile_id: profile_id.to_string(),
            identity_semantics: identity_semantics.to_string(),
            left,
            right,
        }
    }
}

pub fn cannot_link_scope_keys(
    bundle: &ReviewPatchBundle,
) -> BTreeSet<(String, String, String, String)> {
    bundle
        .cannot_link_sidecars
        .iter()
        .map(|record| {
            (
                record.profile_id.clone(),
                record.identity_semantics.clone(),
                record.left.clone(),
                record.right.clone(),
            )
        })
        .collect()
}
