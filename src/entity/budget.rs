#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetStage {
    Index,
    Block,
    Edge,
    Solve,
    Review,
    Apply,
    AllLargeStages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetLimit {
    MaxPostingListEntries,
    MaxCandidatesPerSurface,
    MaxCandidatesPerOperator,
    MaxCandidatesPerRun,
    MaxExactBucketSize,
    MaxEdgeRecords,
    MaxComponentSize,
    MaxReviewGroups,
    MaxArtifactBytes,
    MaxRows,
    MaxBytes,
    RequireFullResolutionApply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetEnforcement {
    RefuseBeforeEmission,
    RefuseBeforeScoring,
    BoundedAbstention,
    RefuseBeforeOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityBudgetRefusalCode {
    #[serde(rename = "E_ENTITY_INDEX_LIMIT")]
    IndexLimit,
    #[serde(rename = "E_ENTITY_CANDIDATE_BUDGET")]
    CandidateBudget,
    #[serde(rename = "E_ENTITY_ARTIFACT_CONTRACT")]
    ArtifactContract,
    #[serde(rename = "E_ENTITY_APPLY_UNRESOLVED")]
    ApplyUnresolved,
    #[serde(rename = "E_ENTITY_IO_BUDGET")]
    IoBudget,
}

impl EntityBudgetRefusalCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IndexLimit => "E_ENTITY_INDEX_LIMIT",
            Self::CandidateBudget => "E_ENTITY_CANDIDATE_BUDGET",
            Self::ArtifactContract => "E_ENTITY_ARTIFACT_CONTRACT",
            Self::ApplyUnresolved => "E_ENTITY_APPLY_UNRESOLVED",
            Self::IoBudget => "E_ENTITY_IO_BUDGET",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetPolicy {
    pub id: &'static str,
    pub stage: BudgetStage,
    pub limit: BudgetLimit,
    pub enforcement: BudgetEnforcement,
    pub refusal_code: EntityBudgetRefusalCode,
    pub next_command: &'static str,
}

impl BudgetPolicy {
    pub const fn new(
        id: &'static str,
        stage: BudgetStage,
        limit: BudgetLimit,
        enforcement: BudgetEnforcement,
        refusal_code: EntityBudgetRefusalCode,
        next_command: &'static str,
    ) -> Self {
        Self {
            id,
            stage,
            limit,
            enforcement,
            refusal_code,
            next_command,
        }
    }

    pub fn breach(&self, observed: u64, configured: u64) -> BudgetBreach {
        BudgetBreach {
            policy_id: self.id,
            stage: self.stage,
            limit: self.limit,
            enforcement: self.enforcement,
            refusal_code: self.refusal_code,
            observed,
            configured,
            next_command: self.next_command,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetBreach {
    pub policy_id: &'static str,
    pub stage: BudgetStage,
    pub limit: BudgetLimit,
    pub enforcement: BudgetEnforcement,
    pub refusal_code: EntityBudgetRefusalCode,
    pub observed: u64,
    pub configured: u64,
    pub next_command: &'static str,
}

pub const DEFAULT_BUDGET_POLICIES: &[BudgetPolicy] = &[
    BudgetPolicy::new(
        "index.max_posting_list_entries",
        BudgetStage::Index,
        BudgetLimit::MaxPostingListEntries,
        BudgetEnforcement::RefuseBeforeEmission,
        EntityBudgetRefusalCode::IndexLimit,
        "Tighten the blocking strategy, increase the explicit posting cap, or review the large posting list before rerunning canon entity index build",
    ),
    BudgetPolicy::new(
        "block.max_candidates_per_surface",
        BudgetStage::Block,
        BudgetLimit::MaxCandidatesPerSurface,
        BudgetEnforcement::RefuseBeforeEmission,
        EntityBudgetRefusalCode::CandidateBudget,
        "Lower-yield blocking operators, increase the explicit per-surface cap, or route the large neighborhood to grouped review before rerunning canon entity block",
    ),
    BudgetPolicy::new(
        "block.max_candidates_per_operator",
        BudgetStage::Block,
        BudgetLimit::MaxCandidatesPerOperator,
        BudgetEnforcement::RefuseBeforeEmission,
        EntityBudgetRefusalCode::CandidateBudget,
        "Tighten or disable the over-broad blocking operator, then rerun canon entity block",
    ),
    BudgetPolicy::new(
        "block.max_candidates_per_run",
        BudgetStage::Block,
        BudgetLimit::MaxCandidatesPerRun,
        BudgetEnforcement::RefuseBeforeEmission,
        EntityBudgetRefusalCode::CandidateBudget,
        "Increase the explicit run candidate budget or split the physical input while preserving the global prepared surface view",
    ),
    BudgetPolicy::new(
        "block.max_exact_bucket_size",
        BudgetStage::Block,
        BudgetLimit::MaxExactBucketSize,
        BudgetEnforcement::RefuseBeforeEmission,
        EntityBudgetRefusalCode::IndexLimit,
        "Emit a compact exact-bucket assertion or mark the bucket for review; never expand it into pairwise candidates",
    ),
    BudgetPolicy::new(
        "edge.max_edge_records",
        BudgetStage::Edge,
        BudgetLimit::MaxEdgeRecords,
        BudgetEnforcement::RefuseBeforeScoring,
        EntityBudgetRefusalCode::ArtifactContract,
        "Validate candidate caps and stale block artifacts before rerunning canon entity edge",
    ),
    BudgetPolicy::new(
        "solve.max_component_size",
        BudgetStage::Solve,
        BudgetLimit::MaxComponentSize,
        BudgetEnforcement::BoundedAbstention,
        EntityBudgetRefusalCode::ArtifactContract,
        "Keep the oversized component in escrow or explicitly configure a larger solve component cap",
    ),
    BudgetPolicy::new(
        "review.max_review_groups",
        BudgetStage::Review,
        BudgetLimit::MaxReviewGroups,
        BudgetEnforcement::BoundedAbstention,
        EntityBudgetRefusalCode::ArtifactContract,
        "Tighten strategy thresholds or export grouped review queues with an explicit review-group waiver",
    ),
    BudgetPolicy::new(
        "all_large_stages.max_artifact_bytes",
        BudgetStage::AllLargeStages,
        BudgetLimit::MaxArtifactBytes,
        BudgetEnforcement::RefuseBeforeOutput,
        EntityBudgetRefusalCode::IoBudget,
        "Increase the explicit artifact byte budget or reduce the input/candidate surface before rerunning",
    ),
    BudgetPolicy::new(
        "all_large_stages.max_rows",
        BudgetStage::AllLargeStages,
        BudgetLimit::MaxRows,
        BudgetEnforcement::RefuseBeforeEmission,
        EntityBudgetRefusalCode::IoBudget,
        "Increase --max-rows or process physical batches while preserving one global prepared surface/index view",
    ),
    BudgetPolicy::new(
        "all_large_stages.max_bytes",
        BudgetStage::AllLargeStages,
        BudgetLimit::MaxBytes,
        BudgetEnforcement::RefuseBeforeEmission,
        EntityBudgetRefusalCode::IoBudget,
        "Increase --max-bytes or reduce input size before rerunning the same canon entity command",
    ),
    BudgetPolicy::new(
        "apply.require_full_resolution",
        BudgetStage::Apply,
        BudgetLimit::RequireFullResolutionApply,
        BudgetEnforcement::RefuseBeforeOutput,
        EntityBudgetRefusalCode::ApplyUnresolved,
        "Promote more exact aliases or rerun canon entity apply without full-resolution mode",
    ),
];

pub fn default_budget_policies() -> &'static [BudgetPolicy] {
    DEFAULT_BUDGET_POLICIES
}

pub fn find_budget_policy(stage: BudgetStage, limit: BudgetLimit) -> Option<&'static BudgetPolicy> {
    DEFAULT_BUDGET_POLICIES
        .iter()
        .find(|policy| policy.stage == stage && policy.limit == limit)
}
