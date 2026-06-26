#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_APPLY_VERSION, CANON_ENTITY_DECISION_LEDGER_VERSION,
        CANON_ENTITY_PROMOTE_VERSION, EntityArtifactHeader, EntityArtifactReference,
        EntityDeterministicSummary, EntityInputReference, EntityPatchNamespaces,
        EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
        apply::{
            APPLY_CANONICAL_FIELDS, ApplyCanonicalResolution, ApplyRegistryReference,
            ApplySafetyCheck, ApplyStreamRequest, run_apply_streaming,
        },
        artifact_chain::{
            EntityArtifactChainExpectation, EntityArtifactChainLink, EntityChainStage,
        },
        audit::{EntityAuditGateCheck, EntityAuditRequest, EntityAuditSuite, run_entity_audit},
        edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
        graph::{SignedEvidenceGraphInput, build_signed_evidence_graph},
        promote::{
            EntityPromoteRegistryRequest, EntityPromotedAlias, EntityPromotionAuditExpectation,
            promote_registry_aliases,
        },
        review::{
            ReviewExportInclude, ReviewQueueArtifact, ReviewQueueRequest,
            build_review_queue_artifact, render_review_queue_csv,
        },
        review_import::{
            ReviewImportAction, ReviewImportContext, ReviewImportDecision, ReviewImportRequest,
            import_review_decisions,
        },
        run::{EntityRunArtifact, EntityRunRequest, render_run_summary, run_entity_workbench},
        schema::CANON_ENTITY_REVIEW_QUEUE_VERSION,
        score::{ScoreLane, ScoreUnits},
        solve::{
            SolveArtifact, SolveArtifactRequest, SolveReconciliationConfig, SolveSurfaceProvenance,
            build_solve_artifact_contract,
        },
    },
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const OBSERVATIONS_PATH: &str = "tests/fixtures/entity/cmbs/small_book/observations.csv";
const SMALL_BOOK_SUMMARY_PATH: &str = "tests/fixtures/entity/cmbs/small_book/expected_summary.json";
const REVIEW_QUEUE_PATH: &str = "tests/fixtures/entity/cmbs/small_book/review_queue.csv";
const PROMOTION_MANIFEST_PATH: &str = "tests/fixtures/entity/cmbs/promotion_loop/manifest.json";
const E2E_SUMMARY_PATH: &str = "tests/fixtures/entity/cmbs/e2e/operator_summary.json";
const E2E_APPLY_EXPECTED_PATH: &str = "tests/fixtures/entity/cmbs/e2e/expected_apply.csv";
const MINI_E2E_MANIFEST_PATH: &str = "tests/fixtures/entity/e2e/cmbs_small/manifest.json";
const STRATEGY_PATH: &str = "tests/fixtures/entity/profiles/cmbs_tenant_label.yaml";

#[test]
fn cmbs_e2e_small_book_operator_summary_is_semantic_and_replayable() {
    let expected = json_fixture(E2E_SUMMARY_PATH);
    let small_book = json_fixture(SMALL_BOOK_SUMMARY_PATH);
    assert_eq!(
        expected["schema_version"],
        "canon.entity.cmbs_e2e_operator_summary.v0"
    );
    assert_eq!(expected["profile_id"], "cmbs_tenant_label");
    assert_eq!(expected["identity_semantics"], "canonical_display_label");

    let rows = csv_rows(&fixture(OBSERVATIONS_PATH));
    assert_source_summary(&rows, &expected);
    assert_expected_surfaces(&small_book, &expected);
    assert_operator_rollups(&rows, &small_book, &expected);
    assert_review_groups(&expected);
    assert_promotable_aliases(&expected);

    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("work");
    write_cmbs_registry(&registry);

    let run = run_entity_workbench(EntityRunRequest {
        rows: &fixture(OBSERVATIONS_PATH),
        profile: "cmbs_tenant_label",
        strategy: &fixture(STRATEGY_PATH),
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("CMBS e2e run succeeds");
    assert_run_summary(&run.artifact.summary_as_json(), &expected);
    assert_stage_artifacts(&work_dir, &expected);

    let summary_line = render_run_summary(&run.artifact);
    for required in [
        "canon_entity_run.v0",
        "profile=cmbs_tenant_label",
        "registry=cmbs-tenants@2026.06.26",
        "review_groups=",
    ] {
        assert!(
            summary_line.contains(required),
            "run summary omits {required}: {summary_line}"
        );
    }
    assert_next_commands(&run.artifact.next_commands_as_json(), &expected);
    assert_apply_replay(&rows, temp.path(), &expected);
}

#[test]
fn entity_cmbs_e2e_small_review_promote_apply_logging_contract() {
    let manifest = json_fixture(MINI_E2E_MANIFEST_PATH);
    assert_eq!(manifest["schema_version"], "canon.entity.cmbs_mini_e2e.v0");

    let temp = tempfile::tempdir().expect("tempdir");
    let run_registry = temp.path().join("run-registry");
    let work_dir = temp.path().join("work");
    write_cmbs_mini_e2e_registry(&run_registry);

    let run = run_entity_workbench(EntityRunRequest {
        rows: &fixture(OBSERVATIONS_PATH),
        profile: "cmbs_tenant_label",
        strategy: &fixture(STRATEGY_PATH),
        registry: &run_registry,
        work_dir: &work_dir,
    })
    .expect("CMBS mini e2e run succeeds");
    let solve: SolveArtifact = read_json(&work_dir.join("solve/solve.json"));
    let mut logs = run_stage_logs(&run.artifact, &work_dir);

    let review = build_review_queue_artifact(ReviewQueueRequest {
        solve_artifact: mini_e2e_review_solve(&solve),
        include: ReviewExportInclude::All,
        provenance_samples: vec![],
        relation_hints: vec![],
    })
    .expect("review export succeeds");
    let review_path = work_dir.join("review.csv");
    fs::write(
        &review_path,
        render_review_queue_csv(&review).expect("review csv renders"),
    )
    .expect("review csv writes");
    assert_repeated_review_ambiguity_grouped_once(&review, &manifest);
    logs.push(stage_log(
        "review_export",
        run.artifact.next_commands.review_export.clone(),
        0,
        &review_path,
        &review.version,
        &review.artifact_content_hash,
        serde_json::to_value(&review.summary).expect("review summary json"),
    ));

    let ledger_path = work_dir.join("solve/decision_ledger.jsonl");
    let import_receipt = import_review_decisions(ReviewImportRequest {
        context: review_import_context(&review),
        decisions: review_import_decisions(&review),
        ledger_path: ledger_path.clone(),
        timestamp: "2026-06-26T20:30:00Z".to_string(),
        previous_event_hash: "blake3:cmbs-mini-e2e-start".to_string(),
    })
    .expect("review import succeeds");
    assert_eq!(
        import_receipt.accepted_decisions,
        review.review_items.len() as u64
    );

    let solve_header = solve_header(&solve);
    let audit = run_entity_audit(EntityAuditRequest {
        expected: EntityArtifactChainExpectation::from_link(
            EntityChainStage::Audit,
            &EntityArtifactChainLink::from_header(&solve_header),
        ),
        certified_artifacts: certified_e2e_artifacts(&run.artifact, &review, &import_receipt),
        result: solve_header.clone(),
        suite: passing_mini_e2e_suite(),
    })
    .expect("audit passes before promotion");
    let audit_path = work_dir.join("audit.json");
    fs::write(
        &audit_path,
        serde_json::to_vec_pretty(&audit).expect("audit json"),
    )
    .expect("audit writes");
    logs.push(stage_log(
        "audit",
        run.artifact.next_commands.audit.clone(),
        0,
        &audit_path,
        &audit.version,
        &audit.artifact_content_hash,
        serde_json::to_value(&audit.summary).expect("audit summary json"),
    ));

    let stale_refusal = stale_audit_refusal(&solve_header, &run.artifact);
    assert_eq!(stale_refusal.code, RefusalCode::EEntityArtifactContract);

    logs.push(stage_log(
        "review_import",
        review_import_command(&run.artifact),
        0,
        &ledger_path,
        CANON_ENTITY_DECISION_LEDGER_VERSION,
        &import_receipt.last_event_hash,
        json!({
            "accepted_decisions": import_receipt.accepted_decisions,
            "ledger_path": import_receipt.ledger_path.display().to_string()
        }),
    ));

    let promotion_registry = temp.path().join("promotion-registry");
    write_empty_cmbs_registry(
        &promotion_registry,
        str_at(&manifest["promotion"], "registry_version_before"),
    );
    let promoted_aliases = promoted_aliases_from_manifest(&manifest);
    let promotion = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: promotion_registry.clone(),
        alias_file: "aliases.json".to_string(),
        next_version: str_at(&manifest["promotion"], "registry_version_after").to_string(),
        audit: audit.clone(),
        audit_expectation: promotion_expectation(&audit, &solve),
        aliases: promoted_aliases.clone(),
        no_lint: false,
    })
    .expect("promotion succeeds");
    let promote_path = work_dir.join("promote.json");
    fs::write(
        &promote_path,
        serde_json::to_vec_pretty(&promotion).expect("promotion json"),
    )
    .expect("promotion writes");
    assert_eq!(promotion.version, CANON_ENTITY_PROMOTE_VERSION);
    assert_eq!(promotion.aliases, promoted_aliases);
    assert_eq!(
        read_json::<Value>(&promotion_registry.join("aliases.json")),
        serde_json::to_value(&promotion.aliases).expect("promoted aliases json")
    );
    logs.push(stage_log(
        "promote",
        run.artifact.next_commands.promote.clone(),
        0,
        &promote_path,
        &promotion.version,
        &audit.artifact_content_hash,
        json!({
            "version_before": promotion.registry.version_before,
            "version_after": promotion.registry.version_after,
            "entry_count_after": promotion.registry.entry_count_after
        }),
    ));

    let apply_rows = work_dir.join("promoted-alias-rows.csv");
    let apply_output = work_dir.join("apply.csv");
    let raw_apply_rows = promoted_alias_apply_rows(&promotion.aliases);
    fs::write(&apply_rows, &raw_apply_rows).expect("apply rows write");
    let resolutions = promoted_apply_resolutions(&promotion.aliases);
    let apply = run_apply_streaming(ApplyStreamRequest {
        rows: &apply_rows,
        output: &apply_output,
        lookup_column: "raw_tenant_name",
        registry: ApplyRegistryReference {
            id: promotion.registry.id.clone(),
            version: promotion.registry.version_after.clone(),
        },
        resolutions: &resolutions,
        safety: ApplySafetyCheck {
            expected_profile_id: Some(solve.metadata.profile.id.clone()),
            actual_profile_id: Some(solve.metadata.profile.id.clone()),
            expected_identity_semantics: Some(solve.metadata.profile.identity_semantics.clone()),
            actual_identity_semantics: Some(solve.metadata.profile.identity_semantics.clone()),
            expected_registry_snapshot_hash: Some(
                solve
                    .metadata
                    .registry_snapshot
                    .lookup_snapshot_hash
                    .clone(),
            ),
            actual_registry_snapshot_hash: Some(
                solve
                    .metadata
                    .registry_snapshot
                    .lookup_snapshot_hash
                    .clone(),
            ),
            ..ApplySafetyCheck::default()
        },
        require_full_resolution: true,
        target_rows_per_chunk: 2,
    })
    .expect("apply succeeds");
    assert_eq!(
        fs::read_to_string(&apply_rows).expect("raw apply rows"),
        raw_apply_rows
    );
    assert_raw_fields_preserved(&apply_rows, &apply_output);
    assert_eq!(apply.version, CANON_ENTITY_APPLY_VERSION);
    assert_eq!(apply.summary["rows"], promotion.aliases.len() as u64);
    assert_eq!(apply.summary["resolved"], promotion.aliases.len() as u64);
    assert_eq!(apply.summary["unresolved"], 0);
    logs.push(stage_log(
        "apply",
        run.artifact.next_commands.apply.clone(),
        0,
        &apply_output,
        &apply.version,
        &apply.artifact_content_hash,
        serde_json::to_value(&apply.summary).expect("apply summary json"),
    ));

    logs.push(stage_log(
        "run_wrapper",
        run.artifact.next_commands.resume.clone(),
        0,
        &work_dir.join("run.json"),
        &run.artifact.version,
        &run.artifact.artifact_content_hash,
        run.artifact.summary_as_json(),
    ));

    let e2e_log = json!({
        "schema_version": "canon.entity.cmbs_mini_e2e_log.v0",
        "fixture_id": manifest["fixture_id"],
        "stages": logs,
        "refusals": [{
            "scenario": "stale_solve_artifact",
            "exit_code": 2,
            "code": refusal_code(&stale_refusal.code),
            "message": stale_refusal.message,
            "detail": stale_refusal.detail,
            "next_command": stale_refusal.next_command
        }]
    });
    let log_path = work_dir.join("e2e-log.json");
    fs::write(
        &log_path,
        serde_json::to_vec_pretty(&e2e_log).expect("log json"),
    )
    .expect("e2e log writes");

    assert_e2e_stage_log_contract(&read_json(&log_path), &manifest);
}

fn run_stage_logs(artifact: &EntityRunArtifact, work_dir: &Path) -> Vec<Value> {
    artifact
        .stage_artifacts
        .iter()
        .map(|stage| {
            let path = work_dir.join(&stage.path);
            stage_log(
                &stage.stage,
                format!("canon entity {} <stage-inputs>", stage.stage),
                0,
                &path,
                &stage.version,
                &stage.artifact_content_hash,
                artifact_summary(&path),
            )
        })
        .collect()
}

fn stage_log(
    stage: &str,
    command: String,
    exit_code: i32,
    artifact_path: &Path,
    artifact_version: &str,
    artifact_hash: &str,
    summary: Value,
) -> Value {
    json!({
        "stage": stage,
        "command": command,
        "exit_code": exit_code,
        "artifact_path": artifact_path.display().to_string(),
        "artifact_version": artifact_version,
        "artifact_hash": artifact_hash,
        "summary": summary
    })
}

fn artifact_summary(path: &Path) -> Value {
    read_json::<Value>(path)
        .get("summary")
        .cloned()
        .unwrap_or(Value::Null)
}

fn mini_e2e_review_queue(solve: &SolveArtifact) -> ReviewQueueArtifact {
    let review_items = vec![ReviewQueueItem {
        review_id: "review:cmbs-mini-e2e-sears-family".to_string(),
        ambiguity_key: "sears_family_distinct".to_string(),
        component_id: "component:cmbs-mini-e2e-sears-family".to_string(),
        state: SolveReconciliationState::Escrow,
        proposed_action: "confirm_merge_distinct_or_relation".to_string(),
        review_priority_units: 7_000,
        priority_reasons: vec![
            "high_deal_count".to_string(),
            "high_row_count".to_string(),
            "support_and_cannot_link".to_string(),
        ],
        affected_rows: 4,
        affected_deals: 4,
        surface_ids: vec!["surf:cmbs:sears".to_string(), "surf:cmbs:auto".to_string()],
        strongest_positive_cut: None,
        strongest_negative_cut: None,
        relation_hints: Vec::new(),
        provenance_samples: Vec::new(),
    }];
    ReviewQueueArtifact {
        version: CANON_ENTITY_REVIEW_QUEUE_VERSION.to_string(),
        artifact_content_hash: "blake3:cmbs-mini-e2e-review".to_string(),
        metadata: solve.metadata.clone(),
        summary: EntityDeterministicSummary {
            counts: BTreeMap::from([
                ("review_items".to_string(), review_items.len() as u64),
                ("review_group_count".to_string(), review_items.len() as u64),
                ("review_rows_covered".to_string(), 4),
                ("review_deals_covered".to_string(), 4),
            ]),
            labels: BTreeMap::from([
                ("grouping".to_string(), "ambiguity_pattern".to_string()),
                (
                    "include".to_string(),
                    "explicit_mini_e2e_fixture".to_string(),
                ),
            ]),
        },
        source_solve_hash: solve.artifact_content_hash.clone(),
        review_items,
    }
}

fn assert_repeated_review_ambiguity_grouped_once(review: &ReviewQueueArtifact, manifest: &Value) {
    let expected = &manifest["review"]["repeated_ambiguity"];
    let row_count = u64_at(expected, "row_count");
    let deal_count = u64_at(expected, "deal_count");
    let matches = review
        .review_items
        .iter()
        .filter(|item| item.affected_rows == row_count && item.affected_deals == deal_count)
        .collect::<Vec<_>>();

    assert_eq!(
        matches.len() as u64,
        u64_at(expected, "expected_group_count"),
        "repeated ambiguity should export once"
    );
    assert_eq!(
        review.review_items.len() as u64,
        u64_at(&manifest["review"], "expected_review_group_count")
    );
}

fn review_import_context(review: &ReviewQueueArtifact) -> ReviewImportContext {
    ReviewImportContext {
        metadata: review.metadata.clone(),
        source_review_queue_hash: review.artifact_content_hash.clone(),
        known_review_ids: review
            .review_items
            .iter()
            .map(|item| item.review_id.clone())
            .collect(),
        cannot_link_review_ids: review
            .review_items
            .iter()
            .filter(|item| item.strongest_negative_cut.is_some())
            .map(|item| item.review_id.clone())
            .collect(),
    }
}

fn review_import_decisions(review: &ReviewQueueArtifact) -> Vec<ReviewImportDecision> {
    review
        .review_items
        .iter()
        .map(|item| ReviewImportDecision {
            review_id: item.review_id.clone(),
            action: if item.strongest_negative_cut.is_some() {
                ReviewImportAction::DistinctConfirmed
            } else {
                ReviewImportAction::RelationConfirmed
            },
            operator_id: "operator:cmbs-mini-e2e".to_string(),
            source_review_queue_hash: review.artifact_content_hash.clone(),
            profile_id: review.metadata.profile.id.clone(),
            profile_version: review.metadata.profile.version.clone(),
            entity_type: Some(review.metadata.profile.entity_type.clone()),
            identity_semantics: Some(review.metadata.profile.identity_semantics.clone()),
            strategy_hash: review.metadata.strategy.content_hash.clone(),
            registry_snapshot_hash: review
                .metadata
                .registry_snapshot
                .lookup_snapshot_hash
                .clone(),
            surface_ids: item.surface_ids.clone(),
            reason_code: format!("cmbs_mini_e2e_{}", item.proposed_action),
            note: "CMBS mini e2e review decision".to_string(),
            override_approved_by: None,
            override_reason_code: None,
        })
        .collect()
}

fn solve_header(solve: &SolveArtifact) -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: solve.version.clone(),
        metadata: solve.metadata.clone(),
        summary: solve.summary.clone(),
    }
}

fn certified_e2e_artifacts(
    run: &EntityRunArtifact,
    review: &ReviewQueueArtifact,
    import_receipt: &canon::entity::review_import::ReviewImportReceipt,
) -> Vec<EntityArtifactReference> {
    let mut artifacts = run
        .stage_artifacts
        .iter()
        .map(|stage| EntityArtifactReference {
            version: stage.version.clone(),
            content_hash: stage.artifact_content_hash.clone(),
        })
        .collect::<Vec<_>>();
    artifacts.push(EntityArtifactReference {
        version: CANON_ENTITY_REVIEW_QUEUE_VERSION.to_string(),
        content_hash: review.artifact_content_hash.clone(),
    });
    artifacts.push(EntityArtifactReference {
        version: CANON_ENTITY_DECISION_LEDGER_VERSION.to_string(),
        content_hash: import_receipt.last_event_hash.clone(),
    });
    artifacts
}

fn passing_mini_e2e_suite() -> EntityAuditSuite {
    EntityAuditSuite {
        id: "cmbs_mini_e2e".to_string(),
        version: "2026.06.26".to_string(),
        gates: vec![
            gate("G08", "review grouping", "grouped_once", "grouped_once"),
            gate(
                "G09",
                "decision ledger continuity",
                "ledger_appended",
                "ledger_appended",
            ),
            gate(
                "G14",
                "promotion gate",
                "passing_audit_required",
                "passing_audit_required",
            ),
        ],
    }
}

fn gate(id: &str, label: &str, expected: &str, actual: &str) -> EntityAuditGateCheck {
    EntityAuditGateCheck {
        gate_id: id.to_string(),
        label: label.to_string(),
        passed: true,
        expected: expected.to_string(),
        actual: actual.to_string(),
        evidence: BTreeMap::new(),
    }
}

fn stale_audit_refusal(
    solve_header: &EntityArtifactHeader,
    run: &EntityRunArtifact,
) -> canon::Refusal {
    let mut expected = EntityArtifactChainExpectation::from_link(
        EntityChainStage::Audit,
        &EntityArtifactChainLink::from_header(solve_header),
    );
    expected.artifact_content_hash = Some("blake3:stale-solve-artifact".to_string());

    run_entity_audit(EntityAuditRequest {
        expected,
        certified_artifacts: run
            .stage_artifacts
            .iter()
            .map(|stage| EntityArtifactReference {
                version: stage.version.clone(),
                content_hash: stage.artifact_content_hash.clone(),
            })
            .collect(),
        result: solve_header.clone(),
        suite: passing_mini_e2e_suite(),
    })
    .expect_err("stale solve artifact refuses audit")
}

fn review_import_command(run: &EntityRunArtifact) -> String {
    run.orchestration
        .handoff_steps
        .iter()
        .find(|step| step.stage == "review_import")
        .map(|step| step.command.clone())
        .unwrap_or_else(|| "canon entity review import <review.csv>".to_string())
}

fn write_empty_cmbs_registry(registry: &Path, version: &str) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        format!(
            r#"{{"id":"cmbs-tenants","version":"{version}","description":"CMBS mini e2e promotion registry","updated":"2026-06-26","entry_count":0}}"#
        ),
    )
    .expect("registry metadata");
    fs::write(registry.join("aliases.json"), "[]\n").expect("aliases");
}

fn write_cmbs_mini_e2e_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"cmbs-tenants","version":"2026.06.26","description":"CMBS mini e2e partial registry","updated":"2026-06-26","entry_count":5}"#,
    )
    .expect("registry metadata");
    fs::write(
        registry.join("aliases.json"),
        serde_json::to_string_pretty(&json!([
            {"input":"24 Hour Fitness","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 HOUR FITNESS USA, INC.","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 HR Fitness","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"238 Sand Island Prop","canonical_id":"TNT-238-SAND-ISLAND-PROPERTY","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"238 SAND ISLAND PROPERTY LLC","canonical_id":"TNT-238-SAND-ISLAND-PROPERTY","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"}
        ]))
        .expect("aliases serialize"),
    )
    .expect("aliases");
}

fn refusal_code(code: &RefusalCode) -> String {
    serde_json::to_value(code)
        .expect("refusal code serializes")
        .as_str()
        .expect("refusal code string")
        .to_string()
}

fn promoted_aliases_from_manifest(manifest: &Value) -> Vec<EntityPromotedAlias> {
    serde_json::from_value(manifest["promotion"]["expected_aliases"].clone())
        .expect("promoted aliases parse")
}

fn promotion_expectation(
    audit: &canon::entity::audit::EntityAuditArtifact,
    solve: &SolveArtifact,
) -> EntityPromotionAuditExpectation {
    EntityPromotionAuditExpectation {
        audit_artifact_hash: audit.artifact_content_hash.clone(),
        audited_artifact_hash: solve.artifact_content_hash.clone(),
        profile_id: solve.metadata.profile.id.clone(),
        profile_version: solve.metadata.profile.version.clone(),
        strategy_hash: solve.metadata.strategy.content_hash.clone(),
        registry_snapshot_hash: solve
            .metadata
            .registry_snapshot
            .lookup_snapshot_hash
            .clone(),
        required_gate_ids: vec!["G08".to_string(), "G09".to_string(), "G14".to_string()],
    }
}

fn promoted_alias_apply_rows(aliases: &[EntityPromotedAlias]) -> String {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer
        .write_record(["loan_id", "raw_tenant_name", "as_reported_amount"])
        .expect("headers write");
    for (index, alias) in aliases.iter().enumerate() {
        writer
            .write_record([
                format!("L-{:03}", index + 1),
                alias.input.clone(),
                ((index + 1) * 10).to_string(),
            ])
            .expect("row writes");
    }
    String::from_utf8(writer.into_inner().expect("csv bytes")).expect("csv utf8")
}

fn promoted_apply_resolutions(
    aliases: &[EntityPromotedAlias],
) -> BTreeMap<String, ApplyCanonicalResolution> {
    aliases
        .iter()
        .map(|alias| {
            (
                alias.input.clone(),
                ApplyCanonicalResolution {
                    canonical_id: alias.canonical_id.clone(),
                    canonical_type: alias.canonical_type.clone(),
                    rule_id: "REGISTRY_EXACT".to_string(),
                },
            )
        })
        .collect()
}

fn assert_e2e_stage_log_contract(log: &Value, manifest: &Value) {
    assert_eq!(log["schema_version"], "canon.entity.cmbs_mini_e2e_log.v0");
    let stages = log["stages"].as_array().expect("stage logs");
    let actual_order = stages
        .iter()
        .map(|stage| str_at(stage, "stage").to_string())
        .collect::<Vec<_>>();
    let expected_order = manifest["stage_order"]
        .as_array()
        .expect("stage order")
        .iter()
        .map(|stage| stage.as_str().expect("stage").to_string())
        .collect::<Vec<_>>();
    assert_eq!(actual_order, expected_order);

    let expected_versions = manifest["expected_stage_versions"]
        .as_object()
        .expect("expected stage versions");
    for stage in stages {
        let stage_name = str_at(stage, "stage");
        let command = str_at(stage, "command");
        assert!(
            command.starts_with("canon entity "),
            "{stage_name} command should be operator-runnable: {command}"
        );
        assert_eq!(stage["exit_code"], 0, "{stage_name} exit code");
        assert!(!str_at(stage, "artifact_path").trim().is_empty());
        assert_eq!(
            &stage["artifact_version"],
            expected_versions
                .get(stage_name)
                .unwrap_or_else(|| panic!("missing expected version for {stage_name}"))
        );
        assert!(
            str_at(stage, "artifact_hash").starts_with("blake3:"),
            "{stage_name} missing artifact hash"
        );
        assert!(stage["summary"].is_object(), "{stage_name} missing summary");
    }

    let refusals = log["refusals"].as_array().expect("refusal logs");
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(refusals[0]["exit_code"], 2);
    assert!(refusals[0]["detail"].is_object());
    assert!(
        refusals[0]["next_command"]
            .as_str()
            .is_some_and(|command| !command.trim().is_empty())
    );
}

fn assert_source_summary(rows: &[BTreeMap<String, String>], expected: &Value) {
    assert_eq!(rows.len() as u64, u64_at(&expected["source"], "rows"));
    assert_eq!(
        unique_count(rows, "deal_id"),
        u64_at(&expected["source"], "deals")
    );
    assert_eq!(
        unique_count(rows, "property_id"),
        u64_at(&expected["source"], "properties")
    );
    assert_eq!(
        unique_count(rows, "raw_tenant_name"),
        u64_at(&expected["source"], "raw_unique_names")
    );
}

fn assert_expected_surfaces(small_book: &Value, expected: &Value) {
    assert_eq!(
        small_book["prepare_summary"]["normalized_unique_surfaces"],
        expected["prepared_surfaces"]["normalized_expected_count"]
    );
    assert_eq!(
        small_book["prepare_summary"]["exact_resolved_surface_count"],
        expected["prepared_surfaces"]["exact_resolved_surface_count"]
    );
    assert_eq!(
        small_book["prepare_summary"]["global_surface_scope"],
        expected["prepared_surfaces"]["global_surface_scope"]
    );
    assert_eq!(
        small_book["exact_resolved_surfaces"],
        expected["exact_resolved_surfaces"]
    );
}

fn assert_operator_rollups(
    rows: &[BTreeMap<String, String>],
    small_book: &Value,
    expected: &Value,
) {
    assert_eq!(
        top_unresolved_tokens(rows, 3),
        count_pairs(
            &expected["operator_summary"]["top_unresolved_tokens"],
            "token"
        )
    );
    assert_eq!(
        anti_merge_reason_counts(small_book),
        count_pairs(
            &expected["operator_summary"]["top_anti_merge_reasons"],
            "reason_code"
        )
    );
}

fn assert_review_groups(expected: &Value) {
    let queue = csv_rows(&fixture(REVIEW_QUEUE_PATH));
    let expected_groups = expected["review"]["groups"]
        .as_array()
        .expect("review groups")
        .iter()
        .map(|group| (str_at(group, "id").to_string(), group))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        queue.len() as u64,
        u64_at(&expected["review"], "group_count")
    );
    assert_eq!(
        queue
            .iter()
            .map(|row| parse_u64(row, "row_count"))
            .sum::<u64>(),
        u64_at(&expected["review"], "rows_covered")
    );

    for row in queue {
        let group_id = row.get("review_group_id").expect("review_group_id");
        let expected_group = expected_groups
            .get(group_id)
            .unwrap_or_else(|| panic!("unexpected review group {group_id}"));
        assert_eq!(row["reason_code"], str_at(expected_group, "reason_code"));
        assert_eq!(
            parse_u64(&row, "row_count"),
            u64_at(expected_group, "row_count")
        );
        assert_eq!(
            parse_u64(&row, "deal_count"),
            u64_at(expected_group, "deal_count")
        );
        assert_eq!(
            row["suggested_action"],
            str_at(expected_group, "suggested_action")
        );
    }
}

fn assert_promotable_aliases(expected: &Value) {
    let promotion = json_fixture(PROMOTION_MANIFEST_PATH);
    assert_eq!(
        promotion["expected_aliases"],
        expected["promotion"]["promotable_aliases"]
    );
}

fn assert_run_summary(summary: &Value, expected: &Value) {
    assert_eq!(summary["counts"]["row_count"], expected["source"]["rows"]);
    assert_eq!(
        summary["counts"]["prepared_surfaces"],
        expected["run_artifact"]["prepared_surfaces"]
    );
    assert_eq!(
        summary["counts"]["exact_resolved_surfaces"],
        expected["run_artifact"]["exact_resolved_surfaces"]
    );
    assert_eq!(summary["labels"]["profile_id"], expected["profile_id"]);
    assert_eq!(summary["labels"]["registry_id"], "cmbs-tenants");
    assert_eq!(summary["labels"]["registry_version"], "2026.06.26");
}

fn assert_stage_artifacts(work_dir: &Path, expected: &Value) {
    let surfaces = jsonl_values(&work_dir.join("prepare/surfaces.jsonl"));
    assert_eq!(
        surfaces.len() as u64,
        u64_at(&expected["run_artifact"], "prepared_surfaces")
    );
    let exact_surfaces = surfaces
        .iter()
        .filter(|surface| surface["exact_lookup"]["status"] == "resolved")
        .collect::<Vec<_>>();
    assert_eq!(
        exact_surfaces.len() as u64,
        u64_at(&expected["run_artifact"], "exact_resolved_surfaces")
    );
    let exact_ids = exact_surfaces
        .iter()
        .filter_map(|surface| surface["exact_lookup"]["canonical_id"].as_str())
        .collect::<BTreeSet<_>>();
    for surface in expected["exact_resolved_surfaces"]
        .as_array()
        .expect("exact surfaces")
    {
        assert!(
            exact_ids.contains(str_at(surface, "canonical_id")),
            "missing exact canonical id {}",
            str_at(surface, "canonical_id")
        );
    }

    let index = json_file(&work_dir.join("index.json"));
    assert_eq!(
        index["summary"]["labels"]["cache_status"],
        expected["cache"]["index_status"]
    );
}

fn assert_next_commands(next_commands: &Value, expected: &Value) {
    for command_name in strings(&expected["operator_summary"]["next_commands"]) {
        let command = next_commands
            .get(command_name.as_str())
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("missing next command {command_name}"));
        assert!(
            command.starts_with("canon entity "),
            "{command_name} next command should be operator-runnable: {command}"
        );
    }
}

fn assert_apply_replay(rows: &[BTreeMap<String, String>], temp_root: &Path, expected: &Value) {
    let output = temp_root.join("cmbs-small-book.canon.csv");
    let rows_path = fixture(OBSERVATIONS_PATH);
    let resolutions = apply_resolutions(rows);
    let request = apply_request(&rows_path, &output, &resolutions);
    let first = run_apply_streaming(request).expect("first e2e apply succeeds");
    let first_bytes = fs::read(&output).expect("first apply bytes");
    let second = run_apply_streaming(apply_request(&rows_path, &output, &resolutions))
        .expect("second apply succeeds");
    let second_bytes = fs::read(&output).expect("second apply bytes");

    assert_eq!(first_bytes, second_bytes);
    assert_eq!(first.artifact_content_hash, second.artifact_content_hash);
    assert_eq!(first.summary["rows"], u64_at(&expected["apply"], "rows"));
    assert_eq!(
        first.summary["resolved"],
        u64_at(&expected["apply"], "resolved")
    );
    assert_eq!(
        first.summary["unresolved"],
        u64_at(&expected["apply"], "unresolved")
    );
    assert_eq!(
        fs::read_to_string(&output).expect("apply output"),
        fs::read_to_string(fixture(E2E_APPLY_EXPECTED_PATH)).expect("expected apply output")
    );
    assert_raw_fields_preserved(&fixture(OBSERVATIONS_PATH), &output);
    assert_eq!(
        strings(&expected["apply"]["canonical_fields"]),
        APPLY_CANONICAL_FIELDS
            .iter()
            .map(|field| (*field).to_string())
            .collect::<BTreeSet<_>>()
    );
}

fn apply_request<'a>(
    rows: &'a Path,
    output: &'a Path,
    resolutions: &'a BTreeMap<String, ApplyCanonicalResolution>,
) -> ApplyStreamRequest<'a> {
    ApplyStreamRequest {
        rows,
        output,
        lookup_column: "raw_tenant_name",
        registry: ApplyRegistryReference {
            id: "cmbs-tenants".to_string(),
            version: "2026.06.26".to_string(),
        },
        resolutions,
        safety: ApplySafetyCheck {
            expected_profile_id: Some("cmbs_tenant_label".to_string()),
            actual_profile_id: Some("cmbs_tenant_label".to_string()),
            expected_identity_semantics: Some("canonical_display_label".to_string()),
            actual_identity_semantics: Some("canonical_display_label".to_string()),
            expected_registry_snapshot_hash: Some("blake3:cmbs-e2e-registry".to_string()),
            actual_registry_snapshot_hash: Some("blake3:cmbs-e2e-registry".to_string()),
            expected_sidecar_artifact_version: Some(
                "canon_entity_promotion_sidecar.v0".to_string(),
            ),
            actual_sidecar_artifact_version: Some("canon_entity_promotion_sidecar.v0".to_string()),
            expected_sidecar_snapshot_hash: Some("blake3:cmbs-e2e-sidecars".to_string()),
            actual_sidecar_snapshot_hash: Some("blake3:cmbs-e2e-sidecars".to_string()),
        },
        require_full_resolution: false,
        target_rows_per_chunk: 5,
    }
}

fn apply_resolutions(
    rows: &[BTreeMap<String, String>],
) -> BTreeMap<String, ApplyCanonicalResolution> {
    rows.iter()
        .filter(|row| row["expected_resolution_status"] == "exact_resolved")
        .map(|row| {
            (
                row["raw_tenant_name"].clone(),
                ApplyCanonicalResolution {
                    canonical_id: row["expected_canonical_id"].clone(),
                    canonical_type: "tenant_label".to_string(),
                    rule_id: "CMBS_ALIAS".to_string(),
                },
            )
        })
        .collect()
}

fn assert_raw_fields_preserved(input: &Path, output: &Path) {
    let input_rows = csv_rows(input);
    let output_rows = csv_rows(output);
    assert_eq!(input_rows.len(), output_rows.len());
    for (input, output) in input_rows.iter().zip(output_rows.iter()) {
        for (key, value) in input {
            assert_eq!(output.get(key), Some(value), "raw field {key} changed");
        }
    }
}

fn top_unresolved_tokens(rows: &[BTreeMap<String, String>], limit: usize) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::<String, u64>::new();
    for row in rows
        .iter()
        .filter(|row| row["expected_resolution_status"] != "exact_resolved")
    {
        for token in row["expected_normalized_surface"].split_whitespace() {
            if token.starts_with("placeholder:") {
                continue;
            }
            *counts.entry(token.to_string()).or_default() += 1;
        }
    }
    let mut ordered = counts.into_iter().collect::<Vec<_>>();
    ordered.sort_by(|(left_token, left_count), (right_token, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| left_token.cmp(right_token))
    });
    ordered.into_iter().take(limit).collect()
}

fn anti_merge_reason_counts(small_book: &Value) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for pair in small_book["hard_negative_pairs"]
        .as_array()
        .expect("hard negative pairs")
    {
        *counts
            .entry(str_at(pair, "reason_code").to_string())
            .or_default() += 1;
    }
    counts
}

fn count_pairs(value: &Value, key: &str) -> BTreeMap<String, u64> {
    value
        .as_array()
        .expect("count pair array")
        .iter()
        .map(|entry| (str_at(entry, key).to_string(), u64_at(entry, "count")))
        .collect()
}

fn csv_rows(path: &Path) -> Vec<BTreeMap<String, String>> {
    let mut reader = csv::Reader::from_path(path).expect("csv opens");
    reader
        .deserialize::<BTreeMap<String, String>>()
        .collect::<Result<Vec<_>, _>>()
        .expect("csv parses")
}

fn jsonl_values(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("jsonl reads")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("jsonl value parses"))
        .collect()
}

fn json_fixture(relative: &str) -> Value {
    json_file(&fixture(relative))
}

fn json_file(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}

fn read_json<T: DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}

fn unique_count(rows: &[BTreeMap<String, String>], field: &str) -> u64 {
    rows.iter()
        .map(|row| row.get(field).expect("field").as_str())
        .collect::<BTreeSet<_>>()
        .len() as u64
}

fn strings(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|item| item.as_str().expect("string").to_string())
        .collect()
}

fn str_at<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("missing {key}"))
}

fn u64_at(value: &Value, key: &str) -> u64 {
    value[key]
        .as_u64()
        .unwrap_or_else(|| panic!("missing {key}"))
}

fn parse_u64(row: &BTreeMap<String, String>, key: &str) -> u64 {
    row[key]
        .parse::<u64>()
        .unwrap_or_else(|error| panic!("{key} should be u64: {error}"))
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn write_cmbs_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"cmbs-tenants","version":"2026.06.26","description":"CMBS e2e test registry","updated":"2026-06-26","entry_count":8}"#,
    )
    .expect("registry metadata");
    fs::write(
        registry.join("aliases.json"),
        serde_json::to_string_pretty(&json!([
            {"input":"Sears","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"SEARS LLC","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"Sears Roebuck & Co.","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 Hour Fitness","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 HOUR FITNESS USA, INC.","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 HR Fitness","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"238 Sand Island Prop","canonical_id":"TNT-238-SAND-ISLAND-PROPERTY","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"238 SAND ISLAND PROPERTY LLC","canonical_id":"TNT-238-SAND-ISLAND-PROPERTY","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"}
        ]))
        .expect("aliases serialize"),
    )
    .expect("aliases");
}

trait EntityRunSummaryJson {
    fn summary_as_json(&self) -> Value;
    fn next_commands_as_json(&self) -> Value;
}

impl EntityRunSummaryJson for canon::entity::run::EntityRunArtifact {
    fn summary_as_json(&self) -> Value {
        serde_json::to_value(&self.summary).expect("summary json")
    }

    fn next_commands_as_json(&self) -> Value {
        serde_json::to_value(&self.next_commands).expect("next commands json")
    }
}
