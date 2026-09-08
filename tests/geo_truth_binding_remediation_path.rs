#![forbid(unsafe_code)]

use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
};

const REMEDIATION: &str = "scripts/geo_measurements/fixtures/e4_truth_binding_remediation_path_2026-09-08/measurement.json";
const TRUTH_QUALITY: &str =
    "scripts/geo_measurements/fixtures/e4_truth_quality_residuals_2026-09-08/measurement.json";

#[test]
fn remediation_path_declares_non_score_non_truth_edit_boundary() {
    let measurement = read_json(REMEDIATION);

    assert_eq!(
        measurement["version"],
        "canon_geo_truth_binding_remediation_path.v0"
    );
    assert_eq!(measurement["bead"], "bd-ptuo");
    assert_eq!(measurement["source_audit_bead"], "bd-1hvb");
    assert_eq!(
        measurement["proof_class"],
        "retained_measurement_remediation_spec_not_live"
    );
    assert_eq!(measurement["release_claim_allowed"], false);
    assert_eq!(measurement["is_rescore"], false);
    assert_eq!(measurement["is_truth_fixture_edit"], false);
    assert_eq!(measurement["is_rho_change"], false);

    let denominator = &measurement["denominator"];
    assert_eq!(denominator["previous_frozen_denominator"], 79);
    assert_eq!(denominator["current_frozen_denominator"], 77);
    assert_eq!(denominator["ruling_commit"], "a0025b3");
    assert_eq!(denominator["evaluated_retained_subjects"], 70);
    assert_eq!(denominator["current_deficit"], 7);
    assert!(
        denominator["rule"]
            .as_str()
            .expect("denominator rule")
            .contains("both 79 and 77")
    );

    assert_eq!(
        movement_object(&measurement["expected_direct_e4_movement"]),
        zero_movement()
    );
    assert_eq!(measurement["summary"]["expected_direct_e4_movement"], 0);
    assert_eq!(measurement["summary"]["admission_gap_count"], 0);
    assert_eq!(measurement["summary"]["truth_row_binding_defect_count"], 5);
    assert_eq!(
        measurement["summary"]["new_acquisition_bead_required_now"],
        false
    );

    let exclusion_basis = &measurement["truth_exclusion_basis"];
    assert_eq!(exclusion_basis["reported_before_correction"], 8);
    assert_eq!(exclusion_basis["completed_classifications"], 6);
    assert_eq!(exclusion_basis["incomplete_classifications"], 2);
    assert_eq!(
        string_set(&exclusion_basis["incomplete_case_fragments"]),
        ["91b2df27", "a7dac634"]
            .into_iter()
            .map(str::to_string)
            .collect()
    );
}

#[test]
fn remediation_path_is_exactly_the_five_truth_row_binding_defects() {
    let remediation = read_json(REMEDIATION);
    let truth_quality = read_json(TRUTH_QUALITY);

    let remediation_cases = cases_by_fragment(&remediation);
    assert_eq!(
        remediation_cases.keys().cloned().collect::<BTreeSet<_>>(),
        ["5934bb93", "68a1a4ce", "91b2df27", "a7dac634", "e3a5228d"]
            .into_iter()
            .map(str::to_string)
            .collect()
    );

    let truth_quality_cases = cases_by_fragment(&truth_quality);
    let truth_row_defects = truth_quality_cases
        .iter()
        .filter(|&(_fragment, case)| case["classification"] == "truth_row_binding_defect")
        .map(|(fragment, _case)| fragment.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        remediation_cases.keys().cloned().collect::<BTreeSet<_>>(),
        truth_row_defects
    );

    for (fragment, case) in remediation_cases {
        let source_case = truth_quality_cases
            .get(&fragment)
            .expect("truth quality source case");
        assert_eq!(case["current_classification"], "truth_row_binding_defect");
        assert_eq!(case["wrong_side"], "truth_source_binding");
        assert_eq!(case["family"], source_case["family"]);
        assert_eq!(
            case["retained_acris_truth"]["document_id"],
            source_case["truth_document"]["document_id"]
        );
        assert_eq!(
            case["filed_sec_property"]["property_key"],
            source_case["filed_property_context"]["property_key"]
        );
        assert_eq!(
            movement_object(&case["expected_direct_e4_movement"]),
            zero_movement()
        );
    }
}

#[test]
fn remediation_path_routes_to_sec_bridge_not_franklin_or_rho() {
    let measurement = read_json(REMEDIATION);
    let sources = &measurement["candidate_sources"];

    assert_eq!(sources["sec_row_to_parcel_bridge"]["bead"], "bd-3io8");
    assert_eq!(sources["sec_row_to_parcel_bridge"]["applicable"], true);
    assert_eq!(
        sources["sec_row_to_parcel_bridge"]["required_before_score_change"],
        true
    );
    assert_eq!(
        sources["sec_row_to_parcel_bridge"]["settles_if_total_unambiguous"],
        true
    );
    assert_eq!(sources["franklin_county_recorder_deeds"]["bead"], "bd-o3fy");
    assert_eq!(
        sources["franklin_county_recorder_deeds"]["applicable"],
        false
    );
    assert!(
        sources["franklin_county_recorder_deeds"]["why"]
            .as_str()
            .expect("franklin reason")
            .contains("NYC H7 subjects")
    );
    assert_eq!(
        sources["acris_legal_rows"]["independent_precision_on_this_population"],
        false
    );

    for case in measurement["cases"].as_array().expect("cases") {
        let path = &case["settlement_path"];
        assert_eq!(path["primary_source"], "bd-3io8_sec_row_to_parcel_bridge");
        assert_eq!(
            path["franklin_recorder_deeds"],
            "not_applicable_nyc_subject"
        );
        assert!(
            path["required_result"]
                .as_str()
                .expect("required result")
                .contains("Bridge")
        );
        assert!(
            string_set(&path["required_fields"]).contains("parcel_ids"),
            "case {} must require parcel_ids",
            case["case_fragment"]
        );
        assert!(
            path["if_bridge_cannot_be_total"]
                .as_str()
                .expect("if bridge cannot be total")
                .contains("UNSETTLED")
        );
    }

    let forbidden = string_set(&measurement["handoff"]["do_not_do"]);
    assert!(forbidden.contains("Do not edit the retained truth fixture."));
    assert!(forbidden.contains("Do not rescore E4 from this artifact."));
    assert!(forbidden.contains("Do not widen rho admission to force agreement."));
}

#[test]
fn remediation_path_preserves_per_case_settling_rows() {
    let measurement = read_json(REMEDIATION);
    let cases = cases_by_fragment(&measurement);

    assert_eq!(
        cases["5934bb93"]["filed_sec_property"]["source_record_id"],
        "EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:d5ddd2d9-07dc-44d6-bf8b-b7bfc373dbc3:a89343223f12351e8cc666345560b9a8:CREP-6AB181363BFF721C:70fd303843624e59f9db34593c61b59f"
    );
    assert_eq!(
        cases["e3a5228d"]["filed_sec_property"]["source_record_id"],
        "EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:d5ddd2d9-07dc-44d6-bf8b-b7bfc373dbc3:d709ee238f8a21b5452734d5fd8c2de5:CREP-11B7C4F0999886E9:c82104d07f770869c71daeead548739e"
    );
    assert_eq!(
        cases["a7dac634"]["filed_sec_property"]["source_record_id"],
        "EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:d5ddd2d9-07dc-44d6-bf8b-b7bfc373dbc3:9970d9136018599001e61f2ba8be3519:CREP-2D511F8FD5A117D5:c84a649628639e9966cbe4a68263b5ed"
    );

    assert_eq!(
        cases["68a1a4ce"]["retained_acris_truth"]["legal_addresses"],
        serde_json::json!(["250 EAST 53RD STREET", "768 5 AVENUE"])
    );
    assert_eq!(
        cases["91b2df27"]["retained_acris_truth"]["legal_addresses"],
        serde_json::json!(["217 WEST 57TH STREET"])
    );

    assert!(
        cases["68a1a4ce"]["settlement_path"]["required_result"]
            .as_str()
            .expect("68a settlement")
            .contains("208 East 14th Street")
    );
    assert!(
        cases["91b2df27"]["settlement_path"]["required_result"]
            .as_str()
            .expect("91b settlement")
            .contains("5 East 22nd Street")
    );
}

fn read_json(path: &str) -> Value {
    let file = File::open(path).unwrap_or_else(|err| panic!("open {path}: {err}"));
    serde_json::from_reader(BufReader::new(file))
        .unwrap_or_else(|err| panic!("parse {path}: {err}"))
}

fn cases_by_fragment(document: &Value) -> BTreeMap<String, &Value> {
    document["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|case| {
            (
                case_fragment(case)
                    .unwrap_or_else(|| panic!("case without fragment: {case}"))
                    .to_string(),
                case,
            )
        })
        .collect()
}

fn case_fragment(case: &Value) -> Option<&str> {
    case.get("case_fragment")
        .and_then(Value::as_str)
        .or_else(|| case.get("short_id").and_then(Value::as_str))
        .or_else(|| {
            case.get("case_id")
                .and_then(Value::as_str)
                .and_then(|id| id.rsplit_once(':').map(|(_, fragment)| &fragment[..8]))
        })
}

fn string_set(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|item| item.as_str().expect("string item").to_string())
        .collect()
}

fn movement_object(value: &Value) -> BTreeMap<String, i64> {
    value
        .as_object()
        .expect("movement object")
        .iter()
        .map(|(key, value)| (key.clone(), value.as_i64().expect("movement integer")))
        .collect()
}

fn zero_movement() -> BTreeMap<String, i64> {
    [
        ("ambiguous_delta", 0),
        ("conflict_delta", 0),
        ("exactly_correct_delta", 0),
        ("false_merges_delta", 0),
        ("resolved_delta", 0),
        ("truth_exclusions_delta", 0),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value))
    .collect()
}
