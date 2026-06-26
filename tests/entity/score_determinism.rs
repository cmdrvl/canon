use canon::entity::score::{
    CANON_ENTITY_SCORE_VERSION, CandidateScoreDecisionReason, ScoreContribution, ScoreLane,
    ScoreOptimizationHints, ScoreThreshold, ScoreUnits, ScoredCandidate, accepted_candidate_ids,
    accepted_candidate_ids_with_hints, accumulate_score_units, evaluate_candidate_score,
    sort_scored_candidates, top_score_contributors,
};

#[test]
fn score_unit_contract() {
    assert_eq!(ScoreUnits::from_scaled(10_001), None);
    assert_eq!(ScoreUnits::from_scaled(10_000), Some(score(10_000)));
    assert_eq!(ScoreUnits::from_ratio_parts(1, 20_000), Some(score(1)));
    assert_eq!(ScoreUnits::from_ratio_parts(1, 2), Some(score(5_000)));
    assert_eq!(ScoreUnits::from_ratio_parts(2, 1), Some(score(10_000)));
    assert_eq!(ScoreUnits::from_ratio_parts(1, 0), None);
    assert_eq!(ScoreUnits::from_f64_ratio(-1.0), score(0));
    assert_eq!(ScoreUnits::from_f64_ratio(0.000_04), score(0));
    assert_eq!(ScoreUnits::from_f64_ratio(0.000_05), score(1));
    assert_eq!(ScoreUnits::from_f64_ratio(0.818_181_818), score(8_182));
    assert_eq!(ScoreUnits::from_f64_ratio(0.999_95), score(10_000));
    assert_eq!(ScoreUnits::from_f64_ratio(f64::NAN), score(0));

    let threshold = ScoreThreshold::new(score(9_000));
    assert!(!threshold.accepts(score(8_999)));
    assert!(threshold.accepts(score(9_000)));

    let top = top_score_contributors(
        &[
            contribution(ScoreLane::Support, "metric:b", "jw", 7_000),
            contribution(ScoreLane::Support, "metric:a", "jw", 7_000),
            contribution(ScoreLane::AntiMerge, "distinct:a", "operator", 9_500),
            contribution(ScoreLane::RelationHint, "relation:a", "brand_family", 9_500),
        ],
        4,
    );
    assert_eq!(
        top.iter()
            .map(|contribution| (
                contribution.lane,
                contribution.source_id.as_str(),
                contribution.reason_code.as_str(),
                contribution.score_units.as_u32()
            ))
            .collect::<Vec<_>>(),
        [
            (ScoreLane::AntiMerge, "distinct:a", "operator", 9_500),
            (ScoreLane::RelationHint, "relation:a", "brand_family", 9_500),
            (ScoreLane::Support, "metric:a", "jw", 7_000),
            (ScoreLane::Support, "metric:b", "jw", 7_000),
        ]
    );

    let blocked = candidate("candidate-hard-negative", "s1", "s2", 10_000, true);
    let decision = evaluate_candidate_score(&blocked, ScoreThreshold::new(score(1)));
    assert!(!decision.accepted);
    assert_eq!(
        decision.reason,
        CandidateScoreDecisionReason::HardCannotLink
    );

    let serialized_score = serde_json::to_string(&score(8_182)).expect("score serializes");
    assert_eq!(serialized_score, "8182");
}

#[test]
fn deterministic_score_units() {
    let contributions = vec![
        contribution(
            ScoreLane::RelationHint,
            "relation:family",
            "context",
            10_000,
        ),
        contribution(ScoreLane::Support, "metric:jw", "string_similarity", 6_250),
        contribution(
            ScoreLane::AntiMerge,
            "distinct:operator",
            "hard_negative",
            9_500,
        ),
        contribution(ScoreLane::Support, "metric:tfidf", "sparse_cosine", 4_000),
    ];
    let first = accumulate_score_units(contributions.clone());
    let mut reversed = contributions;
    reversed.reverse();
    let second = accumulate_score_units(reversed);

    assert_eq!(first, second);
    assert_eq!(first.version, CANON_ENTITY_SCORE_VERSION);
    assert_eq!(first.raw_support_score_units, 10_250);
    assert_eq!(first.total_score_units, score(10_000));
    assert_eq!(
        first
            .contributions
            .iter()
            .map(|contribution| (
                contribution.lane,
                contribution.source_id.as_str(),
                contribution.score_units.as_u32()
            ))
            .collect::<Vec<_>>(),
        [
            (ScoreLane::Support, "metric:jw", 6_250),
            (ScoreLane::Support, "metric:tfidf", 4_000),
            (ScoreLane::AntiMerge, "distinct:operator", 9_500),
            (ScoreLane::RelationHint, "relation:family", 10_000),
        ]
    );

    let candidates = vec![
        candidate("cand-c", "s3", "s4", 9_000, false),
        candidate("cand-hard-negative", "s5", "s6", 10_000, true),
        candidate("cand-a", "s1", "s2", 9_500, false),
        candidate("cand-b", "s1", "s3", 8_999, false),
        candidate("cand-a", "s7", "s8", 9_500, false),
    ];
    let threshold = ScoreThreshold::new(score(9_000));
    let full = accepted_candidate_ids(candidates.clone(), threshold);
    let low_hint = accepted_candidate_ids_with_hints(
        candidates.clone(),
        threshold,
        ScoreOptimizationHints::new(Some(score(1)), Some(score(8_000))),
    );
    let high_hint = accepted_candidate_ids_with_hints(
        candidates.clone(),
        threshold,
        ScoreOptimizationHints::new(Some(score(9_999)), Some(score(10_000))),
    );

    assert_eq!(full, ["cand-a", "cand-c"]);
    assert_eq!(full, low_hint);
    assert_eq!(full, high_hint);

    let mut sorted = candidates;
    sort_scored_candidates(&mut sorted);
    assert_eq!(
        sorted
            .iter()
            .map(|candidate| (
                candidate.candidate_id.as_str(),
                candidate.score_units.as_u32(),
                candidate.left_surface_id.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            ("cand-hard-negative", 10_000, "s5"),
            ("cand-a", 9_500, "s1"),
            ("cand-a", 9_500, "s7"),
            ("cand-c", 9_000, "s3"),
            ("cand-b", 8_999, "s1"),
        ]
    );

    let serialized = serde_json::to_string(&first).expect("score breakdown serializes");
    assert!(serialized.contains("\"total_score_units\":10000"));
    assert!(!serialized.contains("6250.0"));
    assert!(!serialized.contains("0.625"));
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is within entity score scale")
}

fn contribution(
    lane: ScoreLane,
    source_id: &str,
    reason_code: &str,
    score_units: u32,
) -> ScoreContribution {
    ScoreContribution::new(lane, source_id, reason_code, score(score_units))
}

fn candidate(
    candidate_id: &str,
    left_surface_id: &str,
    right_surface_id: &str,
    score_units: u32,
    hard_cannot_link: bool,
) -> ScoredCandidate {
    ScoredCandidate::new(
        candidate_id,
        left_surface_id,
        right_surface_id,
        score(score_units),
        hard_cannot_link,
    )
}
