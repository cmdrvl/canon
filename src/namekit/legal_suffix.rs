//! Legal-form suffix provenance and profile policy contracts.
//!
//! This module ships a canon-curated legal-form table. The upstream libraries
//! named by ENT-P02.8 are reference sources for semantics, not copied runtime
//! data. Every entry carries its source/version/license fields and the table
//! digest changes whenever the curated data changes.

use crate::namekit::normalize::normalize_normality;
use serde::{Deserialize, Serialize};

pub const LEGAL_FORM_CONTRACT_VERSION: &str = "canon_namekit_legal_form.v0";
pub const LEGAL_FORM_EQUIVALENCE_CONTRACT_VERSION: &str = "canon_namekit_legal_form_equivalence.v0";
pub const LEGAL_FORM_DATA_DIGEST: &str =
    "blake3:ed516e3781086c4d2d990f2b87e6d429c851284a4e16af7d94b47c222eb1823f";
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
    fn match_token_count(self) -> usize {
        normalized_entry_tokens(self.term).len()
    }

    pub fn token_count(self) -> usize {
        self.normalized_term.split_whitespace().count()
    }

    pub fn tokens(self) -> impl Iterator<Item = &'static str> {
        self.normalized_term.split_whitespace()
    }
}

macro_rules! curated_entry {
    (
        $term:expr,
        $normalized_term:expr,
        $jurisdiction:expr,
        $entity_types:expr,
        $strip_for_tenant_label:expr,
        $preserve_for_regab_firm:expr,
        $protected_token:expr,
        $provenance:expr $(,)?
    ) => {
        LegalSuffixEntry {
            term: $term,
            normalized_term: $normalized_term,
            source: LegalFormSource::CanonCuratedSeed,
            source_version: LEGAL_FORM_CONTRACT_VERSION,
            license: LEGAL_FORM_LICENSE_REVIEW,
            jurisdiction: $jurisdiction,
            entity_types: $entity_types,
            provenance: $provenance,
            strip_for_tenant_label: $strip_for_tenant_label,
            preserve_for_regab_firm: $preserve_for_regab_firm,
            protected_token: $protected_token,
        }
    };
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
    curated_entry!(
        "Inc.",
        "inc",
        Some("US"),
        &["corporation"],
        true,
        false,
        false,
        "canon-curated common US legal form; no external table data copied",
    ),
    curated_entry!(
        "Incorporated",
        "inc",
        Some("US"),
        &["corporation"],
        true,
        false,
        false,
        "canon-curated common US legal form; no external table data copied",
    ),
    curated_entry!(
        "Corp.",
        "corp",
        Some("US"),
        &["corporation"],
        true,
        false,
        false,
        "canon-curated common US legal form; no external table data copied",
    ),
    curated_entry!(
        "Corporation",
        "corp",
        Some("US"),
        &["corporation"],
        true,
        false,
        false,
        "canon-curated common US legal form; no external table data copied",
    ),
    curated_entry!(
        "LP",
        "lp",
        Some("US"),
        &["limited_partnership"],
        true,
        false,
        false,
        "canon-curated common US legal form; no external table data copied",
    ),
    curated_entry!(
        "L.P.",
        "lp",
        Some("US"),
        &["limited_partnership"],
        true,
        false,
        false,
        "canon-curated common US legal form; no external table data copied",
    ),
    curated_entry!(
        "Limited Partnership",
        "lp",
        Some("US"),
        &["limited_partnership"],
        true,
        false,
        false,
        "canon-curated common US legal form; no external table data copied",
    ),
    curated_entry!(
        "LLP",
        "llp",
        Some("US"),
        &["limited_liability_partnership"],
        true,
        false,
        false,
        "canon-curated common US legal form; no external table data copied",
    ),
    curated_entry!(
        "LLLP",
        "lllp",
        Some("US"),
        &["limited_liability_limited_partnership"],
        true,
        false,
        false,
        "canon-curated common US legal form; no external table data copied",
    ),
    curated_entry!(
        "L.L.C.",
        "llc",
        Some("US"),
        &["limited_liability_company"],
        true,
        false,
        false,
        "canon-curated common US legal form; no external table data copied",
    ),
    curated_entry!(
        "Limited Liability Company",
        "llc",
        Some("US"),
        &["limited_liability_company"],
        true,
        false,
        false,
        "canon-curated common US legal form; no external table data copied",
    ),
    curated_entry!(
        "Ltd.",
        "ltd",
        None,
        &["limited_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "Limited",
        "ltd",
        None,
        &["limited_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "PLC",
        "plc",
        Some("GB"),
        &["public_limited_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "Co",
        "co",
        None,
        &["company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "Company",
        "co",
        None,
        &["company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "Trust",
        "trust",
        None,
        &["trust"],
        false,
        true,
        true,
        "canon-curated protected trust token; never stripped by namekit",
    ),
    curated_entry!(
        "GmbH",
        "gmbh",
        Some("DE"),
        &["limited_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "AG",
        "ag",
        Some("DE"),
        &["public_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "S.A.",
        "sa",
        None,
        &["public_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "S.à r.l.",
        "sarl",
        Some("LU"),
        &["limited_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "Sarl",
        "sarl",
        Some("LU"),
        &["limited_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "B.V.",
        "bv",
        Some("NL"),
        &["limited_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "N.V.",
        "nv",
        Some("NL"),
        &["public_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "Pty Ltd",
        "pty ltd",
        Some("AU"),
        &["proprietary_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "Pty. Ltd.",
        "pty ltd",
        Some("AU"),
        &["proprietary_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "Proprietary Limited",
        "pty ltd",
        Some("AU"),
        &["proprietary_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "ULC",
        "ulc",
        Some("CA"),
        &["unlimited_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "S.p.A.",
        "spa",
        Some("IT"),
        &["public_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "AB",
        "ab",
        Some("SE"),
        &["public_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "ApS",
        "aps",
        Some("DK"),
        &["limited_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
    curated_entry!(
        "Oy",
        "oy",
        Some("FI"),
        &["limited_company"],
        true,
        false,
        false,
        "canon-curated common legal form; no external table data copied",
    ),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegalFormNormalization {
    pub contract_version: &'static str,
    pub data_digest: &'static str,
    pub raw: String,
    pub normalized: String,
    pub steps: Vec<String>,
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

        let count = entry.match_token_count();
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

/// Normalize equivalent legal-form spellings without stripping the form.
///
/// This is deliberately a suffix-only transform: a token such as `Co` in
/// `Co-Invest` is not a legal-form suffix and remains part of the name. The
/// canonical token is retained in the view, and every replacement is recorded
/// as `legal_form:<canonical>` for evidence provenance.
pub fn normalize_legal_forms(input: &str) -> LegalFormNormalization {
    let normalized = normalize_normality(input).normalized;
    let tokens = normalize_tokens(&normalized);
    let mut cursor = tokens.len();
    let mut replacements = Vec::<(usize, String)>::new();
    let mut steps = Vec::new();

    while cursor > 0 {
        let Some(entry) = longest_suffix_entry(&tokens[..cursor]) else {
            break;
        };
        let count = entry.match_token_count();
        if count == 0 || count > cursor {
            break;
        }
        let start = cursor - count;
        replacements.push((start, entry.normalized_term.to_string()));
        steps.push(format!("legal_form:{}", entry.normalized_term));
        cursor = start;
    }

    if replacements.is_empty() {
        return LegalFormNormalization {
            contract_version: LEGAL_FORM_EQUIVALENCE_CONTRACT_VERSION,
            data_digest: LEGAL_FORM_DATA_DIGEST,
            raw: input.to_string(),
            normalized,
            steps,
        };
    }

    replacements.reverse();
    let mut output = tokens[..cursor].to_vec();
    output.extend(
        replacements
            .into_iter()
            .map(|(_, replacement)| replacement)
            .flat_map(|replacement| {
                replacement
                    .split_whitespace()
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            }),
    );
    steps.reverse();
    LegalFormNormalization {
        contract_version: LEGAL_FORM_EQUIVALENCE_CONTRACT_VERSION,
        data_digest: LEGAL_FORM_DATA_DIGEST,
        raw: input.to_string(),
        normalized: output.join(" "),
        steps,
    }
}

fn normalize_tokens(input: &str) -> Vec<String> {
    normalize_normality(input)
        .normalized
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn normalized_entry_tokens(input: &str) -> Vec<String> {
    normalize_tokens(input)
}

fn longest_suffix_entry(tokens: &[String]) -> Option<LegalSuffixEntry> {
    LEGAL_SUFFIX_ENTRIES
        .iter()
        .copied()
        .filter(|entry| suffix_matches(tokens, *entry))
        .max_by_key(|entry| entry.match_token_count())
}

fn suffix_matches(tokens: &[String], entry: LegalSuffixEntry) -> bool {
    let entry_tokens = normalized_entry_tokens(entry.term);
    if entry_tokens.len() > tokens.len() {
        return false;
    }
    tokens[tokens.len() - entry_tokens.len()..]
        .iter()
        .map(String::as_str)
        .eq(entry_tokens.iter().map(String::as_str))
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
    let entry_tokens = normalized_entry_tokens(entry.term);
    if entry_tokens.is_empty() || entry_tokens.len() > tokens.len() {
        return false;
    }
    tokens.windows(entry_tokens.len()).any(|window| {
        window
            .iter()
            .map(String::as_str)
            .eq(entry_tokens.iter().map(String::as_str))
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

#[cfg(test)]
mod tests {
    use super::normalize_legal_forms;

    #[test]
    fn legal_form_equivalence_covers_each_curated_family() {
        for (raw, expected, step) in [
            ("Acme Inc", "acme inc", "legal_form:inc"),
            ("Acme Inc.", "acme inc", "legal_form:inc"),
            ("Acme Incorporated", "acme inc", "legal_form:inc"),
            ("Acme Corp", "acme corp", "legal_form:corp"),
            ("Acme Corp.", "acme corp", "legal_form:corp"),
            ("Acme Corporation", "acme corp", "legal_form:corp"),
            ("Acme LP", "acme lp", "legal_form:lp"),
            ("Acme L.P.", "acme lp", "legal_form:lp"),
            ("Acme Limited Partnership", "acme lp", "legal_form:lp"),
            ("Acme LLP", "acme llp", "legal_form:llp"),
            ("Acme LLLP", "acme lllp", "legal_form:lllp"),
            ("Acme L.L.C.", "acme llc", "legal_form:llc"),
            (
                "Acme Limited Liability Company",
                "acme llc",
                "legal_form:llc",
            ),
            ("Acme Ltd.", "acme ltd", "legal_form:ltd"),
            ("Acme Limited", "acme ltd", "legal_form:ltd"),
            ("Acme PLC", "acme plc", "legal_form:plc"),
            ("Acme Co", "acme co", "legal_form:co"),
            ("Acme Company", "acme co", "legal_form:co"),
            ("Acme Trust", "acme trust", "legal_form:trust"),
            ("Acme GmbH", "acme gmbh", "legal_form:gmbh"),
            ("Acme AG", "acme ag", "legal_form:ag"),
            ("Acme S.A.", "acme sa", "legal_form:sa"),
            ("Acme S.à r.l.", "acme sarl", "legal_form:sarl"),
            ("Acme Sarl", "acme sarl", "legal_form:sarl"),
            ("Acme B.V.", "acme bv", "legal_form:bv"),
            ("Acme N.V.", "acme nv", "legal_form:nv"),
            ("Acme Pty Ltd", "acme pty ltd", "legal_form:pty ltd"),
            ("Acme Pty. Ltd.", "acme pty ltd", "legal_form:pty ltd"),
            (
                "Acme Proprietary Limited",
                "acme pty ltd",
                "legal_form:pty ltd",
            ),
            ("Acme ULC", "acme ulc", "legal_form:ulc"),
            ("Acme S.p.A.", "acme spa", "legal_form:spa"),
            ("Acme AB", "acme ab", "legal_form:ab"),
            ("Acme ApS", "acme aps", "legal_form:aps"),
            ("Acme Oy", "acme oy", "legal_form:oy"),
        ] {
            let normalized = normalize_legal_forms(raw);
            assert_eq!(normalized.normalized, expected, "{raw}");
            assert!(
                normalized.steps.iter().any(|actual| actual == step),
                "{raw}"
            );
        }
    }

    #[test]
    fn legal_form_equivalence_keeps_distinct_forms_and_protected_tokens() {
        let llc = normalize_legal_forms("Acme LLC");
        let inc = normalize_legal_forms("Acme Inc");
        assert_ne!(llc.normalized, inc.normalized);

        let bank = normalize_legal_forms("First National Bank");
        assert_eq!(bank.normalized, "first national bank");
        assert!(bank.steps.contains(&"legal_form:bank".to_string()));

        let co_invest = normalize_legal_forms("Acme Co-Invest LP");
        assert_eq!(co_invest.normalized, "acme co invest lp");
        assert!(!co_invest.steps.contains(&"legal_form:co".to_string()));

        let trust = normalize_legal_forms("Series 2021-1 Trust");
        assert_eq!(trust.normalized, "series 2021 1 trust");
        assert!(trust.steps.contains(&"legal_form:trust".to_string()));
    }
}
