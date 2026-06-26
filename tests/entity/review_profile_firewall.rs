#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        EntityArtifactMetadata, EntityInputReference, EntityPatchNamespaces,
        EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
        review_import::{
            ReviewImportAction, ReviewImportContext, ReviewImportDecision, ReviewImportRequest,
            import_review_decisions,
        },
    },
};
use std::{collections::BTreeSet, path::Path};

#[test]
fn review_import_profile_firewall_refuses_profile_mismatch() {
    assert_firewall_refusal(
        decision_with(|decision| decision.profile_id = "regab_firm_identity".to_string()),
        "profile_id",
        "cmbs_tenant_label",
        "regab_firm_identity",
    );
}

#[test]
fn review_import_profile_firewall_refuses_entity_type_mismatch() {
    assert_firewall_refusal(
        decision_with(|decision| decision.entity_type = Some("organization".to_string())),
        "entity_type",
        "tenant_label",
        "organization",
    );
}

#[test]
fn review_import_profile_firewall_refuses_identity_semantics_mismatch() {
    assert_firewall_refusal(
        decision_with(|decision| {
            decision.identity_semantics = Some("same_firm_or_reviewed_alias".to_string());
        }),
        "identity_semantics",
        "canonical_display_label",
        "same_firm_or_reviewed_alias",
    );
}

#[test]
fn review_import_profile_firewall_refuses_missing_entity_type() {
    assert_firewall_refusal_without_expected_actual(
        decision_with(|decision| decision.entity_type = None),
        "entity_type",
    );
}

#[test]
fn review_import_no_ledger_mutation_on_refusal_for_registry_snapshot_mismatch() {
    assert_firewall_refusal(
        decision_with(|decision| {
            decision.registry_snapshot_hash = "blake3:other-registry".to_string()
        }),
        "registry_snapshot_hash",
        "blake3:registry",
        "blake3:other-registry",
    );
}

fn assert_firewall_refusal(
    decision: ReviewImportDecision,
    expected_field: &str,
    expected: &str,
    actual: &str,
) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let ledger_path = temp_dir.path().join("decision-ledger.jsonl");
    let refusal = import_review_decisions(ReviewImportRequest {
        context: import_context(),
        decisions: vec![decision],
        ledger_path: ledger_path.clone(),
        timestamp: "2026-06-26T16:40:00Z".to_string(),
        previous_event_hash: "blake3:previous-event".to_string(),
    })
    .expect_err("firewall mismatch refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["stage"], "review_import");
    assert_eq!(refusal.detail["field"], expected_field);
    assert_eq!(refusal.detail["expected"], expected);
    assert_eq!(refusal.detail["actual"], actual);
    assert_no_ledger_mutation(&ledger_path);
}

fn assert_firewall_refusal_without_expected_actual(
    decision: ReviewImportDecision,
    expected_field: &str,
) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let ledger_path = temp_dir.path().join("decision-ledger.jsonl");
    let refusal = import_review_decisions(ReviewImportRequest {
        context: import_context(),
        decisions: vec![decision],
        ledger_path: ledger_path.clone(),
        timestamp: "2026-06-26T16:40:00Z".to_string(),
        previous_event_hash: "blake3:previous-event".to_string(),
    })
    .expect_err("missing firewall context refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["stage"], "review_import");
    assert_eq!(refusal.detail["field"], expected_field);
    assert_no_ledger_mutation(&ledger_path);
}

fn assert_no_ledger_mutation(path: &Path) {
    assert!(!path.exists(), "ledger must not be created on refusal");
}

fn decision_with(update: impl FnOnce(&mut ReviewImportDecision)) -> ReviewImportDecision {
    let mut decision = ReviewImportDecision {
        review_id: "review:surf_sears".to_string(),
        action: ReviewImportAction::DistinctConfirmed,
        operator_id: "operator@example.com".to_string(),
        source_review_queue_hash: "blake3:review-queue".to_string(),
        profile_id: "cmbs_tenant_label".to_string(),
        profile_version: "0.1.0".to_string(),
        entity_type: Some("tenant_label".to_string()),
        identity_semantics: Some("canonical_display_label".to_string()),
        strategy_hash: "blake3:strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
        surface_ids: vec!["surf:sears".to_string(), "surf:sears_auto".to_string()],
        reason_code: "review_distinct_confirmed".to_string(),
        note: "profile firewall test".to_string(),
        override_approved_by: None,
        override_reason_code: None,
    };
    update(&mut decision);
    decision
}

fn import_context() -> ReviewImportContext {
    ReviewImportContext {
        metadata: metadata(),
        source_review_queue_hash: "blake3:review-queue".to_string(),
        known_review_ids: BTreeSet::from(["review:surf_sears".to_string()]),
        cannot_link_review_ids: BTreeSet::new(),
    }
}

fn metadata() -> EntityArtifactMetadata {
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
        upstream_artifacts: vec![],
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}
