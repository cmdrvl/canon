#![forbid(unsafe_code)]

use canon::entity::telemetry::{
    EntityBenchmarkTelemetry, forbidden_telemetry_payload_keys, required_telemetry_fields,
    required_telemetry_stage_ids,
};
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path};

const TELEMETRY_FIXTURE: &str =
    include_str!("../fixtures/entity/perf/telemetry/small_ci_telemetry.json");
const EVAL_TARGETS: &str =
    include_str!("../fixtures/entity/evals/entity_eval_performance_targets.json");

#[test]
fn entity_performance_metadata_fixture_matches_shared_required_fields() {
    let telemetry_value = telemetry_fixture_value();
    let telemetry: EntityBenchmarkTelemetry =
        serde_json::from_value(telemetry_value.clone()).expect("telemetry fixture parses");
    telemetry.validate().expect("telemetry fixture validates");

    let contract: Value = serde_json::from_str(EVAL_TARGETS).expect("eval target fixture parses");
    let contract_fields = contract["telemetry_required_fields"]
        .as_array()
        .expect("required fields array")
        .iter()
        .map(|field| field.as_str().expect("field string"))
        .collect::<BTreeSet<_>>();
    let module_fields = required_telemetry_fields()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(module_fields, contract_fields);

    let object = telemetry_value.as_object().expect("telemetry object");
    for field in required_telemetry_fields() {
        assert!(object.contains_key(*field), "missing top-level {field}");
    }
    for stage in required_telemetry_stage_ids() {
        assert!(
            telemetry.artifact_bytes_by_stage.contains_key(*stage),
            "missing artifact bytes for {stage}"
        );
        assert!(
            telemetry.timings_ms_by_stage.contains_key(*stage),
            "missing timing for {stage}"
        );
    }
}

#[test]
fn missing_telemetry_fields_fail_schema_before_gate_evaluation() {
    let mut value = telemetry_fixture_value();
    value
        .as_object_mut()
        .expect("telemetry object")
        .remove("candidate_pair_count");

    let error = serde_json::from_value::<EntityBenchmarkTelemetry>(value)
        .expect_err("missing required field must fail deserialization");
    assert!(
        error.to_string().contains("candidate_pair_count"),
        "{error}"
    );
}

#[test]
fn missing_stage_telemetry_fails_validation() {
    let mut telemetry: EntityBenchmarkTelemetry =
        serde_json::from_str(TELEMETRY_FIXTURE).expect("telemetry fixture parses");
    telemetry.timings_ms_by_stage.remove("block");

    let error = telemetry
        .validate()
        .expect_err("missing block timing must fail");
    assert_eq!(error.field, "timings_ms_by_stage.block");
}

#[test]
fn telemetry_schema_rejects_raw_payload_keys() {
    let mut value = telemetry_fixture_value();
    value
        .as_object_mut()
        .expect("telemetry object")
        .insert("raw_rows".to_string(), Value::Array(Vec::new()));

    let error = serde_json::from_value::<EntityBenchmarkTelemetry>(value)
        .expect_err("unknown raw payload key must fail");
    assert!(error.to_string().contains("raw_rows"), "{error}");

    let fixture_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/entity/perf/telemetry/small_ci_telemetry.json"),
    )
    .expect("fixture text");
    for forbidden in forbidden_telemetry_payload_keys() {
        assert!(
            !fixture_text.contains(forbidden),
            "telemetry fixture must not include {forbidden}"
        );
    }
}

fn telemetry_fixture_value() -> Value {
    serde_json::from_str(TELEMETRY_FIXTURE).expect("telemetry fixture json")
}
