#![forbid(unsafe_code)]

use canon::entity::{
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
    graph::{SignedEvidenceGraphInput, SurfaceIncumbentId, build_signed_evidence_graph},
    score::{ScoreLane, ScoreUnits},
    solve::{
        SolveNewIdPolicy, SolveReconciliationConfig, SolveReconciliationState,
        reconcile_signed_graph_components,
    },
};

#[test]
#[allow(non_snake_case)]
fn EN_S001_no_incumbent_component_is_promotable_when_policy_delegates_new_ids() {
    let graph = graph_from_records(
        vec![support_record("surf:sears", "surf:sears_llc", 8_750)],
        vec![],
    );
    let report = reconcile_signed_graph_components(
        &graph,
        SolveReconciliationConfig::delegate_new_ids(score(5_000)),
    );

    assert_eq!(report.summary["component_count"], 1);
    assert_eq!(report.summary["promotable_new_count"], 1);
    assert_eq!(report.summary["resolved_existing_count"], 0);
    assert_eq!(report.summary["escrow_count"], 0);

    let decision = &report.decisions[0];
    assert_eq!(decision.state, SolveReconciliationState::PromotableNew);
    assert_eq!(decision.reason, "new_id_delegated_to_promotion_policy");
    assert_eq!(decision.canonical_id, None);
    assert_eq!(
        decision.candidate_id.as_deref(),
        Some("candidate:surf_sears")
    );
    assert_eq!(decision.support_score_units, score(8_750));
    assert_eq!(decision.adjusted_support_score_units, score(8_750));
}

#[test]
#[allow(non_snake_case)]
fn EN_S004_single_incumbent_component_inherits_existing_id() {
    let graph = graph_from_records(
        vec![support_record("surf:sears", "surf:sears_llc", 9_250)],
        vec![SurfaceIncumbentId {
            surface_id: "surf:sears".to_string(),
            canonical_id: "TNT-SEARS".to_string(),
        }],
    );
    let report = reconcile_signed_graph_components(
        &graph,
        SolveReconciliationConfig::delegate_new_ids(score(5_000)),
    );

    assert_eq!(report.summary["resolved_existing_count"], 1);
    assert_eq!(report.summary["promotable_new_count"], 0);
    let decision = &report.decisions[0];
    assert_eq!(decision.state, SolveReconciliationState::ResolvedExisting);
    assert_eq!(decision.reason, "single_incumbent_inherits_existing_id");
    assert_eq!(decision.canonical_id.as_deref(), Some("TNT-SEARS"));
    assert_eq!(decision.candidate_id, None);
    assert_eq!(decision.incumbent_canonical_ids, ["TNT-SEARS"]);
}

#[test]
#[allow(non_snake_case)]
fn EN_S005_multiple_incumbents_conflict_without_score_tiebreaking() {
    let graph = graph_from_records(
        vec![support_record("surf:kmart", "surf:sears", 10_000)],
        vec![
            SurfaceIncumbentId {
                surface_id: "surf:sears".to_string(),
                canonical_id: "TNT-SEARS".to_string(),
            },
            SurfaceIncumbentId {
                surface_id: "surf:kmart".to_string(),
                canonical_id: "TNT-KMART".to_string(),
            },
        ],
    );
    let report = reconcile_signed_graph_components(
        &graph,
        SolveReconciliationConfig::delegate_new_ids(score(5_000)),
    );

    assert_eq!(report.summary["conflict_count"], 1);
    assert_eq!(report.summary["resolved_existing_count"], 0);
    let decision = &report.decisions[0];
    assert_eq!(decision.state, SolveReconciliationState::Conflict);
    assert_eq!(decision.reason, "multiple_incumbent_canonical_ids");
    assert_eq!(decision.canonical_id, None);
    assert_eq!(decision.candidate_id, None);
    assert_eq!(decision.incumbent_canonical_ids, ["TNT-KMART", "TNT-SEARS"]);
}

#[test]
fn low_evidence_without_incumbent_goes_to_escrow() {
    let graph = graph_from_records(
        vec![support_record("surf:weak-a", "surf:weak-b", 3_000)],
        vec![],
    );
    let report = reconcile_signed_graph_components(
        &graph,
        SolveReconciliationConfig {
            minimum_adjusted_support_units: score(5_000),
            new_id_policy: SolveNewIdPolicy::DelegateToPromotion,
        },
    );

    assert_eq!(report.summary["escrow_count"], 1);
    assert_eq!(report.summary["promotable_new_count"], 0);
    let decision = &report.decisions[0];
    assert_eq!(decision.state, SolveReconciliationState::Escrow);
    assert_eq!(decision.reason, "below_support_threshold");
    assert_eq!(decision.canonical_id, None);
    assert_eq!(decision.candidate_id, None);
}

fn graph_from_records(
    edge_records: Vec<EdgeEvidenceRecord>,
    incumbent_ids: Vec<SurfaceIncumbentId>,
) -> canon::entity::graph::EntityEvidenceGraph {
    build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records,
        exact_bucket_assertions: vec![],
        incumbent_ids,
    })
    .expect("signed graph builds")
}

fn support_record(left_surface_id: &str, right_surface_id: &str, units: u32) -> EdgeEvidenceRecord {
    build_edge_evidence_record(
        left_surface_id,
        right_surface_id,
        vec![support_hit("name", "string_similarity", units)],
    )
    .expect("support edge builds")
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

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
