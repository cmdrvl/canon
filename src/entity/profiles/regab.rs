//! Reg AB firm-name normalization helpers.
//!
//! These helpers are profile-scoped. They consume the shared namekit
//! normalization and legal-form contracts, but keep Reg AB review cues explicit
//! so related regulated entities are not collapsed by a generic string view.

use crate::namekit::{
    legal_suffix::{LegalSuffixAnalysis, LegalSuffixProfile, analyze_legal_suffixes},
    normalize::{NamekitNormalization, normalize_normality},
};
use serde::Serialize;

pub const REGAB_FIRM_NORMALIZATION_VERSION: &str = "canon_regab_firm_normalize.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegabFirmNormalization {
    pub version: &'static str,
    pub raw: String,
    pub normalized: String,
    pub firm_core: String,
    pub regulated_form_key: String,
    pub tokens: Vec<String>,
    pub legal_form: LegalSuffixAnalysis,
    pub review_cues: Vec<RegabReviewCue>,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegabReviewCue {
    DivisionBoundary,
    ParentSubsidiaryContext,
    PlatformLabel,
    AgentOrCapacityRole,
}

impl RegabReviewCue {
    pub const fn code(self) -> &'static str {
        match self {
            Self::DivisionBoundary => "division_boundary",
            Self::ParentSubsidiaryContext => "parent_subsidiary_context",
            Self::PlatformLabel => "platform_label_guard",
            Self::AgentOrCapacityRole => "role_capacity_guard",
        }
    }
}

pub fn normalize_regab_firm_name(raw: &str) -> RegabFirmNormalization {
    let base = normalize_normality(raw);
    let legal_form =
        analyze_legal_suffixes(&base.normalized, LegalSuffixProfile::RegabFirmIdentity);
    let tokens = legal_form
        .basename
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let regulated_form_key = regulated_form_key(&tokens);
    let review_cues = review_cues(&tokens);
    let reason_codes = reason_codes(&base, &legal_form, &review_cues);

    RegabFirmNormalization {
        version: REGAB_FIRM_NORMALIZATION_VERSION,
        raw: raw.to_string(),
        normalized: base.normalized,
        firm_core: legal_form.basename.clone(),
        regulated_form_key,
        tokens,
        legal_form,
        review_cues,
        reason_codes,
    }
}

fn regulated_form_key(tokens: &[String]) -> String {
    let mut expanded = Vec::with_capacity(tokens.len() + 1);
    let mut index = 0;
    while index < tokens.len() {
        if index + 1 < tokens.len() && tokens[index] == "n" && tokens[index + 1] == "a" {
            expanded.push("national".to_string());
            expanded.push("association".to_string());
            index += 2;
        } else {
            expanded.push(tokens[index].clone());
            index += 1;
        }
    }
    expanded.join(" ")
}

fn review_cues(tokens: &[String]) -> Vec<RegabReviewCue> {
    let mut cues = Vec::new();
    if contains_phrase(tokens, &["division", "of"]) {
        cues.push(RegabReviewCue::DivisionBoundary);
    }
    if tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "affiliate" | "affiliated" | "parent" | "subsidiary" | "successor" | "predecessor"
        )
    }) {
        cues.push(RegabReviewCue::ParentSubsidiaryContext);
    }
    if tokens
        .iter()
        .any(|token| matches!(token.as_str(), "platform" | "category"))
    {
        cues.push(RegabReviewCue::PlatformLabel);
    }
    if tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "agent" | "subservicer" | "servicer" | "trustee" | "custodian"
        )
    }) {
        cues.push(RegabReviewCue::AgentOrCapacityRole);
    }
    cues.sort_unstable();
    cues.dedup();
    cues
}

fn contains_phrase(tokens: &[String], phrase: &[&str]) -> bool {
    !phrase.is_empty()
        && tokens
            .windows(phrase.len())
            .any(|window| window.iter().map(String::as_str).eq(phrase.iter().copied()))
}

fn reason_codes(
    base: &NamekitNormalization,
    legal_form: &LegalSuffixAnalysis,
    review_cues: &[RegabReviewCue],
) -> Vec<String> {
    let mut codes = base
        .reason_codes()
        .into_iter()
        .map(str::to_string)
        .chain(
            legal_form
                .reasons
                .iter()
                .map(|reason| reason.code.to_string()),
        )
        .chain(review_cues.iter().map(|cue| cue.code().to_string()))
        .collect::<Vec<_>>();
    codes.sort();
    codes.dedup();
    codes
}
