#![forbid(unsafe_code)]

use canon::entity::{
    anti_merge::{
        ProtectedTokenConflictRequest, RelatedDistinctPhraseRequest, protected_token_conflict_hit,
        related_distinct_phrase_hit,
    },
    edge::build_edge_evidence_record,
    evidence::{ExactViewSupportRequest, exact_view_support_hit},
    score::{
        CandidateScoreDecisionReason, ScoreLane, ScoreThreshold, ScoreUnits, ScoredCandidate,
        evaluate_candidate_score,
    },
};

#[test]
fn edge_cannot_link_protected_conflict_is_hard_anti_merge_evidence() {
    let support = exact_view_support_hit(ExactViewSupportRequest {
        namespace: "name",
        operator_id: "exact_view:tenant_core",
        reason_code: "exact_tenant_core",
        view_name: "tenant_core",
        left_value: "sears",
        right_value: "sears",
        score_units: score(10_000),
    })
    .expect("exact view supplies strong support");
    let cannot_link = protected_token_conflict_hit(ProtectedTokenConflictRequest {
        namespace: "tenant_role",
        operator_id: "protected_token_conflict:tenant_brand",
        reason_code: "protected_token_conflict",
        left_tokens: &["sears"],
        right_tokens: &["sears", "auto", "center"],
        score_units: score(10_000),
    })
    .expect("protected token conflict emits hard anti-merge evidence");

    let record = build_edge_evidence_record(
        "surf:sears",
        "surf:sears_auto_center",
        vec![support, cannot_link],
    )
    .expect("edge record builds with support and hard cannot-link");

    assert_eq!(record.pair_score_total, score(10_000));
    assert!(record.has_hard_cannot_link);
    assert!(record.hits.iter().any(|hit| {
        hit.lane == ScoreLane::AntiMerge
            && hit.hard_cannot_link
            && hit.reason_code == "protected_token_conflict"
            && hit.explanation.contains("right_only=auto|center")
    }));

    let decision = evaluate_candidate_score(
        &ScoredCandidate::new(
            "candidate:sears-auto",
            "surf:sears",
            "surf:sears_auto_center",
            record.pair_score_total,
            record.has_hard_cannot_link,
        ),
        ScoreThreshold::new(score(1)),
    );

    assert!(!decision.accepted);
    assert_eq!(
        decision.reason,
        CandidateScoreDecisionReason::HardCannotLink
    );
}

#[test]
fn cannot_link_veto_handles_related_distinct_phrases_without_support_lane_leakage() {
    let hit = related_distinct_phrase_hit(RelatedDistinctPhraseRequest {
        namespace: "tenant_role",
        operator_id: "related_distinct_phrase:tenant_label",
        reason_code: "related_distinct_phrase",
        left_value: "Sears",
        right_value: "Sears Auto Center",
        phrases: &["auto center", "holdings", "management"],
        score_units: score(9_500),
    })
    .expect("related-but-distinct phrase emits cannot-link evidence");

    let record = build_edge_evidence_record("surf:sears", "surf:sears_auto_center", vec![hit])
        .expect("anti-merge-only edge record builds");

    assert_eq!(record.pair_score_total, ScoreUnits::ZERO);
    assert!(record.has_hard_cannot_link);
    assert_eq!(record.hits[0].lane, ScoreLane::AntiMerge);
    assert!(record.hits[0].explanation.contains("auto center"));

    let json = serde_json::to_string(&record).expect("record serializes");
    assert!(json.contains("\"anti_merge\""));
    assert!(!json.contains("\"support\""));
}

#[test]
fn protected_token_conflict_ignores_matching_or_empty_protected_sets() {
    assert!(
        protected_token_conflict_hit(ProtectedTokenConflictRequest {
            namespace: "tenant_role",
            operator_id: "protected_token_conflict:tenant_brand",
            reason_code: "protected_token_conflict",
            left_tokens: &["sears"],
            right_tokens: &["sears"],
            score_units: score(10_000),
        })
        .is_none()
    );
    assert!(
        protected_token_conflict_hit(ProtectedTokenConflictRequest {
            namespace: "tenant_role",
            operator_id: "protected_token_conflict:tenant_brand",
            reason_code: "protected_token_conflict",
            left_tokens: &[],
            right_tokens: &[],
            score_units: score(10_000),
        })
        .is_none()
    );
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
