#![forbid(unsafe_code)]

use canon::entity::{
    edge::{EdgeEvidenceHit, build_edge_evidence_record},
    evidence::{StringSimilaritySupportRequest, string_similarity_support_hit},
    score::{
        CandidateScoreDecisionReason, ScoreOptimizationHints, ScoreThreshold, ScoreUnits,
        ScoredCandidate, accepted_candidate_ids_with_hints, evaluate_candidate_score,
        sort_scored_candidates,
    },
    tfidf_evidence::{TfidfCosineSupportRequest, tfidf_cosine_support_evidence},
};
use canon::namekit::{
    similarity::SimilarityMetric,
    tfidf::{SparseTfidfModel, TfidfInputSurface},
};
use serde::Deserialize;
use std::fs;

const FIXTURE: &str = "tests/fixtures/entity/edge/score_determinism/cases.json";

#[derive(Debug, Deserialize)]
struct Fixture {
    version: String,
    edge_case: EdgeCase,
    threshold_cases: Vec<ThresholdCase>,
    candidate_order: Vec<CandidateCase>,
    expected_candidate_order: Vec<(String, String, String, u32)>,
    tfidf_cases: Vec<TfidfCase>,
}

#[derive(Debug, Deserialize)]
struct EdgeCase {
    left_surface_id: String,
    right_surface_id: String,
    hits: Vec<HitCase>,
    expected_pair_score_total: u32,
    expected_raw_support_score_units: u64,
    expected_hit_order: Vec<(String, String, String, String, u32)>,
}

#[derive(Debug, Deserialize)]
struct HitCase {
    lane: String,
    namespace: String,
    operator_id: String,
    reason_code: String,
    score_units: u32,
    hard_cannot_link: bool,
    explanation: String,
}

#[derive(Debug, Deserialize, Clone)]
struct ThresholdCase {
    candidate_id: String,
    left_surface_id: String,
    right_surface_id: String,
    score_units: u32,
    threshold_units: u32,
    hard_cannot_link: bool,
    expected_reason: String,
    expected_accepted: bool,
}

#[derive(Debug, Deserialize, Clone)]
struct CandidateCase {
    candidate_id: String,
    left_surface_id: String,
    right_surface_id: String,
    score_units: u32,
    hard_cannot_link: bool,
}

#[derive(Debug, Deserialize)]
struct TfidfCase {
    case_id: String,
    left_surface_id: String,
    right_surface_id: String,
    expected_score_units: u32,
    expected_reason_code: String,
    expected_top_contributor: String,
    expected_max_shared_idf_units: u32,
}

#[test]
fn edge_score_determinism() {
    let fixture = fixture();
    assert_eq!(fixture.version, "canon_entity_edge_score_determinism.v0");

    let case = &fixture.edge_case;
    let hits = case.hits.iter().map(HitCase::edge_hit).collect::<Vec<_>>();
    let first = build_edge_evidence_record(&case.left_surface_id, &case.right_surface_id, hits)
        .expect("edge record builds");
    let mut reversed = case
        .hits
        .iter()
        .rev()
        .map(HitCase::edge_hit)
        .collect::<Vec<_>>();
    let second = build_edge_evidence_record(
        &case.left_surface_id,
        &case.right_surface_id,
        reversed.clone(),
    )
    .expect("reversed edge record builds");
    reversed.rotate_left(1);
    let third = build_edge_evidence_record(&case.left_surface_id, &case.right_surface_id, reversed)
        .expect("rotated edge record builds");

    assert_eq!(first, second);
    assert_eq!(first, third);
    assert_eq!(
        first.pair_score_total,
        score(case.expected_pair_score_total)
    );
    assert_eq!(
        first.score_breakdown.raw_support_score_units,
        case.expected_raw_support_score_units
    );
    assert_eq!(
        first
            .hits
            .iter()
            .map(|hit| {
                (
                    lane_id(hit.lane).to_string(),
                    hit.namespace.clone(),
                    hit.operator_id.clone(),
                    hit.reason_code.clone(),
                    hit.score_units.as_u32(),
                )
            })
            .collect::<Vec<_>>(),
        case.expected_hit_order
    );
    let first_bytes = serde_json::to_vec(&first).expect("first record serializes");
    assert_eq!(
        first_bytes,
        serde_json::to_vec(&second).expect("second record serializes")
    );
    assert_eq!(
        first_bytes,
        serde_json::to_vec(&third).expect("third record serializes")
    );
    let serialized = String::from_utf8(first_bytes).expect("record json is utf-8");
    assert!(serialized.contains("\"pair_score_total\":10000"));
    assert!(!serialized.contains(".0"));
    assert!(!serialized.contains("0.5"));

    assert_threshold_cases_are_stable(&fixture.threshold_cases);
    assert_candidate_order_is_stable(&fixture);
    assert_string_metric_cutoff_is_stable();
}

#[test]
fn tfidf_rare_token_evidence() {
    let fixture = fixture();
    let first_model = sears_model();
    let reloaded_model = sears_model_reordered();

    for case in &fixture.tfidf_cases {
        let first = tfidf_case_evidence(&first_model, case);
        let second = tfidf_case_evidence(&first_model, case);
        let reloaded = tfidf_case_evidence(&reloaded_model, case);

        assert_eq!(first, second, "{} changed across runs", case.case_id);
        assert_eq!(
            serde_json::to_vec(&first).expect("first evidence serializes"),
            serde_json::to_vec(&second).expect("second evidence serializes"),
            "{} bytes changed across runs",
            case.case_id
        );
        assert_eq!(
            serde_json::to_vec(&first).expect("first evidence serializes"),
            serde_json::to_vec(&reloaded).expect("reloaded evidence serializes"),
            "{} changed after row reorder/cache reload",
            case.case_id
        );
        assert_eq!(first.hit.score_units, score(case.expected_score_units));
        assert_eq!(first.hit.reason_code, case.expected_reason_code);
        assert_eq!(
            first.top_contributors[0].term_key,
            case.expected_top_contributor
        );
        assert_eq!(
            first.max_shared_idf_units,
            case.expected_max_shared_idf_units
        );

        let record = build_edge_evidence_record(
            &case.left_surface_id,
            &case.right_surface_id,
            vec![first.hit],
        )
        .expect("tf-idf edge record builds");
        assert_eq!(record.pair_score_total, score(case.expected_score_units));
        assert_eq!(
            record.score_breakdown.raw_support_score_units,
            u64::from(case.expected_score_units)
        );
    }
}

impl HitCase {
    fn edge_hit(&self) -> EdgeEvidenceHit {
        EdgeEvidenceHit::new(
            parse_lane(&self.lane),
            &self.namespace,
            &self.operator_id,
            &self.reason_code,
            score(self.score_units),
            self.hard_cannot_link,
            &self.explanation,
        )
    }
}

fn assert_threshold_cases_are_stable(cases: &[ThresholdCase]) {
    let candidates = cases.iter().map(threshold_candidate).collect::<Vec<_>>();
    for case in cases {
        for _ in 0..3 {
            let decision = evaluate_candidate_score(
                &threshold_candidate(case),
                ScoreThreshold::new(score(case.threshold_units)),
            );
            assert_eq!(
                decision.accepted, case.expected_accepted,
                "{} acceptance changed",
                case.candidate_id
            );
            assert_eq!(
                decision.reason,
                parse_decision_reason(&case.expected_reason),
                "{} decision reason changed",
                case.candidate_id
            );
        }
    }

    let threshold = ScoreThreshold::new(score(9_000));
    let no_hints = accepted_candidate_ids_with_hints(
        candidates.clone(),
        threshold,
        ScoreOptimizationHints::default(),
    );
    let low_hints = accepted_candidate_ids_with_hints(
        candidates.clone(),
        threshold,
        ScoreOptimizationHints::new(Some(score(1)), Some(score(8_000))),
    );
    let high_hints = accepted_candidate_ids_with_hints(
        candidates,
        threshold,
        ScoreOptimizationHints::new(Some(score(9_999)), Some(score(10_000))),
    );
    assert_eq!(no_hints, ["cand:accepted_exact_threshold"]);
    assert_eq!(no_hints, low_hints);
    assert_eq!(no_hints, high_hints);
}

fn assert_candidate_order_is_stable(fixture: &Fixture) {
    let mut candidates = fixture
        .candidate_order
        .iter()
        .rev()
        .map(candidate_case)
        .collect::<Vec<_>>();
    sort_scored_candidates(&mut candidates);
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| {
                (
                    candidate.candidate_id.clone(),
                    candidate.left_surface_id.clone(),
                    candidate.right_surface_id.clone(),
                    candidate.score_units.as_u32(),
                )
            })
            .collect::<Vec<_>>(),
        fixture.expected_candidate_order
    );
}

fn assert_string_metric_cutoff_is_stable() {
    for _ in 0..3 {
        assert!(
            string_similarity_support_hit(StringSimilaritySupportRequest {
                namespace: "name",
                operator_id: "string_similarity:levenshtein",
                reason_code: "tenant_core_similarity",
                metric: SimilarityMetric::LevenshteinNormalized,
                left_value: "South Korea",
                right_value: "North Korea",
                score_cutoff: Some(score(9_000)),
                score_hint: Some(score(8_000)),
            })
            .is_none()
        );
        let pass = string_similarity_support_hit(StringSimilaritySupportRequest {
            namespace: "name",
            operator_id: "string_similarity:jaro_winkler",
            reason_code: "tenant_core_similarity",
            metric: SimilarityMetric::JaroWinkler,
            left_value: "martha",
            right_value: "marhta",
            score_cutoff: Some(score(9_500)),
            score_hint: Some(score(9_600)),
        })
        .expect("Jaro-Winkler support passes cutoff");
        assert_eq!(pass.score_units, score(9_611));
    }
}

fn tfidf_case_evidence(
    model: &SparseTfidfModel,
    case: &TfidfCase,
) -> canon::entity::tfidf_evidence::TfidfCosineSupportEvidence {
    tfidf_cosine_support_evidence(TfidfCosineSupportRequest {
        namespace: "token",
        operator_id: "tfidf_cosine:tenant_tokens",
        model,
        left_surface_id: &case.left_surface_id,
        right_surface_id: &case.right_surface_id,
        min_score_units: score(1),
        top_k: 4,
        candidate_cap: Some(4),
    })
    .unwrap_or_else(|| panic!("{} emits tf-idf support", case.case_id))
}

fn threshold_candidate(case: &ThresholdCase) -> ScoredCandidate {
    ScoredCandidate::new(
        &case.candidate_id,
        &case.left_surface_id,
        &case.right_surface_id,
        score(case.score_units),
        case.hard_cannot_link,
    )
}

fn candidate_case(case: &CandidateCase) -> ScoredCandidate {
    ScoredCandidate::new(
        &case.candidate_id,
        &case.left_surface_id,
        &case.right_surface_id,
        score(case.score_units),
        case.hard_cannot_link,
    )
}

fn sears_model() -> SparseTfidfModel {
    SparseTfidfModel::build(&sears_surfaces())
}

fn sears_model_reordered() -> SparseTfidfModel {
    let mut surfaces = sears_surfaces();
    surfaces.reverse();
    SparseTfidfModel::build(&surfaces)
}

fn sears_surfaces() -> Vec<TfidfInputSurface> {
    vec![
        TfidfInputSurface::tokenized("surf:sears_roebuck", "sears roebuck", ["sears", "roebuck"]),
        TfidfInputSurface::tokenized("surf:sears_llc", "sears llc", ["sears", "llc"]),
        TfidfInputSurface::tokenized("surf:sears_auto", "sears auto", ["sears", "auto"]),
        TfidfInputSurface::tokenized(
            "surf:roebuck_holdings",
            "roebuck holdings",
            ["roebuck", "holdings"],
        ),
        TfidfInputSurface::tokenized("surf:pnc_bank", "pnc bank", ["pnc", "bank"]),
    ]
}

fn fixture() -> Fixture {
    serde_json::from_str(
        &fs::read_to_string(FIXTURE).expect("edge score determinism fixture is readable"),
    )
    .expect("edge score determinism fixture parses")
}

fn parse_lane(lane: &str) -> canon::entity::score::ScoreLane {
    match lane {
        "support" => canon::entity::score::ScoreLane::Support,
        "anti_merge" => canon::entity::score::ScoreLane::AntiMerge,
        "relation_hint" => canon::entity::score::ScoreLane::RelationHint,
        other => panic!("unexpected lane {other}"),
    }
}

fn lane_id(lane: canon::entity::score::ScoreLane) -> &'static str {
    match lane {
        canon::entity::score::ScoreLane::Support => "support",
        canon::entity::score::ScoreLane::AntiMerge => "anti_merge",
        canon::entity::score::ScoreLane::RelationHint => "relation_hint",
    }
}

fn parse_decision_reason(reason: &str) -> CandidateScoreDecisionReason {
    match reason {
        "accepted" => CandidateScoreDecisionReason::Accepted,
        "below_threshold" => CandidateScoreDecisionReason::BelowThreshold,
        "hard_cannot_link" => CandidateScoreDecisionReason::HardCannotLink,
        other => panic!("unexpected decision reason {other}"),
    }
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("fixture score is inside score scale")
}
