#![forbid(unsafe_code)]

use canon::{
    Refusal, RefusalCode,
    entity::{
        block::{
            AliasPatchMatchBlockOperator, AliasPatchPair, BlockCandidateBudgetConfig,
            BlockCandidateGenerationRequest, BlockCandidateHit, BlockCandidateOperator,
            BlockCandidateRecord, ExactBucketBlockRequest, ExactBucketSurface,
            NgramTopKBlockOperator, RareTokenOverlapBlockOperator, emit_exact_bucket_hyperedges,
            generate_block_candidates,
        },
        block_artifact::{
            EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN, ExactBucketProfile, ExactBucketUpstream,
        },
        index::ngram_index::{EntityNgramBuildConfig, EntityNgramIndex, EntityNgramSurface},
        postings::{EntityPostingBuildConfig, EntityPostingIndex, EntityPostingSurface},
    },
    namekit::ngram::NgramConfig,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[test]
#[allow(non_snake_case)]
fn EN_B001_blocking_golden_exact_bucket_8000_matches_fixture() {
    let result = emit_exact_bucket_hyperedges(ExactBucketBlockRequest {
        profile: sample_profile(),
        upstream: sample_upstream(),
        operator_id: "exact_view:tenant_core".to_string(),
        identity_view: "tenant_core".to_string(),
        placeholder_values: BTreeSet::from(["vacant".to_string()]),
        surfaces: vec![
            ExactBucketSurface::new("surf:sears", "sears", 8_000, 934),
            ExactBucketSurface::new("surf:vacant", "vacant", 25_000, 1_200),
        ],
    })
    .expect("EN-B001 exact bucket emits");
    let assertion = &result.assertions[0];

    assert_eq!(result.assertions.len(), 1);
    assert_eq!(result.diagnostics.expanded_pair_count, 0);
    assert_eq!(assertion.expanded_pair_count(), 0);
    assert_eq!(
        assertion.pair_expansion,
        EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN
    );

    let actual = json!({
        "assertion_count": result.assertions.len(),
        "bucket_id": assertion.bucket_id,
        "row_count": assertion.row_count,
        "deal_count": assertion.deal_count,
        "membership_record_count": assertion.artifact_membership_record_count(),
        "expanded_pair_count": assertion.expanded_pair_count(),
        "suppressed_pair_count": assertion.diagnostics.suppressed_pair_count,
        "pair_expansion": assertion.pair_expansion
    });
    assert_eq!(actual, en_b001_expected_summary());
}

#[test]
#[allow(non_snake_case)]
fn EN_B002_blocking_golden_common_token_cap_records_bounded_diagnostics() {
    let posting_index = common_bank_posting_index(64);
    let result = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: None,
        budget_config: BlockCandidateBudgetConfig::new(1, 1, 1),
        operators: vec![BlockCandidateOperator::RareTokenOverlap(
            RareTokenOverlapBlockOperator::new("rare_token_overlap:tenant_tokens", "tenant_core")
                .with_min_idf_units(0)
                .with_topk(25, 25)
                .with_max_posting_size(8),
        )],
    })
    .expect("EN-B002 common token bucket remains bounded");

    assert!(result.candidates.is_empty());
    assert_eq!(result.diagnostics.candidate_pairs_emitted, 0);
    assert_eq!(result.diagnostics.candidate_pairs_per_surface_p95, 0);
    assert_eq!(result.diagnostics.candidate_pairs_per_surface_p99, 0);
    assert_eq!(result.diagnostics.large_buckets_suppressed, 64);
    assert_eq!(
        result.diagnostics.operator_diagnostics[0].large_posting_suppressed_count,
        64
    );
}

#[test]
#[allow(non_snake_case)]
fn EN_B003_blocking_golden_support_pair_sears_and_sears_llc() {
    let (posting_index, ngram_index) = sears_candidate_indexes();
    let result = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: Some(&ngram_index),
        budget_config: generous_budget(),
        operators: vec![BlockCandidateOperator::NgramTopK(
            NgramTopKBlockOperator::new("ngram_topk:tenant_core", 2, 2),
        )],
    })
    .expect("EN-B003 support candidate emits");

    let support = candidate_pair(&result.candidates, "surf:sears", "surf:sears_llc");
    assert_block_hit(support, "ngram_topk:tenant_core");
    assert!(support.candidate_score_hint > 0);
}

#[test]
#[allow(non_snake_case)]
fn EN_B004_blocking_golden_related_distinct_context_survives_for_edge() {
    let (posting_index, ngram_index) = sears_candidate_indexes();
    let result = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: Some(&ngram_index),
        budget_config: generous_budget(),
        operators: vec![
            BlockCandidateOperator::NgramTopK(NgramTopKBlockOperator::new(
                "ngram_topk:tenant_core",
                3,
                3,
            )),
            BlockCandidateOperator::AliasPatchMatch(AliasPatchMatchBlockOperator::new(
                "relation_hint:brand_family_not_same_tenant",
                vec![AliasPatchPair::new(
                    "surf:sears",
                    "surf:sears_auto",
                    "relation:sears-auto-center",
                )],
            )),
        ],
    })
    .expect("EN-B004 related/distinct context emits");

    let related = candidate_pair(&result.candidates, "surf:sears", "surf:sears_auto");
    assert_block_hit(related, "ngram_topk:tenant_core");
    assert_block_hit(related, "relation_hint:brand_family_not_same_tenant");
    assert_eq!(
        related
            .block_hits
            .iter()
            .map(|hit| hit.operator_id.as_str())
            .collect::<Vec<_>>(),
        [
            "ngram_topk:tenant_core",
            "relation_hint:brand_family_not_same_tenant"
        ]
    );
}

#[test]
#[allow(non_snake_case)]
fn EN_B005_blocking_golden_candidate_cap_refusal_matches_fixture() {
    let refusal = candidate_budget_refusal();
    assert_candidate_budget_refusal_matches_fixture(&refusal);
}

#[test]
fn blocking_candidate_budget_matrix_golden_matches_fixture() {
    let refusal = candidate_budget_refusal();
    assert_candidate_budget_refusal_matches_fixture(&refusal);
    assert_eq!(
        refusal.detail["observed_limits"]["max_candidates_for_surface"],
        refusal.detail["observed"]
    );
    assert_eq!(
        refusal.detail["configured_limits"]["max_candidates_per_surface"],
        refusal.detail["configured"]
    );
}

fn candidate_budget_refusal() -> Refusal {
    let posting_index = budget_posting_index();
    generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: None,
        budget_config: BlockCandidateBudgetConfig::new(1, 16, 16),
        operators: vec![BlockCandidateOperator::AliasPatchMatch(
            AliasPatchMatchBlockOperator::new(
                "alias_patch_match",
                vec![
                    AliasPatchPair::new("surf:sears", "surf:sears_llc", "patch:sears-llc"),
                    AliasPatchPair::new("surf:sears", "surf:sears_auto", "patch:sears-auto"),
                ],
            ),
        )],
    })
    .expect_err("EN-B005 candidate budget refuses")
}

fn assert_candidate_budget_refusal_matches_fixture(refusal: &Refusal) {
    let expected = en_b005_expected_refusal();
    assert_eq!(refusal.code, RefusalCode::EEntityCandidateBudget);
    assert_eq!(refusal.detail["refusal_code"], expected["code"]);
    for field in [
        "policy_id",
        "subject_kind",
        "subject_id",
        "observed",
        "configured",
        "candidate_artifact_bytes",
        "candidate_artifact_written",
        "partial_candidate_artifact_written",
    ] {
        assert_eq!(refusal.detail[field], expected[field], "field {field}");
    }
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|command| command.contains("per-surface cap"))
    );
}

fn en_b001_expected_summary() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/entity/block/en_b001_exact_bucket_8000/expected_summary.json"
    ))
    .expect("EN-B001 fixture parses")
}

fn en_b005_expected_refusal() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/entity/block/en_b005_budget/expected_refusal.json"
    ))
    .expect("EN-B005 fixture parses")
}

fn sears_candidate_indexes() -> (EntityPostingIndex, EntityNgramIndex) {
    let surfaces = [
        posting_surface("surf:sears", "sears", ["sears"]),
        posting_surface("surf:sears_llc", "sears llc", ["sears", "llc"]),
        posting_surface(
            "surf:sears_auto",
            "sears auto center",
            ["sears", "auto", "center"],
        ),
        posting_surface("surf:kmart", "kmart", ["kmart"]),
    ];
    let posting_index = EntityPostingIndex::build(
        &surfaces,
        EntityPostingBuildConfig {
            common_posting_limit: 10,
        },
    )
    .expect("posting index builds");
    let ngram_index = EntityNgramIndex::build(
        &[
            EntityNgramSurface::new("surf:sears", "sears"),
            EntityNgramSurface::new("surf:sears_llc", "sears llc"),
            EntityNgramSurface::new("surf:sears_auto", "sears auto center"),
            EntityNgramSurface::new("surf:kmart", "kmart"),
        ],
        EntityNgramBuildConfig {
            ngram: NgramConfig::new(3).expect("ngram width"),
            common_posting_limit: 10,
        },
    )
    .expect("ngram index builds");
    (posting_index, ngram_index)
}

fn budget_posting_index() -> EntityPostingIndex {
    EntityPostingIndex::build(
        &[
            posting_surface("surf:sears", "sears", ["sears"]),
            posting_surface("surf:sears_llc", "sears llc", ["sears", "llc"]),
            posting_surface(
                "surf:sears_auto",
                "sears auto center",
                ["sears", "auto", "center"],
            ),
        ],
        EntityPostingBuildConfig {
            common_posting_limit: 10,
        },
    )
    .expect("budget posting index builds")
}

fn common_bank_posting_index(surface_count: usize) -> EntityPostingIndex {
    let surfaces = (0..surface_count)
        .map(|index| {
            EntityPostingSurface::new(format!("surf:bank:{index:03}"))
                .with_exact_view("tenant_core", format!("bank branch {index:03}"))
                .with_tokens(["bank".to_string(), format!("unique-{index:03}")])
        })
        .collect::<Vec<_>>();
    EntityPostingIndex::build(
        &surfaces,
        EntityPostingBuildConfig {
            common_posting_limit: 8,
        },
    )
    .expect("common bank posting index builds")
}

fn posting_surface<const N: usize>(
    surface_id: &str,
    tenant_core: &str,
    tokens: [&str; N],
) -> EntityPostingSurface {
    EntityPostingSurface::new(surface_id)
        .with_exact_view("tenant_core", tenant_core)
        .with_tokens(tokens)
}

fn candidate_pair<'a>(
    candidates: &'a [BlockCandidateRecord],
    left: &str,
    right: &str,
) -> &'a BlockCandidateRecord {
    candidates
        .iter()
        .find(|candidate| candidate.left_surface_id == left && candidate.right_surface_id == right)
        .expect("candidate pair exists")
}

fn assert_block_hit(candidate: &BlockCandidateRecord, operator_id: &str) {
    assert!(
        candidate
            .block_hits
            .iter()
            .any(|hit: &BlockCandidateHit| hit.operator_id == operator_id),
        "{operator_id} hit missing from {candidate:?}"
    );
}

fn generous_budget() -> BlockCandidateBudgetConfig {
    BlockCandidateBudgetConfig::new(8, 64, 128)
}

fn sample_profile() -> ExactBucketProfile {
    ExactBucketProfile {
        id: "cmbs_tenant_label".to_string(),
        version: "0.1.0".to_string(),
        identity_semantics: "canonical_display_label".to_string(),
        content_hash: "blake3:profile".to_string(),
    }
}

fn sample_upstream() -> ExactBucketUpstream {
    ExactBucketUpstream {
        prepare_hash: "blake3:prepare".to_string(),
        index_hash: "blake3:index".to_string(),
        strategy_hash: "blake3:block-strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
    }
}
