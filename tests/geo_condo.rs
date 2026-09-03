#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_CONDO_BRIDGE_PAD_METHOD, CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION,
    GeoCandidateReachStatus, GeoCondoBridgeCaseRequest, GeoCondoBridgeReachKind,
    GeoCondoBridgeRequest, GeoPadBblRow, GeoPopulationCaseTruthReachByGrain,
    GeoPopulationEvaluationRequest, GeoTruthReachByGrain, GeoTruthRepresentationGrain,
    build_condo_bridge, canonical_condo_bridge_bytes,
    evaluate_population_with_truth_reach_by_grain, validate_condo_bridge_artifact,
};
use flate2::read::GzDecoder;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs::File, io::BufReader, path::PathBuf};

#[test]
fn replay_pad_condo_bridge_fixture_exactly() {
    let artifact = build_condo_bridge(&fixture_bridge_request()).expect("bridge builds");
    validate_condo_bridge_artifact(&artifact).expect("artifact validates");
    let canonical_bytes = canonical_condo_bridge_bytes(&artifact).expect("artifact canonicalizes");
    let reparsed: Value =
        serde_json::from_slice(&canonical_bytes).expect("canonical artifact parses");
    assert_eq!(reparsed["version"], "canon_geo_condo_bridge.v0");
    assert!(artifact.source_dataset.starts_with("fixture."));

    let expected = read_json(fixture_path("condo_bridge_pad.json"));
    assert_eq!(receipt_projection(&artifact), expected);
}

#[test]
fn population_evaluation_reports_unit_and_billing_truth_reach() {
    let bridge = build_condo_bridge(&fixture_bridge_request()).expect("bridge builds");
    let bridge_row = bridge
        .rows
        .iter()
        .find(|row| row.after.truth_members > 0)
        .expect("fixture has a mapped condo row");
    let mut request: GeoPopulationEvaluationRequest =
        serde_json::from_value(read_json(fixture_path("../h7_population_request.json")))
            .expect("population request parses");
    request.cases.retain(|case| case.id == bridge_row.case_id);
    request.max_cases = 1;
    let expected_truth_reach_by_grain = vec![
        GeoTruthReachByGrain {
            grain: GeoTruthRepresentationGrain::UnitLot,
            truth_members: bridge_row.before.truth_members,
            truth_members_in_universe: bridge_row.before.truth_members_in_universe,
            candidate_reach: reach_status(
                bridge_row.before.truth_members,
                bridge_row.before.truth_members_in_universe,
            ),
        },
        GeoTruthReachByGrain {
            grain: GeoTruthRepresentationGrain::BillingLot,
            truth_members: bridge_row.after.truth_members,
            truth_members_in_universe: bridge_row.after.truth_members_in_universe,
            candidate_reach: reach_status(
                bridge_row.after.truth_members,
                bridge_row.after.truth_members_in_universe,
            ),
        },
    ];
    let overlay = GeoPopulationCaseTruthReachByGrain {
        case_id: bridge_row.case_id.clone(),
        truth_reach_by_grain: expected_truth_reach_by_grain.clone(),
    };

    let evaluation = evaluate_population_with_truth_reach_by_grain(&request, &[overlay])
        .expect("population evaluation succeeds");
    let row = evaluation.cases.first().expect("one evaluated case");
    assert_eq!(row.truth_reach_by_grain, expected_truth_reach_by_grain);
    assert_eq!(evaluation.summary.truth_reach_by_grain.len(), 2);
    let billing_summary = evaluation
        .summary
        .truth_reach_by_grain
        .iter()
        .find(|summary| summary.grain == GeoTruthRepresentationGrain::BillingLot)
        .expect("billing grain summary");
    assert_eq!(billing_summary.cases, 1);
    assert_eq!(
        billing_summary.truth_members_in_universe,
        bridge_row.after.truth_members_in_universe
    );
}

#[test]
fn zero_or_ambiguous_billing_rows_stay_unmapped() {
    let request = GeoCondoBridgeRequest {
        version: CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION.to_string(),
        source_dataset: "fixture.negative.pad_bbl".to_string(),
        source_release: "test".to_string(),
        source_lineage_ids: vec!["fixture.negative.pad_bbl".to_string()],
        max_pad_rows: 8,
        max_cases: 1,
        pad_rows: vec![
            pad_row(
                "1000073001",
                "1000073001",
                "1000073001",
                Some("1000077501"),
                1,
            ),
            pad_row(
                "1000073001",
                "1000073001",
                "1000073001",
                Some("1000077502"),
                1,
            ),
            pad_row("1000074001", "1000074001", "1000074001", None, 2),
        ],
        cases: vec![GeoCondoBridgeCaseRequest {
            case_id: "case-negative".to_string(),
            loan_key: None,
            truth_parcels: vec![
                "1000072001".to_string(),
                "1000073001".to_string(),
                "1000074001".to_string(),
            ],
            universe_parcels: vec![
                "1000077501".to_string(),
                "1000077502".to_string(),
                "1000077503".to_string(),
            ],
        }],
    };

    let artifact = build_condo_bridge(&request).expect("negative bridge builds");
    let row = artifact.rows.first().expect("one condo row");
    assert_eq!(row.kind, GeoCondoBridgeReachKind::Unreached);
    assert!(row.truth_billing_grain.is_empty());
    assert_eq!(row.after.truth_members, 0);
    assert_eq!(row.unmapped_lots.len(), 3);

    let reasons = row
        .unmapped_lots
        .iter()
        .map(|lot| (lot.unit_lot.as_str(), format!("{:?}", lot.reason)))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(reasons["1000072001"], "NoPadBblRow");
    assert_eq!(reasons["1000073001"], "AmbiguousBillingLots");
    assert_eq!(reasons["1000074001"], "MissingBillingLot");
    assert!(
        row.lot_mappings
            .iter()
            .all(|mapping| mapping.billing_lot.is_none()),
        "zero, missing, and ambiguous billing rows must not guess a billing lot"
    );
}

fn fixture_bridge_request() -> GeoCondoBridgeRequest {
    let population: Value = read_json(fixture_path("../h7_population_request.json"));
    let h7_population: Value = read_json(fixture_path("../canon_geo_h7_population.v0.json"));
    let loan_labels = h7_population["cases"]
        .as_array()
        .expect("H7 cases array")
        .iter()
        .map(|case| {
            let subject = case["subject_id"].as_str().expect("subject id");
            let loan_key = case["loan_key"].as_str().expect("loan key");
            (subject.to_string(), loan_key[..8].to_string())
        })
        .collect::<BTreeMap<_, _>>();

    let cases = population["cases"]
        .as_array()
        .expect("population cases array")
        .iter()
        .map(|case| {
            let case_id = case["id"].as_str().expect("case id").to_string();
            GeoCondoBridgeCaseRequest {
                loan_key: loan_labels.get(&case_id).cloned(),
                case_id,
                truth_parcels: string_array(&case["truth"]["parcels"]),
                universe_parcels: string_array(&case["evidence"]["universe"]["parcels"]),
            }
        })
        .collect();

    GeoCondoBridgeRequest {
        version: CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION.to_string(),
        source_dataset: "fixture.mcp_stack_2026_09_03.pad_bbl".to_string(),
        source_release: "26B_2026-05-01".to_string(),
        source_lineage_ids: vec!["EDGAR_DB.SOURCE.NYC_DCP_PAD_BBL_HOT:26B".to_string()],
        pad_rows: read_pad_rows(),
        cases,
        max_pad_rows: 1_000,
        max_cases: 100,
    }
}

fn read_pad_rows() -> Vec<GeoPadBblRow> {
    let file = File::open(fixture_path("pad_bbl.json.gz")).expect("open PAD BBL fixture");
    serde_json::from_reader(GzDecoder::new(BufReader::new(file))).expect("parse PAD BBL rows")
}

fn receipt_projection(artifact: &canon::geo::GeoCondoBridgeArtifact) -> Value {
    json!({
        "method": artifact.method,
        "stats": {
            "fully_reached": artifact.stats.fully_reached,
            "unreached": artifact.stats.unreached,
            "partial": artifact.stats.partial,
        },
        "rows": artifact.rows.iter().map(|row| {
            json!({
                "sid": row.case_id,
                "loan": row.loan_key.as_deref().expect("loan label supplied"),
                "unit_lots": row.unit_lots,
                "unmapped": row.unmapped_lots.len(),
                "bridged_truth": row.truth_billing_grain,
                "before": format!(
                    "{}/{}",
                    row.before.truth_members_in_universe,
                    row.before.truth_members
                ),
                "after": format!(
                    "{}/{}",
                    row.after.truth_members_in_universe,
                    row.after.truth_members
                ),
                "kind": row.kind,
            })
        }).collect::<Vec<_>>()
    })
}

fn pad_row(
    bbl_key: &str,
    low_bbl_key: &str,
    high_bbl_key: &str,
    billing_bbl_key: Option<&str>,
    condo_number: u64,
) -> GeoPadBblRow {
    GeoPadBblRow {
        bbl_key: bbl_key.to_string(),
        low_bbl_key: low_bbl_key.to_string(),
        high_bbl_key: high_bbl_key.to_string(),
        billing_bbl_key: billing_bbl_key.map(str::to_string),
        condo_number: Some(condo_number),
        condo_flag: Some("C".to_string()),
    }
}

fn string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item.as_str().expect("string").to_string())
        .collect()
}

fn reach_status(truth_members: u64, truth_members_in_universe: u64) -> GeoCandidateReachStatus {
    if truth_members == 0 || truth_members_in_universe == 0 {
        GeoCandidateReachStatus::None
    } else if truth_members == truth_members_in_universe {
        GeoCandidateReachStatus::Full
    } else {
        GeoCandidateReachStatus::Partial
    }
}

fn read_json(path: PathBuf) -> Value {
    serde_json::from_reader(File::open(path).expect("open JSON fixture")).expect("parse JSON")
}

fn fixture_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/geo_measurements/fixtures/d1_residuals/mcp_stack_2026-09-03")
        .join(relative)
}

#[test]
fn fixture_projection_uses_module_method_constant() {
    assert_eq!(
        CANON_GEO_CONDO_BRIDGE_PAD_METHOD,
        "PAD BBL current release: unit lot -> BILLING_BBL_KEY via exact row or LOW/HIGH range; truth plane re-expressed at billing-lot grain"
    );
}
