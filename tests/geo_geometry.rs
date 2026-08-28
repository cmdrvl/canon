use canon::geo::{
    GeoLinearRingMm, GeoOrientation, GeoPointLocation, GeoPointMm, GeoPredicateErrorCode,
    GeoSegmentIntersection, exact_orientation, exact_segment_intersection,
};

const FRAME: &str = "tile:892a100d26bffff:local-mm:v1";

#[test]
fn exact_orientation_and_segment_relations_cover_boundary_cases() {
    let origin = GeoPointMm::new(0, 0);
    let east = GeoPointMm::new(10, 0);
    assert_eq!(
        exact_orientation(origin, east, GeoPointMm::new(5, 1)).unwrap(),
        GeoOrientation::CounterClockwise
    );
    assert_eq!(
        exact_orientation(origin, east, GeoPointMm::new(5, -1)).unwrap(),
        GeoOrientation::Clockwise
    );
    assert_eq!(
        exact_orientation(origin, east, GeoPointMm::new(5, 0)).unwrap(),
        GeoOrientation::Collinear
    );

    assert_eq!(
        exact_segment_intersection(
            origin,
            GeoPointMm::new(10, 10),
            GeoPointMm::new(0, 10),
            GeoPointMm::new(10, 0),
        )
        .unwrap(),
        GeoSegmentIntersection::Crosses
    );
    assert_eq!(
        exact_segment_intersection(origin, east, east, GeoPointMm::new(20, 0),).unwrap(),
        GeoSegmentIntersection::Touches
    );
    assert_eq!(
        exact_segment_intersection(origin, east, GeoPointMm::new(2, 0), GeoPointMm::new(8, 0),)
            .unwrap(),
        GeoSegmentIntersection::Overlaps
    );
    assert_eq!(
        exact_segment_intersection(origin, east, GeoPointMm::new(11, 0), GeoPointMm::new(20, 0),)
            .unwrap(),
        GeoSegmentIntersection::Disjoint
    );
}

#[test]
fn boundary_adjacent_point_location_is_translation_and_orientation_invariant() {
    for translated_x in [-2_000_000_i64, -17, 0, 91, 2_000_000] {
        for translated_y in [-2_000_000_i64, -23, 0, 103, 2_000_000] {
            for reverse in [false, true] {
                let ring = rectangle(translated_x, translated_y, reverse);
                let cases = [
                    (GeoPointMm::new(5_000, -1), GeoPointLocation::Exterior),
                    (GeoPointMm::new(5_000, 0), GeoPointLocation::Boundary),
                    (GeoPointMm::new(5_000, 1), GeoPointLocation::Interior),
                    (GeoPointMm::new(-1, 5_000), GeoPointLocation::Exterior),
                    (GeoPointMm::new(0, 5_000), GeoPointLocation::Boundary),
                    (GeoPointMm::new(1, 5_000), GeoPointLocation::Interior),
                    (GeoPointMm::new(10_000, 10_000), GeoPointLocation::Boundary),
                    (GeoPointMm::new(10_001, 10_001), GeoPointLocation::Exterior),
                ];
                for (relative, expected) in cases {
                    let point =
                        GeoPointMm::new(relative.x + translated_x, relative.y + translated_y);
                    assert_eq!(
                        ring.locate_point(FRAME, point).unwrap(),
                        expected,
                        "translation=({translated_x},{translated_y}) reverse={reverse} point={relative:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn diagonal_edge_keeps_one_millimetre_separation_exact() {
    let ring = GeoLinearRingMm::new(
        FRAME,
        vec![
            GeoPointMm::new(0, 0),
            GeoPointMm::new(10_000, 0),
            GeoPointMm::new(0, 10_000),
            GeoPointMm::new(0, 0),
        ],
    )
    .unwrap();

    assert_eq!(
        ring.locate_point(FRAME, GeoPointMm::new(5_000, 4_999))
            .unwrap(),
        GeoPointLocation::Interior
    );
    assert_eq!(
        ring.locate_point(FRAME, GeoPointMm::new(5_000, 5_000))
            .unwrap(),
        GeoPointLocation::Boundary
    );
    assert_eq!(
        ring.locate_point(FRAME, GeoPointMm::new(5_000, 5_001))
            .unwrap(),
        GeoPointLocation::Exterior
    );
}

#[test]
fn invalid_geometry_and_mixed_frames_refuse_instead_of_repairing() {
    let unclosed = GeoLinearRingMm::new(
        FRAME,
        vec![
            GeoPointMm::new(0, 0),
            GeoPointMm::new(10, 0),
            GeoPointMm::new(0, 10),
        ],
    )
    .unwrap_err();
    assert_eq!(unclosed.code, GeoPredicateErrorCode::UnclosedRing);

    let duplicate = GeoLinearRingMm::new(
        FRAME,
        vec![
            GeoPointMm::new(0, 0),
            GeoPointMm::new(10, 0),
            GeoPointMm::new(10, 0),
            GeoPointMm::new(0, 10),
            GeoPointMm::new(0, 0),
        ],
    )
    .unwrap_err();
    assert_eq!(duplicate.code, GeoPredicateErrorCode::DuplicateVertex);

    let self_intersection = GeoLinearRingMm::new(
        FRAME,
        vec![
            GeoPointMm::new(0, 0),
            GeoPointMm::new(4, 4),
            GeoPointMm::new(0, 4),
            GeoPointMm::new(4, 0),
            GeoPointMm::new(4, -1),
            GeoPointMm::new(0, 0),
        ],
    )
    .unwrap_err();
    assert_eq!(
        self_intersection.code,
        GeoPredicateErrorCode::SelfIntersection
    );

    let degenerate = GeoLinearRingMm::new(
        FRAME,
        vec![
            GeoPointMm::new(0, 0),
            GeoPointMm::new(5, 0),
            GeoPointMm::new(10, 0),
            GeoPointMm::new(0, 0),
        ],
    )
    .unwrap_err();
    assert_eq!(degenerate.code, GeoPredicateErrorCode::DegenerateRing);

    let ring = rectangle(0, 0, false);
    let mixed = ring
        .locate_point("tile:other:local-mm:v1", GeoPointMm::new(5, 5))
        .unwrap_err();
    assert_eq!(mixed.code, GeoPredicateErrorCode::MixedFrame);
}

#[test]
fn ring_area_is_exact_but_only_relative_to_quantized_coordinates() {
    let ring = rectangle(0, 0, false);
    assert_eq!(ring.absolute_double_area_mm2(), 200_000_000);
    assert_eq!(ring.vertices().len(), 4);
    assert_eq!(ring.frame_id(), FRAME);
}

#[test]
fn extreme_integer_input_refuses_on_overflow_without_float_fallback() {
    let error = exact_orientation(
        GeoPointMm::new(i64::MIN, i64::MIN),
        GeoPointMm::new(i64::MAX, i64::MIN),
        GeoPointMm::new(i64::MIN, i64::MAX),
    )
    .unwrap_err();
    assert_eq!(error.code, GeoPredicateErrorCode::ArithmeticOverflow);
}

fn rectangle(translated_x: i64, translated_y: i64, reverse: bool) -> GeoLinearRingMm {
    let mut vertices = vec![
        GeoPointMm::new(translated_x, translated_y),
        GeoPointMm::new(translated_x + 10_000, translated_y),
        GeoPointMm::new(translated_x + 10_000, translated_y + 10_000),
        GeoPointMm::new(translated_x, translated_y + 10_000),
    ];
    if reverse {
        vertices.reverse();
    }
    vertices.push(vertices[0]);
    GeoLinearRingMm::new(FRAME, vertices).unwrap()
}
