#![forbid(unsafe_code)]

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use canon::geo::{
    CANON_GEO_GEOMETRY_REQUEST_VERSION, CANON_GEO_LOCAL_FRAME_VERSION,
    CANON_GEO_PROVIDER_TILE_BUILD_VERSION, CANON_GEO_PROVIDER_TILE_CONTRACT_VERSION,
    CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION, GeoAffineProjectionMm, GeoCanonicalGeometryMm,
    GeoExactSourceUnitMm, GeoFeatureValue, GeoGeometryBudgetEnforcement, GeoGeometryErrorCode,
    GeoGeometryFeatureInput, GeoGeometryTileRequest, GeoLicenseClass, GeoLocalFrameContract,
    GeoProjectionProvenance, GeoProviderGeometryFidelity, GeoProviderGeometryTileBuildRequest,
    GeoProviderTileCoverageState, GeoProviderTileDataBookDecision, GeoProviderTileFeatureContract,
    GeoProviderTileFeatureProvenance, GeoProviderTileFieldLocator, GeoProviderTileLicensePosture,
    GeoProviderTileRedactionClass, GeoProviderTileSource, GeoProviderTileSourceCoverage,
    GeoProviderTileSourceProvenance, GeoProviderTileSubsetKind, GeoProviderTileSubsetPredicate,
    GeoSourceAxisDomain, GeoSourceGeometry, GeoSourcePointDecimal, GeoSourcePointFixed,
    GeoWarehouseGeometryRow, GeoWarehouseGeometryRowsRequest, canonical_geometry_tile_bytes,
    canonical_warehouse_geometry_bytes, materialize_geometry_tile,
    materialize_provider_geometry_tile, materialize_warehouse_geometry,
};
use sha2::{Digest as _, Sha256};

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
