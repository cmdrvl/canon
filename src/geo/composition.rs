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
    /// Exact integer additive band over selected members. The measure identity,
    /// unit, and value origin travel with the constraint so source-asserted
    /// areas cannot silently be mixed with exact geometry-derived areas.
    IntegerSumBand {
        level: GeoEntityLevel,
        measure: GeoIntegerMeasure,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoIntegerValueOrigin {
    SourceAsserted,
    ExactDerived,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeoIntegerMeasure {
    pub semantic_id: String,
    pub unit: String,
    pub value_origin: GeoIntegerValueOrigin,
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
    /// Legacy strategy work limit. Exhaustive enumeration measures candidate
    /// assignment masks; closed-form AnyOf measures inclusion-exclusion
    /// subset masks; pruned DFS measures search-node visits. Consumers must
    /// pair the value with the reported component strategy.
    pub max_assignments: u64,
    /// Upper bound on how many combined residual models may be materialized
    /// into `residual_models` for presentation. The exact residual count,
    /// backbone, and component solutions are reported regardless of this
    /// budget; when the residual exceeds it, only the compact component
    /// representation is emitted.
    #[serde(default = "default_max_materialized_models")]
    pub max_materialized_models: u64,
}

/// Default cap on combined residual models materialized for presentation.
pub const DEFAULT_MAX_MATERIALIZED_MODELS: u64 = 4_096;

/// Absolute safety ceiling for the AnyOf-only closed-form fast path, measured
/// in inclusion-exclusion subset masks. The effective ceiling is the lesser
/// of this value and the request's declared work limit. Requests above it take
/// the component solver, whose exhaustion is a typed `BudgetFallback` — the
/// fast path must never become an unbudgeted 2^k enumeration. The ceiling leaves
/// orders-of-magnitude headroom over every measured adjudication case
/// (n <= 92, k <= 5 in the mask-loop regime; the 25-disc H4 case enters the
/// n >= 128 saturation branch and never loops).
const ANYOF_FASTPATH_MAX_MASK_VISITS: u128 = 1 << 24;

fn default_max_materialized_models() -> u64 {
    DEFAULT_MAX_MATERIALIZED_MODELS
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
    /// At least one constraint-connected component exceeded the declared
    /// assignment budget before an exact residual could be produced. The
    /// outcome is a typed handoff with recovery guidance, never a guess.
    BudgetFallback,
}

/// Semantic projection counted by `residual_model_count`. The current v0
/// kernel has only parcel/building decision variables, so counts are over
/// distinct entity selections, never internal search assignments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoModelCountScope {
    EntitySelection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCompositionBackbone {
    pub parcels: Vec<String>,
    pub buildings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoSoftRankedModel {
    pub rank: u64,
    /// Exact additive presentation cost. `u128` covers the maximum sum of a
    /// representable vector of `u64` preferences on supported Rust targets.
    pub cost: u128,
    pub model: GeoCompositionModel,
}

/// Typed component-budget handoff emitted when a deterministic bounded
/// search cannot complete inside `max_assignments`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCompositionFallback {
    pub component_keys: Vec<String>,
    pub max_component_variables: usize,
    pub configured_max_assignments: u64,
    pub guidance: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoCompositionSearchStrategy {
    AnyOfInclusionExclusion,
    ExhaustiveEnumeration,
    PrunedDepthFirst,
    BudgetFallback,
}

/// Auditable connected component of the variable/constraint incidence graph.
/// Local counts are present for component-wise enumeration/search; the global
/// AnyOf closed form records exact global counts while leaving local counts
/// absent, and budget fallback leaves them absent because search did not finish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCompositionComponentSummary {
    pub key: String,
    pub variables: Vec<GeoEntityRef>,
    pub constraint_ids: Vec<String>,
    pub strategy: GeoCompositionSearchStrategy,
    pub exact: bool,
    pub search_visits: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hard_feasible_assignments: Option<u64>,
    #[serde(default)]
    pub hard_feasible_assignments_saturated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub positive_assignments: Option<u64>,
    #[serde(default)]
    pub positive_assignments_saturated: bool,
}

/// Content-addressed source link when a composition was solved directly from
/// an admitted evidence-compilation artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoEvidenceCompilationReference {
    pub version: String,
    pub request_version: String,
    pub blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCompositionSummary {
    pub parcel_candidates: usize,
    pub building_candidates: usize,
    pub candidate_assignments: u64,
    /// True only when `candidate_assignments` is the declared saturated lower
    /// bound rather than the exact count.
    #[serde(default)]
    pub candidate_assignments_saturated: bool,
    pub structurally_feasible_assignments: u64,
    /// Whether the structural count was completed. When false, the numeric
    /// field is an unavailable placeholder and must not be interpreted as
    /// zero. When true, combine this flag with the saturation flag to
    /// distinguish an exact u64 count from a declared lower bound.
    #[serde(default)]
    pub structurally_feasible_assignments_complete: bool,
    /// True only when `structurally_feasible_assignments` is saturated.
    #[serde(default)]
    pub structurally_feasible_assignments_saturated: bool,
    pub hard_constraint_evaluations: u64,
    /// Whether evaluation accounting covers the completed solve. Budget
    /// fallback reports partial work in component summaries instead.
    #[serde(default)]
    pub hard_constraint_evaluations_complete: bool,
    /// True only when `hard_constraint_evaluations` is saturated.
    #[serde(default)]
    pub hard_constraint_evaluations_saturated: bool,
    pub residual_model_count: u64,
    pub model_count_scope: GeoModelCountScope,
    /// Whether the residual count was completed. If false, the numeric field
    /// is an unavailable placeholder, not a proof of conflict. If true and
    /// `residual_model_count_saturated` is false, the count is exact.
    #[serde(default)]
    pub residual_model_count_complete: bool,
    /// True only when `residual_model_count` is the declared saturated lower
    /// bound rather than the exact residual count.
    #[serde(default)]
    pub residual_model_count_saturated: bool,
    /// Aggregate convenience flag: true when any individual counter above is
    /// saturated. Consumers making claims about one count must use that
    /// count's specific flag instead.
    #[serde(default)]
    pub summary_counts_saturated: bool,
    pub component_count: usize,
    pub residual_models_materialized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoCompositionArtifact {
    pub version: String,
    pub request_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_compilation: Option<GeoEvidenceCompilationReference>,
    pub status: GeoCompositionStatus,
    pub summary: GeoCompositionSummary,
    pub hard_forced: GeoCompositionBackbone,
    /// Whether `hard_forced` is the complete backbone of the hard-feasible
    /// residual. Budget fallbacks and conflicts do not claim completeness.
    #[serde(default)]
    pub backbone_complete: bool,
    pub factorization: Vec<GeoCompositionComponentSummary>,
    pub residual_models: Vec<GeoCompositionModel>,
    /// Presentation-only ordering. No member is promoted from this vector.
    pub soft_ranked: Vec<GeoSoftRankedModel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflict_constraint_ids: Vec<String>,
    /// Present only for conflicts. `true` means the listed ids are the
    /// completed irreducible core; `false` means conflict is proven but the
    /// explanation budget allowed only a deterministic superset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict_core_complete: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_fallback: Option<GeoCompositionFallback>,
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
    max_materialized_models: u64,
}

/// Solve the exact hard-feasible parcel/building residual.
///
/// The variable space decomposes over the constraint-incidence graph:
/// structural building-to-parcel containment and every hard constraint's
/// referenced members couple variables into components; unconnected variables
/// remain free booleans. Each component is solved exactly inside
/// `max_assignments`; a component whose space exceeds the budget falls to a
/// deterministic depth-first search with partial-feasibility pruning, and
/// budget exhaustion there produces a typed `BudgetFallback` handoff instead
/// of a guess. The combined residual is the constrained product of component
/// solutions minus the all-empty-parcel combinations; it is materialized into
/// `residual_models` only when it fits `max_materialized_models`. Count
/// exactness and backbone completeness are explicit; a budget fallback never
/// manufactures either view.
pub fn solve_composition(
    request: &GeoCompositionRequest,
) -> Result<GeoCompositionArtifact, GeoCompositionError> {
    let request = normalize_request(request)?;
    let solver = FactorizedSolver::new(&request)?;
    solver.solve()
}

/// Test one concrete model against the normalized request contract without
/// enumerating the residual: universe membership, at-least-one-parcel, the
/// structural containment rule, and every hard constraint must hold. Because
/// the residual is exactly the set of such models, this decides residual
/// membership directly and stays exact whether or not the residual was
/// materialized.
pub fn model_satisfies_request(
    request: &GeoCompositionRequest,
    model: &GeoCompositionModel,
) -> Result<bool, GeoCompositionError> {
    let request = normalize_request(request)?;
    Ok(structural_model_holds(&request, model)
        && request
            .hard_constraints
            .iter()
            .all(|constraint| constraint_holds(model, &constraint.constraint)))
}

fn structural_model_holds(request: &NormalizedRequest, model: &GeoCompositionModel) -> bool {
    if model.parcels.is_empty()
        || !is_sorted_distinct(&model.parcels)
        || !is_sorted_distinct(&model.buildings)
    {
        return false;
    }
    if model
        .parcels
        .iter()
        .any(|id| request.parcels.binary_search(id).is_err())
    {
        return false;
    }
    if model.buildings.iter().any(|id| {
        request
            .buildings
            .binary_search_by(|probe| probe.id.as_str().cmp(id.as_str()))
            .is_err()
    }) {
        return false;
    }
    model.buildings.iter().all(|id| {
        let Ok(building_index) = request
            .buildings
            .binary_search_by(|probe| probe.id.as_str().cmp(id.as_str()))
        else {
            return false;
        };
        let building = &request.buildings[building_index];
        building.parcel_ids.is_empty()
            || building
                .parcel_ids
                .iter()
                .any(|parcel_id| model.parcels.binary_search(parcel_id).is_ok())
    })
}

fn is_sorted_distinct(values: &[String]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VarLevel {
    Parcel,
    Building,
}

struct FactorizedSolver<'a> {
    request: &'a NormalizedRequest,
    total_variables: usize,
}

impl<'a> FactorizedSolver<'a> {
    fn new(request: &'a NormalizedRequest) -> Result<Self, GeoCompositionError> {
        let total_variables = request
            .parcels
            .len()
            .checked_add(request.buildings.len())
            .ok_or_else(|| GeoCompositionError::overflow("candidate variable count"))?;
        Ok(Self {
            request,
            total_variables,
        })
    }

    fn var_level(&self, index: usize) -> VarLevel {
        if index < self.request.parcels.len() {
            VarLevel::Parcel
        } else {
            VarLevel::Building
        }
    }

    fn var_id(&self, index: usize) -> &str {
        match self.var_level(index) {
            VarLevel::Parcel => &self.request.parcels[index],
            VarLevel::Building => &self.request.buildings[index - self.request.parcels.len()].id,
        }
    }

    fn parcel_index(&self, id: &str) -> Option<usize> {
        self.request
            .parcels
            .binary_search_by(|probe| probe.as_str().cmp(id))
            .ok()
    }

    fn building_index(&self, id: &str) -> Option<usize> {
        self.request
            .buildings
            .binary_search_by(|probe| probe.id.as_str().cmp(id))
            .ok()
            .map(|index| self.request.parcels.len() + index)
    }

    fn var_index(&self, member: &GeoEntityRef) -> Option<usize> {
        match member.level {
            GeoEntityLevel::Parcel => self.parcel_index(&member.id),
            GeoEntityLevel::Building => self.building_index(&member.id),
            GeoEntityLevel::PoiUnit | GeoEntityLevel::Property => None,
        }
    }

    /// Global variable indices referenced by a constraint. Level-wide kinds
    /// span every variable of their level, which is what keeps them from
    /// being silently treated as local.
    fn constraint_members(&self, constraint: &GeoHardConstraint) -> Vec<usize> {
        let all_of_level = |level: GeoEntityLevel| {
            (0..self.total_variables)
                .filter(|index| match level {
                    GeoEntityLevel::Parcel => self.var_level(*index) == VarLevel::Parcel,
                    _ => self.var_level(*index) == VarLevel::Building,
                })
                .collect::<Vec<_>>()
        };
        match &constraint.constraint {
            GeoHardConstraintKind::Require { member }
            | GeoHardConstraintKind::Forbid { member } => {
                self.var_index(member).into_iter().collect()
            }
            GeoHardConstraintKind::Cardinality { level, .. }
            | GeoHardConstraintKind::AllowedSets { level, .. } => all_of_level(*level),
            GeoHardConstraintKind::AnyOf { members }
            | GeoHardConstraintKind::AllOrNone { members } => {
                members.iter().filter_map(|m| self.var_index(m)).collect()
            }
            GeoHardConstraintKind::IntegerSumBand { level, values, .. } => values
                .iter()
                .filter_map(|value| self.var_index(&GeoEntityRef::new(*level, value.id.clone())))
                .collect(),
            GeoHardConstraintKind::Requires {
                if_member,
                then_member,
            } => [if_member, then_member]
                .iter()
                .filter_map(|m| self.var_index(m))
                .collect(),
        }
    }

    /// Connected components of the variable-incidence graph, each ascending
    /// by global index. Structural containment and every hard constraint union
    /// their referenced variables. Union-find avoids materializing the
    /// quadratic clique implied by a level-wide constraint.
    fn components(&self) -> Vec<Vec<usize>> {
        let mut sets = DisjointSet::new(self.total_variables);
        for offset in 0..self.request.buildings.len() {
            let building_index = self.request.parcels.len() + offset;
            for parcel_id in &self.request.buildings[offset].parcel_ids {
                if let Some(parcel_index) = self.parcel_index(parcel_id) {
                    sets.union(building_index, parcel_index);
                }
            }
        }
        for constraint in &self.request.hard_constraints {
            let members = self.constraint_members(constraint);
            if let Some(first) = members.first() {
                for member in members.iter().skip(1) {
                    sets.union(*first, *member);
                }
            }
        }

        let mut by_root: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for variable in 0..self.total_variables {
            let root = sets.find(variable);
            by_root.entry(root).or_default().push(variable);
        }
        let mut components = by_root.into_values().collect::<Vec<_>>();
        components.sort_by_key(|members| members[0]);
        components
    }

    /// Per-component lists of indices into `hard_constraints`. Constraints
    /// are carried as indices so component solving borrows nothing but the
    /// normalized request.
    fn component_constraints(
        &self,
        components: &[Vec<usize>],
    ) -> Result<Vec<Vec<usize>>, GeoCompositionError> {
        let mut component_of = vec![usize::MAX; self.total_variables];
        for (component_id, members) in components.iter().enumerate() {
            for variable in members {
                component_of[*variable] = component_id;
            }
        }
        let mut per_component: Vec<Vec<usize>> = vec![Vec::new(); components.len()];
        for (constraint_index, constraint) in self.request.hard_constraints.iter().enumerate() {
            let members = self.constraint_members(constraint);
            let Some(&first) = members.first() else {
                return Err(GeoCompositionError::invalid_input(
                    "Geo composition constraint references no universe member",
                    [("constraint_id", constraint.id.as_str())],
                ));
            };
            let component_id = component_of[first];
            if members
                .iter()
                .any(|member| component_of[*member] != component_id)
            {
                return Err(GeoCompositionError::invalid_input(
                    "Geo composition constraint spans decomposition components",
                    [("constraint_id", constraint.id.as_str())],
                ));
            }
            per_component[component_id].push(constraint_index);
        }
        Ok(per_component)
    }

    fn solve(self) -> Result<GeoCompositionArtifact, GeoCompositionError> {
        // Exact fast path for the pure-existential shape that admitted
        // evidence channels produce (AnyOf-only, parcel universes): closed
        // form inclusion-exclusion instead of exponential decomposition.
        if let Some(artifact) = self.solve_anyof_only()? {
            return Ok(artifact);
        }
        let components = self.components();
        let component_constraints = self.component_constraints(&components)?;
        let mut solved: Vec<ComponentOutcome> = Vec::with_capacity(components.len());
        for (component_id, members) in components.iter().enumerate() {
            solved.push(self.solve_component(members, &component_constraints[component_id])?);
        }
        self.combine(components, solved, component_constraints)
    }

    /// Precondition guard for the AnyOf-only fast path.
    fn is_anyof_only(&self) -> bool {
        self.request.buildings.is_empty()
            && !self.request.hard_constraints.is_empty()
            && self.request.hard_constraints.iter().all(|constraint| {
                matches!(constraint.constraint, GeoHardConstraintKind::AnyOf { .. })
            })
    }

    /// Exact model count and backbone for AnyOf-only parcel universes via
    /// singleton forcing plus inclusion-exclusion over the remaining
    /// "set i is missed" events:
    ///
    ///   count = sum over T of (-1)^|T| * 2^(n - |union of sets in T|)
    ///
    /// In a monotone positive CNF, a parcel is forced iff it appears in a
    /// singleton clause: if no singleton names it, the assignment selecting
    /// every other parcel is a witness without it. That observation keeps
    /// backbone computation exact at arbitrary universe widths. Counts beyond
    /// the u64 reporting range saturate with a count-specific flag. Soft
    /// preferences never constrain; this path does not materialize models.
    fn solve_anyof_only(&self) -> Result<Option<GeoCompositionArtifact>, GeoCompositionError> {
        if !self.is_anyof_only() {
            return Ok(None);
        }
        let n = self.request.parcels.len();

        let member_sets: Vec<Vec<&str>> = self
            .request
            .hard_constraints
            .iter()
            .map(|constraint| match &constraint.constraint {
                GeoHardConstraintKind::AnyOf { members } => members
                    .iter()
                    .map(|member| member.id.as_str())
                    .collect::<Vec<_>>(),
                _ => unreachable!("guarded by is_anyof_only"),
            })
            .collect();
        let forced = member_sets
            .iter()
            .filter_map(|members| (members.len() == 1).then_some(members[0]))
            .collect::<BTreeSet<_>>();
        let remaining_sets = member_sets
            .iter()
            .filter(|members| !members.iter().any(|id| forced.contains(id)))
            .collect::<Vec<_>>();
        let active = remaining_sets
            .iter()
            .flat_map(|members| members.iter().copied())
            .collect::<BTreeSet<_>>();
        let free_count = n
            .checked_sub(forced.len() + active.len())
            .ok_or_else(|| GeoCompositionError::overflow("anyof variable partition"))?;

        // Every free variable doubles a non-empty active solution. Once there
        // are 64 free variables, the result is unambiguously beyond the u64
        // reporting range and no alternating arithmetic is needed.
        let mut evaluations = 0_u128;
        let residual_count = if remaining_sets.is_empty() {
            ReportedCount::pow2(free_count)
        } else if free_count >= u64::BITS as usize {
            ReportedCount::SATURATED
        } else {
            // Signed inclusion-exclusion is bounded to 126 active variables;
            // larger coupled cores take the general arbitrary-width solver.
            if active.len() >= 127 {
                return Ok(None);
            }
            let k = remaining_sets.len();
            let mask_visits = match u32::try_from(k) {
                Ok(shift) if shift < 127 => 1_u128 << shift,
                _ => u128::MAX,
            };
            let declared_limit = u128::from(self.request.max_assignments);
            if mask_visits > ANYOF_FASTPATH_MAX_MASK_VISITS || mask_visits > declared_limit {
                return Ok(None);
            }
            let mut hit_count = 1_i128 << active.len();
            for mask in 1_u128..(1_u128 << k) {
                let mut seen: BTreeSet<&str> = BTreeSet::new();
                for (position, set) in remaining_sets.iter().enumerate() {
                    if mask & (1_u128 << position) == 0 {
                        continue;
                    }
                    for id in set.iter() {
                        evaluations += 1;
                        seen.insert(id);
                    }
                }
                let free = active.len() - seen.len();
                let term = 1_i128 << free;
                let updated = if mask.count_ones() % 2 == 1 {
                    hit_count.checked_sub(term)
                } else {
                    hit_count.checked_add(term)
                };
                let Some(updated) = updated else {
                    // The general solver is slower but has no signed
                    // inclusion-exclusion accumulator to overflow.
                    return Ok(None);
                };
                hit_count = updated;
            }
            if hit_count < 0 {
                return Ok(None);
            }
            ReportedCount::from_u128(hit_count as u128).mul(ReportedCount::pow2(free_count))
        };

        let candidate = ReportedCount::pow2(n);
        let structural = ReportedCount::nonempty_subsets(n);
        let evaluation_count = ReportedCount::from_u128(evaluations);
        let aggregate_saturated = candidate.saturated
            || structural.saturated
            || evaluation_count.saturated
            || residual_count.saturated;
        let components = self.components();
        let component_constraints = self.component_constraints(&components)?;
        let factorization = components
            .iter()
            .enumerate()
            .map(|(component_id, members)| GeoCompositionComponentSummary {
                key: self.component_key(members),
                variables: members
                    .iter()
                    .map(|variable| {
                        GeoEntityRef::new(self.entity_level(*variable), self.var_id(*variable))
                    })
                    .collect(),
                constraint_ids: component_constraints[component_id]
                    .iter()
                    .map(|index| self.request.hard_constraints[*index].id.clone())
                    .collect(),
                strategy: GeoCompositionSearchStrategy::AnyOfInclusionExclusion,
                exact: true,
                search_visits: 0,
                hard_feasible_assignments: None,
                hard_feasible_assignments_saturated: false,
                positive_assignments: None,
                positive_assignments_saturated: false,
            })
            .collect::<Vec<_>>();

        Ok(Some(GeoCompositionArtifact {
            version: CANON_GEO_COMPOSITION_VERSION.to_string(),
            request_version: self.request.request_version.clone(),
            evidence_compilation: None,
            status: if residual_count.is_exactly(1) {
                GeoCompositionStatus::Resolved
            } else {
                GeoCompositionStatus::Ambiguous
            },
            summary: GeoCompositionSummary {
                parcel_candidates: self.request.parcels.len(),
                building_candidates: self.request.buildings.len(),
                candidate_assignments: candidate.value,
                candidate_assignments_saturated: candidate.saturated,
                structurally_feasible_assignments: structural.value,
                structurally_feasible_assignments_complete: true,
                structurally_feasible_assignments_saturated: structural.saturated,
                hard_constraint_evaluations: evaluation_count.value,
                hard_constraint_evaluations_complete: true,
                hard_constraint_evaluations_saturated: evaluation_count.saturated,
                residual_model_count: residual_count.value,
                model_count_scope: GeoModelCountScope::EntitySelection,
                residual_model_count_complete: true,
                residual_model_count_saturated: residual_count.saturated,
                summary_counts_saturated: aggregate_saturated,
                component_count: factorization.len(),
                residual_models_materialized: false,
            },
            hard_forced: GeoCompositionBackbone {
                parcels: forced.into_iter().map(String::from).collect(),
                buildings: Vec::new(),
            },
            backbone_complete: true,
            factorization,
            residual_models: Vec::new(),
            soft_ranked: Vec::new(),
            conflict_constraint_ids: Vec::new(),
            conflict_core_complete: None,
            budget_fallback: None,
        }))
    }

    fn solve_component(
        &self,
        members: &[usize],
        constraints: &[usize],
    ) -> Result<ComponentOutcome, GeoCompositionError> {
        let Some(space) = component_space(members.len(), self.request.max_assignments) else {
            return self.solve_component_dfs(members, constraints);
        };
        let ctx = ComponentContext::new(self, members)?;
        let mut solution = ComponentSolution::new(ctx.width(), true);
        for mask in 0..space {
            if !ctx.structurally_valid(mask) {
                continue;
            }
            solution.structural_count += 1;
            if ctx.mask_has_parcel(mask) {
                solution.structural_positive += 1;
            } else {
                solution.structural_empty += 1;
            }
            let model = ctx.model_from_mask(mask);
            let mut feasible = true;
            for constraint_index in constraints {
                solution.evaluations += 1;
                let constraint = &self.request.hard_constraints[*constraint_index];
                if !constraint_holds(&model, &constraint.constraint) {
                    feasible = false;
                    break;
                }
            }
            if feasible {
                solution.record(mask, &ctx);
            }
        }
        Ok(ComponentOutcome::Exact {
            solution: Box::new(solution),
            strategy: GeoCompositionSearchStrategy::ExhaustiveEnumeration,
            search_visits: u64::try_from(space).unwrap_or(u64::MAX),
        })
    }

    /// Deterministic depth-first search for components whose assignment
    /// space exceeds the declared budget. Variables are assigned in canonical
    /// ascending order, false before true; partial-feasibility pruning skips
    /// infeasible subtrees; a visit budget bounds the work. Completing the
    /// search yields exact counts and backbone flags without storing models.
    fn solve_component_dfs(
        &self,
        members: &[usize],
        constraints: &[usize],
    ) -> Result<ComponentOutcome, GeoCompositionError> {
        let ctx = ComponentContext::new(self, members)?;
        let width = ctx.width();
        let mut search = DfsSearch {
            ctx,
            constraints,
            budget: self.request.max_assignments,
            visits: 0,
            values: vec![false; width],
            exhausted: false,
            solution: ComponentSolution::new(width, false),
        };
        search.run(0);
        if search.exhausted {
            return Ok(ComponentOutcome::Fallback {
                variable_count: members.len(),
                search_visits: search.visits,
            });
        }
        Ok(ComponentOutcome::Exact {
            solution: Box::new(search.solution),
            strategy: GeoCompositionSearchStrategy::PrunedDepthFirst,
            search_visits: search.visits,
        })
    }

    fn component_key(&self, members: &[usize]) -> String {
        let first = members[0];
        format!(
            "{}:{}",
            match self.var_level(first) {
                VarLevel::Parcel => "parcel",
                VarLevel::Building => "building",
            },
            self.var_id(first)
        )
    }

    fn factorization(
        &self,
        components: &[Vec<usize>],
        component_constraints: &[Vec<usize>],
        outcomes: &[ComponentOutcome],
    ) -> Vec<GeoCompositionComponentSummary> {
        components
            .iter()
            .enumerate()
            .map(|(component_id, members)| {
                let variables = members
                    .iter()
                    .map(|variable| {
                        GeoEntityRef::new(self.entity_level(*variable), self.var_id(*variable))
                    })
                    .collect();
                let constraint_ids = component_constraints[component_id]
                    .iter()
                    .map(|index| self.request.hard_constraints[*index].id.clone())
                    .collect();
                let (
                    strategy,
                    exact,
                    search_visits,
                    feasible,
                    feasible_saturated,
                    positive,
                    positive_saturated,
                ) = match &outcomes[component_id] {
                    ComponentOutcome::Exact {
                        solution,
                        strategy,
                        search_visits,
                    } => {
                        let feasible = ReportedCount::from_u128(solution.count);
                        let positive = ReportedCount::from_u128(solution.positive_count);
                        (
                            *strategy,
                            true,
                            *search_visits,
                            Some(feasible.value),
                            feasible.saturated,
                            Some(positive.value),
                            positive.saturated,
                        )
                    }
                    ComponentOutcome::Fallback { search_visits, .. } => (
                        GeoCompositionSearchStrategy::BudgetFallback,
                        false,
                        *search_visits,
                        None,
                        false,
                        None,
                        false,
                    ),
                };
                GeoCompositionComponentSummary {
                    key: self.component_key(members),
                    variables,
                    constraint_ids,
                    strategy,
                    exact,
                    search_visits,
                    hard_feasible_assignments: feasible,
                    hard_feasible_assignments_saturated: feasible_saturated,
                    positive_assignments: positive,
                    positive_assignments_saturated: positive_saturated,
                }
            })
            .collect()
    }

    fn entity_level(&self, variable: usize) -> GeoEntityLevel {
        match self.var_level(variable) {
            VarLevel::Parcel => GeoEntityLevel::Parcel,
            VarLevel::Building => GeoEntityLevel::Building,
        }
    }

    fn build_fallback(
        &self,
        components: &[Vec<usize>],
        component_constraints: &[Vec<usize>],
        outcomes: &[ComponentOutcome],
    ) -> Result<Option<GeoCompositionArtifact>, GeoCompositionError> {
        let fallbacks = outcomes
            .iter()
            .enumerate()
            .filter_map(|(index, outcome)| match outcome {
                ComponentOutcome::Fallback { variable_count, .. } => Some((index, *variable_count)),
                ComponentOutcome::Exact { .. } => None,
            })
            .collect::<Vec<_>>();
        if fallbacks.is_empty() {
            return Ok(None);
        }
        let component_keys = fallbacks
            .iter()
            .map(|(index, _)| self.component_key(&components[*index]))
            .collect();
        let max_component_variables = fallbacks
            .iter()
            .map(|(_, variable_count)| *variable_count)
            .max()
            .unwrap_or_default();
        Ok(Some(GeoCompositionArtifact {
            version: CANON_GEO_COMPOSITION_VERSION.to_string(),
            request_version: self.request.request_version.clone(),
            evidence_compilation: None,
            status: GeoCompositionStatus::BudgetFallback,
            summary: self.summary(components, 0, false, 0)?,
            hard_forced: GeoCompositionBackbone {
                parcels: Vec::new(),
                buildings: Vec::new(),
            },
            backbone_complete: false,
            factorization: self.factorization(components, component_constraints, outcomes),
            residual_models: Vec::new(),
            soft_ranked: Vec::new(),
            conflict_constraint_ids: Vec::new(),
            conflict_core_complete: None,
            budget_fallback: Some(GeoCompositionFallback {
                component_keys,
                max_component_variables,
                configured_max_assignments: self.request.max_assignments,
                guidance: FALLBACK_GUIDANCE.to_string(),
            }),
        }))
    }

    fn summary(
        &self,
        components: &[Vec<usize>],
        residual_model_count: u64,
        residual_models_materialized: bool,
        hard_constraint_evaluations: u64,
    ) -> Result<GeoCompositionSummary, GeoCompositionError> {
        let candidate = ReportedCount::pow2(self.total_variables);
        Ok(GeoCompositionSummary {
            parcel_candidates: self.request.parcels.len(),
            building_candidates: self.request.buildings.len(),
            candidate_assignments: candidate.value,
            candidate_assignments_saturated: candidate.saturated,
            structurally_feasible_assignments: 0,
            structurally_feasible_assignments_complete: false,
            structurally_feasible_assignments_saturated: false,
            hard_constraint_evaluations,
            hard_constraint_evaluations_complete: false,
            hard_constraint_evaluations_saturated: false,
            residual_model_count,
            model_count_scope: GeoModelCountScope::EntitySelection,
            residual_model_count_complete: false,
            residual_model_count_saturated: false,
            summary_counts_saturated: candidate.saturated,
            component_count: components.len(),
            residual_models_materialized,
        })
    }

    fn combine(
        self,
        components: Vec<Vec<usize>>,
        outcomes: Vec<ComponentOutcome>,
        component_constraints: Vec<Vec<usize>>,
    ) -> Result<GeoCompositionArtifact, GeoCompositionError> {
        if let Some(fallback) =
            self.build_fallback(&components, &component_constraints, &outcomes)?
        {
            return Ok(fallback);
        }
        let factorization = self.factorization(&components, &component_constraints, &outcomes);
        let solutions: Vec<ComponentSolution> = outcomes
            .into_iter()
            .map(|outcome| match outcome {
                ComponentOutcome::Exact { solution, .. } => *solution,
                ComponentOutcome::Fallback { .. } => {
                    unreachable!("fallback artifacts are handled before combining")
                }
            })
            .collect();

        // Track positive and empty products separately with an additive
        // recurrence. This never subtracts two saturated sentinels, which
        // previously turned very large satisfiable products into false zeroes.
        let mut residual = ReportedCount::ZERO;
        let mut feasible_empty = ReportedCount::ONE;
        let mut structural_positive = ReportedCount::ZERO;
        let mut structural_empty = ReportedCount::ONE;
        let mut evaluations = ReportedCount::ZERO;
        let mut capable_components = 0_usize;
        for solution in &solutions {
            let component_total = ReportedCount::from_u128(solution.count);
            let component_positive = ReportedCount::from_u128(solution.positive_count);
            let component_empty = ReportedCount::from_u128(solution.empty_count);
            residual = residual
                .mul(component_total)
                .add(feasible_empty.mul(component_positive));
            feasible_empty = feasible_empty.mul(component_empty);

            let component_structural = ReportedCount::from_u128(solution.structural_count);
            let component_structural_positive =
                ReportedCount::from_u128(solution.structural_positive);
            let component_structural_empty = ReportedCount::from_u128(solution.structural_empty);
            structural_positive = structural_positive
                .mul(component_structural)
                .add(structural_empty.mul(component_structural_positive));
            structural_empty = structural_empty.mul(component_structural_empty);
            evaluations = evaluations.add(ReportedCount::from_u128(solution.evaluations));
            if solution.positive_count > 0 {
                capable_components += 1;
            }
        }

        if residual.is_exactly(0) {
            return self.conflict_artifact(
                &components,
                &solutions,
                &component_constraints,
                structural_positive,
                evaluations,
                factorization,
            );
        }

        let mut backbone_parcels = Vec::new();
        let mut backbone_buildings = Vec::new();
        for (component_id, members) in components.iter().enumerate() {
            let solution = &solutions[component_id];
            let others_capable = capable_components - usize::from(solution.positive_count > 0) > 0;
            let (selected_seen, absent_seen) = if !others_capable && solution.positive_count > 0 {
                (
                    &solution.positive_seen_selected,
                    &solution.positive_seen_absent,
                )
            } else {
                (&solution.seen_selected, &solution.seen_absent)
            };
            for (slot, variable) in members.iter().enumerate() {
                if selected_seen[slot] && !absent_seen[slot] {
                    match self.var_level(*variable) {
                        VarLevel::Parcel => backbone_parcels.push(self.var_id(*variable)),
                        VarLevel::Building => backbone_buildings.push(self.var_id(*variable)),
                    }
                }
            }
        }
        // Slot order is ascending global index, so ids land sorted per level.
        let hard_forced = GeoCompositionBackbone {
            parcels: backbone_parcels.into_iter().map(String::from).collect(),
            buildings: backbone_buildings.into_iter().map(String::from).collect(),
        };

        let materialized_capacity = usize::try_from(residual.value).ok();
        let can_materialize = !residual.saturated
            && residual.value <= self.request.max_materialized_models
            && solutions.iter().all(|solution| solution.masks.is_some())
            && materialized_capacity.is_some();
        let residual_models = if can_materialize {
            let mut models = self.materialize(
                &components,
                &solutions,
                materialized_capacity.expect("guarded by can_materialize"),
            )?;
            models.sort();
            models
        } else {
            Vec::new()
        };
        let soft_ranked = if can_materialize {
            rank_residual(&residual_models, &self.request.soft_preferences)?
        } else {
            Vec::new()
        };

        let base_summary = self.summary(&components, 0, can_materialize, 0)?;
        Ok(GeoCompositionArtifact {
            version: CANON_GEO_COMPOSITION_VERSION.to_string(),
            request_version: self.request.request_version.clone(),
            evidence_compilation: None,
            status: if residual.is_exactly(1) {
                GeoCompositionStatus::Resolved
            } else {
                GeoCompositionStatus::Ambiguous
            },
            summary: GeoCompositionSummary {
                structurally_feasible_assignments: structural_positive.value,
                structurally_feasible_assignments_complete: true,
                structurally_feasible_assignments_saturated: structural_positive.saturated,
                hard_constraint_evaluations: evaluations.value,
                hard_constraint_evaluations_complete: true,
                hard_constraint_evaluations_saturated: evaluations.saturated,
                residual_model_count: residual.value,
                residual_model_count_complete: true,
                residual_model_count_saturated: residual.saturated,
                summary_counts_saturated: base_summary.candidate_assignments_saturated
                    || structural_positive.saturated
                    || evaluations.saturated
                    || residual.saturated,
                ..base_summary
            },
            hard_forced,
            backbone_complete: true,
            factorization,
            residual_models,
            soft_ranked,
            conflict_constraint_ids: Vec::new(),
            conflict_core_complete: None,
            budget_fallback: None,
        })
    }

    /// Enumerate the combined residual by odometer over retained component
    /// masks. Called only when every component retained its solutions and the
    /// exact total fits `max_materialized_models`.
    fn materialize(
        &self,
        components: &[Vec<usize>],
        solutions: &[ComponentSolution],
        residual_total: usize,
    ) -> Result<Vec<GeoCompositionModel>, GeoCompositionError> {
        let contexts = components
            .iter()
            .map(|members| ComponentContext::new(self, members))
            .collect::<Result<Vec<_>, _>>()?;
        let mut cursor = vec![0_usize; components.len()];
        let mut models = Vec::with_capacity(residual_total);
        loop {
            let mut any_parcel = false;
            let mut parcels = Vec::new();
            let mut buildings = Vec::new();
            for (component_id, context) in contexts.iter().enumerate() {
                let masks = solutions[component_id]
                    .masks
                    .as_deref()
                    .expect("materialize requires retained masks");
                let mask = masks[cursor[component_id]];
                if context.mask_has_parcel(mask) {
                    any_parcel = true;
                }
                context.append_selection(mask, &mut parcels, &mut buildings);
            }
            if any_parcel {
                parcels.sort();
                buildings.sort();
                models.push(GeoCompositionModel { parcels, buildings });
            }
            let mut advanced = false;
            for index in (0..cursor.len()).rev() {
                cursor[index] += 1;
                if cursor[index] < solutions[index].masks.as_ref().expect("retained").len() {
                    advanced = true;
                    break;
                }
                cursor[index] = 0;
            }
            if !advanced {
                break;
            }
        }
        Ok(models)
    }

    /// Explain an empty combined residual with per-component minimal cores:
    /// whole-component infeasibility, or incapability of selecting any parcel.
    fn conflict_artifact(
        &self,
        components: &[Vec<usize>],
        solutions: &[ComponentSolution],
        component_constraints: &[Vec<usize>],
        structural_positive: ReportedCount,
        evaluation_count: ReportedCount,
        factorization: Vec<GeoCompositionComponentSummary>,
    ) -> Result<GeoCompositionArtifact, GeoCompositionError> {
        let mut conflict_ids = BTreeSet::new();
        let mut conflict_core_complete = true;
        for (component_id, solution) in solutions.iter().enumerate() {
            if solution.count == 0 || solution.positive_count == 0 {
                let (ids, complete) = self.component_conflict_core(
                    &components[component_id],
                    &component_constraints[component_id],
                    solution.count == 0,
                )?;
                conflict_ids.extend(ids);
                conflict_core_complete &= complete;
            }
        }
        let base_summary = self.summary(components, 0, false, 0)?;
        Ok(GeoCompositionArtifact {
            version: CANON_GEO_COMPOSITION_VERSION.to_string(),
            request_version: self.request.request_version.clone(),
            evidence_compilation: None,
            status: GeoCompositionStatus::Conflict,
            summary: GeoCompositionSummary {
                structurally_feasible_assignments: structural_positive.value,
                structurally_feasible_assignments_complete: true,
                structurally_feasible_assignments_saturated: structural_positive.saturated,
                hard_constraint_evaluations: evaluation_count.value,
                hard_constraint_evaluations_complete: true,
                hard_constraint_evaluations_saturated: evaluation_count.saturated,
                residual_model_count_complete: true,
                summary_counts_saturated: base_summary.candidate_assignments_saturated
                    || structural_positive.saturated
                    || evaluation_count.saturated,
                ..base_summary
            },
            hard_forced: GeoCompositionBackbone {
                parcels: Vec::new(),
                buildings: Vec::new(),
            },
            backbone_complete: false,
            factorization,
            residual_models: Vec::new(),
            soft_ranked: Vec::new(),
            conflict_constraint_ids: conflict_ids.into_iter().collect(),
            conflict_core_complete: Some(conflict_core_complete),
            budget_fallback: None,
        })
    }

    /// QuickXplain-style linear core reduction over one component's
    /// enumerable structural space. With `whole_infeasible` the subproblem is
    /// the plain component; otherwise it is the positivity subproblem that
    /// asks whether the component can select at least one parcel.
    fn component_conflict_core(
        &self,
        members: &[usize],
        constraints: &[usize],
        whole_infeasible: bool,
    ) -> Result<(Vec<String>, bool), GeoCompositionError> {
        let Some(space) = component_space(members.len(), self.request.max_assignments) else {
            let mut ids = constraints
                .iter()
                .map(|index| self.request.hard_constraints[*index].id.clone())
                .collect::<Vec<_>>();
            ids.sort();
            return Ok((ids, false));
        };
        let ctx = ComponentContext::new(self, members)?;
        let require_positive = !whole_infeasible;
        let mut structural = Vec::new();
        for mask in 0..space {
            if !ctx.structurally_valid(mask) {
                continue;
            }
            if require_positive && !ctx.mask_has_parcel(mask) {
                continue;
            }
            structural.push(ctx.model_from_mask(mask));
        }
        // Keep global constraint indices throughout. Using `0..len` here
        // accidentally reindexed a component-local slice into the global
        // request and could blame constraints from an unrelated component.
        let mut core: Vec<usize> = constraints.to_vec();
        let mut index = 0;
        while index < core.len() {
            let candidate: Vec<usize> = core
                .iter()
                .copied()
                .enumerate()
                .filter(|(position, _)| *position != index)
                .map(|(_, constraint_index)| constraint_index)
                .collect();
            let feasible = structural.iter().any(|model| {
                candidate.iter().all(|constraint_index| {
                    constraint_holds(
                        model,
                        &self.request.hard_constraints[*constraint_index].constraint,
                    )
                }) && (!require_positive || !model.parcels.is_empty())
            });
            if !feasible {
                core = candidate;
            } else {
                index += 1;
            }
        }
        let mut ids: Vec<String> = core
            .into_iter()
            .map(|constraint_index| self.request.hard_constraints[constraint_index].id.clone())
            .collect();
        ids.sort();
        Ok((ids, true))
    }
}

/// Deterministic union-find for incidence component discovery. Roots always
/// collapse toward the smaller canonical variable index, so grouping is
/// stable before the explicit component sort as well.
struct DisjointSet {
    parent: Vec<usize>,
}

impl DisjointSet {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
        }
    }

    fn find(&mut self, mut node: usize) -> usize {
        let mut root = node;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        while self.parent[node] != node {
            let next = self.parent[node];
            self.parent[node] = root;
            node = next;
        }
        root
    }

    fn union(&mut self, left: usize, right: usize) {
        let left_root = self.find(left);
        let right_root = self.find(right);
        if left_root == right_root {
            return;
        }
        let (root, child) = if left_root < right_root {
            (left_root, right_root)
        } else {
            (right_root, left_root)
        };
        self.parent[child] = root;
    }
}

/// Non-negative count in the public u64 reporting domain. Saturation means
/// the mathematical value is strictly greater than `u64::MAX`; it is never
/// used as an arithmetic value. In particular, multiplication by exact zero
/// remains exact zero instead of inheriting an unrelated saturation flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReportedCount {
    value: u64,
    saturated: bool,
}

impl ReportedCount {
    const ZERO: Self = Self {
        value: 0,
        saturated: false,
    };
    const ONE: Self = Self {
        value: 1,
        saturated: false,
    };
    const SATURATED: Self = Self {
        value: u64::MAX,
        saturated: true,
    };

    fn from_u128(value: u128) -> Self {
        u64::try_from(value).map_or(Self::SATURATED, |value| Self {
            value,
            saturated: false,
        })
    }

    fn pow2(exponent: usize) -> Self {
        if exponent >= u64::BITS as usize {
            Self::SATURATED
        } else {
            Self {
                value: 1_u64 << exponent,
                saturated: false,
            }
        }
    }

    fn nonempty_subsets(width: usize) -> Self {
        if width > u64::BITS as usize {
            Self::SATURATED
        } else if width == u64::BITS as usize {
            Self {
                value: u64::MAX,
                saturated: false,
            }
        } else {
            Self {
                value: (1_u64 << width) - 1,
                saturated: false,
            }
        }
    }

    fn add(self, other: Self) -> Self {
        if self.saturated || other.saturated {
            return Self::SATURATED;
        }
        self.value
            .checked_add(other.value)
            .map_or(Self::SATURATED, |value| Self {
                value,
                saturated: false,
            })
    }

    fn mul(self, other: Self) -> Self {
        if self.is_exactly(0) || other.is_exactly(0) {
            return Self::ZERO;
        }
        if self.saturated || other.saturated {
            return Self::SATURATED;
        }
        self.value
            .checked_mul(other.value)
            .map_or(Self::SATURATED, |value| Self {
                value,
                saturated: false,
            })
    }

    fn is_exactly(self, value: u64) -> bool {
        !self.saturated && self.value == value
    }
}

const FALLBACK_GUIDANCE: &str = "raise max_assignments, narrow the candidate block, or add evidence constraints that decompose the component; no residual was guessed";

/// Per-component assignment space: `2^width` when it fits the declared
/// budget, else `None` (the component takes the bounded-search path).
fn component_space(width: usize, max_assignments: u64) -> Option<u128> {
    if width >= 128 {
        return None;
    }
    let space = 1_u128 << width;
    let fits = u64::try_from(space)
        .map(|bounded| bounded <= max_assignments)
        .unwrap_or(false);
    fits.then_some(space)
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

#[derive(Debug, Clone)]
enum ComponentOutcome {
    Exact {
        solution: Box<ComponentSolution>,
        strategy: GeoCompositionSearchStrategy,
        search_visits: u64,
    },
    Fallback {
        variable_count: usize,
        search_visits: u64,
    },
}

/// Streaming per-component solution statistics. `masks` retains individual
/// feasible assignments only when the component was enumerated with
/// retention; the bounded search path reports counts and backbone flags
/// without storing models.
#[derive(Debug, Clone, Default)]
struct ComponentSolution {
    count: u128,
    positive_count: u128,
    empty_count: u128,
    structural_count: u128,
    structural_positive: u128,
    structural_empty: u128,
    evaluations: u128,
    seen_selected: Vec<bool>,
    seen_absent: Vec<bool>,
    positive_seen_selected: Vec<bool>,
    positive_seen_absent: Vec<bool>,
    masks: Option<Vec<u128>>,
}

impl ComponentSolution {
    fn new(width: usize, retain_masks: bool) -> Self {
        Self {
            seen_selected: vec![false; width],
            seen_absent: vec![false; width],
            positive_seen_selected: vec![false; width],
            positive_seen_absent: vec![false; width],
            masks: retain_masks.then(Vec::new),
            ..Self::default()
        }
    }

    fn record(&mut self, mask: u128, context: &ComponentContext<'_>) {
        let selection = (0..context.width())
            .map(|slot| mask & (1_u128 << slot) != 0)
            .collect::<Vec<_>>();
        self.record_selection(&selection, context);
        if let Some(masks) = self.masks.as_mut() {
            masks.push(mask);
        }
    }

    fn record_selection(&mut self, selection: &[bool], context: &ComponentContext<'_>) {
        self.count += 1;
        let has_parcel = context.selection_has_parcel(selection);
        if has_parcel {
            self.positive_count += 1;
        } else {
            self.empty_count += 1;
        }
        for (slot, selected) in selection.iter().copied().enumerate() {
            if selected {
                self.seen_selected[slot] = true;
                if has_parcel {
                    self.positive_seen_selected[slot] = true;
                }
            } else {
                self.seen_absent[slot] = true;
                if has_parcel {
                    self.positive_seen_absent[slot] = true;
                }
            }
        }
    }
}

/// Slot-level view of one component: maps global variables to local bit
/// positions and evaluates structural rules on raw masks.
struct ComponentContext<'a> {
    solver: &'a FactorizedSolver<'a>,
    /// Global variable indices, ascending.
    members: Vec<usize>,
    /// Local slots holding parcel variables.
    parcel_slots: Vec<usize>,
    /// `(local slot, request.buildings offset)` for building variables.
    building_slots: Vec<(usize, usize)>,
    /// Global variable index to local slot; `usize::MAX` outside the
    /// component.
    slot_of_global: Vec<usize>,
}

impl<'a> ComponentContext<'a> {
    fn new(
        solver: &'a FactorizedSolver<'a>,
        members: &[usize],
    ) -> Result<Self, GeoCompositionError> {
        let mut slot_of_global = vec![usize::MAX; solver.total_variables];
        let mut parcel_slots = Vec::new();
        let mut building_slots = Vec::new();
        for (slot, variable) in members.iter().enumerate() {
            slot_of_global[*variable] = slot;
            match solver.var_level(*variable) {
                VarLevel::Parcel => parcel_slots.push(slot),
                VarLevel::Building => {
                    building_slots.push((slot, *variable - solver.request.parcels.len()))
                }
            }
        }
        Ok(Self {
            solver,
            members: members.to_vec(),
            parcel_slots,
            building_slots,
            slot_of_global,
        })
    }

    fn width(&self) -> usize {
        self.members.len()
    }

    fn member_slot(&self, member: &GeoEntityRef) -> Option<usize> {
        let global = self.solver.var_index(member)?;
        let slot = self.slot_of_global[global];
        (slot != usize::MAX).then_some(slot)
    }

    fn mask_has_parcel(&self, mask: u128) -> bool {
        self.parcel_slots
            .iter()
            .any(|slot| mask & (1_u128 << slot) != 0)
    }

    fn selection_has_parcel(&self, selection: &[bool]) -> bool {
        self.parcel_slots.iter().any(|slot| selection[*slot])
    }

    /// Structural containment rule: a selected building with declared
    /// parcels requires at least one of them selected.
    fn structurally_valid(&self, mask: u128) -> bool {
        self.building_slots.iter().all(|(slot, offset)| {
            if mask & (1_u128 << slot) == 0 {
                return true;
            }
            let building = &self.solver.request.buildings[*offset];
            building.parcel_ids.is_empty()
                || building.parcel_ids.iter().any(|parcel_id| {
                    self.solver.parcel_index(parcel_id).is_some_and(|global| {
                        let parcel_slot = self.slot_of_global[global];
                        parcel_slot != usize::MAX && mask & (1_u128 << parcel_slot) != 0
                    })
                })
        })
    }

    fn structurally_valid_selection(&self, selection: &[bool]) -> bool {
        self.building_slots.iter().all(|(slot, offset)| {
            if !selection[*slot] {
                return true;
            }
            let building = &self.solver.request.buildings[*offset];
            building.parcel_ids.is_empty()
                || building.parcel_ids.iter().any(|parcel_id| {
                    self.solver.parcel_index(parcel_id).is_some_and(|global| {
                        let parcel_slot = self.slot_of_global[global];
                        parcel_slot != usize::MAX && selection[parcel_slot]
                    })
                })
        })
    }

    fn model_from_mask(&self, mask: u128) -> GeoCompositionModel {
        let mut parcels = Vec::new();
        let mut buildings = Vec::new();
        self.append_selection(mask, &mut parcels, &mut buildings);
        GeoCompositionModel { parcels, buildings }
    }

    fn model_from_selection(&self, selection: &[bool]) -> GeoCompositionModel {
        let mut parcels = Vec::new();
        let mut buildings = Vec::new();
        for (slot, variable) in self.members.iter().enumerate() {
            if !selection[slot] {
                continue;
            }
            match self.solver.var_level(*variable) {
                VarLevel::Parcel => parcels.push(self.solver.var_id(*variable).to_string()),
                VarLevel::Building => buildings.push(self.solver.var_id(*variable).to_string()),
            }
        }
        GeoCompositionModel { parcels, buildings }
    }

    /// Appends this mask's selected ids in ascending global-index order,
    /// which is ascending id order within each level.
    fn append_selection(&self, mask: u128, parcels: &mut Vec<String>, buildings: &mut Vec<String>) {
        for (slot, variable) in self.members.iter().enumerate() {
            if mask & (1_u128 << slot) == 0 {
                continue;
            }
            match self.solver.var_level(*variable) {
                VarLevel::Parcel => parcels.push(self.solver.var_id(*variable).to_string()),
                VarLevel::Building => buildings.push(self.solver.var_id(*variable).to_string()),
            }
        }
    }
}

/// Deterministic bounded depth-first search over one oversized component.
/// Variables are assigned in canonical ascending order, `false` before
/// `true`; partial-feasibility pruning skips infeasible subtrees; a visit
/// budget bounds the work. Completion yields exact counts and backbone flags
/// without storing models.
struct DfsSearch<'a, 'b> {
    ctx: ComponentContext<'a>,
    constraints: &'b [usize],
    budget: u64,
    visits: u64,
    values: Vec<bool>,
    exhausted: bool,
    solution: ComponentSolution,
}

impl<'a, 'b> DfsSearch<'a, 'b> {
    fn run(&mut self, depth: usize) {
        if self.exhausted {
            return;
        }
        if self.visits == self.budget {
            self.exhausted = true;
            return;
        }
        self.visits += 1;
        if depth == self.ctx.width() {
            self.record_leaf();
            return;
        }
        for value in [false, true] {
            self.values[depth] = value;
            if self.partial_feasible(depth + 1) {
                self.run(depth + 1);
                if self.exhausted {
                    return;
                }
            }
        }
    }

    fn record_leaf(&mut self) {
        if !self.ctx.structurally_valid_selection(&self.values) {
            return;
        }
        self.solution.structural_count += 1;
        let has_parcel = self.ctx.selection_has_parcel(&self.values);
        if has_parcel {
            self.solution.structural_positive += 1;
        } else {
            self.solution.structural_empty += 1;
        }
        let model = self.ctx.model_from_selection(&self.values);
        let mut feasible = true;
        for constraint_index in self.constraints {
            self.solution.evaluations += 1;
            let constraint = &self.ctx.solver.request.hard_constraints[*constraint_index];
            if !constraint_holds(&model, &constraint.constraint) {
                feasible = false;
                break;
            }
        }
        if feasible {
            let ctx = &self.ctx;
            self.solution.record_selection(&self.values, ctx);
        }
    }

    /// Prunes when the assigned prefix (`[0, assigned_up_to)`) already
    /// violates a constraint or makes satisfaction unreachable.
    fn partial_feasible(&self, assigned_up_to: usize) -> bool {
        let assigned = |slot: usize| slot < assigned_up_to;
        let is_set = |slot: usize| self.values[slot];
        for (slot, offset) in &self.ctx.building_slots {
            if !assigned(*slot) || !is_set(*slot) {
                continue;
            }
            let building = &self.ctx.solver.request.buildings[*offset];
            if building.parcel_ids.is_empty() {
                continue;
            }
            let every_parcel_decided_false = building.parcel_ids.iter().all(|parcel_id| {
                self.ctx
                    .solver
                    .parcel_index(parcel_id)
                    .map(|global| {
                        let parcel_slot = self.ctx.slot_of_global[global];
                        parcel_slot == usize::MAX || (assigned(parcel_slot) && !is_set(parcel_slot))
                    })
                    .unwrap_or(true)
            });
            if every_parcel_decided_false {
                return false;
            }
        }
        for constraint_index in self.constraints {
            let constraint = &self.ctx.solver.request.hard_constraints[*constraint_index];
            let holds_prefix = match &constraint.constraint {
                GeoHardConstraintKind::Require { member } => self
                    .ctx
                    .member_slot(member)
                    .map(|slot| !assigned(slot) || is_set(slot))
                    .unwrap_or(true),
                GeoHardConstraintKind::Forbid { member } => self
                    .ctx
                    .member_slot(member)
                    .map(|slot| !assigned(slot) || !is_set(slot))
                    .unwrap_or(true),
                GeoHardConstraintKind::Requires {
                    if_member,
                    then_member,
                } => match (
                    self.ctx.member_slot(if_member),
                    self.ctx.member_slot(then_member),
                ) {
                    (Some(if_slot), Some(then_slot)) => {
                        !(assigned(if_slot)
                            && is_set(if_slot)
                            && assigned(then_slot)
                            && !is_set(then_slot))
                    }
                    _ => true,
                },
                GeoHardConstraintKind::AnyOf { members } => {
                    let slots: Vec<Option<usize>> =
                        members.iter().map(|m| self.ctx.member_slot(m)).collect();
                    let all_assigned = slots.iter().all(|slot| slot.map(assigned).unwrap_or(false));
                    let none_set = slots
                        .iter()
                        .all(|slot| slot.is_some_and(|slot| !is_set(slot)));
                    !(all_assigned && none_set)
                }
                GeoHardConstraintKind::AllOrNone { members } => {
                    let states: Vec<Option<bool>> = members
                        .iter()
                        .map(|member| {
                            self.ctx
                                .member_slot(member)
                                .and_then(|slot| assigned(slot).then(|| is_set(slot)))
                        })
                        .collect();
                    let any_true = states.contains(&Some(true));
                    let any_false = states.contains(&Some(false));
                    !(any_true && any_false)
                }
                GeoHardConstraintKind::IntegerSumBand {
                    level,
                    values,
                    min,
                    max,
                    ..
                } => {
                    // The declared band is u64, but summing multiple u64
                    // observations in u64 would panic in debug and wrap in
                    // optimized builds. u128 saturation is enough for sound
                    // pruning: anything beyond u64::MAX is above every
                    // representable band maximum.
                    let mut partial = 0_u128;
                    let mut remaining_max = 0_u128;
                    for value in values {
                        let member = GeoEntityRef::new(*level, value.id.clone());
                        match self.ctx.member_slot(&member) {
                            Some(slot) if assigned(slot) && is_set(slot) => {
                                partial = partial.saturating_add(u128::from(value.value));
                            }
                            Some(slot) if !assigned(slot) => {
                                remaining_max =
                                    remaining_max.saturating_add(u128::from(value.value));
                            }
                            _ => {}
                        }
                    }
                    partial <= u128::from(*max)
                        && partial.saturating_add(remaining_max) >= u128::from(*min)
                }
                GeoHardConstraintKind::Cardinality { level, min, max } => {
                    let mut selected = 0_usize;
                    let mut unassigned = 0_usize;
                    for (slot, variable) in self.ctx.members.iter().enumerate() {
                        let same_level = match level {
                            GeoEntityLevel::Parcel => {
                                self.ctx.solver.var_level(*variable) == VarLevel::Parcel
                            }
                            _ => self.ctx.solver.var_level(*variable) == VarLevel::Building,
                        };
                        if !same_level {
                            continue;
                        }
                        if assigned(slot) {
                            if is_set(slot) {
                                selected += 1;
                            }
                        } else {
                            unassigned += 1;
                        }
                    }
                    selected <= *max && selected + unassigned >= *min
                }
                GeoHardConstraintKind::AllowedSets { .. } => true,
            };
            if !holds_prefix {
                return false;
            }
        }
        true
    }
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
        max_materialized_models: normalized.max_materialized_models,
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
        max_materialized_models: request.max_materialized_models,
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
            measure,
            values,
            min,
            max,
        } => {
            level_cardinality(*level, parcels, buildings)?;
            validate_identifier(
                "hard_constraints[].integer_sum_band.semantic_id",
                &measure.semantic_id,
            )?;
            validate_identifier("hard_constraints[].integer_sum_band.unit", &measure.unit)?;
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
            // The sum of every candidate value may exceed u64 even when many
            // individual selections satisfy the u64 band. That is not an
            // invalid request; evaluation uses a wider accumulator and simply
            // rejects selections whose sum is above the band.
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
            ..
        } => model.members(*level).is_some_and(|members| {
            let sum = values
                .iter()
                .filter(|value| members.binary_search(&value.id).is_ok())
                .fold(0_u128, |sum, value| {
                    sum.saturating_add(u128::from(value.value))
                });
            (u128::from(*min)..=u128::from(*max)).contains(&sum)
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

fn rank_residual(
    models: &[GeoCompositionModel],
    preferences: &[GeoSoftPreference],
) -> Result<Vec<GeoSoftRankedModel>, GeoCompositionError> {
    let mut ranked = Vec::with_capacity(models.len());
    for model in models {
        let mut cost = 0_u128;
        for preference in preferences {
            if !model.contains(&preference.member) {
                cost = cost
                    .checked_add(u128::from(preference.cost_if_absent))
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
