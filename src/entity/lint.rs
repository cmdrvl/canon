//! Entity workbench doctor/lint diagnostics.
//!
//! This module is deliberately read-only. It turns already-known artifact,
//! profile, registry, review, and runtime-guard facts into deterministic
//! operator diagnostics with robot-friendly next commands.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub const ENTITY_LINT_REPORT_VERSION: &str = "canon_entity_lint.v0";

#[derive(Debug, Clone, Default)]
pub struct EntityLintRequest {
    pub artifacts: Vec<EntityArtifactFreshnessCheck>,
    pub registry: Option<EntityRegistryPresenceCheck>,
    pub profile: Option<EntityProfileConsistencyCheck>,
    pub candidate_budget: Option<EntityCandidateBudgetCheck>,
    pub review_import: Option<EntityReviewImportSafetyCheck>,
    pub runtime_guard: Option<EntityRuntimeGuardCheck>,
    pub unsupported_operators: Vec<EntityUnsupportedOperatorCheck>,
    pub patch_conflicts: Vec<EntityPatchConflictCheck>,
    pub sidecar_snapshots: Vec<EntitySidecarSnapshotCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityArtifactFreshnessCheck {
    pub stage: String,
    pub expected_hash: String,
    pub actual_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRegistryPresenceCheck {
    pub registry_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityProfileConsistencyCheck {
    pub expected_profile_id: String,
    pub actual_profile_id: String,
    pub expected_identity_semantics: String,
    pub actual_identity_semantics: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityCandidateBudgetCheck {
    pub stage: String,
    pub candidate_pairs: u64,
    pub max_candidate_pairs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityReviewImportSafetyCheck {
    pub expected_review_queue_hash: String,
    pub actual_review_queue_hash: String,
    pub expected_profile_id: String,
    pub actual_profile_id: String,
    pub override_required: bool,
    pub override_approved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRuntimeGuardCheck {
    pub guard_id: String,
    pub status: String,
    pub next_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityUnsupportedOperatorCheck {
    pub stage: String,
    pub operator_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityPatchConflictCheck {
    pub patch_id: String,
    pub left_action: String,
    pub right_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitySidecarSnapshotCheck {
    pub sidecar_path: PathBuf,
    pub expected_registry_snapshot_hash: String,
    pub actual_registry_snapshot_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityLintReport {
    pub version: String,
    pub ok: bool,
    pub summary: EntityLintSummary,
    pub findings: Vec<EntityLintFinding>,
    pub next_command: String,
    pub robot: EntityLintRobotSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityLintSummary {
    pub total_findings: u64,
    pub errors: u64,
    pub warnings: u64,
    pub info: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityLintFinding {
    pub id: String,
    pub severity: String,
    pub category: String,
    pub message: String,
    pub detail: Value,
    pub next_command: String,
    pub robot_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityLintRobotSummary {
    pub schema: String,
    pub retryable_after_fix: bool,
    pub finding_ids: Vec<String>,
    pub commands: Vec<String>,
}

pub fn lint_entity_workbench(request: EntityLintRequest) -> EntityLintReport {
    let mut findings = Vec::new();
    findings.extend(stale_artifact_findings(&request.artifacts));
    if let Some(registry) = &request.registry {
        findings.extend(missing_registry_findings(registry));
    }
    if let Some(profile) = &request.profile
        && (profile.expected_profile_id != profile.actual_profile_id
            || profile.expected_identity_semantics != profile.actual_identity_semantics)
    {
        findings.push(finding(
            "profile_mismatch",
            "error",
            "profile_mismatch",
            "Entity artifact profile metadata does not match the requested profile",
            json!({
                "expected_profile_id": profile.expected_profile_id,
                "actual_profile_id": profile.actual_profile_id,
                "expected_identity_semantics": profile.expected_identity_semantics,
                "actual_identity_semantics": profile.actual_identity_semantics
            }),
            "Use artifacts and sidecars produced by the same entity profile, then rerun canon entity",
            "isolate_artifacts_by_profile",
        ));
    }
    if let Some(budget) = &request.candidate_budget
        && budget.candidate_pairs > budget.max_candidate_pairs
    {
        findings.push(finding(
            "candidate_budget_exceeded",
            "error",
            "over_budget_candidates",
            "Entity candidate generation exceeded the configured budget",
            json!({
                "stage": budget.stage,
                "candidate_pairs": budget.candidate_pairs,
                "max_candidate_pairs": budget.max_candidate_pairs
            }),
            "Tighten blocking configuration or raise the explicit candidate budget, then rerun canon entity block",
            "review_blocking_budget",
        ));
    }
    if let Some(review) = &request.review_import {
        findings.extend(review_import_findings(review));
    }
    if let Some(runtime) = &request.runtime_guard
        && runtime.status != "passed"
    {
        findings.push(finding(
            format!("runtime_guard:{}", runtime.guard_id),
            "error",
            "runtime_guard_failure",
            "Entity runtime guard did not pass",
            json!({
                "guard_id": runtime.guard_id,
                "status": runtime.status
            }),
            non_empty_or(
                &runtime.next_command,
                "Inspect runtime guard telemetry, then rerun the guarded entity command",
            ),
            "rerun_after_runtime_guard_fix",
        ));
    }
    for operator in &request.unsupported_operators {
        findings.push(finding(
            format!("unsupported_operator:{}", operator.operator_id),
            "error",
            "unsupported_operator",
            "Entity strategy references an unsupported operator",
            json!({
                "stage": operator.stage,
                "operator_id": operator.operator_id
            }),
            "Run strategy lint/doctor and replace the unsupported operator, then rerun canon entity",
            "replace_strategy_operator",
        ));
    }
    for conflict in &request.patch_conflicts {
        findings.push(finding(
            format!("patch_conflict:{}", conflict.patch_id),
            "error",
            "patch_conflict",
            "Entity patches contain contradictory decisions",
            json!({
                "patch_id": conflict.patch_id,
                "left_action": conflict.left_action,
                "right_action": conflict.right_action
            }),
            "Remove or adjudicate contradictory patch entries, then rerun canon entity audit",
            "resolve_patch_conflict",
        ));
    }
    for sidecar in &request.sidecar_snapshots {
        if sidecar.expected_registry_snapshot_hash != sidecar.actual_registry_snapshot_hash {
            findings.push(finding(
                format!("sidecar_drift:{}", path_display(&sidecar.sidecar_path)),
                "error",
                "sidecar_snapshot_drift",
                "Entity sidecar was produced from a different registry snapshot",
                json!({
                    "sidecar_path": path_display(&sidecar.sidecar_path),
                    "expected_registry_snapshot_hash": sidecar.expected_registry_snapshot_hash,
                    "actual_registry_snapshot_hash": sidecar.actual_registry_snapshot_hash
                }),
                "Rebuild sidecars from the current registry snapshot, then rerun canon entity audit",
                "rebuild_sidecar_snapshot",
            ));
        }
    }

    build_report(findings)
}

pub fn render_entity_lint_summary(report: &EntityLintReport) -> String {
    format!(
        "entity lint: ok={} findings={} errors={} warnings={} next_command={}",
        report.ok,
        report.summary.total_findings,
        report.summary.errors,
        report.summary.warnings,
        report.next_command
    )
}

fn stale_artifact_findings(checks: &[EntityArtifactFreshnessCheck]) -> Vec<EntityLintFinding> {
    checks
        .iter()
        .filter(|check| check.expected_hash != check.actual_hash)
        .map(|check| {
            finding(
                format!("stale_artifact:{}", check.stage),
                "error",
                "stale_artifact",
                "Entity artifact hash does not match its expected upstream input",
                json!({
                    "stage": check.stage,
                    "expected_hash": check.expected_hash,
                    "actual_hash": check.actual_hash
                }),
                "Rerun canon entity from the stale stage so artifact-chain hashes line up",
                "regenerate_stale_artifact",
            )
        })
        .collect()
}

fn missing_registry_findings(check: &EntityRegistryPresenceCheck) -> Vec<EntityLintFinding> {
    let registry_json = check.registry_path.join("registry.json");
    if registry_json.exists() {
        Vec::new()
    } else {
        vec![finding(
            "missing_registry",
            "error",
            "missing_registry",
            "Entity registry is missing registry.json",
            json!({
                "registry_path": path_display(&check.registry_path),
                "missing": path_display(&registry_json)
            }),
            "Create or point --registry at a versioned entity registry, then rerun canon entity",
            "fix_registry_path",
        )]
    }
}

fn review_import_findings(check: &EntityReviewImportSafetyCheck) -> Vec<EntityLintFinding> {
    let mut findings = Vec::new();
    if check.expected_review_queue_hash != check.actual_review_queue_hash {
        findings.push(finding(
            "unsafe_review_import:source_hash",
            "error",
            "unsafe_review_import",
            "Review import decisions target a different review queue artifact",
            json!({
                "expected_review_queue_hash": check.expected_review_queue_hash,
                "actual_review_queue_hash": check.actual_review_queue_hash
            }),
            "Re-export review decisions from the current artifact, then rerun canon entity review import",
            "reexport_review_queue",
        ));
    }
    if check.expected_profile_id != check.actual_profile_id {
        findings.push(finding(
            "unsafe_review_import:profile",
            "error",
            "unsafe_review_import",
            "Review import decisions cross the entity profile firewall",
            json!({
                "expected_profile_id": check.expected_profile_id,
                "actual_profile_id": check.actual_profile_id
            }),
            "Import review decisions only into artifacts from the same entity profile",
            "reject_cross_profile_review_import",
        ));
    }
    if check.override_required && !check.override_approved {
        findings.push(finding(
            "unsafe_review_import:override",
            "error",
            "unsafe_review_import",
            "Review import override is required but not approved",
            json!({
                "override_required": check.override_required,
                "override_approved": check.override_approved
            }),
            "Add an explicit approved override decision event, then rerun canon entity review import",
            "require_override_approval",
        ));
    }
    findings
}

fn build_report(findings: Vec<EntityLintFinding>) -> EntityLintReport {
    let summary = EntityLintSummary {
        total_findings: findings.len().try_into().expect("finding count fits u64"),
        errors: count_severity(&findings, "error"),
        warnings: count_severity(&findings, "warning"),
        info: count_severity(&findings, "info"),
    };
    let next_command = findings
        .iter()
        .find(|finding| finding.severity == "error")
        .or_else(|| findings.first())
        .map(|finding| finding.next_command.clone())
        .unwrap_or_else(|| "No entity lint action required".to_string());
    let robot = robot_summary(&findings);

    EntityLintReport {
        version: ENTITY_LINT_REPORT_VERSION.to_string(),
        ok: summary.errors == 0,
        summary,
        findings,
        next_command,
        robot,
    }
}

fn robot_summary(findings: &[EntityLintFinding]) -> EntityLintRobotSummary {
    let finding_ids = findings
        .iter()
        .map(|finding| finding.id.clone())
        .collect::<Vec<_>>();
    let commands = findings
        .iter()
        .map(|finding| finding.next_command.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    EntityLintRobotSummary {
        schema: "canon.entity.lint.robot.v0".to_string(),
        retryable_after_fix: findings.iter().all(|finding| finding.severity != "info"),
        finding_ids,
        commands,
    }
}

fn finding(
    id: impl Into<String>,
    severity: &str,
    category: &str,
    message: &str,
    detail: Value,
    next_command: impl Into<String>,
    robot_action: &str,
) -> EntityLintFinding {
    EntityLintFinding {
        id: id.into(),
        severity: severity.to_string(),
        category: category.to_string(),
        message: message.to_string(),
        detail,
        next_command: next_command.into(),
        robot_action: robot_action.to_string(),
    }
}

fn count_severity(findings: &[EntityLintFinding], severity: &str) -> u64 {
    findings
        .iter()
        .filter(|finding| finding.severity == severity)
        .count()
        .try_into()
        .expect("finding count fits u64")
}

fn non_empty_or(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

fn path_display(path: &Path) -> String {
    path.display().to_string()
}
