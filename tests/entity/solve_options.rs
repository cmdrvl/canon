#![forbid(unsafe_code)]

//! Opt-in solver modes declared by a strategy `solve:` section (bd-i6o7,
//! bd-vula). Defaults must stay the historical behavior.

use canon::entity::{
    edge::{EdgeEvidenceHit, build_edge_evidence_record},
    graph::{EntityEvidenceGraph, SignedEvidenceGraphInput, build_signed_evidence_graph},
    score::{ScoreLane, ScoreUnits},
    solve::{SolveComponentAction, SolveGraphOptions, evaluate_signed_graph_components},
};

fn support(units: u32) -> EdgeEvidenceHit {
    EdgeEvidenceHit::new(
        ScoreLane::Support,
        "ns",
        "name",
        "positive_identity_evidence",
        ScoreUnits::from_scaled(units).unwrap(),
        false,
        "positive identity evidence",
    )
}

fn hard_distinct() -> EdgeEvidenceHit {
    EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        "ns",
        "distinct",
        "protected_token_conflict",
        ScoreUnits::from_scaled(10_000).unwrap(),
        true,
        "distinct identity evidence",
    )
}

fn graph(pairs: &[(&str, &str, Vec<EdgeEvidenceHit>)]) -> EntityEvidenceGraph {
    build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records: pairs
            .iter()
            .map(|(left, right, hits)| {
                build_edge_evidence_record(*left, *right, hits.clone()).unwrap()
            })
            .collect(),
        exact_bucket_assertions: vec![],
        incumbent_ids: vec![],
    })
    .unwrap()
}

fn components(graph: &EntityEvidenceGraph) -> Vec<(Vec<String>, SolveComponentAction)> {
    let mut report = evaluate_signed_graph_components(graph)
        .components
        .into_iter()
        .map(|component| (component.surface_ids, component.action))
        .collect::<Vec<_>>();
    report.sort_by(|left, right| left.0.cmp(&right.0));
    report
}

fn sibling_graph() -> EntityEvidenceGraph {
    // III~IV, IV~XIV, III~XIV are all similar by name but pairwise distinct;
    // "II-b" is a spelling variant of III-a that must stay merged with it.
    graph(&[
        ("III-a", "IV", vec![support(9000), hard_distinct()]),
        ("IV", "XIV", vec![support(9000), hard_distinct()]),
        ("III-a", "XIV", vec![support(9000), hard_distinct()]),
        ("III-a", "III-b", vec![support(9500)]),
        ("III-b", "IV", vec![support(8800), hard_distinct()]),
    ])
}

#[test]
fn default_keeps_the_contradiction_component() {
    let graph = sibling_graph();
    let result = components(&graph);
    assert_eq!(result.len(), 1, "{result:?}");
    assert_eq!(result[0].1, SolveComponentAction::Contradiction);
}

#[test]
fn split_direct_cuts_only_directly_contradicted_edges_and_keeps_variants_together() {
    let split = sibling_graph().split_direct_hard_cannot_links();
    assert_eq!(split.split_direct_cut_edge_count, 4);
    assert_eq!(split.diagnostics.support_edge_count, 1);
    let result = components(&split);
    let surfaces = result
        .iter()
        .map(|(ids, action)| (ids.clone(), *action))
        .collect::<Vec<_>>();
    // The spelling variants stay merged; each distinct sibling stands alone;
    // nothing is left as a contradiction.
    assert_eq!(
        surfaces,
        vec![
            (
                vec!["III-a".to_string(), "III-b".to_string()],
                SolveComponentAction::AutoMergeCandidate
            ),
            (
                vec!["IV".to_string()],
                SolveComponentAction::AutoMergeCandidate
            ),
            (
                vec!["XIV".to_string()],
                SolveComponentAction::AutoMergeCandidate
            ),
        ]
    );
    // A naive implementation that dropped every supported edge in a
    // contradicted component would also have split III-a from III-b.
}

#[test]
fn a_transitive_conflict_survives_the_cut_as_a_contradiction() {
    // A~B and B~C are fine; A and C are distinct but have no direct support
    // edge, so there is nothing to cut and the conflict must stay visible.
    let split = graph(&[
        ("A", "B", vec![support(9000)]),
        ("B", "C", vec![support(9000)]),
        ("A", "C", vec![hard_distinct()]),
    ])
    .split_direct_hard_cannot_links();
    assert_eq!(split.split_direct_cut_edge_count, 0);
    let result = components(&split);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, vec!["A", "B", "C"]);
    assert_eq!(result[0].1, SolveComponentAction::Contradiction);
}

#[test]
fn split_direct_is_order_independent() {
    let forward = sibling_graph().split_direct_hard_cannot_links();
    let mut records = vec![
        ("III-b", "IV", vec![support(8800), hard_distinct()]),
        ("III-a", "XIV", vec![support(9000), hard_distinct()]),
        ("III-a", "III-b", vec![support(9500)]),
        ("IV", "XIV", vec![support(9000), hard_distinct()]),
        ("III-a", "IV", vec![support(9000), hard_distinct()]),
    ];
    let reversed = graph(&records).split_direct_hard_cannot_links();
    records.reverse();
    let shuffled = graph(&records).split_direct_hard_cannot_links();
    assert_eq!(components(&forward), components(&reversed));
    assert_eq!(components(&forward), components(&shuffled));
}

#[test]
fn standalone_surfaces_only_appear_when_included() {
    // "lonely" has only a hard cannot-link record: it is in the graph but has
    // no support edge, so it is absent from the solve by default.
    let base = graph(&[
        ("A", "B", vec![support(9000)]),
        ("A", "lonely", vec![hard_distinct()]),
    ]);
    assert_eq!(components(&base).len(), 1);
    let included = base.include_standalone_surfaces();
    let result = components(&included);
    assert_eq!(
        result
            .iter()
            .map(|(ids, _)| ids.clone())
            .collect::<Vec<_>>(),
        vec![
            vec!["A".to_string(), "B".to_string()],
            vec!["lonely".to_string()]
        ]
    );
}

#[test]
fn solve_section_parsing_defaults_and_refusals() {
    assert_eq!(
        SolveGraphOptions::from_strategy_document("strategy_id: s\n").unwrap(),
        SolveGraphOptions::default()
    );
    let both = SolveGraphOptions::from_strategy_document(
        "solve:\n  hard_cannot_link: split_direct\n  standalone_surfaces: include\n",
    )
    .unwrap();
    assert!(both.split_direct_hard_cannot_links && both.include_standalone_surfaces);
    let explicit_defaults = SolveGraphOptions::from_strategy_document(
        "solve:\n  hard_cannot_link: contradiction\n  standalone_surfaces: omit\n",
    )
    .unwrap();
    assert_eq!(explicit_defaults, SolveGraphOptions::default());
    for bad in [
        "solve:\n  hard_cannot_link: yolo\n",
        "solve:\n  standalone: include\n",
        "solve: split_direct\n",
        "solve:\n  standalone_surfaces: true\n",
    ] {
        let refusal = SolveGraphOptions::from_strategy_document(bad).unwrap_err();
        assert_eq!(
            refusal.detail["supported"]["hard_cannot_link"][1], "split_direct",
            "{bad}"
        );
    }
}
