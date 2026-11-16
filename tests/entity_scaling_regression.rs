#![forbid(unsafe_code)]

use canon::entity::postings::{PostingDictionaryEntry, PostingFeatureKind, PostingLayout};

#[test]
fn posting_dictionary_lookup_stays_logarithmic_at_fifty_thousand_terms() {
    const TERM_COUNT: usize = 50_000;
    const MAX_PROBES: usize = 32;

    let dictionary = (0..TERM_COUNT)
        .map(|term_id| {
            PostingDictionaryEntry::new(
                PostingFeatureKind::Token,
                term_id as u32,
                format!("synthetic-term-{term_id:05}"),
            )
        })
        .collect::<Vec<_>>();
    let layout = PostingLayout::build(1, dictionary, Vec::new(), 0)
        .expect("synthetic dictionary layout builds");

    for term_id in [0, 1, 1_234, TERM_COUNT - 1] {
        let key = format!("synthetic-term-{term_id:05}");
        let (actual, probes) =
            layout.term_id_for_with_lookup_steps(PostingFeatureKind::Token, &key);
        assert_eq!(actual, Some(term_id as u32));
        assert!(
            probes <= MAX_PROBES,
            "lookup for {key} used {probes} probes, expected at most {MAX_PROBES}"
        );
    }

    let (actual, probes) =
        layout.term_id_for_with_lookup_steps(PostingFeatureKind::Token, "synthetic-term-absent");
    assert_eq!(actual, None);
    assert!(
        probes <= MAX_PROBES,
        "absent lookup used {probes} probes, expected at most {MAX_PROBES}"
    );
}
