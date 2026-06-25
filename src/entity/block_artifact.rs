#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN: &str = "forbidden";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactBucketAssertion {
    pub version: String,
    pub bucket_id: String,
    pub operator_id: String,
    pub profile: ExactBucketProfile,
    pub upstream: ExactBucketUpstream,
    pub membership: ExactBucketMembership,
    pub row_count: u64,
    pub deal_count: u64,
    pub pair_expansion: String,
    pub diagnostics: ExactBucketDiagnostics,
    pub cannot_link_validation: CannotLinkValidationHook,
}

impl ExactBucketAssertion {
    pub fn validate(&self) -> Result<(), ExactBucketContractError> {
        if self.version != crate::entity::CANON_ENTITY_BLOCK_BUCKET_VERSION {
            return Err(ExactBucketContractError::WrongVersion {
                expected: crate::entity::CANON_ENTITY_BLOCK_BUCKET_VERSION,
                actual: self.version.clone(),
            });
        }
        if self.bucket_id.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField("bucket_id"));
        }
        if self.operator_id.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField("operator_id"));
        }
        self.profile.validate()?;
        self.upstream.validate()?;
        self.membership.validate()?;
        if self.row_count < self.membership.member_count() {
            return Err(ExactBucketContractError::RowCountBelowMembership {
                row_count: self.row_count,
                member_count: self.membership.member_count(),
            });
        }
        if self.pair_expansion != EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN {
            return Err(ExactBucketContractError::PairExpansionAllowed {
                actual: self.pair_expansion.clone(),
            });
        }
        self.cannot_link_validation.validate()?;
        Ok(())
    }

    pub fn expanded_pair_count(&self) -> u64 {
        0
    }

    pub fn theoretical_pair_count(&self) -> u64 {
        let member_count = self.membership.member_count();
        member_count.saturating_mul(member_count.saturating_sub(1)) / 2
    }

    pub fn artifact_membership_record_count(&self) -> u64 {
        self.membership.surface_ids.len() as u64 + self.membership.surface_ranges.len() as u64
    }

    pub fn requires_solver_cannot_link_veto(&self) -> bool {
        self.cannot_link_validation.hard_cannot_link_count > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactBucketProfile {
    pub id: String,
    pub version: String,
    pub identity_semantics: String,
    pub content_hash: String,
}

impl ExactBucketProfile {
    fn validate(&self) -> Result<(), ExactBucketContractError> {
        if self.id.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField("profile.id"));
        }
        if self.version.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField("profile.version"));
        }
        if self.identity_semantics.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField(
                "profile.identity_semantics",
            ));
        }
        if self.content_hash.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField(
                "profile.content_hash",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactBucketUpstream {
    pub prepare_hash: String,
    pub index_hash: String,
    pub strategy_hash: String,
    pub registry_snapshot_hash: String,
}

impl ExactBucketUpstream {
    fn validate(&self) -> Result<(), ExactBucketContractError> {
        for (field, value) in [
            ("upstream.prepare_hash", &self.prepare_hash),
            ("upstream.index_hash", &self.index_hash),
            ("upstream.strategy_hash", &self.strategy_hash),
            (
                "upstream.registry_snapshot_hash",
                &self.registry_snapshot_hash,
            ),
        ] {
            if value.trim().is_empty() {
                return Err(ExactBucketContractError::MissingField(field));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactBucketMembership {
    #[serde(default)]
    pub surface_ids: Vec<String>,
    #[serde(default)]
    pub surface_ranges: Vec<SurfaceIdRange>,
}

impl ExactBucketMembership {
    pub fn member_count(&self) -> u64 {
        self.surface_ids.len() as u64
            + self
                .surface_ranges
                .iter()
                .map(|range| range.member_count)
                .sum::<u64>()
    }

    fn validate(&self) -> Result<(), ExactBucketContractError> {
        if self.surface_ids.is_empty() && self.surface_ranges.is_empty() {
            return Err(ExactBucketContractError::EmptyMembership);
        }
        if self
            .surface_ids
            .iter()
            .any(|surface_id| surface_id.trim().is_empty())
        {
            return Err(ExactBucketContractError::EmptySurfaceId);
        }
        if self.surface_ids.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(ExactBucketContractError::UnsortedSurfaceIds);
        }
        for range in &self.surface_ranges {
            range.validate()?;
        }
        if self
            .surface_ranges
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            return Err(ExactBucketContractError::UnsortedSurfaceRanges);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SurfaceIdRange {
    pub start_surface_id: String,
    pub end_surface_id: String,
    pub member_count: u64,
}

impl SurfaceIdRange {
    fn validate(&self) -> Result<(), ExactBucketContractError> {
        if self.start_surface_id.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField(
                "surface_ranges.start_surface_id",
            ));
        }
        if self.end_surface_id.trim().is_empty() {
            return Err(ExactBucketContractError::MissingField(
                "surface_ranges.end_surface_id",
            ));
        }
        if self.start_surface_id > self.end_surface_id {
            return Err(ExactBucketContractError::InvalidSurfaceRange {
                start_surface_id: self.start_surface_id.clone(),
                end_surface_id: self.end_surface_id.clone(),
            });
        }
        if self.member_count == 0 {
            return Err(ExactBucketContractError::EmptySurfaceRange);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExactBucketDiagnostics {
    pub largest_bucket_size: u64,
    pub suppressed_pair_count: u64,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CannotLinkValidationHook {
    pub status: CannotLinkValidationStatus,
    pub checked_fact_count: u64,
    pub hard_cannot_link_count: u64,
    pub action: CannotLinkAction,
}

impl CannotLinkValidationHook {
    fn validate(&self) -> Result<(), ExactBucketContractError> {
        if self.hard_cannot_link_count > 0 && self.action == CannotLinkAction::AllowMerge {
            return Err(ExactBucketContractError::CannotLinkAllowsMerge);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CannotLinkValidationStatus {
    NotChecked,
    CheckedNoConflicts,
    CheckedConflictsPresent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CannotLinkAction {
    AllowMerge,
    RequireSolverVeto,
    RequireReview,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExactBucketContractError {
    WrongVersion {
        expected: &'static str,
        actual: String,
    },
    MissingField(&'static str),
    EmptyMembership,
    EmptySurfaceId,
    UnsortedSurfaceIds,
    UnsortedSurfaceRanges,
    InvalidSurfaceRange {
        start_surface_id: String,
        end_surface_id: String,
    },
    EmptySurfaceRange,
    RowCountBelowMembership {
        row_count: u64,
        member_count: u64,
    },
    PairExpansionAllowed {
        actual: String,
    },
    CannotLinkAllowsMerge,
}
