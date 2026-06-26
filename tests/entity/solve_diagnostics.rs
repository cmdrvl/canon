#![forbid(unsafe_code)]

use canon::entity::{
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
    graph::{SignedEvidenceGraphInput, SurfaceIncumbentId, build_signed_evidence_graph},
    score::{ScoreLane, ScoreUnits},
    solve::{
        SolveReconciliationConfig, SolveReconciliationState, SolveSurfaceProvenance,
        build_solve_diagnostics,
    },
};

#[test]
fn entity_solve_diagnostics_report_strongest_cuts_and_provenance_counts() {
    let graph = graph_from_records(
        vec![
            build_edge_evidence_record(
                "surf:a",
                "surf:b",
                vec![
                    support_hit("name", "string_similarity", 9_000),
                    anti_merge_hit("tenant_role", "protected_token", 8_750, true),
                ],
            )
            .expect("mixed edge builds"),
        ],
        vec![],
    );
    let diagnostics = build_solve_diagnostics(
        &graph,
        SolveReconciliationConfig::delegate_new_ids(score(5_000)),
        &[
            provenance("surf:a", 10, 2),
            provenance("surf:b", 5, 1),
            provenance("surf:unrelated", 100, 50),
        ],
    );

    assert_eq!(diagnostics.summary["component_count"], 1);
    assert_eq!(diagnostics.summary["review_group_count"], 1);
    assert_eq!(diagnostics.summary["affected_rows"], 15);
    assert_eq!(diagnostics.summary["affected_deals"], 3);

    let component = &diagnostics.components[0];
    assert_eq!(component.state, SolveReconciliationState::Contradiction);
    assert_eq!(component.reason, "hard_cannot_link_constraint");
    assert_eq!(component.affected_rows, 15);
    assert_eq!(component.affected_deals, 3);
    assert_eq!(
        component
            .strongest_positive_cut
            .as_ref()
            .expect("positive cut")
            .score_units,
        score(9_000)
    );
    assert_eq!(
        component
            .strongest_negative_cut
            .as_ref()
            .expect("negative cut")
            .score_units,
        score(8_750)
    );
    assert_eq!(component.score_margin_units, score(250));
    assert_eq!(component.review_priority_reasons, ["hard_cannot_link"]);

    let seed = &diagnostics.review_group_seeds[0];
    assert_eq!(seed.review_group_id, "review:surf_a");
    assert_eq!(
        seed.ambiguity_key,
        "contradiction:hard_cannot_link_constraint"
    );
    assert_eq!(seed.affected_rows, 15);
    assert_eq!(seed.affected_deals, 3);
    assert_eq!(seed.surface_ids, ["surf:a", "surf:b"]);
}

#[test]
fn review_group_seed_ordering_is_component_level_not_raw_row_level() {
    let graph = graph_from_records(
        vec![
            support_record("surf:a", "surf:b", 8_000),
            support_record("surf:c", "surf:d", 8_000),
        ],
        vec![
            incumbent("surf:a", "TNT-A"),
            incumbent("surf:b", "TNT-B"),
            incumbent("surf:c", "TNT-C"),
            incumbent("surf:d", "TNT-D"),
        ],
    );
    let diagnostics = build_solve_diagnostics(
        &graph,
        SolveReconciliationConfig::delegate_new_ids(score(5_000)),
        &[
            provenance("surf:a", 12, 2),
            provenance("surf:b", 8, 2),
            provenance("surf:c", 3, 1),
            provenance("surf:d", 2, 1),
        ],
    );

    assert_eq!(diagnostics.summary["review_group_count"], 2);
    assert_eq!(
        diagnostics
            .review_group_seeds
            .iter()
            .map(|seed| (
                seed.review_group_id.as_str(),
                seed.ambiguity_key.as_str(),
                seed.affected_rows
            ))
            .collect::<Vec<_>>(),
        [
            (
                "review:surf_a",
                "conflict:multiple_incumbent_canonical_ids",
                20
            ),
            (
                "review:surf_c",
                "conflict:multiple_incumbent_canonical_ids",
                5
            ),
        ]
    );
}

#[test]
#[allow(non_snake_case)]
fn EN_S003_relation_hint_only_creates_no_merge_component_or_review_seed() {
    let relation = build_edge_evidence_record(
        "surf:sears",
        "surf:transform",
        vec![EdgeEvidenceHit::new(
            ScoreLane::RelationHint,
            "cmbs_tenant_label.relations",
            "possible_successor_predecessor",
            "related_but_not_same",
            score(10_000),
            false,
            "related entity context only",
        )],
    )
    .expect("relation-only edge builds");
    let graph = graph_from_records(vec![relation], vec![]);
    let diagnostics = build_solve_diagnostics(
        &graph,
        SolveReconciliationConfig::delegate_new_ids(score(5_000)),
        &[
            provenance("surf:sears", 100, 20),
            provenance("surf:transform", 10, 5),
        ],
    );

    assert_eq!(diagnostics.summary["component_count"], 0);
    assert_eq!(diagnostics.summary["review_group_count"], 0);
    assert!(diagnostics.components.is_empty());
    assert!(diagnostics.review_group_seeds.is_empty());
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

fn anti_merge_hit(
    namespace: &str,
    operator_id: &str,
    units: u32,
    hard_cannot_link: bool,
) -> EdgeEvidenceHit {
    EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        namespace,
        operator_id,
        "hard_cannot_link",
        score(units),
        hard_cannot_link,
        "distinct identity evidence",
    )
}

fn incumbent(surface_id: &str, canonical_id: &str) -> SurfaceIncumbentId {
    SurfaceIncumbentId {
        surface_id: surface_id.to_string(),
        canonical_id: canonical_id.to_string(),
    }
}

fn provenance(surface_id: &str, row_count: u64, deal_count: u64) -> SolveSurfaceProvenance {
    SolveSurfaceProvenance {
        surface_id: surface_id.to_string(),
        row_count,
        deal_count,
    }
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
