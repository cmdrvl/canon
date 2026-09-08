#![forbid(unsafe_code)]

use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::Path,
};

const REVIEW: &str = "scripts/geo_measurements/fixtures/e4_pad_truth_binding_2026-09-08/pad_truth_binding_review.json";

const EXPECTED_CASES: [&str; 3] = [
    "h7-subject:non-round:5934bb9370d31d9b1fa687544020e5e006512cb16d073ed3712ba0cf37a17c36",
    "h7-subject:non-round:e3a5228d84bb6bf01ff03e2849e4a996f223cb1fcff79eff04a65d84dcfa8deb",
    "h7-subject:round-exact-lender:a7dac634b44cc8175e7bde5b0a386f2ecda11361200f3b04bce5a3b84aa73ee3",
];

#[test]
fn pad_truth_binding_review_keeps_three_gap_denominator_and_no_overlay() {
    let review = read_json(REVIEW);
    assert_eq!(
        review["version"],
        "canon_geo_e4_pad_truth_binding_review.v0"
    );
    assert_eq!(review["bead"], "bd-1hgq");
    assert_eq!(
        review["proof_class"],
        "retained_cmdrvl_data_warehouse_source_audit_not_live"
    );
    assert_eq!(review["release_claim_allowed"], false);
    assert_eq!(review["frozen_denominator"], 77);
    assert_eq!(review["reported_frozen_denominator"], 79);
    assert_eq!(review["retained_population_deficit"], 7);
    assert_eq!(review["retained_population_denominator"], 70);
    assert_eq!(review["denominator"]["case_count"], 3);
    assert_eq!(
        string_set(&review["denominator"]["case_ids"]),
        EXPECTED_CASES.iter().map(|id| id.to_string()).collect()
    );
    assert_eq!(
        review["measurement"]["classifications"]["same_collateral_property"],
        0
    );
    assert_eq!(
        review["measurement"]["classifications"]["different_property"],
        3
    );
    assert_eq!(review["measurement"]["classifications"]["needs_review"], 0);
    assert_eq!(
        review["measurement"]["expected_direct_e4_movement_from_this_review"]["resolved"],
        0
    );
    assert!(
        review["boundary"]
            .as_str()
            .expect("boundary")
            .contains("do not widen rho.address.pad.membership")
    );
    assert!(
        review.get("admission_overlay_handoff").is_none(),
        "bd-1hgq is a review classification, not an admission overlay"
    );
}

#[test]
fn pad_truth_binding_cases_are_source_pinned_and_different_property() {
    let review = read_json(REVIEW);
    assert_source_terms(&review);
    assert_source_queries(&review);

    let cases = cases_by_id(&review);
    assert_eq!(
        cases.keys().cloned().collect::<BTreeSet<_>>(),
        EXPECTED_CASES.iter().map(|id| id.to_string()).collect()
    );

    for case in cases.values() {
        assert_eq!(case["classification"], "different_property");
        assert_non_empty_str(&case["loan_key"]);
        assert_non_empty_str(&case["property_key"]);
        assert_non_empty_str(&case["document_id"]);
        assert_non_empty_str(&case["filed_property"]["address"]);
        assert_non_empty_str(&case["filed_property"]["city"]);
        assert_non_empty_str(&case["filed_property"]["county"]);
        assert_non_empty_str(&case["filed_property"]["zip"]);
        assert_non_empty_str(&case["filed_property"]["source_record"]["source_record_id"]);
        assert_non_empty_str(&case["filed_property"]["source_record"]["record_blake3"]);

        let truth = string_set(&case["truth_bbls"]);
        let emitted = string_set(&case["emitted_pad_hard_members"]);
        assert!(
            truth.is_disjoint(&emitted),
            "{} must remain a PAD-disjoint binding-gap case",
            case["short_id"].as_str().expect("short id")
        );
        assert!(
            !case["pad"]["emitted_hard_member_rows"]
                .as_array()
                .expect("emitted rows")
                .is_empty()
        );
        assert!(
            !case["acris"]["legal_rows"]
                .as_array()
                .expect("legal rows")
                .is_empty()
        );
        assert!(
            !case["acris"]["party_rows"]
                .as_array()
                .expect("party rows")
                .is_empty()
        );
        assert_non_empty_str(&case["acris"]["master"]["raw_csv_sha256"]);
        assert_non_empty_str(&case["acris"]["master"]["parser_version"]);

        let coverage = &case["pad"]["truth_bbl_coverage"];
        assert_eq!(
            coverage["truth_bbl_count"].as_u64().expect("truth count"),
            truth.len() as u64
        );
        assert!(
            coverage["pad_address_exact_bbl_count"]
                .as_u64()
                .expect("PAD address coverage")
                + coverage["missing_pad_truth_bbls"]
                    .as_array()
                    .expect("missing PAD BBLs")
                    .len() as u64
                <= truth.len() as u64
        );
        assert_eq!(case["filed_truth_relation"]["overlap"], "none");
    }

    assert_case_coverage(&cases);
}

fn assert_case_coverage(cases: &BTreeMap<String, Value>) {
    let beach = cases
        .values()
        .find(|case| case["short_id"] == "5934bb93")
        .expect("5934bb93 case");
    assert_eq!(
        string_set(&beach["pad"]["truth_bbl_coverage"]["missing_pad_truth_bbls"]),
        ["4159100064".to_string()].into_iter().collect()
    );
    assert_eq!(
        beach["filed_truth_relation"]["pad_emitted_address_family"],
        "35 AVENUE / 83 STREET, Jackson Heights"
    );
    assert_eq!(
        beach["filed_truth_relation"]["pad_truth_address_family"],
        "BEACH 65 STREET, Far Rockaway"
    );

    let northern = cases
        .values()
        .find(|case| case["short_id"] == "e3a5228d")
        .expect("e3a5228d case");
    assert_eq!(
        northern["pad"]["truth_bbl_coverage"]["pad_bbl_range_bbl_count"],
        10
    );
    assert_eq!(
        northern["filed_truth_relation"]["pad_emitted_address_family"],
        "QUEENS BOULEVARD / 47 STREET, Sunnyside"
    );
    assert_eq!(
        northern["filed_truth_relation"]["pad_truth_address_family"],
        "NORTHERN BOULEVARD, Flushing"
    );

    let east_sixtieth = cases
        .values()
        .find(|case| case["short_id"] == "a7dac634")
        .expect("a7dac634 case");
    assert_eq!(
        east_sixtieth["filed_property"]["loan_property_count"], 2,
        "loan-grain multi-property context must stay visible"
    );
    assert_eq!(
        east_sixtieth["pad"]["truth_bbl_coverage"]["pad_address_exact_bbl_count"],
        7
    );
    assert_eq!(
        east_sixtieth["filed_truth_relation"]["pad_emitted_address_family"],
        "WEST 40 STREET, Times Square"
    );
    assert_eq!(
        east_sixtieth["filed_truth_relation"]["pad_truth_address_family"],
        "EAST 60 STREET / 1 AVENUE"
    );
}

fn assert_source_terms(review: &Value) {
    for key in ["property_mart", "acris", "pad"] {
        assert_non_empty_str(&review["source_terms"][key]["license_terms"]);
        assert_non_empty_str(&review["source_terms"][key]["attribution_text"]);
    }
}

fn assert_source_queries(review: &Value) {
    let query_names = review["source_queries"]
        .as_array()
        .expect("source queries")
        .iter()
        .map(|query| query["measurement_name"].as_str().expect("query name"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        query_names,
        [
            "bd_1hgq_acris_legal_pins",
            "bd_1hgq_acris_master_pins",
            "bd_1hgq_acris_party_pins_staged",
            "bd_1hgq_loan_property_rows",
            "bd_1hgq_pad_address_exact_rows",
            "bd_1hgq_pad_bbl_range_rows",
        ]
        .into_iter()
        .collect()
    );
}

fn cases_by_id(review: &Value) -> BTreeMap<String, Value> {
    review["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|case| {
            (
                case["case_id"].as_str().expect("case id").to_string(),
                case.clone(),
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

fn assert_non_empty_str(value: &Value) {
    assert!(
        !value.as_str().expect("string").is_empty(),
        "expected non-empty string"
    );
}

fn read_json(relative: &str) -> Value {
    let file = File::open(repo_path(relative)).expect("open JSON fixture");
    serde_json::from_reader(BufReader::new(file)).expect("parse JSON fixture")
}

fn repo_path(relative: &str) -> impl AsRef<Path> {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
