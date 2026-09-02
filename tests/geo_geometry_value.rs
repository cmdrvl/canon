#![forbid(unsafe_code)]

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use canon::geo::{
    CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION, CANON_GEO_GEOMETRY_REQUEST_VERSION,
    CANON_GEO_LOCAL_FRAME_VERSION, CANON_GEO_PROVIDER_TILE_BUILD_VERSION,
    CANON_GEO_PROVIDER_TILE_CONTRACT_VERSION, CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION,
    GeoAffineProjectionMm, GeoCanonicalGeometryMm, GeoClientTileCoverageExtent,
    GeoClientTileCoverageExtentKind, GeoClientTileIngestRequest, GeoClientTileMembershipRule,
    GeoClientTileSourceFormat, GeoClientTileVendorIdentifier, GeoExactSourceUnitMm,
    GeoFeatureValue, GeoGeometryBudgetEnforcement, GeoGeometryErrorCode, GeoGeometryFeatureInput,
    GeoGeometryTileRequest, GeoLicenseClass, GeoLocalFrameContract, GeoProjectionProvenance,
    GeoProviderGeometryFidelity, GeoProviderGeometryTileBuildRequest, GeoProviderTileCoverageState,
    GeoProviderTileDataBookDecision, GeoProviderTileFeatureContract,
    GeoProviderTileFeatureProvenance, GeoProviderTileFieldLocator, GeoProviderTileLicensePosture,
    GeoProviderTileRedactionClass, GeoProviderTileSource, GeoProviderTileSourceCoverage,
    GeoProviderTileSourceProvenance, GeoProviderTileSubsetKind, GeoProviderTileSubsetPredicate,
    GeoSourceAxisDomain, GeoSourceGeometry, GeoSourcePointDecimal, GeoSourcePointFixed,
    GeoWarehouseGeometryRow, GeoWarehouseGeometryRowsRequest, canonical_geometry_tile_bytes,
    canonical_warehouse_geometry_bytes, ingest_client_geometry_tile, materialize_geometry_tile,
    materialize_provider_geometry_tile, materialize_warehouse_geometry,
};
use h3o::{LatLng, Resolution};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use std::str::FromStr;

const CRS: &str = "LOCAL:TEST-METRES";

#[test]
fn equivalent_polygon_encodings_have_identical_canonical_bytes_and_hashes() {
    let first = polygon(vec![
        point("0", "0"),
        point("10", "0"),
        point("10", "10"),
        point("0", "10"),
        point("0", "0"),
    ]);
    let second = polygon(vec![
        point("10.000", "10.000"),
        point("10.000", "0.000"),
        point("0.000", "0.000"),
        point("0.000", "10.000"),
        point("0.000", "10.000"),
        point("10.000", "10.000"),
    ]);

    let first = materialize_geometry_tile(&request(frame(3, 1, 0), first, 100, 100_000))
        .expect("first encoding materializes");
    let second = materialize_geometry_tile(&request(frame(3, 1, 0), second, 100, 100_000))
        .expect("second encoding materializes");
    let first_bytes = canonical_geometry_tile_bytes(&first).unwrap();
    let second_bytes = canonical_geometry_tile_bytes(&second).unwrap();

    assert_eq!(first_bytes, second_bytes);
    assert_eq!(blake3::hash(&first_bytes), blake3::hash(&second_bytes));
    assert_eq!(first.total_canonical_vertices, 4);
    let GeoFeatureValue::Geometry { value } = &first.features[0].value;
    let GeoCanonicalGeometryMm::Polygon { polygon } = &value.geometry else {
        panic!("expected polygon");
    };
    assert_eq!(polygon.exterior.vertices[0].x, 0);
    assert_eq!(polygon.exterior.vertices[0].y, 0);
    assert_eq!(value.bbox.max_x, 10_000);
    assert_eq!(value.bbox.max_y, 10_000);
}

#[test]
fn hole_and_multipolygon_order_are_canonical_and_topology_is_validated() {
    let left = source_polygon(
        rectangle("0", "0", "20", "20", false),
        vec![rectangle("2", "2", "4", "4", false)],
    );
    let right = source_polygon(rectangle("30", "0", "40", "10", true), vec![]);
    let first = GeoSourceGeometry::MultiPolygon {
        polygons: vec![right.clone(), left.clone()],
    };
    let second = GeoSourceGeometry::MultiPolygon {
        polygons: vec![
            source_polygon(
                rotate_closed(rectangle("0.000", "0", "20", "20", true), 2),
                vec![rotate_closed(rectangle("2.000", "2", "4", "4", true), 1)],
            ),
            source_polygon(
                rotate_closed(rectangle("30.000", "0", "40", "10", false), 3),
                vec![],
            ),
        ],
    };

    let first = materialize_geometry_tile(&request(frame(3, 1, 0), first, 100, 100_000))
        .expect("first multipolygon materializes");
    let second = materialize_geometry_tile(&request(frame(3, 1, 0), second, 100, 100_000))
        .expect("second multipolygon materializes");
    assert_eq!(
        canonical_geometry_tile_bytes(&first).unwrap(),
        canonical_geometry_tile_bytes(&second).unwrap()
    );

    let overlapping = GeoSourceGeometry::MultiPolygon {
        polygons: vec![
            left,
            source_polygon(rectangle("10", "10", "25", "25", false), vec![]),
        ],
    };
    let error = materialize_geometry_tile(&request(frame(3, 1, 0), overlapping, 100, 100_000))
        .expect_err("overlapping members must refuse");
    assert_eq!(error.code, GeoGeometryErrorCode::PolygonIntersection);
}

#[test]
fn quantization_audit_measures_snap_loss_against_a_five_metre_extent() {
    let geometry = polygon(vec![
        point("0.000000", "0.000000"),
        point("5.000499", "0.000000"),
        point("5.000499", "5.000499"),
        point("0.000000", "5.000499"),
        point("0.000000", "0.000000"),
    ]);
    let artifact =
        materialize_geometry_tile(&request(frame(6, 1_000, 200), geometry, 100, 100_000))
            .expect("micrometre source geometry materializes");
    let GeoFeatureValue::Geometry { value } = &artifact.features[0].value;

    assert_eq!(value.bbox.max_x, 5_000);
    assert_eq!(value.bbox.max_y, 5_000);
    assert_eq!(value.quantization.max_abs_snap_error_numerator_mm, 499);
    assert_eq!(value.quantization.affine_denominator, 1_000);
    assert_eq!(
        value.quantization.max_abs_snap_error_micrometres_ceiling,
        499
    );
    assert_eq!(value.quantization.combined_error_envelope_micrometres, 699);
    assert_eq!(
        value.quantization.minimum_nonzero_bbox_extent_mm,
        Some(5_000)
    );
    assert_eq!(
        value.quantization.endpoint_distance_error_ppm_upper_bound,
        Some(420)
    );
}

#[test]
fn half_millimetre_ties_round_to_even_and_feature_order_does_not_change_bytes() {
    let mut request = request(
        frame(6, 1_000, 0),
        GeoSourceGeometry::Point {
            coordinate: point("0.000500", "0"),
        },
        10,
        100_000,
    );
    request.features[0].feature_id = "point-zero".to_string();
    request.features.extend([
        GeoGeometryFeatureInput {
            feature_id: "point-positive-two".to_string(),
            source_crs: CRS.to_string(),
            geometry: GeoSourceGeometry::Point {
                coordinate: point("0.001500", "0"),
            },
        },
        GeoGeometryFeatureInput {
            feature_id: "point-negative-two".to_string(),
            source_crs: CRS.to_string(),
            geometry: GeoSourceGeometry::Point {
                coordinate: point("-0.001500", "0"),
            },
        },
    ]);
    let mut reversed = request.clone();
    reversed.features.reverse();

    let artifact = materialize_geometry_tile(&request).expect("ties materialize");
    let reversed = materialize_geometry_tile(&reversed).expect("reordered ties materialize");
    assert_eq!(
        canonical_geometry_tile_bytes(&artifact).unwrap(),
        canonical_geometry_tile_bytes(&reversed).unwrap()
    );
    let coordinates = artifact
        .features
        .iter()
        .map(|feature| {
            let GeoFeatureValue::Geometry { value } = &feature.value;
            let GeoCanonicalGeometryMm::Point { coordinate } = value.geometry else {
                panic!("expected point");
            };
            (feature.feature_id.as_str(), coordinate.x)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        coordinates,
        [
            ("point-negative-two", -2),
            ("point-positive-two", 2),
            ("point-zero", 0),
        ]
    );
}

#[test]
fn nonfinite_precision_mixed_crs_antimeridian_and_invalid_rings_refuse() {
    let nonfinite = GeoSourceGeometry::Point {
        coordinate: point("NaN", "0"),
    };
    let error =
        materialize_geometry_tile(&request(frame(3, 1, 0), nonfinite, 10, 100_000)).unwrap_err();
    assert_eq!(error.code, GeoGeometryErrorCode::NonFiniteCoordinate);

    let precision = GeoSourceGeometry::Point {
        coordinate: point("1.0001", "0"),
    };
    let error =
        materialize_geometry_tile(&request(frame(3, 1, 0), precision, 10, 100_000)).unwrap_err();
    assert_eq!(error.code, GeoGeometryErrorCode::SourcePrecisionExceeded);

    let mut mixed = request(
        frame(3, 1, 0),
        GeoSourceGeometry::Point {
            coordinate: point("0", "0"),
        },
        10,
        100_000,
    );
    mixed.features[0].source_crs = "EPSG:4326".to_string();
    let error = materialize_geometry_tile(&mixed).unwrap_err();
    assert_eq!(error.code, GeoGeometryErrorCode::MixedCrs);

    let extreme = GeoSourceGeometry::Point {
        coordinate: point("-9223372036854775808", "0"),
    };
    let error = materialize_geometry_tile(&request(frame(0, 1, 0), extreme, 10, 100_000))
        .expect_err("extreme coordinate must refuse without abs overflow");
    assert_eq!(error.code, GeoGeometryErrorCode::InvalidFrame);

    let mut geographic = frame(3, 1, 0);
    geographic.source_crs = "EPSG:4326".to_string();
    geographic.source_axis_domain = GeoSourceAxisDomain::GeographicLongitudeLatitude;
    geographic.source_origin = GeoSourcePointFixed { x: 0, y: 0 };
    let crossing = GeoSourceGeometry::Polygon {
        exterior: vec![
            point("179", "0"),
            point("-179", "0"),
            point("-179", "1"),
            point("179", "0"),
        ],
        holes: vec![],
    };
    let mut crossing_request = request(geographic, crossing, 10, 100_000);
    crossing_request.features[0].source_crs = "EPSG:4326".to_string();
    let error = materialize_geometry_tile(&crossing_request).unwrap_err();
    assert_eq!(error.code, GeoGeometryErrorCode::AntimeridianCrossing);

    let unclosed = polygon(vec![point("0", "0"), point("10", "0"), point("0", "10")]);
    let error =
        materialize_geometry_tile(&request(frame(3, 1, 0), unclosed, 10, 100_000)).unwrap_err();
    assert_eq!(error.code, GeoGeometryErrorCode::UnclosedRing);

    let bow_tie = polygon(vec![
        point("0", "0"),
        point("4", "4"),
        point("0", "4"),
        point("4", "0"),
        point("4", "-1"),
        point("0", "0"),
    ]);
    let error = materialize_geometry_tile(&request(frame(3, 1, 0), bow_tie, 10, 100_000))
        .expect_err("self-intersection must refuse, not repair");
    assert_eq!(error.code, GeoGeometryErrorCode::SelfIntersection);
}

#[test]
fn vertex_and_tile_byte_budgets_refuse_without_truncating_geometry() {
    let geometry = polygon(rectangle("0", "0", "10", "10", false));
    let error = materialize_geometry_tile(&request(frame(3, 1, 0), geometry.clone(), 4, 100_000))
        .expect_err("explicit closure counts against the raw vertex budget");
    assert_eq!(error.code, GeoGeometryErrorCode::VertexBudgetExceeded);
    let breach = error.budget.expect("typed vertex budget breach");
    assert_eq!(breach.observed, 5);
    assert_eq!(breach.configured, 4);
    assert_eq!(
        breach.enforcement,
        GeoGeometryBudgetEnforcement::RefuseBeforeMaterialization
    );

    let error = materialize_geometry_tile(&request(frame(3, 1, 0), geometry, 10, 1))
        .expect_err("one-byte tile budget cannot fit decision geometry");
    assert_eq!(error.code, GeoGeometryErrorCode::TileByteBudgetExceeded);
    let breach = error.budget.expect("typed byte budget breach");
    assert!(breach.observed > breach.configured);
    assert_eq!(
        breach.enforcement,
        GeoGeometryBudgetEnforcement::RefuseBeforeOutput
    );
}

#[test]
fn warehouse_wkb_builds_exact_epsg2263_frame_with_separate_loss_receipts() {
    let mut request = warehouse_request(vec![warehouse_row(
        "parcel-1",
        "mn/000000/1",
        &polygon_wkb(&[
            (980_252.301_632_881_2, 191_655.610_172_272_3),
            (980_352.301_632_881_2, 191_655.610_172_272_3),
            (980_352.301_632_881_2, 191_755.610_172_272_3),
            (980_252.301_632_881_2, 191_755.610_172_272_3),
            (980_252.301_632_881_2, 191_655.610_172_272_3),
        ]),
    )]);
    let first = materialize_warehouse_geometry(&request).expect("warehouse WKB materializes");
    request.rows.reverse();
    let second = materialize_warehouse_geometry(&request).expect("reordered rows materialize");

    assert_eq!(
        canonical_warehouse_geometry_bytes(&first).unwrap(),
        canonical_warehouse_geometry_bytes(&second).unwrap()
    );
    assert_eq!(
        first.geometry_tile.frame.projection.method_id,
        "canon:planar-source-affine"
    );
    assert_eq!(
        first
            .geometry_tile
            .frame
            .projection
            .max_projection_error_micrometres,
        0
    );
    assert_eq!(
        first.geometry_tile.frame.affine.x_from_source_x_numerator,
        3
    );
    assert_eq!(first.geometry_tile.frame.affine.denominator, 9_842_500);
    assert_eq!(
        first
            .source_receipt
            .max_abs_source_quantization_error_micrometres_ceiling,
        1
    );
    let GeoFeatureValue::Geometry { value } = &first.geometry_tile.features[0].value;
    assert!(value.quantization.max_abs_snap_error_micrometres_ceiling <= 500);
    assert_eq!(value.quantization.projection_error_envelope_micrometres, 0);
    assert_eq!(first.source_receipt.rows[0].decoded_vertex_count, 5);
    assert_eq!(
        first.source_receipt.transform_execution_id,
        "sha256-execution-26v2"
    );

    let mut accreted_request = request.clone();
    accreted_request.rows.push(warehouse_row(
        "parcel-2",
        "mn/000000/2",
        &polygon_wkb(&[
            (981_000.0, 192_000.0),
            (981_010.0, 192_000.0),
            (981_010.0, 192_010.0),
            (981_000.0, 192_010.0),
            (981_000.0, 192_000.0),
        ]),
    ));
    let accreted = materialize_warehouse_geometry(&accreted_request)
        .expect("new evidence materializes in the stable frame");
    assert_eq!(first.geometry_tile.frame, accreted.geometry_tile.frame);
    assert_eq!(
        first.geometry_tile.features[0],
        accreted.geometry_tile.features[0]
    );
}

#[test]
fn warehouse_wkb_refuses_digest_drift_mixed_execution_and_unsupported_type() {
    let bytes = polygon_wkb(&[
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 10.0),
        (0.0, 10.0),
        (0.0, 0.0),
    ]);
    let mut bad_digest = warehouse_request(vec![warehouse_row("parcel-1", "mn/000000/1", &bytes)]);
    bad_digest.rows[0].source_geom_wkb_sha256 = "0".repeat(64);
    let error = materialize_warehouse_geometry(&bad_digest)
        .expect_err("digest mismatch must refuse before decoding");
    assert_eq!(error.code, GeoGeometryErrorCode::InvalidSourceDigest);

    let mut second = warehouse_row("parcel-2", "mn/000000/2", &bytes);
    second.transform_execution_id = "sha256-execution-other".to_string();
    let mixed = warehouse_request(vec![
        warehouse_row("parcel-1", "mn/000000/1", &bytes),
        second,
    ]);
    let error =
        materialize_warehouse_geometry(&mixed).expect_err("mixed release execution must refuse");
    assert_eq!(error.code, GeoGeometryErrorCode::MixedSourceExecution);

    let mut line_string = Vec::new();
    line_string.push(1);
    line_string.extend_from_slice(&2_u32.to_le_bytes());
    line_string.extend_from_slice(&2_u32.to_le_bytes());
    for (x, y) in [(0.0_f64, 0.0_f64), (1.0, 1.0)] {
        line_string.extend_from_slice(&x.to_le_bytes());
        line_string.extend_from_slice(&y.to_le_bytes());
    }
    let unsupported = warehouse_request(vec![warehouse_row("line-1", "mn/000000/3", &line_string)]);
    let error =
        materialize_warehouse_geometry(&unsupported).expect_err("unsupported WKB type must refuse");
    assert_eq!(error.code, GeoGeometryErrorCode::UnsupportedGeometryType);

    let mut excessive_ring_count = Vec::new();
    excessive_ring_count.push(1);
    excessive_ring_count.extend_from_slice(&3_u32.to_le_bytes());
    excessive_ring_count.extend_from_slice(&100_u32.to_le_bytes());
    let mut bounded = warehouse_request(vec![warehouse_row(
        "parcel-1",
        "mn/000000/4",
        &excessive_ring_count,
    )]);
    bounded.max_vertices_per_geometry = 10;
    let error = materialize_warehouse_geometry(&bounded)
        .expect_err("container count must refuse before a large allocation");
    assert_eq!(error.code, GeoGeometryErrorCode::VertexBudgetExceeded);
}

#[test]
fn provider_tile_contract_preserves_subset_license_provenance_and_losing_geometry() {
    let request = provider_tile_build_request(GeoProviderGeometryFidelity::SourceFidelity, false);
    let mut reordered = request.clone();
    reordered.geometry_request.features.reverse();
    reordered.sources.reverse();
    reordered.subset.work_cells.reverse();
    reordered.subset.source_coverages.reverse();
    reordered.feature_contracts.reverse();
    reordered.license_posture.attribution_requirements.reverse();
    reordered
        .license_posture
        .client_restricted_source_ids
        .reverse();

    let artifact = materialize_provider_geometry_tile(&request)
        .expect("offline provider geometry tile materializes");
    let reordered = materialize_provider_geometry_tile(&reordered)
        .expect("reordered provider geometry tile materializes");

    assert_eq!(
        canonical_geometry_tile_bytes(&artifact).unwrap(),
        canonical_geometry_tile_bytes(&reordered).unwrap()
    );
    assert_eq!(artifact.features.len(), 2);
    assert_eq!(
        artifact
            .features
            .iter()
            .map(|feature| feature.feature_id.as_str())
            .collect::<Vec<_>>(),
        ["building-1", "parcel-1"]
    );

    let provider = artifact
        .provider_tile
        .as_ref()
        .expect("provider tile contract is attached");
    assert_eq!(provider.version, CANON_GEO_PROVIDER_TILE_CONTRACT_VERSION);
    assert_eq!(
        provider.databook_decision,
        GeoProviderTileDataBookDecision::DatabookLikeSelfContainedTileNoNewDependency
    );
    assert_eq!(
        provider.subset.kind,
        GeoProviderTileSubsetKind::H3CellSetAndSourceCoverageIntersection
    );
    assert_eq!(provider.subset.work_cells, ["892a100d26bffff"]);
    assert_eq!(provider.subset.source_coverages.len(), 2);
    assert_eq!(provider.features.len(), artifact.features.len());
    assert_eq!(provider.tile_content_blake3.len(), 64);
    assert!(
        provider
            .license_posture
            .attribution_requirements
            .iter()
            .any(|requirement| requirement == "ORNL and FEMA Geospatial Response Office")
    );
    assert_eq!(
        provider.license_posture.client_restricted_source_ids,
        ["source.client.parcels"]
    );

    let building = provider
        .features
        .iter()
        .find(|feature| feature.feature_id == "building-1")
        .expect("building feature contract");
    assert_eq!(
        building.decision_geometry_fidelity,
        GeoProviderGeometryFidelity::SourceFidelity
    );
    assert_eq!(
        building.display_geometry_fidelity,
        Some(GeoProviderGeometryFidelity::DisplaySimplified)
    );
    assert_eq!(
        building.redaction_class,
        GeoProviderTileRedactionClass::ShareableAttributionRequired
    );
    assert_eq!(
        building.provenance.source_path,
        "tiles/fema/892a100d26bffff.jsonl"
    );
    assert_eq!(building.provenance.record_ordinal, 7);
    assert_eq!(
        building.provenance.field_locators[0].field_path,
        "$.geometry"
    );

    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../schemas/canon.geo.geometry_tile.v0.schema.json"
    ))
    .expect("geometry tile schema parses");
    assert!(schema["properties"]["provider_tile"].is_object());
    assert!(schema["$defs"]["provider_feature_contract"].is_object());
}

#[test]
fn provider_tile_refuses_unacknowledged_vendor_simplified_decision_geometry() {
    let request = provider_tile_build_request(GeoProviderGeometryFidelity::VendorSimplified, false);
    let error = materialize_provider_geometry_tile(&request)
        .expect_err("silent vendor-simplified decision geometry must refuse");

    assert_eq!(error.code, GeoGeometryErrorCode::InvalidTileContract);
    assert_eq!(
        error.detail.get("feature_id").map(String::as_str),
        Some("parcel-1")
    );

    let acknowledged =
        provider_tile_build_request(GeoProviderGeometryFidelity::VendorSimplified, true);
    materialize_provider_geometry_tile(&acknowledged)
        .expect("explicit acknowledgement admits labelled vendor-simplified decision geometry");
}

#[test]
fn client_tile_ingest_indexes_geojson_with_declared_coverage_and_local_license_boundary() {
    let (request, source_bytes, center, neighbor) = client_tile_ingest_fixture();
    let artifact = ingest_client_geometry_tile(&request, source_bytes.as_bytes())
        .expect("declared WGS84 client GeoJSON ingests");
    let repeated = ingest_client_geometry_tile(&request, source_bytes.as_bytes())
        .expect("same client GeoJSON ingests twice");

    assert_eq!(
        canonical_geometry_tile_bytes(&artifact).unwrap(),
        canonical_geometry_tile_bytes(&repeated).unwrap()
    );
    assert_eq!(artifact.version, "canon_geo_geometry_tile.v0");
    assert_eq!(
        artifact
            .features
            .iter()
            .map(|feature| feature.feature_id.as_str())
            .collect::<Vec<_>>(),
        ["client-apn-1", "client-apn-2"]
    );

    let provider = artifact.provider_tile.as_ref().expect("provider tile");
    assert_eq!(provider.tile_id, center);
    assert_eq!(
        provider.license_posture.client_restricted_source_ids,
        ["source.client.parcels"]
    );
    assert!(
        provider
            .tile_content_blake3
            .chars()
            .all(|ch| ch.is_ascii_hexdigit())
    );

    let ingest = provider
        .client_ingest
        .as_ref()
        .expect("client ingest report");
    assert_eq!(ingest.source_format, GeoClientTileSourceFormat::GeoJson);
    assert_eq!(ingest.declared_crs, "EPSG:4326");
    assert_eq!(ingest.transform.h3_library, "h3o=0.10.0");
    assert_eq!(
        ingest.coverage_extent.kind,
        GeoClientTileCoverageExtentKind::ClientDeclaredH3CellSet
    );
    assert_eq!(ingest.summary.validation.source_feature_count, 2);
    assert_eq!(ingest.summary.validation.accepted_feature_count, 2);
    assert_eq!(ingest.summary.validation.refused_feature_count, 0);
    assert_eq!(ingest.summary.anchor_membership_count, 2);
    assert_eq!(ingest.summary.supplemental_membership_count, 1);
    assert_eq!(ingest.summary.membership_row_count, 3);
    assert_eq!(
        ingest
            .aliases
            .iter()
            .map(|alias| (alias.alias_namespace.as_str(), alias.alias_value.as_str()))
            .collect::<Vec<_>>(),
        [
            ("county:apn", "client-apn-1"),
            ("county:apn", "client-apn-2")
        ]
    );
    assert!(ingest.memberships.iter().any(|membership| {
        membership.source_feature_id == "client-apn-1"
            && membership.h3_cell == center
            && membership.rule == GeoClientTileMembershipRule::RepresentativePointAnchor
    }));
    assert!(ingest.memberships.iter().any(|membership| {
        membership.source_feature_id == "client-apn-1"
            && membership.h3_cell == neighbor
            && membership.rule == GeoClientTileMembershipRule::DeclaredSupplementalCoverage
    }));
    assert!(provider.subset.source_coverages.iter().all(|coverage| {
        coverage.source_instance_id == "source.client.parcels"
            && coverage.coverage_state == GeoProviderTileCoverageState::Complete
    }));
    assert!(provider.features.iter().all(|feature| {
        feature.license_class == GeoLicenseClass::RestrictedLocalOnly
            && feature.redaction_class == GeoProviderTileRedactionClass::LocalOnly
    }));
}

#[test]
fn client_tile_ingest_reports_bad_features_without_inventing_membership() {
    let (mut request, source_bytes, _center, _neighbor) = client_tile_ingest_fixture();
    let source: serde_json::Value = serde_json::from_str(&source_bytes).expect("fixture json");
    let mut features = source["features"].as_array().unwrap().clone();
    features.push(json!({
        "type": "Feature",
        "id": "bad-empty",
        "properties": { "apn": "bad-empty", "supplemental_cells": [] },
        "geometry": { "type": "Polygon", "coordinates": [] }
    }));
    features.push(json!({
        "type": "Feature",
        "id": "bad-empty-supplemental",
        "properties": { "apn": "bad-empty-supplemental", "supplemental_cells": [] },
        "geometry": {
            "type": "Polygon",
            "coordinates": [[
                [-73.977050, 40.752950],
                [-73.976950, 40.752950],
                [-73.976950, 40.753050],
                [-73.977050, 40.753050],
                [-73.977050, 40.752950]
            ]]
        }
    }));
    let source = json!({ "type": "FeatureCollection", "features": features }).to_string();
    request.source_digest = blake3_hex(source.as_bytes());

    let artifact = ingest_client_geometry_tile(&request, source.as_bytes())
        .expect("partly valid client ingest still emits validation summary");
    let ingest = artifact
        .provider_tile
        .as_ref()
        .and_then(|provider| provider.client_ingest.as_ref())
        .expect("client ingest report");

    assert_eq!(ingest.summary.validation.source_feature_count, 4);
    assert_eq!(ingest.summary.validation.accepted_feature_count, 2);
    assert_eq!(ingest.summary.validation.refused_feature_count, 2);
    assert_eq!(
        ingest.summary.validation.refusal_counts,
        [
            canon::geo::GeoClientTileValidationRefusalCount {
                reason: GeoGeometryErrorCode::EmptyGeometry,
                count: 1,
            },
            canon::geo::GeoClientTileValidationRefusalCount {
                reason: GeoGeometryErrorCode::InvalidTileContract,
                count: 1,
            }
        ]
    );
    assert!(
        !ingest
            .memberships
            .iter()
            .any(|membership| membership.source_feature_id == "bad-empty")
    );
    assert!(
        !ingest
            .memberships
            .iter()
            .any(|membership| membership.source_feature_id == "bad-empty-supplemental")
    );
}

#[test]
fn client_tile_ingest_refuses_missing_coverage_extent_crs_drift_and_relabels() {
    let (mut request, source_bytes, _center, _neighbor) = client_tile_ingest_fixture();
    request.coverage_extent.h3_cells.clear();
    let error = ingest_client_geometry_tile(&request, source_bytes.as_bytes())
        .expect_err("coverage extent is required and cannot be inferred from parcel rows");
    assert_eq!(error.code, GeoGeometryErrorCode::InvalidTileContract);

    let (mut request, source_bytes, _center, _neighbor) = client_tile_ingest_fixture();
    request.declared_crs = "EPSG:3857".to_string();
    let error = ingest_client_geometry_tile(&request, source_bytes.as_bytes())
        .expect_err("non-WGS84 client layers are not silently reprojected in v0");
    assert_eq!(error.code, GeoGeometryErrorCode::MixedCrs);

    let (mut request, source_bytes, _center, _neighbor) = client_tile_ingest_fixture();
    request.source_digest = blake3_hex(b"other-bytes");
    let error = ingest_client_geometry_tile(&request, source_bytes.as_bytes())
        .expect_err("source bytes must match the declared client digest");
    assert_eq!(error.code, GeoGeometryErrorCode::InvalidSourceDigest);

    let (mut request, source_bytes, _center, _neighbor) = client_tile_ingest_fixture();
    let source: serde_json::Value = serde_json::from_str(&source_bytes).expect("fixture json");
    let mut features = source["features"].as_array().unwrap().clone();
    features[1]["properties"]["apn"] = json!("client-apn-1");
    let source = json!({ "type": "FeatureCollection", "features": features }).to_string();
    request.source_digest = blake3_hex(source.as_bytes());
    let error = ingest_client_geometry_tile(&request, source.as_bytes())
        .expect_err("duplicate vendor identifiers must not be relabeled into one feature");
    assert_eq!(error.code, GeoGeometryErrorCode::InvalidSourceProvenance);
}

fn client_tile_ingest_fixture() -> (GeoClientTileIngestRequest, String, String, String) {
    let resolution = Resolution::Nine;
    let center = h3_cell_for_lon_lat(-73.977000, 40.753000, resolution);
    let mut work_cells = h3o::CellIndex::from_str(&center)
        .unwrap()
        .grid_disk::<Vec<_>>(1)
        .into_iter()
        .map(|cell| cell.to_string())
        .collect::<Vec<_>>();
    work_cells.sort();
    let neighbor = work_cells
        .iter()
        .find(|cell| cell.as_str() != center.as_str())
        .expect("k1 neighbor")
        .clone();
    let source = json!({
        "type": "FeatureCollection",
        "features": [
            {
                "type": "Feature",
                "id": "record-2",
                "properties": { "apn": "client-apn-2" },
                "geometry": {
                    "type": "Polygon",
                    "coordinates": [[
                        [-73.977050, 40.752950],
                        [-73.976950, 40.752950],
                        [-73.976950, 40.753050],
                        [-73.977050, 40.753050],
                        [-73.977050, 40.752950]
                    ]]
                }
            },
            {
                "type": "Feature",
                "id": "record-1",
                "properties": {
                    "apn": "client-apn-1",
                    "supplemental_cells": [neighbor]
                },
                "geometry": {
                    "type": "Polygon",
                    "coordinates": [[
                        [-73.977100, 40.752900],
                        [-73.976900, 40.752900],
                        [-73.976900, 40.753100],
                        [-73.977100, 40.753100],
                        [-73.977100, 40.752900]
                    ]]
                }
            }
        ]
    })
    .to_string();
    let request = GeoClientTileIngestRequest {
        version: CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION.to_string(),
        tile_id: center.clone(),
        source_format: GeoClientTileSourceFormat::GeoJson,
        source_path: "client/parcels.geojson".to_string(),
        source_digest: blake3_hex(source.as_bytes()),
        declared_crs: "EPSG:4326".to_string(),
        frame: wgs84_client_frame(&center),
        source_instance_id: "source.client.parcels".to_string(),
        release_id: "client-parcels-2026-q3".to_string(),
        release_digest: blake3_hex(b"client-parcels-2026-q3"),
        vendor: "county".to_string(),
        vintage: "2026-Q3".to_string(),
        vendor_identifier: GeoClientTileVendorIdentifier {
            issuer: "county".to_string(),
            role: "apn".to_string(),
            property: "apn".to_string(),
        },
        source_record_id_property: None,
        supplemental_h3_cells_property: Some("supplemental_cells".to_string()),
        license_expression: "LicenseRef-Client-Parcel-Local".to_string(),
        coverage_extent: GeoClientTileCoverageExtent {
            extent_id: "client-declared-h3-k1".to_string(),
            kind: GeoClientTileCoverageExtentKind::ClientDeclaredH3CellSet,
            h3_cells: work_cells,
        },
        mutual_exclusivity_declared: false,
        h3_resolution: 9,
        halo_k: 1,
        work_cells: h3o::CellIndex::from_str(&center)
            .unwrap()
            .grid_disk::<Vec<_>>(1)
            .into_iter()
            .map(|cell| cell.to_string())
            .collect(),
        max_features: 8,
        max_vertices_per_geometry: 64,
        max_geometry_bytes_per_tile: 100_000,
    };
    (request, source, center, neighbor)
}

fn h3_cell_for_lon_lat(longitude: f64, latitude: f64, resolution: Resolution) -> String {
    LatLng::new(latitude, longitude)
        .unwrap()
        .to_cell(resolution)
        .to_string()
}

fn wgs84_client_frame(tile_id: &str) -> GeoLocalFrameContract {
    GeoLocalFrameContract {
        version: CANON_GEO_LOCAL_FRAME_VERSION.to_string(),
        frame_id: format!("client:{tile_id}:wgs84-local-affine:v0"),
        tile_id: tile_id.to_string(),
        source_crs: "EPSG:4326".to_string(),
        source_axis_domain: GeoSourceAxisDomain::GeographicLongitudeLatitude,
        source_decimal_places: 6,
        source_origin: GeoSourcePointFixed {
            x: -74_000_000,
            y: 40_000_000,
        },
        affine: GeoAffineProjectionMm {
            x_from_source_x_numerator: 1,
            x_from_source_y_numerator: 0,
            y_from_source_x_numerator: 0,
            y_from_source_y_numerator: 1,
            denominator: 1,
        },
        projection: GeoProjectionProvenance {
            method_id: "fixture:wgs84-local-affine".to_string(),
            method_version: "v0".to_string(),
            parameters_blake3: blake3_hex(format!("fixture:{tile_id}").as_bytes()),
            max_projection_error_micrometres: 10_000_000,
        },
        max_abs_coordinate_mm: 10_000_000,
    }
}

fn provider_tile_build_request(
    parcel_decision_fidelity: GeoProviderGeometryFidelity,
    allow_vendor_simplified_decision_geometry: bool,
) -> GeoProviderGeometryTileBuildRequest {
    let fema_tile_digest = blake3_hex(b"fema-source-tile");
    let client_source_digest = blake3_hex(b"client-parcel-layer");
    let mut geometry_request = request(
        frame(3, 1, 0),
        polygon(rectangle("0", "0", "10", "10", false)),
        100,
        100_000,
    );
    geometry_request.features.push(GeoGeometryFeatureInput {
        feature_id: "building-1".to_string(),
        source_crs: CRS.to_string(),
        geometry: polygon(rectangle("2", "2", "4", "4", false)),
    });

    GeoProviderGeometryTileBuildRequest {
        version: CANON_GEO_PROVIDER_TILE_BUILD_VERSION.to_string(),
        tile_id: "892a100d26bffff".to_string(),
        geometry_request,
        subset: GeoProviderTileSubsetPredicate {
            kind: GeoProviderTileSubsetKind::H3CellSetAndSourceCoverageIntersection,
            predicate_id: "tile-892a100d26bffff-k0-byop".to_string(),
            h3_resolution: 9,
            center_cell: "892a100d26bffff".to_string(),
            halo_k: 0,
            work_cells: vec!["892a100d26bffff".to_string()],
            source_coverages: vec![
                GeoProviderTileSourceCoverage {
                    source_instance_id: "source.client.parcels".to_string(),
                    h3_cell: "892a100d26bffff".to_string(),
                    coverage_state: GeoProviderTileCoverageState::Partial,
                },
                GeoProviderTileSourceCoverage {
                    source_instance_id: "source.fema.structures".to_string(),
                    h3_cell: "892a100d26bffff".to_string(),
                    coverage_state: GeoProviderTileCoverageState::Complete,
                },
            ],
        },
        sources: vec![
            GeoProviderTileSource {
                source_instance_id: "source.client.parcels".to_string(),
                release_id: "client-parcels-2026-q2".to_string(),
                release_digest: blake3_hex(b"client-parcels-release"),
                license_class: GeoLicenseClass::RestrictedLocalOnly,
                license_expression: "LicenseRef-Client-Parcel-Local".to_string(),
                attribution_required: false,
                attribution_text: None,
                provenance: GeoProviderTileSourceProvenance::ClientDeclared {
                    vendor: "client-declared-parcel-vendor".to_string(),
                    vintage: "2026-Q2".to_string(),
                    source_crs: CRS.to_string(),
                    coverage_extent: "h3:892a100d26bffff".to_string(),
                    mutual_exclusivity_declared: false,
                },
            },
            GeoProviderTileSource {
                source_instance_id: "source.fema.structures".to_string(),
                release_id: "fema-usa-structures-2026-fixture".to_string(),
                release_digest: blake3_hex(b"fema-release"),
                license_class: GeoLicenseClass::PublicAttributionRequired,
                license_expression: "CC-BY-4.0".to_string(),
                attribution_required: true,
                attribution_text: Some("ORNL and FEMA Geospatial Response Office".to_string()),
                provenance: GeoProviderTileSourceProvenance::CanonFullProvenance {
                    source_path: "tiles/fema/892a100d26bffff.jsonl".to_string(),
                    source_digest: fema_tile_digest.clone(),
                    source_record_count: 1,
                },
            },
        ],
        license_posture: GeoProviderTileLicensePosture {
            posture_id: "mixed-byop-local-v0".to_string(),
            output_license_expression: "LicenseRef-Mixed-BYOP-Local".to_string(),
            redistribution_notice:
                "Contains client-declared parcel geometry; raw tile stays local.".to_string(),
            attribution_requirements: vec!["ORNL and FEMA Geospatial Response Office".to_string()],
            client_restricted_source_ids: vec!["source.client.parcels".to_string()],
        },
        feature_contracts: vec![
            GeoProviderTileFeatureContract {
                feature_id: "parcel-1".to_string(),
                source_instance_id: "source.client.parcels".to_string(),
                source_feature_id: "client-parcel-1".to_string(),
                decision_geometry_fidelity: parcel_decision_fidelity,
                display_geometry_fidelity: Some(GeoProviderGeometryFidelity::DisplaySimplified),
                license_class: GeoLicenseClass::RestrictedLocalOnly,
                redaction_class: GeoProviderTileRedactionClass::LocalOnly,
                provenance: GeoProviderTileFeatureProvenance {
                    source_instance_id: "source.client.parcels".to_string(),
                    source_path: "client/parcels.gpkg".to_string(),
                    source_digest: client_source_digest,
                    source_record_id: "client-parcel-row-1".to_string(),
                    record_ordinal: 3,
                    field_locators: vec![GeoProviderTileFieldLocator {
                        field_path: "$.geometry".to_string(),
                        source_path: "client/parcels.gpkg".to_string(),
                        record_ordinal: 3,
                    }],
                },
            },
            GeoProviderTileFeatureContract {
                feature_id: "building-1".to_string(),
                source_instance_id: "source.fema.structures".to_string(),
                source_feature_id: "fema-building-1".to_string(),
                decision_geometry_fidelity: GeoProviderGeometryFidelity::SourceFidelity,
                display_geometry_fidelity: Some(GeoProviderGeometryFidelity::DisplaySimplified),
                license_class: GeoLicenseClass::PublicAttributionRequired,
                redaction_class: GeoProviderTileRedactionClass::ShareableAttributionRequired,
                provenance: GeoProviderTileFeatureProvenance {
                    source_instance_id: "source.fema.structures".to_string(),
                    source_path: "tiles/fema/892a100d26bffff.jsonl".to_string(),
                    source_digest: fema_tile_digest,
                    source_record_id: "fema-row-1".to_string(),
                    record_ordinal: 7,
                    field_locators: vec![GeoProviderTileFieldLocator {
                        field_path: "$.geometry".to_string(),
                        source_path: "tiles/fema/892a100d26bffff.jsonl".to_string(),
                        record_ordinal: 7,
                    }],
                },
            },
        ],
        allow_vendor_simplified_decision_geometry,
    }
}

fn request(
    frame: GeoLocalFrameContract,
    geometry: GeoSourceGeometry,
    max_vertices_per_geometry: u64,
    max_geometry_bytes_per_tile: u64,
) -> GeoGeometryTileRequest {
    GeoGeometryTileRequest {
        version: CANON_GEO_GEOMETRY_REQUEST_VERSION.to_string(),
        features: vec![GeoGeometryFeatureInput {
            feature_id: "parcel-1".to_string(),
            source_crs: frame.source_crs.clone(),
            geometry,
        }],
        frame,
        max_vertices_per_geometry,
        max_geometry_bytes_per_tile,
    }
}

fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn frame(
    source_decimal_places: u32,
    affine_denominator: u64,
    projection_error_micrometres: u64,
) -> GeoLocalFrameContract {
    GeoLocalFrameContract {
        version: CANON_GEO_LOCAL_FRAME_VERSION.to_string(),
        frame_id: "tile:892a100d26bffff:local-mm:v1".to_string(),
        tile_id: "892a100d26bffff".to_string(),
        source_crs: CRS.to_string(),
        source_axis_domain: GeoSourceAxisDomain::Planar,
        source_decimal_places,
        source_origin: GeoSourcePointFixed { x: 0, y: 0 },
        affine: GeoAffineProjectionMm {
            x_from_source_x_numerator: 1,
            x_from_source_y_numerator: 0,
            y_from_source_x_numerator: 0,
            y_from_source_y_numerator: 1,
            denominator: affine_denominator,
        },
        projection: GeoProjectionProvenance {
            method_id: "test-fixed-affine".to_string(),
            method_version: "1.0.0".to_string(),
            parameters_blake3: blake3::hash(b"test-fixed-affine-v1").to_hex().to_string(),
            max_projection_error_micrometres: projection_error_micrometres,
        },
        max_abs_coordinate_mm: 2_000_000,
    }
}

fn polygon(exterior: Vec<GeoSourcePointDecimal>) -> GeoSourceGeometry {
    GeoSourceGeometry::Polygon {
        exterior,
        holes: vec![],
    }
}

fn source_polygon(
    exterior: Vec<GeoSourcePointDecimal>,
    holes: Vec<Vec<GeoSourcePointDecimal>>,
) -> canon::geo::GeoSourcePolygon {
    canon::geo::GeoSourcePolygon { exterior, holes }
}

fn rectangle(
    min_x: &str,
    min_y: &str,
    max_x: &str,
    max_y: &str,
    reverse: bool,
) -> Vec<GeoSourcePointDecimal> {
    let mut vertices = vec![
        point(min_x, min_y),
        point(max_x, min_y),
        point(max_x, max_y),
        point(min_x, max_y),
    ];
    if reverse {
        vertices.reverse();
    }
    vertices.push(vertices[0].clone());
    vertices
}

fn rotate_closed(
    mut ring: Vec<GeoSourcePointDecimal>,
    amount: usize,
) -> Vec<GeoSourcePointDecimal> {
    ring.pop();
    ring.rotate_left(amount);
    ring.push(ring[0].clone());
    ring
}

fn point(x: &str, y: &str) -> GeoSourcePointDecimal {
    GeoSourcePointDecimal {
        x: x.to_string(),
        y: y.to_string(),
    }
}

fn warehouse_request(rows: Vec<GeoWarehouseGeometryRow>) -> GeoWarehouseGeometryRowsRequest {
    GeoWarehouseGeometryRowsRequest {
        version: CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION.to_string(),
        tile_id: "892a100d26bffff".to_string(),
        frame_id: "tile:892a100d26bffff:epsg2263-mm:v0".to_string(),
        source_crs: "EPSG:2263".to_string(),
        source_srid: 2263,
        source_decimal_places: 9,
        source_origin: point("980000", "191000"),
        source_unit_to_millimetres: GeoExactSourceUnitMm {
            unit_id: "us-survey-foot".to_string(),
            numerator: 1_200_000,
            denominator: 3_937,
        },
        rows,
        max_abs_coordinate_mm: 1_000_000,
        max_vertices_per_geometry: 10_000,
        max_geometry_bytes_per_tile: 1_000_000,
    }
}

fn warehouse_row(
    feature_id: &str,
    source_record_id: &str,
    bytes: &[u8],
) -> GeoWarehouseGeometryRow {
    let digest = Sha256::digest(bytes);
    GeoWarehouseGeometryRow {
        feature_id: feature_id.to_string(),
        source_record_id: source_record_id.to_string(),
        source_dataset: "nyc_dcp_mappluto".to_string(),
        source_release: "26v2".to_string(),
        source_release_date: "2026-08-01".to_string(),
        source_geometry_contract_version: "nyc_dcp_mappluto_geometry_evidence.v3".to_string(),
        source_archive_sha256: "e06eca9034731bc23f058bf532090e3c1ea6aed44a8128c6928f33872da34ab5"
            .to_string(),
        source_crs: "EPSG:2263".to_string(),
        source_srid: 2263,
        source_geom_wkb_base64: BASE64_STANDARD.encode(bytes),
        source_geom_wkb_sha256: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
        transform_execution_id: "sha256-execution-26v2".to_string(),
        transform_definition_id: "sha256-definition-hpgn".to_string(),
    }
}

fn polygon_wkb(points: &[(f64, f64)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.push(1);
    bytes.extend_from_slice(&3_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&(points.len() as u32).to_le_bytes());
    for (x, y) in points {
        bytes.extend_from_slice(&x.to_le_bytes());
        bytes.extend_from_slice(&y.to_le_bytes());
    }
    bytes
}
