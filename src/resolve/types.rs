//! Shared cross-tape resolution contracts.
//!
//! These types are intentionally broad enough for strategy parsing, tape
//! loading, candidate filtering, scoring, output, gold scoring, and write-back
//! beads to work without redefining the public artifact shape.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, error::Error, fmt, path::PathBuf};

use crate::InputFormat;

pub const CANON_RESOLVE_VERSION: &str = "canon_resolve.v0";

pub type ResolveResult<T> = Result<T, ResolveError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResolveErrorCode {
    Io,
    Parse,
    Strategy,
    InputContract,
    Registry,
    TooLarge,
    TooManyCandidates,
    EmptyTape,
    IncompatibleTapes,
    Gold,
    WriteBack,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolveError {
    pub code: ResolveErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}

impl ResolveError {
    pub fn new(code: ResolveErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(code: ResolveErrorCode, message: impl Into<String>, detail: Value) -> Self {
        Self {
            code,
            message: message.into(),
            detail: Some(detail),
        }
    }

    pub fn unimplemented(area: &'static str) -> Self {
        Self::new(
            ResolveErrorCode::Unimplemented,
            format!("resolve scaffold placeholder for {area}"),
        )
    }
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for ResolveError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveRequest {
    pub reference_tape: PathBuf,
    pub target_tape: PathBuf,
    pub strategy: PathBuf,
    pub registry: PathBuf,
    pub gold: Option<PathBuf>,
    pub write_back: bool,
    pub max_candidates: Option<usize>,
    pub max_rows: Option<usize>,
    pub max_bytes: Option<u64>,
    pub no_witness: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StrategyReference {
    pub id: String,
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResolveRegistrySnapshot {
    pub id: String,
    pub version: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TapeSummary {
    pub path: String,
    pub record_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResolveSummary {
    pub target_records: usize,
    pub matched: usize,
    pub unmatched: usize,
    pub ambiguous: usize,
    pub match_rate: f64,
}

impl ResolveSummary {
    pub fn partition_holds(&self) -> bool {
        self.target_records == self.matched + self.unmatched + self.ambiguous
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TapeSide {
    Reference,
    Target,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolveRecord {
    pub side: TapeSide,
    pub composite_id: String,
    pub row_index: usize,
    pub attributes: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TapeLoadOptions {
    pub max_rows: Option<usize>,
    pub max_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedTape {
    pub side: TapeSide,
    pub path: String,
    pub format: InputFormat,
    pub delimiter: Option<u8>,
    pub records: Vec<ResolveRecord>,
}

impl LoadedTape {
    pub fn summary(&self) -> TapeSummary {
        TapeSummary {
            path: self.path.clone(),
            record_count: self.records.len(),
        }
    }

    pub fn records_sorted_by_id(&self) -> Vec<&ResolveRecord> {
        let mut records = self.records.iter().collect::<Vec<_>>();
        records.sort_by(|left, right| {
            left.composite_id
                .cmp(&right.composite_id)
                .then(left.row_index.cmp(&right.row_index))
        });
        records
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedTapes {
    pub reference: LoadedTape,
    pub target: LoadedTape,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResolveIdentity {
    pub reference: ResolveIdentitySide,
    pub target: ResolveIdentitySide,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResolveIdentitySide {
    #[serde(default)]
    pub id_columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResolveStrategy {
    #[serde(rename = "strategy_id")]
    pub id: String,
    #[serde(rename = "strategy_version")]
    pub version: String,
    pub entity_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub identity: ResolveIdentity,
    #[serde(default)]
    pub candidate_filter: Vec<ResolveOperatorSpec>,
    #[serde(default)]
    pub assertions: Vec<ResolveOperatorSpec>,
    pub match_threshold: f64,
    pub ambiguity_gap: f64,
    #[serde(default)]
    pub max_candidates: Option<usize>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content_hash: String,
}

impl ResolveStrategy {
    pub fn reference(&self) -> StrategyReference {
        StrategyReference {
            id: self.id.clone(),
            version: self.version.clone(),
            content_hash: self.content_hash.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResolveOperatorSpec {
    pub field_ref: String,
    pub field_tgt: String,
    pub op: String,
    #[serde(default)]
    pub weight: f64,
    #[serde(default)]
    pub required: bool,
    #[serde(flatten, default)]
    pub params: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AssertionResult {
    pub field_ref: String,
    pub field_tgt: String,
    pub op: String,
    pub passed: bool,
    pub score: f64,
    pub weight: f64,
    pub required: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub detail: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CandidateScore {
    pub reference_id: String,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<AssertionResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchRecord {
    pub reference_id: String,
    pub target_id: String,
    pub canonical_id: String,
    pub score: f64,
    #[serde(default)]
    pub assertions: Vec<AssertionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_up: Option<CandidateScore>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnmatchedRecord {
    pub target_id: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_candidate: Option<CandidateScore>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AmbiguousRecord {
    pub target_id: String,
    pub candidates: Vec<CandidateScore>,
    pub gap: f64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GoldScore {
    pub total: usize,
    pub correct: usize,
    pub incorrect: usize,
    pub unmatched_in_gold: usize,
    pub accuracy: f64,
    #[serde(default)]
    pub regressions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WriteBackSummary {
    pub requested: bool,
    pub written: bool,
    pub entry_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolveArtifact {
    pub version: String,
    pub strategy: StrategyReference,
    pub registry: ResolveRegistrySnapshot,
    pub reference_tape: TapeSummary,
    pub target_tape: TapeSummary,
    pub summary: ResolveSummary,
    #[serde(default)]
    pub matches: Vec<MatchRecord>,
    #[serde(default)]
    pub unmatched: Vec<UnmatchedRecord>,
    #[serde(default)]
    pub ambiguous: Vec<AmbiguousRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflict_warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gold_score: Option<GoldScore>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write_back: Option<WriteBackSummary>,
}

impl ResolveArtifact {
    pub fn exit_code(&self) -> u8 {
        if self.summary.unmatched == 0 && self.summary.ambiguous == 0 {
            0
        } else {
            1
        }
    }

    pub fn render_summary(&self) -> String {
        format!(
            "{} strategy={} registry={}@{} target_records={} matched={} unmatched={} ambiguous={}",
            self.version,
            self.strategy.id,
            self.registry.id,
            self.registry.version,
            self.summary.target_records,
            self.summary.matched,
            self.summary.unmatched,
            self.summary.ambiguous
        )
    }
}

impl Default for ResolveArtifact {
    fn default() -> Self {
        Self {
            version: CANON_RESOLVE_VERSION.to_string(),
            strategy: StrategyReference::default(),
            registry: ResolveRegistrySnapshot::default(),
            reference_tape: TapeSummary::default(),
            target_tape: TapeSummary::default(),
            summary: ResolveSummary::default(),
            matches: vec![],
            unmatched: vec![],
            ambiguous: vec![],
            conflict_warnings: vec![],
            gold_score: None,
            write_back: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_partition_tracks_target_records() {
        let summary = ResolveSummary {
            target_records: 12,
            matched: 9,
            unmatched: 1,
            ambiguous: 2,
            match_rate: 0.75,
        };

        assert!(summary.partition_holds());
    }

    #[test]
    fn artifact_exit_code_matches_unmatched_or_ambiguous_targets() {
        let mut artifact = ResolveArtifact {
            summary: ResolveSummary {
                target_records: 2,
                matched: 2,
                unmatched: 0,
                ambiguous: 0,
                match_rate: 1.0,
            },
            ..ResolveArtifact::default()
        };
        assert_eq!(artifact.exit_code(), 0);

        artifact.summary.unmatched = 1;
        artifact.summary.matched = 1;
        artifact.summary.match_rate = 0.5;
        assert_eq!(artifact.exit_code(), 1);
    }
}
