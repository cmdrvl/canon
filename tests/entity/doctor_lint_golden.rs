#![forbid(unsafe_code)]

use canon::entity::lint::{
    EntityArtifactFreshnessCheck, EntityCandidateBudgetCheck, EntityLintRequest,
    EntityPatchConflictCheck, EntityProfileConsistencyCheck, EntityProfilePresenceCheck,
    EntityRegistryPresenceCheck, EntityReviewImportSafetyCheck, EntityRuntimeGuardCheck,
    EntitySidecarSnapshotCheck, EntityUnsupportedOperatorCheck, lint_entity_workbench,
    render_entity_lint_summary,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::PathBuf};

const EXPECTED_PROJECTION: &str =
    include_str!("../fixtures/entity/ergonomics/doctor_lint_golden_projection.json");

#[test]
fn entity_doctor_lint_golden_projects_robot_diagnostics_and_next_commands() {
    let report = lint_entity_workbench(golden_lint_request());
    let projection = json!({
        "version": "canon.entity.doctor_lint_golden_projection.v0",
        "report_version": report.version,
        "ok": report.ok,
        "summary": report.summary,
        "finding_ids": report
            .findings
            .iter()
            .map(|finding| finding.id.clone())
            .collect::<Vec<_>>(),
        "categories": report
            .findings
            .iter()
            .map(|finding| finding.category.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>(),
        "next_command": report.next_command,
        "robot": report.robot,
        "human_summary": render_entity_lint_summary(&report)
    });

    assert_eq!(projection, expected_projection());
}

fn golden_lint_request() -> EntityLintRequest {
    EntityLintRequest {
        artifacts: vec![EntityArtifactFreshnessCheck {
            stage: "edge".to_string(),
            expected_hash: "blake3:expected-edge".to_string(),
            actual_hash: "blake3:stale-edge".to_string(),
        }],
        registry: Some(EntityRegistryPresenceCheck {
            registry_path: PathBuf::from("tests/fixtures/entity/ergonomics/missing-registry"),
        }),
        profile_presence: Some(EntityProfilePresenceCheck {
            profile_id: "cmbs_tenant_label".to_string(),
            profile_path: PathBuf::from("tests/fixtures/entity/ergonomics/missing-profile.yaml"),
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
            sidecar_path: PathBuf::from("tests/fixtures/entity/ergonomics/cannot-link.jsonl"),
            expected_registry_snapshot_hash: "blake3:registry-current".to_string(),
            actual_registry_snapshot_hash: "blake3:registry-old".to_string(),
        }],
    }
}

fn expected_projection() -> Value {
    serde_json::from_str(EXPECTED_PROJECTION).expect("expected projection parses")
}
