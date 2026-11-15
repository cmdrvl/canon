#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::block::{
        ABSOLUTE_BLOCK_MAX_CANDIDATES_PER_RUN, BlockCandidateBudgetConfig,
        BlockCandidateBudgetObservation, default_block_runtime_config, load_block_runtime_config,
        validate_block_candidate_budget_before_artifact_emission,
        validate_profile_budget_placement,
    },
    entity::run::{EntityRunRequest, run_entity_workbench},
};
use serde_json::json;
use std::{fs, path::Path};

#[test]
fn block_candidate_budget_matrix_allows_within_caps_and_reports_diagnostics() {
    let diagnostics = validate_block_candidate_budget_before_artifact_emission(
        &BlockCandidateBudgetConfig::new(10, 13, 16),
        &[
            observation("surface-a", "exact_view", 2, 0),
            observation("surface-a", "rare_token", 3, 1),
            observation("surface-b", "exact_view", 1, 0),
            observation("surface-c", "rare_token", 10, 4),
        ],
    )
    .expect("within-budget block may emit a candidate artifact");

    assert_eq!(diagnostics.candidate_pairs_emitted, 16);
    assert_eq!(diagnostics.candidate_pairs_suppressed_by_cap, 5);
    assert_eq!(diagnostics.suppressed_candidate_count, 5);
    assert_eq!(diagnostics.candidate_pairs_per_surface_p50, 5);
    assert_eq!(diagnostics.candidate_pairs_per_surface_p95, 10);
    assert_eq!(diagnostics.candidate_pairs_per_surface_p99, 10);
    assert_eq!(diagnostics.max_candidates_for_surface, 10);
    assert_eq!(diagnostics.max_candidates_for_operator, 13);
    assert!(diagnostics.candidate_budget.validated);
    assert_eq!(
        diagnostics.candidate_budget.policy_id,
        "block.max_candidates_per_run"
    );
    assert_eq!(diagnostics.candidate_budget.observed, 16);
    assert_eq!(diagnostics.candidate_budget.configured, 16);
    assert!(!diagnostics.partial_candidate_artifact_written);
}

#[test]
fn block_candidate_budget_matrix_refuses_before_artifact_write_on_surface_breach() {
    let refusal = validate_block_candidate_budget_before_artifact_emission(
        &BlockCandidateBudgetConfig::new(5, 100, 100),
        &[
            observation("surface-z", "rare_token", 6, 2),
            observation("surface-a", "rare_token", 6, 1),
        ],
    )
    .expect_err("over-budget surface refuses before candidate artifact emission");

    assert_eq!(refusal.code, RefusalCode::EEntityCandidateBudget);
    assert_eq!(refusal.detail["stage"], "block");
    assert_eq!(refusal.detail["artifact"], "candidate_artifact");
    assert_eq!(refusal.detail["reason"], "candidate_budget_exceeded");
    assert_eq!(
        refusal.detail["policy_id"],
        "block.max_candidates_per_surface"
    );
    assert_eq!(
        refusal.detail["budget"]["policy_id"],
        "block.max_candidates_per_surface"
    );
    assert_eq!(refusal.detail["subject_kind"], "surface");
    assert_eq!(refusal.detail["subject_id"], "surface-a");
    assert_eq!(refusal.detail["observed"], 6);
    assert_eq!(refusal.detail["configured"], 5);
    assert_eq!(refusal.detail["candidate_pairs_per_surface_p95"], 6);
    assert_eq!(refusal.detail["candidate_pairs_per_surface_p99"], 6);
    assert_eq!(refusal.detail["suppressed_candidate_count"], 3);
    assert_eq!(
        refusal.detail["partial_candidate_artifact_written"],
        json!(false)
    );
    assert_eq!(refusal.detail["candidate_artifact_written"], json!(false));
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|command| command.contains("per-surface cap"))
    );
}

#[test]
fn block_candidate_budget_matrix_refuses_operator_and_run_breaches_deterministically() {
    let operator_refusal = validate_block_candidate_budget_before_artifact_emission(
        &BlockCandidateBudgetConfig::new(10, 5, 100),
        &[
            observation("surface-a", "operator-z", 4, 0),
            observation("surface-b", "operator-a", 4, 0),
            observation("surface-c", "operator-a", 2, 0),
        ],
    )
    .expect_err("over-budget operator refuses");

    assert_eq!(
        operator_refusal.detail["policy_id"],
        "block.max_candidates_per_operator"
    );
    assert_eq!(operator_refusal.detail["subject_kind"], "operator");
    assert_eq!(operator_refusal.detail["subject_id"], "operator-a");
    assert_eq!(operator_refusal.detail["observed"], 6);
    assert_eq!(operator_refusal.detail["configured"], 5);
    assert_eq!(
        operator_refusal.detail["partial_candidate_artifact_written"],
        json!(false)
    );
    assert_eq!(
        operator_refusal.detail["candidate_artifact_written"],
        json!(false)
    );

    let run_refusal = validate_block_candidate_budget_before_artifact_emission(
        &BlockCandidateBudgetConfig::new(10, 10, 5),
        &[
            observation("surface-a", "operator-z", 3, 0),
            observation("surface-b", "operator-a", 3, 0),
        ],
    )
    .expect_err("over-budget run refuses");

    assert_eq!(
        run_refusal.detail["policy_id"],
        "block.max_candidates_per_run"
    );
    assert_eq!(run_refusal.detail["subject_kind"], "run");
    assert_eq!(run_refusal.detail["subject_id"], json!(null));
    assert_eq!(run_refusal.detail["observed"], 6);
    assert_eq!(run_refusal.detail["configured"], 5);
    assert_eq!(
        run_refusal.detail["partial_candidate_artifact_written"],
        json!(false)
    );
    assert_eq!(
        run_refusal.detail["candidate_artifact_written"],
        json!(false)
    );
}

#[test]
fn block_candidate_budget_matrix_empty_observations_report_zeroes() {
    let diagnostics = validate_block_candidate_budget_before_artifact_emission(
        &BlockCandidateBudgetConfig::new(1, 1, 1),
        &[],
    )
    .expect("empty candidate stream is within budget");

    assert_eq!(diagnostics.candidate_pairs_emitted, 0);
    assert_eq!(diagnostics.suppressed_candidate_count, 0);
    assert_eq!(diagnostics.candidate_pairs_per_surface_p50, 0);
    assert_eq!(diagnostics.candidate_pairs_per_surface_p95, 0);
    assert_eq!(diagnostics.candidate_pairs_per_surface_p99, 0);
    assert_eq!(diagnostics.max_candidates_for_surface, 0);
    assert_eq!(diagnostics.max_candidates_for_operator, 0);
    assert!(diagnostics.candidate_budget.validated);
    assert_eq!(diagnostics.candidate_budget.observed, 0);
    assert_eq!(diagnostics.candidate_budget.configured, 1);
    assert!(!diagnostics.partial_candidate_artifact_written);
}

#[test]
fn implicit_operator_and_run_defaults_scale_with_surface_count() {
    let default_budget = default_block_runtime_config().candidate_budget;
    let scaled = default_budget.for_surface_count(12_111);
    assert_eq!(scaled.max_candidates_per_surface, 100);
    assert_eq!(scaled.max_candidates_per_operator, 1_211_100);
    assert_eq!(scaled.max_candidates_per_run, 1_211_100);

    let explicit = BlockCandidateBudgetConfig::new(100, 100, 100);
    let explicit_scaled = explicit.for_surface_count(12_111);
    assert_eq!(explicit_scaled.max_candidates_per_operator, 100);
    assert_eq!(explicit_scaled.max_candidates_per_run, 100);
}

#[test]
fn candidate_budget_refusal_names_strategy_key_and_pasteable_fragment() {
    let refusal = validate_block_candidate_budget_before_artifact_emission(
        &BlockCandidateBudgetConfig::new(100, 100, 100),
        &[
            observation("surface-a", "operator-a", 51, 0),
            observation("surface-b", "operator-b", 51, 0),
        ],
    )
    .expect_err("the explicit run budget must refuse");

    assert_eq!(refusal.code, RefusalCode::EEntityCandidateBudget);
    assert_eq!(
        refusal.detail["configuration_key_path"],
        "block.max_candidates_per_run"
    );
    assert_eq!(refusal.detail["configuration_file_role"], "strategy file");
    assert_eq!(refusal.detail["observed"], 102);
    assert_eq!(refusal.detail["limit"], 100);
    assert_eq!(
        refusal.detail["suggested_fragment"],
        "block:\n  max_candidates_per_run: 101"
    );
    assert!(refusal.message.contains("block.max_candidates_per_run"));
}

#[test]
fn candidate_budget_refusal_fragment_can_be_pasted_and_passes_same_observations() {
    let temp = tempfile::tempdir().expect("tempdir");
    let strategy = temp.path().join("strategy.yaml");
    fs::write(&strategy, "block:\n  max_candidates_per_run: 100\n").expect("strategy");

    let observations = [
        observation("surface-a", "operator-a", 51, 0),
        observation("surface-b", "operator-b", 51, 0),
    ];
    let initial = load_block_runtime_config(&strategy).expect("low strategy budget loads");
    let refusal = validate_block_candidate_budget_before_artifact_emission(
        &initial.candidate_budget,
        &observations,
    )
    .expect_err("the low run budget refuses the same candidate stream");
    let suggested_fragment = refusal.detail["suggested_fragment"]
        .as_str()
        .expect("refusal includes a YAML fragment");
    assert_eq!(suggested_fragment, "block:\n  max_candidates_per_run: 102");

    fs::write(&strategy, format!("{suggested_fragment}\n")).expect("paste suggested fragment");
    let repaired = load_block_runtime_config(&strategy).expect("pasted strategy loads");
    validate_block_candidate_budget_before_artifact_emission(
        &repaired.candidate_budget,
        &observations,
    )
    .expect("pasted run budget permits the same candidate stream");
}

#[test]
fn entity_run_summary_warns_when_candidates_have_no_executed_support() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("rows.csv");
    fs::write(
        &rows,
        "source_row_id,dataset,doc_id,accession,filing_cik,field_name,org_name,role_context,capacity,subject_role,alias_surfaces_json,mention_surfaces_json\n\
r1,fixture,doc-1,acc-1,0000000001,servicer,Alpha Services LLC,servicer,primary,servicer,,[]\n\
r2,fixture,doc-2,acc-2,0000000002,servicer,Alpha Services Holdings LLC,servicer,primary,servicer,,[]\n",
    )
    .expect("rows");
    let work_dir = temp.path().join("work");
    let result = run_entity_workbench(EntityRunRequest {
        rows: &rows,
        profile: "regab_firm_identity",
        strategy: Path::new("tests/fixtures/entity/profiles/regab_firm_identity.yaml"),
        registry: Path::new("tests/fixtures/registries/empty"),
        work_dir: &work_dir,
    })
    .expect("entity run succeeds");

    assert_eq!(
        result.artifact.summary.labels["candidate_support_warning"],
        "candidates cannot be supported by any executed operator"
    );
}

#[test]
fn misplaced_strategy_budget_is_reported_instead_of_ignored() {
    let temp = tempfile::tempdir().expect("tempdir");
    let strategy = temp.path().join("strategy.yaml");
    fs::write(&strategy, "blocking:\n  max_candidates_per_run: 100\n").expect("strategy");

    let refusal = load_block_runtime_config(&strategy)
        .expect_err("blocking budget is not a strategy block budget");
    assert_eq!(refusal.code, RefusalCode::EEntityStrategy);
    assert_eq!(refusal.detail["reason"], "misplaced_candidate_budget");
    assert_eq!(
        refusal.detail["actual_key_path"],
        "blocking.max_candidates_per_run"
    );
    assert_eq!(
        refusal.detail["expected_key_path"],
        "block.max_candidates_per_run"
    );
    assert_eq!(
        refusal.detail["actual_file_role"],
        "strategy `blocking:` section"
    );
}

#[test]
fn profile_blocking_budget_is_reported_as_a_misplacement() {
    let temp = tempfile::tempdir().expect("tempdir");
    let profile = temp.path().join("profile.yaml");
    fs::write(&profile, "blocking:\n  max_candidates_per_run: 100\n").expect("profile");

    let refusal = validate_profile_budget_placement(Path::new(&profile))
        .expect_err("profile-owned block budget is misplaced");
    assert_eq!(refusal.code, RefusalCode::EEntityStrategy);
    assert_eq!(refusal.detail["actual_file_role"], "profile file");
    assert_eq!(refusal.detail["expected_file_role"], "strategy file");
    assert_eq!(
        refusal.detail["actual_key_path"],
        "blocking.max_candidates_per_run"
    );
}

#[test]
fn absolute_run_ceiling_still_refuses_an_explicitly_large_budget() {
    let refusal = validate_block_candidate_budget_before_artifact_emission(
        &BlockCandidateBudgetConfig::new(
            ABSOLUTE_BLOCK_MAX_CANDIDATES_PER_RUN,
            ABSOLUTE_BLOCK_MAX_CANDIDATES_PER_RUN,
            ABSOLUTE_BLOCK_MAX_CANDIDATES_PER_RUN.saturating_mul(2),
        ),
        &[observation(
            "surface-a",
            "operator-a",
            ABSOLUTE_BLOCK_MAX_CANDIDATES_PER_RUN.saturating_add(1),
            0,
        )],
    )
    .expect_err("absolute run ceiling must remain enforced");

    assert_eq!(
        refusal.detail["policy_id"],
        "block.max_candidates_per_operator"
    );
    assert_eq!(
        refusal.detail["configured"],
        ABSOLUTE_BLOCK_MAX_CANDIDATES_PER_RUN
    );
}

fn observation(
    surface_id: &str,
    operator_id: &str,
    emitted_candidate_count: u64,
    suppressed_candidate_count: u64,
) -> BlockCandidateBudgetObservation {
    BlockCandidateBudgetObservation::new(
        surface_id,
        operator_id,
        emitted_candidate_count,
        suppressed_candidate_count,
    )
}
