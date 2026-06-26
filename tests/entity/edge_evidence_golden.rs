#![forbid(unsafe_code)]

use canon::entity::{
    CANON_ENTITY_EDGE_VERSION,
    anti_merge::{ProtectedTokenConflictRequest, protected_token_conflict_hit},
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
    evidence::{
        ExactViewSupportRequest, StringSimilaritySupportRequest, exact_view_support_hit,
        string_similarity_support_hit,
    },
    relation::{
        RelationHintRequest, RelationPatchHintRequest, relation_hint_hit, relation_patch_hint_hit,
    },
    score::{ScoreLane, ScoreUnits},
};
use canon::namekit::similarity::SimilarityMetric;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GoldenManifest {
    cases: Vec<GoldenCase>,
}

#[derive(Debug, Deserialize)]
struct GoldenCase {
    case_id: String,
    left_surface_id: String,
    right_surface_id: String,
    expected_lanes: Vec<String>,
    expected_reason_codes: Vec<String>,
    expected_pair_score_total: u32,
    expected_hard_cannot_link: bool,
}

#[test]
fn entity_edge_evidence_golden() {
    for case in golden_cases() {
        let record = golden_record(&case);

        assert_eq!(
            record.version, CANON_ENTITY_EDGE_VERSION,
            "{}",
            case.case_id
        );
        assert_eq!(
            record.left_surface_id, case.left_surface_id,
            "{}",
            case.case_id
        );
        assert_eq!(
            record.right_surface_id, case.right_surface_id,
            "{}",
            case.case_id
        );
        assert_eq!(
            record.pair_score_total.as_u32(),
            case.expected_pair_score_total,
            "{}",
            case.case_id
        );
        assert_eq!(
            record.has_hard_cannot_link, case.expected_hard_cannot_link,
            "{}",
            case.case_id
        );
        assert_eq!(lane_names(&record), case.expected_lanes, "{}", case.case_id);
        assert_eq!(
            reason_codes(&record),
            case.expected_reason_codes,
            "{}",
            case.case_id
        );

        for hit in &record.hits {
            assert!(!hit.reason_code.trim().is_empty(), "{}", case.case_id);
            assert!(!hit.explanation.trim().is_empty(), "{}", case.case_id);
            assert!(
                hit.operator_id.contains(':') || hit.operator_id.contains("patch"),
                "{}",
                case.case_id
            );
        }

        let json = serde_json::to_string(&record).expect("record serializes");
        assert!(json.contains("\"score_units\""), "{}", case.case_id);
        assert!(!json.contains(".0"), "{}", case.case_id);
    }
}

#[test]
fn relation_hint_non_merge_edge_golden() {
    let relation_only = golden_cases()
        .into_iter()
        .find(|case| case.case_id == "sears_kmart_relation_distinctness")
        .expect("relation-only case");
    let record = golden_record(&relation_only);

    assert_eq!(record.pair_score_total, ScoreUnits::ZERO);
    assert_eq!(record.score_breakdown.raw_support_score_units, 0);
    assert_eq!(record.hits[0].lane, ScoreLane::RelationHint);
    assert!(!record.has_hard_cannot_link);
}

#[test]
fn edge_evidence_golden_ordering_is_deterministic() {
    for case in golden_cases() {
        let first = golden_record(&case);
        let mut hits = case_hits(&case.case_id);
        hits.reverse();
        let second = build_edge_evidence_record(
            case.left_surface_id.clone(),
            case.right_surface_id.clone(),
            hits,
        )
        .expect("reversed hits still build");

        assert_eq!(first, second, "{}", case.case_id);
        assert_eq!(
            serde_json::to_vec(&first).expect("first serializes"),
            serde_json::to_vec(&second).expect("second serializes"),
            "{}",
            case.case_id
        );
    }
}

fn golden_cases() -> Vec<GoldenCase> {
    serde_json::from_str::<GoldenManifest>(include_str!(
        "../fixtures/entity/edge/evidence_golden.json"
    ))
    .expect("golden manifest parses")
    .cases
}

fn golden_record(case: &GoldenCase) -> EdgeEvidenceRecord {
    build_edge_evidence_record(
        case.left_surface_id.clone(),
        case.right_surface_id.clone(),
        case_hits(&case.case_id),
    )
    .expect("golden record builds")
}

fn case_hits(case_id: &str) -> Vec<EdgeEvidenceHit> {
    match case_id {
        "sears_sears_llc_support" => vec![exact_support("exact_tenant_core"), string_support()],
        "sears_auto_center_relation_cannot_link" => vec![
            string_support(),
            protected_token_conflict_hit(ProtectedTokenConflictRequest {
                namespace: "tenant_role",
                operator_id: "protected_token_conflict:tenant_brand",
                reason_code: "protected_token_conflict",
                left_tokens: &["sears"],
                right_tokens: &["sears", "auto", "center"],
                score_units: score(10_000),
            })
            .expect("protected token conflict"),
            relation_hint_hit(RelationHintRequest {
                namespace: "ontology",
                operator_id: "relation_hint:related_brand_family",
                reason_code: "related_brand_family",
                relation: "related_brand_family",
                left_value: "Sears",
                right_value: "Sears Auto Center",
                score_units: score(10_000),
            })
            .expect("relation hint"),
        ],
        "sears_kmart_relation_distinctness" => {
            vec![
                relation_hint_hit(RelationHintRequest {
                    namespace: "ontology",
                    operator_id: "relation_hint:same_parent_or_sponsor",
                    reason_code: "same_parent_or_sponsor",
                    relation: "same_parent_or_sponsor",
                    left_value: "Kmart",
                    right_value: "Sears",
                    score_units: score(10_000),
                })
                .expect("relation hint"),
            ]
        }
        "pnc_midland_role_anti_merge" => vec![EdgeEvidenceHit::new(
            ScoreLane::AntiMerge,
            "regab_role",
            "role_conflict:servicer_vs_parent_bank",
            "role_capacity_conflict",
            score(10_000),
            true,
            "role/capacity guard keeps parent bank and loan-services division distinct",
        )],
        "platform_category_label_non_firm" => vec![EdgeEvidenceHit::new(
            ScoreLane::AntiMerge,
            "regab_role",
            "platform_category:loan_services",
            "platform_category_label",
            score(9_500),
            true,
            "platform/category label is not a firm identity",
        )],
        "alias_distinct_relation_patch_routing" => vec![
            EdgeEvidenceHit::new(
                ScoreLane::Support,
                "patches",
                "alias_patch_match",
                "alias_patch_match",
                score(10_000),
                false,
                "alias patch routes to support evidence",
            ),
            EdgeEvidenceHit::new(
                ScoreLane::AntiMerge,
                "patches",
                "alias_patch_distinct",
                "distinct_patch",
                score(10_000),
                true,
                "distinct patch routes to hard cannot-link evidence",
            ),
            relation_patch_hint_hit(RelationPatchHintRequest {
                namespace: "patches",
                operator_id: "relation_patch:possible_successor_predecessor",
                reason_code: "relation_patch_hint",
                relation: "possible_successor_predecessor",
                patch_id: "patch:sears-transform",
                source_patch_namespace: "cmbs_tenant_label.relations",
                target_patch_namespace: "cmbs_tenant_label.relations",
                score_units: score(10_000),
            })
            .expect("relation patch hint"),
        ],
        _ => panic!("unknown golden case {case_id}"),
    }
}

fn lane_names(record: &EdgeEvidenceRecord) -> Vec<String> {
    record
        .hits
        .iter()
        .map(|hit| match hit.lane {
            ScoreLane::Support => "support",
            ScoreLane::AntiMerge => "anti_merge",
            ScoreLane::RelationHint => "relation_hint",
        })
        .map(str::to_string)
        .collect()
}

fn reason_codes(record: &EdgeEvidenceRecord) -> Vec<String> {
    record
        .hits
        .iter()
        .map(|hit| hit.reason_code.clone())
        .collect()
}

fn exact_support(reason_code: &str) -> EdgeEvidenceHit {
    exact_view_support_hit(ExactViewSupportRequest {
        namespace: "name",
        operator_id: "exact_view:tenant_core",
        reason_code,
        view_name: "tenant_core",
        left_value: "sears",
        right_value: "sears",
        score_units: score(10_000),
    })
    .expect("exact support")
}

fn string_support() -> EdgeEvidenceHit {
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
    .expect("string support")
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
