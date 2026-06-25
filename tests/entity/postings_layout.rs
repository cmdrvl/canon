use canon::entity::postings::{
    ENTITY_POSTINGS_LAYOUT_VERSION, PostingDictionaryEntry, PostingFeatureKind, PostingInput,
    PostingLayout, PostingLayoutError,
};

#[test]
fn postings_layout_contract() {
    let first = sample_layout();
    let second = PostingLayout::build(
        5,
        sample_dictionary().into_iter().rev().collect(),
        vec![
            PostingInput::new(2, 4, 4_200),
            PostingInput::new(0, 1, 8_000),
            PostingInput::new(1, 2, 7_000),
            PostingInput::new(2, 0, 4_200),
            PostingInput::new(0, 0, 8_000),
            PostingInput::new(0, 3, 8_000),
        ],
        2,
    )
    .expect("layout builds");

    assert_eq!(first.version, ENTITY_POSTINGS_LAYOUT_VERSION);
    assert_eq!(first.dictionary_hash, second.dictionary_hash);
    assert_eq!(first.term_offsets, vec![0, 3, 4, 6]);
    assert_eq!(first.postings.len(), 6);
    assert_eq!(
        first
            .postings_for_term(0)
            .expect("token term postings")
            .iter()
            .map(|posting| posting.surface_ordinal)
            .collect::<Vec<_>>(),
        [0, 1, 3]
    );
    assert_eq!(
        first
            .postings_for_term(2)
            .expect("ngram term postings")
            .iter()
            .map(|posting| posting.surface_ordinal)
            .collect::<Vec<_>>(),
        [0, 4]
    );
    assert_eq!(first.common_posting_diagnostics.len(), 1);
    assert_eq!(first.common_posting_diagnostics[0].term_id, 0);
    assert_eq!(first.common_posting_diagnostics[0].key, "sears");
    assert_eq!(first.common_posting_diagnostics[0].posting_count, 3);

    first.validate_reload().expect("layout validates");
}

#[test]
fn postings_layout_round_trip() {
    let layout = sample_layout();
    let bytes = serde_json::to_vec(&layout).expect("layout serializes");
    let reloaded: PostingLayout = serde_json::from_slice(&bytes).expect("layout deserializes");

    assert_eq!(layout, reloaded);
    reloaded
        .validate_reload()
        .expect("reloaded layout validates");
    assert_eq!(reloaded.posting_range(1).expect("range"), 3..4);

    let mut corrupt_offsets = reloaded.clone();
    corrupt_offsets.term_offsets[1] = 99;
    assert_eq!(
        corrupt_offsets.validate_reload(),
        Err(PostingLayoutError::OffsetsNotMonotonic)
    );

    let mut corrupt_hash = reloaded;
    corrupt_hash.dictionary_hash = "blake3:wrong".to_string();
    assert!(matches!(
        corrupt_hash.validate_reload(),
        Err(PostingLayoutError::DictionaryHashMismatch { .. })
    ));
}

fn sample_layout() -> PostingLayout {
    PostingLayout::build(
        5,
        sample_dictionary(),
        vec![
            PostingInput::new(0, 3, 8_000),
            PostingInput::new(2, 4, 4_200),
            PostingInput::new(0, 0, 8_000),
            PostingInput::new(1, 2, 7_000),
            PostingInput::new(2, 0, 4_200),
            PostingInput::new(0, 1, 8_000),
        ],
        2,
    )
    .expect("sample layout builds")
}

fn sample_dictionary() -> Vec<PostingDictionaryEntry> {
    vec![
        PostingDictionaryEntry::new(PostingFeatureKind::Token, 0, "sears"),
        PostingDictionaryEntry::new(PostingFeatureKind::Token, 1, "roebuck"),
        PostingDictionaryEntry::new(PostingFeatureKind::Ngram, 2, "sea"),
    ]
}
