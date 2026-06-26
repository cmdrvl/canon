#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        block::{
            AliasPatchMatchBlockOperator, AliasPatchPair, BlockCandidateBudgetConfig,
            BlockCandidateGenerationRequest, BlockCandidateOperator, BlockCandidateRecord,
            NgramTopKBlockOperator, RareTokenOverlapBlockOperator, generate_block_candidates,
        },
        index::ngram_index::{EntityNgramBuildConfig, EntityNgramIndex, EntityNgramSurface},
        postings::{EntityPostingBuildConfig, EntityPostingIndex, EntityPostingSurface},
    },
    namekit::ngram::NgramConfig,
};
use std::collections::BTreeSet;

#[test]
fn entity_block_candidates_generate_and_coalesce_operator_hits() {
    let (posting_index, ngram_index) = candidate_indexes();
    let result = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: Some(&ngram_index),
        budget_config: generous_budget(),
        operators: candidate_operators(),
    })
    .expect("block candidates generate");

    assert_eq!(
        result.diagnostics.candidate_record_count,
        result.candidates.len() as u64
    );
    assert!(result.diagnostics.candidate_budget.validated);
    assert!(
        result
            .candidates
            .iter()
            .all(|candidate| candidate.version == "canon_entity_block.v0")
    );
    assert_candidates_reference_index_surfaces(&result.candidates, &posting_index);

    let sears_pair = candidate_pair(&result.candidates, "surf:001", "surf:002");
    assert_eq!(
        sears_pair
            .block_hits
            .iter()
            .map(|hit| hit.operator_id.as_str())
            .collect::<Vec<_>>(),
        ["ngram_topk:tenant_core", "rare_token_overlap:tenant_tokens"]
    );
    assert_eq!(
        sears_pair
            .block_hits
            .iter()
            .map(|hit| hit.operator_id.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        sears_pair.block_hits.len(),
        "duplicate pair emissions coalesce by operator"
    );

    let alias_pair = candidate_pair(&result.candidates, "surf:003", "surf:004");
    assert_eq!(alias_pair.block_hits[0].operator_id, "alias_patch_match");
    assert_eq!(alias_pair.block_hits[0].rank, None);
}

#[test]
fn deterministic_topk_order_survives_operator_order_changes() {
    let (posting_index, ngram_index) = candidate_indexes();
    let first = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: Some(&ngram_index),
        budget_config: generous_budget(),
        operators: candidate_operators(),
    })
    .expect("first run");
    let mut reversed_operators = candidate_operators();
    reversed_operators.reverse();
    let second = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: Some(&ngram_index),
        budget_config: generous_budget(),
        operators: reversed_operators,
    })
    .expect("second run");

    assert_eq!(first.candidates, second.candidates);
    assert_eq!(
        serde_json::to_vec(&first.candidates).expect("serializes"),
        serde_json::to_vec(&second.candidates).expect("serializes")
    );
    assert!(
        first
            .candidates
            .windows(2)
            .all(|pair| pair[0].candidate_score_hint >= pair[1].candidate_score_hint)
    );
}

#[test]
#[allow(non_snake_case)]
fn EN_B002_common_token_bucket_is_bounded_before_output() {
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
    .expect("common-token block stays bounded");

    assert!(result.candidates.is_empty());
    assert_eq!(result.diagnostics.candidate_record_count, 0);
    assert_eq!(result.diagnostics.candidate_pairs_emitted, 0);
    assert_eq!(result.diagnostics.operator_diagnostics.len(), 1);
    assert_eq!(
        result.diagnostics.operator_diagnostics[0].large_posting_suppressed_count,
        64
    );
    assert_eq!(
        result.diagnostics.operator_diagnostics[0].emitted_candidate_count,
        0
    );
}

#[test]
fn alias_patch_candidates_refuse_unknown_surface_ids() {
    let (posting_index, ngram_index) = candidate_indexes();
    let refusal = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: Some(&ngram_index),
        budget_config: generous_budget(),
        operators: vec![BlockCandidateOperator::AliasPatchMatch(
            AliasPatchMatchBlockOperator::new(
                "alias_patch_match",
                vec![AliasPatchPair::new("surf:003", "surf:missing", "patch:bad")],
            ),
        )],
    })
    .expect_err("unknown patch surface refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "block");
    assert_eq!(refusal.detail["reason"], "unknown_surface_id");
}

fn candidate_operators() -> Vec<BlockCandidateOperator> {
    vec![
        BlockCandidateOperator::NgramTopK(NgramTopKBlockOperator::new(
            "ngram_topk:tenant_core",
            2,
            2,
        )),
        BlockCandidateOperator::RareTokenOverlap(
            RareTokenOverlapBlockOperator::new("rare_token_overlap:tenant_tokens", "tenant_core")
                .with_min_idf_units(1_500)
                .with_topk(2, 2)
                .with_max_posting_size(10),
        ),
        BlockCandidateOperator::AliasPatchMatch(AliasPatchMatchBlockOperator::new(
            "alias_patch_match",
            vec![AliasPatchPair::new(
                "surf:004",
                "surf:003",
                "patch:kmart-sears-auto-review",
            )],
        )),
    ]
}

fn candidate_indexes() -> (EntityPostingIndex, EntityNgramIndex) {
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

fn generous_budget() -> BlockCandidateBudgetConfig {
    BlockCandidateBudgetConfig::new(8, 64, 128)
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

fn assert_candidates_reference_index_surfaces(
    candidates: &[BlockCandidateRecord],
    posting_index: &EntityPostingIndex,
) {
    let surface_ids = posting_index
        .surface_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for candidate in candidates {
        assert!(candidate.left_surface_id < candidate.right_surface_id);
        assert!(surface_ids.contains(candidate.left_surface_id.as_str()));
        assert!(surface_ids.contains(candidate.right_surface_id.as_str()));
    }
}
