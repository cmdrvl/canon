#![forbid(unsafe_code)]

use canon::entity::{
    CANON_ENTITY_BLOCK_BUCKET_VERSION,
    block_artifact::{
        CannotLinkAction, CannotLinkValidationHook, CannotLinkValidationStatus,
        EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN, ExactBucketAssertion, ExactBucketDiagnostics,
        ExactBucketMembership, ExactBucketProfile, ExactBucketUpstream, SurfaceIdRange,
    },
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
    graph::{
        EntityEvidenceGraph, ExactBucketSolveAction, SignedEvidenceGraphInput, SurfaceIncumbentId,
        build_signed_evidence_graph,
    },
    score::{ScoreLane, ScoreUnits},
    solve::{
        SolveReconciliationConfig, SolveReconciliationState, SolveSurfaceProvenance,
        build_solve_diagnostics, reconcile_signed_graph_components,
    },
};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::PathBuf};

#[test]
#[allow(non_snake_case)]
fn EN_S001_promotable_cluster_fixture_matches_solver_contract() {
    let graph = graph_from_records(
        vec![support_record("surf:sears", "surf:sears_llc", 8_750)],
        vec![],
    );
    assert_solver_fixture(
        "en_s001_promotable_cluster",
        graph,
        vec![
            provenance("surf:sears", 10, 2),
            provenance("surf:sears_llc", 5, 1),
        ],
    );
}

#[test]
#[allow(non_snake_case)]
fn EN_S002_hard_cannot_link_fixture_never_auto_merges() {
    let graph = graph_from_records(
        vec![support_and_anti_merge_record(
            "surf:sears",
            "surf:sears_auto",
            9_500,
            9_000,
            true,
        )],
        vec![],
    );
    assert_solver_fixture(
        "en_s002_hard_cannot_link",
        graph,
        vec![
            provenance("surf:sears", 10, 2),
            provenance("surf:sears_auto", 4, 1),
        ],
    );
}

#[test]
#[allow(non_snake_case)]
fn EN_S003_relation_hint_only_exports_hint_without_merge_cluster() {
    let graph = graph_from_records(
        vec![relation_record("surf:sears", "surf:transform", 10_000)],
        vec![],
    );
    assert_solver_fixture(
        "en_s003_relation_hint_only",
        graph,
        vec![
            provenance("surf:sears", 100, 20),
            provenance("surf:transform", 10, 5),
        ],
    );
}

#[test]
#[allow(non_snake_case)]
fn EN_S004_single_incumbent_fixture_inherits_existing_id() {
    let graph = graph_from_records(
        vec![support_record("surf:sears", "surf:sears_llc", 9_250)],
        vec![incumbent("surf:sears", "TNT-SEARS")],
    );
    assert_solver_fixture(
        "en_s004_single_incumbent",
        graph,
        vec![
            provenance("surf:sears", 10, 2),
            provenance("surf:sears_llc", 5, 1),
        ],
    );
}

#[test]
#[allow(non_snake_case)]
fn EN_S005_solver_incumbent_conflicts_emit_review_material_without_score_tiebreak() {
    let graph = graph_from_records(
        vec![support_and_anti_merge_record(
            "surf:kmart",
            "surf:sears",
            9_500,
            8_000,
            false,
        )],
        vec![
            incumbent("surf:sears", "TNT-SEARS"),
            incumbent("surf:kmart", "TNT-KMART"),
        ],
    );
    assert_solver_fixture(
        "en_s005_multiple_incumbents",
        graph,
        vec![
            provenance("surf:kmart", 6, 1),
            provenance("surf:sears", 10, 2),
        ],
    );
}

fn assert_solver_fixture(
    fixture_dir: &str,
    graph: EntityEvidenceGraph,
    provenance: Vec<SolveSurfaceProvenance>,
) {
    let fixture = load_fixture(fixture_dir);
    assert_graph_counts(&fixture.fixture_id, &graph, &fixture.graph);

    let config = SolveReconciliationConfig::delegate_new_ids(score(5_000));
    let reconciliation = reconcile_signed_graph_components(&graph, config);
    assert_eq!(
        reconciliation.summary, fixture.reconciliation_summary,
        "{} reconciliation summary",
        fixture.fixture_id
    );

    let diagnostics = build_solve_diagnostics(&graph, config, &provenance);
    assert_eq!(
        diagnostics.summary, fixture.diagnostics_summary,
        "{} diagnostics summary",
        fixture.fixture_id
    );
    assert_eq!(
        diagnostics.review_group_seeds.len() as u64,
        fixture.expected_review_group_count,
        "{} review group seed count",
        fixture.fixture_id
    );

    match fixture.expected_state {
        Some(expected_state) => {
            assert_eq!(
                reconciliation.decisions.len(),
                1,
                "{} decision count",
                fixture.fixture_id
            );
            let decision = &reconciliation.decisions[0];
            assert_eq!(
                decision.state, expected_state,
                "{} state",
                fixture.fixture_id
            );
            assert_eq!(
                decision.reason,
                fixture.expected_reason.as_deref().expect("fixture reason"),
                "{} reason",
                fixture.fixture_id
            );
            assert_eq!(
                decision.surface_ids, fixture.expected_surface_ids,
                "{} surfaces",
                fixture.fixture_id
            );
            assert_eq!(
                decision.incumbent_canonical_ids, fixture.expected_incumbent_ids,
                "{} incumbent ids",
                fixture.fixture_id
            );
            assert_eq!(
                decision.canonical_id.as_deref(),
                fixture.expected_canonical_id.as_deref(),
                "{} canonical id",
                fixture.fixture_id
            );
            assert_eq!(
                decision.candidate_id.as_deref(),
                fixture.expected_candidate_id.as_deref(),
                "{} candidate id",
                fixture.fixture_id
            );
            assert_eq!(
                decision.support_score_units.as_u32(),
                fixture
                    .expected_support_score_units
                    .expect("fixture support score"),
                "{} support score",
                fixture.fixture_id
            );
            assert_eq!(
                decision.adjusted_support_score_units.as_u32(),
                fixture
                    .expected_adjusted_support_score_units
                    .expect("fixture adjusted support score"),
                "{} adjusted support score",
                fixture.fixture_id
            );
            assert_eq!(
                decision.hard_cannot_link_count, fixture.expected_hard_cannot_link_count,
                "{} hard cannot-link count",
                fixture.fixture_id
            );
            assert_eq!(
                decision.soft_anti_merge_warning_count,
                fixture.expected_soft_anti_merge_warning_count,
                "{} soft anti-merge warning count",
                fixture.fixture_id
            );
            if expected_state == SolveReconciliationState::Conflict {
                assert!(
                    diagnostics.review_group_seeds[0]
                        .priority_reasons
                        .iter()
                        .any(|reason| reason == "incumbent_conflict"),
                    "{} conflict emits review material",
                    fixture.fixture_id
                );
            }
        }
        None => assert!(
            reconciliation.decisions.is_empty(),
            "{} relation-only fixture must not create a merge component",
            fixture.fixture_id
        ),
    }

    if let Some(exact_bucket) = fixture.exact_bucket.as_ref() {
        assert_exact_bucket_fixture(&fixture.fixture_id, exact_bucket);
    }
}

fn assert_graph_counts(fixture_id: &str, graph: &EntityEvidenceGraph, expected: &GraphCounts) {
    assert_eq!(
        graph.diagnostics.support_edge_count, expected.support_edge_count,
        "{fixture_id} support edge count"
    );
    assert_eq!(
        graph.diagnostics.cannot_link_edge_count, expected.cannot_link_edge_count,
        "{fixture_id} cannot-link edge count"
    );
    assert_eq!(
        graph.diagnostics.hard_cannot_link_edge_count, expected.hard_cannot_link_edge_count,
        "{fixture_id} hard cannot-link edge count"
    );
    assert_eq!(
        graph.diagnostics.soft_cannot_link_edge_count, expected.soft_cannot_link_edge_count,
        "{fixture_id} soft cannot-link edge count"
    );
    assert_eq!(
        graph.diagnostics.relation_hint_edge_count, expected.relation_hint_edge_count,
        "{fixture_id} relation hint edge count"
    );
    assert_eq!(
        graph.diagnostics.exact_bucket_hyperedge_count, expected.exact_bucket_hyperedge_count,
        "{fixture_id} exact bucket hyperedge count"
    );
}

fn assert_exact_bucket_fixture(fixture_id: &str, expected: &ExactBucketExpectation) {
    let assertion = exact_bucket_assertion(
        &expected.bucket_id,
        ExactBucketMembership {
            surface_ids: vec![],
            surface_ranges: vec![SurfaceIdRange {
                start_surface_id: expected.range_start_surface_id.clone(),
                end_surface_id: expected.range_end_surface_id.clone(),
                member_count: expected.member_count,
            }],
        },
        expected.member_count,
    );
    let mut graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records: vec![],
        exact_bucket_assertions: vec![assertion],
        incumbent_ids: vec![],
    })
    .expect("exact bucket graph builds");
    if expected.hard_cannot_link_count > 0 {
        graph.add_hard_cannot_link(
            expected.range_start_surface_id.clone(),
            expected.range_end_surface_id.clone(),
        );
    }

    assert_eq!(
        graph.diagnostics.materialized_exact_bucket_pair_count, 0,
        "{fixture_id} exact bucket materialized pairs"
    );
    assert_eq!(
        graph.diagnostics.exact_bucket_membership_record_count, expected.membership_record_count,
        "{fixture_id} exact bucket membership record count"
    );
    assert_eq!(
        graph.diagnostics.theoretical_exact_bucket_pair_count, expected.theoretical_pair_count,
        "{fixture_id} exact bucket theoretical pair count"
    );

    let report = graph.solve_exact_bucket_hyperedges();
    assert_eq!(report.hyperedge_count, 1, "{fixture_id} hyperedge count");
    assert_eq!(
        report.membership_record_count, expected.membership_record_count,
        "{fixture_id} solve membership record count"
    );
    assert_eq!(
        report.theoretical_pair_count, expected.theoretical_pair_count,
        "{fixture_id} solve theoretical pair count"
    );
    assert_eq!(
        report.expanded_pair_count, expected.expanded_pair_count,
        "{fixture_id} solve expanded pair count"
    );
    assert_eq!(
        report.decisions[0].action, expected.action,
        "{fixture_id} exact bucket solve action"
    );
    assert_eq!(
        report.decisions[0].hard_cannot_link_count, expected.hard_cannot_link_count,
        "{fixture_id} exact bucket hard cannot-link count"
    );
}

fn load_fixture(fixture_dir: &str) -> SolverFixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity/solve")
        .join(fixture_dir)
        .join("expected.json");
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn graph_from_records(
    edge_records: Vec<EdgeEvidenceRecord>,
    incumbent_ids: Vec<SurfaceIncumbentId>,
) -> EntityEvidenceGraph {
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

fn support_and_anti_merge_record(
    left_surface_id: &str,
    right_surface_id: &str,
    support_units: u32,
    anti_merge_units: u32,
    hard_cannot_link: bool,
) -> EdgeEvidenceRecord {
    build_edge_evidence_record(
        left_surface_id,
        right_surface_id,
        vec![
            support_hit("name", "string_similarity", support_units),
            anti_merge_hit(
                "cmbs_tenant_label.distinct",
                "operator_patch",
                anti_merge_units,
                hard_cannot_link,
            ),
        ],
    )
    .expect("support plus anti-merge edge builds")
}

fn relation_record(
    left_surface_id: &str,
    right_surface_id: &str,
    units: u32,
) -> EdgeEvidenceRecord {
    build_edge_evidence_record(
        left_surface_id,
        right_surface_id,
        vec![relation_hit(
            "cmbs_tenant_label.relations",
            "related_brand_family",
            units,
        )],
    )
    .expect("relation edge builds")
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

#[derive(Debug, Deserialize)]
struct SolverFixture {
    fixture_id: String,
    expected_state: Option<SolveReconciliationState>,
    expected_reason: Option<String>,
    expected_surface_ids: Vec<String>,
    expected_incumbent_ids: Vec<String>,
    expected_canonical_id: Option<String>,
    expected_candidate_id: Option<String>,
    expected_support_score_units: Option<u32>,
    expected_adjusted_support_score_units: Option<u32>,
    expected_hard_cannot_link_count: u64,
    expected_soft_anti_merge_warning_count: u64,
    expected_review_group_count: u64,
    graph: GraphCounts,
    reconciliation_summary: BTreeMap<String, u64>,
    diagnostics_summary: BTreeMap<String, u64>,
    exact_bucket: Option<ExactBucketExpectation>,
}

#[derive(Debug, Deserialize)]
struct GraphCounts {
    support_edge_count: u64,
    cannot_link_edge_count: u64,
    hard_cannot_link_edge_count: u64,
    soft_cannot_link_edge_count: u64,
    relation_hint_edge_count: u64,
    exact_bucket_hyperedge_count: u64,
}

#[derive(Debug, Deserialize)]
struct ExactBucketExpectation {
    bucket_id: String,
    range_start_surface_id: String,
    range_end_surface_id: String,
    member_count: u64,
    membership_record_count: u64,
    theoretical_pair_count: u64,
    expanded_pair_count: u64,
    action: ExactBucketSolveAction,
    hard_cannot_link_count: u64,
}
