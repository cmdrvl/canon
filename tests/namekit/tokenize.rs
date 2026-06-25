use canon::namekit::ReasonCode;
use canon::namekit::tokenize::{TokenizeView, tokenize_sorted_unique, tokenize_words};
use serde_json::json;

#[test]
fn namekit_tokenize() {
    let tokenization = tokenize_words("sears sears llc");

    assert_eq!(tokenization.version, "canon_namekit_tokenize.v0");
    assert_eq!(tokenization.view, TokenizeView::Sequence);
    assert!(!tokenization.lossy);
    assert!(tokenization.reasons.is_empty());
    assert_eq!(
        tokenization
            .tokens
            .iter()
            .map(|token| (token.id.unwrap().as_u32(), token.text.as_str()))
            .collect::<Vec<_>>(),
        [(1, "sears"), (1, "sears"), (0, "llc")]
    );
    assert_eq!(
        tokenization
            .symbol_table
            .entries
            .iter()
            .map(|entry| (entry.id.as_u32(), entry.value.as_str()))
            .collect::<Vec<_>>(),
        [(0, "llc"), (1, "sears")]
    );

    let payload = serde_json::to_value(&tokenization).expect("tokenization serializes");
    assert_eq!(payload["tokens"][0], json!({"id": 1, "text": "sears"}));
    assert_eq!(payload["symbol_table"]["entries"][0]["value"], "llc");
}

#[test]
fn tokenization_sorted_unique_is_reason_coded() {
    let tokenization = tokenize_sorted_unique("sears roebuck sears");

    assert_eq!(tokenization.view, TokenizeView::SortedUnique);
    assert!(tokenization.lossy);
    assert_eq!(
        tokenization
            .tokens
            .iter()
            .map(|token| token.text.as_str())
            .collect::<Vec<_>>(),
        ["roebuck", "sears"]
    );
    assert_eq!(
        tokenization
            .reasons
            .iter()
            .map(|reason| reason.code)
            .collect::<Vec<_>>(),
        [ReasonCode::TokensSorted, ReasonCode::TokensDeduped]
    );
    assert_eq!(tokenization.reasons[0].detail["operation"], "token_sort");
    assert_eq!(tokenization.reasons[1].detail["operation"], "token_dedupe");
}

#[test]
fn tokenization_keeps_unicode_tokens_without_locale_folding() {
    let tokenization = tokenize_words("cafe café pnc");

    assert_eq!(
        tokenization
            .tokens
            .iter()
            .map(|token| token.text.as_str())
            .collect::<Vec<_>>(),
        ["cafe", "café", "pnc"]
    );
    assert_eq!(
        tokenization
            .symbol_table
            .entries
            .iter()
            .map(|entry| entry.value.as_str())
            .collect::<Vec<_>>(),
        ["cafe", "café", "pnc"]
    );
}
