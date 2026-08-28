#![forbid(unsafe_code)]

//! Exact topological predicates over a tile-local integer frame.
//!
//! The predicates in this module are exact relative to the supplied, already
//! projected and quantized millimetre coordinates. They make no claim that a
//! source geometry, projection, or quantization represents world truth exactly.
//! Projection and canonical geometry encoding belong to the offline tile build;
//! floating-point coordinates cannot enter this decision path.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

/// A point in an already materialized tile-local millimetre frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoPointMm {
    pub x: i64,
    pub y: i64,
}

impl GeoPointMm {
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }
}

/// Exact sign of the oriented area of three integer points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoOrientation {
    Clockwise,
    Collinear,
    CounterClockwise,
}

/// Exact closed-segment relationship in the common integer frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoSegmentIntersection {
    Disjoint,
    Touches,
    Crosses,
    Overlaps,
}

/// Boundary semantics are explicit; boundary is never silently inside or out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPointLocation {
    Exterior,
    Boundary,
    Interior,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPredicateErrorCode {
    EmptyGeometry,
    InvalidFrame,
    UnclosedRing,
    TooFewVertices,
    DuplicateVertex,
    DegenerateRing,
    SelfIntersection,
    MixedFrame,
    ArithmeticOverflow,
}

/// Typed refusal from the integer predicate boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoPredicateError {
    pub code: GeoPredicateErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoPredicateError {
    fn new(
        code: GeoPredicateErrorCode,
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            detail: detail
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }

    fn overflow(context: &str) -> Self {
        Self::new(
            GeoPredicateErrorCode::ArithmeticOverflow,
            "Tile-local integer geometry exceeded the exact arithmetic domain",
            [("context", context)],
        )
    }
}

impl fmt::Display for GeoPredicateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoPredicateError {}

/// A validated simple ring. Input is explicitly closed; the repeated closing
/// point is omitted from the stored canonical predicate form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoLinearRingMm {
    frame_id: String,
    vertices: Vec<GeoPointMm>,
    absolute_double_area_mm2: u128,
}

impl GeoLinearRingMm {
    /// Validate an explicitly closed ring without repairing it.
    ///
    /// Ring direction and starting vertex do not affect predicates. Canonical
    /// rotation/orientation for artifact hashing is owned by the typed geometry
    /// materializer, not hidden inside this constructor.
    pub fn new(
        frame_id: impl Into<String>,
        explicitly_closed_vertices: Vec<GeoPointMm>,
    ) -> Result<Self, GeoPredicateError> {
        let frame_id = frame_id.into();
        validate_frame_id(&frame_id)?;
        if explicitly_closed_vertices.is_empty() {
            return Err(GeoPredicateError::new(
                GeoPredicateErrorCode::EmptyGeometry,
                "A linear ring cannot be empty",
                std::iter::empty::<(&str, &str)>(),
            ));
        }
        if explicitly_closed_vertices.first() != explicitly_closed_vertices.last() {
            return Err(GeoPredicateError::new(
                GeoPredicateErrorCode::UnclosedRing,
                "A linear ring must repeat its first point as its final input point",
                [("frame_id", frame_id.as_str())],
            ));
        }

        let vertices = explicitly_closed_vertices[..explicitly_closed_vertices.len() - 1].to_vec();
        if vertices.len() < 3 {
            return Err(GeoPredicateError::new(
                GeoPredicateErrorCode::TooFewVertices,
                "A linear ring needs at least three distinct vertices",
                [("vertex_count", vertices.len().to_string())],
            ));
        }

        let mut seen = BTreeSet::new();
        for (index, point) in vertices.iter().enumerate() {
            if !seen.insert(*point) {
                return Err(GeoPredicateError::new(
                    GeoPredicateErrorCode::DuplicateVertex,
                    "A linear ring cannot repeat a non-closing vertex",
                    [
                        ("vertex_index", index.to_string()),
                        ("x_mm", point.x.to_string()),
                        ("y_mm", point.y.to_string()),
                    ],
                ));
            }
        }

        let signed_double_area = exact_signed_double_area(&vertices)?;
        if signed_double_area == 0 {
            return Err(GeoPredicateError::new(
                GeoPredicateErrorCode::DegenerateRing,
                "A linear ring must enclose nonzero area",
                [("frame_id", frame_id.as_str())],
            ));
        }
        validate_simple_ring(&vertices)?;
        let absolute_double_area_mm2 = signed_double_area
            .checked_abs()
            .ok_or_else(|| GeoPredicateError::overflow("absolute ring double-area"))?
            as u128;

        Ok(Self {
            frame_id,
            vertices,
            absolute_double_area_mm2,
        })
    }

    pub fn frame_id(&self) -> &str {
        &self.frame_id
    }

    pub fn vertices(&self) -> &[GeoPointMm] {
        &self.vertices
    }

    /// Exact twice-area of this ring in square millimetres.
    pub const fn absolute_double_area_mm2(&self) -> u128 {
        self.absolute_double_area_mm2
    }

    /// Locate a point with explicit boundary semantics.
    pub fn locate_point(
        &self,
        point_frame_id: &str,
        point: GeoPointMm,
    ) -> Result<GeoPointLocation, GeoPredicateError> {
        validate_frame_id(point_frame_id)?;
        if point_frame_id != self.frame_id {
            return Err(GeoPredicateError::new(
                GeoPredicateErrorCode::MixedFrame,
                "Point and ring must use the same tile-local frame",
                [
                    ("point_frame_id", point_frame_id),
                    ("ring_frame_id", self.frame_id.as_str()),
                ],
            ));
        }

        let mut inside = false;
        for index in 0..self.vertices.len() {
            let start = self.vertices[index];
            let end = self.vertices[(index + 1) % self.vertices.len()];
            let orientation = exact_orientation(start, end, point)?;
            if orientation == GeoOrientation::Collinear && point_in_bbox(point, start, end) {
                return Ok(GeoPointLocation::Boundary);
            }

            let crosses_scanline = (start.y > point.y) != (end.y > point.y);
            if crosses_scanline {
                let crosses_right = match orientation {
                    GeoOrientation::CounterClockwise => end.y > start.y,
                    GeoOrientation::Clockwise => end.y < start.y,
                    GeoOrientation::Collinear => false,
                };
                if crosses_right {
                    inside = !inside;
                }
            }
        }
        Ok(if inside {
            GeoPointLocation::Interior
        } else {
            GeoPointLocation::Exterior
        })
    }
}

/// Exact orientation using checked `i128` arithmetic.
///
/// Ordinary tile-local millimetre coordinates are far inside this domain. A
/// deliberately extreme integer input refuses instead of overflowing or
/// falling back to floating point.
pub fn exact_orientation(
    start: GeoPointMm,
    end: GeoPointMm,
    point: GeoPointMm,
) -> Result<GeoOrientation, GeoPredicateError> {
    let ab_x = i128::from(end.x) - i128::from(start.x);
    let ab_y = i128::from(end.y) - i128::from(start.y);
    let ap_x = i128::from(point.x) - i128::from(start.x);
    let ap_y = i128::from(point.y) - i128::from(start.y);
    let left = ab_x
        .checked_mul(ap_y)
        .ok_or_else(|| GeoPredicateError::overflow("orientation left product"))?;
    let right = ab_y
        .checked_mul(ap_x)
        .ok_or_else(|| GeoPredicateError::overflow("orientation right product"))?;
    let determinant = left
        .checked_sub(right)
        .ok_or_else(|| GeoPredicateError::overflow("orientation determinant"))?;
    Ok(match determinant.cmp(&0) {
        std::cmp::Ordering::Less => GeoOrientation::Clockwise,
        std::cmp::Ordering::Equal => GeoOrientation::Collinear,
        std::cmp::Ordering::Greater => GeoOrientation::CounterClockwise,
    })
}

/// Classify two closed segments exactly in their common integer frame.
pub fn exact_segment_intersection(
    left_start: GeoPointMm,
    left_end: GeoPointMm,
    right_start: GeoPointMm,
    right_end: GeoPointMm,
) -> Result<GeoSegmentIntersection, GeoPredicateError> {
    let left_start_side = exact_orientation(left_start, left_end, right_start)?;
    let left_end_side = exact_orientation(left_start, left_end, right_end)?;
    let right_start_side = exact_orientation(right_start, right_end, left_start)?;
    let right_end_side = exact_orientation(right_start, right_end, left_end)?;

    if left_start_side == GeoOrientation::Collinear
        && left_end_side == GeoOrientation::Collinear
        && right_start_side == GeoOrientation::Collinear
        && right_end_side == GeoOrientation::Collinear
    {
        return Ok(collinear_intersection(
            left_start,
            left_end,
            right_start,
            right_end,
        ));
    }

    if opposite(left_start_side, left_end_side) && opposite(right_start_side, right_end_side) {
        return Ok(GeoSegmentIntersection::Crosses);
    }

    let touches = (left_start_side == GeoOrientation::Collinear
        && point_in_bbox(right_start, left_start, left_end))
        || (left_end_side == GeoOrientation::Collinear
            && point_in_bbox(right_end, left_start, left_end))
        || (right_start_side == GeoOrientation::Collinear
            && point_in_bbox(left_start, right_start, right_end))
        || (right_end_side == GeoOrientation::Collinear
            && point_in_bbox(left_end, right_start, right_end));
    Ok(if touches {
        GeoSegmentIntersection::Touches
    } else {
        GeoSegmentIntersection::Disjoint
    })
}

fn validate_frame_id(frame_id: &str) -> Result<(), GeoPredicateError> {
    if frame_id.is_empty() || frame_id.trim() != frame_id || frame_id.len() > 256 {
        return Err(GeoPredicateError::new(
            GeoPredicateErrorCode::InvalidFrame,
            "Tile-local frame id must be nonempty, unpadded UTF-8 of at most 256 bytes",
            [("frame_id_length", frame_id.len().to_string())],
        ));
    }
    Ok(())
}

fn exact_signed_double_area(vertices: &[GeoPointMm]) -> Result<i128, GeoPredicateError> {
    let mut sum = 0_i128;
    for index in 0..vertices.len() {
        let start = vertices[index];
        let end = vertices[(index + 1) % vertices.len()];
        let forward = i128::from(start.x)
            .checked_mul(i128::from(end.y))
            .ok_or_else(|| GeoPredicateError::overflow("shoelace forward product"))?;
        let reverse = i128::from(start.y)
            .checked_mul(i128::from(end.x))
            .ok_or_else(|| GeoPredicateError::overflow("shoelace reverse product"))?;
        let term = forward
            .checked_sub(reverse)
            .ok_or_else(|| GeoPredicateError::overflow("shoelace term"))?;
        sum = sum
            .checked_add(term)
            .ok_or_else(|| GeoPredicateError::overflow("shoelace sum"))?;
    }
    Ok(sum)
}

fn validate_simple_ring(vertices: &[GeoPointMm]) -> Result<(), GeoPredicateError> {
    let edge_count = vertices.len();
    for left_index in 0..edge_count {
        let left_start = vertices[left_index];
        let left_end = vertices[(left_index + 1) % edge_count];
        for right_index in left_index + 1..edge_count {
            let adjacent =
                right_index == left_index + 1 || (left_index == 0 && right_index == edge_count - 1);
            if adjacent {
                continue;
            }
            let right_start = vertices[right_index];
            let right_end = vertices[(right_index + 1) % edge_count];
            let relation =
                exact_segment_intersection(left_start, left_end, right_start, right_end)?;
            if relation != GeoSegmentIntersection::Disjoint {
                return Err(GeoPredicateError::new(
                    GeoPredicateErrorCode::SelfIntersection,
                    "A linear ring cannot intersect or touch itself",
                    [
                        ("left_edge", left_index.to_string()),
                        ("right_edge", right_index.to_string()),
                        ("relation", format!("{relation:?}")),
                    ],
                ));
            }
        }
    }
    Ok(())
}

fn opposite(left: GeoOrientation, right: GeoOrientation) -> bool {
    matches!(
        (left, right),
        (GeoOrientation::Clockwise, GeoOrientation::CounterClockwise)
            | (GeoOrientation::CounterClockwise, GeoOrientation::Clockwise)
    )
}

fn point_in_bbox(point: GeoPointMm, start: GeoPointMm, end: GeoPointMm) -> bool {
    point.x >= start.x.min(end.x)
        && point.x <= start.x.max(end.x)
        && point.y >= start.y.min(end.y)
        && point.y <= start.y.max(end.y)
}

fn collinear_intersection(
    left_start: GeoPointMm,
    left_end: GeoPointMm,
    right_start: GeoPointMm,
    right_end: GeoPointMm,
) -> GeoSegmentIntersection {
    let x_low = left_start
        .x
        .min(left_end.x)
        .max(right_start.x.min(right_end.x));
    let x_high = left_start
        .x
        .max(left_end.x)
        .min(right_start.x.max(right_end.x));
    let y_low = left_start
        .y
        .min(left_end.y)
        .max(right_start.y.min(right_end.y));
    let y_high = left_start
        .y
        .max(left_end.y)
        .min(right_start.y.max(right_end.y));
    if x_low > x_high || y_low > y_high {
        GeoSegmentIntersection::Disjoint
    } else if x_low < x_high || y_low < y_high {
        GeoSegmentIntersection::Overlaps
    } else {
        GeoSegmentIntersection::Touches
    }
}
