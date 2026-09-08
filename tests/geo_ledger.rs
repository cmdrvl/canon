#![forbid(unsafe_code)]

use assert_cmd::Command;

mod geo {
    pub use canon::geo::*;
}

#[allow(dead_code)]
#[path = "../src/geo/ledger.rs"]
mod ledger;

use canon::geo::{
    CANON_GEO_COMPOSITION_REQUEST_VERSION, CANON_GEO_COMPOSITION_VERSION, GeoCandidateReachStatus,
    GeoCompositionArtifact, GeoCompositionBackbone, GeoCompositionFallback, GeoCompositionModel,
    GeoCompositionProfile, GeoCompositionStatus, GeoCompositionSummary,
    GeoEvidenceCompilationArtifact, GeoEvidenceCompilationReference, GeoEvidenceCompilationRequest,
    GeoLabeledCompositionCase, GeoModelCountScope, GeoPopulationEvaluationRequest, GeoTruthPlane,
    GeoValidTimeInterval, canonical_composition_bytes, canonical_evidence_compilation_bytes,
    compile_evidence,
};
use ledger::{
    CANON_GEO_COLLATERAL_LEDGER_SEED_VERSION, CANON_GEO_COLLATERAL_LEDGER_VERSION,
    GeoCollateralLedger, GeoCollateralLedgerProofClass, GeoCollateralLedgerSeed,
    GeoCollateralLedgerSeedRow, GeoLedgerErrorCode, GeoLedgerLoanRef, GeoLedgerPropertyRef,
    GeoLedgerRow, GeoSourceReleasePin, build_collateral_ledger, build_collateral_ledger_from_seed,
    build_ledger_row, canonical_collateral_ledger_bytes, canonical_collateral_ledger_seed_bytes,
    roll_up_deal, validate_ledger,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use tempfile::tempdir;

const E4_RESTACK_EVALUATION: &str =
    "../scripts/geo_measurements/fixtures/e4_gate_v2_restack_2026-09-03/e4_eval_roll.json";
const E4_RESTACK_EVALUATION_BYTES: &[u8] = include_bytes!(
    "../scripts/geo_measurements/fixtures/e4_gate_v2_restack_2026-09-03/e4_eval_roll.json"
);
const E4_POPULATION_REQUEST: &str = "../tests/fixtures/geo/e4_gate_v2_population_request.json";
const E4_ENRICHMENT: &str = "../tests/fixtures/geo/e4_gate_v2_evidence_enrichment.json";

const SYNTHETIC_ACCESSION: &str = "0000000000-26-000001";
const SYNTHETIC_DEAL: &str = "fixture-deal-a";
const FORCED_REACH_NONE_CASE: &str = "3cf11e9a58e3b710";
const FORCED_REACH_NONE_LOAN: &str = "073ad3a0862827c75501ac66570eb783";
const FORCED_REACH_NONE_REASON: &str = "no_candidate_parcels";
const GEO_LEDGER_VALIDATE_NEXT_COMMAND: &str = "canon geo ledger validate --ledger <LEDGER.json>";

#[test]
fn t07_fixture_gate_rows_roll_up_per_truth_plane_without_total() {
    let ledger = fixture_ledger();

    assert_eq!(ledger.rows.len(), 15);
    assert_eq!(ledger.rollups.len(), 1);
    assert_eq!(ledger.rollups[0].rows, 15);
    assert_eq!(ledger.rollups[0].deal_id, SYNTHETIC_DEAL);
    assert_eq!(ledger.rollups[0].accession, SYNTHETIC_ACCESSION);
    assert_eq!(
        ledger
            .rollups
            .iter()
            .flat_map(|rollup| rollup.truth_planes.values())
            .map(|counts| counts.resolved
                + counts.ambiguous
                + counts.conflict
                + counts.reach_none
                + counts.budget_fallback)
            .sum::<u64>(),
        ledger.rollups[0].rows
    );
    let gate_counts = ledger.rollups[0]
        .truth_planes
        .get(&GeoTruthPlane::GateV2Historical)
        .expect("gate truth-plane counts");
    assert_eq!(gate_counts.resolved, 4);
    assert_eq!(gate_counts.ambiguous, 9);
    assert_eq!(gate_counts.conflict, 0);
    assert_eq!(gate_counts.reach_none, 1);
    assert_eq!(gate_counts.budget_fallback, 1);

    let serialized_rollup = serde_json::to_value(&ledger.rollups[0]).expect("rollup JSON");
    assert!(serialized_rollup.get("total").is_none());
    let top_level_numeric_keys = serialized_rollup
        .as_object()
        .expect("rollup object")
        .iter()
        .filter_map(|(key, value)| value.is_number().then_some(key.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(top_level_numeric_keys, vec!["rows"]);

    let reach_none_rows = ledger
        .rows
        .iter()
        .filter(|row| row.reach == GeoCandidateReachStatus::None)
        .collect::<Vec<_>>();
    assert_eq!(reach_none_rows.len(), 1);
    let row = reach_none_rows[0];
    assert_eq!(row.loan_id, FORCED_REACH_NONE_LOAN);
    assert_eq!(
        row.reach_none_reason.as_deref(),
        Some(FORCED_REACH_NONE_REASON)
    );
    assert_eq!(row.parcel_set, None);
    assert_eq!(row.building_set, None);

    assert!(ledger.rows.iter().any(|row| {
        row.composition_status == GeoCompositionStatus::Ambiguous
            && row.count_exact
            && !row.ambiguous_parcel_set.is_empty()
    }));

    let recomputed = roll_up_deal(&ledger.rows).expect("recomputed rollup");
    assert_eq!(
        serde_json::to_vec(&recomputed).expect("recomputed rollup bytes"),
        serde_json::to_vec(&ledger.rollups[0]).expect("stored rollup bytes")
    );
    validate_ledger(&ledger).expect("ledger validates");

    let mut unlabeled_rows = ledger.rows[0..2].to_vec();
    unlabeled_rows[0].truth_plane = None;
    unlabeled_rows[1].truth_plane = Some(GeoTruthPlane::RoundExactLenderParty);
    let error = roll_up_deal(&unlabeled_rows).expect_err("unlabeled plane refuses");
    assert_eq!(error.code, GeoLedgerErrorCode::LedgerTruthPlanePooled);
    assert_eq!(error.detail["field"], "truth_plane");
}

#[test]
fn t23_build_ledger_row_refuses_fabricated_sets_without_artifacts() {
    let loan = fixture_loan("loan-a");
    let evidence_request = sample_evidence_request();
    let evidence = compile_evidence(&evidence_request).expect("sample evidence compiles");
    let composition = sample_composition(GeoCompositionStatus::Resolved, vec!["p1"], 1);
    let pins = vec![fixture_pin()];

    let missing_composition = build_ledger_row(
        &loan,
        GeoCandidateReachStatus::Full,
        None,
        None,
        Some(&evidence),
        Some(GeoTruthPlane::GateV2Historical),
        &pins,
    )
    .expect_err("missing composition refuses");
    assert_eq!(
        missing_composition.code,
        GeoLedgerErrorCode::LedgerSetsWithoutArtifacts
    );
    assert_eq!(missing_composition.detail["field"], "composition");

    let missing_evidence = build_ledger_row(
        &loan,
        GeoCandidateReachStatus::Full,
        None,
        Some(&composition),
        None,
        Some(GeoTruthPlane::GateV2Historical),
        &pins,
    )
    .expect_err("missing evidence refuses");
    assert_eq!(
        missing_evidence.code,
        GeoLedgerErrorCode::LedgerSetsWithoutArtifacts
    );
    assert_eq!(missing_evidence.detail["field"], "evidence");

    let missing_reason = build_ledger_row(
        &loan,
        GeoCandidateReachStatus::None,
        None,
        None,
        None,
        Some(GeoTruthPlane::GateV2Historical),
        &pins,
    )
    .expect_err("reach none without reason refuses");
    assert_eq!(missing_reason.code, GeoLedgerErrorCode::InvalidInput);
    assert_eq!(missing_reason.detail["field"], "reach_none_reason");

    let abstained = build_ledger_row(
        &loan,
        GeoCandidateReachStatus::None,
        Some(FORCED_REACH_NONE_REASON.to_string()),
        None,
        None,
        Some(GeoTruthPlane::GateV2Historical),
        &pins,
    )
    .expect("reach none with reason emits a row");
    assert_eq!(abstained.reach, GeoCandidateReachStatus::None);
    assert_eq!(
        abstained.reach_none_reason.as_deref(),
        Some(FORCED_REACH_NONE_REASON)
    );
    assert_eq!(abstained.parcel_set, None);
    assert_eq!(abstained.building_set, None);
}

#[test]
fn t23_build_ledger_row_requires_matching_evidence_digest_chain() {
    let loan = fixture_loan("loan-chain");
    let pins = vec![fixture_pin()];
    let primary_evidence =
        compile_evidence(&load_population_request().cases[0].evidence).expect("primary evidence");
    let other_evidence =
        compile_evidence(&load_population_request().cases[1].evidence).expect("other evidence");
    let composition = with_evidence_reference(
        sample_composition(GeoCompositionStatus::Resolved, vec!["p1"], 1),
        &primary_evidence,
    );
    assert_ne!(
        evidence_blake3(&primary_evidence),
        evidence_blake3(&other_evidence),
        "fixture evidence artifacts must differ for the mismatch regression"
    );

    let row = build_ledger_row(
        &loan,
        GeoCandidateReachStatus::Full,
        None,
        Some(&composition),
        Some(&primary_evidence),
        Some(GeoTruthPlane::GateV2Historical),
        &pins,
    )
    .expect("matching composition/evidence chain builds");
    assert_eq!(row.evidence_blake3, evidence_blake3(&primary_evidence));
    assert_eq!(row.parcel_set, Some(vec!["p1".to_string()]));

    let mismatched = build_ledger_row(
        &loan,
        GeoCandidateReachStatus::Full,
        None,
        Some(&composition),
        Some(&other_evidence),
        Some(GeoTruthPlane::GateV2Historical),
        &pins,
    )
    .expect_err("valid but unrelated evidence must refuse before sets are emitted");
    assert_eq!(mismatched.code, GeoLedgerErrorCode::InvalidInput);
    assert_eq!(
        mismatched.detail["field"],
        "composition.evidence_compilation.blake3"
    );
    assert_eq!(mismatched.detail["loan_id"], "loan-chain");
    assert_eq!(
        mismatched.detail["composition_evidence_blake3"],
        evidence_blake3(&primary_evidence)
    );
    assert_eq!(
        mismatched.detail["evidence_blake3"],
        evidence_blake3(&other_evidence)
    );

    let absent_reference = sample_composition(GeoCompositionStatus::Resolved, vec!["p1"], 1);
    let absent = build_ledger_row(
        &loan,
        GeoCandidateReachStatus::Full,
        None,
        Some(&absent_reference),
        Some(&primary_evidence),
        Some(GeoTruthPlane::GateV2Historical),
        &pins,
    )
    .expect_err("composition without an evidence reference cannot support claimed sets");
    assert_eq!(absent.code, GeoLedgerErrorCode::InvalidInput);
    assert_eq!(absent.detail["field"], "composition.evidence_compilation");
    assert_eq!(absent.detail["loan_id"], "loan-chain");
    assert_eq!(
        absent.detail["evidence_blake3"],
        evidence_blake3(&primary_evidence)
    );
}

#[test]
fn t23_build_collateral_ledger_from_seed_consumes_bound_artifacts_and_keeps_reach_none() {
    let evidence = compile_evidence(&sample_evidence_request()).expect("seed evidence compiles");
    let composition = with_evidence_reference(
        sample_composition(GeoCompositionStatus::Resolved, vec!["parcel:seed:1"], 1),
        &evidence,
    );
    let seed = fixture_build_seed();
    let ledger = build_collateral_ledger_from_seed(
        &seed,
        &BTreeMap::from([("solve-a".to_string(), composition)]),
        &BTreeMap::from([("evidence-a".to_string(), evidence)]),
    )
    .expect("seed plus matching artifacts builds ledger");

    assert_eq!(ledger.version, CANON_GEO_COLLATERAL_LEDGER_VERSION);
    assert_eq!(ledger.proof_class, GeoCollateralLedgerProofClass::Fixture);
    assert_eq!(ledger.rows.len(), 2);
    let solved = ledger
        .rows
        .iter()
        .find(|row| row.loan_id == "loan-build-a")
        .expect("solved row");
    assert_eq!(solved.parcel_set, Some(vec!["parcel:seed:1".to_string()]));
    assert_eq!(
        solved.property_refs,
        vec![GeoLedgerPropertyRef {
            property_id: "property:loan-build-a".to_string(),
            parcel_ids: vec!["parcel:seed:1".to_string()],
            building_ids: Vec::new(),
        }]
    );
    assert_eq!(
        solved.last_observed_present,
        Some(GeoValidTimeInterval {
            start_day: 19_700,
            end_day: 19_730,
        })
    );
    let no_reach = ledger
        .rows
        .iter()
        .find(|row| row.loan_id == "loan-no-reach")
        .expect("reach-none row");
    assert_eq!(no_reach.reach, GeoCandidateReachStatus::None);
    assert_eq!(
        no_reach.reach_none_reason.as_deref(),
        Some(FORCED_REACH_NONE_REASON)
    );
    assert_eq!(no_reach.parcel_set, None);
    assert_eq!(no_reach.building_set, None);
    assert_eq!(ledger.rollups[0].rows, 2);
    validate_ledger(&ledger).expect("built ledger validates");
}

#[test]
fn t07_last_observed_present_interval_is_validated_and_serialized() {
    let mut ledger = fixture_ledger();
    let observed = GeoValidTimeInterval {
        start_day: 19_723,
        end_day: 19_754,
    };
    ledger.rows[0].last_observed_present = Some(observed);
    ledger.rollups = vec![roll_up_deal(&ledger.rows).expect("dated rollup")];
    validate_ledger(&ledger).expect("dated row validates");

    let serialized = serde_json::to_value(&ledger.rows[0]).expect("dated row JSON");
    assert_eq!(
        serialized["last_observed_present"]["start_day"],
        json!(observed.start_day)
    );
    assert_eq!(
        serialized["last_observed_present"]["end_day"],
        json!(observed.end_day)
    );

    let mut inverted = ledger;
    inverted.rows[0].last_observed_present = Some(GeoValidTimeInterval {
        start_day: observed.end_day,
        end_day: observed.start_day,
    });
    let error = validate_ledger(&inverted).expect_err("inverted interval refuses");
    assert_eq!(error.code, GeoLedgerErrorCode::InvalidInput);
    assert_eq!(error.detail["field"], "last_observed_present");
    assert_eq!(error.detail["loan_id"], inverted.rows[0].loan_id);
    assert_eq!(error.detail["start_day"], observed.end_day.to_string());
    assert_eq!(error.detail["end_day"], observed.start_day.to_string());
}

#[test]
fn t07_geo_ledger_validate_cli_emits_canonical_ledger() {
    let temp = tempdir().expect("tempdir");
    let ledger = fixture_ledger();
    let canonical =
        canonical_collateral_ledger_bytes(&ledger).expect("fixture ledger canonicalizes");
    let ledger_path = temp.path().join("ledger.json");
    fs::write(&ledger_path, &canonical).expect("write ledger fixture");

    let assert = canon_command()
        .arg("geo")
        .arg("ledger")
        .arg("validate")
        .arg("--ledger")
        .arg(&ledger_path)
        .assert()
        .success();
    assert!(assert.get_output().stderr.is_empty());
    let mut expected_stdout = canonical;
    expected_stdout.push(b'\n');
    assert_eq!(assert.get_output().stdout, expected_stdout);
}

#[test]
fn t23_geo_ledger_build_cli_emits_canonical_ledger_and_validate_replays_it() {
    let temp = tempdir().expect("tempdir");
    let evidence = compile_evidence(&sample_evidence_request()).expect("seed evidence compiles");
    let composition = with_evidence_reference(
        sample_composition(GeoCompositionStatus::Resolved, vec!["parcel:seed:1"], 1),
        &evidence,
    );
    let seed = fixture_build_seed();
    let seed_path = write_seed_fixture(temp.path(), "seed.json", &seed);
    let composition_path = write_composition_fixture(temp.path(), "solve.json", &composition);
    let evidence_path = write_evidence_fixture(temp.path(), "evidence.json", &evidence);

    let assert = canon_command()
        .arg("geo")
        .arg("ledger")
        .arg("build")
        .arg("--seed")
        .arg(&seed_path)
        .arg("--composition")
        .arg(format!("solve-a={}", composition_path.display()))
        .arg("--evidence")
        .arg(format!("evidence-a={}", evidence_path.display()))
        .assert()
        .success();
    assert!(assert.get_output().stderr.is_empty());

    let mut ledger_bytes = assert.get_output().stdout.clone();
    assert_eq!(ledger_bytes.pop(), Some(b'\n'));
    let ledger: GeoCollateralLedger =
        serde_json::from_slice(&ledger_bytes).expect("built ledger parses");
    assert_eq!(ledger.rows.len(), 2);
    assert_eq!(
        ledger
            .rows
            .iter()
            .find(|row| row.loan_id == "loan-no-reach")
            .expect("reach-none row")
            .reach_none_reason
            .as_deref(),
        Some(FORCED_REACH_NONE_REASON)
    );
    let ledger_path = temp.path().join("built-ledger.json");
    fs::write(&ledger_path, &ledger_bytes).expect("write emitted ledger");

    let replay = canon_command()
        .arg("geo")
        .arg("ledger")
        .arg("validate")
        .arg("--ledger")
        .arg(&ledger_path)
        .assert()
        .success();
    assert!(replay.get_output().stderr.is_empty());
    let mut expected = ledger_bytes;
    expected.push(b'\n');
    assert_eq!(replay.get_output().stdout, expected);
}

#[test]
fn t23_geo_ledger_build_cli_refuses_mismatched_valid_artifacts() {
    let temp = tempdir().expect("tempdir");
    let primary_evidence =
        compile_evidence(&load_population_request().cases[0].evidence).expect("primary evidence");
    let other_evidence =
        compile_evidence(&load_population_request().cases[1].evidence).expect("other evidence");
    let composition = with_evidence_reference(
        sample_composition(GeoCompositionStatus::Resolved, vec!["parcel:seed:1"], 1),
        &primary_evidence,
    );
    let mut seed = fixture_build_seed();
    seed.rows.truncate(1);
    let seed_path = write_seed_fixture(temp.path(), "seed.json", &seed);
    let composition_path = write_composition_fixture(temp.path(), "solve.json", &composition);
    let evidence_path = write_evidence_fixture(temp.path(), "other-evidence.json", &other_evidence);

    let assert = canon_command()
        .arg("geo")
        .arg("ledger")
        .arg("build")
        .arg("--seed")
        .arg(&seed_path)
        .arg("--composition")
        .arg(format!("solve-a={}", composition_path.display()))
        .arg("--evidence")
        .arg(format!("evidence-a={}", evidence_path.display()))
        .assert()
        .failure();
    assert!(assert.get_output().stderr.is_empty());
    let output: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("refusal JSON parses");
    assert_eq!(output["outcome"], "REFUSAL");
    assert_eq!(output["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        output["refusal"]["detail"]["geo_ledger_error_code"],
        "invalid_input"
    );
    assert_eq!(
        output["refusal"]["detail"]["detail"]["field"],
        "composition.evidence_compilation.blake3"
    );
    assert_eq!(
        output["refusal"]["detail"]["detail"]["loan_id"],
        "loan-build-a"
    );
    assert_eq!(
        output["refusal"]["next_command"],
        "canon geo ledger build --seed <SEED.json> --composition <ARTIFACT_ID=COMPOSITION.json> --evidence <ARTIFACT_ID=EVIDENCE.json>"
    );
}

#[test]
fn t23_geo_ledger_build_cli_refuses_missing_bound_composition_artifact() {
    let temp = tempdir().expect("tempdir");
    let evidence = compile_evidence(&sample_evidence_request()).expect("seed evidence compiles");
    let mut seed = fixture_build_seed();
    seed.rows.truncate(1);
    let seed_path = write_seed_fixture(temp.path(), "seed.json", &seed);
    let evidence_path = write_evidence_fixture(temp.path(), "evidence.json", &evidence);

    let assert = canon_command()
        .arg("geo")
        .arg("ledger")
        .arg("build")
        .arg("--seed")
        .arg(&seed_path)
        .arg("--evidence")
        .arg(format!("evidence-a={}", evidence_path.display()))
        .assert()
        .failure();
    assert!(assert.get_output().stderr.is_empty());
    let output: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("refusal JSON parses");
    assert_eq!(output["outcome"], "REFUSAL");
    assert_eq!(output["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        output["refusal"]["detail"]["geo_ledger_error_code"],
        "ledger_sets_without_artifacts"
    );
    assert_eq!(
        output["refusal"]["detail"]["detail"]["field"],
        "composition_artifact_ref"
    );
    assert_eq!(
        output["refusal"]["detail"]["detail"]["artifact_ref"],
        "solve-a"
    );
    assert_eq!(
        output["refusal"]["detail"]["detail"]["loan_id"],
        "loan-build-a"
    );
    assert_eq!(
        output["refusal"]["next_command"],
        "canon geo ledger build --seed <SEED.json> --composition <ARTIFACT_ID=COMPOSITION.json> --evidence <ARTIFACT_ID=EVIDENCE.json>"
    );
}

#[test]
fn t23_geo_ledger_build_cli_refuses_missing_bound_evidence_artifact() {
    let temp = tempdir().expect("tempdir");
    let evidence = compile_evidence(&sample_evidence_request()).expect("seed evidence compiles");
    let composition = with_evidence_reference(
        sample_composition(GeoCompositionStatus::Resolved, vec!["parcel:seed:1"], 1),
        &evidence,
    );
    let mut seed = fixture_build_seed();
    seed.rows.truncate(1);
    let seed_path = write_seed_fixture(temp.path(), "seed.json", &seed);
    let composition_path = write_composition_fixture(temp.path(), "solve.json", &composition);

    let assert = canon_command()
        .arg("geo")
        .arg("ledger")
        .arg("build")
        .arg("--seed")
        .arg(&seed_path)
        .arg("--composition")
        .arg(format!("solve-a={}", composition_path.display()))
        .assert()
        .failure();
    assert!(assert.get_output().stderr.is_empty());
    let output: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("refusal JSON parses");
    assert_eq!(output["outcome"], "REFUSAL");
    assert_eq!(output["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        output["refusal"]["detail"]["geo_ledger_error_code"],
        "ledger_sets_without_artifacts"
    );
    assert_eq!(
        output["refusal"]["detail"]["detail"]["field"],
        "evidence_artifact_ref"
    );
    assert_eq!(
        output["refusal"]["detail"]["detail"]["artifact_ref"],
        "evidence-a"
    );
    assert_eq!(
        output["refusal"]["detail"]["detail"]["loan_id"],
        "loan-build-a"
    );
    assert_eq!(
        output["refusal"]["next_command"],
        "canon geo ledger build --seed <SEED.json> --composition <ARTIFACT_ID=COMPOSITION.json> --evidence <ARTIFACT_ID=EVIDENCE.json>"
    );
}

#[test]
fn t07_geo_ledger_cli_requires_a_subcommand() {
    let assert = canon_command().arg("geo").arg("ledger").assert().failure();
    assert!(assert.get_output().stderr.is_empty());
    let output: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("refusal JSON parses");
    assert_eq!(output["outcome"], "REFUSAL");
    assert_eq!(output["refusal"]["code"], "E_PARSE");
    assert_eq!(output["refusal"]["detail"]["command"], "canon geo ledger");
    assert_eq!(
        output["refusal"]["detail"]["subcommands"],
        json!(["build", "validate"])
    );
    assert_eq!(
        output["refusal"]["next_command"],
        "canon geo ledger build --seed <SEED.json> --composition <ARTIFACT_ID=COMPOSITION.json> --evidence <ARTIFACT_ID=EVIDENCE.json>"
    );
}

#[test]
fn t23_geo_ledger_validate_cli_refuses_invalid_ledger_artifact() {
    let temp = tempdir().expect("tempdir");
    let mut ledger = fixture_ledger();
    ledger.rows[0].truth_plane = None;
    ledger.rollups = vec![roll_up_deal(&ledger.rows[1..]).expect("partial rollup")];
    let ledger_path = temp.path().join("invalid-ledger.json");
    fs::write(
        &ledger_path,
        serde_json::to_vec(&ledger).expect("invalid ledger serializes"),
    )
    .expect("write invalid ledger fixture");

    let assert = canon_command()
        .arg("geo")
        .arg("ledger")
        .arg("validate")
        .arg("--ledger")
        .arg(&ledger_path)
        .assert()
        .failure();
    assert!(assert.get_output().stderr.is_empty());
    let output: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("refusal JSON parses");
    assert_eq!(output["outcome"], "REFUSAL");
    assert_eq!(output["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        output["refusal"]["detail"]["geo_ledger_error_code"],
        "ledger_truth_plane_pooled"
    );
    assert_eq!(
        output["refusal"]["next_command"],
        GEO_LEDGER_VALIDATE_NEXT_COMMAND
    );
}

#[test]
fn t26_fixture_pins_cannot_be_relabelled_as_live() {
    let ledger = fixture_ledger();
    validate_ledger(&ledger).expect("fixture ledger validates with fixture pins");

    let mut relabeled = ledger.clone();
    relabeled.rows[0].source_release_pins[0].source_dataset = "nyc.mappluto.26v1".to_string();
    let error = validate_ledger(&relabeled).expect_err("relabeled fixture pin refuses");
    assert_eq!(error.code, GeoLedgerErrorCode::InvalidInput);
    assert_eq!(error.detail["field"], "source_release_pins");
    assert_eq!(error.detail["source_dataset"], "nyc.mappluto.26v1");
    assert_eq!(error.detail["loan_id"], relabeled.rows[0].loan_id);

    let mut additive_row_value = serde_json::to_value(&ledger.rows[0]).expect("row JSON");
    additive_row_value["building_last_observed"] = json!([]);
    let additive_row: GeoLedgerRow =
        serde_json::from_value(additive_row_value).expect("row accepts additive fields");
    let mut additive_ledger = ledger.clone();
    additive_ledger.rows[0] = additive_row;
    additive_ledger.rollups = vec![roll_up_deal(&additive_ledger.rows).expect("rollup")];
    validate_ledger(&additive_ledger).expect("additive future row fields do not refuse");
}

#[test]
fn t27_ledger_module_has_no_fixture_or_solver_literals() {
    let source = std::fs::read_to_string("src/geo/ledger.rs").expect("ledger source");
    let lower = source.to_ascii_lowercase();
    for forbidden in [
        "1004540041",
        "chimera_wrongly_admitted",
        "asserted_address_core",
        "case_4",
        "franklin",
        "solve_composition",
        "openai",
        "anthropic",
        "gemini",
    ] {
        assert!(
            !lower.contains(&forbidden.to_ascii_lowercase()),
            "generic ledger module contains forbidden literal {forbidden}"
        );
    }
}

#[test]
fn collateral_ledger_schema_matches_a_real_instance() {
    let schema: Value = serde_json::from_str(include_str!(
        "../schemas/canon.geo.collateral_ledger.v0.schema.json"
    ))
    .expect("schema parses");
    assert_eq!(schema["title"], "canon.geo.collateral_ledger.v0");
    assert_eq!(
        schema["properties"]["version"]["const"],
        CANON_GEO_COLLATERAL_LEDGER_VERSION
    );
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["$defs"]["ledger_row"]["additionalProperties"], true);
    assert_eq!(
        schema["$defs"]["ledger_row"]["properties"]["last_observed_present"]["$ref"],
        "#/$defs/valid_time_interval"
    );
    assert_eq!(
        schema["$defs"]["valid_time_interval"]["required"],
        json!(["start_day", "end_day"])
    );
    assert_eq!(
        schema["$defs"]["deal_rollup"]["properties"]["truth_planes"]["additionalProperties"]["$ref"],
        "#/$defs/plane_counts"
    );

    let ledger = fixture_ledger();
    let instance = serde_json::to_value(&ledger).expect("ledger JSON");
    assert_eq!(instance["version"], CANON_GEO_COLLATERAL_LEDGER_VERSION);
    assert!(instance["rows"].as_array().expect("rows").len() == 15);
    assert!(
        canonical_collateral_ledger_bytes(&ledger)
            .expect("canonical ledger bytes")
            .starts_with(b"{\"version\":\"canon_geo_collateral_ledger.v0\"")
    );
}

fn fixture_ledger() -> GeoCollateralLedger {
    let eval = load_restack_evaluation();
    let population = load_population_request();
    let loan_keys = load_loan_keys();
    let population_by_case = population
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();
    let mut rows = Vec::new();
    for case in &eval.cases {
        let population_case = population_by_case
            .get(case.case_id.as_str())
            .expect("population case");
        let loan = GeoLedgerLoanRef {
            accession: SYNTHETIC_ACCESSION.to_string(),
            deal_id: SYNTHETIC_DEAL.to_string(),
            loan_id: loan_keys
                .get(case.case_id.as_str())
                .expect("loan key")
                .clone(),
            deed_ids: Vec::new(),
        };
        let reach = if case.case_id == FORCED_REACH_NONE_CASE {
            GeoCandidateReachStatus::None
        } else {
            match case.candidate_reach.as_str() {
                "full" => GeoCandidateReachStatus::Full,
                "partial" | "none" => GeoCandidateReachStatus::Partial,
                other => panic!("unknown candidate reach {other}"),
            }
        };
        let reach_none_reason =
            (reach == GeoCandidateReachStatus::None).then(|| FORCED_REACH_NONE_REASON.to_string());
        let evidence =
            compile_evidence(&population_case.evidence).expect("compile fixture evidence");
        let composition = with_evidence_reference(
            composition_from_restack_case(case, population_case),
            &evidence,
        );
        rows.push(
            build_ledger_row(
                &loan,
                reach,
                reach_none_reason,
                Some(&composition),
                Some(&evidence),
                Some(population_case.truth_plane),
                &[fixture_pin()],
            )
            .expect("build ledger row"),
        );
    }
    let ledger =
        build_collateral_ledger(rows, GeoCollateralLedgerProofClass::Fixture).expect("ledger");
    assert_eq!(
        ledger
            .rows
            .iter()
            .find(|row| row.loan_id == FORCED_REACH_NONE_LOAN)
            .expect("forced row")
            .reach,
        GeoCandidateReachStatus::None
    );
    ledger
}

fn composition_from_restack_case(
    case: &RestackCaseEvaluation,
    population_case: &GeoLabeledCompositionCase,
) -> GeoCompositionArtifact {
    let status = composition_status(&case.status);
    let hard_forced = GeoCompositionBackbone {
        parcels: sorted_unique(case.hard_forced.parcels.clone()),
        buildings: sorted_unique(case.hard_forced.buildings.clone()),
    };
    let residual_models = residual_models_for_case(status, &hard_forced, population_case);
    GeoCompositionArtifact {
        version: CANON_GEO_COMPOSITION_VERSION.to_string(),
        request_version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        evidence_compilation: None,
        status,
        resolved_claim: None,
        summary: GeoCompositionSummary {
            parcel_candidates: case.candidate_members,
            building_candidates: 0,
            candidate_assignments: case
                .residual_model_count
                .unwrap_or(case.candidate_members as u64),
            candidate_assignments_saturated: case.residual_count_saturated,
            structurally_feasible_assignments: case.residual_model_count.unwrap_or_default(),
            structurally_feasible_assignments_complete: case.residual_count_complete,
            structurally_feasible_assignments_saturated: case.residual_count_saturated,
            hard_constraint_evaluations: 0,
            hard_constraint_evaluations_complete: case.status != "component_budget_fallback",
            hard_constraint_evaluations_saturated: false,
            residual_model_count: case.residual_model_count.unwrap_or_default(),
            model_count_scope: GeoModelCountScope::EntitySelection,
            residual_model_count_complete: case.residual_count_complete,
            residual_model_count_saturated: case.residual_count_saturated,
            summary_counts_saturated: case.residual_count_saturated,
            component_count: usize::from(status != GeoCompositionStatus::BudgetFallback),
            residual_models_materialized: !residual_models.is_empty(),
        },
        hard_forced,
        backbone_complete: case.backbone_complete,
        factorization: Vec::new(),
        residual_models,
        soft_ranked: Vec::new(),
        conflict_constraint_ids: Vec::new(),
        conflict_core_complete: None,
        budget_fallback: (status == GeoCompositionStatus::BudgetFallback).then(|| {
            GeoCompositionFallback {
                component_keys: Vec::new(),
                max_component_variables: case.candidate_members,
                configured_max_assignments: 0,
                guidance: "fixture restack receipt reached component-budget fallback".to_string(),
            }
        }),
        entity_projection: None,
    }
}

fn residual_models_for_case(
    status: GeoCompositionStatus,
    hard_forced: &GeoCompositionBackbone,
    population_case: &GeoLabeledCompositionCase,
) -> Vec<GeoCompositionModel> {
    match status {
        GeoCompositionStatus::Resolved => vec![GeoCompositionModel {
            parcels: hard_forced.parcels.clone(),
            buildings: hard_forced.buildings.clone(),
        }],
        GeoCompositionStatus::Ambiguous => {
            let candidates = population_case
                .evidence
                .universe
                .parcels
                .iter()
                .filter(|parcel| !hard_forced.parcels.binary_search(parcel).is_ok())
                .take(2)
                .cloned()
                .collect::<Vec<_>>();
            assert!(
                candidates.len() >= 2,
                "fixture ambiguous rows need at least two candidate parcels"
            );
            candidates
                .into_iter()
                .map(|candidate| {
                    let mut parcels = hard_forced.parcels.clone();
                    parcels.push(candidate);
                    parcels = sorted_unique(parcels);
                    GeoCompositionModel {
                        parcels,
                        buildings: hard_forced.buildings.clone(),
                    }
                })
                .collect()
        }
        GeoCompositionStatus::Conflict | GeoCompositionStatus::BudgetFallback => Vec::new(),
    }
}

fn composition_status(status: &str) -> GeoCompositionStatus {
    match status {
        "resolved" => GeoCompositionStatus::Resolved,
        "ambiguous" => GeoCompositionStatus::Ambiguous,
        "conflict" => GeoCompositionStatus::Conflict,
        "component_budget_fallback" => GeoCompositionStatus::BudgetFallback,
        other => panic!("unknown restack status {other}"),
    }
}

fn sample_evidence_request() -> GeoEvidenceCompilationRequest {
    load_population_request().cases[0].evidence.clone()
}

fn sample_composition(
    status: GeoCompositionStatus,
    parcel_ids: Vec<&str>,
    residual_model_count: u64,
) -> GeoCompositionArtifact {
    GeoCompositionArtifact {
        version: CANON_GEO_COMPOSITION_VERSION.to_string(),
        request_version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        evidence_compilation: None,
        status,
        resolved_claim: None,
        summary: GeoCompositionSummary {
            parcel_candidates: parcel_ids.len(),
            building_candidates: 0,
            candidate_assignments: residual_model_count,
            candidate_assignments_saturated: false,
            structurally_feasible_assignments: residual_model_count,
            structurally_feasible_assignments_complete: true,
            structurally_feasible_assignments_saturated: false,
            hard_constraint_evaluations: residual_model_count,
            hard_constraint_evaluations_complete: true,
            hard_constraint_evaluations_saturated: false,
            residual_model_count,
            model_count_scope: GeoModelCountScope::EntitySelection,
            residual_model_count_complete: true,
            residual_model_count_saturated: false,
            summary_counts_saturated: false,
            component_count: 1,
            residual_models_materialized: true,
        },
        hard_forced: GeoCompositionBackbone {
            parcels: parcel_ids.iter().map(|parcel| parcel.to_string()).collect(),
            buildings: Vec::new(),
        },
        backbone_complete: true,
        factorization: Vec::new(),
        residual_models: vec![GeoCompositionModel {
            parcels: parcel_ids.iter().map(|parcel| parcel.to_string()).collect(),
            buildings: Vec::new(),
        }],
        soft_ranked: Vec::new(),
        conflict_constraint_ids: Vec::new(),
        conflict_core_complete: None,
        budget_fallback: None,
        entity_projection: None,
    }
}

fn with_evidence_reference(
    mut composition: GeoCompositionArtifact,
    evidence: &GeoEvidenceCompilationArtifact,
) -> GeoCompositionArtifact {
    let canonical = canonical_evidence_compilation_bytes(evidence).expect("evidence canonicalizes");
    composition.evidence_compilation = Some(GeoEvidenceCompilationReference {
        version: evidence.version.clone(),
        request_version: evidence.request_version.clone(),
        blake3: blake3::hash(&canonical).to_hex().to_string(),
    });
    composition
}

fn evidence_blake3(evidence: &GeoEvidenceCompilationArtifact) -> String {
    let canonical = canonical_evidence_compilation_bytes(evidence).expect("evidence canonicalizes");
    format!("blake3:{}", blake3::hash(&canonical).to_hex())
}

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn fixture_loan(loan_id: &str) -> GeoLedgerLoanRef {
    GeoLedgerLoanRef {
        accession: SYNTHETIC_ACCESSION.to_string(),
        deal_id: SYNTHETIC_DEAL.to_string(),
        loan_id: loan_id.to_string(),
        deed_ids: Vec::new(),
    }
}

fn fixture_build_seed() -> GeoCollateralLedgerSeed {
    GeoCollateralLedgerSeed {
        version: CANON_GEO_COLLATERAL_LEDGER_SEED_VERSION.to_string(),
        proof_class: GeoCollateralLedgerProofClass::Fixture,
        rows: vec![
            GeoCollateralLedgerSeedRow {
                accession: SYNTHETIC_ACCESSION.to_string(),
                deal_id: SYNTHETIC_DEAL.to_string(),
                loan_id: "loan-build-a".to_string(),
                reach: GeoCandidateReachStatus::Full,
                reach_none_reason: None,
                deed_ids: vec!["deed:fixture:1".to_string()],
                truth_plane: Some(GeoTruthPlane::GateV2Historical),
                source_release_pins: vec![fixture_pin()],
                composition_artifact_ref: Some("solve-a".to_string()),
                evidence_artifact_ref: Some("evidence-a".to_string()),
                property_refs: vec![GeoLedgerPropertyRef {
                    property_id: "property:loan-build-a".to_string(),
                    parcel_ids: vec!["parcel:seed:1".to_string()],
                    building_ids: Vec::new(),
                }],
                last_observed_present: Some(GeoValidTimeInterval {
                    start_day: 19_700,
                    end_day: 19_730,
                }),
            },
            GeoCollateralLedgerSeedRow {
                accession: SYNTHETIC_ACCESSION.to_string(),
                deal_id: SYNTHETIC_DEAL.to_string(),
                loan_id: "loan-no-reach".to_string(),
                reach: GeoCandidateReachStatus::None,
                reach_none_reason: Some(FORCED_REACH_NONE_REASON.to_string()),
                deed_ids: Vec::new(),
                truth_plane: Some(GeoTruthPlane::GateV2Historical),
                source_release_pins: vec![fixture_pin()],
                composition_artifact_ref: None,
                evidence_artifact_ref: None,
                property_refs: Vec::new(),
                last_observed_present: None,
            },
        ],
    }
}

fn write_seed_fixture(dir: &Path, name: &str, seed: &GeoCollateralLedgerSeed) -> PathBuf {
    let path = dir.join(name);
    let bytes = canonical_collateral_ledger_seed_bytes(seed).expect("seed canonicalizes");
    fs::write(&path, bytes).expect("write seed fixture");
    path
}

fn write_composition_fixture(
    dir: &Path,
    name: &str,
    composition: &GeoCompositionArtifact,
) -> PathBuf {
    let path = dir.join(name);
    let bytes = canonical_composition_bytes(composition).expect("composition canonicalizes");
    fs::write(&path, bytes).expect("write composition fixture");
    path
}

fn write_evidence_fixture(
    dir: &Path,
    name: &str,
    evidence: &GeoEvidenceCompilationArtifact,
) -> PathBuf {
    let path = dir.join(name);
    let bytes = canonical_evidence_compilation_bytes(evidence).expect("evidence canonicalizes");
    fs::write(&path, bytes).expect("write evidence fixture");
    path
}

fn fixture_pin() -> GeoSourceReleasePin {
    GeoSourceReleasePin {
        source_dataset: "fixture.e4_gate_v2_restack".to_string(),
        source_release: "2026-09-03".to_string(),
        blake3: format!(
            "blake3:{}",
            blake3::hash(E4_RESTACK_EVALUATION_BYTES).to_hex()
        ),
    }
}

fn load_restack_evaluation() -> RestackEvaluation {
    serde_json::from_str(include_str!(
        "../scripts/geo_measurements/fixtures/e4_gate_v2_restack_2026-09-03/e4_eval_roll.json"
    ))
    .expect(E4_RESTACK_EVALUATION)
}

fn load_population_request() -> GeoPopulationEvaluationRequest {
    serde_json::from_str(include_str!(
        "../tests/fixtures/geo/e4_gate_v2_population_request.json"
    ))
    .expect(E4_POPULATION_REQUEST)
}

fn load_loan_keys() -> BTreeMap<String, String> {
    let enrichment: EvidenceEnrichment = serde_json::from_str(include_str!(
        "../tests/fixtures/geo/e4_gate_v2_evidence_enrichment.json"
    ))
    .expect(E4_ENRICHMENT);
    enrichment
        .cases
        .into_iter()
        .map(|case| (case.case_id, case.loan_key))
        .collect()
}

fn sorted_unique(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[derive(Debug, Deserialize)]
struct RestackEvaluation {
    cases: Vec<RestackCaseEvaluation>,
}

#[derive(Debug, Deserialize)]
struct RestackCaseEvaluation {
    case_id: String,
    status: String,
    candidate_reach: String,
    candidate_members: usize,
    residual_model_count: Option<u64>,
    residual_count_complete: bool,
    residual_count_saturated: bool,
    hard_forced: RestackForcedSet,
    backbone_complete: bool,
}

#[derive(Debug, Deserialize)]
struct RestackForcedSet {
    parcels: Vec<String>,
    buildings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EvidenceEnrichment {
    cases: Vec<EvidenceEnrichmentCase>,
}

#[derive(Debug, Deserialize)]
struct EvidenceEnrichmentCase {
    case_id: String,
    loan_key: String,
}
