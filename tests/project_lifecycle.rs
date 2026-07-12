#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../src/project/lifecycle.rs"]
mod lifecycle;
#[allow(dead_code)]
#[path = "../src/project/lock.rs"]
mod lock;
#[allow(dead_code)]
#[path = "../src/project/manifest.rs"]
mod manifest;
#[allow(dead_code, clippy::too_many_arguments, clippy::unnecessary_sort_by)]
#[path = "../src/project/plan.rs"]
mod plan;
#[allow(dead_code)]
#[path = "../src/project/receipt.rs"]
mod receipt;
#[allow(dead_code)]
#[path = "../src/project/state.rs"]
mod state;

use lifecycle::{
    ProjectLifecycleRequest, evaluate_project_lifecycle, lifecycle_binding_for_plan_run,
};
use lock::{
    ProjectLock, ProjectLockInput, ProjectLockManifestProjection, ProjectLockRefKind,
    ProjectLockResolvedRef, digest_bytes, refresh_project_lock,
};
use manifest::{
    ProjectManifest, ProjectPackageKind, load_project_manifest_toml, project_manifest_digest,
};
use plan::{ProjectPlan, ProjectPlanRequest, compile_project_plan};
use receipt::{
    CANON_PROJECT_RUN_VERSION, ProjectRunNodeReceipt, ProjectRunReceipt, finalized_run_receipt,
};
use state::{
    ProjectAuditReceipt, ProjectExportReceipt, ProjectLifecycleBlockerCode, ProjectLifecycleState,
    ProjectMutationPreview, ProjectPromotionReceipt, ProjectReplayReceipt, ProjectReviewReceipt,
    ProjectStateErrorCode, project_state_schema_version,
};
use std::path::PathBuf;

const MINIMAL_TOML: &str = include_str!("./fixtures/project/minimal.toml");
const REGISTRY_DIGEST: &str =
    "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const POLICY_DIGEST: &str =
    "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const STRATEGY_DIGEST: &str =
    "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

#[test]
fn lifecycle_reports_one_state_and_next_command_at_each_gate() {
    let plan = minimal_plan();
    let planned = evaluate_project_lifecycle(ProjectLifecycleRequest::new(
        plan.clone(),
        None,
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    ))
    .expect("planned status");
    assert_eq!(planned.schema_version, project_state_schema_version());
    assert_eq!(planned.state, ProjectLifecycleState::Planned);
    assert_eq!(
        planned.blockers[0].code,
        ProjectLifecycleBlockerCode::EvidenceNotReady
    );
    assert!(planned.next_commands.contains_key("run"));

    let run = completed_run(&plan);
    let evidence_ready = evaluate_project_lifecycle(ProjectLifecycleRequest::new(
        plan.clone(),
        Some(run.clone()),
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    ))
    .expect("evidence status");
    assert_eq!(evidence_ready.state, ProjectLifecycleState::EvidenceReady);
    assert!(evidence_ready.next_commands.contains_key("review"));

    let binding = lifecycle_binding_for_plan_run(
        &plan,
        &run,
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    );
    let mut request = ProjectLifecycleRequest::new(
        plan.clone(),
        Some(run.clone()),
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    );
    request.review = Some(review_receipt(binding.clone(), 3, 0, 0));
    let review_required = evaluate_project_lifecycle(request).expect("review required");
    assert_eq!(review_required.state, ProjectLifecycleState::ReviewRequired);
    assert_eq!(
        review_required.blockers[0].code,
        ProjectLifecycleBlockerCode::ReviewPending
    );

    let mut request = ProjectLifecycleRequest::new(
        plan.clone(),
        Some(run.clone()),
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    );
    request.review = Some(review_receipt(binding.clone(), 0, 2, 0));
    request.audit = Some(audit_receipt(binding.clone(), true));
    let audited = evaluate_project_lifecycle(request).expect("audited");
    assert_eq!(audited.state, ProjectLifecycleState::Audited);
    assert_eq!(
        audited.blockers[0].code,
        ProjectLifecycleBlockerCode::PromotionPreviewMissing
    );
    assert!(audited.next_commands.contains_key("promote_preview"));

    let mut request = ProjectLifecycleRequest::new(
        plan.clone(),
        Some(run.clone()),
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    );
    request.review = Some(review_receipt(binding.clone(), 0, 2, 0));
    request.audit = Some(audit_receipt(binding.clone(), true));
    request.promotion = Some(promotion_receipt(binding.clone(), false));
    let promotable = evaluate_project_lifecycle(request).expect("promotable");
    assert_eq!(promotable.state, ProjectLifecycleState::Promotable);
    assert_eq!(
        promotable.blockers[0].code,
        ProjectLifecycleBlockerCode::PromotionApprovalRequired
    );
    assert_eq!(promotable.mutation_previews.len(), 1);
    assert!(promotable.mutation_previews[0].requires_explicit_execution);
    assert!(
        promotable.mutation_previews[0]
            .intended_paths
            .contains(&"registries/entities/aliases.json".to_string())
    );

    let mut request = ProjectLifecycleRequest::new(
        plan.clone(),
        Some(run.clone()),
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    );
    request.review = Some(review_receipt(binding.clone(), 0, 2, 0));
    request.audit = Some(audit_receipt(binding.clone(), true));
    request.promotion = Some(promotion_receipt(binding.clone(), true));
    let promoted = evaluate_project_lifecycle(request).expect("promoted");
    assert_eq!(promoted.state, ProjectLifecycleState::Promoted);
    assert!(promoted.next_commands.contains_key("apply"));

    let mut request = ProjectLifecycleRequest::new(
        plan.clone(),
        Some(run.clone()),
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    );
    request.review = Some(review_receipt(binding.clone(), 0, 2, 0));
    request.audit = Some(audit_receipt(binding.clone(), true));
    request.promotion = Some(promotion_receipt(binding.clone(), true));
    request.replay = Some(replay_receipt(binding.clone(), true));
    let replay_verified = evaluate_project_lifecycle(request).expect("replay verified");
    assert_eq!(replay_verified.state, ProjectLifecycleState::ReplayVerified);
    assert!(replay_verified.next_commands.contains_key("export"));

    let mut request = ProjectLifecycleRequest::new(
        plan.clone(),
        Some(run),
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    );
    request.review = Some(review_receipt(binding.clone(), 0, 2, 0));
    request.audit = Some(audit_receipt(binding.clone(), true));
    request.promotion = Some(promotion_receipt(binding.clone(), true));
    request.replay = Some(replay_receipt(binding.clone(), true));
    request.exports = export_receipts(&plan, binding);
    let exported = evaluate_project_lifecycle(request).expect("exported");
    assert_eq!(exported.state, ProjectLifecycleState::Exported);
    assert!(exported.blockers.is_empty());
    assert!(exported.next_commands.is_empty());
}

#[test]
fn stale_review_audit_promotion_and_export_receipts_refuse() {
    let plan = minimal_plan();
    let run = completed_run(&plan);
    let binding = lifecycle_binding_for_plan_run(
        &plan,
        &run,
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    );

    let mut stale_binding = binding.clone();
    stale_binding.plan_graph_hash = digest_bytes(b"different-plan");
    let mut request = ProjectLifecycleRequest::new(
        plan.clone(),
        Some(run.clone()),
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    );
    request.review = Some(review_receipt(stale_binding, 0, 1, 0));
    let error = evaluate_project_lifecycle(request).expect_err("stale review refuses");
    assert_eq!(error.code, ProjectStateErrorCode::StaleReview);

    let mut request = ProjectLifecycleRequest::new(
        plan.clone(),
        Some(run.clone()),
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    );
    request.review = Some(review_receipt(binding.clone(), 0, 1, 0));
    request.audit = Some(ProjectAuditReceipt {
        reviewed_decision_hash:
            "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".to_string(),
        ..audit_receipt(binding.clone(), true)
    });
    let error = evaluate_project_lifecycle(request).expect_err("stale audit refuses");
    assert_eq!(error.code, ProjectStateErrorCode::StaleAudit);

    let mut request = ProjectLifecycleRequest::new(
        plan.clone(),
        Some(run.clone()),
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    );
    request.review = Some(review_receipt(binding.clone(), 0, 1, 0));
    request.audit = Some(audit_receipt(binding.clone(), true));
    request.promotion = Some(ProjectPromotionReceipt {
        before_registry_digest: digest_bytes(b"raced-registry"),
        ..promotion_receipt(binding.clone(), false)
    });
    let error = evaluate_project_lifecycle(request).expect_err("registry race refuses");
    assert_eq!(error.code, ProjectStateErrorCode::RegistryRace);

    let mut request = ProjectLifecycleRequest::new(
        plan.clone(),
        Some(run),
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    );
    request.review = Some(review_receipt(binding.clone(), 0, 1, 0));
    request.audit = Some(audit_receipt(binding.clone(), true));
    request.promotion = Some(promotion_receipt(binding.clone(), true));
    request.replay = Some(replay_receipt(binding.clone(), true));
    request.exports = vec![ProjectExportReceipt {
        replay_hash: digest_bytes(b"wrong-replay"),
        ..export_receipts(&plan, binding)[0].clone()
    }];
    let error = evaluate_project_lifecycle(request).expect_err("stale export refuses");
    assert_eq!(error.code, ProjectStateErrorCode::StaleExport);
}

#[test]
fn audit_rejections_and_partial_exports_block_without_mutation_theatre() {
    let plan = minimal_plan();
    let run = completed_run(&plan);
    let binding = lifecycle_binding_for_plan_run(
        &plan,
        &run,
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    );

    let mut request = ProjectLifecycleRequest::new(
        plan.clone(),
        Some(run.clone()),
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    );
    request.review = Some(review_receipt(binding.clone(), 0, 1, 0));
    request.audit = Some(audit_receipt(binding.clone(), false));
    let audited = evaluate_project_lifecycle(request).expect("audit rejection reports");
    assert_eq!(audited.state, ProjectLifecycleState::Audited);
    assert_eq!(
        audited.blockers[0].code,
        ProjectLifecycleBlockerCode::AuditRejected
    );
    assert!(audited.mutation_previews.is_empty());

    let mut exports = export_receipts(&plan, binding.clone());
    exports[0].partial = true;
    let mut request = ProjectLifecycleRequest::new(
        plan.clone(),
        Some(run),
        REGISTRY_DIGEST,
        POLICY_DIGEST,
        STRATEGY_DIGEST,
    );
    request.review = Some(review_receipt(binding.clone(), 0, 1, 0));
    request.audit = Some(audit_receipt(binding.clone(), true));
    request.promotion = Some(promotion_receipt(binding.clone(), true));
    request.replay = Some(replay_receipt(binding, true));
    request.exports = exports;
    let replay_verified = evaluate_project_lifecycle(request).expect("partial export reports");
    assert_eq!(replay_verified.state, ProjectLifecycleState::ReplayVerified);
    assert_eq!(
        replay_verified.blockers[0].code,
        ProjectLifecycleBlockerCode::ExportPartial
    );
}

fn minimal_plan() -> ProjectPlan {
    let manifest = minimal_manifest();
    compile_project_plan(ProjectPlanRequest::new(
        manifest.clone(),
        lock_for_manifest(&manifest),
        PathBuf::from("tests/fixtures/project/minimal.toml"),
        PathBuf::from("tests/fixtures/project/minimal.lock.json"),
    ))
    .expect("plan compiles")
}

fn completed_run(plan: &ProjectPlan) -> ProjectRunReceipt {
    finalized_run_receipt(ProjectRunReceipt {
        schema_version: CANON_PROJECT_RUN_VERSION.to_string(),
        project_id: plan.project_id.clone(),
        plan_graph_hash: plan.graph_hash.clone(),
        receipt_hash: String::new(),
        completed_nodes: plan.nodes.iter().map(|node| node.node_id.clone()).collect(),
        failed_nodes: Vec::new(),
        cancelled_nodes: Vec::new(),
        invalidated_nodes: Vec::new(),
        blocked_nodes: Vec::new(),
        node_receipts: Vec::<ProjectRunNodeReceipt>::new(),
    })
    .expect("run receipt finalizes")
}

fn review_receipt(
    binding: state::ProjectLifecycleBinding,
    pending: u64,
    accepted: u64,
    rejected: u64,
) -> ProjectReviewReceipt {
    ProjectReviewReceipt {
        receipt_id: "review.receipt".to_string(),
        binding,
        review_bundle_hash: digest_bytes(b"review-bundle"),
        decision_hash: digest_bytes(b"review-decisions"),
        pending_decisions: pending,
        accepted_decisions: accepted,
        rejected_decisions: rejected,
    }
}

fn audit_receipt(binding: state::ProjectLifecycleBinding, passed: bool) -> ProjectAuditReceipt {
    ProjectAuditReceipt {
        receipt_id: "audit.receipt".to_string(),
        binding,
        audit_hash: digest_bytes(b"audit"),
        reviewed_decision_hash: digest_bytes(b"review-decisions"),
        passed,
    }
}

fn promotion_receipt(
    binding: state::ProjectLifecycleBinding,
    executed: bool,
) -> ProjectPromotionReceipt {
    ProjectPromotionReceipt {
        receipt_id: if executed {
            "promotion.executed".to_string()
        } else {
            "promotion.preview".to_string()
        },
        binding,
        promotion_hash: digest_bytes(b"promotion"),
        review_decision_hash: digest_bytes(b"review-decisions"),
        audit_hash: digest_bytes(b"audit"),
        before_registry_digest: REGISTRY_DIGEST.to_string(),
        after_registry_digest: digest_bytes(b"promoted-registry"),
        mutation_preview: ProjectMutationPreview {
            command: "canon project promote --plan <PLAN> --execute".to_string(),
            intended_paths: vec![
                "registries/entities/aliases.json".to_string(),
                "registries/entities/registry.json".to_string(),
            ],
            version_change: "1.2.0 -> 1.3.0".to_string(),
            requires_explicit_execution: true,
        },
        executed,
    }
}

fn replay_receipt(binding: state::ProjectLifecycleBinding, passed: bool) -> ProjectReplayReceipt {
    ProjectReplayReceipt {
        receipt_id: "replay.receipt".to_string(),
        binding,
        replay_hash: digest_bytes(b"replay"),
        promoted_registry_digest: digest_bytes(b"promoted-registry"),
        passed,
    }
}

fn export_receipts(
    plan: &ProjectPlan,
    binding: state::ProjectLifecycleBinding,
) -> Vec<ProjectExportReceipt> {
    plan.nodes
        .iter()
        .filter(|node| node.kind == plan::ProjectPlanNodeKind::Export)
        .flat_map(|node| node.outputs.iter())
        .map(|output| ProjectExportReceipt {
            receipt_id: format!("export.{}", output.output_id),
            binding: binding.clone(),
            output_id: output.output_id.clone(),
            output_digest: digest_bytes(output.output_id.as_bytes()),
            promoted_registry_digest: digest_bytes(b"promoted-registry"),
            replay_hash: digest_bytes(b"replay"),
            partial: false,
        })
        .collect()
}

fn minimal_manifest() -> ProjectManifest {
    load_project_manifest_toml(MINIMAL_TOML).expect("minimal manifest loads")
}

fn lock_for_manifest(manifest: &ProjectManifest) -> ProjectLock {
    let digest = project_manifest_digest(manifest).expect("manifest digest");
    refresh_project_lock(&ProjectLockManifestProjection {
        project_id: manifest.project_id.clone(),
        project_digest: digest,
        inputs: manifest
            .sources
            .iter()
            .map(|source| ProjectLockInput {
                input_id: source.source_id.clone(),
                relative_path: source.path.clone(),
                content_digest: digest_bytes(source.path.as_bytes()),
            })
            .collect(),
        resolved_refs: manifest
            .packages
            .iter()
            .map(|package| ProjectLockResolvedRef {
                ref_id: package.alias.clone(),
                kind: match package.kind {
                    ProjectPackageKind::Strategy => ProjectLockRefKind::Strategy,
                    ProjectPackageKind::Registry
                    | ProjectPackageKind::EntityProfile
                    | ProjectPackageKind::SourceMapping
                    | ProjectPackageKind::Extension => ProjectLockRefKind::Package,
                },
                resolved_digest: package.content_hash.clone(),
            })
            .collect(),
    })
    .expect("project lock builds")
}
