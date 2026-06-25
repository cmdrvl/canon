use canon::namekit::ReasonCode;
use canon::namekit::ngram::{NgramConfig, NgramView, ngram_fingerprint, trigrams};

#[test]
fn namekit_ngram() {
    let ngrams = trigrams("sears");

    assert_eq!(ngrams.version, "canon_namekit_ngram.v0");
    assert_eq!(ngrams.view, NgramView::Sequence);
    assert_eq!(ngrams.width, 3);
    assert!(!ngrams.lossy);
    assert!(ngrams.reasons.is_empty());
    assert_eq!(
        ngrams
            .ngrams
            .iter()
            .map(|ngram| (ngram.id.unwrap().as_u32(), ngram.text.as_str()))
            .collect::<Vec<_>>(),
        [(2, "sea"), (1, "ear"), (0, "ars")]
    );
    assert_eq!(
        ngrams
            .symbol_table
            .entries
            .iter()
            .map(|entry| (entry.id.as_u32(), entry.value.as_str()))
            .collect::<Vec<_>>(),
        [(0, "ars"), (1, "ear"), (2, "sea")]
    );
}

#[test]
fn ngram_fingerprint_is_sorted_deduped_and_reason_coded() {
    let fingerprint = ngram_fingerprint("sears sears", NgramConfig::DEFAULT);

    assert_eq!(fingerprint.view, NgramView::Fingerprint);
    assert!(fingerprint.lossy);
    assert_eq!(fingerprint.fingerprint, "ars ear rss sea sse");
    assert_eq!(
        fingerprint
            .ngrams
            .iter()
            .map(|ngram| ngram.text.as_str())
            .collect::<Vec<_>>(),
        ["ars", "ear", "rss", "sea", "sse"]
    );
    assert_eq!(
        fingerprint
            .reasons
            .iter()
            .map(|reason| reason.code)
            .collect::<Vec<_>>(),
        [ReasonCode::NgramFingerprintCollision]
    );
    assert_eq!(fingerprint.reasons[0].detail["width"], "3");
}

#[test]
fn ngrams_use_char_boundaries_for_unicode() {
    let config = NgramConfig::new(3).expect("nonzero width");
    let ngrams = canon::namekit::ngram::char_ngrams("éclair", config);

    assert_eq!(
        ngrams
            .ngrams
            .iter()
            .map(|ngram| ngram.text.as_str())
            .collect::<Vec<_>>(),
        ["écl", "cla", "lai", "air"]
    );
    assert!(NgramConfig::new(0).is_none());
}
