#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        EntityDeterministicSummary,
        ledger_replay::{
            DecisionLedgerReplayReport, LedgerDerivedPatch, LedgerDerivedPatchKind,
            LedgerReplayProof,
        },
        patches::{cannot_link_scope_keys, derive_review_patches},
    },
};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::PathBuf};

#[test]
#[allow(non_snake_case)]
fn EN_R003_distinct_decision_derives_distinct_patch_and_cannot_link_sidecar() {
    let expected = expected_patches();
    let bundle = derive_review_patches(&report(vec![
        patch(
            LedgerDerivedPatchKind::Distinct,
            "decision:distinct-sears-auto",
            "blake3:event-distinct",
            "cmbs_tenant_label",
            "canonical_display_label",
            vec!["surf:sears", "surf:sears_auto"],
            "review_distinct_confirmed",
        ),
        patch(
            LedgerDerivedPatchKind::CannotLink,
            "decision:distinct-sears-auto",
            "blake3:event-distinct",
            "cmbs_tenant_label",
            "canonical_display_label",
            vec!["surf:sears", "surf:sears_auto"],
            "review_distinct_confirmed",
        ),
    ]))
    .expect("patches derive");

    assert_eq!(bundle.alias_patches.len(), expected.alias_patches);
    assert_eq!(bundle.distinct_patches.len(), expected.distinct_patches);
    assert_eq!(bundle.relation_patches.len(), expected.relation_patches);
    assert_eq!(
        bundle.cannot_link_sidecars.len(),
        expected.cannot_link_sidecars
    );
    assert_eq!(bundle.distinct_patches[0].profile_id, expected.profile_id);
    assert_eq!(
        bundle.distinct_patches[0].identity_semantics,
        expected.identity_semantics
    );
    assert_eq!(bundle.distinct_patches[0].namespace, expected.namespace);
    assert_eq!(
        bundle.cannot_link_sidecars[0].hard_cannot_link,
        expected.hard_cannot_link
    );
    assert_eq!(
        bundle.cannot_link_sidecars[0].review_decision_id,
        "decision:distinct-sears-auto"
    );
}

#[test]
fn alias_distinct_contradiction_refuses_same_profile_pair() {
    let refusal = derive_review_patches(&report(vec![
        patch(
            LedgerDerivedPatchKind::Alias,
            "decision:alias-sears-auto",
            "blake3:event-alias",
            "cmbs_tenant_label",
            "canonical_display_label",
            vec!["surf:sears", "surf:sears_auto"],
            "review_merge_confirmed",
        ),
        patch(
            LedgerDerivedPatchKind::Distinct,
            "decision:distinct-sears-auto",
            "blake3:event-distinct",
            "cmbs_tenant_label",
            "canonical_display_label",
            vec!["surf:sears", "surf:sears_auto"],
            "review_distinct_confirmed",
        ),
    ]))
    .expect_err("same-profile alias/distinct contradiction refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityPatchConflict);
    assert_eq!(refusal.detail["stage"], "review_patch_derivation");
    assert_eq!(refusal.detail["reason"], "alias_distinct_conflict");
    assert_eq!(refusal.detail["profile_id"], "cmbs_tenant_label");
}

#[test]
fn profile_scoped_negative_knowledge_does_not_leak_across_profiles() {
    let bundle = derive_review_patches(&report(vec![
        patch(
            LedgerDerivedPatchKind::CannotLink,
            "decision:cmbs-distinct",
            "blake3:event-cmbs",
            "cmbs_tenant_label",
            "canonical_display_label",
            vec!["surf:sears", "surf:sears_auto"],
            "review_distinct_confirmed",
        ),
        patch(
            LedgerDerivedPatchKind::CannotLink,
            "decision:regab-distinct",
            "blake3:event-regab",
            "regab_firm_identity",
            "same_firm_or_reviewed_alias",
            vec!["surf:sears", "surf:sears_auto"],
            "review_distinct_confirmed",
        ),
    ]))
    .expect("cross-profile negatives derive separately");

    let scopes = cannot_link_scope_keys(&bundle);
    assert_eq!(scopes.len(), 2);
    assert!(scopes.contains(&(
        "cmbs_tenant_label".to_string(),
        "canonical_display_label".to_string(),
        "surf:sears".to_string(),
        "surf:sears_auto".to_string()
    )));
    assert!(scopes.contains(&(
        "regab_firm_identity".to_string(),
        "same_firm_or_reviewed_alias".to_string(),
        "surf:sears".to_string(),
        "surf:sears_auto".to_string()
    )));
}

fn report(derived_patches: Vec<LedgerDerivedPatch>) -> DecisionLedgerReplayReport {
    DecisionLedgerReplayReport {
        summary: EntityDeterministicSummary {
            counts: BTreeMap::from([(
                "derived_patch_count".to_string(),
                derived_patches.len() as u64,
            )]),
            labels: BTreeMap::new(),
        },
        replay_proofs: derived_patches
            .iter()
            .map(|patch| LedgerReplayProof {
                decision_id: patch.decision_id.clone(),
                event_hash: patch.event_hash.clone(),
                previous_event_hash: "blake3:previous".to_string(),
                event_type: match patch.kind {
                    LedgerDerivedPatchKind::Alias => {
                        canon::entity::ledger::DecisionLedgerEventType::MergeConfirmed
                    }
                    LedgerDerivedPatchKind::Distinct | LedgerDerivedPatchKind::CannotLink => {
                        canon::entity::ledger::DecisionLedgerEventType::DistinctConfirmed
                    }
                    LedgerDerivedPatchKind::Relation => {
                        canon::entity::ledger::DecisionLedgerEventType::RelationConfirmed
                    }
                    LedgerDerivedPatchKind::OverrideProof => {
                        canon::entity::ledger::DecisionLedgerEventType::OperatorOverrideApproved
                    }
                    LedgerDerivedPatchKind::Promotion => {
                        canon::entity::ledger::DecisionLedgerEventType::PromotionApplied
                    }
                    LedgerDerivedPatchKind::Revert => {
                        canon::entity::ledger::DecisionLedgerEventType::PromotionReverted
                    }
                },
                idempotency_key: patch.decision_id.clone(),
            })
            .collect(),
        derived_patches,
    }
}

fn patch(
    kind: LedgerDerivedPatchKind,
    decision_id: &str,
    event_hash: &str,
    profile_id: &str,
    identity_semantics: &str,
    surface_ids: Vec<&str>,
    reason_code: &str,
) -> LedgerDerivedPatch {
    LedgerDerivedPatch {
        kind,
        decision_id: decision_id.to_string(),
        event_hash: event_hash.to_string(),
        profile_id: profile_id.to_string(),
        identity_semantics: identity_semantics.to_string(),
        source_artifact_hash: "blake3:review-queue".to_string(),
        surface_ids: surface_ids.into_iter().map(str::to_string).collect(),
        entity_ids: vec![],
        reason_code: reason_code.to_string(),
    }
}

fn expected_patches() -> ExpectedPatches {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity/patches/en_r003_expected_patches.json");
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

#[derive(Debug, Deserialize)]
struct ExpectedPatches {
    alias_patches: usize,
    distinct_patches: usize,
    relation_patches: usize,
    cannot_link_sidecars: usize,
    profile_id: String,
    identity_semantics: String,
    namespace: String,
    hard_cannot_link: bool,
}
