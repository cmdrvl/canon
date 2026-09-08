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
    "scripts/geo_measurements/fixtures/e4_truth_quality_residuals_2026-09-08/measurement.json";
const OWNER: &str =
    "scripts/geo_measurements/fixtures/e4_owner_source_residuals_2026-09-08/measurement.json";
const PAD_ADJUDICATION: &str =
    "scripts/geo_measurements/fixtures/e4_pad_residual_adjudication_2026-09-08/measurement.json";
const PAD_REVIEW: &str = "scripts/geo_measurements/fixtures/e4_pad_truth_binding_2026-09-08/pad_truth_binding_review.json";
const GSF_CONFLICT: &str =
    "scripts/geo_measurements/fixtures/e4_gsf_conflict_adjudication_2026-09-08/measurement.json";
const COMPLETENESS_SOURCE_HUNT: &str =
    "scripts/geo_measurements/fixtures/e4_completeness_source_hunt_2026-09-08/source_hunt.json";

#[test]
fn truth_quality_audit_declares_non_rescore_boundary_and_counts() {
    let measurement = read_json(MEASUREMENT);

    assert_eq!(
        measurement["version"],
        "canon_geo_truth_quality_residual_audit.v0"
    );
    assert_eq!(measurement["bead"], "bd-1hvb");
    assert_eq!(
        measurement["proof_class"],
        "retained_cmdrvl_data_warehouse_truth_quality_audit_not_live"
    );
    assert_eq!(measurement["release_claim_allowed"], false);
    assert_eq!(measurement["is_rescore"], false);
    assert_eq!(measurement["frozen_denominator"], 79);
    assert_eq!(measurement["retained_population_denominator"], 70);

    assert_eq!(measurement["baseline"]["resolved"], 7);
    assert_eq!(measurement["baseline"]["exactly_correct"], 7);
    assert_eq!(measurement["baseline"]["false_merges"], 0);
    assert_eq!(measurement["baseline"]["truth_exclusions"], 8);
    assert_eq!(
        measurement["baseline"]["reach_full_partial_none"],
        Value::from(vec![70, 0, 0])
    );
    assert_eq!(
        measurement["expected_direct_e4_movement"],
        json_object([
            ("ambiguous_delta", 0),
            ("conflict_delta", 0),
            ("exactly_correct_delta", 0),
            ("false_merges_delta", 0),
            ("resolved_delta", 0),
            ("truth_exclusions_delta", 0),
        ])
    );

    assert_eq!(measurement["measurement"]["case_count"], 8);
    assert_eq!(measurement["measurement"]["admission_gap_count"], 0);
    assert_eq!(
        measurement["measurement"]["truth_rows_likely_wrong_for_filed_property_count"],
        5
    );
    assert_eq!(
        measurement["measurement"]["canon_property_row_binding_defect_count"],
        1
    );
    assert_eq!(
        measurement["measurement"]["truth_document_independently_supported_for_scope_count"],
        2
    );
    assert_eq!(measurement["measurement"]["unsettled_count"], 0);
    assert_eq!(
        measurement["measurement"]["classification_counts"],
        json_object([
            ("canon_contract_binding_defect", 1),
            ("independently_supported_truth", 2),
            ("truth_row_binding_defect", 5),
            ("unsettled", 0),
        ])
    );

    let cases = cases_by_fragment(&measurement);
    assert_eq!(
        cases.keys().cloned().collect::<BTreeSet<_>>(),
        [
            "3899edce", "5934bb93", "68a1a4ce", "69dbf5da", "91b2df27", "a7dac634", "e1ee0535",
            "e3a5228d",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    );
    for case in cases.values() {
        assert_eq!(case["admission_gap"], false);
        assert_eq!(
            case["expected_direct_e4_movement"],
            measurement["expected_direct_e4_movement"]
        );
        assert_eq!(case["settlement"]["release_claim_allowed"], false);
    }
}

#[test]
fn truth_quality_audit_matches_the_family_adjudications() {
    let measurement = read_json(MEASUREMENT);
    let owner = read_json(OWNER);
    let pad_adjudication = read_json(PAD_ADJUDICATION);
    let pad_review = read_json(PAD_REVIEW);
    let gsf_conflict = read_json(GSF_CONFLICT);

    let cases = cases_by_fragment(&measurement);
    let owner_cases = cases_by_fragment(&owner);
    for fragment in ["3899edce", "69dbf5da"] {
        let case = cases.get(fragment).expect("owner residual case");
        let prior = owner_cases.get(fragment).expect("owner source case");
        assert_eq!(case["family"], "owner_exact");
        assert_eq!(case["classification"], "independently_supported_truth");
        assert_eq!(
            case["source_family_classification"],
            prior["classification"]
        );
        assert_eq!(
            case["truth_document"]["document_id"],
            prior["completeness"]["document_id"]
        );
    }

    let pad_cases = cases_by_fragment(&pad_adjudication);
    let pad_review_cases = review_cases_by_fragment(&pad_review);
    for fragment in ["5934bb93", "e3a5228d", "a7dac634"] {
        let case = cases.get(fragment).expect("PAD residual case");
        let prior = pad_cases.get(fragment).expect("PAD adjudication case");
        let review = pad_review_cases.get(fragment).expect("PAD review case");
        assert_eq!(case["family"], "pad_membership");
        assert_eq!(case["classification"], "truth_row_binding_defect");
        assert_eq!(case["wrong_side"], "truth_source_binding");
        assert_eq!(
            case["source_family_classification"],
            prior["settlement"]["classification"]
        );
        assert_eq!(
            case["filed_property_context"]["address"],
            review["filed_property"]["address"]
        );
        assert_eq!(case["truth_document"]["document_id"], review["document_id"]);
        assert_eq!(
            case["filed_property_context"]["filed_truth_overlap"],
            "none"
        );
    }

    let gsf_cases = cases_by_fragment(&gsf_conflict);
    for fragment in ["68a1a4ce", "91b2df27", "e1ee0535"] {
        let case = cases.get(fragment).expect("GSF residual case");
        let prior = gsf_cases.get(fragment).expect("GSF conflict case");
        assert_eq!(case["family"], "gsf_band");
        assert_eq!(
            case["source_family_classification"],
            prior["settlement"]["classification"]
        );
        assert_eq!(case["wrong_side"], prior["settlement"]["wrong_side"]);
        assert_eq!(
            case["truth_document"]["document_id"],
            prior["acris_document_binding"]["document_id"]
        );
    }
    assert_eq!(
        cases["e1ee0535"]["classification"],
        "canon_contract_binding_defect"
    );
    assert_eq!(
        cases["68a1a4ce"]["classification"],
        "truth_row_binding_defect"
    );
    assert_eq!(
        cases["91b2df27"]["classification"],
        "truth_row_binding_defect"
    );
}

#[test]
fn truth_quality_audit_preserves_h7_truth_provenance_label() {
    let measurement = read_json(MEASUREMENT);
    let source_hunt = read_json(COMPLETENESS_SOURCE_HUNT);

    assert_eq!(
        source_hunt["truth_derivation_trace"]["all_retained_truth_bbls_acris_derived"],
        true
    );
    assert_eq!(
        source_hunt["truth_derivation_trace"]["independent_acris_precision_subjects"],
        0
    );
    assert!(
        measurement["truth_selection_decision"]["proof_boundary"]
            .as_str()
            .expect("proof boundary")
            .contains("must not be cited as independent precision")
    );
    assert!(
        measurement["truth_selection_decision"]["decision_rule"]
            .as_str()
            .expect("decision rule")
            .contains("accepted only when exactly one legal-confirmed ACRIS document remains")
    );

    let source_queries = measurement["source_verification"]["queries"]
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
    assert_eq!(
        source_queries["bd_1hvb_acris_master_rows_for_eight_truth_documents"],
        8
    );
    assert_eq!(
        source_queries["bd_1hvb_acris_legal_sets_for_eight_truth_documents"],
        8
    );
    assert_eq!(source_queries["bd_1hvb_round_truth_lender_party_rows"], 8);
    assert_eq!(
        source_queries["bd_1hvb_sec_property_rows_for_eight_truth_quality_audit"],
        14
    );
    assert_eq!(
        source_queries["bd_1hvb_h7_selector_lip_matches_for_owner_truth_documents"],
        5
    );

    let cases = cases_by_fragment(&measurement);
    for fragment in ["69dbf5da", "91b2df27", "a7dac634"] {
        assert_eq!(
            cases[fragment]["truth_document"]["selector_basis"],
            "round amount/date/legal-borough unique document plus lender party_name_norm match"
        );
        assert_non_empty_str(&cases[fragment]["truth_document"]["lender_party_name_norm"]);
    }
    for fragment in ["3899edce", "5934bb93", "68a1a4ce", "e1ee0535", "e3a5228d"] {
        assert_eq!(
            cases[fragment]["truth_document"]["selector_basis"],
            "amount/date/legal-borough unique document"
        );
    }
}

#[test]
fn truth_quality_audit_separates_truth_row_and_canon_binding_failures() {
    let measurement = read_json(MEASUREMENT);
    let cases = cases_by_fragment(&measurement);

    for fragment in ["5934bb93", "e3a5228d", "a7dac634", "68a1a4ce", "91b2df27"] {
        let case = cases.get(fragment).expect("truth binding defect");
        assert_eq!(case["classification"], "truth_row_binding_defect");
        assert_eq!(case["wrong_side"], "truth_source_binding");
        assert_eq!(
            case["filed_property_context"]["filed_truth_overlap"],
            "none"
        );
        assert!(
            case["settlement"]["verdict"]
                .as_str()
                .expect("verdict")
                .contains("truth")
        );
    }

    let regency = &cases["e1ee0535"];
    assert_eq!(regency["classification"], "canon_contract_binding_defect");
    assert_eq!(regency["wrong_side"], "canon_property_row_binding");
    assert_eq!(
        regency["filed_property_context"]["target_property_key"],
        "CREP-5F8AE5E0FA6B5506"
    );
    let matching_keys = regency["filed_property_context"]["same_loan_matching_property_rows"]
        .as_array()
        .expect("same-loan matching rows")
        .iter()
        .map(|row| row["property_key"].as_str().expect("property key"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        matching_keys,
        [
            "CREP-0C97A73C45D5CD7C",
            "CREP-5F29F9FB87795B4A",
            "CREP-72177B208235E832",
        ]
        .into_iter()
        .collect()
    );

    let patchen = &cases["3899edce"];
    assert_eq!(patchen["classification"], "independently_supported_truth");
    assert!(
        patchen["settlement"]["verdict"]
            .as_str()
            .expect("verdict")
            .contains("owner-source spelling conflict")
    );

    let patin = &cases["69dbf5da"];
    assert_eq!(patin["classification"], "independently_supported_truth");
    assert!(
        patin["settlement"]["verdict"]
            .as_str()
            .expect("verdict")
            .contains("current FY2026 owner row")
    );
}

#[test]
fn truth_quality_audit_pins_retained_inputs() {
    let measurement = read_json(MEASUREMENT);
    for (path_field, sha_field) in [
        (
            "owner_source_residual_measurement_path",
            "owner_source_residual_measurement_sha256",
        ),
        (
            "pad_residual_adjudication_path",
            "pad_residual_adjudication_sha256",
        ),
        (
            "pad_truth_binding_review_path",
            "pad_truth_binding_review_sha256",
        ),
        (
            "gsf_residual_measurement_path",
            "gsf_residual_measurement_sha256",
        ),
        (
            "gsf_conflict_adjudication_path",
            "gsf_conflict_adjudication_sha256",
        ),
        (
            "completeness_source_hunt_path",
            "completeness_source_hunt_sha256",
        ),
        (
            "acris_completeness_overlay_path",
            "acris_completeness_overlay_sha256",
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

fn review_cases_by_fragment(review: &Value) -> BTreeMap<String, &Value> {
    review["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|case| {
            (
                case["short_id"].as_str().expect("short id").to_string(),
                case,
            )
        })
        .collect()
}

fn json_object<const N: usize>(pairs: [(&str, i64); N]) -> Value {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), Value::from(value)))
        .collect::<serde_json::Map<_, _>>()
        .into()
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
