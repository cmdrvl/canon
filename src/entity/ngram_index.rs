#![forbid(unsafe_code)]

//! Deterministic char-ngram postings for entity candidate retrieval.
//!
//! The index is a bounded accelerator over prepared unique surfaces. It
//! builds sorted char-ngram postings and uses the shared stable top-k heap for
//! deterministic candidate pruning.

use crate::entity::{
    postings::{
        PostingDictionaryEntry, PostingFeatureKind, PostingInput, PostingLayout, PostingLayoutError,
    },
    topk::{TopKCandidateInput, TopKConfig, TopKPruneResult, prune_top_k_candidates},
};
use crate::namekit::{
    ids::NgramSymbolTable,
    ngram::{NgramConfig, char_ngrams},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CANON_ENTITY_NGRAM_INDEX_VERSION: &str = "canon_entity_ngram_index.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityNgramSurface {
    pub surface_id: String,
    pub normalized_key: String,
}

impl EntityNgramSurface {
    pub fn new(surface_id: impl Into<String>, normalized_key: impl Into<String>) -> Self {
        Self {
            surface_id: surface_id.into(),
            normalized_key: normalized_key.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityNgramBuildConfig {
    pub ngram: NgramConfig,
    pub common_posting_limit: usize,
}

impl Default for EntityNgramBuildConfig {
    fn default() -> Self {
        Self {
            ngram: NgramConfig::DEFAULT,
            common_posting_limit: 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityNgramIndex {
    pub version: String,
    pub surface_ids: Vec<String>,
    pub ngram_layout: PostingLayout,
    pub diagnostics: EntityNgramDiagnostics,
    surfaces: Vec<NormalizedNgramSurface>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedNgramSurface {
    surface_id: String,
    normalized_key: String,
    ngrams: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityNgramDiagnostics {
    pub surface_count: u32,
    pub ngram_count: usize,
    pub total_posting_count: usize,
    pub common_ngram_count: usize,
    pub largest_ngram_posting_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityNgramIndexError {
    Layout(PostingLayoutError),
    UnknownSurfaceId { surface_id: String },
}

impl From<PostingLayoutError> for EntityNgramIndexError {
    fn from(value: PostingLayoutError) -> Self {
        Self::Layout(value)
    }
}

impl EntityNgramIndex {
    pub fn build(
        surfaces: &[EntityNgramSurface],
        config: EntityNgramBuildConfig,
    ) -> Result<Self, EntityNgramIndexError> {
        let normalized = normalize_surfaces(surfaces, config.ngram);
        let surface_count = u32::try_from(normalized.len())
            .map_err(|_| PostingLayoutError::SurfaceCountOverflow(normalized.len()))?;
        let surface_ids = normalized
            .iter()
            .map(|surface| surface.surface_id.clone())
            .collect::<Vec<_>>();
        let ngram_layout =
            build_ngram_layout(surface_count, &normalized, config.common_posting_limit)?;
        let diagnostics = EntityNgramDiagnostics::from_layout(surface_count, &ngram_layout);

        Ok(Self {
            version: CANON_ENTITY_NGRAM_INDEX_VERSION.to_string(),
            surface_ids,
            ngram_layout,
            diagnostics,
            surfaces: normalized,
        })
    }

    pub fn ngram_postings(
        &self,
        ngram: &str,
    ) -> Result<&[crate::entity::postings::PostingRecord], PostingLayoutError> {
        self.ngram_layout
            .postings_for_key(PostingFeatureKind::Ngram, ngram)
    }

    pub fn top_k_for_surface(
        &self,
        surface_id: &str,
        config: TopKConfig,
    ) -> Result<TopKPruneResult, EntityNgramIndexError> {
        let query_ordinal = self
            .surface_ids
            .iter()
            .position(|candidate| candidate == surface_id)
            .ok_or_else(|| EntityNgramIndexError::UnknownSurfaceId {
                surface_id: surface_id.to_string(),
            })?;
        self.top_k_for_ordinal(query_ordinal, config)
    }

    fn top_k_for_ordinal(
        &self,
        query_ordinal: usize,
        config: TopKConfig,
    ) -> Result<TopKPruneResult, EntityNgramIndexError> {
        let query = &self.surfaces[query_ordinal];
        let mut scores = BTreeMap::<usize, u64>::new();

        for (ngram, query_weight) in &query.ngrams {
            for posting in self
                .ngram_layout
                .postings_for_key(PostingFeatureKind::Ngram, ngram)?
            {
                let candidate_ordinal = posting.surface_ordinal as usize;
                if candidate_ordinal == query_ordinal {
                    continue;
                }
                let contribution = *query_weight * posting.weight_units;
                *scores.entry(candidate_ordinal).or_insert(0) += contribution;
            }
        }

        let candidates = scores
            .into_iter()
            .map(|(candidate_ordinal, score_units)| {
                let candidate = &self.surfaces[candidate_ordinal];
                TopKCandidateInput::new(
                    query.surface_id.clone(),
                    candidate.surface_id.clone(),
                    candidate.normalized_key.clone(),
                    saturating_u32(score_units),
                )
            })
            .collect::<Vec<_>>();

        Ok(prune_top_k_candidates(config, candidates))
    }
}

fn normalize_surfaces(
    surfaces: &[EntityNgramSurface],
    ngram: NgramConfig,
) -> Vec<NormalizedNgramSurface> {
    let mut normalized = BTreeMap::<String, NormalizedNgramSurface>::new();

    for surface in surfaces {
        let entry = normalized
            .entry(surface.surface_id.clone())
            .or_insert_with(|| NormalizedNgramSurface {
                surface_id: surface.surface_id.clone(),
                normalized_key: String::new(),
                ngrams: BTreeMap::new(),
            });

        if !surface.normalized_key.trim().is_empty()
            && (entry.normalized_key.is_empty() || surface.normalized_key < entry.normalized_key)
        {
            entry.normalized_key = surface.normalized_key.clone();
        }

        let ngrams = char_ngrams(&surface.normalized_key, ngram);
        for text in ngrams.ngrams.into_iter().map(|ngram| ngram.text) {
            *entry.ngrams.entry(text).or_insert(0) += 1;
        }
    }

    normalized.into_values().collect()
}

fn build_ngram_layout(
    surface_count: u32,
    surfaces: &[NormalizedNgramSurface],
    common_posting_limit: usize,
) -> Result<PostingLayout, PostingLayoutError> {
    let ngram_table = ngram_symbol_table(surfaces);
    let mut ngram_term_ids = BTreeMap::new();
    let dictionary = ngram_table
        .entries
        .iter()
        .map(|entry| {
            ngram_term_ids.insert(entry.value.clone(), entry.id.as_u32());
            PostingDictionaryEntry::new(
                PostingFeatureKind::Ngram,
                entry.id.as_u32(),
                entry.value.clone(),
            )
        })
        .collect::<Vec<_>>();

    let postings = surfaces
        .iter()
        .enumerate()
        .flat_map(|(surface_ordinal, surface)| {
            let ngram_term_ids = &ngram_term_ids;
            surface.ngrams.iter().map(move |(ngram, weight_units)| {
                PostingInput::new(
                    *ngram_term_ids
                        .get(ngram.as_str())
                        .expect("ngram came from symbol table"),
                    u32::try_from(surface_ordinal).expect("surface ordinal already validated"),
                    *weight_units,
                )
            })
        })
        .collect::<Vec<_>>();

    PostingLayout::build(surface_count, dictionary, postings, common_posting_limit)
}

fn ngram_symbol_table(surfaces: &[NormalizedNgramSurface]) -> NgramSymbolTable {
    let mut ngrams = BTreeSet::new();
    for surface in surfaces {
        ngrams.extend(surface.ngrams.keys().cloned());
    }
    NgramSymbolTable::from_ngrams(ngrams)
}

fn saturating_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

impl EntityNgramDiagnostics {
    fn from_layout(surface_count: u32, layout: &PostingLayout) -> Self {
        let posting_sizes = posting_lengths(layout);
        Self {
            surface_count,
            ngram_count: layout.dictionary.len(),
            total_posting_count: layout.postings.len(),
            common_ngram_count: layout.common_posting_diagnostics.len(),
            largest_ngram_posting_size: posting_sizes.into_iter().max().unwrap_or(0),
        }
    }
}

fn posting_lengths(layout: &PostingLayout) -> impl Iterator<Item = usize> + '_ {
    layout
        .term_offsets
        .windows(2)
        .map(|window| window[1].saturating_sub(window[0]))
}
