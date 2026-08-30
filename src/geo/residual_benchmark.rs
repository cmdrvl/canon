#![forbid(unsafe_code)]

//! Reproducible benchmark harness for exact Geo residual representations.
//!
//! This module is deliberately a workbench benchmark. It does not choose a
//! product variable order and it does not change `solve_composition`.

use super::{
    CANON_GEO_COMPOSITION_REQUEST_VERSION, GeoBuildingCandidate, GeoCompositionArtifact,
    GeoCompositionBackbone, GeoCompositionError, GeoCompositionModel, GeoCompositionRequest,
    GeoCompositionStatus, GeoCompositionUniverse, GeoEntityLevel, GeoEntityRef, GeoHardConstraint,
    GeoHardConstraintKind, GeoIntegerMemberValue, GeoModelCountScope,
    canonicalize_composition_request, model_satisfies_request, solve_composition,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    time::Instant,
};

macro_rules! detail {
    ($($key:expr => $value:expr),* $(,)?) => {{
        let mut detail = BTreeMap::new();
        $(detail.insert($key.to_string(), $value.to_string());)*
        detail
    }};
}

pub const CANON_GEO_RESIDUAL_BENCHMARK_VERSION: &str = "canon_geo_residual_benchmark.v0";
pub const CANON_GEO_RESIDUAL_OBDD_VERSION: &str = "canon_geo_residual_obdd.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoResidualBenchmarkInput {
    pub version: String,
    pub benchmark_id: String,
    pub cases: Vec<GeoResidualBenchmarkCase>,
    pub orders: Vec<GeoResidualVariableOrder>,
    pub max_answer_set_models: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoResidualBenchmarkCase {
    pub case_id: String,
    pub source: String,
    pub shape_basis: GeoResidualShapeBasis,
    pub measurement_basis: String,
    pub request: GeoCompositionRequest,
    #[serde(default)]
    pub truth_models: Vec<GeoCompositionModel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub orders: Vec<GeoResidualVariableOrder>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoResidualShapeBasis {
    RetainedWorkedCase,
    MeasuredComponentShapeInstantiation,
    RawObservationStressUpperBound,
    SyntheticOrderSensitivityControl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeoResidualVariableOrder {
    Canonical,
    ReverseCanonical,
    BuildingsFirst,
    IncidenceInterleaved,
    Explicit {
        name: String,
        variables: Vec<GeoEntityRef>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoResidualBenchmarkReport {
    pub version: String,
    pub benchmark_id: String,
    pub input_blake3: String,
    pub sdd_status: GeoResidualSkippedRepresentation,
    pub cases: Vec<GeoResidualCaseReport>,
    pub recommendation: GeoResidualBenchmarkRecommendation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoResidualSkippedRepresentation {
    pub representation: String,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoResidualBenchmarkRecommendation {
    pub decision: String,
    pub rationale: String,
    pub limits: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoResidualCaseReport {
    pub case_id: String,
    pub source: String,
    pub shape_basis: GeoResidualShapeBasis,
    pub measurement_basis: String,
    pub request_blake3: String,
    pub variable_count: usize,
    pub search: GeoResidualSearchReport,
    pub orders: Vec<GeoResidualOrderReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoResidualSearchReport {
    pub status: GeoCompositionStatus,
    pub elapsed_ns: u128,
    pub component_count: usize,
    pub component_widths: Vec<usize>,
    pub max_component_width: usize,
    pub exact_for_count_and_backbone: bool,
    pub residual_model_count: String,
    pub residual_model_count_complete: bool,
    pub residual_model_count_saturated: bool,
    pub model_count_scope: GeoModelCountScope,
    pub hard_forced: GeoCompositionBackbone,
    pub backbone_complete: bool,
    pub residual_models_materialized: bool,
    pub materialized_answer_set_size: usize,
    pub search_visits: u64,
    pub hard_constraint_evaluations: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoResidualOrderReport {
    pub order_name: String,
    pub variable_order: Vec<GeoEntityRef>,
    /// Deterministic bytes of the final serialized OBDD artifact. The
    /// serialization always carries both terminal slots 0/1.
    pub deterministic_build_bytes: u64,
    /// Same byte surface as `deterministic_build_bytes`, named for metrics
    /// tables that compare final serialized size with construction arena size.
    pub final_serialized_build_bytes: u64,
    pub build_blake3: String,
    pub build_elapsed_ns: u128,
    pub query_elapsed_ns: u128,
    /// Final serialized node slots, including fixed false/true terminals.
    pub final_serialized_node_count: usize,
    /// Final serialized decision nodes; all are root-reachable after pruning.
    pub final_serialized_nonterminal_node_count: usize,
    /// Nodes actually reachable from the root. This can be smaller than the
    /// serialized count when one fixed terminal is unused.
    pub root_reachable_node_count: usize,
    pub root_reachable_nonterminal_node_count: usize,
    /// Fixed terminal slots included for canonical serialization but not
    /// reached from this root.
    pub fixed_terminal_overhead_node_count: usize,
    pub unique_state_count: usize,
    pub construction_arena_node_count: usize,
    pub construction_arena_nonterminal_node_count: usize,
    pub construction_peak_node_count: usize,
    pub model_count: String,
    pub backbone: GeoCompositionBackbone,
    pub equivalence: GeoResidualEquivalenceReport,
    /// Formula-level comparability for size/order sensitivity: normalized request
    /// digest, exact model count, backbone, and declared truth-membership agree.
    /// This does not claim materialized answer-set equality.
    pub formula_comparable_to_search: bool,
    /// Full comparability, including materialized answer-set equality.
    pub metrics_comparable_to_search: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoResidualEquivalenceReport {
    pub request_digest_matches: bool,
    pub model_count: GeoResidualCountComparison,
    pub answer_sets: GeoResidualAnswerSetComparison,
    pub truth_membership_matches: bool,
    pub truth_membership: Vec<GeoResidualTruthMembership>,
    pub backbone: GeoResidualBackboneComparison,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoResidualCountComparison {
    Matches,
    Differs,
    SearchIncomplete,
    SearchSaturatedLowerBound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GeoResidualAnswerSetComparison {
    Matches { model_count: u64 },
    Differs { search_count: u64, obdd_count: u64 },
    NotMaterialized { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoResidualBackboneComparison {
    Matches,
    Differs,
    SearchIncomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoResidualTruthMembership {
    pub model: GeoCompositionModel,
    pub request_membership: bool,
    pub obdd_membership: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoResidualObddArtifact {
    pub version: String,
    pub request_blake3: String,
    pub order_name: String,
    pub variables: Vec<GeoEntityRef>,
    pub root: u32,
    pub nodes: Vec<GeoResidualObddNode>,
    pub build_blake3: String,
    /// Deterministic bytes for `nodes` with fixed terminal slots serialized.
    pub deterministic_build_bytes: u64,
    /// Root-reachable nodes; may exclude an unused fixed terminal slot.
    pub root_reachable_node_count: usize,
    pub root_reachable_nonterminal_node_count: usize,
    /// Construction arena before root-reachable pruning.
    pub construction_arena_node_count: usize,
    pub construction_arena_nonterminal_node_count: usize,
    pub construction_peak_node_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeoResidualObddNode {
    False,
    True,
    Decision {
        variable: GeoEntityRef,
        low: u32,
        high: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoResidualBenchmarkErrorCode {
    InvalidInput,
    Composition,
    ArithmeticOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoResidualBenchmarkError {
    pub code: GeoResidualBenchmarkErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoResidualBenchmarkError {
    fn new(
        code: GeoResidualBenchmarkErrorCode,
        message: impl Into<String>,
        detail: BTreeMap<String, String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            detail,
        }
    }

    fn invalid_input(message: impl Into<String>, detail: BTreeMap<String, String>) -> Self {
        Self::new(GeoResidualBenchmarkErrorCode::InvalidInput, message, detail)
    }

    fn overflow(context: &str) -> Self {
        Self::new(
            GeoResidualBenchmarkErrorCode::ArithmeticOverflow,
            "Geo residual benchmark arithmetic overflowed",
            detail!("context" => context),
        )
    }

    fn composition(error: GeoCompositionError) -> Self {
        Self::new(
            GeoResidualBenchmarkErrorCode::Composition,
            error.message,
            error.detail,
        )
    }
}

impl fmt::Display for GeoResidualBenchmarkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoResidualBenchmarkError {}

pub fn run_geo_residual_benchmark(
    input: &GeoResidualBenchmarkInput,
) -> Result<GeoResidualBenchmarkReport, GeoResidualBenchmarkError> {
    if input.version != CANON_GEO_RESIDUAL_BENCHMARK_VERSION {
        return Err(GeoResidualBenchmarkError::invalid_input(
            "Unsupported Geo residual benchmark version",
            detail!(
                "actual" => input.version.as_str(),
                "expected" => CANON_GEO_RESIDUAL_BENCHMARK_VERSION,
            ),
        ));
    }
    if input.cases.is_empty() {
        return Err(GeoResidualBenchmarkError::invalid_input(
            "Geo residual benchmark requires at least one case",
            detail!("cases" => input.cases.len()),
        ));
    }
    if input
        .cases
        .iter()
        .any(|case| input.orders.is_empty() && case.orders.is_empty())
    {
        return Err(GeoResidualBenchmarkError::invalid_input(
            "Geo residual benchmark requires global orders or case-scoped orders",
            detail!(
                "cases" => input.cases.len(),
                "orders" => input.orders.len(),
            ),
        ));
    }
    validate_case_ids(&input.cases)?;
    validate_order_names("global", &input.orders)?;
    for case in &input.cases {
        validate_order_names(case.case_id.as_str(), &case.orders)?;
    }

    let input_blake3 = hash_bytes(&serde_json::to_vec(input).map_err(|error| {
        GeoResidualBenchmarkError::invalid_input(
            "Geo residual benchmark input could not serialize",
            detail!("serde" => error),
        )
    })?);
    let cases = input
        .cases
        .iter()
        .map(|case| run_case(case, &input.orders, input.max_answer_set_models))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(GeoResidualBenchmarkReport {
        version: CANON_GEO_RESIDUAL_BENCHMARK_VERSION.to_string(),
        benchmark_id: input.benchmark_id.clone(),
        input_blake3,
        sdd_status: GeoResidualSkippedRepresentation {
            representation: "sdd".to_string(),
            status: "not_run".to_string(),
            reason: "No maintained in-repo SDD implementation is present; this benchmark does not simulate SDD or report SDD-like numbers.".to_string(),
        },
        recommendation: recommendation_for(&cases),
        cases,
    })
}

pub fn compile_geo_residual_obdd(
    request: &GeoCompositionRequest,
    order: &GeoResidualVariableOrder,
) -> Result<GeoResidualObddArtifact, GeoResidualBenchmarkError> {
    let request = canonicalize_composition_request(request)
        .map_err(GeoResidualBenchmarkError::composition)?;
    let variables = materialize_order(&request, order)?;
    let request_blake3 = composition_request_digest(&request)?;
    let order_name = order_name(order);
    let mut builder = ObddBuilder::new(variables.clone())?;
    let root = build_obdd_root(&mut builder, &request)?;
    let construction_arena_node_count = builder.nodes.len();
    let construction_arena_nonterminal_node_count = construction_arena_node_count
        .checked_sub(2)
        .ok_or_else(|| GeoResidualBenchmarkError::overflow("obdd construction arena terminals"))?;
    let construction_peak_node_count = builder.peak_node_count;
    let pruned = prune_obdd(root, &builder.nodes)?;
    let root = pruned.root;
    let root_reachable_node_count = pruned.root_reachable_node_count;
    let root_reachable_nonterminal_node_count = pruned.root_reachable_nonterminal_node_count;
    let nodes = pruned.nodes;
    let build_bytes = obdd_build_bytes(&GeoResidualObddBuildView {
        version: CANON_GEO_RESIDUAL_OBDD_VERSION,
        request_blake3: request_blake3.as_str(),
        order_name: order_name.as_str(),
        variables: variables.as_slice(),
        root,
        nodes: nodes.as_slice(),
    })?;
    let deterministic_build_bytes = u64::try_from(build_bytes.len())
        .map_err(|_| GeoResidualBenchmarkError::overflow("obdd build byte length"))?;
    Ok(GeoResidualObddArtifact {
        version: CANON_GEO_RESIDUAL_OBDD_VERSION.to_string(),
        request_blake3,
        order_name,
        variables,
        root,
        nodes,
        build_blake3: hash_bytes(&build_bytes),
        deterministic_build_bytes,
        root_reachable_node_count,
        root_reachable_nonterminal_node_count,
        construction_arena_node_count,
        construction_arena_nonterminal_node_count,
        construction_peak_node_count,
    })
}

pub fn verify_geo_residual_obdd(
    request: &GeoCompositionRequest,
    obdd: &GeoResidualObddArtifact,
    truth_models: &[GeoCompositionModel],
    max_answer_set_models: u64,
) -> Result<GeoResidualEquivalenceReport, GeoResidualBenchmarkError> {
    let request = canonicalize_composition_request(request)
        .map_err(GeoResidualBenchmarkError::composition)?;
    let request_blake3 = composition_request_digest(&request)?;
    let search = solve_composition(&request).map_err(GeoResidualBenchmarkError::composition)?;
    verify_geo_residual_obdd_against_search(
        &request,
        &search,
        obdd,
        truth_models,
        max_answer_set_models,
        request_blake3,
    )
}

fn verify_geo_residual_obdd_against_search(
    request: &GeoCompositionRequest,
    search: &GeoCompositionArtifact,
    obdd: &GeoResidualObddArtifact,
    truth_models: &[GeoCompositionModel],
    max_answer_set_models: u64,
    request_blake3: String,
) -> Result<GeoResidualEquivalenceReport, GeoResidualBenchmarkError> {
    let request_digest_matches = request_blake3 == obdd.request_blake3;
    let evaluator = ObddEvaluator::new(obdd)?;
    let model_count = evaluator.model_count()?;
    let model_count_comparison = if !search.summary.residual_model_count_complete {
        GeoResidualCountComparison::SearchIncomplete
    } else if search.summary.residual_model_count_saturated {
        GeoResidualCountComparison::SearchSaturatedLowerBound
    } else if model_count == u128::from(search.summary.residual_model_count) {
        GeoResidualCountComparison::Matches
    } else {
        GeoResidualCountComparison::Differs
    };

    let answer_sets = compare_answer_sets(
        &search.residual_models,
        search.summary.residual_models_materialized,
        &evaluator,
        max_answer_set_models,
    )?;
    let backbone = evaluator.backbone()?;
    let backbone_comparison = if !search.backbone_complete {
        GeoResidualBackboneComparison::SearchIncomplete
    } else if backbone == search.hard_forced {
        GeoResidualBackboneComparison::Matches
    } else {
        GeoResidualBackboneComparison::Differs
    };
    let truth_membership = truth_models
        .iter()
        .map(|model| {
            let request_membership = model_satisfies_request(request, model)
                .map_err(GeoResidualBenchmarkError::composition)?;
            let obdd_membership = evaluator.model_membership(model);
            Ok(GeoResidualTruthMembership {
                model: model.clone(),
                request_membership,
                obdd_membership,
            })
        })
        .collect::<Result<Vec<_>, GeoResidualBenchmarkError>>()?;
    let truth_membership_matches = truth_membership
        .iter()
        .all(|row| row.request_membership == row.obdd_membership);

    Ok(GeoResidualEquivalenceReport {
        request_digest_matches,
        model_count: model_count_comparison,
        answer_sets,
        truth_membership_matches,
        truth_membership,
        backbone: backbone_comparison,
    })
}

pub fn geo_residual_measured_star_case(
    case_id: &str,
    measurement_basis: &str,
    component_width: usize,
    max_assignments: u64,
) -> GeoResidualBenchmarkCase {
    geo_residual_star_case(
        case_id,
        "synthetic_shape_instantiation_from_retained_measurement",
        GeoResidualShapeBasis::MeasuredComponentShapeInstantiation,
        measurement_basis,
        component_width,
        max_assignments,
    )
}

pub fn geo_residual_raw_observation_stress_case(
    case_id: &str,
    measurement_basis: &str,
    raw_observation_width: usize,
    max_assignments: u64,
) -> GeoResidualBenchmarkCase {
    geo_residual_star_case(
        case_id,
        "synthetic_raw_observation_stress_upper_bound",
        GeoResidualShapeBasis::RawObservationStressUpperBound,
        measurement_basis,
        raw_observation_width,
        max_assignments,
    )
}

pub fn geo_residual_order_sensitivity_case(pair_count: usize) -> GeoResidualBenchmarkCase {
    let parcels = (0..pair_count)
        .map(|index| format!("eq:p{index:03}"))
        .collect::<Vec<_>>();
    let buildings = (0..pair_count)
        .map(|index| GeoBuildingCandidate {
            id: format!("eq:b{index:03}"),
            parcel_ids: Vec::new(),
        })
        .collect::<Vec<_>>();
    let hard_constraints = (0..pair_count)
        .map(|index| GeoHardConstraint {
            id: format!("pair-eq-{index:03}"),
            constraint: GeoHardConstraintKind::AllOrNone {
                members: vec![
                    GeoEntityRef::new(GeoEntityLevel::Parcel, format!("eq:p{index:03}")),
                    GeoEntityRef::new(GeoEntityLevel::Building, format!("eq:b{index:03}")),
                ],
            },
        })
        .collect();
    GeoResidualBenchmarkCase {
        case_id: format!("adversarial_order_sensitivity_pairs_{pair_count}"),
        source: "synthetic_order_sensitivity_control".to_string(),
        shape_basis: GeoResidualShapeBasis::SyntheticOrderSensitivityControl,
        measurement_basis: "Classic equality-pair OBDD order-sensitivity control; not a measured Geo incidence claim.".to_string(),
        request: GeoCompositionRequest {
            version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
            universe: GeoCompositionUniverse { parcels, buildings },
            hard_constraints,
            soft_preferences: Vec::new(),
            max_assignments: 16,
            max_materialized_models: 0,
        },
        truth_models: vec![GeoCompositionModel {
            parcels: vec!["eq:p000".to_string()],
            buildings: vec!["eq:b000".to_string()],
        }],
        orders: vec![
            geo_residual_explicit_interleaved_pair_order(pair_count),
            geo_residual_explicit_grouped_pair_order(pair_count),
        ],
    }
}

fn geo_residual_star_case(
    case_id: &str,
    source: &str,
    shape_basis: GeoResidualShapeBasis,
    measurement_basis: &str,
    width: usize,
    max_assignments: u64,
) -> GeoResidualBenchmarkCase {
    let parcel = format!("{case_id}:parcel:000");
    let building_count = width
        .checked_sub(1)
        .expect("residual star width must include one parcel");
    let buildings = (0..building_count)
        .map(|index| GeoBuildingCandidate {
            id: format!("{case_id}:building:{index:03}"),
            parcel_ids: vec![parcel.clone()],
        })
        .collect::<Vec<_>>();
    let truth_model = GeoCompositionModel {
        parcels: vec![parcel.clone()],
        buildings: buildings
            .iter()
            .take(2)
            .map(|building| building.id.clone())
            .collect(),
    };
    GeoResidualBenchmarkCase {
        case_id: case_id.to_string(),
        source: source.to_string(),
        shape_basis,
        measurement_basis: measurement_basis.to_string(),
        request: GeoCompositionRequest {
            version: CANON_GEO_COMPOSITION_REQUEST_VERSION.to_string(),
            universe: GeoCompositionUniverse {
                parcels: vec![parcel],
                buildings,
            },
            hard_constraints: Vec::new(),
            soft_preferences: Vec::new(),
            max_assignments,
            max_materialized_models: 0,
        },
        truth_models: vec![truth_model],
        orders: Vec::new(),
    }
}

pub fn geo_residual_explicit_grouped_pair_order(pair_count: usize) -> GeoResidualVariableOrder {
    let variables =
        (0..pair_count)
            .map(|index| GeoEntityRef::new(GeoEntityLevel::Parcel, format!("eq:p{index:03}")))
            .chain((0..pair_count).map(|index| {
                GeoEntityRef::new(GeoEntityLevel::Building, format!("eq:b{index:03}"))
            }))
            .collect();
    GeoResidualVariableOrder::Explicit {
        name: "explicit_grouped_pairs".to_string(),
        variables,
    }
}

pub fn geo_residual_explicit_interleaved_pair_order(pair_count: usize) -> GeoResidualVariableOrder {
    let variables = (0..pair_count)
        .flat_map(|index| {
            [
                GeoEntityRef::new(GeoEntityLevel::Parcel, format!("eq:p{index:03}")),
                GeoEntityRef::new(GeoEntityLevel::Building, format!("eq:b{index:03}")),
            ]
        })
        .collect();
    GeoResidualVariableOrder::Explicit {
        name: "explicit_interleaved_pairs".to_string(),
        variables,
    }
}

fn run_case(
    case: &GeoResidualBenchmarkCase,
    orders: &[GeoResidualVariableOrder],
    max_answer_set_models: u64,
) -> Result<GeoResidualCaseReport, GeoResidualBenchmarkError> {
    let request = canonicalize_composition_request(&case.request)
        .map_err(GeoResidualBenchmarkError::composition)?;
    let request_blake3 = composition_request_digest(&request)?;
    let variable_count = canonical_variables(&request).len();
    let orders = if case.orders.is_empty() {
        orders
    } else {
        case.orders.as_slice()
    };
    if orders.is_empty() {
        return Err(GeoResidualBenchmarkError::invalid_input(
            "Geo residual benchmark case has no variable orders",
            detail!("case_id" => case.case_id.as_str()),
        ));
    }

    let search_start = Instant::now();
    let search_artifact =
        solve_composition(&request).map_err(GeoResidualBenchmarkError::composition)?;
    let search_elapsed_ns = search_start.elapsed().as_nanos();
    let search_visits =
        search_artifact
            .factorization
            .iter()
            .try_fold(0_u64, |sum, component| {
                sum.checked_add(component.search_visits)
                    .ok_or_else(|| GeoResidualBenchmarkError::overflow("search visit count"))
            })?;
    let search = GeoResidualSearchReport {
        status: search_artifact.status,
        elapsed_ns: search_elapsed_ns,
        component_count: search_artifact.summary.component_count,
        component_widths: search_artifact
            .factorization
            .iter()
            .map(|component| component.variables.len())
            .collect(),
        max_component_width: search_artifact
            .factorization
            .iter()
            .map(|component| component.variables.len())
            .max()
            .unwrap_or(0),
        exact_for_count_and_backbone: search_artifact.summary.residual_model_count_complete
            && !search_artifact.summary.residual_model_count_saturated
            && search_artifact.backbone_complete,
        residual_model_count: count_string_from_search(&search_artifact),
        residual_model_count_complete: search_artifact.summary.residual_model_count_complete,
        residual_model_count_saturated: search_artifact.summary.residual_model_count_saturated,
        model_count_scope: search_artifact.summary.model_count_scope,
        hard_forced: search_artifact.hard_forced.clone(),
        backbone_complete: search_artifact.backbone_complete,
        residual_models_materialized: search_artifact.summary.residual_models_materialized,
        materialized_answer_set_size: search_artifact.residual_models.len(),
        search_visits,
        hard_constraint_evaluations: search_artifact.summary.hard_constraint_evaluations,
    };

    let orders = orders
        .iter()
        .map(|order| {
            let build_start = Instant::now();
            let obdd = compile_geo_residual_obdd(&request, order)?;
            let build_elapsed_ns = build_start.elapsed().as_nanos();

            let query_start = Instant::now();
            let evaluator = ObddEvaluator::new(&obdd)?;
            let model_count = evaluator.model_count()?;
            let backbone = evaluator.backbone()?;
            let query_elapsed_ns = query_start.elapsed().as_nanos();
            let equivalence = verify_geo_residual_obdd_against_search(
                &request,
                &search_artifact,
                &obdd,
                &case.truth_models,
                max_answer_set_models,
                request_blake3.clone(),
            )?;
            let formula_comparable_to_search = equivalence.request_digest_matches
                && equivalence.model_count == GeoResidualCountComparison::Matches
                && equivalence.truth_membership_matches
                && equivalence.backbone == GeoResidualBackboneComparison::Matches;
            let metrics_comparable_to_search = formula_comparable_to_search
                && matches!(
                    equivalence.answer_sets,
                    GeoResidualAnswerSetComparison::Matches { .. }
                );
            let fixed_terminal_overhead_node_count = obdd
                .nodes
                .len()
                .checked_sub(obdd.root_reachable_node_count)
                .ok_or_else(|| GeoResidualBenchmarkError::overflow("fixed terminal overhead"))?;
            let final_serialized_nonterminal_node_count =
                obdd.nodes.len().checked_sub(2).ok_or_else(|| {
                    GeoResidualBenchmarkError::overflow("serialized terminal slots")
                })?;

            Ok(GeoResidualOrderReport {
                order_name: obdd.order_name,
                variable_order: obdd.variables,
                deterministic_build_bytes: obdd.deterministic_build_bytes,
                final_serialized_build_bytes: obdd.deterministic_build_bytes,
                build_blake3: obdd.build_blake3,
                build_elapsed_ns,
                query_elapsed_ns,
                final_serialized_node_count: obdd.nodes.len(),
                final_serialized_nonterminal_node_count,
                root_reachable_node_count: obdd.root_reachable_node_count,
                root_reachable_nonterminal_node_count: obdd.root_reachable_nonterminal_node_count,
                fixed_terminal_overhead_node_count,
                unique_state_count: obdd.root_reachable_node_count,
                construction_arena_node_count: obdd.construction_arena_node_count,
                construction_arena_nonterminal_node_count: obdd
                    .construction_arena_nonterminal_node_count,
                construction_peak_node_count: obdd.construction_peak_node_count,
                model_count: model_count.to_string(),
                backbone,
                equivalence,
                formula_comparable_to_search,
                metrics_comparable_to_search,
            })
        })
        .collect::<Result<Vec<_>, GeoResidualBenchmarkError>>()?;

    Ok(GeoResidualCaseReport {
        case_id: case.case_id.clone(),
        source: case.source.clone(),
        shape_basis: case.shape_basis,
        measurement_basis: case.measurement_basis.clone(),
        request_blake3,
        variable_count,
        search,
        orders,
    })
}

fn recommendation_for(cases: &[GeoResidualCaseReport]) -> GeoResidualBenchmarkRecommendation {
    let answer_set_comparable = cases
        .iter()
        .flat_map(|case| case.orders.iter())
        .filter(|order| order.metrics_comparable_to_search)
        .count();
    let formula_comparable = cases
        .iter()
        .flat_map(|case| case.orders.iter())
        .filter(|order| order.formula_comparable_to_search)
        .count();
    let fallbacks = cases
        .iter()
        .filter(|case| case.search.status == GeoCompositionStatus::BudgetFallback)
        .count();
    let raw_stress = cases
        .iter()
        .filter(|case| case.shape_basis == GeoResidualShapeBasis::RawObservationStressUpperBound)
        .count();
    GeoResidualBenchmarkRecommendation {
        decision: "no_product_order_freeze".to_string(),
        rationale: format!(
            "{answer_set_comparable} order/case pairs proved full answer-set comparability, {formula_comparable} proved formula/count/backbone/truth-membership comparability, {fallbacks} cases hit the shipped solver's budget fallback, and {raw_stress} raw-observation stress upper-bound cases were kept separate from latent solver-component claims. This supports keeping OBDD as a benchmark candidate, not freezing an order or vtree in product semantics."
        ),
        limits: vec![
            "Exactness is relative to the serialized composition request, not world geometry.".to_string(),
            "Measured component-shape cases instantiate retained local widths; raw-observation stress cases are upper-bound source-row shapes, not actual latent solver components.".to_string(),
            "Order-sensitivity size comparisons without materialized answer sets are gated only as formula/count/backbone/truth-membership comparisons.".to_string(),
            "SDD was not benchmarked because no maintained accepted implementation is present in the repo.".to_string(),
        ],
    }
}

fn validate_case_ids(cases: &[GeoResidualBenchmarkCase]) -> Result<(), GeoResidualBenchmarkError> {
    let mut seen = BTreeSet::new();
    for case in cases {
        if case.case_id.is_empty() || case.case_id.trim() != case.case_id {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "Geo residual benchmark case id must be non-empty and canonical",
                detail!("case_id" => case.case_id.as_str()),
            ));
        }
        if !seen.insert(case.case_id.as_str()) {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "Geo residual benchmark case ids must be unique",
                detail!("case_id" => case.case_id.as_str()),
            ));
        }
    }
    Ok(())
}

fn validate_order_names(
    scope: &str,
    orders: &[GeoResidualVariableOrder],
) -> Result<(), GeoResidualBenchmarkError> {
    let mut seen = BTreeSet::new();
    for order in orders {
        let name = order_name(order);
        if name.is_empty() || name.trim() != name {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "Geo residual benchmark order name must be non-empty and canonical",
                detail!("scope" => scope, "order_name" => name.as_str()),
            ));
        }
        if !seen.insert(name.clone()) {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "Geo residual benchmark order names must be unique within their scope",
                detail!("scope" => scope, "order_name" => name.as_str()),
            ));
        }
    }
    Ok(())
}

fn count_string_from_search(artifact: &super::GeoCompositionArtifact) -> String {
    if artifact.summary.residual_model_count_saturated {
        format!(">={}", artifact.summary.residual_model_count)
    } else if artifact.summary.residual_model_count_complete {
        artifact.summary.residual_model_count.to_string()
    } else {
        "unavailable".to_string()
    }
}

fn compare_answer_sets(
    search_models: &[GeoCompositionModel],
    search_materialized: bool,
    evaluator: &ObddEvaluator<'_>,
    max_answer_set_models: u64,
) -> Result<GeoResidualAnswerSetComparison, GeoResidualBenchmarkError> {
    if !search_materialized {
        return Ok(GeoResidualAnswerSetComparison::NotMaterialized {
            reason: "shipped solver did not materialize answer sets for this case".to_string(),
        });
    }
    let obdd_count = evaluator.model_count()?;
    if obdd_count > u128::from(max_answer_set_models) {
        return Ok(GeoResidualAnswerSetComparison::NotMaterialized {
            reason: "OBDD model count exceeds benchmark answer-set materialization limit"
                .to_string(),
        });
    }
    let obdd_models = evaluator.enumerate_models(max_answer_set_models)?;
    if obdd_models == search_models {
        Ok(GeoResidualAnswerSetComparison::Matches {
            model_count: u64::try_from(obdd_models.len())
                .map_err(|_| GeoResidualBenchmarkError::overflow("answer set length"))?,
        })
    } else {
        Ok(GeoResidualAnswerSetComparison::Differs {
            search_count: u64::try_from(search_models.len())
                .map_err(|_| GeoResidualBenchmarkError::overflow("search answer set length"))?,
            obdd_count: u64::try_from(obdd_models.len())
                .map_err(|_| GeoResidualBenchmarkError::overflow("obdd answer set length"))?,
        })
    }
}

fn build_obdd_root(
    builder: &mut ObddBuilder,
    request: &GeoCompositionRequest,
) -> Result<u32, GeoResidualBenchmarkError> {
    let mut clauses = Vec::new();
    clauses.push(
        builder.or_refs(
            request
                .universe
                .parcels
                .iter()
                .map(|id| GeoEntityRef::new(GeoEntityLevel::Parcel, id.clone())),
        )?,
    );
    for building in &request.universe.buildings {
        if building.parcel_ids.is_empty() {
            continue;
        }
        let building_var =
            builder.var(&GeoEntityRef::new(GeoEntityLevel::Building, &building.id))?;
        let parcel_domain = builder.or_refs(
            building
                .parcel_ids
                .iter()
                .map(|id| GeoEntityRef::new(GeoEntityLevel::Parcel, id.clone())),
        )?;
        clauses.push(builder.implies(building_var, parcel_domain)?);
    }
    for constraint in &request.hard_constraints {
        clauses.push(builder.constraint(&constraint.constraint, request)?);
    }
    builder.and_nodes(clauses)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BoolOp {
    And,
    Or,
}

struct ObddBuilder {
    variables: Vec<GeoEntityRef>,
    positions: BTreeMap<GeoEntityRef, usize>,
    nodes: Vec<GeoResidualObddNode>,
    peak_node_count: usize,
    unique: BTreeMap<(usize, u32, u32), u32>,
    apply_cache: BTreeMap<(BoolOp, u32, u32), u32>,
    not_cache: BTreeMap<u32, u32>,
}

#[derive(Debug, Clone, Copy)]
struct IntegerSumBounds {
    min: u128,
    max: u128,
}

impl ObddBuilder {
    fn new(variables: Vec<GeoEntityRef>) -> Result<Self, GeoResidualBenchmarkError> {
        validate_variable_order(&variables)?;
        let positions = variables
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, variable)| (variable, index))
            .collect();
        Ok(Self {
            variables,
            positions,
            nodes: vec![GeoResidualObddNode::False, GeoResidualObddNode::True],
            peak_node_count: 2,
            unique: BTreeMap::new(),
            apply_cache: BTreeMap::new(),
            not_cache: BTreeMap::new(),
        })
    }

    fn var(&mut self, variable: &GeoEntityRef) -> Result<u32, GeoResidualBenchmarkError> {
        let position = self.positions.get(variable).copied().ok_or_else(|| {
            GeoResidualBenchmarkError::invalid_input(
                "OBDD variable is not present in the declared order",
                detail!("level" => level_name(variable.level), "id" => variable.id.as_str()),
            )
        })?;
        self.mk(position, 0, 1)
    }

    fn mk(
        &mut self,
        variable_position: usize,
        low: u32,
        high: u32,
    ) -> Result<u32, GeoResidualBenchmarkError> {
        if low == high {
            return Ok(low);
        }
        let key = (variable_position, low, high);
        if let Some(id) = self.unique.get(&key) {
            return Ok(*id);
        }
        let id = u32::try_from(self.nodes.len())
            .map_err(|_| GeoResidualBenchmarkError::overflow("obdd node id"))?;
        self.nodes.push(GeoResidualObddNode::Decision {
            variable: self.variables[variable_position].clone(),
            low,
            high,
        });
        self.peak_node_count = self.peak_node_count.max(self.nodes.len());
        self.unique.insert(key, id);
        Ok(id)
    }

    fn not(&mut self, node: u32) -> Result<u32, GeoResidualBenchmarkError> {
        match node {
            0 => Ok(1),
            1 => Ok(0),
            _ => {
                if let Some(cached) = self.not_cache.get(&node) {
                    return Ok(*cached);
                }
                let (position, low, high) = self.decision(node)?;
                let low = self.not(low)?;
                let high = self.not(high)?;
                let result = self.mk(position, low, high)?;
                self.not_cache.insert(node, result);
                Ok(result)
            }
        }
    }

    fn apply(
        &mut self,
        op: BoolOp,
        left: u32,
        right: u32,
    ) -> Result<u32, GeoResidualBenchmarkError> {
        let key = if left <= right {
            (op, left, right)
        } else {
            (op, right, left)
        };
        if let Some(cached) = self.apply_cache.get(&key) {
            return Ok(*cached);
        }
        let result = match (terminal_bool(left), terminal_bool(right)) {
            (Some(left), Some(right)) => match op {
                BoolOp::And => terminal_node(left && right),
                BoolOp::Or => terminal_node(left || right),
            },
            _ => {
                let left_top = self.top_position(left)?;
                let right_top = self.top_position(right)?;
                let top = left_top.min(right_top);
                let (left_low, left_high) = self.branch_at(left, top)?;
                let (right_low, right_high) = self.branch_at(right, top)?;
                let low = self.apply(op, left_low, right_low)?;
                let high = self.apply(op, left_high, right_high)?;
                self.mk(top, low, high)?
            }
        };
        self.apply_cache.insert(key, result);
        Ok(result)
    }

    fn decision(&self, node: u32) -> Result<(usize, u32, u32), GeoResidualBenchmarkError> {
        match self.nodes.get(node as usize) {
            Some(GeoResidualObddNode::Decision {
                variable,
                low,
                high,
            }) => {
                let position = self.positions.get(variable).copied().ok_or_else(|| {
                    GeoResidualBenchmarkError::invalid_input(
                        "OBDD node references a variable outside the order",
                        detail!("id" => variable.id.as_str()),
                    )
                })?;
                Ok((position, *low, *high))
            }
            Some(GeoResidualObddNode::False | GeoResidualObddNode::True) => {
                Err(GeoResidualBenchmarkError::invalid_input(
                    "Terminal OBDD node has no decision",
                    detail!("node" => node),
                ))
            }
            None => Err(GeoResidualBenchmarkError::invalid_input(
                "OBDD node id is out of range",
                detail!("node" => node),
            )),
        }
    }

    fn top_position(&self, node: u32) -> Result<usize, GeoResidualBenchmarkError> {
        if terminal_bool(node).is_some() {
            return Ok(usize::MAX);
        }
        self.decision(node).map(|(position, _, _)| position)
    }

    fn branch_at(
        &self,
        node: u32,
        position: usize,
    ) -> Result<(u32, u32), GeoResidualBenchmarkError> {
        if self.top_position(node)? == position {
            let (_, low, high) = self.decision(node)?;
            Ok((low, high))
        } else {
            Ok((node, node))
        }
    }

    fn and_nodes(&mut self, nodes: Vec<u32>) -> Result<u32, GeoResidualBenchmarkError> {
        nodes
            .into_iter()
            .try_fold(1, |acc, node| self.apply(BoolOp::And, acc, node))
    }

    fn or_nodes(&mut self, nodes: Vec<u32>) -> Result<u32, GeoResidualBenchmarkError> {
        nodes
            .into_iter()
            .try_fold(0, |acc, node| self.apply(BoolOp::Or, acc, node))
    }

    fn implies(&mut self, left: u32, right: u32) -> Result<u32, GeoResidualBenchmarkError> {
        let not_left = self.not(left)?;
        self.apply(BoolOp::Or, not_left, right)
    }

    fn or_refs(
        &mut self,
        refs: impl IntoIterator<Item = GeoEntityRef>,
    ) -> Result<u32, GeoResidualBenchmarkError> {
        let vars = refs
            .into_iter()
            .map(|member| self.var(&member))
            .collect::<Result<Vec<_>, _>>()?;
        self.or_nodes(vars)
    }

    fn and_literals(
        &mut self,
        literals: Vec<(GeoEntityRef, bool)>,
    ) -> Result<u32, GeoResidualBenchmarkError> {
        let nodes = literals
            .into_iter()
            .map(|(member, positive)| {
                let node = self.var(&member)?;
                if positive { Ok(node) } else { self.not(node) }
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.and_nodes(nodes)
    }

    fn constraint(
        &mut self,
        constraint: &GeoHardConstraintKind,
        request: &GeoCompositionRequest,
    ) -> Result<u32, GeoResidualBenchmarkError> {
        match constraint {
            GeoHardConstraintKind::Require { member } => self.var(member),
            GeoHardConstraintKind::Forbid { member } => {
                let node = self.var(member)?;
                self.not(node)
            }
            GeoHardConstraintKind::Cardinality { level, min, max } => {
                let variables = level_variables(request, *level)?
                    .into_iter()
                    .map(|member| self.position_of(&member))
                    .collect::<Result<Vec<_>, _>>()?;
                self.cardinality(&variables, *min, *max)
            }
            GeoHardConstraintKind::AllowedSets { level, sets } => {
                let universe = level_variables(request, *level)?;
                let alternatives = sets
                    .iter()
                    .map(|allowed| {
                        let allowed = allowed.iter().cloned().collect::<BTreeSet<_>>();
                        let literals = universe
                            .iter()
                            .map(|member| (member.clone(), allowed.contains(&member.id)))
                            .collect::<Vec<_>>();
                        self.and_literals(literals)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.or_nodes(alternatives)
            }
            GeoHardConstraintKind::AnyOf { members } => self.or_refs(members.iter().cloned()),
            GeoHardConstraintKind::IntegerSumBand {
                level,
                values,
                min,
                max,
                ..
            } => self.integer_sum_band(*level, values, *min, *max),
            GeoHardConstraintKind::AllOrNone { members } => {
                let all_false = self.and_literals(
                    members
                        .iter()
                        .cloned()
                        .map(|member| (member, false))
                        .collect(),
                )?;
                let all_true = self.and_literals(
                    members
                        .iter()
                        .cloned()
                        .map(|member| (member, true))
                        .collect(),
                )?;
                self.apply(BoolOp::Or, all_false, all_true)
            }
            GeoHardConstraintKind::Requires {
                if_member,
                then_member,
            } => {
                let if_node = self.var(if_member)?;
                let then_node = self.var(then_member)?;
                self.implies(if_node, then_node)
            }
        }
    }

    fn position_of(&self, member: &GeoEntityRef) -> Result<usize, GeoResidualBenchmarkError> {
        self.positions.get(member).copied().ok_or_else(|| {
            GeoResidualBenchmarkError::invalid_input(
                "OBDD variable is not present in the declared order",
                detail!("level" => level_name(member.level), "id" => member.id.as_str()),
            )
        })
    }

    fn cardinality(
        &mut self,
        variables: &[usize],
        min: usize,
        max: usize,
    ) -> Result<u32, GeoResidualBenchmarkError> {
        let mut vars = variables.to_vec();
        vars.sort_unstable();
        let mut memo = BTreeMap::new();
        self.cardinality_inner(&vars, 0, 0, min, max, &mut memo)
    }

    fn cardinality_inner(
        &mut self,
        variables: &[usize],
        index: usize,
        selected: usize,
        min: usize,
        max: usize,
        memo: &mut BTreeMap<(usize, usize), u32>,
    ) -> Result<u32, GeoResidualBenchmarkError> {
        if selected > max {
            return Ok(0);
        }
        if selected + (variables.len() - index) < min {
            return Ok(0);
        }
        if index == variables.len() {
            return Ok(terminal_node((min..=max).contains(&selected)));
        }
        if let Some(cached) = memo.get(&(index, selected)) {
            return Ok(*cached);
        }
        let low = self.cardinality_inner(variables, index + 1, selected, min, max, memo)?;
        let high = self.cardinality_inner(variables, index + 1, selected + 1, min, max, memo)?;
        let node = self.mk(variables[index], low, high)?;
        memo.insert((index, selected), node);
        Ok(node)
    }

    fn integer_sum_band(
        &mut self,
        level: GeoEntityLevel,
        values: &[GeoIntegerMemberValue],
        min: u64,
        max: u64,
    ) -> Result<u32, GeoResidualBenchmarkError> {
        let mut weighted = values
            .iter()
            .map(|value| {
                Ok((
                    self.position_of(&GeoEntityRef::new(level, value.id.clone()))?,
                    u128::from(value.value),
                ))
            })
            .collect::<Result<Vec<_>, GeoResidualBenchmarkError>>()?;
        weighted.sort_unstable();
        let remaining = suffix_sums(&weighted)?;
        let mut memo = BTreeMap::new();
        self.integer_sum_inner(
            &weighted,
            &remaining,
            0,
            0,
            IntegerSumBounds {
                min: u128::from(min),
                max: u128::from(max),
            },
            &mut memo,
        )
    }

    fn integer_sum_inner(
        &mut self,
        variables: &[(usize, u128)],
        remaining: &[u128],
        index: usize,
        sum: u128,
        bounds: IntegerSumBounds,
        memo: &mut BTreeMap<(usize, u128), u32>,
    ) -> Result<u32, GeoResidualBenchmarkError> {
        if sum > bounds.max {
            return Ok(0);
        }
        let remaining_sum = sum
            .checked_add(*remaining.get(index).ok_or_else(|| {
                GeoResidualBenchmarkError::invalid_input(
                    "Integer-sum suffix table is missing an index",
                    detail!("index" => index),
                )
            })?)
            .ok_or_else(|| GeoResidualBenchmarkError::overflow("integer-sum remaining"))?;
        if remaining_sum < bounds.min {
            return Ok(0);
        }
        if index == variables.len() {
            return Ok(terminal_node((bounds.min..=bounds.max).contains(&sum)));
        }
        let max_plus_one = bounds
            .max
            .checked_add(1)
            .ok_or_else(|| GeoResidualBenchmarkError::overflow("integer-sum cap"))?;
        let capped = sum.min(max_plus_one);
        if let Some(cached) = memo.get(&(index, capped)) {
            return Ok(*cached);
        }
        let (position, value) = variables[index];
        let low = self.integer_sum_inner(variables, remaining, index + 1, sum, bounds, memo)?;
        let high_sum = sum
            .checked_add(value)
            .ok_or_else(|| GeoResidualBenchmarkError::overflow("integer-sum selected value"))?
            .min(max_plus_one);
        let high =
            self.integer_sum_inner(variables, remaining, index + 1, high_sum, bounds, memo)?;
        let node = self.mk(position, low, high)?;
        memo.insert((index, capped), node);
        Ok(node)
    }
}

struct PrunedObdd {
    root: u32,
    nodes: Vec<GeoResidualObddNode>,
    root_reachable_node_count: usize,
    root_reachable_nonterminal_node_count: usize,
}

fn prune_obdd(
    root: u32,
    nodes: &[GeoResidualObddNode],
) -> Result<PrunedObdd, GeoResidualBenchmarkError> {
    if nodes.len() < 2
        || nodes[0] != GeoResidualObddNode::False
        || nodes[1] != GeoResidualObddNode::True
    {
        return Err(GeoResidualBenchmarkError::invalid_input(
            "OBDD construction arena terminals are malformed",
            detail!("field" => "nodes"),
        ));
    }

    let reachable = reachable_obdd_nodes(root, nodes)?;
    let root_reachable_node_count = reachable.len();
    let root_reachable_nonterminal_node_count = reachable.iter().filter(|node| **node > 1).count();

    let mut remap = BTreeMap::new();
    remap.insert(0_u32, 0_u32);
    remap.insert(1_u32, 1_u32);
    let mut pruned_nodes = vec![GeoResidualObddNode::False, GeoResidualObddNode::True];
    for old_id in reachable.iter().copied().filter(|old_id| *old_id > 1) {
        let new_id = u32::try_from(pruned_nodes.len())
            .map_err(|_| GeoResidualBenchmarkError::overflow("pruned obdd node id"))?;
        remap.insert(old_id, new_id);
        pruned_nodes.push(GeoResidualObddNode::False);
    }

    for old_id in reachable.iter().copied().filter(|old_id| *old_id > 1) {
        let new_id = *remap.get(&old_id).ok_or_else(|| {
            GeoResidualBenchmarkError::invalid_input(
                "OBDD reachable node remap is incomplete",
                detail!("node" => old_id),
            )
        })?;
        let GeoResidualObddNode::Decision {
            variable,
            low,
            high,
        } = &nodes[old_id as usize]
        else {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "Reachable nonterminal OBDD node is malformed",
                detail!("node" => old_id),
            ));
        };
        let low = *remap.get(low).ok_or_else(|| {
            GeoResidualBenchmarkError::invalid_input(
                "OBDD low child remap is incomplete",
                detail!("node" => old_id),
            )
        })?;
        let high = *remap.get(high).ok_or_else(|| {
            GeoResidualBenchmarkError::invalid_input(
                "OBDD high child remap is incomplete",
                detail!("node" => old_id),
            )
        })?;
        pruned_nodes[new_id as usize] = GeoResidualObddNode::Decision {
            variable: variable.clone(),
            low,
            high,
        };
    }

    let root = *remap.get(&root).ok_or_else(|| {
        GeoResidualBenchmarkError::invalid_input(
            "OBDD root remap is incomplete",
            detail!("root" => root),
        )
    })?;
    Ok(PrunedObdd {
        root,
        nodes: pruned_nodes,
        root_reachable_node_count,
        root_reachable_nonterminal_node_count,
    })
}

fn reachable_obdd_nodes(
    root: u32,
    nodes: &[GeoResidualObddNode],
) -> Result<BTreeSet<u32>, GeoResidualBenchmarkError> {
    let mut reachable = BTreeSet::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if !reachable.insert(node) {
            continue;
        }
        match nodes.get(node as usize) {
            Some(GeoResidualObddNode::False | GeoResidualObddNode::True) => {}
            Some(GeoResidualObddNode::Decision { low, high, .. }) => {
                stack.push(*low);
                stack.push(*high);
            }
            None => {
                return Err(GeoResidualBenchmarkError::invalid_input(
                    "OBDD node id is out of range",
                    detail!("node" => node),
                ));
            }
        }
    }
    Ok(reachable)
}

struct ObddEvaluator<'a> {
    artifact: &'a GeoResidualObddArtifact,
    positions: BTreeMap<GeoEntityRef, usize>,
}

impl<'a> ObddEvaluator<'a> {
    fn new(artifact: &'a GeoResidualObddArtifact) -> Result<Self, GeoResidualBenchmarkError> {
        if artifact.version != CANON_GEO_RESIDUAL_OBDD_VERSION {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "Unsupported OBDD artifact version",
                detail!(
                    "actual" => artifact.version.as_str(),
                    "expected" => CANON_GEO_RESIDUAL_OBDD_VERSION,
                ),
            ));
        }
        validate_variable_order(&artifact.variables)?;
        if artifact.nodes.len() < 2
            || artifact.nodes[0] != GeoResidualObddNode::False
            || artifact.nodes[1] != GeoResidualObddNode::True
        {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "OBDD artifact terminals are malformed",
                detail!("field" => "nodes"),
            ));
        }
        let positions = artifact
            .variables
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, variable)| (variable, index))
            .collect::<BTreeMap<_, _>>();
        let evaluator = Self {
            artifact,
            positions,
        };
        let reachable = evaluator.validate_graph()?;
        evaluator.validate_public_counters(&reachable)?;
        let build_bytes = obdd_build_bytes(&GeoResidualObddBuildView {
            version: CANON_GEO_RESIDUAL_OBDD_VERSION,
            request_blake3: artifact.request_blake3.as_str(),
            order_name: artifact.order_name.as_str(),
            variables: artifact.variables.as_slice(),
            root: artifact.root,
            nodes: artifact.nodes.as_slice(),
        })?;
        let expected_len = u64::try_from(build_bytes.len())
            .map_err(|_| GeoResidualBenchmarkError::overflow("obdd build byte length"))?;
        if artifact.deterministic_build_bytes != expected_len
            || artifact.build_blake3 != hash_bytes(&build_bytes)
        {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "OBDD artifact build digest or byte count does not match its nodes",
                detail!("field" => "build_blake3"),
            ));
        }
        Ok(evaluator)
    }

    fn validate_graph(&self) -> Result<BTreeSet<u32>, GeoResidualBenchmarkError> {
        let reachable = reachable_obdd_nodes(self.artifact.root, &self.artifact.nodes)?;
        let mut unique = BTreeSet::new();
        for (node_id, node) in self.artifact.nodes.iter().enumerate().skip(2) {
            let node_u32 = u32::try_from(node_id)
                .map_err(|_| GeoResidualBenchmarkError::overflow("obdd node id"))?;
            if !reachable.contains(&node_u32) {
                return Err(GeoResidualBenchmarkError::invalid_input(
                    "Pruned OBDD artifact contains an unreachable nonterminal node",
                    detail!("node" => node_id),
                ));
            }
            let GeoResidualObddNode::Decision {
                variable,
                low,
                high,
            } = node
            else {
                return Err(GeoResidualBenchmarkError::invalid_input(
                    "Nonterminal OBDD node slot is malformed",
                    detail!("node" => node_id),
                ));
            };
            let position = self.positions.get(variable).copied().ok_or_else(|| {
                GeoResidualBenchmarkError::invalid_input(
                    "OBDD node references a variable outside the order",
                    detail!("id" => variable.id.as_str()),
                )
            })?;
            if low == high {
                return Err(GeoResidualBenchmarkError::invalid_input(
                    "Reduced OBDD node has identical children",
                    detail!("node" => node_id),
                ));
            }
            for child in [*low, *high] {
                match self.artifact.nodes.get(child as usize) {
                    Some(GeoResidualObddNode::False | GeoResidualObddNode::True) => {}
                    Some(GeoResidualObddNode::Decision {
                        variable: child_variable,
                        ..
                    }) => {
                        let child_position =
                            self.positions.get(child_variable).copied().ok_or_else(|| {
                                GeoResidualBenchmarkError::invalid_input(
                                    "OBDD child references a variable outside the order",
                                    detail!("id" => child_variable.id.as_str()),
                                )
                            })?;
                        if child_position <= position {
                            return Err(GeoResidualBenchmarkError::invalid_input(
                                "OBDD node violates variable order",
                                detail!("node" => node_id, "child" => child),
                            ));
                        }
                    }
                    None => {
                        return Err(GeoResidualBenchmarkError::invalid_input(
                            "OBDD child node id is out of range",
                            detail!("node" => node_id, "child" => child),
                        ));
                    }
                }
            }
            if !unique.insert((position, *low, *high)) {
                return Err(GeoResidualBenchmarkError::invalid_input(
                    "Reduced OBDD artifact contains duplicate decision nodes",
                    detail!("node" => node_id),
                ));
            }
        }
        Ok(reachable)
    }

    fn validate_public_counters(
        &self,
        reachable: &BTreeSet<u32>,
    ) -> Result<(), GeoResidualBenchmarkError> {
        let artifact = self.artifact;
        if artifact.construction_arena_node_count < 2 {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "OBDD construction arena node count must include fixed terminals",
                detail!("construction_arena_node_count" => artifact.construction_arena_node_count),
            ));
        }
        let expected_arena_nonterminals = artifact
            .construction_arena_node_count
            .checked_sub(2)
            .ok_or_else(|| GeoResidualBenchmarkError::overflow("obdd arena node count"))?;
        if artifact.construction_arena_nonterminal_node_count != expected_arena_nonterminals {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "OBDD construction arena nonterminal count is inconsistent",
                detail!(
                    "expected" => expected_arena_nonterminals,
                    "actual" => artifact.construction_arena_nonterminal_node_count,
                ),
            ));
        }
        if artifact.construction_arena_node_count < artifact.nodes.len() {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "OBDD construction arena cannot be smaller than final serialized artifact",
                detail!(
                    "construction_arena_node_count" => artifact.construction_arena_node_count,
                    "final_serialized_node_count" => artifact.nodes.len(),
                ),
            ));
        }
        if artifact.construction_peak_node_count < artifact.construction_arena_node_count {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "OBDD construction peak cannot be smaller than construction arena",
                detail!(
                    "construction_peak_node_count" => artifact.construction_peak_node_count,
                    "construction_arena_node_count" => artifact.construction_arena_node_count,
                ),
            ));
        }

        let root_reachable_node_count = reachable.len();
        let root_reachable_nonterminal_node_count =
            reachable.iter().filter(|node| **node > 1).count();
        if artifact.root_reachable_node_count != root_reachable_node_count {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "OBDD root-reachable node count is inconsistent",
                detail!(
                    "expected" => root_reachable_node_count,
                    "actual" => artifact.root_reachable_node_count,
                ),
            ));
        }
        if artifact.root_reachable_nonterminal_node_count != root_reachable_nonterminal_node_count {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "OBDD root-reachable nonterminal count is inconsistent",
                detail!(
                    "expected" => root_reachable_nonterminal_node_count,
                    "actual" => artifact.root_reachable_nonterminal_node_count,
                ),
            ));
        }
        Ok(())
    }

    fn model_count(&self) -> Result<u128, GeoResidualBenchmarkError> {
        let mut memo = BTreeMap::new();
        self.count_from(self.artifact.root, 0, &mut memo)
    }

    fn count_from(
        &self,
        node: u32,
        level: usize,
        memo: &mut BTreeMap<(u32, usize), u128>,
    ) -> Result<u128, GeoResidualBenchmarkError> {
        if let Some(cached) = memo.get(&(node, level)) {
            return Ok(*cached);
        }
        let count = match self.artifact.nodes.get(node as usize) {
            Some(GeoResidualObddNode::False) => 0,
            Some(GeoResidualObddNode::True) => pow2(self.artifact.variables.len() - level)?,
            Some(GeoResidualObddNode::Decision {
                variable,
                low,
                high,
            }) => {
                let position = self.positions[variable];
                let skipped = pow2(position - level)?;
                let low_count = self.count_from(*low, position + 1, memo)?;
                let high_count = self.count_from(*high, position + 1, memo)?;
                let branch_count = low_count
                    .checked_add(high_count)
                    .ok_or_else(|| GeoResidualBenchmarkError::overflow("obdd model count"))?;
                skipped
                    .checked_mul(branch_count)
                    .ok_or_else(|| GeoResidualBenchmarkError::overflow("obdd model count"))?
            }
            None => {
                return Err(GeoResidualBenchmarkError::invalid_input(
                    "OBDD node id is out of range",
                    detail!("node" => node),
                ));
            }
        };
        memo.insert((node, level), count);
        Ok(count)
    }

    fn count_with_assignment(
        &self,
        forced_position: usize,
        forced_value: bool,
    ) -> Result<u128, GeoResidualBenchmarkError> {
        let mut memo = BTreeMap::new();
        self.count_with_assignment_from(
            self.artifact.root,
            0,
            forced_position,
            forced_value,
            &mut memo,
        )
    }

    fn count_with_assignment_from(
        &self,
        node: u32,
        level: usize,
        forced_position: usize,
        forced_value: bool,
        memo: &mut BTreeMap<(u32, usize, usize, bool), u128>,
    ) -> Result<u128, GeoResidualBenchmarkError> {
        let key = (node, level, forced_position, forced_value);
        if let Some(cached) = memo.get(&key) {
            return Ok(*cached);
        }
        let count = match self.artifact.nodes.get(node as usize) {
            Some(GeoResidualObddNode::False) => 0,
            Some(GeoResidualObddNode::True) => {
                let remaining = self.artifact.variables.len() - level;
                let free = remaining - usize::from(forced_position >= level);
                pow2(free)?
            }
            Some(GeoResidualObddNode::Decision {
                variable,
                low,
                high,
            }) => {
                let position = self.positions[variable];
                let skipped_free = (level..position)
                    .filter(|slot| *slot != forced_position)
                    .count();
                let skipped = pow2(skipped_free)?;
                let branch_count = if position == forced_position {
                    let branch = if forced_value { *high } else { *low };
                    self.count_with_assignment_from(
                        branch,
                        position + 1,
                        forced_position,
                        forced_value,
                        memo,
                    )?
                } else {
                    let low_count = self.count_with_assignment_from(
                        *low,
                        position + 1,
                        forced_position,
                        forced_value,
                        memo,
                    )?;
                    let high_count = self.count_with_assignment_from(
                        *high,
                        position + 1,
                        forced_position,
                        forced_value,
                        memo,
                    )?;
                    low_count.checked_add(high_count).ok_or_else(|| {
                        GeoResidualBenchmarkError::overflow("obdd restricted count")
                    })?
                };
                skipped
                    .checked_mul(branch_count)
                    .ok_or_else(|| GeoResidualBenchmarkError::overflow("obdd restricted count"))?
            }
            None => {
                return Err(GeoResidualBenchmarkError::invalid_input(
                    "OBDD node id is out of range",
                    detail!("node" => node),
                ));
            }
        };
        memo.insert(key, count);
        Ok(count)
    }

    fn backbone(&self) -> Result<GeoCompositionBackbone, GeoResidualBenchmarkError> {
        let total = self.model_count()?;
        if total == 0 {
            return Ok(GeoCompositionBackbone {
                parcels: Vec::new(),
                buildings: Vec::new(),
            });
        }
        let mut parcels = Vec::new();
        let mut buildings = Vec::new();
        for (position, variable) in self.artifact.variables.iter().enumerate() {
            if self.count_with_assignment(position, false)? == 0 {
                match variable.level {
                    GeoEntityLevel::Parcel => parcels.push(variable.id.clone()),
                    GeoEntityLevel::Building => buildings.push(variable.id.clone()),
                    GeoEntityLevel::PoiUnit | GeoEntityLevel::Property => {}
                }
            }
        }
        parcels.sort();
        buildings.sort();
        Ok(GeoCompositionBackbone { parcels, buildings })
    }

    fn model_membership(&self, model: &GeoCompositionModel) -> bool {
        if model.parcels.is_empty()
            || !is_sorted_distinct_strings(&model.parcels)
            || !is_sorted_distinct_strings(&model.buildings)
        {
            return false;
        }
        let declared_parcels = self
            .artifact
            .variables
            .iter()
            .filter_map(|variable| match variable.level {
                GeoEntityLevel::Parcel => Some(variable.id.as_str()),
                GeoEntityLevel::Building | GeoEntityLevel::PoiUnit | GeoEntityLevel::Property => {
                    None
                }
            })
            .collect::<BTreeSet<_>>();
        let declared_buildings = self
            .artifact
            .variables
            .iter()
            .filter_map(|variable| match variable.level {
                GeoEntityLevel::Building => Some(variable.id.as_str()),
                GeoEntityLevel::Parcel | GeoEntityLevel::PoiUnit | GeoEntityLevel::Property => None,
            })
            .collect::<BTreeSet<_>>();
        if model
            .parcels
            .iter()
            .any(|id| !declared_parcels.contains(id.as_str()))
            || model
                .buildings
                .iter()
                .any(|id| !declared_buildings.contains(id.as_str()))
        {
            return false;
        }

        let parcel_set = model.parcels.iter().collect::<BTreeSet<_>>();
        let building_set = model.buildings.iter().collect::<BTreeSet<_>>();
        let mut node = self.artifact.root;
        loop {
            match self.artifact.nodes.get(node as usize) {
                Some(GeoResidualObddNode::False) => return false,
                Some(GeoResidualObddNode::True) => return true,
                Some(GeoResidualObddNode::Decision {
                    variable,
                    low,
                    high,
                }) => {
                    let selected = match variable.level {
                        GeoEntityLevel::Parcel => parcel_set.contains(&variable.id),
                        GeoEntityLevel::Building => building_set.contains(&variable.id),
                        GeoEntityLevel::PoiUnit | GeoEntityLevel::Property => false,
                    };
                    node = if selected { *high } else { *low };
                }
                None => return false,
            }
        }
    }

    fn enumerate_models(
        &self,
        max_answer_set_models: u64,
    ) -> Result<Vec<GeoCompositionModel>, GeoResidualBenchmarkError> {
        if self.artifact.variables.len() >= 128 {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "OBDD answer-set materialization is limited to fewer than 128 variables",
                detail!("variables" => self.artifact.variables.len()),
            ));
        }
        let space = 1_u128 << self.artifact.variables.len();
        let mut models = Vec::new();
        for mask in 0..space {
            let model = self.model_from_mask(mask);
            if self.model_membership(&model) {
                if u64::try_from(models.len())
                    .map(|len| len >= max_answer_set_models)
                    .unwrap_or(true)
                {
                    return Err(GeoResidualBenchmarkError::overflow(
                        "obdd answer-set materialization limit",
                    ));
                }
                models.push(model);
            }
        }
        models.sort();
        Ok(models)
    }

    fn model_from_mask(&self, mask: u128) -> GeoCompositionModel {
        let mut parcels = Vec::new();
        let mut buildings = Vec::new();
        for (position, variable) in self.artifact.variables.iter().enumerate() {
            if mask & (1_u128 << position) == 0 {
                continue;
            }
            match variable.level {
                GeoEntityLevel::Parcel => parcels.push(variable.id.clone()),
                GeoEntityLevel::Building => buildings.push(variable.id.clone()),
                GeoEntityLevel::PoiUnit | GeoEntityLevel::Property => {}
            }
        }
        parcels.sort();
        buildings.sort();
        GeoCompositionModel { parcels, buildings }
    }
}

#[derive(Serialize)]
struct GeoResidualObddBuildView<'a> {
    version: &'static str,
    request_blake3: &'a str,
    order_name: &'a str,
    variables: &'a [GeoEntityRef],
    root: u32,
    nodes: &'a [GeoResidualObddNode],
}

fn obdd_build_bytes(
    view: &GeoResidualObddBuildView<'_>,
) -> Result<Vec<u8>, GeoResidualBenchmarkError> {
    serde_json::to_vec(view).map_err(|error| {
        GeoResidualBenchmarkError::invalid_input(
            "OBDD build view could not serialize",
            detail!("serde" => error),
        )
    })
}

fn composition_request_digest(
    request: &GeoCompositionRequest,
) -> Result<String, GeoResidualBenchmarkError> {
    serde_json::to_vec(request)
        .map(|bytes| hash_bytes(&bytes))
        .map_err(|error| {
            GeoResidualBenchmarkError::invalid_input(
                "Geo composition request could not serialize",
                detail!("serde" => error),
            )
        })
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn terminal_bool(node: u32) -> Option<bool> {
    match node {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn terminal_node(value: bool) -> u32 {
    u32::from(value)
}

fn pow2(exponent: usize) -> Result<u128, GeoResidualBenchmarkError> {
    if exponent >= 128 {
        return Err(GeoResidualBenchmarkError::overflow("2^n count"));
    }
    Ok(1_u128 << exponent)
}

fn suffix_sums(values: &[(usize, u128)]) -> Result<Vec<u128>, GeoResidualBenchmarkError> {
    let mut suffix = vec![0_u128; values.len() + 1];
    for index in (0..values.len()).rev() {
        suffix[index] = suffix[index + 1]
            .checked_add(values[index].1)
            .ok_or_else(|| GeoResidualBenchmarkError::overflow("integer-sum suffix"))?;
    }
    Ok(suffix)
}

fn canonical_variables(request: &GeoCompositionRequest) -> Vec<GeoEntityRef> {
    request
        .universe
        .parcels
        .iter()
        .map(|id| GeoEntityRef::new(GeoEntityLevel::Parcel, id.clone()))
        .chain(
            request
                .universe
                .buildings
                .iter()
                .map(|building| GeoEntityRef::new(GeoEntityLevel::Building, building.id.clone())),
        )
        .collect()
}

fn materialize_order(
    request: &GeoCompositionRequest,
    order: &GeoResidualVariableOrder,
) -> Result<Vec<GeoEntityRef>, GeoResidualBenchmarkError> {
    let canonical = canonical_variables(request);
    let variables = match order {
        GeoResidualVariableOrder::Canonical => canonical.clone(),
        GeoResidualVariableOrder::ReverseCanonical => canonical.iter().cloned().rev().collect(),
        GeoResidualVariableOrder::BuildingsFirst => request
            .universe
            .buildings
            .iter()
            .map(|building| GeoEntityRef::new(GeoEntityLevel::Building, building.id.clone()))
            .chain(
                request
                    .universe
                    .parcels
                    .iter()
                    .map(|id| GeoEntityRef::new(GeoEntityLevel::Parcel, id.clone())),
            )
            .collect(),
        GeoResidualVariableOrder::IncidenceInterleaved => incidence_order(request),
        GeoResidualVariableOrder::Explicit { variables, .. } => variables.clone(),
    };
    let expected = canonical.iter().cloned().collect::<BTreeSet<_>>();
    let actual = variables.iter().cloned().collect::<BTreeSet<_>>();
    if expected != actual || expected.len() != variables.len() {
        return Err(GeoResidualBenchmarkError::invalid_input(
            "OBDD order must name each composition variable exactly once",
            detail!(
                "expected" => expected.len(),
                "actual_unique" => actual.len(),
                "actual" => variables.len(),
            ),
        ));
    }
    validate_variable_order(&variables)?;
    Ok(variables)
}

fn incidence_order(request: &GeoCompositionRequest) -> Vec<GeoEntityRef> {
    let mut emitted_buildings = BTreeSet::new();
    let mut variables = Vec::new();
    for parcel_id in &request.universe.parcels {
        variables.push(GeoEntityRef::new(GeoEntityLevel::Parcel, parcel_id.clone()));
        for building in &request.universe.buildings {
            if building.parcel_ids.binary_search(parcel_id).is_ok()
                && emitted_buildings.insert(building.id.clone())
            {
                variables.push(GeoEntityRef::new(
                    GeoEntityLevel::Building,
                    building.id.clone(),
                ));
            }
        }
    }
    for building in &request.universe.buildings {
        if emitted_buildings.insert(building.id.clone()) {
            variables.push(GeoEntityRef::new(
                GeoEntityLevel::Building,
                building.id.clone(),
            ));
        }
    }
    variables
}

fn validate_variable_order(variables: &[GeoEntityRef]) -> Result<(), GeoResidualBenchmarkError> {
    let mut seen = BTreeSet::new();
    for variable in variables {
        match variable.level {
            GeoEntityLevel::Parcel | GeoEntityLevel::Building => {}
            GeoEntityLevel::PoiUnit | GeoEntityLevel::Property => {
                return Err(GeoResidualBenchmarkError::invalid_input(
                    "OBDD variables support only parcel and building levels",
                    detail!("level" => level_name(variable.level)),
                ));
            }
        }
        if variable.id.is_empty() || variable.id.trim() != variable.id {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "OBDD variable id must be non-empty and canonical",
                detail!("id" => variable.id.as_str()),
            ));
        }
        if !seen.insert(variable.clone()) {
            return Err(GeoResidualBenchmarkError::invalid_input(
                "OBDD order contains a duplicate variable",
                detail!("level" => level_name(variable.level), "id" => variable.id.as_str()),
            ));
        }
    }
    Ok(())
}

fn is_sorted_distinct_strings(values: &[String]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn order_name(order: &GeoResidualVariableOrder) -> String {
    match order {
        GeoResidualVariableOrder::Canonical => "canonical".to_string(),
        GeoResidualVariableOrder::ReverseCanonical => "reverse_canonical".to_string(),
        GeoResidualVariableOrder::BuildingsFirst => "buildings_first".to_string(),
        GeoResidualVariableOrder::IncidenceInterleaved => "incidence_interleaved".to_string(),
        GeoResidualVariableOrder::Explicit { name, .. } => format!("explicit:{name}"),
    }
}

fn level_variables(
    request: &GeoCompositionRequest,
    level: GeoEntityLevel,
) -> Result<Vec<GeoEntityRef>, GeoResidualBenchmarkError> {
    match level {
        GeoEntityLevel::Parcel => Ok(request
            .universe
            .parcels
            .iter()
            .map(|id| GeoEntityRef::new(GeoEntityLevel::Parcel, id.clone()))
            .collect()),
        GeoEntityLevel::Building => Ok(request
            .universe
            .buildings
            .iter()
            .map(|building| GeoEntityRef::new(GeoEntityLevel::Building, building.id.clone()))
            .collect()),
        GeoEntityLevel::PoiUnit | GeoEntityLevel::Property => {
            Err(GeoResidualBenchmarkError::invalid_input(
                "OBDD benchmark supports only parcel and building levels",
                detail!("level" => level_name(level)),
            ))
        }
    }
}

const fn level_name(level: GeoEntityLevel) -> &'static str {
    match level {
        GeoEntityLevel::PoiUnit => "poi_unit",
        GeoEntityLevel::Building => "building",
        GeoEntityLevel::Parcel => "parcel",
        GeoEntityLevel::Property => "property",
    }
}
