#![forbid(unsafe_code)]

use canon::entity::{
    CANON_ENTITY_BLOCK_BUCKET_VERSION,
    block_artifact::{
        CannotLinkAction, CannotLinkValidationHook, CannotLinkValidationStatus,
        EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN, ExactBucketAssertion, ExactBucketDiagnostics,
        ExactBucketMembership, ExactBucketProfile, ExactBucketUpstream,
    },
    graph::{EntityEvidenceGraph, ExactBucketSolveAction, SurfacePair},
};
use std::collections::BTreeMap;

#[test]
fn solve_exact_bucket_hyperedge_consumes_compact_cluster_without_pair_expansion() {
    let assertion = exact_bucket_assertion(
        "bucket:tenant_core:sears",
        (0..8_000)
            .map(|ordinal| format!("surf:sears:{ordinal:04}"))
            .collect(),
        8_000,
    );
    let graph = EntityEvidenceGraph::from_exact_bucket_assertions(&[assertion]);
    let report = graph.solve_exact_bucket_hyperedges();

    assert_eq!(report.hyperedge_count, 1);
    assert_eq!(report.membership_record_count, 8_000);
    assert_eq!(report.theoretical_pair_count, 31_996_000);
    assert_eq!(report.expanded_pair_count, 0);
    assert_eq!(
        report.decisions[0].action,
        ExactBucketSolveAction::MergeCluster
    );
    assert_eq!(report.decisions[0].expanded_pair_count, 0);
    assert_eq!(report.decisions[0].hard_cannot_link_count, 0);
    assert_eq!(report.decisions[0].reason, "exact_bucket_cluster_evidence");
}

#[test]
fn cannot_link_veto_inside_exact_bucket() {
    let assertion = exact_bucket_assertion(
        "bucket:tenant_core:sears-family",
        vec![
            "surf:sears".to_string(),
            "surf:sears_auto".to_string(),
            "surf:sears_llc".to_string(),
        ],
        3,
    );
    let mut graph = EntityEvidenceGraph::from_exact_bucket_assertions(&[assertion]);
    graph.add_hard_cannot_link("surf:sears", "surf:sears_auto");
    let report = graph.solve_exact_bucket_hyperedges();

    assert_eq!(report.expanded_pair_count, 0);
    assert_eq!(
        report.decisions[0].action,
        ExactBucketSolveAction::ReviewContradiction
    );
    assert_eq!(
        report.decisions[0].reason,
        "hard_cannot_link_inside_exact_bucket"
    );
    assert_eq!(report.decisions[0].hard_cannot_link_count, 1);
    assert_eq!(
        report.decisions[0].hard_cannot_links,
        [SurfacePair::new("surf:sears", "surf:sears_auto").expect("ordered pair")]
    );
}

fn exact_bucket_assertion(
    bucket_id: &str,
    surface_ids: Vec<String>,
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
        membership: ExactBucketMembership {
            surface_ids,
            surface_ranges: vec![],
        },
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
