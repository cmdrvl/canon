#![forbid(unsafe_code)]

//! Place grains, place evidence types and grain-safe hierarchy for the shared
//! session solver.
//!
//! A place is resolved the way a name is: observations carry an `entity_type`,
//! evidence ties pairs of observations, and the solver accepts, contradicts or
//! abstains. This module only supplies the place vocabulary that layers on the
//! existing session tiers; it owns no second solver.
//!
//! * A **grain** is an `entity_type` (`site`, `parcel`, `building`, `unit`,
//!   `address`). Each grain is its own registry kind, so two grains never share
//!   a node, a cluster or an id, even when they share a surface string.
//! * **Support** types are `pair_support` evidence between two observations of
//!   the same grain. Tier placement stays in the rule table; nothing here
//!   grants authority.
//! * **Veto** types are `anti_merge_veto` evidence. They are hard cannot-links
//!   exactly like `distinct`.
//! * **Hierarchy** relations (`part_of`, `located_on`, `contains_unit`) are the
//!   only evidence allowed to cross grains, and only along the pairs listed in
//!   [`HIERARCHY`]. They are relations, never identity.

use serde_json::{Map, Value};

/// The place entity grains, finest last by convention only.
pub const GRAINS: &[&str] = &["site", "parcel", "building", "unit", "address"];

/// Same-grain support that may sit in `tier_a.evidence_types`: a parcel
/// identifier shared by both observations.
pub const PLACE_TIER_A_TYPES: &[&str] = &["same_parcel_id", "same_apn"];

/// Same-grain support that may sit in `tier_b.eligible_evidence_types`.
pub const PLACE_TIER_B_TYPES: &[&str] = &[
    "address_canonical_match",
    "point_in_parcel",
    "footprint_overlap",
    "deed_chain_link",
    "assessor_corroboration",
];

/// Place anti-merge kinds. Each is a hard cannot-link, like `distinct`.
pub const PLACE_VETO_TYPES: &[&str] = &[
    "conflicting_authoritative_apn",
    "disjoint_parcels",
    "grain_mismatch",
];

/// Relations that may connect two different grains.
pub const PLACE_RELATIONS: &[&str] = &["part_of", "located_on", "contains_unit"];

/// The solve contradiction reason for evidence that would merge two grains.
pub const GRAIN_MISMATCH_REASON: &str = "grain_mismatch";

/// `(relation, subject grain, object grain)` triples that may cross grains.
/// `part_of` and `contains_unit` are inverse readings of one containment; both
/// are stored as the directed fact that was asserted.
const HIERARCHY: &[(&str, &str, &str)] = &[
    ("part_of", "unit", "building"),
    ("part_of", "unit", "site"),
    ("part_of", "building", "site"),
    ("part_of", "parcel", "site"),
    ("located_on", "building", "parcel"),
    ("located_on", "unit", "parcel"),
    ("located_on", "address", "parcel"),
    ("located_on", "address", "building"),
    ("contains_unit", "building", "unit"),
    ("contains_unit", "site", "unit"),
];

pub fn is_place_grain(entity_type: &str) -> bool {
    GRAINS.contains(&entity_type)
}

pub fn is_place_support_type(evidence_type: &str) -> bool {
    PLACE_TIER_A_TYPES.contains(&evidence_type) || PLACE_TIER_B_TYPES.contains(&evidence_type)
}

pub fn is_place_veto_type(evidence_type: &str) -> bool {
    PLACE_VETO_TYPES.contains(&evidence_type)
}

/// Whether `relation` may link a `subject_grain` to an `object_grain`.
pub fn hierarchy_allows(relation: &str, subject_grain: &str, object_grain: &str) -> bool {
    HIERARCHY.iter().any(|(name, subject, object)| {
        *name == relation && *subject == subject_grain && *object == object_grain
    })
}

/// A short assertion whose basis does not carry what the place type needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceBasisError {
    pub field: String,
    pub message: String,
}

impl PlaceBasisError {
    fn new(field: &str, message: impl Into<String>) -> Self {
        Self {
            field: field.to_string(),
            message: message.into(),
        }
    }
}

/// Whether Tier-A place support needs the cited source that `asserted_alias`
/// also needs: a parcel identifier is authoritative, so a non-human asserter
/// has to point at the record that carries it.
pub fn requires_citation(assertion_type: &str) -> bool {
    PLACE_TIER_A_TYPES.contains(&assertion_type)
}

/// The basis field that makes a place support claim inspectable.
fn required_basis_field(assertion_type: &str) -> Option<&'static str> {
    match assertion_type {
        "same_parcel_id" => Some("parcel_id"),
        "same_apn" => Some("apn"),
        "address_canonical_match" => Some("canonical_address"),
        "point_in_parcel" | "footprint_overlap" => Some("method"),
        "deed_chain_link" => Some("instrument"),
        "assessor_corroboration" => Some("field"),
        _ => None,
    }
}

/// Validate the basis of a place assertion. Types outside the place vocabulary
/// are not this module's concern and pass.
pub fn validate_basis(
    assertion_type: &str,
    basis: &Map<String, Value>,
) -> Result<(), PlaceBasisError> {
    if let Some(field) = required_basis_field(assertion_type) {
        non_empty_string(basis, field)?;
        if assertion_type == "footprint_overlap"
            && let Some(ratio) = basis.get("overlap_ratio")
        {
            let valid = ratio
                .as_f64()
                .is_some_and(|ratio| ratio.is_finite() && ratio > 0.0 && ratio <= 1.0);
            if !valid {
                return Err(PlaceBasisError::new(
                    "overlap_ratio",
                    "basis.overlap_ratio must be a number in (0, 1]",
                ));
            }
        }
        return Ok(());
    }
    match assertion_type {
        "conflicting_authoritative_apn" => {
            let left = non_empty_string(basis, "left_value")?;
            let right = non_empty_string(basis, "right_value")?;
            if left == right {
                return Err(PlaceBasisError::new(
                    "right_value",
                    "a conflicting_authoritative_apn veto needs two different identifiers",
                ));
            }
            Ok(())
        }
        "disjoint_parcels" => {
            let distance = number(basis, "distance_m")?;
            let threshold = number(basis, "threshold_m")?;
            if threshold < 0.0 || distance <= threshold {
                return Err(PlaceBasisError::new(
                    "distance_m",
                    "a disjoint_parcels veto needs basis.distance_m beyond basis.threshold_m (>= 0)",
                ));
            }
            Ok(())
        }
        "grain_mismatch" => {
            let left = non_empty_string(basis, "left_grain")?;
            let right = non_empty_string(basis, "right_grain")?;
            if !is_place_grain(&left) {
                return Err(PlaceBasisError::new(
                    "left_grain",
                    format!("basis.left_grain must be one of {}", GRAINS.join(", ")),
                ));
            }
            if !is_place_grain(&right) {
                return Err(PlaceBasisError::new(
                    "right_grain",
                    format!("basis.right_grain must be one of {}", GRAINS.join(", ")),
                ));
            }
            if left == right {
                return Err(PlaceBasisError::new(
                    "right_grain",
                    "a grain_mismatch veto needs two different grains",
                ));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn non_empty_string(basis: &Map<String, Value>, field: &str) -> Result<String, PlaceBasisError> {
    basis
        .get(field)
        .and_then(Value::as_str)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            PlaceBasisError::new(field, format!("basis.{field} must be a non-empty string"))
        })
}

fn number(basis: &Map<String, Value>, field: &str) -> Result<f64, PlaceBasisError> {
    basis
        .get(field)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| {
            PlaceBasisError::new(field, format!("basis.{field} must be a finite number"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn basis(value: Value) -> Map<String, Value> {
        value.as_object().expect("object").clone()
    }

    #[test]
    fn grains_are_the_five_place_grains() {
        for grain in ["site", "parcel", "building", "unit", "address"] {
            assert!(is_place_grain(grain));
        }
        assert!(!is_place_grain("organisation"));
        assert!(!is_place_grain("Parcel"));
    }

    #[test]
    fn hierarchy_never_collapses_a_finer_grain_into_a_coarser_one() {
        assert!(hierarchy_allows("part_of", "unit", "building"));
        assert!(hierarchy_allows("located_on", "building", "parcel"));
        assert!(hierarchy_allows("contains_unit", "building", "unit"));
        // Reversed directions and same-rank pairs are not hierarchy.
        assert!(!hierarchy_allows("part_of", "building", "unit"));
        assert!(!hierarchy_allows("part_of", "building", "parcel"));
        assert!(!hierarchy_allows("located_on", "parcel", "building"));
        assert!(!hierarchy_allows("contains_unit", "unit", "building"));
        assert!(!hierarchy_allows("parent_of", "unit", "building"));
    }

    #[test]
    fn support_and_veto_tables_are_disjoint() {
        for kind in PLACE_VETO_TYPES {
            assert!(!is_place_support_type(kind), "{kind}");
        }
        for kind in PLACE_TIER_A_TYPES.iter().chain(PLACE_TIER_B_TYPES) {
            assert!(!is_place_veto_type(kind), "{kind}");
            assert!(required_basis_field(kind).is_some(), "{kind}");
        }
    }

    #[test]
    fn support_basis_needs_its_inspectable_field() {
        assert!(validate_basis("same_apn", &basis(json!({"apn": "1022-0043"}))).is_ok());
        let missing = validate_basis("same_apn", &basis(json!({"parcel_id": "x"})))
            .expect_err("apn is required");
        assert_eq!(missing.field, "apn");
        let blank = validate_basis(
            "address_canonical_match",
            &basis(json!({"canonical_address": "  "})),
        )
        .expect_err("blank refused");
        assert_eq!(blank.field, "canonical_address");
        assert!(
            validate_basis(
                "footprint_overlap",
                &basis(json!({"method": "ST_INTERSECTS", "overlap_ratio": 0.82})),
            )
            .is_ok()
        );
        for ratio in [json!(0), json!(1.5), json!("high")] {
            let error = validate_basis(
                "footprint_overlap",
                &basis(json!({"method": "ST_INTERSECTS", "overlap_ratio": ratio})),
            )
            .expect_err("ratio out of range");
            assert_eq!(error.field, "overlap_ratio");
        }
    }

    #[test]
    fn conflicting_apn_veto_needs_two_different_identifiers() {
        assert!(
            validate_basis(
                "conflicting_authoritative_apn",
                &basis(json!({"left_value": "1022-0043", "right_value": "1022-0044"})),
            )
            .is_ok()
        );
        let same = validate_basis(
            "conflicting_authoritative_apn",
            &basis(json!({"left_value": "1022-0043", "right_value": " 1022-0043 "})),
        )
        .expect_err("equal identifiers do not conflict");
        assert_eq!(same.field, "right_value");
    }

    #[test]
    fn disjoint_veto_needs_distance_beyond_the_declared_threshold() {
        assert!(
            validate_basis(
                "disjoint_parcels",
                &basis(json!({"distance_m": 450.0, "threshold_m": 25})),
            )
            .is_ok()
        );
        for (distance, threshold) in [(10.0, 25.0), (25.0, 25.0), (30.0, -1.0)] {
            let error = validate_basis(
                "disjoint_parcels",
                &basis(json!({"distance_m": distance, "threshold_m": threshold})),
            )
            .expect_err("distance is not beyond the threshold");
            assert_eq!(error.field, "distance_m");
        }
        let missing = validate_basis("disjoint_parcels", &basis(json!({"distance_m": 90})))
            .expect_err("threshold must be declared");
        assert_eq!(missing.field, "threshold_m");
    }

    #[test]
    fn grain_mismatch_veto_needs_two_different_place_grains() {
        assert!(
            validate_basis(
                "grain_mismatch",
                &basis(json!({"left_grain": "building", "right_grain": "address"})),
            )
            .is_ok()
        );
        assert_eq!(
            validate_basis(
                "grain_mismatch",
                &basis(json!({"left_grain": "building", "right_grain": "building"})),
            )
            .expect_err("same grain")
            .field,
            "right_grain"
        );
        assert_eq!(
            validate_basis(
                "grain_mismatch",
                &basis(json!({"left_grain": "building", "right_grain": "organisation"})),
            )
            .expect_err("not a place grain")
            .field,
            "right_grain"
        );
    }

    #[test]
    fn types_outside_the_place_vocabulary_pass_untouched() {
        assert!(validate_basis("same_slot", &basis(json!({}))).is_ok());
        assert!(validate_basis("distinct", &basis(json!({}))).is_ok());
    }
}
