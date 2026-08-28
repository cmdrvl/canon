#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_GEOMETRY_REQUEST_VERSION, CANON_GEO_LOCAL_FRAME_VERSION, GeoAffineProjectionMm,
    GeoCanonicalGeometryMm, GeoFeatureValue, GeoGeometryBudgetEnforcement, GeoGeometryErrorCode,
    GeoGeometryFeatureInput, GeoGeometryTileRequest, GeoLocalFrameContract,
    GeoProjectionProvenance, GeoSourceAxisDomain, GeoSourceGeometry, GeoSourcePointDecimal,
    GeoSourcePointFixed, canonical_geometry_tile_bytes, materialize_geometry_tile,
};

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
