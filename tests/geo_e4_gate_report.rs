#![forbid(unsafe_code)]

use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
};

const REPORT: &str = "scripts/geo_measurements/fixtures/e4_gate_report_2026-09-08/measurement.json";

#[test]
fn e4_gate_report_restates_the_denominator_ruling_without_silent_swap() {
    let report = read_json(REPORT);

    assert_eq!(report["version"], "canon_geo_e4_gate_report.v0");
    assert_eq!(report["bead"], "bd-1g4x");
    assert_eq!(report["release_claim_allowed"], false);
    assert_eq!(report["denominator"]["current_frozen_e4_subjects"], 77);
    assert_eq!(
        report["denominator"]["previous_reported_frozen_e4_subjects"],
        79
    );
    assert_eq!(report["denominator"]["evaluated_subjects"], 70);
    assert_eq!(report["denominator"]["deficit_subjects"], 7);
    assert_eq!(
        report["denominator"]["previous_reported_deficit_subjects"],
        9
    );
    assert_eq!(report["denominator"]["blocked_on_us_subjects"], 0);
    assert_eq!(report["denominator"]["ruling"]["commit"], "a0025b3");
    assert_eq!(report["denominator"]["ruling"]["issued_by"], "Zac");
    assert_eq!(
        string_set(&report["denominator"]["ruling"]["excluded_case_fragments"]),
        BTreeSet::from([
            "6eb465fd908bc59f".to_string(),
            "aacff3c254d36ae6".to_string(),
        ])
    );
    assert_eq!(
        string_set(&report["denominator"]["ruling"]["kept_case_fragments"]),
        BTreeSet::from(["ced7ad9f0d74abf7".to_string()])
    );

    for ratio in report["restated_ratios"].as_array().expect("ratios") {
        assert_eq!(ratio["current_denominator"], 77);
        assert_eq!(ratio["previous_denominator"], 79);
        assert!(
            ratio["denominator_change_note"]
                .as_str()
                .expect("denominator note")
                .contains("79 to 77")
                || ratio["denominator_change_note"]
                    .as_str()
                    .expect("denominator note")
                    .contains("not independent precision")
        );
    }
}

#[test]
fn e4_gate_report_keeps_current_score_and_completeness_mechanism_separate() {
    let report = read_json(REPORT);
    let current = &report["current_non_definitional_score"];
    let mechanism = &report["acris_completeness_mechanism_check"];

    assert_eq!(current["proof_class"], "retained_not_live");
    assert_eq!(
        mechanism["proof_class"],
        "retained_definitional_on_this_population_not_independent_precision"
    );
    assert_eq!(current["release_claim_allowed"], false);
    assert_eq!(mechanism["release_claim_allowed"], false);
    assert_eq!(mechanism["precision_claim_allowed"], false);
    assert!(
        mechanism["mechanism_claim"]
            .as_str()
            .expect("mechanism claim")
            .contains("not independent precision")
    );

    assert_eq!(current["planes"]["reconciliation"]["resolved_cases"], 7);
    assert_eq!(
        current["planes"]["truth_quality"]["exactly_correct_cases"],
        7
    );
    assert_eq!(mechanism["planes"]["reconciliation"]["resolved_cases"], 54);
    assert_eq!(
        mechanism["planes"]["truth_quality"]["exactly_correct_cases"],
        54
    );

    for plane in [
        "coverage",
        "candidate_reach",
        "rho_admission",
        "solver_exactness",
        "reconciliation",
        "truth_quality",
        "cost",
    ] {
        assert!(
            current["planes"][plane].is_object(),
            "{plane} current plane"
        );
        assert!(
            mechanism["planes"][plane].is_object(),
            "{plane} mechanism plane"
        );
    }
}

#[test]
fn e4_gate_report_splits_completed_exclusions_from_incomplete_classification() {
    let report = read_json(REPORT);
    let rho = &report["current_non_definitional_score"]["planes"]["rho_admission"];

    assert_eq!(rho["reported_truth_exclusions_before_bd_3cez"], 8);
    assert_eq!(rho["completed_truth_exclusion_cases"], 6);
    assert_eq!(rho["truth_classification_incomplete_cases"], 2);
    assert!(rho.get("truth_exclusions").is_none());
    assert_eq!(
        string_set(&rho["truth_classification_incomplete_case_fragments"]),
        BTreeSet::from(["91b2df27".to_string(), "a7dac634".to_string()])
    );

    let mechanism = &report["acris_completeness_mechanism_check"];
    assert_eq!(
        mechanism["planes"]["truth_quality"]["solver_truth_exclusion_cases"],
        6
    );
    assert_eq!(
        mechanism["planes"]["truth_quality"]["solver_truth_classification_incomplete_cases"],
        10
    );
    assert_eq!(mechanism["corrected_conflict_count"], 6);
    assert!(
        mechanism["corrected_conflict_count_note"]
            .as_str()
            .expect("conflict note")
            .contains("did not finish classifying")
    );
}

#[test]
fn e4_gate_report_names_claim_classes_and_truth_quality_boundary() {
    let report = read_json(REPORT);
    assert_eq!(
        report["current_non_definitional_score"]["entity_grain"],
        "parcel_and_building_entity_grain_per_PLAN_CANON_GEO_L5_not_ledger_key_grain"
    );

    let claim_classes = rows_by_key(
        report["claim_classes"]["section_10_2"]
            .as_array()
            .expect("claim classes"),
        "claim_class",
    );
    assert_eq!(
        claim_classes["HARD_FORCED"]["current_non_definitional_count"],
        7
    );
    assert_eq!(
        claim_classes["HARD_FORCED"]["acris_completeness_mechanism_count"],
        54
    );
    assert_eq!(
        claim_classes["SOFT_RANKED"]["current_non_definitional_count"],
        55
    );
    assert_eq!(
        claim_classes["FALLBACK"]["current_non_definitional_count"],
        8
    );
    assert_eq!(
        claim_classes["FALLBACK"]["acris_completeness_mechanism_count"],
        10
    );
    for row in claim_classes.values() {
        assert_eq!(row["promotion_allowed"], false);
    }

    let audit = &report["truth_quality_audit"]["classification_counts"];
    assert_eq!(audit["admission_gap"], 0);
    assert_eq!(audit["truth_row_binding_defect"], 5);
    assert_eq!(audit["canon_property_row_binding_defect"], 1);
    assert_eq!(
        audit["independently_supported_truth_source_current_owner_divergence"],
        2
    );
    assert_eq!(audit["unsettled"], 0);
}

#[test]
fn e4_gate_report_states_what_will_not_close_e4() {
    let report = read_json(REPORT);
    let blockers = report["what_closes_e4"].as_array().expect("blockers");
    let blocker_names = blockers
        .iter()
        .map(|row| row["blocker"].as_str().expect("blocker").to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        blocker_names,
        BTreeSet::from([
            "canon_property_row_binding".to_string(),
            "denominator_deficit".to_string(),
            "independent_precision".to_string(),
            "truth_quality".to_string(),
        ])
    );

    let non_closers = report["what_will_not_close_e4"]
        .as_array()
        .expect("non closers")
        .iter()
        .map(|value| value.as_str().expect("non closer").to_string())
        .collect::<Vec<_>>();
    assert!(
        non_closers
            .iter()
            .any(|line| line.contains("Editing retained truth rows"))
    );
    assert!(
        non_closers
            .iter()
            .any(|line| line.contains("ACRIS completeness 54/77"))
    );
    assert!(
        non_closers
            .iter()
            .any(|line| line.contains("Raising component budgets"))
    );
    assert!(
        non_closers
            .iter()
            .any(|line| line.contains("More Canon-only solver or admission work"))
    );
}

fn read_json(path: &str) -> Value {
    serde_json::from_reader(BufReader::new(File::open(path).expect(path))).expect(path)
}

fn string_set(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item.as_str().expect("string").to_string())
        .collect()
}

fn rows_by_key<'a>(rows: &'a [Value], key: &str) -> BTreeMap<&'a str, &'a Value> {
    rows.iter()
        .map(|row| (row[key].as_str().expect(key), row))
        .collect()
}
