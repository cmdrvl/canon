//! Deterministic top-k candidate pruning for entity blocking.
//!
//! This module is intentionally stage-local: it does not generate candidates
//! or write block artifacts. It owns the stable ordering and diagnostics that
//! later block integration uses before emitting bounded candidate records.

use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{BTreeSet, BinaryHeap},
};

pub const CANON_ENTITY_TOPK_VERSION: &str = "canon_entity_topk.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopKConfig {
    pub profile_id: String,
    pub operator_id: String,
    pub k: usize,
    pub candidate_cap: Option<usize>,
    pub score_floor_units: Option<u32>,
}

impl TopKConfig {
    pub fn new(profile_id: impl Into<String>, operator_id: impl Into<String>, k: usize) -> Self {
        Self {
            profile_id: profile_id.into(),
            operator_id: operator_id.into(),
            k,
            candidate_cap: None,
            score_floor_units: None,
        }
    }

    pub const fn with_candidate_cap(mut self, candidate_cap: usize) -> Self {
        self.candidate_cap = Some(candidate_cap);
        self
    }

    pub const fn with_score_floor_units(mut self, score_floor_units: u32) -> Self {
        self.score_floor_units = Some(score_floor_units);
        self
    }

    fn effective_limit(&self) -> usize {
        self.candidate_cap.map_or(self.k, |cap| self.k.min(cap))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopKCandidateInput {
    pub query_surface_id: String,
    pub candidate_surface_id: String,
    pub normalized_key: String,
    pub score_units: u32,
}

impl TopKCandidateInput {
    pub fn new(
        query_surface_id: impl Into<String>,
        candidate_surface_id: impl Into<String>,
        normalized_key: impl Into<String>,
        score_units: u32,
    ) -> Self {
        Self {
            query_surface_id: query_surface_id.into(),
            candidate_surface_id: candidate_surface_id.into(),
            normalized_key: normalized_key.into(),
            score_units,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopKCandidate {
    pub rank: usize,
    pub query_surface_id: String,
    pub candidate_surface_id: String,
    pub normalized_key: String,
    pub score_units: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopKDropReason {
    BelowScoreFloor,
    CandidateCap,
    TopKLimit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopKDropDiagnostic {
    pub query_surface_id: String,
    pub candidate_surface_id: String,
    pub normalized_key: String,
    pub score_units: u32,
    pub reason: TopKDropReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopKDiagnostics {
    pub version: String,
    pub profile_id: String,
    pub operator_id: String,
    pub k: usize,
    pub candidate_cap: Option<usize>,
    pub score_floor_units: Option<u32>,
    pub input_candidate_count: usize,
    pub eligible_candidate_count: usize,
    pub emitted_candidate_count: usize,
    pub dropped_candidate_count: usize,
    pub dropped_by_score_floor_count: usize,
    pub dropped_by_candidate_cap_count: usize,
    pub dropped_by_topk_count: usize,
    pub candidate_cap_exceeded: bool,
    pub topk_exceeded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopKPruneResult {
    pub candidates: Vec<TopKCandidate>,
    pub dropped: Vec<TopKDropDiagnostic>,
    pub diagnostics: TopKDiagnostics,
}

#[derive(Debug, Clone)]
pub struct StableTopKHeap {
    config: TopKConfig,
    heap: BinaryHeap<HeapEntry>,
    eligible: Vec<EnumeratedCandidate>,
    below_floor_drops: Vec<TopKDropDiagnostic>,
    input_candidate_count: usize,
    next_ordinal: usize,
}

impl StableTopKHeap {
    pub fn new(config: TopKConfig) -> Self {
        Self {
            config,
            heap: BinaryHeap::new(),
            eligible: Vec::new(),
            below_floor_drops: Vec::new(),
            input_candidate_count: 0,
            next_ordinal: 0,
        }
    }

    pub fn push(&mut self, candidate: TopKCandidateInput) {
        self.input_candidate_count += 1;
        let ordinal = self.next_ordinal;
        self.next_ordinal += 1;

        if self
            .config
            .score_floor_units
            .is_some_and(|floor| candidate.score_units < floor)
        {
            self.below_floor_drops.push(TopKDropDiagnostic::new(
                &candidate,
                TopKDropReason::BelowScoreFloor,
            ));
            return;
        }

        let enumerated = EnumeratedCandidate { ordinal, candidate };
        if self.config.effective_limit() > 0 {
            self.heap.push(HeapEntry {
                candidate: enumerated.clone(),
            });
            if self.heap.len() > self.config.effective_limit() {
                self.heap.pop();
            }
        }
        self.eligible.push(enumerated);
    }

    pub fn extend<I>(&mut self, candidates: I)
    where
        I: IntoIterator<Item = TopKCandidateInput>,
    {
        for candidate in candidates {
            self.push(candidate);
        }
    }

    pub fn finish(self) -> TopKPruneResult {
        let StableTopKHeap {
            config,
            heap,
            eligible,
            mut below_floor_drops,
            input_candidate_count,
            next_ordinal: _,
        } = self;

        let mut kept = heap
            .into_vec()
            .into_iter()
            .map(|entry| entry.candidate)
            .collect::<Vec<_>>();
        kept.sort_by(enumerated_output_cmp);

        let kept_ordinals = kept
            .iter()
            .map(|candidate| candidate.ordinal)
            .collect::<BTreeSet<_>>();
        let candidates = kept
            .into_iter()
            .enumerate()
            .map(|(index, candidate)| TopKCandidate::from_enumerated(index + 1, candidate))
            .collect::<Vec<_>>();

        let mut eligible_sorted = eligible;
        eligible_sorted.sort_by(enumerated_output_cmp);

        let mut dropped_by_candidate_cap_count = 0;
        let mut dropped_by_topk_count = 0;
        let mut dropped = Vec::new();
        for (rank_index, candidate) in eligible_sorted.iter().enumerate() {
            if kept_ordinals.contains(&candidate.ordinal) {
                continue;
            }
            let reason = if config.candidate_cap.is_some_and(|cap| rank_index >= cap) {
                dropped_by_candidate_cap_count += 1;
                TopKDropReason::CandidateCap
            } else {
                dropped_by_topk_count += 1;
                TopKDropReason::TopKLimit
            };
            dropped.push(TopKDropDiagnostic::new(&candidate.candidate, reason));
        }

        below_floor_drops.sort_by(drop_diagnostic_cmp);
        dropped.sort_by(drop_diagnostic_cmp);
        let dropped_by_score_floor_count = below_floor_drops.len();
        let eligible_candidate_count = eligible_sorted.len();
        dropped.splice(0..0, below_floor_drops);
        dropped.sort_by(drop_diagnostic_cmp);

        let emitted_candidate_count = candidates.len();
        let dropped_candidate_count = input_candidate_count.saturating_sub(emitted_candidate_count);
        let candidate_cap_exceeded = config
            .candidate_cap
            .is_some_and(|cap| eligible_candidate_count > cap);
        let topk_exceeded = config
            .candidate_cap
            .map_or(eligible_candidate_count, |cap| {
                eligible_candidate_count.min(cap)
            })
            > config.k;

        TopKPruneResult {
            candidates,
            dropped,
            diagnostics: TopKDiagnostics {
                version: CANON_ENTITY_TOPK_VERSION.to_string(),
                profile_id: config.profile_id,
                operator_id: config.operator_id,
                k: config.k,
                candidate_cap: config.candidate_cap,
                score_floor_units: config.score_floor_units,
                input_candidate_count,
                eligible_candidate_count,
                emitted_candidate_count,
                dropped_candidate_count,
                dropped_by_score_floor_count,
                dropped_by_candidate_cap_count,
                dropped_by_topk_count,
                candidate_cap_exceeded,
                topk_exceeded,
            },
        }
    }
}

pub fn prune_top_k_candidates<I>(config: TopKConfig, candidates: I) -> TopKPruneResult
where
    I: IntoIterator<Item = TopKCandidateInput>,
{
    let mut heap = StableTopKHeap::new(config);
    heap.extend(candidates);
    heap.finish()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumeratedCandidate {
    ordinal: usize,
    candidate: TopKCandidateInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HeapEntry {
    candidate: EnumeratedCandidate,
}

impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        enumerated_output_cmp(&self.candidate, &other.candidate)
    }
}

impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl TopKCandidate {
    fn from_enumerated(rank: usize, candidate: EnumeratedCandidate) -> Self {
        Self {
            rank,
            query_surface_id: candidate.candidate.query_surface_id,
            candidate_surface_id: candidate.candidate.candidate_surface_id,
            normalized_key: candidate.candidate.normalized_key,
            score_units: candidate.candidate.score_units,
        }
    }
}

impl TopKDropDiagnostic {
    fn new(candidate: &TopKCandidateInput, reason: TopKDropReason) -> Self {
        Self {
            query_surface_id: candidate.query_surface_id.clone(),
            candidate_surface_id: candidate.candidate_surface_id.clone(),
            normalized_key: candidate.normalized_key.clone(),
            score_units: candidate.score_units,
            reason,
        }
    }
}

fn enumerated_output_cmp(left: &EnumeratedCandidate, right: &EnumeratedCandidate) -> Ordering {
    candidate_output_cmp(&left.candidate, &right.candidate)
        .then_with(|| left.ordinal.cmp(&right.ordinal))
}

fn candidate_output_cmp(left: &TopKCandidateInput, right: &TopKCandidateInput) -> Ordering {
    right
        .score_units
        .cmp(&left.score_units)
        .then_with(|| {
            left.normalized_key
                .as_bytes()
                .cmp(right.normalized_key.as_bytes())
        })
        .then_with(|| left.candidate_surface_id.cmp(&right.candidate_surface_id))
        .then_with(|| left.query_surface_id.cmp(&right.query_surface_id))
}

fn drop_diagnostic_cmp(left: &TopKDropDiagnostic, right: &TopKDropDiagnostic) -> Ordering {
    left.reason
        .cmp(&right.reason)
        .then_with(|| right.score_units.cmp(&left.score_units))
        .then_with(|| {
            left.normalized_key
                .as_bytes()
                .cmp(right.normalized_key.as_bytes())
        })
        .then_with(|| left.candidate_surface_id.cmp(&right.candidate_surface_id))
        .then_with(|| left.query_surface_id.cmp(&right.query_surface_id))
}
