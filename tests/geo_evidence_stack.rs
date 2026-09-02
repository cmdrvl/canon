#![forbid(unsafe_code)]

use assert_cmd::Command;
use canon::geo::{
    CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION,
    CANON_GEO_POPULATION_REQUEST_VERSION, DEFAULT_MAX_MATERIALIZED_MODELS, GeoCompositionModel,
    GeoCompositionUniverse, GeoEntityLevel, GeoEntityRef, GeoEvidenceClaimRole,
    GeoEvidenceCompilationRequest, GeoEvidenceRecordRef, GeoEvidenceStackErrorCode,
    GeoIntegerMeasure, GeoIntegerMemberValue, GeoIntegerValueOrigin, GeoLabeledCompositionCase,
    GeoPopulationCaseEvidenceOverlay, GeoPopulationCaseStatus, GeoPopulationEvaluationRequest,
    GeoPopulationEvidenceStackRequest, GeoRhoBasis, GeoRhoContract, GeoRhoObservation,
    GeoRhoObservationKind, GeoTruthPlane, GeoValidTimeInterval,
    canonical_population_evidence_stack_bytes, compile_evidence, evaluate_population,
    stack_population_evidence, validate_population_evidence_stack_artifact,
};
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::tempdir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn record(id: &str) -> GeoEvidenceRecordRef {
    GeoEvidenceRecordRef {
        source_record_id: id.to_string(),
        source_vintage: "fixture-release-v1".to_string(),
        record_blake3: blake3::hash(id.as_bytes()).to_hex().to_string(),
    }
}

fn logical_contract(id: &str) -> GeoRhoContract {
    GeoRhoContract {
        id: id.to_string(),
        version: "v1".to_string(),
        source_dataset: format!("fixture:{id}"),
        source_release: "fixture-release-v1".to_string(),
        source_lineage_ids: vec![format!("fixture:{id}:lineage")],
        method_id: format!("fixture:{id}:rho"),
        method_version: "v1".to_string(),
        claim_role: GeoEvidenceClaimRole::AttributeObservation,
        basis: GeoRhoBasis::LogicalRelaxation {
            invariant_id: format!("fixture:{id}:invariant"),
        },
    }
}

fn empirical_contract(id: &str) -> GeoRhoContract {
    GeoRhoContract {
        id: id.to_string(),
        version: "v1".to_string(),
        source_dataset: format!("fixture:{id}"),
        source_release: "fixture-release-v1".to_string(),
        source_lineage_ids: vec![format!("fixture:{id}:lineage")],
        method_id: format!("fixture:{id}:rho"),
        method_version: "v1".to_string(),
        claim_role: GeoEvidenceClaimRole::AttributeObservation,
        basis: GeoRhoBasis::EmpiricalCalibration {
            population_id: format!("fixture:{id}:population"),
            calibration_blake3: blake3::hash(format!("calibration:{id}").as_bytes())
                .to_hex()
                .to_string(),
            falsification_rule_id: format!("fixture:{id}:falsification"),
            admissible_hard_band: false,
        },
    }
}

fn empirical_hard_band_contract(id: &str) -> GeoRhoContract {
    let mut contract = empirical_contract(id);
    if let GeoRhoBasis::EmpiricalCalibration {
        admissible_hard_band,
        ..
    } = &mut contract.basis
    {
        *admissible_hard_band = true;
    }
    contract
}

fn observation(
    id: &str,
    contract_id: &str,
    source_record_id: &str,
    observation: GeoRhoObservationKind,
) -> GeoRhoObservation {
    GeoRhoObservation {
        id: id.to_string(),
        contract_id: contract_id.to_string(),
        source_records: vec![record(source_record_id)],
        valid_time: None,
        observation,
    }
}

fn integer_sum_observation(id: &str, contract_id: &str) -> GeoRhoObservation {
    observation(
        id,
        contract_id,
        &format!("{id}-row"),
        GeoRhoObservationKind::IntegerSumBand {
            level: GeoEntityLevel::Parcel,
            measure: GeoIntegerMeasure {
                semantic_id: "fixture:structure-count".to_string(),
                unit: "count".to_string(),
                value_origin: GeoIntegerValueOrigin::ExactDerived,
            },
            values: vec![
                GeoIntegerMemberValue {
                    id: "p1".to_string(),
                    value: 1,
                },
                GeoIntegerMemberValue {
                    id: "p2".to_string(),
                    value: 1,
                },
            ],
            min: 1,
            max: 1,
        },
    )
}

fn base_population() -> GeoPopulationEvaluationRequest {
    GeoPopulationEvaluationRequest {
        version: CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        cases: vec![GeoLabeledCompositionCase {
            id: "case-a".to_string(),
            evidence: GeoEvidenceCompilationRequest {
                version: CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
                profile: Default::default(),
                universe: GeoCompositionUniverse {
                    parcels: vec!["p2".to_string(), "p1".to_string()],
                    buildings: Vec::new(),
                },
                contracts: Vec::new(),
                observations: Vec::new(),
                max_assignments: 8,
                max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
            },
            truth_plane: GeoTruthPlane::HumanAdjudication,
            truth: GeoCompositionModel {
                parcels: vec!["p1".to_string()],
                buildings: Vec::new(),
            },
        }],
        max_cases: 1,
    }
}

fn full_overlay() -> GeoPopulationEvidenceStackRequest {
    GeoPopulationEvidenceStackRequest {
        version: CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION.to_string(),
        case_overlays: vec![GeoPopulationCaseEvidenceOverlay {
            case_id: "case-a".to_string(),
            expected_base_evidence_blake3: None,
            contracts: vec![
                empirical_contract("soft"),
                logical_contract("hard"),
                empirical_contract("diagnostic"),
            ],
            observations: vec![
                observation(
                    "soft-preference",
                    "soft",
                    "soft-row",
                    GeoRhoObservationKind::PreferMember {
                        member: GeoEntityRef::new(GeoEntityLevel::Parcel, "p1"),
                        cost_if_absent: 7,
                    },
                ),
                observation(
                    "hard-exact",
                    "hard",
                    "hard-row",
                    GeoRhoObservationKind::ExactSets {
                        level: GeoEntityLevel::Parcel,
                        sets: vec![vec!["p1".to_string()]],
                    },
                ),
                observation(
                    "empirical-exact",
                    "diagnostic",
                    "diagnostic-row",
                    GeoRhoObservationKind::ExactSets {
                        level: GeoEntityLevel::Parcel,
                        sets: vec![vec!["p2".to_string()]],
                    },
                ),
            ],
        }],
        max_overlay_cases: 1,
        max_overlay_observations: 3,
    }
}

#[test]
fn truth_blind_stack_flows_directly_into_exact_evaluation() {
    let base = base_population();
    let artifact = stack_population_evidence(&base, &full_overlay()).expect("stack evidence");
    validate_population_evidence_stack_artifact(&artifact).expect("artifact replays");

    assert_eq!(artifact.base_population.cases[0].truth, base.cases[0].truth);
    assert_eq!(artifact.population.cases[0].truth, base.cases[0].truth);
    assert_eq!(
        artifact.population.cases[0].truth_plane,
        base.cases[0].truth_plane
    );
    assert_eq!(
        artifact.population.cases[0].evidence.universe,
        artifact.base_population.cases[0].evidence.universe
    );
    assert_eq!(
        artifact.population.cases[0].evidence.profile,
        artifact.base_population.cases[0].evidence.profile
    );
    assert_eq!(
        artifact.population.cases[0].evidence.max_assignments,
        artifact.base_population.cases[0].evidence.max_assignments
    );
    assert_eq!(artifact.summary.added_contracts, 3);
    assert_eq!(artifact.summary.added_observations, 3);
    assert_eq!(artifact.summary.added_source_records, 3);
    assert_eq!(artifact.summary.hard_constraint_observations, 1);
    assert_eq!(artifact.summary.soft_preference_observations, 1);
    assert_eq!(artifact.summary.diagnostic_observations, 1);

    let evaluation =
        evaluate_population(&artifact.population).expect("evaluate stacked population");
    assert_eq!(
        evaluation.cases[0].status,
        GeoPopulationCaseStatus::Resolved
    );
    assert_eq!(evaluation.cases[0].hard_constraint_observations, 1);
    assert_eq!(evaluation.cases[0].soft_preference_observations, 1);
    assert_eq!(evaluation.cases[0].diagnostic_observations, 1);
    assert_eq!(evaluation.cases[0].truth_model_in_residual, Some(true));
}

#[test]
fn observer_overlay_bands_are_hard_only_under_flagged_empirical_contracts() {
    let overlay = |contract: GeoRhoContract, observation: GeoRhoObservation| {
        GeoPopulationEvidenceStackRequest {
            version: CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION.to_string(),
            case_overlays: vec![GeoPopulationCaseEvidenceOverlay {
                case_id: "case-a".to_string(),
                expected_base_evidence_blake3: None,
                contracts: vec![contract],
                observations: vec![observation],
            }],
            max_overlay_cases: 1,
            max_overlay_observations: 1,
        }
    };

    let unflagged = stack_population_evidence(
        &base_population(),
        &overlay(
            empirical_contract("rho.structure_count.v0"),
            integer_sum_observation("observer-count", "rho.structure_count.v0"),
        ),
    )
    .expect("unflagged observer overlay remains diagnostic");
    assert_eq!(unflagged.summary.hard_constraint_observations, 0);
    assert_eq!(unflagged.summary.diagnostic_observations, 1);
    let unflagged_compilation = compile_evidence(&unflagged.population.cases[0].evidence)
        .expect("stacked unflagged evidence still compiles");
    assert!(
        unflagged_compilation
            .composition_request
            .hard_constraints
            .is_empty()
    );
    assert_eq!(
        unflagged_compilation.admissions[0]
            .admission_reason
            .as_deref(),
        Some("rho_band_not_admissible")
    );
    let unflagged_evaluation =
        evaluate_population(&unflagged.population).expect("unflagged stack evaluates");
    assert_eq!(unflagged_evaluation.cases[0].residual_model_count, Some(3));

    let flagged = stack_population_evidence(
        &base_population(),
        &overlay(
            empirical_hard_band_contract("rho.structure_count.v0"),
            integer_sum_observation("observer-count", "rho.structure_count.v0"),
        ),
    )
    .expect("flagged observer overlay contributes one hard band");
    assert_eq!(flagged.summary.hard_constraint_observations, 1);
    assert_eq!(flagged.summary.diagnostic_observations, 0);
    let flagged_compilation =
        compile_evidence(&flagged.population.cases[0].evidence).expect("flagged evidence compiles");
    assert_eq!(
        flagged_compilation
            .composition_request
            .hard_constraints
            .len(),
        1
    );
    assert!(flagged_compilation.admissions[0].admission_reason.is_none());
    let flagged_evaluation =
        evaluate_population(&flagged.population).expect("flagged stack evaluates");
    assert_eq!(flagged_evaluation.cases[0].residual_model_count, Some(2));

    let mut vintage_observation =
        integer_sum_observation("observer-count-at-vintage", "rho.structure_count.v0");
    vintage_observation.valid_time = Some(GeoValidTimeInterval {
        start_day: 19_723,
        end_day: 19_723,
    });
    let vintage = stack_population_evidence(
        &base_population(),
        &overlay(
            empirical_hard_band_contract("rho.structure_count.v0"),
            vintage_observation,
        ),
    )
    .expect("time-scoped observer overlay remains diagnostic");
    assert_eq!(vintage.summary.hard_constraint_observations, 0);
    assert_eq!(vintage.summary.diagnostic_observations, 1);
    let vintage_compilation =
        compile_evidence(&vintage.population.cases[0].evidence).expect("vintage evidence compiles");
    assert!(
        vintage_compilation
            .composition_request
            .hard_constraints
            .is_empty()
    );
    let vintage_evaluation =
        evaluate_population(&vintage.population).expect("vintage stack evaluates");
    assert_eq!(vintage_evaluation.cases[0].residual_model_count, Some(3));
}

#[test]
fn exact_replay_is_idempotent_but_semantic_duplicate_ids_are_rejected() {
    let first =
        stack_population_evidence(&base_population(), &full_overlay()).expect("first stack");
    let replay = stack_population_evidence(&first.population, &full_overlay()).expect("idempotent");
    assert_eq!(replay.summary.added_contracts, 0);
    assert_eq!(replay.summary.added_observations, 0);
    assert_eq!(replay.summary.reused_contracts, 3);
    assert_eq!(replay.summary.reused_observations, 3);
    assert_eq!(replay.population, first.population);

    let mut duplicate = full_overlay();
    duplicate.case_overlays[0].contracts.clear();
    duplicate.case_overlays[0].observations =
        vec![duplicate.case_overlays[0].observations[0].clone()];
    duplicate.case_overlays[0].observations[0].id = "renamed-soft-preference".to_string();
    duplicate.max_overlay_observations = 1;
    let error = stack_population_evidence(&first.population, &duplicate)
        .expect_err("renaming one semantic observation cannot inflate evidence");
    assert_eq!(error.code, GeoEvidenceStackErrorCode::InvalidInput);
    assert!(error.message.contains("semantic observation"));
}

#[test]
fn hard_evidence_can_empty_the_feasible_set_and_contract_drift_is_refused() {
    let first =
        stack_population_evidence(&base_population(), &full_overlay()).expect("first stack");
    let conflict = GeoPopulationEvidenceStackRequest {
        version: CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION.to_string(),
        case_overlays: vec![GeoPopulationCaseEvidenceOverlay {
            case_id: "case-a".to_string(),
            expected_base_evidence_blake3: None,
            contracts: vec![logical_contract("second-hard")],
            observations: vec![observation(
                "second-hard-exact",
                "second-hard",
                "second-hard-row",
                GeoRhoObservationKind::ExactSets {
                    level: GeoEntityLevel::Parcel,
                    sets: vec![vec!["p2".to_string()]],
                },
            )],
        }],
        max_overlay_cases: 1,
        max_overlay_observations: 1,
    };
    let stacked = stack_population_evidence(&first.population, &conflict).expect("stack conflict");
    let evaluation = evaluate_population(&stacked.population).expect("evaluate conflict");
    assert_eq!(
        evaluation.cases[0].status,
        GeoPopulationCaseStatus::Conflict
    );

    let mut drift = full_overlay();
    drift.case_overlays[0].observations = vec![drift.case_overlays[0].observations[0].clone()];
    drift.case_overlays[0].contracts = vec![empirical_contract("soft")];
    drift.case_overlays[0].contracts[0].method_version = "v2".to_string();
    drift.max_overlay_observations = 1;
    let error = stack_population_evidence(&first.population, &drift)
        .expect_err("contract redefinition must be refused");
    assert!(error.message.contains("redefines"));
}

#[test]
fn stale_case_binding_unknown_case_and_truth_field_are_refused() {
    let mut stale = full_overlay();
    stale.case_overlays[0].expected_base_evidence_blake3 = Some("0".repeat(64));
    let error = stack_population_evidence(&base_population(), &stale)
        .expect_err("stale base evidence binding");
    assert_eq!(error.detail.get("expected"), Some(&"0".repeat(64)));
    assert!(error.detail.contains_key("actual"));

    let mut unknown = full_overlay();
    unknown.case_overlays[0].case_id = "case-unknown".to_string();
    let error = stack_population_evidence(&base_population(), &unknown)
        .expect_err("unknown case must be refused");
    assert!(error.message.contains("unknown population case"));

    let mut value = serde_json::to_value(full_overlay()).expect("serialize overlay");
    value["case_overlays"][0]["truth"] = json!({"parcels": ["p2"], "buildings": []});
    let parsed = serde_json::from_value::<GeoPopulationEvidenceStackRequest>(value);
    assert!(parsed.is_err(), "an overlay cannot carry evaluation truth");
}

#[test]
fn permutations_have_identical_canonical_bytes_and_tampering_breaks_replay() {
    let first =
        stack_population_evidence(&base_population(), &full_overlay()).expect("first stack");
    let mut permuted = full_overlay();
    permuted.case_overlays[0].contracts.reverse();
    permuted.case_overlays[0].observations.reverse();
    let second = stack_population_evidence(&base_population(), &permuted).expect("permuted stack");
    assert_eq!(
        canonical_population_evidence_stack_bytes(&first).expect("canonical first"),
        canonical_population_evidence_stack_bytes(&second).expect("canonical second")
    );

    let mut tampered = first;
    tampered.summary.added_source_records += 1;
    let error = validate_population_evidence_stack_artifact(&tampered)
        .expect_err("summary tampering must break replay");
    assert!(error.message.contains("does not replay"));
}

#[test]
fn held_out_truth_cannot_change_the_stacked_evidence_or_admission_summary() {
    let first =
        stack_population_evidence(&base_population(), &full_overlay()).expect("first truth");
    let mut alternate = base_population();
    alternate.cases[0].truth.parcels = vec!["p2".to_string()];
    alternate.cases[0].truth_plane = GeoTruthPlane::AddressDerivedControl;
    let second = stack_population_evidence(&alternate, &full_overlay()).expect("alternate truth");

    assert_eq!(
        first.population.cases[0].evidence,
        second.population.cases[0].evidence
    );
    assert_eq!(first.summary, second.summary);
    assert_ne!(first.base_population_blake3, second.base_population_blake3);
}

fn write_json(path: &Path, value: &impl serde::Serialize) {
    fs::write(path, serde_json::to_vec(value).expect("serialize JSON")).expect("write JSON");
}

#[test]
fn cli_stack_artifact_is_a_direct_evaluate_input() {
    let temp = tempdir().expect("tempdir");
    let population_path = temp.path().join("population.json");
    let overlay_path = temp.path().join("overlay.json");
    let stack_path = temp.path().join("stack.json");
    write_json(&population_path, &base_population());
    write_json(&overlay_path, &full_overlay());

    let stack = canon_command()
        .args(["geo", "stack-evidence", "--population"])
        .arg(&population_path)
        .arg("--overlay")
        .arg(&overlay_path)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    fs::write(&stack_path, &stack).expect("write stack output");

    let evaluate = canon_command()
        .args(["geo", "evaluate", "--population"])
        .arg(&stack_path)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&evaluate).expect("evaluation JSON");
    assert_eq!(value["version"], "canon_geo_population_evaluation.v0");
    assert_eq!(value["cases"][0]["status"], "resolved");
}
