#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::calibrate_em::{
        CALIBRATE_EM_PROBABILITY_SCALE, CANON_ENTITY_CALIBRATE_EM_VERSION,
        CalibrateEmConvergenceStatus, CalibrateEmOptions, CalibrateEmProposalStatus,
        CalibrateEmRecommendationStatus, CalibrateEmReport, CalibrateEmRequest,
        canonical_calibrate_em_report_bytes, run_calibrate_em,
        validate_proposed_strategy_yaml_fragment,
    },
};
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::{TempDir, tempdir};

#[test]
fn planted_synthetic_corpus_recovers_mu_and_emits_review_only_yaml() {
    let fixture = EmFixture::new();
    fixture.write_strategy(&["legal_name", "jurisdiction"]);
    fixture.write_evidence(&planted_records());
    let before_strategy = fs::read(fixture.strategy()).expect("strategy before EM");

    let report = fixture.report();

    assert_eq!(report.version, CANON_ENTITY_CALIBRATE_EM_VERSION);
    assert!(report.read_only);
    assert!(!report.writes_performed);
    assert_eq!(report.metric_units, "integer_fixed_point");
    assert_eq!(report.inputs.pair_count, 1_000);
    assert_eq!(report.inputs.support_operator_count, 2);
    assert_eq!(report.inputs.pattern_count, 4);
    assert_eq!(report.aggregate.source, "distinct_agreement_pattern_counts");
    assert_eq!(report.aggregate.pattern_count, 4);
    assert_eq!(
        report.convergence.status,
        CalibrateEmConvergenceStatus::Converged
    );
    assert!(report.warnings.is_empty());
    assert_eq!(
        report.recommendation.status,
        CalibrateEmRecommendationStatus::Recommended
    );
    assert_eq!(report.recommendation.proposed_operator_count, 2);
    let fragment = report
        .recommendation
        .proposed_strategy_yaml_fragment
        .as_deref()
        .expect("proposed support score YAML");
    validate_proposed_strategy_yaml_fragment(fragment).expect("proposed YAML parses");
    let fragment_yaml: serde_yaml::Value = serde_yaml::from_str(fragment).unwrap();
    let scores = fragment_yaml
        .as_mapping()
        .unwrap()
        .get(serde_yaml::Value::String("support_scores".to_string()))
        .unwrap()
        .as_mapping()
        .unwrap();
    assert_eq!(scores.len(), 2);
    for operator in ["legal_name", "jurisdiction"] {
        let estimate = operator_estimate(&report, operator);
        assert_close(estimate.m_probability_units, 900_000, 1_250);
        assert_close(estimate.u_probability_units, 100_000, 1_250);
        assert!(estimate.log_odds_weight_units > 6_000);
        assert_eq!(
            estimate.proposal_status,
            CalibrateEmProposalStatus::Proposed
        );
        assert!(estimate.proposed_support_score_units.unwrap() > 5_500);
    }
    assert_eq!(
        fs::read(fixture.strategy()).expect("strategy after EM"),
        before_strategy,
        "calibrate em must not mutate the reviewed strategy"
    );
}

#[test]
fn report_is_byte_identical_after_shuffling_input_rows() {
    let fixture = EmFixture::new();
    fixture.write_strategy(&["legal_name", "jurisdiction"]);
    let records = planted_records();
    fixture.write_evidence(&records);
    let first = canonical_calibrate_em_report_bytes(&fixture.report()).unwrap();

    let mut shuffled = records;
    shuffled.reverse();
    fixture.write_evidence(&shuffled);
    let second = canonical_calibrate_em_report_bytes(&fixture.report()).unwrap();

    assert_eq!(first, second);
}

#[test]
fn colon_operator_ids_are_preserved_and_yaml_quoted() {
    let fixture = EmFixture::new();
    fixture.write_strategy(&["exact_view:legal_name", "exact_view:jurisdiction"]);
    fixture.write_evidence(&colon_operator_records());

    let report = fixture.report();

    let legal_name = operator_estimate(&report, "exact_view:legal_name");
    assert_eq!(
        legal_name.proposal_status,
        CalibrateEmProposalStatus::Proposed
    );
    assert!(report.operators.iter().all(|estimate| {
        estimate.operator_id.starts_with("exact_view:")
            && !matches!(estimate.operator_id.as_str(), "legal_name" | "jurisdiction")
    }));
    let fragment = report
        .recommendation
        .proposed_strategy_yaml_fragment
        .as_deref()
        .expect("colon support scores fragment");
    assert!(fragment.contains("'exact_view:legal_name':"));
    validate_proposed_strategy_yaml_fragment(fragment).expect("quoted colon YAML parses");
}

#[test]
fn constant_and_zero_cell_operator_warns_and_gets_no_weight_proposal() {
    let fixture = EmFixture::new();
    fixture.write_strategy(&["legal_name", "jurisdiction"]);
    fixture.write_evidence(&[
        evidence("A0", "B0", &["legal_name", "jurisdiction"]),
        evidence("A1", "B1", &["legal_name"]),
        evidence("A2", "B2", &["legal_name", "jurisdiction"]),
        evidence("A3", "B3", &["legal_name"]),
    ]);

    let report = fixture.report();

    let legal_name = operator_estimate(&report, "legal_name");
    assert_eq!(legal_name.agreement_count, 4);
    assert_eq!(legal_name.disagreement_count, 0);
    assert_eq!(
        legal_name.proposal_status,
        CalibrateEmProposalStatus::Suppressed
    );
    assert!(legal_name.proposed_support_score_units.is_none());
    assert!(
        legal_name
            .warning_codes
            .contains(&"constant_operator".to_string())
    );
    assert!(
        legal_name
            .warning_codes
            .contains(&"zero_count_cell".to_string())
    );
    assert!(report.warnings.iter().any(|warning| {
        warning.code == "zero_count_cell" && warning.operator_id.as_deref() == Some("legal_name")
    }));
}

#[test]
fn non_convergence_warns_and_suppresses_all_weight_proposals() {
    let fixture = EmFixture::new();
    fixture.write_strategy(&["legal_name", "jurisdiction"]);
    fixture.write_evidence(&planted_records());

    let report = run_calibrate_em(CalibrateEmRequest {
        evidence: fixture.evidence(),
        strategy: fixture.strategy(),
        options: CalibrateEmOptions {
            max_iterations: 1,
            convergence_epsilon_units: 0,
        },
    })
    .expect("non-converged EM report");

    assert_eq!(
        report.convergence.status,
        CalibrateEmConvergenceStatus::NotConverged
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.code == "em_not_converged")
    );
    assert_eq!(
        report.recommendation.status,
        CalibrateEmRecommendationStatus::Blocked
    );
    assert!(
        report
            .recommendation
            .proposed_strategy_yaml_fragment
            .is_none()
    );
    assert!(report.operators.iter().all(|estimate| {
        estimate.proposal_status == CalibrateEmProposalStatus::Suppressed
            && estimate.proposed_support_score_units.is_none()
            && estimate
                .warning_codes
                .contains(&"em_not_converged".to_string())
    }));
}

#[test]
fn sealed_holdout_path_is_typed_refusal_before_training() {
    let fixture = EmFixture::new();
    fixture.write_strategy(&["legal_name"]);
    let sealed_dir = fixture.root().join("sealed_holdout");
    fs::create_dir(&sealed_dir).expect("sealed dir");
    let sealed_evidence = sealed_dir.join("evidence.jsonl");
    write_jsonl(&sealed_evidence, &[evidence("A", "B", &["legal_name"])]);

    let refusal = run_calibrate_em(CalibrateEmRequest {
        evidence: &sealed_evidence,
        strategy: fixture.strategy(),
        options: CalibrateEmOptions::default(),
    })
    .unwrap_err();

    assert_eq!(refusal.code, RefusalCode::EEntityInputContract);
    assert_eq!(
        refusal.detail["reason"],
        "sealed_acceptance_or_holdout_input"
    );
    assert_eq!(refusal.detail["writes_performed"], false);
}

struct EmFixture {
    _tempdir: TempDir,
    root: std::path::PathBuf,
    evidence: std::path::PathBuf,
    strategy: std::path::PathBuf,
}

impl EmFixture {
    fn new() -> Self {
        let tempdir = tempdir().expect("tempdir");
        let root = tempdir.path().to_path_buf();
        Self {
            evidence: root.join("evidence.jsonl"),
            strategy: root.join("strategy.yaml"),
            root,
            _tempdir: tempdir,
        }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn evidence(&self) -> &Path {
        &self.evidence
    }

    fn strategy(&self) -> &Path {
        &self.strategy
    }

    fn request(&self) -> CalibrateEmRequest<'_> {
        CalibrateEmRequest {
            evidence: self.evidence(),
            strategy: self.strategy(),
            options: CalibrateEmOptions::default(),
        }
    }

    fn report(&self) -> CalibrateEmReport {
        run_calibrate_em(self.request()).expect("calibrate em report")
    }

    fn write_evidence(&self, records: &[Value]) {
        write_jsonl(self.evidence(), records);
    }

    fn write_strategy(&self, operators: &[&str]) {
        let mut yaml =
            "strategy_id: em-fixture\nstrategy_version: '1'\nsupport_scores:\n".to_string();
        for operator in operators {
            yaml.push_str("  '");
            yaml.push_str(operator);
            yaml.push_str("': 1000\n");
        }
        fs::write(self.strategy(), yaml).expect("write strategy");
    }
}

fn planted_records() -> Vec<Value> {
    let mut records = Vec::new();
    records.extend(pattern_records(0, 410, &["legal_name", "jurisdiction"]));
    records.extend(pattern_records(410, 90, &["legal_name"]));
    records.extend(pattern_records(500, 90, &["jurisdiction"]));
    records.extend(pattern_records(590, 410, &[]));
    assert_eq!(records.len(), 1_000);
    records
}

fn colon_operator_records() -> Vec<Value> {
    let mut records = Vec::new();
    records.extend(pattern_records(
        0,
        410,
        &["exact_view:legal_name", "exact_view:jurisdiction"],
    ));
    records.extend(pattern_records(410, 90, &["exact_view:legal_name"]));
    records.extend(pattern_records(500, 90, &["exact_view:jurisdiction"]));
    records.extend(pattern_records(590, 410, &[]));
    records
}

fn pattern_records(start: usize, count: usize, operators: &[&str]) -> Vec<Value> {
    (start..start + count)
        .map(|index| {
            evidence(
                &format!("left-{index:04}"),
                &format!("right-{index:04}"),
                operators,
            )
        })
        .collect()
}

fn evidence(left: &str, right: &str, operators: &[&str]) -> Value {
    json!({
        "left_surface_id": left,
        "right_surface_id": right,
        "hits": operators
            .iter()
            .map(|operator| {
                json!({
                    "lane": "support",
                    "namespace": "entity",
                    "operator_id": operator,
                    "score_units": 1_000
                })
            })
            .collect::<Vec<_>>()
    })
}

fn write_jsonl(path: &Path, records: &[Value]) {
    let mut content = String::new();
    for record in records {
        content.push_str(&serde_json::to_string(record).unwrap());
        content.push('\n');
    }
    fs::write(path, content).expect("write jsonl");
}

fn operator_estimate<'a>(
    report: &'a CalibrateEmReport,
    operator_id: &str,
) -> &'a canon::entity::calibrate_em::CalibrateEmOperatorEstimate {
    report
        .operators
        .iter()
        .find(|estimate| estimate.operator_id == operator_id)
        .expect("operator estimate")
}

fn assert_close(actual: u32, expected: u32, tolerance: u32) {
    assert!(
        actual.abs_diff(expected) <= tolerance,
        "actual {actual} expected {expected} tolerance {tolerance} scale {CALIBRATE_EM_PROBABILITY_SCALE}"
    );
}
