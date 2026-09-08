#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_COLLATERAL_LEDGER_VERSION, CANON_GEO_EVENT_EXPOSURE_VERSION, GeoAdvisoryArchive,
    GeoAdvisoryPin, GeoCandidateReachStatus, GeoCanonicalGeometryMm, GeoCanonicalPolygonMm,
    GeoClaimClass, GeoCollateralLedger, GeoCollateralLedgerProofClass, GeoCompositionStatus,
    GeoEventExposure, GeoExposureAccessionSummary, GeoExposureErrorCode, GeoExposureGeometryInput,
    GeoLedgerRow, GeoLinearRingMm, GeoPointLocation, GeoPointMm, GeoSourceReleasePin,
    GeoTruthPlane, build_collateral_ledger, canonical_event_exposure_bytes, join_exposure,
    join_exposure_from_geometry_input, validate_event_exposure_artifact,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

const CASE4_GEOMETRY: &str = include_str!("fixtures/geo/case4_building_geometry.json");
const SYNTHETIC_ADV12: &str = include_str!("fixtures/geo/advisory_synthetic_adv12.json");
const FRAME_ID: &str = "fixture.case4.frame";
const ACCESSION: &str = "fixture-accession-case4";
const DEAL_ID: &str = "fixture-deal-case4";
const LOAN_ID: &str = "fixture-loan-case4";

#[derive(Debug, Deserialize)]
struct Case4GeometryFixture {
    frame_id: String,
    buildings: BTreeMap<String, GeoCanonicalPolygonMm>,
}

#[test]
fn t08_event_exposure_joins_wind_radii_to_exact_ledger_building_geometry() {
    let ledger = fixture_ledger();
    let geometry = case4_geometry();
    let advisory = synthetic_advisory();
    let archive = fixture_archive(&advisory);

    let artifact = join_exposure(
        &ledger,
        &advisory,
        &geometry.buildings,
        &advisory.source_blake3s,
    )
    .expect("T08 exposure joins fixture polygons against archived advisory");
    let typed_artifact =
        join_exposure_from_geometry_input(&ledger, &advisory, &geometry_input(), &archive)
            .expect("T08 typed geometry input joins");

    assert_eq!(
        artifact, typed_artifact,
        "T08 polygon-only and typed geometry paths must produce identical exposure"
    );
    assert_eq!(artifact.version, CANON_GEO_EVENT_EXPOSURE_VERSION);
    assert_eq!(artifact.proof_class, GeoCollateralLedgerProofClass::Fixture);
    assert_eq!(artifact.advisory.source_blake3s, advisory.source_blake3s);
    assert!(
        artifact.holes_ignored,
        "T08 exterior-only predicate must be stated"
    );
    assert!(
        artifact.buildings_without_geometry.is_empty(),
        "T08 all six fixture ledger buildings have polygons: {:?}",
        artifact.buildings_without_geometry
    );

    let exposed_by_building = artifact
        .exposed
        .iter()
        .map(|building| (building.building_id.as_str(), building.knots_band))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        exposed_by_building,
        BTreeMap::from([("1006494", 64), ("1006495", 50), ("1006496", 34)]),
        "T08 unexpected exposed rows: {:?}",
        artifact.exposed
    );
    assert!(
        artifact
            .exposed
            .iter()
            .all(|building| building.backbone_member
                && building.claim_class == GeoClaimClass::CollateralComposition),
        "T08 exposed rows must retain backbone/claim metadata: {:?}",
        artifact.exposed
    );

    let not_exposed = artifact
        .not_exposed
        .iter()
        .map(|building| building.building_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        not_exposed,
        vec!["1006497", "1006498", "1006499"],
        "T08 strict-majority predicate should leave the lower row not exposed: {:?}",
        artifact.not_exposed
    );
    assert_classifies_each_fixture_building_once(&artifact);
    assert_eq!(
        artifact.per_accession,
        vec![GeoExposureAccessionSummary {
            accession: ACCESSION.to_string(),
            loans_exposed: 1,
            buildings_exposed: 3,
            max_knots_band: 64,
        }]
    );
    canonical_event_exposure_bytes(&artifact).expect("T08 event exposure canonicalizes");
    validate_event_exposure_artifact(&artifact).expect("T08 event exposure validates");
}

#[test]
fn t08_event_exposure_refuses_missing_archive_pin() {
    let ledger = fixture_ledger();
    let advisory = synthetic_advisory();
    let archive = GeoAdvisoryArchive {
        source_blake3s: Vec::new(),
        advisories: Vec::new(),
    };

    let error = join_exposure_from_geometry_input(&ledger, &advisory, &geometry_input(), &archive)
        .expect_err("T08 missing advisory source pin must refuse");
    assert_eq!(error.code, GeoExposureErrorCode::ExposureAdvisoryStale);
    assert_eq!(
        error.detail.get("source_blake3"),
        advisory.source_blake3s.first(),
        "T08 stale refusal should name the missing source pin: {error:?}"
    );
}

#[test]
fn t08_event_exposure_refuses_superseded_advisory_number() {
    let ledger = fixture_ledger();
    let advisory = synthetic_advisory();
    let mut archive = fixture_archive(&advisory);
    archive.advisories.push(canon::geo::GeoArchivedAdvisoryRef {
        storm_id: advisory.storm_id.clone(),
        advisory_number: 13,
        source_blake3s: vec![prefixed_blake3(b"fixture.nhc.synthetic.adv13.wind_radii")],
    });

    let error = join_exposure_from_geometry_input(&ledger, &advisory, &geometry_input(), &archive)
        .expect_err("T08 later advisory for the same storm must refuse");
    assert_eq!(error.code, GeoExposureErrorCode::ExposureAdvisoryStale);
    assert_eq!(
        error.detail.get("advisory_number").map(String::as_str),
        Some("13"),
        "T08 stale refusal should name the later advisory number: {error:?}"
    );
}

#[test]
fn t08_event_exposure_refuses_all_point_geometry_and_does_not_guess_centroids() {
    let ledger = fixture_ledger();
    let advisory = synthetic_advisory();
    let archive = fixture_archive(&advisory);
    let points = point_geometry_input();

    let error = join_exposure_from_geometry_input(&ledger, &advisory, &points, &archive)
        .expect_err("T08 point-only geometry cannot stand in for building polygons");
    assert_eq!(error.code, GeoExposureErrorCode::ExposureGeometryMissing);
    assert_eq!(
        error.detail.get("building_id").map(String::as_str),
        Some("1006494"),
        "T08 all-point refusal should name the first missing polygon: {error:?}"
    );
}

#[test]
fn t08_event_exposure_lists_mixed_point_geometry_without_aborting_usable_polygons() {
    let ledger = fixture_ledger();
    let advisory = synthetic_advisory();
    let archive = fixture_archive(&advisory);
    let mut geometry = geometry_input();
    geometry.buildings.insert(
        "1006499".to_string(),
        GeoCanonicalGeometryMm::Point {
            coordinate: GeoPointMm::new(50000, 30000),
        },
    );

    let artifact = join_exposure_from_geometry_input(&ledger, &advisory, &geometry, &archive)
        .expect("T08 mixed usable polygons and one point should produce a partial exposure");
    assert_eq!(
        artifact.buildings_without_geometry,
        vec!["1006499".to_string()],
        "T08 mixed point input must list only the unusable building"
    );
    assert_eq!(artifact.exposed.len(), 3);
    assert_eq!(artifact.not_exposed.len(), 2);
}

#[test]
fn t08_event_exposure_exact_half_building_beats_centroid_join() {
    let ledger = fixture_ledger();
    let geometry = case4_geometry();
    let advisory = synthetic_advisory();
    let archive = fixture_archive(&advisory);
    let artifact =
        join_exposure_from_geometry_input(&ledger, &advisory, &geometry_input(), &archive)
            .expect("T08 exposure joins");
    assert!(
        artifact
            .exposed
            .iter()
            .all(|building| building.building_id != "1006497"),
        "T08 exact-half building 1006497 must not be exposed by strict majority: {:?}",
        artifact.exposed
    );

    let ring34 = advisory
        .wind_radii
        .iter()
        .find(|radius| radius.knots == 34)
        .expect("34-knot ring");
    let centroid = centroid(&geometry.buildings["1006497"]);
    let predicate_ring = predicate_ring(&advisory.frame_id, &ring34.ring);
    let point_location = predicate_ring
        .locate_point(&advisory.frame_id, centroid)
        .expect("centroid locates");
    assert_ne!(
        point_location,
        GeoPointLocation::Exterior,
        "T08 a centroid-or-boundary join would call 1006497 exposed; exact area should not"
    );
}

fn assert_classifies_each_fixture_building_once(artifact: &GeoEventExposure) {
    let mut classified = BTreeSet::new();
    for building in &artifact.exposed {
        assert!(
            classified.insert(building.building_id.as_str()),
            "duplicate exposed building {}",
            building.building_id
        );
    }
    for building in &artifact.not_exposed {
        assert!(
            classified.insert(building.building_id.as_str()),
            "duplicate not-exposed building {}",
            building.building_id
        );
    }
    for building_id in &artifact.buildings_without_geometry {
        assert!(
            classified.insert(building_id.as_str()),
            "duplicate missing-geometry building {building_id}"
        );
    }
    assert_eq!(
        classified,
        BTreeSet::from([
            "1006494", "1006495", "1006496", "1006497", "1006498", "1006499"
        ]),
        "T08 fixture denominator must classify six ledger buildings exactly once"
    );
}

fn fixture_ledger() -> GeoCollateralLedger {
    let digest = prefixed_blake3(b"fixture.case4.ledger.artifact");
    let row = GeoLedgerRow {
        version: CANON_GEO_COLLATERAL_LEDGER_VERSION.to_string(),
        accession: ACCESSION.to_string(),
        deal_id: DEAL_ID.to_string(),
        loan_id: LOAN_ID.to_string(),
        reach: GeoCandidateReachStatus::Full,
        reach_none_reason: None,
        parcel_set: Some(vec![
            "1004540041".to_string(),
            "1004540042".to_string(),
            "1004540043".to_string(),
            "1004540044".to_string(),
            "1004540045".to_string(),
            "1004540046".to_string(),
        ]),
        building_set: Some(vec![
            "1006494".to_string(),
            "1006495".to_string(),
            "1006496".to_string(),
            "1006497".to_string(),
            "1006498".to_string(),
            "1006499".to_string(),
        ]),
        deed_ids: Vec::new(),
        truth_plane: Some(GeoTruthPlane::GateV2Historical),
        claim_class: GeoClaimClass::CollateralComposition,
        residual_model_count: 1,
        count_exact: true,
        backbone_complete: true,
        last_observed_present: None,
        source_release_pins: vec![GeoSourceReleasePin {
            source_dataset: "fixture.case4.ledger".to_string(),
            source_release: "case4-fixture-release".to_string(),
            blake3: digest.clone(),
        }],
        composition_blake3: digest.clone(),
        evidence_blake3: digest,
        ambiguous_parcel_set: Vec::new(),
        ambiguous_building_set: Vec::new(),
        property_refs: Vec::new(),
        composition_status: GeoCompositionStatus::Resolved,
    };
    build_collateral_ledger(vec![row], GeoCollateralLedgerProofClass::Fixture)
        .expect("fixture ledger builds")
}

fn case4_geometry() -> Case4GeometryFixture {
    serde_json::from_str(CASE4_GEOMETRY).expect("case4 geometry fixture parses")
}

fn synthetic_advisory() -> GeoAdvisoryPin {
    serde_json::from_str(SYNTHETIC_ADV12).expect("synthetic advisory parses")
}

fn geometry_input() -> GeoExposureGeometryInput {
    let fixture = case4_geometry();
    assert_eq!(fixture.frame_id, FRAME_ID);
    GeoExposureGeometryInput::from_polygons(fixture.frame_id, &fixture.buildings)
}

fn point_geometry_input() -> GeoExposureGeometryInput {
    let fixture = case4_geometry();
    GeoExposureGeometryInput {
        frame_id: fixture.frame_id,
        buildings: fixture
            .buildings
            .iter()
            .map(|(building_id, polygon)| {
                (
                    building_id.clone(),
                    GeoCanonicalGeometryMm::Point {
                        coordinate: centroid(polygon),
                    },
                )
            })
            .collect(),
    }
}

fn fixture_archive(advisory: &GeoAdvisoryPin) -> GeoAdvisoryArchive {
    GeoAdvisoryArchive {
        source_blake3s: advisory.source_blake3s.clone(),
        advisories: vec![canon::geo::GeoArchivedAdvisoryRef {
            storm_id: advisory.storm_id.clone(),
            advisory_number: advisory.advisory_number,
            source_blake3s: advisory.source_blake3s.clone(),
        }],
    }
}

fn predicate_ring(frame_id: &str, polygon: &GeoCanonicalPolygonMm) -> GeoLinearRingMm {
    let mut closed = polygon.exterior.vertices.clone();
    closed.push(closed[0]);
    GeoLinearRingMm::new(frame_id.to_string(), closed).expect("fixture ring converts")
}

fn centroid(polygon: &GeoCanonicalPolygonMm) -> GeoPointMm {
    let min_x = polygon
        .exterior
        .vertices
        .iter()
        .map(|point| point.x)
        .min()
        .expect("polygon x");
    let max_x = polygon
        .exterior
        .vertices
        .iter()
        .map(|point| point.x)
        .max()
        .expect("polygon x");
    let min_y = polygon
        .exterior
        .vertices
        .iter()
        .map(|point| point.y)
        .min()
        .expect("polygon y");
    let max_y = polygon
        .exterior
        .vertices
        .iter()
        .map(|point| point.y)
        .max()
        .expect("polygon y");
    GeoPointMm::new((min_x + max_x) / 2, (min_y + max_y) / 2)
}

fn prefixed_blake3(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}
