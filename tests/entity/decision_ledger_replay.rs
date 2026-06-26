#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        EntityArtifactMetadata, EntityArtifactReference, EntityInputReference,
        EntityPatchNamespaces, EntityProfileReference, EntityRegistrySnapshot,
        EntityStrategyReference,
        ledger::{
            DecisionLedgerEvent, DecisionLedgerEventInput, DecisionLedgerEventType,
            DecisionLedgerExpectedContext, DecisionLedgerRefs, build_decision_ledger_event,
        },
        ledger_replay::{
            DecisionLedgerReplayRequest, LedgerDerivedPatchKind, parse_decision_ledger_jsonl,
            replay_decision_ledger,
        },
    },
};
use serde::Deserialize;
use std::{collections::BTreeSet, fs, path::PathBuf};

#[test]
#[allow(non_snake_case)]
fn G09_decision_ledger_replay_derives_patch_records_deterministically() {
    let events = replay_events("blake3:review-queue", "blake3:previous-event");
    let jsonl = events
        .iter()
        .map(|event| serde_json::to_string(event).expect("event serializes"))
        .collect::<Vec<_>>()
        .join("\n");
    let parsed = parse_decision_ledger_jsonl(&jsonl).expect("jsonl parses");
    let report = replay_decision_ledger(DecisionLedgerReplayRequest {
        expected_context: expected_context("blake3:review-queue"),
        starting_previous_event_hash: "blake3:previous-event".to_string(),
        events: parsed,
        cannot_link_override_decision_ids: BTreeSet::new(),
    })
    .expect("ledger replays");

    assert_eq!(report.summary, expected_summary());
    assert_eq!(report.derived_patches.len(), 4);
    assert!(
        report
            .derived_patches
            .iter()
            .any(|patch| patch.kind == LedgerDerivedPatchKind::Alias)
    );
    assert!(
        report
            .derived_patches
            .iter()
            .any(|patch| patch.kind == LedgerDerivedPatchKind::CannotLink)
    );
    assert!(
        report
            .derived_patches
            .iter()
            .any(|patch| patch.kind == LedgerDerivedPatchKind::Relation)
    );

    let replayed_again = replay_decision_ledger(DecisionLedgerReplayRequest {
        expected_context: expected_context("blake3:review-queue"),
        starting_previous_event_hash: "blake3:previous-event".to_string(),
        events,
        cannot_link_override_decision_ids: BTreeSet::new(),
    })
    .expect("ledger replays again");
    assert_eq!(
        serde_json::to_vec(&report).expect("report serializes"),
        serde_json::to_vec(&replayed_again).expect("second report serializes")
    );
}

#[test]
fn decision_ledger_mixed_run_refusal_rejects_stale_source_hash() {
    let events = replay_events("blake3:other-review-queue", "blake3:previous-event");
    let refusal = replay_decision_ledger(DecisionLedgerReplayRequest {
        expected_context: expected_context("blake3:review-queue"),
        starting_previous_event_hash: "blake3:previous-event".to_string(),
        events,
        cannot_link_override_decision_ids: BTreeSet::new(),
    })
    .expect_err("mixed source hash refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["field"], "source_artifact_hash");
    assert_eq!(refusal.detail["expected"], "blake3:review-queue");
    assert_eq!(refusal.detail["actual"], "blake3:other-review-queue");
}

#[test]
fn decision_ledger_unknown_event_version_refuses() {
    let mut events = replay_events("blake3:review-queue", "blake3:previous-event");
    events[0].event_version = "decision_event.v99".to_string();
    let refusal = replay_decision_ledger(DecisionLedgerReplayRequest {
        expected_context: expected_context("blake3:review-queue"),
        starting_previous_event_hash: "blake3:previous-event".to_string(),
        events,
        cannot_link_override_decision_ids: BTreeSet::new(),
    })
    .expect_err("unknown version refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["field"], "event_version");
}

#[test]
fn decision_ledger_revert_proof_is_required() {
    let base = replay_events("blake3:review-queue", "blake3:previous-event");
    let missing_proof = ledger_event(
        DecisionLedgerEventType::PromotionReverted,
        DecisionLedgerRefs::entity_surfaces("TNT-SEARS", vec!["surf:sears".to_string()]),
        "blake3:review-queue",
        &base[2].event_hash,
        "promotion_reverted",
        "operator reverted promotion",
    );
    let refusal = replay_decision_ledger(DecisionLedgerReplayRequest {
        expected_context: expected_context("blake3:review-queue"),
        starting_previous_event_hash: base[2].event_hash.clone(),
        events: vec![missing_proof],
        cannot_link_override_decision_ids: BTreeSet::new(),
    })
    .expect_err("missing revert proof refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["field"], "note");

    let with_proof = ledger_event(
        DecisionLedgerEventType::PromotionReverted,
        DecisionLedgerRefs::entity_surfaces("TNT-SEARS", vec!["surf:sears".to_string()]),
        "blake3:review-queue",
        &base[2].event_hash,
        "promotion_reverted",
        &format!(
            "revert_of_event_hash={} operator reverted promotion",
            base[2].event_hash
        ),
    );
    let report = replay_decision_ledger(DecisionLedgerReplayRequest {
        expected_context: expected_context("blake3:review-queue"),
        starting_previous_event_hash: base[2].event_hash.clone(),
        events: vec![with_proof],
        cannot_link_override_decision_ids: BTreeSet::new(),
    })
    .expect("revert proof replays");
    assert_eq!(report.summary.counts["promotion_revert_count"], 1);
}

#[test]
#[allow(non_snake_case)]
fn E_ENTITY_CANNOT_LINK_OVERRIDE_replay_requires_approved_provenance() {
    let merge = ledger_event(
        DecisionLedgerEventType::MergeConfirmed,
        DecisionLedgerRefs::surface_pair("surf:sears", "surf:sears_auto"),
        "blake3:review-queue",
        "blake3:previous-event",
        "review_merge_confirmed",
        "operator attempted hard cannot-link override",
    );
    let refusal = replay_decision_ledger(DecisionLedgerReplayRequest {
        expected_context: expected_context("blake3:review-queue"),
        starting_previous_event_hash: "blake3:previous-event".to_string(),
        events: vec![merge.clone()],
        cannot_link_override_decision_ids: BTreeSet::from([merge.decision_id.clone()]),
    })
    .expect_err("missing override proof refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityCannotLinkOverride);
    assert_eq!(refusal.detail["field"], "override_provenance");

    let approved = ledger_event(
        DecisionLedgerEventType::MergeConfirmed,
        DecisionLedgerRefs::surface_pair("surf:sears", "surf:sears_auto"),
        "blake3:review-queue",
        "blake3:previous-event",
        "review_merge_confirmed",
        "override_approved_by=principal@example.com override_reason_code=approved_hard_cannot_link_override",
    );
    let report = replay_decision_ledger(DecisionLedgerReplayRequest {
        expected_context: expected_context("blake3:review-queue"),
        starting_previous_event_hash: "blake3:previous-event".to_string(),
        events: vec![approved.clone()],
        cannot_link_override_decision_ids: BTreeSet::from([approved.decision_id.clone()]),
    })
    .expect("approved override replays");
    assert_eq!(report.summary.counts["alias_patch_count"], 1);
}

fn replay_events(source_artifact_hash: &str, start_hash: &str) -> Vec<DecisionLedgerEvent> {
    let first = ledger_event(
        DecisionLedgerEventType::MergeConfirmed,
        DecisionLedgerRefs::surface_pair("surf:sears", "surf:sears_llc"),
        source_artifact_hash,
        start_hash,
        "review_merge_confirmed",
        "same tenant display label",
    );
    let second = ledger_event(
        DecisionLedgerEventType::DistinctConfirmed,
        DecisionLedgerRefs::surface_pair("surf:sears", "surf:sears_auto"),
        source_artifact_hash,
        &first.event_hash,
        "review_distinct_confirmed",
        "operator confirmed related but distinct tenant labels",
    );
    let third = ledger_event(
        DecisionLedgerEventType::RelationConfirmed,
        DecisionLedgerRefs::surface_pair("surf:sears", "surf:kmart"),
        source_artifact_hash,
        &second.event_hash,
        "review_relation_confirmed",
        "same brand family relation only",
    );
    vec![first, second, third]
}

fn ledger_event(
    event_type: DecisionLedgerEventType,
    refs: DecisionLedgerRefs,
    source_artifact_hash: &str,
    previous_event_hash: &str,
    reason_code: &str,
    note: &str,
) -> DecisionLedgerEvent {
    build_decision_ledger_event(DecisionLedgerEventInput {
        metadata: metadata(source_artifact_hash),
        event_type,
        timestamp: "2026-06-26T16:25:00Z".to_string(),
        operator_id: "operator@example.com".to_string(),
        previous_event_hash: previous_event_hash.to_string(),
        source_artifact_hash: source_artifact_hash.to_string(),
        refs,
        reason_code: reason_code.to_string(),
        note: note.to_string(),
    })
    .expect("ledger event builds")
}

fn metadata(source_artifact_hash: &str) -> EntityArtifactMetadata {
    EntityArtifactMetadata {
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
        patch_namespace: "cmbs_tenant_label.aliases".to_string(),
        input: Some(EntityInputReference {
            row_count: 100,
            content_hash: "blake3:input".to_string(),
        }),
        upstream_artifacts: vec![EntityArtifactReference {
            version: "canon_entity_review_queue.v0".to_string(),
            content_hash: source_artifact_hash.to_string(),
        }],
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}

fn expected_context(source_artifact_hash: &str) -> DecisionLedgerExpectedContext {
    DecisionLedgerExpectedContext {
        profile_id: "cmbs_tenant_label".to_string(),
        profile_version: "0.1.0".to_string(),
        strategy_hash: "blake3:strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
        source_artifact_hash: source_artifact_hash.to_string(),
    }
}

fn expected_summary() -> canon::entity::EntityDeterministicSummary {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity/review/ledger_replay/expected_summary.json");
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_slice::<ExpectedSummary>(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
        .into()
}

#[derive(Debug, Deserialize)]
struct ExpectedSummary {
    counts: std::collections::BTreeMap<String, u64>,
    labels: std::collections::BTreeMap<String, String>,
}

impl From<ExpectedSummary> for canon::entity::EntityDeterministicSummary {
    fn from(value: ExpectedSummary) -> Self {
        Self {
            counts: value.counts,
            labels: value.labels,
        }
    }
}
