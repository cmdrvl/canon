use canon::geo::{
    GeoAreaMajorityErrorCode, GeoLinearRingMm, GeoOrientation, GeoPointLocation, GeoPointMm,
    GeoPredicateErrorCode, GeoSegmentIntersection, exact_orientation, exact_segment_intersection,
    footprint_majority_area_inside_parcel,
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
fn footprint_majority_area_inside_parcel_handles_basic_area_cases_exactly() {
    let footprint = ring(&[(0, 0), (100, 0), (100, 10), (0, 10)]);

    let containing_parcel = ring(&[(-10, -10), (110, -10), (110, 20), (-10, 20)]);
    assert!(
        footprint_majority_area_inside_parcel(&footprint, &containing_parcel).unwrap(),
        "fully contained footprint must satisfy strict area majority"
    );

    let disjoint_parcel = ring(&[(200, 0), (300, 0), (300, 10), (200, 10)]);
    assert!(
        !footprint_majority_area_inside_parcel(&footprint, &disjoint_parcel).unwrap(),
        "disjoint parcel has zero footprint area inside"
    );

    let half_parcel = ring(&[(-10, -10), (50, -10), (50, 20), (-10, 20)]);
    assert!(
        !footprint_majority_area_inside_parcel(&footprint, &half_parcel).unwrap(),
        "exactly half of the computed footprint area is false"
    );

    let one_millimetre_over_half_parcel = ring(&[(-10, -10), (51, -10), (51, 20), (-10, 20)]);
    assert!(
        footprint_majority_area_inside_parcel(&footprint, &one_millimetre_over_half_parcel)
            .unwrap(),
        "one millimetre more than half the computed area is true"
    );
}

#[test]
fn footprint_majority_area_inside_parcel_handles_crossing_and_concave_rings() {
    let footprint = ring(&[(0, 0), (100, 0), (100, 100), (0, 100)]);
    let crossing_parcel = ring(&[(-20, 25), (75, 25), (75, 75), (-20, 75)]);
    assert!(
        !footprint_majority_area_inside_parcel(&footprint, &crossing_parcel).unwrap(),
        "crossing alone is not enough when only 37.5% of footprint area is inside"
    );

    let concave_parcel = ring(&[(0, 0), (60, 0), (60, 20), (20, 20), (20, 60), (0, 60)]);
    let concave_footprint = ring(&[(5, 5), (30, 5), (30, 30), (5, 30)]);
    assert!(
        footprint_majority_area_inside_parcel(&concave_footprint, &concave_parcel).unwrap(),
        "concave parcel intersection must be evaluated by area, not vertex count"
    );
}

#[test]
fn footprint_majority_area_inside_parcel_is_reversal_and_translation_invariant() {
    let footprint_coords = [(0, 0), (100, 0), (100, 10), (0, 10)];
    let parcel_coords = [(-10, -10), (51, -10), (51, 20), (-10, 20)];

    for (translated_x, translated_y, reverse_footprint, reverse_parcel) in [
        (0, 0, false, false),
        (10_000, -7_000, false, true),
        (-44_000, 91_000, true, false),
        (2_000_000, 3_000_000, true, true),
    ] {
        let footprint = translated_ring(
            &footprint_coords,
            translated_x,
            translated_y,
            reverse_footprint,
        );
        let parcel = translated_ring(&parcel_coords, translated_x, translated_y, reverse_parcel);
        assert!(
            footprint_majority_area_inside_parcel(&footprint, &parcel).unwrap(),
            "translation=({translated_x},{translated_y}) reverse_footprint={reverse_footprint} reverse_parcel={reverse_parcel}"
        );
    }
}

#[test]
fn footprint_majority_area_inside_parcel_rejects_shortcut_predicates() {
    let footprint = ring(&[(0, 0), (100, 0), (100, 100), (0, 100)]);
    let centroid_containing_parcel = ring(&[(45, 45), (55, 45), (55, 55), (45, 55)]);

    assert!(
        !footprint_majority_area_inside_parcel(&footprint, &centroid_containing_parcel).unwrap(),
        "bbox overlap, footprint centroid containment, and parcel vertices inside the footprint do not imply area majority"
    );
}

#[test]
fn footprint_majority_area_inside_parcel_uses_rational_threshold_for_odd_double_area() {
    let footprint = ring(&[(0, 0), (7, 0), (0, 1)]);
    let parcel = ring(&[(-1, -1), (2, -1), (2, 2), (-1, 2)]);

    assert!(
        !footprint_majority_area_inside_parcel(&footprint, &parcel).unwrap(),
        "left slice has double-area 24/7, which is below the strict half threshold 7/2"
    );
}

#[test]
fn footprint_majority_area_inside_parcel_handles_near_half_diagonal_rational_cuts() {
    let footprint = ring(&[(0, 0), (101, 0), (0, 1)]);
    let below_half = ring(&[(-1, -1), (29, -1), (29, 2), (-1, 2)]);
    let above_half = ring(&[(-1, -1), (30, -1), (30, 2), (-1, 2)]);

    assert!(
        !footprint_majority_area_inside_parcel(&footprint, &below_half).unwrap(),
        "x<=29 slice has double-area 5017/101, below strict half threshold 101/2"
    );
    assert!(
        footprint_majority_area_inside_parcel(&footprint, &above_half).unwrap(),
        "x<=30 slice has double-area 5160/101, above strict half threshold 101/2"
    );
}

#[test]
fn footprint_majority_area_inside_parcel_handles_partial_collinear_overlap_with_area() {
    let footprint = ring(&[(0, 0), (10, 0), (10, 10), (0, 10)]);
    let parcel = ring(&[(4, 0), (14, 0), (14, 10), (4, 10)]);

    assert!(
        footprint_majority_area_inside_parcel(&footprint, &parcel).unwrap(),
        "partial collinear edge overlap still encloses 60% of the footprint area"
    );
}

#[test]
fn footprint_majority_area_inside_parcel_handles_concave_footprint_across_boundary() {
    let footprint = ring(&[(0, 0), (6, 0), (6, 2), (2, 2), (2, 6), (0, 6)]);
    let parcel = ring(&[(-1, -1), (3, -1), (3, 7), (-1, 7)]);

    assert!(
        footprint_majority_area_inside_parcel(&footprint, &parcel).unwrap(),
        "parcel cuts a concave footprint but contains 14/20 square units"
    );
}

#[test]
fn footprint_majority_area_inside_parcel_collinear_reflex_subdivision_is_not_false() {
    let footprint = ring(&[(0, 0), (3, 0), (6, 0), (6, 2), (2, 2), (2, 6), (0, 6)]);
    let parcel = ring(&[(-1, -1), (7, -1), (7, 7), (-1, 7)]);

    match footprint_majority_area_inside_parcel(&footprint, &parcel) {
        Ok(true) => {}
        Ok(false) => panic!("containing parcel cannot be false for a valid subdivided footprint"),
        Err(error) => assert_eq!(error.code, GeoAreaMajorityErrorCode::UnsupportedTopology),
    }
}

#[test]
fn footprint_majority_area_inside_parcel_refuses_mixed_frames() {
    let footprint = ring(&[(0, 0), (10, 0), (10, 10), (0, 10)]);
    let parcel = ring_in_frame(
        "tile:892a100d26bffff:other-local-mm:v1",
        &[(-1, -1), (11, -1), (11, 11), (-1, 11)],
    );

    let error = footprint_majority_area_inside_parcel(&footprint, &parcel).unwrap_err();
    assert_eq!(error.code, GeoAreaMajorityErrorCode::MixedFrame);
}

#[test]
fn footprint_majority_area_inside_parcel_refuses_overflow_without_float_fallback() {
    let footprint = ring(&[(i64::MIN, 0), (i64::MAX, 0), (0, 1)]);
    let parcel = ring(&[(0, i64::MIN), (0, i64::MAX), (1, 0)]);

    let error = footprint_majority_area_inside_parcel(&footprint, &parcel).unwrap_err();
    assert_eq!(error.code, GeoAreaMajorityErrorCode::ArithmeticOverflow);
}

#[test]
fn footprint_majority_area_inside_parcel_large_identical_square_is_never_false() {
    let max = i64::MAX;
    let footprint = ring(&[(0, 0), (max, 0), (max, max), (0, max)]);
    let parcel = ring(&[(0, 0), (max, 0), (max, max), (0, max)]);

    match footprint_majority_area_inside_parcel(&footprint, &parcel) {
        Ok(true) => {}
        Ok(false) => panic!("identical constructor-valid rings cannot be classified as false"),
        Err(error) => assert_eq!(error.code, GeoAreaMajorityErrorCode::ArithmeticOverflow),
    }
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

fn ring(coords: &[(i64, i64)]) -> GeoLinearRingMm {
    translated_ring(coords, 0, 0, false)
}

fn ring_in_frame(frame: &str, coords: &[(i64, i64)]) -> GeoLinearRingMm {
    let mut vertices = coords
        .iter()
        .map(|(x, y)| GeoPointMm::new(*x, *y))
        .collect::<Vec<_>>();
    vertices.push(vertices[0]);
    GeoLinearRingMm::new(frame, vertices).unwrap()
}

fn translated_ring(
    coords: &[(i64, i64)],
    translated_x: i64,
    translated_y: i64,
    reverse: bool,
) -> GeoLinearRingMm {
    let mut vertices = coords
        .iter()
        .map(|(x, y)| GeoPointMm::new(x + translated_x, y + translated_y))
        .collect::<Vec<_>>();
    if reverse {
        vertices.reverse();
    }
    vertices.push(vertices[0]);
    GeoLinearRingMm::new(FRAME, vertices).unwrap()
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
