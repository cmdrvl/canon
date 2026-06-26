#![forbid(unsafe_code)]

use canon::entity::{
    block::{
        BlockCandidateBudgetConfig, BlockCandidateBudgetDiagnostics,
        BlockCandidateBudgetObservation, ExactBucketBlockRequest, ExactBucketSurface,
        emit_exact_bucket_hyperedges, validate_block_candidate_budget_before_artifact_emission,
        validate_block_exact_bucket_size_limit,
    },
    block_artifact::{
        EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN, ExactBucketProfile, ExactBucketUpstream,
    },
    telemetry::{
        EntityTelemetryCounters, EntityTelemetryHarness, EntityTelemetryMachine,
        EntityTelemetryOutcome, EntityTelemetryRunContext,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const CMBS_500K_CONTRACT: &str =
    include_str!("../fixtures/entity/stress/generators/cmbs_500k_contract.json");

#[test]
fn cmbs_500k_fixture_shape() {
    let contract = stress_contract();
    assert_eq!(contract.schema_version, "canon.entity.stress_generator.v0");
    assert_eq!(contract.id, "cmbs_500k_stress");
    assert_eq!(contract.gate_id, "G11");
    assert_eq!(contract.seed, 424242);
    assert_eq!(contract.profile_id, "cmbs_tenant_label");
    assert_eq!(contract.topk.k, contract.topk.candidate_cap);
    assert_eq!(contract.row_count, 500_000);
    assert_eq!(contract.deal_count, 3_000);
    assert_eq!(contract.raw_unique_surface_count, 25_000);
    assert_eq!(contract.prepared_surface_count, 25_000);
    assert_eq!(contract.topk.k, 25);
    assert_eq!(
        contract.expected.generated_static_artifact_policy,
        "do_not_commit_generated_500k_rows"
    );

    let first = sample_rows(&contract, contract.ci_sample_row_count);
    let second = sample_rows(&contract, contract.ci_sample_row_count);
    assert_eq!(first, second, "same seed/config must be byte-identical");
    assert_eq!(
        row_signatures(&first[..3]),
        contract.expected.first_row_signatures
    );

    let mut changed_seed = contract.clone();
    changed_seed.seed = changed_seed.seed.saturating_add(1);
    assert_ne!(
        sample_rows(&changed_seed, 3),
        first[..3],
        "seed must participate in generated row ordering"
    );

    assert_eq!(
        generated_surface_count(&contract),
        contract.raw_unique_surface_count
    );
    assert_eq!(contract.review_group_count, 1_500);
    assert!(contract.review_group_count <= contract.expected.review_group_count_max);
    assert!(contract.hard_negative_family_count > 0);
    assert!(contract.repeated_ambiguity_family_count > 0);
}

#[test]
#[allow(non_snake_case)]
fn EN_B001_cmbs_500k_exact_bucket_probe_stays_compact() {
    let contract = stress_contract();
    validate_block_exact_bucket_size_limit(
        "exact_bucket:tenant_core",
        &contract.exact_bucket_probe.bucket_value,
        contract.exact_bucket_probe.row_count,
        contract.expected.max_exact_bucket_size,
    )
    .expect("CMBS stress exact bucket stays within configured compact limit");

    let result = emit_exact_bucket_hyperedges(ExactBucketBlockRequest {
        profile: sample_profile(),
        upstream: sample_upstream(),
        operator_id: "exact_bucket:tenant_core".to_string(),
        identity_view: contract.view_name.clone(),
        placeholder_values: BTreeSet::from(["vacant".to_string(), "0".to_string()]),
        surfaces: vec![
            ExactBucketSurface::new(
                &contract.exact_bucket_probe.surface_id,
                &contract.exact_bucket_probe.bucket_value,
                contract.exact_bucket_probe.row_count,
                contract.exact_bucket_probe.deal_count,
            ),
            ExactBucketSurface::new("surf:cmbs-placeholder-zero", "0", 50_000, 3_000),
        ],
    })
    .expect("CMBS stress exact bucket emits compact assertion");
    let assertion = result
        .assertions
        .first()
        .expect("one non-placeholder bucket");

    assert_eq!(result.assertions.len(), 1);
    assert_eq!(result.diagnostics.expanded_pair_count, 0);
    assert_eq!(
        result.diagnostics.excluded_placeholder_bucket_count, 1,
        "placeholder zero must not become an entity bucket"
    );
    assert_eq!(
        assertion.pair_expansion,
        EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN
    );
    assert_eq!(assertion.expanded_pair_count(), 0);
    assert_eq!(
        assertion.diagnostics.suppressed_pair_count,
        theoretical_pair_count(contract.exact_bucket_probe.row_count)
    );
}

#[test]
fn cmbs_500k_candidate_bounds_are_structural_not_wall_clock() {
    let contract = stress_contract();
    let observations = virtual_candidate_observations(
        &contract,
        contract.ci_sample_surface_count,
        contract
            .virtual_candidate_stream
            .emitted_candidates_per_surface,
    );
    let emitted = emitted_total(
        contract.ci_sample_surface_count,
        contract
            .virtual_candidate_stream
            .emitted_candidates_per_surface,
    );
    let budget = BlockCandidateBudgetConfig::new(contract.topk.candidate_cap, emitted, emitted);
    let diagnostics =
        validate_block_candidate_budget_before_artifact_emission(&budget, &observations)
            .expect("CMBS sample stays within configured candidate budget");

    assert_within_g11_caps(&contract, &diagnostics, contract.ci_sample_surface_count);
    assert!(
        theoretical_pair_count(contract.ci_sample_surface_count) > emitted,
        "fixture must prove bounded neighborhoods instead of all-pairs expansion"
    );
}

#[test]
#[ignore = "operator stress tier: validates the full CMBS-shaped 500k-row virtual stream"]
fn cmbs_500k_stress() {
    let contract = stress_contract();
    let observations = virtual_candidate_observations(
        &contract,
        contract.prepared_surface_count,
        contract
            .virtual_candidate_stream
            .emitted_candidates_per_surface,
    );
    let emitted = emitted_total(
        contract.prepared_surface_count,
        contract
            .virtual_candidate_stream
            .emitted_candidates_per_surface,
    );
    let budget = BlockCandidateBudgetConfig::new(contract.topk.candidate_cap, emitted, emitted);
    let diagnostics =
        validate_block_candidate_budget_before_artifact_emission(&budget, &observations)
            .expect("full virtual CMBS 500k stream stays within configured caps");
    assert_within_g11_caps(&contract, &diagnostics, contract.prepared_surface_count);

    let telemetry = stress_telemetry(&contract, &diagnostics, "bounded_completion");
    assert_eq!(telemetry.raw_row_count, contract.row_count);
    assert_eq!(
        telemetry.raw_unique_surface_count,
        contract.raw_unique_surface_count
    );
    assert_eq!(
        telemetry.prepared_surface_count,
        contract.prepared_surface_count
    );
    assert_eq!(telemetry.candidate_pair_count, emitted);
    assert_eq!(
        telemetry.exact_bucket_pair_expansion_count,
        contract.expected.exact_bucket_pair_expansion_count
    );
    assert_eq!(telemetry.review_group_count, contract.review_group_count);
}

#[derive(Debug, Clone, Deserialize)]
struct Cmbs500kStressContract {
    schema_version: String,
    id: String,
    gate_id: String,
    seed: u64,
    profile_id: String,
    row_count: u64,
    deal_count: u64,
    raw_unique_surface_count: u64,
    prepared_surface_count: u64,
    exact_resolved_surface_count: u64,
    exact_bucket_count: u64,
    largest_exact_bucket_size: u64,
    review_group_count: u64,
    hard_negative_family_count: u64,
    repeated_ambiguity_family_count: u64,
    ci_sample_row_count: u64,
    ci_sample_surface_count: u64,
    view_name: String,
    operator_id: String,
    topk: TopKContract,
    virtual_candidate_stream: VirtualCandidateStream,
    exact_bucket_probe: ExactBucketProbe,
    expected: StressExpectations,
}

#[derive(Debug, Clone, Deserialize)]
struct TopKContract {
    k: u64,
    candidate_cap: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct VirtualCandidateStream {
    emitted_candidates_per_surface: u64,
    suppressed_candidates_per_surface: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct ExactBucketProbe {
    bucket_value: String,
    surface_id: String,
    row_count: u64,
    deal_count: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct StressExpectations {
    candidate_pairs_per_surface_p95_max: u64,
    candidate_pairs_per_surface_p99_max: u64,
    exact_bucket_pair_expansion_count: u64,
    review_group_count_max: u64,
    max_exact_bucket_size: u64,
    generated_static_artifact_policy: String,
    first_row_signatures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct GeneratedCmbsTenantRow {
    row_id: String,
    deal_key: String,
    property_id: String,
    tenant_name: String,
    normalized_surface_key: String,
    source_slot: String,
}

fn stress_contract() -> Cmbs500kStressContract {
    serde_json::from_str(CMBS_500K_CONTRACT).expect("CMBS 500k stress contract parses")
}

fn sample_rows(contract: &Cmbs500kStressContract, limit: u64) -> Vec<GeneratedCmbsTenantRow> {
    (0..limit)
        .map(|row_ordinal| generated_row(contract, row_ordinal))
        .collect()
}

fn generated_row(contract: &Cmbs500kStressContract, row_ordinal: u64) -> GeneratedCmbsTenantRow {
    let surface_ordinal = surface_ordinal(contract, row_ordinal);
    let deal_ordinal = (row_ordinal
        .saturating_mul(7_919)
        .saturating_add(contract.seed))
        % contract.deal_count;
    let property_ordinal = (row_ordinal
        .saturating_mul(1_543)
        .saturating_add(contract.seed))
        % 60_000;
    let (tenant_root, normalized_root) = tenant_family(surface_ordinal);
    GeneratedCmbsTenantRow {
        row_id: format!("cmbs-{row_ordinal:06}"),
        deal_key: format!("DEAL-{deal_ordinal:06}"),
        property_id: format!("PROP-{property_ordinal:06}"),
        tenant_name: format!("{tenant_root} #{surface_ordinal:05}"),
        normalized_surface_key: format!("{normalized_root}:{surface_ordinal:05}"),
        source_slot: format!("tenant_{}", row_ordinal % 3 + 1),
    }
}

fn surface_ordinal(contract: &Cmbs500kStressContract, row_ordinal: u64) -> u64 {
    (row_ordinal.saturating_mul(7).saturating_add(contract.seed))
        % contract.raw_unique_surface_count
}

fn tenant_family(surface_ordinal: u64) -> (&'static str, &'static str) {
    match surface_ordinal % 8 {
        0 => ("Sears", "sears"),
        1 => ("SEARS LLC", "sears"),
        2 => ("Sears Auto Center", "sears_auto_center"),
        3 => ("Kmart", "kmart"),
        4 => ("Transform SR LLC", "transform_sr"),
        5 => ("24 Hour Fitness", "24_hour_fitness"),
        6 => ("238 Sand Island Property LLC", "238_sand_island_property"),
        _ => ("Vacant", "vacant"),
    }
}

fn row_signatures(rows: &[GeneratedCmbsTenantRow]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            format!(
                "{}|{}|{}|{}",
                row.row_id, row.deal_key, row.tenant_name, row.normalized_surface_key
            )
        })
        .collect()
}

fn generated_surface_count(contract: &Cmbs500kStressContract) -> u64 {
    let mut surfaces = BTreeSet::new();
    for row_ordinal in 0..contract.row_count {
        surfaces.insert(surface_ordinal(contract, row_ordinal));
        if surfaces.len() as u64 == contract.raw_unique_surface_count {
            break;
        }
    }
    surfaces.len() as u64
}

fn virtual_candidate_observations(
    contract: &Cmbs500kStressContract,
    surface_count: u64,
    emitted_candidates_per_surface: u64,
) -> Vec<BlockCandidateBudgetObservation> {
    let mut observations = Vec::with_capacity(usize::try_from(surface_count).unwrap_or(0));
    for index in 0..surface_count {
        observations.push(BlockCandidateBudgetObservation::new(
            surface_id(index),
            &contract.operator_id,
            emitted_candidates_per_surface,
            contract
                .virtual_candidate_stream
                .suppressed_candidates_per_surface,
        ));
    }
    observations
}

fn assert_within_g11_caps(
    contract: &Cmbs500kStressContract,
    diagnostics: &BlockCandidateBudgetDiagnostics,
    surface_count: u64,
) {
    assert_eq!(
        diagnostics.candidate_pairs_per_surface_p50,
        contract
            .virtual_candidate_stream
            .emitted_candidates_per_surface
    );
    assert!(
        diagnostics.candidate_pairs_per_surface_p95
            <= contract.expected.candidate_pairs_per_surface_p95_max
    );
    assert!(
        diagnostics.candidate_pairs_per_surface_p99
            <= contract.expected.candidate_pairs_per_surface_p99_max
    );
    assert_eq!(
        diagnostics.max_candidates_for_surface,
        contract
            .virtual_candidate_stream
            .emitted_candidates_per_surface
    );
    assert_eq!(
        diagnostics.suppressed_candidate_count,
        suppressed_total(contract, surface_count)
    );
    assert!(diagnostics.candidate_budget.validated);
    assert!(!diagnostics.partial_candidate_artifact_written);
}

fn stress_telemetry(
    contract: &Cmbs500kStressContract,
    diagnostics: &BlockCandidateBudgetDiagnostics,
    runtime_guard_status: &str,
) -> canon::entity::telemetry::EntityBenchmarkTelemetry {
    let mut harness = EntityTelemetryHarness::new(EntityTelemetryRunContext {
        run_id: format!("{}-seed-{}", contract.id, contract.seed),
        suite_id: contract.gate_id.clone(),
        profile: contract.profile_id.clone(),
        machine: EntityTelemetryMachine::detect_redacted(),
        cache_state: "cold".to_string(),
        input_hash: format!("fixture-seed:{}", contract.seed),
        profile_hash: "fixture-profile:cmbs_tenant_label".to_string(),
        strategy_hash: "fixture-strategy:cmbs_500k".to_string(),
        registry_snapshot_hash: "fixture-registry:cmbs-tenants".to_string(),
        patch_hash: "fixture-patch:none".to_string(),
        holdout_id: "stress-cmbs-500k".to_string(),
        metamorphic_relation_id: "none".to_string(),
    });
    harness
        .record_stage_ms("block", 0, 0)
        .expect("block stage records");
    harness
        .finish(
            EntityTelemetryCounters {
                raw_row_count: contract.row_count,
                raw_observation_count: contract.row_count,
                raw_unique_surface_count: contract.raw_unique_surface_count,
                prepared_surface_count: contract.prepared_surface_count,
                exact_resolved_surface_count: contract.exact_resolved_surface_count,
                candidate_pair_count: diagnostics.candidate_pairs_emitted,
                candidate_pairs_per_surface_p50: diagnostics.candidate_pairs_per_surface_p50,
                candidate_pairs_per_surface_p95: diagnostics.candidate_pairs_per_surface_p95,
                candidate_pairs_per_surface_p99: diagnostics.candidate_pairs_per_surface_p99,
                suppressed_candidate_count: diagnostics.suppressed_candidate_count,
                exact_bucket_count: contract.exact_bucket_count,
                exact_bucket_pair_expansion_count: contract
                    .expected
                    .exact_bucket_pair_expansion_count,
                largest_exact_bucket_size: contract.largest_exact_bucket_size,
                largest_component_size: contract.largest_exact_bucket_size,
                edge_count: diagnostics.candidate_pairs_emitted,
                review_group_count: contract.review_group_count,
            },
            EntityTelemetryOutcome {
                peak_memory_bytes: 1,
                peak_memory_method: "not_sampled_in_contract_test".to_string(),
                registry_pre_mutation_hash: "fixture-registry-before:cmbs-tenants".to_string(),
                registry_post_mutation_hash: "fixture-registry-after:cmbs-tenants".to_string(),
                runtime_guard_status: runtime_guard_status.to_string(),
                refusal_code: String::new(),
                next_command:
                    "cargo test --test entity_perf_stress -- --ignored cmbs_500k_stress --nocapture"
                        .to_string(),
            },
        )
        .expect("CMBS stress telemetry validates")
}

fn sample_profile() -> ExactBucketProfile {
    ExactBucketProfile {
        id: "cmbs_tenant_label".to_string(),
        version: "0.1.0".to_string(),
        identity_semantics: "canonical_display_label".to_string(),
        content_hash: "blake3:profile".to_string(),
    }
}

fn sample_upstream() -> ExactBucketUpstream {
    ExactBucketUpstream {
        prepare_hash: "blake3:prepare".to_string(),
        index_hash: "blake3:index".to_string(),
        strategy_hash: "blake3:block-strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
    }
}

fn emitted_total(surface_count: u64, emitted_candidates_per_surface: u64) -> u64 {
    surface_count.saturating_mul(emitted_candidates_per_surface)
}

fn suppressed_total(contract: &Cmbs500kStressContract, surface_count: u64) -> u64 {
    surface_count.saturating_mul(
        contract
            .virtual_candidate_stream
            .suppressed_candidates_per_surface,
    )
}

fn theoretical_pair_count(count: u64) -> u64 {
    count.saturating_mul(count.saturating_sub(1)) / 2
}

fn surface_id(index: u64) -> String {
    format!("surf:cmbs-{index:06}")
}
