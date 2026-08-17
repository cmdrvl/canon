#![forbid(unsafe_code)]

//! Bounded parcel/building composition with exact residual reporting.
//!
//! This is the E4 walking skeleton, not the full solver proposed by
//! `docs/PLAN_CANON_GEO.md`. It deliberately implements only a small extensional
//! hard-constraint kernel. The useful product behavior is already present:
//! hard-feasible models are enumerated exactly within a declared budget, their
//! backbone is reported, and soft preferences only rank the residual.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_COMPOSITION_REQUEST_VERSION: &str = "canon_geo_composition_request.v0";
pub const CANON_GEO_COMPOSITION_VERSION: &str = "canon_geo_composition.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoEntityLevel {
    PoiUnit,
    Building,
    Parcel,
    Property,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoEntityRef {
    pub level: GeoEntityLevel,
    pub id: String,
}

impl GeoEntityRef {
    pub fn new(level: GeoEntityLevel, id: impl Into<String>) -> Self {
        Self {
            level,
            id: id.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoIdentityRelation {
    SameAs,
    Contains,
    PartOf,
    Within,
    On,
}

/// Enforce the level firewall before an identity relation enters a workbench.
///
/// Cross-level facts are relationships, never equality evidence.
pub fn validate_identity_relation(
    left: &GeoEntityRef,
    right: &GeoEntityRef,
    relation: GeoIdentityRelation,
) -> Result<(), GeoCompositionError> {
    validate_identifier("left.id", &left.id)?;
    validate_identifier("right.id", &right.id)?;
    if relation == GeoIdentityRelation::SameAs && left.level != right.level {
        return Err(GeoCompositionError::invalid_input(
            "Cross-level same_as is forbidden",
            [
                ("left_level", level_name(left.level)),
                ("right_level", level_name(right.level)),
            ],
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoBuildingCandidate {
    pub id: String,
    /// Parcel candidates on which this building may sit. An empty list means
    /// that no containment evidence was admitted, not that the building has no
    /// parcel.
    #[serde(default)]
    pub parcel_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCompositionUniverse {
    pub parcels: Vec<String>,
    pub buildings: Vec<GeoBuildingCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoHardConstraint {
    pub id: String,
    pub constraint: GeoHardConstraintKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeoHardConstraintKind {
    Require {
        member: GeoEntityRef,
    },
    Forbid {
        member: GeoEntityRef,
    },
    Cardinality {
        level: GeoEntityLevel,
        min: usize,
        max: usize,
    },
    /// Extensional set-domain restriction emitted by an admitted evidence
    /// channel. Each inner vector is one allowed set at the declared level.
    AllowedSets {
        level: GeoEntityLevel,
        sets: Vec<Vec<String>>,
    },
    /// At least one member of the declared candidate set must be selected.
    /// This is the sound image of an existential evidence statement; it does
    /// not imply that every candidate is part of the answer.
    AnyOf {
        members: Vec<GeoEntityRef>,
    },
    /// Exact integer additive band over selected members. Units and band
    /// calibration belong to the evidence contract that emitted this
    /// constraint; the solver only performs checked integer arithmetic.
    IntegerSumBand {
        level: GeoEntityLevel,
        values: Vec<GeoIntegerMemberValue>,
        min: u64,
        max: u64,
    },
    AllOrNone {
        members: Vec<GeoEntityRef>,
    },
    Requires {
        if_member: GeoEntityRef,
        then_member: GeoEntityRef,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoIntegerMemberValue {
    pub id: String,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoSoftPreference {
    pub id: String,
    pub member: GeoEntityRef,
    /// Exact integer cost added when `member` is absent. This cost affects only
    /// presentation order after the hard residual has been frozen.
    pub cost_if_absent: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCompositionRequest {
    pub version: String,
    pub universe: GeoCompositionUniverse,
    #[serde(default)]
    pub hard_constraints: Vec<GeoHardConstraint>,
    #[serde(default)]
    pub soft_preferences: Vec<GeoSoftPreference>,
    pub max_assignments: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoCompositionModel {
    pub parcels: Vec<String>,
    pub buildings: Vec<String>,
}

impl GeoCompositionModel {
    fn contains(&self, member: &GeoEntityRef) -> bool {
        match member.level {
            GeoEntityLevel::Parcel => self.parcels.binary_search(&member.id).is_ok(),
            GeoEntityLevel::Building => self.buildings.binary_search(&member.id).is_ok(),
            GeoEntityLevel::PoiUnit | GeoEntityLevel::Property => false,
        }
    }

    fn members(&self, level: GeoEntityLevel) -> Option<&[String]> {
        match level {
            GeoEntityLevel::Parcel => Some(&self.parcels),
            GeoEntityLevel::Building => Some(&self.buildings),
            GeoEntityLevel::PoiUnit | GeoEntityLevel::Property => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCompositionStatus {
    Resolved,
    Ambiguous,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCompositionBackbone {
    pub parcels: Vec<String>,
    pub buildings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoSoftRankedModel {
    pub rank: u64,
    pub cost: u64,
    pub model: GeoCompositionModel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCompositionSummary {
    pub parcel_candidates: usize,
    pub building_candidates: usize,
    pub candidate_assignments: u64,
    pub structurally_feasible_assignments: u64,
    pub hard_constraint_evaluations: u64,
    pub residual_model_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCompositionArtifact {
    pub version: String,
    pub request_version: String,
    pub status: GeoCompositionStatus,
    pub summary: GeoCompositionSummary,
    pub hard_forced: GeoCompositionBackbone,
    pub residual_models: Vec<GeoCompositionModel>,
    /// Presentation-only ordering. No member is promoted from this vector.
    pub soft_ranked: Vec<GeoSoftRankedModel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflict_constraint_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCompositionErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCompositionError {
    pub code: GeoCompositionErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoCompositionError {
    fn new(
        code: GeoCompositionErrorCode,
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

    fn invalid_input(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(GeoCompositionErrorCode::InvalidInput, message, detail)
    }

    fn overflow(context: &str) -> Self {
        Self::new(
            GeoCompositionErrorCode::ArithmeticOverflow,
            "Geo composition arithmetic overflowed",
            [("context", context)],
        )
    }
}

impl fmt::Display for GeoCompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoCompositionError {}

struct NormalizedRequest {
    request_version: String,
    parcels: Vec<String>,
    buildings: Vec<GeoBuildingCandidate>,
    hard_constraints: Vec<GeoHardConstraint>,
    soft_preferences: Vec<GeoSoftPreference>,
    max_assignments: u64,
}

/// Enumerate the exact hard-feasible parcel/building residual.
///
/// Assignment enumeration is refused before work begins when the full bounded
/// universe exceeds `max_assignments`.
pub fn solve_composition(
    request: &GeoCompositionRequest,
) -> Result<GeoCompositionArtifact, GeoCompositionError> {
    let request = normalize_request(request)?;
    let total_variables = request
        .parcels
        .len()
        .checked_add(request.buildings.len())
        .ok_or_else(|| GeoCompositionError::overflow("candidate variable count"))?;
    let candidate_assignments = assignment_count(total_variables, request.max_assignments)?;

    let structural_models =
        enumerate_structural_models(&request.parcels, &request.buildings, candidate_assignments);
    let (mut residual_models, hard_constraint_evaluations) =
        filter_models(&structural_models, &request.hard_constraints)?;
    residual_models.sort();

    let conflict_constraint_ids = if residual_models.is_empty() {
        irreducible_conflict(&structural_models, &request.hard_constraints)?
            .into_iter()
            .map(|constraint| constraint.id)
            .collect()
    } else {
        Vec::new()
    };
    let hard_forced = backbone(&residual_models);
    let soft_ranked = rank_residual(&residual_models, &request.soft_preferences)?;
    let status = match residual_models.len() {
        0 => GeoCompositionStatus::Conflict,
        1 => GeoCompositionStatus::Resolved,
        _ => GeoCompositionStatus::Ambiguous,
    };

    Ok(GeoCompositionArtifact {
        version: CANON_GEO_COMPOSITION_VERSION.to_string(),
        request_version: request.request_version,
        status,
        summary: GeoCompositionSummary {
            parcel_candidates: request.parcels.len(),
            building_candidates: request.buildings.len(),
            candidate_assignments,
            structurally_feasible_assignments: u64::try_from(structural_models.len())
                .map_err(|_| GeoCompositionError::overflow("structural model count"))?,
            hard_constraint_evaluations,
            residual_model_count: u64::try_from(residual_models.len())
                .map_err(|_| GeoCompositionError::overflow("residual model count"))?,
        },
        hard_forced,
        residual_models,
        soft_ranked,
        conflict_constraint_ids,
    })
}

pub fn canonical_composition_bytes(
    artifact: &GeoCompositionArtifact,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(artifact)
}

/// Validate and normalize-check a request without enumerating its assignments.
pub fn validate_composition_request(
    request: &GeoCompositionRequest,
) -> Result<(), GeoCompositionError> {
    normalize_request(request).map(|_| ())
}

/// Return the canonical request representation used by the solver.
pub fn canonicalize_composition_request(
    request: &GeoCompositionRequest,
) -> Result<GeoCompositionRequest, GeoCompositionError> {
    let normalized = normalize_request(request)?;
    Ok(GeoCompositionRequest {
        version: normalized.request_version,
        universe: GeoCompositionUniverse {
            parcels: normalized.parcels,
            buildings: normalized.buildings,
        },
        hard_constraints: normalized.hard_constraints,
        soft_preferences: normalized.soft_preferences,
        max_assignments: normalized.max_assignments,
    })
}

fn normalize_request(
    request: &GeoCompositionRequest,
) -> Result<NormalizedRequest, GeoCompositionError> {
    if request.version != CANON_GEO_COMPOSITION_REQUEST_VERSION {
        return Err(GeoCompositionError::new(
            GeoCompositionErrorCode::UnsupportedVersion,
            "Unsupported Geo composition request version",
            [
                ("actual", request.version.as_str()),
                ("expected", CANON_GEO_COMPOSITION_REQUEST_VERSION),
            ],
        ));
    }
    if request.max_assignments == 0 {
        return Err(GeoCompositionError::invalid_input(
            "Geo composition max_assignments must be positive",
            [("field", "max_assignments")],
        ));
    }

    let mut parcels = request.universe.parcels.clone();
    validate_and_sort_ids("universe.parcels", &mut parcels)?;
    if parcels.is_empty() {
        return Err(GeoCompositionError::invalid_input(
            "Geo composition requires at least one parcel candidate",
            [("field", "universe.parcels")],
        ));
    }
    let parcel_set = parcels.iter().cloned().collect::<BTreeSet<_>>();

    let mut buildings = request.universe.buildings.clone();
    for building in &mut buildings {
        validate_identifier("universe.buildings[].id", &building.id)?;
        validate_and_sort_ids("universe.buildings[].parcel_ids", &mut building.parcel_ids)?;
        for parcel_id in &building.parcel_ids {
            if !parcel_set.contains(parcel_id) {
                return Err(GeoCompositionError::invalid_input(
                    "Building containment references an unknown parcel",
                    [
                        ("building_id", building.id.as_str()),
                        ("parcel_id", parcel_id.as_str()),
                    ],
                ));
            }
        }
    }
    buildings.sort_by(|left, right| left.id.cmp(&right.id));
    reject_adjacent_duplicates(
        "universe.buildings",
        buildings.iter().map(|building| building.id.as_str()),
    )?;
    let building_set = buildings
        .iter()
        .map(|building| building.id.clone())
        .collect::<BTreeSet<_>>();

    let mut hard_constraints = request.hard_constraints.clone();
    for constraint in &mut hard_constraints {
        validate_identifier("hard_constraints[].id", &constraint.id)?;
        normalize_constraint(constraint, &parcel_set, &building_set)?;
    }
    hard_constraints.sort_by(|left, right| left.id.cmp(&right.id));
    reject_adjacent_duplicates(
        "hard_constraints",
        hard_constraints
            .iter()
            .map(|constraint| constraint.id.as_str()),
    )?;

    let mut soft_preferences = request.soft_preferences.clone();
    for preference in &soft_preferences {
        validate_identifier("soft_preferences[].id", &preference.id)?;
        validate_member(&preference.member, &parcel_set, &building_set)?;
    }
    soft_preferences.sort_by(|left, right| left.id.cmp(&right.id));
    reject_adjacent_duplicates(
        "soft_preferences",
        soft_preferences
            .iter()
            .map(|preference| preference.id.as_str()),
    )?;

    Ok(NormalizedRequest {
        request_version: request.version.clone(),
        parcels,
        buildings,
        hard_constraints,
        soft_preferences,
        max_assignments: request.max_assignments,
    })
}

fn normalize_constraint(
    constraint: &mut GeoHardConstraint,
    parcels: &BTreeSet<String>,
    buildings: &BTreeSet<String>,
) -> Result<(), GeoCompositionError> {
    match &mut constraint.constraint {
        GeoHardConstraintKind::Require { member } | GeoHardConstraintKind::Forbid { member } => {
            validate_member(member, parcels, buildings)?;
        }
        GeoHardConstraintKind::Cardinality { level, min, max } => {
            let available = level_cardinality(*level, parcels, buildings)?;
            if *min > *max || *max > available {
                return Err(GeoCompositionError::invalid_input(
                    "Invalid Geo composition cardinality bounds",
                    [
                        ("constraint_id".to_string(), constraint.id.clone()),
                        ("available".to_string(), available.to_string()),
                    ],
                ));
            }
        }
        GeoHardConstraintKind::AllowedSets { level, sets } => {
            level_cardinality(*level, parcels, buildings)?;
            if sets.is_empty() {
                return Err(GeoCompositionError::invalid_input(
                    "AllowedSets requires at least one allowed set",
                    [("constraint_id", constraint.id.as_str())],
                ));
            }
            for set in sets.iter_mut() {
                validate_and_sort_ids("hard_constraints[].allowed_sets", set)?;
                for id in set {
                    validate_member(&GeoEntityRef::new(*level, id.clone()), parcels, buildings)?;
                }
            }
            sets.sort();
            reject_adjacent_duplicates(
                "hard_constraints[].allowed_sets",
                sets.iter().map(|set| format!("{set:?}")),
            )?;
        }
        GeoHardConstraintKind::AnyOf { members } => {
            if members.is_empty() {
                return Err(GeoCompositionError::invalid_input(
                    "AnyOf requires at least one member",
                    [("constraint_id", constraint.id.as_str())],
                ));
            }
            for member in members.iter() {
                validate_member(member, parcels, buildings)?;
            }
            members.sort();
            reject_adjacent_duplicates(
                "hard_constraints[].any_of",
                members
                    .iter()
                    .map(|member| format!("{}:{}", level_name(member.level), member.id)),
            )?;
        }
        GeoHardConstraintKind::IntegerSumBand {
            level,
            values,
            min,
            max,
        } => {
            level_cardinality(*level, parcels, buildings)?;
            if values.is_empty() || *min > *max {
                return Err(GeoCompositionError::invalid_input(
                    "IntegerSumBand requires values and an ordered band",
                    [("constraint_id", constraint.id.as_str())],
                ));
            }
            for value in values.iter() {
                validate_member(
                    &GeoEntityRef::new(*level, value.id.clone()),
                    parcels,
                    buildings,
                )?;
            }
            values.sort();
            reject_adjacent_duplicates(
                "hard_constraints[].integer_sum_band",
                values.iter().map(|value| value.id.as_str()),
            )?;
            values.iter().try_fold(0_u64, |sum, value| {
                sum.checked_add(value.value)
                    .ok_or_else(|| GeoCompositionError::overflow("integer sum band total"))
            })?;
        }
        GeoHardConstraintKind::AllOrNone { members } => {
            if members.len() < 2 {
                return Err(GeoCompositionError::invalid_input(
                    "AllOrNone requires at least two members",
                    [("constraint_id", constraint.id.as_str())],
                ));
            }
            for member in members.iter() {
                validate_member(member, parcels, buildings)?;
            }
            members.sort();
            reject_adjacent_duplicates(
                "hard_constraints[].all_or_none",
                members
                    .iter()
                    .map(|member| format!("{}:{}", level_name(member.level), member.id)),
            )?;
        }
        GeoHardConstraintKind::Requires {
            if_member,
            then_member,
        } => {
            validate_member(if_member, parcels, buildings)?;
            validate_member(then_member, parcels, buildings)?;
        }
    }
    Ok(())
}

fn validate_member(
    member: &GeoEntityRef,
    parcels: &BTreeSet<String>,
    buildings: &BTreeSet<String>,
) -> Result<(), GeoCompositionError> {
    validate_identifier("member.id", &member.id)?;
    let present = match member.level {
        GeoEntityLevel::Parcel => parcels.contains(&member.id),
        GeoEntityLevel::Building => buildings.contains(&member.id),
        GeoEntityLevel::PoiUnit | GeoEntityLevel::Property => {
            return Err(GeoCompositionError::invalid_input(
                "Composition constraints support only parcel and building levels",
                [("level", level_name(member.level))],
            ));
        }
    };
    if !present {
        return Err(GeoCompositionError::invalid_input(
            "Composition constraint references an unknown member",
            [
                ("level", level_name(member.level)),
                ("member_id", member.id.as_str()),
            ],
        ));
    }
    Ok(())
}

fn validate_identifier(field: &str, value: &str) -> Result<(), GeoCompositionError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoCompositionError::invalid_input(
            "Geo identifiers must be non-empty and already canonical",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_and_sort_ids(field: &str, values: &mut [String]) -> Result<(), GeoCompositionError> {
    for value in values.iter() {
        validate_identifier(field, value)?;
    }
    values.sort();
    reject_adjacent_duplicates(field, values.iter().map(String::as_str))
}

fn reject_adjacent_duplicates<T>(
    field: &str,
    values: impl IntoIterator<Item = T>,
) -> Result<(), GeoCompositionError>
where
    T: AsRef<str>,
{
    let mut previous: Option<String> = None;
    for value in values {
        let value = value.as_ref();
        if previous.as_deref() == Some(value) {
            return Err(GeoCompositionError::invalid_input(
                "Geo composition input contains a duplicate",
                [("field", field), ("value", value)],
            ));
        }
        previous = Some(value.to_string());
    }
    Ok(())
}

fn level_cardinality(
    level: GeoEntityLevel,
    parcels: &BTreeSet<String>,
    buildings: &BTreeSet<String>,
) -> Result<usize, GeoCompositionError> {
    match level {
        GeoEntityLevel::Parcel => Ok(parcels.len()),
        GeoEntityLevel::Building => Ok(buildings.len()),
        GeoEntityLevel::PoiUnit | GeoEntityLevel::Property => {
            Err(GeoCompositionError::invalid_input(
                "Composition constraints support only parcel and building levels",
                [("level", level_name(level))],
            ))
        }
    }
}

fn assignment_count(
    total_variables: usize,
    max_assignments: u64,
) -> Result<u64, GeoCompositionError> {
    let count = u32::try_from(total_variables)
        .ok()
        .filter(|count| *count < u64::BITS)
        .map(|count| 1_u64 << count);
    let Some(count) = count else {
        return Err(GeoCompositionError::new(
            GeoCompositionErrorCode::BudgetExceeded,
            "Geo composition universe exceeds the declared assignment budget",
            [
                ("estimated_assignments", format!("2^{total_variables}")),
                ("max_assignments", max_assignments.to_string()),
            ],
        ));
    };
    if count > max_assignments {
        return Err(GeoCompositionError::new(
            GeoCompositionErrorCode::BudgetExceeded,
            "Geo composition universe exceeds the declared assignment budget",
            [
                ("estimated_assignments", count.to_string()),
                ("max_assignments", max_assignments.to_string()),
            ],
        ));
    }
    Ok(count)
}

fn enumerate_structural_models(
    parcels: &[String],
    buildings: &[GeoBuildingCandidate],
    candidate_assignments: u64,
) -> Vec<GeoCompositionModel> {
    let mut models = Vec::new();
    for mask in 0..candidate_assignments {
        let selected_parcels = parcels
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1_u64 << index) != 0)
            .map(|(_, id)| id.clone())
            .collect::<Vec<_>>();
        if selected_parcels.is_empty() {
            continue;
        }
        let selected_parcel_set = selected_parcels.iter().collect::<BTreeSet<_>>();
        let selected_buildings = buildings
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1_u64 << (parcels.len() + index)) != 0)
            .filter(|(_, building)| {
                building.parcel_ids.is_empty()
                    || building
                        .parcel_ids
                        .iter()
                        .any(|parcel_id| selected_parcel_set.contains(parcel_id))
            })
            .map(|(_, building)| building.id.clone())
            .collect::<Vec<_>>();
        let selected_building_count = buildings
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1_u64 << (parcels.len() + index)) != 0)
            .count();
        if selected_buildings.len() != selected_building_count {
            continue;
        }
        models.push(GeoCompositionModel {
            parcels: selected_parcels,
            buildings: selected_buildings,
        });
    }
    models
}

fn filter_models(
    models: &[GeoCompositionModel],
    constraints: &[GeoHardConstraint],
) -> Result<(Vec<GeoCompositionModel>, u64), GeoCompositionError> {
    let mut residual = Vec::new();
    let mut evaluations = 0_u64;
    for model in models {
        let mut feasible = true;
        for constraint in constraints {
            evaluations = evaluations
                .checked_add(1)
                .ok_or_else(|| GeoCompositionError::overflow("constraint evaluation count"))?;
            if !constraint_holds(model, &constraint.constraint) {
                feasible = false;
                break;
            }
        }
        if feasible {
            residual.push(model.clone());
        }
    }
    Ok((residual, evaluations))
}

fn constraint_holds(model: &GeoCompositionModel, constraint: &GeoHardConstraintKind) -> bool {
    match constraint {
        GeoHardConstraintKind::Require { member } => model.contains(member),
        GeoHardConstraintKind::Forbid { member } => !model.contains(member),
        GeoHardConstraintKind::Cardinality { level, min, max } => model
            .members(*level)
            .is_some_and(|members| (*min..=*max).contains(&members.len())),
        GeoHardConstraintKind::AllowedSets { level, sets } => model
            .members(*level)
            .is_some_and(|members| sets.iter().any(|allowed| allowed == members)),
        GeoHardConstraintKind::AnyOf { members } => {
            members.iter().any(|member| model.contains(member))
        }
        GeoHardConstraintKind::IntegerSumBand {
            level,
            values,
            min,
            max,
        } => model.members(*level).is_some_and(|members| {
            let sum = values
                .iter()
                .filter(|value| members.binary_search(&value.id).is_ok())
                .map(|value| value.value)
                .sum::<u64>();
            (*min..=*max).contains(&sum)
        }),
        GeoHardConstraintKind::AllOrNone { members } => {
            let selected = members
                .iter()
                .filter(|member| model.contains(member))
                .count();
            selected == 0 || selected == members.len()
        }
        GeoHardConstraintKind::Requires {
            if_member,
            then_member,
        } => !model.contains(if_member) || model.contains(then_member),
    }
}

fn irreducible_conflict(
    models: &[GeoCompositionModel],
    constraints: &[GeoHardConstraint],
) -> Result<Vec<GeoHardConstraint>, GeoCompositionError> {
    let mut core = constraints.to_vec();
    let mut index = 0;
    while index < core.len() {
        let mut candidate = core.clone();
        candidate.remove(index);
        let (residual, _) = filter_models(models, &candidate)?;
        if residual.is_empty() {
            core = candidate;
        } else {
            index += 1;
        }
    }
    Ok(core)
}

fn backbone(models: &[GeoCompositionModel]) -> GeoCompositionBackbone {
    let Some(first) = models.first() else {
        return GeoCompositionBackbone {
            parcels: Vec::new(),
            buildings: Vec::new(),
        };
    };
    let parcels = first
        .parcels
        .iter()
        .filter(|id| models.iter().all(|model| model.parcels.contains(id)))
        .cloned()
        .collect();
    let buildings = first
        .buildings
        .iter()
        .filter(|id| models.iter().all(|model| model.buildings.contains(id)))
        .cloned()
        .collect();
    GeoCompositionBackbone { parcels, buildings }
}

fn rank_residual(
    models: &[GeoCompositionModel],
    preferences: &[GeoSoftPreference],
) -> Result<Vec<GeoSoftRankedModel>, GeoCompositionError> {
    let mut ranked = Vec::with_capacity(models.len());
    for model in models {
        let mut cost = 0_u64;
        for preference in preferences {
            if !model.contains(&preference.member) {
                cost = cost
                    .checked_add(preference.cost_if_absent)
                    .ok_or_else(|| GeoCompositionError::overflow("soft preference cost"))?;
            }
        }
        ranked.push((cost, model.clone()));
    }
    ranked.sort();
    ranked
        .into_iter()
        .enumerate()
        .map(|(index, (cost, model))| {
            Ok(GeoSoftRankedModel {
                rank: u64::try_from(index + 1)
                    .map_err(|_| GeoCompositionError::overflow("soft rank"))?,
                cost,
                model,
            })
        })
        .collect()
}

const fn level_name(level: GeoEntityLevel) -> &'static str {
    match level {
        GeoEntityLevel::PoiUnit => "poi_unit",
        GeoEntityLevel::Building => "building",
        GeoEntityLevel::Parcel => "parcel",
        GeoEntityLevel::Property => "property",
    }
}
