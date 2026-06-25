//! Sparse, deterministic TF-IDF primitives for namekit.
//!
//! This module keeps the ENT-P02.11 contract local to namekit: corpus-local
//! integer dictionaries, sparse rows/postings, fixed-point score units, stable
//! top-k tie order, and sorted-neighborhood diagnostics. It deliberately does
//! not own entity index, block, edge, or solve artifacts.

use crate::namekit::{
    NAMEKIT_SCORE_SCALE, NamekitReason, ReasonCode, ReasonStage, SourceTechnique,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const NAMEKIT_TFIDF_VERSION: &str = "canon_namekit_tfidf.v0";
pub const TF_UNITS_PER_OCCURRENCE: u32 = 1_000;
pub const TF_UNITS_CAP: u32 = 3_000;
pub const IDF_UNITS_BASE: u32 = 1_000;
pub const IDF_UNITS_RANGE: u32 = 1_000;
pub const COMMON_TOKEN_MAX_IDF_UNITS: u32 = 1_499;
pub const RARE_TOKEN_MIN_IDF_UNITS: u32 = 1_500;
pub const DEFAULT_TF_CAP: u32 = 3;
pub const IDF_UNITS_SCALE: u32 = 1_000;
pub const SCORE_UNITS_CAP: u16 = NAMEKIT_SCORE_SCALE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TfidfFeatureKind {
    Token,
    Ngram,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TfidfTermKey {
    pub key: String,
    pub kind: TfidfFeatureKind,
}

impl TfidfTermKey {
    pub fn token(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            kind: TfidfFeatureKind::Token,
        }
    }

    pub fn ngram(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            kind: TfidfFeatureKind::Ngram,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TfidfTermId(u32);

impl TfidfTermId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }

    fn as_index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TfidfTerm {
    pub id: TfidfTermId,
    pub key: TfidfTermKey,
    pub document_frequency: u32,
    pub idf_units: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TfidfInputSurface {
    pub surface_id: String,
    pub normalized_key: String,
    pub features: Vec<TfidfTermKey>,
}

impl TfidfInputSurface {
    pub fn new(
        surface_id: impl Into<String>,
        normalized_key: impl Into<String>,
        features: Vec<TfidfTermKey>,
    ) -> Self {
        Self {
            surface_id: surface_id.into(),
            normalized_key: normalized_key.into(),
            features,
        }
    }

    pub fn tokenized(
        surface_id: impl Into<String>,
        normalized_key: impl Into<String>,
        tokens: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::new(
            surface_id,
            normalized_key,
            tokens.into_iter().map(TfidfTermKey::token).collect(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TfidfRowTerm {
    pub term_id: TfidfTermId,
    pub tf_units: u32,
    pub idf_units: u32,
    pub weight_units: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TfidfSparseRow {
    pub surface_id: String,
    pub normalized_key: String,
    pub terms: Vec<TfidfRowTerm>,
    pub norm_units: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TfidfPosting {
    pub term_id: TfidfTermId,
    pub surface_ordinal: u32,
    pub weight_units: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TfidfPostingSlice {
    pub term_id: TfidfTermId,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SparseTfidfModel {
    pub version: String,
    pub document_count: u32,
    pub terms: Vec<TfidfTerm>,
    pub rows: Vec<TfidfSparseRow>,
    pub postings: Vec<TfidfPosting>,
    pub posting_slices: Vec<TfidfPostingSlice>,
}

impl SparseTfidfModel {
    pub fn build(surfaces: &[TfidfInputSurface]) -> Self {
        let dictionary = build_dictionary(surfaces);
        let document_count = u32::try_from(surfaces.len()).unwrap_or(u32::MAX);
        let mut term_document_frequency = vec![0_u32; dictionary.len()];

        for surface in surfaces {
            let seen = surface
                .features
                .iter()
                .filter_map(|feature| dictionary.get(feature).copied())
                .collect::<BTreeSet<_>>();
            for term_id in seen {
                term_document_frequency[term_id.as_index()] += 1;
            }
        }

        let terms = dictionary
            .iter()
            .map(|(key, id)| {
                let document_frequency = term_document_frequency[id.as_index()];
                TfidfTerm {
                    id: *id,
                    key: key.clone(),
                    document_frequency,
                    idf_units: idf_units(document_count, document_frequency),
                }
            })
            .collect::<Vec<_>>();

        let rows = surfaces
            .iter()
            .map(|surface| build_row(surface, &dictionary, &terms))
            .collect::<Vec<_>>();
        let (postings, posting_slices) = build_postings(&rows, terms.len());

        Self {
            version: NAMEKIT_TFIDF_VERSION.to_string(),
            document_count,
            terms,
            rows,
            postings,
            posting_slices,
        }
    }

    pub fn term(&self, term_id: TfidfTermId) -> Option<&TfidfTerm> {
        self.terms.get(term_id.as_index())
    }

    pub fn term_by_key(&self, key: &TfidfTermKey) -> Option<&TfidfTerm> {
        self.terms.iter().find(|term| &term.key == key)
    }

    pub fn row(&self, surface_id: &str) -> Option<&TfidfSparseRow> {
        self.rows.iter().find(|row| row.surface_id == surface_id)
    }

    pub fn top_k_for_surface(
        &self,
        surface_id: &str,
        config: TopKConfig,
    ) -> Option<TfidfTopKResult> {
        let query_ordinal = self
            .rows
            .iter()
            .position(|row| row.surface_id == surface_id)?;
        Some(self.top_k_for_ordinal(query_ordinal, config))
    }

    pub fn top_k_for_ordinal(&self, query_ordinal: usize, config: TopKConfig) -> TfidfTopKResult {
        let query = &self.rows[query_ordinal];
        let mut accumulators: BTreeMap<usize, CandidateAccumulator> = BTreeMap::new();

        for query_term in &query.terms {
            for posting in self.postings_for_term(query_term.term_id) {
                let candidate_ordinal = posting.surface_ordinal as usize;
                if candidate_ordinal == query_ordinal {
                    continue;
                }
                let accumulator = accumulators.entry(candidate_ordinal).or_default();
                accumulator.dot_units +=
                    u128::from(query_term.weight_units) * u128::from(posting.weight_units);
                accumulator.shared_term_count += 1;
                accumulator.max_shared_idf_units =
                    accumulator.max_shared_idf_units.max(query_term.idf_units);
                accumulator.min_shared_idf_units = accumulator
                    .min_shared_idf_units
                    .map_or(Some(query_term.idf_units), |current| {
                        Some(current.min(query_term.idf_units))
                    });
            }
        }

        let uncapped_candidate_count = accumulators.len();
        let mut candidates = accumulators
            .into_iter()
            .map(|(candidate_ordinal, accumulator)| {
                let candidate = &self.rows[candidate_ordinal];
                let score_units = cosine_score_units(query, candidate, accumulator.dot_units);
                TfidfTopKCandidate {
                    surface_id: candidate.surface_id.clone(),
                    normalized_key: candidate.normalized_key.clone(),
                    score_units,
                    evidence_class: accumulator.evidence_class(),
                    shared_term_count: accumulator.shared_term_count,
                    max_shared_idf_units: accumulator.max_shared_idf_units,
                }
            })
            .collect::<Vec<_>>();
        sort_topk_candidates(&mut candidates);

        let cap = config.candidate_cap.unwrap_or(usize::MAX);
        let capped_candidate_count = uncapped_candidate_count.saturating_sub(cap);
        if candidates.len() > cap {
            candidates.truncate(cap);
        }
        if candidates.len() > config.k {
            candidates.truncate(config.k);
        }

        TfidfTopKResult {
            query_surface_id: query.surface_id.clone(),
            candidates,
            diagnostics: TfidfTopKDiagnostics {
                k: config.k,
                candidate_cap: config.candidate_cap,
                uncapped_candidate_count,
                capped_candidate_count,
                cap_exceeded: capped_candidate_count > 0,
            },
        }
    }

    fn postings_for_term(&self, term_id: TfidfTermId) -> &[TfidfPosting] {
        self.posting_slices
            .get(term_id.as_index())
            .map_or(&[], |slice| &self.postings[slice.start..slice.end])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopKConfig {
    pub k: usize,
    pub candidate_cap: Option<usize>,
}

impl TopKConfig {
    pub const fn new(k: usize) -> Self {
        Self {
            k,
            candidate_cap: None,
        }
    }

    pub const fn with_candidate_cap(mut self, candidate_cap: usize) -> Self {
        self.candidate_cap = Some(candidate_cap);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TfidfEvidenceClass {
    RareTokenSupport,
    CommonTokenOnly,
    Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TfidfTopKCandidate {
    pub surface_id: String,
    pub normalized_key: String,
    pub score_units: u16,
    pub evidence_class: TfidfEvidenceClass,
    pub shared_term_count: u32,
    pub max_shared_idf_units: u32,
}

impl TfidfTopKCandidate {
    pub fn reasons(&self) -> Vec<NamekitReason> {
        let mut reasons = Vec::new();
        match self.evidence_class {
            TfidfEvidenceClass::RareTokenSupport => reasons.push(
                NamekitReason::new(ReasonCode::RareTokenSupport, ReasonStage::Tfidf)
                    .with_source(SourceTechnique::SplinkTfAdjustment)
                    .with_detail(
                        "max_shared_idf_units",
                        self.max_shared_idf_units.to_string(),
                    ),
            ),
            TfidfEvidenceClass::CommonTokenOnly => reasons.push(
                NamekitReason::new(ReasonCode::CommonTokenDownweighted, ReasonStage::Tfidf)
                    .with_source(SourceTechnique::SplinkTfAdjustment)
                    .with_detail(
                        "max_shared_idf_units",
                        self.max_shared_idf_units.to_string(),
                    ),
            ),
            TfidfEvidenceClass::Diagnostic => {}
        }
        reasons
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TfidfTopKDiagnostics {
    pub k: usize,
    pub candidate_cap: Option<usize>,
    pub uncapped_candidate_count: usize,
    pub capped_candidate_count: usize,
    pub cap_exceeded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TfidfTopKResult {
    pub query_surface_id: String,
    pub candidates: Vec<TfidfTopKCandidate>,
    pub diagnostics: TfidfTopKDiagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TfidfConfig {
    pub tf_cap: u32,
    pub idf_scale: u32,
    pub score_cap: u16,
}

impl Default for TfidfConfig {
    fn default() -> Self {
        Self {
            tf_cap: DEFAULT_TF_CAP,
            idf_scale: IDF_UNITS_SCALE,
            score_cap: SCORE_UNITS_CAP,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermEntry {
    pub term_id: u32,
    pub term: String,
    pub document_frequency: u32,
    pub idf_units: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SparseTfidfWeight {
    pub term_id: u32,
    pub term: String,
    pub tf_units: u32,
    pub idf_units: u32,
    pub weight_units: u32,
}

impl SparseTfidfWeight {
    pub fn reason(&self, _document_count: u32) -> NamekitReason {
        if self.idf_units >= RARE_TOKEN_MIN_IDF_UNITS {
            NamekitReason::new(ReasonCode::RareTokenSupport, ReasonStage::Tfidf)
                .with_source(SourceTechnique::SplinkTfAdjustment)
                .with_detail("term", self.term.clone())
        } else {
            NamekitReason::new(ReasonCode::CommonTokenDownweighted, ReasonStage::Tfidf)
                .with_source(SourceTechnique::SplinkTfAdjustment)
                .with_detail("term", self.term.clone())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SparseTfidfIndexRow {
    pub surface_id: String,
    pub terms: Vec<SparseTfidfWeight>,
    pub norm_units: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SparseTfidfPosting {
    pub term_id: u32,
    pub surface_ordinal: u32,
    pub weight_units: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SparseTfidfIndex {
    pub version: String,
    pub config: TfidfConfig,
    pub document_count: u32,
    pub dictionary: Vec<TermEntry>,
    pub rows: Vec<SparseTfidfIndexRow>,
    pub postings: Vec<SparseTfidfPosting>,
    pub term_offsets: Vec<usize>,
}

pub fn build_sparse_tfidf(docs: &[(&str, &[&str])]) -> SparseTfidfIndex {
    build_sparse_tfidf_with_config(docs, TfidfConfig::default())
}

pub fn build_sparse_tfidf_with_config(
    docs: &[(&str, &[&str])],
    config: TfidfConfig,
) -> SparseTfidfIndex {
    let document_count = u32::try_from(docs.len()).unwrap_or(u32::MAX);
    let term_set = docs
        .iter()
        .flat_map(|(_, terms)| terms.iter().copied())
        .collect::<BTreeSet<_>>();
    let term_ids = term_set
        .iter()
        .enumerate()
        .map(|(index, term)| {
            (
                (*term).to_string(),
                u32::try_from(index).expect("tf-idf term id fits in u32"),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut document_frequency = vec![0_u32; term_ids.len()];
    for (_, terms) in docs {
        for term in terms.iter().copied().collect::<BTreeSet<_>>() {
            let term_id = term_ids[term] as usize;
            document_frequency[term_id] += 1;
        }
    }

    let dictionary = term_ids
        .iter()
        .map(|(term, term_id)| {
            let frequency = document_frequency[*term_id as usize];
            TermEntry {
                term_id: *term_id,
                term: term.clone(),
                document_frequency: frequency,
                idf_units: configured_idf_units(document_count, frequency, config.idf_scale),
            }
        })
        .collect::<Vec<_>>();

    let rows = docs
        .iter()
        .map(|(surface_id, terms)| build_compat_row(surface_id, terms, &dictionary, &config))
        .collect::<Vec<_>>();
    let (postings, term_offsets) = build_compat_postings(&rows, dictionary.len());

    SparseTfidfIndex {
        version: NAMEKIT_TFIDF_VERSION.to_string(),
        config,
        document_count,
        dictionary,
        rows,
        postings,
        term_offsets,
    }
}

pub fn top_k_for_surface(
    index: &SparseTfidfIndex,
    surface_id: &str,
    k: usize,
) -> Vec<TfidfTopKCandidate> {
    let Some(query_ordinal) = index
        .rows
        .iter()
        .position(|row| row.surface_id == surface_id)
    else {
        return Vec::new();
    };
    let query = &index.rows[query_ordinal];
    let mut accumulators = BTreeMap::<usize, CandidateAccumulator>::new();

    for query_term in &query.terms {
        let start = index.term_offsets[query_term.term_id as usize];
        let end = index.term_offsets[query_term.term_id as usize + 1];
        for posting in &index.postings[start..end] {
            let candidate_ordinal = posting.surface_ordinal as usize;
            if candidate_ordinal == query_ordinal {
                continue;
            }
            let accumulator = accumulators.entry(candidate_ordinal).or_default();
            accumulator.dot_units +=
                u128::from(query_term.weight_units) * u128::from(posting.weight_units);
            accumulator.shared_term_count += 1;
            accumulator.max_shared_idf_units =
                accumulator.max_shared_idf_units.max(query_term.idf_units);
            accumulator.min_shared_idf_units = accumulator
                .min_shared_idf_units
                .map_or(Some(query_term.idf_units), |current| {
                    Some(current.min(query_term.idf_units))
                });
        }
    }

    let mut candidates = accumulators
        .into_iter()
        .map(|(candidate_ordinal, accumulator)| {
            let candidate = &index.rows[candidate_ordinal];
            TfidfTopKCandidate {
                surface_id: candidate.surface_id.clone(),
                normalized_key: candidate.surface_id.clone(),
                score_units: compat_cosine_score_units(
                    query,
                    candidate,
                    accumulator.dot_units,
                    index.config.score_cap,
                ),
                evidence_class: accumulator.evidence_class(),
                shared_term_count: accumulator.shared_term_count,
                max_shared_idf_units: accumulator.max_shared_idf_units,
            }
        })
        .collect::<Vec<_>>();
    sort_topk_candidates(&mut candidates);
    candidates.truncate(k);
    candidates
}

#[derive(Debug, Default)]
struct CandidateAccumulator {
    dot_units: u128,
    shared_term_count: u32,
    max_shared_idf_units: u32,
    min_shared_idf_units: Option<u32>,
}

impl CandidateAccumulator {
    fn evidence_class(&self) -> TfidfEvidenceClass {
        if self.max_shared_idf_units >= RARE_TOKEN_MIN_IDF_UNITS {
            TfidfEvidenceClass::RareTokenSupport
        } else if self.shared_term_count > 0 {
            TfidfEvidenceClass::CommonTokenOnly
        } else {
            TfidfEvidenceClass::Diagnostic
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortedNeighborhoodInput {
    pub surface_id: String,
    pub key: String,
}

impl SortedNeighborhoodInput {
    pub fn new(surface_id: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            surface_id: surface_id.into(),
            key: key.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortedNeighborhoodPair {
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub left_key: String,
    pub right_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortedNeighborhoodDiagnostics {
    pub key_name: String,
    pub window_size: usize,
    pub pair_cap: Option<usize>,
    pub uncapped_pair_count: usize,
    pub capped_pair_count: usize,
    pub cap_exceeded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortedNeighborhoodResult {
    pub pairs: Vec<SortedNeighborhoodPair>,
    pub diagnostics: SortedNeighborhoodDiagnostics,
    pub window: usize,
    pub cap: usize,
    pub emitted_pair_count: usize,
    pub capped_pair_count: usize,
}

pub fn sorted_neighborhood_pairs(
    inputs: &[(&str, &str)],
    window_size: usize,
    pair_cap: usize,
) -> SortedNeighborhoodResult {
    let typed = inputs
        .iter()
        .map(|(surface_id, key)| SortedNeighborhoodInput::new(*surface_id, *key))
        .collect::<Vec<_>>();
    sorted_neighborhood_pairs_with_key(
        "sorted_neighborhood_key",
        &typed,
        window_size,
        Some(pair_cap),
    )
}

pub fn sorted_neighborhood_pairs_with_key(
    key_name: impl Into<String>,
    inputs: &[SortedNeighborhoodInput],
    window_size: usize,
    pair_cap: Option<usize>,
) -> SortedNeighborhoodResult {
    let mut sorted = inputs.to_vec();
    sorted.sort_by(|left, right| {
        left.key
            .as_bytes()
            .cmp(right.key.as_bytes())
            .then_with(|| left.surface_id.cmp(&right.surface_id))
    });

    let mut deduped_pairs = BTreeSet::<(String, String)>::new();
    let mut pairs = Vec::new();
    if window_size > 1 {
        for left_index in 0..sorted.len() {
            let window_end = (left_index + window_size).min(sorted.len());
            for right_index in (left_index + 1)..window_end {
                let left = &sorted[left_index];
                let right = &sorted[right_index];
                let (left_surface_id, right_surface_id) = ordered_pair_ids(left, right);
                if deduped_pairs.insert((left_surface_id.clone(), right_surface_id.clone())) {
                    pairs.push(SortedNeighborhoodPair {
                        left_surface_id,
                        right_surface_id,
                        left_key: left.key.clone(),
                        right_key: right.key.clone(),
                    });
                }
            }
        }
    }

    pairs.sort_by(|left, right| {
        same_first_token_rank(left)
            .cmp(&same_first_token_rank(right))
            .then_with(|| left.left_key.as_bytes().cmp(right.left_key.as_bytes()))
            .then_with(|| left.right_key.as_bytes().cmp(right.right_key.as_bytes()))
            .then_with(|| left.left_surface_id.cmp(&right.left_surface_id))
            .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
    });

    let uncapped_pair_count = pairs.len();
    let cap = pair_cap.unwrap_or(usize::MAX);
    let capped_pair_count = uncapped_pair_count.saturating_sub(cap);
    if pairs.len() > cap {
        pairs.truncate(cap);
    }

    SortedNeighborhoodResult {
        pairs,
        diagnostics: SortedNeighborhoodDiagnostics {
            key_name: key_name.into(),
            window_size,
            pair_cap,
            uncapped_pair_count,
            capped_pair_count,
            cap_exceeded: capped_pair_count > 0,
        },
        window: window_size,
        cap,
        emitted_pair_count: uncapped_pair_count.min(cap),
        capped_pair_count,
    }
}

pub fn idf_units(document_count: u32, document_frequency: u32) -> u32 {
    if document_count == 0 || document_frequency == 0 {
        return IDF_UNITS_BASE + IDF_UNITS_RANGE;
    }
    let bounded_frequency = document_frequency.min(document_count);
    IDF_UNITS_BASE + ((document_count - bounded_frequency) * IDF_UNITS_RANGE / document_count)
}

pub fn tf_units(raw_count: u32) -> u32 {
    raw_count
        .saturating_mul(TF_UNITS_PER_OCCURRENCE)
        .min(TF_UNITS_CAP)
}

fn build_dictionary(surfaces: &[TfidfInputSurface]) -> BTreeMap<TfidfTermKey, TfidfTermId> {
    let keys = surfaces
        .iter()
        .flat_map(|surface| surface.features.iter().cloned())
        .collect::<BTreeSet<_>>();
    keys.into_iter()
        .enumerate()
        .map(|(index, key)| {
            (
                key,
                TfidfTermId::new(u32::try_from(index).expect("tf-idf dictionary fits in u32")),
            )
        })
        .collect()
}

fn configured_idf_units(document_count: u32, document_frequency: u32, idf_scale: u32) -> u32 {
    if document_count == 0 || document_frequency == 0 {
        return idf_scale.saturating_mul(2);
    }
    let bounded_frequency = document_frequency.min(document_count);
    idf_scale + ((document_count - bounded_frequency) * idf_scale / document_count)
}

fn build_compat_row(
    surface_id: &str,
    terms: &[&str],
    dictionary: &[TermEntry],
    config: &TfidfConfig,
) -> SparseTfidfIndexRow {
    let dictionary_by_term = dictionary
        .iter()
        .map(|entry| (entry.term.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut counts = BTreeMap::<u32, (&TermEntry, u32)>::new();
    for term in terms {
        if let Some(entry) = dictionary_by_term.get(term) {
            counts
                .entry(entry.term_id)
                .and_modify(|(_, count)| *count += 1)
                .or_insert((*entry, 1));
        }
    }

    let row_terms = counts
        .into_values()
        .map(|(entry, raw_count)| {
            let tf_units = raw_count.min(config.tf_cap);
            SparseTfidfWeight {
                term_id: entry.term_id,
                term: entry.term.clone(),
                tf_units,
                idf_units: entry.idf_units,
                weight_units: tf_units.saturating_mul(entry.idf_units),
            }
        })
        .collect::<Vec<_>>();
    let norm_units = compat_norm_units(&row_terms);

    SparseTfidfIndexRow {
        surface_id: surface_id.to_string(),
        terms: row_terms,
        norm_units,
    }
}

fn build_compat_postings(
    rows: &[SparseTfidfIndexRow],
    term_count: usize,
) -> (Vec<SparseTfidfPosting>, Vec<usize>) {
    let mut by_term = vec![Vec::<SparseTfidfPosting>::new(); term_count];
    for (surface_ordinal, row) in rows.iter().enumerate() {
        for term in &row.terms {
            by_term[term.term_id as usize].push(SparseTfidfPosting {
                term_id: term.term_id,
                surface_ordinal: u32::try_from(surface_ordinal)
                    .expect("tf-idf surface ordinal fits in u32"),
                weight_units: term.weight_units,
            });
        }
    }

    let mut postings = Vec::new();
    let mut term_offsets = Vec::with_capacity(term_count + 1);
    term_offsets.push(0);
    for mut term_postings in by_term {
        term_postings.sort_by_key(|posting| posting.surface_ordinal);
        postings.extend(term_postings);
        term_offsets.push(postings.len());
    }

    (postings, term_offsets)
}

fn compat_norm_units(row_terms: &[SparseTfidfWeight]) -> u64 {
    let squared_sum = row_terms.iter().fold(0_u128, |sum, term| {
        let weight = u128::from(term.weight_units);
        sum + (weight * weight)
    });
    u64::try_from(integer_sqrt(squared_sum)).unwrap_or(u64::MAX)
}

fn compat_cosine_score_units(
    left: &SparseTfidfIndexRow,
    right: &SparseTfidfIndexRow,
    dot_units: u128,
    score_cap: u16,
) -> u16 {
    if left.norm_units == 0 || right.norm_units == 0 || dot_units == 0 {
        return 0;
    }
    let denominator = u128::from(left.norm_units) * u128::from(right.norm_units);
    if denominator == 0 {
        return 0;
    }
    let score = dot_units.saturating_mul(u128::from(score_cap)) / denominator;
    u16::try_from(score.min(u128::from(score_cap))).unwrap_or(score_cap)
}

fn build_row(
    surface: &TfidfInputSurface,
    dictionary: &BTreeMap<TfidfTermKey, TfidfTermId>,
    terms: &[TfidfTerm],
) -> TfidfSparseRow {
    let mut counts = BTreeMap::<TfidfTermId, u32>::new();
    for feature in &surface.features {
        if let Some(term_id) = dictionary.get(feature) {
            *counts.entry(*term_id).or_default() += 1;
        }
    }

    let row_terms = counts
        .into_iter()
        .map(|(term_id, raw_count)| {
            let tf_units = tf_units(raw_count);
            let idf_units = terms[term_id.as_index()].idf_units;
            TfidfRowTerm {
                term_id,
                tf_units,
                idf_units,
                weight_units: u64::from(tf_units) * u64::from(idf_units),
            }
        })
        .collect::<Vec<_>>();
    let norm_units = norm_units(&row_terms);

    TfidfSparseRow {
        surface_id: surface.surface_id.clone(),
        normalized_key: surface.normalized_key.clone(),
        terms: row_terms,
        norm_units,
    }
}

fn build_postings(
    rows: &[TfidfSparseRow],
    term_count: usize,
) -> (Vec<TfidfPosting>, Vec<TfidfPostingSlice>) {
    let mut by_term = vec![Vec::<TfidfPosting>::new(); term_count];
    for (surface_ordinal, row) in rows.iter().enumerate() {
        for term in &row.terms {
            by_term[term.term_id.as_index()].push(TfidfPosting {
                term_id: term.term_id,
                surface_ordinal: u32::try_from(surface_ordinal)
                    .expect("tf-idf surface ordinal fits in u32"),
                weight_units: term.weight_units,
            });
        }
    }

    let mut postings = Vec::new();
    let mut posting_slices = Vec::with_capacity(term_count);
    for (term_index, mut term_postings) in by_term.into_iter().enumerate() {
        term_postings.sort_by_key(|posting| posting.surface_ordinal);
        let start = postings.len();
        postings.extend(term_postings);
        let end = postings.len();
        posting_slices.push(TfidfPostingSlice {
            term_id: TfidfTermId::new(u32::try_from(term_index).expect("term index fits in u32")),
            start,
            end,
        });
    }

    (postings, posting_slices)
}

fn norm_units(row_terms: &[TfidfRowTerm]) -> u64 {
    let squared_sum = row_terms.iter().fold(0_u128, |sum, term| {
        let weight = u128::from(term.weight_units);
        sum + (weight * weight)
    });
    u64::try_from(integer_sqrt(squared_sum)).unwrap_or(u64::MAX)
}

fn cosine_score_units(left: &TfidfSparseRow, right: &TfidfSparseRow, dot_units: u128) -> u16 {
    if left.norm_units == 0 || right.norm_units == 0 || dot_units == 0 {
        return 0;
    }
    let denominator = u128::from(left.norm_units) * u128::from(right.norm_units);
    if denominator == 0 {
        return 0;
    }
    let score = dot_units.saturating_mul(u128::from(NAMEKIT_SCORE_SCALE)) / denominator;
    u16::try_from(score.min(u128::from(NAMEKIT_SCORE_SCALE))).unwrap_or(NAMEKIT_SCORE_SCALE)
}

fn integer_sqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut left = 1_u128;
    let mut right = value / 2 + 1;
    let mut answer = 1_u128;
    while left <= right {
        let mid = left + ((right - left) / 2);
        if mid <= value / mid {
            answer = mid;
            left = mid + 1;
        } else {
            right = mid - 1;
        }
    }
    answer
}

fn sort_topk_candidates(candidates: &mut [TfidfTopKCandidate]) {
    candidates.sort_by(|left, right| {
        right
            .score_units
            .cmp(&left.score_units)
            .then_with(|| left.evidence_class.cmp(&right.evidence_class))
            .then_with(|| {
                left.normalized_key
                    .as_bytes()
                    .cmp(right.normalized_key.as_bytes())
            })
            .then_with(|| left.surface_id.cmp(&right.surface_id))
    });
}

fn same_first_token_rank(pair: &SortedNeighborhoodPair) -> u8 {
    if first_token(&pair.left_key) == first_token(&pair.right_key) {
        0
    } else {
        1
    }
}

fn first_token(value: &str) -> Option<&str> {
    value.split_whitespace().next()
}

fn ordered_pair_ids(
    left: &SortedNeighborhoodInput,
    right: &SortedNeighborhoodInput,
) -> (String, String) {
    if left.surface_id <= right.surface_id {
        (left.surface_id.clone(), right.surface_id.clone())
    } else {
        (right.surface_id.clone(), left.surface_id.clone())
    }
}
