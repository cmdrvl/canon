#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_CONDO_BRIDGE_PAD_METHOD, CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION,
    CANON_GEO_LEDGER_BRIDGE_VERSION, GEO_CONDO_CONFIRMATION_INSUFFICIENT, GeoCandidateReachStatus,
    GeoCanonicalPolygonMm, GeoCanonicalRingMm, GeoCondoBridgeCaseRequest, GeoCondoBridgeReachKind,
    GeoCondoBridgeRequest, GeoCondoConfirmation, GeoCondoUnitBridgeRequest, GeoEntityLevel,
    GeoEntityRef, GeoIdentityRelation, GeoLedgerBridge, GeoLinearRingMm, GeoPadBblRow, GeoPointMm,
    GeoPopulationCaseTruthReachByGrain, GeoPopulationEvaluationRequest, GeoTruthReachByGrain,
    GeoTruthRepresentationGrain, bridge_condo_unit, build_condo_bridge,
    canonical_condo_bridge_bytes, canonical_condo_unit_bridge_request_bytes,
    canonical_ledger_bridge_bytes, evaluate_population_with_truth_reach_by_grain,
    footprint_majority_area_inside_parcel, validate_condo_bridge_artifact,
    validate_condo_bridge_request_artifact, validate_ledger_bridge_artifact,
};
use flate2::read::GzDecoder;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    fs::File,
    io::BufReader,
    path::PathBuf,
};

const CONDO_BRIDGE_REQUEST_SCHEMA: &str =
    include_str!("../schemas/canon.geo.condo_bridge_request.v0.schema.json");
const LEDGER_BRIDGE_SCHEMA: &str =
    include_str!("../schemas/canon.geo.ledger_bridge.v0.schema.json");

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
fn matched_zero_or_ambiguous_billing_rows_stay_unmapped() {
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
    assert_eq!(row.truth_billing_grain, vec!["1000072001".to_string()]);
    assert_eq!(row.after.truth_members, 1);
    assert_eq!(row.unmapped_lots.len(), 2);

    let reasons = row
        .unmapped_lots
        .iter()
        .map(|lot| (lot.unit_lot.as_str(), format!("{:?}", lot.reason)))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(reasons["1000073001"], "AmbiguousBillingLots");
    assert_eq!(reasons["1000074001"], "MissingBillingLot");
    assert!(
        !reasons.contains_key("1000072001"),
        "without a PAD row or range, the module must not infer a condo unit from a lot-number range"
    );
    assert!(
        row.lot_mappings
            .iter()
            .all(|mapping| mapping.billing_lot.is_none()),
        "zero, missing, and ambiguous billing rows must not guess a billing lot"
    );
}

#[test]
fn case4_geometry_fixture_holds_the_declared_layout() {
    let fixture = case4_fixture();
    assert!(fixture.source_dataset.starts_with("fixture."));
    assert_eq!(fixture.coordinate_unit, "mm");

    let expected = BTreeMap::from([
        ("1006494", "1004540041"),
        ("1006495", "1004540042"),
        ("1006496", "1004540043"),
        ("1006497", "1004540044"),
        ("1006498", "1004540045"),
        ("1006499", "1004540046"),
        ("1006500", "1004540047"),
    ]);

    for (id, polygon) in fixture.parcels.iter().chain(fixture.buildings.iter()) {
        let ring = predicate_ring(&fixture.frame_id, polygon);
        assert_eq!(ring.frame_id(), fixture.frame_id);
        assert_eq!(ring.vertices().len(), polygon.exterior.vertices.len());
        assert!(
            ring.absolute_double_area_mm2() > 0,
            "ring {id} must have positive exact area"
        );
    }

    for (building_id, expected_parcel_id) in &expected {
        let footprint = fixture
            .buildings
            .get(*building_id)
            .expect("fixture building exists");
        let inside = fixture
            .parcels
            .iter()
            .filter_map(|(parcel_id, parcel)| {
                majority_inside(&fixture.frame_id, footprint, parcel).then_some(parcel_id.as_str())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            inside,
            vec![*expected_parcel_id],
            "building {building_id} should majority-hit only parcel {expected_parcel_id}"
        );
    }

    let core_parcels = [
        "1004540041",
        "1004540042",
        "1004540043",
        "1004540044",
        "1004540045",
        "1004540046",
    ];
    for (index, left_id) in core_parcels.iter().enumerate() {
        for right_id in core_parcels.iter().skip(index + 1) {
            let left = fixture.parcels.get(*left_id).expect("left parcel exists");
            let right = fixture.parcels.get(*right_id).expect("right parcel exists");
            assert!(
                !majority_inside(&fixture.frame_id, left, right)
                    && !majority_inside(&fixture.frame_id, right, left),
                "core parcel interiors must be disjoint: {left_id} vs {right_id}"
            );
        }
    }
}

#[test]
fn t11_condo_bridge_confirms_only_when_block_and_geometry_agree() {
    let request = fixture_unit_request(&["1004540041"], &["1006494"], "454");
    validate_condo_bridge_request_artifact(&request).expect("request validates");
    canonical_condo_unit_bridge_request_bytes(&request).expect("request canonicalizes");

    let bridge = bridge_condo_unit(&request).expect("bridge runs");
    validate_ledger_bridge_artifact(&bridge).expect("ledger bridge validates");
    canonical_ledger_bridge_bytes(&bridge).expect("ledger bridge canonicalizes");

    assert_eq!(bridge.confirmation, GeoCondoConfirmation::BlockAndGeometry);
    assert_eq!(bridge.billing_bbl.as_deref(), Some("1004540041"));
    assert_eq!(bridge.bins, vec!["1006494".to_string()]);
    assert_eq!(bridge.abstained_reason, None);
    assert_eq!(
        bridge.relations,
        expected_relations("1004540041-unit", "1004540041", &["1006494"])
    );
}

#[test]
fn t11_outside_and_half_overlap_are_block_only_abstentions() {
    let fixture = case4_fixture();
    let base = fixture_unit_request_from_fixture(&fixture, &["1004540041"], &["1006494"], "454");

    let mut outside = base.clone();
    outside.footprint_rings.insert(
        "1006494".to_string(),
        translate_polygon(
            fixture.buildings.get("1006494").expect("building exists"),
            12000,
            0,
        ),
    );
    let outside_bridge = bridge_condo_unit(&outside).expect("outside bridge runs");
    assert_abstention(
        &outside_bridge,
        GeoCondoConfirmation::BlockOnly,
        "missing_geometry_confirmation",
    );

    let mut half = base;
    half.footprint_rings.insert(
        "1006494".to_string(),
        translate_polygon(
            fixture.buildings.get("1006494").expect("building exists"),
            10000,
            0,
        ),
    );
    let half_bridge = bridge_condo_unit(&half).expect("half-overlap bridge runs");
    assert_abstention(
        &half_bridge,
        GeoCondoConfirmation::BlockOnly,
        "missing_geometry_confirmation",
    );
}

#[test]
fn t11_block_mismatch_with_geometry_inside_is_key_only() {
    let request = fixture_unit_request(&["1004540041"], &["1006494"], "999");
    let bridge = bridge_condo_unit(&request).expect("bridge runs");
    assert_abstention(&bridge, GeoCondoConfirmation::KeyOnly, "block_mismatch");
}

#[test]
fn t11_two_bins_keep_only_the_majority_inside_bin() {
    let request = fixture_unit_request(&["1004540041"], &["1006494", "1006495"], "454");
    let bridge = bridge_condo_unit(&request).expect("bridge runs");
    assert_eq!(bridge.confirmation, GeoCondoConfirmation::BlockAndGeometry);
    assert_eq!(bridge.billing_bbl.as_deref(), Some("1004540041"));
    assert_eq!(bridge.bins, vec!["1006494".to_string()]);
    assert_eq!(
        bridge.relations,
        expected_relations("1004540041-unit", "1004540041", &["1006494"])
    );
}

#[test]
fn t11_two_billing_candidates_inside_abstains_ambiguous() {
    let request = fixture_unit_request(
        &["1004540041", "1004540042"],
        &["1006494", "1006495"],
        "454",
    );
    let bridge = bridge_condo_unit(&request).expect("bridge runs");
    assert_abstention(
        &bridge,
        GeoCondoConfirmation::BlockOnly,
        "ambiguous_billing_bbl",
    );
}

#[test]
fn condo_bridge_request_schema_matches_a_real_instance() {
    let request = fixture_unit_request(&["1004540041"], &["1006494"], "454");
    let canonical_bytes =
        canonical_condo_unit_bridge_request_bytes(&request).expect("request canonicalizes");
    let instance: Value =
        serde_json::from_slice(&canonical_bytes).expect("canonical request JSON parses");
    assert_schema_pins(
        CONDO_BRIDGE_REQUEST_SCHEMA,
        "canon.geo.condo_bridge_request.v0",
        CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION,
        &instance,
    );
}

#[test]
fn ledger_bridge_schema_matches_a_real_instance() {
    let request = fixture_unit_request(&["1004540041"], &["1006494"], "454");
    let bridge = bridge_condo_unit(&request).expect("bridge runs");
    let canonical_bytes = canonical_ledger_bridge_bytes(&bridge).expect("bridge canonicalizes");
    let instance: Value =
        serde_json::from_slice(&canonical_bytes).expect("canonical bridge JSON parses");
    assert_schema_pins(
        LEDGER_BRIDGE_SCHEMA,
        "canon.geo.ledger_bridge.v0",
        CANON_GEO_LEDGER_BRIDGE_VERSION,
        &instance,
    );
}

#[test]
fn t27_condo_module_has_no_fixture_or_lot_range_literals() {
    let source_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/geo/condo.rs");
    let source = fs::read_to_string(source_path)
        .expect("condo source reads")
        .to_ascii_lowercase();
    for forbidden in [
        "franklin",
        "39049",
        "epsg:3735",
        "1004540041",
        "chimera_wrongly_admitted",
        "asserted_address_core",
        "case_4",
        "solve_composition",
        "7501",
        "1001",
        "reqwest",
        "hyper::",
        "geocodio",
        "nominatim",
        "std::process::command",
        "command::new",
        "mcp",
    ] {
        assert!(
            !source.contains(forbidden),
            "src/geo/condo.rs must not hard-code forbidden literal {forbidden}"
        );
    }
}

#[derive(Debug, Deserialize)]
struct Case4GeometryFixture {
    source_dataset: String,
    frame_id: String,
    coordinate_unit: String,
    parcels: BTreeMap<String, GeoCanonicalPolygonMm>,
    buildings: BTreeMap<String, GeoCanonicalPolygonMm>,
}

fn case4_fixture() -> Case4GeometryFixture {
    serde_json::from_str(include_str!("fixtures/geo/case4_building_geometry.json"))
        .expect("case 4 geometry fixture parses")
}

fn fixture_unit_request(
    billing_bbl_candidates: &[&str],
    bin_candidates: &[&str],
    block: &str,
) -> GeoCondoUnitBridgeRequest {
    let fixture = case4_fixture();
    fixture_unit_request_from_fixture(&fixture, billing_bbl_candidates, bin_candidates, block)
}

fn fixture_unit_request_from_fixture(
    fixture: &Case4GeometryFixture,
    billing_bbl_candidates: &[&str],
    bin_candidates: &[&str],
    block: &str,
) -> GeoCondoUnitBridgeRequest {
    let parcel_rings = billing_bbl_candidates
        .iter()
        .filter_map(|id| {
            fixture
                .parcels
                .get(*id)
                .cloned()
                .map(|polygon| ((*id).to_string(), polygon))
        })
        .collect();
    let footprint_rings = bin_candidates
        .iter()
        .filter_map(|id| {
            fixture
                .buildings
                .get(*id)
                .cloned()
                .map(|polygon| ((*id).to_string(), polygon))
        })
        .collect();

    GeoCondoUnitBridgeRequest {
        version: CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION.to_string(),
        unit_bbl: "1004540041-unit".to_string(),
        billing_bbl_candidates: billing_bbl_candidates
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        bin_candidates: bin_candidates
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        block: block.to_string(),
        frame_id: fixture.frame_id.clone(),
        parcel_rings,
        footprint_rings,
    }
}

fn predicate_ring(frame_id: &str, polygon: &GeoCanonicalPolygonMm) -> GeoLinearRingMm {
    assert!(
        polygon.holes.is_empty(),
        "case 4 fixture and condo bridge tests use exterior rings only"
    );
    let mut closed = polygon.exterior.vertices.clone();
    closed.push(*closed.first().expect("fixture ring has vertices"));
    GeoLinearRingMm::new(frame_id.to_string(), closed).expect("fixture ring validates")
}

fn majority_inside(
    frame_id: &str,
    footprint: &GeoCanonicalPolygonMm,
    parcel: &GeoCanonicalPolygonMm,
) -> bool {
    let footprint_ring = predicate_ring(frame_id, footprint);
    let parcel_ring = predicate_ring(frame_id, parcel);
    footprint_majority_area_inside_parcel(&footprint_ring, &parcel_ring)
        .expect("majority predicate succeeds")
}

fn translate_polygon(
    polygon: &GeoCanonicalPolygonMm,
    delta_x: i64,
    delta_y: i64,
) -> GeoCanonicalPolygonMm {
    GeoCanonicalPolygonMm {
        exterior: translate_ring(&polygon.exterior, delta_x, delta_y),
        holes: polygon
            .holes
            .iter()
            .map(|ring| translate_ring(ring, delta_x, delta_y))
            .collect(),
    }
}

fn translate_ring(ring: &GeoCanonicalRingMm, delta_x: i64, delta_y: i64) -> GeoCanonicalRingMm {
    GeoCanonicalRingMm {
        vertices: ring
            .vertices
            .iter()
            .map(|point| GeoPointMm {
                x: point.x.checked_add(delta_x).expect("fixture x in range"),
                y: point.y.checked_add(delta_y).expect("fixture y in range"),
            })
            .collect(),
    }
}

fn assert_abstention(bridge: &GeoLedgerBridge, confirmation: GeoCondoConfirmation, detail: &str) {
    validate_ledger_bridge_artifact(bridge).expect("abstention validates");
    assert_eq!(bridge.confirmation, confirmation);
    assert_eq!(bridge.billing_bbl, None);
    assert!(bridge.bins.is_empty());
    assert!(bridge.relations.is_empty());
    assert_eq!(
        bridge.abstained_reason.as_deref(),
        Some(format!("{GEO_CONDO_CONFIRMATION_INSUFFICIENT}:{detail}").as_str())
    );
}

fn expected_relations(
    unit_bbl: &str,
    billing_bbl: &str,
    bins: &[&str],
) -> Vec<(GeoEntityRef, GeoIdentityRelation, GeoEntityRef)> {
    let parcel = GeoEntityRef::new(GeoEntityLevel::Parcel, billing_bbl.to_string());
    let mut relations = Vec::with_capacity(bins.len() + 1);
    relations.push((
        GeoEntityRef::new(GeoEntityLevel::PoiUnit, unit_bbl.to_string()),
        GeoIdentityRelation::PartOf,
        parcel.clone(),
    ));
    for bin in bins {
        relations.push((
            GeoEntityRef::new(GeoEntityLevel::Building, (*bin).to_string()),
            GeoIdentityRelation::On,
            parcel.clone(),
        ));
    }
    relations
}

fn assert_schema_pins(schema_text: &str, title: &str, version: &str, instance: &Value) {
    let schema: Value = serde_json::from_str(schema_text).expect("schema JSON parses");
    assert_eq!(schema["title"], title);
    assert_eq!(schema["properties"]["version"]["const"], version);
    assert_eq!(schema["additionalProperties"], false);

    let properties = schema["properties"]
        .as_object()
        .expect("schema properties object")
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    for key in instance
        .as_object()
        .expect("instance is an object")
        .keys()
        .cloned()
    {
        assert!(properties.contains(&key), "schema does not declare {key}");
    }
    for key in schema["required"]
        .as_array()
        .expect("schema required array")
        .iter()
        .map(|value| value.as_str().expect("required key is a string"))
    {
        assert!(instance.get(key).is_some(), "instance is missing {key}");
    }
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
