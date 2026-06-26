#![forbid(unsafe_code)]

use crate::{
    Refusal,
    entity::{
        CANON_ENTITY_BLOCK_BUCKET_VERSION, CANON_ENTITY_BLOCK_VERSION,
        block_artifact::{
            CannotLinkAction, CannotLinkValidationHook, CannotLinkValidationStatus,
            EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN, ExactBucketAssertion, ExactBucketContractError,
            ExactBucketDiagnostics, ExactBucketMembership, ExactBucketProfile, ExactBucketUpstream,
        },
        budget::{BudgetBreach, BudgetLimit, BudgetStage, find_budget_policy},
        edge::EdgeCandidateBudgetProof,
        error::EntityRefusalKind,
        index::ngram_index::{EntityNgramIndex, EntityNgramIndexError},
        postings::{EntityPostingIndex, PostingLayoutError},
        topk::{TopKCandidateInput, TopKConfig, prune_top_k_candidates},
    },
    namekit::tfidf::RARE_TOKEN_MIN_IDF_UNITS,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub const BLOCK_STAGE: &str = "block";
pub const BLOCK_CANDIDATE_ARTIFACT: &str = "candidate_artifact";
pub const BLOCK_PARTIAL_CANDIDATE_ARTIFACT_WRITTEN_ON_REFUSAL: bool = false;
const ALIAS_PATCH_SCORE_UNITS: u32 = 1_000_000;

#[derive(Debug, Clone)]
pub struct BlockCandidateGenerationRequest<'a> {
    pub profile_id: String,
    pub posting_index: &'a EntityPostingIndex,
    pub ngram_index: Option<&'a EntityNgramIndex>,
    pub budget_config: BlockCandidateBudgetConfig,
    pub operators: Vec<BlockCandidateOperator>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockCandidateOperator {
    NgramTopK(NgramTopKBlockOperator),
    RareTokenOverlap(RareTokenOverlapBlockOperator),
    AliasPatchMatch(AliasPatchMatchBlockOperator),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NgramTopKBlockOperator {
    pub operator_id: String,
    pub k: usize,
    pub candidate_cap: usize,
    pub score_floor_units: Option<u32>,
}

impl NgramTopKBlockOperator {
    pub fn new(operator_id: impl Into<String>, k: usize, candidate_cap: usize) -> Self {
        Self {
            operator_id: operator_id.into(),
            k,
            candidate_cap,
            score_floor_units: None,
        }
    }

    pub const fn with_score_floor_units(mut self, score_floor_units: u32) -> Self {
        self.score_floor_units = Some(score_floor_units);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RareTokenOverlapBlockOperator {
    pub operator_id: String,
    pub view_name: String,
    pub min_tokens: u32,
    pub min_idf_units: u32,
    pub k: usize,
    pub candidate_cap: usize,
    pub max_posting_size: usize,
}

impl RareTokenOverlapBlockOperator {
    pub fn new(operator_id: impl Into<String>, view_name: impl Into<String>) -> Self {
        Self {
            operator_id: operator_id.into(),
            view_name: view_name.into(),
            min_tokens: 1,
            min_idf_units: RARE_TOKEN_MIN_IDF_UNITS,
            k: 25,
            candidate_cap: 25,
            max_posting_size: 100,
        }
    }

    pub const fn with_min_tokens(mut self, min_tokens: u32) -> Self {
        self.min_tokens = min_tokens;
        self
    }

    pub const fn with_min_idf_units(mut self, min_idf_units: u32) -> Self {
        self.min_idf_units = min_idf_units;
        self
    }

    pub const fn with_topk(mut self, k: usize, candidate_cap: usize) -> Self {
        self.k = k;
        self.candidate_cap = candidate_cap;
        self
    }

    pub const fn with_max_posting_size(mut self, max_posting_size: usize) -> Self {
        self.max_posting_size = max_posting_size;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasPatchMatchBlockOperator {
    pub operator_id: String,
    pub pairs: Vec<AliasPatchPair>,
}

impl AliasPatchMatchBlockOperator {
    pub fn new(operator_id: impl Into<String>, pairs: Vec<AliasPatchPair>) -> Self {
        Self {
            operator_id: operator_id.into(),
            pairs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasPatchPair {
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub patch_id: String,
}

impl AliasPatchPair {
    pub fn new(
        left_surface_id: impl Into<String>,
        right_surface_id: impl Into<String>,
        patch_id: impl Into<String>,
    ) -> Self {
        Self {
            left_surface_id: left_surface_id.into(),
            right_surface_id: right_surface_id.into(),
            patch_id: patch_id.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCandidateGenerationResult {
    pub candidates: Vec<BlockCandidateRecord>,
    pub diagnostics: BlockCandidateGenerationDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCandidateRecord {
    pub version: String,
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub block_hits: Vec<BlockCandidateHit>,
    pub candidate_score_hint: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCandidateHit {
    pub operator_id: String,
    pub rank: Option<usize>,
    pub score_units: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCandidateGenerationDiagnostics {
    pub candidate_record_count: u64,
    pub candidate_pairs_emitted: u64,
    pub candidate_pairs_suppressed_by_cap: u64,
    pub suppressed_candidate_count: u64,
    pub large_buckets_suppressed: u64,
    pub candidate_pairs_per_surface_p50: u64,
    pub candidate_pairs_per_surface_p95: u64,
    pub candidate_pairs_per_surface_p99: u64,
    pub max_candidates_for_surface: u64,
    pub max_candidates_for_operator: u64,
    pub configured_budget: BlockCandidateBudgetConfig,
    pub candidate_budget: EdgeCandidateBudgetProof,
    pub candidate_artifact_bytes: u64,
    pub partial_candidate_artifact_written: bool,
    pub operator_yield: Vec<BlockOperatorYield>,
    pub operator_diagnostics: Vec<BlockOperatorCandidateDiagnostics>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockOperatorCandidateDiagnostics {
    pub operator_id: String,
    pub input_candidate_count: u64,
    pub eligible_candidate_count: u64,
    pub emitted_candidate_count: u64,
    pub suppressed_candidate_count: u64,
    pub large_posting_suppressed_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockOperatorYield {
    pub operator_id: String,
    pub emitted_candidate_count: u64,
    pub suppressed_candidate_count: u64,
    pub large_posting_suppressed_count: u64,
}

pub fn generate_block_candidates(
    request: BlockCandidateGenerationRequest<'_>,
) -> Result<BlockCandidateGenerationResult, Refusal> {
    validate_candidate_index_surface_sets(request.posting_index, request.ngram_index)?;

    let mut accumulator = BlockCandidateAccumulator::default();
    let mut budget_observations = Vec::new();
    let mut operator_diagnostics = Vec::new();

    for operator in &request.operators {
        match operator {
            BlockCandidateOperator::NgramTopK(config) => {
                let Some(ngram_index) = request.ngram_index else {
                    return Err(candidate_artifact_contract_refusal(
                        "N-gram top-k block operator requires a matching n-gram index",
                        json!({
                            "stage": BLOCK_STAGE,
                            "operator_id": &config.operator_id,
                            "reason": "missing_ngram_index",
                            "writes_performed": false
                        }),
                    ));
                };
                let diagnostic = apply_ngram_topk_operator(
                    &request.profile_id,
                    ngram_index,
                    config,
                    &mut accumulator,
                    &mut budget_observations,
                )?;
                operator_diagnostics.push(diagnostic);
            }
            BlockCandidateOperator::RareTokenOverlap(config) => {
                let diagnostic = apply_rare_token_overlap_operator(
                    &request.profile_id,
                    request.posting_index,
                    config,
                    &mut accumulator,
                    &mut budget_observations,
                )?;
                operator_diagnostics.push(diagnostic);
            }
            BlockCandidateOperator::AliasPatchMatch(config) => {
                let diagnostic = apply_alias_patch_operator(
                    request.posting_index,
                    config,
                    &mut accumulator,
                    &mut budget_observations,
                )?;
                operator_diagnostics.push(diagnostic);
            }
        }
    }

    let budget = validate_block_candidate_budget_before_artifact_emission(
        &request.budget_config,
        &budget_observations,
    )?;
    operator_diagnostics.sort_by(block_operator_diagnostic_cmp);
    let candidates = accumulator.into_records();
    let candidate_artifact_bytes = candidate_artifact_byte_count(&candidates)?;
    let large_buckets_suppressed = operator_diagnostics
        .iter()
        .map(|diagnostic| diagnostic.large_posting_suppressed_count)
        .sum();
    let operator_yield = operator_yield_from_diagnostics(&operator_diagnostics);
    Ok(BlockCandidateGenerationResult {
        diagnostics: BlockCandidateGenerationDiagnostics {
            candidate_record_count: candidates.len() as u64,
            candidate_pairs_emitted: budget.candidate_pairs_emitted,
            candidate_pairs_suppressed_by_cap: budget.candidate_pairs_suppressed_by_cap,
            suppressed_candidate_count: budget.suppressed_candidate_count,
            large_buckets_suppressed,
            candidate_pairs_per_surface_p50: budget.candidate_pairs_per_surface_p50,
            candidate_pairs_per_surface_p95: budget.candidate_pairs_per_surface_p95,
            candidate_pairs_per_surface_p99: budget.candidate_pairs_per_surface_p99,
            max_candidates_for_surface: budget.max_candidates_for_surface,
            max_candidates_for_operator: budget.max_candidates_for_operator,
            configured_budget: request.budget_config.clone(),
            candidate_budget: budget.candidate_budget,
            candidate_artifact_bytes,
            partial_candidate_artifact_written: budget.partial_candidate_artifact_written,
            operator_yield,
            operator_diagnostics,
        },
        candidates,
    })
}

pub fn validate_block_exact_bucket_size_limit(
    operator_id: impl Into<String>,
    bucket_id: impl Into<String>,
    row_count: u64,
    configured_limit: u64,
) -> Result<(), Refusal> {
    if row_count <= configured_limit {
        return Ok(());
    }

    let policy = find_budget_policy(BudgetStage::Block, BudgetLimit::MaxExactBucketSize)
        .expect("block exact bucket size policy is defined");
    let breach = policy.breach(row_count, configured_limit);
    Err(EntityRefusalKind::IndexLimit.to_refusal(
        "Exact bucket exceeds configured block size limit",
        json!({
            "stage": BLOCK_STAGE,
            "artifact": BLOCK_CANDIDATE_ARTIFACT,
            "reason": "exact_bucket_size_exceeded",
            "operator_id": operator_id.into(),
            "bucket_id": bucket_id.into(),
            "policy_id": breach.policy_id,
            "observed": breach.observed,
            "configured": breach.configured,
            "budget": breach,
            "pair_expansion": EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN,
            "candidate_artifact_written": false,
            "partial_candidate_artifact_written": false
        }),
        Some(policy.next_command.to_string()),
    ))
}

fn candidate_artifact_byte_count(candidates: &[BlockCandidateRecord]) -> Result<u64, Refusal> {
    let bytes = serde_json::to_vec(candidates).map_err(|error| {
        candidate_artifact_contract_refusal(
            "Failed to measure block candidate artifact bytes",
            json!({
                "stage": BLOCK_STAGE,
                "reason": "candidate_artifact_serialization_failed",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    Ok(bytes.len() as u64)
}

fn operator_yield_from_diagnostics(
    diagnostics: &[BlockOperatorCandidateDiagnostics],
) -> Vec<BlockOperatorYield> {
    diagnostics
        .iter()
        .map(|diagnostic| BlockOperatorYield {
            operator_id: diagnostic.operator_id.clone(),
            emitted_candidate_count: diagnostic.emitted_candidate_count,
            suppressed_candidate_count: diagnostic.suppressed_candidate_count,
            large_posting_suppressed_count: diagnostic.large_posting_suppressed_count,
        })
        .collect()
}

fn validate_candidate_index_surface_sets(
    posting_index: &EntityPostingIndex,
    ngram_index: Option<&EntityNgramIndex>,
) -> Result<(), Refusal> {
    let Some(ngram_index) = ngram_index else {
        return Ok(());
    };
    let posting_surface_ids = posting_index
        .surface_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let ngram_surface_ids = ngram_index
        .surface_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if posting_surface_ids == ngram_surface_ids {
        return Ok(());
    }

    Err(candidate_artifact_contract_refusal(
        "Entity block indexes do not describe the same prepared surfaces",
        json!({
            "stage": BLOCK_STAGE,
            "reason": "surface_set_mismatch",
            "posting_surface_count": posting_index.surface_ids.len(),
            "ngram_surface_count": ngram_index.surface_ids.len(),
            "writes_performed": false
        }),
    ))
}

fn apply_ngram_topk_operator(
    profile_id: &str,
    ngram_index: &EntityNgramIndex,
    config: &NgramTopKBlockOperator,
    accumulator: &mut BlockCandidateAccumulator,
    budget_observations: &mut Vec<BlockCandidateBudgetObservation>,
) -> Result<BlockOperatorCandidateDiagnostics, Refusal> {
    let mut diagnostic = OperatorDiagnosticAccumulator::new(&config.operator_id);

    for surface_id in &ngram_index.surface_ids {
        let mut topk_config =
            TopKConfig::new(profile_id.to_string(), config.operator_id.clone(), config.k)
                .with_candidate_cap(config.candidate_cap);
        if let Some(score_floor_units) = config.score_floor_units {
            topk_config = topk_config.with_score_floor_units(score_floor_units);
        }
        let result = ngram_index
            .top_k_for_surface(surface_id, topk_config)
            .map_err(ngram_index_refusal)?;
        diagnostic.record_topk_counts(
            result.diagnostics.input_candidate_count,
            result.diagnostics.eligible_candidate_count,
            result.diagnostics.emitted_candidate_count,
            result.diagnostics.dropped_candidate_count,
        );
        budget_observations.push(BlockCandidateBudgetObservation::new(
            surface_id,
            &config.operator_id,
            usize_to_u64(result.diagnostics.emitted_candidate_count),
            usize_to_u64(result.diagnostics.dropped_candidate_count),
        ));

        for candidate in result.candidates {
            accumulator.add_hit(
                &candidate.query_surface_id,
                &candidate.candidate_surface_id,
                BlockCandidateHit {
                    operator_id: config.operator_id.clone(),
                    rank: Some(candidate.rank),
                    score_units: candidate.score_units,
                },
            );
        }
    }

    Ok(diagnostic.finish())
}

fn apply_rare_token_overlap_operator(
    profile_id: &str,
    posting_index: &EntityPostingIndex,
    config: &RareTokenOverlapBlockOperator,
    accumulator: &mut BlockCandidateAccumulator,
    budget_observations: &mut Vec<BlockCandidateBudgetObservation>,
) -> Result<BlockOperatorCandidateDiagnostics, Refusal> {
    let token_features =
        token_features_by_surface(posting_index).map_err(posting_layout_refusal)?;
    let surface_keys = surface_keys_for_exact_view(posting_index, &config.view_name)
        .map_err(posting_layout_refusal)?;
    let mut diagnostic = OperatorDiagnosticAccumulator::new(&config.operator_id);

    for (query_ordinal, query_surface_id) in posting_index.surface_ids.iter().enumerate() {
        let mut candidates = BTreeMap::<usize, RareTokenCandidateAccumulator>::new();
        let mut large_posting_suppressed_count = 0_u64;

        for feature in &token_features[query_ordinal] {
            if feature.idf_units < config.min_idf_units {
                continue;
            }
            let postings = posting_index
                .tfidf_postings(&feature.token)
                .map_err(posting_layout_refusal)?;
            if config.max_posting_size > 0 && postings.len() > config.max_posting_size {
                large_posting_suppressed_count = large_posting_suppressed_count.saturating_add(1);
                continue;
            }
            for posting in postings {
                let candidate_ordinal = posting.surface_ordinal as usize;
                if candidate_ordinal == query_ordinal {
                    continue;
                }
                let entry = candidates.entry(candidate_ordinal).or_default();
                entry.shared_token_count = entry.shared_token_count.saturating_add(1);
                entry.score_units = entry
                    .score_units
                    .saturating_add(u128::from(feature.idf_units));
            }
        }

        let topk_inputs = candidates
            .into_iter()
            .filter(|(_, candidate)| candidate.shared_token_count >= config.min_tokens)
            .map(|(candidate_ordinal, candidate)| {
                TopKCandidateInput::new(
                    query_surface_id.clone(),
                    posting_index.surface_ids[candidate_ordinal].clone(),
                    surface_keys[candidate_ordinal].clone(),
                    u128_to_u32_saturating(candidate.score_units),
                )
            })
            .collect::<Vec<_>>();
        let result = prune_top_k_candidates(
            TopKConfig::new(profile_id.to_string(), config.operator_id.clone(), config.k)
                .with_candidate_cap(config.candidate_cap),
            topk_inputs,
        );

        diagnostic.record_topk_counts(
            result.diagnostics.input_candidate_count,
            result.diagnostics.eligible_candidate_count,
            result.diagnostics.emitted_candidate_count,
            result.diagnostics.dropped_candidate_count,
        );
        diagnostic.large_posting_suppressed_count = diagnostic
            .large_posting_suppressed_count
            .saturating_add(large_posting_suppressed_count);
        budget_observations.push(BlockCandidateBudgetObservation::new(
            query_surface_id,
            &config.operator_id,
            usize_to_u64(result.diagnostics.emitted_candidate_count),
            usize_to_u64(result.diagnostics.dropped_candidate_count),
        ));

        for candidate in result.candidates {
            accumulator.add_hit(
                &candidate.query_surface_id,
                &candidate.candidate_surface_id,
                BlockCandidateHit {
                    operator_id: config.operator_id.clone(),
                    rank: Some(candidate.rank),
                    score_units: candidate.score_units,
                },
            );
        }
    }

    Ok(diagnostic.finish())
}

fn apply_alias_patch_operator(
    posting_index: &EntityPostingIndex,
    config: &AliasPatchMatchBlockOperator,
    accumulator: &mut BlockCandidateAccumulator,
    budget_observations: &mut Vec<BlockCandidateBudgetObservation>,
) -> Result<BlockOperatorCandidateDiagnostics, Refusal> {
    let surface_set = posting_index
        .surface_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut pairs = config.pairs.clone();
    pairs.sort_by(|left, right| {
        left.patch_id
            .cmp(&right.patch_id)
            .then_with(|| left.left_surface_id.cmp(&right.left_surface_id))
            .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
    });

    let mut emitted_pairs = BTreeSet::<(String, String)>::new();
    for pair in pairs {
        if !surface_set.contains(pair.left_surface_id.as_str())
            || !surface_set.contains(pair.right_surface_id.as_str())
        {
            return Err(candidate_artifact_contract_refusal(
                "Alias patch block candidate references an unknown prepared surface",
                json!({
                    "stage": BLOCK_STAGE,
                    "operator_id": &config.operator_id,
                    "patch_id": &pair.patch_id,
                    "left_surface_id": &pair.left_surface_id,
                    "right_surface_id": &pair.right_surface_id,
                    "reason": "unknown_surface_id",
                    "writes_performed": false
                }),
            ));
        }
        let Some((left_surface_id, right_surface_id)) =
            ordered_surface_pair(&pair.left_surface_id, &pair.right_surface_id)
        else {
            continue;
        };
        if emitted_pairs.insert((left_surface_id.clone(), right_surface_id.clone())) {
            accumulator.add_hit(
                &left_surface_id,
                &right_surface_id,
                BlockCandidateHit {
                    operator_id: config.operator_id.clone(),
                    rank: None,
                    score_units: ALIAS_PATCH_SCORE_UNITS,
                },
            );
        }
    }

    for (left_surface_id, _) in &emitted_pairs {
        budget_observations.push(BlockCandidateBudgetObservation::new(
            left_surface_id,
            &config.operator_id,
            1,
            0,
        ));
    }

    Ok(BlockOperatorCandidateDiagnostics {
        operator_id: config.operator_id.clone(),
        input_candidate_count: config.pairs.len() as u64,
        eligible_candidate_count: emitted_pairs.len() as u64,
        emitted_candidate_count: emitted_pairs.len() as u64,
        suppressed_candidate_count: config.pairs.len().saturating_sub(emitted_pairs.len()) as u64,
        large_posting_suppressed_count: 0,
    })
}

fn candidate_artifact_contract_refusal(
    message: impl Into<String>,
    detail: serde_json::Value,
) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        detail,
        Some("Use matching prepared/index artifacts or rerun canon entity block".to_string()),
    )
}

fn ngram_index_refusal(error: EntityNgramIndexError) -> Refusal {
    candidate_artifact_contract_refusal(
        "N-gram index could not produce block candidates",
        json!({
            "stage": BLOCK_STAGE,
            "reason": "ngram_index_error",
            "error": format!("{error:?}"),
            "writes_performed": false
        }),
    )
}

fn posting_layout_refusal(error: PostingLayoutError) -> Refusal {
    candidate_artifact_contract_refusal(
        "Posting index could not produce block candidates",
        json!({
            "stage": BLOCK_STAGE,
            "reason": "posting_layout_error",
            "error": format!("{error:?}"),
            "writes_performed": false
        }),
    )
}

fn token_features_by_surface(
    posting_index: &EntityPostingIndex,
) -> Result<Vec<Vec<TokenFeature>>, PostingLayoutError> {
    let mut by_surface = vec![Vec::<TokenFeature>::new(); posting_index.surface_ids.len()];
    for idf in &posting_index.token_idf {
        for posting in posting_index.tfidf_postings(&idf.key)? {
            let surface_ordinal = posting.surface_ordinal as usize;
            if let Some(features) = by_surface.get_mut(surface_ordinal) {
                features.push(TokenFeature {
                    token: idf.key.clone(),
                    idf_units: idf.idf_units,
                });
            }
        }
    }
    for features in &mut by_surface {
        features.sort_by(|left, right| left.token.cmp(&right.token));
    }
    Ok(by_surface)
}

fn surface_keys_for_exact_view(
    posting_index: &EntityPostingIndex,
    view_name: &str,
) -> Result<Vec<String>, PostingLayoutError> {
    let mut keys = vec![String::new(); posting_index.surface_ids.len()];
    for bucket in posting_index.exact_view_buckets()? {
        if bucket.view_name != view_name {
            continue;
        }
        for surface_ordinal in bucket.surface_ordinals {
            let surface_ordinal = surface_ordinal as usize;
            if let Some(key) = keys.get_mut(surface_ordinal)
                && (key.is_empty() || bucket.value < *key)
            {
                *key = bucket.value.clone();
            }
        }
    }
    for (index, key) in keys.iter_mut().enumerate() {
        if key.is_empty() {
            *key = posting_index.surface_ids[index].clone();
        }
    }
    Ok(keys)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TokenFeature {
    token: String,
    idf_units: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RareTokenCandidateAccumulator {
    shared_token_count: u32,
    score_units: u128,
}

#[derive(Debug, Clone, Default)]
struct BlockCandidateAccumulator {
    pairs: BTreeMap<(String, String), BTreeMap<String, BlockCandidateHit>>,
}

impl BlockCandidateAccumulator {
    fn add_hit(&mut self, left: &str, right: &str, hit: BlockCandidateHit) {
        let Some(pair) = ordered_surface_pair(left, right) else {
            return;
        };
        let hits = self.pairs.entry(pair).or_default();
        hits.entry(hit.operator_id.clone())
            .and_modify(|current| {
                if better_hit(&hit, current) {
                    *current = hit.clone();
                }
            })
            .or_insert(hit);
    }

    fn into_records(self) -> Vec<BlockCandidateRecord> {
        let mut records = self
            .pairs
            .into_iter()
            .map(|((left_surface_id, right_surface_id), hits)| {
                let block_hits = hits.into_values().collect::<Vec<_>>();
                let candidate_score_hint = block_hits
                    .iter()
                    .map(|hit| hit.score_units)
                    .max()
                    .unwrap_or_default();
                BlockCandidateRecord {
                    version: CANON_ENTITY_BLOCK_VERSION.to_string(),
                    left_surface_id,
                    right_surface_id,
                    block_hits,
                    candidate_score_hint,
                }
            })
            .collect::<Vec<_>>();
        records.sort_by(block_candidate_record_cmp);
        records
    }
}

#[derive(Debug, Clone)]
struct OperatorDiagnosticAccumulator {
    operator_id: String,
    input_candidate_count: u64,
    eligible_candidate_count: u64,
    emitted_candidate_count: u64,
    suppressed_candidate_count: u64,
    large_posting_suppressed_count: u64,
}

impl OperatorDiagnosticAccumulator {
    fn new(operator_id: &str) -> Self {
        Self {
            operator_id: operator_id.to_string(),
            input_candidate_count: 0,
            eligible_candidate_count: 0,
            emitted_candidate_count: 0,
            suppressed_candidate_count: 0,
            large_posting_suppressed_count: 0,
        }
    }

    fn record_topk_counts(
        &mut self,
        input_candidate_count: usize,
        eligible_candidate_count: usize,
        emitted_candidate_count: usize,
        suppressed_candidate_count: usize,
    ) {
        self.input_candidate_count = self
            .input_candidate_count
            .saturating_add(usize_to_u64(input_candidate_count));
        self.eligible_candidate_count = self
            .eligible_candidate_count
            .saturating_add(usize_to_u64(eligible_candidate_count));
        self.emitted_candidate_count = self
            .emitted_candidate_count
            .saturating_add(usize_to_u64(emitted_candidate_count));
        self.suppressed_candidate_count = self
            .suppressed_candidate_count
            .saturating_add(usize_to_u64(suppressed_candidate_count));
    }

    fn finish(self) -> BlockOperatorCandidateDiagnostics {
        BlockOperatorCandidateDiagnostics {
            operator_id: self.operator_id,
            input_candidate_count: self.input_candidate_count,
            eligible_candidate_count: self.eligible_candidate_count,
            emitted_candidate_count: self.emitted_candidate_count,
            suppressed_candidate_count: self.suppressed_candidate_count,
            large_posting_suppressed_count: self.large_posting_suppressed_count,
        }
    }
}

fn better_hit(candidate: &BlockCandidateHit, current: &BlockCandidateHit) -> bool {
    candidate
        .score_units
        .cmp(&current.score_units)
        .then_with(|| {
            current
                .rank
                .unwrap_or(usize::MAX)
                .cmp(&candidate.rank.unwrap_or(usize::MAX))
        })
        .is_gt()
}

fn ordered_surface_pair(left: &str, right: &str) -> Option<(String, String)> {
    if left == right {
        return None;
    }
    if left < right {
        Some((left.to_string(), right.to_string()))
    } else {
        Some((right.to_string(), left.to_string()))
    }
}

fn block_candidate_record_cmp(
    left: &BlockCandidateRecord,
    right: &BlockCandidateRecord,
) -> std::cmp::Ordering {
    right
        .candidate_score_hint
        .cmp(&left.candidate_score_hint)
        .then_with(|| primary_operator_id(left).cmp(primary_operator_id(right)))
        .then_with(|| left.left_surface_id.cmp(&right.left_surface_id))
        .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
}

fn block_operator_diagnostic_cmp(
    left: &BlockOperatorCandidateDiagnostics,
    right: &BlockOperatorCandidateDiagnostics,
) -> std::cmp::Ordering {
    right
        .emitted_candidate_count
        .cmp(&left.emitted_candidate_count)
        .then_with(|| {
            right
                .suppressed_candidate_count
                .cmp(&left.suppressed_candidate_count)
        })
        .then_with(|| {
            right
                .large_posting_suppressed_count
                .cmp(&left.large_posting_suppressed_count)
        })
        .then_with(|| left.operator_id.cmp(&right.operator_id))
}

fn primary_operator_id(record: &BlockCandidateRecord) -> &str {
    record
        .block_hits
        .first()
        .map_or("", |hit| hit.operator_id.as_str())
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn u128_to_u32_saturating(value: u128) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCandidateBudgetConfig {
    pub max_candidates_per_surface: u64,
    pub max_candidates_per_operator: u64,
    pub max_candidates_per_run: u64,
}

impl BlockCandidateBudgetConfig {
    pub const fn new(
        max_candidates_per_surface: u64,
        max_candidates_per_operator: u64,
        max_candidates_per_run: u64,
    ) -> Self {
        Self {
            max_candidates_per_surface,
            max_candidates_per_operator,
            max_candidates_per_run,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCandidateBudgetObservation {
    pub surface_id: String,
    pub operator_id: String,
    pub emitted_candidate_count: u64,
    pub suppressed_candidate_count: u64,
}

impl BlockCandidateBudgetObservation {
    pub fn new(
        surface_id: impl Into<String>,
        operator_id: impl Into<String>,
        emitted_candidate_count: u64,
        suppressed_candidate_count: u64,
    ) -> Self {
        Self {
            surface_id: surface_id.into(),
            operator_id: operator_id.into(),
            emitted_candidate_count,
            suppressed_candidate_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BlockCandidateBudgetDiagnostics {
    pub candidate_pairs_emitted: u64,
    pub candidate_pairs_suppressed_by_cap: u64,
    pub suppressed_candidate_count: u64,
    pub candidate_pairs_per_surface_p50: u64,
    pub candidate_pairs_per_surface_p95: u64,
    pub candidate_pairs_per_surface_p99: u64,
    pub max_candidates_for_surface: u64,
    pub max_candidates_for_operator: u64,
    pub candidate_budget: EdgeCandidateBudgetProof,
    pub partial_candidate_artifact_written: bool,
}

pub fn validate_block_candidate_budget_before_artifact_emission(
    config: &BlockCandidateBudgetConfig,
    observations: &[BlockCandidateBudgetObservation],
) -> Result<BlockCandidateBudgetDiagnostics, Refusal> {
    let summary = summarize_candidate_budget(config, observations);

    if let Some(breach) = first_candidate_budget_breach(
        config,
        &summary.surface_totals,
        &summary.operator_totals,
        summary.diagnostics.candidate_pairs_emitted,
    ) {
        return Err(block_candidate_budget_refusal(
            config,
            &breach,
            &summary.diagnostics,
        ));
    }

    Ok(summary.diagnostics)
}

fn summarize_candidate_budget(
    config: &BlockCandidateBudgetConfig,
    observations: &[BlockCandidateBudgetObservation],
) -> BlockCandidateBudgetSummary {
    let mut surface_totals = BTreeMap::<String, u64>::new();
    let mut operator_totals = BTreeMap::<String, u64>::new();
    let mut candidate_pairs_emitted = 0_u64;
    let mut suppressed_candidate_count = 0_u64;

    for observation in observations {
        candidate_pairs_emitted =
            candidate_pairs_emitted.saturating_add(observation.emitted_candidate_count);
        suppressed_candidate_count =
            suppressed_candidate_count.saturating_add(observation.suppressed_candidate_count);
        let surface_total = surface_totals
            .entry(observation.surface_id.clone())
            .or_default();
        *surface_total = (*surface_total).saturating_add(observation.emitted_candidate_count);
        let operator_total = operator_totals
            .entry(observation.operator_id.clone())
            .or_default();
        *operator_total = (*operator_total).saturating_add(observation.emitted_candidate_count);
    }

    let mut per_surface_counts = surface_totals.values().copied().collect::<Vec<_>>();
    per_surface_counts.sort_unstable();

    BlockCandidateBudgetSummary {
        diagnostics: BlockCandidateBudgetDiagnostics {
            candidate_pairs_emitted,
            candidate_pairs_suppressed_by_cap: suppressed_candidate_count,
            suppressed_candidate_count,
            candidate_pairs_per_surface_p50: nearest_rank_percentile(&per_surface_counts, 50),
            candidate_pairs_per_surface_p95: nearest_rank_percentile(&per_surface_counts, 95),
            candidate_pairs_per_surface_p99: nearest_rank_percentile(&per_surface_counts, 99),
            max_candidates_for_surface: surface_totals.values().copied().max().unwrap_or(0),
            max_candidates_for_operator: operator_totals.values().copied().max().unwrap_or(0),
            candidate_budget: EdgeCandidateBudgetProof::within_run_budget(
                candidate_pairs_emitted,
                config.max_candidates_per_run,
            ),
            partial_candidate_artifact_written: false,
        },
        surface_totals,
        operator_totals,
    }
}

fn first_candidate_budget_breach(
    config: &BlockCandidateBudgetConfig,
    surface_totals: &BTreeMap<String, u64>,
    operator_totals: &BTreeMap<String, u64>,
    candidate_pairs_emitted: u64,
) -> Option<BlockCandidateBudgetBreach> {
    if let Some((surface_id, observed)) =
        largest_over_limit(surface_totals, config.max_candidates_per_surface)
    {
        return Some(BlockCandidateBudgetBreach::new(
            BudgetLimit::MaxCandidatesPerSurface,
            observed,
            config.max_candidates_per_surface,
            "surface",
            Some(surface_id),
        ));
    }

    if let Some((operator_id, observed)) =
        largest_over_limit(operator_totals, config.max_candidates_per_operator)
    {
        return Some(BlockCandidateBudgetBreach::new(
            BudgetLimit::MaxCandidatesPerOperator,
            observed,
            config.max_candidates_per_operator,
            "operator",
            Some(operator_id),
        ));
    }

    (candidate_pairs_emitted > config.max_candidates_per_run).then(|| {
        BlockCandidateBudgetBreach::new(
            BudgetLimit::MaxCandidatesPerRun,
            candidate_pairs_emitted,
            config.max_candidates_per_run,
            "run",
            None,
        )
    })
}

fn largest_over_limit(counts: &BTreeMap<String, u64>, limit: u64) -> Option<(String, u64)> {
    let mut best = None::<(&String, u64)>;
    for (id, count) in counts {
        if *count <= limit {
            continue;
        }
        let replace = best.is_none_or(|(best_id, best_count)| {
            *count > best_count || (*count == best_count && id < best_id)
        });
        if replace {
            best = Some((id, *count));
        }
    }
    best.map(|(id, count)| (id.clone(), count))
}

fn nearest_rank_percentile(sorted_counts: &[u64], percentile: u64) -> u64 {
    if sorted_counts.is_empty() {
        return 0;
    }
    let rank = ((sorted_counts.len() as u64) * percentile)
        .div_ceil(100)
        .max(1);
    let index = (rank as usize)
        .saturating_sub(1)
        .min(sorted_counts.len() - 1);
    sorted_counts[index]
}

fn block_candidate_budget_refusal(
    config: &BlockCandidateBudgetConfig,
    breach: &BlockCandidateBudgetBreach,
    diagnostics: &BlockCandidateBudgetDiagnostics,
) -> Refusal {
    EntityRefusalKind::CandidateBudget.to_refusal(
        "Block candidate budget exceeded before candidate artifact emission",
        json!({
            "stage": BLOCK_STAGE,
            "artifact": BLOCK_CANDIDATE_ARTIFACT,
            "refusal_code": breach.budget.refusal_code.as_str(),
            "reason": "candidate_budget_exceeded",
            "policy_id": breach.budget.policy_id,
            "subject_kind": breach.subject_kind,
            "subject_id": breach.subject_id,
            "observed": breach.budget.observed,
            "configured": breach.budget.configured,
            "configured_limits": {
                "max_candidates_per_surface": config.max_candidates_per_surface,
                "max_candidates_per_operator": config.max_candidates_per_operator,
                "max_candidates_per_run": config.max_candidates_per_run
            },
            "observed_limits": {
                "candidate_pairs_emitted": diagnostics.candidate_pairs_emitted,
                "candidate_pairs_suppressed_by_cap": diagnostics.candidate_pairs_suppressed_by_cap,
                "suppressed_candidate_count": diagnostics.suppressed_candidate_count,
                "candidate_pairs_per_surface_p50": diagnostics.candidate_pairs_per_surface_p50,
                "candidate_pairs_per_surface_p95": diagnostics.candidate_pairs_per_surface_p95,
                "candidate_pairs_per_surface_p99": diagnostics.candidate_pairs_per_surface_p99,
                "max_candidates_for_surface": diagnostics.max_candidates_for_surface,
                "max_candidates_for_operator": diagnostics.max_candidates_for_operator
            },
            "budget": breach.budget,
            "candidate_pairs_emitted": diagnostics.candidate_pairs_emitted,
            "candidate_pairs_suppressed_by_cap": diagnostics.candidate_pairs_suppressed_by_cap,
            "suppressed_candidate_count": diagnostics.suppressed_candidate_count,
            "candidate_pairs_per_surface_p50": diagnostics.candidate_pairs_per_surface_p50,
            "candidate_pairs_per_surface_p95": diagnostics.candidate_pairs_per_surface_p95,
            "candidate_pairs_per_surface_p99": diagnostics.candidate_pairs_per_surface_p99,
            "candidate_artifact_written": BLOCK_PARTIAL_CANDIDATE_ARTIFACT_WRITTEN_ON_REFUSAL,
            "partial_candidate_artifact_written": BLOCK_PARTIAL_CANDIDATE_ARTIFACT_WRITTEN_ON_REFUSAL
        }),
        Some(breach.budget.next_command.to_string()),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlockCandidateBudgetSummary {
    diagnostics: BlockCandidateBudgetDiagnostics,
    surface_totals: BTreeMap<String, u64>,
    operator_totals: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlockCandidateBudgetBreach {
    budget: BudgetBreach,
    subject_kind: &'static str,
    subject_id: Option<String>,
}

impl BlockCandidateBudgetBreach {
    fn new(
        limit: BudgetLimit,
        observed: u64,
        configured: u64,
        subject_kind: &'static str,
        subject_id: Option<String>,
    ) -> Self {
        let policy = find_budget_policy(BudgetStage::Block, limit)
            .expect("block candidate budget policy is defined");
        Self {
            budget: policy.breach(observed, configured),
            subject_kind,
            subject_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactBucketBlockRequest {
    pub profile: ExactBucketProfile,
    pub upstream: ExactBucketUpstream,
    pub operator_id: String,
    pub identity_view: String,
    pub placeholder_values: BTreeSet<String>,
    pub surfaces: Vec<ExactBucketSurface>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactBucketSurface {
    pub surface_id: String,
    pub bucket_value: String,
    pub row_count: u64,
    pub deal_count: u64,
}

impl ExactBucketSurface {
    pub fn new(
        surface_id: impl Into<String>,
        bucket_value: impl Into<String>,
        row_count: u64,
        deal_count: u64,
    ) -> Self {
        Self {
            surface_id: surface_id.into(),
            bucket_value: bucket_value.into(),
            row_count,
            deal_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactBucketBlockResult {
    pub assertions: Vec<ExactBucketAssertion>,
    pub diagnostics: ExactBucketBlockDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExactBucketBlockDiagnostics {
    pub exact_bucket_count: u64,
    pub emitted_bucket_count: u64,
    pub excluded_placeholder_bucket_count: u64,
    pub expanded_pair_count: u64,
    pub suppressed_pair_count: u64,
    pub largest_bucket_size: u64,
    pub membership_record_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExactBucketEmissionError {
    Contract(ExactBucketContractError),
}

pub fn emit_exact_bucket_hyperedges(
    request: ExactBucketBlockRequest,
) -> Result<ExactBucketBlockResult, ExactBucketEmissionError> {
    let mut groups = BTreeMap::<String, ExactBucketGroup>::new();
    let mut excluded_placeholder_values = BTreeSet::<String>::new();

    for surface in request.surfaces {
        let bucket_value = surface.bucket_value.trim();
        if bucket_value.is_empty() {
            continue;
        }
        if request.placeholder_values.contains(bucket_value) {
            excluded_placeholder_values.insert(bucket_value.to_string());
            continue;
        }
        let group = groups.entry(bucket_value.to_string()).or_default();
        group.surface_ids.insert(surface.surface_id);
        group.row_count = group.row_count.saturating_add(surface.row_count);
        group.deal_count = group.deal_count.saturating_add(surface.deal_count);
    }

    let mut diagnostics = ExactBucketBlockDiagnostics {
        excluded_placeholder_bucket_count: excluded_placeholder_values.len() as u64,
        ..ExactBucketBlockDiagnostics::default()
    };
    let mut assertions = Vec::with_capacity(groups.len());

    for (bucket_value, group) in groups {
        let surface_ids = group.surface_ids.into_iter().collect::<Vec<_>>();
        let suppressed_pair_count = suppressed_pair_count(group.row_count);
        let assertion = ExactBucketAssertion {
            version: CANON_ENTITY_BLOCK_BUCKET_VERSION.to_string(),
            bucket_id: format!("bucket:{}:{bucket_value}", request.identity_view),
            operator_id: request.operator_id.clone(),
            profile: request.profile.clone(),
            upstream: request.upstream.clone(),
            membership: ExactBucketMembership {
                surface_ids,
                surface_ranges: Vec::new(),
            },
            row_count: group.row_count,
            deal_count: group.deal_count,
            pair_expansion: EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN.to_string(),
            diagnostics: ExactBucketDiagnostics {
                largest_bucket_size: group.row_count,
                suppressed_pair_count,
                labels: BTreeMap::from([
                    ("identity_view".to_string(), request.identity_view.clone()),
                    ("bucket_value".to_string(), bucket_value),
                ]),
            },
            cannot_link_validation: CannotLinkValidationHook {
                status: CannotLinkValidationStatus::NotChecked,
                checked_fact_count: 0,
                hard_cannot_link_count: 0,
                action: CannotLinkAction::RequireReview,
            },
        };
        assertion
            .validate()
            .map_err(ExactBucketEmissionError::Contract)?;

        diagnostics.exact_bucket_count += 1;
        diagnostics.emitted_bucket_count += 1;
        diagnostics.expanded_pair_count += assertion.expanded_pair_count();
        diagnostics.suppressed_pair_count = diagnostics
            .suppressed_pair_count
            .saturating_add(suppressed_pair_count);
        diagnostics.largest_bucket_size = diagnostics.largest_bucket_size.max(assertion.row_count);
        diagnostics.membership_record_count = diagnostics
            .membership_record_count
            .saturating_add(assertion.artifact_membership_record_count());
        assertions.push(assertion);
    }

    Ok(ExactBucketBlockResult {
        assertions,
        diagnostics,
    })
}

fn suppressed_pair_count(row_count: u64) -> u64 {
    row_count.saturating_mul(row_count.saturating_sub(1)) / 2
}

#[derive(Debug, Clone, Default)]
struct ExactBucketGroup {
    surface_ids: BTreeSet<String>,
    row_count: u64,
    deal_count: u64,
}
