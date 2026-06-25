//! Deterministic entity-name primitives for `canon entity`.
//!
//! The initial boundary is intentionally contract-first: downstream namekit
//! implementation modules must emit the stable reason-code and explain payload
//! types from `explain` instead of inventing local strings.

use serde::{Deserialize, Serialize};

pub mod explain;
pub mod ids;
pub mod legal_suffix;
pub mod ngram;
pub mod normalize;
pub mod similarity;
pub mod tenant;
pub mod tfidf;
pub mod tokenize;

pub use explain::{
    NAMEKIT_EXPLAIN_VERSION, NamekitExplainTrace, NamekitReason, ReasonCode, ReasonStage,
    SourceTechnique, sort_reasons,
};

pub const NAMEKIT_VERSION: &str = "canon_namekit.v0";
pub const NAMEKIT_SCORE_SCALE: u16 = 10_000;

const NAMEKIT_CAPABILITIES: &[NamekitCapability] = &[
    NamekitCapability::Normalize,
    NamekitCapability::LegalSuffix,
    NamekitCapability::Tokenize,
    NamekitCapability::Ngram,
    NamekitCapability::Tfidf,
    NamekitCapability::Similarity,
    NamekitCapability::Patch,
    NamekitCapability::Explain,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamekitCapability {
    Normalize,
    LegalSuffix,
    Tokenize,
    Ngram,
    Tfidf,
    Similarity,
    Patch,
    Explain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamekitProfileSemantics {
    TenantLabel,
    #[serde(rename = "regab_firm_identity")]
    RegAbFirmIdentity,
    GenericEntityName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamekitViewKind {
    Raw,
    Normalized,
    LegalSuffix,
    Tokens,
    Ngrams,
    Tfidf,
    Similarity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TokenId(u32);

impl TokenId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NgramId(u32);

impl NgramId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SimilarityScore(u16);

impl SimilarityScore {
    pub const ZERO: Self = Self(0);
    pub const EXACT: Self = Self(NAMEKIT_SCORE_SCALE);

    pub fn from_scaled(value: u16) -> Option<Self> {
        (value <= NAMEKIT_SCORE_SCALE).then_some(Self(value))
    }

    pub const fn as_scaled(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamekitView {
    pub profile: NamekitProfileSemantics,
    pub kind: NamekitViewKind,
    pub value: String,
    pub lossy: bool,
    pub reasons: Vec<NamekitReason>,
}

impl NamekitView {
    pub fn new(
        profile: NamekitProfileSemantics,
        kind: NamekitViewKind,
        value: impl Into<String>,
        mut reasons: Vec<NamekitReason>,
    ) -> Self {
        sort_reasons(&mut reasons);
        let lossy = reasons.iter().any(|reason| reason.lossy);
        Self {
            profile,
            kind,
            value: value.into(),
            lossy,
            reasons,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamekitToken {
    pub id: Option<TokenId>,
    pub text: String,
}

impl NamekitToken {
    pub fn new(id: Option<TokenId>, text: impl Into<String>) -> Self {
        Self {
            id,
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamekitNgram {
    pub id: Option<NgramId>,
    pub text: String,
}

impl NamekitNgram {
    pub fn new(id: Option<NgramId>, text: impl Into<String>) -> Self {
        Self {
            id,
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamekitSimilarityEvidence {
    pub left_view: NamekitViewKind,
    pub right_view: NamekitViewKind,
    pub score: SimilarityScore,
    pub reasons: Vec<NamekitReason>,
}

impl NamekitSimilarityEvidence {
    pub fn new(
        left_view: NamekitViewKind,
        right_view: NamekitViewKind,
        score: SimilarityScore,
        mut reasons: Vec<NamekitReason>,
    ) -> Self {
        sort_reasons(&mut reasons);
        Self {
            left_view,
            right_view,
            score,
            reasons,
        }
    }
}

pub const fn namekit_capabilities() -> &'static [NamekitCapability] {
    NAMEKIT_CAPABILITIES
}
