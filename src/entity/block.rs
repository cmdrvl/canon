#![forbid(unsafe_code)]

use crate::entity::{
    CANON_ENTITY_BLOCK_BUCKET_VERSION,
    block_artifact::{
        CannotLinkAction, CannotLinkValidationHook, CannotLinkValidationStatus,
        EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN, ExactBucketAssertion, ExactBucketContractError,
        ExactBucketDiagnostics, ExactBucketMembership, ExactBucketProfile, ExactBucketUpstream,
    },
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactBucketBlockRequest {
    pub profile: ExactBucketProfile,
    pub upstream: ExactBucketUpstream,
    pub operator_id: String,
    pub identity_view: String,
    pub placeholder_values: BTreeSet<String>,
    pub surfaces: Vec<ExactBucketSurface>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactBucketSurface {
    pub surface_id: String,
    pub bucket_value: String,
    pub row_count: u64,
    pub deal_count: u64,
}

impl ExactBucketSurface {
    pub fn new(
        surface_id: impl Into<String>,
        bucket_value: impl Into<String>,
        row_count: u64,
        deal_count: u64,
    ) -> Self {
        Self {
            surface_id: surface_id.into(),
            bucket_value: bucket_value.into(),
            row_count,
            deal_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactBucketBlockResult {
    pub assertions: Vec<ExactBucketAssertion>,
    pub diagnostics: ExactBucketBlockDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExactBucketBlockDiagnostics {
    pub emitted_bucket_count: u64,
    pub excluded_placeholder_bucket_count: u64,
    pub expanded_pair_count: u64,
    pub suppressed_pair_count: u64,
    pub largest_bucket_size: u64,
    pub membership_record_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExactBucketEmissionError {
    Contract(ExactBucketContractError),
}

pub fn emit_exact_bucket_hyperedges(
    request: ExactBucketBlockRequest,
) -> Result<ExactBucketBlockResult, ExactBucketEmissionError> {
    let mut groups = BTreeMap::<String, ExactBucketGroup>::new();
    let mut excluded_placeholder_values = BTreeSet::<String>::new();

    for surface in request.surfaces {
        let bucket_value = surface.bucket_value.trim();
        if bucket_value.is_empty() {
            continue;
        }
        if request.placeholder_values.contains(bucket_value) {
            excluded_placeholder_values.insert(bucket_value.to_string());
            continue;
        }
        let group = groups.entry(bucket_value.to_string()).or_default();
        group.surface_ids.insert(surface.surface_id);
        group.row_count = group.row_count.saturating_add(surface.row_count);
        group.deal_count = group.deal_count.saturating_add(surface.deal_count);
    }

    let mut diagnostics = ExactBucketBlockDiagnostics {
        excluded_placeholder_bucket_count: excluded_placeholder_values.len() as u64,
        ..ExactBucketBlockDiagnostics::default()
    };
    let mut assertions = Vec::with_capacity(groups.len());

    for (bucket_value, group) in groups {
        let surface_ids = group.surface_ids.into_iter().collect::<Vec<_>>();
        let suppressed_pair_count = suppressed_pair_count(group.row_count);
        let assertion = ExactBucketAssertion {
            version: CANON_ENTITY_BLOCK_BUCKET_VERSION.to_string(),
            bucket_id: format!("bucket:{}:{bucket_value}", request.identity_view),
            operator_id: request.operator_id.clone(),
            profile: request.profile.clone(),
            upstream: request.upstream.clone(),
            membership: ExactBucketMembership {
                surface_ids,
                surface_ranges: Vec::new(),
            },
            row_count: group.row_count,
            deal_count: group.deal_count,
            pair_expansion: EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN.to_string(),
            diagnostics: ExactBucketDiagnostics {
                largest_bucket_size: group.row_count,
                suppressed_pair_count,
                labels: BTreeMap::from([
                    ("identity_view".to_string(), request.identity_view.clone()),
                    ("bucket_value".to_string(), bucket_value),
                ]),
            },
            cannot_link_validation: CannotLinkValidationHook {
                status: CannotLinkValidationStatus::NotChecked,
                checked_fact_count: 0,
                hard_cannot_link_count: 0,
                action: CannotLinkAction::RequireReview,
            },
        };
        assertion
            .validate()
            .map_err(ExactBucketEmissionError::Contract)?;

        diagnostics.emitted_bucket_count += 1;
        diagnostics.expanded_pair_count += assertion.expanded_pair_count();
        diagnostics.suppressed_pair_count = diagnostics
            .suppressed_pair_count
            .saturating_add(suppressed_pair_count);
        diagnostics.largest_bucket_size = diagnostics.largest_bucket_size.max(assertion.row_count);
        diagnostics.membership_record_count = diagnostics
            .membership_record_count
            .saturating_add(assertion.artifact_membership_record_count());
        assertions.push(assertion);
    }

    Ok(ExactBucketBlockResult {
        assertions,
        diagnostics,
    })
}

fn suppressed_pair_count(row_count: u64) -> u64 {
    row_count.saturating_mul(row_count.saturating_sub(1)) / 2
}

#[derive(Debug, Clone, Default)]
struct ExactBucketGroup {
    surface_ids: BTreeSet<String>,
    row_count: u64,
    deal_count: u64,
}
