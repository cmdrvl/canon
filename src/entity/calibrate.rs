#![forbid(unsafe_code)]

//! Read-only threshold calibration for entity workbench score artifacts.
//!
//! This module follows Splink's truth-space-table idea, but keeps Canon's
//! artifact boundary integer-only and advisory-only: it emits a report and a
//! threshold YAML fragment, never a registry mutation or strategy rewrite.

use crate::{Refusal, RefusalCode, entity::score::ENTITY_SCORE_SCALE, witness};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

pub const CANON_ENTITY_CALIBRATE_SWEEP_VERSION: &str = "canon.entity.calibrate_sweep.v0";
pub const CALIBRATE_QUALITY_CONTRACT: &str = "canon.entity.quality.v1";
pub const CALIBRATE_PRECISION_MIN_BASIS_POINTS: u32 = 9_950;
pub const CALIBRATE_RECALL_MIN_BASIS_POINTS: u32 = 9_800;
pub const CALIBRATE_CRITICAL_FALSE_MERGES_MAX: u64 = 0;

const NEXT_COMMAND: &str = "canon entity calibrate sweep <RESULT|EVIDENCE> --gold <GOLD.jsonl> --strategy <STRATEGY.yaml> --emit json";
const MAX_AXIS_VALUES: usize = 8;

const RESULT_ARRAY_FIELDS: &[&str] = &[
    "calibration_pairs",
    "predictions",
    "pairs",
    "records",
    "evidence",
    "edge_records",
];
const GOLD_ARRAY_FIELDS: &[&str] = &["gold", "labels", "pairs", "records"];
const LEFT_ID_FIELDS: &[&str] = &[
    "left_id",
    "left_surface_id",
    "left_row_id",
    "target_id",
    "source_id_l",
    "id_l",
];
const RIGHT_ID_FIELDS: &[&str] = &[
    "right_id",
    "right_surface_id",
    "right_row_id",
    "reference_id",
    "expected_reference_id",
    "source_id_r",
    "id_r",
];
const MATCH_SCORE_FIELDS: &[&str] = &[
    "match_score",
    "score",
    "match_score_units",
    "score_units",
    "pair_score_total",
    "score_breakdown.total_score_units",
    "support_score_units",
    "adjusted_support_score_units",
];
const BACKBONE_SCORE_FIELDS: &[&str] = &["backbone_score", "backbone_score_units"];
const ATTACH_SCORE_FIELDS: &[&str] = &["attach_score", "attach_score_units"];
const ABSTAIN_MARGIN_FIELDS: &[&str] = &[
    "abstain_margin",
    "abstain_margin_units",
    "margin_score",
    "margin_score_units",
    "runner_up_margin_units",
];
const AMBIGUITY_GAP_FIELDS: &[&str] = &[
    "ambiguity_gap",
    "ambiguity_gap_units",
    "gap_score",
    "gap_score_units",
    "runner_up_gap_units",
];

#[derive(Debug, Clone, Copy)]
pub struct CalibrateSweepRequest<'a> {
    pub result: &'a Path,
    pub gold: &'a Path,
    pub strategy: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateSweepReport {
    pub version: String,
    pub quality_contract: String,
    pub read_only: bool,
    pub writes_performed: bool,
    pub metric_units: String,
    pub inputs: CalibrateInputSummary,
    pub grid: CalibrateGrid,
    pub gates: CalibrateGatePolicy,
    pub recommendation: CalibrateRecommendation,
    pub truth_space: Vec<CalibrateTruthSpaceRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateInputSummary {
    pub result_path: String,
    pub result_set_hash: String,
    pub gold_path: String,
    pub gold_set_hash: String,
    pub strategy_path: String,
    pub strategy_hash: String,
    pub labeled_pair_count: u64,
    pub prediction_pair_count: u64,
    pub unlabeled_prediction_pair_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateGrid {
    pub bounds_source: String,
    pub step_policy: String,
    pub max_axis_values: u64,
    pub tuple_count: u64,
    pub axes: Vec<CalibrateGridAxis>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateGridAxis {
    pub field: String,
    pub values: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateGatePolicy {
    pub precision_min_basis_points: u32,
    pub recall_min_basis_points: u32,
    pub critical_false_merges_max: u64,
    pub caller_adjustable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateRecommendation {
    pub status: CalibrateRecommendationStatus,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_thresholds: Option<CalibrateThresholdTuple>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_metrics: Option<CalibrateSelectedMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed_strategy_yaml_fragment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrateRecommendationStatus {
    Recommended,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateSelectedMetrics {
    pub row_index: u64,
    pub auto_accept_count: u64,
    pub escrow_count: u64,
    pub auto_accept_rate_basis_points: u32,
    pub escrow_rate_basis_points: u32,
    pub precision_basis_points: Option<u32>,
    pub recall_basis_points: Option<u32>,
    pub critical_false_merges: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateThresholdTuple {
    pub backbone_score_min: u32,
    pub attach_score_min: u32,
    pub abstain_margin: u32,
    pub match_threshold: u32,
    pub ambiguity_gap: u32,
}

impl CalibrateThresholdTuple {
    pub fn is_looser_or_equal_than(self, other: Self) -> bool {
        self.backbone_score_min <= other.backbone_score_min
            && self.attach_score_min <= other.attach_score_min
            && self.abstain_margin <= other.abstain_margin
            && self.match_threshold <= other.match_threshold
            && self.ambiguity_gap <= other.ambiguity_gap
    }

    fn strictness_units(self) -> u64 {
        u64::from(self.backbone_score_min)
            + u64::from(self.attach_score_min)
            + u64::from(self.abstain_margin)
            + u64::from(self.match_threshold)
            + u64::from(self.ambiguity_gap)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateTruthSpaceRow {
    pub row_index: u64,
    pub thresholds: CalibrateThresholdTuple,
    pub total_labeled_pair_count: u64,
    pub gold_positive_count: u64,
    pub gold_negative_count: u64,
    pub auto_accept_count: u64,
    pub escrow_count: u64,
    pub true_positive_count: u64,
    pub false_positive_count: u64,
    pub false_negative_count: u64,
    pub true_negative_count: u64,
    pub critical_false_merges: u64,
    pub auto_accept_rate_basis_points: u32,
    pub escrow_rate_basis_points: u32,
    pub precision_basis_points: Option<u32>,
    pub recall_basis_points: Option<u32>,
    pub passes_quality_gates: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateObservedScores {
    pub backbone_score: u32,
    pub attach_score: u32,
    pub abstain_margin: u32,
    pub match_score: u32,
    pub ambiguity_gap: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CalibrationPrediction {
    key: PairKey,
    scores: CalibrateObservedScores,
    hard_cannot_link: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CalibrationGold {
    label: GoldLabel,
    critical: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GoldLabel {
    SameEntity,
    DistinctEntity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LabeledCalibrationPair {
    scores: CalibrateObservedScores,
    hard_cannot_link: bool,
    gold: CalibrationGold,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PairKey {
    left_id: String,
    right_id: String,
}

#[derive(Debug, Serialize)]
struct CanonicalPredictionRecord<'a> {
    left_id: &'a str,
    right_id: &'a str,
    scores: CalibrateObservedScores,
    hard_cannot_link: bool,
}

#[derive(Debug, Serialize)]
struct CanonicalGoldRecord<'a> {
    left_id: &'a str,
    right_id: &'a str,
    label: &'static str,
    critical: bool,
}

pub fn run_calibrate_sweep(
    request: CalibrateSweepRequest<'_>,
) -> Result<CalibrateSweepReport, Refusal> {
    let (prediction_records, _) =
        read_json_records(request.result, "calibration result", RESULT_ARRAY_FIELDS)?;
    let predictions = parse_predictions(&prediction_records)?;
    let (gold_records, _) = read_json_records(request.gold, "calibration gold", GOLD_ARRAY_FIELDS)?;
    let gold = parse_gold(&gold_records)?;
    let strategy = read_strategy(request.strategy)?;
    let labeled_pairs = labeled_pairs(&predictions, &gold)?;
    let axes = grid_axes(&labeled_pairs);
    let tuple_count = axes
        .iter()
        .map(|axis| axis.values.len() as u64)
        .product::<u64>();
    let mut truth_space = sweep_truth_space(&labeled_pairs, &axes);
    for (index, row) in truth_space.iter_mut().enumerate() {
        row.row_index = index as u64;
    }
    let recommendation = recommendation_from_truth_space(&truth_space);

    Ok(CalibrateSweepReport {
        version: CANON_ENTITY_CALIBRATE_SWEEP_VERSION.to_string(),
        quality_contract: CALIBRATE_QUALITY_CONTRACT.to_string(),
        read_only: true,
        writes_performed: false,
        metric_units: "integer_basis_points".to_string(),
        inputs: CalibrateInputSummary {
            result_path: request.result.display().to_string(),
            result_set_hash: canonical_prediction_hash(&predictions)?,
            gold_path: request.gold.display().to_string(),
            gold_set_hash: canonical_gold_hash(&gold)?,
            strategy_path: request.strategy.display().to_string(),
            strategy_hash: strategy.content_hash,
            labeled_pair_count: labeled_pairs.len() as u64,
            prediction_pair_count: predictions.len() as u64,
            unlabeled_prediction_pair_count: predictions.len().saturating_sub(gold.len()) as u64,
        },
        grid: CalibrateGrid {
            bounds_source: "observed_labeled_pair_score_distribution".to_string(),
            step_policy: format!(
                "distinct observed integer breakpoints per axis; axes with more than {MAX_AXIS_VALUES} values are deterministically thinned to quantile breakpoints"
            ),
            max_axis_values: MAX_AXIS_VALUES as u64,
            tuple_count,
            axes,
        },
        gates: CalibrateGatePolicy {
            precision_min_basis_points: CALIBRATE_PRECISION_MIN_BASIS_POINTS,
            recall_min_basis_points: CALIBRATE_RECALL_MIN_BASIS_POINTS,
            critical_false_merges_max: CALIBRATE_CRITICAL_FALSE_MERGES_MAX,
            caller_adjustable: false,
        },
        recommendation,
        truth_space,
    })
}

pub fn canonical_calibrate_sweep_report_bytes(
    report: &CalibrateSweepReport,
) -> Result<Vec<u8>, Refusal> {
    serde_json::to_vec(report).map_err(|error| {
        calibration_refusal(
            "Failed to serialize calibration report",
            json!({
                "stage": "calibrate_sweep",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })
}

pub fn render_calibrate_sweep_summary(report: &CalibrateSweepReport) -> String {
    match report.recommendation.status {
        CalibrateRecommendationStatus::Recommended => {
            let metrics = report
                .recommendation
                .selected_metrics
                .as_ref()
                .expect("recommended report carries selected metrics");
            let thresholds = report
                .recommendation
                .selected_thresholds
                .expect("recommended report carries selected thresholds");
            format!(
                "{} recommended\nquality_contract={}\nmetric_units={}\nauto_accept_rate_basis_points={}\nprecision_basis_points={}\nrecall_basis_points={}\ncritical_false_merges={}\nthresholds backbone_score_min={} attach_score_min={} abstain_margin={} match_threshold={} ambiguity_gap={}\n{}",
                report.version,
                report.quality_contract,
                report.metric_units,
                metrics.auto_accept_rate_basis_points,
                option_bps(metrics.precision_basis_points),
                option_bps(metrics.recall_basis_points),
                metrics.critical_false_merges,
                thresholds.backbone_score_min,
                thresholds.attach_score_min,
                thresholds.abstain_margin,
                thresholds.match_threshold,
                thresholds.ambiguity_gap,
                report
                    .recommendation
                    .proposed_strategy_yaml_fragment
                    .as_deref()
                    .unwrap_or("")
            )
        }
        CalibrateRecommendationStatus::Blocked => format!(
            "{} blocked\nquality_contract={}\nmetric_units={}\nreason={}\ntruth_space_rows={}",
            report.version,
            report.quality_contract,
            report.metric_units,
            report.recommendation.reason,
            report.truth_space.len()
        ),
    }
}

fn option_bps(value: Option<u32>) -> String {
    value
        .map(|bps| bps.to_string())
        .unwrap_or_else(|| "not_applicable".to_string())
}

fn read_json_records(
    path: &Path,
    artifact_label: &str,
    array_fields: &[&str],
) -> Result<(Vec<Value>, Vec<u8>), Refusal> {
    let bytes = fs::read(path).map_err(|error| Refusal {
        code: RefusalCode::EIo,
        message: format!(
            "Failed to read entity calibrate sweep {artifact_label} '{}'",
            path.display()
        ),
        detail: json!({
            "stage": "calibrate_sweep",
            "path": path.display().to_string(),
            "error": error.to_string(),
            "writes_performed": false
        }),
        next_command: Some(NEXT_COMMAND.to_string()),
    })?;
    let content = std::str::from_utf8(&bytes).map_err(|error| {
        calibration_refusal(
            format!("Entity calibrate sweep {artifact_label} must be UTF-8 JSON or JSONL"),
            json!({
                "stage": "calibrate_sweep",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    let trimmed = content.trim_start();
    if trimmed.is_empty() {
        return Err(calibration_refusal(
            format!("Entity calibrate sweep {artifact_label} is empty"),
            json!({
                "stage": "calibrate_sweep",
                "path": path.display().to_string(),
                "writes_performed": false
            }),
        ));
    }

    if trimmed.starts_with('[')
        && let Ok(value) = serde_json::from_str::<Value>(content)
    {
        return records_from_json_value(value, path, artifact_label, array_fields)
            .map(|records| (records, bytes));
    }
    if trimmed.starts_with('{')
        && let Ok(value) = serde_json::from_str::<Value>(content)
    {
        return records_from_json_value(value, path, artifact_label, array_fields)
            .map(|records| (records, bytes));
    }

    let mut records = Vec::new();
    for (line_index, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str::<Value>(line).map_err(|error| {
            calibration_refusal(
                format!(
                    "Invalid entity calibrate sweep {artifact_label} JSONL on line {}",
                    line_index + 1
                ),
                json!({
                    "stage": "calibrate_sweep",
                    "path": path.display().to_string(),
                    "line": line_index + 1,
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
        records.push(value);
    }
    if records.is_empty() {
        return Err(calibration_refusal(
            format!("Entity calibrate sweep {artifact_label} contains no records"),
            json!({
                "stage": "calibrate_sweep",
                "path": path.display().to_string(),
                "writes_performed": false
            }),
        ));
    }
    Ok((records, bytes))
}

fn records_from_json_value(
    value: Value,
    path: &Path,
    artifact_label: &str,
    array_fields: &[&str],
) -> Result<Vec<Value>, Refusal> {
    let records = match value {
        Value::Array(records) => records,
        Value::Object(mut object) => {
            let mut selected = None;
            for field in array_fields {
                if let Some(value) = object.remove(*field) {
                    selected = Some(value);
                    break;
                }
            }
            match selected {
                Some(Value::Array(records)) => records,
                Some(other) => {
                    return Err(calibration_refusal(
                        format!(
                            "Entity calibrate sweep {artifact_label} array field is not an array"
                        ),
                        json!({
                            "stage": "calibrate_sweep",
                            "path": path.display().to_string(),
                            "actual": other,
                            "writes_performed": false
                        }),
                    ));
                }
                None => vec![Value::Object(object)],
            }
        }
        other => {
            return Err(calibration_refusal(
                format!(
                    "Entity calibrate sweep {artifact_label} must be a JSON object, array, or JSONL records"
                ),
                json!({
                    "stage": "calibrate_sweep",
                    "path": path.display().to_string(),
                    "actual": other,
                    "writes_performed": false
                }),
            ));
        }
    };
    if records.is_empty() {
        return Err(calibration_refusal(
            format!("Entity calibrate sweep {artifact_label} contains no records"),
            json!({
                "stage": "calibrate_sweep",
                "path": path.display().to_string(),
                "writes_performed": false
            }),
        ));
    }
    Ok(records)
}

fn parse_predictions(
    records: &[Value],
) -> Result<BTreeMap<PairKey, CalibrationPrediction>, Refusal> {
    let mut predictions = BTreeMap::new();
    for (index, value) in records.iter().enumerate() {
        let record_number = index + 1;
        let key = pair_key_from_value(value, "prediction", record_number)?;
        let match_score = required_score(value, MATCH_SCORE_FIELDS, "match_score", record_number)?;
        let backbone_score = optional_score(
            value,
            BACKBONE_SCORE_FIELDS,
            "backbone_score",
            record_number,
        )?
        .unwrap_or(match_score);
        let attach_score =
            optional_score(value, ATTACH_SCORE_FIELDS, "attach_score", record_number)?
                .unwrap_or(match_score);
        let abstain_margin = optional_score(
            value,
            ABSTAIN_MARGIN_FIELDS,
            "abstain_margin",
            record_number,
        )?
        .unwrap_or(0);
        let ambiguity_gap =
            optional_score(value, AMBIGUITY_GAP_FIELDS, "ambiguity_gap", record_number)?
                .unwrap_or(abstain_margin);
        let prediction = CalibrationPrediction {
            key: key.clone(),
            scores: CalibrateObservedScores {
                backbone_score,
                attach_score,
                abstain_margin,
                match_score,
                ambiguity_gap,
            },
            hard_cannot_link: bool_field(
                value,
                &[
                    "hard_cannot_link",
                    "has_hard_cannot_link",
                    "cannot_link",
                    "has_cannot_link",
                ],
                record_number,
            )?,
        };
        if predictions.insert(key.clone(), prediction).is_some() {
            return Err(calibration_refusal(
                "Entity calibrate sweep result contains duplicate prediction pair",
                json!({
                    "stage": "calibrate_sweep",
                    "record_number": record_number,
                    "pair": key.display(),
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(predictions)
}

fn parse_gold(records: &[Value]) -> Result<BTreeMap<PairKey, CalibrationGold>, Refusal> {
    let mut gold = BTreeMap::new();
    for (index, value) in records.iter().enumerate() {
        let record_number = index + 1;
        let key = pair_key_from_value(value, "gold", record_number)?;
        let label = gold_label(value, record_number)?;
        let critical = bool_field(value, &["critical"], record_number)?
            || string_field(value, &["severity"], record_number)?
                .as_deref()
                .is_some_and(|severity| severity == "critical");
        let record = CalibrationGold { label, critical };
        if gold.insert(key.clone(), record).is_some() {
            return Err(calibration_refusal(
                "Entity calibrate sweep gold contains duplicate pair",
                json!({
                    "stage": "calibrate_sweep",
                    "record_number": record_number,
                    "pair": key.display(),
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(gold)
}

fn read_strategy(path: &Path) -> Result<StrategyCalibrationInput, Refusal> {
    let bytes = fs::read(path).map_err(|error| Refusal {
        code: RefusalCode::EIo,
        message: format!(
            "Failed to read entity calibrate sweep strategy '{}'",
            path.display()
        ),
        detail: json!({
            "stage": "calibrate_sweep",
            "path": path.display().to_string(),
            "error": error.to_string(),
            "writes_performed": false
        }),
        next_command: Some(NEXT_COMMAND.to_string()),
    })?;
    serde_yaml::from_slice::<serde_yaml::Value>(&bytes).map_err(|error| Refusal {
        code: RefusalCode::EEntityStrategy,
        message: "Invalid entity calibrate sweep strategy YAML".to_string(),
        detail: json!({
            "stage": "calibrate_sweep",
            "path": path.display().to_string(),
            "error": error.to_string(),
            "writes_performed": false
        }),
        next_command: Some(NEXT_COMMAND.to_string()),
    })?;
    Ok(StrategyCalibrationInput {
        content_hash: witness::hash_bytes(&bytes),
    })
}

struct StrategyCalibrationInput {
    content_hash: String,
}

fn labeled_pairs(
    predictions: &BTreeMap<PairKey, CalibrationPrediction>,
    gold: &BTreeMap<PairKey, CalibrationGold>,
) -> Result<Vec<LabeledCalibrationPair>, Refusal> {
    let missing = gold
        .keys()
        .filter(|key| !predictions.contains_key(*key))
        .map(PairKey::display)
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(calibration_refusal(
            "Entity calibrate sweep gold references pairs absent from the result artifact",
            json!({
                "stage": "calibrate_sweep",
                "reason": "gold_pair_not_found_in_predictions",
                "missing_pairs": missing,
                "writes_performed": false
            }),
        ));
    }

    Ok(gold
        .iter()
        .map(|(key, gold)| {
            let prediction = predictions
                .get(key)
                .expect("missing gold pairs checked before labeling");
            debug_assert_eq!(&prediction.key, key);
            LabeledCalibrationPair {
                scores: prediction.scores,
                hard_cannot_link: prediction.hard_cannot_link,
                gold: *gold,
            }
        })
        .collect())
}

fn grid_axes(pairs: &[LabeledCalibrationPair]) -> Vec<CalibrateGridAxis> {
    let mut backbone = BTreeSet::from([0]);
    let mut attach = BTreeSet::from([0]);
    let mut abstain = BTreeSet::from([0]);
    let mut match_threshold = BTreeSet::from([0]);
    let mut ambiguity = BTreeSet::from([0]);
    for pair in pairs {
        backbone.insert(pair.scores.backbone_score);
        attach.insert(pair.scores.attach_score);
        abstain.insert(pair.scores.abstain_margin);
        match_threshold.insert(pair.scores.match_score);
        ambiguity.insert(pair.scores.ambiguity_gap);
    }

    vec![
        CalibrateGridAxis {
            field: "backbone_score_min".to_string(),
            values: thinned_axis_values(backbone),
        },
        CalibrateGridAxis {
            field: "attach_score_min".to_string(),
            values: thinned_axis_values(attach),
        },
        CalibrateGridAxis {
            field: "abstain_margin".to_string(),
            values: thinned_axis_values(abstain),
        },
        CalibrateGridAxis {
            field: "match_threshold".to_string(),
            values: thinned_axis_values(match_threshold),
        },
        CalibrateGridAxis {
            field: "ambiguity_gap".to_string(),
            values: thinned_axis_values(ambiguity),
        },
    ]
}

fn thinned_axis_values(values: BTreeSet<u32>) -> Vec<u32> {
    let values = values.into_iter().collect::<Vec<_>>();
    if values.len() <= MAX_AXIS_VALUES {
        return values;
    }
    let last = values.len() - 1;
    let mut selected = BTreeSet::new();
    for bucket in 0..MAX_AXIS_VALUES {
        let numerator = bucket * last;
        let index = (numerator + ((MAX_AXIS_VALUES - 1) / 2)) / (MAX_AXIS_VALUES - 1);
        selected.insert(values[index]);
    }
    selected.into_iter().collect()
}

fn sweep_truth_space(
    pairs: &[LabeledCalibrationPair],
    axes: &[CalibrateGridAxis],
) -> Vec<CalibrateTruthSpaceRow> {
    let backbone_values = descending(&axes[0].values);
    let attach_values = descending(&axes[1].values);
    let abstain_values = descending(&axes[2].values);
    let match_values = descending(&axes[3].values);
    let ambiguity_values = descending(&axes[4].values);
    let mut rows = Vec::new();
    for backbone_score_min in &backbone_values {
        for attach_score_min in &attach_values {
            for abstain_margin in &abstain_values {
                for match_threshold in &match_values {
                    for ambiguity_gap in &ambiguity_values {
                        rows.push(truth_space_row(
                            pairs,
                            CalibrateThresholdTuple {
                                backbone_score_min: *backbone_score_min,
                                attach_score_min: *attach_score_min,
                                abstain_margin: *abstain_margin,
                                match_threshold: *match_threshold,
                                ambiguity_gap: *ambiguity_gap,
                            },
                        ));
                    }
                }
            }
        }
    }
    rows
}

fn descending(values: &[u32]) -> Vec<u32> {
    let mut out = values.to_vec();
    out.sort_by(|left, right| right.cmp(left));
    out
}

fn truth_space_row(
    pairs: &[LabeledCalibrationPair],
    thresholds: CalibrateThresholdTuple,
) -> CalibrateTruthSpaceRow {
    let mut gold_positive_count = 0;
    let mut gold_negative_count = 0;
    let mut auto_accept_count = 0;
    let mut true_positive_count = 0;
    let mut false_positive_count = 0;
    let mut false_negative_count = 0;
    let mut true_negative_count = 0;
    let mut critical_false_merges = 0;

    for pair in pairs {
        let accepts = !pair.hard_cannot_link && accepts_thresholds(pair.scores, thresholds);
        match (pair.gold.label, accepts) {
            (GoldLabel::SameEntity, true) => {
                gold_positive_count += 1;
                auto_accept_count += 1;
                true_positive_count += 1;
            }
            (GoldLabel::SameEntity, false) => {
                gold_positive_count += 1;
                false_negative_count += 1;
            }
            (GoldLabel::DistinctEntity, true) => {
                gold_negative_count += 1;
                auto_accept_count += 1;
                false_positive_count += 1;
                if pair.gold.critical {
                    critical_false_merges += 1;
                }
            }
            (GoldLabel::DistinctEntity, false) => {
                gold_negative_count += 1;
                true_negative_count += 1;
            }
        }
    }

    let total = pairs.len() as u64;
    let escrow_count = total.saturating_sub(auto_accept_count);
    let precision_basis_points = nonzero_basis_points(true_positive_count, auto_accept_count);
    let recall_basis_points = nonzero_basis_points(true_positive_count, gold_positive_count);
    let passes_quality_gates = precision_basis_points
        .is_some_and(|precision| precision >= CALIBRATE_PRECISION_MIN_BASIS_POINTS)
        && recall_basis_points.is_some_and(|recall| recall >= CALIBRATE_RECALL_MIN_BASIS_POINTS)
        && critical_false_merges == CALIBRATE_CRITICAL_FALSE_MERGES_MAX;

    CalibrateTruthSpaceRow {
        row_index: 0,
        thresholds,
        total_labeled_pair_count: total,
        gold_positive_count,
        gold_negative_count,
        auto_accept_count,
        escrow_count,
        true_positive_count,
        false_positive_count,
        false_negative_count,
        true_negative_count,
        critical_false_merges,
        auto_accept_rate_basis_points: basis_points(auto_accept_count, total),
        escrow_rate_basis_points: basis_points(escrow_count, total),
        precision_basis_points,
        recall_basis_points,
        passes_quality_gates,
    }
}

fn accepts_thresholds(
    scores: CalibrateObservedScores,
    thresholds: CalibrateThresholdTuple,
) -> bool {
    scores.backbone_score >= thresholds.backbone_score_min
        && scores.attach_score >= thresholds.attach_score_min
        && scores.abstain_margin >= thresholds.abstain_margin
        && scores.match_score >= thresholds.match_threshold
        && scores.ambiguity_gap >= thresholds.ambiguity_gap
}

fn recommendation_from_truth_space(rows: &[CalibrateTruthSpaceRow]) -> CalibrateRecommendation {
    let selected = rows
        .iter()
        .filter(|row| row.passes_quality_gates)
        .max_by(|left, right| recommendation_cmp(left, right));

    match selected {
        Some(row) => CalibrateRecommendation {
            status: CalibrateRecommendationStatus::Recommended,
            reason: "max_auto_accept_subject_to_canon_entity_quality_v1".to_string(),
            selected_thresholds: Some(row.thresholds),
            selected_metrics: Some(CalibrateSelectedMetrics {
                row_index: row.row_index,
                auto_accept_count: row.auto_accept_count,
                escrow_count: row.escrow_count,
                auto_accept_rate_basis_points: row.auto_accept_rate_basis_points,
                escrow_rate_basis_points: row.escrow_rate_basis_points,
                precision_basis_points: row.precision_basis_points,
                recall_basis_points: row.recall_basis_points,
                critical_false_merges: row.critical_false_merges,
            }),
            proposed_strategy_yaml_fragment: Some(strategy_yaml_fragment(row.thresholds)),
        },
        None => CalibrateRecommendation {
            status: CalibrateRecommendationStatus::Blocked,
            reason:
                "no threshold tuple satisfied canon.entity.quality.v1; no fallback tuple selected"
                    .to_string(),
            selected_thresholds: None,
            selected_metrics: None,
            proposed_strategy_yaml_fragment: None,
        },
    }
}

fn recommendation_cmp(left: &CalibrateTruthSpaceRow, right: &CalibrateTruthSpaceRow) -> Ordering {
    left.auto_accept_count
        .cmp(&right.auto_accept_count)
        .then_with(|| {
            left.thresholds
                .strictness_units()
                .cmp(&right.thresholds.strictness_units())
        })
        .then_with(|| {
            left.thresholds
                .backbone_score_min
                .cmp(&right.thresholds.backbone_score_min)
        })
        .then_with(|| {
            left.thresholds
                .attach_score_min
                .cmp(&right.thresholds.attach_score_min)
        })
        .then_with(|| {
            left.thresholds
                .abstain_margin
                .cmp(&right.thresholds.abstain_margin)
        })
        .then_with(|| {
            left.thresholds
                .match_threshold
                .cmp(&right.thresholds.match_threshold)
        })
        .then_with(|| {
            left.thresholds
                .ambiguity_gap
                .cmp(&right.thresholds.ambiguity_gap)
        })
}

fn strategy_yaml_fragment(thresholds: CalibrateThresholdTuple) -> String {
    format!(
        "solver:\n  backbone_score_min: {}\n  attach_score_min: {}\n  abstain_margin: {}\n  match_threshold: {}\n  ambiguity_gap: {}\n",
        thresholds.backbone_score_min,
        thresholds.attach_score_min,
        thresholds.abstain_margin,
        thresholds.match_threshold,
        thresholds.ambiguity_gap
    )
}

fn pair_key_from_value(
    value: &Value,
    record_kind: &str,
    record_number: usize,
) -> Result<PairKey, Refusal> {
    if let Some(pair) = nested_value(value, "pair") {
        return pair_key_from_pair_array(pair, record_kind, record_number);
    }
    let Some(left_id) = string_field(value, LEFT_ID_FIELDS, record_number)? else {
        return Err(calibration_refusal(
            format!("Entity calibrate sweep {record_kind} record is missing a left ID"),
            json!({
                "stage": "calibrate_sweep",
                "record_kind": record_kind,
                "record_number": record_number,
                "accepted_fields": LEFT_ID_FIELDS,
                "writes_performed": false
            }),
        ));
    };
    let Some(right_id) = string_field(value, RIGHT_ID_FIELDS, record_number)? else {
        return Err(calibration_refusal(
            format!("Entity calibrate sweep {record_kind} record is missing a right ID"),
            json!({
                "stage": "calibrate_sweep",
                "record_kind": record_kind,
                "record_number": record_number,
                "accepted_fields": RIGHT_ID_FIELDS,
                "writes_performed": false
            }),
        ));
    };
    Ok(PairKey::new(left_id, right_id))
}

fn pair_key_from_pair_array(
    pair: &Value,
    record_kind: &str,
    record_number: usize,
) -> Result<PairKey, Refusal> {
    let Some(values) = pair.as_array() else {
        return Err(calibration_refusal(
            format!("Entity calibrate sweep {record_kind} pair field must be a two-string array"),
            json!({
                "stage": "calibrate_sweep",
                "record_kind": record_kind,
                "record_number": record_number,
                "field": "pair",
                "value": pair,
                "writes_performed": false
            }),
        ));
    };
    if values.len() != 2 {
        return Err(calibration_refusal(
            format!("Entity calibrate sweep {record_kind} pair field must contain exactly two IDs"),
            json!({
                "stage": "calibrate_sweep",
                "record_kind": record_kind,
                "record_number": record_number,
                "field": "pair",
                "actual_len": values.len(),
                "writes_performed": false
            }),
        ));
    }
    let mut ids = Vec::with_capacity(2);
    for (index, value) in values.iter().enumerate() {
        let Some(text) = value.as_str() else {
            return Err(calibration_refusal(
                format!("Entity calibrate sweep {record_kind} pair ID must be a string"),
                json!({
                    "stage": "calibrate_sweep",
                    "record_kind": record_kind,
                    "record_number": record_number,
                    "field": "pair",
                    "index": index,
                    "value": value,
                    "writes_performed": false
                }),
            ));
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(calibration_refusal(
                format!("Entity calibrate sweep {record_kind} pair ID must not be empty"),
                json!({
                    "stage": "calibrate_sweep",
                    "record_kind": record_kind,
                    "record_number": record_number,
                    "field": "pair",
                    "index": index,
                    "writes_performed": false
                }),
            ));
        }
        ids.push(trimmed.to_string());
    }
    let mut ids = ids.into_iter();
    let left_id = ids.next().expect("pair length checked before parse");
    let right_id = ids.next().expect("pair length checked before parse");
    Ok(PairKey::new(left_id, right_id))
}

fn required_score(
    value: &Value,
    fields: &[&str],
    canonical_field: &str,
    record_number: usize,
) -> Result<u32, Refusal> {
    optional_score(value, fields, canonical_field, record_number)?.ok_or_else(|| {
        calibration_refusal(
            format!("Entity calibrate sweep prediction record is missing {canonical_field}"),
            json!({
                "stage": "calibrate_sweep",
                "record_number": record_number,
                "accepted_fields": fields,
                "writes_performed": false
            }),
        )
    })
}

fn optional_score(
    value: &Value,
    fields: &[&str],
    canonical_field: &str,
    record_number: usize,
) -> Result<Option<u32>, Refusal> {
    for field in fields {
        if let Some(found) = nested_value(value, field) {
            return parse_score_value(found, canonical_field, field, record_number).map(Some);
        }
    }
    Ok(None)
}

fn parse_score_value(
    value: &Value,
    canonical_field: &str,
    actual_field: &str,
    record_number: usize,
) -> Result<u32, Refusal> {
    let parsed = if let Some(unsigned) = value.as_u64() {
        unsigned
    } else if let Some(signed) = value.as_i64() {
        if signed < 0 {
            return Err(invalid_score_refusal(
                value,
                canonical_field,
                actual_field,
                record_number,
            ));
        }
        signed as u64
    } else if let Some(text) = value.as_str() {
        text.parse::<u64>().map_err(|_| {
            invalid_score_refusal(value, canonical_field, actual_field, record_number)
        })?
    } else {
        return Err(invalid_score_refusal(
            value,
            canonical_field,
            actual_field,
            record_number,
        ));
    };
    if parsed > u64::from(ENTITY_SCORE_SCALE) {
        return Err(calibration_refusal(
            format!("Entity calibrate sweep {canonical_field} is outside score-unit range"),
            json!({
                "stage": "calibrate_sweep",
                "record_number": record_number,
                "field": actual_field,
                "value": value,
                "max": ENTITY_SCORE_SCALE,
                "writes_performed": false
            }),
        ));
    }
    Ok(parsed as u32)
}

fn invalid_score_refusal(
    value: &Value,
    canonical_field: &str,
    actual_field: &str,
    record_number: usize,
) -> Refusal {
    calibration_refusal(
        format!("Entity calibrate sweep {canonical_field} must be an integer score unit"),
        json!({
            "stage": "calibrate_sweep",
            "record_number": record_number,
            "field": actual_field,
            "value": value,
            "writes_performed": false
        }),
    )
}

fn string_field(
    value: &Value,
    fields: &[&str],
    record_number: usize,
) -> Result<Option<String>, Refusal> {
    for field in fields {
        if let Some(found) = nested_value(value, field) {
            let Some(text) = found.as_str() else {
                return Err(calibration_refusal(
                    "Entity calibrate sweep ID field must be a string",
                    json!({
                        "stage": "calibrate_sweep",
                        "record_number": record_number,
                        "field": field,
                        "value": found,
                        "writes_performed": false
                    }),
                ));
            };
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err(calibration_refusal(
                    "Entity calibrate sweep ID field must not be empty",
                    json!({
                        "stage": "calibrate_sweep",
                        "record_number": record_number,
                        "field": field,
                        "writes_performed": false
                    }),
                ));
            }
            return Ok(Some(trimmed.to_string()));
        }
    }
    Ok(None)
}

fn bool_field(value: &Value, fields: &[&str], record_number: usize) -> Result<bool, Refusal> {
    for field in fields {
        if let Some(found) = nested_value(value, field) {
            let Some(flag) = found.as_bool() else {
                return Err(calibration_refusal(
                    "Entity calibrate sweep boolean field must be true or false",
                    json!({
                        "stage": "calibrate_sweep",
                        "record_number": record_number,
                        "field": field,
                        "value": found,
                        "writes_performed": false
                    }),
                ));
            };
            return Ok(flag);
        }
    }
    Ok(false)
}

fn gold_label(value: &Value, record_number: usize) -> Result<GoldLabel, Refusal> {
    let label = string_field(value, &["label", "truth", "expected_class"], record_number)?;
    let Some(label) = label else {
        if nested_value(value, "expected_reference_id").is_some() {
            return Ok(GoldLabel::SameEntity);
        }
        return Err(calibration_refusal(
            "Entity calibrate sweep gold record is missing label",
            json!({
                "stage": "calibrate_sweep",
                "record_number": record_number,
                "accepted_fields": ["label", "truth", "expected_class"],
                "writes_performed": false
            }),
        ));
    };
    match label.as_str() {
        "same" | "same_entity" | "match" | "must_link" | "positive" | "true" | "1" => {
            Ok(GoldLabel::SameEntity)
        }
        "distinct" | "distinct_entity" | "non_match" | "cannot_link" | "negative" | "false"
        | "0" => Ok(GoldLabel::DistinctEntity),
        _ => Err(calibration_refusal(
            "Entity calibrate sweep gold label is unknown",
            json!({
                "stage": "calibrate_sweep",
                "record_number": record_number,
                "label": label,
                "writes_performed": false
            }),
        )),
    }
}

fn nested_value<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn basis_points(numerator: u64, denominator: u64) -> u32 {
    if denominator == 0 {
        return 0;
    }
    let scaled = (u128::from(numerator) * u128::from(ENTITY_SCORE_SCALE)
        + (u128::from(denominator) / 2))
        / u128::from(denominator);
    scaled.min(u128::from(ENTITY_SCORE_SCALE)) as u32
}

fn nonzero_basis_points(numerator: u64, denominator: u64) -> Option<u32> {
    (denominator > 0).then(|| basis_points(numerator, denominator))
}

fn canonical_prediction_hash(
    predictions: &BTreeMap<PairKey, CalibrationPrediction>,
) -> Result<String, Refusal> {
    let records = predictions
        .iter()
        .map(|(key, prediction)| CanonicalPredictionRecord {
            left_id: &key.left_id,
            right_id: &key.right_id,
            scores: prediction.scores,
            hard_cannot_link: prediction.hard_cannot_link,
        })
        .collect::<Vec<_>>();
    serde_json::to_vec(&records)
        .map(|bytes| witness::hash_bytes(&bytes))
        .map_err(|error| {
            calibration_refusal(
                "Failed to hash canonical calibration predictions",
                json!({
                    "stage": "calibrate_sweep",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })
}

fn canonical_gold_hash(gold: &BTreeMap<PairKey, CalibrationGold>) -> Result<String, Refusal> {
    let records = gold
        .iter()
        .map(|(key, record)| CanonicalGoldRecord {
            left_id: &key.left_id,
            right_id: &key.right_id,
            label: match record.label {
                GoldLabel::SameEntity => "same_entity",
                GoldLabel::DistinctEntity => "distinct_entity",
            },
            critical: record.critical,
        })
        .collect::<Vec<_>>();
    serde_json::to_vec(&records)
        .map(|bytes| witness::hash_bytes(&bytes))
        .map_err(|error| {
            calibration_refusal(
                "Failed to hash canonical calibration gold",
                json!({
                    "stage": "calibrate_sweep",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })
}

fn calibration_refusal(message: impl Into<String>, detail: Value) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: message.into(),
        detail,
        next_command: Some(NEXT_COMMAND.to_string()),
    }
}

impl PairKey {
    fn new(left_id: String, right_id: String) -> Self {
        if left_id <= right_id {
            Self { left_id, right_id }
        } else {
            Self {
                left_id: right_id,
                right_id: left_id,
            }
        }
    }

    fn display(&self) -> String {
        format!("{}|{}", self.left_id, self.right_id)
    }
}
