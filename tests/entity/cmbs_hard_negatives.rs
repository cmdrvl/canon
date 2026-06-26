#![forbid(unsafe_code)]

use canon::entity::{
    anti_merge::{RelatedDistinctPhraseRequest, related_distinct_phrase_hit},
    edge::{EdgeEvidenceHit, build_edge_evidence_record},
    evidence::{TokenOverlapSupportRequest, token_overlap_support_hit},
    relation::{RelationPatchHintRequest, relation_patch_hint_hit},
    score::{
        CandidateScoreDecisionReason, ScoreLane, ScoreThreshold, ScoreUnits, ScoredCandidate,
        evaluate_candidate_score,
    },
};
use serde::Deserialize;

const HARD_NEGATIVES: &str =
    include_str!("../fixtures/entity/cmbs/hard_negatives/sears_family.json");
const ALIAS_PATCHES: &str =
    include_str!("../fixtures/entity/patches/cmbs_tenant_label.aliases.json");
const DISTINCT_PATCHES: &str =
    include_str!("../fixtures/entity/patches/cmbs_tenant_label.distinct.json");
const RELATION_PATCHES: &str =
    include_str!("../fixtures/entity/patches/cmbs_tenant_label.relations.json");

#[derive(Debug, Deserialize)]
struct HardNegativeFixture {
    version: String,
    profile_id: String,
    identity_semantics: String,
    case_id: String,
    merge_policy: String,
    pairs: Vec<HardNegativePair>,
}

#[derive(Debug, Deserialize)]
struct HardNegativePair {
    id: String,
    left: String,
    right: String,
    left_surface_id: String,
    right_surface_id: String,
    shared_support_tokens: Vec<String>,
    distinct_phrases: Vec<String>,
    relation: String,
    expected_review: String,
    expected_auto_merge: bool,
}

#[derive(Debug, Deserialize)]
struct PatchFixture {
    version: String,
    profile_id: String,
    namespace: String,
    patches: Vec<PatchCase>,
}

#[derive(Debug, Deserialize)]
struct PatchCase {
    patch_id: String,
    canonical_hint: Option<String>,
    inputs: Option<Vec<String>>,
    left: Option<String>,
    right: Option<String>,
    reason: Option<String>,
    relation: Option<String>,
    expected_lane: String,
    expected_operator_id: String,
    expected_reason_code: String,
    merge_authorized: Option<bool>,
    review_policy: Option<String>,
}

#[test]
fn cmbs_i002_sears_family_hard_negatives_never_auto_merge() {
    let fixture = hard_negatives();

    assert_eq!(
        fixture.version,
        "canon_entity_cmbs_hard_negative_fixture.v0"
    );
    assert_eq!(fixture.profile_id, "cmbs_tenant_label");
    assert_eq!(fixture.identity_semantics, "canonical_display_label");
    assert_eq!(fixture.case_id, "CMBS-I002");
    assert_eq!(fixture.merge_policy, "no_silent_collapse");
    assert_eq!(fixture.pairs.len(), 4);

    for pair in fixture.pairs {
        let mut hits = Vec::new();
        if let Some(support) = support_hit(&pair) {
            hits.push(support);
        }
        hits.push(cannot_link_hit(&pair));
        hits.push(relation_hint_hit(&pair));

        let record = build_edge_evidence_record(
            pair.left_surface_id.clone(),
            pair.right_surface_id.clone(),
            hits,
        )
        .expect("CMBS hard-negative edge record builds");

        assert!(
            !pair.expected_auto_merge,
            "{} should be marked as non-auto-merge",
            pair.id
        );
        assert!(
            pair.expected_review.contains("related_distinct"),
            "{} should route to related/distinct review",
            pair.id
        );
        assert!(
            record.has_hard_cannot_link,
            "{} should carry a hard cannot-link constraint",
            pair.id
        );
        assert!(
            record
                .hits
                .iter()
                .any(|hit| hit.lane == ScoreLane::AntiMerge
                    && hit.hard_cannot_link
                    && hit.reason_code == "cmbs_hard_negative"),
            "{} should include anti-merge evidence",
            pair.id
        );
        assert!(
            record
                .hits
                .iter()
                .any(|hit| hit.lane == ScoreLane::RelationHint
                    && hit.reason_code == "cmbs_relation_hint"
                    && hit.explanation.contains("handoff=review_and_ontology")),
            "{} should include relation context for review/ontology",
            pair.id
        );

        let decision = evaluate_candidate_score(
            &ScoredCandidate::new(
                format!("candidate:{}", pair.id),
                pair.left_surface_id,
                pair.right_surface_id,
                record.pair_score_total,
                record.has_hard_cannot_link,
            ),
            ScoreThreshold::new(score(1)),
        );
        assert!(!decision.accepted, "{} must not auto-merge", pair.id);
        assert_eq!(
            decision.reason,
            CandidateScoreDecisionReason::HardCannotLink,
            "{} must be vetoed by hard cannot-link, not score thresholding",
            pair.id
        );
    }
}

#[test]
fn cmbs_patch_fixtures_route_to_support_anti_merge_and_relation_lanes() {
    let aliases = patch_fixture(ALIAS_PATCHES);
    let distinct = patch_fixture(DISTINCT_PATCHES);
    let relations = patch_fixture(RELATION_PATCHES);

    assert_patch_header(&aliases, "cmbs_tenant_label.aliases");
    assert_patch_header(&distinct, "cmbs_tenant_label.distinct");
    assert_patch_header(&relations, "cmbs_tenant_label.relations");

    let alias = aliases.patches.first().expect("alias patch fixture");
    assert_eq!(alias.expected_lane, "support");
    assert_eq!(alias.expected_operator_id, "alias_patch_match:tenant_core");
    assert_eq!(alias.expected_reason_code, "alias_patch_match");
    assert_eq!(alias.canonical_hint.as_deref(), Some("TNT-SEARS"));
    assert_eq!(
        alias.inputs.as_deref().expect("alias inputs"),
        ["Sears", "SEARS LLC", "Sears Roebuck & Co.", "Sears #1234"]
    );

    for patch in &distinct.patches {
        let left = patch.left.as_deref().expect("distinct left");
        let right = patch.right.as_deref().expect("distinct right");
        let reason = patch.reason.as_deref().expect("distinct reason");
        let right_phrase = right.to_ascii_lowercase();
        let hit = related_distinct_phrase_hit(RelatedDistinctPhraseRequest {
            namespace: distinct.namespace.as_str(),
            operator_id: patch.expected_operator_id.as_str(),
            reason_code: patch.expected_reason_code.as_str(),
            left_value: left,
            right_value: right,
            phrases: &[right_phrase.as_str()],
            score_units: score(10_000),
        })
        .expect("distinct patch emits cannot-link evidence");

        assert_eq!(patch.expected_lane, "anti_merge");
        assert_eq!(hit.lane, ScoreLane::AntiMerge);
        assert!(hit.hard_cannot_link);
        assert_eq!(hit.reason_code, "distinct_patch");
        assert!(
            reason == "related_brand_family_not_same_tenant_label"
                || reason == "successor_or_operator_not_display_label"
        );
    }

    for patch in &relations.patches {
        assert_eq!(patch.expected_lane, "relation_hint");
        assert_eq!(patch.merge_authorized, Some(false));
        assert_eq!(patch.review_policy.as_deref(), Some("relation_hint_only"));

        let hit = relation_patch_hint_hit(RelationPatchHintRequest {
            namespace: relations.namespace.as_str(),
            operator_id: patch.expected_operator_id.as_str(),
            reason_code: patch.expected_reason_code.as_str(),
            relation: patch.relation.as_deref().expect("relation"),
            patch_id: patch.patch_id.as_str(),
            source_patch_namespace: relations.namespace.as_str(),
            target_patch_namespace: relations.namespace.as_str(),
            score_units: score(10_000),
        })
        .expect("relation patch emits relation hint");

        assert_eq!(hit.lane, ScoreLane::RelationHint);
        assert!(!hit.hard_cannot_link);
        assert_eq!(hit.reason_code, "relation_patch_hint");
        assert!(hit.explanation.contains("handoff=ontology"));
    }
}

fn hard_negatives() -> HardNegativeFixture {
    serde_json::from_str(HARD_NEGATIVES).expect("hard-negative fixture parses")
}

fn patch_fixture(raw: &str) -> PatchFixture {
    serde_json::from_str(raw).expect("patch fixture parses")
}

fn assert_patch_header(fixture: &PatchFixture, namespace: &str) {
    assert_eq!(fixture.version, "canon_entity_patch_fixture.v0");
    assert_eq!(fixture.profile_id, "cmbs_tenant_label");
    assert_eq!(fixture.namespace, namespace);
    assert!(!fixture.patches.is_empty());
}

fn support_hit(pair: &HardNegativePair) -> Option<EdgeEvidenceHit> {
    let shared_tokens = pair
        .shared_support_tokens
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    token_overlap_support_hit(TokenOverlapSupportRequest {
        namespace: "cmbs_tenant_label.support",
        operator_id: "token_overlap:tenant_tokens",
        reason_code: "shared_tenant_token",
        left_tokens: &shared_tokens,
        right_tokens: &shared_tokens,
        min_shared_tokens: 1,
    })
}

fn cannot_link_hit(pair: &HardNegativePair) -> EdgeEvidenceHit {
    let phrases = pair
        .distinct_phrases
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    related_distinct_phrase_hit(RelatedDistinctPhraseRequest {
        namespace: "cmbs_tenant_label.hard_negatives",
        operator_id: "related_distinct_phrase:cmbs_i002",
        reason_code: "cmbs_hard_negative",
        left_value: pair.left.as_str(),
        right_value: pair.right.as_str(),
        phrases: &phrases,
        score_units: score(10_000),
    })
    .expect("CMBS hard-negative pair emits cannot-link")
}

fn relation_hint_hit(pair: &HardNegativePair) -> EdgeEvidenceHit {
    canon::entity::relation::relation_hint_hit(canon::entity::relation::RelationHintRequest {
        namespace: "cmbs_tenant_label.relations",
        operator_id: "relation_hint:cmbs_i002",
        reason_code: "cmbs_relation_hint",
        relation: pair.relation.as_str(),
        left_value: pair.left.as_str(),
        right_value: pair.right.as_str(),
        score_units: score(10_000),
    })
    .expect("CMBS hard-negative pair emits relation hint")
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
