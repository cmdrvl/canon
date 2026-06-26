#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        block::{
            AliasPatchMatchBlockOperator, AliasPatchPair, BlockCandidateBudgetConfig,
            BlockCandidateGenerationRequest, BlockCandidateOperator, NgramTopKBlockOperator,
            RareTokenOverlapBlockOperator, generate_block_candidates,
        },
        diagnostics::{block_index_limit_refusal, summarize_block_candidate_diagnostics},
        index::ngram_index::{EntityNgramBuildConfig, EntityNgramIndex, EntityNgramSurface},
        postings::{EntityPostingBuildConfig, EntityPostingIndex, EntityPostingSurface},
    },
    namekit::ngram::NgramConfig,
};
use serde_json::json;

#[test]
fn entity_block_diagnostics_summary_is_stable_and_operator_facing() {
    let (posting_index, ngram_index) = diagnostic_indexes();
    let budget = generous_budget();
    let result = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: Some(&ngram_index),
        budget_config: budget.clone(),
        operators: vec![
            BlockCandidateOperator::NgramTopK(NgramTopKBlockOperator::new(
                "ngram_topk:tenant_core",
                2,
                2,
            )),
            BlockCandidateOperator::AliasPatchMatch(AliasPatchMatchBlockOperator::new(
                "alias_patch_match",
                vec![AliasPatchPair::new("surf:003", "surf:004", "patch:manual")],
            )),
        ],
    })
    .expect("diagnostic candidates generate");
    let artifact_bytes = serde_json::to_vec(&result.candidates)
        .expect("candidate records serialize")
        .len() as u64;

    let summary = summarize_block_candidate_diagnostics(&budget, &result, artifact_bytes);

    assert_eq!(result.diagnostics.configured_budget, budget);
    assert_eq!(result.diagnostics.candidate_artifact_bytes, artifact_bytes);
    assert!(!result.diagnostics.operator_yield.is_empty());
    assert_eq!(summary.stage, "block");
    assert_eq!(summary.configured.max_candidates_per_surface, 8);
    assert_eq!(
        summary.observed.candidate_record_count,
        result.candidates.len() as u64
    );
    assert_eq!(summary.observed.candidate_artifact_bytes, artifact_bytes);
    assert_eq!(
        summary
            .top_blocking_operators_by_yield
            .iter()
            .map(|operator| operator.operator_id.as_str())
            .collect::<Vec<_>>(),
        ["ngram_topk:tenant_core", "alias_patch_match"]
    );
    assert!(summary.top_large_postings.is_empty());
}

#[test]
#[allow(non_snake_case)]
fn EN_B005_candidate_budget_refuses_before_artifact_write() {
    let (posting_index, ngram_index) = diagnostic_indexes();
    let refusal = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: Some(&ngram_index),
        budget_config: BlockCandidateBudgetConfig::new(0, 100, 100),
        operators: vec![BlockCandidateOperator::NgramTopK(
            NgramTopKBlockOperator::new("ngram_topk:tenant_core", 2, 2),
        )],
    })
    .expect_err("over-budget candidate generation refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityCandidateBudget);
    assert_eq!(
        refusal.detail["policy_id"],
        "block.max_candidates_per_surface"
    );
    assert_eq!(refusal.detail["refusal_code"], "E_ENTITY_CANDIDATE_BUDGET");
    assert_eq!(
        refusal.detail["configured_limits"]["max_candidates_per_surface"],
        0
    );
    assert_eq!(
        refusal.detail["observed_limits"]["max_candidates_for_surface"],
        2
    );
    assert_eq!(refusal.detail["stage"], "block");
    assert_eq!(refusal.detail["candidate_artifact_written"], json!(false));
    assert_eq!(
        refusal.detail["partial_candidate_artifact_written"],
        json!(false)
    );
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|command| command.contains("per-surface cap"))
    );
}

#[test]
#[allow(non_snake_case)]
fn E_ENTITY_CANDIDATE_BUDGET_refusal_reports_exact_boundary() {
    let (posting_index, ngram_index) = diagnostic_indexes();
    let refusal = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: Some(&ngram_index),
        budget_config: BlockCandidateBudgetConfig::new(100, 0, 100),
        operators: vec![BlockCandidateOperator::NgramTopK(
            NgramTopKBlockOperator::new("ngram_topk:tenant_core", 2, 2),
        )],
    })
    .expect_err("operator budget breach refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityCandidateBudget);
    assert_eq!(
        refusal.detail["policy_id"],
        "block.max_candidates_per_operator"
    );
    assert_eq!(refusal.detail["subject_kind"], "operator");
    assert_eq!(refusal.detail["subject_id"], "ngram_topk:tenant_core");
    assert_eq!(refusal.detail["refusal_code"], "E_ENTITY_CANDIDATE_BUDGET");
    assert_eq!(
        refusal.detail["configured_limits"]["max_candidates_per_operator"],
        0
    );
    assert_eq!(
        refusal.detail["observed_limits"]["max_candidates_for_operator"],
        refusal.detail["observed"]
    );
    assert!(
        refusal.detail["observed"]
            .as_u64()
            .is_some_and(|value| value > 0)
    );
    assert_eq!(refusal.detail["configured"], 0);
}

#[test]
#[allow(non_snake_case)]
fn E_ENTITY_INDEX_LIMIT_refusal_reports_large_bucket_boundary() {
    let refusal = block_index_limit_refusal(
        "exact_view:tenant_core",
        "exact_bucket",
        "bucket:tenant_core:sears",
        8_000,
        100,
        0,
    );

    assert_eq!(refusal.code, RefusalCode::EEntityIndexLimit);
    assert_eq!(refusal.detail["stage"], "block");
    assert_eq!(refusal.detail["reason"], "index_limit_exceeded");
    assert_eq!(refusal.detail["policy_id"], "block.max_exact_bucket_size");
    assert_eq!(refusal.detail["operator_id"], "exact_view:tenant_core");
    assert_eq!(refusal.detail["subject_kind"], "exact_bucket");
    assert_eq!(refusal.detail["observed"], 8_000);
    assert_eq!(refusal.detail["configured"], 100);
    assert_eq!(refusal.detail["candidate_artifact_written"], json!(false));
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|command| command.contains("compact exact-bucket"))
    );
}

#[test]
fn large_posting_diagnostics_are_sorted_and_counted() {
    let posting_index = common_bank_posting_index(16);
    let budget = generous_budget();
    let result = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: None,
        budget_config: budget.clone(),
        operators: vec![BlockCandidateOperator::RareTokenOverlap(
            RareTokenOverlapBlockOperator::new("rare_token_overlap:tenant_tokens", "tenant_core")
                .with_min_idf_units(0)
                .with_topk(25, 25)
                .with_max_posting_size(4),
        )],
    })
    .expect("large posting suppresses instead of expanding");
    let summary = summarize_block_candidate_diagnostics(&budget, &result, 2);

    assert!(result.candidates.is_empty());
    assert_eq!(summary.observed.large_buckets_suppressed, 16);
    assert_eq!(
        summary.top_large_postings,
        [canon::entity::diagnostics::BlockLargePostingDiagnostic {
            operator_id: "rare_token_overlap:tenant_tokens".to_string(),
            suppressed_posting_count: 16
        }]
    );
}

fn diagnostic_indexes() -> (EntityPostingIndex, EntityNgramIndex) {
    let posting_index = EntityPostingIndex::build(
        &[
            posting_surface("surf:001", "sears roebuck", ["sears", "roebuck"]),
            posting_surface(
                "surf:002",
                "sears roebuck store",
                ["sears", "roebuck", "store"],
            ),
            posting_surface("surf:003", "kmart", ["kmart"]),
            posting_surface("surf:004", "sears auto center", ["sears", "auto", "center"]),
        ],
        EntityPostingBuildConfig {
            common_posting_limit: 10,
        },
    )
    .expect("posting index builds");
    let ngram_index = EntityNgramIndex::build(
        &[
            EntityNgramSurface::new("surf:004", "sears auto center"),
            EntityNgramSurface::new("surf:002", "sears roebuck store"),
            EntityNgramSurface::new("surf:001", "sears roebuck"),
            EntityNgramSurface::new("surf:003", "kmart"),
        ],
        EntityNgramBuildConfig {
            ngram: NgramConfig::new(3).expect("ngram width"),
            common_posting_limit: 10,
        },
    )
    .expect("ngram index builds");
    (posting_index, ngram_index)
}

fn common_bank_posting_index(surface_count: usize) -> EntityPostingIndex {
    let surfaces = (0..surface_count)
        .map(|index| {
            EntityPostingSurface::new(format!("surf:{index:03}"))
                .with_exact_view("tenant_core", format!("bank branch {index:03}"))
                .with_tokens(["bank".to_string(), format!("unique-{index:03}")])
        })
        .collect::<Vec<_>>();
    EntityPostingIndex::build(
        &surfaces,
        EntityPostingBuildConfig {
            common_posting_limit: 4,
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

fn generous_budget() -> BlockCandidateBudgetConfig {
    BlockCandidateBudgetConfig::new(8, 64, 128)
}
