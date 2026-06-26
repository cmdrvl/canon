#![forbid(unsafe_code)]

use canon::entity::telemetry::{
    EntityTelemetryCounters, EntityTelemetryHarness, EntityTelemetryMachine,
    EntityTelemetryOutcome, EntityTelemetryRunContext, required_telemetry_stage_ids,
};
use std::time::Duration;

#[test]
fn entity_stage_telemetry_harness_records_stage_metadata_without_wall_clock_assertions() {
    let mut harness = EntityTelemetryHarness::new(run_context());
    for (index, stage) in required_telemetry_stage_ids().iter().enumerate() {
        harness
            .record_stage_ms(
                stage,
                100 + u64::try_from(index).expect("index fits u64"),
                10 + u64::try_from(index).expect("index fits u64"),
            )
            .expect("stage records");
    }

    let telemetry = harness
        .finish(counters(), outcome())
        .expect("telemetry validates");

    assert_eq!(
        telemetry.schema_version,
        "canon_entity_benchmark_telemetry.v0"
    );
    assert_eq!(telemetry.run_id, "stage-telemetry-test");
    assert_eq!(telemetry.profile, "cmbs_tenant_label");
    assert_eq!(telemetry.cache_state, "warm");
    assert!(!telemetry.canon_version.trim().is_empty());
    assert!(!telemetry.target_triple.trim().is_empty());
    assert!(!telemetry.os.trim().is_empty());
    assert!(telemetry.logical_cores > 0);
    assert!(telemetry.memory_bytes > 0);
    assert_eq!(telemetry.raw_row_count, 12);
    assert_eq!(telemetry.prepared_surface_count, 5);
    assert_eq!(telemetry.candidate_pair_count, 9);
    assert_eq!(telemetry.exact_bucket_pair_expansion_count, 0);
    for stage in required_telemetry_stage_ids() {
        assert!(
            telemetry.artifact_bytes_by_stage.contains_key(*stage),
            "missing bytes for {stage}"
        );
        assert!(
            telemetry.timings_ms_by_stage.contains_key(*stage),
            "missing timing for {stage}"
        );
    }
}

#[test]
fn entity_stage_telemetry_harness_accepts_duration_hooks() {
    let mut harness = EntityTelemetryHarness::new(run_context());
    harness
        .record_stage("prepare", 123, Duration::from_millis(7))
        .expect("duration stage records");

    let telemetry = harness
        .finish(counters(), outcome())
        .expect("zero-filled unrecorded stages still validate");

    assert_eq!(telemetry.artifact_bytes_by_stage["prepare"], 123);
    assert_eq!(telemetry.timings_ms_by_stage["prepare"], 7);
    assert_eq!(telemetry.artifact_bytes_by_stage["block"], 0);
    assert_eq!(telemetry.timings_ms_by_stage["block"], 0);
}

#[test]
fn entity_stage_telemetry_harness_rejects_unknown_stage_ids() {
    let mut harness = EntityTelemetryHarness::new(run_context());
    let error = harness
        .record_stage_ms("row_level_pairs", 1, 1)
        .expect_err("unknown stages refuse");

    assert_eq!(error.field, "stage");
    assert!(error.message.contains("row_level_pairs"));
}

fn run_context() -> EntityTelemetryRunContext {
    EntityTelemetryRunContext {
        run_id: "stage-telemetry-test".to_string(),
        suite_id: "entity-small-ci".to_string(),
        profile: "cmbs_tenant_label".to_string(),
        machine: EntityTelemetryMachine::detect_redacted(),
        cache_state: "warm".to_string(),
        input_hash: "blake3:input".to_string(),
        profile_hash: "blake3:profile".to_string(),
        strategy_hash: "blake3:strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
        patch_hash: "blake3:patch".to_string(),
        holdout_id: "none".to_string(),
        metamorphic_relation_id: "none".to_string(),
    }
}

fn counters() -> EntityTelemetryCounters {
    EntityTelemetryCounters {
        raw_row_count: 12,
        raw_observation_count: 12,
        raw_unique_surface_count: 5,
        prepared_surface_count: 5,
        exact_resolved_surface_count: 2,
        candidate_pair_count: 9,
        candidate_pairs_per_surface_p50: 2,
        candidate_pairs_per_surface_p95: 3,
        candidate_pairs_per_surface_p99: 4,
        suppressed_candidate_count: 1,
        exact_bucket_count: 1,
        exact_bucket_pair_expansion_count: 0,
        largest_exact_bucket_size: 2,
        largest_component_size: 3,
        edge_count: 7,
        review_group_count: 2,
    }
}

fn outcome() -> EntityTelemetryOutcome {
    EntityTelemetryOutcome {
        peak_memory_bytes: 64 * 1024 * 1024,
        peak_memory_method: "harness_rss_sample".to_string(),
        registry_pre_mutation_hash: "blake3:registry-before".to_string(),
        registry_post_mutation_hash: "blake3:registry-after".to_string(),
        runtime_guard_status: "passed".to_string(),
        refusal_code: "none".to_string(),
        next_command: String::new(),
    }
}
