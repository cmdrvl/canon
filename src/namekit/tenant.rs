//! CMBS tenant-label normalization composed from core namekit primitives.
//!
//! This module is profile-specific glue: it does not change core lookup or add
//! fuzzy semantics. It prepares deterministic tenant-label views and review
//! signals for later entity stages.

use crate::namekit::explain::{
    ProtectedTokenLane, protected_token_conflict_reason, protected_token_preserved_reason,
};
use crate::namekit::legal_suffix::{LegalSuffixProfile, analyze_legal_suffixes};
use crate::namekit::normalize::normalize_normality;
use crate::namekit::tokenize::tokenize_sorted_unique;
use crate::namekit::{NamekitReason, ReasonCode, ReasonStage, SourceTechnique, sort_reasons};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const CMBS_TENANT_NORMALIZATION_VERSION: &str = "canon_namekit_cmbs_tenant.v0";
pub const CMBS_TENANT_PROFILE_ID: &str = "cmbs_tenant_label";

const TENANT_NOISE_TOKENS: &[&str] = &[
    "store", "stores", "unit", "suite", "ste", "space", "tenant", "location", "no", "number",
];

const TENANT_LEGACY_ALIAS_TOKENS: &[&str] = &["roebuck"];

const TENANT_PROTECTED_DISTINCTION_TOKENS: &[&str] = &[
    "auto",
    "center",
    "kmart",
    "transform",
    "sr",
    "holdings",
    "capital",
    "management",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CmbsTenantNormalization {
    pub version: String,
    pub raw: String,
    pub tenant_core: String,
    pub tenant_brand: String,
    pub tenant_tokens: Vec<String>,
    pub stripped_legal_suffixes: Vec<String>,
    pub dropped_noise_tokens: Vec<String>,
    pub protected_tokens: Vec<String>,
    pub reasons: Vec<NamekitReason>,
}

impl CmbsTenantNormalization {
    pub fn reason_codes(&self) -> Vec<&'static str> {
        self.reasons
            .iter()
            .map(|reason| reason.code.as_str())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CmbsTenantPairEvidence {
    pub left: CmbsTenantNormalization,
    pub right: CmbsTenantNormalization,
    pub shared_tokens: Vec<String>,
    pub same_tenant_label_support: bool,
    pub requires_review: bool,
    pub reasons: Vec<NamekitReason>,
}

pub fn normalize_cmbs_tenant(raw: &str) -> CmbsTenantNormalization {
    let normalized = normalize_normality(raw);
    let suffix = analyze_legal_suffixes(
        normalized.normalized.as_str(),
        LegalSuffixProfile::CmbsTenantLabel,
    );
    let base = if suffix.basename.is_empty() {
        normalized.normalized.as_str()
    } else {
        suffix.basename.as_str()
    };

    let mut dropped_noise_tokens = Vec::new();
    let tenant_tokens = tenant_tokens(base, &mut dropped_noise_tokens);
    let tenant_core = tenant_tokens.join(" ");
    let protected_tokens = protected_tokens(&tenant_tokens);
    let tenant_brand = tenant_brand(&tenant_tokens, &protected_tokens);

    let mut reasons = normalized.reasons;
    reasons.extend(legal_suffix_reasons(&suffix.stripped_terms));
    reasons.extend(dropped_token_reasons(&dropped_noise_tokens));
    reasons.extend(protected_tokens.iter().map(|token| {
        protected_token_preserved_reason(
            CMBS_TENANT_PROFILE_ID,
            ProtectedTokenLane::TenantProtectedBrand,
            token,
        )
    }));
    reasons.extend(support_token_reasons(&tenant_tokens));
    sort_reasons(&mut reasons);

    CmbsTenantNormalization {
        version: CMBS_TENANT_NORMALIZATION_VERSION.to_string(),
        raw: raw.to_string(),
        tenant_core,
        tenant_brand,
        tenant_tokens,
        stripped_legal_suffixes: suffix.stripped_terms,
        dropped_noise_tokens,
        protected_tokens,
        reasons,
    }
}

pub fn cmbs_tenant_pair_evidence(left: &str, right: &str) -> CmbsTenantPairEvidence {
    let left = normalize_cmbs_tenant(left);
    let right = normalize_cmbs_tenant(right);
    let shared_tokens = shared_tokens(&left.tenant_tokens, &right.tenant_tokens);
    let protected_conflict = !shared_tokens.is_empty()
        && left.protected_tokens != right.protected_tokens
        && (!left.protected_tokens.is_empty() || !right.protected_tokens.is_empty());
    let same_tenant_label_support = !protected_conflict
        && !left.tenant_core.is_empty()
        && left.tenant_core == right.tenant_core;
    let requires_review =
        protected_conflict || (!same_tenant_label_support && !shared_tokens.is_empty());

    let mut reasons = Vec::new();
    if same_tenant_label_support {
        reasons.extend(support_token_reasons(&shared_tokens));
    }
    if protected_conflict {
        reasons.push(protected_token_conflict_reason(
            CMBS_TENANT_PROFILE_ID,
            ProtectedTokenLane::TenantProtectedBrand,
            left.tenant_brand.as_str(),
            right.tenant_brand.as_str(),
        ));
    }
    sort_reasons(&mut reasons);

    CmbsTenantPairEvidence {
        left,
        right,
        shared_tokens,
        same_tenant_label_support,
        requires_review,
        reasons,
    }
}

fn tenant_tokens(base: &str, dropped: &mut Vec<String>) -> Vec<String> {
    let tokenization = tokenize_sorted_unique(base);
    tokenization
        .tokens
        .into_iter()
        .map(|token| token.text)
        .filter(|token| {
            if is_noise_token(token) {
                dropped.push(token.clone());
                false
            } else {
                true
            }
        })
        .collect()
}

fn is_noise_token(token: &str) -> bool {
    token.chars().all(|ch| ch.is_ascii_digit())
        || TENANT_NOISE_TOKENS.contains(&token)
        || TENANT_LEGACY_ALIAS_TOKENS.contains(&token)
}

fn protected_tokens(tokens: &[String]) -> Vec<String> {
    tokens
        .iter()
        .filter(|token| TENANT_PROTECTED_DISTINCTION_TOKENS.contains(&token.as_str()))
        .cloned()
        .collect()
}

fn tenant_brand(tokens: &[String], protected_tokens: &[String]) -> String {
    if protected_tokens.is_empty() {
        tokens.join(" ")
    } else {
        protected_tokens.join(" ")
    }
}

fn shared_tokens(left: &[String], right: &[String]) -> Vec<String> {
    let right = right.iter().map(String::as_str).collect::<BTreeSet<_>>();
    left.iter()
        .filter(|token| right.contains(token.as_str()))
        .cloned()
        .collect()
}

fn legal_suffix_reasons(stripped_terms: &[String]) -> Vec<NamekitReason> {
    stripped_terms
        .iter()
        .map(|term| {
            NamekitReason::new(ReasonCode::LegalSuffixStripped, ReasonStage::LegalSuffix)
                .with_source(SourceTechnique::Cleanco)
                .with_detail("suffix", term)
                .with_detail("profile_id", CMBS_TENANT_PROFILE_ID)
        })
        .collect()
}

fn dropped_token_reasons(tokens: &[String]) -> Vec<NamekitReason> {
    tokens
        .iter()
        .map(|token| {
            NamekitReason::new(ReasonCode::ProfileTokenDropped, ReasonStage::ProfilePolicy)
                .with_source(SourceTechnique::CanonProfile)
                .with_detail("profile_id", CMBS_TENANT_PROFILE_ID)
                .with_detail("token", token)
        })
        .collect()
}

fn support_token_reasons(tokens: &[String]) -> Vec<NamekitReason> {
    tokens
        .iter()
        .filter(|token| !TENANT_PROTECTED_DISTINCTION_TOKENS.contains(&token.as_str()))
        .map(|token| {
            NamekitReason::new(ReasonCode::RareTokenSupport, ReasonStage::Tfidf)
                .with_source(SourceTechnique::SplinkTfAdjustment)
                .with_detail("profile_id", CMBS_TENANT_PROFILE_ID)
                .with_detail("token", token)
        })
        .collect()
}
