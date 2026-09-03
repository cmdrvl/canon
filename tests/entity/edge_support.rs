#![forbid(unsafe_code)]

use canon::entity::{
    edge::build_edge_evidence_record,
    evidence::{
        DateNearSupportRequest, DateTransposedDigitSupportRequest, ExactViewSupportRequest,
        StringSimilaritySupportRequest, TokenOverlapSupportRequest, TwoTokenReversalSupportRequest,
        date_near_support_hit, date_transposed_digit_support_hit, exact_view_support_hit,
        string_similarity_support_hit, token_overlap_support_hit, two_token_reversal_support_hit,
    },
    score::{ScoreLane, ScoreUnits},
};
use canon::namekit::similarity::SimilarityMetric;

#[test]
fn entity_edge_support_evidence_records_exact_token_and_string_metric_hits() {
    let hits = vec![
        exact_view_support_hit(ExactViewSupportRequest {
            namespace: "name",
            operator_id: "exact_view:tenant_core",
            reason_code: "exact_tenant_core",
            view_name: "tenant_core",
            left_value: "sears",
            right_value: "sears",
            score_units: score(10_000),
        })
        .expect("exact normalized view is support"),
        token_overlap_support_hit(TokenOverlapSupportRequest {
            namespace: "token",
            operator_id: "rare_token_overlap:tenant_tokens",
            reason_code: "rare_token_overlap",
            left_tokens: &["sears", "roebuck"],
            right_tokens: &["sears", "llc"],
            min_shared_tokens: 1,
        })
        .expect("shared rare token is support"),
        string_similarity_support_hit(StringSimilaritySupportRequest {
            namespace: "name",
            operator_id: "string_similarity:jaro_winkler",
            reason_code: "tenant_core_similarity",
            metric: SimilarityMetric::JaroWinkler,
            left_value: "sears",
            right_value: "sears",
            score_cutoff: Some(score(9_000)),
            score_hint: Some(score(8_000)),
        })
        .expect("matching tenant core passes string metric cutoff"),
    ];

    let record = build_edge_evidence_record("surf:sears", "surf:sears_llc", hits)
        .expect("support edge record builds");

    assert_eq!(record.pair_score_total, score(10_000));
    assert_eq!(record.score_breakdown.raw_support_score_units, 25_000);
    assert!(!record.has_hard_cannot_link);
    assert!(record.hits.iter().all(|hit| hit.lane == ScoreLane::Support));
    assert_eq!(
        record
            .hits
            .iter()
            .map(|hit| (
                hit.namespace.as_str(),
                hit.operator_id.as_str(),
                hit.reason_code.as_str(),
                hit.score_units.as_u32()
            ))
            .collect::<Vec<_>>(),
        [
            (
                "name",
                "exact_view:tenant_core",
                "exact_tenant_core",
                10_000,
            ),
            (
                "name",
                "string_similarity:jaro_winkler",
                "tenant_core_similarity",
                10_000,
            ),
            (
                "token",
                "rare_token_overlap:tenant_tokens",
                "rare_token_overlap",
                5_000,
            ),
        ]
    );

    let json = serde_json::to_string(&record).expect("record serializes");
    assert!(json.contains("\"score_units\":10000"));
    assert!(!json.contains("relation_hint"));
    assert!(!json.contains("anti_merge"));
    assert!(!json.contains("0.5"));
}

#[test]
fn support_evidence_explainable_and_cutoff_safe() {
    assert!(
        exact_view_support_hit(ExactViewSupportRequest {
            namespace: "name",
            operator_id: "exact_view:tenant_core",
            reason_code: "exact_tenant_core",
            view_name: "tenant_core",
            left_value: "sears",
            right_value: "kmart",
            score_units: score(10_000),
        })
        .is_none()
    );

    assert!(
        string_similarity_support_hit(StringSimilaritySupportRequest {
            namespace: "name",
            operator_id: "string_similarity:levenshtein",
            reason_code: "tenant_core_similarity",
            metric: SimilarityMetric::LevenshteinNormalized,
            left_value: "south korea",
            right_value: "north korea",
            score_cutoff: Some(score(9_000)),
            score_hint: Some(score(8_000)),
        })
        .is_none()
    );

    let token_hit = token_overlap_support_hit(TokenOverlapSupportRequest {
        namespace: "token",
        operator_id: "rare_token_overlap:tenant_tokens",
        reason_code: "rare_token_overlap",
        left_tokens: &["sears", "auto"],
        right_tokens: &["sears", "center"],
        min_shared_tokens: 2,
    });
    assert!(token_hit.is_none());
}

#[test]
fn transposed_digit_date_support_is_distinct_from_date_range() {
    let near = date_near_support_hit(DateNearSupportRequest {
        namespace: "date",
        operator_id: "date_near:maturity_date",
        reason_code: "date_range_support",
        view_name: "maturity_date",
        left_value: "2031-01-12",
        right_value: "2031-01-21",
        max_days: 1,
        score_units: score(7_000),
    })
    .expect("valid ISO dates");
    assert!(
        near.is_none(),
        "calendar-distance support must miss before the transposed-digit band is used"
    );

    let transposed = date_transposed_digit_support_hit(DateTransposedDigitSupportRequest {
        namespace: "date",
        operator_id: "date_transposed_digits:maturity_date",
        reason_code: "transposed_digit_date_support",
        view_name: "maturity_date",
        left_value: "2031-01-12",
        right_value: "2031-01-21",
        score_units: score(8_250),
    })
    .expect("valid ISO dates")
    .expect("adjacent digit swap emits support");

    assert_eq!(transposed.lane, ScoreLane::Support);
    assert_eq!(transposed.namespace, "date");
    assert_eq!(
        transposed.operator_id,
        "date_transposed_digits:maturity_date"
    );
    assert_eq!(transposed.reason_code, "transposed_digit_date_support");
    assert_eq!(transposed.score_units, score(8_250));
    assert!(
        transposed
            .explanation
            .contains("matched adjacent digit transposition")
    );
    assert!(transposed.explanation.contains("damerau_score_units=9000"));

    let month_change = date_transposed_digit_support_hit(DateTransposedDigitSupportRequest {
        namespace: "date",
        operator_id: "date_transposed_digits:maturity_date",
        reason_code: "transposed_digit_date_support",
        view_name: "maturity_date",
        left_value: "2031-01-12",
        right_value: "2031-02-12",
        score_units: score(8_250),
    })
    .expect("valid ISO dates");
    assert!(
        month_change.is_none(),
        "a one-character real month change is not an adjacent transposition"
    );
}

#[test]
fn two_token_reversal_support_rejects_partial_three_token_reversal() {
    let reversal = two_token_reversal_support_hit(TwoTokenReversalSupportRequest {
        namespace: "name",
        operator_id: "two_token_reversal:party_name",
        reason_code: "two_token_reversal_support",
        view_name: "party_name",
        left_value: "SMITH JOHN",
        right_value: "JOHN SMITH",
        score_units: score(7_500),
    })
    .expect("two-token reversal emits support");

    assert_eq!(reversal.lane, ScoreLane::Support);
    assert_eq!(reversal.operator_id, "two_token_reversal:party_name");
    assert_eq!(reversal.reason_code, "two_token_reversal_support");
    assert_eq!(reversal.score_units, score(7_500));

    let partial_three_token = two_token_reversal_support_hit(TwoTokenReversalSupportRequest {
        namespace: "name",
        operator_id: "two_token_reversal:party_name",
        reason_code: "two_token_reversal_support",
        view_name: "party_name",
        left_value: "SMITH JOHN ROBERT",
        right_value: "JOHN SMITH ROBERT",
        score_units: score(7_500),
    });
    assert!(
        partial_three_token.is_none(),
        "three-token partial reversals must not borrow the two-token reversal band"
    );
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
