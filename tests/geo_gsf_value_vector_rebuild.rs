#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_FOOTPRINT_ROLL_EVIDENCE_REQUEST_VERSION, DEFAULT_MAX_MATERIALIZED_MODELS,
    GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID, GeoAssessmentRollGrossSqftBandCalibration,
    GeoAssessmentRollGrossSqftPropertyBand, GeoAssessmentRollGrossSqftRow,
    GeoEvidenceCompilationRequest, GeoEvidenceRecordRef, GeoFootprintRollCalibration,
    GeoFootprintRollEvidenceRequest, GeoFootprintRollLoanFields, GeoFootprintRollSourceConfig,
    GeoLabeledCompositionCase, GeoPopulationCaseEvidenceOverlay, GeoPopulationEvaluationRequest,
    GeoPopulationEvidenceStackRequest, GeoRhoAdmissionPolicy, GeoRhoObservation,
    GeoRhoObservationKind, materialize_footprint_roll_evidence,
};
use flate2::{Compression, GzBuilder, read::GzDecoder};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    fs::File,
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
};

const POPULATION: &str = "scripts/geo_measurements/fixtures/e4_reach_pluto_vintages_2026-09-08/population_request_roll_universe_pluto_vintage_condo_representation_widened.json.gz";
const SOURCE_OVERLAY: &str = "scripts/geo_measurements/fixtures/e4_gsf_property_type_bands_2026-09-08/overlay_request_property_type_gsf_bands.json.gz";
const ROLL_ROWS: &str = "scripts/geo_measurements/fixtures/d1_residuals/mcp_stack_2026-09-03/assessment_roll_fy2026p3_lots.json.gz";
const OUT_DIR: &str = "scripts/geo_measurements/fixtures/e4_gsf_value_vector_rebuild_2026-09-08";
const CORRECTED_OVERLAY: &str = "scripts/geo_measurements/fixtures/e4_gsf_value_vector_rebuild_2026-09-08/overlay_request_current_universe_gsf_vectors.json.gz";
const MEASUREMENT: &str =
    "scripts/geo_measurements/fixtures/e4_gsf_value_vector_rebuild_2026-09-08/measurement.json";
const PROPERTY_TYPE_CALIBRATION_BLAKE3: &str =
    "f4242562e55bb4ad750d95f744d0f755a7ca2875ab3be6ea1bfe25b577a5473b";
const EXPECTED_SOURCE_TRUTH_BBLS: u64 = 191;
const EXPECTED_SOURCE_GSF_ROWS: u64 = 191;

const TARGETS: [GsfCase; 5] = [
    GsfCase {
        fragment: "524efa30",
        property_key: "CREP-EA6E195092EF94B1",
        filed_size: 17_636,
        property_class: "IN",
        expected_vector_count: 53,
        expected_truth_sum: 17_872,
        expected_min: 12_345,
        expected_max: 28_218,
        expected_inside_band: true,
    },
    GsfCase {
        fragment: "68a1a4ce",
        property_key: "CREP-A2512F406EDBE52C",
        filed_size: 1_064,
        property_class: "RT",
        expected_vector_count: 201,
        expected_truth_sum: 2_134,
        expected_min: 744,
        expected_max: 1_703,
        expected_inside_band: false,
    },
    GsfCase {
        fragment: "e3a5228d",
        property_key: "CREP-11B7C4F0999886E9",
        filed_size: 35_539,
        property_class: "MU",
        expected_vector_count: 19,
        expected_truth_sum: 133_295,
        expected_min: 24_877,
        expected_max: 781_859,
        expected_inside_band: true,
    },
    GsfCase {
        fragment: "91b2df27",
        property_key: "CREP-5BA5855CC20797E5",
        filed_size: 25_886,
        property_class: "RT",
        expected_vector_count: 791,
        expected_truth_sum: 584_046,
        expected_min: 18_120,
        expected_max: 41_418,
        expected_inside_band: false,
    },
    GsfCase {
        fragment: "e1ee0535",
        property_key: "CREP-5F8AE5E0FA6B5506",
        filed_size: 182_845,
        property_class: "OF",
        expected_vector_count: 353,
        expected_truth_sum: 455_607,
        expected_min: 127_991,
        expected_max: 292_553,
        expected_inside_band: false,
    },
];

const OUT_OF_FAMILY_STALE_TARGETS: [OutOfFamilyGsfCase; 3] = [
    OutOfFamilyGsfCase {
        fragment: "0a6ff10e",
        filed_size: 62_969,
        size_source_record_id: "EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT:CREP-FA7183316782A0F2",
        expected_missing_roll_rows: &["1000281301", "1000281302"],
        expected_rebuilt_value_count: None,
        expected_rebuilt_truth_sum: None,
        expected_rebuilt_member: None,
    },
    OutOfFamilyGsfCase {
        fragment: "4e262201",
        filed_size: 80_169,
        size_source_record_id: "EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT:CREP-4779BBC4A8976D26",
        expected_missing_roll_rows: &["2023270012"],
        expected_rebuilt_value_count: None,
        expected_rebuilt_truth_sum: None,
        expected_rebuilt_member: None,
    },
    OutOfFamilyGsfCase {
        fragment: "8fd55140",
        filed_size: 33_006,
        size_source_record_id: "EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT:CREP-186AFA04D41DFF77",
        expected_missing_roll_rows: &[],
        expected_rebuilt_value_count: Some(185),
        expected_rebuilt_truth_sum: Some(44_844),
        expected_rebuilt_member: Some(("2026030007", 18_000)),
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GsfCase {
    fragment: &'static str,
    property_key: &'static str,
    filed_size: u64,
    property_class: &'static str,
    expected_vector_count: usize,
    expected_truth_sum: u64,
    expected_min: u64,
    expected_max: u64,
    expected_inside_band: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OutOfFamilyGsfCase {
    fragment: &'static str,
    filed_size: u64,
    size_source_record_id: &'static str,
    expected_missing_roll_rows: &'static [&'static str],
    expected_rebuilt_value_count: Option<usize>,
    expected_rebuilt_truth_sum: Option<u64>,
    expected_rebuilt_member: Option<(&'static str, u64)>,
}

#[derive(Debug, Deserialize)]
struct RollFixtureRow {
    gross_sqft: String,
    units: String,
}

#[test]
fn gsf_value_vector_rebuild_artifact_matches_current_universe_replay() {
    let expected = build_corrected_overlay();
    let actual: GeoPopulationEvidenceStackRequest = read_json_gz(CORRECTED_OVERLAY);
    assert_eq!(
        actual, expected,
        "committed handoff overlay must be the current-universe rebuild"
    );

    let measurement = read_json(MEASUREMENT);
    assert_eq!(
        measurement["score_handoff"]["overlay_sha256"],
        sha256_file(repo_path(CORRECTED_OVERLAY))
    );
    assert_eq!(
        measurement["score_handoff"]["overlay_uncompressed_sha256"],
        sha256_bytes(&read_gzip_uncompressed_bytes(repo_path(CORRECTED_OVERLAY)))
    );
}

#[test]
fn gsf_value_vectors_include_truth_bbls_for_each_exclusion_case() {
    let population = read_json_gz::<GeoPopulationEvaluationRequest>(POPULATION);
    let overlay = read_json_gz::<GeoPopulationEvidenceStackRequest>(CORRECTED_OVERLAY);

    for target in TARGETS {
        let case = population_case(&population, target.fragment);
        let overlay_case = overlay_case(&overlay, &case.id);
        let observation = gsf_observation(overlay_case);
        let GeoRhoObservationKind::IntegerSumBand {
            values, min, max, ..
        } = &observation.observation
        else {
            panic!("{} GSF observation must be an integer sum band", case.id);
        };

        let value_ids = values
            .iter()
            .map(|value| value.id.as_str())
            .collect::<BTreeSet<_>>();
        let universe_ids = case
            .evidence
            .universe
            .parcels
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            value_ids, universe_ids,
            "{} GSF vector must be built from the current bound universe",
            target.fragment
        );
        assert_eq!(values.len(), target.expected_vector_count);
        assert_eq!((*min, *max), (target.expected_min, target.expected_max));

        let values_by_id = values
            .iter()
            .map(|value| (value.id.as_str(), value.value))
            .collect::<BTreeMap<_, _>>();
        let truth_sum = case
            .truth
            .parcels
            .iter()
            .map(|parcel| {
                values_by_id
                    .get(parcel.as_str())
                    .copied()
                    .unwrap_or_else(|| {
                        panic!(
                            "{} truth parcel {parcel} must have a GSF value",
                            target.fragment
                        )
                    })
            })
            .sum::<u64>();
        assert_eq!(truth_sum, target.expected_truth_sum);
        assert_eq!(
            *min <= truth_sum && truth_sum <= *max,
            target.expected_inside_band,
            "{} corrected truth sum classification changed",
            target.fragment
        );

        let source_record_ids = observation
            .source_records
            .iter()
            .map(|record| record.source_record_id.as_str())
            .collect::<BTreeSet<_>>();
        for parcel in &case.truth.parcels {
            let expected = format!(
                "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.PROPERTY_VALUATION:FY2026P3:gsf:{parcel}"
            );
            assert!(
                source_record_ids.contains(expected.as_str()),
                "{} truth parcel {parcel} must carry a source-record pin",
                target.fragment
            );
        }
    }
}

#[test]
fn gsf_vector_staleness_is_confined_to_roll_gsf_observations() {
    let population = read_json_gz::<GeoPopulationEvaluationRequest>(POPULATION);
    let source_overlay = read_json_gz::<GeoPopulationEvidenceStackRequest>(SOURCE_OVERLAY);
    let corrected_overlay = read_json_gz::<GeoPopulationEvidenceStackRequest>(CORRECTED_OVERLAY);

    let stale_before = stale_vector_case_fragments(
        &population,
        &source_overlay,
        GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID,
    );
    assert_eq!(
        stale_before,
        BTreeSet::from([
            "0a6ff10e", "4e262201", "524efa30", "68a1a4ce", "8fd55140", "91b2df27", "e1ee0535",
            "e3a5228d",
        ])
    );

    let stale_after = stale_vector_case_fragments(
        &population,
        &corrected_overlay,
        GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID,
    );
    assert_eq!(
        stale_after,
        BTreeSet::from(["0a6ff10e", "4e262201", "8fd55140"]),
        "the scoring handoff fixes the five GSF-exclusion vectors and leaves only out-of-family stale vectors"
    );

    assert!(
        stale_vector_case_fragments(
            &population,
            &source_overlay,
            "rho.footprint.building_count_floor"
        )
        .is_empty(),
        "the stale current-universe defect is not present in footprint-count vectors"
    );
}

#[test]
fn out_of_family_stale_gsf_vectors_rebind_or_abstain_under_current_universe() {
    let population = read_json_gz::<GeoPopulationEvaluationRequest>(POPULATION);
    let roll_rows = read_json_gz::<BTreeMap<String, RollFixtureRow>>(ROLL_ROWS);
    let corrected_overlay = read_json_gz::<GeoPopulationEvidenceStackRequest>(CORRECTED_OVERLAY);

    for target in OUT_OF_FAMILY_STALE_TARGETS {
        let case = population_case(&population, target.fragment);
        let overlay_case = overlay_case(&corrected_overlay, &case.id);
        let retained_values =
            integer_sum_band_value_ids(gsf_observation(overlay_case)).expect("retained GSF vector");
        let universe = parcel_universe(case);
        assert_ne!(
            retained_values, universe,
            "{} remains outside bd-3frw's five-case scoring handoff",
            target.fragment
        );

        assert_eq!(
            missing_roll_rows(case, &roll_rows),
            target
                .expected_missing_roll_rows
                .iter()
                .copied()
                .collect::<BTreeSet<_>>(),
            "{} missing roll-row diagnosis changed",
            target.fragment
        );

        let current_bound =
            materialize_current_bound_out_of_family_gsf_evidence(case, target, &roll_rows);
        let rebuilt = current_bound.observations.iter().find(|observation| {
            observation.contract_id == GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID
        });

        if let Some(expected_count) = target.expected_rebuilt_value_count {
            let observation = rebuilt.expect("rebuildable stale case emits a GSF observation");
            let GeoRhoObservationKind::IntegerSumBand {
                values, min, max, ..
            } = &observation.observation
            else {
                panic!(
                    "{} rebuilt GSF observation must be an integer sum band",
                    target.fragment
                );
            };
            let values_by_id = values
                .iter()
                .map(|value| (value.id.as_str(), value.value))
                .collect::<BTreeMap<_, _>>();
            assert_eq!(values.len(), expected_count);
            assert_eq!(
                values_by_id.keys().copied().collect::<BTreeSet<_>>(),
                universe,
                "{} rebuilt vector must equal the current bound universe",
                target.fragment
            );

            if let Some((parcel, expected_value)) = target.expected_rebuilt_member {
                assert_eq!(values_by_id.get(parcel).copied(), Some(expected_value));
            }

            if let Some(expected_truth_sum) = target.expected_rebuilt_truth_sum {
                let truth_sum = case
                    .truth
                    .parcels
                    .iter()
                    .map(|parcel| {
                        values_by_id
                            .get(parcel.as_str())
                            .copied()
                            .unwrap_or_else(|| {
                                panic!(
                                    "{} truth parcel {parcel} must have a rebuilt GSF value",
                                    target.fragment
                                )
                            })
                    })
                    .sum::<u64>();
                assert_eq!(truth_sum, expected_truth_sum);
                assert!(
                    *min <= truth_sum && truth_sum <= *max,
                    "{} rebuilt truth sum should remain inside the retained band",
                    target.fragment
                );
            }
        } else {
            assert!(
                rebuilt.is_none(),
                "{} must abstain instead of publishing a partial stale vector",
                target.fragment
            );
            assert!(
                current_bound
                    .contracts
                    .iter()
                    .all(|contract| contract.id != GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID),
                "{} must not publish a GSF contract without a complete current-bound vector",
                target.fragment
            );
        }
    }
}

#[test]
fn gsf_value_vector_rebuild_measurement_declares_handoff_boundary() {
    let measurement = read_json(MEASUREMENT);
    assert_eq!(
        measurement["version"],
        "canon_geo_gsf_value_vector_rebuild_handoff.v0"
    );
    assert_eq!(measurement["bead"], "bd-3frw");
    assert_eq!(
        measurement["proof_class"],
        "retained_population_measurement_not_live"
    );
    assert_eq!(measurement["frozen_denominator"], 79);
    assert_eq!(measurement["retained_population_denominator"], 70);
    assert_eq!(
        measurement["source_audit"]["truth_bbls_with_source_rows"],
        EXPECTED_SOURCE_TRUTH_BBLS
    );
    assert_eq!(
        measurement["source_audit"]["truth_bbls_with_non_null_gsf"],
        EXPECTED_SOURCE_GSF_ROWS
    );
    assert_eq!(
        measurement["score_handoff"]["status"],
        "READY_TO_SCORE_by_bd_1g4x_not_scored_by_bd_3frw"
    );
    assert_eq!(measurement["score_handoff"]["population_path"], POPULATION);
    assert_eq!(
        measurement["score_handoff"]["corrected_overlay_path"],
        CORRECTED_OVERLAY
    );
    assert_eq!(
        measurement["score_handoff"]["changed_case_count"],
        TARGETS.len()
    );
    let changed = measurement["score_handoff"]["changed_cases"]
        .as_array()
        .expect("changed cases array")
        .iter()
        .map(|case| {
            case["case_fragment"]
                .as_str()
                .expect("case fragment")
                .to_string()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        changed,
        TARGETS
            .iter()
            .map(|case| case.fragment.to_string())
            .collect::<BTreeSet<_>>()
    );
}

#[test]
#[ignore = "writes committed bd-3frw measurement artifacts when CANON_GEO_GSF_VECTOR_REBUILD_WRITE=1"]
fn rewrite_gsf_value_vector_rebuild_artifacts() {
    if env::var("CANON_GEO_GSF_VECTOR_REBUILD_WRITE").as_deref() != Ok("1") {
        eprintln!("set CANON_GEO_GSF_VECTOR_REBUILD_WRITE=1 to rewrite the fixture artifacts");
        return;
    }

    let overlay = build_corrected_overlay();
    fs::create_dir_all(repo_path(OUT_DIR)).expect("measurement output dir can be created");
    write_json_gz(repo_path(CORRECTED_OVERLAY), &overlay);
    write_pretty_json(repo_path(MEASUREMENT), &measurement_artifact(&overlay));
}

fn build_corrected_overlay() -> GeoPopulationEvidenceStackRequest {
    let population = read_json_gz::<GeoPopulationEvaluationRequest>(POPULATION);
    let roll_rows = read_json_gz::<BTreeMap<String, RollFixtureRow>>(ROLL_ROWS);
    let mut overlay = read_json_gz::<GeoPopulationEvidenceStackRequest>(SOURCE_OVERLAY);

    for target in TARGETS {
        let case = population_case(&population, target.fragment);
        let overlay_case = overlay_case_mut(&mut overlay, &case.id);
        let rebuilt = rebuild_gsf_observation(case, overlay_case, target, &roll_rows);
        replace_gsf_observation(overlay_case, rebuilt);
    }

    overlay
}

fn rebuild_gsf_observation(
    case: &GeoLabeledCompositionCase,
    overlay_case: &GeoPopulationCaseEvidenceOverlay,
    target: GsfCase,
    roll_rows: &BTreeMap<String, RollFixtureRow>,
) -> GeoRhoObservation {
    let retained_observation = gsf_observation(overlay_case);
    let assessment_roll_rows = case
        .evidence
        .universe
        .parcels
        .iter()
        .map(|parcel| {
            let row = roll_rows.get(parcel).unwrap_or_else(|| {
                panic!(
                    "{} current-universe parcel {parcel} must have a retained FY2026P3 roll row",
                    target.fragment
                )
            });
            GeoAssessmentRollGrossSqftRow {
                bbl: parcel.clone(),
                gross_sqft: parse_optional_u64(&row.gross_sqft),
                units: parse_optional_u64(&row.units),
            }
        })
        .collect::<Vec<_>>();

    let request = GeoFootprintRollEvidenceRequest {
        version: CANON_GEO_FOOTPRINT_ROLL_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: case.evidence.profile.clone(),
        case_id: case.id.clone(),
        universe: case.evidence.universe.clone(),
        loan: GeoFootprintRollLoanFields {
            loan_key: target.property_key.to_string(),
            filed_size: Some(target.filed_size),
            size_measure: "SQFT".to_string(),
            property_class: Some(target.property_class.to_string()),
            loan_county_property_count: None,
            size_source_record_id: format!(
                "EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT:{}",
                target.property_key
            ),
            size_source_vintage: "latest_reporting_period".to_string(),
            county_property_count_source_record_id:
                "EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:unused:county_count".to_string(),
            county_property_count_source_vintage: "current".to_string(),
        },
        source_config: GeoFootprintRollSourceConfig::default(),
        calibration: property_type_gsf_calibration(),
        assessment_roll_rows,
        footprint_rows: Vec::new(),
        max_assignments: case.evidence.max_assignments,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };
    let evidence =
        materialize_footprint_roll_evidence(&request).expect("GSF vector rebuild materializes");
    let mut observation = evidence
        .observations
        .into_iter()
        .find(|observation| {
            observation.contract_id == GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID
        })
        .expect("rebuilt GSF observation");

    if let Some(source_size_record) = retained_source_record(
        retained_observation,
        "EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT:",
    ) {
        for record in &mut observation.source_records {
            if record.source_record_id == source_size_record.source_record_id {
                *record = source_size_record.clone();
            }
        }
        observation.source_records.sort();
    }
    observation
}

fn retained_source_record<'a>(
    observation: &'a GeoRhoObservation,
    source_record_id_prefix: &str,
) -> Option<&'a GeoEvidenceRecordRef> {
    observation
        .source_records
        .iter()
        .find(|record| record.source_record_id.starts_with(source_record_id_prefix))
}

fn property_type_gsf_calibration() -> GeoFootprintRollCalibration {
    GeoFootprintRollCalibration {
        assessment_roll_gross_sqft_band: GeoAssessmentRollGrossSqftBandCalibration {
            property_class_bands: vec![
                GeoAssessmentRollGrossSqftPropertyBand {
                    property_class: "MF".to_string(),
                    population_id: Some(
                        "h7-d1-residuals-2026-09-03-roll-property-type-MF".to_string(),
                    ),
                    calibration_blake3: Some(PROPERTY_TYPE_CALIBRATION_BLAKE3.to_string()),
                    falsification_rule_id: Some(
                        "truth-gross-sum-outside-property-type-band".to_string(),
                    ),
                    lower_numerator: 9,
                    lower_denominator: 10,
                    upper_numerator: 13,
                    upper_denominator: 10,
                    upper_inclusive_padding: 1,
                    admissible_hard_band: true,
                    admission_policy: GeoRhoAdmissionPolicy::Declared,
                },
                GeoAssessmentRollGrossSqftPropertyBand {
                    property_class: "MU".to_string(),
                    population_id: Some(
                        "h7-d1-residuals-2026-09-03-roll-property-type-MU-retail-component"
                            .to_string(),
                    ),
                    calibration_blake3: Some(PROPERTY_TYPE_CALIBRATION_BLAKE3.to_string()),
                    falsification_rule_id: Some(
                        "truth-gross-sum-outside-property-type-band".to_string(),
                    ),
                    lower_numerator: 7,
                    lower_denominator: 10,
                    upper_numerator: 22,
                    upper_denominator: 1,
                    upper_inclusive_padding: 1,
                    admissible_hard_band: true,
                    admission_policy: GeoRhoAdmissionPolicy::Declared,
                },
            ],
            ..GeoAssessmentRollGrossSqftBandCalibration::default()
        },
        ..GeoFootprintRollCalibration::default()
    }
}

fn replace_gsf_observation(
    overlay_case: &mut GeoPopulationCaseEvidenceOverlay,
    observation: GeoRhoObservation,
) {
    let slot = overlay_case
        .observations
        .iter()
        .position(|existing| {
            existing.contract_id == GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID
        })
        .expect("case has retained GSF observation");
    overlay_case.observations[slot] = observation;
}

fn measurement_artifact(overlay: &GeoPopulationEvidenceStackRequest) -> Value {
    let population = read_json_gz::<GeoPopulationEvaluationRequest>(POPULATION);
    let source_overlay = read_json_gz::<GeoPopulationEvidenceStackRequest>(SOURCE_OVERLAY);
    let overlay_bytes = serde_json::to_vec(overlay).expect("overlay serializes");
    json!({
        "version": "canon_geo_gsf_value_vector_rebuild_handoff.v0",
        "bead": "bd-3frw",
        "proof_class": "retained_population_measurement_not_live",
        "frozen_denominator": 79,
        "retained_population_denominator": 70,
        "boundary": "Candidate universe, truth labels, and bands are unchanged. This artifact rebuilds only rho.size.assessment_roll_gross_sqft_band value vectors/source records for the named stale cases from the current bound universe.",
        "source_audit": {
            "source_tables_described": [
                "EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT",
                "EDGAR_DB.DBT_WRANGLING_NYC_OPENDATA.WRGL_NYC_OPENDATA_PROPERTY_VALUATION_AND_ASSESSMENT_DATA_TAX_CLASSES_1_2_3_4__STRUCTURED"
            ],
            "measurement_name": "bd_1fq2_gsf_truth_source_vs_filed_size",
            "predicate": "FY2026/PERIOD=3 assessment-roll rows joined by exact deed-truth BBL/PARID for the five GSF exclusion cases",
            "truth_bbls_with_source_rows": EXPECTED_SOURCE_TRUTH_BBLS,
            "truth_bbls_with_non_null_gsf": EXPECTED_SOURCE_GSF_ROWS,
            "finding": "not an external GSF acquisition gap; source rows and values are landed"
        },
        "systemic_audit": {
            "source_overlay_stale_gsf_cases": stale_vector_case_fragments(&population, &source_overlay, GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID).into_iter().collect::<Vec<_>>(),
            "corrected_overlay_remaining_stale_gsf_cases": stale_vector_case_fragments(&population, overlay, GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID).into_iter().collect::<Vec<_>>(),
            "source_overlay_stale_footprint_count_cases": stale_vector_case_fragments(&population, &source_overlay, "rho.footprint.building_count_floor").into_iter().collect::<Vec<_>>(),
            "finding": "staleness is visible in GSF vectors; footprint-count vectors in the same overlay match the current universe"
        },
        "score_handoff": {
            "status": "READY_TO_SCORE_by_bd_1g4x_not_scored_by_bd_3frw",
            "population_path": POPULATION,
            "source_overlay_path": SOURCE_OVERLAY,
            "corrected_overlay_path": CORRECTED_OVERLAY,
            "changed_case_count": TARGETS.len(),
            "changed_cases": TARGETS.iter().map(|target| {
                json!({
                    "case_fragment": target.fragment,
                    "property_key": target.property_key,
                    "filed_size": target.filed_size,
                    "property_class": target.property_class,
                    "vector_count": target.expected_vector_count,
                    "corrected_truth_gsf_sum": target.expected_truth_sum,
                    "band_min": target.expected_min,
                    "band_max": target.expected_max,
                    "corrected_truth_sum_inside_band": target.expected_inside_band
                })
            }).collect::<Vec<_>>(),
            "overlay_sha256": sha256_file(repo_path(CORRECTED_OVERLAY)),
            "overlay_uncompressed_sha256": sha256_bytes(&overlay_bytes)
        }
    })
}

fn stale_vector_case_fragments(
    population: &GeoPopulationEvaluationRequest,
    overlay: &GeoPopulationEvidenceStackRequest,
    contract_id: &str,
) -> BTreeSet<&'static str> {
    let population_by_case = population
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();
    overlay
        .case_overlays
        .iter()
        .filter_map(|overlay_case| {
            let case = population_by_case
                .get(overlay_case.case_id.as_str())
                .expect("overlay case exists in population");
            let values = overlay_case
                .observations
                .iter()
                .find(|observation| observation.contract_id == contract_id)
                .and_then(integer_sum_band_value_ids)?;
            let universe = case
                .evidence
                .universe
                .parcels
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            (values != universe).then(|| case_fragment(&overlay_case.case_id))
        })
        .collect()
}

fn materialize_current_bound_out_of_family_gsf_evidence(
    case: &GeoLabeledCompositionCase,
    target: OutOfFamilyGsfCase,
    roll_rows: &BTreeMap<String, RollFixtureRow>,
) -> GeoEvidenceCompilationRequest {
    let assessment_roll_rows = case
        .evidence
        .universe
        .parcels
        .iter()
        .filter_map(|parcel| {
            roll_rows
                .get(parcel)
                .map(|row| GeoAssessmentRollGrossSqftRow {
                    bbl: parcel.clone(),
                    gross_sqft: parse_optional_u64(&row.gross_sqft),
                    units: parse_optional_u64(&row.units),
                })
        })
        .collect::<Vec<_>>();

    let request = GeoFootprintRollEvidenceRequest {
        version: CANON_GEO_FOOTPRINT_ROLL_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: case.evidence.profile.clone(),
        case_id: case.id.clone(),
        universe: case.evidence.universe.clone(),
        loan: GeoFootprintRollLoanFields {
            loan_key: target.fragment.to_string(),
            filed_size: Some(target.filed_size),
            size_measure: "SQFT".to_string(),
            property_class: None,
            loan_county_property_count: None,
            size_source_record_id: target.size_source_record_id.to_string(),
            size_source_vintage: "latest_reporting_period".to_string(),
            county_property_count_source_record_id:
                "EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:unused:county_count".to_string(),
            county_property_count_source_vintage: "current".to_string(),
        },
        source_config: GeoFootprintRollSourceConfig::default(),
        calibration: property_type_gsf_calibration(),
        assessment_roll_rows,
        footprint_rows: Vec::new(),
        max_assignments: case.evidence.max_assignments,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    };
    materialize_footprint_roll_evidence(&request).expect("current-bound GSF replay materializes")
}

fn parcel_universe(case: &GeoLabeledCompositionCase) -> BTreeSet<&str> {
    case.evidence
        .universe
        .parcels
        .iter()
        .map(String::as_str)
        .collect()
}

fn missing_roll_rows<'a>(
    case: &'a GeoLabeledCompositionCase,
    roll_rows: &BTreeMap<String, RollFixtureRow>,
) -> BTreeSet<&'a str> {
    case.evidence
        .universe
        .parcels
        .iter()
        .filter(|parcel| !roll_rows.contains_key(parcel.as_str()))
        .map(String::as_str)
        .collect()
}

fn integer_sum_band_value_ids(observation: &GeoRhoObservation) -> Option<BTreeSet<&str>> {
    let GeoRhoObservationKind::IntegerSumBand { values, .. } = &observation.observation else {
        return None;
    };
    Some(values.iter().map(|value| value.id.as_str()).collect())
}

fn population_case<'a>(
    population: &'a GeoPopulationEvaluationRequest,
    fragment: &str,
) -> &'a GeoLabeledCompositionCase {
    let mut matches = population
        .cases
        .iter()
        .filter(|case| case.id.contains(fragment))
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "{fragment} must identify one population case"
    );
    matches.pop().expect("one match")
}

fn overlay_case<'a>(
    overlay: &'a GeoPopulationEvidenceStackRequest,
    case_id: &str,
) -> &'a GeoPopulationCaseEvidenceOverlay {
    overlay
        .case_overlays
        .iter()
        .find(|case| case.case_id == case_id)
        .expect("overlay case")
}

fn overlay_case_mut<'a>(
    overlay: &'a mut GeoPopulationEvidenceStackRequest,
    case_id: &str,
) -> &'a mut GeoPopulationCaseEvidenceOverlay {
    overlay
        .case_overlays
        .iter_mut()
        .find(|case| case.case_id == case_id)
        .expect("overlay case")
}

fn gsf_observation(overlay_case: &GeoPopulationCaseEvidenceOverlay) -> &GeoRhoObservation {
    overlay_case
        .observations
        .iter()
        .find(|observation| {
            observation.contract_id == GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID
        })
        .expect("GSF observation")
}

fn case_fragment(case_id: &str) -> &'static str {
    for fragment in [
        "0a6ff10e", "4e262201", "524efa30", "68a1a4ce", "8fd55140", "91b2df27", "e1ee0535",
        "e3a5228d",
    ] {
        if case_id.contains(fragment) {
            return fragment;
        }
    }
    panic!("unexpected stale vector case {case_id}");
}

fn parse_optional_u64(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.parse::<u64>().expect("fixture integer"))
    }
}

fn read_json_gz<T: DeserializeOwned>(relative: &str) -> T {
    let file = File::open(repo_path(relative)).expect("open gzipped fixture");
    serde_json::from_reader(GzDecoder::new(BufReader::new(file))).expect("parse gzipped fixture")
}

fn read_json(relative: &str) -> Value {
    let file = File::open(repo_path(relative)).expect("open JSON fixture");
    serde_json::from_reader(BufReader::new(file)).expect("parse JSON fixture")
}

fn read_gzip_uncompressed_bytes(path: impl AsRef<Path>) -> Vec<u8> {
    let file = File::open(path).expect("open gzipped fixture");
    let mut decoder = GzDecoder::new(BufReader::new(file));
    let mut bytes = Vec::new();
    decoder
        .read_to_end(&mut bytes)
        .expect("read gzipped fixture");
    bytes
}

fn write_json_gz(path: impl AsRef<Path>, value: &GeoPopulationEvidenceStackRequest) {
    let file = File::create(path).expect("create gzipped fixture");
    let mut encoder = GzBuilder::new()
        .mtime(0)
        .write(file, Compression::default());
    serde_json::to_writer(&mut encoder, value).expect("write gzipped JSON");
    encoder.finish().expect("finish gzip writer");
}

fn write_pretty_json(path: impl AsRef<Path>, value: &Value) {
    let mut file = File::create(path).expect("create JSON fixture");
    serde_json::to_writer_pretty(&mut file, value).expect("write pretty JSON");
    file.write_all(b"\n").expect("write trailing newline");
}

fn sha256_file(path: impl AsRef<Path>) -> String {
    let mut file = File::open(path).expect("open fixture for sha256");
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

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
