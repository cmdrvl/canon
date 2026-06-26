#![forbid(unsafe_code)]

use canon::entity::{
    edge::{EdgeEvidenceHit, build_edge_evidence_record},
    graph::{SignedEvidenceGraphInput, build_signed_evidence_graph},
    score::{ScoreLane, ScoreUnits},
    solve::{SolveComponentAction, evaluate_signed_graph_components},
};

#[test]
fn entity_solve_cannot_link_high_support_never_auto_merges() {
    let support_ab = build_edge_evidence_record(
        "surf:a",
        "surf:b",
        vec![support_hit("name", "jaro_winkler", 10_000)],
    )
    .expect("support edge builds");
    let support_bc = build_edge_evidence_record(
        "surf:b",
        "surf:c",
        vec![support_hit("token", "tfidf_cosine", 9_500)],
    )
    .expect("support edge builds");
    let hard_ac = build_edge_evidence_record(
        "surf:a",
        "surf:c",
        vec![anti_merge_hit(
            "tenant_role",
            "protected_token",
            "hard_cannot_link",
            8_750,
            true,
        )],
    )
    .expect("hard cannot-link edge builds");

    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records: vec![support_bc.clone(), hard_ac.clone(), support_ab.clone()],
        exact_bucket_assertions: vec![],
        incumbent_ids: vec![],
    })
    .expect("signed graph builds");
    let report = evaluate_signed_graph_components(&graph);

    assert_eq!(report.summary["component_count"], 1);
    assert_eq!(report.summary["auto_merge_candidate_count"], 0);
    assert_eq!(report.summary["contradiction_count"], 1);
    assert_eq!(report.summary["hard_cannot_link_count"], 1);

    let decision = &report.components[0];
    assert_eq!(decision.component_id, "component:surf:a");
    assert_eq!(decision.action, SolveComponentAction::Contradiction);
    assert_eq!(
        decision.reason,
        "hard_cannot_link_inside_positive_component"
    );
    assert_eq!(decision.surface_ids, ["surf:a", "surf:b", "surf:c"]);
    assert_eq!(decision.support_edge_count, 2);
    assert_eq!(decision.review_priority_reasons, ["hard_cannot_link"]);
    assert_eq!(
        decision
            .strongest_positive_cut
            .as_ref()
            .expect("positive cut")
            .score_units,
        score(10_000)
    );

    let violation = &decision.hard_cannot_link_violations[0];
    assert_eq!(violation.left_surface_id, "surf:a");
    assert_eq!(violation.right_surface_id, "surf:c");
    assert!(violation.hard_cannot_link);
    assert_eq!(violation.score_units, score(8_750));
    assert_eq!(violation.evidence_reason_codes, ["hard_cannot_link"]);
    assert_eq!(violation.evidence_operator_ids, ["protected_token"]);
    assert_eq!(
        decision
            .strongest_negative_cut
            .as_ref()
            .expect("negative cut")
            .score_units,
        score(8_750)
    );

    let reversed = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records: vec![support_ab, hard_ac, support_bc],
        exact_bucket_assertions: vec![],
        incumbent_ids: vec![],
    })
    .expect("reversed signed graph builds");
    let reversed_report = evaluate_signed_graph_components(&reversed);
    assert_eq!(
        serde_json::to_vec(&report).expect("report serializes"),
        serde_json::to_vec(&reversed_report).expect("reversed report serializes")
    );
}

#[test]
#[allow(non_snake_case)]
fn EN_S002_soft_anti_merge_lowers_confidence_and_raises_review_priority() {
    let record = build_edge_evidence_record(
        "surf:sears",
        "surf:sears_auto",
        vec![
            anti_merge_hit(
                "tenant_role",
                "related_distinct_phrase",
                "soft_distinct_phrase",
                2_500,
                false,
            ),
            support_hit("name", "string_similarity", 7_000),
        ],
    )
    .expect("mixed edge builds");
    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records: vec![record],
        exact_bucket_assertions: vec![],
        incumbent_ids: vec![],
    })
    .expect("signed graph builds");
    let report = evaluate_signed_graph_components(&graph);

    assert_eq!(report.summary["component_count"], 1);
    assert_eq!(report.summary["review_component_count"], 1);
    assert_eq!(report.summary["contradiction_count"], 0);
    assert_eq!(report.summary["soft_anti_merge_warning_count"], 1);

    let decision = &report.components[0];
    assert_eq!(decision.action, SolveComponentAction::Review);
    assert_eq!(decision.reason, "soft_anti_merge_inside_positive_component");
    assert_eq!(decision.review_priority_reasons, ["soft_anti_merge"]);
    assert_eq!(decision.raw_support_score_units, score(7_000));
    assert_eq!(decision.adjusted_support_score_units, score(4_500));
    assert!(decision.hard_cannot_link_violations.is_empty());
    assert_eq!(decision.soft_anti_merge_warnings.len(), 1);
    assert!(!decision.soft_anti_merge_warnings[0].hard_cannot_link);
    assert_eq!(
        decision.soft_anti_merge_warnings[0].evidence_reason_codes,
        ["soft_distinct_phrase"]
    );
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

fn anti_merge_hit(
    namespace: &str,
    operator_id: &str,
    reason_code: &str,
    units: u32,
    hard_cannot_link: bool,
) -> EdgeEvidenceHit {
    EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        namespace,
        operator_id,
        reason_code,
        score(units),
        hard_cannot_link,
        "distinct identity evidence",
    )
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
