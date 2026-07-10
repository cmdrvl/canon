use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Debug, Deserialize)]
struct ContractCases {
    schema_version: String,
    contract_schema_path: String,
    cases: Vec<QualityCase>,
}

#[derive(Debug, Deserialize)]
struct QualityCase {
    id: String,
    description: String,
    events: Vec<QualityEvent>,
    expected: ExpectedCase,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct QualityEvent {
    physical_id: String,
    logical_id: String,
    stratum: String,
    expected_class: String,
    resolution: String,
    top_50_hit: Option<bool>,
    true_pair_rank: Option<u64>,
    severity: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExpectedCase {
    physical_event_count: u64,
    unique_logical_event_count: u64,
    discovery_case_count: u64,
    exact_replay_case_count: u64,
    candidate_recall_at_50: ExpectedMetric,
    auto_link_precision: ExpectedMetric,
    auto_link_recall: ExpectedMetric,
    exact_replay_coverage: ExpectedMetric,
    accounted_case_rate: ExpectedMetric,
    critical_false_merges: u64,
    stage_miss_counts: ExpectedStageMisses,
    gate_status: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ExpectedMetric {
    numerator: u64,
    denominator: u64,
    value: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ExpectedStageMisses {
    candidate_generation: u64,
    evidence_scoring: u64,
    solver: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct StageMissCounts {
    candidate_generation: u64,
    evidence_scoring: u64,
    solver: u64,
}

#[derive(Debug)]
struct MetricSummary {
    numerator: u64,
    denominator: u64,
    value: Option<f64>,
    ci95: Option<(f64, f64)>,
}

#[derive(Debug)]
struct CaseSummary {
    physical_event_count: u64,
    unique_logical_event_count: u64,
    discovery_case_count: u64,
    exact_replay_case_count: u64,
    candidate_recall_at_50: MetricSummary,
    auto_link_precision: MetricSummary,
    auto_link_recall: MetricSummary,
    exact_replay_coverage: MetricSummary,
    accounted_case_rate: MetricSummary,
    critical_false_merges: u64,
    stage_miss_counts: StageMissCounts,
    gate_status: BTreeMap<String, String>,
}

fn schema() -> Value {
    serde_json::from_str(include_str!(
        "../schemas/canon.entity.quality.v1.schema.json"
    ))
    .expect("quality schema parses")
}

fn cases_fixture() -> ContractCases {
    serde_json::from_str(include_str!(
        "fixtures/canon_v1/quality/contract_cases.json"
    ))
    .expect("quality case fixture parses")
}

fn doc() -> String {
    fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/ENTITY_EVALS_AND_PERFORMANCE.md"),
    )
    .expect("quality doc opens")
}

fn evaluate_case(case: &QualityCase) -> CaseSummary {
    let mut unique = BTreeMap::<String, QualityEvent>::new();
    for event in &case.events {
        match unique.get(&event.logical_id) {
            Some(previous) => {
                assert_eq!(
                    semantic_event(previous),
                    semantic_event(event),
                    "duplicate logical event {} in {} changed semantics",
                    event.logical_id,
                    case.id
                );
            }
            None => {
                unique.insert(event.logical_id.clone(), event.clone());
            }
        }
    }

    let unique_events = unique.into_values().collect::<Vec<_>>();
    let discovery_events = unique_events
        .iter()
        .filter(|event| event.stratum != "exact_known_replay")
        .collect::<Vec<_>>();
    let must_link_events = unique_events
        .iter()
        .filter(|event| event.expected_class == "must_link")
        .collect::<Vec<_>>();
    let exact_replay_events = unique_events
        .iter()
        .filter(|event| event.expected_class == "exact_replay")
        .collect::<Vec<_>>();

    let candidate_recall_hits = must_link_events
        .iter()
        .filter(|event| event.top_50_hit == Some(true))
        .count() as u64;
    let auto_link_true_positives = must_link_events
        .iter()
        .filter(|event| event.resolution == "correct_auto_link")
        .count() as u64;
    let predicted_auto_links = unique_events
        .iter()
        .filter(|event| {
            event.resolution == "correct_auto_link" || event.resolution == "false_merge"
        })
        .count() as u64;
    let exact_replay_correct = exact_replay_events
        .iter()
        .filter(|event| event.resolution == "correct_exact_replay")
        .count() as u64;
    let critical_false_merges = unique_events
        .iter()
        .filter(|event| {
            event.resolution == "false_merge" && event.severity.as_deref() == Some("critical")
        })
        .count() as u64;

    let mut stage_miss_counts = StageMissCounts {
        candidate_generation: 0,
        evidence_scoring: 0,
        solver: 0,
    };
    for event in &must_link_events {
        match event.resolution.as_str() {
            "candidate_generation_miss" => stage_miss_counts.candidate_generation += 1,
            "evidence_scoring_miss" => stage_miss_counts.evidence_scoring += 1,
            "solver_miss" => stage_miss_counts.solver += 1,
            _ => {}
        }
    }

    let candidate_recall_at_50 =
        proportion_metric(candidate_recall_hits, must_link_events.len() as u64);
    let auto_link_precision = proportion_metric(auto_link_true_positives, predicted_auto_links);
    let auto_link_recall =
        proportion_metric(auto_link_true_positives, must_link_events.len() as u64);
    let exact_replay_coverage =
        proportion_metric(exact_replay_correct, exact_replay_events.len() as u64);
    let accounted_case_rate =
        proportion_metric(discovery_events.len() as u64, discovery_events.len() as u64);
    let gate_status = evaluate_gates(
        &schema()["x-canon-contract"]["gates"],
        critical_false_merges,
        &candidate_recall_at_50,
        &auto_link_precision,
        &auto_link_recall,
        &accounted_case_rate,
    );

    CaseSummary {
        physical_event_count: case.events.len() as u64,
        unique_logical_event_count: unique_events.len() as u64,
        discovery_case_count: discovery_events.len() as u64,
        exact_replay_case_count: exact_replay_events.len() as u64,
        candidate_recall_at_50,
        auto_link_precision,
        auto_link_recall,
        exact_replay_coverage,
        accounted_case_rate,
        critical_false_merges,
        stage_miss_counts,
        gate_status,
    }
}

fn evaluate_gates(
    gates: &Value,
    critical_false_merges: u64,
    candidate_recall_at_50: &MetricSummary,
    auto_link_precision: &MetricSummary,
    auto_link_recall: &MetricSummary,
    accounted_case_rate: &MetricSummary,
) -> BTreeMap<String, String> {
    let mut results = BTreeMap::new();
    for gate in gates.as_array().expect("gate metadata array") {
        let gate_id = gate["gate_id"].as_str().expect("gate id");
        let metric_id = gate["metric_id"].as_str().expect("metric id");
        let status = match metric_id {
            "candidate_recall_at_50" => threshold_status(
                candidate_recall_at_50,
                gate["operator"].as_str().unwrap(),
                gate["threshold"].as_f64(),
            ),
            "auto_link_precision" => threshold_status(
                auto_link_precision,
                gate["operator"].as_str().unwrap(),
                gate["threshold"].as_f64(),
            ),
            "auto_link_recall" => threshold_status(
                auto_link_recall,
                gate["operator"].as_str().unwrap(),
                gate["threshold"].as_f64(),
            ),
            "accounted_case_rate" => threshold_status(
                accounted_case_rate,
                gate["operator"].as_str().unwrap(),
                gate["threshold"].as_f64(),
            ),
            "hard_negative_false_merges" => {
                if critical_false_merges == gate["threshold"].as_u64().expect("count threshold") {
                    "pass".to_string()
                } else {
                    "fail".to_string()
                }
            }
            other => panic!("unexpected gate metric {other}"),
        };
        results.insert(gate_id.to_string(), status);
    }
    results
}

fn threshold_status(metric: &MetricSummary, operator: &str, threshold: Option<f64>) -> String {
    match (metric.value, threshold) {
        (None, _) => "not_applicable".to_string(),
        (Some(value), Some(threshold)) => {
            let passes = match operator {
                ">=" => value >= threshold,
                "==" => (value - threshold).abs() <= f64::EPSILON,
                "<=" => value <= threshold,
                other => panic!("unexpected operator {other}"),
            };
            if passes {
                "pass".to_string()
            } else {
                "fail".to_string()
            }
        }
        (Some(_), None) => "not_applicable".to_string(),
    }
}

fn proportion_metric(numerator: u64, denominator: u64) -> MetricSummary {
    let value = if denominator == 0 {
        None
    } else {
        Some(numerator as f64 / denominator as f64)
    };
    let ci95 = if denominator == 0 {
        None
    } else {
        Some(wilson_interval_95(numerator, denominator))
    };
    MetricSummary {
        numerator,
        denominator,
        value,
        ci95,
    }
}

fn wilson_interval_95(numerator: u64, denominator: u64) -> (f64, f64) {
    let z = 1.959_963_984_540_054_f64;
    let n = denominator as f64;
    let p = numerator as f64 / n;
    let z2 = z * z;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let margin = (z / denom) * ((p * (1.0 - p) / n) + (z2 / (4.0 * n * n))).sqrt();
    (center - margin, center + margin)
}

fn semantic_event(
    event: &QualityEvent,
) -> (
    &str,
    &str,
    &str,
    &str,
    Option<bool>,
    Option<u64>,
    Option<&str>,
) {
    (
        event.stratum.as_str(),
        event.expected_class.as_str(),
        event.resolution.as_str(),
        event.logical_id.as_str(),
        event.top_50_hit,
        event.true_pair_rank,
        event.severity.as_deref(),
    )
}

fn metric_ids(schema: &Value) -> BTreeSet<String> {
    schema["x-canon-contract"]["metric_contract"]
        .as_array()
        .expect("metric contract array")
        .iter()
        .map(|metric| metric["id"].as_str().expect("metric id").to_string())
        .collect()
}

fn stratum_ids(schema: &Value) -> BTreeSet<String> {
    schema["x-canon-contract"]["strata"]
        .as_array()
        .expect("strata metadata array")
        .iter()
        .map(|stratum| stratum["id"].as_str().expect("stratum id").to_string())
        .collect()
}

fn gate_threshold(schema: &Value, gate_id: &str) -> Value {
    schema["x-canon-contract"]["gates"]
        .as_array()
        .expect("gate metadata array")
        .iter()
        .find(|gate| gate["gate_id"] == gate_id)
        .unwrap_or_else(|| panic!("missing gate {gate_id}"))
        .clone()
}

fn metric_instance(
    numerator: Option<u64>,
    denominator: u64,
    denominator_behavior: &str,
    sample_count: u64,
    value: Option<f64>,
    confidence_interval_95: Option<(f64, f64)>,
) -> Value {
    let ci = confidence_interval_95
        .map(|(low, high)| json!([low, high]))
        .unwrap_or(Value::Null);
    json!({
        "confidence_interval_95": ci,
        "denominator": denominator,
        "denominator_behavior": denominator_behavior,
        "numerator": numerator,
        "sample_count": sample_count,
        "value": value,
    })
}

fn gate_result_instance(
    metric_id: &str,
    observed_value: Option<f64>,
    operator: &str,
    status: &str,
    threshold: Option<f64>,
) -> Value {
    json!({
        "metric_id": metric_id,
        "observed_value": observed_value,
        "operator": operator,
        "status": status,
        "threshold": threshold,
        "waiver_bead_id": Value::Null,
    })
}

fn resource_metric_instance(sample_count: u64, unit: &str, value: Option<f64>) -> Value {
    json!({
        "sample_count": sample_count,
        "unit": unit,
        "value": value,
    })
}

fn minimal_quality_report() -> Value {
    json!({
        "schema_version": "canon.entity.quality.v1",
        "doc": "docs/ENTITY_EVALS_AND_PERFORMANCE.md",
        "gates": {
            "candidate_recall_at_50_min": gate_result_instance(
                "candidate_recall_at_50",
                Some(1.0),
                ">=",
                "pass",
                Some(0.995),
            ),
            "auto_link_precision_min": gate_result_instance(
                "auto_link_precision",
                Some(1.0),
                ">=",
                "pass",
                Some(0.995),
            ),
            "auto_link_recall_min": gate_result_instance(
                "auto_link_recall",
                Some(1.0),
                ">=",
                "pass",
                Some(0.98),
            ),
            "critical_false_merges_max": gate_result_instance(
                "hard_negative_false_merges",
                Some(0.0),
                "==",
                "pass",
                Some(0.0),
            ),
            "accounted_case_rate_min": gate_result_instance(
                "accounted_case_rate",
                Some(1.0),
                "==",
                "pass",
                Some(1.0),
            ),
        },
        "metrics": {
            "abstention_precision": metric_instance(
                Some(0),
                0,
                "non_exact_discovery_only",
                0,
                None,
                None,
            ),
            "accounted_case_rate": metric_instance(
                Some(2),
                2,
                "all_non_exact_labeled_cases",
                2,
                Some(1.0),
                Some((0.0, 1.0)),
            ),
            "auto_link_precision": metric_instance(
                Some(1),
                1,
                "non_exact_discovery_only",
                1,
                Some(1.0),
                Some((0.0, 1.0)),
            ),
            "auto_link_recall": metric_instance(
                Some(1),
                1,
                "non_exact_discovery_only",
                1,
                Some(1.0),
                Some((0.0, 1.0)),
            ),
            "b_cubed_f1": metric_instance(
                Some(1),
                1,
                "non_exact_discovery_only",
                1,
                Some(1.0),
                Some((0.0, 1.0)),
            ),
            "b_cubed_precision": metric_instance(
                Some(1),
                1,
                "non_exact_discovery_only",
                1,
                Some(1.0),
                Some((0.0, 1.0)),
            ),
            "b_cubed_recall": metric_instance(
                Some(1),
                1,
                "non_exact_discovery_only",
                1,
                Some(1.0),
                Some((0.0, 1.0)),
            ),
            "candidate_recall_at_50": metric_instance(
                Some(1),
                1,
                "non_exact_discovery_only",
                1,
                Some(1.0),
                Some((0.0, 1.0)),
            ),
            "exact_replay_coverage": metric_instance(
                Some(0),
                0,
                "exact_replay_only",
                0,
                None,
                None,
            ),
            "hard_negative_false_merges": metric_instance(
                Some(0),
                1,
                "distinct_or_hierarchy_only",
                1,
                Some(0.0),
                None,
            ),
            "pairwise_f1": metric_instance(
                Some(1),
                1,
                "non_exact_discovery_only",
                1,
                Some(1.0),
                Some((0.0, 1.0)),
            ),
            "pairwise_precision": metric_instance(
                Some(1),
                1,
                "non_exact_discovery_only",
                1,
                Some(1.0),
                Some((0.0, 1.0)),
            ),
            "pairwise_recall": metric_instance(
                Some(1),
                1,
                "non_exact_discovery_only",
                1,
                Some(1.0),
                Some((0.0, 1.0)),
            ),
            "review_coverage": metric_instance(
                Some(0),
                0,
                "non_exact_discovery_only",
                0,
                None,
                None,
            ),
            "review_yield": metric_instance(
                Some(0),
                0,
                "non_exact_discovery_only",
                0,
                None,
                None,
            ),
            "true_pair_rank": metric_instance(
                None,
                1,
                "non_exact_discovery_only",
                1,
                Some(1.0),
                None,
            ),
        },
        "resources": {
            "candidate_pairs_per_surface_p95": resource_metric_instance(1, "count", Some(1.0)),
            "candidate_pairs_per_surface_p99": resource_metric_instance(1, "count", Some(1.0)),
            "peak_memory_bytes": resource_metric_instance(1, "bytes", Some(1024.0)),
            "wall_clock_seconds": resource_metric_instance(1, "seconds", Some(0.1)),
        },
        "severity_counts": {
            "critical": 0,
            "high": 0,
            "low": 0,
            "medium": 0,
        },
        "stage_miss_counts": {
            "candidate_generation": 0,
            "evidence_scoring": 0,
            "solver": 0,
        },
        "stratum_counts": {
            "directional_cross_source": 0,
            "exact_known_replay": 0,
            "genuinely_unresolved": 0,
            "novel_multi_observation": 0,
            "related_or_hierarchy_distinct": 1,
            "withheld_alias_incumbent": 1,
        },
        "waivers": [],
    })
}

fn validate_quality_report_instance(instance: &Value) -> Result<(), Vec<String>> {
    let root = schema();
    let mut errors = Vec::new();
    validate_schema_node(&root, &root, instance, "$", &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_schema_node(
    root: &Value,
    schema: &Value,
    instance: &Value,
    path: &str,
    errors: &mut Vec<String>,
) {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let resolved = resolve_local_ref(root, reference);
        validate_schema_node(root, resolved, instance, path, errors);
    }

    if let Some(options) = schema.get("oneOf").and_then(Value::as_array) {
        let match_count = options
            .iter()
            .filter(|option| {
                let mut option_errors = Vec::new();
                validate_schema_node(root, option, instance, path, &mut option_errors);
                option_errors.is_empty()
            })
            .count();
        if match_count != 1 {
            errors.push(format!(
                "{path}: expected exactly one matching schema branch, found {match_count}"
            ));
        }
    }

    if let Some(constant) = schema.get("const")
        && instance != constant
    {
        errors.push(format!(
            "{path}: expected const {constant}, found {instance}"
        ));
    }

    if let Some(allowed_values) = schema.get("enum").and_then(Value::as_array)
        && !allowed_values.iter().any(|allowed| allowed == instance)
    {
        errors.push(format!(
            "{path}: expected one of {allowed_values:?}, found {instance}"
        ));
    }

    if let Some(expected_type) = schema.get("type").and_then(Value::as_str)
        && !matches_schema_type(instance, expected_type)
    {
        errors.push(format!(
            "{path}: expected type {expected_type}, found {instance}"
        ));
        return;
    }

    if let Some(minimum) = schema.get("minimum").and_then(Value::as_f64)
        && let Some(value) = instance.as_f64()
        && value < minimum
    {
        errors.push(format!("{path}: value {value} is below minimum {minimum}"));
    }

    if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64)
        && let Some(value) = instance.as_f64()
        && value > maximum
    {
        errors.push(format!("{path}: value {value} exceeds maximum {maximum}"));
    }

    if let Some(required) = schema.get("required").and_then(Value::as_array)
        && let Some(object) = instance.as_object()
    {
        for field in required {
            let field = field.as_str().expect("required field name");
            if !object.contains_key(field) {
                errors.push(format!("{path}: missing required property {field}"));
            }
        }
    }

    if let Some(properties) = schema.get("properties").and_then(Value::as_object)
        && let Some(object) = instance.as_object()
    {
        let additional_properties_forbidden =
            schema.get("additionalProperties") == Some(&Value::Bool(false));
        for (key, value) in object {
            if let Some(property_schema) = properties.get(key) {
                validate_schema_node(root, property_schema, value, &path_join(path, key), errors);
            } else if additional_properties_forbidden {
                errors.push(format!("{path}: unexpected property {key}"));
            }
        }
    }

    if let Some(min_items) = schema.get("minItems").and_then(Value::as_u64)
        && let Some(array) = instance.as_array()
        && (array.len() as u64) < min_items
    {
        errors.push(format!(
            "{path}: expected at least {min_items} items, found {}",
            array.len()
        ));
    }

    if let Some(max_items) = schema.get("maxItems").and_then(Value::as_u64)
        && let Some(array) = instance.as_array()
        && (array.len() as u64) > max_items
    {
        errors.push(format!(
            "{path}: expected at most {max_items} items, found {}",
            array.len()
        ));
    }

    if let Some(items) = schema.get("items")
        && let Some(array) = instance.as_array()
    {
        match items {
            Value::Array(item_schemas) => {
                for (index, item) in array.iter().enumerate().take(item_schemas.len()) {
                    validate_schema_node(
                        root,
                        &item_schemas[index],
                        item,
                        &format!("{path}[{index}]"),
                        errors,
                    );
                }
                if schema.get("additionalItems") == Some(&Value::Bool(false))
                    && array.len() > item_schemas.len()
                {
                    errors.push(format!(
                        "{path}: expected at most {} tuple items, found {}",
                        item_schemas.len(),
                        array.len()
                    ));
                }
            }
            Value::Object(_) => {
                for (index, item) in array.iter().enumerate() {
                    validate_schema_node(root, items, item, &format!("{path}[{index}]"), errors);
                }
            }
            _ => {}
        }
    }
}

fn resolve_local_ref<'a>(root: &'a Value, reference: &str) -> &'a Value {
    let pointer = reference
        .strip_prefix('#')
        .unwrap_or_else(|| panic!("expected local ref, got {reference}"));
    root.pointer(pointer)
        .unwrap_or_else(|| panic!("missing ref target {reference}"))
}

fn matches_schema_type(instance: &Value, expected_type: &str) -> bool {
    match expected_type {
        "array" => instance.is_array(),
        "integer" => {
            instance.as_i64().is_some()
                || instance.as_u64().is_some()
                || instance
                    .as_f64()
                    .is_some_and(|value| (value.fract()).abs() <= f64::EPSILON)
        }
        "null" => instance.is_null(),
        "number" => instance.as_f64().is_some(),
        "object" => instance.is_object(),
        "string" => instance.as_str().is_some(),
        other => panic!("unsupported schema type {other}"),
    }
}

fn path_join(path: &str, segment: &str) -> String {
    format!("{path}.{segment}")
}

#[test]
fn quality_schema_declares_required_contract_components() {
    let schema = schema();
    assert_eq!(schema["title"], "canon.entity.quality.v1");
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        "canon.entity.quality.v1"
    );
    assert_eq!(
        schema["properties"]["doc"]["const"],
        "docs/ENTITY_EVALS_AND_PERFORMANCE.md"
    );

    let strata = stratum_ids(&schema);
    assert_eq!(
        strata,
        BTreeSet::from([
            "directional_cross_source".to_string(),
            "exact_known_replay".to_string(),
            "genuinely_unresolved".to_string(),
            "novel_multi_observation".to_string(),
            "related_or_hierarchy_distinct".to_string(),
            "withheld_alias_incumbent".to_string(),
        ])
    );

    let discovery_ineligible = schema["x-canon-contract"]["strata"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|stratum| stratum["discovery_score_eligible"] == Value::Bool(false))
        .map(|stratum| stratum["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(discovery_ineligible, vec!["exact_known_replay"]);

    let metrics = metric_ids(&schema);
    for required in [
        "candidate_recall_at_50",
        "true_pair_rank",
        "auto_link_precision",
        "auto_link_recall",
        "pairwise_precision",
        "pairwise_recall",
        "pairwise_f1",
        "b_cubed_precision",
        "b_cubed_recall",
        "b_cubed_f1",
        "hard_negative_false_merges",
        "abstention_precision",
        "review_coverage",
        "review_yield",
        "exact_replay_coverage",
        "accounted_case_rate",
    ] {
        assert!(metrics.contains(required), "missing metric {required}");
    }

    let candidate_recall = schema["x-canon-contract"]["metric_contract"]
        .as_array()
        .unwrap()
        .iter()
        .find(|metric| metric["id"] == "candidate_recall_at_50")
        .expect("candidate recall contract");
    assert_eq!(
        candidate_recall["excluded_strata"],
        serde_json::json!(["exact_known_replay"])
    );

    let stage_miss_ids = schema["x-canon-contract"]["stage_miss_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        stage_miss_ids,
        BTreeSet::from(["candidate_generation", "evidence_scoring", "solver"])
    );

    assert_eq!(
        gate_threshold(&schema, "candidate_recall_at_50_min")["threshold"],
        0.995
    );
    assert_eq!(
        gate_threshold(&schema, "auto_link_precision_min")["threshold"],
        0.995
    );
    assert_eq!(
        gate_threshold(&schema, "auto_link_recall_min")["threshold"],
        0.98
    );
    assert_eq!(
        gate_threshold(&schema, "critical_false_merges_max")["threshold"],
        0
    );
    assert_eq!(
        gate_threshold(&schema, "critical_false_merges_max")["waivable"],
        false
    );
    assert_eq!(
        gate_threshold(&schema, "critical_false_merges_max")["observed_value_pointer"],
        "/severity_counts/critical"
    );

    let hard_negative_metric = schema["x-canon-contract"]["metric_contract"]
        .as_array()
        .unwrap()
        .iter()
        .find(|metric| metric["id"] == "hard_negative_false_merges")
        .expect("hard-negative metric contract");
    assert_eq!(
        hard_negative_metric["count_value_pointer"],
        "/metrics/hard_negative_false_merges/value"
    );
    assert_eq!(
        hard_negative_metric["severity_breakout_pointer"],
        "/severity_counts"
    );

    let waiver_policy = &schema["x-canon-contract"]["waiver_policy"];
    assert_eq!(
        waiver_policy["threshold_lowering_requires_new_holdout_or_explicit_waiver"],
        true
    );
    assert_eq!(
        waiver_policy["exact_replay_rows_cannot_justify_threshold_lowering"],
        true
    );
    let required_waiver_fields = waiver_policy["required_fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| field.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    for required in [
        "waiver_bead_id",
        "holdout_id",
        "metric_id",
        "gate_id",
        "old_threshold",
        "new_threshold",
        "reason",
        "approved_by",
        "approved_at",
        "scope",
        "replacement_holdout_id",
        "expires_at",
    ] {
        assert!(
            required_waiver_fields.contains(required),
            "missing waiver field {required}"
        );
    }

    let serialized = serde_json::to_string(&schema).expect("schema serializes");
    for forbidden in ["cmbs", "regab", "sec10d", "tenant", "loan", "servicer"] {
        assert!(
            !serialized.to_ascii_lowercase().contains(forbidden),
            "schema should stay domain-neutral and omit {forbidden}"
        );
    }
}

#[test]
fn quality_doc_cross_references_schema_and_rules() {
    let text = doc();
    for required in [
        "canon.entity.quality.v1",
        "schemas/canon.entity.quality.v1.schema.json",
        "Exact replay is a floor, not a discovery score.",
        "candidate_generation",
        "evidence_scoring",
        "solver",
        "metrics.hard_negative_false_merges.value",
        "severity_counts.critical",
        "Lowering a threshold requires a new versioned holdout or an explicit waiver bead.",
        "The core contract does not waive `hard_negative_false_merges.critical == 0`.",
    ] {
        assert!(text.contains(required), "doc omits {required}");
    }
}

#[test]
fn minimal_quality_report_instance_validates_against_schema() {
    if let Err(errors) = validate_quality_report_instance(&minimal_quality_report()) {
        panic!("expected minimal quality report to validate, errors: {errors:#?}");
    }
}

#[test]
fn minimal_quality_report_rejects_missing_hard_negative_metric() {
    let mut invalid = minimal_quality_report();
    invalid["metrics"]
        .as_object_mut()
        .expect("metrics object")
        .remove("hard_negative_false_merges");

    let errors =
        validate_quality_report_instance(&invalid).expect_err("schema should reject bad report");
    assert!(
        errors.iter().any(|error| {
            error.contains("$.metrics") && error.contains("hard_negative_false_merges")
        }),
        "expected missing hard-negative metric error, got {errors:#?}"
    );
}

#[test]
fn quality_case_fixture_covers_required_scenarios() {
    let cases = cases_fixture();
    assert_eq!(
        cases.schema_version,
        "canon.entity.quality.contract_cases.v1"
    );
    assert_eq!(
        cases.contract_schema_path,
        "schemas/canon.entity.quality.v1.schema.json"
    );

    let ids = cases
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        ids,
        BTreeSet::from([
            "duplicate_row",
            "empty_stratum",
            "false_merge",
            "missed_candidate",
            "over_abstain",
            "planted_perfect",
        ])
    );
}

#[test]
fn quality_contract_cases_evaluate_as_expected() {
    for case in cases_fixture().cases {
        assert!(
            !case.description.trim().is_empty(),
            "{} should describe the scenario",
            case.id
        );
        let actual = evaluate_case(&case);
        assert_eq!(
            actual.physical_event_count,
            case.expected.physical_event_count
        );
        assert_eq!(
            actual.unique_logical_event_count,
            case.expected.unique_logical_event_count
        );
        assert_eq!(
            actual.discovery_case_count,
            case.expected.discovery_case_count
        );
        assert_eq!(
            actual.exact_replay_case_count,
            case.expected.exact_replay_case_count
        );
        assert_metric(
            &actual.candidate_recall_at_50,
            &case.expected.candidate_recall_at_50,
        );
        assert_metric(
            &actual.auto_link_precision,
            &case.expected.auto_link_precision,
        );
        assert_metric(&actual.auto_link_recall, &case.expected.auto_link_recall);
        assert_metric(
            &actual.exact_replay_coverage,
            &case.expected.exact_replay_coverage,
        );
        assert_metric(
            &actual.accounted_case_rate,
            &case.expected.accounted_case_rate,
        );
        assert_eq!(
            actual.critical_false_merges,
            case.expected.critical_false_merges
        );
        assert_eq!(
            actual.stage_miss_counts,
            StageMissCounts {
                candidate_generation: case.expected.stage_miss_counts.candidate_generation,
                evidence_scoring: case.expected.stage_miss_counts.evidence_scoring,
                solver: case.expected.stage_miss_counts.solver,
            }
        );
        assert_eq!(actual.gate_status, case.expected.gate_status);
    }
}

#[test]
fn quality_contract_reports_confidence_intervals_and_zero_denominators() {
    let by_id = cases_fixture()
        .cases
        .into_iter()
        .map(|case| (case.id.clone(), evaluate_case(&case)))
        .collect::<BTreeMap<_, _>>();

    for case_id in [
        "planted_perfect",
        "missed_candidate",
        "false_merge",
        "over_abstain",
        "duplicate_row",
    ] {
        let summary = by_id.get(case_id).expect("case summary");
        for metric in [
            &summary.candidate_recall_at_50,
            &summary.auto_link_precision,
            &summary.auto_link_recall,
            &summary.exact_replay_coverage,
            &summary.accounted_case_rate,
        ] {
            assert!(metric.ci95.is_some(), "{case_id} should report CI95");
            let (low, high) = metric.ci95.expect("ci");
            let value = metric.value.expect("value");
            assert!(
                low <= value && value <= high,
                "{case_id} CI should contain value"
            );
        }
    }

    let empty = by_id.get("empty_stratum").expect("empty-stratum summary");
    assert!(empty.candidate_recall_at_50.ci95.is_none());
    assert!(empty.auto_link_precision.ci95.is_none());
    assert!(empty.auto_link_recall.ci95.is_none());
    assert_eq!(
        empty.gate_status["candidate_recall_at_50_min"],
        "not_applicable"
    );
    assert_eq!(
        empty.gate_status["auto_link_precision_min"],
        "not_applicable"
    );
    assert_eq!(empty.gate_status["auto_link_recall_min"], "not_applicable");
}

#[test]
fn repeating_exact_replay_rows_does_not_change_discovery_scores() {
    let cases = cases_fixture()
        .cases
        .into_iter()
        .map(|case| (case.id.clone(), evaluate_case(&case)))
        .collect::<BTreeMap<_, _>>();
    let planted = cases.get("planted_perfect").expect("planted perfect");
    let duplicate = cases.get("duplicate_row").expect("duplicate row");

    assert_eq!(planted.physical_event_count, 6);
    assert_eq!(duplicate.physical_event_count, 8);
    assert_eq!(
        planted.exact_replay_case_count, duplicate.exact_replay_case_count,
        "logical replay case count should stay deduped"
    );
    assert_eq!(planted.discovery_case_count, duplicate.discovery_case_count);
    assert_metric(
        &planted.candidate_recall_at_50,
        &ExpectedMetric {
            numerator: duplicate.candidate_recall_at_50.numerator,
            denominator: duplicate.candidate_recall_at_50.denominator,
            value: duplicate.candidate_recall_at_50.value,
        },
    );
    assert_metric(
        &planted.auto_link_precision,
        &ExpectedMetric {
            numerator: duplicate.auto_link_precision.numerator,
            denominator: duplicate.auto_link_precision.denominator,
            value: duplicate.auto_link_precision.value,
        },
    );
    assert_metric(
        &planted.auto_link_recall,
        &ExpectedMetric {
            numerator: duplicate.auto_link_recall.numerator,
            denominator: duplicate.auto_link_recall.denominator,
            value: duplicate.auto_link_recall.value,
        },
    );
    assert_eq!(planted.gate_status, duplicate.gate_status);
}

#[test]
fn stage_miss_attribution_distinguishes_candidate_evidence_and_solver_failures() {
    let cases = cases_fixture()
        .cases
        .into_iter()
        .map(|case| (case.id.clone(), evaluate_case(&case)))
        .collect::<BTreeMap<_, _>>();

    let missed = cases
        .get("missed_candidate")
        .expect("missed-candidate summary");
    assert_eq!(missed.stage_miss_counts.candidate_generation, 1);
    assert_eq!(missed.stage_miss_counts.evidence_scoring, 1);
    assert_eq!(missed.stage_miss_counts.solver, 0);

    let abstain = cases.get("over_abstain").expect("over-abstain summary");
    assert_eq!(abstain.stage_miss_counts.candidate_generation, 0);
    assert_eq!(abstain.stage_miss_counts.evidence_scoring, 0);
    assert_eq!(abstain.stage_miss_counts.solver, 1);
}

fn assert_metric(actual: &MetricSummary, expected: &ExpectedMetric) {
    assert_eq!(actual.numerator, expected.numerator);
    assert_eq!(actual.denominator, expected.denominator);
    match (actual.value, expected.value) {
        (Some(actual), Some(expected)) => {
            assert!(
                (actual - expected).abs() <= 1e-12,
                "metric value mismatch: actual={actual} expected={expected}"
            );
        }
        (None, None) => {}
        other => panic!("metric value mismatch: {other:?}"),
    }
}
