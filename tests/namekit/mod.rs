use canon::namekit::{
    NAMEKIT_SCORE_SCALE, NAMEKIT_VERSION, NamekitCapability, NamekitNgram, NamekitProfileSemantics,
    NamekitReason, NamekitSimilarityEvidence, NamekitToken, NamekitView, NamekitViewKind, NgramId,
    ReasonCode, ReasonStage, SimilarityScore, SourceTechnique, TokenId, namekit_capabilities,
};
use serde_json::json;
use std::collections::BTreeSet;

#[test]
fn namekit_module_boundary() {
    assert_eq!(NAMEKIT_VERSION, "canon_namekit.v0");
    assert_eq!(NAMEKIT_SCORE_SCALE, 10_000);

    let capabilities = namekit_capabilities();
    assert_eq!(
        capabilities,
        [
            NamekitCapability::Normalize,
            NamekitCapability::LegalSuffix,
            NamekitCapability::Tokenize,
            NamekitCapability::Ngram,
            NamekitCapability::Tfidf,
            NamekitCapability::Similarity,
            NamekitCapability::Patch,
            NamekitCapability::Explain,
        ]
    );
    let unique = capabilities.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), capabilities.len());

    assert_eq!(
        serde_json::to_value(NamekitProfileSemantics::RegAbFirmIdentity).unwrap(),
        json!("regab_firm_identity")
    );
}

#[test]
fn namekit_public_api_compiles() {
    let view = NamekitView::new(
        NamekitProfileSemantics::TenantLabel,
        NamekitViewKind::Normalized,
        "sears roebuck",
        vec![
            NamekitReason::new(ReasonCode::RareTokenSupport, ReasonStage::Tfidf)
                .with_source(SourceTechnique::SplinkTfAdjustment)
                .with_detail("token", "roebuck"),
            NamekitReason::new(ReasonCode::LegalSuffixStripped, ReasonStage::LegalSuffix)
                .with_source(SourceTechnique::Cleanco)
                .with_detail("suffix", "co"),
        ],
    );
    assert!(view.lossy);
    assert_eq!(
        view.reasons
            .iter()
            .map(|reason| reason.code.as_str())
            .collect::<Vec<_>>(),
        ["legal_suffix_stripped", "rare_token_support"]
    );

    let token = NamekitToken::new(Some(TokenId::new(7)), "sears");
    assert_eq!(token.id.unwrap().as_u32(), 7);

    let ngram = NamekitNgram::new(Some(NgramId::new(13)), "sea");
    assert_eq!(ngram.id.unwrap().as_u32(), 13);

    let score = SimilarityScore::from_scaled(9_700).expect("score within scale");
    assert_eq!(score.as_scaled(), 9_700);
    assert_eq!(SimilarityScore::ZERO.as_scaled(), 0);
    assert_eq!(SimilarityScore::EXACT.as_scaled(), NAMEKIT_SCORE_SCALE);
    assert!(SimilarityScore::from_scaled(NAMEKIT_SCORE_SCALE + 1).is_none());

    let evidence = NamekitSimilarityEvidence::new(
        NamekitViewKind::Normalized,
        NamekitViewKind::Normalized,
        score,
        vec![
            NamekitReason::new(ReasonCode::MetricCutoff, ReasonStage::Similarity)
                .with_source(SourceTechnique::RapidFuzz)
                .with_detail("metric", "jaro_winkler"),
        ],
    );

    let payload = serde_json::to_value(evidence).expect("evidence serializes");
    assert_eq!(payload["score"], 9_700);
    assert_eq!(payload["left_view"], "normalized");
    assert_eq!(payload["reasons"][0]["source"], "rapid_fuzz");
}
