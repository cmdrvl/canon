#![forbid(unsafe_code)]

use canon::{
    entity::{
        index::ngram_index::{
            CANON_ENTITY_NGRAM_INDEX_VERSION, EntityNgramBuildConfig, EntityNgramIndex,
            EntityNgramSurface,
        },
        postings::PostingFeatureKind,
        topk::TopKConfig,
    },
    namekit::ngram::NgramConfig,
};

#[test]
fn optimized_ngram_selection_matches_exhaustive_scoring() {
    use canon::entity::topk::{TopKCandidateInput, prune_top_k_candidates};
    use canon::namekit::ngram::char_ngrams;
    use std::collections::BTreeMap;
    let texts = [
        "alphabet soup",
        "alphabet stew",
        "alpha alphabet",
        "zeta soup",
        "soup alphabet",
        "aaaaaaa",
        "unrelated",
    ];
    let surfaces = texts
        .iter()
        .enumerate()
        .map(|(i, text)| EntityNgramSurface::new(format!("surface-{}", texts.len() - i), *text))
        .collect::<Vec<_>>();
    let config = EntityNgramBuildConfig::default();
    let index = EntityNgramIndex::build(&surfaces, config).unwrap();
    let counts = |text: &str| {
        let mut counts = BTreeMap::<String, u64>::new();
        for gram in char_ngrams(text, config.ngram).ngrams {
            *counts.entry(gram.text).or_default() += 1;
        }
        counts
    };
    for query in &surfaces {
        for k in [0, 1, 3, 20] {
            for floor in [0, 3, 100] {
                let topk = TopKConfig::new("generic", "ngram", k)
                    .with_candidate_cap(4)
                    .with_score_floor_units(floor);
                let query_counts = counts(&query.normalized_key);
                let inputs = surfaces
                    .iter()
                    .filter(|candidate| candidate.surface_id != query.surface_id)
                    .filter_map(|candidate| {
                        let candidate_counts = counts(&candidate.normalized_key);
                        let score = query_counts
                            .iter()
                            .map(|(gram, count)| {
                                count * candidate_counts.get(gram).copied().unwrap_or(0)
                            })
                            .sum::<u64>();
                        (score > 0).then(|| {
                            TopKCandidateInput::new(
                                &query.surface_id,
                                &candidate.surface_id,
                                &candidate.normalized_key,
                                u32::try_from(score).unwrap_or(u32::MAX),
                            )
                        })
                    });
                let expected = prune_top_k_candidates(topk.clone(), inputs);
                assert_eq!(
                    index.top_k_for_surface(&query.surface_id, topk).unwrap(),
                    expected
                );
            }
        }
    }
}

#[test]
fn entity_ngram_index_builds_sorted_postings_and_tracks_common_terms() {
    let first = sample_index();
    let mut reversed = sample_surfaces();
    reversed.reverse();
    let second = EntityNgramIndex::build(&reversed, sample_config()).expect("index builds");

    assert_eq!(first, second);
    assert_eq!(first.version, CANON_ENTITY_NGRAM_INDEX_VERSION);
    assert_eq!(
        first.surface_ids,
        [
            "surf:001".to_string(),
            "surf:002".to_string(),
            "surf:003".to_string(),
            "surf:004".to_string(),
        ]
    );
    assert_eq!(
        first
            .ngram_postings("ab")
            .expect("ab postings")
            .iter()
            .map(|posting| posting.surface_ordinal)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert!(
        first
            .ngram_layout
            .term_id_for(PostingFeatureKind::Ngram, "ab")
            .is_some()
    );
    assert_eq!(first.ngram_layout.common_posting_diagnostics.len(), 1);
    assert_eq!(first.ngram_layout.common_posting_diagnostics[0].key, "ab");
    assert_eq!(
        first.ngram_layout.common_posting_diagnostics[0].posting_count,
        3
    );
    assert_eq!(
        first.ngram_layout.common_posting_diagnostics[0].configured_limit,
        1
    );
    assert_eq!(first.diagnostics.surface_count, 4);
    assert_eq!(first.diagnostics.ngram_count, 10);
    assert_eq!(first.diagnostics.total_posting_count, 12);
    assert_eq!(first.diagnostics.common_ngram_count, 1);
    assert_eq!(first.diagnostics.largest_ngram_posting_size, 3);
    assert!(first.ngram_layout.validate_reload().is_ok());
}

#[test]
fn entity_ngram_index_top_k_prunes_deterministically() {
    let index = sample_index();
    let result = index
        .top_k_for_surface(
            "surf:001",
            TopKConfig::new("cmbs_tenant_label", "ngram_topk:tenant_core", 1).with_candidate_cap(2),
        )
        .expect("top-k result");

    assert_eq!(
        result
            .candidates
            .iter()
            .map(|candidate| (
                candidate.rank,
                candidate.candidate_surface_id.as_str(),
                candidate.normalized_key.as_str(),
                candidate.score_units,
            ))
            .collect::<Vec<_>>(),
        [(1, "surf:002", "abaa", 1)]
    );
    assert_eq!(result.diagnostics.input_candidate_count, 2);
    assert_eq!(result.diagnostics.eligible_candidate_count, 2);
    assert_eq!(result.diagnostics.emitted_candidate_count, 1);
    assert_eq!(result.diagnostics.dropped_candidate_count, 1);
    assert_eq!(result.diagnostics.dropped_by_candidate_cap_count, 0);
    assert_eq!(result.diagnostics.dropped_by_topk_count, 1);
    assert!(!result.diagnostics.candidate_cap_exceeded);
    assert!(result.diagnostics.topk_exceeded);
    assert!(result.dropped.is_empty());
    assert_eq!(result.diagnostics.version, "canon_entity_topk.v1");
}

#[test]
fn ngram_posting_caps_emit_budget_diagnostics() {
    let index = EntityNgramIndex::build(
        &[
            EntityNgramSurface::new("surf:003", "sears"),
            EntityNgramSurface::new("surf:002", "sears auto"),
            EntityNgramSurface::new("surf:001", "sears retail"),
        ],
        EntityNgramBuildConfig {
            ngram: NgramConfig::new(3).expect("width"),
            common_posting_limit: 2,
        },
    )
    .expect("index builds");

    assert_eq!(index.diagnostics.common_ngram_count, 3);
    assert_eq!(index.diagnostics.largest_ngram_posting_size, 3);
    assert_eq!(
        index
            .ngram_layout
            .common_posting_diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.key.as_str(),
                diagnostic.posting_count,
                diagnostic.configured_limit,
            ))
            .collect::<Vec<_>>(),
        [("ars", 3, 2), ("ear", 3, 2), ("sea", 3, 2)]
    );
}

#[test]
fn entity_ngram_index_duplicate_surface_keys_are_order_independent() {
    let first = EntityNgramIndex::build(
        &[
            EntityNgramSurface::new("surf:001", "zeta tenant"),
            EntityNgramSurface::new("surf:001", "alpha tenant"),
            EntityNgramSurface::new("surf:002", "alpha tenent"),
        ],
        sample_config(),
    )
    .expect("index builds");
    let second = EntityNgramIndex::build(
        &[
            EntityNgramSurface::new("surf:002", "alpha tenent"),
            EntityNgramSurface::new("surf:001", "alpha tenant"),
            EntityNgramSurface::new("surf:001", "zeta tenant"),
        ],
        sample_config(),
    )
    .expect("index builds");

    assert_eq!(first, second);
    let result = first
        .top_k_for_surface(
            "surf:002",
            TopKConfig::new("cmbs_tenant_label", "ngram_topk:tenant_core", 1),
        )
        .expect("top-k result");
    assert_eq!(result.candidates[0].normalized_key, "alpha tenant");
}

fn sample_index() -> EntityNgramIndex {
    EntityNgramIndex::build(&sample_surfaces(), sample_config()).expect("index builds")
}

#[test]
fn saturated_scores_use_normalized_key_order_before_surface_id() {
    let index = EntityNgramIndex::build(
        &[
            EntityNgramSurface::new("query", "a".repeat(70_000)),
            EntityNgramSurface::new("a-higher-raw-score", "a".repeat(68_000)),
            EntityNgramSurface::new("z-smaller-key", "a".repeat(66_000)),
        ],
        EntityNgramBuildConfig::default(),
    )
    .unwrap();
    let result = index
        .top_k_for_surface("query", TopKConfig::new("generic", "ngram", 1))
        .unwrap();
    assert_eq!(result.candidates[0].score_units, u32::MAX);
    assert_eq!(result.candidates[0].candidate_surface_id, "z-smaller-key");
    assert_eq!(result.diagnostics.input_candidate_count, 2);
    assert_eq!(result.diagnostics.dropped_by_topk_count, 1);
}

fn sample_config() -> EntityNgramBuildConfig {
    EntityNgramBuildConfig {
        ngram: NgramConfig::new(2).expect("width"),
        common_posting_limit: 1,
    }
}

fn sample_surfaces() -> Vec<EntityNgramSurface> {
    vec![
        EntityNgramSurface::new("surf:003", "abzz"),
        EntityNgramSurface::new("surf:001", "abxy"),
        EntityNgramSurface::new("surf:004", "pqrs"),
        EntityNgramSurface::new("surf:002", "abaa"),
    ]
}
