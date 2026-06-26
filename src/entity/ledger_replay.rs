#![forbid(unsafe_code)]

//! Deterministic decision-ledger replay into derived patch-sidecar records.

use crate::{
    Refusal,
    entity::{
        EntityDeterministicSummary,
        error::EntityRefusalKind,
        ledger::{
            DECISION_LEDGER_EVENT_VERSION, DecisionLedgerEvent, DecisionLedgerEventType,
            DecisionLedgerExpectedContext, validate_decision_ledger_event,
            validate_decision_ledger_event_context,
        },
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionLedgerReplayRequest {
    pub expected_context: DecisionLedgerExpectedContext,
    pub starting_previous_event_hash: String,
    pub events: Vec<DecisionLedgerEvent>,
    pub cannot_link_override_decision_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionLedgerReplayReport {
    pub summary: EntityDeterministicSummary,
    pub derived_patches: Vec<LedgerDerivedPatch>,
    pub replay_proofs: Vec<LedgerReplayProof>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerDerivedPatchKind {
    Alias,
    Distinct,
    CannotLink,
    Relation,
    OverrideProof,
    Promotion,
    Revert,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerDerivedPatch {
    pub kind: LedgerDerivedPatchKind,
    pub decision_id: String,
    pub event_hash: String,
    pub profile_id: String,
    pub identity_semantics: String,
    pub source_artifact_hash: String,
    pub surface_ids: Vec<String>,
    pub entity_ids: Vec<String>,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerReplayProof {
    pub decision_id: String,
    pub event_hash: String,
    pub previous_event_hash: String,
    pub event_type: DecisionLedgerEventType,
    pub idempotency_key: String,
}

pub fn parse_decision_ledger_jsonl(input: &str) -> Result<Vec<DecisionLedgerEvent>, Refusal> {
    input
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(line_index, line)| {
            serde_json::from_str::<DecisionLedgerEvent>(line).map_err(|error| {
                replay_refusal(
                    EntityRefusalKind::ReviewImport,
                    "Decision ledger JSONL is malformed",
                    json!({
                        "stage": "decision_ledger_replay",
                        "field": "ledger_jsonl",
                        "line": line_index + 1,
                        "error": error.to_string(),
                        "writes_performed": false
                    }),
                )
            })
        })
        .collect()
}

pub fn replay_decision_ledger(
    request: DecisionLedgerReplayRequest,
) -> Result<DecisionLedgerReplayReport, Refusal> {
    if request.starting_previous_event_hash.trim().is_empty() {
        return Err(replay_refusal(
            EntityRefusalKind::ReviewImport,
            "Decision ledger replay requires a starting previous event hash",
            json!({
                "stage": "decision_ledger_replay",
                "field": "starting_previous_event_hash",
                "writes_performed": false
            }),
        ));
    }

    let mut previous_event_hash = request.starting_previous_event_hash;
    let mut seen_decisions = BTreeMap::<String, String>::new();
    let mut idempotent_duplicate_count = 0u64;
    let mut derived_patches = Vec::new();
    let mut replay_proofs = Vec::new();

    for event in request.events {
        validate_event_for_replay(&event, &request.expected_context, &previous_event_hash)?;
        if request
            .cannot_link_override_decision_ids
            .contains(&event.decision_id)
            && !has_override_provenance(&event)
        {
            return Err(replay_refusal(
                EntityRefusalKind::CannotLinkOverride,
                "Decision ledger replay cannot apply a cannot-link override without approved provenance",
                json!({
                    "stage": "decision_ledger_replay",
                    "field": "override_provenance",
                    "decision_id": event.decision_id,
                    "writes_performed": false
                }),
            ));
        }

        if let Some(existing_hash) = seen_decisions.get(&event.decision_id) {
            if existing_hash == &event.event_hash {
                idempotent_duplicate_count = idempotent_duplicate_count.saturating_add(1);
                previous_event_hash = event.event_hash.clone();
                continue;
            }
            return Err(replay_refusal(
                EntityRefusalKind::PatchConflict,
                "Decision ledger replay found a conflicting idempotency key",
                json!({
                    "stage": "decision_ledger_replay",
                    "field": "decision_id",
                    "decision_id": event.decision_id,
                    "existing_event_hash": existing_hash,
                    "actual_event_hash": event.event_hash,
                    "writes_performed": false
                }),
            ));
        }
        seen_decisions.insert(event.decision_id.clone(), event.event_hash.clone());

        let mut event_patches = derived_patches_for_event(&event);
        derived_patches.append(&mut event_patches);
        replay_proofs.push(LedgerReplayProof {
            decision_id: event.decision_id.clone(),
            event_hash: event.event_hash.clone(),
            previous_event_hash: event.previous_event_hash.clone(),
            event_type: event.event_type,
            idempotency_key: event.decision_id.clone(),
        });
        previous_event_hash = event.event_hash;
    }
    derived_patches.sort_by(derived_patch_cmp);
    replay_proofs.sort_by(replay_proof_cmp);

    Ok(DecisionLedgerReplayReport {
        summary: replay_summary(&derived_patches, &replay_proofs, idempotent_duplicate_count),
        derived_patches,
        replay_proofs,
    })
}

fn validate_event_for_replay(
    event: &DecisionLedgerEvent,
    expected_context: &DecisionLedgerExpectedContext,
    expected_previous_event_hash: &str,
) -> Result<(), Refusal> {
    if event.event_version != DECISION_LEDGER_EVENT_VERSION {
        return Err(replay_refusal(
            EntityRefusalKind::ReviewImport,
            "Decision ledger event has an unknown event version",
            json!({
                "stage": "decision_ledger_replay",
                "field": "event_version",
                "expected": DECISION_LEDGER_EVENT_VERSION,
                "actual": event.event_version,
                "decision_id": event.decision_id,
                "writes_performed": false
            }),
        ));
    }
    validate_decision_ledger_event(event)?;
    validate_decision_ledger_event_context(event, expected_context)?;
    if event.previous_event_hash != expected_previous_event_hash {
        return Err(replay_refusal(
            EntityRefusalKind::ReviewImport,
            "Decision ledger event chain is not continuous",
            json!({
                "stage": "decision_ledger_replay",
                "field": "previous_event_hash",
                "decision_id": event.decision_id,
                "expected": expected_previous_event_hash,
                "actual": event.previous_event_hash,
                "writes_performed": false
            }),
        ));
    }
    if event.event_type == DecisionLedgerEventType::PromotionReverted
        && !event.note.contains("revert_of_event_hash=blake3:")
    {
        return Err(replay_refusal(
            EntityRefusalKind::ReviewImport,
            "Promotion revert events require an explicit revert proof",
            json!({
                "stage": "decision_ledger_replay",
                "field": "note",
                "decision_id": event.decision_id,
                "required": "revert_of_event_hash=blake3:<hash>",
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn derived_patches_for_event(event: &DecisionLedgerEvent) -> Vec<LedgerDerivedPatch> {
    let kinds = match event.event_type {
        DecisionLedgerEventType::MergeConfirmed | DecisionLedgerEventType::AliasPatchAdded => {
            vec![LedgerDerivedPatchKind::Alias]
        }
        DecisionLedgerEventType::DistinctConfirmed | DecisionLedgerEventType::CannotLinkAdded => {
            vec![
                LedgerDerivedPatchKind::Distinct,
                LedgerDerivedPatchKind::CannotLink,
            ]
        }
        DecisionLedgerEventType::RelationConfirmed => vec![LedgerDerivedPatchKind::Relation],
        DecisionLedgerEventType::OperatorOverrideRequested
        | DecisionLedgerEventType::OperatorOverrideApproved => {
            vec![LedgerDerivedPatchKind::OverrideProof]
        }
        DecisionLedgerEventType::PromotionApplied => vec![LedgerDerivedPatchKind::Promotion],
        DecisionLedgerEventType::PromotionReverted => vec![LedgerDerivedPatchKind::Revert],
    };
    kinds
        .into_iter()
        .map(|kind| LedgerDerivedPatch {
            kind,
            decision_id: event.decision_id.clone(),
            event_hash: event.event_hash.clone(),
            profile_id: event.metadata.profile.id.clone(),
            identity_semantics: event.metadata.profile.identity_semantics.clone(),
            source_artifact_hash: event.source_artifact_hash.clone(),
            surface_ids: surface_ids_from_refs(event),
            entity_ids: entity_ids_from_refs(event),
            reason_code: event.reason_code.clone(),
        })
        .collect()
}

fn has_override_provenance(event: &DecisionLedgerEvent) -> bool {
    event.note.contains("override_approved_by=") && event.note.contains("override_reason_code=")
}

fn surface_ids_from_refs(event: &DecisionLedgerEvent) -> Vec<String> {
    let mut surface_ids = event.refs.surface_ids.clone();
    if let Some(surface_id) = &event.refs.left_surface_id {
        surface_ids.push(surface_id.clone());
    }
    if let Some(surface_id) = &event.refs.right_surface_id {
        surface_ids.push(surface_id.clone());
    }
    surface_ids.sort();
    surface_ids.dedup();
    surface_ids
}

fn entity_ids_from_refs(event: &DecisionLedgerEvent) -> Vec<String> {
    let mut entity_ids = Vec::new();
    if let Some(entity_id) = &event.refs.entity_id {
        entity_ids.push(entity_id.clone());
    }
    if let Some(entity_id) = &event.refs.left_entity_id {
        entity_ids.push(entity_id.clone());
    }
    if let Some(entity_id) = &event.refs.right_entity_id {
        entity_ids.push(entity_id.clone());
    }
    entity_ids.sort();
    entity_ids.dedup();
    entity_ids
}

fn replay_summary(
    derived_patches: &[LedgerDerivedPatch],
    replay_proofs: &[LedgerReplayProof],
    idempotent_duplicate_count: u64,
) -> EntityDeterministicSummary {
    EntityDeterministicSummary {
        counts: BTreeMap::from([
            ("events_replayed".to_string(), replay_proofs.len() as u64),
            (
                "derived_patch_count".to_string(),
                derived_patches.len() as u64,
            ),
            (
                "alias_patch_count".to_string(),
                count_kind(derived_patches, LedgerDerivedPatchKind::Alias),
            ),
            (
                "distinct_patch_count".to_string(),
                count_kind(derived_patches, LedgerDerivedPatchKind::Distinct),
            ),
            (
                "cannot_link_patch_count".to_string(),
                count_kind(derived_patches, LedgerDerivedPatchKind::CannotLink),
            ),
            (
                "relation_patch_count".to_string(),
                count_kind(derived_patches, LedgerDerivedPatchKind::Relation),
            ),
            (
                "override_proof_count".to_string(),
                count_kind(derived_patches, LedgerDerivedPatchKind::OverrideProof),
            ),
            (
                "promotion_revert_count".to_string(),
                count_kind(derived_patches, LedgerDerivedPatchKind::Revert),
            ),
            (
                "idempotent_duplicate_count".to_string(),
                idempotent_duplicate_count,
            ),
        ]),
        labels: BTreeMap::from([("replay".to_string(), "deterministic".to_string())]),
    }
}

fn count_kind(derived_patches: &[LedgerDerivedPatch], kind: LedgerDerivedPatchKind) -> u64 {
    derived_patches
        .iter()
        .filter(|patch| patch.kind == kind)
        .count() as u64
}

fn derived_patch_cmp(left: &LedgerDerivedPatch, right: &LedgerDerivedPatch) -> std::cmp::Ordering {
    left.kind
        .cmp(&right.kind)
        .then_with(|| left.decision_id.cmp(&right.decision_id))
        .then_with(|| left.surface_ids.cmp(&right.surface_ids))
}

fn replay_proof_cmp(left: &LedgerReplayProof, right: &LedgerReplayProof) -> std::cmp::Ordering {
    left.decision_id
        .cmp(&right.decision_id)
        .then_with(|| left.event_hash.cmp(&right.event_hash))
}

fn replay_refusal(
    kind: EntityRefusalKind,
    message: &'static str,
    detail: serde_json::Value,
) -> Refusal {
    kind.to_refusal(
        message,
        detail,
        Some("canon entity review import <REVIEW.json|csv> --registry <REGISTRY_DIR>".to_string()),
    )
}
