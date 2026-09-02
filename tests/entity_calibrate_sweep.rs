#![forbid(unsafe_code)]

use assert_cmd::Command;
use canon::{
    RefusalCode,
    entity::calibrate::{
        CALIBRATE_CRITICAL_FALSE_MERGES_MAX, CALIBRATE_PRECISION_MIN_BASIS_POINTS,
        CALIBRATE_QUALITY_CONTRACT, CALIBRATE_RECALL_MIN_BASIS_POINTS,
        CANON_ENTITY_CALIBRATE_SWEEP_VERSION, CalibrateRecommendationStatus, CalibrateSweepReport,
        CalibrateSweepRequest, canonical_calibrate_sweep_report_bytes, run_calibrate_sweep,
    },
};
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::{TempDir, tempdir};

#[test]
fn hand_computable_fixture_recommends_strictest_gate_passing_tuple() {
    let fixture = CalibrationFixture::new();
    fixture.write_strategy();
    fixture.write_predictions(&[
        prediction("A", "B", 90),
        prediction("C", "D", 80),
        prediction("E", "F", 70),
    ]);
    fixture.write_gold(&[
        gold("C", "D", "same", None),
        gold("E", "F", "distinct", Some("critical")),
        gold("A", "B", "same", None),
    ]);
    let before_strategy = fs::read(fixture.strategy()).expect("strategy bytes before run");

    let report = fixture.report();

    assert_eq!(report.version, CANON_ENTITY_CALIBRATE_SWEEP_VERSION);
    assert_eq!(report.quality_contract, CALIBRATE_QUALITY_CONTRACT);
    assert!(report.read_only);
    assert!(!report.writes_performed);
    assert_eq!(report.metric_units, "integer_basis_points");
    assert_eq!(
        report.recommendation.status,
        CalibrateRecommendationStatus::Recommended
    );
    assert_eq!(
        report.recommendation.reason,
        "max_auto_accept_subject_to_canon_entity_quality_v1"
    );
    assert_eq!(
        report.recommendation.selected_thresholds.unwrap(),
        canon::entity::calibrate::CalibrateThresholdTuple {
            backbone_score_min: 80,
            attach_score_min: 80,
            abstain_margin: 80,
            match_threshold: 80,
            ambiguity_gap: 80,
        }
    );
    let metrics = report.recommendation.selected_metrics.as_ref().unwrap();
    assert_eq!(metrics.auto_accept_count, 2);
    assert_eq!(metrics.escrow_count, 1);
    assert_eq!(metrics.auto_accept_rate_basis_points, 6667);
    assert_eq!(metrics.precision_basis_points, Some(10_000));
    assert_eq!(metrics.recall_basis_points, Some(10_000));
    assert_eq!(metrics.critical_false_merges, 0);
    assert_eq!(
        report
            .recommendation
            .proposed_strategy_yaml_fragment
            .as_deref(),
        Some(
            "solver:\n  backbone_score_min: 80\n  attach_score_min: 80\n  abstain_margin: 80\n  match_threshold: 80\n  ambiguity_gap: 80\n"
        )
    );
    assert_eq!(
        report.gates.precision_min_basis_points,
        CALIBRATE_PRECISION_MIN_BASIS_POINTS
    );
    assert_eq!(
        report.gates.recall_min_basis_points,
        CALIBRATE_RECALL_MIN_BASIS_POINTS
    );
    assert_eq!(
        report.gates.critical_false_merges_max,
        CALIBRATE_CRITICAL_FALSE_MERGES_MAX
    );
    assert!(!report.gates.caller_adjustable);
    assert_eq!(
        fs::read(fixture.strategy()).expect("strategy bytes after run"),
        before_strategy,
        "calibration must not mutate the strategy under test"
    );
}

#[test]
fn report_is_byte_identical_after_shuffling_input_rows() {
    let fixture = CalibrationFixture::new();
    fixture.write_strategy();
    fixture.write_predictions(&[
        prediction("A", "B", 90),
        prediction("C", "D", 80),
        prediction("E", "F", 70),
    ]);
    fixture.write_gold(&[
        gold("A", "B", "same", None),
        gold("C", "D", "same", None),
        gold("E", "F", "distinct", Some("critical")),
    ]);
    let first = canonical_calibrate_sweep_report_bytes(&fixture.report()).unwrap();

    fixture.write_predictions(&[
        prediction("E", "F", 70),
        prediction("A", "B", 90),
        prediction("C", "D", 80),
    ]);
    fixture.write_gold(&[
        gold("E", "F", "distinct", Some("critical")),
        gold("C", "D", "same", None),
        gold("A", "B", "same", None),
    ]);
    let second = canonical_calibrate_sweep_report_bytes(&fixture.report()).unwrap();

    assert_eq!(first, second);
}

#[test]
fn gate_infeasible_fixture_emits_blocked_recommendation_without_fallback() {
    let fixture = CalibrationFixture::new();
    fixture.write_strategy();
    fixture.write_predictions(&[
        prediction("A", "B", 90),
        prediction("C", "D", 80),
        prediction("E", "F", 80),
    ]);
    fixture.write_gold(&[
        gold("A", "B", "same", None),
        gold("C", "D", "same", None),
        gold("E", "F", "distinct", Some("critical")),
    ]);

    let report = fixture.report();

    assert_eq!(
        report.recommendation.status,
        CalibrateRecommendationStatus::Blocked
    );
    assert_eq!(
        report.recommendation.reason,
        "no threshold tuple satisfied canon.entity.quality.v1; no fallback tuple selected"
    );
    assert!(report.recommendation.selected_thresholds.is_none());
    assert!(report.recommendation.selected_metrics.is_none());
    assert!(
        report
            .recommendation
            .proposed_strategy_yaml_fragment
            .is_none()
    );
    assert!(
        report
            .truth_space
            .iter()
            .all(|row| !row.passes_quality_gates)
    );
}

#[test]
fn unknown_gold_pair_is_typed_refusal_and_cli_exit_two() {
    let fixture = CalibrationFixture::new();
    fixture.write_strategy();
    fixture.write_predictions(&[prediction("A", "B", 90)]);
    fixture.write_gold(&[gold("A", "Z", "same", None)]);

    let refusal = run_calibrate_sweep(fixture.request()).unwrap_err();
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(
        refusal.detail["reason"],
        "gold_pair_not_found_in_predictions"
    );
    assert_eq!(refusal.detail["writes_performed"], false);

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "calibrate",
            "sweep",
            fixture.result().to_str().unwrap(),
            "--gold",
            fixture.gold().to_str().unwrap(),
            "--strategy",
            fixture.strategy().to_str().unwrap(),
            "--emit",
            "json",
        ])
        .output()
        .expect("run calibrate sweep CLI");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let stdout: Value = serde_json::from_slice(&output.stdout).expect("refusal stdout JSON");
    assert_eq!(stdout["outcome"], "REFUSAL");
    assert_eq!(stdout["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        stdout["refusal"]["detail"]["reason"],
        "gold_pair_not_found_in_predictions"
    );
}

#[test]
fn native_style_pair_aliases_parse_without_mutation() {
    let fixture = CalibrationFixture::new();
    fixture.write_strategy();
    fixture.write_predictions(&[
        json!({
            "left_row_id": "row-left",
            "right_row_id": "row-right",
            "score": 90,
            "has_cannot_link": false
        }),
        json!({
            "pair": ["row-bad-left", "row-bad-right"],
            "score": 85,
            "has_cannot_link": true
        }),
    ]);
    fixture.write_gold(&[
        json!({
            "pair": ["row-right", "row-left"],
            "label": "same"
        }),
        json!({
            "pair": ["row-bad-right", "row-bad-left"],
            "label": "distinct",
            "severity": "critical"
        }),
    ]);
    let before_strategy = fs::read(fixture.strategy()).expect("strategy before aliases");

    let report = fixture.report();

    assert_eq!(report.inputs.labeled_pair_count, 2);
    assert_eq!(report.inputs.unlabeled_prediction_pair_count, 0);
    assert_eq!(
        report.recommendation.status,
        CalibrateRecommendationStatus::Recommended
    );
    assert_eq!(
        fs::read(fixture.strategy()).expect("strategy after aliases"),
        before_strategy
    );
}

#[test]
fn auto_accept_rate_is_monotone_when_thresholds_loosen() {
    let fixture = CalibrationFixture::new();
    fixture.write_strategy();
    fixture.write_predictions(&[
        prediction("A", "B", 90),
        prediction("C", "D", 80),
        prediction("E", "F", 70),
    ]);
    fixture.write_gold(&[
        gold("A", "B", "same", None),
        gold("C", "D", "same", None),
        gold("E", "F", "distinct", Some("critical")),
    ]);
    let report = fixture.report();

    for stricter in &report.truth_space {
        for looser in &report.truth_space {
            if looser
                .thresholds
                .is_looser_or_equal_than(stricter.thresholds)
            {
                assert!(
                    looser.auto_accept_count >= stricter.auto_accept_count,
                    "looser {:?} accepted fewer rows than stricter {:?}",
                    looser.thresholds,
                    stricter.thresholds
                );
            }
        }
    }
}

struct CalibrationFixture {
    _temp: TempDir,
    result: std::path::PathBuf,
    gold: std::path::PathBuf,
    strategy: std::path::PathBuf,
}

impl CalibrationFixture {
    fn new() -> Self {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().to_path_buf();
        Self {
            _temp: temp,
            result: root.join("result.jsonl"),
            gold: root.join("gold.jsonl"),
            strategy: root.join("strategy.yaml"),
        }
    }

    fn result(&self) -> &Path {
        &self.result
    }

    fn gold(&self) -> &Path {
        &self.gold
    }

    fn strategy(&self) -> &Path {
        &self.strategy
    }

    fn request(&self) -> CalibrateSweepRequest<'_> {
        CalibrateSweepRequest {
            result: self.result(),
            gold: self.gold(),
            strategy: self.strategy(),
        }
    }

    fn report(&self) -> CalibrateSweepReport {
        run_calibrate_sweep(self.request()).expect("calibration report")
    }

    fn write_predictions(&self, records: &[Value]) {
        write_jsonl(self.result(), records);
    }

    fn write_gold(&self, records: &[Value]) {
        write_jsonl(self.gold(), records);
    }

    fn write_strategy(&self) {
        fs::write(
            self.strategy(),
            "strategy_id: calibrate-fixture\nstrategy_version: '1'\nsolver:\n  backbone_score_min: 32\n  attach_score_min: 28\n  abstain_margin: 6\n",
        )
        .expect("write strategy");
    }
}

fn prediction(left: &str, right: &str, score: u32) -> Value {
    json!({
        "left_id": left,
        "right_id": right,
        "backbone_score": score,
        "attach_score": score,
        "abstain_margin": score,
        "match_score": score,
        "ambiguity_gap": score
    })
}

fn gold(left: &str, right: &str, label: &str, severity: Option<&str>) -> Value {
    let mut value = json!({
        "left_id": left,
        "right_id": right,
        "label": label
    });
    if let Some(severity) = severity {
        value["severity"] = json!(severity);
    }
    value
}

fn write_jsonl(path: &Path, records: &[Value]) {
    let mut content = String::new();
    for record in records {
        content.push_str(&serde_json::to_string(record).expect("record serializes"));
        content.push('\n');
    }
    fs::write(path, content).expect("write jsonl");
}
