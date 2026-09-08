#![forbid(unsafe_code)]

use serde_json::Value;
use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

const INCOMPLETE_CASES: [&str; 2] = ["91b2df27", "a7dac634"];

const TRUTH_EXCLUSION_RESTATEMENT_FILES: [&str; 5] = [
    "scripts/geo_measurements/fixtures/e4_gsf_residuals_2026-09-08/measurement.json",
    "scripts/geo_measurements/fixtures/e4_owner_source_residuals_2026-09-08/measurement.json",
    "scripts/geo_measurements/fixtures/e4_pad_residual_adjudication_2026-09-08/measurement.json",
    "scripts/geo_measurements/fixtures/e4_gsf_conflict_adjudication_2026-09-08/measurement.json",
    "scripts/geo_measurements/fixtures/e4_truth_quality_residuals_2026-09-08/measurement.json",
];

const DENOMINATOR_RESTATEMENT_FILES: [&str; 15] = [
    "scripts/geo_measurements/fixtures/e4_pad_truth_binding_2026-09-08/pad_truth_binding_review.json",
    "scripts/geo_measurements/fixtures/e4_gsf_property_type_bands_2026-09-08/measurement.json",
    "scripts/geo_measurements/fixtures/e4_owner_source_residuals_2026-09-08/measurement.json",
    "scripts/geo_measurements/fixtures/e4_gsf_residuals_2026-09-08/measurement.json",
    "scripts/geo_measurements/fixtures/e4_owner_normalization_2026-09-08/measurement.json",
    "scripts/geo_measurements/fixtures/e4_gsf_value_vector_rebuild_2026-09-08/measurement.json",
    "scripts/geo_measurements/fixtures/e4_truth_quality_residuals_2026-09-08/measurement.json",
    "scripts/geo_measurements/fixtures/e4_collateral_completeness_2026-09-08/measurement.json",
    "scripts/geo_measurements/fixtures/e4_completeness_source_hunt_2026-09-08/source_hunt.json",
    "scripts/geo_measurements/fixtures/e4_pad_residual_adjudication_2026-09-08/measurement.json",
    "scripts/geo_measurements/fixtures/e4_gsf_conflict_adjudication_2026-09-08/measurement.json",
    "scripts/geo_measurements/fixtures/e4_gsf_property_row_binding_2026-09-08/measurement.json",
    "scripts/geo_measurements/fixtures/e4_rho_admission_policy_2026-09-08/measurement.json",
    "scripts/geo_measurements/fixtures/e4_denominator_deficit_2026-09-08/measurement.json",
    "operator.json",
];

#[test]
fn restated_truth_exclusion_reports_carry_incomplete_bucket() {
    for path in TRUTH_EXCLUSION_RESTATEMENT_FILES {
        let report = read_json(path);
        let baseline = report
            .get("baseline")
            .unwrap_or_else(|| panic!("{path} missing baseline"));
        assert_truth_exclusion_accounting(path, baseline).expect("truth exclusion accounting");
        assert_eq!(
            report["release_claim_allowed"], false,
            "{path} must remain retained-only"
        );
    }
}

#[test]
fn truth_exclusion_report_without_incomplete_count_is_rejected() {
    let stale = serde_json::json!({
        "truth_exclusions": 8,
        "truth_exclusions_completed": 8,
        "reported_truth_exclusions": 8
    });
    let error = assert_truth_exclusion_accounting("stale", &stale).expect_err("stale shape fails");
    assert!(error.contains("truth_classification_incomplete"), "{error}");
}

#[test]
fn restated_denominator_reports_keep_previous_and_current_denominators() {
    for path in DENOMINATOR_RESTATEMENT_FILES {
        let report = read_json(path);
        if path == "operator.json" {
            let rendered = serde_json::to_string(&report).expect("operator renders");
            assert!(rendered.contains("77-subject E4 gate"), "{path}");
            assert!(rendered.contains("previously reported as 79"), "{path}");
            assert!(rendered.contains("a0025b3"), "{path}");
            continue;
        }
        assert_denominator_restatement(path, &report).expect("denominator restatement");
    }
}

#[test]
fn denominator_report_without_previous_denominator_is_rejected() {
    let stale = serde_json::json!({
        "frozen_denominator": 77,
        "retained_population_denominator": 70,
        "denominator_ruling": {
            "commit": "a0025b3"
        }
    });
    let error = assert_denominator_restatement("stale", &stale).expect_err("stale shape fails");
    assert!(error.contains("previous_frozen_denominator"), "{error}");
}

fn assert_truth_exclusion_accounting(path: &str, baseline: &Value) -> Result<(), String> {
    let reported = required_u64(path, baseline, "reported_truth_exclusions")?;
    let completed = required_u64(path, baseline, "truth_exclusions_completed")?;
    let incomplete = required_u64(path, baseline, "truth_classification_incomplete")?;
    let current = required_u64(path, baseline, "truth_exclusions")?;
    if current != completed {
        return Err(format!(
            "{path} truth_exclusions must equal completed truth exclusions"
        ));
    }
    if reported != completed + incomplete {
        return Err(format!(
            "{path} reported_truth_exclusions must equal completed plus incomplete"
        ));
    }
    let cases = baseline
        .get("truth_classification_incomplete_case_fragments")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{path} missing truth_classification_incomplete_case_fragments"))?
        .iter()
        .map(|case| {
            case.as_str()
                .ok_or_else(|| format!("{path} incomplete case id must be a string"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if cases != INCOMPLETE_CASES {
        return Err(format!("{path} incomplete case ids drifted"));
    }
    Ok(())
}

fn assert_denominator_restatement(path: &str, report: &Value) -> Result<(), String> {
    let frozen_denominator = report
        .get("frozen_denominator")
        .or_else(|| report.get("denominator")?.get("frozen_e4_subjects"))
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{path} missing frozen_denominator"))?;
    if frozen_denominator != 77 {
        return Err(format!("{path} frozen_denominator must be restated to 77"));
    }
    let ruling = report
        .get("denominator_ruling")
        .or_else(|| report.get("denominator")?.get("denominator_ruling"))
        .ok_or_else(|| format!("{path} missing denominator_ruling"))?;
    let previous = ruling
        .get("previous_frozen_denominator")
        .or_else(|| report.get("reported_frozen_denominator"))
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{path} missing previous_frozen_denominator"))?;
    if previous != 79 {
        return Err(format!("{path} previous_frozen_denominator must retain 79"));
    }
    if !serde_json::to_string(ruling)
        .expect("ruling renders")
        .contains("a0025b3")
    {
        return Err(format!("{path} denominator ruling must cite a0025b3"));
    }
    Ok(())
}

fn required_u64(path: &str, value: &Value, field: &str) -> Result<u64, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{path} missing numeric {field}"))
}

fn read_json(relative: &str) -> Value {
    let file =
        File::open(repo_path(relative)).unwrap_or_else(|error| panic!("open {relative}: {error}"));
    serde_json::from_reader(BufReader::new(file))
        .unwrap_or_else(|error| panic!("parse {relative}: {error}"))
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
