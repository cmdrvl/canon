//! Reg AB firm-name normalization helpers.
//!
//! These helpers are profile-scoped. They consume the shared namekit
//! normalization and legal-form contracts, but keep Reg AB review cues explicit
//! so related regulated entities are not collapsed by a generic string view.

use crate::{
    entity::{
        edge::EdgeEvidenceHit,
        score::{ScoreLane, ScoreUnits},
    },
    namekit::{
        legal_suffix::{LegalSuffixAnalysis, LegalSuffixProfile, analyze_legal_suffixes},
        normalize::{NamekitNormalization, normalize_normality},
    },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegabFirmGuardKind {
    BankLoanServicesDivision,
    PlatformCategoryLabel,
    ServicerSubservicerAgentRoleConflict,
    AuditorSubjectPartyRoleConflict,
    ParentSubsidiaryBoundary,
    SameFamilyDifferentRegulatedEntity,
}

impl RegabFirmGuardKind {
    pub const fn from_code(code: &str) -> Option<Self> {
        match code.as_bytes() {
            b"bank_vs_loan_services_division" => Some(Self::BankLoanServicesDivision),
            b"platform_category_label" => Some(Self::PlatformCategoryLabel),
            b"servicer_subservicer_agent_role_conflict" => {
                Some(Self::ServicerSubservicerAgentRoleConflict)
            }
            b"auditor_subject_party_role_conflict" => Some(Self::AuditorSubjectPartyRoleConflict),
            b"parent_subsidiary_boundary" => Some(Self::ParentSubsidiaryBoundary),
            b"same_family_different_regulated_entity" => {
                Some(Self::SameFamilyDifferentRegulatedEntity)
            }
            _ => None,
        }
    }

    pub const fn code(self) -> &'static str {
        match self {
            Self::BankLoanServicesDivision => "bank_vs_loan_services_division",
            Self::PlatformCategoryLabel => "platform_category_label",
            Self::ServicerSubservicerAgentRoleConflict => {
                "servicer_subservicer_agent_role_conflict"
            }
            Self::AuditorSubjectPartyRoleConflict => "auditor_subject_party_role_conflict",
            Self::ParentSubsidiaryBoundary => "parent_subsidiary_boundary",
            Self::SameFamilyDifferentRegulatedEntity => "same_family_different_regulated_entity",
        }
    }

    pub const fn operator_id(self) -> &'static str {
        match self {
            Self::BankLoanServicesDivision => "division_boundary:regab_firm_identity",
            Self::PlatformCategoryLabel => "platform_label_guard:regab_firm_identity",
            Self::ServicerSubservicerAgentRoleConflict => "role_conflict:regab_firm_identity",
            Self::AuditorSubjectPartyRoleConflict => "role_conflict:regab_firm_identity",
            Self::ParentSubsidiaryBoundary => "role_conflict:regab_parent_subsidiary",
            Self::SameFamilyDifferentRegulatedEntity => {
                "protected_token_conflict:regab_regulated_entity"
            }
        }
    }

    pub const fn reason_code(self) -> &'static str {
        match self {
            Self::BankLoanServicesDivision => "regab_bank_division_boundary",
            Self::PlatformCategoryLabel => "regab_platform_label_guard",
            Self::ServicerSubservicerAgentRoleConflict => "regab_role_capacity_conflict",
            Self::AuditorSubjectPartyRoleConflict => "regab_auditor_subject_role_conflict",
            Self::ParentSubsidiaryBoundary => "regab_parent_subsidiary_boundary",
            Self::SameFamilyDifferentRegulatedEntity => "regab_same_family_distinct_entity",
        }
    }

    pub const fn review_priority(self) -> &'static str {
        match self {
            Self::PlatformCategoryLabel | Self::AuditorSubjectPartyRoleConflict => "critical",
            Self::BankLoanServicesDivision
            | Self::ServicerSubservicerAgentRoleConflict
            | Self::ParentSubsidiaryBoundary
            | Self::SameFamilyDifferentRegulatedEntity => "high",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegabFirmGuardRequest<'a> {
    pub namespace: &'a str,
    pub guard: RegabFirmGuardKind,
    pub left_name: &'a str,
    pub right_name: &'a str,
    pub left_role: Option<&'a str>,
    pub right_role: Option<&'a str>,
    pub score_units: ScoreUnits,
}

pub fn regab_firm_guard_hit(request: RegabFirmGuardRequest<'_>) -> Option<EdgeEvidenceHit> {
    let left_name = request.left_name.trim();
    let right_name = request.right_name.trim();
    if left_name.is_empty() || right_name.is_empty() {
        return None;
    }

    let left = normalize_regab_firm_name(left_name);
    let right = normalize_regab_firm_name(right_name);
    Some(EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        request.namespace,
        request.guard.operator_id(),
        request.guard.reason_code(),
        request.score_units,
        true,
        format!(
            "regab guard={} left_core={} right_core={} left_cues={} right_cues={} left_role={} right_role={} review_priority={} score_units={}",
            request.guard.code(),
            left.firm_core,
            right.firm_core,
            cue_codes(&left.review_cues),
            cue_codes(&right.review_cues),
            optional_value(request.left_role),
            optional_value(request.right_role),
            request.guard.review_priority(),
            request.score_units.as_u32()
        ),
    ))
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

fn cue_codes(cues: &[RegabReviewCue]) -> String {
    if cues.is_empty() {
        return "none".to_string();
    }
    cues.iter()
        .map(|cue| cue.code())
        .collect::<Vec<_>>()
        .join("|")
}

fn optional_value(value: Option<&str>) -> &str {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("none")
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
