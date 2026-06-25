use canon::namekit::legal_suffix::{
    LEGAL_FORM_CONTRACT_VERSION, LEGAL_FORM_DATA_DIGEST, LEGAL_FORM_LICENSE_REVIEW,
    LEGAL_SUFFIX_PRESERVED_PROFILE, LEGAL_SUFFIX_REPEATED_STRIP, LEGAL_SUFFIX_STRIPPED,
    LegalFormSource, LegalSuffixProfile, PROTECTED_LEGAL_TOKEN_RETAINED, analyze_legal_suffixes,
    legal_suffix_entries,
};
use serde_json::Value;
use std::collections::BTreeSet;

#[test]
fn namekit_legal_suffix_profile_policy_covers_required_examples() {
    for case in jsonl_cases(include_str!("fixtures/namekit/legal_suffix/examples.jsonl")) {
        let profile = match case["profile"].as_str().expect("profile must be a string") {
            "cmbs_tenant_label" => LegalSuffixProfile::CmbsTenantLabel,
            "regab_firm_identity" => LegalSuffixProfile::RegabFirmIdentity,
            other => panic!("unknown profile in fixture: {other}"),
        };
        let analysis =
            analyze_legal_suffixes(case["raw"].as_str().expect("raw must be a string"), profile);

        assert_eq!(
            analysis.contract_version, LEGAL_FORM_CONTRACT_VERSION,
            "{}",
            case["id"]
        );
        assert_eq!(
            analysis.data_digest, LEGAL_FORM_DATA_DIGEST,
            "{}",
            case["id"]
        );
        assert_eq!(
            analysis.basename,
            case["expected_basename"]
                .as_str()
                .expect("expected basename must be a string"),
            "{}",
            case["id"]
        );
        assert_eq!(
            analysis.stripped_terms,
            string_array(&case["expected_stripped_terms"]),
            "{}",
            case["id"]
        );
        assert_eq!(
            analysis.preserved_terms,
            string_array(&case["expected_preserved_terms"]),
            "{}",
            case["id"]
        );

        let reason_codes = analysis
            .reasons
            .iter()
            .map(|reason| reason.code)
            .collect::<Vec<_>>();
        assert_eq!(
            reason_codes,
            string_array(&case["expected_reason_codes"]),
            "{}",
            case["id"]
        );
        assert!(
            analysis.reasons.iter().all(|reason| reason.source_version
                == LEGAL_FORM_CONTRACT_VERSION
                && reason.license == LEGAL_FORM_LICENSE_REVIEW),
            "{}",
            case["id"]
        );
    }
}

#[test]
fn legal_suffix_provenance_snapshot_covers_sources_and_license_review() {
    let entries = legal_suffix_entries();
    let terms = entries
        .iter()
        .map(|entry| entry.normalized_term)
        .collect::<BTreeSet<_>>();

    for expected in [
        "llc",
        "ltd",
        "and co",
        "bank",
        "national association",
        "n a",
    ] {
        assert!(terms.contains(expected), "missing suffix term: {expected}");
    }

    for source in [
        LegalFormSource::CanonCuratedSeed,
        LegalFormSource::CleancoReference,
        LegalFormSource::OccrpRigourReference,
        LegalFormSource::Iso20275GleifReference,
        LegalFormSource::OpenRefineReference,
    ] {
        assert!(!source.source_version().is_empty());
        assert!(!source.license_note().is_empty());
    }

    for entry in entries {
        assert_eq!(entry.source, LegalFormSource::CanonCuratedSeed);
        assert_eq!(entry.source_version, LEGAL_FORM_CONTRACT_VERSION);
        assert_eq!(entry.license, LEGAL_FORM_LICENSE_REVIEW);
        assert!(!entry.term.is_empty());
        assert!(!entry.normalized_term.is_empty());
        assert!(!entry.entity_types.is_empty());
        assert!(!entry.provenance.is_empty());
        assert!(
            entry
                .normalized_term
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch == ' ')
        );
    }

    for snapshot in jsonl_cases(include_str!(
        "fixtures/namekit/legal_suffix/provenance.jsonl"
    )) {
        let normalized_term = snapshot["normalized_term"]
            .as_str()
            .expect("normalized_term must be a string");
        let entry = entries
            .iter()
            .find(|entry| entry.normalized_term == normalized_term)
            .unwrap_or_else(|| panic!("snapshot term missing from table: {normalized_term}"));
        assert_eq!(
            entry.source_version,
            snapshot["source_version"]
                .as_str()
                .expect("source_version must be a string")
        );
        assert_eq!(
            entry.license,
            snapshot["license"]
                .as_str()
                .expect("license must be a string")
        );
        assert_eq!(
            entry.protected_token,
            snapshot["protected_token"]
                .as_bool()
                .expect("protected_token must be a bool")
        );
    }
}

#[test]
fn legal_suffix_reason_code_constants_are_stable() {
    assert_eq!(
        LegalSuffixProfile::CmbsTenantLabel.as_str(),
        "cmbs_tenant_label"
    );
    assert_eq!(
        LegalSuffixProfile::RegabFirmIdentity.as_str(),
        "regab_firm_identity"
    );
    assert_eq!(LEGAL_SUFFIX_STRIPPED, "legal_suffix_stripped");
    assert_eq!(
        LEGAL_SUFFIX_PRESERVED_PROFILE,
        "legal_suffix_preserved_profile"
    );
    assert_eq!(LEGAL_SUFFIX_REPEATED_STRIP, "legal_suffix_repeated_strip");
    assert_eq!(
        PROTECTED_LEGAL_TOKEN_RETAINED,
        "protected_legal_token_retained"
    );
}

#[test]
fn legal_suffix_protected_forms() {
    let regab_national_bank = analyze_legal_suffixes(
        "PNC Bank, National Association",
        LegalSuffixProfile::RegabFirmIdentity,
    );

    assert_eq!(
        regab_national_bank.basename,
        "pnc bank national association"
    );
    assert!(regab_national_bank.stripped_terms.is_empty());
    assert_eq!(
        regab_national_bank.preserved_terms,
        ["bank", "national association"]
    );
    assert_eq!(
        reason_codes(&regab_national_bank),
        [
            PROTECTED_LEGAL_TOKEN_RETAINED,
            PROTECTED_LEGAL_TOKEN_RETAINED,
            LEGAL_SUFFIX_PRESERVED_PROFILE
        ]
    );

    let regab_with_strip_around_protected =
        analyze_legal_suffixes("PNC Bank LLC", LegalSuffixProfile::RegabFirmIdentity);

    assert_eq!(regab_with_strip_around_protected.basename, "pnc bank");
    assert_eq!(regab_with_strip_around_protected.stripped_terms, ["llc"]);
    assert_eq!(regab_with_strip_around_protected.preserved_terms, ["bank"]);
    assert_eq!(
        reason_codes(&regab_with_strip_around_protected),
        [
            PROTECTED_LEGAL_TOKEN_RETAINED,
            LEGAL_SUFFIX_STRIPPED,
            LEGAL_SUFFIX_PRESERVED_PROFILE
        ]
    );

    let tenant_view =
        analyze_legal_suffixes("PNC Bank, National Association", LegalSuffixProfile::CmbsTenantLabel);

    assert_eq!(tenant_view.basename, "pnc bank");
    assert_eq!(tenant_view.stripped_terms, ["national association"]);
    assert!(
        tenant_view
            .reasons
            .iter()
            .all(|reason| reason.code != PROTECTED_LEGAL_TOKEN_RETAINED),
        "generic tenant view must not invent Reg AB protected-form evidence"
    );
}

fn jsonl_cases(input: &str) -> impl Iterator<Item = Value> + '_ {
    input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("fixture line must be valid JSON"))
}

fn reason_codes(analysis: &canon::namekit::legal_suffix::LegalSuffixAnalysis) -> Vec<&'static str> {
    analysis.reasons.iter().map(|reason| reason.code).collect()
}

fn string_array(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("fixture value must be an array")
        .iter()
        .map(|item| item.as_str().expect("fixture array item must be a string"))
        .collect()
}
