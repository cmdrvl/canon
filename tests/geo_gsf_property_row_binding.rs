#![forbid(unsafe_code)]

use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

const MEASUREMENT: &str =
    "scripts/geo_measurements/fixtures/e4_gsf_property_row_binding_2026-09-08/measurement.json";

#[test]
fn gsf_property_row_binding_handoff_declares_denominator_ruling_and_expected_movement() {
    let measurement = read_json(MEASUREMENT);

    assert_eq!(
        measurement["version"],
        "canon_geo_gsf_property_row_binding_fix_handoff.v0"
    );
    assert_eq!(measurement["bead"], "bd-35t1");
    assert_eq!(
        measurement["proof_class"],
        "retained_population_measurement_not_live"
    );
    assert_eq!(measurement["release_claim_allowed"], false);
    assert_eq!(measurement["is_rescore"], false);
    assert_eq!(measurement["frozen_denominator"], 77);
    assert_eq!(
        measurement["denominator_ruling"]["previous_frozen_denominator"],
        79
    );
    assert_eq!(
        measurement["denominator_ruling"]["ruling_commit"],
        "a0025b3"
    );
    assert_eq!(measurement["retained_population_denominator"], 70);
    assert_eq!(measurement["retained_population_deficit"], 7);
    assert_eq!(
        measurement["baseline"]["truth_exclusions"]["previously_reported_total_against_79"],
        8
    );
    assert_eq!(measurement["baseline"]["truth_exclusions"]["completed"], 6);
    assert_eq!(measurement["baseline"]["truth_exclusions"]["incomplete"], 2);
    assert_eq!(
        measurement["predeclared_expected_e4_movement"]["completed_truth_exclusions_delta"],
        -1
    );
    assert_eq!(
        measurement["predeclared_expected_e4_movement"]["expected_after"]["truth_exclusions"]["completed"],
        5
    );
    assert_eq!(
        measurement["predeclared_expected_e4_movement"]["expected_after"]["truth_exclusions"]["incomplete"],
        2
    );
    assert_eq!(
        measurement["retained_inputs"]["gsf_conflict_adjudication_path"],
        "scripts/geo_measurements/fixtures/e4_gsf_conflict_adjudication_2026-09-08/measurement.json"
    );
    assert_eq!(
        measurement["retained_inputs"]["gsf_value_vector_rebuild_path"],
        "scripts/geo_measurements/fixtures/e4_gsf_value_vector_rebuild_2026-09-08/measurement.json"
    );
    assert!(
        measurement["retained_inputs"]["hash_policy"]
            .as_str()
            .expect("hash policy")
            .contains("denominator restatement sweep owns these prior measurement artifacts")
    );
}

#[test]
fn gsf_property_row_binding_handoff_names_the_generic_same_loan_defect() {
    let measurement = read_json(MEASUREMENT);
    let defect = &measurement["defect"];

    assert_eq!(defect["case_fragment"], "e1ee0535");
    assert_eq!(defect["loan_key"], "6bfe47de21ff7d7e24bf6464871dea9f");
    assert_eq!(defect["acris_document_id"], "2019050100411003");
    assert_eq!(
        defect["wrongly_bound_property_row"]["property_key"],
        "CREP-5F8AE5E0FA6B5506"
    );
    assert_eq!(
        defect["wrongly_bound_property_row"]["document_address_coverage"],
        "0/3"
    );
    assert_eq!(
        defect["selected_property_row"]["property_key"],
        "CREP-72177B208235E832"
    );
    assert_eq!(
        defect["selected_property_row"]["document_address_coverage"],
        "3/3"
    );
    assert_eq!(defect["selected_property_row"]["size_measure"], "UNITS");
    assert!(
        defect["binding_rule"]
            .as_str()
            .expect("binding rule")
            .contains("unique row whose filed address covers every bound document legal address")
    );
}

#[test]
fn gsf_property_row_binding_handoff_keeps_other_exclusions_out_of_scope() {
    let measurement = read_json(MEASUREMENT);
    let check = &measurement["other_seven_binding_path_check"];

    assert_eq!(
        check["result"],
        "e1ee0535 is the only remaining exclusion affected by this GSF property-row rebinding path"
    );
    let fragments = check["cases"]
        .as_array()
        .expect("other cases")
        .iter()
        .map(|case| {
            assert_eq!(case["affects_binding_path"], false);
            case["case_fragment"]
                .as_str()
                .expect("case fragment")
                .to_string()
        })
        .collect::<BTreeSet<_>>();

    assert_eq!(
        fragments,
        BTreeSet::from([
            "3899edce".to_string(),
            "5934bb93".to_string(),
            "68a1a4ce".to_string(),
            "69dbf5da".to_string(),
            "91b2df27".to_string(),
            "a7dac634".to_string(),
            "e3a5228d".to_string(),
        ])
    );
    assert_eq!(
        measurement["score_handoff"]["status"],
        "READY_TO_SCORE_by_bd_1g4x_not_scored_by_bd_35t1"
    );
    assert_eq!(measurement["score_handoff"]["release_claim_allowed"], false);
}

fn read_json(relative: &str) -> Value {
    let file = File::open(repo_path(relative)).expect("open JSON fixture");
    serde_json::from_reader(BufReader::new(file)).expect("parse JSON fixture")
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
