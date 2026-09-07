#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_CONDO_BRIDGE_PAD_METHOD, CANON_GEO_CONDO_BRIDGE_REQUEST_VERSION,
    CANON_GEO_LEDGER_BRIDGE_VERSION, GEO_CONDO_CONFIRMATION_INSUFFICIENT, GeoCandidateReachStatus,
    GeoCanonicalPolygonMm, GeoCanonicalRingMm, GeoCondoBridgeCaseRequest, GeoCondoBridgeReachKind,
    GeoCondoBridgeRequest, GeoCondoConfirmation, GeoCondoSourcePin, GeoCondoUnitBridgeRequest,
    GeoEntityLevel, GeoEntityRef, GeoIdentityRelation, GeoLedgerBridge, GeoLinearRingMm,
    GeoPadBblRow, GeoPointMm, GeoPopulationCaseTruthReachByGrain, GeoPopulationEvaluationRequest,
    GeoTruthReachByGrain, GeoTruthRepresentationGrain, bridge_condo_unit, build_condo_bridge,
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
    assert_eq!(artifact.source_pins, vec![fixture_pad_source_pin()]);

    let expected = read_json(fixture_path("condo_bridge_pad.json"));
    assert_eq!(receipt_projection(&artifact), expected);
}

#[test]
fn condo_bridge_rows_preserve_source_pin_attribution() {
    let artifact = build_condo_bridge(&fixture_bridge_request()).expect("bridge builds");
    let mapping = artifact
        .rows
        .iter()
        .flat_map(|row| row.lot_mappings.iter())
        .find(|mapping| !mapping.source_pins.is_empty())
        .expect("mapped fixture rows carry source pins");

    let source_pin = mapping
        .source_pins
        .iter()
        .find(|pin| pin.source_table == "EDGAR_DB.SOURCE.NYC_DCP_PAD_BBL_HOT")
        .expect("mapping carries the PAD source pin");
    assert_eq!(source_pin.source_release, "26B");
    assert!(
        source_pin.source_row_number.is_some(),
        "mapping must carry the live PAD SOURCE_ROW_NUMBER pin"
    );
    assert_eq!(
        source_pin.source_content_sha256.as_deref(),
        Some("016a29968b4bed9e8dde10b9c27b68132aba994baf1dc3e2543a861eadfdf4bd")
    );
    assert_eq!(
        source_pin.source_content_sha256_field.as_deref(),
        Some("SOURCE_ZIP_SHA256")
    );
    assert_eq!(
        source_pin.license_terms,
        "Public NYC Department of City Planning Bytes of the Big Apple release for informational purposes only; DCP disclaims completeness, accuracy, content, and fitness warranties."
    );
    assert_eq!(
        source_pin.attribution_text,
        "NYC Department of City Planning (DCP)"
    );
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
        source_pins: vec![fixture_pad_source_pin()],
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
    assert_eq!(bridge.source_pins, vec![fixture_geometry_source_pin()]);
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
        source_pins: vec![fixture_geometry_source_pin()],
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
    for key in instance.as_object().expect("instance is an object").keys() {
        assert!(properties.contains(key), "schema does not declare {key}");
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

    let cases: Vec<GeoCondoBridgeCaseRequest> = population["cases"]
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
        source_pins: vec![fixture_pad_source_pin()],
        pad_rows: read_pad_rows(&cases),
        cases,
        max_pad_rows: 1_000,
        max_cases: 100,
    }
}

fn read_pad_rows(cases: &[GeoCondoBridgeCaseRequest]) -> Vec<GeoPadBblRow> {
    let file = File::open(fixture_path("pad_bbl.json.gz")).expect("open PAD BBL fixture");
    let mut rows: Vec<GeoPadBblRow> =
        serde_json::from_reader(GzDecoder::new(BufReader::new(file))).expect("parse PAD BBL rows");
    let lots = cases
        .iter()
        .flat_map(|case| {
            case.truth_parcels
                .iter()
                .chain(case.universe_parcels.iter())
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    rows.retain(|row| {
        lots.iter().any(|lot| {
            row.bbl_key.as_str() == lot.as_str()
                || (row.low_bbl_key.as_str() <= lot.as_str()
                    && lot.as_str() <= row.high_bbl_key.as_str())
        })
    });
    let source_row_numbers = fixture_pad_source_row_numbers();
    for row in &mut rows {
        row.source_row_number = Some(
            *source_row_numbers
                .get(&pad_row_key(row))
                .expect("fixture PAD row has a live SOURCE_ROW_NUMBER pin"),
        );
    }
    rows
}

fn fixture_pad_source_row_numbers() -> BTreeMap<String, u64> {
    BTreeMap::from([
        (
            "1000281201|1000281201|1000281203|1000287502|3159".to_string(),
            198,
        ),
        (
            "1000281201|1000281301|1000281302|1000287502|3159".to_string(),
            199,
        ),
        (
            "1000291101|1000291101|1000291102|1000297502|1049".to_string(),
            218,
        ),
        (
            "1000291301|1000291301|1000291307|1000297504|1805".to_string(),
            220,
        ),
        (
            "1010291102|1010291102|1010291102|1010297502|2967".to_string(),
            17076,
        ),
        (
            "1010291103|1010291103|1010291103|1010297502|2967".to_string(),
            17077,
        ),
        (
            "1010291104|1010291104|1010291104|1010297502|2967".to_string(),
            17078,
        ),
        (
            "1010291105|1010291105|1010291280|1010297502|2967".to_string(),
            17079,
        ),
        (
            "1012741301|1012741301|1012741479|1012747504|1508".to_string(),
            22836,
        ),
        (
            "1013251301|1013251301|1013251322|1013257504|2240".to_string(),
            23670,
        ),
        (
            "1013261001|1013261001|1013261137|1013267501|1840".to_string(),
            23703,
        ),
        (
            "1013751201|1013751201|1013751205|1013757503|2425".to_string(),
            24516,
        ),
        (
            "1013751201|1013751207|1013751207|1013757503|2425".to_string(),
            24517,
        ),
        (
            "1013751201|1013751209|1013751300|1013757503|2425".to_string(),
            24518,
        ),
        (
            "1013751201|1013751303|1013751312|1013757503|2425".to_string(),
            24519,
        ),
        (
            "1015691301|1015691301|1015691430|1015697503|1701".to_string(),
            30024,
        ),
        (
            "1022151101|1022151101|1022151102|1022157502|2582".to_string(),
            43846,
        ),
        (
            "2023271101|2023271101|2023271102|2023277501|301".to_string(),
            46082,
        ),
        ("2058021275|2058021275|2058021275||".to_string(), 133166),
        ("2058021280|2058021280|2058021280||".to_string(), 133167),
        ("2058021294|2058021294|2058021294||".to_string(), 133168),
        ("2058021301|2058021301|2058021301||".to_string(), 133169),
        ("2058021302|2058021302|2058021302||".to_string(), 133170),
        ("2058021321|2058021321|2058021321||".to_string(), 133171),
        ("2058141101|2058141101|2058141101||".to_string(), 133321),
        (
            "3001641101|3001641101|3001641284|3001647502|2853".to_string(),
            136320,
        ),
        (
            "3023661001|3023661001|3023661003|3023667501|4924".to_string(),
            205921,
        ),
        (
            "3033571001|3033571001|3033571002|3033577501|4449".to_string(),
            223834,
        ),
        (
            "3033571003|3033571003|3033571004|3033577501|4449".to_string(),
            223836,
        ),
        (
            "3061261001|3061261001|3061261003|3061267501|5363".to_string(),
            313261,
        ),
        (
            "3087731001|3087731001|3087731141|3087737501|3818".to_string(),
            410474,
        ),
        (
            "4050141101|4050141101|4050141110|4050147502|1182".to_string(),
            534118,
        ),
        (
            "4067971301|4067971301|4067971445|4067977503|325".to_string(),
            566098,
        ),
    ])
}

fn pad_row_key(row: &GeoPadBblRow) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        row.bbl_key,
        row.low_bbl_key,
        row.high_bbl_key,
        row.billing_bbl_key.as_deref().unwrap_or(""),
        row.condo_number
            .map(|condo_number| condo_number.to_string())
            .unwrap_or_default()
    )
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
        release: None,
        release_dt: None,
        source_row_number: None,
        source_file: None,
        source_filename: None,
        source_zip_sha256: None,
        parser_version: None,
        license_terms: None,
        attribution_text: None,
    }
}

fn fixture_pad_source_pin() -> GeoCondoSourcePin {
    GeoCondoSourcePin {
        source_table: "EDGAR_DB.SOURCE.NYC_DCP_PAD_BBL_HOT".to_string(),
        natural_key: "release/source_row_number".to_string(),
        source_release: "26B".to_string(),
        release_dt: Some("2026-05-01".to_string()),
        variant: None,
        source_row_number: None,
        source_file: Some("bobabbl.txt".to_string()),
        source_file_field: Some("SOURCE_FILE".to_string()),
        source_content_sha256: Some(
            "016a29968b4bed9e8dde10b9c27b68132aba994baf1dc3e2543a861eadfdf4bd".to_string(),
        ),
        source_content_sha256_field: Some("SOURCE_ZIP_SHA256".to_string()),
        parser_version: Some("2026-08-16".to_string()),
        license_terms: "Public NYC Department of City Planning Bytes of the Big Apple release for informational purposes only; DCP disclaims completeness, accuracy, content, and fitness warranties.".to_string(),
        attribution_text: "NYC Department of City Planning (DCP)".to_string(),
    }
}

fn fixture_geometry_source_pin() -> GeoCondoSourcePin {
    GeoCondoSourcePin {
        source_table: "fixture.geometry".to_string(),
        natural_key: "fixture_row".to_string(),
        source_release: "fixture-release".to_string(),
        release_dt: Some("2026-09-03".to_string()),
        variant: None,
        source_row_number: Some(1),
        source_file: Some("fixture-geometry.json".to_string()),
        source_file_field: Some("fixture_file".to_string()),
        source_content_sha256: Some(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        ),
        source_content_sha256_field: Some("fixture_sha256".to_string()),
        parser_version: Some("fixture-parser-v1".to_string()),
        license_terms: "fixture-only geometry terms".to_string(),
        attribution_text: "fixture-only geometry attribution".to_string(),
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
