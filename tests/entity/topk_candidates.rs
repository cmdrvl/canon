use canon::entity::topk::{
    CANON_ENTITY_TOPK_VERSION, TopKCandidateInput, TopKConfig, TopKDropReason,
    prune_top_k_candidates,
};

#[test]
fn topk_candidates_deterministic() {
    let candidates = candidate_fixture();
    let config = TopKConfig::new("cmbs_tenant_label", "ngram_topk:tenant_core", 3)
        .with_candidate_cap(2)
        .with_score_floor_units(200);

    let first = prune_top_k_candidates(config.clone(), candidates.clone());
    let mut reversed = candidates;
    reversed.reverse();
    let second = prune_top_k_candidates(config, reversed);

    assert_eq!(first, second);
    assert_eq!(first.diagnostics.version, CANON_ENTITY_TOPK_VERSION);
    assert_eq!(first.diagnostics.profile_id, "cmbs_tenant_label");
    assert_eq!(first.diagnostics.operator_id, "ngram_topk:tenant_core");
    assert_eq!(first.diagnostics.input_candidate_count, 5);
    assert_eq!(first.diagnostics.eligible_candidate_count, 4);
    assert_eq!(first.diagnostics.emitted_candidate_count, 2);
    assert_eq!(first.diagnostics.dropped_candidate_count, 3);
    assert_eq!(first.diagnostics.dropped_by_score_floor_count, 1);
    assert_eq!(first.diagnostics.dropped_by_candidate_cap_count, 2);
    assert_eq!(first.diagnostics.dropped_by_topk_count, 0);
    assert!(first.diagnostics.candidate_cap_exceeded);
    assert!(!first.diagnostics.topk_exceeded);

    assert_eq!(
        first
            .candidates
            .iter()
            .map(|candidate| (
                candidate.rank,
                candidate.candidate_surface_id.as_str(),
                candidate.score_units
            ))
            .collect::<Vec<_>>(),
        [(1, "surface-001", 950), (2, "surface-002", 900)]
    );
    assert_eq!(
        first
            .dropped
            .iter()
            .map(|drop| (drop.candidate_surface_id.as_str(), drop.reason))
            .collect::<Vec<_>>(),
        [
            ("surface-005", TopKDropReason::BelowScoreFloor),
            ("surface-003", TopKDropReason::CandidateCap),
            ("surface-004", TopKDropReason::CandidateCap),
        ]
    );
}

#[test]
fn topk_tie_order_stable() {
    let result = prune_top_k_candidates(
        TopKConfig::new("regab_firm_identity", "token_overlap:firm_core", 4),
        [
            candidate("query", "surface-004", "zeta bank", 700),
            candidate("query", "surface-003", "midland loan services", 900),
            candidate("query", "surface-001", "midland loan services", 900),
            candidate("query", "surface-002", "alpha servicing", 900),
            candidate("query", "surface-005", "midland loan services", 900),
        ],
    );

    assert_eq!(
        result
            .candidates
            .iter()
            .map(|candidate| (
                candidate.rank,
                candidate.candidate_surface_id.as_str(),
                candidate.normalized_key.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            (1, "surface-002", "alpha servicing"),
            (2, "surface-001", "midland loan services"),
            (3, "surface-003", "midland loan services"),
            (4, "surface-005", "midland loan services"),
        ]
    );
    assert_eq!(result.diagnostics.dropped_by_topk_count, 1);
    assert_eq!(result.dropped[0].candidate_surface_id, "surface-004");
    assert_eq!(result.dropped[0].reason, TopKDropReason::TopKLimit);
}

fn candidate_fixture() -> Vec<TopKCandidateInput> {
    vec![
        candidate("query", "surface-004", "gamma tenant", 700),
        candidate("query", "surface-002", "alpha tenant", 900),
        candidate("query", "surface-005", "delta tenant", 100),
        candidate("query", "surface-003", "alpha tenant", 900),
        candidate("query", "surface-001", "beta tenant", 950),
    ]
}

fn candidate(
    query_surface_id: &str,
    candidate_surface_id: &str,
    normalized_key: &str,
    score_units: u32,
) -> TopKCandidateInput {
    TopKCandidateInput::new(
        query_surface_id,
        candidate_surface_id,
        normalized_key,
        score_units,
    )
}
