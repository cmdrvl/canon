use canon::namekit::explain::{
    ReasonCode, ResolverDecisionHint, ReviewActionHint, ReviewEvidenceCategory,
    ReviewPatchVocabulary, review_reason_mapping, review_reason_mappings,
};
use serde_json::json;

#[test]
fn namekit_review_reason_mapping_covers_every_reason_code_in_stable_order() {
    let mappings = review_reason_mappings();
    let mapped_codes = mappings
        .iter()
        .map(|mapping| mapping.reason_code.as_str())
        .collect::<Vec<_>>();
    let expected_codes = ReasonCode::ALL
        .iter()
        .map(|code| code.as_str())
        .collect::<Vec<_>>();

    assert_eq!(mapped_codes, expected_codes);
    assert_eq!(mappings.len(), ReasonCode::ALL.len());
}

#[test]
fn namekit_review_reason_mapping_keeps_support_and_distinctness_separate() {
    let support = review_reason_mapping(ReasonCode::RareTokenSupport);
    assert_eq!(
        support.evidence_category,
        ReviewEvidenceCategory::SupportEvidence
    );
    assert_eq!(support.action_hint, ReviewActionHint::ReviewCandidate);
    assert_eq!(
        support.patch_vocabulary,
        Some(ReviewPatchVocabulary::AliasCandidate)
    );
    assert_eq!(
        support.resolver_decision_hint,
        ResolverDecisionHint::SameCandidate
    );

    let conflict = review_reason_mapping(ReasonCode::ProtectedTokenConflict);
    assert_eq!(
        conflict.evidence_category,
        ReviewEvidenceCategory::CannotLinkEvidence
    );
    assert_eq!(conflict.action_hint, ReviewActionHint::ReviewDistinctness);
    assert_eq!(
        conflict.patch_vocabulary,
        Some(ReviewPatchVocabulary::DistinctCandidate)
    );
    assert_eq!(
        conflict.resolver_decision_hint,
        ResolverDecisionHint::NotSameCandidate
    );
}

#[test]
fn review_reason_mapping_serializes_patch_vocabulary_without_making_merge_decisions() {
    let mappings = vec![
        review_reason_mapping(ReasonCode::UnicodeFolded),
        review_reason_mapping(ReasonCode::MetricCutoff),
        review_reason_mapping(ReasonCode::ProtectedTokenConflict),
        review_reason_mapping(ReasonCode::SourceParityReference),
    ];

    let payload = serde_json::to_value(mappings).expect("mappings serialize");
    assert_eq!(
        payload,
        json!([
            {
                "reason_code": "unicode_folded",
                "evidence_category": "normalization_transform",
                "action_hint": "explain_only",
                "patch_vocabulary": "normalization_trace",
                "resolver_decision_hint": "context_only"
            },
            {
                "reason_code": "metric_cutoff",
                "evidence_category": "risk_evidence",
                "action_hint": "review_candidate",
                "patch_vocabulary": "relation_hint",
                "resolver_decision_hint": "undecided"
            },
            {
                "reason_code": "protected_token_conflict",
                "evidence_category": "cannot_link_evidence",
                "action_hint": "review_distinctness",
                "patch_vocabulary": "distinct_candidate",
                "resolver_decision_hint": "not_same_candidate"
            },
            {
                "reason_code": "source_parity_reference",
                "evidence_category": "source_provenance",
                "action_hint": "explain_only",
                "patch_vocabulary": null,
                "resolver_decision_hint": "context_only"
            }
        ])
    );
}
