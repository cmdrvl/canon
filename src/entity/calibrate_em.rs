#![forbid(unsafe_code)]

//! Read-only Fellegi-Sunter EM weight suggestions for entity evidence artifacts.
//!
//! This module consumes already-materialized edge evidence. It collapses raw
//! pair records into agreement-pattern counts before iteration, runs a
//! deterministic fixed-point EM loop, and emits an advisory support-score YAML
//! fragment for operator review. It never mutates a strategy or registry.

use crate::{
    Refusal,
    entity::{error::EntityRefusalKind, score::ENTITY_SCORE_SCALE},
    witness,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path},
};

pub const CANON_ENTITY_CALIBRATE_EM_VERSION: &str = "canon.entity.calibrate_em.v0";
pub const CALIBRATE_EM_PROBABILITY_SCALE: u32 = 1_000_000;
pub const CALIBRATE_EM_DEFAULT_MAX_ITERATIONS: u32 = 25;
pub const CALIBRATE_EM_CONVERGENCE_EPSILON_UNITS: u32 = 100;
pub const CALIBRATE_EM_LOG_ODDS_BASE: &str = "log2_milli";

const NEXT_COMMAND: &str =
    "canon entity calibrate em <EVIDENCE.jsonl> --strategy <STRATEGY.yaml> --emit json";
const MIN_PROBABILITY_UNITS: u32 = 1_000;
const MAX_PROBABILITY_UNITS: u32 = CALIBRATE_EM_PROBABILITY_SCALE - MIN_PROBABILITY_UNITS;
const LOG_ODDS_SCORE_CAP_MILLI: i64 = 10_000;
const LOG2_FRACTION_BITS: u32 = 32;
const LOG2_ITERATIONS: u32 = 16;
const EVIDENCE_ARRAY_FIELDS: &[&str] = &[
    "edge_records",
    "evidence_records",
    "records",
    "evidence",
    "pairs",
];
const LEFT_ID_FIELDS: &[&str] = &[
    "left_surface_id",
    "left_id",
    "left_row_id",
    "source_id_l",
    "id_l",
];
const RIGHT_ID_FIELDS: &[&str] = &[
    "right_surface_id",
    "right_id",
    "right_row_id",
    "source_id_r",
    "id_r",
];
const SUPPORT_COLLECTION_FIELDS: &[&str] = &["hits", "support", "evidence", "support_hits"];
const OPERATOR_ID_FIELDS: &[&str] = &["operator_id", "rule_id", "source_id"];
const SCORE_UNIT_FIELDS: &[&str] = &["score_units", "source_score_units", "support_score_units"];
const AGREEMENT_FIELDS: &[&str] = &["agreement", "agreed", "matched", "match"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalibrateEmRequest<'a> {
    pub evidence: &'a Path,
    pub strategy: &'a Path,
    pub options: CalibrateEmOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalibrateEmOptions {
    pub max_iterations: u32,
    pub convergence_epsilon_units: u32,
}

impl Default for CalibrateEmOptions {
    fn default() -> Self {
        Self {
            max_iterations: CALIBRATE_EM_DEFAULT_MAX_ITERATIONS,
            convergence_epsilon_units: CALIBRATE_EM_CONVERGENCE_EPSILON_UNITS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateEmReport {
    pub version: String,
    pub read_only: bool,
    pub writes_performed: bool,
    pub metric_units: String,
    pub inputs: CalibrateEmInputSummary,
    pub config: CalibrateEmConfig,
    pub aggregate: CalibrateEmAggregateSummary,
    pub convergence: CalibrateEmConvergence,
    pub operators: Vec<CalibrateEmOperatorEstimate>,
    pub recommendation: CalibrateEmRecommendation,
    pub warnings: Vec<CalibrateEmWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateEmInputSummary {
    pub evidence_path: String,
    pub evidence_set_hash: String,
    pub strategy_path: String,
    pub strategy_hash: String,
    pub strategy_id: String,
    pub strategy_version: String,
    pub pair_count: u64,
    pub support_operator_count: u64,
    pub pattern_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateEmConfig {
    pub probability_scale: u32,
    pub score_units_scale: u32,
    pub log_odds_units: String,
    pub max_iterations: u32,
    pub convergence_epsilon_units: u32,
    pub u_estimation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateEmAggregateSummary {
    pub source: String,
    pub pair_count: u64,
    pub pattern_count: u64,
    pub patterns: Vec<CalibrateEmAgreementPattern>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateEmAgreementPattern {
    pub count: u64,
    pub operators: Vec<CalibrateEmPatternValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateEmPatternValue {
    pub operator_id: String,
    pub agreed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateEmConvergence {
    pub status: CalibrateEmConvergenceStatus,
    pub iterations: u32,
    pub max_iterations: u32,
    pub max_delta_units: u32,
    pub convergence_epsilon_units: u32,
    pub match_prior_probability_units: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrateEmConvergenceStatus {
    Converged,
    NotConverged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateEmOperatorEstimate {
    pub operator_id: String,
    pub agreement_count: u64,
    pub disagreement_count: u64,
    pub m_probability_units: u32,
    pub u_probability_units: u32,
    pub log_odds_weight_units: i64,
    pub proposed_support_score_units: Option<u32>,
    pub proposal_status: CalibrateEmProposalStatus,
    pub warning_codes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrateEmProposalStatus {
    Proposed,
    Suppressed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateEmRecommendation {
    pub status: CalibrateEmRecommendationStatus,
    pub reason: String,
    pub proposed_operator_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed_strategy_yaml_fragment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrateEmRecommendationStatus {
    Recommended,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrateEmWarning {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator_id: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub detail: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StrategyCalibrationInput {
    id: String,
    version: String,
    content_hash: String,
    support_operator_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PairEvidence {
    key: PairKey,
    agreements: BTreeSet<String>,
    mentioned_operators: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AggregatedEvidence {
    operator_ids: Vec<String>,
    pair_count: u64,
    agreement_counts: Vec<u64>,
    patterns: Vec<AgreementPatternCount>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgreementPatternCount {
    agreements: Vec<bool>,
    count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EmOperatorState {
    m_probability_units: u32,
    u_probability_units: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EmComputation {
    convergence: CalibrateEmConvergence,
    states: Vec<EmOperatorState>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PairKey {
    left_id: String,
    right_id: String,
}

pub fn run_calibrate_em(request: CalibrateEmRequest<'_>) -> Result<CalibrateEmReport, Refusal> {
    validate_options(request.options)?;
    guard_unsealed_path(request.evidence, "evidence")?;
    guard_unsealed_path(request.strategy, "strategy")?;

    let pairs = read_evidence_pairs(request.evidence)?;
    let strategy = read_strategy(request.strategy)?;
    let operator_ids = support_operator_ids(&pairs, &strategy)?;
    let aggregate = aggregate_evidence(&pairs, operator_ids)?;
    let em = run_em(&aggregate, request.options);
    let mut warnings = Vec::new();
    if em.convergence.status == CalibrateEmConvergenceStatus::NotConverged {
        warnings.push(warning(
            "em_not_converged",
            None,
            "Entity calibrate EM reached the fixed iteration cap before convergence",
            [
                ("iterations", em.convergence.iterations.to_string()),
                (
                    "max_delta_units",
                    em.convergence.max_delta_units.to_string(),
                ),
                (
                    "convergence_epsilon_units",
                    em.convergence.convergence_epsilon_units.to_string(),
                ),
            ],
        ));
    }
    let operators = operator_estimates(&aggregate, &em, &mut warnings);
    let proposed_strategy_yaml_fragment = proposed_strategy_yaml_fragment(&operators);
    let proposed_operator_count = operators
        .iter()
        .filter(|estimate| estimate.proposal_status == CalibrateEmProposalStatus::Proposed)
        .count() as u64;
    let recommendation = recommendation(proposed_operator_count, &em.convergence);

    Ok(CalibrateEmReport {
        version: CANON_ENTITY_CALIBRATE_EM_VERSION.to_string(),
        read_only: true,
        writes_performed: false,
        metric_units: "integer_fixed_point".to_string(),
        inputs: CalibrateEmInputSummary {
            evidence_path: request.evidence.display().to_string(),
            evidence_set_hash: canonical_evidence_hash(&pairs)?,
            strategy_path: request.strategy.display().to_string(),
            strategy_hash: strategy.content_hash,
            strategy_id: strategy.id,
            strategy_version: strategy.version,
            pair_count: aggregate.pair_count,
            support_operator_count: aggregate.operator_ids.len() as u64,
            pattern_count: aggregate.patterns.len() as u64,
        },
        config: CalibrateEmConfig {
            probability_scale: CALIBRATE_EM_PROBABILITY_SCALE,
            score_units_scale: ENTITY_SCORE_SCALE,
            log_odds_units: CALIBRATE_EM_LOG_ODDS_BASE.to_string(),
            max_iterations: request.options.max_iterations,
            convergence_epsilon_units: request.options.convergence_epsilon_units,
            u_estimation: "full_evidence_artifact_no_sampling".to_string(),
        },
        aggregate: aggregate_summary(&aggregate),
        convergence: em.convergence,
        operators,
        recommendation: CalibrateEmRecommendation {
            proposed_strategy_yaml_fragment,
            ..recommendation
        },
        warnings,
    })
}

pub fn canonical_calibrate_em_report_bytes(report: &CalibrateEmReport) -> Result<Vec<u8>, Refusal> {
    serde_json::to_vec(report).map_err(|error| {
        calibrate_em_refusal(
            "Failed to serialize entity calibrate EM report",
            json!({
                "stage": "calibrate_em",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })
}

pub fn render_calibrate_em_summary(report: &CalibrateEmReport) -> String {
    let warning_count = report.warnings.len();
    let proposed_count = report.recommendation.proposed_operator_count;
    let header = match report.recommendation.status {
        CalibrateEmRecommendationStatus::Recommended => "recommended",
        CalibrateEmRecommendationStatus::Blocked => "blocked",
    };
    let fragment = report
        .recommendation
        .proposed_strategy_yaml_fragment
        .as_deref()
        .unwrap_or("");
    format!(
        "{} {}\nmetric_units={}\npairs={}\npatterns={}\noperators={}\nproposed_operators={}\nwarnings={}\niterations={}\nmax_delta_units={}\n{}",
        report.version,
        header,
        report.metric_units,
        report.inputs.pair_count,
        report.inputs.pattern_count,
        report.inputs.support_operator_count,
        proposed_count,
        warning_count,
        report.convergence.iterations,
        report.convergence.max_delta_units,
        fragment
    )
}

pub fn validate_proposed_strategy_yaml_fragment(fragment: &str) -> Result<(), Refusal> {
    let value = serde_yaml::from_str::<serde_yaml::Value>(fragment).map_err(|error| {
        EntityRefusalKind::Strategy.to_refusal(
            "Entity calibrate EM proposed strategy YAML fragment is invalid",
            json!({
                "stage": "calibrate_em",
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some(NEXT_COMMAND.to_string()),
        )
    })?;
    let Some(mapping) = value.as_mapping() else {
        return Err(EntityRefusalKind::Strategy.to_refusal(
            "Entity calibrate EM proposed strategy YAML fragment must be a mapping",
            json!({
                "stage": "calibrate_em",
                "writes_performed": false
            }),
            Some(NEXT_COMMAND.to_string()),
        ));
    };
    let support_scores = mapping.get(serde_yaml::Value::String("support_scores".to_string()));
    if !matches!(support_scores, Some(serde_yaml::Value::Mapping(_))) {
        return Err(EntityRefusalKind::Strategy.to_refusal(
            "Entity calibrate EM proposed strategy YAML fragment must contain support_scores",
            json!({
                "stage": "calibrate_em",
                "field": "support_scores",
                "writes_performed": false
            }),
            Some(NEXT_COMMAND.to_string()),
        ));
    }
    Ok(())
}

fn validate_options(options: CalibrateEmOptions) -> Result<(), Refusal> {
    if options.max_iterations == 0 {
        return Err(calibrate_em_refusal(
            "Entity calibrate EM max_iterations must be positive",
            json!({
                "stage": "calibrate_em",
                "field": "max_iterations",
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn guard_unsealed_path(path: &Path, role: &str) -> Result<(), Refusal> {
    for component in path.components() {
        let Component::Normal(part) = component else {
            continue;
        };
        let lower = part.to_string_lossy().to_ascii_lowercase();
        if lower.contains("holdout") || lower.contains("sealed") || lower.contains("acceptance") {
            return Err(EntityRefusalKind::InputContract.to_refusal(
                "Entity calibrate EM cannot train from sealed acceptance or holdout artifacts",
                json!({
                    "stage": "calibrate_em",
                    "reason": "sealed_acceptance_or_holdout_input",
                    "path_role": role,
                    "path": path.display().to_string(),
                    "path_component": lower,
                    "writes_performed": false
                }),
                Some("Use training or tuning evidence artifacts, not sealed acceptance/holdout suites".to_string()),
            ));
        }
    }
    Ok(())
}

fn read_evidence_pairs(path: &Path) -> Result<Vec<PairEvidence>, Refusal> {
    let records = read_json_records(path, "evidence", EVIDENCE_ARRAY_FIELDS)?;
    let mut pairs = BTreeMap::new();
    for (index, value) in records.iter().enumerate() {
        let pair = pair_evidence_from_value(value, index + 1)?;
        if pairs.insert(pair.key.clone(), pair).is_some() {
            return Err(calibrate_em_refusal(
                "Entity calibrate EM evidence contains duplicate pair records",
                json!({
                    "stage": "calibrate_em",
                    "record_number": index + 1,
                    "writes_performed": false
                }),
            ));
        }
    }
    if pairs.is_empty() {
        return Err(calibrate_em_refusal(
            "Entity calibrate EM evidence contains no pair records",
            json!({
                "stage": "calibrate_em",
                "writes_performed": false
            }),
        ));
    }
    Ok(pairs.into_values().collect())
}

fn read_json_records(
    path: &Path,
    artifact_label: &str,
    array_fields: &[&str],
) -> Result<Vec<Value>, Refusal> {
    let bytes = fs::read(path).map_err(|error| {
        EntityRefusalKind::IoBudget.to_refusal(
            format!(
                "Failed to read entity calibrate EM {artifact_label} '{}'",
                path.display()
            ),
            json!({
                "stage": "calibrate_em",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some(NEXT_COMMAND.to_string()),
        )
    })?;
    let content = std::str::from_utf8(&bytes).map_err(|error| {
        calibrate_em_refusal(
            format!("Entity calibrate EM {artifact_label} must be UTF-8 JSON or JSONL"),
            json!({
                "stage": "calibrate_em",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    let trimmed = content.trim_start();
    if trimmed.is_empty() {
        return Err(calibrate_em_refusal(
            format!("Entity calibrate EM {artifact_label} is empty"),
            json!({
                "stage": "calibrate_em",
                "path": path.display().to_string(),
                "writes_performed": false
            }),
        ));
    }

    if (trimmed.starts_with('[') || trimmed.starts_with('{'))
        && let Ok(value) = serde_json::from_str::<Value>(content)
    {
        return records_from_json_value(value, path, artifact_label, array_fields);
    }

    let mut records = Vec::new();
    for (line_index, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str::<Value>(line).map_err(|error| {
            calibrate_em_refusal(
                format!(
                    "Invalid entity calibrate EM {artifact_label} JSONL on line {}",
                    line_index + 1
                ),
                json!({
                    "stage": "calibrate_em",
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
        return Err(calibrate_em_refusal(
            format!("Entity calibrate EM {artifact_label} contains no records"),
            json!({
                "stage": "calibrate_em",
                "path": path.display().to_string(),
                "writes_performed": false
            }),
        ));
    }
    Ok(records)
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
                    return Err(calibrate_em_refusal(
                        format!("Entity calibrate EM {artifact_label} array field is not an array"),
                        json!({
                            "stage": "calibrate_em",
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
            return Err(calibrate_em_refusal(
                format!(
                    "Entity calibrate EM {artifact_label} must be a JSON object, array, or JSONL records"
                ),
                json!({
                    "stage": "calibrate_em",
                    "path": path.display().to_string(),
                    "actual": other,
                    "writes_performed": false
                }),
            ));
        }
    };
    if records.is_empty() {
        return Err(calibrate_em_refusal(
            format!("Entity calibrate EM {artifact_label} contains no records"),
            json!({
                "stage": "calibrate_em",
                "path": path.display().to_string(),
                "writes_performed": false
            }),
        ));
    }
    Ok(records)
}

fn read_strategy(path: &Path) -> Result<StrategyCalibrationInput, Refusal> {
    let bytes = fs::read(path).map_err(|error| {
        EntityRefusalKind::IoBudget.to_refusal(
            format!(
                "Failed to read entity calibrate EM strategy '{}'",
                path.display()
            ),
            json!({
                "stage": "calibrate_em",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some(NEXT_COMMAND.to_string()),
        )
    })?;
    let value = serde_yaml::from_slice::<serde_yaml::Value>(&bytes).map_err(|error| {
        EntityRefusalKind::Strategy.to_refusal(
            "Invalid entity calibrate EM strategy YAML",
            json!({
                "stage": "calibrate_em",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some(NEXT_COMMAND.to_string()),
        )
    })?;
    let mut support_operator_ids = BTreeSet::new();
    collect_support_score_keys(&value, &mut support_operator_ids);
    Ok(StrategyCalibrationInput {
        id: yaml_string(&value, "strategy_id")
            .or_else(|| yaml_string(&value, "id"))
            .unwrap_or_else(|| "entity_calibrate_em_strategy".to_string()),
        version: yaml_string(&value, "strategy_version")
            .or_else(|| yaml_string(&value, "version"))
            .unwrap_or_else(|| "v0".to_string()),
        content_hash: witness::hash_bytes(&bytes),
        support_operator_ids,
    })
}

fn pair_evidence_from_value(value: &Value, record_number: usize) -> Result<PairEvidence, Refusal> {
    let key = pair_key_from_value(value, record_number)?;
    let mut agreements = BTreeSet::new();
    let mut mentioned_operators = BTreeSet::new();
    for field in SUPPORT_COLLECTION_FIELDS {
        let Some(collection) = nested_value(value, field) else {
            continue;
        };
        let Some(hits) = collection.as_array() else {
            return Err(calibrate_em_refusal(
                "Entity calibrate EM support hit collection must be an array",
                json!({
                    "stage": "calibrate_em",
                    "record_number": record_number,
                    "field": field,
                    "writes_performed": false
                }),
            ));
        };
        for hit in hits {
            if let Some(parsed) = support_hit_from_value(hit, field, record_number)? {
                mentioned_operators.insert(parsed.operator_id.clone());
                if parsed.agreed {
                    agreements.insert(parsed.operator_id);
                }
            }
        }
    }
    if let Some(parsed) = support_hit_from_value(value, "record", record_number)? {
        mentioned_operators.insert(parsed.operator_id.clone());
        if parsed.agreed {
            agreements.insert(parsed.operator_id);
        }
    }
    Ok(PairEvidence {
        key,
        agreements,
        mentioned_operators,
    })
}

fn pair_key_from_value(value: &Value, record_number: usize) -> Result<PairKey, Refusal> {
    if let Some(pair) = nested_value(value, "pair") {
        return pair_key_from_pair_array(pair, record_number);
    }
    let Some(left_id) = string_field(value, LEFT_ID_FIELDS, record_number)? else {
        return Err(calibrate_em_refusal(
            "Entity calibrate EM evidence record is missing a left ID",
            json!({
                "stage": "calibrate_em",
                "record_number": record_number,
                "accepted_fields": LEFT_ID_FIELDS,
                "writes_performed": false
            }),
        ));
    };
    let Some(right_id) = string_field(value, RIGHT_ID_FIELDS, record_number)? else {
        return Err(calibrate_em_refusal(
            "Entity calibrate EM evidence record is missing a right ID",
            json!({
                "stage": "calibrate_em",
                "record_number": record_number,
                "accepted_fields": RIGHT_ID_FIELDS,
                "writes_performed": false
            }),
        ));
    };
    Ok(PairKey::new(left_id, right_id))
}

fn pair_key_from_pair_array(pair: &Value, record_number: usize) -> Result<PairKey, Refusal> {
    let Some(values) = pair.as_array() else {
        return Err(calibrate_em_refusal(
            "Entity calibrate EM pair field must be a two-string array",
            json!({
                "stage": "calibrate_em",
                "record_number": record_number,
                "field": "pair",
                "writes_performed": false
            }),
        ));
    };
    if values.len() != 2 {
        return Err(calibrate_em_refusal(
            "Entity calibrate EM pair field must contain exactly two IDs",
            json!({
                "stage": "calibrate_em",
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
            return Err(calibrate_em_refusal(
                "Entity calibrate EM pair ID must be a string",
                json!({
                    "stage": "calibrate_em",
                    "record_number": record_number,
                    "field": "pair",
                    "index": index,
                    "writes_performed": false
                }),
            ));
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(calibrate_em_refusal(
                "Entity calibrate EM pair ID must not be empty",
                json!({
                    "stage": "calibrate_em",
                    "record_number": record_number,
                    "field": "pair",
                    "index": index,
                    "writes_performed": false
                }),
            ));
        }
        ids.push(trimmed.to_string());
    }
    Ok(PairKey::new(ids[0].clone(), ids[1].clone()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedSupportHit {
    operator_id: String,
    agreed: bool,
}

fn support_hit_from_value(
    value: &Value,
    parent_field: &str,
    record_number: usize,
) -> Result<Option<ParsedSupportHit>, Refusal> {
    let Some(object) = value.as_object() else {
        return Err(calibrate_em_refusal(
            "Entity calibrate EM support hit must be an object",
            json!({
                "stage": "calibrate_em",
                "record_number": record_number,
                "field": parent_field,
                "writes_performed": false
            }),
        ));
    };
    if parent_field == "record"
        && !OPERATOR_ID_FIELDS
            .iter()
            .any(|field| object.contains_key(*field))
    {
        return Ok(None);
    }
    if let Some(lane) = string_field(value, &["lane", "score_lane"], record_number)? {
        match lane.as_str() {
            "support" => {}
            "anti_merge" | "relation_hint" => return Ok(None),
            _ if parent_field == "hits" => return Ok(None),
            _ => {}
        }
    }
    let Some(operator_id) = string_field(value, OPERATOR_ID_FIELDS, record_number)? else {
        return Err(calibrate_em_refusal(
            "Entity calibrate EM support hit is missing an operator ID",
            json!({
                "stage": "calibrate_em",
                "record_number": record_number,
                "field": parent_field,
                "accepted_fields": OPERATOR_ID_FIELDS,
                "writes_performed": false
            }),
        ));
    };
    let agreement = optional_bool_field(value, AGREEMENT_FIELDS, record_number)?;
    let score_units = optional_u64_field(value, SCORE_UNIT_FIELDS, record_number)?;
    let agreed = agreement.unwrap_or_else(|| score_units.is_none_or(|score| score > 0));
    Ok(Some(ParsedSupportHit {
        operator_id: operator_from_source_id(&operator_id),
        agreed,
    }))
}

fn support_operator_ids(
    pairs: &[PairEvidence],
    strategy: &StrategyCalibrationInput,
) -> Result<Vec<String>, Refusal> {
    let mut ids = strategy.support_operator_ids.clone();
    for pair in pairs {
        ids.extend(pair.mentioned_operators.iter().cloned());
    }
    if ids.is_empty() {
        return Err(calibrate_em_refusal(
            "Entity calibrate EM found no support evidence operators",
            json!({
                "stage": "calibrate_em",
                "reason": "no_support_operators",
                "writes_performed": false
            }),
        ));
    }
    Ok(ids.into_iter().collect())
}

fn aggregate_evidence(
    pairs: &[PairEvidence],
    operator_ids: Vec<String>,
) -> Result<AggregatedEvidence, Refusal> {
    if pairs.is_empty() {
        return Err(calibrate_em_refusal(
            "Entity calibrate EM cannot aggregate an empty evidence set",
            json!({
                "stage": "calibrate_em",
                "writes_performed": false
            }),
        ));
    }
    let mut agreement_counts = vec![0u64; operator_ids.len()];
    let mut pattern_counts = BTreeMap::<Vec<bool>, u64>::new();
    for pair in pairs {
        let mut pattern = Vec::with_capacity(operator_ids.len());
        for (index, operator_id) in operator_ids.iter().enumerate() {
            let agreed = pair.agreements.contains(operator_id);
            if agreed {
                agreement_counts[index] += 1;
            }
            pattern.push(agreed);
        }
        *pattern_counts.entry(pattern).or_insert(0) += 1;
    }
    Ok(AggregatedEvidence {
        operator_ids,
        pair_count: pairs.len() as u64,
        agreement_counts,
        patterns: pattern_counts
            .into_iter()
            .map(|(agreements, count)| AgreementPatternCount { agreements, count })
            .collect(),
    })
}

fn run_em(aggregate: &AggregatedEvidence, options: CalibrateEmOptions) -> EmComputation {
    let mut states = initial_operator_states(aggregate);
    let mut prior = CALIBRATE_EM_PROBABILITY_SCALE / 2;
    let mut max_delta = CALIBRATE_EM_PROBABILITY_SCALE;
    let mut iterations = 0;
    let mut status = CalibrateEmConvergenceStatus::NotConverged;

    for iteration in 1..=options.max_iterations {
        let next = em_iteration(aggregate, &states, prior);
        max_delta = max_parameter_delta(prior, &states, next.prior, &next.states);
        states = next.states;
        prior = next.prior;
        iterations = iteration;
        if max_delta <= options.convergence_epsilon_units {
            status = CalibrateEmConvergenceStatus::Converged;
            break;
        }
    }

    EmComputation {
        convergence: CalibrateEmConvergence {
            status,
            iterations,
            max_iterations: options.max_iterations,
            max_delta_units: max_delta,
            convergence_epsilon_units: options.convergence_epsilon_units,
            match_prior_probability_units: prior,
        },
        states,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EmIteration {
    prior: u32,
    states: Vec<EmOperatorState>,
}

fn em_iteration(
    aggregate: &AggregatedEvidence,
    states: &[EmOperatorState],
    prior: u32,
) -> EmIteration {
    let operator_count = aggregate.operator_ids.len();
    let mut expected_match_total = 0u128;
    let mut expected_non_match_total = 0u128;
    let mut match_agreements = vec![0u128; operator_count];
    let mut non_match_agreements = vec![0u128; operator_count];

    for pattern in &aggregate.patterns {
        let posterior = u128::from(match_posterior(&pattern.agreements, states, prior));
        let count = u128::from(pattern.count);
        let non_match = u128::from(CALIBRATE_EM_PROBABILITY_SCALE) - posterior;
        expected_match_total += count * posterior;
        expected_non_match_total += count * non_match;
        for (index, agreed) in pattern.agreements.iter().enumerate() {
            if *agreed {
                match_agreements[index] += count * posterior;
                non_match_agreements[index] += count * non_match;
            }
        }
    }

    let total_probability_mass =
        u128::from(aggregate.pair_count) * u128::from(CALIBRATE_EM_PROBABILITY_SCALE);
    let prior = probability_from_scaled_count(expected_match_total, total_probability_mass);
    let states = (0..operator_count)
        .map(|index| EmOperatorState {
            m_probability_units: probability_from_scaled_count(
                match_agreements[index],
                expected_match_total,
            ),
            u_probability_units: probability_from_scaled_count(
                non_match_agreements[index],
                expected_non_match_total,
            ),
        })
        .collect();

    EmIteration { prior, states }
}

fn match_posterior(pattern: &[bool], states: &[EmOperatorState], prior: u32) -> u32 {
    let mut match_likelihood = u128::from(clamp_probability(prior));
    let mut non_match_likelihood = u128::from(clamp_probability(
        CALIBRATE_EM_PROBABILITY_SCALE.saturating_sub(prior),
    ));
    for (agreed, state) in pattern.iter().zip(states) {
        let m_factor = if *agreed {
            state.m_probability_units
        } else {
            CALIBRATE_EM_PROBABILITY_SCALE - state.m_probability_units
        };
        let u_factor = if *agreed {
            state.u_probability_units
        } else {
            CALIBRATE_EM_PROBABILITY_SCALE - state.u_probability_units
        };
        match_likelihood = scaled_probability_product(match_likelihood, m_factor);
        non_match_likelihood = scaled_probability_product(non_match_likelihood, u_factor);
    }
    let denominator = match_likelihood + non_match_likelihood;
    if denominator == 0 {
        return CALIBRATE_EM_PROBABILITY_SCALE / 2;
    }
    let posterior = (match_likelihood * u128::from(CALIBRATE_EM_PROBABILITY_SCALE)
        + (denominator / 2))
        / denominator;
    posterior.min(u128::from(CALIBRATE_EM_PROBABILITY_SCALE)) as u32
}

fn scaled_probability_product(left_units: u128, right_units: u32) -> u128 {
    (left_units * u128::from(right_units) + u128::from(CALIBRATE_EM_PROBABILITY_SCALE / 2))
        / u128::from(CALIBRATE_EM_PROBABILITY_SCALE)
}

fn initial_operator_states(aggregate: &AggregatedEvidence) -> Vec<EmOperatorState> {
    aggregate
        .agreement_counts
        .iter()
        .map(|agreement_count| {
            let agreement_units = probability_units(*agreement_count, aggregate.pair_count);
            let mut m_probability_units =
                clamp_probability((CALIBRATE_EM_PROBABILITY_SCALE + agreement_units) / 2);
            let mut u_probability_units = clamp_probability(agreement_units / 2);
            if m_probability_units <= u_probability_units {
                m_probability_units = CALIBRATE_EM_PROBABILITY_SCALE * 3 / 4;
                u_probability_units = CALIBRATE_EM_PROBABILITY_SCALE / 4;
            }
            EmOperatorState {
                m_probability_units,
                u_probability_units,
            }
        })
        .collect()
}

fn probability_from_scaled_count(numerator: u128, denominator: u128) -> u32 {
    if denominator == 0 {
        return CALIBRATE_EM_PROBABILITY_SCALE / 2;
    }
    let units =
        (numerator * u128::from(CALIBRATE_EM_PROBABILITY_SCALE) + (denominator / 2)) / denominator;
    clamp_probability(units.min(u128::from(CALIBRATE_EM_PROBABILITY_SCALE)) as u32)
}

fn probability_units(numerator: u64, denominator: u64) -> u32 {
    if denominator == 0 {
        return 0;
    }
    let units = (u128::from(numerator) * u128::from(CALIBRATE_EM_PROBABILITY_SCALE)
        + (u128::from(denominator) / 2))
        / u128::from(denominator);
    units.min(u128::from(CALIBRATE_EM_PROBABILITY_SCALE)) as u32
}

fn clamp_probability(value: u32) -> u32 {
    value.clamp(MIN_PROBABILITY_UNITS, MAX_PROBABILITY_UNITS)
}

fn max_parameter_delta(
    previous_prior: u32,
    previous_states: &[EmOperatorState],
    next_prior: u32,
    next_states: &[EmOperatorState],
) -> u32 {
    let mut max_delta = previous_prior.abs_diff(next_prior);
    for (previous, next) in previous_states.iter().zip(next_states) {
        max_delta = max_delta
            .max(
                previous
                    .m_probability_units
                    .abs_diff(next.m_probability_units),
            )
            .max(
                previous
                    .u_probability_units
                    .abs_diff(next.u_probability_units),
            );
    }
    max_delta
}

fn operator_estimates(
    aggregate: &AggregatedEvidence,
    em: &EmComputation,
    warnings: &mut Vec<CalibrateEmWarning>,
) -> Vec<CalibrateEmOperatorEstimate> {
    aggregate
        .operator_ids
        .iter()
        .enumerate()
        .map(|(index, operator_id)| {
            let agreement_count = aggregate.agreement_counts[index];
            let disagreement_count = aggregate.pair_count.saturating_sub(agreement_count);
            let state = em.states[index];
            let mut warning_codes = Vec::new();
            if agreement_count == 0 || disagreement_count == 0 {
                warning_codes.push("constant_operator".to_string());
                warnings.push(warning(
                    "constant_operator",
                    Some(operator_id),
                    "Entity calibrate EM operator is constant across the evidence corpus",
                    [
                        ("agreement_count", agreement_count.to_string()),
                        ("disagreement_count", disagreement_count.to_string()),
                    ],
                ));
            }
            if agreement_count == 0 {
                warning_codes.push("zero_count_cell".to_string());
                warnings.push(warning(
                    "zero_count_cell",
                    Some(operator_id),
                    "Entity calibrate EM operator has no agreement observations",
                    [("missing_cell", "agreement".to_string())],
                ));
            } else if disagreement_count == 0 {
                warning_codes.push("zero_count_cell".to_string());
                warnings.push(warning(
                    "zero_count_cell",
                    Some(operator_id),
                    "Entity calibrate EM operator has no disagreement observations",
                    [("missing_cell", "disagreement".to_string())],
                ));
            }
            if em.convergence.status == CalibrateEmConvergenceStatus::NotConverged {
                warning_codes.push("em_not_converged".to_string());
            }
            let log_odds_weight_units =
                log_odds_weight_units(state.m_probability_units, state.u_probability_units);
            if state.m_probability_units <= state.u_probability_units {
                warning_codes.push("non_discriminating_operator".to_string());
                warnings.push(warning(
                    "non_discriminating_operator",
                    Some(operator_id),
                    "Entity calibrate EM estimated m <= u for this support operator",
                    [
                        ("m_probability_units", state.m_probability_units.to_string()),
                        ("u_probability_units", state.u_probability_units.to_string()),
                    ],
                ));
            }
            warning_codes.sort();
            warning_codes.dedup();
            let proposed_support_score_units = if warning_codes.is_empty() {
                support_score_from_log_weight(log_odds_weight_units)
            } else {
                None
            };
            CalibrateEmOperatorEstimate {
                operator_id: operator_id.clone(),
                agreement_count,
                disagreement_count,
                m_probability_units: state.m_probability_units,
                u_probability_units: state.u_probability_units,
                log_odds_weight_units,
                proposed_support_score_units,
                proposal_status: if proposed_support_score_units.is_some() {
                    CalibrateEmProposalStatus::Proposed
                } else {
                    CalibrateEmProposalStatus::Suppressed
                },
                warning_codes,
            }
        })
        .collect()
}

fn log_odds_weight_units(m_probability_units: u32, u_probability_units: u32) -> i64 {
    let scale = u128::from(CALIBRATE_EM_PROBABILITY_SCALE);
    let m = u128::from(clamp_probability(m_probability_units));
    let u = u128::from(clamp_probability(u_probability_units));
    let numerator = m * (scale - u);
    let denominator = u * (scale - m);
    match numerator.cmp(&denominator) {
        Ordering::Greater => log2_ratio_milli(numerator, denominator),
        Ordering::Less => -log2_ratio_milli(denominator, numerator),
        Ordering::Equal => 0,
    }
}

fn log2_ratio_milli(numerator: u128, denominator: u128) -> i64 {
    if denominator == 0 || numerator <= denominator {
        return 0;
    }
    let q = (numerator << LOG2_FRACTION_BITS) / denominator;
    let bit_len = 128 - q.leading_zeros();
    let integer_part = i64::from(bit_len) - i64::from(LOG2_FRACTION_BITS) - 1;
    let shift = bit_len - 1;
    let mut normalized = if shift >= LOG2_FRACTION_BITS {
        q >> (shift - LOG2_FRACTION_BITS)
    } else {
        q << (LOG2_FRACTION_BITS - shift)
    };
    let one = 1u128 << LOG2_FRACTION_BITS;
    let two = one << 1;
    let mut fraction = 0u64;
    for bit_index in 1..=LOG2_ITERATIONS {
        normalized = (normalized * normalized) >> LOG2_FRACTION_BITS;
        if normalized >= two {
            normalized >>= 1;
            fraction |= 1u64 << (LOG2_ITERATIONS - bit_index);
        }
    }
    integer_part * 1_000 + ((fraction * 1_000) / (1u64 << LOG2_ITERATIONS)) as i64
}

fn support_score_from_log_weight(log_odds_weight_units: i64) -> Option<u32> {
    if log_odds_weight_units <= 0 {
        return None;
    }
    let scaled = (log_odds_weight_units.min(LOG_ODDS_SCORE_CAP_MILLI) as u128
        * u128::from(ENTITY_SCORE_SCALE))
        / u128::from(LOG_ODDS_SCORE_CAP_MILLI as u64);
    Some(scaled.min(u128::from(ENTITY_SCORE_SCALE)) as u32)
}

fn proposed_strategy_yaml_fragment(operators: &[CalibrateEmOperatorEstimate]) -> Option<String> {
    let proposed = operators
        .iter()
        .filter_map(|estimate| {
            estimate
                .proposed_support_score_units
                .map(|score| (estimate.operator_id.as_str(), score))
        })
        .collect::<Vec<_>>();
    if proposed.is_empty() {
        return None;
    }
    let mut fragment =
        "# proposed by canon.entity.calibrate_em.v0; review before freezing\nsupport_scores:\n"
            .to_string();
    for (operator_id, score) in proposed {
        fragment.push_str("  ");
        fragment.push_str(&single_quoted_yaml_key(operator_id));
        fragment.push_str(": ");
        fragment.push_str(&score.to_string());
        fragment.push('\n');
    }
    Some(fragment)
}

fn recommendation(
    proposed_operator_count: u64,
    convergence: &CalibrateEmConvergence,
) -> CalibrateEmRecommendation {
    if proposed_operator_count > 0 {
        return CalibrateEmRecommendation {
            status: CalibrateEmRecommendationStatus::Recommended,
            reason: "support_scores_proposed_for_operator_review".to_string(),
            proposed_operator_count,
            proposed_strategy_yaml_fragment: None,
        };
    }
    let reason = if convergence.status == CalibrateEmConvergenceStatus::NotConverged {
        "em_did_not_converge_no_support_scores_proposed"
    } else {
        "no_operator_had_non_degenerate_discriminating_cells"
    };
    CalibrateEmRecommendation {
        status: CalibrateEmRecommendationStatus::Blocked,
        reason: reason.to_string(),
        proposed_operator_count,
        proposed_strategy_yaml_fragment: None,
    }
}

fn aggregate_summary(aggregate: &AggregatedEvidence) -> CalibrateEmAggregateSummary {
    CalibrateEmAggregateSummary {
        source: "distinct_agreement_pattern_counts".to_string(),
        pair_count: aggregate.pair_count,
        pattern_count: aggregate.patterns.len() as u64,
        patterns: aggregate
            .patterns
            .iter()
            .map(|pattern| CalibrateEmAgreementPattern {
                count: pattern.count,
                operators: aggregate
                    .operator_ids
                    .iter()
                    .zip(&pattern.agreements)
                    .map(|(operator_id, agreed)| CalibrateEmPatternValue {
                        operator_id: operator_id.clone(),
                        agreed: *agreed,
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn canonical_evidence_hash(pairs: &[PairEvidence]) -> Result<String, Refusal> {
    #[derive(Serialize)]
    struct CanonicalPair<'a> {
        left_id: &'a str,
        right_id: &'a str,
        agreements: Vec<&'a str>,
    }
    let records = pairs
        .iter()
        .map(|pair| CanonicalPair {
            left_id: &pair.key.left_id,
            right_id: &pair.key.right_id,
            agreements: pair.agreements.iter().map(String::as_str).collect(),
        })
        .collect::<Vec<_>>();
    serde_json::to_vec(&records)
        .map(|bytes| witness::hash_bytes(&bytes))
        .map_err(|error| {
            calibrate_em_refusal(
                "Failed to hash canonical entity calibrate EM evidence",
                json!({
                    "stage": "calibrate_em",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })
}

fn warning<const N: usize>(
    code: &str,
    operator_id: Option<&str>,
    message: &str,
    detail: [(&str, String); N],
) -> CalibrateEmWarning {
    CalibrateEmWarning {
        code: code.to_string(),
        operator_id: operator_id.map(ToOwned::to_owned),
        message: message.to_string(),
        detail: detail
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    }
}

fn collect_support_score_keys(value: &serde_yaml::Value, ids: &mut BTreeSet<String>) {
    match value {
        serde_yaml::Value::Mapping(mapping) => {
            for (key, value) in mapping {
                if key.as_str() == Some("support_scores")
                    && let Some(scores) = value.as_mapping()
                {
                    for score_key in scores.keys() {
                        if let Some(operator_id) = score_key.as_str() {
                            let trimmed = operator_id.trim();
                            if !trimmed.is_empty() {
                                ids.insert(trimmed.to_string());
                            }
                        }
                    }
                }
                collect_support_score_keys(value, ids);
            }
        }
        serde_yaml::Value::Sequence(values) => {
            for value in values {
                collect_support_score_keys(value, ids);
            }
        }
        _ => {}
    }
}

fn yaml_string(value: &serde_yaml::Value, key: &str) -> Option<String> {
    value
        .as_mapping()
        .and_then(|mapping| mapping.get(serde_yaml::Value::String(key.to_string())))
        .and_then(serde_yaml::Value::as_str)
        .map(ToOwned::to_owned)
}

fn string_field(
    value: &Value,
    fields: &[&str],
    record_number: usize,
) -> Result<Option<String>, Refusal> {
    for field in fields {
        if let Some(found) = nested_value(value, field) {
            let Some(text) = found.as_str() else {
                return Err(calibrate_em_refusal(
                    "Entity calibrate EM field must be a string",
                    json!({
                        "stage": "calibrate_em",
                        "record_number": record_number,
                        "field": field,
                        "value": found,
                        "writes_performed": false
                    }),
                ));
            };
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err(calibrate_em_refusal(
                    "Entity calibrate EM field must not be empty",
                    json!({
                        "stage": "calibrate_em",
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

fn optional_bool_field(
    value: &Value,
    fields: &[&str],
    record_number: usize,
) -> Result<Option<bool>, Refusal> {
    for field in fields {
        if let Some(found) = nested_value(value, field) {
            let Some(flag) = found.as_bool() else {
                return Err(calibrate_em_refusal(
                    "Entity calibrate EM boolean field must be true or false",
                    json!({
                        "stage": "calibrate_em",
                        "record_number": record_number,
                        "field": field,
                        "value": found,
                        "writes_performed": false
                    }),
                ));
            };
            return Ok(Some(flag));
        }
    }
    Ok(None)
}

fn optional_u64_field(
    value: &Value,
    fields: &[&str],
    record_number: usize,
) -> Result<Option<u64>, Refusal> {
    for field in fields {
        if let Some(found) = nested_value(value, field) {
            let parsed = if let Some(unsigned) = found.as_u64() {
                unsigned
            } else if let Some(signed) = found.as_i64() {
                if signed < 0 {
                    return Err(invalid_unsigned_refusal(found, field, record_number));
                }
                signed as u64
            } else if let Some(text) = found.as_str() {
                text.parse::<u64>()
                    .map_err(|_| invalid_unsigned_refusal(found, field, record_number))?
            } else {
                return Err(invalid_unsigned_refusal(found, field, record_number));
            };
            return Ok(Some(parsed));
        }
    }
    Ok(None)
}

fn invalid_unsigned_refusal(value: &Value, field: &str, record_number: usize) -> Refusal {
    calibrate_em_refusal(
        "Entity calibrate EM score field must be an unsigned integer",
        json!({
            "stage": "calibrate_em",
            "record_number": record_number,
            "field": field,
            "value": value,
            "writes_performed": false
        }),
    )
}

fn nested_value<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn operator_from_source_id(value: &str) -> String {
    value.trim().to_string()
}

fn single_quoted_yaml_key(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn calibrate_em_refusal(message: impl Into<String>, detail: Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(message, detail, Some(NEXT_COMMAND.to_string()))
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
}
