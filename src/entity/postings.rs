//! Compact deterministic posting-list layout for entity index accelerators.
//!
//! The layout is intentionally semantic-free: it stores corpus-local feature
//! IDs, CSR-style offsets, and sorted postings so later block/edge stages can
//! reload indexes without choosing a second sparse representation.

use crate::namekit::{
    ids::TokenSymbolTable,
    tfidf::{idf_units, tf_units},
};
use crate::witness;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    ops::Range,
};

pub const ENTITY_POSTINGS_LAYOUT_VERSION: &str = "canon_entity_postings.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostingFeatureKind {
    ExactView,
    Token,
    Ngram,
    TfidfTerm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingDictionaryEntry {
    pub kind: PostingFeatureKind,
    pub term_id: u32,
    pub key: String,
}

impl PostingDictionaryEntry {
    pub fn new(kind: PostingFeatureKind, term_id: u32, key: impl Into<String>) -> Self {
        Self {
            kind,
            term_id,
            key: key.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingInput {
    pub term_id: u32,
    pub surface_ordinal: u32,
    pub weight_units: u64,
}

impl PostingInput {
    pub const fn new(term_id: u32, surface_ordinal: u32, weight_units: u64) -> Self {
        Self {
            term_id,
            surface_ordinal,
            weight_units,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingRecord {
    pub surface_ordinal: u32,
    pub weight_units: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommonPostingDiagnostic {
    pub term_id: u32,
    pub key: String,
    pub posting_count: usize,
    pub configured_limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingLayout {
    pub version: String,
    pub surface_count: u32,
    pub dictionary_hash: String,
    pub dictionary: Vec<PostingDictionaryEntry>,
    pub term_offsets: Vec<usize>,
    pub postings: Vec<PostingRecord>,
    pub common_posting_diagnostics: Vec<CommonPostingDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPostingSurface {
    pub surface_id: String,
    pub exact_views: Vec<EntityExactViewFeature>,
    pub tokens: Vec<String>,
}

impl EntityPostingSurface {
    pub fn new(surface_id: impl Into<String>) -> Self {
        Self {
            surface_id: surface_id.into(),
            exact_views: Vec::new(),
            tokens: Vec::new(),
        }
    }

    pub fn with_exact_view(
        mut self,
        view_name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.exact_views
            .push(EntityExactViewFeature::new(view_name, value));
        self
    }

    pub fn with_tokens(mut self, tokens: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tokens.extend(tokens.into_iter().map(Into::into));
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityExactViewFeature {
    pub view_name: String,
    pub value: String,
}

impl EntityExactViewFeature {
    pub fn new(view_name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            view_name: view_name.into(),
            value: value.into(),
        }
    }

    fn dictionary_key(&self) -> String {
        exact_view_key(&self.view_name, &self.value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPostingBuildConfig {
    pub common_posting_limit: usize,
}

impl Default for EntityPostingBuildConfig {
    fn default() -> Self {
        Self {
            common_posting_limit: 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPostingIndex {
    pub version: String,
    pub surface_ids: Vec<String>,
    pub exact_view_layout: PostingLayout,
    pub token_layout: PostingLayout,
    pub tfidf_layout: PostingLayout,
    pub token_idf: Vec<PostingIdfSummary>,
    pub diagnostics: EntityPostingDiagnostics,
}

impl EntityPostingIndex {
    pub fn build(
        surfaces: &[EntityPostingSurface],
        config: EntityPostingBuildConfig,
    ) -> Result<Self, PostingLayoutError> {
        let normalized = normalize_surfaces(surfaces)?;
        let surface_count = usize_to_u32(normalized.len())?;
        let surface_ids = normalized
            .iter()
            .map(|surface| surface.surface_id.clone())
            .collect::<Vec<_>>();

        let exact_view_layout =
            build_exact_view_layout(surface_count, &normalized, config.common_posting_limit)?;
        let token_idf = build_token_idf(&normalized);
        let token_layout =
            build_token_layout(surface_count, &normalized, config.common_posting_limit)?;
        let tfidf_layout = build_tfidf_layout(
            surface_count,
            &normalized,
            &token_idf,
            config.common_posting_limit,
        )?;
        let diagnostics = EntityPostingDiagnostics::from_layouts(
            surface_count,
            &exact_view_layout,
            &token_layout,
        );

        Ok(Self {
            version: ENTITY_POSTINGS_LAYOUT_VERSION.to_string(),
            surface_ids,
            exact_view_layout,
            token_layout,
            tfidf_layout,
            token_idf,
            diagnostics,
        })
    }

    pub fn exact_view_postings(
        &self,
        view_name: &str,
        value: &str,
    ) -> Result<&[PostingRecord], PostingLayoutError> {
        self.exact_view_layout.postings_for_key(
            PostingFeatureKind::ExactView,
            &exact_view_key(view_name, value),
        )
    }

    pub fn token_postings(&self, token: &str) -> Result<&[PostingRecord], PostingLayoutError> {
        self.token_layout
            .postings_for_key(PostingFeatureKind::Token, token)
    }

    pub fn tfidf_postings(&self, token: &str) -> Result<&[PostingRecord], PostingLayoutError> {
        self.tfidf_layout
            .postings_for_key(PostingFeatureKind::TfidfTerm, token)
    }

    pub fn token_idf(&self, token: &str) -> Option<&PostingIdfSummary> {
        self.token_idf.iter().find(|entry| entry.key == token)
    }

    pub fn exact_view_buckets(&self) -> Result<Vec<ExactViewPostingBucket>, PostingLayoutError> {
        self.exact_view_layout
            .dictionary
            .iter()
            .map(|entry| {
                let (view_name, value) = split_exact_view_key(&entry.key);
                let postings = self.exact_view_layout.postings_for_term(entry.term_id)?;
                Ok(ExactViewPostingBucket {
                    term_id: entry.term_id,
                    view_name,
                    value,
                    surface_ordinals: postings
                        .iter()
                        .map(|posting| posting.surface_ordinal)
                        .collect(),
                    surface_count: postings.len(),
                    pair_expansion: "forbidden".to_string(),
                })
            })
            .collect()
    }

    pub fn exact_view_value_frequencies(
        &self,
    ) -> Result<Vec<ExactViewValueFrequency>, PostingLayoutError> {
        let mut frequencies = self
            .exact_view_buckets()?
            .into_iter()
            .map(|bucket| ExactViewValueFrequency {
                term_id: bucket.term_id,
                view_name: bucket.view_name,
                value: bucket.value,
                count: u64::try_from(bucket.surface_count).unwrap_or(u64::MAX),
            })
            .collect::<Vec<_>>();
        frequencies.sort_by(exact_view_value_frequency_cmp);
        Ok(frequencies)
    }

    pub fn content_hash(&self) -> Result<String, PostingLayoutError> {
        let bytes = serde_json::to_vec(self)
            .map_err(|error| PostingLayoutError::Serialization(error.to_string()))?;
        Ok(witness::hash_bytes(&bytes))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingIdfSummary {
    pub term_id: u32,
    pub key: String,
    pub document_frequency: u32,
    pub idf_units: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactViewPostingBucket {
    pub term_id: u32,
    pub view_name: String,
    pub value: String,
    pub surface_ordinals: Vec<u32>,
    pub surface_count: usize,
    pub pair_expansion: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactViewValueFrequency {
    pub term_id: u32,
    pub view_name: String,
    pub value: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPostingDiagnostics {
    pub surface_count: u32,
    pub exact_view_count: usize,
    pub token_count: usize,
    pub tfidf_term_count: usize,
    pub large_exact_view_bucket_count: usize,
    pub common_token_count: usize,
    pub largest_exact_view_bucket_size: usize,
    pub largest_token_posting_size: usize,
    pub suppressed_exact_view_pair_count: u64,
    pub exact_bucket_pair_expansion_count: u64,
}

impl EntityPostingDiagnostics {
    fn from_layouts(
        surface_count: u32,
        exact_view_layout: &PostingLayout,
        token_layout: &PostingLayout,
    ) -> Self {
        let exact_bucket_sizes = posting_lengths(exact_view_layout);
        let token_posting_sizes = posting_lengths(token_layout);
        let suppressed_exact_view_pair_count = exact_bucket_sizes
            .iter()
            .map(|count| {
                let count = u64::try_from(*count).unwrap_or(u64::MAX);
                count.saturating_mul(count.saturating_sub(1)) / 2
            })
            .sum();

        Self {
            surface_count,
            exact_view_count: exact_view_layout.dictionary.len(),
            token_count: token_layout.dictionary.len(),
            tfidf_term_count: token_layout.dictionary.len(),
            large_exact_view_bucket_count: exact_view_layout.common_posting_diagnostics.len(),
            common_token_count: token_layout.common_posting_diagnostics.len(),
            largest_exact_view_bucket_size: exact_bucket_sizes.into_iter().max().unwrap_or(0),
            largest_token_posting_size: token_posting_sizes.into_iter().max().unwrap_or(0),
            suppressed_exact_view_pair_count,
            exact_bucket_pair_expansion_count: 0,
        }
    }
}

impl PostingLayout {
    pub fn build(
        surface_count: u32,
        dictionary: Vec<PostingDictionaryEntry>,
        postings: Vec<PostingInput>,
        common_posting_limit: usize,
    ) -> Result<Self, PostingLayoutError> {
        let dictionary = normalize_dictionary(dictionary)?;
        let dictionary_hash = dictionary_hash(&dictionary)?;
        let mut postings_by_term = vec![Vec::<PostingRecord>::new(); dictionary.len()];

        for posting in postings {
            let term_index = usize::try_from(posting.term_id)
                .map_err(|_| PostingLayoutError::TermIdOverflow(posting.term_id))?;
            if term_index >= dictionary.len() {
                return Err(PostingLayoutError::UnknownTermId(posting.term_id));
            }
            if posting.surface_ordinal >= surface_count {
                return Err(PostingLayoutError::SurfaceOrdinalOutOfRange {
                    term_id: posting.term_id,
                    surface_ordinal: posting.surface_ordinal,
                    surface_count,
                });
            }
            postings_by_term[term_index].push(PostingRecord {
                surface_ordinal: posting.surface_ordinal,
                weight_units: posting.weight_units,
            });
        }

        let mut term_offsets = Vec::with_capacity(dictionary.len() + 1);
        let mut flattened = Vec::new();
        let mut diagnostics = Vec::new();
        term_offsets.push(0);

        for (term_index, mut term_postings) in postings_by_term.into_iter().enumerate() {
            term_postings.sort_by_key(|posting| posting.surface_ordinal);
            reject_duplicate_surface(dictionary[term_index].term_id, &term_postings)?;
            if common_posting_limit > 0 && term_postings.len() > common_posting_limit {
                diagnostics.push(CommonPostingDiagnostic {
                    term_id: dictionary[term_index].term_id,
                    key: dictionary[term_index].key.clone(),
                    posting_count: term_postings.len(),
                    configured_limit: common_posting_limit,
                });
            }
            flattened.extend(term_postings);
            term_offsets.push(flattened.len());
        }

        let layout = Self {
            version: ENTITY_POSTINGS_LAYOUT_VERSION.to_string(),
            surface_count,
            dictionary_hash,
            dictionary,
            term_offsets,
            postings: flattened,
            common_posting_diagnostics: diagnostics,
        };
        layout.validate_reload()?;
        Ok(layout)
    }

    pub fn postings_for_term(&self, term_id: u32) -> Result<&[PostingRecord], PostingLayoutError> {
        let range = self.posting_range(term_id)?;
        Ok(&self.postings[range])
    }

    pub fn postings_for_key(
        &self,
        kind: PostingFeatureKind,
        key: &str,
    ) -> Result<&[PostingRecord], PostingLayoutError> {
        let term_id = self.term_id_for(kind, key).ok_or_else(|| {
            PostingLayoutError::UnknownDictionaryKey {
                kind,
                key: key.to_string(),
            }
        })?;
        self.postings_for_term(term_id)
    }

    pub fn term_id_for(&self, kind: PostingFeatureKind, key: &str) -> Option<u32> {
        self.dictionary
            .iter()
            .find(|entry| entry.kind == kind && entry.key == key)
            .map(|entry| entry.term_id)
    }

    pub fn posting_range(&self, term_id: u32) -> Result<Range<usize>, PostingLayoutError> {
        let index =
            usize::try_from(term_id).map_err(|_| PostingLayoutError::TermIdOverflow(term_id))?;
        if index >= self.dictionary.len() {
            return Err(PostingLayoutError::UnknownTermId(term_id));
        }
        Ok(self.term_offsets[index]..self.term_offsets[index + 1])
    }

    pub fn validate_reload(&self) -> Result<(), PostingLayoutError> {
        if self.version != ENTITY_POSTINGS_LAYOUT_VERSION {
            return Err(PostingLayoutError::VersionMismatch {
                expected: ENTITY_POSTINGS_LAYOUT_VERSION,
                actual: self.version.clone(),
            });
        }
        validate_dictionary(&self.dictionary)?;

        let expected_hash = dictionary_hash(&self.dictionary)?;
        if self.dictionary_hash != expected_hash {
            return Err(PostingLayoutError::DictionaryHashMismatch {
                expected: expected_hash,
                actual: self.dictionary_hash.clone(),
            });
        }
        if self.term_offsets.len() != self.dictionary.len() + 1 {
            return Err(PostingLayoutError::OffsetLengthMismatch {
                expected: self.dictionary.len() + 1,
                actual: self.term_offsets.len(),
            });
        }
        if self.term_offsets.first().copied() != Some(0) {
            return Err(PostingLayoutError::OffsetStartMismatch);
        }
        if self.term_offsets.last().copied() != Some(self.postings.len()) {
            return Err(PostingLayoutError::OffsetEndMismatch {
                expected: self.postings.len(),
                actual: self.term_offsets.last().copied().unwrap_or_default(),
            });
        }
        for offsets in self.term_offsets.windows(2) {
            if offsets[0] > offsets[1] {
                return Err(PostingLayoutError::OffsetsNotMonotonic);
            }
        }
        for (term_index, entry) in self.dictionary.iter().enumerate() {
            let start = self.term_offsets[term_index];
            let end = self.term_offsets[term_index + 1];
            let slice = &self.postings[start..end];
            reject_duplicate_surface(entry.term_id, slice)?;
            if slice
                .iter()
                .any(|posting| posting.surface_ordinal >= self.surface_count)
            {
                return Err(PostingLayoutError::SurfaceOrdinalOutOfRange {
                    term_id: entry.term_id,
                    surface_ordinal: slice
                        .iter()
                        .find(|posting| posting.surface_ordinal >= self.surface_count)
                        .map(|posting| posting.surface_ordinal)
                        .unwrap_or_default(),
                    surface_count: self.surface_count,
                });
            }
            if !slice
                .windows(2)
                .all(|pair| pair[0].surface_ordinal < pair[1].surface_ordinal)
            {
                return Err(PostingLayoutError::PostingsNotSorted {
                    term_id: entry.term_id,
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostingLayoutError {
    DictionaryHashMismatch {
        expected: String,
        actual: String,
    },
    DictionaryNotCompact {
        expected: u32,
        actual: u32,
    },
    DictionaryNotSorted,
    DuplicateDictionaryTermId(u32),
    DuplicateSurfaceForTerm {
        term_id: u32,
        surface_ordinal: u32,
    },
    OffsetEndMismatch {
        expected: usize,
        actual: usize,
    },
    OffsetLengthMismatch {
        expected: usize,
        actual: usize,
    },
    OffsetStartMismatch,
    OffsetsNotMonotonic,
    PostingsNotSorted {
        term_id: u32,
    },
    SurfaceOrdinalOutOfRange {
        term_id: u32,
        surface_ordinal: u32,
        surface_count: u32,
    },
    SurfaceCountOverflow(usize),
    Serialization(String),
    TermIdOverflow(u32),
    UnknownTermId(u32),
    UnknownDictionaryKey {
        kind: PostingFeatureKind,
        key: String,
    },
    VersionMismatch {
        expected: &'static str,
        actual: String,
    },
}

impl fmt::Display for PostingLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for PostingLayoutError {}

fn normalize_dictionary(
    mut dictionary: Vec<PostingDictionaryEntry>,
) -> Result<Vec<PostingDictionaryEntry>, PostingLayoutError> {
    dictionary.sort_by_key(|entry| (entry.term_id, entry.kind, entry.key.clone()));
    validate_dictionary(&dictionary)?;
    Ok(dictionary)
}

fn validate_dictionary(dictionary: &[PostingDictionaryEntry]) -> Result<(), PostingLayoutError> {
    let mut previous_key: Option<(u32, PostingFeatureKind, &str)> = None;
    for (index, entry) in dictionary.iter().enumerate() {
        let expected = u32::try_from(index).map_err(|_| PostingLayoutError::TermIdOverflow(0))?;
        if entry.term_id != expected {
            return Err(PostingLayoutError::DictionaryNotCompact {
                expected,
                actual: entry.term_id,
            });
        }
        let key = (entry.term_id, entry.kind, entry.key.as_str());
        if let Some(previous) = previous_key
            && previous >= key
        {
            if previous.0 == entry.term_id {
                return Err(PostingLayoutError::DuplicateDictionaryTermId(entry.term_id));
            }
            return Err(PostingLayoutError::DictionaryNotSorted);
        }
        previous_key = Some(key);
    }
    Ok(())
}

fn reject_duplicate_surface(
    term_id: u32,
    postings: &[PostingRecord],
) -> Result<(), PostingLayoutError> {
    for pair in postings.windows(2) {
        if pair[0].surface_ordinal == pair[1].surface_ordinal {
            return Err(PostingLayoutError::DuplicateSurfaceForTerm {
                term_id,
                surface_ordinal: pair[0].surface_ordinal,
            });
        }
    }
    Ok(())
}

fn dictionary_hash(dictionary: &[PostingDictionaryEntry]) -> Result<String, PostingLayoutError> {
    validate_dictionary(dictionary)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(ENTITY_POSTINGS_LAYOUT_VERSION.as_bytes());
    for entry in dictionary {
        hasher.update(entry.term_id.to_string().as_bytes());
        hasher.update(&[0]);
        hasher.update(format!("{:?}", entry.kind).as_bytes());
        hasher.update(&[0]);
        hasher.update(entry.key.len().to_string().as_bytes());
        hasher.update(&[0]);
        hasher.update(entry.key.as_bytes());
        hasher.update(&[0xff]);
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

#[derive(Debug, Clone, Default)]
struct NormalizedSurface {
    surface_id: String,
    exact_views: BTreeSet<EntityExactViewFeature>,
    tokens: BTreeSet<String>,
}

fn normalize_surfaces(
    surfaces: &[EntityPostingSurface],
) -> Result<Vec<NormalizedSurface>, PostingLayoutError> {
    let mut normalized = BTreeMap::<String, NormalizedSurface>::new();
    for surface in surfaces {
        let entry = normalized
            .entry(surface.surface_id.clone())
            .or_insert_with(|| NormalizedSurface {
                surface_id: surface.surface_id.clone(),
                exact_views: BTreeSet::new(),
                tokens: BTreeSet::new(),
            });
        for exact_view in &surface.exact_views {
            if !exact_view.view_name.trim().is_empty() && !exact_view.value.trim().is_empty() {
                entry.exact_views.insert(exact_view.clone());
            }
        }
        for token in &surface.tokens {
            if token.trim().is_empty() {
                continue;
            }
            entry.tokens.insert(token.clone());
        }
    }

    Ok(normalized.into_values().collect())
}

fn build_exact_view_layout(
    surface_count: u32,
    surfaces: &[NormalizedSurface],
    common_posting_limit: usize,
) -> Result<PostingLayout, PostingLayoutError> {
    let exact_keys = surfaces
        .iter()
        .flat_map(|surface| {
            surface
                .exact_views
                .iter()
                .map(EntityExactViewFeature::dictionary_key)
        })
        .collect::<BTreeSet<_>>();
    let dictionary = dictionary_from_keys(PostingFeatureKind::ExactView, exact_keys)?;
    let term_ids = term_ids_by_key(&dictionary);
    let mut postings = Vec::new();
    for (surface_ordinal, surface) in surfaces.iter().enumerate() {
        let surface_ordinal =
            u32::try_from(surface_ordinal).expect("surface ordinal already validated");
        for feature in &surface.exact_views {
            let key = feature.dictionary_key();
            postings.push(PostingInput::new(term_ids[&key], surface_ordinal, 1));
        }
    }
    PostingLayout::build(surface_count, dictionary, postings, common_posting_limit)
}

fn build_token_layout(
    surface_count: u32,
    surfaces: &[NormalizedSurface],
    common_posting_limit: usize,
) -> Result<PostingLayout, PostingLayoutError> {
    let token_table = token_symbol_table(surfaces);
    let dictionary = token_table
        .entries
        .iter()
        .map(|entry| {
            PostingDictionaryEntry::new(
                PostingFeatureKind::Token,
                entry.id.as_u32(),
                entry.value.clone(),
            )
        })
        .collect::<Vec<_>>();
    let postings = surfaces
        .iter()
        .enumerate()
        .flat_map(|(surface_ordinal, surface)| {
            let token_table = &token_table;
            surface.tokens.iter().map(move |token| {
                PostingInput::new(
                    token_table
                        .token_id(token)
                        .expect("token came from token symbol table")
                        .as_u32(),
                    u32::try_from(surface_ordinal).expect("surface ordinal already validated"),
                    1,
                )
            })
        })
        .collect::<Vec<_>>();
    PostingLayout::build(surface_count, dictionary, postings, common_posting_limit)
}

fn build_tfidf_layout(
    surface_count: u32,
    surfaces: &[NormalizedSurface],
    token_idf: &[PostingIdfSummary],
    common_posting_limit: usize,
) -> Result<PostingLayout, PostingLayoutError> {
    let idf_by_token = token_idf
        .iter()
        .map(|entry| (entry.key.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let dictionary = token_idf
        .iter()
        .map(|entry| {
            PostingDictionaryEntry::new(PostingFeatureKind::TfidfTerm, entry.term_id, &entry.key)
        })
        .collect::<Vec<_>>();
    let postings = surfaces
        .iter()
        .enumerate()
        .flat_map(|(surface_ordinal, surface)| {
            let idf_by_token = &idf_by_token;
            surface.tokens.iter().map(move |token| {
                let idf = idf_by_token[token.as_str()];
                PostingInput::new(
                    idf.term_id,
                    u32::try_from(surface_ordinal).expect("surface ordinal already validated"),
                    u64::from(tf_units(1)) * u64::from(idf.idf_units),
                )
            })
        })
        .collect::<Vec<_>>();
    PostingLayout::build(surface_count, dictionary, postings, common_posting_limit)
}

fn build_token_idf(surfaces: &[NormalizedSurface]) -> Vec<PostingIdfSummary> {
    let token_table = token_symbol_table(surfaces);
    let mut document_frequency = vec![0_u32; token_table.entries.len()];
    for surface in surfaces {
        for token in &surface.tokens {
            if let Some(token_id) = token_table.token_id(token) {
                document_frequency[token_id.as_u32() as usize] += 1;
            }
        }
    }
    let document_count = u32::try_from(surfaces.len()).unwrap_or(u32::MAX);
    token_table
        .entries
        .into_iter()
        .map(|entry| {
            let frequency = document_frequency[entry.id.as_u32() as usize];
            PostingIdfSummary {
                term_id: entry.id.as_u32(),
                key: entry.value,
                document_frequency: frequency,
                idf_units: idf_units(document_count, frequency),
            }
        })
        .collect()
}

fn token_symbol_table(surfaces: &[NormalizedSurface]) -> TokenSymbolTable {
    TokenSymbolTable::from_tokens(
        surfaces
            .iter()
            .flat_map(|surface| surface.tokens.iter().cloned()),
    )
}

fn dictionary_from_keys(
    kind: PostingFeatureKind,
    keys: BTreeSet<String>,
) -> Result<Vec<PostingDictionaryEntry>, PostingLayoutError> {
    keys.into_iter()
        .enumerate()
        .map(|(index, key)| {
            Ok(PostingDictionaryEntry::new(
                kind,
                usize_to_term_id(index)?,
                key,
            ))
        })
        .collect()
}

fn term_ids_by_key(dictionary: &[PostingDictionaryEntry]) -> BTreeMap<String, u32> {
    dictionary
        .iter()
        .map(|entry| (entry.key.clone(), entry.term_id))
        .collect()
}

fn posting_lengths(layout: &PostingLayout) -> Vec<usize> {
    layout
        .term_offsets
        .windows(2)
        .map(|window| window[1].saturating_sub(window[0]))
        .collect()
}

fn exact_view_value_frequency_cmp(
    left: &ExactViewValueFrequency,
    right: &ExactViewValueFrequency,
) -> std::cmp::Ordering {
    left.view_name
        .as_bytes()
        .cmp(right.view_name.as_bytes())
        .then_with(|| left.value.as_bytes().cmp(right.value.as_bytes()))
        .then_with(|| left.term_id.cmp(&right.term_id))
}

fn exact_view_key(view_name: &str, value: &str) -> String {
    format!("{view_name}:{value}")
}

fn split_exact_view_key(key: &str) -> (String, String) {
    key.split_once(':').map_or_else(
        || (String::new(), key.to_string()),
        |(view_name, value)| (view_name.to_string(), value.to_string()),
    )
}

fn usize_to_u32(value: usize) -> Result<u32, PostingLayoutError> {
    u32::try_from(value).map_err(|_| PostingLayoutError::SurfaceCountOverflow(value))
}

fn usize_to_term_id(value: usize) -> Result<u32, PostingLayoutError> {
    u32::try_from(value).map_err(|_| PostingLayoutError::TermIdOverflow(u32::MAX))
}
