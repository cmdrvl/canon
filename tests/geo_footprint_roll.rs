#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_FOOTPRINT_ROLL_EVIDENCE_REQUEST_VERSION,
    DEFAULT_MAX_MATERIALIZED_MODELS, GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CALIBRATION_BLAKE3,
    GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID,
    GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_CALIBRATION_BLAKE3,
    GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_CONTRACT_ID, GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_MAX,
    GeoAssessmentRollGrossSqftRow, GeoBuildingCandidate, GeoBuildingFootprintRow,
    GeoCompositionModel, GeoCompositionProfile, GeoCompositionStatus, GeoCompositionUniverse,
    GeoEvidenceDisposition, GeoFootprintRollCalibration, GeoFootprintRollEvidenceRequest,
    GeoFootprintRollLoanFields, GeoFootprintRollSourceConfig, GeoIntegerValueOrigin, GeoRhoBasis,
    GeoRhoObservationKind, calibration_receipt_blake3,
    canonical_footprint_roll_evidence_request_bytes, compile_evidence,
    materialize_footprint_roll_evidence, solve_composition,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
};

const FIXTURE_CLASS: &str = "observed_warehouse_snapshot_fixture_not_live_proof";
const MCP_STACK_DIR: &str = "scripts/geo_measurements/fixtures/d1_residuals/mcp_stack_2026-09-03";

#[derive(Debug, Deserialize)]
struct RollCalibrationReceipt {
    population_id: String,
    band: [f64; 2],
    rows: Vec<RollCalibrationRow>,
    coverage: RollCoverage,
}

#[derive(Debug, Deserialize)]
struct RollCalibrationRow {
    asserted: f64,
    ratio: f64,
    truth_gross: u64,
}

#[derive(Debug, Deserialize)]
struct RollCoverage {
    n: u64,
    in_band: u64,
}

#[derive(Debug, Deserialize)]
struct FootprintCalibrationReceipt {
    population_id: String,
    rows: Vec<FootprintCalibrationRow>,
    coverage: FootprintCoverage,
}

#[derive(Debug, Deserialize)]
struct FootprintCalibrationRow {
    holds: bool,
    property_count: u64,
    truth_buildings: u64,
}

#[derive(Debug, Deserialize)]
struct FootprintCoverage {
    n: u64,
    holds: u64,
}

#[derive(Debug, Deserialize)]
struct RollFixtureRow {
    gross_sqft: String,
    units: String,
}

#[derive(Debug, Deserialize)]
struct FootprintFixtureRow {
    bins: u64,
}

fn parcels(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_string()).collect()
}

fn source_record_ids(request: &GeoFootprintRollEvidenceRequest) -> GeoFootprintRollLoanFields {
    GeoFootprintRollLoanFields {
        loan_key: request.loan.loan_key.clone(),
        filed_size: request.loan.filed_size,
        size_measure: request.loan.size_measure.clone(),
        loan_county_property_count: request.loan.loan_county_property_count,
        size_source_record_id: request.loan.size_source_record_id.clone(),
        size_source_vintage: request.loan.size_source_vintage.clone(),
        county_property_count_source_record_id: request
            .loan
            .county_property_count_source_record_id
            .clone(),
        county_property_count_source_vintage: request
            .loan
            .county_property_count_source_vintage
            .clone(),
    }
}

fn request_with(
    size_measure: &str,
    filed_size: Option<u64>,
    county_count: Option<u64>,
    parcel_ids: &[&str],
) -> GeoFootprintRollEvidenceRequest {
    GeoFootprintRollEvidenceRequest {
        version: CANON_GEO_FOOTPRINT_ROLL_EVIDENCE_REQUEST_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        case_id: "case-fixture".to_string(),
        universe: GeoCompositionUniverse {
            parcels: parcels(parcel_ids),
            buildings: Vec::new(),
        },
        loan: GeoFootprintRollLoanFields {
            loan_key: "loan-fixture".to_string(),
            filed_size,
            size_measure: size_measure.to_string(),
            loan_county_property_count: county_count,
            size_source_record_id: "EDGAR_DB.PROPERTY_MART.PROPERTY_PERIOD_FACT:loan-fixture:size"
                .to_string(),
            size_source_vintage: "latest_reporting_period".to_string(),
            county_property_count_source_record_id:
                "EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE_PROPERTY:loan-fixture:county_count"
                    .to_string(),
            county_property_count_source_vintage: "current".to_string(),
        },
        source_config: GeoFootprintRollSourceConfig::default(),
        calibration: GeoFootprintRollCalibration::default(),
        assessment_roll_rows: parcel_ids
            .iter()
            .enumerate()
            .map(|(index, bbl)| GeoAssessmentRollGrossSqftRow {
                bbl: (*bbl).to_string(),
                gross_sqft: Some(400 + (index as u64 * 300)),
                units: Some(1),
            })
            .collect(),
        footprint_rows: parcel_ids
            .iter()
            .map(|bbl| GeoBuildingFootprintRow {
                mappluto_bbl: (*bbl).to_string(),
                bin: format!("{bbl}-bin-1"),
                active: true,
            })
            .collect(),
        max_assignments: 1_000,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

#[test]
fn footprint_roll_stage_emits_admissible_hard_integer_bands() {
    let mut request = request_with("SQFT", Some(1_000), Some(3), &["1000000001", "1000000002"]);
    request.assessment_roll_rows = vec![
        GeoAssessmentRollGrossSqftRow {
            bbl: "1000000002".to_string(),
            gross_sqft: Some(700),
            units: Some(1),
        },
        GeoAssessmentRollGrossSqftRow {
            bbl: "1000000001".to_string(),
            gross_sqft: Some(400),
            units: Some(1),
        },
    ];
    request.footprint_rows.push(GeoBuildingFootprintRow {
        mappluto_bbl: "1000000002".to_string(),
        bin: "1000000002-bin-inactive".to_string(),
        active: false,
    });

    let evidence =
        materialize_footprint_roll_evidence(&request).expect("stage evidence materializes");

    assert_eq!(
        FIXTURE_CLASS,
        "observed_warehouse_snapshot_fixture_not_live_proof"
    );
    assert_eq!(evidence.version, CANON_GEO_EVIDENCE_REQUEST_VERSION);
    assert_eq!(
        evidence
            .contracts
            .iter()
            .map(|contract| contract.id.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID,
            GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_CONTRACT_ID,
        ])
    );
    assert_eq!(evidence.observations.len(), 2);

    let roll = evidence
        .observations
        .iter()
        .find(|observation| {
            observation.contract_id == GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID
        })
        .expect("roll observation");
    let GeoRhoObservationKind::IntegerSumBand {
        measure,
        values,
        min,
        max,
        ..
    } = &roll.observation
    else {
        panic!("roll observation must be an integer sum band");
    };
    assert_eq!(measure.semantic_id, "assessment_roll.gross_sqft");
    assert_eq!(measure.unit, "sqft");
    assert_eq!(measure.value_origin, GeoIntegerValueOrigin::SourceAsserted);
    assert_eq!((*min, *max), (700, 1_601));
    assert_eq!(
        values
            .iter()
            .map(|value| (&value.id, value.value))
            .collect::<Vec<_>>(),
        vec![
            (&"1000000001".to_string(), 400),
            (&"1000000002".to_string(), 700),
        ]
    );
    assert!(roll.source_records.iter().any(|record| {
        record
            .source_record_id
            .contains("PROPERTY_PERIOD_FACT:loan-fixture:size")
    }));
    assert!(roll.source_records.iter().all(has_lowercase_blake3));

    let footprint = evidence
        .observations
        .iter()
        .find(|observation| {
            observation.contract_id == GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_CONTRACT_ID
        })
        .expect("footprint observation");
    let GeoRhoObservationKind::IntegerSumBand {
        measure,
        values,
        min,
        max,
        ..
    } = &footprint.observation
    else {
        panic!("footprint observation must be an integer sum band");
    };
    assert_eq!(measure.semantic_id, "footprints.active_bin_count");
    assert_eq!(measure.unit, "buildings");
    assert_eq!(measure.value_origin, GeoIntegerValueOrigin::SourceAsserted);
    assert_eq!((*min, *max), (2, GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_MAX));
    assert_eq!(
        values.iter().map(|value| value.value).collect::<Vec<_>>(),
        vec![1, 1]
    );
    assert!(
        footprint
            .source_records
            .iter()
            .all(|record| !record.source_record_id.contains("bin-inactive")),
        "inactive footprint rows are not active-BIN source rows"
    );

    let roll_contract = evidence
        .contracts
        .iter()
        .find(|contract| contract.id == GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID)
        .expect("roll contract");
    assert!(
        roll_contract
            .method_version
            .contains("band_7_over_10_to_16_over_10")
    );
    let GeoRhoBasis::EmpiricalCalibration {
        calibration_blake3,
        admissible_hard_band,
        ..
    } = &roll_contract.basis
    else {
        panic!("roll contract must be empirical");
    };
    assert_eq!(
        calibration_blake3,
        GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CALIBRATION_BLAKE3
    );
    assert!(*admissible_hard_band);

    let compilation = compile_evidence(&evidence).expect("emitted evidence compiles");
    assert_eq!(compilation.composition_request.hard_constraints.len(), 2);
    assert!(
        compilation
            .admissions
            .iter()
            .all(|admission| admission.disposition == GeoEvidenceDisposition::HardConstraint)
    );
}

#[test]
fn footprint_roll_request_canonicalization_is_order_independent() {
    let mut left = request_with("SQFT", Some(1_000), Some(3), &["p2", "p1"]);
    left.universe.buildings = vec![
        GeoBuildingCandidate {
            id: "b2".to_string(),
            parcel_ids: vec!["p2".to_string(), "p1".to_string()],
        },
        GeoBuildingCandidate {
            id: "b1".to_string(),
            parcel_ids: vec!["p1".to_string()],
        },
    ];
    left.footprint_rows.reverse();

    let mut right = request_with("SQFT", Some(1_000), Some(3), &["p1", "p2"]);
    right.universe.buildings = vec![
        GeoBuildingCandidate {
            id: "b1".to_string(),
            parcel_ids: vec!["p1".to_string()],
        },
        GeoBuildingCandidate {
            id: "b2".to_string(),
            parcel_ids: vec!["p1".to_string(), "p2".to_string()],
        },
    ];

    assert_eq!(
        canonical_footprint_roll_evidence_request_bytes(&left).expect("left request canonicalizes"),
        canonical_footprint_roll_evidence_request_bytes(&right)
            .expect("right request canonicalizes")
    );
}

#[test]
fn calibration_receipts_replay_roll_gsf_band_counts_and_digest() {
    let value: Value = read_json(rooted(&[MCP_STACK_DIR, "calibration_roll_gsf_band.json"]));
    let receipt: RollCalibrationReceipt =
        serde_json::from_value(value.clone()).expect("roll calibration parses");
    let in_band = receipt
        .rows
        .iter()
        .filter(|row| (receipt.band[0]..=receipt.band[1]).contains(&row.ratio))
        .count() as u64;
    let recomputed_from_raw = receipt
        .rows
        .iter()
        .filter(|row| {
            let truth = row.truth_gross as f64;
            truth >= receipt.band[0] * row.asserted && truth <= receipt.band[1] * row.asserted
        })
        .count() as u64;
    assert_eq!(receipt.population_id, "h7-d1-residuals-2026-09-03-roll");
    assert_eq!(receipt.coverage.n, 25);
    assert_eq!(receipt.coverage.in_band, 17);
    assert_eq!(in_band, 17);
    assert_eq!(recomputed_from_raw, 17);
    assert_eq!(
        calibration_receipt_blake3(&value).expect("roll calibration hashes"),
        GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CALIBRATION_BLAKE3
    );

    let roll_rows: BTreeMap<String, RollFixtureRow> = read_json_gz(rooted(&[
        MCP_STACK_DIR,
        "assessment_roll_fy2026p3_lots.json.gz",
    ]));
    let fixture_row = roll_rows
        .get("3061260001")
        .expect("fixture source row exists");
    assert_eq!(fixture_row.gross_sqft, "14628");
    assert_eq!(fixture_row.units, "1");
}

#[test]
fn calibration_receipts_replay_footprint_floor_counts_and_digest() {
    let value: Value = read_json(rooted(&[MCP_STACK_DIR, "calibration_footprint.json"]));
    let receipt: FootprintCalibrationReceipt =
        serde_json::from_value(value.clone()).expect("footprint calibration parses");
    let holds = receipt.rows.iter().filter(|row| row.holds).count() as u64;
    let recomputed_floor_holds = receipt
        .rows
        .iter()
        .filter(|row| row.truth_buildings >= row.property_count.saturating_sub(1))
        .count() as u64;
    assert_eq!(receipt.population_id, "h7-d1-residuals-2026-09-03");
    assert_eq!(receipt.coverage.n, 60);
    assert_eq!(receipt.coverage.holds, 60);
    assert_eq!(holds, 60);
    assert_eq!(recomputed_floor_holds, 60);
    assert_eq!(
        calibration_receipt_blake3(&value).expect("footprint calibration hashes"),
        GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_CALIBRATION_BLAKE3
    );

    let footprints: BTreeMap<String, FootprintFixtureRow> =
        read_json(rooted(&[MCP_STACK_DIR, "footprints.json"]));
    assert_eq!(
        footprints
            .get("4066300015")
            .expect("fixture footprint row exists")
            .bins,
        3
    );
}

#[test]
fn units_size_measure_suppresses_assessment_roll_gsf_band() {
    let evidence =
        materialize_footprint_roll_evidence(&request_with("UNITS", Some(1_000), None, &["p1"]))
            .expect("units measure materializes without sqft band");
    assert!(
        evidence.observations.iter().all(|observation| {
            observation.contract_id != GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID
        }),
        "SIZE_MEASURE=UNITS must not emit the sqft band"
    );
}

#[test]
fn sub_500_filed_sqft_suppresses_assessment_roll_gsf_band() {
    let evidence =
        materialize_footprint_roll_evidence(&request_with("SQFT", Some(499), None, &["p1"]))
            .expect("sub-500 sqft materializes without sqft band");
    assert!(
        evidence.observations.iter().all(|observation| {
            observation.contract_id != GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID
        }),
        "filed sqft below 500 must not emit the sqft band"
    );
}

#[test]
fn count_minus_one_not_positive_suppresses_footprint_floor() {
    let evidence =
        materialize_footprint_roll_evidence(&request_with("SQFT", None, Some(1), &["p1"]))
            .expect("count-one materializes without footprint floor");
    assert!(
        evidence.observations.iter().all(|observation| {
            observation.contract_id != GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_CONTRACT_ID
        }),
        "count-1 <= 0 must not emit the footprint floor"
    );
}

#[test]
fn any_universe_lot_without_active_footprint_row_suppresses_footprint_floor() {
    let mut request = request_with("SQFT", None, Some(3), &["p1", "p2"]);
    request
        .footprint_rows
        .retain(|row| row.mappluto_bbl != "p2");
    let evidence = materialize_footprint_roll_evidence(&request)
        .expect("missing footprint row materializes without footprint floor");
    assert!(
        evidence.observations.iter().all(|observation| {
            observation.contract_id != GEO_FOOTPRINT_BUILDING_COUNT_FLOOR_CONTRACT_ID
        }),
        "every universe lot must have an active footprint row before the floor is emitted"
    );
}

#[test]
fn retail_only_312_97th_falsification_keeps_units_1_2_and_rejects_unit_3() {
    let mut request = request_with(
        "SQFT",
        Some(10_000),
        None,
        &["312_97th_unit_1", "312_97th_unit_2", "312_97th_unit_3"],
    );
    request.case_id = "312_97th_retail_only_falsification".to_string();
    request.assessment_roll_rows = vec![
        GeoAssessmentRollGrossSqftRow {
            bbl: "312_97th_unit_1".to_string(),
            gross_sqft: Some(4_000),
            units: Some(1),
        },
        GeoAssessmentRollGrossSqftRow {
            bbl: "312_97th_unit_2".to_string(),
            gross_sqft: Some(6_000),
            units: Some(1),
        },
        GeoAssessmentRollGrossSqftRow {
            bbl: "312_97th_unit_3".to_string(),
            gross_sqft: Some(6_002),
            units: Some(1),
        },
    ];

    let evidence =
        materialize_footprint_roll_evidence(&request).expect("retail-only evidence materializes");
    let roll = evidence
        .observations
        .iter()
        .find(|observation| {
            observation.contract_id == GEO_ASSESSMENT_ROLL_GROSS_SQFT_BAND_CONTRACT_ID
        })
        .expect("roll observation");
    let GeoRhoObservationKind::IntegerSumBand { min, max, .. } = &roll.observation else {
        panic!("roll observation must be an integer sum band");
    };
    assert_eq!((*min, *max), (7_000, 16_001));

    let compilation = compile_evidence(&evidence).expect("retail-only evidence compiles");
    let solved = solve_composition(&compilation.composition_request)
        .expect("retail-only residual solves exactly");
    assert_eq!(solved.status, GeoCompositionStatus::Ambiguous);
    assert!(
        solved.residual_models.contains(&GeoCompositionModel {
            parcels: parcels(&["312_97th_unit_1", "312_97th_unit_2"]),
            buildings: Vec::new(),
        }),
        "units 1+2 remain inside the filed-size band"
    );
    assert!(
        !solved.residual_models.contains(&GeoCompositionModel {
            parcels: parcels(&["312_97th_unit_1", "312_97th_unit_2", "312_97th_unit_3",]),
            buildings: Vec::new(),
        }),
        "adding unit 3 is the recorded falsification outcome"
    );
}

#[test]
fn loan_source_record_helper_keeps_fixture_request_copy_explicit() {
    let request = request_with("SQFT", Some(1_000), Some(2), &["p1"]);
    let loan = source_record_ids(&request);
    assert_eq!(loan.size_source_vintage, "latest_reporting_period");
    assert_eq!(loan.county_property_count_source_vintage, "current");
}

fn has_lowercase_blake3(record: &canon::geo::GeoEvidenceRecordRef) -> bool {
    record.record_blake3.len() == 64
        && record
            .record_blake3
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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
        .arg(Path::new(&path))
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
