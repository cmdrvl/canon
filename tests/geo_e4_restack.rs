#![forbid(unsafe_code)]

use canon::geo::assessment_roll::{
    produce_assessment_roll_owner_evidence, GeoAssessmentRollCaseDocument, GeoAssessmentRollLotRow,
    GeoAssessmentRollOwnerCalibration, GeoAssessmentRollOwnerContractSource,
    GeoAssessmentRollOwnerProofClass, GeoAssessmentRollOwnerRequest, GeoAssessmentRollPartyRow,
    CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION,
};
use canon::geo::footprint_roll::GeoAssessmentRollGrossSqftRow;
use canon::geo::{
    build_condo_bridge, canonical_population_evaluation_bytes,
    evaluate_population_with_run_artifacts, materialize_footprint_roll_evidence,
    stack_population_evidence, GeoBuildingFootprintRow, GeoCompositionBackbone,
    GeoCompositionProfile, GeoCompositionUniverse, GeoCondoBridgeCaseRequest,
    GeoCondoBridgeRequest, GeoEntityLevel, GeoFootprintRollCalibration,
    GeoFootprintRollEvidenceRequest, GeoFootprintRollLoanFields, GeoFootprintRollSourceConfig,
    GeoPopulationCaseEvaluation, GeoPopulationCaseStatus, GeoPopulationEvaluationArtifact,
    GeoPopulationEvaluationRequest, GeoPopulationEvidenceStackRequest, GeoRhoContract,
    GeoRhoObservation, GeoRhoObservationKind, CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION,
    CANON_GEO_FOOTPRINT_ROLL_EVIDENCE_REQUEST_VERSION,
    CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION,
    CANON_GEO_POPULATION_EVIDENCE_STACK_VERSION, CANON_GEO_POPULATION_REQUEST_VERSION,
    DEFAULT_MAX_MATERIALIZED_MODELS, GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID,
    GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_CONTRACT_ID,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const FIXTURE_CLASS: &str = "fixture_replay_observed_warehouse_snapshot_not_live";
const MEASUREMENT_VERSION: &str = "canon_geo_e4_gate_v2_restack_measurement.v0";
const MCP_STACK_DIR: &str = "scripts/geo_measurements/fixtures/d1_residuals/mcp_stack_2026-09-03";
const OUT_DIR: &str = "scripts/geo_measurements/fixtures/e4_gate_v2";
const MEASUREMENT_MAX_ASSIGNMENTS: u64 = 2_097_152;

const EXPECTED_BASELINE: G1Numbers = G1Numbers {
    cases: 15,
    evidence_no_observation_cases: 2,
    reachable_cases: 7,
    resolved_cases: 0,
    ambiguous_cases: 15,
    conflict_cases: 0,
    component_budget_fallback_cases: 0,
    deed_exact_cases: 0,
    false_merge_cases: 0,
    truth_exclusion_cases: 0,
    residual_count_le16_cases: 0,
};
const EXPECTED_STACKED: G1Numbers = G1Numbers {
    cases: 15,
    evidence_no_observation_cases: 0,
    reachable_cases: 7,
    resolved_cases: 4,
    ambiguous_cases: 9,
    conflict_cases: 0,
    component_budget_fallback_cases: 2,
    deed_exact_cases: 3,
    false_merge_cases: 0,
    truth_exclusion_cases: 1,
    residual_count_le16_cases: 8,
};

#[derive(Debug, Deserialize)]
struct E4Bindings {
    note: String,
    cases: Vec<E4Binding>,
}

#[derive(Debug, Clone, Deserialize)]
struct E4Binding {
    case_id: String,
    truth_parcels: Vec<String>,
    loan_keys: Vec<String>,
    document_ids: Vec<String>,
    borrower_names_norm: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RetainedD1PopulationStack {
    version: String,
    population: GeoPopulationEvaluationRequest,
}

#[derive(Debug, Deserialize)]
struct RollFixtureRow {
    owner: String,
    gross_sqft: String,
    units: String,
    #[serde(default)]
    condo: String,
}

#[derive(Debug, Deserialize)]
struct FootprintSummaryRow {
    bins: u64,
}

#[derive(Debug, Deserialize)]
struct RollGsfCalibration {
    rows: Vec<RollGsfCalibrationRow>,
}

#[derive(Debug, Deserialize)]
struct RollGsfCalibrationRow {
    subject_id: String,
    asserted: f64,
}

#[derive(Debug, Deserialize)]
struct FootprintCalibration {
    rows: Vec<FootprintCalibrationRow>,
}

#[derive(Debug, Deserialize)]
struct FootprintCalibrationRow {
    subject_id: String,
    property_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MeasurementSummary {
    version: String,
    fixture_class: String,
    proof_class: String,
    source_fixture_digests: Vec<SourceFixtureDigest>,
    stage_notes: Vec<String>,
    stage_summaries: BTreeMap<String, Value>,
    g1_numbers: BTreeMap<String, G1Numbers>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct SourceFixtureDigest {
    path: String,
    blake3: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct G1Numbers {
    cases: u64,
    evidence_no_observation_cases: u64,
    reachable_cases: u64,
    resolved_cases: u64,
    ambiguous_cases: u64,
    conflict_cases: u64,
    component_budget_fallback_cases: u64,
    deed_exact_cases: u64,
    false_merge_cases: u64,
    truth_exclusion_cases: u64,
    residual_count_le16_cases: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MeasurementCase {
    case_id: String,
    d1_subject_id: String,
    loan_key: String,
    document_id: String,
    truth_parcels: Vec<String>,
    stage_observations: StageObservationCounts,
    condo_bridge: Option<CondoBridgeCaseSummary>,
    baseline: CaseOutcome,
    stacked: CaseOutcome,
    explanation: Option<ConflictExplanationSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StageObservationCounts {
    retained_pad: u64,
    retained_geocode_preference: u64,
    assessment_roll_owner_exact: u64,
    assessment_roll_affiliate_preference: u64,
    roll_gsf_band: u64,
    footprint_floor: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CondoBridgeCaseSummary {
    kind: String,
    unit_lots: u64,
    truth_billing_grain: Vec<String>,
    universe_billing_grain_count: u64,
    billing_truth_in_universe: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CaseOutcome {
    status: String,
    residual_count: Option<u64>,
    residual_count_saturated: bool,
    truth_in_universe: String,
    truth_model_in_residual: Option<bool>,
    forced: GeoCompositionBackbone,
    deed_exact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ConflictExplanationSummary {
    core_observation_ids: Vec<String>,
    core_constraint_ids: Vec<String>,
    core_source_record_ids: Vec<String>,
    complete: bool,
}

struct MeasurementBundle {
    base_population: GeoPopulationEvaluationRequest,
    widened_population: GeoPopulationEvaluationRequest,
    pad_only_overlay: GeoPopulationEvidenceStackRequest,
    stacked_overlay: GeoPopulationEvidenceStackRequest,
    summary: MeasurementSummary,
    cases: Vec<MeasurementCase>,
}

#[test]
fn e4_gate_v2_restack_measurement_replays_from_retained_receipts() {
    let bundle = build_measurement();

    assert_eq!(
        bundle.summary.g1_numbers.get("pad_only_baseline"),
        Some(&EXPECTED_BASELINE)
    );
    assert_eq!(
        bundle.summary.g1_numbers.get("stacked"),
        Some(&EXPECTED_STACKED)
    );
    assert!(
        bundle
            .summary
            .proof_class
            .contains("fixture replay of retained warehouse snapshot"),
        "measurement must not be represented as live proof"
    );

    assert_committed_json("base_population_request.json", &bundle.base_population);
    assert_committed_json(
        "widened_population_request.json",
        &bundle.widened_population,
    );
    assert_committed_json("pad_only_overlay_request.json", &bundle.pad_only_overlay);
    assert_committed_json("stacked_overlay_request.json", &bundle.stacked_overlay);
    assert_committed_json("summary.json", &bundle.summary);
    assert_committed_json("cases.json", &bundle.cases);
}

#[test]
#[ignore = "writes committed bd-2ezy measurement artifacts when CANON_GEO_E4_RESTACK_WRITE=1"]
fn rewrite_e4_gate_v2_measurement_artifacts() {
    if env::var("CANON_GEO_E4_RESTACK_WRITE").as_deref() != Ok("1") {
        eprintln!("set CANON_GEO_E4_RESTACK_WRITE=1 to rewrite the fixture artifacts");
        return;
    }
    let out_dir = rooted(OUT_DIR);
    fs::create_dir_all(&out_dir).expect("measurement output dir can be created");
    let bundle = build_measurement();
    write_pretty_json(
        out_dir.join("base_population_request.json"),
        &bundle.base_population,
    );
    write_pretty_json(
        out_dir.join("widened_population_request.json"),
        &bundle.widened_population,
    );
    write_pretty_json(
        out_dir.join("pad_only_overlay_request.json"),
        &bundle.pad_only_overlay,
    );
    write_pretty_json(
        out_dir.join("stacked_overlay_request.json"),
        &bundle.stacked_overlay,
    );
    write_pretty_json(out_dir.join("summary.json"), &bundle.summary);
    write_pretty_json(out_dir.join("cases.json"), &bundle.cases);
}

fn build_measurement() -> MeasurementBundle {
    let mut base_population: GeoPopulationEvaluationRequest = read_json(rooted(
        "tests/fixtures/geo/e4_gate_v2_population_request.json",
    ));
    assert_eq!(
        base_population.version,
        CANON_GEO_POPULATION_REQUEST_VERSION
    );
    assert_eq!(base_population.cases.len(), 15);

    let bindings: E4Bindings = read_json(rooted(&format!("{MCP_STACK_DIR}/e4_case_bindings.json")));
    assert!(
        bindings.note.contains("fixture class"),
        "binding receipt must be labeled as fixture class"
    );
    let binding_by_case = bindings_by_case(&bindings);
    assert_eq!(binding_by_case.len(), base_population.cases.len());

    let roll_rows_by_bbl: BTreeMap<String, RollFixtureRow> = read_json_gz(rooted(&format!(
        "{MCP_STACK_DIR}/assessment_roll_fy2026p3_lots.json.gz"
    )));
    let roll_stage_rows =
        assessment_roll_owner_rows_for_candidate_blocks(&base_population, &roll_rows_by_bbl);
    let party_rows = party_rows_from_bindings(&bindings);
    let owner_artifact = produce_assessment_roll_owner_evidence(&GeoAssessmentRollOwnerRequest {
        version: CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION.to_string(),
        proof_class: GeoAssessmentRollOwnerProofClass::Fixture,
        population: base_population.clone(),
        case_documents: bindings
            .cases
            .iter()
            .map(|binding| GeoAssessmentRollCaseDocument {
                case_id: binding.case_id.clone(),
                document_id: only(&binding.document_ids, "document_ids").clone(),
            })
            .collect(),
        contract_source: GeoAssessmentRollOwnerContractSource {
            source_dataset:
                "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION_FY2026P3_x_ACRIS_PARTIES"
                    .to_string(),
            source_release: "FY2026P3_acris-latest".to_string(),
            source_lineage_ids: vec![
                "EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:latest".to_string(),
                "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.WRGL_NYC_OPENDATA_PROPERTY_VALUATION_AND_ASSESSMENT_DATA_TAX_CLASSES_1_2_3_4__STRUCTURED:FY2026P3".to_string(),
            ],
        },
        calibration: GeoAssessmentRollOwnerCalibration {
            population_id: "h7-d1-residuals-2026-09-03-roll".to_string(),
            calibration_blake3: calibration_receipt_digest(&format!(
                "{MCP_STACK_DIR}/calibration_roll_owner_exact.json"
            )),
            exact_falsification_rule_id: "truth-lot-owner-not-exact".to_string(),
            affiliate_falsification_rule_id: "truth-lot-owner-mismatch".to_string(),
        },
        roll_rows: roll_stage_rows,
        party_rows,
        max_cases: 15,
        max_roll_rows: roll_rows_by_bbl.len(),
        max_party_rows: 128,
        max_overlay_observations: 2_000,
    })
    .expect("assessment-roll owner stage must materialize");

    cap_measurement_budgets(&mut base_population);
    let mut widened_population = owner_artifact.widened_population.clone();
    cap_measurement_budgets(&mut widened_population);
    assert_eq!(widened_population.cases.len(), 15);

    let retained_roll_overlay: GeoPopulationEvidenceStackRequest = read_json_gz(rooted(&format!(
        "{MCP_STACK_DIR}/overlay_request_roll_exact_owner_gsf_band.json.gz"
    )));
    let retained_footprint_overlay: GeoPopulationEvidenceStackRequest = read_json_gz(rooted(
        &format!("{MCP_STACK_DIR}/overlay_request_soft_owner_footprint.json.gz"),
    ));
    let subject_by_case = d1_subject_by_case(&base_population);
    let roll_gsf_by_subject = roll_gsf_by_subject();
    let footprint_count_by_subject = footprint_count_by_subject();
    let footprints_by_bbl: BTreeMap<String, FootprintSummaryRow> =
        read_json(rooted(&format!("{MCP_STACK_DIR}/footprints.json")));

    let condo_artifact = build_condo_bridge(&GeoCondoBridgeRequest {
        version: CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION.to_string(),
        source_dataset: "EDGAR_DB.SOURCE.NYC_DCP_PAD_BBL_HOT".to_string(),
        source_release: "26B/2026-05-01".to_string(),
        source_lineage_ids: vec!["EDGAR_DB.SOURCE.NYC_DCP_PAD_BBL_HOT:26B".to_string()],
        pad_rows: read_json_gz(rooted(&format!("{MCP_STACK_DIR}/pad_bbl.json.gz"))),
        cases: widened_population
            .cases
            .iter()
            .map(|case| {
                let binding = binding_by_case.get(case.id.as_str()).expect("case binding");
                GeoCondoBridgeCaseRequest {
                    case_id: case.id.clone(),
                    loan_key: Some(only(&binding.loan_keys, "loan_keys").clone()),
                    truth_parcels: case.truth.parcels.clone(),
                    universe_parcels: case.evidence.universe.parcels.clone(),
                }
            })
            .collect(),
        max_pad_rows: 1_000,
        max_cases: 15,
    })
    .expect("condo bridge stage must replay");

    let retained_pad_only = retained_overlay_for_e4_cases(
        &base_population,
        &binding_by_case,
        &subject_by_case,
        &retained_roll_overlay,
        &BTreeSet::from(["rho.address.pad.membership"]),
    );
    let retained_pad_and_geocode = retained_overlay_for_e4_cases(
        &widened_population,
        &binding_by_case,
        &subject_by_case,
        &retained_roll_overlay,
        &BTreeSet::from([
            "rho.address.pad.membership",
            "rho.address.geocode.parcel_containment",
        ]),
    );
    let footprint_roll_overlay = footprint_roll_overlay_for_e4_cases(FootprintRollOverlayInputs {
        population: &widened_population,
        binding_by_case: &binding_by_case,
        subject_by_case: &subject_by_case,
        roll_gsf_by_subject: &roll_gsf_by_subject,
        footprint_count_by_subject: &footprint_count_by_subject,
        retained_roll_overlay: &retained_roll_overlay,
        retained_footprint_overlay: &retained_footprint_overlay,
        roll_rows_by_bbl: &roll_rows_by_bbl,
        footprints_by_bbl: &footprints_by_bbl,
    });
    let stacked_overlay = merge_overlay_requests(
        15,
        vec![
            retained_pad_and_geocode.clone(),
            owner_artifact.overlay.clone(),
            footprint_roll_overlay,
        ],
    );
    let pad_only_overlay = merge_overlay_requests(15, vec![retained_pad_only]);

    let pad_only_stack = stack_population_evidence(&base_population, &pad_only_overlay)
        .expect("PAD-only stack request must compile");
    let stacked_stack = stack_population_evidence(&widened_population, &stacked_overlay)
        .expect("stacked evidence request must compile");
    let pad_only_with_artifacts = evaluate_population_with_run_artifacts(
        &pad_only_stack.population,
        rooted("target/geo_e4_restack/pad_only"),
    )
    .expect("PAD-only run-path population must evaluate");
    let pad_only_eval = pad_only_with_artifacts.evaluation.clone();
    let stacked_with_artifacts = evaluate_population_with_run_artifacts(
        &stacked_stack.population,
        rooted("target/geo_e4_restack/stacked"),
    )
    .expect("stacked run-path population must evaluate");
    let stacked_eval = stacked_with_artifacts.evaluation.clone();

    let cases = measurement_cases(
        &bindings,
        &subject_by_case,
        &pad_only_eval,
        &stacked_eval,
        &stacked_with_artifacts.case_artifacts,
        &stacked_overlay,
        &condo_artifact.rows,
    );
    let summary = measurement_summary(
        &pad_only_eval,
        &stacked_eval,
        &owner_artifact.summary,
        &condo_artifact.stats,
        &stacked_overlay,
        &cases,
    );

    MeasurementBundle {
        base_population,
        widened_population,
        pad_only_overlay,
        stacked_overlay,
        summary,
        cases,
    }
}

fn bindings_by_case(bindings: &E4Bindings) -> BTreeMap<&str, &E4Binding> {
    let mut by_case = BTreeMap::new();
    for binding in &bindings.cases {
        assert!(
            by_case.insert(binding.case_id.as_str(), binding).is_none(),
            "duplicate case binding {}",
            binding.case_id
        );
    }
    by_case
}

fn assessment_roll_owner_rows_for_candidate_blocks(
    population: &GeoPopulationEvaluationRequest,
    roll_rows_by_bbl: &BTreeMap<String, RollFixtureRow>,
) -> Vec<GeoAssessmentRollLotRow> {
    let blocks = population
        .cases
        .iter()
        .flat_map(|case| case.evidence.universe.parcels.iter())
        .map(|bbl| block_key(bbl).to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        blocks.len(),
        25,
        "the frozen E4 candidate population has 25 blocks"
    );
    let roll_blocks = roll_rows_by_bbl
        .keys()
        .map(|bbl| block_key(bbl).to_string())
        .collect::<BTreeSet<_>>();
    let missing = blocks.difference(&roll_blocks).cloned().collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "roll fixture is missing E4 blocks: {missing:?}"
    );

    roll_rows_by_bbl
        .iter()
        .filter(|(bbl, _)| blocks.contains(block_key(bbl)))
        .map(|(bbl, row)| GeoAssessmentRollLotRow {
            bbl: bbl.clone(),
            owner: row.owner.clone(),
            gross_sqft: row.gross_sqft.clone(),
            units: row.units.clone(),
            condo_number: row.condo.clone(),
            source_record_id: format!(
                "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:{bbl}"
            ),
            source_vintage: "FY2026P3".to_string(),
        })
        .collect()
}

fn party_rows_from_bindings(bindings: &E4Bindings) -> Vec<GeoAssessmentRollPartyRow> {
    let mut rows = BTreeMap::new();
    for binding in &bindings.cases {
        let document_id = only(&binding.document_ids, "document_ids");
        for borrower in &binding.borrower_names_norm {
            rows.entry((document_id.clone(), borrower.clone()))
                .or_insert_with(|| GeoAssessmentRollPartyRow {
                    document_id: document_id.clone(),
                    party_type: "1".to_string(),
                    party_name_norm: borrower.clone(),
                    source_record_id: format!(
                        "EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_PARTIES_EXT:2026-08-10:{document_id}:{}",
                        borrower.replace(' ', "_")
                    ),
                    source_vintage: "2026-08-10".to_string(),
                });
        }
    }
    rows.into_values().collect()
}

fn d1_subject_by_case(population: &GeoPopulationEvaluationRequest) -> BTreeMap<String, String> {
    let retained: RetainedD1PopulationStack = read_json(rooted(
        "scripts/geo_measurements/fixtures/d1_residuals/d1_population_evidence_stack.json",
    ));
    assert_eq!(
        retained.version, CANON_GEO_POPULATION_EVIDENCE_STACK_VERSION,
        "retained D1 stack fixture must be a population evidence-stack artifact"
    );
    let mut d1_by_truth = BTreeMap::<Vec<String>, String>::new();
    for case in retained.population.cases {
        let key = truth_key(&case.truth.parcels);
        d1_by_truth.entry(key).or_insert(case.id);
    }

    let mut subject_by_case = BTreeMap::new();
    for case in &population.cases {
        let key = truth_key(&case.truth.parcels);
        let subject_id = d1_by_truth.get(&key).unwrap_or_else(|| {
            panic!("E4 case {} must bind to a D1 subject by truth set", case.id)
        });
        subject_by_case.insert(case.id.clone(), subject_id.clone());
    }
    subject_by_case
}

fn roll_gsf_by_subject() -> BTreeMap<String, u64> {
    read_json::<RollGsfCalibration>(rooted(&format!(
        "{MCP_STACK_DIR}/calibration_roll_gsf_band.json"
    )))
    .rows
    .into_iter()
    .map(|row| {
        assert!(
            row.asserted.fract() == 0.0 && row.asserted >= 500.0,
            "roll GSF calibration asserted size must be whole sqft >=500"
        );
        (row.subject_id, row.asserted as u64)
    })
    .collect()
}

fn footprint_count_by_subject() -> BTreeMap<String, u64> {
    read_json::<FootprintCalibration>(rooted(&format!(
        "{MCP_STACK_DIR}/calibration_footprint.json"
    )))
    .rows
    .into_iter()
    .map(|row| (row.subject_id, row.property_count))
    .collect()
}

fn retained_overlay_for_e4_cases(
    population: &GeoPopulationEvaluationRequest,
    binding_by_case: &BTreeMap<&str, &E4Binding>,
    subject_by_case: &BTreeMap<String, String>,
    retained_overlay: &GeoPopulationEvidenceStackRequest,
    contract_ids: &BTreeSet<&str>,
) -> GeoPopulationEvidenceStackRequest {
    let retained_by_case = retained_overlay
        .case_overlays
        .iter()
        .map(|overlay| (overlay.case_id.as_str(), overlay))
        .collect::<BTreeMap<_, _>>();
    let mut overlays = Vec::new();
    for case in &population.cases {
        let binding = binding_by_case
            .get(case.id.as_str())
            .expect("E4 case binding");
        let d1_subject = d1_subject_id(&binding.case_id, subject_by_case);
        let Some(retained) = retained_by_case.get(d1_subject.as_str()) else {
            continue;
        };
        let universe = case
            .evidence
            .universe
            .parcels
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut observations = Vec::new();
        for observation in &retained.observations {
            if !contract_ids.contains(observation.contract_id.as_str()) {
                continue;
            }
            if let Some(observation) =
                translate_retained_observation(observation, &d1_subject, &case.id, &universe)
            {
                observations.push(observation);
            }
        }
        if observations.is_empty() {
            continue;
        }
        let used_contract_ids = observations
            .iter()
            .map(|observation| observation.contract_id.as_str())
            .collect::<BTreeSet<_>>();
        let mut contracts = BTreeMap::<String, GeoRhoContract>::new();
        for contract in &retained.contracts {
            if used_contract_ids.contains(contract.id.as_str()) {
                contracts.insert(contract.id.clone(), contract.clone());
            }
        }
        observations.sort_by(|left, right| left.id.cmp(&right.id));
        overlays.push(canon::geo::GeoPopulationCaseEvidenceOverlay {
            case_id: case.id.clone(),
            expected_base_evidence_blake3: None,
            contracts: contracts.into_values().collect(),
            observations,
        });
    }
    GeoPopulationEvidenceStackRequest {
        version: CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION.to_string(),
        case_overlays: overlays,
        max_overlay_cases: population.cases.len(),
        max_overlay_observations: 2_000,
    }
}

fn translate_retained_observation(
    observation: &GeoRhoObservation,
    d1_subject: &str,
    e4_case_id: &str,
    universe: &BTreeSet<String>,
) -> Option<GeoRhoObservation> {
    let translated_kind = match &observation.observation {
        GeoRhoObservationKind::ExistentialMembership { members } => {
            if !members.iter().all(|member| {
                member.level == GeoEntityLevel::Parcel && universe.contains(&member.id)
            }) {
                return None;
            }
            GeoRhoObservationKind::ExistentialMembership {
                members: members.clone(),
            }
        }
        GeoRhoObservationKind::PreferMember {
            member,
            cost_if_absent,
        } => {
            if member.level != GeoEntityLevel::Parcel || !universe.contains(&member.id) {
                return None;
            }
            GeoRhoObservationKind::PreferMember {
                member: member.clone(),
                cost_if_absent: *cost_if_absent,
            }
        }
        _ => return None,
    };
    Some(GeoRhoObservation {
        id: observation.id.replace(d1_subject, e4_case_id),
        contract_id: observation.contract_id.clone(),
        source_records: observation.source_records.clone(),
        valid_time: observation.valid_time,
        observation: translated_kind,
    })
}

struct FootprintRollOverlayInputs<'a> {
    population: &'a GeoPopulationEvaluationRequest,
    binding_by_case: &'a BTreeMap<&'a str, &'a E4Binding>,
    subject_by_case: &'a BTreeMap<String, String>,
    roll_gsf_by_subject: &'a BTreeMap<String, u64>,
    footprint_count_by_subject: &'a BTreeMap<String, u64>,
    retained_roll_overlay: &'a GeoPopulationEvidenceStackRequest,
    retained_footprint_overlay: &'a GeoPopulationEvidenceStackRequest,
    roll_rows_by_bbl: &'a BTreeMap<String, RollFixtureRow>,
    footprints_by_bbl: &'a BTreeMap<String, FootprintSummaryRow>,
}

fn footprint_roll_overlay_for_e4_cases(
    inputs: FootprintRollOverlayInputs<'_>,
) -> GeoPopulationEvidenceStackRequest {
    let FootprintRollOverlayInputs {
        population,
        binding_by_case,
        subject_by_case,
        roll_gsf_by_subject,
        footprint_count_by_subject,
        retained_roll_overlay,
        retained_footprint_overlay,
        roll_rows_by_bbl,
        footprints_by_bbl,
    } = inputs;

    let mut case_overlays = Vec::new();
    for case in &population.cases {
        let binding = binding_by_case.get(case.id.as_str()).expect("E4 binding");
        let d1_subject = d1_subject_id(&binding.case_id, subject_by_case);
        let loan_key = only(&binding.loan_keys, "loan_keys");
        let request = GeoFootprintRollEvidenceRequest {
            version: CANON_GEO_FOOTPRINT_ROLL_EVIDENCE_REQUEST_VERSION.to_string(),
            profile: GeoCompositionProfile::parcel(),
            case_id: case.id.clone(),
            universe: case.evidence.universe.clone(),
            loan: GeoFootprintRollLoanFields {
                loan_key: loan_key.clone(),
                filed_size: roll_gsf_by_subject.get(d1_subject.as_str()).copied(),
                size_measure: if roll_gsf_by_subject.contains_key(d1_subject.as_str()) {
                    "SQFT".to_string()
                } else {
                    "UNITS".to_string()
                },
                loan_county_property_count: footprint_count_by_subject
                    .get(d1_subject.as_str())
                    .copied(),
                size_source_record_id: retained_source_record_id(
                    retained_roll_overlay,
                    &d1_subject,
                    GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID,
                )
                .unwrap_or_else(|| {
                    format!("EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT:{loan_key}:SIZE")
                }),
                size_source_vintage: retained_source_record_vintage(
                    retained_roll_overlay,
                    &d1_subject,
                    GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID,
                )
                .unwrap_or_else(|| "latest_reporting_period".to_string()),
                county_property_count_source_record_id: retained_source_record_id(
                    retained_footprint_overlay,
                    &d1_subject,
                    GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_CONTRACT_ID,
                )
                .unwrap_or_else(|| {
                    format!("EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:{loan_key}:county_count")
                }),
                county_property_count_source_vintage: retained_source_record_vintage(
                    retained_footprint_overlay,
                    &d1_subject,
                    GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_CONTRACT_ID,
                )
                .unwrap_or_else(|| "current".to_string()),
            },
            source_config: GeoFootprintRollSourceConfig::default(),
            calibration: GeoFootprintRollCalibration::default(),
            assessment_roll_rows: case
                .evidence
                .universe
                .parcels
                .iter()
                .filter_map(|bbl| {
                    roll_rows_by_bbl
                        .get(bbl)
                        .map(|row| GeoAssessmentRollGrossSqftRow {
                            bbl: bbl.clone(),
                            gross_sqft: parse_u64(&row.gross_sqft),
                            units: parse_u64(&row.units),
                        })
                })
                .collect(),
            footprint_rows: footprint_stage_rows(&case.evidence.universe, footprints_by_bbl),
            max_assignments: case.evidence.max_assignments,
            max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
        };
        let evidence =
            materialize_footprint_roll_evidence(&request).expect("footprint/roll stage replays");
        if evidence.observations.is_empty() {
            continue;
        }
        case_overlays.push(canon::geo::GeoPopulationCaseEvidenceOverlay {
            case_id: case.id.clone(),
            expected_base_evidence_blake3: None,
            contracts: evidence.contracts,
            observations: evidence.observations,
        });
    }
    GeoPopulationEvidenceStackRequest {
        version: CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION.to_string(),
        case_overlays,
        max_overlay_cases: population.cases.len(),
        max_overlay_observations: 2_000,
    }
}

fn footprint_stage_rows(
    universe: &GeoCompositionUniverse,
    footprints_by_bbl: &BTreeMap<String, FootprintSummaryRow>,
) -> Vec<GeoBuildingFootprintRow> {
    let mut rows = Vec::new();
    for bbl in &universe.parcels {
        let Some(summary) = footprints_by_bbl.get(bbl) else {
            continue;
        };
        for index in 0..summary.bins {
            rows.push(GeoBuildingFootprintRow {
                mappluto_bbl: bbl.clone(),
                bin: format!("{bbl}:fixture-active-bin:{index:04}"),
                active: true,
            });
        }
    }
    rows
}

fn retained_source_record_id(
    overlay: &GeoPopulationEvidenceStackRequest,
    case_id: &str,
    contract_id: &str,
) -> Option<String> {
    retained_source_record(overlay, case_id, contract_id)
        .map(|record| record.source_record_id.clone())
}

fn retained_source_record_vintage(
    overlay: &GeoPopulationEvidenceStackRequest,
    case_id: &str,
    contract_id: &str,
) -> Option<String> {
    retained_source_record(overlay, case_id, contract_id)
        .map(|record| record.source_vintage.clone())
}

fn retained_source_record<'a>(
    overlay: &'a GeoPopulationEvidenceStackRequest,
    case_id: &str,
    contract_id: &str,
) -> Option<&'a canon::geo::GeoEvidenceRecordRef> {
    overlay
        .case_overlays
        .iter()
        .find(|case| case.case_id == case_id)?
        .observations
        .iter()
        .find(|observation| observation.contract_id == contract_id)?
        .source_records
        .first()
}

fn merge_overlay_requests(
    max_cases: usize,
    overlays: Vec<GeoPopulationEvidenceStackRequest>,
) -> GeoPopulationEvidenceStackRequest {
    let mut by_case = BTreeMap::<
        String,
        (
            BTreeMap<String, GeoRhoContract>,
            BTreeMap<String, GeoRhoObservation>,
        ),
    >::new();
    for overlay in overlays {
        for case in overlay.case_overlays {
            let entry = by_case.entry(case.case_id).or_default();
            for contract in case.contracts {
                entry.0.entry(contract.id.clone()).or_insert(contract);
            }
            for observation in case.observations {
                entry.1.entry(observation.id.clone()).or_insert(observation);
            }
        }
    }
    let case_overlays = by_case
        .into_iter()
        .filter_map(|(case_id, (contracts, observations))| {
            (!observations.is_empty()).then_some(canon::geo::GeoPopulationCaseEvidenceOverlay {
                case_id,
                expected_base_evidence_blake3: None,
                contracts: contracts.into_values().collect(),
                observations: observations.into_values().collect(),
            })
        })
        .collect::<Vec<_>>();
    let max_overlay_observations = case_overlays
        .iter()
        .map(|case| case.observations.len())
        .sum::<usize>()
        .max(1);
    GeoPopulationEvidenceStackRequest {
        version: CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION.to_string(),
        case_overlays,
        max_overlay_cases: max_cases,
        max_overlay_observations,
    }
}

fn measurement_summary(
    pad_only_eval: &GeoPopulationEvaluationArtifact,
    stacked_eval: &GeoPopulationEvaluationArtifact,
    owner_summary: &canon::geo::assessment_roll::GeoAssessmentRollOwnerSummary,
    condo_stats: &canon::geo::GeoCondoBridgeStats,
    stacked_overlay: &GeoPopulationEvidenceStackRequest,
    cases: &[MeasurementCase],
) -> MeasurementSummary {
    assert_eq!(
        canonical_population_evaluation_bytes(pad_only_eval)
            .expect("PAD-only evaluation serializes"),
        canonical_population_evaluation_bytes(pad_only_eval)
            .expect("PAD-only evaluation serializes again")
    );
    let source_fixture_paths = vec![
        "tests/fixtures/geo/e4_gate_v2_population_request.json".to_string(),
        "scripts/geo_measurements/fixtures/d1_residuals/d1_population_evidence_stack.json"
            .to_string(),
        format!("{MCP_STACK_DIR}/e4_case_bindings.json"),
        format!("{MCP_STACK_DIR}/assessment_roll_fy2026p3_lots.json.gz"),
        format!("{MCP_STACK_DIR}/footprints.json"),
        format!("{MCP_STACK_DIR}/pad_bbl.json.gz"),
        format!("{MCP_STACK_DIR}/calibration_roll_owner_exact.json"),
        format!("{MCP_STACK_DIR}/calibration_roll_gsf_band.json"),
        format!("{MCP_STACK_DIR}/calibration_footprint.json"),
        format!("{MCP_STACK_DIR}/overlay_request_roll_exact_owner_gsf_band.json.gz"),
        format!("{MCP_STACK_DIR}/overlay_request_soft_owner_footprint.json.gz"),
    ];
    let source_fixture_digests = source_fixture_paths
        .into_iter()
        .map(|path| SourceFixtureDigest {
            blake3: blake3_file_hex(rooted(&path)),
            path,
        })
        .collect::<Vec<_>>();

    let stage_summaries = BTreeMap::from([
        (
            "assessment_roll_owner".to_string(),
            serde_json::to_value(owner_summary).expect("owner summary serializes"),
        ),
        (
            "condo_bridge".to_string(),
            serde_json::to_value(condo_stats).expect("condo stats serializes"),
        ),
        (
            "stacked_overlay_observations".to_string(),
            serde_json::json!(stage_observation_totals(cases)),
        ),
        (
            "stacked_overlay_cases".to_string(),
            serde_json::json!(stacked_overlay.case_overlays.len()),
        ),
    ]);

    MeasurementSummary {
        version: MEASUREMENT_VERSION.to_string(),
        fixture_class: FIXTURE_CLASS.to_string(),
        proof_class: "fixture replay of retained warehouse snapshot; not live proof; not a gate pass"
            .to_string(),
        source_fixture_digests,
        stage_summaries,
        stage_notes: vec![
            "The stage replay starts from tests/fixtures/geo/e4_gate_v2_population_request.json."
                .to_string(),
            "Assessment-roll owner stage widens only the frozen candidate-universe blocks."
                .to_string(),
            "Retained PAD membership and geocode preferences are translated from the first matching D1 subject receipt by exact deed truth-set binding; hard PAD membership is all-or-nothing, not subset-trimmed."
                .to_string(),
            "footprints.json is a summarized active-BIN count by BBL; the replay expands it into deterministic fixture BIN rows for the footprint_roll stage."
                .to_string(),
            "Loan SIZE and LOAN_COUNTY_PROPERTY_COUNT are replayed from retained calibration/overlay receipts; no live warehouse query is made."
                .to_string(),
            format!(
                "The replay caps case max_assignments at {MEASUREMENT_MAX_ASSIGNMENTS}; budget fallbacks are typed abstentions, never guessed residuals."
            ),
        ],
        g1_numbers: BTreeMap::from([
            ("pad_only_baseline".to_string(), g1_numbers(pad_only_eval)),
            ("stacked".to_string(), g1_numbers(stacked_eval)),
        ]),
    }
}

fn cap_measurement_budgets(population: &mut GeoPopulationEvaluationRequest) {
    for case in &mut population.cases {
        case.evidence.max_assignments = case
            .evidence
            .max_assignments
            .min(MEASUREMENT_MAX_ASSIGNMENTS);
    }
}

fn measurement_cases(
    bindings: &E4Bindings,
    subject_by_case: &BTreeMap<String, String>,
    pad_only_eval: &GeoPopulationEvaluationArtifact,
    stacked_eval: &GeoPopulationEvaluationArtifact,
    stacked_artifacts: &[canon::geo::GeoPopulationCaseArtifacts],
    stacked_overlay: &GeoPopulationEvidenceStackRequest,
    condo_rows: &[canon::geo::GeoCondoBridgeCase],
) -> Vec<MeasurementCase> {
    let pad_by_case = pad_only_eval
        .cases
        .iter()
        .map(|case| (case.case_id.as_str(), case))
        .collect::<BTreeMap<_, _>>();
    let stacked_by_case = stacked_eval
        .cases
        .iter()
        .map(|case| (case.case_id.as_str(), case))
        .collect::<BTreeMap<_, _>>();
    let condo_by_case = condo_rows
        .iter()
        .map(|row| (row.case_id.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    let mut cases = bindings
        .cases
        .iter()
        .map(|binding| {
            let loan_key = only(&binding.loan_keys, "loan_keys").clone();
            let d1_subject_id = d1_subject_id(&binding.case_id, subject_by_case);
            let stacked_case = stacked_by_case
                .get(binding.case_id.as_str())
                .expect("stacked case exists");
            MeasurementCase {
                case_id: binding.case_id.clone(),
                d1_subject_id,
                loan_key,
                document_id: only(&binding.document_ids, "document_ids").clone(),
                truth_parcels: binding.truth_parcels.clone(),
                stage_observations: stage_counts(&binding.case_id, stacked_overlay),
                condo_bridge: condo_by_case.get(binding.case_id.as_str()).map(|row| {
                    CondoBridgeCaseSummary {
                        kind: format!("{:?}", row.kind),
                        unit_lots: row.unit_lots,
                        truth_billing_grain: row.truth_billing_grain.clone(),
                        universe_billing_grain_count: row.universe_billing_grain.len() as u64,
                        billing_truth_in_universe: format!(
                            "{}/{}",
                            row.after.truth_members_in_universe, row.after.truth_members
                        ),
                    }
                }),
                baseline: case_outcome(
                    pad_by_case
                        .get(binding.case_id.as_str())
                        .expect("baseline case exists"),
                ),
                stacked: case_outcome(stacked_case),
                explanation: conflict_explanation(
                    &binding.case_id,
                    stacked_case,
                    stacked_artifacts,
                ),
            }
        })
        .collect::<Vec<_>>();
    cases.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    cases
}

fn g1_numbers(evaluation: &GeoPopulationEvaluationArtifact) -> G1Numbers {
    G1Numbers {
        cases: evaluation.summary.cases,
        evidence_no_observation_cases: evaluation.summary.evidence_no_observation_cases,
        reachable_cases: evaluation.summary.candidate_reach_full_cases,
        resolved_cases: evaluation.summary.resolved_cases,
        ambiguous_cases: evaluation.summary.ambiguous_cases,
        conflict_cases: evaluation.summary.conflict_cases,
        component_budget_fallback_cases: evaluation.summary.component_budget_fallback_cases,
        deed_exact_cases: evaluation
            .cases
            .iter()
            .filter(|case| {
                case.status == GeoPopulationCaseStatus::Resolved
                    && case.truth_model_in_residual == Some(true)
            })
            .count() as u64,
        false_merge_cases: evaluation.summary.false_merge_cases,
        truth_exclusion_cases: evaluation.summary.solver_truth_exclusion_cases,
        residual_count_le16_cases: evaluation
            .cases
            .iter()
            .filter(|case| case.residual_model_count.is_some_and(|count| count <= 16))
            .count() as u64,
    }
}

fn case_outcome(case: &GeoPopulationCaseEvaluation) -> CaseOutcome {
    CaseOutcome {
        status: status_name(case.status).to_string(),
        residual_count: case.residual_model_count,
        residual_count_saturated: case.residual_count_saturated,
        truth_in_universe: format!("{}/{}", case.truth_members_in_universe, case.truth_members),
        truth_model_in_residual: case.truth_model_in_residual,
        forced: case.hard_forced.clone(),
        deed_exact: case.status == GeoPopulationCaseStatus::Resolved
            && case.truth_model_in_residual == Some(true),
    }
}

fn conflict_explanation(
    case_id: &str,
    stacked_case: &GeoPopulationCaseEvaluation,
    stacked_artifacts: &[canon::geo::GeoPopulationCaseArtifacts],
) -> Option<ConflictExplanationSummary> {
    if stacked_case.status != GeoPopulationCaseStatus::Conflict {
        return None;
    }
    let artifact = stacked_artifacts
        .iter()
        .find(|artifact| artifact.case_id == case_id)
        .expect("conflict case artifact exists");
    let order = canon::geo::reliability_order_from_evidence(&artifact.evidence);
    let explanation = canon::geo::minimal_core(
        &artifact.evidence.composition_request,
        &artifact.evidence,
        &order,
        &canon::geo::GeoExplanationBudget::default(),
    )
    .expect("conflict core must be explainable");
    let core = explanation.cores.first().expect("one core is emitted");
    Some(ConflictExplanationSummary {
        core_observation_ids: core.observation_ids.clone(),
        core_constraint_ids: core.constraint_ids.clone(),
        core_source_record_ids: core.source_record_ids.clone(),
        complete: explanation.explanation_complete,
    })
}

fn stage_counts(
    case_id: &str,
    overlay: &GeoPopulationEvidenceStackRequest,
) -> StageObservationCounts {
    let mut counts = StageObservationCounts {
        retained_pad: 0,
        retained_geocode_preference: 0,
        assessment_roll_owner_exact: 0,
        assessment_roll_affiliate_preference: 0,
        roll_gsf_band: 0,
        footprint_floor: 0,
    };
    let Some(case) = overlay
        .case_overlays
        .iter()
        .find(|overlay| overlay.case_id == case_id)
    else {
        return counts;
    };
    for observation in &case.observations {
        match observation.contract_id.as_str() {
            "rho.address.pad.membership" => counts.retained_pad += 1,
            "rho.address.geocode.parcel_containment" => counts.retained_geocode_preference += 1,
            "rho.owner.assessment_roll_exact_match" => counts.assessment_roll_owner_exact += 1,
            "rho.owner.assessment_roll_affiliate_preference" => {
                counts.assessment_roll_affiliate_preference += 1
            }
            "rho.size.assessment_roll_gross_sqft_band" => counts.roll_gsf_band += 1,
            "rho.footprint.building_count_floor" => counts.footprint_floor += 1,
            other => panic!("unexpected observation contract {other}"),
        }
    }
    counts
}

fn stage_observation_totals(cases: &[MeasurementCase]) -> StageObservationCounts {
    cases.iter().fold(
        StageObservationCounts {
            retained_pad: 0,
            retained_geocode_preference: 0,
            assessment_roll_owner_exact: 0,
            assessment_roll_affiliate_preference: 0,
            roll_gsf_band: 0,
            footprint_floor: 0,
        },
        |mut total, case| {
            total.retained_pad += case.stage_observations.retained_pad;
            total.retained_geocode_preference +=
                case.stage_observations.retained_geocode_preference;
            total.assessment_roll_owner_exact +=
                case.stage_observations.assessment_roll_owner_exact;
            total.assessment_roll_affiliate_preference +=
                case.stage_observations.assessment_roll_affiliate_preference;
            total.roll_gsf_band += case.stage_observations.roll_gsf_band;
            total.footprint_floor += case.stage_observations.footprint_floor;
            total
        },
    )
}

fn only<'a>(values: &'a [String], field: &str) -> &'a String {
    assert_eq!(values.len(), 1, "{field} must contain exactly one value");
    &values[0]
}

fn d1_subject_id(case_id: &str, subject_by_case: &BTreeMap<String, String>) -> String {
    subject_by_case
        .get(case_id)
        .unwrap_or_else(|| panic!("E4 case {case_id} must bind to a D1 subject"))
        .clone()
}

fn truth_key(values: &[String]) -> Vec<String> {
    let mut key = values.to_vec();
    key.sort();
    key
}

fn block_key(bbl: &str) -> &str {
    assert!(
        bbl.len() == 10 && bbl.bytes().all(|byte| byte.is_ascii_digit()),
        "BBL must be 10 ASCII digits: {bbl}"
    );
    &bbl[..6]
}

fn parse_u64(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        trimmed.parse::<u64>().ok()
    }
}

fn status_name(status: GeoPopulationCaseStatus) -> &'static str {
    match status {
        GeoPopulationCaseStatus::Resolved => "resolved",
        GeoPopulationCaseStatus::Ambiguous => "ambiguous",
        GeoPopulationCaseStatus::Conflict => "conflict",
        GeoPopulationCaseStatus::AssignmentBudgetExceeded => "assignment_budget_exceeded",
        GeoPopulationCaseStatus::ComponentBudgetFallback => "component_budget_fallback",
    }
}

fn assert_committed_json<T: Serialize>(relative: &str, value: &T) {
    let expected_path = rooted(&format!("{OUT_DIR}/{relative}"));
    let expected = fs::read(&expected_path)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", expected_path.display()));
    let actual = pretty_json_bytes(value);
    assert_eq!(
        expected,
        actual,
        "{} is stale; run scripts/geo_demo/e2e_e4_gate.sh --refresh",
        expected_path.display()
    );
}

fn write_pretty_json<T: Serialize>(path: PathBuf, value: &T) {
    fs::write(&path, pretty_json_bytes(value))
        .unwrap_or_else(|error| panic!("{} must be writable: {error}", path.display()));
}

fn pretty_json_bytes<T: Serialize>(value: &T) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(value).expect("JSON serializes");
    bytes.push(b'\n');
    bytes
}

fn rooted(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: PathBuf) -> T {
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("{} must parse: {error}", path.display()))
}

fn read_json_gz<T: for<'de> Deserialize<'de>>(path: PathBuf) -> T {
    let output = Command::new("gzip")
        .args(["-dc"])
        .arg(&path)
        .output()
        .unwrap_or_else(|error| panic!("gzip must run for {}: {error}", path.display()));
    assert!(
        output.status.success(),
        "gzip failed for {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{} must parse: {error}", path.display()))
}

fn calibration_receipt_digest(path: &str) -> String {
    let receipt: Value = read_json(rooted(path));
    canon::geo::calibration_receipt_blake3(&receipt)
        .unwrap_or_else(|error| panic!("{path} must canonicalize: {error}"))
}

fn blake3_file_hex(path: PathBuf) -> String {
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));
    blake3::hash(&bytes).to_hex().to_string()
}
