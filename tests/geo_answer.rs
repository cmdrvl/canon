//! Retained projection integration. No fresh acquisition, held-out accuracy, policy
//! acceptance or E5 claim is made by these tests. Synthetic mutations are named.
#[path = "common/geo_answer_fixture.rs"]
mod fixture;

use canon::geo::*;
use fixture::retained_answer_fixture;
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path, process::Command};

fn value(case: &str) -> Value {
    serde_json::to_value(retained_answer_fixture(case).answer()).unwrap()
}
fn preferred(answer: &Value, view: &str) -> BTreeSet<String> {
    answer["preference"][view]["preferred"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|model| {
            model["members"]["parcels"]
                .as_array()
                .unwrap()
                .iter()
                .chain(model["members"]["buildings"].as_array().unwrap())
                .map(|m| m.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        })
        .collect()
}
fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).into()).collect()
}
fn support<'a>(answer: &'a Value, id: &str) -> &'a Value {
    answer["association_support"]["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["member"]["id"] == id)
        .unwrap_or_else(|| panic!("missing member support {id}"))
}
fn assert_unaccepted(answer: &Value) {
    assert_eq!(answer["acceptance"]["status"], "no_policy_declared");
    assert_eq!(answer["preference"]["calibration"], "uncalibrated");
    assert!(
        !answer["extent_and_reach"]["complete_extent_claimed"]
            .as_bool()
            .unwrap()
    );
    fn walk(value: &Value) {
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    assert!(
                        ![
                            "accepted",
                            "probability",
                            "percent",
                            "percentage",
                            "confidence",
                            "expected_value"
                        ]
                        .contains(&key.as_str()),
                        "unsupported claim key {key}"
                    );
                    walk(value);
                }
            }
            Value::Array(values) => {
                for value in values {
                    walk(value);
                }
            }
            _ => {}
        }
    }
    walk(answer);
}
fn assert_both(answer: &Value, ids: &[&str], cost: u64, next: u64, residual: u64) {
    assert_eq!(answer["preference"]["state"]["kind"], "ranked");
    for view in ["composition_view", "grouped_view"] {
        assert_eq!(preferred(answer, view), set(ids));
        assert_eq!(answer["preference"][view]["tie_count"], ids.len() as u64);
        assert_eq!(answer["preference"][view]["next_best_cost"], next);
        for model in answer["preference"][view]["preferred"].as_array().unwrap() {
            assert_eq!(model["cost"], cost);
        }
    }
    assert_eq!(
        answer["preference"]["grouped_view"]["rule_id"],
        "contract_lineage_dedupe.v0"
    );
    assert_eq!(
        answer["logical_consequence"]["residual_model_count"],
        residual
    );
    assert_eq!(
        answer["logical_consequence"]["hard_forced"],
        json!({"parcels":[],"buildings":[]})
    );
    assert_unaccepted(answer);
}

#[test]
fn ans_01_retained_parcel_preference_and_mirrored_support_are_visible() {
    let answer = value("cornerstone.national_parcel_supported");
    assert_both(&answer, &["302304550403000"], 0, 2, 2983);
    assert_eq!(answer["preference"]["lineage_dependent"], false);
    let member = support(&answer, "302304550403000");
    assert_eq!(member["independent_support_groups"], 1);
    assert_eq!(member["soft"].as_array().unwrap().len(), 2);
    let serialized = member.to_string();
    for source in [
        "foursquare:4c61282fb6f3be9aa7f36073",
        "overture.places:7bef8d4a-8fe2-4ae5-adae-02db204536bb",
        "orange.address:367667",
        "bh-property-operator",
        "orange-county-property-appraiser",
    ] {
        assert!(
            serialized.contains(source),
            "missing retained source {source}"
        );
    }
    assert_eq!(answer["claim_label"]["kind"], "single_member_association");
    assert_eq!(answer["further_work"]["source"], "not_in_scope");
    assert_eq!(answer["further_work"]["action_discovery"], "not_attempted");
}

#[test]
fn ans_02_retained_buildings_are_an_exact_tie_not_positional_rank_one() {
    let typed = retained_answer_fixture("cornerstone.national_building_supported").answer();
    let bytes = canonical_answer_bytes(&typed).unwrap();
    let readback: GeoAnswerArtifact = serde_json::from_slice(&bytes).unwrap();
    validate_answer_artifact(&readback).unwrap();
    assert_eq!(
        canonical_answer_bytes(&readback).unwrap(),
        bytes,
        "real retained answer must survive typed canonical readback"
    );
    let answer = value("cornerstone.national_building_supported");
    assert_both(
        &answer,
        &[
            "0a5045cd-8321-429b-b0b7-0f3f24b6e615",
            "a6145c70-10cb-46d0-95b8-b85ff82ac700",
        ],
        1,
        2,
        347,
    );
    assert_eq!(answer["preference"]["lineage_dependent"], false);
    for id in [
        "0a5045cd-8321-429b-b0b7-0f3f24b6e615",
        "a6145c70-10cb-46d0-95b8-b85ff82ac700",
    ] {
        assert_eq!(support(&answer, id)["independent_support_groups"], 1);
    }
    let support = answer["association_support"].to_string();
    assert!(support.contains("OpenStreetMap") && support.contains("bh-property-operator"));
}

#[test]
fn ans_03_retained_brooklyn_forcing_preserves_optional_neighborhood_and_unknown_extent() {
    let answer = value("brooklyn.02_address_building");
    assert_eq!(
        answer["logical_consequence"]["hard_forced"],
        json!({"parcels":["3051090025"],"buildings":["3117371"]})
    );
    assert!(answer["logical_consequence"]["residual_model_count"].is_null());
    assert_eq!(
        answer["logical_consequence"]["residual_model_count_saturated"],
        true
    );
    assert_eq!(answer["preference"]["state"]["kind"], "no_soft_evidence");
    assert_eq!(
        support(&answer, "3051090025")["independent_support_groups"],
        1
    );
    assert_eq!(support(&answer, "3117371")["independent_support_groups"], 1);
    assert_eq!(
        answer["logical_consequence"]["hard_forced"]["parcels"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_unaccepted(&answer);
}

#[test]
fn ans_03b_retained_failed_execution_is_distinct_from_its_useful_forced_association() {
    let fixture = retained_answer_fixture("brooklyn.02_address_building");
    let answer = serde_json::to_value(fixture.answer_with_execution()).unwrap();
    assert_eq!(answer["execution"]["source"], "verified_run_snapshot");
    assert_eq!(answer["execution"]["actual_runner_history_available"], true);
    let snapshot = &answer["execution"]["run_snapshot"];
    assert_eq!(snapshot["status"], "FAILED");
    assert_eq!(snapshot["phase"], "SOLVED");
    assert_eq!(
        snapshot["completed_project_node_ids"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
    assert_eq!(
        snapshot["failed_project_node_ids"],
        json!(["geo.parcel.separation"])
    );
    assert_eq!(
        snapshot["blocked_project_node_ids"],
        json!(["geo.parcel.next_evidence"])
    );
    assert_eq!(snapshot["cancelled_project_node_ids"], json!([]));
    assert_eq!(
        snapshot["receipt_body_verified_project_node_ids"],
        snapshot["completed_project_node_ids"]
    );
    assert_eq!(
        snapshot["report_only_terminal_project_node_ids"],
        json!(["geo.parcel.separation"])
    );
    for field in [
        "receipt_body_verified_project_node_ids",
        "report_only_terminal_project_node_ids",
    ] {
        assert!(
            !snapshot[field]
                .as_array()
                .unwrap()
                .contains(&json!("geo.parcel.next_evidence"))
        );
    }
    assert_eq!(
        answer["logical_consequence"]["hard_forced"],
        json!({"parcels":["3051090025"],"buildings":["3117371"]})
    );
    assert_unaccepted(&answer);
}

#[test]
fn ans_03c_retained_successful_execution_is_verified_and_unknown_history_stays_unknown() {
    let fixture = retained_answer_fixture("cornerstone.national_building_supported");
    let answer = serde_json::to_value(fixture.answer_with_execution()).unwrap();
    let snapshot = &answer["execution"]["run_snapshot"];
    assert_eq!(snapshot["status"], "ABSTAINED");
    assert_eq!(
        snapshot["completed_project_node_ids"]
            .as_array()
            .unwrap()
            .len(),
        9
    );
    assert_eq!(snapshot["failed_project_node_ids"], json!([]));
    assert_eq!(snapshot["blocked_project_node_ids"], json!([]));
    assert_eq!(
        snapshot["receipt_body_verified_project_node_ids"],
        snapshot["completed_project_node_ids"]
    );
    assert_eq!(snapshot["report_only_terminal_project_node_ids"], json!([]));
    assert_eq!(
        snapshot["run_semantic_hash"],
        fixture.retained_run.semantic_hash
    );
    assert_eq!(snapshot["composition_budget_fallback"], false);
    let unknown = serde_json::to_value(fixture.answer()).unwrap();
    assert_eq!(
        unknown["execution"]["actual_runner_history_available"],
        false
    );
    assert!(unknown["execution"]["run_snapshot"].is_null());
    assert!(
        answer["execution"]["consumed_stages"]
            .as_array()
            .unwrap()
            .len()
            < 9,
        "four supplied artifacts do not describe nine actual executed stages"
    );
    let pure_snapshot =
        serde_json::to_value(project_answer_execution(&fixture.retained_run).unwrap()).unwrap();
    assert!(
        pure_snapshot["composition_budget_fallback"].is_null(),
        "a run-only projection has no composition input from which to claim composition fallback"
    );
    assert_both(
        &answer,
        &[
            "0a5045cd-8321-429b-b0b7-0f3f24b6e615",
            "a6145c70-10cb-46d0-95b8-b85ff82ac700",
        ],
        1,
        2,
        347,
    );
}

#[test]
fn ans_04_synthetic_duplicate_contract_changes_primary_not_diagnostic_tie() {
    let mut fixture = retained_answer_fixture("cornerstone.national_building_supported");
    let mut duplicate = fixture
        .compilation
        .admissions
        .iter()
        .find(|a| a.observation_id == "national:foursquare")
        .unwrap()
        .clone();
    duplicate.observation_id = "synthetic:duplicate-observation".into();
    let generated = format!(
        "rho:{}@{}:{}",
        duplicate.contract.id, duplicate.contract.version, duplicate.observation_id
    );
    duplicate.generated_ids = vec![generated.clone()];
    let mut preference = fixture
        .compilation
        .composition_request
        .soft_preferences
        .iter()
        .find(|p| p.member.id == "a6145c70-10cb-46d0-95b8-b85ff82ac700")
        .unwrap()
        .clone();
    preference.id = generated;
    fixture
        .compilation
        .composition_request
        .soft_preferences
        .push(preference);
    fixture.compilation.admissions.push(duplicate);
    for ranked in &mut fixture.composition.soft_ranked {
        if !ranked
            .model
            .buildings
            .contains(&"a6145c70-10cb-46d0-95b8-b85ff82ac700".to_string())
        {
            ranked.cost += 1;
        }
    }
    fixture
        .composition
        .soft_ranked
        .sort_by(|a, b| a.cost.cmp(&b.cost).then_with(|| a.model.cmp(&b.model)));
    for (index, ranked) in fixture.composition.soft_ranked.iter_mut().enumerate() {
        ranked.rank = index as u64 + 1;
    }
    fixture.rebind_changed_evidence();
    let answer = serde_json::to_value(fixture.answer()).unwrap();
    assert_eq!(
        preferred(&answer, "composition_view"),
        set(&["a6145c70-10cb-46d0-95b8-b85ff82ac700"])
    );
    assert_eq!(
        preferred(&answer, "grouped_view"),
        set(&[
            "0a5045cd-8321-429b-b0b7-0f3f24b6e615",
            "a6145c70-10cb-46d0-95b8-b85ff82ac700"
        ])
    );
    assert_eq!(
        answer["preference"]["composition_view"]["preferred"][0]["cost"],
        1
    );
    assert_eq!(
        answer["preference"]["composition_view"]["next_best_cost"],
        2
    );
    assert_eq!(answer["preference"]["lineage_dependent"], true);
    assert_eq!(
        support(&answer, "a6145c70-10cb-46d0-95b8-b85ff82ac700")["independent_support_groups"],
        1
    );
    assert_unaccepted(&answer);
}

#[test]
fn ans_05_synthetic_unmaterialized_models_preserve_support_without_inventing_order() {
    let mut fixture = retained_answer_fixture("cornerstone.national_building_supported");
    fixture.composition.summary.residual_models_materialized = false;
    fixture.composition.residual_models.clear();
    fixture.composition.soft_ranked.clear();
    let answer = serde_json::to_value(fixture.answer()).unwrap();
    assert_eq!(
        answer["preference"]["state"],
        json!({"kind":"unavailable","reason":"residual_models_not_materialized"})
    );
    assert!(preferred(&answer, "composition_view").is_empty());
    assert!(preferred(&answer, "grouped_view").is_empty());
    assert!(
        !support(&answer, "a6145c70-10cb-46d0-95b8-b85ff82ac700")["soft"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_unaccepted(&answer);
}

#[test]
fn ans_06_policy_reference_is_a_slot_not_an_acceptance_decision() {
    let mut fixture = retained_answer_fixture("cornerstone.national_building_supported");
    assert_unaccepted(&serde_json::to_value(fixture.answer()).unwrap());
    fixture.config.decision_policy = Some(GeoDecisionPolicyRef {
        policy_id: "synthetic-policy".into(),
        version: "v0".into(),
        content_hash: format!("blake3:{}", "a".repeat(64)),
    });
    let answer = serde_json::to_value(fixture.answer()).unwrap();
    assert_eq!(answer["acceptance"]["status"], "policy_not_evaluated");
    assert_eq!(
        answer["acceptance"]["policy"]["policy_id"],
        "synthetic-policy"
    );
    assert!(answer["acceptance"].get("accepted").is_none());
}

#[test]
fn ans_07_mismatched_explanation_digest_refuses_instead_of_splicing_support() {
    let mut fixture = retained_answer_fixture("cornerstone.national_building_supported");
    fixture.explanation.evidence_blake3 = format!("blake3:{}", "0".repeat(64));
    let error = project_answer(
        &fixture.inputs(),
        &fixture.config,
        &GeoAnswerLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, GeoAnswerErrorCode::AnswerInputMismatch);
    assert!(
        error.detail.contains_key("artifact")
            && error.detail.contains_key("expected_blake3")
            && error.detail.contains_key("actual_blake3")
    );
}

#[test]
fn ans_07b_diagnostic_without_soft_evidence_cannot_fabricate_an_exact_count() {
    let mut fixture = retained_answer_fixture("cornerstone.national_building");
    assert!(
        fixture
            .compilation
            .composition_request
            .soft_preferences
            .is_empty()
    );
    assert_eq!(fixture.composition.residual_models.len(), 347);
    fixture.composition.summary.residual_model_count = 348;
    let error = project_answer(
        &fixture.inputs(),
        &fixture.config,
        &GeoAnswerLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, GeoAnswerErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("composition.summary.residual_model_count")
    );
}

#[test]
fn ans_07c_unmaterialized_models_do_not_authorize_an_unknown_forced_building() {
    let mut fixture = retained_answer_fixture("cornerstone.national_building_supported");
    fixture.composition.summary.residual_models_materialized = false;
    fixture.composition.residual_models.clear();
    fixture.composition.soft_ranked.clear();
    assert!(
        project_answer(
            &fixture.inputs(),
            &fixture.config,
            &GeoAnswerLimits::default()
        )
        .is_ok(),
        "valid unavailable preference remains a useful projection"
    );
    fixture
        .composition
        .hard_forced
        .buildings
        .push("synthetic-absent-building".into());
    let error = project_answer(
        &fixture.inputs(),
        &fixture.config,
        &GeoAnswerLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, GeoAnswerErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("composition.hard_forced.buildings")
    );
}

fn restamp_run(run: &mut GeoRun) {
    run.semantic_hash = geo_run_semantic_hash(run).unwrap();
    run.run_id = format!(
        "{CANON_GEO_RUN_VERSION}:{}",
        run.semantic_hash.trim_start_matches("blake3:")
    );
}

#[test]
fn ans_07d_actual_run_history_must_bind_the_question_and_exact_supplied_outputs() {
    let fixture = retained_answer_fixture("cornerstone.national_building_supported");
    let mut wrong_question = fixture.config.clone();
    wrong_question.question_hash = format!("blake3:{}", "f".repeat(64));
    let error = project_answer_with_execution(
        &fixture.inputs(),
        &wrong_question,
        &GeoAnswerLimits::default(),
        &fixture.retained_run,
    )
    .unwrap_err();
    assert_eq!(error.code, GeoAnswerErrorCode::AnswerInputMismatch);
    assert!(error.detail["artifact"].starts_with("execution.question"));
    let different = retained_answer_fixture("cornerstone.national_building");
    assert_eq!(
        fixture.retained_run.plan_ref.question_hash,
        different.retained_run.plan_ref.question_hash
    );
    let error = project_answer_with_execution(
        &fixture.inputs(),
        &fixture.config,
        &GeoAnswerLimits::default(),
        &different.retained_run,
    )
    .unwrap_err();
    assert_eq!(error.code, GeoAnswerErrorCode::AnswerInputMismatch);
    assert!(error.detail["artifact"].starts_with("execution.output."));
    let mut wrong_bytes = fixture.retained_run.clone();
    wrong_bytes
        .output_refs
        .iter_mut()
        .find(|r| r.output_id == "compile_evidence")
        .unwrap()
        .byte_count += 1;
    restamp_run(&mut wrong_bytes);
    let error = project_answer_with_execution(
        &fixture.inputs(),
        &fixture.config,
        &GeoAnswerLimits::default(),
        &wrong_bytes,
    )
    .unwrap_err();
    assert_eq!(error.code, GeoAnswerErrorCode::AnswerInputMismatch);
    assert!(error.detail["artifact"].starts_with("execution.output."));
}

#[test]
fn ans_07e_duplicate_output_or_forged_project_report_is_not_verified_execution() {
    let fixture = retained_answer_fixture("cornerstone.national_building_supported");
    let mut duplicate = fixture.retained_run.clone();
    duplicate.output_refs.push(duplicate.output_refs[0].clone());
    restamp_run(&mut duplicate);
    let error = project_answer_with_execution(
        &fixture.inputs(),
        &fixture.config,
        &GeoAnswerLimits::default(),
        &duplicate,
    )
    .unwrap_err();
    assert_eq!(error.code, GeoAnswerErrorCode::InvalidInput);
    let mut forged = fixture.retained_run.clone();
    forged
        .project_run_report
        .as_mut()
        .unwrap()
        .executed_nodes
        .clear();
    restamp_run(&mut forged);
    let error = project_answer_with_execution(
        &fixture.inputs(),
        &fixture.config,
        &GeoAnswerLimits::default(),
        &forged,
    )
    .unwrap_err();
    assert_eq!(error.code, GeoAnswerErrorCode::InvalidInput);
}

#[test]
fn ans_07f_retained_failed_history_requires_unique_terminal_report_metadata() {
    use canon::project::ProjectRunNodeOutcome;
    let fixture = retained_answer_fixture("brooklyn.02_address_building");
    // The original failed run is a positive: seven verified bodies plus one
    // report-only failure, rather than a fabricated completed workflow.
    fixture.answer_with_execution();
    for mutation in 0..5 {
        let mut forged = fixture.retained_run.clone();
        let reports = &mut forged.project_run_report.as_mut().unwrap().node_reports;
        let index = reports
            .iter()
            .position(|node| node.node_id == "geo.parcel.separation")
            .unwrap();
        match mutation {
            0 => {
                reports.remove(index);
            }
            1 => reports.push(reports[index].clone()),
            2 => reports[index].outcome = ProjectRunNodeOutcome::Completed,
            3 => reports[index].receipt_hash = None,
            4 => reports[index].receipt_hash = Some("blake3:malformed".into()),
            _ => unreachable!(),
        }
        restamp_run(&mut forged);
        let error = project_answer_with_execution(
            &fixture.inputs(),
            &fixture.config,
            &GeoAnswerLimits::default(),
            &forged,
        )
        .unwrap_err();
        assert_eq!(
            error.code,
            GeoAnswerErrorCode::InvalidInput,
            "mutation {mutation}"
        );
    }
}

#[test]
fn ans_07g_report_only_failure_cannot_be_promoted_to_body_verified_scope() {
    let fixture = retained_answer_fixture("brooklyn.02_address_building");
    let mut answer = fixture.answer_with_execution();
    let snapshot = answer.execution.run_snapshot.as_mut().unwrap();
    snapshot
        .receipt_body_verified_project_node_ids
        .push("geo.parcel.separation".into());
    snapshot.receipt_body_verified_project_node_ids.sort();
    snapshot.report_only_terminal_project_node_ids.clear();
    let error = canonical_answer_bytes(&answer).unwrap_err();
    assert_eq!(error.code, GeoAnswerErrorCode::InvalidInput);
    assert_eq!(
        error.detail["field"],
        "execution.receipt_body_verified_project_node_ids"
    );
}

#[test]
fn ans_07h_validly_hashed_failed_body_is_not_a_shared_embedded_completed_receipt() {
    use canon::project::{
        ProjectRunNextAction, ProjectRunNodeOutcome, canonical_run_receipt_bytes,
        finalized_node_receipt, finalized_run_receipt, validate_node_receipt,
    };
    let fixture = retained_answer_fixture("brooklyn.02_address_building");
    let mut forged = fixture.retained_run.clone();
    let report = forged.project_run_report.as_mut().unwrap();
    // This deliberately forged negative has internally consistent hashes at
    // every layer. It must fail on the shared completed-body contract itself.
    let mut body = report.receipt.node_receipts[0].clone();
    body.node_id = "geo.parcel.separation".into();
    body.outcome = ProjectRunNodeOutcome::Failed;
    body.outputs.clear();
    body.next_action = ProjectRunNextAction::InspectFailure;
    body.failure_code = Some("SYNTHETIC_NEGATIVE".into());
    body.failure_message = Some("Deliberate invalid embedded terminal body".into());
    let body = finalized_node_receipt(body).unwrap();
    validate_node_receipt(body.clone()).unwrap();
    report
        .node_reports
        .iter_mut()
        .find(|node| node.node_id == body.node_id)
        .unwrap()
        .receipt_hash = Some(body.receipt_hash.clone());
    report.receipt.node_receipts.push(body);
    report.receipt = finalized_run_receipt(report.receipt.clone()).unwrap();
    canonical_run_receipt_bytes(&report.receipt).unwrap();
    report.run_receipt_hash = report.receipt.receipt_hash.clone();
    restamp_run(&mut forged);
    validate_geo_run(&forged).unwrap();
    let error = project_answer_with_execution(
        &fixture.inputs(),
        &fixture.config,
        &GeoAnswerLimits::default(),
        &forged,
    )
    .unwrap_err();
    assert_eq!(error.code, GeoAnswerErrorCode::InvalidInput);
    assert_eq!(error.detail["field"], "execution.report.node_receipt");
    assert!(error.message.contains("unique completed outcomes"));
}

#[test]
fn ans_08_permuted_admissions_preferences_and_source_read_order_are_byte_identical() {
    let baseline = retained_answer_fixture("cornerstone.national_building_supported");
    let expected = canonical_answer_bytes(&baseline.answer()).unwrap();
    let mut permuted = baseline.clone();
    permuted.compilation.admissions.reverse();
    permuted
        .compilation
        .composition_request
        .soft_preferences
        .reverse();
    for a in &mut permuted.compilation.admissions {
        a.source_records.reverse();
        a.generated_ids.reverse();
        a.contract.source_lineage_ids.reverse();
    }
    permuted.rebind_changed_evidence();
    assert_eq!(
        canonical_answer_bytes(&permuted.answer()).unwrap(),
        expected
    );
}

#[test]
fn ans_09_generic_answer_module_contains_no_solver_calls_or_development_literals() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/geo/answer.rs"))
            .unwrap();
    for forbidden in [
        "solve_composition",
        "separate(",
        "minimal_unsat",
        "repair(",
        "enumerate_cores",
    ] {
        assert!(
            !source.contains(forbidden),
            "answer must consume stored artifacts: {forbidden}"
        );
    }
    let lower = source.to_ascii_lowercase();
    for vendor in [
        "foursquare",
        "overture",
        "brooklyn",
        "courtney",
        "cornerstone",
        "sabal",
        "nyc_dcp_pad",
    ] {
        assert!(
            !lower.contains(vendor),
            "generic source branches on retained subject/vendor {vendor}"
        );
    }
    let catalog: Value = serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/geo/replay/catalog.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let literals = source
        .split('"')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, s)| s.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let generic = [
        "parcel",
        "building",
        "property",
        "address",
        "unit",
        "site",
        "unknown",
        "source",
        "target",
        "unavailable",
        "not_in_scope",
    ];
    for subject in catalog["development_subjects"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .filter(|s| s.len() >= 6)
    {
        let subject = subject.to_ascii_lowercase();
        if generic.contains(&subject.as_str()) {
            continue;
        }
        assert!(
            !literals.contains(&subject),
            "development subject literal in generic logic: {subject}"
        );
    }
}

#[test]
fn ans_11_fresh_process_projection_is_byte_identical() {
    let mut outputs = Vec::new();
    for _ in 0..2 {
        let directory = tempfile::Builder::new()
            .prefix("canon-answer-fresh-")
            .tempdir()
            .unwrap()
            .keep();
        let file = directory.join("answer.json");
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "answer_fresh_process_worker", "--nocapture"])
            .env("CANON_ANSWER_FRESH_OUT", &file)
            .output()
            .unwrap();
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        outputs.push(fs::read(file).unwrap());
    }
    assert_eq!(outputs[0], outputs[1]);
}

#[test]
fn answer_fresh_process_worker() {
    let answer = fixture::real_answer_instance();
    validate_answer_artifact(&answer).unwrap();
    let bytes = canonical_answer_bytes(&answer).unwrap();
    assert_eq!(answer.version, CANON_GEO_ANSWER_VERSION);
    assert!(!answer.preference.composition_view.preferred.is_empty());
    if let Some(destination) = std::env::var_os("CANON_ANSWER_FRESH_OUT") {
        fs::write(destination, bytes).unwrap();
    }
}

fn registered_run(
    fixture: &fixture::RetainedAnswerFixture,
    root: &Path,
    workspace: &Path,
    label: &str,
) -> (Value, Vec<u8>) {
    fs::create_dir_all(workspace).unwrap();
    let plan = root.join(format!("{label}.plan.json"));
    fs::write(
        &plan,
        canonical_geo_plan_bytes(&fixture.registered_answer_plan()).unwrap(),
    )
    .unwrap();
    let mut arguments = vec![
        "geo".into(),
        "run".into(),
        "--plan".into(),
        plan.display().to_string(),
        "--work-dir".into(),
        workspace.display().to_string(),
    ];
    arguments.extend(fixture.write_caller_inputs(&root.join("inputs")));
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(&arguments)
        .output()
        .unwrap();
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    fs::write(root.join(format!("{label}.run.json")), &output.stdout).unwrap();
    fs::write(root.join(format!("{label}.stderr")), &output.stderr).unwrap();
    println!(
        "{}",
        json!({"proof_class":"retained","argv":arguments,"exit":output.status.code(),"log":root})
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let run: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        run["status"], "ABSTAINED",
        "successful process alone is not a domain outcome"
    );
    let reference = run["output_refs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["output_id"] == "answer")
        .expect("registered stage must publish an answer");
    let hex = reference["content_digest"]
        .as_str()
        .unwrap()
        .strip_prefix("blake3:")
        .unwrap();
    let bytes = fs::read(
        workspace
            .join(".canon/geo-run/artifacts/cas")
            .join(format!("{hex}.bin")),
    )
    .unwrap();
    assert_eq!(blake3::hash(&bytes).to_hex().as_str(), hex);
    (run, bytes)
}

#[test]
fn ans_10_registered_public_run_publishes_answer_and_policy_change_reuses_only_upstream() {
    let mut fixture = retained_answer_fixture("cornerstone.national_building_supported");
    let root = tempfile::Builder::new()
        .prefix("canon-answer-run-")
        .tempdir()
        .unwrap()
        .keep();
    let workspace = root.join("work");
    let (initial, bytes) = registered_run(&fixture, &root, &workspace, "initial");
    assert_eq!(bytes, canonical_answer_bytes(&fixture.answer()).unwrap());
    let typed_initial: GeoRun = serde_json::from_value(initial.clone()).unwrap();
    let initial_history = project_answer_with_execution(
        &fixture.inputs(),
        &fixture.config,
        &GeoAnswerLimits::default(),
        &typed_initial,
    )
    .unwrap();
    let snapshot =
        serde_json::to_value(initial_history).unwrap()["execution"]["run_snapshot"].clone();
    assert_eq!(
        snapshot["completed_project_node_ids"]
            .as_array()
            .unwrap()
            .len(),
        10
    );
    assert_eq!(
        snapshot["executed_project_node_ids"]
            .as_array()
            .unwrap()
            .len(),
        10
    );
    assert_eq!(snapshot["resumed_project_node_ids"], json!([]));
    assert_eq!(
        snapshot["receipt_body_verified_project_node_ids"],
        snapshot["completed_project_node_ids"]
    );
    assert_eq!(snapshot["report_only_terminal_project_node_ids"], json!([]));
    let next_reference = typed_initial
        .output_refs
        .iter()
        .find(|r| r.output_id == "next_evidence")
        .unwrap();
    let next_hex = next_reference
        .content_digest
        .strip_prefix("blake3:")
        .unwrap();
    let next_bytes = fs::read(
        workspace
            .join(".canon/geo-run/artifacts/cas")
            .join(format!("{next_hex}.bin")),
    )
    .unwrap();
    let actual_next: GeoNextEvidenceArtifact = serde_json::from_slice(&next_bytes).unwrap();
    let mut full_inputs = fixture.inputs();
    full_inputs.next_evidence = Some(&actual_next);
    let with_next = project_answer_with_execution(
        &full_inputs,
        &fixture.config,
        &GeoAnswerLimits::default(),
        &typed_initial,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(with_next).unwrap()["further_work"]["source"],
        "next_evidence_artifact"
    );
    let mut missing_next = typed_initial.clone();
    missing_next
        .output_refs
        .retain(|r| r.output_id != "next_evidence");
    restamp_run(&mut missing_next);
    let error = project_answer_with_execution(
        &full_inputs,
        &fixture.config,
        &GeoAnswerLimits::default(),
        &missing_next,
    )
    .unwrap_err();
    assert_eq!(error.code, GeoAnswerErrorCode::AnswerInputMismatch);
    assert_eq!(error.detail["artifact"], "execution.output.next_evidence");
    assert_eq!(initial["deterministic_usage"]["answer_members_listed"], 2);
    assert_eq!(initial["deterministic_usage"]["answer_preferred_models"], 2);
    fixture.config.decision_policy = Some(GeoDecisionPolicyRef {
        policy_id: "synthetic-cache-policy".into(),
        version: "v0".into(),
        content_hash: format!("blake3:{}", "b".repeat(64)),
    });
    let (changed, bytes) = registered_run(&fixture, &root, &workspace, "policy-change");
    assert_eq!(
        changed["project_run_report"]["executed_nodes"],
        json!(["geo.building.answer"])
    );
    assert_eq!(
        changed["project_run_report"]["resumed_nodes"]
            .as_array()
            .unwrap()
            .len(),
        9
    );
    let typed_changed: GeoRun = serde_json::from_value(changed.clone()).unwrap();
    let resumed_history = project_answer_with_execution(
        &fixture.inputs(),
        &fixture.config,
        &GeoAnswerLimits::default(),
        &typed_changed,
    )
    .unwrap();
    let snapshot =
        serde_json::to_value(resumed_history).unwrap()["execution"]["run_snapshot"].clone();
    assert_eq!(
        snapshot["completed_project_node_ids"]
            .as_array()
            .unwrap()
            .len(),
        10
    );
    assert_eq!(
        snapshot["executed_project_node_ids"],
        json!(["geo.building.answer"])
    );
    assert_eq!(
        snapshot["receipt_body_verified_project_node_ids"],
        snapshot["completed_project_node_ids"]
    );
    assert_eq!(snapshot["report_only_terminal_project_node_ids"], json!([]));
    assert_eq!(
        snapshot["resumed_project_node_ids"]
            .as_array()
            .unwrap()
            .len(),
        9
    );
    let answer: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(answer["acceptance"]["status"], "policy_not_evaluated");
    assert_eq!(
        answer["acceptance"]["policy"]["policy_id"],
        "synthetic-cache-policy"
    );
    assert_eq!(
        preferred(&answer, "composition_view"),
        set(&[
            "0a5045cd-8321-429b-b0b7-0f3f24b6e615",
            "a6145c70-10cb-46d0-95b8-b85ff82ac700"
        ])
    );
}

#[test]
fn ans_13_answer_is_registered_and_unsurfaced_with_no_eighth_primary_command() {
    let capabilities = default_geo_capabilities().unwrap();
    let commands = &capabilities.commands.implemented;
    let answer = commands
        .iter()
        .filter(|c| c.command == "canon.geo.stage.answer.v0")
        .collect::<Vec<_>>();
    assert_eq!(answer.len(), 1);
    assert!(answer[0].surface.is_none());
    assert_eq!(answer[0].output_contract, CANON_GEO_ANSWER_VERSION);
    assert_eq!(
        commands
            .iter()
            .filter(|c| c.surface == Some(GeoCommandSurface::Primary))
            .count(),
        7
    );
    assert!(
        capabilities
            .contracts
            .implemented
            .iter()
            .any(|c| c.contract_version == CANON_GEO_ANSWER_VERSION)
    );
}

#[test]
fn ans_14_courtney_composition_and_grouped_views_preserve_the_unique_leader() {
    let answer = value("courtney.final_hints");
    assert_eq!(preferred(&answer, "composition_view"), set(&["1029330100"]));
    assert_eq!(preferred(&answer, "grouped_view"), set(&["1029330100"]));
    assert_eq!(
        answer["preference"]["composition_view"]["preferred"][0]["cost"],
        6
    );
    assert_eq!(
        answer["preference"]["composition_view"]["next_best_cost"],
        7
    );
    assert_eq!(
        answer["preference"]["composition_view"]["next_best_tie_count"],
        6
    );
    assert_eq!(
        answer["preference"]["grouped_view"]["cost_histogram"],
        json!([{"cost":0,"model_count":1},{"cost":1,"model_count":6},{"cost":2,"model_count":882}])
    );
    assert_eq!(answer["preference"]["lineage_dependent"], false);
    assert_unaccepted(&answer);
}

#[test]
fn ans_15_diagnostic_only_runs_do_not_promote_positional_rank_to_a_preference() {
    for case in [
        "cornerstone.national_building",
        "cornerstone.foursquare_parcel",
    ] {
        let answer = value(case);
        assert_eq!(answer["preference"]["state"]["kind"], "no_soft_evidence");
        assert!(
            preferred(&answer, "composition_view").is_empty()
                && preferred(&answer, "grouped_view").is_empty()
        );
        assert!(
            answer["association_support"]["diagnostic_observations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["admission_reason"] == "rho_soft_fallback_not_representable")
        );
        assert_unaccepted(&answer);
    }
    let positive = value("cornerstone.foursquare_parcel_supported");
    assert_both(&positive, &["302304550403000"], 0, 1, 2983);
}

#[test]
fn ans_16_orlando_without_soft_evidence_is_not_a_2983_way_ranked_tie() {
    let answer = value("orlando.case_1.hints");
    assert_eq!(answer["logical_consequence"]["residual_model_count"], 2983);
    assert_eq!(answer["preference"]["state"]["kind"], "no_soft_evidence");
    assert_eq!(answer["preference"]["composition_view"]["tie_count"], 0);
    assert!(preferred(&answer, "composition_view").is_empty());
    assert_unaccepted(&answer);
}

#[test]
fn ans_17_summit_retained_truth_is_residual_not_preferred_or_accepted() {
    let fixture = retained_answer_fixture("courtney.final_summit");
    let answer = serde_json::to_value(fixture.answer()).unwrap();
    for view in ["composition_view", "grouped_view"] {
        assert_eq!(preferred(&answer, view), set(&["0349070000", "0395740016"]));
    }
    // Evaluation-only knowledge is kept in this test, never projector inputs/config.
    assert!(
        fixture
            .composition
            .residual_models
            .iter()
            .any(|m| m.parcels.iter().any(|id| id == "0652990300"))
    );
    assert!(!preferred(&answer, "composition_view").contains("0652990300"));
    assert_eq!(
        answer["logical_consequence"]["hard_forced"],
        json!({"parcels":[],"buildings":[]})
    );
    assert_unaccepted(&answer);
}

#[test]
fn ans_18_prefinal_claim_projection_needs_neither_final_answer_nor_next_actions() {
    let fixture = retained_answer_fixture("cornerstone.national_building_supported");
    let claim = serde_json::to_value(
        project_current_claim(
            &fixture.claim_inputs(),
            &fixture.config,
            &GeoAnswerLimits::default(),
        )
        .unwrap(),
    )
    .unwrap();
    let answer = serde_json::to_value(fixture.answer()).unwrap();
    for key in [
        "claim_label",
        "association_support",
        "preference",
        "logical_consequence",
        "extent_and_reach",
    ] {
        assert_eq!(claim[key], answer[key], "prefinal projection drift: {key}");
    }
    assert!(claim.get("further_work").is_none());
}

#[test]
fn answer_e2e_prepare_worker() {
    let baseline = retained_answer_fixture("cornerstone.national_building_supported");
    validate_answer_artifact(&baseline.answer()).unwrap();
    let Some(destination) = std::env::var_os("CANON_ANSWER_E2E_OUT") else {
        assert_eq!(
            baseline
                .registered_answer_plan()
                .geo_nodes
                .iter()
                .filter(|n| n.stage == GeoPlanStage::ProjectCurrentAnswer)
                .count(),
            1
        );
        return;
    };
    let root = std::path::PathBuf::from(destination);
    fs::create_dir_all(&root).unwrap();
    let mut cases = Vec::new();
    for case in [
        "cornerstone.national_parcel_supported",
        "cornerstone.national_building_supported",
        "brooklyn.02_address_building",
        "courtney.final_hints",
        "cornerstone.national_building",
        "cornerstone.foursquare_parcel",
        "cornerstone.foursquare_parcel_supported",
        "orlando.case_1.hints",
        "courtney.final_summit",
    ] {
        let fixture = retained_answer_fixture(case);
        let answer = fixture.answer();
        validate_answer_artifact(&answer).unwrap();
        let bytes = canonical_answer_bytes(&answer).unwrap();
        let path = root.join(format!("{case}.answer.json"));
        fs::write(&path, &bytes).unwrap();
        cases.push(json!({"case_id":case,"answer":path,"answer_blake3":format!("blake3:{}",blake3::hash(&bytes)),"archive":fixture.catalog_row["archive"],"retained_execution":fixture.answer_with_execution().execution}));
    }
    let plan = root.join("registered-answer.plan.json");
    fs::write(
        &plan,
        canonical_geo_plan_bytes(&baseline.registered_answer_plan()).unwrap(),
    )
    .unwrap();
    let workspace = root.join("public-run");
    fs::create_dir_all(&workspace).unwrap();
    let mut run_args = vec![
        "geo".to_string(),
        "run".to_string(),
        "--plan".to_string(),
        plan.display().to_string(),
        "--work-dir".to_string(),
        workspace.display().to_string(),
    ];
    run_args.extend(baseline.write_caller_inputs(&root.join("caller-inputs")));
    let manifest = json!({"proof_class":"retained","no_claim":"No fresh acquisition, held-out accuracy, policy acceptance or E5 proof","cases":cases,"run_args":run_args,"workspace":workspace});
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    println!("{}", manifest);
}
