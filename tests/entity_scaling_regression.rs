#![forbid(unsafe_code)]

use canon::{
    entity::postings::{PostingDictionaryEntry, PostingFeatureKind, PostingLayout},
    namekit::tfidf::{SparseTfidfModel, TfidfInputSurface, TfidfQueryWorkspace, TopKConfig},
};

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

#[test]
fn tfidf_common_token_keeps_materialized_topk_linear() {
    let small = measure_common_token_work(128);
    let large = measure_common_token_work(256);

    assert_eq!(small.postings_visited, 128 * 129);
    assert_eq!(large.postings_visited, 256 * 257);
    assert_eq!(small.candidates_materialized, 128 * 5);
    assert_eq!(large.candidates_materialized, 256 * 5);
    assert!(
        large.candidates_materialized <= small.candidates_materialized * 3,
        "materialized candidates grew superlinearly: small={small:?} large={large:?}"
    );
}

fn measure_common_token_work(surface_count: usize) -> canon::namekit::tfidf::TfidfQueryStats {
    let surfaces = (0..surface_count)
        .map(|ordinal| {
            TfidfInputSurface::tokenized(
                format!("surface-{ordinal:04}"),
                format!("common unique-{ordinal:04}"),
                ["common".to_string(), format!("unique-{ordinal:04}")],
            )
        })
        .collect::<Vec<_>>();
    let model = SparseTfidfModel::build(&surfaces);
    let mut workspace = TfidfQueryWorkspace::default();
    let config = TopKConfig::new(5).with_candidate_cap(5);
    let mut total = canon::namekit::tfidf::TfidfQueryStats::default();
    for ordinal in 0..surface_count {
        model.top_k_for_ordinal_with_workspace(ordinal, config, &mut workspace);
        let stats = workspace.stats();
        total.postings_visited += stats.postings_visited;
        total.candidates_materialized += stats.candidates_materialized;
    }
    total
}
