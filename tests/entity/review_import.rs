#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        EntityArtifactMetadata, EntityInputReference, EntityPatchNamespaces,
        EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
        ledger::DecisionLedgerEvent,
        review_import::{
            ReviewImportAction, ReviewImportContext, ReviewImportDecision, ReviewImportRequest,
            decisions_by_review_id, import_review_decisions, parse_review_import_csv,
            parse_review_import_json,
        },
    },
};
use std::{collections::BTreeSet, fs, path::PathBuf};

#[test]
#[allow(non_snake_case)]
fn EN_R002_stale_review_import_refuses_before_ledger_append() {
    let decisions = parse_review_import_json(include_str!(
        "../fixtures/entity/review/import/en_r002_stale_review.json"
    ))
    .expect("stale fixture parses");
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let ledger_path = temp_dir.path().join("decision-ledger.jsonl");
    let refusal = import_review_decisions(ReviewImportRequest {
        context: import_context(),
        decisions,
        ledger_path: ledger_path.clone(),
        timestamp: "2026-06-26T16:20:00Z".to_string(),
        previous_event_hash: "blake3:previous-event".to_string(),
    })
    .expect_err("stale review refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
    assert_eq!(refusal.detail["stage"], "review_import");
    assert_eq!(refusal.detail["field"], "source_review_queue_hash");
    assert_eq!(refusal.detail["expected"], "blake3:review-queue");
    assert_eq!(refusal.detail["actual"], "blake3:stale-review-queue");
    assert!(
        !ledger_path.exists(),
        "ledger append must not occur on refusal"
    );
}

#[test]
#[allow(non_snake_case)]
fn EN_R003_review_import_parses_csv_and_appends_ledger_event() {
    let decisions = parse_review_import_csv(include_str!(
        "../fixtures/entity/review/import/en_r003_distinct_decision.csv"
    ))
    .expect("csv fixture parses");
    assert_eq!(
        decisions_by_review_id(&decisions)["review:surf_sears"],
        ReviewImportAction::DistinctConfirmed
    );

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let ledger_path = temp_dir.path().join("decision-ledger.jsonl");
    let receipt = import_review_decisions(ReviewImportRequest {
        context: import_context(),
        decisions,
        ledger_path: ledger_path.clone(),
        timestamp: "2026-06-26T16:20:00Z".to_string(),
        previous_event_hash: "blake3:previous-event".to_string(),
    })
    .expect("valid review import appends ledger");

    assert_eq!(receipt.accepted_decisions, 1);
    assert_eq!(receipt.appended_events.len(), 1);
    let contents = fs::read_to_string(&ledger_path).expect("ledger reads");
    let events = contents
        .lines()
        .map(|line| serde_json::from_str::<DecisionLedgerEvent>(line).expect("event parses"))
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type.as_str(), "distinct_confirmed");
    assert_eq!(events[0].source_artifact_hash, "blake3:review-queue");
    assert_eq!(events[0].operator_id, "operator@example.com");
}

#[test]
#[allow(non_snake_case)]
fn E_ENTITY_CANNOT_LINK_OVERRIDE_requires_explicit_operator_provenance() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let ledger_path = temp_dir.path().join("decision-ledger.jsonl");
    let refusal = import_review_decisions(ReviewImportRequest {
        context: import_context(),
        decisions: vec![merge_override_decision(None, None)],
        ledger_path: ledger_path.clone(),
        timestamp: "2026-06-26T16:20:00Z".to_string(),
        previous_event_hash: "blake3:previous-event".to_string(),
    })
    .expect_err("missing override provenance refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityCannotLinkOverride);
    assert_eq!(refusal.detail["stage"], "review_import");
    assert_eq!(refusal.detail["review_id"], "review:surf_sears");
    assert!(
        !ledger_path.exists(),
        "ledger append must not occur on refusal"
    );

    let receipt = import_review_decisions(ReviewImportRequest {
        context: import_context(),
        decisions: vec![merge_override_decision(
            Some("principal@example.com"),
            Some("approved_hard_cannot_link_override"),
        )],
        ledger_path,
        timestamp: "2026-06-26T16:21:00Z".to_string(),
        previous_event_hash: "blake3:previous-event".to_string(),
    })
    .expect("explicit override provenance permits ledger append");
    assert_eq!(receipt.accepted_decisions, 1);
}

fn merge_override_decision(
    override_approved_by: Option<&str>,
    override_reason_code: Option<&str>,
) -> ReviewImportDecision {
    ReviewImportDecision {
        review_id: "review:surf_sears".to_string(),
        action: ReviewImportAction::MergeConfirmed,
        operator_id: "operator@example.com".to_string(),
        source_review_queue_hash: "blake3:review-queue".to_string(),
        profile_id: "cmbs_tenant_label".to_string(),
        profile_version: "0.1.0".to_string(),
        entity_type: Some("tenant_label".to_string()),
        identity_semantics: Some("canonical_display_label".to_string()),
        strategy_hash: "blake3:strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
        surface_ids: vec!["surf:sears".to_string(), "surf:sears_auto".to_string()],
        reason_code: "review_merge_confirmed".to_string(),
        note: "operator requested merge across a hard cannot-link".to_string(),
        override_approved_by: override_approved_by.map(str::to_string),
        override_reason_code: override_reason_code.map(str::to_string),
    }
}

fn import_context() -> ReviewImportContext {
    ReviewImportContext {
        metadata: metadata(),
        source_review_queue_hash: "blake3:review-queue".to_string(),
        known_review_ids: BTreeSet::from(["review:surf_sears".to_string()]),
        cannot_link_review_ids: BTreeSet::from(["review:surf_sears".to_string()]),
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

#[allow(dead_code)]
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity/review/import")
        .join(name)
}
