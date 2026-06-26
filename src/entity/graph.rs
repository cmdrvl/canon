#![forbid(unsafe_code)]

//! Compact signed evidence graph helpers for entity solving.
//!
//! Exact-bucket assertions enter solve as hyperedges. The graph stores the
//! bucket membership directly and checks hard cannot-link facts against that
//! membership without expanding the bucket into pairwise edges.

use crate::entity::block_artifact::ExactBucketAssertion;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityEvidenceGraph {
    pub exact_bucket_hyperedges: Vec<ExactBucketHyperedge>,
    pub hard_cannot_links: BTreeSet<SurfacePair>,
}

impl EntityEvidenceGraph {
    pub fn from_exact_bucket_assertions(assertions: &[ExactBucketAssertion]) -> Self {
        let mut exact_bucket_hyperedges = assertions
            .iter()
            .map(ExactBucketHyperedge::from_assertion)
            .collect::<Vec<_>>();
        exact_bucket_hyperedges.sort_by(|left, right| left.bucket_id.cmp(&right.bucket_id));
        Self {
            exact_bucket_hyperedges,
            hard_cannot_links: BTreeSet::new(),
        }
    }

    pub fn add_hard_cannot_link(
        &mut self,
        left_surface_id: impl Into<String>,
        right_surface_id: impl Into<String>,
    ) {
        if let Some(pair) = SurfacePair::new(left_surface_id, right_surface_id) {
            self.hard_cannot_links.insert(pair);
        }
    }

    pub fn solve_exact_bucket_hyperedges(&self) -> ExactBucketSolveReport {
        let mut decisions = self
            .exact_bucket_hyperedges
            .iter()
            .map(|hyperedge| self.solve_hyperedge(hyperedge))
            .collect::<Vec<_>>();
        decisions.sort_by(|left, right| left.bucket_id.cmp(&right.bucket_id));

        ExactBucketSolveReport {
            hyperedge_count: decisions.len() as u64,
            expanded_pair_count: 0,
            membership_record_count: self
                .exact_bucket_hyperedges
                .iter()
                .map(|hyperedge| hyperedge.membership_record_count)
                .sum(),
            theoretical_pair_count: self
                .exact_bucket_hyperedges
                .iter()
                .map(|hyperedge| hyperedge.theoretical_pair_count)
                .sum(),
            decisions,
        }
    }

    fn solve_hyperedge(&self, hyperedge: &ExactBucketHyperedge) -> ExactBucketSolveDecision {
        let hard_cannot_links = self
            .hard_cannot_links
            .iter()
            .filter(|pair| hyperedge.contains_pair(pair))
            .cloned()
            .collect::<Vec<_>>();
        let hard_cannot_link_count = hard_cannot_links.len() as u64;
        let (action, reason) = if hard_cannot_link_count == 0 {
            (
                ExactBucketSolveAction::MergeCluster,
                "exact_bucket_cluster_evidence",
            )
        } else {
            (
                ExactBucketSolveAction::ReviewContradiction,
                "hard_cannot_link_inside_exact_bucket",
            )
        };

        ExactBucketSolveDecision {
            bucket_id: hyperedge.bucket_id.clone(),
            action,
            reason: reason.to_string(),
            member_count: hyperedge.member_count,
            membership_record_count: hyperedge.membership_record_count,
            expanded_pair_count: 0,
            hard_cannot_link_count,
            hard_cannot_links,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactBucketHyperedge {
    pub bucket_id: String,
    pub operator_id: String,
    pub member_count: u64,
    pub membership_record_count: u64,
    pub theoretical_pair_count: u64,
    pub expanded_pair_count: u64,
    pub explicit_surface_ids: BTreeSet<String>,
}

impl ExactBucketHyperedge {
    fn from_assertion(assertion: &ExactBucketAssertion) -> Self {
        Self {
            bucket_id: assertion.bucket_id.clone(),
            operator_id: assertion.operator_id.clone(),
            member_count: assertion.membership.member_count(),
            membership_record_count: assertion.artifact_membership_record_count(),
            theoretical_pair_count: assertion.theoretical_pair_count(),
            expanded_pair_count: assertion.expanded_pair_count(),
            explicit_surface_ids: assertion
                .membership
                .surface_ids
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
        }
    }

    fn contains_pair(&self, pair: &SurfacePair) -> bool {
        self.explicit_surface_ids.contains(&pair.left_surface_id)
            && self.explicit_surface_ids.contains(&pair.right_surface_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SurfacePair {
    pub left_surface_id: String,
    pub right_surface_id: String,
}

impl SurfacePair {
    pub fn new(
        left_surface_id: impl Into<String>,
        right_surface_id: impl Into<String>,
    ) -> Option<Self> {
        let left_surface_id = left_surface_id.into();
        let right_surface_id = right_surface_id.into();
        if left_surface_id == right_surface_id {
            return None;
        }
        if left_surface_id < right_surface_id {
            Some(Self {
                left_surface_id,
                right_surface_id,
            })
        } else {
            Some(Self {
                left_surface_id: right_surface_id,
                right_surface_id: left_surface_id,
            })
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExactBucketSolveAction {
    MergeCluster,
    ReviewContradiction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactBucketSolveDecision {
    pub bucket_id: String,
    pub action: ExactBucketSolveAction,
    pub reason: String,
    pub member_count: u64,
    pub membership_record_count: u64,
    pub expanded_pair_count: u64,
    pub hard_cannot_link_count: u64,
    pub hard_cannot_links: Vec<SurfacePair>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactBucketSolveReport {
    pub hyperedge_count: u64,
    pub expanded_pair_count: u64,
    pub membership_record_count: u64,
    pub theoretical_pair_count: u64,
    pub decisions: Vec<ExactBucketSolveDecision>,
}
