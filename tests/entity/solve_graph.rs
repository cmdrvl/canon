#![forbid(unsafe_code)]

use canon::entity::{
    CANON_ENTITY_BLOCK_BUCKET_VERSION, CANON_ENTITY_SOLVE_VERSION,
    block_artifact::{
        CannotLinkAction, CannotLinkValidationHook, CannotLinkValidationStatus,
        EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN, ExactBucketAssertion, ExactBucketDiagnostics,
        ExactBucketMembership, ExactBucketProfile, ExactBucketUpstream, SurfaceIdRange,
    },
    edge::{EdgeEvidenceHit, build_edge_evidence_record},
    graph::{
        EntityEvidenceGraph, ExactBucketSolveAction, SignedEvidenceGraphInput, SurfaceIncumbentId,
        SurfacePair, build_signed_evidence_graph,
    },
    score::{ScoreLane, ScoreUnits},
};
use std::collections::BTreeMap;

#[test]
fn entity_solve_graph_preserves_signed_evidence_lanes_and_ordering() {
    let mixed_record = build_edge_evidence_record(
        "surf:001",
        "surf:002",
        vec![
            relation_hit("ontology", "related_brand_family", 10_000),
            support_hit("name", "jaro_winkler", 6_250),
            cannot_link_hit("tenant_role", "protected_token", 9_500, true),
        ],
    )
    .expect("mixed signed evidence record builds");
    let support_record = build_edge_evidence_record(
        "surf:000",
        "surf:003",
        vec![support_hit("token", "tfidf_cosine", 4_000)],
    )
    .expect("support evidence record builds");

    let input = SignedEvidenceGraphInput {
        edge_records: vec![mixed_record.clone(), support_record.clone()],
        exact_bucket_assertions: vec![],
        incumbent_ids: vec![SurfaceIncumbentId {
            surface_id: "surf:001".to_string(),
            canonical_id: "TENANT-001".to_string(),
        }],
    };
    let graph = build_signed_evidence_graph(input).expect("signed graph builds");

    assert_eq!(graph.version, CANON_ENTITY_SOLVE_VERSION);
    assert_eq!(
        graph
            .surface_nodes
            .iter()
            .map(|node| (
                node.surface_id.as_str(),
                node.incumbent_canonical_id.as_deref()
            ))
            .collect::<Vec<_>>(),
        [
            ("surf:000", None),
            ("surf:001", Some("TENANT-001")),
            ("surf:002", None),
            ("surf:003", None),
        ]
    );
    assert_eq!(graph.support_edges.len(), 2);
    assert_eq!(
        graph
            .support_edges
            .iter()
            .map(|edge| (
                edge.left_surface_id.as_str(),
                edge.right_surface_id.as_str(),
                edge.score_units.as_u32()
            ))
            .collect::<Vec<_>>(),
        [
            ("surf:000", "surf:003", 4_000),
            ("surf:001", "surf:002", 6_250),
        ]
    );
    assert!(
        graph
            .support_edges
            .iter()
            .flat_map(|edge| edge.evidence.iter())
            .all(|hit| hit.lane == ScoreLane::Support)
    );
    assert_eq!(graph.cannot_link_edges.len(), 1);
    assert!(graph.cannot_link_edges[0].hard_cannot_link);
    assert_eq!(graph.cannot_link_edges[0].score_units, score(9_500));
    assert_eq!(graph.relation_hint_edges.len(), 1);
    assert_eq!(graph.relation_hint_edges[0].score_units, score(10_000));
    assert_eq!(
        graph.hard_cannot_links,
        [SurfacePair::new("surf:001", "surf:002").expect("ordered pair")]
            .into_iter()
            .collect()
    );
    assert_eq!(graph.diagnostics.support_edge_count, 2);
    assert_eq!(graph.diagnostics.cannot_link_edge_count, 1);
    assert_eq!(graph.diagnostics.hard_cannot_link_edge_count, 1);
    assert_eq!(graph.diagnostics.relation_hint_edge_count, 1);

    let reversed = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records: vec![support_record, mixed_record],
        exact_bucket_assertions: vec![],
        incumbent_ids: vec![SurfaceIncumbentId {
            surface_id: "surf:001".to_string(),
            canonical_id: "TENANT-001".to_string(),
        }],
    })
    .expect("reversed graph builds");
    assert_eq!(
        serde_json::to_vec(&graph).expect("graph serializes"),
        serde_json::to_vec(&reversed).expect("reversed graph serializes")
    );
}

#[test]
fn exact_bucket_hyperedge_solver_consumes_range_membership_without_pair_expansion() {
    let assertion = exact_bucket_assertion(
        "bucket:tenant_core:sears-range",
        ExactBucketMembership {
            surface_ids: vec![],
            surface_ranges: vec![SurfaceIdRange {
                start_surface_id: "surf:sears:0000".to_string(),
                end_surface_id: "surf:sears:7999".to_string(),
                member_count: 8_000,
            }],
        },
        8_000,
    );
    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records: vec![],
        exact_bucket_assertions: vec![assertion],
        incumbent_ids: vec![],
    })
    .expect("range-backed exact bucket graph builds");

    assert_eq!(graph.exact_bucket_hyperedges.len(), 1);
    let hyperedge = &graph.exact_bucket_hyperedges[0];
    assert_eq!(hyperedge.member_count, 8_000);
    assert_eq!(hyperedge.membership_record_count, 1);
    assert_eq!(hyperedge.expanded_pair_count, 0);
    assert_eq!(hyperedge.theoretical_pair_count, 31_996_000);
    assert!(hyperedge.explicit_surface_ids.is_empty());
    assert_eq!(hyperedge.surface_ranges.len(), 1);
    assert_eq!(graph.surface_nodes.len(), 2);
    assert_eq!(graph.diagnostics.exact_bucket_member_count, 8_000);
    assert_eq!(graph.diagnostics.exact_bucket_membership_record_count, 1);
    assert_eq!(graph.diagnostics.materialized_exact_bucket_pair_count, 0);
    assert_eq!(
        graph.diagnostics.theoretical_exact_bucket_pair_count,
        31_996_000
    );

    let report = graph.solve_exact_bucket_hyperedges();
    assert_eq!(report.hyperedge_count, 1);
    assert_eq!(report.membership_record_count, 1);
    assert_eq!(report.expanded_pair_count, 0);
    assert_eq!(report.theoretical_pair_count, 31_996_000);
    assert_eq!(
        report.decisions[0].action,
        ExactBucketSolveAction::MergeCluster
    );
    assert_eq!(report.decisions[0].expanded_pair_count, 0);
}

#[test]
fn relation_hint_non_merge_graph_does_not_create_support_edge() {
    let record = build_edge_evidence_record(
        "surf:sears",
        "surf:transform",
        vec![relation_hit(
            "cmbs_tenant_label.relations",
            "possible_successor_predecessor",
            10_000,
        )],
    )
    .expect("relation-only edge record builds");
    assert_eq!(record.pair_score_total, ScoreUnits::ZERO);

    let graph = EntityEvidenceGraph::from_signed_evidence(&[record], &[])
        .expect("relation-only graph builds");

    assert!(graph.support_edges.is_empty());
    assert!(graph.cannot_link_edges.is_empty());
    assert_eq!(graph.relation_hint_edges.len(), 1);
    assert_eq!(graph.relation_hint_edges[0].score_units, score(10_000));
    assert_eq!(graph.diagnostics.support_edge_count, 0);
    assert_eq!(graph.diagnostics.relation_hint_edge_count, 1);
}

fn exact_bucket_assertion(
    bucket_id: &str,
    membership: ExactBucketMembership,
    row_count: u64,
) -> ExactBucketAssertion {
    ExactBucketAssertion {
        version: CANON_ENTITY_BLOCK_BUCKET_VERSION.to_string(),
        bucket_id: bucket_id.to_string(),
        operator_id: "exact_view:tenant_core".to_string(),
        profile: ExactBucketProfile {
            id: "cmbs_tenant_label".to_string(),
            version: "0.1.0".to_string(),
            identity_semantics: "canonical_display_label".to_string(),
            content_hash: "blake3:profile".to_string(),
        },
        upstream: ExactBucketUpstream {
            prepare_hash: "blake3:prepare".to_string(),
            index_hash: "blake3:index".to_string(),
            strategy_hash: "blake3:block-strategy".to_string(),
            registry_snapshot_hash: "blake3:registry".to_string(),
        },
        membership,
        row_count,
        deal_count: row_count,
        pair_expansion: EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN.to_string(),
        diagnostics: ExactBucketDiagnostics {
            largest_bucket_size: row_count,
            suppressed_pair_count: row_count.saturating_mul(row_count.saturating_sub(1)) / 2,
            labels: BTreeMap::from([("identity_view".to_string(), "tenant_core".to_string())]),
        },
        cannot_link_validation: CannotLinkValidationHook {
            status: CannotLinkValidationStatus::CheckedNoConflicts,
            checked_fact_count: 0,
            hard_cannot_link_count: 0,
            action: CannotLinkAction::AllowMerge,
        },
    }
}

fn support_hit(namespace: &str, operator_id: &str, units: u32) -> EdgeEvidenceHit {
    EdgeEvidenceHit::new(
        ScoreLane::Support,
        namespace,
        operator_id,
        "positive_identity_evidence",
        score(units),
        false,
        "positive identity evidence",
    )
}

fn cannot_link_hit(
    namespace: &str,
    operator_id: &str,
    units: u32,
    hard_cannot_link: bool,
) -> EdgeEvidenceHit {
    EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        namespace,
        operator_id,
        "distinct_identity_evidence",
        score(units),
        hard_cannot_link,
        "distinct identity evidence",
    )
}

fn relation_hit(namespace: &str, operator_id: &str, units: u32) -> EdgeEvidenceHit {
    EdgeEvidenceHit::new(
        ScoreLane::RelationHint,
        namespace,
        operator_id,
        "related_but_not_same",
        score(units),
        false,
        "related entity context only",
    )
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
