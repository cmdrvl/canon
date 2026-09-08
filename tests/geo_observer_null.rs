#![forbid(unsafe_code)]

use canon::geo::{
    DEFAULT_MAX_MATERIALIZED_MODELS, GEO_NULL_FOOTPRINT_OBSERVER_ID,
    GEO_NULL_FOOTPRINT_RHO_CONTRACT_ID, GEO_NULL_FOOTPRINT_SOURCE_DATASET, GeoCanonicalPolygonMm,
    GeoCanonicalRingMm, GeoCompositionArtifact, GeoCompositionRequest, GeoEntityLevel,
    GeoEntityRef, GeoEvidenceDisposition, GeoImageTilePin, GeoNullFootprintPlane,
    GeoNullFootprintPlaneSourcePin, GeoNullRedundancyCase, GeoObservationKind,
    GeoObservationPayload, GeoObserverErrorCode, GeoPointMm, GeoSoftPreference, assert_redundant,
    canonical_polygon_blake3, canonical_ring_blake3, canonicalize_composition_request,
    characterize_null, compile_evidence, emit_null_footprint_observations,
    materialize_warehouse_rows, null_footprint_observer_contract, null_footprint_rho_contract,
    null_observer_rows_to_warehouse_rows, solve_composition,
};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeMap, fs, path::PathBuf};

const POPULATION_ID: &str = "population.d6.null.fixture";

#[derive(Debug, Deserialize)]
struct Case4GeometryFixture {
    source_dataset: String,
    frame_id: String,
    coordinate_unit: String,
    parcels: BTreeMap<String, GeoCanonicalPolygonMm>,
    buildings: BTreeMap<String, GeoCanonicalPolygonMm>,
}

#[derive(Debug, Deserialize)]
struct WorkedCorpus {
    cases: Vec<WorkedCase>,
}

#[derive(Debug, Deserialize)]
struct WorkedCase {
    case_id: String,
    request: GeoCompositionRequest,
}

fn case4_fixture() -> Case4GeometryFixture {
    serde_json::from_str(include_str!("fixtures/geo/case4_building_geometry.json"))
        .expect("case 4 geometry fixture parses")
}

fn worked_corpus() -> WorkedCorpus {
    serde_json::from_str(include_str!("fixtures/geo/e4_worked_cases.json"))
        .expect("worked corpus parses")
}

fn fixture_tile_pin() -> GeoImageTilePin {
    serde_json::from_str(include_str!("fixtures/geo/fixture_pin_cc_by.json"))
        .expect("fixture pin parses")
}

fn digest(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn source_pin(fixture: &Case4GeometryFixture) -> GeoNullFootprintPlaneSourcePin {
    GeoNullFootprintPlaneSourcePin {
        source_dataset: fixture.source_dataset.clone(),
        source_release: "fixture_case4_2024".to_string(),
        source_content_sha256: sha256_hex(include_bytes!(
            "fixtures/geo/case4_building_geometry.json"
        )),
        parser_version: "fixture_parser_v0".to_string(),
        license_terms: "Fixture geometry for Canon Geo tests only".to_string(),
        attribution_text: "CMD+RVL fixture".to_string(),
        source_lineage_ids: vec!["fixture.case4.geometry".to_string()],
    }
}

fn null_plane(fixture: &Case4GeometryFixture) -> GeoNullFootprintPlane {
    GeoNullFootprintPlane {
        source_pin: source_pin(fixture),
        frame_id: fixture.frame_id.clone(),
        parcel_rings: fixture.parcels.clone(),
        footprint_rings: fixture.buildings.clone(),
    }
}

fn core_window() -> GeoCanonicalPolygonMm {
    square(0, 0, 60_000, 40_000)
}

fn square(min_x: i64, min_y: i64, max_x: i64, max_y: i64) -> GeoCanonicalPolygonMm {
    GeoCanonicalPolygonMm {
        exterior: GeoCanonicalRingMm {
            vertices: vec![
                GeoPointMm { x: min_x, y: min_y },
                GeoPointMm { x: max_x, y: min_y },
                GeoPointMm { x: max_x, y: max_y },
                GeoPointMm { x: min_x, y: max_y },
            ],
        },
        holes: Vec::new(),
    }
}

fn shifted_plane(
    fixture: &Case4GeometryFixture,
    footprint_id: &str,
    delta_x: i64,
) -> GeoNullFootprintPlane {
    let mut plane = null_plane(fixture);
    let shifted = translate_polygon(
        plane
            .footprint_rings
            .get(footprint_id)
            .expect("fixture footprint exists"),
        delta_x,
        0,
    );
    plane
        .footprint_rings
        .insert(footprint_id.to_string(), shifted);
    plane
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
                x: point.x.checked_add(delta_x).expect("fixture x fits"),
                y: point.y.checked_add(delta_y).expect("fixture y fits"),
            })
            .collect(),
    }
}

fn emit_fixture_null(
    fixture: &Case4GeometryFixture,
    plane: &GeoNullFootprintPlane,
) -> canon::geo::GeoNullFootprintObservationResult {
    let characterization_blake3 = digest(b"fixture null observer characterization");
    let contract = null_footprint_observer_contract(POPULATION_ID, characterization_blake3);
    let rho_contract =
        null_footprint_rho_contract(&plane.source_pin, &contract.error_population_id)
            .expect("rho contract builds");
    let window = core_window();
    let window_blake3 = canonical_polygon_blake3(&window).expect("window hashes");
    emit_null_footprint_observations(
        &contract,
        &rho_contract,
        &fixture_tile_pin(),
        &window_blake3,
        &window,
        &fixture.frame_id,
        &plane.footprint_rings,
        &["commercial_basemap_tos".to_string()],
    )
    .expect("null observer emits")
}

fn solved_case(case_id: &str, request: &GeoCompositionRequest) -> GeoCompositionArtifact {
    solve_composition(request).unwrap_or_else(|error| panic!("{case_id} solve failed: {error}"))
}

fn request_with_zero_cost_building_preferences(
    request: &GeoCompositionRequest,
) -> GeoCompositionRequest {
    let mut with_null = request.clone();
    with_null
        .soft_preferences
        .extend(
            request
                .universe
                .buildings
                .iter()
                .map(|building| GeoSoftPreference {
                    id: format!("observer.null.pref.{}", building.id),
                    member: GeoEntityRef::new(GeoEntityLevel::Building, building.id.clone()),
                    cost_if_absent: 0,
                }),
        );
    canonicalize_composition_request(&with_null).expect("zero-cost preferences stay canonical")
}

fn empty_baseline_from_warehouse_request(
    request: &canon::geo::GeoWarehouseRowsRequest,
) -> GeoCompositionRequest {
    let mut baseline = request.clone();
    baseline.contracts.clear();
    baseline.evidence_rows.clear();
    let evidence_request = materialize_warehouse_rows(&baseline).expect("baseline materializes");
    evidence_request.composition_request()
}

trait EvidenceRequestExt {
    fn composition_request(self) -> GeoCompositionRequest;
}

impl EvidenceRequestExt for canon::geo::GeoEvidenceCompilationRequest {
    fn composition_request(self) -> GeoCompositionRequest {
        compile_evidence(&self)
            .expect("evidence request compiles")
            .composition_request
    }
}

fn same_residual(left: &GeoCompositionArtifact, right: &GeoCompositionArtifact) {
    assert_eq!(left.status, right.status);
    assert_eq!(
        left.summary.residual_model_count,
        right.summary.residual_model_count
    );
    assert_eq!(left.hard_forced, right.hard_forced);
}

#[test]
fn t31_null_observer_reemits_landed_footprint_rings_and_characterizes_zero_error() {
    let fixture = case4_fixture();
    assert_eq!(fixture.coordinate_unit, "mm");
    let plane = null_plane(&fixture);
    let result = emit_fixture_null(&fixture, &plane);

    let expected_ids = vec![
        "1006494".to_string(),
        "1006495".to_string(),
        "1006496".to_string(),
        "1006497".to_string(),
        "1006498".to_string(),
        "1006499".to_string(),
    ];
    assert_eq!(result.emitted_footprint_ids, expected_ids);
    assert_eq!(result.excluded_by_window, ["1006500".to_string()]);
    assert!(result.holes_ignored);
    assert!(result.artifact.rho_observations.is_empty());
    assert_eq!(
        result.artifact.not_admitted_ids,
        expected_ids
            .iter()
            .map(|id| format!("obs:null:{id}"))
            .collect::<Vec<_>>()
    );

    for row in &result.artifact.rows {
        assert_eq!(row.observer_id, GEO_NULL_FOOTPRINT_OBSERVER_ID);
        assert_eq!(row.kind, GeoObservationKind::FootprintOutline);
        let footprint_id = row
            .id
            .strip_prefix("obs:null:")
            .expect("row id uses null prefix");
        let expected = canonical_ring_blake3(
            &fixture
                .buildings
                .get(footprint_id)
                .expect("fixture building exists")
                .exterior,
        )
        .expect("ring hashes");
        let GeoObservationPayload::FootprintOutline { ring_blake3 } = &row.payload else {
            panic!("null row must carry footprint outline payload");
        };
        assert_eq!(
            ring_blake3, &expected,
            "ring digest mismatch for {footprint_id}"
        );
        assert_eq!(row.label_blake3, expected);
    }

    let population_blake3 = digest(b"fixture d6 null observer population");
    let characterization =
        characterize_null(&result.artifact, &fixture.buildings, &population_blake3)
            .expect("null characterization succeeds");
    let footprint = characterization
        .per_kind
        .get("footprint_outline")
        .expect("footprint characterization exists");
    assert_eq!(footprint.compared, 6);
    assert_eq!(footprint.exact_agreement, 6);
    assert_eq!(footprint.max_abs_error, 0);
    assert!(characterization.is_null_baseline);

    let mut moved = fixture.buildings.clone();
    let shifted = translate_polygon(moved.get("1006499").expect("building exists"), 1, 0);
    moved.insert("1006499".to_string(), shifted);
    let error = characterize_null(&result.artifact, &moved, &population_blake3)
        .expect_err("moved landed ring must refuse");
    assert_eq!(error.code, GeoObserverErrorCode::ObserverNullRingMismatch);
    assert_eq!(error.detail["footprint_id"], "1006499");
}

#[test]
fn t32_null_observer_warehouse_rows_are_admitted_but_residual_redundant() {
    let fixture = case4_fixture();
    let plane = null_plane(&fixture);
    let result = emit_fixture_null(&fixture, &plane);
    let converted = null_observer_rows_to_warehouse_rows(
        &result.artifact,
        &plane,
        20_000,
        DEFAULT_MAX_MATERIALIZED_MODELS,
    )
    .expect("null rows convert to warehouse rows");

    assert!(converted.unassigned_footprints.is_empty());
    assert_eq!(converted.request.building_parcel_rows.len(), 7);
    assert_eq!(
        converted
            .request
            .building_parcel_rows
            .iter()
            .map(|row| (row.building_id.as_str(), row.parcel_id.as_deref()))
            .collect::<Vec<_>>(),
        vec![
            ("1006494", Some("1004540041")),
            ("1006495", Some("1004540042")),
            ("1006496", Some("1004540043")),
            ("1006497", Some("1004540044")),
            ("1006498", Some("1004540045")),
            ("1006499", Some("1004540046")),
            ("1006500", Some("1004540047")),
        ]
    );
    let null_contract = converted
        .request
        .contracts
        .first()
        .expect("null rho contract emitted");
    assert_eq!(
        null_contract.source_release,
        converted.source_pin.source_release
    );
    assert!(null_contract.source_lineage_ids.contains(&format!(
        "{}:{}",
        converted.source_pin.source_dataset, converted.source_pin.source_release
    )));
    assert!(
        null_contract
            .source_lineage_ids
            .contains(&format!("{GEO_NULL_FOOTPRINT_OBSERVER_ID}:{POPULATION_ID}"))
    );
    assert_eq!(
        converted.source_pin.license_terms,
        "Fixture geometry for Canon Geo tests only"
    );
    assert_eq!(converted.source_pin.attribution_text, "CMD+RVL fixture");

    let mut unlicensed_pin = converted.source_pin.clone();
    unlicensed_pin.license_terms.clear();
    let pin_error = null_footprint_rho_contract(&unlicensed_pin, POPULATION_ID)
        .expect_err("unlicensed source pin must refuse");
    assert_eq!(pin_error.code, GeoObserverErrorCode::InvalidInput);
    assert_eq!(pin_error.detail["field"], "license_terms");

    let evidence_request =
        materialize_warehouse_rows(&converted.request).expect("null rows materialize");
    let compilation = compile_evidence(&evidence_request).expect("null evidence compiles");
    assert_eq!(compilation.admissions.len(), result.artifact.rows.len());
    assert!(
        compilation
            .admissions
            .iter()
            .all(|admission| admission.contract.source_dataset == GEO_NULL_FOOTPRINT_SOURCE_DATASET)
    );
    assert!(
        compilation
            .admissions
            .iter()
            .all(|admission| admission.contract.id == GEO_NULL_FOOTPRINT_RHO_CONTRACT_ID)
    );
    assert!(
        compilation
            .admissions
            .iter()
            .all(|admission| admission.disposition == GeoEvidenceDisposition::SoftPreference)
    );
    assert!(compilation.composition_request.hard_constraints.is_empty());
    assert_eq!(
        compilation.composition_request.soft_preferences.len(),
        result.artifact.rows.len()
    );

    let baseline_request = empty_baseline_from_warehouse_request(&converted.request);
    let baseline = solved_case("demo0-null-baseline", &baseline_request);
    let with_null = solved_case("demo0-null-admitted", &compilation.composition_request);
    same_residual(&baseline, &with_null);

    let corpus = worked_corpus();
    assert_eq!(corpus.cases.len(), 6);
    let mut redundancy_cases = vec![GeoNullRedundancyCase {
        case_id: "demo0.case4.materialized_null_plane".to_string(),
        before: baseline,
        after: with_null,
    }];
    for case in corpus.cases {
        let before = solved_case(&case.case_id, &case.request);
        let after_request = request_with_zero_cost_building_preferences(&case.request);
        let after = solved_case(&case.case_id, &after_request);
        redundancy_cases.push(GeoNullRedundancyCase {
            case_id: case.case_id,
            before,
            after,
        });
    }
    let report = assert_redundant(&redundancy_cases).expect("null observer is redundant");
    assert_eq!(report.denominator, 7);
    assert!(report.non_redundant_case_ids.is_empty());

    let half_plane = shifted_plane(&fixture, "1006494", 10_000);
    let half_result = emit_fixture_null(&fixture, &half_plane);
    let half = null_observer_rows_to_warehouse_rows(
        &half_result.artifact,
        &half_plane,
        20_000,
        DEFAULT_MAX_MATERIALIZED_MODELS,
    )
    .expect("half-overlap plane converts");
    assert_eq!(
        half.unassigned_footprints
            .iter()
            .map(|row| (row.footprint_id.as_str(), row.reason.as_str()))
            .collect::<Vec<_>>(),
        vec![("1006494", "no_majority_parcel")]
    );
    assert!(
        half.request
            .building_parcel_rows
            .iter()
            .any(|row| row.building_id == "1006494" && row.parcel_id.is_none())
    );

    let shifted_plane = shifted_plane(&fixture, "1006494", 12_000);
    let shifted_result = emit_fixture_null(&fixture, &shifted_plane);
    let shifted = null_observer_rows_to_warehouse_rows(
        &shifted_result.artifact,
        &shifted_plane,
        20_000,
        DEFAULT_MAX_MATERIALIZED_MODELS,
    )
    .expect("strict-majority plane converts");
    let shifted_targets = shifted
        .request
        .building_parcel_rows
        .iter()
        .filter(|row| row.building_id == "1006494")
        .map(|row| row.parcel_id.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(shifted_targets, vec![Some("1004540042")]);

    let mut non_redundant = redundancy_cases;
    non_redundant[0].after.summary.residual_model_count += 1;
    let error = assert_redundant(&non_redundant).expect_err("changed count must refuse");
    assert_eq!(error.code, GeoObserverErrorCode::ObserverNotRedundant);
    assert_eq!(
        error.detail["case_id"],
        "demo0.case4.materialized_null_plane"
    );
}

#[test]
fn t33_null_observer_has_no_decoder_network_or_hosted_model_literals() {
    let source_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/geo/observer_null.rs");
    let source = fs::read_to_string(source_path).expect("null observer source reads");
    assert!(
        forbidden_null_observer_literal(&source).is_none(),
        "src/geo/observer_null.rs must stay rule-based and offline"
    );

    let seeded = format!("{source}\nconst BAD: &str = \"Png\";\n");
    assert_eq!(forbidden_null_observer_literal(&seeded), Some("png"));
}

fn forbidden_null_observer_literal(source: &str) -> Option<&'static str> {
    let folded = source.to_ascii_lowercase();
    [
        "png",
        "tiff",
        "jpeg",
        "decode",
        "reqwest",
        "hyper::",
        "openai",
        "anthropic",
        "gemini",
        "bedrock",
        "invoke_model",
        "std::process::command",
        "command::new",
    ]
    .into_iter()
    .find(|literal| folded.contains(literal))
}
