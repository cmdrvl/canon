#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fmt};

pub const CANON_PROJECT_STATE_VERSION: &str = "canon.project.state.v1";

pub fn project_state_schema_version() -> &'static str {
    CANON_PROJECT_STATE_VERSION
}

pub type ProjectStateResult<T> = Result<T, ProjectStateError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLifecycleState {
    Planned,
    EvidenceReady,
    ReviewRequired,
    Audited,
    Promotable,
    Promoted,
    ReplayVerified,
    Exported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStateErrorCode {
    ArtifactContract,
    StaleReview,
    StaleAudit,
    StalePromotion,
    StaleReplay,
    StaleExport,
    RegistryRace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectStateError {
    pub code: ProjectStateErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_command: Option<String>,
}

impl ProjectStateError {
    pub fn new(code: ProjectStateErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            next_command: None,
        }
    }

    pub fn with_next_command(
        code: ProjectStateErrorCode,
        message: impl Into<String>,
        next_command: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            next_command: Some(next_command.into()),
        }
    }
}

impl fmt::Display for ProjectStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for ProjectStateError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLifecycleReceiptKind {
    Run,
    Review,
    Audit,
    PromotionPreview,
    Promotion,
    Replay,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLifecycleBlockerCode {
    EvidenceNotReady,
    ReviewNotExported,
    ReviewPending,
    ReviewRejected,
    AuditMissing,
    AuditRejected,
    PromotionPreviewMissing,
    PromotionApprovalRequired,
    PromotionMissing,
    ReplayMissing,
    ReplayFailed,
    ExportMissing,
    ExportPartial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLifecycleBinding {
    pub project_id: String,
    pub plan_graph_hash: String,
    pub run_receipt_hash: String,
    pub registry_digest: String,
    pub policy_digest: String,
    pub strategy_digest: String,
}

impl ProjectLifecycleBinding {
    pub fn new(
        project_id: impl Into<String>,
        plan_graph_hash: impl Into<String>,
        run_receipt_hash: impl Into<String>,
        registry_digest: impl Into<String>,
        policy_digest: impl Into<String>,
        strategy_digest: impl Into<String>,
    ) -> Self {
        Self {
            project_id: project_id.into(),
            plan_graph_hash: plan_graph_hash.into(),
            run_receipt_hash: run_receipt_hash.into(),
            registry_digest: registry_digest.into(),
            policy_digest: policy_digest.into(),
            strategy_digest: strategy_digest.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectReviewReceipt {
    pub receipt_id: String,
    pub binding: ProjectLifecycleBinding,
    pub review_bundle_hash: String,
    pub decision_hash: String,
    pub pending_decisions: u64,
    pub accepted_decisions: u64,
    pub rejected_decisions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAuditReceipt {
    pub receipt_id: String,
    pub binding: ProjectLifecycleBinding,
    pub audit_hash: String,
    pub reviewed_decision_hash: String,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectMutationPreview {
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub intended_paths: Vec<String>,
    pub version_change: String,
    pub requires_explicit_execution: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPromotionReceipt {
    pub receipt_id: String,
    pub binding: ProjectLifecycleBinding,
    pub promotion_hash: String,
    pub review_decision_hash: String,
    pub audit_hash: String,
    pub before_registry_digest: String,
    pub after_registry_digest: String,
    pub mutation_preview: ProjectMutationPreview,
    pub executed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectReplayReceipt {
    pub receipt_id: String,
    pub binding: ProjectLifecycleBinding,
    pub replay_hash: String,
    pub promoted_registry_digest: String,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectExportReceipt {
    pub receipt_id: String,
    pub binding: ProjectLifecycleBinding,
    pub output_id: String,
    pub output_digest: String,
    pub promoted_registry_digest: String,
    pub replay_hash: String,
    pub partial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCompletedReceipt {
    pub kind: ProjectLifecycleReceiptKind,
    pub receipt_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLifecycleBlocker {
    pub code: ProjectLifecycleBlockerCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_command: Option<String>,
}

impl ProjectLifecycleBlocker {
    pub fn new(
        code: ProjectLifecycleBlockerCode,
        message: impl Into<String>,
        next_command: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            next_command: Some(next_command.into()),
        }
    }

    pub fn without_next_command(
        code: ProjectLifecycleBlockerCode,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            next_command: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLifecycleReport {
    pub schema_version: String,
    pub project_id: String,
    pub state: ProjectLifecycleState,
    pub binding: ProjectLifecycleBinding,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<ProjectLifecycleBlocker>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed_receipts: Vec<ProjectCompletedReceipt>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub next_commands: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mutation_previews: Vec<ProjectMutationPreview>,
}

pub fn completed_receipt(
    kind: ProjectLifecycleReceiptKind,
    receipt_id: impl Into<String>,
) -> ProjectCompletedReceipt {
    ProjectCompletedReceipt {
        kind,
        receipt_id: receipt_id.into(),
        output_id: None,
    }
}

pub fn completed_export_receipt(
    receipt_id: impl Into<String>,
    output_id: impl Into<String>,
) -> ProjectCompletedReceipt {
    ProjectCompletedReceipt {
        kind: ProjectLifecycleReceiptKind::Export,
        receipt_id: receipt_id.into(),
        output_id: Some(output_id.into()),
    }
}
