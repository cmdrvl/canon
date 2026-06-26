#![forbid(unsafe_code)]

//! Deterministic block candidate generation over entity index accelerators.

use crate::{
    Refusal,
    entity::{
        CANON_ENTITY_BLOCK_VERSION,
        block::{
            BlockCandidateBudgetConfig, BlockCandidateBudgetDiagnostics,
            BlockCandidateBudgetObservation,
            validate_block_candidate_budget_before_artifact_emission,
        },
        index::ngram_index::{EntityNgramIndex, EntityNgramIndexError},
        postings::{EntityPostingIndex, PostingLayoutError, PostingRecord},
        topk::{TopKConfig, TopKPruneResult},
    },
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCandidateGenerationConfig {
    pub profile_id: String,
    pub operator_id: String,
    pub top_k: usize,
    pub candidate_cap_per_surface: usize,
    pub score_floor_units: Option<u32>,
    pub budget: BlockCandidateBudgetConfig,
}

impl BlockCandidateGenerationConfig {
    pub fn new(
        profile_id: impl Into<String>,
        operator_id: impl Into<String>,
        top_k: usize,
        candidate_cap_per_surface: usize,
        budget: BlockCandidateBudgetConfig,
    ) -> Self {
        Self {
            profile_id: profile_id.into(),
            operator_id: operator_id.into(),
            top_k,
            candidate_cap_per_surface,
            score_floor_units: None,
            budget,
        }
    }

    pub const fn with_score_floor_units(mut self, score_floor_units: u32) -> Self {
        self.score_floor_units = Some(score_floor_units);
        self
    }

    fn topk_config(&self) -> TopKConfig {
        let mut config = TopKConfig::new(
            self.profile_id.clone(),
            self.operator_id.clone(),
            self.top_k,
        )
        .with_candidate_cap(self.candidate_cap_per_surface);
        if let Some(score_floor_units) = self.score_floor_units {
            config = config.with_score_floor_units(score_floor_units);
        }
        config
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RareTokenOverlapConfig {
    pub operator_id: String,
    pub max_document_frequency: u32,
    pub max_posting_len: usize,
    pub budget: BlockCandidateBudgetConfig,
}

impl RareTokenOverlapConfig {
    pub fn new(
        operator_id: impl Into<String>,
        max_document_frequency: u32,
        max_posting_len: usize,
        budget: BlockCandidateBudgetConfig,
    ) -> Self {
        Self {
            operator_id: operator_id.into(),
            max_document_frequency,
            max_posting_len,
            budget,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasPatchMatchGroup {
    pub group_id: String,
    pub surface_ids: Vec<String>,
    pub score_units: u32,
}

impl AliasPatchMatchGroup {
    pub fn new(
        group_id: impl Into<String>,
        surface_ids: impl IntoIterator<Item = impl Into<String>>,
        score_units: u32,
    ) -> Self {
        Self {
            group_id: group_id.into(),
            surface_ids: surface_ids.into_iter().map(Into::into).collect(),
            score_units,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCandidateSet {
    pub candidates: Vec<BlockCandidate>,
    pub diagnostics: BlockCandidateGenerationDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCandidate {
    pub version: String,
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub block_hits: Vec<BlockHit>,
    pub candidate_score_hint: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockHit {
    pub operator_id: String,
    pub query_surface_id: String,
    pub candidate_surface_id: String,
    pub rank: Option<usize>,
    pub normalized_key: String,
    pub score_units: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCandidateGenerationDiagnostics {
    pub operator_id: String,
    pub candidate_pair_count: u64,
    pub block_hit_count: u64,
    pub suppressed_candidate_count: u64,
    pub budget: BlockCandidateBudgetDiagnostics,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockCandidateGenerationError {
    Budget(Refusal),
    Ngram(EntityNgramIndexError),
    Posting(PostingLayoutError),
}

impl From<EntityNgramIndexError> for BlockCandidateGenerationError {
    fn from(value: EntityNgramIndexError) -> Self {
        Self::Ngram(value)
    }
}

impl From<PostingLayoutError> for BlockCandidateGenerationError {
    fn from(value: PostingLayoutError) -> Self {
        Self::Posting(value)
    }
}

pub fn generate_ngram_topk_candidates(
    index: &EntityNgramIndex,
    config: &BlockCandidateGenerationConfig,
) -> Result<BlockCandidateSet, BlockCandidateGenerationError> {
    let mut accumulator = CandidateAccumulator::new(config.operator_id.clone());
    let mut observations = Vec::with_capacity(index.surface_ids.len());
    let mut suppressed_candidate_count = 0_u64;

    for surface_id in &index.surface_ids {
        let result = index.top_k_for_surface(surface_id, config.topk_config())?;
        suppressed_candidate_count =
            suppressed_candidate_count.saturating_add(result.dropped.len() as u64);
        observations.push(topk_observation(&config.operator_id, surface_id, &result));
        for candidate in result.candidates {
            accumulator.add_hit(BlockHit {
                operator_id: config.operator_id.clone(),
                query_surface_id: candidate.query_surface_id,
                candidate_surface_id: candidate.candidate_surface_id,
                rank: Some(candidate.rank),
                normalized_key: candidate.normalized_key,
                score_units: candidate.score_units,
            });
        }
    }

    accumulator.finish(&config.budget, observations, suppressed_candidate_count)
}

pub fn generate_rare_token_overlap_candidates(
    index: &EntityPostingIndex,
    config: &RareTokenOverlapConfig,
) -> Result<BlockCandidateSet, BlockCandidateGenerationError> {
    let mut accumulator = CandidateAccumulator::new(config.operator_id.clone());
    let mut surface_counts = BTreeMap::<String, u64>::new();
    let mut suppressed_candidate_count = 0_u64;

    for idf in &index.token_idf {
        if idf.document_frequency > config.max_document_frequency {
            suppressed_candidate_count = suppressed_candidate_count
                .saturating_add(posting_pair_count(idf.document_frequency as usize));
            continue;
        }
        let postings = index.token_postings(&idf.key)?;
        if postings.len() > config.max_posting_len {
            suppressed_candidate_count =
                suppressed_candidate_count.saturating_add(posting_pair_count(postings.len()));
            continue;
        }
        add_posting_pairs(
            &mut accumulator,
            &mut surface_counts,
            &config.operator_id,
            &index.surface_ids,
            &idf.key,
            idf.idf_units,
            postings,
        );
    }

    let observations = observations_from_counts(&config.operator_id, surface_counts);
    accumulator.finish(&config.budget, observations, suppressed_candidate_count)
}

pub fn generate_alias_patch_match_candidates(
    operator_id: impl Into<String>,
    groups: &[AliasPatchMatchGroup],
    budget: &BlockCandidateBudgetConfig,
) -> Result<BlockCandidateSet, BlockCandidateGenerationError> {
    let operator_id = operator_id.into();
    let mut accumulator = CandidateAccumulator::new(operator_id.clone());
    let mut surface_counts = BTreeMap::<String, u64>::new();

    for group in groups {
        let mut surface_ids = group.surface_ids.clone();
        surface_ids.sort();
        surface_ids.dedup();
        for left_index in 0..surface_ids.len() {
            for right_index in left_index + 1..surface_ids.len() {
                let left = surface_ids[left_index].clone();
                let right = surface_ids[right_index].clone();
                increment_surface_count(&mut surface_counts, &left);
                increment_surface_count(&mut surface_counts, &right);
                accumulator.add_hit(BlockHit {
                    operator_id: operator_id.clone(),
                    query_surface_id: left,
                    candidate_surface_id: right,
                    rank: None,
                    normalized_key: group.group_id.clone(),
                    score_units: group.score_units,
                });
            }
        }
    }

    let observations = observations_from_counts(&operator_id, surface_counts);
    accumulator.finish(budget, observations, 0)
}

fn topk_observation(
    operator_id: &str,
    surface_id: &str,
    result: &TopKPruneResult,
) -> BlockCandidateBudgetObservation {
    BlockCandidateBudgetObservation::new(
        surface_id,
        operator_id,
        result.candidates.len() as u64,
        result.dropped.len() as u64,
    )
}

fn add_posting_pairs(
    accumulator: &mut CandidateAccumulator,
    surface_counts: &mut BTreeMap<String, u64>,
    operator_id: &str,
    surface_ids: &[String],
    normalized_key: &str,
    score_units: u32,
    postings: &[PostingRecord],
) {
    for left_index in 0..postings.len() {
        for right_index in left_index + 1..postings.len() {
            let left = surface_ids[postings[left_index].surface_ordinal as usize].clone();
            let right = surface_ids[postings[right_index].surface_ordinal as usize].clone();
            increment_surface_count(surface_counts, &left);
            increment_surface_count(surface_counts, &right);
            accumulator.add_hit(BlockHit {
                operator_id: operator_id.to_string(),
                query_surface_id: left,
                candidate_surface_id: right,
                rank: None,
                normalized_key: normalized_key.to_string(),
                score_units,
            });
        }
    }
}

fn observations_from_counts(
    operator_id: &str,
    surface_counts: BTreeMap<String, u64>,
) -> Vec<BlockCandidateBudgetObservation> {
    surface_counts
        .into_iter()
        .map(|(surface_id, emitted_count)| {
            BlockCandidateBudgetObservation::new(surface_id, operator_id, emitted_count, 0)
        })
        .collect()
}

fn increment_surface_count(surface_counts: &mut BTreeMap<String, u64>, surface_id: &str) {
    *surface_counts.entry(surface_id.to_string()).or_default() += 1;
}

fn posting_pair_count(posting_len: usize) -> u64 {
    let posting_len = posting_len as u64;
    posting_len.saturating_mul(posting_len.saturating_sub(1)) / 2
}

#[derive(Debug, Default)]
struct CandidateAccumulator {
    operator_id: String,
    candidates: BTreeMap<(String, String), BlockCandidate>,
}

impl CandidateAccumulator {
    fn new(operator_id: String) -> Self {
        Self {
            operator_id,
            candidates: BTreeMap::new(),
        }
    }

    fn add_hit(&mut self, hit: BlockHit) {
        if hit.query_surface_id == hit.candidate_surface_id {
            return;
        }
        let (left_surface_id, right_surface_id) =
            ordered_pair(&hit.query_surface_id, &hit.candidate_surface_id);
        let candidate = self
            .candidates
            .entry((left_surface_id.clone(), right_surface_id.clone()))
            .or_insert_with(|| BlockCandidate {
                version: CANON_ENTITY_BLOCK_VERSION.to_string(),
                left_surface_id,
                right_surface_id,
                block_hits: Vec::new(),
                candidate_score_hint: 0,
            });
        candidate.candidate_score_hint = candidate.candidate_score_hint.max(hit.score_units);
        candidate.block_hits.push(hit);
    }

    fn finish(
        self,
        budget: &BlockCandidateBudgetConfig,
        observations: Vec<BlockCandidateBudgetObservation>,
        suppressed_candidate_count: u64,
    ) -> Result<BlockCandidateSet, BlockCandidateGenerationError> {
        let budget =
            validate_block_candidate_budget_before_artifact_emission(budget, &observations)
                .map_err(BlockCandidateGenerationError::Budget)?;
        let mut candidates = self.candidates.into_values().collect::<Vec<_>>();
        for candidate in &mut candidates {
            candidate.block_hits.sort();
        }
        candidates.sort_by(candidate_cmp);
        let block_hit_count = candidates
            .iter()
            .map(|candidate| candidate.block_hits.len() as u64)
            .sum();

        Ok(BlockCandidateSet {
            diagnostics: BlockCandidateGenerationDiagnostics {
                operator_id: self.operator_id,
                candidate_pair_count: candidates.len() as u64,
                block_hit_count,
                suppressed_candidate_count,
                budget,
            },
            candidates,
        })
    }
}

fn ordered_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn candidate_cmp(left: &BlockCandidate, right: &BlockCandidate) -> std::cmp::Ordering {
    right
        .candidate_score_hint
        .cmp(&left.candidate_score_hint)
        .then_with(|| left.left_surface_id.cmp(&right.left_surface_id))
        .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
}
