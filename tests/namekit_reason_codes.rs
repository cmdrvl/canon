use canon::namekit::{
    NAMEKIT_EXPLAIN_VERSION, NamekitExplainTrace, NamekitReason, ReasonCode, ReasonStage,
    SourceTechnique, sort_reasons,
};
use serde_json::Value;
use std::{collections::BTreeSet, fs};

#[test]
fn namekit_reason_codes_are_stable_strings_and_cover_lossy_operations() {
    let expected = [
        "no_loss",
        "unicode_folded",
        "punctuation_removed",
        "control_removed",
        "whitespace_collapsed",
        "legal_suffix_stripped",
        "legal_suffix_preserved",
        "tokens_sorted",
        "tokens_deduped",
        "ngram_fingerprint_collision",
        "common_token_downweighted",
        "rare_token_support",
        "metric_cutoff",
        "protected_token_conflict",
        "profile_token_preserved",
        "profile_token_dropped",
        "source_parity_reference",
    ];
    let actual = ReasonCode::ALL
        .iter()
        .map(|code| code.as_str())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);

    let unique = actual.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), actual.len(), "reason code strings are unique");

    for code in ReasonCode::LOSSY {
        assert!(code.is_lossy(), "{code} must be marked lossy");
        assert!(
            !code.summary().is_empty(),
            "{code} must have an explain summary"
        );
    }

    let fixture = fs::read_to_string("tests/fixtures/namekit/reason_codes/taxonomy.jsonl")
        .expect("taxonomy fixture");
    let fixture_codes = fixture
        .lines()
        .map(|line| serde_json::from_str::<NamekitReason>(line).expect("reason fixture JSON"))
        .map(|reason| reason.code.as_str())
        .collect::<Vec<_>>();
    assert_eq!(fixture_codes, expected);
}

#[test]
fn normalization_reason_order_is_deterministic() {
    let mut first = vec![
        NamekitReason::new(ReasonCode::TokensDeduped, ReasonStage::Tokenize),
        NamekitReason::new(ReasonCode::UnicodeFolded, ReasonStage::Normalize),
        NamekitReason::new(ReasonCode::LegalSuffixStripped, ReasonStage::LegalSuffix),
        NamekitReason::new(ReasonCode::WhitespaceCollapsed, ReasonStage::Normalize),
    ];
    let mut second = first.iter().cloned().rev().collect::<Vec<_>>();

    sort_reasons(&mut first);
    sort_reasons(&mut second);

    let ordered = first
        .iter()
        .map(|reason| reason.code.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ordered,
        [
            "unicode_folded",
            "whitespace_collapsed",
            "legal_suffix_stripped",
            "tokens_deduped"
        ]
    );
    assert_eq!(first, second);
}

#[test]
fn entity_explain_namekit_trace_serializes_context_for_support_and_risk() {
    let trace = NamekitExplainTrace::new(
        "cmbs_tenant_label",
        "tenant_core",
        "Sears, Roebuck and Co.",
        "sears roebuck",
        vec![
            NamekitReason::new(ReasonCode::RareTokenSupport, ReasonStage::Tfidf)
                .with_source(SourceTechnique::SplinkTfAdjustment)
                .with_detail("token", "roebuck")
                .with_detail("weight", "rare"),
            NamekitReason::new(
                ReasonCode::ProtectedTokenConflict,
                ReasonStage::ProtectedToken,
            )
            .with_source(SourceTechnique::CanonProfile)
            .with_detail("left", "sears")
            .with_detail("right", "sears auto center"),
            NamekitReason::new(ReasonCode::PunctuationRemoved, ReasonStage::Normalize)
                .with_source(SourceTechnique::Normality)
                .with_detail("removed", ", ."),
            NamekitReason::new(ReasonCode::LegalSuffixStripped, ReasonStage::LegalSuffix)
                .with_source(SourceTechnique::Cleanco)
                .with_detail("suffix", "co"),
        ],
    );

    assert_eq!(trace.version, NAMEKIT_EXPLAIN_VERSION);
    assert!(trace.lossy);
    let codes = trace
        .reasons
        .iter()
        .map(|reason| reason.code.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        codes,
        [
            "punctuation_removed",
            "legal_suffix_stripped",
            "rare_token_support",
            "protected_token_conflict"
        ]
    );

    let payload = serde_json::to_value(&trace).expect("trace serializes");
    assert_eq!(payload["version"], "canon_namekit_explain.v0");
    assert_eq!(payload["profile_id"], "cmbs_tenant_label");
    assert_eq!(payload["view"], "tenant_core");
    assert_eq!(payload["reasons"][0]["source"], "normality");
    assert_eq!(payload["reasons"][1]["source"], "cleanco");
    assert_eq!(payload["reasons"][2]["source"], "splink_tf_adjustment");
    assert_eq!(payload["reasons"][3]["code"], "protected_token_conflict");
    assert_eq!(
        payload["reasons"][3]["detail"]["right"],
        "sears auto center"
    );

    let round_trip: NamekitExplainTrace =
        serde_json::from_value::<NamekitExplainTrace>(payload).expect("trace deserializes");
    assert_eq!(round_trip, trace);

    let json_text = serde_json::to_string(&round_trip).unwrap();
    let json: Value = serde_json::from_str(&json_text).unwrap();
    assert_eq!(json["lossy"], true);
}
