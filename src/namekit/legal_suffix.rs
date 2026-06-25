//! Legal-form suffix provenance and profile policy contracts.
//!
//! This module deliberately starts with a small canon-curated seed table. The
//! upstream libraries named by ENT-P02.8 are reference sources for semantics,
//! not copied runtime data. A larger generated table must carry its own digest,
//! source versions, and license review before it can replace this seed.

use serde::{Deserialize, Serialize};

pub const LEGAL_FORM_CONTRACT_VERSION: &str = "canon_namekit_legal_form.v0";
pub const LEGAL_FORM_DATA_DIGEST: &str = "blake3:canon-namekit-legal-form-seed-v0";
pub const LEGAL_FORM_LICENSE_REVIEW: &str = "canon_curated_seed_no_external_suffix_table_copied";
pub const LEGAL_SUFFIX_STRIPPED: &str = "legal_suffix_stripped";
pub const LEGAL_SUFFIX_PRESERVED_PROFILE: &str = "legal_suffix_preserved_profile";
pub const LEGAL_SUFFIX_REPEATED_STRIP: &str = "legal_suffix_repeated_strip";
pub const PROTECTED_LEGAL_TOKEN_RETAINED: &str = "protected_legal_token_retained";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegalFormSource {
    CanonCuratedSeed,
    CleancoReference,
    OccrpRigourReference,
    Iso20275GleifReference,
    OpenRefineReference,
}

impl LegalFormSource {
    pub const fn source_version(self) -> &'static str {
        match self {
            Self::CanonCuratedSeed => LEGAL_FORM_CONTRACT_VERSION,
            Self::CleancoReference => "reference-only:cleanco-basename-type-country",
            Self::OccrpRigourReference => "reference-only:rigour-fingerprints-legal-forms",
            Self::Iso20275GleifReference => "reference-only:iso-20275-gleif-elf",
            Self::OpenRefineReference => "reference-only:openrefine-fingerprint",
        }
    }

    pub const fn license_note(self) -> &'static str {
        match self {
            Self::CanonCuratedSeed => LEGAL_FORM_LICENSE_REVIEW,
            Self::CleancoReference => "reference_only_verify_license_before_copying_data",
            Self::OccrpRigourReference => "reference_only_verify_license_before_copying_data",
            Self::Iso20275GleifReference => "reference_only_verify_terms_before_generating_table",
            Self::OpenRefineReference => "reference_only_semantics_no_suffix_data_copied",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegalSuffixProfile {
    CmbsTenantLabel,
    RegabFirmIdentity,
}

impl LegalSuffixProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CmbsTenantLabel => "cmbs_tenant_label",
            Self::RegabFirmIdentity => "regab_firm_identity",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LegalSuffixEntry {
    pub term: &'static str,
    pub normalized_term: &'static str,
    pub source: LegalFormSource,
    pub source_version: &'static str,
    pub license: &'static str,
    pub jurisdiction: Option<&'static str>,
    pub entity_types: &'static [&'static str],
    pub provenance: &'static str,
    pub strip_for_tenant_label: bool,
    pub preserve_for_regab_firm: bool,
    pub protected_token: bool,
}

impl LegalSuffixEntry {
    pub fn token_count(self) -> usize {
        self.normalized_term.split_whitespace().count()
    }

    pub fn tokens(self) -> impl Iterator<Item = &'static str> {
        self.normalized_term.split_whitespace()
    }
}

pub const LEGAL_SUFFIX_ENTRIES: &[LegalSuffixEntry] = &[
    LegalSuffixEntry {
        term: "LLC",
        normalized_term: "llc",
        source: LegalFormSource::CanonCuratedSeed,
        source_version: LEGAL_FORM_CONTRACT_VERSION,
        license: LEGAL_FORM_LICENSE_REVIEW,
        jurisdiction: Some("US"),
        entity_types: &["limited_liability_company"],
        provenance: "canon-curated common US legal form; cleanco and rigour are reference-only",
        strip_for_tenant_label: true,
        preserve_for_regab_firm: false,
        protected_token: false,
    },
    LegalSuffixEntry {
        term: "Ltd",
        normalized_term: "ltd",
        source: LegalFormSource::CanonCuratedSeed,
        source_version: LEGAL_FORM_CONTRACT_VERSION,
        license: LEGAL_FORM_LICENSE_REVIEW,
        jurisdiction: None,
        entity_types: &["limited_company"],
        provenance: "canon-curated common legal form; cleanco repeated-strip behavior is reference-only",
        strip_for_tenant_label: true,
        preserve_for_regab_firm: false,
        protected_token: false,
    },
    LegalSuffixEntry {
        term: "Co.",
        normalized_term: "co",
        source: LegalFormSource::CanonCuratedSeed,
        source_version: LEGAL_FORM_CONTRACT_VERSION,
        license: LEGAL_FORM_LICENSE_REVIEW,
        jurisdiction: None,
        entity_types: &["company"],
        provenance: "canon-curated common abbreviation; OpenRefine fingerprint token folding is reference-only",
        strip_for_tenant_label: true,
        preserve_for_regab_firm: false,
        protected_token: false,
    },
    LegalSuffixEntry {
        term: "and Co.",
        normalized_term: "and co",
        source: LegalFormSource::CanonCuratedSeed,
        source_version: LEGAL_FORM_CONTRACT_VERSION,
        license: LEGAL_FORM_LICENSE_REVIEW,
        jurisdiction: None,
        entity_types: &["company"],
        provenance: "canon-curated phrase for Sears, Roebuck and Co.; OpenRefine token folding is reference-only",
        strip_for_tenant_label: true,
        preserve_for_regab_firm: false,
        protected_token: false,
    },
    LegalSuffixEntry {
        term: "Bank",
        normalized_term: "bank",
        source: LegalFormSource::CanonCuratedSeed,
        source_version: LEGAL_FORM_CONTRACT_VERSION,
        license: LEGAL_FORM_LICENSE_REVIEW,
        jurisdiction: None,
        entity_types: &["regulated_financial_institution"],
        provenance: "canon-curated protected Reg AB firm token; not copied from external tables",
        strip_for_tenant_label: false,
        preserve_for_regab_firm: true,
        protected_token: true,
    },
    LegalSuffixEntry {
        term: "National Association",
        normalized_term: "national association",
        source: LegalFormSource::CanonCuratedSeed,
        source_version: LEGAL_FORM_CONTRACT_VERSION,
        license: LEGAL_FORM_LICENSE_REVIEW,
        jurisdiction: Some("US"),
        entity_types: &["national_bank"],
        provenance: "canon-curated protected regulated-entity phrase; GLEIF/ISO legal-form lists are reference-only",
        strip_for_tenant_label: true,
        preserve_for_regab_firm: true,
        protected_token: true,
    },
    LegalSuffixEntry {
        term: "N.A.",
        normalized_term: "n a",
        source: LegalFormSource::CanonCuratedSeed,
        source_version: LEGAL_FORM_CONTRACT_VERSION,
        license: LEGAL_FORM_LICENSE_REVIEW,
        jurisdiction: Some("US"),
        entity_types: &["national_bank"],
        provenance: "canon-curated abbreviation for national association; GLEIF/ISO legal-form lists are reference-only",
        strip_for_tenant_label: true,
        preserve_for_regab_firm: true,
        protected_token: true,
    },
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegalSuffixReason {
    pub code: &'static str,
    pub term: String,
    pub normalized_term: String,
    pub source: LegalFormSource,
    pub source_version: &'static str,
    pub license: &'static str,
    pub profile: LegalSuffixProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegalSuffixAnalysis {
    pub contract_version: &'static str,
    pub data_digest: &'static str,
    pub profile: LegalSuffixProfile,
    pub raw: String,
    pub normalized_tokens: Vec<String>,
    pub basename: String,
    pub stripped_terms: Vec<String>,
    pub preserved_terms: Vec<String>,
    pub reasons: Vec<LegalSuffixReason>,
}

pub fn legal_suffix_entries() -> &'static [LegalSuffixEntry] {
    LEGAL_SUFFIX_ENTRIES
}

pub fn analyze_legal_suffixes(input: &str, profile: LegalSuffixProfile) -> LegalSuffixAnalysis {
    let mut tokens = normalize_tokens(input);
    let original_tokens = tokens.clone();
    let mut stripped_terms = Vec::new();
    let mut preserved_terms = Vec::new();
    let mut reasons = Vec::new();

    record_protected_terms(profile, &tokens, &mut preserved_terms, &mut reasons);

    while let Some(entry) = longest_suffix_entry(&tokens) {
        if should_preserve(entry, profile) {
            push_reason(&mut reasons, LEGAL_SUFFIX_PRESERVED_PROFILE, entry, profile);
            if !preserved_terms
                .iter()
                .any(|term| term == entry.normalized_term)
            {
                preserved_terms.push(entry.normalized_term.to_string());
            }
            break;
        }

        if !should_strip(entry, profile) {
            break;
        }

        let count = entry.token_count();
        tokens.truncate(tokens.len().saturating_sub(count));
        stripped_terms.push(entry.normalized_term.to_string());
        push_reason(&mut reasons, LEGAL_SUFFIX_STRIPPED, entry, profile);
        if stripped_terms.len() > 1 {
            push_reason(&mut reasons, LEGAL_SUFFIX_REPEATED_STRIP, entry, profile);
        }
    }

    LegalSuffixAnalysis {
        contract_version: LEGAL_FORM_CONTRACT_VERSION,
        data_digest: LEGAL_FORM_DATA_DIGEST,
        profile,
        raw: input.to_string(),
        normalized_tokens: original_tokens,
        basename: tokens.join(" "),
        stripped_terms,
        preserved_terms,
        reasons,
    }
}

fn normalize_tokens(input: &str) -> Vec<String> {
    input
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_lowercase())
        .collect()
}

fn longest_suffix_entry(tokens: &[String]) -> Option<LegalSuffixEntry> {
    LEGAL_SUFFIX_ENTRIES
        .iter()
        .copied()
        .filter(|entry| suffix_matches(tokens, *entry))
        .max_by_key(|entry| entry.token_count())
}

fn suffix_matches(tokens: &[String], entry: LegalSuffixEntry) -> bool {
    let entry_tokens = entry.tokens().collect::<Vec<_>>();
    if entry_tokens.len() > tokens.len() {
        return false;
    }
    tokens[tokens.len() - entry_tokens.len()..]
        .iter()
        .map(String::as_str)
        .eq(entry_tokens)
}

fn should_strip(entry: LegalSuffixEntry, profile: LegalSuffixProfile) -> bool {
    match profile {
        LegalSuffixProfile::CmbsTenantLabel => entry.strip_for_tenant_label,
        LegalSuffixProfile::RegabFirmIdentity => !entry.preserve_for_regab_firm,
    }
}

fn should_preserve(entry: LegalSuffixEntry, profile: LegalSuffixProfile) -> bool {
    match profile {
        LegalSuffixProfile::CmbsTenantLabel => false,
        LegalSuffixProfile::RegabFirmIdentity => entry.preserve_for_regab_firm,
    }
}

fn record_protected_terms(
    profile: LegalSuffixProfile,
    tokens: &[String],
    preserved_terms: &mut Vec<String>,
    reasons: &mut Vec<LegalSuffixReason>,
) {
    if profile != LegalSuffixProfile::RegabFirmIdentity {
        return;
    }
    for entry in LEGAL_SUFFIX_ENTRIES
        .iter()
        .copied()
        .filter(|entry| entry.protected_token)
    {
        if contains_term(tokens, entry) {
            if !preserved_terms
                .iter()
                .any(|term| term == entry.normalized_term)
            {
                preserved_terms.push(entry.normalized_term.to_string());
            }
            push_reason(reasons, PROTECTED_LEGAL_TOKEN_RETAINED, entry, profile);
        }
    }
}

fn contains_term(tokens: &[String], entry: LegalSuffixEntry) -> bool {
    let entry_tokens = entry.tokens().collect::<Vec<_>>();
    if entry_tokens.is_empty() || entry_tokens.len() > tokens.len() {
        return false;
    }
    tokens.windows(entry_tokens.len()).any(|window| {
        window
            .iter()
            .map(String::as_str)
            .eq(entry_tokens.iter().copied())
    })
}

fn push_reason(
    reasons: &mut Vec<LegalSuffixReason>,
    code: &'static str,
    entry: LegalSuffixEntry,
    profile: LegalSuffixProfile,
) {
    reasons.push(LegalSuffixReason {
        code,
        term: entry.term.to_string(),
        normalized_term: entry.normalized_term.to_string(),
        source: entry.source,
        source_version: entry.source_version,
        license: entry.license,
        profile,
    });
}
