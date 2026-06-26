#![forbid(unsafe_code)]

use canon::entity::{
    EntityArtifactMetadata, EntityArtifactReference, EntityInputReference, EntityPatchNamespaces,
    EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
    graph::{SignedEvidenceGraphInput, build_signed_evidence_graph},
    ledger::DecisionLedgerEvent,
    review::{
        ReviewExportInclude, ReviewProvenanceSample, ReviewQueueArtifact, ReviewQueueRequest,
        ReviewRelationHint, build_review_queue_artifact, render_review_queue_csv,
    },
    review_import::{
        ReviewImportAction, ReviewImportContext, ReviewImportDecision, ReviewImportRequest,
        import_review_decisions,
    },
    score::{ScoreLane, ScoreUnits},
    solve::{
        SolveArtifact, SolveArtifactRequest, SolveReconciliationConfig, SolveSurfaceProvenance,
        build_solve_artifact_contract,
    },
};
use serde::Deserialize;
use std::{fs, path::Path};

const EXPECTED: &str =
    include_str!("../fixtures/entity/cmbs/review_loop/expected_review_loop.json");

#[derive(Debug, Deserialize)]
struct ReviewLoopExpected {
    schema_version: String,
    profile_id: String,
    review_export: ExpectedReviewExport,
    review_import: ExpectedReviewImport,
    non_goals: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ExpectedReviewExport {
    expected_review_items: u64,
    expected_review_id: String,
    expected_surface_ids: Vec<String>,
    affected_rows: u64,
    affected_deals: u64,
    priority_reason_codes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ExpectedReviewImport {
    action: ReviewImportAction,
    operator_id: String,
    reason_code: String,
    note: String,
}

#[test]
fn cmbs_review_loop_exports_one_ambiguity_group_and_imports_distinct_decision() {
    let expected = expected_loop();
    let artifact = cmbs_review_queue();

    assert_eq!(artifact.version, "canon_entity_review_queue.v0");
    assert_eq!(
        artifact.review_items.len() as u64,
        expected.review_export.expected_review_items
    );
    assert_eq!(
        artifact.summary.counts["review_items"],
        expected.review_export.expected_review_items
    );
    assert_eq!(
        artifact.summary.counts["review_rows_covered"],
        expected.review_export.affected_rows
    );
    assert_eq!(
        artifact.summary.counts["review_deals_covered"],
        expected.review_export.affected_deals
    );

    let item = &artifact.review_items[0];
    assert_eq!(item.review_id, expected.review_export.expected_review_id);
    assert_eq!(
        item.surface_ids,
        expected.review_export.expected_surface_ids
    );
    assert_eq!(item.affected_rows, expected.review_export.affected_rows);
    assert_eq!(item.affected_deals, expected.review_export.affected_deals);
    for reason in &expected.review_export.priority_reason_codes {
        assert!(
            item.priority_reasons.contains(reason),
            "missing review priority reason {reason}"
        );
    }

    let csv = render_review_queue_csv(&artifact).expect("review csv renders");
    let mut reader = csv::Reader::from_reader(csv.as_bytes());
    let records = reader
        .records()
        .collect::<Result<Vec<_>, _>>()
        .expect("review csv rows parse");
    assert_eq!(records.len(), 1, "CMBS repeated ambiguity exports once");

    let temp = tempfile::tempdir().expect("tempdir");
    let ledger_path = temp.path().join("decision-ledger.jsonl");
    let receipt = import_review_decisions(ReviewImportRequest {
        context: review_import_context(&artifact),
        decisions: vec![review_decision(&artifact, &expected)],
        ledger_path: ledger_path.clone(),
        timestamp: "2026-06-26T19:50:00Z".to_string(),
        previous_event_hash: "blake3:previous-cmbs-review-event".to_string(),
    })
    .expect("CMBS review import appends ledger");

    assert_eq!(receipt.accepted_decisions, 1);
    assert_eq!(receipt.appended_events.len(), 1);
    assert!(!temp.path().join("promote.json").exists());
    assert!(!temp.path().join("apply.csv").exists());

    let event = ledger_events(&ledger_path)
        .into_iter()
        .next()
        .expect("one ledger event");
    assert_eq!(event.decision, "distinct_confirmed");
    assert_eq!(event.operator_id, expected.review_import.operator_id);
    assert_eq!(event.reason_code, expected.review_import.reason_code);
    assert_eq!(event.note, expected.review_import.note);
    assert_eq!(event.source_artifact_hash, artifact.artifact_content_hash);
    assert_eq!(event.refs.left_surface_id.as_deref(), Some("surf:sears"));
    assert_eq!(
        event.refs.right_surface_id.as_deref(),
        Some("surf:sears_auto")
    );
}

#[test]
#[allow(non_snake_case)]
fn ER_REVIEW_GOLDEN_001_cmbs_review_loop_fixture_is_behavioral() {
    let expected = expected_loop();
    assert_eq!(expected.schema_version, "canon.entity.cmbs_review_loop.v0");
    assert_eq!(expected.profile_id, "cmbs_tenant_label");
    assert_eq!(
        expected.review_export.expected_surface_ids,
        ["surf:sears", "surf:sears_auto"]
    );
    assert_eq!(
        expected.review_import.action,
        ReviewImportAction::DistinctConfirmed
    );
    assert_eq!(expected.non_goals, ["promotion", "apply"]);
}

fn cmbs_review_queue() -> ReviewQueueArtifact {
    build_review_queue_artifact(ReviewQueueRequest {
        solve_artifact: cmbs_solve_artifact(),
        include: ReviewExportInclude::All,
        provenance_samples: provenance_samples(),
        relation_hints: relation_hints(),
    })
    .expect("CMBS review queue builds")
}

fn cmbs_solve_artifact() -> SolveArtifact {
    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records: vec![support_and_anti_merge_record(
            "surf:sears",
            "surf:sears_auto",
        )],
        exact_bucket_assertions: vec![],
        incumbent_ids: vec![],
    })
    .expect("signed graph builds");
    build_solve_artifact_contract(SolveArtifactRequest {
        metadata: metadata(),
        graph,
        config: SolveReconciliationConfig::delegate_new_ids(score(5_000)),
        provenance: vec![
            SolveSurfaceProvenance {
                surface_id: "surf:sears".to_string(),
                row_count: 80,
                deal_count: 28,
            },
            SolveSurfaceProvenance {
                surface_id: "surf:sears_auto".to_string(),
                row_count: 32,
                deal_count: 9,
            },
        ],
        decision_ledger_path: "review/decision-ledger.jsonl".to_string(),
    })
    .expect("solve artifact builds")
}

fn support_and_anti_merge_record(left: &str, right: &str) -> EdgeEvidenceRecord {
    build_edge_evidence_record(
        left,
        right,
        vec![
            EdgeEvidenceHit::new(
                ScoreLane::Support,
                "name",
                "string_similarity",
                "positive_identity_evidence",
                score(9_500),
                false,
                "Sears tenant labels share strong name evidence",
            ),
            EdgeEvidenceHit::new(
                ScoreLane::AntiMerge,
                "cmbs_tenant_label.distinct",
                "operator_patch",
                "distinct_identity_evidence",
                score(9_000),
                true,
                "Sears Auto Center is a distinct tenant label",
            ),
        ],
    )
    .expect("support plus anti-merge edge builds")
}

fn review_import_context(artifact: &ReviewQueueArtifact) -> ReviewImportContext {
    ReviewImportContext {
        metadata: artifact.metadata.clone(),
        source_review_queue_hash: artifact.artifact_content_hash.clone(),
        known_review_ids: artifact
            .review_items
            .iter()
            .map(|item| item.review_id.clone())
            .collect(),
        cannot_link_review_ids: artifact
            .review_items
            .iter()
            .map(|item| item.review_id.clone())
            .collect(),
    }
}

fn review_decision(
    artifact: &ReviewQueueArtifact,
    expected: &ReviewLoopExpected,
) -> ReviewImportDecision {
    let item = &artifact.review_items[0];
    ReviewImportDecision {
        review_id: item.review_id.clone(),
        action: expected.review_import.action,
        operator_id: expected.review_import.operator_id.clone(),
        source_review_queue_hash: artifact.artifact_content_hash.clone(),
        profile_id: artifact.metadata.profile.id.clone(),
        profile_version: artifact.metadata.profile.version.clone(),
        entity_type: Some(artifact.metadata.profile.entity_type.clone()),
        identity_semantics: Some(artifact.metadata.profile.identity_semantics.clone()),
        strategy_hash: artifact.metadata.strategy.content_hash.clone(),
        registry_snapshot_hash: artifact
            .metadata
            .registry_snapshot
            .lookup_snapshot_hash
            .clone(),
        surface_ids: item.surface_ids.clone(),
        reason_code: expected.review_import.reason_code.clone(),
        note: expected.review_import.note.clone(),
        override_approved_by: None,
        override_reason_code: None,
    }
}

fn provenance_samples() -> Vec<ReviewProvenanceSample> {
    vec![
        ReviewProvenanceSample {
            surface_id: "surf:sears".to_string(),
            row_id: "deal-001#tenant-001".to_string(),
            source: "CMBS-DEAL-001".to_string(),
            raw_value: "Sears".to_string(),
        },
        ReviewProvenanceSample {
            surface_id: "surf:sears_auto".to_string(),
            row_id: "deal-017#tenant-004".to_string(),
            source: "CMBS-DEAL-017".to_string(),
            raw_value: "Sears Auto Center".to_string(),
        },
    ]
}

fn relation_hints() -> Vec<ReviewRelationHint> {
    vec![ReviewRelationHint {
        left_surface_id: "surf:sears".to_string(),
        right_surface_id: "surf:sears_auto".to_string(),
        relation: "related_brand_family".to_string(),
        reason_code: "same_brand_family_review_only".to_string(),
    }]
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
            row_count: 112,
            content_hash: "blake3:input".to_string(),
        }),
        upstream_artifacts: vec![
            EntityArtifactReference {
                version: "canon_entity_block.v0".to_string(),
                content_hash: "blake3:block".to_string(),
            },
            EntityArtifactReference {
                version: "canon_entity_edge.v0".to_string(),
                content_hash: "blake3:edge".to_string(),
            },
        ],
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}

fn ledger_events(path: &Path) -> Vec<DecisionLedgerEvent> {
    fs::read_to_string(path)
        .expect("ledger reads")
        .lines()
        .map(|line| serde_json::from_str(line).expect("ledger event parses"))
        .collect()
}

fn expected_loop() -> ReviewLoopExpected {
    serde_json::from_str(EXPECTED).expect("CMBS review loop fixture parses")
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
