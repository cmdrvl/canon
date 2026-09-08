#![forbid(unsafe_code)]

use canon::geo::assessment_roll::{
    CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION,
    GEO_ASSESSMENT_ROLL_OWNER_AFFILIATE_CONTRACT_ID, GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID,
    GEO_ASSESSMENT_ROLL_OWNER_FAMILY_CONTRACT_ID, GeoAssessmentRollCaseDocument,
    GeoAssessmentRollLotRow, GeoAssessmentRollOwnerCalibration,
    GeoAssessmentRollOwnerContractSource, GeoAssessmentRollOwnerExactNormalizationProfile,
    GeoAssessmentRollOwnerMatch, GeoAssessmentRollOwnerProofClass, GeoAssessmentRollOwnerRequest,
    GeoAssessmentRollPartyFamilyRelationRow, GeoAssessmentRollPartyRow,
    assessment_roll_owner_match, assessment_roll_owner_match_with_exact_normalization,
    assessment_roll_owner_match_with_family_relations,
    assessment_roll_owner_normalized_exact_matches, build_assessment_roll_owner_family_overlay,
    derive_assessment_roll_party_family_relations, normalize_assessment_roll_owner_exact_key,
    normalize_assessment_roll_owner_name, produce_assessment_roll_owner_evidence,
};
use canon::geo::{
    GeoEvidenceCompilationRequest, GeoEvidenceDisposition, GeoEvidenceRecordRef,
    GeoPopulationCaseEvidenceOverlay, GeoPopulationCaseStatus, GeoPopulationEvaluationArtifact,
    GeoPopulationEvaluationRequest, GeoPopulationEvidenceStackRequest, GeoRhoAdmissionFallback,
    GeoRhoAdmissionPolicy, GeoRhoBasis, GeoRhoContract, GeoRhoObservationKind,
    calibration_receipt_blake3, evaluate_population, stack_population_evidence,
};
use canon::namekit::legal_suffix::LegalSuffixProfile;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    process::Command,
};

const FIXTURE_DIR: &str = "scripts/geo_measurements/fixtures/d1_residuals";
const MCP_STACK_DIR: &str = "scripts/geo_measurements/fixtures/d1_residuals/mcp_stack_2026-09-03";
const FULL_REACH_POPULATION: &str = "scripts/geo_measurements/fixtures/e4_reach_pluto_vintages_2026-09-08/population_request_roll_universe_pluto_vintage_condo_representation_widened.json.gz";
const PROPERTY_TYPE_OVERLAY: &str = "scripts/geo_measurements/fixtures/e4_gsf_property_type_bands_2026-09-08/overlay_request_property_type_gsf_bands.json.gz";
const OWNER_NORMALIZED_OVERLAY: &str = "scripts/geo_measurements/fixtures/e4_owner_normalization_2026-09-08/overlay_request_owner_normalized_exact.json.gz";
const OWNER_NORMALIZATION_MEASUREMENT: &str =
    "scripts/geo_measurements/fixtures/e4_owner_normalization_2026-09-08/measurement.json";
const ROLL_SOURCE_DATASET: &str =
    "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION_FY2026P3_x_ACRIS_PARTIES";
const ROLL_SOURCE_RELEASE: &str = "FY2026P3_acris-latest";
const BOUNDED_REPLAY_LOAN_PREFIXES: &[&str] = &[
    "6668e47c", // dossier case and deed-exact case
    "ba808176", // dossier case
    "c6da00e8", "645469ab", "11306d29", "07571f6c", "2b7bc3d7",
];

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

/// Full retained 70-case replay against `evaluation_roll_exact_owner_gsf_band.json`.
///
/// Run with:
/// `cargo test --test geo_assessment_roll assessment_roll_stage_replays_retained_d1_roll_owner_counts -- --ignored --exact`
///
/// Observed runtime on the 2026-09-03 shared checkout: >300s before manual
/// interruption, so this remains opt-in and is not part of the default suite.
#[test]
#[ignore]
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

/// Bounded direct replay for the named G1 subset: dossier cases `6668e47c`
/// and `ba808176`, plus deed-exact cases `c6da00e8`, `645469ab`,
/// `11306d29`, `6668e47c`, `07571f6c`, and `2b7bc3d7`.
///
/// Run with:
/// `cargo test --test geo_assessment_roll assessment_roll_stage_replays_named_d1_subset -- --exact`
///
/// Observed runtime on the 2026-09-03 shared checkout: 1.09s for the
/// `geo_assessment_roll` test binary after compilation.
#[test]
fn assessment_roll_stage_replays_named_d1_subset() {
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
    let case_ids =
        retained_case_ids_for_loan_prefixes(&retained_overlay, BOUNDED_REPLAY_LOAN_PREFIXES);
    assert_eq!(
        case_ids.len(),
        7,
        "the named subset has seven unique retained 26v2 solver cases"
    );

    let population = select_population_cases(&base_population, &case_ids);
    let expected_widened_population =
        select_population_cases(&expected_widened_population, &case_ids);
    let retained_overlay = select_overlay_cases(&retained_overlay, &case_ids);
    let request = assessment_roll_owner_fixture_request(&population, &retained_overlay);
    let artifact =
        produce_assessment_roll_owner_evidence(&request).expect("owner stage produces artifact");

    assert_eq!(artifact.summary.cases, case_ids.len() as u64);
    assert_eq!(
        universe_by_case(&artifact.widened_population),
        universe_by_case(&expected_widened_population),
        "bounded replay must reproduce assessment-roll block widening"
    );
    assert_eq!(
        owner_signature(&artifact.overlay),
        owner_signature(&retained_overlay),
        "bounded replay must reproduce retained owner evidence for the named subset"
    );

    let combined_overlay = retained_overlay_with_stage_owner(&retained_overlay, &artifact.overlay);
    let stacked = stack_population_evidence(&artifact.widened_population, &combined_overlay)
        .expect("bounded stage owner overlay stacks with retained non-owner channels");
    let evaluation =
        evaluate_population(&stacked.population).expect("bounded stacked population evaluates");
    assert_eq!(evaluation.summary.cases, case_ids.len() as u64);
    assert_named_case_statuses_and_forced_sets_match_retained(
        &evaluation,
        &retained_evaluation,
        &case_ids,
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

#[test]
fn owner_exact_normalization_splits_safe_variants_from_true_mismatches() {
    let profile = GeoAssessmentRollOwnerExactNormalizationProfile::legal_suffix_numeric_ordinal(
        LegalSuffixProfile::RegabFirmIdentity,
    );
    let kingsbridge = BTreeSet::from([
        normalize_assessment_roll_owner_name("KINGSBRIDGE ASSOCIATES, LLC"),
        normalize_assessment_roll_owner_name("KINGSBRIDGE ASSOCIATES II, LLC"),
    ]);
    let manhattan_owner = BTreeSet::from([normalize_assessment_roll_owner_name(
        "591 MANHATTAN AVE OWNER LLC",
    )]);
    let ahead = BTreeSet::from([normalize_assessment_roll_owner_name("AHEAD REALTY LLC")]);
    let west_24 = BTreeSet::from([normalize_assessment_roll_owner_name(
        "WEST 24TH OWNERS CORP.",
    )]);
    let linden = BTreeSet::from([normalize_assessment_roll_owner_name("104 LINDEN BLVD, LLC")]);
    let parchen = BTreeSet::from([normalize_assessment_roll_owner_name("316 PARCHEN LLC")]);
    let emmons = BTreeSet::from([normalize_assessment_roll_owner_name(
        "1809 EMMONS AVENUE RETAIL LLC",
    )]);

    assert_eq!(
        normalize_assessment_roll_owner_exact_key("WEST 24 OWNERS CORP.", profile),
        normalize_assessment_roll_owner_exact_key("WEST 24TH OWNERS CORP.", profile)
    );
    assert_eq!(
        normalize_assessment_roll_owner_exact_key(
            "AHEAD REALTY, LLC",
            GeoAssessmentRollOwnerExactNormalizationProfile::source_norm(),
        ),
        normalize_assessment_roll_owner_name("AHEAD REALTY, LLC"),
        "the default profile must preserve the legacy source-normalized exact predicate"
    );
    assert!(assessment_roll_owner_normalized_exact_matches(
        "KINGSBRIDGE ASSOCIATES",
        &kingsbridge,
        profile
    ));
    assert!(assessment_roll_owner_normalized_exact_matches(
        "591 MANHATTAN AVE OWNER, LLC",
        &manhattan_owner,
        profile
    ));
    assert!(assessment_roll_owner_normalized_exact_matches(
        "AHEAD REALTY, LLC",
        &ahead,
        profile
    ));
    assert_eq!(
        assessment_roll_owner_match_with_exact_normalization("AHEAD REALTY, LLC", &ahead, profile),
        GeoAssessmentRollOwnerMatch::Exact
    );
    assert!(assessment_roll_owner_normalized_exact_matches(
        "WEST 24 OWNERS CORP.",
        &west_24,
        profile
    ));
    assert!(assessment_roll_owner_normalized_exact_matches(
        "104 LINDEN BLVD LLC",
        &linden,
        profile
    ));

    assert!(!assessment_roll_owner_normalized_exact_matches(
        "316 PATCHEN LLC",
        &parchen,
        profile
    ));
    assert!(!assessment_roll_owner_normalized_exact_matches(
        "PATIN, MICHAEL",
        &emmons,
        profile
    ));
    assert_ne!(
        assessment_roll_owner_match("AHEAD REALTY, LLC", &ahead),
        GeoAssessmentRollOwnerMatch::Exact,
        "legacy exact matching remains bound to the source-normalized predicate"
    );
}

#[test]
fn owner_exact_normalization_profile_is_consumed_by_owner_stage() {
    let population = GeoPopulationEvaluationRequest {
        version: canon::geo::CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        max_cases: 1,
        cases: vec![canon::geo::GeoLabeledCompositionCase {
            id: "case-owner-normalization".to_string(),
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
            case_id: "case-owner-normalization".to_string(),
            document_id: "doc-owner-normalization".to_string(),
        }],
        contract_source: contract_source(),
        calibration: calibration_from_fixture_contracts(
            &fixture_contract(GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID),
            &fixture_contract(GEO_ASSESSMENT_ROLL_OWNER_AFFILIATE_CONTRACT_ID),
        ),
        roll_rows: vec![
            GeoAssessmentRollLotRow {
                bbl: "1000000001".to_string(),
                owner: "AHEAD REALTY, LLC".to_string(),
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
                owner: "OTHER OWNER LLC".to_string(),
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
            document_id: "doc-owner-normalization".to_string(),
            party_type: "1".to_string(),
            party_name_norm: "AHEAD REALTY LLC".to_string(),
            source_record_id:
                "EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:doc-owner-normalization:AHEAD_REALTY_LLC"
                    .to_string(),
            source_vintage: "latest".to_string(),
        }],
        max_cases: 1,
        max_roll_rows: 2,
        max_party_rows: 1,
        max_overlay_observations: 4,
    };

    let default_artifact =
        produce_assessment_roll_owner_evidence(&request).expect("default owner request builds");
    assert_eq!(default_artifact.summary.exact_hard_observations, 0);
    assert_eq!(default_artifact.summary.affiliate_soft_observations, 1);

    let mut normalized_request = request;
    normalized_request.calibration.exact_normalization_profile =
        GeoAssessmentRollOwnerExactNormalizationProfile::legal_suffix_numeric_ordinal(
            LegalSuffixProfile::RegabFirmIdentity,
        );
    let normalized_artifact = produce_assessment_roll_owner_evidence(&normalized_request)
        .expect("normalized owner request builds");
    assert_eq!(
        normalized_artifact.summary.exact_hard_observations, 1,
        "the explicit normalized profile must let the owner contract consume the safer exact key"
    );
    assert_eq!(normalized_artifact.summary.affiliate_soft_observations, 0);

    let overlay = &normalized_artifact.overlay.case_overlays[0];
    let contract = overlay
        .contracts
        .iter()
        .find(|contract| contract.id == GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID)
        .expect("normalized exact owner contract");
    assert!(
        contract
            .method_version
            .ends_with("legal_suffix_numeric_ordinal")
    );
    let GeoRhoObservationKind::IntegerSumBand {
        measure, values, ..
    } = &overlay.observations[0].observation
    else {
        panic!("normalized exact owner observation is an integer band");
    };
    assert_eq!(
        measure.semantic_id,
        "assessment_roll.owner_not_exact.legal_suffix_numeric_ordinal"
    );
    assert_eq!(
        values
            .iter()
            .map(|value| (value.id.as_str(), value.value))
            .collect::<BTreeMap<_, _>>(),
        BTreeMap::from([("1000000001", 0), ("1000000002", 1)])
    );
}

#[test]
fn owner_exact_normalization_handoff_is_retained_and_ready_to_score() {
    let measurement: Value = read_json(repo_path(OWNER_NORMALIZATION_MEASUREMENT));

    assert_eq!(
        measurement["version"],
        "canon_geo_e4_owner_exact_normalization_handoff.v0"
    );
    assert_eq!(measurement["bead"], "bd-1fq2");
    assert_eq!(
        measurement["proof_class"],
        "retained_population_measurement_not_live"
    );
    assert_eq!(measurement["release_claim_allowed"], false);
    assert_eq!(measurement["frozen_denominator"], 77);
    assert_eq!(measurement["reported_frozen_denominator"], 79);
    assert_eq!(measurement["retained_population_deficit"], 7);
    assert_eq!(measurement["retained_population_denominator"], 70);
    assert_eq!(
        measurement["baseline"]["reach_full_partial_none"],
        serde_json::json!([70, 0, 0])
    );
    assert_eq!(measurement["baseline"]["resolved"], 7);
    assert_eq!(measurement["baseline"]["exactly_correct"], 7);
    assert_eq!(measurement["baseline"]["false_merges"], 0);
    assert_eq!(measurement["baseline"]["truth_exclusions"], 15);
    assert_eq!(
        measurement["score_handoff"]["population_path"],
        FULL_REACH_POPULATION
    );
    assert_eq!(
        measurement["score_handoff"]["source_property_type_overlay_path"],
        PROPERTY_TYPE_OVERLAY
    );
    assert_eq!(
        measurement["score_handoff"]["owner_normalized_overlay_path"],
        OWNER_NORMALIZED_OVERLAY
    );
    assert_eq!(
        measurement["score_handoff"]["full_70_score_status"],
        "READY_TO_SCORE_by_bd_1g4x_safe_normalization_subset_not_scored_by_bd_1fq2_69dbf5da_quarantined_until_bd_2rc0"
    );
    assert_eq!(
        measurement["score_handoff"]["overlay_sha256"],
        sha256_file(repo_path(OWNER_NORMALIZED_OVERLAY))
    );
    assert_eq!(
        measurement["score_handoff"]["source_property_type_overlay_sha256"],
        sha256_file(repo_path(PROPERTY_TYPE_OVERLAY))
    );
    assert_eq!(
        calibration_receipt_blake3(&measurement["calibration_profile"])
            .expect("calibration profile hashes"),
        measurement["calibration_profile_blake3"]
            .as_str()
            .expect("calibration hash is a string")
    );
    assert_eq!(
        measurement["classification"]["normalization_fixable_case_count"],
        5
    );
    assert_eq!(measurement["classification"]["owner_exact_family_total"], 7);
    let excluded = measurement["classification"]["calibration_exclusions"]
        .as_array()
        .expect("calibration exclusions")
        .iter()
        .map(|value| value.as_str().expect("case prefix").to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        excluded,
        BTreeSet::from(["3899edce".to_string(), "69dbf5da".to_string()])
    );
}

#[test]
fn owner_exact_normalization_handoff_changes_only_safe_source_derived_values() {
    let measurement: Value = read_json(repo_path(OWNER_NORMALIZATION_MEASUREMENT));
    let source_overlay = case_overlay_map(&read_json_gz::<Value>(repo_path(PROPERTY_TYPE_OVERLAY)));
    let normalized_overlay =
        case_overlay_map(&read_json_gz::<Value>(repo_path(OWNER_NORMALIZED_OVERLAY)));
    let expected_flips = expected_owner_normalization_flips(&measurement);
    let expected_changed = expected_flips.keys().cloned().collect::<BTreeSet<_>>();

    assert_eq!(source_overlay.len(), 70);
    assert_eq!(normalized_overlay.len(), 70);
    assert_eq!(
        expected_changed.len(),
        measurement["score_handoff"]["changed_owner_case_count"]
            .as_u64()
            .expect("changed count") as usize
    );

    let mut actual_changed = BTreeSet::new();
    for (case_id, source_case) in &source_overlay {
        let normalized_case = normalized_overlay
            .get(case_id)
            .expect("normalized overlay preserves case ids");
        if source_case != normalized_case {
            actual_changed.insert(case_id.clone());
            assert!(
                expected_changed.contains(case_id),
                "unexpected owner-normalization change in {case_id}"
            );
            assert_eq!(
                owner_value_member_ids(source_case),
                owner_value_member_ids(normalized_case),
                "owner normalization must not add candidates for {case_id}"
            );
            assert_eq!(
                owner_source_record_ids(source_case),
                owner_source_record_ids(normalized_case),
                "owner normalization must preserve source records for {case_id}"
            );
            let mut reverted = strip_owner_normalization_metadata_delta(normalized_case.clone());
            revert_owner_value_flips(
                &mut reverted,
                expected_flips
                    .get(case_id)
                    .unwrap_or_else(|| panic!("expected flips for {case_id}")),
            );
            assert_eq!(
                strip_owner_normalization_metadata_delta(source_case.clone()),
                reverted,
                "only declared owner_not_exact values and owner exact metadata may change for {case_id}"
            );
        }
    }
    assert_eq!(actual_changed, expected_changed);
    assert!(
        actual_changed
            .iter()
            .any(|case_id| case_id.contains("ae5fa9ee"))
    );
    assert!(
        actual_changed
            .iter()
            .any(|case_id| case_id.contains("13ff4751"))
    );
    assert!(
        actual_changed
            .iter()
            .any(|case_id| case_id.contains("3709a9d0"))
    );
    assert!(
        actual_changed
            .iter()
            .any(|case_id| case_id.contains("3991a574"))
    );
    assert!(
        actual_changed
            .iter()
            .any(|case_id| case_id.contains("f5588ba9"))
    );
    for quarantined in ["3899edce", "69dbf5da"] {
        let case_id = source_overlay
            .keys()
            .find(|case_id| case_id.contains(quarantined))
            .unwrap_or_else(|| panic!("missing quarantined case {quarantined}"));
        assert_eq!(
            source_overlay.get(case_id),
            normalized_overlay.get(case_id),
            "{quarantined} must remain unchanged until its source conflict is resolved"
        );
    }
}

#[test]
fn exact_owner_policy_demotes_singleton_match_before_it_can_exclude_truth() {
    let population = GeoPopulationEvaluationRequest {
        version: canon::geo::CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        max_cases: 1,
        cases: vec![canon::geo::GeoLabeledCompositionCase {
            id: "case-owner-policy-demotion".to_string(),
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
                parcels: vec!["1000000001".to_string(), "1000000002".to_string()],
                buildings: Vec::new(),
            },
        }],
    };
    let mut calibration = calibration_from_fixture_contracts(
        &fixture_contract(GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID),
        &fixture_contract(GEO_ASSESSMENT_ROLL_OWNER_AFFILIATE_CONTRACT_ID),
    );
    calibration.exact_admission_policy =
        GeoRhoAdmissionPolicy::HardOnlyWhenSupportedMembersAtLeast {
            minimum_supported_members: 2,
            fallback: GeoRhoAdmissionFallback::SoftWithWeight { cost_if_absent: 1 },
        };
    let artifact = produce_assessment_roll_owner_evidence(&GeoAssessmentRollOwnerRequest {
        version: CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION.to_string(),
        proof_class: GeoAssessmentRollOwnerProofClass::Fixture,
        population,
        case_documents: vec![GeoAssessmentRollCaseDocument {
            case_id: "case-owner-policy-demotion".to_string(),
            document_id: "doc-owner-policy-demotion".to_string(),
        }],
        contract_source: contract_source(),
        calibration,
        roll_rows: vec![
            GeoAssessmentRollLotRow {
                bbl: "1000000001".to_string(),
                owner: "ACME BORROWER LLC".to_string(),
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
                owner: "AFFILIATE HOLDINGS LLC".to_string(),
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
            document_id: "doc-owner-policy-demotion".to_string(),
            party_type: "1".to_string(),
            party_name_norm: "ACME BORROWER LLC".to_string(),
            source_record_id:
                "EDGAR_DB.DBT_STAGING_GEO.STG_GEO_NYC_ACRIS_PARTIES:doc-owner-policy-demotion:ACME_BORROWER_LLC"
                    .to_string(),
            source_vintage: "latest".to_string(),
        }],
        max_cases: 1,
        max_roll_rows: 2,
        max_party_rows: 1,
        max_overlay_observations: 4,
    })
    .expect("owner policy request produces overlay");

    let stacked = stack_population_evidence(&artifact.widened_population, &artifact.overlay)
        .expect("owner policy overlay stacks");
    let compilation = canon::geo::compile_evidence(&stacked.population.cases[0].evidence)
        .expect("owner policy evidence compiles");
    assert!(compilation.composition_request.hard_constraints.is_empty());
    assert_eq!(compilation.composition_request.soft_preferences.len(), 1);
    let exact_admission = compilation
        .admissions
        .iter()
        .find(|admission| admission.contract.id == GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID)
        .expect("exact owner admission");
    assert_eq!(
        exact_admission.disposition,
        GeoEvidenceDisposition::SoftPreference
    );
    assert_eq!(
        exact_admission.admission_reason.as_deref(),
        Some("rho_member_support_not_met")
    );
}

#[test]
fn party_family_relation_identifies_recoverable_owner_falsification_variants() {
    struct OwnerFalsification<'a> {
        borrower_name_norm: &'a str,
        truth_owner_names: &'a [&'a str],
        family_member_name_norms: &'a [&'a str],
        supported_truth_lots: u64,
        truth_lots: u64,
    }

    let cases = [
        OwnerFalsification {
            borrower_name_norm: "KEW GARDENS OWNERS CORP",
            truth_owner_names: &["KEW GARDENS OWNERS CORP", "KEW GRDNS OWNRS CP"],
            family_member_name_norms: &["KEW GARDENS OWNERS CORP", "KEW GRDNS OWNRS CP"],
            supported_truth_lots: 2,
            truth_lots: 2,
        },
        OwnerFalsification {
            borrower_name_norm: "82 HORATIO OWNERS LTD",
            truth_owner_names: &[
                "82 HORATIO OWNERS LTD",
                "THE CITY OF NEW YORK",
                "THE CITY OF NEW YORK",
                "THE CITY OF NEW YORK",
            ],
            family_member_name_norms: &["82 HORATIO OWNERS LTD", "HORATIO OWNERS"],
            supported_truth_lots: 1,
            truth_lots: 4,
        },
        OwnerFalsification {
            borrower_name_norm: "WEST 23RD STREET OWNERS CORP",
            truth_owner_names: &["UNAVAILABLE OWNER", "WEST 23RD STREET OWNERS CORP"],
            family_member_name_norms: &["WEST 23RD STREET OWNERS CORP", "WEST 23 STREET OWNERS"],
            supported_truth_lots: 1,
            truth_lots: 2,
        },
        OwnerFalsification {
            borrower_name_norm: "TALBOT APARTMENTS INC",
            truth_owner_names: &["TALBOT APARTMENTS INC", "TALBOT APARTMENT INC"],
            family_member_name_norms: &["TALBOT APARTMENTS INC", "TALBOT APARTMENT INC"],
            supported_truth_lots: 2,
            truth_lots: 2,
        },
    ];

    for (index, case) in cases.iter().enumerate() {
        let borrowers = BTreeSet::from([case.borrower_name_norm.to_string()]);
        let family_relations = [party_family_relation(
            "doc-owner-falsification",
            &format!("family-owner-falsification-{index}"),
            case.family_member_name_norms,
        )];
        let supported = case
            .truth_owner_names
            .iter()
            .filter(|owner| {
                matches!(
                    assessment_roll_owner_match_with_family_relations(
                        owner,
                        &borrowers,
                        &family_relations,
                    ),
                    GeoAssessmentRollOwnerMatch::Exact | GeoAssessmentRollOwnerMatch::Family
                )
            })
            .count() as u64;
        assert_eq!(supported, case.supported_truth_lots);
        assert_eq!(case.truth_owner_names.len() as u64, case.truth_lots);
    }

    let borrowers = BTreeSet::from(["KEW GARDENS OWNERS CORP".to_string()]);
    assert_eq!(
        assessment_roll_owner_match("KEW GRDNS OWNRS CP", &borrowers),
        GeoAssessmentRollOwnerMatch::None,
        "the legacy token predicate must not silently learn source-specific aliases"
    );
}

#[test]
fn party_family_relations_derive_from_configured_document_parties() {
    let document_id = "2024041000706002";
    let party_rows = retained_manhattan_owner_party_rows();
    let mut derivation_rows = party_rows.clone();
    derivation_rows.push(acris_party_row(
        document_id,
        "2",
        "CITI REAL ESTATE FUNDING INC.",
        44523888,
        "79a9ae7c38c784da26d7131bb3564eed114e5ff79f8a32be89c913685cd0b164",
    ));
    derivation_rows.push(acris_party_row(
        "2026040100000001",
        "1",
        "SOLO BORROWER LLC",
        1,
        "0000000000000000000000000000000000000000000000000000000000000001",
    ));

    let relations = derive_assessment_roll_party_family_relations(
        &derivation_rows,
        &["1".to_string()],
        "fixture:acris_party_family",
    )
    .expect("same-document party relation derives");

    assert_eq!(relations.len(), 1);
    let relation = &relations[0];
    assert_eq!(relation.document_id, document_id);
    assert_eq!(
        relation.family_id,
        format!("party-family:{document_id}:1:2026-08-10")
    );
    assert_eq!(
        relation.source_record_id,
        format!("fixture:acris_party_family:{document_id}:1:2026-08-10")
    );
    let expected_members = [
        "574 MANHATTAN AVE OWNER, LLC",
        "591 MANHATTAN AVE OWNER LLC",
        "592 MANHATTAN AVE OWNER, LLC",
        "593 MANHATTAN AVE OWNER, LLC",
        "595 MANHATTAN AVE OWNER, LLC",
        "602 MANHATTAN AVE OWNER, LLC",
        "872 LORIMER ST OWNER LLC",
    ]
    .into_iter()
    .map(normalize_assessment_roll_owner_name)
    .collect::<Vec<_>>();
    assert_eq!(relation.member_name_norms, expected_members);
    assert!(
        !relation
            .member_name_norms
            .contains(&"CITI REAL ESTATE FUNDING INC".to_string()),
        "configured party roles, not all ACRIS rows, define the owner family"
    );

    let lender_relations = derive_assessment_roll_party_family_relations(
        &derivation_rows,
        &["2".to_string()],
        "fixture:acris_party_family",
    )
    .expect("single lender row is valid but not a family");
    assert!(lender_relations.is_empty());
}

#[test]
fn party_family_overlay_hard_admits_when_relation_supports_two_members() {
    let population = GeoPopulationEvaluationRequest {
        version: canon::geo::CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        max_cases: 1,
        cases: vec![canon::geo::GeoLabeledCompositionCase {
            id: "case-owner-family".to_string(),
            evidence: GeoEvidenceCompilationRequest {
                version: canon::geo::CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
                profile: canon::geo::GeoCompositionProfile::parcel(),
                universe: canon::geo::GeoCompositionUniverse {
                    parcels: vec!["4066300015".to_string(), "4066300030".to_string()],
                    buildings: Vec::new(),
                },
                contracts: Vec::new(),
                observations: Vec::new(),
                max_assignments: 64,
                max_materialized_models: 64,
            },
            truth_plane: canon::geo::GeoTruthPlane::HumanAdjudication,
            truth: canon::geo::GeoCompositionModel {
                parcels: vec!["4066300015".to_string(), "4066300030".to_string()],
                buildings: Vec::new(),
            },
        }],
    };
    let mut calibration = calibration_from_fixture_contracts(
        &fixture_contract(GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID),
        &fixture_contract(GEO_ASSESSMENT_ROLL_OWNER_AFFILIATE_CONTRACT_ID),
    );
    calibration.exact_admission_policy =
        GeoRhoAdmissionPolicy::HardOnlyWhenSupportedMembersAtLeast {
            minimum_supported_members: 2,
            fallback: GeoRhoAdmissionFallback::SoftWithWeight { cost_if_absent: 1 },
        };
    let request = GeoAssessmentRollOwnerRequest {
        version: CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION.to_string(),
        proof_class: GeoAssessmentRollOwnerProofClass::Fixture,
        population,
        case_documents: vec![GeoAssessmentRollCaseDocument {
            case_id: "case-owner-family".to_string(),
            document_id: "2025120900884001".to_string(),
        }],
        contract_source: contract_source(),
        calibration,
        roll_rows: vec![
            GeoAssessmentRollLotRow {
                bbl: "4066300015".to_string(),
                owner: "KEW GARDENS OWNERS CORP".to_string(),
                gross_sqft: "45750".to_string(),
                units: "60".to_string(),
                condo_number: String::new(),
                source_record_id:
                    "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:4066300015"
                        .to_string(),
                source_vintage: "FY2026P3".to_string(),
            },
            GeoAssessmentRollLotRow {
                bbl: "4066300030".to_string(),
                owner: "KEW GRDNS OWNRS CP".to_string(),
                gross_sqft: "34329".to_string(),
                units: "45".to_string(),
                condo_number: String::new(),
                source_record_id:
                    "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:4066300030"
                        .to_string(),
                source_vintage: "FY2026P3".to_string(),
            },
        ],
        party_rows: vec![GeoAssessmentRollPartyRow {
            document_id: "2025120900884001".to_string(),
            party_type: "1".to_string(),
            party_name_norm: "KEW GARDENS OWNERS CORP".to_string(),
            source_record_id:
                "EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_PARTIES_EXT:2026-08-10:45997530"
                    .to_string(),
            source_vintage: "2026-08-10".to_string(),
        }],
        max_cases: 1,
        max_roll_rows: 2,
        max_party_rows: 1,
        max_overlay_observations: 2,
    };
    let family_overlay = build_assessment_roll_owner_family_overlay(
        &request,
        &[party_family_relation(
            "2025120900884001",
            "family:kew-gardens-owner-name-variants",
            &["KEW GARDENS OWNERS CORP", "KEW GRDNS OWNRS CP"],
        )],
    )
    .expect("party-family overlay builds");

    assert_eq!(family_overlay.case_overlays.len(), 1);
    let overlay = &family_overlay.case_overlays[0];
    assert_eq!(overlay.contracts.len(), 1);
    assert_eq!(
        overlay.contracts[0].id,
        GEO_ASSESSMENT_ROLL_OWNER_FAMILY_CONTRACT_ID
    );
    let GeoRhoObservationKind::IntegerSumBand { values, .. } = &overlay.observations[0].observation
    else {
        panic!("family observation must be an integer sum band");
    };
    let supported = values
        .iter()
        .filter(|value| value.value == 0)
        .map(|value| value.id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        supported,
        BTreeSet::from(["4066300015".to_string(), "4066300030".to_string()])
    );

    let stacked = stack_population_evidence(&request.population, &family_overlay)
        .expect("family overlay stacks");
    let compilation = canon::geo::compile_evidence(&stacked.population.cases[0].evidence)
        .expect("family overlay compiles");
    let family_admission = compilation
        .admissions
        .iter()
        .find(|admission| admission.contract.id == GEO_ASSESSMENT_ROLL_OWNER_FAMILY_CONTRACT_ID)
        .expect("family admission");
    assert_eq!(
        family_admission.disposition,
        GeoEvidenceDisposition::HardConstraint
    );
    assert!(family_admission.admission_reason.is_none());
    assert_eq!(compilation.composition_request.hard_constraints.len(), 1);
    assert!(compilation.composition_request.soft_preferences.is_empty());
}

#[test]
fn derived_party_family_overlay_supports_retained_multi_owner_truth_lots() {
    let document_id = "2024041000706002";
    let truth_parcels = vec![
        "3026790021".to_string(),
        "3026790022".to_string(),
        "3026790023".to_string(),
        "3026790054".to_string(),
        "3026800037".to_string(),
        "3026800043".to_string(),
        "3026800046".to_string(),
    ];
    let population = GeoPopulationEvaluationRequest {
        version: canon::geo::CANON_GEO_POPULATION_REQUEST_VERSION.to_string(),
        max_cases: 1,
        cases: vec![canon::geo::GeoLabeledCompositionCase {
            id: "case-retained-manhattan-owner-family".to_string(),
            evidence: GeoEvidenceCompilationRequest {
                version: canon::geo::CANON_GEO_EVIDENCE_REQUEST_VERSION.to_string(),
                profile: canon::geo::GeoCompositionProfile::parcel(),
                universe: canon::geo::GeoCompositionUniverse {
                    parcels: truth_parcels.clone(),
                    buildings: Vec::new(),
                },
                contracts: Vec::new(),
                observations: Vec::new(),
                max_assignments: 64,
                max_materialized_models: 64,
            },
            truth_plane: canon::geo::GeoTruthPlane::HumanAdjudication,
            truth: canon::geo::GeoCompositionModel {
                parcels: truth_parcels.clone(),
                buildings: Vec::new(),
            },
        }],
    };
    let mut calibration = calibration_from_fixture_contracts(
        &fixture_contract(GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID),
        &fixture_contract(GEO_ASSESSMENT_ROLL_OWNER_AFFILIATE_CONTRACT_ID),
    );
    calibration.exact_admission_policy =
        GeoRhoAdmissionPolicy::HardOnlyWhenSupportedMembersAtLeast {
            minimum_supported_members: 2,
            fallback: GeoRhoAdmissionFallback::SoftWithWeight { cost_if_absent: 1 },
        };
    let party_rows = retained_manhattan_owner_party_rows();
    let family_relations = derive_assessment_roll_party_family_relations(
        &party_rows,
        &["1".to_string()],
        "fixture:acris_party_family",
    )
    .expect("retained ACRIS type-1 party rows derive a family relation");
    let request = GeoAssessmentRollOwnerRequest {
        version: CANON_GEO_ASSESSMENT_ROLL_OWNER_REQUEST_VERSION.to_string(),
        proof_class: GeoAssessmentRollOwnerProofClass::Fixture,
        population,
        case_documents: vec![GeoAssessmentRollCaseDocument {
            case_id: "case-retained-manhattan-owner-family".to_string(),
            document_id: document_id.to_string(),
        }],
        contract_source: contract_source(),
        calibration,
        roll_rows: retained_manhattan_owner_roll_rows(),
        party_rows,
        max_cases: 1,
        max_roll_rows: truth_parcels.len(),
        max_party_rows: 7,
        max_overlay_observations: 4,
    };

    let family_overlay = build_assessment_roll_owner_family_overlay(&request, &family_relations)
        .expect("derived party-family overlay builds");
    assert_eq!(family_overlay.case_overlays.len(), 1);
    let overlay = &family_overlay.case_overlays[0];
    assert_eq!(
        overlay.observations[0].contract_id,
        GEO_ASSESSMENT_ROLL_OWNER_FAMILY_CONTRACT_ID
    );
    assert!(
        overlay.observations[0]
            .source_records
            .iter()
            .any(|record| record
                .source_record_id
                .starts_with("fixture:acris_party_family:2024041000706002")),
        "family observation must cite the derived family relation record"
    );
    let GeoRhoObservationKind::IntegerSumBand { values, .. } = &overlay.observations[0].observation
    else {
        panic!("family observation must be an integer sum band");
    };
    let supported = values
        .iter()
        .filter(|value| value.value == 0)
        .map(|value| value.id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(supported, truth_parcels.into_iter().collect());

    let stacked = stack_population_evidence(&request.population, &family_overlay)
        .expect("derived family overlay stacks");
    let compilation = canon::geo::compile_evidence(&stacked.population.cases[0].evidence)
        .expect("derived family overlay compiles");
    let family_admission = compilation
        .admissions
        .iter()
        .find(|admission| admission.contract.id == GEO_ASSESSMENT_ROLL_OWNER_FAMILY_CONTRACT_ID)
        .expect("family admission");
    assert_eq!(
        family_admission.disposition,
        GeoEvidenceDisposition::HardConstraint
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
        exact_normalization_profile: GeoAssessmentRollOwnerExactNormalizationProfile::source_norm(),
        exact_admission_policy: GeoRhoAdmissionPolicy::Declared,
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

fn party_family_relation(
    document_id: &str,
    family_id: &str,
    member_name_norms: &[&str],
) -> GeoAssessmentRollPartyFamilyRelationRow {
    GeoAssessmentRollPartyFamilyRelationRow {
        document_id: document_id.to_string(),
        family_id: family_id.to_string(),
        member_name_norms: member_name_norms
            .iter()
            .map(|value| value.to_string())
            .collect(),
        source_record_id: format!(
            "fixture:party_family_relation:{document_id}:{}",
            family_id.replace(':', "_")
        ),
        source_vintage: "retained-2026-09-08".to_string(),
    }
}

fn retained_manhattan_owner_party_rows() -> Vec<GeoAssessmentRollPartyRow> {
    let document_id = "2024041000706002";
    let source_hash = "79a9ae7c38c784da26d7131bb3564eed114e5ff79f8a32be89c913685cd0b164";
    vec![
        acris_party_row(
            document_id,
            "1",
            "574 MANHATTAN AVE OWNER, LLC",
            44565176,
            source_hash,
        ),
        acris_party_row(
            document_id,
            "1",
            "591 MANHATTAN AVE OWNER LLC",
            44573802,
            source_hash,
        ),
        acris_party_row(
            document_id,
            "1",
            "592 MANHATTAN AVE OWNER, LLC",
            44550596,
            source_hash,
        ),
        acris_party_row(
            document_id,
            "1",
            "593 MANHATTAN AVE OWNER, LLC",
            44521437,
            source_hash,
        ),
        acris_party_row(
            document_id,
            "1",
            "595 MANHATTAN AVE OWNER, LLC",
            44559763,
            source_hash,
        ),
        acris_party_row(
            document_id,
            "1",
            "602 MANHATTAN AVE OWNER, LLC",
            44560221,
            source_hash,
        ),
        acris_party_row(
            document_id,
            "1",
            "872 LORIMER ST OWNER LLC",
            44522200,
            source_hash,
        ),
    ]
}

fn retained_manhattan_owner_roll_rows() -> Vec<GeoAssessmentRollLotRow> {
    [
        ("3026790021", "595 MANHATTAN AVE OWNER, LLC", "2160", "3"),
        ("3026790022", "593 MANHATTAN AVE OWNER, LLC", "2926", "3"),
        ("3026790023", "591 MANHATTAN AVE OWNER, LLC", "2926", "3"),
        ("3026790054", "872 LORIMER ST OWNER, LLC", "4125", "6"),
        ("3026800037", "574 MANHATTAN AVE OWNER, LLC", "6636", "10"),
        ("3026800043", "592 MANHATTAN AVE OWNER, LLC", "4500", "6"),
        ("3026800046", "602 MANHATTAN AVE OWNER, LLC", "7600", "9"),
    ]
    .into_iter()
    .map(|(bbl, owner, gross_sqft, units)| GeoAssessmentRollLotRow {
        bbl: bbl.to_string(),
        owner: owner.to_string(),
        gross_sqft: gross_sqft.to_string(),
        units: units.to_string(),
        condo_number: String::new(),
        source_record_id: format!(
            "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:{bbl}"
        ),
        source_vintage: "FY2026P3".to_string(),
    })
    .collect()
}

fn acris_party_row(
    document_id: &str,
    party_type: &str,
    name: &str,
    source_row_number: u64,
    raw_csv_sha256: &str,
) -> GeoAssessmentRollPartyRow {
    GeoAssessmentRollPartyRow {
        document_id: document_id.to_string(),
        party_type: party_type.to_string(),
        party_name_norm: normalize_assessment_roll_owner_name(name),
        source_record_id: format!(
            "EDGAR_DB.SOURCE.NYC_ACRIS_REAL_PROPERTY_PARTIES_EXT:2026-08-10:{source_row_number}:{raw_csv_sha256}"
        ),
        source_vintage: "2026-08-10".to_string(),
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

fn retained_case_ids_for_loan_prefixes(
    overlay: &GeoPopulationEvidenceStackRequest,
    loan_prefixes: &[&str],
) -> BTreeSet<String> {
    let requested_prefixes = loan_prefixes.iter().copied().collect::<BTreeSet<_>>();
    let mut cases_by_prefix = requested_prefixes
        .iter()
        .map(|prefix| (prefix.to_string(), BTreeSet::<String>::new()))
        .collect::<BTreeMap<_, _>>();
    for case_overlay in &overlay.case_overlays {
        for source_record in case_overlay
            .observations
            .iter()
            .flat_map(|observation| &observation.source_records)
        {
            for prefix in &requested_prefixes {
                if source_record.source_record_id.contains(prefix) {
                    cases_by_prefix
                        .get_mut(*prefix)
                        .expect("prefix entry exists")
                        .insert(case_overlay.case_id.clone());
                }
            }
        }
    }

    let mut selected = BTreeSet::new();
    for (prefix, cases) in cases_by_prefix {
        assert_eq!(
            cases.len(),
            1,
            "loan prefix {prefix} must bind to exactly one retained overlay case"
        );
        selected.extend(cases);
    }
    selected
}

fn select_population_cases(
    population: &GeoPopulationEvaluationRequest,
    case_ids: &BTreeSet<String>,
) -> GeoPopulationEvaluationRequest {
    let cases = population
        .cases
        .iter()
        .filter(|case| case_ids.contains(&case.id))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        cases.len(),
        case_ids.len(),
        "selected population must contain every named replay case"
    );
    GeoPopulationEvaluationRequest {
        version: population.version.clone(),
        max_cases: cases.len(),
        cases,
    }
}

fn select_overlay_cases(
    overlay: &GeoPopulationEvidenceStackRequest,
    case_ids: &BTreeSet<String>,
) -> GeoPopulationEvidenceStackRequest {
    let case_overlays = overlay
        .case_overlays
        .iter()
        .filter(|case_overlay| case_ids.contains(&case_overlay.case_id))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        case_overlays.len(),
        case_ids.len(),
        "selected overlay must contain every named replay case"
    );
    let max_overlay_observations = case_overlays
        .iter()
        .map(|case| case.observations.len())
        .sum();
    GeoPopulationEvidenceStackRequest {
        version: overlay.version.clone(),
        max_overlay_cases: case_overlays.len(),
        max_overlay_observations,
        case_overlays,
    }
}

fn assert_named_case_statuses_and_forced_sets_match_retained(
    evaluation: &GeoPopulationEvaluationArtifact,
    retained_evaluation: &Value,
    case_ids: &BTreeSet<String>,
) {
    let retained_cases = retained_evaluation_cases_by_id(retained_evaluation);
    let actual_cases = evaluation
        .cases
        .iter()
        .map(|case| {
            (
                case.case_id.clone(),
                serde_json::to_value(case).expect("evaluation case serializes"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        actual_cases.len(),
        case_ids.len(),
        "bounded replay should evaluate exactly the selected cases"
    );

    for case_id in case_ids {
        let actual = actual_cases
            .get(case_id)
            .unwrap_or_else(|| panic!("bounded replay missing actual case {case_id}"));
        let expected = retained_cases
            .get(case_id)
            .unwrap_or_else(|| panic!("retained evaluation missing case {case_id}"));
        assert_eq!(
            actual["status"], expected["status"],
            "{case_id} status must match retained evaluation"
        );
        assert_eq!(
            actual["hard_forced"], expected["hard_forced"],
            "{case_id} hard forced set must match retained evaluation"
        );
    }
}

fn retained_evaluation_cases_by_id(retained_evaluation: &Value) -> BTreeMap<String, &Value> {
    retained_evaluation["cases"]
        .as_array()
        .expect("retained evaluation cases must be an array")
        .iter()
        .map(|case| {
            (
                case["case_id"]
                    .as_str()
                    .expect("retained evaluation case_id must be a string")
                    .to_string(),
                case,
            )
        })
        .collect()
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

fn case_overlay_map(overlay: &Value) -> BTreeMap<String, Value> {
    overlay["case_overlays"]
        .as_array()
        .expect("case overlays")
        .iter()
        .map(|case| {
            (
                case["case_id"].as_str().expect("case id").to_string(),
                case.clone(),
            )
        })
        .collect()
}

fn expected_owner_normalization_flips(
    measurement: &Value,
) -> BTreeMap<String, BTreeMap<String, u64>> {
    measurement["score_handoff"]["changed_owner_cases"]
        .as_array()
        .expect("changed owner cases")
        .iter()
        .map(|case| {
            let case_id = case["case_id"]
                .as_str()
                .expect("changed case id")
                .to_string();
            let flips = case["flipped_owner_not_exact_values"]
                .as_array()
                .expect("owner value flips")
                .iter()
                .map(|flip| {
                    assert_eq!(flip["after"], 0);
                    (
                        flip["bbl"].as_str().expect("flipped bbl").to_string(),
                        flip["before"].as_u64().expect("before value"),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            (case_id, flips)
        })
        .collect()
}

fn strip_owner_normalization_metadata_delta(mut case: Value) -> Value {
    for contract in case["contracts"]
        .as_array_mut()
        .expect("case contracts are an array")
    {
        if contract["id"] == GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID {
            contract["method_version"] = serde_json::json!("owner-normalization-profile");
            contract["basis"]["population_id"] =
                serde_json::json!("owner-normalization-population");
            contract["basis"]["calibration_blake3"] =
                serde_json::json!("owner-normalization-calibration");
            contract["basis"]["falsification_rule_id"] =
                serde_json::json!("owner-normalization-falsification-rule");
        }
    }
    for observation in case["observations"]
        .as_array_mut()
        .expect("case observations are an array")
    {
        if observation["contract_id"] == GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID {
            observation["observation"]["measure"]["semantic_id"] =
                serde_json::json!("owner-normalization-measure");
        }
    }
    case
}

fn revert_owner_value_flips(case: &mut Value, flips: &BTreeMap<String, u64>) {
    for value in owner_exact_observation_mut(case)["observation"]["values"]
        .as_array_mut()
        .expect("owner exact values")
    {
        let id = value["id"].as_str().expect("value id");
        if let Some(before) = flips.get(id) {
            value["value"] = serde_json::json!(before);
        }
    }
}

fn owner_value_member_ids(case: &Value) -> Vec<String> {
    owner_exact_observation(case)["observation"]["values"]
        .as_array()
        .expect("owner exact values")
        .iter()
        .map(|value| value["id"].as_str().expect("value id").to_string())
        .collect()
}

fn owner_source_record_ids(case: &Value) -> Vec<String> {
    owner_exact_observation(case)["source_records"]
        .as_array()
        .expect("owner source records")
        .iter()
        .map(|record| {
            record["source_record_id"]
                .as_str()
                .expect("source record id")
                .to_string()
        })
        .collect()
}

fn owner_exact_observation(case: &Value) -> &Value {
    case["observations"]
        .as_array()
        .expect("case observations")
        .iter()
        .find(|observation| {
            observation["contract_id"] == GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID
        })
        .expect("owner exact observation")
}

fn owner_exact_observation_mut(case: &mut Value) -> &mut Value {
    case["observations"]
        .as_array_mut()
        .expect("case observations")
        .iter_mut()
        .find(|observation| {
            observation["contract_id"] == GEO_ASSESSMENT_ROLL_OWNER_EXACT_CONTRACT_ID
        })
        .expect("owner exact observation")
}

fn sha256_file(path: PathBuf) -> String {
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()));
    format!("{:x}", Sha256::digest(&bytes))
}

fn rooted(parts: &[&str]) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for part in parts {
        path.push(part);
    }
    path
}

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
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
