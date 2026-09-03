#![forbid(unsafe_code)]

use canon::geo::assessment_roll::{
    CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION,
    GEO_ASSESSMENT_ROLL_OWNER_AFFILIATE_CONTRACT_ID, GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID,
    GeoAssessmentRollCaseDocument, GeoAssessmentRollLotRow, GeoAssessmentRollOwnerCalibration,
    GeoAssessmentRollOwnerContractSource, GeoAssessmentRollOwnerProofClass,
    GeoAssessmentRollOwnerRequest, GeoAssessmentRollPartyRow, assessment_roll_owner_match,
    normalize_assessment_roll_owner_name, produce_assessment_roll_owner_evidence,
};
use canon::geo::{
    GeoEvidenceCompilationRequest, GeoEvidenceRecordRef, GeoPopulationCaseEvidenceOverlay,
    GeoPopulationCaseStatus, GeoPopulationEvaluationRequest, GeoPopulationEvidenceStackRequest,
    GeoRhoBasis, GeoRhoContract, GeoRhoObservationKind, evaluate_population,
    stack_population_evidence,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    process::Command,
};

const FIXTURE_DIR: &str = "scripts/geo_measurements/fixtures/d1_residuals";
const MCP_STACK_DIR: &str = "scripts/geo_measurements/fixtures/d1_residuals/mcp_stack_2026-09-03";
const ROLL_SOURCE_DATASET: &str =
    "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION_FY2026P3_x_ACRIS_PARTIES";
const ROLL_SOURCE_RELEASE: &str = "FY2026P3_acris-latest";

#[derive(Debug, Default, PartialEq, Eq)]
struct OwnerOverlaySignature {
    hard_exact_lots: BTreeMap<String, Vec<String>>,
    soft_affiliate_lots: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct RollFixtureRow {
    #[serde(default)]
    owner: String,
    #[serde(default)]
    units: String,
    #[serde(default)]
    gross_sqft: String,
    #[serde(default)]
    condo: String,
}

#[test]
fn assessment_roll_stage_replays_retained_d1_roll_owner_counts() {
    let base_population: GeoPopulationEvaluationRequest =
        read_json(rooted(&[FIXTURE_DIR, "h7_population_request.json"]));
    let expected_widened_population: GeoPopulationEvaluationRequest = read_json_gz(rooted(&[
        MCP_STACK_DIR,
        "population_request_roll_universe.json.gz",
    ]));
    let retained_overlay: GeoPopulationEvidenceStackRequest = read_json_gz(rooted(&[
        MCP_STACK_DIR,
        "overlay_request_roll_exact_owner_gsf_band.json.gz",
    ]));
    let retained_evaluation: Value = read_json(rooted(&[
        MCP_STACK_DIR,
        "evaluation_roll_exact_owner_gsf_band.json",
    ]));

    let request = assessment_roll_owner_fixture_request(&base_population, &retained_overlay);
    let artifact =
        produce_assessment_roll_owner_evidence(&request).expect("owner stage produces artifact");

    assert_eq!(
        artifact.proof_class,
        GeoAssessmentRollOwnerProofClass::Fixture
    );
    assert_eq!(artifact.summary.cases, 70);
    assert_eq!(
        artifact.summary.owner_overlay_cases,
        owner_overlay_case_count(&retained_overlay)
    );
    assert_eq!(artifact.summary.exact_hard_observations, 40);
    assert_eq!(artifact.summary.affiliate_soft_observations, 265);
    assert_eq!(
        universe_by_case(&artifact.widened_population),
        universe_by_case(&expected_widened_population),
        "stage must reproduce assessment-roll block widening"
    );
    assert_eq!(
        owner_signature(&artifact.overlay),
        owner_signature(&retained_overlay),
        "stage must reproduce the retained exact-owner hard channel and affiliate soft channel"
    );
    assert_owner_observations_name_roll_and_party_records(&artifact.overlay);

    let combined_overlay = retained_overlay_with_stage_owner(&retained_overlay, &artifact.overlay);
    let stacked = stack_population_evidence(&artifact.widened_population, &combined_overlay)
        .expect("stage owner overlay stacks with retained non-owner channels");
    let evaluation =
        evaluate_population(&stacked.population).expect("stacked population evaluates");

    assert_eq!(evaluation.summary.cases, 70);
    assert_eq!(
        evaluation.summary.resolved_cases,
        retained_evaluation["summary"]["resolved_cases"]
            .as_u64()
            .expect("retained resolved count")
    );
    assert_eq!(evaluation.summary.resolved_cases, 16);
    assert_eq!(resolved_correct_cases(&evaluation.cases), 6);
    assert_eq!(evaluation.summary.ambiguous_cases, 44);
    assert_eq!(evaluation.summary.conflict_cases, 4);
    assert_eq!(evaluation.summary.solver_truth_exclusion_cases, 15);
    assert_eq!(
        evaluation.summary.solver_truth_exclusion_cases,
        retained_evaluation["summary"]["solver_truth_exclusion_cases"]
            .as_u64()
            .expect("retained truth exclusion count")
    );
}

#[test]
fn affiliate_only_match_never_emits_the_hard_exact_band() {
    let population = GeoPopulationEvaluationRequest {
        version: canon::geo::CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        max_cases: 1,
        cases: vec![canon::geo::GeoLabeledCompositionCase {
            id: "case-affiliate-only".to_string(),
            evidence: GeoEvidenceCompilationRequest {
                version: canon::geo::CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
                profile: canon::geo::GeoCompositionProfile::parcel(),
                universe: canon::geo::GeoCompositionUniverse {
                    parcels: vec!["1000000001".to_string(), "1000000002".to_string()],
                    buildings: Vec::new(),
                },
                contracts: Vec::new(),
                observations: Vec::new(),
                max_assignments: 64,
                max_materialized_models: 64,
            },
            truth_plane: canon::geo::GeoTruthPlane::HumanAdjudication,
            truth: canon::geo::GeoCompositionModel {
                parcels: vec!["1000000001".to_string()],
                buildings: Vec::new(),
            },
        }],
    };
    let request = GeoAssessmentRollOwnerRequest {
        version: CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION.to_string(),
        proof_class: GeoAssessmentRollOwnerProofClass::Fixture,
        population,
        case_documents: vec![GeoAssessmentRollCaseDocument {
            case_id: "case-affiliate-only".to_string(),
            document_id: "doc-affiliate".to_string(),
        }],
        contract_source: contract_source(),
        calibration: calibration_from_fixture_contracts(
            &fixture_contract(GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID),
            &fixture_contract(GEO_ASSESSMENT_ROLL_OWNER_AFFILIATE_CONTRACT_ID),
        ),
        roll_rows: vec![
            GeoAssessmentRollLotRow {
                bbl: "1000000001".to_string(),
                owner: "ALPHA REALTY LLC".to_string(),
                gross_sqft: "1000".to_string(),
                units: "1".to_string(),
                condo_number: String::new(),
                source_record_id:
                    "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:1000000001"
                        .to_string(),
                source_vintage: "FY2026P3".to_string(),
            },
            GeoAssessmentRollLotRow {
                bbl: "1000000002".to_string(),
                owner: "UNRELATED OWNER LLC".to_string(),
                gross_sqft: "1000".to_string(),
                units: "1".to_string(),
                condo_number: String::new(),
                source_record_id:
                    "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:1000000002"
                        .to_string(),
                source_vintage: "FY2026P3".to_string(),
            },
        ],
        party_rows: vec![GeoAssessmentRollPartyRow {
            document_id: "doc-affiliate".to_string(),
            party_type: "1".to_string(),
            party_name_norm: "ALPHA HOLDINGS".to_string(),
            source_record_id:
                "EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:doc-affiliate:ALPHA_HOLDINGS"
                    .to_string(),
            source_vintage: "latest".to_string(),
        }],
        max_cases: 1,
        max_roll_rows: 2,
        max_party_rows: 1,
        max_overlay_observations: 4,
    };

    let artifact = produce_assessment_roll_owner_evidence(&request)
        .expect("affiliate-only request produces soft overlay");
    assert_eq!(artifact.summary.exact_hard_observations, 0);
    assert_eq!(artifact.summary.affiliate_soft_observations, 1);
    assert_eq!(artifact.overlay.case_overlays.len(), 1);
    let overlay = &artifact.overlay.case_overlays[0];
    assert!(
        overlay
            .contracts
            .iter()
            .all(|contract| contract.id != GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID),
        "token-only owner matches must not register the hard exact-match contract"
    );
    assert_eq!(
        owner_signature(&artifact.overlay)
            .soft_affiliate_lots
            .get("case-affiliate-only")
            .cloned()
            .unwrap_or_default(),
        vec!["1000000001".to_string()]
    );

    let stacked = stack_population_evidence(&artifact.widened_population, &artifact.overlay)
        .expect("affiliate-only overlay stacks");
    let compilation = canon::geo::compile_evidence(&stacked.population.cases[0].evidence)
        .expect("affiliate-only stacked evidence compiles");
    assert!(compilation.composition_request.hard_constraints.is_empty());
    assert_eq!(compilation.composition_request.soft_preferences.len(), 1);

    let borrowers = BTreeSet::from(["ALPHA HOLDINGS".to_string()]);
    assert_eq!(
        assessment_roll_owner_match("Alpha Realty LLC", &borrowers),
        canon::geo::assessment_roll::GeoAssessmentRollOwnerMatch::Token
    );
    assert_eq!(
        normalize_assessment_roll_owner_name("Alpha Realty LLC"),
        "ALPHA REALTY LLC"
    );
}

fn assessment_roll_owner_fixture_request(
    population: &GeoPopulationEvaluationRequest,
    retained_overlay: &GeoPopulationEvidenceStackRequest,
) -> GeoAssessmentRollOwnerRequest {
    let exact_contract = owner_contract(
        retained_overlay,
        GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID,
    );
    let affiliate_contract = owner_contract(
        retained_overlay,
        GEO_ASSESSMENT_ROLL_OWNER_AFFILIATE_CONTRACT_ID,
    );
    let (case_documents, party_rows) =
        party_rows_from_retained_overlay(population, retained_overlay);
    let roll_rows = assessment_roll_rows();
    GeoAssessmentRollOwnerRequest {
        version: CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION.to_string(),
        proof_class: GeoAssessmentRollOwnerProofClass::Fixture,
        population: population.clone(),
        case_documents,
        contract_source: GeoAssessmentRollOwnerContractSource {
            source_dataset: exact_contract.source_dataset.clone(),
            source_release: exact_contract.source_release.clone(),
            source_lineage_ids: exact_contract.source_lineage_ids.clone(),
        },
        calibration: calibration_from_fixture_contracts(&exact_contract, &affiliate_contract),
        max_cases: population.cases.len(),
        max_roll_rows: roll_rows.len(),
        max_party_rows: party_rows.len(),
        max_overlay_observations: 1_000,
        roll_rows,
        party_rows,
    }
}

fn assessment_roll_rows() -> Vec<GeoAssessmentRollLotRow> {
    let rows: BTreeMap<String, RollFixtureRow> = read_json_gz(rooted(&[
        MCP_STACK_DIR,
        "assessment_roll_fy2026p3_lots.json.gz",
    ]));
    rows.into_iter()
        .map(|(bbl, row)| GeoAssessmentRollLotRow {
            source_record_id: format!(
                "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:{bbl}"
            ),
            source_vintage: "FY2026P3".to_string(),
            bbl,
            owner: row.owner,
            gross_sqft: row.gross_sqft,
            units: row.units,
            condo_number: row.condo,
        })
        .collect()
}

fn party_rows_from_retained_overlay(
    population: &GeoPopulationEvaluationRequest,
    retained_overlay: &GeoPopulationEvidenceStackRequest,
) -> (
    Vec<GeoAssessmentRollCaseDocument>,
    Vec<GeoAssessmentRollPartyRow>,
) {
    let mut document_by_case = BTreeMap::<String, String>::new();
    let mut party_rows = BTreeMap::<String, GeoAssessmentRollPartyRow>::new();
    for overlay in &retained_overlay.case_overlays {
        for observation in &overlay.observations {
            if !is_owner_contract(&observation.contract_id) {
                continue;
            }
            for record in &observation.source_records {
                if let Some((document_id, party_name_norm)) = parse_acris_party_record(record) {
                    match document_by_case.insert(overlay.case_id.clone(), document_id.clone()) {
                        Some(previous) if previous != document_id => {
                            panic!("case {} has multiple retained documents", overlay.case_id)
                        }
                        _ => {}
                    }
                    party_rows
                        .entry(record.source_record_id.clone())
                        .or_insert_with(|| GeoAssessmentRollPartyRow {
                            document_id,
                            party_type: "1".to_string(),
                            party_name_norm,
                            source_record_id: record.source_record_id.clone(),
                            source_vintage: record.source_vintage.clone(),
                        });
                }
            }
        }
    }

    let case_documents = population
        .cases
        .iter()
        .map(|case| GeoAssessmentRollCaseDocument {
            case_id: case.id.clone(),
            document_id: document_by_case
                .get(&case.id)
                .cloned()
                .unwrap_or_else(|| format!("fixture:no-owner-party:{}", case.id)),
        })
        .collect::<Vec<_>>();
    (case_documents, party_rows.into_values().collect())
}

fn parse_acris_party_record(record: &GeoEvidenceRecordRef) -> Option<(String, String)> {
    let prefix = "EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:";
    let tail = record.source_record_id.strip_prefix(prefix)?;
    let (document_id, norm) = tail.split_once(':')?;
    Some((document_id.to_string(), norm.replace('_', " ")))
}

fn calibration_from_fixture_contracts(
    exact_contract: &GeoRhoContract,
    affiliate_contract: &GeoRhoContract,
) -> GeoAssessmentRollOwnerCalibration {
    let GeoRhoBasis::EmpiricalCalibration {
        population_id,
        calibration_blake3,
        falsification_rule_id: exact_falsification_rule_id,
        ..
    } = &exact_contract.basis
    else {
        panic!("exact fixture owner contract must be empirical");
    };
    let GeoRhoBasis::EmpiricalCalibration {
        falsification_rule_id: affiliate_falsification_rule_id,
        ..
    } = &affiliate_contract.basis
    else {
        panic!("affiliate fixture owner contract must be empirical");
    };
    GeoAssessmentRollOwnerCalibration {
        population_id: population_id.clone(),
        calibration_blake3: calibration_blake3.clone(),
        exact_falsification_rule_id: exact_falsification_rule_id.clone(),
        affiliate_falsification_rule_id: affiliate_falsification_rule_id.clone(),
    }
}

fn fixture_contract(contract_id: &str) -> GeoRhoContract {
    let retained_overlay: GeoPopulationEvidenceStackRequest = read_json_gz(rooted(&[
        MCP_STACK_DIR,
        "overlay_request_roll_exact_owner_gsf_band.json.gz",
    ]));
    owner_contract(&retained_overlay, contract_id)
}

fn owner_contract(
    overlay: &GeoPopulationEvidenceStackRequest,
    contract_id: &str,
) -> GeoRhoContract {
    overlay
        .case_overlays
        .iter()
        .flat_map(|case| &case.contracts)
        .find(|contract| contract.id == contract_id)
        .unwrap_or_else(|| panic!("retained overlay must include {contract_id}"))
        .clone()
}

fn contract_source() -> GeoAssessmentRollOwnerContractSource {
    GeoAssessmentRollOwnerContractSource {
        source_dataset: ROLL_SOURCE_DATASET.to_string(),
        source_release: ROLL_SOURCE_RELEASE.to_string(),
        source_lineage_ids: vec![
            "EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:latest".to_string(),
            "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.WRGL_NYC_OPENDATA_PROPERTY_VALUATION_AND_ASSESSMENT_DATA_TAX_CLASSES_1_2_3_4__STRUCTURED:FY2026P3"
                .to_string(),
        ],
    }
}

fn retained_overlay_with_stage_owner(
    retained: &GeoPopulationEvidenceStackRequest,
    owner: &GeoPopulationEvidenceStackRequest,
) -> GeoPopulationEvidenceStackRequest {
    let mut by_case = BTreeMap::<String, GeoPopulationCaseEvidenceOverlay>::new();
    for overlay in &retained.case_overlays {
        let filtered = GeoPopulationCaseEvidenceOverlay {
            case_id: overlay.case_id.clone(),
            expected_base_evidence_blake3: None,
            contracts: overlay
                .contracts
                .iter()
                .filter(|contract| !is_owner_contract(&contract.id))
                .cloned()
                .collect(),
            observations: overlay
                .observations
                .iter()
                .filter(|observation| !is_owner_contract(&observation.contract_id))
                .cloned()
                .collect(),
        };
        if !filtered.observations.is_empty() {
            by_case.insert(filtered.case_id.clone(), filtered);
        }
    }
    for overlay in &owner.case_overlays {
        let entry = by_case.entry(overlay.case_id.clone()).or_insert_with(|| {
            GeoPopulationCaseEvidenceOverlay {
                case_id: overlay.case_id.clone(),
                expected_base_evidence_blake3: None,
                contracts: Vec::new(),
                observations: Vec::new(),
            }
        });
        entry.contracts.extend(overlay.contracts.clone());
        entry.observations.extend(overlay.observations.clone());
        entry
            .contracts
            .sort_by(|left, right| left.id.cmp(&right.id));
        entry.contracts.dedup_by(|left, right| left.id == right.id);
        entry
            .observations
            .sort_by(|left, right| left.id.cmp(&right.id));
    }
    let case_overlays = by_case.into_values().collect::<Vec<_>>();
    let max_overlay_observations = case_overlays
        .iter()
        .map(|case| case.observations.len())
        .sum::<usize>();
    GeoPopulationEvidenceStackRequest {
        version: canon::geo::CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION.to_string(),
        max_overlay_cases: case_overlays.len(),
        max_overlay_observations,
        case_overlays,
    }
}

fn owner_signature(overlay: &GeoPopulationEvidenceStackRequest) -> OwnerOverlaySignature {
    let mut signature = OwnerOverlaySignature::default();
    for case_overlay in &overlay.case_overlays {
        for observation in &case_overlay.observations {
            match &observation.observation {
                GeoRhoObservationKind::IntegerSumBand { values, .. }
                    if observation.contract_id == GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID =>
                {
                    signature.hard_exact_lots.insert(
                        case_overlay.case_id.clone(),
                        values
                            .iter()
                            .filter(|value| value.value == 0)
                            .map(|value| value.id.clone())
                            .collect(),
                    );
                }
                GeoRhoObservationKind::PreferMember { member, .. }
                    if observation.contract_id
                        == GEO_ASSESSMENT_ROLL_OWNER_AFFILIATE_CONTRACT_ID =>
                {
                    signature
                        .soft_affiliate_lots
                        .entry(case_overlay.case_id.clone())
                        .or_default()
                        .push(member.id.clone());
                }
                _ => {}
            }
        }
    }
    for values in signature.hard_exact_lots.values_mut() {
        values.sort();
    }
    for values in signature.soft_affiliate_lots.values_mut() {
        values.sort();
    }
    signature
}

fn owner_overlay_case_count(overlay: &GeoPopulationEvidenceStackRequest) -> u64 {
    overlay
        .case_overlays
        .iter()
        .filter(|case_overlay| {
            case_overlay
                .observations
                .iter()
                .any(|observation| is_owner_contract(&observation.contract_id))
        })
        .count() as u64
}

fn is_owner_contract(contract_id: &str) -> bool {
    contract_id == GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID
        || contract_id == GEO_ASSESSMENT_ROLL_OWNER_AFFILIATE_CONTRACT_ID
}

fn universe_by_case(population: &GeoPopulationEvaluationRequest) -> BTreeMap<String, Vec<String>> {
    population
        .cases
        .iter()
        .map(|case| {
            let mut parcels = case.evidence.universe.parcels.clone();
            parcels.sort();
            (case.id.clone(), parcels)
        })
        .collect()
}

fn assert_owner_observations_name_roll_and_party_records(
    overlay: &GeoPopulationEvidenceStackRequest,
) {
    for observation in overlay
        .case_overlays
        .iter()
        .flat_map(|case| &case.observations)
        .filter(|observation| is_owner_contract(&observation.contract_id))
    {
        assert!(
            observation.source_records.iter().any(|record| record
                .source_record_id
                .contains("STG_GEO_NYC_ACRIS_PARTIES")),
            "{} must cite at least one ACRIS party record",
            observation.id
        );
        assert!(
            observation
                .source_records
                .iter()
                .any(|record| record.source_record_id.contains("PROPERTY_VALUATION")),
            "{} must cite at least one assessment-roll record",
            observation.id
        );
        assert!(observation.source_records.iter().all(|record| {
            record.record_blake3.len() == 64
                && record
                    .record_blake3
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }));
    }
}

fn resolved_correct_cases(cases: &[canon::geo::GeoPopulationCaseEvaluation]) -> u64 {
    cases
        .iter()
        .filter(|case| {
            case.status == GeoPopulationCaseStatus::Resolved
                && case.truth_model_in_residual == Some(true)
        })
        .count() as u64
}

fn rooted(parts: &[&str]) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for part in parts {
        path.push(part);
    }
    path
}

fn read_json<T: for<'de> Deserialize<'de>>(path: PathBuf) -> T {
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!("{} must be readable: {error}", path.display());
    });
    serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!("{} must parse as JSON: {error}", path.display());
    })
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
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!("{} must parse as gzipped JSON: {error}", path.display());
    })
}
