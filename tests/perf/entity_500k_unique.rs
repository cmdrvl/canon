#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        block::{
            BlockCandidateBudgetConfig, BlockCandidateBudgetDiagnostics,
            BlockCandidateBudgetObservation, BlockCandidateGenerationRequest,
            BlockCandidateOperator, RareTokenOverlapBlockOperator, generate_block_candidates,
            validate_block_candidate_budget_before_artifact_emission,
            validate_block_exact_bucket_size_limit,
        },
        postings::{EntityPostingBuildConfig, EntityPostingIndex, EntityPostingSurface},
        stream::{EntityStreamStage, stream_io_budget_refusal},
        telemetry::{
            EntityTelemetryCounters, EntityTelemetryHarness, EntityTelemetryMachine,
            EntityTelemetryOutcome, EntityTelemetryRunContext,
        },
    },
};
use serde::{Deserialize, Serialize};

const UNIQUE_500K_CONTRACT: &str =
    include_str!("../fixtures/entity/stress/generators/unique_500k_contract.json");

#[test]
fn candidate_budget_refusal_is_deterministic_for_unique_surface_stress_probe() {
    let contract = stress_contract();
    assert_eq!(contract.schema_version, "canon.entity.stress_generator.v0");
    assert_eq!(contract.id, "entity_500k_unique_stress");
    assert_eq!(contract.gate_id, "G12");
    assert_eq!(contract.seed, 424243);
    assert_eq!(contract.surface_count, 500_000);
    assert_eq!(
        contract.expected.generated_static_artifact_policy,
        "do_not_commit_generated_500k_rows"
    );

    let first = sample_surfaces(&contract, contract.ci_sample_surface_count);
    let second = sample_surfaces(&contract, contract.ci_sample_surface_count);
    assert_eq!(first, second, "same seed/config must be byte-identical");
    assert_eq!(
        surface_signatures(&first[..3]),
        contract.expected.first_surface_signatures
    );

    let mut changed_seed = contract.clone();
    changed_seed.seed = changed_seed.seed.saturating_add(1);
    assert_ne!(
        sample_surfaces(&changed_seed, 3),
        first[..3],
        "seed must participate in generated surface ordering"
    );

    let observations = virtual_candidate_observations(
        &contract,
        contract.refusal_probe_surface_count,
        contract
            .virtual_candidate_stream
            .emitted_candidates_per_surface,
    );
    let emitted = emitted_total(
        contract.refusal_probe_surface_count,
        contract
            .virtual_candidate_stream
            .emitted_candidates_per_surface,
    );
    let refusal_budget = BlockCandidateBudgetConfig::new(
        contract.topk.candidate_cap,
        emitted,
        emitted.saturating_sub(1),
    );

    let first =
        validate_block_candidate_budget_before_artifact_emission(&refusal_budget, &observations)
            .expect_err("run cap refuses before writing a candidate artifact");
    let mut reversed = observations.clone();
    reversed.reverse();
    let second =
        validate_block_candidate_budget_before_artifact_emission(&refusal_budget, &reversed)
            .expect_err("same over-budget stream refuses deterministically");

    assert_eq!(first.code, RefusalCode::EEntityCandidateBudget);
    assert_eq!(
        first.detail["refusal_code"],
        contract.expected.candidate_budget_refusal_code
    );
    assert_eq!(first.detail["policy_id"], "block.max_candidates_per_run");
    assert_eq!(first.detail["subject_kind"], "run");
    assert_eq!(first.detail["observed"], emitted);
    assert_eq!(first.detail["configured"], emitted - 1);
    assert_eq!(
        first.detail["candidate_pairs_per_surface_p95"],
        contract.expected.candidate_pairs_per_surface_p95_max
    );
    assert_eq!(
        first.detail["candidate_pairs_per_surface_p99"],
        contract.expected.candidate_pairs_per_surface_p99_max
    );
    assert_eq!(
        first.detail["suppressed_candidate_count"],
        suppressed_total(&contract, contract.refusal_probe_surface_count)
    );
    assert_eq!(first.detail["candidate_artifact_written"], false);
    assert_eq!(first.detail["partial_candidate_artifact_written"], false);
    assert_eq!(
        serde_json::to_vec(&first).expect("first refusal serializes"),
        serde_json::to_vec(&second).expect("second refusal serializes")
    );
}

#[test]
fn large_posting_cap_suppresses_common_token_bucket_and_reports_index_limit() {
    let contract = stress_contract();
    let posting_index = sample_posting_index(&contract, contract.ci_sample_surface_count);
    let budget = BlockCandidateBudgetConfig::new(
        contract.topk.candidate_cap,
        contract.topk.candidate_cap * contract.ci_sample_surface_count,
        contract.topk.candidate_cap * contract.ci_sample_surface_count,
    );

    let result = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: contract.profile_id.clone(),
        posting_index: &posting_index,
        ngram_index: None,
        budget_config: budget,
        operators: vec![BlockCandidateOperator::RareTokenOverlap(
            RareTokenOverlapBlockOperator::new(
                "rare_token_overlap:unique_500k_common_token",
                contract.view_name.clone(),
            )
            .with_min_idf_units(0)
            .with_topk(
                usize::try_from(contract.topk.k).expect("top-k fits usize"),
                usize::try_from(contract.topk.candidate_cap).expect("cap fits usize"),
            )
            .with_max_posting_size(
                usize::try_from(contract.rare_token_max_posting_size)
                    .expect("posting cap fits usize"),
            ),
        )],
    })
    .expect("large common postings are suppressed instead of expanded");

    assert!(result.candidates.is_empty());
    assert_eq!(result.diagnostics.candidate_record_count, 0);
    assert_eq!(result.diagnostics.candidate_pairs_emitted, 0);
    assert_eq!(
        result.diagnostics.large_buckets_suppressed,
        contract.ci_sample_surface_count
    );
    assert_eq!(
        result.diagnostics.operator_diagnostics[0].large_posting_suppressed_count,
        contract.ci_sample_surface_count
    );
    assert_eq!(
        result.diagnostics.candidate_pairs_per_surface_p95, 0,
        "large posting cap must not materialize row or surface pairs"
    );

    let index_refusal = validate_block_exact_bucket_size_limit(
        "exact_bucket:tenant_core",
        "bucket:tenant",
        contract.surface_count,
        contract.expected.max_exact_bucket_size,
    )
    .expect_err("oversized exact bucket reports an index-limit refusal");
    assert_eq!(index_refusal.code, RefusalCode::EEntityIndexLimit);
    assert_eq!(
        index_refusal.detail["refusal_code"],
        contract.expected.index_limit_refusal_code
    );
    assert_eq!(index_refusal.detail["pair_expansion"], "forbidden");
    assert_eq!(index_refusal.detail["observed"], contract.surface_count);
}

#[test]
fn io_budget_refusal_code_is_available_for_stress_abort() {
    let contract = stress_contract();
    let refusal = stream_io_budget_refusal(
        EntityStreamStage::Prepare,
        "max_rows",
        contract.surface_count + 1,
        contract.surface_count,
    );

    assert_eq!(refusal.code, RefusalCode::EEntityIoBudget);
    assert_eq!(refusal.detail["stage"], EntityStreamStage::Prepare.as_str());
    assert_eq!(refusal.detail["limit"], "max_rows");
    assert_eq!(refusal.detail["observed"], contract.surface_count + 1);
    assert_eq!(refusal.detail["configured"], contract.surface_count);
    assert_eq!(
        serde_json::to_value(&refusal.code).expect("refusal code serializes"),
        contract.expected.io_budget_refusal_code
    );
}

#[test]
#[ignore = "operator stress tier: validates the full 500k unique-surface virtual stream"]
fn entity_500k_unique_stress() {
    let contract = stress_contract();
    let observations = virtual_candidate_observations(
        &contract,
        contract.surface_count,
        contract
            .virtual_candidate_stream
            .emitted_candidates_per_surface,
    );
    let full_pair_expansion = pair_expansion_count(contract.surface_count);
    let emitted = emitted_total(
        contract.surface_count,
        contract
            .virtual_candidate_stream
            .emitted_candidates_per_surface,
    );
    assert!(
        u128::from(emitted) * 1_000 < full_pair_expansion,
        "stress generator must stay a bounded neighborhood, not all-pairs expansion"
    );

    let budget = BlockCandidateBudgetConfig::new(contract.topk.candidate_cap, emitted, emitted);
    let diagnostics =
        validate_block_candidate_budget_before_artifact_emission(&budget, &observations)
            .expect("full virtual 500k unique stream stays within configured caps");
    assert_within_g12_caps(&contract, &diagnostics);

    let telemetry = stress_telemetry(&contract, &diagnostics, "none", "bounded_completion");
    assert_eq!(telemetry.raw_unique_surface_count, contract.surface_count);
    assert_eq!(telemetry.prepared_surface_count, contract.surface_count);
    assert_eq!(telemetry.candidate_pair_count, emitted);
    assert_eq!(
        telemetry.suppressed_candidate_count,
        suppressed_total(&contract, contract.surface_count)
    );
    assert_eq!(
        telemetry.exact_bucket_pair_expansion_count,
        contract.expected.exact_bucket_pair_expansion_count
    );
}

#[derive(Debug, Clone, Deserialize)]
struct Unique500kStressContract {
    schema_version: String,
    id: String,
    gate_id: String,
    seed: u64,
    profile_id: String,
    surface_count: u64,
    ci_sample_surface_count: u64,
    refusal_probe_surface_count: u64,
    view_name: String,
    common_token: String,
    unique_token_prefix: String,
    operator_id: String,
    common_posting_limit: usize,
    rare_token_max_posting_size: u64,
    topk: TopKContract,
    virtual_candidate_stream: VirtualCandidateStream,
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
struct StressExpectations {
    candidate_pairs_per_surface_p95_max: u64,
    candidate_pairs_per_surface_p99_max: u64,
    exact_bucket_pair_expansion_count: u64,
    candidate_budget_refusal_code: String,
    index_limit_refusal_code: String,
    io_budget_refusal_code: String,
    max_exact_bucket_size: u64,
    generated_static_artifact_policy: String,
    first_surface_signatures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct GeneratedUniqueSurface {
    surface_id: String,
    raw_name: String,
    normalized_surface_key: String,
    common_token: String,
    unique_token: String,
}

fn stress_contract() -> Unique500kStressContract {
    serde_json::from_str(UNIQUE_500K_CONTRACT).expect("500k unique stress contract parses")
}

fn virtual_candidate_observations(
    contract: &Unique500kStressContract,
    surface_count: u64,
    emitted_candidates_per_surface: u64,
) -> Vec<BlockCandidateBudgetObservation> {
    let mut observations = Vec::with_capacity(usize::try_from(surface_count).unwrap_or(0));
    for index in 0..surface_count {
        observations.push(BlockCandidateBudgetObservation::new(
            surface_id(index),
            contract.operator_id.clone(),
            emitted_candidates_per_surface,
            contract
                .virtual_candidate_stream
                .suppressed_candidates_per_surface,
        ));
    }
    observations
}

fn sample_posting_index(
    contract: &Unique500kStressContract,
    surface_count: u64,
) -> EntityPostingIndex {
    let surfaces = (0..surface_count)
        .map(|index| {
            let surface = generated_surface(contract, index);
            EntityPostingSurface::new(surface.surface_id)
                .with_exact_view(&contract.view_name, surface.raw_name)
                .with_tokens([surface.common_token, surface.unique_token])
        })
        .collect::<Vec<_>>();
    EntityPostingIndex::build(
        &surfaces,
        EntityPostingBuildConfig {
            common_posting_limit: contract.common_posting_limit,
        },
    )
    .expect("sample posting index builds")
}

fn sample_surfaces(contract: &Unique500kStressContract, limit: u64) -> Vec<GeneratedUniqueSurface> {
    (0..limit)
        .map(|surface_ordinal| generated_surface(contract, surface_ordinal))
        .collect()
}

fn generated_surface(
    contract: &Unique500kStressContract,
    surface_ordinal: u64,
) -> GeneratedUniqueSurface {
    let generated_ordinal = (surface_ordinal
        .saturating_mul(97)
        .saturating_add(contract.seed))
        % contract.surface_count;
    GeneratedUniqueSurface {
        surface_id: format!("surf:unique-{generated_ordinal:06}"),
        raw_name: format!("Tenant Unique {generated_ordinal:06}"),
        normalized_surface_key: format!("tenant_unique:{generated_ordinal:06}"),
        common_token: contract.common_token.clone(),
        unique_token: format!("{}{generated_ordinal:06}", contract.unique_token_prefix),
    }
}

fn surface_signatures(surfaces: &[GeneratedUniqueSurface]) -> Vec<String> {
    surfaces
        .iter()
        .map(|surface| {
            format!(
                "{}|{}|{}|{}",
                surface.surface_id,
                surface.raw_name,
                surface.normalized_surface_key,
                surface.unique_token
            )
        })
        .collect()
}

fn assert_within_g12_caps(
    contract: &Unique500kStressContract,
    diagnostics: &BlockCandidateBudgetDiagnostics,
) {
    assert_eq!(
        diagnostics.candidate_pairs_per_surface_p50,
        contract.topk.candidate_cap
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
        contract.topk.candidate_cap
    );
    assert_eq!(
        diagnostics.suppressed_candidate_count,
        suppressed_total(contract, contract.surface_count)
    );
    assert!(diagnostics.candidate_budget.validated);
    assert!(!diagnostics.partial_candidate_artifact_written);
}

fn stress_telemetry(
    contract: &Unique500kStressContract,
    diagnostics: &BlockCandidateBudgetDiagnostics,
    refusal_code: &str,
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
        strategy_hash: "fixture-strategy:unique_500k".to_string(),
        registry_snapshot_hash: "fixture-registry:none".to_string(),
        patch_hash: "fixture-patch:none".to_string(),
        holdout_id: "stress-unique-500k".to_string(),
        metamorphic_relation_id: "none".to_string(),
    });
    harness
        .record_stage_ms("block", 0, 0)
        .expect("block stage records");
    harness
        .finish(
            EntityTelemetryCounters {
                raw_row_count: contract.surface_count,
                raw_observation_count: contract.surface_count,
                raw_unique_surface_count: contract.surface_count,
                prepared_surface_count: contract.surface_count,
                exact_resolved_surface_count: 0,
                candidate_pair_count: diagnostics.candidate_pairs_emitted,
                candidate_pairs_per_surface_p50: diagnostics.candidate_pairs_per_surface_p50,
                candidate_pairs_per_surface_p95: diagnostics.candidate_pairs_per_surface_p95,
                candidate_pairs_per_surface_p99: diagnostics.candidate_pairs_per_surface_p99,
                suppressed_candidate_count: diagnostics.suppressed_candidate_count,
                exact_bucket_count: 0,
                exact_bucket_pair_expansion_count: 0,
                largest_exact_bucket_size: 0,
                largest_component_size: 1,
                edge_count: diagnostics.candidate_pairs_emitted,
                review_group_count: 0,
            },
            EntityTelemetryOutcome {
                peak_memory_bytes: 1,
                peak_memory_method: "not_sampled_in_contract_test".to_string(),
                registry_pre_mutation_hash: "fixture-registry-before:none".to_string(),
                registry_post_mutation_hash: "fixture-registry-after:none".to_string(),
                runtime_guard_status: runtime_guard_status.to_string(),
                refusal_code: refusal_code.to_string(),
                next_command: String::new(),
            },
        )
        .expect("stress telemetry validates")
}

fn emitted_total(surface_count: u64, emitted_candidates_per_surface: u64) -> u64 {
    surface_count.saturating_mul(emitted_candidates_per_surface)
}

fn suppressed_total(contract: &Unique500kStressContract, surface_count: u64) -> u64 {
    surface_count.saturating_mul(
        contract
            .virtual_candidate_stream
            .suppressed_candidates_per_surface,
    )
}

fn pair_expansion_count(surface_count: u64) -> u128 {
    let count = u128::from(surface_count);
    count.saturating_mul(count.saturating_sub(1)) / 2
}

fn surface_id(index: u64) -> String {
    format!("surf:{index:06}")
}
