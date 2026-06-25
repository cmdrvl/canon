#![forbid(unsafe_code)]

use canon::entity::{
    CANON_ENTITY_BLOCK_BUCKET_VERSION,
    block_artifact::{
        CannotLinkAction, CannotLinkValidationHook, CannotLinkValidationStatus,
        EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN, ExactBucketAssertion, ExactBucketContractError,
        ExactBucketDiagnostics, ExactBucketMembership, ExactBucketProfile, ExactBucketUpstream,
        SurfaceIdRange,
    },
};
use std::collections::BTreeMap;

#[test]
fn exact_bucket_contract_represents_eight_thousand_rows_as_one_assertion() {
    let bucket = sample_bucket(
        ExactBucketMembership {
            surface_ids: vec!["surf:cmbs_tenant_label:blake3:sears".to_string()],
            surface_ranges: vec![],
        },
        8_000,
        934,
        CannotLinkValidationHook {
            status: CannotLinkValidationStatus::CheckedNoConflicts,
            checked_fact_count: 0,
            hard_cannot_link_count: 0,
            action: CannotLinkAction::AllowMerge,
        },
    );

    bucket.validate().expect("bucket contract validates");
    assert_eq!(bucket.pair_expansion, EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN);
    assert_eq!(bucket.artifact_membership_record_count(), 1);
    assert_eq!(bucket.expanded_pair_count(), 0);
    assert_eq!(bucket.theoretical_pair_count(), 0);
    assert_eq!(
        bucket.diagnostics.suppressed_pair_count,
        8_000_u64 * 7_999 / 2
    );
}

#[test]
fn exact_bucket_pair_expansion_forbidden_for_multi_surface_buckets() {
    let bucket = sample_bucket(
        ExactBucketMembership {
            surface_ids: vec![
                "surf:cmbs_tenant_label:blake3:sears".to_string(),
                "surf:cmbs_tenant_label:blake3:sears_llc".to_string(),
                "surf:cmbs_tenant_label:blake3:sears_roebuck".to_string(),
                "surf:cmbs_tenant_label:blake3:sears_store_1234".to_string(),
            ],
            surface_ranges: vec![],
        },
        183,
        42,
        CannotLinkValidationHook {
            status: CannotLinkValidationStatus::CheckedNoConflicts,
            checked_fact_count: 3,
            hard_cannot_link_count: 0,
            action: CannotLinkAction::AllowMerge,
        },
    );

    bucket.validate().expect("bucket contract validates");
    assert_eq!(bucket.artifact_membership_record_count(), 4);
    assert_eq!(bucket.theoretical_pair_count(), 6);
    assert_eq!(bucket.expanded_pair_count(), 0);

    let value = serde_json::to_value(&bucket).expect("bucket serializes");
    assert_eq!(value["version"], CANON_ENTITY_BLOCK_BUCKET_VERSION);
    assert_eq!(value["pair_expansion"], "forbidden");
    assert_eq!(
        value["membership"]["surface_ids"].as_array().unwrap().len(),
        4
    );
}

#[test]
fn exact_bucket_contract_accepts_sorted_surface_ranges_without_pair_expansion() {
    let bucket = sample_bucket(
        ExactBucketMembership {
            surface_ids: vec![],
            surface_ranges: vec![
                SurfaceIdRange {
                    start_surface_id: "surf:cmbs_tenant_label:blake3:0001".to_string(),
                    end_surface_id: "surf:cmbs_tenant_label:blake3:0100".to_string(),
                    member_count: 100,
                },
                SurfaceIdRange {
                    start_surface_id: "surf:cmbs_tenant_label:blake3:0200".to_string(),
                    end_surface_id: "surf:cmbs_tenant_label:blake3:0300".to_string(),
                    member_count: 101,
                },
            ],
        },
        500,
        15,
        CannotLinkValidationHook {
            status: CannotLinkValidationStatus::CheckedNoConflicts,
            checked_fact_count: 0,
            hard_cannot_link_count: 0,
            action: CannotLinkAction::AllowMerge,
        },
    );

    bucket.validate().expect("range bucket validates");
    assert_eq!(bucket.membership.member_count(), 201);
    assert_eq!(bucket.artifact_membership_record_count(), 2);
    assert_eq!(bucket.expanded_pair_count(), 0);
}

#[test]
fn exact_bucket_contract_rejects_unsorted_membership_and_pair_expansion() {
    let mut unsorted = sample_bucket(
        ExactBucketMembership {
            surface_ids: vec![
                "surf:cmbs_tenant_label:blake3:sears_llc".to_string(),
                "surf:cmbs_tenant_label:blake3:sears".to_string(),
            ],
            surface_ranges: vec![],
        },
        20,
        2,
        CannotLinkValidationHook {
            status: CannotLinkValidationStatus::CheckedNoConflicts,
            checked_fact_count: 0,
            hard_cannot_link_count: 0,
            action: CannotLinkAction::AllowMerge,
        },
    );
    assert_eq!(
        unsorted.validate(),
        Err(ExactBucketContractError::UnsortedSurfaceIds)
    );

    unsorted.membership.surface_ids.sort();
    unsorted.pair_expansion = "pairwise".to_string();
    assert_eq!(
        unsorted.validate(),
        Err(ExactBucketContractError::PairExpansionAllowed {
            actual: "pairwise".to_string()
        })
    );
}

#[test]
fn exact_bucket_cannot_link_hook_forces_solver_veto_instead_of_merge() {
    let bucket = sample_bucket(
        ExactBucketMembership {
            surface_ids: vec![
                "surf:cmbs_tenant_label:blake3:sears".to_string(),
                "surf:cmbs_tenant_label:blake3:sears_auto_center".to_string(),
            ],
            surface_ranges: vec![],
        },
        98,
        7,
        CannotLinkValidationHook {
            status: CannotLinkValidationStatus::CheckedConflictsPresent,
            checked_fact_count: 1,
            hard_cannot_link_count: 1,
            action: CannotLinkAction::RequireSolverVeto,
        },
    );

    bucket
        .validate()
        .expect("cannot-link veto bucket validates");
    assert!(bucket.requires_solver_cannot_link_veto());
    assert_eq!(bucket.expanded_pair_count(), 0);

    let mut invalid = bucket.clone();
    invalid.cannot_link_validation.action = CannotLinkAction::AllowMerge;
    assert_eq!(
        invalid.validate(),
        Err(ExactBucketContractError::CannotLinkAllowsMerge)
    );
}

fn sample_bucket(
    membership: ExactBucketMembership,
    row_count: u64,
    deal_count: u64,
    cannot_link_validation: CannotLinkValidationHook,
) -> ExactBucketAssertion {
    ExactBucketAssertion {
        version: CANON_ENTITY_BLOCK_BUCKET_VERSION.to_string(),
        bucket_id: "bucket:tenant_core:sears".to_string(),
        operator_id: "exact_view:tenant_core".to_string(),
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
        membership,
        row_count,
        deal_count,
        pair_expansion: EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN.to_string(),
        diagnostics: ExactBucketDiagnostics {
            largest_bucket_size: row_count,
            suppressed_pair_count: row_count.saturating_mul(row_count.saturating_sub(1)) / 2,
            labels: BTreeMap::from([("identity_view".to_string(), "tenant_core".to_string())]),
        },
        cannot_link_validation,
    }
}
