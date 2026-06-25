//! Stable namekit reason-code and explain payload contracts.
//!
//! Reason ordering is canonical and independent of the order in which later
//! normalization/scoring stages discover evidence: sort by `ReasonCode::ALL`,
//! then stage, source, detail, and summary. This keeps review CSVs, explain
//! artifacts, and golden tests byte-stable.

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{collections::BTreeMap, fmt};

pub const NAMEKIT_EXPLAIN_VERSION: &str = "canon_namekit_explain.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReasonCode {
    NoLoss,
    UnicodeFolded,
    PunctuationRemoved,
    ControlRemoved,
    WhitespaceCollapsed,
    LegalSuffixStripped,
    LegalSuffixPreserved,
    TokensSorted,
    TokensDeduped,
    NgramFingerprintCollision,
    CommonTokenDownweighted,
    RareTokenSupport,
    MetricCutoff,
    ProtectedTokenConflict,
    ProfileTokenPreserved,
    ProfileTokenDropped,
    SourceParityReference,
}

impl ReasonCode {
    pub const ALL: &'static [ReasonCode] = &[
        ReasonCode::NoLoss,
        ReasonCode::UnicodeFolded,
        ReasonCode::PunctuationRemoved,
        ReasonCode::ControlRemoved,
        ReasonCode::WhitespaceCollapsed,
        ReasonCode::LegalSuffixStripped,
        ReasonCode::LegalSuffixPreserved,
        ReasonCode::TokensSorted,
        ReasonCode::TokensDeduped,
        ReasonCode::NgramFingerprintCollision,
        ReasonCode::CommonTokenDownweighted,
        ReasonCode::RareTokenSupport,
        ReasonCode::MetricCutoff,
        ReasonCode::ProtectedTokenConflict,
        ReasonCode::ProfileTokenPreserved,
        ReasonCode::ProfileTokenDropped,
        ReasonCode::SourceParityReference,
    ];

    pub const LOSSY: &'static [ReasonCode] = &[
        ReasonCode::UnicodeFolded,
        ReasonCode::PunctuationRemoved,
        ReasonCode::ControlRemoved,
        ReasonCode::WhitespaceCollapsed,
        ReasonCode::LegalSuffixStripped,
        ReasonCode::TokensSorted,
        ReasonCode::TokensDeduped,
        ReasonCode::NgramFingerprintCollision,
        ReasonCode::ProfileTokenDropped,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ReasonCode::NoLoss => "no_loss",
            ReasonCode::UnicodeFolded => "unicode_folded",
            ReasonCode::PunctuationRemoved => "punctuation_removed",
            ReasonCode::ControlRemoved => "control_removed",
            ReasonCode::WhitespaceCollapsed => "whitespace_collapsed",
            ReasonCode::LegalSuffixStripped => "legal_suffix_stripped",
            ReasonCode::LegalSuffixPreserved => "legal_suffix_preserved",
            ReasonCode::TokensSorted => "tokens_sorted",
            ReasonCode::TokensDeduped => "tokens_deduped",
            ReasonCode::NgramFingerprintCollision => "ngram_fingerprint_collision",
            ReasonCode::CommonTokenDownweighted => "common_token_downweighted",
            ReasonCode::RareTokenSupport => "rare_token_support",
            ReasonCode::MetricCutoff => "metric_cutoff",
            ReasonCode::ProtectedTokenConflict => "protected_token_conflict",
            ReasonCode::ProfileTokenPreserved => "profile_token_preserved",
            ReasonCode::ProfileTokenDropped => "profile_token_dropped",
            ReasonCode::SourceParityReference => "source_parity_reference",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            ReasonCode::NoLoss => "view kept the source text without lossy transformation",
            ReasonCode::UnicodeFolded => {
                "unicode characters were folded into a deterministic comparison form"
            }
            ReasonCode::PunctuationRemoved => {
                "punctuation was removed or folded during normalization"
            }
            ReasonCode::ControlRemoved => "control characters were removed during normalization",
            ReasonCode::WhitespaceCollapsed => {
                "whitespace was collapsed into the canonical separator"
            }
            ReasonCode::LegalSuffixStripped => "legal-form suffix text was stripped for this view",
            ReasonCode::LegalSuffixPreserved => {
                "legal-form suffix text was preserved by profile policy"
            }
            ReasonCode::TokensSorted => "token order was canonicalized for this view",
            ReasonCode::TokensDeduped => "duplicate tokens were removed for this view",
            ReasonCode::NgramFingerprintCollision => {
                "different source strings share the same ngram fingerprint"
            }
            ReasonCode::CommonTokenDownweighted => {
                "a common token contributed reduced evidence weight"
            }
            ReasonCode::RareTokenSupport => "a rare token contributed positive support evidence",
            ReasonCode::MetricCutoff => "a string metric score was below the configured cutoff",
            ReasonCode::ProtectedTokenConflict => {
                "protected tokens conflict and must not support an auto-merge"
            }
            ReasonCode::ProfileTokenPreserved => {
                "profile policy preserved a token that another view might drop"
            }
            ReasonCode::ProfileTokenDropped => "profile policy dropped a token from this view",
            ReasonCode::SourceParityReference => {
                "reason traces the upstream technique used for parity"
            }
        }
    }

    pub fn is_lossy(self) -> bool {
        Self::LOSSY.contains(&self)
    }

    pub fn order(self) -> usize {
        Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .expect("reason code must be in canonical order table")
    }

    pub fn merge_evidence_role(self) -> MergeEvidenceRole {
        match self {
            ReasonCode::RareTokenSupport => MergeEvidenceRole::Support,
            ReasonCode::ProtectedTokenConflict => MergeEvidenceRole::CannotLink,
            ReasonCode::CommonTokenDownweighted
            | ReasonCode::LegalSuffixPreserved
            | ReasonCode::MetricCutoff
            | ReasonCode::NgramFingerprintCollision
            | ReasonCode::NoLoss
            | ReasonCode::ProfileTokenPreserved
            | ReasonCode::SourceParityReference => MergeEvidenceRole::ReviewContext,
            ReasonCode::ControlRemoved
            | ReasonCode::LegalSuffixStripped
            | ReasonCode::ProfileTokenDropped
            | ReasonCode::PunctuationRemoved
            | ReasonCode::TokensDeduped
            | ReasonCode::TokensSorted
            | ReasonCode::UnicodeFolded
            | ReasonCode::WhitespaceCollapsed => MergeEvidenceRole::Transform,
        }
    }

    pub fn can_support_merge(self) -> bool {
        self.merge_evidence_role() == MergeEvidenceRole::Support
    }
}

impl TryFrom<&str> for ReasonCode {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::ALL
            .iter()
            .copied()
            .find(|code| code.as_str() == value)
            .ok_or_else(|| format!("unknown namekit reason code: {value}"))
    }
}

impl Serialize for ReasonCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ReasonCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        ReasonCode::try_from(value.as_str()).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonStage {
    Normalize,
    LegalSuffix,
    Tokenize,
    Fingerprint,
    Tfidf,
    Similarity,
    ProtectedToken,
    ProfilePolicy,
    SourceParity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceTechnique {
    CanonProfile,
    Cleanco,
    IngEntityMatchingModel,
    Normality,
    NomenklaturaResolver,
    OpenSanctionsRigour,
    RapidFuzz,
    SparseDotTopn,
    SplinkTfAdjustment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeEvidenceRole {
    Transform,
    ReviewContext,
    Support,
    CannotLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtectedTokenLane {
    TenantProtectedBrand,
    RegulatedLegalIdentity,
    PlatformCategory,
    ProfileDistinctness,
}

impl ProtectedTokenLane {
    pub const fn as_str(self) -> &'static str {
        match self {
            ProtectedTokenLane::TenantProtectedBrand => "tenant_protected_brand",
            ProtectedTokenLane::RegulatedLegalIdentity => "regulated_legal_identity",
            ProtectedTokenLane::PlatformCategory => "platform_category",
            ProtectedTokenLane::ProfileDistinctness => "profile_distinctness",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewEvidenceCategory {
    NormalizationTransform,
    SupportEvidence,
    RiskEvidence,
    CannotLinkEvidence,
    ProfilePolicy,
    SourceProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewActionHint {
    ExplainOnly,
    ReviewCandidate,
    ReviewDistinctness,
    PreserveProfileSemantics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewPatchVocabulary {
    AliasCandidate,
    DistinctCandidate,
    RelationHint,
    ProtectedToken,
    NormalizationTrace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolverDecisionHint {
    SameCandidate,
    NotSameCandidate,
    Undecided,
    ContextOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewReasonMapping {
    pub reason_code: ReasonCode,
    pub evidence_category: ReviewEvidenceCategory,
    pub action_hint: ReviewActionHint,
    pub patch_vocabulary: Option<ReviewPatchVocabulary>,
    pub resolver_decision_hint: ResolverDecisionHint,
}

pub fn review_reason_mapping(reason_code: ReasonCode) -> ReviewReasonMapping {
    let (evidence_category, action_hint, patch_vocabulary, resolver_decision_hint) =
        match reason_code {
            ReasonCode::RareTokenSupport => (
                ReviewEvidenceCategory::SupportEvidence,
                ReviewActionHint::ReviewCandidate,
                Some(ReviewPatchVocabulary::AliasCandidate),
                ResolverDecisionHint::SameCandidate,
            ),
            ReasonCode::ProtectedTokenConflict => (
                ReviewEvidenceCategory::CannotLinkEvidence,
                ReviewActionHint::ReviewDistinctness,
                Some(ReviewPatchVocabulary::DistinctCandidate),
                ResolverDecisionHint::NotSameCandidate,
            ),
            ReasonCode::CommonTokenDownweighted
            | ReasonCode::MetricCutoff
            | ReasonCode::NgramFingerprintCollision => (
                ReviewEvidenceCategory::RiskEvidence,
                ReviewActionHint::ReviewCandidate,
                Some(ReviewPatchVocabulary::RelationHint),
                ResolverDecisionHint::Undecided,
            ),
            ReasonCode::LegalSuffixPreserved | ReasonCode::ProfileTokenPreserved => (
                ReviewEvidenceCategory::ProfilePolicy,
                ReviewActionHint::PreserveProfileSemantics,
                Some(ReviewPatchVocabulary::ProtectedToken),
                ResolverDecisionHint::ContextOnly,
            ),
            ReasonCode::SourceParityReference => (
                ReviewEvidenceCategory::SourceProvenance,
                ReviewActionHint::ExplainOnly,
                None,
                ResolverDecisionHint::ContextOnly,
            ),
            ReasonCode::ControlRemoved
            | ReasonCode::LegalSuffixStripped
            | ReasonCode::NoLoss
            | ReasonCode::ProfileTokenDropped
            | ReasonCode::PunctuationRemoved
            | ReasonCode::TokensDeduped
            | ReasonCode::TokensSorted
            | ReasonCode::UnicodeFolded
            | ReasonCode::WhitespaceCollapsed => (
                ReviewEvidenceCategory::NormalizationTransform,
                ReviewActionHint::ExplainOnly,
                Some(ReviewPatchVocabulary::NormalizationTrace),
                ResolverDecisionHint::ContextOnly,
            ),
        };

    ReviewReasonMapping {
        reason_code,
        evidence_category,
        action_hint,
        patch_vocabulary,
        resolver_decision_hint,
    }
}

pub fn review_reason_mappings() -> Vec<ReviewReasonMapping> {
    ReasonCode::ALL
        .iter()
        .copied()
        .map(review_reason_mapping)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamekitReason {
    pub code: ReasonCode,
    pub stage: ReasonStage,
    pub lossy: bool,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceTechnique>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub detail: BTreeMap<String, String>,
}

impl NamekitReason {
    pub fn new(code: ReasonCode, stage: ReasonStage) -> Self {
        Self {
            code,
            stage,
            lossy: code.is_lossy(),
            summary: code.summary().to_string(),
            source: None,
            detail: BTreeMap::new(),
        }
    }

    pub fn with_source(mut self, source: SourceTechnique) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.detail.insert(key.into(), value.into());
        self
    }

    pub fn merge_evidence_role(&self) -> MergeEvidenceRole {
        self.code.merge_evidence_role()
    }

    pub fn can_support_merge(&self) -> bool {
        self.code.can_support_merge()
    }
}

pub fn protected_token_conflict_reason(
    profile_id: impl Into<String>,
    lane: ProtectedTokenLane,
    left_token: impl Into<String>,
    right_token: impl Into<String>,
) -> NamekitReason {
    NamekitReason::new(
        ReasonCode::ProtectedTokenConflict,
        ReasonStage::ProtectedToken,
    )
    .with_source(SourceTechnique::CanonProfile)
    .with_detail("profile_id", profile_id)
    .with_detail("lane", lane.as_str())
    .with_detail("left_token", left_token)
    .with_detail("right_token", right_token)
}

pub fn protected_token_preserved_reason(
    profile_id: impl Into<String>,
    lane: ProtectedTokenLane,
    token: impl Into<String>,
) -> NamekitReason {
    NamekitReason::new(
        ReasonCode::ProfileTokenPreserved,
        ReasonStage::ProfilePolicy,
    )
    .with_source(SourceTechnique::CanonProfile)
    .with_detail("profile_id", profile_id)
    .with_detail("lane", lane.as_str())
    .with_detail("token", token)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamekitExplainTrace {
    pub version: String,
    pub profile_id: String,
    pub view: String,
    pub raw: String,
    pub normalized: String,
    pub lossy: bool,
    pub reasons: Vec<NamekitReason>,
}

impl NamekitExplainTrace {
    pub fn new(
        profile_id: impl Into<String>,
        view: impl Into<String>,
        raw: impl Into<String>,
        normalized: impl Into<String>,
        mut reasons: Vec<NamekitReason>,
    ) -> Self {
        sort_reasons(&mut reasons);
        let lossy = reasons.iter().any(|reason| reason.lossy);
        Self {
            version: NAMEKIT_EXPLAIN_VERSION.to_string(),
            profile_id: profile_id.into(),
            view: view.into(),
            raw: raw.into(),
            normalized: normalized.into(),
            lossy,
            reasons,
        }
    }
}

pub fn sort_reasons(reasons: &mut [NamekitReason]) {
    reasons.sort_by(|left, right| {
        left.code
            .order()
            .cmp(&right.code.order())
            .then_with(|| left.stage.cmp(&right.stage))
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.detail.cmp(&right.detail))
            .then_with(|| left.summary.cmp(&right.summary))
    });
}

impl fmt::Display for ReasonCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
