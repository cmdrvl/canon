use canon::namekit::ReasonCode;
use canon::namekit::normalize::{normalize_normality, normalize_openrefine_fingerprint};
use canon::namekit::tfidf::{
    SparseTfidfModel, TfidfInputSurface, TopKConfig, sorted_neighborhood_pairs,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const SOURCE_FIXTURES: &[&str] = &[
    "tests/fixtures/namekit/source_parity/normality_unicode.jsonl",
    "tests/fixtures/namekit/source_parity/openrefine_fingerprint.jsonl",
    "tests/fixtures/namekit/source_parity/cleanco_suffixes.jsonl",
    "tests/fixtures/namekit/source_parity/sparse_topn.jsonl",
    "tests/fixtures/namekit/source_parity/splink_tf_adjustments.jsonl",
    "tests/fixtures/namekit/source_parity/logic_v2_features.jsonl",
];

const PROFILE_FIXTURES: &[&str] = &[
    "tests/fixtures/namekit/cmbs_tenant/profile_cases.jsonl",
    "tests/fixtures/namekit/regab_firm/profile_cases.jsonl",
];

#[test]
fn namekit_source_parity_fixture_rows_are_explicit() {
    let rows = all_contract_rows();
    assert!(rows.len() >= 9);

    for row in rows {
        for key in [
            "case_id",
            "fixture_family",
            "profile",
            "source_techniques",
            "raw_inputs",
            "expected_views",
            "expected_reason_codes",
            "protected_tokens",
            "support_features",
            "anti_merge_expectation",
            "expected_non_equivalent",
            "lossy",
        ] {
            assert!(row.get(key).is_some(), "{key} missing from {row}");
        }
        assert!(!strings(&row, "source_techniques").is_empty());
        assert!(!strings(&row, "raw_inputs").is_empty());
        assert!(row["expected_views"].is_object());
        assert_reason_order(&row);
    }
}

#[test]
fn namekit_source_parity_covers_required_sources_and_negative_cases() {
    let rows = all_contract_rows();
    let case_ids = rows
        .iter()
        .map(|row| string(row, "case_id"))
        .collect::<BTreeSet<_>>();
    for required in [
        "normality_cafe_societe",
        "openrefine_sears_roebuck_co",
        "cleanco_multi_suffix_strip",
        "tfidf_sparse_topn_sears",
        "splink_common_sears_downweight",
        "logic_v2_sears_auto_antimerge",
        "cmbs_sears_llc_support",
        "cmbs_sears_auto_antimerge",
        "regab_pnc_bank_na_preserves_regulated_terms",
    ] {
        assert!(case_ids.contains(required), "missing case {required}");
    }

    let sources = rows
        .iter()
        .flat_map(|row| strings(row, "source_techniques"))
        .collect::<BTreeSet<_>>();
    for required in [
        "normality",
        "openrefine_fingerprint",
        "cleanco",
        "ing_entity_matching_model",
        "sparse_dot_topn",
        "splink_tf_adjustments",
        "opensanctions_logic_v2",
        "nomenklatura_resolver",
        "iso20275_gleif",
    ] {
        assert!(sources.contains(required), "missing source {required}");
    }

    assert!(
        rows.iter()
            .any(|row| bool_field(row, "anti_merge_expectation"))
    );
    assert!(
        rows.iter()
            .any(|row| bool_field(row, "expected_non_equivalent"))
    );
    assert!(
        rows.iter()
            .any(|row| !strings(row, "protected_tokens").is_empty())
    );
}

#[test]
fn namekit_golden_source_parity_expectations_match_current_primitives() {
    let normality = row_by_case("normality_cafe_societe");
    let normalized = normalize_normality(&strings(&normality, "raw_inputs")[0]);
    assert_eq!(
        normalized.normalized,
        normality["expected_views"]["normalized"]
    );
    assert_eq!(
        normalized.fingerprint,
        normality["expected_views"]["fingerprint"]
    );

    let openrefine = row_by_case("openrefine_sears_roebuck_co");
    let fingerprint = normalize_openrefine_fingerprint(&strings(&openrefine, "raw_inputs")[0]);
    assert_eq!(
        fingerprint.normalized,
        openrefine["expected_views"]["normalized"]
    );
    assert_eq!(
        fingerprint.fingerprint,
        openrefine["expected_views"]["fingerprint"]
    );

    let model = SparseTfidfModel::build(&[
        TfidfInputSurface::tokenized("sears-roebuck", "sears roebuck", ["sears", "roebuck"]),
        TfidfInputSurface::tokenized("sears-llc", "sears llc", ["sears", "llc"]),
        TfidfInputSurface::tokenized(
            "sears-auto-center",
            "sears auto center",
            ["sears", "auto", "center"],
        ),
        TfidfInputSurface::tokenized(
            "roebuck-holdings",
            "roebuck holdings",
            ["roebuck", "holdings"],
        ),
    ]);
    let topk = model
        .top_k_for_surface("sears-roebuck", TopKConfig::new(3))
        .expect("query row exists");
    let actual = topk
        .candidates
        .iter()
        .map(|candidate| candidate.normalized_key.as_str())
        .collect::<Vec<_>>();
    let sparse = row_by_case("tfidf_sparse_topn_sears");
    let expected = sparse["expected_views"]["expected_topk"]
        .as_array()
        .expect("expected topk")
        .iter()
        .map(|value| value.as_str().expect("topk string"))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);

    let neighborhood = sorted_neighborhood_pairs(
        &[
            ("s3", "sears roebuck"),
            ("s1", "sears"),
            ("s2", "sears auto"),
            ("s4", "pnc bank"),
        ],
        3,
        3,
    );
    assert!(neighborhood.capped_pair_count > 0);
    assert_eq!(neighborhood.emitted_pair_count, 3);
}

#[test]
#[allow(non_snake_case)]
fn CMBS_I001_cmbs_tenant_profile_cases_cover_support_and_antimerge() {
    let rows = fixture_rows("tests/fixtures/namekit/cmbs_tenant/profile_cases.jsonl");
    let support = rows
        .iter()
        .find(|row| string(row, "case_id") == "cmbs_sears_llc_support")
        .expect("support case");
    assert_eq!(support["expected_views"]["right_legal_basename"], "sears");
    assert!(!bool_field(support, "anti_merge_expectation"));
    assert!(!bool_field(support, "expected_non_equivalent"));

    let antimerge = rows
        .iter()
        .find(|row| string(row, "case_id") == "cmbs_sears_auto_antimerge")
        .expect("anti-merge case");
    assert!(bool_field(antimerge, "anti_merge_expectation"));
    assert!(bool_field(antimerge, "expected_non_equivalent"));
    assert_eq!(strings(antimerge, "protected_tokens"), ["auto", "center"]);
}

#[test]
#[allow(non_snake_case)]
fn REGAB_I002_regab_firm_profile_preserves_regulated_distinctions() {
    let row = row_by_case("regab_pnc_bank_na_preserves_regulated_terms");
    assert_eq!(string(&row, "profile"), "regab_firm_identity");
    assert!(bool_field(&row, "anti_merge_expectation"));
    assert!(bool_field(&row, "expected_non_equivalent"));
    assert_eq!(
        strings(&row, "protected_tokens"),
        ["bank", "national_association"]
    );
    assert_eq!(
        row["expected_views"]["regulated_terms"]
            .as_array()
            .expect("regulated terms")
            .iter()
            .map(|value| value.as_str().expect("regulated term"))
            .collect::<Vec<_>>(),
        ["bank", "national_association"]
    );
}

fn all_contract_rows() -> Vec<Value> {
    SOURCE_FIXTURES
        .iter()
        .chain(PROFILE_FIXTURES)
        .flat_map(fixture_rows)
        .collect()
}

fn row_by_case(case_id: &str) -> Value {
    all_contract_rows()
        .into_iter()
        .find(|row| string(row, "case_id") == case_id)
        .unwrap_or_else(|| panic!("missing fixture case {case_id}"))
}

fn fixture_rows(path: impl AsRef<Path>) -> Vec<Value> {
    let path = path.as_ref();
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|error| panic!("parse {} row {line}: {error}", path.display()))
        })
        .collect()
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

fn strings(row: &Value, key: &str) -> Vec<String> {
    row[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} must be array in {row}"))
        .iter()
        .map(|value| value.as_str().expect("string array entry").to_string())
        .collect()
}

fn string(row: &Value, key: &str) -> String {
    row[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} must be string in {row}"))
        .to_string()
}

fn bool_field(row: &Value, key: &str) -> bool {
    row[key]
        .as_bool()
        .unwrap_or_else(|| panic!("{key} must be bool in {row}"))
}
