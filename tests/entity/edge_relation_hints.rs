#![forbid(unsafe_code)]

use canon::entity::{
    edge::build_edge_evidence_record,
    evidence::{ExactViewSupportRequest, exact_view_support_hit},
    relation::{
        RelationHintRequest, RelationPatchHintRequest, relation_hint_hit, relation_patch_hint_hit,
    },
    score::{ScoreLane, ScoreUnits},
};

#[test]
fn edge_relation_hints_are_structurally_separate() {
    let support = exact_view_support_hit(ExactViewSupportRequest {
        namespace: "name",
        operator_id: "exact_view:tenant_core",
        reason_code: "exact_tenant_core",
        view_name: "tenant_core",
        left_value: "sears",
        right_value: "sears",
        score_units: score(10_000),
    })
    .expect("support hit");
    let relation = relation_hint_hit(RelationHintRequest {
        namespace: "ontology",
        operator_id: "relation_hint:related_brand_family",
        reason_code: "related_brand_family",
        relation: "related_brand_family",
        left_value: "Sears",
        right_value: "Sears Auto Center",
        score_units: score(10_000),
    })
    .expect("relation hint hit");

    let record = build_edge_evidence_record(
        "surf:sears",
        "surf:sears_auto_center",
        vec![relation, support],
    )
    .expect("edge record builds");

    assert_eq!(record.pair_score_total, score(10_000));
    assert!(!record.has_hard_cannot_link);
    assert_eq!(
        record
            .hits
            .iter()
            .map(|hit| (hit.lane, hit.namespace.as_str(), hit.reason_code.as_str()))
            .collect::<Vec<_>>(),
        [
            (ScoreLane::Support, "name", "exact_tenant_core"),
            (ScoreLane::RelationHint, "ontology", "related_brand_family"),
        ]
    );
    assert!(
        record
            .hits
            .iter()
            .find(|hit| hit.lane == ScoreLane::RelationHint)
            .expect("relation hit")
            .explanation
            .contains("handoff=review_and_ontology")
    );
}

#[test]
fn relation_hint_non_merge_edge() {
    let relation = relation_patch_hint_hit(RelationPatchHintRequest {
        namespace: "patches",
        operator_id: "relation_patch:tenant_related_distinct",
        reason_code: "relation_patch_hint",
        relation: "possible-successor-predecessor",
        patch_id: "patch:sears-transform",
        source_patch_namespace: "cmbs_tenant_label.relations",
        target_patch_namespace: "cmbs_tenant_label.relations",
        score_units: score(10_000),
    })
    .expect("relation patch hit");

    let record = build_edge_evidence_record("surf:sears", "surf:transform", vec![relation])
        .expect("relation-only edge record builds");

    assert_eq!(record.pair_score_total, ScoreUnits::ZERO);
    assert_eq!(record.score_breakdown.raw_support_score_units, 0);
    assert!(!record.has_hard_cannot_link);
    assert_eq!(record.hits[0].lane, ScoreLane::RelationHint);
    assert!(record.hits[0].explanation.contains("handoff=ontology"));
    assert!(
        record.hits[0]
            .explanation
            .contains("relation=possible_successor_predecessor")
    );

    let json = serde_json::to_string(&record).expect("record serializes");
    assert!(json.contains("\"relation_hint\""));
    assert!(!json.contains("\"support\""));
}

#[test]
fn relation_hints_do_not_encode_same_as_merge_authority() {
    assert!(
        relation_hint_hit(RelationHintRequest {
            namespace: "ontology",
            operator_id: "relation_hint:same_as",
            reason_code: "same_as",
            relation: "same-as",
            left_value: "Sears",
            right_value: "Sears LLC",
            score_units: score(10_000),
        })
        .is_none()
    );
    assert!(
        relation_patch_hint_hit(RelationPatchHintRequest {
            namespace: "patches",
            operator_id: "relation_patch:same_as",
            reason_code: "same_as",
            relation: "same_as",
            patch_id: "patch:bad",
            source_patch_namespace: "cmbs_tenant_label.relations",
            target_patch_namespace: "cmbs_tenant_label.relations",
            score_units: score(10_000),
        })
        .is_none()
    );
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
