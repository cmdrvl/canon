use canon::namekit::ReasonCode;
use canon::namekit::legal_suffix::{
    LegalSuffixAnalysis, LegalSuffixProfile, analyze_legal_suffixes,
};
use canon::namekit::normalize::{
    NamekitNormalization, normalize_normality, normalize_openrefine_fingerprint,
};
use canon::namekit::tokenize::{NamekitTokenization, tokenize_sorted_unique, tokenize_words};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;

const GOLDEN_SUITE: &str = "tests/fixtures/namekit/golden_suite.jsonl";

#[test]
fn namekit_golden_suite_covers_tenant_and_regab_examples() {
    let rows = golden_suite_rows();
    let case_ids = rows
        .iter()
        .map(|row| string(row, "case_id"))
        .collect::<BTreeSet<_>>();

    for required in [
        "golden_sears_llc_tenant_label",
        "golden_sears_auto_center_tenant",
        "golden_kmart_distinct_tenant",
        "golden_transform_sr_llc_successor",
        "golden_pnc_bank_na_regab",
        "golden_midland_loan_services_regab",
        "golden_platform_label_regab",
        "golden_accented_tenant_label",
        "golden_punctuation_whitespace_variant",
    ] {
        assert!(case_ids.contains(required), "missing {required}");
    }

    for row in &rows {
        for key in [
            "profile",
            "raw",
            "normalization_view",
            "token_mode",
            "expected_views",
            "expected_reason_codes",
            "expected_legal_reason_codes",
            "semantic_protected_tokens",
            "expected_non_equivalent",
            "non_equivalence_note",
        ] {
            assert!(row.get(key).is_some(), "{key} missing from {row}");
        }
        assert_reason_order(row);
        assert!(
            !string(row, "non_equivalence_note").is_empty(),
            "{} must explain equivalence risk",
            string(row, "case_id")
        );
    }

    assert!(
        rows.iter()
            .any(|row| bool_field(row, "expected_non_equivalent"))
    );
    assert!(
        rows.iter()
            .any(|row| !strings(row, "semantic_protected_tokens").is_empty())
    );
}

#[test]
fn namekit_golden_suite_matches_current_primitives() {
    for row in golden_suite_rows() {
        let normalized = normalize_for_row(&row);
        assert_eq!(
            normalized.normalized,
            row["expected_views"]["normalized"],
            "{}",
            string(&row, "case_id")
        );
        assert_eq!(
            normalized.fingerprint,
            row["expected_views"]["fingerprint"],
            "{}",
            string(&row, "case_id")
        );
        assert_eq!(
            normalized.reason_codes(),
            strings(&row, "expected_reason_codes"),
            "{}",
            string(&row, "case_id")
        );

        let legal = analyze_legal_suffixes(&normalized.normalized, profile(&row));
        assert_legal_view(&row, &legal);

        let tokenization = tokenize_for_row(&row, &legal.basename);
        assert_eq!(
            token_texts(&tokenization),
            strings_from(&row["expected_views"], "tokens"),
            "{}",
            string(&row, "case_id")
        );
    }
}

#[test]
fn namekit_golden_suite_locks_profile_specific_distinctions() {
    let rows = golden_suite_rows();
    let pnc = row_by_case_id(&rows, "golden_pnc_bank_na_regab");
    assert_eq!(
        strings_from(&pnc["expected_views"], "protected_tokens"),
        ["bank", "n a"]
    );
    assert!(bool_field(pnc, "expected_non_equivalent"));

    let platform = row_by_case_id(&rows, "golden_platform_label_regab");
    assert_eq!(strings(platform, "semantic_protected_tokens"), ["platform"]);
    assert!(string(platform, "non_equivalence_note").contains("platform"));

    let sears_auto = row_by_case_id(&rows, "golden_sears_auto_center_tenant");
    assert_eq!(
        strings(sears_auto, "semantic_protected_tokens"),
        ["sears", "auto", "center"]
    );
    assert!(bool_field(sears_auto, "expected_non_equivalent"));
}

fn normalize_for_row(row: &Value) -> NamekitNormalization {
    let raw = string(row, "raw");
    match string(row, "normalization_view") {
        "normality" => normalize_normality(raw),
        "openrefine_fingerprint" => normalize_openrefine_fingerprint(raw),
        other => panic!("unsupported normalization view {other}"),
    }
}

fn tokenize_for_row(row: &Value, input: &str) -> NamekitTokenization {
    match string(row, "token_mode") {
        "sequence" => tokenize_words(input),
        "sorted_unique" => tokenize_sorted_unique(input),
        other => panic!("unsupported token mode {other}"),
    }
}

fn assert_legal_view(row: &Value, legal: &LegalSuffixAnalysis) {
    assert_eq!(
        legal.basename,
        row["expected_views"]["legal_basename"],
        "{}",
        string(row, "case_id")
    );
    assert_eq!(
        legal.stripped_terms,
        strings_from(&row["expected_views"], "stripped_terms"),
        "{}",
        string(row, "case_id")
    );
    assert_eq!(
        legal.preserved_terms,
        strings_from(&row["expected_views"], "protected_tokens"),
        "{}",
        string(row, "case_id")
    );
    assert_eq!(
        legal_reason_codes(legal),
        strings(row, "expected_legal_reason_codes"),
        "{}",
        string(row, "case_id")
    );
}

fn profile(row: &Value) -> LegalSuffixProfile {
    match string(row, "profile") {
        "cmbs_tenant_label" => LegalSuffixProfile::CmbsTenantLabel,
        "regab_firm_identity" => LegalSuffixProfile::RegabFirmIdentity,
        other => panic!("unsupported profile {other}"),
    }
}

fn golden_suite_rows() -> Vec<Value> {
    fs::read_to_string(GOLDEN_SUITE)
        .expect("namekit golden suite fixture is readable")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("namekit golden suite row is JSON"))
        .collect()
}

fn row_by_case_id<'a>(rows: &'a [Value], case_id: &str) -> &'a Value {
    rows.iter()
        .find(|row| string(row, "case_id") == case_id)
        .unwrap_or_else(|| panic!("missing case {case_id}"))
}

fn assert_reason_order(row: &Value) {
    let mut previous = None;
    for code in strings(row, "expected_reason_codes") {
        let order = ReasonCode::try_from(code.as_str())
            .unwrap_or_else(|error| panic!("{}: {error}", string(row, "case_id")))
            .order();
        if let Some(previous) = previous {
            assert!(
                previous <= order,
                "{} reason codes are not canonical",
                string(row, "case_id")
            );
        }
        previous = Some(order);
    }
}

fn token_texts(tokenization: &NamekitTokenization) -> Vec<String> {
    tokenization
        .tokens
        .iter()
        .map(|token| token.text.clone())
        .collect()
}

fn legal_reason_codes(legal: &LegalSuffixAnalysis) -> Vec<&'static str> {
    legal.reasons.iter().map(|reason| reason.code).collect()
}

fn strings(row: &Value, key: &str) -> Vec<String> {
    strings_from(row, key)
}

fn strings_from(row: &Value, key: &str) -> Vec<String> {
    row[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} must be an array in {row}"))
        .iter()
        .map(|value| value.as_str().expect("string array entry").to_string())
        .collect()
}

fn string<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} must be a string in {row}"))
}

fn bool_field(row: &Value, key: &str) -> bool {
    row[key]
        .as_bool()
        .unwrap_or_else(|| panic!("{key} must be a bool in {row}"))
}
