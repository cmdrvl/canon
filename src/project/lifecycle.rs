#![forbid(unsafe_code)]

use super::{
    plan::{ProjectPlan, ProjectPlanNodeClass, ProjectPlanNodeKind},
    receipt::ProjectRunReceipt,
    state::{
        CANON_PROJECT_STATE_VERSION, ProjectAuditReceipt, ProjectCompletedReceipt,
        ProjectExportReceipt, ProjectLifecycleBinding, ProjectLifecycleBlocker,
        ProjectLifecycleBlockerCode, ProjectLifecycleReceiptKind, ProjectLifecycleReport,
        ProjectLifecycleState, ProjectMutationPreview, ProjectPromotionReceipt,
        ProjectReplayReceipt, ProjectReviewReceipt, ProjectStateError, ProjectStateErrorCode,
        ProjectStateResult, completed_export_receipt, completed_receipt,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLifecycleRequest {
    pub plan: ProjectPlan,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_receipt: Option<ProjectRunReceipt>,
    pub registry_digest: String,
    pub policy_digest: String,
    pub strategy_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review: Option<ProjectReviewReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit: Option<ProjectAuditReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promotion: Option<ProjectPromotionReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay: Option<ProjectReplayReceipt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<ProjectExportReceipt>,
}

impl ProjectLifecycleRequest {
    pub fn new(
        plan: ProjectPlan,
        run_receipt: Option<ProjectRunReceipt>,
        registry_digest: impl Into<String>,
        policy_digest: impl Into<String>,
        strategy_digest: impl Into<String>,
    ) -> Self {
        Self {
            plan,
            run_receipt,
            registry_digest: registry_digest.into(),
            policy_digest: policy_digest.into(),
            strategy_digest: strategy_digest.into(),
            review: None,
            audit: None,
            promotion: None,
            replay: None,
            exports: Vec::new(),
        }
    }
}

pub fn lifecycle_binding_for_plan_run(
    plan: &ProjectPlan,
    run_receipt: &ProjectRunReceipt,
    registry_digest: impl Into<String>,
    policy_digest: impl Into<String>,
    strategy_digest: impl Into<String>,
) -> ProjectLifecycleBinding {
    ProjectLifecycleBinding::new(
        plan.project_id.clone(),
        plan.graph_hash.clone(),
        run_receipt.receipt_hash.clone(),
        registry_digest,
        policy_digest,
        strategy_digest,
    )
}

pub fn evaluate_project_lifecycle(
    request: ProjectLifecycleRequest,
) -> ProjectStateResult<ProjectLifecycleReport> {
    let binding = match &request.run_receipt {
        Some(run) => lifecycle_binding_for_plan_run(
            &request.plan,
            run,
            request.registry_digest.clone(),
            request.policy_digest.clone(),
            request.strategy_digest.clone(),
        ),
        None => ProjectLifecycleBinding::new(
            request.plan.project_id.clone(),
            request.plan.graph_hash.clone(),
            "",
            request.registry_digest.clone(),
            request.policy_digest.clone(),
            request.strategy_digest.clone(),
        ),
    };

    validate_request_shape(&request, &binding)?;

    let mut completed_receipts = Vec::new();
    let mut mutation_previews = Vec::new();
    let mut next_commands = BTreeMap::new();
    let mut blockers = Vec::new();

    if let Some(run) = &request.run_receipt {
        completed_receipts.push(completed_receipt(
            ProjectLifecycleReceiptKind::Run,
            run.receipt_hash.clone(),
        ));
    }

    let state = if !evidence_is_ready(&request) {
        blockers.push(ProjectLifecycleBlocker::new(
            ProjectLifecycleBlockerCode::EvidenceNotReady,
            "project evidence nodes have not all completed successfully",
            "canon project run --plan <PLAN>",
        ));
        next_commands.insert(
            "run".to_string(),
            "canon project run --plan <PLAN>".to_string(),
        );
        ProjectLifecycleState::Planned
    } else if request.review.is_none() {
        blockers.push(ProjectLifecycleBlocker::new(
            ProjectLifecycleBlockerCode::ReviewNotExported,
            "evidence is ready but no bound review bundle has been exported",
            "canon project review export --plan <PLAN> --run <RUN>",
        ));
        next_commands.insert(
            "review".to_string(),
            "canon project review export --plan <PLAN> --run <RUN>".to_string(),
        );
        ProjectLifecycleState::EvidenceReady
    } else {
        let review = request.review.as_ref().expect("review checked");
        completed_receipts.push(completed_receipt(
            ProjectLifecycleReceiptKind::Review,
            review.receipt_id.clone(),
        ));

        if review.pending_decisions > 0 {
            blockers.push(ProjectLifecycleBlocker::new(
                ProjectLifecycleBlockerCode::ReviewPending,
                format!(
                    "{} review decisions remain pending",
                    review.pending_decisions
                ),
                "canon project review import --plan <PLAN> --review <REVIEW>",
            ));
            next_commands.insert(
                "review".to_string(),
                "canon project review import --plan <PLAN> --review <REVIEW>".to_string(),
            );
            ProjectLifecycleState::ReviewRequired
        } else if review.accepted_decisions == 0 {
            blockers.push(ProjectLifecycleBlocker::new(
                ProjectLifecycleBlockerCode::ReviewRejected,
                "review contains no accepted decisions to promote",
                "canon project review export --plan <PLAN> --include review",
            ));
            next_commands.insert(
                "review".to_string(),
                "canon project review export --plan <PLAN> --include review".to_string(),
            );
            ProjectLifecycleState::ReviewRequired
        } else if request.audit.is_none() {
            blockers.push(ProjectLifecycleBlocker::new(
                ProjectLifecycleBlockerCode::AuditMissing,
                "accepted review decisions need an audit receipt before promotion",
                "canon project audit --plan <PLAN> --review <REVIEW>",
            ));
            next_commands.insert(
                "audit".to_string(),
                "canon project audit --plan <PLAN> --review <REVIEW>".to_string(),
            );
            ProjectLifecycleState::ReviewRequired
        } else {
            audited_state(
                request,
                &mut completed_receipts,
                &mut mutation_previews,
                &mut next_commands,
                &mut blockers,
            )?
        }
    };

    Ok(ProjectLifecycleReport {
        schema_version: CANON_PROJECT_STATE_VERSION.to_string(),
        project_id: binding.project_id.clone(),
        state,
        binding,
        blockers,
        completed_receipts,
        next_commands,
        mutation_previews,
    })
}

fn audited_state(
    request: ProjectLifecycleRequest,
    completed_receipts: &mut Vec<ProjectCompletedReceipt>,
    mutation_previews: &mut Vec<ProjectMutationPreview>,
    next_commands: &mut BTreeMap<String, String>,
    blockers: &mut Vec<ProjectLifecycleBlocker>,
) -> ProjectStateResult<ProjectLifecycleState> {
    let audit = request.audit.as_ref().expect("audit checked");
    completed_receipts.push(completed_receipt(
        ProjectLifecycleReceiptKind::Audit,
        audit.receipt_id.clone(),
    ));
    if !audit.passed {
        blockers.push(ProjectLifecycleBlocker::new(
            ProjectLifecycleBlockerCode::AuditRejected,
            "audit receipt rejected the reviewed decisions",
            "canon project review export --plan <PLAN> --include rejected",
        ));
        next_commands.insert(
            "review".to_string(),
            "canon project review export --plan <PLAN> --include rejected".to_string(),
        );
        return Ok(ProjectLifecycleState::Audited);
    }

    let Some(promotion) = request.promotion.as_ref() else {
        blockers.push(ProjectLifecycleBlocker::new(
            ProjectLifecycleBlockerCode::PromotionPreviewMissing,
            "audit passed but no mutation preview has been produced",
            "canon project promote --plan <PLAN> --dry-run",
        ));
        next_commands.insert(
            "promote_preview".to_string(),
            "canon project promote --plan <PLAN> --dry-run".to_string(),
        );
        return Ok(ProjectLifecycleState::Audited);
    };

    if !promotion.executed {
        completed_receipts.push(completed_receipt(
            ProjectLifecycleReceiptKind::PromotionPreview,
            promotion.receipt_id.clone(),
        ));
        mutation_previews.push(promotion.mutation_preview.clone());
        blockers.push(ProjectLifecycleBlocker::new(
            ProjectLifecycleBlockerCode::PromotionApprovalRequired,
            "promotion is promotable but still requires explicit execution",
            promotion.mutation_preview.command.clone(),
        ));
        next_commands.insert(
            "promote".to_string(),
            promotion.mutation_preview.command.clone(),
        );
        return Ok(ProjectLifecycleState::Promotable);
    }

    completed_receipts.push(completed_receipt(
        ProjectLifecycleReceiptKind::Promotion,
        promotion.receipt_id.clone(),
    ));

    let Some(replay) = request.replay.as_ref() else {
        blockers.push(ProjectLifecycleBlocker::new(
            ProjectLifecycleBlockerCode::ReplayMissing,
            "promoted registry has not been replay-verified",
            "canon project apply --plan <PLAN> --registry <PROMOTED_REGISTRY>",
        ));
        next_commands.insert(
            "apply".to_string(),
            "canon project apply --plan <PLAN> --registry <PROMOTED_REGISTRY>".to_string(),
        );
        return Ok(ProjectLifecycleState::Promoted);
    };

    completed_receipts.push(completed_receipt(
        ProjectLifecycleReceiptKind::Replay,
        replay.receipt_id.clone(),
    ));
    if !replay.passed {
        blockers.push(ProjectLifecycleBlocker::new(
            ProjectLifecycleBlockerCode::ReplayFailed,
            "exact replay did not verify promoted registry descendants",
            "canon project apply --plan <PLAN> --registry <PROMOTED_REGISTRY>",
        ));
        next_commands.insert(
            "apply".to_string(),
            "canon project apply --plan <PLAN> --registry <PROMOTED_REGISTRY>".to_string(),
        );
        return Ok(ProjectLifecycleState::Promoted);
    }

    let missing_exports = missing_export_outputs(&request);
    let partial_exports = request
        .exports
        .iter()
        .filter(|export| export.partial)
        .collect::<Vec<_>>();
    for export in &request.exports {
        completed_receipts.push(completed_export_receipt(
            export.receipt_id.clone(),
            export.output_id.clone(),
        ));
    }

    if !partial_exports.is_empty() {
        blockers.push(ProjectLifecycleBlocker::new(
            ProjectLifecycleBlockerCode::ExportPartial,
            "one or more exports are partial and must be regenerated",
            "canon project export --plan <PLAN>",
        ));
        next_commands.insert(
            "export".to_string(),
            "canon project export --plan <PLAN>".to_string(),
        );
        return Ok(ProjectLifecycleState::ReplayVerified);
    }
    if !missing_exports.is_empty() {
        blockers.push(ProjectLifecycleBlocker::new(
            ProjectLifecycleBlockerCode::ExportMissing,
            format!("missing exports: {}", missing_exports.join(", ")),
            "canon project export --plan <PLAN>",
        ));
        next_commands.insert(
            "export".to_string(),
            "canon project export --plan <PLAN>".to_string(),
        );
        return Ok(ProjectLifecycleState::ReplayVerified);
    }

    Ok(ProjectLifecycleState::Exported)
}

fn validate_request_shape(
    request: &ProjectLifecycleRequest,
    expected: &ProjectLifecycleBinding,
) -> ProjectStateResult<()> {
    if request.plan.project_id.trim().is_empty() || request.plan.graph_hash.trim().is_empty() {
        return Err(ProjectStateError::new(
            ProjectStateErrorCode::ArtifactContract,
            "project plan must carry a project_id and graph_hash",
        ));
    }
    if request.run_receipt.as_ref().is_some_and(|run| {
        run.project_id != request.plan.project_id || run.plan_graph_hash != request.plan.graph_hash
    }) {
        return Err(ProjectStateError::with_next_command(
            ProjectStateErrorCode::ArtifactContract,
            "run receipt is not for this project plan",
            "canon project run --plan <PLAN>",
        ));
    }
    if let Some(review) = &request.review {
        ensure_binding(
            &review.binding,
            expected,
            ProjectStateErrorCode::StaleReview,
            "review bundle was produced from a different run, registry, policy, or strategy",
        )?;
    }
    if let Some(audit) = &request.audit {
        ensure_binding(
            &audit.binding,
            expected,
            ProjectStateErrorCode::StaleAudit,
            "audit receipt was produced from a different run, registry, policy, or strategy",
        )?;
        if request
            .review
            .as_ref()
            .is_some_and(|review| audit.reviewed_decision_hash != review.decision_hash)
        {
            return Err(ProjectStateError::with_next_command(
                ProjectStateErrorCode::StaleAudit,
                "audit receipt does not cover the imported review decisions",
                "canon project audit --plan <PLAN> --review <REVIEW>",
            ));
        }
    }
    if let Some(promotion) = &request.promotion {
        ensure_binding(
            &promotion.binding,
            expected,
            ProjectStateErrorCode::StalePromotion,
            "promotion receipt was produced from a different run, registry, policy, or strategy",
        )?;
        if promotion.before_registry_digest != expected.registry_digest {
            return Err(ProjectStateError::with_next_command(
                ProjectStateErrorCode::RegistryRace,
                "promotion before_registry_digest no longer matches the current registry snapshot",
                "canon project status --plan <PLAN> --refresh-registry",
            ));
        }
        if request
            .review
            .as_ref()
            .is_some_and(|review| promotion.review_decision_hash != review.decision_hash)
        {
            return Err(ProjectStateError::with_next_command(
                ProjectStateErrorCode::StalePromotion,
                "promotion receipt does not consume the imported review decisions",
                "canon project promote --plan <PLAN> --dry-run",
            ));
        }
        if request
            .audit
            .as_ref()
            .is_some_and(|audit| promotion.audit_hash != audit.audit_hash)
        {
            return Err(ProjectStateError::with_next_command(
                ProjectStateErrorCode::StalePromotion,
                "promotion receipt does not consume the current audit receipt",
                "canon project promote --plan <PLAN> --dry-run",
            ));
        }
    }
    if let Some(replay) = &request.replay {
        ensure_binding(
            &replay.binding,
            expected,
            ProjectStateErrorCode::StaleReplay,
            "replay receipt was produced from a different run, registry, policy, or strategy",
        )?;
        let promotion = request.promotion.as_ref().ok_or_else(|| {
            ProjectStateError::with_next_command(
                ProjectStateErrorCode::StaleReplay,
                "replay receipt cannot be accepted without a promotion receipt",
                "canon project promote --plan <PLAN> --dry-run",
            )
        })?;
        if replay.promoted_registry_digest != promotion.after_registry_digest {
            return Err(ProjectStateError::with_next_command(
                ProjectStateErrorCode::StaleReplay,
                "replay receipt does not verify the promoted registry digest",
                "canon project apply --plan <PLAN> --registry <PROMOTED_REGISTRY>",
            ));
        }
    }
    for export in &request.exports {
        ensure_binding(
            &export.binding,
            expected,
            ProjectStateErrorCode::StaleExport,
            "export receipt was produced from a different run, registry, policy, or strategy",
        )?;
        let replay = request.replay.as_ref().ok_or_else(|| {
            ProjectStateError::with_next_command(
                ProjectStateErrorCode::StaleExport,
                "export receipt cannot be accepted before replay verification",
                "canon project apply --plan <PLAN> --registry <PROMOTED_REGISTRY>",
            )
        })?;
        if export.promoted_registry_digest != replay.promoted_registry_digest
            || export.replay_hash != replay.replay_hash
        {
            return Err(ProjectStateError::with_next_command(
                ProjectStateErrorCode::StaleExport,
                "export receipt is not a descendant of the replay-verified registry",
                "canon project export --plan <PLAN>",
            ));
        }
    }
    Ok(())
}

fn ensure_binding(
    actual: &ProjectLifecycleBinding,
    expected: &ProjectLifecycleBinding,
    code: ProjectStateErrorCode,
    message: &str,
) -> ProjectStateResult<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(ProjectStateError::new(code, message))
    }
}

fn evidence_is_ready(request: &ProjectLifecycleRequest) -> bool {
    let Some(run) = &request.run_receipt else {
        return false;
    };
    if !run.failed_nodes.is_empty() || !run.cancelled_nodes.is_empty() {
        return false;
    }
    let completed = run.completed_nodes.iter().collect::<BTreeSet<_>>();
    request
        .plan
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.kind,
                ProjectPlanNodeKind::Evidence
                    | ProjectPlanNodeKind::Solve
                    | ProjectPlanNodeKind::Link
                    | ProjectPlanNodeKind::Evaluate
            )
        })
        .all(|node| completed.contains(&node.node_id))
}

fn missing_export_outputs(request: &ProjectLifecycleRequest) -> Vec<String> {
    let completed = request
        .exports
        .iter()
        .map(|export| export.output_id.as_str())
        .collect::<BTreeSet<_>>();
    request
        .plan
        .nodes
        .iter()
        .filter(|node| node.class == ProjectPlanNodeClass::Export)
        .flat_map(|node| node.outputs.iter())
        .map(|output| output.output_id.clone())
        .filter(|output_id| !completed.contains(output_id.as_str()))
        .collect()
}
