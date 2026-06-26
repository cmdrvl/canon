#![forbid(unsafe_code)]

use canon::entity::{
    block::{ExactBucketBlockRequest, ExactBucketSurface, emit_exact_bucket_hyperedges},
    block_artifact::{
        EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN, ExactBucketProfile, ExactBucketUpstream,
    },
    solve::{
        SolveBudgetAction, SolveBudgetConfig, SolveComponentBudgetInput,
        evaluate_solve_component_budget,
    },
};
use std::collections::BTreeSet;

#[test]
fn entity_block_exact_bucket_emits_summary_count() {
    let result = emit_exact_bucket_hyperedges(request(vec![
        ExactBucketSurface::new("surf:cmbs_tenant_label:blake3:sears", "sears", 183, 42),
        ExactBucketSurface::new("surf:cmbs_tenant_label:blake3:sears_llc", "sears", 19, 7),
    ]))
    .expect("exact bucket emits");

    assert_eq!(result.diagnostics.exact_bucket_count, 1);
    assert_eq!(result.diagnostics.emitted_bucket_count, 1);
    assert_eq!(result.diagnostics.expanded_pair_count, 0);
    assert_eq!(result.diagnostics.largest_bucket_size, 202);
    assert_eq!(result.assertions[0].membership.surface_ids.len(), 2);
    assert_eq!(
        result.assertions[0].pair_expansion,
        EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN
    );
}

#[test]
#[allow(non_snake_case)]
fn EN_B001_compact_sears_bucket() {
    let result = emit_exact_bucket_hyperedges(request(vec![ExactBucketSurface::new(
        "surf:cmbs_tenant_label:blake3:sears",
        "sears",
        8_000,
        934,
    )]))
    .expect("EN-B001 exact bucket emits");

    let bucket = &result.assertions[0];
    assert_eq!(bucket.bucket_id, "bucket:tenant_core:sears");
    assert_eq!(bucket.row_count, 8_000);
    assert_eq!(bucket.expanded_pair_count(), 0);
    assert_eq!(bucket.artifact_membership_record_count(), 1);
    assert_eq!(result.diagnostics.exact_bucket_count, 1);
    assert_eq!(
        result.diagnostics.suppressed_pair_count,
        8_000_u64 * 7_999 / 2
    );
}

#[test]
fn exact_bucket_no_pairs() {
    let surfaces = (0..256)
        .map(|index| {
            ExactBucketSurface::new(
                format!("surf:cmbs_tenant_label:blake3:{index:04}"),
                "sears",
                1,
                1,
            )
        })
        .collect::<Vec<_>>();

    let result = emit_exact_bucket_hyperedges(request(surfaces)).expect("compact bucket emits");

    assert_eq!(result.assertions.len(), 1);
    assert_eq!(result.assertions[0].theoretical_pair_count(), 32_640);
    assert_eq!(result.assertions[0].expanded_pair_count(), 0);
    assert_eq!(result.diagnostics.expanded_pair_count, 0);
    assert_eq!(result.diagnostics.membership_record_count, 256);
}

#[test]
fn exact_bucket_assertion_feeds_solve_membership_without_pair_expansion() {
    let result = emit_exact_bucket_hyperedges(request(vec![
        ExactBucketSurface::new("surf:cmbs_tenant_label:blake3:sears", "sears", 183, 42),
        ExactBucketSurface::new("surf:cmbs_tenant_label:blake3:sears_llc", "sears", 19, 7),
    ]))
    .expect("exact bucket emits");

    let assertion = &result.assertions[0];
    assertion.validate().expect("bucket assertion validates");
    assert_eq!(assertion.expanded_pair_count(), 0);

    let solve_decision = evaluate_solve_component_budget(
        SolveComponentBudgetInput::new(
            assertion.bucket_id.clone(),
            assertion.membership.surface_ids.clone(),
        ),
        SolveBudgetConfig::bounded_abstention(10),
    )
    .expect("compact exact bucket can feed solve membership");

    assert_eq!(solve_decision.action, SolveBudgetAction::Solve);
    assert_eq!(solve_decision.observed, 2);
    assert_eq!(solve_decision.surface_ids, assertion.membership.surface_ids);
}

fn request(surfaces: Vec<ExactBucketSurface>) -> ExactBucketBlockRequest {
    ExactBucketBlockRequest {
        profile: ExactBucketProfile {
            id: "cmbs_tenant_label".to_string(),
            version: "0.1.0".to_string(),
            identity_semantics: "canonical_display_label".to_string(),
            content_hash: "blake3:profile".to_string(),
        },
        upstream: ExactBucketUpstream {
            prepare_hash: "blake3:prepare".to_string(),
            index_hash: "blake3:index".to_string(),
            strategy_hash: "blake3:strategy".to_string(),
            registry_snapshot_hash: "blake3:registry".to_string(),
        },
        operator_id: "exact_view:tenant_core".to_string(),
        identity_view: "tenant_core".to_string(),
        placeholder_values: BTreeSet::from(["unknown".to_string(), "vacant".to_string()]),
        surfaces,
    }
}
