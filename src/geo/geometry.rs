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
    cmp::Ordering,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAreaMajorityErrorCode {
    MixedFrame,
    ArithmeticOverflow,
    UnsupportedTopology,
}

/// Typed refusal from the footprint-vs-parcel area-majority predicate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoAreaMajorityError {
    pub code: GeoAreaMajorityErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoAreaMajorityError {
    fn new(
        code: GeoAreaMajorityErrorCode,
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

    fn overflow(context: &'static str) -> Self {
        Self::new(
            GeoAreaMajorityErrorCode::ArithmeticOverflow,
            "Tile-local integer geometry exceeded the exact area-majority arithmetic domain",
            [("context", context)],
        )
    }

    fn unsupported_topology(context: &'static str) -> Self {
        Self::new(
            GeoAreaMajorityErrorCode::UnsupportedTopology,
            "The integer area-majority predicate does not support this simple-ring topology",
            [("context", context)],
        )
    }
}

impl From<GeoPredicateError> for GeoAreaMajorityError {
    fn from(error: GeoPredicateError) -> Self {
        let code = match error.code {
            GeoPredicateErrorCode::MixedFrame => GeoAreaMajorityErrorCode::MixedFrame,
            GeoPredicateErrorCode::ArithmeticOverflow => {
                GeoAreaMajorityErrorCode::ArithmeticOverflow
            }
            GeoPredicateErrorCode::EmptyGeometry
            | GeoPredicateErrorCode::InvalidFrame
            | GeoPredicateErrorCode::UnclosedRing
            | GeoPredicateErrorCode::TooFewVertices
            | GeoPredicateErrorCode::DuplicateVertex
            | GeoPredicateErrorCode::DegenerateRing
            | GeoPredicateErrorCode::SelfIntersection => {
                GeoAreaMajorityErrorCode::UnsupportedTopology
            }
        };
        Self {
            code,
            message: error.message,
            detail: error.detail,
        }
    }
}

impl fmt::Display for GeoAreaMajorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoAreaMajorityError {}

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

/// Decide whether strictly more than half of a footprint ring's computed
/// geometric area lies inside a parcel ring.
///
/// The predicate is exact relative to the already-quantized local integer frame.
/// It uses only computed ring geometry: asserted source area fields, warehouse
/// predicates, floating point, and epsilons are deliberately outside this API.
pub fn footprint_majority_area_inside_parcel(
    footprint: &GeoLinearRingMm,
    parcel: &GeoLinearRingMm,
) -> Result<bool, GeoAreaMajorityError> {
    if footprint.frame_id != parcel.frame_id {
        return Err(GeoAreaMajorityError::new(
            GeoAreaMajorityErrorCode::MixedFrame,
            "Footprint and parcel rings must use the same tile-local frame",
            [
                ("footprint_frame_id", footprint.frame_id.as_str()),
                ("parcel_frame_id", parcel.frame_id.as_str()),
            ],
        ));
    }

    let footprint_triangles = triangulate_ring(footprint)?;
    let parcel_triangles = triangulate_ring(parcel)?;
    let mut intersection_double_area = ExactRational::zero();
    for footprint_triangle in &footprint_triangles {
        for parcel_triangle in &parcel_triangles {
            let triangle_overlap =
                triangle_intersection_double_area(*footprint_triangle, *parcel_triangle)?;
            intersection_double_area = intersection_double_area
                .checked_add(triangle_overlap, "triangle intersection area sum")?;
        }
    }

    let half_footprint_double_area = ExactRational::new(
        i128::try_from(footprint.absolute_double_area_mm2)
            .map_err(|_| GeoAreaMajorityError::overflow("footprint double-area threshold"))?,
        2,
        "footprint half-area threshold",
    )?;
    Ok(intersection_double_area.checked_cmp(
        &half_footprint_double_area,
        "footprint majority area comparison",
    )? == Ordering::Greater)
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

type Triangle = [GeoPointMm; 3];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExactRational {
    numerator: i128,
    denominator: i128,
}

impl ExactRational {
    const fn zero() -> Self {
        Self {
            numerator: 0,
            denominator: 1,
        }
    }

    const fn integer(value: i128) -> Self {
        Self {
            numerator: value,
            denominator: 1,
        }
    }

    fn new(
        mut numerator: i128,
        mut denominator: i128,
        context: &'static str,
    ) -> Result<Self, GeoAreaMajorityError> {
        if denominator == 0 {
            return Err(GeoAreaMajorityError::unsupported_topology(
                "rational zero denominator",
            ));
        }
        if denominator < 0 {
            numerator = numerator
                .checked_neg()
                .ok_or_else(|| GeoAreaMajorityError::overflow(context))?;
            denominator = denominator
                .checked_neg()
                .ok_or_else(|| GeoAreaMajorityError::overflow(context))?;
        }
        if numerator == 0 {
            return Ok(Self::zero());
        }

        let divisor = gcd_u128(numerator.unsigned_abs(), denominator as u128);
        if divisor > 1 {
            let divisor =
                i128::try_from(divisor).map_err(|_| GeoAreaMajorityError::overflow(context))?;
            numerator /= divisor;
            denominator /= divisor;
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    fn checked_add(self, other: Self, context: &'static str) -> Result<Self, GeoAreaMajorityError> {
        let denominator_gcd = gcd_u128(self.denominator as u128, other.denominator as u128);
        let left_multiplier = i128::try_from((other.denominator as u128) / denominator_gcd)
            .map_err(|_| GeoAreaMajorityError::overflow(context))?;
        let right_multiplier = i128::try_from((self.denominator as u128) / denominator_gcd)
            .map_err(|_| GeoAreaMajorityError::overflow(context))?;
        let left = self
            .numerator
            .checked_mul(left_multiplier)
            .ok_or_else(|| GeoAreaMajorityError::overflow(context))?;
        let right = other
            .numerator
            .checked_mul(right_multiplier)
            .ok_or_else(|| GeoAreaMajorityError::overflow(context))?;
        let numerator = left
            .checked_add(right)
            .ok_or_else(|| GeoAreaMajorityError::overflow(context))?;
        let denominator_reduced = self
            .denominator
            .checked_div(
                i128::try_from(denominator_gcd)
                    .map_err(|_| GeoAreaMajorityError::overflow(context))?,
            )
            .ok_or_else(|| GeoAreaMajorityError::overflow(context))?;
        let denominator = denominator_reduced
            .checked_mul(other.denominator)
            .ok_or_else(|| GeoAreaMajorityError::overflow(context))?;
        Self::new(numerator, denominator, context)
    }

    fn checked_sub(self, other: Self, context: &'static str) -> Result<Self, GeoAreaMajorityError> {
        self.checked_add(
            Self {
                numerator: other
                    .numerator
                    .checked_neg()
                    .ok_or_else(|| GeoAreaMajorityError::overflow(context))?,
                denominator: other.denominator,
            },
            context,
        )
    }

    fn checked_mul(self, other: Self, context: &'static str) -> Result<Self, GeoAreaMajorityError> {
        let numerator = self
            .numerator
            .checked_mul(other.numerator)
            .ok_or_else(|| GeoAreaMajorityError::overflow(context))?;
        let denominator = self
            .denominator
            .checked_mul(other.denominator)
            .ok_or_else(|| GeoAreaMajorityError::overflow(context))?;
        Self::new(numerator, denominator, context)
    }

    fn checked_abs(self, context: &'static str) -> Result<Self, GeoAreaMajorityError> {
        Ok(Self {
            numerator: self
                .numerator
                .checked_abs()
                .ok_or_else(|| GeoAreaMajorityError::overflow(context))?,
            denominator: self.denominator,
        })
    }

    fn checked_cmp(
        &self,
        other: &Self,
        context: &'static str,
    ) -> Result<Ordering, GeoAreaMajorityError> {
        Ok(self.checked_sub(*other, context)?.numerator.cmp(&0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExactPoint {
    x: ExactRational,
    y: ExactRational,
}

impl ExactPoint {
    fn integer(point: GeoPointMm) -> Self {
        Self {
            x: ExactRational::integer(i128::from(point.x)),
            y: ExactRational::integer(i128::from(point.y)),
        }
    }
}

fn triangulate_ring(ring: &GeoLinearRingMm) -> Result<Vec<Triangle>, GeoAreaMajorityError> {
    let mut vertices = simplify_collinear_vertices(ring.vertices())?;
    let signed_double_area = exact_signed_double_area(&vertices)?;
    if signed_double_area < 0 {
        vertices.reverse();
    }

    let mut remaining = vertices;
    let mut triangles = Vec::new();
    while remaining.len() > 3 {
        let mut removed_ear = false;
        let vertex_count = remaining.len();
        for index in 0..vertex_count {
            let previous = remaining[(index + vertex_count - 1) % vertex_count];
            let current = remaining[index];
            let next = remaining[(index + 1) % vertex_count];
            if exact_orientation(previous, current, next)? != GeoOrientation::CounterClockwise {
                continue;
            }
            let triangle = [previous, current, next];
            let mut contains_other_vertex = false;
            for (candidate_index, candidate) in remaining.iter().copied().enumerate() {
                if candidate_index == index
                    || candidate_index == (index + vertex_count - 1) % vertex_count
                    || candidate_index == (index + 1) % vertex_count
                {
                    continue;
                }
                if integer_point_in_ccw_triangle(candidate, triangle)? {
                    contains_other_vertex = true;
                    break;
                }
            }
            if !contains_other_vertex {
                triangles.push(triangle);
                remaining.remove(index);
                removed_ear = true;
                break;
            }
        }

        if !removed_ear {
            return Err(unsupported_topology("simple-ring triangulation"));
        }
    }

    if exact_orientation(remaining[0], remaining[1], remaining[2])?
        != GeoOrientation::CounterClockwise
    {
        return Err(unsupported_topology("final simple-ring triangle"));
    }
    triangles.push([remaining[0], remaining[1], remaining[2]]);
    Ok(triangles)
}

fn simplify_collinear_vertices(
    vertices: &[GeoPointMm],
) -> Result<Vec<GeoPointMm>, GeoAreaMajorityError> {
    let mut simplified = vertices.to_vec();
    loop {
        if simplified.len() < 3 {
            return Err(unsupported_topology("collinear ring simplification"));
        }
        let mut removed = false;
        for index in 0..simplified.len() {
            let previous = simplified[(index + simplified.len() - 1) % simplified.len()];
            let current = simplified[index];
            let next = simplified[(index + 1) % simplified.len()];
            if exact_orientation(previous, current, next)? == GeoOrientation::Collinear {
                if !point_in_bbox(current, previous, next) {
                    return Err(unsupported_topology("collinear ring reversal"));
                }
                simplified.remove(index);
                removed = true;
                break;
            }
        }
        if !removed {
            return Ok(simplified);
        }
    }
}

fn triangle_intersection_double_area(
    left: Triangle,
    right: Triangle,
) -> Result<ExactRational, GeoAreaMajorityError> {
    let mut points = Vec::new();
    for point in left {
        if integer_point_in_ccw_triangle(point, right)? {
            points.push(ExactPoint::integer(point));
        }
    }
    for point in right {
        if integer_point_in_ccw_triangle(point, left)? {
            points.push(ExactPoint::integer(point));
        }
    }

    for left_edge in triangle_edges(left) {
        for right_edge in triangle_edges(right) {
            if let Some(point) =
                segment_intersection_point(left_edge.0, left_edge.1, right_edge.0, right_edge.1)?
            {
                points.push(point);
            }
        }
    }

    let hull = convex_hull(points)?;
    if hull.len() < 3 {
        return Ok(ExactRational::zero());
    }
    rational_polygon_double_area(&hull)
}

fn integer_point_in_ccw_triangle(
    point: GeoPointMm,
    triangle: Triangle,
) -> Result<bool, GeoAreaMajorityError> {
    Ok(
        exact_orientation(triangle[0], triangle[1], point)? != GeoOrientation::Clockwise
            && exact_orientation(triangle[1], triangle[2], point)? != GeoOrientation::Clockwise
            && exact_orientation(triangle[2], triangle[0], point)? != GeoOrientation::Clockwise,
    )
}

fn triangle_edges(triangle: Triangle) -> [(GeoPointMm, GeoPointMm); 3] {
    [
        (triangle[0], triangle[1]),
        (triangle[1], triangle[2]),
        (triangle[2], triangle[0]),
    ]
}

fn segment_intersection_point(
    left_start: GeoPointMm,
    left_end: GeoPointMm,
    right_start: GeoPointMm,
    right_end: GeoPointMm,
) -> Result<Option<ExactPoint>, GeoAreaMajorityError> {
    let relation = exact_segment_intersection(left_start, left_end, right_start, right_end)?;
    if relation == GeoSegmentIntersection::Disjoint || relation == GeoSegmentIntersection::Overlaps
    {
        return Ok(None);
    }

    let left_x = checked_delta(left_end.x, left_start.x, "segment left dx")?;
    let left_y = checked_delta(left_end.y, left_start.y, "segment left dy")?;
    let right_x = checked_delta(right_end.x, right_start.x, "segment right dx")?;
    let right_y = checked_delta(right_end.y, right_start.y, "segment right dy")?;
    let denominator = checked_cross(left_x, left_y, right_x, right_y, "segment intersection")?;
    if denominator == 0 {
        return Ok(None);
    }

    let start_delta_x = checked_delta(right_start.x, left_start.x, "segment start dx")?;
    let start_delta_y = checked_delta(right_start.y, left_start.y, "segment start dy")?;
    let numerator = checked_cross(
        start_delta_x,
        start_delta_y,
        right_x,
        right_y,
        "segment intersection parameter",
    )?;
    let parameter = ExactRational::new(numerator, denominator, "segment intersection parameter")?;
    let x = ExactRational::integer(i128::from(left_start.x)).checked_add(
        ExactRational::integer(left_x).checked_mul(parameter, "segment intersection x scale")?,
        "segment intersection x",
    )?;
    let y = ExactRational::integer(i128::from(left_start.y)).checked_add(
        ExactRational::integer(left_y).checked_mul(parameter, "segment intersection y scale")?,
        "segment intersection y",
    )?;
    Ok(Some(ExactPoint { x, y }))
}

fn convex_hull(mut points: Vec<ExactPoint>) -> Result<Vec<ExactPoint>, GeoAreaMajorityError> {
    sort_points(&mut points)?;
    points.dedup();
    if points.len() <= 2 {
        return Ok(points);
    }

    let mut lower = Vec::new();
    for point in points.iter().copied() {
        while lower.len() >= 2
            && rational_orientation(lower[lower.len() - 2], lower[lower.len() - 1], point)?
                != Ordering::Greater
        {
            lower.pop();
        }
        lower.push(point);
    }

    let mut upper = Vec::new();
    for point in points.iter().rev().copied() {
        while upper.len() >= 2
            && rational_orientation(upper[upper.len() - 2], upper[upper.len() - 1], point)?
                != Ordering::Greater
        {
            upper.pop();
        }
        upper.push(point);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);
    Ok(lower)
}

fn sort_points(points: &mut [ExactPoint]) -> Result<(), GeoAreaMajorityError> {
    for index in 1..points.len() {
        let mut current = index;
        while current > 0 && compare_points(points[current], points[current - 1])? == Ordering::Less
        {
            points.swap(current, current - 1);
            current -= 1;
        }
    }
    Ok(())
}

fn compare_points(left: ExactPoint, right: ExactPoint) -> Result<Ordering, GeoAreaMajorityError> {
    match left.x.checked_cmp(&right.x, "rational point x ordering")? {
        Ordering::Equal => left.y.checked_cmp(&right.y, "rational point y ordering"),
        ordering => Ok(ordering),
    }
}

fn rational_orientation(
    start: ExactPoint,
    end: ExactPoint,
    point: ExactPoint,
) -> Result<Ordering, GeoAreaMajorityError> {
    let end_x = end.x.checked_sub(start.x, "rational orientation end dx")?;
    let end_y = end.y.checked_sub(start.y, "rational orientation end dy")?;
    let point_x = point
        .x
        .checked_sub(start.x, "rational orientation point dx")?;
    let point_y = point
        .y
        .checked_sub(start.y, "rational orientation point dy")?;
    let left = end_x.checked_mul(point_y, "rational orientation left product")?;
    let right = end_y.checked_mul(point_x, "rational orientation right product")?;
    Ok(left
        .checked_sub(right, "rational orientation determinant")?
        .numerator
        .cmp(&0))
}

fn rational_polygon_double_area(
    points: &[ExactPoint],
) -> Result<ExactRational, GeoAreaMajorityError> {
    let mut sum = ExactRational::zero();
    for index in 0..points.len() {
        let start = points[index];
        let end = points[(index + 1) % points.len()];
        let forward = start
            .x
            .checked_mul(end.y, "rational shoelace forward product")?;
        let reverse = start
            .y
            .checked_mul(end.x, "rational shoelace reverse product")?;
        let term = forward.checked_sub(reverse, "rational shoelace term")?;
        sum = sum.checked_add(term, "rational shoelace sum")?;
    }
    sum.checked_abs("rational shoelace absolute area")
}

fn checked_delta(
    end: i64,
    start: i64,
    context: &'static str,
) -> Result<i128, GeoAreaMajorityError> {
    i128::from(end)
        .checked_sub(i128::from(start))
        .ok_or_else(|| GeoAreaMajorityError::overflow(context))
}

fn checked_cross(
    left_x: i128,
    left_y: i128,
    right_x: i128,
    right_y: i128,
    context: &'static str,
) -> Result<i128, GeoAreaMajorityError> {
    let left = left_x
        .checked_mul(right_y)
        .ok_or_else(|| GeoAreaMajorityError::overflow(context))?;
    let right = left_y
        .checked_mul(right_x)
        .ok_or_else(|| GeoAreaMajorityError::overflow(context))?;
    left.checked_sub(right)
        .ok_or_else(|| GeoAreaMajorityError::overflow(context))
}

fn unsupported_topology(context: &'static str) -> GeoAreaMajorityError {
    GeoAreaMajorityError::unsupported_topology(context)
}

fn gcd_u128(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
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
