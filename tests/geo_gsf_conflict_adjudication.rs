#![forbid(unsafe_code)]

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

const MEASUREMENT: &str =
    "scripts/geo_measurements/fixtures/e4_gsf_conflict_adjudication_2026-09-08/measurement.json";
const RESIDUAL_MEASUREMENT: &str =
    "scripts/geo_measurements/fixtures/e4_gsf_residuals_2026-09-08/measurement.json";

#[test]
fn gsf_conflict_adjudication_declares_no_direct_gsf_lever() {
    let measurement = read_json(MEASUREMENT);
    assert_eq!(
        measurement["version"],
        "bd_3g94_gsf_conflict_adjudication.v0"
    );
    assert_eq!(measurement["bead"], "bd-3g94");
    assert_eq!(
        measurement["proof_class"],
        "retained_cmdrvl_data_warehouse_adjudication_not_live"
    );
    assert!(
        !measurement["release_claim_allowed"]
            .as_bool()
            .expect("release claim flag")
    );
    assert_eq!(measurement["frozen_denominator"], 77);
    assert_eq!(measurement["reported_frozen_denominator"], 79);
    assert_eq!(measurement["retained_population_deficit"], 7);
    assert_eq!(measurement["retained_population_denominator"], 70);
    assert!(
        measurement["boundary"]
            .as_str()
            .expect("boundary")
            .contains("no rho admission is relaxed")
    );

    assert_eq!(measurement["baseline"]["truth_exclusions"], 6);
    assert_eq!(measurement["baseline"]["truth_exclusions_completed"], 6);
    assert_eq!(measurement["baseline"]["reported_truth_exclusions"], 8);
    assert_eq!(
        measurement["baseline"]["truth_classification_incomplete"],
        2
    );
    assert_eq!(
        measurement["baseline"]["truth_classification_incomplete_case_fragments"],
        serde_json::json!(["91b2df27", "a7dac634"])
    );
    assert_eq!(measurement["baseline"]["resolved"], 7);
    assert_eq!(measurement["baseline"]["exactly_correct"], 7);
    assert_eq!(measurement["baseline"]["false_merges"], 0);
    assert_eq!(
        measurement["measurement"]["direct_gsf_admission_lever"],
        "none"
    );
    assert!(
        !measurement["measurement"]["safe_canon_gsf_lever_exists"]
            .as_bool()
            .expect("safe lever flag")
    );
    assert_eq!(
        measurement["expected_direct_e4_movement"],
        json_object([
            ("ambiguous_delta", 0),
            ("exactly_correct_delta", 0),
            ("false_merges_delta", 0),
            ("resolved_delta", 0),
            ("truth_exclusions_delta", 0),
        ])
    );

    let cases = cases_by_fragment(&measurement);
    assert_eq!(
        cases.keys().cloned().collect::<BTreeSet<_>>(),
        ["68a1a4ce", "91b2df27", "e1ee0535"]
            .into_iter()
            .map(str::to_string)
            .collect()
    );
    assert_eq!(
        measurement["measurement"]["classification_counts"],
        json_object([
            ("canon_property_row_binding_defect", 1),
            ("retained_truth_document_binding_conflict", 2),
        ])
    );

    for case in cases.values() {
        assert_eq!(
            case["current_gsf_contract"]["contract_id"],
            "rho.size.assessment_roll_gross_sqft_band"
        );
        assert!(
            !case["current_gsf_contract"]["corrected_truth_sum_inside_band"]
                .as_bool()
                .expect("inside band flag")
        );
        assert_eq!(
            case["settlement"]["expected_direct_e4_movement"],
            measurement["expected_direct_e4_movement"]
        );
        assert!(
            !case["settlement"]["release_claim_allowed"]
                .as_bool()
                .expect("case release claim flag")
        );
    }
}

#[test]
fn gsf_conflict_adjudication_reuses_the_current_vector_residual_facts() {
    let measurement = read_json(MEASUREMENT);
    let residual = read_json(RESIDUAL_MEASUREMENT);
    let adjudicated = cases_by_fragment(&measurement);
    let residual_cases = cases_by_fragment(&residual);

    for (fragment, case) in adjudicated {
        let prior = residual_cases
            .get(fragment.as_str())
            .expect("matching residual case");
        assert_eq!(case["case_id"], prior["case_id"]);
        assert_eq!(case["filed_property"], prior["filed_property"]);
        assert_eq!(case["current_gsf_contract"]["band_min"], prior["band_min"]);
        assert_eq!(case["current_gsf_contract"]["band_max"], prior["band_max"]);
        assert_eq!(
            case["current_gsf_contract"]["corrected_truth_gsf_sum"],
            prior["corrected_truth_gsf_sum"]
        );
        assert_eq!(
            case["current_gsf_contract"]["vector_count"],
            prior["vector_count"]
        );
        assert_eq!(case["truth_roll_summary"], prior["truth_roll_summary"]);
    }
}

#[test]
fn gsf_conflict_adjudication_separates_truth_binding_from_property_row_binding() {
    let measurement = read_json(MEASUREMENT);
    let cases = cases_by_fragment(&measurement);

    let east_14th = &cases["68a1a4ce"];
    assert_eq!(
        east_14th["settlement"]["classification"],
        "retained_truth_document_binding_conflict"
    );
    assert_eq!(
        east_14th["settlement"]["wrong_side"],
        "truth_source_binding"
    );
    assert_eq!(
        east_14th["acris_document_binding"]["document_id"],
        "2019121000627002"
    );
    assert_eq!(
        string_set(&east_14th["acris_document_binding"]["legal_addresses"]),
        ["250 EAST 53RD STREET", "768 5 AVENUE"]
            .into_iter()
            .map(str::to_string)
            .collect()
    );
    assert_eq!(
        string_set(&east_14th["property_mart_context"]["target_property_key_history"]["addresses"]),
        ["208 East 14th Street"]
            .into_iter()
            .map(str::to_string)
            .collect()
    );
    assert!(
        east_14th["property_mart_context"]["matching_filed_property_rows_for_acris_legal_addresses"]
            .as_array()
            .expect("matching filed rows")
            .is_empty()
    );
    assert!(
        east_14th["completeness_score_interaction"]["classified_as_completeness_conflict"]
            .as_bool()
            .expect("conflict flag")
    );

    let west_57th = &cases["91b2df27"];
    assert_eq!(
        west_57th["settlement"]["classification"],
        "retained_truth_document_binding_conflict"
    );
    assert_eq!(
        west_57th["acris_document_binding"]["document_id"],
        "2021060701261001"
    );
    assert_eq!(west_57th["acris_document_binding"]["legal_bbl_count"], 172);
    assert_eq!(
        string_set(&west_57th["acris_document_binding"]["legal_addresses"]),
        ["217 WEST 57TH STREET"]
            .into_iter()
            .map(str::to_string)
            .collect()
    );
    assert!(
        west_57th["property_mart_context"]
            ["matching_filed_property_rows_for_acris_legal_addresses"]
            .as_array()
            .expect("matching filed rows")
            .is_empty()
    );
    assert!(
        !west_57th["completeness_score_interaction"]["classified_as_completeness_conflict"]
            .as_bool()
            .expect("conflict flag")
    );
    assert!(
        west_57th["completeness_score_interaction"]["follow_up"]
            .as_str()
            .expect("follow-up")
            .contains("Score-report diagnostics")
    );

    let regency = &cases["e1ee0535"];
    assert_eq!(
        regency["settlement"]["classification"],
        "canon_property_row_binding_defect"
    );
    assert_eq!(
        regency["settlement"]["wrong_side"],
        "canon_property_row_binding"
    );
    assert_eq!(
        regency["acris_document_binding"]["document_id"],
        "2019050100411003"
    );
    assert_eq!(
        string_set(&regency["acris_document_binding"]["legal_addresses"]),
        [
            "141-02 79TH AVENUE",
            "141-24 78TH AVENUE",
            "141-48 78TH ROAD",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    );
    assert_eq!(
        string_set(&regency["property_mart_context"]["target_property_key_history"]["addresses"]),
        ["450-460 PARK AVENUE SOUTH"]
            .into_iter()
            .map(str::to_string)
            .collect()
    );
    let matching_rows = regency["property_mart_context"]["same_loan_matching_property_rows"]
        .as_array()
        .expect("same-loan matching rows");
    assert_eq!(
        matching_rows
            .iter()
            .map(|row| row["property_key"].as_str().expect("property key"))
            .collect::<BTreeSet<_>>(),
        [
            "CREP-0C97A73C45D5CD7C",
            "CREP-5F29F9FB87795B4A",
            "CREP-72177B208235E832",
        ]
        .into_iter()
        .collect()
    );
    assert!(matching_rows.iter().any(|row| {
        row["address"]
            .as_str()
            .expect("address")
            .contains("141-24 78th Avenue")
            && row["size_measure"] == "UNITS"
    }));
}

#[test]
fn gsf_conflict_adjudication_pins_retained_inputs_and_source_queries() {
    let measurement = read_json(MEASUREMENT);
    for (path_field, sha_field) in [
        ("population_path", "population_sha256"),
        (
            "gsf_residual_measurement_path",
            "gsf_residual_measurement_sha256",
        ),
        (
            "gsf_value_vector_rebuild_measurement_path",
            "gsf_value_vector_rebuild_measurement_sha256",
        ),
        (
            "gsf_value_vector_rebuild_overlay_path",
            "gsf_value_vector_rebuild_overlay_sha256",
        ),
        (
            "collateral_completeness_measurement_path",
            "collateral_completeness_measurement_sha256",
        ),
    ] {
        let relative = measurement["retained_inputs"][path_field]
            .as_str()
            .expect("retained input path");
        let expected = measurement["retained_inputs"][sha_field]
            .as_str()
            .expect("retained input sha256");
        assert_eq!(sha256_file(repo_path(relative)), expected);
    }

    assert_eq!(
        string_set(&measurement["source_verification"]["described_tables"]),
        [
            "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.WRGL_NYC_OPENDATA_PROPERTY_VALUATION_AND_ASSESSMENT_DATA_TAX_CLASSES_1_2_3_4__STRUCTURED",
            "EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE",
            "EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY",
            "EDGAR_DB.PROPERTY_MART.PROPERTY_DIM",
            "EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT",
            "EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_LEGALS_EXT",
            "EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_MASTER_EXT",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    );

    let query_counts = measurement["source_verification"]["queries"]
        .as_array()
        .expect("source queries")
        .iter()
        .map(|query| {
            (
                query["measurement_name"]
                    .as_str()
                    .expect("measurement name"),
                query["row_count"].as_u64().expect("row count"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(query_counts["bd_3g94_sec_loan_property_fanout"], 15);
    assert_eq!(query_counts["bd_3g94_acris_truth_document_trace"], 3);
    assert_eq!(query_counts["bd_3g94_acris_document_full_legal_counts"], 3);
    assert_eq!(query_counts["bd_3g94_property_key_history_summary"], 12);
    assert_eq!(
        query_counts["bd_3g94_filed_rows_matching_acris_legal_addresses"],
        7
    );

    assert_eq!(
        measurement["source_terms"]["assessment_roll"]["period"],
        "FY2026P3"
    );
    assert!(
        measurement["source_terms"]["loan_property_mart"]["license_terms"]
            .as_str()
            .expect("license terms")
            .contains("underlying SEC")
    );
}

fn cases_by_fragment(measurement: &Value) -> BTreeMap<String, &Value> {
    measurement["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|case| {
            (
                case["case_fragment"]
                    .as_str()
                    .expect("case fragment")
                    .to_string(),
                case,
            )
        })
        .collect()
}

fn string_set(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|item| item.as_str().expect("string item").to_string())
        .collect()
}

fn json_object<const N: usize>(pairs: [(&str, i64); N]) -> Value {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), Value::from(value)))
        .collect::<serde_json::Map<_, _>>()
        .into()
}

fn read_json(relative: &str) -> Value {
    let file = File::open(repo_path(relative)).expect("open JSON fixture");
    serde_json::from_reader(BufReader::new(file)).expect("parse JSON fixture")
}

fn sha256_file(path: impl AsRef<Path>) -> String {
    let mut file = File::open(path).expect("open fixture for sha256");
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file.read(&mut buffer).expect("read fixture for sha256");
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    format!("{:x}", hasher.finalize())
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
