use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

pub const CANON_IDENTITY_FACT_VERSION: &str = "canon.identity.fact.v1";

pub type TemporalResult<T> = Result<T, TemporalError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TemporalErrorCode {
    ArtifactContract,
    CorruptReference,
    LinkInvariant,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalError {
    pub code: TemporalErrorCode,
    pub message: String,
}

impl TemporalError {
    pub fn new(code: TemporalErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for TemporalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for TemporalError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IntervalBoundary {
    #[default]
    Inclusive,
    Exclusive,
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AssertionStatus {
    #[default]
    Asserted,
    Accepted,
    Disputed,
    Retracted,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TimeInterval {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_at: Option<String>,
    #[serde(default)]
    pub start_bound: IntervalBoundary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_at: Option<String>,
    #[serde(default)]
    pub end_bound: IntervalBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RecordedTime {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_at: Option<String>,
    #[serde(default)]
    pub start_bound: IntervalBoundary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_at: Option<String>,
    #[serde(default)]
    pub end_bound: IntervalBoundary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_seq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SourceLocator {
    pub source_system: String,
    pub locator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FactScope {
    pub scope_type: String,
    pub scope_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IdentityFact {
    pub version: String,
    pub fact_id: String,
    pub assertion_key: String,
    pub conflict_key: String,
    pub subject_id: String,
    pub predicate: String,
    pub object_id: String,
    pub valid_time: TimeInterval,
    pub recorded_time: RecordedTime,
    pub source_locator: SourceLocator,
    pub materialization_digest: String,
    pub assertion_status: AssertionStatus,
    pub trust_policy_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<FactScope>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supersedes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retracts: Vec<String>,
}
