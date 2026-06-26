#![forbid(unsafe_code)]

use canon::entity::lint::{
    ENTITY_LINT_REPORT_VERSION, EntityArtifactFreshnessCheck, EntityCandidateBudgetCheck,
    EntityLintRequest, EntityPatchConflictCheck, EntityProfileConsistencyCheck,
    EntityRegistryPresenceCheck, EntityReviewImportSafetyCheck, EntityRuntimeGuardCheck,
    EntitySidecarSnapshotCheck, EntityUnsupportedOperatorCheck, lint_entity_workbench,
    render_entity_lint_summary,
};
use std::{collections::BTreeSet, fs};

#[test]
fn entity_doctor_lint_reports_required_diagnostics_with_robot_next_commands() {
    let temp = tempfile::tempdir().expect("tempdir");
    let missing_registry = temp.path().join("missing-registry");
    let report = lint_entity_workbench(EntityLintRequest {
        artifacts: vec![EntityArtifactFreshnessCheck {
            stage: "edge".to_string(),
            expected_hash: "blake3:expected-edge".to_string(),
            actual_hash: "blake3:stale-edge".to_string(),
        }],
        registry: Some(EntityRegistryPresenceCheck {
            registry_path: missing_registry.clone(),
        }),
        profile: Some(EntityProfileConsistencyCheck {
            expected_profile_id: "cmbs_tenant_label".to_string(),
            actual_profile_id: "regab_firm_identity".to_string(),
            expected_identity_semantics: "tenant_display_label".to_string(),
            actual_identity_semantics: "same_firm_or_reviewed_alias".to_string(),
        }),
        candidate_budget: Some(EntityCandidateBudgetCheck {
            stage: "block".to_string(),
            candidate_pairs: 25_001,
            max_candidate_pairs: 25_000,
        }),
        review_import: Some(EntityReviewImportSafetyCheck {
            expected_review_queue_hash: "blake3:review-current".to_string(),
            actual_review_queue_hash: "blake3:review-stale".to_string(),
            expected_profile_id: "cmbs_tenant_label".to_string(),
            actual_profile_id: "regab_firm_identity".to_string(),
            override_required: true,
            override_approved: false,
        }),
        runtime_guard: Some(EntityRuntimeGuardCheck {
            guard_id: "no_network_or_model_runtime".to_string(),
            status: "failed".to_string(),
            next_command: "Disable network/model runtime path and rerun canon entity".to_string(),
        }),
        unsupported_operators: vec![EntityUnsupportedOperatorCheck {
            stage: "block".to_string(),
            operator_id: "embedding_similarity".to_string(),
        }],
        patch_conflicts: vec![EntityPatchConflictCheck {
            patch_id: "patch:sears-auto-center".to_string(),
            left_action: "alias".to_string(),
            right_action: "distinct".to_string(),
        }],
        sidecar_snapshots: vec![EntitySidecarSnapshotCheck {
            sidecar_path: temp.path().join("cannot-link.jsonl"),
            expected_registry_snapshot_hash: "blake3:registry-current".to_string(),
            actual_registry_snapshot_hash: "blake3:registry-old".to_string(),
        }],
    });

    assert_eq!(report.version, ENTITY_LINT_REPORT_VERSION);
    assert!(!report.ok);
    assert_eq!(report.summary.total_findings, 11);
    assert_eq!(report.summary.errors, report.summary.total_findings);
    assert!(report.next_command.contains("Rerun canon entity"));
    assert!(report.robot.retryable_after_fix);
    assert_eq!(report.robot.finding_ids.len(), report.findings.len());
    assert!(
        report
            .robot
            .commands
            .iter()
            .all(|command| !command.trim().is_empty())
    );

    let categories = report
        .findings
        .iter()
        .map(|finding| finding.category.as_str())
        .collect::<BTreeSet<_>>();
    for required in [
        "stale_artifact",
        "missing_registry",
        "profile_mismatch",
        "over_budget_candidates",
        "unsafe_review_import",
        "runtime_guard_failure",
        "unsupported_operator",
        "patch_conflict",
        "sidecar_snapshot_drift",
    ] {
        assert!(
            categories.contains(required),
            "missing diagnostic category {required}"
        );
    }

    let missing_registry_finding = report
        .findings
        .iter()
        .find(|finding| finding.category == "missing_registry")
        .expect("missing registry diagnostic");
    assert_eq!(
        missing_registry_finding.detail["registry_path"],
        missing_registry.display().to_string()
    );
    assert!(missing_registry_finding.next_command.contains("--registry"));
}

#[test]
fn runtime_guard_diagnostic_has_actionable_next_command() {
    let report = lint_entity_workbench(EntityLintRequest {
        runtime_guard: Some(EntityRuntimeGuardCheck {
            guard_id: "runtime_guard_diagnostic".to_string(),
            status: "failed".to_string(),
            next_command: "cargo test runtime_guard_diagnostic -- --nocapture".to_string(),
        }),
        ..EntityLintRequest::default()
    });

    assert!(!report.ok);
    assert_eq!(report.summary.errors, 1);
    assert_eq!(report.findings[0].category, "runtime_guard_failure");
    assert_eq!(
        report.findings[0].next_command,
        "cargo test runtime_guard_diagnostic -- --nocapture"
    );
    assert_eq!(
        report.robot.commands,
        ["cargo test runtime_guard_diagnostic -- --nocapture"]
    );
}

#[test]
fn entity_doctor_lint_clean_report_is_robot_readable_and_read_only() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::write(registry.join("registry.json"), "{}").expect("registry metadata");

    let report = lint_entity_workbench(EntityLintRequest {
        artifacts: vec![EntityArtifactFreshnessCheck {
            stage: "solve".to_string(),
            expected_hash: "blake3:same".to_string(),
            actual_hash: "blake3:same".to_string(),
        }],
        registry: Some(EntityRegistryPresenceCheck {
            registry_path: registry,
        }),
        runtime_guard: Some(EntityRuntimeGuardCheck {
            guard_id: "no_network_or_model_runtime".to_string(),
            status: "passed".to_string(),
            next_command: String::new(),
        }),
        ..EntityLintRequest::default()
    });

    assert!(report.ok);
    assert!(report.findings.is_empty());
    assert_eq!(report.summary.total_findings, 0);
    assert_eq!(report.next_command, "No entity lint action required");
    assert_eq!(report.robot.schema, "canon.entity.lint.robot.v0");
    assert!(report.robot.commands.is_empty());
    assert!(render_entity_lint_summary(&report).contains("ok=true"));
}
