#![forbid(unsafe_code)]

use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
};

const SOURCE_HUNT: &str =
    "scripts/geo_measurements/fixtures/e4_completeness_source_hunt_2026-09-08/source_hunt.json";
const H7_REQUEST: &str =
    "scripts/geo_measurements/fixtures/d1_residuals/h7_population_request.json";
const H7_POPULATION: &str =
    "scripts/geo_measurements/fixtures/d1_residuals/canon_geo_h7_population.v0.json";

const PROBE_CASES: [&str; 3] = [
    "h7-subject:round-exact-lender:13ff475129dbf3a8874ae7ab326c4ca40ef96552cb935aaf8435450c014a0d6a",
    "h7-subject:round-exact-lender:3991a5741b4f55913b9dc7a66ad63b6a5d93b39bf7b30b82618163dfbcf09625",
    "h7-subject:round-exact-lender:f5588ba9a55f697afea24ed55d8b34f58e1b544ad9bc5af07b6eaf49ae430ef7",
];

#[test]
fn completeness_source_hunt_classifies_landed_sources_by_grain() {
    let hunt = read_json(SOURCE_HUNT);
    assert_eq!(hunt["version"], "canon_geo_e4_completeness_source_hunt.v0");
    assert_eq!(hunt["bead"], "bd-2gvl");
    assert_eq!(
        hunt["proof_class"],
        "retained_cmdrvl_data_warehouse_source_hunt_not_live"
    );
    assert_eq!(hunt["frozen_denominator"], 79);
    assert_eq!(hunt["retained_population_denominator"], 70);
    assert_eq!(hunt["probe_denominator"], 3);
    assert!(
        hunt["boundary"]
            .as_str()
            .expect("boundary")
            .contains("do not relax rho admission")
    );
    assert!(
        hunt["proof_class"]
            .as_str()
            .expect("proof class")
            .ends_with("_not_live")
    );

    let sources = candidate_sources_by_id(&hunt);
    assert_eq!(
        sources.keys().cloned().collect::<BTreeSet<_>>(),
        [
            "acris_document_legal_rows".to_string(),
            "loan_to_property_row_grouping".to_string(),
            "sec_annex_loan_property_schedule".to_string(),
        ]
        .into_iter()
        .collect()
    );

    let acris = &sources["acris_document_legal_rows"];
    assert_eq!(acris["asserts_completeness"], true);
    assert_eq!(acris["safe_as_tax_lot_all_of"], true);
    assert_eq!(acris["completeness_grain"], "acris_document_legal_rows");
    assert_eq!(
        acris["warehouse_probe"]["full_document_sets_equal_truth"],
        3
    );
    assert_eq!(acris["retained_coverage"]["acris_legal_present"], 70);
    assert_eq!(
        acris["retained_coverage"]["acris_legal_parcel_set_equals_truth"],
        70
    );
    assert!(
        acris["not_independent_of_truth_when_used_on_h7_deed_truth"]
            .as_str()
            .expect("truth leakage boundary")
            .contains("independent document binding")
    );

    for id in [
        "sec_annex_loan_property_schedule",
        "loan_to_property_row_grouping",
    ] {
        let source = &sources[id];
        assert_eq!(source["asserts_completeness"], true);
        assert_eq!(source["safe_as_tax_lot_all_of"], false);
        assert!(
            source["safe_as_tax_lot_all_of_reason"]
                .as_str()
                .expect("safe_as_tax_lot_all_of_reason")
                .contains("property-row")
        );
    }

    let handoff = &hunt["mechanism_handoff"];
    assert_eq!(handoff["typed_relation"], "all_of_exact_cardinality");
    assert!(
        handoff["positive_rule"]
            .as_str()
            .expect("positive rule")
            .contains("declared grain")
    );
    assert!(
        handoff["negative_safety_rule"]
            .as_str()
            .expect("negative safety rule")
            .contains("abstain")
    );
}

#[test]
fn completeness_source_hunt_probe_cases_keep_exact_document_sets_visible() {
    let hunt = read_json(SOURCE_HUNT);
    let cases = probe_cases_by_id(&hunt);
    assert_eq!(
        cases.keys().cloned().collect::<BTreeSet<_>>(),
        PROBE_CASES.iter().map(|id| id.to_string()).collect()
    );

    let greenpoint = &cases[PROBE_CASES[0]];
    assert_eq!(greenpoint["short_id"], "13ff4751");
    assert_eq!(greenpoint["truth_lot_count"], 7);
    assert_eq!(greenpoint["loan_property_rows"], 7);
    assert_eq!(greenpoint["acris_legal_rows"], 7);
    assert_eq!(greenpoint["acris_legal_bbls_equal_truth"], true);
    assert_eq!(
        string_set(&greenpoint["truth_bbls"]),
        [
            "3026790021",
            "3026790022",
            "3026790023",
            "3026790054",
            "3026800037",
            "3026800043",
            "3026800046",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    );

    let west_24th = &cases[PROBE_CASES[1]];
    assert_eq!(west_24th["short_id"], "3991a574");
    assert_eq!(west_24th["truth_lot_count"], 3);
    assert_eq!(west_24th["loan_property_rows"], 2);
    assert_eq!(west_24th["nyc_loan_property_rows"], 1);
    assert_eq!(west_24th["acris_legal_rows"], 3);
    assert!(
        west_24th["source_hunt_result"]
            .as_str()
            .expect("source hunt result")
            .contains("cannot be used as direct three-lot cardinality evidence")
    );

    let mixed_county = &cases[PROBE_CASES[2]];
    assert_eq!(mixed_county["short_id"], "f5588ba9");
    assert_eq!(mixed_county["truth_lot_count"], 4);
    assert_eq!(mixed_county["loan_property_rows"], 8);
    assert_eq!(mixed_county["nyc_loan_property_rows"], 4);
    assert_eq!(mixed_county["acris_legal_rows"], 4);
    assert!(
        mixed_county["source_hunt_result"]
            .as_str()
            .expect("source hunt result")
            .contains("not a loan-grain all-of assertion")
    );
}

#[test]
fn completeness_source_hunt_retained_coverage_matches_h7_population_pins() {
    let hunt = read_json(SOURCE_HUNT);
    let request = read_json(H7_REQUEST);
    let population = read_json(H7_POPULATION);
    let request_ids = request["cases"]
        .as_array()
        .expect("request cases")
        .iter()
        .map(|case| case["id"].as_str().expect("request id").to_string())
        .collect::<BTreeSet<_>>();
    let cases = retained_cases_by_subject(&population, &request_ids);

    assert_eq!(request_ids.len(), 70);
    assert_eq!(cases.len(), 70);
    assert_eq!(
        hunt["retained_h7_coverage"]["request_subjects"],
        request_ids.len()
    );
    assert_eq!(
        hunt["retained_h7_coverage"]["matched_population_subjects"],
        cases.len()
    );

    let mut acris_master_present = 0_u64;
    let mut acris_legal_present = 0_u64;
    let mut acris_legal_parcel_set_equals_truth = 0_u64;
    let mut bridge_rows_present = 0_u64;
    let mut bridge_count_equals_truth_count = 0_u64;
    let mut total_truth_lots = 0_u64;
    let mut total_bridge_rows = 0_u64;

    for case in cases.values() {
        let source_records = case["source_records"]
            .as_array()
            .expect("case source records");
        let truth = string_set(&case["truth_parcels"]);
        let legal = source_records
            .iter()
            .filter(|record| record["role"] == "acris_legal")
            .flat_map(|record| {
                record["parcel_ids"]
                    .as_array()
                    .expect("legal parcel ids")
                    .iter()
                    .map(|parcel| parcel.as_str().expect("legal parcel id").to_string())
            })
            .collect::<BTreeSet<_>>();
        let bridge_count = source_records
            .iter()
            .filter(|record| record["role"] == "bridge_loan")
            .count() as u64;

        if source_records
            .iter()
            .any(|record| record["role"] == "acris_master")
        {
            acris_master_present += 1;
        }
        if !legal.is_empty() {
            acris_legal_present += 1;
        }
        if legal == truth {
            acris_legal_parcel_set_equals_truth += 1;
        }
        if bridge_count > 0 {
            bridge_rows_present += 1;
        }
        if bridge_count == truth.len() as u64 {
            bridge_count_equals_truth_count += 1;
        }
        total_truth_lots += truth.len() as u64;
        total_bridge_rows += bridge_count;
    }

    let coverage = &hunt["retained_h7_coverage"];
    assert_eq!(coverage["acris_master_present"], acris_master_present);
    assert_eq!(coverage["acris_legal_present"], acris_legal_present);
    assert_eq!(
        coverage["acris_legal_parcel_set_equals_truth"],
        acris_legal_parcel_set_equals_truth
    );
    assert_eq!(coverage["bridge_rows_present"], bridge_rows_present);
    assert_eq!(
        coverage["bridge_count_equals_truth_count"],
        bridge_count_equals_truth_count
    );
    assert_eq!(coverage["total_truth_lots"], total_truth_lots);
    assert_eq!(coverage["total_bridge_rows"], total_bridge_rows);
}

fn read_json(path: &str) -> Value {
    serde_json::from_reader(BufReader::new(
        File::open(path).unwrap_or_else(|error| panic!("open {path}: {error}")),
    ))
    .unwrap_or_else(|error| panic!("parse {path}: {error}"))
}

fn candidate_sources_by_id(hunt: &Value) -> BTreeMap<String, Value> {
    hunt["candidate_sources"]
        .as_array()
        .expect("candidate sources")
        .iter()
        .map(|source| {
            (
                source["id"].as_str().expect("source id").to_string(),
                source.clone(),
            )
        })
        .collect()
}

fn probe_cases_by_id(hunt: &Value) -> BTreeMap<String, Value> {
    hunt["probe_cases"]
        .as_array()
        .expect("probe cases")
        .iter()
        .map(|case| {
            (
                case["case_id"].as_str().expect("case id").to_string(),
                case.clone(),
            )
        })
        .collect()
}

fn retained_cases_by_subject(
    population: &Value,
    request_ids: &BTreeSet<String>,
) -> BTreeMap<String, Value> {
    let mut cases = BTreeMap::new();
    for case in population["cases"].as_array().expect("population cases") {
        let subject_id = case["subject_id"].as_str().expect("subject id");
        if request_ids.contains(subject_id) {
            cases
                .entry(subject_id.to_string())
                .or_insert_with(|| case.clone());
        }
    }
    cases
}

fn string_set(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|item| item.as_str().expect("string item").to_string())
        .collect()
}
