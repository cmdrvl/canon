#![forbid(unsafe_code)]

use canon::entity::{
    lint::{
        ENTITY_LINT_REPORT_VERSION, EntityArtifactFreshnessCheck, EntityCandidateBudgetCheck,
        EntityLintRequest, EntityPatchConflictCheck, EntityProfileConsistencyCheck,
        EntityProfilePresenceCheck, EntityRegistryPresenceCheck, EntityReviewImportSafetyCheck,
        EntityRuntimeGuardCheck, EntitySidecarSnapshotCheck, EntityUnsupportedOperatorCheck,
        lint_entity_workbench, render_entity_lint_summary,
    },
    runtime::{
        explain::explain_from_artifact_value,
        types::{EntityState, EvidenceKind, ExplainQuery, PromotionDecision},
    },
};
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path};

const GOLDEN_MANIFEST: &str = "tests/fixtures/entity/ergonomics/golden_manifest.json";

#[test]
fn entity_explain_golden_reconstructs_row_surface_and_canon_id() {
    let manifest = json_fixture(GOLDEN_MANIFEST);
    assert_eq!(
        manifest["schema_version"],
        "canon.entity.ergonomics_golden.v0"
    );
    let bundle = json_fixture(str_at(&manifest["explain"], "bundle"));

    let row_case = &manifest["explain"]["queries"]["row"];
    let row_artifact = explain_from_artifact_value(
        ExplainQuery {
            row_id: Some(str_at(row_case, "row_id").to_string()),
            ..ExplainQuery::default()
        },
        bundle.clone(),
    )
    .expect("row explain golden reconstructs");
    assert_explain_projection(&row_artifact.result, &manifest);
    assert_eq!(
        row_artifact.result.canonical_id.as_deref(),
        Some(str_at(row_case, "expected_canonical_id"))
    );
    assert!(
        row_artifact
            .result
            .surfaces
            .iter()
            .any(|surface| surface.surface_id == str_at(row_case, "expected_surface_id"))
    );

    let surface_case = &manifest["explain"]["queries"]["surface"];
    let surface_artifact = explain_from_artifact_value(
        ExplainQuery {
            surface_id: Some(str_at(surface_case, "surface_id").to_string()),
            ..ExplainQuery::default()
        },
        bundle.clone(),
    )
    .expect("surface explain golden reconstructs");
    assert_explain_projection(&surface_artifact.result, &manifest);
    assert_eq!(
        surface_artifact.result.canonical_id.as_deref(),
        Some(str_at(surface_case, "expected_canonical_id"))
    );
    assert!(surface_artifact.result.surfaces.iter().any(|surface| {
        surface
            .normalized_views
            .get("tenant_label")
            .is_some_and(|value| value == str_at(surface_case, "expected_normalized_view"))
    }));

    let canonical_case = &manifest["explain"]["queries"]["canonical"];
    let canonical_artifact = explain_from_artifact_value(
        ExplainQuery {
            canonical_id: Some(str_at(canonical_case, "canonical_id").to_string()),
            ..ExplainQuery::default()
        },
        bundle,
    )
    .expect("canonical-id explain golden reconstructs");
    assert_explain_projection(&canonical_artifact.result, &manifest);
    assert_eq!(
        canonical_artifact.result.canonical_id.as_deref(),
        Some(str_at(canonical_case, "canonical_id"))
    );
    assert_eq!(
        (canonical_artifact.result.backbone_rows.len()
            + canonical_artifact.result.attached_rows.len()) as u64,
        u64_at(canonical_case, "expected_row_count")
    );
    let actual_surface_ids = canonical_artifact
        .result
        .surfaces
        .iter()
        .map(|surface| surface.surface_id.as_str())
        .collect::<BTreeSet<_>>();
    for expected in strings(&canonical_case["expected_surface_ids"]) {
        assert!(
            actual_surface_ids.contains(expected.as_str()),
            "canonical explain missing surface {expected}"
        );
    }
}

#[test]
fn entity_explain_golden_summary_robot_projection_is_stable() {
    let manifest = json_fixture(GOLDEN_MANIFEST);
    let summary = &manifest["summary"];
    let robot = json_fixture(str_at(summary, "robot_projection"));

    assert_eq!(
        robot["version"],
        "canon.entity.operator_journey.summary_robot.v0"
    );
    let run_counts = robot["run"]["counts"].as_object().expect("run counts");
    for key in strings(&summary["required_count_keys"]) {
        assert!(run_counts.contains_key(&key), "missing run count {key}");
        assert_eq!(
            robot["run"]["counts"][key.as_str()],
            summary["expected_run_counts"][key.as_str()]
        );
    }
    assert_eq!(
        strings(&robot["run"]["top_unresolved_tokens"]),
        strings(&summary["top_unresolved_tokens"])
    );
    assert_eq!(
        strings(&robot["run"]["top_anti_merge_reasons"]),
        strings(&summary["top_anti_merge_reasons"])
    );
    assert_eq!(
        strings(&robot["run"]["next_command_keys"]),
        strings(&summary["required_next_command_keys"])
    );
    assert_eq!(
        strings(&robot["run"]["telemetry_link_keys"]),
        strings(&summary["required_telemetry_link_keys"])
    );
    let stage_names = robot["stages"]
        .as_array()
        .expect("stage array")
        .iter()
        .map(|stage| stage["stage"].as_str().expect("stage").to_string())
        .collect::<Vec<_>>();
    assert_eq!(stage_names, strings(&summary["required_stage_names"]));
}

#[test]
fn entity_doctor_lint_golden_reports_actionable_robot_diagnostics() {
    let manifest = json_fixture(GOLDEN_MANIFEST);
    let lint = &manifest["doctor_lint"];
    let temp = tempfile::tempdir().expect("tempdir");

    let report = lint_entity_workbench(EntityLintRequest {
        artifacts: vec![EntityArtifactFreshnessCheck {
            stage: "solve".to_string(),
            expected_hash: "blake3:expected-solve".to_string(),
            actual_hash: "blake3:stale-solve".to_string(),
        }],
        registry: Some(EntityRegistryPresenceCheck {
            registry_path: temp.path().join("missing-registry"),
        }),
        profile_presence: Some(EntityProfilePresenceCheck {
            profile_id: "cmbs_tenant_label".to_string(),
            profile_path: temp.path().join("profiles/cmbs_tenant_label.yaml"),
        }),
        profile: Some(EntityProfileConsistencyCheck {
            expected_profile_id: "cmbs_tenant_label".to_string(),
            actual_profile_id: "regab_firm_identity".to_string(),
            expected_identity_semantics: "canonical_display_label".to_string(),
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
        unsupported_operators: vec![
            EntityUnsupportedOperatorCheck {
                stage: "edge".to_string(),
                operator_id: "embedding_similarity".to_string(),
            },
            EntityUnsupportedOperatorCheck {
                stage: "prepare".to_string(),
                operator_id: "overbroad_token_stripping".to_string(),
            },
        ],
        patch_conflicts: vec![
            EntityPatchConflictCheck {
                patch_id: "patch:sears-auto-center".to_string(),
                left_action: "alias".to_string(),
                right_action: "distinct".to_string(),
            },
            EntityPatchConflictCheck {
                patch_id: "patch:duplicate-sears-alias".to_string(),
                left_action: "alias:TNT-SEARS".to_string(),
                right_action: "alias:TNT-SEARS-2".to_string(),
            },
        ],
        sidecar_snapshots: vec![EntitySidecarSnapshotCheck {
            sidecar_path: temp.path().join("sidecars/cannot-link.jsonl"),
            expected_registry_snapshot_hash: "blake3:registry-current".to_string(),
            actual_registry_snapshot_hash: "blake3:registry-old".to_string(),
        }],
    });

    assert_eq!(report.version, ENTITY_LINT_REPORT_VERSION);
    assert!(!report.ok);
    assert_eq!(
        report.summary.total_findings,
        u64_at(lint, "expected_total_findings")
    );
    assert_eq!(report.summary.errors, report.summary.total_findings);
    assert_eq!(report.robot.schema, "canon.entity.lint.robot.v0");
    assert!(report.robot.retryable_after_fix);
    assert_eq!(report.robot.finding_ids.len(), report.findings.len());

    let categories = report
        .findings
        .iter()
        .map(|finding| finding.category.as_str())
        .collect::<BTreeSet<_>>();
    for expected in strings(&lint["expected_categories"]) {
        assert!(
            categories.contains(expected.as_str()),
            "missing lint category {expected}"
        );
    }

    let robot_actions = report
        .findings
        .iter()
        .map(|finding| finding.robot_action.as_str())
        .collect::<BTreeSet<_>>();
    for expected in strings(&lint["expected_robot_actions"]) {
        assert!(
            robot_actions.contains(expected.as_str()),
            "missing robot action {expected}"
        );
    }

    for fragment in strings(&lint["required_robot_command_fragments"]) {
        assert!(
            report
                .robot
                .commands
                .iter()
                .any(|command| command.contains(fragment.as_str())),
            "missing robot command fragment {fragment}"
        );
    }

    let human = render_entity_lint_summary(&report);
    assert!(human.contains("ok=false"));
    assert!(human.contains("unsafe_review_import"));
    assert!(human.contains("sidecar_snapshot_drift"));
}

#[test]
fn entity_ergonomics_golden_manifest_references_parseable_supplemental_goldens() {
    let manifest = json_fixture(GOLDEN_MANIFEST);
    let supplemental = manifest["supplemental_goldens"]
        .as_object()
        .expect("supplemental goldens");
    let expected_schemas = [
        ("explain_cases", "canon.entity.explain_goldens.v0"),
        ("summary_robot", "canon.entity.summary_robot_golden.v0"),
        ("doctor_lint", "canon.entity.doctor_lint_golden.v0"),
    ];

    for (key, expected_schema) in expected_schemas {
        let path = supplemental[key]
            .as_str()
            .unwrap_or_else(|| panic!("missing supplemental golden {key}"));
        let golden = json_fixture(path);
        assert_eq!(golden["schema_version"], expected_schema);
    }
}

fn assert_explain_projection(
    result: &canon::entity::runtime::types::ExplainResult,
    manifest: &Value,
) {
    let expected = &manifest["explain"]["expected_counts"];
    assert_eq!(result.state, EntityState::ResolvedExisting);
    assert_eq!(
        result.next_action.as_deref(),
        Some(str_at(&manifest["explain"], "expected_next_action"))
    );
    assert_eq!(
        result.candidates.len() as u64,
        u64_at(expected, "candidates")
    );
    assert!(result.positive_evidence.len() as u64 >= u64_at(expected, "positive_evidence_min"));
    assert_eq!(
        result.anti_merge_evidence.len() as u64,
        u64_at(expected, "anti_merge_evidence")
    );
    assert_eq!(
        result.review_decisions.len() as u64,
        u64_at(expected, "review_decisions")
    );
    assert_eq!(
        result.promotion_provenance.len() as u64,
        u64_at(expected, "promotion_provenance")
    );
    assert!(
        result
            .positive_evidence
            .iter()
            .any(|evidence| evidence.kind == EvidenceKind::Support)
    );
    assert!(
        result
            .positive_evidence
            .iter()
            .any(|evidence| evidence.namespace == "relation_hint")
    );
    assert!(
        result
            .anti_merge_evidence
            .iter()
            .any(|evidence| evidence.kind == EvidenceKind::CannotLink)
    );
    assert!(
        result
            .promotion_provenance
            .iter()
            .any(|record| record.decision == PromotionDecision::Promote)
    );
    for section in strings(&manifest["explain"]["required_sections"]) {
        assert_required_explain_section(result, section.as_str());
    }
}

fn assert_required_explain_section(
    result: &canon::entity::runtime::types::ExplainResult,
    section: &str,
) {
    match section {
        "normalized_views" => assert!(
            result
                .surfaces
                .iter()
                .any(|surface| !surface.normalized_views.is_empty())
        ),
        "candidates" => assert!(!result.candidates.is_empty()),
        "positive_evidence" | "relation_hints" => assert!(!result.positive_evidence.is_empty()),
        "anti_merge_evidence" => assert!(!result.anti_merge_evidence.is_empty()),
        "solver_decision" => assert_eq!(result.state, EntityState::ResolvedExisting),
        "review_decisions" => assert!(!result.review_decisions.is_empty()),
        "promotion_provenance" => assert!(!result.promotion_provenance.is_empty()),
        other => panic!("unknown explain section {other}"),
    }
}

fn json_fixture(relative: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    serde_json::from_slice(&fs::read(&path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", path.display());
    }))
    .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn str_at<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("missing string field {key}"))
}

fn u64_at(value: &Value, key: &str) -> u64 {
    value[key]
        .as_u64()
        .unwrap_or_else(|| panic!("missing u64 field {key}"))
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("expected string array: {value}"))
        .iter()
        .map(|entry| entry.as_str().expect("string").to_string())
        .collect()
}
