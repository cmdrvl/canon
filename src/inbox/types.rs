use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fmt};

pub const CANON_UNRESOLVED_INBOX_VERSION: &str = "canon.unresolved.inbox.v1";

pub type InboxResult<T> = Result<T, InboxError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InboxErrorCode {
    ArtifactContract,
    PrivacyPolicy,
    CorruptReference,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxError {
    pub code: InboxErrorCode,
    pub message: String,
}

impl InboxError {
    pub fn new(code: InboxErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for InboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for InboxError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InboxEventKind {
    #[default]
    ExactLookup,
    ClusterAbstention,
    LinkAbstention,
    CandidateRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InboxReasonCode {
    #[default]
    NoMatchingRule,
    EmptyValue,
    MissingField,
    NullValue,
    NonScalarValue,
    AmbiguousCandidates,
    ScoreBelowThreshold,
    BudgetExceeded,
    CannotLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InboxFieldRole {
    #[default]
    LookupInput,
    NameField,
    AnchorField,
    ContextField,
    CandidatePair,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    #[default]
    Public,
    Internal,
    Restricted,
    Secret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RawValueRetention {
    #[default]
    Omit,
    ExternalReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InboxExportMode {
    #[default]
    Redacted,
    Retained,
    FingerprintsOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InboxMergeMode {
    #[default]
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CandidateStatus {
    #[default]
    None,
    Ambiguous,
    Rejected,
    BudgetLimited,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InboxPrivacyPolicy {
    pub policy_id: String,
    #[serde(default)]
    pub raw_value_retention: RawValueRetention,
    #[serde(default)]
    pub default_export_mode: InboxExportMode,
    #[serde(default)]
    pub merge_mode: InboxMergeMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProfileFieldRef {
    pub profile_id: String,
    pub profile_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NormalizedSurfaceFingerprint {
    pub normalizer_id: String,
    pub surface_role: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NamespaceHint {
    pub namespace: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CandidateSummary {
    #[serde(default)]
    pub status: CandidateStatus,
    #[serde(default)]
    pub candidate_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_score_band: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejection_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TemporalScope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InboxOccurrenceRef {
    pub project_ref: String,
    pub run_ref: String,
    pub source_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_ref: Option<String>,
    pub seen_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OccurrenceSummary {
    #[serde(default)]
    pub total_occurrences: u64,
    #[serde(default)]
    pub distinct_projects: u64,
    #[serde(default)]
    pub distinct_runs: u64,
    #[serde(default)]
    pub distinct_sources: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExternalRawValueReference {
    pub store: String,
    pub locator: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UnresolvedInboxItem {
    pub event_key: String,
    pub event_kind: InboxEventKind,
    pub reason_code: InboxReasonCode,
    pub field_name: String,
    pub field_role: InboxFieldRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_ref: Option<ProfileFieldRef>,
    #[serde(default)]
    pub surface_fingerprints: Vec<NormalizedSurfaceFingerprint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub namespace_hints: Vec<NamespaceHint>,
    #[serde(default)]
    pub candidate_summary: CandidateSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporal_scope: Option<TemporalScope>,
    pub first_seen_at: String,
    pub last_seen_at: String,
    #[serde(default)]
    pub occurrence_summary: OccurrenceSummary,
    #[serde(default)]
    pub occurrences: Vec<InboxOccurrenceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy_class: Option<PrivacyClass>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub raw_values_redacted: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_values: Vec<ExternalRawValueReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InboxSummary {
    #[serde(default)]
    pub total_items: u64,
    #[serde(default)]
    pub total_occurrences: u64,
    #[serde(default)]
    pub redacted_items: u64,
    #[serde(default)]
    pub retained_raw_reference_count: u64,
    #[serde(default)]
    pub by_reason_code: BTreeMap<String, u64>,
    #[serde(default)]
    pub by_event_kind: BTreeMap<String, u64>,
    #[serde(default)]
    pub by_privacy_class: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UnresolvedInboxArtifact {
    pub version: String,
    pub view: InboxExportMode,
    pub artifact_content_hash: String,
    pub policy: InboxPrivacyPolicy,
    pub summary: InboxSummary,
    #[serde(default)]
    pub items: Vec<UnresolvedInboxItem>,
}
