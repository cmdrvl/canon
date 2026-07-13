#![forbid(unsafe_code)]

#[path = "entity/index_fixture_support.rs"]
mod index_fixture_support;

use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_INDEX_VERSION, CANON_ENTITY_INDEX_VERSION_V1, CANON_ENTITY_PREPARE_VERSION_V1,
        artifact_chain::EntityHashField,
        index::{
            EntityIndexBuildRequest, EntityIndexCacheStatus, index_build_v1_report,
            run_index_build_v1, validate_index_artifact_contract,
        },
        postings::{CommonPostingDiagnostic, PostingFeatureKind, PostingLayout},
        prepare::{PrepareRunRequest, run_prepare, run_prepare_v1},
        schema::{validate_artifact_v1_core_contract, validate_entity_v1_self_hash},
    },
};
use index_fixture_support::{
    ExpectedCommonPosting, IndexFixture, build_index_fixture, parse_fixture,
};
use std::{cmp::Reverse, collections::BTreeMap};

const SMALL_FIXTURE: &str = include_str!("fixtures/entity/index/small_cmbs_fixture.json");
const MEDIUM_FIXTURE: &str = include_str!("fixtures/entity/index/medium_common_bank_fixture.json");
const TENANT_SMALL_FIXTURE: &str =
    include_str!("fixtures/entity/index/en_i001_small_tenant_index.json");
const TENANT_MEDIUM_FIXTURE: &str =
    include_str!("fixtures/entity/index/en_i002_medium_tenant_index.json");
const REGAB_ROWS: &str =
    "tests/fixtures/entity/regab/sec10d_baseline_public/org_mentions_selected.csv";
const REGAB_REGISTRY: &str =
    "tests/fixtures/entity/regab/sec10d_baseline_public/registry_snapshot/firms";
const REGAB_STRATEGY: &str = "tests/fixtures/entity/strategies/regab_firm_identity.yaml";

#[test]
fn entity_index_golden_small_fixture_pins_postings_and_artifact_inputs() {
    let fixture = parse_fixture(SMALL_FIXTURE);
    let built = build_index_fixture(&fixture, EntityIndexCacheStatus::Rebuilt);
    let posting_index = &built.postings.posting_index;
    let ngram_index = built
        .postings
        .ngram_index
        .as_ref()
        .expect("ngram index present");

    assert_eq!(fixture.schema_version, "entity_index_fixture.v0");
    assert_eq!(posting_index.surface_ids, fixture.expected.surface_ids);
    assert_eq!(ngram_index.surface_ids, fixture.expected.surface_ids);
    assert_eq!(
        dictionary_ids(&posting_index.token_layout, PostingFeatureKind::Token),
        fixture.expected.token_term_ids
    );
    assert_eq!(
        document_frequency_by_token(posting_index),
        fixture.expected.token_document_frequency
    );
    assert_eq!(
        idf_descending(posting_index),
        fixture.expected.token_idf_descending
    );
    assert_eq!(
        postings_by_key(&posting_index.token_layout, PostingFeatureKind::Token),
        fixture.expected.token_postings
    );
    assert_eq!(
        posting_index.token_layout.term_offsets,
        fixture.expected.token_offsets
    );
    assert_eq!(
        postings_by_key(
            &posting_index.exact_view_layout,
            PostingFeatureKind::ExactView,
        ),
        fixture.expected.exact_view_postings
    );
    assert_eq!(
        posting_index.exact_view_layout.term_offsets,
        fixture.expected.exact_offsets
    );
    assert_eq!(
        dictionary_ids(&ngram_index.ngram_layout, PostingFeatureKind::Ngram),
        fixture.expected.ngram_term_ids
    );
    assert_eq!(
        postings_by_key(&ngram_index.ngram_layout, PostingFeatureKind::Ngram),
        fixture.expected.ngram_postings
    );
    assert_eq!(
        Some(ngram_index.ngram_layout.term_offsets.clone()),
        fixture.expected.ngram_offsets
    );

    assert_common_postings(
        &posting_index.exact_view_layout.common_posting_diagnostics,
        &fixture.expected.common_exact_view_diagnostics,
    );
    assert_common_postings(
        &posting_index.token_layout.common_posting_diagnostics,
        &fixture.expected.common_token_diagnostics,
    );
    assert_common_postings(
        &ngram_index.ngram_layout.common_posting_diagnostics,
        &fixture.expected.common_ngram_diagnostics,
    );
    assert_counts(&fixture, &built);
    assert_artifact_inputs(&fixture, &built);
}

#[test]
fn entity_index_golden_medium_fixture_pins_common_and_rare_terms() {
    let fixture = parse_fixture(MEDIUM_FIXTURE);
    let built = build_index_fixture(&fixture, EntityIndexCacheStatus::Rebuilt);
    let posting_index = &built.postings.posting_index;
    let ngram_index = built
        .postings
        .ngram_index
        .as_ref()
        .expect("ngram index present");

    assert_eq!(posting_index.surface_ids, fixture.expected.surface_ids);
    assert_eq!(
        dictionary_ids(&posting_index.token_layout, PostingFeatureKind::Token),
        fixture.expected.token_term_ids
    );
    assert_eq!(
        document_frequency_by_token(posting_index),
        fixture.expected.token_document_frequency
    );
    assert_eq!(
        idf_descending(posting_index),
        fixture.expected.token_idf_descending
    );
    assert_expected_postings_present(
        &posting_index.token_layout,
        PostingFeatureKind::Token,
        &fixture.expected.token_postings,
    );
    assert_eq!(
        posting_index.token_layout.term_offsets,
        fixture.expected.token_offsets
    );
    assert_eq!(
        postings_by_key(
            &posting_index.exact_view_layout,
            PostingFeatureKind::ExactView,
        ),
        fixture.expected.exact_view_postings
    );
    assert_eq!(
        posting_index.exact_view_layout.term_offsets,
        fixture.expected.exact_offsets
    );
    assert_expected_term_ids_present(
        &ngram_index.ngram_layout,
        PostingFeatureKind::Ngram,
        &fixture.expected.ngram_term_ids,
    );
    assert_expected_postings_present(
        &ngram_index.ngram_layout,
        PostingFeatureKind::Ngram,
        &fixture.expected.ngram_postings,
    );
    assert_eq!(
        Some(ngram_index.ngram_layout.term_offsets.clone()),
        fixture.expected.ngram_offsets
    );
    assert_common_postings(
        &posting_index.token_layout.common_posting_diagnostics,
        &fixture.expected.common_token_diagnostics,
    );
    assert_common_postings(
        &ngram_index.ngram_layout.common_posting_diagnostics,
        &fixture.expected.common_ngram_diagnostics,
    );
    assert_counts(&fixture, &built);
    assert_artifact_inputs(&fixture, &built);
}

#[test]
fn entity_index_golden_fixture_order_is_stable() {
    let mut fixture = parse_fixture(SMALL_FIXTURE);
    let original = build_index_fixture(&fixture, EntityIndexCacheStatus::Rebuilt);
    fixture.surfaces.reverse();
    let reversed = build_index_fixture(&fixture, EntityIndexCacheStatus::Rebuilt);

    assert_eq!(original.postings, reversed.postings);
    assert_eq!(
        serde_json::to_vec(&original.postings).expect("postings json"),
        serde_json::to_vec(&reversed.postings).expect("reversed postings json")
    );
    assert_eq!(original.artifact, reversed.artifact);
    assert_eq!(original.cache_key, reversed.cache_key);
}

#[test]
fn entity_index_golden_extra_tenant_fixtures_match_expected_contracts() {
    for fixture_text in [TENANT_SMALL_FIXTURE, TENANT_MEDIUM_FIXTURE] {
        let fixture = parse_fixture(fixture_text);
        assert_eq!(fixture.schema_version, "entity_index_fixture.v0");
        let built = build_index_fixture(&fixture, EntityIndexCacheStatus::Rebuilt);
        let posting_index = &built.postings.posting_index;
        let ngram_index = built
            .postings
            .ngram_index
            .as_ref()
            .expect("ngram index present");

        assert_eq!(posting_index.surface_ids, fixture.expected.surface_ids);
        assert_eq!(
            dictionary_ids(&posting_index.token_layout, PostingFeatureKind::Token),
            fixture.expected.token_term_ids
        );
        assert_eq!(
            document_frequency_by_token(posting_index),
            fixture.expected.token_document_frequency
        );
        assert_eq!(
            idf_descending(posting_index),
            fixture.expected.token_idf_descending
        );
        assert_eq!(
            postings_by_key(&posting_index.token_layout, PostingFeatureKind::Token),
            fixture.expected.token_postings
        );
        assert_eq!(
            postings_by_key(
                &posting_index.exact_view_layout,
                PostingFeatureKind::ExactView,
            ),
            fixture.expected.exact_view_postings
        );
        assert_eq!(
            dictionary_ids(&ngram_index.ngram_layout, PostingFeatureKind::Ngram),
            fixture.expected.ngram_term_ids
        );
        assert_eq!(
            postings_by_key(&ngram_index.ngram_layout, PostingFeatureKind::Ngram),
            fixture.expected.ngram_postings
        );
        assert_counts(&fixture, &built);
        assert_artifact_inputs(&fixture, &built);
    }
}

#[test]
fn entity_index_v1_build_consumes_prepare_v1_and_warm_hit_is_byte_identical() {
    let temp = tempfile::tempdir().expect("tempdir");
    let prepare = run_prepare_v1(PrepareRunRequest {
        rows: REGAB_ROWS.as_ref(),
        profile: "regab_firm_identity",
        registry: REGAB_REGISTRY.as_ref(),
        work_dir: temp.path(),
    })
    .expect("prepare v1");
    assert_eq!(prepare["version"], CANON_ENTITY_PREPARE_VERSION_V1);

    let request = EntityIndexBuildRequest {
        rows: REGAB_ROWS.as_ref(),
        profile: "regab_firm_identity",
        strategy: REGAB_STRATEGY.as_ref(),
        registry: REGAB_REGISTRY.as_ref(),
        work_dir: temp.path(),
        max_artifact_bytes: None,
    };
    let first = run_index_build_v1(request).expect("first index v1 build");
    let first_bytes = std::fs::read(&first.paths.artifact_path).expect("first artifact bytes");
    std::fs::write(
        temp.path().join("prepare").join("surfaces.jsonl"),
        b"{not valid prepared surfaces jsonl}\n",
    )
    .expect("poison prepared surfaces after cache publication");
    let second = run_index_build_v1(request).expect("second index v1 build");
    let second_bytes = std::fs::read(&second.paths.artifact_path).expect("second artifact bytes");
    let report = index_build_v1_report(&second);

    assert_eq!(first.cache_status, EntityIndexCacheStatus::Rebuilt);
    assert_eq!(second.cache_status, EntityIndexCacheStatus::Hit);
    assert_eq!(first_bytes, second_bytes);
    assert_eq!(first.artifact, second.artifact);
    assert_eq!(report.version, "canon_entity_index_build.v1");
    assert_eq!(second.artifact["version"], CANON_ENTITY_INDEX_VERSION_V1);
    assert_eq!(
        validate_artifact_v1_core_contract(&second.artifact)
            .expect("index v1 core contract")
            .artifact_version,
        CANON_ENTITY_INDEX_VERSION_V1
    );
    assert_eq!(
        validate_entity_v1_self_hash(&second.artifact).expect("index self hash"),
        second.artifact["artifact_content_hash"]
            .as_str()
            .expect("hash")
    );
    assert_eq!(
        second.artifact["metadata"]["upstream_artifacts"][0]["version"],
        CANON_ENTITY_PREPARE_VERSION_V1
    );
    assert_eq!(
        second.artifact["metadata"]["workdir"]["artifact_relpath"],
        "index/index.json"
    );
    assert_eq!(
        second.artifact["metadata"]["workdir"]["payload_relpath"],
        "index/postings.bin"
    );
    assert_eq!(second.artifact["postings_path"], "index/postings.bin");
    assert!(!temp.path().join("index.json").exists());
    assert!(
        !std::str::from_utf8(&second_bytes)
            .expect("utf8 artifact")
            .contains("canon_entity_index.v0"),
        "index v1 artifact must not serialize a v0 backing version"
    );
}

#[test]
fn entity_index_v1_refuses_legacy_prepare_before_index_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    run_prepare(PrepareRunRequest {
        rows: REGAB_ROWS.as_ref(),
        profile: "regab_firm_identity",
        registry: REGAB_REGISTRY.as_ref(),
        work_dir: temp.path(),
    })
    .expect("legacy prepare fixture");
    let refusal = run_index_build_v1(EntityIndexBuildRequest {
        rows: REGAB_ROWS.as_ref(),
        profile: "regab_firm_identity",
        strategy: REGAB_STRATEGY.as_ref(),
        registry: REGAB_REGISTRY.as_ref(),
        work_dir: temp.path(),
        max_artifact_bytes: None,
    })
    .expect_err("legacy prepare refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["actual_version"], "canon_entity_prepare.v0");
    assert!(!temp.path().join("index").join("index.json").exists());
    assert!(!temp.path().join("index").join("cache_key.json").exists());
}

#[test]
fn entity_index_v1_refuses_semantically_equivalent_artifact_byte_tamper() {
    let temp = tempfile::tempdir().expect("tempdir");
    run_prepare_v1(PrepareRunRequest {
        rows: REGAB_ROWS.as_ref(),
        profile: "regab_firm_identity",
        registry: REGAB_REGISTRY.as_ref(),
        work_dir: temp.path(),
    })
    .expect("prepare v1");

    let request = EntityIndexBuildRequest {
        rows: REGAB_ROWS.as_ref(),
        profile: "regab_firm_identity",
        strategy: REGAB_STRATEGY.as_ref(),
        registry: REGAB_REGISTRY.as_ref(),
        work_dir: temp.path(),
        max_artifact_bytes: None,
    };
    let first = run_index_build_v1(request).expect("first index v1 build");
    let pretty_artifact = serde_json::to_vec_pretty(&first.artifact).expect("pretty artifact json");
    std::fs::write(&first.paths.artifact_path, pretty_artifact)
        .expect("rewrite artifact with equivalent json bytes");

    let refusal =
        run_index_build_v1(request).expect_err("receipt refuses byte-level artifact drift");
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["field"], "cache_receipt");
}

fn dictionary_ids(layout: &PostingLayout, kind: PostingFeatureKind) -> BTreeMap<String, u32> {
    layout
        .dictionary
        .iter()
        .filter(|entry| entry.kind == kind)
        .map(|entry| (entry.key.clone(), entry.term_id))
        .collect()
}

fn document_frequency_by_token(
    index: &canon::entity::postings::EntityPostingIndex,
) -> BTreeMap<String, u32> {
    index
        .token_idf
        .iter()
        .map(|entry| (entry.key.clone(), entry.document_frequency))
        .collect()
}

fn idf_descending(index: &canon::entity::postings::EntityPostingIndex) -> Vec<String> {
    let mut entries = index.token_idf.clone();
    entries.sort_by_key(|entry| (Reverse(entry.idf_units), entry.key.clone()));
    entries.into_iter().map(|entry| entry.key).collect()
}

fn postings_by_key(layout: &PostingLayout, kind: PostingFeatureKind) -> BTreeMap<String, Vec<u32>> {
    layout
        .dictionary
        .iter()
        .filter(|entry| entry.kind == kind)
        .map(|entry| {
            let postings = layout
                .postings_for_term(entry.term_id)
                .expect("dictionary term has postings")
                .iter()
                .map(|posting| posting.surface_ordinal)
                .collect::<Vec<_>>();
            (entry.key.clone(), postings)
        })
        .collect()
}

fn assert_expected_term_ids_present(
    layout: &PostingLayout,
    kind: PostingFeatureKind,
    expected: &BTreeMap<String, u32>,
) {
    let actual = dictionary_ids(layout, kind);
    for (key, term_id) in expected {
        assert_eq!(actual.get(key), Some(term_id), "term id for {key}");
    }
}

fn assert_expected_postings_present(
    layout: &PostingLayout,
    kind: PostingFeatureKind,
    expected: &BTreeMap<String, Vec<u32>>,
) {
    let actual = postings_by_key(layout, kind);
    for (key, postings) in expected {
        assert_eq!(actual.get(key), Some(postings), "postings for {key}");
    }
}

fn assert_common_postings(actual: &[CommonPostingDiagnostic], expected: &[ExpectedCommonPosting]) {
    let actual = actual
        .iter()
        .map(|diagnostic| ExpectedCommonPosting {
            key: diagnostic.key.clone(),
            posting_count: diagnostic.posting_count,
            configured_limit: diagnostic.configured_limit,
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

fn assert_counts(fixture: &IndexFixture, built: &index_fixture_support::BuiltIndexFixture) {
    let posting = &built.postings.posting_index;
    let ngram = built
        .postings
        .ngram_index
        .as_ref()
        .expect("ngram index present");
    let expected = &fixture.expected.counts;

    assert_eq!(posting.diagnostics.surface_count, expected.surface_count);
    assert_eq!(
        posting.diagnostics.exact_view_count,
        expected.exact_view_count
    );
    assert_eq!(posting.diagnostics.token_count, expected.token_count);
    assert_eq!(ngram.diagnostics.ngram_count, expected.ngram_count);
    assert_eq!(
        posting.diagnostics.large_exact_view_bucket_count,
        expected.large_exact_view_bucket_count
    );
    assert_eq!(
        posting.diagnostics.common_token_count,
        expected.common_token_count
    );
    assert_eq!(
        ngram.diagnostics.common_ngram_count,
        expected.common_ngram_count
    );
    assert_eq!(
        posting.diagnostics.largest_exact_view_bucket_size,
        expected.largest_exact_view_bucket_size
    );
    assert_eq!(
        posting.diagnostics.largest_token_posting_size,
        expected.largest_token_posting_size
    );
    assert_eq!(
        ngram.diagnostics.largest_ngram_posting_size,
        expected.largest_ngram_posting_size
    );
    assert_eq!(posting.diagnostics.exact_bucket_pair_expansion_count, 0);
}

fn assert_artifact_inputs(
    fixture: &IndexFixture,
    built: &index_fixture_support::BuiltIndexFixture,
) {
    let artifact = &built.artifact;
    let cache_key = &built.cache_key;
    let expected = &fixture.expected.artifact;

    validate_index_artifact_contract(artifact).expect("artifact hash contract validates");
    assert_eq!(artifact.version, CANON_ENTITY_INDEX_VERSION);
    assert_eq!(artifact.prepare_hash, expected.prepare_hash);
    assert_eq!(
        artifact.metadata.strategy.content_hash,
        expected.strategy_hash
    );
    assert_eq!(
        artifact
            .metadata
            .profile
            .content_hash
            .as_deref()
            .expect("profile hash"),
        expected.profile_hash
    );
    assert_eq!(
        artifact.metadata.registry_snapshot.lookup_snapshot_hash,
        expected.registry_snapshot_hash
    );
    assert_eq!(
        artifact
            .metadata
            .input
            .as_ref()
            .expect("input reference")
            .content_hash,
        expected.input_hash
    );
    assert_eq!(
        artifact
            .metadata
            .patch_set
            .as_ref()
            .expect("patch set")
            .content_hash,
        expected.patch_hash
    );
    assert_eq!(
        artifact.metadata.namekit.as_ref().expect("namekit").version,
        expected.namekit_version
    );
    assert_eq!(
        artifact
            .metadata
            .namekit
            .as_ref()
            .expect("namekit")
            .content_hash,
        expected.namekit_hash
    );
    assert_eq!(
        artifact.summary.labels["cache_status"],
        expected.cache_status
    );
    assert_eq!(
        cache_key.required_hash_fields(),
        [
            EntityHashField::InputHash,
            EntityHashField::ProfileHash,
            EntityHashField::StrategyHash,
            EntityHashField::RegistrySnapshotHash,
            EntityHashField::UpstreamArtifactHash,
            EntityHashField::PatchHash,
            EntityHashField::NamekitVersion,
            EntityHashField::NamekitHash,
        ]
    );
}
