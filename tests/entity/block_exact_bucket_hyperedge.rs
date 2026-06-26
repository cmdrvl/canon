#![forbid(unsafe_code)]

use canon::entity::{
    block::{ExactBucketBlockRequest, ExactBucketSurface, emit_exact_bucket_hyperedges},
    block_artifact::{
        CannotLinkAction, CannotLinkValidationStatus, EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN,
        ExactBucketProfile, ExactBucketUpstream,
    },
};
use std::collections::BTreeSet;

#[test]
fn exact_bucket_hyperedge_block_emission() {
    let result = emit_exact_bucket_hyperedges(ExactBucketBlockRequest {
        profile: sample_profile(),
        upstream: sample_upstream(),
        operator_id: "exact_view:tenant_core".to_string(),
        identity_view: "tenant_core".to_string(),
        placeholder_values: BTreeSet::from(["0".to_string(), "vacant".to_string()]),
        surfaces: vec![
            ExactBucketSurface::new("surf:sears", "sears", 8_000, 3_000),
            ExactBucketSurface::new("surf:placeholder", "0", 12_000, 3_000),
        ],
    })
    .expect("exact bucket emits");

    assert_eq!(result.assertions.len(), 1);
    let assertion = &result.assertions[0];
    assertion.validate().expect("assertion validates");
    assert_eq!(assertion.bucket_id, "bucket:tenant_core:sears");
    assert_eq!(assertion.operator_id, "exact_view:tenant_core");
    assert_eq!(assertion.membership.surface_ids, ["surf:sears"]);
    assert_eq!(assertion.row_count, 8_000);
    assert_eq!(assertion.deal_count, 3_000);
    assert_eq!(
        assertion.pair_expansion,
        EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN
    );
    assert_eq!(assertion.expanded_pair_count(), 0);
    assert_eq!(assertion.artifact_membership_record_count(), 1);
    assert_eq!(assertion.diagnostics.labels["identity_view"], "tenant_core");
    assert_eq!(assertion.diagnostics.labels["bucket_value"], "sears");
    assert_eq!(
        assertion.diagnostics.suppressed_pair_count,
        8_000_u64 * 7_999 / 2
    );
    assert_eq!(
        assertion.cannot_link_validation.status,
        CannotLinkValidationStatus::NotChecked
    );
    assert_eq!(
        assertion.cannot_link_validation.action,
        CannotLinkAction::RequireReview
    );

    assert_eq!(result.diagnostics.emitted_bucket_count, 1);
    assert_eq!(result.diagnostics.excluded_placeholder_bucket_count, 1);
    assert_eq!(result.diagnostics.expanded_pair_count, 0);
    assert_eq!(
        result.diagnostics.suppressed_pair_count,
        8_000_u64 * 7_999 / 2
    );
    assert_eq!(result.diagnostics.largest_bucket_size, 8_000);
    assert_eq!(result.diagnostics.membership_record_count, 1);
}

#[test]
fn exact_bucket_no_on2() {
    let mut surfaces = Vec::new();
    for ordinal in 0..8_000 {
        surfaces.push(ExactBucketSurface::new(
            format!("surf:{ordinal:04}"),
            "oak plaza",
            1,
            1,
        ));
    }

    let result = emit_exact_bucket_hyperedges(ExactBucketBlockRequest {
        profile: sample_profile(),
        upstream: sample_upstream(),
        operator_id: "exact_view:tenant_core".to_string(),
        identity_view: "tenant_core".to_string(),
        placeholder_values: BTreeSet::new(),
        surfaces,
    })
    .expect("large exact bucket emits");

    assert_eq!(result.assertions.len(), 1);
    let assertion = &result.assertions[0];
    assert_eq!(assertion.membership.surface_ids.len(), 8_000);
    assert_eq!(assertion.artifact_membership_record_count(), 8_000);
    assert_eq!(assertion.theoretical_pair_count(), 31_996_000);
    assert_eq!(assertion.expanded_pair_count(), 0);
    assert_eq!(result.diagnostics.expanded_pair_count, 0);
    assert_eq!(result.diagnostics.membership_record_count, 8_000);
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
        strategy_hash: "blake3:strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
    }
}
