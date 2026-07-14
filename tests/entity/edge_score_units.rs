#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_EDGE_VERSION,
        edge::{EdgeEvidenceHit, build_edge_evidence_record},
        score::{CANON_ENTITY_SCORE_VERSION, ScoreLane, ScoreUnits},
    },
};

#[test]
fn edge_score_units_are_stored_as_integers_without_float_debug_output() {
    let record = build_edge_evidence_record("surf:001", "surf:002", mixed_hits())
        .expect("edge evidence record builds");

    assert_eq!(record.version, CANON_ENTITY_EDGE_VERSION);
    assert_eq!(record.pair_score_total, score(10_000));
    assert_eq!(record.score_breakdown.version, CANON_ENTITY_SCORE_VERSION);
    assert_eq!(record.score_breakdown.raw_support_score_units, 10_250);
    assert!(record.has_hard_cannot_link);

    let json = serde_json::to_string(&record).expect("edge record serializes");
    assert!(json.contains("\"pair_score_total\":10000"));
    assert!(json.contains("\"score_units\":6250"));
    assert!(!json.contains("6250.0"));
    assert!(!json.contains("0.625"));
}

#[test]
fn edge_score_determinism_survives_input_hit_order_changes() {
    let first = build_edge_evidence_record("surf:001", "surf:002", mixed_hits())
        .expect("first record builds");
    let mut reversed = mixed_hits();
    reversed.reverse();
    let second =
        build_edge_evidence_record("surf:001", "surf:002", reversed).expect("second record builds");

    assert_eq!(first, second);
    assert_eq!(
        serde_json::to_vec(&first).expect("first record serializes"),
        serde_json::to_vec(&second).expect("second record serializes")
    );
    assert_eq!(
        first
            .hits
            .iter()
            .map(|hit| (
                hit.lane,
                hit.namespace.as_str(),
                hit.operator_id.as_str(),
                hit.score_units.as_u32()
            ))
            .collect::<Vec<_>>(),
        [
            (ScoreLane::Support, "name", "jaro_winkler", 6_250),
            (ScoreLane::Support, "token", "tfidf_cosine", 4_000),
            (
                ScoreLane::AntiMerge,
                "tenant_role",
                "protected_token",
                9_500
            ),
            (
                ScoreLane::RelationHint,
                "ontology",
                "brand_family_context",
                10_000
            ),
        ]
    );
}

#[test]
fn relation_hints_do_not_add_positive_merge_score() {
    let record = build_edge_evidence_record(
        "surf:001",
        "surf:002",
        vec![EdgeEvidenceHit::new(
            ScoreLane::RelationHint,
            "ontology",
            "brand_family_context",
            "related_brand_family",
            score(10_000),
            false,
            "same brand family is review context only",
        )],
    )
    .expect("relation-only record builds");

    assert_eq!(record.pair_score_total, ScoreUnits::ZERO);
    assert_eq!(record.score_breakdown.raw_support_score_units, 0);
    assert!(!record.has_hard_cannot_link);
}

#[test]
fn edge_score_units_refuse_non_canonical_surface_pairs() {
    let refusal = build_edge_evidence_record("surf:002", "surf:001", mixed_hits())
        .expect_err("reversed pair refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "evidence");
    assert_eq!(refusal.detail["reason"], "invalid_surface_pair");
}

fn mixed_hits() -> Vec<EdgeEvidenceHit> {
    vec![
        EdgeEvidenceHit::new(
            ScoreLane::RelationHint,
            "ontology",
            "brand_family_context",
            "related_brand_family",
            score(10_000),
            false,
            "same brand family is review context only",
        ),
        EdgeEvidenceHit::new(
            ScoreLane::Support,
            "name",
            "jaro_winkler",
            "tenant_core_similarity",
            score(6_250),
            false,
            "tenant_core Jaro-Winkler score",
        ),
        EdgeEvidenceHit::new(
            ScoreLane::AntiMerge,
            "tenant_role",
            "protected_token",
            "protected_distinct_phrase",
            score(9_500),
            true,
            "auto center is protected distinct context",
        ),
        EdgeEvidenceHit::new(
            ScoreLane::Support,
            "token",
            "tfidf_cosine",
            "rare_token_overlap",
            score(4_000),
            false,
            "sparse token cosine score",
        ),
    ]
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
