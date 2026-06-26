#![forbid(unsafe_code)]

use canon::{
    entity::postings::{
        EntityPostingBuildConfig, EntityPostingIndex, EntityPostingSurface, PostingFeatureKind,
        PostingLayoutError,
    },
    namekit::tfidf::idf_units,
};

#[test]
fn entity_index_postings_build_token_idf_and_exact_view_memberships() {
    let index = sample_index();

    assert_eq!(index.version, "canon_entity_postings.v0");
    assert_eq!(
        index.surface_ids,
        [
            "surf:cmbs:001".to_string(),
            "surf:cmbs:002".to_string(),
            "surf:cmbs:003".to_string()
        ]
    );

    let sears_token_id = index
        .token_layout
        .term_id_for(PostingFeatureKind::Token, "sears")
        .expect("sears token id");
    assert_eq!(
        sears_token_id, 3,
        "token IDs come from sorted namekit symbols"
    );
    assert_eq!(
        index
            .token_postings("sears")
            .expect("sears token postings")
            .iter()
            .map(|posting| posting.surface_ordinal)
            .collect::<Vec<_>>(),
        [0, 2]
    );

    let sears_idf = index.token_idf("sears").expect("sears idf summary");
    assert_eq!(sears_idf.document_frequency, 2);
    assert_eq!(sears_idf.idf_units, idf_units(3, 2));

    assert_eq!(
        index
            .exact_view_postings("tenant_core", "sears")
            .expect("sears exact bucket")
            .iter()
            .map(|posting| posting.surface_ordinal)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    let sears_bucket = index
        .exact_view_buckets()
        .expect("exact buckets")
        .into_iter()
        .find(|bucket| bucket.view_name == "tenant_core" && bucket.value == "sears")
        .expect("sears bucket");
    assert_eq!(sears_bucket.surface_count, 2);
    assert_eq!(sears_bucket.pair_expansion, "forbidden");

    assert_eq!(index.diagnostics.surface_count, 3);
    assert_eq!(index.diagnostics.token_count, 4);
    assert_eq!(index.diagnostics.tfidf_term_count, 4);
    assert_eq!(index.diagnostics.common_token_count, 1);
    assert_eq!(index.diagnostics.large_exact_view_bucket_count, 1);
    assert_eq!(index.diagnostics.suppressed_exact_view_pair_count, 1);
    assert_eq!(index.diagnostics.exact_bucket_pair_expansion_count, 0);
}

#[test]
fn postings_are_deterministic() {
    let first = sample_index();
    let second = EntityPostingIndex::build(
        &[
            EntityPostingSurface::new("surf:cmbs:003")
                .with_exact_view("tenant_core", "sears")
                .with_tokens(["sears", "auto", "sears"]),
            EntityPostingSurface::new("surf:cmbs:001")
                .with_exact_view("tenant_core", "sears")
                .with_tokens(["roebuck", "sears"]),
            EntityPostingSurface::new("surf:cmbs:002")
                .with_exact_view("tenant_core", "kmart")
                .with_tokens(["kmart"]),
            EntityPostingSurface::new("surf:cmbs:001")
                .with_exact_view("tenant_core", "sears")
                .with_tokens(["sears", "roebuck"]),
        ],
        sample_config(),
    )
    .expect("posting index builds");

    assert_eq!(first, second);
    assert_eq!(
        serde_json::to_vec(&first).expect("index serializes"),
        serde_json::to_vec(&second).expect("repeat index serializes")
    );
    assert!(
        first
            .token_layout
            .term_offsets
            .windows(2)
            .all(|window| window[0] <= window[1])
    );
    assert!(
        first
            .tfidf_layout
            .term_offsets
            .windows(2)
            .all(|window| window[0] <= window[1])
    );
}

#[test]
fn entity_index_postings_reject_unknown_dictionary_key_on_reload_lookup() {
    let index = sample_index();
    assert_eq!(
        index.token_postings("missing"),
        Err(PostingLayoutError::UnknownDictionaryKey {
            kind: PostingFeatureKind::Token,
            key: "missing".to_string()
        })
    );
    index
        .token_layout
        .validate_reload()
        .expect("token layout reloads");
    index
        .tfidf_layout
        .validate_reload()
        .expect("tfidf layout reloads");
    index
        .exact_view_layout
        .validate_reload()
        .expect("exact layout reloads");
}

fn sample_index() -> EntityPostingIndex {
    EntityPostingIndex::build(
        &[
            EntityPostingSurface::new("surf:cmbs:002")
                .with_exact_view("tenant_core", "kmart")
                .with_tokens(["kmart"]),
            EntityPostingSurface::new("surf:cmbs:001")
                .with_exact_view("tenant_core", "sears")
                .with_tokens(["sears", "roebuck", "sears"]),
            EntityPostingSurface::new("surf:cmbs:003")
                .with_exact_view("tenant_core", "sears")
                .with_tokens(["auto", "sears"]),
        ],
        sample_config(),
    )
    .expect("posting index builds")
}

fn sample_config() -> EntityPostingBuildConfig {
    EntityPostingBuildConfig {
        common_posting_limit: 1,
    }
}
