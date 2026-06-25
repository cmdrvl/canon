use canon::namekit::tfidf::{
    DEFAULT_TF_CAP, IDF_UNITS_SCALE, NAMEKIT_TFIDF_VERSION, SCORE_UNITS_CAP, SparseTfidfIndex,
    TermEntry, TfidfConfig, build_sparse_tfidf, build_sparse_tfidf_with_config,
    sorted_neighborhood_pairs, top_k_for_surface,
};

#[test]
fn namekit_tfidf_sparse_builds_deterministic_dictionary_rows_and_postings() {
    let docs = corpus();
    let first = build_sparse_tfidf(&docs);
    let second = build_sparse_tfidf(&docs);

    assert_eq!(first, second);
    assert_eq!(first.version, NAMEKIT_TFIDF_VERSION);
    assert_eq!(first.config.tf_cap, DEFAULT_TF_CAP);
    assert_eq!(first.config.idf_scale, IDF_UNITS_SCALE);
    assert_eq!(first.config.score_cap, SCORE_UNITS_CAP);
    assert_eq!(first.document_count, 4);
    assert_eq!(first.term_offsets.len(), first.dictionary.len() + 1);
    assert_eq!(first.term_offsets.first(), Some(&0));
    assert_eq!(first.term_offsets.last(), Some(&first.postings.len()));

    let terms = first
        .dictionary
        .iter()
        .map(|entry| entry.term.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        terms,
        [
            "auto", "bank", "center", "national", "pnc", "roebuck", "sears"
        ]
    );

    for row in &first.rows {
        assert!(
            row.terms
                .windows(2)
                .all(|pair| pair[0].term_id < pair[1].term_id),
            "{} terms are sorted by term_id",
            row.surface_id
        );
    }
}

#[test]
fn tfidf_rare_token_weighting_downweights_common_terms() {
    let index = build_sparse_tfidf(&corpus());
    let sears = term(&index, "sears");
    let roebuck = term(&index, "roebuck");

    assert!(sears.document_frequency > roebuck.document_frequency);
    assert!(sears.idf_units < roebuck.idf_units);

    let row = index
        .rows
        .iter()
        .find(|row| row.surface_id == "s1")
        .expect("s1 row");
    let sears_weight = row
        .terms
        .iter()
        .find(|weight| weight.term == "sears")
        .expect("sears weight");
    let roebuck_weight = row
        .terms
        .iter()
        .find(|weight| weight.term == "roebuck")
        .expect("roebuck weight");

    assert_eq!(
        sears_weight.reason(index.document_count).code.as_str(),
        "common_token_downweighted"
    );
    assert_eq!(
        roebuck_weight.reason(index.document_count).code.as_str(),
        "rare_token_support"
    );
    assert!(roebuck_weight.weight_units > sears_weight.weight_units);
}

#[test]
fn namekit_tfidf_top_k_uses_score_then_surface_id_tie_order() {
    let index = build_sparse_tfidf(&corpus());
    let candidates = top_k_for_surface(&index, "s1", 3);

    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.surface_id.as_str())
            .collect::<Vec<_>>(),
        ["s2", "s3"]
    );
    assert!(candidates[0].score_units > candidates[1].score_units);
    assert_eq!(top_k_for_surface(&index, "missing", 10), []);
}

#[test]
fn namekit_tfidf_caps_repeated_boilerplate_tf_units() {
    let docs = [("a", &["sears", "sears", "sears", "sears"][..])];
    let index = build_sparse_tfidf_with_config(
        &docs,
        TfidfConfig {
            tf_cap: 2,
            idf_scale: 1_000,
            score_cap: 10_000,
        },
    );

    let weight = &index.rows[0].terms[0];
    assert_eq!(weight.tf_units, 2);
    assert_eq!(weight.weight_units, weight.idf_units * 2);
}

#[test]
fn sorted_neighborhood_is_deterministic_supplemental_recall_with_cap_diagnostics() {
    let result = sorted_neighborhood_pairs(
        &[
            ("s3", "sears roebuck"),
            ("s1", "sears"),
            ("s2", "sears auto"),
            ("s4", "pnc bank"),
        ],
        3,
        3,
    );

    assert_eq!(result.window, 3);
    assert_eq!(result.cap, 3);
    assert_eq!(result.emitted_pair_count, 3);
    assert!(result.capped_pair_count > 0);
    assert_eq!(
        result
            .pairs
            .iter()
            .map(|pair| (
                pair.left_surface_id.as_str(),
                pair.right_surface_id.as_str()
            ))
            .collect::<Vec<_>>(),
        [("s1", "s2"), ("s1", "s3"), ("s2", "s3")]
    );
}

fn corpus() -> [(&'static str, &'static [&'static str]); 4] {
    [
        ("s1", &["sears", "roebuck"]),
        ("s2", &["sears", "roebuck"]),
        ("s3", &["sears", "auto", "center"]),
        ("s4", &["pnc", "bank", "national"]),
    ]
}

fn term<'a>(index: &'a SparseTfidfIndex, needle: &str) -> &'a TermEntry {
    index
        .dictionary
        .iter()
        .find(|entry| entry.term == needle)
        .unwrap_or_else(|| panic!("missing term {needle}"))
}
