#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CANON_EVIDENCE_VERSION: &str = "canon.evidence.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Observation,
    CandidateScope,
    PairSupport,
    HyperedgeSupport,
    RecordLinkSupport,
    ContextOnly,
    ContextualNegative,
    Missingness,
    #[default]
    AntiMergeVeto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceAuthorityBasis {
    #[default]
    ReviewedConstraint,
    AuthoritativeIncompatibility,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target_kind", rename_all = "snake_case")]
pub enum EvidenceTarget {
    Observation {
        observation_id: String,
        surface: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subject_hint: Option<String>,
    },
    CandidateScope {
        scope_id: String,
        candidate_ids: Vec<String>,
    },
    Pair {
        left_id: String,
        right_id: String,
    },
    Hyperedge {
        member_ids: Vec<String>,
    },
    RecordLink {
        left_source: String,
        left_record_id: String,
        right_source: String,
        right_record_id: String,
    },
}

impl Default for EvidenceTarget {
    fn default() -> Self {
        Self::Observation {
            observation_id: String::new(),
            surface: String::new(),
            subject_hint: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvidenceOperatorRef {
    pub namespace: String,
    pub operator_id: String,
    pub operator_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvidencePolicyRef {
    pub policy_id: String,
    pub policy_version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvidenceScope {
    pub scope_type: String,
    pub scope_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvidenceTemporalScope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvidenceProvenanceRef {
    pub source_type: String,
    pub source_id: String,
    pub locator: String,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvidenceMeasurement {
    Numeric(EvidenceNumericMeasurement),
    Categorical(EvidenceCategoricalMeasurement),
    Boolean(EvidenceBooleanMeasurement),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvidenceNumericMeasurement {
    pub feature_id: String,
    pub units: String,
    pub scaled_value: i64,
    pub scale: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvidenceCategoricalMeasurement {
    pub feature_id: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvidenceBooleanMeasurement {
    pub feature_id: String,
    pub value: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvidenceExtension {
    pub namespace: String,
    pub schema_ref: String,
    #[serde(default)]
    pub payload: BTreeMap<String, EvidenceExtensionValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EvidenceExtensionValue {
    Bool(bool),
    Int(i64),
    UInt(u64),
    String(String),
    List(Vec<EvidenceExtensionValue>),
    Object(BTreeMap<String, EvidenceExtensionValue>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvidenceRecord {
    pub version: String,
    pub evidence_id: String,
    pub kind: EvidenceKind,
    pub target: EvidenceTarget,
    pub operator: EvidenceOperatorRef,
    pub reason_code: String,
    pub policy: EvidencePolicyRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_basis: Option<EvidenceAuthorityBasis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<EvidenceScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporal_scope: Option<EvidenceTemporalScope>,
    #[serde(default)]
    pub provenance: Vec<EvidenceProvenanceRef>,
    #[serde(default)]
    pub measurements: Vec<EvidenceMeasurement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<EvidenceExtension>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvidenceBundle {
    pub version: String,
    pub record_count: u64,
    pub content_hash: String,
    #[serde(default)]
    pub records: Vec<EvidenceRecord>,
}
