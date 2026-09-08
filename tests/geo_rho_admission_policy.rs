#![forbid(unsafe_code)]

use canon::geo::assessment_roll::GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID;
use canon::geo::footprint_roll::GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID;
use canon::geo::{
    GeoCandidateReachStatus, GeoPopulationCaseStatus, GeoPopulationEvaluationRequest,
    GeoPopulationEvidenceStackRequest, GeoRhoAdmissionFallback, GeoRhoAdmissionPolicy, GeoRhoBasis,
    GeoRhoObservationKind, evaluate_population, stack_population_evidence,
};
use flate2::read::GzDecoder;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

const FULL_REACH_POPULATION: &str = "scripts/geo_measurements/fixtures/e4_reach_pluto_vintages_2026-09-08/population_request_roll_universe_pluto_vintage_condo_representation_widened.json.gz";
const POLICY_OVERLAY: &str = "scripts/geo_measurements/fixtures/e4_rho_admission_policy_2026-09-08/overlay_request_roll_owner_threshold_mu_gsf.json.gz";
const POLICY_MEASUREMENT: &str =
    "scripts/geo_measurements/fixtures/e4_rho_admission_policy_2026-09-08/measurement.json";
const GSF_FALSIFICATION_CASE: &str =
    "h7-subject:non-round:0e60eb66adadc21f4be4187e6ca87242d544bcce4ae57592bdc1f65cde1a8001";
const TARGET_CASE_IDS: [&str; 5] = [
    GSF_FALSIFICATION_CASE,
    "h7-subject:non-round:97ae3eeb8035a1914b178d2cf3a447b50735e954a33067d57d6e9a5e1c3e6d58",
    "h7-subject:round-exact-lender:51eae762b75edfcd0d73a0e6de35dd3e29c36e8a207ca7d8c5a154ec6bcd7301",
    "h7-subject:round-exact-lender:5605d0d3f0f90d8112d6f8cecfadd323843a99158ce337e981da62902fe8e5f1",
    "h7-subject:round-exact-lender:7b694e770de8331827bf785d9303bf2d5d8857fdc235011fa2a9b0c2cf37dbf6",
];

#[test]
fn rho_admission_policy_overlay_carries_generic_policy_instances() {
    let overlay: GeoPopulationEvidenceStackRequest = read_json_gz(POLICY_OVERLAY);
    assert_eq!(overlay.case_overlays.len(), 70);
    assert_eq!(overlay.max_overlay_cases, 70);

    let owner_policy = GeoRhoAdmissionPolicy::HardOnlyWhenSupportedMembersAtLeast {
        minimum_supported_members: 2,
        fallback: GeoRhoAdmissionFallback::SoftWithWeight { cost_if_absent: 1 },
    };
    let mut owner_contracts = 0u64;
    for contract in overlay
        .case_overlays
        .iter()
        .flat_map(|overlay| overlay.contracts.iter())
        .filter(|contract| contract.id == GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID)
    {
        owner_contracts += 1;
        let GeoRhoBasis::EmpiricalCalibration {
            admission_policy, ..
        } = &contract.basis
        else {
            panic!("owner exact contract must remain an empirical calibration");
        };
        assert_eq!(admission_policy, &owner_policy);
    }
    assert_eq!(owner_contracts, 40);

    let gsf_overlay = overlay
        .case_overlays
        .iter()
        .find(|overlay| overlay.case_id == GSF_FALSIFICATION_CASE)
        .expect("GSF falsification case overlay");
    let gsf_contract = gsf_overlay
        .contracts
        .iter()
        .find(|contract| contract.id == GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID)
        .expect("GSF band contract");
    assert!(gsf_contract.method_version.ends_with("_property_class_MU"));
    let GeoRhoBasis::EmpiricalCalibration {
        population_id,
        falsification_rule_id,
        ..
    } = &gsf_contract.basis
    else {
        panic!("GSF band contract must remain an empirical calibration");
    };
    assert_eq!(
        population_id,
        "h7-d1-residuals-2026-09-03-roll-property-class-MU"
    );
    assert_eq!(
        falsification_rule_id,
        "truth-gross-sum-outside-property-class-band"
    );

    let gsf_observation = gsf_overlay
        .observations
        .iter()
        .find(|observation| {
            observation.contract_id == GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID
        })
        .expect("GSF band observation");
    let GeoRhoObservationKind::IntegerSumBand { min, max, .. } = &gsf_observation.observation
    else {
        panic!("GSF band observation must be an integer sum band");
    };
    assert_eq!((*min, *max), (6_905, 31_568));
}

#[test]
fn rho_admission_policy_clears_known_false_merges_on_full_reach_population() {
    let population: GeoPopulationEvaluationRequest = read_json_gz(FULL_REACH_POPULATION);
    let overlay: GeoPopulationEvidenceStackRequest = read_json_gz(POLICY_OVERLAY);
    let target_population = GeoPopulationEvaluationRequest {
        version: population.version,
        max_cases: TARGET_CASE_IDS.len(),
        cases: population
            .cases
            .into_iter()
            .filter(|case| is_target_case(&case.id))
            .collect(),
    };
    let target_overlay = GeoPopulationEvidenceStackRequest {
        version: overlay.version,
        max_overlay_cases: TARGET_CASE_IDS.len(),
        max_overlay_observations: overlay
            .case_overlays
            .iter()
            .filter(|overlay| is_target_case(&overlay.case_id))
            .map(|overlay| overlay.observations.len())
            .sum(),
        case_overlays: overlay
            .case_overlays
            .into_iter()
            .filter(|overlay| is_target_case(&overlay.case_id))
            .collect(),
    };
    assert_eq!(target_population.cases.len(), TARGET_CASE_IDS.len());
    assert_eq!(target_overlay.case_overlays.len(), TARGET_CASE_IDS.len());

    let stacked = stack_population_evidence(&target_population, &target_overlay)
        .expect("policy overlay stacks onto target retained cases");
    let evaluated =
        evaluate_population(&stacked.population).expect("target retained cases evaluate");

    assert_eq!(
        evaluated.summary.candidate_reach_full_cases,
        TARGET_CASE_IDS.len() as u64
    );
    assert_eq!(evaluated.summary.resolved_cases, 0);
    assert_eq!(
        evaluated.summary.ambiguous_cases,
        TARGET_CASE_IDS.len() as u64
    );
    assert_eq!(evaluated.summary.conflict_cases, 0);
    assert_eq!(evaluated.summary.false_merge_cases, 0);
    assert_eq!(evaluated.summary.solver_truth_exclusion_cases, 0);
    for case in &evaluated.cases {
        assert!(is_target_case(&case.case_id));
        assert_eq!(case.status, GeoPopulationCaseStatus::Ambiguous);
        assert_eq!(case.candidate_reach, GeoCandidateReachStatus::Full);
        assert_eq!(case.truth_model_in_residual, Some(true));
        assert!(!case.false_merge);
    }
}

#[test]
fn rho_admission_policy_handoff_artifact_is_retained_not_live() {
    let artifact = read_json(POLICY_MEASUREMENT);
    assert_eq!(
        artifact["version"],
        "canon_geo_e4_rho_admission_policy_handoff.v0"
    );
    assert_eq!(artifact["bead"], "bd-2wby");
    assert_eq!(
        artifact["proof_class"],
        "retained_population_measurement_not_live"
    );
    assert_eq!(artifact["frozen_denominator"], 79);
    assert_eq!(artifact["retained_population_denominator"], 70);
    assert_eq!(
        artifact["policy_handoff"]["population_path"],
        FULL_REACH_POPULATION
    );
    assert_eq!(
        artifact["policy_handoff"]["policy_overlay_path"],
        POLICY_OVERLAY
    );
    assert_eq!(
        artifact["policy_handoff"]["full_70_score_status"],
        "handoff_to_bd_1g4x_not_scored_by_bd_2wby"
    );
    assert_eq!(
        artifact["policy_handoff"]["policy_overlay_sha256"],
        sha256_file(POLICY_OVERLAY)
    );
    assert_eq!(
        artifact["target_summary"]["after_false_merges"],
        serde_json::json!(0)
    );
    let cases = artifact["target_falsification_cases"]
        .as_array()
        .expect("target cases array");
    assert_eq!(cases.len(), TARGET_CASE_IDS.len());
    for case_id in TARGET_CASE_IDS {
        assert!(
            cases.iter().any(|case| case["case_id"] == case_id),
            "handoff artifact must name {case_id}"
        );
    }
}

fn is_target_case(case_id: &str) -> bool {
    TARGET_CASE_IDS.contains(&case_id)
}

fn read_json_gz<T: DeserializeOwned>(relative: &str) -> T {
    let file = File::open(repo_path(relative)).expect("open gzipped fixture");
    serde_json::from_reader(GzDecoder::new(BufReader::new(file))).expect("parse gzipped fixture")
}

fn read_json(relative: &str) -> Value {
    let file = File::open(repo_path(relative)).expect("open JSON fixture");
    serde_json::from_reader(BufReader::new(file)).expect("parse JSON fixture")
}

fn sha256_file(relative: &str) -> String {
    let mut file = File::open(repo_path(relative)).expect("open fixture for sha256");
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file.read(&mut buffer).expect("read fixture for sha256");
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    format!("{:x}", hasher.finalize())
}

fn repo_path(relative: &str) -> impl AsRef<Path> {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
