use super::index_fixture_support::{
    ExpectedCommonPosting, IndexFixture, build_index_fixture, parse_fixture,
};
use canon::entity::{
    index::{EntityIndexCacheStatus, validate_index_artifact_contract},
    postings::{PostingFeatureKind, PostingLayout},
};
use std::{cmp::Reverse, collections::BTreeMap};

const SMALL_FIXTURE: &str =
    include_str!("../fixtures/entity/index/en_i001_small_index_golden.json");
const MEDIUM_FIXTURE: &str =
    include_str!("../fixtures/entity/index/en_i002_medium_cache_reload.json");

#[test]
fn entity_index_golden_small_fixture_pins_ids_postings_and_layouts() {
    let fixture = parse_fixture(SMALL_FIXTURE);
    assert_eq!(fixture.schema_version, "canon_entity_index_golden.v0");

    let built = build_index_fixture(&fixture, EntityIndexCacheStatus::Rebuilt);
    let posting = &built.postings.posting_index;
    let ngram = built.postings.ngram_index.as_ref().expect("ngram index");

    assert_eq!(posting.surface_ids, fixture.expected.surface_ids);
    assert_eq!(ngram.surface_ids, fixture.expected.surface_ids);
    assert_eq!(
        term_ids(&posting.token_layout, PostingFeatureKind::Token),
        fixture.expected.token_term_ids
    );
    assert_eq!(
        document_frequency(posting),
        fixture.expected.token_document_frequency
    );
    assert_eq!(idf_order(posting), fixture.expected.token_idf_descending);
    assert_eq!(
        posting.token_layout.term_offsets,
        fixture.expected.token_offsets
    );
    assert_eq!(
        posting.exact_view_layout.term_offsets,
        fixture.expected.exact_offsets
    );

    for (token, expected) in &fixture.expected.token_postings {
        assert_eq!(token_postings(posting, token), *expected, "{token}");
    }
    for (key, expected) in &fixture.expected.exact_view_postings {
        let (view, value) = key.split_once(':').expect("view:value expected key");
        assert_eq!(exact_postings(posting, view, value), *expected, "{key}");
    }
    for (ngram_key, expected) in &fixture.expected.ngram_postings {
        assert_eq!(ngram_postings(ngram, ngram_key), *expected, "{ngram_key}");
    }
    for (ngram_key, expected) in &fixture.expected.ngram_term_ids {
        assert_eq!(
            ngram
                .ngram_layout
                .term_id_for(PostingFeatureKind::Ngram, ngram_key),
            Some(*expected),
            "{ngram_key}"
        );
    }
    assert_eq!(
        ngram.ngram_layout.term_offsets,
        fixture
            .expected
            .ngram_offsets
            .clone()
            .expect("small fixture pins ngram offsets")
    );

    assert_common_diagnostics(
        &posting.exact_view_layout,
        &fixture.expected.common_exact_view_diagnostics,
    );
    assert_common_diagnostics(
        &posting.token_layout,
        &fixture.expected.common_token_diagnostics,
    );
    assert_common_diagnostics(
        &ngram.ngram_layout,
        &fixture.expected.common_ngram_diagnostics,
    );
    assert_counts(&fixture, &built);
    assert_artifact_hash_inputs(&fixture, &built);
    assert_input_order_invariance(&fixture);
}

#[test]
fn entity_index_golden_medium_fixture_pins_common_and_rare_terms() {
    let fixture = parse_fixture(MEDIUM_FIXTURE);
    assert_eq!(fixture.schema_version, "canon_entity_index_golden.v0");

    let built = build_index_fixture(&fixture, EntityIndexCacheStatus::Rebuilt);
    let posting = &built.postings.posting_index;
    let ngram = built.postings.ngram_index.as_ref().expect("ngram index");

    assert_eq!(posting.surface_ids, fixture.expected.surface_ids);
    assert_eq!(
        term_ids(&posting.token_layout, PostingFeatureKind::Token),
        fixture.expected.token_term_ids
    );
    assert_eq!(
        document_frequency(posting),
        fixture.expected.token_document_frequency
    );
    assert_eq!(idf_order(posting), fixture.expected.token_idf_descending);
    for (token, expected) in &fixture.expected.token_postings {
        assert_eq!(token_postings(posting, token), *expected, "{token}");
    }
    for (key, expected) in &fixture.expected.exact_view_postings {
        let (view, value) = key.split_once(':').expect("view:value expected key");
        assert_eq!(exact_postings(posting, view, value), *expected, "{key}");
    }
    for (ngram_key, expected) in &fixture.expected.ngram_postings {
        assert_eq!(ngram_postings(ngram, ngram_key), *expected, "{ngram_key}");
    }
    for (ngram_key, expected) in &fixture.expected.ngram_term_ids {
        assert_eq!(
            ngram
                .ngram_layout
                .term_id_for(PostingFeatureKind::Ngram, ngram_key),
            Some(*expected),
            "{ngram_key}"
        );
    }

    assert_common_diagnostics(
        &posting.exact_view_layout,
        &fixture.expected.common_exact_view_diagnostics,
    );
    assert_common_diagnostics(
        &posting.token_layout,
        &fixture.expected.common_token_diagnostics,
    );
    for expected in &fixture.expected.common_ngram_diagnostics {
        let actual = ngram
            .ngram_layout
            .common_posting_diagnostics
            .iter()
            .find(|diagnostic| diagnostic.key == expected.key)
            .unwrap_or_else(|| panic!("missing common ngram {}", expected.key));
        assert_eq!(actual.posting_count, expected.posting_count);
        assert_eq!(actual.configured_limit, expected.configured_limit);
    }
    assert_counts(&fixture, &built);
    assert_artifact_hash_inputs(&fixture, &built);
    assert_input_order_invariance(&fixture);
}

fn assert_input_order_invariance(fixture: &IndexFixture) {
    let first = build_index_fixture(fixture, EntityIndexCacheStatus::Rebuilt);
    let mut reversed = fixture.clone();
    reversed.surfaces.reverse();
    let second = build_index_fixture(&reversed, EntityIndexCacheStatus::Rebuilt);

    assert_eq!(first.postings, second.postings);
    assert_eq!(
        serde_json::to_vec(&first.postings).expect("first postings serialize"),
        serde_json::to_vec(&second.postings).expect("second postings serialize")
    );
}

fn assert_counts(fixture: &IndexFixture, built: &super::index_fixture_support::BuiltIndexFixture) {
    let posting = &built.postings.posting_index;
    let ngram = built.postings.ngram_index.as_ref().expect("ngram index");
    let expected = &fixture.expected.counts;

    assert_eq!(posting.diagnostics.surface_count, expected.surface_count);
    assert_eq!(
        posting.diagnostics.exact_view_count,
        expected.exact_view_count
    );
    assert_eq!(posting.diagnostics.token_count, expected.token_count);
    assert_eq!(
        posting.diagnostics.large_exact_view_bucket_count,
        expected.large_exact_view_bucket_count
    );
    assert_eq!(
        posting.diagnostics.common_token_count,
        expected.common_token_count
    );
    assert_eq!(
        posting.diagnostics.largest_exact_view_bucket_size,
        expected.largest_exact_view_bucket_size
    );
    assert_eq!(
        posting.diagnostics.largest_token_posting_size,
        expected.largest_token_posting_size
    );
    assert_eq!(ngram.diagnostics.ngram_count, expected.ngram_count);
    assert_eq!(
        ngram.diagnostics.common_ngram_count,
        expected.common_ngram_count
    );
    assert_eq!(
        ngram.diagnostics.largest_ngram_posting_size,
        expected.largest_ngram_posting_size
    );
}

fn assert_artifact_hash_inputs(
    fixture: &IndexFixture,
    built: &super::index_fixture_support::BuiltIndexFixture,
) {
    validate_index_artifact_contract(&built.artifact).expect("artifact contract validates");
    let expected = &fixture.expected.artifact;
    assert_eq!(built.artifact.prepare_hash, expected.prepare_hash);
    assert_eq!(
        built.artifact.metadata.strategy.content_hash,
        expected.strategy_hash
    );
    assert_eq!(
        built.artifact.metadata.profile.content_hash.as_deref(),
        Some(expected.profile_hash.as_str())
    );
    assert_eq!(
        built
            .artifact
            .metadata
            .registry_snapshot
            .lookup_snapshot_hash,
        expected.registry_snapshot_hash
    );
    assert_eq!(
        built
            .artifact
            .metadata
            .input
            .as_ref()
            .expect("input metadata")
            .content_hash,
        expected.input_hash
    );
    assert_eq!(
        built
            .artifact
            .metadata
            .patch_set
            .as_ref()
            .expect("patch metadata")
            .content_hash,
        expected.patch_hash
    );
    assert_eq!(
        built
            .artifact
            .metadata
            .namekit
            .as_ref()
            .expect("namekit metadata")
            .version,
        expected.namekit_version
    );
    assert_eq!(
        built
            .artifact
            .metadata
            .namekit
            .as_ref()
            .expect("namekit metadata")
            .content_hash,
        expected.namekit_hash
    );
    assert_eq!(
        built.artifact.summary.labels["cache_status"],
        expected.cache_status
    );
    assert_eq!(built.cache_key.input_hash, expected.input_hash);
    assert_eq!(built.cache_key.profile_hash, expected.profile_hash);
    assert_eq!(built.cache_key.strategy_hash, expected.strategy_hash);
    assert_eq!(
        built.cache_key.registry_snapshot_hash,
        expected.registry_snapshot_hash
    );
    assert_eq!(
        built.cache_key.patch_hash.as_deref(),
        Some(expected.patch_hash.as_str())
    );
    assert_eq!(built.cache_key.namekit_version, expected.namekit_version);
    assert_eq!(
        built.cache_key.namekit_hash.as_deref(),
        Some(expected.namekit_hash.as_str())
    );
}

fn term_ids(layout: &PostingLayout, kind: PostingFeatureKind) -> BTreeMap<String, u32> {
    layout
        .dictionary
        .iter()
        .filter(|entry| entry.kind == kind)
        .map(|entry| (entry.key.clone(), entry.term_id))
        .collect()
}

fn document_frequency(
    posting: &canon::entity::postings::EntityPostingIndex,
) -> BTreeMap<String, u32> {
    posting
        .token_idf
        .iter()
        .map(|entry| (entry.key.clone(), entry.document_frequency))
        .collect()
}

fn idf_order(posting: &canon::entity::postings::EntityPostingIndex) -> Vec<String> {
    let mut entries = posting.token_idf.clone();
    entries.sort_by_key(|entry| (Reverse(entry.idf_units), entry.key.clone()));
    entries.into_iter().map(|entry| entry.key).collect()
}

fn token_postings(posting: &canon::entity::postings::EntityPostingIndex, token: &str) -> Vec<u32> {
    posting
        .token_postings(token)
        .expect("token postings")
        .iter()
        .map(|posting| posting.surface_ordinal)
        .collect()
}

fn exact_postings(
    posting: &canon::entity::postings::EntityPostingIndex,
    view: &str,
    value: &str,
) -> Vec<u32> {
    posting
        .exact_view_postings(view, value)
        .expect("exact postings")
        .iter()
        .map(|posting| posting.surface_ordinal)
        .collect()
}

fn ngram_postings(
    ngram: &canon::entity::index::ngram_index::EntityNgramIndex,
    key: &str,
) -> Vec<u32> {
    ngram
        .ngram_postings(key)
        .expect("ngram postings")
        .iter()
        .map(|posting| posting.surface_ordinal)
        .collect()
}

fn assert_common_diagnostics(layout: &PostingLayout, expected: &[ExpectedCommonPosting]) {
    let actual = layout
        .common_posting_diagnostics
        .iter()
        .map(|diagnostic| ExpectedCommonPosting {
            key: diagnostic.key.clone(),
            posting_count: diagnostic.posting_count,
            configured_limit: diagnostic.configured_limit,
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}
