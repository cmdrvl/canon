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

const GOLDEN_MATRIX: &str = "tests/fixtures/namekit/source_parity/nk_golden_matrix.jsonl";
const GOLDEN_SUITE: &str = "tests/fixtures/namekit/golden_suite.jsonl";

#[test]
fn namekit_source_parity_golden_matrix_covers_nk_u001_through_nk_u005() {
    let rows = golden_rows();
    let ids = rows
        .iter()
        .map(|row| string(row, "fixture_id"))
        .collect::<BTreeSet<_>>();

    assert_eq!(rows.len(), 5);
    assert_eq!(
        ids,
        ["NK-U001", "NK-U002", "NK-U003", "NK-U004", "NK-U005"]
            .into_iter()
            .collect::<BTreeSet<_>>()
    );

    for row in rows {
        for key in [
            "case_id",
            "profile",
            "normalization_view",
            "token_mode",
            "source_techniques",
            "raw_inputs",
            "expected_views",
            "expected_reason_codes",
            "expected_legal_reason_codes",
            "expected_non_equivalent",
            "intentional_divergence",
        ] {
            assert!(row.get(key).is_some(), "{key} missing from {row}");
        }
        assert!(!strings(&row, "raw_inputs").is_empty());
        assert!(!strings(&row, "source_techniques").is_empty());
        assert!(!string(&row, "intentional_divergence").is_empty());
        assert_reason_order(&row);
    }
}

#[test]
fn namekit_golden_matrix_matches_current_primitives() {
    for row in golden_rows() {
        for raw in strings(&row, "raw_inputs") {
            let normalization = normalize_for_row(&row, &raw);
            assert_eq!(
                normalization.normalized,
                row["expected_views"]["normalized"],
                "{}",
                string(&row, "fixture_id")
            );
            assert_eq!(
                normalization.fingerprint,
                row["expected_views"]["fingerprint"],
                "{}",
                string(&row, "fixture_id")
            );
            assert_eq!(
                normalization.reason_codes(),
                strings(&row, "expected_reason_codes"),
                "{}",
                string(&row, "fixture_id")
            );

            let legal = analyze_legal_suffixes(&normalization.normalized, profile(&row));
            assert_legal_view(&row, &legal);

            let tokenization = tokenize_for_row(&row, &legal.basename);
            assert_eq!(
                token_texts(&tokenization),
                strings_from(&row["expected_views"], "tokens"),
                "{}",
                string(&row, "fixture_id")
            );
        }
    }
}

#[test]
fn namekit_golden_fixture_suite_covers_tenant_and_regab_examples() {
    let rows = fixture_rows(GOLDEN_SUITE);
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
        assert!(!string(row, "profile").is_empty());
        assert!(!string(row, "raw").is_empty());
        assert!(row["expected_views"].is_object());
        assert!(!strings_from(&row["expected_views"], "tokens").is_empty());
        assert_reason_order(row);
    }
}

#[test]
fn namekit_golden_fixture_suite_matches_current_normalization_and_tokens() {
    for row in fixture_rows(GOLDEN_SUITE) {
        let normalization = normalize_for_row(&row, string(&row, "raw"));
        assert_eq!(
            normalization.normalized,
            row["expected_views"]["normalized"],
            "{}",
            string(&row, "case_id")
        );
        assert_eq!(
            normalization.reason_codes(),
            strings(&row, "expected_reason_codes"),
            "{}",
            string(&row, "case_id")
        );
        assert_eq!(
            normalization.fingerprint,
            row["expected_views"]["fingerprint"],
            "{}",
            string(&row, "case_id")
        );

        let legal = analyze_legal_suffixes(&normalization.normalized, profile(&row));
        assert_eq!(
            legal.basename,
            row["expected_views"]["legal_basename"],
            "{}",
            string(&row, "case_id")
        );

        let tokenization = tokenize_words(&legal.basename);
        assert_eq!(
            token_texts(&tokenization),
            strings_from(&row["expected_views"], "tokens"),
            "{}",
            string(&row, "case_id")
        );
    }
}

#[test]
fn namekit_golden_fixture_suite_records_non_equivalence_notes() {
    for row in fixture_rows(GOLDEN_SUITE) {
        if bool_field(&row, "expected_non_equivalent") {
            assert!(
                !string(&row, "non_equivalence_note").is_empty(),
                "{} needs a non-equivalence note",
                string(&row, "case_id")
            );
            assert!(
                !strings(&row, "semantic_protected_tokens").is_empty(),
                "{} needs protected tokens",
                string(&row, "case_id")
            );
        }
    }
}

#[test]
#[allow(non_snake_case)]
fn NK_U001_sears_llc_yields_tenant_core_and_suffix_reason() {
    let row = row_by_fixture_id("NK-U001");
    let normalization = normalize_for_row(&row, "SEARS LLC");
    let legal = analyze_legal_suffixes(&normalization.normalized, profile(&row));

    assert_eq!(legal.basename, "sears");
    assert_eq!(legal.stripped_terms, ["llc"]);
    assert_eq!(legal_reason_codes(&legal), ["legal_suffix_stripped"]);
}

#[test]
#[allow(non_snake_case)]
fn NK_U005_variants_share_views_and_reason_order() {
    let row = row_by_fixture_id("NK-U005");
    let mut projections = Vec::new();

    for raw in strings(&row, "raw_inputs") {
        let normalization = normalize_for_row(&row, &raw);
        let legal = analyze_legal_suffixes(&normalization.normalized, profile(&row));
        let tokenization = tokenize_for_row(&row, &legal.basename);
        projections.push((
            normalization.normalized.clone(),
            normalization.fingerprint.clone(),
            normalization.reason_codes(),
            legal.basename,
            token_texts(&tokenization),
        ));
    }

    assert_eq!(projections.len(), 2);
    assert_eq!(projections[0], projections[1]);
    assert_eq!(
        projections[0].2,
        [
            "punctuation_removed",
            "whitespace_collapsed",
            "tokens_sorted",
            "source_parity_reference"
        ]
    );
    assert_eq!(projections[0].3, "sears roebuck");
    assert_eq!(projections[0].4, ["roebuck", "sears"]);
}

fn normalize_for_row(row: &Value, raw: &str) -> NamekitNormalization {
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
        string(row, "fixture_id")
    );
    assert_eq!(
        legal.stripped_terms,
        strings_from(&row["expected_views"], "stripped_terms"),
        "{}",
        string(row, "fixture_id")
    );
    assert_eq!(
        legal.preserved_terms,
        strings_from(&row["expected_views"], "protected_tokens"),
        "{}",
        string(row, "fixture_id")
    );
    assert_eq!(
        legal_reason_codes(legal),
        strings(row, "expected_legal_reason_codes"),
        "{}",
        string(row, "fixture_id")
    );
}

fn profile(row: &Value) -> LegalSuffixProfile {
    match string(row, "profile") {
        "cmbs_tenant_label" => LegalSuffixProfile::CmbsTenantLabel,
        "regab_firm_identity" => LegalSuffixProfile::RegabFirmIdentity,
        other => panic!("unsupported profile {other}"),
    }
}

fn row_by_fixture_id(fixture_id: &str) -> Value {
    golden_rows()
        .into_iter()
        .find(|row| string(row, "fixture_id") == fixture_id)
        .unwrap_or_else(|| panic!("missing fixture id {fixture_id}"))
}

fn golden_rows() -> Vec<Value> {
    fixture_rows(GOLDEN_MATRIX)
}

fn fixture_rows(path: &str) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read {path}: {error}"))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap_or_else(|error| panic!("{path}: {error}")))
        .collect()
}

fn assert_reason_order(row: &Value) {
    let mut previous = None;
    for code in strings(row, "expected_reason_codes") {
        let order = ReasonCode::try_from(code.as_str())
            .unwrap_or_else(|error| panic!("{}: {error}", string(row, "fixture_id")))
            .order();
        if let Some(previous) = previous {
            assert!(
                previous <= order,
                "{} reason codes are not canonical",
                string(row, "fixture_id")
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

fn bool_field(row: &Value, key: &str) -> bool {
    row[key]
        .as_bool()
        .unwrap_or_else(|| panic!("{key} must be a bool in {row}"))
}

fn string<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} must be a string in {row}"))
}
