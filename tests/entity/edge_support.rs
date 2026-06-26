#![forbid(unsafe_code)]

use canon::entity::{
    edge::build_edge_evidence_record,
    evidence::{
        ExactViewSupportRequest, StringSimilaritySupportRequest, TokenOverlapSupportRequest,
        exact_view_support_hit, string_similarity_support_hit, token_overlap_support_hit,
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

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
