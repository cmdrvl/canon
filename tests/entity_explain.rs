#![forbid(unsafe_code)]

use canon::entity::runtime::{
    explain::{ExplainReconstructionBundle, explain_from_bundle},
    types::{
        CANON_ENTITY_EDGE_VERSION, CANON_ENTITY_SOLVE_VERSION, EdgeRecord, EntityErrorCode,
        EntityState, EvidenceHit, EvidenceKind, ExplainPromotionProvenanceRecord, ExplainQuery,
        ExplainReviewDecisionRecord, ExplainSurfaceRecord, InheritanceMode, InheritanceRecord,
        PromotionDecision, PromotionWrites, RegistryPatchSummary, RegistrySnapshot,
        SolveRunArtifact, SolveRunSummary, SolvedEntity, StrategyReference,
    },
};
use predicates::prelude::*;
use std::collections::BTreeMap;

#[test]
fn entity_explain_reconstructs_rows_surfaces_evidence_review_and_promotion() {
    let bundle = reconstruction_bundle();

    let artifact = explain_from_bundle(
        ExplainQuery {
            row_id: Some("row-1".to_string()),
            ..ExplainQuery::default()
        },
        &bundle,
    )
    .expect("row explain reconstructs bundle context");

    assert_eq!(artifact.result.canonical_id.as_deref(), Some("TEN-001"));
    assert_eq!(artifact.result.backbone_rows, vec!["row-1", "row-2"]);
    assert!(artifact.result.surfaces.iter().any(|surface| {
        surface.surface_id == "surf:tenant:acme"
            && surface
                .normalized_views
                .get("tenant_name")
                .map(String::as_str)
                == Some("acme llc")
    }));
    assert_eq!(artifact.result.candidates.len(), 2);
    assert_eq!(artifact.result.positive_evidence.len(), 1);
    assert_eq!(
        artifact.result.positive_evidence[0].operator_id,
        "exact_view:tenant_name"
    );
    assert_eq!(artifact.result.anti_merge_evidence.len(), 1);
    assert_eq!(
        artifact.result.anti_merge_evidence[0].operator_id,
        "cannot_link:lease_overlap"
    );
    assert_eq!(artifact.result.review_decisions.len(), 1);
    assert_eq!(artifact.result.review_decisions[0].review_id, "review-acme");
    assert_eq!(
        artifact.result.review_decisions[0].decision,
        "accept_aliases"
    );
    assert_eq!(artifact.result.promotion_provenance.len(), 1);
    assert_eq!(
        artifact.result.promotion_provenance[0]
            .registry_version_after
            .as_deref(),
        Some("2026.06.26")
    );
}

#[test]
fn entity_explain_surface_selector_resolves_through_row_provenance() {
    let bundle = reconstruction_bundle();

    let artifact = explain_from_bundle(
        ExplainQuery {
            surface_id: Some("surf:tenant:acme".to_string()),
            ..ExplainQuery::default()
        },
        &bundle,
    )
    .expect("surface explain resolves through reconstructed row provenance");

    assert_eq!(
        artifact.query.surface_id.as_deref(),
        Some("surf:tenant:acme")
    );
    assert_eq!(artifact.result.canonical_id.as_deref(), Some("TEN-001"));
    assert_eq!(artifact.result.backbone_rows, vec!["row-1", "row-2"]);
    assert_eq!(artifact.result.positive_evidence.len(), 1);
    assert_eq!(artifact.result.anti_merge_evidence.len(), 1);
}

#[test]
fn entity_explain_unknown_surface_refuses() {
    let bundle = reconstruction_bundle();
    let error = explain_from_bundle(
        ExplainQuery {
            surface_id: Some("surf:tenant:missing".to_string()),
            ..ExplainQuery::default()
        },
        &bundle,
    )
    .expect_err("unknown surface selector refuses");

    assert_eq!(error.code, EntityErrorCode::Explain);
    assert!(error.message.contains("surface_id"));
}

#[test]
fn cli_entity_explain_reads_reconstruction_bundle() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let result_path = temp_dir.path().join("result.json");
    std::fs::write(
        &result_path,
        serde_json::to_vec(&reconstruction_bundle()).expect("bundle json"),
    )
    .expect("write bundle");

    assert_cmd::cargo::cargo_bin_cmd!("canon")
        .args([
            "entity",
            "explain",
            result_path.to_str().expect("utf-8 path"),
            "--surface-id",
            "surf:tenant:acme",
            "--emit",
            "summary",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("surface surf:tenant:acme"))
        .stdout(predicate::str::contains("support=1"))
        .stdout(predicate::str::contains("anti_merge=1"))
        .stdout(predicate::str::contains("review_decisions=1"))
        .stdout(predicate::str::contains("promotions=1"));
}

fn reconstruction_bundle() -> ExplainReconstructionBundle {
    ExplainReconstructionBundle {
        result: SolveRunArtifact {
            version: CANON_ENTITY_SOLVE_VERSION.to_string(),
            strategy: strategy_reference(),
            registry: registry_snapshot(),
            summary: SolveRunSummary {
                observations: 3,
                resolved_existing: 2,
                promotable_new: 0,
                abstain_low_evidence: 0,
                abstain_conflict: 0,
            },
            entities: vec![
                SolvedEntity {
                    state: EntityState::ResolvedExisting,
                    canonical_id: Some("TEN-001".to_string()),
                    backbone_rows: vec!["row-1".to_string(), "row-2".to_string()],
                    all_rows: vec!["row-1".to_string(), "row-2".to_string()],
                    aliases: vec!["Acme LLC".to_string()],
                    inheritance: InheritanceRecord {
                        mode: InheritanceMode::SingleIncumbentOverlap,
                        incumbent_ids: vec!["TEN-001".to_string()],
                    },
                    ..SolvedEntity::default()
                },
                SolvedEntity {
                    state: EntityState::ResolvedExisting,
                    canonical_id: Some("TEN-999".to_string()),
                    backbone_rows: vec!["row-9".to_string()],
                    all_rows: vec!["row-9".to_string()],
                    inheritance: InheritanceRecord {
                        mode: InheritanceMode::SingleIncumbentOverlap,
                        incumbent_ids: vec!["TEN-999".to_string()],
                    },
                    ..SolvedEntity::default()
                },
            ],
            proposed_registry_patch: RegistryPatchSummary {
                mapping_files: vec!["aliases.json".to_string()],
                new_entity_entries: 0,
                existing_alias_entries: 1,
            },
            ..SolveRunArtifact::default()
        },
        edges: vec![
            edge_record(
                "row-1",
                "row-2",
                48,
                false,
                vec![EvidenceHit {
                    kind: EvidenceKind::Support,
                    namespace: "tenant_name".to_string(),
                    operator_id: "exact_view:tenant_name".to_string(),
                    score: 48,
                    explanation: "normalized tenant names match".to_string(),
                }],
            ),
            edge_record(
                "row-1",
                "row-9",
                -100,
                true,
                vec![EvidenceHit {
                    kind: EvidenceKind::CannotLink,
                    namespace: "anti_merge".to_string(),
                    operator_id: "cannot_link:lease_overlap".to_string(),
                    score: -100,
                    explanation: "trusted cannot-link sidecar separates tenants".to_string(),
                }],
            ),
        ],
        surfaces: vec![
            ExplainSurfaceRecord {
                surface_id: "surf:tenant:acme".to_string(),
                primary_surface: "Acme LLC".to_string(),
                row_ids: vec!["row-2".to_string(), "row-1".to_string()],
                row_count: 2,
                normalized_views: BTreeMap::from([(
                    "tenant_name".to_string(),
                    "acme llc".to_string(),
                )]),
                provenance: BTreeMap::from([(
                    "source_rows".to_string(),
                    vec!["row-1".to_string(), "row-2".to_string()],
                )]),
            },
            ExplainSurfaceRecord {
                surface_id: "surf:tenant:beta".to_string(),
                primary_surface: "Beta LLC".to_string(),
                row_ids: vec!["row-9".to_string()],
                row_count: 1,
                normalized_views: BTreeMap::from([(
                    "tenant_name".to_string(),
                    "beta llc".to_string(),
                )]),
                provenance: BTreeMap::new(),
            },
        ],
        review_decisions: vec![
            ExplainReviewDecisionRecord {
                review_id: "review-acme".to_string(),
                category: "resolved".to_string(),
                state: EntityState::ResolvedExisting,
                canonical_id: Some("TEN-001".to_string()),
                escrow_id: None,
                source_row_ids: vec!["row-2".to_string(), "row-1".to_string()],
                surface_ids: vec!["surf:tenant:acme".to_string()],
                proposed_action: "accept_aliases".to_string(),
                decision: "accept_aliases".to_string(),
            },
            ExplainReviewDecisionRecord {
                review_id: "review-other".to_string(),
                category: "resolved".to_string(),
                state: EntityState::ResolvedExisting,
                canonical_id: Some("TEN-999".to_string()),
                escrow_id: None,
                source_row_ids: vec!["row-9".to_string()],
                surface_ids: vec!["surf:tenant:beta".to_string()],
                proposed_action: "accept_aliases".to_string(),
                decision: "skip".to_string(),
            },
        ],
        promotion_provenance: vec![ExplainPromotionProvenanceRecord {
            artifact_version: Some("canon_entity_promote.v0".to_string()),
            canonical_id: Some("TEN-001".to_string()),
            escrow_id: None,
            row_ids: vec!["row-1".to_string(), "row-2".to_string()],
            surface_ids: vec!["surf:tenant:acme".to_string()],
            decision: PromotionDecision::Promote,
            writes: PromotionWrites {
                mapping_files: vec!["aliases.json".to_string()],
                new_entity_entries: 0,
                existing_alias_entries: 1,
                pending_cluster_entries: 0,
                cannot_link_entries: 0,
            },
            registry_version_before: Some("2026.06.25".to_string()),
            registry_version_after: Some("2026.06.26".to_string()),
        }],
        ..ExplainReconstructionBundle::default()
    }
}

fn edge_record(
    left_row_id: &str,
    right_row_id: &str,
    pair_score_total: i64,
    has_cannot_link: bool,
    hits: Vec<EvidenceHit>,
) -> EdgeRecord {
    EdgeRecord {
        version: CANON_ENTITY_EDGE_VERSION.to_string(),
        strategy: strategy_reference(),
        registry_snapshot: registry_snapshot(),
        left_row_id: left_row_id.to_string(),
        right_row_id: right_row_id.to_string(),
        pair_score_by_namespace: hits
            .iter()
            .filter(|hit| !matches!(hit.kind, EvidenceKind::CannotLink))
            .map(|hit| (hit.namespace.clone(), hit.score))
            .collect(),
        pair_score_total,
        has_must_link: false,
        has_cannot_link,
        hits,
    }
}

fn strategy_reference() -> StrategyReference {
    StrategyReference {
        id: "cmbs_tenant_label.v1".to_string(),
        version: "0.1.0".to_string(),
        content_hash: "blake3:strategy".to_string(),
    }
}

fn registry_snapshot() -> RegistrySnapshot {
    RegistrySnapshot {
        id: "cmbs-tenants".to_string(),
        version: "2026.06.25".to_string(),
        source: "registries/cmbs-tenants".to_string(),
        lookup_snapshot_hash: "blake3:lookup".to_string(),
        escrow_snapshot_hash: "blake3:escrow".to_string(),
    }
}
