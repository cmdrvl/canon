#![forbid(unsafe_code)]

use canon::entity::{
    edge::build_edge_evidence_record,
    profiles::regab::{
        RegabFirmGuardKind, RegabFirmGuardRequest, RegabReviewCue, normalize_regab_firm_name,
        regab_firm_guard_hit,
    },
    relation::{RelationHintRequest, relation_hint_hit},
    score::{
        CandidateScoreDecisionReason, ScoreLane, ScoreThreshold, ScoreUnits, ScoredCandidate,
        evaluate_candidate_score,
    },
};
use serde::Deserialize;
use std::collections::BTreeSet;

const HARD_NEGATIVES: &str =
    include_str!("../fixtures/entity/regab/hard_negatives/role_capacity_guards.json");
const REGAB_I002_SOLVE: &str = include_str!("../fixtures/entity/regab/regab_i002_solve.json");
const REGAB_I003_SOLVE: &str = include_str!("../fixtures/entity/regab/regab_i003_solve.json");

#[derive(Debug, Deserialize)]
struct RegabHardNegativeFixture {
    version: String,
    profile_id: String,
    identity_semantics: String,
    merge_policy: String,
    cases: Vec<RegabHardNegativeCase>,
}

#[derive(Debug, Deserialize)]
struct RegabHardNegativeCase {
    id: String,
    benchmark_id: String,
    left: String,
    right: String,
    surface_ids: Vec<String>,
    left_role: String,
    right_role: String,
    guard: String,
    relation: String,
    expected_review_priority: String,
    expected_auto_merge: bool,
    expected_cues: Vec<String>,
}

#[test]
fn regab_i002_and_i003_hard_negatives_emit_guarded_anti_merge() {
    let fixture = hard_negatives();

    assert_eq!(
        fixture.version,
        "canon_entity_regab_hard_negative_fixture.v0"
    );
    assert_eq!(fixture.profile_id, "regab_firm_identity");
    assert_eq!(fixture.identity_semantics, "same_firm_or_reviewed_alias");
    assert_eq!(
        fixture.merge_policy,
        "no_silent_parent_division_or_role_collapse"
    );
    assert_required_guard_cases(&fixture);

    for case in fixture.cases {
        assert_eq!(
            case.surface_ids.len(),
            2,
            "{} should declare one deterministic edge pair",
            case.id
        );
        assert!(
            case.surface_ids[0] < case.surface_ids[1],
            "{} surface IDs must already satisfy edge artifact ordering",
            case.id
        );
        assert!(
            !case.expected_auto_merge,
            "{} should be non-auto-merge",
            case.id
        );

        let guard = RegabFirmGuardKind::from_code(case.guard.as_str())
            .unwrap_or_else(|| panic!("unknown guard {}", case.guard));
        assert_eq!(guard.review_priority(), case.expected_review_priority);

        let cannot_link = regab_firm_guard_hit(RegabFirmGuardRequest {
            namespace: "regab_firm_identity.hard_negatives",
            guard,
            left_name: case.left.as_str(),
            right_name: case.right.as_str(),
            left_role: Some(case.left_role.as_str()),
            right_role: Some(case.right_role.as_str()),
            score_units: score(10_000),
        })
        .expect("Reg AB guard emits cannot-link evidence");
        let relation = relation_hint_hit(RelationHintRequest {
            namespace: "regab_firm_identity.relations",
            operator_id: "relation_hint:regab_guard",
            reason_code: "regab_relation_context",
            relation: case.relation.as_str(),
            left_value: case.left.as_str(),
            right_value: case.right.as_str(),
            score_units: score(10_000),
        })
        .expect("Reg AB guard emits relation context");

        let record = build_edge_evidence_record(
            case.surface_ids[0].clone(),
            case.surface_ids[1].clone(),
            vec![relation, cannot_link],
        )
        .expect("Reg AB hard-negative edge record builds");

        assert_eq!(record.pair_score_total, ScoreUnits::ZERO);
        assert_eq!(record.score_breakdown.raw_support_score_units, 0);
        assert!(record.has_hard_cannot_link);
        assert!(record.hits.iter().any(|hit| {
            hit.lane == ScoreLane::AntiMerge
                && hit.hard_cannot_link
                && hit.operator_id == guard.operator_id()
                && hit.reason_code == guard.reason_code()
                && hit
                    .explanation
                    .contains(format!("review_priority={}", guard.review_priority()).as_str())
        }));
        assert!(record.hits.iter().any(|hit| {
            hit.lane == ScoreLane::RelationHint
                && hit.reason_code == "regab_relation_context"
                && hit.explanation.contains("handoff=review_and_ontology")
        }));
        assert_expected_cues(&case);

        let decision = evaluate_candidate_score(
            &ScoredCandidate::new(
                format!("candidate:{}", case.id),
                case.surface_ids[0].clone(),
                case.surface_ids[1].clone(),
                record.pair_score_total,
                record.has_hard_cannot_link,
            ),
            ScoreThreshold::new(score(1)),
        );
        assert!(!decision.accepted, "{} must not auto-merge", case.id);
        assert_eq!(
            decision.reason,
            CandidateScoreDecisionReason::HardCannotLink
        );
    }
}

#[test]
fn regab_i002_i003_summary_fixtures_pin_no_silent_collapse() {
    let i002: serde_json::Value = serde_json::from_str(REGAB_I002_SOLVE).expect("REGAB-I002 json");
    let i003: serde_json::Value = serde_json::from_str(REGAB_I003_SOLVE).expect("REGAB-I003 json");

    assert_eq!(i002["fixture_id"], "REGAB-I002");
    assert_eq!(i002["summary"]["cannot_link_count"], 2);
    assert_eq!(i002["summary"]["auto_merge_count"], 0);

    assert_eq!(i003["fixture_id"], "REGAB-I003");
    assert_eq!(i003["summary"]["review_group_count"], 2);
    assert_eq!(i003["summary"]["auto_merge_count"], 0);
}

fn hard_negatives() -> RegabHardNegativeFixture {
    serde_json::from_str(HARD_NEGATIVES).expect("Reg AB hard-negative fixture parses")
}

fn assert_required_guard_cases(fixture: &RegabHardNegativeFixture) {
    let ids = fixture
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    for expected in [
        "REGAB-I002-PNC-MIDLAND-DIVISION",
        "REGAB-HIER-002-WELLS-FARGO-SERVICING-DIVISION",
        "REGAB-I003-PLATFORM-LABEL",
        "REGAB-ROLE-SERVICER-AGENT-CONFLICT",
        "REGAB-ROLE-AUDITOR-SUBJECT-CONFLICT",
        "REGAB-PARENT-SUBSIDIARY-DEPOSITOR-SPV",
        "REGAB-SAME-FAMILY-PNC-CAPITAL-MARKETS",
    ] {
        assert!(
            ids.contains(expected),
            "missing Reg AB guard case {expected}"
        );
    }

    let benchmarks = fixture
        .cases
        .iter()
        .map(|case| case.benchmark_id.as_str())
        .collect::<BTreeSet<_>>();
    assert!(benchmarks.contains("REGAB-I002"));
    assert!(benchmarks.contains("REGAB-I003"));
}

fn assert_expected_cues(case: &RegabHardNegativeCase) {
    let left = normalize_regab_firm_name(case.left.as_str());
    let right = normalize_regab_firm_name(case.right.as_str());
    let cue_codes = left
        .review_cues
        .iter()
        .chain(right.review_cues.iter())
        .map(|cue| cue.code())
        .collect::<BTreeSet<_>>();

    for expected in &case.expected_cues {
        assert!(
            cue_codes.contains(expected.as_str()),
            "{} should expose review cue {}",
            case.id,
            expected
        );
    }
    if case.guard == "platform_category_label" {
        assert!(
            left.review_cues.contains(&RegabReviewCue::PlatformLabel)
                || right.review_cues.contains(&RegabReviewCue::PlatformLabel)
        );
    }
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
