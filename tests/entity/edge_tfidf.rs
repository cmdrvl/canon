#![forbid(unsafe_code)]

use canon::entity::{
    edge::build_edge_evidence_record,
    score::{ScoreLane, ScoreUnits},
    tfidf_evidence::{
        TfidfCosineSupportRequest, sparse_architecture_proof, tfidf_cosine_support_evidence,
        tfidf_cosine_support_hit,
    },
};
use canon::namekit::tfidf::{
    SparseTfidfModel, TfidfEvidenceClass, TfidfInputSurface, TfidfTermKey,
};
use serde_json::json;

#[test]
fn edge_tfidf_evidence() {
    let model = sears_model();
    let evidence = tfidf_cosine_support_evidence(TfidfCosineSupportRequest {
        namespace: "token",
        operator_id: "tfidf_cosine:tenant_tokens",
        model: &model,
        left_surface_id: "surf:roebuck_holdings",
        right_surface_id: "surf:sears_roebuck",
        min_score_units: score(4_000),
        top_k: 4,
        candidate_cap: Some(4),
    })
    .expect("rare shared token emits support evidence");

    assert_eq!(
        evidence.evidence_class,
        TfidfEvidenceClass::RareTokenSupport
    );
    assert_eq!(evidence.hit.lane, ScoreLane::Support);
    assert_eq!(evidence.hit.reason_code, "rare_token_support");
    assert_eq!(evidence.hit.score_units, score(4_999));
    assert_eq!(evidence.shared_term_count, 1);
    assert_eq!(evidence.max_shared_idf_units, 1_600);
    assert_eq!(
        evidence
            .top_contributors
            .iter()
            .map(|contributor| (
                contributor.term_id,
                contributor.term_key.as_str(),
                contributor.idf_units,
                contributor.contribution_units,
                contributor.reason_code.as_str()
            ))
            .collect::<Vec<_>>(),
        [(5, "roebuck", 1_600, 2_560_000_000_000, "rare_token_support")]
    );

    let snapshot = serde_json::to_value(&evidence).expect("evidence serializes");
    assert_eq!(
        snapshot,
        json!({
            "hit": {
                "lane": "support",
                "namespace": "token",
                "operator_id": "tfidf_cosine:tenant_tokens",
                "reason_code": "rare_token_support",
                "score_units": 4999,
                "hard_cannot_link": false,
                "explanation": "tfidf cosine score_units=4999 evidence_class=rare_token_support shared_terms=1 max_shared_idf_units=1600 top_contributors=roebuck"
            },
            "evidence_class": "rare_token_support",
            "shared_term_count": 1,
            "max_shared_idf_units": 1600,
            "top_contributors": [{
                "term_id": 5,
                "term_key": "roebuck",
                "idf_units": 1600,
                "left_weight_units": 1600000,
                "right_weight_units": 1600000,
                "contribution_units": 2560000000000_u64,
                "reason_code": "rare_token_support"
            }],
            "top_k_diagnostics": {
                "k": 4,
                "candidate_cap": 4,
                "uncapped_candidate_count": 1,
                "capped_candidate_count": 0,
                "cap_exceeded": false
            }
        })
    );

    let record = build_edge_evidence_record(
        "surf:roebuck_holdings",
        "surf:sears_roebuck",
        vec![evidence.hit],
    )
    .expect("tf-idf support edge record builds");
    assert_eq!(record.pair_score_total, score(4_999));
    assert_eq!(record.score_breakdown.raw_support_score_units, 4_999);
    assert!(!record.has_hard_cannot_link);

    let json = serde_json::to_string(&record).expect("record serializes");
    assert!(json.contains("\"tfidf_cosine:tenant_tokens\""));
    assert!(!json.contains("4999.0"));
    assert!(!json.contains("0.4999"));
}

#[test]
fn tfidf_rare_token_evidence() {
    let model = sears_model();
    let rare = tfidf_cosine_support_evidence(TfidfCosineSupportRequest {
        namespace: "token",
        operator_id: "tfidf_cosine:tenant_tokens",
        model: &model,
        left_surface_id: "surf:roebuck_holdings",
        right_surface_id: "surf:sears_roebuck",
        min_score_units: score(1),
        top_k: 4,
        candidate_cap: Some(4),
    })
    .expect("rare token support emits");
    let common = tfidf_cosine_support_evidence(TfidfCosineSupportRequest {
        namespace: "token",
        operator_id: "tfidf_cosine:tenant_tokens",
        model: &model,
        left_surface_id: "surf:sears_llc",
        right_surface_id: "surf:sears_roebuck",
        min_score_units: score(1),
        top_k: 4,
        candidate_cap: Some(4),
    })
    .expect("common token support emits with downweighted reason");

    assert_eq!(rare.hit.reason_code, "rare_token_support");
    assert_eq!(common.hit.reason_code, "common_token_downweighted");
    assert_eq!(rare.hit.score_units, score(4_999));
    assert_eq!(common.hit.score_units, score(4_042));
    assert!(rare.hit.score_units > common.hit.score_units);
    assert_eq!(common.top_contributors[0].term_key, "sears");
    assert_eq!(
        common.top_contributors[0].reason_code,
        "common_token_downweighted"
    );

    assert!(
        tfidf_cosine_support_hit(TfidfCosineSupportRequest {
            namespace: "token",
            operator_id: "tfidf_cosine:tenant_tokens",
            model: &model,
            left_surface_id: "surf:roebuck_holdings",
            right_surface_id: "surf:sears_roebuck",
            min_score_units: score(5_000),
            top_k: 4,
            candidate_cap: Some(4),
        })
        .is_none(),
        "integer cutoff above score must suppress the support hit"
    );
    assert!(
        tfidf_cosine_support_hit(TfidfCosineSupportRequest {
            namespace: "token",
            operator_id: "tfidf_cosine:tenant_tokens",
            model: &model,
            left_surface_id: "surf:sears_roebuck",
            right_surface_id: "surf:sears_llc",
            min_score_units: score(1),
            top_k: 1,
            candidate_cap: Some(1),
        })
        .is_none(),
        "top-k/candidate caps bound evidence to emitted candidate artifacts"
    );

    let proof = sparse_architecture_proof(&wide_sparse_model());
    assert_eq!(proof.document_count, 64);
    assert_eq!(proof.dictionary_term_count, 65);
    assert_eq!(proof.sparse_row_term_count, 128);
    assert_eq!(proof.posting_count, 128);
    assert_eq!(proof.dense_cell_count_if_materialized, 4_160);
    assert!(!proof.uses_dense_vectors);
}

fn sears_model() -> SparseTfidfModel {
    SparseTfidfModel::build(&[
        TfidfInputSurface::tokenized("surf:sears_roebuck", "sears roebuck", ["sears", "roebuck"]),
        TfidfInputSurface::tokenized("surf:sears_llc", "sears llc", ["sears", "llc"]),
        TfidfInputSurface::tokenized("surf:sears_auto", "sears auto", ["sears", "auto"]),
        TfidfInputSurface::tokenized(
            "surf:roebuck_holdings",
            "roebuck holdings",
            ["roebuck", "holdings"],
        ),
        TfidfInputSurface::tokenized("surf:pnc_bank", "pnc bank", ["pnc", "bank"]),
    ])
}

fn wide_sparse_model() -> SparseTfidfModel {
    let surfaces = (0..64)
        .map(|index| {
            let surface_id = format!("surf:{index:03}");
            let unique = format!("unique_{index:03}");
            TfidfInputSurface::new(
                surface_id,
                format!("common {unique}"),
                vec![TfidfTermKey::token("common"), TfidfTermKey::token(unique)],
            )
        })
        .collect::<Vec<_>>();
    SparseTfidfModel::build(&surfaces)
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
